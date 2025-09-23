use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::error::BackpressureKind;

#[derive(Debug, Default, Clone, Copy)]
pub struct AdmissionMetricsSnapshot {
    pub total_latency_ns: u64,
    pub latency_samples: u64,
    pub would_block_admission: u64,
    pub would_block_rollover: u64,
    pub would_block_hydration: u64,
    pub would_block_flush: u64,
    pub would_block_tail: u64,
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

#[derive(Debug, Default)]
pub struct AdmissionMetrics {
    total_latency_ns: AtomicU64,
    latency_samples: AtomicU64,
    would_block_admission: AtomicU64,
    would_block_rollover: AtomicU64,
    would_block_hydration: AtomicU64,
    would_block_flush: AtomicU64,
    would_block_tail: AtomicU64,
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

