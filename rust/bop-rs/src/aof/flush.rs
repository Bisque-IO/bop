use crossbeam::queue::SegQueue;
use hdrhistogram::Histogram;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::aof::record::{FlushStrategy, current_timestamp};

/// Comprehensive flush metrics with HDR histograms and detailed counters
#[derive(Debug)]
pub struct FlushControllerMetrics {
    // Flush operation counters
    pub total_flushes: AtomicU64,
    pub successful_flushes: AtomicU64,
    pub failed_flushes: AtomicU64,
    pub forced_flushes: AtomicU64,
    pub periodic_flushes: AtomicU64,
    pub batched_flushes: AtomicU64,

    // Record counters
    pub total_records_flushed: AtomicU64,
    pub total_bytes_flushed: AtomicU64,
    pub pending_records: AtomicU64,
    pub pending_bytes: AtomicU64,

    // Timing counters
    pub total_flush_time_us: AtomicU64,
    pub last_flush_timestamp: AtomicU64,
    pub longest_flush_us: AtomicU64,
    pub shortest_flush_us: AtomicU64,

    // HDR Histograms for latency tracking (microseconds, 1-60 second range)
    pub flush_latency_histogram: Mutex<Histogram<u64>>,
    pub records_per_flush_histogram: Mutex<Histogram<u64>>,
    pub bytes_per_flush_histogram: Mutex<Histogram<u64>>,

    // Throughput metrics
    pub records_per_second: AtomicU64,
    pub bytes_per_second: AtomicU64,
    pub average_record_size: AtomicU64,

    // Efficiency metrics
    pub flush_efficiency_percent: AtomicU64, // (records_flushed / pending_at_start) * 100
    pub time_between_flushes_us: AtomicU64,
}

impl Default for FlushControllerMetrics {
    fn default() -> Self {
        Self {
            total_flushes: AtomicU64::new(0),
            successful_flushes: AtomicU64::new(0),
            failed_flushes: AtomicU64::new(0),
            forced_flushes: AtomicU64::new(0),
            periodic_flushes: AtomicU64::new(0),
            batched_flushes: AtomicU64::new(0),
            total_records_flushed: AtomicU64::new(0),
            total_bytes_flushed: AtomicU64::new(0),
            pending_records: AtomicU64::new(0),
            pending_bytes: AtomicU64::new(0),
            total_flush_time_us: AtomicU64::new(0),
            last_flush_timestamp: AtomicU64::new(current_timestamp()),
            longest_flush_us: AtomicU64::new(0),
            shortest_flush_us: AtomicU64::new(u64::MAX),
            flush_latency_histogram: Mutex::new(Histogram::new(3).unwrap()),
            records_per_flush_histogram: Mutex::new(Histogram::new(3).unwrap()),
            bytes_per_flush_histogram: Mutex::new(Histogram::new(3).unwrap()),
            records_per_second: AtomicU64::new(0),
            bytes_per_second: AtomicU64::new(0),
            average_record_size: AtomicU64::new(0),
            flush_efficiency_percent: AtomicU64::new(0),
            time_between_flushes_us: AtomicU64::new(0),
        }
    }
}

impl FlushControllerMetrics {
    /// Record a successful flush operation with comprehensive metrics
    pub fn record_flush(
        &self,
        latency_us: u64,
        records_flushed: u64,
        bytes_flushed: u64,
        flush_type: FlushType,
    ) {
        let now = current_timestamp();
        let last_flush = self.last_flush_timestamp.swap(now, Ordering::Relaxed);

        // Update counters
        self.total_flushes.fetch_add(1, Ordering::Relaxed);
        self.successful_flushes.fetch_add(1, Ordering::Relaxed);
        self.total_records_flushed
            .fetch_add(records_flushed, Ordering::Relaxed);
        self.total_bytes_flushed
            .fetch_add(bytes_flushed, Ordering::Relaxed);
        self.total_flush_time_us
            .fetch_add(latency_us, Ordering::Relaxed);

        // Update timing extremes
        let mut current_longest = self.longest_flush_us.load(Ordering::Relaxed);
        while latency_us > current_longest {
            match self.longest_flush_us.compare_exchange_weak(
                current_longest,
                latency_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_longest = actual,
            }
        }

        let mut current_shortest = self.shortest_flush_us.load(Ordering::Relaxed);
        while latency_us < current_shortest {
            match self.shortest_flush_us.compare_exchange_weak(
                current_shortest,
                latency_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_shortest = actual,
            }
        }

        // Update flush type counters
        match flush_type {
            FlushType::Forced => self.forced_flushes.fetch_add(1, Ordering::Relaxed),
            FlushType::Periodic => self.periodic_flushes.fetch_add(1, Ordering::Relaxed),
            FlushType::Batched => self.batched_flushes.fetch_add(1, Ordering::Relaxed),
        };

        // Calculate time between flushes
        if last_flush > 0 {
            self.time_between_flushes_us
                .store(now - last_flush, Ordering::Relaxed);
        }

        // Update histograms
        if let Ok(mut hist) = self.flush_latency_histogram.lock() {
            let _ = hist.record(latency_us);
        }
        if let Ok(mut hist) = self.records_per_flush_histogram.lock() {
            let _ = hist.record(records_flushed);
        }
        if let Ok(mut hist) = self.bytes_per_flush_histogram.lock() {
            let _ = hist.record(bytes_flushed);
        }

        // Calculate throughput (records and bytes per second)
        if latency_us > 0 {
            let records_per_sec = (records_flushed * 1_000_000) / latency_us;
            let bytes_per_sec = (bytes_flushed * 1_000_000) / latency_us;
            self.records_per_second
                .store(records_per_sec, Ordering::Relaxed);
            self.bytes_per_second
                .store(bytes_per_sec, Ordering::Relaxed);
        }

        // Calculate average record size
        if records_flushed > 0 {
            self.average_record_size
                .store(bytes_flushed / records_flushed, Ordering::Relaxed);
        }

        Self::decrement_pending(&self.pending_records, records_flushed);
        Self::decrement_pending(&self.pending_bytes, bytes_flushed);
    }

    /// Record a failed flush operation
    pub fn record_flush_failure(&self, latency_us: u64) {
        self.total_flushes.fetch_add(1, Ordering::Relaxed);
        self.failed_flushes.fetch_add(1, Ordering::Relaxed);
        self.total_flush_time_us
            .fetch_add(latency_us, Ordering::Relaxed);
    }

    /// Record an append operation
    pub fn record_append(&self, record_size: u64) {
        self.pending_records.fetch_add(1, Ordering::Relaxed);
        self.pending_bytes.fetch_add(record_size, Ordering::Relaxed);
    }

    /// Get percentile latency from flush histogram
    pub fn get_flush_latency_percentile(&self, percentile: f64) -> Option<u64> {
        Some(
            self.flush_latency_histogram
                .lock()
                .ok()?
                .value_at_percentile(percentile),
        )
    }

    /// Get percentile from records per flush histogram
    pub fn get_records_per_flush_percentile(&self, percentile: f64) -> Option<u64> {
        Some(
            self.records_per_flush_histogram
                .lock()
                .ok()?
                .value_at_percentile(percentile),
        )
    }

    /// Get percentile from bytes per flush histogram
    pub fn get_bytes_per_flush_percentile(&self, percentile: f64) -> Option<u64> {
        Some(
            self.bytes_per_flush_histogram
                .lock()
                .ok()?
                .value_at_percentile(percentile),
        )
    }

    /// Get current throughput in records per second
    pub fn current_records_per_second(&self) -> u64 {
        self.records_per_second.load(Ordering::Relaxed)
    }

    /// Get current throughput in bytes per second
    pub fn current_bytes_per_second(&self) -> u64 {
        self.bytes_per_second.load(Ordering::Relaxed)
    }

    /// Get flush success rate as percentage
    pub fn flush_success_rate(&self) -> f64 {
        let total = self.total_flushes.load(Ordering::Relaxed);
        if total == 0 {
            return 100.0;
        }
        let successful = self.successful_flushes.load(Ordering::Relaxed);
        (successful as f64 / total as f64) * 100.0
    }

    /// Get average flush latency in microseconds
    pub fn average_flush_latency_us(&self) -> u64 {
        let total_time = self.total_flush_time_us.load(Ordering::Relaxed);
        let total_flushes = self.total_flushes.load(Ordering::Relaxed);
        if total_flushes == 0 {
            return 0;
        }
        total_time / total_flushes
    }

    fn decrement_pending(counter: &AtomicU64, amount: u64) {
        if amount == 0 {
            return;
        }

        let mut current = counter.load(Ordering::Relaxed);
        loop {
            let new = current.saturating_sub(amount);
            match counter.compare_exchange(current, new, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }
}

/// Types of flush operations for metrics tracking
#[derive(Debug, Clone, Copy)]
pub enum FlushType {
    Forced,   // Manually triggered flush
    Periodic, // Time-based flush
    Batched,  // Batch size threshold flush
}

#[derive(Clone, Copy, Debug)]
pub struct FlushWindow {
    pub start_time: Instant,
    pub flush_type: FlushType,
    pub records: u64,
    pub bytes: u64,
}

/// Enhanced flush control with comprehensive metrics
pub struct FlushController {
    strategy: FlushStrategy,
    last_flush: Arc<AtomicU64>,
    metrics: Arc<FlushControllerMetrics>,
    #[allow(dead_code)]
    flush_queue: Arc<SegQueue<u64>>,
}

impl FlushController {
    pub fn new(strategy: FlushStrategy) -> Self {
        Self {
            strategy,
            last_flush: Arc::new(AtomicU64::new(current_timestamp())),
            metrics: Arc::new(FlushControllerMetrics::default()),
            flush_queue: Arc::new(SegQueue::new()),
        }
    }

    /// Get access to comprehensive flush metrics
    pub fn metrics(&self) -> &Arc<FlushControllerMetrics> {
        &self.metrics
    }

    pub fn should_flush(&self) -> bool {
        match &self.strategy {
            FlushStrategy::Sync => true,
            FlushStrategy::Async => false,
            FlushStrategy::Batched(size) => {
                self.metrics.pending_records.load(Ordering::Acquire) >= *size as u64
            }
            FlushStrategy::Periodic(interval_ms) => {
                let now = current_timestamp();
                let last = self.last_flush.load(Ordering::Acquire);
                (now - last) >= (*interval_ms * 1000) // Convert ms to microseconds
            }
            FlushStrategy::BatchedOrPeriodic {
                batch_size,
                interval_ms,
            } => {
                let pending = self.metrics.pending_records.load(Ordering::Acquire);
                let now = current_timestamp();
                let last = self.last_flush.load(Ordering::Acquire);
                pending >= *batch_size as u64 || (now - last) >= (*interval_ms * 1000)
            }
        }
    }

    /// Determine the type of flush based on current conditions
    pub fn flush_type(&self) -> FlushType {
        match &self.strategy {
            FlushStrategy::Sync => FlushType::Forced,
            FlushStrategy::Async => FlushType::Forced,
            FlushStrategy::Batched(size) => {
                if self.metrics.pending_records.load(Ordering::Acquire) >= *size as u64 {
                    FlushType::Batched
                } else {
                    FlushType::Forced
                }
            }
            FlushStrategy::Periodic(_) => FlushType::Periodic,
            FlushStrategy::BatchedOrPeriodic {
                batch_size,
                interval_ms,
            } => {
                let pending = self.metrics.pending_records.load(Ordering::Acquire);
                let now = current_timestamp();
                let last = self.last_flush.load(Ordering::Acquire);

                if pending >= *batch_size as u64 {
                    FlushType::Batched
                } else if (now - last) >= (*interval_ms * 1000) {
                    FlushType::Periodic
                } else {
                    FlushType::Forced
                }
            }
        }
    }

    pub fn record_append(&self, record_size: u64) {
        self.metrics.record_append(record_size);
    }

    pub fn prepare_window_with_start(&self, start: Instant) -> FlushWindow {
        FlushWindow {
            start_time: start,
            flush_type: self.flush_type(),
            records: self.metrics.pending_records.load(Ordering::Acquire),
            bytes: self.metrics.pending_bytes.load(Ordering::Acquire),
        }
    }

    pub fn prepare_window(&self) -> FlushWindow {
        self.prepare_window_with_start(Instant::now())
    }

    pub fn complete_window_success(&self, window: &FlushWindow) {
        let latency_us = window.start_time.elapsed().as_micros() as u64;
        self.record_flush(window.flush_type, latency_us, window.records, window.bytes);
    }

    pub fn complete_window_failure(&self, window: &FlushWindow) {
        let latency_us = window.start_time.elapsed().as_micros() as u64;
        self.record_flush_failure_with_type(window.flush_type, latency_us);
    }

    pub fn record_flush(
        &self,
        flush_type: FlushType,
        latency_us: u64,
        records_flushed: u64,
        bytes_flushed: u64,
    ) {
        self.metrics
            .record_flush(latency_us, records_flushed, bytes_flushed, flush_type);
        self.last_flush
            .store(current_timestamp(), Ordering::Relaxed);
    }

    pub fn record_flush_failure_with_type(&self, flush_type: FlushType, latency_us: u64) {
        self.metrics.record_flush_failure(latency_us);
        if matches!(flush_type, FlushType::Forced) {
            self.last_flush
                .store(current_timestamp(), Ordering::Relaxed);
        }
    }

    pub fn record_flush_failure(&self, latency_us: u64) {
        self.record_flush_failure_with_type(FlushType::Forced, latency_us);
    }
}

impl Clone for FlushController {
    fn clone(&self) -> Self {
        Self {
            strategy: self.strategy.clone(),
            last_flush: Arc::clone(&self.last_flush),
            metrics: Arc::clone(&self.metrics),
            flush_queue: Arc::clone(&self.flush_queue),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn flush_window_success_updates_metrics() {
        let controller = FlushController::new(FlushStrategy::Batched(1));
        controller.record_append(128);
        let window = controller.prepare_window_with_start(Instant::now());

        controller.complete_window_success(&window);

        let metrics = controller.metrics();
        assert_eq!(metrics.pending_records.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.pending_bytes.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.successful_flushes.load(Ordering::Relaxed), 1);
        assert_eq!(
            metrics.total_records_flushed.load(Ordering::Relaxed),
            window.records
        );
        assert_eq!(
            metrics.total_bytes_flushed.load(Ordering::Relaxed),
            window.bytes
        );
    }

    #[test]
    fn flush_window_failure_preserves_pending_counts() {
        let controller = FlushController::new(FlushStrategy::Sync);
        controller.record_append(256);
        let window = controller.prepare_window_with_start(Instant::now());

        controller.complete_window_failure(&window);

        let metrics = controller.metrics();
        assert_eq!(metrics.failed_flushes.load(Ordering::Relaxed), 1);
        assert_eq!(
            metrics.pending_records.load(Ordering::Relaxed),
            window.records
        );
        assert_eq!(metrics.pending_bytes.load(Ordering::Relaxed), window.bytes);
    }
}
