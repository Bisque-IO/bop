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

use maniac::io::{AsyncReadRent, AsyncWriteRent};
use maniac::net::{TcpListener, TcpStream};
use crate::multi::codec::{
    Decode, Encode, ResponseMessage as CodecResponseMessage, RpcMessage as CodecRpcMessage,
    SnapshotMeta as CodecSnapshotMeta, Vote as CodecVote,
};
use crate::multi::manager::MultiRaftManager;
use crate::multi::network::MultiplexedTransport;
use crate::multi::storage::MultiRaftLogStorage;
use crate::multi::tcp_transport::{read_frame, write_frame};
use maniac::sync::mpsc::bounded as mpsc;
use maniac::sync::mutex::Mutex;
use maniac::time::timeout;
use dashmap::DashMap;
use openraft::RaftTypeConfig;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub use protocol::{ResponseMessage, RpcMessage};

/// Key for identifying an in-progress snapshot transfer
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SnapshotTransferKey {
    /// The group receiving the snapshot
    group_id: u64,
    /// The snapshot ID being transferred
    snapshot_id: String,
}

/// State for an in-progress chunked snapshot transfer
struct SnapshotAccumulator {
    /// Vote from the leader sending the snapshot
    vote: CodecVote,
    /// Snapshot metadata
    meta: CodecSnapshotMeta,
    /// Accumulated data chunks
    data: Vec<u8>,
    /// Expected next offset
    next_offset: u64,
    /// Last activity timestamp for timeout
    last_activity: Instant,
}

impl SnapshotAccumulator {
    fn new(vote: CodecVote, meta: CodecSnapshotMeta) -> Self {
        Self {
            vote,
            meta,
            data: Vec::new(),
            next_offset: 0,
            last_activity: Instant::now(),
        }
    }

    /// Append a chunk of data at the expected offset
    /// Returns true if the chunk was accepted, false if offset mismatch
    fn append_chunk(&mut self, offset: u64, chunk: &[u8]) -> bool {
        if offset != self.next_offset {
            tracing::warn!(
                "Snapshot chunk offset mismatch: expected {}, got {}",
                self.next_offset,
                offset
            );
            return false;
        }
        self.data.extend_from_slice(chunk);
        self.next_offset = offset + chunk.len() as u64;
        self.last_activity = Instant::now();
        true
    }

    /// Check if this accumulator has timed out
    fn is_expired(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

/// Manages in-progress snapshot transfers across all groups
struct SnapshotTransferManager {
    /// In-progress transfers keyed by (group_id, snapshot_id)
    transfers: DashMap<SnapshotTransferKey, SnapshotAccumulator>,
    /// Timeout for incomplete transfers (default 5 minutes)
    transfer_timeout: Duration,
}

impl SnapshotTransferManager {
    fn new(transfer_timeout: Duration) -> Self {
        Self {
            transfers: DashMap::new(),
            transfer_timeout,
        }
    }

    /// Get or create an accumulator for a snapshot transfer
    fn get_or_create(
        &self,
        group_id: u64,
        snapshot_id: String,
        vote: CodecVote,
        meta: CodecSnapshotMeta,
    ) -> dashmap::mapref::one::RefMut<'_, SnapshotTransferKey, SnapshotAccumulator> {
        let key = SnapshotTransferKey {
            group_id,
            snapshot_id: snapshot_id.clone(),
        };

        self.transfers
            .entry(key)
            .or_insert_with(|| SnapshotAccumulator::new(vote, meta))
    }

    /// Remove a completed or aborted transfer
    fn remove(&self, group_id: u64, snapshot_id: &str) -> Option<SnapshotAccumulator> {
        let key = SnapshotTransferKey {
            group_id,
            snapshot_id: snapshot_id.to_string(),
        };
        self.transfers.remove(&key).map(|(_, v)| v)
    }

    /// Clean up expired transfers
    fn cleanup_expired(&self) {
        let timeout = self.transfer_timeout;
        self.transfers.retain(|_, acc| !acc.is_expired(timeout));
    }
}

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
    /// Timeout for incomplete snapshot transfers.
    /// Default: 5 minutes
    pub snapshot_transfer_timeout: Duration,
}

impl Default for ManiacRpcServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:5000".parse().unwrap(),
            max_connections: 1000,
            connection_timeout: std::time::Duration::from_secs(60),
            max_concurrent_requests: 256,
            snapshot_transfer_timeout: Duration::from_secs(300), // 5 minutes
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
    /// Manages in-progress chunked snapshot transfers
    snapshot_transfers: Arc<SnapshotTransferManager>,
    _phantom: PhantomData<(C, T, S)>,
}

impl<C, T, S> ManiacRpcServer<C, T, S>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            Vote = openraft::impls::Vote<C>,
            Node = openraft::impls::BasicNode,
            Entry = openraft::impls::Entry<C>,
            SnapshotData = std::io::Cursor<Vec<u8>>,
        >,
    C::Entry: Clone,
    C::D: crate::multi::codec::ToCodec<crate::multi::codec::RawBytes>
        + crate::multi::codec::FromCodec<crate::multi::codec::RawBytes>,
    T: MultiplexedTransport<C>,
    S: MultiRaftLogStorage<C>,
{
    /// Create a new RPC server
    pub fn new(config: ManiacRpcServerConfig, manager: Arc<MultiRaftManager<C, T, S>>) -> Self {
        let snapshot_transfers = Arc::new(SnapshotTransferManager::new(config.snapshot_transfer_timeout));
        Self {
            config,
            manager,
            active_connections: AtomicU64::new(0),
            snapshot_transfers,
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
            let _ = maniac::spawn(async move {
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
                        if let crate::multi::tcp_transport::ManiacTransportError::IoError(
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
            let request: CodecRpcMessage<crate::multi::codec::RawBytes> =
                match CodecRpcMessage::decode_from_slice(&request_data) {
                    Ok(req) => req,
                    Err(e) => {
                        tracing::error!("Failed to decode request from {}: {}", peer_addr, e);
                        continue;
                    }
                };

            let request_id = request.request_id();
            let manager = self.manager.clone();
            let snapshot_transfers = self.snapshot_transfers.clone();
            let mut tx = response_tx.clone();

            // Spawn handler for this request
            let _ = maniac::spawn(async move {
                let response = Self::process_codec_request(&manager, &snapshot_transfers, request).await;

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
        snapshot_transfers: &Arc<SnapshotTransferManager>,
        request: CodecRpcMessage<crate::multi::codec::RawBytes>,
    ) -> CodecRpcMessage<crate::multi::codec::RawBytes> {
        use crate::multi::codec::{
            AppendEntriesResponse as CodecAppendEntriesResponse,
            InstallSnapshotResponse as CodecInstallSnapshotResponse, RawBytes,
            VoteResponse as CodecVoteResponse, ToCodec, FromCodec,
        };

        match request {
            CodecRpcMessage::AppendEntries {
                request_id,
                group_id,
                rpc,
            } => {
                tracing::trace!(
                    "Processing AppendEntries for group {} (req_id={}, entries={})",
                    group_id,
                    request_id,
                    rpc.entries.len()
                );

                if let Some(raft) = manager.get_group(group_id) {
                    // Convert codec types to raft types using FromCodec
                    let vote = openraft::impls::Vote::<C>::from_codec(rpc.vote);
                    let prev_log_id = rpc.prev_log_id.map(|l| openraft::LogId::<C>::from_codec(l));
                    let leader_commit = rpc.leader_commit.map(|l| openraft::LogId::<C>::from_codec(l));

                    // Convert entries using FromCodec trait
                    let entries: Vec<C::Entry> = rpc
                        .entries
                        .into_iter()
                        .map(|e| openraft::impls::Entry::<C>::from_codec(e))
                        .collect();

                    let raft_rpc = openraft::raft::AppendEntriesRequest {
                        vote,
                        prev_log_id,
                        entries,
                        leader_commit,
                    };

                    match raft.append_entries(raft_rpc).await {
                        Ok(response) => {
                            let codec_response = match response {
                                openraft::raft::AppendEntriesResponse::Success => {
                                    CodecAppendEntriesResponse::Success
                                }
                                openraft::raft::AppendEntriesResponse::PartialSuccess(lid) => {
                                    CodecAppendEntriesResponse::PartialSuccess(lid.map(|l| l.to_codec()))
                                }
                                openraft::raft::AppendEntriesResponse::Conflict => {
                                    CodecAppendEntriesResponse::Conflict
                                }
                                openraft::raft::AppendEntriesResponse::HigherVote(v) => {
                                    CodecAppendEntriesResponse::HigherVote(v.to_codec())
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
                    let vote = openraft::impls::Vote::<C>::from_codec(rpc.vote);
                    let last_log_id = rpc.last_log_id.map(|l| openraft::LogId::<C>::from_codec(l));
                    let raft_rpc = openraft::raft::VoteRequest { vote, last_log_id };

                    match raft.vote(raft_rpc).await {
                        Ok(response) => {
                            let codec_response = CodecVoteResponse {
                                vote: response.vote.to_codec(),
                                vote_granted: response.vote_granted,
                                last_log_id: response.last_log_id.map(|l| l.to_codec()),
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
                let snapshot_id = rpc.meta.snapshot_id.clone();
                tracing::trace!(
                    "Processing InstallSnapshot for group {} (req_id={}, snapshot_id={}, offset={}, done={}, data_len={})",
                    group_id,
                    request_id,
                    snapshot_id,
                    rpc.offset,
                    rpc.done,
                    rpc.data.0.len()
                );

                if let Some(raft) = manager.get_group(group_id) {
                    // Clean up any expired transfers periodically
                    snapshot_transfers.cleanup_expired();

                    if rpc.offset == 0 && rpc.done {
                        // Full snapshot in one piece - no accumulation needed
                        let vote = openraft::impls::Vote::<C>::from_codec(rpc.vote.clone());
                        let meta = openraft::storage::SnapshotMeta::<C>::from_codec(rpc.meta);
                        let snapshot = openraft::storage::Snapshot {
                            meta,
                            snapshot: std::io::Cursor::new(rpc.data.0),
                        };

                        match raft.install_full_snapshot(vote, snapshot).await {
                            Ok(response) => {
                                let codec_response = CodecInstallSnapshotResponse {
                                    vote: response.vote.to_codec(),
                                };
                                CodecRpcMessage::Response {
                                    request_id,
                                    message: CodecResponseMessage::InstallSnapshot(codec_response),
                                }
                            }
                            Err(e) => CodecRpcMessage::Error {
                                request_id,
                                error: format!("InstallSnapshot failed: {}", e),
                            },
                        }
                    } else if rpc.offset == 0 {
                        // First chunk of a multi-chunk transfer - create accumulator
                        tracing::debug!(
                            "Starting chunked snapshot transfer for group {} (snapshot_id={}, first_chunk_len={})",
                            group_id,
                            snapshot_id,
                            rpc.data.0.len()
                        );

                        let mut acc = snapshot_transfers.get_or_create(
                            group_id,
                            snapshot_id.clone(),
                            rpc.vote.clone(),
                            rpc.meta.clone(),
                        );

                        // Append the first chunk
                        if !acc.append_chunk(rpc.offset, &rpc.data.0) {
                            drop(acc);
                            snapshot_transfers.remove(group_id, &snapshot_id);
                            return CodecRpcMessage::Error {
                                request_id,
                                error: "Snapshot chunk offset mismatch".to_string(),
                            };
                        }

                        // Return success to continue receiving chunks
                        let codec_response = CodecInstallSnapshotResponse {
                            vote: rpc.vote,
                        };
                        CodecRpcMessage::Response {
                            request_id,
                            message: CodecResponseMessage::InstallSnapshot(codec_response),
                        }
                    } else {
                        // Subsequent chunk - append to existing accumulator
                        let key = SnapshotTransferKey {
                            group_id,
                            snapshot_id: snapshot_id.clone(),
                        };

                        if let Some(mut acc) = snapshot_transfers.transfers.get_mut(&key) {
                            let expected_offset = acc.next_offset;
                            if !acc.append_chunk(rpc.offset, &rpc.data.0) {
                                drop(acc);
                                snapshot_transfers.remove(group_id, &snapshot_id);
                                return CodecRpcMessage::Error {
                                    request_id,
                                    error: format!(
                                        "Snapshot chunk offset mismatch: expected {}, got {}",
                                        expected_offset, rpc.offset
                                    ),
                                };
                            }

                            if rpc.done {
                                // Final chunk - install the complete snapshot
                                let vote = openraft::impls::Vote::<C>::from_codec(acc.vote.clone());
                                let meta = openraft::storage::SnapshotMeta::<C>::from_codec(acc.meta.clone());
                                let data = std::mem::take(&mut acc.data);
                                drop(acc);

                                // Remove the accumulator since we're done
                                snapshot_transfers.remove(group_id, &snapshot_id);

                                tracing::debug!(
                                    "Completed chunked snapshot transfer for group {} (snapshot_id={}, total_len={})",
                                    group_id,
                                    snapshot_id,
                                    data.len()
                                );

                                let snapshot = openraft::storage::Snapshot {
                                    meta,
                                    snapshot: std::io::Cursor::new(data),
                                };

                                match raft.install_full_snapshot(vote, snapshot).await {
                                    Ok(response) => {
                                        let codec_response = CodecInstallSnapshotResponse {
                                            vote: response.vote.to_codec(),
                                        };
                                        CodecRpcMessage::Response {
                                            request_id,
                                            message: CodecResponseMessage::InstallSnapshot(codec_response),
                                        }
                                    }
                                    Err(e) => CodecRpcMessage::Error {
                                        request_id,
                                        error: format!("InstallSnapshot failed: {}", e),
                                    },
                                }
                            } else {
                                // More chunks to come
                                tracing::trace!(
                                    "Received snapshot chunk for group {} (snapshot_id={}, offset={}, accumulated={})",
                                    group_id,
                                    snapshot_id,
                                    rpc.offset,
                                    acc.next_offset
                                );

                                let codec_response = CodecInstallSnapshotResponse {
                                    vote: rpc.vote,
                                };
                                CodecRpcMessage::Response {
                                    request_id,
                                    message: CodecResponseMessage::InstallSnapshot(codec_response),
                                }
                            }
                        } else {
                            // No accumulator found - this is an error (received non-first chunk without first chunk)
                            tracing::warn!(
                                "Received snapshot chunk without first chunk for group {} (snapshot_id={}, offset={})",
                                group_id,
                                snapshot_id,
                                rpc.offset
                            );
                            CodecRpcMessage::Error {
                                request_id,
                                error: format!(
                                    "Snapshot transfer not found for snapshot_id={} (received offset {} without starting chunk)",
                                    snapshot_id, rpc.offset
                                ),
                            }
                        }
                    }
                } else {
                    CodecRpcMessage::Error {
                        request_id,
                        error: format!("Group {} not found", group_id),
                    }
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
                    // Heartbeats typically have no entries
                    let vote = openraft::impls::Vote::<C>::from_codec(rpc.vote);
                    let prev_log_id = rpc.prev_log_id.map(|l| openraft::LogId::<C>::from_codec(l));
                    let leader_commit = rpc.leader_commit.map(|l| openraft::LogId::<C>::from_codec(l));

                    // Convert entries (typically empty for heartbeats)
                    let entries: Vec<C::Entry> = rpc
                        .entries
                        .into_iter()
                        .map(|e| openraft::impls::Entry::<C>::from_codec(e))
                        .collect();

                    let raft_rpc = openraft::raft::AppendEntriesRequest {
                        vote,
                        prev_log_id,
                        entries,
                        leader_commit,
                    };

                    match raft.append_entries(raft_rpc).await {
                        Ok(response) => {
                            let codec_response = match response {
                                openraft::raft::AppendEntriesResponse::Success => {
                                    CodecAppendEntriesResponse::Success
                                }
                                openraft::raft::AppendEntriesResponse::PartialSuccess(lid) => {
                                    CodecAppendEntriesResponse::PartialSuccess(lid.map(|l| l.to_codec()))
                                }
                                openraft::raft::AppendEntriesResponse::Conflict => {
                                    CodecAppendEntriesResponse::Conflict
                                }
                                openraft::raft::AppendEntriesResponse::HigherVote(v) => {
                                    CodecAppendEntriesResponse::HigherVote(v.to_codec())
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

    use openraft::RaftTypeConfig;
    use openraft::raft::{
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
