mod archive;
mod db;
mod flush;
mod io;
#[cfg(feature = "libsql")]
pub mod libsql;
mod manager;
mod manifest;
mod page_cache;
mod wal;

pub use archive::Archive;
pub use db::{DB, DbConfig, DbDiagnostics, DbError, DbId};
#[cfg(any(unix, target_os = "windows"))]
pub use io::{DirectIoBuffer, DirectIoDriver};
pub use io::{
    IoBackendKind, IoDriver, IoError, IoFile, IoOpenOptions, IoRegistry, IoResult, IoVec, IoVecMut,
    SharedIoDriver,
};
#[cfg(feature = "libsql")]
pub use libsql::{
    LibsqlVfs, LibsqlVfsBuilder, LibsqlVfsConfig, LibsqlVfsError, LibsqlVirtualWal,
    LibsqlVirtualWalError, LibsqlWalHook, LibsqlWalHookError, VirtualWalConfig,
};
pub use manager::{Manager, ManagerClosedError, ManagerDiagnostics, ManagerError};
pub use manifest::Manifest;
pub use page_cache::{
    PageCache, PageCacheConfig, PageCacheKey, PageCacheMetricsSnapshot, PageCacheNamespace,
    PageCacheObserver, PageFrame, allocate_cache_object_id,
};
pub use wal::{Wal, WalDiagnostics};
