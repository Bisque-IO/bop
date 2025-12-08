//! Maniac Storage Implementation for Multi-Raft
//!
//! Provides multiplexed storage that can handle multiple raft groups within
//! a single storage instance. Groups are keyed by group_id, and multiple
//! storage instances can be used for different sets of groups.
//!
//! Key design:
//! - Single logical log file per storage instance containing all groups
//! - Log entries keyed by (group_id, index)
//! - Votes stored as log entries (no separate vote file)
//! - State machines are external - each group provides its own
//! - Multiple storage instances can be created for different group sets
//! - Interval-based fsync with IOFlushed callback notification
//! - CRC64-NVME checksums for crash recovery and partial write detection
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
//! - `len`: Total length of the record (excluding len field itself)
//! - `type`: Record type (0x01=Vote, 0x02=Entry, 0x03=Truncate, 0x04=Purge)
//! - `group_id`: The raft group this record belongs to
//! - `payload`: Type-specific data
//! - `crc64`: CRC64-NVME checksum of [type + group_id + payload]
//!
//! The CRC64 allows detection of partial writes during crash recovery.
//! Records with invalid CRC are discarded during log replay.

use crate::fs::OpenOptions;
use crate::io::AsyncWriteRentExt;
use crate::io::{AsyncReadRent, AsyncReadRentAt, AsyncWriteRent};
use crate::raft::multi::codec::{Encode, ToCodec, Vote as CodecVote};
use crate::sync::mutex::Mutex;
use crc64fast_nvme::Digest;
use dashmap::DashMap;
use maniac_raft::{
    LogId, LogState, RaftTypeConfig,
    storage::{IOFlushed, RaftLogReader, RaftLogStorage},
};
use std::io;
use std::marker::PhantomData;
use std::ops::RangeBounds;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// Log record types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    Vote = 0x01,
    Entry = 0x02,
    Truncate = 0x03,
    Purge = 0x04,
}

impl TryFrom<u8> for RecordType {
    type Error = io::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(RecordType::Vote),
            0x02 => Ok(RecordType::Entry),
            0x03 => Ok(RecordType::Truncate),
            0x04 => Ok(RecordType::Purge),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid record type: {:#x}", value),
            )),
        }
    }
}

/// Header size: len(4) + type(1) + group_id(8) = 13 bytes
const RECORD_HEADER_SIZE: usize = 13;
/// CRC64 size: 8 bytes
const CRC64_SIZE: usize = 8;

/// Encodes a log record with CRC64 checksum
/// Format: [len: u32][type: u8][group_id: u64][payload...][crc64: u64]
fn encode_record(record_type: RecordType, group_id: u64, payload: &[u8]) -> Vec<u8> {
    // Total record size (excluding the len field itself)
    let record_len = 1 + 8 + payload.len() + CRC64_SIZE; // type + group_id + payload + crc

    let mut buf = Vec::with_capacity(4 + record_len);

    // Length (u32 LE)
    buf.extend_from_slice(&(record_len as u32).to_le_bytes());

    // Type (u8)
    buf.push(record_type as u8);

    // Group ID (u64 LE)
    buf.extend_from_slice(&group_id.to_le_bytes());

    // Payload
    buf.extend_from_slice(payload);

    // Compute CRC64 over [type + group_id + payload]
    let mut digest = Digest::new();
    digest.write(&buf[4..]); // Skip the len field
    let crc = digest.sum64();

    // CRC64 (u64 LE)
    buf.extend_from_slice(&crc.to_le_bytes());

    buf
}

/// Validates a record's CRC64 checksum
/// Returns (record_type, group_id, payload) if valid
fn validate_record(data: &[u8]) -> io::Result<(RecordType, u64, &[u8])> {
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
                "CRC mismatch: expected {:#x}, got {:#x}",
                stored_crc, computed_crc
            ),
        ));
    }

    // Parse header
    let record_type = RecordType::try_from(data[0])?;
    let group_id = u64::from_le_bytes(data[1..9].try_into().unwrap());
    let payload = &data[9..payload_end];

    Ok((record_type, group_id, payload))
}

/// Configuration for multiplexed log storage
#[derive(Debug, Clone)]
pub struct MultiplexedStorageConfig {
    /// Base directory for storage files
    pub base_dir: PathBuf,
    /// Interval for fsync. If None, fsync after every write.
    /// If Some(duration), fsync on interval only if dirty.
    pub fsync_interval: Option<Duration>,
    /// Maximum entries to keep in memory cache per group
    pub max_cache_entries_per_group: usize,
}

impl Default for MultiplexedStorageConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("./raft-data"),
            fsync_interval: Some(Duration::from_millis(100)), // 100ms default
            max_cache_entries_per_group: 10000,
        }
    }
}

/// Per-group state within the multiplexed storage
struct GroupState<C: RaftTypeConfig> {
    /// In-memory log cache: index -> entry
    cache: DashMap<u64, Arc<C::Entry>>,
    /// Current vote for this group (atomic)
    vote: AtomicVote,
    /// Log state
    first_index: AtomicU64,
    last_index: AtomicU64,
    last_log_id: AtomicLogId,
}

impl<C: RaftTypeConfig> GroupState<C> {
    fn new() -> Self {
        Self {
            cache: DashMap::new(),
            vote: AtomicVote::new(),
            first_index: AtomicU64::new(0),
            last_index: AtomicU64::new(0),
            last_log_id: AtomicLogId::new(),
        }
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

    fn store<C>(&self, vote: Option<&maniac_raft::impls::Vote<C>>)
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = maniac_raft::impls::leader_id_adv::LeaderId<C>,
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

    fn load<C>(&self) -> Option<maniac_raft::impls::Vote<C>>
    where
        C: RaftTypeConfig<
                NodeId = u64,
                Term = u64,
                LeaderId = maniac_raft::impls::leader_id_adv::LeaderId<C>,
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

        Some(maniac_raft::impls::Vote {
            leader_id: maniac_raft::impls::leader_id_adv::LeaderId { term, node_id },
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
                LeaderId = maniac_raft::impls::leader_id_adv::LeaderId<C>,
            >,
    {
        match log_id {
            Some(lid) => {
                let term = lid.leader_id.term;
                let node_id = lid.leader_id.node_id;
                let index = lid.index;

                let high = (term << Self::TERM_SHIFT) | (node_id & 0xFF_FFFF);
                let low = (index << Self::INDEX_SHIFT) | Self::VALID_BIT;

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
                LeaderId = maniac_raft::impls::leader_id_adv::LeaderId<C>,
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
            leader_id: maniac_raft::impls::leader_id_adv::LeaderId { term, node_id },
            index,
        })
    }
}

/// Shared state for the log file and fsync mechanism
struct SharedLogState<C: RaftTypeConfig> {
    /// The shared log file handle
    file: Mutex<crate::fs::File>,
    /// Whether there are unflushed writes
    dirty: AtomicBool,
    /// Sender for IOFlushed callbacks
    callback_tx: crate::sync::mpsc::bounded::MpscSender<IOFlushed<C>>,
    /// Whether the fsync task is running
    running: AtomicBool,
}

impl<C: RaftTypeConfig> SharedLogState<C> {
    async fn new(
        log_path: PathBuf,
    ) -> io::Result<(Self, crate::sync::mpsc::bounded::MpscReceiver<IOFlushed<C>>)> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .append(true)
            .open(&log_path)
            .await?;

        let (callback_tx, callback_rx) = crate::sync::mpsc::bounded::channel();

        Ok((
            Self {
                file: Mutex::new(file),
                dirty: AtomicBool::new(false),
                callback_tx,
                running: AtomicBool::new(true),
            },
            callback_rx,
        ))
    }

    /// Append data to the log file and optionally sync
    async fn append(&self, data: Vec<u8>, sync_immediately: bool) -> io::Result<()> {
        let mut file = self.file.lock().await;
        let (res, _) = file.write_all(data).await;
        res?;

        self.dirty.store(true, Ordering::Release);

        if sync_immediately {
            file.sync_all().await?;
            self.dirty.store(false, Ordering::Release);
        }

        Ok(())
    }

    /// Sync the file if dirty, returns true if sync was performed
    async fn sync_if_dirty(&self) -> io::Result<bool> {
        if !self.dirty.swap(false, Ordering::AcqRel) {
            return Ok(false);
        }

        let file = self.file.lock().await;
        file.sync_all().await?;
        Ok(true)
    }

    /// Queue an IOFlushed callback
    async fn queue_callback(&self, callback: IOFlushed<C>) {
        let _ = self.callback_tx.clone().send(callback).await;
    }
}

/// Multiplexed log storage that handles multiple raft groups.
///
/// All groups share a single logical log file. Each entry in the log
/// is tagged with its group_id. Votes are also stored as log entries.
///
/// Fsync behavior:
/// - If `fsync_interval` is None, fsync after every write (slow but safe)
/// - If `fsync_interval` is Some(duration), a background task fsyncs on
///   interval only if there are pending writes, then notifies all waiting
///   IOFlushed callbacks
pub struct MultiplexedLogStorage<C: RaftTypeConfig> {
    config: MultiplexedStorageConfig,
    /// Per-group state: group_id -> GroupState
    groups: Arc<DashMap<u64, Arc<GroupState<C>>>>,
    /// Shared log file state (file handle, fsync state, callbacks)
    shared_log: Arc<SharedLogState<C>>,
}

impl<C: RaftTypeConfig> MultiplexedLogStorage<C> {
    /// Create a new multiplexed storage instance
    pub async fn new(config: MultiplexedStorageConfig) -> io::Result<Self>
    where
        C: 'static,
    {
        std::fs::create_dir_all(&config.base_dir)?;

        let log_path = config.base_dir.join("raft.log");
        let (shared_log, callback_rx) = SharedLogState::new(log_path).await?;
        let shared_log = Arc::new(shared_log);

        // Start fsync task if interval-based
        if let Some(interval) = config.fsync_interval {
            let shared_log_clone = shared_log.clone();
            let _ = crate::spawn(async move {
                Self::fsync_loop(shared_log_clone, callback_rx, interval).await;
            });
        }

        let storage = Self {
            config,
            groups: Arc::new(DashMap::new()),
            shared_log,
        };

        Ok(storage)
    }

    /// Background fsync loop
    async fn fsync_loop(
        shared_log: Arc<SharedLogState<C>>,
        mut callback_rx: crate::sync::mpsc::bounded::MpscReceiver<IOFlushed<C>>,
        interval: Duration,
    ) {
        let mut pending_callbacks: Vec<IOFlushed<C>> = Vec::new();

        while shared_log.running.load(Ordering::Relaxed) {
            crate::time::sleep(interval).await;

            // Drain all pending callbacks from the channel
            while let Ok(callback) = callback_rx.try_recv() {
                pending_callbacks.push(callback);
            }

            // Skip if nothing to do
            if pending_callbacks.is_empty() && !shared_log.dirty.load(Ordering::Acquire) {
                continue;
            }

            // Sync if dirty
            let result = shared_log.sync_if_dirty().await;

            // Notify all pending callbacks
            if !pending_callbacks.is_empty() {
                // io::Error doesn't implement Clone, so create fresh results for each callback
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
            }
        }
    }

    /// Stop the fsync task
    pub fn stop(&self) {
        self.shared_log.running.store(false, Ordering::Release);
    }

    /// Get or create state for a group
    fn get_or_create_group(&self, group_id: u64) -> Arc<GroupState<C>> {
        if let Some(state) = self.groups.get(&group_id) {
            return state.clone();
        }
        let state = Arc::new(GroupState::new());
        self.groups.insert(group_id, state.clone());
        state
    }

    /// Get a log storage handle for a specific group
    pub fn get_log_storage(&self, group_id: u64) -> GroupLogStorage<C> {
        let group_state = self.get_or_create_group(group_id);
        GroupLogStorage {
            group_id,
            config: self.config.clone(),
            state: group_state,
            shared_log: self.shared_log.clone(),
        }
    }

    /// Remove a group from this storage (e.g., when group is deleted)
    pub fn remove_group(&self, group_id: u64) {
        self.groups.remove(&group_id);
    }

    /// Get list of all group IDs in this storage
    pub fn group_ids(&self) -> Vec<u64> {
        self.groups.iter().map(|e| *e.key()).collect()
    }

    /// Get the single log file path for this storage instance
    pub fn log_file_path(&self) -> PathBuf {
        self.config.base_dir.join("raft.log")
    }
}

impl<C: RaftTypeConfig> Clone for MultiplexedLogStorage<C> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            groups: self.groups.clone(),
            shared_log: self.shared_log.clone(),
        }
    }
}

impl<C: RaftTypeConfig> Drop for MultiplexedLogStorage<C> {
    fn drop(&mut self) {
        // Signal fsync task to stop
        // Note: This only affects this clone; task continues if other clones exist
        if Arc::strong_count(&self.shared_log) == 1 {
            self.shared_log.running.store(false, Ordering::Release);
        }
    }
}

/// Log storage handle for a specific group within a MultiplexedLogStorage.
///
/// Multiple handles to the same group share the same underlying state.
/// Votes and log entries are both stored in the same logical log.
pub struct GroupLogStorage<C: RaftTypeConfig> {
    group_id: u64,
    config: MultiplexedStorageConfig,
    state: Arc<GroupState<C>>,
    shared_log: Arc<SharedLogState<C>>,
}

impl<C: RaftTypeConfig> Clone for GroupLogStorage<C> {
    fn clone(&self) -> Self {
        Self {
            group_id: self.group_id,
            config: self.config.clone(),
            state: self.state.clone(),
            shared_log: self.shared_log.clone(),
        }
    }
}

impl<C: RaftTypeConfig> GroupLogStorage<C> {
    /// Append data to the log file
    async fn append_to_log(&self, data: Vec<u8>) -> Result<(), io::Error> {
        let sync_immediately = self.config.fsync_interval.is_none();
        self.shared_log.append(data, sync_immediately).await
    }

    /// Queue an IOFlushed callback to be notified after the next fsync
    async fn queue_callback(&self, callback: IOFlushed<C>) {
        if self.config.fsync_interval.is_some() {
            // Queue for batch notification after fsync
            self.shared_log.queue_callback(callback).await;
        } else {
            // Immediate mode - already synced, notify now
            callback.io_completed(Ok(()));
        }
    }
}

impl<C> RaftLogReader<C> for GroupLogStorage<C>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = maniac_raft::impls::leader_id_adv::LeaderId<C>,
            Vote = maniac_raft::impls::Vote<C>,
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
                // Clone the entry out of the Arc
                entries.push(entry.value().as_ref().clone());
            } else {
                break;
            }
        }

        Ok(entries)
    }

    async fn read_vote(&mut self) -> Result<Option<C::Vote>, io::Error> {
        // Vote is stored atomically in memory (persisted as part of log)
        Ok(self.state.vote.load())
    }
}

impl<C> RaftLogStorage<C> for GroupLogStorage<C>
where
    C: RaftTypeConfig<
            NodeId = u64,
            Term = u64,
            LeaderId = maniac_raft::impls::leader_id_adv::LeaderId<C>,
            Vote = maniac_raft::impls::Vote<C>,
        >,
    C::Entry: Send + Sync + Clone + 'static,
    C::Vote: ToCodec<CodecVote>,
{
    type LogReader = Self;

    async fn get_log_state(&mut self) -> Result<LogState<C>, io::Error> {
        let last_log_id = self.state.last_log_id.load();
        let last_purged_log_id = None; // TODO: track purged log id
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

        // Persist to log file as a vote entry
        // Format: [type: u8 = 0x01][group_id: u64][vote_data...]
        let codec_vote = vote.to_codec();
        let vote_bytes = codec_vote
            .encode_to_vec()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let mut data = Vec::with_capacity(1 + 8 + vote_bytes.len());
        data.push(0x01); // Vote entry type
        data.extend_from_slice(&self.group_id.to_le_bytes());
        data.extend_from_slice(&vote_bytes);

        self.append_to_log(data).await?;

        Ok(())
    }

    async fn append<I>(&mut self, entries: I, callback: IOFlushed<C>) -> Result<(), io::Error>
    where
        I: IntoIterator<Item = C::Entry> + Send,
    {
        use maniac_raft::entry::RaftEntry;

        let mut last_log_id = None;

        for entry in entries {
            let log_id = entry.log_id();
            let index = log_id.index;

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

        // Mark dirty
        self.shared_log.dirty.store(true, Ordering::Release);

        // TODO: Persist entries to log file
        // Format: [type: u8 = 0x02][group_id: u64][entry_data...]

        // Queue callback to be notified after fsync
        self.queue_callback(callback).await;

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

        // TODO: Persist truncation marker to log file
        // Format: [type: u8 = 0x03][group_id: u64][index: u64]

        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<C>) -> Result<(), io::Error> {
        let index = log_id.index;

        // Remove entries up to and including the purge point from cache
        let first = self.state.first_index.load(Ordering::Relaxed);
        for idx in first..=index {
            self.state.cache.remove(&idx);
        }

        // Update first index
        self.state.first_index.store(index + 1, Ordering::Relaxed);

        // TODO: Persist purge marker to log file
        // Format: [type: u8 = 0x04][group_id: u64][index: u64]

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_config_default() {
        let config = MultiplexedStorageConfig::default();
        assert_eq!(config.fsync_interval, Some(Duration::from_millis(100)));
        assert_eq!(config.max_cache_entries_per_group, 10000);
    }

    #[test]
    fn test_atomic_vote() {
        let vote = AtomicVote::new();

        // Initially None
        let loaded: Option<
            maniac_raft::impls::Vote<crate::raft::multi::type_config::ManiacRaftTypeConfig<(), ()>>,
        > = vote.load();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_atomic_log_id() {
        let log_id = AtomicLogId::new();

        // Initially None
        let loaded: Option<LogId<crate::raft::multi::type_config::ManiacRaftTypeConfig<(), ()>>> =
            log_id.load();
        assert!(loaded.is_none());
    }
}
