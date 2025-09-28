# Segmented Storage Architecture (Reset Plan)

This document captures the reset design for the segmented log pathway. We want a single segmented log implemented with bop-aof plus one bop-aof per partition. Each partition log is paired with its state machine. All quorum and truncation decisions are derived directly from the segmented log; manifests disappear.

## Objectives
- Keep everything on bop-aof: the segmented log is an AOF, every partition log is an AOF, durability is uniform.
- Route raft entries into the correct partition log while continuing to satisfy the NuRaft LogStoreInterface contract.
- Execute partition state machines asynchronously with no more than one in-flight entry per partition.
- Maintain quorum and master metadata using only segmented-log information.
- Track a monotonic segmented durable index and a global minimum required raft index for truncation and recovery.

## Components
### SegmentedLogStore
- Implements the NuRaft LogStoreInterface.
- Persists raft entries to a segmented bop-aof; each entry carries partition_id, partition_log_index, and the user payload.
- Maintains an ordered durable index that advances only after the segmented bop-aof fsync.
- Computes quorum/master state plus the minimum required raft index from in-memory metadata.
- Serves raft read/pack/apply requests straight from the segmented AOF.

### PartitionLogStore
- A bop-aof per partition.
- Receives decoded entries from SegmentedLogStore and stores the payload bytes.
- Persists the highest durable partition index so recovery knows the next index to allocate.

### SegmentedStateMachine and PartitionStateMachine
- SegmentedStateMachine is the only state machine registered with Raft. It decodes entries and hands them to the PartitionStateMachine tied to that partition.
- Enforces one in-flight apply per partition (mirrors the bop-aof notify queue).
- PartitionStateMachine performs user logic and acknowledges completion back to the segmented layer.

### Quorum and Master Tracking
- Quorum state is keyed by partition_id + partition_log_index, updated when segmented entries land and when replica acknowledgements arrive.
- Master assignments are deterministic (hash of membership + partition) or fixed per policy; disabled mode leaves the partition unowned.
- Tracking data is used to decide eligibility for truncation and to publish external state.

### Truncation Watermarks
- Segmented durable index: highest raft index flushed into segmented bop-aof.
- Per-partition durable index: highest partition index flushed into its bop-aof.
- For each partition, compute the raft index still required (oldest quorum/pending apply). Global minimum required raft index is the min over all partitions. Truncate the segmented log up to that minus one.

## Recovery Workflow
1. Open all partition bop-aof instances and read their highest durable partition indices and next-partition counters.
2. Open the segmented bop-aof, scan backward, and drop any entries whose partition index exceeds that partition's durable checkpoint (rollback to the lowest safe raft index).
3. Rebuild acknowledgement and master state from the remaining segmented entries; recompute the truncation watermark.
4. Resume raft service and partition apply loops with consistent indices across the cluster.

## Data Flow
1. Raft append: SegmentedLogStore::append encodes the partition header and persists to the segmented bop-aof.
2. Durability: when the segmented entry is fsynced, decode it, forward the payload to the corresponding PartitionLogStore, and update partition durable metadata.
3. Apply: SegmentedStateMachine schedules partition applies (one at a time per partition) and waits for completion before scheduling the next entry.
4. Quorum: replica acknowledgements update the partition's ack set in the segmented ack tracker; once satisfied we mark the entry for truncation and downstream visibility.
5. Truncation: recompute the minimum required raft index via the ack tracker and compact the segmented bop-aof as allowed.

## Current Progress (Phase 1)
- Shared segmented types now capture record headers, partition descriptors, and truncation watermarks (`SegmentedRecordHeader`, `PartitionDescriptor`, `SegmentedWatermarks`).
- `SegmentedAckTracker` scaffolding exists alongside the legacy manifest flow; the coordinator registers partitions, records durable indices, and exposes watermarks for upcoming truncation logic.
- `PartitionDispatcher` surfaces `PartitionDescriptor` snapshots so the new ack tracker can be driven without manifest state.
- Coordinator acknowledgement plumbing now hits both the legacy quorum tracker and `SegmentedAckTracker`, giving us side-by-side visibility before manifest removal.

## Immediate Next Steps
- Drive dispatcher/coordinator acknowledgement updates through both the legacy manifest metadata and `SegmentedAckTracker`, validating parity before manifest removal.
- Implement the segmented bop-aof append/read helpers using the shared header struct and begin forwarding durable payloads into partition log stores.
- Outline recovery plumbing that replays segmented headers, aligns partition metadata, and repopulates the ack tracker before state-machine replay.

## Testing Strategy
- Unit coverage for segmented encode/decode, quorum math, and truncation calculations.
- Integration tests that drive multiple partitions through append/apply cycles, verifying correct routing, quorum decisions, master assignments, and truncation.
- Recovery tests that simulate partial partition flushes and ensure we rollback to the lowest safe raft index and continue without duplicates.

Next steps: implement the new segmented log/partition log/state-machine pipeline, remove manifest dependencies, and wire quorum/truncation to the segmented log metadata.
### Invariants
- For any partition: `partition_snapshot_index <= partition_applied_index <= partition_durable_index <= highest_partition_log_index`.
- Parent entry `{partition_id, partition_log_index}` must match the actual persisted record in the partition log. Replays verify this tuple before appending to avoid duplicate writes.
- Global ordering is preserved: even though partitions apply independently, the parent log remains a single total order, ensuring consistent replay on new replicas.

## Write Path
1. **Admission**: client requests are classified by `partition_id`; dispatcher checks per-partition and global backlog thresholds. If limits are exceeded, the request is rejected immediately with an overload error.
2. **Prepare**: the dispatcher reserves the next `partition_log_index` and prepares the payload for `bop-aof` append (checksum, compression metadata, etc.).
3. **Persist (optional eager mode)**: depending on configuration, the payload may be appended to the partition log before Raft append (write-ahead) or after Raft commit (write-behind). In either case, the combination `{partition_id, partition_log_index}` is ready before the parent entry is made visible to followers.
4. **Raft Append**: the parent log receives an entry containing routing metadata and payload pointer information. Followers replicate it as a normal Raft entry.
5. **Commit**: once the entry is committed (`global_commit_index` updated), the dispatcher confirms the partition payload is durable (triggering flush if necessary) and updates `partition_durable_index`.
6. **Enqueue for Apply**: the partition worker receives the new entry (payload reference via `bop-aof`) and begins state machine execution.

## Apply Path
- Each partition has an async worker (or worker pool) that processes queued entries sequentially.
- Execution uses zero-copy access via `bop-aof::SegmentReader` when possible.
- After successful apply, the worker updates the manifest (`partition_applied_index`, `applied_global_index`) and emits a progress event to Raft observers or waiters.
- Failures trigger retry/backoff logic; repeated failures may pause the partition and propagate an alert while keeping the parent index committed but unapplied.

## Cross-Partition Coordination
- Higher-level services implement barriers by issuing per-partition commands and then waiting until each targeted partition reports `applied_global_index >= target_global_index`.
- Waiting can be serviced from in-memory watermarks; if a partition lags, waiters may block or time out.
- No attempt is made to provide atomic multi-partition commits; the barrier mechanism offers eventual visibility once all partitions caught up.

## Recovery & Replay
1. **Load manifests**: on startup, load each partition's manifest to recover `partition_snapshot_index`, `partition_durable_index`, and `partition_applied_index`.
2. **Restore snapshots**: apply the latest partition snapshot to rebuild in-memory state.
3. **Rebuild logs**: replay `bop-aof` records newer than the snapshot up to the partition's `partition_durable_index`.
4. **Raft replay**: as Raft replays committed entries, verify the `{partition_id, partition_log_index}` tuple. If the partition already persisted the record, skip re-append; otherwise append/idempotently repair.
5. **Catch up apply**: enqueue any entries whose `partition_applied_index < partition_durable_index` for execution.
6. **Resume service**: once all partitions reached their durable/applied state, the node serves read/write traffic.

Recovery must be deterministic: followers must reach the same partition manifests given the same parent log. Manifest updates are therefore ordered with the partition's `partition_durable_index` and flushed atomically with durability updates.

## Snapshotting & Compaction
- **Partition snapshots** capture the state machine plus `partition_log_index` and corresponding `global_log_index`. Metadata records the parent Raft term/index to validate compatibility during install.
- **Parent snapshots** can truncate the Raft log once every partition referenced entry ≤ global snapshot watermark is safely snapshotted.
- **Partition log compaction** is allowed to drop segments older than `partition_snapshot_index` once all referencing parent entries are beyond the global snapshot point.
- Coordinated snapshotting ensures we never mix snapshots taken from different parent indices.

## Backpressure & Flow Control
- Admission checks happen before Raft append. Limits include queued operations, queued bytes, max age of oldest unapplied entry, and optional custom signals.
- A second guard after commit but before routing prevents sudden backlog spikes by rejecting entries that cannot be enqueued safely.
- Rejections return retryable errors that encode the offending partition and limit; clients use exponential backoff.
- Large requests may be chunked so that small operations continue to make progress even when near the limit.

## Failure Handling
- **Partition worker stall**: dispatcher marks the partition as throttled, pauses new admissions for that partition, and optionally advances a "frozen" flag to clients.
- **Durability failure**: if `bop-aof` append/flush fails, the parent entry remains committed but unapplied; recovery logic attempts replay after the fault clears. Severe corruption escalates to operator intervention.
- **Node crash**: manifests plus `bop-aof` logs guarantee reconstruction. Unapplied but durable entries replay via the apply path after restart.
- **Leader change**: followers already possess durable partition metadata via `bop-aof`; the new leader resumes admission after re-establishing per-partition indices from manifests and Raft commit state.

## Prototype Sketches
These sketches outline minimal data structures and helper logic that we can exercise in isolation tests before wiring the runtime. They are intentionally lightweight but enforce the critical invariants described above.

### Partition Manifest Prototype
```rust
type PartitionId = u64;
type PartitionLogIndex = u64;
type GlobalLogIndex = u64;

#[derive(Debug, Clone)]
struct PartitionManifest {
    partition_id: PartitionId,
    durable_index: PartitionLogIndex,
    applied_index: PartitionLogIndex,
    applied_global: GlobalLogIndex,
    snapshot_index: Option<PartitionLogIndex>,
}

impl PartitionManifest {
    fn new(partition_id: PartitionId) -> Self {
        Self {
            partition_id,
            durable_index: 0,
            applied_index: 0,
            applied_global: 0,
            snapshot_index: None,
        }
    }

    fn update_durable(&mut self, log_index: PartitionLogIndex, global: GlobalLogIndex) {
        assert!(log_index >= self.durable_index);
        self.durable_index = log_index;
        self.applied_global = self.applied_global.max(global);
        self.check_invariants();
    }

    fn update_applied(&mut self, log_index: PartitionLogIndex, global: GlobalLogIndex) {
        assert!(log_index <= self.durable_index);
        assert!(global >= self.applied_global);
        self.applied_index = log_index;
        self.applied_global = global;
        self.check_invariants();
    }

    fn set_snapshot(&mut self, log_index: PartitionLogIndex) {
        assert!(log_index <= self.applied_index);
        self.snapshot_index = Some(log_index);
        self.check_invariants();
    }

    fn check_invariants(&self) {
        if let Some(snapshot) = self.snapshot_index {
            assert!(snapshot <= self.applied_index);
        }
        assert!(self.applied_index <= self.durable_index);
    }
}
```

A thin property-test harness that repeatedly applies random `update_durable`, `update_applied`, and `set_snapshot` calls can validate that invariants hold even under interleaved operations and dynamic partition lifecycle events.

### Dispatcher Prototype
```rust
use std::collections::HashMap;

#[derive(Debug)]
enum AdmissionError {
    PartitionMissing,
    Backpressure,
}

struct PartitionContext {
    manifest: PartitionManifest,
    next_partition_index: PartitionLogIndex,
    backlog_len: usize,
    max_backlog: usize,
}

impl PartitionContext {
    fn new(partition_id: PartitionId, max_backlog: usize) -> Self {
        Self {
            manifest: PartitionManifest::new(partition_id),
            next_partition_index: 1,
            backlog_len: 0,
            max_backlog,
        }
    }
}

struct Dispatcher {
    partitions: HashMap<PartitionId, PartitionContext>,
}

impl Dispatcher {
    fn admit(&mut self, partition_id: PartitionId) -> Result<PartitionLogIndex, AdmissionError> {
        let ctx = self.partitions.get_mut(&partition_id).ok_or(AdmissionError::PartitionMissing)?;
        if ctx.backlog_len >= ctx.max_backlog {
            return Err(AdmissionError::Backpressure);
        }
        let index = ctx.next_partition_index;
        ctx.next_partition_index += 1;
        ctx.backlog_len += 1;
        Ok(index)
    }

    fn record_durable(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) {
        let ctx = self.partitions.get_mut(&partition_id).expect("partition exists");
        ctx.manifest.update_durable(partition_index, global_index);
        ctx.backlog_len = ctx.backlog_len.saturating_sub(1);
        ctx.next_partition_index = ctx.next_partition_index.max(partition_index + 1);
    }

    fn record_applied(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) {
        let ctx = self.partitions.get_mut(&partition_id).expect("partition exists");
        ctx.manifest.update_applied(partition_index, global_index);
    }

    fn global_applied_floor(&self) -> GlobalLogIndex {
        self.partitions
            .values()
            .map(|ctx| ctx.manifest.applied_global)
            .min()
            .unwrap_or(0)
    }
}
```

This dispatcher skeleton makes it straightforward to write targeted tests for admission control, backlog enforcement, and the relationship between global and partition-local indices. We can evolve it into the production dispatcher once the invariants are thoroughly validated.

## Observability & Metrics
Expose per-partition and aggregate metrics:
- `partition_enqueued_ops`, `partition_enqueued_bytes`, `partition_backlog_age_ms`.
- `partition_durable_index`, `partition_applied_index`, `partition_applied_global`.
- Flush statistics from `bop-aof` (fsync latency, retry counts).
- Dispatcher admission counters (rejects, retries).
- Raft-side metrics linking partition health to leader decisions.

Tracing hooks should annotate stages of the write/apply pipeline per partition to aid debugging of tail latency issues.

## Testing & Validation Strategy
- **Unit tests** for manifest handling, index arithmetic, and idempotent replay of `{partition_id, partition_log_index}` tuples.
- **Integration tests** running multiple partitions against an in-memory Raft harness to validate asynchronous apply, barrier waits, and recovery.
- **Fault injection** to simulate `bop-aof` flush failures, worker stalls, and admission overruns.
- **Performance benchmarks** measuring ingest latency under mixed partition workloads and verifying isolation.
- **Replay tests** ensuring logs plus manifests restore identical state across restarts.

## Open Questions w/ Answers
- **Do we require dynamic partition creation/destruction at runtime, and how does that interact with manifest lifecycle?**  
  Yes. Partition manifests must support creation on first write, persistence of metadata alongside `bop-aof`, and garbage collection when the partition is torn down so that disk state tracks the dynamic roster.

- **Should we allow configurable ordering guarantees (e.g., strict commit after apply) for select partitions that need linearizable visibility?**  
  No. Higher layers that need stricter visibility can build atop the exposed watermarks, keeping the core segmented storage simple.

- **How do we authenticate barrier waiters and enforce quotas so they cannot DoS the system with long waits?**  
  Barrier waiting will be asynchronous and implemented as a lightweight pub/sub channel so listeners register interest cheaply; admission to that channel can be rate-limited per client to avoid abuse.

- **What policy governs leader-driven rebalancing if a partition chronically lags (e.g., automatic throttling vs. eviction)?**  
  Each partition configuration supplies a maximum tolerated lag; once exceeded, the dispatcher rejects new entries for that partition until it catches up or operators intervene.
