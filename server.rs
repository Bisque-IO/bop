use std::ptr::{self, NonNull};

use bop_sys::*;

use crate::async_result::AsyncBuffer;
use crate::buffer::Buffer;
use crate::config::{ClusterConfig, RaftParams, ServerConfig};
use crate::error::{RaftError, RaftResult};
use crate::state::PeerInfo;
use crate::types::{LogIndex, PrioritySetResult, ServerId, Term};

/// Main Raft server implementation.
pub struct RaftServer {
    ptr: NonNull<bop_raft_server_ptr>,
}

impl RaftServer {
    /// TODO: Implement server launch once callback shims are in place.
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
    //
    //     NonNull::new(ptr)
    //         .map(|ptr| RaftServer { ptr })
    //         .ok_or(RaftError::NullPointer)
    // }

    /// Stop the server with a timeout.
    pub fn stop(&mut self, time_limit_sec: usize) -> bool {
        unsafe { bop_raft_server_stop(self.ptr.as_ptr(), time_limit_sec) }
    }

    /// Check if server is initialized.
    pub fn is_initialized(&self) -> bool {
        unsafe { bop_raft_server_is_initialized(self.raw_server_ptr()) }
    }

    /// Check if server is catching up.
    pub fn is_catching_up(&self) -> bool {
        unsafe { bop_raft_server_is_catching_up(self.raw_server_ptr()) }
    }

    /// Check if server is receiving a snapshot.
    pub fn is_receiving_snapshot(&self) -> bool {
        unsafe { bop_raft_server_is_receiving_snapshot(self.raw_server_ptr()) }
    }

    /// Add a server to the cluster.
    pub fn add_srv(
        &mut self,
        srv_config: &ServerConfig,
        handler: Option<&mut AsyncBuffer>,
    ) -> RaftResult<()> {
        unsafe {
            let server_ptr = self.raw_server_ptr();
            let handler_ptr = handler
                .map(|h| h.as_mut_ptr())
                .unwrap_or(ptr::null_mut());
            let ok = bop_raft_server_add_srv(
                server_ptr,
                srv_config.as_handle_ptr() as *const bop_raft_srv_config_ptr,
                handler_ptr,
            );
            if ok {
                Ok(())
            } else {
                Err(RaftError::ServerError)
            }
        }
    }

    /// Remove a server from the cluster.
    pub fn remove_srv(
        &mut self,
        server_id: ServerId,
        handler: Option<&mut AsyncBuffer>,
    ) -> RaftResult<()> {
        unsafe {
            let server_ptr = self.raw_server_ptr();
            let handler_ptr = handler
                .map(|h| h.as_mut_ptr())
                .unwrap_or(ptr::null_mut());
            let ok =
                bop_raft_server_remove_srv(server_ptr, server_id.0, handler_ptr);
            if ok {
                Ok(())
            } else {
                Err(RaftError::ServerError)
            }
        }
    }

    /// Set or clear the learner flag for a server.
    pub fn flip_learner_flag(
        &mut self,
        server_id: ServerId,
        learner: bool,
        handler: Option<&mut AsyncBuffer>,
    ) -> RaftResult<()> {
        unsafe {
            let server_ptr = self.raw_server_ptr();
            let handler_ptr = handler
                .map(|h| h.as_mut_ptr())
                .unwrap_or(ptr::null_mut());
            let ok = bop_raft_server_flip_learner_flag(
                server_ptr,
                server_id.0,
                learner,
                handler_ptr,
            );
            if ok {
                Ok(())
            } else {
                Err(RaftError::ServerError)
            }
        }
    }

    /// Append entries to the log.
    pub fn append_entries(
        &mut self,
        mut entries: Vec<Buffer>,
        handler: Option<&mut AsyncBuffer>,
    ) -> RaftResult<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut append_entries = AppendEntriesPtr::new()?;
        for buffer in entries.iter_mut() {
            append_entries.push(buffer)?;
        }

        // Transfer ownership of each buffer to the underlying C++ container.
        for buffer in entries {
            std::mem::forget(buffer);
        }

        unsafe {
            let server_ptr = self.raw_server_ptr();
            let handler_ptr = handler
                .map(|h| h.as_mut_ptr())
                .unwrap_or(ptr::null_mut());
            let ok = bop_raft_server_append_entries(
                server_ptr,
                append_entries.as_mut_ptr(),
                handler_ptr,
            );
            if ok {
                Ok(())
            } else {
                Err(RaftError::ServerError)
            }
        }
    }

    /// Set priority for a server.
    pub fn set_priority(
        &mut self,
        server_id: ServerId,
        priority: i32,
        broadcast_when_leader_exists: bool,
    ) -> PrioritySetResult {
        unsafe {
            let server_ptr = self.raw_server_ptr();
            map_priority_result(bop_raft_server_set_priority(
                server_ptr,
                server_id.0,
                priority,
                broadcast_when_leader_exists,
            ))
        }
    }

    /// Yield leadership to an optional successor.
    pub fn yield_leadership(
        &mut self,
        immediate: bool,
        successor: Option<ServerId>,
    ) {
        unsafe {
            let server_ptr = self.raw_server_ptr();
            let successor_id = successor.map(|id| id.0).unwrap_or(-1);
            bop_raft_server_yield_leadership(server_ptr, immediate, successor_id);
        }
    }

    /// Request leadership.
    pub fn request_leadership(&mut self) -> bool {
        unsafe { bop_raft_server_request_leadership(self.raw_server_ptr()) }
    }

    /// Get the server ID.
    pub fn get_id(&self) -> ServerId {
        unsafe { ServerId(bop_raft_server_get_id(self.raw_server_ptr())) }
    }

    /// Get the current term.
    pub fn get_term(&self) -> Term {
        unsafe { Term(bop_raft_server_get_term(self.raw_server_ptr())) }
    }

    /// Get the last log index.
    pub fn get_last_log_idx(&self) -> LogIndex {
        unsafe { LogIndex(bop_raft_server_get_last_log_idx(self.raw_server_ptr())) }
    }

    /// Get the committed log index.
    pub fn get_committed_log_idx(&self) -> LogIndex {
        unsafe { LogIndex(bop_raft_server_get_committed_log_idx(self.raw_server_ptr())) }
    }

    /// Get the leader ID.
    pub fn get_leader(&self) -> ServerId {
        unsafe { ServerId(bop_raft_server_get_leader(self.raw_server_ptr())) }
    }

    /// Check if this server is the leader.
    pub fn is_leader(&self) -> bool {
        unsafe { bop_raft_server_is_leader(self.raw_server_ptr()) }
    }

    /// Check if the leader is alive.
    pub fn is_leader_alive(&self) -> bool {
        unsafe { bop_raft_server_is_leader_alive(self.raw_server_ptr()) }
    }

    /// Get the current cluster configuration.
    pub fn get_config(&self) -> RaftResult<ClusterConfig> {
        unsafe {
            let server_ptr = self.raw_server_ptr();
            let mut handle = ClusterConfigPtr::new()?;
            bop_raft_server_get_config(server_ptr, handle.as_mut_ptr());

            let cfg_ptr = handle.get();
            if cfg_ptr.is_null() {
                return Err(RaftError::NullPointer);
            }

            let serialized_ptr = bop_raft_cluster_config_serialize(cfg_ptr);
            let mut serialized = Buffer::from_raw(serialized_ptr)?;
            let cloned_ptr = bop_raft_cluster_config_deserialize(serialized.as_ptr());
            let cluster = ClusterConfig::from_raw(cloned_ptr)?;
            // serialized buffer will be released when it drops here.
            Ok(cluster)
        }
    }

    /// Get peer information for all servers.
    pub fn get_peer_info_all(&self) -> RaftResult<Vec<PeerInfo>> {
        unsafe {
            let server_ptr = self.raw_server_ptr();
            let mut vec = PeerInfoVec::new()?;
            bop_raft_server_get_peer_info_all(server_ptr, vec.as_mut_ptr());

            let size = vec.len();
            let mut peers = Vec::with_capacity(size);
            for idx in 0..size {
                let info_ptr = vec.get(idx);
                if info_ptr.is_null() {
                    continue;
                }
                let info = *info_ptr;
                peers.push(PeerInfo {
                    server_id: ServerId(info.id),
                    last_log_idx: LogIndex(info.last_log_idx),
                    last_succ_resp_us: info.last_succ_resp_us,
                });
            }
            Ok(peers)
        }
    }

    /// Shutdown the server.
    pub fn shutdown(&mut self) {
        unsafe {
            bop_raft_server_shutdown(self.raw_server_ptr());
        }
    }

    fn raw_server_ptr(&self) -> *mut bop_raft_server {
        unsafe { bop_raft_server_get(self.ptr.as_ptr()) }
    }
}

impl Drop for RaftServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

unsafe impl Send for RaftServer {}
unsafe impl Sync for RaftServer {}

/// Placeholder for AsioService - will need proper implementation.
pub struct AsioService {
    ptr: NonNull<bop_raft_asio_service_ptr>,
}

impl AsioService {
    pub(crate) fn as_ptr(&self) -> *mut bop_raft_asio_service_ptr {
        self.ptr.as_ptr()
    }
}

/// Placeholder for StateManager - will need proper implementation.
pub struct StateManager {
    ptr: NonNull<bop_raft_state_mgr_ptr>,
}

impl StateManager {
    pub(crate) fn as_ptr(&self) -> *mut bop_raft_state_mgr_ptr {
        self.ptr.as_ptr()
    }
}

/// Placeholder for LogStore - will need proper implementation.
pub struct LogStore {
    ptr: NonNull<bop_raft_log_store_ptr>,
}

impl LogStore {
    pub(crate) fn as_ptr(&self) -> *mut bop_raft_log_store_ptr {
        self.ptr.as_ptr()
    }
}

/// Placeholder for RpcListener - will need proper implementation.
pub struct RpcListener {
    ptr: NonNull<bop_raft_rpc_listener_ptr>,
}

impl RpcListener {
    pub(crate) fn as_ptr(&self) -> *mut bop_raft_rpc_listener_ptr {
        self.ptr.as_ptr()
    }
}

/// Builder for creating Raft server instances.
pub struct RaftServerBuilder {
    asio_service: Option<AsioService>,
    params: Option<RaftParams>,
    state_mgr: Option<StateManager>,
    log_store: Option<LogStore>,
    listener: Option<RpcListener>,
}

impl RaftServerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            asio_service: None,
            params: None,
            state_mgr: None,
            log_store: None,
            listener: None,
        }
    }

    /// Set the ASIO service.
    pub fn asio_service(mut self, service: AsioService) -> Self {
        self.asio_service = Some(service);
        self
    }

    /// Set the Raft parameters.
    pub fn params(mut self, params: RaftParams) -> Self {
        self.params = Some(params);
        self
    }

    /// Set the state manager.
    pub fn state_manager(mut self, state_mgr: StateManager) -> Self {
        self.state_mgr = Some(state_mgr);
        self
    }

    /// Set the log store.
    pub fn log_store(mut self, log_store: LogStore) -> Self {
        self.log_store = Some(log_store);
        self
    }

    /// Set the RPC listener.
    pub fn listener(mut self, listener: RpcListener) -> Self {
        self.listener = Some(listener);
        self
    }

    /// Build the Raft server.
    pub fn build(self) -> RaftResult<RaftServer> {
        let asio_service = self
            .asio_service
            .ok_or_else(|| RaftError::ConfigError("ASIO service is required".to_string()))?;

        let params = self
            .params
            .ok_or_else(|| RaftError::ConfigError("Raft parameters are required".to_string()))?;

        let state_mgr = self
            .state_mgr
            .ok_or_else(|| RaftError::ConfigError("State manager is required".to_string()))?;

        let log_store = self
            .log_store
            .ok_or_else(|| RaftError::ConfigError("Log store is required".to_string()))?;

        let listener = self
            .listener
            .ok_or_else(|| RaftError::ConfigError("RPC listener is required".to_string()))?;

        // Create a second params instance for the second parameter
        let raft_params = RaftParams::new()?;

        // Placeholder until launch is implemented.
        let _ = (
            asio_service,
            params,
            state_mgr,
            log_store,
            listener,
            raft_params,
        );
        Err(RaftError::ConfigError(
            "RaftServer::launch is not implemented yet".to_string(),
        ))
    }
}

impl Default for RaftServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

struct AppendEntriesPtr {
    ptr: NonNull<bop_raft_append_entries_ptr>,
}

impl AppendEntriesPtr {
    fn new() -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_append_entries_create() };
        NonNull::new(ptr)
            .map(|ptr| AppendEntriesPtr { ptr })
            .ok_or(RaftError::NullPointer)
    }

    fn push(&mut self, buffer: &mut Buffer) -> RaftResult<()> {
        unsafe {
            let _ = bop_raft_append_entries_push(self.ptr.as_ptr(), buffer.as_ptr());
        }
        Ok(())
    }

    fn as_mut_ptr(&mut self) -> *mut bop_raft_append_entries_ptr {
        self.ptr.as_ptr()
    }
}

impl Drop for AppendEntriesPtr {
    fn drop(&mut self) {
        unsafe {
            bop_raft_append_entries_delete(self.ptr.as_ptr());
        }
    }
}

struct ClusterConfigPtr {
    ptr: NonNull<bop_raft_cluster_config_ptr>,
}

impl ClusterConfigPtr {
    fn new() -> RaftResult<Self> {
        unsafe {
            let raw_cfg = bop_raft_cluster_config_new();
            let raw_cfg = NonNull::new(raw_cfg).ok_or(RaftError::NullPointer)?;
            let ptr = bop_raft_cluster_config_ptr_make(raw_cfg.as_ptr());
            if ptr.is_null() {
                bop_raft_cluster_config_free(raw_cfg.as_ptr());
                return Err(RaftError::NullPointer);
            }
            Ok(ClusterConfigPtr {
                ptr: NonNull::new_unchecked(ptr),
            })
        }
    }

    fn as_mut_ptr(&mut self) -> *mut bop_raft_cluster_config_ptr {
        self.ptr.as_ptr()
    }

    unsafe fn get(&mut self) -> *mut bop_raft_cluster_config {
        bop_raft_cluster_config_ptr_get(self.ptr.as_ptr())
    }
}

impl Drop for ClusterConfigPtr {
    fn drop(&mut self) {
        unsafe {
            bop_raft_cluster_config_ptr_delete(self.ptr.as_ptr());
        }
    }
}

struct PeerInfoVec {
    ptr: NonNull<bop_raft_server_peer_info_vec>,
}

impl PeerInfoVec {
    fn new() -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_server_peer_info_vec_make() };
        NonNull::new(ptr)
            .map(|ptr| PeerInfoVec { ptr })
            .ok_or(RaftError::NullPointer)
    }

    fn as_mut_ptr(&mut self) -> *mut bop_raft_server_peer_info_vec {
        self.ptr.as_ptr()
    }

    fn len(&self) -> usize {
        unsafe { bop_raft_server_peer_info_vec_size(self.ptr.as_ptr()) }
    }

    unsafe fn get(&self, idx: usize) -> *mut bop_raft_server_peer_info {
        bop_raft_server_peer_info_vec_get(self.ptr.as_ptr(), idx)
    }
}

impl Drop for PeerInfoVec {
    fn drop(&mut self) {
        unsafe {
            bop_raft_server_peer_info_vec_delete(self.ptr.as_ptr());
        }
    }
}

fn map_priority_result(code: bop_raft_server_priority_set_result) -> PrioritySetResult {
    match code {
        bop_raft_server_priority_set_result_bop_raft_server_priority_set_result_set => {
            PrioritySetResult::Set
        }
        bop_raft_server_priority_set_result_bop_raft_server_priority_set_result_broadcast => {
            PrioritySetResult::Broadcast
        }
        _ => PrioritySetResult::Ignored,
    }
}
