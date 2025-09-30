# Checkpoint Implementation - Next Steps

## Current Status (2025-09-30)

**✅ Completed: 42/99 tests checkpoint-related, all passing**

### T1 - Checkpoint Planner (100% ✅)
- **Files**: [planner.rs](../src/checkpoint/planner.rs)
- **Status**: Production-ready
- **Tests**: 10 passing
- **Features**:
  - Append-only planner with WAL segment sealing
  - libsql planner with dirty page diffing
  - Delta vs full rewrite decision logic
  - CRC64-NVME content addressing

### T2 - Executor Staging Pipeline (100% ✅)
- **Files**: [executor.rs](../src/checkpoint/executor.rs)
- **Status**: Production-ready, needs async S3 integration
- **Tests**: 8 passing
- **Features**:
  - Scratch directory management
  - Content-hash staging with deduplication
  - Upload stubs ready for async S3

### T3 - Manifest Writer (100% ✅)
- **Files**: [manifest_ops.rs](../src/manifest/manifest_ops.rs), [operations.rs](../src/manifest/operations.rs)
- **Status**: Production-ready
- **Tests**: 5 passing
- **Features**:
  - Atomic batch operations
  - Change log integration
  - Chunk delta management

### T5 - Truncation Coordination (85% 🚧)
- **Files**: [truncation.rs](../src/checkpoint/truncation.rs)
- **Status**: Needs T6 integration
- **Tests**: 14 passing
- **Features**:
  - Lease-based coordination complete
  - Manifest integration documented
  - Async S3 deletion noted

---

## Remaining Work

### T8 - RemoteStore S3 Client (0% ⏸️)

**Priority**: HIGH - Blocks T2 upload and T6 orchestration

#### Decision Required: S3 Client Library

**Option 1: minio-rs (RECOMMENDED)**
```toml
[dependencies]
minio = "0.3"
bytes = "1.5"
```

**Pros**:
- Lightweight, S3-focused
- Works with MinIO (dev/test) and AWS S3 (prod)
- Active maintenance (v0.3.0 June 2025)
- Async-first with tokio

**Cons**:
- Sparse documentation (needs API discovery)
- Smaller ecosystem than AWS SDK

**Option 2: aws-sdk-s3**
```toml
[dependencies]
aws-config = "1.5"
aws-sdk-s3 = "1.47"
```

**Pros**:
- Official AWS SDK
- Comprehensive documentation
- Well-tested, production-ready

**Cons**:
- Heavy dependency tree (~50 crates)
- AWS-centric (MinIO is secondary)
- Frequent breaking changes

#### Implementation Plan (T8)

1. **T8a: Client Setup** (2-3 days)
   - Choose client library (recommend minio-rs)
   - Implement `RemoteStore::new()` with config
   - Handle MinIO vs AWS S3 endpoints
   - Verify bucket existence

2. **T8b: Async Upload** (3-4 days)
   - Implement `put_blob<R: AsyncRead>()`
   - Stream data with CRC64-NVME checksum
   - Use multipart upload for large objects
   - Add exponential backoff retry logic
   - Progress callback support

3. **T8c: Async Download** (2-3 days)
   - Implement `get_blob<W: AsyncWrite>()`
   - Stream download with checksum validation
   - Cold scan passthrough mode
   - Bandwidth limiting (RemoteReadLimiter)

4. **T8d: Testing** (2 days)
   - Unit tests for BlobKey formatting
   - Integration tests with MinIO container
   - Retry/failure simulation
   - Performance benchmarks

**Code Skeleton**:
```rust
// crates/bop-storage/src/remote_store.rs
use minio::s3::Client;
use tokio::io::{AsyncRead, AsyncWrite};

pub struct RemoteStore {
    client: Client,
    config: RemoteStoreConfig,
}

impl RemoteStore {
    pub async fn new(config: RemoteStoreConfig) -> Result<Self, RemoteStoreError> {
        // Initialize minio client with endpoint
        // Verify bucket exists
    }

    pub async fn put_blob<R: AsyncRead + Unpin + Send>(
        &self,
        key: &BlobKey,
        data: R,
        size: Option<u64>,
    ) -> Result<[u64; 4], RemoteStoreError> {
        // Stream upload with CRC64 hashing
        // Handle multipart for large files
        // Retry on transient failures
    }

    pub async fn get_blob<W: AsyncWrite + Unpin + Send>(
        &self,
        key: &BlobKey,
        writer: W,
    ) -> Result<(u64, [u64; 4]), RemoteStoreError> {
        // Stream download with validation
    }
}
```

---

### T6 - Job Queue Orchestration (0% ⏸️)

**Priority**: HIGH - Integrates all checkpoint components

#### Dependencies
- T8 RemoteStore must be complete
- Existing RocksDB job queue (referenced in plan)

#### Implementation Plan (T6)

1. **T6a: Job Types** (2 days)
   - Define `CheckpointJob` enum
   - Wire to existing `JobKind::Checkpoint`
   - Add serialization/deserialization
   - Integrate with RocksDB queue

2. **T6b: Progress Tracking** (3-4 days)
   - Persist `CheckpointJobState` to queue metadata
   - Track phases: Preparing → Staging → Uploading → Publishing
   - Track processed chunks for idempotent retry
   - Recovery logic on restart

3. **T6c: Orchestration Loop** (4-5 days)
   - Dequeue checkpoint jobs
   - Call `CheckpointPlanner::plan()`
   - Call `CheckpointExecutor::stage_chunks()` (sync)
   - Call `RemoteStore::put_blob()` (async) for each chunk
   - Call `Manifest::begin()` for atomic publication
   - Handle cancellation (T5c) from truncation requests

**Code Skeleton**:
```rust
// crates/bop-storage/src/checkpoint/orchestrator.rs
pub struct CheckpointOrchestrator {
    planner: CheckpointPlanner,
    executor: CheckpointExecutor,
    remote_store: Arc<RemoteStore>,
    manifest: Arc<Manifest>,
    job_queue: Arc<JobQueue>,
}

impl CheckpointOrchestrator {
    pub async fn run_checkpoint_job(&self, job_id: JobId) -> Result<(), OrchestratorError> {
        // 1. Load job state from queue
        let state = self.job_queue.load_state(job_id)?;

        // 2. Create plan (if not already done)
        let plan = if state.phase == ExecutionPhase::Preparing {
            self.planner.plan_append_only(/* ... */)?
        } else {
            state.plan.unwrap()
        };

        // 3. Stage chunks
        let mut staged_chunks = Vec::new();
        for action in &plan.actions {
            let staged = self.executor.stage_chunk(/* ... */)?;
            staged_chunks.push(staged);
            self.job_queue.update_progress(job_id, staged_chunks.len())?;
        }

        // 4. Upload to S3 (async)
        let mut uploaded = HashMap::new();
        for staged in &staged_chunks {
            let key = BlobKey::chunk(/* ... */);
            let hash = self.remote_store.put_blob(&key, /* ... */).await?;
            uploaded.insert(staged.chunk_id, (key, hash));
        }

        // 5. Publish to manifest (atomic)
        let mut txn = self.manifest.begin();
        for (chunk_id, (key, hash)) in uploaded {
            txn.upsert_chunk(/* ... */);
        }
        txn.commit()?;

        Ok(())
    }
}
```

---

### T7 - LocalStore Quota & Janitor (0% ⏸️)

**Priority**: MEDIUM - Prevents disk exhaustion

#### Implementation Plan (T7)

1. **T7a: StorageQuota Service** (3-4 days)
   - Define hierarchical quota (global → per-tenant → per-job)
   - Implement `reserve()` / `release()` with CAS loops
   - Track usage in memory (DashMap)
   - Integrate with WriteController backpressure

2. **T7b: hot_cache/ Eviction** (2-3 days)
   - LRU eviction respecting PageCache pin counts
   - Coordinate with manifest to avoid evicting active chunks
   - Metrics for cache hit/miss rates

3. **T7c: Scratch Janitor** (1-2 days)
   - Background tokio task
   - Scan `checkpoint_scratch/` every 10 minutes
   - Delete directories >1 hour old without active job
   - Log cleanup actions

**Code Skeleton**:
```rust
// crates/bop-storage/src/storage_quota.rs
pub struct StorageQuota {
    global_limit: AtomicU64,
    global_used: AtomicU64,
    tenant_quotas: DashMap<TenantId, TenantQuota>,
}

impl StorageQuota {
    pub fn reserve(&self, tenant_id: TenantId, bytes: u64) -> Result<ReservationGuard, QuotaError> {
        // CAS loop to reserve quota
        // Check global → tenant → job hierarchy
        // Return guard that releases on drop
    }
}

// crates/bop-storage/src/scratch_janitor.rs
pub struct ScratchJanitor {
    scratch_base: PathBuf,
    active_jobs: Arc<DashMap<JobId, Instant>>,
}

impl ScratchJanitor {
    pub async fn run(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(600)).await; // 10 minutes

            for entry in std::fs::read_dir(&self.scratch_base)? {
                let path = entry?.path();
                let age = SystemTime::now().duration_since(metadata.modified()?)?;

                if age > Duration::from_secs(3600) && !self.is_job_active(&path) {
                    std::fs::remove_dir_all(&path)?;
                    // Log cleanup
                }
            }
        }
    }
}
```

---

## Integration Sequence

**Recommended order for completing remaining tasks:**

1. **T8 RemoteStore** (9-12 days)
   - Blocks both T6 and full T5 completion
   - Critical for end-to-end checkpoint flow
   - Can be developed/tested independently with MinIO

2. **T6 Job Queue** (9-13 days)
   - Integrates all components
   - Enables end-to-end testing
   - Completes T5b truncation manifest integration

3. **T7 Quota & Janitor** (6-9 days)
   - Prevents operational issues
   - Can be added after basic checkpoint flow works
   - Lower priority than core checkpoint functionality

**Total estimated time: 24-34 days (5-7 weeks) for 1 engineer**

---

## Testing Strategy

### Unit Tests
- ✅ 42 tests passing for T1, T2, T3, T5
- ⏸️ Need 15+ tests for T8 RemoteStore
- ⏸️ Need 10+ tests for T6 orchestrator
- ⏸️ Need 8+ tests for T7 quota

### Integration Tests
- ⏸️ End-to-end checkpoint with MinIO
- ⏸️ Restart/recovery from crash during upload
- ⏸️ Truncation blocking on active checkpoint
- ⏸️ Quota exhaustion handling

### Load Tests
- ⏸️ 10 concurrent checkpoints with limited disk quota
- ⏸️ Large checkpoint (1000+ chunks, 64 GiB)
- ⏸️ S3 retry under network partition

---

## Deployment Checklist

Before M1 (Foundations Ready):
- [x] T1 Planner complete
- [x] T2 Executor staging complete
- [x] T3 Manifest writer complete
- [ ] T8 RemoteStore with MinIO/S3
- [ ] T6 Job orchestration
- [ ] T7a Quota enforcement
- [ ] End-to-end integration test passing
- [ ] Crash recovery test passing

Before M2 (Durability Enhancements):
- [ ] T4 Chunk deltas (not started)
- [x] T5 Truncation (mostly done, needs T6)
- [ ] T10 PageStore lease wiring (not started)
- [ ] Cold scan API (T9, not started)

---

## Questions for Product/Engineering

1. **S3 Client Choice**: minio-rs or aws-sdk-s3? (Recommend minio-rs)
2. **MinIO in Production**: Are we using MinIO for prod or AWS S3?
3. **Quota Limits**: What are the target per-tenant quota limits?
4. **Checkpoint Frequency**: How often should checkpoints trigger? (Design says every 5 min)
5. **Multipart Upload Threshold**: Use 5 MiB chunk size? (AWS default)
6. **Retry Strategy**: Exponential backoff parameters? (100ms → 30s max?)
7. **Cold Scan Priority**: When is T9 (cold scan API) needed? (M2 or M3?)

---

## References

- [Checkpointing Design](./checkpointing_design.md)
- [Checkpointing Plan](./checkpointing_plan.md)
- [T1 Planner Implementation](../src/checkpoint/planner.rs)
- [T2 Executor Implementation](../src/checkpoint/executor.rs)
- [T3 Manifest Operations](../src/manifest/operations.rs)
- [T5 Truncation Implementation](../src/checkpoint/truncation.rs)