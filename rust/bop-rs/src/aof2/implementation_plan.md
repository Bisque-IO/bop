# AOF2 Async Tiered Integration Plan

## Next Up
- Implement **AP3** (`append_record_async`) so the sync backpressure loop has an async companion.
- Finish **RD5** reader documentation and move into **RC1**/**RC2** recovery milestones once the docs are merged.
- Prep recovery design notes so RC1/RC2 can start once docs land (catalog snapshot format, Tier 2 reconciliation checklist).


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
- [x] **AO4**: Update recovery/bootstrap to request residency through `TieredInstance` rather than touching the filesystem directly. (completed 2025-09-24, recovery walks TieredInstance::recover_segments and rehydrates via tiered checkout)
- [x] **AO5**: Port unit tests/integration tests to construct `Aof` via the manager (completed 2025-09-25, shared `TestManager` harness now builds `Aof` instances through `AofManager` across unit and reader tests).

## Milestone 4 – Append Path Integration
Keep synchronous append semantics while delegating capacity decisions to the tiered store.
- [ ] **AP1**: Replace direct Tier 0 file handling with `AdmissionGuard` obtained from Tier 0 cache; return `WouldBlock` when admission fails.
    - [x] Promote `AdmissionGuard` in `store/tier0.rs` so it encapsulates segment slot state, flush budget, and drop-driven release semantics.
    - [x] Update `Aof::append_record` and supporting helpers in `flush.rs` to acquire the guard before touching Tier 0 writers and translate guard acquisition failures into `AofError::WouldBlock(BackpressureKind::Admission)`.
    - [x] Add targeted coverage under `aof2::tests` that exhausts Tier 0 capacity, asserts the sync API surfaces `WouldBlock`, and verifies coordinator wake-ups when the guard drops. (aof2::tests::append_surfaces_admission_would_block_until_guard_released)
    - [x] Refresh the append path section in `store.md` with an example that creates the guard, handles `WouldBlock`, and defers to `append_record_async` for retry loops.
- [x] **AP2**: Ensure sealing/rollover requests enqueue work on the coordinator and emit `WouldBlock` until acknowledged. (completed 2025-09-28, rollover queue now defers ack until Tier1 compression + Tier2 upload scheduling succeed)
    - [x] Introduce a `CoordinatorCommand::Rollover { stream_id, resident, ack }` variant (oneshot sender) and have `TieredInstance::request_rollover` push it onto the rollover queue, surfacing a future tied to the ack.
    - [x] Update the coordinator loop to drain the new command, stage Tier 1 metadata + Tier 2 upload intents, and fulfil or error the ack; capture failure paths as `AofError::RolloverFailed` with telemetry counters.
    - [x] Teach `Aof::seal_current_segment` (and the flush scheduler) to poll the ack opportunistically; when the queue back-pressures or the ack is pending, surface `WouldBlock(BackpressureKind::Rollover)` and let the async helper await coordinator notifications.
    - [x] Back the change with an async regression test under `aof2::tests` that fakes a slow coordinator, ensuring the sync API returns `WouldBlock` and the async API resumes once the ack arrives. (`aof2::tests::seal_active_requires_rollover_ack`)
    - [x] Documented the rollover handshake in `design_next.md` (state machine, async retry loop) and `store.md` (event sequencing, notifier usage); linked both sections here.
- [ ] **AP3**: Implement `append_record_async` that loops on the sync API and awaits coordinator notifications between retries.
    - [ ] Capture the async retry state machine (`BackpressureKind` -> notifier mapping) in `design_next.md` to keep reviewers aligned.
    - [ ] Implement `Aof::append_record_async` in `mod.rs`, reusing the sync helper inside a loop that awaits admission and rollover notifies.
    - [ ] Add async regression tests under `aof2::tests` covering admission exhaustion and rollover ack delays via the new helper.
    - [ ] Refresh `store.md` examples to show when to call the async helper versus retrying manually.
- [ ] **AP4**: Rework flush scheduling to interact with tier metadata (durability markers stored alongside residency state).
- [ ] **AP5**: Extend metrics to capture Tier 0 admission latency and `WouldBlock` frequency.

## Milestone 5 – Read Path and Hydration
Support synchronous readers with `WouldBlock` plus async helpers that await hydration.
- [x] **RD1**: Update `AofReader` to fetch segments through `TieredInstance`, returning `WouldBlock` while hydration/promotions run. (completed 2025-09-24, reader uses tier checkout + sealed stream awaits hydration)
- [x] **RD2**: Provide `next_record_async` that awaits hydration futures and resumes iteration seamlessly. (completed 2025-09-24, async sealed stream bridges WouldBlock via tier await)
- [x] **RD3**: Maintain reader-side caches of residency hints to avoid redundant promotions. (completed 2025-09-26, residency hint cache integrated into Aof with unit coverage in aof2::tests::residency_hint_cache_tracks_pending_segments).
- [x] **RD4**: Add tests covering Tier 1 miss -> hydration -> read resume, including concurrent readers. (completed 2025-09-26, async regression test aof2::tests::tier1_hydration_recovers_segment_on_read_miss exercises Tier1 miss hydration with parallel readers.)
- [ ] **RD5**: Document reader API changes and guidance for handling `WouldBlock` in downstream services.
    - [ ] Once the reader docs land, circulate an RC1/RC2 kickoff checklist and update dependencies here before starting implementation.
    - [ ] Author `docs/aof2_read_path.md` with sections covering sync vs async flows, handling `BackpressureKind::Hydration`/`Rollover`, sample retry loops, and integration advice for gRPC + HTTP surfaces.
    - [ ] Cross-link the new doc from `design_next.md` (reader flow diagrams) and `store.md` (hydration/notification details), adding "See also" callouts that explain when to delegate to async helpers.
    - [ ] Provide a migration note in `progress.md` that links to the reader guidance, highlights behavioural differences from legacy AOF, and enumerates follow-up tasks for downstream services.

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


