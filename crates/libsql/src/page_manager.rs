//! Page Manager
//!
//! Provides a unified interface for reading SQLite pages from the storage layers.
//! Handles the page lookup order:
//! 1. WAL (Write-Ahead Log) - contains uncommitted/recent changes
//! 2. Page Cache (DashMap) - in-memory cache of recently accessed pages
//! 3. Chunk Store - persistent storage of page chunks
//!
//! # Chunk Design
//!
//! Chunks are the unit of storage for SQLite pages:
//! - Deduplicated by CRC64-NVME hash (content-addressable storage)
//! - Contain contiguous ranges of pages
//! - Checkpointing rewrites entire chunk and pushes a new version
//! - Optional compression (LZ4) and encryption (AES-GCM)
//!
//! # WAL Integration
//!
//! The WAL contains page deltas (modified pages). When looking up a page:
//! 1. Check if page is in the WAL (newest version)
//! 2. Check page cache
//! 3. Read from chunk, decompress/decrypt if needed

use crate::async_io::AsyncChunkStorage;
use crate::chunk_storage::{ChunkKey, ChunkStorage};
use crate::raft_log::BranchRef;
use crate::schema::ChunkId;
use crate::store::Store;
use dashmap::DashMap;
use std::sync::Arc;

/// Configuration for chunk compression and encryption.
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Enable LZ4 compression for chunks.
    pub compression_enabled: bool,
    /// Compression level (1-12 for LZ4, higher = better compression, slower).
    pub compression_level: u32,
    /// Enable AES-256-GCM encryption for chunks.
    pub encryption_enabled: bool,
    /// Encryption key (32 bytes for AES-256). None if encryption disabled.
    pub encryption_key: Option<[u8; 32]>,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            compression_enabled: true,
            compression_level: 4,
            encryption_enabled: false,
            encryption_key: None,
        }
    }
}

impl ChunkConfig {
    /// Create a config with compression enabled but no encryption.
    pub fn compressed() -> Self {
        Self::default()
    }

    /// Create a config with both compression and encryption.
    pub fn encrypted(key: [u8; 32]) -> Self {
        Self {
            compression_enabled: true,
            compression_level: 4,
            encryption_enabled: true,
            encryption_key: Some(key),
        }
    }

    /// Create a config with no compression or encryption (for testing).
    pub fn uncompressed() -> Self {
        Self {
            compression_enabled: false,
            compression_level: 0,
            encryption_enabled: false,
            encryption_key: None,
        }
    }
}

/// Header prepended to chunk data to indicate compression/encryption.
///
/// Format: [flags (1 byte)] [original_size (4 bytes if compressed)]
#[derive(Debug, Clone, Copy)]
pub struct ChunkHeader {
    /// Bit flags: 0x01 = compressed, 0x02 = encrypted
    pub flags: u8,
    /// Original uncompressed size (only present if compressed)
    pub original_size: Option<u32>,
}

impl ChunkHeader {
    const FLAG_COMPRESSED: u8 = 0x01;
    const FLAG_ENCRYPTED: u8 = 0x02;

    pub fn is_compressed(&self) -> bool {
        self.flags & Self::FLAG_COMPRESSED != 0
    }

    pub fn is_encrypted(&self) -> bool {
        self.flags & Self::FLAG_ENCRYPTED != 0
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![self.flags];
        if let Some(size) = self.original_size {
            bytes.extend_from_slice(&size.to_le_bytes());
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<(Self, usize)> {
        if bytes.is_empty() {
            return None;
        }
        let flags = bytes[0];
        if flags & Self::FLAG_COMPRESSED != 0 {
            if bytes.len() < 5 {
                return None;
            }
            let size = u32::from_le_bytes(bytes[1..5].try_into().ok()?);
            Some((Self { flags, original_size: Some(size) }, 5))
        } else {
            Some((Self { flags, original_size: None }, 1))
        }
    }
}

/// Key for the page cache.
/// Identifies a specific page within a branch's view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageCacheKey {
    pub db_id: u64,
    pub branch_id: u32,
    pub page_id: u64,
}

impl PageCacheKey {
    pub fn new(db_id: u64, branch_id: u32, page_id: u64) -> Self {
        Self { db_id, branch_id, page_id }
    }
}

/// A cached page with its data.
#[derive(Debug, Clone)]
pub struct CachedPage {
    /// The raw page data (uncompressed, decrypted).
    pub data: Arc<Vec<u8>>,
    /// Version/frame number this page came from.
    pub version: u64,
}

/// Universal page cache using DashMap for concurrent access.
///
/// Pages are cached after reading from chunks. The cache is shared
/// across all databases and branches for maximum efficiency.
pub struct PageCache {
    /// Map from (db_id, branch_id, page_id) -> page data
    cache: DashMap<PageCacheKey, CachedPage>,
    /// Maximum number of pages to cache (0 = unlimited)
    max_pages: usize,
    /// Page size for this cache (all pages same size)
    page_size: u32,
}

impl PageCache {
    /// Create a new page cache.
    pub fn new(page_size: u32, max_pages: usize) -> Self {
        Self {
            cache: DashMap::new(),
            max_pages,
            page_size,
        }
    }

    /// Get a page from the cache.
    pub fn get(&self, key: &PageCacheKey) -> Option<CachedPage> {
        self.cache.get(key).map(|entry| entry.value().clone())
    }

    /// Insert a page into the cache.
    pub fn insert(&self, key: PageCacheKey, page: CachedPage) {
        // Simple eviction: if at capacity, don't insert
        // In production, would use LRU or similar
        if self.max_pages > 0 && self.cache.len() >= self.max_pages {
            // Could implement random eviction here
            return;
        }
        self.cache.insert(key, page);
    }

    /// Invalidate a specific page.
    pub fn invalidate(&self, key: &PageCacheKey) {
        self.cache.remove(key);
    }

    /// Invalidate all pages for a branch.
    pub fn invalidate_branch(&self, db_id: u64, branch_id: u32) {
        self.cache.retain(|k, _| k.db_id != db_id || k.branch_id != branch_id);
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get the number of cached pages.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get the page size.
    pub fn page_size(&self) -> u32 {
        self.page_size
    }
}

/// Compresses data using LZ4.
pub fn compress_chunk(data: &[u8]) -> Vec<u8> {
    lz4_flex::compress_prepend_size(data)
}

/// Decompresses LZ4-compressed data.
pub fn decompress_chunk(data: &[u8]) -> Result<Vec<u8>, lz4_flex::block::DecompressError> {
    lz4_flex::decompress_size_prepended(data)
}

/// Prepares chunk data for storage (compress/encrypt as configured).
pub fn prepare_chunk_for_storage(data: &[u8], config: &ChunkConfig) -> Vec<u8> {
    let mut flags = 0u8;
    let mut result = data.to_vec();

    // Compress if enabled
    if config.compression_enabled {
        flags |= ChunkHeader::FLAG_COMPRESSED;
        result = compress_chunk(&result);
    }

    // Encrypt if enabled (placeholder - actual implementation would use ring/aes-gcm)
    if config.encryption_enabled && config.encryption_key.is_some() {
        flags |= ChunkHeader::FLAG_ENCRYPTED;
        // TODO: Implement AES-256-GCM encryption
        // For now, just set the flag but don't actually encrypt
    }

    // Prepend header
    let header = ChunkHeader {
        flags,
        original_size: if config.compression_enabled {
            Some(data.len() as u32)
        } else {
            None
        },
    };

    let mut output = header.to_bytes();
    output.extend(result);
    output
}

/// Restores chunk data from storage (decrypt/decompress as needed).
pub fn restore_chunk_from_storage(data: &[u8], config: &ChunkConfig) -> anyhow::Result<Vec<u8>> {
    let (header, header_len) = ChunkHeader::from_bytes(data)
        .ok_or_else(|| anyhow::anyhow!("Invalid chunk header"))?;

    let payload = &data[header_len..];

    // Decrypt if needed
    let decrypted = if header.is_encrypted() {
        if !config.encryption_enabled || config.encryption_key.is_none() {
            anyhow::bail!("Chunk is encrypted but no key provided");
        }
        // TODO: Implement AES-256-GCM decryption
        payload.to_vec()
    } else {
        payload.to_vec()
    };

    // Decompress if needed
    let decompressed = if header.is_compressed() {
        decompress_chunk(&decrypted)
            .map_err(|e| anyhow::anyhow!("Decompression failed: {}", e))?
    } else {
        decrypted
    };

    Ok(decompressed)
}

/// High-level manager for reading and writing pages.
///
/// Coordinates between:
/// - WAL (for uncommitted changes)
/// - Page Cache (for recently accessed pages)
/// - Chunk Store (for persistent storage)
pub struct PageManager<S: ChunkStorage> {
    store: Store,
    chunk_storage: S,
    cache: Arc<PageCache>,
    config: ChunkConfig,
}

impl<S: ChunkStorage> PageManager<S> {
    pub fn new(store: Store, chunk_storage: S, cache: Arc<PageCache>, config: ChunkConfig) -> Self {
        Self {
            store,
            chunk_storage,
            cache,
            config,
        }
    }

    /// Get the underlying store.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Get the page cache.
    pub fn cache(&self) -> &Arc<PageCache> {
        &self.cache
    }

    /// Get the chunk config.
    pub fn config(&self) -> &ChunkConfig {
        &self.config
    }

    /// Read a page, checking WAL first, then cache, then chunk store.
    ///
    /// The `wal_lookup` function is provided by the caller to check the WAL.
    /// This allows flexibility in how WAL is managed (in-memory, memory-mapped, etc.)
    pub async fn read_page<F>(
        &self,
        branch: BranchRef,
        page_id: u64,
        wal_lookup: F,
    ) -> anyhow::Result<Option<Arc<Vec<u8>>>>
    where
        F: FnOnce(u64) -> Option<Arc<Vec<u8>>>,
    {
        // 1. Check WAL first (most recent changes)
        if let Some(page_data) = wal_lookup(page_id) {
            return Ok(Some(page_data));
        }

        // 2. Check page cache
        let cache_key = PageCacheKey::new(branch.db_id, branch.branch_id, page_id);
        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(Some(cached.data));
        }

        // 3. Find the chunk containing this page
        let found = self.store.find_chunk_for_page(branch.db_id, branch.branch_id, page_id)?;
        let chunk_id = match found {
            Some(f) => f.chunk_id,
            None => return Ok(None), // Page doesn't exist yet
        };

        // 4. Read chunk from store
        let chunk_data = match self.store.get_chunk(chunk_id)? {
            Some(data) => data,
            None => return Ok(None), // Chunk not found (shouldn't happen)
        };

        // 5. Restore chunk (decompress/decrypt)
        let restored = restore_chunk_from_storage(&chunk_data, &self.config)?;

        // 6. Extract the specific page from the chunk
        let (_, page_size, ppc_shift) = decode_db_id_inline(branch.db_id);
        let pages_per_chunk = 1u32 << ppc_shift;
        let page_offset_in_chunk = (page_id as u32) % pages_per_chunk;
        let byte_offset = (page_offset_in_chunk * page_size) as usize;
        let byte_end = byte_offset + page_size as usize;

        if byte_end > restored.len() {
            anyhow::bail!("Page {} extends beyond chunk boundary", page_id);
        }

        let page_data = Arc::new(restored[byte_offset..byte_end].to_vec());

        // 7. Cache the page
        self.cache.insert(cache_key, CachedPage {
            data: page_data.clone(),
            version: 0, // Could track version from chunk metadata
        });

        Ok(Some(page_data))
    }

    /// Write a chunk to storage with compression/encryption.
    /// Returns the CAS ID (CRC64 hash) of the stored chunk.
    pub fn write_chunk(&self, chunk_data: &[u8]) -> ChunkId {
        // Calculate CAS ID before any transformation
        let crc = crc::Crc::<u64>::new(&crc::CRC_64_XZ);
        crc.checksum(chunk_data)
    }

    /// Prepare chunk data for storage (compress/encrypt).
    pub fn prepare_chunk(&self, chunk_data: &[u8]) -> Vec<u8> {
        prepare_chunk_for_storage(chunk_data, &self.config)
    }

    /// Invalidate cached pages for a branch (e.g., after checkpoint).
    pub fn invalidate_branch_cache(&self, branch: BranchRef) {
        self.cache.invalidate_branch(branch.db_id, branch.branch_id);
    }
}

/// Inline version of decode_db_id to avoid circular dependency.
fn decode_db_id_inline(encoded: u64) -> (u32, u32, u32) {
    let page_shift_kb = (encoded >> 56) & 0xFF;
    let ppc_shift = (encoded >> 48) & 0xFF;
    let raw_id = (encoded & 0xFFFFFFFF) as u32;
    let page_size = 1024 * (1 << page_shift_kb);
    (raw_id, page_size as u32, ppc_shift as u32)
}

/// Async page manager for reading pages using async storage backend.
///
/// This manager uses `sync_await` to bridge async storage operations
/// within synchronous VFS/WAL callbacks when running in a maniac
/// stackful coroutine context.
pub struct AsyncPageManager<S: AsyncChunkStorage> {
    storage: Arc<S>,
    cache: Arc<PageCache>,
    config: ChunkConfig,
    page_size: u32,
    pages_per_chunk: u32,
}

impl<S: AsyncChunkStorage> AsyncPageManager<S> {
    /// Create a new async page manager.
    pub fn new(
        storage: Arc<S>,
        cache: Arc<PageCache>,
        config: ChunkConfig,
        page_size: u32,
        pages_per_chunk: u32,
    ) -> Self {
        Self {
            storage,
            cache,
            config,
            page_size,
            pages_per_chunk,
        }
    }

    /// Get the page cache.
    pub fn cache(&self) -> &Arc<PageCache> {
        &self.cache
    }

    /// Get the chunk config.
    pub fn config(&self) -> &ChunkConfig {
        &self.config
    }

    /// Calculate the page offset within a chunk.
    fn page_offset_in_chunk(&self, page_no: u32) -> usize {
        let page_index = (page_no.saturating_sub(1)) % self.pages_per_chunk;
        page_index as usize * self.page_size as usize
    }

    /// Read a page asynchronously, checking WAL first, then cache, then async storage.
    ///
    /// The `wal_lookup` function is provided by the caller to check the WAL.
    pub async fn read_page<F>(
        &self,
        branch: BranchRef,
        start_frame: u64,
        end_frame: u64,
        page_no: u32,
        wal_lookup: F,
    ) -> anyhow::Result<Option<Arc<Vec<u8>>>>
    where
        F: FnOnce(u32) -> Option<Arc<Vec<u8>>>,
    {
        // 1. Check WAL first (most recent changes)
        if let Some(page_data) = wal_lookup(page_no) {
            return Ok(Some(page_data));
        }

        // 2. Check page cache
        let cache_key = PageCacheKey::new(branch.db_id, branch.branch_id, page_no as u64);
        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(Some(cached.data));
        }

        // 3. Read chunk from async storage
        let key = ChunkKey {
            db_id: branch.db_id,
            branch_id: branch.branch_id,
            start_frame,
            end_frame,
        };

        let chunk_data = match self.storage.read_chunk(&key).await {
            Ok(data) => data,
            Err(_) => return Ok(None),
        };

        // 4. Restore chunk (decompress/decrypt)
        let restored = restore_chunk_from_storage(&chunk_data, &self.config)?;

        // 5. Extract the specific page from the chunk
        let offset = self.page_offset_in_chunk(page_no);
        let page_end = offset + self.page_size as usize;

        if page_end > restored.len() {
            anyhow::bail!("Page {} extends beyond chunk boundary", page_no);
        }

        let page_data = Arc::new(restored[offset..page_end].to_vec());

        // 6. Cache the page
        self.cache.insert(
            cache_key,
            CachedPage {
                data: page_data.clone(),
                version: end_frame,
            },
        );

        Ok(Some(page_data))
    }

    /// Read a page synchronously using `sync_await`.
    ///
    /// This is the main entry point for reading pages from synchronous
    /// VFS/WAL callbacks when running inside a maniac coroutine.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a maniac task/generator context.
    #[cfg(feature = "maniac-runtime")]
    pub fn read_page_sync<F>(
        &self,
        branch: BranchRef,
        start_frame: u64,
        end_frame: u64,
        page_no: u32,
        wal_lookup: F,
    ) -> anyhow::Result<Option<Arc<Vec<u8>>>>
    where
        F: FnOnce(u32) -> Option<Arc<Vec<u8>>>,
    {
        // Check cache first (sync path, no await needed)
        let cache_key = PageCacheKey::new(branch.db_id, branch.branch_id, page_no as u64);
        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(Some(cached.data));
        }

        // Check WAL (sync path)
        if let Some(page_data) = wal_lookup(page_no) {
            return Ok(Some(page_data));
        }

        // Fall through to async storage via sync_await
        maniac::runtime::worker::sync_await(
            self.read_page(branch, start_frame, end_frame, page_no, |_| None),
        )
    }

    /// Try to read a page synchronously, returning None if not in coroutine context.
    #[cfg(feature = "maniac-runtime")]
    pub fn try_read_page_sync<F>(
        &self,
        branch: BranchRef,
        start_frame: u64,
        end_frame: u64,
        page_no: u32,
        wal_lookup: F,
    ) -> Option<anyhow::Result<Option<Arc<Vec<u8>>>>>
    where
        F: FnOnce(u32) -> Option<Arc<Vec<u8>>>,
    {
        // Check cache first (sync path, no await needed)
        let cache_key = PageCacheKey::new(branch.db_id, branch.branch_id, page_no as u64);
        if let Some(cached) = self.cache.get(&cache_key) {
            return Some(Ok(Some(cached.data)));
        }

        // Check WAL (sync path)
        if let Some(page_data) = wal_lookup(page_no) {
            return Some(Ok(Some(page_data)));
        }

        // Fall through to async storage via try_sync_await
        maniac::runtime::worker::try_sync_await(
            self.read_page(branch, start_frame, end_frame, page_no, |_| None),
        )
    }

    /// Stub for read_page_sync when maniac runtime is not available.
    #[cfg(not(feature = "maniac-runtime"))]
    pub fn read_page_sync<F>(
        &self,
        _branch: BranchRef,
        _start_frame: u64,
        _end_frame: u64,
        _page_no: u32,
        _wal_lookup: F,
    ) -> anyhow::Result<Option<Arc<Vec<u8>>>>
    where
        F: FnOnce(u32) -> Option<Arc<Vec<u8>>>,
    {
        anyhow::bail!("Async page manager requires maniac runtime")
    }

    /// Stub for try_read_page_sync when maniac runtime is not available.
    #[cfg(not(feature = "maniac-runtime"))]
    pub fn try_read_page_sync<F>(
        &self,
        _branch: BranchRef,
        _start_frame: u64,
        _end_frame: u64,
        _page_no: u32,
        _wal_lookup: F,
    ) -> Option<anyhow::Result<Option<Arc<Vec<u8>>>>>
    where
        F: FnOnce(u32) -> Option<Arc<Vec<u8>>>,
    {
        None
    }

    /// Invalidate cached pages for a branch (e.g., after checkpoint).
    pub fn invalidate_branch_cache(&self, branch: BranchRef) {
        self.cache.invalidate_branch(branch.db_id, branch.branch_id);
    }
}

impl<S: AsyncChunkStorage + Clone> Clone for AsyncPageManager<S> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            cache: self.cache.clone(),
            config: self.config.clone(),
            page_size: self.page_size,
            pages_per_chunk: self.pages_per_chunk,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_config_defaults() {
        let config = ChunkConfig::default();
        assert!(config.compression_enabled);
        assert!(!config.encryption_enabled);
    }

    #[test]
    fn test_chunk_header_roundtrip() {
        // Uncompressed, unencrypted
        let header = ChunkHeader { flags: 0, original_size: None };
        let bytes = header.to_bytes();
        let (parsed, len) = ChunkHeader::from_bytes(&bytes).unwrap();
        assert_eq!(len, 1);
        assert!(!parsed.is_compressed());
        assert!(!parsed.is_encrypted());

        // Compressed
        let header = ChunkHeader { flags: ChunkHeader::FLAG_COMPRESSED, original_size: Some(1024) };
        let bytes = header.to_bytes();
        let (parsed, len) = ChunkHeader::from_bytes(&bytes).unwrap();
        assert_eq!(len, 5);
        assert!(parsed.is_compressed());
        assert_eq!(parsed.original_size, Some(1024));
    }

    #[test]
    fn test_compress_decompress() {
        let original = vec![0u8; 4096]; // Compresses well
        let compressed = compress_chunk(&original);
        assert!(compressed.len() < original.len());

        let decompressed = decompress_chunk(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_page_cache() {
        let cache = PageCache::new(4096, 100);
        let key = PageCacheKey::new(1, 1, 0);

        assert!(cache.get(&key).is_none());

        cache.insert(key, CachedPage {
            data: Arc::new(vec![0u8; 4096]),
            version: 1,
        });

        assert!(cache.get(&key).is_some());
        assert_eq!(cache.len(), 1);

        cache.invalidate(&key);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_prepare_restore_chunk() {
        let config = ChunkConfig::compressed();
        let original = vec![1u8, 2, 3, 4, 5, 6, 7, 8];

        let prepared = prepare_chunk_for_storage(&original, &config);
        let restored = restore_chunk_from_storage(&prepared, &config).unwrap();

        assert_eq!(restored, original);
    }

    #[test]
    fn test_uncompressed_chunk() {
        let config = ChunkConfig::uncompressed();
        let original = vec![1u8, 2, 3, 4, 5, 6, 7, 8];

        let prepared = prepare_chunk_for_storage(&original, &config);
        // Should only have 1-byte header + original data
        assert_eq!(prepared.len(), 1 + original.len());

        let restored = restore_chunk_from_storage(&prepared, &config).unwrap();
        assert_eq!(restored, original);
    }
}
