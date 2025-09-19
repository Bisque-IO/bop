use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};
use std::path::PathBuf;

const DEFAULT_SEGMENT_MAX_BYTES: u64 = 256 * 1024 * 1024; // 256 MiB
const DEFAULT_SEGMENT_TARGET_BYTES: u64 = 224 * 1024 * 1024; // 224 MiB
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
    /// Hard ceiling for a single segment file (bytes).
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
    /// Returns a copy of the configuration with `segment_target_bytes` clamped to `segment_max_bytes`.
    pub fn normalized(mut self) -> Self {
        if self.segment_target_bytes > self.segment_max_bytes {
            self.segment_target_bytes = self.segment_max_bytes;
        }
        self
    }
}

impl Display for AofConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AofConfig(root_dir={:?}, segment_max_bytes={}, segment_target_bytes={}, preallocate_segments={}, id_strategy={:?}, compression={:?}, retention={:?}, compaction={:?}, flush_watermark_bytes={}, flush_interval_ms={}, max_unflushed_bytes={}, enable_sparse_index={})",
            self.root_dir,
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
        assert!(cfg.segment_target_bytes <= cfg.segment_max_bytes);
        assert!(cfg.flush.max_unflushed_bytes >= cfg.flush.flush_watermark_bytes);
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
