//! Raft Log Facade
//!
//! Provides a high-level interface for managing raft logs across multiple
//! databases and branches. Handles:
//! - Appending entries (metadata + changesets)
//! - Tracking durability state per branch
//! - Determining when log segments can be compacted
//!
//! # Compaction Eligibility
//!
//! A raft log segment can be compacted (deleted) when ALL of the following
//! conditions are met for EVERY branch referencing entries in that segment:
//!
//! 1. **WAL Durable**: All WAL segments up to the entry's frame range have
//!    been persisted to S3
//! 2. **Checkpoint Complete**: The checkpoint has advanced past the entry's
//!    frame range (WAL applied to base layer)
//! 3. **Committed**: The entry has been committed (commit_index >= log_index)
//!
//! Only when all entries in a segment meet these criteria for all branches
//! can the segment file be safely deleted.

use crate::chunk_storage::{ChunkStorage, StorageResult};
use crate::schema::*;
use crate::store::Store;
use anyhow::Result;
use std::collections::HashMap;
use std::future::Future;

/// Durability state for a branch.
///
/// Tracks what has been durably persisted, allowing us to determine
/// which raft log entries are no longer needed.
#[derive(Debug, Clone, Default)]
pub struct BranchDurabilityState {
    /// Highest WAL frame that has been persisted to S3.
    pub wal_durable_frame: u64,
    /// Highest WAL frame that has been checkpointed (applied to base layer).
    pub checkpointed_frame: u64,
    /// Highest raft log index that has been committed.
    pub committed_index: u64,
    /// Highest raft log index that has been applied.
    pub applied_index: u64,
}

impl BranchDurabilityState {
    /// Check if an entry at the given log index covering the given frame range
    /// is fully durable and can be compacted.
    pub fn is_entry_compactable(&self, log_index: u64, end_frame: u64) -> bool {
        // Entry must be committed
        if log_index > self.committed_index {
            return false;
        }
        // WAL must be durable in S3
        if end_frame > self.wal_durable_frame {
            return false;
        }
        // Must be checkpointed (applied to base layer)
        if end_frame > self.checkpointed_frame {
            return false;
        }
        true
    }
}

/// Identifies a branch within the raft log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchRef {
    pub db_id: u64,
    pub branch_id: u32,
}

impl BranchRef {
    pub fn new(db_id: u64, branch_id: u32) -> Self {
        Self { db_id, branch_id }
    }
}

/// Information about a segment's compaction eligibility.
#[derive(Debug, Clone)]
pub struct SegmentCompactionInfo {
    pub segment: RaftSegment,
    /// True if this segment can be safely deleted.
    pub can_compact: bool,
    /// Branches that still need entries from this segment.
    pub blocking_branches: Vec<BranchRef>,
}

/// Trait for raft log segment file I/O.
///
/// The actual changeset data lives in segment files, not in libmdbx.
/// This trait handles reading/writing those files.
pub trait RaftSegmentStorage: Send + Sync {
    /// Write changeset data to the current segment.
    /// Returns the byte offset where the data was written.
    fn write_changeset(
        &self,
        branch: BranchRef,
        log_index: u64,
        data: &[u8],
    ) -> impl Future<Output = StorageResult<u64>> + Send;

    /// Read changeset data from a segment.
    fn read_changeset(
        &self,
        segment_path: &str,
        offset: u64,
        len: u64,
    ) -> impl Future<Output = StorageResult<Vec<u8>>> + Send;

    /// Rotate the current segment, creating a new one.
    /// Returns metadata for the completed segment.
    fn rotate_segment(&self, branch: BranchRef) -> impl Future<Output = StorageResult<RaftSegment>> + Send;

    /// Delete a segment file.
    fn delete_segment(&self, segment_path: &str) -> impl Future<Output = StorageResult<()>> + Send;

    /// Sync segment to disk.
    fn sync(&self) -> impl Future<Output = StorageResult<()>> + Send;
}

/// High-level facade for raft log operations.
///
/// Manages the raft log across all databases and branches, providing:
/// - Entry append/read operations
/// - Durability state tracking
/// - Compaction eligibility determination
pub struct RaftLogManager<S: ChunkStorage, R: RaftSegmentStorage> {
    store: Store,
    /// S3-like storage for WAL chunks.
    chunk_storage: S,
    /// Segment file storage for changeset data.
    segment_storage: R,
    /// Cached durability state per branch.
    durability_cache: HashMap<BranchRef, BranchDurabilityState>,
}

impl<S: ChunkStorage, R: RaftSegmentStorage> RaftLogManager<S, R> {
    pub fn new(store: Store, chunk_storage: S, segment_storage: R) -> Self {
        Self {
            store,
            chunk_storage,
            segment_storage,
            durability_cache: HashMap::new(),
        }
    }

    /// Get the underlying store.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Get the chunk storage.
    pub fn chunk_storage(&self) -> &S {
        &self.chunk_storage
    }

    // ========================================================================
    // Entry Operations
    // ========================================================================

    /// Append a metadata entry (WAL info, checkpoint, etc.) to the raft log.
    pub fn append_metadata_entry(
        &self,
        branch: BranchRef,
        log_index: u64,
        entry: &RaftLogEntry,
    ) -> Result<()> {
        self.store
            .append_raft_entry(branch.db_id, branch.branch_id, log_index, entry)
    }

    /// Append a changeset entry to the raft log.
    ///
    /// The changeset data is written to the segment file, and only
    /// lightweight metadata is stored in libmdbx.
    pub async fn append_changeset_entry(
        &self,
        branch: BranchRef,
        log_index: u64,
        term: u64,
        start_frame: u64,
        end_frame: u64,
        changeset_data: &[u8],
    ) -> Result<()> {
        // Write changeset to segment file
        let _offset = self
            .segment_storage
            .write_changeset(branch, log_index, changeset_data)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write changeset: {}", e))?;

        // Create metadata entry (no inline payload - data is in segment)
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let entry = RaftLogEntry::changeset(term, start_frame, end_frame, timestamp);

        self.store
            .append_raft_entry(branch.db_id, branch.branch_id, log_index, &entry)
    }

    /// Get a raft log entry.
    pub fn get_entry(&self, branch: BranchRef, log_index: u64) -> Result<Option<RaftLogEntry>> {
        self.store
            .get_raft_entry(branch.db_id, branch.branch_id, log_index)
    }

    /// Get entries in a range.
    pub fn get_entries(
        &self,
        branch: BranchRef,
        start_index: u64,
        end_index: u64,
    ) -> Result<Vec<(u64, RaftLogEntry)>> {
        self.store
            .get_raft_entries(branch.db_id, branch.branch_id, start_index, end_index)
    }

    // ========================================================================
    // Durability Tracking
    // ========================================================================

    /// Update the durability state for a branch.
    pub fn update_durability_state(&mut self, branch: BranchRef, state: BranchDurabilityState) {
        self.durability_cache.insert(branch, state);
    }

    /// Get the cached durability state for a branch.
    pub fn get_durability_state(&self, branch: BranchRef) -> Option<&BranchDurabilityState> {
        self.durability_cache.get(&branch)
    }

    /// Load durability state from the store for a branch.
    pub fn load_durability_state(&mut self, branch: BranchRef) -> Result<BranchDurabilityState> {
        let mut state = BranchDurabilityState::default();

        // Get raft state for committed/applied indices
        if let Some(raft_state) = self.store.get_raft_state(branch.db_id, branch.branch_id)? {
            state.committed_index = raft_state.commit_index;
            state.applied_index = raft_state.last_applied;
        }

        // Get checkpoint info for checkpointed frame
        if let Some(checkpoint) = self.store.get_checkpoint(branch.db_id, branch.branch_id)? {
            state.checkpointed_frame = checkpoint.last_applied_frame;
        }

        // Get WAL segments to determine highest durable frame
        let wal_segments = self.store.get_wal_segments(branch.db_id, branch.branch_id, 0)?;
        if let Some(last_seg) = wal_segments.last() {
            state.wal_durable_frame = last_seg.end_frame;
        }

        self.durability_cache.insert(branch, state.clone());
        Ok(state)
    }

    /// Notify that WAL has been persisted to S3 up to the given frame.
    pub fn notify_wal_durable(&mut self, branch: BranchRef, durable_frame: u64) {
        let state = self.durability_cache.entry(branch).or_default();
        state.wal_durable_frame = state.wal_durable_frame.max(durable_frame);
    }

    /// Notify that a checkpoint has completed.
    pub fn notify_checkpoint(&mut self, branch: BranchRef, checkpointed_frame: u64) {
        let state = self.durability_cache.entry(branch).or_default();
        state.checkpointed_frame = state.checkpointed_frame.max(checkpointed_frame);
    }

    /// Notify that entries have been committed.
    pub fn notify_committed(&mut self, branch: BranchRef, committed_index: u64) {
        let state = self.durability_cache.entry(branch).or_default();
        state.committed_index = state.committed_index.max(committed_index);
    }

    // ========================================================================
    // Compaction
    // ========================================================================

    /// Get compaction info for all segments of a branch.
    pub fn get_compaction_info(&self, branch: BranchRef) -> Result<Vec<SegmentCompactionInfo>> {
        let segments = self.store.get_raft_segments(branch.db_id, branch.branch_id)?;
        let durability = self.durability_cache.get(&branch);

        let mut result = Vec::with_capacity(segments.len());

        for segment in segments {
            let can_compact = if let Some(state) = durability {
                self.is_segment_compactable(&segment, branch, state)?
            } else {
                false
            };

            result.push(SegmentCompactionInfo {
                segment,
                can_compact,
                blocking_branches: if can_compact {
                    Vec::new()
                } else {
                    vec![branch]
                },
            });
        }

        Ok(result)
    }

    /// Check if a segment can be compacted for a single branch.
    fn is_segment_compactable(
        &self,
        segment: &RaftSegment,
        branch: BranchRef,
        durability: &BranchDurabilityState,
    ) -> Result<bool> {
        // Get all entries in this segment's range
        let entries = self.store.get_raft_entries(
            branch.db_id,
            branch.branch_id,
            segment.start_index,
            segment.end_index,
        )?;

        // All entries must be compactable
        for (log_index, entry) in entries {
            if !durability.is_entry_compactable(log_index, entry.end_frame) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Find the highest log index that can be compacted for a branch.
    pub fn find_compactable_index(&self, branch: BranchRef) -> Result<Option<u64>> {
        let durability = match self.durability_cache.get(&branch) {
            Some(d) => d,
            None => return Ok(None),
        };

        // Start from the lowest possible and find the highest compactable
        let entries = self
            .store
            .get_raft_entries(branch.db_id, branch.branch_id, 0, durability.committed_index)?;

        let mut highest_compactable = None;

        for (log_index, entry) in entries {
            if durability.is_entry_compactable(log_index, entry.end_frame) {
                highest_compactable = Some(log_index);
            } else {
                // Once we hit a non-compactable entry, stop
                break;
            }
        }

        Ok(highest_compactable)
    }

    /// Compact (delete) segments that are fully durable.
    ///
    /// Returns the list of deleted segment paths.
    pub async fn compact_segments(&mut self, branch: BranchRef) -> Result<Vec<String>> {
        let compaction_info = self.get_compaction_info(branch)?;
        let mut deleted = Vec::new();

        for info in compaction_info {
            if info.can_compact {
                self.segment_storage
                    .delete_segment(&info.segment.path)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to delete segment: {}", e))?;

                deleted.push(info.segment.path);
            }
        }

        Ok(deleted)
    }

    // ========================================================================
    // Segment Management
    // ========================================================================

    /// Rotate the current segment for a branch.
    pub async fn rotate_segment(&self, branch: BranchRef) -> Result<RaftSegment> {
        let segment = self
            .segment_storage
            .rotate_segment(branch)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to rotate segment: {}", e))?;

        self.store
            .add_raft_segment(branch.db_id, branch.branch_id, &segment)?;

        Ok(segment)
    }

    /// Sync all pending writes to disk.
    pub async fn sync(&self) -> Result<()> {
        self.segment_storage
            .sync()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to sync: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_durability_state_compactable() {
        let state = BranchDurabilityState {
            wal_durable_frame: 100,
            checkpointed_frame: 80,
            committed_index: 50,
            applied_index: 50,
        };

        // Entry within all bounds is compactable
        assert!(state.is_entry_compactable(40, 70));

        // Entry beyond committed index is not compactable
        assert!(!state.is_entry_compactable(60, 70));

        // Entry beyond checkpointed frame is not compactable
        assert!(!state.is_entry_compactable(40, 90));

        // Entry beyond wal durable frame is not compactable
        assert!(!state.is_entry_compactable(40, 110));
    }
}
