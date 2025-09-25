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
