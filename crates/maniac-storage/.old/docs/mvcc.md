# MVCC Page Versioning and WAL Retention

## Goals
- Preserve snapshot isolation semantics (libsql-compatible) without blocking checkpoints or truncation.
- Track every mutable page version that resides in non-checkpointed WAL segments so the system can reconcile the correct view for each reader.
- Allow checkpointing and garbage collection to reclaim WAL segments as soon as no active reader depends on the pages they contain.
- Keep the design crash-safe, idempotent, and consistent with existing manifest, WAL, and quota subsystems.

## Non-Goals
- Implement per-row MVCC or new SQL-visible transaction semantics. The scope is WAL page version retention only.
- Rewrite on-disk checkpoint formats; existing chunk and delta layouts remain unchanged.
- Introduce distributed read tracking. Readers run on a single node per tenant in this iteration.

## Background
Today writers append frames to WAL segments without persisting which logical pages live in those segments. Snapshot readers therefore rely on the checkpoint manifest alone; they implicitly assume the latest checkpoint contains every page they might touch. Once libsql-style snapshot isolation is enabled, a reader may need an older page version even after a newer checkpoint is available. Without a page-version index, the system cannot tell when it is safe to truncate the WAL or drop staged pages, so retention falls back to coarse, time-based heuristics.

The checkpoint pipeline already stores durable metadata in LMDB (chunk_catalog, chunk_delta_index, wal_catalog, change_log). Extending LMDB with a page-index allows us to reason about reader-visible versions and orchestrate WAL checkpointing precisely.

## High-Level Design
We introduce three cooperating responsibilities:

1. **Page Version Index** – LMDB tables track every modified page that still resides in the WAL. Entries include the page identifier, producing WAL segment, frame offset, and lineage to the previous version.
2. **Snapshot Floor Tracking** – Readers register their snapshot visibility window in LMDB so the storage layer can derive the lowest safely visible sequence number per tenant.
3. **Retention Orchestrator** – The checkpoint planner combines the page index and snapshot floor to decide which WAL segments can be checkpointed or truncated. When a segment no longer contributes pages to any active snapshot, it becomes eligible for checkpointing and eventual deletion.

`
        +-----------+   frames   +----------------+    apply delta   +---------------------+
        | Writer    |----------->| WAL Segment N  |----------------->| Page Version Index  |
        +-----------+            +----------------+                  +---------+-----------+
             |                                                         reader floor |
             | register latest head                                     |             v
             v                                                         +---------------+
        +-----------+   lease/checkpoint jobs   +------------------->  | Retention GC  |
        | Planner   |------------------------------------------------->| & Checkpoint  |
        +-----------+                                                 +---------------+
`

## Data Model

### Version Epochs
Every page version is tagged with a **version epoch** describing its ordering. For WAL pages the epoch is (segment_id, frame_lsn); for chunks it is (checkpoint_generation, chunk_delta_id). Epochs are comparable because rame_lsn increases monotonically within a segment and segment_id inherits the global WAL sequence. Checkpoints record the highest epoch they covered in the manifest.

### LMDB Tables (New)
- page_version_index
  - **Key**: (tenant_id, page_id, version_epoch) stored with dupsort ordering by descending epoch.
  - **Value**: { source = WalSegment | Chunk | Delta, location_pointer, lsn, crc64, prev_epoch }.
  - Maintains a full history limited to non-checkpointed generations. Once a checkpoint absorbs the highest epoch for a page, all older WAL-backed entries for that page become reclaimable if no reader references them.

- page_version_head
  - **Key**: (tenant_id, page_id).
  - **Value**: { head_epoch, head_source }.
  - Fast path for locating the latest visible page. Used by PageCache and the checkpoint planner.

- snapshot_readers
  - **Key**: (tenant_id, reader_id).
  - **Value**: { snapshot_epoch, last_heartbeat_ts, client_tags }.
  - Readers renew heartbeats periodically (default 5s). Lack of heartbeat for > timeout retires the entry.

- snapshot_floor
  - **Key**: 	enant_id.
  - **Value**: { min_snapshot_epoch, writer_high_watermark }.
  - Updated eagerly when readers register/release snapshots. Derivation is inexpensive because we query snapshot_readers dupsort order.

### LMDB Table (Extended)
- wal_catalog
  - Add fields: { segment_epoch_range, sealed_at, checkpoint_state, newest_reader_epoch, gc_ticket }.
  - checkpoint_state transitions: Open -> Sealed -> Indexed -> CheckpointReady -> Checkpointed -> Truncated.

- change_log
  - Include new ManifestComponent::PageIndex entries describing checkpoint actions that deleted index rows.

## Write Path Changes
1. **Frame Preparation** – When the write controller prepares a WAL frame, it captures (tenant_id, page_id, payload, prev_epoch) from the dirty page descriptor.
2. **Segment Delta Journal** – Each WAL segment maintains an append-only page_index_delta file storing { page_id, new_epoch, prev_epoch, checksum }. This file lives alongside the WAL segment and is fsynced whenever the WAL segment fsyncs.
3. **Index Publication** – After the WAL writer seals a segment or hits a periodic threshold (e.g., every 4 MiB), it opens an LMDB write transaction:
   - Insert page_version_index records for the accumulated frames.
   - Update page_version_head to point to the newest epoch.
   - Update wal_catalog state to Indexed and record segment_epoch_range.
   - Commit the LMDB transaction before acknowledging the WAL seal.
4. **Crash Recovery** – On startup, if a WAL segment is Sealed but not Indexed, the writer replays its page_index_delta file to rebuild the missing LMDB entries. This keeps WAL and index authoritative.

## Reader Lifecycle
1. **Open Snapshot** – The libsql front-end asks PageStore for a snapshot. PageStore assigns a eader_id, records snapshot_epoch = writer_high_watermark in snapshot_readers, and returns a lease token.
2. **Serving Pages** – When PageCache misses, PageStore::read_page consults page_version_head. If the head epoch is newer than the reader snapshot, it traverses the chain via page_version_index until it finds an epoch <= snapshot_epoch.
3. **Heartbeat** – Active readers call enew_snapshot(reader_id) every few seconds. Missing heartbeats past the timeout expire the snapshot entry.
4. **Release** – Upon transaction completion, close_snapshot(reader_id) removes the entry and possibly advances snapshot_floor.

The snapshot floor equals min(snapshot_epoch) across active readers, or writer_high_watermark when none exist. It drives checkpointing decisions.

## Checkpoint Integration

### Planner Input
- Query wal_catalog for segments with checkpoint_state = Indexed and segment_epoch_range.high <= snapshot_floor.
- For each candidate segment, read page_version_index rows where source == WalSegment(segment_id).
- Build a CheckpointPlan action that materializes the newest epoch per page into chunks/deltas, omitting entries superseded by later epochs that are already checkpointed.

### Executor Behavior
- Stage chunks by streaming the WAL segment and emitting pages according to the index.
- For libsql mode, we typically materialize deltas layered on top of the last checkpoint chunk; the index already lists which pages changed.
- After successfully uploading, the executor issues a manifest batch containing:
  - ManifestOp::UpsertChunk / UpsertChunkDelta for staged pages.
  - ManifestOp::DeletePageIndex { tenant_id, page_id, epoch } for each WAL-backed entry now covered by the checkpoint.
  - ManifestOp::MarkWalSegmentCheckpointed to move the segment state.

### Post-Checkpoint Cleanup
- Once the manifest commit succeeds, the orchestrator:
  - Removes index entries (page_version_index) whose epoch now maps to a chunk/delta.
  - Updates page_version_head if the head pointer referenced the retired WAL epoch.
  - Transitions wal_catalog to Checkpointed and enqueues a GC ticket.

## Garbage Collection and Truncation
1. **Segment Eligibility** – A segment is truncation-safe when:
   - checkpoint_state == Checkpointed.
   - segment_epoch_range.high < snapshot_floor.
   - No page_version_index rows reference the segment.
2. **Manifest GC Batch** – The retention orchestrator emits a batch that deletes the wal_catalog row, removes local WAL files, and schedules remote deletion (if mirrored).
3. **Page Chain Compaction** – When older epochs disappear, we optionally rewrite page_version_index to keep only the newest WAL epoch and any checkpoint epoch that predates the snapshot floor. This keeps the index bounded.
4. **Quota Hooks** – StorageQuota subtracts reclaimed bytes from the WAL budget and releases capacity back to the writer pool.

## Failure Handling
- **Crash before Index Publication** – Recovery scans WAL segments with checkpoint_state = Sealed and replays page_index_delta files using segment_epoch_range metadata to avoid duplicate entries.
- **Crash during Checkpoint** – The checkpoint executor records progress in wal_catalog.checkpoint_state. On restart, unfinished work remains in Indexed. The planner can reschedule it; the idempotent manifest batch prevents duplication.
- **Reader GC Races** – snapshot_floor updates occur inside LMDB transactions that also mark reader entries as expired. The planner always reads snapshot_floor after acquiring a checkpoint lease, so the value cannot move backward while a checkpoint is in progress.

## Observability
- Metrics:
  - page_index.rows{tenant} – total WAL-backed versions tracked.
  - snapshot_readers.active{tenant} – current reader count.
  - wal_segments.blocked_by_readers – segments awaiting reader floor advancement.
  - checkpoint.wait_for_floor_ms – planner time spent waiting for eligible segments.
- Tracing:
  - Span around PageStore::read_page showing which epoch satisfied a page request.
  - Span for CheckpointPlanner::select_mvcc_segments including chosen segments and blocking readers.
- Logging:
  - Warn if a reader heartbeat drops without clean release.
  - Info when a segment transitions to CheckpointReady, Checkpointed, and Truncated.

## Interaction with PageCache
- PageCache already keeps dirty pages and leases. We extend the dirty-page metadata to carry prev_epoch so the writer can link chains without additional lookups.
- When PageCache evicts a clean page, it updates page_version_head lazily the next time the writer touches that page. Lazy updates are safe because the authoritative data lives in the index.

## Open Questions
- Do we need to persist the entire per-page history or just the chain until the next checkpoint? A configurable cap (e.g., keep at most N historical epochs) may be sufficient.
- Should snapshot_readers survive process restarts? Persisting them avoids false-positive GC but requires clients to reconnect and clean up entries.
- For multi-node read replicas, we may need a distributed lease system instead of a local heartbeat table.
- Can we batch ManifestOp::DeletePageIndex rows to reduce LMDB churn during large checkpoints? We may want a compressed representation per segment.
- How aggressively should we compact the index when many pages share the same newest epoch? A background task could merge duplicate chains.

## Next Steps
1. Implement WAL segment delta journaling and index publication in the write path.
2. Add reader registration APIs to the libsql adapter, backed by snapshot_readers and snapshot_floor tables.
3. Extend the checkpoint planner to pull eligible segments using the snapshot floor and page index.
4. Enhance GC flows to remove index entries and truncate WAL segments once checkpoints finish.
5. Backfill documentation (checkpointing_design.md) once the implementation crystallizes.
