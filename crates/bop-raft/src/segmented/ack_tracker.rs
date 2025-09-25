//! Ack tracking for the segmented Raft pipeline.

use std::collections::{BTreeMap, HashMap};

use crate::segmented::types::{
    GlobalLogIndex, PartitionAckKey, PartitionAckStatus, PartitionDescriptor, PartitionId,
    PartitionLogIndex, PartitionWatermark, ReplicaId, SegmentedWatermarks,
};

/// Result describing an acknowledgement update for a partition entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AckOutcome {
    pub partition_id: PartitionId,
    pub partition_index: PartitionLogIndex,
    pub raft_index: GlobalLogIndex,
    pub ack_count: usize,
    pub quorum_satisfied: bool,
    pub newly_satisfied: bool,
}

/// Errors surfaced by the segmented ack tracker.
#[derive(Debug)]
pub enum AckError {
    PartitionMissing(PartitionId),
    EntryMissing {
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
    },
}

impl std::fmt::Display for AckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AckError::PartitionMissing(pid) => write!(f, "partition {pid} not registered"),
            AckError::EntryMissing {
                partition_id,
                partition_index,
            } => write!(
                f,
                "partition {partition_id} entry {partition_index} not tracked",
            ),
        }
    }
}

impl std::error::Error for AckError {}

#[derive(Debug)]
struct PartitionState {
    descriptor: PartitionDescriptor,
    pending: BTreeMap<PartitionLogIndex, PartitionAckStatus>,
    min_required_raft_index: GlobalLogIndex,
}

impl PartitionState {
    fn new(descriptor: PartitionDescriptor, fallback: GlobalLogIndex) -> Self {
        Self {
            descriptor,
            pending: BTreeMap::new(),
            min_required_raft_index: fallback,
        }
    }

    fn recompute_min_required(&mut self, fallback: GlobalLogIndex) {
        self.min_required_raft_index = self
            .pending
            .values()
            .filter(|status| !status.quorum_satisfied)
            .map(|status| status.global_index)
            .min()
            .unwrap_or(fallback);
    }
}

/// Tracks per-partition acknowledgement state, exposing truncation watermarks derived
/// from partition quorum progress.
#[derive(Debug, Default)]
pub struct SegmentedAckTracker {
    partitions: HashMap<PartitionId, PartitionState>,
    segmented_durable_index: GlobalLogIndex,
}

impl SegmentedAckTracker {
    /// Create a tracker with no registered partitions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace a partition descriptor.
    pub fn upsert_partition(&mut self, descriptor: PartitionDescriptor) {
        let partition_id = descriptor.partition_id;
        let state = PartitionState::new(descriptor, self.segmented_durable_index);
        self.partitions.insert(partition_id, state);
    }

    /// Remove a partition from the tracker.
    pub fn remove_partition(&mut self, partition_id: PartitionId) {
        self.partitions.remove(&partition_id);
    }

    /// Record that a new entry has been appended for the partition.
    pub fn record_append(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        raft_index: GlobalLogIndex,
    ) -> Result<(), AckError> {
        let state = self
            .partitions
            .get_mut(&partition_id)
            .ok_or(AckError::PartitionMissing(partition_id))?;

        let key = PartitionAckKey::new(partition_id, partition_index);
        let entry = state
            .pending
            .entry(partition_index)
            .or_insert_with(|| PartitionAckStatus::new(key, raft_index, &state.descriptor.quorum));

        if raft_index > entry.global_index {
            *entry = PartitionAckStatus::new(key, raft_index, &state.descriptor.quorum);
        }

        state.recompute_min_required(self.segmented_durable_index);
        Ok(())
    }

    /// Record an acknowledgement from `replica` for the supplied partition entry.
    pub fn acknowledge(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        raft_index: GlobalLogIndex,
        replica: ReplicaId,
    ) -> Result<AckOutcome, AckError> {
        let state = self
            .partitions
            .get_mut(&partition_id)
            .ok_or(AckError::PartitionMissing(partition_id))?;

        let (ack_count, raft_index, quorum_satisfied, newly_satisfied) = {
            let status = state
                .pending
                .get_mut(&partition_index)
                .ok_or(AckError::EntryMissing {
                    partition_id,
                    partition_index,
                })?;

            if raft_index > status.global_index {
                let key = PartitionAckKey::new(partition_id, partition_index);
                *status = PartitionAckStatus::new(key, raft_index, &state.descriptor.quorum);
            }

            let was_satisfied = status.quorum_satisfied;
            if state.descriptor.quorum.contains(&replica) {
                status.acknowledgements.insert(replica);
            }
            status.quorum_satisfied = state.descriptor.quorum.evaluate(&status.acknowledgements);

            (
                status.acknowledgements.len(),
                status.global_index,
                status.quorum_satisfied,
                status.quorum_satisfied && !was_satisfied,
            )
        };

        if newly_satisfied || !quorum_satisfied {
            state.recompute_min_required(self.segmented_durable_index);
        }

        Ok(AckOutcome {
            partition_id,
            partition_index,
            raft_index,
            ack_count,
            quorum_satisfied,
            newly_satisfied,
        })
    }

    /// Release a partition entry after it is no longer required (applied or truncated).
    pub fn release_entry(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
    ) -> Result<(), AckError> {
        let state = self
            .partitions
            .get_mut(&partition_id)
            .ok_or(AckError::PartitionMissing(partition_id))?;

        if state.pending.remove(&partition_index).is_none() {
            return Err(AckError::EntryMissing {
                partition_id,
                partition_index,
            });
        }
        state.recompute_min_required(self.segmented_durable_index);
        Ok(())
    }

    /// Update the segmented durable watermark and recompute partition minima.
    pub fn update_segmented_durable(&mut self, raft_index: GlobalLogIndex) {
        if raft_index <= self.segmented_durable_index {
            return;
        }
        self.segmented_durable_index = raft_index;
        for state in self.partitions.values_mut() {
            state.recompute_min_required(self.segmented_durable_index);
        }
    }

    /// Segmented durable index currently tracked by the coordinator.
    pub fn segmented_durable_index(&self) -> GlobalLogIndex {
        self.segmented_durable_index
    }

    /// Minimum Raft index still required for the supplied partition.
    pub fn partition_watermark(&self, partition_id: PartitionId) -> Option<PartitionWatermark> {
        self.partitions
            .get(&partition_id)
            .map(|state| PartitionWatermark::new(partition_id, state.min_required_raft_index))
    }

    /// Global segmented watermarks derived from all partitions.
    pub fn segmented_watermarks(&self) -> SegmentedWatermarks {
        let min_required = self
            .partitions
            .values()
            .map(|state| state.min_required_raft_index)
            .min()
            .unwrap_or(self.segmented_durable_index);
        SegmentedWatermarks::new(self.segmented_durable_index, min_required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmented::types::{
        PartitionMasterPolicy, PartitionQueueConfig, PartitionQuorumRule, QuorumDecision,
    };
    use std::collections::HashSet;

    fn descriptor_with_majority(partition_id: PartitionId) -> PartitionDescriptor {
        let queue = PartitionQueueConfig::default();
        let quorum = PartitionQuorumRule::new(
            HashSet::from([ReplicaId(1), ReplicaId(2), ReplicaId(3)]),
            QuorumDecision::Majority,
        );
        PartitionDescriptor::new(
            partition_id,
            "test-sm".into(),
            queue,
            quorum,
            PartitionMasterPolicy::Disabled,
        )
    }

    #[test]
    fn quorum_satisfaction_updates_watermarks() {
        let mut tracker = SegmentedAckTracker::new();
        tracker.upsert_partition(descriptor_with_majority(7));
        tracker.update_segmented_durable(10);
        tracker.record_append(7, 1, 12).expect("record append");

        let outcome = tracker
            .acknowledge(7, 1, 12, ReplicaId(1))
            .expect("acknowledge");
        assert_eq!(outcome.ack_count, 1);
        assert!(!outcome.quorum_satisfied);

        let outcome = tracker
            .acknowledge(7, 1, 12, ReplicaId(2))
            .expect("acknowledge");
        assert!(outcome.quorum_satisfied);
        assert!(outcome.newly_satisfied);

        let watermark = tracker.partition_watermark(7).expect("watermark");
        assert_eq!(watermark.min_required_raft_index, 10);

        let combined = tracker.segmented_watermarks();
        assert_eq!(combined.segmented_durable_index, 10);
        assert_eq!(combined.min_required_raft_index, 10);
    }

    #[test]
    fn release_entry_advances_partition_floor() {
        let mut tracker = SegmentedAckTracker::new();
        tracker.upsert_partition(descriptor_with_majority(11));
        tracker.update_segmented_durable(20);
        tracker.record_append(11, 5, 25).unwrap();
        tracker.acknowledge(11, 5, 25, ReplicaId(1)).unwrap();
        tracker.acknowledge(11, 5, 25, ReplicaId(2)).unwrap();
        tracker.release_entry(11, 5).unwrap();

        let watermark = tracker.partition_watermark(11).expect("watermark");
        assert_eq!(watermark.min_required_raft_index, 20);
    }
}
