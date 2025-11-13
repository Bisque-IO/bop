/// Comprehensive tests for the signaling mechanism, focusing on SignalWaker
/// status bit correctness after bug fixes.
///
/// This test suite validates:
/// 1. SignalWaker status bit lifecycle
/// 2. Status vs summary field independence
/// 3. Partition summary management
/// 4. Bug regression prevention
use maniac_runtime::runtime::waker::{STATUS_SUMMARY_BITS, STATUS_SUMMARY_MASK, WorkerWaker};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

// ============================================================================
// UNIT TESTS: SignalWaker Status Bits
// ============================================================================

#[test]
fn test_signal_waker_status_bit_yield_lifecycle() {
    let waker = WorkerWaker::new();

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
    assert!(
        !yield_bit,
        "yield bit should be cleared after try_unmark_yield"
    );
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
    let waker = WorkerWaker::new();

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
    assert!(
        !tasks_bit,
        "tasks bit should be cleared after try_unmark_tasks"
    );

    // Permit should remain
    assert_eq!(
        waker.permits(),
        1,
        "permit should remain after try_unmark_tasks"
    );
}

#[test]
fn test_signal_waker_status_bits_idempotent() {
    let waker = WorkerWaker::new();

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
    let waker = WorkerWaker::new();

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
fn test_signal_waker_mark_active_mask_semantics() {
    let waker = WorkerWaker::new();

    let mask = (1 << 1) | (1 << 3) | (1 << 5);
    waker.mark_active_mask(mask);
    assert_eq!(
        waker.permits(),
        3,
        "mark_active_mask should release one permit per new bit"
    );
    assert_eq!(waker.snapshot_summary(), mask);

    // Reapplying the same mask should be a no-op for permits.
    waker.mark_active_mask(mask);
    assert_eq!(
        waker.permits(),
        3,
        "re-marking existing bits must not change permits"
    );

    // Add a new bit plus an existing one; only the new bit should count.
    waker.mark_active_mask((1 << 2) | (1 << 5));
    assert_eq!(
        waker.permits(),
        4,
        "only newly-set bits should contribute another permit"
    );
    assert_eq!(
        waker.snapshot_summary(),
        mask | (1 << 2),
        "summary should contain all marked bits exactly once"
    );
}

#[test]
fn test_signal_waker_mark_active_permit_semantics() {
    let waker = WorkerWaker::new();
    assert_eq!(waker.permits(), 0);

    waker.mark_active(5);
    assert_eq!(
        waker.permits(),
        1,
        "first 0→1 transition adds exactly one permit"
    );
    assert_eq!(waker.snapshot_summary(), 1 << 5);

    waker.mark_active(5);
    assert_eq!(
        waker.permits(),
        1,
        "re-marking the same word must not increment permits"
    );

    waker.mark_active(6);
    assert_eq!(
        waker.permits(),
        2,
        "activating a new word increments permits once"
    );
    assert_eq!(waker.snapshot_summary(), (1 << 5) | (1 << 6));

    assert_eq!(
        waker.snapshot_summary() & !STATUS_SUMMARY_MASK,
        0,
        "summary must stay confined to queue bits"
    );
}

#[test]
fn test_signal_waker_permits_track_acquisition() {
    let waker = WorkerWaker::new();

    waker.mark_active(1);
    waker.mark_active(7);
    waker.mark_active(12);

    assert_eq!(waker.permits(), 3);
    assert!(waker.try_acquire());
    assert!(waker.try_acquire());
    assert!(waker.try_acquire());
    assert!(
        !waker.try_acquire(),
        "permits should be exhausted after three acquisitions"
    );
    assert_eq!(waker.permits(), 0);
}

#[test]
fn test_signal_waker_sleepers_balance_after_timeout() {
    use std::thread;

    let waker = Arc::new(WorkerWaker::new());
    let clone = Arc::clone(&waker);

    let handle = thread::spawn(move || {
        let acquired = clone.acquire_timeout(Duration::from_millis(10));
        assert!(
            !acquired,
            "acquire_timeout should return false when no permits ever arrive"
        );
    });

    handle.join().expect("acquire_timeout thread panicked");
    assert_eq!(
        waker.sleepers(),
        0,
        "sleepers counter should return to zero after timeout path"
    );
}

#[test]
fn stress_signal_waker_partition_sync_race() {
    use rand::{Rng, SeedableRng, rngs::SmallRng};

    let waker = Arc::new(WorkerWaker::new());
    let leaves: Arc<Vec<AtomicU64>> = Arc::new((0..8).map(|_| AtomicU64::new(0)).collect());
    let barrier = Arc::new(Barrier::new(3));
    const ITERATIONS: usize = 1_000;

    let toggler = {
        let waker = Arc::clone(&waker);
        let leaves = Arc::clone(&leaves);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            let mut rng = SmallRng::seed_from_u64(0xfeed_beef_u64);
            for _ in 0..ITERATIONS {
                let leaf_idx = rng.random_range(0..leaves.len());
                let bit = rng.random_range(0..64);
                let mask = 1u64 << bit;
                let leaf = &leaves[leaf_idx];

                let prev = leaf.fetch_xor(mask, Ordering::AcqRel);
                let now = prev ^ mask;
                if now != 0 {
                    waker.mark_partition_leaf_active(leaf_idx);
                } else {
                    waker.clear_partition_leaf(leaf_idx);
                }
            }
        })
    };

    let synchronizer = {
        let waker = Arc::clone(&waker);
        let leaves = Arc::clone(&leaves);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            for _ in 0..ITERATIONS {
                let slice: &[AtomicU64] = &leaves;
                waker.sync_partition_summary(0, slice.len(), slice);
            }
        })
    };

    let summary_cleaner = {
        let waker = Arc::clone(&waker);
        let leaves = Arc::clone(&leaves);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            for _ in 0..ITERATIONS {
                let slice: &[AtomicU64] = &leaves;
                waker.sync_partition_summary(0, slice.len(), slice);
            }
        })
    };

    toggler.join().expect("toggler thread panicked");
    synchronizer.join().expect("sync thread panicked");
    summary_cleaner.join().expect("summary thread panicked");

    let slice: &[AtomicU64] = &leaves;
    waker.sync_partition_summary(0, slice.len(), slice);

    let actual_mask: u64 = slice
        .iter()
        .enumerate()
        .filter_map(|(idx, leaf)| {
            if leaf.load(Ordering::Acquire) != 0 {
                Some(1u64 << idx)
            } else {
                None
            }
        })
        .fold(0, |acc, bit| acc | bit);

    let observed = waker.partition_summary();
    assert_eq!(
        observed & actual_mask,
        actual_mask,
        "partition summary lost bits"
    );
    let (_, tasks_bit) = waker.status_bits();
    assert_eq!(
        tasks_bit,
        actual_mask != 0,
        "tasks bit should mirror partition non-emptiness"
    );
}

#[test]
fn stress_signal_waker_summary_race() {
    use rand::{Rng, SeedableRng, rngs::SmallRng};

    let waker = Arc::new(WorkerWaker::new());
    let signals: Arc<Vec<AtomicU64>> = Arc::new((0..4).map(|_| AtomicU64::new(0)).collect());
    let barrier = Arc::new(Barrier::new(3));
    const ITERATIONS: usize = 1_000;

    let producer = {
        let waker = Arc::clone(&waker);
        let signals = Arc::clone(&signals);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            let mut rng = SmallRng::seed_from_u64(0x1234_5678_u64);
            for _ in 0..ITERATIONS {
                let idx = rng.random_range(0..signals.len());
                let bit = rng.random_range(0..64);
                let mask = 1u64 << bit;
                let signal = &signals[idx];
                let prev = signal.fetch_or(mask, Ordering::AcqRel);
                if prev == 0 {
                    waker.mark_active(idx as u64);
                }
                if rng.random_bool(0.3) {
                    let prev = signal.fetch_and(!mask, Ordering::AcqRel);
                    if prev & !mask == 0 {
                        waker.try_unmark_if_empty(idx as u64, signal);
                    }
                }
            }
        })
    };

    let cleaner = {
        let waker = Arc::clone(&waker);
        let signals = Arc::clone(&signals);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            for iter in 0..ITERATIONS {
                let idx = iter % signals.len();
                waker.try_unmark_if_empty(idx as u64, &signals[idx]);
            }
        })
    };

    let sampler = {
        let waker = Arc::clone(&waker);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            for _ in 0..ITERATIONS {
                let _ = waker.snapshot_summary();
                let _ = waker.summary_select(0);
            }
        })
    };

    producer.join().expect("producer thread panicked");
    cleaner.join().expect("cleaner thread panicked");
    sampler.join().expect("sampler thread panicked");

    let summary = waker.snapshot_summary();
    for (idx, signal) in signals.iter().enumerate() {
        if signal.load(Ordering::Acquire) != 0 {
            assert!(
                summary & (1u64 << idx) != 0,
                "summary lost bit {} for non-empty signal",
                idx
            );
        }
    }
}

#[test]
fn test_signal_waker_try_unmark_if_empty_behaves() {
    let waker = WorkerWaker::new();
    waker.mark_active(11);
    let signal = AtomicU64::new(1 << 11);

    // Should be a no-op while the backing signal word is non-empty.
    waker.try_unmark_if_empty(11, &signal);
    assert_eq!(
        waker.snapshot_summary() & (1 << 11),
        1 << 11,
        "summary bit should remain while signal word is non-zero"
    );

    // Clearing the signal should allow the summary bit to fall away.
    signal.store(0, Ordering::Relaxed);
    waker.try_unmark_if_empty(11, &signal);
    assert_eq!(
        waker.snapshot_summary() & (1 << 11),
        0,
        "summary bit should clear once signal word is empty"
    );
}

#[test]
fn test_signal_waker_partition_summary_superset() {
    let waker = WorkerWaker::new();
    let leaves: Vec<AtomicU64> = (0..4).map(|_| AtomicU64::new(0)).collect();

    // Activate a single leaf and confirm partition + status engage.
    leaves[1].store(0b1, Ordering::Relaxed);
    let has_work = waker.sync_partition_summary(0, 4, &leaves);
    assert!(has_work);
    assert_eq!(waker.partition_summary(), 1 << 1);
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        tasks_bit,
        "tasks bit should raise on first non-empty partition"
    );

    // Add another leaf; summary should include both without dropping the original.
    leaves[2].store(0b1, Ordering::Relaxed);
    let has_work = waker.sync_partition_summary(0, 4, &leaves);
    assert!(has_work);
    assert_eq!(waker.partition_summary(), (1 << 1) | (1 << 2));

    // Clear leaf 1 while leaf 2 stays set; superset property must hold.
    leaves[1].store(0, Ordering::Relaxed);
    let has_work = waker.sync_partition_summary(0, 4, &leaves);
    assert!(has_work, "partition still has work via leaf 2");
    assert_eq!(waker.partition_summary(), 1 << 2);

    // Clear all leaves and ensure everything drops to zero.
    leaves
        .iter()
        .for_each(|leaf| leaf.store(0, Ordering::Relaxed));
    let has_work = waker.sync_partition_summary(0, 4, &leaves);
    assert!(!has_work);
    assert_eq!(waker.partition_summary(), 0);
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        !tasks_bit,
        "tasks bit must clear when partition summary becomes empty"
    );
}

#[test]
fn test_signal_waker_summary_vs_status_separation() {
    let waker = WorkerWaker::new();

    // Mark signal words active in summary
    waker.mark_active(0);
    waker.mark_active(1);
    waker.mark_active(2);

    let summary = waker.snapshot_summary();
    assert_ne!(summary & 0b111, 0, "summary bits 0-2 should be set");

    // Status bits should be independent
    let (yield_bit, tasks_bit) = waker.status_bits();
    assert!(
        !yield_bit,
        "yield status bit should be independent of summary"
    );
    assert!(
        !tasks_bit,
        "tasks status bit should be independent of summary"
    );

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
    let waker = WorkerWaker::new();

    // Initial state: empty partition
    assert_eq!(waker.partition_summary(), 0);
    let (_, tasks_bit) = waker.status_bits();
    assert!(!tasks_bit);

    // Mark first leaf active (0 → non-zero transition)
    let was_empty = waker.mark_partition_leaf_active(0);
    assert!(was_empty, "partition should have been empty");

    // Should trigger mark_tasks() → status bit set + permit added
    let (_, tasks_bit) = waker.status_bits();
    assert!(
        tasks_bit,
        "tasks bit should be set on 0→non-zero transition"
    );
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
    assert!(
        !tasks_bit,
        "tasks bit should be cleared on non-zero→0 transition"
    );
    assert_eq!(waker.partition_summary(), 0);
}

#[test]
fn test_signal_waker_sync_partition_summary() {
    let waker = WorkerWaker::new();

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
    let waker = WorkerWaker::new();
    let summary_bits = STATUS_SUMMARY_BITS as usize;

    // Simulate rapid transitions
    for i in 0..100 {
        let bit = i % summary_bits;
        waker.mark_partition_leaf_active(bit);
        if i % 10 == 0 {
            // Occasionally clear leaves to trigger 0→non-zero again
            for j in 0..summary_bits {
                waker.clear_partition_leaf(j);
            }
        }
    }

    // Should have accumulated some permits
    let permits = waker.permits();
    assert!(
        permits > 0,
        "should have accumulated permits from transitions"
    );
}

#[test]
fn test_try_unmark_when_already_clear() {
    let waker = WorkerWaker::new();

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
    let waker = Arc::new(WorkerWaker::new());
    let barrier = Arc::new(Barrier::new(3));

    let waker1 = waker.clone();
    let barrier1 = barrier.clone();
    let handle1 = std::thread::spawn(move || {
        barrier1.wait();
        for i in 0..100 {
            let bit = (i as u64) % (STATUS_SUMMARY_BITS as u64);
            waker1.mark_active(bit);
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
    let waker = Arc::new(WorkerWaker::new());
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
