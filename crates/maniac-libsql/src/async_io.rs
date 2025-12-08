//! Async I/O Integration for VFS and WAL
//!
//! This module provides async I/O capabilities for SQLite VFS and WAL operations
//! using maniac's stackful coroutines. The key insight is that SQLite's VFS callbacks
//! are synchronous, but we can use `sync_await` to bridge async I/O operations.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        SQLite Core                              │
//! │   (Synchronous - expects VFS/WAL callbacks to return results)   │
//! └─────────────────────────────────────────────────────────────────┘
//!                               │
//!                               ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                   VirtualVfs / VirtualWal                       │
//! │   (Sync interface, uses sync_await internally)                  │
//! └─────────────────────────────────────────────────────────────────┘
//!                               │
//!                               ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     Async I/O Layer                             │
//! │   - maniac::fs::File for local chunks                           │
//! │   - maniac::net::TcpStream for remote replication               │
//! │   - Chunk storage (local FS or object store)                    │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! The `sync_await` function from maniac runtime allows calling async code from
//! within synchronous VFS/WAL callbacks when running inside a stackful coroutine:
//!
//! ```ignore
//! fn read_page(&self, page_no: u32, buffer: &mut [u8]) -> Result<()> {
//!     // First check in-memory cache (fast path)
//!     if let Some(data) = self.cache.get(&page_no) {
//!         buffer.copy_from_slice(&data);
//!         return Ok(());
//!     }
//!
//!     // Fall back to async storage (slow path via sync_await)
//!     sync_await(self.load_page_async(page_no, buffer))
//! }
//! ```

use crate::chunk_storage::{ChunkKey, StorageResult};
use crate::page_manager::{ChunkConfig, restore_chunk_from_storage};
use crate::raft_log::BranchRef;
use std::future::Future;
use std::io;
use std::sync::Arc;

/// Trait for async chunk storage operations.
///
/// Implementations can use maniac's async file I/O or network operations.
pub trait AsyncChunkStorage: Send + Sync + 'static {
    /// Read a chunk asynchronously.
    fn read_chunk(&self, key: &ChunkKey) -> impl Future<Output = StorageResult<Vec<u8>>> + Send;

    /// Write a chunk asynchronously.
    fn write_chunk(
        &self,
        key: &ChunkKey,
        data: Vec<u8>,
    ) -> impl Future<Output = StorageResult<()>> + Send;

    /// Check if a chunk exists asynchronously.
    fn chunk_exists(&self, key: &ChunkKey) -> impl Future<Output = StorageResult<bool>> + Send;

    /// Delete a chunk asynchronously.
    fn delete_chunk(&self, key: &ChunkKey) -> impl Future<Output = StorageResult<()>> + Send;
}

/// Async page loader that bridges sync WAL operations with async storage.
///
/// This struct is designed to be used within SQLite VFS/WAL callbacks
/// where we need to load pages that aren't in the in-memory WAL.
pub struct AsyncPageLoader<S: AsyncChunkStorage> {
    storage: Arc<S>,
    config: ChunkConfig,
    page_size: u32,
    pages_per_chunk: u32,
}

impl<S: AsyncChunkStorage> AsyncPageLoader<S> {
    /// Create a new async page loader.
    pub fn new(storage: Arc<S>, config: ChunkConfig, page_size: u32, pages_per_chunk: u32) -> Self {
        Self {
            storage,
            config,
            page_size,
            pages_per_chunk,
        }
    }

    /// Calculate which chunk contains the given page.
    pub fn page_to_chunk(&self, page_no: u32) -> u32 {
        (page_no.saturating_sub(1)) / self.pages_per_chunk
    }

    /// Calculate the offset within a chunk for the given page.
    pub fn page_offset_in_chunk(&self, page_no: u32) -> usize {
        let page_index = (page_no.saturating_sub(1)) % self.pages_per_chunk;
        page_index as usize * self.page_size as usize
    }

    /// Load a page asynchronously from chunk storage.
    ///
    /// This is the async implementation that will be called via `sync_await`
    /// from synchronous VFS/WAL callbacks.
    ///
    /// # Arguments
    /// * `branch` - The branch reference (db_id, branch_id)
    /// * `start_frame` - The starting frame number of the chunk
    /// * `end_frame` - The ending frame number of the chunk
    /// * `page_no` - The page number to load
    pub async fn load_page(
        &self,
        branch: BranchRef,
        start_frame: u64,
        end_frame: u64,
        page_no: u32,
    ) -> io::Result<Vec<u8>> {
        let key = ChunkKey {
            db_id: branch.db_id,
            branch_id: branch.branch_id,
            start_frame,
            end_frame,
        };

        // Read the compressed/encrypted chunk
        let chunk_data = self
            .storage
            .read_chunk(&key)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        // Restore (decompress/decrypt) the chunk
        let restored = restore_chunk_from_storage(&chunk_data, &self.config)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Extract the specific page
        let offset = self.page_offset_in_chunk(page_no);
        let page_size = self.page_size as usize;

        if offset + page_size > restored.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "Page {} at offset {} exceeds chunk size {}",
                    page_no,
                    offset,
                    restored.len()
                ),
            ));
        }

        Ok(restored[offset..offset + page_size].to_vec())
    }

    /// Load multiple pages from a chunk asynchronously.
    pub async fn load_pages(
        &self,
        branch: BranchRef,
        start_frame: u64,
        end_frame: u64,
        page_numbers: &[u32],
    ) -> io::Result<Vec<(u32, Vec<u8>)>> {
        let key = ChunkKey {
            db_id: branch.db_id,
            branch_id: branch.branch_id,
            start_frame,
            end_frame,
        };

        // Read and restore the chunk once
        let chunk_data = self
            .storage
            .read_chunk(&key)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        let restored = restore_chunk_from_storage(&chunk_data, &self.config)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Extract all requested pages
        let page_size = self.page_size as usize;
        let mut results = Vec::with_capacity(page_numbers.len());

        for &page_no in page_numbers {
            let offset = self.page_offset_in_chunk(page_no);
            if offset + page_size <= restored.len() {
                results.push((page_no, restored[offset..offset + page_size].to_vec()));
            }
        }

        Ok(results)
    }
}

/// Wrapper that provides sync access to async storage using `sync_await`.
///
/// This is used by VFS/WAL implementations to call async operations
/// from within synchronous callbacks.
pub struct SyncAsyncBridge<S: AsyncChunkStorage> {
    loader: Arc<AsyncPageLoader<S>>,
}

impl<S: AsyncChunkStorage> SyncAsyncBridge<S> {
    /// Create a new sync-async bridge.
    pub fn new(loader: Arc<AsyncPageLoader<S>>) -> Self {
        Self { loader }
    }

    /// Load a page synchronously by awaiting the async operation.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a maniac task/generator context.
    #[cfg(feature = "maniac-runtime")]
    pub fn load_page_sync(
        &self,
        branch: BranchRef,
        start_frame: u64,
        end_frame: u64,
        page_no: u32,
    ) -> io::Result<Vec<u8>> {
        maniac::runtime::worker::sync_await(self.loader.load_page(
            branch,
            start_frame,
            end_frame,
            page_no,
        ))
    }

    /// Try to load a page synchronously, returning None if not in coroutine context.
    #[cfg(feature = "maniac-runtime")]
    pub fn try_load_page_sync(
        &self,
        branch: BranchRef,
        start_frame: u64,
        end_frame: u64,
        page_no: u32,
    ) -> Option<io::Result<Vec<u8>>> {
        maniac::runtime::worker::try_sync_await(self.loader.load_page(
            branch,
            start_frame,
            end_frame,
            page_no,
        ))
    }

    /// Load a page (blocking fallback when not using maniac runtime).
    #[cfg(not(feature = "maniac-runtime"))]
    pub fn load_page_sync(
        &self,
        _branch: BranchRef,
        _start_frame: u64,
        _end_frame: u64,
        _page_no: u32,
    ) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Async storage requires maniac runtime",
        ))
    }

    /// Try to load a page (stub when not using maniac runtime).
    #[cfg(not(feature = "maniac-runtime"))]
    pub fn try_load_page_sync(
        &self,
        _branch: BranchRef,
        _start_frame: u64,
        _end_frame: u64,
        _page_no: u32,
    ) -> Option<io::Result<Vec<u8>>> {
        None
    }
}

impl<S: AsyncChunkStorage> Clone for SyncAsyncBridge<S> {
    fn clone(&self) -> Self {
        Self {
            loader: self.loader.clone(),
        }
    }
}

/// Context for async WAL operations.
///
/// This provides the async backend for WAL frame persistence and retrieval.
pub struct AsyncWalContext<S: AsyncChunkStorage> {
    /// Async chunk storage backend.
    pub storage: Arc<S>,
    /// Chunk configuration (compression, encryption).
    pub config: ChunkConfig,
    /// Page size.
    pub page_size: u32,
    /// Pages per chunk.
    pub pages_per_chunk: u32,
}

impl<S: AsyncChunkStorage> AsyncWalContext<S> {
    /// Create a new async WAL context.
    pub fn new(storage: Arc<S>, config: ChunkConfig, page_size: u32, pages_per_chunk: u32) -> Self {
        Self {
            storage,
            config,
            page_size,
            pages_per_chunk,
        }
    }

    /// Create a page loader from this context.
    pub fn create_loader(&self) -> AsyncPageLoader<S> {
        AsyncPageLoader::new(
            self.storage.clone(),
            self.config.clone(),
            self.page_size,
            self.pages_per_chunk,
        )
    }

    /// Create a sync-async bridge from this context.
    pub fn create_bridge(&self) -> SyncAsyncBridge<S> {
        SyncAsyncBridge::new(Arc::new(self.create_loader()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk_storage::StorageError;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock async storage for testing.
    struct MockAsyncStorage {
        chunks: Mutex<HashMap<ChunkKey, Vec<u8>>>,
    }

    impl MockAsyncStorage {
        fn new() -> Self {
            Self {
                chunks: Mutex::new(HashMap::new()),
            }
        }
    }

    impl AsyncChunkStorage for MockAsyncStorage {
        async fn read_chunk(&self, key: &ChunkKey) -> StorageResult<Vec<u8>> {
            let chunks = self.chunks.lock().unwrap();
            chunks
                .get(key)
                .cloned()
                .ok_or_else(|| StorageError::NotFound(format!("{:?}", key)))
        }

        async fn write_chunk(&self, key: &ChunkKey, data: Vec<u8>) -> StorageResult<()> {
            let mut chunks = self.chunks.lock().unwrap();
            chunks.insert(key.clone(), data);
            Ok(())
        }

        async fn chunk_exists(&self, key: &ChunkKey) -> StorageResult<bool> {
            let chunks = self.chunks.lock().unwrap();
            Ok(chunks.contains_key(key))
        }

        async fn delete_chunk(&self, key: &ChunkKey) -> StorageResult<()> {
            let mut chunks = self.chunks.lock().unwrap();
            chunks.remove(key);
            Ok(())
        }
    }

    #[test]
    fn test_page_to_chunk_calculation() {
        let storage = Arc::new(MockAsyncStorage::new());
        let config = ChunkConfig::default();
        let loader = AsyncPageLoader::new(storage, config, 4096, 64);

        // Page 1 is in chunk 0
        assert_eq!(loader.page_to_chunk(1), 0);
        // Page 64 is in chunk 0
        assert_eq!(loader.page_to_chunk(64), 0);
        // Page 65 is in chunk 1
        assert_eq!(loader.page_to_chunk(65), 1);
        // Page 128 is in chunk 1
        assert_eq!(loader.page_to_chunk(128), 1);
        // Page 129 is in chunk 2
        assert_eq!(loader.page_to_chunk(129), 2);
    }

    #[test]
    fn test_page_offset_calculation() {
        let storage = Arc::new(MockAsyncStorage::new());
        let config = ChunkConfig::default();
        let loader = AsyncPageLoader::new(storage, config, 4096, 64);

        // Page 1 is at offset 0
        assert_eq!(loader.page_offset_in_chunk(1), 0);
        // Page 2 is at offset 4096
        assert_eq!(loader.page_offset_in_chunk(2), 4096);
        // Page 65 is at offset 0 (first page of chunk 1)
        assert_eq!(loader.page_offset_in_chunk(65), 0);
        // Page 66 is at offset 4096
        assert_eq!(loader.page_offset_in_chunk(66), 4096);
    }
}
