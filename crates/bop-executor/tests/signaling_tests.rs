/// Comprehensive tests for the signaling mechanism, focusing on SignalWaker
/// status bit correctness after bug fixes.
///
/// This test suite validates:
/// 1. SignalWaker status bit lifecycle
/// 2. Status vs summary field independence
/// 3. Partition summary management
/// 4. Bug regression prevention

use bop_executor::signal_waker::SignalWaker;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

// ============================================================================
// UNIT TESTS: SignalWaker Status Bits
// ============================================================================

#[test]
fn test_signal_waker_status_bit_yield_lifecycle() {
    let waker = SignalWaker::new();

    // Initial state: no bits set
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit, "yield bit should start clear");
    assert!(!tasks_bit, "tasks bit should start clear");

    // Mark yield available
    waker.mark_yield();
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(yield_bit, "yield bit should be set after mark_yield");
    assert!(!tasks_bit, "tasks bit should remain clear");

    // Verify permit was added
    assert_eq!(waker.permits(), 1, "should have 1 permit after mark_yield");

    // Try to unmark yield (should clear bit)
    waker.try_unmark_yield();
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit, "yield bit should be cleared after try_unmark_yield");
    assert!(!tasks_bit, "tasks bit should remain clear");

    // Permit should remain (not consumed by try_unmark)
    assert_eq!(
        waker.permits(),
        1,
        "permit should remain after try_unmark_yield"
    );
}

#[test]
fn test_signal_waker_status_bit_tasks_lifecycle() {
    let waker = SignalWaker::new();

    // Initial state
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit);
    assert!(!tasks_bit);

    // Mark tasks available
    waker.mark_tasks();
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit, "yield bit should remain clear");
    assert!(tasks_bit, "tasks bit should be set after mark_tasks");

    // Verify permit was added
    assert_eq!(waker.permits(), 1, "should have 1 permit after mark_tasks");

    // Try to unmark tasks (should clear bit)
    waker.try_unmark_tasks();
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit, "yield bit should remain clear");
    assert!(!tasks_bit, "tasks bit should be cleared after try_unmark_tasks");

    // Permit should remain
    assert_eq!(
        waker.permits(),
        1,
        "permit should remain after try_unmark_tasks"
    );
}

#[test]
fn test_signal_waker_status_bits_idempotent() {
    let waker = SignalWaker::new();

    // Multiple mark_yield calls should only add one permit
    waker.mark_yield();
    waker.mark_yield();
    waker.mark_yield();

    assert_eq!(
        waker.permits(),
        1,
        "multiple mark_yield should only add 1 permit"
    );

    // Multiple mark_tasks calls should only add one permit
    waker.mark_tasks();
    waker.mark_tasks();
    waker.mark_tasks();

    assert_eq!(
        waker.permits(),
        2,
        "should have 2 permits total (1 from yield, 1 from tasks)"
    );
}

#[test]
fn test_signal_waker_status_bits_independent() {
    let waker = SignalWaker::new();

    // Set both bits
    waker.mark_yield();
    waker.mark_tasks();

    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(yield_bit);
    assert!(tasks_bit);

    // Clear only yield bit
    waker.try_unmark_yield();
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit, "yield bit should be cleared");
    assert!(tasks_bit, "tasks bit should remain set");

    // Clear tasks bit
    waker.try_unmark_tasks();
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit, "yield bit should remain clear");
    assert!(!tasks_bit, "tasks bit should be cleared");
}

#[test]
fn test_signal_waker_summary_vs_status_separation() {
    let waker = SignalWaker::new();

    // Mark signal words active in summary
    waker.mark_active(0);
    waker.mark_active(1);
    waker.mark_active(2);

    let summary = waker.snapshot_summary();
    assert_ne!(summary & 0b111, 0, "summary bits 0-2 should be set");

    // Status bits should be independent
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit, "yield status bit should be independent of summary");
    assert!(!tasks_bit, "tasks status bit should be independent of summary");

    // Now set status bits
    waker.mark_yield();
    waker.mark_tasks();

    // Summary should not be affected by status operations
    let summary_after = waker.snapshot_summary();
    assert_eq!(
        summary, summary_after,
        "summary should not change from status operations"
    );

    // Clear status bits
    waker.try_unmark_yield();
    waker.try_unmark_tasks();

    // Summary should still be unchanged
    let summary_final = waker.snapshot_summary();
    assert_eq!(
        summary, summary_final,
        "summary should remain unchanged after status operations"
    );
}

// ============================================================================
// UNIT TESTS: SignalWaker Partition Summary
// ============================================================================

#[test]
fn test_signal_waker_partition_summary_transitions() {
    let waker = SignalWaker::new();

    // Initial state: empty partition
    assert_eq!(waker.partition_summary(), 0);
    let (_, tasks_bit) = waker.status_bits();
    assert!(!tasks_bit);

    // Mark first leaf active (0 → non-zero transition)
    let was_empty = waker.mark_partition_leaf_active(0);
    assert!(was_empty, "partition should have been empty");

    // Should trigger mark_tasks() → status bit set + permit added
    let (_, tasks_bit) = waker.status_bits();
    assert!(tasks_bit, "tasks bit should be set on 0→non-zero transition");
    assert_eq!(waker.permits(), 1, "should have 1 permit");
    assert_eq!(waker.partition_summary(), 1 << 0);

    // Mark second leaf active (non-zero → non-zero, no transition)
    let was_empty = waker.mark_partition_leaf_active(1);
    assert!(!was_empty, "partition should not have been empty");

    // Should NOT add another permit
    assert_eq!(waker.permits(), 1, "should still have only 1 permit");
    assert_eq!(waker.partition_summary(), (1 << 0) | (1 << 1));

    // Clear first leaf (non-zero → non-zero, no transition)
    waker.clear_partition_leaf(0);
    let (_, tasks_bit) = waker.status_bits();
    assert!(tasks_bit, "tasks bit should remain set");
    assert_eq!(waker.partition_summary(), 1 << 1);

    // Clear second leaf (non-zero → 0 transition)
    waker.clear_partition_leaf(1);

    // Should trigger try_unmark_tasks() → status bit cleared
    let (_, tasks_bit) = waker.status_bits();
    assert!(!tasks_bit, "tasks bit should be cleared on non-zero→0 transition");
    assert_eq!(waker.partition_summary(), 0);
}

#[test]
fn test_signal_waker_sync_partition_summary() {
    let waker = SignalWaker::new();

    // Simulate SummaryTree leaf_words
    let leaf_words: Vec<AtomicU64> = (0..8).map(|i| AtomicU64::new(i % 2)).collect();

    // Sync partition [0, 8)
    let has_work = waker.sync_partition_summary(0, 8, &leaf_words);

    // Should have work (leafs 1, 3, 5, 7 are non-zero)
    assert!(has_work, "should detect work in partition");

    let partition_summary = waker.partition_summary();
    let expected = (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7);
    assert_eq!(
        partition_summary, expected,
        "partition summary should match non-zero leaves"
    );

    // Should have set tasks bit and added permit (0→non-zero)
    let (_, tasks_bit) = waker.status_bits();
    assert!(tasks_bit, "tasks bit should be set after sync finds work");
    assert_eq!(waker.permits(), 1);

    // Clear all leaf words
    for leaf_word in &leaf_words {
        leaf_word.store(0, Ordering::Relaxed);
    }

    // Sync again (non-zero → 0 transition)
    let has_work = waker.sync_partition_summary(0, 8, &leaf_words);
    assert!(!has_work, "should detect no work after clearing leaves");

    let partition_summary = waker.partition_summary();
    assert_eq!(partition_summary, 0, "partition summary should be empty");

    // Should have cleared tasks bit (non-zero→0)
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        !tasks_bit,
        "tasks bit should be cleared after sync finds no work"
    );
}

// ============================================================================
// STRESS TESTS: Concurrent Signaling
// ============================================================================

#[test]
fn test_status_bit_race_on_partition_empty_to_nonempty() {
    // Test that rapid empty→non-empty transitions don't lose permits
    let waker = SignalWaker::new();

    // Simulate rapid transitions
    for i in 0..100 {
        waker.mark_partition_leaf_active(i % 64);
        if i % 10 == 0 {
            // Occasionally clear leaves to trigger 0→non-zero again
            for j in 0..64 {
                waker.clear_partition_leaf(j);
            }
        }
    }

    // Should have accumulated some permits
    let permits = waker.permits();
    assert!(permits > 0, "should have accumulated permits from transitions");
}

#[test]
fn test_try_unmark_when_already_clear() {
    let waker = SignalWaker::new();

    // Try to unmark when already clear (should be no-op)
    waker.try_unmark_yield();
    waker.try_unmark_tasks();

    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit);
    assert!(!tasks_bit);
    assert_eq!(waker.permits(), 0);
}

#[test]
fn test_concurrent_signal_waker_operations() {
    let waker = Arc::new(SignalWaker::new());
    let barrier = Arc::new(Barrier::new(3));

    let waker1 = waker.clone();
    let barrier1 = barrier.clone();
    let handle1 = std::thread::spawn(move || {
        barrier1.wait();
        for i in 0..100 {
            waker1.mark_active(i % 64);
        }
    });

    let waker2 = waker.clone();
    let barrier2 = barrier.clone();
    let handle2 = std::thread::spawn(move || {
        barrier2.wait();
        for _ in 0..100 {
            waker2.mark_yield();
            waker2.try_unmark_yield();
        }
    });

    let waker3 = waker.clone();
    let barrier3 = barrier.clone();
    let handle3 = std::thread::spawn(move || {
        barrier3.wait();
        for _ in 0..100 {
            waker3.mark_tasks();
            waker3.try_unmark_tasks();
        }
    });

    handle1.join().unwrap();
    handle2.join().unwrap();
    handle3.join().unwrap();

    // No crashes = success
    let _ = waker.snapshot_summary();
    let _ = waker.status_bits();
}

#[test]
fn test_parking_and_waking() {
    let waker = Arc::new(SignalWaker::new());
    let waker_clone = waker.clone();

    let handle = std::thread::spawn(move || {
        // Try to acquire - should timeout
        let acquired = waker_clone.acquire_timeout(Duration::from_millis(50));
        assert!(!acquired, "should timeout with no permits");

        // Mark tasks and try again
        waker_clone.mark_tasks();
        let acquired = waker_clone.try_acquire();
        assert!(acquired, "should acquire permit after mark_tasks");
    });

    handle.join().unwrap();
}
