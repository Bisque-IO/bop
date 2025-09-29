use std::env;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use thiserror::Error;

use crate::flush::{
    FlushController, FlushControllerConfig, FlushControllerSnapshot, FlushScheduleError, FlushSink,
};
use crate::io::{IoBackendKind, SharedIoDriver};
use crate::manager::ManagerInner;
use crate::manifest::{Manifest, ManifestFlushSink};
use crate::page_cache::{
    PageCache, PageCacheKey, PageCacheMetricsSnapshot, PageCacheNamespace, allocate_cache_object_id,
};
use crate::runtime::StorageRuntime;
use crate::wal::{Wal, WalDiagnostics, WalSegment};
use crate::write::{
    WriteController, WriteControllerConfig, WriteControllerSnapshot, WriteScheduleError,
};

/// Identifier for a storage pod/database instance managed by `Manager`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DbId(u32);

impl DbId {
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

impl fmt::Display for DbId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for DbId {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl From<DbId> for u32 {
    fn from(value: DbId) -> Self {
        value.get()
    }
}

impl From<DbId> for u64 {
    fn from(value: DbId) -> Self {
        value.as_u64()
    }
}

/// Configuration used when constructing a new storage pod.
#[derive(Debug, Clone)]
pub struct DbConfig {
    id: Option<DbId>,
    name: String,
    data_dir: PathBuf,
    wal_dir: PathBuf,
    cache_capacity_bytes: Option<usize>,
    io_backend: Option<IoBackendKind>,
}

impl DbConfig {
    pub fn builder(name: impl Into<String>) -> DbConfigBuilder {
        DbConfigBuilder::new(name.into())
    }

    pub fn id(&self) -> Option<DbId> {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
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

    pub(crate) fn assign_id(&mut self, id: DbId) {
        self.id = Some(id);
    }
}

pub struct DbConfigBuilder {
    id: Option<DbId>,
    name: String,
    data_dir: Option<PathBuf>,
    wal_dir: Option<PathBuf>,
    cache_capacity_bytes: Option<usize>,
    io_backend: Option<IoBackendKind>,
}

impl DbConfigBuilder {
    fn new(name: String) -> Self {
        Self {
            id: None,
            name,
            data_dir: None,
            wal_dir: None,
            cache_capacity_bytes: None,
            io_backend: None,
        }
    }

    pub fn id(mut self, id: DbId) -> Self {
        self.id = Some(id);
        self
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
        let base = default_base_dir(&self.name);
        let data_dir = self.data_dir.unwrap_or_else(|| base.join("data"));
        let wal_dir = self.wal_dir.unwrap_or_else(|| data_dir.join("wal"));

        DbConfig {
            id: self.id,
            name: self.name,
            data_dir,
            wal_dir,
            cache_capacity_bytes: self.cache_capacity_bytes,
            io_backend: self.io_backend,
        }
    }
}

fn default_base_dir(name: &str) -> PathBuf {
    env::temp_dir().join(format!("bop-storage-{}", slugify(name)))
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    if slug.is_empty() {
        slug.push('d');
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "db".to_string()
    } else {
        trimmed.to_string()
    }
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

    pub fn id(&self) -> DbId {
        self.inner.id
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn checkpoint(&self) -> Result<(), DbError> {
        self.inner.checkpoint()
    }

    pub fn enqueue_write(&self, segment: Arc<WalSegment>) -> Result<(), WriteScheduleError> {
        self.inner.enqueue_write(segment)
    }

    pub fn enqueue_flush(&self, segment: Arc<WalSegment>) -> Result<(), FlushScheduleError> {
        self.inner.enqueue_flush(segment)
    }

    pub fn close(&self) {
        self.inner.close();
    }

    pub fn diagnostics(&self) -> DbDiagnostics {
        self.inner.diagnostics()
    }

    #[allow(dead_code)]
    pub(crate) fn runtime(&self) -> Arc<StorageRuntime> {
        self.inner.runtime()
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
    pub name: String,
    pub io_backend: IoBackendKind,
    pub is_closed: bool,
    pub wal: WalDiagnostics,
    pub page_store: PageStoreMetrics,
    pub cache: PageCacheMetricsSnapshot,
    pub write_controller: WriteControllerSnapshot,
    pub flush_controller: FlushControllerSnapshot,
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
    name: String,
    _config: DbConfig,
    io: SharedIoDriver,
    wal: Wal,
    page_store: PageStore,
    write_controller: WriteController,
    flush_controller: FlushController,
    #[allow(dead_code)]
    runtime: Arc<StorageRuntime>,
    manager: Weak<ManagerInner>,
    closed: AtomicBool,
    deregistered: AtomicBool,
}

impl DBInner {
    pub(crate) fn bootstrap(
        config: DbConfig,
        io: SharedIoDriver,
        manager: Arc<ManagerInner>,
        runtime: Arc<StorageRuntime>,
        manifest: Arc<Manifest>,
    ) -> Arc<Self> {
        let id = config
            .id()
            .expect("database id must be assigned before bootstrap");
        let name = config.name().to_string();
        let wal = Wal::new();
        let page_store = PageStore::new(manager.page_cache());
        let write_controller =
            WriteController::new(runtime.clone(), WriteControllerConfig::default());
        let sink: Arc<dyn FlushSink> = Arc::new(ManifestFlushSink::new(manifest, id.get()));
        let flush_controller =
            FlushController::new(runtime.clone(), sink, FlushControllerConfig::default());

        Arc::new(Self {
            id,
            name,
            _config: config,
            io,
            wal,
            page_store,
            write_controller,
            flush_controller,
            runtime,
            manager: Arc::downgrade(&manager),
            closed: AtomicBool::new(false),
            deregistered: AtomicBool::new(false),
        })
    }

    #[allow(dead_code)]
    pub(crate) fn runtime(&self) -> Arc<StorageRuntime> {
        self.runtime.clone()
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
        self.shutdown_controllers();
        self.notify_manager();
        true
    }

    pub fn diagnostics(&self) -> DbDiagnostics {
        DbDiagnostics {
            id: self.id,
            name: self.name.clone(),
            io_backend: self.io_backend(),
            is_closed: self.is_closed(),
            wal: self.wal.diagnostics(),
            page_store: self.page_store.metrics(),
            cache: self.page_store.cache_snapshot(),
            write_controller: self.write_controller.snapshot(),
            flush_controller: self.flush_controller.snapshot(),
        }
    }

    pub fn io_backend(&self) -> IoBackendKind {
        self.io.backend()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn enqueue_write(&self, segment: Arc<WalSegment>) -> Result<(), WriteScheduleError> {
        self.write_controller.enqueue(segment)
    }

    pub fn enqueue_flush(&self, segment: Arc<WalSegment>) -> Result<(), FlushScheduleError> {
        self.flush_controller.enqueue(segment)
    }

    fn notify_manager(&self) {
        if self.deregistered.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(manager) = self.manager.upgrade() {
            manager.deregister_pod(&self.id);
        }
    }

    fn shutdown_controllers(&self) {
        self.write_controller.shutdown();
        self.flush_controller.shutdown();
    }
}

impl Drop for DBInner {
    fn drop(&mut self) {
        self.shutdown_controllers();
        self.closed.store(true, Ordering::SeqCst);
        if !self.deregistered.swap(true, Ordering::SeqCst) {
            if let Some(manager) = self.manager.upgrade() {
                manager.deregister_pod(&self.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WriteChunk;
    use crate::io::{IoFile, IoResult, IoVec, IoVecMut};
    use crate::manager::Manager;
    use crate::manifest::{Manifest, ManifestOptions};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::TempDir;

    #[derive(Debug, Default)]
    struct RecordingIoFile {
        writes: Mutex<Vec<(u64, Vec<u8>)>>,
        flushes: AtomicUsize,
    }

    impl RecordingIoFile {
        fn bytes_written(&self) -> usize {
            let guard = self.writes.lock().unwrap();
            guard.iter().map(|(_, data)| data.len()).sum()
        }

        fn flush_count(&self) -> usize {
            self.flushes.load(Ordering::SeqCst)
        }
    }

    impl IoFile for RecordingIoFile {
        fn readv_at(&self, _offset: u64, _bufs: &mut [IoVecMut<'_>]) -> IoResult<usize> {
            Ok(0)
        }

        fn writev_at(&self, offset: u64, bufs: &[IoVec<'_>]) -> IoResult<usize> {
            let mut payload = Vec::new();
            for buf in bufs {
                payload.extend_from_slice(buf.as_slice());
            }
            let len = payload.len();
            self.writes.lock().unwrap().push((offset, payload));
            Ok(len)
        }

        fn allocate(&self, _offset: u64, _len: u64) -> IoResult<()> {
            Ok(())
        }

        fn flush(&self) -> IoResult<()> {
            self.flushes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn manager_with_manifest() -> (Manager, Arc<Manifest>, TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest =
            Arc::new(Manifest::open(dir.path(), ManifestOptions::default()).expect("manifest"));
        let manager = Manager::new(manifest.clone());
        (manager, manifest, dir)
    }

    fn wait_for<F>(predicate: F, timeout: Duration)
    where
        F: Fn() -> bool,
    {
        let start = std::time::Instant::now();
        while !predicate() {
            if start.elapsed() > timeout {
                panic!("condition not met within {:?}", timeout);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn enqueue_write_dispatches_batches() {
        let (manager, _manifest, _guard) = manager_with_manifest();
        let config = DbConfig::builder("enqueue-write").build();
        let db = manager.open_db(config).expect("open db");

        let io = Arc::new(RecordingIoFile::default());
        let segment = Arc::new(WalSegment::new(io.clone(), 0, 1024));

        segment.reserve_pending(3).unwrap();
        segment.with_active_batch(|batch| batch.push(WriteChunk::Owned(vec![1, 2, 3])));
        segment.stage_active_batch().unwrap();

        db.enqueue_write(segment.clone()).expect("enqueue write");

        wait_for(|| segment.written_size() == 3, Duration::from_secs(1));
        assert_eq!(segment.pending_size(), 0);
        assert_eq!(io.bytes_written(), 3);

        manager.shutdown();
    }

    #[test]
    fn enqueue_flush_advances_manifest() {
        let (manager, manifest, _guard) = manager_with_manifest();
        let config = DbConfig::builder("enqueue-flush").build();
        let db = manager.open_db(config).expect("open db");
        let db_id = db.id().get();

        let io = Arc::new(RecordingIoFile::default());
        let segment = Arc::new(WalSegment::new(io.clone(), 0, 1024));

        segment.mark_written(256).unwrap();
        assert!(segment.request_flush(256).unwrap());

        db.enqueue_flush(segment.clone()).expect("enqueue flush");

        wait_for(|| segment.durable_size() == 256, Duration::from_secs(1));
        assert_eq!(io.flush_count(), 1);

        let state = manifest
            .wal_state(db_id)
            .expect("read wal state")
            .expect("wal state entry");
        assert_eq!(state.last_applied_lsn, 256);
        assert!(state.flush_gate_state.last_success_at_epoch_ms.is_some());

        manager.shutdown();
    }
}
