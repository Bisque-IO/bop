use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Stable metric identifiers for manifest replay operations.
///
/// These constants ensure consistent naming across dashboards,
/// monitoring systems, and test suites.
pub const METRIC_MANIFEST_REPLAY_CHUNK_LAG_SECONDS: &str = "aof_manifest_replay_chunk_lag_seconds";
pub const METRIC_MANIFEST_REPLAY_JOURNAL_LAG_BYTES: &str = "aof_manifest_replay_journal_lag_bytes";
pub const METRIC_MANIFEST_REPLAY_CHUNK_COUNT: &str = "aof_manifest_replay_chunk_count";
pub const METRIC_MANIFEST_REPLAY_CORRUPTION_EVENTS: &str = "aof_manifest_replay_corruption_events";

const MICROS_PER_SECOND: f64 = 1_000_000.0;

/// Point-in-time snapshot of manifest replay metrics.
///
/// Captures recovery progress, data integrity status, and
/// performance characteristics during manifest replay operations.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ManifestReplaySnapshot {
    /// How far behind the chunks are (seconds)
    pub chunk_lag_seconds: f64,
    /// Bytes of unprocessed manifest data
    pub journal_lag_bytes: u64,
    /// Total number of chunks processed
    pub chunk_count: u64,
    /// Number of corruption events detected
    pub corruption_events: u64,
}

/// Thread-safe metrics collection for manifest replay operations.
///
/// Tracks replay progress, latency, and data integrity issues
/// during recovery and bootstrap operations using atomic counters.
///
/// ## Usage Pattern
///
/// ```ignore
/// let metrics = ManifestReplayMetrics::new();
/// metrics.record_chunk_count(processed_chunks);
/// metrics.record_chunk_lag(lag_duration);
///
/// // Monitor progress
/// let snapshot = metrics.snapshot();
/// log_replay_progress(snapshot);
/// ```
#[derive(Default)]
pub struct ManifestReplayMetrics {
    /// Chunk processing lag in microseconds
    chunk_lag_micros: AtomicU64,
    /// Unprocessed manifest journal bytes
    journal_lag_bytes: AtomicU64,
    /// Total chunks processed during replay
    chunk_count: AtomicU64,
    /// Count of corruption events detected
    corruption_events: AtomicU64,
}

impl ManifestReplayMetrics {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn record_chunk_lag(&self, duration: Duration) {
        let micros = duration.as_micros().min(u64::MAX as u128) as u64;
        self.chunk_lag_micros.store(micros, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_chunk_lag_seconds(&self, seconds: f64) {
        if seconds <= 0.0 {
            self.chunk_lag_micros.store(0, Ordering::Relaxed);
            return;
        }
        let micros = (seconds * MICROS_PER_SECOND)
            .round()
            .clamp(0.0, u64::MAX as f64) as u64;
        self.chunk_lag_micros.store(micros, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_journal_lag_bytes(&self, bytes: u64) {
        self.journal_lag_bytes.store(bytes, Ordering::Relaxed);
    }

    #[inline]
    pub fn snapshot(&self) -> ManifestReplaySnapshot {
        let micros = self.chunk_lag_micros.load(Ordering::Relaxed);
        ManifestReplaySnapshot {
            chunk_lag_seconds: micros as f64 / MICROS_PER_SECOND,
            journal_lag_bytes: self.journal_lag_bytes.load(Ordering::Relaxed),
            chunk_count: self.chunk_count.load(Ordering::Relaxed),
            corruption_events: self.corruption_events.load(Ordering::Relaxed),
        }
    }

    #[inline]
    pub fn record_chunk_count(&self, chunks: usize) {
        self.chunk_count.store(chunks as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn incr_corruption(&self) {
        self.corruption_events.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn clear(&self) {
        self.chunk_lag_micros.store(0, Ordering::Relaxed);
        self.journal_lag_bytes.store(0, Ordering::Relaxed);
        self.chunk_count.store(0, Ordering::Relaxed);
        self.corruption_events.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_defaults_to_zero() {
        let metrics = ManifestReplayMetrics::new();
        assert_eq!(metrics.snapshot(), ManifestReplaySnapshot::default());
    }

    #[test]
    fn recorders_store_expected_values() {
        let metrics = ManifestReplayMetrics::new();
        metrics.record_chunk_lag(Duration::from_millis(250));
        metrics.record_journal_lag_bytes(12_345);
        metrics.record_chunk_count(3);
        metrics.incr_corruption();

        let snapshot = metrics.snapshot();
        assert!((snapshot.chunk_lag_seconds - 0.25).abs() < f64::EPSILON);
        assert_eq!(snapshot.journal_lag_bytes, 12_345);
        assert_eq!(snapshot.chunk_count, 3);
        assert_eq!(snapshot.corruption_events, 1);

        metrics.clear();
        assert_eq!(metrics.snapshot(), ManifestReplaySnapshot::default());
    }

    #[test]
    fn chunk_lag_seconds_helper_bounds_values() {
        let metrics = ManifestReplayMetrics::new();
        metrics.record_chunk_lag_seconds(-1.0);
        assert_eq!(metrics.snapshot().chunk_lag_seconds, 0.0);

        metrics.record_chunk_lag_seconds(1.5);
        let snapshot = metrics.snapshot();
        assert!((snapshot.chunk_lag_seconds - 1.5).abs() < f64::EPSILON);
    }
}
