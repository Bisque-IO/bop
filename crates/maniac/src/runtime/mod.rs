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
pub mod io_driver;
pub mod mpsc;
pub mod preemption;
pub mod task;
pub mod timer;
pub mod worker;

pub use crate::{join, select, try_join};

pub(crate) mod io_builder;
#[allow(dead_code)]
pub(crate) mod io_runtime;

use std::future::{Future, IntoFuture};

pub use crate::driver::Driver;
#[cfg(all(target_os = "linux", feature = "iouring"))]
pub use crate::driver::IoUringDriver;
#[cfg(feature = "poll")]
pub use crate::driver::PollerDriver;
pub use io_builder::{Buildable, RuntimeBuilder};

// pub use runtime::{spawn, Runtime};
pub use io_runtime::IoRuntime;
#[cfg(any(all(target_os = "linux", feature = "iouring"), feature = "poll"))]
pub use {io_builder::FusionDriver, io_runtime::FusionRuntime};

/// A specialized `Result` type for `io-uring` operations with buffers.
pub use crate::buf::BufResult;

use std::any::TypeId;

use std::io;
use std::panic::Location;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;
use parking_lot::Mutex;
use task::{
    FutureAllocator, SpawnError, TaskArena, TaskArenaConfig, TaskArenaOptions, TaskArenaStats,
    TaskHandle,
};
use timer::ticker::TickService;
use worker::{WorkerService, WorkerServiceConfig};

use crate::num_cpus;

pub use crate::blocking::unblock;

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

pub struct Runtime<
    const P: usize = 10,
    const NUM_SEGS_P2: usize = 8,
    const BLOCKING_P: usize = 8,
    const BLOCKING_NUM_SEGS_P2: usize = 5,
> {
    executor: Executor<P, NUM_SEGS_P2>,
    tick_service: Arc<TickService>,
}

pub type DefaultRuntime = Runtime<10, 8, 8, 5>;

impl<
    const P: usize,
    const NUM_SEGS_P2: usize,
    const BLOCKING_P: usize,
    const BLOCKING_NUM_SEGS_P2: usize,
> Runtime<P, NUM_SEGS_P2, BLOCKING_P, BLOCKING_NUM_SEGS_P2>
{
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
        self.executor.inner.spawn(type_id, location, future)
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
pub struct Executor<const P: usize = 10, const NUM_SEGS_P2: usize = 8> {
    /// Internal executor state shared across all operations
    inner: Arc<ExecutorInner<P, NUM_SEGS_P2>>,
    /// Optionally owned tick service - if Some, this executor is responsible for shutting it down
    owned_tick_service: Option<Arc<TickService>>,
}

pub type DefaultExecutor = Executor<10, 8>;

impl<const P: usize, const NUM_SEGS_P2: usize> Executor<P, NUM_SEGS_P2> {
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
struct ExecutorInner<const P: usize, const NUM_SEGS_P2: usize> {
    /// Atomic flag indicating if the executor is shutting down.
    /// Used to prevent new tasks from being spawned during shutdown.
    shutdown: Arc<AtomicBool>,
    /// Shared tick service for time-based operations across worker services.
    tick_service: Arc<TickService>,
    /// Worker service that manages task scheduling, worker threads, and task execution.
    service: Arc<WorkerService<P, NUM_SEGS_P2>>,
}

impl<const P: usize, const NUM_SEGS_P2: usize> ExecutorInner<P, NUM_SEGS_P2> {
    /// Spawns a new async task on the executor.
    ///
    /// This method performs the following steps:
    /// 1. Validates that the executor is not shutting down and the arena is open
    /// 2. Reserves a task slot from the worker service
    /// 3. Initializes the task in the arena with its global ID
    /// 4. Wraps the user's future in a join future that handles completion and cleanup
    /// 5. Attaches the future to the task and schedules it for execution
    ///
    /// # Arguments
    ///
    /// * `future`: The future to execute. Must implement `IntoFuture` and be `Send + 'static`.
    ///
    /// # Returns
    ///
    /// * `Ok(JoinHandle<T>)`: A join handle that can be awaited to get the task's result
    /// * `Err(SpawnError)`: Error if spawning fails (closed executor, no capacity, or attach failed)
    ///
    /// # Errors
    ///
    /// - `SpawnError::Closed`: Executor is shutting down or arena is closed
    /// - `SpawnError::NoCapacity`: No available task slots in the arena
    /// - `SpawnError::AttachFailed`: Failed to attach the future to the task
    fn spawn<F, T>(
        &self,
        type_id: TypeId,
        location: &'static Location,
        future: F,
    ) -> Result<JoinHandle<T>, SpawnError>
    where
        F: IntoFuture<Output = T> + Send + 'static,
        F::IntoFuture: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let arena = self.service.arena();

        // Early return if executor is shutting down or arena is closed
        // This prevents spawning new tasks during shutdown
        if self.shutdown.load(Ordering::Acquire) || arena.is_closed() {
            return Err(SpawnError::Closed);
        }

        // Reserve a task slot from the worker service
        // Returns None if no capacity is available
        let Some(handle) = self.service.reserve_task() else {
            return Err(if arena.is_closed() {
                SpawnError::Closed
            } else {
                SpawnError::NoCapacity
            });
        };

        // Calculate the global task ID and initialize the task in the arena
        // The summary pointer is used for task tracking and statistics
        let global_id = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global_id, self.service.summary() as *const _);

        // Get the task reference and prepare handles for cleanup
        let task = handle.task();
        let release_handle = TaskHandle::from_non_null(handle.as_non_null());
        let service = Arc::clone(&self.service);

        // Create shared state for the join handle
        // This allows the spawned task to signal completion and the join handle to await it
        let shared = Arc::new(JoinShared::new());
        let shared_for_future = Arc::clone(&shared);
        let release_handle_for_future = release_handle;
        let service_for_future = Arc::clone(&self.service);

        // Wrap the user's future in a join future that:
        // 1. Awaits the user's future
        // 2. Signals completion via JoinShared
        // 3. Releases the task handle back to the worker service
        let join_future = async move {
            let result = future.into_future().await;
            shared_for_future.complete(result);
            service_for_future.release_task(release_handle_for_future);
        };

        // Allocate the future on the heap using the custom allocator
        let future_ptr = FutureAllocator::box_future(join_future);

        // Attach the future to the task
        // If attachment fails, clean up the allocated future and release the task handle
        if task.attach_future(future_ptr).is_err() {
            unsafe { FutureAllocator::drop_boxed(future_ptr) };
            self.service.release_task(release_handle);
            return Err(SpawnError::AttachFailed);
        }

        // Schedule the task for execution by worker threads
        task.schedule();

        // Return a join handle that the caller can await
        Ok(JoinHandle::new(shared))
    }

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

impl<const P: usize, const NUM_SEGS_P2: usize> Executor<P, NUM_SEGS_P2> {
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
        let worker_config = WorkerServiceConfig::default();
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
        let worker_config = WorkerServiceConfig {
            worker_count,
            ..WorkerServiceConfig::default()
        };

        // Start the worker service with the arena and configuration
        // This spawns worker threads that will execute tasks
        let service = WorkerService::<P, NUM_SEGS_P2>::start(arena, worker_config, &tick_service);

        // Initialize shutdown flag to false (executor is active)
        let shutdown = Arc::new(AtomicBool::new(false));

        // Create internal executor state shared across all operations
        let inner = Arc::new(ExecutorInner::<P, NUM_SEGS_P2> {
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
        self.inner.spawn(type_id, location, future)
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
    pub fn service(&self) -> &Arc<WorkerService<P, NUM_SEGS_P2>> {
        &self.inner.service
    }
}

impl<const P: usize, const NUM_SEGS_P2: usize> Drop for Executor<P, NUM_SEGS_P2> {
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

/// Shared state between a spawned task and its join handle.
///
/// This structure enables communication between:
/// - The spawned task (which calls `complete()` when done)
/// - The join handle (which calls `poll()` to await completion)
///
/// It uses a lock-free fast path (atomic `ready` flag) with a fallback to
/// mutex-protected state for storing the result and waker.
struct JoinShared<T> {
    /// Atomic flag indicating if the task has completed.
    /// Used as a fast-path check before acquiring the mutex.
    ready: AtomicBool,
    /// Mutex-protected state containing the result and waker.
    /// The mutex is only held briefly during result storage/retrieval.
    state: Mutex<JoinSharedState<T>>,
}

/// Internal state protected by the mutex in `JoinShared`.
struct JoinSharedState<T> {
    /// The task's result, if it has completed.
    result: Option<T>,
    /// The waker to notify when the task completes.
    /// Stored here so we can wake the awaiting future when completion occurs.
    waker: Option<Waker>,
}

impl<T> JoinShared<T> {
    /// Creates a new `JoinShared` instance in the initial (pending) state.
    fn new() -> Self {
        Self {
            ready: AtomicBool::new(false),
            state: Mutex::new(JoinSharedState {
                result: None,
                waker: None,
            }),
        }
    }

    /// Marks the task as complete and stores the result.
    ///
    /// This is called by the spawned task when it finishes execution.
    /// If a waker is registered, it will be notified to wake the awaiting future.
    ///
    /// # Arguments
    ///
    /// * `value`: The result value from the completed task
    ///
    /// # Safety
    ///
    /// This method is idempotent - calling it multiple times is safe but only
    /// the first call will have an effect.
    fn complete(&self, value: T) {
        let mut state = self.state.lock();

        // Idempotency check: if result already exists, don't overwrite it
        if state.result.is_some() {
            return;
        }

        // Store the result
        state.result = Some(value);

        // Set the ready flag atomically (Release ordering ensures previous writes are visible)
        self.ready.store(true, Ordering::Release);

        // Take the waker (if any) and drop the lock before waking
        // This prevents holding the lock while executing waker code
        let waker = state.waker.take();
        drop(state);

        // Wake the awaiting future if a waker was registered
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Polls for the task's completion.
    ///
    /// This is called by the `JoinHandle` future's `poll` method.
    /// Returns `Poll::Ready(T)` if the task has completed, or `Poll::Pending` if not.
    ///
    /// # Arguments
    ///
    /// * `cx`: The async context, used to register a waker for notification
    ///
    /// # Returns
    ///
    /// * `Poll::Ready(T)`: Task has completed, returns the result
    /// * `Poll::Pending`: Task is still running, waker has been registered
    fn poll(&self, cx: &mut Context<'_>) -> Poll<T> {
        // Fast path: check atomic flag first (avoid mutex if already ready)
        if self.ready.load(Ordering::Acquire) {
            let mut state = self.state.lock();
            if let Some(value) = state.result.take() {
                return Poll::Ready(value);
            }
        }

        // Slow path: acquire lock and check again (handles race condition)
        // Also register waker if task is still pending
        let mut state = self.state.lock();

        // Double-check: result might have been set between the atomic check and lock acquisition
        if let Some(value) = state.result.take() {
            return Poll::Ready(value);
        }

        // Task is still pending: register the waker so we can be notified when it completes
        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }

    /// Returns `true` if the task has completed.
    ///
    /// This is a non-blocking check that uses the atomic flag.
    /// Note: The result may have already been taken by a previous poll.
    fn is_finished(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

/// Awaitable join handle returned from [`Executor::spawn`].
///
/// This handle implements `Future<Output = T>` and can be awaited to get the
/// result of the spawned task. The handle can be cloned and shared, allowing
/// multiple futures to await the same task.
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
/// let handle = executor.spawn(async { 42 })?;
///
/// // Await the result
/// let result = handle.await;
/// # Ok::<(), maniac::runtime::task::SpawnError>(())
/// ```
pub struct JoinHandle<T> {
    /// Shared state with the spawned task for synchronization
    shared: Arc<JoinShared<T>>,
}

impl<T> JoinHandle<T> {
    /// Creates a new join handle from shared state.
    ///
    /// This is an internal constructor used by `ExecutorInner::spawn`.
    fn new(shared: Arc<JoinShared<T>>) -> Self {
        Self { shared }
    }

    /// Returns `true` if the task has completed.
    ///
    /// This is a non-blocking check. Note that even if `is_finished()` returns
    /// `true`, the result may have already been consumed by a previous await.
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
    /// let handle = executor.spawn(async { 42 })?;
    ///
    /// // Check if task is done (non-blocking)
    /// if handle.is_finished() {
    ///     println!("Task completed!");
    /// }
    /// # Ok::<(), maniac::runtime::task::SpawnError>(())
    /// ```
    pub fn is_finished(&self) -> bool {
        self.shared.is_finished()
    }
}

impl<T: Send + 'static> Future for JoinHandle<T> {
    type Output = T;

    /// Polls the join handle for completion.
    ///
    /// This method:
    /// - Returns `Poll::Ready(T)` if the task has completed
    /// - Returns `Poll::Pending` if the task is still running
    /// - Registers a waker to be notified when the task completes
    ///
    /// # Arguments
    ///
    /// * `cx`: The async context containing the waker
    ///
    /// # Returns
    ///
    /// * `Poll::Ready(T)`: Task completed, returns the result
    /// * `Poll::Pending`: Task still running, will wake when done
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.shared.poll(cx)
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
