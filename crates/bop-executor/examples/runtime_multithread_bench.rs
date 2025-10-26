use bop_executor::runtime::Runtime;
use bop_executor::task::{ArenaConfig, ArenaOptions};
use bop_executor::timer::Timer;
use futures_lite::future::{block_on, poll_fn};
use futures_lite::pin;
use num_cpus;
use std::env;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;
use std::time::{Duration, Instant};

/// Configuration for the runtime benchmark.
#[derive(Debug)]
struct BenchOptions {
    workers: usize,
    leaf_count: usize,
    tasks_per_leaf: usize,
    futures: usize,
    iterations: usize,
    base_delay_micros: u64,
}

impl BenchOptions {
    fn from_args() -> Self {
        let mut opts = Self {
            workers: num_cpus::get().max(1),
            leaf_count: 4,
            tasks_per_leaf: 256,
            futures: 1024,
            iterations: 8,
            base_delay_micros: 50,
        };

        for arg in env::args().skip(1) {
            let Some((flag, value)) = arg.split_once('=') else {
                continue;
            };
            match flag {
                "--workers" => {
                    opts.workers = value
                        .parse()
                        .ok()
                        .filter(|v| *v > 0)
                        .unwrap_or(opts.workers);
                }
                "--leafs" | "--leaves" => {
                    opts.leaf_count = value
                        .parse()
                        .ok()
                        .filter(|v| *v > 0)
                        .unwrap_or(opts.leaf_count);
                }
                "--tasks-per-leaf" => {
                    opts.tasks_per_leaf = value
                        .parse()
                        .ok()
                        .filter(|v| *v > 0)
                        .unwrap_or(opts.tasks_per_leaf);
                }
                "--futures" => {
                    opts.futures = value
                        .parse()
                        .ok()
                        .filter(|v| *v > 0)
                        .unwrap_or(opts.futures);
                }
                "--iterations" => {
                    opts.iterations = value
                        .parse()
                        .ok()
                        .filter(|v| *v > 0)
                        .unwrap_or(opts.iterations);
                }
                "--base-delay-us" => {
                    opts.base_delay_micros = value
                        .parse()
                        .ok()
                        .filter(|v| *v > 0)
                        .unwrap_or(opts.base_delay_micros);
                }
                _ => {}
            }
        }

        opts
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let opts = BenchOptions::from_args();
    let worker_capacity = opts.leaf_count.max(1);
    let worker_count = opts.workers.min(worker_capacity);
    println!(
        "runtime multithread benchmark: requested_workers={} actual_workers={} futures={} iterations={} leafs={} tasks_per_leaf={}",
        opts.workers,
        worker_count,
        opts.futures,
        opts.iterations,
        opts.leaf_count,
        opts.tasks_per_leaf
    );

    let arena_config = ArenaConfig::new(opts.leaf_count, opts.tasks_per_leaf)?;
    let runtime = Runtime::new(arena_config, ArenaOptions::default(), worker_count)?;
    let operations = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();
    let mut handles = Vec::with_capacity(opts.futures);
    handles.push(
        runtime
            .spawn(async move {
                let timer = Timer::new();
                for _ in 0..100 {
                    timer.delay(Duration::from_millis(500)).await;
                    println!("{}", Instant::now().elapsed().as_millis());
                }
            })
            .expect("spawn task into runtime"),
    );
    for idx in 0..opts.futures {
        let ops_counter = Arc::clone(&operations);
        let iterations = opts.iterations;
        let base_delay = opts.base_delay_micros;
        handles.push(
            runtime
                .spawn(async move {
                    let ops = match idx % 3 {
                        0 => timer_sleep_job(iterations, base_delay).await,
                        1 => timer_reschedule_job(iterations, base_delay).await,
                        _ => timer_cancel_job(iterations, base_delay).await,
                    };
                    ops_counter.fetch_add(ops, Ordering::Relaxed);
                })
                .expect("spawn task into runtime"),
        );
    }

    for handle in handles {
        block_on(handle);
    }
    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64().max(f64::EPSILON);
    let timer_ops = operations.load(Ordering::Relaxed);
    let tasks_per_sec = opts.futures as f64 / elapsed_secs;
    let ops_per_sec = timer_ops as f64 / elapsed_secs;

    println!(
        "completed {} futures in {:?} ({:.2} futures/sec, ~{:.2} timer ops/sec, total timer ops={})",
        opts.futures, elapsed, tasks_per_sec, ops_per_sec, timer_ops
    );
    println!(
        "active tasks after benchmark: {}",
        runtime.stats().active_tasks
    );

    drop(runtime);
    Ok(())
}

async fn timer_sleep_job(iterations: usize, base_delay_micros: u64) -> usize {
    let timer = Timer::new();
    for round in 0..iterations {
        let delay = base_delay_micros * (1 + (round % 5) as u64);
        timer.delay(Duration::from_micros(delay)).await;
    }
    iterations
}

async fn timer_reschedule_job(iterations: usize, base_delay_micros: u64) -> usize {
    let timer = Timer::new();
    for round in 0..iterations {
        let first = base_delay_micros * (1 + (round % 3) as u64);
        let second = base_delay_micros * (2 + (round % 4) as u64);
        timer.delay(Duration::from_micros(first)).await;
        timer.delay(Duration::from_micros(second)).await;
    }
    iterations * 2
}

async fn timer_cancel_job(iterations: usize, base_delay_micros: u64) -> usize {
    let timer = Timer::new();
    for round in 0..iterations {
        let delay = base_delay_micros * (1 + (round % 7) as u64);
        let delay_future = timer.delay(Duration::from_micros(delay));
        pin!(delay_future);
        poll_fn(|cx| match delay_future.as_mut().poll(cx) {
            Poll::Pending => {
                timer.cancel();
                Poll::Ready(())
            }
            Poll::Ready(()) => Poll::Ready(()),
        })
        .await;
    }
    iterations
}
