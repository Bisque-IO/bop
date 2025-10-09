//! Tokio Multi-Threaded Executor Benchmark
//!
//! Comprehensive benchmark comparing Tokio's multi-threaded runtime with:
//! - Non-blocking async tasks (default runtime)
//! - Blocking task pool (spawn_blocking)
//! - CPU-bound workloads
//! - I/O simulation workloads
//! - Mixed workloads

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::runtime::{Builder, Runtime};
use tokio::task;

/// Format a large number with thousand separators for readability
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

/// Simulate CPU-bound work
fn cpu_work(iterations: u64) -> u64 {
    let mut sum = 0u64;
    for i in 0..iterations {
        sum = sum.wrapping_add(i).wrapping_mul(31);
    }
    sum
}

/// Simulate I/O-bound work with async sleep
async fn io_work_async(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// Simulate I/O-bound work with blocking sleep
fn io_work_blocking(duration: Duration) {
    std::thread::sleep(duration);
}

// ============================================================================
// CPU-Bound Workload Benchmarks
// ============================================================================

/// Benchmark CPU-bound work on async tasks (should show contention)
async fn benchmark_cpu_async(num_tasks: usize, work_per_task: u64) -> (Duration, u64) {
    let counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let counter = counter.clone();
        let handle = task::spawn(async move {
            let result = cpu_work(work_per_task);
            counter.fetch_add(result, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let duration = start.elapsed();
    let total = counter.load(Ordering::Relaxed);
    (duration, total)
}

/// Benchmark CPU-bound work on blocking thread pool (should show better parallelism)
async fn benchmark_cpu_blocking(num_tasks: usize, work_per_task: u64) -> (Duration, u64) {
    let counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let counter = counter.clone();
        let handle = task::spawn_blocking(move || {
            let result = cpu_work(work_per_task);
            counter.fetch_add(result, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let duration = start.elapsed();
    let total = counter.load(Ordering::Relaxed);
    (duration, total)
}

// ============================================================================
// I/O-Bound Workload Benchmarks
// ============================================================================

/// Benchmark I/O-bound work on async tasks (should be efficient)
async fn benchmark_io_async(num_tasks: usize, sleep_duration: Duration) -> Duration {
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let handle = task::spawn(async move {
            io_work_async(sleep_duration).await;
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    start.elapsed()
}

/// Benchmark I/O-bound work on blocking thread pool (should use more threads)
async fn benchmark_io_blocking(num_tasks: usize, sleep_duration: Duration) -> Duration {
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let handle = task::spawn_blocking(move || {
            io_work_blocking(sleep_duration);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    start.elapsed()
}

// ============================================================================
// Mixed Workload Benchmarks
// ============================================================================

/// Benchmark mixed CPU and I/O work on async tasks
async fn benchmark_mixed_async(
    num_tasks: usize,
    cpu_iterations: u64,
    io_duration: Duration,
) -> (Duration, u64) {
    let counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let counter = counter.clone();
        let handle = task::spawn(async move {
            let result = cpu_work(cpu_iterations);
            io_work_async(io_duration).await;
            counter.fetch_add(result, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let duration = start.elapsed();
    let total = counter.load(Ordering::Relaxed);
    (duration, total)
}

/// Benchmark mixed CPU and I/O work on blocking thread pool
async fn benchmark_mixed_blocking(
    num_tasks: usize,
    cpu_iterations: u64,
    io_duration: Duration,
) -> (Duration, u64) {
    let counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let counter = counter.clone();
        let handle = task::spawn_blocking(move || {
            let result = cpu_work(cpu_iterations);
            io_work_blocking(io_duration);
            counter.fetch_add(result, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let duration = start.elapsed();
    let total = counter.load(Ordering::Relaxed);
    (duration, total)
}

// ============================================================================
// Throughput Benchmarks (High Task Count)
// ============================================================================

/// Benchmark task spawning throughput with minimal work
async fn benchmark_spawn_throughput_async(num_tasks: usize) -> Duration {
    let start = Instant::now();

    let mut handles = Vec::new();
    for i in 0..num_tasks {
        let handle = task::spawn(async move {
            // Minimal work
            let _ = i.wrapping_add(1);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    start.elapsed()
}

/// Benchmark blocking task spawning throughput with minimal work
async fn benchmark_spawn_throughput_blocking(num_tasks: usize) -> Duration {
    let start = Instant::now();

    let mut handles = Vec::new();
    for i in 0..num_tasks {
        let handle = task::spawn_blocking(move || {
            // Minimal work
            let _ = i.wrapping_add(1);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    start.elapsed()
}

// ============================================================================
// Work Stealing / Yield Benchmarks (Maximal Throughput)
// ============================================================================

/// Benchmark raw yield throughput - measures work stealing with zero CPU work
/// This spawns many tasks that immediately yield, forcing maximal work stealing
async fn benchmark_yield_throughput(num_tasks: usize, yields_per_task: usize) -> Duration {
    let counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let counter = counter.clone();
        let handle = task::spawn(async move {
            for _ in 0..yields_per_task {
                tokio::task::yield_now().await;
                counter.fetch_add(1, Ordering::Relaxed);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let duration = start.elapsed();
    duration
}

/// Benchmark yield throughput with cooperative tasks
/// Each task yields multiple times, showing how well the executor handles cooperative multitasking
async fn benchmark_cooperative_yield(num_tasks: usize, yields_per_task: usize) -> Duration {
    let start = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let handle = task::spawn(async move {
            for _ in 0..yields_per_task {
                tokio::task::yield_now().await;
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    start.elapsed()
}

/// Benchmark single long-running task with many yields
/// Tests yield overhead in a single task context
async fn benchmark_single_task_yield(total_yields: usize) -> Duration {
    let start = Instant::now();

    task::spawn(async move {
        for _ in 0..total_yields {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    start.elapsed()
}

fn print_yield_results(label: &str, total_ops: usize, duration: Duration) {
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / total_ops as f64;
    let ops_per_sec = 1_000_000_000.0 / ns_per_op;

    println!(
        "{:<40} {:>12} yields   {:>10.2} ms    {:>10.2} ns/yield   {:>15} yields/sec",
        label,
        humanize_number(total_ops as u64),
        duration.as_secs_f64() * 1000.0,
        ns_per_op,
        humanize_number(ops_per_sec as u64)
    );
}

// ============================================================================
// Print Helpers
// ============================================================================

fn print_cpu_results(
    label: &str,
    num_tasks: usize,
    work_per_task: u64,
    duration: Duration,
    _total: u64,
) {
    let total_work = num_tasks as u64 * work_per_task;
    let total_ns = duration.as_nanos();
    let ns_per_task = total_ns as f64 / num_tasks as f64;
    let tasks_per_sec = (1_000_000_000.0 / ns_per_task) * num_tasks as f64;

    println!(
        "{:<40} {:>12} tasks    {:>10.2} ms    {:>10.2} µs/task    {:>15} tasks/sec",
        label,
        humanize_number(num_tasks as u64),
        duration.as_secs_f64() * 1000.0,
        ns_per_task / 1000.0,
        humanize_number(tasks_per_sec as u64)
    );
}

fn print_io_results(label: &str, num_tasks: usize, duration: Duration) {
    let total_ns = duration.as_nanos();
    let ns_per_task = total_ns as f64 / num_tasks as f64;
    let tasks_per_sec = (1_000_000_000.0 / ns_per_task) * num_tasks as f64;

    println!(
        "{:<40} {:>12} tasks    {:>10.2} ms    {:>10.2} µs/task    {:>15} tasks/sec",
        label,
        humanize_number(num_tasks as u64),
        duration.as_secs_f64() * 1000.0,
        ns_per_task / 1000.0,
        humanize_number(tasks_per_sec as u64)
    );
}

fn print_throughput_results(label: &str, num_tasks: usize, duration: Duration) {
    let total_ns = duration.as_nanos();
    let ns_per_spawn = total_ns as f64 / num_tasks as f64;
    let spawns_per_sec = 1_000_000_000.0 / ns_per_spawn;

    println!(
        "{:<40} {:>12} spawns   {:>10.2} ms    {:>10.2} ns/spawn   {:>15} spawns/sec",
        label,
        humanize_number(num_tasks as u64),
        duration.as_secs_f64() * 1000.0,
        ns_per_spawn,
        humanize_number(spawns_per_sec as u64)
    );
}

// ============================================================================
// Main Benchmark Runner
// ============================================================================

fn create_runtime(worker_threads: usize, max_blocking_threads: usize) -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(max_blocking_threads)
        .enable_all()
        .build()
        .unwrap()
}

#[tokio::main]
async fn main() {
    println!("{}", "=".repeat(100));
    println!("Tokio Multi-Threaded Executor Benchmark Suite");
    println!("{}", "=".repeat(100));

    // Configuration
    let worker_threads = num_cpus::get();
    let max_blocking_threads = 512;

    println!("\nRuntime Configuration:");
    println!("  Worker threads (async): {}", worker_threads);
    println!("  Max blocking threads:   {}", max_blocking_threads);
    println!("  CPU cores detected:     {}", num_cpus::get());

    // ========================================================================
    // CPU-Bound Workloads
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("CPU-BOUND WORKLOADS");
    println!("{}", "=".repeat(100));
    println!("\nLight CPU work (1K iterations per task):");

    let (duration, total) = benchmark_cpu_async(100, 1_000).await;
    print_cpu_results("Async tasks", 100, 1_000, duration, total);

    let (duration, total) = benchmark_cpu_blocking(100, 1_000).await;
    print_cpu_results("Blocking tasks", 100, 1_000, duration, total);

    println!("\nModerate CPU work (100K iterations per task):");

    let (duration, total) = benchmark_cpu_async(100, 100_000).await;
    print_cpu_results("Async tasks", 100, 100_000, duration, total);

    let (duration, total) = benchmark_cpu_blocking(100, 100_000).await;
    print_cpu_results("Blocking tasks", 100, 100_000, duration, total);

    println!("\nHeavy CPU work (1M iterations per task):");

    let (duration, total) = benchmark_cpu_async(20, 1_000_000).await;
    print_cpu_results("Async tasks", 20, 1_000_000, duration, total);

    let (duration, total) = benchmark_cpu_blocking(20, 1_000_000).await;
    print_cpu_results("Blocking tasks", 20, 1_000_000, duration, total);

    // ========================================================================
    // I/O-Bound Workloads
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("I/O-BOUND WORKLOADS");
    println!("{}", "=".repeat(100));
    println!("\nShort sleep (1ms per task, 1000 tasks):");

    let duration = benchmark_io_async(1000, Duration::from_millis(1)).await;
    print_io_results("Async tasks", 1000, duration);

    let duration = benchmark_io_blocking(1000, Duration::from_millis(1)).await;
    print_io_results("Blocking tasks", 1000, duration);

    println!("\nMedium sleep (10ms per task, 500 tasks):");

    let duration = benchmark_io_async(500, Duration::from_millis(10)).await;
    print_io_results("Async tasks", 500, duration);

    let duration = benchmark_io_blocking(500, Duration::from_millis(10)).await;
    print_io_results("Blocking tasks", 500, duration);

    println!("\nHigh concurrency (1ms per task, 10000 tasks):");

    let duration = benchmark_io_async(10000, Duration::from_millis(1)).await;
    print_io_results("Async tasks", 10000, duration);

    let duration = benchmark_io_blocking(10000, Duration::from_millis(1)).await;
    print_io_results("Blocking tasks", 10000, duration);

    // ========================================================================
    // Mixed Workloads
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("MIXED WORKLOADS (CPU + I/O)");
    println!("{}", "=".repeat(100));
    println!("\nLight mix (10K CPU iterations + 1ms sleep, 100 tasks):");

    let (duration, total) = benchmark_mixed_async(100, 10_000, Duration::from_millis(1)).await;
    print_cpu_results("Async tasks", 100, 10_000, duration, total);

    let (duration, total) = benchmark_mixed_blocking(100, 10_000, Duration::from_millis(1)).await;
    print_cpu_results("Blocking tasks", 100, 10_000, duration, total);

    println!("\nModerate mix (100K CPU iterations + 5ms sleep, 50 tasks):");

    let (duration, total) = benchmark_mixed_async(50, 100_000, Duration::from_millis(5)).await;
    print_cpu_results("Async tasks", 50, 100_000, duration, total);

    let (duration, total) = benchmark_mixed_blocking(50, 100_000, Duration::from_millis(5)).await;
    print_cpu_results("Blocking tasks", 50, 100_000, duration, total);

    // ========================================================================
    // Task Spawning Throughput
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("TASK SPAWNING THROUGHPUT");
    println!("{}", "=".repeat(100));
    println!("\nMinimal work spawning:");

    let duration = benchmark_spawn_throughput_async(10_000).await;
    print_throughput_results("Async spawn", 10_000, duration);

    let duration = benchmark_spawn_throughput_blocking(10_000).await;
    print_throughput_results("Blocking spawn", 10_000, duration);

    println!("\nHigh volume spawning:");

    let duration = benchmark_spawn_throughput_async(100_000).await;
    print_throughput_results("Async spawn", 100_000, duration);

    let duration = benchmark_spawn_throughput_blocking(100_000).await;
    print_throughput_results("Blocking spawn", 100_000, duration);

    // ========================================================================
    // Work Stealing / Yield Throughput
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("WORK STEALING / YIELD THROUGHPUT (Zero CPU Work)");
    println!("{}", "=".repeat(100));
    println!("\nSingle task with many yields (baseline overhead):");

    let duration = benchmark_single_task_yield(100_000).await;
    print_yield_results("Single task", 100_000, duration);

    let duration = benchmark_single_task_yield(1_000_000).await;
    print_yield_results("Single task", 1_000_000, duration);

    println!("\nMultiple tasks, few yields (work stealing focus):");

    let duration = benchmark_cooperative_yield(1_000, 10).await;
    print_yield_results("1K tasks × 10 yields", 10_000, duration);

    let duration = benchmark_cooperative_yield(10_000, 10).await;
    print_yield_results("10K tasks × 10 yields", 100_000, duration);

    println!("\nMultiple tasks, many yields (maximal work stealing):");

    let duration = benchmark_cooperative_yield(100, 1_000).await;
    print_yield_results("100 tasks × 1K yields", 100_000, duration);

    let duration = benchmark_cooperative_yield(1_000, 1_000).await;
    print_yield_results("1K tasks × 1K yields", 1_000_000, duration);

    println!("\nHigh contention scenarios:");

    let duration = benchmark_yield_throughput(100, 10_000).await;
    print_yield_results("100 tasks × 10K yields", 1_000_000, duration);

    let duration = benchmark_yield_throughput(1_000, 10_000).await;
    print_yield_results("1K tasks × 10K yields", 10_000_000, duration);

    println!("\nExtreme work stealing test:");

    let duration = benchmark_cooperative_yield(10_000, 1_000).await;
    print_yield_results("10K tasks × 1K yields", 10_000_000, duration);

    // ========================================================================
    // Summary
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("BENCHMARK SUMMARY");
    println!("{}", "=".repeat(100));
    println!("\nKey Findings:");
    println!("  • Async tasks are ideal for I/O-bound workloads with high concurrency");
    println!("  • Blocking tasks better suited for CPU-intensive work that blocks the executor");
    println!("  • Async spawning has much lower overhead than spawn_blocking");
    println!("  • Yield throughput shows work stealing efficiency with zero CPU work");
    println!(
        "  • Single task yield establishes baseline ~{} ns/yield overhead",
        "varies by system"
    );
    println!("  • Multi-task yield demonstrates work stealing and context switch costs");
    println!("\nRecommendations:");
    println!("  • Use spawn_blocking for: file I/O, CPU work, synchronous APIs");
    println!("  • Use async tasks for: network I/O, timers, async APIs");
    println!("  • Use yield_now() for cooperative multitasking in long-running tasks");
    println!("{}", "=".repeat(100));
}
