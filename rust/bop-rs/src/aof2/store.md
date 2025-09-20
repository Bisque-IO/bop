# AOF2 Tiered Segment Store Design

This document outlines a tiered storage system for AOF2 segments. The goals are to keep the append path fast, maintain a bounded local footprint, and provide deterministic archival across the cluster. The system is divided into three tiers:

- Tier 0 (Hot) - local, memory-mapped segments that can be opened for read or write with minimal latency.
- Tier 1 (Warm) - on-node cache of sealed segments stored as seekable Zstandard streams to balance density and random access.
- Tier 2 (Cold) - deep storage backed by an S3-compatible object store.

Only the current Raft master performs archival, compaction, or cross-tier mutations. All other nodes consume a replicated log of these operations to stay in sync.

---

## Tier 0 - Hot Cache

Purpose: host active segments that are currently writable or have been recently read by clients. All segments remain in the native mmap format for zero-copy append and read paths.

Characteristics

- Backed by a fast local filesystem.
- Houses all active segments plus a sliding window of sealed, recently accessed segments.
- Managed by the AofManager within each process.

Capacity enforcement

- Each AofManager is configured with a cluster-wide maximum for total Tier 0 bytes.
- Active segments are always admitted. Sealed segments are kept in an LRU list by access and evicted when the limit is exceeded.
- When admission would exceed the limit, sealed segments are queued for eviction (promotion to Tier 1). If only active files remain, new writers block until space is reclaimed through sealing and eviction.

Pending ingress queue

- Tier 0 exposes a PendingActivationQueue that tracks segments waiting for local space. The queue is ordered by priority, for example write urgency or follower catch-up demand.
- When an Arc<Segment> leaves the Tier 0 cache (for example, the last strong reference drops after sealing), the eviction path enqueues the segment for Tier 1 placement if it is not already present there; background workers perform the copy-on-evict conversion.
- When space becomes available, the queue drains in order, paging the selected segments from Tier 1 (or Tier 2 via Tier 1) back into mmap form.

Concurrency

- Appends and reads operate only on Tier 0 objects.
- Background workers consult Tier 0 metadata for eviction candidates and to honour admission priorities.

---

## Tier 1 - Warm Cache and Compression Layer

Purpose: provide an expanded on-node reservoir of sealed segments, compressed with seekable Zstd frames to reduce local storage footprint while still allowing random reads without full decompression.

Responsibilities

- Receive evicted Tier 0 segments, converting mmap files into seekable Zstd archives while preserving footer metadata and record offsets.
- Serve as the source for rehydrating segments back into Tier 0 upon cache misses or replay requests.
- Batch and stage uploads into Tier 2 according to retention policy and master directives.
- Drive compression and hydration through Tokio tasks; blocking filesystem work and zstd transforms run on the runtime's blocking pool.
- Drive compression and hydration through Tokio tasks; blocking filesystem work and zstd transforms run on the runtime's blocking pool so Tier 0 never stalls.

Metadata and indexing

- Maintain a manifest of available segments, their compression dictionaries, and byte ranges to enable targeted partial reads.
- Track residency state (ResidentInTier0, StagedForTier0, UploadedToTier2) for orchestration decisions.

Space management

- Configurable size budget independent of Tier 0. Eviction policy is typically LRU by last access, with master directives able to pin segments.
- Compression jobs run in worker pools to avoid blocking Tier 0 eviction.

Promotion and demotion flow

1. Tier 0 -> Tier 1: triggered by eviction or scheduled rollover. The segment is flushed, sealed, converted to Zstd, and the mmap file is either removed or truncated.
2. Tier 1 -> Tier 0: on cache miss or explicit prefetch, Tier 1 decompresses the segment into a new mmap file, validates checksums, and inserts it into the activation queue.
3. Tier 1 -> Tier 2: only executed on the master. The compressed payload is uploaded to S3, and the artifact is marked as durable in Tier 2. Local retention policy decides whether to keep a copy.

---

## Tier 2 - S3-Compatible Deep Storage

Purpose: provide durable, cost-effective storage for the full segment history. Acts as the system of record for sealed data.

Responsibilities (master only)

- Accept uploads from Tier 1 workers once segments meet retention thresholds, such as age or compaction state.
- Maintain a partitioned namespace such as /stream/<segment-id>.zst for parallel fetch.
- Expose operations to list, fetch, and delete according to compaction plans.
- Run an async command queue on the shared Tokio runtime; uploads and deletes are awaited in tasks gated by a semaphore so the coordinator never spins up bespoke threads.
- Use an async S3 client facade so retries, checksum validation, and retention deletes happen without blocking Tier 1 or Raft threads.

Consuming nodes

- Followers never mutate Tier 2. They mirror Tier 2 contents by replaying the master operation log, fetching objects on demand via the Tier 1 manager.
- Tier 1 hydrators call into the Tier 2 handle synchronously; the handle proxies that request onto the shared runtime so existing call sites stay synchronous while network I/O remains async.

---

## Master Coordination and Operation Log

Master role

- Driven by the Raft state machine. When a node is elected leader, it obtains an exclusive lease to perform Tier 1 -> Tier 2 actions and global compactions.
- The master enforces retention windows, orchestrates compaction or merge jobs, and coordinates global space budgets.

Append-only operational log

- All cross-tier actions are recorded in a binary, append-only log named segment_store.log. Each record includes:
  - Monotonic OperationId and Raft term.
  - Operation type (EvictToTier1, PromoteToTier0, UploadToTier2, DeleteFromTier2, CompactRange, and similar).
  - Segment identifiers and checksums.
  - Associated metadata such as compression parameters, S3 object keys, and compaction manifests.

Replication

- The master persists each operation to the log and replicates it via Raft. Followers apply the log in order to mirror tier transitions locally.
- Upon startup or leadership change, nodes first replay the log to reconstruct Tier 1 and Tier 2 manifests, then begin servicing client requests.

Failure handling

- Because uploads are idempotent and keyed by SegmentId, replaying an UploadToTier2 record is safe.
- Followers ignore mutations they cannot execute, such as S3 deletions, but mark the intent to allow retries once the master role returns.
- On leadership transfer, the new master scans the log for in-flight operations and completes or rolls them forward.

---

## Background Workers and Queues

Tier 0 eviction worker

- Monitors used bytes; when above threshold, it moves the coldest sealed segments to Tier 1. Active segments are never evicted.

Tier 1 compression and uploader

- Converts incoming mmap files to Zstd, updates manifests, and schedules uploads.
- Uses backpressure from Tier 2, such as network failures or throttling, to pause evictions.

Tier 0 rehydration worker

- Drains the pending activation queue, pulling segments down from Tier 1 or Tier 2 (via Tier 1) based on priority.

Tier 2 uploader (master)

- Pulls from a PendingUploadQueue populated by Tier 1 once segments reach upload eligibility.
- Ensures S3 uploads are committed before deleting local copies when policy allows deletion.

---

## Capacity and Policy Configuration

- Tier 0: fixed byte limit per cluster, with optional per-AOF overrides. Policy determines whether to block writers (Strict) or evict aggressively (Elastic).
- Tier 1: separate byte limit; optional minimum residency window to avoid thrashing. Supports pinning, for example snapshots or checkpoints.
- Tier 2: unlimited in principle, governed by retention policies that are time-based, size-based, or compaction driven.

Policies are stored in the Raft configuration so that all nodes agree on thresholds. The master validates that local budgets across all AOF managers honor global constraints before scheduling uploads.

---

## Interaction with Compaction and Retention

- Compaction plans originate on the master and are represented as log entries such as CompactRange. They describe input segments from Tier 1 or Tier 2, resulting merged segments, and post-compact placement.
- Tier 0 only holds post-compaction segments if they are required for near-term reads; otherwise new artifacts are written directly into Tier 1 or Tier 2.
- Retention policy triggers deletions from Tier 2 after verifying that no follower requires the segments, using replicated high watermark acknowledgements.

---

## Operational Considerations

Recovery

1. Replay the operation log to reconstruct the Tier 1 manifest and pending queues.
2. Validate Tier 0 contents against the log and evict anything not referenced.
3. Reconcile Tier 2 by listing current objects and cross-checking with the log, repairing discrepancies on the master.

Monitoring

- Export metrics such as bytes per tier, queue lengths, upload latencies, failed operations, and divergence between log index and applied state.
- Emit tracing spans around cross-tier moves to aid debugging.

Security and integrity

- All uploads carry checksums stored in the operational log; followers verify on download.
- Optional encryption at rest per tier; Tier 2 relies on S3-managed keys or client-side encryption, with keys tracked by the master.

---

## Summary of Responsibilities

| Component              | Key Responsibilities                                                        |
|------------------------|------------------------------------------------------------------------------|
| Tier 0 Manager         | Enforce local byte budget, track hot segments, maintain activation queue.   |
| Tier 1 Manager         | Compress, cache, and rehydrate sealed segments; stage uploads.              |
| Tier 2 Manager (Master)| Perform uploads and deletions, enforce retention, coordinate compaction.    |
| Operational Log        | Serialize all cross-tier mutations for deterministic replay.                |
| Raft Master            | Sole authority for Tier 2 mutations and global policy enforcement.          |
| Followers              | Apply log-driven state changes, serve local reads, request rehydration.     |

This tiered approach keeps the append path fast while ensuring the cluster can enforce global storage bounds and maintain durable archives in S3. The replicated operation log guarantees that every node converges on the same view of segment placement, even across leadership changes or partial failures.

---

## Implementation Plan

### Milestone 1: Tier 0 Cache Foundations

- [x] **Segment Residency Tracking**: Instrument AofManager to record Tier 0 residency, reference counts, and aggregate byte usage per AOF instance (Tier0Cache tracks per-instance residency totals).
- [x] **Drop-Triggered Eviction Hook**: Introduce a guard so that when the last Arc<Segment> drops, the manager enqueues the segment for Tier 1 placement if it is not already present there (SegmentDropGuard emits Tier1 placement events).
- [x] **Capacity Enforcement**: Implement global byte accounting (per-cluster quota + per-instance override) and LRU metadata structures to select sealed segments for eviction (Tier0Cache schedules LRU evictions when quotas are exceeded).
- [x] **PendingActivationQueue**: Build a priority queue that holds requests for rehydration; integrate with the enforcement logic so new requests block when Tier 0 is saturated (activation heap queues and grants requests as space frees).
- [x] **Metrics & Tracing**: Emit gauges for Tier 0 bytes, queue depth, and eviction latencies; add tracing instrumentation around drop-triggered enqueue operations (Tier0Metrics snapshots + tracing on eviction/drop).

### Milestone 2: Tier 1 Compression and Cache Layer

- [x] **Manifest & Index Structures**: Tier1 manifest/entry types track per-segment checksum, offset index, residency flags, and persist warm metadata to `manifest.json`.
- [x] **Compression Pipeline**: `Tier1Cache` spawns background workers that accept drop events, compress sealed segments to seekable Zstd, and commit manifest updates atomically.
- [x] **Integration with Tier 0**: `TieredCoordinator` bridges Tier0 drop events/activation grants into Tier1 compression and hydration so cache promotions/demotions stay coordinated.
- [x] **Space Budget**: Tier1 instance state enforces warm-layer quotas with LRU eviction, removing old warm files when `max_bytes` is exceeded.
- [x] **Retry & Integrity**: Compression jobs retry with backoff and hydration re-verifies payload checksums before publishing decompressed segments, failing safely on corruption.

### Milestone 3: Tier 2 Deep Storage (Master Controlled)

- [x] **S3 Client Abstraction**: Implemented a reusable client facade backed by MinIO with retry, exponential backoff, and optional SSE/KMS hooks.
- [x] **Upload Scheduler**: Background workers dequeue Tier 1 completions, enforce idempotence via remote HEAD checks, and emit completion events back into the manifest pipeline.
- [x] **Deletion & Retention**: Time-based retention queues schedule object deletions; Tier 1 receives delete confirmations and clears residency metadata.
- [x] **Fallback to Tier 1**: Hydration paths now source cold segments from Tier 2 when warm artifacts are missing, re-materializing them before activation.
- [x] **Security Options**: Tier 2 uploads honor optional SSE-S3, SSE-KMS, or customer-provided keys and propagate user metadata for auditing.

### Milestone 4: Operation Log & Replication

- [ ] **Log Schema**: Define binary record format for operations (EvictToTier1, PromoteToTier0, UploadToTier2, DeleteFromTier2, CompactRange, etc.) including OperationId, term, timestamps, and payload checksums.
- [ ] **Log Writer (Master)**: Integrate with Raft so each cross-tier mutation appends to the log before execution; ensure durable fsync and crash recovery.
- [ ] **Log Reader (Followers)**: Implement playback routines that reconcile local Tier 1/Tier 2 state to match the master’s history.
- [ ] **Conflict Resolution**: Establish idempotent semantics—replayed uploads, deletes, and compactions should be safe even if partially completed previously.
- [ ] **Tooling**: Provide inspection utilities for operators to audit log contents and detect divergence.

### Milestone 5: Recovery, Monitoring, and Ops

- [ ] **Startup Replay**: Combine Tier 0 scavenging, Tier 1 manifest rebuild, and Tier 2 reconciliation using the operation log.
- [ ] **Health Metrics**: Expose metrics for tier byte usage, queue lengths, upload success/failure counts, and replication lag.
- [ ] **Alerting Hooks**: Add alerts for capacity exhaustion, S3 failures, and log replay lag.
- [ ] **Testing Matrix**: Build integration tests covering drop-triggered eviction, Tier 1 compression correctness, Tier 2 upload/download paths, and leadership transitions.
- [ ] **Documentation**: Update developer and operator guides with deployment steps, tuning knobs, and troubleshooting runbooks.

### Milestone 6: Compaction & Retention Workflows

- [ ] **Compaction Planner**: Develop master-side planners that choose segment ranges for compaction and produce new artifacts directly in Tier 1/Tier 2.
- [ ] **Follower Coordination**: Ensure compaction/deletion operations pause or delay on nodes that still require older segments (via replicated watermarks).
- [ ] **Retention Policies**: Implement configurable time/size-based retention that triggers Tier 2 deletions and Tier 1 purges in a coordinated fashion.
- [ ] **Benchmarking**: Extend the Criterion harness to measure eviction throughput, compression latency, and rehydration performance across tiers.
- [ ] **Chaos Testing**: Add failure-injection scenarios (S3 outages, disk full, premature segment drop) to validate resilience.

Delivering these milestones incrementally ensures the tiered store evolves from a local cache into a fully replicated, master-coordinated archival system with clear operational guarantees.

---

## Current Progress

- Tier 0 and Tier 1 caches integrated into the AOF lifecycle with automated compression, residency tracking, and hydration.
- Warm cache manifest persists segment metadata (checksums, offset indexes, residency) and enforces LRU space budgets.
- TieredCoordinator bridges segment drop events and activation grants between Tier 0 and Tier 1; hydration paths restore segments automatically.
- Tier 2 deep storage wired: S3-compatible uploads, retention-aware deletions, and hydration fallbacks now flow through TieredCoordinator polling.
- Added unit coverage for compression persistence, warm-cache hydration, and Tier 2 upload/idempotence logic alongside existing Tier 0 tests.

## Upcoming Focus

1. Define and implement the replicated operation log for cross-tier actions (uploads, deletes, promotions).
2. Expand observability: surface Tier 2 metrics, success/failure counters, and queue depths through the existing flush exporter.
3. Design recovery/startup replay that reconciles Tier 1 manifests with Tier 2 object state and newly captured operation logs.


## Async Integration Plan

1. **Surface TieredCoordinator in AofManager**
   - Extend AofManagerConfig with Tier-0/1/2 settings (defaults mirroring legacy constants).
   - Initialize Tier0Cache, Tier1Cache::new(rt.handle().clone(), cfg), and optional Tier2Manager::new(rt.handle().clone(), cfg) inside AofManager::with_config.
   - Expose handles (tier0(), tier1(), tiered_store()) so tests and callers can reach the caches.

2. **Drive TieredCoordinator from the manager runtime**
   - Replace the existing servicing logic with an async helper that awaits TieredCoordinator::poll().
   - Invoke the helper from the flush/management loop via the manager's runtime (spawn tasks or Handle::block_on where needed).
   - Ensure activation queues, drop events, and retention flows use the async drains.

3. **Thread the coordinator through Aof construction**
   - Register TieredInstance on Aof::new, wiring layout plus runtime-owned handles.
   - Guarantee coordinator lifetime covers all Tier0Instance/Tier1Instance users and shut down gracefully on drop.

4. **Adjust public APIs and tests**
   - Update any sync callers to await the new async poll/activation flows.
   - Migrate tier and AOF tests to run inside Tokio (or block_on) so new futures are exercised.

5. **Documentation & examples**
   - Document new configuration knobs and async behavior in the developer guide.
   - Refresh examples that construct AofManager to highlight tiered storage configuration.

### Task Tracking

- [ ] Extend AofManagerConfig and build caches/managers in AofManager.
- [ ] Add async service_tiered helper plus call sites awaiting coordinator polling.
- [ ] Pass tier handles through Aof::new/TieredInstance and ensure orderly shutdown.
- [ ] Update APIs/tests to await async tier operations; migrate affected tests to Tokio.
- [ ] Refresh documentation/examples describing async tier integration.

