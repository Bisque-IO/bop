//! Example demonstrating preemptive generator switching
//!
//! This example shows how worker threads can be preempted via signals (Unix) or
//! thread suspension (Windows), causing the current generator to be pinned to
//! the running task. The worker then creates a new generator and continues.

use futures_lite::future::{block_on, poll_fn};
use maniac::runtime::Executor;
use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Preemptive Generator Switching Example");
    println!("========================================\n");
    println!("This example demonstrates timer-based preemption of workers.");
    println!("A separate thread will periodically INTERRUPT workers (not kill them!),");
    println!("causing their generators to be pinned and new generators to be created.");
    println!("The worker threads continue running - only the generator context switches.\n");

    // Create runtime with 2 workers
    let arena_config = TaskArenaConfig::new(4, 256)?;
    let runtime: Executor<10, 6> = Executor::new(arena_config, TaskArenaOptions::default(), 2, 2)?;

    // Get access to the worker service for interrupting workers
    let service = runtime.service();
    let service_clone = Arc::clone(&service);

    // Spawn a timer thread that will preemptively interrupt worker 0
    let preemption_interval = Duration::from_millis(100);
    let preemption_thread = std::thread::spawn(move || {
        for i in 0..10 {
            std::thread::sleep(preemption_interval);

            // Interrupt worker 0
            match service_clone.interrupt_worker(0) {
                Ok(true) => println!("[Preemption] Interrupted worker 0 at tick {}", i),
                Ok(false) => println!("[Preemption] Worker 0 not available at tick {}", i),
                Err(e) => eprintln!("[Preemption] Failed to interrupt worker 0: {}", e),
            }
        }
    });

    // Example 1: Long-running compute task that gets preempted
    println!("Example 1: CPU-bound task with preemption");
    let compute_counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = Arc::clone(&compute_counter);

    let handle = runtime.spawn(async move {
        println!("[Task] Starting CPU-bound work");

        // Simulate a long-running computation that periodically yields
        for iteration in 0..20 {
            // Do some "work"
            for _ in 0..100_000 {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            }

            // Yield to allow preemption to be checked
            poll_fn(|_cx| Poll::Ready(())).await;

            if iteration % 5 == 0 {
                println!("[Task] Iteration {} completed", iteration);
            }
        }

        println!("[Task] CPU-bound work completed");
        counter_clone.load(Ordering::Relaxed)
    })?;

    let result = block_on(handle);
    println!("Task completed {} increments\n", result);

    // Example 2: Multiple tasks that might get preempted
    println!("Example 2: Multiple concurrent tasks");
    let task_count = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for task_id in 0..4 {
        let count_clone = Arc::clone(&task_count);
        let handle = runtime.spawn(async move {
            println!("[Task {}] Started", task_id);

            for i in 0..10 {
                // Simulate some work
                std::thread::sleep(Duration::from_millis(50));
                count_clone.fetch_add(1, Ordering::Relaxed);

                // Yield periodically
                poll_fn(|_cx| Poll::Ready(())).await;

                if i % 3 == 0 {
                    println!("[Task {}] Progress: {}/10", task_id, i);
                }
            }

            println!("[Task {}] Completed", task_id);
        })?;
        handles.push(handle);
    }

    for handle in handles {
        block_on(handle);
    }

    println!(
        "All tasks completed {} work units\n",
        task_count.load(Ordering::Relaxed)
    );

    // Wait for preemption thread to finish
    preemption_thread
        .join()
        .expect("Preemption thread panicked");

    println!("Preemption example completed!");
    println!("Active tasks: {}", runtime.stats().active_tasks);

    Ok(())
}
