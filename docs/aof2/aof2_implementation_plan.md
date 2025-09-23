# AOF2 Async Tiered Integration Plan

## Next Up

- [x] **AP4**: Rework flush scheduling to interact with tier metadata (durability markers stored alongside residency state).
- [x] **AP5**: Extend metrics to capture Tier 0 admission latency and `WouldBlock` frequency.

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

    - [x] Introduce an in-memory durability cursor on `TieredInstance` that records requested vs durable bytes and exposes helpers to seed state from recovered segments.

    - [x] Seed durability markers during `Aof::segment_snapshot`/`recover_existing_segments` by scanning segment tails so restarts hydrate tier metadata before serving readers.

    - [x] Harden the flush worker with retry + metrics when tier metadata persistence fails and surface fatal cases as `BackpressureKind::Flush`.

    - [x] Add regression coverage for partial flush advancement, restart recovery of the durability marker, and async readers observing the updated watermark before leaving `WouldBlock`.

    - [x] Update `aof2_store.md` and `aof2_design_next.md` to document the flush metadata handshake and the new tier persistence invariants.

- [x] **AP5**: Extend metrics to capture Tier 0 admission latency and `WouldBlock` frequency.

## Milestone 5 - Read Path and Hydration

Support synchronous readers with `WouldBlock` plus async helpers that await hydration.

- [x] **RD1**: Update `AofReader` to fetch segments through `TieredInstance`, returning `WouldBlock` while hydration/promotions run. (completed 2025-09-24, reader uses tier checkout + sealed stream awaits hydration)

- [x] **RD2**: Provide `next_record_async` that awaits hydration futures and resumes iteration seamlessly. (completed 2025-09-24, async sealed stream bridges WouldBlock via tier await)

- [x] **RD3**: Maintain reader-side caches of residency hints to avoid redundant promotions. (completed 2025-09-26, residency hint cache integrated into Aof with unit coverage in aof2::tests::residency_hint_cache_tracks_pending_segments).

- [x] **RD4**: Add tests covering Tier 1 miss -> hydration -> read resume, including concurrent readers. (completed 2025-09-26, async regression test aof2::tests::tier1_hydration_recovers_segment_on_read_miss exercises Tier1 miss hydration with parallel readers.)

- [x] **RD5**: Document reader API changes and guidance for handling `WouldBlock` in downstream services. (`docs/aof2/aof2_read_path.md` now captures notifier usage, retry strategies, pointer bootstrap, and metadata propagation for downstream clients.)

    - [x] Lock the outline (glossary, sync vs async sequences, backpressure table) and circulate to platform reviewers for sign-off before writing. (outline drafted in `docs/aof2/aof2_read_path.md`)

    - [x] Author `docs/aof2/aof2_read_path.md` with sections covering sync vs async flows, handling `BackpressureKind::Hydration`/`Rollover`, sample retry loops, and integration advice for gRPC + HTTP surfaces. (draft content published; awaiting platform review and cross-link pass)

    - [x] Capture concrete examples from `Aof::open_reader{,_async}` and `reader.rs` (sealed stream vs tail follower) showing when to expect `WouldBlock`; embed them as doctest-style snippets.

    - [x] Cross-link the new doc from `aof2_design_next.md` (reader flow diagrams) and `aof2_store.md` (hydration/notification details), adding "See also" callouts that explain when to delegate to async helpers.

    - [x] Provide a migration note in `aof2_progress.md` that links to the reader guidance, highlights behavioural differences from legacy AOF, and enumerates follow-up tasks for downstream services.

    - [x] Once the reader docs land, circulate an RC1/RC2 kickoff checklist and update dependencies here before starting implementation.

## Milestone 6a - Manifest Log and Replay

Replace ad-hoc JSON manifests with the append-only manifest log.

- [x] Close out crash-mid-commit follow-ups for MAN1/MAN3.
    - [x] Capture metrics/alerts expectations in the test assertions to prove replay surfaces chunk/journal lag stats.
        - [x] Inventory the replay metrics we expect (`aof_manifest_replay_chunk_lag_seconds`, `aof_manifest_replay_journal_lag_bytes`) and document pass/fail bounds.
        - [x] Locate or add metric definitions under `rust/bop-rs/src/aof2/metrics/` so the test can read stable names.
        - [x] Extend `manifest_crash_mid_commit` in `rust/bop-rs/tests/aof2_manifest_log.rs:116` to snapshot metrics and assert the counters before/after replay.
        - [x] Ensure the test trims the chunk, triggers replay, and validates metric deltas reset when the test reruns.
        - [x] Add a quick reference to `docs/aof2/README.md` describing how the replay metrics appear in dashboards and alerting.
        - [x] Include Grafana panel IDs or log query examples with the expected replay SLA thresholds (warn vs critical).
    - [x] Document the matrix entry in `tests/README.md` with repro steps for manual validation.
        - [x] Record the command sequence for forcing the crash-mid-commit flow (feature flags, env vars, artifact paths).
        - [x] Note the `cargo test manifest_crash_mid_commit -- --nocapture` invocation plus any env required to surface metrics.
        - [x] Note expected manifest/journal artifacts to collect for manual verification and include the metrics-to-check list.
        - [x] Attach example paths for the trimmed chunk (`/tmp/bop-manifest/...`) and the replay journal files.
        - [x] Cross-link the MAN1/MAN3 manifest crash scenarios from `docs/aof2_manifest_log.md` for context.
        - [x] Highlight where the JSON vs log parity notes live so manual testers understand the fallback path.

- [x] Mature MAN2 by replaying manifest logs into the Tier1 in-memory index and retiring JSON persistence.
    - [x] Implement manifest log replay on `Tier1Instance` startup, seeding the existing index APIs from `ManifestLogReader` output.
        - [x] Build a replay bootstrap that streams `ManifestLogReader` records into the in-memory `Tier1Manifest` before admissions run.
        - [x] Preserve LRU ordering/`used_bytes` when seeding from the log so eviction pressure matches pre-crash state.
        - [x] Handle partially written chunks by trimming to the committed watermark before replay (reuse the crash-mid-commit helper).
        - [x] Emit `ManifestReplayMetrics` snapshots during replay and surface traces for slow segments.
    - [x] Introduce a feature-flagged path that skips JSON writes once replay parity is validated, keeping a fast rollback toggle.
        - [x] Add a `tier1_manifest_log_only` option to `Tier1Config`/`AofManagerConfig` with env override plumbing.
        - [x] Gate the JSON save path behind the flag and assert log parity using a one-shot comparison test before disabling JSON.
        - [x] Document rollback instructions in `docs/aof2_store.md` and surface the flag in `README.md` for operators.
    - [x] Backfill tests that boot from log-only manifests to prove lookups, hydration, and Tier2 promotions survive restart.
        - [x] Create a replay fixture helper in the Tier1 tests that synthesizes manifest log snapshots covering multiple segments.
        - [x] Add a Tokio test that boots `Tier1Instance` in log-only mode, hydrates from the fixture, and exercises lookup/promote paths.
        - [x] Simulate a truncated chunk during startup (see `manifest_log_only_replay_bootstrap`) to ensure recovery surfaces actionable errors and metrics increments.

- [x] Expand MAN3 coverage with tooling and observability.
    - [x] Add a `manifest inspect` developer utility (or test helper) that dumps chunk headers, record counts, and Tier2 object keys for a stream.
    - [x] Emit and validate metrics for replay duration, chunk count, and journal lag inside the crash/recovery integration test.
    - [x] Surface corruption alerts by injecting a CRC failure in tests and asserting the error path increments the new counters.

- [x] Round out MAN4 integration tests for Tier2 compaction and document the crash matrix (new retry flow covered by `manifest_tier2_retry_flow` and corresponding operator docs).
    - [x] Extend the manifest log test matrix with a Tier2 retry scenario that forces delete/upload failures and confirms recovery (`rust/bop-rs/tests/aof2_manifest_log.rs` now exercises flaky uploads/deletes and validates manifest replay).
    - [x] Update `tests/README.md` with repro steps for the crash-mid-commit + Tier2 retry cases and link to `aof2_manifest_log.md` (see the MAN3 entry under *AOF2 Manifest Log Tests*).
    - [x] Add a short operator note to `aof2_store.md` summarising the new coverage and how to run the replay diagnostics (see the "Tier2 retry diagnostics" subsection).


- [x] **MAN1**: Implement chunked manifest log writer/reader with crc64-nvme checksums and mmap-backed append path. (landed `aof2::manifest::{chunk, record, reader, writer}` modules with rotation + CRC validation; see `rust/bop-rs/src/aof2/manifest/`)

    - [x] Support chunk rotation (16 KiB to 1 MiB configurable), sealing, and Tier 2 upload streaming hooks. (`ManifestLogWriter` handles rotation/ sealing and surfaces `SealedChunkHandle` for Tier 2 streaming)

    - [x] Provide binary record schemas for seal/compression/upload/eviction events and end-to-end crc64 validation. (`ManifestRecordPayload` encodes all tier transitions with per-record CRC checks)

- [x] **MAN2**: Integrate the log with `Tier1Instance` replacing `Tier1Manifest` JSON loading/saving. (completed 2025-10-07, Tier1 boot now replays MAN1 chunks into the live manifest map while JSON snapshots are gated behind the `tier1_manifest_log_only` rollback flag.)

    - [x] Maintain in-memory manifest index fed by log replay and expose existing lookup APIs. (`Tier1InstanceState::replay_manifest_from_log` hydrates the manifest map before residency updates so callers continue using the existing lookup surface.)

    - [x] Remove the dormant JSON manifest loader/saver and keep the new log gated behind a feature flag until Tier 1 burn-in completes. (`Tier1Manifest::persist_json` only runs when the rollback flag is enabled; log replay is authoritative by default and the flag is plumbed through `Tier1Config::tier1_manifest_log_only`.)

- [x] **MAN3**: Add recovery replay that loads the latest manifest snapshot then applies log chunks, reconciling Tier 0/Tier 1/Tier 2 state (Tier 1 bootstrap now replays the manifest log, seeds the in-memory index, and exposes replay metrics snapshots).

    - [x] Provide tooling to inspect manifest chunks and verify Tier 2 object presence via the `ManifestInspector` helper in `rust/bop-rs/src/aof2/manifest/inspect.rs`, which reports chunk headers, record counts, CRC parity, and Tier 2 keys for developers and tests.

    - [x] Emit metrics (chunk count, replay time, journal lag) and alerts for corruption using `ManifestReplayMetrics` (`rust/bop-rs/src/aof2/metrics/manifest_replay.rs`) wired through `Tier1InstanceState::replay_manifest_from_log` and validated by the crash/corruption scenarios in `rust/bop-rs/tests/aof2_manifest_log.rs`.

- [x] **MAN4**: Update documentation (`aof2_manifest_log.md`, `aof2_store.md`) and integration tests to cover crash/restart with pending chunks and Tier 2 compaction (Tier2 retry flow captured in `manifest_tier2_retry_flow`, docs refreshed for operators).

    - [x] Publish the manifest log spec in `aof2_manifest_log.md` and cross-link it from `aof2_store.md`.

    - [x] Extend integration tests to exercise crash/restart with pending chunks and Tier 2 compaction (see `manifest_crash_mid_commit`).

## Milestone 6 - Recovery and Operation Log

Ensure startup replay and replicated actions respect the tiered architecture.

- [x] **RC1**: Embed catalog and durability metadata in segment headers/footers and retire durability snapshots.
    - [x] Add `ext_id` to segment header, record header, and footer (defaulting to zero until callers supply a value). (completed 2025-09-30, segments now embed ext_id end-to-end)
    - [x] Persist durable bytes, record count, coordinator watermark, and the flush-failure flag in the sealing footer. (completed 2025-09-30, sealing writes enriched footer and tests updated)
    - [x] Document the design in `docs/aof2/aof2_catalog_snapshots.md` and extend unit tests (`aof2::segment`, `aof2::tests`) to validate the footer metadata and recovery flow. (completed 2025-10-03)

- [x] **RC2**: Introduce the atomic current.sealed pointer used during recovery.
    - [x] Write the pointer via `TempFileGuard` + rename after each successful seal. (`Layout::store_current_sealed_pointer`, exercised by `aof2::fs` tests)
    - [x] Document crash-consistency requirements and verify restart logic consumes the pointer before scanning segments. (`aof2::tests::pointer_*` scenarios cover nominal, missing, and mismatched pointers)

- [x] **RC3**: Align manifest replay with the enriched segment metadata.
    - [x] Include ext_id, coordinator watermark, and flush-failure in manifest entries and `SealSegment` log records. (see `Tier1Manifest` updates and `tests/aof2_manifest_log.rs`)
    - [x] Replay now prefers header/footer metadata seeded via the pointer, using manifest data only for Tier1/Tier2 residency. (implemented in `recover_existing_segments` with pointer guarded fallbacks)

- [x] **RC4**: Handle partial failures (e.g., missing Tier 1 artifact) by queuing downloads from Tier 2 and flagging segments `NeedsRecovery` until satisfied. (`Tier1ResidencyState` now includes `NeedsRecovery`, `Tier2Handle::schedule_fetch` enqueues fetch jobs, and `Tier1InstanceState::hydrate_segment_blocking` schedules fetch + logs `HydrationFailed` when warm files go missing.)

- [x] **RC5**: Add integration tests simulating crash/restart with pending uploads and partially applied log entries. (New tier1 tests cover missing warm recovery, fetch completion, and manifest replay preserving `NeedsRecovery`, ensuring restart paths honour queued fetches.)

## Milestone 7 - Observability, Tooling, and Docs

Round out developer/operator experience.

- [x] **OBS1**: Wire metrics for queue depths, hydration latency, upload retries, and residency state counts. *(TieredObservabilitySnapshot + expanded Tier0/1/2 snapshots)*

- [x] **OBS2**: Emit structured logs for major tier events (promotion, eviction, upload, hydration failures). *(Targets `aof2::tiered`, `aof2::tier0`, `aof2::tier1`, `aof2::tier2`)*

- [x] **OBS3**: Provide admin/debug endpoints or CLI tooling to dump residency maps per stream. *(`cargo run --bin aof2-admin -- dump --root <path>`)*

- [x] **OBS4**: Update developer docs (`design.md`, `aof2_store.md`, samples) to reflect new manager requirements and API pairs.

- [x] **OBS5**: Author operator runbook covering configuration knobs, scaling guidance, and troubleshooting `WouldBlock` hot spots.

## Milestone 8 - Validation and Rollout

Prove correctness and plan adoption.

- [x] **VAL1**: Extend existing test harness to run under Tokio, covering append/read under eviction, Tier 2 outages, and recovery scenarios. (2025-10-05) — New `aof2_failure_tests.rs` suite drives metadata flush retries plus Tier 2 upload/delete/fetch retries using deterministic failure injection.

- [x] **VAL2**: Add chaos/failure injection tests (disk full, S3 throttling, forced coordinator restart) ensuring APIs surface actionable errors. (2025-10-05) — Failure injection helpers live in `rust/bop-rs/src/aof2/test_support.rs`; the integration suite asserts metrics/log behaviour under forced errors.

- [ ] **VAL3**: Benchmark sync vs async append/read throughput to confirm back-pressure behaves as expected. Initial `aof2_append_metrics` Criterion bench added (2025-10-05); hydration load bench pending.

- [x] **VAL4**: Document rollout steps (feature flagging, staged deployment, telemetry checks) and align with release process. (2025-10-05) — Runbook now includes validation checklist and benchmark commands; store/read-path docs reference the new harness.

- [x] **VAL5**: Retire legacy non-tiered code paths and document the unified tiered runtime. (completed 2025-09-30, non-tiered stubs removed)

## Cross-cutting tasks

- [ ] **CC1**: Establish coding guidelines for pairing sync/async APIs (`WouldBlock` contract, naming, documentation notes).

- [ ] **CC2**: Ensure all new futures are cancel-safe and runtime handles are not held across await points that could block shutdown.

- [ ] **CC3**: Audit error enums and map new failure types (Tier 1 compression failure, Tier 2 upload failure, hydration timeout) to actionable diagnostics.

- [ ] **CC4**: Keep this plan in sync with implementation progress; include PR references when checking off tasks.




