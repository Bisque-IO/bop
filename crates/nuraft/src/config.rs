use std::collections::HashMap;
use std::ffi::CString;
use std::marker::PhantomData;
use std::ptr::{self, NonNull};
use std::slice;

use maniac_sys::*;

use crate::buffer::Buffer;
use crate::error::{RaftError, RaftResult};
use crate::types::{DataCenterId, LogIndex, ServerId};

/// Server configuration for a Raft node.
#[derive(Debug)]
pub struct ServerConfig {
    ptr: NonNull<bop_raft_srv_config>,
    handle: NonNull<bop_raft_srv_config_ptr>,
}

impl ServerConfig {
    /// Create a new server configuration.
    pub fn new(
        id: ServerId,
        dc_id: DataCenterId,
        endpoint: &str,
        aux: &str,
        is_learner: bool,
    ) -> RaftResult<Self> {
        let endpoint_cstr = CString::new(endpoint)?;
        let aux_cstr = CString::new(aux)?;

        let raw_config = unsafe {
            bop_raft_srv_config_make(
                id.0,
                dc_id.0,
                endpoint_cstr.as_ptr(),
                endpoint.len(),
                aux_cstr.as_ptr(),
                aux.len(),
                is_learner,
                0,
            )
        };
        let ptr = NonNull::new(raw_config).ok_or(RaftError::NullPointer)?;
        let raw_handle = unsafe { bop_raft_srv_config_ptr_make(ptr.as_ptr()) };
        let handle = NonNull::new(raw_handle).ok_or(RaftError::NullPointer)?;
        Ok(ServerConfig { ptr, handle })
    }

    /// Internal helper for wrapping an existing pointer.
    #[allow(dead_code)]
    pub(crate) unsafe fn from_raw(ptr: *mut bop_raft_srv_config) -> RaftResult<Self> {
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        let raw_handle = unsafe { bop_raft_srv_config_ptr_make(ptr.as_ptr()) };
        let handle = NonNull::new(raw_handle).ok_or(RaftError::NullPointer)?;
        Ok(ServerConfig { ptr, handle })
    }

    /// Get the server ID.
    pub fn id(&self) -> ServerId {
        ServerId(unsafe { bop_raft_srv_config_id(self.ptr.as_ptr()) })
    }

    /// Get the data center ID.
    pub fn dc_id(&self) -> DataCenterId {
        DataCenterId(unsafe { bop_raft_srv_config_dc_id(self.ptr.as_ptr()) })
    }

    /// Get the endpoint string.
    pub fn endpoint(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_srv_config_endpoint(self.ptr.as_ptr());
            let size = bop_raft_srv_config_endpoint_size(self.ptr.as_ptr());
            let slice = slice::from_raw_parts(ptr as *const u8, size);
            Ok(String::from_utf8_lossy(slice).into_owned())
        }
    }

    /// Get the auxiliary data string.
    pub fn aux(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_srv_config_aux(self.ptr.as_ptr());
            let size = bop_raft_srv_config_aux_size(self.ptr.as_ptr());
            let slice = slice::from_raw_parts(ptr as *const u8, size);
            Ok(String::from_utf8_lossy(slice).into_owned())
        }
    }

    /// Check if this is a learner node.
    pub fn is_learner(&self) -> bool {
        unsafe { bop_raft_srv_config_is_learner(self.ptr.as_ptr()) }
    }

    /// Set learner status.
    pub fn set_learner(&mut self, learner: bool) {
        unsafe { bop_raft_srv_config_set_is_learner(self.ptr.as_ptr(), learner) }
    }

    /// Check if this is a new joiner.
    pub fn is_new_joiner(&self) -> bool {
        unsafe { bop_raft_srv_config_is_new_joiner(self.ptr.as_ptr()) }
    }

    /// Set new joiner status.
    pub fn set_new_joiner(&mut self, new_joiner: bool) {
        unsafe { bop_raft_srv_config_set_new_joiner(self.ptr.as_ptr(), new_joiner) }
    }

    /// Get the priority.
    pub fn priority(&self) -> i32 {
        unsafe { bop_raft_srv_config_priority(self.ptr.as_ptr()) }
    }

    /// Set the priority.
    pub fn set_priority(&mut self, priority: i32) {
        unsafe { bop_raft_srv_config_set_priority(self.ptr.as_ptr(), priority) }
    }

    /// Get the raw pointer (for C API interop).
    pub fn as_ptr(&self) -> *mut bop_raft_srv_config {
        self.ptr.as_ptr()
    }

    pub fn as_handle_ptr(&self) -> *mut bop_raft_srv_config_ptr {
        self.handle.as_ptr()
    }
}

impl Drop for ServerConfig {
    fn drop(&mut self) {
        unsafe {
            bop_raft_srv_config_ptr_delete(self.handle.as_ptr());
            bop_raft_srv_config_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for ServerConfig {}
unsafe impl Sync for ServerConfig {}

/// Cluster configuration containing multiple server configurations.
pub struct ClusterConfig {
    ptr: NonNull<bop_raft_cluster_config>,
}

impl ClusterConfig {
    /// Create a new cluster configuration.
    pub fn new() -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_cluster_config_new() };
        NonNull::new(ptr)
            .map(|ptr| ClusterConfig { ptr })
            .ok_or(RaftError::NullPointer)
    }

    /// Internal helper for wrapping an existing pointer.
    pub(crate) unsafe fn from_raw(ptr: *mut bop_raft_cluster_config) -> RaftResult<Self> {
        NonNull::new(ptr)
            .map(|ptr| ClusterConfig { ptr })
            .ok_or(RaftError::NullPointer)
    }

    /// Get the log index for this configuration.
    pub fn log_idx(&self) -> LogIndex {
        LogIndex(unsafe { bop_raft_cluster_config_log_idx(self.ptr.as_ptr()) })
    }

    /// Get the previous log index.
    pub fn prev_log_idx(&self) -> LogIndex {
        LogIndex(unsafe { bop_raft_cluster_config_prev_log_idx(self.ptr.as_ptr()) })
    }

    /// Check if async replication is enabled.
    pub fn is_async_replication(&self) -> bool {
        unsafe { bop_raft_cluster_config_is_async_replication(self.ptr.as_ptr()) }
    }

    /// Get the number of servers in the configuration.
    pub fn servers_size(&self) -> usize {
        unsafe { bop_raft_cluster_config_servers_size(self.ptr.as_ptr()) }
    }

    /// Get user context data.
    pub fn user_ctx(&self) -> RaftResult<Vec<u8>> {
        unsafe {
            let size = bop_raft_cluster_config_user_ctx_size(self.ptr.as_ptr());
            if size == 0 {
                return Ok(Vec::new());
            }

            let mut data = vec![0u8; size];
            bop_raft_cluster_config_user_ctx(self.ptr.as_ptr(), data.as_mut_ptr() as *mut i8, size);
            Ok(data)
        }
    }

    /// Serialize the configuration to a buffer.
    pub fn serialize(&self) -> RaftResult<Buffer> {
        let ptr = unsafe { bop_raft_cluster_config_serialize(self.ptr.as_ptr()) };
        unsafe { Buffer::from_raw(ptr) }
    }

    /// Get the raw pointer (for C API interop).
    pub fn as_ptr(&self) -> *mut bop_raft_cluster_config {
        self.ptr.as_ptr()
    }

    #[allow(dead_code)]
    pub(crate) fn as_mut_ptr(&self) -> *mut bop_raft_cluster_config {
        self.ptr.as_ptr()
    }

    /// Borrow this configuration as an immutable view.
    pub fn view(&self) -> ClusterConfigView<'_> {
        ClusterConfigView {
            ptr: self.ptr.as_ptr(),
            _marker: PhantomData,
        }
    }

    /// Transfer ownership of the underlying raw pointer to the caller.
    #[allow(dead_code)]
    pub(crate) fn into_raw(self) -> *mut bop_raft_cluster_config {
        let ptr = self.ptr.as_ptr();
        std::mem::forget(self);
        ptr
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

/// Borrowed view into a cluster configuration owned by the C++ layer.
#[derive(Clone, Copy)]
pub struct ClusterConfigView<'a> {
    ptr: *const bop_raft_cluster_config,
    _marker: PhantomData<&'a bop_raft_cluster_config>,
}

impl<'a> ClusterConfigView<'a> {
    /// Construct a view from a raw pointer.
    #[allow(dead_code)]
    pub(crate) unsafe fn new(ptr: *const bop_raft_cluster_config) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self {
                ptr,
                _marker: PhantomData,
            })
        }
    }

    /// Get the log index for this configuration.
    pub fn log_idx(&self) -> LogIndex {
        LogIndex(unsafe { bop_raft_cluster_config_log_idx(self.ptr as *mut _) })
    }

    /// Get the previous log index.
    pub fn prev_log_idx(&self) -> LogIndex {
        LogIndex(unsafe { bop_raft_cluster_config_prev_log_idx(self.ptr as *mut _) })
    }

    /// Check if async replication is enabled.
    pub fn is_async_replication(&self) -> bool {
        unsafe { bop_raft_cluster_config_is_async_replication(self.ptr as *mut _) }
    }

    /// Get the number of servers in the configuration.
    pub fn servers_size(&self) -> usize {
        unsafe { bop_raft_cluster_config_servers_size(self.ptr as *mut _) }
    }

    /// Iterate over server configurations contained in this cluster config.
    pub fn servers(&self) -> ServerConfigIter<'a> {
        ServerConfigIter {
            config: self.ptr,
            idx: 0,
            len: self.servers_size(),
            _marker: PhantomData,
        }
    }

    /// Expose the raw pointer for advanced interop.
    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *const bop_raft_cluster_config {
        self.ptr
    }
}

unsafe impl<'a> Send for ClusterConfigView<'a> {}
unsafe impl<'a> Sync for ClusterConfigView<'a> {}

/// Borrowed view into a server configuration.
#[derive(Clone, Copy)]
pub struct ServerConfigView<'a> {
    ptr: *const bop_raft_srv_config,
    _marker: PhantomData<&'a bop_raft_srv_config>,
}

impl<'a> ServerConfigView<'a> {
    pub(crate) unsafe fn new(ptr: *const bop_raft_srv_config) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self {
                ptr,
                _marker: PhantomData,
            })
        }
    }

    pub fn id(&self) -> ServerId {
        ServerId(unsafe { bop_raft_srv_config_id(self.ptr as *mut _) })
    }

    pub fn dc_id(&self) -> DataCenterId {
        DataCenterId(unsafe { bop_raft_srv_config_dc_id(self.ptr as *mut _) })
    }

    pub fn endpoint(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_srv_config_endpoint(self.ptr as *mut _);
            let size = bop_raft_srv_config_endpoint_size(self.ptr as *mut _);
            let slice = slice::from_raw_parts(ptr as *const u8, size);
            Ok(String::from_utf8_lossy(slice).into_owned())
        }
    }

    pub fn aux(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_srv_config_aux(self.ptr as *mut _);
            let size = bop_raft_srv_config_aux_size(self.ptr as *mut _);
            let slice = slice::from_raw_parts(ptr as *const u8, size);
            Ok(String::from_utf8_lossy(slice).into_owned())
        }
    }

    pub fn is_learner(&self) -> bool {
        unsafe { bop_raft_srv_config_is_learner(self.ptr as *mut _) }
    }

    pub fn is_new_joiner(&self) -> bool {
        unsafe { bop_raft_srv_config_is_new_joiner(self.ptr as *mut _) }
    }

    pub fn priority(&self) -> i32 {
        unsafe { bop_raft_srv_config_priority(self.ptr as *mut _) }
    }

    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *const bop_raft_srv_config {
        self.ptr
    }
}

unsafe impl<'a> Send for ServerConfigView<'a> {}
unsafe impl<'a> Sync for ServerConfigView<'a> {}

/// Iterator over server configuration views.
pub struct ServerConfigIter<'a> {
    config: *const bop_raft_cluster_config,
    idx: usize,
    len: usize,
    _marker: PhantomData<&'a bop_raft_cluster_config>,
}

impl<'a> Iterator for ServerConfigIter<'a> {
    type Item = ServerConfigView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.len {
            let current = self.idx;
            self.idx += 1;
            let raw =
                unsafe { bop_raft_cluster_config_server(self.config as *mut _, current as i32) };
            if let Some(view) = unsafe { ServerConfigView::new(raw) } {
                return Some(view);
            }
        }
        None
    }
}

/// Immutable membership entry captured from a cluster config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipEntry {
    pub server_id: ServerId,
    pub dc_id: DataCenterId,
    pub endpoint: String,
    pub aux: String,
    pub is_learner: bool,
    pub is_new_joiner: bool,
    pub priority: i32,
}

impl<'a> TryFrom<ServerConfigView<'a>> for MembershipEntry {
    type Error = RaftError;

    fn try_from(value: ServerConfigView<'a>) -> Result<Self, Self::Error> {
        Ok(MembershipEntry {
            server_id: value.id(),
            dc_id: value.dc_id(),
            endpoint: value.endpoint()?,
            aux: value.aux()?,
            is_learner: value.is_learner(),
            is_new_joiner: value.is_new_joiner(),
            priority: value.priority(),
        })
    }
}

/// Snapshot of cluster membership for external observers.
#[derive(Debug, Clone)]
pub struct ClusterMembershipSnapshot {
    log_idx: LogIndex,
    prev_log_idx: LogIndex,
    entries: Vec<MembershipEntry>,
}

impl ClusterMembershipSnapshot {
    /// Capture a snapshot from a live cluster configuration view.
    pub fn from_view(view: ClusterConfigView<'_>) -> RaftResult<Self> {
        let mut entries = Vec::with_capacity(view.servers_size());
        for server in view.servers() {
            entries.push(MembershipEntry::try_from(server)?);
        }
        Ok(Self {
            log_idx: view.log_idx(),
            prev_log_idx: view.prev_log_idx(),
            entries,
        })
    }

    /// Construct a snapshot from explicit values (useful in tests).
    pub fn new(log_idx: LogIndex, prev_log_idx: LogIndex, entries: Vec<MembershipEntry>) -> Self {
        Self {
            log_idx,
            prev_log_idx,
            entries,
        }
    }

    pub fn log_idx(&self) -> LogIndex {
        self.log_idx
    }

    pub fn prev_log_idx(&self) -> LogIndex {
        self.prev_log_idx
    }

    pub fn entries(&self) -> &[MembershipEntry] {
        &self.entries
    }

    /// Compute differences compared to a newer snapshot.
    pub fn diff(&self, newer: &Self) -> MembershipChangeSet {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut updated = Vec::new();

        let mut current_index: HashMap<ServerId, &MembershipEntry> =
            HashMap::with_capacity(self.entries.len());
        for entry in &self.entries {
            current_index.insert(entry.server_id, entry);
        }

        let mut next_index: HashMap<ServerId, &MembershipEntry> =
            HashMap::with_capacity(newer.entries.len());
        for entry in &newer.entries {
            if let Some(prev) = current_index.get(&entry.server_id) {
                if *prev != entry {
                    updated.push(MembershipDelta {
                        before: (*prev).clone(),
                        after: entry.clone(),
                    });
                }
            } else {
                added.push(entry.clone());
            }
            next_index.insert(entry.server_id, entry);
        }

        for entry in &self.entries {
            if !next_index.contains_key(&entry.server_id) {
                removed.push(entry.clone());
            }
        }

        MembershipChangeSet {
            added,
            removed,
            updated,
        }
    }
}

/// Describes membership changes between two snapshots.
#[derive(Debug, Default, Clone)]
pub struct MembershipChangeSet {
    pub added: Vec<MembershipEntry>,
    pub removed: Vec<MembershipEntry>,
    pub updated: Vec<MembershipDelta>,
}

impl MembershipChangeSet {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.updated.is_empty()
    }
}

/// A single membership update between snapshots.
#[derive(Debug, Clone)]
pub struct MembershipDelta {
    pub before: MembershipEntry,
    pub after: MembershipEntry,
}

/// Raft parameters configuration.
pub struct RaftParams {
    ptr: NonNull<bop_raft_params>,
}

impl RaftParams {
    /// Create new Raft parameters with defaults.
    pub fn new() -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_params_make() };
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(RaftParams { ptr })
    }

    /// Get the raw pointer (for C API interop).
    pub fn as_ptr(&self) -> *mut bop_raft_params {
        self.ptr.as_ptr()
    }

    fn raw(&self) -> &bop_raft_params {
        unsafe { self.ptr.as_ref() }
    }

    fn raw_mut(&mut self) -> &mut bop_raft_params {
        unsafe { self.ptr.as_mut() }
    }

    /// Current election timeout bounds `(lower, upper)` in milliseconds.
    pub fn election_timeout(&self) -> (i32, i32) {
        let raw = self.raw();
        (
            raw.election_timeout_lower_bound,
            raw.election_timeout_upper_bound,
        )
    }

    /// Update the election timeout bounds in milliseconds.
    pub fn set_election_timeout(&mut self, lower: i32, upper: i32) {
        let raw = self.raw_mut();
        raw.election_timeout_lower_bound = lower;
        raw.election_timeout_upper_bound = upper;
    }

    /// Heartbeat interval in milliseconds.
    pub fn heart_beat_interval(&self) -> i32 {
        self.raw().heart_beat_interval
    }

    /// Set the heartbeat interval in milliseconds.
    pub fn set_heart_beat_interval(&mut self, interval: i32) {
        self.raw_mut().heart_beat_interval = interval;
    }

    /// Backoff duration in milliseconds after RPC failures.
    pub fn rpc_failure_backoff(&self) -> i32 {
        self.raw().rpc_failure_backoff
    }

    /// Set the RPC failure backoff in milliseconds.
    pub fn set_rpc_failure_backoff(&mut self, backoff: i32) {
        self.raw_mut().rpc_failure_backoff = backoff;
    }

    /// Check whether parallel log appending is enabled.
    pub fn parallel_log_appending(&self) -> bool {
        self.raw().parallel_log_appending
    }

    /// Enable or disable parallel log appending.
    pub fn set_parallel_log_appending(&mut self, enable: bool) {
        self.raw_mut().parallel_log_appending = enable;
    }

    /// Execute a closure with read-only access to the raw params struct.
    pub fn with_raw<R>(&self, f: impl FnOnce(&bop_raft_params) -> R) -> R {
        f(self.raw())
    }

    /// Execute a closure with mutable access to the raw params struct.
    pub fn with_raw_mut<R>(&mut self, f: impl FnOnce(&mut bop_raft_params) -> R) -> R {
        f(self.raw_mut())
    }

    /// Copy values from another instance while preserving allocation.
    pub fn copy_from(&mut self, other: &RaftParams) {
        unsafe {
            ptr::copy_nonoverlapping(other.as_ptr(), self.as_ptr(), 1);
        }
    }
}

impl Clone for RaftParams {
    fn clone(&self) -> Self {
        let mut cloned = RaftParams::new().expect("Failed to allocate RaftParams clone");
        cloned.copy_from(self);
        cloned
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
