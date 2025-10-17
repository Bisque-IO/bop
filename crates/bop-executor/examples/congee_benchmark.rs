//! Comprehensive Multi-Threaded Benchmark for Congee ART (Adaptive Radix Tree)
//!
//! This benchmark tests the congee crate's ART radix tree map with usize keys across
//! multiple concurrent scenarios:
//!
//! 1. Pure Insert Throughput - Concurrent inserts with disjoint key ranges
//! 2. Pure Lookup Throughput - Concurrent reads after pre-population
//! 3. Mixed Read/Write - Realistic workload with 80% reads, 20% writes
//! 4. Scalability Test - Performance across 1, 2, 4, 8 threads
//! 5. Contention Test - Hot key scenarios with overlapping ranges
//!
//! The ART is a concurrent radix tree optimized for:
//! - Space-efficient key storage
//! - Lock-free reads
//! - Fine-grained locking for writes

use std::{
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use congee::Congee;

const WARMUP_ITERATIONS: usize = 10_000;
const BENCHMARK_DURATION_SECS: u64 = 5;

/// Benchmark configuration
#[derive(Clone, Debug)]
struct BenchmarkConfig {
    num_threads: usize,
    key_range_per_thread: usize,
    read_percentage: usize, // 0-100
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            num_threads: 8,
            key_range_per_thread: 10_000_000,
            read_percentage: 80,
        }
    }
}

/// Statistics collector
struct BenchmarkStats {
    inserts_completed: AtomicUsize,
    lookups_completed: AtomicUsize,
    lookups_found: AtomicUsize,
    updates_completed: AtomicUsize,
    removes_completed: AtomicUsize,
}

impl BenchmarkStats {
    fn new() -> Self {
        Self {
            inserts_completed: AtomicUsize::new(0),
            lookups_completed: AtomicUsize::new(0),
            lookups_found: AtomicUsize::new(0),
            updates_completed: AtomicUsize::new(0),
            removes_completed: AtomicUsize::new(0),
        }
    }

    fn print_summary(&self, duration: Duration, label: &str, num_threads: usize) {
        let inserts = self.inserts_completed.load(Ordering::Relaxed);
        let lookups = self.lookups_completed.load(Ordering::Relaxed);
        let found = self.lookups_found.load(Ordering::Relaxed);
        let updates = self.updates_completed.load(Ordering::Relaxed);
        let removes = self.removes_completed.load(Ordering::Relaxed);

        let secs = duration.as_secs_f64();
        let total_ops = inserts + lookups + updates + removes;
        let total_ops_per_sec = total_ops as f64 / secs;

        println!("\n=== {} ===", label);
        println!("Duration: {:.2}s", secs);
        println!("Threads: {}", num_threads);
        println!("\nThroughput:");

        if total_ops > 0 {
            println!(
                "  Total ops: {:.2} M/sec ({} total)",
                total_ops_per_sec / 1_000_000.0,
                total_ops
            );
        }

        if inserts > 0 {
            let inserts_per_sec = inserts as f64 / secs;
            println!(
                "  Inserts: {:.2} M/sec ({} total)",
                inserts_per_sec / 1_000_000.0,
                inserts
            );
        }

        if lookups > 0 {
            let lookups_per_sec = lookups as f64 / secs;
            let hit_rate = (found as f64 / lookups as f64) * 100.0;
            println!(
                "  Lookups: {:.2} M/sec ({} total, {:.1}% hit rate)",
                lookups_per_sec / 1_000_000.0,
                lookups,
                hit_rate
            );
        }

        if updates > 0 {
            let updates_per_sec = updates as f64 / secs;
            println!(
                "  Updates: {:.2} M/sec ({} total)",
                updates_per_sec / 1_000_000.0,
                updates
            );
        }

        if removes > 0 {
            let removes_per_sec = removes as f64 / secs;
            println!(
                "  Removes: {:.2} M/sec ({} total)",
                removes_per_sec / 1_000_000.0,
                removes
            );
        }

        println!("\nPer-Thread Performance:");
        println!(
            "  Ops/thread: {:.2} M/sec",
            total_ops_per_sec / 1_000_000.0 / num_threads as f64
        );
    }
}

/// Benchmark 1: Pure Insert Throughput
/// Each thread inserts into disjoint key ranges to minimize contention
fn benchmark_pure_insert(config: BenchmarkConfig) {
    println!("\n### Benchmark 1: Pure Insert Throughput (Disjoint Keys) ###");
    println!("Threads: {}", config.num_threads);

    let tree = Arc::new(Congee::<usize, usize>::default());
    let stats = Arc::new(BenchmarkStats::new());
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(config.num_threads + 1));

    let mut handles = vec![];

    for thread_id in 0..config.num_threads {
        let tree = tree.clone();
        let stats = stats.clone();
        let running = running.clone();
        let barrier = barrier.clone();
        let cfg = config.clone();

        let handle = thread::spawn(move || {
            // Each thread gets its own key range
            let key_base = thread_id * cfg.key_range_per_thread;

            // Warmup
            let guard = tree.pin();
            for i in 0..WARMUP_ITERATIONS {
                let key = key_base + i;
                tree.insert(key, key * 2, &guard);
            }

            barrier.wait();

            // Benchmark: Insert as fast as possible
            let mut local_count = 0usize;
            while running.load(Ordering::Relaxed) {
                let key = key_base + local_count;
                tree.insert(key, key * 2, &guard);
                local_count += 1;
            }

            stats
                .inserts_completed
                .fetch_add(local_count, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    // Wait for all threads ready
    barrier.wait();

    // Run benchmark
    let start = Instant::now();
    thread::sleep(Duration::from_secs(BENCHMARK_DURATION_SECS));
    running.store(false, Ordering::Relaxed);

    // Wait for completion
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    stats.print_summary(duration, "Pure Insert (Disjoint Keys)", config.num_threads);
}

/// Benchmark 2: Pure Lookup Throughput
/// Pre-populate tree, then all threads perform concurrent lookups
fn benchmark_pure_lookup(config: BenchmarkConfig) {
    println!("\n### Benchmark 2: Pure Lookup Throughput ###");
    println!("Threads: {}", config.num_threads);

    let tree = Arc::new(Congee::<usize, usize>::default());

    // Pre-populate tree
    println!(
        "Pre-populating tree with {} keys per thread...",
        config.key_range_per_thread
    );
    let guard = tree.pin();
    for thread_id in 0..config.num_threads {
        let key_base = thread_id * config.key_range_per_thread;
        for i in 0..config.key_range_per_thread {
            let key = key_base + i;
            tree.insert(key, key * 2, &guard);
        }
    }
    println!("Pre-population complete.");

    let stats = Arc::new(BenchmarkStats::new());
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(config.num_threads + 1));

    let mut handles = vec![];

    for thread_id in 0..config.num_threads {
        let tree = tree.clone();
        let stats = stats.clone();
        let running = running.clone();
        let barrier = barrier.clone();
        let cfg = config.clone();

        let handle = thread::spawn(move || {
            let key_base = thread_id * cfg.key_range_per_thread;
            let guard = tree.pin();

            // Warmup
            for i in 0..WARMUP_ITERATIONS {
                let key = key_base + (i % cfg.key_range_per_thread);
                let _ = tree.get(&key, &guard);
            }

            barrier.wait();

            // Benchmark: Lookup as fast as possible
            let mut local_count = 0usize;
            let mut found_count = 0usize;
            while running.load(Ordering::Relaxed) {
                let key = key_base + (local_count % cfg.key_range_per_thread);
                if tree.get(&key, &guard).is_some() {
                    found_count += 1;
                }
                local_count += 1;
            }

            stats
                .lookups_completed
                .fetch_add(local_count, Ordering::Relaxed);
            stats
                .lookups_found
                .fetch_add(found_count, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    // Wait for all threads ready
    barrier.wait();

    // Run benchmark
    let start = Instant::now();
    thread::sleep(Duration::from_secs(BENCHMARK_DURATION_SECS));
    running.store(false, Ordering::Relaxed);

    // Wait for completion
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    stats.print_summary(duration, "Pure Lookup", config.num_threads);
}

/// Benchmark 3: Mixed Read/Write (80/20)
/// Realistic workload with mostly reads and some writes
fn benchmark_mixed_read_write(config: BenchmarkConfig) {
    println!(
        "\n### Benchmark 3: Mixed Read/Write ({}% reads, {}% writes) ###",
        config.read_percentage,
        100 - config.read_percentage
    );
    println!("Threads: {}", config.num_threads);

    let tree = Arc::new(Congee::<usize, usize>::default());

    // Pre-populate tree with half capacity
    println!("Pre-populating tree...");
    let guard = tree.pin();
    for thread_id in 0..config.num_threads {
        let key_base = thread_id * config.key_range_per_thread;
        for i in 0..(config.key_range_per_thread / 2) {
            let key = key_base + i;
            tree.insert(key, key * 2, &guard);
        }
    }
    println!("Pre-population complete.");

    let stats = Arc::new(BenchmarkStats::new());
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(config.num_threads + 1));

    let mut handles = vec![];

    for thread_id in 0..config.num_threads {
        let tree = tree.clone();
        let stats = stats.clone();
        let running = running.clone();
        let barrier = barrier.clone();
        let cfg = config.clone();

        let handle = thread::spawn(move || {
            let key_base = thread_id * cfg.key_range_per_thread;
            let guard = tree.pin();

            // Warmup
            for i in 0..WARMUP_ITERATIONS {
                let key = key_base + (i % cfg.key_range_per_thread);
                if i % 100 < cfg.read_percentage {
                    let _ = tree.get(&key, &guard);
                } else {
                    tree.insert(key, key * 3, &guard);
                }
            }

            barrier.wait();

            // Benchmark: Mixed workload
            let mut local_lookups = 0usize;
            let mut local_found = 0usize;
            let mut local_inserts = 0usize;
            let mut op_count = 0usize;

            while running.load(Ordering::Relaxed) {
                let key = key_base + (op_count % cfg.key_range_per_thread);

                // 80% reads, 20% writes (by default)
                if (op_count % 100) < cfg.read_percentage {
                    // Read
                    if tree.get(&key, &guard).is_some() {
                        local_found += 1;
                    }
                    local_lookups += 1;
                } else {
                    // Write
                    tree.insert(key, key * 3, &guard);
                    local_inserts += 1;
                }
                op_count += 1;
            }

            stats
                .lookups_completed
                .fetch_add(local_lookups, Ordering::Relaxed);
            stats
                .lookups_found
                .fetch_add(local_found, Ordering::Relaxed);
            stats
                .inserts_completed
                .fetch_add(local_inserts, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    // Wait for all threads ready
    barrier.wait();

    // Run benchmark
    let start = Instant::now();
    thread::sleep(Duration::from_secs(BENCHMARK_DURATION_SECS));
    running.store(false, Ordering::Relaxed);

    // Wait for completion
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    stats.print_summary(duration, "Mixed Read/Write", config.num_threads);
}

/// Benchmark 4: Contention Test (Overlapping Keys)
/// Test performance when threads compete for same key ranges
fn benchmark_contention(config: BenchmarkConfig) {
    println!("\n### Benchmark 4: Contention Test (Overlapping Keys) ###");
    println!("Threads: {}", config.num_threads);
    println!(
        "All threads compete for the same {} keys",
        config.key_range_per_thread
    );

    let tree = Arc::new(Congee::<usize, usize>::default());

    // Pre-populate tree
    println!("Pre-populating tree...");
    let guard = tree.pin();
    for i in 0..config.key_range_per_thread {
        tree.insert(i, i * 2, &guard);
    }
    println!("Pre-population complete.");

    let stats = Arc::new(BenchmarkStats::new());
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(config.num_threads + 1));

    let mut handles = vec![];

    for _thread_id in 0..config.num_threads {
        let tree = tree.clone();
        let stats = stats.clone();
        let running = running.clone();
        let barrier = barrier.clone();
        let cfg = config.clone();

        let handle = thread::spawn(move || {
            let guard = tree.pin();

            // Warmup
            for i in 0..WARMUP_ITERATIONS {
                let key = i % cfg.key_range_per_thread;
                if i % 2 == 0 {
                    let _ = tree.get(&key, &guard);
                } else {
                    tree.insert(key, key * 3, &guard);
                }
            }

            barrier.wait();

            // Benchmark: All threads hit same keys (contention!)
            let mut local_lookups = 0usize;
            let mut local_found = 0usize;
            let mut local_inserts = 0usize;
            let mut op_count = 0usize;

            while running.load(Ordering::Relaxed) {
                let key = (op_count % cfg.key_range_per_thread);

                // 50/50 read/write for maximum contention
                if op_count % 2 == 0 {
                    if tree.get(&key, &guard).is_some() {
                        local_found += 1;
                    }
                    local_lookups += 1;
                } else {
                    tree.insert(key, key * 3, &guard);
                    local_inserts += 1;
                }
                op_count += 1;
            }

            stats
                .lookups_completed
                .fetch_add(local_lookups, Ordering::Relaxed);
            stats
                .lookups_found
                .fetch_add(local_found, Ordering::Relaxed);
            stats
                .inserts_completed
                .fetch_add(local_inserts, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    // Wait for all threads ready
    barrier.wait();

    // Run benchmark
    let start = Instant::now();
    thread::sleep(Duration::from_secs(BENCHMARK_DURATION_SECS));
    running.store(false, Ordering::Relaxed);

    // Wait for completion
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    stats.print_summary(
        duration,
        "Contention Test (Overlapping Keys)",
        config.num_threads,
    );
}

/// Benchmark 5: Insert/Delete Throughput
/// Test insert and delete performance - critical for timer services
fn benchmark_insert_delete(config: BenchmarkConfig) {
    println!("\n### Benchmark 5: Insert/Delete Throughput ###");
    println!("Threads: {}", config.num_threads);
    println!("Pattern: Insert N keys, delete N keys, repeat");

    let tree = Arc::new(Congee::<usize, usize>::default());
    let stats = Arc::new(BenchmarkStats::new());
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(config.num_threads + 1));

    let mut handles = vec![];

    for thread_id in 0..config.num_threads {
        let tree = tree.clone();
        let stats = stats.clone();
        let running = running.clone();
        let barrier = barrier.clone();
        let cfg = config.clone();

        let handle = thread::spawn(move || {
            let key_base = thread_id * cfg.key_range_per_thread;
            let guard = tree.pin();

            // Warmup
            for i in 0..WARMUP_ITERATIONS {
                let key = key_base + i;
                tree.insert(key, key * 2, &guard);
                tree.remove(&key, &guard);
            }

            barrier.wait();

            // Benchmark: Insert batch, delete batch, repeat
            let mut local_inserts = 0usize;
            let mut local_removes = 0usize;
            let batch_size = 100;
            let mut cycle = 0usize;

            while running.load(Ordering::Relaxed) {
                let batch_base = key_base + (cycle * batch_size);

                // Insert batch
                for i in 0..batch_size {
                    let key = batch_base + i;
                    tree.insert(key, key * 2, &guard);
                    local_inserts += 1;
                }

                // Delete batch
                for i in 0..batch_size {
                    let key = batch_base + i;
                    tree.remove(&key, &guard);
                    local_removes += 1;
                }

                cycle += 1;
            }

            stats
                .inserts_completed
                .fetch_add(local_inserts, Ordering::Relaxed);
            stats
                .removes_completed
                .fetch_add(local_removes, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    barrier.wait();

    let start = Instant::now();
    thread::sleep(Duration::from_secs(BENCHMARK_DURATION_SECS));
    running.store(false, Ordering::Relaxed);

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    stats.print_summary(duration, "Insert/Delete Throughput", config.num_threads);
}

/// Benchmark 6: Timer Service Simulation
/// Realistic timer workload: 70% insert, 20% delete, 10% lookup
fn benchmark_timer_simulation(config: BenchmarkConfig) {
    println!("\n### Benchmark 6: Timer Service Simulation ###");
    println!("Threads: {}", config.num_threads);
    println!("Pattern: 70% insert, 20% delete, 10% lookup");

    let tree = Arc::new(Congee::<usize, usize>::default());
    let stats = Arc::new(BenchmarkStats::new());
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(config.num_threads + 1));

    // Pre-populate with some timers
    println!("Pre-populating tree...");
    let guard = tree.pin();
    for thread_id in 0..config.num_threads {
        let key_base = thread_id * config.key_range_per_thread;
        for i in 0..(config.key_range_per_thread / 4) {
            let key = key_base + i;
            tree.insert(key, key * 2, &guard);
        }
    }
    println!("Pre-population complete.");

    let mut handles = vec![];

    for thread_id in 0..config.num_threads {
        let tree = tree.clone();
        let stats = stats.clone();
        let running = running.clone();
        let barrier = barrier.clone();
        let cfg = config.clone();

        let handle = thread::spawn(move || {
            let key_base = thread_id * cfg.key_range_per_thread;
            let guard = tree.pin();

            barrier.wait();

            let mut local_inserts = 0usize;
            let mut local_removes = 0usize;
            let mut local_lookups = 0usize;
            let mut local_found = 0usize;
            let mut op_count = 0usize;

            while running.load(Ordering::Relaxed) {
                let key = key_base + (op_count % cfg.key_range_per_thread);
                let op_type = op_count % 100;

                if op_type < 70 {
                    // 70% insert (schedule timer)
                    tree.insert(key, key * 2, &guard);
                    local_inserts += 1;
                } else if op_type < 90 {
                    // 20% delete (cancel timer)
                    if tree.remove(&key, &guard).is_some() {
                        local_removes += 1;
                    }
                } else {
                    // 10% lookup (check timer exists)
                    if tree.get(&key, &guard).is_some() {
                        local_found += 1;
                    }
                    local_lookups += 1;
                }

                op_count += 1;
            }

            stats
                .inserts_completed
                .fetch_add(local_inserts, Ordering::Relaxed);
            stats
                .removes_completed
                .fetch_add(local_removes, Ordering::Relaxed);
            stats
                .lookups_completed
                .fetch_add(local_lookups, Ordering::Relaxed);
            stats
                .lookups_found
                .fetch_add(local_found, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    barrier.wait();

    let start = Instant::now();
    thread::sleep(Duration::from_secs(BENCHMARK_DURATION_SECS));
    running.store(false, Ordering::Relaxed);

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    stats.print_summary(duration, "Timer Service Simulation", config.num_threads);
}

/// Benchmark 7: Scalability Test
/// Test performance across different thread counts
fn benchmark_scalability() {
    println!("\n### Benchmark 7: Scalability Test (Mixed Workload) ###");

    let thread_counts = vec![1, 2, 4, 8];

    for num_threads in thread_counts {
        let config = BenchmarkConfig {
            num_threads,
            key_range_per_thread: 500_000,
            read_percentage: 80,
        };

        println!("\n--- Testing with {} threads ---", num_threads);

        let tree = Arc::new(Congee::<usize, usize>::default());

        // Pre-populate
        let guard = tree.pin();
        for thread_id in 0..config.num_threads {
            let key_base = thread_id * config.key_range_per_thread;
            for i in 0..(config.key_range_per_thread / 2) {
                let key = key_base + i;
                tree.insert(key, key * 2, &guard);
            }
        }

        let stats = Arc::new(BenchmarkStats::new());
        let running = Arc::new(AtomicBool::new(true));
        let barrier = Arc::new(Barrier::new(config.num_threads + 1));

        let mut handles = vec![];

        for thread_id in 0..config.num_threads {
            let tree = tree.clone();
            let stats = stats.clone();
            let running = running.clone();
            let barrier = barrier.clone();
            let cfg = config.clone();

            let handle = thread::spawn(move || {
                let key_base = thread_id * cfg.key_range_per_thread;
                let guard = tree.pin();

                barrier.wait();

                let mut local_lookups = 0usize;
                let mut local_found = 0usize;
                let mut local_inserts = 0usize;
                let mut op_count = 0usize;

                while running.load(Ordering::Relaxed) {
                    let key = key_base + (op_count % cfg.key_range_per_thread);

                    if (op_count % 100) < 80 {
                        if tree.get(&key, &guard).is_some() {
                            local_found += 1;
                        }
                        local_lookups += 1;
                    } else {
                        tree.insert(key, key * 3, &guard);
                        local_inserts += 1;
                    }
                    op_count += 1;
                }

                stats
                    .lookups_completed
                    .fetch_add(local_lookups, Ordering::Relaxed);
                stats
                    .lookups_found
                    .fetch_add(local_found, Ordering::Relaxed);
                stats
                    .inserts_completed
                    .fetch_add(local_inserts, Ordering::Relaxed);
            });
            handles.push(handle);
        }

        barrier.wait();

        let start = Instant::now();
        thread::sleep(Duration::from_secs(BENCHMARK_DURATION_SECS));
        running.store(false, Ordering::Relaxed);

        for handle in handles {
            handle.join().unwrap();
        }

        let duration = start.elapsed();

        // Calculate efficiency
        let total_ops = stats.inserts_completed.load(Ordering::Relaxed)
            + stats.lookups_completed.load(Ordering::Relaxed);
        let ops_per_sec = total_ops as f64 / duration.as_secs_f64();
        let per_thread_ops = ops_per_sec / num_threads as f64;

        println!("  Total: {:.2} M/sec", ops_per_sec / 1_000_000.0);
        println!("  Per-thread: {:.2} M/sec", per_thread_ops / 1_000_000.0);

        if num_threads == 1 {
            println!("  Efficiency: 100%");
        } else {
            // Compare to single-threaded baseline
            // Note: We'd need to store the baseline, but for now just show per-thread
        }
    }
}

fn main() {
    println!("====================================");
    println!("Congee ART Radix Tree Benchmark");
    println!("usize Keys, usize Values");
    println!("====================================");

    let config = BenchmarkConfig::default();

    // Run all benchmarks
    // benchmark_pure_insert(config.clone());
    // benchmark_pure_lookup(config.clone());
    // benchmark_mixed_read_write(config.clone());
    // benchmark_contention(config.clone());
    benchmark_insert_delete(config.clone());
    // benchmark_timer_simulation(config.clone());
    // benchmark_scalability();

    println!("\n====================================");
    println!("Benchmark Suite Complete");
    println!("====================================");
}
