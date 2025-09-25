//! High-level coordinator wiring the partition dispatcher to manifest persistence.

use crate::error::RaftError;
use crate::segmented::ack_tracker::SegmentedAckTracker;
use crate::segmented::dispatcher::{AdmissionError, PartitionDispatcher, QuorumEvaluation};
use crate::segmented::manifest::{PartitionManifest, PartitionManifestUpdate};
use crate::segmented::manifest_store::PartitionManifestStore;
use crate::segmented::types::{
    GlobalLogIndex, PartitionDispatcherConfig, PartitionId, PartitionLogIndex,
    PartitionMasterPolicy, PartitionQueueConfig, PartitionQuorumRule, PartitionWatermark,
    ReplicaId, SegmentedWatermarks,
};

/// Result type for coordinator operations.
pub type CoordinatorResult<T> = Result<T, CoordinatorError>;

/// Errors surfaced by the segmented coordinator.
#[derive(Debug)]
pub enum CoordinatorError {
    Admission(AdmissionError),
    Storage(RaftError),
    Ack(crate::segmented::ack_tracker::AckError),
}

impl From<AdmissionError> for CoordinatorError {
    fn from(err: AdmissionError) -> Self {
        Self::Admission(err)
    }
}

impl From<RaftError> for CoordinatorError {
    fn from(err: RaftError) -> Self {
        Self::Storage(err)
    }
}

impl From<crate::segmented::ack_tracker::AckError> for CoordinatorError {
    fn from(err: crate::segmented::ack_tracker::AckError) -> Self {
        Self::Ack(err)
    }
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorError::Admission(err) => write!(f, "admission error: {:?}", err),
            CoordinatorError::Storage(err) => write!(f, "storage error: {err}"),
            CoordinatorError::Ack(err) => write!(f, "ack tracker error: {err}"),
        }
    }
}

impl std::error::Error for CoordinatorError {}

/// Coordinates admission and progress tracking for segmented partitions.
#[derive(Debug)]
pub struct SegmentedCoordinator {
    dispatcher: PartitionDispatcher,
    manifest_store: PartitionManifestStore,
    ack_tracker: SegmentedAckTracker,
}

impl SegmentedCoordinator {
    pub fn new(config: PartitionDispatcherConfig, manifest_store: PartitionManifestStore) -> Self {
        Self {
            dispatcher: PartitionDispatcher::new(config),
            manifest_store,
            ack_tracker: SegmentedAckTracker::new(),
        }
    }

    /// Build a coordinator by replaying manifest updates to restore partition state.
    pub fn with_recovery(
        config: PartitionDispatcherConfig,
        manifest_store: PartitionManifestStore,
    ) -> CoordinatorResult<Self> {
        let manifests = manifest_store
            .load_manifests()
            .map_err(CoordinatorError::from)?;
        let mut dispatcher = PartitionDispatcher::new(config);
        let mut ack_tracker = SegmentedAckTracker::new();
        for manifest in manifests.into_values() {
            let partition_id = manifest.partition_id();
            dispatcher.upsert_partition(manifest, None, None, None);
            if let Some(descriptor) = dispatcher.descriptor(partition_id) {
                ack_tracker.upsert_partition(descriptor);
            }
        }
        Ok(Self {
            dispatcher,
            manifest_store,
            ack_tracker,
        })
    }

    /// Access the underlying dispatcher.
    pub fn dispatcher(&self) -> &PartitionDispatcher {
        &self.dispatcher
    }

    /// Access the manifest store for inspection.
    pub fn manifest_store(&self) -> &PartitionManifestStore {
        &self.manifest_store
    }

    /// Mutable access to the manifest store, primarily for recovery flows.
    pub fn manifest_store_mut(&mut self) -> &mut PartitionManifestStore {
        &mut self.manifest_store
    }

    /// Registers or updates a partition manifest with optional queue configuration override.
    pub fn upsert_partition(
        &mut self,
        manifest: PartitionManifest,
        queue_cfg: Option<PartitionQueueConfig>,
        quorum_rule: Option<PartitionQuorumRule>,
        master_policy: Option<PartitionMasterPolicy>,
    ) -> CoordinatorResult<()> {
        let partition_id = manifest.partition_id();
        if let Some(update) =
            self.dispatcher
                .upsert_partition(manifest, queue_cfg, quorum_rule, master_policy)
        {
            let manifest_ref = self
                .dispatcher
                .manifest(partition_id)
                .expect("partition manifest should exist");
            self.manifest_store
                .append_update(manifest_ref, &update)
                .map_err(CoordinatorError::from)?;
        }
        if let Some(descriptor) = self.dispatcher.descriptor(partition_id) {
            self.ack_tracker.upsert_partition(descriptor);
        }
        Ok(())
    }

    /// Removes a partition and returns its final manifest if present.
    pub fn remove_partition(&mut self, partition_id: PartitionId) -> Option<PartitionManifest> {
        self.ack_tracker.remove_partition(partition_id);
        self.dispatcher.remove_partition(partition_id)
    }

    /// Attempt to admit a new entry for a partition.
    pub fn admit(
        &mut self,
        partition_id: PartitionId,
        current_global_commit: GlobalLogIndex,
    ) -> CoordinatorResult<PartitionLogIndex> {
        self.dispatcher
            .admit(partition_id, current_global_commit)
            .map_err(CoordinatorError::from)
    }

    pub fn record_segmented_append(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) -> CoordinatorResult<()> {
        self.ack_tracker
            .record_append(partition_id, partition_index, global_index)
            .map_err(CoordinatorError::from)
    }

    /// Record that a partition entry has become durable and persist manifest changes.
    pub fn record_durable(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) -> CoordinatorResult<PartitionManifestUpdate> {
        let update = self
            .dispatcher
            .record_durable(partition_id, partition_index, global_index)
            .map_err(CoordinatorError::from)?;
        if !update.is_empty() {
            let manifest = self
                .dispatcher
                .manifest(partition_id)
                .expect("partition manifest should exist");
            self.manifest_store
                .append_update(manifest, &update)
                .map_err(CoordinatorError::from)?;
        }
        self.ack_tracker.update_segmented_durable(global_index);
        Ok(update)
    }

    /// Record that a partition entry has been applied and persist manifest changes.
    pub fn record_applied(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) -> CoordinatorResult<PartitionManifestUpdate> {
        let update = self
            .dispatcher
            .record_applied(partition_id, partition_index, global_index)
            .map_err(CoordinatorError::from)?;
        if !update.is_empty() {
            let manifest = self
                .dispatcher
                .manifest(partition_id)
                .expect("partition manifest should exist");
            self.manifest_store
                .append_update(manifest, &update)
                .map_err(CoordinatorError::from)?;
        }
        if let Err(err) = self
            .ack_tracker
            .release_entry(partition_id, partition_index)
        {
            return Err(CoordinatorError::from(err));
        }
        Ok(update)
    }

    /// Update the dispatcher with the latest cluster membership view.
    pub fn update_membership<I>(&mut self, membership: I) -> CoordinatorResult<()>
    where
        I: IntoIterator<Item = ReplicaId>,
    {
        let updates = self.dispatcher.update_membership(membership);
        for (partition_id, update) in updates {
            if let Some(manifest) = self.dispatcher.manifest(partition_id) {
                self.manifest_store
                    .append_update(manifest, &update)
                    .map_err(CoordinatorError::from)?;
            }
        }
        Ok(())
    }

    /// Lookup the currently assigned partition master, if present.
    pub fn partition_master(&self, partition_id: PartitionId) -> Option<ReplicaId> {
        self.dispatcher.partition_master(partition_id)
    }

    pub fn partition_watermark(&self, partition_id: PartitionId) -> Option<PartitionWatermark> {
        self.ack_tracker.partition_watermark(partition_id)
    }

    /// Snapshot of aggregated segmented watermarks derived from the ack tracker.
    pub fn segmented_watermarks(&self) -> SegmentedWatermarks {
        self.ack_tracker.segmented_watermarks()
    }

    /// Access the segmented ack tracker snapshot.
    pub fn ack_tracker(&self) -> &SegmentedAckTracker {
        &self.ack_tracker
    }

    /// Track a replica acknowledgement for a partition entry, returning both legacy
    /// dispatcher evaluation and the segmented ack tracker outcome.
    pub fn acknowledge(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        raft_index: GlobalLogIndex,
        replica: ReplicaId,
    ) -> CoordinatorResult<(QuorumEvaluation, crate::segmented::ack_tracker::AckOutcome)> {
        let evaluation = self
            .dispatcher
            .acknowledge(partition_id, partition_index, raft_index, replica)
            .map_err(CoordinatorError::from)?;
        let outcome = self
            .ack_tracker
            .acknowledge(partition_id, partition_index, raft_index, replica)
            .map_err(CoordinatorError::from)?;
        Ok((evaluation, outcome))
    }

    /// Minimum applied global index across all partitions.
    pub fn global_applied_floor(&self) -> GlobalLogIndex {
        self.dispatcher.global_applied_floor()
    }

    /// Returns the count of managed partitions.
    pub fn partition_count(&self) -> usize {
        self.dispatcher.partition_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmented::types::{
        PartitionMasterPolicy, PartitionQueueConfig, PartitionQuorumRule, ReplicaId,
    };
    use bop_aof::manifest::ManifestLogWriterConfig;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::num::NonZeroUsize;
    use tempfile::TempDir;

    fn writer_config() -> ManifestLogWriterConfig {
        ManifestLogWriterConfig {
            chunk_capacity_bytes: 8 * 1024,
            rotation_period: std::time::Duration::from_secs(1),
            growth_step_bytes: 8 * 1024,
            enabled: true,
        }
    }

    fn coordinator(tempdir: &TempDir) -> SegmentedCoordinator {
        let store = PartitionManifestStore::new(tempdir.path(), 7, writer_config()).unwrap();
        SegmentedCoordinator::new(PartitionDispatcherConfig::default(), store)
    }

    #[test]
    fn record_durable_persists_manifest() {
        let dir = TempDir::new().expect("tempdir");
        let mut coordinator = coordinator(&dir);
        coordinator
            .upsert_partition(PartitionManifest::new(1), None, None, None)
            .unwrap();

        let idx = coordinator.admit(1, 0).unwrap();
        assert_eq!(idx, 1);
        coordinator.record_durable(1, idx, 5).unwrap();

        let updates = coordinator
            .manifest_store()
            .load_updates()
            .expect("load updates");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, 1);
        assert_eq!(updates[0].1.durable_index, Some(1));
    }

    #[test]
    fn membership_update_assigns_master() {
        let dir = TempDir::new().expect("tempdir");
        let mut coordinator = coordinator(&dir);
        coordinator
            .upsert_partition(
                PartitionManifest::new(1),
                None,
                None,
                Some(PartitionMasterPolicy::Automatic),
            )
            .unwrap();

        coordinator
            .update_membership([ReplicaId(2), ReplicaId(3)])
            .unwrap();
        let master = coordinator.partition_master(1).expect("master assigned");
        let membership = coordinator.dispatcher().membership().to_vec();
        let mut hasher = DefaultHasher::new();
        1u64.hash(&mut hasher);
        let expected = membership[(hasher.finish() as usize) % membership.len()];
        assert_eq!(master, expected);

        let updates = coordinator
            .manifest_store()
            .load_updates()
            .expect("load updates");
        assert!(
            updates
                .iter()
                .any(|(pid, update)| *pid == 1 && update.master == Some(Some(master)))
        );
    }

    #[test]
    fn admission_failures_surface() {
        let dir = TempDir::new().expect("tempdir");
        let mut coordinator = coordinator(&dir);
        let mut cfg = PartitionDispatcherConfig::default();
        cfg.default_queue =
            PartitionQueueConfig::new(std::num::NonZeroUsize::new(1).unwrap(), None);
        coordinator.dispatcher = PartitionDispatcher::new(cfg);
        coordinator
            .upsert_partition(PartitionManifest::new(9), None, None, None)
            .unwrap();

        assert!(coordinator.admit(9, 0).is_ok());
        let err = coordinator.admit(9, 0).unwrap_err();
        assert!(matches!(err, CoordinatorError::Admission(_)));
    }

    #[test]
    fn acknowledge_updates_tracker() {
        let dir = TempDir::new().expect("tempdir");
        let mut coordinator = coordinator(&dir);
        coordinator
            .upsert_partition(PartitionManifest::new(3), None, None, None)
            .unwrap();
        let idx = coordinator.admit(3, 0).unwrap();
        coordinator.record_segmented_append(3, idx, 11).unwrap();
        coordinator.record_durable(3, idx, 11).unwrap();

        let (_evaluation, outcome) = coordinator
            .acknowledge(3, idx, 11, ReplicaId(1))
            .expect("acknowledge");
        assert!(outcome.quorum_satisfied);
        let watermark = coordinator
            .ack_tracker()
            .partition_watermark(3)
            .expect("watermark");
        assert_eq!(watermark.partition_id, 3);
    }

    #[test]
    fn watermarks_reflect_tracker_state() {
        let dir = TempDir::new().expect("tempdir");
        let mut coordinator = coordinator(&dir);
        let quorum = PartitionQuorumRule::threshold(
            [ReplicaId(1), ReplicaId(2)],
            NonZeroUsize::new(2).unwrap(),
        );
        coordinator
            .upsert_partition(PartitionManifest::new(42), None, Some(quorum), None)
            .unwrap();
        coordinator
            .update_membership([ReplicaId(1), ReplicaId(2)])
            .unwrap();

        let partition_index = coordinator.admit(42, 0).expect("admit");
        coordinator
            .record_segmented_append(42, partition_index, 1)
            .expect("record append");

        let watermark = coordinator.partition_watermark(42).expect("watermark");
        assert_eq!(watermark.partition_id, 42);
        assert_eq!(watermark.min_required_raft_index, 1);

        let segmented = coordinator.segmented_watermarks();
        assert_eq!(segmented.segmented_durable_index, 0);
        assert_eq!(segmented.min_required_raft_index, 1);
        assert!(!segmented.can_truncate());

        coordinator
            .record_durable(42, partition_index, 1)
            .expect("record durable");
        let segmented = coordinator.segmented_watermarks();
        assert_eq!(segmented.segmented_durable_index, 1);
        assert_eq!(segmented.min_required_raft_index, 1);

        coordinator
            .acknowledge(42, partition_index, 1, ReplicaId(1))
            .expect("ack 1");
        coordinator
            .acknowledge(42, partition_index, 1, ReplicaId(2))
            .expect("ack 2");
        let segmented = coordinator.segmented_watermarks();
        assert_eq!(segmented.segmented_durable_index, 1);
        assert_eq!(segmented.min_required_raft_index, 1);

        coordinator
            .record_applied(42, partition_index, 1)
            .expect("record applied");
        let watermark = coordinator.partition_watermark(42).expect("watermark");
        assert_eq!(watermark.min_required_raft_index, 1);
    }

    #[test]
    fn recovery_replays_manifests() {
        let dir = TempDir::new().expect("tempdir");
        let store = PartitionManifestStore::new(dir.path(), 99, writer_config()).unwrap();
        let mut coordinator =
            SegmentedCoordinator::new(PartitionDispatcherConfig::default(), store);
        coordinator
            .upsert_partition(PartitionManifest::new(5), None, None, None)
            .unwrap();
        let idx = coordinator.admit(5, 0).unwrap();
        coordinator.record_durable(5, idx, 7).unwrap();
        drop(coordinator);

        let store = PartitionManifestStore::new(dir.path(), 99, writer_config()).unwrap();
        let coordinator =
            SegmentedCoordinator::with_recovery(PartitionDispatcherConfig::default(), store)
                .unwrap();
        let manifest = coordinator.dispatcher().manifest(5).expect("manifest");
        assert_eq!(manifest.durable_index(), 1);
        assert_eq!(manifest.applied_index(), 0);
        assert_eq!(manifest.applied_global(), 7);
    }
}
