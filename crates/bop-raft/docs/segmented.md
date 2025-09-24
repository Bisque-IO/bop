# Segmented Log Store Architecture

## Overview
This document describes the segmented log store and segmented state machine architecture for `bop-raft`. The goal is to run many independent partition logs and state machines on top of a single parent Raft log. Each partition owns its own durable history, checkpoints, and apply pipeline while still benefiting from the ordering, replication, and fault tolerance of the shared Raft cluster. All partition logs are implemented with `bop-aof`.

The parent Raft log remains compact: every entry encodes the destination partition plus the metadata needed to locate the full payload in the partition's append-only log. Partitions ingest, persist, and apply entries asynchronously. They may advance their local applied index at their own pace while feeding progress updates back to Raft so higher layers can enforce consistency requirements (for example, cross-partition barriers).

## Goals
- Keep the parent Raft log lightweight by storing only partition routing metadata while offloading payload durability to partition-specific `bop-aof` logs.
- Allow each partition to run its state machine asynchronously without violating Raft ordering or durability guarantees.
- Support fast per-partition recovery using local manifests and snapshots while maintaining deterministic replay across the cluster.
- Expose explicit watermarks (`durable`, `applied`) so clients and orchestration layers can wait for the level of durability they require.
- Provide backpressure hooks that prevent unbounded backlog growth per partition and across the cluster.
- Surface metrics and inspection endpoints so operators can reason about per-partition health.

## Non-Goals
- Changing the underlying Raft consensus protocol or its safety properties.
- Providing cross-partition transactional semantics beyond barrier-based coordination (`wait for partition X to apply global index Y`).
- Introducing a new storage backend; partitions must use `bop-aof` and its tiered storage pipeline.
- Guaranteeing completely independent scheduling across partitions when shared resources (CPU, disk) are exhausted.

## Terminology
| Term | Description |
| --- | --- |
| `global_log_index` | Index of an entry in the parent Raft log. |
| `global_commit_index` | Highest `global_log_index` known to be committed by Raft. |
| `partition_id` | Stable identifier for a partition/segment. |
| `partition_log_index` | Monotonic index within a partition's `bop-aof` log. |
| `partition_durable_index` | Highest partition entry confirmed durable in `bop-aof`. |
| `partition_applied_index` | Highest partition entry whose state machine side effects completed. |
| `partition_snapshot_index` | `partition_log_index` captured in the latest durable snapshot. |
| `applied_global_index` | Highest `global_log_index` this partition has applied; used for cross-partition barriers. |

## System Components
### Global Raft Log (Parent)
- Stores lightweight entries containing `{partition_id, partition_log_index, payload_digest, term_hint}`.
- Drives replication, leader election, and commit as usual; an entry is considered committed when the Raft quorum acknowledges it.
- Emits commit notifications to the dispatcher, which routes them to the appropriate partition pipeline.

### Partition Log Stores (`bop-aof`)
- Each partition is backed by its own `bop-aof::Aof` instance configured via the shared `AofManager`.
- The partition log owns the full payload bytes as well as auxiliary metadata (checksums, timestamps) required for recovery and validation.
- `bop-aof` tiers (memory/SSD/remote) remain in play; compaction and flush policies can be tuned per partition.

### Partition State Machines
- Execute deterministic logic for the partition and emit side effects (e.g., storage mutations, network notifications).
- Consume entries strictly in partition-log order, but execute asynchronously relative to other partitions and the parent Raft commit stream.
- Report completion via an `AppliedUpdate` channel carrying `{partition_id, partition_log_index, applied_global_index}`.

### Dispatcher & Coordination Layer
- Runs on the Raft leader and followers to keep partition logs aligned with the parent log.
- Responsibilities:
  1. Admission control prior to appending to Raft.
  2. Durability handoff: ensure partition payloads are written to `bop-aof` either before or immediately after the parent entry commits (strategy configurable).
  3. Queue management: append committed entries to per-partition queues for execution workers.
  4. Watermark aggregation: track per-partition `durable`/`applied` indices and surface aggregate views (e.g., minimum across a set of partitions).

### Metadata Surfaces
- **Partition manifest**: stored alongside the `bop-aof` data, contains the latest durable/applied/global indices and snapshot references.
- **Raft-side mapping**: in-memory table `{partition_id -> PartitionContext}` with cached watermarks, backlog metrics, and routing information.
- Both surfaces must be replayable deterministically from durable state so that followers and leaders converge on the same view.

## Index & Watermark Model
### Global Indices
- `global_commit_index` continues to advance only when Raft commits an entry.
- `global_durable_index` is synonymous with `global_commit_index` because durability is tied to Raft commit.
- `global_applied_index` can be derived as `min(partition_applied_index over participating partitions)` when a global view is needed (e.g., cluster-wide snapshotting).

### Per-Partition Indices
- `partition_log_index` increments with each accepted entry for that partition; stored in the parent entry payload for idempotent replay.
- `partition_durable_index` advances once `bop-aof` confirms the entry is persisted (after fsync/pipeline guarantees based on config).
- `partition_applied_index` advances when the state machine reports success.
- `applied_global_index` mirrors the parent `global_log_index` each partition has processed; used to fulfil barrier waits.

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

## Open Questions
- Do we require dynamic partition creation/destruction at runtime, and how does that interact with manifest lifecycle?
- Should we allow configurable ordering guarantees (e.g., strict commit after apply) for select partitions that need linearizable visibility?
- How do we authenticate barrier waiters and enforce quotas so they cannot DoS the system with long waits?
- What policy governs leader-driven rebalancing if a partition chronically lags (e.g., automatic throttling vs. eviction)?
