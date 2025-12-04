//! Example demonstrating generator pinning in the maniac-runtime
//!
//! This example shows how tasks can pin the Worker's current generator
//! to get dedicated stackful coroutine execution.

use futures_lite::future::{block_on, poll_fn};
use maniac::runtime::Executor;
use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
use maniac::runtime::worker::as_coroutine;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Generator Pinning Example");
    println!("==========================\n");
    println!("This example demonstrates pinning the Worker's generator to a task.");
    println!("When a task calls pin_current_generator(), it takes ownership of");
    println!("the generator it's currently running in.\n");

    // Create runtime with 1 worker
    let arena_config = TaskArenaConfig::new(4, 256)?;
    let runtime: Executor<10, 6> = Executor::new(arena_config, TaskArenaOptions::default(), 1, 1)?;

    // Example 1: Task that pins the current generator
    println!("Example 1: Pin current generator");
    let handle = runtime.spawn(async {
        println!("[Task] Task started inside Worker's generator");

        // Pin the current generator (the one Worker is running in)
        if as_coroutine() {
            println!("[Task] Generator pin requested!");
            println!("[Task] Worker will transfer this generator to us");
        } else {
            println!("[Task] Failed to request pin (not in worker context?)");
        }

        // After returning, the Worker will:
        // 1. Detect the pin request
        // 2. Transfer the generator to this task
        // 3. Create a new generator for itself
        // 4. Continue running

        println!("[Task] Task completing");
    })?;

    block_on(handle);
    println!("Example 1 completed\n");

    // Example 2: Multiple tasks requesting generator pinning
    println!("Example 2: Multiple tasks with pinned generators");
    let counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for task_id in 0..3 {
        let counter_clone = Arc::clone(&counter);
        let handle = runtime.spawn(async move {
            println!("[Task {}] Starting", task_id);

            // Request to pin the current generator
            if as_coroutine() {
                println!("[Task {}] Generator pinned!", task_id);
                counter_clone.fetch_add(1, Ordering::Relaxed);
            }

            // Do some work in multiple poll cycles
            for i in 0..3 {
                poll_fn(|_cx| {
                    println!("[Task {}] Poll iteration {}", task_id, i);
                    Poll::Ready(())
                })
                .await;
            }

            println!("[Task {}] Completed", task_id);
        })?;
        handles.push(handle);
    }

    for handle in handles {
        block_on(handle);
    }

    println!(
        "\nSuccessfully pinned {} generators",
        counter.load(Ordering::Relaxed)
    );
    println!("Example 2 completed\n");

    println!("All examples completed successfully!");
    println!("Active tasks: {}", runtime.stats().active_tasks);

    Ok(())
}
