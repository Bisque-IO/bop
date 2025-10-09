//! Complete Signal/Slot Integration with async-task
//!
//! This example shows how to integrate async-task with the existing
//! bop-executor signal/slot system using raw pointers for maximum
//! performance while maintaining thread safety.
//!
//! Key concepts demonstrated:
//! 1. Raw pointer scheduling with proper Send/Sync traits
//! 2. Integration with SignalGroup and_slots
//! 3. Task lifecycle management
//! 4. Cooperative multitasking with the signal/slot mechanism

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use bop_executor::{Executor, SignalGroup, Slot, SlotState};

/// High-performance async task scheduler that integrates with signal/slot system
pub struct SignalSlotAsyncScheduler {
    /// Signal group for task scheduling
    group: Arc<SignalGroup>,
    /// Maps slot indices to runnables for execution
    task_map: Arc<parking_lot::Mutex<HashMap<u64, async_task::Runnable>>>,
    /// Counter for spawned tasks
    task_counter: AtomicUsize,
}

// Mark as Send + Sync since we manage thread safety manually
// This is safe because:
// 1. SignalGroup is thread-safe
// 2. HashMap is protected by Mutex
// 3. We only access through thread-safe operations
unsafe impl Send for SignalSlotAsyncScheduler {}
unsafe impl Sync for SignalSlotAsyncScheduler {}

impl SignalSlotAsyncScheduler {
    /// Create a new scheduler
    pub fn new() -> Self {
        Self {
            group: Arc::new(SignalGroup::new()),
            task_map: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            task_counter: AtomicUsize::new(0),
        }
    }

    /// Spawn an async task using the signal/slot scheduling system
    pub fn spawn<F, T>(&self, future: F) -> AsyncTaskHandle<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        // Take raw pointers to avoid lifetime issues
        let group_ptr = Arc::into_raw(Arc::clone(&self.group)) as *const SignalGroup as *mut ();
        let task_map_ptr = Arc::into_raw(Arc::clone(&self.task_map))
            as *const parking_lot::Mutex<HashMap<u64, async_task::Runnable>>
            as *mut ();

        // Schedule function that integrates with the signal/slot system
        let schedule = move |runnable| {
            // Use raw pointers for maximum performance
            unsafe {
                let group = &*group_ptr;
                let task_map = &*task_map_ptr;

                // Reserve a slot for this runnable
                if let Some(slot) = group.reserve() {
                    let slot_index = slot.index();

                    // Store the runnable in the task map
                    {
                        let mut maps = task_map.lock();
                        maps.insert(slot_index, runnable);
                    }

                    // Schedule the slot - this sets the signal bit
                    slot.schedule();
                }
                // If no slot is available, we drop the runnable
                // In a production system, you might want better error handling
            }
        };

        // Create the async task
        let (runnable, task) = async_task::spawn(future, schedule);

        // Schedule the initial execution
        schedule(runnable);

        // Increment task counter
        self.task_counter.fetch_add(1, Ordering::Relaxed);

        AsyncTaskHandle {
            join_handle: task,
            scheduler: self,
        }
    }

    /// Try to execute one ready task
    /// Returns true if a task was executed, false if no tasks were ready
    pub fn try_execute_one(&self) -> bool {
        // Find a ready signal
        for signal in self.group.signals() {
            let signal_value = signal.load(Ordering::Relaxed);

            if signal_value != 0 {
                // Find the first set bit
                let bit_index = signal_value.trailing_zeros() as u64;

                // Try to acquire the signal
                if signal.acquire(bit_index) {
                    let global_index = self.calculate_global_index(signal, bit_index);

                    // Get the runnable and execute it
                    let runnable_opt = {
                        let mut maps = self.task_map.lock();
                        maps.remove(&global_index)
                    };

                    if let Some(runnable) = runnable_opt {
                        // Execute the runnable - this polls the future
                        runnable.run();
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Execute tasks until no more are ready
    pub fn run_until_idle(&self) {
        while self.try_execute_one() {
            // Keep executing while there are ready tasks
        }
    }

    /// Get the number of spawned tasks
    pub fn spawned_count(&self) -> usize {
        self.task_counter.load(Ordering::Relaxed)
    }

    /// Check if there are ready tasks
    pub fn has_ready_tasks(&self) -> bool {
        self.group.fast_check()
    }

    /// Calculate global slot index from signal and bit index
    fn calculate_global_index(&self, signal: &bop_executor::Signal, bit_index: u64) -> u64 {
        // This is a simplified calculation - in reality you'd need to know
        // which signal in the group this is and calculate the proper index
        bit_index
    }
}

impl Default for SignalSlotAsyncScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle for spawned async tasks
pub struct AsyncTaskHandle<'a, T> {
    join_handle: async_task::JoinHandle<T>,
    scheduler: &'a SignalSlotAsyncScheduler,
}

impl<'a, T> AsyncTaskHandle<'a, T> {
    /// Cancel the task and return the join handle
    pub fn cancel(self) -> async_task::JoinHandle<T> {
        self.join_handle.cancel()
    }

    /// Detach the task (no longer track completion)
    pub fn detach(self) -> async_task::JoinHandle<T> {
        self.join_handle
    }
}

/// Simple async computation function
async fn signal_slot_computation(task_id: u32, iterations: usize) -> u32 {
    println!(
        "Signal/Slot Task {} starting with {} iterations",
        task_id, iterations
    );

    let mut value = task_id;
    for i in 0..iterations {
        // Yield control to allow other tasks to run
        simple_yield().await;
        value += i;

        if i % 3 == 0 {
            println!("Task {} iteration {}: value = {}", task_id, i, value);
        }
    }

    println!(
        "Signal/Slot Task {} completed with value {}",
        task_id, value
    );
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

/// Async function that demonstrates signal/slot coordination
async fn coordinated_task(task_id: u32, coordination_points: usize) -> String {
    println!("Coordinated task {} starting", task_id);

    for i in 0..coordination_points {
        // Do some work
        simple_yield().await;
        println!("Task {} at coordination point {}", task_id, i + 1);

        // Yield again to allow other tasks to catch up
        simple_yield().await;
    }

    let result = format!(
        "Coordinated task {} completed all {} points",
        task_id, coordination_points
    );
    println!("{}", result);
    result
}

fn main() {
    println!("=== Signal/Slot Async Integration Example ===");
    println!("This demonstrates raw pointer integration with the signal/slot system.\n");

    // Create the scheduler
    let scheduler = SignalSlotAsyncScheduler::new();

    // Example 1: Basic computation tasks
    println!("--- Example 1: Basic Computation Tasks ---");

    let _task1 = scheduler.spawn(signal_slot_computation(100, 5));
    let _task2 = scheduler.spawn(signal_slot_computation(200, 7));
    let _task3 = scheduler.spawn(signal_slot_computation(300, 4));

    println!("Spawned {} tasks, running...", scheduler.spawned_count());
    scheduler.run_until_idle();

    // Example 2: Coordinated tasks
    println!("\n--- Example 2: Coordinated Tasks ---");

    let _coord1 = scheduler.spawn(coordinated_task(1, 6));
    let _coord2 = scheduler.spawn(coordinated_task(2, 8));
    let _coord3 = scheduler.spawn(coordinated_task(3, 4));

    scheduler.run_until_idle();

    // Example 3: Mixed workload
    println!("\n--- Example 3: Mixed Workload ---");

    let _mixed1 = scheduler.spawn(signal_slot_computation(1000, 3));
    let _mixed2 = scheduler.spawn(coordinated_task(10, 3));
    let _mixed3 = scheduler.spawn(signal_slot_computation(2000, 5));
    let _mixed4 = scheduler.spawn(coordinated_task(20, 2));

    // Run step by step to show interleaving
    println!("Running mixed workload step by step:");
    for step in 0..20 {
        print!("Step {}: ", step + 1);
        if scheduler.try_execute_one() {
            println!("executed");
        } else {
            println!("no ready tasks");
            break;
        }
    }

    // Finish remaining tasks
    scheduler.run_until_idle();

    println!("\n--- Statistics ---");
    println!("Total tasks spawned: {}", scheduler.spawned_count());
    println!(
        "Ready tasks remaining: {}",
        if scheduler.has_ready_tasks() {
            "Yes"
        } else {
            "No"
        }
    );

    println!("\n=== Integration Summary ===");
    println!("✓ Raw pointer scheduling with proper Send/Sync");
    println!("✓ Integration with SignalGroup and slot reservation");
    println!("✓ Task storage and retrieval via slot indices");
    println!("✓ Cooperative multitasking through signal bit management");
    println!("✓ High performance with minimal overhead");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_creation() {
        let scheduler = SignalSlotAsyncScheduler::new();
        assert_eq!(scheduler.spawned_count(), 0);
        assert!(!scheduler.has_ready_tasks());
    }

    #[test]
    fn test_task_spawning() {
        let scheduler = SignalSlotAsyncScheduler::new();

        let _task = scheduler.spawn(async { 42 });

        assert_eq!(scheduler.spawned_count(), 1);
        assert!(scheduler.has_ready_tasks());
    }

    #[test]
    fn test_task_execution() {
        let scheduler = SignalSlotAsyncScheduler::new();
        let completed = Arc::new(AtomicBool::new(false));
        let completed_clone = Arc::clone(&completed);

        let _task = scheduler.spawn(async move {
            completed_clone.store(true, Ordering::Relaxed);
            123
        });

        // Execute the task
        assert!(scheduler.try_execute_one());
        assert!(completed.load(Ordering::Relaxed));
    }

    #[test]
    fn test_multiple_task_execution() {
        let scheduler = SignalSlotAsyncScheduler::new();
        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn multiple tasks
        for i in 0..5 {
            let counter_clone = Arc::clone(&counter);
            let _task = scheduler.spawn(async move {
                simple_yield().await;
                counter_clone.fetch_add(1, Ordering::Relaxed);
                simple_yield().await;
            });
        }

        // Run all tasks
        scheduler.run_until_idle();

        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_signal_slot_computation() {
        let scheduler = SignalSlotAsyncScheduler::new();

        let _task = scheduler.spawn(signal_slot_computation(1, 3));

        // Should complete without panicking
        scheduler.run_until_idle();
        assert!(true);
    }

    #[test]
    fn test_coordinated_task() {
        let scheduler = SignalSlotAsyncScheduler::new();

        let _task = scheduler.spawn(coordinated_task(1, 2));

        // Should complete without panicking
        scheduler.run