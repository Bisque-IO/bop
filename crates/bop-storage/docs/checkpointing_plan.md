# Checkpointing Implementation Plan

## Table of Contents
- [Objectives](#objectives)
- [Scope](#scope)
- [Dependencies](#dependencies)
- [Current Implementation Status](#current-implementation-status)
- [Phased Delivery](#phased-delivery)
- [Task List](#task-list)
- [Task Tracking](#task-tracking)
- [Milestones](#milestones)
- [Definition of Done](#definition-of-done)
- [Verification Strategy](#verification-strategy)
- [Risks & Mitigations](#risks--mitigations)
- [Rollout & Ops](#rollout--ops)
- [Deferred / Future Work](#deferred--future-work)
- [Rules](#rules)

## Objectives
- Deliver the checkpointing system described in `checkpointing_design.md`, covering planner, executor, manifest integration, and retention flows.
- Ensure LMDB-backed manifests (`chunk_catalog`, `chunk_delta_index`, `change_log`) and S3 blobs stay strongly consistent under retries and restarts.
- Provide operational hooks for append-only truncation, cold scans, and manifest-driven job orchestration with clear observability.
- Keep PageStore, LocalStore, and RemoteStore interactions efficient while respecting quota and lease coordination rules.

## Scope
- **In scope**: Checkpoint planner/executor code paths, manifest writer batches, chunk delta handling, append-only truncation jobs, cold scan APIs, quota enforcement, GC integration, and monitoring.
- **Out of scope**: Non-checkpoint storage engines, libsql-specific planner optimisations beyond current design, UI tooling, or multi-tenant billing integrations.

## Dependencies
- Stable LMDB manifest APIs (`ManifestOp`, `ChunkEntryRecord`, `ChunkDeltaRecord`).
- S3 client abstractions with multipart upload + retry support.
- PageStore and PageCache primitives for lease tracking and dirty page enumeration.
- Job queue runtime capable of durable enqueue/ack (existing RocksDB-backed queue).

## Current Implementation Status

Based on codebase audit (as of 2025-09-30):

### ✅ Implemented Components
- **[Manifest](../src/manifest.rs)** (39K+ tokens): Core LMDB-backed manifest with catalog structures
- **[PageCache](../src/page_cache.rs)**: Global cache with observer hooks, eviction, and metrics (lines 1-544)
- **[WriteController](../src/write.rs)**: Write scheduling with quota enforcement and panic recovery (lines 262-348)
- **[WalSegment](../src/wal.rs)**: WAL segment management with staging and write batching
- **[PageStore](../src/db.rs)**: Basic page storage abstraction (line 276)
- **[FlushController](../src/flush.rs)**: Flush orchestration with backpressure

### 🚧 Partial / In Progress
- **Manifest Writer**: Core LMDB structures exist, but checkpoint-specific batch operations (chunk deltas, truncation) need integration
- **Quota Enforcement**: WriteController has inflight limits, but hierarchical StorageQuota service not yet implemented
- **PageCache Leasing**: Observer trait exists, but lease registration/coordination not wired

### ❌ Not Started
- **CheckpointPlanner**: No planner code found
- **CheckpointExecutor**: No executor scaffolding found
- **LocalStore**: No hot_cache/ or checkpoint_scratch/ management found
- **RemoteStore**: No S3 streaming wrapper found
- **Job Queue Integration**: No checkpoint job types defined
- **Cold Scan APIs**: No ColdScan read mode implemented
- **Truncation Jobs**: No truncation coordination logic found

## Phased Delivery
1. **Foundations** – planner/executor scaffolding, manifest writer plumbing, storage quota service.
2. **Durability Features** – chunk delta pipeline, append-only truncation jobs, LMDB change-log updates.
3. **Operational Enhancements** – cold scan read mode, metrics/logging, disaster-recovery validation, GC alignment.
4. **Hardening** – fault-injection tests, lease race tests, documentation, readiness review.

## Task List
1. Implement checkpoint planner split for append-only vs. libsql workloads.
2. Build checkpoint executor staging pipeline (scratch dirs, hash verification, upload flow).
3. Wire manifest writer batches for chunk catalog, delta index, WAL catalog, and change log updates.
4. Add chunk delta production/consumption (delta files, manifest ops, snapshot references).
5. Implement append-only truncation interface plus manifest job (`TruncateAppendOnly`).
6. Extend job queue runner with planner/executor/truncation orchestration + progress reporting.
7. Finalise LocalStore quota enforcement, `hot_cache/` eviction, and scratch janitor behaviour.
8. Wrap RemoteStore S3 streams with retry/backoff, checksum validation, and naming scheme.
9. Expose cold scan (`ColdScan`) read APIs and integrate `RemoteReadLimiter` controls.
10. Integrate PageStore leasing, checkpoint leases, and cache population policies.
11. Add failure-handling surface (idempotent retries, compensating changes, recovery paths).
12. Instrument metrics/logging, publish dashboards, and document operational runbooks.

## Task Tracking

**Legend**:
- **Effort**: S (Small: <3 days), M (Medium: 3-7 days), L (Large: >7 days)
- **Status**: ✅ DONE, 🚧 IN_PROGRESS, 📋 TODO, ⏸️ BLOCKED
- **%**: Estimated completion percentage

| ID | Task | Owner | Effort | Status | Deps | % | Code References | Acceptance Criteria |
|----|------|-------|--------|--------|------|---|-----------------|---------------------|
| **T1** | **Checkpoint planner split** | TBD | L | 📋 TODO | - | 0% | N/A (new file: `src/checkpoint/planner.rs`) | • Planner detects append-only vs libsql by WAL format metadata<br>• Emits `CheckpointPlan` struct with chunk actions<br>• Unit tests cover both workload types |
| T1a | Planner scaffolding & CheckpointPlan struct | - | M | 📋 TODO | - | 0% | - | Define `CheckpointPlan`, `ChunkAction`, `PlannerContext` types |
| T1b | Append-only planner (seal segment → chunk) | - | M | 📋 TODO | T1a | 0% | - | Can promote WalSegment to 64MiB chunk with checksum |
| T1c | libsql planner (diff pages, group by chunk) | - | L | 📋 TODO | T1a | 0% | - | Diffs dirty PageCache entries, emits delta vs full chunk decisions |
| **T2** | **Executor staging pipeline** | TBD | L | 📋 TODO | T1, T7 | 0% | N/A (new file: `src/checkpoint/executor.rs`) | • Stages files to `checkpoint_scratch/{job_id}/`<br>• Verifies CRC64-NVME checksums<br>• Handles upload failures with batch restore |
| T2a | Executor scaffolding & scratch directory mgmt | - | M | 📋 TODO | - | 0% | - | Creates/deletes job scratch dirs under LocalStore |
| T2b | Content-hash staging (deduplication support) | - | M | 📋 TODO | T2a | 0% | - | Writes pages in hash order, records footers |
| T2c | Upload coordination & validation | - | M | 📋 TODO | T2b, T8 | 0% | - | Calls RemoteStore.put_blob, verifies checksums before publication |
| **T3** | **Manifest writer batch operations** | TBD | M | 🚧 IN_PROGRESS | - | 40% | [manifest.rs](../src/manifest.rs) | • Supports atomic batch of `ManifestOp` values<br>• Appends to `change_log` in same transaction<br>• Integration test verifies LMDB rollback on failure |
| T3a | ManifestOp enum & batch submission API | - | S | 🚧 IN_PROGRESS | - | 70% | manifest.rs (exists but needs checkpoint ops) | Define UpsertChunk, DeleteChunk, UpsertChunkDelta, DeleteChunkDelta |
| T3b | LMDB transaction logic for checkpoint batches | - | M | 📋 TODO | T3a | 0% | - | Single txn writes chunk_catalog + chunk_delta_index + change_log |
| **T4** | **Chunk delta production & consumption** | TBD | L | 📋 TODO | T3 | 0% | N/A | • Delta files <10MB preferred over full 64MB rewrites<br>• PageStore can layer deltas over base chunks<br>• Coalescing triggered after 3+ deltas |
| T4a | Delta file format & staging | - | M | 📋 TODO | T2 | 0% | - | Executor emits delta chunks with RemoteObjectKind::Delta |
| T4b | chunk_delta_index integration | - | M | 📋 TODO | T3b | 0% | - | Manifest writer upserts/deletes delta records |
| T4c | PageStore delta layering (read path) | - | L | 📋 TODO | T10 | 0% | - | Compose latest view from base + delta chain |
| **T5** | **Append-only truncation jobs** | TBD | L | 📋 TODO | T1, T3 | 0% | N/A (new file: `src/checkpoint/truncation.rs`) | • Tail/head truncation commands accepted<br>• Lease coordination blocks truncation during checkpoint<br>• Manifest atomically removes chunk_catalog + wal_catalog entries |
| T5a | TruncateAppendOnly job type & lease map | - | M | 📋 TODO | - | 0% | - | Define job, maintain generation lease map in planner |
| T5b | Truncation manifest batch operations | - | M | 📋 TODO | T3b, T5a | 0% | - | DeleteChunk, DeleteChunkDelta, WAL catalog updates |
| T5c | Cancellation signal for in-flight checkpoints | - | M | 📋 TODO | T5a, T6 | 0% | - | Executor receives cancellation via job queue |
| **T6** | **Job queue orchestration** | TBD | M | 📋 TODO | T1, T2 | 0% | N/A | • CreateCheckpoint, TruncateAppendOnly jobs enqueue/dequeue<br>• Progress markers persist to RocksDB<br>• Crash recovery resumes from last marker |
| T6a | Checkpoint job types & queue integration | - | S | 📋 TODO | - | 0% | - | Define CreateCheckpoint, wire to existing RocksDB queue |
| T6b | Progress tracking & resume logic | - | M | 📋 TODO | T6a | 0% | - | Store (job_id, stage, progress) in queue metadata |
| **T7** | **LocalStore quota & janitor** | TBD | M | 🚧 IN_PROGRESS | - | 30% | [write.rs](../src/write.rs):270-348 | • Hierarchical quota (global, per-tenant, per-job)<br>• LRU eviction in hot_cache/ respects pin counts<br>• Janitor reclaims abandoned scratch dirs |
| T7a | StorageQuota service (hierarchical budgets) | - | M | 📋 TODO | - | 0% | N/A (new file: `src/storage_quota.rs`) | Reserve/release API, per-tenant/per-job accounting |
| T7b | hot_cache/ eviction with PageCache pin awareness | - | M | 📋 TODO | T10 | 0% | N/A (new file: `src/local_store.rs`) | Evicts unpinned files, retains manifest refs |
| T7c | Scratch janitor coroutine | - | S | 📋 TODO | T7a | 0% | - | Scans checkpoint_scratch/, deletes dirs >1hr old without active job |
| **T8** | **RemoteStore S3 streaming** | TBD | M | 📋 TODO | - | 0% | N/A (new file: `src/remote_store.rs`) | • put_blob/get_blob with multipart upload<br>• Retry with exponential backoff<br>• Checksum validation on upload/download |
| T8a | S3 client wrapper & blob naming scheme | - | M | 📋 TODO | - | 0% | - | Implement naming from design doc (chunk-{id}-{gen}-{hash}.blob) |
| T8b | Streaming upload with retry & checksum | - | M | 📋 TODO | T8a | 0% | - | Multipart upload, validate CRC64-NVME footer |
| T8c | Streaming download with passthrough mode | - | M | 📋 TODO | T8a | 0% | - | Support LocalStore write vs. cold scan passthrough |
| **T9** | **Cold scan API** | TBD | M | 📋 TODO | T8, T10 | 0% | N/A | • PageStore::read_page_trickle streams from S3<br>• RemoteReadLimiter enforces per-tenant bandwidth caps<br>• Metrics distinguish cold vs normal reads |
| T9a | ColdScanOptions & RemoteReadLimiter | - | S | 📋 TODO | - | 0% | - | Define throttling options, bandwidth limiter |
| T9b | PageStore cold read path integration | - | M | 📋 TODO | T9a, T8c | 0% | - | read_page_trickle bypasses LocalStore/PageCache |
| **T10** | **PageStore lease wiring** | TBD | M | 🚧 IN_PROGRESS | - | 20% | [page_cache.rs](../src/page_cache.rs):93-106 | • Checkpoint executor registers chunk leases<br>• PageCache observer hooks notify on eviction<br>• Cache population policy respects fetch hints |
| T10a | PageCache observer checkpoint hooks | - | M | 🚧 IN_PROGRESS | - | 40% | page_cache.rs (observer trait exists) | on_checkpoint_start/end hooks |
| T10b | Lease tracking in planner state | - | M | 📋 TODO | T1, T5a | 0% | - | Maintain lease map: chunk_id → (generation, job_id) |
| T10c | Cache policies (pin_in_cache, fetch hints) | - | S | 📋 TODO | T10a | 0% | - | FetchOptions struct, pin count management |
| **T11** | **Failure handling & idempotency** | TBD | L | 📋 TODO | T2, T3, T5 | 0% | N/A | • Upload failures restore batch to WalSegment<br>• Manifest publication retries are safe (hash-based keys)<br>• Fault injection tests pass (crash during upload, LMDB panic) |
| T11a | Idempotent retry logic in executor | - | M | 📋 TODO | T2 | 0% | - | Deterministic chunk ordering, hash-based blob keys |
| T11b | Compensating change log entries | - | M | 📋 TODO | T3, T5 | 0% | - | Cancellation records in change_log for truncation |
| T11c | Fault injection test suite | - | L | 📋 TODO | T11a, T11b | 0% | - | Chaos tests: upload failure, manifest writer panic, quota race |
| **T12** | **Observability & runbooks** | TBD | M | 📋 TODO | T11 | 0% | N/A | • Prometheus metrics for checkpoint duration, queue depth, quota<br>• Structured logs for job state transitions<br>• Runbook covers lease troubleshooting, truncation cancel |
| T12a | Metrics instrumentation | - | M | 📋 TODO | - | 0% | - | Implement metrics from design doc Observability section |
| T12b | Structured logging for checkpoint lifecycle | - | S | 📋 TODO | - | 0% | - | checkpoint.{started,staged,uploaded,published,failed} events |
| T12c | Runbooks & dashboards | - | M | 📋 TODO | T12a, T12b | 0% | - | Grafana dashboard, runbook for common scenarios |

## Milestones

### M1 – Foundations Ready
**Target**: End-to-end checkpoint flow operational
**Tasks**: T1 (Planner), T2 (Executor), T3 (Manifest writer), T7 (Quota), T8 (RemoteStore)
**Completion Criteria**:
- ✅ Can checkpoint a simple append-only tenant from WAL → staged chunks → S3 → manifest
- ✅ Quota enforcement prevents disk exhaustion
- ✅ Integration test demonstrates full pipeline
- ✅ No data loss on crash during staging (idempotent retry works)

**Estimated Duration**: 8-12 weeks (assuming 2 engineers)

---

### M2 – Durability Enhancements
**Target**: Production-grade durability features
**Tasks**: T4 (Chunk deltas), T5 (Truncation), T6 (Job orchestration), T10 (Lease wiring)
**Completion Criteria**:
- ✅ Delta files avoid full chunk rewrites for small updates
- ✅ Append-only truncation (head/tail) works without data corruption
- ✅ Lease coordination blocks conflicting truncation during checkpoint
- ✅ Job queue survives process restart and resumes
- ✅ PageCache observers notify checkpoint lifecycle events

**Estimated Duration**: 6-8 weeks

---

### M3 – Operational Readiness
**Target**: Production deployment ready
**Tasks**: T9 (Cold scan), T11 (Failure handling), T12 (Observability)
**Completion Criteria**:
- ✅ Cold scan API operational with bandwidth throttling
- ✅ Fault injection tests pass (20+ failure scenarios)
- ✅ Prometheus metrics exported, Grafana dashboard deployed
- ✅ Runbooks cover common operational scenarios
- ✅ Staged rollout plan approved by ops team

**Estimated Duration**: 4-6 weeks

---

**Total Estimated Timeline**: 18-26 weeks (4.5-6.5 months)

## Definition of Done

A task is considered **complete** when all of the following criteria are met:

1. ✅ **Implementation Merged**: Code reviewed by 2+ team members and merged to `main` branch
2. ✅ **Tests Passing**:
   - Unit tests achieve ≥80% line coverage for new code
   - Integration test(s) demonstrate the feature end-to-end
   - No flaky tests introduced (3 consecutive CI runs pass)
3. ✅ **Documentation Updated**:
   - Inline rustdoc comments for all public APIs
   - Design doc updated if implementation deviates from plan
   - Task Tracking table updated with completion status and notes
4. ✅ **Observability Instrumented**:
   - Relevant metrics added per `checkpointing_design.md` Observability section
   - Structured logs emit at appropriate points (started, completed, failed)
5. ✅ **Error Handling Validated**:
   - Failure modes explicitly tested (e.g., S3 timeout, disk full)
   - Error types properly propagated (no unwrap() in production paths)
6. ✅ **Performance Acceptable**:
   - No performance regressions on existing benchmarks
   - New code path meets latency/throughput targets (if defined)

**Special Cases**:
- For subtasks (e.g., T1a, T1b), completion requires parent task (T1) acceptance criteria
- For blocked tasks (⏸️), completion blocked until dependency resolves

## Verification Strategy

### Unit Tests

Test coverage for each component:

**Planner (T1)**:
- `test_append_only_plan_seals_wal_segment`: Planner converts 64MiB WalSegment to ChunkAction::SealAsBase
- `test_libsql_plan_emits_deltas`: Planner groups dirty pages by chunk, emits delta when <10MB changed
- `test_libsql_plan_full_rewrite_threshold`: Planner triggers full chunk rewrite when >50% pages dirty
- `test_plan_deterministic_ordering`: Same input produces identical CheckpointPlan (for idempotency)

**Manifest Writer (T3)**:
- `test_batch_atomic_commit`: All ManifestOp values commit in single LMDB txn
- `test_batch_rollback_on_failure`: Partial batch failure leaves manifest unchanged
- `test_change_log_append`: Each batch appends ManifestChangeRecord with correct generation
- `test_chunk_delta_upsert_delete`: UpsertChunkDelta/DeleteChunkDelta update chunk_delta_index

**Chunk Deltas (T4)**:
- `test_delta_file_format`: Delta chunk encodes only changed pages with correct offsets
- `test_delta_layering_read_path`: PageStore composes base + delta chain correctly
- `test_coalescing_trigger`: 3+ deltas trigger full chunk rewrite

**Truncation (T5)**:
- `test_tail_truncation_lease_blocking`: Truncation waits for active checkpoint lease
- `test_head_truncation_manifest_atomicity`: DeleteChunk + WAL updates in single txn
- `test_cancellation_compensating_log`: Cancelled checkpoint emits change_log entry

**Quota (T7)**:
- `test_hierarchical_reservation`: Per-job reservation deducts from per-tenant and global budgets
- `test_reservation_release_on_failure`: Failed job releases quota without leak
- `test_quota_exhaustion_backpressure`: Enqueue returns WriteScheduleError::Backpressure

### Integration Tests

End-to-end scenarios using temporary LMDB + S3 mock (Minio or custom fake):

1. **`test_checkpoint_append_only_roundtrip`**:
   - Create append-only DB, write 64MiB of records to WAL
   - Trigger checkpoint via API
   - Verify S3 contains chunk blob, LMDB has chunk_catalog entry
   - Restart process, verify recovery reads from checkpoint

2. **`test_checkpoint_libsql_delta`**:
   - Create libsql DB with 10 chunks, modify 100 pages in chunk 3
   - Trigger checkpoint
   - Verify S3 contains delta blob (<1MB), chunk_delta_index updated
   - Read modified pages via PageStore, verify correct values

3. **`test_truncation_blocks_on_lease`**:
   - Start checkpoint for chunk 5 (acquires lease)
   - Issue tail truncation for chunk 5
   - Verify truncation waits (polls lease map)
   - Complete checkpoint
   - Verify truncation proceeds after lease release

4. **`test_job_queue_resume_after_crash`**:
   - Start checkpoint, crash during upload (before manifest publication)
   - Restart process
   - Verify job queue resumes from staging phase
   - Complete upload, verify manifest eventually published

5. **`test_cold_scan_bypasses_cache`**:
   - Checkpoint tenant A with 100 chunks
   - Trigger cold scan read of 50 chunks
   - Verify PageCache size unchanged (no eviction pressure)
   - Verify RemoteReadLimiter throttles bandwidth

### Fault Injection Tests

Chaos scenarios using failure injection framework:

| Scenario | Injection Point | Expected Behavior |
|----------|----------------|-------------------|
| S3 upload timeout | RemoteStore.put_blob | Executor retries with backoff, eventually succeeds or fails gracefully |
| Manifest writer panic | LMDB txn commit | Quota released, job marked failed, no partial manifest update |
| Disk full during staging | checkpoint_scratch write | Executor fails early, releases quota, no orphaned scratch dirs |
| Network partition during lease check | Planner.check_lease | Truncation times out (30s), returns error without corrupting manifest |
| Concurrent quota exhaustion | Multiple jobs reserve simultaneously | CAS loop prevents overcommit, backpressure returned to callers |
| Process crash during LMDB txn | Mid-transaction kill -9 | LMDB rollback on restart, change_log consistent |

**Target**: 20+ failure scenarios passing before M3 completion

### Load Tests

Workload simulations for performance validation:

- **Mixed workload**: 10 append-only tenants + 5 libsql tenants, checkpoint every 5 minutes
  - Target: p99 checkpoint duration <60s, queue depth <5
- **Quota pressure**: 50 concurrent checkpoints with limited disk quota
  - Target: Backpressure engages gracefully, no crashes, quota accounting accurate
- **Cold scan at scale**: Analytical query scans 10GB across 1000 chunks
  - Target: RemoteReadLimiter keeps bandwidth <500MB/s, no PageCache thrashing

## Risks & Mitigations

| Risk | Impact | Probability | Mitigation | Owner | Status |
|------|--------|-------------|------------|-------|--------|
| **LMDB lock contention under high write load** | High (blocks manifest publication) | Medium | • Benchmark with simulated load (1000 writes/sec)<br>• Consider read-only replica for queries<br>• Profile lock hold times, optimize txn batching | TBD | Open |
| **S3 eventual consistency breaks lease coordination** | High (data corruption) | Low | • Use S3 conditional puts with ETag for lease state<br>• Add retry with exponential backoff<br>• Document eventual consistency assumptions | TBD | Open |
| **Quota enforcement race conditions** | Medium (disk exhaustion) | Medium | • Use CAS loops for reservation<br>• Add integration tests for concurrent reservation<br>• Monitor quota accounting drift in production | TBD | Open |
| **Checkpoint duration exceeds WAL segment lifetime** | High (WAL pressure, OOM) | Medium | • Set checkpoint trigger threshold <50% WAL capacity<br>• Add backpressure to write path when queue depth >10<br>• Alert on checkpoint duration >5min | TBD | Open |
| **Delta coalescing threshold tuning** | Medium (perf degradation) | High | • Start conservative (3 deltas → rewrite)<br>• Add metrics for delta chain length<br>• Plan for adaptive tuning based on access patterns | TBD | Open |
| **Cold scan bandwidth starvation of foreground traffic** | High (user-facing latency) | Medium | • Enforce per-tenant bandwidth limits<br>• Deprioritize cold scan requests in RemoteReadLimiter<br>• Add circuit breaker if p99 read latency >100ms | TBD | Open |
| **Truncation cancels checkpoint with unrecoverable state** | Critical (data loss) | Low | • Executor flushes staged data before cancellation<br>• Lease timeout >2x typical checkpoint duration<br>• Add manual recovery runbook for stuck leases | TBD | Open |
| **Manifest schema evolution breaks backward compatibility** | High (rollback impossible) | Medium | • Version manifest records with schema_version field<br>• Maintain forward/backward compat for 2 versions<br>• Require schema migration plan before deployment | TBD | Open |
| **S3 rate limiting causes cascading checkpoint failures** | Medium (backlog) | Medium | • Implement exponential backoff with jitter<br>• Spread checkpoint triggers across tenants<br>• Monitor S3 429 rate, alert if >5% of requests | TBD | Open |
| **Dependency delays (manifest.rs work)** | Medium (timeline slip) | High | • Phase 0: Stabilize manifest APIs before checkpoint work<br>• Weekly sync meetings with manifest team<br>• Prototype with stub manifest early to unblock | TBD | Open |

## Rollout & Ops

### Feature Flags

Control rollout risk with per-tenant feature flags:

```rust
pub struct CheckpointFeatureFlags {
    pub enable_checkpoints: bool,           // Master kill switch
    pub enable_chunk_deltas: bool,          // M2: Delta optimization
    pub enable_append_only_truncation: bool, // M2: Truncation operations
    pub enable_cold_scan: bool,             // M3: Cold read mode
    pub checkpoint_interval_seconds: u64,   // Tunable trigger frequency
}
```

### Staged Rollout Plan

| Stage | Scope | Duration | Success Criteria | Rollback Trigger |
|-------|-------|----------|------------------|------------------|
| **1. Dev/Test** | Internal test cluster (5 synthetic tenants) | 1 week | • All integration tests pass<br>• No crashes in 24hr soak test<br>• Checkpoint duration <30s | Any data corruption |
| **2. Canary** | 2-3 friendly customer tenants | 2 weeks | • Zero data loss incidents<br>• Checkpoint success rate >99%<br>• No user-reported performance degradation | >1 data integrity issue<br>OR success rate <95% |
| **3. Beta** | 10% of production tenants (opt-in) | 4 weeks | • p99 checkpoint duration <60s<br>• Quota exhaustion <0.1% of attempts<br>• S3 cost within budget (+20% baseline) | Widespread failures (>5 tenants)<br>OR cost overrun >50% |
| **4. GA** | All tenants (gradual 10%/day) | 2 weeks | • Metrics stable across rollout<br>• Alert noise <5 pages/week<br>• Team confident in runbooks | System-wide outage<br>OR data loss affecting >1 tenant |

**Rollback Procedure**:
1. Set `enable_checkpoints=false` via feature flag
2. Drain existing job queue (wait for in-flight checkpoints to complete)
3. Verify WAL-based recovery works (run test suite against prod snapshot)
4. Collect diagnostics (logs, metrics, LMDB dumps) for postmortem

### Success Metrics

Track these during rollout to validate health:

| Metric | Target | Alert Threshold |
|--------|--------|----------------|
| `checkpoint_success_rate` | >99% | <95% over 1hr window |
| `checkpoint_duration_seconds{p99}` | <60s | >120s over 30min window |
| `checkpoint_queue_depth` | <5 | >20 sustained for 10min |
| `storage_quota_exhausted_total` (rate) | <0.1% | >1% of enqueue attempts |
| `page_fetch_latency_seconds{tier=Remote, p99}` | <500ms | >1s over 15min window |
| `manifest_write_errors_total` (rate) | 0 | >1/hour |
| `cold_scan_bandwidth_bytes{p99}` | <500MB/s per tenant | >1GB/s (indicates limiter failure) |

### Operational Runbooks

Document procedures for common scenarios (see T12c):

1. **Stuck Checkpoint (lease not releasing)**:
   - Check executor logs for panic/hang
   - Query lease map via debug endpoint: `GET /debug/checkpoint/leases`
   - Manual lease release: `POST /admin/checkpoint/lease/release/{chunk_id}` (requires approval)
   - Verify no data corruption after forced release

2. **Quota Exhaustion Loop**:
   - Check `storage_quota_snapshot` for per-tenant usage
   - Identify largest checkpoint_scratch/ dirs: `du -sh checkpoint_scratch/*`
   - Emergency cleanup: Run janitor manually, increase global quota temporarily
   - Long-term: Adjust per-tenant quotas or scale disk capacity

3. **Truncation Blocked Indefinitely**:
   - Check for long-running checkpoint: `GET /debug/checkpoint/active_jobs`
   - If checkpoint stuck, follow "Stuck Checkpoint" runbook first
   - Retry truncation after lease clears
   - If repeated failures, escalate to engineering (potential lease logic bug)

4. **S3 Upload Failures Cascade**:
   - Check S3 service health & rate limits
   - Verify IAM permissions & bucket policies
   - Temporarily reduce checkpoint frequency (increase `checkpoint_interval_seconds`)
   - Monitor S3 429 rate, engage AWS support if persistent

5. **Manifest Corruption Detected**:
   - **CRITICAL**: Immediately set `enable_checkpoints=false`
   - Take LMDB backup: `cp -r manifest.lmdb manifest.lmdb.backup`
   - Export change_log: `GET /debug/manifest/change_log?limit=1000`
   - Page on-call engineering lead
   - Do NOT attempt repair without engineering review

## Deferred / Future Work

Items explicitly out of scope for initial release (M1-M3), tracked for future iterations:

### Performance Optimizations

**Adaptive Blob Coalescing** (from `checkpointing_design.md` Q1)
- **Description**: Dynamically adjust page coalescing based on access patterns
- **Rationale**: Current design uses fixed 64MiB chunks; workloads with random access may benefit from smaller blobs
- **Dependencies**: Production access pattern data (3+ months of telemetry)
- **Estimated Effort**: 4-6 weeks
- **Priority**: Medium (only if p99 fetch latency >500ms observed)

**Differential Manifest Snapshots** (from `checkpointing_design.md` Q2)
- **Description**: Layer differential catalog updates over base manifest snapshots
- **Rationale**: Full LMDB table scans become expensive at >10K chunks per tenant
- **Dependencies**: Profiling with >1TB dataset, LMDB compaction analysis
- **Estimated Effort**: 6-8 weeks
- **Priority**: Medium (defer until >5TB tenants exist)

**Parallel Chunk Staging**
- **Description**: Stage multiple chunks concurrently during checkpoint execution
- **Rationale**: Current design stages sequentially; large checkpoints could parallelize
- **Dependencies**: M1 completion, quota accounting race testing
- **Estimated Effort**: 2-3 weeks
- **Priority**: Low (only if checkpoint duration >5min observed)

### Operational Features

**Checkpoint Verification Service**
- **Description**: Background job that re-downloads checkpoints and validates checksums
- **Rationale**: Detects S3 bit rot or upload corruption proactively
- **Dependencies**: M1 completion, separate quota pool decision (Q3 from design doc)
- **Estimated Effort**: 3-4 weeks
- **Priority**: Medium (required for compliance certifications)

**Cross-Region Replication**
- **Description**: Replicate checkpoints to secondary S3 region for disaster recovery
- **Rationale**: Enables fast failover for multi-region deployments
- **Dependencies**: M2 completion, S3 cross-region transfer cost analysis
- **Estimated Effort**: 6-8 weeks
- **Priority**: Low (enterprise feature, demand unclear)

**Checkpoint Compression**
- **Description**: Apply zstd or lz4 compression to staged chunks before S3 upload
- **Rationale**: Reduces S3 storage cost (20-40% typical) and transfer time
- **Dependencies**: M1 completion, CPU overhead benchmarking
- **Estimated Effort**: 2-3 weeks
- **Priority**: Medium (if S3 costs >$10K/month)

### Developer Experience

**Checkpoint CLI Tool**
- **Description**: `bop-checkpoint` CLI for manual checkpoint triggering, inspection, debugging
- **Rationale**: Simplifies testing and ops troubleshooting
- **Dependencies**: M2 completion
- **Estimated Effort**: 2 weeks
- **Priority**: Low (nice-to-have, not blocking)

**Grafana Dashboard Templates**
- **Description**: Pre-built Grafana dashboards for checkpoint health monitoring
- **Rationale**: Speeds up monitoring setup for new deployments
- **Dependencies**: M3 completion (T12a metrics instrumented)
- **Estimated Effort**: 1 week
- **Priority**: Medium (included in T12c, but templates deferred)

### Research / Prototypes

**Page-Level Deduplication**
- **Description**: Deduplicate identical pages across chunks within a tenant
- **Rationale**: Some workloads have high redundancy (e.g., default values, templates)
- **Dependencies**: Content-addressed storage prototype
- **Estimated Effort**: 8-12 weeks (research + implementation)
- **Priority**: Low (speculative, needs workload analysis)

**Incremental Manifest GC**
- **Description**: Stream GC decisions through change_log instead of full-table scans
- **Rationale**: Reduces GC latency for large tenants
- **Dependencies**: M2 completion, differential manifest work
- **Estimated Effort**: 4-6 weeks
- **Priority**: Low (only if GC becomes bottleneck)

---

**Review Cadence**: Revisit this list quarterly after M3 completion to re-prioritize based on production data.

## Rules

### Task Execution
- After completing any task, immediately update the Task Tracking table (Status, %, Notes) before starting new work.
- Record test coverage (automated or manual validation) in the Notes column for each completed task.
- Mark subtasks complete only after parent task acceptance criteria validated.

### Code Quality
- Preserve terminology and naming from `checkpointing_design.md` when adding code or documentation references.
- Avoid Unicode art/emojis in code comments unless file already uses them (docs are fine).
- All public APIs require rustdoc comments with examples.
- No `unwrap()` or `expect()` in production code paths (tests OK).

### Communication
- Surface any unexpected manifest/schema change requirements to the team via Slack #bop-storage channel before implementation continues.
- Update this plan document when scope changes (new tasks, milestone slip, risk materialization).
- Notify team lead when task status changes to ⏸️ BLOCKED (include blocking reason in Notes).

### Documentation
- Update `checkpointing_design.md` if implementation deviates from design (keep docs in sync).
- Add code references to Task Tracking table when implementation files are created.
- When task completes, add link to merged PR in Notes column.
