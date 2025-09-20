# AOF2 Async Tiered Storage Design

## Guiding principles
- Preserve predictable crash recovery while enabling elastic capacity across local caches and remote object storage.
- Keep synchronous hot-path APIs (`append_record`, iterator-based readers) non-blocking; surface `WouldBlock` whenever progress requires background work.
- Pair each synchronous call with an async helper that awaits the Tokio runtime so callers can choose back-pressure strategy.
- Centralize background work inside `AofManager` so individual `Aof` instances remain lightweight and policy-free.
- Maintain deterministic on-disk layout: at most two segments may be unsealed at crash time, every sealed segment ends with a footer, and recovery never requires an external manifest.
- Treat the tiered store as the single source of truth for segment residency. No module may bypass it for ad-hoc filesystem access.

## External API surface

### Synchronous entrypoints (non-blocking)
- `Aof::append_record(&mut self, record) -> Result<(), AppendError>` returns `AppendError::WouldBlock` when Tier 0 lacks space, a rollover is pending, or a background task must progress first.
- `AofReader::next(&mut self) -> Result<Option<Record>, ReadError>` returns `ReadError::WouldBlock` while hydration or promotion is in flight.
- Management calls such as `Aof::flush` and `Aof::sync_after(offset)` remain synchronous but may also emit `WouldBlock`.

These APIs never spin or sleep; callers decide whether to retry, poll, or switch to async variants. Error enums reserve space for additional failure modes (I/O, validation, shutdown) so we can extend without breaking compatibility.

### Async companions
Every synchronous method above gains an async sibling on `AofAsync` (or via extension traits) that internally loops on the sync form, parking on condition variables/futures exposed by the tiered store. Examples:

- `async fn append_record_async(&mut self, record)`
- `async fn next_record_async(&mut self) -> Result<Option<Record>, ReadError>`
- `async fn flush_async(&self)`

Async helpers live close to the synchronous wrappers so they can share throttling and metrics. They always execute on the Tokio runtime owned by `AofManager`.

### Manager lifecycle
- `AofManager::with_config(config)` builds the Tokio runtime, tier caches, S3 client, and `TieredCoordinator`.
- `AofManager::open_aof(stream_id, options)` returns an `Aof` handle bound to the shared tiered store.
- Shutdown drops the manager, signals all coordinators, and awaits in-flight uploads/flushes.

`Aof` creation requires a reference/Arc to the manager; tests still may construct lightweight managers with in-memory tiers.

## Component architecture

### AofManager
- Owns the Tokio runtime (multi-threaded) and exposes `Handle` so synchronous callers can spawn async waits through helper APIs.
- Creates and wires `Tier0Cache`, `Tier1Store`, `Tier2Store`, and `TieredCoordinator` using configuration from `AofManagerConfig`.
- Runs service loops: tiered coordinator polling, eviction/compression pipelines, retention enforcement, metrics emission.
- Exposes observation hooks for tests (probe tier sizes, inject faults) guarded by cfg options.

### TieredCoordinator
- Bridges tier residency state with append/read demands.
- Consumes a queue of residency requests (e.g., promote segment to Tier 0, archive sealed segment) and drives them via async tasks.
- Maintains the replicated operation log (upload, delete, compaction instructions) and ensures operations are idempotent.
- Provides condition variables/futures for sync APIs to wait on (`tier0_ready`, `hydration_complete`).

### Tier caches
Refer to `store.md` for full details; at a high level:

- **Tier 0 (hot mmap)**: only location where appends occur. Keeps active writer segment plus recently accessed sealed segments. Exposes `AdmissionGuard` objects used by `Aof` append path. Evicts through an async queue when space exhausted.
- **Tier 1 (warm compressed)**: stores sealed segments as seekable Zstd. Handles compression, hydration back to Tier 0, and staging for remote uploads. Maintains manifest of checksums, offsets, residency state.
- **Tier 2 (S3/deep)**: master-only writes/uploads; all nodes can hydrate/download. Uses async S3 client with retries, checksums, and retention gating.

### Aof instance
- Holds stream-specific catalog (segment ids, logical offsets, checksums) persisted alongside tier metadata.
- Uses Tier 0 for append/resident reads; falls back to Tier 1/Tier 2 via coordinator-provided handles.
- Exposes synchronous and async APIs while deferring heavy work to the manager/coordinator.
- Participates in recovery by replaying catalog and requesting missing segments from tiers.

### Catalog and operation log
- Catalog tracks current head segment, sealed segments, and logical positions per stream.
- Operation log (replicated via Raft) lists tiered actions: seal, admit, promote, upload, delete, compaction. Followers apply log to converge caches.
- Catalog snapshots reference operation log watermark so recovery knows which entries are durable.

## Data and control flows

### Append path
1. Caller invokes `append_record`.
2. Append guard checks Tier 0 capacity; if insufficient, returns `WouldBlock` and signals coordinator to free space.
3. If capacity available, record is appended, segment footer/state updated atomically, and optional flush scheduling triggered.
4. Background: coordinator seals segments when they reach size/duration thresholds, enqueues compression/upload work.

Async append loops call `append_record` repeatedly, awaiting tier events between attempts.

### Read path
1. Reader requests records starting at offset.
2. Tier 0 cache hit returns data immediately.
3. On miss, coordinator initiates hydration from Tier 1 (or Tier 2). Sync reader receives `WouldBlock` until hydration completes.
4. Once hydration finishes, reader resumes without state loss.

Async readers await hydration futures; they can also opt into streaming decompress results while Tier 0 warms the mmap file.

### Eviction and promotion
- Tier 0 eviction triggers sealing (if active) and passes sealed segment descriptors to Tier 1.
- Tier 1 compression runs on the runtime blocking pool; results stored with checksum footers.
- Tier 2 uploads initiated only by the Raft master. Followers mark residency as `RemotePending` until log entry confirms completion.
- Promotions follow reverse path: Tier 1 decompress -> Tier 0 admission -> notify waiting readers.

### Recovery
- On startup, Tier 0 scans local segments, validating footers and trimming partial writes.
- Tier 1 rebuilds its manifest from headers/sidecar metadata; missing entries are reconciled by fetching from Tier 2 using the operation log.
- Tiered coordinator replays operation log to re-establish residency, ensuring no segment becomes active before dependencies restored.
- `Aof::recover` rehydrates catalog, schedules hydrations as needed, and unblocks append once head segment reopened.

### Compaction and retention
- Compaction planner (master) selects segment ranges, produces replacement artifacts directly in Tier 1/Tier 2, and emits delete operations into log.
- Retention policy enforces size/time constraints per tier. Deletes propagate through tiers, respecting reader fences.

## Concurrency and async model
- Tokio runtime drives all async work; blocking filesystem/compression tasks use `spawn_blocking`.
- Synchronous APIs coordinate via lock-free primitives plus async-aware condition variables; no method waits on a runtime from within a runtime thread.
- Shared state (catalog, manifest) protected by `parking_lot::RwLock` or sharded indices; we avoid `Mutex` inside hot loops.
- Background tasks are cancellable via `CancellationToken`s owned by manager so shutdown is orderly.

## Error handling and resiliency
- Synchronous errors distinguish between `WouldBlock`, permanent I/O failures, validation failures (checksum mismatch), and shutdown signals.
- Async helpers surface the same error enums; they retry only when internal policy deems safe (e.g., transient S3 errors with backoff).
- Tiered coordinator records failure context in the operation log; irrecoverable failures flip the stream into `NeedsRecovery` status to prevent silent data loss.
- Metrics and structured logs capture queue depths, retry counts, and latency histograms per tier.

## Observability
- Expose metrics via existing exporter: tier occupancy, promotion/eviction throughput, upload success/fail, hydrations waiting.
- Trace spans wrap append, hydration, upload, and recovery phases to aid debugging.
- Provide developer hooks to dump residency maps and operation log for a stream.

## Testing strategy
- Unit tests cover catalog transitions, `WouldBlock` conditions, and async helpers (with mocked futures).
- Integration tests (Tokio) simulate eviction, promotion, S3 outage, and recovery scenarios.
- Criterion benchmarks keep tracking append latency under contention, hydration throughput, and compaction cost.
- Property tests validate operation log idempotency and partial failure handling.

## Migration and compatibility
- Existing callers using synchronous APIs keep working, but must handle `WouldBlock` in more scenarios.
- Async integration requires updating constructors to receive `AofManager` references; helpers ease transition in tests by providing in-process runtimes.
- Documentation in `store.md` and `/docs` cross-references this design; implementation plan (see `implementation_plan.md`) tracks rollout progress and must be updated as tasks complete.
