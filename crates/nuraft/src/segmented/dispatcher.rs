//! Dispatcher coordinating admission and progress tracking for partitioned logs.

use std::cmp::Ordering;
use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;

use crate::segmented::manifest::{PartitionManifest, PartitionManifestUpdate};
use crate::segmented::types::{
    GlobalLogIndex, PartitionDescriptor, PartitionDispatcherConfig, PartitionId, PartitionLogIndex,
    PartitionMasterPolicy, PartitionQueueConfig, PartitionQuorumRule, PartitionStateMachineId,
    ReplicaId,
};

/// Errors surfaced when attempting to admit new work for a partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionError {
    PartitionMissing(PartitionId),
    Backpressure {
        partition_id: PartitionId,
        inflight: usize,
        limit: NonZeroUsize,
    },
    LagExceeded {
        partition_id: PartitionId,
        lag: GlobalLogIndex,
        max_allowed: GlobalLogIndex,
    },
}

/// Result describing the outcome of a quorum acknowledgement update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuorumEvaluation {
    pub partition_id: PartitionId,
    pub partition_index: PartitionLogIndex,
    pub global_index: GlobalLogIndex,
    pub ack_count: usize,
    pub member_count: usize,
    pub satisfied: bool,
    pub newly_satisfied: bool,
}

/// Snapshot of the current quorum state for a partition log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuorumStatus {
    pub partition_id: PartitionId,
    pub partition_index: PartitionLogIndex,
    pub global_index: GlobalLogIndex,
    pub ack_count: usize,
    pub member_count: usize,
    pub satisfied: bool,
}

#[derive(Debug, Clone)]
struct PartitionContext {
    manifest: PartitionManifest,
    inflight_entries: usize,
    next_partition_index: PartitionLogIndex,
    queue_cfg: PartitionQueueConfig,
    quorum_rule: PartitionQuorumRule,
    master_policy: PartitionMasterPolicy,
    master: Option<ReplicaId>,
    ack_states: HashMap<PartitionLogIndex, QuorumAckState>,
    state_machine: PartitionStateMachineId,
}

#[derive(Debug, Clone)]
struct QuorumAckState {
    global_index: GlobalLogIndex,
    acknowledgements: HashSet<ReplicaId>,
    satisfied: bool,
}

impl QuorumAckState {
    fn new(global_index: GlobalLogIndex) -> Self {
        Self {
            global_index,
            acknowledgements: HashSet::new(),
            satisfied: false,
        }
    }

    fn reset(&mut self, global_index: GlobalLogIndex) {
        self.global_index = global_index;
        self.acknowledgements.clear();
        self.satisfied = false;
    }
}

impl PartitionContext {
    fn new(
        manifest: PartitionManifest,
        queue_cfg: PartitionQueueConfig,
        quorum_rule: PartitionQuorumRule,
        master_policy: PartitionMasterPolicy,
    ) -> Self {
        let partition_id = manifest.partition_id();
        let next_index = manifest.durable_index().max(manifest.applied_index()) + 1;
        let master = manifest.master();
        let ctx = Self {
            manifest,
            inflight_entries: 0,
            next_partition_index: next_index,
            queue_cfg,
            quorum_rule,
            master_policy,
            master,
            ack_states: HashMap::new(),
            state_machine: PartitionStateMachineId::from(format!("legacy:{}", partition_id)),
        };
        ctx
    }

    fn to_descriptor(&self) -> PartitionDescriptor {
        PartitionDescriptor::new(
            self.manifest.partition_id(),
            self.state_machine.clone(),
            self.queue_cfg,
            self.quorum_rule.clone(),
            self.master_policy,
        )
    }

    fn can_admit(&self, current_global_commit: GlobalLogIndex) -> Result<(), AdmissionError> {
        if self.inflight_entries >= self.queue_cfg.max_inflight_entries.get() {
            return Err(AdmissionError::Backpressure {
                partition_id: self.manifest.partition_id(),
                inflight: self.inflight_entries,
                limit: self.queue_cfg.max_inflight_entries,
            });
        }

        if let Some(max_lag) = self.queue_cfg.max_global_lag {
            let applied = self.manifest.applied_global();
            let lag = current_global_commit.saturating_sub(applied);
            if lag > max_lag {
                return Err(AdmissionError::LagExceeded {
                    partition_id: self.manifest.partition_id(),
                    lag,
                    max_allowed: max_lag,
                });
            }
        }

        Ok(())
    }

    fn admit(&mut self) -> PartitionLogIndex {
        let index = self.next_partition_index;
        self.next_partition_index = self.next_partition_index.saturating_add(1);
        self.inflight_entries += 1;
        index
    }

    fn record_durable(
        &mut self,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) -> PartitionManifestUpdate {
        let update = self.manifest.record_durable(partition_index, global_index);
        if self.inflight_entries > 0 {
            self.inflight_entries -= 1;
        }
        if partition_index + 1 > self.next_partition_index {
            self.next_partition_index = partition_index + 1;
        }
        update
    }

    fn record_applied(
        &mut self,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) -> PartitionManifestUpdate {
        let update = self.manifest.record_applied(partition_index, global_index);
        self.ack_states.retain(|idx, _| *idx > partition_index);
        update
    }

    fn quorum_rule(&self) -> &PartitionQuorumRule {
        &self.quorum_rule
    }

    fn master_policy(&self) -> PartitionMasterPolicy {
        self.master_policy
    }

    fn master(&self) -> Option<ReplicaId> {
        self.master
    }

    fn set_master(&mut self, master: Option<ReplicaId>) -> Option<PartitionManifestUpdate> {
        if self.master == master {
            return None;
        }
        self.master = master;
        let update = self.manifest.set_master(master);
        if update.is_empty() {
            None
        } else {
            Some(update)
        }
    }

    fn quorum_state(&self, partition_index: PartitionLogIndex) -> Option<&QuorumAckState> {
        self.ack_states.get(&partition_index)
    }

    fn apply_master_policy(&mut self, membership: &[ReplicaId]) -> Option<PartitionManifestUpdate> {
        let desired = match self.master_policy {
            PartitionMasterPolicy::Disabled => None,
            PartitionMasterPolicy::Fixed(replica) => membership
                .iter()
                .copied()
                .find(|candidate| *candidate == replica),
            PartitionMasterPolicy::Automatic => {
                if membership.is_empty() {
                    None
                } else {
                    let index = self.select_master_index(membership.len());
                    Some(membership[index])
                }
            }
        };
        self.set_master(desired)
    }

    fn select_master_index(&self, membership_len: usize) -> usize {
        debug_assert!(membership_len > 0);
        let mut hasher = DefaultHasher::new();
        self.manifest.partition_id().hash(&mut hasher);
        (hasher.finish() as usize) % membership_len
    }

    fn acknowledge(
        &mut self,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
        replica: ReplicaId,
    ) -> QuorumEvaluation {
        let partition_id = self.manifest.partition_id();
        let entry = self.ack_states.entry(partition_index);

        let state = match entry {
            Entry::Occupied(mut occupied) => {
                let stale_evaluation = {
                    let state_ref = occupied.get_mut();
                    match global_index.cmp(&state_ref.global_index) {
                        Ordering::Less => Some(QuorumEvaluation {
                            partition_id,
                            partition_index,
                            global_index: state_ref.global_index,
                            ack_count: state_ref.acknowledgements.len(),
                            member_count: self.quorum_rule.member_count(),
                            satisfied: state_ref.satisfied,
                            newly_satisfied: false,
                        }),
                        Ordering::Greater => {
                            state_ref.reset(global_index);
                            None
                        }
                        Ordering::Equal => None,
                    }
                };
                if let Some(result) = stale_evaluation {
                    return result;
                }
                occupied.into_mut()
            }
            Entry::Vacant(vacant) => vacant.insert(QuorumAckState::new(global_index)),
        };

        let mut ack_added = false;
        if self.quorum_rule.contains(&replica) {
            ack_added = state.acknowledgements.insert(replica);
        }

        let was_satisfied = state.satisfied;
        let satisfied = if !ack_added && was_satisfied {
            true
        } else {
            self.quorum_rule.evaluate(&state.acknowledgements)
        };
        let newly_satisfied = satisfied && !was_satisfied;
        state.satisfied = satisfied;

        QuorumEvaluation {
            partition_id,
            partition_index,
            global_index: state.global_index,
            ack_count: state.acknowledgements.len(),
            member_count: self.quorum_rule.member_count(),
            satisfied,
            newly_satisfied,
        }
    }
}

/// Tracks all partitions and arbitrates admission/backpressure decisions.
#[derive(Debug, Default)]
pub struct PartitionDispatcher {
    config: PartitionDispatcherConfig,
    partitions: HashMap<PartitionId, PartitionContext>,
    cluster_membership: Vec<ReplicaId>,
}

impl PartitionDispatcher {
    /// Create a dispatcher with the supplied defaults.
    pub fn new(config: PartitionDispatcherConfig) -> Self {
        Self {
            config,
            partitions: HashMap::new(),
            cluster_membership: Vec::new(),
        }
    }

    /// Returns true if a partition is currently registered.
    pub fn has_partition(&self, partition_id: PartitionId) -> bool {
        self.partitions.contains_key(&partition_id)
    }

    /// Register or replace a partition context.
    pub fn upsert_partition(
        &mut self,
        manifest: PartitionManifest,
        queue_cfg: Option<PartitionQueueConfig>,
        quorum_rule: Option<PartitionQuorumRule>,
        master_policy: Option<PartitionMasterPolicy>,
    ) -> Option<PartitionManifestUpdate> {
        let partition_id = manifest.partition_id();
        let cfg = queue_cfg
            .or_else(|| self.config.override_for(partition_id))
            .unwrap_or(self.config.default_queue);
        let quorum = quorum_rule.unwrap_or_else(|| self.config.quorum_rule_for(partition_id));
        let policy = master_policy.unwrap_or_else(|| self.config.master_policy_for(partition_id));
        let mut context = PartitionContext::new(manifest, cfg, quorum, policy);
        let membership_snapshot = self.cluster_membership.clone();
        let update = context.apply_master_policy(&membership_snapshot);
        self.partitions.insert(partition_id, context);
        update
    }

    /// Remove a partition context, returning the manifest that was tracked if present.
    pub fn remove_partition(&mut self, partition_id: PartitionId) -> Option<PartitionManifest> {
        self.partitions
            .remove(&partition_id)
            .map(|ctx| ctx.manifest)
    }

    /// Peek at the current manifest for a partition.
    pub fn manifest(&self, partition_id: PartitionId) -> Option<&PartitionManifest> {
        self.partitions.get(&partition_id).map(|ctx| &ctx.manifest)
    }

    /// Obtain a mutable manifest reference for in-place updates outside dispatcher helpers.
    pub fn manifest_mut(&mut self, partition_id: PartitionId) -> Option<&mut PartitionManifest> {
        self.partitions
            .get_mut(&partition_id)
            .map(|ctx| &mut ctx.manifest)
    }

    /// Attempt to admit a new entry for the given partition.
    pub fn admit(
        &mut self,
        partition_id: PartitionId,
        current_global_commit: GlobalLogIndex,
    ) -> Result<PartitionLogIndex, AdmissionError> {
        let ctx = self
            .partitions
            .get_mut(&partition_id)
            .ok_or(AdmissionError::PartitionMissing(partition_id))?;
        ctx.can_admit(current_global_commit)?;
        Ok(ctx.admit())
    }

    /// Record durability for a partition entry.
    pub fn record_durable(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) -> Result<PartitionManifestUpdate, AdmissionError> {
        let ctx = self
            .partitions
            .get_mut(&partition_id)
            .ok_or(AdmissionError::PartitionMissing(partition_id))?;
        Ok(ctx.record_durable(partition_index, global_index))
    }

    /// Record application progress for a partition entry.
    pub fn record_applied(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) -> Result<PartitionManifestUpdate, AdmissionError> {
        let ctx = self
            .partitions
            .get_mut(&partition_id)
            .ok_or(AdmissionError::PartitionMissing(partition_id))?;
        Ok(ctx.record_applied(partition_index, global_index))
    }

    /// Update the cluster membership view, re-evaluating master assignments where applicable.
    pub fn update_membership<I>(
        &mut self,
        membership: I,
    ) -> Vec<(PartitionId, PartitionManifestUpdate)>
    where
        I: IntoIterator<Item = ReplicaId>,
    {
        let mut members: Vec<ReplicaId> = membership.into_iter().collect();
        members.sort_by_key(|replica| replica.0);
        members.dedup();
        self.cluster_membership = members;
        let membership_snapshot = self.cluster_membership.clone();
        let mut updates = Vec::new();
        for ctx in self.partitions.values_mut() {
            if let Some(update) = ctx.apply_master_policy(&membership_snapshot) {
                updates.push((ctx.manifest.partition_id(), update));
            }
        }
        updates
    }

    /// Returns the current master for a partition, if any.
    pub fn partition_master(&self, partition_id: PartitionId) -> Option<ReplicaId> {
        self.partitions
            .get(&partition_id)
            .and_then(|ctx| ctx.master())
    }

    /// Access the master policy for a partition.
    pub fn master_policy(&self, partition_id: PartitionId) -> Option<PartitionMasterPolicy> {
        self.partitions
            .get(&partition_id)
            .map(|ctx| ctx.master_policy())
    }

    /// Snapshot of the current cluster membership supplied to the dispatcher.
    pub fn membership(&self) -> &[ReplicaId] {
        &self.cluster_membership
    }

    /// Build a descriptor suitable for ack tracker registration.
    pub fn descriptor(&self, partition_id: PartitionId) -> Option<PartitionDescriptor> {
        self.partitions
            .get(&partition_id)
            .map(|ctx| ctx.to_descriptor())
    }

    /// Track an acknowledgement from a replica for a specific partition index.
    pub fn acknowledge(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
        replica: ReplicaId,
    ) -> Result<QuorumEvaluation, AdmissionError> {
        let ctx = self
            .partitions
            .get_mut(&partition_id)
            .ok_or(AdmissionError::PartitionMissing(partition_id))?;
        Ok(ctx.acknowledge(partition_index, global_index, replica))
    }

    /// Inspect the current quorum status for a partition entry if it exists.
    pub fn quorum_status(
        &self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
    ) -> Option<QuorumStatus> {
        let ctx = self.partitions.get(&partition_id)?;
        let state = ctx.quorum_state(partition_index)?;
        Some(QuorumStatus {
            partition_id,
            partition_index,
            global_index: state.global_index,
            ack_count: state.acknowledgements.len(),
            member_count: ctx.quorum_rule().member_count(),
            satisfied: state.satisfied,
        })
    }

    /// Compute the minimum applied global index across all partitions.
    pub fn global_applied_floor(&self) -> GlobalLogIndex {
        self.partitions
            .values()
            .map(|ctx| ctx.manifest.applied_global())
            .min()
            .unwrap_or(0)
    }

    /// Iterate over all manifests, typically for persistence.
    pub fn manifests(&self) -> impl Iterator<Item = &PartitionManifest> {
        self.partitions.values().map(|ctx| &ctx.manifest)
    }

    /// Number of known partitions.
    pub fn partition_count(&self) -> usize {
        self.partitions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmented::types::{PartitionMasterPolicy, PartitionQuorumRule, ReplicaId};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn dispatcher_with_partition(partition_id: PartitionId) -> PartitionDispatcher {
        let mut dispatcher = PartitionDispatcher::new(PartitionDispatcherConfig::default());
        dispatcher.upsert_partition(PartitionManifest::new(partition_id), None, None, None);
        dispatcher
    }

    fn dispatcher_with_quorum(
        partition_id: PartitionId,
        quorum_rule: PartitionQuorumRule,
    ) -> PartitionDispatcher {
        let mut cfg = PartitionDispatcherConfig::default();
        cfg.default_quorum = quorum_rule;
        let mut dispatcher = PartitionDispatcher::new(cfg);
        dispatcher.upsert_partition(PartitionManifest::new(partition_id), None, None, None);
        dispatcher
    }

    #[test]
    fn admit_increments_inflight_and_index() {
        let mut dispatcher = dispatcher_with_partition(1);
        let idx = dispatcher.admit(1, 0).expect("admit");
        assert_eq!(idx, 1);
        let idx2 = dispatcher.admit(1, 0).expect("admit");
        assert_eq!(idx2, 2);
    }

    #[test]
    fn backpressure_triggers_when_limit_hit() {
        let mut cfg = PartitionDispatcherConfig::default();
        cfg.default_queue = PartitionQueueConfig::new(NonZeroUsize::new(1).unwrap(), None);
        let mut dispatcher = PartitionDispatcher::new(cfg);
        dispatcher.upsert_partition(PartitionManifest::new(7), None, None, None);

        assert!(dispatcher.admit(7, 0).is_ok());
        let err = dispatcher.admit(7, 0).unwrap_err();
        assert!(matches!(err, AdmissionError::Backpressure { .. }));
    }

    #[test]
    fn per_partition_override_applies() {
        let default_cfg = PartitionQueueConfig::new(NonZeroUsize::new(4).unwrap(), None);
        let override_cfg = PartitionQueueConfig::new(NonZeroUsize::new(1).unwrap(), None);
        let cfg = PartitionDispatcherConfig::new(default_cfg).with_override(11, override_cfg);
        let mut dispatcher = PartitionDispatcher::new(cfg);
        dispatcher.upsert_partition(PartitionManifest::new(11), None, None, None);

        assert!(dispatcher.admit(11, 0).is_ok());
        let err = dispatcher.admit(11, 0).unwrap_err();
        assert!(matches!(err, AdmissionError::Backpressure { .. }));
    }

    #[test]
    fn lag_guard_blocks_when_exceeded() {
        let mut cfg = PartitionDispatcherConfig::default();
        cfg.default_queue = PartitionQueueConfig::new(NonZeroUsize::new(4).unwrap(), Some(3));
        let mut dispatcher = PartitionDispatcher::new(cfg);
        dispatcher.upsert_partition(PartitionManifest::new(9), None, None, None);

        // Applied global stays at zero so lag equals commit index.
        let err = dispatcher.admit(9, 10).unwrap_err();
        assert!(matches!(err, AdmissionError::LagExceeded { .. }));
    }

    #[test]
    fn record_progress_reduces_backlog() {
        let mut cfg = PartitionDispatcherConfig::default();
        cfg.default_queue = PartitionQueueConfig::new(NonZeroUsize::new(2).unwrap(), None);
        let mut dispatcher = PartitionDispatcher::new(cfg);
        dispatcher.upsert_partition(PartitionManifest::new(5), None, None, None);

        let idx = dispatcher.admit(5, 0).unwrap();
        dispatcher
            .record_durable(5, idx, 1)
            .expect("record durable");
        dispatcher
            .record_applied(5, idx, 1)
            .expect("record applied");

        // Should admit again without error.
        dispatcher.admit(5, 1).expect("admit after progress");
    }

    #[test]
    fn auto_master_assignment_tracks_membership() {
        let mut dispatcher = PartitionDispatcher::new(PartitionDispatcherConfig::default());
        dispatcher.upsert_partition(
            PartitionManifest::new(42),
            None,
            None,
            Some(PartitionMasterPolicy::Automatic),
        );
        assert!(dispatcher.partition_master(42).is_none());

        let updates = dispatcher.update_membership([ReplicaId(7), ReplicaId(9), ReplicaId(3)]);
        assert_eq!(updates.len(), 1);
        assert!(matches!(updates[0].1.master, Some(Some(_))));
        let membership = dispatcher.membership().to_vec();
        let master = dispatcher.partition_master(42).expect("master assigned");
        assert!(membership.contains(&master));

        let mut hasher = DefaultHasher::new();
        42u64.hash(&mut hasher);
        let index = (hasher.finish() as usize) % membership.len();
        assert_eq!(master, membership[index]);

        let updates = dispatcher.update_membership([ReplicaId(9), ReplicaId(11)]);
        assert_eq!(updates.len(), 1);
        let membership_after = dispatcher.membership().to_vec();
        let mut hasher = DefaultHasher::new();
        42u64.hash(&mut hasher);
        let next_index = (hasher.finish() as usize) % membership_after.len();
        assert_eq!(
            dispatcher.partition_master(42).expect("master assigned"),
            membership_after[next_index]
        );
    }

    #[test]
    fn fixed_master_clears_when_member_missing() {
        let mut dispatcher = PartitionDispatcher::new(PartitionDispatcherConfig::default());
        dispatcher.upsert_partition(
            PartitionManifest::new(8),
            None,
            None,
            Some(PartitionMasterPolicy::Fixed(ReplicaId(5))),
        );

        let updates = dispatcher.update_membership([ReplicaId(5), ReplicaId(7)]);
        assert_eq!(updates.len(), 1);
        assert_eq!(dispatcher.partition_master(8), Some(ReplicaId(5)));

        let updates = dispatcher.update_membership([ReplicaId(7)]);
        assert_eq!(updates.len(), 1);
        assert!(dispatcher.partition_master(8).is_none());
    }

    #[test]
    fn master_disabled_remains_none() {
        let mut dispatcher = PartitionDispatcher::new(PartitionDispatcherConfig::default());
        dispatcher.upsert_partition(PartitionManifest::new(12), None, None, None);
        let updates = dispatcher.update_membership([ReplicaId(1), ReplicaId(2)]);
        assert!(updates.is_empty());
        assert!(dispatcher.partition_master(12).is_none());
    }

    #[test]
    fn quorum_majority_evaluates_and_detects_first_satisfaction() {
        let members = [ReplicaId(1), ReplicaId(2), ReplicaId(3)];
        let quorum_rule = PartitionQuorumRule::majority(members);
        let mut dispatcher = dispatcher_with_quorum(21, quorum_rule);

        let first = dispatcher
            .acknowledge(21, 1, 10, ReplicaId(1))
            .expect("ack");
        assert_eq!(first.ack_count, 1);
        assert!(!first.satisfied);
        assert!(!first.newly_satisfied);

        let second = dispatcher
            .acknowledge(21, 1, 10, ReplicaId(2))
            .expect("ack");
        assert_eq!(second.ack_count, 2);
        assert_eq!(second.member_count, 3);
        assert!(second.satisfied);
        assert!(second.newly_satisfied);
    }

    #[test]
    fn quorum_duplicate_ack_does_not_change_state() {
        let quorum_rule = PartitionQuorumRule::majority([ReplicaId(1), ReplicaId(2)]);
        let mut dispatcher = dispatcher_with_quorum(31, quorum_rule);

        let first = dispatcher
            .acknowledge(31, 4, 20, ReplicaId(1))
            .expect("ack");
        assert!(!first.satisfied);

        let second = dispatcher
            .acknowledge(31, 4, 20, ReplicaId(2))
            .expect("ack");
        assert!(second.newly_satisfied);

        let duplicate = dispatcher
            .acknowledge(31, 4, 20, ReplicaId(2))
            .expect("ack");
        assert!(duplicate.satisfied);
        assert!(!duplicate.newly_satisfied);
        assert_eq!(duplicate.ack_count, 2);
    }

    #[test]
    fn quorum_ignores_non_member_acknowledgements() {
        let quorum_rule = PartitionQuorumRule::majority([ReplicaId(1), ReplicaId(2), ReplicaId(3)]);
        let mut dispatcher = dispatcher_with_quorum(41, quorum_rule);

        let non_member = dispatcher.acknowledge(41, 2, 7, ReplicaId(9)).expect("ack");
        assert_eq!(non_member.ack_count, 0);
        assert!(!non_member.satisfied);

        let member = dispatcher.acknowledge(41, 2, 7, ReplicaId(1)).expect("ack");
        assert_eq!(member.ack_count, 1);
        assert!(!member.satisfied);
    }

    #[test]
    fn quorum_threshold_rule_respects_requirement() {
        let quorum_rule = PartitionQuorumRule::threshold(
            [ReplicaId(1), ReplicaId(2), ReplicaId(3)],
            NonZeroUsize::new(2).unwrap(),
        );
        let mut dispatcher = dispatcher_with_quorum(51, quorum_rule);

        let first = dispatcher
            .acknowledge(51, 9, 33, ReplicaId(1))
            .expect("ack");
        assert!(!first.satisfied);

        let second = dispatcher
            .acknowledge(51, 9, 33, ReplicaId(2))
            .expect("ack");
        assert!(second.satisfied);
        assert!(second.newly_satisfied);
    }

    #[test]
    fn quorum_resets_when_global_index_advances() {
        let quorum_rule = PartitionQuorumRule::majority([ReplicaId(1), ReplicaId(2), ReplicaId(3)]);
        let mut dispatcher = dispatcher_with_quorum(61, quorum_rule);

        dispatcher
            .acknowledge(61, 5, 18, ReplicaId(1))
            .expect("ack");
        dispatcher
            .acknowledge(61, 5, 18, ReplicaId(2))
            .expect("ack");

        let reset = dispatcher
            .acknowledge(61, 5, 19, ReplicaId(1))
            .expect("ack");
        assert_eq!(reset.ack_count, 1);
        assert!(!reset.satisfied);
        assert!(!reset.newly_satisfied);
    }

    #[test]
    fn quorum_state_cleared_after_apply() {
        let quorum_rule = PartitionQuorumRule::majority([ReplicaId(1), ReplicaId(2), ReplicaId(3)]);
        let mut dispatcher = dispatcher_with_quorum(71, quorum_rule);

        dispatcher
            .acknowledge(71, 3, 11, ReplicaId(1))
            .expect("ack");
        assert!(dispatcher.quorum_status(71, 3).is_some());

        dispatcher
            .record_durable(71, 3, 11)
            .expect("record durable");
        dispatcher
            .record_applied(71, 3, 11)
            .expect("record applied");
        assert!(dispatcher.quorum_status(71, 3).is_none());
    }

    #[test]
    fn remove_partition_returns_manifest() {
        let mut dispatcher = dispatcher_with_partition(3);
        let manifest = dispatcher.remove_partition(3).expect("manifest");
        assert_eq!(manifest.partition_id(), 3);
        assert_eq!(dispatcher.partition_count(), 0);
    }
}
