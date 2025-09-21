# AOF2 Async Tiered Integration Plan

## Next Up

- [ ] Wire `Tier1Instance` to instantiate the new `ManifestLogWriter` behind `aof2_manifest_log_enabled`, emitting shadow records alongside the legacy JSON manifest (MAN2).

- [ ] Add the crash-mid-commit manifest test exercising recovery trim and replay (MAN1/MAN3 integration test matrix entry).

- [ ] Delete `manifest_log.rs` and migrate any remaining references onto the new manifest module surface (MAN4 hygiene follow-up).

## Milestone 1 - Manager Foundations

Establish the runtime and configuration scaffolding that will host the async tiered store.

- [x] **CFG1**: Extend `AofManagerConfig` with Tier 0/1/2 sizing, eviction, and S3 client settings plus defaults mirroring legacy behaviour. (completed 2025-09-20, config now stores TieredStoreConfig defaults).

- [x] **CFG2**: Introduce `TieredRuntime` wrapper that owns the Tokio runtime, exposes `Handle`, and supports deterministic shutdown (cancellation tokens, graceful join). (completed 2025-09-20, wrapper exposes handle + cancellation token).

- [x] **CFG3**: Update `AofManager::with_config` to construct Tier 0 cache, Tier 1 store, Tier 2 client facade, and `TieredCoordinator`; ensure creation errors map to manager init errors. (completed 2025-09-20, manager builds tier scaffolding and propagates errors).

- [x] **CFG4**: Provide lightweight builders for tests (ephemeral filesystem, in-memory S3 stub) and document in-code usage. (completed 2025-09-20, `AofManagerConfig::for_tests` helper available).

- [x] **CFG5**: Add smoke tests validating manager startup/shutdown without opening streams (env gating ensures they run under `cargo test`). (completed 2025-09-20, `manager_tests` covers startup with stub tiers).

## Milestone 2 - Coordinator Service Loop

Wire the coordinator so residency requests flow through the manager runtime.

- [x] **CO1**: Implement `TieredCoordinator::poll` futures for activation, eviction, upload, and hydration queues (see `aof2_store.md`). (completed 2025-09-20, poll now drains Tier-0/1/2 queues and emits stats)

- [x] **CO2**: Add manager-owned service task (`service_tiered`) that repeatedly awaits `poll` and records metrics for each queue. (completed 2025-09-20, background loop drives polling with idle backoff)

- [x] **CO3**: Expose sync/async wake hooks (Notify or wakers) so `WouldBlock` sites can register interest. (completed 2025-09-20, coordinator publishes activity and hydration `Notify` handles)

- [x] **CO4**: Ensure shutdown cancels the poll loop, drains pending work gracefully, and surfaces errors through manager drop. (completed 2025-09-20, service task honors the manager cancellation token)

- [x] **CO5**: Add instrumentation tests that simulate queue items and assert state transitions (e.g., Tier 0 eviction -> Tier 1 compression). (completed 2025-09-20, async test now exercises the core tiered path end-to-end)

## Milestone 3 - Aof Construction and Lifecycle

Adapt `Aof` to depend on the manager/tiered store.

- [x] **AO1**: Change `Aof::new` signature to accept an `Arc<AofManagerHandle>` and retrieve tier handles plus runtime references. (completed 2025-09-21, Aof now builds via manager handle and callers/tests updated)

- [x] **AO2**: Refactor internal catalog structures to store `ResidentSegment` handles instead of raw paths. (completed 2025-09-21, catalog/pending/flush queues now retain tier-owned ResidentSegment handles)

- [x] **AO3**: Introduce `TieredInstance` (per-stream view) that mediates between catalog and coordinator. (completed 2025-09-22, TieredInstance now owns Tier 0 admissions, updates Tier 1 residency, and wakes the coordinator on evictions.)

- [x] **AO4**: Update recovery/bootstrap to request residency through `TieredInstance` rather than touching the filesystem directly. (completed 2025-09-24, recovery walks TieredInstance::recover_segments and rehydrates via tiered checkout)

- [x] **AO5**: Port unit tests/integration tests to construct `Aof` via the manager (completed 2025-09-25, shared `TestManager` harness now builds `Aof` instances through `AofManager` across unit and reader tests).

## Milestone 4 - Append Path Integration

Keep synchronous append semantics while delegating capacity decisions to the tiered store.

- [x] **AP1**: Replace direct Tier 0 file handling with `AdmissionGuard` obtained from Tier 0 cache; return `WouldBlock` when admission fails. (completed 2025-09-30, sync/async append tests now drive admission guards end-to-end.)

    - [x] Promote `AdmissionGuard` in `store/tier0.rs` so it encapsulates segment slot state, flush budget, and drop-driven release semantics.

    - [x] Update `Aof::append_record` and supporting helpers in `flush.rs` to acquire the guard before touching Tier 0 writers and translate guard acquisition failures into `AofError::WouldBlock(BackpressureKind::Admission)`.

    - [x] Add targeted coverage under `aof2::tests` that exhausts Tier 0 capacity, asserts the sync API surfaces `WouldBlock`, and verifies coordinator wake-ups when the guard drops. (aof2::tests::append_surfaces_admission_would_block_until_guard_released)

    - [x] Refresh the append path section in `aof2_store.md` with an example that creates the guard, handles `WouldBlock`, and defers to `append_record_async` for retry loops.

- [x] **AP2**: Ensure sealing/rollover requests enqueue work on the coordinator and emit `WouldBlock` until acknowledged. (completed 2025-09-28, rollover queue now defers ack until Tier1 compression + Tier2 upload scheduling succeed)

    - [x] Introduce a `CoordinatorCommand::Rollover { stream_id, resident, ack }` variant (oneshot sender) and have `TieredInstance::request_rollover` push it onto the rollover queue, surfacing a future tied to the ack.

    - [x] Update the coordinator loop to drain the new command, stage Tier 1 metadata + Tier 2 upload intents, and fulfil or error the ack; capture failure paths as `AofError::RolloverFailed` with telemetry counters.

    - [x] Teach `Aof::seal_current_segment` (and the flush scheduler) to poll the ack opportunistically; when the queue back-pressures or the ack is pending, surface `WouldBlock(BackpressureKind::Rollover)` and let the async helper await coordinator notifications.

    - [x] Back the change with an async regression test under `aof2::tests` that fakes a slow coordinator, ensuring the sync API returns `WouldBlock` and the async API resumes once the ack arrives. (`aof2::tests::seal_active_requires_rollover_ack`)

    - [x] Documented the rollover handshake in `aof2_design_next.md` (state machine, async retry loop) and `aof2_store.md` (event sequencing, notifier usage); linked both sections here.

- [x] **AP3**: Implement `append_record_async` that loops on the sync API and awaits coordinator notifications between retries. (completed 2025-09-30, helper now coordinates with admission/rollover notifiers and passes the async regression suite.)

    - [x] Capture the async retry state machine (`BackpressureKind` -> notifier mapping) in `aof2_design_next.md` to keep reviewers aligned. (completed 2025-10-02, notifier matrix added under §11.2 clarifying admission/rollover/flush handling.)

    - [x] Implement `Aof::append_record_async` in `mod.rs`, reusing the sync helper inside a loop that awaits admission and rollover notifies.

    - [x] Add async regression tests under `aof2::tests` covering admission exhaustion and rollover ack delays via the new helper.

    - [x] Refresh `aof2_store.md` examples to show when to call the async helper versus retrying manually. (completed 2025-10-02, doc now contrasts sync backoff loop vs Tokio helper usage.)

- [ ] **AP4**: Rework flush scheduling to interact with tier metadata (durability markers stored alongside residency state).

    - [x] Capture the current flush lifecycle (`flush.rs`, `segment.rs`, catalog flush queue) in a short design note and annotate the entry points that must push tier metadata updates. (see `aof2_flush_metadata_note.md`)

    - [x] Extend `SegmentFlushState`/`FlushManager` to carry the owning `ResidentSegment` and instance identifiers so tier metadata writes can happen when `mark_durable` advances. (implemented via `FlushRequest` in `rust/bop-rs/src/aof2/flush.rs:30` and `Aof::schedule_flush` wiring)

    - [ ] Introduce a durability cursor on `TieredInstance` that records requested vs durable bytes, and persist it whenever the flush worker finishes a segment.

    - [ ] Propagate the persisted durability marker into `Aof::segment_snapshot`/`recover_existing_segments` so restarts hydrate tier metadata before serving readers.

    - [ ] Harden the flush worker with retry + metrics when tier metadata persistence fails and surface fatal cases as `BackpressureKind::Flush`.

    - [ ] Add regression coverage for partial flush advancement, restart recovery of the durability marker, and async readers observing the updated watermark before leaving `WouldBlock`.

    - [ ] Update `aof2_store.md` and `aof2_design_next.md` to document the flush metadata handshake and the new tier persistence invariants.

- [ ] **AP5**: Extend metrics to capture Tier 0 admission latency and `WouldBlock` frequency.

## Milestone 5 - Read Path and Hydration

Support synchronous readers with `WouldBlock` plus async helpers that await hydration.

- [x] **RD1**: Update `AofReader` to fetch segments through `TieredInstance`, returning `WouldBlock` while hydration/promotions run. (completed 2025-09-24, reader uses tier checkout + sealed stream awaits hydration)

- [x] **RD2**: Provide `next_record_async` that awaits hydration futures and resumes iteration seamlessly. (completed 2025-09-24, async sealed stream bridges WouldBlock via tier await)

- [x] **RD3**: Maintain reader-side caches of residency hints to avoid redundant promotions. (completed 2025-09-26, residency hint cache integrated into Aof with unit coverage in aof2::tests::residency_hint_cache_tracks_pending_segments).

- [x] **RD4**: Add tests covering Tier 1 miss -> hydration -> read resume, including concurrent readers. (completed 2025-09-26, async regression test aof2::tests::tier1_hydration_recovers_segment_on_read_miss exercises Tier1 miss hydration with parallel readers.)

- [ ] **RD5**: Document reader API changes and guidance for handling `WouldBlock` in downstream services.

    - [x] Lock the outline (glossary, sync vs async sequences, backpressure table) and circulate to platform reviewers for sign-off before writing. (outline drafted in `docs/aof2_read_path.md`)

    - [ ] Author `docs/aof2_read_path.md` with sections covering sync vs async flows, handling `BackpressureKind::Hydration`/`Rollover`, sample retry loops, and integration advice for gRPC + HTTP surfaces. (draft content published; awaiting platform review and cross-link pass)

    - [ ] Capture concrete examples from `Aof::open_reader{,_async}` and `reader.rs` (sealed stream vs tail follower) showing when to expect `WouldBlock`; embed them as doctest-style snippets.

    - [ ] Cross-link the new doc from `aof2_design_next.md` (reader flow diagrams) and `aof2_store.md` (hydration/notification details), adding "See also" callouts that explain when to delegate to async helpers.

    - [ ] Provide a migration note in `aof2_progress.md` that links to the reader guidance, highlights behavioural differences from legacy AOF, and enumerates follow-up tasks for downstream services.

    - [ ] Once the reader docs land, circulate an RC1/RC2 kickoff checklist and update dependencies here before starting implementation.

## Milestone 6a - Manifest Log and Replay

Replace ad-hoc JSON manifests with the append-only manifest log.

- [x] **MAN1**: Implement chunked manifest log writer/reader with crc64-nvme checksums and mmap-backed append path. (landed `aof2::manifest::{chunk, record, reader, writer}` modules with rotation + CRC validation; see `rust/bop-rs/src/aof2/manifest/`)

    - [x] Support chunk rotation (16 KiB to 1 MiB configurable), sealing, and Tier 2 upload streaming hooks. (`ManifestLogWriter` handles rotation/ sealing and surfaces `SealedChunkHandle` for Tier 2 streaming)

    - [x] Provide binary record schemas for seal/compression/upload/eviction events and end-to-end crc64 validation. (`ManifestRecordPayload` encodes all tier transitions with per-record CRC checks)

- [ ] **MAN2**: Integrate the log with `Tier1Instance` replacing `Tier1Manifest` JSON loading/saving.

    - [ ] Maintain in-memory manifest index fed by log replay and expose existing lookup APIs.

    - [ ] Remove the dormant JSON manifest loader/saver and keep the new log gated behind a feature flag until Tier 1 burn-in completes.

- [ ] **MAN3**: Add recovery replay that loads the latest manifest snapshot then applies log chunks, reconciling Tier 0/Tier 1/Tier 2 state.

    - [ ] Provide tooling to inspect manifest chunks and verify Tier 2 object presence.

    - [ ] Emit metrics (chunk count, replay time, journal lag) and alerts for corruption.

- [ ] **MAN4**: Update documentation (`aof2_manifest_log.md`, `aof2_store.md`) and integration tests to cover crash/restart with pending chunks and Tier 2 compaction.

    - [x] Publish the manifest log spec in `aof2_manifest_log.md` and cross-link it from `aof2_store.md`.

    - [ ] Extend integration tests to exercise crash/restart with pending chunks and Tier 2 compaction.

## Milestone 6 - Recovery and Operation Log

Ensure startup replay and replicated actions respect the tiered architecture.

- [ ] **RC1**: Persist catalog snapshots that reference the tiered operation log watermark.

    - [x] Gather requirements for the snapshot payload (catalog entries, flush queue state, tier residency metadata, durability cursors) and confirm recovery consumers. (draft captured in `aof2_catalog_snapshots.md`, pending stakeholder review)

    - [ ] Define the watermark semantics so `TieredCoordinator` operation log sequences stay aligned with segment durability and flush metadata updates.

    - [ ] Sketch storage layout (local staging vs Tier 2 upload), retention policy, and how snapshots coordinate with ongoing flush/rollover work.

    - [ ] Prototype a `SnapshotWriter` API in `Aof`/`TieredInstance` that can serialize the catalog + metadata while gating append/flush safely.

    - [ ] Plan validation hooks (unit coverage via `recover_existing_segments`, integration test for snapshot -> restart -> replay) and metrics for snapshot age/watermark drift.

    - [ ] Document the design in `aof2_catalog_snapshots.md`, referencing RD5 reader guidance and listing open questions for recovery milestones.

- [ ] **RC2**: Implement startup replay that rebuilds Tier 0/1 manifests, reconciles Tier 2 objects, and schedules hydrations before enabling append.

- [ ] **RC3**: Integrate replicated operation log application into coordinator so followers converge on residency decisions.

- [ ] **RC4**: Handle partial failures (e.g., missing Tier 1 artifact) by queuing downloads from Tier 2 and flagging segments `NeedsRecovery` until satisfied.

- [ ] **RC5**: Add integration tests simulating crash/restart with pending uploads and partially applied log entries.

## Milestone 7 - Observability, Tooling, and Docs

Round out developer/operator experience.

- [ ] **OBS1**: Wire metrics for queue depths, hydration latency, upload retries, and residency state counts.

- [ ] **OBS2**: Emit structured logs for major tier events (promotion, eviction, upload, hydration failures).

- [ ] **OBS3**: Provide admin/debug endpoints or CLI tooling to dump residency maps per stream.

- [ ] **OBS4**: Update developer docs (`design.md`, `aof2_store.md`, samples) to reflect new manager requirements and API pairs.

- [ ] **OBS5**: Author operator runbook covering configuration knobs, scaling guidance, and troubleshooting `WouldBlock` hot spots.

## Milestone 8 - Validation and Rollout

Prove correctness and plan adoption.

- [ ] **VAL1**: Extend existing test harness to run under Tokio, covering append/read under eviction, Tier 2 outages, and recovery scenarios.

- [ ] **VAL2**: Add chaos/failure injection tests (disk full, S3 throttling, forced coordinator restart) ensuring APIs surface actionable errors.

- [ ] **VAL3**: Benchmark sync vs async append/read throughput to confirm back-pressure behaves as expected.

- [ ] **VAL4**: Document rollout steps (feature flagging, staged deployment, telemetry checks) and align with release process.

- [x] **VAL5**: Retire legacy non-tiered code paths and document the unified tiered runtime. (completed 2025-09-30, non-tiered stubs removed)

## Cross-cutting tasks

- [ ] **CC1**: Establish coding guidelines for pairing sync/async APIs (`WouldBlock` contract, naming, documentation notes).

- [ ] **CC2**: Ensure all new futures are cancel-safe and runtime handles are not held across await points that could block shutdown.

- [ ] **CC3**: Audit error enums and map new failure types (Tier 1 compression failure, Tier 2 upload failure, hydration timeout) to actionable diagnostics.

- [ ] **CC4**: Keep this plan in sync with implementation progress; include PR references when checking off tasks.




