use bop_executor::runtime::Runtime;
use bop_executor::task::{TaskArenaConfig, TaskArenaOptions};
use bop_executor::timer::Timer;
use futures_lite::future::block_on;
use num_cpus;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let workers = num_cpus::get().max(1);
    let futures = 128;
    let iterations = 16;

    let arena_config = TaskArenaConfig::new(4, 256)?;
    let runtime: Runtime<10, 6> =
        Runtime::new(arena_config, TaskArenaOptions::default(), workers.min(4))?;
    let operations = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();
    let mut handles = Vec::with_capacity(futures);
    for idx in 0..futures {
        let ops_counter = Arc::clone(&operations);
        handles.push(runtime.spawn(async move {
            let timer = Timer::new();
            for round in 0..iterations {
                let delay = Duration::from_millis(1 + (idx as u64 % 4) + (round as u64 % 5));
                timer.delay(delay).await;
            }
            ops_counter.fetch_add(iterations, Ordering::Relaxed);
        })?);
    }

    for handle in handles {
        block_on(handle);
    }

    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64().max(f64::EPSILON);
    let timer_ops = operations.load(Ordering::Relaxed);
    let tasks_per_sec = futures as f64 / elapsed_secs;
    let ops_per_sec = timer_ops as f64 / elapsed_secs;

    println!(
        "completed {} futures in {:?} ({:.2} futures/sec, ~{:.2} timer ops/sec, total timer ops={})",
        futures, elapsed, tasks_per_sec, ops_per_sec, timer_ops
    );
    println!(
        "active tasks after benchmark: {}",
        runtime.stats().active_tasks
    );

    Ok(())
}
