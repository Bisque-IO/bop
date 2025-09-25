# Segmented Quorum and Truncation Plan (Reset)

## Goals
- Evaluate partition quorum directly from segmented-log metadata.
- Track master assignment without manifests.
- Maintain truncation and recovery invariants using the segmented durable index plus partition durable checkpoints.

## Data Model
- SegmentedLogStore entries: (raft_log_index, partition_id, partition_log_index, payload).
- Per-partition state includes next_partition_index, highest_durable_partition_index (persisted), quorum rule, master policy, acknowledgement set, quorum flag.
- Global state includes segmented_durable_index and min_required_raft_index = min(partition_min_required).

## Quorum Evaluation
1. When an entry is persisted in the segmented log, initialise ack tracking for that partition/index.
2. Replica acknowledgements update the ack set; evaluate quorum against the partition's policy (majority, threshold, fixed, or custom).
3. Once satisfied, mark the entry and update the partition's minimum required raft index.
4. Unsatisfied entries keep the partition's minimum pointing at the lowest outstanding raft index.

## Master Assignment
- Automatic mode hashes membership with partition_id to choose a replica.
- Fixed mode pins a replica; disabled leaves it unset.
- The chosen master is stored in memory and can be flushed back into segmented metadata if needed for restart.

## Truncation Mechanics
- Segmented durable index increases strictly with segmented bop-aof fsync.
- Partition durable indices are read from partition bop-aof metadata.
- For each partition, determine the oldest raft index still required (pending quorum or apply).
- Global min_required_raft_index = min of those values; truncate segmented bop-aof up to min_required_raft_index - 1.

## Recovery
1. Load partition durable metadata (highest partition index + next counter).
2. Scan the segmented bop-aof from the tail and drop entries beyond each partition's durable checkpoint to align all partitions with raft.
3. Rebuild quorum/master tracking from the remaining entries.
4. Recompute truncation watermark and resume normal operation.

## Testing Checklist
- Quorum edge cases: duplicates, stale acknowledgements, out-of-order replicas, majority vs threshold vs custom policies.
- Master policies: automatic hashing, fixed overrides, disabled mode.
- Truncation: ensure we never drop entries still required by any partition.
- Recovery: simulate partial partition flush and verify rollback to the lowest safe raft index.

This plan complements the reset architecture document and replaces the manifest-driven quorum workflow.
