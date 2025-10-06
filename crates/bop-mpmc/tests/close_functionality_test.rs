//! Test for the close functionality in SegSpsc
//! This test validates that the close bit encoding works correctly
//! and doesn't interfere with normal queue operations.

use bop_mpmc::seg_spsc::SegSpsc;
use bop_mpmc::{PopError, PushError};

const P: usize = 6; // 64 items/segment
const NUM_SEGS_P2: usize = 8; // 256 segments
type TestQueue = SegSpsc<u64, P, NUM_SEGS_P2>;

#[test]
fn test_close_functionality_basic() {
    let q = TestQueue::new();

    // Initially not closed
    assert!(!q.is_closed());

    // Can push when open
    q.try_push(42).unwrap();
    q.try_push(43).unwrap();

    // Close the queue
    q.close();
    assert!(q.is_closed());

    // Cannot push after closing
    assert!(matches!(q.try_push(44), Err(PushError::Closed(_))));

    // Can still pop existing items
    assert_eq!(q.try_pop(), Some(42));
    assert_eq!(q.try_pop(), Some(43));

    // When empty, try_pop should return None
    assert_eq!(q.try_pop(), None);

    // try_pop_n should return Closed error when queue is closed and empty
    let mut buf = [0u64];
    assert!(matches!(q.try_pop_n(&mut buf), Err(PopError::Closed)));
}

#[test]
fn test_close_with_batch_operations() {
    let q = TestQueue::new();

    // Fill up some items
    let data: Vec<u64> = (0..10).collect();
    assert_eq!(q.try_push_n(&data).unwrap(), 10);

    // Close the queue
    q.close();

    // Cannot push more
    let more_data: Vec<u64> = (10..15).collect();
    assert!(matches!(
        q.try_push_n(&more_data),
        Err(PushError::Closed(()))
    ));

    // Can still pop existing batch
    let mut out = vec![0u64; 10];
    assert_eq!(q.try_pop_n(&mut out).unwrap(), 10);
    assert_eq!(out[..10], data);

    // When empty, should return Closed
    assert!(matches!(q.try_pop_n(&mut out), Err(PopError::Closed)));
}

#[test]
fn test_close_during_concurrent_operations() {
    use std::sync::Arc;
    use std::thread;

    let q = Arc::new(TestQueue::new());
    let q_producer = Arc::clone(&q);
    let q_consumer = Arc::clone(&q);

    // Producer thread
    let producer = thread::spawn(move || {
        for i in 0..100 {
            if let Err(_) = q_producer.try_push(i) {
                break; // Queue was closed
            }
        }
    });

    // Consumer thread
    let consumer = thread::spawn(move || {
        let mut sum = 0;
        while sum < 50 {
            // Only consume half the items
            if let Some(item) = q_consumer.try_pop() {
                sum += item;
            } else {
                thread::yield_now();
            }
        }
        sum
    });

    // Let them run a bit
    thread::sleep(std::time::Duration::from_millis(1));

    // Close the queue
    q.close();

    // Wait for threads to finish
    producer.join().unwrap();
    let consumed_sum = consumer.join().unwrap();

    // Verify some items were consumed
    assert!(consumed_sum > 0);

    // Queue should be closed
    assert!(q.is_closed());
}

#[test]
fn test_close_bit_configuration() {
    let capacity = TestQueue::capacity();
    let expected_mask = capacity.next_power_of_two() as u64;

    // Verify the close mask is correctly positioned
    assert!(expected_mask > capacity as u64);

    // The close bit should be in a position that doesn't interfere
    // with the maximum queue position
    let max_position = capacity as u64;
    assert!(max_position & expected_mask == 0);
}

#[test]
fn test_zero_copy_consume_after_close() {
    let q = TestQueue::new();

    // Add some items
    for i in 0..10 {
        q.try_push(i).unwrap();
    }

    // Close the queue
    q.close();

    // Zero-copy consume should still work
    let mut consumed = Vec::new();
    let count = q.consume_in_place(20, |slice| {
        consumed.extend_from_slice(slice);
        slice.len()
    });

    assert_eq!(count, 10);
    assert_eq!(consumed, (0..10).collect::<Vec<_>>());

    // Queue should still be closed
    assert!(q.is_closed());
}
