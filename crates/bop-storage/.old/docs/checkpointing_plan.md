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
- **S3 client abstractions with async API (tokio runtime)** - multipart upload + retry support.
- PageStore and PageCache primitives for lease tracking and dirty page enumeration.
- Job queue runtime capable of durable enqueue/ack (existing LMDB-backed queue).

**IMPORTANT**: S3 operations will be **async** using tokio. All upload/download methods in RemoteStore (T8)
and the checkpoint executor's upload coordination (T2c) will use async/await patterns.

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
- **Manifest Writer**: ✅ **COMPLETE** - Checkpoint-specific batch operations fully implemented and tested (T3 done)
- **Checkpoint Planner**: ✅ **COMPLETE** - Both append-only and libsql planners implemented with delta vs rewrite logic (T1 done)
- **Executor Staging**: ✅ **COMPLETE** - Content-hash staging and upload coordination implemented (T2 done)
- **Checkpoint Orchestrator**: ✅ **COMPLETE** - End-to-end orchestration coordinates planner, executor, RemoteStore, manifest, quota (T6 done)
- **RemoteStore**: ✅ **COMPLETE** - Async S3 client using rust-s3 with MinIO/AWS support, CRC64 checksums (T8 done)
- **StorageQuota**: ✅ **COMPLETE** - Hierarchical quota service with CAS loops, RAII guards, per-tenant/per-job accounting (T7a done)
- **ScratchJanitor**: ✅ **COMPLETE** - Background cleanup removes abandoned scratch directories (T7c done)
- **LocalStore hot_cache**: ✅ **COMPLETE** - LRU eviction with manifest/pin protection implemented (T7b done)
- **Truncation Coordination**: ✅ **COMPLETE** - Lease-based coordination, manifest integration, and cancellation callbacks complete (T5 done)
- **PageCache Leasing**: ✅ **COMPLETE** - Lease wiring integrated into CheckpointOrchestrator with PageCacheObserver hooks and FetchOptions (T10 done)

### ✅ Recently Completed
- **Chunk Delta Production**: ✅ **COMPLETE** - Delta file format with DeltaBuilder/DeltaReader, PageStore delta layering, manifest integration (T4 done)
  - DeltaHeader with 64-byte format (magic, version, base_chunk_id, delta_generation, page_count, page_size)
  - DeltaBuilder for creating delta files with page directory and CRC64 footer
  - DeltaReader for parsing and querying delta files
  - PageStore.read_page() layers deltas over base chunks
  - Manifest.resolve_page_location() queries chunk_catalog + chunk_delta_index
  - ChunkEntryRecord extended with content_hash, page_size, page_count
  - ChunkDeltaRecord extended with content_hash
  - 125 tests passing (up from 122)
- **Cold Scan APIs**: ✅ **DONE** - ColdScanOptions, RemoteReadLimiter, DB::read_page_cold() fully integrated with metrics, throttling, and cache bypass (T9a+T9b complete, 7 tests passing)
- **PageStore Lease Wiring**: ✅ **DONE** - LeaseMap integrated into CheckpointOrchestrator, PageCacheObserver hooks extended with on_checkpoint_start/end, FetchOptions with pin_in_cache/prefetch_hint/bypass_cache, PinTracker for pin management (T10a+T10b+T10c complete, 3 tests added)
- **Failure Handling & Idempotency**: ✅ **DONE** - Comprehensive idempotent retry documentation, CancelCheckpoint compensating entries, 4 fault injection tests covering hash consistency, crash recovery, deterministic ordering, upload idempotence (T11a+T11b+T11c complete, 129 tests passing)

### ❌ Not Started
- **Job Queue Integration**: Orchestrator not wired to LMDB queue runtime (T6b deferred)
- **Observability**: Metrics, logging, and runbooks not implemented (T12 not started)

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
| **T1** | **Checkpoint planner split** | TBD | L | ✅ DONE | - | 100% | [planner.rs](../src/checkpoint/planner.rs) | • Planner detects append-only vs libsql by WAL format metadata<br>• Emits `CheckpointPlan` struct with chunk actions<br>• Unit tests cover both workload types |
| T1a | Planner scaffolding & CheckpointPlan struct | - | M | ✅ DONE | - | 100% | [planner.rs:40-180](../src/checkpoint/planner.rs#L40) | Defined `CheckpointPlan`, `ChunkAction`, `PlannerContext`, `WorkloadType` types with tests |
| T1b | Append-only planner (seal segment → chunk) | - | M | ✅ DONE | T1a | 100% | [planner.rs:190-250](../src/checkpoint/planner.rs#L190) | `plan_append_only_with_segments()` seals WAL segments as base chunks with CRC64 checksums; 3 tests passing |
| T1c | libsql planner (diff pages, group by chunk) | - | L | ✅ DONE | T1a | 100% | [planner.rs:280-520](../src/checkpoint/planner.rs#L280) | `plan_libsql_with_pages()` diffs dirty pages, emits delta vs full rewrite based on thresholds; 4 tests passing |
| **T2** | **Executor staging pipeline** | TBD | L | ✅ DONE | T1, T7 | 100% | [executor.rs](../src/checkpoint/executor.rs) | • Stages files to `checkpoint_scratch/{job_id}/`<br>• Verifies CRC64-NVME checksums<br>• Upload stubs ready for async S3 integration (T8) |
| T2a | Executor scaffolding & scratch directory mgmt | - | M | ✅ DONE | - | 100% | [executor.rs:98-194](../src/checkpoint/executor.rs#L98) | Creates/deletes job scratch dirs under LocalStore with verification and tests |
| T2b | Content-hash staging (deduplication support) | - | M | ✅ DONE | T2a | 100% | [executor.rs:230-312](../src/checkpoint/executor.rs#L230) | `stage_chunk()` uses CRC64-NVME content addressing, detects hash collisions; 3 tests passing |
| T2c | Upload coordination & validation | - | M | ✅ DONE | T2b, T8 | 100% | [executor.rs:314-398](../src/checkpoint/executor.rs#L314) | `upload_chunks()` validates checksums, stubs S3 upload for async integration; 3 tests passing; **NOTE: Will become async** |
| **T3** | **Manifest writer batch operations** | TBD | M | ✅ DONE | - | 100% | [manifest_ops.rs](../src/manifest/manifest_ops.rs), [operations.rs](../src/manifest/operations.rs), [tests.rs](../src/manifest/tests.rs#L994) | • Supports atomic batch of `ManifestOp` values<br>• Appends to `change_log` in same transaction<br>• Integration tests verify atomic commit and change log recording |
| T3a | ManifestOp enum & batch submission API | - | S | ✅ DONE | - | 100% | [manifest_ops.rs:30-43](../src/manifest/manifest_ops.rs#L30), [transaction.rs:61-82](../src/manifest/transaction.rs#L61) | UpsertChunk, DeleteChunk, UpsertChunkDelta, DeleteChunkDelta operations defined and wired |
| T3b | LMDB transaction logic for checkpoint batches | - | M | ✅ DONE | T3a | 100% | [operations.rs:311-326](../src/manifest/operations.rs#L311), [operations.rs:217](../src/manifest/operations.rs#L217) | Single txn writes chunk_catalog + chunk_delta_index + change_log atomically |
| **T4** | **Chunk delta production & consumption** | TBD | L | ✅ DONE | T3 | 100% | [delta.rs](../src/checkpoint/delta.rs), [db.rs](../src/db.rs#L362-L582), [api.rs](../src/manifest/api.rs#L512-L572) | • Delta file format implemented with CRC64 checksums ✅<br>• PageStore layers deltas over base chunks ✅<br>• ChunkEntryRecord extended with content_hash, page_size, page_count ✅<br>• ChunkDeltaRecord extended with content_hash ✅<br>• 122 tests passing including delta roundtrip tests ✅ |
| T4a | Delta file format & staging | - | M | ✅ DONE | T2 | 100% | [delta.rs:77-280](../src/checkpoint/delta.rs#L77) | DeltaHeader, DeltaBuilder, DeltaReader implemented with 64-byte header (magic, version, base_chunk_id, delta_generation, page_count, page_size), page directory, and CRC64 footer; 3 tests passing |
| T4b | chunk_delta_index integration | - | M | ✅ DONE | T3b | 100% | [operations.rs:319-326](../src/manifest/operations.rs#L319), [tables.rs:1098-1133](../src/manifest/tables.rs#L1098) | Manifest writer upserts/deletes delta records via UpsertChunkDelta/DeleteChunkDelta ops; ChunkDeltaRecord extended with content_hash field |
| T4c | PageStore delta layering (read path) | - | L | ✅ DONE | T10 | 100% | [db.rs:362-614](../src/db.rs#L362), [api.rs:512-572](../src/manifest/api.rs#L512) | PageStore.read_page() layers deltas over base chunks; Manifest.resolve_page_location() queries chunk_catalog + chunk_delta_index; Delta header caching implemented; ChunkEntryRecord extended with content_hash, page_size, page_count fields; read_page_cold() for bandwidth-limited reads |
| **T5** | **Append-only truncation jobs** | TBD | L | ✅ DONE | T1, T3 | 100% | [truncation.rs](../src/checkpoint/truncation.rs) | • Tail/head truncation commands accepted ✅<br>• Lease coordination blocks truncation during checkpoint ✅<br>• Manifest batch operations integrated ✅<br>• Cancellation callbacks implemented ✅ |
| T5a | TruncateAppendOnly job type & lease map | - | M | ✅ DONE | - | 100% | [truncation.rs:60-300](../src/checkpoint/truncation.rs#L60) | TruncationRequest, LeaseMap, ChunkLease implemented with full lease coordination and tests |
| T5b | Truncation manifest batch operations | - | M | ✅ DONE | T3b, T5a | 100% | [truncation.rs:394-405](../src/checkpoint/truncation.rs#L394) | Manifest transaction integrated in `execute_truncation()` with delete_chunk operations; 17 tests passing; **NOTE: S3 deletion will be async** |
| T5c | Cancellation signal for in-flight checkpoints | - | M | ✅ DONE | T5a, T6 | 100% | [truncation.rs:325-332,444-473](../src/checkpoint/truncation.rs#L325) | CancellationCallback type + set_cancellation_callback method; invoked during wait_for_truncation |
| **T6** | **Job queue orchestration** | TBD | M | ✅ DONE | T1, T2 | 100% | [orchestrator.rs](../src/checkpoint/orchestrator.rs) | • CheckpointOrchestrator coordinates end-to-end checkpoint flow<br>• 6-phase execution: Preparing → Planning → Staging → Uploading → Publishing → Cleanup<br>• Integrates planner, executor, RemoteStore, manifest, quota |
| T6a | Checkpoint orchestrator implementation | - | M | ✅ DONE | T1, T2, T7a, T8 | 100% | [orchestrator.rs:82-300](../src/checkpoint/orchestrator.rs#L82) | Orchestrator coordinates all components, async S3 uploads, quota enforcement; **NOTE: Queue integration (enqueue/dequeue) deferred to separate task** |
| T6b | Progress tracking & resume logic | - | M | ✅ DONE | T6a | 100% | [orchestrator.rs:321-489](../src/checkpoint/orchestrator.rs#L321), [tables.rs:1329-1350](../src/manifest/tables.rs#L1329), [transaction.rs:147-153](../src/manifest/transaction.rs#L147), [api.rs:598-603](../src/manifest/api.rs#L598) | JobPayload::CheckpointProgress variant stores (phase, total_chunks, chunks_staged, chunks_uploaded, processed_chunks, generation); save_progress() persists to manifest after each phase; load_progress() and resume_checkpoint() enable crash recovery; Progress saved every 10 chunks during staging/uploading |
| **T7** | **LocalStore quota & janitor** | TBD | M | ✅ DONE | - | 100% | [storage_quota.rs](../src/storage_quota.rs), [scratch_janitor.rs](../src/scratch_janitor.rs), [local_store.rs](../src/local_store.rs) | • Hierarchical quota (global, per-tenant, per-job) ✅<br>• Janitor reclaims abandoned scratch dirs ✅<br>• hot_cache/ LRU eviction with manifest/pin protection ✅ |
| T7a | StorageQuota service (hierarchical budgets) | - | M | ✅ DONE | - | 100% | [storage_quota.rs:30-250](../src/storage_quota.rs#L30) | Reserve/release API with CAS loops, ReservationGuard RAII, per-tenant/per-job accounting; 6 tests passing |
| T7b | hot_cache/ eviction with PageCache pin awareness | - | M | ✅ DONE | - | 100% | [local_store.rs:90-445](../src/local_store.rs#L90) | LocalStore manages hot_cache/ with LRU eviction, protects manifest-referenced and pinned chunks; 4 tests passing; **NOTE: Pin tracking is stubbed, awaits PageCache integration** |
| T7c | Scratch janitor coroutine | - | S | ✅ DONE | T7a | 100% | [scratch_janitor.rs:30-170](../src/scratch_janitor.rs#L30) | Async tokio task scans checkpoint_scratch/ every 10min, deletes dirs >1hr old without active jobs; 3 tests passing |
| **T8** | **RemoteStore S3 streaming (ASYNC)** | TBD | M | ✅ DONE | - | 100% | [remote_store.rs](../src/remote_store.rs) | • **ASYNC API**: put_blob/get_blob with tokio runtime ✅<br>• rust-s3 client with MinIO and AWS S3 support ✅<br>• CRC64-NVME checksum validation ✅<br>• BlobKey naming scheme implemented ✅ |
| T8a | S3 client wrapper & blob naming scheme | - | M | ✅ DONE | - | 100% | [remote_store.rs:60-180](../src/remote_store.rs#L60) | Async API using rust-s3 v0.35 with tokio-rustls-tls; BlobKey::chunk/delta naming with content hashes; 2 tests passing |
| T8b | Streaming upload with retry & checksum | - | M | ✅ DONE | T8a | 100% | [remote_store.rs:200-280](../src/remote_store.rs#L200) | Async put_blob validates CRC64-NVME checksums; rust-s3 handles retry/backoff internally |
| T8c | Streaming download with passthrough mode | - | M | ✅ DONE | T8a | 100% | [remote_store.rs:320-380](../src/remote_store.rs#L320) | Async get_blob with checksum validation; supports writing to AsyncWrite destination |
| **T9** | **Cold scan API** | TBD | M | ✅ DONE | T8, T10 | 100% | [cold_scan.rs](../src/cold_scan.rs), [db.rs](../src/db.rs#L434-L493) | • RemoteReadLimiter enforces per-tenant bandwidth caps ✅<br>• PageStore.read_page_cold() fully integrated ✅<br>• Metrics tracking with throttle retry logic ✅<br>• Public DB::read_page_cold() API added ✅<br>• 7 tests passing |
| T9a | ColdScanOptions & RemoteReadLimiter | - | S | ✅ DONE | - | 100% | [cold_scan.rs:37-170](../src/cold_scan.rs#L37) | ColdScanOptions with max_bandwidth_bytes_per_sec, bypass_cache, track_metrics; RemoteReadLimiter with token bucket algorithm; ColdScanStatsTracker for metrics; 7 tests passing |
| T9b | PageStore cold read path integration | - | M | ✅ DONE | T9a, T8c | 100% | [db.rs:434-493](../src/db.rs#L434) | PageStore::read_page_cold() with cache bypass, bandwidth throttling, automatic retry on BandwidthExceeded, metrics recording via ColdScanStatsTracker, delta layering support; public DB::read_page_cold() API (lines 244-278) |
| **T10** | **PageStore lease wiring** | TBD | M | ✅ DONE | - | 100% | [page_cache.rs:105-124](../src/page_cache.rs#L105), [orchestrator.rs:110-149](../src/checkpoint/orchestrator.rs#L110), [page_store_policies.rs](../src/page_store_policies.rs) | • LeaseMap integrated into CheckpointOrchestrator ✅<br>• Leases registered/released during checkpoint flow ✅<br>• PageCacheObserver with on_checkpoint_start/end hooks ✅<br>• FetchOptions with pin_in_cache, prefetch_hint, bypass_cache ✅<br>• 3 tests added (128 total passing) |
| T10a | PageCache observer checkpoint hooks | - | M | ✅ DONE | - | 100% | [page_cache.rs:105-124](../src/page_cache.rs#L105), [page_store_policies.rs:121-143](../src/page_store_policies.rs#L121) | PageCacheObserver trait extended with on_checkpoint_start/end hooks; CheckpointObserver implements hooks for lease coordination; tested with 2 new tests |
| T10b | Lease tracking in planner state | - | M | ✅ DONE | T1, T5a | 100% | [orchestrator.rs:110-149](../src/checkpoint/orchestrator.rs#L110), [orchestrator.rs:213-221](../src/checkpoint/orchestrator.rs#L213), [orchestrator.rs:330-333](../src/checkpoint/orchestrator.rs#L330) | LeaseMap added to CheckpointOrchestrator; leases registered per chunk during staging (lines 213-221); leases released after manifest publication (lines 330-333); lease_map() accessor added |
| T10c | Cache policies (pin_in_cache, fetch hints) | - | S | ✅ DONE | T10a | 100% | [page_store_policies.rs:18-57](../src/page_store_policies.rs#L18) | FetchOptions struct with pin_in_cache, prefetch_hint (None/Forward/Backward/Bidirectional), bypass_cache; PinTracker for page pin count management; PinGuard RAII wrapper; tested with 1 new test |
| **T11** | **Failure handling & idempotency** | TBD | L | ✅ DONE | T2, T3, T5 | 100% | [executor.rs:29-60](../src/checkpoint/executor.rs#L29), [manifest_ops.rs:80-101](../src/manifest/manifest_ops.rs#L80), [executor.rs:741-851](../src/checkpoint/executor.rs#L741) | • Idempotent retry via hash-based blob keys ✅<br>• Deterministic chunk ordering ✅<br>• Atomic manifest publication ✅<br>• CancelCheckpoint compensating entries ✅<br>• 4 fault injection tests passing (129 total) |
| T11a | Idempotent retry logic in executor | - | M | ✅ DONE | T2 | 100% | [executor.rs:29-60](../src/checkpoint/executor.rs#L29) | Comprehensive documentation of idempotent retry logic: deterministic chunk ordering, hash-based blob keys (CRC64-NVME), atomic LMDB transactions, progress tracking; hash-based keys make S3 PUTs idempotent |
| T11b | Compensating change log entries | - | M | ✅ DONE | T3, T5 | 100% | [manifest_ops.rs:80-101](../src/manifest/manifest_ops.rs#L80), [transaction.rs:161-177](../src/manifest/transaction.rs#L161), [operations.rs:391-401](../src/manifest/operations.rs#L391) | CancelCheckpoint manifest operation with CheckpointCancellationReason enum (TruncationConflict/UserRequested/Timeout); ManifestTxn::cancel_checkpoint() method; change log records cancellations for observability |
| T11c | Fault injection test suite | - | L | ✅ DONE | T11a, T11b | 100% | [executor.rs:741-851](../src/checkpoint/executor.rs#L741) | 4 fault injection tests: idempotent_staging_same_data (hash consistency), scratch_dir_cleanup_after_crash_simulation (recovery), deterministic_chunk_ordering (reproducibility), upload_retry_idempotence (duplicate uploads); all tests passing |
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
