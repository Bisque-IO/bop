//! WAL Storage Layer
//!
//! This module provides a unified WAL (Write-Ahead Log) system that:
//!
//! 1. **Local persistence**: Writes frames to local filesystem via `maniac::fs` with fsync
//!    for immediate durability on each commit
//! 2. **Remote replication**: Periodically flushes segments to remote storage (S3, etc.)
//!    for disaster recovery and replication
//!
//! # Architecture
//!
//! ```text
//! SQLite commit
//!     |
//!     v
//! WalWriter
//!     |-> Local WAL file (maniac::fs + fsync) <-- Immediate durability
//!     |-> Pending buffer
//!             |
//!             v (on interval/size trigger)
//!         Remote storage (S3, etc.) <-- Disaster recovery
//!
//! WAL Segment Format:
//! +------------------+------------------+------------------+
//! | Segment Header   | Frame 1          | Frame 2          | ...
//! +------------------+------------------+------------------+
//!
//! Segment Header (64 bytes):
//! - magic: u32 (0x57414C53 = "WALS")
//! - version: u16
//! - flags: u16
//! - db_id: u64
//! - branch_id: u32
//! - start_frame: u64
//! - end_frame: u64
//! - frame_count: u32
//! - page_size: u32
//! - checksum: u64 (CRC64 of all frame data)
//!
//! Frame Format (8 + page_size bytes):
//! - page_no: u32
//! - db_size: u32 (0 if not a commit frame)
//! - data: [u8; page_size]
//! ```

use crate::raft_log::BranchRef;
use crate::virtual_wal::WalFrame;
use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Magic number for WAL segment files: "WALS"
pub const WAL_SEGMENT_MAGIC: u32 = 0x57414C53;

/// Current WAL segment format version
pub const WAL_SEGMENT_VERSION: u16 = 1;

/// Default flush interval for remote storage (1 second)
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// Default maximum segment size (16MB)
pub const DEFAULT_MAX_SEGMENT_SIZE: usize = 16 * 1024 * 1024;

/// Default maximum frames per segment
pub const DEFAULT_MAX_FRAMES_PER_SEGMENT: usize = 4096;

/// WAL segment header (fixed size: 64 bytes)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct WalSegmentHeader {
    /// Magic number (WAL_SEGMENT_MAGIC)
    pub magic: u32,
    /// Format version
    pub version: u16,
    /// Flags (reserved)
    pub flags: u16,
    /// Database ID
    pub db_id: u64,
    /// Branch ID
    pub branch_id: u32,
    /// Padding for alignment
    pub _pad: u32,
    /// First frame number in this segment
    pub start_frame: u64,
    /// Last frame number in this segment
    pub end_frame: u64,
    /// Number of frames in this segment
    pub frame_count: u32,
    /// Page size for all frames
    pub page_size: u32,
    /// CRC64 checksum of all frame data
    pub checksum: u64,
}

impl WalSegmentHeader {
    pub const SIZE: usize = 64;

    /// Create a new segment header
    pub fn new(
        db_id: u64,
        branch_id: u32,
        start_frame: u64,
        end_frame: u64,
        frame_count: u32,
        page_size: u32,
        checksum: u64,
    ) -> Self {
        Self {
            magic: WAL_SEGMENT_MAGIC,
            version: WAL_SEGMENT_VERSION,
            flags: 0,
            db_id,
            branch_id,
            _pad: 0,
            start_frame,
            end_frame,
            frame_count,
            page_size,
            checksum,
        }
    }

    /// Serialize header to bytes
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..8].copy_from_slice(&self.flags.to_le_bytes());
        buf[8..16].copy_from_slice(&self.db_id.to_le_bytes());
        buf[16..20].copy_from_slice(&self.branch_id.to_le_bytes());
        buf[20..24].copy_from_slice(&self._pad.to_le_bytes());
        buf[24..32].copy_from_slice(&self.start_frame.to_le_bytes());
        buf[32..40].copy_from_slice(&self.end_frame.to_le_bytes());
        buf[40..44].copy_from_slice(&self.frame_count.to_le_bytes());
        buf[44..48].copy_from_slice(&self.page_size.to_le_bytes());
        buf[48..56].copy_from_slice(&self.checksum.to_le_bytes());
        // bytes 56-63 are padding/reserved
        buf
    }

    /// Deserialize header from bytes
    pub fn from_bytes(buf: &[u8]) -> io::Result<Self> {
        if buf.len() < Self::SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Buffer too small for WAL segment header",
            ));
        }

        let magic = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        if magic != WAL_SEGMENT_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid WAL segment magic: {:#x}", magic),
            ));
        }

        Ok(Self {
            magic,
            version: u16::from_le_bytes(buf[4..6].try_into().unwrap()),
            flags: u16::from_le_bytes(buf[6..8].try_into().unwrap()),
            db_id: u64::from_le_bytes(buf[8..16].try_into().unwrap()),
            branch_id: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            _pad: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
            start_frame: u64::from_le_bytes(buf[24..32].try_into().unwrap()),
            end_frame: u64::from_le_bytes(buf[32..40].try_into().unwrap()),
            frame_count: u32::from_le_bytes(buf[40..44].try_into().unwrap()),
            page_size: u32::from_le_bytes(buf[44..48].try_into().unwrap()),
            checksum: u64::from_le_bytes(buf[48..56].try_into().unwrap()),
        })
    }
}

/// Key for identifying WAL segments in storage
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WalSegmentKey {
    pub db_id: u64,
    pub branch_id: u32,
    pub start_frame: u64,
    pub end_frame: u64,
}

impl WalSegmentKey {
    /// Generate S3-style path for this segment
    pub fn to_path(&self) -> String {
        format!(
            "{:016x}/{:08x}/wal/{:016x}-{:016x}.wal",
            self.db_id, self.branch_id, self.start_frame, self.end_frame
        )
    }

    /// Parse from S3-style path
    pub fn from_path(path: &str) -> Option<Self> {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() < 4 {
            return None;
        }

        let db_id = u64::from_str_radix(parts[0], 16).ok()?;
        let branch_id = u32::from_str_radix(parts[1], 16).ok()?;

        // Parse the filename: "{start}-{end}.wal"
        let filename = parts[3].strip_suffix(".wal")?;
        let range_parts: Vec<&str> = filename.split('-').collect();
        if range_parts.len() != 2 {
            return None;
        }

        let start_frame = u64::from_str_radix(range_parts[0], 16).ok()?;
        let end_frame = u64::from_str_radix(range_parts[1], 16).ok()?;

        Some(Self {
            db_id,
            branch_id,
            start_frame,
            end_frame,
        })
    }
}

/// Result type for WAL storage operations
pub type WalStorageResult<T> = Result<T, WalStorageError>;

/// Errors from WAL storage operations
#[derive(Debug, thiserror::Error)]
pub enum WalStorageError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Segment not found: {0}")]
    NotFound(String),

    #[error("Invalid segment data: {0}")]
    InvalidData(String),

    #[error("Checksum mismatch")]
    ChecksumMismatch,

    #[error("Storage backend error: {0}")]
    Backend(String),
}

/// Trait for remote WAL storage backends (S3, GCS, etc.)
///
/// This is for disaster recovery and replication - NOT for immediate durability.
/// Immediate durability comes from local fsync via `maniac::fs`.
pub trait WalStorageBackend: Send + Sync + 'static {
    /// Write a WAL segment to remote storage
    fn write_segment(
        &self,
        key: &WalSegmentKey,
        data: Vec<u8>,
    ) -> impl Future<Output = WalStorageResult<()>> + Send;

    /// Read a WAL segment from remote storage
    fn read_segment(
        &self,
        key: &WalSegmentKey,
    ) -> impl Future<Output = WalStorageResult<Vec<u8>>> + Send;

    /// Check if a segment exists in remote storage
    fn segment_exists(
        &self,
        key: &WalSegmentKey,
    ) -> impl Future<Output = WalStorageResult<bool>> + Send;

    /// List all segments for a branch (for recovery)
    fn list_segments(
        &self,
        db_id: u64,
        branch_id: u32,
    ) -> impl Future<Output = WalStorageResult<Vec<WalSegmentKey>>> + Send;

    /// Delete a segment (for compaction)
    fn delete_segment(
        &self,
        key: &WalSegmentKey,
    ) -> impl Future<Output = WalStorageResult<()>> + Send;
}

/// Configuration for WAL writer
#[derive(Debug, Clone)]
pub struct WalWriterConfig {
    /// Local WAL directory path
    pub local_wal_path: PathBuf,
    /// How often to flush pending frames to remote storage
    pub remote_flush_interval: Duration,
    /// Maximum segment size in bytes for remote storage
    pub max_segment_size: usize,
    /// Maximum frames per segment for remote storage
    pub max_frames_per_segment: usize,
    /// Page size (must match database page size)
    pub page_size: u32,
    /// Whether to compress segments for remote storage
    pub compress: bool,
}

impl Default for WalWriterConfig {
    fn default() -> Self {
        Self {
            local_wal_path: PathBuf::from("./wal"),
            remote_flush_interval: DEFAULT_FLUSH_INTERVAL,
            max_segment_size: DEFAULT_MAX_SEGMENT_SIZE,
            max_frames_per_segment: DEFAULT_MAX_FRAMES_PER_SEGMENT,
            page_size: 4096,
            compress: true,
        }
    }
}

impl WalWriterConfig {
    /// Create config with local path
    pub fn with_local_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.local_wal_path = path.into();
        self
    }

    /// Create config with page size
    pub fn with_page_size(mut self, page_size: u32) -> Self {
        self.page_size = page_size;
        self
    }
}

/// Pending frame waiting to be flushed to remote storage
#[derive(Debug, Clone)]
pub struct PendingFrame {
    /// Frame number
    pub frame_no: u64,
    /// The frame data
    pub frame: WalFrame,
    /// When this frame was added
    pub added_at: Instant,
}

/// Statistics for a WAL writer
#[derive(Debug, Default)]
pub struct WalWriterStats {
    /// Total frames written locally
    pub local_frames_written: AtomicU64,
    /// Total frames flushed to remote
    pub remote_frames_written: AtomicU64,
    /// Total segments written to remote
    pub remote_segments_written: AtomicU64,
    /// Total bytes written to remote
    pub remote_bytes_written: AtomicU64,
    /// Last local write time (unix timestamp)
    pub last_local_write_time: AtomicU64,
    /// Last remote flush time (unix timestamp)
    pub last_remote_flush_time: AtomicU64,
}

/// WAL writer for a single branch
///
/// Handles both local persistence (immediate durability via fsync) and
/// periodic remote flushing (disaster recovery).
///
/// # Design
///
/// Frame appending is split into two phases:
/// 1. **Sync buffering** (`append_frame`): Adds frames to pending buffer (fast, no I/O)
/// 2. **Async persistence** (`flush_local`, `flush_remote`): Writes to local/remote storage
///
/// This allows SQLite's synchronous code path to remain fast while durability
/// is achieved through explicit flush calls.
pub struct WalWriter<B: WalStorageBackend> {
    /// Branch this writer is for
    branch: BranchRef,
    /// Remote storage backend
    remote_backend: Arc<B>,
    /// Configuration
    config: WalWriterConfig,
    /// Local WAL file (for appending frames)
    local_file: Option<maniac::fs::File>,
    /// Current position in local file
    local_file_pos: u64,
    /// Pending frames waiting to be written locally
    pub(crate) pending_local: VecDeque<PendingFrame>,
    /// Frames that have been written locally but not yet to remote
    pub(crate) pending_remote: VecDeque<PendingFrame>,
    /// Current accumulated size of all pending frames
    pending_size: usize,
    /// Next frame number to be written
    next_frame_no: u64,
    /// Last frame number that was durably written to local storage
    local_durable_frame_no: u64,
    /// Last frame number that was durably written to remote storage
    remote_durable_frame_no: u64,
    /// Last remote flush time
    last_remote_flush: Instant,
    /// Statistics
    stats: Arc<WalWriterStats>,
}

impl<B: WalStorageBackend> WalWriter<B> {
    /// Create a new WAL writer
    pub fn new(branch: BranchRef, remote_backend: Arc<B>, config: WalWriterConfig) -> Self {
        Self {
            branch,
            remote_backend,
            config,
            local_file: None,
            local_file_pos: 0,
            pending_local: VecDeque::new(),
            pending_remote: VecDeque::new(),
            pending_size: 0,
            next_frame_no: 1,
            local_durable_frame_no: 0,
            remote_durable_frame_no: 0,
            last_remote_flush: Instant::now(),
            stats: Arc::new(WalWriterStats::default()),
        }
    }

    /// Create with a starting frame number (for recovery)
    pub fn with_start_frame(mut self, start_frame: u64) -> Self {
        self.next_frame_no = start_frame;
        self.local_durable_frame_no = start_frame.saturating_sub(1);
        self.remote_durable_frame_no = start_frame.saturating_sub(1);
        self
    }

    /// Get the branch reference
    pub fn branch(&self) -> BranchRef {
        self.branch
    }

    /// Get the next frame number that will be assigned
    pub fn next_frame_no(&self) -> u64 {
        self.next_frame_no
    }

    /// Get the last locally durable frame number
    pub fn local_durable_frame_no(&self) -> u64 {
        self.local_durable_frame_no
    }

    /// Get the last remotely durable frame number
    pub fn remote_durable_frame_no(&self) -> u64 {
        self.remote_durable_frame_no
    }

    /// Alias for remote_durable_frame_no for compatibility
    pub fn durable_frame_no(&self) -> u64 {
        self.remote_durable_frame_no
    }

    /// Get the number of pending frames (waiting for local flush)
    pub fn pending_local_count(&self) -> usize {
        self.pending_local.len()
    }

    /// Get the number of pending frames (waiting for remote flush)
    pub fn pending_remote_count(&self) -> usize {
        self.pending_remote.len()
    }

    /// Get total pending count (local + remote)
    pub fn pending_count(&self) -> usize {
        self.pending_local.len() + self.pending_remote.len()
    }

    /// Get statistics
    pub fn stats(&self) -> &Arc<WalWriterStats> {
        &self.stats
    }

    /// Get the local WAL file path for this branch
    fn local_wal_path(&self) -> PathBuf {
        self.config
            .local_wal_path
            .join(format!("{:016x}", self.branch.db_id))
            .join(format!("{:08x}", self.branch.branch_id))
            .join("wal.log")
    }

    /// Ensure the local WAL file is open
    async fn ensure_local_file(&mut self) -> WalStorageResult<()> {
        if self.local_file.is_some() {
            return Ok(());
        }

        let path = self.local_wal_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open file for append (create if doesn't exist)
        let file = maniac::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        // Get current file size for position tracking
        let metadata = maniac::fs::metadata(&path).await?;
        self.local_file_pos = metadata.len();

        self.local_file = Some(file);
        Ok(())
    }

    /// Append a frame to the pending buffer (sync, no I/O).
    ///
    /// This is a fast operation that just buffers the frame. Call `flush_local()`
    /// to persist to local storage with fsync, and `flush_remote()` to replicate
    /// to remote storage.
    ///
    /// Returns the assigned frame number.
    pub fn append_frame(&mut self, frame: WalFrame) -> u64 {
        let frame_no = self.next_frame_no;
        self.next_frame_no += 1;

        let frame_size = 8 + frame.data.len(); // 8 bytes header + page data
        self.pending_size += frame_size;

        self.pending_local.push_back(PendingFrame {
            frame_no,
            frame,
            added_at: Instant::now(),
        });

        frame_no
    }

    /// Flush pending frames to local storage with fsync.
    ///
    /// This writes all pending local frames to the local WAL file and calls
    /// fsync for durability. After this returns, frames are durable on local disk.
    ///
    /// Frames are then moved to the remote pending queue.
    pub async fn flush_local(&mut self) -> WalStorageResult<u64> {
        if self.pending_local.is_empty() {
            return Ok(self.local_durable_frame_no);
        }

        // Ensure local file is open
        self.ensure_local_file().await?;

        // Collect all frame data
        let mut all_data = Vec::new();
        let frames: Vec<PendingFrame> = self.pending_local.drain(..).collect();

        for pf in &frames {
            let frame_data = self.serialize_frame(&pf.frame);
            all_data.extend_from_slice(&frame_data);
        }

        let data_len = all_data.len() as u64;
        let frame_count = frames.len();

        // Write all frames in one batch
        let file = self.local_file.as_ref().unwrap();
        let (result, _) = file.write_all_at(all_data, self.local_file_pos).await;
        result?;

        // Fsync for durability
        file.sync_data().await?;

        self.local_file_pos += data_len;

        // Get the last frame number
        let last_frame_no = frames.last().map(|f| f.frame_no).unwrap_or(0);
        self.local_durable_frame_no = last_frame_no;

        // Update stats
        self.stats
            .local_frames_written
            .fetch_add(frame_count as u64, Ordering::Relaxed);
        self.stats.last_local_write_time.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            Ordering::Relaxed,
        );

        // Move to remote pending queue
        for pf in frames {
            self.pending_remote.push_back(pf);
        }

        Ok(self.local_durable_frame_no)
    }

    /// Serialize a frame for storage
    fn serialize_frame(&self, frame: &WalFrame) -> Vec<u8> {
        let mut data = Vec::with_capacity(8 + frame.data.len());
        data.extend_from_slice(&frame.page_no.to_le_bytes());
        data.extend_from_slice(&frame.db_size.to_le_bytes());
        data.extend_from_slice(&frame.data);
        data
    }

    /// Check if a local flush is needed (any pending local frames)
    pub fn should_flush_local(&self) -> bool {
        !self.pending_local.is_empty()
    }

    /// Check if a remote flush is needed based on time or size
    pub fn should_flush_remote(&self) -> bool {
        if self.pending_remote.is_empty() {
            return false;
        }

        // Time-based flush
        if self.last_remote_flush.elapsed() >= self.config.remote_flush_interval {
            return true;
        }

        // Size-based flush
        if self.pending_size >= self.config.max_segment_size {
            return true;
        }

        // Frame count-based flush
        if self.pending_remote.len() >= self.config.max_frames_per_segment {
            return true;
        }

        false
    }

    /// Check if any flush is needed (either local or remote)
    pub fn should_flush(&self) -> bool {
        self.should_flush_local() || self.should_flush_remote()
    }

    /// Flush pending frames to remote storage
    ///
    /// Returns the segment key if frames were written, None if no frames pending.
    /// Note: Call `flush_local()` first to ensure frames are locally durable.
    pub async fn flush_remote(&mut self) -> WalStorageResult<Option<WalSegmentKey>> {
        if self.pending_remote.is_empty() {
            return Ok(None);
        }

        // Take all pending remote frames
        let frames: Vec<PendingFrame> = self.pending_remote.drain(..).collect();
        // Update pending size (remote frames are now gone)
        self.pending_size = self.pending_local.iter().map(|f| 8 + f.frame.data.len()).sum();

        let start_frame = frames.first().unwrap().frame_no;
        let end_frame = frames.last().unwrap().frame_no;
        let frame_count = frames.len() as u32;

        // Build segment data
        let segment_data = self.build_segment(&frames)?;

        // Create segment key
        let key = WalSegmentKey {
            db_id: self.branch.db_id,
            branch_id: self.branch.branch_id,
            start_frame,
            end_frame,
        };

        // Write to remote backend
        let data_len = segment_data.len();
        self.remote_backend.write_segment(&key, segment_data).await?;

        // Update state
        self.remote_durable_frame_no = end_frame;
        self.last_remote_flush = Instant::now();

        // Update stats
        self.stats
            .remote_frames_written
            .fetch_add(frame_count as u64, Ordering::Relaxed);
        self.stats
            .remote_segments_written
            .fetch_add(1, Ordering::Relaxed);
        self.stats
            .remote_bytes_written
            .fetch_add(data_len as u64, Ordering::Relaxed);
        self.stats.last_remote_flush_time.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            Ordering::Relaxed,
        );

        Ok(Some(key))
    }

    /// Build a segment from pending frames
    pub(crate) fn build_segment(&self, frames: &[PendingFrame]) -> WalStorageResult<Vec<u8>> {
        let start_frame = frames.first().unwrap().frame_no;
        let end_frame = frames.last().unwrap().frame_no;
        let frame_count = frames.len() as u32;

        // Calculate total size
        let frame_header_size = 8; // page_no (4) + db_size (4)
        let total_frame_data_size: usize = frames
            .iter()
            .map(|f| frame_header_size + f.frame.data.len())
            .sum();

        let mut data = Vec::with_capacity(WalSegmentHeader::SIZE + total_frame_data_size);

        // Reserve space for header (we'll fill it after calculating checksum)
        data.resize(WalSegmentHeader::SIZE, 0);

        // Write frames and calculate checksum
        let crc = crc::Crc::<u64>::new(&crc::CRC_64_XZ);
        let mut digest = crc.digest();

        for pf in frames {
            // Write frame header
            data.extend_from_slice(&pf.frame.page_no.to_le_bytes());
            data.extend_from_slice(&pf.frame.db_size.to_le_bytes());

            // Write frame data
            data.extend_from_slice(&pf.frame.data);

            // Update checksum
            digest.update(&pf.frame.page_no.to_le_bytes());
            digest.update(&pf.frame.db_size.to_le_bytes());
            digest.update(&pf.frame.data);
        }

        let checksum = digest.finalize();

        // Create and write header
        let header = WalSegmentHeader::new(
            self.branch.db_id,
            self.branch.branch_id,
            start_frame,
            end_frame,
            frame_count,
            self.config.page_size,
            checksum,
        );

        data[..WalSegmentHeader::SIZE].copy_from_slice(&header.to_bytes());

        // Optionally compress
        if self.config.compress {
            let compressed = lz4_flex::compress_prepend_size(&data);
            // Only use compressed if it's smaller
            if compressed.len() < data.len() {
                return Ok(compressed);
            }
        }

        Ok(data)
    }
}

/// WAL segment reader for recovery
pub struct WalSegmentReader {
    header: WalSegmentHeader,
    data: Vec<u8>,
    offset: usize,
}

impl WalSegmentReader {
    /// Parse a WAL segment from bytes
    pub fn new(mut data: Vec<u8>) -> WalStorageResult<Self> {
        // Try to decompress if it looks like LZ4
        if data.len() >= 4 {
            // LZ4 prepended size format starts with the uncompressed size
            if let Ok(decompressed) = lz4_flex::decompress_size_prepended(&data) {
                data = decompressed;
            }
        }

        if data.len() < WalSegmentHeader::SIZE {
            return Err(WalStorageError::InvalidData(
                "Segment too small".to_string(),
            ));
        }

        let header = WalSegmentHeader::from_bytes(&data)?;

        Ok(Self {
            header,
            data,
            offset: WalSegmentHeader::SIZE,
        })
    }

    /// Get the segment header
    pub fn header(&self) -> &WalSegmentHeader {
        &self.header
    }

    /// Iterate over frames in the segment
    pub fn frames(&mut self) -> WalFrameIterator<'_> {
        self.offset = WalSegmentHeader::SIZE;
        let start_frame = self.header.start_frame;
        WalFrameIterator {
            reader: self,
            frame_no: start_frame,
        }
    }

    /// Verify the segment checksum
    pub fn verify(&self) -> bool {
        let crc = crc::Crc::<u64>::new(&crc::CRC_64_XZ);
        let mut digest = crc.digest();

        let mut offset = WalSegmentHeader::SIZE;
        let frame_size = 8 + self.header.page_size as usize;

        for _ in 0..self.header.frame_count {
            if offset + frame_size > self.data.len() {
                return false;
            }
            digest.update(&self.data[offset..offset + frame_size]);
            offset += frame_size;
        }

        digest.finalize() == self.header.checksum
    }
}

/// Iterator over frames in a WAL segment
pub struct WalFrameIterator<'a> {
    reader: &'a mut WalSegmentReader,
    frame_no: u64,
}

impl<'a> Iterator for WalFrameIterator<'a> {
    type Item = (u64, WalFrame);

    fn next(&mut self) -> Option<Self::Item> {
        let page_size = self.reader.header.page_size as usize;
        let frame_size = 8 + page_size;

        if self.reader.offset + frame_size > self.reader.data.len() {
            return None;
        }

        let offset = self.reader.offset;

        let page_no =
            u32::from_le_bytes(self.reader.data[offset..offset + 4].try_into().unwrap());
        let db_size =
            u32::from_le_bytes(self.reader.data[offset + 4..offset + 8].try_into().unwrap());
        let data = self.reader.data[offset + 8..offset + 8 + page_size].to_vec();

        self.reader.offset += frame_size;
        let frame_no = self.frame_no;
        self.frame_no += 1;

        Some((
            frame_no,
            WalFrame {
                page_no,
                data,
                db_size,
            },
        ))
    }
}

/// No-op WAL storage backend for testing
#[derive(Clone, Debug, Default)]
pub struct NoopWalStorage;

impl WalStorageBackend for NoopWalStorage {
    async fn write_segment(&self, _key: &WalSegmentKey, _data: Vec<u8>) -> WalStorageResult<()> {
        Ok(())
    }

    async fn read_segment(&self, key: &WalSegmentKey) -> WalStorageResult<Vec<u8>> {
        Err(WalStorageError::NotFound(key.to_path()))
    }

    async fn segment_exists(&self, _key: &WalSegmentKey) -> WalStorageResult<bool> {
        Ok(false)
    }

    async fn list_segments(
        &self,
        _db_id: u64,
        _branch_id: u32,
    ) -> WalStorageResult<Vec<WalSegmentKey>> {
        Ok(Vec::new())
    }

    async fn delete_segment(&self, _key: &WalSegmentKey) -> WalStorageResult<()> {
        Ok(())
    }
}

/// Local filesystem remote storage backend using maniac::fs
///
/// This uses async filesystem operations for remote storage simulation.
/// In production, you would use S3 or similar.
pub struct LocalWalStorage {
    root: PathBuf,
}

impl LocalWalStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn segment_path(&self, key: &WalSegmentKey) -> PathBuf {
        self.root.join(key.to_path())
    }
}

impl WalStorageBackend for LocalWalStorage {
    async fn write_segment(&self, key: &WalSegmentKey, data: Vec<u8>) -> WalStorageResult<()> {
        let path = self.segment_path(key);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write using maniac::fs
        let (result, _) = maniac::fs::write(&path, data).await;
        result?;

        Ok(())
    }

    async fn read_segment(&self, key: &WalSegmentKey) -> WalStorageResult<Vec<u8>> {
        let path = self.segment_path(key);

        maniac::fs::read(&path).await.map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                WalStorageError::NotFound(key.to_path())
            } else {
                WalStorageError::Io(e)
            }
        })
    }

    async fn segment_exists(&self, key: &WalSegmentKey) -> WalStorageResult<bool> {
        let path = self.segment_path(key);

        match maniac::fs::metadata(&path).await {
            Ok(meta) => Ok(meta.is_file()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(WalStorageError::Io(e)),
        }
    }

    async fn list_segments(
        &self,
        db_id: u64,
        branch_id: u32,
    ) -> WalStorageResult<Vec<WalSegmentKey>> {
        let wal_dir = self
            .root
            .join(format!("{:016x}", db_id))
            .join(format!("{:08x}", branch_id))
            .join("wal");

        // Directory listing is sync (no async readdir in maniac::fs yet)
        if !wal_dir.exists() {
            return Ok(Vec::new());
        }

        let mut segments = Vec::new();
        for entry in std::fs::read_dir(&wal_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".wal") {
                    let full_path = format!("{:016x}/{:08x}/wal/{}", db_id, branch_id, name);
                    if let Some(key) = WalSegmentKey::from_path(&full_path) {
                        segments.push(key);
                    }
                }
            }
        }

        // Sort by start frame
        segments.sort_by_key(|k| k.start_frame);
        Ok(segments)
    }

    async fn delete_segment(&self, key: &WalSegmentKey) -> WalStorageResult<()> {
        let path = self.segment_path(key);

        // Use std::fs for now (no async unlink in maniac::fs without feature)
        std::fs::remove_file(&path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                WalStorageError::NotFound(key.to_path())
            } else {
                WalStorageError::Io(e)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_segment_header_roundtrip() {
        let header = WalSegmentHeader::new(123, 456, 1, 100, 100, 4096, 0xDEADBEEF);

        let bytes = header.to_bytes();
        let parsed = WalSegmentHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.magic, WAL_SEGMENT_MAGIC);
        assert_eq!(parsed.db_id, 123);
        assert_eq!(parsed.branch_id, 456);
        assert_eq!(parsed.start_frame, 1);
        assert_eq!(parsed.end_frame, 100);
        assert_eq!(parsed.frame_count, 100);
        assert_eq!(parsed.page_size, 4096);
        assert_eq!(parsed.checksum, 0xDEADBEEF);
    }

    #[test]
    fn test_wal_segment_key_path() {
        let key = WalSegmentKey {
            db_id: 1,
            branch_id: 2,
            start_frame: 100,
            end_frame: 200,
        };

        let path = key.to_path();
        assert!(path.contains("/wal/"));
        assert!(path.ends_with(".wal"));

        let parsed = WalSegmentKey::from_path(&path).unwrap();
        assert_eq!(parsed, key);
    }

    #[test]
    fn test_wal_writer_basic() {
        let backend = Arc::new(NoopWalStorage);
        let branch = BranchRef::new(1, 1);
        let config = WalWriterConfig {
            page_size: 4096,
            ..Default::default()
        };

        let mut writer = WalWriter::new(branch, backend, config);

        // Create and append frames using the sync append_frame method
        let frame1 = WalFrame {
            page_no: 1,
            data: vec![0xAA; 4096],
            db_size: 0,
        };
        let frame2 = WalFrame {
            page_no: 2,
            data: vec![0xBB; 4096],
            db_size: 2,
        };

        // Use the sync append_frame method
        let frame_no1 = writer.append_frame(frame1);
        let frame_no2 = writer.append_frame(frame2);

        assert_eq!(frame_no1, 1);
        assert_eq!(frame_no2, 2);
        assert_eq!(writer.pending_local_count(), 2);
        assert_eq!(writer.pending_count(), 2);

        // Test building segment from pending_local frames
        let pending: Vec<_> = writer.pending_local.drain(..).collect();
        let segment_data = writer.build_segment(&pending).unwrap();
        assert!(!segment_data.is_empty());
    }

    #[test]
    fn test_wal_segment_reader() {
        let branch = BranchRef::new(1, 1);
        let backend = Arc::new(NoopWalStorage);
        let config = WalWriterConfig {
            page_size: 64, // Small page size for testing
            compress: false,
            ..Default::default()
        };

        let mut writer = WalWriter::new(branch, backend, config);

        // Add some frames using the sync append_frame method
        for i in 1..=3 {
            writer.append_frame(WalFrame {
                page_no: i as u32,
                data: vec![i as u8; 64],
                db_size: i as u32,
            });
        }

        // Build segment from pending_local
        let frames: Vec<_> = writer.pending_local.drain(..).collect();
        let segment_data = writer.build_segment(&frames).unwrap();

        // Read it back
        let mut reader = WalSegmentReader::new(segment_data).unwrap();

        assert_eq!(reader.header().frame_count, 3);
        assert_eq!(reader.header().start_frame, 1);
        assert_eq!(reader.header().end_frame, 3);
        assert!(reader.verify());

        let frames: Vec<_> = reader.frames().collect();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].0, 1);
        assert_eq!(frames[0].1.page_no, 1);
        assert_eq!(frames[2].1.db_size, 3);
    }
}
