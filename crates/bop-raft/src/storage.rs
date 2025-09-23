use std::fmt;
use std::ptr::NonNull;

use bop_sys::{
    bop_raft_log_store_delete, bop_raft_log_store_ptr, bop_raft_state_mgr_delete,
    bop_raft_state_mgr_ptr,
};

use crate::error::{RaftError, RaftResult};
use crate::traits::{LogStoreInterface, StateManagerInterface};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackendKind {
    Callbacks,
    #[cfg(feature = "mdbx")]
    Mdbx,
}

impl StorageBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StorageBackendKind::Callbacks => "callbacks",
            #[cfg(feature = "mdbx")]
            StorageBackendKind::Mdbx => "mdbx",
        }
    }
}

impl fmt::Display for StorageBackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub struct RawLogStore {
    ptr: NonNull<bop_raft_log_store_ptr>,
    backend: StorageBackendKind,
}

impl RawLogStore {
    pub unsafe fn from_raw(
        ptr: *mut bop_raft_log_store_ptr,
        backend: StorageBackendKind,
    ) -> RaftResult<Self> {
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr, backend })
    }

    pub fn as_ptr(&self) -> *mut bop_raft_log_store_ptr {
        self.ptr.as_ptr()
    }

    pub fn backend(&self) -> StorageBackendKind {
        self.backend
    }
}

impl Drop for RawLogStore {
    fn drop(&mut self) {
        unsafe {
            bop_raft_log_store_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for RawLogStore {}
unsafe impl Sync for RawLogStore {}

#[derive(Debug)]
pub struct RawStateManager {
    ptr: NonNull<bop_raft_state_mgr_ptr>,
    backend: StorageBackendKind,
}

impl RawStateManager {
    pub unsafe fn from_raw(
        ptr: *mut bop_raft_state_mgr_ptr,
        backend: StorageBackendKind,
    ) -> RaftResult<Self> {
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr, backend })
    }

    pub fn as_ptr(&self) -> *mut bop_raft_state_mgr_ptr {
        self.ptr.as_ptr()
    }

    pub fn backend(&self) -> StorageBackendKind {
        self.backend
    }
}

impl Drop for RawStateManager {
    fn drop(&mut self) {
        unsafe {
            bop_raft_state_mgr_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for RawStateManager {}
unsafe impl Sync for RawStateManager {}

pub enum LogStoreBuild {
    Callbacks {
        backend: StorageBackendKind,
        store: Box<dyn LogStoreInterface>,
    },
    Raw(RawLogStore),
    #[cfg(feature = "mdbx")]
    Mdbx(crate::mdbx::MdbxLogStore),
}

pub enum StateManagerBuild {
    Callbacks {
        backend: StorageBackendKind,
        manager: Box<dyn StateManagerInterface>,
    },
    Raw(RawStateManager),
    #[cfg(feature = "mdbx")]
    Mdbx(crate::mdbx::MdbxStateManager),
}

impl LogStoreBuild {
    pub fn backend(&self) -> StorageBackendKind {
        match self {
            LogStoreBuild::Callbacks { backend, .. } => *backend,
            LogStoreBuild::Raw(raw) => raw.backend(),
            #[cfg(feature = "mdbx")]
            LogStoreBuild::Mdbx(_) => StorageBackendKind::Mdbx,
        }
    }
}

impl StateManagerBuild {
    pub fn backend(&self) -> StorageBackendKind {
        match self {
            StateManagerBuild::Callbacks { backend, .. } => *backend,
            StateManagerBuild::Raw(raw) => raw.backend(),
            #[cfg(feature = "mdbx")]
            StateManagerBuild::Mdbx(_) => StorageBackendKind::Mdbx,
        }
    }
}

pub struct StorageComponents {
    pub state_manager: StateManagerBuild,
    pub log_store: LogStoreBuild,
}

impl StorageComponents {
    pub fn new(
        state_manager: Box<dyn StateManagerInterface>,
        log_store: Box<dyn LogStoreInterface>,
    ) -> Self {
        Self {
            state_manager: StateManagerBuild::Callbacks {
                backend: state_manager.storage_backend(),
                manager: state_manager,
            },
            log_store: LogStoreBuild::Callbacks {
                backend: log_store.storage_backend(),
                store: log_store,
            },
        }
    }

    pub fn into_parts(self) -> (StateManagerBuild, LogStoreBuild) {
        (self.state_manager, self.log_store)
    }
}
