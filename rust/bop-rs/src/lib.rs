pub mod allocator;
pub mod usockets;
pub mod raft_integration;

pub use raft_integration::{BopNodeId, BopNode, BopRequest, BopResponse, BopRaftLogStorage, BopRaftStateMachine, BopRaftNetwork, BopTypeConfig};

/// Raft-enabled Application wrapper (commented out until openraft is enabled)
/*
pub struct RaftApp {
    state: Arc<State>,
    raft: Raft<BopTypeConfig, BopRaftLogStorage, BopRaftStateMachine, BopRaftNetwork>,
    node_id: BopNodeId,
}

impl RaftApp {
    /// Create a new RaftApp instance with Raft consensus
    pub async fn new(node_id: BopNodeId, address: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let state = Arc::new(State::new_with_raft_node(node_id as i32, address.clone()));
        
        // Create Raft log storage and state machine
        let log_storage = BopRaftLogStorage::new(state.clone());
        let state_machine = BopRaftStateMachine::new(state.clone());
        
        // Create Raft network
        let network = BopRaftNetwork::new(node_id);
        
        // Create Raft configuration
        let config = RaftConfig::default();
        
        // Create the Raft instance
        let raft = Raft::new(node_id, config, network, log_storage, state_machine).await?;
        
        Ok(Self {
            state,
            raft,
            node_id,
        })
    }

    /// Get the node ID
    pub fn node_id(&self) -> BopNodeId {
        self.node_id
    }

    /// Get a reference to the internal state
    pub fn state(&self) -> &Arc<State> {
        &self.state
    }

    /// Get a reference to the Raft instance
    pub fn raft(&self) -> &Raft<BopTypeConfig, BopRaftLogStorage, BopRaftStateMachine, BopRaftNetwork> {
        &self.raft
    }

    /// Process a message with Raft consensus (for write operations)
    pub async fn process_message_with_consensus(
        &self,
        message: Message,
        connection_id: Option<u64>,
        node_id: Option<String>,
    ) -> Result<BopResponse, Box<dyn std::error::Error + Send + Sync>> {
        let request = BopRequest {
            message,
            connection_id,
            node_id,
        };

        // Submit to Raft for consensus
        let response = self.raft.client_write(request).await?;
        Ok(response.data)
    }

    /// Process a read-only message (bypasses Raft)
    pub fn process_read_message(
        &self,
        message: FramedMessage<Message>,
        connection_id: Option<u64>,
        node_id: Option<String>,
    ) -> FramedMessage<Message> {
        // For read operations, use the local state directly
        process_message(&self.state, message, connection_id, node_id)
    }

    /// Initialize a single-node cluster
    pub async fn initialize_single_node_cluster(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut nodes = std::collections::BTreeMap::new();
        nodes.insert(self.node_id, BasicNode::new("127.0.0.1:0")); // TODO: Use actual address

        self.raft.initialize(nodes.into()).await?;
        Ok(())
    }

    /// Add a node to the cluster
    pub async fn add_node(&self, node_id: BopNodeId, address: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let node = BasicNode::new(address);
        self.raft.add_learner(node_id, node, true).await?;
        self.raft.change_membership([node_id], false).await?;
        Ok(())
    }

    /// Remove a node from the cluster
    pub async fn remove_node(&self, node_id: BopNodeId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.raft.change_membership([], false).await?;
        Ok(())
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        self.raft.current_leader().await == Some(self.node_id)
    }

    /// Get current cluster metrics
    pub async fn metrics(&self) -> openraft::RaftMetrics<BopNodeId, BopNode> {
        self.raft.metrics().borrow().clone()
    }
}
*/

pub fn greeting() -> &'static str {
    "Hello from bop-rs!"
}

// Import our comprehensive raft module
pub mod raft;

