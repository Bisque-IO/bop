//! St3 Work-Stealing Throughput Benchmark
//!
//! Multi-threaded benchmark testing the throughput of st3 work-stealing queues.
//! St3 is a bounded, lock-free work-stealing queue optimized for performance.
//! Tests various scenarios:
//! - Single producer, multiple workers (work-stealing)
//! - Balanced vs imbalanced workloads
//! - Different worker counts and contention levels
//! - LIFO vs FIFO semantics

use st3::fifo::Worker as FifoWorker;
use st3::lifo::Worker as LifoWorker;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Benchmark: LIFO workers with balanced workload and work-stealing
fn benchmark_lifo_balanced_workstealing(
    num_workers: usize,
    items_per_worker: usize,
    capacity: usize,
) {
    let total_items = num_workers * items_per_worker;
    println!(
        "\n=== {} LIFO Workers (Balanced) | Items/Worker: {} | Total: {} | Capacity: {} ===",
        num_workers,
        humanize_number(items_per_worker as u64),
        humanize_number(total_items as u64),
        capacity
    );

    let workers: Vec<LifoWorker<usize>> = (0..num_workers)
        .map(|_| LifoWorker::new(capacity))
        .collect();
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
                    // If queue is full, process some items first
                    while worker.push(base + i).is_err() {
                        if worker.pop().is_some() {
                            completed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                let mut local_count = 0u64;

                // Process items
                loop {
                    let mut found_work = false;

                    // Try local queue first (LIFO pop)
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
                        // Steal up to half of the victim's items
                        match stealer.steal(&worker, |n| n / 2) {
                            Ok(stolen) if stolen > 0 => {
                                local_count += stolen as u64;
                                found_work = true;
                                break;
                            }
                            _ => {}
                        }
                    }

                    // No more work available anywhere
                    if !found_work {
                        // Try one more round of stealing with minimal effort
                        let mut any_work = false;
                        for (i, stealer) in stealers.iter().enumerate() {
                            if i == worker_id {
                                continue;
                            }
                            // Try to steal 1 item to check if queue has work
                            match stealer.steal(&worker, |_| 1) {
                                Ok(stolen) if stolen > 0 => {
                                    local_count += stolen as u64;
                                    any_work = true;
                                    break;
                                }
                                _ => {}
                            }
                        }
                        if !any_work {
                            break;
                        }
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

/// Benchmark: FIFO workers with balanced workload and work-stealing
fn benchmark_fifo_balanced_workstealing(
    num_workers: usize,
    items_per_worker: usize,
    capacity: usize,
) {
    let total_items = num_workers * items_per_worker;
    println!(
        "\n=== {} FIFO Workers (Balanced) | Items/Worker: {} | Total: {} | Capacity: {} ===",
        num_workers,
        humanize_number(items_per_worker as u64),
        humanize_number(total_items as u64),
        capacity
    );

    let workers: Vec<FifoWorker<usize>> = (0..num_workers)
        .map(|_| FifoWorker::new(capacity))
        .collect();
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
                    // If queue is full, process some items first
                    while worker.push(base + i).is_err() {
                        if worker.pop().is_some() {
                            completed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                let mut local_count = 0u64;

                // Process items
                loop {
                    let mut found_work = false;

                    // Try local queue first (FIFO pop)
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
                        // Steal up to half of the victim's items
                        match stealer.steal(&worker, |n| n / 2) {
                            Ok(stolen) if stolen > 0 => {
                                local_count += stolen as u64;
                                found_work = true;
                                break;
                            }
                            _ => {}
                        }
                    }

                    // No more work available anywhere
                    if !found_work {
                        // Try one more round of stealing with minimal effort
                        let mut any_work = false;
                        for (i, stealer) in stealers.iter().enumerate() {
                            if i == worker_id {
                                continue;
                            }
                            // Try to steal 1 item to check if queue has work
                            match stealer.steal(&worker, |_| 1) {
                                Ok(stolen) if stolen > 0 => {
                                    local_count += stolen as u64;
                                    any_work = true;
                                    break;
                                }
                                _ => {}
                            }
                        }
                        if !any_work {
                            break;
                        }
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

/// Benchmark: LIFO imbalanced workload - one worker gets most work, others steal
fn benchmark_lifo_imbalanced_workstealing(num_workers: usize, total_items: usize, capacity: usize) {
    println!(
        "\n=== Imbalanced LIFO Workload ({} Workers) | Items: {} | Capacity: {} ===",
        num_workers,
        humanize_number(total_items as u64),
        capacity
    );

    let workers: Vec<LifoWorker<usize>> = (0..num_workers)
        .map(|_| LifoWorker::new(capacity))
        .collect();
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
                    // If queue is full, process some items first
                    while worker.push(base + i).is_err() {
                        if worker.pop().is_some() {
                            completed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
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
                        match stealer.steal(&worker, |n| n / 2) {
                            Ok(stolen) if stolen > 0 => {
                                local_count += stolen as u64;
                                found_work = true;
                                break;
                            }
                            _ => {}
                        }
                    }

                    if !found_work {
                        // Try one more round of stealing with minimal effort
                        let mut any_work = false;
                        for (i, stealer) in stealers.iter().enumerate() {
                            if i == worker_id {
                                continue;
                            }
                            // Try to steal 1 item to check if queue has work
                            match stealer.steal(&worker, |_| 1) {
                                Ok(stolen) if stolen > 0 => {
                                    local_count += stolen as u64;
                                    any_work = true;
                                    break;
                                }
                                _ => {}
                            }
                        }
                        if !any_work {
                            break;
                        }
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

/// Benchmark: High contention - many workers stealing from few queues (LIFO)
fn benchmark_lifo_high_contention(
    num_workers: usize,
    num_victim_queues: usize,
    total_items: usize,
    capacity: usize,
) {
    println!(
        "\n=== High Contention LIFO: {} Workers stealing from {} Queues | Items: {} | Capacity: {} ===",
        num_workers,
        num_victim_queues,
        humanize_number(total_items as u64),
        capacity
    );

    let workers: Vec<LifoWorker<usize>> = (0..num_victim_queues)
        .map(|_| LifoWorker::new(capacity))
        .collect();
    let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();

    // Pre-fill victim queues
    let items_per_queue = total_items / num_victim_queues;
    let completed = Arc::new(AtomicU64::new(0));

    for (i, worker) in workers.iter().enumerate() {
        let base = i * items_per_queue;
        for j in 0..items_per_queue {
            // If queue is full, we've hit capacity - this is expected for bounded queues
            if worker.push(base + j).is_err() {
                break;
            }
        }
    }

    let stealers = Arc::new(stealers);
    let start = Instant::now();

    // Spawn worker threads (all are stealers in this benchmark)
    let worker_handles: Vec<_> = (0..num_workers)
        .map(|_| {
            let stealers = stealers.clone();
            let completed = completed.clone();

            thread::spawn(move || {
                let local_worker = LifoWorker::<usize>::new(capacity);
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
                        match stealer.steal(&local_worker, |n| n / 2) {
                            Ok(stolen) if stolen > 0 => {
                                local_count += stolen as u64;
                                found_work = true;
                                break;
                            }
                            _ => {}
                        }
                    }

                    if !found_work {
                        // Try one more round of stealing with minimal effort
                        let mut any_work = false;
                        for stealer in stealers.iter() {
                            // Try to steal 1 item to check if queue has work
                            match stealer.steal(&local_worker, |_| 1) {
                                Ok(stolen) if stolen > 0 => {
                                    local_count += stolen as u64;
                                    any_work = true;
                                    break;
                                }
                                _ => {}
                            }
                        }
                        if !any_work {
                            break;
                        }
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
    let total_completed = completed.load(Ordering::Relaxed);
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / total_completed.max(1) as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "  ⏱️  {:>10.2} ns/op | {:>15} ops/sec | Completed: {} | Duration: {:.2}s",
        ns_per_op,
        humanize_number(ops_per_sec),
        humanize_number(total_completed),
        duration.as_secs_f64()
    );
}

/// Benchmark: Varying stealing strategies
fn benchmark_stealing_strategies(num_workers: usize, items_per_worker: usize, capacity: usize) {
    println!(
        "\n=== Stealing Strategy Comparison ({} Workers) | Items/Worker: {} | Capacity: {} ===",
        num_workers,
        humanize_number(items_per_worker as u64),
        capacity
    );

    // Test different stealing strategies
    let strategies: Vec<(&str, fn(usize) -> usize)> = vec![
        ("Steal Half", |n| n / 2),
        ("Steal Quarter", |n| n / 4),
        ("Steal 75%", |n| (n * 3) / 4),
        ("Steal All", |n| n),
    ];

    for (name, strategy) in strategies {
        let total_items = num_workers * items_per_worker;
        let workers: Vec<LifoWorker<usize>> = (0..num_workers)
            .map(|_| LifoWorker::new(capacity))
            .collect();
        let stealers: Vec<_> = workers.iter().map(|w| w.stealer()).collect();
        let stealers = Arc::new(stealers);

        let completed = Arc::new(AtomicU64::new(0));
        let start = Instant::now();

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
                        while worker.push(base + i).is_err() {
                            if worker.pop().is_some() {
                                completed.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }

                    let mut local_count = 0u64;

                    loop {
                        let mut found_work = false;

                        if let Some(_item) = worker.pop() {
                            local_count += 1;
                            found_work = true;
                            continue;
                        }

                        for (i, stealer) in stealers.iter().enumerate() {
                            if i == worker_id {
                                continue;
                            }
                            match stealer.steal(&worker, strategy) {
                                Ok(stolen) if stolen > 0 => {
                                    local_count += stolen as u64;
                                    found_work = true;
                                    break;
                                }
                                _ => {}
                            }
                        }

                        if !found_work {
                            // Try one more round of stealing with minimal effort
                            let mut any_work = false;
                            for (i, stealer) in stealers.iter().enumerate() {
                                if i == worker_id {
                                    continue;
                                }
                                // Try to steal 1 item to check if queue has work
                                match stealer.steal(&worker, |_| 1) {
                                    Ok(stolen) if stolen > 0 => {
                                        local_count += stolen as u64;
                                        any_work = true;
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            if !any_work {
                                break;
                            }
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

        println!(
            "  {} - {:>10.2} ns/op | {:>15} ops/sec | Duration: {:.2}s",
            name,
            ns_per_op,
            humanize_number(ops_per_sec),
            duration.as_secs_f64()
        );
    }
}

fn main() {
    println!("{}", "=".repeat(80));
    println!("St3 Work-Stealing Throughput Benchmark");
    println!("{}", "=".repeat(80));

    let num_cpus = num_cpus::get();
    println!("\nDetected {} CPU cores", num_cpus);

    let total_items = 1_000_000;
    let capacity = 1024; // Bounded queue capacity

    // Test 1: LIFO balanced workload
    println!("\n{}", "-".repeat(80));
    println!("Test 1: LIFO Workers (Balanced Workload)");
    println!("{}", "-".repeat(80));

    benchmark_lifo_balanced_workstealing(2, total_items / 2, capacity);
    benchmark_lifo_balanced_workstealing(4, total_items / 4, capacity);
    benchmark_lifo_balanced_workstealing(8, total_items / 8, capacity);

    // Test 2: FIFO balanced workload
    println!("\n{}", "-".repeat(80));
    println!("Test 2: FIFO Workers (Balanced Workload)");
    println!("{}", "-".repeat(80));

    benchmark_fifo_balanced_workstealing(2, total_items / 2, capacity);
    benchmark_fifo_balanced_workstealing(4, total_items / 4, capacity);
    benchmark_fifo_balanced_workstealing(8, total_items / 8, capacity);

    // Test 3: LIFO imbalanced workload
    println!("\n{}", "-".repeat(80));
    println!("Test 3: Imbalanced Workload (90% on one worker, 10% distributed)");
    println!("{}", "-".repeat(80));

    benchmark_lifo_imbalanced_workstealing(4, total_items, capacity);
    benchmark_lifo_imbalanced_workstealing(8, total_items, capacity);

    // Test 4: High contention scenario
    println!("\n{}", "-".repeat(80));
    println!("Test 4: High Contention (Many stealers, few victims)");
    println!("{}", "-".repeat(80));

    benchmark_lifo_high_contention(8, 2, total_items, capacity);
    benchmark_lifo_high_contention(16, 4, total_items, capacity);

    // Test 5: Different stealing strategies
    println!("\n{}", "-".repeat(80));
    println!("Test 5: Stealing Strategy Comparison");
    println!("{}", "-".repeat(80));

    benchmark_stealing_strategies(4, total_items / 4, capacity);

    println!("\n{}", "=".repeat(80));
    println!("Benchmark complete!");
    println!("{}", "=".repeat(80));
}
