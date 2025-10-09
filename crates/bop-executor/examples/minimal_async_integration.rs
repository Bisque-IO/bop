//! Minimal example: Integrating async-task with bop-executor
//!
//! This shows the absolute simplest way to integrate async-task
//! with a custom executor. We use basic types and focus on the
//! core concepts.


use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// Minimal task structure using async-task
type TaskOutput = i32;

/// Simple runnable wrapper
struct MinRunnable {
    runnable: async_task::Runnable,
}

/// Minimal executor using a simple queue
pub struct MinimalExecutor {
    ready_queue: VecDeque<async_task::Runnable>,
    task_count: AtomicUsize,
}

impl MinimalExecutor {
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
            task_count: AtomicUsize::new(0),
        }
    }

    /// Spawn an async task
    pub fn spawn<F>(&self, future: F) -> async_task::Task<TaskOutput>
    where
        F: std::future::Future<Output = TaskOutput> + Send + 'static,
    {
        let executor_ptr = self as *const MinimalExecutor as *mut ();

        // Schedule function that gets called when task needs to wake up
        let schedule = move |runnable| {
            // Store the runnable in our queue
            let executor = unsafe { &*executor_ptr };
            executor.ready_queue.push_back(runnable);
        };

        // Create the task
        let (runnable, task) = async_task::spawn(future, schedule);

        // Schedule the first execution
        schedule(runnable);

        self.task_count.fetch_add(1, Ordering::Relaxed);
        task
    }

    /// Try to execute one task
    pub fn try_execute_one(&self) -> bool {
        if let Some(runnable) = self.ready_queue.pop_front() {
            // This is where the task's future gets polled
            runnable.run();
            true
        } else {
            false
        }
    }

    /// Run until all tasks are complete
    pub fn run_until_idle(&self) {
        while !self.ready_queue.is_empty() {
            self.try_execute_one();
        }
    }

    pub fn pending_tasks(&self) -> usize {
        self.ready_queue.len()
    }
}

/// Simple async function for testing
async fn compute_value(start: i32, steps: usize) -> i32 {
    let mut value = start;

    for i in 0..steps {
        // Simulate async work
        yield_now().await;
        value += i as i32;
        println!("Step {}: value = {}", i, value);
    }

    value
}

/// Simple yield implementation
async fn yield_now() {
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

fn main() {
    println!("=== Minimal Async-Task Integration ===");

    let executor = MinimalExecutor::new();

    // Spawn a simple task
    println!("Spawning task...");
    let task = executor.spawn(async {
        println!("Task started!");
        let result = compute_value(10, 5).await;
        println!("Task finished with result: {}", result);
        result
    });

    println!("Task spawned. Running executor...");

    // Run the executor
    executor.run_until_idle();

    println!(
        "Executor finished. Pending tasks: {}",
        executor.pending_tasks()
    );

    // In a real implementation, you'd handle task completion differently
    // For this minimal example, we just show the spawning mechanism
    println!("Example completed!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        let executor = MinimalExecutor::new();
        assert_eq!(executor.pending_tasks(), 0);
    }

    #[test]
    fn test_task_spawning() {
        let executor = MinimalExecutor::new();

        let _task = executor.spawn(async { 42 });

        // Task should be in the queue
        assert_eq!(executor.pending_tasks(), 1);
    }

    #[test]
    fn test_task_execution() {
        let executor = MinimalExecutor::new();
        let completed = Arc::new(AtomicBool::new(false));
        let completed_clone = Arc::clone(&completed);

        let _task = executor.spawn(async move {
            completed_clone.store(true, Ordering::Relaxed);
            123
        });

        // Execute the task
        assert!(executor.try_execute_one());
        assert_eq!(executor.pending_tasks(), 0);
    }
}
