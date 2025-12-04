//! Multi-Node Raft Cluster Example
//!
//! This example demonstrates a multi-node, multi-group Raft cluster using
//! maniac's async runtime and networking stack.
//!
//! Run 3 nodes on different ports:
//! Terminal 1: cargo run --example raft_multi_node -- --node-id 1 --port 5001 --peer 5002 --peer 5003
//! Terminal 2: cargo run --example raft_multi_node -- --node-id 2 --port 5002 --peer 5001 --peer 5003
//! Terminal 3: cargo run --example raft_multi_node -- --node-id 3 --port 5003 --peer 5001 --peer 5002

use maniac::{
    net::TcpListener,
    raft::multi::{
        MultiRaftNetworkFactory, MultiplexedTransport,
        config::MultiRaftConfig,
        manager::MultiRaftManager,
        maniac_rpc_server::{ManiacRpcServer, ManiacRpcServerConfig},
        maniac_storage::{ManiacRaftStorage, ManiacStorageConfig},
        maniac_tcp_transport::{ManiacTcpTransport, ManiacTcpTransportConfig},
    },
    spawn,
    sync::Arc,
    time::{Duration, sleep},
};
use maniac_raft::{BasicNode, EntryPayload, Raft, RaftMetrics, RaftTypeConfig};
use serde::{Deserialize, Serialize};
use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ApplicationRequest {
    Set { key: String, value: String },
    Delete { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ApplicationResponse {
    Ok,
    Value(String),
    NotFound,
}

struct ApplicationData;

impl RaftTypeConfig for ApplicationData {
    type D = ApplicationRequest;
    type R = ApplicationResponse;
    type NodeId = u64;
    type Node = BasicNode;
}

#[derive(Debug)]
struct RaftNode {
    node_id: u64,
    port: u16,
    peers: Vec<SocketAddr>,
}

impl RaftNode {
    fn new(node_id: u64, port: u16, peers: Vec<SocketAddr>) -> Self {
        Self {
            node_id,
            port,
            peers,
        }
    }

    async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        println!("[Node {}] Starting on port {}...", self.node_id, self.port);

        let bind_addr: SocketAddr = (IpAddr::V4(Ipv4Addr::LOCALHOST), self.port).into();

        // Configure transport
        let transport_config = ManiacTcpTransportConfig {
            bind_addr,
            ..Default::default()
        };
        let transport: ManiacTcpTransport<ApplicationData> =
            ManiacTcpTransport::new(transport_config);

        // Configure storage
        let storage_dir = PathBuf::from(format!("./raft-data/node-{}", self.node_id));
        let storage_config = ManiacStorageConfig {
            base_dir: storage_dir,
            sync_on_write: true,
            ..Default::default()
        };
        let storage: ManiacRaftStorage<ApplicationData> = ManiacRaftStorage::new(storage_config)?;

        // Configure multi-raft
        let multi_config = MultiRaftConfig {
            heartbeat_interval: Duration::from_millis(100),
        };

        let manager = Arc::new(MultiRaftManager::new(transport, storage, multi_config));

        // Setup RPC server
        let server_config = ManiacRpcServerConfig {
            bind_addr,
            ..Default::default()
        };
        let server = Arc::new(ManiacRpcServer::new(server_config, manager.clone()));

        // Start server task
        let server_clone = server.clone();
        spawn(async move {
            if let Err(e) = server_clone.serve().await {
                eprintln!("[Node {}] Server error: {}", self.node_id, e);
            }
        });

        println!(
            "[Node {}] RPC server listening on {}",
            self.node_id, bind_addr
        );

        // Create initial Raft group
        self.create_group(1, manager.clone()).await?;

        // Give the cluster time to stabilize
        sleep(Duration::from_secs(2)).await;

        // Show leadership status
        self.show_leader_status(manager.clone()).await;

        // If this is the leader, propose some data
        if let Ok(leadership) = self.check_leadership(manager.clone(), 1).await {
            if leadership.is_leader(&self.node_id) {
                println!(
                    "[Node {}] I am the leader of group 1, proposing data...",
                    self.node_id
                );
                self.propose_sample_data(manager.clone()).await?;
            }
        }

        // Keep running
        println!("[Node {}] Running... Press Ctrl+C to stop", self.node_id);
        loop {
            sleep(Duration::from_secs(5)).await;
            self.show_metrics(manager.clone()).await;
        }
    }

    async fn create_group(
        &self,
        group_id: u64,
        manager: Arc<
            MultiRaftManager<
                ApplicationData,
                ManiacTcpTransport<ApplicationData>,
                ManiacRaftStorage<ApplicationData>,
            >,
        >,
    ) -> Result<Raft<ApplicationData>, Box<dyn std::error::Error>> {
        let members: std::collections::BTreeMap<_, _> = self
            .peers
            .iter()
            .enumerate()
            .map(|(idx, addr)| {
                (
                    (idx + 1) as u64,
                    BasicNode::new(addr.to_string(), Default::default()),
                )
            })
            .collect();

        let membership =
            maniac_raft::Membership::new(vec![members.keys().cloned().collect()], None);
        let config = Arc::new(maniac_raft::Config {
            heartbeat_interval: 500,
            election_timeout_min: 1500,
            election_timeout_max: 3000,
            ..Default::default()
        });

        manager.add_group(group_id, self.node_id, config).await
    }

    async fn show_leader_status(
        &self,
        manager: Arc<
            MultiRaftManager<
                ApplicationData,
                ManiacTcpTransport<ApplicationData>,
                ManiacRaftStorage<ApplicationData>,
            >,
        >,
    ) {
        if let Some(raft) = manager.get_group(1) {
            let metrics = raft.metrics().borrow().clone();
            let leader_id = metrics.current_leader;

            match leader_id {
                Some(leader) => {
                    println!(
                        "[Node {}] Current leader of group 1: {}",
                        self.node_id, leader
                    );
                }
                None => {
                    println!("[Node {}] No leader elected for group 1 yet", self.node_id);
                }
            }
        }
    }

    async fn check_leadership(
        &self,
        manager: Arc<
            MultiRaftManager<
                ApplicationData,
                ManiacTcpTransport<ApplicationData>,
                ManiacRaftStorage<ApplicationData>,
            >,
        >,
        group_id: u64,
    ) -> Result<RaftMetrics<ApplicationData>, Box<dyn std::error::Error>> {
        if let Some(raft) = manager.get_group(group_id) {
            let metrics = raft.metrics().borrow().clone();
            Ok(metrics)
        } else {
            Err("Group not found".into())
        }
    }

    async fn propose_sample_data(
        &self,
        manager: Arc<
            MultiRaftManager<
                ApplicationData,
                ManiacTcpTransport<ApplicationData>,
                ManiacRaftStorage<ApplicationData>,
            >,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(raft) = manager.get_group(1) {
            for i in 0..5 {
                let request = ApplicationRequest::Set {
                    key: format!("key-{}-{}", self.node_id, i),
                    value: format!("value-{}-{}", self.node_id, i),
                };

                match raft.client_write(request).await {
                    Ok(resp) => {
                        println!(
                            "[Node {}] Successfully wrote entry {}: {:?}",
                            self.node_id, i, resp.data
                        );
                    }
                    Err(e) => {
                        eprintln!("[Node {}] Failed to write entry {}: {}", self.node_id, i, e);
                    }
                }

                sleep(Duration::from_millis(500)).await;
            }
        }

        Ok(())
    }

    async fn show_metrics(
        &self,
        manager: Arc<
            MultiRaftManager<
                ApplicationData,
                ManiacTcpTransport<ApplicationData>,
                ManiacRaftStorage<ApplicationData>,
            >,
        >,
    ) {
        if let Some(raft) = manager.get_group(1) {
            let metrics = raft.metrics().borrow().clone();
            println!("[Node {}] {:?}", self.node_id, metrics);
        }
    }
}

#[maniac::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    let mut node_id = None;
    let mut port = None;
    let mut peers = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--node-id" => {
                if i + 1 < args.len() {
                    node_id = Some(args[i + 1].parse::<u64>()?);
                    i += 2;
                } else {
                    eprintln!("Expected value after --node-id");
                    std::process::exit(1);
                }
            }
            "--port" => {
                if i + 1 < args.len() {
                    port = Some(args[i + 1].parse::<u16>()?);
                    i += 2;
                } else {
                    eprintln!("Expected value after --port");
                    std::process::exit(1);
                }
            }
            "--peer" => {
                if i + 1 < args.len() {
                    let peer_addr: SocketAddr = args[i + 1].parse()?;
                    peers.push(peer_addr);
                    i += 2;
                } else {
                    eprintln!("Expected value after --peer");
                    std::process::exit(1);
                }
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                eprintln!("Usage: --node-id <id> --port <port> --peer <addr> [--peer <addr>]...");
                std::process::exit(1);
            }
        }
    }

    let node_id = node_id.unwrap_or_else(|| {
        eprintln!("--node-id is required");
        eprintln!("Usage: --node-id <id> --port <port> --peer <addr> [--peer <addr>]...");
        std::process::exit(1);
    });

    let port = port.unwrap_or_else(|| {
        eprintln!("--port is required");
        eprintln!("Usage: --node-id <id> --port <port> --peer <addr> [--peer <addr>]...");
        std::process::exit(1);
    });

    println!("Starting Raft node {} on port {}", node_id, port);
    println!("Peers: {:?}", peers);

    let node = RaftNode::new(node_id, port, peers);
    node.start().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let addr1: SocketAddr = "127.0.0.1:5001".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:5002".parse().unwrap();
        let peers = vec![addr1, addr2];

        let node = RaftNode::new(1, 5001, peers.clone());
        assert_eq!(node.node_id, 1);
        assert_eq!(node.port, 5001);
        assert_eq!(node.peers.len(), 2);
    }
}
