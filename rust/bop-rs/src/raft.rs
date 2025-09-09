//! Idiomatic Rust wrappers for the BOP Raft consensus implementation
//! 
//! This module provides safe, ergonomic Rust bindings for the NuRaft C++ library
//! wrapped by BOP. The API is designed to be idiomatic Rust with proper error
//! handling, memory safety, and async integration.

use bop_sys::*;
use std::ffi::{CStr, CString};
use std::ptr::{self, null_mut, NonNull};

/// Strong type for server IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerId(pub i32);

/// Strong type for data center IDs  
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DataCenterId(pub i32);

/// Strong type for Raft terms
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Term(pub u64);

/// Strong type for log indices
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogIndex(pub u64);

/// Strong type for log entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogEntryType(pub i32);

/// Raft-specific errors
#[derive(Debug, thiserror::Error)]
pub enum RaftError {
    #[error("Null pointer returned from C API")]
    NullPointer,
    
    #[error("Invalid UTF-8 string: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    
    #[error("Invalid C string: {0}")]
    InvalidCString(#[from] std::ffi::NulError),
    
    #[error("Server operation failed")]
    ServerError,
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("State machine error: {0}")]
    StateMachineError(String),
    
    #[error("Log store error: {0}")]
    LogStoreError(String),
}

pub type RaftResult<T> = Result<T, RaftError>;

/// Callback types for Raft events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CallbackType {
    ProcessRequest = 1,
    GotAppendEntryRespFromPeer = 2,
    AppendLogs = 3,
    HeartBeat = 4,
    JoinedCluster = 5,
    BecomeLeader = 6,
    RequestAppendEntries = 7,
    SaveSnapshot = 8,
    NewConfig = 9,
    RemovedFromCluster = 10,
    BecomeFollower = 11,
    BecomeFresh = 12,
}

/// Priority set operation results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PrioritySetResult {
    Set = 1,
    Broadcast = 2,
    Ignored = 3,
}

/// RAII wrapper for bop_raft_buffer with automatic cleanup
pub struct Buffer {
    ptr: NonNull<bop_raft_buffer>,
}

impl Buffer {
    /// Create a new buffer with the specified size
    pub fn new(size: usize) -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_buffer_new(size) };
        NonNull::new(ptr)
            .map(|ptr| Buffer { ptr })
            .ok_or(RaftError::NullPointer)
    }
    
    /// Get the raw data pointer
    pub fn data(&mut self) -> *mut u8 {
        unsafe { bop_raft_buffer_data(self.ptr.as_ptr()) }
    }
    
    /// Get the container size (allocated capacity)
    pub fn container_size(&self) -> usize {
        unsafe { bop_raft_buffer_container_size(self.ptr.as_ptr()) }
    }
    
    /// Get the current size (amount of data)
    pub fn size(&self) -> usize {
        unsafe { bop_raft_buffer_size(self.ptr.as_ptr()) }
    }
    
    /// Get the current position
    pub fn pos(&self) -> usize {
        unsafe { bop_raft_buffer_pos(self.ptr.as_ptr()) }
    }
    
    /// Set the current position
    pub fn set_pos(&mut self, pos: usize) {
        unsafe { bop_raft_buffer_set_pos(self.ptr.as_ptr(), pos) }
    }
    
    /// Copy data to a Vec<u8>
    pub fn to_vec(&self) -> Vec<u8> {
        let size = self.size();
        let mut vec = Vec::with_capacity(size);
        unsafe {
            let data = bop_raft_buffer_data(self.ptr.as_ptr());
            ptr::copy_nonoverlapping(data, vec.as_mut_ptr(), size);
            vec.set_len(size);
        }
        vec
    }
    
    /// Get the raw pointer (for C API interop)
    pub fn as_ptr(&mut self) -> *mut bop_raft_buffer {
        self.ptr.as_ptr()
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            bop_raft_buffer_free(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

/// Server configuration for a Raft node
pub struct ServerConfig {
    ptr: NonNull<bop_raft_srv_config_ptr>,
}

impl ServerConfig {
    /// Create a new server configuration
    pub fn new(
        id: ServerId,
        dc_id: DataCenterId,
        endpoint: &str,
        aux: &str,
        is_learner: bool,
    ) -> RaftResult<Self> {
        let endpoint_cstr = CString::new(endpoint)?;
        let aux_cstr = CString::new(aux)?;
        
        let ptr = unsafe {
            bop_raft_srv_config_make(
                id.0,
                dc_id.0,
                endpoint_cstr.as_ptr(),
                endpoint.len(),
                aux_cstr.as_ptr(),
                aux.len(),
                is_learner,
                0, // default priority
            )
        };
        
        NonNull::new(ptr)
            .map(|ptr| ServerConfig { ptr })
            .ok_or(RaftError::NullPointer)
    }
    
    /// Get the server ID
    pub fn id(&self) -> ServerId {
        ServerId(unsafe { bop_raft_srv_config_id(self.ptr.as_ptr()) })
    }
    
    /// Get the data center ID
    pub fn dc_id(&self) -> DataCenterId {
        DataCenterId(unsafe { bop_raft_srv_config_dc_id(self.ptr.as_ptr()) })
    }
    
    /// Get the endpoint string
    pub fn endpoint(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_srv_config_endpoint(self.ptr.as_ptr());
            let size = bop_raft_srv_config_endpoint_size(self.ptr.as_ptr());
            let slice = std::slice::from_raw_parts(ptr as *const u8, size);
            Ok(String::from_utf8_lossy(slice).into_owned())
        }
    }
    
    /// Get the auxiliary data string
    pub fn aux(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_srv_config_aux(self.ptr.as_ptr());
            let size = bop_raft_srv_config_aux_size(self.ptr.as_ptr());
            let slice = std::slice::from_raw_parts(ptr as *const u8, size);
            Ok(String::from_utf8_lossy(slice).into_owned())
        }
    }
    
    /// Check if this is a learner node
    pub fn is_learner(&self) -> bool {
        unsafe { bop_raft_srv_config_is_learner(self.ptr.as_ptr()) }
    }
    
    /// Set learner status
    pub fn set_learner(&mut self, learner: bool) {
        unsafe { bop_raft_srv_config_set_is_learner(self.ptr.as_ptr(), learner) }
    }
    
    /// Check if this is a new joiner
    pub fn is_new_joiner(&self) -> bool {
        unsafe { bop_raft_srv_config_is_new_joiner(self.ptr.as_ptr()) }
    }
    
    /// Set new joiner status
    pub fn set_new_joiner(&mut self, new_joiner: bool) {
        unsafe { bop_raft_srv_config_set_new_joiner(self.ptr.as_ptr(), new_joiner) }
    }
    
    /// Get the priority
    pub fn priority(&self) -> i32 {
        unsafe { bop_raft_srv_config_priority(self.ptr.as_ptr()) }
    }
    
    /// Set the priority
    pub fn set_priority(&mut self, priority: i32) {
        unsafe { bop_raft_srv_config_set_priority(self.ptr.as_ptr(), priority) }
    }
    
    /// Get the raw pointer (for C API interop)
    pub fn as_ptr(&self) -> *mut bop_raft_srv_config {
        self.ptr.as_ptr()
    }
}

impl Drop for ServerConfig {
    fn drop(&mut self) {
        unsafe {
            bop_raft_srv_config_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for ServerConfig {}
unsafe impl Sync for ServerConfig {}

/// Cluster configuration containing multiple server configurations
pub struct ClusterConfig {
    ptr: NonNull<bop_raft_cluster_config>,
}

impl ClusterConfig {
    /// Create a new cluster configuration
    pub fn new() -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_cluster_config_new() };
        NonNull::new(ptr)
            .map(|ptr| ClusterConfig { ptr })
            .ok_or(RaftError::NullPointer)
    }
    
    /// Get the log index for this configuration
    pub fn log_idx(&self) -> LogIndex {
        LogIndex(unsafe { bop_raft_cluster_config_log_idx(self.ptr.as_ptr()) })
    }
    
    /// Get the previous log index
    pub fn prev_log_idx(&self) -> LogIndex {
        LogIndex(unsafe { bop_raft_cluster_config_prev_log_idx(self.ptr.as_ptr()) })
    }
    
    /// Check if async replication is enabled
    pub fn is_async_replication(&self) -> bool {
        unsafe { bop_raft_cluster_config_is_async_replication(self.ptr.as_ptr()) }
    }
    
    /// Get the number of servers in the configuration
    pub fn servers_size(&self) -> usize {
        unsafe { bop_raft_cluster_config_servers_size(self.ptr.as_ptr()) }
    }
    
    /// Get user context data
    pub fn user_ctx(&self) -> RaftResult<Vec<u8>> {
        unsafe {
            let size = bop_raft_cluster_config_user_ctx_size(self.ptr.as_ptr());
            if size == 0 {
                return Ok(Vec::new());
            }
            
            let mut data = vec![0u8; size];
            bop_raft_cluster_config_user_ctx(
                self.ptr.as_ptr(),
                data.as_mut_ptr() as *mut i8,
                size,
            );
            Ok(data)
        }
    }
    
    /// Serialize the configuration to a buffer
    pub fn serialize(&self) -> RaftResult<Buffer> {
        let ptr = unsafe { bop_raft_cluster_config_serialize(self.ptr.as_ptr()) };
        NonNull::new(ptr)
            .map(|ptr| Buffer { ptr })
            .ok_or(RaftError::NullPointer)
    }
    
    /// Get the raw pointer (for C API interop)
    pub fn as_ptr(&self) -> *mut bop_raft_cluster_config {
        self.ptr.as_ptr()
    }
}

impl Drop for ClusterConfig {
    fn drop(&mut self) {
        unsafe {
            bop_raft_cluster_config_free(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for ClusterConfig {}
unsafe impl Sync for ClusterConfig {}

/// Raft parameters configuration
pub struct RaftParams {
    ptr: NonNull<bop_raft_params>,
}

impl RaftParams {
    /// Create new Raft parameters with defaults
    pub fn new() -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_params_make() };
        NonNull::new(ptr)
            .map(|ptr| RaftParams { ptr })
            .ok_or(RaftError::NullPointer)
    }
    
    /// Get the raw pointer (for C API interop)
    pub fn as_ptr(&self) -> *mut bop_raft_params {
        self.ptr.as_ptr()
    }
}

impl Default for RaftParams {
    fn default() -> Self {
        Self::new().expect("Failed to create default RaftParams")
    }
}

impl Drop for RaftParams {
    fn drop(&mut self) {
        unsafe {
            bop_raft_params_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for RaftParams {}
unsafe impl Sync for RaftParams {}

/// Log entry data
pub struct LogEntry {
    ptr: NonNull<bop_raft_log_entry>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(
        term: Term,
        buffer: Buffer,
        timestamp: u64,
    ) -> RaftResult<Self> {
        let mut buffer = buffer; // Make mutable to get pointer
        let ptr = unsafe {
            bop_raft_log_entry_make(
                term.0,
                buffer.as_ptr(),
                timestamp,
                false, // has_crc32
                0,     // crc32
            )
        };
        std::mem::forget(buffer); // Buffer ownership transferred to C++
        
        NonNull::new(ptr)
            .map(|ptr| LogEntry { ptr })
            .ok_or(RaftError::NullPointer)
    }
    
    /// Get the raw pointer (for C API interop)
    pub fn as_ptr(&self) -> *const bop_raft_log_entry {
        self.ptr.as_ptr()
    }
}

impl Drop for LogEntry {
    fn drop(&mut self) {
        unsafe {
            bop_raft_log_entry_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for LogEntry {}
unsafe impl Sync for LogEntry {}

// Server state information
pub struct ServerState {
    ptr: NonNull<bop_raft_srv_state>,
}

impl ServerState {
    /// Get current term
    pub fn term(&self) -> Term {
        Term(unsafe { bop_raft_srv_state_term(self.ptr.as_ptr()) })
    }
    
    /// Get who this server voted for
    pub fn voted_for(&self) -> ServerId {
        ServerId(unsafe { bop_raft_srv_state_voted_for(self.ptr.as_ptr()) })
    }
    
    /// Check if election timer is allowed
    pub fn is_election_timer_allowed(&self) -> bool {
        unsafe { bop_raft_srv_state_is_election_timer_allowed(self.ptr.as_ptr()) }
    }
    
    /// Check if server is catching up
    pub fn is_catching_up(&self) -> bool {
        unsafe { bop_raft_srv_state_is_catching_up(self.ptr.as_ptr()) }
    }
    
    /// Check if server is receiving a snapshot
    pub fn is_receiving_snapshot(&self) -> bool {
        unsafe { bop_raft_srv_state_is_receiving_snapshot(self.ptr.as_ptr()) }
    }
    
    /// Serialize the state
    pub fn serialize(&self) -> RaftResult<Buffer> {
        let ptr = unsafe { bop_raft_srv_state_serialize(self.ptr.as_ptr()) };
        NonNull::new(ptr)
            .map(|ptr| Buffer { ptr })
            .ok_or(RaftError::NullPointer)
    }
}

impl Drop for ServerState {
    fn drop(&mut self) {
        unsafe {
            bop_raft_srv_state_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for ServerState {}
unsafe impl Sync for ServerState {}

/// Peer information for other servers in the cluster
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub server_id: ServerId,
    pub last_log_idx: LogIndex,
    pub last_succ_resp_us: u64,
}

/// Main Raft server implementation
pub struct RaftServer {
    ptr: NonNull<bop_raft_server_ptr>,
}

impl RaftServer {
    /// Launch a new Raft server
    // pub fn launch(
    //     asio_service: AsioService,
    //     params: RaftParams,
    //     state_mgr: StateManager,
    //     log_store: LogStore,
    //     raft_params: RaftParams,
    //     listener: RpcListener,
    // ) -> RaftResult<Self> {
    //     let ptr = unsafe {
    //         bop_raft_server_launch(
    //             asio_service.as_ptr(),
    //             params.as_ptr(),
    //             state_mgr.as_ptr(),
    //             log_store.as_ptr(),
    //             raft_params.as_ptr(),
    //             listener.as_ptr(),
    //             raft_params.as_ptr(),
    //             true,
    //             false,
    //             false,
    //             None,
    //         )
    //     };
        
    //     NonNull::new(ptr)
    //         .map(|ptr| RaftServer { ptr })
    //         .ok_or(RaftError::NullPointer)
    // }
    
    /// Stop the server with a timeout
    pub fn stop(&mut self, time_limit_sec: usize) -> bool {
        unsafe { bop_raft_server_stop(self.ptr.as_ptr(), time_limit_sec) }
    }
    
    /// Check if server is initialized
    pub fn is_initialized(&self) -> bool {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_is_initialized(server_ptr)
        }
    }
    
    /// Check if server is catching up
    pub fn is_catching_up(&self) -> bool {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_is_catching_up(server_ptr)
        }
    }
    
    /// Check if server is receiving a snapshot
    pub fn is_receiving_snapshot(&self) -> bool {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_is_receiving_snapshot(server_ptr)
        }
    }
    
    /// Add a server to the cluster
    pub fn add_srv(&mut self, srv_config: ServerConfig) -> RaftResult<()> {
        let result = unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_add_srv(server_ptr, srv_config.as_ptr(), null_mut())
        };
        if result {
            Ok(())
        } else {
            Err(RaftError::ServerError)
        }
    }
    
    /// Remove a server from the cluster
    pub fn remove_srv(&mut self, server_id: ServerId) -> RaftResult<()> {
        let result = unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_remove_srv(server_ptr, server_id.0)
        };
        if result {
            Ok(())
        } else {
            Err(RaftError::ServerError)
        }
    }
    
    /// Flip the learner flag for a server
    pub fn flip_learner_flag(&mut self, server_id: ServerId) -> RaftResult<()> {
        let result = unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_flip_learner_flag(server_ptr, server_id.0)
        };
        if result {
            Ok(())
        } else {
            Err(RaftError::ServerError)
        }
    }
    
    /// Append entries to the log
    pub fn append_entries(&mut self, entries: Vec<LogEntry>) -> RaftResult<()> {
        // Create append_entries container
        let append_entries_ptr = unsafe { bop_raft_append_entries_create() };
        if append_entries_ptr.is_null() {
            return Err(RaftError::NullPointer);
        }
        
        // Add all entries
        for entry in entries {
            unsafe {
                bop_raft_append_entries_push(append_entries_ptr, entry.as_ptr());
                std::mem::forget(entry); // Ownership transferred
            }
        }
        
        let result = unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_append_entries(server_ptr, append_entries_ptr)
        };
        
        // Clean up the container
        unsafe {
            bop_raft_append_entries_delete(append_entries_ptr);
        }
        
        if result {
            Ok(())
        } else {
            Err(RaftError::ServerError)
        }
    }
    
    /// Set priority for a server
    pub fn set_priority(&mut self, server_id: ServerId, priority: i32) -> PrioritySetResult {
        let result = unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_set_priority(server_ptr, server_id.0, priority)
        };
        match result {
            1 => PrioritySetResult::Set,
            2 => PrioritySetResult::Broadcast,
            _ => PrioritySetResult::Ignored,
        }
    }
    
    /// Yield leadership
    pub fn yield_leadership(&mut self, immediate: bool) -> RaftResult<()> {
        let result = unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_yield_leadership(server_ptr, immediate)
        };
        if result {
            Ok(())
        } else {
            Err(RaftError::ServerError)
        }
    }
    
    /// Request leadership
    pub fn request_leadership(&mut self) -> bool {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_request_leadership(server_ptr)
        }
    }
    
    /// Get the server ID
    pub fn get_id(&self) -> ServerId {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            ServerId(bop_raft_server_get_id(server_ptr))
        }
    }
    
    /// Get the current term
    pub fn get_term(&self) -> Term {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            Term(bop_raft_server_get_term(server_ptr))
        }
    }
    
    /// Get the last log index
    pub fn get_last_log_idx(&self) -> LogIndex {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            LogIndex(bop_raft_server_get_last_log_idx(server_ptr))
        }
    }
    
    /// Get the committed log index
    pub fn get_committed_log_idx(&self) -> LogIndex {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            LogIndex(bop_raft_server_get_committed_log_idx(server_ptr))
        }
    }
    
    /// Get the leader ID
    pub fn get_leader(&self) -> ServerId {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            ServerId(bop_raft_server_get_leader(server_ptr))
        }
    }
    
    /// Check if this server is the leader
    pub fn is_leader(&self) -> bool {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_is_leader(server_ptr)
        }
    }
    
    /// Check if the leader is alive
    pub fn is_leader_alive(&self) -> bool {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_is_leader_alive(server_ptr)
        }
    }
    
    /// Get the current cluster configuration
    pub fn get_config(&self) -> RaftResult<ClusterConfig> {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            let config_ptr = bop_raft_server_get_config(server_ptr);
            NonNull::new(config_ptr)
                .map(|ptr| ClusterConfig { ptr })
                .ok_or(RaftError::NullPointer)
        }
    }
    
    /// Get peer information for all servers
    /// Note: This is a complex API that requires proper vector handling
    pub fn get_peer_info_all(&self) -> Vec<PeerInfo> {
        // TODO: Implement this properly with the correct API
        // The C API requires creating a vector first and passing it as output parameter
        Vec::new()
    }
    
    /// Shutdown the server
    pub fn shutdown(&mut self) {
        unsafe {
            let server_ptr = bop_raft_server_get(self.ptr.as_ptr());
            bop_raft_server_shutdown(server_ptr);
        }
    }
}

impl Drop for RaftServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

unsafe impl Send for RaftServer {}
unsafe impl Sync for RaftServer {}

/// Placeholder for AsioService - will need proper implementation
pub struct AsioService {
    ptr: NonNull<bop_raft_asio_service_ptr>,
}

impl AsioService {
    pub fn as_ptr(&self) -> *mut bop_raft_asio_service_ptr {
        self.ptr.as_ptr()
    }
}

/// Placeholder for StateManager - will need proper implementation
pub struct StateManager {
    ptr: NonNull<bop_raft_state_mgr_ptr>,
}

impl StateManager {
    pub fn as_ptr(&self) -> *mut bop_raft_state_mgr_ptr {
        self.ptr.as_ptr()
    }
}

/// Placeholder for LogStore - will need proper implementation
pub struct LogStore {
    ptr: NonNull<bop_raft_log_store_ptr>,
}

impl LogStore {
    pub fn as_ptr(&self) -> *mut bop_raft_log_store_ptr {
        self.ptr.as_ptr()
    }
}

/// Placeholder for RpcListener - will need proper implementation
pub struct RpcListener {
    ptr: NonNull<bop_raft_rpc_listener_ptr>,
}

impl RpcListener {
    pub fn as_ptr(&self) -> *mut bop_raft_rpc_listener_ptr {
        self.ptr.as_ptr()
    }
}

/// State Machine Interface for Raft
/// This trait defines the interface that applications must implement
/// to integrate with the Raft consensus algorithm.
pub trait StateMachine {
    /// Apply a log entry to the state machine
    fn apply(&mut self, log_idx: LogIndex, data: &[u8]) -> RaftResult<Vec<u8>>;
    
    /// Create a snapshot of the current state
    fn take_snapshot(&self) -> RaftResult<Buffer>;
    
    /// Restore state from a snapshot
    fn restore_from_snapshot(&mut self, snapshot: &[u8]) -> RaftResult<()>;
    
    /// Get the last applied log index
    fn last_applied_index(&self) -> LogIndex;
}

/// Log Store Interface for Raft
/// This trait defines the interface for persistent log storage
pub trait LogStoreInterface {
    /// Get the start index of the log
    fn start_index(&self) -> LogIndex;
    
    /// Get the next index to append
    fn next_index(&self) -> LogIndex;
    
    /// Get a range of log entries
    fn get_log_entries(&self, start: LogIndex, end: LogIndex) -> RaftResult<Vec<LogEntry>>;
    
    /// Get a specific log entry
    fn get_log_entry(&self, idx: LogIndex) -> RaftResult<Option<LogEntry>>;
    
    /// Append log entries
    fn append_entries(&mut self, entries: Vec<LogEntry>) -> RaftResult<()>;
    
    /// Truncate log entries from index (inclusive)
    fn truncate(&mut self, from_index: LogIndex) -> RaftResult<()>;
    
    /// Get the term for a specific log index
    fn term_at(&self, idx: LogIndex) -> RaftResult<Option<Term>>;
}

/// State Manager Interface for Raft
/// This trait defines the interface for managing persistent server state
pub trait StateManagerInterface {
    /// Load the server state
    fn load_state(&self) -> RaftResult<Option<ServerState>>;
    
    /// Save the server state
    fn save_state(&mut self, state: &ServerState) -> RaftResult<()>;
    
    /// Load the cluster configuration
    fn load_config(&self) -> RaftResult<Option<ClusterConfig>>;
    
    /// Save the cluster configuration
    fn save_config(&mut self, config: &ClusterConfig) -> RaftResult<()>;
}

/// Builder for creating Raft server instances
pub struct RaftServerBuilder {
    asio_service: Option<AsioService>,
    params: Option<RaftParams>,
    state_mgr: Option<StateManager>,
    log_store: Option<LogStore>,
    listener: Option<RpcListener>,
}

impl RaftServerBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            asio_service: None,
            params: None,
            state_mgr: None,
            log_store: None,
            listener: None,
        }
    }
    
    /// Set the ASIO service
    pub fn asio_service(mut self, service: AsioService) -> Self {
        self.asio_service = Some(service);
        self
    }
    
    /// Set the Raft parameters
    pub fn params(mut self, params: RaftParams) -> Self {
        self.params = Some(params);
        self
    }
    
    /// Set the state manager
    pub fn state_manager(mut self, state_mgr: StateManager) -> Self {
        self.state_mgr = Some(state_mgr);
        self
    }
    
    /// Set the log store
    pub fn log_store(mut self, log_store: LogStore) -> Self {
        self.log_store = Some(log_store);
        self
    }
    
    /// Set the RPC listener
    pub fn listener(mut self, listener: RpcListener) -> Self {
        self.listener = Some(listener);
        self
    }
    
    /// Build the Raft server
    pub fn build(self) -> RaftResult<RaftServer> {
        let asio_service = self.asio_service.ok_or_else(|| {
            RaftError::ConfigError("ASIO service is required".to_string())
        })?;
        
        let params = self.params.ok_or_else(|| {
            RaftError::ConfigError("Raft parameters are required".to_string())
        })?;
        
        let state_mgr = self.state_mgr.ok_or_else(|| {
            RaftError::ConfigError("State manager is required".to_string())
        })?;
        
        let log_store = self.log_store.ok_or_else(|| {
            RaftError::ConfigError("Log store is required".to_string())
        })?;
        
        let listener = self.listener.ok_or_else(|| {
            RaftError::ConfigError("RPC listener is required".to_string())
        })?;
        
        // Create a second params instance for the second parameter
        let raft_params = RaftParams::new()?;
        
        RaftServer::launch(asio_service, params, state_mgr, log_store, raft_params, listener)
    }
}

impl Default for RaftServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot data container
pub struct Snapshot {
    ptr: NonNull<bop_raft_snapshot>,
}

impl Snapshot {
    /// Serialize snapshot to buffer
    pub fn serialize(&self) -> RaftResult<Buffer> {
        let ptr = unsafe { bop_raft_snapshot_serialize(self.ptr.as_ptr()) };
        NonNull::new(ptr)
            .map(|ptr| Buffer { ptr })
            .ok_or(RaftError::NullPointer)
    }
    
    /// Deserialize snapshot from buffer
    pub fn deserialize(buffer: Buffer) -> RaftResult<Self> {
        let mut buffer = buffer;
        let ptr = unsafe { bop_raft_snapshot_deserialize(buffer.as_ptr()) };
        std::mem::forget(buffer); // Buffer ownership transferred
        
        NonNull::new(ptr)
            .map(|ptr| Snapshot { ptr })
            .ok_or(RaftError::NullPointer)
    }
    
    /// Get the raw pointer (for C API interop)
    pub fn as_ptr(&self) -> *mut bop_raft_snapshot {
        self.ptr.as_ptr()
    }
}

unsafe impl Send for Snapshot {}
unsafe impl Sync for Snapshot {}

/// Metrics and statistics
pub struct Counter {
    ptr: NonNull<bop_raft_counter>,
}

impl Counter {
    /// Get counter name
    pub fn name(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_counter_name(self.ptr.as_ptr());
            if ptr.is_null() {
                return Ok(String::new());
            }
            let cstr = CStr::from_ptr(ptr);
            Ok(cstr.to_str()?.to_owned())
        }
    }
    
    /// Get counter value
    pub fn value(&self) -> u64 {
        unsafe { bop_raft_counter_value(self.ptr.as_ptr()) }
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        unsafe {
            bop_raft_counter_delete(self.ptr.as_ptr());
        }
    }
}

pub struct Gauge {
    ptr: NonNull<bop_raft_gauge>,
}

impl Gauge {
    /// Get gauge name
    pub fn name(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_gauge_name(self.ptr.as_ptr());
            if ptr.is_null() {
                return Ok(String::new());
            }
            let cstr = CStr::from_ptr(ptr);
            Ok(cstr.to_str()?.to_owned())
        }
    }
    
    /// Get gauge value
    pub fn value(&self) -> i64 {
        unsafe { bop_raft_gauge_value(self.ptr.as_ptr()) }
    }
}

impl Drop for Gauge {
    fn drop(&mut self) {
        unsafe {
            bop_raft_gauge_delete(self.ptr.as_ptr());
        }
    }
}

/// Async result types for non-blocking operations
pub struct AsyncBool {
    ptr: NonNull<bop_raft_async_bool_ptr>,
}

impl AsyncBool {
    /// Create a new async bool
    pub fn new(
        user_data: *mut std::ffi::c_void,
        when_ready: Option<unsafe extern "C" fn(*mut std::ffi::c_void, bool, *const i8)>,
    ) -> RaftResult<Self> {
        let ptr = unsafe {
            bop_raft_async_bool_make(user_data, when_ready)
        };
        
        NonNull::new(ptr)
            .map(|ptr| AsyncBool { ptr })
            .ok_or(RaftError::NullPointer)
    }
    
    /// Get user data
    pub fn get_user_data(&self) -> *mut std::ffi::c_void {
        unsafe { bop_raft_async_bool_get_user_data(self.ptr.as_ptr()) }
    }
    
    /// Set user data
    pub fn set_user_data(&mut self, data: *mut std::ffi::c_void) {
        unsafe { bop_raft_async_bool_set_user_data(self.ptr.as_ptr(), data) }
    }
}

impl Drop for AsyncBool {
    fn drop(&mut self) {
        unsafe {
            bop_raft_async_bool_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for AsyncBool {}
unsafe impl Sync for AsyncBool {}

/// Convenience functions and utilities
impl ServerId {
    /// Create a new server ID
    pub fn new(id: i32) -> Self {
        ServerId(id)
    }
    
    /// Get the inner value
    pub fn inner(self) -> i32 {
        self.0
    }
}

impl DataCenterId {
    /// Create a new data center ID
    pub fn new(id: i32) -> Self {
        DataCenterId(id)
    }
    
    /// Get the inner value
    pub fn inner(self) -> i32 {
        self.0
    }
}

impl Term {
    /// Create a new term
    pub fn new(term: u64) -> Self {
        Term(term)
    }
    
    /// Get the inner value
    pub fn inner(self) -> u64 {
        self.0
    }
    
    /// Get the next term
    pub fn next(self) -> Self {
        Term(self.0 + 1)
    }
}

impl LogIndex {
    /// Create a new log index
    pub fn new(index: u64) -> Self {
        LogIndex(index)
    }
    
    /// Get the inner value
    pub fn inner(self) -> u64 {
        self.0
    }
    
    /// Get the next index
    pub fn next(self) -> Self {
        LogIndex(self.0 + 1)
    }
    
    /// Get the previous index, returns None if at 0
    pub fn prev(self) -> Option<Self> {
        if self.0 > 0 {
            Some(LogIndex(self.0 - 1))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;