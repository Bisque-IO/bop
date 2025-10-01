use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::Arc;

use thiserror::Error;
use zstd::stream::read::Decoder;
use zstd::stream::write::Encoder;

use crate::local_store::{LocalChunkHandle, LocalChunkKey, LocalChunkStore, LocalChunkStoreError};
use crate::manifest::{ChunkId, DbId, Generation, RemoteObjectKey};

/// Description of a remote chunk object.
#[derive(Debug, Clone)]
pub struct RemoteChunkSpec {
    pub db_id: DbId,
    pub chunk_id: ChunkId,
    pub generation: Generation,
    pub remote_key: RemoteObjectKey,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
}

impl RemoteChunkSpec {
    pub fn cache_key(&self) -> LocalChunkKey {
        LocalChunkKey::new(self.db_id, self.chunk_id, self.generation)
    }
}

/// Errors that can occur while hydrating chunks from remote storage.
#[derive(Debug, Error)]
pub enum RemoteChunkError {
    #[error("remote fetch failed: {0}")]
    Fetch(String),
    #[error("remote upload failed: {0}")]
    Upload(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("local cache error: {0}")]
    Local(#[from] LocalChunkStoreError),
    #[error("compression error: {0}")]
    Compression(String),
}

/// Request to upload a chunk to remote storage.
#[derive(Debug, Clone)]
pub struct RemoteUploadRequest {
    pub db_id: DbId,
    pub chunk_id: ChunkId,
    pub generation: Generation,
    pub remote_key: RemoteObjectKey,
    pub local_path: std::path::PathBuf,
}

/// Result of uploading a chunk to remote storage.
#[derive(Debug, Clone)]
pub struct RemoteUploadResult {
    pub remote_key: RemoteObjectKey,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
}

/// Trait implemented by backends capable of fetching and uploading remote chunk objects.
pub trait RemoteChunkFetcher: Send + Sync {
    /// Fetch a chunk from remote storage, returning a reader for compressed data.
    fn fetch(&self, spec: &RemoteChunkSpec) -> Result<Box<dyn Read + Send>, RemoteChunkError>;

    /// Upload a chunk to remote storage, accepting a reader for compressed data.
    fn upload(
        &self,
        remote_key: &RemoteObjectKey,
        compressed_data: &mut dyn Read,
        compressed_size: u64,
    ) -> Result<(), RemoteChunkError>;
}

/// High-level helper that downloads and installs remote chunks into a local cache.
#[derive(Clone)]
pub struct RemoteChunkStore {
    fetcher: Arc<dyn RemoteChunkFetcher>,
}

impl RemoteChunkStore {
    pub fn new(fetcher: Arc<dyn RemoteChunkFetcher>) -> Self {
        Self { fetcher }
    }

    /// Download and decompress a chunk from remote storage into the local cache.
    pub fn hydrate(
        &self,
        spec: &RemoteChunkSpec,
        local: &LocalChunkStore,
    ) -> Result<LocalChunkHandle, RemoteChunkError> {
        let remote_reader = self.fetcher.fetch(spec)?;
        let mut decoder = Decoder::new(remote_reader)?;
        let handle =
            local.insert_from_reader(spec.cache_key(), spec.uncompressed_size, &mut decoder)?;
        Ok(handle)
    }

    /// Compress and upload a chunk from the local filesystem to remote storage.
    /// Returns metadata about the uploaded chunk.
    pub fn upload(
        &self,
        request: &RemoteUploadRequest,
    ) -> Result<RemoteUploadResult, RemoteChunkError> {
        let uncompressed_size = std::fs::metadata(&request.local_path)?.len();

        // Compress to a temporary buffer
        let mut compressed = Vec::new();
        {
            let mut encoder = Encoder::new(&mut compressed, 3)
                .map_err(|e| RemoteChunkError::Compression(e.to_string()))?;

            let mut source = File::open(&request.local_path)?;
            std::io::copy(&mut source, &mut encoder)?;

            encoder.finish()
                .map_err(|e| RemoteChunkError::Compression(e.to_string()))?;
        }

        let compressed_size = compressed.len() as u64;

        // Upload compressed data
        let mut cursor = std::io::Cursor::new(compressed);
        self.fetcher.upload(&request.remote_key, &mut cursor, compressed_size)?;

        Ok(RemoteUploadResult {
            remote_key: request.remote_key.clone(),
            compressed_size,
            uncompressed_size,
        })
    }

    /// Upload a chunk that's already in the local cache.
    pub fn upload_from_handle(
        &self,
        db_id: DbId,
        chunk_id: ChunkId,
        generation: Generation,
        remote_key: RemoteObjectKey,
        handle: &LocalChunkHandle,
    ) -> Result<RemoteUploadResult, RemoteChunkError> {
        let request = RemoteUploadRequest {
            db_id,
            chunk_id,
            generation,
            remote_key,
            local_path: handle.path.clone(),
        };
        self.upload(&request)
    }

    /// Compress a local file and return the compressed data and sizes.
    /// Useful for testing or streaming uploads.
    pub fn compress_file(
        local_path: &Path,
        compression_level: i32,
    ) -> Result<(Vec<u8>, u64, u64), RemoteChunkError> {
        let uncompressed_size = std::fs::metadata(local_path)?.len();

        let mut compressed = Vec::new();
        {
            let mut encoder = Encoder::new(&mut compressed, compression_level)
                .map_err(|e| RemoteChunkError::Compression(e.to_string()))?;

            let mut source = File::open(local_path)?;
            std::io::copy(&mut source, &mut encoder)?;

            encoder.finish()
                .map_err(|e| RemoteChunkError::Compression(e.to_string()))?;
        }

        let compressed_size = compressed.len() as u64;

        Ok((compressed, compressed_size, uncompressed_size))
    }
}
