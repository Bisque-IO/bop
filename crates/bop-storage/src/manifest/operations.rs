//! Manifest operations and commit logic.
//!
//! This module implements the core logic for applying manifest operations to LMDB
//! and managing the commit process. It handles:
//! - Loading initial state on startup
//! - Journaling pending batches for crash recovery
//! - Applying batches of operations atomically
//! - Managing refcounts for garbage collection
//! - Updating generations and metrics
//!
//! # Operation Types
//!
//! The manifest supports a variety of operations that modify different aspects of
//! database state:
//! - Database lifecycle (create, delete, update)
//! - WAL state tracking
//! - Chunk and delta management
//! - Snapshot creation and deletion
//! - Job queue management
//! - Metric aggregation
//! - Generation bumping
//!
//! All operations within a batch are applied atomically in a single LMDB transaction.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use bincode::serde::encode_to_vec;
use heed::{Env, RwTxn};
use serde::{Deserialize, Serialize};

use crate::page_cache::{PageCache, PageCacheKey};

use super::change_log::ChangeLogState;
use super::tables::{
    CHANGE_RECORD_VERSION, CHUNK_REFCOUNT_VERSION, ChunkId, ChunkRefcountRecord,
    ComponentGeneration, ComponentId, GENERATION_RECORD_VERSION, GenerationRecord,
    JOB_ID_COMPONENT, METRIC_RECORD_VERSION, ManifestChangeRecord, ManifestTables, MetricDelta,
    MetricKey, MetricRecord, SnapshotKey, SnapshotRecord, epoch_millis,
};
use super::worker::{
    ManifestBatch, PENDING_BATCH_RECORD_VERSION, PendingBatchId, PendingBatchRecord, PendingCommit,
};
use super::{Generation, JobId, ManifestDiagnostics, ManifestError, ManifestOp};

/// Data collected when forking a database.
///
/// Contains copies of all chunks, deltas, WAL state, and artifacts from the source
/// database that need to be cloned to the target database.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ForkData {
    /// Chunk catalog entries to copy.
    pub(super) chunk_entries: Vec<(super::ChunkKey, super::ChunkEntryRecord)>,

    /// Chunk delta entries to copy.
    pub(super) chunk_deltas: Vec<(super::ChunkDeltaKey, super::ChunkDeltaRecord)>,

    /// WAL state records to copy.
    pub(super) wal_states: Vec<(super::WalStateKey, super::WalStateRecord)>,

    /// WAL artifact records to copy.
    pub(super) wal_artifacts: Vec<(super::WalArtifactKey, super::WalArtifactRecord)>,
}

/// Loads the generation watermarks from LMDB on startup.
///
/// Returns a map of component IDs to their current generation numbers.
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
pub(super) fn load_generations(
    env: &Env,
    tables: &ManifestTables,
) -> Result<HashMap<ComponentId, Generation>, ManifestError> {
    let txn = env.read_txn()?;
    let mut map = HashMap::new();
    {
        let mut iter = tables.generation_watermarks.iter(&txn)?;
        while let Some((component, record)) = iter.next().transpose()? {
            map.insert(component as ComponentId, record.current_generation);
        }
    }
    txn.commit()?;
    Ok(map)
}

/// Loads pending batches from the journal on startup for crash recovery.
///
/// Returns a tuple of (pending commits to replay, highest batch ID seen).
///
/// # Errors
///
/// Returns an error if LMDB operations fail or if an unknown record version is encountered.
pub(super) fn load_pending_batches(
    env: &Env,
    tables: &ManifestTables,
) -> Result<(Vec<PendingCommit>, PendingBatchId), ManifestError> {
    let txn = env.read_txn()?;
    let mut entries = Vec::new();
    let mut iter = tables.pending_batches.iter(&txn)?;
    let mut max_id = 0;
    while let Some((id, record)) = iter.next().transpose()? {
        if record.record_version != PENDING_BATCH_RECORD_VERSION {
            return Err(ManifestError::InvariantViolation(format!(
                "unknown pending batch version: {}",
                record.record_version
            )));
        }
        max_id = max_id.max(id);
        entries.push(PendingCommit {
            id,
            batch: record.batch.clone(),
            completion: None,
        });
    }
    Ok((entries, max_id))
}

/// Journals a pending batch to LMDB before applying it.
///
/// This enables crash recovery - if the process crashes after journaling but before
/// committing the batch, it will be replayed on the next startup.
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
pub(super) fn persist_pending_batch(
    env: &Env,
    tables: &ManifestTables,
    id: PendingBatchId,
    batch: &ManifestBatch,
) -> Result<(), ManifestError> {
    let mut txn = env.write_txn()?;
    let record = PendingBatchRecord {
        record_version: PENDING_BATCH_RECORD_VERSION,
        batch: batch.clone(),
    };
    tables.pending_batches.put(&mut txn, &id, &record)?;
    txn.commit()?;
    Ok(())
}

/// Commits a batch of pending operations to LMDB atomically.
///
/// This function:
/// 1. Opens a write transaction
/// 2. Applies each pending batch's operations
/// 3. Records a change log entry for each batch
/// 4. Removes the pending batch journal entries
/// 5. Commits the transaction
/// 6. Updates the page cache with new change log entries
/// 7. Returns generation snapshots for each committed batch
///
/// # Parameters
///
/// - `env`: LMDB environment
/// - `tables`: Manifest tables
/// - `pending`: Array of pending commits to apply
/// - `generations`: Working generation map (updated in-place)
/// - `persisted_job_counter`: Job ID watermark (updated in-place)
/// - `diagnostics`: Metrics to update
/// - `change_state`: Change log state (updated in-place)
/// - `page_cache`: Optional page cache for change log entries
/// - `page_cache_object_id`: Cache key prefix
///
/// # Returns
///
/// A vector of generation snapshots, one for each committed batch, showing the
/// generation state after that batch was applied.
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
#[allow(clippy::too_many_arguments)]
pub(super) fn commit_pending(
    env: &Env,
    tables: &ManifestTables,
    pending: &[PendingCommit],
    generations: &mut HashMap<ComponentId, Generation>,
    persisted_job_counter: &mut JobId,
    diagnostics: &ManifestDiagnostics,
    change_state: &mut ChangeLogState,
    page_cache: Option<&Arc<PageCache<PageCacheKey>>>,
    page_cache_object_id: Option<u64>,
) -> Result<Vec<HashMap<ComponentId, Generation>>, heed::Error> {
    let mut txn = env.write_txn()?;
    let mut snapshots = Vec::with_capacity(pending.len());
    let mut working_generations = generations.clone();
    let config = bincode::config::standard();

    for entry in pending.iter() {
        let batch = &entry.batch;
        let before_generations = working_generations.clone();
        apply_batch(
            &mut txn,
            tables,
            batch,
            &mut working_generations,
            persisted_job_counter,
        )?;
        let sequence = change_state.next_sequence;
        change_state.next_sequence = change_state.next_sequence.saturating_add(1);
        if change_state.oldest_sequence == 0 || change_state.oldest_sequence > sequence {
            change_state.oldest_sequence = sequence;
        }
        let generation_updates =
            compute_generation_updates(&before_generations, &working_generations);
        let change_record = ManifestChangeRecord {
            record_version: CHANGE_RECORD_VERSION,
            sequence,
            committed_at_epoch_ms: epoch_millis(),
            operations: batch.ops.clone(),
            generation_updates,
        };
        tables.change_log.put(&mut txn, &sequence, &change_record)?;
        if let (Some(cache), Some(object_id)) = (page_cache, page_cache_object_id) {
            if let Ok(bytes) = encode_to_vec(&change_record, config) {
                cache.insert(
                    PageCacheKey::manifest(object_id, sequence),
                    Arc::from(bytes.into_boxed_slice()),
                );
            }
        }
        tables.pending_batches.delete(&mut txn, &entry.id)?;
        snapshots.push(working_generations.clone());
    }

    txn.commit()?;
    diagnostics
        .committed_batches
        .fetch_add(1, Ordering::Relaxed);
    *generations = working_generations;

    Ok(snapshots)
}

/// Computes the set of generation changes between two generation snapshots.
///
/// Returns only the components whose generation number changed, filtering out
/// components that remained at the same generation.
///
/// # Arguments
///
/// - `before`: Generation map before the batch was applied
/// - `after`: Generation map after the batch was applied
///
/// # Returns
///
/// A vector of generation updates for components that changed.
fn compute_generation_updates(
    before: &HashMap<ComponentId, Generation>,
    after: &HashMap<ComponentId, Generation>,
) -> Vec<ComponentGeneration> {
    after
        .iter()
        .filter_map(|(component, generation)| {
            if before.get(component) != Some(generation) {
                Some(ComponentGeneration {
                    component: *component,
                    generation: *generation,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Applies a batch of operations to LMDB within a transaction.
///
/// This is the core dispatch function that routes each operation to its
/// specific handler. All operations in the batch are applied atomically -
/// either all succeed or all fail.
///
/// # Arguments
///
/// - `txn`: Active LMDB write transaction
/// - `tables`: Manifest tables
/// - `batch`: Batch of operations to apply
/// - `generations`: Working generation map (updated in-place)
/// - `persisted_job_counter`: Job ID watermark (updated in-place)
///
/// # Errors
///
/// Returns an error if any LMDB operation fails.
fn apply_batch(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    batch: &ManifestBatch,
    generations: &mut HashMap<ComponentId, Generation>,
    persisted_job_counter: &mut JobId,
) -> Result<(), heed::Error> {
    for op in &batch.ops {
        match op {
            ManifestOp::PutDb { db_id, value } => {
                tables.db.put(txn, &(*db_id as u64), value)?;
            }
            ManifestOp::DeleteDb { db_id } => {
                tables.db.delete(txn, &(*db_id as u64))?;
            }
            ManifestOp::PutWalState { key, value } => {
                let key = key.encode();
                tables.wal_state.put(txn, &key, value)?;
            }
            ManifestOp::DeleteWalState { key } => {
                let key = key.encode();
                tables.wal_state.delete(txn, &key)?;
            }
            ManifestOp::UpsertChunk { key, value } => {
                let key = key.encode();
                tables.chunk_catalog.put(txn, &key, value)?;
            }
            ManifestOp::DeleteChunk { key } => {
                let key = key.encode();
                tables.chunk_catalog.delete(txn, &key)?;
            }
            ManifestOp::UpsertChunkDelta { key, value } => {
                let key = key.encode();
                tables.chunk_delta_index.put(txn, &key, value)?;
            }
            ManifestOp::DeleteChunkDelta { key } => {
                let key = key.encode();
                tables.chunk_delta_index.delete(txn, &key)?;
            }
            ManifestOp::PublishSnapshot { record } => {
                publish_snapshot(txn, tables, record.clone())?;
            }
            ManifestOp::DropSnapshot { key } => {
                drop_snapshot(txn, tables, *key)?;
            }
            ManifestOp::UpsertWalArtifact { key, record } => {
                let key = key.encode();
                tables.wal_catalog.put(txn, &key, record)?;
            }
            ManifestOp::DeleteWalArtifact { key } => {
                let key = key.encode();
                tables.wal_catalog.delete(txn, &key)?;
            }
            ManifestOp::PutJob { record } => {
                tables.job_queue.put(txn, &record.job_id, record)?;
            }
            ManifestOp::UpdateJobState { job_id, new_state } => {
                if let Some(mut record) = tables.job_queue.get(txn, job_id)? {
                    record.state = new_state.clone();
                    tables.job_queue.put(txn, job_id, &record)?;
                }
            }
            ManifestOp::RemoveJob { job_id } => {
                tables.job_queue.delete(txn, job_id)?;
            }
            ManifestOp::UpsertPendingJob { key, job_id } => {
                let key = key.encode();
                tables.job_pending_index.put(txn, &key, job_id)?;
            }
            ManifestOp::DeletePendingJob { key } => {
                let key = key.encode();
                tables.job_pending_index.delete(txn, &key)?;
            }
            ManifestOp::MergeMetric { key, delta } => {
                merge_metric(txn, tables, *key, delta.clone())?;
            }
            ManifestOp::AdjustRefcount { chunk_id, delta } => {
                adjust_refcount(txn, tables, *chunk_id, *delta)?;
            }
            ManifestOp::BumpGeneration {
                component,
                increment,
                timestamp_ms,
            } => {
                bump_generation(
                    txn,
                    tables,
                    *component,
                    *increment,
                    *timestamp_ms,
                    generations,
                )?;
            }
            ManifestOp::PersistJobCounter { next, timestamp_ms } => {
                persist_job_counter(
                    txn,
                    tables,
                    *next,
                    *timestamp_ms,
                    generations,
                    persisted_job_counter,
                )?;
            }
        }
    }

    Ok(())
}

/// Publishes a snapshot to the manifest, incrementing refcounts for all referenced chunks.
///
/// This function stores the snapshot record and increments the reference count
/// for each chunk that the snapshot references. This prevents chunks from being
/// garbage collected while they're still needed by a snapshot.
///
/// # Arguments
///
/// - `txn`: Active LMDB write transaction
/// - `tables`: Manifest tables
/// - `record`: Snapshot record to publish
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
fn publish_snapshot(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    record: SnapshotRecord,
) -> Result<(), heed::Error> {
    let key = record.key().encode();
    // Increment refcount for each chunk in the snapshot
    for entry in &record.chunks {
        adjust_refcount(txn, tables, entry.chunk_id, 1)?;
    }
    tables.snapshot_index.put(txn, &key, &record)
}

/// Drops a snapshot from the manifest, decrementing refcounts for all referenced chunks.
///
/// This function removes the snapshot record and decrements the reference count
/// for each chunk that the snapshot referenced. When a chunk's refcount reaches
/// zero, it becomes eligible for garbage collection.
///
/// # Arguments
///
/// - `txn`: Active LMDB write transaction
/// - `tables`: Manifest tables
/// - `key`: Key of the snapshot to drop
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
fn drop_snapshot(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    key: SnapshotKey,
) -> Result<(), heed::Error> {
    let key = key.encode();
    if let Some(record) = tables.snapshot_index.get(txn, &key)? {
        // Decrement refcount for each chunk in the snapshot
        for entry in &record.chunks {
            adjust_refcount(txn, tables, entry.chunk_id, -1)?;
        }
        tables.snapshot_index.delete(txn, &key)?;
    }
    Ok(())
}

/// Merges a metric delta into an existing metric record.
///
/// This function implements incremental metric aggregation. It updates the
/// count, sum, min, and max fields of the metric record by merging in the
/// provided delta. If no record exists, a new one is created.
///
/// # Arguments
///
/// - `txn`: Active LMDB write transaction
/// - `tables`: Manifest tables
/// - `key`: Metric key
/// - `delta`: Delta to merge into the metric
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
fn merge_metric(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    key: MetricKey,
    delta: MetricDelta,
) -> Result<(), heed::Error> {
    let key = key.encode();
    let mut record = tables
        .metrics
        .get(txn, &key)?
        .unwrap_or_else(|| MetricRecord {
            record_version: METRIC_RECORD_VERSION,
            count: 0,
            sum: 0.0,
            min: None,
            max: None,
        });

    // Saturating add prevents overflow
    record.count = record.count.saturating_add(delta.count);
    record.sum += delta.sum;

    // Update min: take the smaller of current and new values
    record.min = match (record.min, delta.min) {
        (Some(current), Some(other)) => Some(current.min(other)),
        (Some(current), None) => Some(current),
        (None, other) => other,
    };

    // Update max: take the larger of current and new values
    record.max = match (record.max, delta.max) {
        (Some(current), Some(other)) => Some(current.max(other)),
        (Some(current), None) => Some(current),
        (None, other) => other,
    };

    tables.metrics.put(txn, &key, &record)
}

/// Adjusts the reference count for a chunk.
///
/// This function implements garbage collection tracking by maintaining refcounts
/// for chunks. When a chunk's refcount reaches zero, the record is deleted,
/// making the chunk eligible for garbage collection.
///
/// # Arguments
///
/// - `txn`: Active LMDB write transaction
/// - `tables`: Manifest tables
/// - `chunk_id`: ID of the chunk to adjust
/// - `delta`: Amount to adjust (positive = increment, negative = decrement)
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
///
/// # Notes
///
/// - If delta is 0, this is a no-op
/// - If the new refcount is <= 0, the record is deleted
/// - Refcounts never go negative; they're capped at 0
fn adjust_refcount(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    chunk_id: ChunkId,
    delta: i64,
) -> Result<(), heed::Error> {
    // Early return if no adjustment needed
    if delta == 0 {
        return Ok(());
    }

    let key = chunk_id as u64;
    let mut record = tables
        .gc_refcounts
        .get(txn, &key)?
        .unwrap_or_else(|| ChunkRefcountRecord {
            record_version: CHUNK_REFCOUNT_VERSION,
            chunk_id,
            strong: 0,
        });

    // Calculate new refcount value
    let new_value = record.strong as i64 + delta;

    if new_value <= 0 {
        // Refcount reached zero - delete the record (chunk is now eligible for GC)
        tables.gc_refcounts.delete(txn, &key)?;
    } else {
        // Update the refcount
        record.strong = new_value as u64;
        tables.gc_refcounts.put(txn, &key, &record)?;
    }

    Ok(())
}

/// Increments the generation number for a component.
///
/// Generation numbers are monotonically increasing values used for versioning.
/// This function bumps a component's generation and updates both the persistent
/// storage and the in-memory generation cache.
///
/// # Arguments
///
/// - `txn`: Active LMDB write transaction
/// - `tables`: Manifest tables
/// - `component`: Component ID to bump
/// - `increment`: Amount to increment (typically 1)
/// - `timestamp_ms`: Current timestamp
/// - `generations`: In-memory generation cache (updated in-place)
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
fn bump_generation(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    component: ComponentId,
    increment: u64,
    timestamp_ms: u64,
    generations: &mut HashMap<ComponentId, Generation>,
) -> Result<(), heed::Error> {
    // Early return if no increment needed
    if increment == 0 {
        return Ok(());
    }

    let key = component as u64;
    let mut record = tables
        .generation_watermarks
        .get(txn, &key)?
        .unwrap_or_else(|| GenerationRecord {
            record_version: GENERATION_RECORD_VERSION,
            component,
            current_generation: 0,
            updated_at_epoch_ms: timestamp_ms,
        });

    // Saturating add prevents overflow
    record.current_generation = record.current_generation.saturating_add(increment);
    record.updated_at_epoch_ms = timestamp_ms;

    // Persist to LMDB
    tables.generation_watermarks.put(txn, &key, &record)?;

    // Update in-memory cache
    generations.insert(component, record.current_generation);
    Ok(())
}

/// Persists the job ID counter watermark to storage.
///
/// The job ID counter is stored as a generation number under a special component ID.
/// This ensures job IDs remain unique across restarts by persisting the watermark.
///
/// # Arguments
///
/// - `txn`: Active LMDB write transaction
/// - `tables`: Manifest tables
/// - `next`: Next job ID to persist
/// - `timestamp_ms`: Current timestamp
/// - `generations`: In-memory generation cache (updated in-place)
/// - `persisted_job_counter`: Persisted job counter (updated in-place)
///
/// # Errors
///
/// Returns an error if LMDB operations fail.
///
/// # Notes
///
/// Only updates if `next` is greater than the currently persisted value,
/// ensuring the watermark only moves forward.
fn persist_job_counter(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    next: JobId,
    timestamp_ms: u64,
    generations: &mut HashMap<ComponentId, Generation>,
    persisted_job_counter: &mut JobId,
) -> Result<(), heed::Error> {
    // Use special component ID for job counter
    let key = JOB_ID_COMPONENT as u64;
    let mut record = tables
        .generation_watermarks
        .get(txn, &key)?
        .unwrap_or_else(|| GenerationRecord {
            record_version: GENERATION_RECORD_VERSION,
            component: JOB_ID_COMPONENT,
            current_generation: 0,
            updated_at_epoch_ms: timestamp_ms,
        });

    // Only update if the new value is higher (watermark only moves forward)
    if record.current_generation < next {
        record.current_generation = next;
        record.updated_at_epoch_ms = timestamp_ms;

        // Persist to LMDB
        tables.generation_watermarks.put(txn, &key, &record)?;

        // Update in-memory caches
        generations.insert(JOB_ID_COMPONENT, record.current_generation);
        *persisted_job_counter = (*persisted_job_counter).max(next);
    }

    Ok(())
}
