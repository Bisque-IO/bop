# Integrating async-task with bop-executor

This guide explains how to integrate the `async-task` crate with your custom `bop-executor` implementation. 

## Overview

The `async-task` crate is designed for custom executors and provides:
- Task scheduling and management
- Separation of futures from scheduling logic
- Integration with custom wakers
- Support for task pipes and advanced patterns

## Key Concepts

### 1. async_task::Task<T>
Represents a scheduled future that can be awaited or cancelled. The `T` is the output type.

### 2. async_task::Runnable
Contains the executable portion that actually polls the future. Your executor runs these.

### 3. Schedule Function
A callback that gets called when a task needs to be woken up. This is where you integrate with your executor's scheduling system.

### 4. Waker Integration
Tasks are woken up by calling your schedule function, which should re-schedule the task.

## Simple Integration Strategy

### Step 1: Basic Task Queue Executor

Here's the simplest possible integration using a queue:

```rust
use std::collections::VecDeque;

pub struct SimpleExecutor {
    ready_queue: VecDeque<async_task::Runnable>,
}

impl SimpleExecutor {
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }

    pub fn spawn<F, T>(&self, future: F) -> async_task::Task<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let executor_ptr = self as *const SimpleExecutor as *mut ();

        // Schedule function - called when task needs to wake up
        let schedule = move |runnable| {
            let executor = unsafe { &*executor_ptr };
            executor.ready_queue.push_back(runnable);
        };

        let (runnable, task) = async_task::spawn(future, schedule);
        schedule(runnable); // Schedule first execution
        task
    }

    pub fn try_execute_one(&mut self) -> bool {
        if let Some(runnable) = self.ready_queue.pop_front() {
            runnable.run(); // This polls the future
            true
        } else {
            false
        }
    }

    pub fn run_until_idle(&mut self) {
        while let Some(runnable) = self.ready_queue.pop_front() {
            runnable.run();
        }
    }
}
```

### Step 2: Integrating with bop-executor's Signal/Slot System

To integrate with your existing slot-based system:

```rust
use bop_executor::signal::SignalGroup;
use bop_executor::slot::Slot;

pub struct BopAsyncExecutor {
    group: Arc<SignalGroup>,
    // Map from slot index to task runnable
    task_slots: HashMap<u64, async_task::Runnable>,
}

impl BopAsyncExecutor {
    pub fn new() -> Self {
        Self {
            group: Arc::new(SignalGroup::new()),
            task_slots: HashMap::new(),
        }
    }

    pub fn spawn<F, T>(&mut self, future: F) -> async_task::Task<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let group = Arc::clone(&self.group);
        let slots = &mut self.task_slots;

        let schedule = move |runnable| {
            // Reserve a slot and store the runnable
            if let Some(slot) = group.reserve() {
                let slot_index = slot.index();
                
                // In a real implementation, you'd store the runnable
                // associated with this slot index
                // slots.insert(slot_index, runnable);
                
                slot.schedule(); // Set the signal bit
            }
        };

        let (runnable, task) = async_task::spawn(future, schedule);
        schedule(runnable);
        task
    }

    pub fn try_execute_one(&mut self) -> bool {
        // Find a set signal bit
        for signal in self.group.signals() {
            let value = signal.load(Ordering::Relaxed);
            if let Some(index) = find_first_set_bit(value) {
                if signal.acquire(index) {
                    // Get the runnable for this slot and execute it
                    if let Some(runnable) = self.task_slots.remove(&index) {
                        runnable.run();
                        return true;
                    }
                }
            }
        }
        false
    }
}

fn find_first_set_bit(value: u64) -> Option<u64> {
    if value == 0 {
        None
    } else {
        Some(value.trailing_zeros() as u64)
    }
}
```

## Complete Working Example

### Example 1: Hello World

```rust
async fn hello_world() -> String {
    println!("Hello from async task!");
    "Hello".to_string()
}

fn main() {
    let mut executor = SimpleExecutor::new();
    
    let task = executor.spawn(hello_world());
    executor.run_until_idle();
}
```

### Example 2: Multiple Tasks

```rust
async fn compute_value(id: u32, delay_ms: u64) -> u32 {
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    println!("Task {} completed", id);
    id * 2
}

fn main() {
    let mut executor = SimpleExecutor::new();
    
    // Spawn multiple tasks
    let task1 = executor.spawn(compute_value(1, 100));
    let task2 = executor.spawn(compute_value(2, 200));
    let task3 = executor.spawn(compute_value(3, 50));
    
    // Run until all tasks complete
    executor.run_until_idle();
}
```

### Example 3: Task with Async .await

```rust
async fn async_counter(start: i32, steps: usize) -> i32 {
    let mut value = start;
    
    for i in 0..steps {
        // Yield control to allow other tasks to run
        yield_now().await;
        value += i;
        println!("Step {}: value = {}", i, value);
    }
    
    value
}

// Simple yield implementation
async fn yield_now() {
    struct YieldNow { yielded: bool }
    
    impl std::future::Future for YieldNow {
        type Output = ();
        
        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>
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
```

## Integration Patterns

### 1. Race-Free Task Storage

For thread-safe task storage, use atomic operations or locking:

```rust
use std::sync::Mutex;
use std::collections::HashMap;

pub struct ThreadSafeExecutor {
    ready_queue: Mutex<VecDeque<async_task::Runnable>>,
}

impl ThreadSafeExecutor {
    pub fn spawn<F, T>(&self, future: F) -> async_task::Task<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let queue = &self.ready_queue;
        
        let schedule = move |runnable| {
            let mut queue = queue.lock().unwrap();
            queue.push_back(runnable);
        };
        
        let (runnable, task) = async_task::spawn(future, schedule);
        schedule(runnable);
        task
    }
}
```

### 2. Task Completion Handling

To handle task completion and get results:

```rust
pub struct TaskHandle<T> {
    join_handle: async_task::JoinHandle<T>,
}

impl<T> TaskHandle<T> {
    pub async fn await_result(self) -> Result<T, async_task::JoinError> {
        self.join_handle.await
    }
    
    pub fn cancel(self) -> async_task::JoinHandle<T> {
        self.join_handle
    }
}
```

### 3. Integration with Your Waker System

Your existing `Waker` can be used with async-task:

```rust
use bop_executor::waker::Waker;

pub struct WakerIntegratedExecutor {
    waker: Arc<Waker>,
    ready_queue: VecDeque<async_task::Runnable>,
}

impl WakerIntegratedExecutor {
    pub fn spawn<F, T>(&mut self, future: F) -> async_task::Task<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let waker = Arc::clone(&self.waker);
        let queue = &mut self.ready_queue;
        
        let schedule = move |runnable| {
            queue.push_back(runnable);
            waker.increment(); // Use your existing waker
        };
        
        let (runnable, task) = async_task::spawn(future, schedule);
        schedule(runnable);
        task
    }
}
```

## Best Practices

1. **Keep Schedule Function Simple**: The schedule function should be fast and non-blocking
2. **Thread Safety**: Use proper synchronization if your executor is multi-threaded
3. **Memory Management**: Be careful with lifetime management in schedule functions
4. **Error Handling**: Handle cases where task submission fails (e.g., out of slots)
5. **Fair Scheduling**: Consider implementing fair scheduling policies

## Performance Considerations

- Minimize allocations in the schedule function
- Use thread-local storage where possible for better cache locality
- Consider lock-free data structures for the ready queue
- Profile your integration to identify bottlenecks

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_spawning() {
        let executor = SimpleExecutor::new();
        let _task = executor.spawn(async { 42 });
        // Should not panic
    }
    
    #[test]
    fn test_task_execution() {
        let mut executor = SimpleExecutor::new();
        let completed = std::sync::atomic::AtomicBool::new(false);
        
        let _task = executor.spawn(async {
            completed.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        
        executor.run_until_idle();
        assert!(completed.load(std::sync::atomic::Ordering::Relaxed));
    }
}
```

## Resources

- [async-task crate documentation](https://docs.rs/async-task/)
- [async-task examples](https://github.com/smol-rs/async-task/tree/master/examples)
- [Your existing executor implementation](../crates/bop-executor/)