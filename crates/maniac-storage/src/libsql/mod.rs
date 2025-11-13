#![cfg(feature = "libsql")]

//! Integration scaffolding for libsql-backed storage components.
//!
//! The module exposes building blocks required to embed libsql within the
//! storage layer, namely a custom virtual file system (VFS) and a virtual WAL
//! shim that surfaces higher-level storage events to bop.

use std::fmt;

mod page_cache;
mod vfs;
mod virtual_wal;

pub use libsql_ffi::RefCountedWalManager;
pub use page_cache::{
    PageCache, PageCacheConfig, PageCacheKey, PageCacheMetricsSnapshot, PageCacheNamespace,
    allocate_cache_object_id,
};
pub use vfs::{LibsqlVfs, LibsqlVfsBuilder, LibsqlVfsConfig, LibsqlVfsError};
pub use virtual_wal::{
    LibsqlVirtualWal, LibsqlVirtualWalError, LibsqlWalHook, LibsqlWalHookError, VirtualWalConfig,
};

/// Identifier for a LibSQL database instance.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct LibSqlId(u32);

impl LibSqlId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn get(self) -> u32 {
        self.0
    }

    pub fn as_u64(self) -> u64 {
        self.0 as u64
    }
}

impl fmt::Display for LibSqlId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for LibSqlId {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl From<LibSqlId> for u32 {
    fn from(value: LibSqlId) -> Self {
        value.get()
    }
}

impl From<LibSqlId> for u64 {
    fn from(value: LibSqlId) -> Self {
        value.as_u64()
    }
}
