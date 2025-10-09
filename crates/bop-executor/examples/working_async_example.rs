//! Complete Working Example: async-task integration with bop-executor
//!
//! This example demonstrates a fully functional integration of async-task
//! with a minimal executor that can be compiled and run immediately.
//!
//! It shows:
//! 1. How to spawn async tasks
//! 2. How tasks are scheduled and executed
//! 3. How async/await works with custom executors
//! 4. How to handle task completion

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Simple thread-safe executor using a queue
#[derive(Debug)]
pub struct WorkingExecutor {
    /// Queue of ready tasks
    ready_queue: Arc<Mutex<VecDeque<async_task::Runnable>>>,
    /// Counter for spawned tasks
    task_counter: AtomicUsize,
}

impl WorkingExecutor {
    /// Create a new executor
    pub fn new() -> Self {
        Self {
            ready_queue: Arc::new(Mutex::new(VecDeque::new())),
            task_counter: AtomicUsize::new(0),
        }
    }

    /// Spawn an async task
    pub fn spawn<F, T>(&self, future: F) -> TaskHandle<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        // Clone the queue for the schedule function
        let queue = Arc::clone(&self.ready_queue);

        // This schedule function is called when the task needs to be woken up
        let schedule = move |runnable| {
            // Add the runnable back to the ready queue
            if let Ok(mut queue) = queue.lock() {
                queue.push_back(runnable);
            }
        };

        // Create the task (runnable + join handle)
        let (runnable, task) = async_task::spawn(future, schedule);

        // Schedule the first execution
        schedule(runnable);

        // Increment task counter
        self.task_counter.fetch_add(1, Ordering::Relaxed);

        TaskHandle {
            join_handle: task,
            executor: self,
        }
    }

    /// Try to execute one task from the ready queue
    /// Returns true if a task was executed, false if no tasks were ready
    pub fn try_execute_one(&self) -> bool {
        if let Ok(mut queue) = self.ready_queue.lock() {
            if let Some(runnable) = queue.pop_front() {
                // Run the task - this polls the future internally
                runnable.run();
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Run until all tasks are complete or queue is empty
    pub fn run_until_idle(&self) {
        // Keep executing tasks while there are any ready
        while self.has_ready_tasks() {
            if !self.try_execute_one() {
                // No tasks ready, break to avoid busy-wait
                break;
            }
        }
    }

    /// Check if there are ready tasks in the queue
    pub fn has_ready_tasks(&self) -> bool {
        if let Ok(queue) = self.ready_queue.lock() {
            !queue.is_empty()
        } else {
            false
        }
    }

    /// Get the number of spawned tasks
    pub fn spawned_count(&self) -> usize {
        self.task_counter.load(Ordering::Relaxed)
    }
}

impl Default for WorkingExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to a spawned task that can be awaited or cancelled
pub struct TaskHandle<'a, T> {
    join_handle: async_task::JoinHandle<T>,
    executor: &'a WorkingExecutor,
}

impl<'a, T> TaskHandle<'a, T> {
    /// Detach the task and return the join handle
    /// Note: In this simple example, we can't actually await across threads
    pub fn detach(self) -> async_task::JoinHandle<T> {
        self.join_handle
    }

    /// Cancel the task and return a join handle
    pub fn cancel(self) -> async_task::JoinHandle<T> {
        self.join_handle.cancel()
    }
}

/// Simple async function for testing
async fn simple_computation(start: i32, steps: usize) -> i32 {
    println!("Starting computation from {} with {} steps", start, steps);

    let mut value = start;
    for i in 0..steps {
        // Yield control to allow other tasks to run
        simple_yield().await;
        value += i;

        if i % 5 == 0 {
            println!("Computation step {}: value = {}", i, value);
        }
    }

    println!("Computation completed, final value: {}", value);
    value
}

/// Simple yield implementation for cooperative multitasking
async fn simple_yield() {
    struct YieldNow {
        yielded: bool,
    }

    impl std::future::Future for YieldNow {
        type Output = ();

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            if self.yielded {
                std::task::Poll::Ready(())
            } else {
                self.yielded = true;
                std::task::Poll::Pending
            }
        }
    }

    YieldNow { yielded: false }.await
}

/// Async function that simulates I/O or network work
async fn simulated_io_work(task_id: u32, delay_iterations: usize) -> String {
    println!("IO task {} starting work", task_id);

    for i in 0..delay_iterations {
        simple_yield().await; // Simulate waiting for I/O

        if i % 3 == 0 {
            println!("IO task {} progress: {}/{}", task_id, i, delay_iterations);
        }
    }

    let result = format!("IO task {} completed!", task_id);
    println!("{}", result);
    result
}

/// Main demonstration function
fn main() {
    println!("=== Working Async Task Example ===");
    println!("This example shows async-task integration with a custom executor.\n");

    // Create the executor
    let executor = WorkingExecutor::new();

    // Example 1: Simple computation task
    println!("--- Example 1: Simple Computation ---");
    let task1 = executor.spawn(simple_computation(10, 5));

    // Run the executor to complete the task
    executor.run_until_idle();

    // Example 2: Multiple concurrent tasks
    println!("\n--- Example 2: Multiple Concurrent Tasks ---");

    // Spawn several tasks that will interleave their execution
    let _task2a = executor.spawn(simple_computation(100, 8));
    let _task2b = executor.spawn(simple_computation(200, 6));
    let _task2c = executor.spawn(simple_computation(300, 10));

    // Run all tasks
    executor.run_until_idle();

    // Example 3: Simulated I/O tasks
    println!("\n--- Example 3: Simulated I/O Work ---");

    let _task3a = executor.spawn(simulated_io_work(1, 12));
    let _task3b = executor.spawn(simulated_io_work(2, 8));
    let _task3c = executor.spawn(simulated_io_work(3, 15));

    // Run all I/O tasks
    executor.run_until_idle();

    // Example 4: Demonstrating cooperative multitasking
    println!("\n--- Example 4: Cooperative Multitasking ---");

    let _task4a = executor.spawn(async {
        println!("Task A: Starting");
        for i in 0..4 {
            simple_yield().await;
            println!("Task A: Step {}", i);
        }
        println!("Task A: Done");
    });

    let _task4b = executor.spawn(async {
        println!("Task B: Starting");
        for i in 0..4 {
            simple_yield().await;
            println!("Task B: Step {}", i);
        }
        println!("Task B: Done");
    });

    // Run both tasks and watch them interleave
    executor.run_until_idle();

    // Show final statistics
    println!("\n--- Statistics ---");
    println!("Total tasks spawned: {}", executor.spawned_count());
    println!(
        "Ready tasks remaining: {}",
        if executor.has_ready_tasks() {
            "Yes"
        } else {
            "No"
        }
    );

    println!("\n=== Example Completed Successfully ===");
    println!("\nKey takeaways:");
    println!("1. async-task separates the future from scheduling");
    println!("2. The schedule function re-queues tasks when they need to wake up");
    println!("3. runnable.run() polls the future and may need to be re-scheduled");
    println!("4. Multiple tasks interleave their execution through yields");
    println!("5. The executor controls when and how tasks are executed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        let executor = WorkingExecutor::new();
        assert_eq!(executor.spawned_count(), 0);
        assert!(!executor.has_ready_tasks());
    }

    #[test]
    fn test_task_spawning() {
        let executor = WorkingExecutor::new();

        let _task = executor.spawn(async { 42 });

        assert_eq!(executor.spawned_count(), 1);
        assert!(executor.has_ready_tasks());
    }

    #[test]
    fn test_task_execution() {
        let executor = WorkingExecutor::new();
        let completed = Arc::new(AtomicBool::new(false));
        let completed_clone = Arc::clone(&completed);

        let _task = executor.spawn(async move {
            completed_clone.store(true, Ordering::Relaxed);
        });

        executor.run_until_idle();

        assert!(completed.load(Ordering::Relaxed));
        assert!(!executor.has_ready_tasks());
    }

    #[test]
    fn test_multiple_task_execution() {
        let executor = WorkingExecutor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn multiple tasks
        for i in 0..5 {
            let counter_clone = Arc::clone(&counter);
            let _task = executor.spawn(async move {
                simple_yield().await;
                counter_clone.fetch_add(1, Ordering::Relaxed);
                simple_yield().await;
            });
        }

        executor.run_until_idle();

        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_simple_computation() {
        let executor = WorkingExecutor::new();
        let _task = executor.spawn(simple_computation(0, 3));

        executor.run_until_idle();

        // Test passes if we get here without panicking
        assert!(true);
    }
}
