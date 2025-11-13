# Checkpointing and Tiered Storage Design

This document captures the checkpointing architecture for bop-storage and how
it coordinates manifest-driven jobs, local disk limits, S3 replication, and the
PageStore hierarchy.

## Table of Contents
- [Goals](#goals)
- [System Overview](#system-overview)
- [Append-Only Truncation](#append-only-truncation)
- [Checkpoint Source Variants](#checkpoint-source-variants)
- [Checkpoint Pipeline](#checkpoint-pipeline)
- [Manifest Integration](#manifest-integration)
- [Chunk Delta Workflow](#chunk-delta-workflow)
- [Manifest Driven Jobs](#manifest-driven-jobs)
- [Local Disk Tier](#local-disk-tier)
- [S3 Remote Store](#s3-remote-store)
- [Cold Scan Reads](#cold-scan-reads)
- [PageStore Integration](#pagestore-integration)
- [Failure Handling and Idempotency](#failure-handling-and-idempotency)
- [Observability](#observability)
- [Open Questions](#open-questions)

## Goals
- Provide deterministic checkpoints that can be reconstructed from manifests alone.
- Support incremental chunk deltas to avoid full rewrites when only a small fraction of pages change.
- Schedule checkpoint work through declarative plans so execution can be retried, resumed, or sharded across workers.
- Keep local disk usage bounded while still staging I/O efficiently.
- Stream data to and from S3 without starving PageStore foreground traffic.
- Let PageStore promote and demote pages between memory, local disk, and remote storage based on access patterns.

## System Overview

    +--------------+      plan jobs      +-----------------+
    | Checkpoint   |-------------------->| Manifest Queue  |
    | Planner      |                     +--------+--------+
    +------+-------+                              |
           |                                      | consume job
           | checkpoint plan                      v
    +------+-------+     stage to disk   +--------+--------+   upload   +------------+
    | Checkpoint   |-------------------->| LocalStore      |----------->| Remote S3  |
    | Executor     |                     +--------+--------+            +------+-----+
    +------+-------+                              |                            |
           | publish manifest                     | spill or fetch             |
           v                                      v                            v
    +--------------+                      +----------------+        +----------------+
    | PageStore    |<-------------------->| PageCache      |<------>| PageHandles    |
    +--------------+  access pages        +----------------+        +----------------+

At the heart of the system is an append-only manifest log that records every
checkpoint. Jobs reference manifest identifiers instead of raw files so
execution remains idempotent.

## Append-Only Truncation

Append-only tenants require bounded retention without rewriting existing chunks. We support two truncation directions while preserving atomicity with ongoing checkpoints.

- **Tail truncation (back)** – Operators can drop the most recent unsealed tail chunk when they need to roll back a partially ingesting WAL segment. The truncation request pauses the checkpoint planner, marks any in-flight job touching that chunk as cancellable, and issues a manifest batch that deletes the corresponding `wal_catalog` and `chunk_catalog` entries in the same LMDB transaction. If the chunk already exported deltas, the batch removes those `chunk_delta_index` rows as well. Remote blobs are evicted only after the manifest commit lands so readers never observe missing data.
- **Head truncation (front)** – Retention policies remove the oldest fully sealed chunks along chunk boundaries. A truncation job walks `chunk_catalog` in order, selects chunks before the new low-watermark, and applies `ManifestOp::DeleteChunk` plus matching refcount decrements. Because append-only mode never reuses chunk ids, this job can stream removals one chunk at a time without touching PageCache. Any `SnapshotChunkKind::Base` or delta that references the chunk blocks truncation until snapshots are retired.

### Lease-Based Coordination

Both operations coordinate with the checkpoint executor through a lightweight lease protocol:

1. **Lease Registration**: When a checkpoint executor begins processing a chunk, it registers a lease in the planner's generation lease map, keyed by `chunk_id`. The lease records the generation being written and the executor's job ID.

2. **Truncation Check**: Before issuing a truncation manifest batch, the planner checks the lease map for any active lease with `generation >= target_generation`. If found, the planner waits up to 30 seconds for the lease to expire.

3. **Cancellation Signal**: If a checkpoint is mid-flight during truncation, it receives a cancellation message via the job queue. The executor flushes any staged data to a temporary location, releases its lease, and exits cleanly without publishing to the manifest.

4. **Lease Release**: Executors release their lease in three cases:
   - Successfully publishing the checkpoint (after manifest commit)
   - Receiving a cancellation signal (before manifest publication)
   - Job timeout or failure (automatic release via lease expiry)

5. **Manifest Atomicity**: Once all blocking leases are released or expired, the truncation operation commits a single LMDB transaction that removes catalog entries and appends a `ManifestChangeRecord` describing the truncation. This ensures subscribers (GC, replication) observe the removal atomically.

Truncation batches emit a `ManifestChangeRecord` describing the removed chunks, updated generation watermarks, and new retention window. Subscribers (e.g. GC, remote replication) process the change log to delete local scratch files and S3 objects only after the manifest commit is durable, keeping the system consistent under retries.

## Checkpoint Source Variants

Foreground mutations always land in a `WalSegment`, but workloads split into two
patterns:

- Append-only tenants append variable-length records that map 1:1 onto a new
  tail chunk at checkpoint time. The planner can seal the segment, verify the
  checksum, and promote it as a fresh 64 MiB chunk with no diffing.
- libsql-style tenants may touch arbitrary pages across many existing chunks.
  The planner diffs dirty pages against the active manifest, grouping writes by
  chunk id. If the modified footprint stays small it emits an incremental chunk
  containing just the changed pages; when churn crosses a threshold it rewrites
  the full 64 MiB chunk instead.

The resulting `CheckpointPlan` records which WAL ranges feed each chunk action,
so execution can stage the right set of files in `checkpoint_scratch/`.

## Checkpoint Pipeline

1. **Trigger**: A checkpoint may be requested by WAL pressure, timer, or operator
   API. The trigger hands control to CheckpointPlanner with the last durable
   manifest id and current WAL high water mark.

2. **Plan**: The planner queries PageCache and WAL metadata to determine which page
   versions need persistence. It emits a CheckpointPlan that lists page ranges,
   estimated sizes, and the manifest dependencies being superseded.

3. **Quota Reservation**: The CheckpointPlanner requests bytes from the StorageQuota
   service before enqueuing the job. The reservation is allocated to the job's
   unique ID and covers both staging scratch space and potential LocalStore cache
   growth. If the reservation cannot be granted immediately, the plan stays in the
   queue until disk pressure subsides or the job is re-sized. The reservation is
   held until Step 7 (cleanup) completes.

4. **Execution**: CheckpointExecutor streams dirty pages into staging files under
   LocalStore/checkpoint_scratch/{job_id}/. Pages are written in content hash
   order so duplicates can be reused across checkpoints. The executor registers
   a lease with the planner for each chunk it processes (see Lease-Based Coordination).

5. **Validation**: Each staged file records SHA-256 plus length in a small metadata
   footer. Once the staging set is complete, the executor verifies the recorded
   hashes before upload.

6. **Publication**: After remote uploads succeed AND validation completes, the executor
   submits a batch of manifest operations to the LMDB writer's command queue. The writer
   opens a single LMDB transaction, records new chunk entries, updates the related WAL
   catalog rows, bumps generation watermarks, and appends a `ManifestChangeRecord` to
   the `change_log`. The commit is atomic so subscribers observe the same set of changes
   that became durable. Multiple checkpoint executors serialize at this point through the
   single-threaded manifest writer queue.

7. **Cleanup**: After manifest publication succeeds, the executor:
   - Releases all chunk leases registered in Step 4
   - Deletes local scratch data from checkpoint_scratch/{job_id}/
   - Releases the StorageQuota reservation allocated in Step 3

   If the job fails before publication (e.g., upload error, cancellation), the cleanup
   phase still executes to release leases and quota, but the manifest remains unchanged.
   The CheckpointGC worker later prunes older checkpoints that no longer satisfy retention
   policy.

## Manifest Integration

The manifest now lives inside a dedicated LMDB environment managed by a single writer thread. Checkpoint publication submits a batch of `ManifestOp` values to that worker so every catalog update and change-feed record commits inside one LMDB transaction. Readers open snapshot transactions and observe a consistent view without blocking the writer.

Key LMDB databases involved in checkpointing:

- `chunk_catalog`: Immutable chunk metadata keyed by chunk id. Entries capture tenant, generation, content-hash key, residency (local/S3), compression, and purge timers. Checkpoints insert new rows or mark superseded chunks so GC can retire them later.
- `chunk_delta_index`: Incremental updates keyed by `(db_id, chunk_id, generation)`. Each record references the base chunk, delta object file, residency, and byte size so readers can layer patches without rewriting the base.
- `wal_catalog`: WAL artifacts (append-only segments or pager bundles) with their local paths and per-format metadata. When a checkpoint absorbs a WAL range the writer updates these rows to reflect coverage and to signal when the WAL segment may be truncated.
- `change_log`: Append-only feed of `ManifestChangeRecord` values ordered by `ChangeSequence`. Each record carries the operations that landed in the commit plus component generation watermarks so subscribers can rebuild state.
- `change_cursors`: Per-subscriber acknowledgements used to trim the change log and to resume streams after restarts.

Generation tracking replaces the old CheckpointIndex file: every batch records its resulting generation watermark for the checkpoint component, and readers consult the LMDB tables to discover the active chunk set. Because chunks and WAL artifacts live in separate catalogs we can evolve them independently while still publishing a single coherent change record per checkpoint.

Subscribers (GC, replication, page-serve nodes) attach to the change feed via registered cursors, fetch pages of change records, and acknowledge sequences. As soon as all cursors have consumed a prefix, the writer trims the log within the same LMDB transaction, keeping storage bounded.

## Chunk Delta Workflow

Incremental checkpoints avoid rewriting an entire 64 MiB base chunk when only a handful of pages change. The executor writes a delta chunk in `checkpoint_scratch/`, uploads it, and asks the manifest writer to emit `ManifestOp::UpsertChunkDelta` operations. Each entry inserted into `chunk_delta_index` is keyed by `(db_id, chunk_id, generation)` and its record stores the `base_chunk_id`, a monotonic `delta_id`, the staged file name, residency state, and byte size. The original base chunk metadata stays in `chunk_catalog`; deltas layer on top so PageStore composes the newest view without touching the base file.

When churn crosses a threshold or several deltas need coalescing, the planner schedules a full chunk rewrite. That publication batch issues `ManifestOp::DeleteChunkDelta` for superseded generations and updates the base chunk entry, keeping the delta index bounded. Remote object names mirror the relationship: delta uploads use `RemoteObjectKind::Delta` with the matching `delta_id`, which enables deduplication and provenance checks.

Snapshot metadata references deltas through `SnapshotChunkKind::Delta`, so recovery can replay the latest delta chain or materialize a fresh base chunk. Garbage collection walks `chunk_delta_index` alongside chunk refcounts; once a replacement base chunk is durable and no snapshot pins the older generation, the delta row and remote object are reclaimed.

## Manifest Driven Jobs

Manifest jobs encapsulate durability work. The queue is backed by an embedded
LMDB so it survives restarts and replicates across control plane processes.

Job types:

- CreateCheckpoint: Run planner and executor pipeline.
- VerifyManifest: Re-fetch blobs, validate checksums, mark manifest as verified.
- DownloadCheckpoint: Hydrate local disk from a manifest (warm standby flow).
- PruneCheckpoint: Remove manifests and blobs once retention conditions are met.
- TruncateAppendOnly: Execute front/back chunk truncations for append-only tenants once checkpoints release leases.

Each job carries an idempotency token and references manifest identifiers instead
of raw file paths. Workers record progress (for example "uploaded 7 of 12 blobs")
so they can resume after crashes.

Jobs pull quota reservations from StorageQuota. If a worker crashes mid job,
leases time out and the reservation returns to the pool.

## Local Disk Tier

LocalStore manages two namespaces:

- hot_cache/: Backing store for PageCache spillover. Stores sealed 64 MiB
  chunk files that mirror the PageStore layout and keeps them pinned while active.
- checkpoint_scratch/: Temporary space for checkpoint assembly. Directories are
  keyed by job id and hold in-progress chunk files before they are uploaded.

Key behaviors:

- StorageQuota enforces global limits with hierarchical budgets (per tenant and
  per job). Plans must reserve bytes before staging can start.
- LRU eviction in hot_cache/ ignores files with positive pin counts supplied by
  PageCache. Evicted entries retain their manifest reference so remote fetches
  remain possible.
- Checkpoint staging always produces 64 MiB chunk files, matching the on-disk
  PageStore format, so the same file can serve both local cache and remote
  replication paths.
- Scratch directories are deleted once the manifest commits. A janitor coroutine
  guards against leaks by scanning for abandoned jobs and reclaiming space.

This tier plays a dual role: checkpoint execution constructs new 64 MiB chunks
inside `checkpoint_scratch/`, while S3 downloads hydrate `hot_cache/` so PageStore can
serve reads without re-fetching remote objects. Because every cached file mirrors
the PageStore chunk layout, we can reuse the exact bytes for checkpoint upload
retries or future manifests. Storage quotas account for both scratch staging and
chunk caching, ensuring large checkpoints do not evict frequently accessed
chunks and keeping S3 traffic to the minimum necessary.

## S3 Remote Store

RemoteStore wraps the S3 SDK with streaming APIs:

- put_blob(key, reader, size, checksum): Multipart uploads with retry and
  checksum validation against the staged footer.
- get_blob(key, writer): Streams directly into PageCache or a LocalStore file
  while verifying checksums.
- head_blob(key): Lightweight existence check for idempotent retries.

### Blob Naming Scheme

Uploaded blobs use a stable naming scheme organized by object kind:

- **Base chunks**: `checkpoints/{tenant}/{checkpoint_id}/chunk-{chunk_id}-{generation}-{hash}.blob`
- **Delta chunks**: `checkpoints/{tenant}/{checkpoint_id}/delta-{chunk_id}-{delta_id}-{generation}-{hash}.blob`
- **Snapshots**: `snapshots/{tenant}/{snapshot_id}/meta-{hash}.json`

Where:
- `checkpoint_id` corresponds to the manifest generation watermark at publication time
- `chunk_id` is the stable identifier from `chunk_catalog`
- `delta_id` is a monotonic counter for incremental updates to the same base chunk
- `hash` is the SHA-256 content hash of the blob payload

This layout enables deduplication (identical content produces identical keys), supports both base and delta objects in the same namespace, and allows GC to list blobs by checkpoint or tenant prefix efficiently.

The LMDB manifest is updated only after these uploads succeed, so any
`ManifestChangeRecord` visible to subscribers references blobs that already
exist in remote storage.

## Cold Scan Reads [🚧 PLANNED]

Some workloads perform large analytical scans that should not disturb the local
chunk cache. `ColdScan` mode allows callers to stream pages directly from S3
without populating `hot_cache/` or PageCache:

- `PageStore::read_page_trickle` takes a tenant id, page handle, and an async
  sink the caller owns (e.g. a buffer pool or temporary file).
- The method resolves the manifest entry, issues `RemoteStore::get_blob` with a
  passthrough writer that performs checksum validation but skips local caching.
- Calls register with a per-tenant `RemoteReadLimiter` so background scans cannot
  starve foreground traffic. The limiter enforces bandwidth ceilings and
  concurrent request caps.
- Metrics report `page_fetch_latency_seconds{tier="ColdScan"}` and
  `remote_bytes_read_total{mode="cold_scan"}` to highlight remote pressure.
- Access control and generation checks mirror the normal read path; GC waits for
  outstanding cold scans before reclaiming blobs.

Cold scans default off; clients opt in per request when they detect sequential
scans or spillover workloads.

### API Signature

```rust
impl PageStore {
    pub async fn read_page_trickle<W>(
        &self,
        tenant: TenantId,
        handle: PageHandle,
        mut sink: W,
        opts: ColdScanOptions,
    ) -> Result<(), PageStoreError>
    where
        W: futures::io::AsyncWrite + Unpin,
    {
        // Resolve manifest -> remote blob, stream bytes without mutating caches
    }
}
```

**Types**:
- `TenantId`: Opaque identifier for tenant-level quota tracking (defined in `crate::tenant`)
- `PageHandle`: Logical page reference containing `(namespace, object_id, page_no)`, equivalent to `PageCacheKey` from [page_cache.rs](../src/page_cache.rs)
- `PageStoreError`: Union of manifest lookup failures, remote fetch errors, and quota violations

`ColdScanOptions` wraps throttling hints (priority, expected span) so the
limiter can enforce per-tenant budgets while letting the scheduler deprioritize
these remote-heavy reads. The method feeds the existing remote streaming
pipeline but simply skips the LocalStore write step.

## PageStore Integration

### Read Path

1. Client requests a page. PageStore looks in PageCache for a fresh copy.
2. On miss, PageStore consults LocalStore and loads the page if available.
3. If neither tier has the page, PageStore issues RemoteStore.get_blob using
   the manifest entry. The blob streams into a LocalStore file, and the requested
   page is inserted into PageCache for subsequent reads.

**Pinning Behavior**: Currently, pages are inserted into the cache on demand rather than
pre-pinned during remote fetch. Future optimization may support pinned PageCache slots
during streaming (controlled by caller via `FetchOptions::pin_in_cache`).

### Write Path

1. Updates land in WAL and PageCache. The cache retains dirty buffers until the
   checkpoint pipeline persists them.
2. During checkpoint execution, PageCache hands dirty pages to the executor,
   which emits staged files and later local and remote blobs.
3. Once the manifest commits, PageStore updates its internal mapping from
   PageHandle to blob_ref. Old handles remain valid until GC removes them.

### Eviction

- PageCache evicts pages only after confirming LocalStore holds the blob. If
  LocalStore needs space it may drop the file but keeps the manifest pointer.
- RemoteStore is the source of truth; any tier may rehydrate from it on demand.

## Failure Handling and Idempotency

- Planner outputs include deterministic chunk ordering so retries do not change
  blob keys.
- Uploads are idempotent because keys incorporate hashes; duplicate puts become
  no-ops.
- Manifest publication relies on the single-writer LMDB transaction so catalog
  updates and change-log appends remain atomic even when retries occur.
- Crash recovery replays the job queue, resuming incomplete stages based on the
  persisted progress markers.

## Observability

Expose metrics via StorageMetrics:

- `checkpoint_duration_seconds`: Histogram tracking end-to-end checkpoint time from plan to cleanup
- `checkpoint_bytes_written_local`: Counter for bytes written to LocalStore staging
- `checkpoint_bytes_written_remote`: Counter for bytes uploaded to S3
- `checkpoint_queue_depth`: Gauge showing pending checkpoint jobs
- `storage_quota_exhausted_total`: Counter incremented when quota reservations fail
- `page_fetch_latency_seconds{tier}`: Histogram with `tier` label values: `PageCache`, `Local`, `Remote`, `ColdScan`
- `remote_bytes_read_total{mode}`: Counter with `mode` label values: `normal`, `cold_scan`

### Metric Labels and Cardinality

- `tier`: Fixed set of 4 values (`PageCache`, `Local`, `Remote`, `ColdScan`)
- `mode`: Fixed set of 2 values (`normal`, `cold_scan`)
- `tenant_id`: Per-tenant basis (unbounded cardinality; use with caution or aggregate at query time)
- Export format: Prometheus exposition format on `/metrics` endpoint

### Structured Logs

Checkpoint job state transitions emit structured log events (JSON) at INFO level:

- `checkpoint.started`: `{job_id, tenant, plan_size_bytes, wal_range}`
- `checkpoint.staged`: `{job_id, staged_chunks, staged_bytes, duration_ms}`
- `checkpoint.uploaded`: `{job_id, uploaded_blobs, uploaded_bytes, duration_ms}`
- `checkpoint.published`: `{job_id, manifest_generation, checkpoint_id}`
- `checkpoint.cleaned_up`: `{job_id, quota_released_bytes}`
- `checkpoint.failed`: `{job_id, error, stage, retryable}`

## Open Questions

### Q1: S3 Blob Coalescing Strategy
**Question**: How aggressively should we coalesce small pages into larger blobs to maximize S3 throughput?

**Status**: Early prototyping shows 2-4x throughput improvement when coalescing 4KB pages into 4MB blobs vs. individual page uploads.

**Tradeoffs**:
- Larger blobs reduce S3 request overhead but increase latency for single-page fetches
- Coalescing adds CPU/memory cost during checkpoint staging
- Random access patterns may download unnecessary data

**Decision Deadline**: Before production scale testing (>100GB checkpoints)

**Next Steps**: Benchmark with real workload traces; consider adaptive coalescing based on access patterns

---

### Q2: Differential Manifests for Large Datasets
**Question**: Do we need differential manifests (delta logs) in addition to full manifests for very large datasets?

**Status**: Current `change_log` design provides incremental feed, but manifest reads still require full LMDB table scans for chunk lookups.

**Tradeoffs**:
- Differential snapshots reduce manifest read latency but complicate GC and snapshot materialization
- Compaction overhead for merging differential logs
- Query complexity for resolving latest chunk state across multiple differential layers

**Decision Deadline**: Blocked on testing with >1TB datasets and >10K chunks per tenant

**Next Steps**: Profile manifest read latency at scale; prototype differential catalog layering in LMDB

---

### Q3: Quota Reservations for Verification Jobs
**Question**: Should quota reservations span verification jobs, or can we borrow bytes from an available pool when verification re-downloads blobs?

**Status**: Current design requires explicit reservation for all disk-touching jobs to prevent overcommit.

**Tradeoffs**:
- Borrowing from available pool increases quota utilization but risks thrashing if foreground traffic spikes
- Explicit reservations provide strict guarantees but may block verification during quota pressure
- Verification is read-only and can stream without LocalStore staging, reducing actual disk usage

**Decision Deadline**: Before enabling background verification in production

**Next Steps**: Measure typical verification working set size; consider separate quota pool for read-only verification work
