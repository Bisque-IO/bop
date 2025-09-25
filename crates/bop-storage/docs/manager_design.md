# Manager Design

## ManifestDB Overview
- Single LMDB environment opened through `heed` with `NOSUBDIR | SYNC_DURABLE` flags; the manager owns the unique write transaction while readers clone `RoTxn`s as needed.
- Environment sized for manifest metadata only (no user payloads). Map size configured from options and auto-resized up to a ceiling when free headroom drops below a threshold.
- All durable state for the manager/orchestrator flows through this manifest: DB definitions, WAL progress, chunk catalogue, job queue + per-DB single-flight indices, metrics, and generation watermarks.

## Schema

### Serialization Conventions
- Keys use `heed::types::Bytes` with fixed-width little-endian encodings so lexicographical order matches the natural tuple ordering. Composite keys are represented by `#[repr(C)]` structs packed without padding and encoded through `bytemuck`.
- Values use `SerdeBincode<T>` with an explicit `record_version: u16` prefix. `bincode` is configured via `DefaultOptions::new().with_fixint_encoding().reject_trailing_bytes()` to guarantee deterministic layouts across platforms.
- Every record struct below includes a `record_version` field; bump it on incompatible schema changes while keeping deserializers tolerant of previous variants.

### Tables

#### `db`
- Key: `DbKey = u64` (monotonic `DbId`).
- Value: `DbDescriptorRecord`.

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct DbDescriptorRecord {
    pub record_version: u16,
    pub created_at_epoch_ms: u64,
    pub name: String,
    pub config_hash: [u8; 32],
    pub wal_shards: u16,
    pub options: ManifestDbOptions,
    pub status: DbLifecycle,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestDbOptions {
    pub wal_dir: PathBuf,
    pub chunk_dir: PathBuf,
    pub compression: CompressionConfig,
    pub encryption: Option<EncryptionConfig>,
    pub retention: RetentionPolicy,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DbLifecycle {
    Active,
    Draining,
    Tombstoned { tombstoned_at_epoch_ms: u64 },
}
```

#### `wal_state`
- Key: `WalStateKey { db_id: u64, shard: u16 }`.
- Value: `WalStateRecord`.

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct WalStateKey {
    pub db_id: u64,
    pub shard: u16,
    pub _pad: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WalStateRecord {
    pub record_version: u16,
    pub last_sealed_segment: u64,
    pub last_applied_lsn: u64,
    pub last_checkpoint_generation: u64,
    pub restart_epoch: u32,
    pub flush_gate_state: FlushGateState,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FlushGateState {
    pub active_job_id: Option<JobId>,
    pub last_success_at_epoch_ms: Option<u64>,
    pub errored_since_epoch_ms: Option<u64>,
}
```

#### `chunk_catalog`
- Key: `ChunkKey { db_id: u64, chunk_id: u64 }`.
- Value: `ChunkEntryRecord`.

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ChunkKey {
    pub db_id: u64,
    pub chunk_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkEntryRecord {
    pub record_version: u16,
    pub file_name: ArrayString<120>,
    pub generation: u64,
    pub size_bytes: u64,
    pub compression: CompressionCodec,
    pub encrypted: bool,
    pub residency: ChunkResidency,
    pub wal_range: (u64, u64),
    pub uploaded_at_epoch_ms: Option<u64>,
    pub purge_after_epoch_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ChunkResidency {
    Local,
    RemoteArchived { object: RemoteObjectKey, verified: bool },
    Evicted,
}
```

#### `job_queue`
- Key: `JobId = u64` (allocated by manifest).
- Value: `JobRecord`.

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct JobRecord {
    pub record_version: u16,
    pub job_id: u64,
    pub created_at_epoch_ms: u64,
    pub db_id: Option<u64>,
    pub kind: JobKind,
    pub state: JobDurableState,
    pub payload: JobPayload,
    pub last_error: Option<JobErrorSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum JobDurableState {
    Pending,
    InFlight { worker_id: WorkerId, leased_at_ms: u64 },
    Completed { finished_at_ms: u64 },
    FailedPermanent { finished_at_ms: u64 },
}
```

#### `job_pending_index`
- Key: `PendingJobKey { db_id: u64, kind: PendingJobKind }`.
- Value: `PendingJobValue = u64` (the `JobId`).

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PendingJobKey {
    pub db_id: u64,
    pub kind: PendingJobKind,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PendingJobKind(pub u16);
```

The index enforces the per-DB `UniqueJobGate`: when a job transitions out of `Pending`, the writer removes the index row; re-enqueueing inserts a new row if none exists.

#### `metrics`
- Key: `MetricKey { scope: u32, kind: u32 }`.
- Value: `MetricRecord`.

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct MetricKey {
    pub scope: u32,    // e.g. DB id or global scope id
    pub kind: u32,     // stable metric identifier
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricRecord {
    pub record_version: u16,
    pub sum: f64,
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub bucket_counts: SmallVec<[u32; 8]>,
    pub updated_at_epoch_ms: u64,
}
```

#### `generation_watermarks`
- Key: `ComponentId = u32` (enumerates logical subsystems: manifest, wal, orchestrator, etc.).
- Value: `GenerationRecord`.

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct GenerationRecord {
    pub record_version: u16,
    pub current_generation: u64,
    pub last_committed_at_epoch_ms: u64,
    pub blocked_waiters: Vec<GenerationWaiterSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerationWaiterSnapshot {
    pub waiter_id: WaiterId,
    pub target_generation: u64,
    pub deadline_epoch_ms: u64,
}
```

## Manifest Writer Thread Protocol
- All writes go through a dedicated `ManifestWriter` thread that owns a long-lived `heed::RwTxn` scope reused per batch to minimize creation costs.
- The orchestrator submits `ManifestRequest` items through a bounded MPSC channel:

```rust
enum ManifestRequest {
    Apply(ManifestBatch),
    WaitForGeneration { component: ComponentId, target: u64, waiter: WaiterHandle },
    Sync { responder: oneshot::Sender<Result<u64>> },
    Shutdown,
}

struct ManifestBatch {
    generation_hint: Option<u64>,
    operations: SmallVec<[ManifestOp; 12]>,
    completion: oneshot::Sender<Result<BatchAck>>,   // returns committed generation
}

enum ManifestOp {
    PutDb(DbDescriptorRecord),
    PutWalState { key: WalStateKey, value: WalStateRecord },
    DeleteWalState(WalStateKey),
    UpsertChunk { key: ChunkKey, value: ChunkEntryRecord },
    DeleteChunk(ChunkKey),
    PutJob(JobRecord),
    UpdateJobState { job_id: JobId, new_state: JobDurableState },
    RemoveJob(JobId),
    UpsertPendingIndex { key: PendingJobKey, job_id: JobId },
    DeletePendingIndex(PendingJobKey),
    MergeMetric { key: MetricKey, delta: MetricDelta },
    BumpGeneration { component: ComponentId, increment: u64 },
}
```

- The writer thread drains the channel with `recv_deadline = now + commit_latency_budget`. It coalesces consecutive `Apply` batches into a single LMDB write transaction, applying operations in order to maintain manifest invariants (e.g., metrics merges before generation bumps).
- Commit path:
  1. Begin `RwTxn`.
  2. Apply all ops; convert errors into `ManifestError::Apply` and abort the txn.
  3. Update `generation_watermarks` for any `BumpGeneration` ops or implicit commits.
  4. Commit; on success, compute the resulting generation (max across components in the batch) and resolve completions.
  5. After commit, fulfill `WaitForGeneration` requests by checking the min-heap of waiters (`BinaryHeap` keyed by `target_generation`). Waiters whose target `<= committed_generation(component)` are notified; timed-out waiters return `Err(WaitExpired)`.
- Error handling: transaction failures (MDBX map full, etc.) trigger backoff and optional map resize requests routed back to orchestrator; repeated failures propagate to requesters.
- Shutdown: once `Shutdown` is received and the input channel is empty, the thread flushes any open transaction (abort if not committed), notifies pending waiters with `Err(Shutdown)`, and exits.

## Public Manifest API

### Opening
- `Manifest::open(path: &Path, options: ManifestOptions) -> Result<Self>` sets environment size, maximum DB count, and installs typed database handles for the tables above. It also runs migrations by iterating tables and upgrading old `record_version`s.

### Transactions
- `Manifest::begin_write(batch_hint: usize) -> Result<ManifestTxn>` returns a handle that buffers `ManifestOp`s without hitting LMDB until `commit`.
- `ManifestTxn::commit(self) -> Result<CommitReceipt>` enqueues the buffered operations as a `ManifestBatch` and awaits completion from the writer thread.
- `Manifest::read<T, F>(&self, f: F) -> Result<T>` provides a read-only view, cloning a `RoTxn` and executing the user callback.

### CRUD Helpers
- `ManifestTxn::put_db(&mut self, descriptor: DbDescriptorRecord)`.
- `ManifestTxn::put_wal_state(&mut self, key: WalStateKey, value: WalStateRecord)` and `delete_wal_state`.
- `ManifestTxn::upsert_chunk`, `delete_chunk`, `scan_chunks(db_id)` returning an iterator via cursors.
- `ManifestTxn::enqueue_job(record: JobRecord)` returns `JobId`; also updates `job_pending_index` when applicable.
- `ManifestTxn::update_job_state(job_id, new_state)` ensures invariants (e.g., removing from pending index when transitioning out of `Pending`).
- Metric helpers: `merge_metric(key, delta)` computing aggregated moments on write.

### Generation Helpers
- `ManifestTxn::bump_generation(component, increment)` and `Manifest::wait_for_generation(component, target, deadline)` bridging to the waiter heap.
- `Manifest::reserve_job_id()` fetches and increments the job id counter stored in the manifest (e.g., in `generation_watermarks` under a dedicated `ComponentId::JobIds`).

### Waiter API
- `Manifest::register_waiter(component, target, deadline) -> WaitHandle` sends a `WaitForGeneration` request; the handle resolves when the writer commits past the target or deadline lapses.

## UniqueJobGate Integration
- Each DB owns a `UniqueJobGate` that guards mutually-exclusive work kinds (flush, checkpoint, compaction). The durable state lives in two places:
  - `WalStateRecord.flush_gate_state.active_job_id` records which job currently holds the gate, ensuring recovery sees the in-flight job.
  - `job_pending_index` guarantees at most one pending job per `(DB, kind)` pair. Enqueuing a job first attempts to insert into the index; if an entry exists, the enqueue returns `UniqueJobAlreadyPending` and the gate remains closed.
- When a worker acquires the gate:
  1. It transitions the job to `InFlight` via a manifest transaction that also updates `flush_gate_state.active_job_id`.
  2. On completion, it clears the gate state and removes the pending index entry (if any) in the same commit.
- If a worker crashes mid-flight, recovery sees `active_job_id` in `WalStateRecord` and re-enqueues a repair job while keeping the gate closed until the repair succeeds.

## Recovery Workflow
1. Open the manifest environment and run migrations (ensuring `record_version` compatibility).
2. Replay all `db` descriptors to rebuild the in-memory `DbRegistry` with configuration and lifecycle state.
3. For each DB, scan `wal_state` ordered by shard, reconstructing WAL checkpoints, restart epochs, and rehydrate the `UniqueJobGate` using the persisted `flush_gate_state`.
4. Scan `chunk_catalog` per DB to rebuild the chunk inventory; stage remote-archived chunks for verification or rehydration based on `ChunkResidency`.
5. Rebuild the durable job queue by iterating `job_queue` ordered by `JobId`. Pending and in-flight jobs are re-enqueued to the orchestrator, respecting `job_pending_index` to prevent duplicates. In-flight jobs whose lease has expired trigger compensating jobs.
6. Restore metrics by loading `metrics` into the in-memory aggregator so telemetry starts from the last committed counters.
7. Load `generation_watermarks` to initialize the generation clock and repopulate waiter state. Any waiters persisted in the snapshot are compared against the live orchestrator waitlist; stale ones are dropped or reattached depending on deadlines.
8. Once state is rehydrated, the orchestrator spawns the manifest writer thread, drains any queued recovery jobs, and transitions DBs to active service.

## Interaction Summary
- The orchestrator and manifest writer form a single-writer pipeline that batches durable updates while allowing callers to await generation milestones.
- Callers interact with the manifest exclusively through the typed API, ensuring all record structs and invariants stay centralized and consistent.
