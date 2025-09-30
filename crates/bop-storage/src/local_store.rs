//! Local filesystem storage management for checkpoint hot_cache.
//!
//! This module implements T7b from the checkpointing plan: LRU eviction
//! in hot_cache/ that respects manifest references and PageCache pin counts.
//!
//! # Architecture
//!
//! The LocalStore manages a hot_cache/ directory containing recently staged
//! checkpoint chunks that haven't yet been uploaded to S3 or have been
//! recently downloaded for fast access.
//!
//! Eviction policy:
//! - LRU eviction when quota is exceeded
//! - Never evicts chunks referenced by manifest
//! - Never evicts chunks with active PageCache pins (future: requires PageCache integration)
//! - Scans periodically via janitor task

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::manifest::{ChunkId, DbId};

/// Errors that can occur during LocalStore operations.
#[derive(Debug, thiserror::Error)]
pub enum LocalStoreError {
    /// I/O error accessing local filesystem.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Chunk not found in hot_cache.
    #[error("chunk {chunk_id} not found in hot_cache")]
    ChunkNotFound { chunk_id: ChunkId },

    /// Quota exhausted.
    #[error("quota exhausted: {used_bytes} / {quota_bytes} bytes")]
    QuotaExhausted { used_bytes: u64, quota_bytes: u64 },
}

/// Configuration for the LocalStore hot_cache.
#[derive(Debug, Clone)]
pub struct LocalStoreConfig {
    /// Base directory for hot_cache.
    pub hot_cache_dir: PathBuf,

    /// Maximum size of hot_cache in bytes.
    pub max_cache_bytes: u64,

    /// Minimum age before evicting chunks (prevents thrashing).
    pub min_eviction_age: Duration,
}

impl Default for LocalStoreConfig {
    fn default() -> Self {
        Self {
            hot_cache_dir: PathBuf::from("hot_cache"),
            max_cache_bytes: 10 * 1024 * 1024 * 1024, // 10 GiB
            min_eviction_age: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Metadata for a cached chunk in hot_cache.
#[derive(Debug, Clone)]
struct CachedChunk {
    /// Database ID.
    db_id: DbId,

    /// Chunk ID.
    chunk_id: ChunkId,

    /// File path in hot_cache.
    path: PathBuf,

    /// Size in bytes.
    size_bytes: u64,

    /// Last access time.
    last_access: SystemTime,

    /// Whether chunk is referenced by manifest.
    manifest_referenced: bool,

    /// Pin count (future: integration with PageCache).
    pin_count: u32,
}

/// LocalStore manages hot_cache/ directory with LRU eviction.
///
/// # T7b: hot_cache Eviction with PageCache Awareness
///
/// The LocalStore provides:
/// - LRU eviction when quota is exceeded
/// - Protection for manifest-referenced chunks
/// - Protection for pinned chunks (future: requires PageCache integration)
/// - Periodic janitor for cleanup
pub struct LocalStore {
    config: LocalStoreConfig,
    chunks: Arc<Mutex<HashMap<ChunkId, CachedChunk>>>,
    lru_order: Arc<Mutex<VecDeque<ChunkId>>>,
    used_bytes: Arc<Mutex<u64>>,
}

impl LocalStore {
    /// Creates a new LocalStore with the given configuration.
    pub fn new(config: LocalStoreConfig) -> Result<Self, LocalStoreError> {
        // Create hot_cache directory if it doesn't exist
        fs::create_dir_all(&config.hot_cache_dir)?;

        Ok(Self {
            config,
            chunks: Arc::new(Mutex::new(HashMap::new())),
            lru_order: Arc::new(Mutex::new(VecDeque::new())),
            used_bytes: Arc::new(Mutex::new(0)),
        })
    }

    /// Adds a chunk to the hot_cache.
    ///
    /// If quota is exceeded, evicts LRU chunks that are not manifest-referenced
    /// or pinned.
    pub fn add_chunk(
        &self,
        db_id: DbId,
        chunk_id: ChunkId,
        path: PathBuf,
        manifest_referenced: bool,
    ) -> Result<(), LocalStoreError> {
        let metadata = fs::metadata(&path)?;
        let size_bytes = metadata.len();

        let mut old_path = None;
        let mut reclaimed = 0u64;

        {
            let mut chunks = self.chunks.lock().unwrap();
            if let Some(existing) = chunks.remove(&chunk_id) {
                reclaimed = existing.size_bytes;
                old_path = Some(existing.path);
            }
        }

        if reclaimed > 0 {
            let mut lru_order = self.lru_order.lock().unwrap();
            if let Some(pos) = lru_order.iter().position(|id| *id == chunk_id) {
                lru_order.remove(pos);
            }
        }

        if reclaimed > 0 {
            let mut used_bytes = self.used_bytes.lock().unwrap();
            *used_bytes = used_bytes.saturating_sub(reclaimed);
        }

        if let Some(old_path) = old_path {
            if old_path != path {
                let _ = fs::remove_file(&old_path);
            }
        }

        // Evict chunks if necessary to make space
        self.evict_to_fit(size_bytes)?;

        let cached_chunk = CachedChunk {
            db_id,
            chunk_id,
            path: path.clone(),
            size_bytes,
            last_access: SystemTime::now(),
            manifest_referenced,
            pin_count: 0,
        };

        let mut chunks = self.chunks.lock().unwrap();
        let mut lru_order = self.lru_order.lock().unwrap();
        let mut used_bytes = self.used_bytes.lock().unwrap();

        chunks.insert(chunk_id, cached_chunk);
        lru_order.push_back(chunk_id);
        *used_bytes = used_bytes.saturating_add(size_bytes);

        Ok(())
    }

    /// Marks a chunk as manifest-referenced (protects from eviction).
    pub fn mark_manifest_referenced(&self, chunk_id: ChunkId) {
        let mut chunks = self.chunks.lock().unwrap();
        if let Some(chunk) = chunks.get_mut(&chunk_id) {
            chunk.manifest_referenced = true;
        }
    }

    /// Marks a chunk as no longer manifest-referenced.
    pub fn unmark_manifest_referenced(&self, chunk_id: ChunkId) {
        let mut chunks = self.chunks.lock().unwrap();
        if let Some(chunk) = chunks.get_mut(&chunk_id) {
            chunk.manifest_referenced = false;
        }
    }

    /// Increments the pin count for a chunk (protects from eviction).
    ///
    /// Future: This should be integrated with PageCache to track actual pins.
    pub fn pin_chunk(&self, chunk_id: ChunkId) {
        let mut chunks = self.chunks.lock().unwrap();
        if let Some(chunk) = chunks.get_mut(&chunk_id) {
            chunk.pin_count += 1;
        }
    }

    /// Decrements the pin count for a chunk.
    pub fn unpin_chunk(&self, chunk_id: ChunkId) {
        let mut chunks = self.chunks.lock().unwrap();
        if let Some(chunk) = chunks.get_mut(&chunk_id) {
            if chunk.pin_count > 0 {
                chunk.pin_count -= 1;
            }
        }
    }

    /// Evicts LRU chunks to fit the given size.
    ///
    /// # T7b: Eviction Policy
    ///
    /// - Evicts oldest chunks first (LRU)
    /// - Skips manifest-referenced chunks
    /// - Skips pinned chunks
    /// - Skips chunks younger than min_eviction_age
    fn evict_to_fit(&self, needed_bytes: u64) -> Result<(), LocalStoreError> {
        let mut chunks = self.chunks.lock().unwrap();
        let mut lru_order = self.lru_order.lock().unwrap();
        let mut used_bytes = self.used_bytes.lock().unwrap();

        let available = self.config.max_cache_bytes.saturating_sub(*used_bytes);
        if available >= needed_bytes {
            return Ok(()); // Already have space
        }

        let mut bytes_to_free = needed_bytes - available;
        let now = SystemTime::now();
        let min_age = self.config.min_eviction_age;

        // Iterate LRU order and evict candidates
        let initial_size = lru_order.len();
        let mut examined = 0;
        let mut skipped_chunks = Vec::new();

        while bytes_to_free > 0 && examined < initial_size {
            let chunk_id = match lru_order.pop_front() {
                Some(id) => id,
                None => break,
            };
            examined += 1;

            let chunk = match chunks.get(&chunk_id) {
                Some(c) => c,
                None => continue,
            };

            // Check eviction protection
            let mut should_skip = false;

            if chunk.manifest_referenced {
                // Skip manifest-referenced chunks
                should_skip = true;
            } else if chunk.pin_count > 0 {
                // Skip pinned chunks
                should_skip = true;
            } else if let Ok(age) = now.duration_since(chunk.last_access) {
                if age < min_age {
                    // Too young to evict
                    should_skip = true;
                }
            }

            if should_skip {
                skipped_chunks.push(chunk_id);
                continue;
            }

            // Evict this chunk
            let size = chunk.size_bytes;
            let path = chunk.path.clone();

            chunks.remove(&chunk_id);
            *used_bytes = used_bytes.saturating_sub(size);
            bytes_to_free = bytes_to_free.saturating_sub(size);

            // Delete file from disk
            let _ = fs::remove_file(&path);
        }

        // Restore skipped chunks to the end of the queue
        for chunk_id in skipped_chunks {
            lru_order.push_back(chunk_id);
        }

        // Check if we freed enough space
        let available = self.config.max_cache_bytes.saturating_sub(*used_bytes);
        if available < needed_bytes {
            return Err(LocalStoreError::QuotaExhausted {
                used_bytes: *used_bytes,
                quota_bytes: self.config.max_cache_bytes,
            });
        }

        Ok(())
    }

    /// Returns the current cache usage in bytes.
    pub fn used_bytes(&self) -> u64 {
        *self.used_bytes.lock().unwrap()
    }

    /// Returns the number of cached chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunks.lock().unwrap().len()
    }

    /// Scans the hot_cache and removes chunks not tracked in metadata.
    ///
    /// Should be called periodically by a janitor task.
    pub fn cleanup_orphaned(&self) -> Result<usize, LocalStoreError> {
        let chunks = self.chunks.lock().unwrap();
        let tracked_paths: std::collections::HashSet<_> =
            chunks.values().map(|c| c.path.clone()).collect();

        let mut cleaned = 0;
        for entry in fs::read_dir(&self.config.hot_cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && !tracked_paths.contains(&path) {
                fs::remove_file(&path)?;
                cleaned += 1;
            }
        }

        Ok(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn local_store_add_and_evict() {
        let temp = TempDir::new().unwrap();
        let config = LocalStoreConfig {
            hot_cache_dir: temp.path().to_path_buf(),
            max_cache_bytes: 1000,
            min_eviction_age: Duration::from_millis(0),
        };

        let store = LocalStore::new(config).unwrap();

        // Add chunk 1 (500 bytes)
        let chunk1_path = temp.path().join("chunk1.bin");
        fs::write(&chunk1_path, vec![0u8; 500]).unwrap();
        store.add_chunk(1, 1, chunk1_path, false).unwrap();
        assert_eq!(store.used_bytes(), 500);
        assert_eq!(store.chunk_count(), 1);

        // Add chunk 2 (400 bytes) - should fit
        let chunk2_path = temp.path().join("chunk2.bin");
        fs::write(&chunk2_path, vec![0u8; 400]).unwrap();
        store.add_chunk(1, 2, chunk2_path, false).unwrap();
        assert_eq!(store.used_bytes(), 900);
        assert_eq!(store.chunk_count(), 2);

        // Add chunk 3 (300 bytes) - should evict chunk 1
        let chunk3_path = temp.path().join("chunk3.bin");
        fs::write(&chunk3_path, vec![0u8; 300]).unwrap();
        store.add_chunk(1, 3, chunk3_path, false).unwrap();
        assert_eq!(store.chunk_count(), 2); // Chunk 1 evicted
        assert!(store.used_bytes() <= 1000);
    }

    #[test]
    fn local_store_protects_manifest_referenced() {
        let temp = TempDir::new().unwrap();
        let config = LocalStoreConfig {
            hot_cache_dir: temp.path().to_path_buf(),
            max_cache_bytes: 1000,
            min_eviction_age: Duration::from_millis(0),
        };

        let store = LocalStore::new(config).unwrap();

        // Add chunk 1 (600 bytes, manifest-referenced)
        let chunk1_path = temp.path().join("chunk1.bin");
        fs::write(&chunk1_path, vec![0u8; 600]).unwrap();
        store.add_chunk(1, 1, chunk1_path, true).unwrap();

        // Add chunk 2 (300 bytes)
        let chunk2_path = temp.path().join("chunk2.bin");
        fs::write(&chunk2_path, vec![0u8; 300]).unwrap();
        store.add_chunk(1, 2, chunk2_path, false).unwrap();

        // Add chunk 3 (300 bytes) - should evict chunk 2, not chunk 1
        let chunk3_path = temp.path().join("chunk3.bin");
        fs::write(&chunk3_path, vec![0u8; 300]).unwrap();
        store.add_chunk(1, 3, chunk3_path, false).unwrap();

        // Chunk 1 should still be present (manifest-referenced)
        assert_eq!(store.chunk_count(), 2);
    }

    #[test]
    fn local_store_protects_pinned_chunks() {
        let temp = TempDir::new().unwrap();
        let config = LocalStoreConfig {
            hot_cache_dir: temp.path().join("hot_cache"),
            max_cache_bytes: 1000,
            min_eviction_age: Duration::from_millis(0),
        };

        let store = LocalStore::new(config).unwrap();

        // Add chunk 1 (600 bytes)
        let chunk1_path = temp.path().join("chunk1.bin");
        fs::write(&chunk1_path, vec![0u8; 600]).unwrap();
        store.add_chunk(1, 1, chunk1_path, false).unwrap();

        // Pin chunk 1
        store.pin_chunk(1);

        // Add chunk 2 (300 bytes)
        let chunk2_path = temp.path().join("chunk2.bin");
        fs::write(&chunk2_path, vec![0u8; 300]).unwrap();
        store.add_chunk(1, 2, chunk2_path, false).unwrap();

        // Now pin chunk 2 as well
        store.pin_chunk(2);

        // Try to add chunk 3 (300 bytes) - should fail because all chunks are pinned
        let chunk3_path = temp.path().join("chunk3.bin");
        fs::write(&chunk3_path, vec![0u8; 300]).unwrap();
        let result = store.add_chunk(1, 3, chunk3_path, false);

        // Should fail with quota exhausted (can't evict any pinned chunk)
        assert!(result.is_err());

        // Both chunks should still be present
        assert_eq!(store.chunk_count(), 2);
    }

    #[test]
    fn local_store_cleanup_orphaned() {
        let temp = TempDir::new().unwrap();
        let config = LocalStoreConfig {
            hot_cache_dir: temp.path().to_path_buf(),
            max_cache_bytes: 10000,
            min_eviction_age: Duration::from_millis(0),
        };

        let store = LocalStore::new(config).unwrap();

        // Create orphaned file
        let orphaned_path = temp.path().join("orphaned.bin");
        fs::write(&orphaned_path, vec![0u8; 100]).unwrap();

        // Add tracked chunk
        let chunk1_path = temp.path().join("chunk1.bin");
        fs::write(&chunk1_path, vec![0u8; 100]).unwrap();
        store.add_chunk(1, 1, chunk1_path.clone(), false).unwrap();

        // Cleanup should remove orphaned file
        let cleaned = store.cleanup_orphaned().unwrap();
        assert_eq!(cleaned, 1);
        assert!(!orphaned_path.exists());
        assert!(chunk1_path.exists());
    }
}
