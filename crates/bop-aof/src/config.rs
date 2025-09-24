use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};
use std::path::PathBuf;

/// Minimum allowed segment size (64 KiB).
///
/// Segments smaller than this limit can lead to excessive metadata overhead
/// and poor I/O performance due to frequent file operations.
const SEGMENT_SIZE_MIN_LIMIT: u64 = 64 * 1024; // 64 KiB

/// Maximum allowed segment size (~4 GiB).
///
/// Limited by u32::MAX to ensure efficient indexing and memory mapping.
/// Larger segments may also impact recovery time and memory usage.
const SEGMENT_SIZE_MAX_LIMIT: u64 = u32::MAX as u64; // ~4 GiB

/// Default minimum segment size.
const DEFAULT_SEGMENT_MIN_BYTES: u64 = SEGMENT_SIZE_MIN_LIMIT;

/// Default maximum segment size.
const DEFAULT_SEGMENT_MAX_BYTES: u64 = SEGMENT_SIZE_MIN_LIMIT;

/// Default target segment size for rollover decisions.
const DEFAULT_SEGMENT_TARGET_BYTES: u64 = SEGMENT_SIZE_MIN_LIMIT;

/// Computes the largest power of two that is less than or equal to the input value.
///
/// This function is used to align segment sizes to power-of-two boundaries,
/// which optimizes memory mapping and I/O operations.
///
/// # Arguments
///
/// * `value` - The input value to floor to a power of two
///
/// # Returns
///
/// The largest power of two ≤ `value`, or 0 if `value` is 0
#[inline]
fn floor_power_of_two(value: u64) -> u64 {
    if value == 0 {
        0
    } else {
        let shift = 63_u32 - value.leading_zeros();
        1_u64 << shift
    }
}

/// Clamps a value to the given range and rounds to the nearest power of two.
///
/// This function ensures that segment size configurations are both within
/// acceptable bounds and aligned to power-of-two boundaries for optimal
/// performance.
///
/// # Arguments
///
/// * `value` - The input value to clamp and align
/// * `min` - Minimum allowed value
/// * `max` - Maximum allowed value
///
/// # Returns
///
/// A power of two within the range [min, max] that is closest to `value`
#[inline]
fn clamp_power_of_two(value: u64, min: u64, max: u64) -> u64 {
    let clamped = value.clamp(min, max);
    if clamped.is_power_of_two() {
        return clamped;
    }

    let lower = floor_power_of_two(clamped).max(min);
    let upper = (lower << 1).min(max).max(min);

    if clamped - lower <= upper.saturating_sub(clamped) {
        lower
    } else {
        upper
    }
}

/// Default maximum bytes that can be acknowledged but not yet flushed to disk.
///
/// This backpressure limit prevents the system from accepting more writes
/// than it can durably persist, maintaining flow control under load.
const DEFAULT_MAX_UNFLUSHED_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB

/// Default number of bytes to accumulate before triggering a flush operation.
///
/// Larger watermarks improve batching efficiency but increase latency.
/// Smaller watermarks reduce latency but may impact throughput.
const DEFAULT_FLUSH_WATERMARK_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB

/// Default maximum time to wait before forcing a flush (milliseconds).
///
/// Ensures bounded write latency even when the watermark is not reached,
/// providing consistent durability guarantees.
const DEFAULT_FLUSH_INTERVAL_MS: u64 = 5; // 5 milliseconds

/// Default number of segments to keep preallocated for fast rollover.
///
/// Preallocated segments eliminate allocation delays during high-throughput
/// periods but consume additional disk space.
const DEFAULT_PREALLOCATE_SEGMENTS: u16 = 2;

/// Logical identifier for a segment file.
///
/// Segment IDs are monotonically increasing identifiers that uniquely identify
/// segment files within an AOF instance. They are used for ordering, indexing,
/// and file naming throughout the system.
///
/// # Properties
///
/// - **Monotonic**: IDs always increase, enabling chronological ordering
/// - **Dense**: No gaps in the sequence under normal operation
/// - **Persistent**: IDs are stable across restarts and recovery
/// - **Compact**: Fits in a u64 for efficient storage and transmission
///
/// # Example
///
/// ```rust
/// use bop_aof::SegmentId;
///
/// let id = SegmentId::new(42);
/// let next = id.next();
/// assert_eq!(next.as_u64(), 43);
/// ```
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SegmentId(pub u64);

impl SegmentId {
    /// Creates a new segment ID from a raw u64 value.
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the segment ID as a u64.
    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the segment ID as a u32.
    ///
    /// # Panics
    ///
    /// May truncate if the segment ID exceeds u32::MAX.
    #[inline]
    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }

    /// Returns the next segment ID in sequence.
    ///
    /// # Overflow Behavior
    ///
    /// Wraps around to 0 if called on SegmentId(u64::MAX).
    #[inline]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl From<u64> for SegmentId {
    #[inline]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<SegmentId> for u64 {
    #[inline]
    fn from(value: SegmentId) -> Self {
        value.0
    }
}

impl Display for SegmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// Logical identifier for a record within the AOF.
///
/// Record IDs encode both the segment index and offset within that segment,
/// enabling efficient lookups and addressing. The ID format packs both
/// components into a single u64 for space efficiency.
///
/// # Encoding Format
///
/// ```text
/// |  32 bits   |  32 bits  |
/// | Segment ID | Offset    |
/// |------------|-----------|
/// | High bits  | Low bits  |
/// ```
///
/// # Properties
///
/// - **Composite**: Encodes both segment and offset information
/// - **Efficient**: Single u64 for fast comparisons and storage
/// - **Ordered**: Natural ordering follows write order within segments
/// - **Addressable**: Direct mapping to physical storage location
///
/// # Example
///
/// ```rust
/// use bop_aof::RecordId;
///
/// let record_id = RecordId::from_parts(5, 1024);
/// assert_eq!(record_id.segment_index(), 5);
/// assert_eq!(record_id.segment_offset(), 1024);
/// ```
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct RecordId(pub u64);

impl RecordId {
    /// Creates a new record ID from a raw u64 value.
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the record ID as a u64.
    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the record ID as a u32 (truncated).
    ///
    /// # Note
    ///
    /// This truncates the high 32 bits, losing segment index information.
    /// Use with caution.
    #[inline]
    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }

    /// Creates a record ID from segment index and byte offset components.
    ///
    /// # Arguments
    ///
    /// * `segment_index` - The segment number (stored in high 32 bits)
    /// * `segment_offset` - Byte offset within the segment (stored in low 32 bits)
    #[inline]
    pub const fn from_parts(segment_index: u32, segment_offset: u32) -> Self {
        Self(((segment_index as u64) << 32) | segment_offset as u64)
    }

    /// Extracts the segment index from the record ID.
    ///
    /// Returns the high 32 bits, representing which segment contains this record.
    #[inline]
    pub const fn segment_index(self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// Extracts the byte offset within the segment from the record ID.
    ///
    /// Returns the low 32 bits, representing the byte offset within the segment.
    #[inline]
    pub const fn segment_offset(self) -> u32 {
        self.0 as u32
    }
}

impl From<u64> for RecordId {
    #[inline]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<RecordId> for u64 {
    #[inline]
    fn from(value: RecordId) -> Self {
        value.0
    }
}

impl Display for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// Strategy used to derive record identifiers.
///
/// Different ID strategies provide trade-offs between simplicity, performance,
/// and addressing capabilities. The choice affects how records are identified
/// and retrieved throughout the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdStrategy {
    /// Monotonically increasing counter per stream.
    ///
    /// Simple sequential numbering that provides stable, predictable IDs.
    /// Best for applications that don't need direct offset addressing.
    Monotonic,

    /// Record identifiers derived from the global byte index within the log.
    ///
    /// IDs correspond to byte offsets, enabling direct offset-based addressing
    /// and efficient range queries. Useful for applications requiring precise
    /// positioning within the log.
    ByteIndex,
}

impl Default for IdStrategy {
    fn default() -> Self {
        Self::Monotonic
    }
}

/// Compression policy applied when sealing segments.
///
/// Different compression algorithms provide various trade-offs between
/// compression ratio, CPU usage, and decompression speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    /// No compression applied.
    ///
    /// Fastest option with no CPU overhead, but uses maximum storage space.
    /// Best for high-performance workloads with ample storage.
    None,

    /// LZ4 compression algorithm.
    ///
    /// Provides fast compression/decompression with moderate compression ratios.
    /// Good balance between performance and storage efficiency.
    Lz4,

    /// Zstandard (zstd) compression algorithm.
    ///
    /// Higher compression ratios than LZ4 with configurable compression levels.
    /// Best for storage-constrained environments where CPU usage is acceptable.
    Zstd,
}

impl Default for Compression {
    fn default() -> Self {
        Self::None
    }
}

/// Retention policy that governs how many finalized segments are kept on disk.
///
/// Retention policies automatically remove old segments based on different
/// criteria, helping manage disk space usage over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionPolicy {
    /// Keep all segments indefinitely.
    ///
    /// No automatic cleanup is performed. Suitable for systems with unlimited
    /// storage or manual cleanup procedures.
    KeepAll,

    /// Remove segments older than the specified time-to-live.
    ///
    /// Segments are deleted when their creation time exceeds the TTL threshold.
    /// Provides predictable data lifecycle management.
    Time { ttl_seconds: u64 },

    /// Remove segments when total storage exceeds the specified limit.
    ///
    /// Oldest segments are deleted first when the size threshold is exceeded.
    /// Provides bounded disk usage with FIFO eviction.
    Size { max_total_bytes: u64 },
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::KeepAll
    }
}

/// Compaction policy applied to cold segments.
///
/// Compaction policies optimize storage efficiency and access patterns
/// for segments that are no longer actively written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionPolicy {
    /// No compaction is performed.
    ///
    /// Segments remain in their original format. Simplest policy with
    /// no CPU overhead but potentially suboptimal storage usage.
    Disabled,

    /// Remove deleted or obsolete records from the segment head.
    ///
    /// Reclaims space by trimming unused prefixes, improving storage
    /// efficiency while maintaining segment structure.
    HeadTrim,

    /// Merge multiple segments with seekable Zstd compression.
    ///
    /// Combines small segments into larger, compressed units with
    /// seek-friendly compression. Maximizes storage efficiency but
    /// requires more CPU resources.
    MergeSeekableZstd,
}

impl Default for CompactionPolicy {
    fn default() -> Self {
        Self::Disabled
    }
}

/// Configuration for background flush behaviour.
///
/// Controls when and how data is flushed from memory to durable storage.
/// These settings balance write latency, throughput, and durability guarantees.
///
/// # Performance Considerations
///
/// - **Higher watermarks**: Better batching, higher throughput, increased latency
/// - **Lower watermarks**: Lower latency, more frequent I/O, reduced throughput
/// - **More concurrency**: Higher throughput under load, more resource usage
/// - **Shorter intervals**: More predictable latency, potential performance impact
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlushConfig {
    /// Bytes appended to a segment before scheduling a flush.
    ///
    /// Larger values improve I/O efficiency through better batching but
    /// increase write latency. Smaller values reduce latency but may
    /// decrease throughput.
    pub flush_watermark_bytes: u64,

    /// Maximum time to wait before forcing a flush when the watermark is not reached (milliseconds).
    ///
    /// Provides an upper bound on write latency even when write volume
    /// is insufficient to trigger watermark-based flushes. Essential for
    /// maintaining consistent durability guarantees.
    pub flush_interval_ms: u64,

    /// Maximum number of bytes that may be acknowledged but not yet durable.
    ///
    /// Acts as a backpressure mechanism to prevent the system from accepting
    /// writes faster than it can durably persist them. When exceeded, new
    /// writes will block until the backlog decreases.
    pub max_unflushed_bytes: u64,

    /// Maximum number of concurrent flush tasks dispatched to the blocking pool.
    ///
    /// Higher values can improve throughput under heavy write loads but
    /// consume more system resources. Should be tuned based on available
    /// I/O bandwidth and CPU capacity.
    pub max_concurrent_flushes: usize,
}

impl Default for FlushConfig {
    fn default() -> Self {
        Self {
            flush_watermark_bytes: DEFAULT_FLUSH_WATERMARK_BYTES,
            flush_interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
            max_unflushed_bytes: DEFAULT_MAX_UNFLUSHED_BYTES,
            max_concurrent_flushes: 1,
        }
    }
}

/// Primary configuration surface for an AOF instance.
///
/// Defines all operational parameters for an individual AOF instance, including
/// storage layout, segment sizing, durability policies, and performance tuning.
///
/// # Configuration Philosophy
///
/// The configuration is designed around these principles:
/// - **Power-of-two alignment**: Segment sizes are automatically rounded to powers of two
/// - **Validated constraints**: Invalid configurations are normalized to safe defaults
/// - **Performance defaults**: Default values are optimized for common use cases
/// - **Extensible**: New policies can be added without breaking existing configurations
///
/// # Example
///
/// ```rust
/// use bop_aof::{AofConfig, Compression, RetentionPolicy};
/// use std::path::PathBuf;
///
/// let config = AofConfig {
///     root_dir: PathBuf::from("/data/aof"),
///     segment_max_bytes: 128 * 1024 * 1024, // 128 MiB
///     compression: Compression::Zstd,
///     retention: RetentionPolicy::Time { ttl_seconds: 86400 }, // 1 day
///     ..AofConfig::default()
/// }.normalized();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AofConfig {
    /// Root directory that contains metadata/ and segments/ subdirectories.
    ///
    /// This directory will be created if it doesn't exist. The AOF will create
    /// subdirectories for segments, manifests, and temporary files.
    pub root_dir: PathBuf,

    /// Lower bound for dynamically sized segments (bytes).
    ///
    /// Prevents segments from becoming too small, which would increase metadata
    /// overhead. Will be normalized to a power of two within valid limits.
    pub segment_min_bytes: u64,

    /// Upper bound for a single segment file (bytes).
    ///
    /// Limits maximum segment size to control memory usage and recovery time.
    /// Will be normalized to a power of two within valid limits.
    pub segment_max_bytes: u64,

    /// Soft target for rollover before reaching `segment_max_bytes`.
    ///
    /// Provides a buffer zone for graceful segment transitions without hitting
    /// hard limits. Should be between min and max segment sizes.
    pub segment_target_bytes: u64,

    /// Number of segments to keep preallocated for fast rollover.
    ///
    /// Preallocated segments eliminate file creation delays during high-throughput
    /// periods. Set to 0 to disable preallocation.
    pub preallocate_segments: u16,

    /// Maximum number of concurrent segment preallocation tasks.
    ///
    /// Controls resource usage during segment preallocation. Higher values
    /// speed up preallocation but consume more I/O bandwidth.
    pub preallocate_concurrency: u16,

    /// Strategy for assigning record identifiers.
    ///
    /// Affects how records are identified and addressed throughout the system.
    /// Choose based on application access patterns.
    pub id_strategy: IdStrategy,

    /// Compression applied when finalizing segments.
    ///
    /// Trade-offs between CPU usage, storage space, and access latency.
    /// Applied only to finalized (read-only) segments.
    pub compression: Compression,

    /// Retention policy for finalized segments.
    ///
    /// Controls automatic cleanup of old segments to manage disk usage.
    /// Choose based on data lifecycle requirements.
    pub retention: RetentionPolicy,

    /// Compaction policy used for cold segments.
    ///
    /// Optimizes storage efficiency for segments that are rarely accessed.
    /// Applies to segments that have been finalized and aged.
    pub compaction: CompactionPolicy,

    /// Flush tuning parameters.
    ///
    /// Controls when and how data is persisted to storage, affecting both
    /// performance and durability characteristics.
    pub flush: FlushConfig,

    /// Enable sparse per-segment indexes when finalizing.
    ///
    /// Indexes improve random access performance at the cost of additional
    /// storage space and segment finalization time.
    pub enable_sparse_index: bool,
}

impl Default for AofConfig {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from("./data/aof"),
            segment_min_bytes: DEFAULT_SEGMENT_MIN_BYTES,
            segment_max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
            segment_target_bytes: DEFAULT_SEGMENT_TARGET_BYTES,
            preallocate_segments: DEFAULT_PREALLOCATE_SEGMENTS,
            preallocate_concurrency: 1,
            id_strategy: IdStrategy::default(),
            compression: Compression::default(),
            retention: RetentionPolicy::default(),
            compaction: CompactionPolicy::default(),
            flush: FlushConfig::default(),
            enable_sparse_index: true,
        }
    }
}

impl AofConfig {
    /// Returns a copy of the configuration with segment sizing rounded into the configured power-of-two window.
    ///
    /// This method validates and normalizes all configuration parameters to ensure
    /// they are within acceptable bounds and aligned to optimal values. It should
    /// be called after loading configuration from external sources.
    ///
    /// # Normalization Rules
    ///
    /// - **Segment sizes**: Rounded to nearest power of two within valid limits
    /// - **Size relationships**: `min <= target <= max` is enforced
    /// - **Concurrency limits**: Clamped to reasonable bounds based on other settings
    /// - **Zero values**: Replaced with sensible defaults
    ///
    /// # Returns
    ///
    /// A new configuration with all parameters validated and normalized.
    ///
    /// # Example
    ///
    /// ```rust
    /// let config = AofConfig {
    ///     segment_min_bytes: 100_000,  // Will be rounded to 65_536
    ///     segment_max_bytes: 0,        // Will use default
    ///     ..AofConfig::default()
    /// }.normalized();
    /// ```
    pub fn normalized(mut self) -> Self {
        let min_raw = if self.segment_min_bytes == 0 {
            DEFAULT_SEGMENT_MIN_BYTES
        } else {
            self.segment_min_bytes
        };
        self.segment_min_bytes =
            clamp_power_of_two(min_raw, SEGMENT_SIZE_MIN_LIMIT, SEGMENT_SIZE_MAX_LIMIT);

        let max_raw = if self.segment_max_bytes == 0 {
            DEFAULT_SEGMENT_MAX_BYTES
        } else {
            self.segment_max_bytes
        };
        self.segment_max_bytes =
            clamp_power_of_two(max_raw, self.segment_min_bytes, SEGMENT_SIZE_MAX_LIMIT);

        if self.segment_max_bytes < self.segment_min_bytes {
            self.segment_max_bytes = self.segment_min_bytes;
        }

        let target_raw = if self.segment_target_bytes == 0 {
            self.segment_min_bytes
        } else {
            self.segment_target_bytes
        };
        self.segment_target_bytes =
            clamp_power_of_two(target_raw, self.segment_min_bytes, self.segment_max_bytes);

        if self.preallocate_segments == 0 {
            self.preallocate_concurrency = 0;
        } else {
            if self.preallocate_concurrency == 0 {
                self.preallocate_concurrency = 1;
            }
            if self.preallocate_concurrency > self.preallocate_segments {
                self.preallocate_concurrency = self.preallocate_segments;
            }
        }

        if self.flush.max_concurrent_flushes == 0 {
            self.flush.max_concurrent_flushes = 1;
        }

        self
    }
}

impl Display for AofConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AofConfig(root_dir={:?}, segment_min_bytes={}, segment_max_bytes={}, segment_target_bytes={}, preallocate_segments={}, preallocate_concurrency={}, id_strategy={:?}, compression={:?}, retention={:?}, compaction={:?}, flush_watermark_bytes={}, flush_interval_ms={}, max_unflushed_bytes={}, max_concurrent_flushes={}, enable_sparse_index={})",
            self.root_dir,
            self.segment_min_bytes,
            self.segment_max_bytes,
            self.segment_target_bytes,
            self.preallocate_segments,
            self.preallocate_concurrency,
            self.id_strategy,
            self.compression,
            self.retention,
            self.compaction,
            self.flush.flush_watermark_bytes,
            self.flush.flush_interval_ms,
            self.flush.max_unflushed_bytes,
            self.flush.max_concurrent_flushes,
            self.enable_sparse_index
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_reasonable() {
        let cfg = AofConfig::default();
        assert!(cfg.segment_min_bytes.is_power_of_two());
        assert!(cfg.segment_max_bytes.is_power_of_two());
        assert!(cfg.segment_target_bytes.is_power_of_two());
        assert!(cfg.segment_min_bytes >= SEGMENT_SIZE_MIN_LIMIT);
        assert!(cfg.segment_max_bytes <= SEGMENT_SIZE_MAX_LIMIT);
        assert!(cfg.segment_min_bytes <= cfg.segment_target_bytes);
        assert!(cfg.segment_target_bytes <= cfg.segment_max_bytes);
        assert!(cfg.flush.max_unflushed_bytes >= cfg.flush.flush_watermark_bytes);
    }

    #[test]
    fn normalized_clamps_segment_bounds() {
        let cfg = AofConfig {
            segment_min_bytes: 10 * 1024 * 1024,
            segment_max_bytes: 3 * 1024 * 1024 * 1024,
            segment_target_bytes: 123 * 1024 * 1024,
            ..AofConfig::default()
        }
        .normalized();

        assert_eq!(cfg.segment_min_bytes, 8 * 1024 * 1024);
        assert_eq!(cfg.segment_max_bytes, SEGMENT_SIZE_MAX_LIMIT);
        assert_eq!(cfg.segment_target_bytes, 128 * 1024 * 1024);
    }

    #[test]
    fn serde_round_trip() {
        let cfg = AofConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let decoded: AofConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, decoded);
    }

    #[test]
    fn segment_id_next() {
        let id = SegmentId::new(41);
        assert_eq!(SegmentId::new(42), id.next());
    }

    #[test]
    fn record_id_pack_unpack() {
        let packed = RecordId::from_parts(7, 4096);
        assert_eq!(7, packed.segment_index());
        assert_eq!(4096, packed.segment_offset());
        assert_eq!(packed, RecordId::new(((7u64) << 32) | 4096));
    }
}

/// Checks if manifest logging is enabled via environment variable.
///
/// Reads the `AOF2_MANIFEST_LOG_ENABLED` environment variable to determine
/// whether manifest write-ahead logging should be active. Accepts "1" or
/// "true" (case-insensitive) as enabled values.
///
/// This allows runtime control over manifest logging for debugging,
/// monitoring, and development scenarios without code changes.
pub fn aof2_manifest_log_enabled() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| {
        std::env::var("AOF2_MANIFEST_LOG_ENABLED")
            .ok()
            .map(|value| {
                let trimmed = value.trim();
                trimmed == "1" || trimmed.eq_ignore_ascii_case("true")
            })
            .unwrap_or(false)
    })
}

/// Checks if manifest-only logging mode is enabled via environment variable.
///
/// Reads the `AOF2_MANIFEST_LOG_ONLY` environment variable to determine
/// whether only manifest operations should be logged, without actual
/// data persistence. Accepts "1" or "true" (case-insensitive).
///
/// This testing mode allows validation of manifest logic without
/// the overhead and side effects of full data persistence.
pub fn aof2_manifest_log_only() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| {
        std::env::var("AOF2_MANIFEST_LOG_ONLY")
            .ok()
            .map(|value| {
                let trimmed = value.trim();
                trimmed == "1" || trimmed.eq_ignore_ascii_case("true")
            })
            .unwrap_or(false)
    })
}
