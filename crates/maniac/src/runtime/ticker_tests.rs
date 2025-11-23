#![cfg(test)]

use super::ticker::{TickHandler, TickService};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

/// Simple test handler that counts ticks
struct CountingHandler {
    tick_duration: Duration,
    tick_count: AtomicU64,
    last_tick_count: AtomicU64,
    last_now_ns: AtomicU64,
    on_shutdown_called: AtomicBool,
}

impl CountingHandler {
    fn new(tick_duration: Duration) -> Self {
        Self {
            tick_duration,
            tick_count: AtomicU64::new(0),
            last_tick_count: AtomicU64::new(0),
            last_now_ns: AtomicU64::new(0),
            on_shutdown_called: AtomicBool::new(false),
        }
    }

    fn tick_count(&self) -> u64 {
        self.tick_count.load(Ordering::Relaxed)
    }

    fn last_tick_count(&self) -> u64 {
        self.last_tick_count.load(Ordering::Relaxed)
    }

    fn last_now_ns(&self) -> u64 {
        self.last_now_ns.load(Ordering::Relaxed)
    }

    fn shutdown_called(&self) -> bool {
        self.on_shutdown_called.load(Ordering::Relaxed)
    }
}

impl TickHandler for CountingHandler {
    fn tick_duration(&self) -> Duration {
        self.tick_duration
    }

    fn on_tick(&self, tick_count: u64, now_ns: u64) {
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.last_tick_count.store(tick_count, Ordering::Relaxed);
        self.last_now_ns.store(now_ns, Ordering::Relaxed);
    }

    fn on_shutdown(&self) {
        self.on_shutdown_called.store(true, Ordering::Relaxed);
    }
}

/// Handler that records all ticks with timing information
struct RecordingHandler {
    tick_duration: Duration,
    ticks: Mutex<Vec<(u64, u64)>>, // (handler_tick_count, now_ns)
    on_shutdown_called: AtomicBool,
}

impl RecordingHandler {
    fn new(tick_duration: Duration) -> Self {
        Self {
            tick_duration,
            ticks: Mutex::new(Vec::new()),
            on_shutdown_called: AtomicBool::new(false),
        }
    }

    fn ticks(&self) -> Vec<(u64, u64)> {
        self.ticks.lock().unwrap().clone()
    }

    fn shutdown_called(&self) -> bool {
        self.on_shutdown_called.load(Ordering::Relaxed)
    }
}

impl TickHandler for RecordingHandler {
    fn tick_duration(&self) -> Duration {
        self.tick_duration
    }

    fn on_tick(&self, tick_count: u64, now_ns: u64) {
        self.ticks.lock().unwrap().push((tick_count, now_ns));
    }

    fn on_shutdown(&self) {
        self.on_shutdown_called.store(true, Ordering::Relaxed);
    }
}

#[test]
fn test_tick_service_basic_lifecycle() {
    let tick_service = TickService::new(Duration::from_millis(10));
    
    // Service should not be started yet
    let stats = tick_service.tick_stats();
    assert_eq!(stats.total_ticks, 0);
    
    // Start the service
    tick_service.start();
    
    // Wait a bit for ticks to accumulate
    std::thread::sleep(Duration::from_millis(50));
    
    // Should have some ticks now
    let stats = tick_service.tick_stats();
    assert!(stats.total_ticks > 0, "Expected some ticks, got {}", stats.total_ticks);
    
    // Shutdown
    tick_service.shutdown();
    
    // Stats should be frozen after shutdown
    let final_ticks = tick_service.tick_stats().total_ticks;
    std::thread::sleep(Duration::from_millis(30));
    let later_ticks = tick_service.tick_stats().total_ticks;
    assert_eq!(final_ticks, later_ticks, "Ticks should not increase after shutdown");
}

#[test]
fn test_single_handler_registration() {
    let tick_service = TickService::new(Duration::from_millis(10));
    let handler = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    
    // Register handler
    let registration = tick_service.register(handler.clone() as Arc<dyn TickHandler>);
    assert!(registration.is_some(), "Registration should succeed");
    
    // Start ticking
    tick_service.start();
    std::thread::sleep(Duration::from_millis(50));
    
    // Handler should have received ticks
    assert!(handler.tick_count() > 0, "Handler should receive ticks");
    
    // Shutdown should call on_shutdown
    tick_service.shutdown();
    assert!(handler.shutdown_called(), "Handler should receive shutdown notification");
}

#[test]
fn test_multiple_handler_registrations() {
    let tick_service = TickService::new(Duration::from_millis(10));
    let handler = Arc::new(CountingHandler::new(Duration::from_millis(10)));

    // Multiple registrations of the same handler instance are allowed
    // Each gets its own handler_id and registration guard
    let reg1 = tick_service.register(handler.clone() as Arc<dyn TickHandler>);
    assert!(reg1.is_some(), "First registration should succeed");

    let reg2 = tick_service.register(handler.clone() as Arc<dyn TickHandler>);
    assert!(reg2.is_some(), "Second registration should succeed");

    tick_service.shutdown();
}

#[test]
fn test_multiple_handlers_same_tick_rate() {
    let tick_service = TickService::new(Duration::from_millis(10));
    
    let handler1 = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    let handler2 = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    let handler3 = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    
    let _reg1 = tick_service.register(handler1.clone() as Arc<dyn TickHandler>);
    let _reg2 = tick_service.register(handler2.clone() as Arc<dyn TickHandler>);
    let _reg3 = tick_service.register(handler3.clone() as Arc<dyn TickHandler>);
    
    tick_service.start();
    std::thread::sleep(Duration::from_millis(60));
    tick_service.shutdown();
    
    // All handlers should receive approximately the same number of ticks
    let count1 = handler1.tick_count();
    let count2 = handler2.tick_count();
    let count3 = handler3.tick_count();
    
    assert!(count1 > 0, "Handler 1 should receive ticks");
    assert!(count2 > 0, "Handler 2 should receive ticks");
    assert!(count3 > 0, "Handler 3 should receive ticks");
    
    // Allow for some variance due to timing
    assert!((count1 as i64 - count2 as i64).abs() <= 2, 
        "Handlers should have similar tick counts: {} vs {}", count1, count2);
    assert!((count2 as i64 - count3 as i64).abs() <= 2,
        "Handlers should have similar tick counts: {} vs {}", count2, count3);
}

#[test]
fn test_multiple_handlers_different_tick_rates() {
    let tick_service = TickService::new(Duration::from_millis(5));
    
    // Handler 1: every 5ms (1:1 with base)
    let handler1 = Arc::new(RecordingHandler::new(Duration::from_millis(5)));
    // Handler 2: every 10ms (1:2 with base)
    let handler2 = Arc::new(RecordingHandler::new(Duration::from_millis(10)));
    // Handler 3: every 20ms (1:4 with base)
    let handler3 = Arc::new(RecordingHandler::new(Duration::from_millis(20)));
    
    let _reg1 = tick_service.register(handler1.clone() as Arc<dyn TickHandler>);
    let _reg2 = tick_service.register(handler2.clone() as Arc<dyn TickHandler>);
    let _reg3 = tick_service.register(handler3.clone() as Arc<dyn TickHandler>);
    
    tick_service.start();
    std::thread::sleep(Duration::from_millis(100));
    tick_service.shutdown();
    
    let count1 = handler1.ticks().len();
    let count2 = handler2.ticks().len();
    let count3 = handler3.ticks().len();
    
    // Handler 1 should tick most frequently
    assert!(count1 > count2, "Handler 1 (5ms) should tick more than Handler 2 (10ms): {} vs {}", count1, count2);
    assert!(count2 > count3, "Handler 2 (10ms) should tick more than Handler 3 (20ms): {} vs {}", count2, count3);
    
    // Approximate ratio check (allowing for timing variance)
    let ratio_2_1 = count1 as f64 / count2 as f64;
    let ratio_3_2 = count2 as f64 / count3 as f64;
    
    assert!(ratio_2_1 > 1.5 && ratio_2_1 < 2.5, 
        "Handler 1:2 ratio should be ~2.0, got {}", ratio_2_1);
    assert!(ratio_3_2 > 1.5 && ratio_3_2 < 2.5,
        "Handler 2:3 ratio should be ~2.0, got {}", ratio_3_2);
}

#[test]
fn test_handler_monotonic_tick_counts() {
    let tick_service = TickService::new(Duration::from_millis(5));
    let handler = Arc::new(RecordingHandler::new(Duration::from_millis(10)));
    
    let _reg = tick_service.register(handler.clone() as Arc<dyn TickHandler>);
    
    tick_service.start();
    std::thread::sleep(Duration::from_millis(60));
    tick_service.shutdown();
    
    let ticks = handler.ticks();
    assert!(ticks.len() >= 5, "Should have at least 5 ticks");
    
    // Verify monotonic tick counts (0, 1, 2, 3, ...)
    for (i, &(tick_count, _)) in ticks.iter().enumerate() {
        assert_eq!(tick_count, i as u64, 
            "Tick count should be monotonic: expected {} at index {}, got {}", 
            i, i, tick_count);
    }
}

#[test]
fn test_handler_registration_guard_drop() {
    let tick_service = TickService::new(Duration::from_millis(10));
    let handler = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    
    tick_service.start();
    
    {
        let _reg = tick_service.register(handler.clone() as Arc<dyn TickHandler>);
        std::thread::sleep(Duration::from_millis(30));
        
        // Handler should receive ticks while registered
        assert!(handler.tick_count() > 0);
    } // Registration dropped here
    
    let count_after_drop = handler.tick_count();
    std::thread::sleep(Duration::from_millis(30));
    let count_later = handler.tick_count();
    
    // Handler should not receive more ticks after unregistration
    assert_eq!(count_after_drop, count_later, 
        "Handler should not receive ticks after unregistration");
    
    tick_service.shutdown();
}

#[test]
fn test_dynamic_tick_duration_adjustment() {
    let tick_service = TickService::new(Duration::from_millis(20));
    
    // Initial tick duration should be 20ms
    let initial_duration = tick_service.current_tick_duration_ns();
    assert_eq!(initial_duration, 20_000_000);
    
    // Register handler with shorter duration
    let handler1 = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    let _reg1 = tick_service.register(handler1);
    
    // Tick duration should adjust to 10ms
    let adjusted_duration = tick_service.current_tick_duration_ns();
    assert_eq!(adjusted_duration, 10_000_000);
    
    // Register handler with even shorter duration
    let handler2 = Arc::new(CountingHandler::new(Duration::from_millis(5)));
    let _reg2 = tick_service.register(handler2);
    
    // Tick duration should adjust to 5ms
    let final_duration = tick_service.current_tick_duration_ns();
    assert_eq!(final_duration, 5_000_000);
    
    tick_service.shutdown();
}

#[test]
fn test_tick_duration_restoration_after_unregister() {
    let tick_service = TickService::new(Duration::from_millis(20));
    
    let handler1 = Arc::new(CountingHandler::new(Duration::from_millis(5)));
    let handler2 = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    
    let reg1 = tick_service.register(handler1.clone() as Arc<dyn TickHandler>);
    let _reg2 = tick_service.register(handler2.clone() as Arc<dyn TickHandler>);
    
    // Should be 5ms (minimum)
    assert_eq!(tick_service.current_tick_duration_ns(), 5_000_000);
    
    // Drop handler1 (5ms)
    drop(reg1);
    
    // Should adjust back to 10ms
    assert_eq!(tick_service.current_tick_duration_ns(), 10_000_000);
    
    tick_service.shutdown();
}

#[test]
fn test_tick_stats_accumulation() {
    let tick_service = TickService::new(Duration::from_millis(5));
    
    tick_service.start();
    std::thread::sleep(Duration::from_millis(60));
    tick_service.shutdown();
    
    let stats = tick_service.tick_stats();
    
    // Should have accumulated ticks
    assert!(stats.total_ticks > 0, "Should have ticks");
    
    // Average tick duration should be reasonable (not zero, not huge)
    assert!(stats.avg_tick_loop_duration_ns > 0, "Should have non-zero tick duration");
    assert!(stats.avg_tick_loop_duration_ns < 5_000_000, // Less than 5ms
        "Tick duration should be reasonable: {} ns", stats.avg_tick_loop_duration_ns);
    
    // Sleep duration should be close to tick duration
    assert!(stats.avg_tick_loop_sleep_ns > 1_000_000, // At least 1ms
        "Sleep duration should be reasonable: {} ns", stats.avg_tick_loop_sleep_ns);
}

#[test]
fn test_handler_stats_tracking() {
    let tick_service = TickService::new(Duration::from_millis(5));
    
    let handler1 = Arc::new(CountingHandler::new(Duration::from_millis(5)));
    let handler2 = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    
    let _reg1 = tick_service.register(handler1.clone() as Arc<dyn TickHandler>);
    let _reg2 = tick_service.register(handler2.clone() as Arc<dyn TickHandler>);
    
    tick_service.start();
    std::thread::sleep(Duration::from_millis(60));
    tick_service.shutdown();
    
    let handler_stats = tick_service.handler_stats();
    assert_eq!(handler_stats.len(), 2, "Should have stats for 2 handlers");

    // Find which stat corresponds to which handler by tick count
    // Handler with 5ms interval should have more ticks than 10ms interval
    let (fast_stats, slow_stats) = if handler_stats[0].total_ticks > handler_stats[1].total_ticks {
        (&handler_stats[0], &handler_stats[1])
    } else {
        (&handler_stats[1], &handler_stats[0])
    };

    // Both handlers should have ticks and duration
    assert!(fast_stats.total_ticks > 0, "Fast handler should have ticks");
    assert!(fast_stats.avg_tick_loop_duration_ns > 0, "Fast handler should have duration");
    assert!(slow_stats.total_ticks > 0, "Slow handler should have ticks");
    assert!(slow_stats.avg_tick_loop_duration_ns > 0, "Slow handler should have duration");

    // Fast handler (5ms) should have more ticks than slow handler (10ms)
    assert!(fast_stats.total_ticks > slow_stats.total_ticks,
        "Handler with 5ms interval should have more ticks than handler with 10ms interval: {} vs {}",
        fast_stats.total_ticks, slow_stats.total_ticks);
}

#[test]
fn test_concurrent_registration_unregistration() {
    let tick_service = TickService::new(Duration::from_millis(5));
    tick_service.start();
    
    let service = Arc::clone(&tick_service);
    
    // Spawn threads that register and unregister handlers
    let handles: Vec<_> = (0..4).map(|_| {
        let svc = Arc::clone(&service);
        std::thread::spawn(move || {
            for _ in 0..10 {
                let handler = Arc::new(CountingHandler::new(Duration::from_millis(10)));
                let reg = svc.register(handler);
                if let Some(reg) = reg {
                    std::thread::sleep(Duration::from_millis(5));
                    drop(reg);
                }
            }
        })
    }).collect();
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    tick_service.shutdown();
}

#[test]
fn test_empty_service_runs_without_handlers() {
    let tick_service = TickService::new(Duration::from_millis(10));
    
    tick_service.start();
    std::thread::sleep(Duration::from_millis(50));
    
    let stats = tick_service.tick_stats();
    assert!(stats.total_ticks > 0, "Service should tick even without handlers");
    
    tick_service.shutdown();
}

#[test]
fn test_handler_receives_increasing_now_ns() {
    let tick_service = TickService::new(Duration::from_millis(5));
    let handler = Arc::new(RecordingHandler::new(Duration::from_millis(5)));

    let _reg = tick_service.register(handler.clone() as Arc<dyn TickHandler>);

    tick_service.start();
    std::thread::sleep(Duration::from_millis(50));
    tick_service.shutdown();

    let ticks = handler.ticks();
    assert!(ticks.len() >= 5, "Should have at least 5 ticks");
    
    // Verify now_ns is monotonically increasing
    for i in 1..ticks.len() {
        let (_, prev_ns) = ticks[i - 1];
        let (_, curr_ns) = ticks[i];
        assert!(curr_ns > prev_ns, 
            "now_ns should increase: {} -> {}", prev_ns, curr_ns);
    }
}

#[test]
fn test_shutdown_idempotency() {
    let tick_service = TickService::new(Duration::from_millis(10));
    
    tick_service.start();
    std::thread::sleep(Duration::from_millis(30));
    
    // First shutdown
    tick_service.shutdown();
    let ticks1 = tick_service.tick_stats().total_ticks;
    
    // Second shutdown should be safe
    tick_service.shutdown();
    let ticks2 = tick_service.tick_stats().total_ticks;
    
    assert_eq!(ticks1, ticks2, "Multiple shutdowns should be safe");
}

#[test]
fn test_start_idempotency() {
    let tick_service = TickService::new(Duration::from_millis(10));
    
    // First start
    tick_service.start();
    std::thread::sleep(Duration::from_millis(30));
    let ticks1 = tick_service.tick_stats().total_ticks;
    
    // Second start should be no-op
    tick_service.start();
    std::thread::sleep(Duration::from_millis(30));
    let ticks2 = tick_service.tick_stats().total_ticks;
    
    // Should have more ticks (proving the service is still running)
    assert!(ticks2 > ticks1, "Ticks should continue accumulating");
    // The tick count should be roughly linear, not exponential
    // If we had multiple threads, we'd expect significantly more ticks
    // But this is a weak assertion since timing can vary
    
    tick_service.shutdown();
}

#[test]
fn test_manual_unregister() {
    let tick_service = TickService::new(Duration::from_millis(10));
    let handler = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    
    tick_service.start();
    
    let reg = tick_service.register(handler.clone() as Arc<dyn TickHandler>);
    std::thread::sleep(Duration::from_millis(30));
    
    let count_before = handler.tick_count();
    assert!(count_before > 0);
    
    // Unregister by dropping the registration guard
    drop(reg);
    
    // Wait a bit to ensure any in-progress tick completes
    std::thread::sleep(Duration::from_millis(5));
    let count_immediately_after = handler.tick_count();
    
    std::thread::sleep(Duration::from_millis(30));
    let count_after = handler.tick_count();
    
    // Count should not increase (or at most by 1 if a tick was already in progress)
    assert!(count_after <= count_immediately_after + 1, 
        "Handler should not tick after unregistration: {} vs {}", 
        count_immediately_after, count_after);
    
    tick_service.shutdown();
}

#[test]
fn test_fast_handler_slow_handler_coexistence() {
    let tick_service = TickService::new(Duration::from_millis(100));
    
    // Very fast handler (1ms)
    let fast_handler = Arc::new(CountingHandler::new(Duration::from_millis(1)));
    // Slow handler (50ms)
    let slow_handler = Arc::new(CountingHandler::new(Duration::from_millis(50)));
    
    let _reg1 = tick_service.register(fast_handler.clone() as Arc<dyn TickHandler>);
    let _reg2 = tick_service.register(slow_handler.clone() as Arc<dyn TickHandler>);
    
    // Base tick should adjust to 1ms
    assert_eq!(tick_service.current_tick_duration_ns(), 1_000_000);
    
    tick_service.start();
    std::thread::sleep(Duration::from_millis(150));
    tick_service.shutdown();
    
    let fast_count = fast_handler.tick_count();
    let slow_count = slow_handler.tick_count();
    
    // Fast handler should tick many more times
    assert!(fast_count > slow_count * 10, 
        "Fast handler should tick much more: {} vs {}", fast_count, slow_count);
    
    // Approximate ratio check (50:1)
    let ratio = fast_count as f64 / slow_count as f64;
    assert!(ratio > 30.0 && ratio < 70.0, 
        "Ratio should be approximately 50:1, got {}", ratio);
}

#[test]
fn test_drop_behavior() {
    let handler = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    
    {
        let tick_service = TickService::new(Duration::from_millis(10));
        let _reg = tick_service.register(handler.clone() as Arc<dyn TickHandler>);
        
        tick_service.start();
        std::thread::sleep(Duration::from_millis(30));
        
        assert!(handler.tick_count() > 0);
        
        // Explicitly shutdown before drop to ensure on_shutdown is called
        tick_service.shutdown();
    } // tick_service dropped here
    
    // Shutdown was called explicitly, so handler should have received notification
    assert!(handler.shutdown_called(), "Handler should receive shutdown notification");
}

#[test]
fn test_zero_handlers_after_all_unregistered() {
    let tick_service = TickService::new(Duration::from_millis(10));
    
    let handler1 = Arc::new(CountingHandler::new(Duration::from_millis(5)));
    let handler2 = Arc::new(CountingHandler::new(Duration::from_millis(10)));
    
    let reg1 = tick_service.register(handler1.clone() as Arc<dyn TickHandler>);
    let reg2 = tick_service.register(handler2.clone() as Arc<dyn TickHandler>);
    
    tick_service.start();
    std::thread::sleep(Duration::from_millis(30));
    
    // Unregister all handlers
    drop(reg1);
    drop(reg2);
    
    // Service should continue running with no handlers
    std::thread::sleep(Duration::from_millis(30));
    
    let stats = tick_service.tick_stats();
    assert!(stats.total_ticks > 0, "Service should continue ticking");
    
    tick_service.shutdown();
}
