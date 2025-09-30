use std::collections::{HashMap, VecDeque};
use std::convert::TryFrom;
use std::env;
use std::fmt;
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use thiserror::Error;

use crate::checkpoint::delta::{DeltaError, DeltaHeader, DeltaReader, PageDirEntry};
use crate::cold_scan::{ColdScanError, ColdScanOptions, ColdScanStatsTracker, RemoteReadLimiter};
use crate::flush::{
    FlushController, FlushControllerConfig, FlushControllerSnapshot, FlushScheduleError, FlushSink,
};
use crate::io::{IoBackendKind, SharedIoDriver};
use crate::manager::ManagerInner;
use crate::manifest::{
    ChunkDeltaKey, ChunkDeltaRecord, ChunkEntryRecord, ChunkKey, Generation, Manifest,
    ManifestError, ManifestFlushSink,
};
use crate::page_cache::{
    PageCache, PageCacheKey, PageCacheMetricsSnapshot, PageCacheNamespace, allocate_cache_object_id,
};
use crate::page_store_policies::FetchOptions;
use crate::remote_store::{BlobKey, RemoteStore, RemoteStoreError};
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

    /// Reads a page from cold storage with bandwidth throttling.
    ///
    /// This is designed for scanning operations that need to read pages from
    /// remote storage without warming the cache or exhausting bandwidth.
    ///
    /// # Parameters
    ///
    /// - `page_no`: The page number to read
    /// - `manifest`: Manifest for resolving page locations
    /// - `remote`: Remote store for fetching chunks
    /// - `limiter`: Bandwidth limiter for throttling
    /// - `stats`: Optional statistics tracker
    /// - `options`: Cold scan configuration options
    pub async fn read_page_cold(
        &self,
        page_no: u64,
        manifest: &Arc<Manifest>,
        remote: &Arc<RemoteStore>,
        limiter: &RemoteReadLimiter,
        stats: Option<&ColdScanStatsTracker>,
        options: ColdScanOptions,
    ) -> Result<Vec<u8>, ColdScanError> {
        self.inner
            .page_store
            .read_page_cold(
                self.inner.id,
                page_no,
                manifest,
                remote,
                limiter,
                stats,
                options,
            )
            .await
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

/// Errors produced by PageStore operations.
#[derive(Debug, Error)]
pub enum PageStoreError {
    #[error("page not found: {0}")]
    NotFound(u32),

    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),

    #[error("remote store error: {0}")]
    Remote(#[from] RemoteStoreError),

    #[error("delta error: {0}")]
    Delta(#[from] DeltaError),

    #[error("cache error: {0}")]
    Cache(String),

    #[error("page out of bounds for chunk")]
    PageOutOfBounds,
}

/// Metadata about where a page is located (base chunk + delta chain).
#[derive(Debug, Clone)]
pub struct PageLocation {
    pub base_chunk: ChunkEntryRecord,
    pub base_chunk_key: ChunkKey,
    pub base_start_page: u64,
    pub deltas: Vec<DeltaLocation>,
}

/// Metadata about a delta in the chain.
#[derive(Debug, Clone)]
pub struct DeltaLocation {
    pub delta_key: ChunkDeltaKey,
    pub delta_record: ChunkDeltaRecord,
    pub generation: Generation,
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
    pub delta_cache: DeltaCacheMetricsSnapshot,
}

#[derive(Debug)]
struct DeltaCacheEntry {
    header: DeltaHeader,
    directory: Vec<PageDirEntry>,
    data_start: usize,
    data: Arc<Vec<u8>>,
}

impl DeltaCacheEntry {
    fn contains_page(&self, page_in_chunk: u32) -> bool {
        self.directory
            .iter()
            .any(|entry| entry.page_no == page_in_chunk)
    }

    fn read_page(&self, page_in_chunk: u32) -> Result<Vec<u8>, PageStoreError> {
        let entry = self
            .directory
            .iter()
            .find(|entry| entry.page_no == page_in_chunk)
            .ok_or(PageStoreError::NotFound(page_in_chunk))?;

        let start =
            self.data_start
                .checked_add(entry.offset as usize)
                .ok_or(PageStoreError::Delta(DeltaError::InvalidFormat(
                    "delta offset overflow".into(),
                )))?;
        let end =
            start
                .checked_add(self.header.page_size as usize)
                .ok_or(PageStoreError::Delta(DeltaError::InvalidFormat(
                    "delta length overflow".into(),
                )))?;

        if end > self.data.len() {
            return Err(PageStoreError::Delta(DeltaError::InvalidFormat(
                "delta entry exceeds file size".into(),
            )));
        }

        Ok(self.data[start..end].to_vec())
    }

    fn byte_len(&self) -> usize {
        self.data.len() + self.directory.len() * size_of::<PageDirEntry>()
    }
}
#[derive(Debug)]
struct DeltaCacheState {
    entries: HashMap<ChunkDeltaKey, Arc<DeltaCacheEntry>>,
    order: VecDeque<ChunkDeltaKey>,
    current_bytes: usize,
}

impl DeltaCacheState {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            current_bytes: 0,
        }
    }

    fn touch(&mut self, key: ChunkDeltaKey) {
        if let Some(pos) = self.order.iter().position(|k| *k == key) {
            self.order.remove(pos);
        }
        self.order.push_back(key);
    }
}

#[derive(Default, Debug)]
struct DeltaCacheMetrics {
    current_bytes: AtomicU64,
    current_entries: AtomicU64,
    total_insertions: AtomicU64,
    total_evictions: AtomicU64,
}

impl DeltaCacheMetrics {
    fn record_insert(&self, bytes: usize) {
        self.total_insertions.fetch_add(1, Ordering::Relaxed);
        self.current_entries.fetch_add(1, Ordering::Relaxed);
        self.current_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    fn record_eviction(&self, bytes: usize) {
        self.total_evictions.fetch_add(1, Ordering::Relaxed);
        self.current_entries.fetch_sub(1, Ordering::Relaxed);
        self.current_bytes
            .fetch_sub(bytes as u64, Ordering::Relaxed);
    }

    fn snapshot(&self) -> DeltaCacheMetricsSnapshot {
        DeltaCacheMetricsSnapshot {
            current_entries: self.current_entries.load(Ordering::Relaxed),
            current_bytes: self.current_bytes.load(Ordering::Relaxed),
            total_insertions: self.total_insertions.load(Ordering::Relaxed),
            total_evictions: self.total_evictions.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeltaCacheMetricsSnapshot {
    pub current_entries: u64,
    pub current_bytes: u64,
    pub total_insertions: u64,
    pub total_evictions: u64,
}

const DEFAULT_DELTA_CACHE_TARGET_BYTES: usize = 256 * 1024 * 1024;

struct PageStore {
    cache: Arc<PageCache<PageCacheKey>>,
    _namespace: PageCacheNamespace,
    object_id: u64,
    cached_pages: AtomicU64,
    delta_cache: Mutex<DeltaCacheState>,
    delta_metrics: DeltaCacheMetrics,
    delta_cache_target_bytes: usize,
}

impl PageStore {
    fn new(cache: Arc<PageCache<PageCacheKey>>) -> Self {
        Self {
            cache,
            _namespace: PageCacheNamespace::StoragePod,
            object_id: allocate_cache_object_id(),
            cached_pages: AtomicU64::new(0),
            delta_cache: Mutex::new(DeltaCacheState::new()),
            delta_metrics: DeltaCacheMetrics::default(),
            delta_cache_target_bytes: DEFAULT_DELTA_CACHE_TARGET_BYTES,
        }
    }

    fn metrics(&self) -> PageStoreMetrics {
        PageStoreMetrics {
            cache_object_id: self.object_id,
            cached_pages: self.cached_pages.load(Ordering::Relaxed),
            delta_cache: self.delta_metrics.snapshot(),
        }
    }

    fn cache_snapshot(&self) -> PageCacheMetricsSnapshot {
        self.cache.metrics()
    }

    #[allow(dead_code)]
    fn cache_key(&self, page_no: u64) -> PageCacheKey {
        PageCacheKey::storage_pod(self.object_id, page_no)
    }

    /// Read a page from cache or remote storage with delta layering.
    ///
    /// This implements T4c: reading pages by applying deltas over base chunks.
    async fn read_page(
        &self,
        db_id: DbId,
        page_no: u64,
        manifest: &Arc<Manifest>,
        remote: &Arc<RemoteStore>,
        options: FetchOptions,
    ) -> Result<Vec<u8>, PageStoreError> {
        let key = self.cache_key(page_no);

        // Check cache first unless bypass_cache is set
        if !options.bypass_cache {
            if let Some(cached) = self.cache.get(&key) {
                return Ok(cached.as_slice().to_vec());
            }
        }

        // Resolve page location from manifest
        let location = manifest.resolve_page_location(db_id.get(), page_no)?;

        // Read page with delta layering
        let page_data = self.read_with_deltas(location, page_no, remote).await?;

        // Cache the result unless bypassing cache
        if !options.bypass_cache {
            self.cache.insert(key, Arc::from(page_data.as_slice()));
            self.cached_pages.fetch_add(1, Ordering::Relaxed);
        }

        Ok(page_data)
    }

    /// Read a page with cold scan throttling (T9b).
    async fn read_page_cold(
        &self,
        db_id: DbId,
        page_no: u64,
        manifest: &Arc<Manifest>,
        remote: &Arc<RemoteStore>,
        limiter: &RemoteReadLimiter,
        stats: Option<&ColdScanStatsTracker>,
        options: ColdScanOptions,
    ) -> Result<Vec<u8>, ColdScanError> {
        let key = self.cache_key(page_no);

        // Check cache first unless bypass_cache is set
        if !options.bypass_cache {
            if let Some(cached) = self.cache.get(&key) {
                return Ok(cached.as_slice().to_vec());
            }
        }

        // Resolve page location
        let location = manifest
            .resolve_page_location(db_id.get(), page_no)
            .map_err(|e| ColdScanError::PageStore(e.to_string()))?;

        // Estimate bytes to read (base chunk + deltas)
        let estimated_bytes = Self::base_page_estimate(&location.base_chunk)
            + location
                .deltas
                .iter()
                .map(|d| d.delta_record.size_bytes)
                .sum::<u64>();

        // Request bandwidth from limiter
        match limiter.request_bytes(estimated_bytes) {
            Ok(()) => {}
            Err(ColdScanError::BandwidthExceeded(duration)) => {
                if let Some(tracker) = stats {
                    tracker.record_throttle();
                }
                // Sleep and retry once
                tokio::time::sleep(duration).await;
                limiter.request_bytes(estimated_bytes)?;
            }
            Err(e) => return Err(e),
        }

        // Read page with delta layering
        let page_data = self
            .read_with_deltas(location, page_no, remote)
            .await
            .map_err(|e| ColdScanError::PageStore(e.to_string()))?;

        // Record metrics
        if let Some(tracker) = stats {
            tracker.record_page_read(estimated_bytes);
        }

        Ok(page_data)
    }

    /// Apply delta layering to read a page (Phase 3 core algorithm).
    async fn read_with_deltas(
        &self,
        location: PageLocation,
        page_no: u64,
        remote: &Arc<RemoteStore>,
    ) -> Result<Vec<u8>, PageStoreError> {
        let chunk_page_count = location.base_chunk.effective_page_count();
        if chunk_page_count == 0 {
            return Err(PageStoreError::PageOutOfBounds);
        }

        let chunk_start = location.base_start_page;
        let chunk_end = chunk_start
            .checked_add(chunk_page_count)
            .ok_or(PageStoreError::PageOutOfBounds)?;
        let global_page = page_no;

        if global_page < chunk_start || global_page >= chunk_end {
            return Err(PageStoreError::PageOutOfBounds);
        }

        let relative_page = u32::try_from(global_page - chunk_start)
            .map_err(|_| PageStoreError::PageOutOfBounds)?;

        let mut page_data = self
            .fetch_page_from_base_chunk(
                &location.base_chunk,
                &location.base_chunk_key,
                relative_page,
                remote,
            )
            .await?;

        // Apply deltas in order (oldest to newest)
        for delta_loc in &location.deltas {
            if self
                .delta_contains_page(delta_loc, relative_page, remote)
                .await?
            {
                let delta_page = self
                    .fetch_delta_page(delta_loc, relative_page, remote)
                    .await?;
                page_data = delta_page; // Replace with delta version
            }
        }

        Ok(page_data)
    }
    /// Fetch a page from a base chunk blob.
    fn base_page_estimate(chunk: &ChunkEntryRecord) -> u64 {
        if chunk.page_size != 0 {
            chunk.page_size as u64
        } else {
            chunk.size_bytes
        }
    }

    async fn fetch_page_from_base_chunk(
        &self,
        chunk: &ChunkEntryRecord,
        chunk_key: &ChunkKey,
        page_in_chunk: u32,
        remote: &Arc<RemoteStore>,
    ) -> Result<Vec<u8>, PageStoreError> {
        if (page_in_chunk as u64) >= chunk.effective_page_count() {
            return Err(PageStoreError::PageOutOfBounds);
        }

        let blob_key = BlobKey::chunk(
            chunk_key.db_id,
            chunk_key.chunk_id,
            chunk.generation,
            chunk.content_hash,
        );

        if chunk.page_size != 0 {
            let page_size = chunk.page_size as u64;
            if let Some(start) = (page_in_chunk as u64).checked_mul(page_size) {
                if let Some(end) = start.checked_add(page_size) {
                    match remote.get_blob_range(&blob_key, start..end).await {
                        Ok(data) if data.len() == page_size as usize => return Ok(data),
                        Ok(_) => {} // Fallback to full download below
                        Err(err) => {
                            if matches!(err, RemoteStoreError::NotFound(_)) {
                                return Err(err.into());
                            }
                        }
                    }
                }
            }
        }

        let mut chunk_data = Vec::new();
        remote
            .get_blob(&blob_key, &mut chunk_data, None)
            .await
            .map_err(PageStoreError::Remote)?;

        self.extract_page_from_chunk(&chunk_data, page_in_chunk, chunk.page_size)
    }

    /// Extract a specific page from chunk data.
    fn extract_page_from_chunk(
        &self,
        chunk_data: &[u8],
        page_in_chunk: u32,
        page_size: u32,
    ) -> Result<Vec<u8>, PageStoreError> {
        let page_size = page_size as usize;
        let offset = (page_in_chunk as usize)
            .checked_mul(page_size)
            .ok_or(PageStoreError::PageOutOfBounds)?;
        let end = offset
            .checked_add(page_size)
            .ok_or(PageStoreError::PageOutOfBounds)?;

        if end > chunk_data.len() {
            return Err(PageStoreError::PageOutOfBounds);
        }

        Ok(chunk_data[offset..end].to_vec())
    }

    /// Check if a delta contains a specific page.
    ///
    /// Phase 4: Uses cached delta headers to avoid downloading full deltas.
    async fn delta_contains_page(
        &self,
        delta_loc: &DeltaLocation,
        page_in_chunk: u32,
        remote: &Arc<RemoteStore>,
    ) -> Result<bool, PageStoreError> {
        let entry = self.load_delta_entry(delta_loc, remote).await?;
        Ok(entry.contains_page(page_in_chunk))
    }

    async fn load_delta_entry(
        &self,
        delta_loc: &DeltaLocation,
        remote: &Arc<RemoteStore>,
    ) -> Result<Arc<DeltaCacheEntry>, PageStoreError> {
        let key = delta_loc.delta_key;

        {
            let mut cache = self.delta_cache.lock().unwrap();
            if let Some(entry) = cache.entries.get(&key).cloned() {
                cache.touch(key);
                return Ok(entry);
            }
        }

        let fetched = self.download_delta_entry(delta_loc, remote).await?;
        let entry_size = fetched.byte_len();

        let mut cache = self.delta_cache.lock().unwrap();
        if let Some(entry) = cache.entries.get(&key).cloned() {
            cache.touch(key);
            return Ok(entry);
        }

        cache.entries.insert(key, fetched.clone());
        cache.touch(key);
        cache.current_bytes = cache.current_bytes.saturating_add(entry_size);
        self.delta_metrics.record_insert(entry_size);
        self.evict_delta_cache(&mut cache);

        Ok(fetched)
    }

    async fn download_delta_entry(
        &self,
        delta_loc: &DeltaLocation,
        remote: &Arc<RemoteStore>,
    ) -> Result<Arc<DeltaCacheEntry>, PageStoreError> {
        let blob_key = BlobKey::delta(
            delta_loc.delta_key.db_id,
            delta_loc.delta_key.chunk_id,
            delta_loc.generation,
            delta_loc.delta_record.delta_id,
            delta_loc.delta_record.content_hash,
        );

        let mut delta_data = Vec::new();
        remote
            .get_blob(&blob_key, &mut delta_data, None)
            .await
            .map_err(PageStoreError::Remote)?;

        let mut cursor = std::io::Cursor::new(delta_data);
        let delta_reader = DeltaReader::new(&mut cursor)?;
        let header = delta_reader.header().clone();
        let directory = delta_reader.directory().to_vec();
        let data_start = cursor.position() as usize;
        let data_vec = cursor.into_inner();
        if data_start > data_vec.len() {
            return Err(PageStoreError::Delta(DeltaError::InvalidFormat(
                "delta data shorter than header".into(),
            )));
        }
        let data = Arc::new(data_vec);

        Ok(Arc::new(DeltaCacheEntry {
            header,
            directory,
            data_start,
            data,
        }))
    }

    fn evict_delta_cache(&self, cache: &mut DeltaCacheState) {
        while cache.current_bytes > self.delta_cache_target_bytes {
            let Some(oldest) = cache.order.pop_front() else {
                break;
            };

            if let Some(entry) = cache.entries.remove(&oldest) {
                let size = entry.byte_len();
                cache.current_bytes = cache.current_bytes.saturating_sub(size);
                self.delta_metrics.record_eviction(size);
            }
        }
    }

    /// Fetch a specific page from a delta.
    async fn fetch_delta_page(
        &self,
        delta_loc: &DeltaLocation,
        page_in_chunk: u32,
        remote: &Arc<RemoteStore>,
    ) -> Result<Vec<u8>, PageStoreError> {
        let entry = self.load_delta_entry(delta_loc, remote).await?;
        entry.read_page(page_in_chunk)
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
