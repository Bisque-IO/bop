//! SignalWaker Summary Selection Benchmark
//!
//! Producers call summary_select and track statistics
//! Consumers call mark_active to set bits
//! Runs for a specified duration and reports per-second stats
//!
//! Usage:
//!   cargo run --example signal_waker_benchmark --release [--duration SECONDS]

use bop_mpmc::SignalWaker;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

/// Format a large number with thousand separators for readability
fn humanize_number(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }

    let mut result = String::new();
    let mut count = 0;

    while n > 0 {
        if count > 0 && count % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, char::from_digit((n % 10) as u32, 10).unwrap());
        n /= 10;
        count += 1;
    }

    result
}

fn main() {
    // Parse command line args
    let args: Vec<String> = std::env::args().collect();
    let duration_secs = if args.len() > 1 {
        args[1].parse().unwrap_or(3)
    } else {
        3
    };

    println!("{}", "=".repeat(60));
    println!("SignalWaker Summary Selection Benchmark");
    println!("Duration: {} seconds", duration_secs);
    println!("{}", "=".repeat(60));

    run_benchmark(duration_secs);
}

fn run_benchmark(duration_secs: u64) {
    let waker = Arc::new(SignalWaker::new());
    let stop_flag = Arc::new(AtomicU64::new(0));

    // Statistics tracking
    let total_producer_calls = Arc::new(AtomicU64::new(0));
    let total_successful_calls = Arc::new(AtomicU64::new(0));
    let total_consumer_calls = Arc::new(AtomicU64::new(0));

    // Thread counts
    const NUM_PRODUCERS: usize = 6;
    const NUM_CONSUMERS: usize = 4;

    let barrier = Arc::new(Barrier::new(NUM_PRODUCERS + NUM_CONSUMERS + 1)); // +1 for main thread

    let mut handles = vec![];

    // Start producer threads
    for producer_id in 0..NUM_PRODUCERS {
        let waker_clone = waker.clone();
        let barrier_clone = barrier.clone();
        let stop_clone = stop_flag.clone();
        let calls_clone = total_producer_calls.clone();
        let success_clone = total_successful_calls.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait(); // Wait for all threads to start

            let mut local_calls = 0;
            let mut local_success = 0;

            // Each producer starts with a different base for variety
            let base_index = ((producer_id as u64) * 1000) % 64;
            let mut index = base_index;

            while stop_clone.load(Ordering::Relaxed) == 0 {
                // Call summary_select and track results
                let result = waker_clone.summary_select(producer_id as u64);
                local_calls += 1;

                if result < 64 {
                    local_success += 1;
                    // waker_clone.try_unmark(result);
                }

                // Increment index for next call
                // index = (index + 7) % 64; // Use prime number for better distribution

                // // Optional: yield periodically to avoid overwhelming CPU
                // if local_calls % 10000 == 0 {
                //     std::hint::spin_loop();
                // }
            }

            // Update global counters
            calls_clone.fetch_add(local_calls, Ordering::Relaxed);
            success_clone.fetch_add(local_success, Ordering::Relaxed);

            local_calls
        });

        handles.push(handle);
    }

    // Start consumer threads
    for consumer_id in 0..NUM_CONSUMERS {
        let waker_clone = waker.clone();
        let barrier_clone = barrier.clone();
        let stop_clone = stop_flag.clone();
        let calls_clone = total_consumer_calls.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait(); // Wait for all threads to start

            let mut local_calls = 0;
            let mut operation_counter = 0;

            while stop_clone.load(Ordering::Relaxed) == 0 {
                // Call mark_active with a bit pattern
                // Use different strategies for variety
                // let bit_index = match operation_counter % 3 {
                //     0 => operation_counter % 64,                        // Sequential
                //     1 => ((consumer_id * 17) + operation_counter) % 64, // Mix with thread ID
                //     _ => (operation_counter * 13) % 64,                 // Prime number pattern
                // };

                local_calls += 1;
                operation_counter += 1;
                waker_clone.try_unmark(consumer_id as u64);
                if local_calls & 2047 == 0 {
                    waker_clone.mark_active(consumer_id as u64);
                }

                // Optional: yield periodically
                // if local_calls % 5000 == 0 {
                //     std::hint::spin_loop();
                // }
            }

            // Update global counter
            calls_clone.fetch_add(local_calls, Ordering::Relaxed);

            local_calls
        });

        handles.push(handle);
    }

    // Start timing and run for specified duration
    let start_time = Instant::now();

    // Signal all threads to start
    barrier.wait();

    loop {
        // println!("{} of {}", sec, duration_secs);
        // Run for specified duration
        thread::sleep(Duration::from_millis(50));
        let now = Instant::now();
        if now.duration_since(start_time) > Duration::from_secs(duration_secs) {
            break;
        }
    }

    // Signal threads to stop
    stop_flag.store(1, Ordering::Relaxed);

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    let total_duration = start_time.elapsed();
    let duration_secs_f64 = total_duration.as_secs_f64();

    // Get final statistics
    let total_producer_calls_val = total_producer_calls.load(Ordering::Relaxed);
    let total_successful_calls_val = total_successful_calls.load(Ordering::Relaxed);
    let total_consumer_calls_val = total_consumer_calls.load(Ordering::Relaxed);

    // Calculate per-second rates
    let producer_calls_per_sec = total_producer_calls_val as f64 / duration_secs_f64;
    let _successful_calls_per_sec = total_successful_calls_val as f64 / duration_secs_f64;
    let consumer_calls_per_sec = total_consumer_calls_val as f64 / duration_secs_f64;
    let success_rate = if total_producer_calls_val > 0 {
        (total_successful_calls_val as f64 / total_producer_calls_val as f64) * 100.0
    } else {
        0.0
    };

    // Print results
    println!("\n=== Results ===");
    println!("Test duration: {:.2} seconds", duration_secs_f64);

    println!("\nProducer Statistics:");
    println!("  Thread count: {}", NUM_PRODUCERS);
    println!(
        "  Total summary_select calls: {}",
        humanize_number(total_producer_calls_val)
    );
    println!(
        "  Successful returns (< 64): {}",
        humanize_number(total_successful_calls_val)
    );
    println!("  Calls per second: {:.0}", producer_calls_per_sec);
    println!("  Success rate: {:.1}%", success_rate);

    // Individual results are tracked globally

    println!("\nConsumer Statistics:");
    println!("  Thread count: {}", NUM_CONSUMERS);
    println!(
        "  Total mark_active calls: {}",
        humanize_number(total_consumer_calls_val)
    );
    println!("  Calls per second: {:.0}", consumer_calls_per_sec);

    // Individual results are tracked globally

    println!("\n=== Summary ===");
    println!("Overall Performance:");
    println!(
        "  Total operations per second: {:.0}",
        producer_calls_per_sec + consumer_calls_per_sec
    );
    println!("  Summary selection efficiency: {:.1}%", success_rate);
    println!(
        "  Production-to-consumption ratio: {:.2}",
        total_consumer_calls_val as f64 / total_producer_calls_val as f64
    );

    println!("\n{}", "=".repeat(60));
}
