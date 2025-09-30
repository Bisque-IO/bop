//! Manifest database for tracking storage system metadata.
//!
//! The manifest is a durable, transactional metadata store that tracks all aspects of
//! the storage system's state. It uses LMDB as its underlying storage engine and provides
//! a high-level API for managing:
//!
//! - **Databases**: Logical database instances with their configuration
//! - **Chunks**: Base data files and their metadata (size, compression, encryption, residency)
//! - **Deltas**: Incremental updates to base chunks
//! - **Snapshots**: Point-in-time captures of database state
//! - **WAL State**: Write-ahead log progress and flush state
//! - **WAL Artifacts**: Physical WAL segments and pager bundles
//! - **Jobs**: Background tasks like flushes, checkpoints, and uploads
//! - **Metrics**: Aggregated statistics
//! - **Generations**: Monotonic version numbers for components
//!
//! # Architecture
//!
//! The manifest uses a **single-writer, multiple-reader** architecture:
//!
//! - All writes go through a background worker thread that batches operations
//! - Reads can occur directly from any thread using LMDB's MVCC transactions
//! - A change log records all mutations for replication and auditing
//!
//! ```text
//! ┌─────────────┐     Commands      ┌──────────────┐
//! │   Client    │──────────────────>│    Worker    │
//! │   Threads   │<──────────────────│    Thread    │
//! └─────────────┘     Receipts      └──────────────┘
//!       │                                   │
//!       │ Read Txns                         │ Write Txns
//!       ▼                                   ▼
//! ┌─────────────────────────────────────────────────┐
//! │                    LMDB                         │
//! │  (db, chunks, snapshots, jobs, change_log...)  │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! # Change Log
//!
//! The manifest maintains a change log that records every committed transaction.
//! This enables:
//!
//! - **Replication**: Remote systems can subscribe to changes via cursors
//! - **Auditing**: Complete history of all metadata changes
//! - **Point-in-time recovery**: Replay changes to reconstruct past states
//!
//! The change log is automatically truncated when entries are no longer needed by
//! any active cursor, preventing unbounded growth.
//!
//! # Crash Recovery
//!
//! The manifest provides crash safety through:
//!
//! - **Pending batch journal**: Operations are journaled before being applied
//! - **Runtime sentinel**: Detects unclean shutdown and replays pending operations
//! - **LMDB ACID guarantees**: All committed data is durable
//!
//! # Concurrency
//!
//! - Write operations are serialized through the worker thread
//! - Multiple readers can run concurrently without blocking each other or the writer
//! - Generation numbers provide a lightweight synchronization primitive
//! - Cursors enable non-blocking consumption of the change log
//!
//! # Examples
//!
//! ```ignore
//! // Open a manifest
//! let manifest = Manifest::open("/path/to/manifest", ManifestOptions::default())?;
//!
//! // Begin a transaction
//! let mut txn = manifest.begin();
//! txn.put_db(db_id, descriptor);
//! txn.upsert_chunk(chunk_key, chunk_entry);
//! txn.bump_generation(COMPONENT_ID, 1);
//!
//! // Commit atomically
//! let receipt = txn.commit()?;
//! println!("Committed at generation {:?}", receipt.generations);
//!
//! // Read directly
//! let state = manifest.wal_state(db_id)?;
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use heed::{Env, EnvOpenOptions};
use thiserror::Error;

use crate::page_cache::{PageCache, PageCacheKey};

// Module declarations
mod api;
mod change_log;
mod cursors;
mod flush_sink;
mod manifest_ops;
mod operations;
mod state;
mod tables;
mod transaction;
mod worker;

// Re-export public types from tables
pub use tables::{
    ChangeCursorId, ChangeSequence, ChunkDeltaKey, ChunkDeltaRecord, ChunkEntryRecord, ChunkId,
    ChunkKey, ChunkRefcountRecord, ChunkResidency, ComponentGeneration, ComponentId,
    CompressionCodec, CompressionConfig, DbDescriptorRecord, DbId, DbLifecycle,
    EncryptionAlgorithm, EncryptionConfig, FlushGateState, Generation, GenerationRecord,
    JobDurableState, JobId, JobKind, JobPayload, JobRecord, ManifestChangeRecord,
    ManifestDbOptions, MetricDelta, MetricKey, MetricRecord, PendingJobKey, RemoteNamespaceId,
    RemoteNamespaceRecord, RemoteObjectId, RemoteObjectKey, RemoteObjectKind, RetentionPolicy,
    RuntimeStateRecord, SnapshotChunkKind, SnapshotChunkRef, SnapshotId, SnapshotKey,
    SnapshotRecord, WalArtifactId, WalArtifactKey, WalArtifactKind, WalArtifactRecord, WalStateKey,
    WalStateRecord,
};

// Re-export internal types and constants from tables
pub(crate) use tables::{
    CHANGE_RECORD_VERSION, CHUNK_DELTA_VERSION, CHUNK_ENTRY_VERSION, CHUNK_REFCOUNT_VERSION,
    CURSOR_RECORD_VERSION, DB_DESCRIPTOR_VERSION, GENERATION_RECORD_VERSION, JOB_ID_COMPONENT,
    JOB_RECORD_VERSION, METRIC_RECORD_VERSION, ManifestCursorRecord, ManifestTables,
    SNAPSHOT_RECORD_VERSION, WAL_ARTIFACT_VERSION, WAL_STATE_VERSION, epoch_millis,
};

// Re-export types from submodules
pub use api::{ChangeBatchPage, ChangeCursorSnapshot, ChangeCursorStart};
pub use manifest_ops::ManifestOp;
pub use state::ManifestDiagnosticsSnapshot;
pub use transaction::ManifestTxn;
pub use worker::CommitReceipt;

// Re-export internal functions and types from change_log
use change_log::{
    ChangeLogState, apply_startup_truncation, compute_change_log_cache_id,
    hydrate_change_log_cache, load_change_state,
};

// Re-export internal types from cursors
use cursors::{CursorAckRequest, CursorRegistrationRequest};

// Re-export types from flush_sink
pub(crate) use flush_sink::ManifestFlushSink;

// Re-export types and functions from operations
use operations::{ForkData, load_generations, load_pending_batches};

// Re-export types from worker
pub(crate) use worker::PendingBatchRecord;
use worker::{ManifestBatch, ManifestCommand, WaitRequest, WorkerHandle, worker_loop};

// Re-export types from state
use state::{ChangeSignal, ChangeStateBootstrap, ManifestDiagnostics, ManifestRuntimeState};

/// Errors that can occur when using the manifest.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// I/O error from filesystem operations.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// LMDB error from database operations.
    #[error("heed error: {0}")]
    Heed(#[from] heed::Error),

    /// The manifest worker thread has terminated unexpectedly.
    #[error("manifest worker terminated")]
    WorkerClosed,

    /// Waiting for a generation timed out.
    #[error("wait for generation timed out")]
    WaitTimeout,

    /// A commit operation failed with the given reason.
    #[error("manifest commit failed: {0}")]
    CommitFailed(String),

    /// An internal invariant was violated (indicates a bug).
    #[error("manifest invariant violated: {0}")]
    InvariantViolation(String),

    /// Failed to serialize or deserialize change log data for the page cache.
    #[error("page cache serialization failed: {0}")]
    CacheSerialization(String),
}

/// Configuration options for opening a manifest.
#[derive(Debug, Clone)]
pub struct ManifestOptions {
    /// Maximum size of the LMDB memory map in bytes.
    pub map_size: usize,

    /// Maximum number of named databases (LMDB sub-databases).
    pub max_dbs: u32,

    /// Capacity of the command queue between client threads and the worker.
    pub queue_capacity: usize,

    /// How long to batch operations before committing (for throughput).
    pub commit_latency: Duration,

    /// Optional page cache for caching change log entries.
    pub page_cache: Option<Arc<PageCache<PageCacheKey>>>,

    /// Optional object ID for change log cache keys (computed from path if not provided).
    pub change_log_cache_object_id: Option<u64>,
}

impl Default for ManifestOptions {
    fn default() -> Self {
        Self {
            map_size: 256 * 1024 * 1024,
            max_dbs: 16,
            queue_capacity: 128,
            commit_latency: Duration::from_millis(5),
            page_cache: None,
            change_log_cache_object_id: None,
        }
    }
}

/// The manifest database handle.
///
/// This is the main entry point for interacting with the manifest. It provides:
/// - Transaction API for batching writes
/// - Direct read access to metadata
/// - Change cursor management for replication
/// - Generation tracking and waiting
///
/// # Thread Safety
///
/// `Manifest` is `Send` and `Sync`. Multiple threads can safely share a manifest
/// instance and perform operations concurrently. Writes are serialized through
/// the internal worker thread.
///
/// # Lifetime
///
/// When the `Manifest` is dropped, it will:
/// 1. Send a shutdown command to the worker
/// 2. Wait for all pending operations to complete
/// 3. Clear the runtime sentinel to indicate clean shutdown
/// 4. Join the worker thread
#[derive(Debug)]
pub struct Manifest {
    /// LMDB environment handle.
    env: Env,

    /// Manifest table handles.
    tables: Arc<ManifestTables>,

    /// Channel for sending commands to the worker thread.
    command_tx: SyncSender<ManifestCommand>,

    /// Handle to the background worker thread.
    worker: WorkerHandle,

    /// Shared diagnostics counters.
    diagnostics: Arc<ManifestDiagnostics>,

    /// Cached generation watermarks (updated on each commit).
    generation_cache: Arc<Mutex<HashMap<ComponentId, Generation>>>,

    /// Atomic counter for allocating job IDs.
    job_id_counter: Arc<AtomicU64>,

    /// Shared change log state for readers.
    change_state: Arc<Mutex<ChangeLogState>>,

    /// Condition variable for signaling new change log entries.
    change_signal: Arc<ChangeSignal>,

    /// Optional page cache for change log entries.
    page_cache: Option<Arc<PageCache<PageCacheKey>>>,

    /// Cache key prefix for this manifest's change log entries.
    change_log_cache_object_id: Option<u64>,

    /// Runtime state for crash detection.
    runtime_state: ManifestRuntimeState,
}

#[cfg(test)]
mod tests;
