use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};

use crossfire::{MTx, Rx, mpsc};
use dashmap::{DashMap, mapref::entry::Entry};
use thiserror::Error;

use crate::db::{DB, DBInner, DbConfig, DbDiagnostics, DbId};
use crate::io::{IoBackendKind, IoError, IoRegistry, IoResult, SharedIoDriver};
use crate::page_cache::{PageCache, PageCacheConfig, PageCacheKey, PageCacheMetricsSnapshot};

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
pub struct ManagerDiagnostics {
    pub active_pods: usize,
    pub jobs_executed: u64,
    pub page_cache: PageCacheMetricsSnapshot,
    pub default_io_backend: IoBackendKind,
    pub pods: Vec<DbDiagnostics>,
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
}

impl Manager {
    /// Create a manager with the default queue capacity and cache configuration.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_QUEUE_CAPACITY)
    }

    /// Create a manager with the provided queue capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_cache(capacity, PageCacheConfig::default())
    }

    /// Create a manager with explicit queue capacity and cache configuration.
    pub fn with_capacity_and_cache(capacity: usize, cache_config: PageCacheConfig) -> Self {
        Self::with_capacity_cache_backend(capacity, cache_config, IoBackendKind::Std)
    }

    /// Create a manager with explicit queue, cache, and default I/O backend configuration.
    pub fn with_capacity_cache_backend(
        capacity: usize,
        cache_config: PageCacheConfig,
        default_backend: IoBackendKind,
    ) -> Self {
        let (sender, receiver) = mpsc::bounded_blocking(capacity.max(1));
        let inner = Arc::new(ManagerInner::new(sender, cache_config, default_backend));
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
    pub fn open_db(&self, config: DbConfig) -> Result<DB, ManagerError> {
        if self.inner.is_closed() {
            return Err(ManagerError::Closed);
        }

        let db_id = config.id().clone();
        match self.inner.pods.entry(db_id.clone()) {
            Entry::Occupied(_) => Err(ManagerError::DbAlreadyExists(db_id)),
            Entry::Vacant(entry) => {
                let driver = self.inner.resolve_io(config.io_backend())?;
                let pod = DBInner::bootstrap(config, driver, self.inner.clone());
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

        ManagerDiagnostics {
            active_pods: pods.len(),
            jobs_executed: self.inner.jobs_executed(),
            page_cache: self.inner.page_cache.metrics(),
            default_io_backend: self.inner.default_io_backend(),
            pods,
        }
    }
}

impl ManagerInner {
    fn new(
        sender: MTx<ManagerCommand>,
        cache_config: PageCacheConfig,
        default_backend: IoBackendKind,
    ) -> Self {
        Self {
            sender,
            worker: Mutex::new(None),
            pods: DashMap::new(),
            closed: AtomicBool::new(false),
            jobs_executed: AtomicU64::new(0),
            page_cache: Arc::new(PageCache::new(cache_config)),
            io_registry: IoRegistry::new(default_backend),
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

    pub(crate) fn page_cache(&self) -> Arc<PageCache<PageCacheKey>> {
        self.page_cache.clone()
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
}
impl Drop for ManagerInner {
    fn drop(&mut self) {
        let _ = self.request_shutdown();
        self.join_worker();
        self.clear_pods();
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
    use crate::db::DbConfig;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn open_and_fetch_pod() {
        let manager = Manager::new();
        let config = DbConfig::builder("pod-open").build();
        let db_id = config.id().clone();
        manager.open_db(config.clone()).expect("open");
        assert_eq!(manager.default_io_backend(), IoBackendKind::Std);
        let fetched = manager.get_db(&db_id).expect("missing pod");
        assert_eq!(fetched.id(), &db_id);
        assert_eq!(fetched.io_backend(), IoBackendKind::Std);
        manager.shutdown();
    }

    #[test]
    fn duplicate_open_returns_error() {
        let manager = Manager::new();
        let config = DbConfig::builder("dup").build();
        manager.open_db(config.clone()).expect("first open");
        let result = manager.open_db(config);
        assert!(matches!(result, Err(ManagerError::DbAlreadyExists(_))));
        manager.shutdown();
    }

    #[test]
    fn shutdown_deregisters_pods() {
        let manager = Manager::new();
        let config = DbConfig::builder("shutdown").build();
        let db_id = config.id().clone();
        manager.open_db(config).expect("open");
        manager.shutdown();
        assert!(manager.get_db(&db_id).is_none());
    }

    #[test]
    fn submit_job_can_access_pod() {
        let manager = Manager::new();
        let config = DbConfig::builder("job").build();
        let db_id = config.id().clone();
        manager.open_db(config).expect("open");

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
        let manager = Manager::new();
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
        let manager = Manager::new();
        let config = DbConfig::builder("diag").build();
        let db = manager.open_db(config).expect("open");
        let db_id = db.id().clone();

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
        assert_eq!(pod.wal.last_sequence, 1);
        assert!(pod.page_store.cache_object_id > 0);

        drop(db);
        manager.shutdown();
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn diagnostics_report_direct_backend() {
        let manager = Manager::with_capacity_cache_backend(
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

        let db_id = db.id().clone();
        assert_eq!(manager.default_io_backend(), IoBackendKind::DirectIo);
        assert_eq!(db.io_backend(), IoBackendKind::DirectIo);

        let diagnostics = manager.diagnostics();
        assert_eq!(diagnostics.default_io_backend, IoBackendKind::DirectIo);
        let pod = diagnostics
            .pods
            .iter()
            .find(|pod| pod.id == db_id)
            .expect("pod diagnostics");
        assert_eq!(pod.io_backend, IoBackendKind::DirectIo);

        drop(db);
        manager.shutdown();
    }

    #[test]
    fn close_db_removes_pod() {
        let manager = Manager::new();
        let config = DbConfig::builder("close").build();
        let db_id = config.id().clone();
        manager.open_db(config).expect("open");
        manager.close_db(&db_id).expect("close");
        assert!(manager.get_db(&db_id).is_none());
        manager.shutdown();
    }
}
