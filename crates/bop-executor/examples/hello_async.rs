//! Hello World Async Example with async-task integration
//!
//! This is the simplest possible example showing how to:
//! 1. Create an async task using async-task
//! 2. Integrate it with a minimal executor
//! 3. Run the task and get the result

use std::sync::Arc;

use crossbeam_queue::SegQueue;

unsafe impl Send for HelloExecutor {}
unsafe impl Sync for HelloExecutor {}

/// Minimal executor that uses a simple queue
#[derive(Clone)]
pub struct HelloExecutor {
    ready_queue: Arc<SegQueue<async_task::Runnable>>,
}

impl HelloExecutor {
    pub fn new() -> Self {
        Self {
            ready_queue: Arc::new(SegQueue::new()),
        }
    }

    /// Spawn an async task
    pub fn spawn<F, T>(&self, future: F) -> async_task::Task<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let executor_ptr = self.clone();

        // Schedule function that gets called when task needs to wake up
        let schedule = move |runnable| {
            executor_ptr.ready_queue.push(runnable);
        };

        // Create the task
        let (runnable, task) = async_task::spawn(future, schedule);

        // Schedule the first execution
        runnable.schedule();

        task
    }

    /// Run until all tasks are complete
    pub fn run(&mut self) {
        while let Some(runnable) = self.ready_queue.pop() {
            runnable.run();
        }
    }
}

/// Simple async function
async fn hello_async(name: &str) -> String {
    println!("Hello, {}!", name);
    format!("Hello, {}!", name)
}

fn main() {
    println!("=== Hello World Async Example ===");

    let mut executor = HelloExecutor::new();

    // Spawn a simple hello task
    let task1 = executor.spawn(hello_async("World"));

    // Spawn another hello task
    let task2 = executor.spawn(hello_async("async-task"));

    // Run the executor
    executor.run();

    println!("All tasks completed!");

    // Note: In this simple example, we don't handle task completion
    // In a real implementation, you'd use JoinHandle to get results
    println!("Example finished!");
}
