//! AOF sequential reader with automatic chunk hydration.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tracing::{debug, error, instrument, trace, warn};

use crate::aof::{AofError, AofInner};
use crate::io::{IoError, IoFile, IoVecMut};
use crate::local_store::{LocalChunkHandle, LocalChunkKey, LocalChunkStoreError};
use crate::manifest::ChunkId;
use crate::remote_chunk::RemoteChunkError;

const DEFAULT_HYDRATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Errors that can occur during AOF read operations.
#[derive(Debug, Error)]
pub enum AofReaderError {
    #[error("AOF is closed")]
    Closed,
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("IO driver error: {0}")]
    IoDriver(#[from] IoError),
    #[error("chunk not found: chunk_id={0}")]
    ChunkNotFound(ChunkId),
    #[error("chunk hydration failed: {0}")]
    HydrationFailed(String),
    #[error("local chunk store error: {0}")]
    LocalStore(#[from] LocalChunkStoreError),
    #[error("remote chunk error: {0}")]
    Remote(#[from] RemoteChunkError),
    #[error("AOF error: {0}")]
    Aof(#[from] AofError),
    #[error("invalid seek position: {0}")]
    InvalidSeek(u64),
}

/// Sequential cursor for reading from an AOF instance.
///
/// The cursor handles:
/// - Automatic chunk hydration from remote storage
/// - Chunk boundary transitions
/// - Concurrent download coordination
/// - Tail segment reads (mutable region)
///
/// # Example
/// ```ignore
/// let cursor = aof.open_cursor()?;
/// cursor.seek(lsn)?;
/// let mut buf = vec![0u8; 1024];
/// cursor.read(&mut buf)?;
/// ```
pub struct AofCursor {
    aof: Arc<AofInner>,
    current_lsn: u64,
    current_chunk_id: Option<ChunkId>,
    current_chunk_handle: Option<LocalChunkHandle>,
    current_chunk_file: Option<Box<dyn IoFile>>,
    hydration_timeout: Duration,
}

impl AofCursor {
    /// Create a new cursor starting at LSN 0.
    #[instrument(skip(aof), fields(aof_id = aof.id.get()))]
    pub(crate) fn new(aof: Arc<AofInner>) -> Self {
        trace!("creating new AOF cursor at LSN 0");
        Self {
            aof,
            current_lsn: 0,
            current_chunk_id: None,
            current_chunk_handle: None,
            current_chunk_file: None,
            hydration_timeout: DEFAULT_HYDRATION_TIMEOUT,
        }
    }

    /// Set the timeout for waiting on chunk hydration.
    pub fn with_hydration_timeout(mut self, timeout: Duration) -> Self {
        trace!(timeout_ms = timeout.as_millis(), "setting hydration timeout");
        self.hydration_timeout = timeout;
        self
    }

    /// Get the current LSN position of this cursor.
    pub fn position(&self) -> u64 {
        self.current_lsn
    }

    /// Seek to a specific LSN position.
    ///
    /// This will load the appropriate chunk if not already loaded.
    /// If the chunk is not in the local cache, it will be downloaded from remote storage.
    #[instrument(skip(self), fields(
        current_lsn = self.current_lsn,
        target_lsn = lsn,
        current_chunk_id = ?self.current_chunk_id
    ))]
    pub fn seek(&mut self, lsn: u64) -> Result<(), AofReaderError> {
        if self.aof.is_closed() {
            error!("attempted to seek on closed AOF");
            return Err(AofReaderError::Closed);
        }

        let target_chunk_id = self.chunk_id_for_lsn(lsn);
        trace!(target_chunk_id, "calculated target chunk for LSN");

        // If we're already in the right chunk, just update the LSN
        if self.current_chunk_id == Some(target_chunk_id) {
            trace!("already in target chunk, updating position only");
            self.current_lsn = lsn;
            return Ok(());
        }

        // Load the new chunk
        debug!(target_chunk_id, "loading new chunk for seek operation");
        self.load_chunk(target_chunk_id)?;
        self.current_lsn = lsn;
        debug!(lsn, chunk_id = target_chunk_id, "seek completed successfully");

        Ok(())
    }

    /// Read data from the current position, advancing the cursor.
    ///
    /// This method handles chunk boundaries automatically, switching to the next
    /// chunk when needed.
    #[instrument(skip(self, buf), fields(
        position = self.current_lsn,
        chunk_id = ?self.current_chunk_id,
        requested_bytes = buf.len()
    ))]
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, AofReaderError> {
        if self.aof.is_closed() {
            error!("attempted to read from closed AOF");
            return Err(AofReaderError::Closed);
        }

        if buf.is_empty() {
            trace!("empty buffer provided, returning 0");
            return Ok(0);
        }

        let chunk_id = self.chunk_id_for_lsn(self.current_lsn);

        // Ensure we have the right chunk loaded
        if self.current_chunk_id != Some(chunk_id) {
            debug!(chunk_id, "switching to new chunk");
            self.load_chunk(chunk_id)?;
        }

        let chunk_offset = self.chunk_offset(self.current_lsn);
        let chunk_end = self.chunk_end_lsn(chunk_id);
        let remaining_in_chunk = (chunk_end - self.current_lsn) as usize;
        let to_read = buf.len().min(remaining_in_chunk);

        if to_read == 0 {
            warn!(chunk_id, position = self.current_lsn, "at chunk boundary, no bytes to read");
            return Ok(0);
        }

        let file = self.current_chunk_file.as_mut()
            .ok_or_else(|| {
                error!(chunk_id, "chunk file not loaded");
                AofReaderError::ChunkNotFound(chunk_id)
            })?;

        // Read from the file at the correct offset
        trace!(chunk_offset, to_read, "reading from chunk file");
        let io_buf = IoVecMut::new(&mut buf[..to_read]);
        let read = file.readv_at(chunk_offset, &mut [io_buf])
            .map_err(|e| {
                error!(error = ?e, chunk_id, chunk_offset, to_read, "I/O read failed");
                AofReaderError::IoDriver(e)
            })?;

        self.current_lsn += read as u64;
        debug!(bytes_read = read, new_position = self.current_lsn, "read completed successfully");

        Ok(read)
    }

    /// Read exact amount or return an error.
    #[instrument(skip(self, buf), fields(
        position = self.current_lsn,
        required_bytes = buf.len()
    ))]
    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), AofReaderError> {
        trace!("starting exact read");
        let mut offset = 0;
        while offset < buf.len() {
            let read = self.read(&mut buf[offset..])?;
            if read == 0 {
                error!(
                    offset,
                    required = buf.len(),
                    "unexpected EOF during exact read"
                );
                return Err(AofReaderError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected end of AOF",
                )));
            }
            offset += read;
            trace!(offset, remaining = buf.len() - offset, "partial read in read_exact");
        }
        debug!(bytes_read = buf.len(), "exact read completed successfully");
        Ok(())
    }

    /// Check if more data is available at the current position.
    pub fn has_data(&self) -> bool {
        // TODO: Implement proper end-of-AOF detection
        // This should check against the highest durable LSN
        true
    }

    #[instrument(skip(self), fields(
        chunk_id,
        aof_id = self.aof.id.get(),
        timeout_ms = self.hydration_timeout.as_millis()
    ))]
    fn load_chunk(&mut self, chunk_id: ChunkId) -> Result<(), AofReaderError> {
        let key = LocalChunkKey::new(self.aof.id.into(), chunk_id, 0);
        trace!(?key, "loading chunk");

        // First, check if it's already in local cache
        if let Some(handle) = self.aof.local_chunks.get(&key) {
            debug!(path = ?handle.path, size_bytes = handle.size_bytes, "chunk found in local cache");
            self.open_chunk_file(chunk_id, handle)?;
            return Ok(());
        }

        // Wait if another thread is downloading
        trace!("chunk not in cache, waiting for concurrent download if in progress");
        if let Some(handle) = self.aof.local_chunks.get_or_wait(&key, self.hydration_timeout) {
            debug!(path = ?handle.path, size_bytes = handle.size_bytes, "chunk became available after waiting");
            self.open_chunk_file(chunk_id, handle)?;
            return Ok(());
        }

        // We need to hydrate from remote
        debug!("initiating chunk hydration from remote storage");
        self.hydrate_chunk(chunk_id)?;

        Ok(())
    }

    #[instrument(skip(self), fields(chunk_id, aof_id = self.aof.id.get()))]
    fn hydrate_chunk(&mut self, chunk_id: ChunkId) -> Result<(), AofReaderError> {
        // Query manifest for chunk metadata
        // For now, we'll return an error if chunk isn't available
        // In a full implementation, this would:
        // 1. Query manifest for RemoteChunkSpec
        // 2. Call RemoteChunkStore::hydrate()
        // 3. Open the resulting file

        trace!("attempting chunk hydration");
        let _key = LocalChunkKey::new(self.aof.id.into(), chunk_id, 0);

        // Try to ensure the chunk is available
        // This is a simplified version - full implementation would query manifest
        let _remote_store = self.aof.remote_chunks.as_ref()
            .ok_or_else(|| {
                error!("no remote chunk store configured");
                AofReaderError::HydrationFailed(
                    "no remote chunk store configured".to_string()
                )
            })?;

        // TODO: Query manifest for chunk metadata to build RemoteChunkSpec
        // For now, return an error
        error!(chunk_id, "chunk hydration not fully implemented");
        Err(AofReaderError::ChunkNotFound(chunk_id))
    }

    #[instrument(skip(self, handle), fields(
        chunk_id,
        path = ?handle.path,
        size_bytes = handle.size_bytes
    ))]
    fn open_chunk_file(
        &mut self,
        chunk_id: ChunkId,
        handle: LocalChunkHandle,
    ) -> Result<(), AofReaderError> {
        trace!("opening chunk file for reading");
        let file = self.aof.io.open(
            handle.path.as_path(),
            &crate::io::IoOpenOptions::read_only(),
        ).map_err(|e| {
            error!(error = ?e, "failed to open chunk file");
            e
        })?;

        self.current_chunk_id = Some(chunk_id);
        self.current_chunk_handle = Some(handle);
        self.current_chunk_file = Some(file);
        debug!("chunk file opened successfully");

        Ok(())
    }

    fn chunk_id_for_lsn(&self, lsn: u64) -> ChunkId {
        (lsn / self.aof.chunk_size_bytes) as ChunkId
    }

    fn chunk_start_lsn(&self, chunk_id: ChunkId) -> u64 {
        (chunk_id as u64) * self.aof.chunk_size_bytes
    }

    fn chunk_end_lsn(&self, chunk_id: ChunkId) -> u64 {
        ((chunk_id as u64) + 1) * self.aof.chunk_size_bytes
    }

    fn chunk_offset(&self, lsn: u64) -> u64 {
        lsn % self.aof.chunk_size_bytes
    }
}

impl Read for AofCursor {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        trace!(buf_len = buf.len(), position = self.current_lsn, "Read trait implementation called");
        self.read(buf).map_err(|e| {
            match e {
                AofReaderError::Io(io_err) => io_err,
                other => {
                    error!(error = ?other, "converting AofReaderError to io::Error");
                    io::Error::new(io::ErrorKind::Other, other)
                }
            }
        })
    }
}

impl Seek for AofCursor {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        trace!(current_position = self.current_lsn, seek_from = ?pos, "Seek trait implementation called");
        let target_lsn = match pos {
            SeekFrom::Start(lsn) => {
                trace!(target_lsn = lsn, "seeking from start");
                lsn
            }
            SeekFrom::Current(offset) => {
                let target = if offset >= 0 {
                    self.current_lsn.saturating_add(offset as u64)
                } else {
                    self.current_lsn.saturating_sub((-offset) as u64)
                };
                trace!(offset, target_lsn = target, "seeking from current");
                target
            }
            SeekFrom::End(_) => {
                warn!("seek from end not supported");
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "seek from end not supported",
                ));
            }
        };

        self.seek(target_lsn).map_err(|e| {
            match e {
                AofReaderError::Io(io_err) => io_err,
                other => {
                    error!(error = ?other, "converting AofReaderError to io::Error in Seek trait");
                    io::Error::new(io::ErrorKind::Other, other)
                }
            }
        })?;

        debug!(final_position = self.current_lsn, "Seek trait implementation completed");
        Ok(self.current_lsn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: Add tests for AofCursor
    // - Test seek and read across chunk boundaries
    // - Test hydration from remote
    // - Test concurrent download coordination
}
