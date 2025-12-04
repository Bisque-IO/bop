//! Executor module providing a cooperative async executor.
//!
//! This module implements a high-performance async executor that uses memory-mapped
//! task arenas and worker threads for efficient task scheduling and execution.
//!
//! # Architecture
//!
//! The executor consists of:
//! - `Executor`: Public API for spawning and managing async tasks
//! - `ExecutorInner`: Internal shared state for task spawning
//! - `WorkerService`: Manages worker threads that execute tasks
//! - `TaskArena`: Memory-mapped arena for task storage
//! - `JoinHandle`: Future that resolves when a spawned task completes
//!
pub mod deque;
pub mod mpsc;
pub mod preemption;
pub mod task;
pub mod timer;
pub mod worker;

pub use crate::{join, select, try_join};

use std::future::{Future, IntoFuture};

pub use crate::driver::Driver;
#[cfg(all(target_os = "linux", feature = "iouring"))]
pub use crate::driver::IoUringDriver;
#[cfg(feature = "poll")]
pub use crate::driver::PollerDriver;
pub use worker::io::{Buildable, RuntimeBuilder};

// pub use runtime::{spawn, Runtime};
pub use worker::io::IoRuntime;
#[cfg(any(all(target_os = "linux", feature = "iouring"), feature = "poll"))]
pub use worker::io::{FusionDriver, FusionRuntime};

/// A specialized `Result` type for `io-uring` operations with buffers.
pub use crate::buf::BufResult;

use std::any::TypeId;

use parking_lot::Mutex;
use std::io;
use std::panic::Location;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;
use task::{SpawnError, TaskArena, TaskArenaConfig, TaskArenaOptions, TaskArenaStats, TaskHandle};
use timer::ticker::TickService;
use worker::{Scheduler, SchedulerConfig};

use crate::{num_cpus, runtime::worker::JoinHandle};

pub use crate::blocking::unblock;
pub use worker::{SchedulerStats, WorkerSnapshot, WorkerStats};

pub fn new_single_threaded() -> io::Result<DefaultRuntime> {
    let tick_service = TickService::new(Duration::from_millis(1));
    tick_service.start();
    Ok(DefaultRuntime {
        executor: DefaultExecutor::new_with_tick_service(
            TaskArenaConfig::new(8, 4096).unwrap(),
            TaskArenaOptions::new(false, false),
            1,
            Arc::clone(&tick_service),
        )?,
        tick_service,
    })
}

pub fn new_multi_threaded(
    mut worker_count: usize,
    mut max_tasks: usize,
) -> io::Result<DefaultRuntime> {
    worker_count = worker_count.max(0);
    if worker_count == 0 {
        worker_count = crate::utils::cpu_cores().len();
        if worker_count > 1 {
            worker_count = worker_count - 1;
        }
    }
    if max_tasks == 0 {
        max_tasks = worker_count * 4096;
    }
    max_tasks = max_tasks.next_power_of_two();
    let config = ExecutorConfig::with_max_tasks(worker_count, max_tasks)?;

    let tick_service = TickService::new(Duration::from_millis(1));
    tick_service.start();

    Ok(DefaultRuntime {
        executor: DefaultExecutor::new_with_tick_service(
            config.task_arena,
            config.task_arena_options,
            config.worker_count,
            Arc::clone(&tick_service),
        )?,
        tick_service,
    })
}

pub struct Runtime {
    executor: Executor,
    tick_service: Arc<TickService>,
}

pub type DefaultRuntime = Runtime;

impl Runtime {
    /// Returns a reference to the underlying worker service.
    ///
    /// This allows access to worker service operations and statistics.
    pub fn service(&self) -> &Arc<Scheduler> {
        self.executor.service()
    }

    /// Spawns an asynchronous task on the executor, returning an awaitable join handle.
    ///
    /// This is the primary method for scheduling async work on the executor.
    /// The spawned task will be executed by one of the executor's worker threads.
    ///
    /// # Arguments
    ///
    /// * `future`: The future to execute. Can be any type that implements `IntoFuture`.
    ///
    /// # Returns
    ///
    /// * `Ok(JoinHandle<T>)`: A join handle that implements `Future<Output = T>`
    /// * `Err(SpawnError)`: Error if spawning fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use maniac::runtime::Executor;
    /// let rt = manic::runtime::new_multi_threaded();
    /// let handle = rt.spawn(async {
    ///     // Your async code here
    ///     42
    /// })?;
    ///
    /// // Await the result
    /// // let result = handle.await;
    /// # Ok::<(), maniac::runtime::task::SpawnError>(())
    /// ```
    #[track_caller]
    pub fn spawn<F, T>(&self, future: F) -> Result<JoinHandle<T>, SpawnError>
    where
        F: IntoFuture<Output = T> + Send + 'static,
        F::IntoFuture: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let location = Location::caller();
        let type_id = TypeId::of::<F>();
        self.executor.inner.service.spawn(type_id, location, future)
    }

    /// Spawns a blocking task on the blocking executor, returning an awaitable join handle.
    ///
    /// This is the primary method for scheduling async work on the executor.
    /// The spawned task will be executed by one of the executor's worker threads.
    ///
    /// # Arguments
    ///
    /// * `future`: The future to execute. Can be any type that implements `IntoFuture`.
    ///
    /// # Returns
    ///
    /// * `Ok(JoinHandle<T>)`: A join handle that implements `Future<Output = T>`
    /// * `Err(SpawnError)`: Error if spawning fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use maniac::runtime::Executor;
    /// # use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
    /// # let executor = Executor::new(
    /// #     TaskArenaConfig::new(1, 8).unwrap(),
    /// #     TaskArenaOptions::default(),
    /// #     1,
    /// # ).unwrap();
    /// let handle = executor.spawn(async {
    ///     // Your async code here
    ///     42
    /// })?;
    ///
    /// // Await the result
    /// // let result = handle.await;
    /// # Ok::<(), maniac::runtime::task::SpawnError>(())
    /// ```
    #[track_caller]
    pub fn spawn_blocking<F, T>(&self, future: F) -> Result<crate::blocking::Task<T>, SpawnError>
    where
        F: IntoFuture<Output = T> + Send + 'static,
        F::IntoFuture: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let location = Location::caller();
        let type_id = TypeId::of::<F>();
        Ok(unblock(move || {
            // Block on the future using a simple executor
            pollster::block_on(future.into_future())
        }))
    }
}

/// Cooperative executor executor backed by `MmapExecutorArena` and worker threads.
///
/// The executor provides a high-performance async executor that uses:
/// - Memory-mapped task arenas for efficient task storage
/// - Worker threads for parallel task execution
/// - Cooperative scheduling for optimal throughput
///
/// # Type Parameters
///
/// - `P`: Priority levels (default: 10)
/// - `NUM_SEGS_P2`: Number of segments as a power of 2 (default: 6, meaning 2^6 = 64 segments)
///
/// # Example
///
/// ```no_run
/// use maniac::runtime::Executor;
/// use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
///
/// let executor = Executor::new(
///     TaskArenaConfig::new(1, 8).unwrap(),
///     TaskArenaOptions::default(),
///     4, // worker_count
/// ).unwrap();
///
/// let handle = executor.spawn(async {
///     // Your async code here
///     42
/// }).unwrap();
///
/// // Later, await the result
/// // let result = handle.await;
/// ```
#[derive(Clone)]
pub struct Executor {
    /// Internal executor state shared across all operations
    inner: Arc<ExecutorInner>,
    /// Optionally owned tick service - if Some, this executor is responsible for shutting it down
    owned_tick_service: Option<Arc<TickService>>,
}

pub type DefaultExecutor = Executor;

impl Executor {
    pub fn new_single_threaded() -> Self {
        Self::new(
            TaskArenaConfig::new(8, 4096).unwrap(),
            TaskArenaOptions::new(false, false),
            1,
        )
        .unwrap()
    }
}

/// Internal executor state shared between all executor operations.
///
/// This structure holds the core components needed for task spawning and execution:
/// - `shutdown`: Atomic flag indicating whether the executor is shutting down
/// - `tick_service`: Shared tick service for time-based operations
/// - `service`: Worker service that manages task scheduling and execution
struct ExecutorInner {
    /// Atomic flag indicating if the executor is shutting down.
    /// Used to prevent new tasks from being spawned during shutdown.
    shutdown: Arc<AtomicBool>,
    /// Shared tick service for time-based operations across worker services.
    tick_service: Arc<TickService>,
    /// Worker service that manages task scheduling, worker threads, and task execution.
    service: Arc<Scheduler>,
}

impl ExecutorInner {
    /// Initiates shutdown of the executor.
    ///
    /// This method:
    /// 1. Sets the shutdown flag atomically (idempotent - safe to call multiple times)
    /// 2. Closes the task arena to prevent new tasks
    /// 3. Signals the worker service to shut down
    ///
    /// After shutdown is initiated, no new tasks can be spawned, but existing tasks
    /// will continue to run until they complete.
    fn shutdown(&self) {
        // Use swap to atomically set shutdown flag and check if it was already set
        // Return early if shutdown was already initiated
        if self.shutdown.swap(true, Ordering::Release) {
            return;
        }

        // Close the arena to prevent new task initialization
        self.service.arena().close();

        // Signal worker service to begin shutdown process
        self.service.shutdown();
    }
}

pub struct ExecutorConfig {
    task_arena: TaskArenaConfig,
    task_arena_options: TaskArenaOptions,
    worker_count: usize,
}

impl ExecutorConfig {
    pub fn new(
        task_arena: TaskArenaConfig,
        task_arena_options: TaskArenaOptions,
        worker_count: usize,
    ) -> ExecutorConfig {
        Self {
            task_arena,
            task_arena_options,
            worker_count,
        }
    }

    pub fn with_max_tasks(worker_count: usize, max_tasks: usize) -> io::Result<Self> {
        let worker_count = worker_count.max(1);
        let max_tasks = max_tasks.max(worker_count);
        let mut tasks_per_leaf = 4096;
        let mut leaf_count = (max_tasks / tasks_per_leaf).max(1);

        if leaf_count < worker_count {
            leaf_count = worker_count;
            tasks_per_leaf = (max_tasks / leaf_count).max(1);
        }

        Ok(Self {
            task_arena: TaskArenaConfig::new(leaf_count, tasks_per_leaf)?,
            task_arena_options: TaskArenaOptions::new(false, false),
            worker_count,
        })
    }
}

impl Executor {
    /// Creates a new executor instance with the specified configuration.
    ///
    /// This method initializes:
    /// 1. A task arena with the given configuration and options
    /// 2. A worker service with the specified number of worker threads
    /// 3. Internal executor state for task management
    ///
    /// # Arguments
    ///
    /// * `config`: Configuration for the task arena (capacity, layout, etc.)
    /// * `options`: Options for arena initialization (memory mapping, etc.)
    /// * `worker_count`: Number of worker threads to spawn for task execution
    ///
    /// # Returns
    ///
    /// * `Ok(Executor)`: Successfully created executor instance
    /// * `Err(io::Error)`: Error if arena initialization fails (e.g., memory mapping issues)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use maniac::runtime::Executor;
    /// use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
    ///
    /// let executor = Executor::new(
    ///     TaskArenaConfig::new(1, 8).unwrap(),
    ///     TaskArenaOptions::default(),
    ///     4, // Use 4 worker threads
    /// )?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn new(
        config: TaskArenaConfig,
        options: TaskArenaOptions,
        worker_count: usize,
    ) -> io::Result<Self> {
        let worker_config = SchedulerConfig::default();
        // Create shared tick service for time-based operations
        // This allows multiple worker services to share a single tick thread
        let tick_service = TickService::new(worker_config.tick_duration);
        tick_service.start();

        let mut executor =
            Self::new_with_tick_service(config, options, worker_count, Arc::clone(&tick_service))?;
        // Mark that this executor owns the tick service and is responsible for shutting it down
        executor.owned_tick_service = Some(tick_service);
        Ok(executor)
    }

    /// Creates a new executor instance with the specified configuration.
    ///
    /// This method initializes:
    /// 1. A task arena with the given configuration and options
    /// 2. A worker service with the specified number of worker threads
    /// 3. Internal executor state for task management
    ///
    /// # Arguments
    ///
    /// * `config`: Configuration for the task arena (capacity, layout, etc.)
    /// * `options`: Options for arena initialization (memory mapping, etc.)
    /// * `worker_count`: Number of worker threads to spawn for task execution
    ///
    /// # Returns
    ///
    /// * `Ok(Executor)`: Successfully created executor instance
    /// * `Err(io::Error)`: Error if arena initialization fails (e.g., memory mapping issues)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use maniac::runtime::Executor;
    /// use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
    ///
    /// let executor = Executor::new(
    ///     TaskArenaConfig::new(1, 8).unwrap(),
    ///     TaskArenaOptions::default(),
    ///     4, // Use 4 worker threads
    /// )?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn new_with_tick_service(
        config: TaskArenaConfig,
        options: TaskArenaOptions,
        worker_count: usize,
        tick_service: Arc<TickService>,
    ) -> io::Result<Self> {
        let worker_count = worker_count.max(1);

        // Initialize the memory-mapped task arena with the provided configuration
        let arena = TaskArena::with_config(config, options)?;

        // Create worker service configuration
        // Set min_workers to the desired count to ensure workers start immediately
        // Set max_workers to at least the desired count (or CPU count, whichever is higher)
        // This ensures workers start immediately on WorkerService::start()
        let worker_config = SchedulerConfig {
            worker_count,
            ..SchedulerConfig::default()
        };

        // Start the worker service with the arena and configuration
        // This spawns worker threads that will execute tasks
        let service = Scheduler::start(arena, worker_config, &tick_service);

        // Initialize shutdown flag to false (executor is active)
        let shutdown = Arc::new(AtomicBool::new(false));

        // Create internal executor state shared across all operations
        let inner = Arc::new(ExecutorInner {
            shutdown: Arc::clone(&shutdown),
            tick_service,
            service: Arc::clone(&service),
        });

        Ok(Self {
            inner,
            owned_tick_service: None, // Not owned when using an external tick service
        })
    }

    /// Spawns an asynchronous task on the executor, returning an awaitable join handle.
    ///
    /// This is the primary method for scheduling async work on the executor.
    /// The spawned task will be executed by one of the executor's worker threads.
    ///
    /// # Arguments
    ///
    /// * `future`: The future to execute. Can be any type that implements `IntoFuture`.
    ///
    /// # Returns
    ///
    /// * `Ok(JoinHandle<T>)`: A join handle that implements `Future<Output = T>`
    /// * `Err(SpawnError)`: Error if spawning fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use maniac::runtime::Executor;
    /// # use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
    /// # let executor = Executor::new(
    /// #     TaskArenaConfig::new(1, 8).unwrap(),
    /// #     TaskArenaOptions::default(),
    /// #     1,
    /// # ).unwrap();
    /// let handle = executor.spawn(async {
    ///     // Your async code here
    ///     42
    /// })?;
    ///
    /// // Await the result
    /// // let result = handle.await;
    /// # Ok::<(), maniac::runtime::task::SpawnError>(())
    /// ```
    #[track_caller]
    pub fn spawn<F, T>(&self, future: F) -> Result<JoinHandle<T>, SpawnError>
    where
        F: IntoFuture<Output = T> + Send + 'static,
        F::IntoFuture: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let location = Location::caller();
        let type_id = TypeId::of::<F>();
        self.inner.service.spawn(type_id, location, future)
    }

    /// Returns current arena statistics.
    ///
    /// This provides insight into the executor's current state, including:
    /// - Number of active tasks
    /// - Task capacity and utilization
    /// - Other arena-specific metrics
    ///
    /// # Returns
    ///
    /// `TaskArenaStats` containing current executor statistics
    pub fn stats(&self) -> TaskArenaStats {
        self.inner.service.arena().stats()
    }

    /// Returns a reference to the underlying worker service.
    ///
    /// This allows direct access to worker service operations such as:
    /// - Interrupting workers for preemptive scheduling
    /// - Accessing worker statistics and health information
    /// - Managing worker threads dynamically
    ///
    /// # Returns
    ///
    /// An `Arc` to the `WorkerService` managing this executor's workers
    pub fn service(&self) -> &Arc<Scheduler> {
        &self.inner.service
    }
}

impl Drop for Executor {
    /// Cleans up the executor when it goes out of scope.
    ///
    /// This implementation:
    /// 1. Initiates shutdown of the executor (closes arena, signals workers)
    /// 2. If this executor owns a tick service, shuts it down as well
    /// 3. Unparks any worker threads that might be waiting
    /// 4. Joins all worker threads to ensure clean shutdown
    ///
    /// Note: Currently, worker threads are managed by `WorkerService`, so the
    /// `workers` vector is typically empty. This code is prepared for future
    /// scenarios where we might manage worker threads directly.
    fn drop(&mut self) {
        // Initiate shutdown process (idempotent)
        self.inner.shutdown();

        // If this executor owns the tick service, shut it down
        // This must happen after worker shutdown to avoid issues with the tick handler
        if let Some(tick_service) = &self.owned_tick_service {
            tick_service.shutdown();
        }

        // // Unpark any waiting worker threads to allow them to exit
        // for worker in &self.workers {
        //     worker.thread().unpark();
        // }
        //
        // // Wait for all worker threads to complete
        // // drain(..) consumes the vector and returns an iterator
        // for handle in self.workers.drain(..) {
        //     let _ = handle.join();
        // }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::future::block_on;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, RawWaker, RawWakerVTable};
    use std::time::Duration;

    fn create_executor() -> DefaultExecutor {
        DefaultExecutor::new_single_threaded()
    }

    /// Creates a no-op waker for testing purposes.
    ///
    /// This waker does nothing when woken, which is useful for testing
    /// scenarios where we don't need actual async executor behavior.
    fn noop_waker() -> Waker {
        unsafe fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        unsafe fn wake(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake, wake);
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    /// Tests that a spawned task completes successfully and returns the correct result.
    ///
    /// This test:
    /// 1. Creates a executor with a single worker thread
    /// 2. Spawns a task that increments an atomic counter
    /// 3. Awaits the task completion
    /// 4. Verifies the result and that the counter was incremented
    /// 5. Verifies that the task was properly cleaned up (active_tasks == 0)
    #[test]
    fn executor_spawn_completes() {
        // Create a executor with minimal configuration for testing
        let executor = create_executor();

        // Create a shared counter to verify task execution
        let counter = Arc::new(AtomicUsize::new(0));
        let cloned = counter.clone();

        // Spawn a task that increments the counter and returns the new value
        let join = executor
            .spawn(async move { cloned.fetch_add(1, Ordering::Relaxed) + 1 })
            .expect("spawn");

        // Block until the task completes and get the result
        let result = block_on(join);

        // Verify the result is correct
        assert_eq!(result, 1);
        // Verify the counter was actually incremented
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        // Verify the task was cleaned up after completion
        assert_eq!(executor.stats().active_tasks, 0);
    }

    /// Tests that `JoinHandle::is_finished()` correctly reports task completion status.
    ///
    /// This test:
    /// 1. Creates a executor and spawns a simple task
    /// 2. Verifies `is_finished()` returns `false` before completion
    /// 3. Awaits the task to completion
    /// 4. Verifies that the task was properly cleaned up
    #[test]
    fn join_handle_reports_completion() {
        // Create a executor with minimal configuration
        let executor = create_executor();

        // Spawn an empty task (just completes immediately)
        let join = executor.spawn(async {}).expect("spawn");

        // Verify task is not finished immediately after spawning
        // (though it may complete very quickly, this check happens before awaiting)
        assert!(!join.is_finished());

        // Await the task to completion
        block_on(join);

        // Verify the task was cleaned up after completion
        assert_eq!(executor.stats().active_tasks, 0);
    }
}
