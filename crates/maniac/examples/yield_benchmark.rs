//! Multi-threaded benchmark for the Worker/Task executor built on the new runtime.
//!
//! Spawns a configurable number of cooperative tasks that repeatedly yield.
//! The runtime executes them on a configurable worker pool and we report simple
//! throughput statistics for each configuration.
//!
//! Includes a Tokio variant for side-by-side comparison.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures_lite::future::block_on;
use maniac::runtime::task::TaskArenaConfig;

const DEFAULT_LEAVES: usize = 64;
const DEFAULT_TASKS_PER_LEAF: usize = 4096;

#[derive(Debug)]
struct BenchmarkConfig {
    worker_count: usize,
    total_tasks: usize,
    yields_per_task: usize,
}

const BENCHMARKS: &[BenchmarkConfig] = &[
    // BenchmarkConfig {
    //     worker_count: 1,
    //     total_tasks: 128,
    //     yields_per_task: 100_000, // 12.8M yields total
    // },
    // BenchmarkConfig {
    //     worker_count: 2,
    //     total_tasks: 256,
    //     yields_per_task: 100_000, // 25.6M yields total
    // },
    // BenchmarkConfig {
    //     worker_count: 4,
    //     total_tasks: 512,
    //     yields_per_task: 100_000, // 51.2M yields total
    // },
    BenchmarkConfig {
        worker_count: 32,
        total_tasks: 1024,
        yields_per_task: 10_000, // 51.2M yields total
    },
];

async fn yield_n_times_maniac(mut remaining: usize) {
    while remaining > 0 {
        futures_lite::future::yield_now().await;
        remaining -= 1;
    }
}

async fn yield_n_times_tokio(mut remaining: usize) {
    while remaining > 0 {
        tokio::task::yield_now().await;
        // futures_lite::future::yield_now().await;
        remaining -= 1;
    }
}

fn run_maniac_benchmark(config: &BenchmarkConfig) {
    println!(
        "  [maniac] workers={} tasks={} yields/task={}",
        config.worker_count, config.total_tasks, config.yields_per_task
    );

    let _arena_config =
        TaskArenaConfig::new(DEFAULT_LEAVES, DEFAULT_TASKS_PER_LEAF).expect("arena config");
    let runtime =
        maniac::runtime::new_multi_threaded(config.worker_count, (config.worker_count * 8) * 4096)
            .expect("runtime");

    let mut completion_counters = Vec::with_capacity(config.total_tasks);
    for _ in 0..config.total_tasks {
        completion_counters.push(Arc::new(AtomicUsize::new(0)));
    }

    let start = Instant::now();
    let mut handles = Vec::with_capacity(config.total_tasks);
    for i in 0..config.total_tasks {
        let counter = Arc::clone(&completion_counters[i]);
        let yields = config.yields_per_task;
        let task = async move {
            yield_n_times_maniac(yields).await;
            counter.fetch_add(1, Ordering::Relaxed);
        };
        handles.push(runtime.spawn(task).expect("spawn task for benchmark"));
    }

    for handle in handles {
        block_on(handle);
    }

    let elapsed = start.elapsed();
    let completed: usize = completion_counters
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .sum();

    // Keep a reference to the service to collect stats after workers drop
    let service = std::sync::Arc::clone(runtime.service());
    drop(runtime); // This drops workers, which copy their stats to the service

    // Now collect stats after workers have exited
    let stats = service.aggregate_stats();
    summarize_maniac(config, elapsed, completed, Some(stats));
}

fn run_tokio_benchmark(config: &BenchmarkConfig) {
    println!(
        "  [tokio]  workers={} tasks={} yields/task={}",
        config.worker_count, config.total_tasks, config.yields_per_task
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(config.worker_count)
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut completion_counters = Vec::with_capacity(config.total_tasks);
    for _ in 0..config.total_tasks {
        completion_counters.push(Arc::new(AtomicUsize::new(0)));
    }

    let start = Instant::now();
    let mut handles = Vec::with_capacity(config.total_tasks);
    for i in 0..config.total_tasks {
        let counter = Arc::clone(&completion_counters[i]);
        let yields = config.yields_per_task;
        let task = async move {
            yield_n_times_tokio(yields).await;
            counter.fetch_add(1, Ordering::Relaxed);
        };
        handles.push(runtime.spawn(task));
    }

    runtime.block_on(async {
        for handle in handles {
            let _ = handle.await;
        }
    });

    let elapsed = start.elapsed();
    let completed: usize = completion_counters
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .sum();

    drop(runtime);

    summarize_maniac(config, elapsed, completed, None);
}

fn summarize_maniac(
    config: &BenchmarkConfig,
    duration: Duration,
    completed: usize,
    stats: Option<maniac::runtime::SchedulerStats>,
) {
    let duration_secs = duration.as_secs_f64().max(f64::EPSILON);
    let task_throughput = completed as f64 / duration_secs;

    let total_yields = (completed as u64).saturating_mul(config.yields_per_task as u64);
    let yield_throughput = total_yields as f64 / duration_secs;

    println!(
        "    Completed {:>6} tasks in {:?} (~{:.2} tasks/sec)",
        completed, duration, task_throughput
    );
    println!(
        "    Total yields: {} ({:.2}M yields/sec)",
        total_yields,
        yield_throughput / 1_000_000.0
    );
    if let Some(stats) = stats {
        println!();
        println!("    {}", stats);
    }
}

fn main() {
    println!("Worker executor benchmark (multi-threaded).");
    println!("=========================================\n");

    for config in BENCHMARKS {
        println!(
            "Config: workers={} tasks={} yields/task={}\n",
            config.worker_count, config.total_tasks, config.yields_per_task
        );

        run_tokio_benchmark(config);
        println!();
        run_maniac_benchmark(config);
        println!();

        println!("-----------------------------------------\n");
    }
}
