//! Lightweight manifest metadata maintained per partition.

use crate::segmented::types::{GlobalLogIndex, PartitionId, PartitionLogIndex, ReplicaId};

/// Summary of manifest field changes that should be persisted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartitionManifestUpdate {
    pub durable_index: Option<PartitionLogIndex>,
    pub applied_index: Option<PartitionLogIndex>,
    pub applied_global: Option<GlobalLogIndex>,
    pub snapshot_index: Option<Option<PartitionLogIndex>>,
    pub master: Option<Option<ReplicaId>>,
}

impl PartitionManifestUpdate {
    /// Returns true when the update carries no changes.
    pub fn is_empty(&self) -> bool {
        self.durable_index.is_none()
            && self.applied_index.is_none()
            && self.applied_global.is_none()
            && self.snapshot_index.is_none()
            && self.master.is_none()
    }
}

/// Durable metadata describing a partition''s progress through the shared log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionManifest {
    partition_id: PartitionId,
    durable_index: PartitionLogIndex,
    applied_index: PartitionLogIndex,
    applied_global: GlobalLogIndex,
    snapshot_index: Option<PartitionLogIndex>,
    master: Option<ReplicaId>,
}

impl PartitionManifest {
    /// Create an empty manifest for a new partition.
    pub fn new(partition_id: PartitionId) -> Self {
        Self {
            partition_id,
            durable_index: 0,
            applied_index: 0,
            applied_global: 0,
            snapshot_index: None,
            master: None,
        }
    }

    /// Identifier of the partition this manifest describes.
    pub fn partition_id(&self) -> PartitionId {
        self.partition_id
    }

    /// Highest index confirmed durable within the partition log.
    pub fn durable_index(&self) -> PartitionLogIndex {
        self.durable_index
    }

    /// Highest index successfully applied to the state machine.
    pub fn applied_index(&self) -> PartitionLogIndex {
        self.applied_index
    }

    /// Highest global index that has been applied by this partition.
    pub fn applied_global(&self) -> GlobalLogIndex {
        self.applied_global
    }

    /// Latest snapshot index, if any.
    pub fn snapshot_index(&self) -> Option<PartitionLogIndex> {
        self.snapshot_index
    }

    /// Currently assigned partition master, if any.
    pub fn master(&self) -> Option<ReplicaId> {
        self.master
    }

    /// True when no entries have been persisted yet.
    pub fn is_empty(&self) -> bool {
        self.durable_index == 0
            && self.applied_index == 0
            && self.applied_global == 0
            && self.snapshot_index.is_none()
    }

    /// Record that an entry became durable.
    pub fn record_durable(
        &mut self,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) -> PartitionManifestUpdate {
        assert!(
            partition_index >= self.durable_index,
            "durable index must be monotonic"
        );
        assert!(
            global_index >= self.applied_global,
            "global index cannot regress"
        );

        if partition_index == self.durable_index && global_index == self.applied_global {
            return PartitionManifestUpdate::default();
        }

        self.durable_index = partition_index;
        if global_index > self.applied_global {
            self.applied_global = global_index;
        }
        self.check_invariants();

        PartitionManifestUpdate {
            durable_index: Some(self.durable_index),
            applied_global: Some(self.applied_global),
            ..PartitionManifestUpdate::default()
        }
    }

    /// Record that an entry has been applied by the partition state machine.
    pub fn record_applied(
        &mut self,
        partition_index: PartitionLogIndex,
        global_index: GlobalLogIndex,
    ) -> PartitionManifestUpdate {
        assert!(
            partition_index <= self.durable_index,
            "cannot apply beyond durable index"
        );
        assert!(
            partition_index >= self.applied_index,
            "applied index must be monotonic"
        );
        assert!(
            global_index >= self.applied_global,
            "global index cannot regress"
        );

        if partition_index == self.applied_index && global_index == self.applied_global {
            return PartitionManifestUpdate::default();
        }

        self.applied_index = partition_index;
        self.applied_global = global_index;
        self.check_invariants();

        PartitionManifestUpdate {
            applied_index: Some(self.applied_index),
            applied_global: Some(self.applied_global),
            ..PartitionManifestUpdate::default()
        }
    }

    /// Record a new snapshot watermark for the partition.
    pub fn record_snapshot(
        &mut self,
        partition_index: PartitionLogIndex,
    ) -> PartitionManifestUpdate {
        assert!(
            partition_index <= self.applied_index,
            "snapshot cannot exceed applied index"
        );
        if self.snapshot_index == Some(partition_index) {
            return PartitionManifestUpdate::default();
        }

        self.snapshot_index = Some(partition_index);
        self.check_invariants();

        PartitionManifestUpdate {
            snapshot_index: Some(self.snapshot_index),
            ..PartitionManifestUpdate::default()
        }
    }

    /// Remove snapshot metadata when a partition is reset.
    pub fn clear_snapshot(&mut self) -> PartitionManifestUpdate {
        if self.snapshot_index.is_none() {
            return PartitionManifestUpdate::default();
        }
        self.snapshot_index = None;
        self.check_invariants();
        PartitionManifestUpdate {
            snapshot_index: Some(None),
            ..PartitionManifestUpdate::default()
        }
    }

    /// Update the recorded master assignment for this partition.
    pub fn set_master(&mut self, master: Option<ReplicaId>) -> PartitionManifestUpdate {
        if self.master == master {
            return PartitionManifestUpdate::default();
        }
        self.master = master;
        self.check_invariants();
        PartitionManifestUpdate {
            master: Some(self.master),
            ..PartitionManifestUpdate::default()
        }
    }

    /// Apply a persisted update, typically loaded during recovery.
    pub fn apply_update(&mut self, update: &PartitionManifestUpdate) {
        if let Some(durable) = update.durable_index {
            assert!(durable >= self.durable_index);
            self.durable_index = durable;
        }
        if let Some(applied) = update.applied_index {
            assert!(applied >= self.applied_index);
            self.applied_index = applied;
        }
        if let Some(global) = update.applied_global {
            assert!(global >= self.applied_global);
            self.applied_global = global;
        }
        if let Some(snapshot) = update.snapshot_index {
            if let Some(idx) = snapshot {
                assert!(idx <= self.applied_index);
                self.snapshot_index = Some(idx);
            } else {
                self.snapshot_index = None;
            }
        }
        if let Some(master) = update.master {
            self.master = master;
        }
        self.check_invariants();
    }

    fn check_invariants(&self) {
        if let Some(snapshot) = self.snapshot_index {
            assert!(
                snapshot <= self.applied_index,
                "snapshot beyond applied index"
            );
        }
        assert!(
            self.applied_index <= self.durable_index,
            "applied cannot exceed durable"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_durable_monotonic() {
        let mut manifest = PartitionManifest::new(7);
        let update = manifest.record_durable(1, 1);
        assert_eq!(manifest.durable_index(), 1);
        assert_eq!(manifest.applied_global(), 1);
        assert!(!update.is_empty());

        // idempotent re-record
        let update = manifest.record_durable(1, 1);
        assert!(update.is_empty());

        // progressing forward
        manifest.record_durable(5, 6);
        assert_eq!(manifest.durable_index(), 5);
        assert_eq!(manifest.applied_global(), 6);
    }

    #[test]
    fn record_applied_checks_bounds() {
        let mut manifest = PartitionManifest::new(1);
        manifest.record_durable(5, 5);
        let update = manifest.record_applied(3, 5);
        assert_eq!(manifest.applied_index(), 3);
        assert_eq!(manifest.applied_global(), 5);
        assert!(!update.is_empty());

        let update = manifest.record_applied(3, 5);
        assert!(update.is_empty());

        manifest.record_applied(5, 8);
        assert_eq!(manifest.applied_index(), 5);
        assert_eq!(manifest.applied_global(), 8);
    }

    #[test]
    fn snapshot_management() {
        let mut manifest = PartitionManifest::new(3);
        manifest.record_durable(5, 5);
        manifest.record_applied(4, 5);

        let update = manifest.record_snapshot(4);
        assert_eq!(manifest.snapshot_index(), Some(4));
        assert!(!update.is_empty());

        let update = manifest.clear_snapshot();
        assert_eq!(manifest.snapshot_index(), None);
        assert!(!update.is_empty());
    }

    #[test]
    fn apply_update_recovers_state() {
        let mut manifest = PartitionManifest::new(9);
        let update = PartitionManifestUpdate {
            durable_index: Some(10),
            applied_index: Some(9),
            applied_global: Some(17),
            snapshot_index: Some(Some(8)),
            master: None,
        };
        manifest.apply_update(&update);
        assert_eq!(manifest.durable_index(), 10);
        assert_eq!(manifest.applied_index(), 9);
        assert_eq!(manifest.applied_global(), 17);
        assert_eq!(manifest.snapshot_index(), Some(8));
    }

    #[test]
    fn master_assignment_round_trip() {
        let mut manifest = PartitionManifest::new(11);
        let assign = manifest.set_master(Some(ReplicaId(4)));
        assert_eq!(assign.master, Some(Some(ReplicaId(4))));
        manifest.apply_update(&assign);
        assert_eq!(manifest.master(), Some(ReplicaId(4)));

        let clear = manifest.set_master(None);
        assert_eq!(clear.master, Some(None));
        manifest.apply_update(&clear);
        assert_eq!(manifest.master(), None);
    }
}
