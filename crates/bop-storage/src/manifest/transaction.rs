//! Transaction API for batching manifest operations.
//!
//! This module provides the `ManifestTxn` builder for accumulating operations
//! and committing them atomically to the manifest.

use std::sync::mpsc;

use super::manifest_ops::ManifestOp;
use super::tables::{JOB_RECORD_VERSION, epoch_millis};
use super::worker::{CommitReceipt, ManifestBatch, ManifestCommand};
use super::*;

/// A manifest transaction builder.
///
/// Accumulates operations and commits them atomically when `commit()` is called.
/// Operations are sent to the manifest worker thread and applied in a single
/// LMDB transaction.
///
/// # Example
///
/// ```ignore
/// let mut txn = manifest.begin();
/// txn.put_db(db_id, descriptor);
/// txn.upsert_chunk(chunk_key, chunk_entry);
/// txn.bump_generation(component_id, 1);
/// let receipt = txn.commit()?;
/// ```
#[derive(Debug)]
pub struct ManifestTxn<'a> {
    pub(super) manifest: &'a Manifest,
    pub(super) ops: Vec<ManifestOp>,
}

impl<'a> ManifestTxn<'a> {
    /// Put a database descriptor.
    pub fn put_db(&mut self, db_id: DbId, value: DbDescriptorRecord) -> &mut Self {
        self.ops.push(ManifestOp::PutDb { db_id, value });
        self
    }

    /// Delete a database descriptor.
    pub fn delete_db(&mut self, db_id: DbId) -> &mut Self {
        self.ops.push(ManifestOp::DeleteDb { db_id });
        self
    }

    /// Put a WAL state record.
    pub fn put_wal_state(&mut self, key: WalStateKey, value: WalStateRecord) -> &mut Self {
        self.ops.push(ManifestOp::PutWalState { key, value });
        self
    }

    /// Delete a WAL state record.
    pub fn delete_wal_state(&mut self, key: WalStateKey) -> &mut Self {
        self.ops.push(ManifestOp::DeleteWalState { key });
        self
    }

    /// Upsert a chunk entry.
    pub fn upsert_chunk(&mut self, key: ChunkKey, value: ChunkEntryRecord) -> &mut Self {
        self.ops.push(ManifestOp::UpsertChunk { key, value });
        self
    }

    /// Delete a chunk entry.
    pub fn delete_chunk(&mut self, key: ChunkKey) -> &mut Self {
        self.ops.push(ManifestOp::DeleteChunk { key });
        self
    }

    /// Upsert a chunk delta.
    pub fn upsert_chunk_delta(&mut self, key: ChunkDeltaKey, value: ChunkDeltaRecord) -> &mut Self {
        self.ops.push(ManifestOp::UpsertChunkDelta { key, value });
        self
    }

    /// Delete a chunk delta.
    pub fn delete_chunk_delta(&mut self, key: ChunkDeltaKey) -> &mut Self {
        self.ops.push(ManifestOp::DeleteChunkDelta { key });
        self
    }

    /// Publish a snapshot.
    pub fn publish_snapshot(&mut self, record: SnapshotRecord) -> &mut Self {
        self.ops.push(ManifestOp::PublishSnapshot { record });
        self
    }

    /// Drop a snapshot.
    pub fn drop_snapshot(&mut self, key: SnapshotKey) -> &mut Self {
        self.ops.push(ManifestOp::DropSnapshot { key });
        self
    }

    /// Register a WAL artifact.
    pub fn register_wal_artifact(
        &mut self,
        key: WalArtifactKey,
        record: WalArtifactRecord,
    ) -> &mut Self {
        self.ops.push(ManifestOp::UpsertWalArtifact { key, record });
        self
    }

    /// Remove a WAL artifact.
    pub fn remove_wal_artifact(&mut self, key: WalArtifactKey) -> &mut Self {
        self.ops.push(ManifestOp::DeleteWalArtifact { key });
        self
    }

    /// Enqueue a background job and return its ID.
    pub fn enqueue_job(
        &mut self,
        db_id: Option<DbId>,
        kind: JobKind,
        payload: JobPayload,
    ) -> JobId {
        let job_id = self.manifest.next_job_id();
        let now = epoch_millis();
        let record = JobRecord {
            record_version: JOB_RECORD_VERSION,
            job_id,
            created_at_epoch_ms: now,
            db_id,
            kind,
            state: JobDurableState::Pending {
                enqueued_at_epoch_ms: now,
            },
            payload,
        };
        self.ops.push(ManifestOp::PutJob { record });
        self.ops.push(ManifestOp::PersistJobCounter {
            next: job_id,
            timestamp_ms: now,
        });
        job_id
    }

    /// Update a job's state.
    pub fn update_job_state(&mut self, job_id: JobId, new_state: JobDurableState) -> &mut Self {
        self.ops
            .push(ManifestOp::UpdateJobState { job_id, new_state });
        self
    }

    /// Upsert a job record (T6b).
    ///
    /// Updates an existing job or creates a new one. Used for progress tracking.
    pub fn upsert_job(&mut self, _job_id: JobId, record: JobRecord) -> &mut Self {
        self.ops.push(ManifestOp::PutJob { record });
        self
    }

    /// Remove a job.
    pub fn remove_job(&mut self, job_id: JobId) -> &mut Self {
        self.ops.push(ManifestOp::RemoveJob { job_id });
        self
    }

    /// Cancel a checkpoint job with a compensating change log entry (T11b).
    ///
    /// This records the cancellation in the change log for observability and
    /// ensures subscribers can observe the cancellation event.
    pub fn cancel_checkpoint(
        &mut self,
        job_id: JobId,
        reason: crate::manifest::CheckpointCancellationReason,
    ) -> &mut Self {
        use crate::manifest::ManifestOp;
        self.ops.push(ManifestOp::CancelCheckpoint {
            job_id,
            reason,
            timestamp_ms: crate::manifest::epoch_millis(),
        });
        self
    }

    /// Upsert a pending job index entry.
    pub fn upsert_pending_job(&mut self, key: PendingJobKey, job_id: JobId) -> &mut Self {
        self.ops.push(ManifestOp::UpsertPendingJob { key, job_id });
        self
    }

    /// Delete a pending job index entry.
    pub fn delete_pending_job(&mut self, key: PendingJobKey) -> &mut Self {
        self.ops.push(ManifestOp::DeletePendingJob { key });
        self
    }

    /// Merge a metric delta.
    pub fn merge_metric(&mut self, key: MetricKey, delta: MetricDelta) -> &mut Self {
        self.ops.push(ManifestOp::MergeMetric { key, delta });
        self
    }

    /// Adjust a chunk's reference count.
    pub fn adjust_refcount(&mut self, chunk_id: ChunkId, delta: i64) -> &mut Self {
        if delta != 0 {
            self.ops
                .push(ManifestOp::AdjustRefcount { chunk_id, delta });
        }
        self
    }

    /// Bump a component's generation number.
    pub fn bump_generation(&mut self, component: ComponentId, increment: u64) -> &mut Self {
        if increment > 0 {
            self.ops.push(ManifestOp::BumpGeneration {
                component,
                increment,
                timestamp_ms: epoch_millis(),
            });
        }
        self
    }

    /// Commit this transaction.
    ///
    /// If the transaction is empty, returns immediately with the current generations.
    /// Otherwise, sends the batch to the worker and waits for the commit to complete.
    pub fn commit(mut self) -> Result<CommitReceipt, ManifestError> {
        if self.ops.is_empty() {
            return Ok(CommitReceipt {
                generations: self.manifest.current_generation_snapshot(),
            });
        }

        let batch = ManifestBatch {
            ops: self.ops.drain(..).collect(),
        };
        let (tx, rx) = mpsc::sync_channel(0);
        self.manifest.send_command(ManifestCommand::Apply {
            batch,
            completion: tx,
        })?;
        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err(ManifestError::WorkerClosed),
        }
    }
}
