//! Multi-threaded benchmark for the Worker/Task executor built on the new runtime.
//!
//! Spawns a configurable number of cooperative tasks that repeatedly yield.
//! The runtime executes them on a configurable worker pool and we report simple
//! throughput statistics for each configuration.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures_lite::future::{block_on, yield_now};
use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
use maniac::runtime::Executor;

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
        worker_count: 16,
        total_tasks: 1024,
        yields_per_task: 10_000, // 51.2M yields total
    },
];

async fn yield_n_times(mut remaining: usize) {
    while remaining > 0 {
        yield_now().await;
        remaining -= 1;
    }
}

fn run_benchmark(config: &BenchmarkConfig) {
    println!(
        "> workers={} tasks={} yields/task={}",
        config.worker_count, config.total_tasks, config.yields_per_task
    );

    let arena_config =
        TaskArenaConfig::new(DEFAULT_LEAVES, DEFAULT_TASKS_PER_LEAF).expect("arena config");
    let runtime = maniac::runtime::new_multi_threaded(config.worker_count, (config.worker_count * 16) * 4096)
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
            yield_n_times(yields).await;
            counter.fetch_add(1, Ordering::Relaxed);
        };
        handles.push(runtime.spawn(task).expect("spawn task for benchmark"));
    }

    for (_, handle) in handles.into_iter().enumerate() {
        block_on(handle);
        // if (idx + 1) % 10 == 0 || idx + 1 == config.total_tasks {
        //     println!("    joined {}/{} tasks", idx + 1, config.total_tasks);
        // }
    }

    let elapsed = start.elapsed();
    let completed: usize = completion_counters.iter().map(|a| a.load(Ordering::Acquire)).sum();
    
    // Keep a reference to the service to collect stats after workers drop
    let service = std::sync::Arc::clone(runtime.service());
    drop(runtime); // This drops workers, which copy their stats to the service
    
    // Now collect stats after workers have exited
    let stats = service.aggregate_stats();
    summarize(config, elapsed, completed, stats);
    println!();
}

fn summarize(
    config: &BenchmarkConfig,
    duration: Duration,
    completed: usize,
    stats: maniac::runtime::WorkerServiceStats,
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
    println!();
    println!("    {}", stats);
}

fn main() {
    println!("Worker executor benchmark (multi-threaded).");
    for config in BENCHMARKS {
        run_benchmark(config);
    }
}
