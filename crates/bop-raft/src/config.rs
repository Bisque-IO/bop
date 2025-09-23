use std::ffi::CString;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::slice;

use bop_sys::*;

use crate::buffer::Buffer;
use crate::error::{RaftError, RaftResult};
use crate::types::{DataCenterId, LogIndex, ServerId};

/// Server configuration for a Raft node.
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

    /// Expose the raw pointer for advanced interop.
    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *const bop_raft_cluster_config {
        self.ptr
    }
}

unsafe impl<'a> Send for ClusterConfigView<'a> {}
unsafe impl<'a> Sync for ClusterConfigView<'a> {}

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
