use crc64fast_nvme::Digest as Crc64Digest;
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
    /// Folded CRC64-NVME checksum of the record data stored as u32.
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
        let checksum64 = Self::compute_crc64(data);
        let checksum = Self::fold_crc64(checksum64);

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
        let computed = Self::fold_crc64(Self::compute_crc64(data));
        computed == self.checksum
    }

    /// Calculate hash for integrity checking
    fn calculate_hash(data: &[u8], id: u64) -> u64 {
        let mut hash = id;
        for &byte in data {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }

    fn compute_crc64(data: &[u8]) -> u64 {
        let mut digest = Crc64Digest::new();
        digest.write(data);
        digest.sum64()
    }

    fn fold_crc64(sum: u64) -> u32 {
        let folded = sum ^ (sum >> 32);
        ((folded ^ (folded >> 16)) & 0xFFFF_FFFF) as u32
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

/// Trailer written after each record to enable reverse iteration and crash-safe validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct RecordTrailer {
    /// Mirror of the folded CRC64 checksum stored in the header.
    pub checksum: u32,
    /// Mirror of the record payload size in bytes.
    pub size: u32,
    /// Distance in bytes from this trailer back to the previous trailer (0 for the first record).
    pub prev_span: u64,
    /// CRC64-NVME over the trailer fields (excluding this checksum field).
    pub trailer_crc64: u64,
}

impl RecordTrailer {
    pub const fn size() -> usize {
        mem::size_of::<Self>()
    }

    pub fn new(header: &RecordHeader, prev_span: u64) -> Self {
        let mut trailer = RecordTrailer {
            checksum: header.checksum,
            size: header.size,
            prev_span,
            trailer_crc64: 0,
        };

        trailer.trailer_crc64 = trailer.compute_crc();
        trailer
    }

    fn compute_crc(&self) -> u64 {
        let mut digest = Crc64Digest::new();
        digest.write(&self.checksum.to_le_bytes());
        digest.write(&self.size.to_le_bytes());
        digest.write(&self.prev_span.to_le_bytes());
        digest.sum64()
    }

    pub fn validate(&self, header: &RecordHeader, expected_prev_span: Option<u64>) -> bool {
        if self.checksum != header.checksum || self.size != header.size {
            return false;
        }

        if let Some(span) = expected_prev_span {
            if self.prev_span != span {
                return false;
            }
        }

        self.compute_crc() == self.trailer_crc64
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

/// Segment header written at the beginning of every segment file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct SegmentHeader {
    pub magic: u32,
    pub version: u16,
    pub flags: u16,
    pub base_id: u64,
    pub created_at: u64,
    pub generation: u64,
    pub writer_epoch: u64,
    pub header_checksum: u64,
}

impl SegmentHeader {
    pub const MAGIC: u32 = 0x5345_4730; // "SEG0"
    pub const VERSION: u16 = 1;

    pub const fn size() -> usize {
        mem::size_of::<Self>()
    }

    pub fn new(
        base_id: u64,
        created_at: u64,
        generation: u64,
        writer_epoch: u64,
        flags: u16,
    ) -> Self {
        let mut header = SegmentHeader {
            magic: Self::MAGIC,
            version: Self::VERSION,
            flags,
            base_id,
            created_at,
            generation,
            writer_epoch,
            header_checksum: 0,
        };

        header.header_checksum = header.compute_checksum();
        header
    }

    fn compute_checksum(&self) -> u64 {
        let mut digest = Crc64Digest::new();
        digest.write(&self.magic.to_le_bytes());
        digest.write(&self.version.to_le_bytes());
        digest.write(&self.flags.to_le_bytes());
        digest.write(&self.base_id.to_le_bytes());
        digest.write(&self.created_at.to_le_bytes());
        digest.write(&self.generation.to_le_bytes());
        digest.write(&self.writer_epoch.to_le_bytes());
        digest.sum64()
    }

    pub fn verify(&self) -> bool {
        self.magic == Self::MAGIC
            && self.version == Self::VERSION
            && self.compute_checksum() == self.header_checksum
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

/// Flush checkpoint stamp appended after durable flush boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct FlushCheckpointStamp {
    pub magic: u32,
    pub version: u16,
    pub flags: u16,
    pub durable_offset: u64,
    pub last_record_id: u64,
    pub data_crc64: u64,
    pub stamp_crc64: u64,
}

impl FlushCheckpointStamp {
    pub const MAGIC: u32 = 0x0F1_7C5_04; // "FCP\0"
    pub const VERSION: u16 = 1;

    pub const fn size() -> usize {
        mem::size_of::<Self>()
    }

    pub fn new(durable_offset: u64, last_record_id: u64, data_crc64: u64) -> Self {
        let mut stamp = FlushCheckpointStamp {
            magic: Self::MAGIC,
            version: Self::VERSION,
            flags: 0,
            durable_offset,
            last_record_id,
            data_crc64,
            stamp_crc64: 0,
        };

        stamp.stamp_crc64 = stamp.compute_crc();
        stamp
    }

    fn compute_crc(&self) -> u64 {
        let mut digest = Crc64Digest::new();
        digest.write(&self.magic.to_le_bytes());
        digest.write(&self.version.to_le_bytes());
        digest.write(&self.flags.to_le_bytes());
        digest.write(&self.durable_offset.to_le_bytes());
        digest.write(&self.last_record_id.to_le_bytes());
        digest.write(&self.data_crc64.to_le_bytes());
        digest.sum64()
    }

    pub fn verify(&self) -> bool {
        self.magic == Self::MAGIC
            && self.version == Self::VERSION
            && self.compute_crc() == self.stamp_crc64
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

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
    /// CRC64-NVME checksum of all record data in the segment
    pub data_checksum: u64,
    /// Version for compatibility
    pub version: u8,
    /// Reserved for future use
    pub reserved: [u8; 7],
}

impl SegmentMetadataRecord {
    pub const MAGIC: u32 = 0xABCDEF02;
    pub const BYTE_SIZE: usize = 4 + (5 * 8) + 1 + 7; // fixed-size fields under bincode

    /// Create a new segment metadata record
    pub fn new(
        base_id: u64,
        last_record_id: u64,
        record_count: u64,
        data_checksum: u64,
        finalized_at: u64,
    ) -> Self {
        Self {
            magic: Self::MAGIC,
            record_count,
            last_record_id,
            base_id,
            finalized_at,
            data_checksum,
            version: 1,
            reserved: [0; 7],
        }
    }

    /// Check if this is a valid metadata record
    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version == 1
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_header_serialization_roundtrip() {
        let data = b"hello world";
        let header = RecordHeader::new(42, data);

        let encoded = header.to_bytes().expect("serialize header");
        let decoded = RecordHeader::from_bytes(&encoded).expect("deserialize header");

        assert_eq!(decoded.id, header.id);
        assert_eq!(decoded.size, header.size);
        assert_eq!(decoded.version, header.version);
        assert_eq!(decoded.flags, header.flags);
        assert_eq!(decoded.reserved, header.reserved);
        assert_eq!(decoded.timestamp, header.timestamp);
        assert_eq!(decoded.checksum, header.checksum);
        assert!(decoded.verify_checksum(data));
    }

    #[test]
    fn record_header_checksum_validation() {
        let data = b"checksum test";
        let header = RecordHeader::new(7, data);

        assert!(header.verify_checksum(data));

        let mut corrupted = data.to_vec();
        corrupted[0] ^= 0xFF;
        assert!(!header.verify_checksum(&corrupted));
    }

    #[test]
    fn segment_metadata_record_validity() {
        let record = SegmentMetadataRecord::new(10, 20, 11, 0xDEADBEEF, 123);
        assert!(record.is_valid());
        assert_eq!(record.base_id, 10);
        assert_eq!(record.last_record_id, 20);
        assert_eq!(record.record_count, 11);
        assert_eq!(record.data_checksum, 0xDEADBEEF);
    }

    #[test]
    fn record_trailer_validation() {
        let data = b"trailer";
        let header = RecordHeader::new(5, data);
        let trailer = RecordTrailer::new(&header, 0);
        assert!(trailer.validate(&header, Some(0)));

        let mut corrupted = trailer;
        corrupted.checksum ^= 0xFFFF;
        assert!(!corrupted.validate(&header, Some(0)));
    }

    #[test]
    fn segment_header_roundtrip() {
        let header = SegmentHeader::new(42, 1234, 1, 0, 0);
        let bytes = header.to_bytes().expect("serialize header");
        let decoded = SegmentHeader::from_bytes(&bytes).expect("deserialize header");
        assert!(decoded.verify());
        assert_eq!(decoded.base_id, header.base_id);
        assert_eq!(decoded.created_at, header.created_at);
    }

    #[test]
    fn flush_checkpoint_stamp_roundtrip() {
        let stamp = FlushCheckpointStamp::new(512, 99, 0xAA55AA55);
        assert!(stamp.verify());

        let bytes = stamp.to_bytes().expect("serialize stamp");
        let decoded = FlushCheckpointStamp::from_bytes(&bytes).expect("deserialize stamp");
        assert!(decoded.verify());
        assert_eq!(decoded.durable_offset, 512);
        assert_eq!(decoded.last_record_id, 99);
        assert_eq!(decoded.data_crc64, 0xAA55AA55);
    }
    #[test]
    fn segment_footer_roundtrip_and_verification() {
        let footer = SegmentFooter::new(128, 64, 3, 0xAA55AA55AA55AA55u64, 0x1122EEFFu64);
        assert!(footer.verify());

        let bytes = footer.serialize().expect("serialize footer");
        assert_eq!(bytes.len(), SegmentFooter::size());

        let decoded = SegmentFooter::deserialize(&bytes).expect("deserialize footer");
        assert_eq!(decoded.index_offset, 128);
        assert_eq!(decoded.index_size, 64);
        assert_eq!(decoded.record_count, 3);
        assert_eq!(decoded.data_checksum, 0xAA55AA55AA55AA55u64);
        assert_eq!(decoded.index_checksum, 0x1122EEFFu64);
        assert!(decoded.verify());
    }

    #[test]
    fn segment_footer_read_from_end_succeeds() {
        let mut payload = vec![0u8; 256];
        payload[10] = 0xAB;
        payload[200] = 0xCD;

        let footer = SegmentFooter::new(192, 32, 4, 0xCAFEBABEu64, 0xDEADBEEFu64);
        let footer_bytes = footer.serialize().expect("serialize footer");
        payload.extend_from_slice(&footer_bytes);

        let parsed = SegmentFooter::read_from_end(&payload).expect("parse footer from payload");
        assert_eq!(parsed.record_count, 4);
        assert_eq!(parsed.index_offset, 192);
        assert!(parsed.verify());
    }

    #[test]
    fn segment_footer_read_from_end_rejects_short_buffer() {
        let data = vec![0u8; SegmentFooter::size() - 1];
        let err = SegmentFooter::read_from_end(&data).expect_err("expected footer read failure");
        assert!(matches!(err, AofError::CorruptedRecord(_)));
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
    /// CRC64-NVME checksum of the segment's raw bytes
    pub checksum: u64,
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
