//! Unified Log Storage for Multi-Raft and LibSQL
//!
//! Provides per-group sharded, segmented storage with:
//! - One dedicated shard per raft group (1:1 mapping)
//! - Dynamic shard creation on first access
//! - Segmented log files (default 64MB per segment)
//! - No mutex contention - writes serialized through channels per shard
//! - CRC64-NVME checksums for crash recovery
//!
//! This design aligns with libsql where each database/branch is a raft group
//! with independent lifecycle (checkpointing, compaction, replication).
//!
//! ## Log Entry Format
//!
//! Each log record has the following format:
//! ```text
//! +----------+----------+----------+-------------+----------+
//! | len (4B) | type (1B)| group_id | payload     | crc64    |
//! |  u32 LE  |   u8     |  u64 LE  | (variable)  | (8B) LE  |
//! +----------+----------+----------+-------------+----------+
//! ```
//!
//! ## Record Types
//!
//! - **Raft records**: Vote, Entry, Truncate, Purge
//! - **LibSQL records**: WalFrame, Changeset, Checkpoint
//!
//! ## Segment File Naming
//!
//! Segments are named: `group-{group_id}-{segment_id}.log`
//! Each group has its own directory: `{base_dir}/{group_id}/`
//!
//! ## Recovery Protocol
//!
//! On startup/first access to a group:
//! 1. Lists all segment files for the group
//! 2. Sorts by segment_id to process in order
//! 3. Reads each segment, validating CRC64 for each record
//! 4. Stops at first invalid record (partial write from crash)
//! 5. Truncates the file at that point
//! Rebuilds in-memory state from valid records

use crate::multi::codec::{Decode, FromCodec};
use crate::multi::codec::{
    Encode, Entry as CodecEntry, LogId as CodecLogId, RawBytes, ToCodec, Vote as CodecVote,
};
use congee::U64Congee;
use congee::epoch;
use crc64fast_nvme::Digest;
use crossfire::MAsyncTx;
use dashmap::DashMap;
use maniac::buf::{AlignedBuf, IoBufMut};
use maniac::fs::{File, OpenOptions};
// Using flume instead of maniac mpsc for reliability
// use maniac::sync::mpsc::bounded as mpsc_bounded;
use openraft::{
    LogId, LogState, RaftTypeConfig,
    storage::{IOFlushed, RaftLogReader, RaftLogStorage},
};
use std::io;
use std::ops::RangeBounds;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use super::manifest_mdbx::{ManifestManager, SegmentMeta};
use parking_lot::RwLock;
use tokio::sync::oneshot;

#[cfg(unix)]
use std::os::fd::AsRawFd;

/// Log record types - supports both Raft and LibSQL records
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    // Raft records
    Vote = 0x01,
    Entry = 0x02,
    Truncate = 0x03,
    Purge = 0x04,
    // LibSQL records
    WalFrame = 0x10,
    Changeset = 0x11,
    Checkpoint = 0x12,
    WalMetadata = 0x13,
}

impl TryFrom<u8> for RecordType {
    type Error = io::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(RecordType::Vote),
            0x02 => Ok(RecordType::Entry),
            0x03 => Ok(RecordType::Truncate),
            0x04 => Ok(RecordType::Purge),
            0x10 => Ok(RecordType::WalFrame),
            0x11 => Ok(RecordType::Changeset),
            0x12 => Ok(RecordType::Checkpoint),
            0x13 => Ok(RecordType::WalMetadata),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid record type: {:#x}", value),
            )),
        }
    }
}

/// Length field size: 4 bytes
const LENGTH_SIZE: usize = 4;
/// CRC64 size: 8 bytes
const CRC64_SIZE: usize = 8;
/// Group ID size: 3 bytes (u24 little-endian)
const GROUP_ID_SIZE: usize = 3;
/// Minimum record size: len(4) + type(1) + group_id(3) + crc(8) = 16 bytes
#[allow(dead_code)]
const MIN_RECORD_SIZE: usize = LENGTH_SIZE + 1 + GROUP_ID_SIZE + CRC64_SIZE;
/// Default segment size: 1MB
pub const DEFAULT_SEGMENT_SIZE: u64 = 1 * 1024 * 1024;
/// Default streaming chunk size: 1MB (should be aligned and large enough for efficiency)
pub const DEFAULT_CHUNK_SIZE: usize = 1 * 1024 * 1024;

/// Header size: len(4) + type(1) + group_id(3) = 8 bytes (64-bit aligned)
const HEADER_SIZE: usize = LENGTH_SIZE + 1 + GROUP_ID_SIZE;

/// Default maximum record size: 1MB
pub const DEFAULT_MAX_RECORD_SIZE: u64 = 1 * 1024 * 1024;

/// Write a u24 little-endian value to the buffer at the given offset.
#[inline]
fn write_u24_le(buf: &mut [u8], offset: usize, value: u64) {
    debug_assert!(value <= 0xFF_FFFF, "group_id exceeds u24 range");
    buf[offset] = value as u8;
    buf[offset + 1] = (value >> 8) as u8;
    buf[offset + 2] = (value >> 16) as u8;
}

/// Read a u24 little-endian value from the buffer at the given offset.
#[inline]
fn read_u24_le(buf: &[u8], offset: usize) -> u64 {
    let lo = buf[offset] as u64;
    let mid = (buf[offset + 1] as u64) << 8;
    let high = (buf[offset + 2] as u64) << 16;
    lo | mid | high
}

async fn read_exact_at_reuse(
    file: &File,
    offset: u64,
    len: usize,
    buf: &mut Vec<u8>,
    dio_scratch: &mut AlignedBuf,
) -> io::Result<()> {
    if len == 0 {
        buf.clear();
        return Ok(());
    }

    if buf.capacity() < len {
        buf.reserve(len - buf.capacity());
    }

    // Primary fast path: let the runtime fill the provided Vec (no allocation if capacity is enough).
    let mut owned = std::mem::take(buf);
    let (res, slice) = file.read_exact_at(owned.slice_mut(0..len), offset).await;
    owned = slice.into_inner();

    match res {
        Ok(()) => {
            *buf = owned;
            Ok(())
        }
        Err(e) => {
            // For direct-I/O handles (and some io_uring constraints), reads can return EINVAL for
            // unaligned buffers/offsets/lengths. Try the non-allocating unaligned helper first.
            if e.kind() == io::ErrorKind::InvalidInput {
                owned.resize(len, 0);
                if file
                    .read_exact_at_unaligned_into(offset, &mut owned[..len], dio_scratch)
                    .await
                    .is_ok()
                {
                    *buf = owned;
                    return Ok(());
                }

                // As a last resort on unix, fall back to blocking pread on a thread pool (still async).
                #[cfg(unix)]
                {
                    let fd = file.as_raw_fd();
                    let mut read = 0usize;
                    while read < len {
                        let ptr = unsafe { owned.as_mut_ptr().add(read) };
                        let n = unsafe {
                            maniac::blocking::unblock_fread_at(
                                fd,
                                ptr,
                                len - read,
                                offset + read as u64,
                            )
                        }
                        .await?;
                        if n == 0 {
                            *buf = owned;
                            return Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "failed to fill whole buffer",
                            ));
                        }
                        read += n;
                    }
                    *buf = owned;
                    return Ok(());
                }
            }

            *buf = owned;
            Err(e)
        }
    }
}

async fn open_segment_file(
    path: &Path,
    read: bool,
    write: bool,
    create: bool,
    truncate: bool,
) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(read)
        .write(write)
        .create(create)
        .truncate(truncate);

    // Only enable direct I/O for writable handles
    if write {
        opts.direct_io(true);
    }

    match opts.open(path).await {
        Ok(f) => Ok(f),
        Err(e) => {
            // Check for InvalidInput (EINVAL) which indicates Direct I/O is not supported
            // Error code 22 is EINVAL on Unix systems, 87 on Windows
            if write
                && (e.kind() == io::ErrorKind::InvalidInput
                    || e.raw_os_error() == Some(22)
                    || e.raw_os_error() == Some(87))
            {
                tracing::debug!(
                    "Direct I/O not supported for {:?}, falling back to buffered I/O: {}",
                    path,
                    e
                );
                let mut opts = OpenOptions::new();
                opts.read(read)
                    .write(write)
                    .create(create)
                    .truncate(truncate);
                // Don't set direct_io this time
                opts.open(path).await
            } else {
                Err(e)
            }
        }
    }
}

// Avoid allocating for zero padding during segment tail alignment.
static ZERO_4K: [u8; 4096] = [0u8; 4096];

#[inline]
fn dio_align() -> u64 {
    #[cfg(any(target_os = "linux", windows))]
    {
        4096
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        1
    }
}

#[inline]
fn align_up_u64(v: u64, align: u64) -> u64 {
    if align <= 1 {
        return v;
    }
    ((v + align - 1) / align) * align
}

/// Encodes a log record with CRC64 checksum into the provided buffer.
/// Format: [len: u32][type: u8][group_id: u64][payload...][crc64: u64]
///
/// The buffer must have at least `HEADER_SIZE + payload.len() + CRC64_SIZE` capacity.
/// Returns the total number of bytes written.
fn encode_record_into(
    buf: &mut Vec<u8>,
    record_type: RecordType,
    group_id: u64,
    payload: &[u8],
) -> usize {
    buf.clear();
    append_record_into(buf, record_type, group_id, payload)
}

/// Appends a log record to `buf` (does NOT clear first).
/// Returns the number of bytes appended.
fn append_record_into(
    buf: &mut Vec<u8>,
    record_type: RecordType,
    group_id: u64,
    payload: &[u8],
) -> usize {
    // Total record size (excluding the len field itself)
    let record_len = 1 + GROUP_ID_SIZE + payload.len() + CRC64_SIZE; // type + group_id + payload + crc
    let total_size = LENGTH_SIZE + record_len;

    buf.reserve(total_size);

    // Build header: [len][type][group_id]
    let mut header: [u8; HEADER_SIZE] = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&(record_len as u32).to_le_bytes());
    header[4] = record_type as u8;
    write_u24_le(&mut header, 5, group_id);

    // Compute CRC64 incrementally over [type + group_id + payload]
    let mut digest = Digest::new();
    digest.write(&header[LENGTH_SIZE..]); // type + group_id (4 bytes)
    digest.write(payload);
    let crc = digest.sum64();

    // Append everything to buffer
    buf.extend_from_slice(&header);
    buf.extend_from_slice(payload);
    buf.extend_from_slice(&crc.to_le_bytes());

    total_size
}

/// Encodes a log record with CRC64 checksum (allocating version for tests/convenience)
/// Format: [len: u32][type: u8][group_id: u64][payload...][crc64: u64]
#[cfg(test)]
fn encode_record(record_type: RecordType, group_id: u64, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_record_into(&mut buf, record_type, group_id, payload);
    buf
}

/// A parsed record from the log
#[derive(Debug)]
pub struct ParsedRecord<'a> {
    pub record_type: RecordType,
    pub group_id: u64,
    pub payload: &'a [u8],
}

/// Validates a record's CRC64 checksum
/// Input is the data AFTER the length field (type + group_id + payload + crc)
/// Returns (record_type, group_id, payload) if valid
fn validate_record(data: &[u8], max_record_size: usize) -> io::Result<ParsedRecord<'_>> {
    // Minimum: type(1) + group_id(3) + crc(8) = 12 bytes
    if data.len() < 1 + GROUP_ID_SIZE + CRC64_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "record too short",
        ));
    }

    if data.len() > max_record_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "record exceeds max_record_size: {} > {}",
                data.len(),
                max_record_size
            ),
        ));
    }

    let payload_end = data.len() - CRC64_SIZE;
    let checksummed_data = &data[..payload_end];
    let stored_crc = u64::from_le_bytes(data[payload_end..].try_into().unwrap());

    // Verify CRC
    let mut digest = Digest::new();
    digest.write(checksummed_data);
    let computed_crc = digest.sum64();

    if computed_crc != stored_crc {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "CRC mismatch: stored {:#x}, computed {:#x}",
                stored_crc, computed_crc
            ),
        ));
    }

    // Parse header
    let record_type = RecordType::try_from(data[0])?;
    let group_id = read_u24_le(data, 1);
    let payload = &data[1 + GROUP_ID_SIZE..payload_end];

    Ok(ParsedRecord {
        record_type,
        group_id,
        payload,
    })
}

/// Configuration for per-group log storage.
/// Uses Arc<PathBuf> for cheap cloning of config across groups.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Base directory for all raft data
    pub base_dir: Arc<PathBuf>,
    /// Optional manifest directory. If None, manifest is stored in base_dir/.raft_manifest
    /// Use this if base_dir is on a filesystem that doesn't support Direct I/O (e.g., tmpfs)
    pub manifest_dir: Option<Arc<PathBuf>>,
    /// Maximum segment size in bytes. When exceeded, a new segment is created.
    /// Default: 1MB, minimum: MIN_RECORD_SIZE as u64
    pub segment_size: u64,
    /// Maximum size for individual records. None means no limit (but constrained by segment_size).
    /// Default: 1MB, validated to be <= segment_size
    pub max_record_size: Option<u64>,
    /// Interval for fsync per group. If None, fsync after every write.
    /// If Some(duration), fsync on interval only if dirty.
    pub fsync_interval: Option<Duration>,
    /// Maximum entries to keep in memory cache per group
    pub max_cache_entries_per_group: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_dir: Arc::new(PathBuf::from("./raft-data")),
            manifest_dir: None,
            segment_size: DEFAULT_SEGMENT_SIZE,
            max_record_size: Some(DEFAULT_MAX_RECORD_SIZE),
            fsync_interval: Some(Duration::from_millis(100)),
            max_cache_entries_per_group: 10000,
        }
    }
}

impl StorageConfig {
    /// Create a new StorageConfig with the given base directory
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: Arc::new(base_dir.into()),
            ..Default::default()
        }
    }

    /// Set the segment size
    /// Validates that segment_size >= MIN_RECORD_SIZE
    pub fn with_segment_size(mut self, size: u64) -> Self {
        assert!(
            size >= MIN_RECORD_SIZE as u64,
            "segment_size must be at least MIN_RECORD_SIZE ({})",
            MIN_RECORD_SIZE
        );

        // Ensure segment_size is aligned for Direct I/O (must be multiple of 512 or 4096)
        let aligned_size = align_up_u64(size, dio_align());
        if aligned_size != size {
            tracing::warn!(
                "segment_size {} is not aligned to {} bytes for Direct I/O, rounding up to {}",
                size,
                dio_align(),
                aligned_size
            );
        }
        self.segment_size = aligned_size;

        // Validate max_record_size if set
        if let Some(max_record) = self.max_record_size {
            assert!(
                max_record <= aligned_size,
                "max_record_size ({}) must be <= segment_size ({})",
                max_record,
                aligned_size
            );
        }
        self
    }

    /// Set the maximum record size
    /// None means no explicit limit (but still constrained by segment_size)
    /// Validates that max_record_size <= segment_size
    pub fn with_max_record_size(mut self, max_size: impl Into<Option<u64>>) -> Self {
        let max_size_opt = max_size.into();
        if let Some(max_size) = max_size_opt {
            assert!(
                max_size <= self.segment_size,
                "max_record_size ({}) must be <= segment_size ({})",
                max_size,
                self.segment_size
            );
            assert!(
                max_size >= MIN_RECORD_SIZE as u64,
                "max_record_size must be at least MIN_RECORD_SIZE ({})",
                MIN_RECORD_SIZE
            );
        }
        self.max_record_size = max_size_opt;
        self
    }

    /// Set the fsync interval
    pub fn with_fsync_interval(mut self, interval: Option<Duration>) -> Self {
        self.fsync_interval = interval;
        self
    }

    /// Set the maximum cache entries per group
    pub fn with_max_cache_entries(mut self, max_entries: usize) -> Self {
        self.max_cache_entries_per_group = max_entries;
        self
    }

    /// Set the manifest directory. Use this if the base_dir is on a filesystem
    /// that doesn't support Direct I/O (e.g., tmpfs). If not set, manifest is
    /// stored in base_dir/.raft_manifest
    pub fn with_manifest_dir(mut self, manifest_dir: impl AsRef<Path>) -> Self {
        self.manifest_dir = Some(Arc::new(manifest_dir.as_ref().to_path_buf()));
        self
    }
}

/// Legacy config alias - maps to new StorageConfig
/// The `num_shards` field is ignored (each group gets its own shard)
#[derive(Debug, Clone)]
pub struct ShardedStorageConfig {
    pub base_dir: PathBuf,
    /// Ignored - each group gets its own shard now
    pub num_shards: usize,
    pub segment_size: u64,
    pub max_record_size: Option<u64>,
    pub fsync_interval: Option<Duration>,
    pub max_cache_entries_per_group: usize,
}

impl Default for ShardedStorageConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("./raft-data"),
            num_shards: 8, // Default value, not used
            segment_size: DEFAULT_SEGMENT_SIZE,
            max_record_size: Some(DEFAULT_MAX_RECORD_SIZE),
            fsync_interval: Some(Duration::from_millis(100)),
            max_cache_entries_per_group: 10000,
        }
    }
}

impl From<ShardedStorageConfig> for StorageConfig {
    fn from(sharded: ShardedStorageConfig) -> Self {
        Self {
            base_dir: Arc::new(sharded.base_dir),
            manifest_dir: None,
            segment_size: sharded.segment_size,
            max_record_size: sharded.max_record_size,
            fsync_interval: sharded.fsync_interval,
            max_cache_entries_per_group: sharded.max_cache_entries_per_group,
        }
    }
}

// Keep the old config as an alias for compatibility
pub type MultiplexedStorageConfig = ShardedStorageConfig;

/// Per-group state within the storage
struct GroupState<C: RaftTypeConfig> {
    /// In-memory log cache: index -> entry
    cache: DashMap<u64, Arc<C::Entry>>,
    /// On-disk log index: entry index -> segment location.
    /// Implemented with congee (u64 -> slot) + side vector for locations.
    log_index: LogIndex,
    /// Current vote for this group (atomic)
    vote: AtomicVote,
    /// Log state
    first_index: AtomicU64,
    last_index: AtomicU64,
    /// Lower bound (inclusive) of the cache window.
    /// Entries with index < cache_low may be evicted from `cache`.
    cache_low: AtomicU64,
    last_log_id: AtomicLogId,
    /// Last purged log id (for log compaction tracking)
    last_purged_log_id: AtomicLogId,
    /// The shard for this group
    shard: Arc<ShardState<C>>,
}

#[derive(Debug, Clone, Copy)]
struct LogLocation {
    segment_id: u64,
    offset: u64,
    len: u32,
}

struct LogIndex {
    map: U64Congee<usize>,
    locations: RwLock<Vec<LogLocation>>,
    free: RwLock<Vec<usize>>,
}

impl LogIndex {
    fn new() -> Self {
        Self {
            map: U64Congee::<usize>::new(),
            locations: RwLock::new(Vec::new()),
            free: RwLock::new(Vec::new()),
        }
    }

    fn get(&self, key: u64) -> Option<LogLocation> {
        let guard = epoch::pin();
        let slot = self.map.get(key, &guard)?;
        let locs = self.locations.read();
        locs.get(slot).copied()
    }

    fn insert(&self, key: u64, loc: LogLocation) -> io::Result<()> {
        let guard = epoch::pin();

        let slot = if let Some(slot) = self.free.write().pop() {
            let mut locs = self.locations.write();
            if slot >= locs.len() {
                locs.resize(
                    slot + 1,
                    LogLocation {
                        segment_id: 0,
                        offset: 0,
                        len: 0,
                    },
                );
            }
            locs[slot] = loc;
            slot
        } else {
            let mut locs = self.locations.write();
            let slot = locs.len();
            locs.push(loc);
            slot
        };

        self.map
            .insert(key, slot, &guard)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        Ok(())
    }

    fn remove(&self, key: u64) {
        let guard = epoch::pin();
        if let Some(slot) = self.map.remove(key, &guard) {
            self.free.write().push(slot);
        }
    }

    fn truncate_from(&self, first_removed: u64) {
        // Remove keys in [first_removed, u64::MAX] in batches.
        let guard = epoch::pin();
        let mut buf: Vec<([u8; 8], usize)> = vec![([0u8; 8], 0usize); 256];

        loop {
            let n = self
                .map
                .range(first_removed, u64::MAX, &mut buf[..], &guard);
            if n == 0 {
                break;
            }

            for i in 0..n {
                let key = u64::from_be_bytes(buf[i].0);
                if key < first_removed {
                    continue;
                }
                if let Some(slot) = self.map.remove(key, &guard) {
                    self.free.write().push(slot);
                }
            }
        }
    }

    fn purge_to(&self, last_removed: u64) {
        // Remove keys in [0, last_removed] in batches.
        let guard = epoch::pin();
        let mut buf: Vec<([u8; 8], usize)> = vec![([0u8; 8], 0usize); 256];

        let end = last_removed.saturating_add(1);
        loop {
            let n = self.map.range(0, end, &mut buf[..], &guard);
            if n == 0 {
                break;
            }

            for i in 0..n {
                let key = u64::from_be_bytes(buf[i].0);
                if key > last_removed {
                    continue;
                }
                if let Some(slot) = self.map.remove(key, &guard) {
                    self.free.write().push(slot);
                }
            }
        }
    }
}

/// Owned parsed record to avoid lifetime issues
#[derive(Debug)]
struct OwnedParsedRecord {
    pub record_type: RecordType,
    pub group_id: u64,
    pub payload: Vec<u8>,
}

/// Iterator for reading records from a segment file.
/// Reads aligned chunks and yields parsed records with proper boundary handling.
struct RecordIterator<'a> {
    file: &'a File,
    offset: u64,
    end_offset: u64,
    chunk_size: usize,
    buffer: Vec<u8>,
    carry: Vec<u8>,
    position: usize,
    dio_scratch: &'a mut AlignedBuf,
}

impl<'a> RecordIterator<'a> {
    /// Creates a new RecordIterator
    pub fn new(
        file: &'a File,
        start_offset: u64,
        end_offset: u64,
        chunk_size: usize,
        dio_scratch: &'a mut AlignedBuf,
    ) -> Self {
        Self {
            file,
            offset: start_offset,
            end_offset,
            chunk_size,
            buffer: Vec::new(),
            carry: Vec::new(),
            position: 0,
            dio_scratch,
        }
    }

    /// Reads the next chunk from the file, handling alignment requirements
    pub async fn read_chunk(&mut self) -> io::Result<bool> {
        if self.offset >= self.end_offset {
            return Ok(false);
        }

        // Calculate aligned read range
        let remaining = (self.end_offset - self.offset) as usize;
        let to_read = remaining.min(self.chunk_size);

        // Ensure buffer is large enough
        if self.buffer.capacity() < to_read {
            self.buffer.reserve(to_read - self.buffer.capacity());
        }

        // Read the chunk with alignment safety
        read_exact_at_reuse(
            self.file,
            self.offset,
            to_read,
            &mut self.buffer,
            self.dio_scratch,
        )
        .await?;

        // Prepend any carried-over data from previous chunk
        if !self.carry.is_empty() {
            self.carry.extend_from_slice(&self.buffer);
            std::mem::swap(&mut self.carry, &mut self.buffer);
            self.carry.clear();
        }

        self.position = 0;
        self.offset += to_read as u64;
        Ok(true)
    }

    /// Returns the next record from the current chunk, or None if more data is needed
    pub async fn next_record(&mut self) -> io::Result<Option<(u64, OwnedParsedRecord)>> {
        loop {
            if self.position + LENGTH_SIZE > self.buffer.len() {
                // Not enough data for length prefix
                return Ok(None);
            }

            // Read length prefix
            let len_bytes: [u8; LENGTH_SIZE] = self.buffer
                [self.position..self.position + LENGTH_SIZE]
                .try_into()
                .unwrap();
            let total_len = u32::from_le_bytes(len_bytes) as usize;

            // Validate length using configurable max_record_size (fallback to chunk_size if not configured)
            if total_len < MIN_RECORD_SIZE - LENGTH_SIZE || total_len > self.chunk_size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid record length: {}", total_len),
                ));
            }

            let record_start = self.position + LENGTH_SIZE;
            let record_end = record_start + total_len;

            if record_end > self.buffer.len() {
                // Record spans chunk boundary - carry over to next read
                self.carry = self.buffer[self.position..].to_vec();
                self.position = self.buffer.len(); // Mark buffer as consumed

                if !self.read_chunk().await? {
                    return Ok(None);
                }
                continue;
            }

            // Parse the record
            let record_data = &self.buffer[record_start..record_end];
            let record_offset = self.offset - self.buffer.len() as u64 + self.position as u64;

            let parsed = validate_record(record_data, self.chunk_size)?;

            self.position = record_end;

            // Convert to owned record to avoid lifetime issues
            let owned_record = OwnedParsedRecord {
                record_type: parsed.record_type,
                group_id: parsed.group_id,
                payload: record_data[1 + GROUP_ID_SIZE..record_data.len() - CRC64_SIZE].to_vec(),
            };

            return Ok(Some((record_offset, owned_record)));
        }
    }
}

struct RecoveredGroup<C: RaftTypeConfig> {
    vote: Option<openraft::impls::Vote<C>>,
    last_log_id: Option<LogId<C>>,
    last_purged_log_id: Option<LogId<C>>,
    first_index: u64,
    last_index: u64,
    log_index: LogIndex,
    cache: Vec<(u64, Arc<C::Entry>)>,
}

async fn replay_group_from_manifest<C>(
    group_dir: &PathBuf,
    group_id: u64,
    manifest: &[(u64, u64)],
    max_cache_entries: usize,
) -> io::Result<RecoveredGroup<C>>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            Vote = openraft::impls::Vote<C>,
            Node = openraft::impls::BasicNode,
            Entry = openraft::impls::Entry<C>,
        >,
    C::D: FromCodec<RawBytes>,
{
    let mut vote: Option<openraft::impls::Vote<C>> = None;
    let mut last_purged_log_id: Option<LogId<C>> = None;

    let log_index = LogIndex::new();
    let mut last_index: u64 = 0;
    let mut first_index: u64 = 0;
    let mut last_log_id_hint: Option<LogId<C>> = None;
    let mut cache: Vec<(u64, Arc<C::Entry>)> = Vec::new();
    let mut cache_low: u64 = 0;
    // Note: cache_low is computed and used later in recovery process

    // Reusable buffer for reading record fields/bodies.
    let mut read_buf: Vec<u8> = Vec::with_capacity(4096);
    let mut dio_scratch = AlignedBuf::default();

    // Calculate max record size from config
    let max_record_size = manifest
        .get(0)
        .and_then(|m| m.1.checked_sub(1).map(|i| i as usize))
        .unwrap_or(DEFAULT_CHUNK_SIZE);
    // Note: cache_low is computed and used later in recovery process

    for (segment_id, valid_bytes) in manifest.iter().copied() {
        if valid_bytes == 0 {
            continue;
        }

        let path = SegmentedLog::segment_path(group_dir, group_id, segment_id);
        let file = open_segment_file(&path, true, false, false, false).await?;

        // Use streaming record iterator for efficient bulk reading
        let mut record_iter = RecordIterator::new(
            &file,
            0u64,
            valid_bytes,
            DEFAULT_CHUNK_SIZE,
            &mut dio_scratch,
        );

        // Pre-read first chunk
        record_iter.read_chunk().await?;

        while let Some((record_offset, parsed)) = record_iter.next_record().await? {
            if parsed.group_id != group_id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "group_id mismatch in segment {}: expected {}, got {}",
                        segment_id, group_id, parsed.group_id
                    ),
                ));
            }

            match parsed.record_type {
                RecordType::Vote => {
                    let codec_vote = CodecVote::decode_from_slice(&parsed.payload)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                    vote = Some(openraft::impls::Vote::<C>::from_codec(codec_vote));
                }
                RecordType::Entry => {
                    let codec_entry = CodecEntry::<RawBytes>::decode_from_slice(&parsed.payload)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                    let entry: openraft::impls::Entry<C> =
                        openraft::impls::Entry::<C>::from_codec(codec_entry);
                    let log_id = entry.log_id;
                    let index = log_id.index;

                    log_index.insert(
                        index,
                        LogLocation {
                            segment_id,
                            offset: record_offset,
                            len: (LENGTH_SIZE
                                + 1
                                + GROUP_ID_SIZE
                                + parsed.payload.len()
                                + CRC64_SIZE) as u32,
                        },
                    )?;

                    last_index = last_index.max(index);
                    last_log_id_hint = Some(log_id);

                    // Maintain a simple "last N" cache window.
                    if max_cache_entries > 0 {
                        let keep = max_cache_entries as u64;
                        cache_low = last_index
                            .saturating_sub(keep.saturating_sub(1))
                            .max(first_index);
                        if index >= cache_low {
                            cache.push((index, Arc::new(entry)));
                            // Prune old cached entries.
                            cache.retain(|(i, _)| *i >= cache_low);
                        }
                    }
                }
                RecordType::Truncate => {
                    // New format: full LogId. Backwards compat: old payload was only u64 index.
                    let (truncate_index, truncate_log_id) =
                        match CodecLogId::decode_from_slice(&parsed.payload) {
                            Ok(codec_lid) => {
                                let lid = LogId::<C>::from_codec(codec_lid);
                                (lid.index, Some(lid))
                            }
                            Err(_) if parsed.payload.len() == 8 => {
                                let idx = u64::from_le_bytes(parsed.payload.try_into().unwrap());
                                (idx, None)
                            }
                            Err(e) => {
                                return Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    e.to_string(),
                                ));
                            }
                        };

                    let split_key = truncate_index.saturating_add(1);
                    log_index.truncate_from(split_key);
                    cache.retain(|(i, _)| *i <= truncate_index);
                    last_index = truncate_index;
                    if let Some(lid) = truncate_log_id {
                        last_log_id_hint = Some(lid);
                    } else {
                        last_log_id_hint = None;
                    }
                }
                RecordType::Purge => {
                    let codec_lid = CodecLogId::decode_from_slice(&parsed.payload)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                    let lid = LogId::<C>::from_codec(codec_lid);
                    let purge_index = lid.index;
                    last_purged_log_id = Some(lid);

                    log_index.purge_to(purge_index);
                    first_index = purge_index.saturating_add(1);
                    cache.retain(|(i, _)| *i >= first_index);
                }
                _ => {
                    // LibSQL and other record types are ignored for Raft recovery state here.
                }
            }
        }
    }

    // Determine first index from purge marker (if present).
    if let Some(ref p) = last_purged_log_id {
        first_index = first_index.max(p.index.saturating_add(1));
    }

    // If we don't have a reliable last_log_id hint (e.g. old truncate format),
    // try to read the last entry's LogId from disk using the index map.
    let last_log_id = if let Some(lid) = last_log_id_hint {
        Some(lid)
    } else if last_index > 0 {
        if let Some(loc) = log_index.get(last_index) {
            let path = SegmentedLog::segment_path(group_dir, group_id, loc.segment_id);
            let file = open_segment_file(&path, true, false, false, false).await?;
            read_exact_at_reuse(
                &file,
                loc.offset,
                loc.len as usize,
                &mut read_buf,
                &mut dio_scratch,
            )
            .await?;
            let buf = &read_buf[..loc.len as usize];
            let parsed = validate_record(&buf[LENGTH_SIZE..loc.len as usize], max_record_size)?;
            if parsed.record_type == RecordType::Entry {
                let codec_entry = CodecEntry::<RawBytes>::decode_from_slice(parsed.payload)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                let entry: openraft::impls::Entry<C> =
                    openraft::impls::Entry::<C>::from_codec(codec_entry);
                Some(entry.log_id)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(RecoveredGroup {
        vote,
        last_log_id,
        last_purged_log_id,
        first_index,
        last_index,
        log_index,
        cache,
    })
}

impl<C: RaftTypeConfig + 'static> GroupState<C> {
    async fn new(
        group_id: u64,
        config: &StorageConfig,
        manifest: Arc<ManifestManager>,
    ) -> io::Result<Self>
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
                Vote = openraft::impls::Vote<C>,
                Node = openraft::impls::BasicNode,
                Entry = openraft::impls::Entry<C>,
            >,
        C::D: FromCodec<RawBytes>,
    {
        let group_dir = config.base_dir.join(format!("{}", group_id));

        // Recover segments (truncate partial writes) and build a replay manifest.
        let (segmented_log, replay_manifest) = SegmentedLog::recover(
            group_dir.clone(),
            group_id,
            config.segment_size,
            manifest.clone(),
            manifest.sender(),
        )
        .await?;

        // Replay records to rebuild in-memory state and build an on-disk index.
        let recovered = replay_group_from_manifest::<C>(
            &group_dir,
            group_id,
            &replay_manifest,
            config.max_cache_entries_per_group,
        )
        .await?;

        let shard = ShardState::spawn(segmented_log, config.fsync_interval);

        let vote = AtomicVote::new();
        vote.store::<C>(recovered.vote.as_ref());

        let last_log_id = AtomicLogId::new();
        last_log_id.store::<C>(recovered.last_log_id.as_ref());

        let last_purged_log_id = AtomicLogId::new();
        last_purged_log_id.store::<C>(recovered.last_purged_log_id.as_ref());

        let cache = DashMap::new();
        for (idx, entry) in recovered.cache {
            cache.insert(idx, entry);
        }

        let cache_low = if config.max_cache_entries_per_group == 0 {
            recovered.last_index.saturating_add(1)
        } else {
            let keep = config.max_cache_entries_per_group as u64;
            recovered
                .last_index
                .saturating_sub(keep.saturating_sub(1))
                .max(recovered.first_index)
        };

        Ok(Self {
            cache,
            log_index: recovered.log_index,
            vote,
            first_index: AtomicU64::new(recovered.first_index),
            last_index: AtomicU64::new(recovered.last_index),
            cache_low: AtomicU64::new(cache_low),
            last_log_id,
            last_purged_log_id,
            shard: Arc::new(shard),
        })
    }
}

/// Atomic storage for Vote.
///
/// Uses a SeqLock-style approach: a sequence number protects the two u64 values.
/// Writers increment the sequence before and after writing (odd = write in progress).
/// Readers retry if they see an odd sequence or if the sequence changed during read.
/// This provides lock-free reads with consistent snapshots.
struct AtomicVote {
    /// Sequence number: odd = write in progress, even = stable
    seq: AtomicU64,
    /// term (64 bits)
    high: AtomicU64,
    /// [node_id: 62 bits][committed: 1 bit][valid: 1 bit]
    low: AtomicU64,
}

impl AtomicVote {
    const VALID_BIT: u64 = 1;
    const COMMITTED_BIT: u64 = 2;
    const NODE_ID_SHIFT: u32 = 2;

    fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            high: AtomicU64::new(0),
            low: AtomicU64::new(0),
        }
    }

    fn store<C>(&self, vote: Option<&openraft::impls::Vote<C>>)
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            >,
    {
        // Increment sequence to odd (write in progress)
        let seq = self.seq.fetch_add(1, Ordering::Release);
        debug_assert!(seq % 2 == 0, "concurrent writes to AtomicVote");

        match vote {
            Some(v) => {
                let high = v.leader_id.term;
                let low = (v.leader_id.node_id << Self::NODE_ID_SHIFT)
                    | if v.committed { Self::COMMITTED_BIT } else { 0 }
                    | Self::VALID_BIT;
                self.high.store(high, Ordering::Relaxed);
                self.low.store(low, Ordering::Relaxed);
            }
            None => {
                self.high.store(0, Ordering::Relaxed);
                self.low.store(0, Ordering::Relaxed);
            }
        }

        // Increment sequence to even (write complete)
        self.seq.fetch_add(1, Ordering::Release);
    }

    fn load<C>(&self) -> Option<openraft::impls::Vote<C>>
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            >,
    {
        loop {
            let seq1 = self.seq.load(Ordering::Acquire);
            if seq1 % 2 != 0 {
                // Write in progress, spin
                std::hint::spin_loop();
                continue;
            }

            let high = self.high.load(Ordering::Relaxed);
            let low = self.low.load(Ordering::Relaxed);

            let seq2 = self.seq.load(Ordering::Acquire);
            if seq1 != seq2 {
                // Sequence changed during read, retry
                std::hint::spin_loop();
                continue;
            }

            // Consistent read
            if (low & Self::VALID_BIT) == 0 {
                return None;
            }

            let term = high;
            let node_id = low >> Self::NODE_ID_SHIFT;
            let committed = (low & Self::COMMITTED_BIT) != 0;

            return Some(openraft::impls::Vote {
                leader_id: openraft::impls::leader_id_adv::LeaderId { term, node_id },
                committed,
            });
        }
    }
}

/// Atomic storage for LogId.
///
/// Uses a SeqLock-style approach: a sequence number protects the two u64 values.
/// Writers increment the sequence before and after writing (odd = write in progress).
/// Readers retry if they see an odd sequence or if the sequence changed during read.
/// This provides lock-free reads with consistent snapshots.
struct AtomicLogId {
    /// Sequence number: odd = write in progress, even = stable
    seq: AtomicU64,
    /// [term: 40 bits][node_id: 24 bits]
    high: AtomicU64,
    /// [index: 63 bits][valid: 1 bit]
    low: AtomicU64,
}

impl AtomicLogId {
    const VALID_BIT: u64 = 1;
    const INDEX_SHIFT: u32 = 1;
    const TERM_SHIFT: u32 = 24;
    const MAX_TERM: u64 = (1u64 << (64 - Self::TERM_SHIFT)) - 1;
    const MAX_NODE_ID: u64 = 0xFF_FFFF;

    fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            high: AtomicU64::new(0),
            low: AtomicU64::new(0),
        }
    }

    fn store<C>(&self, log_id: Option<&LogId<C>>)
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            >,
    {
        // Increment sequence to odd (write in progress)
        let seq = self.seq.fetch_add(1, Ordering::Release);
        debug_assert!(seq % 2 == 0, "concurrent writes to AtomicLogId");

        match log_id {
            Some(lid) => {
                // Check bounds in both debug and release builds to prevent silent corruption
                assert!(
                    lid.leader_id.term <= Self::MAX_TERM,
                    "term {} exceeds maximum {} for packed AtomicLogId",
                    lid.leader_id.term,
                    Self::MAX_TERM
                );
                assert!(
                    lid.leader_id.node_id <= Self::MAX_NODE_ID,
                    "node_id {} exceeds maximum {} for packed AtomicLogId",
                    lid.leader_id.node_id,
                    Self::MAX_NODE_ID
                );
                let high = (lid.leader_id.term << Self::TERM_SHIFT)
                    | (lid.leader_id.node_id & Self::MAX_NODE_ID);
                let low = (lid.index << Self::INDEX_SHIFT) | Self::VALID_BIT;
                self.high.store(high, Ordering::Relaxed);
                self.low.store(low, Ordering::Relaxed);
            }
            None => {
                self.high.store(0, Ordering::Relaxed);
                self.low.store(0, Ordering::Relaxed);
            }
        }

        // Increment sequence to even (write complete)
        self.seq.fetch_add(1, Ordering::Release);
    }

    fn load<C>(&self) -> Option<LogId<C>>
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            >,
    {
        loop {
            let seq1 = self.seq.load(Ordering::Acquire);
            if seq1 % 2 != 0 {
                // Write in progress, spin
                std::hint::spin_loop();
                continue;
            }

            let high = self.high.load(Ordering::Relaxed);
            let low = self.low.load(Ordering::Relaxed);

            let seq2 = self.seq.load(Ordering::Acquire);
            if seq1 != seq2 {
                // Sequence changed during read, retry
                std::hint::spin_loop();
                continue;
            }

            // Consistent read
            if (low & Self::VALID_BIT) == 0 {
                return None;
            }

            let term = high >> Self::TERM_SHIFT;
            let node_id = high & Self::MAX_NODE_ID;
            let index = low >> Self::INDEX_SHIFT;

            return Some(LogId {
                leader_id: openraft::impls::leader_id_adv::LeaderId { term, node_id },
                index,
            });
        }
    }
}

/// Write request sent to shard writer task
enum WriteRequest<C: RaftTypeConfig> {
    /// Write data to the log
    Write {
        data: Vec<u8>,
        /// Optional entry index range covered by this write (min,max), for segment metadata/compaction.
        entry_index_range: Option<(u64, u64)>,
        /// Complete these callbacks after the next durable fsync that covers this write.
        callbacks: Callbacks<C>,
        /// If true, fsync immediately after writing (and before replying).
        sync_immediately: bool,
        /// Response completes when the write is processed; includes fsync result if `sync_immediately`.
        /// Always returns the `Vec<u8>` so the sender can reuse its capacity.
        respond_to: oneshot::Sender<(io::Result<()>, Vec<u8>, Option<WritePlacement>)>,
    },
    /// Shutdown the writer task
    Shutdown,
}

/// Callback payload optimized for the common cases (0 or 1 callback).
enum Callbacks<C: RaftTypeConfig> {
    None,
    One(IOFlushed<C>),
    Many(Vec<IOFlushed<C>>),
}

impl<C: RaftTypeConfig> Callbacks<C> {
    #[inline]
    fn is_empty(&self) -> bool {
        matches!(self, Self::None)
    }

    #[inline]
    fn extend_into(self, dst: &mut Vec<IOFlushed<C>>) {
        match self {
            Self::None => {}
            Self::One(cb) => dst.push(cb),
            Self::Many(mut v) => dst.append(&mut v),
        }
    }
}

#[cfg(test)]
impl<C: RaftTypeConfig> Callbacks<C> {
    #[allow(dead_code)]
    fn _from_vec(v: Vec<IOFlushed<C>>) -> Self {
        match v.len() {
            0 => Self::None,
            1 => Self::One(v.into_iter().next().unwrap()),
            _ => Self::Many(v),
        }
    }
}

#[derive(Debug, Clone)]
struct ShardError {
    kind: io::ErrorKind,
    message: String,
}

impl ShardError {
    fn to_io_error(&self) -> io::Error {
        io::Error::new(self.kind, self.message.clone())
    }
}

#[derive(Debug, Clone, Copy)]
struct WritePlacement {
    segment_id: u64,
    offset: u64,
}

/// Manages segmented log files for a group
struct SegmentedLog {
    /// Base directory for this group's segments
    group_dir: PathBuf,
    /// Group ID
    group_id: u64,
    /// Maximum segment size
    segment_size: u64,
    /// Current segment ID (monotonically increasing)
    current_segment_id: u64,
    /// Current segment file
    current_file: File,
    /// Current segment's size
    current_size: u64,
    /// First and last entry index contained in this segment (best-effort; updated on writes).
    /// Used for segment-level compaction decisions.
    min_entry_index: Option<u64>,
    max_entry_index: Option<u64>,
    /// Best-effort write time range (unix nanos) for entries written into this segment.
    min_ts: Option<u64>,
    max_ts: Option<u64>,
    /// A sealed segment update emitted when `rotate()` is called.
    sealed_update: Option<SegmentMeta>,
    /// Manifest update channel (best-effort, non-blocking).
    manifest_tx: MAsyncTx<SegmentMeta>,
}

impl SegmentedLog {
    /// Create or recover a segmented log for a group (async version)
    async fn recover(
        group_dir: PathBuf,
        group_id: u64,
        segment_size: u64,
        manifest: Arc<ManifestManager>,
        manifest_tx: MAsyncTx<SegmentMeta>,
    ) -> io::Result<(Self, Vec<(u64, u64)>)> {
        maniac::fs::create_dir_all(&group_dir).await?;

        // Find existing segments
        let segments = Self::list_segments_async(&group_dir, group_id).await?;

        let (current_segment_id, current_file, current_size, replay_manifest) = if segments
            .is_empty()
        {
            // No existing segments - create first one
            let segment_id = 0u64;
            let path = Self::segment_path(&group_dir, group_id, segment_id);

            // Ensure segment_size is properly aligned for Direct I/O
            let aligned_segment_size = align_up_u64(segment_size, dio_align());
            if aligned_segment_size != segment_size {
                tracing::warn!(
                    "segment_size {} is not aligned to {} bytes, using aligned size {}",
                    segment_size,
                    dio_align(),
                    aligned_segment_size
                );
            }

            // Open file initially without Direct I/O for setup operations on problematic filesystems
            let file = open_segment_file(&path, true, true, true, true).await?;
            // Preallocate segment to full size for alignment and performance
            file.set_len(aligned_segment_size).await?;
            drop(file); // Close the file

            // Reopen with Direct I/O enabled if supported
            let file = open_segment_file(&path, true, true, false, false).await?;
            (segment_id, file, 0, vec![(segment_id, 0)])
        } else {
            // Use MDBX segment metadata to avoid scanning all segments.
            // We only validate the *tail* segment; earlier segments are assumed sealed.
            let meta_map: std::collections::HashMap<u64, SegmentMeta> = {
                let manifest = manifest.clone();
                maniac::blocking::unblock(move || manifest.read_group_segments(group_id)).await?
            };

            let mut manifest_vec: Vec<(u64, u64)> = Vec::with_capacity(segments.len());

            let mut last_segment_id = 0u64;
            let mut last_valid_bytes = 0u64;

            for (i, segment_id) in segments.iter().enumerate() {
                let path = Self::segment_path(&group_dir, group_id, *segment_id);
                let file_len = maniac::fs::metadata(&path).await?.len();

                // Only validate the last segment's tail; earlier segments use manifest metadata if present.
                let is_last = i + 1 == segments.len();
                let (valid_bytes, file_len) = if is_last {
                    Self::recover_segment_async(&path, segment_size).await?
                } else if let Some(m) = meta_map.get(segment_id) {
                    (m.valid_bytes.min(file_len), file_len)
                } else {
                    // Fallback: if metadata missing, validate this segment too.
                    Self::recover_segment_async(&path, segment_size).await?
                };

                // Truncate this segment if it contains a partial/corrupt tail.
                if valid_bytes < file_len {
                    // Truncation does not benefit from direct I/O and may fail with `EINVAL` on
                    // some kernels when the fd is opened with `O_DIRECT`. Do it on a buffered fd.
                    let desired_len = align_up_u64(valid_bytes, dio_align());

                    // For truncation operations, always use a file opened without Direct I/O
                    // to avoid alignment restrictions
                    {
                        let trunc_file = OpenOptions::new()
                            .read(true)
                            .write(true)
                            .open(&path)
                            .await?;

                        if file_len != desired_len {
                            trunc_file.set_len(desired_len).await?;
                        }

                        if desired_len > valid_bytes {
                            let pad = (desired_len - valid_bytes) as usize;
                            // This write is small (<= 4KB) and doesn't need Direct I/O
                            let (res, _buf) = trunc_file
                                .write_all_at(&ZERO_4K[..pad] as &'static [u8], valid_bytes)
                                .await;
                            res?;
                        }
                    }

                    // Remove any later segments; they are after the first invalid record.
                    for later_id in segments.iter().skip(i + 1) {
                        let later_path = Self::segment_path(&group_dir, group_id, *later_id);
                        // Best-effort: we cannot unlink without the `unlinkat` feature.
                        // Leaving later segments is safe because recovery will stop at the first
                        // invalid tail again and ignore later segments.
                        let _ = later_path;
                    }
                }

                manifest_vec.push((*segment_id, valid_bytes));
                last_segment_id = *segment_id;
                last_valid_bytes = valid_bytes;

                if valid_bytes < file_len {
                    break;
                }
            }

            // Open the last segment for appending.
            let path = Self::segment_path(&group_dir, group_id, last_segment_id);
            // Ensure the last segment has an aligned length so subsequent O_DIRECT RMW reads work.
            {
                let desired_len = align_up_u64(last_valid_bytes, dio_align());
                let tmp = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .await?;
                let current_len = tmp.metadata().await?.len();
                if current_len != desired_len {
                    tmp.set_len(desired_len).await?;
                }
                if desired_len > last_valid_bytes {
                    let pad = (desired_len - last_valid_bytes) as usize;
                    let (res, _buf) = tmp
                        .write_all_at(&ZERO_4K[..pad] as &'static [u8], last_valid_bytes)
                        .await;
                    res?;
                }
            }

            let file = open_segment_file(&path, true, true, false, false).await?;

            (last_segment_id, file, last_valid_bytes, manifest_vec)
        };

        Ok((
            Self {
                group_dir,
                group_id,
                segment_size,
                current_segment_id,
                current_file,
                current_size,
                min_entry_index: None,
                max_entry_index: None,
                min_ts: None,
                max_ts: None,
                sealed_update: None,
                manifest_tx,
            },
            replay_manifest,
        ))
    }

    /// List segment IDs for a group, sorted by ID (async version)
    async fn list_segments_async(group_dir: &PathBuf, group_id: u64) -> io::Result<Vec<u64>> {
        let prefix = format!("group-{}-", group_id);
        let suffix = ".log";

        let mut segments = Vec::new();

        // Use async read_dir from maniac::fs
        if let Ok(entries) = maniac::fs::read_dir(group_dir).await {
            for entry in entries {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if name_str.starts_with(&prefix) && name_str.ends_with(&suffix) {
                    // Extract segment ID from filename
                    let id_str = &name_str[prefix.len()..name_str.len() - suffix.len()];
                    if let Ok(segment_id) = id_str.parse::<u64>() {
                        segments.push(segment_id);
                    }
                }
            }
        }

        segments.sort();
        Ok(segments)
    }

    /// Get the path for a segment file
    fn segment_path(group_dir: &PathBuf, group_id: u64, segment_id: u64) -> PathBuf {
        group_dir.join(format!("group-{}-{}.log", group_id, segment_id))
    }

    /// Recover a segment file using async I/O, validating all records
    /// Returns the byte offset of the last valid record's end
    async fn recover_segment_async(path: &PathBuf, max_record_size: u64) -> io::Result<(u64, u64)> {
        let file = open_segment_file(path.as_path(), true, false, false, false).await?;
        let file_len = file.metadata().await?.len();

        if file_len == 0 {
            return Ok((0, 0));
        }

        let mut offset = 0u64;
        let mut read_buf: Vec<u8> = Vec::with_capacity(4096);
        let mut dio_scratch = AlignedBuf::default();

        loop {
            if offset + LENGTH_SIZE as u64 > file_len {
                break;
            }

            let len_bytes = match read_exact_at_reuse(
                &file,
                offset,
                LENGTH_SIZE,
                &mut read_buf,
                &mut dio_scratch,
            )
            .await
            {
                Ok(()) => &read_buf[..LENGTH_SIZE],
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(_) => break,
            };

            let record_len =
                u32::from_le_bytes(len_bytes[..LENGTH_SIZE].try_into().unwrap()) as usize;

            // Handle zero-length record (padding at end of segment)
            if record_len == 0 {
                tracing::debug!(
                    "Found zero-length record at offset {}, treating as padding",
                    offset
                );
                break;
            }

            // Use provided max_record_size, but also ensure it doesn't exceed file boundaries
            let effective_max = max_record_size.min(file_len) as usize;
            if record_len < (1 + GROUP_ID_SIZE + CRC64_SIZE) || record_len > effective_max {
                tracing::debug!("Invalid record length {} at offset {}", record_len, offset);
                break;
            }
            if offset + LENGTH_SIZE as u64 + record_len as u64 > file_len {
                tracing::debug!(
                    "Record at offset {} would exceed file length (record_len={}, file_len={})",
                    offset,
                    record_len,
                    file_len
                );
                break;
            }

            // Read and validate record data - handle UnexpectedEof gracefully (padding bytes)
            // Also handle "failed to fill whole buffer" which can occur with Direct I/O
            match read_exact_at_reuse(
                &file,
                offset + LENGTH_SIZE as u64,
                record_len,
                &mut read_buf,
                &mut dio_scratch,
            )
            .await
            {
                Ok(()) => {
                    // Successfully read record data, now validate it
                    match validate_record(&read_buf[..record_len], max_record_size as usize) {
                        Ok(_parsed) => {
                            // Record is valid, continue to next
                            offset += LENGTH_SIZE as u64 + record_len as u64;
                        }
                        Err(e) => {
                            tracing::debug!("Record validation failed at offset {}: {}", offset, e);
                            break;
                        }
                    }
                }
                Err(e)
                    if e.kind() == io::ErrorKind::UnexpectedEof
                        || e.to_string().contains("failed to fill whole buffer") =>
                {
                    // Padding bytes at end of segment can look like length prefix
                    // but there's insufficient data - this is normal, stop here
                    tracing::debug!(
                        "Reached end of valid data at offset {} (len={}): {}",
                        offset,
                        record_len,
                        e
                    );
                    break;
                }
                Err(e) => {
                    // Other errors are more serious
                    tracing::warn!(
                        "Failed to read record data at offset {} (len={}): {}",
                        offset,
                        record_len,
                        e
                    );
                    break;
                }
            }
        }

        tracing::info!(
            "Recovered segment {:?}: valid_bytes={}, file_len={}",
            path,
            offset,
            file_len
        );
        Ok((offset, file_len))
    }

    /// Write data to the current segment, rotating if needed
    async fn write(
        &mut self,
        data: Vec<u8>,
        index_range: Option<(u64, u64)>,
    ) -> io::Result<(Vec<u8>, WritePlacement)> {
        let data_len = data.len() as u64;

        // Check if we need to rotate to a new segment
        let mut base_offset = self.current_size;
        if self.current_size + data_len > self.segment_size {
            self.rotate().await?;
            base_offset = 0;
        }

        if let Some((min_i, max_i)) = index_range {
            self.min_entry_index =
                Some(self.min_entry_index.map(|v| v.min(min_i)).unwrap_or(min_i));
            self.max_entry_index =
                Some(self.max_entry_index.map(|v| v.max(max_i)).unwrap_or(max_i));
        }

        // Write to current segment at the computed offset.
        // The file may be opened with direct I/O; use the unaligned helper to stay correct with
        // variable-length records.
        self.current_file
            .write_all_at_unaligned(base_offset, &data)
            .await?;

        let segment_id = self.current_segment_id;
        self.current_size += data_len;
        Ok((
            data,
            WritePlacement {
                segment_id,
                offset: base_offset,
            },
        ))
    }

    /// Rotate to a new segment
    async fn rotate(&mut self) -> io::Result<()> {
        // Sync current segment before rotating
        self.current_file.sync_all().await?;

        // Record a sealed segment update (best-effort) for the segment we're leaving.
        self.sealed_update = Some(SegmentMeta {
            group_id: self.group_id,
            segment_id: self.current_segment_id,
            valid_bytes: self.current_size,
            min_index: self.min_entry_index,
            max_index: self.max_entry_index,
            min_ts: self.min_ts,
            max_ts: self.max_ts,
            sealed: true,
        });

        // Create new segment
        self.current_segment_id += 1;
        let path = Self::segment_path(&self.group_dir, self.group_id, self.current_segment_id);

        // Use async OpenOptions to create the new segment file
        self.current_file = open_segment_file(&path, true, true, true, true).await?;
        self.current_size = 0;
        self.min_entry_index = None;
        self.max_entry_index = None;
        self.min_ts = None;
        self.max_ts = None;

        Ok(())
    }

    /// Sync the current segment
    async fn sync(&self) -> io::Result<()> {
        self.current_file.sync_all().await
    }

    fn emit_manifest_updates(&mut self) {
        // Emit sealed segment update if present.
        if let Some(sealed) = self.sealed_update.take() {
            let _ = self.manifest_tx.try_send(sealed);
        }
        // Emit current segment state.
        let current = SegmentMeta {
            group_id: self.group_id,
            segment_id: self.current_segment_id,
            valid_bytes: self.current_size,
            min_index: self.min_entry_index,
            max_index: self.max_entry_index,
            min_ts: self.min_ts,
            max_ts: self.max_ts,
            sealed: false,
        };
        let _ = self.manifest_tx.try_send(current);
    }
}

/// Per-group shard state
struct ShardState<C: RaftTypeConfig> {
    /// Channel to send write requests to the shard writer task (using flume for reliability)
    write_tx: flume::Sender<WriteRequest<C>>,
    /// Whether the shard is running
    #[allow(dead_code)]
    running: Arc<AtomicBool>,
    /// Sticky failure state: once set, all new writes fail fast.
    failed: Arc<std::sync::OnceLock<ShardError>>,
    /// Base directory for this group's segments (used for reads/recovery helpers).
    group_dir: PathBuf,
    /// Group ID for this shard.
    #[allow(dead_code)]
    group_id: u64,
}

impl<C: RaftTypeConfig + 'static> ShardState<C> {
    /// Spawn a new shard writer task using a recovered `SegmentedLog`.
    fn spawn(segmented_log: SegmentedLog, fsync_interval: Option<Duration>) -> Self {
        let group_dir = segmented_log.group_dir.clone();
        let group_id = segmented_log.group_id;

        // Create channel for write requests using flume for reliability
        let (write_tx, write_rx) = flume::bounded::<WriteRequest<C>>(256);

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let failed = Arc::new(std::sync::OnceLock::new());
        let failed_clone = failed.clone();

        // Spawn the writer task
        let _ = maniac::spawn(async move {
            Self::writer_loop(
                segmented_log,
                write_rx,
                fsync_interval,
                running_clone,
                failed_clone,
            )
            .await;
        });

        Self {
            write_tx,
            running,
            failed,
            group_dir,
            group_id,
        }
    }

    /// Writer task loop - owns the segmented log, no mutex needed
    async fn writer_loop(
        mut log: SegmentedLog,
        write_rx: flume::Receiver<WriteRequest<C>>,
        fsync_interval: Option<Duration>,
        running: Arc<AtomicBool>,
        failed: Arc<std::sync::OnceLock<ShardError>>,
    ) {
        let mut pending_callbacks: Vec<IOFlushed<C>> = Vec::new();
        let mut pending_sync_responders: Vec<(
            oneshot::Sender<(io::Result<()>, Vec<u8>, Option<WritePlacement>)>,
            Vec<u8>,
            Option<WritePlacement>,
        )> = Vec::new();

        let fail_and_exit = |err: io::Error, pending_callbacks: &mut Vec<IOFlushed<C>>| {
            let shard_err = ShardError {
                kind: err.kind(),
                message: err.to_string(),
            };
            let _ = failed.set(shard_err.clone());
            for cb in pending_callbacks.drain(..) {
                cb.io_completed(Err(shard_err.to_io_error()));
            }
            running.store(false, Ordering::Release);
        };

        loop {
            // Only wake when there is work. No periodic timer while idle.
            let mut next_req = match write_rx.recv_async().await {
                Ok(req) => Some(req),
                Err(_) => {
                    running.store(false, Ordering::Release);
                    return;
                }
            };

            // For fsync_interval=Some(_): we use an "immediate fsync per batch" strategy:
            // - first write triggers an fsync right after we drain the current queue
            // - writes arriving while fsync is in progress will queue and be included in the next fsync
            //
            // For fsync_interval=None: fsync after every write (legacy semantics).
            let mut wrote_any = false;
            let mut shutdown_after_batch = false;

            while let Some(request) = next_req.take() {
                match request {
                    WriteRequest::Write {
                        mut data,
                        entry_index_range,
                        callbacks,
                        sync_immediately,
                        respond_to,
                    } => {
                        let mut placement: Option<WritePlacement> = None;
                        if let Some(err) = failed.get() {
                            let _ = respond_to.send((Err(err.to_io_error()), data, None));
                            callbacks.extend_into(&mut pending_callbacks);
                            for cb in pending_callbacks.drain(..) {
                                cb.io_completed(Err(err.to_io_error()));
                            }
                        } else {
                            tracing::trace!(
                                "writer_loop: received write request ({} bytes), sync_immediately={}",
                                data.len(),
                                sync_immediately
                            );

                            if !data.is_empty() {
                                let write_result = log.write(data, entry_index_range).await;
                                match write_result {
                                    Ok((returned, pl)) => {
                                        data = returned;
                                        placement = Some(pl);
                                    }
                                    Err(e) => {
                                        // We can't return the original buffer (it was moved into the I/O op),
                                        // so return an empty buffer and fail the shard.
                                        let _ = respond_to.send((
                                            Err(io::Error::new(e.kind(), e.to_string())),
                                            Vec::new(),
                                            None,
                                        ));
                                        for (tx, mut buf, placement) in
                                            pending_sync_responders.drain(..)
                                        {
                                            buf.clear();
                                            let _ = tx.send((
                                                Err(io::Error::new(e.kind(), e.to_string())),
                                                buf,
                                                placement,
                                            ));
                                        }
                                        fail_and_exit(e, &mut pending_callbacks);
                                        return;
                                    }
                                };
                                wrote_any = true;
                                tracing::trace!("writer_loop: write complete");
                            }

                            callbacks.extend_into(&mut pending_callbacks);

                            if fsync_interval.is_none() {
                                // Legacy semantics: fsync after every write (or when explicitly requested).
                                // Important: callbacks must not get stranded even if this request
                                // contains no bytes (e.g. append([]) with an IOFlushed callback).
                                if sync_immediately || wrote_any || !pending_callbacks.is_empty() {
                                    let sync_result = log.sync().await;
                                    let (reply, cb_result) = match sync_result {
                                        Ok(_) => (Ok(()), Ok(())),
                                        Err(e) => {
                                            let err = io::Error::new(e.kind(), e.to_string());
                                            (Err(err), Err(io::Error::new(e.kind(), e.to_string())))
                                        }
                                    };

                                    data.clear();
                                    let _ = respond_to.send((reply, data, placement));

                                    // Complete pending callbacks for this fsync boundary.
                                    for cb in pending_callbacks.drain(..) {
                                        cb.io_completed(
                                            cb_result.as_ref().map(|_| ()).map_err(|e| {
                                                io::Error::new(e.kind(), e.to_string())
                                            }),
                                        );
                                    }

                                    match cb_result {
                                        Ok(_) => {
                                            tracing::trace!("writer_loop: sync complete");
                                            log.emit_manifest_updates();
                                        }
                                        Err(e) => {
                                            fail_and_exit(e, &mut pending_callbacks);
                                            return;
                                        }
                                    }
                                } else {
                                    data.clear();
                                    let _ = respond_to.send((Ok(()), data, placement));
                                }
                            } else {
                                // fsync_interval=Some(_): reply immediately unless the caller requires fsync.
                                data.clear();
                                if sync_immediately {
                                    pending_sync_responders.push((respond_to, data, placement));
                                } else {
                                    let _ = respond_to.send((Ok(()), data, placement));
                                }
                            }
                        }
                    }
                    WriteRequest::Shutdown => {
                        shutdown_after_batch = true;
                    }
                }

                next_req = match write_rx.try_recv() {
                    Ok(r) => Some(r),
                    Err(flume::TryRecvError::Empty) => None,
                    Err(flume::TryRecvError::Disconnected) => {
                        running.store(false, Ordering::Release);
                        return;
                    }
                };
            }

            // Immediate fsync per batch when configured with an interval.
            if fsync_interval.is_some() && wrote_any {
                let result = log.sync().await;
                match result {
                    Ok(_) => {
                        for cb in pending_callbacks.drain(..) {
                            cb.io_completed(Ok(()));
                        }
                        for (tx, mut buf, placement) in pending_sync_responders.drain(..) {
                            buf.clear();
                            let _ = tx.send((Ok(()), buf, placement));
                        }
                        log.emit_manifest_updates();
                    }
                    Err(e) => {
                        let err = io::Error::new(e.kind(), e.to_string());
                        for cb in pending_callbacks.drain(..) {
                            cb.io_completed(Err(io::Error::new(err.kind(), err.to_string())));
                        }
                        for (tx, mut buf, placement) in pending_sync_responders.drain(..) {
                            buf.clear();
                            let _ = tx.send((
                                Err(io::Error::new(err.kind(), err.to_string())),
                                buf,
                                placement,
                            ));
                        }
                        fail_and_exit(err, &mut pending_callbacks);
                        return;
                    }
                }
            }

            // If no writes occurred in this batch, there is nothing to flush; complete any queued
            // callbacks / sync-responders immediately. This avoids a hang for append([]) and
            // matches the "only when a write occurs" policy.
            if fsync_interval.is_some() && !wrote_any {
                for cb in pending_callbacks.drain(..) {
                    cb.io_completed(Ok(()));
                }
                for (tx, mut buf, placement) in pending_sync_responders.drain(..) {
                    buf.clear();
                    let _ = tx.send((Ok(()), buf, placement));
                }
            }

            if shutdown_after_batch {
                // Best-effort: ensure any queued callbacks are completed.
                for cb in pending_callbacks.drain(..) {
                    cb.io_completed(Ok(()));
                }
                for (tx, mut buf, placement) in pending_sync_responders.drain(..) {
                    buf.clear();
                    let _ = tx.send((Ok(()), buf, placement));
                }
                running.store(false, Ordering::Release);
                return;
            }
        }
    }

    /// Send a write request to the shard
    async fn write(
        &self,
        data: Vec<u8>,
        entry_index_range: Option<(u64, u64)>,
        callbacks: Callbacks<C>,
        sync_immediately: bool,
    ) -> io::Result<(Vec<u8>, Option<WritePlacement>)> {
        if let Some(err) = self.failed.get() {
            return Err(err.to_io_error());
        }

        let (tx, rx) = oneshot::channel::<(io::Result<()>, Vec<u8>, Option<WritePlacement>)>();

        tracing::trace!(
            "ShardState::write: sending {} bytes, sync_immediately={}",
            data.len(),
            sync_immediately
        );

        self.write_tx
            .send_async(WriteRequest::Write {
                data,
                entry_index_range,
                callbacks,
                sync_immediately,
                respond_to: tx,
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "shard writer task closed"))?;

        let (res, buf, placement) = rx.await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "shard writer task dropped response",
            )
        })?;
        res.map(|_| (buf, placement))
    }

    /// Shutdown the shard (fire-and-forget, spawns async send)
    fn shutdown(&self) {
        let tx = self.write_tx.clone();
        let _ = maniac::spawn(async move {
            let _ = tx.send_async(WriteRequest::Shutdown).await;
        });
    }
}

/// Per-group log storage with dedicated shard per group.
///
/// Each group has its own directory with segmented log files.
/// Groups are created dynamically on first access.
/// This eliminates mutex contention and enables independent lifecycle per group.
///
/// Segment rotation:
/// - When a segment exceeds `segment_size`, a new segment is created
/// - Segments are named: `group-{group_id}-{segment_id}.log`
///
/// Crash recovery:
/// - On startup/first access, the group recovers by scanning all segments
/// - Records are validated using CRC64-NVME checksums
/// - Partial/corrupt records at the tail are discarded
pub struct PerGroupLogStorage<C: RaftTypeConfig> {
    config: StorageConfig,
    manifest: Arc<ManifestManager>,
    /// Per-group state index: maps group_id -> slot in `group_states`
    groups: GroupIndex<C>,
}

/// Maximum number of raft groups supported (4K)
const MAX_GROUPS: usize = 4096;

/// Lock-free group index using AtomicPtr slab for O(1) access.
///
/// This provides ~2.3x faster reads than DashMap by using direct array indexing
/// instead of hashing. Each slot holds an Arc pointer stored atomically.
///
/// Safety invariants:
/// 1. Arc pointers are stable (the allocation doesn't move)
/// 2. We increment the refcount before storing and decrement on removal
/// 3. AtomicPtr provides lock-free concurrent access
struct GroupIndex<C: RaftTypeConfig> {
    /// Fixed-size array of AtomicPtr slots for O(1) access
    slots: Box<[AtomicPtr<GroupState<C>>]>,
    /// Number of active groups (for iteration optimization)
    count: AtomicUsize,
}

impl<C: RaftTypeConfig> GroupIndex<C> {
    fn new() -> Self {
        let mut slots = Vec::with_capacity(MAX_GROUPS);
        for _ in 0..MAX_GROUPS {
            slots.push(AtomicPtr::new(ptr::null_mut()));
        }
        Self {
            slots: slots.into_boxed_slice(),
            count: AtomicUsize::new(0),
        }
    }

    /// Get a group state by group_id (lock-free, O(1))
    #[inline]
    fn get(&self, group_id: u64) -> Option<Arc<GroupState<C>>> {
        let idx = group_id as usize;
        if idx >= self.slots.len() {
            return None;
        }

        let ptr = self.slots[idx].load(Ordering::Acquire);
        if ptr.is_null() {
            return None;
        }

        // Safety: ptr was created from Arc::into_raw. We reconstruct the Arc
        // temporarily to clone it, then put it back without dropping.
        unsafe {
            let arc = Arc::from_raw(ptr);
            let cloned = arc.clone();
            // Don't drop the original - put it back
            let _ = Arc::into_raw(arc);
            Some(cloned)
        }
    }

    /// Insert a group state. Returns Ok(()) if inserted, or the existing state if slot occupied.
    fn insert(&self, group_id: u64, state: Arc<GroupState<C>>) -> Result<(), Arc<GroupState<C>>> {
        let idx = group_id as usize;
        if idx >= self.slots.len() {
            return Err(state); // Out of bounds
        }

        let new_ptr = Arc::into_raw(state) as *mut GroupState<C>;

        // Use CAS to ensure we only insert if slot is empty
        match self.slots[idx].compare_exchange(
            ptr::null_mut(),
            new_ptr,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                self.count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(_existing) => {
                // Slot already occupied - return the state we tried to insert
                let state = unsafe { Arc::from_raw(new_ptr) };
                Err(state)
            }
        }
    }

    /// Remove a group state, returning it if it existed
    fn remove(&self, group_id: u64) -> Option<Arc<GroupState<C>>> {
        let idx = group_id as usize;
        if idx >= self.slots.len() {
            return None;
        }

        let ptr = self.slots[idx].swap(ptr::null_mut(), Ordering::AcqRel);
        if ptr.is_null() {
            None
        } else {
            self.count.fetch_sub(1, Ordering::Relaxed);
            // Safety: We own this pointer now (removed from slot)
            Some(unsafe { Arc::from_raw(ptr) })
        }
    }

    /// Iterate over all group states (for stop/shutdown)
    fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(u64, &Arc<GroupState<C>>),
    {
        for (idx, slot) in self.slots.iter().enumerate() {
            let ptr = slot.load(Ordering::Acquire);
            if !ptr.is_null() {
                // Safety: ptr is valid while stored in slot
                unsafe {
                    let arc = Arc::from_raw(ptr);
                    f(idx as u64, &arc);
                    // Don't drop - put it back
                    let _ = Arc::into_raw(arc);
                }
            }
        }
    }

    /// Get all group IDs
    fn group_ids(&self) -> Vec<u64> {
        let mut result = Vec::with_capacity(self.count.load(Ordering::Relaxed));
        for (idx, slot) in self.slots.iter().enumerate() {
            if !slot.load(Ordering::Acquire).is_null() {
                result.push(idx as u64);
            }
        }
        result
    }

    /// Get the number of groups
    #[inline]
    fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
}

impl<C: RaftTypeConfig> Drop for GroupIndex<C> {
    fn drop(&mut self) {
        // Clean up all remaining Arc pointers
        for slot in self.slots.iter() {
            let ptr = slot.load(Ordering::Acquire);
            if !ptr.is_null() {
                // Safety: We own this pointer, drop the Arc
                unsafe {
                    let _ = Arc::from_raw(ptr);
                }
            }
        }
    }
}

// Type aliases for compatibility
pub type ShardedLogStorage<C> = PerGroupLogStorage<C>;
pub type MultiplexedLogStorage<C> = PerGroupLogStorage<C>;

impl<C: RaftTypeConfig + 'static> PerGroupLogStorage<C> {
    /// Create a new per-group storage instance.
    /// Groups are created dynamically on first access.
    pub async fn new(config: impl Into<StorageConfig>) -> io::Result<Self> {
        let config = config.into();
        maniac::fs::create_dir_all(&*config.base_dir).await?;

        let base = (*config.base_dir).clone();
        let manifest_dir = config
            .manifest_dir
            .as_ref()
            .map(|p| (**p).clone())
            .unwrap_or(base);
        let manifest = maniac::blocking::unblock(move || {
            // Try to open MDBX manifest, but fall back to no-op if it fails
            // This can happen on filesystems that don't support Direct I/O like tmpfs
            let result = match ManifestManager::open(&manifest_dir) {
                Ok(manifest) => Ok(manifest),
                Err(e) if e.raw_os_error() == Some(22) || e.kind() == io::ErrorKind::InvalidInput => {
                    tracing::warn!(
                        "MDBX manifest not supported on this filesystem ({}), using in-memory fallback",
                        e
                    );
                    Ok(ManifestManager::open_in_memory())
                }
                Err(e) => Err(e),
            };
            result
        })
        .await?;

        Ok(Self {
            config,
            manifest: Arc::new(manifest),
            groups: GroupIndex::new(),
        })
    }

    /// Stop all groups
    pub fn stop(&self) {
        self.groups.for_each(|_group_id, state| {
            state.shard.shutdown();
        });
    }

    /// Get or create state for a group (creates shard on first access)
    async fn get_or_create_group(&self, group_id: u64) -> io::Result<Arc<GroupState<C>>>
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
                Vote = openraft::impls::Vote<C>,
                Node = openraft::impls::BasicNode,
                Entry = openraft::impls::Entry<C>,
            >,
        C::D: FromCodec<RawBytes>,
    {
        // Fast path: group already exists (lock-free lookup)
        if let Some(state) = self.groups.get(group_id) {
            return Ok(state);
        }

        // Slow path: need to create the group.
        // Create the state - if another task beats us, we'll detect on insert
        let state = Arc::new(GroupState::new(group_id, &self.config, self.manifest.clone()).await?);

        // Try to insert - if someone else inserted while we were creating,
        // shut down our orphan and use theirs
        match self.groups.insert(group_id, state.clone()) {
            Ok(()) => Ok(state),
            Err(returned_state) => {
                // Another task won - shut down our orphaned shard and use theirs
                returned_state.shard.shutdown();
                // Get the winner's state
                Ok(self
                    .groups
                    .get(group_id)
                    .expect("group must exist after failed insert"))
            }
        }
    }

    /// Get a log storage handle for a specific group
    pub async fn get_log_storage(&self, group_id: u64) -> io::Result<GroupLogStorage<C>>
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
                Vote = openraft::impls::Vote<C>,
                Node = openraft::impls::BasicNode,
                Entry = openraft::impls::Entry<C>,
            >,
        C::D: FromCodec<RawBytes>,
    {
        let group_state = self.get_or_create_group(group_id).await?;
        Ok(GroupLogStorage {
            group_id,
            config: self.config.clone(),
            state: group_state,
            encode_buf: Vec::new(),
            payload_buf: Vec::new(),
            disk_read_buf: Vec::with_capacity(4096),
            disk_dio_scratch: AlignedBuf::default(),
            index_updates_buf: Vec::with_capacity(256),
        })
    }

    /// Remove a group from this storage (e.g., when group is deleted)
    pub fn remove_group(&self, group_id: u64) {
        if let Some(state) = self.groups.remove(group_id) {
            state.shard.shutdown();
        }
    }

    /// Get the list of active group IDs
    pub fn group_ids(&self) -> Vec<u64> {
        self.groups.group_ids()
    }

    /// Get the number of active shards (groups).
    /// Note: With per-group sharding, this returns the number of active groups.
    /// This method is provided for backwards compatibility.
    pub fn num_shards(&self) -> usize {
        self.groups.len()
    }
}

/// Trait for multiplexed storage that can provide per-group log storage
pub trait MultiplexedStorage<C: RaftTypeConfig> {
    type GroupLogStorage: RaftLogStorage<C>;

    fn get_log_storage(
        &self,
        group_id: u64,
    ) -> impl std::future::Future<Output = Self::GroupLogStorage> + Send;
    fn remove_group(&self, group_id: u64);
    fn group_ids(&self) -> Vec<u64>;
}

impl<C> MultiplexedStorage<C> for PerGroupLogStorage<C>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            Vote = openraft::impls::Vote<C>,
            Node = openraft::impls::BasicNode,
            Entry = openraft::impls::Entry<C>,
        > + 'static,
    C::Entry: Send + Sync + Clone + 'static,
    C::Vote: ToCodec<CodecVote>,
    C::D: ToCodec<RawBytes> + FromCodec<RawBytes>,
{
    type GroupLogStorage = GroupLogStorage<C>;

    async fn get_log_storage(&self, group_id: u64) -> Self::GroupLogStorage {
        // Panics if group creation fails - in practice this should be fallible
        PerGroupLogStorage::get_log_storage(self, group_id)
            .await
            .expect("Failed to create group storage")
    }

    fn remove_group(&self, group_id: u64) {
        PerGroupLogStorage::remove_group(self, group_id)
    }

    fn group_ids(&self) -> Vec<u64> {
        PerGroupLogStorage::group_ids(self)
    }
}

// Implement MultiRaftLogStorage trait for use with MultiRaftManager
impl<C> crate::multi::storage::MultiRaftLogStorage<C> for PerGroupLogStorage<C>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            Vote = openraft::impls::Vote<C>,
            Node = openraft::impls::BasicNode,
            Entry = openraft::impls::Entry<C>,
        > + 'static,
    C::Entry: Send + Sync + Clone + 'static,
    C::Vote: ToCodec<CodecVote>,
    C::D: ToCodec<RawBytes> + FromCodec<RawBytes>,
{
    type GroupLogStorage = GroupLogStorage<C>;

    async fn get_log_storage(&self, group_id: u64) -> Self::GroupLogStorage {
        // Panics if group creation fails - in practice this should be fallible
        PerGroupLogStorage::get_log_storage(self, group_id)
            .await
            .expect("Failed to create group storage")
    }

    fn remove_group(&self, group_id: u64) {
        PerGroupLogStorage::remove_group(self, group_id)
    }

    fn group_ids(&self) -> Vec<u64> {
        PerGroupLogStorage::group_ids(self)
    }
}

/// Log storage handle for a specific group within a PerGroupLogStorage.
///
/// Each group has its own dedicated shard (1:1 mapping).
/// Votes and log entries are written to the group's shard.
pub struct GroupLogStorage<C: RaftTypeConfig> {
    group_id: u64,
    config: StorageConfig,
    state: Arc<GroupState<C>>,
    /// Reusable buffer for encoding records (avoids allocation per write)
    encode_buf: Vec<u8>,
    /// Reusable buffer for encoding payloads before wrapping in record format
    payload_buf: Vec<u8>,
    /// Reusable buffer for reading records from disk (avoid allocation per read).
    disk_read_buf: Vec<u8>,
    /// Scratch buffer for direct-I/O fallback reads (reused).
    disk_dio_scratch: AlignedBuf,
    /// Reusable index update list for batched `append()` (avoid allocation per call).
    index_updates_buf: Vec<(u64, u64, u32)>,
}

impl<C: RaftTypeConfig> Clone for GroupLogStorage<C> {
    fn clone(&self) -> Self {
        Self {
            group_id: self.group_id,
            config: self.config.clone(),
            state: self.state.clone(),
            encode_buf: Vec::new(), // Each clone gets its own buffer
            payload_buf: Vec::new(),
            disk_read_buf: Vec::new(),
            disk_dio_scratch: AlignedBuf::default(),
            index_updates_buf: Vec::new(),
        }
    }
}

impl<C: RaftTypeConfig> GroupLogStorage<C> {
    /// Send the data in encode_buf to the shard writer.
    /// This takes the buffer and gives it to the writer task.
    async fn send_encoded(
        &mut self,
        entry_index_range: Option<(u64, u64)>,
        callbacks: Callbacks<C>,
        sync_immediately: bool,
    ) -> Result<Option<WritePlacement>, io::Error> {
        let data = std::mem::take(&mut self.encode_buf);
        let (buf, placement) = self
            .state
            .shard
            .write(data, entry_index_range, callbacks, sync_immediately)
            .await?;
        self.encode_buf = buf;
        Ok(placement)
    }

    /// Encode a record into the internal buffer and send it to the shard writer.
    /// The buffer is taken and a new empty buffer is received back from the channel
    /// response (amortizing allocation over time).
    async fn write_record(
        &mut self,
        record_type: RecordType,
        payload: &[u8],
    ) -> Result<(), io::Error> {
        encode_record_into(&mut self.encode_buf, record_type, self.group_id, payload);
        // Records without IOFlushed callback must be durable before returning.
        let _ = self.send_encoded(None, Callbacks::None, true).await?;
        Ok(())
    }

    /// Write a raw record (for libsql integration)
    pub async fn write_raw_record(
        &mut self,
        record_type: RecordType,
        payload: &[u8],
    ) -> Result<(), io::Error> {
        // External integration expects durable writes by default.
        self.write_record(record_type, payload).await
    }

    /// Get the group ID
    pub fn group_id(&self) -> u64 {
        self.group_id
    }

    async fn read_entry_from_disk(&mut self, index: u64) -> io::Result<Option<C::Entry>>
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
                Vote = openraft::impls::Vote<C>,
                Node = openraft::impls::BasicNode,
                Entry = openraft::impls::Entry<C>,
            >,
        C::D: FromCodec<RawBytes>,
    {
        let loc = self.state.log_index.get(index);

        let Some(loc) = loc else {
            return Ok(None);
        };

        let path =
            SegmentedLog::segment_path(&self.state.shard.group_dir, self.group_id, loc.segment_id);
        let file = open_segment_file(&path, true, false, false, false).await?;
        read_exact_at_reuse(
            &file,
            loc.offset,
            loc.len as usize,
            &mut self.disk_read_buf,
            &mut self.disk_dio_scratch,
        )
        .await?;
        let buf = &self.disk_read_buf[..loc.len as usize];

        if buf.len() < LENGTH_SIZE {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short record"));
        }

        let record_len = u32::from_le_bytes(buf[..LENGTH_SIZE].try_into().unwrap()) as usize;
        if record_len + LENGTH_SIZE != buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "record length mismatch",
            ));
        }

        let max_record_size = self
            .config
            .max_record_size
            .unwrap_or(self.config.segment_size);
        let parsed = validate_record(&buf[LENGTH_SIZE..], max_record_size as usize)?;
        if parsed.group_id != self.group_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "group_id mismatch",
            ));
        }
        if parsed.record_type != RecordType::Entry {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected Entry record",
            ));
        }

        let codec_entry = CodecEntry::<RawBytes>::decode_from_slice(parsed.payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let entry: openraft::impls::Entry<C> = openraft::impls::Entry::<C>::from_codec(codec_entry);
        Ok(Some(entry))
    }

    fn cache_window_start(&self) -> u64 {
        let max = self.config.max_cache_entries_per_group;
        if max == 0 {
            return self
                .state
                .last_index
                .load(Ordering::Relaxed)
                .saturating_add(1);
        }

        let last = self.state.last_index.load(Ordering::Relaxed);
        let first = self.state.first_index.load(Ordering::Relaxed);
        let keep = max as u64;
        last.saturating_sub(keep.saturating_sub(1)).max(first)
    }

    fn enforce_cache_window(&self) {
        let start = self.cache_window_start();
        let mut low = self.state.cache_low.load(Ordering::Relaxed);

        // If the window moved backwards (truncate/purge), reset the low watermark.
        if low > start {
            low = start;
        }

        while low < start {
            let _ = self.state.cache.remove(&low);
            low += 1;
        }

        self.state.cache_low.store(start, Ordering::Relaxed);
    }

    async fn compact_purged_segments(&self, purged_before: u64) -> io::Result<()>
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
                Vote = openraft::impls::Vote<C>,
                Node = openraft::impls::BasicNode,
                Entry = openraft::impls::Entry<C>,
            >,
        C::D: FromCodec<RawBytes>,
    {
        // Conservative compaction: only delete segments that contain ONLY Entry records
        // and whose max entry index is < purged_before. This avoids deleting vote/purge/truncate history.
        if purged_before == 0 {
            return Ok(());
        }

        let segments =
            SegmentedLog::list_segments_async(&self.state.shard.group_dir, self.group_id).await?;

        // Never delete the current segment (writer may be appending to it).
        let current_segment_id = segments.last().copied().unwrap_or(0);

        for segment_id in segments {
            if segment_id >= current_segment_id {
                continue;
            }

            let path =
                SegmentedLog::segment_path(&self.state.shard.group_dir, self.group_id, segment_id);
            let file = match open_segment_file(&path, true, false, false, false).await {
                Ok(f) => f,
                Err(_) => continue,
            };

            let file_len = file.metadata().await?.len();
            if file_len == 0 {
                // Empty old segment: safe to delete.
                if let Err(e) = maniac::fs::remove_file(&path).await {
                    tracing::warn!(
                        group_id = self.group_id,
                        segment_id,
                        error = %e,
                        "failed to delete empty segment during compaction"
                    );
                }
                continue;
            }

            let mut offset = 0u64;
            let mut read_buf: Vec<u8> = Vec::with_capacity(4096);
            let mut dio_scratch = AlignedBuf::default();

            let mut max_entry_index_in_segment: Option<u64> = None;
            let mut only_entries = true;

            while offset + LENGTH_SIZE as u64 <= file_len {
                // Read length - handle UnexpectedEof for padding bytes
                let len_bytes = match read_exact_at_reuse(
                    &file,
                    offset,
                    LENGTH_SIZE,
                    &mut read_buf,
                    &mut dio_scratch,
                )
                .await
                {
                    Ok(()) => &read_buf[..LENGTH_SIZE],
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        tracing::debug!("Reached EOF while reading length at offset {}", offset);
                        break;
                    }
                    Err(_) => break,
                };
                let record_len =
                    u32::from_le_bytes(len_bytes[..LENGTH_SIZE].try_into().unwrap()) as usize;
                let max_record_size =
                    self.config
                        .max_record_size
                        .unwrap_or(self.config.segment_size) as usize;

                // Handle zero-length record (padding)
                if record_len == 0 {
                    tracing::debug!(
                        "Found zero-length record at offset {}, treating as padding",
                        offset
                    );
                    break;
                }

                if record_len < (1 + GROUP_ID_SIZE + CRC64_SIZE) || record_len > max_record_size {
                    tracing::debug!(
                        "compact_purged_segments: Invalid record length {} at offset {}",
                        record_len,
                        offset
                    );
                    break;
                }
                if offset + LENGTH_SIZE as u64 + record_len as u64 > file_len {
                    tracing::debug!(
                        "compact_purged_segments: Record at offset {} would exceed file length",
                        offset
                    );
                    break;
                }

                if let Err(e) = read_exact_at_reuse(
                    &file,
                    offset + LENGTH_SIZE as u64,
                    record_len,
                    &mut read_buf,
                    &mut dio_scratch,
                )
                .await
                {
                    tracing::debug!(
                        "compact_purged_segments: Failed to read record at offset {}: {}",
                        offset,
                        e
                    );
                    break;
                }

                let parsed = match validate_record(&read_buf[..record_len], max_record_size) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::debug!(
                            "compact_purged_segments: Record validation failed at offset {}: {}",
                            offset,
                            e
                        );
                        break;
                    }
                };

                if parsed.group_id != self.group_id {
                    only_entries = false;
                    break;
                }

                match parsed.record_type {
                    RecordType::Entry => {
                        // Decode to find index.
                        let codec_entry = CodecEntry::<RawBytes>::decode_from_slice(parsed.payload)
                            .map_err(|e| {
                                io::Error::new(io::ErrorKind::InvalidData, e.to_string())
                            })?;
                        let entry: openraft::impls::Entry<C> =
                            openraft::impls::Entry::<C>::from_codec(codec_entry);
                        max_entry_index_in_segment = Some(
                            max_entry_index_in_segment
                                .map(|v| v.max(entry.log_id.index))
                                .unwrap_or(entry.log_id.index),
                        );
                    }
                    _ => {
                        only_entries = false;
                        break;
                    }
                }

                offset += LENGTH_SIZE as u64 + record_len as u64;
            }

            if only_entries {
                let should_delete = if let Some(max_idx) = max_entry_index_in_segment {
                    max_idx < purged_before
                } else {
                    // No entries, no non-entry records (e.g. empty-ish): safe to delete.
                    true
                };

                if should_delete {
                    if let Err(e) = maniac::fs::remove_file(&path).await {
                        tracing::warn!(
                            group_id = self.group_id,
                            segment_id,
                            max_entry_index = ?max_entry_index_in_segment,
                            purged_before,
                            error = %e,
                            "failed to delete purged segment during compaction"
                        );
                    } else {
                        tracing::debug!(
                            group_id = self.group_id,
                            segment_id,
                            max_entry_index = ?max_entry_index_in_segment,
                            "deleted purged segment"
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

impl<C> RaftLogReader<C> for GroupLogStorage<C>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            Vote = openraft::impls::Vote<C>,
            Node = openraft::impls::BasicNode,
            Entry = openraft::impls::Entry<C>,
        >,
    C::Entry: Clone + 'static,
    C::D: FromCodec<RawBytes>,
{
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + std::fmt::Debug + Send>(
        &mut self,
        range: RB,
    ) -> Result<Vec<C::Entry>, io::Error> {
        use std::ops::Bound;

        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(&n) => n + 1,
            Bound::Excluded(&n) => n,
            Bound::Unbounded => u64::MAX,
        };

        // Pre-size the vec based on expected range (cap at reasonable size)
        let expected_len = end.saturating_sub(start).min(1024) as usize;
        let mut entries = Vec::with_capacity(expected_len);

        for idx in start..end {
            if let Some(entry) = self.state.cache.get(&idx) {
                entries.push(entry.value().as_ref().clone());
                continue;
            }

            if let Some(entry) = self.read_entry_from_disk(idx).await? {
                if idx >= self.cache_window_start() {
                    self.state.cache.insert(idx, Arc::new(entry.clone()));
                    self.enforce_cache_window();
                }
                entries.push(entry);
            } else {
                break;
            }
        }

        Ok(entries)
    }

    async fn read_vote(&mut self) -> Result<Option<C::Vote>, io::Error> {
        Ok(self.state.vote.load())
    }
}

impl<C> RaftLogStorage<C> for GroupLogStorage<C>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
            Vote = openraft::impls::Vote<C>,
            Node = openraft::impls::BasicNode,
            Entry = openraft::impls::Entry<C>,
        >,
    C::Entry: Send + Sync + Clone + 'static,
    C::Vote: ToCodec<CodecVote>,
    C::D: ToCodec<RawBytes> + FromCodec<RawBytes>,
{
    type LogReader = Self;

    async fn get_log_state(&mut self) -> Result<LogState<C>, io::Error> {
        let last_log_id = self.state.last_log_id.load();
        let last_purged_log_id = self.state.last_purged_log_id.load();
        Ok(LogState {
            last_purged_log_id,
            last_log_id,
        })
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn save_vote(&mut self, vote: &C::Vote) -> Result<(), io::Error> {
        // Store atomically in memory
        self.state.vote.store(Some(vote));

        // Persist to log file - encode into payload buffer, then build record
        let codec_vote = vote.to_codec();
        self.payload_buf.clear();
        codec_vote
            .encode_into(&mut self.payload_buf)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Build the record directly from payload_buf contents
        encode_record_into(
            &mut self.encode_buf,
            RecordType::Vote,
            self.group_id,
            &self.payload_buf,
        );
        // Vote must be durably persisted before returning.
        let _ = self.send_encoded(None, Callbacks::None, true).await?;

        Ok(())
    }

    async fn append<I>(&mut self, entries: I, callback: IOFlushed<C>) -> Result<(), io::Error>
    where
        I: IntoIterator<Item = C::Entry> + Send,
    {
        use openraft::entry::RaftEntry;

        let mut last_log_id = None;

        self.encode_buf.clear();
        self.index_updates_buf.clear();
        for entry in entries {
            let log_id = entry.log_id();
            let index = log_id.index;

            // Convert entry to codec type and serialize - reuse payload buffer
            let codec_entry: CodecEntry<RawBytes> = entry.to_codec();
            self.payload_buf.clear();
            codec_entry
                .encode_into(&mut self.payload_buf)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

            // Append record to the batch buffer.
            let record_start = self.encode_buf.len() as u64;
            let record_len = append_record_into(
                &mut self.encode_buf,
                RecordType::Entry,
                self.group_id,
                &self.payload_buf,
            );
            self.index_updates_buf
                .push((index, record_start, record_len as u32));

            // Store in cache
            self.state.cache.insert(index, Arc::new(entry));

            // Update last index
            self.state.last_index.fetch_max(index, Ordering::Relaxed);
            last_log_id = Some(log_id);
        }

        // Update last log id
        if let Some(ref lid) = last_log_id {
            self.state.last_log_id.store(Some(lid));
        }

        // Send the entire batch as a single write request and attach the callback.
        // The callback is completed only after the next durable fsync that covers this write.
        let entry_index_range = self
            .index_updates_buf
            .first()
            .zip(self.index_updates_buf.last())
            .map(|(a, b)| (a.0, b.0));

        let placement = self
            .send_encoded(entry_index_range, Callbacks::One(callback), false)
            .await?;

        if let Some(pl) = placement {
            for (entry_index, record_start, record_len) in self.index_updates_buf.drain(..) {
                self.state.log_index.insert(
                    entry_index,
                    LogLocation {
                        segment_id: pl.segment_id,
                        offset: pl.offset + record_start,
                        len: record_len,
                    },
                )?;
            }
        }
        self.enforce_cache_window();

        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<C>) -> Result<(), io::Error> {
        let index = log_id.index;

        // Remove entries after the truncation point from cache
        let last = self.state.last_index.load(Ordering::Relaxed);
        for idx in (index + 1)..=last {
            self.state.cache.remove(&idx);
        }
        self.state.log_index.truncate_from(index.saturating_add(1));

        // Update last index
        self.state.last_index.store(index, Ordering::Relaxed);
        self.state.last_log_id.store(Some(&log_id));

        // Persist truncation marker with CRC (store full LogId for recovery correctness).
        let codec_log_id: CodecLogId = log_id.to_codec();
        self.payload_buf.clear();
        codec_log_id
            .encode_into(&mut self.payload_buf)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        encode_record_into(
            &mut self.encode_buf,
            RecordType::Truncate,
            self.group_id,
            &self.payload_buf,
        );
        let _ = self.send_encoded(None, Callbacks::None, true).await?;
        self.enforce_cache_window();

        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<C>) -> Result<(), io::Error> {
        let index = log_id.index;

        // Remove entries up to and including the purge point from cache
        let first = self.state.first_index.load(Ordering::Relaxed);
        for idx in first..=index {
            self.state.cache.remove(&idx);
        }
        self.state.log_index.purge_to(index);

        // Update first index and last_purged_log_id
        self.state.first_index.store(index + 1, Ordering::Relaxed);
        self.state.last_purged_log_id.store(Some(&log_id));

        // Persist purge marker with CRC - reuse payload buffer
        let codec_log_id: CodecLogId = log_id.to_codec();
        self.payload_buf.clear();
        codec_log_id
            .encode_into(&mut self.payload_buf)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Build the record directly from payload_buf contents
        encode_record_into(
            &mut self.encode_buf,
            RecordType::Purge,
            self.group_id,
            &self.payload_buf,
        );
        // Purge must be durably persisted before returning.
        let _ = self.send_encoded(None, Callbacks::None, true).await?;
        self.enforce_cache_window();
        // Best-effort: compact fully-purged segments.
        let _ = self.compact_purged_segments(index.saturating_add(1)).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openraft::RaftTypeConfig;
    use openraft::type_config::async_runtime::Oneshot;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestData(Vec<u8>);

    impl std::fmt::Display for TestData {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestData(len={})", self.0.len())
        }
    }

    impl ToCodec<RawBytes> for TestData {
        fn to_codec(&self) -> RawBytes {
            RawBytes(self.0.clone())
        }
    }

    impl FromCodec<RawBytes> for TestData {
        fn from_codec(codec: RawBytes) -> Self {
            Self(codec.0)
        }
    }

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.segment_size, DEFAULT_SEGMENT_SIZE);
        assert_eq!(config.fsync_interval, Some(Duration::from_millis(100)));
        assert_eq!(config.max_cache_entries_per_group, 10000);
    }

    #[test]
    fn test_sharded_config_conversion() {
        let sharded = ShardedStorageConfig {
            base_dir: PathBuf::from("/tmp/test"),
            num_shards: 16, // Should be ignored
            segment_size: 32 * 1024 * 1024,
            max_record_size: Some(2 * 1024 * 1024),
            fsync_interval: None,
            max_cache_entries_per_group: 5000,
        };

        let config: StorageConfig = sharded.into();
        assert_eq!(*config.base_dir, PathBuf::from("/tmp/test"));
        assert_eq!(config.segment_size, 32 * 1024 * 1024);
        assert_eq!(config.fsync_interval, None);
        assert_eq!(config.max_cache_entries_per_group, 5000);
    }

    #[test]
    fn test_encode_decode_record() {
        let record_type = RecordType::Entry;
        let group_id = 42u64;
        let payload = b"test payload data";

        let encoded = encode_record(record_type, group_id, payload);

        // Verify length field
        let len = u32::from_le_bytes(encoded[..4].try_into().unwrap());
        assert_eq!(len as usize, encoded.len() - LENGTH_SIZE);

        // Validate the record
        let record_data = &encoded[LENGTH_SIZE..];
        let parsed = validate_record(record_data, DEFAULT_MAX_RECORD_SIZE as usize)
            .expect("should validate");

        assert_eq!(parsed.record_type, RecordType::Entry);
        assert_eq!(parsed.group_id, 42);
        assert_eq!(parsed.payload, payload);
    }

    #[test]
    fn test_record_crc_corruption() {
        let encoded = encode_record(RecordType::Vote, 1, b"vote data");

        // Corrupt a byte in the payload
        let mut corrupted = encoded.clone();
        corrupted[10] ^= 0xFF;

        let record_data = &corrupted[LENGTH_SIZE..];
        assert!(validate_record(record_data, DEFAULT_MAX_RECORD_SIZE as usize).is_err());
    }

    #[test]
    fn test_libsql_record_types() {
        // Test that libsql record types can be encoded/decoded
        for record_type in [
            RecordType::WalFrame,
            RecordType::Changeset,
            RecordType::Checkpoint,
            RecordType::WalMetadata,
        ] {
            let encoded = encode_record(record_type, 123, b"libsql data");
            let record_data = &encoded[LENGTH_SIZE..];
            let parsed = validate_record(record_data, DEFAULT_MAX_RECORD_SIZE as usize)
                .expect("should validate");
            assert_eq!(parsed.record_type, record_type);
            assert_eq!(parsed.group_id, 123);
        }
    }

    #[test]
    fn test_atomic_vote() {
        let vote = AtomicVote::new();

        let loaded: Option<
            openraft::impls::Vote<crate::multi::type_config::ManiacRaftTypeConfig<String, String>>,
        > = vote.load();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_atomic_log_id() {
        let log_id = AtomicLogId::new();

        let loaded: Option<LogId<crate::multi::type_config::ManiacRaftTypeConfig<String, String>>> =
            log_id.load();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_fsync_none_callback_completes() {
        // With fsync_interval=None, callbacks must complete successfully after a durable fsync.
        use crate::multi::type_config::ManiacRaftTypeConfig;
        type C = ManiacRaftTypeConfig<TestData, ()>;

        crate::multi::test_support::run_async(async {
            let dir = TempDir::new().unwrap();
            let cfg = StorageConfig::new(dir.path()).with_fsync_interval(None);
            let storage = PerGroupLogStorage::<C>::new(cfg).await.unwrap();
            let mut group = storage.get_log_storage(1).await.unwrap();

            let (tx, rx) = <<C as RaftTypeConfig>::AsyncRuntime as openraft::type_config::async_runtime::AsyncRuntime>::Oneshot::channel::<
                Result<(), io::Error>,
            >();
            let cb = IOFlushed::<C>::signal(tx);

            group
                .append(Vec::<openraft::impls::Entry<C>>::new(), cb)
                .await
                .unwrap();

            assert!(rx.await.unwrap().is_ok());
        });
    }

    #[test]
    fn test_recovery_replays_vote_and_entries() {
        use crate::multi::type_config::ManiacRaftTypeConfig;
        type C = ManiacRaftTypeConfig<TestData, ()>;

        crate::multi::test_support::run_async(async {
            let dir = TempDir::new().unwrap();
            // Tests often use tmpfs which may not support MDBX properly
            // The manifest will automatically fall back to in-memory mode
            let cfg = StorageConfig::new(dir.path())
                .with_fsync_interval(None)
                .with_max_cache_entries(1);

            // Write some state.
            {
                let storage = PerGroupLogStorage::<C>::new(cfg.clone()).await.unwrap();
                let mut group = storage.get_log_storage(42).await.unwrap();

                let vote = openraft::impls::Vote::<C> {
                    leader_id: openraft::impls::leader_id_adv::LeaderId {
                        term: 3,
                        node_id: 7,
                    },
                    committed: true,
                };
                group.save_vote(&vote).await.unwrap();

                let entry = openraft::impls::Entry::<C> {
                    log_id: LogId {
                        leader_id: openraft::impls::leader_id_adv::LeaderId {
                            term: 3,
                            node_id: 7,
                        },
                        index: 1,
                    },
                    payload: openraft::EntryPayload::Normal(TestData(b"hello".to_vec())),
                };

                let (tx, rx) = <<C as RaftTypeConfig>::AsyncRuntime as openraft::type_config::async_runtime::AsyncRuntime>::Oneshot::channel::<
                    Result<(), io::Error>,
                >();
                let cb = IOFlushed::<C>::signal(tx);
                group.append(vec![entry], cb).await.unwrap();
                let _ = rx.await;

                // Ensure writer task releases file handles before reopening.
                storage.stop();
                maniac::time::sleep(Duration::from_millis(50)).await;
            }

            // Re-open and verify recovery.
            {
                let storage = PerGroupLogStorage::<C>::new(cfg).await.unwrap();
                let mut group = storage.get_log_storage(42).await.unwrap();

                let recovered_vote = group.read_vote().await.unwrap().unwrap();
                assert_eq!(recovered_vote.leader_id.term, 3);
                assert_eq!(recovered_vote.leader_id.node_id, 7);
                assert!(recovered_vote.committed);

                let entries: Vec<openraft::impls::Entry<ManiacRaftTypeConfig<TestData, ()>>> =
                    group.try_get_log_entries(1u64..=1u64).await.unwrap();
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].log_id.index, 1);
            }
        });
    }

    #[test]
    fn test_record_validation_edge_cases() {
        // Test minimum valid record size
        let min_record = encode_record(RecordType::Vote, 1, &[]);
        let record_data = &min_record[LENGTH_SIZE..];
        let parsed = validate_record(record_data, DEFAULT_MAX_RECORD_SIZE as usize)
            .expect("should validate");
        assert_eq!(parsed.payload.len(), 0);

        // Test record too short (missing CRC)
        let mut short = vec![0u8; 16]; // type(1) + group_id(3) + payload(0) = 4, but we need 8 more for CRC
        short[0] = RecordType::Vote as u8;
        assert!(validate_record(&short, DEFAULT_MAX_RECORD_SIZE as usize).is_err());

        // Test record with invalid type
        let mut invalid_type = encode_record(RecordType::Vote, 1, b"data");
        invalid_type[LENGTH_SIZE] = 0xFF; // Invalid record type
        let record_data = &invalid_type[LENGTH_SIZE..];
        assert!(validate_record(record_data, DEFAULT_MAX_RECORD_SIZE as usize).is_err());
    }

    #[test]
    fn test_record_type_conversion() {
        // Test valid conversions
        assert_eq!(RecordType::try_from(0x01).unwrap(), RecordType::Vote);
        assert_eq!(RecordType::try_from(0x02).unwrap(), RecordType::Entry);
        assert_eq!(RecordType::try_from(0x03).unwrap(), RecordType::Truncate);
        assert_eq!(RecordType::try_from(0x04).unwrap(), RecordType::Purge);
        assert_eq!(RecordType::try_from(0x10).unwrap(), RecordType::WalFrame);
        assert_eq!(RecordType::try_from(0x11).unwrap(), RecordType::Changeset);
        assert_eq!(RecordType::try_from(0x12).unwrap(), RecordType::Checkpoint);
        assert_eq!(RecordType::try_from(0x13).unwrap(), RecordType::WalMetadata);

        // Test invalid conversion
        assert!(RecordType::try_from(0x99).is_err());
    }

    #[test]
    fn test_encode_record_various_sizes() {
        // Test small payload
        let small = encode_record(RecordType::Entry, 1, b"a");
        assert!(validate_record(&small[LENGTH_SIZE..], DEFAULT_MAX_RECORD_SIZE as usize).is_ok());

        // Test large payload
        let large_payload = vec![0x42u8; 10000];
        let large = encode_record(RecordType::Entry, 1, &large_payload);
        let parsed =
            validate_record(&large[LENGTH_SIZE..], DEFAULT_MAX_RECORD_SIZE as usize).unwrap();
        assert_eq!(parsed.payload, large_payload.as_slice());

        // Test zero-length payload
        let empty = encode_record(RecordType::Entry, 1, &[]);
        let parsed =
            validate_record(&empty[LENGTH_SIZE..], DEFAULT_MAX_RECORD_SIZE as usize).unwrap();
        assert_eq!(parsed.payload.len(), 0);
    }

    #[test]
    fn test_crc_calculation_consistency() {
        let payload = b"test data for CRC";
        let encoded1 = encode_record(RecordType::Entry, 42, payload);
        let encoded2 = encode_record(RecordType::Entry, 42, payload);

        // Same input should produce same CRC
        assert_eq!(encoded1, encoded2);

        // Different group_id should produce different CRC
        let encoded3 = encode_record(RecordType::Entry, 43, payload);
        assert_ne!(encoded1, encoded3);

        // Different record type should produce different CRC
        let encoded4 = encode_record(RecordType::Vote, 42, payload);
        assert_ne!(encoded1, encoded4);
    }
}
