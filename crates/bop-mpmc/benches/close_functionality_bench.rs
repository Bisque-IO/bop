//! Simple benchmark to test that the close functionality doesn't break normal operations
//! Uses the same parameters as the mpmc_benchmark to reproduce any segfaults

use bop_mpmc::mpmc::Mpmc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

/// Simple test with the same parameters as the failing benchmark
fn test_close_functionality_under_load() {
    const P: usize = 6;
    const NUM_SEGS_P2: usize = 14;
    const NUM_PRODUCERS: usize = 4;
    const ITEMS_PER_PRODUCER: usize = 100_000;

    println!(
        "Testing close functionality with P={}, NUM_SEGS_P2={}",
        P, NUM_SEGS_P2
    );

    let queue = Arc::new(Mpmc::<usize, P, NUM_SEGS_P2>::new());
    let items_pushed = Arc::new(AtomicUsize::new(0));
    let items_popped = Arc::new(AtomicUsize::new(0));

    // Test 1: Normal operation without close
    println!("Test 1: Normal operation");
    let start = Instant::now();

    let producers: Vec<_> = (0..NUM_PRODUCERS)
        .map(|producer_id| {
            let queue = queue.clone();
            let items_pushed = items_pushed.clone();
            thread::spawn(move || {
                let producer = queue.create_producer_handle().unwrap();
                for i in 0..ITEMS_PER_PRODUCER {
                    match producer.try_push(producer_id * ITEMS_PER_PRODUCER + i) {
                        Ok(_) => {
                            items_pushed.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            // Queue is full or closed, retry
                            thread::yield_now();
                            continue;
                        }
                    }
                }
            })
        })
        .collect();

    let consumer = thread::spawn({
        let queue = queue.clone();
        let items_popped = items_popped.clone();
        let target_items = NUM_PRODUCERS * ITEMS_PER_PRODUCER;
        move || {
            let mut consumed = 0;
            while consumed < target_items {
                match queue.pop() {
                    Ok(_) => {
                        consumed += 1;
                        items_popped.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        thread::yield_now();
                    }
                }
            }
            consumed
        }
    });

    // Wait for completion
    for p in producers {
        p.join().unwrap();
    }
    consumer.join().unwrap();

    let duration = start.elapsed();
    let total_pushed = items_pushed.load(Ordering::Relaxed);
    let total_popped = items_popped.load(Ordering::Relaxed);

    println!(
        "Normal test: pushed={}, popped={} in {:?}",
        total_pushed, total_popped, duration
    );
    assert_eq!(total_pushed, NUM_PRODUCERS * ITEMS_PER_PRODUCER);
    assert_eq!(total_popped, NUM_PRODUCERS * ITEMS_PER_PRODUCER);

    // Test 2: Operation with close (should not crash)
    println!("Test 2: Operation with close");
    let queue2 = Arc::new(Mpmc::<usize, P, NUM_SEGS_P2>::new());
    let items_pushed2 = Arc::new(AtomicUsize::new(0));
    let items_popped2 = Arc::new(AtomicUsize::new(0));

    let start2 = Instant::now();

    let producers2: Vec<_> = (0..NUM_PRODUCERS)
        .map(|producer_id| {
            let queue = queue2.clone();
            let items_pushed = items_pushed2.clone();
            thread::spawn(move || {
                let producer = queue.create_producer_handle().unwrap();
                for i in 0..ITEMS_PER_PRODUCER / 2 {
                    // Only push half the items
                    match producer.try_push(producer_id * (ITEMS_PER_PRODUCER / 2) + i) {
                        Ok(_) => {
                            items_pushed.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            if let bop_mpmc::PushError::Closed(_) = e {
                                break; // Queue was closed
                            }
                            thread::yield_now();
                            continue;
                        }
                    }
                }
            })
        })
        .collect();

    let consumer2 = thread::spawn({
        let queue = queue2.clone();
        let items_popped = items_popped2.clone();
        move || {
            let mut consumed = 0;
            for _ in 0..(NUM_PRODUCERS * ITEMS_PER_PRODUCER / 2) {
                match queue.pop() {
                    Ok(_) => {
                        consumed += 1;
                        items_popped.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        if let bop_mpmc::PopError::Closed = e {
                            break; // Queue was closed and empty
                        }
                        thread::yield_now();
                    }
                }
            }
            consumed
        }
    });

    // Let some work happen
    thread::sleep(std::time::Duration::from_millis(1));

    // Close the queue
    queue2.close();
    println!("Queue closed");

    // Wait for completion
    for p in producers2 {
        p.join().unwrap();
    }
    consumer2.join().unwrap();

    let duration2 = start2.elapsed();
    let total_pushed2 = items_pushed2.load(Ordering::Relaxed);
    let total_popped2 = items_popped2.load(Ordering::Relaxed);

    println!(
        "Close test: pushed={}, popped={} in {:?}",
        total_pushed2, total_popped2, duration2
    );

    // The close test should not crash, even though some pushes/pops might fail
    assert!(total_pushed2 <= NUM_PRODUCERS * (ITEMS_PER_PRODUCER / 2));
    assert!(total_popped2 <= total_pushed2);

    println!("✅ All tests passed successfully!");
}

fn main() {
    println!("Close functionality benchmark");
    println!("=============================");

    test_close_functionality_under_load();

    println!("Benchmark completed successfully!");
}
