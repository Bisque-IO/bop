use std::mem;
use std::path::PathBuf;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::time::Duration;

use maniac_sys::*;

use crate::aof_state_manager::{AofStateManager, AofStateManagerConfig};
use crate::async_result::{AsyncBool, AsyncBuffer, AsyncU64};
use crate::buffer::Buffer;
use crate::callbacks::events::ServerCallbacksHandle;
use crate::callbacks::log_store::LogStoreHandle;
use crate::callbacks::logger::LoggerHandle;
use crate::callbacks::state_machine::StateMachineHandle;
use crate::callbacks::state_manager::StateManagerHandle;
use crate::config::{ClusterConfig, ClusterMembershipSnapshot, RaftParams, ServerConfig};
use crate::error::{RaftError, RaftResult};
use crate::metrics::{Counter, Gauge, Histogram};
use crate::state::PeerInfo;
use crate::storage::{
    LogStoreBuild, SegmentedStorageRuntime, SegmentedStorageSettings, StateManagerBuild,
    StorageComponents,
};
use crate::traits::{
    LogStoreInterface, Logger, ServerCallbacks, StateMachine, StateManagerInterface,
};
use crate::types::{LogIndex, PrioritySetResult, ServerId, Term};

/// Main Raft server implementation.
pub struct RaftServer {
    ptr: NonNull<bop_raft_server_ptr>,
    _state_machine: StateMachineHandle,
    _state_manager: StateManagerHandle,
    _logger: Option<LoggerHandle>,
    _callbacks: Option<ServerCallbacksHandle>,
    _asio_service: AsioService,
    _params: RaftParams,
    _storage_runtime: Option<SegmentedStorageRuntime>,
}

/// Launch configuration for a Raft server instance.
pub struct LaunchConfig {
    pub state_machine: Box<dyn StateMachine>,
    pub state_manager: StateManagerBuild,
    pub log_store: LogStoreBuild,
    pub asio_service: AsioService,
    pub params: RaftParams,
    pub logger: Option<Arc<dyn Logger>>,
    pub callbacks: Option<Arc<dyn ServerCallbacks>>,
    pub port: i32,
    pub skip_initial_election_timeout: bool,
    pub start_immediately: bool,
    pub test_mode: bool,
    pub storage_runtime: Option<SegmentedStorageRuntime>,
}

impl RaftServer {
    /// Launch a new Raft server using the provided configuration.
    pub fn launch(config: LaunchConfig) -> RaftResult<Self> {
        let LaunchConfig {
            state_machine,
            state_manager,
            log_store,
            asio_service,
            params,
            logger,
            callbacks,
            port,
            skip_initial_election_timeout,
            start_immediately,
            test_mode,
            storage_runtime,
        } = config;

        let state_machine_handle = StateMachineHandle::new(state_machine)?;
        let logger_handle = match logger {
            Some(logger) => Some(LoggerHandle::new(logger)?),
            None => None,
        };
        let logger_ptr = logger_handle.as_ref().map(|handle| handle.as_ptr());
        let log_store_handle = LogStoreHandle::new(log_store, logger_ptr)?;
        let log_backend = log_store_handle.backend();
        let state_manager_handle =
            StateManagerHandle::new(state_manager, Some(log_store_handle), logger_ptr)?;
        if state_manager_handle.backend() != log_backend {
            return Err(RaftError::ConfigError(
                "State manager and log store backends must match".into(),
            ));
        }
        let callbacks_handle: Option<ServerCallbacksHandle> =
            callbacks.map(ServerCallbacksHandle::new);
        let (user_data, cb_func) = if let Some(handle) = callbacks_handle.as_ref() {
            (handle.user_data(), handle.func())
        } else {
            (ptr::null_mut(), None)
        };

        let raw_ptr = unsafe {
            bop_raft_server_launch(
                user_data,
                state_machine_handle.as_ptr(),
                state_manager_handle.as_ptr(),
                logger_handle
                    .as_ref()
                    .map(|handle| handle.as_ptr())
                    .unwrap_or(ptr::null_mut()),
                port,
                asio_service.as_ptr(),
                params.as_ptr(),
                skip_initial_election_timeout,
                start_immediately,
                test_mode,
                cb_func,
            )
        };

        let ptr = match NonNull::new(raw_ptr) {
            Some(ptr) => ptr,
            None => {
                let err = callbacks_handle
                    .as_ref()
                    .and_then(|handle| handle.take_last_error())
                    .or_else(|| state_machine_handle.take_last_error())
                    .or_else(|| state_manager_handle.take_last_error())
                    .unwrap_or(RaftError::ServerError);
                return Err(err);
            }
        };

        Ok(Self {
            ptr,
            _state_machine: state_machine_handle,
            _state_manager: state_manager_handle,
            _logger: logger_handle,
            _callbacks: callbacks_handle,
            _asio_service: asio_service,
            _params: params,
            _storage_runtime: storage_runtime,
        })
    }

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
            let handler_ptr = handler.map(|h| h.as_mut_ptr()).unwrap_or(ptr::null_mut());
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
            let handler_ptr = handler.map(|h| h.as_mut_ptr()).unwrap_or(ptr::null_mut());
            let ok = bop_raft_server_remove_srv(server_ptr, server_id.0, handler_ptr);
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
            let handler_ptr = handler.map(|h| h.as_mut_ptr()).unwrap_or(ptr::null_mut());
            let ok =
                bop_raft_server_flip_learner_flag(server_ptr, server_id.0, learner, handler_ptr);
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
            mem::forget(buffer);
        }

        unsafe {
            let server_ptr = self.raw_server_ptr();
            let handler_ptr = handler.map(|h| h.as_mut_ptr()).unwrap_or(ptr::null_mut());
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
            map_priority_result(bop_raft_server_set_priority(
                self.raw_server_ptr(),
                server_id.0,
                priority,
                broadcast_when_leader_exists,
            ))
        }
    }

    /// Yield leadership to an optional successor.
    pub fn yield_leadership(&mut self, immediate: bool, successor: Option<ServerId>) {
        unsafe {
            let successor_id = successor.map(|id| id.0).unwrap_or(-1);
            bop_raft_server_yield_leadership(self.raw_server_ptr(), immediate, successor_id);
        }
    }

    /// Request leadership.
    pub fn request_leadership(&mut self) -> bool {
        unsafe { bop_raft_server_request_leadership(self.raw_server_ptr()) }
    }

    /// Ask the leader to re-establish the connection to this server.
    pub fn send_reconnect_request(&mut self) {
        unsafe { bop_raft_server_send_reconnect_request(self.raw_server_ptr()) }
    }

    /// Fetch the server's current Raft parameters.
    pub fn current_params(&self) -> RaftResult<RaftParams> {
        let params = RaftParams::new()?;
        unsafe {
            bop_raft_server_get_current_params(self.raw_server_ptr(), params.as_ptr());
        }
        Ok(params)
    }

    /// Apply a new set of Raft parameters and keep the internal copy in sync.
    pub fn update_params(&mut self, params: &RaftParams) -> RaftResult<()> {
        unsafe { bop_raft_server_update_params(self.raw_server_ptr(), params.as_ptr()) };
        self._params.copy_from(params);
        Ok(())
    }

    /// Convenience helper to fetch, mutate, and reapply parameters atomically.
    pub fn reload_params<F>(&mut self, updater: F) -> RaftResult<()>
    where
        F: FnOnce(&mut RaftParams),
    {
        let mut params = self.current_params()?;
        updater(&mut params);
        self.update_params(&params)
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
            let mut handle = ClusterConfigPtr::new()?;
            bop_raft_server_get_config(self.raw_server_ptr(), handle.as_mut_ptr());

            let cfg_ptr = handle.get();
            if cfg_ptr.is_null() {
                return Err(RaftError::NullPointer);
            }

            let serialized_ptr = bop_raft_cluster_config_serialize(cfg_ptr);
            let mut serialized = Buffer::from_raw(serialized_ptr)?;
            let cloned_ptr = bop_raft_cluster_config_deserialize(serialized.as_ptr());
            let cluster = ClusterConfig::from_raw(cloned_ptr)?;
            Ok(cluster)
        }
    }

    /// Capture a read-only snapshot of cluster membership.
    pub fn membership_snapshot(&self) -> RaftResult<ClusterMembershipSnapshot> {
        let config = self.get_config()?;
        ClusterMembershipSnapshot::from_view(config.view())
    }

    /// Get peer information for all servers.
    pub fn get_peer_info_all(&self) -> RaftResult<Vec<PeerInfo>> {
        unsafe {
            let mut vec = PeerInfoVec::new()?;
            bop_raft_server_get_peer_info_all(self.raw_server_ptr(), vec.as_mut_ptr());

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

    /// Pause state machine execution until the timeout elapses or the pause completes.
    pub fn pause_state_machine_execution(&mut self, timeout: Duration) {
        unsafe {
            bop_raft_server_pause_state_machine_execution(
                self.raw_server_ptr(),
                duration_to_millis(timeout),
            );
        }
    }

    /// Resume state machine execution.
    pub fn resume_state_machine_execution(&mut self) {
        unsafe { bop_raft_server_resume_state_machine_execution(self.raw_server_ptr()) }
    }

    /// Check if the state machine execution is currently paused.
    pub fn is_state_machine_execution_paused(&self) -> bool {
        unsafe { bop_raft_server_is_state_machine_execution_paused(self.raw_server_ptr()) }
    }

    /// Wait until the state machine pause completes or the timeout expires.
    pub fn wait_for_state_machine_pause(&mut self, timeout: Duration) -> bool {
        unsafe {
            bop_raft_server_wait_for_state_machine_pause(
                self.raw_server_ptr(),
                duration_to_millis(timeout),
            )
        }
    }

    /// Notify NuRaft whether the latest append batch was successful.
    pub fn notify_log_append_completion(&mut self, ok: bool) {
        unsafe { bop_raft_server_notify_log_append_completion(self.raw_server_ptr(), ok) }
    }

    /// Create a snapshot synchronously, returning the snapshot log index.
    pub fn create_snapshot(&mut self, serialize_commit: bool) -> LogIndex {
        unsafe {
            LogIndex(bop_raft_server_create_snapshot(
                self.raw_server_ptr(),
                serialize_commit,
            ))
        }
    }

    /// Schedule a snapshot asynchronously and return the completion handle.
    pub fn schedule_snapshot_creation(&mut self) -> RaftResult<AsyncU64> {
        let mut handle = AsyncU64::new(ptr::null_mut(), None)?;
        unsafe {
            bop_raft_server_schedule_snapshot_creation(self.raw_server_ptr(), handle.as_mut_ptr());
        }
        Ok(handle)
    }

    /// Get the last snapshot index recorded by the server.
    pub fn get_last_snapshot_index(&self) -> LogIndex {
        unsafe { LogIndex(bop_raft_server_get_last_snapshot_idx(self.raw_server_ptr())) }
    }

    /// Mark or unmark this server as down for heartbeat purposes.
    pub fn set_self_mark_down(&mut self, to: bool) -> bool {
        unsafe { bop_raft_server_set_self_mark_down(self.raw_server_ptr(), to) }
    }

    /// Returns true when the server participates in the full consensus group.
    pub fn is_part_of_full_consensus(&mut self) -> bool {
        unsafe { bop_raft_server_is_part_of_full_consensus(self.raw_server_ptr()) }
    }

    /// Returns true if the current leader excludes this node from voting.
    pub fn is_excluded_by_leader(&mut self) -> bool {
        unsafe { bop_raft_server_is_excluded_by_leader(self.raw_server_ptr()) }
    }

    /// Waits until the state machine commits at least  and resolves asynchronously.
    pub fn wait_for_state_machine_commit(&mut self, target: LogIndex) -> RaftResult<AsyncBool> {
        let mut handle = AsyncBool::new(ptr::null_mut(), None)?;
        let ok = unsafe {
            bop_raft_server_wait_for_state_machine_commit(
                self.raw_server_ptr(),
                handle.as_mut_ptr(),
                target.0,
            )
        };
        if ok {
            Ok(handle)
        } else {
            Err(RaftError::ServerError)
        }
    }

    /// Obtain the value of a counter metric.
    pub fn counter_value(&self, name: &str) -> RaftResult<u64> {
        let mut counter = Counter::new(name)?;
        Ok(
            unsafe {
                bop_raft_server_get_stat_counter(self.raw_server_ptr(), counter.as_mut_ptr())
            },
        )
    }

    /// Reset the specified counter metric.
    pub fn reset_counter(&mut self, name: &str) -> RaftResult<()> {
        let mut counter = Counter::new(name)?;
        unsafe { bop_raft_server_reset_counter(self.raw_server_ptr(), counter.as_mut_ptr()) };
        Ok(())
    }

    /// Obtain the value of a gauge metric.
    pub fn gauge_value(&self, name: &str) -> RaftResult<i64> {
        let mut gauge = Gauge::new(name)?;
        Ok(unsafe { bop_raft_server_get_stat_gauge(self.raw_server_ptr(), gauge.as_mut_ptr()) })
    }

    /// Reset the specified gauge metric.
    pub fn reset_gauge(&mut self, name: &str) -> RaftResult<()> {
        let mut gauge = Gauge::new(name)?;
        unsafe { bop_raft_server_reset_gauge(self.raw_server_ptr(), gauge.as_mut_ptr()) };
        Ok(())
    }

    /// Fetch histogram buckets for the supplied upper bounds.
    pub fn histogram_buckets(&self, name: &str, bounds: &[f64]) -> RaftResult<Vec<(f64, u64)>> {
        let mut histogram = Histogram::new(name)?;
        let success = unsafe {
            bop_raft_server_get_stat_histogram(self.raw_server_ptr(), histogram.as_mut_ptr())
        };
        if !success {
            return Err(RaftError::ServerError);
        }
        Ok(histogram.snapshot(bounds))
    }

    /// Reset the named histogram metric.
    pub fn reset_histogram(&mut self, name: &str) -> RaftResult<()> {
        let mut histogram = Histogram::new(name)?;
        unsafe { bop_raft_server_reset_histogram(self.raw_server_ptr(), histogram.as_mut_ptr()) };
        Ok(())
    }

    /// Reset all collected statistics.
    pub fn reset_all_stats(&mut self) {
        unsafe { bop_raft_server_reset_all_stats(self.raw_server_ptr()) }
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
#[allow(dead_code)]
pub struct AsioService {
    ptr: NonNull<bop_raft_asio_service_ptr>,
}

impl AsioService {
    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *mut bop_raft_asio_service_ptr {
        self.ptr.as_ptr()
    }
}

/// Builder for creating Raft server instances.
pub struct RaftServerBuilder {
    asio_service: Option<AsioService>,
    params: Option<RaftParams>,
    state_machine: Option<Box<dyn StateMachine>>,
    state_manager: Option<StateManagerBuild>,
    log_store: Option<LogStoreBuild>,
    storage_runtime: Option<SegmentedStorageRuntime>,
    logger: Option<Arc<dyn Logger>>,
    callbacks: Option<Arc<dyn ServerCallbacks>>,
    port: i32,
    skip_initial_election_timeout: bool,
    start_immediately: bool,
    test_mode: bool,
}

impl RaftServerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            asio_service: None,
            params: None,
            state_machine: None,
            state_manager: None,
            log_store: None,
            storage_runtime: None,
            logger: None,
            callbacks: None,
            port: 0,
            skip_initial_election_timeout: false,
            start_immediately: true,
            test_mode: false,
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

    /// Set the state machine implementation.
    pub fn state_machine(mut self, state_machine: Box<dyn StateMachine>) -> Self {
        self.state_machine = Some(state_machine);
        self
    }

    /// Set the state manager.
    pub fn state_manager(mut self, state_mgr: Box<dyn StateManagerInterface>) -> Self {
        let backend = state_mgr.storage_backend();
        self.state_manager = Some(StateManagerBuild::Callbacks {
            backend,
            manager: state_mgr,
        });
        self
    }

    /// Set the log store.
    pub fn log_store(mut self, log_store: Box<dyn LogStoreInterface>) -> Self {
        let backend = log_store.storage_backend();
        self.log_store = Some(LogStoreBuild::Callbacks {
            backend,
            store: log_store,
        });
        self
    }

    /// Configure a segmented storage backend using bop-aof.
    pub fn segmented_storage_settings(
        mut self,
        start_index: LogIndex,
        settings: SegmentedStorageSettings,
    ) -> RaftResult<Self> {
        let components = settings.build(start_index)?;
        let (state_manager, log_store, storage_runtime) = components.into_parts();
        self.state_manager = Some(state_manager);
        self.log_store = Some(log_store);
        self.storage_runtime = storage_runtime;
        Ok(self)
    }

    /// Provide both state manager and log store as a single storage bundle.
    pub fn storage(mut self, components: StorageComponents) -> Self {
        let (state_manager, log_store, storage_runtime) = components.into_parts();
        self.state_manager = Some(state_manager);
        self.log_store = Some(log_store);
        self.storage_runtime = storage_runtime;
        self
    }

    /// Configure the builder to use the AOF-backed state manager stored at `root_dir`.
    pub fn try_aof_state_manager<P>(self, root_dir: P, server_id: ServerId) -> RaftResult<Self>
    where
        P: Into<PathBuf>,
    {
        let config = AofStateManagerConfig::new(root_dir.into(), server_id);
        self.try_aof_state_manager_with(config)
    }

    /// Configure the builder with an explicit AOF state manager configuration.
    pub fn try_aof_state_manager_with(mut self, config: AofStateManagerConfig) -> RaftResult<Self> {
        let manager = AofStateManager::new(config)?;
        let manager = Box::new(manager) as Box<dyn StateManagerInterface>;
        let backend = manager.storage_backend();
        self.state_manager = Some(StateManagerBuild::Callbacks { backend, manager });
        Ok(self)
    }

    #[cfg(feature = "mdbx")]
    pub fn with_mdbx_storage(mut self, storage: crate::mdbx::MdbxStorage) -> Self {
        self.storage(storage.into_components())
    }

    /// Attempt to configure MDBX storage regardless of feature state.
    pub fn try_mdbx_storage<P>(self, server_config: ServerConfig, data_dir: P) -> RaftResult<Self>
    where
        P: Into<PathBuf>,
    {
        #[cfg(feature = "mdbx")]
        {
            let storage = crate::mdbx::MdbxStorage::new(server_config, data_dir.into());
            Ok(self.with_mdbx_storage(storage))
        }

        #[cfg(not(feature = "mdbx"))]
        {
            let _ = server_config;
            let _ = data_dir.into();
            Err(RaftError::feature_disabled("mdbx"))
        }
    }

    /// Set the optional logger.
    pub fn logger(mut self, logger: Arc<dyn Logger>) -> Self {
        self.logger = Some(logger);
        self
    }

    /// Set optional server callbacks.
    pub fn callbacks(mut self, callbacks: Arc<dyn ServerCallbacks>) -> Self {
        self.callbacks = Some(callbacks);
        self
    }

    /// Set the listening port number.
    pub fn port(mut self, port: i32) -> Self {
        self.port = port;
        self
    }

    /// Configure whether to skip the initial election timeout.
    pub fn skip_initial_election_timeout(mut self, skip: bool) -> Self {
        self.skip_initial_election_timeout = skip;
        self
    }

    /// Configure whether the server should start immediately after launch.
    pub fn start_immediately(mut self, start: bool) -> Self {
        self.start_immediately = start;
        self
    }

    /// Enable or disable test mode during launch.
    pub fn test_mode(mut self, test_mode: bool) -> Self {
        self.test_mode = test_mode;
        self
    }

    /// Build the Raft server.
    pub fn build(self) -> RaftResult<RaftServer> {
        let asio_service = self
            .asio_service
            .ok_or_else(|| RaftError::ConfigError("ASIO service is required".to_string()))?;

        let params = self.params.unwrap_or_else(RaftParams::default);

        let state_machine = self
            .state_machine
            .ok_or_else(|| RaftError::ConfigError("State machine is required".to_string()))?;

        let state_manager = self
            .state_manager
            .ok_or_else(|| RaftError::ConfigError("State manager is required".to_string()))?;

        let log_store = self
            .log_store
            .ok_or_else(|| RaftError::ConfigError("Log store is required".to_string()))?;

        let sm_backend = state_manager.backend();
        let ls_backend = log_store.backend();
        if sm_backend != ls_backend {
            return Err(RaftError::ConfigError(format!(
                "Storage backend mismatch: state manager uses `{}`, log store uses `{}`",
                sm_backend, ls_backend
            )));
        }

        let config = LaunchConfig {
            state_machine,
            state_manager,
            log_store,
            asio_service,
            params,
            logger: self.logger,
            callbacks: self.callbacks,
            port: self.port,
            skip_initial_election_timeout: self.skip_initial_election_timeout,
            start_immediately: self.start_immediately,
            test_mode: self.test_mode,
            storage_runtime: self.storage_runtime,
        };

        RaftServer::launch(config)
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
        unsafe { bop_raft_cluster_config_ptr_get(self.ptr.as_ptr()) }
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
        unsafe { bop_raft_server_peer_info_vec_get(self.ptr.as_ptr(), idx) }
    }
}

impl Drop for PeerInfoVec {
    fn drop(&mut self) {
        unsafe {
            bop_raft_server_peer_info_vec_delete(self.ptr.as_ptr());
        }
    }
}

fn duration_to_millis(duration: Duration) -> usize {
    let millis = duration.as_millis();
    if millis > usize::MAX as u128 {
        usize::MAX
    } else {
        millis as usize
    }
}

fn map_priority_result(code: bop_raft_server_priority_set_result) -> PrioritySetResult {
    match code {
        0 => PrioritySetResult::Set,
        1 => PrioritySetResult::Broadcast,
        _ => PrioritySetResult::Ignored,
    }
}
