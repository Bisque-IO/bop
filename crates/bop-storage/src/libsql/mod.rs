#![cfg(feature = "libsql")]

//! Integration scaffolding for libsql-backed storage components.
//!
//! The module exposes building blocks required to embed libsql within the
//! storage layer, namely a custom virtual file system (VFS) and a virtual WAL
//! shim that surfaces higher-level storage events to bop.

mod vfs;
mod virtual_wal;

pub use libsql_ffi::RefCountedWalManager;
pub use vfs::{LibsqlVfs, LibsqlVfsBuilder, LibsqlVfsConfig, LibsqlVfsError};
pub use virtual_wal::{
    LibsqlVirtualWal, LibsqlVirtualWalError, LibsqlWalHook, LibsqlWalHookError, VirtualWalConfig,
};
