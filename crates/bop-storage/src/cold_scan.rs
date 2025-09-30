//! Cold scan API for streaming pages directly from S3.
//!
//! This module implements T9 from the checkpointing plan: cold scan mode
//! that bypasses LocalStore/PageCache for bandwidth-limited queries.
//!
//! # Architecture
//!
//! Cold scan mode is used for:
//! - Analytical queries that scan large datasets once
//! - Backup/restore operations
//! - Data export workflows
//!
//! The cold scan path:
//! 1. Queries manifest for chunk locations
//! 2. Streams chunks directly from S3 (via RemoteStore)
//! 3. Applies bandwidth throttling (per-tenant limits)
//! 4. Bypasses PageCache to avoid polluting hot data
//!
//! # Usage
//!
//! ```ignore
//! let options = ColdScanOptions {
//!     max_bandwidth_bytes_per_sec: Some(10 * 1024 * 1024), // 10 MB/s
//!     bypass_cache: true,
//! };
//!
//! let limiter = RemoteReadLimiter::new(options);
//! let page_data = page_store.read_page_cold(page_no, &limiter).await?;
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::db::DbId;

/// Configuration for cold scan operations.
///
/// # T9a: ColdScanOptions
#[derive(Debug, Clone)]
pub struct ColdScanOptions {
    /// Maximum bandwidth in bytes per second (None = unlimited).
    pub max_bandwidth_bytes_per_sec: Option<u64>,

    /// Bypass PageCache (always fetch from remote).
    pub bypass_cache: bool,

    /// Enable metrics tracking for cold reads.
    pub track_metrics: bool,
}

impl Default for ColdScanOptions {
    fn default() -> Self {
        Self {
            max_bandwidth_bytes_per_sec: None,
            bypass_cache: true,
            track_metrics: true,
        }
    }
}

/// Errors that can occur during cold scan operations.
#[derive(Debug, thiserror::Error)]
pub enum ColdScanError {
    /// Remote store error.
    #[error("remote store error: {0}")]
    RemoteStore(String),

    /// Manifest query error.
    #[error("manifest error: {0}")]
    Manifest(String),

    /// Bandwidth limit exceeded (transient).
    #[error("bandwidth limit exceeded, retry after {0:?}")]
    BandwidthExceeded(Duration),

    /// Page not found.
    #[error("page {page_no} not found")]
    PageNotFound { page_no: u32 },

    /// PageStore error.
    #[error("page store error: {0}")]
    PageStore(String),
}

/// Bandwidth limiter for remote reads.
///
/// # T9a: RemoteReadLimiter
///
/// Implements token bucket algorithm for per-tenant bandwidth limiting.
pub struct RemoteReadLimiter {
    options: ColdScanOptions,
    bytes_read: Arc<AtomicU64>,
    window_start: Arc<std::sync::Mutex<Instant>>,
}

impl RemoteReadLimiter {
    /// Creates a new bandwidth limiter with the given options.
    pub fn new(options: ColdScanOptions) -> Self {
        Self {
            options,
            bytes_read: Arc::new(AtomicU64::new(0)),
            window_start: Arc::new(std::sync::Mutex::new(Instant::now())),
        }
    }

    /// Requests permission to read the given number of bytes.
    ///
    /// Returns Ok(()) if the read can proceed, or Err with retry duration
    /// if the bandwidth limit would be exceeded.
    pub fn request_bytes(&self, bytes: u64) -> Result<(), ColdScanError> {
        if let Some(max_bps) = self.options.max_bandwidth_bytes_per_sec {
            let mut window_start = self.window_start.lock().unwrap();
            let now = Instant::now();
            let elapsed = now.duration_since(*window_start);

            // Reset window if 1 second has passed
            if elapsed >= Duration::from_secs(1) {
                self.bytes_read.store(0, Ordering::Relaxed);
                *window_start = now;
            }

            let current_bytes = self.bytes_read.load(Ordering::Relaxed);
            let new_total = current_bytes + bytes;

            if new_total > max_bps {
                // Calculate how long to wait
                let remaining = Duration::from_secs(1).saturating_sub(elapsed);
                return Err(ColdScanError::BandwidthExceeded(remaining));
            }

            self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
        }

        Ok(())
    }

    /// Records bytes that were read (for metrics).
    pub fn record_bytes_read(&self, bytes: u64) {
        if self.options.track_metrics {
            self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    /// Returns the total bytes read in the current window.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read.load(Ordering::Relaxed)
    }

    /// Resets the limiter state.
    pub fn reset(&self) {
        self.bytes_read.store(0, Ordering::Relaxed);
        *self.window_start.lock().unwrap() = Instant::now();
    }
}

/// Statistics for cold scan operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct ColdScanStats {
    /// Total number of pages read via cold scan.
    pub pages_read: u64,

    /// Total bytes read from remote storage.
    pub bytes_read: u64,

    /// Number of bandwidth throttle events.
    pub throttle_events: u64,

    /// Total time spent reading (includes throttle time).
    pub total_duration_ms: u64,
}

/// Tracker for cold scan statistics per database.
pub struct ColdScanStatsTracker {
    db_id: DbId,
    pages_read: AtomicU64,
    bytes_read: AtomicU64,
    throttle_events: AtomicU64,
    start_time: Instant,
}

impl ColdScanStatsTracker {
    /// Creates a new stats tracker for a database.
    pub fn new(db_id: DbId) -> Self {
        Self {
            db_id,
            pages_read: AtomicU64::new(0),
            bytes_read: AtomicU64::new(0),
            throttle_events: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    /// Records a successful page read.
    pub fn record_page_read(&self, bytes: u64) {
        self.pages_read.fetch_add(1, Ordering::Relaxed);
        self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Records a throttle event.
    pub fn record_throttle(&self) {
        self.throttle_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns the current statistics.
    pub fn stats(&self) -> ColdScanStats {
        ColdScanStats {
            pages_read: self.pages_read.load(Ordering::Relaxed),
            bytes_read: self.bytes_read.load(Ordering::Relaxed),
            throttle_events: self.throttle_events.load(Ordering::Relaxed),
            total_duration_ms: self.start_time.elapsed().as_millis() as u64,
        }
    }

    /// Returns the database ID.
    pub fn db_id(&self) -> DbId {
        self.db_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbId;

    #[test]
    fn cold_scan_options_default() {
        let options = ColdScanOptions::default();
        assert!(options.bypass_cache);
        assert!(options.track_metrics);
        assert!(options.max_bandwidth_bytes_per_sec.is_none());
    }

    #[test]
    fn bandwidth_limiter_unlimited() {
        let options = ColdScanOptions {
            max_bandwidth_bytes_per_sec: None,
            bypass_cache: true,
            track_metrics: false,
        };

        let limiter = RemoteReadLimiter::new(options);

        // Should allow unlimited reads
        assert!(limiter.request_bytes(1_000_000_000).is_ok());
        assert!(limiter.request_bytes(1_000_000_000).is_ok());
    }

    #[test]
    fn bandwidth_limiter_enforces_limit() {
        let options = ColdScanOptions {
            max_bandwidth_bytes_per_sec: Some(1000),
            bypass_cache: true,
            track_metrics: true,
        };

        let limiter = RemoteReadLimiter::new(options);

        // First 1000 bytes should succeed
        assert!(limiter.request_bytes(1000).is_ok());

        // Next request should fail (limit exceeded)
        let result = limiter.request_bytes(1);
        assert!(result.is_err());
    }

    #[test]
    fn bandwidth_limiter_resets_window() {
        let options = ColdScanOptions {
            max_bandwidth_bytes_per_sec: Some(1000),
            bypass_cache: true,
            track_metrics: true,
        };

        let limiter = RemoteReadLimiter::new(options);

        assert!(limiter.request_bytes(1000).is_ok());
        assert_eq!(limiter.bytes_read(), 1000);

        // Reset and try again
        limiter.reset();
        assert_eq!(limiter.bytes_read(), 0);
        assert!(limiter.request_bytes(1000).is_ok());
    }

    #[test]
    fn stats_tracker_records_operations() {
        let tracker = ColdScanStatsTracker::new(DbId::new(42));

        tracker.record_page_read(4096);
        tracker.record_page_read(4096);
        tracker.record_throttle();

        std::thread::sleep(std::time::Duration::from_millis(1));

        let stats = tracker.stats();
        assert_eq!(stats.pages_read, 2);
        assert_eq!(stats.bytes_read, 8192);
        assert_eq!(stats.throttle_events, 1);
        assert!(stats.total_duration_ms > 0);
        assert_eq!(tracker.db_id(), DbId::new(42));
    }

    #[test]
    fn bandwidth_limiter_partial_request() {
        let options = ColdScanOptions {
            max_bandwidth_bytes_per_sec: Some(1000),
            bypass_cache: true,
            track_metrics: true,
        };

        let limiter = RemoteReadLimiter::new(options);

        // First 500 bytes should succeed
        assert!(limiter.request_bytes(500).is_ok());
        assert_eq!(limiter.bytes_read(), 500);

        // Another 500 bytes should succeed
        assert!(limiter.request_bytes(500).is_ok());
        assert_eq!(limiter.bytes_read(), 1000);

        // Next request should fail
        assert!(limiter.request_bytes(1).is_err());
    }

    #[test]
    fn cold_scan_options_builder_pattern() {
        let options = ColdScanOptions {
            max_bandwidth_bytes_per_sec: Some(1_000_000),
            bypass_cache: false,
            track_metrics: true,
        };

        assert_eq!(options.max_bandwidth_bytes_per_sec, Some(1_000_000));
        assert!(!options.bypass_cache);
        assert!(options.track_metrics);
    }
}
