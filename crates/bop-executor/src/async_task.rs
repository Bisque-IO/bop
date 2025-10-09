// //! Simple async-task integration with the bop executor
// //!
// //! This module provides a straightforward way to integrate async-task
// //! with the existing slot/signal-based executor.

// use std::sync::Arc;
// use std::sync::atomic::{AtomicUsize, Ordering};

// use crate::signal::SignalGroup;
// use crate::slot::{Slot, SlotState};

// /// Simple task handle that represents a scheduled async task
// pub struct AsyncTask<T> {
//     task: async_task::Task<T>,
// }

// /// Simple runnable wrapper that stores the async-task runnable in a slot
// struct AsyncRunnable {
//     runnable: Option<async_task::Runnable>,
// }

// /// Simple scheduler that integrates with our signal/slot system
// pub struct AsyncTaskScheduler {
//     group: Arc<SignalGroup>,
//     task_count: AtomicUsize,
// }

// impl AsyncTaskScheduler {
//     pub fn new(group: Arc<SignalGroup>) -> Self {
//         Self {
//             group,
//             task_count: AtomicUsize::new(0),
//         }
//     }

//     /// Schedule an async task by reserving a slot and storing the runnable
//     pub fn schedule<F, T>(&self, future: F) -> AsyncTask<T>
//     where
//         F: std::future::Future<Output = T> + Send + 'static,
//         T: Send + 'static,
//     {
//         // Reserve a slot for this task
//         let slot = match self.group.reserve() {
//             Some(slot) => slot,
//             None => panic!("Executor is out of slots"),
//         };

//         // Create the schedule function that will be called when the task needs to wake up
//         let group = Arc::clone(&self.group);
//         let schedule = move |runnable| {
//             // Store the runnable and schedule it for execution
//             let slot = match group.reserve() {
//                 Some(slot) => slot,
//                 None => return,
//             };

//             slot.schedule();
//         };

//         // Create the async task
//         let (runnable, task) = async_task::spawn(future, schedule);

//         // Schedule the first execution
//         slot.schedule();

//         self.task_count.fetch_add(1, Ordering::Relaxed);

//         AsyncTask { task }
//     }

//     /// Try to execute one runnable from the ready queue
//     pub fn try_execute_one(&self) -> bool {
//         // For this simple example, we'll just try to find any scheduled slot
//         for signal in self.group.signals() {
//             if let Some(index) =
//                 find_first_set_bit(signal.load(std::sync::atomic::Ordering::Relaxed))
//             {
//                 if signal.acquire(index) {
//                     // In a real implementation, you'd look up the slot for this index
//                     // and execute its runnable
//                     return true;
//                 }
//             }
//         }
//         false
//     }

//     pub fn task_count(&self) -> usize {
//         self.task_count.load(Ordering::Relaxed)
//     }
// }

// unsafe impl Send for AsyncRunnable {}
// unsafe impl Sync for AsyncRunnable {}

// impl<T> AsyncTask<T> {
//     /// Cancel the task and get a JoinHandle
//     pub fn cancel(self) -> async_task::JoinHandle<T> {
//         self.task.cancel()
//     }

//     /// Get the JoinHandle to await the task's completion
//     pub fn detach(self) -> async_task::JoinHandle<T> {
//         self.task
//     }
// }

// // Helper function to find first set bit
// fn find_first_set_bit(value: u64) -> Option<u64> {
//     if value == 0 {
//         None
//     } else {
//         Some(value.trailing_zeros() as u64)
//     }
// }

// /// Simple executor that combines the basic executor with async task support
// pub struct SimpleAsyncExecutor {
//     scheduler: AsyncTaskScheduler,
// }

// impl SimpleAsyncExecutor {
//     pub fn new() -> Self {
//         let group = Arc::new(SignalGroup::new());
//         Self {
//             scheduler: AsyncTaskScheduler::new(group),
//         }
//     }

//     /// Spawn an async task
//     pub fn spawn<F, T>(&self, future: F) -> AsyncTask<T>
//     where
//         F: std::future::Future<Output = T> + Send + 'static,
//         T: Send + 'static,
//     {
//         self.scheduler.schedule(future)
//     }

//     /// Run the executor until all tasks are complete
//     pub fn run_until_complete(&self) {
//         while self.scheduler.task_count() > 0 {
//             if !self.scheduler.try_execute_one() {
//                 // No ready tasks, could block or yield here
//                 std::thread::yield_now();
//             }
//         }
//     }
// }

// impl Default for SimpleAsyncExecutor {
//     fn default() -> Self {
//         Self::new()
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_create_executor() {
//         let executor = SimpleAsyncExecutor::new();
//         // Test that we can create an executor without panicking
//         assert!(true);
//     }
// }
