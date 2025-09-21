# Catalog Snapshot Design

## Overview
- Snapshots capture the durable view of an `Aof` instance so recovery and replication can rebuild residency without replaying the entire operation log.
- This document refines RC1: persist catalog snapshots that reference the tiered operation log watermark.
- Inputs come from runtime state (`AofManagement`), tier metadata (durability cursors from AP4), and coordinator state (operation log sequence).

## Payload Structure
| Section | Fields | Source | Notes |
| ------- | ------ | ------ | ----- |
| Header | instance id, snapshot id, captured at, coordinator watermark | `TieredInstance::instance_id`, monotonic counter, `Utc::now`, `TieredCoordinator::watermark()` | Snapshot id can reuse the coordinator sequence or a UUID; watermark is the last applied log entry. |
| Catalog | `SegmentCatalogEntry` for active and sealed segments | `Aof::catalog_snapshot()` | Include `segment_id`, `sealed`, `base_offset`, `base_record_count`, `current_size`, `record_count`.
| Flush Queue | ordered list of `ResidentSegment` ids awaiting durability | `AofManagement::flush_queue` | Maintains scheduling priority for background flushes.
| Pending Finalize | segments waiting for footer writes | `AofManagement::pending_finalize` | Ensures startup reseals or retries footers before admitting new segments.
| Durability Cursors | `requested_bytes`, `durable_bytes` per segment | AP4 `DurabilityCursor` on `TieredInstance` | Aligns flush metadata with tier residency and informs readers about watermarks.
| Tier Residency | Tier 0 and Tier 1 residency hints | `TieredInstance::manifest_snapshot` | Allows recovery to warm caches without a full hydration sweep.
| Operation Log Tail | last applied operation and pending in-flight commands | `TieredCoordinator` | Needed so replay can resume at `watermark + 1`.

## Watermark Semantics
- `coordinator_watermark` represents the highest operation log entry fully applied to Tier 0/1.
- `durability_watermark` represents the highest byte offset proven durable across all segments (AP4 cursor).
- The snapshot must guarantee `coordinator_watermark >= durability_watermark`; otherwise replay would reference residency that is not yet durable.
- Recovery applies the following ordering:
  1. Restore durability cursor and flush queue.
  2. Replay operation log entries `(watermark + 1 .. latest)`.
  3. Rekindle hydrations for segments whose residency is stale relative to durability.

## Persistence Workflow
1. Acquire a lightweight read guard over `AofManagement` to copy catalog, pending finalize queue, and flush queue.
2. Query the `DurabilityCursor` for each resident segment and attach `{requested, durable}` bytes.
3. Request a consistent coordinator watermark (read-only call that does not stall the service loop).
4. Serialize the payload to an intermediate buffer (CBOR or JSON lines are simple starting points).
5. Write to a temp file under `layout.archive_dir()/snapshots`. Use the same atomic rename pattern as manifest writes (`fs::write` + `fsync`).
6. Optionally upload to Tier 2 (MinIO/S3). Record the remote key in the snapshot header for audit.
7. Expose metrics for duration, payload size, and watermark delta.

## Coordination with Flush and Rollover
- `FlushRequest` now carries `ResidentSegment` and `InstanceId` context (`rust/bop-rs/src/aof2/flush.rs:31`); the flush worker can update the `DurabilityCursor` immediately after `mark_durable`.
- Snapshots should reuse that cursor to avoid double bookkeeping.
- While a snapshot is in flight, the flush queue continues to progress. Capture the queue at the start and allow divergence; recovery will reschedule anything missing via the cursor deltas.
- When a rollover is pending, include the ack state so recovery knows whether to resume compression or requeue the rollover command.

## Validation Plan
- **Unit tests**: synthesize a catalog with pending flushes, generate a snapshot payload, and assert that `recover_existing_segments` reconstructs identical state.
- **Integration test**: append records, trigger flush and rollover, capture a snapshot, drop the process, then restart using only the snapshot plus Tier 2 data.
- **Fuzz test (stretch)**: mutate snapshot payloads to ensure the loader rejects inconsistent durability cursors or watermark regressions.

## Observability
- Metrics: snapshot duration, bytes written, durability watermark lag, coordinator watermark lag.
- Logs: structured entries on snapshot start, success, failure, and Tier 2 upload status.
- Traces: span covering the entire capture plus child spans for serialization, fsync, and upload.

## Open Questions
- Do we require incremental snapshots (delta since last capture) or is a full snapshot per capture acceptable for the initial milestone?
- What retention policy should prune old local and Tier 2 snapshot artifacts?
- Should we co-locate the snapshot with Tier 1 manifest writes or keep them independent to limit coupling?
- How should we secure snapshots (encryption at rest, access control) before production rollout?

## Next Steps
1. Finalise the serialization format (recommend CBOR for compactness with typed fields).
2. Prototype `SnapshotWriter` in `rust/bop-rs/src/aof2/mod.rs` using the workflow above.
3. Coordinate with recovery consumers to define acceptable recovery times using snapshot + replay.
4. Extend the implementation plan with follow-on tasks for retention, validation, and observability once RC1 code lands.
