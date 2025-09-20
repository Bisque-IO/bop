# AOF2 Async Tiered Integration Plan


## Milestone 1 - Manager Foundations
Establish the runtime and configuration scaffolding that will host the async tiered store.
- [x] **CFG1**: Extend `AofManagerConfig` with Tier 0/1/2 sizing, eviction, and S3 client settings plus defaults mirroring legacy behaviour. (completed 2025-09-20, config now stores TieredStoreConfig defaults).
- [x] **CFG2**: Introduce `TieredRuntime` wrapper that owns the Tokio runtime, exposes `Handle`, and supports deterministic shutdown (cancellation tokens, graceful join). (completed 2025-09-20, wrapper exposes handle + cancellation token).
- [x] **CFG3**: Update `AofManager::with_config` to construct Tier 0 cache, Tier 1 store, Tier 2 client facade, and `TieredCoordinator`; ensure creation errors map to manager init errors. (completed 2025-09-20, manager builds tier scaffolding and propagates errors).
- [x] **CFG4**: Provide lightweight builders for tests (ephemeral filesystem, in-memory S3 stub) and document in-code usage. (completed 2025-09-20, `AofManagerConfig::for_tests` helper available).
- [x] **CFG5**: Add smoke tests validating manager startup/shutdown without opening streams (env gating ensures they run under `cargo test`). (completed 2025-09-20, `manager_tests` covers startup with stub tiers).

## Milestone 2 – Coordinator Service Loop
Wire the coordinator so residency requests flow through the manager runtime.
- [x] **CO1**: Implement `TieredCoordinator::poll` futures for activation, eviction, upload, and hydration queues (see `store.md`). (completed 2025-09-20, poll now drains Tier-0/1/2 queues and emits stats)
- [x] **CO2**: Add manager-owned service task (`service_tiered`) that repeatedly awaits `poll` and records metrics for each queue. (completed 2025-09-20, background loop drives polling with idle backoff)
- [x] **CO3**: Expose sync/async wake hooks (Notify or wakers) so `WouldBlock` sites can register interest. (completed 2025-09-20, coordinator publishes activity and hydration `Notify` handles)
- [x] **CO4**: Ensure shutdown cancels the poll loop, drains pending work gracefully, and surfaces errors through manager drop. (completed 2025-09-20, service task honors the manager cancellation token)
- [x] **CO5**: Add instrumentation tests that simulate queue items and assert state transitions (e.g., Tier 0 eviction -> Tier 1 compression). (completed 2025-09-20, gated async test exercises drop-to-compression flow when `tiered-store` is enabled)

## Milestone 3 – Aof Construction and Lifecycle
Adapt `Aof` to depend on the manager/tiered store.
- [x] **AO1**: Change `Aof::new` signature to accept an `Arc<AofManagerHandle>` and retrieve tier handles plus runtime references. (completed 2025-09-21, Aof now builds via manager handle and callers/tests updated)
- [x] **AO2**: Refactor internal catalog structures to store `ResidentSegment` handles instead of raw paths. (completed 2025-09-21, catalog/pending/flush queues now retain tier-owned ResidentSegment handles)
- [x] **AO3**: Introduce `TieredInstance` (per-stream view) that mediates between catalog and coordinator. (completed 2025-09-22, TieredInstance now owns Tier 0 admissions, updates Tier 1 residency, and wakes the coordinator on evictions.)
- [ ] **AO4**: Update recovery/bootstrap to request residency through `TieredInstance` rather than touching the filesystem directly.
- [ ] **AO5**: Port unit tests/integration tests to construct `Aof` via the manager (adjust helpers accordingly).

## Milestone 4 – Append Path Integration
Keep synchronous append semantics while delegating capacity decisions to the tiered store.
- [ ] **AP1**: Replace direct Tier 0 file handling with `AdmissionGuard` obtained from Tier 0 cache; return `WouldBlock` when admission fails.
- [ ] **AP2**: Ensure sealing/rollover requests enqueue work on the coordinator and emit `WouldBlock` until acknowledged.
- [ ] **AP3**: Implement `append_record_async` that loops on the sync API and awaits coordinator notifications between retries.
- [ ] **AP4**: Rework flush scheduling to interact with tier metadata (durability markers stored alongside residency state).
- [ ] **AP5**: Extend metrics to capture Tier 0 admission latency and `WouldBlock` frequency.

## Milestone 5 – Read Path and Hydration
Support synchronous readers with `WouldBlock` plus async helpers that await hydration.
- [ ] **RD1**: Update `AofReader` to fetch segments through `TieredInstance`, returning `WouldBlock` while hydration/promotions run.
- [ ] **RD2**: Provide `next_record_async` that awaits hydration futures and resumes iteration seamlessly.
- [ ] **RD3**: Maintain reader-side caches of residency hints to avoid redundant promotions.
- [ ] **RD4**: Add tests covering Tier 1 miss -> hydration -> read resume, including concurrent readers.
- [ ] **RD5**: Document reader API changes and guidance for handling `WouldBlock` in downstream services.

## Milestone 6 – Recovery and Operation Log
Ensure startup replay and replicated actions respect the tiered architecture.
- [ ] **RC1**: Persist catalog snapshots that reference the tiered operation log watermark.
- [ ] **RC2**: Implement startup replay that rebuilds Tier 0/1 manifests, reconciles Tier 2 objects, and schedules hydrations before enabling append.
- [ ] **RC3**: Integrate replicated operation log application into coordinator so followers converge on residency decisions.
- [ ] **RC4**: Handle partial failures (e.g., missing Tier 1 artifact) by queuing downloads from Tier 2 and flagging segments `NeedsRecovery` until satisfied.
- [ ] **RC5**: Add integration tests simulating crash/restart with pending uploads and partially applied log entries.

## Milestone 7 – Observability, Tooling, and Docs
Round out developer/operator experience.
- [ ] **OBS1**: Wire metrics for queue depths, hydration latency, upload retries, and residency state counts.
- [ ] **OBS2**: Emit structured logs for major tier events (promotion, eviction, upload, hydration failures).
- [ ] **OBS3**: Provide admin/debug endpoints or CLI tooling to dump residency maps per stream.
- [ ] **OBS4**: Update developer docs (`design.md`, `store.md`, samples) to reflect new manager requirements and API pairs.
- [ ] **OBS5**: Author operator runbook covering configuration knobs, scaling guidance, and troubleshooting `WouldBlock` hot spots.

## Milestone 8 – Validation and Rollout
Prove correctness and plan adoption.
- [ ] **VAL1**: Extend existing test harness to run under Tokio, covering append/read under eviction, Tier 2 outages, and recovery scenarios.
- [ ] **VAL2**: Add chaos/failure injection tests (disk full, S3 throttling, forced coordinator restart) ensuring APIs surface actionable errors.
- [ ] **VAL3**: Benchmark sync vs async append/read throughput to confirm back-pressure behaves as expected.
- [ ] **VAL4**: Document rollout steps (feature flagging, staged deployment, telemetry checks) and align with release process.
- [ ] **VAL5**: Define exit criteria for legacy non-tiered code paths and plan clean-up.

## Cross-cutting tasks
- [ ] **CC1**: Establish coding guidelines for pairing sync/async APIs (`WouldBlock` contract, naming, documentation notes).
- [ ] **CC2**: Ensure all new futures are cancel-safe and runtime handles are not held across await points that could block shutdown.
- [ ] **CC3**: Audit error enums and map new failure types (Tier 1 compression failure, Tier 2 upload failure, hydration timeout) to actionable diagnostics.
- [ ] **CC4**: Keep this plan in sync with implementation progress; include PR references when checking off tasks.


