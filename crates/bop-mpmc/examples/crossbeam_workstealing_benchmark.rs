//! Crossbeam-Deque Work-Stealing Throughput Benchmark
//!
//! Multi-threaded benchmark testing the throughput of crossbeam-deque work-stealing queues.
//! Tests various scenarios:
//! - Single producer, multiple workers (work-stealing)
//! - Multiple producers, multiple workers
//! - Balanced vs imbalanced workloads
//! - Different worker counts and contention levels

use crossbeam_deque::{Injector, Steal, Worker};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

fn humanize_number(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Benchmark: Single injector feeding multiple workers with work-stealing
fn benchmark_injector_workstealing(num_workers: usize, total_items: usize) {
    println!(
        "\n=== Injector + {} Workers (Work-Stealing) | Items: {} ===",
        num_workers,
        humanize_number(total_items as u64)
    );

    let injector = Arc::new(Injector::<usize>::new());
    let workers: Vec<Worker<usize>> = (0..num_workers).map(|_| Worker::new_fifo()).collect();
    let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();
    let stealers = Arc::new(stealers);

    let completed = Arc::new(AtomicU64::new(0));
    let remaining = Arc::new(AtomicUsize::new(total_items));

    let start = Instant::now();

    // Spawn producer thread
    let producer_handle = {
        let injector = injector.clone();
        thread::spawn(move || {
            for i in 0..total_items {
                injector.push(i);
            }
        })
    };

    // Spawn worker threads
    let worker_handles: Vec<_> = workers
        .into_iter()
        .enumerate()
        .map(|(worker_id, worker)| {
            let injector = injector.clone();
            let stealers = stealers.clone();
            let completed = completed.clone();
            let remaining = remaining.clone();

            thread::spawn(move || {
                let mut local_count = 0u64;
                loop {
                    let mut found_work = false;

                    // Try local queue first
                    if let Some(_item) = worker.pop() {
                        local_count += 1;
                        remaining.fetch_sub(1, Ordering::Relaxed);
                        found_work = true;
                        continue;
                    }

                    // Try stealing from injector
                    loop {
                        match injector.steal_batch_and_pop(&worker) {
                            Steal::Success(_item) => {
                                local_count += 1;
                                remaining.fetch_sub(1, Ordering::Relaxed);
                                found_work = true;
                                break;
                            }
                            Steal::Empty => break,
                            Steal::Retry => {
                                std::hint::spin_loop();
                                continue;
                            }
                        }
                    }

                    if found_work {
                        continue;
                    }

                    // Try stealing from other workers
                    for (i, stealer) in stealers.iter().enumerate() {
                        if i == worker_id {
                            continue;
                        }
                        loop {
                            match stealer.steal_batch_and_pop(&worker) {
                                Steal::Success(_item) => {
                                    local_count += 1;
                                    remaining.fetch_sub(1, Ordering::Relaxed);
                                    found_work = true;
                                    break;
                                }
                                Steal::Empty => break,
                                Steal::Retry => {
                                    std::hint::spin_loop();
                                    continue;
                                }
                            }
                        }
                        if found_work {
                            break;
                        }
                    }

                    // If no work found and no items remaining, exit
                    if !found_work && remaining.load(Ordering::Relaxed) == 0 {
                        break;
                    }

                    if !found_work {
                        std::hint::spin_loop();
                    }
                }
                completed.fetch_add(local_count, Ordering::Relaxed);
            })
        })
        .collect();

    // Wait for producer to finish
    producer_handle.join().unwrap();

    // Wait for workers to finish
    for handle in worker_handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / total_items as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "  ⏱️  {:>10.2} ns/op | {:>15} ops/sec | Duration: {:.2}s",
        ns_per_op,
        humanize_number(ops_per_sec),
        duration.as_secs_f64()
    );
}

/// Benchmark: Each worker has its own local queue, steals from others when empty
fn benchmark_worker_local_workstealing(num_workers: usize, items_per_worker: usize) {
    let total_items = num_workers * items_per_worker;
    println!(
        "\n=== {} Workers (Local Queues + Stealing) | Items/Worker: {} | Total: {} ===",
        num_workers,
        humanize_number(items_per_worker as u64),
        humanize_number(total_items as u64)
    );

    let workers: Vec<Worker<usize>> = (0..num_workers).map(|_| Worker::new_fifo()).collect();
    let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();
    let stealers = Arc::new(stealers);

    let completed = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    // Spawn worker threads - each starts with its own work
    let worker_handles: Vec<_> = workers
        .into_iter()
        .enumerate()
        .map(|(worker_id, worker)| {
            let stealers = stealers.clone();
            let completed = completed.clone();

            thread::spawn(move || {
                // Fill local queue
                let base = worker_id * items_per_worker;
                for i in 0..items_per_worker {
                    worker.push(base + i);
                }

                let mut local_count = 0u64;

                // Process items
                loop {
                    let mut found_work = false;

                    // Try local queue first
                    if let Some(_item) = worker.pop() {
                        local_count += 1;
                        found_work = true;
                        continue;
                    }

                    // Try stealing from other workers
                    for (i, stealer) in stealers.iter().enumerate() {
                        if i == worker_id {
                            continue;
                        }
                        loop {
                            match stealer.steal_batch_and_pop(&worker) {
                                Steal::Success(_item) => {
                                    local_count += 1;
                                    found_work = true;
                                    break;
                                }
                                Steal::Empty => break,
                                Steal::Retry => {
                                    std::hint::spin_loop();
                                    continue;
                                }
                            }
                        }
                        if found_work {
                            break;
                        }
                    }

                    // No more work available anywhere
                    if !found_work {
                        // Double-check all queues are empty
                        let mut any_work = false;
                        for stealer in stealers.iter() {
                            if !stealer.is_empty() {
                                any_work = true;
                                break;
                            }
                        }
                        if !any_work {
                            break;
                        }
                        std::hint::spin_loop();
                    }
                }

                completed.fetch_add(local_count, Ordering::Relaxed);
            })
        })
        .collect();

    // Wait for all workers to finish
    for handle in worker_handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / total_items as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;
    let total_completed = completed.load(Ordering::Relaxed);

    println!(
        "  ⏱️  {:>10.2} ns/op | {:>15} ops/sec | Completed: {} | Duration: {:.2}s",
        ns_per_op,
        humanize_number(ops_per_sec),
        humanize_number(total_completed),
        duration.as_secs_f64()
    );
}

/// Benchmark: Imbalanced workload - one worker gets most work, others steal
fn benchmark_imbalanced_workstealing(num_workers: usize, total_items: usize) {
    println!(
        "\n=== Imbalanced Workload ({} Workers) | Items: {} ===",
        num_workers,
        humanize_number(total_items as u64)
    );

    let workers: Vec<Worker<usize>> = (0..num_workers).map(|_| Worker::new_fifo()).collect();
    let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();
    let stealers = Arc::new(stealers);

    let completed = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    // Worker 0 gets 90% of the work, others get 10% divided among them
    let main_worker_items = (total_items * 9) / 10;
    let other_worker_items = total_items - main_worker_items;
    let items_per_other = if num_workers > 1 {
        other_worker_items / (num_workers - 1)
    } else {
        0
    };

    let worker_handles: Vec<_> = workers
        .into_iter()
        .enumerate()
        .map(|(worker_id, worker)| {
            let stealers = stealers.clone();
            let completed = completed.clone();

            thread::spawn(move || {
                // Fill local queue based on worker role
                let item_count = if worker_id == 0 {
                    main_worker_items
                } else {
                    items_per_other
                };

                let base = if worker_id == 0 {
                    0
                } else {
                    main_worker_items + (worker_id - 1) * items_per_other
                };

                for i in 0..item_count {
                    worker.push(base + i);
                }

                let mut local_count = 0u64;

                loop {
                    let mut found_work = false;

                    // Try local queue first
                    if let Some(_item) = worker.pop() {
                        local_count += 1;
                        found_work = true;
                        continue;
                    }

                    // Try stealing from other workers
                    for (i, stealer) in stealers.iter().enumerate() {
                        if i == worker_id {
                            continue;
                        }
                        loop {
                            match stealer.steal_batch_and_pop(&worker) {
                                Steal::Success(_item) => {
                                    local_count += 1;
                                    found_work = true;
                                    break;
                                }
                                Steal::Empty => break,
                                Steal::Retry => {
                                    std::hint::spin_loop();
                                    continue;
                                }
                            }
                        }
                        if found_work {
                            break;
                        }
                    }

                    if !found_work {
                        // Double-check all queues are empty before exiting
                        let mut any_work = false;
                        for stealer in stealers.iter() {
                            if !stealer.is_empty() {
                                any_work = true;
                                break;
                            }
                        }
                        if !any_work {
                            break;
                        }
                        std::hint::spin_loop();
                    }
                }

                completed.fetch_add(local_count, Ordering::Relaxed);
            })
        })
        .collect();

    for handle in worker_handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / total_items as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;
    let total_completed = completed.load(Ordering::Relaxed);

    println!(
        "  ⏱️  {:>10.2} ns/op | {:>15} ops/sec | Completed: {} | Duration: {:.2}s",
        ns_per_op,
        humanize_number(ops_per_sec),
        humanize_number(total_completed),
        duration.as_secs_f64()
    );
}

/// Benchmark: Multiple producers pushing to injector, multiple workers stealing
fn benchmark_multi_producer_workstealing(
    num_producers: usize,
    num_workers: usize,
    items_per_producer: usize,
) {
    let total_items = num_producers * items_per_producer;
    println!(
        "\n=== {} Producers + {} Workers | Items/Producer: {} | Total: {} ===",
        num_producers,
        num_workers,
        humanize_number(items_per_producer as u64),
        humanize_number(total_items as u64)
    );

    let injector = Arc::new(Injector::<usize>::new());
    let workers: Vec<Worker<usize>> = (0..num_workers).map(|_| Worker::new_fifo()).collect();
    let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();
    let stealers = Arc::new(stealers);

    let completed = Arc::new(AtomicU64::new(0));
    let remaining = Arc::new(AtomicUsize::new(total_items));

    let start = Instant::now();

    // Spawn producer threads
    let producer_handles: Vec<_> = (0..num_producers)
        .map(|producer_id| {
            let injector = injector.clone();
            thread::spawn(move || {
                let base = producer_id * items_per_producer;
                for i in 0..items_per_producer {
                    injector.push(base + i);
                }
            })
        })
        .collect();

    // Spawn worker threads
    let worker_handles: Vec<_> = workers
        .into_iter()
        .enumerate()
        .map(|(worker_id, worker)| {
            let injector = injector.clone();
            let stealers = stealers.clone();
            let completed = completed.clone();
            let remaining = remaining.clone();

            thread::spawn(move || {
                let mut local_count = 0u64;
                loop {
                    let mut found_work = false;

                    // Try local queue first
                    if let Some(_item) = worker.pop() {
                        local_count += 1;
                        remaining.fetch_sub(1, Ordering::Relaxed);
                        found_work = true;
                        continue;
                    }

                    // Try stealing from injector
                    loop {
                        match injector.steal_batch_and_pop(&worker) {
                            Steal::Success(_item) => {
                                local_count += 1;
                                remaining.fetch_sub(1, Ordering::Relaxed);
                                found_work = true;
                                break;
                            }
                            Steal::Empty => break,
                            Steal::Retry => {
                                std::hint::spin_loop();
                                continue;
                            }
                        }
                    }

                    if found_work {
                        continue;
                    }

                    // Try stealing from other workers
                    for (i, stealer) in stealers.iter().enumerate() {
                        if i == worker_id {
                            continue;
                        }
                        loop {
                            match stealer.steal_batch_and_pop(&worker) {
                                Steal::Success(_item) => {
                                    local_count += 1;
                                    remaining.fetch_sub(1, Ordering::Relaxed);
                                    found_work = true;
                                    break;
                                }
                                Steal::Empty => break,
                                Steal::Retry => {
                                    std::hint::spin_loop();
                                    continue;
                                }
                            }
                        }
                        if found_work {
                            break;
                        }
                    }

                    if !found_work && remaining.load(Ordering::Relaxed) == 0 {
                        break;
                    }

                    if !found_work {
                        std::hint::spin_loop();
                    }
                }
                completed.fetch_add(local_count, Ordering::Relaxed);
            })
        })
        .collect();

    // Wait for producers
    for handle in producer_handles {
        handle.join().unwrap();
    }

    // Wait for workers
    for handle in worker_handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / total_items as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;
    let total_completed = completed.load(Ordering::Relaxed);

    println!(
        "  ⏱️  {:>10.2} ns/op | {:>15} ops/sec | Completed: {} | Duration: {:.2}s",
        ns_per_op,
        humanize_number(ops_per_sec),
        humanize_number(total_completed),
        duration.as_secs_f64()
    );
}

/// Benchmark: High contention - many workers stealing from few queues
fn benchmark_high_contention(num_workers: usize, num_victim_queues: usize, total_items: usize) {
    println!(
        "\n=== High Contention: {} Workers stealing from {} Queues | Items: {} ===",
        num_workers,
        num_victim_queues,
        humanize_number(total_items as u64)
    );

    let workers: Vec<Worker<usize>> = (0..num_victim_queues).map(|_| Worker::new_fifo()).collect();
    let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();

    // Pre-fill victim queues
    let items_per_queue = total_items / num_victim_queues;
    for (i, worker) in workers.iter().enumerate() {
        let base = i * items_per_queue;
        for j in 0..items_per_queue {
            worker.push(base + j);
        }
    }

    let stealers = Arc::new(stealers);
    let completed = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    // Spawn worker threads (all are stealers in this benchmark)
    let worker_handles: Vec<_> = (0..num_workers)
        .map(|_| {
            let stealers = stealers.clone();
            let completed = completed.clone();

            thread::spawn(move || {
                let local_worker = Worker::<usize>::new_fifo();
                let mut local_count = 0u64;

                loop {
                    let mut found_work = false;

                    // Try local queue first
                    if let Some(_item) = local_worker.pop() {
                        local_count += 1;
                        found_work = true;
                        continue;
                    }

                    // Try stealing from victim queues
                    for stealer in stealers.iter() {
                        loop {
                            match stealer.steal_batch_and_pop(&local_worker) {
                                Steal::Success(_item) => {
                                    local_count += 1;
                                    found_work = true;
                                    break;
                                }
                                Steal::Empty => break,
                                Steal::Retry => {
                                    std::hint::spin_loop();
                                    continue;
                                }
                            }
                        }
                        if found_work {
                            break;
                        }
                    }

                    if !found_work {
                        // Double-check all queues are empty before exiting
                        let mut any_work = false;
                        for stealer in stealers.iter() {
                            if !stealer.is_empty() {
                                any_work = true;
                                break;
                            }
                        }
                        if !any_work {
                            break;
                        }
                        std::hint::spin_loop();
                    }
                }

                completed.fetch_add(local_count, Ordering::Relaxed);
            })
        })
        .collect();

    for handle in worker_handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / total_items as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;
    let total_completed = completed.load(Ordering::Relaxed);

    println!(
        "  ⏱️  {:>10.2} ns/op | {:>15} ops/sec | Completed: {} | Duration: {:.2}s",
        ns_per_op,
        humanize_number(ops_per_sec),
        humanize_number(total_completed),
        duration.as_secs_f64()
    );
}

fn main() {
    println!("{}", "=".repeat(80));
    println!("Crossbeam-Deque Work-Stealing Throughput Benchmark");
    println!("{}", "=".repeat(80));

    let num_cpus = num_cpus::get();
    println!("\nDetected {} CPU cores", num_cpus);

    let total_items = 1_000_000; // Reduced for faster testing

    // Test 1: Injector-based work-stealing with varying worker counts
    println!("\n{}", "-".repeat(80));
    println!("Test 1: Injector + Workers (Single Producer, Multiple Consumers)");
    println!("{}", "-".repeat(80));

    benchmark_injector_workstealing(2, total_items);
    benchmark_injector_workstealing(4, total_items);
    benchmark_injector_workstealing(8, total_items);

    // Test 2: Local worker queues with work-stealing
    println!("\n{}", "-".repeat(80));
    println!("Test 2: Local Worker Queues (Balanced Workload)");
    println!("{}", "-".repeat(80));

    benchmark_worker_local_workstealing(2, total_items / 2);
    benchmark_worker_local_workstealing(4, total_items / 4);
    benchmark_worker_local_workstealing(8, total_items / 8);

    // Test 3: Imbalanced workload
    println!("\n{}", "-".repeat(80));
    println!("Test 3: Imbalanced Workload (90% on one worker, 10% distributed)");
    println!("{}", "-".repeat(80));

    benchmark_imbalanced_workstealing(4, total_items);
    benchmark_imbalanced_workstealing(8, total_items);

    // Test 4: Multiple producers
    println!("\n{}", "-".repeat(80));
    println!("Test 4: Multiple Producers + Multiple Workers");
    println!("{}", "-".repeat(80));

    benchmark_multi_producer_workstealing(2, 4, total_items / 2);
    benchmark_multi_producer_workstealing(4, 4, total_items / 4);

    // Test 5: High contention scenario
    println!("\n{}", "-".repeat(80));
    println!("Test 5: High Contention (Many stealers, few victims)");
    println!("{}", "-".repeat(80));

    benchmark_high_contention(8, 2, total_items);
    benchmark_high_contention(16, 4, total_items);

    println!("\n{}", "=".repeat(80));
    println!("Benchmark complete!");
    println!("{}", "=".repeat(80));
}
