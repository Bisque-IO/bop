use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use bop_rs::aof::flush::{FlushController, FlushType};
use bop_rs::aof::record::FlushStrategy;

#[test]
fn flush_controller_flush_type_transitions() {
    let controller = FlushController::new(FlushStrategy::BatchedOrPeriodic {
        batch_size: 2,
        interval_ms: 1,
    });

    // No pending work yet, expect forced flush
    assert!(matches!(controller.flush_type(), FlushType::Forced));

    // Accumulate enough appends to trigger batched behaviour
    controller.record_append(128);
    controller.record_append(256);
    assert!(matches!(controller.flush_type(), FlushType::Batched));

    // Record a successful batched flush; pending counters reset
    controller.record_flush(FlushType::Batched, 150, 2, 384);
    assert!(matches!(controller.flush_type(), FlushType::Forced));

    // Allow the periodic interval to elapse without new appends
    thread::sleep(Duration::from_millis(3));
    assert!(matches!(controller.flush_type(), FlushType::Periodic));
}

#[test]
fn flush_metrics_success_rate_and_latency() {
    let controller = FlushController::new(FlushStrategy::Sync);

    // First flush succeeds
    controller.record_append(64);
    let window = controller.prepare_window_with_start(std::time::Instant::now());
    controller.record_flush(FlushType::Forced, 100, window.records, window.bytes);

    // Second flush fails
    controller.record_flush_failure(200);

    let metrics = controller.metrics();
    assert_eq!(metrics.total_flushes.load(Ordering::Relaxed), 2);
    assert_eq!(metrics.successful_flushes.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.failed_flushes.load(Ordering::Relaxed), 1);

    assert_eq!(metrics.flush_success_rate(), 50.0);
    assert_eq!(metrics.average_flush_latency_us(), 150);
}
