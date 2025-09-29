use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};

use crossfire::{MTx, Rx, mpsc};
use dashmap::{DashMap, mapref::entry::Entry};
use thiserror::Error;

use crate::db::{DB, DBInner, DbConfig, DbDiagnostics, DbId};
use crate::flush::FlushControllerSnapshot;
use crate::io::{IoBackendKind, IoError, IoRegistry, IoResult, SharedIoDriver};
use crate::manifest::{Manifest, ManifestError};
use crate::page_cache::{PageCache, PageCacheConfig, PageCacheKey, PageCacheMetricsSnapshot};
use crate::runtime::{StorageRuntime, StorageRuntimeOptions};
use crate::write::WriteControllerSnapshot;

const DEFAULT_QUEUE_CAPACITY: usize = 64;

type ManagerJob = Box<dyn FnOnce() + Send + 'static>;

enum ManagerCommand {
    Execute(ManagerJob),
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagerClosedError;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("manager is closed")]
    Closed,
    #[error("database already open: {0}")]
    DbAlreadyExists(DbId),
    #[error("database not found: {0}")]
    DbNotFound(DbId),
    #[error(transparent)]
    Io(#[from] IoError),
}

#[derive(Debug, Clone)]
pub struct ControllerDiagnostics {
    pub db_id: DbId,
    pub write: WriteControllerSnapshot,
    pub flush: FlushControllerSnapshot,
}

#[derive(Debug, Clone)]
pub struct ManagerDiagnostics {
    pub active_pods: usize,
    pub jobs_executed: u64,
    pub page_cache: PageCacheMetricsSnapshot,
    pub default_io_backend: IoBackendKind,
    pub pods: Vec<DbDiagnostics>,
    pub controllers: Vec<ControllerDiagnostics>,
}

#[derive(Clone)]
pub struct Manager {
    pub(crate) inner: Arc<ManagerInner>,
}

pub(crate) struct ManagerInner {
    sender: MTx<ManagerCommand>,
    worker: Mutex<Option<JoinHandle<()>>>,
    pods: DashMap<DbId, Arc<DBInner>>,
    closed: AtomicBool,
    jobs_executed: AtomicU64,
    page_cache: Arc<PageCache<PageCacheKey>>,
    io_registry: IoRegistry,
    runtime: Arc<StorageRuntime>,
    manifest: Arc<Manifest>,
    next_db_id: AtomicU32,
}

impl Manager {
    /// Create a manager using the provided manifest handle with the default queue capacity
    /// and cache configuration.
    pub fn new(manifest: Arc<Manifest>) -> Self {
        Self::with_capacity(manifest, DEFAULT_QUEUE_CAPACITY)
    }

    /// Create a manager using the provided manifest handle and queue capacity.
    pub fn with_capacity(manifest: Arc<Manifest>, capacity: usize) -> Self {
        Self::with_capacity_and_cache(manifest, capacity, PageCacheConfig::default())
    }

    /// Create a manager with explicit queue capacity, cache configuration, and shared manifest.
    pub fn with_capacity_and_cache(
        manifest: Arc<Manifest>,
        capacity: usize,
        cache_config: PageCacheConfig,
    ) -> Self {
        Self::with_capacity_cache_backend(manifest, capacity, cache_config, IoBackendKind::Std)
    }

    /// Create a manager with explicit queue, cache, manifest, and default I/O backend configuration.
    pub fn with_capacity_cache_backend(
        manifest: Arc<Manifest>,
        capacity: usize,
        cache_config: PageCacheConfig,
        default_backend: IoBackendKind,
    ) -> Self {
        let (sender, receiver) = mpsc::bounded_blocking(capacity.max(1));
        let runtime = StorageRuntime::create(StorageRuntimeOptions::default())
            .expect("failed to initialize storage runtime");
        let inner = Arc::new(ManagerInner::new(
            sender,
            cache_config,
            default_backend,
            runtime,
            manifest,
        ));
        let worker = spawn_worker(receiver, Arc::downgrade(&inner));
        *inner.worker.lock().expect("manager worker mutex poisoned") = Some(worker);
        Self { inner }
    }

    /// Return the default I/O backend used when configurations do not specify one.
    pub fn default_io_backend(&self) -> IoBackendKind {
        self.inner.default_io_backend()
    }

    /// Submit a job to be executed on the worker thread.
    pub fn submit<F>(&self, job: F) -> Result<(), ManagerClosedError>
    where
        F: FnOnce() + Send + 'static,
    {
        self.inner.submit(job)
    }

    /// Open or register a storage pod described by `config`.
    pub fn open_db(&self, mut config: DbConfig) -> Result<DB, ManagerError> {
        if self.inner.is_closed() {
            return Err(ManagerError::Closed);
        }

        let db_id = if let Some(id) = config.id() {
            self.inner.observe_explicit_db_id(id);
            id
        } else {
            let id = self.inner.allocate_db_id();
            config.assign_id(id);
            id
        };

        match self.inner.pods.entry(db_id) {
            Entry::Occupied(_) => Err(ManagerError::DbAlreadyExists(db_id)),
            Entry::Vacant(entry) => {
                let driver = self.inner.resolve_io(config.io_backend())?;
                let pod = DBInner::bootstrap(
                    config,
                    driver,
                    self.inner.clone(),
                    self.inner.runtime(),
                    self.inner.manifest.clone(),
                );
                let handle = DB::from_arc(pod.clone());
                entry.insert(pod);
                Ok(handle)
            }
        }
    }

    /// Fetch an already-opened pod by identifier.
    pub fn get_db(&self, id: &DbId) -> Option<DB> {
        self.inner
            .pods
            .get(id)
            .map(|pod| DB::from_arc(pod.value().clone()))
    }

    /// Close a pod and remove it from the registry.
    pub fn close_db(&self, id: &DbId) -> Result<(), ManagerError> {
        let removed = self
            .inner
            .pods
            .remove(id)
            .ok_or_else(|| ManagerError::DbNotFound(id.clone()))?;
        let (_, pod) = removed;
        pod.close();
        Ok(())
    }

    /// Shutdown the worker thread and deregister all pods.
    pub fn shutdown(&self) {
        self.inner.request_shutdown();
        self.inner.join_worker();
        self.inner.clear_pods();
        self.inner.shutdown_runtime();
    }

    /// Produce a diagnostic snapshot of the manager and managed pods.
    pub fn diagnostics(&self) -> ManagerDiagnostics {
        let mut pods: Vec<_> = self
            .inner
            .pods
            .iter()
            .map(|entry| entry.value().diagnostics())
            .collect();
        pods.sort_by(|a, b| a.id.cmp(&b.id));

        let controllers = pods
            .iter()
            .map(|pod| ControllerDiagnostics {
                db_id: pod.id,
                write: pod.write_controller.clone(),
                flush: pod.flush_controller.clone(),
            })
            .collect();

        ManagerDiagnostics {
            active_pods: pods.len(),
            jobs_executed: self.inner.jobs_executed(),
            page_cache: self.inner.page_cache.metrics(),
            default_io_backend: self.inner.default_io_backend(),
            pods,
            controllers,
        }
    }

    /// Seed the database id allocator from the manifest's current maximum id.
    pub fn seed_db_ids_from_manifest(&self, manifest: &Manifest) -> Result<(), ManifestError> {
        if let Some(max_id) = manifest.max_db_id()? {
            self.inner.observe_explicit_db_id(DbId::new(max_id));
        }
        Ok(())
    }
}

impl ManagerInner {
    fn new(
        sender: MTx<ManagerCommand>,
        cache_config: PageCacheConfig,
        default_backend: IoBackendKind,
        runtime: Arc<StorageRuntime>,
        manifest: Arc<Manifest>,
    ) -> Self {
        Self {
            sender,
            worker: Mutex::new(None),
            pods: DashMap::new(),
            closed: AtomicBool::new(false),
            jobs_executed: AtomicU64::new(0),
            page_cache: Arc::new(PageCache::new(cache_config)),
            io_registry: IoRegistry::new(default_backend),
            runtime,
            manifest,
            next_db_id: AtomicU32::new(0),
        }
    }

    fn submit<F>(&self, job: F) -> Result<(), ManagerClosedError>
    where
        F: FnOnce() + Send + 'static,
    {
        if self.is_closed() {
            return Err(ManagerClosedError);
        }

        self.sender
            .send(ManagerCommand::Execute(Box::new(job)))
            .map_err(|_| ManagerClosedError)
    }

    fn resolve_io(&self, requested: Option<IoBackendKind>) -> IoResult<SharedIoDriver> {
        self.io_registry.resolve(requested)
    }

    fn default_io_backend(&self) -> IoBackendKind {
        self.io_registry.default_backend()
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn request_shutdown(&self) -> bool {
        if self.closed.swap(true, Ordering::SeqCst) {
            return false;
        }

        let _ = self.sender.send(ManagerCommand::Shutdown);
        true
    }

    fn join_worker(&self) {
        if let Ok(mut guard) = self.worker.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }

    fn clear_pods(&self) {
        let keys: Vec<DbId> = self.pods.iter().map(|entry| entry.key().clone()).collect();
        for key in keys {
            if let Some((_, pod)) = self.pods.remove(&key) {
                pod.close();
            }
        }
    }

    fn shutdown_runtime(&self) {
        self.runtime.shutdown();
    }

    pub(crate) fn page_cache(&self) -> Arc<PageCache<PageCacheKey>> {
        self.page_cache.clone()
    }

    pub(crate) fn runtime(&self) -> Arc<StorageRuntime> {
        self.runtime.clone()
    }

    fn jobs_executed(&self) -> u64 {
        self.jobs_executed.load(Ordering::Relaxed)
    }

    fn mark_job_executed(&self) {
        self.jobs_executed.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn deregister_pod(&self, id: &DbId) {
        self.pods.remove(id);
    }

    fn allocate_db_id(&self) -> DbId {
        let next = self
            .next_db_id
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        DbId::new(next)
    }

    fn observe_explicit_db_id(&self, id: DbId) {
        let mut current = self.next_db_id.load(Ordering::SeqCst);
        let target = id.get();
        while target > current {
            match self.next_db_id.compare_exchange(
                current,
                target,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }
}
impl Drop for ManagerInner {
    fn drop(&mut self) {
        let _ = self.request_shutdown();
        self.join_worker();
        self.clear_pods();
        self.shutdown_runtime();
    }
}

fn spawn_worker(receiver: Rx<ManagerCommand>, inner: Weak<ManagerInner>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("bop-storage-manager".into())
        .spawn(move || {
            loop {
                match receiver.recv() {
                    Ok(ManagerCommand::Execute(job)) => {
                        job();
                        if let Some(inner) = inner.upgrade() {
                            inner.mark_job_executed();
                        }
                    }
                    Ok(ManagerCommand::Shutdown) | Err(_) => break,
                }
            }
        })
        .expect("failed to spawn storage manager thread")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DbConfig, DbId};
    use crate::manifest::{Manifest, ManifestOptions};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    fn test_manifest() -> (Arc<Manifest>, TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest =
            Arc::new(Manifest::open(dir.path(), ManifestOptions::default()).expect("manifest"));
        (manifest, dir)
    }

    fn test_manager() -> (Manager, TempDir) {
        let (manifest, dir) = test_manifest();
        (Manager::new(manifest), dir)
    }

    #[test]
    fn open_and_fetch_pod() {
        let (manager, _guard) = test_manager();
        let config = DbConfig::builder("pod-open").build();
        let db = manager.open_db(config).expect("open");
        let db_id = db.id();
        assert_eq!(manager.default_io_backend(), IoBackendKind::Std);
        let fetched = manager.get_db(&db_id).expect("missing pod");
        assert_eq!(fetched.id(), db_id);
        assert_eq!(fetched.name(), "pod-open");
        assert!(!fetched.runtime().is_shutdown());
        assert_eq!(fetched.io_backend(), IoBackendKind::Std);
        manager.shutdown();
    }

    #[test]
    fn duplicate_open_returns_error() {
        let (manager, _guard) = test_manager();
        let config = DbConfig::builder("dup").id(DbId::new(42)).build();
        manager.open_db(config.clone()).expect("first open");
        let result = manager.open_db(config);
        match result {
            Err(ManagerError::DbAlreadyExists(id)) => assert_eq!(id.get(), 42),
            Err(other) => panic!("expected duplicate error, got {other:?}"),
            Ok(_) => panic!("expected duplicate error, got Ok"),
        }
        manager.shutdown();
    }

    #[test]
    fn shutdown_deregisters_pods() {
        let (manager, _guard) = test_manager();
        let config = DbConfig::builder("shutdown").build();
        let db = manager.open_db(config).expect("open");
        let db_id = db.id();
        manager.shutdown();
        assert!(manager.get_db(&db_id).is_none());
    }

    #[test]
    fn submit_job_can_access_pod() {
        let (manager, _guard) = test_manager();
        let config = DbConfig::builder("job").build();
        let db = manager.open_db(config).expect("open");
        let db_id = db.id();

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let handle = manager.clone();

        manager
            .submit(move || {
                let pod = handle.get_db(&db_id).expect("pod missing");
                pod.checkpoint().expect("checkpoint");
                counter_clone.fetch_add(1, Ordering::SeqCst);
            })
            .expect("submit");

        manager.shutdown();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn requesting_unavailable_backend_returns_error() {
        let (manager, _guard) = test_manager();
        let config = DbConfig::builder("io-uring")
            .io_backend(IoBackendKind::IoUring)
            .build();

        match manager.open_db(config) {
            Err(ManagerError::Io(IoError::BackendUnavailable { backend })) => {
                assert_eq!(backend, IoBackendKind::IoUring);
            }
            Err(other) => panic!("expected backend unavailable error, got {other:?}"),
            Ok(_) => panic!("expected backend unavailable error, got Ok"),
        }

        manager.shutdown();
    }

    #[test]
    fn diagnostics_capture_pod_metrics() {
        let (manager, _guard) = test_manager();
        let config = DbConfig::builder("diag").build();
        let db = manager.open_db(config).expect("open");
        let db_id = db.id();
        assert_eq!(db.name(), "diag");

        db.checkpoint().expect("checkpoint");

        let diagnostics = manager.diagnostics();
        assert_eq!(diagnostics.active_pods, 1);
        assert_eq!(diagnostics.default_io_backend, IoBackendKind::Std);
        let pod = diagnostics
            .pods
            .iter()
            .find(|pod| pod.id == db_id)
            .expect("pod diagnostics");
        assert_eq!(pod.io_backend, IoBackendKind::Std);
        assert_eq!(pod.name, "diag");
        assert_eq!(pod.wal.last_sequence, 1);
        assert!(pod.page_store.cache_object_id > 0);
        assert_eq!(diagnostics.controllers.len(), 1);
        let controller = diagnostics
            .controllers
            .iter()
            .find(|c| c.db_id == db_id)
            .expect("controller diagnostics");
        assert_eq!(
            controller.write.pending_queue_depth,
            pod.write_controller.pending_queue_depth
        );
        assert_eq!(
            controller.flush.pending_queue_depth,
            pod.flush_controller.pending_queue_depth
        );

        drop(db);
        manager.shutdown();
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn diagnostics_report_direct_backend() {
        let (manifest, _manifest_guard) = test_manifest();
        let manager = Manager::with_capacity_cache_backend(
            manifest,
            DEFAULT_QUEUE_CAPACITY,
            PageCacheConfig::default(),
            IoBackendKind::DirectIo,
        );
        let config = DbConfig::builder("direct-diag")
            .io_backend(IoBackendKind::DirectIo)
            .build();

        let db = match manager.open_db(config) {
            Ok(db) => db,
            Err(ManagerError::Io(IoError::BackendUnavailable { .. })) => {
                manager.shutdown();
                return;
            }
            Err(other) => panic!("expected direct backend to open, got {other:?}"),
        };

        let db_id = db.id();
        assert_eq!(manager.default_io_backend(), IoBackendKind::DirectIo);
        assert_eq!(db.io_backend(), IoBackendKind::DirectIo);
        assert_eq!(db.name(), "direct-diag");

        let diagnostics = manager.diagnostics();
        assert_eq!(diagnostics.default_io_backend, IoBackendKind::DirectIo);
        let pod = diagnostics
            .pods
            .iter()
            .find(|pod| pod.id == db_id)
            .expect("pod diagnostics");
        assert_eq!(pod.io_backend, IoBackendKind::DirectIo);
        assert_eq!(pod.name, "direct-diag");

        drop(db);
        manager.shutdown();
    }

    #[test]
    fn close_db_removes_pod() {
        let (manager, _guard) = test_manager();
        let config = DbConfig::builder("close").build();
        let db = manager.open_db(config).expect("open");
        let db_id = db.id();
        manager.close_db(&db_id).expect("close");
        assert!(manager.get_db(&db_id).is_none());
        manager.shutdown();
    }

    #[test]
    fn seed_db_ids_from_manifest_uses_max_id() {
        use crate::manifest::{DbDescriptorRecord, ManifestOptions};

        let (manager, _guard) = test_manager();
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).expect("manifest");

        let mut txn = manifest.begin();
        txn.put_db(3, DbDescriptorRecord::default());
        txn.put_db(7, DbDescriptorRecord::default());
        txn.commit().expect("commit");

        manager
            .seed_db_ids_from_manifest(&manifest)
            .expect("seed ids");

        let db = manager
            .open_db(DbConfig::builder("seeded").build())
            .expect("open");

        assert_eq!(db.id().get(), 8);
        manager.shutdown();
    }
}
