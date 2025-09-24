use std::collections::{HashMap, VecDeque};
use std::convert::TryInto;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering as AtomicOrdering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crc64fast_nvme::Digest;
use futures::future::join_all;
use hdrhistogram::Histogram;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tokio::runtime::Handle;
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::{JoinHandle, spawn_blocking};
use tokio::time::sleep as tokio_sleep;
use tracing::{trace, warn};
use zstd::stream::{read::Decoder as ZstdDecoder, write::Encoder as ZstdEncoder};

use crate::config::{SegmentId, aof2_manifest_log_enabled, aof2_manifest_log_only};
use crate::error::{AofError, AofResult};
use crate::fs::Layout;
use crate::manifest::{
    CHUNK_HEADER_LEN, ChunkHeader, ManifestLogReader, ManifestLogWriter, ManifestLogWriterConfig,
    ManifestRecordPayload, SEAL_RESERVED_BYTES,
};
use crate::metrics::manifest_replay::{ManifestReplayMetrics, ManifestReplaySnapshot};
use crate::segment::{
    RECORD_HEADER_SIZE, SEGMENT_FOOTER_SIZE, SEGMENT_HEADER_SIZE, Segment, SegmentFooter,
};

use super::tier0::{ActivationGrant, InstanceId, Tier0DropEvent};
use super::tier2::{
    Tier2DeleteComplete, Tier2DeleteRequest, Tier2Event, Tier2FetchComplete, Tier2FetchFailed,
    Tier2FetchRequest, Tier2Handle, Tier2Metadata, Tier2UploadComplete, Tier2UploadDescriptor,
};

/// Returns current time as milliseconds since Unix epoch.
///
/// Provides consistent timestamp generation for Tier 1 operations,
/// including manifest entries and cache lifecycle tracking.
/// Falls back to 0 if system time is unavailable.
fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

const HYDRATION_REASON_MISSING_WARM: u32 = 1;

/// Configuration for the Tier 1 SSD-based cache.
///
/// Tier 1 provides compressed on-disk storage with background
/// hydration and compression workers.
#[derive(Debug, Clone)]
pub struct Tier1Config {
    /// Maximum bytes allowed for compressed storage
    pub max_bytes: u64,
    /// Number of background worker threads
    pub worker_threads: usize,
    /// Compression level (higher = better compression)
    pub compression_level: i32,
    /// Maximum retries for failed operations
    pub max_retries: u32,
    /// Backoff delay between retries
    pub retry_backoff: Duration,
    /// Manifest log configuration
    pub manifest_log: ManifestLogWriterConfig,
    /// Only use manifest log, skip actual storage
    pub manifest_log_only: bool,
}

impl Tier1Config {
    pub fn new(max_bytes: u64) -> Self {
        let mut manifest_log = ManifestLogWriterConfig::default();
        manifest_log.enabled = aof2_manifest_log_enabled();
        let manifest_log_only = aof2_manifest_log_only();
        if manifest_log_only {
            manifest_log.enabled = true;
        }
        Self {
            max_bytes,
            worker_threads: 2,
            compression_level: 3,
            max_retries: 3,
            retry_backoff: Duration::from_millis(50),
            manifest_log,
            manifest_log_only,
        }
    }

    pub fn with_worker_threads(mut self, workers: usize) -> Self {
        self.worker_threads = workers.max(1);
        self
    }

    pub fn with_compression_level(mut self, level: i32) -> Self {
        self.compression_level = level;
        self
    }

    pub fn with_retry_policy(mut self, max_retries: u32, backoff: Duration) -> Self {
        self.max_retries = max_retries.max(1);
        self.retry_backoff = backoff;
        self
    }

    pub fn with_manifest_log_config(mut self, mut config: ManifestLogWriterConfig) -> Self {
        config.enabled = config.enabled && config.chunk_capacity_bytes > 0;
        self.manifest_log = config;
        self
    }

    pub fn with_manifest_log_only(mut self, enabled: bool) -> Self {
        self.manifest_log_only = enabled;
        if enabled {
            self.manifest_log.enabled = true;
        }
        self
    }
    pub fn with_manifest_log_enabled(mut self, enabled: bool) -> Self {
        self.manifest_log.enabled = enabled;
        self
    }
}

/// Breakdown of segment residency across tiers.
///
/// Tracks where segments currently reside in the storage hierarchy.
#[derive(Debug, Default, Clone, Copy)]
pub struct Tier1ResidencyGauge {
    /// Segments currently in Tier 0 (memory)
    pub resident_in_tier0: u64,
    /// Segments staged for Tier 0 activation
    pub staged_for_tier0: u64,
    /// Segments uploaded to Tier 2 (remote)
    pub uploaded_to_tier2: u64,
    /// Segments requiring recovery operations
    pub needs_recovery: u64,
}

/// Point-in-time metrics snapshot for Tier 1 cache.
///
/// Provides comprehensive observability into cache performance,
/// queue depths, latencies, and residency distribution.
#[derive(Debug, Default, Clone, Copy)]
pub struct Tier1MetricsSnapshot {
    /// Total bytes stored in compressed form
    pub stored_bytes: u64,
    /// Total compression jobs completed (lifetime)
    pub compression_jobs: u64,
    /// Total compression failures (lifetime)
    pub compression_failures: u64,
    /// Total hydration failures (lifetime)
    pub hydration_failures: u64,
    /// Current compression queue depth
    pub compression_queue_depth: u32,
    /// Current hydration queue depth
    pub hydration_queue_depth: u32,
    /// Median hydration latency in milliseconds
    pub hydration_latency_p50_ms: u64,
    /// 90th percentile hydration latency in milliseconds
    pub hydration_latency_p90_ms: u64,
    /// 99th percentile hydration latency in milliseconds
    pub hydration_latency_p99_ms: u64,
    /// Number of hydration latency samples
    pub hydration_latency_samples: u64,
    /// Cross-tier residency breakdown
    pub residency: Tier1ResidencyGauge,
}

/// Thread-safe metrics collection for Tier 1 cache.
///
/// Uses atomic counters and protected histogram for concurrent
/// metric updates from compression and hydration workers.
#[derive(Debug)]
pub struct Tier1Metrics {
    /// Current compressed storage size
    stored_bytes: AtomicU64,
    /// Total compression jobs completed
    compression_jobs: AtomicU64,
    /// Total compression failures
    compression_failures: AtomicU64,
    /// Total hydration failures
    hydration_failures: AtomicU64,
    /// Current compression queue depth
    compression_queue_depth: AtomicU32,
    /// Current hydration queue depth
    hydration_queue_depth: AtomicU32,
    /// Segments currently in Tier 0
    residency_resident_in_tier0: AtomicU64,
    /// Segments staged for Tier 0
    residency_staged_for_tier0: AtomicU64,
    /// Segments uploaded to Tier 2
    residency_uploaded_to_tier2: AtomicU64,
    /// Segments needing recovery
    residency_needs_recovery: AtomicU64,
    /// Hydration latency distribution (protected by mutex)
    hydration_latency: Mutex<Histogram<u64>>,
}

impl Default for Tier1Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Tier1Metrics {
    fn new() -> Self {
        let histogram =
            Histogram::new_with_bounds(1, 3_600_000, 2).expect("hydrate latency histogram bounds");
        Self {
            stored_bytes: AtomicU64::new(0),
            compression_jobs: AtomicU64::new(0),
            compression_failures: AtomicU64::new(0),
            hydration_failures: AtomicU64::new(0),
            compression_queue_depth: AtomicU32::new(0),
            hydration_queue_depth: AtomicU32::new(0),
            residency_resident_in_tier0: AtomicU64::new(0),
            residency_staged_for_tier0: AtomicU64::new(0),
            residency_uploaded_to_tier2: AtomicU64::new(0),
            residency_needs_recovery: AtomicU64::new(0),
            hydration_latency: Mutex::new(histogram),
        }
    }

    fn snapshot(&self) -> Tier1MetricsSnapshot {
        let hist = self.hydration_latency.lock();
        let samples = hist.len() as u64;
        let percentile = |p: f64| -> u64 {
            if samples == 0 {
                0
            } else {
                hist.value_at_percentile(p)
            }
        };
        Tier1MetricsSnapshot {
            stored_bytes: self.stored_bytes.load(AtomicOrdering::Relaxed),
            compression_jobs: self.compression_jobs.load(AtomicOrdering::Relaxed),
            compression_failures: self.compression_failures.load(AtomicOrdering::Relaxed),
            hydration_failures: self.hydration_failures.load(AtomicOrdering::Relaxed),
            compression_queue_depth: self.compression_queue_depth.load(AtomicOrdering::Relaxed),
            hydration_queue_depth: self.hydration_queue_depth.load(AtomicOrdering::Relaxed),
            hydration_latency_p50_ms: percentile(50.0),
            hydration_latency_p90_ms: percentile(90.0),
            hydration_latency_p99_ms: percentile(99.0),
            hydration_latency_samples: samples,
            residency: Tier1ResidencyGauge {
                resident_in_tier0: self
                    .residency_resident_in_tier0
                    .load(AtomicOrdering::Relaxed),
                staged_for_tier0: self
                    .residency_staged_for_tier0
                    .load(AtomicOrdering::Relaxed),
                uploaded_to_tier2: self
                    .residency_uploaded_to_tier2
                    .load(AtomicOrdering::Relaxed),
                needs_recovery: self.residency_needs_recovery.load(AtomicOrdering::Relaxed),
            },
        }
    }

    fn set_stored_bytes(&self, bytes: u64) {
        self.stored_bytes.store(bytes, AtomicOrdering::Relaxed);
    }

    fn incr_compression_jobs(&self) {
        self.compression_jobs.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_compression_failures(&self) {
        self.compression_failures
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_hydration_failures(&self) {
        self.hydration_failures
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_compression_queue(&self) {
        self.compression_queue_depth
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn decr_compression_queue(&self) {
        self.compression_queue_depth
            .fetch_update(AtomicOrdering::AcqRel, AtomicOrdering::Relaxed, |value| {
                value.checked_sub(1)
            })
            .ok();
    }

    fn add_hydration_queue(&self, count: usize) {
        if count > 0 {
            self.hydration_queue_depth
                .fetch_add(count as u32, AtomicOrdering::Relaxed);
        }
    }

    fn sub_hydration_queue(&self, count: usize) {
        if count == 0 {
            return;
        }
        self.hydration_queue_depth
            .fetch_update(AtomicOrdering::AcqRel, AtomicOrdering::Relaxed, |value| {
                value.checked_sub(count as u32)
            })
            .ok();
    }

    fn record_hydration_latency(&self, latency: Duration) {
        let value = latency.as_millis().max(1) as u64;
        let mut histogram = self.hydration_latency.lock();
        let _ = histogram.record(value);
    }

    fn rebuild_residency<I>(&self, states: I)
    where
        I: IntoIterator<Item = Tier1ResidencyState>,
    {
        self.residency_resident_in_tier0
            .store(0, AtomicOrdering::Relaxed);
        self.residency_staged_for_tier0
            .store(0, AtomicOrdering::Relaxed);
        self.residency_uploaded_to_tier2
            .store(0, AtomicOrdering::Relaxed);
        self.residency_needs_recovery
            .store(0, AtomicOrdering::Relaxed);
        for state in states {
            self.inc_residency(state);
        }
    }

    fn record_residency_transition(
        &self,
        previous: Option<Tier1ResidencyState>,
        next: Option<Tier1ResidencyState>,
    ) {
        if let Some(state) = previous {
            self.dec_residency(state);
        }
        if let Some(state) = next {
            self.inc_residency(state);
        }
    }

    fn inc_residency(&self, state: Tier1ResidencyState) {
        self.cell_for(state).fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn dec_residency(&self, state: Tier1ResidencyState) {
        let cell = self.cell_for(state);
        let _ = cell.fetch_update(AtomicOrdering::AcqRel, AtomicOrdering::Relaxed, |value| {
            value.checked_sub(1)
        });
    }

    fn cell_for(&self, state: Tier1ResidencyState) -> &AtomicU64 {
        match state {
            Tier1ResidencyState::ResidentInTier0 => &self.residency_resident_in_tier0,
            Tier1ResidencyState::StagedForTier0 => &self.residency_staged_for_tier0,
            Tier1ResidencyState::UploadedToTier2 => &self.residency_uploaded_to_tier2,
            Tier1ResidencyState::NeedsRecovery => &self.residency_needs_recovery,
        }
    }
}

/// Current residency state of a segment in the tiered storage system.
///
/// Tracks where segments currently exist and their availability status
/// across the three-tier cache hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier1ResidencyState {
    /// Segment is currently loaded in Tier 0 (memory cache)
    ResidentInTier0,
    /// Segment is staged for activation to Tier 0
    StagedForTier0,
    /// Segment has been uploaded to Tier 2 (remote storage)
    UploadedToTier2,
    /// Segment requires recovery operations
    NeedsRecovery,
}

impl Default for Tier1ResidencyState {
    fn default() -> Self {
        Tier1ResidencyState::StagedForTier0
    }
}

/// Manifest entry tracking a compressed segment in Tier 1 storage.
///
/// Contains all metadata needed to locate, verify, and decompress
/// segments stored in the Tier 1 compressed cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Unique segment identifier
    pub segment_id: SegmentId,
    /// Base offset in the original AOF log
    pub base_offset: u64,
    /// Number of records at compression time
    pub base_record_count: u64,
    /// Segment creation timestamp (epoch millis)
    pub created_at: i64,
    /// Segment sealing timestamp (epoch millis)
    pub sealed_at: i64,
    /// CRC32 checksum for integrity verification
    pub checksum: u32,
    /// Extended metadata identifier
    #[serde(default)]
    pub ext_id: u64,
    /// Whether a flush operation failed for this segment
    #[serde(default)]
    pub flush_failure: bool,
    /// Reserved bytes for future extensions
    #[serde(default)]
    pub reserved: [u8; SEAL_RESERVED_BYTES],
    /// Uncompressed segment size in bytes
    pub original_bytes: u64,
    /// Compressed segment size in bytes
    pub compressed_bytes: u64,
    /// Path to the compressed segment file
    pub compressed_path: String,
    /// Optional compression dictionary for better ratios
    #[serde(default)]
    pub dictionary: Option<String>,
    /// Offset index for fast random access within segment
    #[serde(default)]
    pub offset_index: Vec<u32>,
    /// Current residency state across storage tiers
    #[serde(default)]
    pub residency: Tier1ResidencyState,
    /// Tier 2 remote storage metadata if uploaded
    #[serde(default)]
    pub tier2: Option<Tier2Metadata>,
    /// Last access timestamp for LRU eviction
    #[serde(default)]
    pub last_access_epoch_ms: u64,
}

impl ManifestEntry {
    fn touch(&mut self) {
        self.last_access_epoch_ms = current_epoch_ms();
    }
}

/// In-memory manifest tracking all Tier 1 compressed segments.
///
/// Maintains segment metadata, compression paths, and residency state.
/// Persisted as JSON for crash recovery.
#[derive(Debug)]
struct Tier1Manifest {
    /// Filesystem path for manifest persistence
    path: PathBuf,
    /// All tracked manifest entries by segment ID
    entries: HashMap<SegmentId, ManifestEntry>,
    /// Whether to persist manifest to disk
    persist_json: bool,
}

impl Tier1Manifest {
    fn load(path: PathBuf, persist_json: bool) -> AofResult<Self> {
        if path.exists() {
            let data = fs::read(&path).map_err(AofError::from)?;
            if data.is_empty() {
                return Ok(Self {
                    path,
                    entries: HashMap::new(),
                    persist_json,
                });
            }
            let entries: Vec<ManifestEntry> = serde_json::from_slice(&data)
                .map_err(|err| AofError::Serialization(err.to_string()))?;
            let map = entries
                .into_iter()
                .map(|entry| (entry.segment_id, entry))
                .collect();
            Ok(Self {
                path,
                entries: map,
                persist_json,
            })
        } else {
            Ok(Self {
                path,
                entries: HashMap::new(),
                persist_json,
            })
        }
    }

    fn save(&self) -> AofResult<Vec<u8>> {
        let entries: Vec<&ManifestEntry> = self.entries.values().collect();
        let json = serde_json::to_vec_pretty(&entries)
            .map_err(|err| AofError::Serialization(err.to_string()))?;
        if !self.persist_json {
            return Ok(json);
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| AofError::Io(io::Error::new(io::ErrorKind::Other, "missing parent")))?;
        let temp = NamedTempFile::new_in(parent).map_err(AofError::from)?;
        temp.as_file().write_all(&json).map_err(AofError::from)?;
        temp.as_file().sync_all().map_err(AofError::from)?;
        temp.persist(&self.path)
            .map_err(|err| AofError::from(err.error))?;
        Ok(json)
    }

    fn get(&self, segment_id: SegmentId) -> Option<&ManifestEntry> {
        self.entries.get(&segment_id)
    }

    fn get_mut(&mut self, segment_id: SegmentId) -> Option<&mut ManifestEntry> {
        self.entries.get_mut(&segment_id)
    }

    fn insert(&mut self, entry: ManifestEntry) -> Option<ManifestEntry> {
        self.entries.insert(entry.segment_id, entry)
    }

    fn remove(&mut self, segment_id: SegmentId) -> Option<ManifestEntry> {
        self.entries.remove(&segment_id)
    }

    fn all(&self) -> impl Iterator<Item = &ManifestEntry> {
        self.entries.values()
    }

    fn replace_entries(&mut self, entries: Vec<ManifestEntry>) {
        self.entries = entries
            .into_iter()
            .map(|entry| (entry.segment_id, entry))
            .collect();
    }
}

/// Result of trimming operations on compressed storage.
struct TrimResult {
    /// Total bytes freed by trimming
    trimmed_bytes: u64,
    /// Number of chunks/segments removed
    chunk_count: usize,
}

/// Per-instance state for Tier 1 operations.
///
/// Manages compressed storage, manifest tracking, and LRU eviction
/// for a single AOF instance.
struct Tier1InstanceState {
    /// Unique instance identifier
    instance_id: InstanceId,
    /// Filesystem layout for storage paths
    layout: Layout,
    /// Manifest tracking compressed segments
    manifest: Tier1Manifest,
    /// Optional manifest log writer for durability
    manifest_log: Option<ManifestLogWriter>,
    /// Current bytes used by compressed segments
    used_bytes: u64,
    /// LRU queue for eviction ordering
    lru: VecDeque<SegmentId>,
    /// Metrics for manifest replay operations
    replay_metrics: Arc<ManifestReplayMetrics>,
}

/// Result of inserting a new segment into Tier 1 storage.
#[derive(Debug)]
struct InsertOutcome {
    /// Previous entry that was replaced (if any)
    replaced: Option<ManifestEntry>,
    /// Entries evicted to make space
    evicted: Vec<ManifestEntry>,
}

impl Tier1InstanceState {
    fn new(instance_id: InstanceId, layout: Layout, config: &Tier1Config) -> AofResult<Self> {
        let persist_json = !config.manifest_log_only;
        let manifest_path = layout.warm_dir().join("manifest.json");
        let mut manifest = Tier1Manifest::load(manifest_path, persist_json)?;
        let replay_metrics = Arc::new(ManifestReplayMetrics::new());
        if config.manifest_log.enabled {
            if let Some(entries) =
                Self::replay_manifest_from_log(&layout, instance_id, replay_metrics.as_ref())?
            {
                manifest.replace_entries(entries);
            }
        }
        let used_bytes: u64 = manifest
            .entries
            .values()
            .map(|entry| entry.compressed_bytes)
            .sum();
        let mut order: Vec<_> = manifest
            .entries
            .values()
            .map(|entry| (entry.last_access_epoch_ms, entry.segment_id))
            .collect();
        order.sort_by_key(|(timestamp, _)| *timestamp);
        let mut lru = VecDeque::new();
        for (_, segment_id) in order {
            lru.push_back(segment_id);
        }
        let manifest_log = if config.manifest_log.enabled {
            let mut writer_config = config.manifest_log.clone();
            writer_config.enabled = true;
            let writer = ManifestLogWriter::new(
                layout.manifest_dir().to_path_buf(),
                instance_id.get(),
                writer_config,
            )?;
            Some(writer)
        } else {
            None
        };
        Ok(Self {
            instance_id,
            layout,
            manifest,
            manifest_log,
            used_bytes,
            lru,
            replay_metrics,
        })
    }

    /// Persists manifest state to storage and write-ahead log.
    ///
    /// Serializes current manifest entries and writes to both
    /// persistent storage and manifest write-ahead log for
    /// durability and recovery purposes.
    fn persist_manifest(&mut self) -> AofResult<()> {
        let snapshot = self.manifest.save()?;
        self.log_manifest_snapshot(&snapshot);
        Ok(())
    }

    /// Writes manifest snapshot to write-ahead log.
    ///
    /// Records Tier 1 manifest state changes in the manifest
    /// log for recovery and debugging. Provides audit trail
    /// of cache state transitions.
    fn log_manifest_snapshot(&mut self, snapshot: &[u8]) {
        if let Some(writer) = self.manifest_log.as_mut() {
            if let Err(err) = writer.append_now(
                SegmentId::new(0),
                0,
                ManifestRecordPayload::Tier1Snapshot {
                    json: snapshot.to_vec(),
                },
            ) {
                warn!(
                    instance = self.instance_id.get(),
                    "manifest snapshot append failed: {err}"
                );
                return;
            }
            if let Err(err) = writer.commit() {
                warn!(
                    instance = self.instance_id.get(),
                    "manifest snapshot commit failed: {err}"
                );
            }
        }
    }

    fn replay_manifest_from_log(
        layout: &Layout,
        instance_id: InstanceId,
        metrics: &ManifestReplayMetrics,
    ) -> AofResult<Option<Vec<ManifestEntry>>> {
        let start = Instant::now();
        let trim = Self::trim_manifest_chunks(layout, instance_id)?;
        metrics.record_journal_lag_bytes(trim.trimmed_bytes);
        metrics.record_chunk_count(trim.chunk_count);
        let reader =
            match ManifestLogReader::open(layout.manifest_dir().to_path_buf(), instance_id.get()) {
                Ok(reader) => reader,
                Err(err) => {
                    metrics.incr_corruption();
                    return Err(err);
                }
            };
        let mut snapshot = None;
        for record in reader.iter() {
            match record {
                Ok(record) => {
                    if let ManifestRecordPayload::Tier1Snapshot { json } = record.payload {
                        snapshot = Some(json);
                    }
                }
                Err(err) => {
                    metrics.incr_corruption();
                    return Err(err);
                }
            }
        }
        let elapsed = start.elapsed();
        metrics.record_chunk_lag(elapsed);
        if elapsed > Duration::from_millis(500) {
            trace!(
                instance = instance_id.get(),
                duration_ms = elapsed.as_millis(),
                trimmed_bytes = trim.trimmed_bytes,
                chunk_count = trim.chunk_count,
                "slow manifest replay"
            );
        }
        if let Some(json) = snapshot {
            let entries: Vec<ManifestEntry> = serde_json::from_slice(&json)
                .map_err(|err| AofError::Serialization(err.to_string()))?;
            return Ok(Some(entries));
        }
        Ok(None)
    }

    fn trim_manifest_chunks(layout: &Layout, instance_id: InstanceId) -> AofResult<TrimResult> {
        let stream_dir = layout
            .manifest_dir()
            .join(format!("{id:020}", id = instance_id.get()));
        if !stream_dir.exists() {
            return Ok(TrimResult {
                trimmed_bytes: 0,
                chunk_count: 0,
            });
        }
        let mut trimmed = 0u64;
        let mut chunk_count = 0usize;
        for entry in fs::read_dir(&stream_dir).map_err(AofError::from)? {
            let entry = entry.map_err(AofError::from)?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("mlog") {
                continue;
            }
            chunk_count += 1;
            let mut file = File::open(&path).map_err(AofError::from)?;
            let mut header_bytes = vec![0u8; CHUNK_HEADER_LEN];
            file.read_exact(&mut header_bytes).map_err(AofError::from)?;
            let header = ChunkHeader::read_from(&header_bytes)?;
            let file_len = file.metadata().map_err(AofError::from)?.len();
            let committed_len = header.committed_len as u64;
            if file_len > committed_len {
                drop(file);
                let trim = OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(AofError::from)?;
                trim.set_len(committed_len).map_err(AofError::from)?;
                trim.sync_data().map_err(AofError::from)?;
                trimmed += file_len - committed_len;
            }
        }
        Ok(TrimResult {
            trimmed_bytes: trimmed,
            chunk_count,
        })
    }

    fn log_manifest_record(
        &mut self,
        segment_id: SegmentId,
        logical_offset: u64,
        payload: ManifestRecordPayload,
    ) {
        if let Some(writer) = self.manifest_log.as_mut() {
            if let Err(err) = writer.append_now(segment_id, logical_offset, payload) {
                warn!(
                    instance = self.instance_id.get(),
                    segment = segment_id.as_u64(),
                    "manifest log append failed: {err}"
                );
                return;
            }
            if let Err(err) = writer.commit() {
                warn!(
                    instance = self.instance_id.get(),
                    segment = segment_id.as_u64(),
                    "manifest log commit failed: {err}"
                );
            }
        }
    }

    /// Updates LRU order for segment access.
    ///
    /// Moves segment to most recently used position in LRU queue
    /// and updates manifest entry timestamp. Called on segment
    /// access to maintain proper eviction ordering.
    fn touch(&mut self, segment_id: SegmentId) {
        self.lru.retain(|id| *id != segment_id);
        self.lru.push_back(segment_id);
        if let Some(entry) = self.manifest.get_mut(segment_id) {
            entry.touch();
        }
    }

    /// Inserts manifest entry with capacity management.
    ///
    /// Adds compressed segment entry to manifest, enforcing
    /// capacity limits through LRU eviction. Handles both
    /// new entries and updates to existing entries.
    ///
    /// Core cache insertion logic with automatic eviction.
    fn insert_entry(&mut self, entry: ManifestEntry, budget: u64) -> AofResult<InsertOutcome> {
        let mut evicted = Vec::new();
        let replaced = if let Some(existing) = self.manifest.insert(entry.clone()) {
            self.used_bytes = self.used_bytes.saturating_sub(existing.compressed_bytes);
            Some(existing)
        } else {
            None
        };
        self.used_bytes = self.used_bytes.saturating_add(entry.compressed_bytes);
        self.touch(entry.segment_id);
        while self.used_bytes > budget {
            if let Some(evict_id) = self.lru.pop_front() {
                if evict_id == entry.segment_id {
                    self.lru.push_back(evict_id);
                    break;
                }
                if let Some(evicted_entry) = self.manifest.remove(evict_id) {
                    let path = self.layout.warm_dir().join(&evicted_entry.compressed_path);
                    if let Err(err) = fs::remove_file(&path) {
                        warn!(path = %path.display(), "failed to remove warm file: {err}");
                    }
                    self.used_bytes = self
                        .used_bytes
                        .saturating_sub(evicted_entry.compressed_bytes);
                    evicted.push(evicted_entry);
                }
            } else {
                break;
            }
        }
        self.persist_manifest()?;
        self.log_manifest_record(
            entry.segment_id,
            entry.base_offset,
            ManifestRecordPayload::CompressionDone {
                compressed_bytes: entry.compressed_bytes,
                dictionary_id: 0,
            },
        );
        for evicted_entry in &evicted {
            self.log_manifest_record(
                evicted_entry.segment_id,
                evicted_entry.base_offset,
                ManifestRecordPayload::LocalEvicted {
                    reclaimed_bytes: evicted_entry.compressed_bytes,
                },
            );
        }
        Ok(InsertOutcome { replaced, evicted })
    }

    fn update_residency(
        &mut self,
        segment_id: SegmentId,
        residency: Tier1ResidencyState,
    ) -> AofResult<Option<Tier1ResidencyState>> {
        if let Some(entry) = self.manifest.get_mut(segment_id) {
            let previous = entry.residency;
            let (base_offset, payload) = {
                entry.residency = residency;
                entry.touch();
                let base_offset = entry.base_offset;
                let original_bytes = entry.original_bytes;
                let payload = match residency {
                    Tier1ResidencyState::ResidentInTier0 => {
                        Some(ManifestRecordPayload::HydrationDone {
                            resident_bytes: original_bytes,
                        })
                    }
                    Tier1ResidencyState::StagedForTier0 => {
                        Some(ManifestRecordPayload::LocalEvicted {
                            reclaimed_bytes: original_bytes,
                        })
                    }
                    Tier1ResidencyState::UploadedToTier2 => None,
                    Tier1ResidencyState::NeedsRecovery => {
                        Some(ManifestRecordPayload::HydrationFailed {
                            reason_code: HYDRATION_REASON_MISSING_WARM,
                        })
                    }
                };
                (base_offset, payload)
            };
            self.touch(segment_id);
            self.persist_manifest()?;
            if let Some(payload) = payload {
                self.log_manifest_record(segment_id, base_offset, payload);
            }
            Ok(Some(previous))
        } else {
            Ok(None)
        }
    }

    fn used_bytes(&self) -> u64 {
        self.used_bytes
    }
}

impl fmt::Debug for Tier1InstanceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tier1InstanceState")
            .field("instance_id", &self.instance_id)
            .field("layout", &self.layout)
            .field("manifest", &self.manifest)
            .field("used_bytes", &self.used_bytes)
            .field("lru_len", &self.lru.len())
            .finish()
    }
}

#[derive(Debug, Clone)]
/// Result of successfully hydrating a segment from Tier 1.
///
/// Contains all metadata and the path to the decompressed segment
/// file ready for Tier 0 activation.
pub struct HydrationOutcome {
    /// Instance that requested the hydration
    pub instance_id: InstanceId,
    /// Hydrated segment identifier
    pub segment_id: SegmentId,
    /// Path to the decompressed segment file
    pub path: PathBuf,
    /// Base offset in the original AOF log
    pub base_offset: u64,
    /// Record count at compression time
    pub base_record_count: u64,
    /// Segment creation timestamp
    pub created_at: i64,
    /// Segment sealing timestamp
    pub sealed_at: i64,
    /// Integrity checksum
    pub checksum: u32,
    /// Logical segment size in bytes
    pub logical_size: u32,
}

/// The Tier 1 SSD-based compressed cache for the tiered storage system.
///
/// Tier 1 provides compressed on-disk storage with background compression
/// workers and intelligent hydration based on Tier 0 activation requests.
///
/// ## Architecture
///
/// ```text
/// ┌─────────────────────────────────────────────┐
/// │              Tier1Cache                     │
/// ├─────────────────────────────────────────────┤
/// │ • Background Compression                    │
/// │ • Intelligent Hydration                     │
/// │ • LRU-based Eviction                        │
/// │ • Manifest Durability                       │
/// │ • Tier 2 Integration                        │
/// └─────────────────────────────────────────────┘
/// ```
///
/// ## Key Features
///
/// - **Compression**: Zstd compression with configurable levels
/// - **Hydration**: Fast decompression for Tier 0 activation
/// - **Manifest**: Durable metadata tracking with optional WAL
/// - **Eviction**: LRU-based space management with capacity limits
/// - **Integration**: Seamless coordination with Tier 0 and Tier 2
pub struct Tier1Cache {
    /// Core implementation and worker coordination
    inner: Arc<Tier1Inner>,
    /// Background dispatcher task handle
    dispatcher: Mutex<Option<JoinHandle<()>>>,
}

impl Tier1Cache {
    pub fn new(runtime: Handle, config: Tier1Config) -> AofResult<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let inner = Arc::new(Tier1Inner::new(runtime.clone(), config, tx));
        let dispatcher_inner = Arc::clone(&inner);
        let dispatcher = runtime.spawn(async move {
            dispatcher_inner.run(rx).await;
        });
        Ok(Self {
            inner,
            dispatcher: Mutex::new(Some(dispatcher)),
        })
    }

    pub fn metrics(&self) -> Tier1MetricsSnapshot {
        self.inner.metrics.snapshot()
    }

    pub fn register_instance(
        &self,
        instance_id: InstanceId,
        layout: Layout,
    ) -> AofResult<Tier1Instance> {
        self.inner.register_instance(instance_id, layout)?;
        Ok(Tier1Instance {
            inner: Arc::clone(&self.inner),
            instance_id,
        })
    }

    pub fn attach_tier2(&self, handle: Tier2Handle) {
        self.inner.attach_tier2(handle);
    }

    pub fn handle_tier2_events(&self, events: Vec<Tier2Event>) {
        if !events.is_empty() {
            self.inner.handle_tier2_events(events);
        }
    }

    pub fn handle_drop_events(&self, events: Vec<Tier0DropEvent>) {
        for event in events {
            if let Some(segment) = event.segment {
                if !event.was_sealed {
                    trace!(
                        segment = event.segment_id.as_u64(),
                        "skipping unsealed segment drop"
                    );
                    continue;
                }
                if let Err(err) = self
                    .inner
                    .enqueue_compression(event.instance_id, segment, None)
                {
                    warn!(
                        instance = event.instance_id.get(),
                        segment = event.segment_id.as_u64(),
                        "failed to enqueue compression: {err}"
                    );
                }
            } else {
                warn!(
                    instance = event.instance_id.get(),
                    segment = event.segment_id.as_u64(),
                    "drop event missing segment handle"
                );
            }
        }
    }

    pub fn update_residency(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
        residency: Tier1ResidencyState,
    ) -> AofResult<()> {
        self.inner
            .update_residency(instance_id, segment_id, residency)
    }

    pub async fn handle_activation_grants(
        &self,
        grants: Vec<ActivationGrant>,
    ) -> Vec<HydrationOutcome> {
        self.inner.process_hydration_requests_async(grants).await
    }

    pub fn manifest_snapshot(&self, instance_id: InstanceId) -> AofResult<Vec<ManifestEntry>> {
        self.inner.manifest_snapshot(instance_id)
    }
}

impl Drop for Tier1Cache {
    fn drop(&mut self) {
        let _ = self.inner.compression_tx.send(CompressionCommand::Shutdown);
        if let Some(handle) = self.dispatcher.lock().take() {
            handle.abort();
        }
    }
}

/// Per-instance view of the Tier 1 cache.
///
/// Provides instance-isolated operations while sharing the underlying
/// compression infrastructure and storage pool.
///
/// ## Usage Pattern
///
/// ```ignore
/// // Schedule compression of a sealed segment
/// instance.schedule_compression(segment)?;
///
/// // Update residency when segment moves between tiers
/// instance.update_residency(segment_id, Tier1ResidencyState::UploadedToTier2)?;
///
/// // Retrieve manifest information
/// let entry = instance.manifest_entry(segment_id)?;
/// ```
#[derive(Clone)]
pub struct Tier1Instance {
    /// Shared cache implementation
    inner: Arc<Tier1Inner>,
    /// Unique instance identifier
    instance_id: InstanceId,
}

impl Tier1Instance {
    pub fn instance_id(&self) -> InstanceId {
        self.instance_id
    }

    pub fn schedule_compression(&self, segment: Arc<Segment>) -> AofResult<()> {
        self.schedule_compression_with_ack(segment, None)
    }

    pub fn schedule_compression_with_ack(
        &self,
        segment: Arc<Segment>,
        ack: Option<oneshot::Sender<AofResult<()>>>,
    ) -> AofResult<()> {
        self.inner
            .enqueue_compression(self.instance_id, segment, ack)
    }

    pub fn update_residency(
        &self,
        segment_id: SegmentId,
        residency: Tier1ResidencyState,
    ) -> AofResult<()> {
        self.inner
            .update_residency(self.instance_id, segment_id, residency)
    }

    pub fn manifest_entry(&self, segment_id: SegmentId) -> AofResult<Option<ManifestEntry>> {
        self.inner.manifest_entry(self.instance_id, segment_id)
    }

    pub fn manifest_replay_metrics(&self) -> AofResult<ManifestReplaySnapshot> {
        self.inner.manifest_replay_metrics(self.instance_id)
    }

    pub fn hydrate_segment_blocking(
        &self,
        segment_id: SegmentId,
    ) -> AofResult<Option<HydrationOutcome>> {
        self.inner
            .hydrate_segment_blocking(self.instance_id, segment_id)
    }
}

/// Background compression job for worker threads.
struct CompressionJob {
    /// Instance requesting compression
    instance_id: InstanceId,
    /// Segment to compress
    segment: Arc<Segment>,
    /// Optional acknowledgment channel
    ack: Option<oneshot::Sender<AofResult<()>>>,
}

/// Core implementation of Tier 1 cache operations.
///
/// Coordinates compression workers, manifest management, and
/// integration with Tier 0 and Tier 2 systems.
#[derive(Debug)]
struct Tier1Inner {
    /// Tokio runtime handle for async operations
    runtime: Handle,
    config: Tier1Config,
    compression_tx: mpsc::UnboundedSender<CompressionCommand>,
    semaphore: Arc<Semaphore>,
    instances: Mutex<HashMap<InstanceId, Tier1InstanceState>>,
    metrics: Tier1Metrics,
    tier2: Mutex<Option<Tier2Handle>>,
}

impl Tier1Inner {
    fn new(
        runtime: Handle,
        config: Tier1Config,
        compression_tx: mpsc::UnboundedSender<CompressionCommand>,
    ) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.worker_threads.max(1)));
        Self {
            runtime,
            config,
            compression_tx,
            semaphore,
            instances: Mutex::new(HashMap::new()),
            metrics: Tier1Metrics::default(),
            tier2: Mutex::new(None),
        }
    }

    async fn run(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<CompressionCommand>) {
        while let Some(command) = rx.recv().await {
            match command {
                CompressionCommand::Compress(job) => {
                    let permit = match Arc::clone(&self.semaphore).acquire_owned().await {
                        Ok(permit) => permit,
                        Err(_) => break,
                    };
                    let inner = Arc::clone(&self);
                    let runtime = self.runtime.clone();
                    runtime.spawn(async move {
                        let _permit = permit;
                        inner.process_compression_job(job).await;
                    });
                }
                CompressionCommand::Shutdown => break,
            }
        }
    }

    fn register_instance(&self, instance_id: InstanceId, layout: Layout) -> AofResult<()> {
        let mut instances = self.instances.lock();
        if !instances.contains_key(&instance_id) {
            let state = Tier1InstanceState::new(instance_id, layout, &self.config)?;
            let residencies: Vec<Tier1ResidencyState> =
                state.manifest.all().map(|entry| entry.residency).collect();
            self.metrics.rebuild_residency(residencies);
            instances.insert(instance_id, state);
        }
        Ok(())
    }

    fn enqueue_compression(
        self: &Arc<Self>,
        instance_id: InstanceId,
        segment: Arc<Segment>,
        ack: Option<oneshot::Sender<AofResult<()>>>,
    ) -> AofResult<()> {
        if !segment.is_sealed() {
            let message = "cannot compress unsealed segment".to_string();
            if let Some(tx) = ack {
                let _ = tx.send(Err(AofError::InvalidState(message.clone())));
            }
            return Err(AofError::InvalidState(message));
        }
        if !self.instances.lock().contains_key(&instance_id) {
            let message = format!("Tier1 instance {:?} not registered", instance_id.get());
            if let Some(tx) = ack {
                let _ = tx.send(Err(AofError::InvalidState(message.clone())));
            }
            return Err(AofError::InvalidState(message));
        }
        let job = CompressionJob {
            instance_id,
            segment,
            ack,
        };
        match self.compression_tx.send(CompressionCommand::Compress(job)) {
            Ok(()) => {
                self.metrics.incr_compression_jobs();
                self.metrics.incr_compression_queue();
                Ok(())
            }
            Err(err) => {
                if let CompressionCommand::Compress(job) = err.0 {
                    if let Some(tx) = job.ack {
                        let _ = tx.send(Err(AofError::other("compression queue closed")));
                    }
                }
                Err(AofError::other("failed to enqueue compression job"))
            }
        }
    }

    fn attach_tier2(&self, handle: Tier2Handle) {
        *self.tier2.lock() = Some(handle);
    }

    fn tier2(&self) -> Option<Tier2Handle> {
        self.tier2.lock().clone()
    }

    fn handle_tier2_events(&self, events: Vec<Tier2Event>) {
        for event in events {
            match event {
                Tier2Event::UploadCompleted(info) => {
                    if let Err(err) = self.apply_upload_complete(info) {
                        warn!("tier2 upload complete handling failed: {err}");
                    }
                }
                Tier2Event::UploadFailed(failure) => {
                    warn!(
                        instance = failure.instance_id.get(),
                        segment = failure.segment_id.as_u64(),
                        "tier2 upload failed: {}",
                        failure.error
                    );
                }
                Tier2Event::DeleteCompleted(info) => {
                    if let Err(err) = self.apply_delete_complete(info) {
                        warn!("tier2 delete complete handling failed: {err}");
                    }
                }
                Tier2Event::DeleteFailed(failure) => {
                    warn!(
                        instance = failure.instance_id.get(),
                        segment = failure.segment_id.as_u64(),
                        "tier2 delete failed: {}",
                        failure.error
                    );
                }
                Tier2Event::FetchCompleted(info) => {
                    if let Err(err) = self.apply_fetch_complete(info) {
                        warn!("tier2 fetch complete handling failed: {err}");
                    }
                }
                Tier2Event::FetchFailed(failure) => {
                    self.metrics.incr_hydration_failures();
                    warn!(
                        instance = failure.instance_id.get(),
                        segment = failure.segment_id.as_u64(),
                        "tier2 fetch failed: {}",
                        failure.error
                    );
                    if let Err(err) = self.apply_fetch_failed(failure) {
                        warn!("tier2 fetch failure handling failed: {err}");
                    }
                }
            }
        }
    }

    fn apply_upload_complete(&self, info: Tier2UploadComplete) -> AofResult<()> {
        let mut instances = self.instances.lock();
        let state = instances.get_mut(&info.instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {} not registered",
                info.instance_id.get()
            ))
        })?;
        let mut previous_residency = None;
        if let Some(entry) = state.manifest.get_mut(info.segment_id) {
            let previous = entry.residency;
            let (base_offset, tier2_meta) = {
                entry.tier2 = Some(info.metadata.clone());
                entry.residency = Tier1ResidencyState::UploadedToTier2;
                entry.touch();
                (entry.base_offset, entry.tier2.clone())
            };
            previous_residency = Some(previous);
            state.touch(info.segment_id);
            state.persist_manifest()?;
            if let Some(metadata) = tier2_meta {
                state.log_manifest_record(
                    info.segment_id,
                    base_offset,
                    ManifestRecordPayload::UploadDone {
                        key: metadata.object_key,
                        etag: metadata.etag,
                    },
                );
            }
        }
        drop(instances);
        if let Some(previous) = previous_residency {
            self.metrics.record_residency_transition(
                Some(previous),
                Some(Tier1ResidencyState::UploadedToTier2),
            );
        }
        Ok(())
    }

    fn apply_delete_complete(&self, info: Tier2DeleteComplete) -> AofResult<()> {
        let mut instances = self.instances.lock();
        let state = instances.get_mut(&info.instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {} not registered",
                info.instance_id.get()
            ))
        })?;
        let base_offset = if let Some(entry) = state.manifest.get_mut(info.segment_id) {
            entry.tier2 = None;
            entry.touch();
            Some(entry.base_offset)
        } else {
            None
        };
        if let Some(base_offset) = base_offset {
            state.persist_manifest()?;
            state.log_manifest_record(
                info.segment_id,
                base_offset,
                ManifestRecordPayload::Tier2Deleted {
                    key: info.object_key.clone(),
                },
            );
        }
        Ok(())
    }

    fn apply_fetch_complete(&self, info: Tier2FetchComplete) -> AofResult<()> {
        let mut instances = self.instances.lock();
        let state = instances.get_mut(&info.instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {} not registered",
                info.instance_id.get()
            ))
        })?;
        let mut previous_residency = None;
        if let Some(entry) = state.manifest.get_mut(info.segment_id) {
            let previous = entry.residency;
            entry.residency = Tier1ResidencyState::StagedForTier0;
            entry.touch();
            let base_offset = entry.base_offset;
            state.persist_manifest()?;
            let resident_bytes = fs::metadata(&info.destination)
                .map(|meta| meta.len())
                .unwrap_or_default();
            state.log_manifest_record(
                info.segment_id,
                base_offset,
                ManifestRecordPayload::HydrationDone { resident_bytes },
            );
            previous_residency = Some(previous);
        }
        drop(instances);
        if let Some(previous) = previous_residency {
            self.metrics.record_residency_transition(
                Some(previous),
                Some(Tier1ResidencyState::StagedForTier0),
            );
        }
        Ok(())
    }

    fn apply_fetch_failed(&self, failure: Tier2FetchFailed) -> AofResult<()> {
        let mut instances = self.instances.lock();
        let state = instances.get_mut(&failure.instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {} not registered",
                failure.instance_id.get()
            ))
        })?;
        let mut previous_residency = None;
        if let Some(entry) = state.manifest.get_mut(failure.segment_id) {
            let previous = entry.residency;
            entry.residency = Tier1ResidencyState::NeedsRecovery;
            entry.touch();
            let base_offset = entry.base_offset;
            state.persist_manifest()?;
            state.log_manifest_record(
                failure.segment_id,
                base_offset,
                ManifestRecordPayload::HydrationFailed {
                    reason_code: HYDRATION_REASON_MISSING_WARM,
                },
            );
            previous_residency = Some(previous);
        }
        drop(instances);
        if let Some(previous) = previous_residency {
            self.metrics.record_residency_transition(
                Some(previous),
                Some(Tier1ResidencyState::NeedsRecovery),
            );
        }
        Ok(())
    }

    async fn process_compression_job(self: Arc<Self>, mut job: CompressionJob) {
        let max_attempts = self.config.max_retries.max(1);
        let mut attempt = 0;
        let instance_id = job.instance_id;
        let segment = job.segment.clone();
        let start = Instant::now();

        let final_result = loop {
            let inner = Arc::clone(&self);
            let segment_clone = segment.clone();
            let result = spawn_blocking(move || {
                inner.run_compression_job_blocking(instance_id, &segment_clone)
            })
            .await;

            match result {
                Ok(Ok(())) => break Ok(()),
                Ok(Err(err)) => {
                    attempt += 1;
                    if attempt >= max_attempts {
                        self.metrics.incr_compression_failures();
                        break Err(err);
                    }
                    tokio_sleep(self.config.retry_backoff * attempt as u32).await;
                }
                Err(join_err) => {
                    self.metrics.incr_compression_failures();
                    break Err(AofError::other(format!(
                        "compression task panicked: {join_err}"
                    )));
                }
            }
        };

        let attempts = attempt + 1;
        match &final_result {
            Ok(()) => tracing::event!(
                target: "aof2::tier1",
                tracing::Level::INFO,
                instance = instance_id.get(),
                segment = segment.id().as_u64(),
                attempts,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "tier1_compression_success"
            ),
            Err(err) => tracing::event!(
                target: "aof2::tier1",
                tracing::Level::WARN,
                instance = instance_id.get(),
                segment = segment.id().as_u64(),
                attempts,
                error = %err,
                "tier1_compression_failed"
            ),
        }

        self.metrics.decr_compression_queue();

        if let Some(ack) = job.ack.take() {
            let _ = ack.send(final_result);
        }
    }

    fn run_compression_job_blocking(
        &self,
        instance_id: InstanceId,
        segment: &Arc<Segment>,
    ) -> AofResult<()> {
        let (entry, footer) = self.compress_segment_blocking(instance_id, segment)?;
        self.finish_compression(instance_id, entry, footer)?;
        Ok(())
    }

    fn finish_compression(
        &self,
        instance_id: InstanceId,
        entry: ManifestEntry,
        footer: SegmentFooter,
    ) -> AofResult<()> {
        let tier2_handle = self.tier2();
        let (insert_outcome, stored_bytes, upload_descriptor, new_residency) = {
            let mut instances = self.instances.lock();
            let state = instances.get_mut(&instance_id).ok_or_else(|| {
                AofError::InvalidState(format!(
                    "Tier1 instance {:?} not registered",
                    instance_id.get()
                ))
            })?;
            let warm_path = state.layout.warm_dir().join(&entry.compressed_path);
            let descriptor = tier2_handle.as_ref().map(|_| Tier2UploadDescriptor {
                instance_id,
                segment_id: entry.segment_id,
                warm_path,
                sealed_at: entry.sealed_at,
                base_offset: entry.base_offset,
                base_record_count: entry.base_record_count,
                checksum: entry.checksum,
                compressed_bytes: entry.compressed_bytes,
                original_bytes: entry.original_bytes,
            });
            state.log_manifest_record(
                entry.segment_id,
                entry.base_offset,
                ManifestRecordPayload::SealSegment {
                    durable_bytes: footer.durable_bytes,
                    segment_crc64: footer.checksum as u64,
                    ext_id: entry.ext_id,
                    flush_failure: entry.flush_failure,
                    reserved: [0u8; SEAL_RESERVED_BYTES],
                },
            );
            let new_residency = entry.residency;
            let evicted = state.insert_entry(entry, self.config.max_bytes)?;
            let stored_bytes = state.used_bytes();
            (evicted, stored_bytes, descriptor, new_residency)
        };
        self.metrics.set_stored_bytes(stored_bytes);
        self.metrics.record_residency_transition(
            insert_outcome
                .replaced
                .as_ref()
                .map(|entry| entry.residency),
            Some(new_residency),
        );
        for evicted_entry in &insert_outcome.evicted {
            self.metrics
                .record_residency_transition(Some(evicted_entry.residency), None);
        }

        if let Some(tier2) = tier2_handle {
            if let Some(descriptor) = upload_descriptor {
                tier2
                    .schedule_upload(descriptor)
                    .map_err(AofError::rollover_failed)?;
            }
            for evicted_entry in &insert_outcome.evicted {
                if let Some(metadata) = &evicted_entry.tier2 {
                    let key = metadata.object_key.clone();
                    let request = Tier2DeleteRequest {
                        instance_id,
                        segment_id: evicted_entry.segment_id,
                        object_key: key.clone(),
                    };
                    if let Err(err) = tier2.schedule_delete(request) {
                        warn!(
                            instance = instance_id.get(),
                            segment = evicted_entry.segment_id.as_u64(),
                            "failed to schedule tier2 delete for evicted warm entry: {err}"
                        );
                    } else {
                        let mut instances = self.instances.lock();
                        if let Some(state) = instances.get_mut(&instance_id) {
                            state.log_manifest_record(
                                evicted_entry.segment_id,
                                evicted_entry.base_offset,
                                ManifestRecordPayload::Tier2DeleteQueued { key },
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn compress_segment_blocking(
        &self,
        instance_id: InstanceId,
        segment: &Arc<Segment>,
    ) -> AofResult<(ManifestEntry, SegmentFooter)> {
        let footer = read_segment_footer(segment)?;
        let layout = {
            let instances = self.instances.lock();
            let state = instances.get(&instance_id).ok_or_else(|| {
                AofError::InvalidState(format!(
                    "Tier1 instance {:?} not registered",
                    instance_id.get()
                ))
            })?;
            state.layout.clone()
        };

        let sealed_at = footer.sealed_at;
        let warm_path = layout.warm_file_path(segment.id(), segment.base_offset(), sealed_at);
        if warm_path.exists() {
            fs::remove_file(&warm_path).map_err(AofError::from)?;
        }
        let warm_file_name = warm_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                AofError::invalid_config(format!("invalid warm file name: {}", warm_path.display()))
            })?
            .to_string();
        let temp = NamedTempFile::new_in(layout.warm_dir()).map_err(AofError::from)?;
        {
            let mut source = File::open(segment.path()).map_err(AofError::from)?;
            let mut encoder = ZstdEncoder::new(temp.as_file(), self.config.compression_level)
                .map_err(|err| AofError::CompressionError(err.to_string()))?;
            encoder
                .include_checksum(true)
                .map_err(|err| AofError::CompressionError(err.to_string()))?;
            io::copy(&mut source, &mut encoder).map_err(AofError::from)?;
            encoder
                .finish()
                .map_err(|err| AofError::CompressionError(err.to_string()))?;
        }
        temp.as_file().sync_all().map_err(AofError::from)?;
        let metadata = temp.as_file().metadata().map_err(AofError::from)?;
        temp.persist(&warm_path)
            .map_err(|err| AofError::from(err.error))?;

        let offset_index = build_offset_index(segment)?;

        Ok((
            ManifestEntry {
                segment_id: segment.id(),
                base_offset: segment.base_offset(),
                base_record_count: segment.base_record_count(),
                created_at: segment.created_at(),
                sealed_at,
                checksum: footer.checksum,
                ext_id: footer.ext_id,
                flush_failure: footer.flush_failure,
                reserved: [0u8; SEAL_RESERVED_BYTES],
                original_bytes: segment.current_size() as u64,
                compressed_bytes: metadata.len(),
                compressed_path: warm_file_name,
                dictionary: None,
                offset_index,
                residency: Tier1ResidencyState::StagedForTier0,
                tier2: None,
                last_access_epoch_ms: current_epoch_ms(),
            },
            footer,
        ))
    }

    async fn process_hydration_requests_async(
        self: &Arc<Self>,
        grants: Vec<ActivationGrant>,
    ) -> Vec<HydrationOutcome> {
        let grant_count = grants.len();
        self.metrics.add_hydration_queue(grant_count);
        let futures = grants.into_iter().map(|grant| {
            let inner = Arc::clone(self);
            async move {
                let start = Instant::now();
                let hydrate_owner = Arc::clone(&inner);
                let result = hydrate_owner
                    .hydrate_segment_async(grant.instance_id, grant.segment_id)
                    .await;
                inner.metrics.sub_hydration_queue(1);
                match result {
                    Ok(Some(outcome)) => {
                        inner.metrics.record_hydration_latency(start.elapsed());
                        tracing::event!(
                            target: "aof2::tier1",
                            tracing::Level::INFO,
                            instance = grant.instance_id.get(),
                            segment = grant.segment_id.as_u64(),
                            latency_ms = start.elapsed().as_millis() as u64,
                            "tier1_hydration_success"
                        );
                        Some(outcome)
                    }
                    Ok(None) => None,
                    Err(err) => {
                        inner.metrics.incr_hydration_failures();
                        tracing::event!(
                            target: "aof2::tier1",
                            tracing::Level::WARN,
                            instance = grant.instance_id.get(),
                            segment = grant.segment_id.as_u64(),
                            error = %err,
                            "tier1_hydration_failed"
                        );
                        None
                    }
                }
            }
        });
        let results = join_all(futures).await;
        results.into_iter().flatten().collect()
    }

    async fn hydrate_segment_async(
        self: Arc<Self>,
        instance_id: InstanceId,
        segment_id: SegmentId,
    ) -> AofResult<Option<HydrationOutcome>> {
        let inner = Arc::clone(&self);
        spawn_blocking(move || inner.hydrate_segment_blocking(instance_id, segment_id))
            .await
            .map_err(|err| AofError::Other(format!("hydrate task panicked: {err}")))?
    }

    fn hydrate_segment_blocking(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
    ) -> AofResult<Option<HydrationOutcome>> {
        let (layout, mut entry) = {
            let mut instances = self.instances.lock();
            let state = match instances.get_mut(&instance_id) {
                Some(state) => state,
                None => {
                    return Err(AofError::InvalidState(format!(
                        "Tier1 instance {:?} not registered",
                        instance_id.get()
                    )));
                }
            };
            match state.manifest.get(segment_id).cloned() {
                Some(entry) => (state.layout.clone(), entry),
                None => {
                    trace!(
                        segment = segment_id.as_u64(),
                        "manifest missing entry for hydration"
                    );
                    return Ok(None);
                }
            }
        };

        let compressed_path = layout.warm_dir().join(&entry.compressed_path);
        trace!(
            segment = segment_id.as_u64(),
            path = %compressed_path.display(),
            exists = compressed_path.exists(),
            "hydrate warm lookup"
        );
        if !compressed_path.exists() {
            self.metrics.incr_hydration_failures();
            if let Some(tier2) = self.tier2() {
                let fetch_request = Tier2FetchRequest {
                    instance_id,
                    segment_id,
                    sealed_at: entry.sealed_at,
                    destination: compressed_path.clone(),
                };
                if let Err(err) = tier2.schedule_fetch(fetch_request) {
                    tracing::event!(
                        target: "aof2::tier1",
                        tracing::Level::WARN,
                        instance = instance_id.get(),
                        segment = segment_id.as_u64(),
                        error = %err,
                        "tier1_hydration_retry_failed"
                    );
                } else {
                    tracing::event!(
                        target: "aof2::tier1",
                        tracing::Level::INFO,
                        instance = instance_id.get(),
                        segment = segment_id.as_u64(),
                        sealed_at = entry.sealed_at,
                        "tier1_hydration_retry_queued"
                    );
                }
            } else {
                tracing::event!(
                    target: "aof2::tier1",
                    tracing::Level::WARN,
                    instance = instance_id.get(),
                    segment = segment_id.as_u64(),
                    "tier1_hydration_retry_unavailable"
                );
            }

            let mut instances = self.instances.lock();
            if let Some(state) = instances.get_mut(&instance_id) {
                if let Some(manifest_entry) = state.manifest.get_mut(segment_id) {
                    manifest_entry.residency = Tier1ResidencyState::NeedsRecovery;
                    manifest_entry.touch();
                    let base_offset = manifest_entry.base_offset;
                    state.persist_manifest()?;
                    state.log_manifest_record(
                        segment_id,
                        base_offset,
                        ManifestRecordPayload::HydrationFailed {
                            reason_code: HYDRATION_REASON_MISSING_WARM,
                        },
                    );
                }
            }
            return Ok(None);
        }

        let mut decoder = ZstdDecoder::new(File::open(&compressed_path).map_err(AofError::from)?)
            .map_err(|err| AofError::CompressionError(err.to_string()))?;
        let mut decompressed_bytes = Vec::new();
        let mut temp = NamedTempFile::new_in(layout.segments_dir()).map_err(AofError::from)?;
        {
            let file = temp.as_file_mut();
            let mut buffer = [0u8; 8 * 1024];
            loop {
                let read_bytes = decoder.read(&mut buffer).map_err(AofError::from)?;
                if read_bytes == 0 {
                    break;
                }
                file.write_all(&buffer[..read_bytes])
                    .map_err(AofError::from)?;
                decompressed_bytes.extend_from_slice(&buffer[..read_bytes]);
            }
            file.sync_all().map_err(AofError::from)?;
        }
        let logical_size = decompressed_bytes.len() as u64;
        let checksum = compute_manifest_checksum(&decompressed_bytes)?;
        if checksum != entry.checksum {
            return Err(AofError::Corruption(format!(
                "hydrated checksum mismatch for segment {}",
                segment_id.as_u64()
            )));
        }

        let destination = layout.segment_file_path(segment_id, entry.base_offset, entry.created_at);
        temp.persist(&destination)
            .map_err(|err| AofError::from(err.error))?;

        entry.residency = Tier1ResidencyState::StagedForTier0;
        entry.touch();

        {
            let mut instances = self.instances.lock();
            if let Some(state) = instances.get_mut(&instance_id) {
                state.touch(segment_id);
                if let Some(manifest_entry) = state.manifest.get_mut(segment_id) {
                    *manifest_entry = entry.clone();
                }
                state.persist_manifest()?;
            }
        }

        Ok(Some(HydrationOutcome {
            instance_id,
            segment_id,
            path: destination,
            base_offset: entry.base_offset,
            base_record_count: entry.base_record_count,
            created_at: entry.created_at,
            sealed_at: entry.sealed_at,
            checksum: entry.checksum,
            logical_size: logical_size as u32,
        }))
    }

    fn update_residency(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
        residency: Tier1ResidencyState,
    ) -> AofResult<()> {
        let previous = {
            let mut instances = self.instances.lock();
            let state = instances.get_mut(&instance_id).ok_or_else(|| {
                AofError::InvalidState(format!(
                    "Tier1 instance {:?} not registered",
                    instance_id.get()
                ))
            })?;
            state.update_residency(segment_id, residency)?
        };
        self.metrics
            .record_residency_transition(previous, Some(residency));
        Ok(())
    }

    fn manifest_entry(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
    ) -> AofResult<Option<ManifestEntry>> {
        let instances = self.instances.lock();
        let state = instances.get(&instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {:?} not registered",
                instance_id.get()
            ))
        })?;
        Ok(state.manifest.get(segment_id).cloned())
    }

    fn manifest_replay_metrics(
        &self,
        instance_id: InstanceId,
    ) -> AofResult<ManifestReplaySnapshot> {
        let instances = self.instances.lock();
        let state = instances.get(&instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {:?} not registered",
                instance_id.get()
            ))
        })?;
        Ok(state.replay_metrics.snapshot())
    }

    fn manifest_snapshot(&self, instance_id: InstanceId) -> AofResult<Vec<ManifestEntry>> {
        let instances = self.instances.lock();
        let state = instances.get(&instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {:?} not registered",
                instance_id.get()
            ))
        })?;
        Ok(state.manifest.all().cloned().collect())
    }
}

/// Commands sent to compression worker threads.
enum CompressionCommand {
    /// Compress a segment job
    Compress(CompressionJob),
    /// Shutdown signal for clean termination
    Shutdown,
}

fn read_segment_footer(segment: &Segment) -> AofResult<SegmentFooter> {
    let mut file = File::open(segment.path()).map_err(AofError::from)?;
    let mut buf = [0u8; SEGMENT_FOOTER_SIZE as usize];
    let footer_offset = segment.footer_offset() as u64;
    file.seek(SeekFrom::Start(footer_offset))
        .map_err(AofError::from)?;
    file.read_exact(&mut buf).map_err(AofError::from)?;
    SegmentFooter::decode(&buf).ok_or_else(|| {
        AofError::Corruption(format!(
            "failed to decode footer for segment {}",
            segment.id().as_u64()
        ))
    })
}

fn build_offset_index(segment: &Segment) -> AofResult<Vec<u32>> {
    let mut offsets = Vec::new();
    let mut file = File::open(segment.path()).map_err(AofError::from)?;
    let mut header = [0u8; RECORD_HEADER_SIZE as usize];
    let mut cursor = SEGMENT_HEADER_SIZE as u64;
    let durable = segment.durable_size() as u64;

    while cursor + RECORD_HEADER_SIZE as u64 <= durable {
        file.seek(SeekFrom::Start(cursor)).map_err(AofError::from)?;
        file.read_exact(&mut header).map_err(AofError::from)?;
        let length = u32::from_le_bytes(
            header[0..4]
                .try_into()
                .map_err(|_| AofError::Corruption("record header corrupt".to_string()))?,
        );
        if length == 0 {
            break;
        }
        offsets.push(cursor as u32);
        cursor = cursor
            .checked_add(RECORD_HEADER_SIZE as u64)
            .and_then(|value| value.checked_add(length as u64))
            .ok_or_else(|| AofError::Corruption("record length overflow".to_string()))?;
        if cursor > durable {
            return Err(AofError::Corruption(
                "record extends beyond durable size".to_string(),
            ));
        }
    }

    Ok(offsets)
}

fn compute_manifest_checksum(payload: &[u8]) -> AofResult<u32> {
    if payload.len() <= SEGMENT_HEADER_SIZE as usize {
        return Ok(0);
    }

    let usable_limit = payload.len().saturating_sub(SEGMENT_FOOTER_SIZE as usize);
    if usable_limit <= SEGMENT_HEADER_SIZE as usize {
        return Ok(0);
    }

    let mut cursor = SEGMENT_HEADER_SIZE as usize;
    let mut digest = Digest::new();
    while cursor + RECORD_HEADER_SIZE as usize <= usable_limit {
        let header = &payload[cursor..cursor + RECORD_HEADER_SIZE as usize];
        let length = u32::from_le_bytes(
            header[0..4]
                .try_into()
                .map_err(|_| AofError::Corruption("record length corrupt".to_string()))?,
        ) as usize;
        if length == 0 {
            break;
        }

        let payload_start = cursor + RECORD_HEADER_SIZE as usize;
        let payload_end = payload_start + length;
        if payload_end > usable_limit {
            break;
        }

        digest.write(&payload[payload_start..payload_end]);
        cursor = payload_end;
    }

    Ok(fold_crc64(digest.sum64()))
}

fn fold_crc64(value: u64) -> u32 {
    (value as u32) ^ ((value >> 32) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AofConfig, FlushConfig};
    use crate::error::AofResult;
    use crate::manifest::{ManifestLogReader, ManifestRecordPayload};
    use crate::store::tier0::ActivationReason;
    use crate::store::tier0::{Tier0Cache, Tier0CacheConfig};
    use crate::store::tier2::{
        GetObjectRequest, GetObjectResult, HeadObjectResult, PutObjectRequest, PutObjectResult,
        Tier2Client, Tier2Config, Tier2Event, Tier2FetchComplete, Tier2Manager, Tier2Metadata,
        Tier2UploadComplete,
    };
    use futures::future::BoxFuture;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;
    use tempfile::TempDir;

    fn build_layout(dir: &TempDir) -> Layout {
        let config = AofConfig {
            root_dir: dir.path().to_path_buf(),
            flush: FlushConfig::default(),
            ..AofConfig::default()
        }
        .normalized();
        let layout = Layout::new(&config);
        layout.ensure().expect("layout ensure");
        layout
    }

    #[test]
    fn tier1_metrics_snapshot_tracks_new_gauges() {
        let metrics = Tier1Metrics::default();
        metrics.record_residency_transition(None, Some(Tier1ResidencyState::StagedForTier0));
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.residency.staged_for_tier0, 1);

        metrics.record_residency_transition(
            Some(Tier1ResidencyState::StagedForTier0),
            Some(Tier1ResidencyState::ResidentInTier0),
        );
        metrics.record_residency_transition(Some(Tier1ResidencyState::ResidentInTier0), None);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.residency.staged_for_tier0, 0);
        assert_eq!(snapshot.residency.resident_in_tier0, 0);

        metrics.add_hydration_queue(3);
        metrics.sub_hydration_queue(2);
        metrics.record_hydration_latency(Duration::from_millis(25));
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.hydration_queue_depth, 1);
        assert_eq!(snapshot.hydration_latency_samples, 1);
        assert_eq!(snapshot.hydration_latency_p50_ms, 25);
    }

    fn seed_manifest_log(
        layout: &Layout,
        instance_id: InstanceId,
        manifest_cfg: &ManifestLogWriterConfig,
    ) -> (Vec<ManifestEntry>, usize) {
        let mut entries = Vec::new();
        for (idx, base_offset) in [(1u64, 0u64), (2u64, 128u64)] {
            let segment_id = SegmentId::new(idx);
            let sealed_at = 1_000 + idx as i64;
            let warm_path = layout.warm_file_path(segment_id, base_offset, sealed_at);
            std::fs::write(&warm_path, format!("segment-{idx}")).expect("write warm file");
            let compressed_path = warm_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("warm file name")
                .to_string();
            let entry = ManifestEntry {
                segment_id,
                base_offset,
                base_record_count: 64,
                created_at: 500 + idx as i64,
                sealed_at,
                checksum: 0xDEAD ^ idx as u32,
                ext_id: 0,
                flush_failure: false,
                reserved: [0u8; SEAL_RESERVED_BYTES],
                original_bytes: 2048,
                compressed_bytes: std::fs::metadata(&warm_path).expect("warm metadata").len(),
                compressed_path,
                dictionary: None,
                offset_index: vec![0, 128, 256],
                residency: Tier1ResidencyState::StagedForTier0,
                tier2: None,
                last_access_epoch_ms: 10_000 + idx * 10,
            };
            entries.push(entry);
        }

        let snapshot = serde_json::to_vec(&entries).expect("snapshot encode");
        let mut writer_cfg = manifest_cfg.clone();
        writer_cfg.enabled = true;
        let mut writer = ManifestLogWriter::new(
            layout.manifest_dir().to_path_buf(),
            instance_id.get(),
            writer_cfg,
        )
        .expect("manifest writer");
        writer
            .append_now(
                SegmentId::new(0),
                0,
                ManifestRecordPayload::Tier1Snapshot { json: snapshot },
            )
            .expect("append snapshot");
        writer.commit().expect("commit snapshot");
        drop(writer);

        let stream_dir = layout
            .manifest_dir()
            .join(format!("{id:020}", id = instance_id.get()));
        let chunk_path = std::fs::read_dir(&stream_dir)
            .expect("stream manifest dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("mlog"))
            .expect("manifest chunk");
        let dangling = b"dangling";
        let mut file = OpenOptions::new()
            .append(true)
            .open(&chunk_path)
            .expect("open chunk for append");
        file.write_all(dangling).expect("append dangling bytes");
        file.sync_data().expect("sync dangling");

        (entries, dangling.len())
    }

    fn create_sealed_segment(layout: &Layout, id: u64, payload: &[u8]) -> Arc<Segment> {
        let segment_id = SegmentId::new(id);
        let path = layout.segment_file_path(segment_id, 0, 0);
        let segment = Segment::create_active(segment_id, 0, 0, 0, 1024, path.as_path())
            .expect("segment create");
        let now = current_epoch_ms();
        let append = segment.append_record(payload, now).expect("append record");
        let flush_state = segment.flush_state();
        let _ = flush_state.try_begin_flush();
        flush_state.mark_durable(append.logical_size);
        flush_state.finish_flush();
        segment.mark_durable(append.logical_size);
        assert_eq!(segment.durable_size(), append.logical_size);
        segment.seal(now as i64, false).expect("seal segment");
        Arc::new(segment)
    }

    #[derive(Clone, Default)]
    struct RecordingTier2Client {
        uploads: Arc<StdMutex<Vec<String>>>,
    }

    impl RecordingTier2Client {
        fn upload_count(&self) -> usize {
            self.uploads.lock().unwrap().len()
        }
    }

    impl Tier2Client for RecordingTier2Client {
        fn put_object(
            &self,
            request: PutObjectRequest,
        ) -> BoxFuture<'_, AofResult<PutObjectResult>> {
            let uploads = self.uploads.clone();
            Box::pin(async move {
                let size = tokio::fs::metadata(&request.source)
                    .await
                    .map_err(AofError::from)?
                    .len();
                uploads.lock().unwrap().push(request.key.clone());
                Ok(PutObjectResult {
                    etag: Some("mock-etag".to_string()),
                    size,
                })
            })
        }

        fn get_object(
            &self,
            _request: GetObjectRequest,
        ) -> BoxFuture<'_, AofResult<GetObjectResult>> {
            Box::pin(async {
                Err(AofError::InvalidState(
                    "recording tier2 client does not support get_object".to_string(),
                ))
            })
        }

        fn delete_object(&self, _bucket: &str, _key: &str) -> BoxFuture<'_, AofResult<()>> {
            Box::pin(async { Ok(()) })
        }

        fn head_object(
            &self,
            _bucket: &str,
            _key: &str,
        ) -> BoxFuture<'_, AofResult<Option<HeadObjectResult>>> {
            Box::pin(async { Ok(None) })
        }
    }

    #[test]
    fn manifest_log_replay_detects_corruption() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let instance_id = InstanceId::new(99);

        let mut cfg = ManifestLogWriterConfig::default();
        cfg.enabled = true;
        let mut writer = ManifestLogWriter::new(
            layout.manifest_dir().to_path_buf(),
            instance_id.get(),
            cfg.clone(),
        )
        .expect("writer");
        writer
            .append_now(
                SegmentId::new(1),
                0,
                ManifestRecordPayload::CompressionDone {
                    compressed_bytes: 256,
                    dictionary_id: 0,
                },
            )
            .expect("append record");
        writer.commit().expect("commit record");
        drop(writer);

        let chunk_dir = layout
            .manifest_dir()
            .join(format!("{id:020}", id = instance_id.get()));
        let chunk_path = std::fs::read_dir(&chunk_dir)
            .expect("read manifest dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("mlog"))
            .expect("manifest chunk");

        let mut header_buf = [0u8; CHUNK_HEADER_LEN];
        let mut header_file = File::open(&chunk_path).expect("open chunk");
        header_file
            .read_exact(&mut header_buf)
            .expect("read chunk header");
        let header = ChunkHeader::read_from(&header_buf).expect("decode header");
        drop(header_file);

        let mut corrupt = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&chunk_path)
            .expect("open chunk for corruption");
        let seek_pos = (header.committed_len as u64).saturating_sub(1);
        corrupt
            .seek(SeekFrom::Start(seek_pos))
            .expect("seek to tail");
        let mut byte = [0u8];
        corrupt.read_exact(&mut byte).expect("read tail byte");
        byte[0] ^= 0xFF;
        corrupt
            .seek(SeekFrom::Start(seek_pos))
            .expect("seek to tail for write");
        corrupt.write_all(&byte).expect("write corrupt byte");
        corrupt.sync_data().expect("sync corrupt chunk");
        drop(corrupt);

        let metrics = ManifestReplayMetrics::new();
        let result = Tier1InstanceState::replay_manifest_from_log(&layout, instance_id, &metrics);
        assert!(matches!(result, Err(AofError::Corruption(_))));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.chunk_count, 1);
        assert_eq!(snapshot.corruption_events, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn manifest_log_only_replay_bootstrap() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let instance_id = InstanceId::new(42);

        let mut config = Tier1Config::new(10 * 1024 * 1024)
            .with_worker_threads(1)
            .with_manifest_log_enabled(true)
            .with_manifest_log_only(true);
        config.manifest_log.chunk_capacity_bytes = 128 * 1024;

        let (entries, dangling_bytes) =
            seed_manifest_log(&layout, instance_id, &config.manifest_log);
        let expected_entry = entries[0].clone();

        let tier1 = Tier1Cache::new(tokio::runtime::Handle::current(), config.clone())
            .expect("tier1 cache");
        let instance = tier1
            .register_instance(instance_id, layout.clone())
            .expect("register instance");

        assert!(
            !layout.warm_dir().join("manifest.json").exists(),
            "json manifest should not be written in log-only mode"
        );

        let restored = instance
            .manifest_entry(expected_entry.segment_id)
            .expect("manifest entry")
            .expect("entry present");
        assert_eq!(restored.compressed_path, expected_entry.compressed_path);
        assert_eq!(restored.base_offset, expected_entry.base_offset);

        let metrics = instance
            .manifest_replay_metrics()
            .expect("replay metrics snapshot");
        assert!(metrics.chunk_lag_seconds >= 0.0);
        assert!(metrics.journal_lag_bytes >= dangling_bytes as u64);

        let chunk_dir = layout
            .manifest_dir()
            .join(format!("{id:020}", id = instance_id.get()));
        let chunk_path = std::fs::read_dir(&chunk_dir)
            .expect("stream manifest dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("mlog"))
            .expect("manifest chunk after replay");
        let mut header_bytes = vec![0u8; CHUNK_HEADER_LEN];
        let mut chunk_file = File::open(&chunk_path).expect("open chunk");
        chunk_file
            .read_exact(&mut header_bytes)
            .expect("read chunk header");
        let header = ChunkHeader::read_from(&header_bytes).expect("decode header");
        let file_len = chunk_file.metadata().expect("chunk metadata").len();
        assert_eq!(file_len, header.committed_len as u64);

        instance
            .update_residency(
                expected_entry.segment_id,
                Tier1ResidencyState::ResidentInTier0,
            )
            .expect("update residency");
        let hydrated = instance
            .manifest_entry(expected_entry.segment_id)
            .expect("manifest entry")
            .expect("entry present");
        assert!(matches!(
            hydrated.residency,
            Tier1ResidencyState::ResidentInTier0
        ));

        let tier2_meta = Tier2Metadata::new(
            "tier2/tests/object".to_string(),
            Some("etag-123".to_string()),
            hydrated.original_bytes,
        );
        tier1.handle_tier2_events(vec![Tier2Event::UploadCompleted(Tier2UploadComplete {
            instance_id,
            segment_id: hydrated.segment_id,
            metadata: tier2_meta.clone(),
            base_offset: hydrated.base_offset,
            base_record_count: hydrated.base_record_count,
            checksum: hydrated.checksum,
        })]);

        let promoted = instance
            .manifest_entry(hydrated.segment_id)
            .expect("manifest entry")
            .expect("entry present");
        assert!(matches!(
            promoted.residency,
            Tier1ResidencyState::UploadedToTier2
        ));
        let tier2 = promoted.tier2.expect("tier2 metadata");
        assert_eq!(tier2.object_key, tier2_meta.object_key);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compression_pipeline_persists_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let tier0 = Tier0Cache::new(Tier0CacheConfig::new(1024 * 1024, 1024 * 1024));
        let tier0_instance = tier0.register_instance("test", None);
        let config = Tier1Config::new(10 * 1024 * 1024).with_worker_threads(1);
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");
        let tier1_instance = tier1
            .register_instance(tier0_instance.instance_id(), layout.clone())
            .expect("register tier1 instance");
        let segment = create_sealed_segment(&layout, 1, b"payload");

        tier1_instance
            .schedule_compression(segment.clone())
            .expect("schedule compression");

        wait_for(|| {
            tier1
                .manifest_snapshot(tier0_instance.instance_id())
                .ok()
                .and_then(|entries| {
                    entries
                        .into_iter()
                        .find(|entry| entry.segment_id == segment.id())
                })
                .is_some()
        })
        .await;

        let snapshot = tier1
            .manifest_snapshot(tier0_instance.instance_id())
            .expect("manifest snapshot");
        assert_eq!(snapshot.len(), 1);
        let entry = &snapshot[0];
        assert_eq!(entry.segment_id, segment.id());
        assert_eq!(entry.base_offset, segment.base_offset());
        assert_eq!(entry.base_record_count, segment.base_record_count());

        let warm_file = layout.warm_dir().join(&entry.compressed_path);
        assert!(warm_file.exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn manifest_log_disabled_is_noop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let tier0 = Tier0Cache::new(Tier0CacheConfig::new(1024 * 1024, 1024 * 1024));
        let tier0_instance = tier0.register_instance("disabled", None);
        let config = Tier1Config::new(10 * 1024 * 1024)
            .with_worker_threads(1)
            .with_manifest_log_enabled(false);
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");
        let tier1_instance = tier1
            .register_instance(tier0_instance.instance_id(), layout.clone())
            .expect("register tier1 instance");

        let segment = create_sealed_segment(&layout, 11, b"disabled");
        tier1_instance
            .schedule_compression(segment.clone())
            .expect("schedule compression");

        wait_for(|| {
            tier1
                .manifest_snapshot(tier0_instance.instance_id())
                .map(|entries| entries.len() == 1)
                .unwrap_or(false)
        })
        .await;

        drop(segment);

        let reader = ManifestLogReader::open(
            layout.manifest_dir().to_path_buf(),
            tier0_instance.instance_id().get(),
        )
        .expect("open manifest log");
        let records = reader
            .iter()
            .collect::<AofResult<Vec<_>>>()
            .expect("collect manifest records");
        assert!(records.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn hydrate_missing_warm_marks_needs_recovery() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let instance_id = InstanceId::new(7);

        let config = Tier1Config::new(2 * 1024 * 1024).with_worker_threads(1);
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");
        let instance = tier1
            .register_instance(instance_id, layout.clone())
            .expect("register instance");

        let segment_id = SegmentId::new(55);
        let missing_name = "missing-segment.zst".to_string();
        {
            let mut instances = tier1.inner.instances.lock();
            let state = instances
                .get_mut(&instance_id)
                .expect("tier1 state present");
            let entry = ManifestEntry {
                segment_id,
                base_offset: 0,
                base_record_count: 0,
                created_at: 0,
                sealed_at: 0,
                checksum: 0,
                ext_id: 0,
                flush_failure: false,
                reserved: [0u8; SEAL_RESERVED_BYTES],
                original_bytes: 0,
                compressed_bytes: 512,
                compressed_path: missing_name.clone(),
                dictionary: None,
                offset_index: Vec::new(),
                residency: Tier1ResidencyState::StagedForTier0,
                tier2: None,
                last_access_epoch_ms: current_epoch_ms(),
            };
            state.manifest.insert(entry.clone());
            state.used_bytes = state.used_bytes.saturating_add(entry.compressed_bytes);
            state.lru.push_back(segment_id);
        }

        let result = instance
            .hydrate_segment_blocking(segment_id)
            .expect("hydrate call");
        assert!(result.is_none());

        let entry = instance
            .manifest_entry(segment_id)
            .expect("manifest entry")
            .expect("entry present");
        assert_eq!(entry.residency, Tier1ResidencyState::NeedsRecovery);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fetch_events_clear_needs_recovery() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let instance_id = InstanceId::new(9);

        let mut config = Tier1Config::new(2 * 1024 * 1024).with_worker_threads(1);
        config.manifest_log.enabled = false;
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");
        tier1
            .register_instance(instance_id, layout.clone())
            .expect("register instance");

        let segment_id = SegmentId::new(99);
        let warm_name = "recovered-segment.zst".to_string();
        let warm_path = layout.warm_dir().join(&warm_name);
        std::fs::write(&warm_path, b"warm-bytes").expect("write warm file");

        {
            let mut instances = tier1.inner.instances.lock();
            let state = instances
                .get_mut(&instance_id)
                .expect("tier1 state present");
            let entry = ManifestEntry {
                segment_id,
                base_offset: 0,
                base_record_count: 0,
                created_at: 0,
                sealed_at: 0,
                checksum: 0,
                ext_id: 0,
                flush_failure: false,
                reserved: [0u8; SEAL_RESERVED_BYTES],
                original_bytes: 0,
                compressed_bytes: warm_path.metadata().unwrap().len(),
                compressed_path: warm_name.clone(),
                dictionary: None,
                offset_index: Vec::new(),
                residency: Tier1ResidencyState::NeedsRecovery,
                tier2: None,
                last_access_epoch_ms: current_epoch_ms(),
            };
            state.manifest.insert(entry.clone());
            state.used_bytes = state.used_bytes.saturating_add(entry.compressed_bytes);
            state.lru.push_back(segment_id);
        }

        tier1.handle_tier2_events(vec![Tier2Event::FetchCompleted(Tier2FetchComplete {
            instance_id,
            segment_id,
            destination: layout.warm_dir().join(&warm_name),
        })]);

        let entry = tier1
            .manifest_snapshot(instance_id)
            .expect("manifest snapshot")
            .into_iter()
            .find(|entry| entry.segment_id == segment_id)
            .expect("entry present");
        assert_eq!(entry.residency, Tier1ResidencyState::StagedForTier0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn needs_recovery_persists_across_manifest_replay() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let instance_id = InstanceId::new(11);

        let mut config = Tier1Config::new(2 * 1024 * 1024)
            .with_worker_threads(1)
            .with_manifest_log_enabled(true)
            .with_manifest_log_only(true);
        config.manifest_log.chunk_capacity_bytes = 128 * 1024;

        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config.clone()).expect("tier1");
        tier1
            .register_instance(instance_id, layout.clone())
            .expect("register instance");

        let segment_id = SegmentId::new(123);
        {
            let mut instances = tier1.inner.instances.lock();
            let state = instances
                .get_mut(&instance_id)
                .expect("tier1 state present");
            let entry = ManifestEntry {
                segment_id,
                base_offset: 0,
                base_record_count: 0,
                created_at: 0,
                sealed_at: 0,
                checksum: 0,
                ext_id: 0,
                flush_failure: false,
                reserved: [0u8; SEAL_RESERVED_BYTES],
                original_bytes: 0,
                compressed_bytes: 256,
                compressed_path: "needs-recovery.zst".to_string(),
                dictionary: None,
                offset_index: Vec::new(),
                residency: Tier1ResidencyState::NeedsRecovery,
                tier2: None,
                last_access_epoch_ms: current_epoch_ms(),
            };
            state.manifest.insert(entry.clone());
            state.used_bytes = state.used_bytes.saturating_add(entry.compressed_bytes);
            state.lru.push_back(segment_id);
            state.log_manifest_record(
                segment_id,
                entry.base_offset,
                ManifestRecordPayload::HydrationFailed {
                    reason_code: HYDRATION_REASON_MISSING_WARM,
                },
            );
            state.persist_manifest().expect("persist manifest");
        }

        drop(tier1);

        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 replay");
        let instance = tier1
            .register_instance(instance_id, layout.clone())
            .expect("register instance");

        let entry = instance
            .manifest_entry(segment_id)
            .expect("manifest entry")
            .expect("entry present after replay");
        assert_eq!(entry.residency, Tier1ResidencyState::NeedsRecovery);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn manifest_log_records_compression_done() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let tier0 = Tier0Cache::new(Tier0CacheConfig::new(1024 * 1024, 1024 * 1024));
        let tier0_instance = tier0.register_instance("enabled", None);
        let config = Tier1Config::new(10 * 1024 * 1024)
            .with_worker_threads(1)
            .with_manifest_log_enabled(true);
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");
        let tier1_instance = tier1
            .register_instance(tier0_instance.instance_id(), layout.clone())
            .expect("register tier1 instance");

        let segment = create_sealed_segment(&layout, 12, b"enabled");
        let segment_id = segment.id();
        let base_offset = segment.base_offset();
        tier1_instance
            .schedule_compression(segment.clone())
            .expect("schedule compression");

        wait_for(|| {
            tier1
                .manifest_snapshot(tier0_instance.instance_id())
                .ok()
                .and_then(|entries| {
                    entries
                        .into_iter()
                        .find(|entry| entry.segment_id == segment_id)
                })
                .is_some()
        })
        .await;

        let compressed_bytes_expect = tier1
            .manifest_snapshot(tier0_instance.instance_id())
            .expect("manifest snapshot")
            .into_iter()
            .find(|entry| entry.segment_id == segment_id)
            .map(|entry| entry.compressed_bytes)
            .expect("entry present");

        drop(segment);

        let reader = ManifestLogReader::open(
            layout.manifest_dir().to_path_buf(),
            tier0_instance.instance_id().get(),
        )
        .expect("open manifest log");
        let records = reader
            .iter()
            .collect::<AofResult<Vec<_>>>()
            .expect("collect manifest records");
        assert!(
            !records.is_empty(),
            "expected manifest records to be written"
        );
        let compression_done = records
            .iter()
            .find_map(|record| match &record.payload {
                ManifestRecordPayload::CompressionDone {
                    compressed_bytes, ..
                } => Some((record.logical_offset, *compressed_bytes)),
                _ => None,
            })
            .expect("compression done record present");
        assert_eq!(compression_done.0, base_offset);
        assert_eq!(compression_done.1, compressed_bytes_expect);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hydration_decompresses_segment() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let tier0 = Tier0Cache::new(Tier0CacheConfig::new(1024 * 1024, 1024 * 1024));
        let tier0_instance = tier0.register_instance("test", None);
        let config = Tier1Config::new(10 * 1024 * 1024).with_worker_threads(1);
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");
        let tier1_instance = tier1
            .register_instance(tier0_instance.instance_id(), layout.clone())
            .expect("register tier1 instance");

        let segment = create_sealed_segment(&layout, 2, b"hydrate");
        let segment_id = segment.id();
        let segment_bytes = segment.current_size() as u64;
        tier1_instance
            .schedule_compression(segment.clone())
            .expect("schedule compression");
        drop(segment);

        wait_for(|| {
            tier1
                .manifest_snapshot(tier0_instance.instance_id())
                .ok()
                .and_then(|entries| {
                    entries
                        .into_iter()
                        .find(|entry| entry.segment_id == segment_id)
                })
                .is_some()
        })
        .await;

        let grants = vec![ActivationGrant {
            instance_id: tier0_instance.instance_id(),
            segment_id,
            bytes: segment_bytes,
            priority: 10,
            reason: ActivationReason::ReadMiss,
        }];

        let outcomes = tier1.handle_activation_grants(grants).await;
        assert_eq!(outcomes.len(), 1);
        let outcome = &outcomes[0];
        assert_eq!(outcome.segment_id, segment_id);
        assert!(outcome.path.exists());

        let snapshot = tier1
            .manifest_snapshot(tier0_instance.instance_id())
            .expect("manifest snapshot");
        let entry = snapshot
            .into_iter()
            .find(|entry| entry.segment_id == segment_id)
            .expect("entry present");
        assert!(matches!(
            entry.residency,
            Tier1ResidencyState::ResidentInTier0 | Tier1ResidencyState::StagedForTier0
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compression_schedules_tier2_upload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let tier0 = Tier0Cache::new(Tier0CacheConfig::new(1024 * 1024, 1024 * 1024));
        let tier0_instance = tier0.register_instance("tier2", None);
        let config = Tier1Config::new(10 * 1024 * 1024).with_worker_threads(1);
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");

        let client = RecordingTier2Client::default();
        let tier2_manager = Tier2Manager::with_client(
            tokio::runtime::Handle::current(),
            Tier2Config::default(),
            Arc::new(client.clone()),
        )
        .expect("tier2 manager");
        tier1.attach_tier2(tier2_manager.handle());

        let tier1_instance = tier1
            .register_instance(tier0_instance.instance_id(), layout.clone())
            .expect("register tier1 instance");

        let segment = create_sealed_segment(&layout, 7, b"tier2 upload");
        tier1_instance
            .schedule_compression(segment.clone())
            .expect("schedule compression");

        wait_for(|| client.upload_count() >= 1).await;

        drop(segment);
        drop(tier2_manager);
    }

    async fn wait_for(mut predicate: impl FnMut() -> bool) {
        for _ in 0..50 {
            if predicate() {
                return;
            }
            tokio_sleep(Duration::from_millis(20)).await;
        }
        panic!("condition not satisfied");
    }
}
