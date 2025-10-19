//! Comprehensive Multi-Threaded Benchmark for MPMC Timer Wheel (TRUE Thread-Local)
//!
//! This benchmark tests the NEW thread-local architecture with ZERO synchronization
//! for local operations.
//!
//! Benchmarks:
//! 1. Pure scheduling throughput (each thread owns its wheel - NO LOCKS!)
//! 2. Cross-thread cancellation throughput (via SegSpsc queues)
//! 3. Mixed scheduling + cross-thread cancel

use std::{
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use bop_executor::mpmc_timer_wheel::{MpmcCoordinator, ThreadLocalWheel};

const WARMUP_ITERATIONS: usize = 10_000;
const BENCHMARK_DURATION_SECS: u64 = 5;

/// Benchmark configuration
#[derive(Clone, Debug)]
struct BenchmarkConfig {
    num_threads: usize,
    tick_resolution_ns: u64,
    ticks_per_wheel: usize,
    timer_spread_ticks: u64,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            num_threads: 4,
            tick_resolution_ns: 1024,
            ticks_per_wheel: 256,
            timer_spread_ticks: 100,
        }
    }
}

/// Statistics collector
struct BenchmarkStats {
    schedules_completed: AtomicU64,
    cancels_sent: AtomicU64,
    cancels_successful: AtomicU64,
    polls_completed: AtomicU64,
    timers_expired: AtomicU64,
}

impl BenchmarkStats {
    fn new() -> Self {
        Self {
            schedules_completed: AtomicU64::new(0),
            cancels_sent: AtomicU64::new(0),
            cancels_successful: AtomicU64::new(0),
            polls_completed: AtomicU64::new(0),
            timers_expired: AtomicU64::new(0),
        }
    }

    fn print_summary(&self, duration: Duration, label: &str) {
        let schedules = self.schedules_completed.load(Ordering::Relaxed);
        let cancels_sent = self.cancels_sent.load(Ordering::Relaxed);
        let cancels_ok = self.cancels_successful.load(Ordering::Relaxed);
        let polls = self.polls_completed.load(Ordering::Relaxed);
        let expired = self.timers_expired.load(Ordering::Relaxed);

        let secs = duration.as_secs_f64();
        let schedules_per_sec = schedules as f64 / secs;
        let cancels_per_sec = cancels_sent as f64 / secs;
        let polls_per_sec = polls as f64 / secs;
        let expired_per_sec = expired as f64 / secs;

        println!("\n=== {} ===", label);
        println!("Duration: {:.2}s", secs);
        println!("\nThroughput:");
        println!(
            "  Schedules: {:.2} M/sec ({} total)",
            schedules_per_sec / 1_000_000.0,
            schedules
        );

        if cancels_sent > 0 {
            let cancel_success_rate = (cancels_ok as f64 / cancels_sent as f64) * 100.0;
            println!(
                "  Cancels Sent: {:.2} M/sec ({} total)",
                cancels_per_sec / 1_000_000.0,
                cancels_sent
            );
            println!(
                "  Cancel Success: {} ({:.1}%)",
                cancels_ok, cancel_success_rate
            );
        }

        if polls > 0 {
            println!(
                "  Polls: {:.2} M/sec ({} total)",
                polls_per_sec / 1_000_000.0,
                polls
            );
            println!(
                "  Timers Expired: {:.2} M/sec ({} total)",
                expired_per_sec / 1_000_000.0,
                expired
            );
        }

        println!("\nPer-Thread Performance:");
        println!(
            "  Schedules/thread: {:.2} M/sec",
            schedules_per_sec / 1_000_000.0 / (schedules as f64 / schedules as f64).max(1.0)
        );
    }
}

/// Benchmark 1: Pure scheduling throughput (TRUE thread-local, NO LOCKS!)
fn benchmark_pure_scheduling(config: BenchmarkConfig) {
    println!("\n### Benchmark 1: Pure Scheduling Throughput (Thread-Local, NO LOCKS) ###");
    println!("Threads: {}", config.num_threads);

    let coordinator = MpmcCoordinator::<u64>::new(config.num_threads);
    let stats = Arc::new(BenchmarkStats::new());
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(config.num_threads + 1));

    let mut handles = vec![];

    for _ in 0..config.num_threads {
        let coord = coordinator.clone();
        let stats = stats.clone();
        let running = running.clone();
        let barrier = barrier.clone();
        let cfg = config.clone();

        let handle = thread::spawn(move || {
            // Each thread creates its OWN wheel (thread-local!)
            let thread_id = coord.register_thread();
            let mut wheel = ThreadLocalWheel::new(
                thread_id,
                coord,
                Duration::from_nanos(cfg.tick_resolution_ns),
                cfg.ticks_per_wheel,
            );

            // Warmup
            for i in 0..WARMUP_ITERATIONS {
                let deadline = (i as u64 % cfg.timer_spread_ticks) * cfg.tick_resolution_ns;
                wheel.schedule(deadline, i as u64);
            }

            barrier.wait();

            // Benchmark: Schedule as fast as possible (NO LOCKS!)
            let mut local_count = 0u64;
            while running.load(Ordering::Relaxed) {
                let deadline = (local_count % cfg.timer_spread_ticks) * cfg.tick_resolution_ns;
                if wheel.schedule(deadline, local_count).is_some() {
                    local_count += 1;
                }
            }

            stats
                .schedules_completed
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
    stats.print_summary(duration, "Pure Scheduling (Thread-Local, NO LOCKS)");
}

/// Benchmark 2: Cross-thread cancellation throughput
fn benchmark_cross_thread_cancel(config: BenchmarkConfig) {
    println!("\n### Benchmark 2: Cross-Thread Cancellation Throughput ###");
    println!("Threads: {}", config.num_threads);

    let coordinator = MpmcCoordinator::<u64>::new(config.num_threads);
    let stats = Arc::new(BenchmarkStats::new());
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(config.num_threads + 1));

    // Circular buffer for timer IDs (per thread)
    const BUFFER_SIZE: usize = 4096;
    let timer_buffers: Arc<Vec<Vec<AtomicU64>>> = Arc::new(
        (0..config.num_threads)
            .map(|_| (0..BUFFER_SIZE).map(|_| AtomicU64::new(0)).collect())
            .collect(),
    );
    let buffer_indices: Arc<Vec<AtomicU64>> =
        Arc::new((0..config.num_threads).map(|_| AtomicU64::new(0)).collect());

    let mut handles = vec![];

    for idx in 0..config.num_threads {
        let coord = coordinator.clone();
        let stats = stats.clone();
        let running = running.clone();
        let barrier = barrier.clone();
        let timer_buffers = timer_buffers.clone();
        let buffer_indices = buffer_indices.clone();
        let cfg = config.clone();

        let handle = thread::spawn(move || {
            // Each thread creates its OWN wheel
            let thread_id = coord.register_thread();
            let mut wheel = ThreadLocalWheel::new(
                thread_id,
                coord,
                Duration::from_nanos(cfg.tick_resolution_ns),
                cfg.ticks_per_wheel,
            );

            let my_buffer = &timer_buffers[thread_id];
            let my_buffer_idx = &buffer_indices[thread_id];
            let target_thread = (thread_id + 1) % cfg.num_threads;
            let target_buffer = &timer_buffers[target_thread];
            let target_buffer_idx = &buffer_indices[target_thread];

            // Warmup
            for i in 0..WARMUP_ITERATIONS {
                let deadline = (i as u64 % cfg.timer_spread_ticks) * cfg.tick_resolution_ns;
                wheel.schedule(deadline, i as u64);
            }

            barrier.wait();

            // Benchmark: Schedule + Cancel from different thread
            let mut local_schedules = 0u64;
            let mut local_cancels = 0u64;
            let mut local_success = 0u64;
            let mut read_idx = 0usize;

            while running.load(Ordering::Relaxed) {
                // Schedule locally (FAST - no locks!)
                for _ in 0..10 {
                    let deadline =
                        (local_schedules % cfg.timer_spread_ticks) * cfg.tick_resolution_ns;
                    if let Some(timer_id) = wheel.schedule(deadline, local_schedules) {
                        // Store timer_id for others to cancel
                        let idx =
                            my_buffer_idx.fetch_add(1, Ordering::Relaxed) as usize % BUFFER_SIZE;
                        my_buffer[idx].store(timer_id, Ordering::Release);
                        local_schedules += 1;
                    }
                }

                // Cancel from target thread (cross-thread via SegSpsc)
                let latest_idx = target_buffer_idx.load(Ordering::Acquire) as usize;
                if latest_idx > read_idx {
                    let cancel_count = ((latest_idx - read_idx).min(8)).max(1);
                    for _ in 0..cancel_count {
                        let idx = read_idx % BUFFER_SIZE;
                        let timer_id = target_buffer[idx].load(Ordering::Acquire);

                        if timer_id > 0 {
                            if wheel.cancel_remote(target_thread, timer_id, 0) {
                                local_success += 1;
                            }
                            local_cancels += 1;
                        }
                        read_idx += 1;
                    }
                }
            }

            stats
                .schedules_completed
                .fetch_add(local_schedules, Ordering::Relaxed);
            stats
                .cancels_sent
                .fetch_add(local_cancels, Ordering::Relaxed);
            stats
                .cancels_successful
                .fetch_add(local_success, Ordering::Relaxed);
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
    stats.print_summary(duration, "Schedule + Cross-Thread Cancel");
}

/// Benchmark 3: Scheduling + Polling (realistic workload)
fn benchmark_schedule_and_poll(config: BenchmarkConfig) {
    println!("\n### Benchmark 3: Scheduling + Polling (Realistic Workload) ###");
    println!("Threads: {}", config.num_threads);

    let coordinator = MpmcCoordinator::<u64>::new(config.num_threads);
    let stats = Arc::new(BenchmarkStats::new());
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(config.num_threads + 1));

    let mut handles = vec![];

    for _ in 0..config.num_threads {
        let coord = coordinator.clone();
        let stats = stats.clone();
        let running = running.clone();
        let barrier = barrier.clone();
        let cfg = config.clone();

        let handle = thread::spawn(move || {
            // Each thread creates its OWN wheel
            let thread_id = coord.register_thread();
            let mut wheel = ThreadLocalWheel::new(
                thread_id,
                coord,
                Duration::from_nanos(cfg.tick_resolution_ns),
                cfg.ticks_per_wheel,
            );

            // Warmup
            for i in 0..WARMUP_ITERATIONS {
                let deadline = (i as u64 % cfg.timer_spread_ticks) * cfg.tick_resolution_ns;
                wheel.schedule(deadline, i as u64);
            }

            barrier.wait();

            // Benchmark: Schedule + Poll (realistic pattern)
            let mut local_schedules = 0u64;
            let mut local_polls = 0u64;
            let mut local_expired = 0u64;
            let mut current_tick = 0u64;

            while running.load(Ordering::Relaxed) {
                // Schedule some timers
                for _ in 0..100 {
                    let deadline =
                        (local_schedules % cfg.timer_spread_ticks) * cfg.tick_resolution_ns;
                    if wheel.schedule(deadline, local_schedules).is_some() {
                        local_schedules += 1;
                    }
                }

                // Poll to collect expired timers
                for _ in 0..10 {
                    let now_ns = current_tick * cfg.tick_resolution_ns + cfg.tick_resolution_ns;
                    let expired = wheel.poll(now_ns, 100, 100);
                    local_expired += expired.len() as u64;
                    local_polls += 1;
                    current_tick += 1;
                }
            }

            stats
                .schedules_completed
                .fetch_add(local_schedules, Ordering::Relaxed);
            stats
                .polls_completed
                .fetch_add(local_polls, Ordering::Relaxed);
            stats
                .timers_expired
                .fetch_add(local_expired, Ordering::Relaxed);
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
    stats.print_summary(duration, "Schedule + Poll (Realistic)");
}

/// Benchmark 4: Scalability test
fn benchmark_scalability() {
    println!("\n### Benchmark 4: Scalability Test ###");

    let thread_counts = vec![1, 2, 4, 8];

    for num_threads in thread_counts {
        let config = BenchmarkConfig {
            num_threads,
            ..Default::default()
        };

        println!("\n--- Testing with {} threads ---", num_threads);

        let coordinator = MpmcCoordinator::<u64>::new(config.num_threads);
        let stats = Arc::new(BenchmarkStats::new());
        let running = Arc::new(AtomicBool::new(true));
        let barrier = Arc::new(Barrier::new(config.num_threads + 1));

        let mut handles = vec![];

        for _ in 0..config.num_threads {
            let coord = coordinator.clone();
            let stats = stats.clone();
            let running = running.clone();
            let barrier = barrier.clone();
            let cfg = config.clone();

            let handle = thread::spawn(move || {
                let thread_id = coord.register_thread();
                let mut wheel = ThreadLocalWheel::new(
                    thread_id,
                    coord,
                    Duration::from_nanos(cfg.tick_resolution_ns),
                    cfg.ticks_per_wheel,
                );

                // Warmup
                for i in 0..WARMUP_ITERATIONS {
                    let deadline = (i as u64 % cfg.timer_spread_ticks) * cfg.tick_resolution_ns;
                    wheel.schedule(deadline, i as u64);
                }

                barrier.wait();

                let mut local_count = 0u64;
                while running.load(Ordering::Relaxed) {
                    let deadline = (local_count % cfg.timer_spread_ticks) * cfg.tick_resolution_ns;
                    if wheel.schedule(deadline, local_count).is_some() {
                        local_count += 1;
                    }
                }

                stats
                    .schedules_completed
                    .fetch_add(local_count, Ordering::Relaxed);
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
        let schedules = stats.schedules_completed.load(Ordering::Relaxed);
        let secs = duration.as_secs_f64();
        let schedules_per_sec = schedules as f64 / secs;

        println!("  Total: {:.2} M/sec", schedules_per_sec / 1_000_000.0);
        println!(
            "  Per-thread: {:.2} M/sec",
            schedules_per_sec / 1_000_000.0 / num_threads as f64
        );
    }
}

fn main() {
    println!("====================================");
    println!("MPMC Timer Wheel Benchmark Suite");
    println!("TRUE Thread-Local (NO LOCKS!)");
    println!("====================================");

    let config = BenchmarkConfig::default();

    // Run all benchmarks
    // benchmark_pure_scheduling(config.clone());
    benchmark_cross_thread_cancel(config.clone());
    benchmark_schedule_and_poll(config.clone());
    benchmark_scalability();

    println!("\n====================================");
    println!("Benchmark Suite Complete");
    println!("====================================");
}
