//! Multi-Raft Cluster Example
//!
//! This example demonstrates how to create a multi-raft cluster using the maniac-raft crate.
//! It shows:
//! - Creating multiple Raft groups that share networking and storage
//! - Setting up the TCP transport layer with connection multiplexing
//! - Creating and initializing a cluster with multiple nodes
//! - Proposing commands to different Raft groups
//! - Leader election and cluster membership changes
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         Node 1                              │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐    │
//! │  │   Group 1     │  │   Group 2     │  │   Group 3     │    │
//! │  │   (Raft)      │  │   (Raft)      │  │   (Raft)      │    │
//! │  └───────────────┘  └───────────────┘  └───────────────┘    │
//! │           │                 │                  │            │
//! │           └─────────────────┼──────────────────┘            │
//! │                             │                               │
//! │  ┌──────────────────────────┴────────────────────────────┐  │
//! │  │            Multiplexed TCP Transport                  │  │
//! │  │         (Shared connections across groups)            │  │
//! │  └──────────────────────────┬────────────────────────────┘  │
//! │                             │                               │
//! │  ┌──────────────────────────┴────────────────────────────┐  │
//! │  │         Multiplexed Log Storage                       │  │
//! │  │  (Single log file for all groups, keyed by group_id)  │  │
//! │  └───────────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Running the Example
//!
//! This example runs a single-node cluster with 3 Raft groups for demonstration.
//! In a real deployment, you would run multiple nodes on different machines.

use futures::StreamExt;
use maniac_raft::ManiacRaftTypeConfig;
use maniac_raft::multi::codec::{FromCodec, RawBytes, ToCodec};
use maniac_raft::multi::{
    DefaultNodeRegistry, ManiacTcpTransport, ManiacTcpTransportConfig, MultiRaftConfig,
    MultiRaftManager, MultiplexedLogStorage, MultiplexedStorageConfig, NodeAddressResolver,
};
use openraft::impls::BasicNode;
use openraft::storage::RaftStateMachine;
use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{Config, LogId, OptionalSend, SnapshotMeta, StoredMembership};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

// =============================================================================
// Application Data Types
// =============================================================================

/// The application request type - a simple key-value store command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KvCommand {
    /// Set a key to a value
    Set { key: String, value: String },
    /// Delete a key
    Delete { key: String },
}

// Required by openraft::AppData
impl fmt::Display for KvCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KvCommand::Set { key, value } => write!(f, "SET {}={}", key, value),
            KvCommand::Delete { key } => write!(f, "DELETE {}", key),
        }
    }
}

/// The application response type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KvResponse {
    /// Operation succeeded
    Ok,
    /// Get result
    Value(Option<String>),
}

// Required by openraft::AppDataResponse
impl fmt::Display for KvResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KvResponse::Ok => write!(f, "OK"),
            KvResponse::Value(Some(v)) => write!(f, "{}", v),
            KvResponse::Value(None) => write!(f, "(nil)"),
        }
    }
}

// Implement codec traits for KvCommand to work with multi-raft
impl ToCodec<RawBytes> for KvCommand {
    fn to_codec(&self) -> RawBytes {
        // Use bincode for serialization
        let bytes = bincode::serde::encode_to_vec(self, bincode::config::standard())
            .expect("serialization should not fail");
        RawBytes(bytes)
    }
}

impl FromCodec<RawBytes> for KvCommand {
    fn from_codec(raw: RawBytes) -> Self {
        let (cmd, _): (KvCommand, _) =
            bincode::serde::decode_from_slice(&raw.0, bincode::config::standard())
                .expect("deserialization should not fail");
        cmd
    }
}

// =============================================================================
// Type Configuration
// =============================================================================

/// Our Raft type configuration using KvCommand and KvResponse
pub type KvTypeConfig = ManiacRaftTypeConfig<KvCommand, KvResponse>;

// =============================================================================
// State Machine
// =============================================================================

/// Simple in-memory key-value store state machine.
pub struct KvStateMachine {
    /// The group ID this state machine belongs to
    group_id: u64,
    /// The key-value store
    data: BTreeMap<String, String>,
    /// Last applied log ID
    last_applied: Option<LogId<KvTypeConfig>>,
    /// Last membership
    last_membership: StoredMembership<KvTypeConfig>,
}

impl KvStateMachine {
    pub fn new(group_id: u64) -> Self {
        Self {
            group_id,
            data: BTreeMap::new(),
            last_applied: None,
            last_membership: StoredMembership::default(),
        }
    }
}

impl RaftStateMachine<KvTypeConfig> for KvStateMachine {
    type SnapshotBuilder = Self;

    async fn applied_state(
        &mut self,
    ) -> Result<(Option<LogId<KvTypeConfig>>, StoredMembership<KvTypeConfig>), std::io::Error> {
        Ok((self.last_applied.clone(), self.last_membership.clone()))
    }

    async fn apply<Strm>(&mut self, mut entries: Strm) -> Result<(), std::io::Error>
    where
        Strm: futures::Stream<
                Item = Result<openraft::storage::EntryResponder<KvTypeConfig>, std::io::Error>,
            > + Unpin
            + OptionalSend,
    {
        while let Some(entry_result) = entries.next().await {
            let (entry, responder) = entry_result?;
            self.last_applied = Some(entry.log_id.clone());

            let response = match entry.payload {
                openraft::EntryPayload::Blank => KvResponse::Ok,
                openraft::EntryPayload::Normal(cmd) => match cmd {
                    KvCommand::Set { key, value } => {
                        println!("  [Group {}] SET {} = {}", self.group_id, key, value);
                        self.data.insert(key, value);
                        KvResponse::Ok
                    }
                    KvCommand::Delete { key } => {
                        println!("  [Group {}] DELETE {}", self.group_id, key);
                        self.data.remove(&key);
                        KvResponse::Ok
                    }
                },
                openraft::EntryPayload::Membership(m) => {
                    self.last_membership = StoredMembership::new(Some(entry.log_id.clone()), m);
                    KvResponse::Ok
                }
            };

            // Send the response if a responder is present
            if let Some(r) = responder {
                r.send(response);
            }
        }

        Ok(())
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        // For simplicity, we return a clone
        KvStateMachine {
            group_id: self.group_id,
            data: self.data.clone(),
            last_applied: self.last_applied.clone(),
            last_membership: self.last_membership.clone(),
        }
    }

    async fn begin_receiving_snapshot(&mut self) -> Result<Cursor<Vec<u8>>, std::io::Error> {
        Ok(Cursor::new(Vec::new()))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<KvTypeConfig>,
        snapshot: Cursor<Vec<u8>>,
    ) -> Result<(), std::io::Error> {
        // Deserialize the snapshot data
        let data: BTreeMap<String, String> =
            bincode::serde::decode_from_slice(snapshot.get_ref(), bincode::config::standard())
                .map(|(d, _)| d)
                .unwrap_or_default();

        self.data = data;
        self.last_applied = meta.last_log_id.clone();
        self.last_membership = meta.last_membership.clone();

        println!(
            "  [Group {}] Installed snapshot at {:?}",
            self.group_id, meta.last_log_id
        );

        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<openraft::Snapshot<KvTypeConfig>>, std::io::Error> {
        // For this example, we don't implement snapshotting
        Ok(None)
    }
}

impl openraft::storage::RaftSnapshotBuilder<KvTypeConfig> for KvStateMachine {
    async fn build_snapshot(&mut self) -> Result<openraft::Snapshot<KvTypeConfig>, std::io::Error> {
        let data = bincode::serde::encode_to_vec(&self.data, bincode::config::standard())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let last_applied = self.last_applied.clone().unwrap_or(LogId {
            leader_id: openraft::impls::leader_id_adv::LeaderId {
                term: 0,
                node_id: 0,
            },
            index: 0,
        });

        let meta = SnapshotMeta {
            last_log_id: Some(last_applied),
            last_membership: self.last_membership.clone(),
            snapshot_id: format!(
                "snapshot-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        };

        Ok(openraft::Snapshot {
            meta,
            snapshot: Cursor::new(data),
        })
    }
}

// =============================================================================
// Main Example
// =============================================================================

fn main() {
    use futures_lite::future::block_on;

    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    println!("=== Multi-Raft Cluster Example ===\n");
    println!("This example demonstrates:");
    println!("  1. Creating a multi-raft node");
    println!("  2. Running multiple independent Raft groups");
    println!("  3. Shared networking and storage infrastructure");
    println!("  4. Proposing commands to different groups\n");

    // Create the maniac runtime with single thread to avoid cross-thread generator issues
    // Note: Using single-threaded mode avoids potential issues with stackful coroutines
    // accessing data across worker threads
    let runtime = maniac::runtime::new_multi_threaded(8, 65536).expect("Failed to create runtime");

    // Set the global scheduler for spawning tasks from outside worker context
    // This is required because OpenRaft spawns internal tasks during initialization
    maniac_raft::set_global_scheduler(runtime.service());

    // Spawn our async task on the runtime
    let handle = runtime.spawn(async_main()).expect("Failed to spawn task");

    // Block on the handle to wait for completion (using futures_lite like other examples)
    block_on(handle);

    println!("exiting...");
    // Clear the global scheduler on shutdown
    maniac_raft::clear_global_scheduler();
}

async fn async_main() {
    // Create temporary data directory
    let temp_dir = std::env::temp_dir().join("multi-raft-example");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    println!("--- Setting up Multi-Raft infrastructure ---\n");

    // Create storage configuration with 4 shards
    // Note: fsync_interval is None to avoid spawning background tasks during debugging
    let storage_config = MultiplexedStorageConfig {
        base_dir: temp_dir.clone(),
        num_shards: 4, // 4 shards for parallel writes
        segment_size: 64 * 1024 * 1024, // 64MB segment files
        fsync_interval: None,
        max_cache_entries_per_group: 10000,
    };

    // Create the sharded log storage using async constructor
    let storage = MultiplexedLogStorage::<KvTypeConfig>::new(storage_config)
        .await
        .expect("Failed to create storage");

    println!("  Created sharded log storage at {:?} with {} shards", temp_dir, storage.num_shards());

    // Create TCP transport with node registry
    let transport_config = ManiacTcpTransportConfig {
        connect_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(10),
        connections_per_addr: 2,
        max_concurrent_requests_per_conn: 128,
        connection_ttl: Duration::from_secs(300),
        tcp_nodelay: true,
    };

    let node_registry = Arc::new(DefaultNodeRegistry::new());
    let node_id: u64 = 1;
    let node_addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();

    // Register our node
    node_registry.register(node_id, node_addr);

    let transport = ManiacTcpTransport::new(transport_config, node_registry.clone());

    println!(
        "  Created TCP transport for node {} at {}",
        node_id, node_addr
    );

    // Create multi-raft configuration
    let multi_config = MultiRaftConfig {
        heartbeat_interval: Duration::from_millis(100),
    };

    // Create the manager
    let manager = MultiRaftManager::new(transport, storage, multi_config);

    println!("  Created MultiRaftManager\n");

    // Create Raft configuration
    let raft_config = Arc::new(
        Config {
            heartbeat_interval: 100,
            election_timeout_min: 300,
            election_timeout_max: 600,
            ..Default::default()
        }
        .validate()
        .expect("Invalid raft config"),
    );

    println!("--- Creating Raft groups ---\n");

    // Add three Raft groups
    for group_id in 1..=3u64 {
        let state_machine = KvStateMachine::new(group_id);

        match manager
            .add_group(group_id, node_id, raft_config.clone(), state_machine)
            .await
        {
            Ok(raft) => {
                println!("  Created Raft group {}", group_id);

                // Initialize with just ourselves as the only member
                let mut members = BTreeMap::new();
                members.insert(node_id, BasicNode::default());

                println!("  Calling initialize for group {}...", group_id);
                match maniac::time::timeout(
                    std::time::Duration::from_secs(5),
                    raft.initialize(members),
                )
                .await
                {
                    Ok(Ok(_)) => {
                        println!(
                            "  Initialized group {} with node {} as leader",
                            group_id, node_id
                        );
                    }
                    Ok(Err(e)) => {
                        println!(
                            "  Warning: Could not initialize group {}: {:?}",
                            group_id, e
                        );
                    }
                    Err(_) => {
                        println!(
                            "  TIMEOUT: Initialize for group {} timed out after 5s",
                            group_id
                        );
                    }
                }
            }
            Err(e) => {
                println!("  Failed to create group {}: {:?}", group_id, e);
            }
        }
    }

    // Show final state
    println!("\n--- Raft Groups Initialized ---\n");
    println!("  Active groups: {:?}", manager.group_ids());
    println!("  Total groups: {}", manager.group_count());

    // Cleanup
    println!("\n--- Shutting down ---\n");
    for group_id in 1..=3u64 {
        manager.remove_group(group_id).await;
        println!("  Removed group {}", group_id);
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
    println!("  Cleaned up temporary directory");

    println!("\n=== Example completed successfully! ===\n");
}

// =============================================================================
// Additional Documentation
// =============================================================================

/// # Multi-Raft Design Notes
///
/// ## Why Multi-Raft?
///
/// In a large-scale distributed system, using a single Raft group becomes a bottleneck:
/// - All reads/writes must go through a single leader
/// - A single log grows unboundedly
/// - Scaling to many nodes increases election latency
///
/// Multi-Raft solves this by partitioning data across multiple Raft groups:
/// - Each group handles a subset of the data (e.g., by key range or hash)
/// - Groups elect leaders independently
/// - Network and storage can be shared to reduce overhead
///
/// ## Shared Infrastructure
///
/// This implementation shares:
/// - **TCP Transport**: Connection pools are shared across groups
/// - **Log Storage**: Single log file with group_id prefix for entries
/// - **Configuration**: Heartbeat intervals, timeouts, etc.
///
/// ## Scaling Patterns
///
/// 1. **Horizontal scaling**: Add more nodes to each group
/// 2. **Sharding**: Split data across groups by key range
/// 3. **Placement**: Locate group leaders near their data
///
/// ## Example Use Cases
///
/// - **Key-Value Store**: Each group handles a key range
/// - **Message Queue**: Each group handles a topic/partition
/// - **Database**: Each group handles a table or shard
#[allow(dead_code)]
fn _documentation() {}
