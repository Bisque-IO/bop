use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};
use std::path::PathBuf;

const SEGMENT_SIZE_MIN_LIMIT: u64 = 64 * 1024; // 64 KiB
const SEGMENT_SIZE_MAX_LIMIT: u64 = u32::MAX as u64; // ~4 GiB
const DEFAULT_SEGMENT_MIN_BYTES: u64 = SEGMENT_SIZE_MIN_LIMIT;
const DEFAULT_SEGMENT_MAX_BYTES: u64 = SEGMENT_SIZE_MIN_LIMIT;
const DEFAULT_SEGMENT_TARGET_BYTES: u64 = SEGMENT_SIZE_MIN_LIMIT;

#[inline]
fn floor_power_of_two(value: u64) -> u64 {
    if value == 0 {
        0
    } else {
        let shift = 63_u32 - value.leading_zeros();
        1_u64 << shift
    }
}

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

const DEFAULT_MAX_UNFLUSHED_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB
const DEFAULT_FLUSH_WATERMARK_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB
const DEFAULT_FLUSH_INTERVAL_MS: u64 = 5; // 5 milliseconds
const DEFAULT_PREALLOCATE_SEGMENTS: u16 = 2;

/// Logical identifier for a segment file.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SegmentId(pub u64);

impl SegmentId {
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }

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
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct RecordId(pub u64);

impl RecordId {
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }

    #[inline]
    pub const fn from_parts(segment_index: u32, segment_offset: u32) -> Self {
        Self(((segment_index as u64) << 32) | segment_offset as u64)
    }

    #[inline]
    pub const fn segment_index(self) -> u32 {
        (self.0 >> 32) as u32
    }

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdStrategy {
    /// Monotonically increasing counter per stream.
    Monotonic,
    /// Record identifiers derived from the global byte index within the log.
    ByteIndex,
}

impl Default for IdStrategy {
    fn default() -> Self {
        Self::Monotonic
    }
}

/// Compression policy applied when sealing segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    None,
    Lz4,
    Zstd,
}

impl Default for Compression {
    fn default() -> Self {
        Self::None
    }
}

/// Retention policy that governs how many finalized segments are kept on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionPolicy {
    KeepAll,
    Time { ttl_seconds: u64 },
    Size { max_total_bytes: u64 },
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::KeepAll
    }
}

/// Compaction policy applied to cold segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionPolicy {
    Disabled,
    HeadTrim,
    MergeSeekableZstd,
}

impl Default for CompactionPolicy {
    fn default() -> Self {
        Self::Disabled
    }
}

/// Configuration for background flush behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlushConfig {
    /// Bytes appended to a segment before scheduling a flush.
    pub flush_watermark_bytes: u64,
    /// Maximum time to wait before forcing a flush when the watermark is not reached (milliseconds).
    pub flush_interval_ms: u64,
    /// Maximum number of bytes that may be acknowledged but not yet durable.
    pub max_unflushed_bytes: u64,
}

impl Default for FlushConfig {
    fn default() -> Self {
        Self {
            flush_watermark_bytes: DEFAULT_FLUSH_WATERMARK_BYTES,
            flush_interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
            max_unflushed_bytes: DEFAULT_MAX_UNFLUSHED_BYTES,
        }
    }
}

/// Primary configuration surface for an AOF instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AofConfig {
    /// Root directory that contains metadata/ and segments/ subdirectories.
    pub root_dir: PathBuf,
    /// Lower bound for dynamically sized segments (bytes).
    pub segment_min_bytes: u64,
    /// Upper bound for a single segment file (bytes).
    pub segment_max_bytes: u64,
    /// Soft target for rollover before reaching `segment_max_bytes`.
    pub segment_target_bytes: u64,
    /// Number of segments to keep preallocated for fast rollover.
    pub preallocate_segments: u16,
    /// Strategy for assigning record identifiers.
    pub id_strategy: IdStrategy,
    /// Compression applied when finalizing segments.
    pub compression: Compression,
    /// Retention policy for finalized segments.
    pub retention: RetentionPolicy,
    /// Compaction policy used for cold segments.
    pub compaction: CompactionPolicy,
    /// Flush tuning parameters.
    pub flush: FlushConfig,
    /// Enable sparse per-segment indexes when finalizing.
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
            self.preallocate_segments = 1;
        }

        self
    }
}

impl Display for AofConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AofConfig(root_dir={:?}, segment_min_bytes={}, segment_max_bytes={}, segment_target_bytes={}, preallocate_segments={}, id_strategy={:?}, compression={:?}, retention={:?}, compaction={:?}, flush_watermark_bytes={}, flush_interval_ms={}, max_unflushed_bytes={}, enable_sparse_index={})",
            self.root_dir,
            self.segment_min_bytes,
            self.segment_max_bytes,
            self.segment_target_bytes,
            self.preallocate_segments,
            self.id_strategy,
            self.compression,
            self.retention,
            self.compaction,
            self.flush.flush_watermark_bytes,
            self.flush.flush_interval_ms,
            self.flush.max_unflushed_bytes,
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
