//! Virtual WAL Implementation
//!
//! Implements the libsql WAL interface backed by our branched storage engine.
//!
//! # Architecture
//!
//! The VirtualWal maintains:
//! - In-memory frame index: page_no -> (frame_no, frame_data)
//! - Branch context: which database/branch this WAL belongs to
//! - Connection to the underlying chunk store for checkpointing
//! - Async storage backend for page fallback (via sync_await)
//!
//! # Page Lookup Path
//!
//! 1. Check in-memory WAL frames (uncommitted + recent committed)
//! 2. Fall through to chunk store via page_manager (async via sync_await)
//!
//! # Frame Management
//!
//! Frames are written to memory first, then:
//! - On commit: frames are persisted and replicated via raft
//! - On checkpoint: frames are merged into chunk store
//!
//! # Async I/O Integration
//!
//! When running inside a maniac stackful coroutine, page reads that miss
//! the in-memory cache will use `sync_await` to load from async storage:
//!
//! ```text
//! SQLite read_page() -> find_frame() -> not in WAL
//!                                    -> fallback_read_page()
//!                                    -> sync_await(load_page_async())
//!                                    -> yield coroutine on I/O
//!                                    -> resume when data ready
//!                                    -> return to SQLite
//! ```

use crate::async_io::{AsyncChunkStorage, SyncAsyncBridge};
use crate::page_manager::ChunkConfig;
use crate::raft_log::BranchRef;
use crate::store::Store;
use crate::wal_storage::{WalStorageBackend, WalWriter, WalWriterConfig};
use dashmap::DashMap;
use maniac_libsql_ffi::{SQLITE_BUSY, WAL_SAVEPOINT_NDATA};
use maniac_libsql_sys::wal::{
    BusyHandler, CheckpointCallback, CheckpointMode, PageHeaders, Result as WalResult, Sqlite3Db,
    Sqlite3File, UndoHandler, Vfs, Wal, WalManager,
};
use std::ffi::{c_int, CStr};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Frame data stored in the WAL.
#[derive(Clone, Debug)]
pub struct WalFrame {
    /// The page number this frame belongs to.
    pub page_no: u32,
    /// The page data.
    pub data: Vec<u8>,
    /// Database size after this frame (in pages), or 0 if not a commit frame.
    pub db_size: u32,
}

/// Information about which chunk contains a page.
#[derive(Clone, Copy, Debug)]
pub struct ChunkLocation {
    /// Starting frame of the chunk.
    pub start_frame: u64,
    /// Ending frame of the chunk.
    pub end_frame: u64,
}

/// In-memory WAL state for a branch.
///
/// This is shared across all connections to the same branch.
pub struct WalState {
    /// Database/branch this WAL belongs to.
    pub branch: BranchRef,
    /// Page size for this database.
    pub page_size: u32,
    /// Current frame count (mxFrame).
    pub max_frame: AtomicU32,
    /// Map from frame_no -> frame data.
    pub frames: DashMap<u32, Arc<WalFrame>>,
    /// Map from page_no -> latest frame_no containing that page.
    pub page_index: DashMap<u32, u32>,
    /// Number of frames that have been checkpointed.
    pub checkpointed_frame: AtomicU32,
    /// Database size in pages (nBackfill).
    pub db_size: AtomicU32,
    /// Write lock holder (0 = no holder).
    pub write_lock: AtomicU64,
    /// Read transaction count.
    pub read_txn_count: AtomicU32,
    /// Chunk index: maps page_no -> chunk location for pages in storage.
    /// Pages in this index are NOT in the in-memory frames - they've been
    /// checkpointed to chunk storage.
    pub chunk_index: DashMap<u32, ChunkLocation>,
    /// WAL writer for persisting frames to remote storage.
    /// Protected by mutex since it's accessed from multiple connections.
    pub(crate) wal_writer: Mutex<Option<WalWriterHolder>>,
}

/// Holds a type-erased WAL writer for persistence.
///
/// This allows WalState to be generic-agnostic while still supporting
/// WAL persistence through the WalStorageBackend trait.
pub(crate) struct WalWriterHolder {
    /// The actual writer, boxed for type erasure.
    inner: Box<dyn WalWriterOps + Send>,
}

/// Operations we need from a WalWriter, type-erased.
pub(crate) trait WalWriterOps: Send {
    /// Append a frame to the pending buffer.
    fn append_frame(&mut self, frame: WalFrame) -> u64;
    /// Check if a flush should be triggered.
    fn should_flush(&self) -> bool;
    /// Get the last durable frame number.
    fn durable_frame_no(&self) -> u64;
    /// Get the pending frame count.
    fn pending_count(&self) -> usize;
}

impl<B: WalStorageBackend> WalWriterOps for WalWriter<B> {
    fn append_frame(&mut self, frame: WalFrame) -> u64 {
        // Sync buffer-only operation - no I/O
        WalWriter::append_frame(self, frame)
    }

    fn should_flush(&self) -> bool {
        WalWriter::should_flush(self)
    }

    fn durable_frame_no(&self) -> u64 {
        // Returns remote durable frame number
        WalWriter::durable_frame_no(self)
    }

    fn pending_count(&self) -> usize {
        WalWriter::pending_count(self)
    }
}

impl WalWriterHolder {
    /// Create a new holder wrapping a WalWriter.
    pub fn new<B: WalStorageBackend>(writer: WalWriter<B>) -> Self {
        Self {
            inner: Box::new(writer),
        }
    }

    /// Append a frame and return the assigned frame number.
    pub fn append_frame(&mut self, frame: WalFrame) -> u64 {
        self.inner.append_frame(frame)
    }

    /// Check if we should flush.
    pub fn should_flush(&self) -> bool {
        self.inner.should_flush()
    }

    /// Get the durable frame number.
    pub fn durable_frame_no(&self) -> u64 {
        self.inner.durable_frame_no()
    }

    /// Get pending count.
    pub fn pending_count(&self) -> usize {
        self.inner.pending_count()
    }
}

impl WalState {
    pub fn new(branch: BranchRef, page_size: u32) -> Self {
        Self {
            branch,
            page_size,
            max_frame: AtomicU32::new(0),
            frames: DashMap::new(),
            page_index: DashMap::new(),
            checkpointed_frame: AtomicU32::new(0),
            db_size: AtomicU32::new(0),
            write_lock: AtomicU64::new(0),
            read_txn_count: AtomicU32::new(0),
            chunk_index: DashMap::new(),
            wal_writer: Mutex::new(None),
        }
    }

    /// Set the WAL writer for this state.
    pub fn set_wal_writer<B: WalStorageBackend>(&self, writer: WalWriter<B>) {
        let mut guard = self.wal_writer.lock().unwrap();
        *guard = Some(WalWriterHolder::new(writer));
    }

    /// Check if WAL persistence is configured.
    pub fn has_wal_writer(&self) -> bool {
        self.wal_writer.lock().unwrap().is_some()
    }

    /// Get the durable frame number from the WAL writer.
    pub fn durable_frame_no(&self) -> Option<u64> {
        self.wal_writer
            .lock()
            .unwrap()
            .as_ref()
            .map(|w| w.durable_frame_no())
    }

    /// Check if the WAL writer should flush.
    pub fn should_flush_wal(&self) -> bool {
        self.wal_writer
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|w| w.should_flush())
    }

    /// Queue frames for persistence.
    /// This is called after frames are inserted into the in-memory WAL.
    pub(crate) fn queue_frames_for_persistence(&self, frames: &[WalFrame]) {
        let mut guard = self.wal_writer.lock().unwrap();
        if let Some(ref mut writer) = *guard {
            for frame in frames {
                writer.append_frame(frame.clone());
            }
        }
    }

    /// Look up which chunk contains a given page.
    /// Returns None if the page is only in-memory or doesn't exist.
    pub fn find_chunk(&self, page_no: u32) -> Option<ChunkLocation> {
        self.chunk_index.get(&page_no).map(|r| *r)
    }

    /// Register that a page has been checkpointed to a chunk.
    pub fn register_page_in_chunk(&self, page_no: u32, location: ChunkLocation) {
        self.chunk_index.insert(page_no, location);
    }

    /// Find the frame containing a page.
    pub fn find_frame(&self, page_no: u32) -> Option<u32> {
        self.page_index.get(&page_no).map(|r| *r)
    }

    /// Read frame data.
    pub fn read_frame(&self, frame_no: u32) -> Option<Arc<WalFrame>> {
        self.frames.get(&frame_no).map(|r| r.clone())
    }

    /// Insert frames into the WAL.
    /// Returns the new max frame number.
    pub fn insert_frames(&self, new_frames: Vec<WalFrame>) -> u32 {
        // Queue frames for persistence before modifying in-memory state
        self.queue_frames_for_persistence(&new_frames);

        let mut max_frame = self.max_frame.load(Ordering::SeqCst);

        for frame in new_frames {
            max_frame += 1;
            let page_no = frame.page_no;
            let db_size = frame.db_size;

            self.frames.insert(max_frame, Arc::new(frame));
            self.page_index.insert(page_no, max_frame);

            if db_size > 0 {
                self.db_size.store(db_size, Ordering::SeqCst);
            }
        }

        self.max_frame.store(max_frame, Ordering::SeqCst);
        max_frame
    }

    /// Undo frames back to a savepoint.
    pub fn undo_to(&self, savepoint_frame: u32) {
        let max_frame = self.max_frame.load(Ordering::SeqCst);

        // Collect pages that need their index updated
        let mut pages_to_update: Vec<(u32, Option<u32>)> = Vec::new();

        // Remove frames after savepoint
        for frame_no in (savepoint_frame + 1)..=max_frame {
            if let Some((_, frame)) = self.frames.remove(&frame_no) {
                // Check if this was the latest frame for the page
                if let Some(entry) = self.page_index.get(&frame.page_no) {
                    if *entry == frame_no {
                        // Find previous frame for this page by scanning from savepoint down
                        let mut prev_frame: Option<u32> = None;
                        for prev_no in (1..=savepoint_frame).rev() {
                            if let Some(prev) = self.frames.get(&prev_no) {
                                if prev.page_no == frame.page_no {
                                    prev_frame = Some(prev_no);
                                    break;
                                }
                            }
                        }
                        pages_to_update.push((frame.page_no, prev_frame));
                    }
                }
            }
        }

        // Update page index
        for (page_no, prev_frame) in pages_to_update {
            if let Some(prev) = prev_frame {
                self.page_index.insert(page_no, prev);
            } else {
                self.page_index.remove(&page_no);
            }
        }

        self.max_frame.store(savepoint_frame, Ordering::SeqCst);
    }

    /// Try to acquire write lock.
    pub fn try_write_lock(&self, conn_id: u64) -> bool {
        self.write_lock
            .compare_exchange(0, conn_id, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Release write lock.
    pub fn release_write_lock(&self, conn_id: u64) {
        let _ = self
            .write_lock
            .compare_exchange(conn_id, 0, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Check if we hold the write lock.
    pub fn holds_write_lock(&self, conn_id: u64) -> bool {
        self.write_lock.load(Ordering::SeqCst) == conn_id
    }
}

/// Virtual WAL Manager.
///
/// Creates VirtualWal instances for database connections.
/// Supports optional async storage backend for page fallback.
pub struct VirtualWalManager<S: AsyncChunkStorage = NoopAsyncStorage> {
    /// Reference to the storage layer.
    store: Store,
    /// Shared WAL state per branch.
    /// Key: (db_id, branch_id)
    wal_states: DashMap<(u64, u32), Arc<WalState>>,
    /// Optional async storage bridge for page fallback.
    async_bridge: Option<SyncAsyncBridge<S>>,
    /// Chunk configuration for async storage.
    chunk_config: ChunkConfig,
    /// Pages per chunk for async storage calculations.
    pages_per_chunk: u32,
}

/// No-op async storage for when async backend is not needed.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopAsyncStorage;

impl AsyncChunkStorage for NoopAsyncStorage {
    async fn read_chunk(
        &self,
        _key: &crate::chunk_storage::ChunkKey,
    ) -> crate::chunk_storage::StorageResult<Vec<u8>> {
        Err(crate::chunk_storage::StorageError::NotFound(
            "NoopAsyncStorage".to_string(),
        ))
    }

    async fn write_chunk(
        &self,
        _key: &crate::chunk_storage::ChunkKey,
        _data: Vec<u8>,
    ) -> crate::chunk_storage::StorageResult<()> {
        Ok(())
    }

    async fn chunk_exists(
        &self,
        _key: &crate::chunk_storage::ChunkKey,
    ) -> crate::chunk_storage::StorageResult<bool> {
        Ok(false)
    }

    async fn delete_chunk(
        &self,
        _key: &crate::chunk_storage::ChunkKey,
    ) -> crate::chunk_storage::StorageResult<()> {
        Ok(())
    }
}

impl VirtualWalManager<NoopAsyncStorage> {
    /// Create a new VirtualWalManager without async storage backend.
    pub fn new(store: Store) -> Self {
        Self {
            store,
            wal_states: DashMap::new(),
            async_bridge: None,
            chunk_config: ChunkConfig::default(),
            pages_per_chunk: 64,
        }
    }
}

impl<S: AsyncChunkStorage> VirtualWalManager<S> {
    /// Create a new VirtualWalManager with async storage backend.
    pub fn with_async_storage(
        store: Store,
        async_bridge: SyncAsyncBridge<S>,
        chunk_config: ChunkConfig,
        pages_per_chunk: u32,
    ) -> Self {
        Self {
            store,
            wal_states: DashMap::new(),
            async_bridge: Some(async_bridge),
            chunk_config,
            pages_per_chunk,
        }
    }

    /// Get or create WAL state for a branch.
    pub fn get_or_create_state(&self, branch: BranchRef, page_size: u32) -> Arc<WalState> {
        let key = (branch.db_id, branch.branch_id);
        self.wal_states
            .entry(key)
            .or_insert_with(|| Arc::new(WalState::new(branch, page_size)))
            .clone()
    }

    /// Get the store reference.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Get the async bridge if available.
    pub fn async_bridge(&self) -> Option<&SyncAsyncBridge<S>> {
        self.async_bridge.as_ref()
    }

    /// Configure WAL persistence for a branch.
    ///
    /// This sets up a `WalWriter` to batch and flush frames to remote storage
    /// on a periodic basis.
    ///
    /// # Arguments
    ///
    /// * `branch` - The branch to configure
    /// * `page_size` - Page size for this branch
    /// * `backend` - The storage backend to use
    /// * `config` - Optional writer configuration (uses defaults if None)
    pub fn configure_wal_persistence<B: WalStorageBackend>(
        &self,
        branch: BranchRef,
        page_size: u32,
        backend: Arc<B>,
        config: Option<WalWriterConfig>,
    ) {
        let state = self.get_or_create_state(branch, page_size);
        let config = config.unwrap_or_else(|| WalWriterConfig {
            page_size,
            ..Default::default()
        });
        let writer = WalWriter::new(branch, backend, config);
        state.set_wal_writer(writer);
    }

    /// Get the WAL state for a branch if it exists.
    pub fn get_state(&self, db_id: u64, branch_id: u32) -> Option<Arc<WalState>> {
        self.wal_states.get(&(db_id, branch_id)).map(|r| r.clone())
    }

    /// Check if a branch should flush its WAL.
    pub fn should_flush(&self, db_id: u64, branch_id: u32) -> bool {
        self.wal_states
            .get(&(db_id, branch_id))
            .is_some_and(|state| state.should_flush_wal())
    }

    /// Get all branches that should be flushed.
    pub fn branches_needing_flush(&self) -> Vec<BranchRef> {
        self.wal_states
            .iter()
            .filter(|entry| entry.value().should_flush_wal())
            .map(|entry| entry.value().branch)
            .collect()
    }
}

impl<S: AsyncChunkStorage + Clone> Clone for VirtualWalManager<S> {
    fn clone(&self) -> Self {
        // Clone shares the same WAL states
        Self {
            store: self.store.clone(),
            wal_states: self.wal_states.clone(),
            async_bridge: self.async_bridge.clone(),
            chunk_config: self.chunk_config.clone(),
            pages_per_chunk: self.pages_per_chunk,
        }
    }
}

impl<S: AsyncChunkStorage + Clone + 'static> WalManager for VirtualWalManager<S> {
    type Wal = VirtualWal<S>;

    fn use_shared_memory(&self) -> bool {
        false // We don't use shared memory
    }

    fn open(
        &self,
        _vfs: &mut Vfs,
        _file: &mut Sqlite3File,
        _no_shm_mode: c_int,
        _max_log_size: i64,
        db_path: &CStr,
    ) -> WalResult<Self::Wal> {
        // Parse branch info from db_path
        // Format: "/db/{db_id}/branch/{branch_id}/main.db"
        let path = db_path.to_string_lossy();
        let (db_id, branch_id, page_size) = parse_db_path(&path)
            .ok_or_else(|| maniac_libsql_ffi::Error::new(maniac_libsql_ffi::SQLITE_CANTOPEN))?;

        let branch = BranchRef::new(db_id, branch_id);
        let state = self.get_or_create_state(branch, page_size);

        // Generate unique connection ID
        static CONN_COUNTER: AtomicU64 = AtomicU64::new(1);
        let conn_id = CONN_COUNTER.fetch_add(1, Ordering::SeqCst);

        Ok(VirtualWal {
            state,
            conn_id,
            in_read_txn: false,
            in_write_txn: false,
            read_mark: 0,
            async_bridge: self.async_bridge.clone(),
        })
    }

    fn close(
        &self,
        wal: &mut Self::Wal,
        _db: &mut Sqlite3Db,
        _sync_flags: c_int,
        _scratch: Option<&mut [u8]>,
    ) -> WalResult<()> {
        // Release any locks
        if wal.in_write_txn {
            wal.state.release_write_lock(wal.conn_id);
        }
        if wal.in_read_txn {
            wal.state.read_txn_count.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }

    fn destroy_log(&self, _vfs: &mut Vfs, _db_path: &CStr) -> WalResult<()> {
        // We don't actually destroy anything - the storage layer handles cleanup
        Ok(())
    }

    fn log_exists(&self, _vfs: &mut Vfs, _db_path: &CStr) -> WalResult<bool> {
        // Our virtual WAL always "exists"
        Ok(true)
    }

    fn destroy(self) {
        // Nothing to destroy
    }
}

/// Virtual WAL instance for a single connection.
pub struct VirtualWal<S: AsyncChunkStorage = NoopAsyncStorage> {
    /// Shared WAL state.
    pub(crate) state: Arc<WalState>,
    /// Unique connection ID.
    pub(crate) conn_id: u64,
    /// Whether we're in a read transaction.
    pub(crate) in_read_txn: bool,
    /// Whether we're in a write transaction.
    pub(crate) in_write_txn: bool,
    /// Read mark (max frame visible to this transaction).
    pub(crate) read_mark: u32,
    /// Optional async storage bridge for page fallback.
    pub(crate) async_bridge: Option<SyncAsyncBridge<S>>,
}

unsafe impl<S: AsyncChunkStorage> Send for VirtualWal<S> {}

impl<S: AsyncChunkStorage> VirtualWal<S> {
    /// Try to load a page from async storage.
    ///
    /// This is called when a page is not found in the in-memory WAL frames.
    /// It uses `sync_await` to bridge the async storage operation.
    ///
    /// Returns None if:
    /// - No async bridge is configured
    /// - The page is not in the chunk index
    /// - We're not running in a coroutine context
    #[cfg(feature = "maniac-runtime")]
    pub fn try_load_page_from_storage(&self, page_no: u32) -> Option<Vec<u8>> {
        let bridge = self.async_bridge.as_ref()?;
        let location = self.state.find_chunk(page_no)?;

        bridge
            .try_load_page_sync(
                self.state.branch,
                location.start_frame,
                location.end_frame,
                page_no,
            )
            .and_then(|result| result.ok())
    }

    /// Load a page from async storage (blocking).
    ///
    /// # Panics
    ///
    /// Panics if called outside of a maniac coroutine context.
    #[cfg(feature = "maniac-runtime")]
    pub fn load_page_from_storage(&self, page_no: u32) -> Option<Vec<u8>> {
        let bridge = self.async_bridge.as_ref()?;
        let location = self.state.find_chunk(page_no)?;

        bridge
            .load_page_sync(
                self.state.branch,
                location.start_frame,
                location.end_frame,
                page_no,
            )
            .ok()
    }

    /// Load a page - stub for when maniac runtime is not available.
    #[cfg(not(feature = "maniac-runtime"))]
    pub fn try_load_page_from_storage(&self, _page_no: u32) -> Option<Vec<u8>> {
        None
    }

    /// Load a page - stub for when maniac runtime is not available.
    #[cfg(not(feature = "maniac-runtime"))]
    pub fn load_page_from_storage(&self, _page_no: u32) -> Option<Vec<u8>> {
        None
    }

    /// Check if this WAL has async storage configured.
    pub fn has_async_storage(&self) -> bool {
        self.async_bridge.is_some()
    }

    /// Get the branch reference for this WAL.
    pub fn branch(&self) -> BranchRef {
        self.state.branch
    }
}

impl<S: AsyncChunkStorage + 'static> Wal for VirtualWal<S> {
    fn limit(&mut self, _size: i64) {
        // We handle WAL size limits differently (via checkpointing)
    }

    fn begin_read_txn(&mut self) -> WalResult<bool> {
        if self.in_read_txn {
            return Ok(false);
        }

        self.state.read_txn_count.fetch_add(1, Ordering::SeqCst);
        self.in_read_txn = true;

        // Snapshot the current max frame
        let new_read_mark = self.state.max_frame.load(Ordering::SeqCst);
        let changed = new_read_mark != self.read_mark;
        self.read_mark = new_read_mark;

        // Return true if cache should be invalidated (new frames since last read)
        Ok(changed)
    }

    fn end_read_txn(&mut self) {
        if self.in_read_txn {
            self.state.read_txn_count.fetch_sub(1, Ordering::SeqCst);
            self.in_read_txn = false;
        }
    }

    fn find_frame(&mut self, page_no: NonZeroU32) -> WalResult<Option<NonZeroU32>> {
        // Find the frame containing this page (up to our read mark)
        if let Some(frame_no) = self.state.find_frame(page_no.get()) {
            if frame_no <= self.read_mark {
                return Ok(NonZeroU32::new(frame_no));
            }
        }
        Ok(None)
    }

    fn read_frame(&mut self, frame_no: NonZeroU32, buffer: &mut [u8]) -> WalResult<()> {
        let frame = self
            .state
            .read_frame(frame_no.get())
            .ok_or_else(|| maniac_libsql_ffi::Error::new(maniac_libsql_ffi::SQLITE_CORRUPT))?;

        if buffer.len() < frame.data.len() {
            return Err(maniac_libsql_ffi::Error::new(
                maniac_libsql_ffi::SQLITE_CORRUPT,
            ));
        }

        buffer[..frame.data.len()].copy_from_slice(&frame.data);
        Ok(())
    }

    fn read_frame_raw(&mut self, frame_no: NonZeroU32, buffer: &mut [u8]) -> WalResult<()> {
        // For virtual WAL, raw read is same as normal read (no frame header)
        self.read_frame(frame_no, buffer)
    }

    fn db_size(&self) -> u32 {
        self.state.db_size.load(Ordering::SeqCst)
    }

    fn begin_write_txn(&mut self) -> WalResult<()> {
        if self.in_write_txn {
            return Ok(());
        }

        // Must be in a read transaction first
        if !self.in_read_txn {
            return Err(maniac_libsql_ffi::Error::new(
                maniac_libsql_ffi::SQLITE_ERROR,
            ));
        }

        // Try to acquire write lock
        if !self.state.try_write_lock(self.conn_id) {
            return Err(maniac_libsql_ffi::Error::new(SQLITE_BUSY));
        }

        self.in_write_txn = true;
        Ok(())
    }

    fn end_write_txn(&mut self) -> WalResult<()> {
        if self.in_write_txn {
            self.state.release_write_lock(self.conn_id);
            self.in_write_txn = false;
        }
        Ok(())
    }

    fn undo<U: UndoHandler>(&mut self, handler: Option<&mut U>) -> WalResult<()> {
        if !self.in_write_txn {
            return Ok(());
        }

        // Get pages that will be undone
        let max_frame = self.state.max_frame.load(Ordering::SeqCst);

        // Notify handler about pages being undone
        if let Some(h) = handler {
            for frame_no in (self.read_mark + 1)..=max_frame {
                if let Some(frame) = self.state.frames.get(&frame_no) {
                    h.handle_undo(frame.page_no)?;
                }
            }
        }

        // Undo to read mark
        self.state.undo_to(self.read_mark);

        Ok(())
    }

    fn savepoint(&mut self, rollback_data: &mut [u32]) {
        assert_eq!(rollback_data.len(), WAL_SAVEPOINT_NDATA as usize);
        // Store current state for potential rollback
        rollback_data[0] = self.state.max_frame.load(Ordering::SeqCst);
        // Other slots can store additional state if needed
    }

    fn savepoint_undo(&mut self, rollback_data: &mut [u32]) -> WalResult<()> {
        assert_eq!(rollback_data.len(), WAL_SAVEPOINT_NDATA as usize);
        let savepoint_frame = rollback_data[0];
        self.state.undo_to(savepoint_frame);
        Ok(())
    }

    fn frame_count(&self, _locked: i32) -> WalResult<u32> {
        Ok(self.state.max_frame.load(Ordering::SeqCst))
    }

    fn insert_frames(
        &mut self,
        page_size: c_int,
        page_headers: &mut PageHeaders,
        size_after: u32,
        is_commit: bool,
        _sync_flags: c_int,
    ) -> WalResult<usize> {
        if !self.in_write_txn {
            return Err(maniac_libsql_ffi::Error::new(
                maniac_libsql_ffi::SQLITE_ERROR,
            ));
        }

        // Collect frames from page headers
        let mut frames = Vec::new();
        for (page_no, data) in page_headers.iter() {
            frames.push(WalFrame {
                page_no, // page_no is u32 from PageHdrIter
                data: data[..page_size as usize].to_vec(),
                db_size: 0, // Set on commit frame
            });
        }

        // Set db_size on the last frame if this is a commit
        if is_commit && !frames.is_empty() {
            if let Some(last) = frames.last_mut() {
                last.db_size = size_after;
            }
        }

        let frame_count = frames.len();
        self.state.insert_frames(frames);

        // Update read mark to see our own changes
        self.read_mark = self.state.max_frame.load(Ordering::SeqCst);

        if is_commit { Ok(frame_count) } else { Ok(0) }
    }

    fn checkpoint(
        &mut self,
        _db: &mut Sqlite3Db,
        mode: CheckpointMode,
        mut busy_handler: Option<&mut dyn BusyHandler>,
        _sync_flags: u32,
        _buf: &mut [u8],
        mut checkpoint_cb: Option<&mut dyn CheckpointCallback>,
        in_wal: Option<&mut i32>,
        backfilled: Option<&mut i32>,
    ) -> WalResult<()> {
        let max_frame = self.state.max_frame.load(Ordering::SeqCst);
        let checkpointed = self.state.checkpointed_frame.load(Ordering::SeqCst);

        if let Some(in_wal) = in_wal {
            *in_wal = max_frame as i32;
        }

        if max_frame == checkpointed {
            // Nothing to checkpoint
            if let Some(backfilled) = backfilled {
                *backfilled = max_frame as i32;
            }
            return Ok(());
        }

        // For PASSIVE mode, don't wait for readers
        let safe_frame = match mode {
            CheckpointMode::Passive => {
                // Can only checkpoint frames not being read
                if self.state.read_txn_count.load(Ordering::SeqCst) > 0 {
                    checkpointed
                } else {
                    max_frame
                }
            }
            _ => {
                // Wait for readers to finish
                loop {
                    if self.state.read_txn_count.load(Ordering::SeqCst) == 0 {
                        break max_frame;
                    }
                    if let Some(ref mut handler) = busy_handler {
                        if !handler.handle_busy() {
                            return Err(maniac_libsql_ffi::Error::new(SQLITE_BUSY));
                        }
                    } else {
                        return Err(maniac_libsql_ffi::Error::new(SQLITE_BUSY));
                    }
                }
            }
        };

        // Invoke checkpoint callback for each frame
        if let Some(ref mut cb) = checkpoint_cb {
            for frame_no in (checkpointed + 1)..=safe_frame {
                if let Some(frame) = self.state.frames.get(&frame_no) {
                    cb.frame(
                        safe_frame,
                        &frame.data,
                        NonZeroU32::new(frame.page_no).unwrap(),
                        NonZeroU32::new(frame_no).unwrap(),
                    )?;
                }
            }
            cb.finish()?;
        }

        // Update checkpointed frame
        self.state
            .checkpointed_frame
            .store(safe_frame, Ordering::SeqCst);

        if let Some(backfilled) = backfilled {
            *backfilled = safe_frame as i32;
        }

        // Clean up checkpointed frames if mode is TRUNCATE
        if mode == CheckpointMode::Truncate && safe_frame == max_frame {
            // Remove all frames from DashMap
            for frame_no in 1..=max_frame {
                let _ = self.state.frames.remove(&frame_no);
            }
            self.state.page_index.clear();
            self.state.max_frame.store(0, Ordering::SeqCst);
            self.state.checkpointed_frame.store(0, Ordering::SeqCst);
        }

        Ok(())
    }

    fn exclusive_mode(&mut self, _op: c_int) -> WalResult<()> {
        // We don't support exclusive mode changes
        Ok(())
    }

    fn uses_heap_memory(&self) -> bool {
        true // Our WAL is entirely in heap memory
    }

    fn set_db(&mut self, _db: &mut Sqlite3Db) {
        // Nothing to do
    }

    fn callback(&self) -> i32 {
        // Return number of frames since last callback
        self.state.max_frame.load(Ordering::SeqCst) as i32
    }

    fn frames_in_wal(&self) -> u32 {
        self.state.max_frame.load(Ordering::SeqCst)
    }
}

/// Parse database path to extract db_id, branch_id, and page_size.
/// Expected format: "/db/{db_id}/branch/{branch_id}/main.db" or similar
fn parse_db_path(path: &str) -> Option<(u64, u32, u32)> {
    // For now, use a simple format: "vwal:{db_id}:{branch_id}:{page_size}"
    if let Some(rest) = path.strip_prefix("vwal:") {
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() >= 3 {
            let db_id = parts[0].parse().ok()?;
            let branch_id = parts[1].parse().ok()?;
            let page_size = parts[2].parse().ok()?;
            return Some((db_id, branch_id, page_size));
        }
    }

    // Default fallback for testing
    Some((1, 1, 4096))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_db_path() {
        let result = parse_db_path("vwal:123:456:4096");
        assert_eq!(result, Some((123, 456, 4096)));
    }

    #[test]
    fn test_wal_state_insert_and_find() {
        let branch = BranchRef::new(1, 1);
        let state = WalState::new(branch, 4096);

        // Insert a frame
        state.insert_frames(vec![WalFrame {
            page_no: 1,
            data: vec![0u8; 4096],
            db_size: 1,
        }]);

        // Find it
        assert_eq!(state.find_frame(1), Some(1));
        assert_eq!(state.find_frame(2), None);
    }

    #[test]
    fn test_wal_state_undo() {
        let branch = BranchRef::new(1, 1);
        let state = WalState::new(branch, 4096);

        // Insert multiple frames
        state.insert_frames(vec![
            WalFrame {
                page_no: 1,
                data: vec![0u8; 4096],
                db_size: 0,
            },
            WalFrame {
                page_no: 2,
                data: vec![0u8; 4096],
                db_size: 0,
            },
            WalFrame {
                page_no: 3,
                data: vec![0u8; 4096],
                db_size: 3,
            },
        ]);

        assert_eq!(state.max_frame.load(Ordering::SeqCst), 3);

        // Undo to frame 1
        state.undo_to(1);

        assert_eq!(state.max_frame.load(Ordering::SeqCst), 1);
        assert_eq!(state.find_frame(1), Some(1));
        assert_eq!(state.find_frame(2), None);
        assert_eq!(state.find_frame(3), None);
    }
}
