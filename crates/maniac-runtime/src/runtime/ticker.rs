use std::{
    panic,
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

// Note: Panic logging is now per-instance, configured via TickService::new_with_config()
// or TickService::set_panic_logging()

/// RAII guard for a registered tick handler.
/// Automatically unregisters the handler when dropped.
pub struct TickHandlerRegistration {
    tick_service: Weak<TickService>,
    handler_id: u64,
}

impl TickHandlerRegistration {
    fn new(tick_service: &Arc<TickService>, handler_id: u64) -> Self {
        Self {
            tick_service: Arc::downgrade(tick_service),
            handler_id,
        }
    }

}

impl Drop for TickHandlerRegistration {
    fn drop(&mut self) {
        if let Some(service) = self.tick_service.upgrade() {
            service.unregister(self.handler_id);
        }
    }
}

// SAFETY: TickHandlerRegistration only contains thread-safe types
unsafe impl Send for TickHandlerRegistration {}
unsafe impl Sync for TickHandlerRegistration {}

/// Trait for services that need to be notified on each tick.
pub trait TickHandler: Send + Sync {
    /// Returns the desired tick duration for this handler.
    /// The TickService will use the minimum of all registered handlers' tick durations.
    fn tick_duration(&self) -> Duration;

    /// Called on each tick with the current time in nanoseconds and tick count.
    fn on_tick(&self, tick_count: u64, now_ns: u64);

    /// Called when the tick service is shutting down.
    fn on_shutdown(&self);
}

/// State tracking for a registered tick handler.
struct TickHandlerState {
    /// Weak reference to the handler implementation.
    /// This allows handlers to be dropped even if they're still registered.
    handler: Weak<dyn TickHandler>,
    /// Unique identifier for this handler registration.
    handler_id: u64,
    /// How many base ticks must elapse between calls to this handler.
    /// E.g., if base tick is 1ms and handler wants 10ms, tick_interval = 10.
    tick_interval: u64,
    /// The handler's own monotonic tick count, incremented on each invocation.
    handler_tick_count: u64,
    /// Statistics for this handler's tick timing and performance.
    stats: TickStats,
}

/// Statistics for the tick thread to monitor timing accuracy and performance
#[derive(Debug)]
pub struct TickStats {
    /// Average duration of tick loop processing (excluding sleep) in nanoseconds
    pub avg_tick_loop_duration_ns: AtomicU64,
    /// Average sleep duration in nanoseconds
    pub avg_tick_loop_sleep_ns: AtomicU64,
    /// Maximum absolute drift observed in nanoseconds
    pub max_drift_ns: AtomicU64,
    /// Total number of ticks processed
    pub total_ticks: AtomicU64,
    /// Total cumulative drift in nanoseconds (positive = behind schedule)
    pub total_drift_ns: AtomicI64,
    /// Number of ticks where this handler wasn't called (due to interval)
    pub missed_ticks: AtomicU64,
    /// Timestamp of the last tick in nanoseconds
    pub last_tick_ns: AtomicU64,
}

impl Default for TickStats {
    fn default() -> Self {
        Self {
            avg_tick_loop_duration_ns: AtomicU64::new(0),
            avg_tick_loop_sleep_ns: AtomicU64::new(0),
            max_drift_ns: AtomicU64::new(0),
            total_ticks: AtomicU64::new(0),
            total_drift_ns: AtomicI64::new(0),
            missed_ticks: AtomicU64::new(0),
            last_tick_ns: AtomicU64::new(0),
        }
    }
}

impl TickStats {
    /// Returns a snapshot of the current statistics
    pub fn snapshot(&self) -> TickStatsSnapshot {
        TickStatsSnapshot {
            avg_tick_loop_duration_ns: self.avg_tick_loop_duration_ns.load(Ordering::Relaxed),
            avg_tick_loop_sleep_ns: self.avg_tick_loop_sleep_ns.load(Ordering::Relaxed),
            max_drift_ns: self.max_drift_ns.load(Ordering::Relaxed) as i64,
            total_ticks: self.total_ticks.load(Ordering::Relaxed),
            total_drift_ns: self.total_drift_ns.load(Ordering::Relaxed),
            missed_ticks: self.missed_ticks.load(Ordering::Relaxed),
            last_tick_ns: self.last_tick_ns.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of tick statistics at a point in time
#[derive(Clone, Copy, Debug, Default)]
pub struct TickStatsSnapshot {
    /// Average duration of tick loop processing (excluding sleep) in nanoseconds
    pub avg_tick_loop_duration_ns: u64,
    /// Average sleep duration in nanoseconds
    pub avg_tick_loop_sleep_ns: u64,
    /// Maximum absolute drift observed in nanoseconds
    pub max_drift_ns: i64,
    /// Total number of ticks processed
    pub total_ticks: u64,
    /// Total cumulative drift in nanoseconds (positive = behind schedule)
    pub total_drift_ns: i64,
    /// Number of ticks where this handler wasn't called (due to interval)
    pub missed_ticks: u64,
    /// Timestamp of the last tick in nanoseconds
    pub last_tick_ns: u64,
}

/// Shared tick service that can coordinate multiple WorkerServices.
pub struct TickService {
    default_tick_duration: Duration,
    tick_duration_ns: AtomicU64,
    handlers: Mutex<HashMap<u64, TickHandlerState>>,
    shutdown: AtomicBool,
    tick_thread: Mutex<Option<thread::JoinHandle<()>>>,
    tick_stats: TickStats,
    next_handler_id: AtomicU64,
    shutdown_timeout_ns: AtomicU64,
    error_count: AtomicU64,
    log_handler_panics: AtomicBool,
}

impl TickService {
    /// Create a new TickService with the specified default tick duration.
    /// The actual tick duration used will be the minimum of the default and all registered handlers.
    pub fn new(tick_duration: Duration) -> Arc<Self> {
        // Debug assertion to catch duration truncation
        debug_assert!(
            tick_duration.as_nanos() <= u128::from(u64::MAX),
            "Tick duration exceeds u64::MAX nanoseconds, will be truncated"
        );

        Arc::new(Self {
            default_tick_duration: tick_duration,
            // Use Relaxed ordering since this is only set during construction
            tick_duration_ns: AtomicU64::new(
                tick_duration.as_nanos().min(u128::from(u64::MAX)) as u64
            ),
            handlers: Mutex::new(HashMap::new()),
            shutdown: AtomicBool::new(false),
            tick_thread: Mutex::new(None),
            tick_stats: TickStats::default(),
            next_handler_id: AtomicU64::new(1),
            shutdown_timeout_ns: AtomicU64::new(100_000_000), // Default to 100ms
            error_count: AtomicU64::new(0),
            log_handler_panics: AtomicBool::new(false),
        })
    }

    /// Register a handler to be called on each tick.
    /// Returns a registration guard that will automatically unregister the handler when dropped.
    /// Recalculates the base tick duration and all handler intervals.
    /// This is called when handlers are added or removed to maintain optimal timing.
    ///
    /// Note: Must be called while holding the handlers lock to prevent race conditions.
    fn recalculate_tick_duration(&self, handlers: &mut HashMap<u64, TickHandlerState>) {
        let default_duration_ns = self.default_tick_duration.as_nanos().min(u128::from(u64::MAX)) as u64;

        let min_handler_duration_ns = handlers
            .values()
            .filter_map(|state| {
                state.handler.upgrade().map(|h| {
                    let duration_ns = h.tick_duration().as_nanos();
                    // Debug assertion for duration truncation
                    debug_assert!(
                        duration_ns <= u128::from(u64::MAX),
                        "Handler tick duration exceeds u64::MAX nanoseconds, will be truncated"
                    );
                    duration_ns.min(u128::from(u64::MAX)) as u64
                })
            })
            .min()
            .unwrap_or(default_duration_ns);

        // Always store the new base duration and recalculate intervals
        // We hold the handlers lock so this is safe from race conditions
        self.tick_duration_ns.store(min_handler_duration_ns, Ordering::Relaxed);

        // Recalculate intervals for all handlers with the new base duration
        for state in handlers.values_mut() {
            if let Some(handler_arc) = state.handler.upgrade() {
                let h_duration_ns = handler_arc.tick_duration().as_nanos();
                debug_assert!(
                    h_duration_ns <= u128::from(u64::MAX),
                    "Handler tick duration exceeds u64::MAX nanoseconds, will be truncated"
                );
                let h_duration_ns = h_duration_ns.min(u128::from(u64::MAX)) as u64;
                state.tick_interval = if h_duration_ns <= min_handler_duration_ns {
                    1
                } else {
                    (h_duration_ns + min_handler_duration_ns - 1) / min_handler_duration_ns
                };
            }
        }
    }

    pub fn register(
        self: &Arc<Self>,
        handler: Arc<dyn TickHandler>,
    ) -> Option<TickHandlerRegistration> {
        let mut handlers = self.handlers.lock().expect("handlers lock poisoned");
        
        let handler_duration_ns = handler.tick_duration().as_nanos();
        debug_assert!(
            handler_duration_ns <= u128::from(u64::MAX),
            "Handler tick duration exceeds u64::MAX nanoseconds, will be truncated"
        );
        let handler_duration_ns = handler_duration_ns.min(u128::from(u64::MAX)) as u64;

        // Calculate how many base ticks between calls to this handler
        // Use Acquire ordering to get the latest tick duration from other threads
        let current_base_ns = self.tick_duration_ns.load(Ordering::Acquire);
        let tick_interval = if handler_duration_ns <= current_base_ns {
            1 // Call on every tick
        } else {
            (handler_duration_ns + current_base_ns - 1) / current_base_ns // Round up
        };
        
        let handler_id = self.next_handler_id.fetch_add(1, Ordering::Relaxed);
        
        handlers.insert(handler_id, TickHandlerState {
            handler: Arc::downgrade(&handler),
            handler_id,
            tick_interval,
            handler_tick_count: 0,
            stats: TickStats::default(),
        });

        // Update tick duration and recalculate intervals if needed
        self.recalculate_tick_duration(&mut handlers);

        Some(TickHandlerRegistration::new(self, handler_id))
    }

    /// Internal method to unregister a handler by ID.
    /// Returns true if a handler was found and removed.
    fn unregister(&self, handler_id: u64) -> bool {
        let mut handlers = self.handlers.lock().expect("handlers lock poisoned");
        
        let removed = handlers.remove(&handler_id).is_some();
        
        if removed {
            // Recalculate tick duration and intervals after removing a handler
            self.recalculate_tick_duration(&mut handlers);
        }

        removed
    }

    /// Start the tick thread if not already started.
    pub fn start(self: &Arc<Self>) {
        let mut tick_thread_guard = self.tick_thread.lock().expect("tick_thread lock poisoned");
        if tick_thread_guard.is_some() {
            return; // Already started
        }

        let service = Arc::clone(self);

        let handle = thread::spawn(move || {
            let start = Instant::now();
            let mut tick_count: u64 = 0;

            loop {
                let loop_start = Instant::now();

                // Reload tick_duration from atomic on each iteration (allows dynamic adjustment)
                let tick_duration_ns = service.tick_duration_ns.load(Ordering::Relaxed);
                let tick_duration = Duration::from_nanos(tick_duration_ns);

                // Calculate target time for this tick to maintain precise timing
                // We calculate based on the next tick to know when to wake up
                let next_tick = tick_count.wrapping_add(1);
                let target_time = if let Some(target_duration) = tick_duration.checked_mul(next_tick as u32) {
                    start.checked_add(target_duration).unwrap_or_else(|| {
                        // If we overflow Instant, just use now + tick_duration as fallback
                        Instant::now() + tick_duration
                    })
                } else {
                    // tick_count is too large to multiply, use incremental approach
                    Instant::now() + tick_duration
                };

                if service.shutdown.load(Ordering::Acquire) {
                    // Graceful shutdown: notify all handlers with timeout
                    let handlers = service.handlers.lock().expect("handlers lock poisoned");
                    let shutdown_start = Instant::now();
                    
                    for (_handler_id, state) in handlers.iter() {
                        // Check if we've exceeded the shutdown timeout
                        let shutdown_timeout = Duration::from_nanos(service.shutdown_timeout_ns.load(Ordering::Relaxed));
                        if shutdown_start.elapsed() > shutdown_timeout {
                            eprintln!("Warning: Shutdown timeout exceeded, forcing exit");
                            break;
                        }
                        
                        // Call on_shutdown with panic isolation
                        if let Some(handler) = state.handler.upgrade() {
                            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                                handler.on_shutdown();
                            }));
                            if service.log_handler_panics.load(Ordering::Relaxed) && result.is_err() {
                                eprintln!("Warning: Handler on_shutdown() panicked during shutdown");
                                service.error_count.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    break;
                }

                // Calculate current time
                let now_ns = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;

                // Collect handlers to call and release lock before calling them
                // Pre-allocate vectors with estimated capacity to reduce allocations
                let (handlers_to_call, dead_handler_ids): (Vec<_>, Vec<_>) = {
                    let mut handlers = service.handlers.lock().expect("handlers lock poisoned");
                    let estimated_handlers = handlers.len();
                    let mut calls = Vec::with_capacity(estimated_handlers);
                    let mut dead_ids = Vec::new(); // Usually empty, so no pre-allocation
                    
                    for (handler_id, state) in handlers.iter_mut() {
                        // Check if handler is still alive - if not, mark for cleanup immediately
                        // This ensures dead handlers are cleaned up promptly regardless of interval
                        let handler_alive = state.handler.upgrade();

                        if handler_alive.is_none() {
                            dead_ids.push(*handler_id);
                            continue;
                        }

                        // Always update the last tick timestamp
                        state.stats.last_tick_ns.store(now_ns, Ordering::Relaxed);

                        // Only call on_tick if tick_count is a multiple of this handler's interval
                        if tick_count % state.tick_interval == 0 {
                            if let Some(handler) = handler_alive {
                                calls.push((
                                    handler,
                                    state.handler_tick_count,
                                    *handler_id,
                                ));
                                state.handler_tick_count = state.handler_tick_count.wrapping_add(1);
                            }
                        } else {
                            // Track missed tick for statistics
                            state.stats.missed_ticks.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    (calls, dead_ids)
                }; // Lock released here

                // Clean up dead handlers if any
                if !dead_handler_ids.is_empty() {
                    let mut handlers = service.handlers.lock().expect("handlers lock poisoned");
                    for dead_id in dead_handler_ids {
                        handlers.remove(&dead_id);
                    }
                    
                    // Use the shared recalculation method to update tick duration and intervals
                    service.recalculate_tick_duration(&mut handlers);
                }

                // Call handlers outside the lock to prevent deadlocks
                // Process handlers and collect stats updates to minimize lock contention
                let mut stats_updates = Vec::with_capacity(handlers_to_call.len());
                for (handler, handler_tick_count, handler_id) in handlers_to_call {
                    let handler_start = Instant::now();

                    // Call on_tick with panic isolation
                    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                        handler.on_tick(handler_tick_count, now_ns);
                    }));
                    if service.log_handler_panics.load(Ordering::Relaxed) && result.is_err() {
                        eprintln!("Warning: Handler on_tick() panicked for tick {}", tick_count);
                        service.error_count.fetch_add(1, Ordering::Relaxed);
                    }

                    // Measure handler execution duration
                    let handler_duration_ns = handler_start.elapsed().as_nanos() as u64;

                    // Store stats update for batch processing
                    stats_updates.push((handler_id, handler_duration_ns));
                }

                // Batch update handler statistics to minimize lock contention
                if !stats_updates.is_empty() {
                    let mut handlers = service.handlers.lock().expect("handlers lock poisoned");
                    for (handler_id, handler_duration_ns) in stats_updates {
                        if let Some(state) = handlers.get_mut(&handler_id) {
                            let prev_total = state.stats.total_ticks.fetch_add(1, Ordering::Relaxed);
                            let new_total = prev_total + 1;

                            // Update running average for handler duration
                            let new_avg_duration = if prev_total == 0 {
                                handler_duration_ns
                            } else {
                                let prev_avg = state
                                    .stats
                                    .avg_tick_loop_duration_ns
                                    .load(Ordering::Relaxed) as i64;
                                let new_avg = prev_avg
                                    + ((handler_duration_ns as i64 - prev_avg) / new_total as i64);
                                new_avg as u64
                            };
                            state
                                .stats
                                .avg_tick_loop_duration_ns
                                .store(new_avg_duration, Ordering::Relaxed);
                        }
                    }
                }

                tick_count = tick_count.wrapping_add(1);

                // Measure tick loop processing duration
                let loop_end = Instant::now();
                let loop_duration_ns = loop_end.duration_since(loop_start).as_nanos() as u64;

                // Sleep until target time, accounting for processing time
                let now = Instant::now();
                let actual_sleep_ns =
                    if let Some(sleep_duration) = target_time.checked_duration_since(now) {
                        thread::sleep(sleep_duration);
                        sleep_duration.as_nanos() as u64
                    } else {
                        // We're behind schedule - yield but don't sleep to catch up
                        thread::yield_now();
                        0
                    };

                // Calculate drift (positive = behind schedule, negative = ahead of schedule)
                let after_sleep = Instant::now();
                let drift_ns_i64 = if after_sleep >= target_time {
                    // Behind schedule (or exactly on time)
                    let drift = after_sleep.duration_since(target_time).as_nanos();
                    if drift > i64::MAX as u128 {
                        eprintln!("Warning: Extreme positive drift detected ({} ns), exceeding i64::MAX", drift);
                        i64::MAX
                    } else {
                        drift as i64
                    }
                } else {
                    // Ahead of schedule (woke up early) - negative drift
                    let drift = target_time.duration_since(after_sleep).as_nanos();
                    if drift > i64::MAX as u128 {
                        eprintln!("Warning: Extreme negative drift detected ({} ns), exceeding i64::MAX", drift);
                        i64::MIN
                    } else {
                        -(drift as i64)
                    }
                };

                // Update tick stats atomically
                let prev_total = service
                    .tick_stats
                    .total_ticks
                    .fetch_add(1, Ordering::Relaxed);
                let new_total = prev_total + 1;

                // Update running averages using incremental formula
                let new_avg_duration = if prev_total == 0 {
                    loop_duration_ns
                } else {
                    let prev_avg = service
                        .tick_stats
                        .avg_tick_loop_duration_ns
                        .load(Ordering::Relaxed) as i64;
                    let new_avg =
                        prev_avg + ((loop_duration_ns as i64 - prev_avg) / new_total as i64);
                    new_avg as u64
                };
                service
                    .tick_stats
                    .avg_tick_loop_duration_ns
                    .store(new_avg_duration, Ordering::Relaxed);

                let new_avg_sleep = if prev_total == 0 {
                    actual_sleep_ns
                } else {
                    let prev_avg = service
                        .tick_stats
                        .avg_tick_loop_sleep_ns
                        .load(Ordering::Relaxed) as i64;
                    let new_avg =
                        prev_avg + ((actual_sleep_ns as i64 - prev_avg) / new_total as i64);
                    new_avg as u64
                };
                service
                    .tick_stats
                    .avg_tick_loop_sleep_ns
                    .store(new_avg_sleep, Ordering::Relaxed);

                // Track max drift using compare-exchange loop (store absolute value)
                let drift_abs = drift_ns_i64.unsigned_abs();
                let mut current_max = service.tick_stats.max_drift_ns.load(Ordering::Relaxed);
                while drift_abs > current_max {
                    match service.tick_stats.max_drift_ns.compare_exchange_weak(
                        current_max,
                        drift_abs,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(actual) => current_max = actual,
                    }
                }

                // Accumulate total drift
                service
                    .tick_stats
                    .total_drift_ns
                    .fetch_add(drift_ns_i64, Ordering::Relaxed);
            }
        });

        *tick_thread_guard = Some(handle);
    }

    /// Shutdown the tick service and wait for the thread to exit.
    /// This is idempotent - multiple calls are safe.
    pub fn shutdown(&self) {
        // swap returns the OLD value - if it was already true, we've already shut down
        if self.shutdown.swap(true, Ordering::Release) {
            return; // Already shutting down or shut down
        }

        // Join tick thread
        if let Some(handle) = self.tick_thread.lock().expect("tick_thread lock poisoned").take() {
            let _ = handle.join();
        }
    }

    /// Returns a snapshot of the current tick thread statistics
    pub fn tick_stats(&self) -> TickStatsSnapshot {
        self.tick_stats.snapshot()
    }

    /// Returns the current tick duration in nanoseconds
    pub fn current_tick_duration_ns(&self) -> u64 {
        self.tick_duration_ns.load(Ordering::Acquire)
    }

    /// Set the shutdown timeout for handlers
    pub fn set_shutdown_timeout(&self, timeout: Duration) {
        let timeout_ns = timeout.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.shutdown_timeout_ns.store(timeout_ns, Ordering::Relaxed);
    }


    /// Get error statistics for the tick service
    pub fn error_stats(&self) -> (u64, TickStatsSnapshot) {
        (self.error_count.load(Ordering::Relaxed), self.tick_stats.snapshot())
    }

    /// Enable or disable panic logging for handlers
    pub fn set_panic_logging(&self, enabled: bool) {
        self.log_handler_panics.store(enabled, Ordering::Relaxed);
    }

    /// Check if panic logging is enabled
    pub fn is_panic_logging_enabled(&self) -> bool {
        self.log_handler_panics.load(Ordering::Relaxed)
    }

    /// Returns a vector of tick statistics for each registered handler
    pub fn handler_stats(&self) -> Vec<TickStatsSnapshot> {
        let handlers = self.handlers.lock().expect("handlers lock poisoned");
        handlers
            .values()
            .map(|state| state.stats.snapshot())
            .collect()
    }
}

impl Drop for TickService {
    fn drop(&mut self) {
        if !self.shutdown.load(Ordering::Acquire) {
            self.shutdown();
        }
    }
}
