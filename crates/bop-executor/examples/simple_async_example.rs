//! Simple example demonstrating async-task integration with bop-executor
//!
//! This example shows how to:
//! 1. Create async tasks using async-task
//! 2. Integrate them with the bop executor's slot/signal system
//! 3. Run and await task completion

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bop_executor::{SimpleAsyncExecutor, AsyncTask};

/// Simple counter that can be shared between tasks
struct SharedCounter {
    value: AtomicU64,
}

impl SharedCounter {
    fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    fn increment(&self) -> u64 {
        self.value.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Async function that simulates some work
async fn do_work(counter: Arc<SharedCounter>, task_id: u32, iterations: usize) -> u64 {
    println!("Task {} starting with {} iterations", task_id, iterations);

    for i in 0..iterations {
        // Simulate async work
        tokio::time::sleep(Duration::from_millis(1)).await;
        let current = counter.increment();

        if i % 10 == 0 {
            println!("Task {} iteration {}: counter = {}", task_id, i, current);
        }
    }

    println!("Task {} completed", task_id);
    counter.get()
}

/// Example that spawns multiple tasks and waits for them
fn main() {
    println!("=== Simple Async Task Example ===");

    // Create the async executor
    let executor = SimpleAsyncExecutor::new();
    let counter = Arc::new(SharedCounter::new());

    // Spawn several tasks
    let mut handles = Vec::new();

    // Task 1: Quick task
    let counter1 = Arc::clone(&counter);
    let task1 = executor.spawn(async move {
        do_work(counter1, 1, 25).await
    });
    handles.push(task1);

    // Task 2: Medium task
    let counter2 = Arc::clone(&counter);
    let task2 = executor.spawn(async move {
        do_work(counter2, 2, 50).await
    });
    handles.push(task2);

    // Task 3: Slower task
    let counter3 = Arc::clone(&counter);
    let task3 = executor.spawn(async move {
        do_work(counter3, 3, 75).await
    });
    handles.push(task3);

    // Convert tasks to join handles
    let join_handles: Vec<_> = handles.into_iter()
        .map(|task| task.detach())
        .collect();

    println!("Spawned {} tasks, running executor...", join_handles.len());

    // Note: In a real implementation, you'd need a proper runtime or
    // blocking executor to await these. For this simple example,
    // we'll just demonstrate the spawning mechanism.

    // For demonstration, we'll run a few iterations manually
    println!("Running executor for a few iterations...");
    for _ in 0..100 {
        if !executor.scheduler.try_execute_one() {
            std::thread::yield_now();
        }
    }

    println!("Final counter value: {}", counter.get());
    println!("Example completed!");
}

/// Alternative simple example with basic futures
fn simple_futures_example() {
    println!("\n=== Simple Futures Example ===");

    let executor = SimpleAsyncExecutor::new();

    // Create a simple future that returns a value
    let task = executor.spawn(async {
        println!("Simple future starting");
        std::thread::sleep(Duration::from_millis(10));
        println!("Simple future completed");
        42
    });

    // For this simple case, we'd need to implement proper waiting
    // in a real executor. Here we just show the structure.
    println!("Simple task spawned");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = SharedCounter::new();
        assert_eq!(counter.get(), 0);
        assert_eq!(counter.increment(), 1);
        assert_eq!(counter.increment(), 2);
        assert_eq!(counter.get(), 2);
    }

    #[test]
    fn test_executor_creation() {
        let executor = SimpleAsyncExecutor::new();
        // Basic test that we can create an executor
        assert!(true); // If we get here, creation worked
    }

    #[test]
    fn test_task_spawning() {
        let executor = SimpleAsyncExecutor::new();

        let task = executor.spawn(async {
            123
        });

        // Test that we can spawn tasks without panicking
        assert!(true);
    }
}
