use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::error::BackpressureKind;

/// Point-in-time snapshot of admission control metrics.
///
/// Provides insights into system backpressure, latency, and
/// flow control behavior across all AOF operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct AdmissionMetricsSnapshot {
    /// Total latency across all operations (nanoseconds)
    pub total_latency_ns: u64,
    /// Number of latency measurements taken
    pub latency_samples: u64,
    /// Operations blocked due to admission control
    pub would_block_admission: u64,
    /// Operations blocked due to segment rollover
    pub would_block_rollover: u64,
    /// Operations blocked due to hydration backpressure
    pub would_block_hydration: u64,
    /// Operations blocked due to flush backpressure
    pub would_block_flush: u64,
    /// Operations blocked due to tail following
    pub would_block_tail: u64,
    /// Operations blocked for unknown reasons
    pub would_block_unknown: u64,
}

impl AdmissionMetricsSnapshot {
    pub fn average_latency_ns(&self) -> u64 {
        if self.latency_samples == 0 {
            return 0;
        }
        self.total_latency_ns / self.latency_samples
    }
}

/// Thread-safe metrics collection for admission control.
///
/// Tracks system backpressure events and operation latencies
/// using atomic counters for concurrent access from all
/// AOF operations.
///
/// ## Threading
///
/// All operations are lock-free and can be called concurrently
/// from any thread without coordination.
#[derive(Debug, Default)]
pub struct AdmissionMetrics {
    /// Accumulated latency in nanoseconds
    total_latency_ns: AtomicU64,
    /// Count of latency samples
    latency_samples: AtomicU64,
    /// Admission control blocks
    would_block_admission: AtomicU64,
    /// Rollover backpressure blocks
    would_block_rollover: AtomicU64,
    /// Hydration backpressure blocks
    would_block_hydration: AtomicU64,
    /// Flush backpressure blocks
    would_block_flush: AtomicU64,
    /// Tail following blocks
    would_block_tail: AtomicU64,
    /// Unknown/unclassified blocks
    would_block_unknown: AtomicU64,
}

impl AdmissionMetrics {
    pub fn record_latency(&self, latency: Duration) {
        let nanos = latency.as_nanos().min(u64::MAX as u128) as u64;
        self.total_latency_ns.fetch_add(nanos, Ordering::Relaxed);
        self.latency_samples.fetch_add(1, Ordering::Relaxed);
    }

    pub fn incr_would_block(&self, kind: BackpressureKind) {
        match kind {
            BackpressureKind::Admission => {
                self.would_block_admission.fetch_add(1, Ordering::Relaxed);
            }
            BackpressureKind::Rollover => {
                self.would_block_rollover.fetch_add(1, Ordering::Relaxed);
            }
            BackpressureKind::Hydration => {
                self.would_block_hydration.fetch_add(1, Ordering::Relaxed);
            }
            BackpressureKind::Flush => {
                self.would_block_flush.fetch_add(1, Ordering::Relaxed);
            }
            BackpressureKind::Tail => {
                self.would_block_tail.fetch_add(1, Ordering::Relaxed);
            }
            BackpressureKind::Unknown => {
                self.would_block_unknown.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn snapshot(&self) -> AdmissionMetricsSnapshot {
        AdmissionMetricsSnapshot {
            total_latency_ns: self.total_latency_ns.load(Ordering::Relaxed),
            latency_samples: self.latency_samples.load(Ordering::Relaxed),
            would_block_admission: self.would_block_admission.load(Ordering::Relaxed),
            would_block_rollover: self.would_block_rollover.load(Ordering::Relaxed),
            would_block_hydration: self.would_block_hydration.load(Ordering::Relaxed),
            would_block_flush: self.would_block_flush.load(Ordering::Relaxed),
            would_block_tail: self.would_block_tail.load(Ordering::Relaxed),
            would_block_unknown: self.would_block_unknown.load(Ordering::Relaxed),
        }
    }
}
