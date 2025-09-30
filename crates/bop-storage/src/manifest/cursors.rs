//! Change cursor management for the manifest system.
//!
//! This module handles cursor registration, acknowledgment, and tracking. Cursors are used
//! by external consumers (e.g., replication agents) to track their progress through the
//! manifest change log.
//!
//! # Cursor Lifecycle
//!
//! 1. **Registration**: A cursor is created with a starting position (oldest, latest, or specific sequence)
//! 2. **Reading**: The consumer fetches changes from their current position
//! 3. **Acknowledgment**: After successfully processing changes, the consumer acknowledges them
//! 4. **Truncation**: Acknowledged changes can be truncated once all cursors have moved past them
//!
//! # Concurrency
//!
//! Cursor operations are processed by the manifest worker thread to avoid race conditions.
//! Requests are sent via channels and responses are returned synchronously.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;

use heed::Env;

use crate::page_cache::{PageCache, PageCacheKey};

use super::change_log::{
    ChangeCursorState, ChangeLogState, compute_truncate_before, delete_change_log_range,
    evict_change_log_cache_range,
};
use super::tables::{
    CURSOR_RECORD_VERSION, ChangeCursorId, ChangeSequence, ManifestCursorRecord, ManifestTables,
    epoch_millis,
};
use super::{ChangeCursorSnapshot, ChangeCursorStart, ManifestError};

/// Request to register a new change cursor.
///
/// Sent from the main manifest API to the worker thread.
#[derive(Debug)]
pub(crate) struct CursorRegistrationRequest {
    /// Starting position for the new cursor.
    pub(crate) start: ChangeCursorStart,

    /// Channel to send the registration result back to the caller.
    pub(crate) responder: SyncSender<Result<ChangeCursorSnapshot, ManifestError>>,
}

/// Request to acknowledge changes up to a sequence number.
///
/// Sent from the main manifest API to the worker thread.
#[derive(Debug)]
pub(crate) struct CursorAckRequest {
    /// ID of the cursor being acknowledged.
    pub(crate) cursor_id: ChangeCursorId,

    /// Sequence number up to (and including) which changes have been processed.
    pub(crate) sequence: ChangeSequence,

    /// Channel to send the acknowledgment result back to the caller.
    pub(crate) responder: SyncSender<Result<ChangeCursorSnapshot, ManifestError>>,
}

/// Registers a new change cursor with the specified starting position.
///
/// This function allocates a new cursor ID, determines the initial acknowledged sequence
/// based on the requested starting position, persists the cursor to LMDB, and adds it
/// to the in-memory change log state.
///
/// # Starting Positions
///
/// - `ChangeCursorStart::Oldest`: Start at the oldest available sequence (before any entries)
/// - `ChangeCursorStart::Latest`: Start at the latest sequence (won't see any existing entries)
/// - `ChangeCursorStart::Sequence(n)`: Start just before sequence n
///
/// If the requested position has been truncated, it will be clamped to the available range.
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
pub(crate) fn register_cursor(
    env: &Env,
    tables: &ManifestTables,
    change_state: &mut ChangeLogState,
    cursor_id_counter: &Arc<AtomicU64>,
    start: ChangeCursorStart,
) -> Result<ChangeCursorSnapshot, ManifestError> {
    let cursor_id = cursor_id_counter.fetch_add(1, Ordering::SeqCst);
    let min_sequence = change_state.oldest_sequence.saturating_sub(1);
    let latest_sequence = change_state.latest_sequence();
    let desired = match start {
        ChangeCursorStart::Oldest => min_sequence,
        ChangeCursorStart::Latest => latest_sequence,
        ChangeCursorStart::Sequence(seq) => seq.saturating_sub(1),
    };
    let acked_sequence = clamp_sequence(desired, min_sequence, latest_sequence);
    let now = epoch_millis();
    let record = ManifestCursorRecord {
        record_version: CURSOR_RECORD_VERSION,
        cursor_id,
        acked_sequence,
        created_at_epoch_ms: now,
        updated_at_epoch_ms: now,
    };

    {
        let mut txn = env.write_txn()?;
        tables.change_cursors.put(&mut txn, &cursor_id, &record)?;
        txn.commit()?;
    }

    let cursor_state = ChangeCursorState {
        cursor_id,
        acked_sequence,
        created_at_epoch_ms: now,
        updated_at_epoch_ms: now,
    };
    change_state.cursors.insert(cursor_id, cursor_state.clone());

    Ok(change_state.snapshot_for_cursor(&cursor_state))
}

/// Acknowledges changes up to the specified sequence number for a cursor.
///
/// This function updates the cursor's acknowledged sequence, persists the update to LMDB,
/// and attempts to truncate the change log if this acknowledgment enables it. Truncation
/// occurs if this cursor was the blocker (had the minimum acked sequence) and the new
/// position allows entries to be removed.
///
/// # Behavior
///
/// - If `sequence` is less than or equal to the cursor's current acked sequence, this is a no-op
/// - The sequence is clamped to the available range [oldest-1, latest]
/// - After updating, checks if truncation can advance and performs it if so
/// - Returns an updated snapshot of the cursor state
///
/// # Errors
///
/// Returns an error if the cursor doesn't exist or if LMDB operations fail.
pub(crate) fn acknowledge_cursor(
    env: &Env,
    tables: &ManifestTables,
    change_state: &mut ChangeLogState,
    cursor_id: ChangeCursorId,
    sequence: ChangeSequence,
    page_cache: Option<&Arc<PageCache<PageCacheKey>>>,
    page_cache_object_id: Option<u64>,
) -> Result<ChangeCursorSnapshot, ManifestError> {
    let existing_state = change_state
        .cursors
        .get(&cursor_id)
        .cloned()
        .ok_or_else(|| {
            ManifestError::InvariantViolation(format!("cursor {cursor_id} not registered"))
        })?;

    let min_sequence = change_state.oldest_sequence.saturating_sub(1);
    let latest_sequence = change_state.latest_sequence();
    let desired = sequence;
    let ack_target = clamp_sequence(desired, min_sequence, latest_sequence);

    if ack_target <= existing_state.acked_sequence {
        return Ok(change_state.snapshot_for_cursor(change_state.cursors.get(&cursor_id).unwrap()));
    }

    let now = epoch_millis();
    let mut updated_entry = existing_state.clone();
    updated_entry.acked_sequence = ack_target;
    updated_entry.updated_at_epoch_ms = now;

    let mut projected_state = change_state.clone();
    if let Some(cursor) = projected_state.cursors.get_mut(&cursor_id) {
        cursor.acked_sequence = ack_target;
        cursor.updated_at_epoch_ms = now;
    }

    let truncate_before = compute_truncate_before(&projected_state);
    let truncation_range = if truncate_before > projected_state.oldest_sequence {
        Some((projected_state.oldest_sequence, truncate_before))
    } else {
        None
    };

    {
        let mut txn = env.write_txn()?;
        let record = ManifestCursorRecord {
            record_version: CURSOR_RECORD_VERSION,
            cursor_id,
            acked_sequence: ack_target,
            created_at_epoch_ms: updated_entry.created_at_epoch_ms,
            updated_at_epoch_ms: now,
        };
        tables.change_cursors.put(&mut txn, &cursor_id, &record)?;

        if let Some((start, end)) = truncation_range {
            delete_change_log_range(&mut txn, tables, start, end)?;
        }

        txn.commit()?;
    }

    change_state
        .cursors
        .insert(cursor_id, updated_entry.clone());
    if let Some((start, end)) = truncation_range {
        if let (Some(cache), Some(object_id)) = (page_cache, page_cache_object_id) {
            evict_change_log_cache_range(cache, object_id, start, end);
        }
        if end > change_state.oldest_sequence {
            change_state.oldest_sequence = end;
        }
    }

    Ok(change_state.snapshot_for_cursor(&updated_entry))
}

/// Clamps a sequence number to the valid range [min, max].
///
/// This is used to ensure cursor positions stay within the available change log bounds,
/// even if a consumer requests a sequence that has been truncated or doesn't exist yet.
pub(crate) fn clamp_sequence(
    value: ChangeSequence,
    min: ChangeSequence,
    max: ChangeSequence,
) -> ChangeSequence {
    let mut clamped = value;
    if clamped < min {
        clamped = min;
    }
    if clamped > max {
        clamped = max;
    }
    clamped
}
