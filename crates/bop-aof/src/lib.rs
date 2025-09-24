//! Utilities for building and orchestrating BOP's tiered append-only file (AOF) store.
//!
//! The crate wires together the runtime, multi-tier cache, and durability pipeline
//! that back the AOF service, and exposes helper types for metrics, metadata, and
//! read/write APIs consumed by higher-level components such as Raft integration.

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlushMetricSample {
    pub name: &'static str,
    pub value: u64,
}

/// Helper for exporting flush metrics snapshots with stable metric names.
#[derive(Debug, Clone, Copy)]
pub struct FlushMetricsExporter {
    snapshot: FlushMetricsSnapshot,
}

impl FlushMetricsExporter {
    pub fn new(snapshot: FlushMetricsSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn retry_attempts(&self) -> u64 {
        self.snapshot.retry_attempts
    }

    pub fn retry_failures(&self) -> u64 {
        self.snapshot.retry_failures
    }

    pub fn flush_failures(&self) -> u64 {
        self.snapshot.flush_failures
    }

    pub fn metadata_retry_attempts(&self) -> u64 {
        self.snapshot.metadata_retry_attempts
    }

    pub fn metadata_retry_failures(&self) -> u64 {
        self.snapshot.metadata_retry_failures
    }

    pub fn metadata_failures(&self) -> u64 {
        self.snapshot.metadata_failures
    }

    pub fn backlog_bytes(&self) -> u64 {
        self.snapshot.backlog_bytes
    }

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
