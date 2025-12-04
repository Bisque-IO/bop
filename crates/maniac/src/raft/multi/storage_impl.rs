//! Maniac Storage Implementation for Multi-Raft
//!
//! Provides multiplexed storage that can handle multiple raft groups within
//! a single storage instance. Groups are keyed by group_id, and multiple
//! storage instances can be used for different sets of groups.
//!
//! Key design:
//! - LogStorage multiplexes entries for multiple groups: (group_id, index) -> Entry
//! - Vote storage multiplexes votes for multiple groups: group_id -> Vote
//! - State machines are external - each group provides its own
//! - Multiple storage instances can be created for different group sets

use crate::fs::OpenOptions;
use crate::io::AsyncWriteRentExt;
use crate::io::{AsyncReadRent, AsyncReadRentAt, AsyncWriteRent};
use crate::raft::multi::codec::{Encode, ToCodec, Vote as CodecVote};
use dashmap::DashMap;
use maniac_raft::{
    LogId, LogState, RaftTypeConfig, StoredMembership,
    storage::{IOFlushed, RaftLogReader, RaftLogStorage},
};
use std::io;
use std::marker::PhantomData;
use std::ops::RangeBounds;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Configuration for multiplexed log storage
#[derive(Debug, Clone)]
pub struct MultiplexedStorageConfig {
    /// Base directory for storage files
    pub base_dir: PathBuf,
    /// Sync data to disk after each write
    pub sync_on_write: bool,
    /// Maximum entries to keep in memory cache per group
    pub max_cache_entries_per_group: usize,
}

impl Default for MultiplexedStorageConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("./raft-data"),
            sync_on_write: false,
            max_cache_entries_per_group: 10000,
        }
    }
}

/// Per-group state within the multiplexed storage
struct GroupState<C: RaftTypeConfig> {
    /// In-memory log cache: index -> entry
    cache: DashMap<u64, Arc<C::Entry>>,
    /// Current vote for this group
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

/// Multiplexed log storage that handles multiple raft groups.
///
/// Each storage instance can handle any number of groups. You can create
/// multiple instances to shard groups across different storage backends
/// (e.g., fast SSD for hot groups, slower storage for cold groups).
pub struct MultiplexedLogStorage<C: RaftTypeConfig> {
    config: MultiplexedStorageConfig,
    /// Per-group state: group_id -> GroupState
    groups: Arc<DashMap<u64, Arc<GroupState<C>>>>,
    _phantom: PhantomData<C>,
}

impl<C: RaftTypeConfig> MultiplexedLogStorage<C> {
    /// Create a new multiplexed storage instance
    pub fn new(config: MultiplexedStorageConfig) -> io::Result<Self> {
        std::fs::create_dir_all(&config.base_dir)?;
        Ok(Self {
            config,
            groups: Arc::new(DashMap::new()),
            _phantom: PhantomData,
        })
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
}

impl<C: RaftTypeConfig> Clone for MultiplexedLogStorage<C> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            groups: self.groups.clone(),
            _phantom: PhantomData,
        }
    }
}

/// Log storage handle for a specific group within a MultiplexedLogStorage
pub struct GroupLogStorage<C: RaftTypeConfig> {
    group_id: u64,
    config: MultiplexedStorageConfig,
    state: Arc<GroupState<C>>,
}

impl<C: RaftTypeConfig> Clone for GroupLogStorage<C> {
    fn clone(&self) -> Self {
        Self {
            group_id: self.group_id,
            config: self.config.clone(),
            state: self.state.clone(),
        }
    }
}

impl<C: RaftTypeConfig> GroupLogStorage<C> {
    /// Get the log file path for this group
    fn log_file_path(&self) -> PathBuf {
        self.config
            .base_dir
            .join(format!("group_{}_log.dat", self.group_id))
    }

    /// Get the vote file path for this group
    fn vote_file_path(&self) -> PathBuf {
        self.config
            .base_dir
            .join(format!("group_{}_vote.dat", self.group_id))
    }

    /// Helper to write data to file with optional sync
    async fn write_to_file(&self, path: &Path, data: Vec<u8>) -> Result<(), io::Error> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .await?;

        let (res, _) = file.write_all(data).await;
        res?;

        if self.config.sync_on_write {
            file.sync_all().await?;
        }

        Ok(())
    }

    /// Helper to read entire file
    async fn read_from_file(&self, path: &Path) -> Result<Vec<u8>, io::Error> {
        let file = match OpenOptions::new().read(true).open(path).await {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let metadata = file.metadata().await?;
        let size = metadata.len() as usize;

        if size == 0 {
            return Ok(Vec::new());
        }

        let buffer = vec![0u8; size];
        let (res, buffer) = file.read_exact_at(buffer, 0).await;
        match res {
            Ok(_) => Ok(buffer),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(Vec::new()),
            Err(e) => Err(e),
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

        // Persist to disk
        let codec_vote = vote.to_codec();
        let data = codec_vote
            .encode_to_vec()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        self.write_to_file(&self.vote_file_path(), data).await?;

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

        // Signal completion
        callback.io_completed(Ok(()));

        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<C>) -> Result<(), io::Error> {
        let index = log_id.index;

        // Remove entries after the truncation point
        let last = self.state.last_index.load(Ordering::Relaxed);
        for idx in (index + 1)..=last {
            self.state.cache.remove(&idx);
        }

        // Update last index
        self.state.last_index.store(index, Ordering::Relaxed);
        self.state.last_log_id.store(Some(&log_id));

        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<C>) -> Result<(), io::Error> {
        let index = log_id.index;

        // Remove entries up to and including the purge point
        let first = self.state.first_index.load(Ordering::Relaxed);
        for idx in first..=index {
            self.state.cache.remove(&idx);
        }

        // Update first index
        self.state.first_index.store(index + 1, Ordering::Relaxed);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_config_default() {
        let config = MultiplexedStorageConfig::default();
        assert!(!config.sync_on_write);
        assert_eq!(config.max_cache_entries_per_group, 10000);
    }

    #[test]
    fn test_atomic_vote() {
        // Test AtomicVote store/load
        let vote = AtomicVote::new();

        // Initially None
        let loaded: Option<
            maniac_raft::impls::Vote<crate::raft::multi::type_config::ManiacRaftTypeConfig>,
        > = vote.load();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_atomic_log_id() {
        // Test AtomicLogId store/load
        let log_id = AtomicLogId::new();

        // Initially None
        let loaded: Option<LogId<crate::raft::multi::type_config::ManiacRaftTypeConfig>> =
            log_id.load();
        assert!(loaded.is_none());
    }
}
