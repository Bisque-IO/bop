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
    /// Put a database descriptor.
    PutDb {
        db_id: DbId,
        value: DbDescriptorRecord,
    },
    /// Delete a database descriptor.
    DeleteDb { db_id: DbId },
    /// Put a WAL state record.
    PutWalState {
        key: WalStateKey,
        value: WalStateRecord,
    },
    /// Delete a WAL state record.
    DeleteWalState { key: WalStateKey },
    /// Upsert a chunk entry.
    UpsertChunk {
        key: ChunkKey,
        value: ChunkEntryRecord,
    },
    /// Delete a chunk entry.
    DeleteChunk { key: ChunkKey },
    /// Upsert a chunk delta.
    UpsertChunkDelta {
        key: ChunkDeltaKey,
        value: ChunkDeltaRecord,
    },
    /// Delete a chunk delta.
    DeleteChunkDelta { key: ChunkDeltaKey },
    /// Publish a snapshot.
    PublishSnapshot { record: SnapshotRecord },
    /// Drop a snapshot.
    DropSnapshot { key: SnapshotKey },
    /// Upsert a WAL artifact.
    UpsertWalArtifact {
        key: WalArtifactKey,
        record: WalArtifactRecord,
    },
    /// Delete a WAL artifact.
    DeleteWalArtifact { key: WalArtifactKey },
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
}
