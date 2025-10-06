//! Comprehensive tests for SignalWaker functionality
//!
//! These tests validate all aspects of the SignalWaker including:
//! - Producer-side API (mark_active, mark_active_mask)
//! - Consumer-side API (snapshot_summary, summary_select, try_unmark_if_empty)
//! - Permit management (try_acquire, acquire, acquire_timeout, release)
//! - Thread safety and concurrency
//! - Edge cases and error conditions

use bop_mpmc::SignalWaker;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

/// Test basic waker creation and initial state
#[test]
fn test_waker_creation() {
    let waker = SignalWaker::new();

    // Initially all should be zero
    assert_eq!(waker.summary_bits(), 0);
    assert_eq!(waker.permits(), 0);
    assert_eq!(waker.sleepers(), 0);

    // Default construction should also work
    let default_waker = SignalWaker::default();
    assert_eq!(default_waker.summary_bits(), 0);
    assert_eq!(default_waker.permits(), 0);
    assert_eq!(default_waker.sleepers(), 0);
}

/// Test marking single bits active
#[test]
fn test_mark_single_active() {
    let waker = SignalWaker::new();

    // Mark bit 0 as active
    waker.mark_active(0);
    assert_eq!(waker.summary_bits(), 1);
    assert_eq!(waker.permits(), 1);

    // Mark bit 10 as active
    waker.mark_active(10);
    assert_eq!(waker.summary_bits(), 1 | (1 << 10));
    assert_eq!(waker.permits(), 2);

    // Mark the same bit again - should not increase permits
    waker.mark_active(0);
    assert_eq!(waker.summary_bits(), 1 | (1 << 10));
    assert_eq!(waker.permits(), 2); // Still 2 permits
}

/// Test marking multiple bits with mask
#[test]
fn test_mark_mask_active() {
    let waker = SignalWaker::new();

    // Mark multiple bits at once
    let mask = (1 << 5) | (1 << 15) | (1 << 25);
    waker.mark_active_mask(mask);
    assert_eq!(waker.summary_bits(), mask);
    assert_eq!(waker.permits(), 3);

    // Mark overlapping bits - should only add permits for new bits
    let overlapping_mask = (1 << 15) | (1 << 35);
    waker.mark_active_mask(overlapping_mask);
    let expected_summary = mask | (1 << 35);
    assert_eq!(waker.summary_bits(), expected_summary);
    assert_eq!(waker.permits(), 4); // Only bit 35 was new
}

/// Test empty mask handling
#[test]
fn test_empty_mask() {
    let waker = SignalWaker::new();

    // Empty mask should do nothing
    waker.mark_active_mask(0);
    assert_eq!(waker.summary_bits(), 0);
    assert_eq!(waker.permits(), 0);
}

/// Test snapshot functionality
#[test]
fn test_snapshot_summary() {
    let waker = SignalWaker::new();

    // Initially empty
    assert_eq!(waker.snapshot_summary(), 0);

    // Mark some bits
    waker.mark_active(5);
    waker.mark_active(10);

    let snapshot = waker.snapshot_summary();
    assert_eq!(snapshot, (1 << 5) | (1 << 10));
}

/// Test summary_select functionality
#[test]
fn test_summary_select() {
    let waker = SignalWaker::new();

    // Mark bits at various positions
    waker.mark_active(3);
    waker.mark_active(15);
    waker.mark_active(25);

    // Test selecting nearest to different indices
    assert_eq!(waker.summary_select(0), 3);
    assert_eq!(waker.summary_select(5), 3); // Closer to 3 than 15
    assert_eq!(waker.summary_select(12), 15); // Closer to 15
    assert_eq!(waker.summary_select(20), 25); // Closer to 25
    assert_eq!(waker.summary_select(23), 25); // Closer to 25
    assert_eq!(waker.summary_select(30), 25); // Closer to 25

    // When there are no bits, should return 64 (out of bounds)
    let empty_waker = SignalWaker::new();
    assert_eq!(empty_waker.summary_select(10), 64);
}

/// Test try_acquire functionality
#[test]
fn test_try_acquire() {
    let waker = SignalWaker::new();

    // No permits initially
    assert!(!waker.try_acquire());

    // Add permits
    waker.release(2);

    // Should be able to acquire permits
    assert!(waker.try_acquire());
    assert!(waker.try_acquire());
    assert!(!waker.try_acquire()); // No permits left

    // Check permit count
    assert_eq!(waker.permits(), 0);
}

/// Test blocking acquire functionality
#[test]
fn test_blocking_acquire() {
    let waker = Arc::new(SignalWaker::new());

    // Test acquiring with existing permits
    waker.release(1);
    waker.acquire(); // Should return immediately

    // Test blocking until permit is available
    let waker_clone = waker.clone();
    let handle = thread::spawn(move || {
        // This should block until we release a permit
        waker_clone.acquire();
        0 as usize // Return value to match other threads
    });

    // Give the thread time to block
    thread::sleep(Duration::from_millis(10));
    assert_eq!(waker.sleepers(), 1);

    // Release a permit - this should wake up the thread
    waker.release(1);
    handle.join().expect("Thread should complete");

    // Sleeper count should be back to 0
    assert_eq!(waker.sleepers(), 0);
}

/// Test acquire with timeout
#[test]
fn test_acquire_timeout() {
    let waker = SignalWaker::new();

    // Test acquiring with existing permits
    waker.release(1);
    assert!(waker.acquire_timeout(Duration::from_millis(100)));

    // Test timeout when no permits available
    assert!(!waker.acquire_timeout(Duration::from_millis(50)));

    // Test that it succeeds when permit becomes available during wait
    let waker = Arc::new(SignalWaker::new());
    let waker_clone = waker.clone();

    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(25));
        waker_clone.release(1);
    });

    // This should succeed after the other thread releases a permit
    let start = Instant::now();
    let result = waker.acquire_timeout(Duration::from_millis(100));
    let elapsed = start.elapsed();

    assert!(result);
    assert!(elapsed >= Duration::from_millis(20)); // Should have waited
    assert!(elapsed < Duration::from_millis(100)); // But not timed out

    handle.join().expect("Thread should complete");
}

/// Test release functionality with sleepers
#[test]
fn test_release_with_sleepers() {
    let waker = Arc::new(SignalWaker::new());

    // Start multiple sleeping threads
    let mut handles = vec![];
    for _i in 0..3 {
        let waker_clone = waker.clone();
        let handle = thread::spawn(move || {
            waker_clone.acquire();
            0 as usize // Return value
        });
        handles.push(handle);
    }

    // Give threads time to start sleeping
    thread::sleep(Duration::from_millis(10));
    assert_eq!(waker.sleepers(), 3);

    // Release fewer permits than sleepers - only some should wake up
    waker.release(2);
    thread::sleep(Duration::from_millis(10));
    assert_eq!(waker.sleepers(), 1); // One still sleeping

    // Release permit for the last sleeper
    waker.release(1);

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    // Verify all threads completed
    assert_eq!(waker.sleepers(), 0);
}

/// Test try_unmark_if_empty functionality
#[test]
fn test_try_unmark_if_empty() {
    let waker = SignalWaker::new();
    let signal = AtomicU64::new(0);

    // Mark bit as active
    waker.mark_active(5);
    assert_eq!(waker.summary_bits(), 1 << 5);

    // Try to unmark with empty signal - should clear the bit
    waker.try_unmark_if_empty(5, &signal);
    assert_eq!(waker.summary_bits(), 0);

    // Mark bit again
    waker.mark_active(5);
    assert_eq!(waker.summary_bits(), 1 << 5);

    // Set signal to non-zero value
    signal.store(1 << 10, Ordering::Relaxed);

    // Try to unmark - should not clear the bit since signal is not empty
    waker.try_unmark_if_empty(5, &signal);
    assert_eq!(waker.summary_bits(), 1 << 5);

    // Clear signal and try again
    signal.store(0, Ordering::Relaxed);
    waker.try_unmark_if_empty(5, &signal);
    assert_eq!(waker.summary_bits(), 0);
}

/// Test concurrent producer and consumer operations
#[test]
fn test_concurrent_producer_consumer() {
    let waker = Arc::new(SignalWaker::new());
    let barrier = Arc::new(Barrier::new(4)); // 1 producer + 3 consumers

    let mut handles = vec![];

    // Producer thread
    let producer_waker = waker.clone();
    let producer_barrier = barrier.clone();
    let producer_handle = thread::spawn(move || {
        producer_barrier.wait();
        for i in 0..10 {
            producer_waker.mark_active(i as u64 % 64);
            std::hint::spin_loop(); // Small delay
        }
        0 as usize // Return value to match consumers
    });
    handles.push(producer_handle);

    // Consumer threads
    for _consumer_id in 0..3 {
        let consumer_waker = waker.clone();
        let consumer_barrier = barrier.clone();
        let handle = thread::spawn(move || {
            consumer_barrier.wait();
            let mut acquired = 0;
            while acquired < 3 {
                if consumer_waker.try_acquire() {
                    acquired += 1;
                } else {
                    // Try to wait briefly
                    consumer_waker.acquire_timeout(Duration::from_millis(10));
                }
            }
            acquired
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    let total_acquired: usize = handles
        .into_iter()
        .map(|h| h.join().expect("Thread should complete"))
        .sum();

    // Should have acquired at least the 10 permits we released
    assert!(total_acquired >= 10);
}

/// Test stress scenario with many threads
#[test]
fn test_stress_many_threads() {
    const NUM_THREADS: usize = 10;
    const OPERATIONS_PER_THREAD: usize = 100;

    let waker = Arc::new(SignalWaker::new());
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let total_permit_counter = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    for thread_id in 0..NUM_THREADS {
        let waker_clone = waker.clone();
        let barrier_clone = barrier.clone();
        let counter_clone = total_permit_counter.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait();

            for _ in 0..OPERATIONS_PER_THREAD {
                if thread_id % 2 == 0 {
                    // Even threads are producers
                    waker_clone.mark_active(((thread_id as u64) * 7) % 64);
                } else {
                    // Odd threads are consumers
                    if waker_clone.try_acquire() {
                        counter_clone.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // If no permit available, try to wait briefly
                        if waker_clone.acquire_timeout(Duration::from_millis(1)) {
                            counter_clone.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                // Small yield to allow other threads
                std::hint::spin_loop();
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    println!(
        "Total permits acquired: {}",
        total_permit_counter.load(Ordering::Relaxed)
    );
}

/// Test edge cases at boundary conditions
#[test]
fn test_boundary_conditions() {
    let waker = SignalWaker::new();

    // Test bit 63 (highest valid bit)
    waker.mark_active(63);
    assert_eq!(waker.summary_bits(), 1 << 63);
    assert_eq!(waker.permits(), 1);

    // Test bit 0 (lowest valid bit)
    waker.mark_active(0);
    assert_eq!(waker.summary_bits(), (1 << 63) | 1);
    assert_eq!(waker.permits(), 2);

    // Test edge case for summary_select near boundaries
    assert_eq!(waker.summary_select(0), 0); // Nearest to 0 is 0
    assert_eq!(waker.summary_select(62), 63); // Nearest to 62 is 63

    // Test with large release values
    waker.release(1000);
    assert_eq!(waker.permits(), 1002); // 1000 + 2 from marks

    // Consume many permits
    for _ in 0..1002 {
        assert!(waker.try_acquire());
    }
    assert!(!waker.try_acquire()); // Should be empty now
}

/// Test that empty zero release is handled correctly
#[test]
fn test_zero_release() {
    let waker = SignalWaker::new();

    waker.release(0);
    assert_eq!(waker.permits(), 0);
    assert_eq!(waker.summary_bits(), 0);
    assert_eq!(waker.sleepers(), 0);
}

/// Test that the waker works correctly under high contention
#[test]
fn test_high_contention() {
    const NUM_PRODUCERS: usize = 8;
    const NUM_CONSUMERS: usize = 4;
    const OPERATIONS_PER_PRODUCER: usize = 50;

    let waker = Arc::new(SignalWaker::new());
    let start_barrier = Arc::new(Barrier::new(NUM_PRODUCERS + NUM_CONSUMERS));

    let mut handles = vec![];

    // Producer threads
    for producer_id in 0..NUM_PRODUCERS {
        let waker_clone = waker.clone();
        let barrier_clone = start_barrier.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait();

            for i in 0..OPERATIONS_PER_PRODUCER {
                // Vary the bits to mark to create interesting patterns
                let bit = ((producer_id as u64) * 10 + i as u64) % 64;
                waker_clone.mark_active(bit);

                // Small random delay
                if i % 10 == 0 {
                    thread::sleep(Duration::from_micros(1));
                }
            }
        });

        handles.push(handle);
    }

    // Consumer threads
    let consumed_count = Arc::new(AtomicUsize::new(0));
    for _consumer_id in 0..NUM_CONSUMERS {
        let waker_clone = waker.clone();
        let barrier_clone = start_barrier.clone();
        let count_clone = consumed_count.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait();

            loop {
                if waker_clone.try_acquire() {
                    count_clone.fetch_add(1, Ordering::Relaxed);
                } else {
                    // Try to wait for more permits
                    if waker_clone.acquire_timeout(Duration::from_millis(100)) {
                        count_clone.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // Timeout - check if we're done
                        // (This is a simple termination condition for the test)
                        break;
                    }
                }
            }
        });

        handles.push(handle);
    }

    // Wait for producers to complete
    for handle in handles.drain(..NUM_PRODUCERS) {
        handle.join().expect("Producer should complete");
    }

    // Let consumers finish
    thread::sleep(Duration::from_millis(50));

    // Wake up any remaining consumers
    waker.release(100);
    thread::sleep(Duration::from_millis(50));

    // Wait for all consumers to complete
    for handle in handles {
        handle.join().expect("Consumer should complete");
    }

    let consumed = consumed_count.load(Ordering::Relaxed);
    let expected_produced = NUM_PRODUCERS * OPERATIONS_PER_PRODUCER;

    println!("Consumed: {}, Expected: {}", consumed, expected_produced);
    // Should have consumed approximately what was produced (allowing for some test timing issues)
    assert!(consumed >= expected_produced * 80 / 100); // At least 80% consumed
}

/// Test persistent functionality under repeated operations
#[test]
fn test_persistent_operations() {
    let waker = SignalWaker::new();

    // Perform many cycles of operations
    for cycle in 0..100 {
        // Mark several bits
        waker.mark_active(cycle % 64);
        if cycle % 3 == 0 {
            waker.mark_active((cycle + 1) % 64);
        }

        // Acquire permits
        let acquired = waker.try_acquire();
        if acquired {
            assert!(waker.permits() < 100); // Reasonable bound
        }

        // Occasionally do a blocking acquire with timeout
        if cycle % 10 == 0 {
            waker.acquire_timeout(Duration::from_micros(1));
        }

        // Release some permits periodically
        if cycle % 15 == 0 {
            waker.release(2);
        }
    }

    // Final state should be reasonable
    println!(
        "Final state - permits: {}, sleepers: {}, summary: {:#x}",
        waker.permits(),
        waker.sleepers(),
        waker.summary_bits()
    );
}

/// Test interaction with actual signal atomics
#[test]
fn test_with_signal_atomic() {
    let waker = SignalWaker::new();
    let signals: Vec<AtomicU64> = (0..64).map(|_| AtomicU64::new(0)).collect();

    // Mark some signals as active
    for &i in &[5u64, 15, 25, 35] {
        waker.mark_active(i);
        signals[i as usize].store(1 << i, Ordering::Relaxed);
    }

    // Acquire and process signals
    while waker.try_acquire() {
        let summary = waker.snapshot_summary();

        for bit_index in 0u64..64 {
            if (summary >> bit_index) & 1 != 0 {
                // Simulate processing the signal
                let prev =
                    signals[bit_index as usize].fetch_and(!(1 << bit_index), Ordering::Relaxed);

                // Try to unmark if now empty
                waker.try_unmark_if_empty(bit_index, &signals[bit_index as usize]);

                assert!(
                    (prev & (1 << bit_index)) != 0,
                    "Signal should have been set"
                );
            }
        }
    }

    // All signals should be cleared now
    for signal in &signals {
        assert_eq!(signal.load(Ordering::Relaxed), 0);
    }
}

/// Test signal waker behavior under extreme load
#[test]
fn test_extreme_load() {
    let waker = Arc::new(SignalWaker::new());
    let num_threads = 16;
    let operations_per_thread = 1000;
    let barrier = Arc::new(Barrier::new(num_threads));

    let completed_ops = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let waker_clone = waker.clone();
        let barrier_clone = barrier.clone();
        let completed_clone = completed_ops.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait();

            for i in 0..operations_per_thread {
                if thread_id % 4 == 0 {
                    // 25% of threads are producers
                    let bit = (thread_id as u64 * 1000 + i as u64) % 64;
                    waker_clone.mark_active(bit);
                } else {
                    // 75% are consumers
                    if waker_clone.try_acquire() {
                        completed_clone.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // Brief timeout to avoid blocking the test
                        waker_clone.acquire_timeout(Duration::from_micros(10));
                    }
                }

                // Occasional yield
                if i % 100 == 0 {
                    std::hint::spin_loop();
                }
            }

            thread_id
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    println!(
        "Extreme load test completed ops: {}",
        completed_ops.load(Ordering::Relaxed)
    );
}

/// Test that summary_select handles all possible cases correctly
#[test]
fn test_summary_select_edge_cases() {
    let waker = SignalWaker::new();

    // Test with no bits set
    assert_eq!(waker.summary_select(0), 64);
    assert_eq!(waker.summary_select(32), 64);
    assert_eq!(waker.summary_select(63), 64);

    // Test with single bit at various positions
    for bit in 0..64 {
        let waker = SignalWaker::new();
        waker.mark_active(bit);

        // Should always return that bit regardless of query position
        assert_eq!(waker.summary_select(bit), bit);

        // Test distances to other positions
        if bit > 0 {
            assert_eq!(waker.summary_select(bit - 1), bit);
        }
        if bit < 63 {
            assert_eq!(waker.summary_select(bit + 1), bit);
        }
    }

    // Test with multiple bits in specific patterns
    let waker = SignalWaker::new();
    waker.mark_active(0);
    waker.mark_active(31);
    waker.mark_active(63);

    // Test nearest selection logic
    assert_eq!(waker.summary_select(0), 0); // Exactly at 0
    assert_eq!(waker.summary_select(15), 0); // Closer to 0 than 31
    assert_eq!(waker.summary_select(16), 31); // Closer to 31 than 0
    assert_eq!(waker.summary_select(47), 31); // Closer to 31 than 63
    assert_eq!(waker.summary_select(48), 63); // Closer to 63 than 31
}

/// Test that permits are correctly tracked during complex scenarios
#[test]
fn test_permit_accounting() {
    let waker = SignalWaker::new();

    // Test basic permit counting
    assert_eq!(waker.permits(), 0);

    waker.release(5);
    assert_eq!(waker.permits(), 5);

    for _ in 0..3 {
        assert!(waker.try_acquire());
    }
    assert_eq!(waker.permits(), 2);

    // Test that marking active adds permits
    waker.mark_active(10);
    assert_eq!(waker.permits(), 3); // 2 remaining + 1 new

    // Test that marking same bit doesn't add permits
    waker.mark_active(10);
    assert_eq!(waker.permits(), 3);

    // Test batch marking
    waker.mark_active_mask((1 << 5) | (1 << 15));
    assert_eq!(waker.permits(), 5); // 3 + 2 new permits
}

/// Test timeout behavior with various edge cases
#[test]
fn test_timeout_edge_cases() {
    let waker = SignalWaker::new();

    // Test zero timeout
    assert!(!waker.acquire_timeout(Duration::from_nanos(0)));

    // Test very short timeout
    assert!(!waker.acquire_timeout(Duration::from_nanos(1)));

    // Test that existing permits are consumed even with timeout
    waker.release(1);
    assert!(waker.acquire_timeout(Duration::from_secs(1)));

    // Test timeout after consuming permit
    assert!(!waker.acquire_timeout(Duration::from_millis(10)));

    // Test that permit arriving during wait is detected
    let waker = Arc::new(SignalWaker::new());
    let waker_clone = waker.clone();

    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        waker_clone.release(1);
    });

    let start = Instant::now();
    let result = waker.acquire_timeout(Duration::from_millis(200));
    let elapsed = start.elapsed();

    assert!(result);
    assert!(elapsed >= Duration::from_millis(45)); // Should have waited
    assert!(elapsed < Duration::from_millis(150)); // But not full timeout

    handle.join().expect("Thread should complete");
}

/// Test memory ordering properties
#[test]
fn test_memory_ordering() {
    use std::sync::atomic::AtomicU64 as AtomicU64Test;

    let waker = Arc::new(SignalWaker::new());
    let flag = Arc::new(AtomicU64Test::new(0));

    let producer_thread = {
        let waker_clone = waker.clone();
        let flag_clone = flag.clone();

        thread::spawn(move || {
            // Set the flag first
            flag_clone.store(1, Ordering::Release);

            // Then mark the signal active
            waker_clone.mark_active(0);
        })
    };

    let consumer_thread = {
        let waker_clone = waker.clone();
        let flag_clone = flag.clone();

        thread::spawn(move || {
            // Wait for the signal
            waker_clone.acquire();

            // Should see the flag set due to release/acquire semantics
            assert_eq!(flag_clone.load(Ordering::Acquire), 1);
        })
    };

    producer_thread.join().expect("Producer should complete");
    consumer_thread.join().expect("Consumer should complete");
}

/// Test graceful shutdown scenarios
#[test]
fn test_graceful_shutdown() {
    let waker = Arc::new(SignalWaker::new());
    let shutdown = Arc::new(AtomicU64::new(0));
    let num_threads = 5;

    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let waker_clone = waker.clone();
        let shutdown_clone = shutdown.clone();

        let handle = thread::spawn(move || {
            loop {
                // Check for shutdown signal
                if shutdown_clone.load(Ordering::Relaxed) != 0 {
                    break;
                }

                // Try to acquire with timeout
                if waker_clone.try_acquire() {
                    // Process the signal
                } else {
                    // Wait briefly then check shutdown again
                    waker_clone.acquire_timeout(Duration::from_millis(100));
                }
            }

            thread_id
        });

        handles.push(handle);
    }

    // Let threads run briefly
    thread::sleep(Duration::from_millis(100));

    // Signal shutdown
    shutdown.store(1, Ordering::Relaxed);

    // Wake up any sleeping threads
    waker.release(num_threads as usize);

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete");
    }
}
