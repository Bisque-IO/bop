//! Append-only storage engine components.

pub mod checkpoint;
pub mod reader;
mod wal;

pub use checkpoint::{
    AofCheckpointConfig, AofCheckpointContext, AofCheckpointError, AofCheckpointJob,
    AofCheckpointOutcome, AofPlanner, AofPlannerContext, LeaseMap, TruncateDirection,
    TruncationError, TruncationRequest, run_checkpoint,
};
pub use reader::{AofCursor, AofReaderError};
pub use wal::{
    AofWal, AofWalDiagnostics, AofWalSegment, AofWalSegmentError, AofWalSegmentSnapshot,
    StagedBatchStats, WriteBatch, WriteBufferError, WriteChunk,
};

use crate::manifest::ChunkId;

/// The virtual chunk ID used for the active tail segment.
/// This is always the highest possible chunk ID to avoid conflicts with sealed chunks.
pub const TAIL_CHUNK_ID: ChunkId = ChunkId::MAX;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use std::{env, fmt};

use thiserror::Error;

use crate::chunk_quota::ChunkStorageQuota;
use crate::flush::{FlushController, FlushControllerConfig, FlushControllerSnapshot};
use crate::io::{IoBackendKind, IoError, SharedIoDriver};
use crate::local_store::{
    LocalChunkHandle, LocalChunkStore, LocalChunkStoreConfig, LocalChunkStoreError,
};
use crate::manager::ManagerInner;
use crate::manifest::{DbId, Manifest};
use crate::remote_chunk::{
    RemoteChunkError, RemoteChunkFetcher, RemoteChunkSpec, RemoteChunkStore,
};
const DEFAULT_CHUNK_CACHE_BYTES: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB
const DEFAULT_CHUNK_CACHE_MIN_AGE_SECS: u64 = 300; // 5 minutes

use crate::runtime::StorageRuntime;
use crate::write::{WriteController, WriteControllerConfig, WriteControllerSnapshot};

/// Identifier for a storage pod/database instance managed by `Manager`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct AofId(u32);

impl AofId {
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

impl fmt::Display for AofId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for AofId {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl From<AofId> for u32 {
    fn from(value: AofId) -> Self {
        value.get()
    }
}

impl From<AofId> for u64 {
    fn from(value: AofId) -> Self {
        value.as_u64()
    }
}

/// Errors produced by storage pod operations.
#[derive(Debug, Error)]
pub enum AofError {
    #[error("database is closed")]
    Closed,
    #[error("local chunk store error: {0}")]
    LocalChunk(#[from] LocalChunkStoreError),
    #[error("remote chunk error: {0}")]
    RemoteChunk(#[from] RemoteChunkError),
    #[error("I/O error: {0}")]
    Io(#[from] IoError),
    #[error("remote chunk fetcher is not configured")]
    MissingRemoteFetcher,
    #[error("chunk spec targets db {db_id} but this AOF has id {aof_id}")]
    WrongDatabase { db_id: DbId, aof_id: AofId },
}

/// Chunk size constraints for AOF instances.
pub const MIN_CHUNK_SIZE_BYTES: u64 = 64 * 1024; // 64 KB (2^16)
pub const MAX_CHUNK_SIZE_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GB (2^32)
pub const DEFAULT_CHUNK_SIZE_BYTES: u64 = 64 * 1024 * 1024; // 64 MB

/// Calculate the optimal chunk size based on recent write velocity.
///
/// This function selects a power-of-2 chunk size between 64KB and 4GB based on the
/// observed write rate. The goal is to:
/// - Use smaller chunks for slow/bursty workloads to minimize wasted space
/// - Use larger chunks for sustained high-throughput workloads to reduce overhead
///
/// # Write Rate Thresholds
///
/// - < 100 KB/s: 64 KB chunks (very slow/idle)
/// - < 500 KB/s: 256 KB chunks (slow)
/// - < 2 MB/s: 1 MB chunks (moderate)
/// - < 10 MB/s: 4 MB chunks (active)
/// - < 50 MB/s: 16 MB chunks (fast)
/// - < 200 MB/s: 64 MB chunks (very fast)
/// - < 500 MB/s: 256 MB chunks (burst)
/// - >= 500 MB/s: 1 GB chunks (sustained burst)
///
/// If no write rate data is available, returns `default_size`.
pub fn calculate_adaptive_chunk_size(
    write_rate_bytes_per_sec: Option<u64>,
    default_size: u64,
) -> u64 {
    let rate = match write_rate_bytes_per_sec {
        Some(r) if r > 0 => r,
        _ => return default_size,
    };

    let size = if rate < 100 * 1024 {
        64 * 1024 // 64 KB
    } else if rate < 500 * 1024 {
        256 * 1024 // 256 KB
    } else if rate < 2 * 1024 * 1024 {
        1024 * 1024 // 1 MB
    } else if rate < 10 * 1024 * 1024 {
        4 * 1024 * 1024 // 4 MB
    } else if rate < 50 * 1024 * 1024 {
        16 * 1024 * 1024 // 16 MB
    } else if rate < 200 * 1024 * 1024 {
        64 * 1024 * 1024 // 64 MB
    } else if rate < 500 * 1024 * 1024 {
        256 * 1024 * 1024 // 256 MB
    } else {
        1024 * 1024 * 1024 // 1 GB
    };

    // Clamp to valid range
    size.clamp(MIN_CHUNK_SIZE_BYTES, MAX_CHUNK_SIZE_BYTES)
}

/// Configuration used when constructing a new storage pod.
#[derive(Clone)]
pub struct AofConfig {
    id: Option<AofId>,
    name: String,
    data_dir: PathBuf,
    wal_dir: PathBuf,
    chunk_size_bytes: u64,
    pre_allocate_threshold: f64, // 0.0 to 1.0, fraction of chunk fullness
    io_backend: Option<IoBackendKind>,
    chunk_cache_dir: Option<PathBuf>,
    chunk_cache_bytes: Option<u64>,
    chunk_cache_min_age: Option<Duration>,
    remote_fetcher: Option<Arc<dyn RemoteChunkFetcher>>,
}

impl AofConfig {
    pub fn builder(name: impl Into<String>) -> AofConfigBuilder {
        AofConfigBuilder::new(name.into())
    }

    pub fn id(&self) -> Option<AofId> {
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

    pub fn chunk_size_bytes(&self) -> u64 {
        self.chunk_size_bytes
    }

    pub fn pre_allocate_threshold(&self) -> f64 {
        self.pre_allocate_threshold
    }

    pub fn io_backend(&self) -> Option<IoBackendKind> {
        self.io_backend
    }

    pub fn chunk_cache_dir(&self) -> Option<&PathBuf> {
        self.chunk_cache_dir.as_ref()
    }

    pub fn chunk_cache_bytes(&self) -> Option<u64> {
        self.chunk_cache_bytes
    }

    pub fn chunk_cache_min_age(&self) -> Option<Duration> {
        self.chunk_cache_min_age
    }

    pub fn remote_fetcher(&self) -> Option<&Arc<dyn RemoteChunkFetcher>> {
        self.remote_fetcher.as_ref()
    }

    pub(crate) fn assign_id(&mut self, id: AofId) {
        self.id = Some(id);
    }
}

pub struct AofConfigBuilder {
    id: Option<AofId>,
    name: String,
    data_dir: Option<PathBuf>,
    wal_dir: Option<PathBuf>,
    chunk_size_bytes: u64,
    pre_allocate_threshold: f64,
    chunk_cache_dir: Option<PathBuf>,
    chunk_cache_bytes: Option<u64>,
    chunk_cache_min_age: Option<Duration>,
    io_backend: Option<IoBackendKind>,
    remote_fetcher: Option<Arc<dyn RemoteChunkFetcher>>,
}

impl AofConfigBuilder {
    fn new(name: String) -> Self {
        Self {
            id: None,
            name,
            data_dir: None,
            wal_dir: None,
            chunk_size_bytes: DEFAULT_CHUNK_SIZE_BYTES,
            pre_allocate_threshold: 0.5, // Default: 50%
            chunk_cache_dir: None,
            chunk_cache_bytes: None,
            chunk_cache_min_age: None,
            io_backend: None,
            remote_fetcher: None,
        }
    }

    pub fn id(mut self, id: AofId) -> Self {
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

    pub fn chunk_size_bytes(mut self, bytes: u64) -> Self {
        self.chunk_size_bytes = bytes.clamp(MIN_CHUNK_SIZE_BYTES, MAX_CHUNK_SIZE_BYTES);
        self
    }

    pub fn pre_allocate_threshold(mut self, threshold: f64) -> Self {
        self.pre_allocate_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    pub fn chunk_cache_bytes(mut self, bytes: u64) -> Self {
        self.chunk_cache_bytes = Some(bytes);
        self
    }

    pub fn chunk_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.chunk_cache_dir = Some(dir.into());
        self
    }

    pub fn chunk_cache_min_eviction_age(mut self, age: Duration) -> Self {
        self.chunk_cache_min_age = Some(age);
        self
    }

    pub fn with_remote_fetcher(mut self, fetcher: Arc<dyn RemoteChunkFetcher>) -> Self {
        self.remote_fetcher = Some(fetcher);
        self
    }

    pub fn io_backend(mut self, backend: IoBackendKind) -> Self {
        self.io_backend = Some(backend);
        self
    }

    pub fn build(self) -> AofConfig {
        let base = default_base_dir(&self.name);
        let data_dir = self.data_dir.unwrap_or_else(|| base.join("data"));
        let wal_dir = self.wal_dir.unwrap_or_else(|| data_dir.join("wal"));

        AofConfig {
            id: self.id,
            name: self.name,
            data_dir,
            wal_dir,
            chunk_size_bytes: self.chunk_size_bytes,
            pre_allocate_threshold: self.pre_allocate_threshold,
            io_backend: self.io_backend,
            chunk_cache_dir: self.chunk_cache_dir,
            chunk_cache_bytes: self.chunk_cache_bytes,
            chunk_cache_min_age: self.chunk_cache_min_age,
            remote_fetcher: self.remote_fetcher,
        }
    }
}

fn default_base_dir(name: &str) -> PathBuf {
    env::temp_dir().join(format!("bop-storage-{}", slugify(name)))
}

impl fmt::Debug for AofConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AofConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("data_dir", &self.data_dir)
            .field("wal_dir", &self.wal_dir)
            .field("chunk_size_bytes", &self.chunk_size_bytes)
            .field("io_backend", &self.io_backend)
            .field("chunk_cache_dir", &self.chunk_cache_dir)
            .field("chunk_cache_bytes", &self.chunk_cache_bytes)
            .field("chunk_cache_min_age", &self.chunk_cache_min_age)
            .finish()
    }
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
        slug.push('a');
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "aof".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct AofDiagnostics {
    pub id: AofId,
    pub name: String,
    pub io_backend: IoBackendKind,
    pub is_closed: bool,
    pub wal: AofWalDiagnostics,
    pub write_controller: WriteControllerSnapshot,
    pub flush_controller: FlushControllerSnapshot,
}

/// Top-level handle to a storage pod instance backed by `Arc<AofInner>`.
#[derive(Clone)]
pub struct Aof {
    inner: Arc<AofInner>,
}

impl Aof {
    pub(crate) fn from_arc(inner: Arc<AofInner>) -> Self {
        Self { inner }
    }

    pub fn id(&self) -> AofId {
        self.inner.id
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    /// Append data to the AOF.
    /// Returns the starting LSN where the data was written.
    pub fn append(&self, data: WriteChunk) -> Result<u64, AofError> {
        self.inner.append(data)
    }

    /// Append multiple data chunks atomically.
    /// Returns the starting LSN where the data was written.
    pub fn append_batch(&self, data: &[WriteChunk]) -> Result<u64, AofError> {
        self.inner.append_batch(data)
    }

    /// Ensure all appended data up to the returned LSN is durable.
    pub fn sync(&self) -> Result<u64, AofError> {
        self.inner.sync()
    }

    /// Run a checkpoint to seal and upload completed chunks.
    pub fn checkpoint(&self) -> Result<(), AofError> {
        self.inner.checkpoint()
    }

    /// Get the current tail LSN (highest allocated position).
    pub fn tail_lsn(&self) -> u64 {
        self.inner.tail_lsn()
    }

    /// Get the current durable LSN (highest fsynced position).
    pub fn durable_lsn(&self) -> u64 {
        self.inner.durable_lsn()
    }

    pub fn close(&self) {
        self.inner.close();
    }

    #[allow(dead_code)]
    pub(crate) fn runtime(&self) -> Arc<StorageRuntime> {
        self.inner.runtime()
    }

    pub fn io_backend(&self) -> IoBackendKind {
        self.inner.io_backend()
    }

    pub fn ensure_chunk(&self, spec: &RemoteChunkSpec) -> Result<LocalChunkHandle, AofError> {
        self.inner.ensure_chunk(spec)
    }

    pub fn local_chunk_store(&self) -> Arc<LocalChunkStore> {
        self.inner.local_chunk_store()
    }

    pub fn diagnostics(&self) -> AofDiagnostics {
        self.inner.diagnostics()
    }

    pub fn chunk_size_bytes(&self) -> u64 {
        self.inner.chunk_size_bytes
    }

    /// Calculate which chunk ID contains the given LSN.
    ///
    /// # Note
    ///
    /// This currently uses fixed-size arithmetic for sealed chunks. When variable-sized
    /// chunks are sealed and recorded in the manifest, this should query the manifest
    /// to find the chunk containing the given LSN by searching AofChunkRecord entries
    /// where start_lsn <= lsn < end_lsn.
    pub fn chunk_id_for_lsn(&self, lsn: u64) -> ChunkId {
        // TODO: Query manifest for variable-sized chunks once sealing is implemented
        (lsn / self.inner.chunk_size_bytes) as ChunkId
    }

    /// Calculate the starting LSN for a given chunk ID.
    ///
    /// # Note
    ///
    /// This currently uses fixed-size arithmetic. With variable-sized chunks,
    /// this should query the AofChunkRecord from the manifest.
    pub fn chunk_start_lsn(&self, chunk_id: ChunkId) -> u64 {
        // TODO: Query manifest for variable-sized chunks
        (chunk_id as u64) * self.inner.chunk_size_bytes
    }

    /// Calculate the ending LSN (exclusive) for a given chunk ID.
    ///
    /// # Note
    ///
    /// This currently uses fixed-size arithmetic. With variable-sized chunks,
    /// this should query the AofChunkRecord from the manifest.
    pub fn chunk_end_lsn(&self, chunk_id: ChunkId) -> u64 {
        // TODO: Query manifest for variable-sized chunks
        ((chunk_id as u64) + 1) * self.inner.chunk_size_bytes
    }

    /// Calculate the offset within a chunk for a given LSN.
    ///
    /// # Note
    ///
    /// With variable-sized chunks, this will use the chunk's start_lsn from manifest.
    pub fn chunk_offset(&self, lsn: u64) -> u64 {
        lsn % self.inner.chunk_size_bytes
    }

    /// Open a sequential reader cursor starting at LSN 0.
    pub fn open_cursor(&self) -> AofCursor {
        AofCursor::new(self.inner.clone())
    }

    /// Open a sequential reader cursor starting at a specific LSN.
    pub fn open_cursor_at(&self, lsn: u64) -> Result<AofCursor, reader::AofReaderError> {
        let mut cursor = self.open_cursor();
        cursor.seek(lsn)?;
        Ok(cursor)
    }

    /// Get the current tail segment (active append region).
    /// Note: This is primarily for internal use and testing.
    #[allow(dead_code)]
    pub(crate) fn tail_segment(&self) -> Option<Arc<AofWalSegment>> {
        self.inner
            .tail_state
            .lock()
            .expect("tail state mutex poisoned")
            .segment
            .clone()
    }

    /// Seal the current tail segment and prepare it for checkpointing.
    /// This converts the mutable tail into an immutable sealed chunk.
    /// Returns the sealed segment and updates the base LSN for the next segment.
    pub fn seal_tail(&self) -> Option<Arc<AofWalSegment>> {
        let mut state = self
            .inner
            .tail_state
            .lock()
            .expect("tail state mutex poisoned");
        let sealed = state.segment.take();
        if sealed.is_some() {
            // Update base LSN for next segment
            state.base_lsn = state.tail_lsn;
        }
        sealed
    }

    /// Calculate which chunk ID a given LSN belongs to.
    /// Returns TAIL_CHUNK_ID if the LSN is in the active tail segment.
    pub fn chunk_id_for_lsn_with_tail(&self, lsn: u64) -> ChunkId {
        let sealed_chunks_end_lsn = self.inner.wal.last_sequence();

        if lsn >= sealed_chunks_end_lsn {
            // LSN is in the tail segment
            TAIL_CHUNK_ID
        } else {
            // LSN is in a sealed chunk
            self.chunk_id_for_lsn(lsn)
        }
    }
}

/// Tail segment state tracking.
struct TailState {
    /// Current active segment for writes.
    segment: Option<Arc<AofWalSegment>>,
    /// Base LSN of the current segment.
    base_lsn: u64,
    /// Highest allocated LSN.
    tail_lsn: u64,
    /// Next segment pre-allocation in progress.
    next_segment: Option<Arc<AofWalSegment>>,
    /// Base LSN for the next pre-allocated segment.
    next_base_lsn: u64,
}

impl TailState {
    fn new() -> Self {
        Self {
            segment: None,
            base_lsn: 0,
            tail_lsn: 0,
            next_segment: None,
            next_base_lsn: 0,
        }
    }
}

pub(crate) struct AofInner {
    id: AofId,
    name: String,
    _config: AofConfig,
    chunk_size_bytes: u64,
    io: SharedIoDriver,
    wal: AofWal,
    /// Active tail segment for appends. This is the mutable region of the AOF.
    tail_state: Mutex<TailState>,
    write_controller: WriteController,
    flush_controller: FlushController,
    #[allow(dead_code)]
    runtime: Arc<StorageRuntime>,
    manager: Weak<ManagerInner>,
    manifest: Arc<Manifest>,
    local_chunks: Arc<LocalChunkStore>,
    remote_chunks: Option<RemoteChunkStore>,
    closed: AtomicBool,
    deregistered: AtomicBool,
}

impl AofInner {
    pub(crate) fn bootstrap(
        config: AofConfig,
        io: SharedIoDriver,
        manager: Arc<ManagerInner>,
        runtime: Arc<StorageRuntime>,
        manifest: Arc<Manifest>,
        chunk_quota: Arc<ChunkStorageQuota>,
    ) -> Result<Arc<Self>, LocalChunkStoreError> {
        let id = config
            .id()
            .expect("database id must be assigned before bootstrap");
        let name = config.name().to_string();
        let wal = AofWal::new();
        let write_controller =
            WriteController::new(runtime.clone(), WriteControllerConfig::default());
        let flush_controller = FlushController::new(
            runtime.clone(),
            manifest.clone(),
            id.get(),
            FlushControllerConfig::default(),
        );

        let data_dir = config.data_dir().clone();
        let chunk_cache_dir = config
            .chunk_cache_dir()
            .cloned()
            .unwrap_or_else(|| data_dir.join("chunks"));
        let chunk_cache_bytes = config
            .chunk_cache_bytes()
            .unwrap_or(DEFAULT_CHUNK_CACHE_BYTES);
        let chunk_cache_min_age = config
            .chunk_cache_min_age()
            .unwrap_or(Duration::from_secs(DEFAULT_CHUNK_CACHE_MIN_AGE_SECS));
        let local_cache_config = LocalChunkStoreConfig {
            root_dir: chunk_cache_dir,
            max_cache_bytes: chunk_cache_bytes,
            min_eviction_age: chunk_cache_min_age,
        };
        let local_chunks = Arc::new(LocalChunkStore::new(
            local_cache_config,
            chunk_quota.clone(),
            io.clone(),
            runtime.clone(),
        )?);
        let remote_chunks = config.remote_fetcher().cloned().map(RemoteChunkStore::new);

        let chunk_size_bytes = config.chunk_size_bytes();

        Ok(Arc::new(Self {
            id,
            name,
            _config: config,
            chunk_size_bytes,
            io,
            wal,
            tail_state: Mutex::new(TailState::new()),
            write_controller,
            flush_controller,
            runtime,
            manager: Arc::downgrade(&manager),
            manifest,
            local_chunks,
            remote_chunks,
            closed: AtomicBool::new(false),
            deregistered: AtomicBool::new(false),
        }))
    }

    #[allow(dead_code)]
    pub(crate) fn runtime(&self) -> Arc<StorageRuntime> {
        self.runtime.clone()
    }

    pub fn diagnostics(&self) -> AofDiagnostics {
        AofDiagnostics {
            id: self.id,
            name: self.name.clone(),
            io_backend: self.io_backend(),
            is_closed: self.is_closed(),
            wal: self.wal.diagnostics(),
            write_controller: self.write_controller.snapshot(),
            flush_controller: self.flush_controller.snapshot(),
        }
    }

    /// Append data to the tail segment.
    pub fn append(&self, chunk: WriteChunk) -> Result<u64, AofError> {
        if self.is_closed() {
            return Err(AofError::Closed);
        }

        let chunk_len = chunk.len() as u64;

        let mut state = self.tail_state.lock().expect("tail state mutex poisoned");

        // Ensure we have an active tail segment
        if state.segment.is_none() {
            self.initialize_tail_segment(&mut state)?;
        }

        let start_lsn = state.tail_lsn;

        // Check if we're at or past a chunk boundary or need to roll
        let offset_in_chunk = start_lsn % self.chunk_size_bytes;
        let remaining_in_chunk = self.chunk_size_bytes - offset_in_chunk;

        if remaining_in_chunk < chunk_len || remaining_in_chunk == 0 {
            // Need to roll to a new segment
            // First, seal the current segment
            let sealed = state.segment.take();
            if sealed.is_some() {
                // Update to next chunk boundary
                state.base_lsn = ((start_lsn / self.chunk_size_bytes) + 1) * self.chunk_size_bytes;
                state.tail_lsn = state.base_lsn;
            }

            // Initialize new segment
            self.initialize_tail_segment(&mut state)?;
        }

        let segment = state.segment.as_ref().unwrap();

        // Reserve space in the segment
        segment.reserve_pending(chunk_len).map_err(|e| {
            AofError::LocalChunk(crate::local_store::LocalChunkStoreError::Io(
                std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            ))
        })?;

        // Add to active batch and enqueue if it was empty
        let was_empty = segment.with_active_batch(|batch| {
            let empty = batch.is_empty();
            batch.push(chunk);
            empty
        });

        // Enqueue if batch transitioned from empty to non-empty
        // This guarantees exactly one enqueue per batch cycle
        if was_empty {
            self.write_controller
                .enqueue(segment.clone())
                .map_err(|_| AofError::Closed)?;
        }

        state.tail_lsn += chunk_len;

        // Check if we should trigger pre-allocation for the next segment
        let _ = self.check_preallocate_trigger(&mut state);

        Ok(start_lsn)
    }

    /// Append multiple chunks atomically.
    pub fn append_batch(&self, chunks: &[WriteChunk]) -> Result<u64, AofError> {
        if self.is_closed() {
            return Err(AofError::Closed);
        }

        let total_len: usize = chunks.iter().map(|c| c.len()).sum();
        let mut state = self.tail_state.lock().expect("tail state mutex poisoned");

        if state.segment.is_none() {
            self.initialize_tail_segment(&mut state)?;
        }

        let start_lsn = state.tail_lsn;

        // Check if we need to roll to a new segment
        let offset_in_chunk = start_lsn % self.chunk_size_bytes;
        let remaining_in_chunk = self.chunk_size_bytes - offset_in_chunk;

        if remaining_in_chunk < total_len as u64 || remaining_in_chunk == 0 {
            let sealed = state.segment.take();
            if sealed.is_some() {
                state.base_lsn = ((start_lsn / self.chunk_size_bytes) + 1) * self.chunk_size_bytes;
                state.tail_lsn = state.base_lsn;
            }
            self.initialize_tail_segment(&mut state)?;
        }

        let segment = state.segment.as_ref().unwrap();

        segment.reserve_pending(total_len as u64).map_err(|e| {
            AofError::LocalChunk(crate::local_store::LocalChunkStoreError::Io(
                std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            ))
        })?;

        // Add to active batch and check if we need to enqueue
        let was_empty = segment.with_active_batch(|batch| {
            let empty = batch.is_empty();
            for chunk in chunks {
                batch.push(chunk.clone());
            }
            empty
        });

        // Only enqueue if batch was empty (transitioning from 0→1 entries)
        if was_empty {
            self.write_controller
                .enqueue(segment.clone())
                .map_err(|_| AofError::Closed)?;
        }

        state.tail_lsn += total_len as u64;

        // Check if we should trigger pre-allocation for the next segment
        let _ = self.check_preallocate_trigger(&mut state);

        Ok(state.tail_lsn - total_len as u64)
    }

    /// Ensure all data is durable up to the current tail.
    pub fn sync(&self) -> Result<u64, AofError> {
        if self.is_closed() {
            return Err(AofError::Closed);
        }

        let state = self.tail_state.lock().expect("tail state mutex poisoned");

        if let Some(segment) = state.segment.clone() {
            let target = state.tail_lsn - state.base_lsn;
            let base_lsn = state.base_lsn;
            drop(state); // Release lock before waiting

            // Wait for writes to complete
            let timeout = std::time::Duration::from_secs(10);
            let start = std::time::Instant::now();
            while segment.written_size() < target {
                if start.elapsed() > timeout {
                    return Err(AofError::LocalChunk(
                        crate::local_store::LocalChunkStoreError::Io(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "write timeout",
                        )),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }

            // Request flush
            if segment.request_flush(target).map_err(|e| {
                AofError::LocalChunk(crate::local_store::LocalChunkStoreError::Io(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                ))
            })? {
                self.flush_controller
                    .enqueue(segment.clone())
                    .map_err(|_| AofError::Closed)?;
            }

            // Wait for flush to complete
            let start = std::time::Instant::now();
            while segment.durable_size() < target {
                if start.elapsed() > timeout {
                    return Err(AofError::LocalChunk(
                        crate::local_store::LocalChunkStoreError::Io(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "flush timeout",
                        )),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }

            Ok(base_lsn + segment.durable_size())
        } else {
            Ok(0)
        }
    }

    pub fn tail_lsn(&self) -> u64 {
        let state = self.tail_state.lock().expect("tail state mutex poisoned");
        state.tail_lsn
    }

    pub fn durable_lsn(&self) -> u64 {
        let state = self.tail_state.lock().expect("tail state mutex poisoned");
        if let Some(segment) = &state.segment {
            state.base_lsn + segment.durable_size()
        } else {
            0
        }
    }

    pub fn checkpoint(&self) -> Result<(), AofError> {
        if self.is_closed() {
            return Err(AofError::Closed);
        }
        let next = self.wal.last_sequence().saturating_add(1);
        self.wal.mark_progress(next);
        Ok(())
    }

    /// Check if we should pre-allocate the next segment based on the threshold.
    /// Returns true if pre-allocation was triggered.
    fn check_preallocate_trigger(&self, state: &mut TailState) -> Result<bool, AofError> {
        // Skip if already pre-allocated
        if state.next_segment.is_some() {
            return Ok(false);
        }

        let segment = match &state.segment {
            Some(seg) => seg,
            None => return Ok(false),
        };

        // Calculate current fullness percentage
        let written = segment.written_size();
        let capacity = segment.preallocated_size();
        if capacity == 0 {
            return Ok(false);
        }

        let fullness = written as f64 / capacity as f64;
        let threshold = self._config.pre_allocate_threshold();

        if fullness < threshold {
            return Ok(false);
        }

        // Trigger pre-allocation
        let next_base_lsn = state.base_lsn + capacity;

        // Calculate adaptive chunk size based on current segment's write rate
        let write_rate = segment.write_rate_bytes_per_sec();
        let next_size = calculate_adaptive_chunk_size(
            if write_rate > 0 {
                Some(write_rate)
            } else {
                None
            },
            self.chunk_size_bytes,
        );

        // Pre-allocate the next segment
        let next_segment = self.create_segment(next_base_lsn, next_size)?;

        state.next_segment = Some(next_segment);
        state.next_base_lsn = next_base_lsn;

        Ok(true)
    }

    /// Initialize or use pre-allocated tail segment.
    fn initialize_tail_segment(&self, state: &mut TailState) -> Result<(), AofError> {
        // Check if we have a pre-allocated next segment
        if let Some(next_segment) = state.next_segment.take() {
            state.base_lsn = state.next_base_lsn;
            state.segment = Some(next_segment);
            return Ok(());
        }

        // No pre-allocated segment, create one now
        let segment = self.create_segment(state.base_lsn, self.chunk_size_bytes)?;
        state.segment = Some(segment);
        Ok(())
    }

    /// Create a new segment file with the given base LSN and size.
    fn create_segment(&self, base_lsn: u64, size: u64) -> Result<Arc<AofWalSegment>, AofError> {
        let segment_path = self.tail_segment_path(base_lsn);

        // Ensure directory exists
        if let Some(parent) = segment_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AofError::LocalChunk(crate::local_store::LocalChunkStoreError::Io(e))
            })?;
        }

        // Open file for writing
        let file = self
            .io
            .open(&segment_path, &crate::io::IoOpenOptions::write_only())?;

        // Create segment with specified size
        let segment = Arc::new(AofWalSegment::new(Arc::from(file), base_lsn, size));

        Ok(segment)
    }

    fn tail_segment_path(&self, base_lsn: u64) -> PathBuf {
        let data_dir = self._config.data_dir();
        let chunk_id = (base_lsn / self.chunk_size_bytes) as ChunkId;
        // Create subfolder for this AOF under data_dir
        let aof_dir = data_dir.join(format!("{}", self.id.get()));
        // Use chunk_id for naming: {chunk_id_padded}.chunk (decimal, zero-padded)
        aof_dir.join(format!("{:020}.chunk", chunk_id))
    }

    pub fn ensure_chunk(&self, spec: &RemoteChunkSpec) -> Result<LocalChunkHandle, AofError> {
        let aof_db_id: DbId = self.id.get();
        if spec.db_id != aof_db_id {
            return Err(AofError::WrongDatabase {
                db_id: spec.db_id,
                aof_id: self.id,
            });
        }
        let key = spec.cache_key();
        if let Some(handle) = self.local_chunks.get(&key) {
            return Ok(handle);
        }
        let remote = self
            .remote_chunks
            .as_ref()
            .ok_or(AofError::MissingRemoteFetcher)?;
        let handle = remote.hydrate(spec, &self.local_chunks)?;
        Ok(handle)
    }

    pub fn local_chunk_store(&self) -> Arc<LocalChunkStore> {
        self.local_chunks.clone()
    }

    pub fn close(&self) -> bool {
        if self.closed.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.shutdown_controllers();
        self.notify_manager();
        true
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

    fn shutdown_controllers(&self) {
        self.write_controller.shutdown();
        self.flush_controller.shutdown();
    }
}
impl Drop for AofInner {
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
    fn append_dispatches_writes() {
        let (manager, _manifest, _guard) = manager_with_manifest();
        let config = AofConfig::builder("append-write").build();
        let db = manager.open_db(config).expect("open db");

        // Append data - should automatically enqueue write
        let lsn = db.append(WriteChunk::Owned(vec![1, 2, 3])).expect("append");
        assert_eq!(lsn, 0);
        assert_eq!(db.tail_lsn(), 3);

        // Sync to make it durable
        let durable = db.sync().expect("sync");
        assert_eq!(durable, 3);

        manager.shutdown();
    }

    #[test]
    fn sync_advances_manifest() {
        let (manager, manifest, _guard) = manager_with_manifest();
        let config = AofConfig::builder("sync-flush").build();
        let db = manager.open_db(config).expect("open db");
        let db_id = db.id().get();

        // Append some data
        db.append(WriteChunk::Owned(vec![0u8; 256]))
            .expect("append");

        // Sync to make it durable
        let durable = db.sync().expect("sync");
        assert_eq!(durable, 256);

        // Wait for manifest to be updated (async commit from flush controller)
        wait_for(
            || {
                manifest
                    .aof_state(db_id)
                    .ok()
                    .flatten()
                    .map(|s| s.last_applied_lsn >= 256)
                    .unwrap_or(false)
            },
            Duration::from_secs(1),
        );

        // Check manifest was updated
        let state = manifest
            .aof_state(db_id)
            .expect("read aof state")
            .expect("aof state entry");
        assert_eq!(state.last_applied_lsn, 256);

        manager.shutdown();
    }
}
