//! Maniac TCP Transport Implementation for Multi-Raft
//!
//! Provides a concrete implementation of the MultiplexedTransport trait using
//! maniac's TcpStream with true connection multiplexing.
//!
//! ## True Connection Multiplexing
//!
//! This transport implements proper multiplexing where:
//! - Multiple requests can be in-flight concurrently on a single TCP connection
//! - Responses can arrive out-of-order and are correlated by request ID
//! - Each connection has a dedicated reader task that demultiplexes responses
//! - Connection pools maintain multiple connections per peer for maximum throughput
//! - Connections have a TTL to prevent degradation from long-lived connections

use crate::io::{AsyncReadRent, AsyncWriteRent};
use crate::net::{TcpConnectOpts, TcpStream};
use crate::raft::multi::codec::{Decode, Encode, ResponseMessage, RpcMessage};
use crate::raft::multi::network::MultiplexedTransport;
use crate::sync::oneshot;
use crate::time::Instant;
use dashmap::DashMap;
use maniac_raft::OptionalSend;
use maniac_raft::RaftTypeConfig;
use maniac_raft::error::{InstallSnapshotError, RPCError, RaftError};
use maniac_raft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use std::future::Future;
use std::io;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// Network error types for maniac transport
#[derive(Debug, thiserror::Error)]
pub enum ManiacTransportError {
    #[error("Connection failed: {0}")]
    ConnectionError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Invalid response")]
    InvalidResponse,

    #[error("Request timeout")]
    RequestTimeout,

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("Io error: {0}")]
    IoError(#[from] io::Error),

    #[error("Remote error: {0}")]
    RemoteError(String),

    #[error("Codec error: {0}")]
    CodecError(String),
}

impl<C: RaftTypeConfig> From<ManiacTransportError> for RPCError<C> {
    fn from(error: ManiacTransportError) -> Self {
        RPCError::Network(maniac_raft::error::NetworkError::new(&error))
    }
}

impl<C: RaftTypeConfig> From<ManiacTransportError>
    for RPCError<C, RaftError<C, InstallSnapshotError>>
{
    fn from(error: ManiacTransportError) -> Self {
        RPCError::Network(maniac_raft::error::NetworkError::new(&error))
    }
}

impl<C: RaftTypeConfig> From<ManiacTransportError> for RPCError<C, RaftError<C>> {
    fn from(error: ManiacTransportError) -> Self {
        RPCError::Network(maniac_raft::error::NetworkError::new(&error))
    }
}

/// Configuration for ManiacTcpTransport
#[derive(Debug, Clone)]
pub struct ManiacTcpTransportConfig {
    /// Connection timeout for establishing new connections
    pub connect_timeout: Duration,
    /// Request timeout for individual RPC calls
    pub request_timeout: Duration,
    /// Number of connections to maintain per peer address for multiplexing.
    /// Higher values allow more concurrent requests but use more resources.
    /// Default: 4
    pub connections_per_addr: usize,
    /// Maximum number of concurrent in-flight requests per connection.
    /// Default: 256
    pub max_concurrent_requests_per_conn: usize,
    /// Connection time-to-live. Connections older than this will be closed
    /// and replaced to prevent TCP connection degradation.
    /// Default: 5 minutes
    pub connection_ttl: Duration,
    /// TCP nodelay (disable Nagle's algorithm)
    pub tcp_nodelay: bool,
}

impl Default for ManiacTcpTransportConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            connections_per_addr: 4,
            max_concurrent_requests_per_conn: 256,
            connection_ttl: Duration::from_secs(300), // 5 minutes
            tcp_nodelay: true,
        }
    }
}

/// Frame format: length (u32) + payload
const FRAME_PREFIX_LEN: usize = 4;

/// Helper to read a frame from any AsyncReadRent stream
pub async fn read_frame<R: AsyncReadRent>(stream: &mut R) -> Result<Vec<u8>, ManiacTransportError> {
    use crate::io::AsyncReadRentExt;

    // Read length prefix
    let len_buf = vec![0u8; FRAME_PREFIX_LEN];
    let (res, len_buf) = stream.read_exact(len_buf).await;
    res.map_err(ManiacTransportError::IoError)?;

    let len = u32::from_le_bytes(len_buf[..FRAME_PREFIX_LEN].try_into().unwrap()) as usize;

    if len == 0 {
        return Ok(Vec::new());
    }

    // Read payload
    let payload = vec![0u8; len];
    let (res, payload) = stream.read_exact(payload).await;
    res.map_err(ManiacTransportError::IoError)?;

    Ok(payload)
}

/// Helper to write a frame to any AsyncWriteRent stream
pub async fn write_frame<W: AsyncWriteRent>(
    stream: &mut W,
    data: &[u8],
) -> Result<(), ManiacTransportError> {
    use crate::io::AsyncWriteRentExt;

    let len = data.len() as u32;
    let mut frame = Vec::with_capacity(4 + data.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(data);

    let (res, _) = stream.write_all(frame).await;
    res.map_err(ManiacTransportError::IoError)?;

    Ok(())
}

/// Type alias for pending request channel
type PendingResponseSender = oneshot::Sender<Result<Vec<u8>, ManiacTransportError>>;
type PendingResponseReceiver = oneshot::Receiver<Result<Vec<u8>, ManiacTransportError>>;

/// Message sent to the connection task for writing
struct WriteRequest {
    request_id: u64,
    data: Vec<u8>,
    response_tx: PendingResponseSender,
}

/// A single multiplexed connection with its own IO task
struct MultiplexedConnection {
    /// Channel to send write requests to the IO task
    write_tx: crate::sync::mpsc::bounded::MpscSender<WriteRequest>,
    /// Connection creation time for TTL tracking
    created_at: Instant,
    /// Number of in-flight requests
    in_flight: AtomicU64,
    /// Whether the connection is still alive
    alive: AtomicBool,
    /// Connection ID for logging
    conn_id: u64,
}

impl MultiplexedConnection {
    /// Create a new multiplexed connection and spawn the IO task
    fn new(stream: TcpStream, conn_id: u64) -> Arc<Self> {
        // Channel for write requests - buffer up to 256 pending writes
        let (write_tx, write_rx) = crate::sync::mpsc::bounded::channel::<WriteRequest>();

        let conn = Arc::new(Self {
            write_tx,
            created_at: Instant::now(),
            in_flight: AtomicU64::new(0),
            alive: AtomicBool::new(true),
            conn_id,
        });

        // Spawn IO task that handles both reading and writing
        let conn_clone = conn.clone();
        let _ = crate::spawn(async move {
            conn_clone.io_loop(stream, write_rx).await;
        });

        conn
    }

    /// IO loop that handles both reading responses and writing requests
    /// This runs in a single task to avoid split issues
    async fn io_loop(
        self: Arc<Self>,
        mut stream: TcpStream,
        mut write_rx: crate::sync::mpsc::bounded::MpscReceiver<WriteRequest>,
    ) {
        use crate::io::AsyncReadRentExt;

        // Map of pending requests awaiting responses
        let pending: Arc<DashMap<u64, PendingResponseSender>> = Arc::new(DashMap::new());

        loop {
            // Process any pending write requests first (non-blocking)
            while let Ok(write_req) = write_rx.try_recv() {
                // Register the pending request before writing
                pending.insert(write_req.request_id, write_req.response_tx);

                // Write the request
                if let Err(e) = write_frame(&mut stream, &write_req.data).await {
                    tracing::debug!("Connection {} write error: {}", self.conn_id, e);
                    // Remove and notify this request
                    if let Some((_, tx)) = pending.remove(&write_req.request_id) {
                        tx.send(Err(e));
                    }
                    self.alive.store(false, Ordering::Release);
                    // Notify all other pending requests
                    self.notify_all_pending_error(&pending);
                    return;
                }
            }

            // Now try to read a response (with a short timeout to allow write processing)
            let read_timeout = std::time::Duration::from_millis(10);
            match crate::time::timeout(read_timeout, read_frame(&mut stream)).await {
                Ok(Ok(data)) => {
                    // Parse request_id from response
                    if data.len() < 9 {
                        tracing::error!("Response too short: {} bytes", data.len());
                        continue;
                    }

                    let request_id = u64::from_le_bytes(data[1..9].try_into().unwrap());

                    if let Some((_, sender)) = pending.remove(&request_id) {
                        sender.send(Ok(data));
                    } else {
                        tracing::warn!("Received response for unknown request ID: {}", request_id);
                    }
                }
                Ok(Err(e)) => {
                    // Check if it's an EOF (connection closed gracefully)
                    if let ManiacTransportError::IoError(ref io_err) = e {
                        if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                            tracing::debug!("Connection {} closed by peer", self.conn_id);
                        } else {
                            tracing::debug!("Connection {} read error: {}", self.conn_id, e);
                        }
                    }
                    self.alive.store(false, Ordering::Release);
                    self.notify_all_pending_error(&pending);
                    return;
                }
                Err(_timeout) => {
                    // Timeout is fine, just continue to process writes
                    // Check if connection is still supposed to be alive
                    if !self.alive.load(Ordering::Acquire) {
                        return;
                    }
                }
            }
        }
    }

    /// Notify all pending requests of connection error
    fn notify_all_pending_error(&self, pending: &DashMap<u64, PendingResponseSender>) {
        let keys: Vec<u64> = pending.iter().map(|e| *e.key()).collect();
        for key in keys {
            if let Some((_, sender)) = pending.remove(&key) {
                sender.send(Err(ManiacTransportError::ConnectionClosed));
            }
        }
    }

    /// Check if connection is still alive and not expired
    fn is_usable(&self, ttl: Duration) -> bool {
        self.alive.load(Ordering::Acquire) && self.created_at.elapsed() < ttl
    }

    /// Get current in-flight count
    fn in_flight_count(&self) -> u64 {
        self.in_flight.load(Ordering::Relaxed)
    }

    /// Send a request and wait for the response
    async fn send_request(
        &self,
        request_id: u64,
        request_data: Vec<u8>,
        timeout: Duration,
    ) -> Result<Vec<u8>, ManiacTransportError> {
        if !self.alive.load(Ordering::Acquire) {
            return Err(ManiacTransportError::ConnectionClosed);
        }

        // Create response channel
        let (response_tx, response_rx) = oneshot::channel();

        self.in_flight.fetch_add(1, Ordering::Relaxed);

        // Send write request to the IO task
        let write_req = WriteRequest {
            request_id,
            data: request_data,
            response_tx,
        };

        if self.write_tx.clone().send(write_req).await.is_err() {
            self.in_flight.fetch_sub(1, Ordering::Relaxed);
            self.alive.store(false, Ordering::Release);
            return Err(ManiacTransportError::ConnectionClosed);
        }

        // Wait for response with timeout
        // Note: Receiver implements Future directly, returning Result<T, RecvError>
        let result = match crate::time::timeout(timeout, response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_recv_err)) => Err(ManiacTransportError::ConnectionClosed),
            Err(_timeout) => Err(ManiacTransportError::RequestTimeout),
        };

        self.in_flight.fetch_sub(1, Ordering::Relaxed);
        result
    }
}

/// Pool of multiplexed TCP connections per peer with TTL management
struct MultiplexedConnectionPool {
    /// Connections per address
    pools: DashMap<SocketAddr, Vec<Arc<MultiplexedConnection>>>,
    /// Target number of connections per peer
    connections_per_addr: usize,
    /// Max concurrent requests per connection
    max_per_conn: usize,
    /// Connection TTL
    connection_ttl: Duration,
    /// Global connection ID counter
    conn_id_counter: AtomicU64,
}

impl MultiplexedConnectionPool {
    fn new(connections_per_addr: usize, max_per_conn: usize, connection_ttl: Duration) -> Self {
        Self {
            pools: DashMap::new(),
            connections_per_addr,
            max_per_conn,
            connection_ttl,
            conn_id_counter: AtomicU64::new(0),
        }
    }

    fn next_conn_id(&self) -> u64 {
        self.conn_id_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Get or create a connection to the given address.
    /// Returns the connection with the lowest in-flight count that is still usable.
    async fn get_or_create<F, Fut>(
        &self,
        addr: SocketAddr,
        factory: F,
    ) -> Result<Arc<MultiplexedConnection>, ManiacTransportError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<TcpStream, io::Error>>,
    {
        let ttl = self.connection_ttl;

        // Clean up expired/dead connections and find best available
        let mut pool = self.pools.entry(addr).or_insert_with(Vec::new);

        // Remove dead or expired connections
        pool.retain(|conn| conn.is_usable(ttl));

        // Find connection with lowest in-flight count that has capacity
        let mut best_conn: Option<Arc<MultiplexedConnection>> = None;
        let mut best_count = u64::MAX;

        for conn in pool.iter() {
            let count = conn.in_flight_count();
            if count < best_count && count < self.max_per_conn as u64 {
                best_count = count;
                best_conn = Some(conn.clone());
            }
        }

        // If we found a good connection, use it
        if let Some(conn) = best_conn {
            return Ok(conn);
        }

        // Need to create a new connection if under limit
        if pool.len() < self.connections_per_addr {
            let stream = factory().await.map_err(|e| {
                ManiacTransportError::ConnectionError(format!(
                    "Failed to connect to {}: {}",
                    addr, e
                ))
            })?;

            let conn_id = self.next_conn_id();
            let conn = MultiplexedConnection::new(stream, conn_id);
            pool.push(conn.clone());

            tracing::debug!(
                "Created new multiplexed connection {} to {} (pool size: {})",
                conn_id,
                addr,
                pool.len()
            );

            return Ok(conn);
        }

        // All connections are at max capacity, use the one with lowest count anyway
        if let Some(conn) = pool.iter().min_by_key(|c| c.in_flight_count()) {
            return Ok(conn.clone());
        }

        // This shouldn't happen, but create a new connection as fallback
        let stream = factory().await.map_err(|e| {
            ManiacTransportError::ConnectionError(format!("Failed to connect to {}: {}", addr, e))
        })?;

        let conn_id = self.next_conn_id();
        let conn = MultiplexedConnection::new(stream, conn_id);
        pool.push(conn.clone());
        Ok(conn)
    }

    /// Remove a specific connection from the pool
    fn remove_connection(&self, addr: SocketAddr, conn_id: u64) {
        if let Some(mut pool) = self.pools.get_mut(&addr) {
            let before = pool.len();
            pool.retain(|c| c.conn_id != conn_id);
            if pool.len() < before {
                tracing::debug!(
                    "Removed connection {} to {} (pool size: {})",
                    conn_id,
                    addr,
                    pool.len()
                );
            }
        }
    }
}

/// Trait for resolving node IDs to socket addresses
pub trait NodeAddressResolver<NodeId>: Send + Sync + 'static {
    /// Resolve a node ID to a socket address
    fn resolve(&self, node_id: &NodeId) -> Option<SocketAddr>;

    /// Register a node ID with its address
    fn register(&self, node_id: NodeId, addr: SocketAddr);

    /// Unregister a node ID
    fn unregister(&self, node_id: &NodeId);
}

/// Default implementation using DashMap
pub struct DefaultNodeRegistry<NodeId: Eq + std::hash::Hash + Clone + Send + Sync + 'static> {
    nodes: DashMap<NodeId, SocketAddr>,
}

impl<NodeId: Eq + std::hash::Hash + Clone + Send + Sync + 'static> DefaultNodeRegistry<NodeId> {
    pub fn new() -> Self {
        Self {
            nodes: DashMap::new(),
        }
    }
}

impl<NodeId: Eq + std::hash::Hash + Clone + Send + Sync + 'static> Default
    for DefaultNodeRegistry<NodeId>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<NodeId: Eq + std::hash::Hash + Clone + Send + Sync + 'static> NodeAddressResolver<NodeId>
    for DefaultNodeRegistry<NodeId>
{
    fn resolve(&self, node_id: &NodeId) -> Option<SocketAddr> {
        self.nodes.get(node_id).map(|r| *r.value())
    }

    fn register(&self, node_id: NodeId, addr: SocketAddr) {
        self.nodes.insert(node_id, addr);
    }

    fn unregister(&self, node_id: &NodeId) {
        self.nodes.remove(node_id);
    }
}

/// TCP transport implementation for Multi-Raft with true connection multiplexing
pub struct ManiacTcpTransport<C: RaftTypeConfig> {
    config: ManiacTcpTransportConfig,
    connection_pool: Arc<MultiplexedConnectionPool>,
    /// Node address resolver
    node_registry: Arc<dyn NodeAddressResolver<C::NodeId>>,
    /// Global request ID counter for correlation
    request_id_counter: AtomicU64,
    _phantom: PhantomData<C>,
}

impl<C> ManiacTcpTransport<C>
where
    C: RaftTypeConfig,
    C::NodeId: Eq + std::hash::Hash + Clone,
{
    /// Create a new ManiacTcpTransport with true multiplexing support
    pub fn new(
        config: ManiacTcpTransportConfig,
        node_registry: Arc<dyn NodeAddressResolver<C::NodeId>>,
    ) -> Self {
        Self {
            connection_pool: Arc::new(MultiplexedConnectionPool::new(
                config.connections_per_addr,
                config.max_concurrent_requests_per_conn,
                config.connection_ttl,
            )),
            config,
            node_registry,
            request_id_counter: AtomicU64::new(0),
            _phantom: PhantomData,
        }
    }

    /// Create a new transport with default configuration and a new registry
    pub fn with_defaults() -> Self
    where
        C::NodeId: Eq + std::hash::Hash + Clone + Send + Sync + 'static,
    {
        Self::new(
            ManiacTcpTransportConfig::default(),
            Arc::new(DefaultNodeRegistry::new()),
        )
    }

    /// Get the node registry for registering node addresses
    pub fn node_registry(&self) -> &Arc<dyn NodeAddressResolver<C::NodeId>> {
        &self.node_registry
    }

    /// Get the next request ID
    fn next_request_id(&self) -> u64 {
        self.request_id_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Internal RPC call helper with true multiplexing
    async fn rpc_call<D: Encode + Send + 'static>(
        &self,
        target: &SocketAddr,
        request_id: u64,
        request_msg: &RpcMessage<D>,
    ) -> Result<Vec<u8>, ManiacTransportError> {
        let pool = self.connection_pool.clone();
        let addr = *target;

        // Serialize request using zero-copy codec
        let request_data = request_msg
            .encode_to_vec()
            .map_err(|e| ManiacTransportError::CodecError(e.to_string()))?;

        // Get a connection from the pool
        let conn = pool
            .get_or_create(addr, || async {
                let opts = TcpConnectOpts::default();
                TcpStream::connect_addr_with_config(addr, &opts).await
            })
            .await?;

        // Send request and wait for response
        let result = conn
            .send_request(request_id, request_data, self.config.request_timeout)
            .await;

        // If the connection failed, remove it from the pool
        if result.is_err() && !conn.alive.load(Ordering::Acquire) {
            pool.remove_connection(addr, conn.conn_id);
        }

        result
    }
}

impl<C> MultiplexedTransport<C> for ManiacTcpTransport<C>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = maniac_raft::impls::leader_id_adv::LeaderId<C>,
            Vote = maniac_raft::impls::Vote<C>,
        >,
    C::SnapshotData: AsyncReadRent + AsyncWriteRent + 'static,
    C::Entry: Clone,
{
    async fn send_append_entries(
        &self,
        target: C::NodeId,
        group_id: u64,
        rpc: AppendEntriesRequest<C>,
    ) -> Result<AppendEntriesResponse<C>, RPCError<C, RaftError<C>>> {
        use crate::raft::multi::codec::{
            AppendEntriesRequest as CodecAppendEntriesRequest,
            AppendEntriesResponse as CodecAppendEntriesResponse, BasicNode as CodecBasicNode,
            Entry as CodecEntry, EntryPayload as CodecEntryPayload, LeaderId as CodecLeaderId,
            LogId as CodecLogId, Membership as CodecMembership, Vote as CodecVote,
        };

        let addr = self.node_registry.resolve(&target).ok_or_else(|| {
            ManiacTransportError::ConnectionError(format!("Unknown node: {}", target))
        })?;
        let request_id = self.next_request_id();

        // Convert to codec types
        let codec_vote = CodecVote {
            leader_id: CodecLeaderId {
                term: rpc.vote.leader_id.term,
                node_id: rpc.vote.leader_id.node_id,
            },
            committed: rpc.vote.committed,
        };

        let codec_prev_log_id = rpc.prev_log_id.map(|lid| CodecLogId {
            leader_id: CodecLeaderId {
                term: lid.leader_id.term,
                node_id: lid.leader_id.node_id,
            },
            index: lid.index,
        });

        let codec_leader_commit = rpc.leader_commit.map(|lid| CodecLogId {
            leader_id: CodecLeaderId {
                term: lid.leader_id.term,
                node_id: lid.leader_id.node_id,
            },
            index: lid.index,
        });

        // Convert entries (simplified - assumes Vec<u8> data type for now)
        let codec_entries: Vec<CodecEntry<Vec<u8>>> = Vec::new(); // TODO: proper entry conversion

        let codec_rpc = CodecAppendEntriesRequest {
            vote: codec_vote,
            prev_log_id: codec_prev_log_id,
            entries: codec_entries,
            leader_commit: codec_leader_commit,
        };

        let request = RpcMessage::AppendEntries {
            request_id,
            group_id,
            rpc: codec_rpc,
        };

        let response_data = self
            .rpc_call(&addr, request_id, &request)
            .await
            .map_err(RPCError::<C, RaftError<C>>::from)?;

        // Deserialize response
        let response: RpcMessage<Vec<u8>> = RpcMessage::decode_from_slice(&response_data)
            .map_err(|e| ManiacTransportError::CodecError(e.to_string()))
            .map_err(RPCError::<C, RaftError<C>>::from)?;

        match response {
            RpcMessage::Response {
                message: ResponseMessage::AppendEntries(resp),
                ..
            } => {
                // Convert back from codec type
                use crate::raft::multi::codec::AppendEntriesResponse as CodecResp;
                match resp {
                    CodecResp::Success => Ok(AppendEntriesResponse::Success),
                    CodecResp::PartialSuccess(log_id) => {
                        let lid = log_id.map(|l| maniac_raft::LogId {
                            leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                                term: l.leader_id.term,
                                node_id: l.leader_id.node_id,
                            },
                            index: l.index,
                        });
                        Ok(AppendEntriesResponse::PartialSuccess(lid))
                    }
                    CodecResp::Conflict => Ok(AppendEntriesResponse::Conflict),
                    CodecResp::HigherVote(v) => {
                        let vote = maniac_raft::impls::Vote {
                            leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                                term: v.leader_id.term,
                                node_id: v.leader_id.node_id,
                            },
                            committed: v.committed,
                        };
                        Ok(AppendEntriesResponse::HigherVote(vote))
                    }
                }
            }
            RpcMessage::Error { error, .. } => Err(ManiacTransportError::RemoteError(error).into()),
            _ => Err(ManiacTransportError::InvalidResponse.into()),
        }
    }

    async fn send_vote(
        &self,
        target: C::NodeId,
        group_id: u64,
        rpc: VoteRequest<C>,
    ) -> Result<VoteResponse<C>, RPCError<C, RaftError<C>>> {
        use crate::raft::multi::codec::{
            LeaderId as CodecLeaderId, LogId as CodecLogId, Vote as CodecVote,
            VoteRequest as CodecVoteRequest, VoteResponse as CodecVoteResponse,
        };

        let addr = self.node_registry.resolve(&target).ok_or_else(|| {
            ManiacTransportError::ConnectionError(format!("Unknown node: {}", target))
        })?;
        let request_id = self.next_request_id();

        let codec_vote = CodecVote {
            leader_id: CodecLeaderId {
                term: rpc.vote.leader_id.term,
                node_id: rpc.vote.leader_id.node_id,
            },
            committed: rpc.vote.committed,
        };

        let codec_last_log_id = rpc.last_log_id.map(|lid| CodecLogId {
            leader_id: CodecLeaderId {
                term: lid.leader_id.term,
                node_id: lid.leader_id.node_id,
            },
            index: lid.index,
        });

        let codec_rpc = CodecVoteRequest {
            vote: codec_vote,
            last_log_id: codec_last_log_id,
        };

        let request: RpcMessage<Vec<u8>> = RpcMessage::Vote {
            request_id,
            group_id,
            rpc: codec_rpc,
        };

        let response_data = self
            .rpc_call(&addr, request_id, &request)
            .await
            .map_err(RPCError::<C, RaftError<C>>::from)?;

        // Deserialize response
        let response: RpcMessage<Vec<u8>> = RpcMessage::decode_from_slice(&response_data)
            .map_err(|e| ManiacTransportError::CodecError(e.to_string()))
            .map_err(RPCError::<C, RaftError<C>>::from)?;

        match response {
            RpcMessage::Response {
                message: ResponseMessage::Vote(resp),
                ..
            } => {
                let vote = maniac_raft::impls::Vote {
                    leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                        term: resp.vote.leader_id.term,
                        node_id: resp.vote.leader_id.node_id,
                    },
                    committed: resp.vote.committed,
                };

                let last_log_id = resp.last_log_id.map(|l| maniac_raft::LogId {
                    leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                        term: l.leader_id.term,
                        node_id: l.leader_id.node_id,
                    },
                    index: l.index,
                });

                Ok(VoteResponse {
                    vote,
                    vote_granted: resp.vote_granted,
                    last_log_id,
                })
            }
            RpcMessage::Error { error, .. } => Err(ManiacTransportError::RemoteError(error).into()),
            _ => Err(ManiacTransportError::InvalidResponse.into()),
        }
    }

    async fn send_install_snapshot(
        &self,
        target: C::NodeId,
        group_id: u64,
        rpc: InstallSnapshotRequest<C>,
    ) -> Result<InstallSnapshotResponse<C>, RPCError<C, RaftError<C, InstallSnapshotError>>> {
        use crate::raft::multi::codec::{
            InstallSnapshotRequest as CodecInstallSnapshotRequest,
            InstallSnapshotResponse as CodecInstallSnapshotResponse, LeaderId as CodecLeaderId,
            LogId as CodecLogId, Membership as CodecMembership, RawBytes,
            SnapshotMeta as CodecSnapshotMeta, StoredMembership as CodecStoredMembership,
            Vote as CodecVote,
        };

        let addr = self.node_registry.resolve(&target).ok_or_else(|| {
            ManiacTransportError::ConnectionError(format!("Unknown node: {}", target))
        })?;
        let request_id = self.next_request_id();

        let codec_vote = CodecVote {
            leader_id: CodecLeaderId {
                term: rpc.vote.leader_id.term,
                node_id: rpc.vote.leader_id.node_id,
            },
            committed: rpc.vote.committed,
        };

        let codec_meta = CodecSnapshotMeta {
            last_log_id: rpc.meta.last_log_id.map(|lid| CodecLogId {
                leader_id: CodecLeaderId {
                    term: lid.leader_id.term,
                    node_id: lid.leader_id.node_id,
                },
                index: lid.index,
            }),
            last_membership: CodecStoredMembership {
                log_id: rpc.meta.last_membership.log_id().map(|lid| CodecLogId {
                    leader_id: CodecLeaderId {
                        term: lid.leader_id.term,
                        node_id: lid.leader_id.node_id,
                    },
                    index: lid.index,
                }),
                membership: CodecMembership::default(),
            },
            snapshot_id: rpc.meta.snapshot_id.clone(),
        };

        let codec_rpc = CodecInstallSnapshotRequest {
            vote: codec_vote,
            meta: codec_meta,
            offset: rpc.offset,
            data: RawBytes(Vec::new()), // TODO: handle actual snapshot data
            done: rpc.done,
        };

        let request: RpcMessage<Vec<u8>> = RpcMessage::InstallSnapshot {
            request_id,
            group_id,
            rpc: codec_rpc,
        };

        let response_data = self
            .rpc_call(&addr, request_id, &request)
            .await
            .map_err(RPCError::<C, RaftError<C, InstallSnapshotError>>::from)?;

        // Deserialize response
        let response: RpcMessage<Vec<u8>> = RpcMessage::decode_from_slice(&response_data)
            .map_err(|e| ManiacTransportError::CodecError(e.to_string()))
            .map_err(RPCError::<C, RaftError<C, InstallSnapshotError>>::from)?;

        match response {
            RpcMessage::Response {
                message: ResponseMessage::InstallSnapshot(resp),
                ..
            } => {
                let vote = maniac_raft::impls::Vote {
                    leader_id: maniac_raft::impls::leader_id_adv::LeaderId {
                        term: resp.vote.leader_id.term,
                        node_id: resp.vote.leader_id.node_id,
                    },
                    committed: resp.vote.committed,
                };

                Ok(InstallSnapshotResponse { vote })
            }
            RpcMessage::Error { error, .. } => Err(ManiacTransportError::RemoteError(error).into()),
            _ => Err(ManiacTransportError::InvalidResponse.into()),
        }
    }

    async fn send_heartbeat_batch(
        &self,
        target: C::NodeId,
        batch: &[(u64, AppendEntriesRequest<C>)],
    ) -> Result<Vec<(u64, AppendEntriesResponse<C>)>, RPCError<C, RaftError<C>>> {
        // For heartbeat batches, we can send them concurrently using the multiplexed connection
        let mut results = Vec::with_capacity(batch.len());

        for (gid, rpc) in batch {
            match self
                .send_append_entries(target.clone(), *gid, rpc.clone())
                .await
            {
                Ok(resp) => results.push((*gid, resp)),
                Err(e) => return Err(e),
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_config_default() {
        let config = ManiacTcpTransportConfig::default();
        assert_eq!(config.connections_per_addr, 4);
        assert_eq!(config.max_concurrent_requests_per_conn, 256);
        assert_eq!(config.connection_ttl, Duration::from_secs(300));
    }
}
