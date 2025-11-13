//! Manifest operation types.
//!
//! This module defines the `ManifestOp` enum, which represents all possible
//! operations that can be performed on the manifest.

use serde::{Deserialize, Serialize};

use super::tables::*;

/// An operation to be applied to the manifest.
///
/// Operations are accumulated in a `ManifestTxn` and applied atomically
/// when the transaction is committed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManifestOp {
    /// Put an AOF database descriptor.
    PutAofDb {
        db_id: DbId,
        value: AofDescriptorRecord,
    },
    /// Delete an AOF database descriptor.
    DeleteAofDb { db_id: DbId },
    /// Put an AOF state record.
    PutAofState {
        key: AofStateKey,
        value: AofStateRecord,
    },
    /// Delete an AOF state record.
    DeleteAofState { key: AofStateKey },
    /// Upsert a chunk entry.
    UpsertChunk {
        key: ChunkKey,
        value: ChunkEntryRecord,
    },
    /// Delete a chunk entry.
    DeleteChunk { key: ChunkKey },
    /// Upsert a LibSQL chunk delta.
    #[cfg(feature = "libsql")]
    UpsertLibSqlChunkDelta {
        key: ChunkDeltaKey,
        value: LibSqlChunkDeltaRecord,
    },
    /// Delete a LibSQL chunk delta.
    #[cfg(feature = "libsql")]
    DeleteLibSqlChunkDelta { key: ChunkDeltaKey },
    /// Publish a LibSQL snapshot.
    #[cfg(feature = "libsql")]
    PublishLibSqlSnapshot { record: LibSqlSnapshotRecord },
    /// Drop a LibSQL snapshot.
    #[cfg(feature = "libsql")]
    DropLibSqlSnapshot { key: SnapshotKey },
    /// Upsert an AOF WAL artifact.
    UpsertAofWalArtifact {
        key: WalArtifactKey,
        record: AofWalArtifactRecord,
    },
    /// Delete an AOF WAL artifact.
    DeleteAofWalArtifact { key: WalArtifactKey },
    /// Upsert a LibSQL WAL artifact.
    #[cfg(feature = "libsql")]
    UpsertLibSqlWalArtifact {
        key: WalArtifactKey,
        record: LibSqlWalArtifactRecord,
    },
    /// Delete a LibSQL WAL artifact.
    #[cfg(feature = "libsql")]
    DeleteLibSqlWalArtifact { key: WalArtifactKey },
    /// Put a job record.
    PutJob { record: JobRecord },
    /// Update a job's state.
    UpdateJobState {
        job_id: JobId,
        new_state: JobDurableState,
    },
    /// Remove a job.
    RemoveJob { job_id: JobId },
    /// Upsert a pending job index entry.
    UpsertPendingJob { key: PendingJobKey, job_id: JobId },
    /// Delete a pending job index entry.
    DeletePendingJob { key: PendingJobKey },
    /// Merge a metric delta.
    MergeMetric { key: MetricKey, delta: MetricDelta },
    /// Adjust a chunk's reference count.
    AdjustRefcount { chunk_id: ChunkId, delta: i64 },
    /// Bump a component's generation.
    BumpGeneration {
        component: ComponentId,
        increment: u64,
        timestamp_ms: u64,
    },
    /// Persist the job counter generation.
    PersistJobCounter { next: JobId, timestamp_ms: u64 },
    /// Cancel a checkpoint job mid-flight (T11b: Compensating entry).
    ///
    /// This operation records that a checkpoint was cancelled, typically due to
    /// truncation conflicts. It ensures the change log reflects the cancellation
    /// for observability and auditing purposes.
    CancelCheckpoint {
        job_id: JobId,
        reason: CheckpointCancellationReason,
        timestamp_ms: u64,
    },
    /// Upsert an AOF chunk record.
    UpsertAofChunk { record: AofChunkRecord },
    /// Delete an AOF chunk record.
    DeleteAofChunk {
        aof_id: crate::aof::AofId,
        start_lsn: u64,
    },
    /// Upsert a LibSQL chunk record.
    #[cfg(feature = "libsql")]
    UpsertLibSqlChunk { record: LibSqlChunkRecord },
    /// Delete a LibSQL chunk record.
    #[cfg(feature = "libsql")]
    DeleteLibSqlChunk {
        libsql_id: crate::libsql::LibSqlId,
        chunk_id: ChunkId,
    },
}

/// Reason for checkpoint cancellation (T11b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointCancellationReason {
    /// Cancelled due to truncation conflict.
    TruncationConflict { generation: u64 },
    /// Cancelled due to user request.
    UserRequested,
    /// Cancelled due to timeout.
    Timeout,
}
