# Segmented Refactor Plan

## Overview
Refactor  to match the reset architecture:
- Segmented log implemented with bop-aof
- Partition bop-aof per partition
- Segmented/partition state-machine pairing
- Quorum & truncation derived from segmented log
- Removal of manifests

## Phases & Tasks

### Phase 1 – Foundations
1. **Audit current segmented module** (prereq)
   - Note all manifest dependencies (dispatcher/coordinator/log_store/tests).
   - Identify interfaces exported by  used elsewhere.
2. **Define new shared types/config**
   - Update  with new policies (state machine pairing metadata, quorum config, partition settings without manifests).
   - Introduce enums/structs for ack tracking and truncation watermarks.
3. **Plan data persistence**
   - Specify segmented AOF record format.
   - Decide where partition durable metadata lives (likely in partition AOF header/trailer).

#### Phase 1 Audit Findings – Manifest Dependencies
- `crates/bop-raft/src/segmented/log_store.rs:100-161` auto-creates `PartitionManifest` records, gates admission through `SegmentedCoordinator`, and records durability via `PartitionManifestStore`; unit tests at `crates/bop-raft/src/segmented/log_store.rs:392-440` assert manifest log side effects.
- `crates/bop-raft/src/segmented/coordinator.rs:47-154` owns the `PartitionManifestStore`, replays manifests on startup, and persists every durable/applied/master update; associated tests at `crates/bop-raft/src/segmented/coordinator.rs:235-327` validate manifest writes and recovery semantics.
- `crates/bop-raft/src/segmented/dispatcher.rs:9-195` embeds `PartitionManifest` inside each `PartitionContext`, deriving admission windows, quorum bookkeeping, and master assignment entirely from manifest state, emitting `PartitionManifestUpdate` deltas for persistence.
- `crates/bop-raft/src/segmented/manifest.rs:13-228` defines the manifest data model (`durable_index`, `applied_index`, `applied_global`, snapshot, master) plus the replay logic (`apply_update`) that recovery relies on.
- `crates/bop-raft/src/segmented/manifest_store.rs:6-248` wraps `bop_aof::manifest` reader/writer APIs to append and replay manifest deltas; tests at `crates/bop-raft/src/segmented/manifest_store.rs:287-346` exercise log round-trips.
- `crates/bop-raft/src/storage.rs:270-303` wires `PartitionManifestStore` into the storage builder and `SegmentedStorageSettings`, while `crates/bop-raft/src/lib.rs:44-64` re-exports manifest types and helpers, exposing the manifest dependency to downstream crates.

#### Phase 1 Shared Types – Reset Definitions
- `crates/bop-raft/src/segmented/types.rs:33` introduces `PartitionStateMachineId` so partitions can bind directly to their state machine implementations without relying on manifest metadata.
- `crates/bop-raft/src/segmented/types.rs:75-153` defines `SegmentedRecordHeader`/`SegmentedRecordFlags` to standardise the encoded prefix for segmented AOF records.
- `crates/bop-raft/src/segmented/types.rs:299-361` adds `PartitionAckKey`/`PartitionAckStatus` to represent segmented ack tracking state (Raft index, quorum hints, acknowledgement set).
- `crates/bop-raft/src/segmented/types.rs:378-471` introduces `PartitionDescriptor`, `PartitionAofMetadata`, `PartitionDurability`, `PartitionWatermark`, and `SegmentedWatermarks`, covering partition runtime settings together with the truncation watermarks derived from segmented + partition AOF durability.
- `crates/bop-raft/src/segmented/ack_tracker.rs` provides `SegmentedAckTracker` and `AckOutcome` for quorum evaluation without manifests, feeding the new truncation watermarks.
- `crates/bop-raft/src/segmented/coordinator.rs:47-210` now seeds `SegmentedAckTracker` alongside the legacy manifest flow, keeping registrations/durable updates in sync so we can compare manifest vs. segmented watermarks during the transition.
- `crates/bop-raft/src/segmented/coordinator.rs:212-243` bridges replica acknowledgements through both `PartitionDispatcher` and `SegmentedAckTracker`, returning detailed ack outcomes while maintaining the existing manifest-driven quorum evaluation.

#### Phase 1 Data Persistence Plan
- Segmented log records carry a fixed header (`SegmentedRecordHeader`) that includes Raft index, partition id/index, payload length, optional checksum, and extensible flags. Headers are versioned (`SEGMENTED_RECORD_VERSION`) so future upgrades can be negotiated without rewriting the log.
- Partition bop-aof streams persist a compact metadata block (`PartitionAofMetadata`) that tracks the highest durable partition index, the next partition index to allocate, and the last Raft index applied by that partition. Updates occur on durability acknowledgement, ensuring recovery can align partition logs with the segmented durable watermark.
- Recovery/truncation compute global watermarks via `PartitionDurability` + `PartitionWatermark` projections, feeding into `SegmentedWatermarks` so the segmented log can expose `segmented_durable_index` and `min_required_raft_index` directly from replayed metadata.
- Append path: `SegmentedLogStore` will emit header+payload tuples into the segmented AOF and immediately register the raft/partition tuple with `SegmentedAckTracker::record_append` once the append is accepted.
- Durability path: when `record_durable` is invoked, the coordinator updates segmented durability (`update_segmented_durable`) and forwards the decoded payload into the appropriate partition AOF, updating its `PartitionAofMetadata` checkpoint.
- Recovery path: segmented replay rebuilds headers first, drops entries whose partition index exceeds the durable checkpoint recovered from `PartitionAofMetadata`, and replays ack events into `SegmentedAckTracker` to rebuild truncation watermarks before handing entries to partition state machines.

### Phase 2 – SegmentedLogStore rewrite
_Status_: SegmentedLogStore now writes through bop-aof with header encode/decode, registers the segmented ack tracker on append, and updates durability/pack tests.
1. Replace the in-memory  backing with an actual  writer/read stack.
2. Implement record encode/decode (raft index, partition id/index, payload).
3. Provide durable index tracking + min required raft index computation.
4. Update tests to assert durable index monotonicity and pack/apply functions.

### Phase 3 – PartitionLogStore integration
_Status_: PartitionLogStore stores partition payloads in bop-aof, persists metadata, and tests cover durable and reopen flows.
1. Create new module (or extend existing) handling partition-level bop-aof operations.
2. Add logic in segmented log to forward payloads to partition log(s) on durability.
3. Persist highest durable partition index + next index in partition AOF metadata.
4. Write tests verifying partition append, replay, and next-index recovery.

### Phase 4 – Segmented/Partition State Machine Pipeline
1. Implement  covering apply/precommit/rollback hooking into partition state machines.
2. Provide partition state machine trait glue (one in-flight guarantee, completion callbacks).
3. Update coordinator/dispatcher wiring to link state machine completion with quorum updates.
4. Unit tests mocking partition state machines to validate sequencing and backpressure.

### Phase 5 – Quorum & Master Tracking
1. Remove manifest-derived tracker, replace with segmented ack tracker.
2. Integrate membership updates (automatic master hashing, fixed/disabled policies).
3. Ensure ack tracker updates  and interacts with truncation.
4. Tests covering majority/threshold/custom rules and master assignment scenarios.

### Phase 6 – Recovery Workflow
1. Implement segmented/partition AOF scanning and rollback to lowest safe raft index.
2. Restore ack/master state from segmented log; rebuild next indices from partition metadata.
3. Expose recovery entry point () mirroring new flow.
4. End-to-end tests simulating crash/restart with async partition lag.

### Phase 7 – Coordinator & API Surface Cleanup
1. Simplify dispatcher/coordinator API removing manifest references.
2. Update  exports and any downstream consumers.
3. Refresh docs/tests/examples (including new quick start for segmented storage).

### Phase 8 – Removal & Cleanup
1. Delete obsolete manifest modules/tests.
2. Prune configuration options tied to old manifests.
3. Run crate-wide clippy/fmt/test and address warnings.

## Tracking Table
| Phase | Status | Notes |
|-------|--------|-------|
| Phase 1 | done | Audit, shared types, ack tracker + coordinator bridging complete |
| Phase 2 | done | Segmented log now writes via bop-aof with ack tracker instrumentation |
| Phase 3 | in progress | PartitionLogStore persists payloads and metadata; apply/recovery wiring pending |
| Phase 4 | todo | |
| Phase 5 | todo | |
| Phase 6 | todo | |
| Phase 7 | todo | |
| Phase 8 | todo | |

## Dependencies / Concerns
- Ensure downstream crates expecting manifest APIs are updated concurrently.
- Evaluate existing tests relying on manifests; rewrite or retire them.
- Confirm bop-aof supports required metadata hooks (partition durable indices).
- Monitor Windows file locking (AOF force_rollover usage).
