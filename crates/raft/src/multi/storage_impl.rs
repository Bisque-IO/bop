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
//! 6. Rebuilds in-memory state from valid records

use crate::multi::codec::{Encode, Entry as CodecEntry, RawBytes, ToCodec, Vote as CodecVote};
use crc64fast_nvme::Digest;
use dashmap::DashMap;
use maniac::fs::File;
use maniac::io::AsyncWriteRentExt;
use openraft::{
    storage::{IOFlushed, RaftLogReader, RaftLogStorage},
    LogId, LogState, RaftTypeConfig,
};
use std::io;
use std::io::{Read, Seek, Write};
use std::ops::RangeBounds;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
/// Minimum record size: len(4) + type(1) + group_id(8) + crc(8) = 21 bytes
#[allow(dead_code)]
const MIN_RECORD_SIZE: usize = LENGTH_SIZE + 1 + 8 + CRC64_SIZE;
/// Default segment size: 64MB
pub const DEFAULT_SEGMENT_SIZE: u64 = 64 * 1024 * 1024;

/// Header size: len(4) + type(1) + group_id(8) = 13 bytes
const HEADER_SIZE: usize = LENGTH_SIZE + 1 + 8;

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
    // Total record size (excluding the len field itself)
    let record_len = 1 + 8 + payload.len() + CRC64_SIZE; // type + group_id + payload + crc
    let total_size = LENGTH_SIZE + record_len;

    // Ensure capacity without over-allocating
    buf.clear();
    buf.reserve(total_size);

    // Build header: [len][type][group_id]
    let header: [u8; HEADER_SIZE] = {
        let mut h = [0u8; HEADER_SIZE];
        h[0..4].copy_from_slice(&(record_len as u32).to_le_bytes());
        h[4] = record_type as u8;
        h[5..13].copy_from_slice(&group_id.to_le_bytes());
        h
    };

    // Compute CRC64 incrementally over [type + group_id + payload]
    let mut digest = Digest::new();
    digest.write(&header[LENGTH_SIZE..]); // type + group_id (9 bytes)
    digest.write(payload);
    let crc = digest.sum64();

    // Write all components
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
fn validate_record(data: &[u8]) -> io::Result<ParsedRecord<'_>> {
    // Minimum: type(1) + group_id(8) + crc(8) = 17 bytes
    if data.len() < 1 + 8 + CRC64_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "record too short",
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
    let group_id = u64::from_le_bytes(data[1..9].try_into().unwrap());
    let payload = &data[9..payload_end];

    Ok(ParsedRecord {
        record_type,
        group_id,
        payload,
    })
}

/// Configuration for per-group log storage
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Base directory for storage files
    pub base_dir: PathBuf,
    /// Maximum segment size in bytes. When exceeded, a new segment is created.
    /// Default: 64MB
    pub segment_size: u64,
    /// Interval for fsync per group. If None, fsync after every write.
    /// If Some(duration), fsync on interval only if dirty.
    pub fsync_interval: Option<Duration>,
    /// Maximum entries to keep in memory cache per group
    pub max_cache_entries_per_group: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("./raft-data"),
            segment_size: DEFAULT_SEGMENT_SIZE,
            fsync_interval: Some(Duration::from_millis(100)),
            max_cache_entries_per_group: 10000,
        }
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
    pub fsync_interval: Option<Duration>,
    pub max_cache_entries_per_group: usize,
}

impl Default for ShardedStorageConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("./raft-data"),
            num_shards: 8, // Ignored
            segment_size: DEFAULT_SEGMENT_SIZE,
            fsync_interval: Some(Duration::from_millis(100)),
            max_cache_entries_per_group: 10000,
        }
    }
}

impl From<ShardedStorageConfig> for StorageConfig {
    fn from(cfg: ShardedStorageConfig) -> Self {
        Self {
            base_dir: cfg.base_dir,
            segment_size: cfg.segment_size,
            fsync_interval: cfg.fsync_interval,
            max_cache_entries_per_group: cfg.max_cache_entries_per_group,
        }
    }
}

// Keep the old config as an alias for compatibility
pub type MultiplexedStorageConfig = ShardedStorageConfig;

/// Per-group state within the storage
struct GroupState<C: RaftTypeConfig> {
    /// In-memory log cache: index -> entry
    cache: DashMap<u64, Arc<C::Entry>>,
    /// Current vote for this group (atomic)
    vote: AtomicVote,
    /// Log state
    first_index: AtomicU64,
    last_index: AtomicU64,
    last_log_id: AtomicLogId,
    /// Last purged log id (for log compaction tracking)
    last_purged_log_id: AtomicLogId,
    /// The shard for this group
    shard: Arc<ShardState<C>>,
}

impl<C: RaftTypeConfig + 'static> GroupState<C> {
    fn new(group_id: u64, config: &StorageConfig) -> io::Result<Self> {
        let shard = ShardState::new(group_id, config)?;
        Ok(Self {
            cache: DashMap::new(),
            vote: AtomicVote::new(),
            first_index: AtomicU64::new(0),
            last_index: AtomicU64::new(0),
            last_log_id: AtomicLogId::new(),
            last_purged_log_id: AtomicLogId::new(),
            shard: Arc::new(shard),
        })
    }
}

/// Atomic storage for Vote packed into 128 bits
/// Layout: high=[term: 64 bits], low=[node_id: 62 bits][committed: 1 bit][valid: 1 bit]
struct AtomicVote {
    high: std::sync::atomic::AtomicU64,
    low: std::sync::atomic::AtomicU64,
}

impl AtomicVote {
    const VALID_BIT: u64 = 1;
    const COMMITTED_BIT: u64 = 2;
    const NODE_ID_SHIFT: u32 = 2;

    fn new() -> Self {
        Self {
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
        match vote {
            Some(v) => {
                let high = v.leader_id.term;
                let low = (v.leader_id.node_id << Self::NODE_ID_SHIFT)
                    | if v.committed { Self::COMMITTED_BIT } else { 0 }
                    | Self::VALID_BIT;
                self.high.store(high, Ordering::Relaxed);
                self.low.store(low, Ordering::Release);
            }
            None => {
                self.low.store(0, Ordering::Release);
            }
        }
    }

    fn load<C>(&self) -> Option<openraft::impls::Vote<C>>
    where
        C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
        >,
    {
        let low = self.low.load(Ordering::Acquire);
        if (low & Self::VALID_BIT) == 0 {
            return None;
        }

        let high = self.high.load(Ordering::Relaxed);
        let term = high;
        let node_id = low >> Self::NODE_ID_SHIFT;
        let committed = (low & Self::COMMITTED_BIT) != 0;

        Some(openraft::impls::Vote {
            leader_id: openraft::impls::leader_id_adv::LeaderId { term, node_id },
            committed,
        })
    }
}

/// Atomic storage for LogId packed into 128 bits
/// Layout: high=[term: 40 bits][node_id: 24 bits], low=[index: 63 bits][valid: 1 bit]
struct AtomicLogId {
    high: std::sync::atomic::AtomicU64,
    low: std::sync::atomic::AtomicU64,
}

impl AtomicLogId {
    const VALID_BIT: u64 = 1;
    const INDEX_SHIFT: u32 = 1;
    const TERM_SHIFT: u32 = 24;

    fn new() -> Self {
        Self {
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
        match log_id {
            Some(lid) => {
                let high = (lid.leader_id.term << Self::TERM_SHIFT) | (lid.leader_id.node_id & 0xFF_FFFF);
                let low = (lid.index << Self::INDEX_SHIFT) | Self::VALID_BIT;
                self.high.store(high, Ordering::Relaxed);
                self.low.store(low, Ordering::Release);
            }
            None => {
                self.low.store(0, Ordering::Release);
            }
        }
    }

    fn load<C>(&self) -> Option<LogId<C>>
    where
        C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
        >,
    {
        let low = self.low.load(Ordering::Acquire);
        if (low & Self::VALID_BIT) == 0 {
            return None;
        }

        let high = self.high.load(Ordering::Relaxed);
        let term = high >> Self::TERM_SHIFT;
        let node_id = high & 0xFF_FFFF;
        let index = low >> Self::INDEX_SHIFT;

        Some(LogId {
            leader_id: openraft::impls::leader_id_adv::LeaderId { term, node_id },
            index,
        })
    }
}

/// Write request sent to shard writer task
enum WriteRequest<C: RaftTypeConfig> {
    /// Write data to the log
    Write {
        data: Vec<u8>,
        /// If true, fsync immediately after write
        sync_immediately: bool,
    },
    /// Queue an IOFlushed callback
    QueueCallback(IOFlushed<C>),
    /// Shutdown the writer task
    Shutdown,
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
}

impl SegmentedLog {
    /// Create or recover a segmented log for a group
    fn recover(group_dir: PathBuf, group_id: u64, segment_size: u64) -> io::Result<Self> {
        std::fs::create_dir_all(&group_dir)?;

        // Find existing segments
        let segments = Self::list_segments(&group_dir, group_id)?;

        let (current_segment_id, current_file, current_size) = if segments.is_empty() {
            // No existing segments - create first one
            let segment_id = 0u64;
            let path = Self::segment_path(&group_dir, group_id, segment_id);
            let std_file = std::fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(&path)?;
            let file = File::from_std(std_file)?;
            (segment_id, file, 0)
        } else {
            // Recover from existing segments
            let mut last_valid_segment_id = 0u64;
            let mut total_valid_bytes = 0u64;

            for segment_id in &segments {
                let path = Self::segment_path(&group_dir, group_id, *segment_id);
                let valid_bytes = Self::recover_segment(&path)?;

                if valid_bytes > 0 {
                    last_valid_segment_id = *segment_id;
                    total_valid_bytes = valid_bytes;
                }
            }

            // Open the last valid segment for appending
            let path = Self::segment_path(&group_dir, group_id, last_valid_segment_id);
            let std_file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)?;

            // Seek to end of valid data
            let mut file_handle = std_file;
            file_handle.seek(io::SeekFrom::Start(total_valid_bytes))?;
            // Truncate any data after valid point
            file_handle.set_len(total_valid_bytes)?;

            let file = File::from_std(file_handle)?;
            (last_valid_segment_id, file, total_valid_bytes)
        };

        Ok(Self {
            group_dir,
            group_id,
            segment_size,
            current_segment_id,
            current_file,
            current_size,
        })
    }

    /// List segment IDs for a group, sorted by ID
    fn list_segments(group_dir: &PathBuf, group_id: u64) -> io::Result<Vec<u64>> {
        let prefix = format!("group-{}-", group_id);
        let suffix = ".log";

        let mut segments = Vec::new();

        if let Ok(entries) = std::fs::read_dir(group_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if name_str.starts_with(&prefix) && name_str.ends_with(suffix) {
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

    /// Recover a segment file, validating all records
    /// Returns the byte offset of the last valid record's end
    fn recover_segment(path: &PathBuf) -> io::Result<u64> {
        let mut file = std::fs::File::open(path)?;
        let file_len = file.metadata()?.len();

        if file_len == 0 {
            return Ok(0);
        }

        let mut offset = 0u64;
        let mut len_buf = [0u8; LENGTH_SIZE];

        loop {
            // Check if we have enough bytes for a length field
            if offset + LENGTH_SIZE as u64 > file_len {
                // Partial length field - truncate here
                break;
            }

            // Read length
            file.seek(io::SeekFrom::Start(offset))?;
            if file.read_exact(&mut len_buf).is_err() {
                break;
            }

            let record_len = u32::from_le_bytes(len_buf) as u64;

            // Sanity check on length
            if record_len < (1 + 8 + CRC64_SIZE) as u64 || record_len > 100 * 1024 * 1024 {
                // Invalid length - truncate here
                break;
            }

            // Check if we have enough bytes for the full record
            if offset + LENGTH_SIZE as u64 + record_len > file_len {
                // Partial record - truncate here
                break;
            }

            // Read the record data (everything after length)
            let mut record_data = vec![0u8; record_len as usize];
            if file.read_exact(&mut record_data).is_err() {
                break;
            }

            // Validate CRC
            if validate_record(&record_data).is_err() {
                // Invalid CRC - truncate here
                break;
            }

            // Record is valid, move to next
            offset += LENGTH_SIZE as u64 + record_len;
        }

        Ok(offset)
    }

    /// Write data to the current segment, rotating if needed
    async fn write(&mut self, data: Vec<u8>) -> io::Result<()> {
        let data_len = data.len() as u64;

        // Check if we need to rotate to a new segment
        if self.current_size + data_len > self.segment_size {
            self.rotate().await?;
        }

        // Write to current segment
        let (result, _) = self.current_file.write_all(data).await;
        result?;

        self.current_size += data_len;
        Ok(())
    }

    /// Rotate to a new segment
    async fn rotate(&mut self) -> io::Result<()> {
        // Sync current segment before rotating
        self.current_file.sync_all().await?;

        // Create new segment
        self.current_segment_id += 1;
        let path = Self::segment_path(&self.group_dir, self.group_id, self.current_segment_id);

        let std_file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;

        self.current_file = File::from_std(std_file)?;
        self.current_size = 0;

        Ok(())
    }

    /// Sync the current segment
    async fn sync(&self) -> io::Result<()> {
        self.current_file.sync_all().await
    }
}

/// Per-group shard state
struct ShardState<C: RaftTypeConfig> {
    /// Channel to send write requests to the shard writer task
    write_tx: flume::Sender<WriteRequest<C>>,
    /// Whether the shard is running
    #[allow(dead_code)]
    running: Arc<AtomicBool>,
}

impl<C: RaftTypeConfig + 'static> ShardState<C> {
    /// Create a new shard with its writer task, performing recovery
    fn new(group_id: u64, config: &StorageConfig) -> io::Result<Self> {
        // Group-specific directory
        let group_dir = config.base_dir.join(format!("{}", group_id));

        // Recover the segmented log synchronously during initialization
        let segmented_log = SegmentedLog::recover(group_dir, group_id, config.segment_size)?;

        // Create channel for write requests (bounded for backpressure)
        let (write_tx, write_rx) = flume::bounded::<WriteRequest<C>>(1024);

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let fsync_interval = config.fsync_interval;

        // Spawn the writer task
        let _ = maniac::spawn(async move {
            Self::writer_loop(segmented_log, write_rx, fsync_interval, running_clone).await;
        });

        Ok(Self { write_tx, running })
    }

    /// Writer task loop - owns the segmented log, no mutex needed
    async fn writer_loop(
        mut log: SegmentedLog,
        write_rx: flume::Receiver<WriteRequest<C>>,
        fsync_interval: Option<Duration>,
        running: Arc<AtomicBool>,
    ) {
        let mut dirty = false;
        let mut pending_callbacks: Vec<IOFlushed<C>> = Vec::new();
        let mut last_sync = std::time::Instant::now();

        loop {
            // Calculate timeout for next fsync check
            let timeout = fsync_interval
                .map(|interval| {
                    let elapsed = last_sync.elapsed();
                    if elapsed >= interval {
                        Duration::ZERO
                    } else {
                        interval - elapsed
                    }
                })
                .unwrap_or(Duration::from_secs(3600));

            // Try to receive with timeout
            let request = if timeout.is_zero() {
                write_rx.try_recv().ok()
            } else {
                match maniac::time::timeout(timeout, async {
                    write_rx.recv_async().await.ok()
                })
                .await
                {
                    Ok(req) => req,
                    Err(_) => None,
                }
            };

            match request {
                Some(WriteRequest::Write {
                    data,
                    sync_immediately,
                }) => {
                    if let Err(e) = log.write(data).await {
                        eprintln!("Shard write error: {}", e);
                        continue;
                    }
                    dirty = true;

                    if sync_immediately {
                        if let Err(e) = log.sync().await {
                            eprintln!("Shard sync error: {}", e);
                        }
                        dirty = false;
                        last_sync = std::time::Instant::now();
                    }
                }
                Some(WriteRequest::QueueCallback(callback)) => {
                    pending_callbacks.push(callback);
                }
                Some(WriteRequest::Shutdown) => {
                    if dirty {
                        let _ = log.sync().await;
                    }
                    for callback in pending_callbacks.drain(..) {
                        callback.io_completed(Ok(()));
                    }
                    running.store(false, Ordering::Release);
                    return;
                }
                None => {}
            }

            // Check if it's time for interval-based fsync
            if let Some(interval) = fsync_interval {
                if last_sync.elapsed() >= interval && (dirty || !pending_callbacks.is_empty()) {
                    // Drain any pending requests first (non-blocking batch)
                    while let Ok(req) = write_rx.try_recv() {
                        match req {
                            WriteRequest::Write {
                                data,
                                sync_immediately: _,
                            } => {
                                if log.write(data).await.is_ok() {
                                    dirty = true;
                                }
                            }
                            WriteRequest::QueueCallback(callback) => {
                                pending_callbacks.push(callback);
                            }
                            WriteRequest::Shutdown => {
                                if dirty {
                                    let _ = log.sync().await;
                                }
                                for callback in pending_callbacks.drain(..) {
                                    callback.io_completed(Ok(()));
                                }
                                running.store(false, Ordering::Release);
                                return;
                            }
                        }
                    }

                    // Now sync
                    if dirty {
                        let result = log.sync().await;
                        dirty = false;
                        last_sync = std::time::Instant::now();

                        let (is_ok, err_kind, err_msg) = match &result {
                            Ok(_) => (true, io::ErrorKind::Other, String::new()),
                            Err(e) => (false, e.kind(), e.to_string()),
                        };

                        for callback in pending_callbacks.drain(..) {
                            let io_result = if is_ok {
                                Ok(())
                            } else {
                                Err(io::Error::new(err_kind, err_msg.clone()))
                            };
                            callback.io_completed(io_result);
                        }
                    } else if !pending_callbacks.is_empty() {
                        for callback in pending_callbacks.drain(..) {
                            callback.io_completed(Ok(()));
                        }
                        last_sync = std::time::Instant::now();
                    }
                }
            }
        }
    }

    /// Send a write request to the shard
    async fn write(&self, data: Vec<u8>, sync_immediately: bool) -> io::Result<()> {
        self.write_tx
            .send_async(WriteRequest::Write {
                data,
                sync_immediately,
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "shard writer task closed"))
    }

    /// Queue an IOFlushed callback
    fn queue_callback(&self, callback: IOFlushed<C>) {
        let _ = self.write_tx.try_send(WriteRequest::QueueCallback(callback));
    }

    /// Shutdown the shard
    fn shutdown(&self) {
        let _ = self.write_tx.try_send(WriteRequest::Shutdown);
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
    /// Per-group state: group_id -> GroupState (created dynamically)
    groups: Arc<DashMap<u64, Arc<GroupState<C>>>>,
}

// Type aliases for compatibility
pub type ShardedLogStorage<C> = PerGroupLogStorage<C>;
pub type MultiplexedLogStorage<C> = PerGroupLogStorage<C>;

impl<C: RaftTypeConfig + 'static> PerGroupLogStorage<C> {
    /// Create a new per-group storage instance.
    /// Groups are created dynamically on first access.
    pub async fn new(config: impl Into<StorageConfig>) -> io::Result<Self> {
        let config = config.into();
        std::fs::create_dir_all(&config.base_dir)?;

        Ok(Self {
            config,
            groups: Arc::new(DashMap::new()),
        })
    }

    /// Stop all groups
    pub fn stop(&self) {
        for entry in self.groups.iter() {
            entry.value().shard.shutdown();
        }
    }

    /// Get or create state for a group (creates shard on first access)
    fn get_or_create_group(&self, group_id: u64) -> io::Result<Arc<GroupState<C>>> {
        if let Some(state) = self.groups.get(&group_id) {
            return Ok(state.value().clone());
        }

        // Create new group state (includes shard creation and recovery)
        let state = Arc::new(GroupState::new(group_id, &self.config)?);
        self.groups.insert(group_id, state.clone());
        Ok(state)
    }

    /// Get a log storage handle for a specific group
    pub fn get_log_storage(&self, group_id: u64) -> io::Result<GroupLogStorage<C>> {
        let group_state = self.get_or_create_group(group_id)?;
        Ok(GroupLogStorage {
            group_id,
            config: self.config.clone(),
            state: group_state,
            encode_buf: Vec::new(),
        })
    }

    /// Remove a group from this storage (e.g., when group is deleted)
    pub fn remove_group(&self, group_id: u64) {
        if let Some((_, state)) = self.groups.remove(&group_id) {
            state.shard.shutdown();
        }
    }

    /// Get the list of active group IDs
    pub fn group_ids(&self) -> Vec<u64> {
        self.groups.iter().map(|e| *e.key()).collect()
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

    fn get_log_storage(&self, group_id: u64) -> Self::GroupLogStorage;
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
    C::D: ToCodec<RawBytes>,
{
    type GroupLogStorage = GroupLogStorage<C>;

    fn get_log_storage(&self, group_id: u64) -> Self::GroupLogStorage {
        // Panics if group creation fails - in practice this should be fallible
        self.get_log_storage(group_id).expect("Failed to create group storage")
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
    C::D: ToCodec<RawBytes>,
{
    type GroupLogStorage = GroupLogStorage<C>;

    fn get_log_storage(&self, group_id: u64) -> Self::GroupLogStorage {
        // Panics if group creation fails - in practice this should be fallible
        self.get_log_storage(group_id).expect("Failed to create group storage")
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
}

impl<C: RaftTypeConfig> Clone for GroupLogStorage<C> {
    fn clone(&self) -> Self {
        Self {
            group_id: self.group_id,
            config: self.config.clone(),
            state: self.state.clone(),
            encode_buf: Vec::new(), // Each clone gets its own buffer
        }
    }
}

impl<C: RaftTypeConfig> GroupLogStorage<C> {
    /// Encode a record into the internal buffer and send it to the shard writer.
    /// The buffer is taken and a new empty buffer is received back from the channel
    /// response (amortizing allocation over time).
    async fn write_record(
        &mut self,
        record_type: RecordType,
        payload: &[u8],
    ) -> Result<(), io::Error> {
        encode_record_into(&mut self.encode_buf, record_type, self.group_id, payload);
        let data = std::mem::take(&mut self.encode_buf);
        let sync_immediately = self.config.fsync_interval.is_none();
        self.state.shard.write(data, sync_immediately).await
    }

    /// Queue an IOFlushed callback to be notified after the next fsync
    fn queue_callback(&self, callback: IOFlushed<C>) {
        if self.config.fsync_interval.is_some() {
            self.state.shard.queue_callback(callback);
        } else {
            callback.io_completed(Ok(()));
        }
    }

    /// Write a raw record (for libsql integration)
    pub async fn write_raw_record(
        &mut self,
        record_type: RecordType,
        payload: &[u8],
    ) -> Result<(), io::Error> {
        self.write_record(record_type, payload).await
    }

    /// Get the group ID
    pub fn group_id(&self) -> u64 {
        self.group_id
    }
}

impl<C> RaftLogReader<C> for GroupLogStorage<C>
where
    C: RaftTypeConfig<
        NodeId = u64,
        Term = u64,
        LeaderId = openraft::impls::leader_id_adv::LeaderId<C>,
        Vote = openraft::impls::Vote<C>,
    >,
    C::Entry: Clone + 'static,
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

        let mut entries = Vec::new();
        for idx in start..end {
            if let Some(entry) = self.state.cache.get(&idx) {
                entries.push(entry.value().as_ref().clone());
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
    C::D: ToCodec<RawBytes>,
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

        // Persist to log file
        let codec_vote = vote.to_codec();
        let vote_bytes = codec_vote
            .encode_to_vec()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        self.write_record(RecordType::Vote, &vote_bytes).await?;

        Ok(())
    }

    async fn append<I>(&mut self, entries: I, callback: IOFlushed<C>) -> Result<(), io::Error>
    where
        I: IntoIterator<Item = C::Entry> + Send,
    {
        use openraft::entry::RaftEntry;

        let mut last_log_id = None;

        for entry in entries {
            let log_id = entry.log_id();
            let index = log_id.index;

            // Convert entry to codec type and serialize
            let codec_entry: CodecEntry<RawBytes> = entry.to_codec();
            let entry_bytes = codec_entry
                .encode_to_vec()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

            // Write record with CRC
            self.write_record(RecordType::Entry, &entry_bytes).await?;

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

        // Queue callback to be notified after fsync
        self.queue_callback(callback);

        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<C>) -> Result<(), io::Error> {
        let index = log_id.index;

        // Remove entries after the truncation point from cache
        let last = self.state.last_index.load(Ordering::Relaxed);
        for idx in (index + 1)..=last {
            self.state.cache.remove(&idx);
        }

        // Update last index
        self.state.last_index.store(index, Ordering::Relaxed);
        self.state.last_log_id.store(Some(&log_id));

        // Persist truncation marker with CRC
        let payload = index.to_le_bytes();
        self.write_record(RecordType::Truncate, &payload).await?;

        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<C>) -> Result<(), io::Error> {
        let index = log_id.index;

        // Remove entries up to and including the purge point from cache
        let first = self.state.first_index.load(Ordering::Relaxed);
        for idx in first..=index {
            self.state.cache.remove(&idx);
        }

        // Update first index and last_purged_log_id
        self.state.first_index.store(index + 1, Ordering::Relaxed);
        self.state.last_purged_log_id.store(Some(&log_id));

        // Persist purge marker with CRC
        use crate::multi::codec::LogId as CodecLogId;
        let codec_log_id: CodecLogId = log_id.to_codec();
        let log_id_bytes = codec_log_id
            .encode_to_vec()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        self.write_record(RecordType::Purge, &log_id_bytes).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            fsync_interval: None,
            max_cache_entries_per_group: 5000,
        };

        let config: StorageConfig = sharded.into();
        assert_eq!(config.base_dir, PathBuf::from("/tmp/test"));
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
        let parsed = validate_record(record_data).expect("should validate");

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
        assert!(validate_record(record_data).is_err());
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
            let parsed = validate_record(record_data).expect("should validate");
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
}
