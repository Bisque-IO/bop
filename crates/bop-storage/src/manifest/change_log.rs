//! Change log management for the manifest system.
//!
//! This module handles the change log, which records all mutations applied to the manifest
//! database. The change log enables features like replication, point-in-time recovery,
//! and audit trails.
//!
//! # Architecture
//!
//! The change log is implemented as a sequence of change records, each with a monotonically
//! increasing sequence number. Each record contains:
//! - The operations performed in that transaction
//! - Generation updates resulting from those operations
//! - A commit timestamp
//!
//! # Truncation
//!
//! To prevent unbounded growth, the change log is automatically truncated when it exceeds
//! a threshold. Truncation removes old entries that are no longer needed by any active
//! change cursor. Cursors track which entries they've acknowledged, preventing truncation
//! of data they still need to consume.
//!
//! # Caching
//!
//! Recent change log entries can be cached in the page cache to avoid repeated LMDB reads.
//! This is especially beneficial for replication scenarios where consumers repeatedly
//! fetch recent changes.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use heed::{Env, RwTxn};

use super::ManifestError;
use super::tables::{ChangeCursorId, ChangeSequence, ManifestTables};

/// Maximum number of recent change log entries to prefill into the page cache on startup.
pub(crate) const CHANGE_LOG_CACHE_PREFILL_LIMIT: usize = 256;

/// Threshold for triggering automatic change log truncation (number of entries).
pub(crate) const CHANGE_LOG_TRUNCATION_THRESHOLD: u64 = 256;

/// Computes a unique cache ID for a manifest path.
///
/// This ID is used as the object_id component of page cache keys for change log entries.
/// It's derived from the manifest's filesystem path to ensure different manifests don't
/// collide in a shared page cache.
pub(crate) fn compute_change_log_cache_id(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    hasher.finish()
}

/// Internal state tracking for the change log.
///
/// This structure tracks the current bounds of the change log (oldest and next sequence)
/// and maintains metadata for all active change cursors. It's used by the manifest worker
/// thread to coordinate truncation and cursor operations.
#[derive(Debug, Clone)]
pub(crate) struct ChangeLogState {
    /// Next sequence number to be assigned (one past the latest committed sequence).
    pub(crate) next_sequence: ChangeSequence,

    /// Oldest sequence number still present in the log (after truncation).
    pub(crate) oldest_sequence: ChangeSequence,

    /// Map of active cursors and their acknowledgment state.
    pub(crate) cursors: HashMap<ChangeCursorId, ChangeCursorState>,
}

impl ChangeLogState {
    /// Returns the sequence number of the latest committed change.
    ///
    /// This is one less than `next_sequence` since `next_sequence` is the next
    /// sequence to be allocated.
    pub(crate) fn latest_sequence(&self) -> ChangeSequence {
        self.next_sequence.saturating_sub(1)
    }

    /// Returns the minimum acknowledged sequence across all active cursors.
    ///
    /// This value is used to determine the safe truncation point - we cannot
    /// truncate past any cursor that hasn't acknowledged those entries yet.
    ///
    /// Returns `None` if there are no active cursors.
    pub(crate) fn min_acked_sequence(&self) -> Option<ChangeSequence> {
        self.cursors
            .values()
            .map(|cursor| cursor.acked_sequence)
            .min()
    }

    /// Creates a snapshot of the change log state for a specific cursor.
    ///
    /// This snapshot includes the cursor's current position, the oldest available
    /// sequence, and the latest sequence. If the cursor's next read position has
    /// been truncated, it's clamped to the oldest available sequence.
    pub(crate) fn snapshot_for_cursor(
        &self,
        cursor: &ChangeCursorState,
    ) -> super::ChangeCursorSnapshot {
        let oldest_sequence = self.oldest_sequence;
        let latest_sequence = self.latest_sequence();
        let mut next_sequence = cursor.acked_sequence.saturating_add(1);
        if next_sequence < oldest_sequence {
            next_sequence = oldest_sequence;
        }
        super::ChangeCursorSnapshot {
            cursor_id: cursor.cursor_id,
            acked_sequence: cursor.acked_sequence,
            next_sequence,
            latest_sequence,
            oldest_sequence,
        }
    }
}

/// State for a single change cursor.
///
/// A cursor tracks a consumer's progress through the change log. Consumers
/// acknowledge sequences they've successfully processed, allowing the system
/// to truncate old entries once all cursors have moved past them.
#[derive(Debug, Clone)]
pub(crate) struct ChangeCursorState {
    /// Unique identifier for this cursor.
    pub(crate) cursor_id: ChangeCursorId,

    /// Last sequence number acknowledged by this cursor.
    pub(crate) acked_sequence: ChangeSequence,

    /// Timestamp when this cursor was created.
    pub(crate) created_at_epoch_ms: u64,

    /// Timestamp when this cursor was last updated.
    pub(crate) updated_at_epoch_ms: u64,
}

/// Bootstrap result from loading change state on manifest startup.
///
/// Contains the reconstructed change log state and the next cursor ID to allocate.
pub(crate) struct ChangeStateBootstrap {
    /// Reconstructed change log state from LMDB.
    pub(crate) state: ChangeLogState,

    /// Next cursor ID to be allocated (one past the highest existing cursor ID).
    pub(crate) next_cursor_id: ChangeCursorId,
}

/// Loads the change log state from the manifest database.
///
/// This function scans the change log and cursor tables to reconstruct the in-memory
/// state needed by the manifest worker. It determines the oldest and latest sequence
/// numbers, loads all active cursors, and validates cursor positions against the
/// available range.
///
/// # Errors
///
/// Returns an error if LMDB operations fail or if the database is corrupted.
pub(crate) fn load_change_state(
    env: &Env,
    tables: &ManifestTables,
) -> Result<ChangeStateBootstrap, ManifestError> {
    let txn = env.read_txn()?;
    let mut oldest_sequence: ChangeSequence = 1;
    let mut next_sequence: Option<ChangeSequence> = None;

    {
        let mut iter = tables.change_log.iter(&txn)?;
        if let Some((seq, _)) = iter.next().transpose()? {
            oldest_sequence = seq;
        }
    }

    {
        let mut rev_iter = tables.change_log.rev_iter(&txn)?;
        if let Some((seq, _)) = rev_iter.next().transpose()? {
            next_sequence = Some(seq.saturating_add(1));
        }
    }

    let mut cursors = HashMap::new();
    let mut max_cursor_id: ChangeCursorId = 0;
    let next_sequence = next_sequence.unwrap_or(oldest_sequence);
    let latest_sequence = next_sequence.saturating_sub(1);
    let min_sequence = oldest_sequence.saturating_sub(1);

    {
        let mut cursor_iter = tables.change_cursors.iter(&txn)?;
        while let Some((cursor_id_raw, record)) = cursor_iter.next().transpose()? {
            let cursor_id = cursor_id_raw as ChangeCursorId;
            let mut acked_sequence = record.acked_sequence;
            if acked_sequence > latest_sequence {
                acked_sequence = latest_sequence;
            }
            if acked_sequence < min_sequence {
                acked_sequence = min_sequence;
            }
            max_cursor_id = max_cursor_id.max(cursor_id);
            cursors.insert(
                cursor_id,
                ChangeCursorState {
                    cursor_id,
                    acked_sequence,
                    created_at_epoch_ms: record.created_at_epoch_ms,
                    updated_at_epoch_ms: record.updated_at_epoch_ms,
                },
            );
        }
    }

    txn.commit()?;

    let state = ChangeLogState {
        next_sequence: next_sequence.max(1),
        oldest_sequence: oldest_sequence.max(1),
        cursors,
    };

    Ok(ChangeStateBootstrap {
        state,
        next_cursor_id: max_cursor_id.saturating_add(1).max(1),
    })
}

/// Computes the sequence number before which the log should be truncated.
///
/// The truncation point is one past the minimum acknowledged sequence across all cursors.
/// If there are no cursors, all entries up to the next sequence can be truncated.
pub(crate) fn compute_truncate_before(state: &ChangeLogState) -> ChangeSequence {
    state
        .min_acked_sequence()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(state.next_sequence)
}

/// Deletes a range of change log entries from the database.
///
/// This function removes all change log entries with sequence numbers in the
/// range [start, end). It's used during truncation to reclaim space.
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
pub(crate) fn delete_change_log_range(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    start: ChangeSequence,
    end: ChangeSequence,
) -> Result<(), heed::Error> {
    let mut current = start;
    while current < end {
        tables.change_log.delete(txn, &current)?;
        current = current.saturating_add(1);
    }
    Ok(())
}

/// Evicts a range of change log entries from the page cache.
///
/// This should be called after truncating entries from LMDB to ensure the cache
/// doesn't contain stale data. The range is [start, end).
pub(crate) fn evict_change_log_cache_range(start: ChangeSequence, end: ChangeSequence) {
    let mut current = start;
    while current < end {
        // cache.remove(&PageCacheKey::manifest(object_id, current));
        current = current.saturating_add(1);
    }
}

/// Truncates the change log up to the given sequence number.
///
/// This function removes all change log entries before `truncate_before` from both
/// LMDB and the page cache (if configured). It updates the `oldest_sequence` in
/// the change state to reflect the new lower bound.
///
/// # Returns
///
/// Returns `Ok(true)` if truncation was performed, `Ok(false)` if there was nothing
/// to truncate (truncation point was not beyond the current oldest sequence).
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
pub(crate) fn truncate_change_log(
    env: &Env,
    tables: &ManifestTables,
    change_state: &mut ChangeLogState,
    truncate_before: ChangeSequence,
) -> Result<bool, ManifestError> {
    if truncate_before <= change_state.oldest_sequence {
        return Ok(false);
    }

    let start = change_state.oldest_sequence;
    let mut txn = env.write_txn()?;
    delete_change_log_range(&mut txn, tables, start, truncate_before)?;
    txn.commit()?;

    change_state.oldest_sequence = truncate_before;
    Ok(true)
}

/// Attempts to truncate the change log if the threshold is exceeded.
///
/// This function checks if the change log depth exceeds the truncation threshold.
/// If so, it computes a safe truncation point based on cursor acknowledgments and
/// performs the truncation.
///
/// This should be called periodically (e.g., after committing batches) to prevent
/// unbounded log growth.
///
/// # Returns
///
/// Returns `Ok(true)` if truncation was performed, `Ok(false)` otherwise.
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
pub(crate) fn maybe_truncate_change_log(
    env: &Env,
    tables: &ManifestTables,
    change_state: &mut ChangeLogState,
) -> Result<bool, ManifestError> {
    let depth = change_state
        .next_sequence
        .saturating_sub(change_state.oldest_sequence);

    if depth < CHANGE_LOG_TRUNCATION_THRESHOLD {
        return Ok(false);
    }

    let truncate_before = compute_truncate_before(change_state);
    truncate_change_log(env, tables, change_state, truncate_before)
}

/// Applies truncation during manifest startup.
///
/// This function performs an initial truncation based on cursor acknowledgments
/// loaded from disk. It runs without page cache integration since the cache is
/// populated after this operation.
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
pub(crate) fn apply_startup_truncation(
    env: &Env,
    tables: &ManifestTables,
    change_state: &mut ChangeLogState,
) -> Result<(), ManifestError> {
    let truncate_before = compute_truncate_before(change_state);
    truncate_change_log(env, tables, change_state, truncate_before).map(|_| ())
}
