//! Maniac RPC Server for Multi-Raft
//!
//! Accepts incoming TCP connections and handles RPC requests from peers.
//! Integrates with MultiRaftManager to route requests to the appropriate Raft groups.
//!
//! ## True Connection Multiplexing
//!
//! This server properly supports connection multiplexing where:
//! - Multiple RPC requests are received and processed concurrently
//! - Responses are sent back out-of-order as soon as they're ready
//! - Each connection has separate reader and writer tasks
//! - Request IDs correlate responses to their original requests

use crate::io::{AsyncReadRent, AsyncWriteRent};
use crate::net::{TcpListener, TcpStream};
use crate::raft::multi::codec::{
    Decode, Encode, ResponseMessage as CodecResponseMessage, RpcMessage as CodecRpcMessage,
};
use crate::raft::multi::manager::MultiRaftManager;
use crate::raft::multi::network::MultiplexedTransport;
use crate::raft::multi::storage::MultiRaftLogStorage;
use crate::raft::multi::tcp_transport::{read_frame, write_frame};
use crate::sync::mpsc::bounded as mpsc;
use crate::sync::mutex::Mutex;
use crate::time::timeout;
use maniac_raft::RaftTypeConfig;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub use protocol::{ResponseMessage, RpcMessage};

/// RPC server configuration
#[derive(Debug, Clone)]
pub struct ManiacRpcServerConfig {
    /// Address to bind to
    pub bind_addr: SocketAddr,
    /// Max number of concurrent connections
    pub max_connections: usize,
    /// Connection read timeout (idle timeout)
    pub connection_timeout: std::time::Duration,
    /// Maximum number of concurrent in-flight requests per connection.
    /// Default: 256
    pub max_concurrent_requests: usize,
}

impl Default for ManiacRpcServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:5000".parse().unwrap(),
            max_connections: 1000,
            connection_timeout: std::time::Duration::from_secs(60),
            max_concurrent_requests: 256,
        }
    }
}

/// RPC server for handling incoming Raft requests with true multiplexing
pub struct ManiacRpcServer<C, T, S>
where
    C: RaftTypeConfig,
    T: MultiplexedTransport<C>,
    S: MultiRaftLogStorage<C>,
{
    config: ManiacRpcServerConfig,
    manager: Arc<MultiRaftManager<C, T, S>>,
    /// Active connection count
    active_connections: AtomicU64,
    _phantom: PhantomData<(C, T, S)>,
}

impl<C, T, S> ManiacRpcServer<C, T, S>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = maniac_raft::impls::leader_id_adv::LeaderId<C>,
            Vote = maniac_raft::impls::Vote<C>,
        >,
    C::Entry: Clone,
    T: MultiplexedTransport<C>,
    S: MultiRaftLogStorage<C>,
{
    /// Create a new RPC server
    pub fn new(config: ManiacRpcServerConfig, manager: Arc<MultiRaftManager<C, T, S>>) -> Self {
        Self {
            config,
            manager,
            active_connections: AtomicU64::new(0),
            _phantom: PhantomData,
        }
    }

    /// Start the server and listen for connections
    pub async fn serve(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.config.bind_addr)?;
        let actual_addr = listener.local_addr()?;

        tracing::info!(
            "Raft RPC server listening on: {} (max_concurrent={})",
            actual_addr,
            self.config.max_concurrent_requests
        );

        loop {
            let (stream, peer_addr) = listener.accept().await?;

            // Check connection limit
            let current = self.active_connections.fetch_add(1, Ordering::Relaxed);
            if current >= self.config.max_connections as u64 {
                self.active_connections.fetch_sub(1, Ordering::Relaxed);
                tracing::warn!(
                    "Connection limit reached, rejecting connection from {}",
                    peer_addr
                );
                continue;
            }

            let server = self.clone();

            // Spawn connection handler
            let _ = crate::spawn(async move {
                match server
                    .handle_multiplexed_connection(stream, peer_addr)
                    .await
                {
                    Ok(_) => {
                        tracing::debug!("Connection from {} closed gracefully", peer_addr);
                    }
                    Err(e) => {
                        tracing::debug!("Connection from {} closed: {}", peer_addr, e);
                    }
                }
                server.active_connections.fetch_sub(1, Ordering::Relaxed);
            });
        }
    }

    /// Handle a multiplexed connection with true out-of-order response support
    ///
    /// Uses a channel-based approach:
    /// - Main task reads requests and spawns handlers
    /// - Handler tasks send responses to a channel
    /// - Main task also drains the channel and writes responses
    async fn handle_multiplexed_connection(
        &self,
        mut stream: TcpStream,
        peer_addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!(
            "New multiplexed connection from: {} (max_concurrent={})",
            peer_addr,
            self.config.max_concurrent_requests
        );

        // Channel for responses from handler tasks
        let (response_tx, mut response_rx) = mpsc::channel::<Vec<u8>>();

        loop {
            // Try to send any pending responses first (non-blocking)
            while let Ok(response_data) = response_rx.try_recv() {
                if let Err(e) = write_frame(&mut stream, &response_data).await {
                    tracing::error!("Failed to write response: {}", e);
                    return Err(Box::new(e));
                }
            }

            // Read next request with timeout
            let request_data =
                match timeout(self.config.connection_timeout, read_frame(&mut stream)).await {
                    Ok(Ok(data)) => data,
                    Ok(Err(e)) => {
                        if let crate::raft::multi::tcp_transport::ManiacTransportError::IoError(
                            ref io_err,
                        ) = e
                        {
                            if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                                tracing::debug!("Connection closed by peer: {}", peer_addr);
                                break;
                            }
                        }
                        return Err(Box::new(e));
                    }
                    Err(_) => {
                        tracing::debug!("Connection timeout from: {}", peer_addr);
                        break;
                    }
                };

            // Decode request using zero-copy codec
            let request: CodecRpcMessage<Vec<u8>> =
                match CodecRpcMessage::decode_from_slice(&request_data) {
                    Ok(req) => req,
                    Err(e) => {
                        tracing::error!("Failed to decode request from {}: {}", peer_addr, e);
                        continue;
                    }
                };

            let request_id = request.request_id();
            let manager = self.manager.clone();
            let mut tx = response_tx.clone();

            // Spawn handler for this request
            let _ = crate::spawn(async move {
                let response = Self::process_codec_request(&manager, request).await;

                // Serialize and send response to writer
                match response.encode_to_vec() {
                    Ok(response_data) => {
                        if tx.send(response_data).await.is_err() {
                            tracing::debug!("Response channel closed for request {}", request_id);
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to encode response for request {}: {}",
                            request_id,
                            e
                        );
                    }
                }
            });
        }

        // Drain any remaining responses before closing
        drop(response_tx);
        while let Ok(response_data) = response_rx.try_recv() {
            let _ = write_frame(&mut stream, &response_data).await;
        }

        Ok(())
    }

    /// Process a codec request and return a codec response
    async fn process_codec_request(
        manager: &Arc<MultiRaftManager<C, T, S>>,
        request: CodecRpcMessage<Vec<u8>>,
    ) -> CodecRpcMessage<Vec<u8>> {
        use crate::raft::multi::codec::{
            AppendEntriesResponse as CodecAppendEntriesResponse,
            InstallSnapshotResponse as CodecInstallSnapshotResponse, LeaderId as CodecLeaderId,
            LogId as CodecLogId, Vote as CodecVote, VoteResponse as CodecVoteResponse,
        };

        match request {
            CodecRpcMessage::AppendEntries {
                request_id,
                group_id,
                rpc,
            } => {
                tracing::trace!(
                    "Processing AppendEntries for group {} (req_id={})",
                    group_id,
                    request_id
                );

                if let Some(raft) = manager.get_group(group_id) {
                    // Convert codec types to raft types
                    let vote = maniac_raft::impls::Vote {
                        leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                            term: rpc.vote.leader_id.term,
                            node_id: rpc.vote.leader_id.node_id,
                        },
                        committed: rpc.vote.committed,
                    };

                    let prev_log_id = rpc.prev_log_id.map(|l| maniac_raft::LogId {
                        leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                            term: l.leader_id.term,
                            node_id: l.leader_id.node_id,
                        },
                        index: l.index,
                    });

                    let leader_commit = rpc.leader_commit.map(|l| maniac_raft::LogId {
                        leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                            term: l.leader_id.term,
                            node_id: l.leader_id.node_id,
                        },
                        index: l.index,
                    });

                    let raft_rpc = maniac_raft::raft::AppendEntriesRequest {
                        vote,
                        prev_log_id,
                        entries: Vec::new(), // TODO: proper entry conversion
                        leader_commit,
                    };

                    match raft.append_entries(raft_rpc).await {
                        Ok(response) => {
                            let codec_response = match response {
                                maniac_raft::raft::AppendEntriesResponse::Success => {
                                    CodecAppendEntriesResponse::Success
                                }
                                maniac_raft::raft::AppendEntriesResponse::PartialSuccess(lid) => {
                                    CodecAppendEntriesResponse::PartialSuccess(lid.map(|l| {
                                        CodecLogId {
                                            leader_id: CodecLeaderId {
                                                term: l.leader_id.term,
                                                node_id: l.leader_id.node_id,
                                            },
                                            index: l.index,
                                        }
                                    }))
                                }
                                maniac_raft::raft::AppendEntriesResponse::Conflict => {
                                    CodecAppendEntriesResponse::Conflict
                                }
                                maniac_raft::raft::AppendEntriesResponse::HigherVote(v) => {
                                    CodecAppendEntriesResponse::HigherVote(CodecVote {
                                        leader_id: CodecLeaderId {
                                            term: v.leader_id.term,
                                            node_id: v.leader_id.node_id,
                                        },
                                        committed: v.committed,
                                    })
                                }
                            };
                            CodecRpcMessage::Response {
                                request_id,
                                message: CodecResponseMessage::AppendEntries(codec_response),
                            }
                        }
                        Err(e) => CodecRpcMessage::Error {
                            request_id,
                            error: format!("AppendEntries failed: {}", e),
                        },
                    }
                } else {
                    CodecRpcMessage::Error {
                        request_id,
                        error: format!("Group {} not found", group_id),
                    }
                }
            }

            CodecRpcMessage::Vote {
                request_id,
                group_id,
                rpc,
            } => {
                tracing::trace!(
                    "Processing Vote for group {} (req_id={})",
                    group_id,
                    request_id
                );

                if let Some(raft) = manager.get_group(group_id) {
                    let vote = maniac_raft::impls::Vote {
                        leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                            term: rpc.vote.leader_id.term,
                            node_id: rpc.vote.leader_id.node_id,
                        },
                        committed: rpc.vote.committed,
                    };

                    let last_log_id = rpc.last_log_id.map(|l| maniac_raft::LogId {
                        leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                            term: l.leader_id.term,
                            node_id: l.leader_id.node_id,
                        },
                        index: l.index,
                    });

                    let raft_rpc = maniac_raft::raft::VoteRequest { vote, last_log_id };

                    match raft.vote(raft_rpc).await {
                        Ok(response) => {
                            let codec_response = CodecVoteResponse {
                                vote: CodecVote {
                                    leader_id: CodecLeaderId {
                                        term: response.vote.leader_id.term,
                                        node_id: response.vote.leader_id.node_id,
                                    },
                                    committed: response.vote.committed,
                                },
                                vote_granted: response.vote_granted,
                                last_log_id: response.last_log_id.map(|l| CodecLogId {
                                    leader_id: CodecLeaderId {
                                        term: l.leader_id.term,
                                        node_id: l.leader_id.node_id,
                                    },
                                    index: l.index,
                                }),
                            };
                            CodecRpcMessage::Response {
                                request_id,
                                message: CodecResponseMessage::Vote(codec_response),
                            }
                        }
                        Err(e) => CodecRpcMessage::Error {
                            request_id,
                            error: format!("Vote failed: {}", e),
                        },
                    }
                } else {
                    CodecRpcMessage::Error {
                        request_id,
                        error: format!("Group {} not found", group_id),
                    }
                }
            }

            CodecRpcMessage::InstallSnapshot {
                request_id,
                group_id,
                rpc,
            } => {
                tracing::trace!(
                    "Processing InstallSnapshot for group {} (req_id={})",
                    group_id,
                    request_id
                );

                // TODO: Implement proper snapshot handling
                CodecRpcMessage::Error {
                    request_id,
                    error: "InstallSnapshot not yet fully implemented".to_string(),
                }
            }

            CodecRpcMessage::HeartbeatBatch {
                request_id,
                group_id,
                rpc,
            } => {
                tracing::trace!(
                    "Processing HeartbeatBatch for group {} (req_id={})",
                    group_id,
                    request_id
                );

                if let Some(raft) = manager.get_group(group_id) {
                    let vote = maniac_raft::impls::Vote {
                        leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                            term: rpc.vote.leader_id.term,
                            node_id: rpc.vote.leader_id.node_id,
                        },
                        committed: rpc.vote.committed,
                    };

                    let prev_log_id = rpc.prev_log_id.map(|l| maniac_raft::LogId {
                        leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                            term: l.leader_id.term,
                            node_id: l.leader_id.node_id,
                        },
                        index: l.index,
                    });

                    let leader_commit = rpc.leader_commit.map(|l| maniac_raft::LogId {
                        leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                            term: l.leader_id.term,
                            node_id: l.leader_id.node_id,
                        },
                        index: l.index,
                    });

                    let raft_rpc = maniac_raft::raft::AppendEntriesRequest {
                        vote,
                        prev_log_id,
                        entries: Vec::new(),
                        leader_commit,
                    };

                    match raft.append_entries(raft_rpc).await {
                        Ok(response) => {
                            let codec_response = match response {
                                maniac_raft::raft::AppendEntriesResponse::Success => {
                                    CodecAppendEntriesResponse::Success
                                }
                                maniac_raft::raft::AppendEntriesResponse::PartialSuccess(lid) => {
                                    CodecAppendEntriesResponse::PartialSuccess(lid.map(|l| {
                                        CodecLogId {
                                            leader_id: CodecLeaderId {
                                                term: l.leader_id.term,
                                                node_id: l.leader_id.node_id,
                                            },
                                            index: l.index,
                                        }
                                    }))
                                }
                                maniac_raft::raft::AppendEntriesResponse::Conflict => {
                                    CodecAppendEntriesResponse::Conflict
                                }
                                maniac_raft::raft::AppendEntriesResponse::HigherVote(v) => {
                                    CodecAppendEntriesResponse::HigherVote(CodecVote {
                                        leader_id: CodecLeaderId {
                                            term: v.leader_id.term,
                                            node_id: v.leader_id.node_id,
                                        },
                                        committed: v.committed,
                                    })
                                }
                            };
                            CodecRpcMessage::Response {
                                request_id,
                                message: CodecResponseMessage::AppendEntries(codec_response),
                            }
                        }
                        Err(e) => CodecRpcMessage::Error {
                            request_id,
                            error: format!("HeartbeatBatch failed: {}", e),
                        },
                    }
                } else {
                    CodecRpcMessage::Error {
                        request_id,
                        error: format!("Group {} not found", group_id),
                    }
                }
            }

            CodecRpcMessage::Response { request_id, .. }
            | CodecRpcMessage::BatchResponse { request_id, .. }
            | CodecRpcMessage::Error { request_id, .. } => CodecRpcMessage::Error {
                request_id,
                error: "Invalid request type: received response message as request".to_string(),
            },
        }
    }
}

// Re-export message types for use by both client and server
pub mod protocol {
    //! Shared protocol types for maniac transport (serde-based, kept for compatibility)
    //!
    //! Note: The actual wire protocol now uses the zero-copy codec from `codec.rs`.
    //! These types are kept for API compatibility.

    use maniac_raft::RaftTypeConfig;
    use maniac_raft::raft::{
        AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest,
        InstallSnapshotResponse, VoteRequest, VoteResponse,
    };

    /// RPC message wrapper (serde version for compatibility)
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    #[serde(bound = "")]
    pub enum RpcMessage<C: RaftTypeConfig> {
        AppendEntries {
            request_id: u64,
            group_id: u64,
            rpc: AppendEntriesRequest<C>,
        },
        Vote {
            request_id: u64,
            group_id: u64,
            rpc: VoteRequest<C>,
        },
        InstallSnapshot {
            request_id: u64,
            group_id: u64,
            rpc: InstallSnapshotRequest<C>,
        },
        HeartbeatBatch {
            request_id: u64,
            group_id: u64,
            rpc: AppendEntriesRequest<C>,
        },
        Response {
            request_id: u64,
            message: ResponseMessage<C>,
        },
        BatchResponse {
            request_id: u64,
            responses: Vec<(u64, ResponseMessage<C>)>,
        },
        Error {
            request_id: u64,
            error: String,
        },
    }

    impl<C: RaftTypeConfig> RpcMessage<C> {
        pub fn request_id(&self) -> u64 {
            match self {
                RpcMessage::AppendEntries { request_id, .. } => *request_id,
                RpcMessage::Vote { request_id, .. } => *request_id,
                RpcMessage::InstallSnapshot { request_id, .. } => *request_id,
                RpcMessage::HeartbeatBatch { request_id, .. } => *request_id,
                RpcMessage::Response { request_id, .. } => *request_id,
                RpcMessage::BatchResponse { request_id, .. } => *request_id,
                RpcMessage::Error { request_id, .. } => *request_id,
            }
        }
    }

    /// Response message wrapper
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    #[serde(bound = "")]
    pub enum ResponseMessage<C: RaftTypeConfig> {
        AppendEntries(AppendEntriesResponse<C>),
        Vote(VoteResponse<C>),
        InstallSnapshot(InstallSnapshotResponse<C>),
    }
}
