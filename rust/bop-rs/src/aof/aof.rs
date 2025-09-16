//! Main AOF implementation with fully async operations and SegmentIndex as source of truth

use memmap2::{Mmap, MmapMut};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::{Notify, RwLock as TokioRwLock, mpsc, mpsc::error::TrySendError, oneshot};

use crate::aof::archive::ArchiveStorage;
use crate::aof::error::{AofError, AofResult};
use crate::aof::filesystem::{FileHandle, FileSystem};
use crate::aof::flush::{FlushController, FlushControllerMetrics};
use crate::aof::reader::{
    AofMetrics, Reader, ReaderLifecycleGuard, ReaderLifecycleHooks, SegmentPosition,
};
use crate::aof::record::{AofConfig, SegmentMetadata, ensure_segment_size_is_valid};
use crate::aof::record::{FlushStrategy, RecordHeader, current_timestamp};
use crate::aof::segment::Segment;
use crate::aof::segment_index::MdbxSegmentIndex;
use crate::aof::segment_store::SegmentEntry;

/// Reader-aware cached segment with reference counting
struct CachedSegment<FS: FileSystem> {
    /// The actual segment
    segment: Arc<Segment<FS>>,
    /// Number of active readers on this segment
    reader_count: usize,
    /// Last access timestamp for LRU eviction
    last_accessed: u64,
}

impl<FS: FileSystem> CachedSegment<FS> {
    fn new(segment: Arc<Segment<FS>>) -> Self {
        Self {
            segment,
            reader_count: 0,
            last_accessed: current_timestamp(),
        }
    }

    fn add_reader(&mut self) {
        self.reader_count += 1;
        self.last_accessed = current_timestamp();
    }

    fn remove_reader(&mut self) {
        self.reader_count = self.reader_count.saturating_sub(1);
        self.last_accessed = current_timestamp();
    }

    fn touch(&mut self) {
        self.last_accessed = current_timestamp();
    }

    fn can_evict(&self) -> bool {
        self.reader_count == 0
    }
}

/// Reader-aware segment cache with pinning
struct ReaderAwareSegmentCache<FS: FileSystem> {
    /// Cached segments with reader tracking
    segments: HashMap<u64, CachedSegment<FS>>,
    /// Readers to segments mapping
    reader_segments: HashMap<u64, HashSet<u64>>,
    /// Maximum cache size
    max_size: usize,
}

impl<FS: FileSystem> ReaderAwareSegmentCache<FS> {
    fn new(max_size: usize) -> Self {
        Self {
            segments: HashMap::new(),
            reader_segments: HashMap::new(),
            max_size,
        }
    }

    /// Get a segment, adding a reader reference
    fn get_segment(&mut self, segment_id: u64, reader_id: u64) -> Option<Arc<Segment<FS>>> {
        if let Some(cached) = self.segments.get_mut(&segment_id) {
            cached.add_reader();

            self.reader_segments
                .entry(reader_id)
                .or_insert_with(HashSet::new)
                .insert(segment_id);

            return Some(Arc::clone(&cached.segment));
        }
        None
    }

    /// Insert a segment into the cache
    fn insert_segment(&mut self, segment_id: u64, segment: Arc<Segment<FS>>) -> AofResult<()> {
        // Evict segments if at capacity
        if self.segments.len() >= self.max_size {
            self.evict_lru()?;
        }

        let cached = CachedSegment::new(segment);
        self.segments.insert(segment_id, cached);
        Ok(())
    }

    /// Remove a reader from a segment
    fn remove_reader_from_segment(&mut self, segment_id: u64, reader_id: u64) {
        if let Some(cached) = self.segments.get_mut(&segment_id) {
            cached.remove_reader();
        }

        // Remove from reader -> segment mapping
        if let Some(segments) = self.reader_segments.get_mut(&reader_id) {
            segments.remove(&segment_id);
            if segments.is_empty() {
                self.reader_segments.remove(&reader_id);
            }
        }
    }

    /// Remove all references for a reader (when reader is dropped)
    fn remove_reader(&mut self, reader_id: u64) {
        if let Some(segments) = self.reader_segments.remove(&reader_id) {
            for segment_id in segments {
                if let Some(cached) = self.segments.get_mut(&segment_id) {
                    cached.remove_reader();
                }
            }
        }
    }

    /// Evict least recently used segment that can be evicted
    fn evict_lru(&mut self) -> AofResult<()> {
        let mut lru_candidate = None;
        let mut lru_time = u64::MAX;

        for (&segment_id, cached) in &self.segments {
            if cached.can_evict() && cached.last_accessed < lru_time {
                lru_time = cached.last_accessed;
                lru_candidate = Some(segment_id);
            }
        }

        if let Some(segment_id) = lru_candidate {
            self.segments.remove(&segment_id);
        }

        Ok(())
    }
}

/// Registry tracking tail readers and their notification handles
struct ReaderRegistry {
    tail_readers: Mutex<HashMap<u64, Arc<Notify>>>,
}

impl ReaderRegistry {
    fn new() -> Self {
        Self {
            tail_readers: Mutex::new(HashMap::new()),
        }
    }

    fn register_tail_reader(&self, reader_id: u64, notify: Arc<Notify>) {
        let mut readers = self.tail_readers.lock().unwrap();
        readers.insert(reader_id, notify);
    }

    fn unregister_tail_reader(&self, reader_id: u64) {
        let mut readers = self.tail_readers.lock().unwrap();
        readers.remove(&reader_id);
    }

    fn tail_reader_ids(&self) -> Vec<u64> {
        let readers = self.tail_readers.lock().unwrap();
        readers.keys().copied().collect()
    }

    fn tail_notify(&self, reader_id: u64) -> Option<Arc<Notify>> {
        let readers = self.tail_readers.lock().unwrap();
        readers.get(&reader_id).map(Arc::clone)
    }
}

struct ReaderLifecycleManager<FS: FileSystem> {
    segment_cache: Arc<TokioRwLock<ReaderAwareSegmentCache<FS>>>,
    reader_registry: Arc<ReaderRegistry>,
    runtime_handle: Handle,
}

impl<FS: FileSystem> ReaderLifecycleManager<FS> {
    fn new(
        segment_cache: Arc<TokioRwLock<ReaderAwareSegmentCache<FS>>>,
        reader_registry: Arc<ReaderRegistry>,
        runtime_handle: Handle,
    ) -> Self {
        Self {
            segment_cache,
            reader_registry,
            runtime_handle,
        }
    }
}

impl<FS: FileSystem + 'static> ReaderLifecycleHooks for ReaderLifecycleManager<FS> {
    fn on_drop(&self, reader_id: u64) {
        self.reader_registry.unregister_tail_reader(reader_id);

        let cache = Arc::clone(&self.segment_cache);
        self.runtime_handle.spawn(async move {
            let mut cache = cache.write().await;
            cache.remove_reader(reader_id);
        });
    }
}

/// Active segment for writing with read-write mmap and shadow read-only segment
struct ActiveSegment<FS: FileSystem> {
    /// File handle for writing
    file: FS::Handle,
    /// Read-write memory map for writing
    write_mmap: Option<MmapMut>,
    /// Shadow read-only segment for readers
    shadow_segment: Option<Arc<Segment<FS>>>,
    /// Current segment metadata
    metadata: SegmentMetadata,
    /// Current offset in the file
    current_offset: u64,
    /// Notification for readers tailing this segment
    reader_notify: Arc<Notify>,
    /// Flag to track if notification is in progress
    notify_in_progress: AtomicBool,
}

impl<FS: FileSystem> ActiveSegment<FS> {
    /// Create a new active segment
    fn new(file: FS::Handle, metadata: SegmentMetadata) -> Self {
        Self {
            file,
            write_mmap: None,
            shadow_segment: None,
            metadata,
            current_offset: 0,
            reader_notify: Arc::new(Notify::new()),
            notify_in_progress: AtomicBool::new(false),
        }
    }

    /// Initialize the write mmap for this active segment
    async fn init_write_mmap(&mut self, segment_size: u64) -> AofResult<()> {
        // Set the file size before creating mmap
        self.file.set_size(segment_size).await?;

        self.write_mmap = Some(self.file.memory_map_mut().await?.ok_or_else(|| {
            AofError::FileSystem("Mutable memory mapping not supported".to_string())
        })?);
        Ok(())
    }

    /// Update the shadow segment for readers
    async fn update_shadow_segment(&mut self, fs: Arc<FS>) -> AofResult<Arc<Segment<FS>>> {
        // Create a new read-only segment for the same file using the correct path format
        let segment_path = format!("segment_{}.log", self.metadata.base_id);
        let shadow = Segment::open(&segment_path, fs, Some(self.metadata.clone())).await?;
        let arc = Arc::new(shadow);
        self.shadow_segment = Some(Arc::clone(&arc));
        Ok(arc)
    }

    /// Notify tailing readers for uncommitted appends
    fn notify_tailing_readers_append(&self) {
        // Use compare_and_swap to ensure only one notification is in progress
        if self
            .notify_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            self.reader_notify.notify_waiters();
            self.notify_in_progress.store(false, Ordering::Release);
        }
    }

    /// Notify tailing readers when records are committed
    fn notify_tailing_readers_commit(&self) {
        // Use compare_and_swap to ensure only one notification is in progress
        if self
            .notify_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            self.reader_notify.notify_waiters();
            self.notify_in_progress.store(false, Ordering::Release);
        }
    }

    /// Get the shadow segment for readers
    fn get_shadow_segment(&self) -> Option<Arc<Segment<FS>>> {
        self.shadow_segment.as_ref().map(Arc::clone)
    }

    /// Get the notification handle for readers
    fn get_reader_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.reader_notify)
    }

    /// Check if the mmap is ready for writing
    fn is_mmap_ready(&self) -> bool {
        self.write_mmap.is_some()
    }

    /// Write data directly to mmap (synchronous)
    fn write_to_mmap(&mut self, offset: usize, data: &[u8]) -> AofResult<()> {
        if let Some(ref mut mmap) = self.write_mmap {
            if offset + data.len() > mmap.len() {
                return Err(AofError::SegmentFull(
                    "Not enough space in mmap".to_string(),
                ));
            }
            mmap[offset..offset + data.len()].copy_from_slice(data);
            Ok(())
        } else {
            Err(AofError::WouldBlock("Mmap not ready".to_string()))
        }
    }

    /// Flush pending mmap changes to disk
    fn flush_mmap(&mut self) -> AofResult<()> {
        if let Some(ref mut mmap) = self.write_mmap {
            mmap.flush()
                .map_err(|e| AofError::FileSystem(format!("Failed to flush mmap: {}", e)))?;
        }
        Ok(())
    }

    /// Get available space in mmap
    fn available_space(&self) -> usize {
        if let Some(ref mmap) = self.write_mmap {
            mmap.len().saturating_sub(self.current_offset as usize)
        } else {
            0
        }
    }
}

/// Performance metrics for the AOF
#[derive(Debug)]
pub struct AofPerformanceMetrics {
    pub total_appends: AtomicU64,
    pub total_reads: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub segments_created: AtomicU64,
    pub segments_finalized: AtomicU64,
    pub segments_archived: AtomicU64,

    /// Comprehensive flush metrics from FlushController
    pub flush_metrics: FlushControllerMetrics,
}

impl Default for AofPerformanceMetrics {
    fn default() -> Self {
        Self {
            total_appends: AtomicU64::new(0),
            total_reads: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            segments_created: AtomicU64::new(0),
            segments_finalized: AtomicU64::new(0),
            segments_archived: AtomicU64::new(0),
            flush_metrics: FlushControllerMetrics::default(),
        }
    }
}

/// Background task request types
pub struct PreAllocationRequest<FS: FileSystem> {
    pub segment_size: u64,
    pub base_id_hint: u64, // Hint for what the base_id might be
    pub response: oneshot::Sender<AofResult<ActiveSegment<FS>>>,
}

pub struct FinalizationRequest<FS: FileSystem> {
    pub segment: ActiveSegment<FS>,
    pub response: oneshot::Sender<AofResult<SegmentEntry>>,
}

pub struct FlushRequest {
    pub segments: Vec<SegmentEntry>,
    pub response: oneshot::Sender<AofResult<()>>,
}

pub struct ArchiveRequest {
    pub segment_entry: SegmentEntry,
    pub response: oneshot::Sender<AofResult<()>>,
}

pub struct ReaderNotificationRequest {
    pub segment_id: u64,
    pub new_record_count: u64,
    pub readers_to_notify: Vec<u64>, // Reader IDs that might be tailing this segment
}

/// Background task failure tracking
#[derive(Debug, Default)]
pub struct BackgroundTaskErrors {
    pub pre_allocation: Option<String>,
    pub finalization: Option<String>,
    pub flush: Option<String>,
    pub archive: Option<String>,
}

/// Background task coordinator channels
pub struct BackgroundTaskChannels<FS: FileSystem + 'static> {
    pub pre_allocate_tx: mpsc::Sender<PreAllocationRequest<FS>>,
    pub finalize_tx: mpsc::Sender<FinalizationRequest<FS>>,
    pub flush_tx: mpsc::Sender<FlushRequest>,
    pub archive_tx: mpsc::Sender<ArchiveRequest>,
    pub reader_notify_tx: mpsc::Sender<ReaderNotificationRequest>,

    // Error reporting from background tasks
    pub error_rx: mpsc::Receiver<BackgroundTaskErrors>,

    // Task handles for cleanup
    pub task_handles: Vec<tokio::task::JoinHandle<()>>,
}

/// Shared state that background tasks need access to
pub struct SharedAofState<FS: FileSystem, A: ArchiveStorage> {
    pub config: AofConfig,
    pub local_fs: Arc<FS>,
    pub archive_storage: Arc<A>,
    pub segment_index: Arc<MdbxSegmentIndex>,
    pub metrics: Arc<AofPerformanceMetrics>,
    pub errors: Arc<TokioRwLock<BackgroundTaskErrors>>,
    pub reader_registry: Arc<ReaderRegistry>,
}

/// The main AOF (Append-Only File) implementation
pub struct Aof<FS: FileSystem + 'static, A: ArchiveStorage + 'static> {
    /// Configuration
    config: AofConfig,
    /// Local filesystem for segment files
    local_fs: Arc<FS>,
    /// Archive storage for completed segments
    archive_storage: Arc<A>,
    /// MDBX-based segment index
    segment_index: Arc<MdbxSegmentIndex>,
    /// Current active segment for writing
    active_segment: Option<ActiveSegment<FS>>,
    /// Pre-allocated next segment (ready to become active)
    next_segment: Option<ActiveSegment<FS>>,
    /// Reader-aware segment cache with pinning
    segment_cache: Arc<TokioRwLock<ReaderAwareSegmentCache<FS>>>,
    /// Registry tracking reader notification handles
    reader_registry: Arc<ReaderRegistry>,
    /// Lifecycle management for reader registration and cleanup
    reader_lifecycle: Arc<ReaderLifecycleManager<FS>>,
    /// Pending segment metadata updates awaiting flush
    pending_flush_segments: Mutex<HashMap<u64, SegmentEntry>>,
    /// Next record ID counter
    next_record_id: AtomicU64,
    /// Reader ID counter
    next_reader_id: AtomicU64,
    /// Flush strategy
    flush_strategy: FlushStrategy,
    /// Flush controller for managing flush operations and metrics
    flush_controller: FlushController,
    /// Unflushed record count for batched flushing
    unflushed_count: AtomicU64,
    /// Flag to track if a flush is in progress (prevents multiple concurrent flushes)
    flush_in_progress: AtomicBool,
    /// Performance metrics
    metrics: Arc<AofPerformanceMetrics>,
    /// Background task channels
    background_channels: Option<BackgroundTaskChannels<FS>>,
    /// Background task error state
    background_errors: Arc<TokioRwLock<BackgroundTaskErrors>>,
    /// Pending background pre-allocation result
    pending_preallocation: Mutex<Option<oneshot::Receiver<AofResult<ActiveSegment<FS>>>>>,
    /// Flag to track if AOF has been properly closed
    closed: std::sync::atomic::AtomicBool,
}

impl<FS: FileSystem + 'static, A: ArchiveStorage + 'static> Aof<FS, A> {
    /// Creates or opens an AOF log with custom configuration and file system.
    pub async fn open_with_fs_and_config(
        fs: FS,
        archive_storage: Arc<A>,
        config: AofConfig,
    ) -> AofResult<Self> {
        ensure_segment_size_is_valid(config.segment_size)?;

        if config.segment_cache_size == 0 {
            return Err(AofError::InvalidSegmentSize(
                "Segment cache size must be non-zero".to_string(),
            ));
        }

        // Use configured index path or default
        let index_path = config
            .index_path
            .clone()
            .unwrap_or_else(|| "aof_segment_index.mdbx".to_string());

        // Initialize the segment index
        let segment_index = Arc::new(MdbxSegmentIndex::open(&index_path)?);

        let local_fs = Arc::new(fs);
        let segment_cache = Arc::new(TokioRwLock::new(ReaderAwareSegmentCache::new(
            config.segment_cache_size,
        )));
        let reader_registry = Arc::new(ReaderRegistry::new());
        let reader_lifecycle = Arc::new(ReaderLifecycleManager::new(
            Arc::clone(&segment_cache),
            Arc::clone(&reader_registry),
            Handle::current(),
        ));

        let mut aof = Aof {
            config: config.clone(),
            local_fs,
            archive_storage,
            segment_index,
            active_segment: None,
            next_segment: None,
            segment_cache,
            reader_registry,
            reader_lifecycle,
            pending_flush_segments: Mutex::new(HashMap::new()),
            next_record_id: AtomicU64::new(1),
            next_reader_id: AtomicU64::new(0),
            flush_strategy: config.flush_strategy.clone(),
            flush_controller: FlushController::new(config.flush_strategy.clone()),
            unflushed_count: AtomicU64::new(0),
            flush_in_progress: AtomicBool::new(false),
            metrics: Arc::new(AofPerformanceMetrics::default()),
            background_channels: None,
            background_errors: Arc::new(TokioRwLock::new(BackgroundTaskErrors::default())),
            pending_preallocation: Mutex::new(None),
            closed: std::sync::atomic::AtomicBool::new(false),
        };

        // Load existing segments and determine next record ID
        aof.load_existing_segments().await?;

        // Initialize background tasks
        aof.initialize_background_tasks().await?;

        Ok(aof)
    }

    /// Initialize background tasks for pre-allocation, finalization, flushing, and archiving
    async fn initialize_background_tasks(&mut self) -> AofResult<()> {
        // Create channels
        let (pre_allocate_tx, pre_allocate_rx) = mpsc::channel::<PreAllocationRequest<FS>>(1);
        let (finalize_tx, finalize_rx) = mpsc::channel::<FinalizationRequest<FS>>(1);
        let (flush_tx, flush_rx) = mpsc::channel::<FlushRequest>(1);
        let (archive_tx, archive_rx) = mpsc::channel::<ArchiveRequest>(1);
        let (reader_notify_tx, reader_notify_rx) = mpsc::channel::<ReaderNotificationRequest>(1); // Only 1 inflight
        let (_error_tx, error_rx) = mpsc::channel::<BackgroundTaskErrors>(1);

        // Create shared state
        let shared_state = Arc::new(SharedAofState {
            config: self.config.clone(),
            local_fs: self.local_fs.clone(),
            archive_storage: self.archive_storage.clone(),
            segment_index: self.segment_index.clone(),
            metrics: self.metrics.clone(),
            errors: self.background_errors.clone(),
            reader_registry: Arc::clone(&self.reader_registry),
        });

        let mut task_handles = Vec::new();

        // Spawn pre-allocation task (critical priority - blocking)
        let pre_alloc_state = shared_state.clone();
        let pre_alloc_handle = tokio::task::spawn_blocking(move || {
            Self::pre_allocation_task(pre_alloc_state, pre_allocate_rx)
        });
        task_handles.push(pre_alloc_handle);

        // Spawn finalization task (high priority - blocking)
        let finalize_state = shared_state.clone();
        let finalize_handle = tokio::task::spawn_blocking(move || {
            Self::finalization_task(finalize_state, finalize_rx)
        });
        task_handles.push(finalize_handle);

        // Spawn flush task (medium priority - blocking)
        let flush_state = shared_state.clone();
        let flush_handle =
            tokio::task::spawn_blocking(move || Self::flush_task(flush_state, flush_rx));
        task_handles.push(flush_handle);

        // Spawn archive task (low priority - blocking)
        let archive_state = shared_state.clone();
        let archive_handle = tokio::task::spawn_blocking(move || {
            Self::archive_task_blocking(archive_state, archive_rx)
        });
        task_handles.push(archive_handle);

        // Spawn reader notification task (fire-and-forget - async)
        let reader_notify_state = shared_state.clone();
        let reader_notify_handle = tokio::spawn(async move {
            Self::reader_notification_task(reader_notify_state, reader_notify_rx).await
        });
        task_handles.push(reader_notify_handle);

        // Store channels
        self.background_channels = Some(BackgroundTaskChannels {
            pre_allocate_tx,
            finalize_tx,
            flush_tx,
            archive_tx,
            reader_notify_tx,
            error_rx,
            task_handles,
        });

        Ok(())
    }

    /// Load existing segments from index and set next record ID using O(1) recovery
    async fn load_existing_segments(&mut self) -> AofResult<()> {
        // Use O(1) recovery to find max last_id without loading all segments
        let max_record_id = match self.segment_index.get_max_last_id()? {
            Some(last_id) => last_id,
            None => 0, // No segments exist yet
        };

        // Check if there's an active segment that needs recovery
        if let Some(active_entry) = self.segment_index.get_active_segment()? {
            self.recover_active_segment(active_entry).await?;
        } else {
            self.next_record_id
                .store(max_record_id + 1, Ordering::SeqCst);
        }

        Ok(())
    }

    async fn recover_active_segment(&mut self, entry: SegmentEntry) -> AofResult<()> {
        let segment_path = entry
            .local_path
            .clone()
            .unwrap_or_else(|| format!("segment_{}.log", entry.base_id));

        let metadata = Self::segment_entry_to_metadata(&entry);
        let file = self.local_fs.open_file_mut(&segment_path).await?;
        let mut active = ActiveSegment::new(file, metadata.clone());
        active.current_offset = entry.original_size;
        active.metadata.last_id = entry.last_id;
        active.metadata.record_count = entry.record_count;
        active.metadata.size = entry.original_size;

        active.init_write_mmap(self.config.segment_size).await?;
        let shadow = active
            .update_shadow_segment(Arc::clone(&self.local_fs))
            .await?;
        self.cache_segment(entry.base_id, Arc::clone(&shadow))
            .await?;

        self.next_record_id
            .store(entry.last_id + 1, Ordering::SeqCst);
        self.active_segment = Some(active);
        self.trigger_pre_allocation();

        Ok(())
    }

    /// Check if flush is needed based on strategy.
    pub fn should_flush(&self) -> bool {
        self.flush_controller.should_flush()
    }

    /// Get the configuration of this AOF instance.
    pub fn config(&self) -> &AofConfig {
        &self.config
    }

    /// Get the next record ID.
    pub fn next_id(&self) -> u64 {
        self.next_record_id.load(Ordering::SeqCst)
    }

    /// Get access to the flush controller metrics
    pub fn flush_metrics(&self) -> &FlushControllerMetrics {
        self.flush_controller.metrics()
    }

    /// Get performance metrics
    pub fn performance_metrics(&self) -> &Arc<AofPerformanceMetrics> {
        &self.metrics
    }

    /// Create a reader starting from a specific record ID (thread-safe).
    /// Create a reader starting from a specific record ID (thread-safe).
    pub fn create_reader_from_id(&self, record_id: u64) -> AofResult<Reader<FS>> {
        let reader_id = self.next_reader_id.fetch_add(1, Ordering::SeqCst);
        let mut reader = Reader::new(reader_id);
        self.attach_reader_lifecycle(&mut reader, reader_id);

        if let Some(entry) = self.segment_index.find_segment_for_id(record_id)? {
            self.position_reader_in_segment(&mut reader, &entry, Some(record_id))?;
        }

        Ok(reader)
    }

    /// Create a reader starting from a specific timestamp (thread-safe).
    pub fn create_reader_from_timestamp(&self, timestamp: u64) -> AofResult<Reader<FS>> {
        let reader_id = self.next_reader_id.fetch_add(1, Ordering::SeqCst);
        let mut reader = Reader::new(reader_id);
        self.attach_reader_lifecycle(&mut reader, reader_id);

        if let Some(entry) = self
            .segment_index
            .find_segments_for_timestamp(timestamp)?
            .into_iter()
            .min_by_key(|entry| entry.created_at)
        {
            self.position_reader_in_segment(&mut reader, &entry, None)?;
        }

        Ok(reader)
    }

    /// Create a reader starting at the current tail (most recent record) - thread-safe.
    pub fn create_reader_at_tail(&self) -> AofResult<Reader<FS>> {
        let reader_id = self.next_reader_id.fetch_add(1, Ordering::SeqCst);
        let mut reader = Reader::new(reader_id);
        self.attach_reader_lifecycle(&mut reader, reader_id);

        let tail_notify = Arc::new(Notify::new());
        reader.set_tail_notify(Arc::clone(&tail_notify));

        if let Some(ref active) = self.active_segment {
            if let Some(ref shadow) = active.shadow_segment {
                reader.set_current_segment(Arc::clone(shadow), active.current_offset);
                reader.set_position(SegmentPosition {
                    segment_id: active.metadata.base_id,
                    file_offset: active.current_offset,
                    record_id: active.metadata.last_id,
                });
                reader.is_tailing.store(true, Ordering::SeqCst);
                self.reader_registry
                    .register_tail_reader(reader_id, Arc::clone(&tail_notify));
                return Ok(reader);
            }
        }

        if let Some(entry) = self.segment_index.get_latest_finalized_segment()? {
            self.position_reader_in_segment(&mut reader, &entry, None)?;
        }

        reader.is_tailing.store(true, Ordering::SeqCst);
        self.reader_registry
            .register_tail_reader(reader_id, tail_notify);
        Ok(reader)
    }

    fn attach_reader_lifecycle(&self, reader: &mut Reader<FS>, reader_id: u64) {
        let guard = ReaderLifecycleGuard::new(
            reader_id,
            Arc::clone(&self.reader_lifecycle) as Arc<dyn ReaderLifecycleHooks>,
        );
        reader.set_lifecycle_guard(guard);
    }

    async fn cache_segment(&self, segment_id: u64, segment: Arc<Segment<FS>>) -> AofResult<()> {
        let mut cache = self.segment_cache.write().await;
        cache.insert_segment(segment_id, segment)
    }

    fn position_reader_in_segment(
        &self,
        reader: &mut Reader<FS>,
        entry: &SegmentEntry,
        target_record: Option<u64>,
    ) -> AofResult<()> {
        let segment = self.get_segment_for_reader(entry, reader.id)?;
        let offset = if let Some(target) = target_record {
            Self::find_record_offset_in_segment(&segment, target)?
        } else {
            segment.first_record_offset()
        };

        reader.set_current_segment(Arc::clone(&segment), offset);
        let initial_record = target_record.unwrap_or(entry.base_id).saturating_sub(1);
        reader.set_position(SegmentPosition {
            segment_id: entry.base_id,
            file_offset: offset,
            record_id: initial_record,
        });

        Ok(())
    }

    fn get_segment_for_reader(
        &self,
        entry: &SegmentEntry,
        reader_id: u64,
    ) -> AofResult<Arc<Segment<FS>>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.get_or_load_segment(entry, reader_id))
        })
    }

    async fn get_or_load_segment(
        &self,
        entry: &SegmentEntry,
        reader_id: u64,
    ) -> AofResult<Arc<Segment<FS>>> {
        {
            let mut cache = self.segment_cache.write().await;
            if let Some(segment) = cache.get_segment(entry.base_id, reader_id) {
                return Ok(segment);
            }
        }

        let segment_arc = Arc::new(self.open_segment_for_reader(entry).await?);

        let mut cache = self.segment_cache.write().await;
        cache.insert_segment(entry.base_id, Arc::clone(&segment_arc))?;
        if let Some(segment) = cache.get_segment(entry.base_id, reader_id) {
            Ok(segment)
        } else {
            Ok(segment_arc)
        }
    }

    async fn open_segment_for_reader(&self, entry: &SegmentEntry) -> AofResult<Segment<FS>> {
        let segment_path = entry
            .local_path
            .clone()
            .unwrap_or_else(|| format!("segment_{}.log", entry.base_id));
        let metadata = Self::segment_entry_to_metadata(entry);
        Segment::open(&segment_path, Arc::clone(&self.local_fs), Some(metadata)).await
    }

    fn segment_entry_to_metadata(entry: &SegmentEntry) -> SegmentMetadata {
        SegmentMetadata {
            base_id: entry.base_id,
            last_id: entry.last_id,
            record_count: entry.record_count,
            created_at: entry.created_at,
            size: entry.original_size,
            checksum: entry.uncompressed_checksum as u32,
            compressed: entry.compressed_size.is_some(),
            encrypted: false,
        }
    }

    fn metadata_to_active_entry(metadata: &SegmentMetadata) -> SegmentEntry {
        SegmentEntry::new_active(
            metadata,
            format!("segment_{}.log", metadata.base_id),
            metadata.checksum as u64,
        )
    }

    fn enqueue_flush_entry(&self, entry: SegmentEntry) {
        let mut pending = self.pending_flush_segments.lock().unwrap();
        pending.insert(entry.base_id, entry);
    }

    fn drain_pending_flush_entries(&self) -> Vec<SegmentEntry> {
        let mut pending = self.pending_flush_segments.lock().unwrap();
        pending.drain().map(|(_, entry)| entry).collect()
    }

    fn restore_pending_flush_entries(&self, entries: Vec<SegmentEntry>) {
        if entries.is_empty() {
            return;
        }
        let mut pending = self.pending_flush_segments.lock().unwrap();
        for entry in entries {
            pending.insert(entry.base_id, entry);
        }
    }

    fn persist_flush_entries(&self, entries: &[SegmentEntry]) -> AofResult<()> {
        for entry in entries {
            self.segment_index.update_segment_entry(entry)?;
        }
        Ok(())
    }

    fn find_record_offset_in_segment(segment: &Segment<FS>, target_id: u64) -> AofResult<u64> {
        let data = segment.get_mmap_data()?;
        let segment_size = segment.metadata().size as usize;
        let mut offset = 0usize;
        let header_size = RecordHeader::size();

        while offset + header_size <= segment_size {
            let header_bytes = &data[offset..offset + header_size];
            let header = RecordHeader::from_bytes(header_bytes).map_err(|e| {
                AofError::CorruptedRecord(format!("Failed to deserialize header: {}", e))
            })?;

            if header.id >= target_id {
                return Ok(offset as u64);
            }

            offset += header_size + header.size as usize;
        }

        Ok(segment_size as u64)
    }

    /// Appends a record to the AOF.
    /// Synchronous append operation using mmap
    /// Returns WouldBlock if no active segment is ready for writing
    /// Returns errors from background tasks that prevent forward progress
    pub fn append(&mut self, data: &[u8]) -> AofResult<u64> {
        // Check for background task errors that prevent forward progress
        self.check_background_task_errors()?;

        // Check archive backpressure if archiving is enabled
        if self.config.archive_enabled {
            self.check_archive_backpressure()?;
        }

        let record_id = self.next_record_id.fetch_add(1, Ordering::SeqCst);

        // Create record header
        let header = RecordHeader::new(record_id, data);
        let header_bytes = bincode::serialize(&header)
            .map_err(|e| AofError::Serialization(format!("Failed to serialize header: {}", e)))?;

        let total_size = header_bytes.len() + data.len();

        let prepare_result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.prepare_active_segment_for_append(total_size))
        });
        if let Err(err) = prepare_result {
            if let AofError::WouldBlock(_) = &err {
                self.trigger_pre_allocation();
            }
            return Err(err);
        }

        let active_segment = match &mut self.active_segment {
            Some(active) if active.is_mmap_ready() => active,
            _ => {
                self.trigger_pre_allocation();
                return Err(AofError::WouldBlock(
                    "No active segment ready for writing".to_string(),
                ));
            }
        };

        let current_offset = active_segment.current_offset as usize;

        // Write to mmap
        active_segment.write_to_mmap(current_offset, &header_bytes)?;
        active_segment.write_to_mmap(current_offset + header_bytes.len(), data)?;

        // Update metadata
        active_segment.current_offset += total_size as u64;
        active_segment.metadata.last_id = record_id;
        active_segment.metadata.record_count += 1;
        active_segment.metadata.size = active_segment.current_offset;

        // Queue updated metadata for background flush/index persistence
        let flush_entry = Self::metadata_to_active_entry(&active_segment.metadata);
        self.enqueue_flush_entry(flush_entry);

        // Increment unflushed count
        self.unflushed_count.fetch_add(1, Ordering::Relaxed);

        // Record the append with flush controller for metrics
        self.flush_controller.record_append(total_size as u64);

        // Extract notification data while we still have the mutable borrow
        let segment_id = active_segment.metadata.base_id;
        let new_record_count = active_segment.metadata.record_count;

        // Check if pre-allocation is needed
        let should_pre_allocate =
            self.config.pre_allocate_enabled && self.next_segment.is_none() && {
                let current_usage = active_segment.current_offset as f32;
                let total_capacity = self.config.segment_size as f32;
                let usage_percentage = current_usage / total_capacity;
                usage_percentage >= self.config.pre_allocate_threshold
            };

        if should_pre_allocate {
            self.trigger_pre_allocation();
        }

        // Trigger async flush if needed (without awaiting)
        if self.should_flush() {
            self.trigger_background_flush();
        }

        self.metrics.total_appends.fetch_add(1, Ordering::Relaxed);

        // Notify tailing readers of new data (fire-and-forget background task)
        let readers_to_notify = self.reader_registry.tail_reader_ids();
        self.trigger_reader_notification(segment_id, new_record_count, readers_to_notify);

        Ok(record_id)
    }

    /// Check for background task errors that prevent forward progress
    fn check_background_task_errors(&self) -> AofResult<()> {
        let errors = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.background_errors.read())
        });

        if let Some(message) = &errors.pre_allocation {
            return Err(AofError::InternalError(format!(
                "Pre-allocation task failed: {}",
                message
            )));
        }
        if let Some(message) = &errors.finalization {
            return Err(AofError::InternalError(format!(
                "Finalization task failed: {}",
                message
            )));
        }
        if let Some(message) = &errors.flush {
            return Err(AofError::InternalError(format!(
                "Flush task failed: {}",
                message
            )));
        }
        if let Some(message) = &errors.archive {
            return Err(AofError::InternalError(format!(
                "Archive task failed: {}",
                message
            )));
        }

        Ok(())
    }

    /// Check archive backpressure and return error if queue is too full
    fn check_archive_backpressure(&self) -> AofResult<()> {
        if !self.config.archive_enabled {
            return Ok(());
        }

        // Check segment count backpressure
        let segment_count = self.segment_index.get_segment_count()?;
        if segment_count > self.config.archive_backpressure_segments {
            return Err(AofError::Backpressure(format!(
                "Archive queue too full: {} segments (max {})",
                segment_count, self.config.archive_backpressure_segments
            )));
        }

        // TODO: Check bytes backpressure
        // This would require tracking total unarchived bytes

        Ok(())
    }

    /// Trigger pre-allocation of next segment
    fn trigger_pre_allocation(&self) {
        if self.next_segment.is_some() {
            return;
        }

        if let Some(ref channels) = self.background_channels {
            let mut pending = self.pending_preallocation.lock().unwrap();
            if pending.is_some() {
                return;
            }

            let next_base_id = self.next_record_id.load(Ordering::SeqCst) + 1000; // Rough estimate
            let (response_tx, response_rx) = oneshot::channel();

            let request = PreAllocationRequest {
                segment_size: self.config.segment_size,
                base_id_hint: next_base_id,
                response: response_tx,
            };

            if channels.pre_allocate_tx.try_send(request).is_ok() {
                *pending = Some(response_rx);
            }
        }
    }

    /// Trigger background flush
    fn trigger_background_flush(&self) {
        if let Some(ref channels) = self.background_channels {
            let segments = self.drain_pending_flush_entries();
            if segments.is_empty() {
                return;
            }

            let (response_tx, _response_rx) = oneshot::channel();
            let request = FlushRequest {
                segments,
                response: response_tx,
            };

            if let Err(err) = channels.flush_tx.try_send(request) {
                match err {
                    TrySendError::Full(mut req) => {
                        let entries = req.segments.drain(..).collect();
                        self.restore_pending_flush_entries(entries);
                    }
                    TrySendError::Closed(req) => {
                        self.restore_pending_flush_entries(req.segments);
                    }
                }
            }
        }
    }

    /// Trigger reader notification (fire-and-forget)
    fn trigger_reader_notification(
        &self,
        segment_id: u64,
        new_record_count: u64,
        readers_to_notify: Vec<u64>,
    ) {
        if let Some(ref channels) = self.background_channels {
            let request = ReaderNotificationRequest {
                segment_id,
                new_record_count,
                readers_to_notify,
            };

            // Fire-and-forget: try_send and ignore if channel is full (1 inflight message limit)
            // If the channel is full, it means there's already a notification being processed
            let _ = channels.reader_notify_tx.try_send(request);
        }
    }

    /// Create a new active segment for writing
    async fn create_new_active_segment(&mut self, base_id: u64) -> AofResult<()> {
        let segment_path = format!("segment_{}.log", base_id);
        let file = self.local_fs.create_file(&segment_path).await?;

        let metadata = SegmentMetadata {
            base_id,
            last_id: base_id,
            record_count: 0,
            created_at: current_timestamp(),
            size: 0,
            checksum: 0,
            compressed: false,
            encrypted: false,
        };

        self.active_segment = Some(ActiveSegment::new(file, metadata));

        Ok(())
    }

    /// Finalize the current active segment and optionally archive
    async fn finalize_active_segment(&mut self) -> AofResult<()> {
        if let Some(mut active) = self.active_segment.take() {
            // Flush the file
            active.flush_mmap()?;
            active.file.flush().await?;
            active.file.sync().await?;

            // Update metadata size and finalization time
            let mut metadata = active.metadata;
            metadata.size = active.current_offset;
            let finalized_at = current_timestamp();

            // Create segment entry
            let segment_path = format!("segment_{}.log", metadata.base_id);
            let uncompressed_checksum = 0; // TODO: Calculate checksum

            let segment_entry = SegmentEntry::new_finalized(
                &metadata,
                segment_path,
                uncompressed_checksum,
                finalized_at,
            );

            // Add to segment index
            self.segment_index.add_segment_entry(&segment_entry)?;
            self.metrics
                .segments_finalized
                .fetch_add(1, Ordering::Relaxed);

            // Check if archiving is needed
            if self.config.archive_enabled {
                self.check_and_archive_segments().await?;
            }
        }
        Ok(())
    }

    /// Check if segments should be archived and perform archiving
    async fn check_and_archive_segments(&mut self) -> AofResult<()> {
        let segment_count = self.segment_index.get_segment_count()?;

        if segment_count >= self.config.archive_threshold {
            // Get segments to archive (oldest finalized segments)
            let segments_to_archive = self
                .segment_index
                .get_segments_for_archiving(segment_count - self.config.archive_threshold + 1)?;

            for segment_entry in segments_to_archive {
                if !segment_entry.is_archived() {
                    self.archive_segment(segment_entry).await?;
                }
            }
        }
        Ok(())
    }

    /// Archive a single segment to storage
    async fn archive_segment(&mut self, mut segment_entry: SegmentEntry) -> AofResult<()> {
        let segment_path = segment_entry
            .local_path
            .as_ref()
            .ok_or_else(|| AofError::Other("Segment has no local path".to_string()))?;

        // Read segment data
        let segment_data = self
            .local_fs
            .open_file(segment_path)
            .await?
            .read_all()
            .await?;

        // Compress data if enabled
        let (archive_data, compressed_size, compressed_checksum) =
            if self.config.archive_compression_level > 0 {
                // TODO: Implement compression with zstd
                // For now, store uncompressed
                (segment_data.clone(), None, None)
            } else {
                (segment_data, None, None)
            };

        // Generate archive key
        let archive_key = format!(
            "segment_{}_{}.log",
            segment_entry.base_id,
            segment_entry.finalized_at.unwrap_or(0)
        );

        // Store in archive
        let segment_path = format!("segment_{}.log", segment_entry.base_id);
        self.archive_storage
            .store_segment(&segment_path, &archive_data, &archive_key)
            .await?;

        // Update segment entry
        segment_entry.archive(
            archive_key,
            current_timestamp(),
            compressed_size,
            compressed_checksum,
        );

        // Update in index
        self.segment_index.update_segment_entry(&segment_entry)?;

        self.metrics
            .segments_archived
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Flushes all pending writes to disk and performs incremental archiving
    pub async fn flush(&mut self) -> AofResult<()> {
        use std::time::Instant;

        let start_time = Instant::now();
        let pending_records = self
            .flush_controller
            .metrics()
            .pending_records
            .load(Ordering::Relaxed);
        let pending_bytes = self
            .flush_controller
            .metrics()
            .pending_bytes
            .load(Ordering::Relaxed);

        let flush_result = async {
            if let Some(ref mut active) = self.active_segment {
                active.flush_mmap()?;
                active.file.flush().await?;
                active.file.sync().await?;
                self.unflushed_count.store(0, Ordering::Relaxed);
            }

            let flush_entries = self.drain_pending_flush_entries();
            if let Err(e) = self.persist_flush_entries(&flush_entries) {
                self.restore_pending_flush_entries(flush_entries);
                return Err(e);
            }

            // Perform incremental archiving during flush
            if self.config.archive_enabled {
                self.incremental_archive().await?;
            }

            Ok::<(), AofError>(())
        }
        .await;

        let latency_us = start_time.elapsed().as_micros() as u64;

        match flush_result {
            Ok(()) => {
                // Record successful flush
                self.flush_controller
                    .record_flush(latency_us, pending_records, pending_bytes);
                Ok(())
            }
            Err(e) => {
                // Record failed flush
                self.flush_controller.record_flush_failure(latency_us);
                Err(e)
            }
        }
    }

    /// Perform incremental archiving - archive one segment per call
    async fn incremental_archive(&mut self) -> AofResult<()> {
        let segment_count = self.segment_index.get_segment_count()?;

        if segment_count > self.config.archive_threshold {
            // Archive just one segment per flush call for incremental processing
            let segments_to_archive = self.segment_index.get_segments_for_archiving(1)?;

            if let Some(segment_entry) = segments_to_archive.into_iter().next() {
                if !segment_entry.is_archived() {
                    self.archive_segment(segment_entry).await?;
                }
            }
        }

        Ok(())
    }

    /// Trigger asynchronous flush without blocking append operations
    fn trigger_async_flush(&self) {
        // Use atomic compare-exchange to ensure only one flush runs at a time
        if self
            .flush_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            // In a full implementation, this would spawn a background task.
            // For now, we'll mark the flush as complete immediately since we can't
            // spawn async tasks from a sync context without proper architecture.
            // The flush will happen on the next explicit flush() call.
            self.flush_in_progress.store(false, Ordering::Release);

            // TODO: For production use, implement one of these approaches:
            // 1. Background flush thread with channel communication
            // 2. AOF service with async task spawning capability
            // 3. Flush queue processed by external scheduler
        }
    }

    /// Closes the AOF and ensures all data is properly persisted for recovery.
    /// This method should be called before dropping the AOF to ensure proper cleanup.
    pub async fn close(mut self) -> AofResult<()> {
        // Flush any pending data
        self.flush().await?;

        // Mark as closed
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);

        Ok(())
    }

    /// Check if pre-allocation is needed based on current segment usage
    fn should_pre_allocate(&self, active_segment: &ActiveSegment<FS>) -> bool {
        if !self.config.pre_allocate_enabled || self.next_segment.is_some() {
            return false;
        }

        let current_usage = active_segment.current_offset as f32;
        let segment_size = self.config.segment_size as f32;
        let usage_ratio = current_usage / segment_size;

        usage_ratio >= self.config.pre_allocate_threshold
    }

    /// Pre-allocate the next segment asynchronously
    async fn pre_allocate_next_segment(&mut self) -> AofResult<()> {
        if self.next_segment.is_some() {
            return Ok(()); // Already pre-allocated
        }

        let next_base_id = self.next_record_id.load(Ordering::SeqCst) + 1000; // Estimate next base ID
        let segment_path = format!("segment_{}.log", next_base_id);
        let file = self.local_fs.create_file(&segment_path).await?;

        let metadata = SegmentMetadata {
            base_id: next_base_id,
            last_id: next_base_id,
            record_count: 0,
            created_at: current_timestamp(),
            size: 0,
            checksum: 0,
            compressed: false,
            encrypted: false,
        };

        let mut next_segment = ActiveSegment::new(file, metadata);
        next_segment
            .init_write_mmap(self.config.segment_size)
            .await?;
        let _ = next_segment
            .update_shadow_segment(Arc::clone(&self.local_fs))
            .await?;

        self.next_segment = Some(next_segment);
        Ok(())
    }

    /// Rotate to pre-allocated segment or create new one
    async fn rotate_to_next_segment(&mut self) -> AofResult<()> {
        // Finalize current active segment
        self.finalize_active_segment().await?;

        // Use pre-allocated segment if available
        if let Some(next_segment) = self.next_segment.take() {
            // Update base_id to current record ID
            let actual_base_id = self.next_record_id.load(Ordering::SeqCst);
            let mut next_segment = next_segment;
            next_segment.metadata.base_id = actual_base_id;
            next_segment.metadata.last_id = actual_base_id;

            self.active_segment = Some(next_segment);
        } else {
            // Create new segment if pre-allocation wasn't ready
            let base_id = self.next_record_id.load(Ordering::SeqCst);
            self.create_new_active_segment(base_id).await?;
        }

        self.trigger_pre_allocation();

        Ok(())
    }

    /// Wait for the next active segment to be ready, creating it if necessary
    pub async fn wait_for_active_segment(&mut self) -> AofResult<()> {
        if let Some(ref active) = self.active_segment {
            if active.current_offset >= self.config.segment_size {
                self.rotate_to_next_segment().await?;
            }
        }

        if self.next_segment.is_none() {
            let pending = {
                let mut guard = self.pending_preallocation.lock().unwrap();
                guard.take()
            };

            if let Some(receiver) = pending {
                match receiver.await {
                    Ok(Ok(segment)) => {
                        self.next_segment = Some(segment);
                    }
                    Ok(Err(err)) => {
                        return Err(err);
                    }
                    Err(_) => {
                        return Err(AofError::InternalError(
                            "Pre-allocation channel closed unexpectedly".to_string(),
                        ));
                    }
                }
            }
        }

        if self.active_segment.is_none() {
            let base_id = self.next_record_id.load(Ordering::SeqCst);
            self.create_new_active_segment(base_id).await?;
        }

        let mut cache_entry: Option<(u64, Arc<Segment<FS>>)> = None;
        if let Some(ref mut active) = self.active_segment {
            if !active.is_mmap_ready() {
                active.init_write_mmap(self.config.segment_size).await?;
                let shadow = active
                    .update_shadow_segment(Arc::clone(&self.local_fs))
                    .await?;
                cache_entry = Some((active.metadata.base_id, Arc::clone(&shadow)));
            } else if let Some(shadow) = active.get_shadow_segment() {
                cache_entry = Some((active.metadata.base_id, shadow));
            }
        }

        if let Some((segment_id, segment)) = cache_entry {
            self.cache_segment(segment_id, segment).await?;
        }

        self.trigger_pre_allocation();

        Ok(())
    }

    async fn prepare_active_segment_for_append(&mut self, required_size: usize) -> AofResult<()> {
        if required_size > self.config.segment_size as usize {
            return Err(AofError::SegmentFull(format!(
                "Record size {} exceeds segment capacity {}",
                required_size, self.config.segment_size
            )));
        }

        loop {
            self.wait_for_active_segment().await?;

            let rotate = if let Some(active) = self.active_segment.as_ref() {
                let available = active.available_space();
                let projected_offset = active.current_offset as usize + required_size;
                available < required_size || projected_offset > self.config.segment_size as usize
            } else {
                true
            };

            if rotate {
                self.rotate_to_next_segment().await?;
                continue;
            }

            break;
        }

        Ok(())
    }

    /// Trigger pre-allocation if needed (can be called from background task)
    pub async fn trigger_pre_allocation_if_needed(&mut self) -> AofResult<()> {
        if let Some(ref active) = self.active_segment {
            if self.should_pre_allocate(active) {
                self.pre_allocate_next_segment().await?;
            }
        }
        Ok(())
    }

    /// Creates a new reader.
    pub fn create_reader(&mut self) -> Reader<FS> {
        let reader_id = self.next_reader_id.fetch_add(1, Ordering::SeqCst);
        let mut reader = Reader::new(reader_id);
        self.attach_reader_lifecycle(&mut reader, reader_id);
        reader
    }

    // Direct read_record method removed - AOF only supports Reader-based access

    /*
    /// Read from a segment using the segment entry from the index
    /// TODO: Implement this method using SegmentStore API
    async fn read_from_segment_entry(&mut self, id: u64, entry: crate::aof::segment_store::SegmentEntry) -> AofResult<Option<Vec<u8>>> {
        let base_id = entry.base_id;

        // Check cache first
        {
            let mut cache = self.segment_cache.lock().await;
            if let Some(segment) = cache.get_mut(&base_id) {
                self.metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
                if let Ok(Some(record)) = segment.read(id) {
                    return Ok(Some(record.data.to_vec()));
                }
            }
        }

        // Load from disk
        self.metrics.cache_misses.fetch_add(1, Ordering::Relaxed);
        let segment_path = format!("{}.log", base_id);

        // Convert SegmentEntry to SegmentMetadata
        let metadata = SegmentMetadata {
            base_id: entry.base_id,
            last_id: entry.last_id,
            record_count: entry.record_count,
            created_at: entry.created_at,
            size: entry.original_size,
            checksum: entry.uncompressed_checksum as u32,
            compressed: entry.compressed_size.is_some(),
            encrypted: false, // Encryption not yet implemented
        };

        match tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                Segment::open(&segment_path, self.fs.clone(), Some(metadata))
            )
        }) {
            Ok(mut segment) => {
                let result = segment.read(id);
                if let Ok(Some(record)) = result {
                    let data = record.data.to_vec();
                    // Add to cache
                    {
                        let mut cache = self.segment_cache.lock().await;
                        cache.put(base_id, segment);
                    }
                    return Ok(Some(data));
                }
            }
            Err(_) => {
                // Segment file may not exist or be corrupted
            }
        }

        Ok(None)
    }
    */

    /*
    /// Read from a completed segment at a specific offset
    /// TODO: Implement this method using SegmentStore API
    async fn read_from_segment_offset(
        &mut self,
        segment_id: u64,
        file_offset: u64,
        expected_record_id: u64,
        reader: &Reader,
    ) -> AofResult<Option<(u64, Vec<u8>)>> {
        // Get segment entry from segment store
        let segment_entry = self.segment_store.get_segment(segment_id)?;

        let Some(entry) = segment_entry else {
            return Ok(None);
        };

        // Check cache first
        {
            let mut cache = self.segment_cache.lock().await;
            if let Some(segment) = cache.get_mut(&segment_id) {
                self.metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
                if let Some((record_id, data, next_offset)) = segment.read_at_offset(file_offset)? {
                    if record_id == expected_record_id {
                        reader.advance(next_offset, record_id + 1);
                        return Ok(Some((record_id, data)));
                    }
                }
                return Ok(None);
            }
        }

        // Load from disk if not in cache
        self.metrics.cache_misses.fetch_add(1, Ordering::Relaxed);
        let segment_path = format!("{}.log", segment_id);

        // Convert SegmentEntry to SegmentMetadata
        let metadata = SegmentMetadata {
            base_id: entry.base_id,
            last_id: entry.last_id,
            record_count: entry.record_count,
            created_at: entry.created_at,
            size: entry.original_size,
            checksum: entry.uncompressed_checksum as u32,
            compressed: entry.compressed_size.is_some(),
            encrypted: false, // Encryption not yet implemented
        };

        match tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                Segment::open(&segment_path, self.fs.clone(), Some(metadata))
            )
        }) {
            Ok(mut segment) => {
                if let Some((record_id, data, next_offset)) = segment.read_at_offset(file_offset)? {
                    if record_id == expected_record_id {
                        // Cache the segment
                        {
                            let mut cache = self.segment_cache.lock().await;
                            cache.put(segment_id, segment);
                        }
                        reader.advance(next_offset, record_id + 1);
                        return Ok(Some((record_id, data)));
                    }
                }
            }
            Err(_) => {
                // Segment file may not exist
            }
        }

        Ok(None)
    }
    */

    /*
    /// Find the next segment for the reader to move to
    /// TODO: Implement this method using SegmentStore API
    async fn find_next_segment_for_reader(&mut self, reader: &Reader) -> AofResult<Option<(u64, Vec<u8>)>> {
        let current_pos = reader.position();

        // Find the next segment after the current one using the segment store
        let next_segment = self.segment_store.get_next_segment_after(current_pos.segment_id)?;

        if let Some(next_entry) = next_segment {
            // Move reader to the beginning of the next segment
            reader.move_to_segment(next_entry.base_id, 0, next_entry.base_id);

            // Try to read the first record of the next segment
            return self.read_from_segment_offset(next_entry.base_id, 0, next_entry.base_id, reader).await;
        }

        // No more segments available
        Ok(None)
    }
    */

    // Background task implementations

    /// Pre-allocation task - runs on blocking thread pool
    fn pre_allocation_task(
        shared_state: Arc<SharedAofState<FS, A>>,
        mut rx: mpsc::Receiver<PreAllocationRequest<FS>>,
    ) {
        let rt = tokio::runtime::Handle::current();
        while let Some(request) = rt.block_on(rx.recv()) {
            let result = rt.block_on(async {
                // Create new segment file
                let segment_path = format!("segment_{}.log", request.base_id_hint);
                let file = shared_state.local_fs.create_file(&segment_path).await?;

                let metadata = SegmentMetadata {
                    base_id: request.base_id_hint,
                    last_id: request.base_id_hint,
                    record_count: 0,
                    created_at: current_timestamp(),
                    size: 0,
                    checksum: 0,
                    compressed: false,
                    encrypted: false,
                };

                let mut active = ActiveSegment::new(file, metadata);

                // Initialize mmap for writing
                active.init_write_mmap(request.segment_size).await?;

                shared_state
                    .metrics
                    .segments_created
                    .fetch_add(1, Ordering::Relaxed);
                Ok::<_, AofError>(active)
            });

            match result {
                Ok(segment) => {
                    let _ = request.response.send(Ok(segment));
                    let _ = rt.block_on(async {
                        let mut errors = shared_state.errors.write().await;
                        errors.pre_allocation = None;
                    });
                }
                Err(err) => {
                    let message = err.to_string();
                    let _ = rt.block_on(async {
                        let mut errors = shared_state.errors.write().await;
                        errors.pre_allocation = Some(message);
                    });
                    let _ = request.response.send(Err(err));
                }
            }
        }
    }

    /// Finalization task - runs on blocking thread pool
    fn finalization_task(
        shared_state: Arc<SharedAofState<FS, A>>,
        mut rx: mpsc::Receiver<FinalizationRequest<FS>>,
    ) {
        let rt = tokio::runtime::Handle::current();
        while let Some(request) = rt.block_on(rx.recv()) {
            let result = rt.block_on(async {
                let mut segment = request.segment;

                // Flush the file
                segment.file.flush().await?;

                // Update metadata
                let mut metadata = segment.metadata;
                metadata.size = segment.current_offset;
                let finalized_at = current_timestamp();

                // Create segment entry
                let segment_path = format!("segment_{}.log", metadata.base_id);
                let uncompressed_checksum = 0; // TODO: Calculate checksum

                let segment_entry = SegmentEntry::new_finalized(
                    &metadata,
                    segment_path,
                    finalized_at,
                    uncompressed_checksum,
                );

                // Add to segment index
                shared_state
                    .segment_index
                    .add_segment_entry(&segment_entry)?;

                shared_state
                    .metrics
                    .segments_finalized
                    .fetch_add(1, Ordering::Relaxed);
                Ok::<_, AofError>(segment_entry)
            });

            match result {
                Ok(entry) => {
                    let _ = request.response.send(Ok(entry));
                    let _ = rt.block_on(async {
                        let mut errors = shared_state.errors.write().await;
                        errors.finalization = None;
                    });
                }
                Err(err) => {
                    let message = err.to_string();
                    let _ = rt.block_on(async {
                        let mut errors = shared_state.errors.write().await;
                        errors.finalization = Some(message);
                    });
                    let _ = request.response.send(Err(err));
                }
            }
        }
    }

    /// Flush task - runs on blocking thread pool
    fn flush_task(shared_state: Arc<SharedAofState<FS, A>>, mut rx: mpsc::Receiver<FlushRequest>) {
        let rt = tokio::runtime::Handle::current();
        while let Some(request) = rt.block_on(rx.recv()) {
            let result = rt.block_on(async {
                // Perform flush operations for segments
                for segment_entry in &request.segments {
                    // Update segment index
                    shared_state
                        .segment_index
                        .update_segment_entry(segment_entry)?;
                }

                // Sync the segment index
                shared_state.segment_index.sync()?;

                Ok::<_, AofError>(())
            });

            match result {
                Ok(()) => {
                    let _ = request.response.send(Ok(()));
                    let _ = rt.block_on(async {
                        let mut errors = shared_state.errors.write().await;
                        errors.flush = None;
                    });
                }
                Err(err) => {
                    let message = err.to_string();
                    let _ = rt.block_on(async {
                        let mut errors = shared_state.errors.write().await;
                        errors.flush = Some(message);
                    });
                    let _ = request.response.send(Err(err));
                }
            }
        }
    }

    /// Archive task - runs as blocking task
    fn archive_task_blocking(
        shared_state: Arc<SharedAofState<FS, A>>,
        mut rx: mpsc::Receiver<ArchiveRequest>,
    ) {
        let rt = tokio::runtime::Handle::current();
        while let Some(request) = rt.block_on(rx.recv()) {
            let result = rt.block_on(async {
                let mut segment_entry = request.segment_entry;

                // Read segment data
                let segment_path = format!("segment_{}.log", segment_entry.base_id);
                let segment_data = shared_state
                    .local_fs
                    .open_file(&segment_path)
                    .await?
                    .read_all()
                    .await?;

                // Compress data (TODO: implement compression)
                let archive_data = segment_data; // For now, no compression
                let compressed_size = archive_data.len() as u64;
                let compressed_checksum = 0; // TODO: Calculate checksum

                // Generate archive key
                let archive_key = format!(
                    "segment_{}_{}.log",
                    segment_entry.base_id,
                    segment_entry.finalized_at.unwrap_or(0)
                );

                // Store in archive
                let segment_path = format!("segment_{}.log", segment_entry.base_id);
                shared_state
                    .archive_storage
                    .store_segment(&segment_path, &archive_data, &archive_key)
                    .await?;

                // Update segment entry
                segment_entry.archive(
                    archive_key,
                    current_timestamp(),
                    Some(compressed_size),
                    Some(compressed_checksum),
                );

                // Update in index
                shared_state
                    .segment_index
                    .update_segment_entry(&segment_entry)?;

                // Delete local file
                shared_state.local_fs.delete_file(&segment_path).await?;

                shared_state
                    .metrics
                    .segments_archived
                    .fetch_add(1, Ordering::Relaxed);
                Ok::<_, AofError>(())
            });

            match result {
                Ok(()) => {
                    let _ = request.response.send(Ok(()));
                    let _ = rt.block_on(async {
                        let mut errors = shared_state.errors.write().await;
                        errors.archive = None;
                    });
                }
                Err(err) => {
                    let message = err.to_string();
                    let _ = rt.block_on(async {
                        let mut errors = shared_state.errors.write().await;
                        errors.archive = Some(message);
                    });
                    let _ = request.response.send(Err(err));
                }
            }
        }
    }

    /// Reader notification task - runs as async task (fire-and-forget)
    async fn reader_notification_task(
        shared_state: Arc<SharedAofState<FS, A>>,
        mut rx: mpsc::Receiver<ReaderNotificationRequest>,
    ) {
        // Only 1 inflight message, so we process one at a time
        while let Some(request) = rx.recv().await {
            for reader_id in request.readers_to_notify {
                if let Some(notify) = shared_state.reader_registry.tail_notify(reader_id) {
                    notify.notify_waiters();
                }
            }
            // Update metrics if needed
            // shared_state.metrics.reader_notifications_sent.fetch_add(request.readers_to_notify.len() as u64, Ordering::Relaxed);
        }
    }
}

// AofReader implementation removed - AOF only supports Reader-based access

/*
impl<FS: FileSystem + 'static, A: ArchiveStorage + 'static> AofSegmentReader<FS> for Aof<FS, A> {
    fn read_at_segment_offset(
        &mut self,
        segment_id: u64,
        file_offset: u64,
        expected_record_id: u64,
        reader: &Reader
    ) -> AofResult<Option<(u64, Vec<u8>)>> {
        // Note: TailAppender handles active segment reads through segment cache

        // Check if this is a completed segment
        return tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                self.read_from_segment_offset(segment_id, file_offset, expected_record_id, reader)
            )
        });
    }
}
*/

impl<FS: FileSystem + 'static, A: ArchiveStorage + 'static> Aof<FS, A> {
    /// Read from the active writer segment at a specific offset (handled by TailAppender)
    fn read_from_active_segment_offset(
        &mut self,
        _file_offset: u64,
        _expected_record_id: u64,
        _reader: &Reader<FS>,
    ) -> AofResult<Option<(u64, Vec<u8>)>> {
        // TailAppender handles active segment reads through segment cache
        Ok(None)
    }
}

impl<FS: FileSystem + 'static, A: ArchiveStorage + 'static> AofMetrics for Aof<FS, A> {
    fn get_total_records(&self) -> u64 {
        self.next_record_id.load(Ordering::SeqCst).saturating_sub(1)
    }
}

impl<FS: FileSystem + 'static, A: ArchiveStorage + 'static> Drop for Aof<FS, A> {
    fn drop(&mut self) {
        // Only warn if AOF was not properly closed
        if !self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            eprintln!(
                "WARNING: AOF dropped without calling close() - some data may not be persisted"
            );
            eprintln!("HINT: Call aof.close().await before dropping to ensure data integrity");
        }
    }
}
