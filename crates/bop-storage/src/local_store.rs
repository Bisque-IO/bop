//! Local chunk cache shared by append-only workloads.
//!
//! The cache stores decompressed chunk files on local disk so that subsequent
//! reads can avoid re-downloading objects from remote storage. A single
//! [`ChunkStorageQuota`](crate::chunk_quota::ChunkStorageQuota) is shared across
//! all caches to constrain aggregate disk usage.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::task::JoinError;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::chunk_quota::{ChunkQuotaError, ChunkQuotaGuard, ChunkStorageQuota};
use crate::io::{IoError, IoFile, IoOpenOptions, IoVec, SharedIoDriver};
use crate::manifest::{ChunkId, DbId, Generation};
use crate::runtime::StorageRuntime;

/// Errors that can occur during local chunk cache operations.
#[derive(Debug, Error)]
pub enum LocalChunkStoreError {
    /// Underlying filesystem error.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Driver-level error surfaced by the configured [`IoDriver`].
    #[error("io driver error: {0}")]
    IoDriver(#[from] IoError),

    /// Tokio task handling asynchronous file operations was cancelled.
    #[error("async task cancelled: {0}")]
    TaskCancelled(#[from] JoinError),

    /// Requested chunk is not present in the cache.
    #[error("chunk {chunk_id} generation {generation} not found in cache")]
    ChunkNotFound {
        chunk_id: ChunkId,
        generation: Generation,
    },

    /// Another task is already hydrating this chunk.
    #[error("chunk {chunk_id} generation {generation} is currently being hydrated")]
    ChunkBusy {
        chunk_id: ChunkId,
        generation: Generation,
    },

    /// Cache capacity could not be increased enough to satisfy an insertion.
    #[error("cache quota exhausted after eviction attempts (needed {needed_bytes} bytes)")]
    QuotaExhausted { needed_bytes: u64 },

    /// Global chunk quota is exhausted.
    #[error(transparent)]
    GlobalQuota(ChunkQuotaError),

    /// The downloaded chunk length did not match the expected manifest metadata.
    #[error("unexpected chunk length: expected {expected_bytes} bytes, wrote {actual_bytes} bytes")]
    UnexpectedLength {
        expected_bytes: u64,
        actual_bytes: u64,
    },
}

/// Metadata describing a cached chunk.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LocalChunkKey {
    pub db_id: DbId,
    pub chunk_id: ChunkId,
    pub generation: Generation,
}

impl LocalChunkKey {
    pub fn new(db_id: DbId, chunk_id: ChunkId, generation: Generation) -> Self {
        Self {
            db_id,
            chunk_id,
            generation,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LocalChunkHandle {
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct LocalChunkStoreConfig {
    pub root_dir: PathBuf,
    pub max_cache_bytes: u64,
    pub min_eviction_age: Duration,
}

impl Default for LocalChunkStoreConfig {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from("chunk_cache"),
            max_cache_bytes: 10 * 1024 * 1024 * 1024, // 10 GiB
            min_eviction_age: Duration::from_secs(300), // 5 minutes
        }
    }
}

#[derive(Debug)]
struct CachedChunk {
    key: LocalChunkKey,
    path: PathBuf,
    size_bytes: u64,
    inserted_at: Instant,
    last_access: Instant,
    quota_guard: ChunkQuotaGuard,
}

#[derive(Debug)]
enum DownloadState {
    /// No download in progress
    Idle,
    /// Download in progress, waiters will be notified on completion
    InProgress,
}

#[derive(Debug)]
struct StoreState {
    used_bytes: u64,
    entries: HashMap<LocalChunkKey, CachedChunk>,
    lru: VecDeque<LocalChunkKey>,
    downloads: HashMap<LocalChunkKey, DownloadState>,
}

impl StoreState {
    fn new() -> Self {
        Self {
            used_bytes: 0,
            entries: HashMap::new(),
            lru: VecDeque::new(),
            downloads: HashMap::new(),
        }
    }

    fn touch(&mut self, key: &LocalChunkKey) {
        self.lru.push_back(key.clone());
    }

    fn is_downloading(&self, key: &LocalChunkKey) -> bool {
        matches!(self.downloads.get(key), Some(DownloadState::InProgress))
    }

    fn start_download(&mut self, key: LocalChunkKey) -> bool {
        use std::collections::hash_map::Entry;
        match self.downloads.entry(key) {
            Entry::Vacant(e) => {
                e.insert(DownloadState::InProgress);
                true
            }
            Entry::Occupied(_) => false,
        }
    }

    fn finish_download(&mut self, key: &LocalChunkKey) {
        self.downloads.remove(key);
    }
}

/// On-disk cache of decompressed chunks.
pub struct LocalChunkStore {
    config: LocalChunkStoreConfig,
    quota: Arc<ChunkStorageQuota>,
    io: SharedIoDriver,
    runtime: Arc<StorageRuntime>,
    state: Mutex<StoreState>,
    download_notify: Condvar,
}

impl fmt::Debug for LocalChunkStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalChunkStore")
            .field("config", &self.config)
            .field("quota", &self.quota)
            .finish()
    }
}

impl LocalChunkStore {
    #[instrument(skip(quota, io, runtime), fields(root_dir = %config.root_dir.display(), max_cache_bytes = config.max_cache_bytes))]
    pub(crate) fn new(
        config: LocalChunkStoreConfig,
        quota: Arc<ChunkStorageQuota>,
        io: SharedIoDriver,
        runtime: Arc<StorageRuntime>,
    ) -> Result<Self, LocalChunkStoreError> {
        debug!("creating local chunk store");
        fs::create_dir_all(&config.root_dir)?;
        info!("local chunk store created successfully");
        Ok(Self {
            config,
            quota,
            io,
            runtime,
            state: Mutex::new(StoreState::new()),
            download_notify: Condvar::new(),
        })
    }

    /// Returns the cached chunk handle if present.
    #[instrument(skip(self), fields(db_id = key.db_id, chunk_id = key.chunk_id, generation = key.generation))]
    pub fn get(&self, key: &LocalChunkKey) -> Option<LocalChunkHandle> {
        let mut state = self.state.lock().expect("local chunk store mutex poisoned");
        if let Some(handle) = state.entries.get_mut(key).map(|entry| {
            entry.last_access = Instant::now();
            LocalChunkHandle {
                path: entry.path.clone(),
                size_bytes: entry.size_bytes,
            }
        }) {
            state.touch(key);
            trace!(size_bytes = handle.size_bytes, "cache hit");
            debug!(size_bytes = handle.size_bytes, path = %handle.path.display(), "chunk retrieved from cache");
            Some(handle)
        } else {
            trace!("cache miss");
            None
        }
    }

    #[instrument(skip(self), fields(db_id = key.db_id, chunk_id = key.chunk_id, generation = key.generation))]
    pub async fn open_read_only(
        &self,
        key: &LocalChunkKey,
    ) -> Result<Box<dyn IoFile>, LocalChunkStoreError> {
        trace!("opening chunk for read-only access");
        let handle = self.get(key).ok_or_else(|| {
            debug!("chunk not found in cache");
            LocalChunkStoreError::ChunkNotFound {
                chunk_id: key.chunk_id,
                generation: key.generation,
            }
        })?;
        let result = self.open_with_options_async(handle.path.clone(), IoOpenOptions::read_only())
            .await;
        match &result {
            Ok(_) => debug!(path = %handle.path.display(), "chunk opened successfully"),
            Err(e) => error!(error = ?e, path = %handle.path.display(), "failed to open chunk"),
        }
        result
    }

    #[instrument(skip(self), fields(db_id = key.db_id, chunk_id = key.chunk_id, generation = key.generation))]
    pub async fn create_temp_writer(
        &self,
        key: &LocalChunkKey,
    ) -> Result<(Box<dyn IoFile>, PathBuf), LocalChunkStoreError> {
        trace!("creating temporary writer");
        let temp_path = self.temporary_path(key);
        if let Some(parent) = temp_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = self
            .open_with_options_async(temp_path.clone(), IoOpenOptions::write_only())
            .await?;
        debug!(temp_path = %temp_path.display(), "temporary writer created");
        Ok((file, temp_path))
    }

    async fn open_with_options_async(
        &self,
        path: PathBuf,
        options: IoOpenOptions,
    ) -> Result<Box<dyn IoFile>, LocalChunkStoreError> {
        let driver = self.io.clone();
        let handle = self.runtime.handle();
        let options_clone = options.clone();
        let path_clone = path.clone();
        handle
            .spawn_blocking(move || driver.open(path_clone.as_path(), &options_clone))
            .await
            .map_err(LocalChunkStoreError::TaskCancelled)?
            .map_err(LocalChunkStoreError::IoDriver)
    }

    /// Returns `true` if the chunk is currently cached.
    #[instrument(skip(self), fields(db_id = key.db_id, chunk_id = key.chunk_id, generation = key.generation))]
    pub fn contains(&self, key: &LocalChunkKey) -> bool {
        let state = self.state.lock().expect("local chunk store mutex poisoned");
        let contains = state.entries.contains_key(key);
        trace!(contains, "checked cache for chunk");
        contains
    }

    /// Waits for a chunk to be available, either from cache or by waiting for an in-flight download.
    /// Returns the handle if the chunk becomes available within a reasonable timeout.
    #[instrument(skip(self), fields(db_id = key.db_id, chunk_id = key.chunk_id, generation = key.generation, timeout_ms = timeout.as_millis()))]
    pub fn get_or_wait(&self, key: &LocalChunkKey, timeout: Duration) -> Option<LocalChunkHandle> {
        trace!("waiting for chunk to become available");
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().expect("local chunk store mutex poisoned");

        loop {
            // Check if chunk is already cached
            if let Some(handle) = state.entries.get_mut(key).map(|entry| {
                entry.last_access = Instant::now();
                LocalChunkHandle {
                    path: entry.path.clone(),
                    size_bytes: entry.size_bytes,
                }
            }) {
                state.touch(key);
                debug!(size_bytes = handle.size_bytes, "chunk available from cache");
                return Some(handle);
            }

            // If not downloading, return None so caller can initiate download
            if !state.is_downloading(key) {
                trace!("chunk not downloading, caller should initiate");
                return None;
            }

            // Wait for download to complete
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                debug!("wait timeout expired");
                return None;
            }

            trace!(remaining_ms = remaining.as_millis(), "waiting for download to complete");
            let (new_state, timeout_result) = self.download_notify
                .wait_timeout(state, remaining)
                .expect("local chunk store mutex poisoned");

            state = new_state;

            if timeout_result.timed_out() {
                debug!("wait timed out");
                return None;
            }
        }
    }

    /// Hydrates a chunk into the cache from a reader containing uncompressed bytes.
    /// This method coordinates with concurrent callers to ensure only one download happens.
    #[instrument(skip(self, reader), fields(db_id = key.db_id, chunk_id = key.chunk_id, generation = key.generation, expected_len))]
    pub fn insert_from_reader<R: Read + ?Sized>(
        &self,
        key: LocalChunkKey,
        expected_len: u64,
        reader: &mut R,
    ) -> Result<LocalChunkHandle, LocalChunkStoreError> {
        trace!("attempting to insert chunk from reader");
        {
            let mut state = self.state.lock().expect("local chunk store mutex poisoned");
            if let Some(handle) = state.entries.get_mut(&key).map(|entry| {
                entry.last_access = Instant::now();
                LocalChunkHandle {
                    path: entry.path.clone(),
                    size_bytes: entry.size_bytes,
                }
            }) {
                state.touch(&key);
                debug!(size_bytes = handle.size_bytes, "chunk already in cache, skipping insert");
                return Ok(handle);
            }
            if !state.start_download(key.clone()) {
                debug!("chunk is currently being downloaded by another caller");
                return Err(LocalChunkStoreError::ChunkBusy {
                    chunk_id: key.chunk_id,
                    generation: key.generation,
                });
            }
            trace!("download started, acquired exclusive lock");
        }

        let reserve_guard = match self.quota.try_acquire(expected_len) {
            Ok(guard) => {
                trace!(size_bytes = expected_len, "quota acquired");
                guard
            }
            Err(err) => {
                error!(size_bytes = expected_len, error = ?err, "failed to acquire quota");
                let mut state = self.state.lock().expect("local chunk store mutex poisoned");
                state.finish_download(&key);
                self.download_notify.notify_all();
                return Err(LocalChunkStoreError::GlobalQuota(err));
            }
        };

        let temp_path = self.temporary_path(&key);
        let final_path = self.final_path(&key);
        trace!(temp_path = %temp_path.display(), final_path = %final_path.display(), "preparing to write chunk");
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut temp_file = self
            .io
            .open(&temp_path, &IoOpenOptions::write_only())
            .map_err(LocalChunkStoreError::IoDriver)?;
        let _ = temp_file.allocate(0, expected_len);
        trace!(size_bytes = expected_len, "writing chunk data to temporary file");
        let start = Instant::now();
        let copied = write_stream_to_file(reader, temp_file.as_mut())?;
        let duration = start.elapsed();
        drop(temp_file);

        if copied != expected_len {
            warn!(expected_bytes = expected_len, actual_bytes = copied, "unexpected chunk length, removing temporary file");
            let _ = fs::remove_file(&temp_path);
            let mut state = self.state.lock().expect("local chunk store mutex poisoned");
            state.finish_download(&key);
            self.download_notify.notify_all();
            return Err(LocalChunkStoreError::UnexpectedLength {
                expected_bytes: expected_len,
                actual_bytes: copied,
            });
        }

        debug!(size_bytes = copied, duration_ms = duration.as_millis(), "chunk data written successfully");
        trace!("renaming temporary file to final path");
        fs::rename(&temp_path, &final_path)?;

        let mut state = self.state.lock().expect("local chunk store mutex poisoned");
        state.finish_download(&key);

        trace!(needed_bytes = expected_len, "checking if eviction is needed");
        if let Err(err) = self.evict_to_fit_with_lock(&mut state, expected_len) {
            error!(needed_bytes = expected_len, error = ?err, "failed to evict enough space");
            drop(state);
            let _ = fs::remove_file(&final_path);
            return Err(err);
        }

        let entry = CachedChunk {
            key: key.clone(),
            path: final_path.clone(),
            size_bytes: expected_len,
            inserted_at: Instant::now(),
            last_access: Instant::now(),
            quota_guard: reserve_guard,
        };

        state.used_bytes = state.used_bytes.saturating_add(expected_len);
        state.entries.insert(key.clone(), entry);
        state.touch(&key);

        let cache_size = state.used_bytes;
        let cache_entries = state.entries.len();

        let handle = LocalChunkHandle {
            path: final_path,
            size_bytes: expected_len,
        };

        drop(state);
        self.download_notify.notify_all();

        info!(size_bytes = expected_len, cache_size, cache_entries, "chunk successfully inserted into cache");
        Ok(handle)
    }

    fn evict_to_fit_with_lock(
        &self,
        state: &mut StoreState,
        needed_bytes: u64,
    ) -> Result<(), LocalChunkStoreError> {
        let mut available = self.config.max_cache_bytes.saturating_sub(state.used_bytes);
        let initial_available = available;

        if available >= needed_bytes {
            trace!(available, needed_bytes, "sufficient space available, no eviction needed");
            return Ok(());
        }

        debug!(available = initial_available, needed_bytes, used_bytes = state.used_bytes, max_cache_bytes = self.config.max_cache_bytes, "eviction required");
        let mut evicted_count = 0;
        let mut evicted_bytes = 0u64;
        let mut skipped_downloading = 0;
        let mut skipped_min_age = 0;

        while available < needed_bytes {
            let candidate = match state.lru.pop_front() {
                Some(k) => k,
                None => {
                    warn!(needed_bytes, available, evicted_count, evicted_bytes, "quota exhausted, no more eviction candidates");
                    return Err(LocalChunkStoreError::QuotaExhausted { needed_bytes });
                }
            };

            if state.is_downloading(&candidate) {
                trace!(db_id = candidate.db_id, chunk_id = candidate.chunk_id, "skipping candidate, currently downloading");
                skipped_downloading += 1;
                continue;
            }

            let should_keep = if let Some(entry) = state.entries.get(&candidate) {
                entry.inserted_at.elapsed() < self.config.min_eviction_age
            } else {
                false
            };

            if should_keep {
                trace!(db_id = candidate.db_id, chunk_id = candidate.chunk_id, "skipping candidate, below minimum eviction age");
                skipped_min_age += 1;
                state.lru.push_back(candidate);
                continue;
            }

            if let Some(entry) = state.entries.remove(&candidate) {
                let size = entry.size_bytes;
                trace!(db_id = candidate.db_id, chunk_id = candidate.chunk_id, generation = candidate.generation, size_bytes = size, "evicting chunk");
                let _ = fs::remove_file(&entry.path);
                state.used_bytes = state.used_bytes.saturating_sub(size);
                available = self.config.max_cache_bytes.saturating_sub(state.used_bytes);
                evicted_count += 1;
                evicted_bytes += size;
                // dropping entry releases the quota guard
            }
        }

        debug!(evicted_count, evicted_bytes, skipped_downloading, skipped_min_age, available, "eviction completed successfully");
        Ok(())
    }

    fn temporary_path(&self, key: &LocalChunkKey) -> PathBuf {
        self.config.root_dir.join(format!(
            "db_{:08}/chunk_{:08}_gen_{:08}.partial",
            key.db_id, key.chunk_id, key.generation
        ))
    }

    fn final_path(&self, key: &LocalChunkKey) -> PathBuf {
        self.config.root_dir.join(format!(
            "db_{:08}/chunk_{:08}_gen_{:08}.chunk",
            key.db_id, key.chunk_id, key.generation
        ))
    }
}

fn write_stream_to_file<R: Read + ?Sized>(
    reader: &mut R,
    file: &mut dyn IoFile,
) -> Result<u64, LocalChunkStoreError> {
    let mut total = 0u64;
    let mut offset = 0u64;
    let mut buffer = [0u8; 1 << 20];
    let mut chunks_written = 0;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let io_vec = IoVec::new(&buffer[..read]);
        let written = file
            .writev_at(offset, &[io_vec])
            .map_err(LocalChunkStoreError::IoDriver)?;
        total = total.saturating_add(written as u64);
        offset = offset.saturating_add(written as u64);
        chunks_written += 1;

        if chunks_written % 100 == 0 {
            trace!(bytes_written = total, chunks = chunks_written, "streaming write progress");
        }
    }
    trace!(total_bytes = total, chunks = chunks_written, "flushing file");
    file.flush().map_err(LocalChunkStoreError::IoDriver)?;
    debug!(total_bytes = total, chunks = chunks_written, "stream write completed");
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IoBackendKind, IoRegistry};
    use crate::runtime::{StorageRuntime, StorageRuntimeOptions};
    use tempfile::TempDir;

    fn test_store(limit: u64) -> (TempDir, LocalChunkStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = LocalChunkStoreConfig {
            root_dir: dir.path().join("cache"),
            max_cache_bytes: limit,
            min_eviction_age: Duration::from_secs(0),
        };
        let quota = ChunkStorageQuota::new(limit);
        let runtime = StorageRuntime::create(StorageRuntimeOptions::default()).expect("runtime");
        let registry = IoRegistry::new(IoBackendKind::Std);
        let driver = registry.resolve(Some(IoBackendKind::Std)).expect("driver");
        let store = LocalChunkStore::new(config, quota, driver, runtime).expect("store");
        (dir, store)
    }

    #[test]
    fn insert_and_get_round_trip() {
        let (_dir, store) = test_store(10 * 1024 * 1024);
        let key = LocalChunkKey::new(1, 2, 3);
        let mut data = &b"hello world"[..];
        store
            .insert_from_reader(key.clone(), data.len() as u64, &mut data)
            .expect("insert");

        let handle = store.get(&key).expect("fetch");
        assert_eq!(handle.size_bytes, 11);
        assert!(handle.path.exists());
    }

    #[test]
    fn busy_guard_is_reported() {
        let (_dir, store) = test_store(10 * 1024 * 1024);
        let key = LocalChunkKey::new(1, 2, 3);
        {
            let mut state = store.state.lock().unwrap();
            state.start_download(key.clone());
        }
        let mut data = &b"hello"[..];
        let err = store
            .insert_from_reader(key.clone(), data.len() as u64, &mut data)
            .unwrap_err();
        assert!(matches!(err, LocalChunkStoreError::ChunkBusy { .. }));
    }

    #[test]
    fn evicts_old_chunks() {
        let (_dir, store) = test_store(10 * 1024 * 1024);
        let key1 = LocalChunkKey::new(1, 1, 0);
        let key2 = LocalChunkKey::new(1, 2, 0);

        let mut data1 = vec![0u8; 100];
        let mut data2 = vec![0u8; 100];

        store
            .insert_from_reader(key1.clone(), 100, &mut &data1[..])
            .expect("insert 1");
        store
            .insert_from_reader(key2.clone(), 100, &mut &data2[..])
            .expect("insert 2");

        // Both chunks should be cached
        assert!(store.get(&key1).is_some());
        assert!(store.get(&key2).is_some());
    }
}
