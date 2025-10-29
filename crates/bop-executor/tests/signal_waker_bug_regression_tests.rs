/// Regression tests for the critical bugs found in SignalWaker:
///
/// BUG #1: try_unmark_tasks() was clearing summary bit instead of status bit
/// BUG #2: try_unmark_yield() was clearing summary bit instead of status bit
///
/// These tests ensure the fixes remain correct and detect any future regressions.

use bop_executor::signal_waker::SignalWaker;
use std::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// REGRESSION TEST: try_unmark_tasks() Bug
// ============================================================================

#[test]
fn regression_try_unmark_tasks_clears_status_not_summary() {
    let waker = SignalWaker::new();

    // Set some summary bits to detect corruption
    waker.mark_active(0);
    waker.mark_active(1); // Bit 1 in summary - should NOT be affected by try_unmark_tasks
    waker.mark_active(2);

    let initial_summary = waker.snapshot_summary();
    assert_eq!(
        initial_summary & 0b111,
        0b111,
        "summary bits 0-2 should be set"
    );

    // Set status bit 1 (tasks available)
    waker.mark_tasks();
    let (_, tasks_bit) = waker.status_bits();
    assert!(tasks_bit, "status bit 1 should be set");

    // Clear status bit 1 via try_unmark_tasks
    waker.try_unmark_tasks();

    // CRITICAL: Status bit 1 should be cleared
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        !tasks_bit,
        "BUG REGRESSION: status bit 1 should be cleared by try_unmark_tasks"
    );

    // CRITICAL: Summary bit 1 should be UNCHANGED
    let final_summary = waker.snapshot_summary();
    assert_eq!(
        final_summary, initial_summary,
        "BUG REGRESSION: summary should not be affected by try_unmark_tasks (was clearing summary bit 1)"
    );
    assert_ne!(
        final_summary & (1 << 1),
        0,
        "BUG REGRESSION: summary bit 1 should still be set"
    );
}

#[test]
fn regression_try_unmark_yield_clears_status_not_summary() {
    let waker = SignalWaker::new();

    // Set some summary bits to detect corruption
    waker.mark_active(0); // Bit 0 in summary - should NOT be affected by try_unmark_yield
    waker.mark_active(1);
    waker.mark_active(2);

    let initial_summary = waker.snapshot_summary();
    assert_eq!(
        initial_summary & 0b111,
        0b111,
        "summary bits 0-2 should be set"
    );

    // Set status bit 0 (yield available)
    waker.mark_yield();
    let (yield_bit, _) = waker.status_bits();
    assert!(yield_bit, "status bit 0 should be set");

    // Clear status bit 0 via try_unmark_yield
    waker.try_unmark_yield();

    // CRITICAL: Status bit 0 should be cleared
    let (yield_bit, _) = waker.status_bits();
    assert!(
        !yield_bit,
        "BUG REGRESSION: status bit 0 should be cleared by try_unmark_yield"
    );

    // CRITICAL: Summary bit 0 should be UNCHANGED
    let final_summary = waker.snapshot_summary();
    assert_eq!(
        final_summary, initial_summary,
        "BUG REGRESSION: summary should not be affected by try_unmark_yield (was clearing summary bit 0)"
    );
    assert_ne!(
        final_summary & (1 << 0),
        0,
        "BUG REGRESSION: summary bit 0 should still be set"
    );
}

// ============================================================================
// REGRESSION TEST: Status Bit Lifecycle After Fixes
// ============================================================================

#[test]
fn regression_status_bits_full_lifecycle() {
    let waker = SignalWaker::new();

    // Set summary bits (should be independent)
    for i in 0..10 {
        waker.mark_active(i);
    }

    let summary = waker.snapshot_summary();
    assert_ne!(summary, 0, "summary should have bits set");

    // Cycle status bits multiple times
    for iteration in 0..5 {
        // Set both status bits
        waker.mark_yield();
        waker.mark_tasks();

        let (yield_bit, tasks_bit) = waker.status_bits();
        assert!(
            yield_bit,
            "iteration {}: yield bit should be set",
            iteration
        );
        assert!(
            tasks_bit,
            "iteration {}: tasks bit should be set",
            iteration
        );

        // Clear both status bits
        waker.try_unmark_yield();
        waker.try_unmark_tasks();

        let (yield_bit, tasks_bit) = waker.status_bits();
        assert!(
            !yield_bit,
            "iteration {}: yield bit should be cleared",
            iteration
        );
        assert!(
            !tasks_bit,
            "iteration {}: tasks bit should be cleared",
            iteration
        );

        // Summary should remain unchanged throughout
        let summary_current = waker.snapshot_summary();
        assert_eq!(
            summary, summary_current,
            "iteration {}: summary should remain unchanged",
            iteration
        );
    }
}

// ============================================================================
// REGRESSION TEST: Partition Summary Updates Status Correctly
// ============================================================================

#[test]
fn regression_sync_partition_summary_updates_status_not_summary() {
    let waker = SignalWaker::new();

    // Set some summary bits for signal words
    waker.mark_active(5);
    waker.mark_active(10);
    waker.mark_active(15);

    let initial_summary = waker.snapshot_summary();

    // Create leaf_words with some active leaves
    let leaf_words: Vec<AtomicU64> = (0..8)
        .map(|i| {
            if i % 2 == 1 {
                AtomicU64::new(0xFFFF) // Non-zero
            } else {
                AtomicU64::new(0)
            }
        })
        .collect();

    // Sync partition (should set status bit 1, NOT affect summary)
    let has_work = waker.sync_partition_summary(0, 8, &leaf_words);

    assert!(has_work, "should detect work");

    // Status bit 1 should be set
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        tasks_bit,
        "BUG REGRESSION: status bit 1 should be set after sync finds work"
    );

    // Summary for signal words should be UNCHANGED
    let final_summary = waker.snapshot_summary();
    assert_eq!(
        initial_summary, final_summary,
        "BUG REGRESSION: summary for signal words should not be affected by sync_partition_summary"
    );

    // Now clear all leaves
    for leaf_word in &leaf_words {
        leaf_word.store(0, Ordering::Relaxed);
    }

    // Sync again (should clear status bit 1)
    let has_work = waker.sync_partition_summary(0, 8, &leaf_words);
    assert!(!has_work, "should detect no work");

    // Status bit 1 should be cleared
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        !tasks_bit,
        "BUG REGRESSION: status bit 1 should be cleared after sync finds no work"
    );

    // Summary should STILL be unchanged
    let final_summary = waker.snapshot_summary();
    assert_eq!(
        initial_summary, final_summary,
        "BUG REGRESSION: summary should never be affected by partition operations"
    );
}

// ============================================================================
// REGRESSION TEST: Clear Partition Leaf Updates Status Correctly
// ============================================================================

#[test]
fn regression_clear_partition_leaf_updates_status_not_summary() {
    let waker = SignalWaker::new();

    // Set summary bits for signal words
    for i in 0..20 {
        waker.mark_active(i);
    }
    let initial_summary = waker.snapshot_summary();

    // Mark some partition leaves active
    waker.mark_partition_leaf_active(0);
    waker.mark_partition_leaf_active(1);
    waker.mark_partition_leaf_active(2);

    // Status bit 1 should be set
    let (_, tasks_bit) = waker.status_bits();
    assert!(tasks_bit, "tasks bit should be set");

    // Clear leaves one by one (should not clear status yet)
    waker.clear_partition_leaf(0);
    let (_, tasks_bit) = waker.status_bits();
    assert!(tasks_bit, "tasks bit should remain set");

    waker.clear_partition_leaf(1);
    let (_, tasks_bit) = waker.status_bits();
    assert!(tasks_bit, "tasks bit should remain set");

    // Clear last leaf (should trigger try_unmark_tasks)
    waker.clear_partition_leaf(2);
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        !tasks_bit,
        "BUG REGRESSION: tasks bit should be cleared after last leaf cleared"
    );

    // Summary should be UNCHANGED throughout
    let final_summary = waker.snapshot_summary();
    assert_eq!(
        initial_summary, final_summary,
        "BUG REGRESSION: summary should not be affected by partition leaf operations"
    );
}

// ============================================================================
// REGRESSION TEST: Mark Partition Leaf Active Updates Status Correctly
// ============================================================================

#[test]
fn regression_mark_partition_leaf_active_updates_status_not_summary() {
    let waker = SignalWaker::new();

    // Set summary bits
    for i in 0..30 {
        waker.mark_active(i);
    }
    let initial_summary = waker.snapshot_summary();

    // Partition starts empty
    let (_, tasks_bit) = waker.status_bits();
    assert!(!tasks_bit, "tasks bit should start clear");

    // Mark first leaf active (0 → non-zero transition)
    let was_empty = waker.mark_partition_leaf_active(5);
    assert!(was_empty, "partition should have been empty");

    // Status bit 1 should now be set
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        tasks_bit,
        "BUG REGRESSION: tasks bit should be set on 0→non-zero transition"
    );

    // Summary should be UNCHANGED
    let final_summary = waker.snapshot_summary();
    assert_eq!(
        initial_summary, final_summary,
        "BUG REGRESSION: summary should not be affected by mark_partition_leaf_active"
    );

    // Verify partition_summary is correctly updated (separate from summary)
    let partition_summary = waker.partition_summary();
    assert_eq!(
        partition_summary,
        1 << 5,
        "partition_summary should have bit 5 set"
    );
}

// ============================================================================
// STRESS TEST: Rapid Status Bit Cycling
// ============================================================================

#[test]
fn regression_rapid_status_cycling_preserves_summary() {
    let waker = SignalWaker::new();

    // Establish a stable summary state
    for i in 0..64 {
        if i % 3 == 0 {
            waker.mark_active(i);
        }
    }
    let expected_summary = waker.snapshot_summary();

    // Rapidly cycle status bits
    for _ in 0..1000 {
        waker.mark_yield();
        waker.mark_tasks();
        waker.try_unmark_yield();
        waker.try_unmark_tasks();
    }

    // Summary should be completely unchanged
    let final_summary = waker.snapshot_summary();
    assert_eq!(
        final_summary, expected_summary,
        "BUG REGRESSION: rapid status cycling corrupted summary"
    );

    // Status bits should be clear
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(!yield_bit, "yield bit should be clear");
    assert!(!tasks_bit, "tasks bit should be clear");
}

// ============================================================================
// STRESS TEST: Concurrent Status and Summary Operations
// ============================================================================

#[test]
fn regression_concurrent_status_summary_operations() {
    use std::sync::Arc;
    use std::thread;

    let waker = Arc::new(SignalWaker::new());

    // Thread 1: Manipulate summary bits
    let waker1 = waker.clone();
    let handle1 = thread::spawn(move || {
        for i in 0..100 {
            waker1.mark_active(i % 64);
            if i % 10 == 0 {
                waker1.try_unmark(i % 64);
            }
        }
    });

    // Thread 2: Manipulate status bits
    let waker2 = waker.clone();
    let handle2 = thread::spawn(move || {
        for _ in 0..100 {
            waker2.mark_yield();
            waker2.mark_tasks();
            waker2.try_unmark_yield();
            waker2.try_unmark_tasks();
        }
    });

    // Thread 3: Manipulate partition summary
    let waker3 = waker.clone();
    let handle3 = thread::spawn(move || {
        for i in 0..50 {
            waker3.mark_partition_leaf_active(i % 64);
        }
        for i in 0..50 {
            waker3.clear_partition_leaf(i % 64);
        }
    });

    handle1.join().unwrap();
    handle2.join().unwrap();
    handle3.join().unwrap();

    // No crashes or data races = success
    // Both fields should be readable
    let summary = waker.snapshot_summary();
    let (yield_bit, tasks_bit) = waker.status_bits();

    println!(
        "After concurrent operations: summary={:016x}, yield={}, tasks={}",
        summary, yield_bit, tasks_bit
    );
}

// ============================================================================
// EDGE CASE: Status Bit TOCTOU Race
// ============================================================================

#[test]
fn regression_status_bit_toctou_is_safe() {
    // Tests that the check-then-clear pattern in try_unmark_* is safe
    // Even if the bit is set again between check and clear, it's acceptable
    // because worst case is a spurious wakeup

    let waker = SignalWaker::new();

    // Set tasks bit
    waker.mark_tasks();

    // Simulate race: bit gets cleared, then set again before try_unmark
    waker.try_unmark_tasks();
    waker.mark_tasks(); // Race: bit set again

    // This is fine - the bit should be set
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        tasks_bit,
        "tasks bit should be set (was set after try_unmark check)"
    );

    // The important thing is that try_unmark doesn't corrupt other state
    let summary = waker.snapshot_summary();
    assert_eq!(summary, 0, "summary should be empty");
}

// ============================================================================
// VALIDATION: Status and Summary Are Truly Independent
// ============================================================================

#[test]
fn regression_status_and_summary_full_independence() {
    let waker = SignalWaker::new();

    // Enumerate all possible status states
    let status_states = vec![
        (false, false), // Neither bit set
        (true, false),  // Yield only
        (false, true),  // Tasks only
        (true, true),   // Both bits set
    ];

    // Enumerate some summary states
    let summary_states = vec![0u64, 0b1, 0b11, 0xFF, u64::MAX];

    for &(should_have_yield, should_have_tasks) in &status_states {
        for &summary_mask in &summary_states {
            // Reset waker
            let waker = SignalWaker::new();

            // Set summary state
            for i in 0..64 {
                if (summary_mask >> i) & 1 != 0 {
                    waker.mark_active(i);
                }
            }

            // Set status state
            if should_have_yield {
                waker.mark_yield();
            }
            if should_have_tasks {
                waker.mark_tasks();
            }

            // Verify summary matches expected
            let summary = waker.snapshot_summary();
            assert_eq!(
                summary, summary_mask,
                "summary should match expected: expected={:016x}, got={:016x}",
                summary_mask, summary
            );

            // Verify status matches expected
            let (yield_bit, tasks_bit) = waker.status_bits();
            assert_eq!(
                yield_bit, should_have_yield,
                "yield bit mismatch for summary={:016x}",
                summary_mask
            );
            assert_eq!(
                tasks_bit, should_have_tasks,
                "tasks bit mismatch for summary={:016x}",
                summary_mask
            );

            // Now clear status bits
            waker.try_unmark_yield();
            waker.try_unmark_tasks();

            // Summary should be unchanged
            let summary_after = waker.snapshot_summary();
            assert_eq!(
                summary_after, summary_mask,
                "BUG REGRESSION: summary changed after clearing status bits"
            );

            // Status should be clear
            let (yield_bit, tasks_bit) = waker.status_bits();
            assert!(!yield_bit, "yield bit should be clear");
            assert!(!tasks_bit, "tasks bit should be clear");
        }
    }
}
