use crc32fast::Hasher as Crc32Hasher;
use serde::{Deserialize, Serialize};
use std::mem;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::aof::error::{AofError, AofResult};

// Constants for header sizes and defaults
pub const DEFAULT_SEGMENT_SIZE: u64 = 64 * 1024 * 1024; // 64MB
pub const MIN_SEGMENT_SIZE: u64 = 1024; // 1KB for testing
pub const MAX_SEGMENT_SIZE: u64 = 4 * 1024 * 1024 * 1024; // 4GB
pub const DEFAULT_SEGMENT_CACHE_SIZE: usize = 100;
pub const DEFAULT_BATCH_SIZE: usize = 100;
pub const DEFAULT_FLUSH_INTERVAL_MS: u64 = 1000;

/// Configuration for flush strategies
#[derive(Debug, Clone)]
pub enum FlushStrategy {
    /// Synchronous flush on every append
    Sync,
    /// Asynchronous flush
    Async,
    /// Batch flush every N records
    Batched(usize),
    /// Periodic flush every N milliseconds
    Periodic(u64),
    /// Combination of batched and periodic (whichever comes first)
    BatchedOrPeriodic { batch_size: usize, interval_ms: u64 },
}

impl Default for FlushStrategy {
    fn default() -> Self {
        FlushStrategy::BatchedOrPeriodic {
            batch_size: DEFAULT_BATCH_SIZE,
            interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
        }
    }
}

/// Configuration for AOF instances
#[derive(Debug, Clone)]
pub struct AofConfig {
    pub segment_size: u64,
    pub flush_strategy: FlushStrategy,
    pub segment_cache_size: usize,
    pub enable_compression: bool,
    pub enable_encryption: bool,
    pub ttl_seconds: Option<u64>,
    pub pre_allocate_threshold: f32, // Pre-allocate next segment when current is X% full (0.0-1.0)
    pub pre_allocate_enabled: bool,  // Enable pre-allocation of next segment
    pub archive_enabled: bool,       // Enable archiving of finalized segments
    pub archive_compression_level: i32, // zstd compression level for archives
    pub archive_threshold: u64,      // Archive segments when this many segments exist
    pub archive_backpressure_segments: u64, // Stop progress if archive queue exceeds this
    pub archive_backpressure_bytes: u64, // Stop progress if archive queue exceeds this size
    pub index_path: Option<String>,  // Path for MDBX segment index database
}

impl Default for AofConfig {
    fn default() -> Self {
        Self {
            segment_size: DEFAULT_SEGMENT_SIZE,
            flush_strategy: FlushStrategy::default(),
            segment_cache_size: DEFAULT_SEGMENT_CACHE_SIZE,
            enable_compression: false,
            enable_encryption: false,
            ttl_seconds: None,
            pre_allocate_threshold: 0.8, // Pre-allocate when 80% full
            pre_allocate_enabled: true,
            archive_enabled: false,       // Archiving disabled by default
            archive_compression_level: 3, // Default zstd compression level
            archive_threshold: 100,       // Archive when 100 segments exist
            archive_backpressure_segments: 500, // Stop progress after 500 unarchived segments
            archive_backpressure_bytes: 10 * 1024 * 1024 * 1024, // 10GB of unarchived data
            index_path: None,             // Use default path generation
        }
    }
}

/// Represents the header of a record in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct RecordHeader {
    /// CRC32 checksum of the record data.
    pub checksum: u32,
    /// Size of the record data in bytes.
    pub size: u32,
    /// Monotonically increasing ID for the record.
    pub id: u64,
    /// Timestamp when the record was created (microseconds since epoch).
    pub timestamp: u64,
    /// Version for future compatibility
    pub version: u8,
    /// Flags for compression, encryption, etc.
    pub flags: u8,
    /// Reserved for future use
    pub reserved: [u8; 6],
}

impl RecordHeader {
    pub const fn size() -> usize {
        mem::size_of::<Self>()
    }

    /// Create a new record header with checksum and timestamp
    pub fn new(id: u64, data: &[u8]) -> Self {
        let mut hasher = Crc32Hasher::new();
        hasher.update(data);
        let checksum = hasher.finalize();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let _hash = Self::calculate_hash(data, id);

        RecordHeader {
            checksum,
            id,
            size: data.len() as u32,
            timestamp,
            version: 1,
            flags: 0,
            reserved: [0; 6],
        }
    }

    /// Verify the checksum of the record data
    pub fn verify_checksum(&self, data: &[u8]) -> bool {
        let mut hasher = Crc32Hasher::new();
        hasher.update(data);
        hasher.finalize() == self.checksum
    }

    /// Calculate hash for integrity checking
    fn calculate_hash(data: &[u8], id: u64) -> u64 {
        let mut hash = id;
        for &byte in data {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }

    /// Serialize header to bytes safely
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize header from bytes safely
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

/// Represents a full record including its header and data.
pub struct Record<'a> {
    pub header: RecordHeader,
    pub data: &'a [u8],
}

/// Special record type for segment metadata (written by background flush)
/// This provides a durable index and marks end-of-file for tailing readers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMetadataRecord {
    /// Magic number to identify this as a metadata record
    pub magic: u32,
    /// Total number of records in this segment (excluding this metadata record)
    pub record_count: u64,
    /// Last record ID written to this segment
    pub last_record_id: u64,
    /// Segment base ID
    pub base_id: u64,
    /// Timestamp when this metadata was written
    pub finalized_at: u64,
    /// Checksum of all record data in the segment
    pub data_checksum: u64,
    /// Version for compatibility
    pub version: u8,
    /// Reserved for future use
    pub reserved: [u8; 7],
}

impl SegmentMetadataRecord {
    /// Create a new segment metadata record
    pub fn new(base_id: u64, last_record_id: u64, record_count: u64, data_checksum: u64) -> Self {
        Self {
            magic: 0xABCDEF02, // Special magic for metadata records
            record_count,
            last_record_id,
            base_id,
            finalized_at: current_timestamp(),
            data_checksum,
            version: 1,
            reserved: [0; 7],
        }
    }

    /// Check if this is a valid metadata record
    pub fn is_valid(&self) -> bool {
        self.magic == 0xABCDEF02 && self.version == 1
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

/// Segment metadata for indexing and management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMetadata {
    pub base_id: u64,
    pub last_id: u64,
    pub record_count: u64,
    pub created_at: u64,
    pub size: u64,
    pub checksum: u32,
    pub compressed: bool,
    pub encrypted: bool,
}

pub fn ensure_segment_size_is_valid(size: u64) -> AofResult<()> {
    if size < MIN_SEGMENT_SIZE || size > MAX_SEGMENT_SIZE {
        return Err(AofError::InvalidSegmentSize(format!(
            "Segment size must be between {} and {} bytes",
            MIN_SEGMENT_SIZE, MAX_SEGMENT_SIZE
        )));
    }
    Ok(())
}

pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

/// Builder for AofConfig to provide a fluent interface
#[derive(Debug, Clone)]
pub struct AofConfigBuilder {
    config: AofConfig,
}

impl AofConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: AofConfig::default(),
        }
    }

    pub fn segment_size(mut self, size: u64) -> Self {
        self.config.segment_size = size;
        self
    }

    pub fn flush_strategy(mut self, strategy: FlushStrategy) -> Self {
        self.config.flush_strategy = strategy;
        self
    }

    pub fn segment_cache_size(mut self, size: usize) -> Self {
        self.config.segment_cache_size = size;
        self
    }

    pub fn enable_compression(mut self, enabled: bool) -> Self {
        self.config.enable_compression = enabled;
        self
    }

    pub fn enable_encryption(mut self, enabled: bool) -> Self {
        self.config.enable_encryption = enabled;
        self
    }

    pub fn ttl_seconds(mut self, ttl: Option<u64>) -> Self {
        self.config.ttl_seconds = ttl;
        self
    }

    pub fn pre_allocate_threshold(mut self, threshold: f32) -> Self {
        self.config.pre_allocate_threshold = threshold;
        self
    }

    pub fn pre_allocate_enabled(mut self, enabled: bool) -> Self {
        self.config.pre_allocate_enabled = enabled;
        self
    }

    pub fn archive_enabled(mut self, enabled: bool) -> Self {
        self.config.archive_enabled = enabled;
        self
    }

    pub fn archive_compression_level(mut self, level: i32) -> Self {
        self.config.archive_compression_level = level;
        self
    }

    pub fn archive_threshold(mut self, threshold: u64) -> Self {
        self.config.archive_threshold = threshold;
        self
    }

    pub fn index_path<S: Into<String>>(mut self, path: S) -> Self {
        self.config.index_path = Some(path.into());
        self
    }

    pub fn build(self) -> AofConfig {
        self.config
    }
}

impl Default for AofConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
