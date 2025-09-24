//! Utilities for building and orchestrating BOP's tiered append-only file (AOF) store.
//!
//! This crate provides a high-performance, tiered append-only file system designed for
//! write-heavy workloads with strong durability guarantees. The AOF store consists of
//! three storage tiers:
//!
//! - **Tier 0**: High-speed in-memory cache with admission control
//! - **Tier 1**: SSD-based cache with background hydration
//! - **Tier 2**: Remote storage for long-term persistence (optional)
//!
//! ## Architecture Overview
//!
//! The crate orchestrates a multi-layered system:
//!
//! - **Runtime management**: Shared Tokio runtime with coordinated shutdown
//! - **Multi-tier caching**: Hierarchical storage with automatic promotion/demotion
//! - **Durability pipeline**: Background flush operations with configurable guarantees
//! - **Segment management**: Memory-mapped files with atomic operations
//! - **Manifest system**: Metadata tracking for recovery and consistency
//!
//! ## Key Components
//!
//! - [`AofManager`]: Manages shared runtime and coordinates all AOF instances
//! - [`Aof`]: Individual AOF instance with append and read operations
//! - [`SegmentReader`]: Zero-copy record reading with bounds checking
//! - [`TailFollower`]: Real-time tail following for live data streams
//!
//! ## Example Usage
//!
//! ```rust
//! use bop_aof::{AofManager, AofManagerConfig, AofConfig};
//!
//! // Create a manager with default configuration
//! let manager = AofManager::with_config(AofManagerConfig::default())?;
//! let handle = manager.handle();
//!
//! // Create an AOF instance
//! let config = AofConfig::default();
//! let aof = handle.create_aof(config).await?;
//!
//! // Append data
//! let data = b"hello world";
//! let record_id = aof.append(data).await?;
//!
//! // Read data back
//! let reader = aof.reader().await?;
//! let record = reader.get(record_id)?;
//! ```
//!
//! ## Thread Safety
//!
//! All public APIs are thread-safe and designed for concurrent access. The internal
//! architecture uses lock-free algorithms where possible, with strategic use of
//! parking_lot mutexes for coordination points.
//!
//! ## Performance Characteristics
//!
//! - **Append latency**: Sub-millisecond for cached writes
//! - **Read latency**: Zero-copy access to resident segments
//! - **Throughput**: Scales with available CPU cores and storage bandwidth
//! - **Memory usage**: Configurable with automatic eviction policies

pub mod config;
pub mod error;
pub mod flush;
pub mod fs;
pub mod manifest;
pub mod metrics;
pub mod reader;
pub mod segment;
pub mod store;
pub mod test_support;

mod aof;
mod manager;

pub use config::{
    AofConfig, CompactionPolicy, Compression, FlushConfig, IdStrategy, RecordId, RetentionPolicy,
    SegmentId,
};
pub use fs::{
    Layout, SEGMENT_FILE_EXTENSION, SegmentFileName, TempFileGuard, create_fixed_size_file,
    fsync_dir,
};
pub use metrics::manifest_replay::{
    METRIC_MANIFEST_REPLAY_CHUNK_COUNT, METRIC_MANIFEST_REPLAY_CHUNK_LAG_SECONDS,
    METRIC_MANIFEST_REPLAY_CORRUPTION_EVENTS, METRIC_MANIFEST_REPLAY_JOURNAL_LAG_BYTES,
    ManifestReplayMetrics, ManifestReplaySnapshot,
};
pub use reader::{RecordBounds, SegmentReader, SegmentRecord, TailFollower};

pub use aof::{
    Aof, AppendReservation, AppendReserve, SegmentCatalogEntry, SegmentStatusSnapshot, TailEvent,
    TailSegmentState, TailState,
};
pub use manager::{
    AofManager, AofManagerConfig, AofManagerHandle, CoordinatorMetadataRegistry, InstanceMetadata,
    InstanceMetadataHandle, TieredStoreConfig,
};

use flush::FlushMetricsSnapshot;

/// Named metric sample produced by the flush pipeline.
///
/// Represents a single metric observation from the flush subsystem, containing
/// both the metric name and its current value. Used for exporting flush
/// performance and health metrics to monitoring systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlushMetricSample {
    /// Static name of the metric (e.g., "aof_flush_retry_attempts_total")
    pub name: &'static str,
    /// Current value of the metric
    pub value: u64,
}

/// Helper for exporting flush metrics snapshots with stable metric names.
///
/// Provides a stable interface for accessing flush metrics with consistent naming
/// conventions. Converts internal flush statistics into standardized metric samples
/// suitable for Prometheus or similar monitoring systems.
///
/// # Example
///
/// ```rust
/// use bop_aof::FlushMetricsExporter;
///
/// let exporter = FlushMetricsExporter::new(snapshot);
/// exporter.emit(|sample| {
///     println!("{}: {}", sample.name, sample.value);
/// });
/// ```
#[derive(Debug, Clone, Copy)]
pub struct FlushMetricsExporter {
    snapshot: FlushMetricsSnapshot,
}

impl FlushMetricsExporter {
    /// Creates a new exporter from a flush metrics snapshot.
    pub fn new(snapshot: FlushMetricsSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the total number of flush retry attempts.
    ///
    /// This metric tracks how many times the flush pipeline has retried
    /// failed flush operations, indicating potential I/O or durability issues.
    pub fn retry_attempts(&self) -> u64 {
        self.snapshot.retry_attempts
    }

    /// Returns the total number of failed flush retry attempts.
    ///
    /// Represents retry attempts that ultimately failed, which may indicate
    /// persistent storage issues or resource exhaustion.
    pub fn retry_failures(&self) -> u64 {
        self.snapshot.retry_failures
    }

    /// Returns the total number of flush failures.
    ///
    /// Tracks initial flush failures before any retry logic is applied.
    /// High values may indicate storage system instability.
    pub fn flush_failures(&self) -> u64 {
        self.snapshot.flush_failures
    }

    /// Returns the total number of metadata flush retry attempts.
    ///
    /// Metadata flushes are critical for consistency, so retries indicate
    /// potential issues with the metadata storage layer.
    pub fn metadata_retry_attempts(&self) -> u64 {
        self.snapshot.metadata_retry_attempts
    }

    /// Returns the total number of failed metadata flush retry attempts.
    ///
    /// Failed metadata retries are especially concerning as they can impact
    /// system recovery and consistency guarantees.
    pub fn metadata_retry_failures(&self) -> u64 {
        self.snapshot.metadata_retry_failures
    }

    /// Returns the total number of metadata flush failures.
    ///
    /// Initial metadata flush failures before retry logic is applied.
    pub fn metadata_failures(&self) -> u64 {
        self.snapshot.metadata_failures
    }

    /// Returns the current backlog size in bytes.
    ///
    /// Represents the amount of data waiting to be flushed to storage.
    /// High values indicate the flush pipeline is falling behind write load.
    pub fn backlog_bytes(&self) -> u64 {
        self.snapshot.backlog_bytes
    }

    /// Returns an iterator over all flush metric samples.
    ///
    /// Provides a convenient way to iterate over all available flush metrics
    /// with their standardized names and current values.
    pub fn samples(&self) -> impl Iterator<Item = FlushMetricSample> {
        const METRIC_NAMES: [(&str, fn(&FlushMetricsSnapshot) -> u64); 7] = [
            ("aof_flush_retry_attempts_total", |s| s.retry_attempts),
            ("aof_flush_retry_failures_total", |s| s.retry_failures),
            ("aof_flush_failures_total", |s| s.flush_failures),
            ("aof_flush_metadata_retry_attempts_total", |s| {
                s.metadata_retry_attempts
            }),
            ("aof_flush_metadata_retry_failures_total", |s| {
                s.metadata_retry_failures
            }),
            ("aof_flush_metadata_failures_total", |s| s.metadata_failures),
            ("aof_flush_backlog_bytes", |s| s.backlog_bytes),
        ];
        METRIC_NAMES
            .into_iter()
            .map(move |(name, accessor)| FlushMetricSample {
                name,
                value: accessor(&self.snapshot),
            })
    }

    /// Emits all flush metrics using the provided callback function.
    ///
    /// Convenient method for writing all flush metrics to an external system
    /// such as a metrics collector or logging framework.
    ///
    /// # Arguments
    ///
    /// * `writer` - Callback function that will be called for each metric sample
    ///
    /// # Example
    ///
    /// ```rust
    /// exporter.emit(|sample| {
    ///     metrics_registry.gauge(sample.name).set(sample.value as f64);
    /// });
    /// ```
    pub fn emit<F>(&self, mut writer: F)
    where
        F: FnMut(FlushMetricSample),
    {
        for sample in self.samples() {
            writer(sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flush::FlushMetricsSnapshot;

    #[test]
    fn flush_metrics_exporter_emits_retry_and_backlog() {
        let snapshot = FlushMetricsSnapshot {
            retry_attempts: 3,
            retry_failures: 2,
            flush_failures: 1,
            metadata_retry_attempts: 5,
            metadata_retry_failures: 4,
            metadata_failures: 3,
            backlog_bytes: 42,
            ..Default::default()
        };
        let exporter = FlushMetricsExporter::new(snapshot);
        let metrics: Vec<_> = exporter.samples().collect();
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "aof_flush_retry_attempts_total" && m.value == 3)
        );
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "aof_flush_metadata_retry_failures_total" && m.value == 4)
        );
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "aof_flush_backlog_bytes" && m.value == 42)
        );
    }
}
