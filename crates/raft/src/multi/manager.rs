use maniac::io::{AsyncReadRent, AsyncWriteRent};
use crate::multi::config::MultiRaftConfig;
use crate::multi::network::GroupNetworkFactory;
use crate::multi::network::MultiRaftNetworkFactory;
use crate::multi::network::MultiplexedTransport;
use crate::multi::storage::MultiRaftLogStorage;
use dashmap::DashMap;
use openraft::Config;
use openraft::Raft;
use openraft::RaftTypeConfig;
use openraft::storage::RaftStateMachine;
use std::marker::Unpin;
use std::sync::Arc;

/// Manager for multiple Raft groups.
///
/// The manager handles:
/// - Group lifecycle (add/remove groups)
/// - Shared transport layer for all groups
/// - Log storage (multiplexed across groups)
///
/// State machines are provided externally when adding groups since
/// different groups may have completely different state machine types.
pub struct MultiRaftManager<
    C: RaftTypeConfig,
    T: MultiplexedTransport<C>,
    S: MultiRaftLogStorage<C>,
> {
    groups: DashMap<u64, Raft<C>>,
    network_factory: Arc<MultiRaftNetworkFactory<C, T>>,
    storage: Arc<S>,
    config: MultiRaftConfig,
}

impl<C, T, S> MultiRaftManager<C, T, S>
where
    C: RaftTypeConfig,
    T: MultiplexedTransport<C>,
    S: MultiRaftLogStorage<C>,
{
    pub fn new(transport: T, storage: S, config: MultiRaftConfig) -> Self
    where
        C::SnapshotData: AsyncReadRent + AsyncWriteRent + Unpin,
        C::Entry: Clone,
    {
        Self {
            groups: DashMap::new(),
            network_factory: Arc::new(MultiRaftNetworkFactory::new(transport, config.clone())),
            storage: Arc::new(storage),
            config,
        }
    }

    /// Add a new Raft group with the provided state machine.
    ///
    /// Each group can have its own state machine type/implementation.
    /// The log storage is obtained from the shared multiplexed storage.
    pub async fn add_group<SM>(
        &self,
        group_id: u64,
        node_id: C::NodeId,
        raft_config: Arc<Config>,
        state_machine: SM,
    ) -> Result<Raft<C>, openraft::error::Fatal<C>>
    where
        C::SnapshotData: AsyncReadRent + AsyncWriteRent + Unpin,
        C::Entry: Clone,
        SM: RaftStateMachine<C>,
    {
        let group_factory = GroupNetworkFactory::new(self.network_factory.clone(), group_id);
        let log_store = self.storage.get_log_storage(group_id);

        let raft = Raft::new(
            node_id,
            raft_config,
            group_factory,
            log_store,
            state_machine,
        )
        .await?;

        self.groups.insert(group_id, raft.clone());
        Ok(raft)
    }

    /// Get a handle to an existing group
    pub fn get_group(&self, group_id: u64) -> Option<Raft<C>> {
        self.groups.get(&group_id).map(|r| r.clone())
    }

    /// Remove a group, shutting down its Raft instance
    pub async fn remove_group(&self, group_id: u64) {
        if let Some((_, raft)) = self.groups.remove(&group_id) {
            let _ = raft.shutdown().await;
        }
        // Also remove from storage
        self.storage.remove_group(group_id);
    }

    /// Get all active group IDs
    pub fn group_ids(&self) -> Vec<u64> {
        self.groups.iter().map(|e| *e.key()).collect()
    }

    /// Get the number of active groups
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }
}
