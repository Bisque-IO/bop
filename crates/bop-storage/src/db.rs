use std::env;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use thiserror::Error;

use crate::io::{IoBackendKind, SharedIoDriver};
use crate::manager::ManagerInner;
use crate::page_cache::{
    PageCache, PageCacheKey, PageCacheMetricsSnapshot, PageCacheNamespace, allocate_cache_object_id,
};
use crate::wal::{Wal, WalDiagnostics};

/// Identifier for a storage pod/database instance managed by `Manager`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DbId(String);

impl DbId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn slug(&self) -> String {
        self.0
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect()
    }
}

impl fmt::Display for DbId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for DbId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DbId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Configuration used when constructing a new storage pod.
#[derive(Debug, Clone)]
pub struct DbConfig {
    id: DbId,
    data_dir: PathBuf,
    wal_dir: PathBuf,
    cache_capacity_bytes: Option<usize>,
    io_backend: Option<IoBackendKind>,
}

impl DbConfig {
    pub fn builder(id: impl Into<DbId>) -> DbConfigBuilder {
        DbConfigBuilder::new(id.into())
    }

    pub fn id(&self) -> &DbId {
        &self.id
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub fn wal_dir(&self) -> &PathBuf {
        &self.wal_dir
    }

    pub fn cache_capacity_bytes(&self) -> Option<usize> {
        self.cache_capacity_bytes
    }

    pub fn io_backend(&self) -> Option<IoBackendKind> {
        self.io_backend
    }
}

pub struct DbConfigBuilder {
    id: DbId,
    data_dir: Option<PathBuf>,
    wal_dir: Option<PathBuf>,
    cache_capacity_bytes: Option<usize>,
    io_backend: Option<IoBackendKind>,
}

impl DbConfigBuilder {
    fn new(id: DbId) -> Self {
        Self {
            id,
            data_dir: None,
            wal_dir: None,
            cache_capacity_bytes: None,
            io_backend: None,
        }
    }

    pub fn data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(dir.into());
        self
    }

    pub fn wal_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.wal_dir = Some(dir.into());
        self
    }

    pub fn cache_capacity_bytes(mut self, bytes: usize) -> Self {
        self.cache_capacity_bytes = Some(bytes);
        self
    }

    pub fn io_backend(mut self, backend: IoBackendKind) -> Self {
        self.io_backend = Some(backend);
        self
    }

    pub fn build(self) -> DbConfig {
        let base = default_base_dir(&self.id);
        let data_dir = self.data_dir.unwrap_or_else(|| base.join("data"));
        let wal_dir = self.wal_dir.unwrap_or_else(|| data_dir.join("wal"));

        DbConfig {
            id: self.id,
            data_dir,
            wal_dir,
            cache_capacity_bytes: self.cache_capacity_bytes,
            io_backend: self.io_backend,
        }
    }
}

fn default_base_dir(id: &DbId) -> PathBuf {
    env::temp_dir().join(format!("bop-storage-{}", id.slug()))
}

/// Top-level handle to a storage pod instance backed by `Arc<DBInner>`.
#[derive(Clone)]
pub struct DB {
    inner: Arc<DBInner>,
}

impl DB {
    pub(crate) fn from_arc(inner: Arc<DBInner>) -> Self {
        Self { inner }
    }

    pub fn id(&self) -> &DbId {
        &self.inner.id
    }

    pub fn checkpoint(&self) -> Result<(), DbError> {
        self.inner.checkpoint()
    }

    pub fn close(&self) {
        self.inner.close();
    }

    pub fn diagnostics(&self) -> DbDiagnostics {
        self.inner.diagnostics()
    }

    pub fn io_backend(&self) -> IoBackendKind {
        self.inner.io_backend()
    }
}

/// Errors produced by storage pod operations.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("database is closed")]
    Closed,
}

#[derive(Debug, Clone)]
pub struct DbDiagnostics {
    pub id: DbId,
    pub io_backend: IoBackendKind,
    pub is_closed: bool,
    pub wal: WalDiagnostics,
    pub page_store: PageStoreMetrics,
    pub cache: PageCacheMetricsSnapshot,
}

#[derive(Debug, Clone)]
pub struct PageStoreMetrics {
    pub cache_object_id: u64,
    pub cached_pages: u64,
}

#[derive(Debug)]
struct PageStore {
    cache: Arc<PageCache<PageCacheKey>>,
    _namespace: PageCacheNamespace,
    object_id: u64,
    cached_pages: AtomicU64,
}

impl PageStore {
    fn new(cache: Arc<PageCache<PageCacheKey>>) -> Self {
        Self {
            cache,
            _namespace: PageCacheNamespace::StoragePod,
            object_id: allocate_cache_object_id(),
            cached_pages: AtomicU64::new(0),
        }
    }

    fn metrics(&self) -> PageStoreMetrics {
        PageStoreMetrics {
            cache_object_id: self.object_id,
            cached_pages: self.cached_pages.load(Ordering::Relaxed),
        }
    }

    fn cache_snapshot(&self) -> PageCacheMetricsSnapshot {
        self.cache.metrics()
    }

    #[allow(dead_code)]
    fn cache_key(&self, page_no: u64) -> PageCacheKey {
        PageCacheKey::storage_pod(self.object_id, page_no)
    }
}

pub(crate) struct DBInner {
    id: DbId,
    _config: DbConfig,
    io: SharedIoDriver,
    wal: Wal,
    page_store: PageStore,
    manager: Weak<ManagerInner>,
    closed: AtomicBool,
    deregistered: AtomicBool,
}

impl DBInner {
    pub(crate) fn bootstrap(
        config: DbConfig,
        io: SharedIoDriver,
        manager: Arc<ManagerInner>,
    ) -> Arc<Self> {
        let id = config.id().clone();
        let wal = Wal::new();
        let page_store = PageStore::new(manager.page_cache());

        Arc::new(Self {
            id,
            _config: config,
            io,
            wal,
            page_store,
            manager: Arc::downgrade(&manager),
            closed: AtomicBool::new(false),
            deregistered: AtomicBool::new(false),
        })
    }

    pub fn checkpoint(&self) -> Result<(), DbError> {
        if self.is_closed() {
            return Err(DbError::Closed);
        }
        let next = self.wal.last_sequence().saturating_add(1);
        self.wal.mark_progress(next);
        Ok(())
    }

    pub fn close(&self) -> bool {
        if self.closed.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.notify_manager();
        true
    }

    pub fn diagnostics(&self) -> DbDiagnostics {
        DbDiagnostics {
            id: self.id.clone(),
            io_backend: self.io_backend(),
            is_closed: self.is_closed(),
            wal: self.wal.diagnostics(),
            page_store: self.page_store.metrics(),
            cache: self.page_store.cache_snapshot(),
        }
    }

    pub fn io_backend(&self) -> IoBackendKind {
        self.io.backend()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn notify_manager(&self) {
        if self.deregistered.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(manager) = self.manager.upgrade() {
            manager.deregister_pod(&self.id);
        }
    }
}

impl Drop for DBInner {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
        if !self.deregistered.swap(true, Ordering::SeqCst) {
            if let Some(manager) = self.manager.upgrade() {
                manager.deregister_pod(&self.id);
            }
        }
    }
}
