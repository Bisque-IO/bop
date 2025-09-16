// Production-level socket wrapper with backpressure and flow control
// This provides a high-level interface for managing socket connections with proper resource management

use crate::usockets::{LoopInner, Socket, SocketContext, SocketContextOptions};
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

/// Rate limiting algorithm
#[derive(Debug, Clone)]
pub enum RateLimitAlgorithm {
    /// Token bucket algorithm
    TokenBucket,
    /// Sliding window algorithm
    SlidingWindow,
    /// Fixed window algorithm
    FixedWindow,
}

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum bytes per second
    pub max_bytes_per_second: usize,
    /// Maximum operations per second
    pub max_operations_per_second: usize,
    /// Burst capacity (bytes)
    pub burst_capacity: usize,
    /// Rate limiting algorithm
    pub algorithm: RateLimitAlgorithm,
    /// Window size for sliding/fixed window (seconds)
    pub window_size: u64,
    /// Enable rate limiting
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_bytes_per_second: 1024 * 1024, // 1MB/s
            max_operations_per_second: 10000,  // 10K ops/s
            burst_capacity: 2 * 1024 * 1024,   // 2MB burst
            algorithm: RateLimitAlgorithm::TokenBucket,
            window_size: 1, // 1 second window
            enabled: true,
        }
    }
}

/// Configuration for production socket wrapper
#[derive(Debug, Clone)]
pub struct ProductionSocketConfig {
    /// Maximum buffer size before applying backpressure (bytes)
    pub max_buffer_size: usize,
    /// High watermark - start applying backpressure (bytes)
    pub high_watermark: usize,
    /// Low watermark - resume after backpressure (bytes)
    pub low_watermark: usize,
    /// Maximum time to pause before forcing drain (milliseconds)
    pub max_pause_duration: u64,
    /// Chunk size for writing buffered data (bytes)
    pub write_chunk_size: usize,
    /// Enable automatic corking for small writes
    pub enable_corking: bool,
    /// Cork timeout in milliseconds
    pub cork_timeout: u64,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
}

impl Default for ProductionSocketConfig {
    fn default() -> Self {
        Self {
            max_buffer_size: 1024 * 1024, // 1MB max buffer
            high_watermark: 768 * 1024,   // 768KB - start backpressure
            low_watermark: 256 * 1024,    // 256KB - resume normal operation
            max_pause_duration: 5000,     // 5 seconds max pause
            write_chunk_size: 64 * 1024,  // 64KB chunks
            enable_corking: true,
            cork_timeout: 10, // 10ms cork timeout
            rate_limit: RateLimitConfig::default(),
        }
    }
}

/// Socket state for tracking connection lifecycle
#[derive(Debug, Clone, PartialEq)]
pub enum SocketState {
    Connecting,
    Connected,
    Paused,
    Draining,
    Closing,
    Closed,
    Error(String),
}

/// Backpressure strategy
#[derive(Debug, Clone)]
pub enum BackpressureStrategy {
    /// Pause reading until buffer drains below low watermark
    PauseReading,
    /// Drop oldest data to make room
    DropOldest,
    /// Drop newest data  
    DropNewest,
    /// Close connection when buffer full
    CloseConnection,
}

/// Statistics for monitoring socket performance
#[derive(Debug, Default)]
pub struct SocketStats {
    pub bytes_received: AtomicUsize,
    pub bytes_sent: AtomicUsize,
    pub bytes_buffered: AtomicUsize,
    pub backpressure_events: AtomicUsize,
    pub pause_events: AtomicUsize,
    pub resume_events: AtomicUsize,
    pub cork_events: AtomicUsize,
    pub drain_events: AtomicUsize,
    pub rate_limit_events: AtomicUsize,
    pub operations_count: AtomicUsize,
    pub connection_time: Option<Instant>,
    pub last_activity: Option<Instant>,
}

/// Rate limiter using token bucket algorithm
#[derive(Debug)]
pub struct TokenBucketRateLimiter {
    /// Maximum tokens (burst capacity)
    max_tokens: usize,
    /// Current token count
    tokens: AtomicUsize,
    /// Token refill rate per second
    refill_rate: usize,
    /// Last refill timestamp
    last_refill: Mutex<Instant>,
    /// Operations counter for ops/second limiting
    operations: AtomicUsize,
    /// Operations rate limit
    max_ops_per_second: usize,
    /// Last ops reset timestamp
    last_ops_reset: Mutex<Instant>,
}

impl TokenBucketRateLimiter {
    pub fn new(config: &RateLimitConfig) -> Self {
        let now = Instant::now();
        Self {
            max_tokens: config.burst_capacity,
            tokens: AtomicUsize::new(config.burst_capacity),
            refill_rate: config.max_bytes_per_second,
            last_refill: Mutex::new(now),
            operations: AtomicUsize::new(0),
            max_ops_per_second: config.max_operations_per_second,
            last_ops_reset: Mutex::new(now),
        }
    }

    /// Try to consume tokens for bytes
    pub fn try_consume_bytes(&self, bytes: usize) -> bool {
        self.refill_tokens();

        let current = self.tokens.load(Ordering::Relaxed);
        if current >= bytes {
            // Try to atomically subtract tokens
            match self.tokens.compare_exchange_weak(
                current,
                current - bytes,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => true,
                Err(_) => false, // Another thread modified it, rate limit
            }
        } else {
            false
        }
    }

    /// Try to consume one operation
    pub fn try_consume_operation(&self) -> bool {
        self.reset_operations_if_needed();

        let current = self.operations.load(Ordering::Relaxed);
        if current < self.max_ops_per_second {
            self.operations.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill_tokens(&self) {
        let now = Instant::now();
        let mut last_refill = self.last_refill.lock().unwrap();
        let elapsed = now.duration_since(*last_refill);

        if elapsed >= Duration::from_millis(100) {
            // Refill every 100ms
            let tokens_to_add = (self.refill_rate as f64 * elapsed.as_secs_f64()) as usize;
            if tokens_to_add > 0 {
                let current = self.tokens.load(Ordering::Relaxed);
                let new_tokens = (current + tokens_to_add).min(self.max_tokens);
                self.tokens.store(new_tokens, Ordering::Relaxed);
                *last_refill = now;
            }
        }
    }

    /// Reset operations counter if window has passed
    fn reset_operations_if_needed(&self) {
        let now = Instant::now();
        let mut last_reset = self.last_ops_reset.lock().unwrap();

        if now.duration_since(*last_reset) >= Duration::from_secs(1) {
            self.operations.store(0, Ordering::Relaxed);
            *last_reset = now;
        }
    }

    /// Get current token count
    pub fn available_tokens(&self) -> usize {
        self.refill_tokens();
        self.tokens.load(Ordering::Relaxed)
    }

    /// Get current operations count
    pub fn operations_used(&self) -> usize {
        self.reset_operations_if_needed();
        self.operations.load(Ordering::Relaxed)
    }
}

/// Internal write buffer entry
#[derive(Debug)]
struct BufferEntry {
    data: Vec<u8>,
    timestamp: Instant,
    retries: usize,
}

/// Production socket wrapper with backpressure and flow control
pub struct ProductionSocket {
    /// Configuration
    config: ProductionSocketConfig,
    /// Current socket state
    state: Arc<Mutex<SocketState>>,
    /// Write buffer with backpressure control
    write_buffer: Arc<Mutex<VecDeque<BufferEntry>>>,
    /// Current buffer size in bytes
    buffer_size: AtomicUsize,
    /// Is reading paused due to backpressure
    reading_paused: AtomicBool,
    /// Is writing paused
    writing_paused: AtomicBool,
    /// Corking state for small write optimization
    corked: AtomicBool,
    /// Last cork timestamp
    last_cork: Arc<Mutex<Option<Instant>>>,
    /// Backpressure strategy
    pub backpressure_strategy: BackpressureStrategy,
    /// Rate limiter
    rate_limiter: Option<TokenBucketRateLimiter>,
    /// Statistics
    stats: Arc<SocketStats>,
    /// Callbacks for application events
    on_data: Option<Box<dyn Fn(&[u8]) + Send + Sync>>,
    on_drain: Option<Box<dyn Fn() + Send + Sync>>,
    on_backpressure: Option<Box<dyn Fn(bool) + Send + Sync>>,
    on_rate_limit: Option<Box<dyn Fn(usize, usize) + Send + Sync>>, // (bytes_requested, available_tokens)
    on_error: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl ProductionSocket {
    /// Create new production socket wrapper
    pub fn new(config: ProductionSocketConfig) -> Self {
        let rate_limiter = if config.rate_limit.enabled {
            Some(TokenBucketRateLimiter::new(&config.rate_limit))
        } else {
            None
        };

        Self {
            state: Arc::new(Mutex::new(SocketState::Connecting)),
            write_buffer: Arc::new(Mutex::new(VecDeque::new())),
            buffer_size: AtomicUsize::new(0),
            reading_paused: AtomicBool::new(false),
            writing_paused: AtomicBool::new(false),
            corked: AtomicBool::new(false),
            last_cork: Arc::new(Mutex::new(None)),
            backpressure_strategy: BackpressureStrategy::PauseReading,
            rate_limiter,
            stats: Arc::new(SocketStats::default()),
            on_data: None,
            on_drain: None,
            on_backpressure: None,
            on_rate_limit: None,
            on_error: None,
            config,
        }
    }

    /// Set data callback
    pub fn on_data<F>(mut self, callback: F) -> Self
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        self.on_data = Some(Box::new(callback));
        self
    }

    /// Set drain callback (called when write buffer empties)
    pub fn on_drain<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_drain = Some(Box::new(callback));
        self
    }

    /// Set backpressure callback (called when backpressure starts/stops)
    pub fn on_backpressure<F>(mut self, callback: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.on_backpressure = Some(Box::new(callback));
        self
    }

    /// Set rate limit callback (called when rate limited)
    pub fn on_rate_limit<F>(mut self, callback: F) -> Self
    where
        F: Fn(usize, usize) + Send + Sync + 'static,
    {
        self.on_rate_limit = Some(Box::new(callback));
        self
    }

    /// Set error callback
    pub fn on_error<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_error = Some(Box::new(callback));
        self
    }

    /// Write data with backpressure and rate limiting
    pub fn write(&self, data: &[u8]) -> Result<bool, String> {
        // Check rate limiting first
        if let Some(rate_limiter) = &self.rate_limiter {
            // Check both operation and bytes rate limiting
            if !rate_limiter.try_consume_operation() {
                self.stats.rate_limit_events.fetch_add(1, Ordering::Relaxed);

                if let Some(callback) = &self.on_rate_limit {
                    callback(data.len(), rate_limiter.operations_used());
                }

                return Ok(false); // Rate limited - operation not allowed
            }

            if !rate_limiter.try_consume_bytes(data.len()) {
                self.stats.rate_limit_events.fetch_add(1, Ordering::Relaxed);

                if let Some(callback) = &self.on_rate_limit {
                    callback(data.len(), rate_limiter.available_tokens());
                }

                return Ok(false); // Rate limited - not enough tokens
            }
        }

        let current_size = self.buffer_size.load(Ordering::Relaxed);

        // Check if we're over the maximum buffer size
        if current_size + data.len() > self.config.max_buffer_size {
            match self.backpressure_strategy {
                BackpressureStrategy::DropOldest => {
                    self.drop_oldest_data(data.len());
                }
                BackpressureStrategy::DropNewest => {
                    return Ok(false); // Indicate data was dropped
                }
                BackpressureStrategy::CloseConnection => {
                    self.set_state(SocketState::Error("Buffer overflow".to_string()));
                    return Err("Buffer overflow - connection closed".to_string());
                }
                BackpressureStrategy::PauseReading => {
                    // Continue with buffering, reading will be paused
                }
            }
        }

        // Add to write buffer
        let entry = BufferEntry {
            data: data.to_vec(),
            timestamp: Instant::now(),
            retries: 0,
        };

        {
            let mut buffer = self
                .write_buffer
                .lock()
                .map_err(|e| format!("Lock error: {}", e))?;
            buffer.push_back(entry);
        }

        let new_size = current_size + data.len();
        self.buffer_size.store(new_size, Ordering::Relaxed);

        // Check for backpressure threshold
        if new_size >= self.config.high_watermark && !self.reading_paused.load(Ordering::Relaxed) {
            self.apply_backpressure();
        }

        // Handle corking for small writes
        if self.config.enable_corking && data.len() < 1024 {
            self.maybe_cork();
        }

        Ok(true)
    }

    /// Force flush all buffered data
    pub fn flush(&self) -> Result<usize, String> {
        self.uncork();
        self.drain_write_buffer()
    }

    /// Pause reading (application-level pause)
    pub fn pause_reading(&self) {
        if !self.reading_paused.swap(true, Ordering::Relaxed) {
            self.stats.pause_events.fetch_add(1, Ordering::Relaxed);

            if let Some(callback) = &self.on_backpressure {
                callback(true);
            }
        }
    }

    /// Resume reading
    pub fn resume_reading(&self) {
        if self.reading_paused.swap(false, Ordering::Relaxed) {
            self.stats.resume_events.fetch_add(1, Ordering::Relaxed);

            if let Some(callback) = &self.on_backpressure {
                callback(false);
            }
        }
    }

    /// Check if reading is paused
    pub fn is_reading_paused(&self) -> bool {
        self.reading_paused.load(Ordering::Relaxed)
    }

    /// Get current socket state
    pub fn state(&self) -> SocketState {
        self.state.lock().unwrap().clone()
    }

    /// Get socket statistics
    pub fn stats(&self) -> &SocketStats {
        &self.stats
    }

    /// Get current buffer utilization (0.0 - 1.0)
    pub fn buffer_utilization(&self) -> f64 {
        let current = self.buffer_size.load(Ordering::Relaxed) as f64;
        let max = self.config.max_buffer_size as f64;
        (current / max).min(1.0)
    }

    /// Check if socket has backpressure
    pub fn has_backpressure(&self) -> bool {
        let current_size = self.buffer_size.load(Ordering::Relaxed);
        current_size >= self.config.high_watermark
    }

    /// Check if socket is rate limited
    pub fn is_rate_limited(&self) -> bool {
        if let Some(rate_limiter) = &self.rate_limiter {
            rate_limiter.available_tokens() < 1024 || // Less than 1KB available
            rate_limiter.operations_used() > rate_limiter.max_ops_per_second - 100 // Near ops limit
        } else {
            false
        }
    }

    /// Get current rate limiter status
    pub fn rate_limit_status(&self) -> Option<(usize, usize, usize, usize)> {
        if let Some(rate_limiter) = &self.rate_limiter {
            Some((
                rate_limiter.available_tokens(),
                rate_limiter.max_tokens,
                rate_limiter.operations_used(),
                rate_limiter.max_ops_per_second,
            ))
        } else {
            None
        }
    }

    /// Get rate limiting efficiency (0.0 - 1.0)
    pub fn rate_limit_efficiency(&self) -> f64 {
        if let Some((available, max, ops_used, max_ops)) = self.rate_limit_status() {
            let token_efficiency = available as f64 / max as f64;
            let ops_efficiency = 1.0 - (ops_used as f64 / max_ops as f64);
            (token_efficiency + ops_efficiency) / 2.0
        } else {
            1.0 // No rate limiting = 100% efficiency
        }
    }

    // Internal methods

    fn set_state(&self, new_state: SocketState) {
        let mut state = self.state.lock().unwrap();
        *state = new_state;
    }

    fn apply_backpressure(&self) {
        self.reading_paused.store(true, Ordering::Relaxed);
        self.stats
            .backpressure_events
            .fetch_add(1, Ordering::Relaxed);
        self.stats.pause_events.fetch_add(1, Ordering::Relaxed);

        if let Some(callback) = &self.on_backpressure {
            callback(true);
        }
    }

    fn check_resume_condition(&self) {
        let current_size = self.buffer_size.load(Ordering::Relaxed);

        if current_size <= self.config.low_watermark && self.reading_paused.load(Ordering::Relaxed)
        {
            self.reading_paused.store(false, Ordering::Relaxed);
            self.stats.resume_events.fetch_add(1, Ordering::Relaxed);

            if let Some(callback) = &self.on_backpressure {
                callback(false);
            }
        }
    }

    fn drop_oldest_data(&self, needed_space: usize) {
        let mut buffer = self.write_buffer.lock().unwrap();
        let mut freed_space = 0;

        while freed_space < needed_space && !buffer.is_empty() {
            if let Some(entry) = buffer.pop_front() {
                freed_space += entry.data.len();
            }
        }

        self.buffer_size.fetch_sub(freed_space, Ordering::Relaxed);
    }

    fn maybe_cork(&self) {
        if !self.corked.swap(true, Ordering::Relaxed) {
            *self.last_cork.lock().unwrap() = Some(Instant::now());
            self.stats.cork_events.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn uncork(&self) {
        self.corked.store(false, Ordering::Relaxed);
        *self.last_cork.lock().unwrap() = None;
    }

    fn should_uncork(&self) -> bool {
        if !self.corked.load(Ordering::Relaxed) {
            return false;
        }

        if let Some(cork_time) = *self.last_cork.lock().unwrap() {
            cork_time.elapsed().as_millis() >= self.config.cork_timeout as u128
        } else {
            false
        }
    }

    /// Drain write buffer - returns number of bytes processed
    pub fn drain_write_buffer(&self) -> Result<usize, String> {
        let mut buffer = self
            .write_buffer
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;
        let mut bytes_processed = 0;
        let chunk_size = self.config.write_chunk_size;

        // Process in chunks to avoid blocking
        while !buffer.is_empty() && bytes_processed < chunk_size {
            if let Some(entry) = buffer.pop_front() {
                bytes_processed += entry.data.len();

                // Here you would write to actual socket
                // For now, just simulate successful write
                self.stats
                    .bytes_sent
                    .fetch_add(entry.data.len(), Ordering::Relaxed);
            }
        }

        // Update buffer size
        self.buffer_size
            .fetch_sub(bytes_processed, Ordering::Relaxed);

        // Check if we should resume reading
        self.check_resume_condition();

        // Call drain callback if buffer is now empty
        if buffer.is_empty() && bytes_processed > 0 {
            self.stats.drain_events.fetch_add(1, Ordering::Relaxed);

            if let Some(callback) = &self.on_drain {
                callback();
            }
        }

        Ok(bytes_processed)
    }

    /// Process tick for periodic maintenance
    pub fn tick(&self) -> Result<(), String> {
        // Handle cork timeout
        if self.should_uncork() {
            self.uncork();
            self.drain_write_buffer()?;
        }

        // Check for forced drain due to max pause duration
        if self.reading_paused.load(Ordering::Relaxed) {
            // Force drain if paused too long
            self.drain_write_buffer()?;
        }

        Ok(())
    }
}

// Thread-safe implementation
unsafe impl Send for ProductionSocket {}
unsafe impl Sync for ProductionSocket {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_production_socket_creation() {
        let config = ProductionSocketConfig::default();
        let socket = ProductionSocket::new(config);

        assert_eq!(socket.state(), SocketState::Connecting);
        assert!(!socket.has_backpressure());
        assert!(!socket.is_reading_paused());
        assert_eq!(socket.buffer_utilization(), 0.0);
    }

    #[test]
    fn test_backpressure_triggering() {
        let config = ProductionSocketConfig {
            high_watermark: 1000,
            low_watermark: 500,
            max_buffer_size: 2000,
            ..Default::default()
        };

        let socket = ProductionSocket::new(config);

        // Write data below high watermark
        let result = socket.write(&vec![0u8; 500]);
        assert!(result.is_ok());
        assert!(!socket.has_backpressure());

        // Write data to trigger backpressure
        let result = socket.write(&vec![0u8; 600]);
        assert!(result.is_ok());
        assert!(socket.has_backpressure());
        assert!(socket.is_reading_paused());
    }

    #[test]
    fn test_buffer_overflow_protection() {
        let config = ProductionSocketConfig {
            max_buffer_size: 1000,
            high_watermark: 800,
            ..Default::default()
        };

        let mut socket = ProductionSocket::new(config);
        socket.backpressure_strategy = BackpressureStrategy::CloseConnection;

        // Try to write more than max buffer size
        let result = socket.write(&vec![0u8; 1500]);
        assert!(result.is_err());

        match socket.state() {
            SocketState::Error(msg) => assert!(msg.contains("Buffer overflow")),
            _ => panic!("Expected error state"),
        }
    }

    #[test]
    fn test_drain_and_resume() {
        let config = ProductionSocketConfig {
            high_watermark: 1000,
            low_watermark: 400,
            max_buffer_size: 2000,
            ..Default::default()
        };

        let socket = ProductionSocket::new(config);

        // Fill buffer to trigger backpressure
        socket.write(&vec![0u8; 1100]).unwrap();
        assert!(socket.has_backpressure());

        // Drain buffer
        let drained = socket.drain_write_buffer().unwrap();
        assert!(drained > 0);

        // Should resume when below low watermark
        assert!(!socket.is_reading_paused());
    }

    #[test]
    fn test_statistics_tracking() {
        let config = ProductionSocketConfig::default();
        let socket = ProductionSocket::new(config);

        // Generate some activity
        socket.write(&vec![0u8; 500]).unwrap();
        socket.pause_reading();
        socket.resume_reading();
        socket.drain_write_buffer().unwrap();

        let stats = socket.stats();
        assert!(stats.pause_events.load(Ordering::Relaxed) > 0);
        assert!(stats.resume_events.load(Ordering::Relaxed) > 0);
        assert!(stats.bytes_sent.load(Ordering::Relaxed) > 0);
    }
}
