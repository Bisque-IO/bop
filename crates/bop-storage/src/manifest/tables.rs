//! LMDB table definitions, record structures, and keys for the manifest storage system.
//!
//! This module defines the core data structures and database schema for the manifest system,
//! which tracks metadata for databases, chunks, snapshots, WAL artifacts, jobs, and more.
//!
//! # Architecture
//!
//! The manifest uses LMDB as its underlying storage engine with multiple named databases (tables).
//! Each table stores a specific type of metadata using typed keys and values encoded with bincode.
//!
//! # Key Encoding
//!
//! Most keys are composite types that encode multiple fields into a single integer (u32, u64, or u128)
//! to enable efficient range queries and sorted iteration. Keys are encoded in big-endian format
//! to ensure correct lexicographic ordering in LMDB.
//!
//! # Versioning
//!
//! All records include a `record_version` field to support future schema evolution. When the
//! schema changes, the version number should be incremented and migration code added.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use heed::Database;
use heed::types::{SerdeBincode, U32, U64, U128};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::PendingBatchRecord;

// ============================================================================
// Type Aliases
// ============================================================================

/// Unique identifier for a database instance.
pub type DbId = u32;

/// Unique identifier for a chunk (base data file).
pub type ChunkId = u32;

/// Unique identifier for a snapshot.
pub type SnapshotId = u32;

/// Unique identifier for a background job.
pub type JobId = u64;

/// Unique identifier for a logical component that tracks generations.
pub type ComponentId = u32;

/// Monotonically increasing generation number for versioning.
pub type Generation = u64;

/// Unique identifier for a remote storage namespace.
pub type RemoteNamespaceId = u32;

/// Unique identifier for a WAL artifact.
pub type WalArtifactId = u64;

/// Monotonically increasing sequence number for change log entries.
pub type ChangeSequence = u64;

/// Unique identifier for a change cursor.
pub type ChangeCursorId = u64;

// ============================================================================
// Version Constants
// ============================================================================

/// Special component ID used for tracking job ID generation.
pub(crate) const JOB_ID_COMPONENT: ComponentId = 0;

/// Current version of the database descriptor record format.
pub(crate) const DB_DESCRIPTOR_VERSION: u16 = 1;

/// Current version of the WAL state record format.
pub(crate) const WAL_STATE_VERSION: u16 = 1;

/// Current version of the chunk entry record format.
pub(crate) const CHUNK_ENTRY_VERSION: u16 = 1;

/// Current version of the chunk delta record format.
pub(crate) const CHUNK_DELTA_VERSION: u16 = 1;

/// Current version of the snapshot record format.
pub(crate) const SNAPSHOT_RECORD_VERSION: u16 = 1;

/// Current version of the WAL artifact record format.
pub(crate) const WAL_ARTIFACT_VERSION: u16 = 1;

/// Current version of the job record format.
pub(crate) const JOB_RECORD_VERSION: u16 = 1;

/// Current version of the metric record format.
pub(crate) const METRIC_RECORD_VERSION: u16 = 1;

/// Current version of the generation record format.
pub(crate) const GENERATION_RECORD_VERSION: u16 = 1;

/// Current version of the chunk refcount record format.
pub(crate) const CHUNK_REFCOUNT_VERSION: u16 = 1;

/// Current version of the remote namespace record format (currently unused).
#[allow(dead_code)]
pub(crate) const REMOTE_NAMESPACE_VERSION: u16 = 1;

/// Current version of the change record format.
pub(crate) const CHANGE_RECORD_VERSION: u16 = 1;

/// Current version of the cursor record format.
pub(crate) const CURSOR_RECORD_VERSION: u16 = 1;

/// Current version of the runtime state record format.
pub(crate) const RUNTIME_STATE_VERSION: u16 = 1;

/// Fixed key used for the runtime state sentinel record.
pub(crate) const RUNTIME_STATE_KEY: u32 = 0;

// ============================================================================
// LMDB Tables
// ============================================================================

/// Collection of all LMDB databases (tables) used by the manifest system.
///
/// Each field represents a separate named database within the LMDB environment.
/// All tables are created with `create_database` which ensures they exist and
/// can be opened for reading and writing.
#[derive(Debug)]
pub(crate) struct ManifestTables {
    /// Database descriptor records, indexed by DbId.
    pub(crate) db: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<DbDescriptorRecord>>,

    /// WAL state records, indexed by DbId.
    pub(crate) wal_state: Database<U32<heed::byteorder::BigEndian>, SerdeBincode<WalStateRecord>>,

    /// Chunk catalog entries, indexed by (DbId, ChunkId).
    pub(crate) chunk_catalog:
        Database<U64<heed::byteorder::BigEndian>, SerdeBincode<ChunkEntryRecord>>,

    /// Chunk delta records, indexed by (DbId, ChunkId, Generation).
    pub(crate) chunk_delta_index:
        Database<U128<heed::byteorder::BigEndian>, SerdeBincode<ChunkDeltaRecord>>,

    /// Snapshot records, indexed by (DbId, SnapshotId).
    pub(crate) snapshot_index:
        Database<U64<heed::byteorder::BigEndian>, SerdeBincode<SnapshotRecord>>,

    /// WAL artifact records, indexed by (DbId, WalArtifactId).
    pub(crate) wal_catalog:
        Database<U128<heed::byteorder::BigEndian>, SerdeBincode<WalArtifactRecord>>,

    /// Job records, indexed by JobId.
    pub(crate) job_queue: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<JobRecord>>,

    /// Pending job index for fast lookup, indexed by (DbId, JobKind) -> JobId.
    pub(crate) job_pending_index:
        Database<U64<heed::byteorder::BigEndian>, U64<heed::byteorder::BigEndian>>,

    /// Metric records, indexed by (Scope, MetricKind).
    pub(crate) metrics: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<MetricRecord>>,

    /// Generation watermarks, indexed by ComponentId.
    pub(crate) generation_watermarks:
        Database<U64<heed::byteorder::BigEndian>, SerdeBincode<GenerationRecord>>,

    /// Garbage collection reference counts, indexed by ChunkId.
    pub(crate) gc_refcounts:
        Database<U64<heed::byteorder::BigEndian>, SerdeBincode<ChunkRefcountRecord>>,

    /// Remote namespace configuration records, indexed by RemoteNamespaceId.
    pub(crate) remote_namespaces:
        Database<U32<heed::byteorder::BigEndian>, SerdeBincode<RemoteNamespaceRecord>>,

    /// Change log entries for replication, indexed by ChangeSequence.
    pub(crate) change_log:
        Database<U64<heed::byteorder::BigEndian>, SerdeBincode<ManifestChangeRecord>>,

    /// Change cursor tracking records, indexed by ChangeCursorId.
    pub(crate) change_cursors:
        Database<U64<heed::byteorder::BigEndian>, SerdeBincode<ManifestCursorRecord>>,

    /// Pending batch journal for crash recovery, indexed by PendingBatchId.
    pub(crate) pending_batches:
        Database<U64<heed::byteorder::BigEndian>, SerdeBincode<PendingBatchRecord>>,

    /// Runtime state sentinel for crash detection, indexed by fixed key.
    pub(crate) runtime_state:
        Database<U32<heed::byteorder::BigEndian>, SerdeBincode<RuntimeStateRecord>>,
}

impl ManifestTables {
    /// Opens or creates all manifest tables in the given LMDB environment.
    ///
    /// # Errors
    ///
    /// Returns an error if any database cannot be created or opened.
    pub(crate) fn open(env: &heed::Env) -> Result<Self, heed::Error> {
        let mut txn = env.write_txn()?;
        let db = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<DbDescriptorRecord>>(
                &mut txn,
                Some("db"),
            )?;
        let wal_state = env
            .create_database::<U32<heed::byteorder::BigEndian>, SerdeBincode<WalStateRecord>>(
                &mut txn,
                Some("wal_state"),
            )?;
        let chunk_catalog = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<ChunkEntryRecord>>(
                &mut txn,
                Some("chunk_catalog"),
            )?;
        let chunk_delta_index = env
            .create_database::<U128<heed::byteorder::BigEndian>, SerdeBincode<ChunkDeltaRecord>>(
                &mut txn,
                Some("chunk_delta_index"),
            )?;
        let snapshot_index = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<SnapshotRecord>>(
                &mut txn,
                Some("snapshot_index"),
            )?;
        let wal_catalog = env
            .create_database::<U128<heed::byteorder::BigEndian>, SerdeBincode<WalArtifactRecord>>(
                &mut txn,
                Some("wal_catalog"),
            )?;
        let job_queue = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<JobRecord>>(
                &mut txn,
                Some("job_queue"),
            )?;
        let job_pending_index = env
            .create_database::<U64<heed::byteorder::BigEndian>, U64<heed::byteorder::BigEndian>>(
                &mut txn,
                Some("job_pending_index"),
            )?;
        let metrics = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<MetricRecord>>(
                &mut txn,
                Some("metrics"),
            )?;
        let generation_watermarks = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<GenerationRecord>>(
                &mut txn,
                Some("generation_watermarks"),
            )?;
        let gc_refcounts = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<ChunkRefcountRecord>>(
                &mut txn,
                Some("gc_refcounts"),
            )?;
        let remote_namespaces = env.create_database::<U32<heed::byteorder::BigEndian>, SerdeBincode<RemoteNamespaceRecord>>(&mut txn, Some("remote_namespaces"))?;
        let change_log = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<ManifestChangeRecord>>(
                &mut txn,
                Some("change_log"),
            )?;
        let change_cursors = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<ManifestCursorRecord>>(
                &mut txn,
                Some("change_cursors"),
            )?;
        let pending_batches = env
            .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<PendingBatchRecord>>(
                &mut txn,
                Some("pending_batches"),
            )?;
        let runtime_state = env
            .create_database::<U32<heed::byteorder::BigEndian>, SerdeBincode<RuntimeStateRecord>>(
                &mut txn,
                Some("runtime_state"),
            )?;
        txn.commit()?;

        Ok(ManifestTables {
            db,
            wal_state,
            chunk_catalog,
            chunk_delta_index,
            snapshot_index,
            wal_catalog,
            job_queue,
            job_pending_index,
            metrics,
            generation_watermarks,
            gc_refcounts,
            remote_namespaces,
            change_log,
            change_cursors,
            pending_batches,
            runtime_state,
        })
    }
}

// ============================================================================
// Key Structures
// ============================================================================

/// Key for indexing WAL state records by database ID.
///
/// This is a simple key structure that directly maps to a u32 in LMDB.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WalStateKey {
    /// Database ID for this WAL state.
    pub db_id: DbId,
}

impl WalStateKey {
    /// Creates a new WAL state key for the given database.
    pub fn new(db_id: DbId) -> Self {
        Self { db_id }
    }

    /// Encodes this key into its LMDB storage format (u32).
    pub(crate) fn encode(&self) -> u32 {
        self.db_id
    }

    /// Decodes a raw LMDB key into a WAL state key.
    pub(crate) fn decode(raw: u32) -> Self {
        Self { db_id: raw }
    }

    /// Creates a new key with a different database ID, preserving other fields.
    pub(crate) fn with_db(self, db_id: DbId) -> Self {
        Self { db_id, ..self }
    }
}

/// Composite key for indexing chunk entries by (database ID, chunk ID).
///
/// This key is encoded as a u64 with the database ID in the upper 32 bits
/// and the chunk ID in the lower 32 bits, ensuring chunks are sorted first
/// by database, then by chunk ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkKey {
    /// Database ID that owns this chunk.
    pub db_id: DbId,

    /// Unique chunk identifier within the database.
    pub chunk_id: ChunkId,
}

impl ChunkKey {
    /// Creates a new chunk key.
    pub fn new(db_id: DbId, chunk_id: ChunkId) -> Self {
        Self { db_id, chunk_id }
    }

    /// Encodes this key into its LMDB storage format (u64).
    ///
    /// Format: [db_id (32 bits)][chunk_id (32 bits)]
    pub(crate) fn encode(&self) -> u64 {
        ((self.db_id as u64) << 32) | (self.chunk_id as u64)
    }

    /// Decodes a raw LMDB key into a chunk key.
    pub(crate) fn decode(raw: u64) -> Self {
        Self {
            db_id: (raw >> 32) as DbId,
            chunk_id: raw as ChunkId,
        }
    }

    /// Creates a new key with a different database ID, preserving the chunk ID.
    pub(crate) fn with_db(self, db_id: DbId) -> Self {
        Self { db_id, ..self }
    }
}

/// Composite key for indexing chunk delta entries by (database ID, chunk ID, generation).
///
/// Deltas are incremental updates to base chunks. This key structure enables efficient
/// range queries to find all deltas for a given chunk, sorted by generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkDeltaKey {
    /// Database ID that owns this delta.
    pub db_id: DbId,

    /// Base chunk ID that this delta applies to.
    pub chunk_id: ChunkId,

    /// Generation number of this delta (higher = newer).
    pub generation: u32,
}

impl ChunkDeltaKey {
    /// Creates a new chunk delta key.
    pub fn new(db_id: DbId, chunk_id: ChunkId, generation: u32) -> Self {
        Self {
            db_id,
            chunk_id,
            generation,
        }
    }

    /// Encodes this key into its LMDB storage format (u128).
    ///
    /// Format: [db_id (32 bits)][chunk_id (32 bits)][generation (32 bits)][padding (32 bits)]
    pub(crate) fn encode(&self) -> u128 {
        ((self.db_id as u128) << 96)
            | ((self.chunk_id as u128) << 64)
            | ((self.generation as u128) << 32)
    }

    /// Decodes a raw LMDB key into a chunk delta key.
    pub(crate) fn decode(raw: u128) -> Self {
        Self {
            db_id: (raw >> 96) as DbId,
            chunk_id: ((raw >> 64) & 0xffff_ffff) as ChunkId,
            generation: ((raw >> 32) & 0xffff_ffff) as u32,
        }
    }

    /// Creates a new key with a different database ID, preserving other fields.
    pub(crate) fn with_db(self, db_id: DbId) -> Self {
        Self { db_id, ..self }
    }
}

/// Composite key for indexing snapshots by (database ID, snapshot ID).
///
/// Snapshots represent point-in-time captures of database state, consisting
/// of a collection of chunks and metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotKey {
    /// Database ID that owns this snapshot.
    pub db_id: DbId,

    /// Unique snapshot identifier within the database.
    pub snapshot_id: SnapshotId,
}

impl SnapshotKey {
    /// Creates a new snapshot key.
    pub fn new(db_id: DbId, snapshot_id: SnapshotId) -> Self {
        Self { db_id, snapshot_id }
    }

    /// Encodes this key into its LMDB storage format (u64).
    ///
    /// Format: [db_id (32 bits)][snapshot_id (32 bits)]
    pub(crate) fn encode(&self) -> u64 {
        ((self.db_id as u64) << 32) | (self.snapshot_id as u64)
    }

    /// Decodes a raw LMDB key into a snapshot key.
    pub(crate) fn decode(raw: u64) -> Self {
        Self {
            db_id: (raw >> 32) as DbId,
            snapshot_id: raw as SnapshotId,
        }
    }
}

/// Composite key for pending job index by (database ID, job kind).
///
/// This allows quick lookup of pending jobs by type for a given database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingJobKey {
    /// Database ID that owns this job.
    pub db_id: DbId,

    /// Kind of job (see JobKind enum).
    pub job_kind: u16,
}

impl PendingJobKey {
    /// Creates a new pending job key.
    pub fn new(db_id: DbId, job_kind: u16) -> Self {
        Self { db_id, job_kind }
    }

    /// Encodes this key into its LMDB storage format (u64).
    ///
    /// Format: [db_id (32 bits)][job_kind (16 bits)][padding (16 bits)]
    pub(crate) fn encode(&self) -> u64 {
        ((self.db_id as u64) << 32) | (self.job_kind as u64)
    }

    /// Decodes a raw LMDB key into a pending job key.
    pub(crate) fn decode(raw: u64) -> Self {
        Self {
            db_id: (raw >> 32) as DbId,
            job_kind: raw as u16,
        }
    }
}

/// Composite key for metrics by (scope, metric kind).
///
/// Metrics can be scoped to different levels (global, per-database, etc.)
/// and organized by kind (latency, throughput, errors, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MetricKey {
    /// Scope of the metric (e.g., global=0, or database ID).
    pub scope: u32,

    /// Kind of metric being tracked.
    pub metric_kind: u32,
}

impl MetricKey {
    /// Creates a new metric key.
    pub fn new(scope: u32, metric_kind: u32) -> Self {
        Self { scope, metric_kind }
    }

    /// Encodes this key into its LMDB storage format (u64).
    ///
    /// Format: [scope (32 bits)][metric_kind (32 bits)]
    pub(crate) fn encode(&self) -> u64 {
        ((self.scope as u64) << 32) | (self.metric_kind as u64)
    }

    /// Decodes a raw LMDB key into a metric key.
    pub(crate) fn decode(raw: u64) -> Self {
        Self {
            scope: (raw >> 32) as u32,
            metric_kind: raw as u32,
        }
    }
}

/// Composite key for WAL artifacts by (database ID, artifact ID).
///
/// WAL artifacts represent sealed WAL segments or bundles that have been
/// written to stable storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WalArtifactKey {
    /// Database ID that owns this artifact.
    pub db_id: DbId,

    /// Unique artifact identifier.
    pub artifact_id: WalArtifactId,
}

impl WalArtifactKey {
    /// Creates a new WAL artifact key.
    pub fn new(db_id: DbId, artifact_id: WalArtifactId) -> Self {
        Self { db_id, artifact_id }
    }

    /// Encodes this key into its LMDB storage format (u128).
    ///
    /// Format: [db_id (32 bits)][artifact_id (64 bits)][padding (32 bits)]
    pub(crate) fn encode(&self) -> u128 {
        ((self.db_id as u128) << 96) | (self.artifact_id as u128)
    }

    /// Decodes a raw LMDB key into a WAL artifact key.
    pub(crate) fn decode(raw: u128) -> Self {
        Self {
            db_id: (raw >> 96) as DbId,
            artifact_id: raw as WalArtifactId,
        }
    }

    /// Creates a new key with a different database ID, preserving the artifact ID.
    pub(crate) fn with_db(self, db_id: DbId) -> Self {
        Self { db_id, ..self }
    }
}

// ============================================================================
// Record Structures
// ============================================================================

/// Metadata record for a database instance.
///
/// This record stores configuration and lifecycle information for a database.
/// Each database has a unique DbId and maintains its own set of chunks, WAL state,
/// and snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DbDescriptorRecord {
    /// Record format version for schema evolution.
    pub record_version: u16,

    /// Timestamp when this database was created (milliseconds since Unix epoch).
    pub created_at_epoch_ms: u64,

    /// Human-readable name for this database.
    pub name: String,

    /// Hash of the database configuration for detecting configuration changes.
    pub config_hash: [u8; 32],

    /// Number of WAL shards for this database (affects parallelism).
    pub wal_shards: u16,

    /// Database configuration options (directories, compression, encryption, etc.).
    pub options: ManifestDbOptions,

    /// Current lifecycle state of the database.
    pub status: DbLifecycle,
}

impl Default for DbDescriptorRecord {
    fn default() -> Self {
        Self {
            record_version: DB_DESCRIPTOR_VERSION,
            created_at_epoch_ms: epoch_millis(),
            name: String::new(),
            config_hash: [0; 32],
            wal_shards: 1,
            options: ManifestDbOptions::default(),
            status: DbLifecycle::Active,
        }
    }
}

/// Configuration options for a database instance.
///
/// These options control where files are stored and how data is processed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestDbOptions {
    /// Directory where WAL files are stored.
    pub wal_dir: PathBuf,

    /// Directory where chunk files are stored.
    pub chunk_dir: PathBuf,

    /// Compression configuration for data at rest.
    pub compression: CompressionConfig,

    /// Optional encryption configuration.
    pub encryption: Option<EncryptionConfig>,

    /// Retention policy for old generations.
    pub retention: RetentionPolicy,
}

impl Default for ManifestDbOptions {
    fn default() -> Self {
        Self {
            wal_dir: PathBuf::new(),
            chunk_dir: PathBuf::new(),
            compression: CompressionConfig::default(),
            encryption: None,
            retention: RetentionPolicy::KeepAll,
        }
    }
}

/// Lifecycle state of a database.
///
/// Tracks the database through its operational states from creation to deletion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DbLifecycle {
    /// Database is active and accepting operations.
    Active,

    /// Database is draining (preparing for shutdown, no new operations).
    Draining,

    /// Database has been deleted and is waiting for garbage collection.
    Tombstoned {
        /// Timestamp when the database was tombstoned.
        tombstoned_at_epoch_ms: u64
    },
}

/// Configuration for compression at rest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompressionConfig {
    /// Compression codec to use.
    pub codec: CompressionCodec,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            codec: CompressionCodec::None,
        }
    }
}

/// Supported compression codecs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionCodec {
    /// No compression.
    None,

    /// Zstandard compression.
    Zstd,
}

/// Configuration for encryption at rest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptionConfig {
    /// Encryption algorithm to use.
    pub algorithm: EncryptionAlgorithm,
}

/// Supported encryption algorithms.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    /// No encryption.
    None,

    /// AES-256-GCM encryption.
    Aes256Gcm,
}

/// Policy for retaining old generations of data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RetentionPolicy {
    /// Keep all generations (no automatic cleanup).
    KeepAll,

    /// Keep only the most recent N generations.
    KeepGenerations {
        /// Number of generations to retain.
        count: u32
    },
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        RetentionPolicy::KeepAll
    }
}

/// Persistent state tracking for a database's write-ahead log (WAL).
///
/// This record tracks the current state of the WAL system, including which
/// segments have been sealed, what data has been durably flushed, and checkpoint
/// progress. It's critical for crash recovery and ensuring data durability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalStateRecord {
    /// Record format version for schema evolution.
    pub record_version: u16,

    /// ID of the last WAL segment that was sealed (closed for writing).
    pub last_sealed_segment: u64,

    /// Log sequence number (LSN) of the last successfully flushed data.
    pub last_applied_lsn: u64,

    /// Generation number of the last completed checkpoint.
    pub last_checkpoint_generation: u64,

    /// Restart epoch counter, incremented on each database restart.
    pub restart_epoch: u32,

    /// Flush gate state for coordinating flush operations and error handling.
    pub flush_gate_state: FlushGateState,
}

impl Default for WalStateRecord {
    fn default() -> Self {
        Self {
            record_version: WAL_STATE_VERSION,
            last_sealed_segment: 0,
            last_applied_lsn: 0,
            last_checkpoint_generation: 0,
            restart_epoch: 0,
            flush_gate_state: FlushGateState::default(),
        }
    }
}

/// Runtime state sentinel for crash detection.
///
/// This record is written when the manifest opens and deleted on clean shutdown.
/// If present on startup, it indicates the previous instance crashed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeStateRecord {
    /// Record format version.
    pub record_version: u16,

    /// Unique ID for this manifest instance.
    pub instance_id: Uuid,

    /// Process ID of the manifest owner.
    pub pid: u32,

    /// Timestamp when this instance started.
    pub started_at_epoch_ms: u64,
}

/// State of the flush gate for error handling and coordination.
///
/// The flush gate tracks the current flush job and error state to coordinate
/// flush operations and handle failures gracefully.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlushGateState {
    /// Job ID of the currently active flush (if any).
    pub active_job_id: Option<JobId>,

    /// Timestamp of the last successful flush.
    pub last_success_at_epoch_ms: Option<u64>,

    /// Timestamp when errors started occurring (cleared on success).
    pub errored_since_epoch_ms: Option<u64>,
}

impl Default for FlushGateState {
    fn default() -> Self {
        Self {
            active_job_id: None,
            last_success_at_epoch_ms: None,
            errored_since_epoch_ms: None,
        }
    }
}

/// Metadata record for a chunk (base data file).
///
/// Chunks are the fundamental storage units in the system. This record tracks
/// the chunk's location, properties, and lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkEntryRecord {
    /// Record format version.
    pub record_version: u16,

    /// File name of the chunk on disk.
    pub file_name: String,

    /// Generation number when this chunk was created.
    pub generation: u64,

    /// Size of the chunk in bytes.
    pub size_bytes: u64,

    /// Compression codec used for this chunk.
    pub compression: CompressionCodec,

    /// Whether this chunk is encrypted.
    pub encrypted: bool,

    /// Current residency state (local, remote, both, or evicted).
    pub residency: ChunkResidency,

    /// Timestamp when uploaded to remote storage (if applicable).
    pub uploaded_at_epoch_ms: Option<u64>,

    /// Timestamp after which this chunk can be purged.
    pub purge_after_epoch_ms: Option<u64>,
}

impl Default for ChunkEntryRecord {
    fn default() -> Self {
        Self {
            record_version: CHUNK_ENTRY_VERSION,
            file_name: String::new(),
            generation: 0,
            size_bytes: 0,
            compression: CompressionCodec::None,
            encrypted: false,
            residency: ChunkResidency::Local,
            uploaded_at_epoch_ms: None,
            purge_after_epoch_ms: None,
        }
    }
}

/// Location state for a chunk indicating where it resides.
///
/// Chunks can exist in local storage, remote storage, both, or be evicted entirely.
/// This enum tracks the current residency and provides access to remote object keys
/// when applicable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChunkResidency {
    /// Chunk exists only in local storage.
    Local,

    /// Chunk exists only in remote storage (archived).
    RemoteArchived {
        /// Remote object key identifying the chunk in remote storage.
        object: RemoteObjectKey,
        /// Whether the remote copy has been verified against local checksums.
        verified: bool,
    },

    /// Chunk exists in both local and remote storage.
    LocalAndRemote {
        /// Remote object key identifying the chunk in remote storage.
        object: RemoteObjectKey,
        /// Whether the remote copy has been verified against local checksums.
        verified: bool,
    },

    /// Chunk has been evicted from both local and remote storage.
    Evicted,
}

/// Identifies a remote object in cloud storage.
///
/// Combines a namespace (bucket/prefix configuration) with an object identifier
/// to fully specify the location of a chunk or delta in remote storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteObjectKey {
    /// Remote namespace ID (references RemoteNamespaceRecord).
    pub namespace_id: RemoteNamespaceId,

    /// Object identifier within the namespace.
    pub object: RemoteObjectId,
}

impl RemoteObjectKey {
    /// Renders the full S3 path for this object within the given namespace.
    ///
    /// Combines the bucket, prefix, and object suffix to create the complete path.
    pub fn render_path(&self, namespace: &RemoteNamespaceRecord) -> String {
        let suffix = self.object.render_suffix();
        let prefix = namespace.prefix.trim_matches('/');
        if prefix.is_empty() {
            format!("{}/{}", namespace.bucket, suffix)
        } else {
            format!("{}/{}/{}", namespace.bucket, prefix, suffix)
        }
    }
}

/// Identifies a specific object within remote storage.
///
/// Encodes the origin database, chunk, object kind, and generation into a structured
/// format that can be rendered as a hierarchical path in S3 or similar storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteObjectId {
    /// Database that created this object.
    pub origin_db_id: DbId,

    /// Chunk that this object represents or relates to.
    pub chunk_id: ChunkId,

    /// Kind of object (base chunk, delta, WAL, or custom).
    pub kind: RemoteObjectKind,

    /// Generation number of this object.
    pub generation: Generation,
}

impl RemoteObjectId {
    /// Renders the path suffix for this object (without bucket/prefix).
    ///
    /// Format: `{db_id:08x}/{chunk_id:08x}/{variant}:{generation:08x}`
    pub fn render_suffix(&self) -> String {
        format!(
            "{db_id:08x}/{chunk_id:08x}/{variant}:{generation:08x}",
            db_id = self.origin_db_id,
            chunk_id = self.chunk_id,
            variant = self.kind.variant_tag(),
            generation = self.generation
        )
    }
}

/// Classifies the type of remote object stored in cloud storage.
///
/// Different object kinds have different naming conventions and lifecycle management.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RemoteObjectKind {
    /// Base chunk (full snapshot of chunk data).
    Base,

    /// Delta chunk (incremental update to base chunk).
    Delta {
        /// Sequential delta identifier within the chunk's delta chain.
        delta_id: u16
    },

    /// Write-ahead log artifact for a specific shard.
    Wal {
        /// Shard index for this WAL artifact.
        shard: u16
    },

    /// Custom object type with user-defined tag.
    Custom {
        /// User-defined tag for this object type.
        tag: String
    },
}

impl RemoteObjectKind {
    /// Returns the string tag used in the object path.
    ///
    /// Examples: "base", "delta-0001", "wal-0000", or custom tag.
    pub fn variant_tag(&self) -> String {
        match self {
            RemoteObjectKind::Base => "base".to_string(),
            RemoteObjectKind::Delta { delta_id } => format!("delta-{delta_id:04x}"),
            RemoteObjectKind::Wal { shard } => format!("wal-{shard:04x}"),
            RemoteObjectKind::Custom { tag } => tag.clone(),
        }
    }
}

/// Classifies the type of WAL artifact stored.
///
/// WAL artifacts can be either append-only segments (page-oriented) or pager bundles
/// (frame-oriented), each with different metadata requirements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WalArtifactKind {
    /// Append-only WAL segment with page-based indexing.
    AppendOnlySegment {
        /// Starting page index (inclusive).
        start_page_index: u64,
        /// Ending page index (exclusive).
        end_page_index: u64,
        /// Total size of the segment in bytes.
        size_bytes: u64,
    },

    /// Pager-style WAL bundle with frame-based indexing.
    PagerBundle {
        /// Number of frames in this bundle.
        frame_count: u64,
    },
}

/// Metadata record for a WAL artifact (sealed segment or bundle).
///
/// WAL artifacts are immutable files containing committed WAL data that have been
/// sealed and written to stable storage. They can be reclaimed once checkpointed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalArtifactRecord {
    /// Record format version.
    pub record_version: u16,

    /// Database that owns this artifact.
    pub db_id: DbId,

    /// Unique identifier for this artifact.
    pub artifact_id: WalArtifactId,

    /// Timestamp when this artifact was created.
    pub created_at_epoch_ms: u64,

    /// Type and metadata of the artifact (segment or bundle).
    pub kind: WalArtifactKind,

    /// Local filesystem path where this artifact is stored.
    pub local_path: PathBuf,
}

impl WalArtifactRecord {
    /// Creates a new WAL artifact record with the current timestamp.
    pub fn new(
        db_id: DbId,
        artifact_id: WalArtifactId,
        kind: WalArtifactKind,
        local_path: PathBuf,
    ) -> Self {
        Self {
            record_version: WAL_ARTIFACT_VERSION,
            db_id,
            artifact_id,
            created_at_epoch_ms: epoch_millis(),
            kind,
            local_path,
        }
    }
}

/// Configuration record for a remote storage namespace.
///
/// A namespace defines a bucket and optional prefix where objects are stored,
/// along with any encryption settings that apply to that namespace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteNamespaceRecord {
    /// Record format version.
    pub record_version: u16,

    /// S3 bucket name (or equivalent for other cloud providers).
    pub bucket: String,

    /// Optional prefix within the bucket (e.g., "production/chunks").
    pub prefix: String,

    /// Optional encryption profile name for objects in this namespace.
    pub encryption_profile: Option<String>,
}

/// Metadata record for a delta (incremental update to a base chunk).
///
/// Deltas allow efficient storage of incremental changes by storing only the
/// differences from a base chunk, forming a delta chain that can be applied
/// sequentially to reconstruct the current state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkDeltaRecord {
    /// Record format version.
    pub record_version: u16,

    /// Base chunk ID that this delta applies to.
    pub base_chunk_id: ChunkId,

    /// Sequential identifier for this delta in the chunk's delta chain.
    pub delta_id: u16,

    /// File name of the delta on disk.
    pub delta_file: String,

    /// Size of the delta file in bytes.
    pub size_bytes: u64,

    /// Current residency state (local, remote, or both).
    pub residency: ChunkResidency,
}

impl Default for ChunkDeltaRecord {
    fn default() -> Self {
        Self {
            record_version: CHUNK_DELTA_VERSION,
            base_chunk_id: 0,
            delta_id: 0,
            delta_file: String::new(),
            size_bytes: 0,
            residency: ChunkResidency::Local,
        }
    }
}

/// Metadata record for a database snapshot.
///
/// A snapshot is a point-in-time capture of the database state, consisting of
/// a collection of chunk references (base chunks and deltas). Snapshots are used
/// for backups, replication, and time-travel queries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotRecord {
    /// Record format version.
    pub record_version: u16,

    /// Database that owns this snapshot.
    pub db_id: DbId,

    /// Unique identifier for this snapshot.
    pub snapshot_id: SnapshotId,

    /// Timestamp when this snapshot was created.
    pub created_at_epoch_ms: u64,

    /// Generation number of the source data when this snapshot was created.
    pub source_generation: u64,

    /// List of chunk references that comprise this snapshot.
    pub chunks: Vec<SnapshotChunkRef>,
}

impl SnapshotRecord {
    /// Returns the composite key for this snapshot.
    pub fn key(&self) -> SnapshotKey {
        SnapshotKey::new(self.db_id, self.snapshot_id)
    }
}

impl Default for SnapshotRecord {
    fn default() -> Self {
        Self {
            record_version: SNAPSHOT_RECORD_VERSION,
            db_id: 0,
            snapshot_id: 0,
            created_at_epoch_ms: epoch_millis(),
            source_generation: 0,
            chunks: Vec::new(),
        }
    }
}

/// Reference to a chunk within a snapshot.
///
/// Identifies a specific chunk and whether it's a base chunk or delta,
/// allowing the snapshot to reference both types of data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotChunkRef {
    /// Chunk ID being referenced.
    pub chunk_id: ChunkId,

    /// Kind of chunk reference (base or delta).
    pub kind: SnapshotChunkKind,
}

impl SnapshotChunkRef {
    /// Creates a reference to a base chunk.
    pub fn base(chunk_id: ChunkId) -> Self {
        Self {
            chunk_id,
            kind: SnapshotChunkKind::Base,
        }
    }
}

/// Classifies a chunk reference as either base or delta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SnapshotChunkKind {
    /// Reference to a base chunk (full data).
    Base,

    /// Reference to a delta chunk (incremental update).
    Delta {
        /// The base chunk ID that this delta applies to.
        base_chunk_id: ChunkId
    },
}

/// Metadata record for a background job.
///
/// Jobs represent asynchronous work such as flushing, checkpointing, uploading,
/// or garbage collection. Each job has a lifecycle state and optional payload data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobRecord {
    /// Record format version.
    pub record_version: u16,

    /// Unique identifier for this job.
    pub job_id: JobId,

    /// Timestamp when this job was created.
    pub created_at_epoch_ms: u64,

    /// Database associated with this job (None for global jobs).
    pub db_id: Option<DbId>,

    /// Type of job (flush, checkpoint, upload, etc.).
    pub kind: JobKind,

    /// Current execution state (pending, in-flight, completed, failed).
    pub state: JobDurableState,

    /// Optional payload data specific to the job type.
    pub payload: JobPayload,
}

/// Classifies the type of background job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobKind {
    /// Flush WAL data to stable storage.
    Flush,

    /// Create a checkpoint (merge WAL into base chunks).
    Checkpoint,

    /// Upload chunks to remote storage.
    Upload,

    /// Delete obsolete data.
    Delete,

    /// Run garbage collection to reclaim space.
    Gc,

    /// Custom job type with user-defined identifier.
    Custom(u16),
}

/// Lifecycle state of a background job.
///
/// Tracks the job's progress from creation through completion or failure,
/// recording timestamps at each state transition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobDurableState {
    /// Job is queued but not yet executing.
    Pending {
        /// Timestamp when the job was enqueued.
        enqueued_at_epoch_ms: u64,
    },

    /// Job is currently executing.
    InFlight {
        /// Timestamp when execution started.
        started_at_epoch_ms: u64,
    },

    /// Job completed successfully.
    Completed {
        /// Timestamp when the job completed.
        completed_at_epoch_ms: u64,
    },

    /// Job failed with an error.
    Failed {
        /// Timestamp when the job failed.
        failed_at_epoch_ms: u64,
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Payload data for a background job.
///
/// Different job types carry different payload information. This enum provides
/// type-safe payloads for common job types and a generic Custom variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobPayload {
    /// No payload data.
    None,

    /// Flush job payload specifying which WAL shard to flush.
    FlushShard {
        /// Shard index to flush.
        shard: u16
    },

    /// Upload job payload specifying which chunk to upload.
    ChunkUpload {
        /// Chunk ID to upload.
        chunk_id: ChunkId
    },

    /// Custom payload with arbitrary bytes.
    Custom {
        /// Opaque payload bytes.
        bytes: Vec<u8>
    },
}

impl Default for JobPayload {
    fn default() -> Self {
        JobPayload::None
    }
}

/// Aggregated metric record for monitoring and observability.
///
/// Stores statistical aggregates (count, sum, min, max) for a specific metric,
/// allowing efficient updates via delta merging without needing to store individual samples.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricRecord {
    /// Record format version.
    pub record_version: u16,

    /// Number of observations recorded.
    pub count: u64,

    /// Sum of all observed values.
    pub sum: f64,

    /// Minimum observed value (None if count is 0).
    pub min: Option<f64>,

    /// Maximum observed value (None if count is 0).
    pub max: Option<f64>,
}

/// Delta update for merging into a metric record.
///
/// Represents an incremental update to a metric that can be atomically merged
/// into the existing aggregated record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricDelta {
    /// Number of new observations to add.
    pub count: u64,

    /// Sum of new observations to add.
    pub sum: f64,

    /// New minimum value to consider.
    pub min: Option<f64>,

    /// New maximum value to consider.
    pub max: Option<f64>,
}

/// Persistent generation watermark for a component.
///
/// Generations are monotonically increasing counters used for versioning and
/// coordinating distributed operations. Each component maintains its own generation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationRecord {
    /// Record format version.
    pub record_version: u16,

    /// Component that owns this generation.
    pub component: ComponentId,

    /// Current generation number for this component.
    pub current_generation: Generation,

    /// Timestamp of the last generation update.
    pub updated_at_epoch_ms: u64,
}

/// Lightweight component-generation pair for tracking updates.
///
/// Used in change log records to indicate which generations were updated
/// during a transaction without needing the full GenerationRecord.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentGeneration {
    /// Component that was updated.
    pub component: ComponentId,

    /// New generation value after the update.
    pub generation: Generation,
}

/// Change log entry for replication and auditing.
///
/// Each committed transaction creates a change record containing all operations
/// and generation updates. These records form a sequential log that can be
/// consumed by replication clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestChangeRecord {
    /// Record format version.
    pub record_version: u16,

    /// Monotonically increasing sequence number for this change.
    pub sequence: ChangeSequence,

    /// Timestamp when this change was committed.
    pub committed_at_epoch_ms: u64,

    /// List of operations included in this transaction.
    pub operations: Vec<super::ManifestOp>,

    /// List of generation updates that occurred in this transaction.
    pub generation_updates: Vec<ComponentGeneration>,
}

/// Cursor tracking record for change log subscription.
///
/// Cursors track a consumer's progress through the change log, recording which
/// changes have been acknowledged. This enables automatic truncation of old changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ManifestCursorRecord {
    /// Record format version.
    pub(crate) record_version: u16,

    /// Unique identifier for this cursor.
    pub(crate) cursor_id: ChangeCursorId,

    /// Last sequence number acknowledged by this cursor.
    pub(crate) acked_sequence: ChangeSequence,

    /// Timestamp when this cursor was created.
    pub(crate) created_at_epoch_ms: u64,

    /// Timestamp when this cursor was last updated.
    pub(crate) updated_at_epoch_ms: u64,
}

/// Reference count record for garbage collection.
///
/// Tracks how many snapshots reference a given chunk. When the refcount reaches
/// zero, the chunk becomes eligible for garbage collection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkRefcountRecord {
    /// Record format version.
    pub record_version: u16,

    /// Chunk being reference counted.
    pub chunk_id: ChunkId,

    /// Number of strong references (typically snapshot count).
    pub strong: u64,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Returns the current time as milliseconds since the Unix epoch.
///
/// This is used throughout the manifest system for timestamping records.
/// If the system clock is set before the Unix epoch (unlikely), returns 0.
pub(crate) fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
