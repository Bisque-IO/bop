pub mod waker;

use super::deque::{Stealer, Worker as YieldWorker};
use super::task::summary::Summary;
use super::task::{GeneratorRunMode, Task, TaskArena, TaskHandle, TaskSlot};
use super::timer::{Timer, TimerHandle};
use super::timer::wheel::TimerWheel;
use waker::WorkerWaker;
use crate::PopError;
use crate::future::waker::DiatomicWaker;
use crate::runtime::io_driver::IoDriver;
use crate::runtime::preemption::GeneratorYieldReason;
use crate::runtime::task::{GeneratorOwnership, SpawnError};
use crate::runtime::timer::ticker::{TickHandler, TickHandlerRegistration};
use crate::runtime::{mpsc, preemption};
use crate::{PushError, utils};
use std::any::TypeId;
use std::cell::{Cell, UnsafeCell};
use std::panic::Location;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crate::utils::bits;
use rand::RngCore;

#[cfg(unix)]
use libc;

const MAX_WORKER_LIMIT: usize = 512;

const DEFAULT_WAKE_BURST: usize = 4;
const FULL_SUMMARY_SCAN_CADENCE_MASK: u64 = 1024 * 8 - 1;
const DEFAULT_TICK_DURATION_NS: u64 = 1 << 20; // ~1.05ms, power of two as required by TimerWheel
const TIMER_TICKS_PER_WHEEL: usize = 1024 * 1;
const TIMER_EXPIRE_BUDGET: usize = 4096;
const MESSAGE_BATCH_SIZE: usize = 4096;

// Worker status is now managed via WorkerWaker:
// - WorkerWaker.summary: mpsc queue signals (bits 0-63)
// - WorkerWaker.status bit 63: yield queue has items
// - WorkerWaker.status bit 62: partition cache has work

/// Trait for cross-worker operations that don't depend on const parameters
pub(crate) trait WorkerBus: Send + Sync {
    fn post_cancel_message(&self, from_worker_id: u32, to_worker_id: u32, timer_id: u64) -> bool;
}

pub struct WorkerTLS {}

pub enum RunOnceError {
    StackPinned,

    Raised(Box<dyn std::error::Error>),
}

thread_local! {
    pub(crate) static CURRENT: Cell<*mut Worker<'static>> = Cell::new(core::ptr::null_mut());
}

#[inline]
pub fn is_worker_thread() -> bool {
    current_task().is_some()
}

#[inline]
pub fn current_worker<'a>() -> Option<&'a mut WorkerInner<'a>> {
    if crate::runtime::io_runtime::CURRENT.is_set() {
        crate::runtime::io_runtime::CURRENT.with(|ctx| {
            let ptr = ctx.user_data.get();
            if ptr.is_null() {
                None
            } else {
                unsafe { Some(&mut *(ptr as *mut WorkerInner<'a>)) }
            }
        })
    } else {
        None
    }
}

#[inline]
pub fn current_task<'a>() -> Option<&'a mut Task> {
    let worker = current_worker()?;
    unsafe { Some(&mut *worker.current_task) }
}

pub fn current_timer_wheel<'a>() -> Option<&'a mut TimerWheel<TimerHandle>> {
    let worker = current_worker()?;
    unsafe { Some(&mut *worker.timer_wheel) }
}

pub fn current_event_loop<'a>() -> Option<&'a mut IoDriver> {
    let worker = current_worker()?;
    unsafe { Some(&mut *worker.io) }
}

/// Returns the most recent `now_ns` observed by the active worker.
pub fn current_worker_now_ns() -> u64 {
    let timer_wheel = current_timer_wheel();
    if let Some(timer_wheel) = current_timer_wheel() {
        timer_wheel.now_ns()
    } else {
        // fallback to high frequency clock
        Instant::now().elapsed().as_nanos() as u64
    }
}

/// Returns the current worker's ID, or None if not called from within a worker context.
pub fn current_worker_id() -> Option<u32> {
    let timer_wheel = current_timer_wheel()?;
    Some(timer_wheel.worker_id())
}

// ============================================================================
// Internal Helper (Called by Trampoline)
// ============================================================================

/// This function is called by the assembly trampoline.
/// It runs on the worker thread's stack in a normal execution context.
/// It is safe to access TLS and yield here.
#[unsafe(no_mangle)]
pub extern "C" fn rust_preemption_helper() {
    if let Some(worker) = current_worker() {
        let task = worker.current_task;
        let scope = worker.current_scope;
        if task.is_null() || scope.is_null() {
            return;
        }

        unsafe {
            let task = &mut *task;
            if !task.has_pinned_generator() {
                unsafe {
                    task.pin_generator(
                        scope as *mut usize,
                        GeneratorOwnership::Worker,
                        GeneratorRunMode::Switch,
                    )
                };
            } else {
                task.set_generator_run_mode(GeneratorRunMode::Switch);
            }

            // 1. Check and clear flag (for cooperative correctness / hygiene)
            // We don't require it to be true to yield, because we might be here via Unix signal
            // which couldn't set the flag.
            worker.preemption_requested.store(false, Ordering::Release);

            // SAFETY: The pointer is set only while the worker generator is running and
            // cleared immediately after it exits. The trampoline only executes while the
            // worker is inside that generator, so the pointer is always valid here.
            let scope = &mut *(scope as *mut crate::generator::Scope<(), usize>);
            let _ = scope.yield_(GeneratorYieldReason::Preempted.as_usize());
        }
    } else {
        // Non-worker path: Use TLS generator scope
        // CRITICAL: Don't clear the scope during preemption, as another signal could arrive
        // between clear and restore, seeing a null pointer. The scope remains valid throughout
        // the generator's lifetime and is only cleared when the generator exits.
        let scope = preemption::get_generator_scope();
        if !scope.is_null() {
            let scope = unsafe { &mut *(scope as *mut crate::generator::Scope<(), usize>) };
            let _ = scope.yield_(GeneratorYieldReason::Preempted.as_usize());
        }
    }
}

/// Request that the current task receive its own dedicated generator (stackful coroutine).
///
/// This MUST be called from within a task that is executing inside a Worker's generator.
/// When called, it signals the Worker that after the current poll returns, the task
/// should receive its own dedicated poll-mode generator instead of sharing the worker's.
///
/// **Important**: This does NOT capture the current stack state. It simply marks that
/// the task wants a dedicated generator. The worker will create a new generator for
/// the task after the current poll completes.
///
/// For preemption (capturing the current stack mid-execution), use the signal-based
/// preemption mechanism instead (which uses Switch mode).
///
/// # Returns
/// `true` if the pin request was successfully registered, `false` if not in a task context.
///
/// # Example
/// ```ignore
/// use maniac::runtime::worker::pin_stack;
///
/// // Inside an async task running on a worker:
/// async {
///     // Request a dedicated generator for this task
///     if pin_stack() {
///         // Task will receive its own generator after this poll returns
///     }
/// }
/// ```
pub fn pin_stack() -> bool {
    if let Some(worker) = current_worker() {
        let task = worker.current_task;
        if task.is_null() {
            return false;
        }
        let task = unsafe { &mut *task };
        if !task.has_pinned_generator() {
            // Mark the task as wanting a dedicated generator.
            // We use a null pointer as a sentinel value - the worker will detect this
            // and create a proper poll-mode generator for the task.
            // The Owned ownership indicates we need to CREATE a generator, not that
            // we're capturing the worker's generator.
            unsafe {
                task.pin_generator(
                    std::ptr::null_mut(),
                    GeneratorOwnership::Owned,
                    GeneratorRunMode::Poll,
                )
            };
        }
        true
    } else {
        false
    }
}

/// Type-erased header for futures stored in tasks.
/// Contains function pointers for polling and dropping without knowing the concrete type.
#[repr(C)]
struct FutureHeader {
    /// Function to poll the future. Takes pointer to the header (start of allocation).
    poll_fn: unsafe fn(*mut FutureHeader, &mut Context<'_>) -> Poll<()>,
    /// Function to drop the future. Takes pointer to the header (start of allocation).
    drop_fn: unsafe fn(*mut FutureHeader),
}

/// Join state header - the type-erased portion of JoinableFuture that JoinHandle accesses.
///
/// This struct contains only the synchronization state needed for joining,
/// without the future type. It must be at the start of JoinableFuture (same layout).
#[repr(C)]
pub struct JoinState<T> {
    /// Waker for join notification. Uses DiatomicWaker for lock-free inline storage.
    waker: DiatomicWaker,
    /// Atomic flag indicating the task has completed and result is ready.
    ready: AtomicBool,
    /// The result value, written once when the future completes.
    result: UnsafeCell<std::mem::MaybeUninit<T>>,
}

impl<T> JoinState<T> {
    /// Stores the result and marks as ready. Wakes any waiting JoinHandle.
    #[inline]
    pub fn complete(&self, value: T) {
        unsafe {
            (*self.result.get()).write(value);
        }
        self.ready.store(true, Ordering::Release);
        self.waker.notify();
    }

    /// Registers a waker to be notified when the future completes.
    ///
    /// # Safety
    /// Only one task should register wakers at a time (single consumer).
    #[inline]
    pub unsafe fn register_waker(&self, waker: &Waker) {
        unsafe { self.waker.register(waker) };
    }

    /// Checks if the result is ready.
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Takes the result.
    ///
    /// # Safety
    /// Must only be called once after is_ready() returns true.
    #[inline]
    pub unsafe fn take_result(&self) -> T {
        unsafe { (*self.result.get()).assume_init_read() }
    }
}

/// A future combined with its join state in a single allocation.
///
/// Layout (repr(C)):
/// 1. FutureHeader - poll/drop function pointers for type erasure
/// 2. JoinState<T> - join synchronization (waker, ready, result)
/// 3. F - the future stored inline
///
/// This is a **single allocation** for header, join state, and future.
#[repr(C)]
pub struct JoinableFuture<T, F> {
    /// Function pointers for type-erased polling and dropping.
    header: FutureHeader,
    /// Join state (waker, ready, result).
    join: JoinState<T>,
    /// The future stored inline (not boxed).
    future: F,
}

// Safety: JoinableFuture is Send if T and F are Send.
unsafe impl<T: Send, F: Send> Send for JoinableFuture<T, F> {}
// Safety: JoinableFuture is Sync - DiatomicWaker handles concurrent access,
// AtomicBool is atomic, and result is only written once then read once.
unsafe impl<T: Send, F: Send> Sync for JoinableFuture<T, F> {}

impl<T, F: Future<Output = ()> + Send + 'static> JoinableFuture<T, F> {
    /// Creates a new joinable future with the future stored inline.
    #[inline]
    pub fn new(future: F) -> Self {
        Self {
            header: FutureHeader {
                poll_fn: Self::poll_erased,
                drop_fn: Self::drop_erased,
            },
            join: JoinState {
                waker: DiatomicWaker::new(),
                ready: AtomicBool::new(false),
                result: UnsafeCell::new(std::mem::MaybeUninit::uninit()),
            },
            future,
        }
    }

    /// Type-erased poll function called via function pointer.
    unsafe fn poll_erased(ptr: *mut FutureHeader, cx: &mut Context<'_>) -> Poll<()> {
        let this = ptr as *mut Self;
        // Safety: ptr points to a valid pinned JoinableFuture
        let future = unsafe { Pin::new_unchecked(&mut (*this).future) };
        future.poll(cx)
    }

    /// Type-erased drop function called via function pointer.
    unsafe fn drop_erased(ptr: *mut FutureHeader) {
        let this = ptr as *mut Self;
        // Safety: ptr points to a valid boxed JoinableFuture
        unsafe { drop(Box::from_raw(this)) };
    }

    /// Returns a reference to the join state.
    #[inline]
    pub fn join_state(&self) -> &JoinState<T> {
        &self.join
    }
}

/// Polls a type-erased future via its header.
///
/// # Safety
/// `ptr` must point to a valid `JoinableFuture` that was created via `Box::pin`.
#[inline]
pub(crate) unsafe fn poll_future(ptr: *mut (), cx: &mut Context<'_>) -> Poll<()> {
    let header = ptr as *mut FutureHeader;
    unsafe { ((*header).poll_fn)(header, cx) }
}

/// Drops a type-erased future via its header.
///
/// # Safety
/// `ptr` must point to a valid `JoinableFuture` that was created via `Box::pin`.
/// Must only be called once.
#[inline]
pub(crate) unsafe fn drop_future(ptr: *mut ()) {
    if ptr.is_null() {
        return;
    }
    let header = ptr as *mut FutureHeader;
    unsafe { ((*header).drop_fn)(header) };
}

/// Awaitable join handle returned from [`Executor::spawn`].
///
/// This handle points directly to the `JoinableFuture<T>` allocation which contains
/// both the user's future and the join synchronization state (waker, ready flag, result).
/// This avoids any separate allocation for join state.
///
/// The generation number ensures that if a task slot is reused before the
/// JoinHandle is awaited, polling will return Pending forever rather than
/// accessing incorrect results.
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
    /// Pointer to the JoinState<T> (which is at the start of JoinableFuture<T, F>).
    /// This allows type-erased access to the join state without knowing F.
    /// The JoinableFuture box is kept alive until the result is taken.
    join_state_ptr: *const JoinState<T>,
    /// Prevents the executor/arena from being deallocated while this handle exists.
    /// This ensures the JoinableFuture storage remains valid.
    _guard: Arc<WorkerService>,
}

impl<T> JoinHandle<T> {
    /// Creates a new join handle.
    ///
    /// # Arguments
    /// * `join_state_ptr` - Pointer to the JoinState<T> at the start of JoinableFuture
    /// * `guard` - Arc that keeps the executor/arena alive while this handle exists
    #[inline]
    pub fn new(join_state_ptr: *const JoinState<T>, guard: Arc<WorkerService>) -> Self {
        Self {
            join_state_ptr,
            _guard: guard,
        }
    }

    /// Returns `true` if the task has completed.
    ///
    /// This is a non-blocking check. Note that even if `is_finished()` returns
    /// `true`, the result may have already been consumed by a previous await.
    #[inline]
    pub fn is_finished(&self) -> bool {
        unsafe { (*self.join_state_ptr).is_ready() }
    }

    #[inline]
    fn join_state(&self) -> &JoinState<T> {
        unsafe { &*self.join_state_ptr }
    }
}

// JoinHandle is Send if T is Send (the result will be sent across threads)
unsafe impl<T: Send> Send for JoinHandle<T> {}
// JoinHandle is Sync if T is Send (multiple threads can poll it)
unsafe impl<T: Send> Sync for JoinHandle<T> {}

impl<T: Send + 'static> Future for JoinHandle<T> {
    type Output = T;

    /// Polls the join handle for completion.
    ///
    /// This method:
    /// - Returns `Poll::Ready(T)` if the task has completed
    /// - Returns `Poll::Pending` if the task is still running
    /// - Registers a waker to be notified when the task completes
    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let join_state = self.join_state();

        // Fast path: check if already ready
        if join_state.is_ready() {
            // Safety: result is ready, and we only take it once
            return Poll::Ready(unsafe { join_state.take_result() });
        }

        // Register waker and check again
        // Safety: JoinHandle is the single consumer
        unsafe { join_state.register_waker(cx.waker()) };

        // Double-check after registration
        if join_state.is_ready() {
            // Safety: result is ready, and we only take it once
            return Poll::Ready(unsafe { join_state.take_result() });
        }

        Poll::Pending
    }
}

/// Remote scheduling request for a timer.
#[derive(Clone, Copy, Debug)]
pub struct TimerSchedule {
    handle: TimerHandle,
    deadline_ns: u64,
}

impl TimerSchedule {
    pub fn new(handle: TimerHandle, deadline_ns: u64) -> Self {
        Self {
            handle,
            deadline_ns,
        }
    }

    #[inline(always)]
    pub fn handle(&self) -> TimerHandle {
        self.handle
    }

    #[inline(always)]
    pub fn deadline_ns(&self) -> u64 {
        self.deadline_ns
    }

    #[inline(always)]
    pub fn into_parts(self) -> (TimerHandle, u64) {
        (self.handle, self.deadline_ns)
    }
}

unsafe impl Send for TimerSchedule {}
unsafe impl Sync for TimerSchedule {}

/// Batch of scheduling requests.
#[derive(Debug, Clone)]
pub struct TimerBatch {
    entries: Box<[TimerSchedule]>,
}

impl TimerBatch {
    pub fn new(entries: Vec<TimerSchedule>) -> Self {
        Self {
            entries: entries.into_boxed_slice(),
        }
    }

    pub fn empty() -> Self {
        Self {
            entries: Box::new([]),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &TimerSchedule> {
        self.entries.iter()
    }

    pub fn into_vec(self) -> Vec<TimerSchedule> {
        self.entries.into_vec()
    }
}

/// Worker control-plane messages.
#[derive(Debug)]
pub enum WorkerMessage {
    ScheduleTimer {
        timer: TimerSchedule,
    },
    ScheduleBatch {
        timers: TimerBatch,
    },
    CancelTimer {
        worker_id: u32,
        timer_id: u64,
    },

    /// Request worker to report its current health metrics
    ReportHealth,

    /// Graceful shutdown: finish current task then exit
    GracefulShutdown,

    /// Immediate shutdown
    Shutdown,

    Noop,
}

unsafe impl Send for WorkerMessage {}
unsafe impl Sync for WorkerMessage {}

#[derive(Clone, Copy, Debug)]
pub struct WorkerServiceConfig {
    pub tick_duration: Duration,
    pub worker_count: usize,
}

impl Default for WorkerServiceConfig {
    fn default() -> Self {
        Self {
            tick_duration: Duration::from_nanos(DEFAULT_TICK_DURATION_NS),
            worker_count: utils::num_cpus(),
        }
    }
}

pub struct WorkerService {
    // Core ownership - WorkerService owns the arena and coordinates work via SummaryTree
    arena: TaskArena,
    summary_tree: Summary,

    config: WorkerServiceConfig,
    tick_duration: Duration,
    tick_duration_ns: u64,
    clock_ns: Arc<AtomicU64>,

    // Per-worker WorkerWakers - each tracks all three work types:
    // - summary: mpsc queue signals (bits 0-63 for different signal words)
    // - status bit 63: yield queue has items
    // - status bit 62: task partition has work
    wakers: Box<[Arc<WorkerWaker>]>,

    worker_actives: Box<[AtomicU64]>,
    worker_now_ns: Box<[AtomicU64]>,
    worker_shutdowns: Box<[AtomicBool]>,
    worker_threads: Box<[Mutex<Option<thread::JoinHandle<()>>>]>,
    worker_thread_handles: Box<[Mutex<Option<crate::runtime::preemption::WorkerThreadHandle>>]>,
    worker_stats: Box<[WorkerStats]>,
    worker_count: Arc<AtomicUsize>,
    receivers: Box<[UnsafeCell<mpsc::Receiver<WorkerMessage>>]>,
    senders: Box<[mpsc::Sender<WorkerMessage>]>,
    tick_senders: Box<[mpsc::Sender<WorkerMessage>]>,
    yield_queues: Box<[YieldWorker<TaskHandle>]>,
    yield_stealers: Box<[Stealer<TaskHandle>]>,
    timers: Box<[UnsafeCell<TimerWheel<TimerHandle>>]>,
    shutdown: AtomicBool,
    register_mutex: Mutex<()>,
    // RAII guard for tick service registration - automatically unregisters on drop
    tick_registration: Mutex<Option<TickHandlerRegistration>>,
    // Tick-related counters for on_tick logic
    tick_health_check_interval: u64,
}

unsafe impl Send for WorkerService {}

// SAFETY: WorkerService contains UnsafeCells for receivers and timers, but each worker
// has exclusive access to its own index. The Arc-wrapped WorkerService is shared between
// threads, but the UnsafeCell data is partitioned by worker_id.
unsafe impl Sync for WorkerService {}

impl WorkerService {
    pub fn start(
        arena: TaskArena,
        config: WorkerServiceConfig,
        tick_service: &Arc<super::timer::ticker::TickService>,
    ) -> Arc<Self> {
        let tick_duration = config.tick_duration;
        let tick_duration_ns = normalize_tick_duration_ns(config.tick_duration);
        let worker_count_val = config.worker_count.max(1).min(MAX_WORKER_LIMIT);

        // Create per-worker WorkerWakers
        let mut wakers = Vec::with_capacity(worker_count_val);
        for _ in 0..worker_count_val {
            wakers.push(Arc::new(WorkerWaker::new()));
        }
        let wakers = wakers.into_boxed_slice();

        // Create worker_count early so we can pass it to SummaryTree (single source of truth)
        let worker_count = Arc::new(AtomicUsize::new(worker_count_val));

        // Create SummaryTree with reference to wakers and worker_count
        let summary_tree = Summary::new(
            arena.config().leaf_count,
            arena.layout().signals_per_leaf,
            &wakers,
            &worker_count,
        );

        let mut worker_actives = Vec::with_capacity(worker_count_val);
        for _ in 0..worker_count_val {
            worker_actives.push(AtomicU64::new(0));
        }
        let mut worker_now_ns = Vec::with_capacity(worker_count_val);
        for _ in 0..worker_count_val {
            worker_now_ns.push(AtomicU64::new(0));
        }
        let mut worker_shutdowns = Vec::with_capacity(worker_count_val);
        for _ in 0..worker_count_val {
            worker_shutdowns.push(AtomicBool::new(false));
        }
        let mut worker_threads = Vec::with_capacity(worker_count_val);
        for _ in 0..worker_count_val {
            worker_threads.push(Mutex::new(None));
        }
        let mut worker_thread_handles = Vec::with_capacity(worker_count_val);
        for _ in 0..worker_count_val {
            worker_thread_handles.push(Mutex::new(None));
        }
        let mut worker_stats = Vec::with_capacity(worker_count_val);
        for _ in 0..worker_count_val {
            worker_stats.push(WorkerStats::default());
        }

        let mut receivers = Vec::with_capacity(worker_count_val);
        let mut senders = Vec::<mpsc::Sender<WorkerMessage>>::with_capacity(
            worker_count_val * worker_count_val,
        );
        for worker_id in 0..worker_count_val {
            // Use the worker's WorkerWaker for mpsc queue
            let rx = mpsc::new_with_waker(Arc::clone(&wakers[worker_id]));
            receivers.push(UnsafeCell::new(rx));
        }
        for worker_id in 0..worker_count_val {
            for other_worker_id in 0..worker_count_val {
                senders.push(
                    unsafe { &*receivers[worker_id].get() }
                        .create_sender_with_config(0)
                        .expect("ran out of producer slots"),
                )
            }
        }

        // Create tick_senders for on_tick communication
        let mut tick_senders = Vec::with_capacity(worker_count_val);
        for worker_id in 0..worker_count_val {
            tick_senders.push(
                unsafe { &*receivers[worker_id].get() }
                    .create_sender_with_config(0)
                    .expect("ran out of producer slots"),
            );
        }

        let mut yield_queues = Vec::with_capacity(worker_count_val);
        for _ in 0..worker_count_val {
            yield_queues.push(YieldWorker::new_fifo());
        }
        let mut yield_stealers = Vec::with_capacity(worker_count_val);
        for worker_id in 0..worker_count_val {
            yield_stealers.push(yield_queues[worker_id].stealer());
        }
        let mut timers = Vec::with_capacity(worker_count_val);
        for worker_id in 0..worker_count_val {
            timers.push(UnsafeCell::new(TimerWheel::from_duration(
                tick_duration,
                TIMER_TICKS_PER_WHEEL,
                worker_id as u32,
            )));
        }

        let service = Arc::new(Self {
            arena,
            summary_tree,
            config,
            tick_duration,
            tick_duration_ns,
            clock_ns: Arc::new(AtomicU64::new(0)),
            wakers,
            worker_actives: worker_actives.into_boxed_slice(),
            worker_now_ns: worker_now_ns.into_boxed_slice(),
            worker_shutdowns: worker_shutdowns.into_boxed_slice(),
            worker_threads: worker_threads.into_boxed_slice(),
            worker_thread_handles: worker_thread_handles.into_boxed_slice(),
            worker_stats: worker_stats.into_boxed_slice(),
            worker_count,
            receivers: receivers.into_boxed_slice(),
            senders: senders.into_boxed_slice(),
            tick_senders: tick_senders.into_boxed_slice(),
            yield_queues: yield_queues.into_boxed_slice(),
            yield_stealers: yield_stealers.into_boxed_slice(),
            timers: timers.into_boxed_slice(),
            shutdown: AtomicBool::new(false),
            register_mutex: Mutex::new(()),
            tick_registration: Mutex::new(None),
            tick_health_check_interval: 100,
        });

        // Statically partition the arena leaves among workers
        let total_leaves = service.arena.leaf_count();
        let leaves_per_worker = (total_leaves + worker_count_val - 1) / worker_count_val;

        // Spawn all workers
        for worker_id in 0..worker_count_val {
            let partition_start = worker_id * leaves_per_worker;
            let partition_end = ((worker_id + 1) * leaves_per_worker).min(total_leaves);

            if service
                .spawn_worker_fixed(&service, worker_id, partition_start, partition_end)
                .is_err()
            {
                // Log error but continue spawning others
                tracing::trace!("Failed to spawn worker {}", worker_id);
            }
        }

        // Register this service with the TickService and get RAII guard
        let registration = {
            let handler: Arc<dyn TickHandler> = Arc::clone(&service) as Arc<dyn TickHandler>;
            tick_service
                .register(handler)
                .expect("Failed to register with TickService")
        };

        // Store the registration in the service
        // SAFETY: We use raw pointer manipulation because we can't get a mutable reference to an Arc.
        // This is safe because:
        // 1. We're still in the constructor before returning the Arc
        // 2. No other thread has access to this service yet
        // 3. We're only writing to _tick_registration once during construction
        unsafe {
            let service_ptr = Arc::as_ptr(&service) as *mut WorkerService;
            *(*service_ptr).tick_registration.lock().unwrap() = Some(registration);
        }

        service
    }

    /// Get a reference to the ExecutorArena owned by this service.
    #[inline]
    pub fn arena(&self) -> &TaskArena {
        &self.arena
    }

    /// Get a reference to the SummaryTree owned by this service.
    #[inline]
    pub fn summary(&self) -> &Summary {
        &self.summary_tree
    }

    /// Reserve a task slot using the SummaryTree.
    #[inline]
    pub fn reserve_task(&self) -> Option<TaskHandle> {
        if self.arena.is_closed() {
            return None;
        }
        let (leaf_idx, signal_idx, bit) = self.summary_tree.reserve_task()?;
        let handle = match self.arena.handle_for_location(leaf_idx, signal_idx, bit) {
            Some(handle) => handle,
            None => {
                self.summary_tree
                    .release_task_in_leaf(leaf_idx, signal_idx, bit as usize);
                return None;
            }
        };
        self.arena.increment_total_tasks();
        Some(handle)
    }

    /// Release a task slot back to the SummaryTree.
    #[inline]
    pub fn release_task(&self, handle: TaskHandle) {
        let task = handle.task();
        self.summary_tree.release_task_in_leaf(
            task.leaf_idx() as usize,
            task.signal_idx() as usize,
            task.signal_bit() as usize,
        );
        self.arena.decrement_total_tasks();
    }

    fn spawn_worker_fixed(
        &self,
        service: &Arc<Self>,
        worker_id: usize,
        partition_start: usize,
        partition_end: usize,
    ) -> Result<(), PushError<()>> {
        let _lock = self.register_mutex.lock().expect("register_mutex poisoned");
        let now_ns = Instant::now().elapsed().as_nanos() as u64;

        // Mark worker as active
        self.worker_actives[worker_id].store(1, Ordering::SeqCst);
        self.worker_now_ns[worker_id].store(now_ns, Ordering::SeqCst);

        // Initialize partition cache in summary tree for this worker
        self.wakers[worker_id].sync_partition_summary(
            partition_start,
            partition_end,
            &self.summary_tree.leaf_words,
        );

        let service_clone = Arc::clone(service);
        let timer_resolution_ns = self.tick_duration().as_nanos().max(1) as u64;
        let partition_len = partition_end.saturating_sub(partition_start);

        let join = std::thread::spawn(move || {
            let task_slot = TaskSlot::new(std::ptr::null_mut());
            let task_slot_ptr = &task_slot as *const _ as *mut TaskSlot;
            let mut timer_output = Vec::with_capacity(MESSAGE_BATCH_SIZE);
            for _ in 0..MESSAGE_BATCH_SIZE {
                timer_output.push((
                    0u64,
                    0u64,
                    TimerHandle::new(unsafe { NonNull::new_unchecked(task_slot_ptr) }, 0, 0, 0),
                ));
            }
            let mut message_batch = Vec::with_capacity(MESSAGE_BATCH_SIZE);
            for _ in 0..MESSAGE_BATCH_SIZE {
                message_batch.push(WorkerMessage::Noop);
            }

            // Create EventLoop for this worker (with integrated SingleWheel timer)
            // Box it to avoid stack overflow (EventLoop contains large Slab and buffers)
            let event_loop = Box::new(
                crate::runtime::io_driver::IoDriver::new()
                    .expect("Failed to create EventLoop for worker"),
            );

            let mut w = Worker {
                service: Arc::clone(&service_clone),
                receiver: unsafe {
                    // SAFETY: Worker thread has exclusive access to its own receiver.
                    // The service Arc is kept alive for the lifetime of the worker.
                    &mut *service_clone.receivers[worker_id].get()
                },
                inner: WorkerInner {
                    wait_strategy: WaitStrategy::default(),
                    shutdown: &service_clone.worker_shutdowns[worker_id],
                    yield_queue: &service_clone.yield_queues[worker_id],
                    timer_wheel: unsafe {
                        // SAFETY: Worker thread has exclusive access to its own timer wheel.
                        // The service Arc is kept alive for the lifetime of the worker.
                        &mut *service_clone.timers[worker_id].get()
                    },
                    bus: unsafe { &*(&*service_clone as *const dyn WorkerBus) },
                    wake_stats: WakeStats::default(),
                    stats: WorkerStats::default(),
                    partition_start,
                    partition_end,
                    partition_len,
                    cached_worker_count: service_clone.worker_count.load(Ordering::Relaxed),
                    wake_burst_limit: DEFAULT_WAKE_BURST,
                    worker_id: worker_id as u32,
                    timer_resolution_ns,
                    timer_output,
                    message_batch: message_batch.into_boxed_slice(),
                    rng: crate::utils::Random::new(),
                    current_task: std::ptr::null_mut(),
                    current_scope: std::ptr::null_mut(),
                    preemption_requested: AtomicBool::new(false),
                    io: event_loop,
                    io_budget: 16, // Process up to 16 I/O events per run_once iteration
                },
            };

            // CURRENT_WORKER.set(unsafe { &w.inner as *const _ as *mut WorkerInner<'static> });

            // Wrap execution in monoio context and set user_data
            let w_ptr = std::ptr::addr_of_mut!(w);
            // Wrap execution in monoio context and set user_data
            unsafe { &(*w_ptr).inner.io }.with_context(|| {
                crate::runtime::io_runtime::CURRENT.with(|ctx| {
                    ctx.user_data
                        .set(unsafe { &(*w_ptr).inner as *const _ as *mut () });
                });

                // Initialize preemption support for this worker thread
                let _preemption_handle = crate::runtime::preemption::init_worker_preemption().ok(); // Ignore errors - preemption is optional

                // Create a handle to this worker thread so it can be interrupted
                // On Windows, we pass the preemption flag so APC can access it
                #[cfg(unix)]
                let thread_handle_result =
                    crate::runtime::preemption::WorkerThreadHandle::current();

                #[cfg(windows)]
                let thread_handle_result =
                    crate::runtime::preemption::WorkerThreadHandle::current(unsafe {
                        &(*w_ptr).inner.preemption_requested
                    });

                #[cfg(not(any(unix, windows)))]
                let thread_handle_result =
                    crate::runtime::preemption::WorkerThreadHandle::current(unsafe {
                        &(*w_ptr).preemption_requested
                    });

                if let Ok(thread_handle) = thread_handle_result {
                    *service_clone.worker_thread_handles[worker_id]
                        .lock()
                        .expect("worker_thread_handles lock poisoned") = Some(thread_handle);
                }

                // Catch panics to prevent thread termination from propagating
                // SAFETY: Worker cleanup code (service counter updates) will still execute after a panic
                // We use a raw pointer wrapped in AssertUnwindSafe because Worker contains mutable
                // references that don't implement UnwindSafe, but the cleanup code will still execute.
                // let w_ptr = &mut w as *mut Worker<'_, P, NUM_SEGS_P2>; // Already defined outside
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    unsafe { (*w_ptr).run() };
                }));
                if let Err(e) = result {
                    tracing::trace!("[Worker {}] Panicked: {:?}", worker_id, e);
                }
            });
            // w.run();

            // Worker exited - clean up counters
            service_clone.worker_actives[worker_id].store(0, Ordering::SeqCst);
        });

        // Store the join handle
        *self.worker_threads[worker_id]
            .lock()
            .expect("worker_threads lock poisoned") = Some(join);

        Ok(())
    }

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
    pub fn spawn<F, T>(
        self: &Arc<Self>,
        _type_id: TypeId,
        _location: &'static Location,
        future: F,
    ) -> Result<JoinHandle<T>, SpawnError>
    where
        F: IntoFuture<Output = T> + Send + 'static,
        F::IntoFuture: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let arena = self.arena();

        // Early return if executor is shutting down or arena is closed
        // This prevents spawning new tasks during shutdown
        if self.shutdown.load(Ordering::Acquire) || arena.is_closed() {
            return Err(SpawnError::Closed);
        }

        // Reserve a task slot from the worker service
        // Returns None if no capacity is available
        let Some(handle) = self.reserve_task() else {
            return Err(if arena.is_closed() {
                SpawnError::Closed
            } else {
                SpawnError::NoCapacity
            });
        };

        // Calculate the global task ID and initialize the task in the arena
        // The summary pointer is used for task tracking and statistics
        let global_id = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global_id, self.summary() as *const _);

        // Get the task reference and prepare handles for cleanup
        let task = handle.task();
        let task_handle = TaskHandle::from_non_null(handle.as_non_null());

        // Create pointer for the JoinableFuture (will be set after allocation)
        let service_for_future = Arc::clone(self);
        let release_handle = task_handle;

        // Create the wrapper future that:
        // 1. Awaits the user's future
        // 2. Stores result via JoinState::complete()
        // 3. Releases the task handle back to the worker service
        let user_future = future.into_future();
        let wrapper_future = async move {
            let result = user_future.await;
            // The JoinableFuture pointer is stored in the task's future_ptr.
            // JoinState is at offset sizeof(FutureHeader) from the start.
            let task = release_handle.task();
            let base_ptr = task.future_ptr() as *const u8;
            let join_state_ptr = unsafe {
                base_ptr.add(std::mem::size_of::<FutureHeader>()) as *const JoinState<T>
            };
            // Safety: We know this points to a JoinState<T> after the header
            unsafe { (*join_state_ptr).complete(result) };
            service_for_future.release_task(release_handle);
        };

        // Single allocation: Box::pin the entire JoinableFuture (join state + future inline)
        let joinable = Box::pin(JoinableFuture::new(wrapper_future));

        // Get the pointer to the join state (at the start of the pinned allocation)
        let join_state_ptr = joinable.join_state() as *const JoinState<T>;

        // Convert to raw pointer for storage in task
        // Safety: We're unpinning to get the raw pointer, but the data stays pinned in memory
        let future_ptr = unsafe { Box::into_raw(Pin::into_inner_unchecked(joinable)) as *mut () };

        // Attach the future to the task
        // If attachment fails, clean up the allocated future and release the task handle
        if task.attach_future(future_ptr).is_err() {
            unsafe { drop_future(future_ptr) };
            self.release_task(task_handle);
            return Err(SpawnError::AttachFailed);
        }

        // Schedule the task for execution by worker threads
        task.schedule();

        // Return a join handle that points to the JoinState
        Ok(JoinHandle::new(join_state_ptr, Arc::clone(self)))
    }

    pub(crate) fn post_message(
        &self,
        from_worker_id: u32,
        to_worker_id: u32,
        message: WorkerMessage,
    ) -> Result<(), PushError<WorkerMessage>> {
        // mpsc will automatically set bits in the target worker's WorkerWaker.summary
        unsafe {
            self.senders
                [((from_worker_id as usize) * self.config.worker_count) + (to_worker_id as usize)]
                .unsafe_try_push(message)
        }
    }

    pub(crate) fn try_yield_steal(
        &self,
        from_worker_id: usize,
        limit: usize,
        next_rand: usize,
    ) -> (u64, bool) {
        let queue = &self.yield_queues[from_worker_id];
        let stealer = &self.yield_stealers[from_worker_id];
        let max_workers = self.config.worker_count;
        let mut index = next_rand;
        let mut attempts = 0;
        for _ in 0..max_workers {
            let worker_id = index % max_workers;
            if worker_id == from_worker_id {
                continue;
            }
            let steal = stealer.steal_batch_with_limit(queue, limit);
            if steal.is_success() {
                return (attempts, true);
            }
            index += 1;
            attempts += 1;
        }
        (attempts, false)
    }

    pub fn tick_duration(&self) -> Duration {
        self.tick_duration
    }

    pub fn clock_ns(&self) -> u64 {
        self.clock_ns.load(Ordering::Acquire)
    }

    pub fn shutdown(&self) {
        *self.tick_registration.lock().expect("lock poisoned") = None;
        if self.shutdown.swap(true, Ordering::Release) {
            return;
        }

        tracing::trace!("[WorkerService] Shutdown initiated, sending shutdown messages...");

        // Send shutdown messages to all active workers
        let worker_count = self.config.worker_count;
        for worker_id in 0..worker_count {
            if self.worker_actives[worker_id].load(Ordering::Relaxed) != 0 {
                // SAFETY: unsafe_try_push only requires a shared reference
                let _ = unsafe {
                    self.tick_senders[worker_id].unsafe_try_push(WorkerMessage::Shutdown)
                };
                // Mark work available to wake up the worker
                self.wakers[worker_id].mark_tasks();
            }
        }

        tracing::trace!("[WorkerService] Joining worker threads...");

        // Join all worker threads
        for (idx, worker_thread) in self.worker_threads.iter().enumerate() {
            if let Some(handle) = worker_thread.lock().unwrap().take() {
                tracing::trace!("[WorkerService] Joining worker thread {}...", idx);
                let _ = handle.join();
                tracing::trace!("[Worker Service] Worker thread {} joined", idx);
            }
        }

        // Clear thread handles after joining threads to ensure proper cleanup order
        // This is especially important on Windows where handles need to be closed
        // after the threads have finished executing
        for thread_handle_mutex in self.worker_thread_handles.iter() {
            let _ = thread_handle_mutex.lock().unwrap().take();
        }

        tracing::trace!("[WorkerService] Shutdown complete");
    }

    /// Returns the current worker count
    #[inline]
    pub fn worker_count(&self) -> usize {
        self.config.worker_count
    }

    /// Checks if a specific worker has any work to do
    #[inline]
    pub fn worker_has_work(&self, worker_id: usize) -> bool {
        if worker_id >= self.wakers.len() {
            return false;
        }
        let waker = &self.wakers[worker_id];
        let summary = waker.snapshot_summary();
        let status = waker.status();
        summary != 0 || status != 0
    }

    /// Supervisor: Health monitoring - check for stuck workers and collect metrics
    fn supervisor_health_check(&self, now_ns: u64) {
        let worker_count = self.config.worker_count;
        let tick_duration_ns = self.tick_duration_ns;
        let stuck_threshold_ns = tick_duration_ns * 10; // Worker is stuck if no progress for 10 ticks

        for worker_id in 0..worker_count {
            let is_active = self.worker_actives[worker_id].load(Ordering::Relaxed);
            if is_active == 0 {
                continue; // Worker slot not active
            }

            let last_update = self.worker_now_ns[worker_id].load(Ordering::Relaxed);
            let time_since_update = now_ns.saturating_sub(last_update);

            // Detect stuck workers
            if time_since_update > stuck_threshold_ns {
                #[cfg(debug_assertions)]
                {
                    tracing::trace!(
                        "[Supervisor] WARNING: Worker {} appears stuck (no update for {}ns)",
                        worker_id,
                        time_since_update
                    );
                }
            }

            // Request health report from active workers
            if worker_id < self.tick_senders.len() {
                // SAFETY: try_push only requires a shared reference, not a mutable one
                let _ = unsafe {
                    self.tick_senders[worker_id].unsafe_try_push(WorkerMessage::ReportHealth)
                };
            }
        }
    }

    /// Supervisor: Graceful shutdown coordination
    fn supervisor_graceful_shutdown(&self) {
        let _ = self.tick_registration.lock().unwrap().take();
        let worker_count = self.config.worker_count;

        tracing::trace!("[Supervisor] Shutting down, worker_count={}", worker_count);

        // Send graceful shutdown to all active workers
        for worker_id in 0..worker_count {
            #[cfg(debug_assertions)]
            {
                let is_active = self.worker_actives[worker_id].load(Ordering::Relaxed) != 0;
                tracing::trace!("[Supervisor] Worker {} active={}", worker_id, is_active);
            }
            if self.worker_actives[worker_id].load(Ordering::Relaxed) != 0 {
                #[cfg(debug_assertions)]
                {
                    tracing::trace!("[Supervisor] Sending shutdown to worker {}", worker_id);
                }
                // SAFETY: unsafe_try_push only requires a shared reference
                let _ = unsafe {
                    self.tick_senders[worker_id].unsafe_try_push(WorkerMessage::GracefulShutdown)
                };
                // Mark work available to wake up the worker
                self.wakers[worker_id].mark_tasks();
            }
        }

        tracing::trace!("[Supervisor] Sent graceful shutdown to all workers");

        // Wait for workers to finish (with timeout)
        let shutdown_timeout = std::time::Duration::from_secs(5);
        let shutdown_start = std::time::Instant::now();

        loop {
            // Count active workers by checking worker_actives
            let mut active_count = 0;
            for i in 0..worker_count {
                if self.worker_actives[i].load(Ordering::Relaxed) != 0 {
                    active_count += 1;
                }
            }

            if active_count == 0 {
                break;
            }

            if shutdown_start.elapsed() > shutdown_timeout {
                #[cfg(debug_assertions)]
                {
                    tracing::trace!(
                        "[Supervisor] Graceful shutdown timeout, {} workers still active",
                        active_count
                    );
                }
                // Force shutdown remaining workers
                for worker_id in 0..worker_count {
                    if self.worker_actives[worker_id].load(Ordering::Relaxed) != 0 {
                        // SAFETY: unsafe_try_push only requires a shared reference
                        let _ = unsafe {
                            self.tick_senders[worker_id].unsafe_try_push(WorkerMessage::Shutdown)
                        };
                    }
                }
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Interrupt a specific worker thread to trigger preemptive generator switching.
    ///
    /// **The worker thread is NOT terminated!** It is only briefly interrupted to:
    /// 1. Pin its current generator to the running task (save stack state)
    /// 2. Create a new generator for itself
    /// 3. Continue executing with the new generator
    ///
    /// The interrupted task can later be resumed with its pinned generator.
    /// This allows true preemptive multitasking at the task level.
    ///
    /// Can be called from any thread (e.g., a timer thread).
    ///
    /// # Arguments
    /// * `worker_id` - The ID of the worker to interrupt (0-based index)
    ///
    /// # Returns
    /// * `Ok(true)` - Worker was successfully interrupted
    /// * `Ok(false)` - Worker doesn't exist (invalid ID)
    /// * `Err` - Interrupt operation failed (platform error)
    pub fn interrupt_worker(
        &self,
        worker_id: usize,
    ) -> Result<bool, crate::runtime::preemption::PreemptionError> {
        if worker_id >= self.worker_thread_handles.len() {
            return Ok(false);
        }

        let handle_guard = self.worker_thread_handles[worker_id]
            .lock()
            .expect("worker_thread_handles lock poisoned");

        if let Some(ref handle) = *handle_guard {
            handle.interrupt()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl WorkerBus for WorkerService {
    fn post_cancel_message(&self, from_worker_id: u32, to_worker_id: u32, timer_id: u64) -> bool {
        self.post_message(
            from_worker_id,
            to_worker_id,
            WorkerMessage::CancelTimer {
                worker_id: to_worker_id,
                timer_id,
            },
        )
        .is_ok()
    }
}

impl TickHandler for WorkerService {
    fn tick_duration(&self) -> Duration {
        self.tick_duration
    }

    fn on_tick(&self, tick_count: u64, now_ns: u64) {
        // Update clock for all workers
        self.clock_ns.store(now_ns, Ordering::Release);
        let worker_count = self.config.worker_count;
        for i in 0..worker_count {
            if i >= self.worker_now_ns.len() {
                break;
            }
            self.worker_now_ns[i].store(now_ns, Ordering::Release);
            // Update TimerWheel's now_ns
            unsafe {
                // SAFETY: Each worker has exclusive access to its own TimerWheel.
                // The tick thread is the only one updating now_ns on TimerWheels.
                let timer_wheel = &mut *self.timers[i].get();
                timer_wheel.set_now_ns(now_ns);
            }
        }

        // Periodic health monitoring
        if tick_count % self.tick_health_check_interval == 0 {
            self.supervisor_health_check(now_ns);
        }
    }

    fn on_shutdown(&self) {
        // Graceful shutdown: notify all workers
        self.supervisor_graceful_shutdown();
    }
}

impl Drop for WorkerService {
    fn drop(&mut self) {
        *self.tick_registration.lock().expect("lock poisoned") = None;
        if !self.shutdown.load(Ordering::Acquire) {
            self.shutdown();
        }
    }
}

fn normalize_tick_duration_ns(duration: Duration) -> u64 {
    let nanos = duration.as_nanos().max(1).min(u128::from(u64::MAX)) as u64;
    nanos.next_power_of_two()
}

/// Comprehensive statistics for fast worker.
#[derive(Debug, Default, Clone)]
pub struct WorkerStats {
    /// Number of tasks polled.
    pub tasks_polled: u64,

    /// Number of tasks that completed.
    pub completed_count: u64,

    /// Number of tasks that yielded cooperatively.
    pub yielded_count: u64,

    /// Number of tasks that are waiting (Poll::Pending, not yielded).
    pub waiting_count: u64,

    /// Number of CAS failures (contention or already executing).
    pub cas_failures: u64,

    /// Number of empty scans (no tasks available).
    pub empty_scans: u64,

    /// Number of polls from yield queue (hot path).
    pub yield_queue_polls: u64,

    /// Number of polls from signals (cold path).
    pub signal_polls: u64,

    /// Number of work stealing attempts from other workers.
    pub steal_attempts: u64,

    /// Number of successful work steals.
    pub steal_successes: u64,

    /// Number of leaf summary checks.
    pub leaf_summary_checks: u64,

    /// Number of leaf summary hits (summary != 0).
    pub leaf_summary_hits: u64,

    /// Number of attempts to steal from other workers' leaf partitions.
    pub leaf_steal_attempts: u64,

    /// Number of successful steals from other workers' leaf partitions.
    pub leaf_steal_successes: u64,

    pub timer_fires: u64,
}

/// Health snapshot of a worker at a point in time.
/// Used by the supervisor for health monitoring and load balancing decisions.
#[derive(Debug, Clone)]
pub struct WorkerHealthSnapshot {
    pub worker_id: u32,
    pub timestamp_ns: u64,
    pub stats: WorkerStats,
    pub yield_queue_len: usize,
    pub mpsc_queue_len: usize,
    pub active_leaf_partitions: Vec<usize>,
    pub has_work: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WakeStats {
    pub release_calls: u64,
    pub released_permits: u64,
    pub last_backlog: usize,
    pub max_backlog: usize,
    pub queue_release_calls: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct WaitStrategy {
    pub spin_before_sleep: usize,
    pub park_timeout: Option<Duration>,
}

impl WaitStrategy {
    pub fn new(spin_before_sleep: usize, park_timeout: Option<Duration>) -> Self {
        Self {
            spin_before_sleep,
            park_timeout,
        }
    }

    pub fn non_blocking() -> Self {
        Self::new(0, Some(Duration::from_secs(0)))
    }

    pub fn park_immediately() -> Self {
        Self::new(0, None)
    }
}

impl Default for WaitStrategy {
    fn default() -> Self {
        Self::new(0, Some(Duration::from_millis(2000)))
    }
}

pub struct WorkerInner<'a> {
    wait_strategy: WaitStrategy,
    shutdown: &'a AtomicBool,
    yield_queue: &'a YieldWorker<TaskHandle>,
    timer_wheel: &'a mut TimerWheel<TimerHandle>,
    pub(crate) bus: &'static dyn WorkerBus,
    wake_stats: WakeStats,
    stats: WorkerStats,
    partition_start: usize,
    partition_end: usize,
    partition_len: usize,
    cached_worker_count: usize,
    wake_burst_limit: usize,
    worker_id: u32,
    timer_resolution_ns: u64,
    timer_output: Vec<(u64, u64, TimerHandle)>,
    message_batch: Box<[WorkerMessage]>,
    rng: crate::utils::Random,
    current_task: *mut Task,
    current_scope: *mut (),
    /// Preemption flag for this worker - set by signal handler (Unix) or timer thread (Windows)
    preemption_requested: AtomicBool,
    /// Per-worker EventLoop for async I/O operations (boxed to avoid stack overflow)
    io: Box<crate::runtime::io_driver::IoDriver>,
    /// I/O budget per run_once iteration (prevents I/O from starving tasks)
    io_budget: usize,
}

pub struct Worker<'a> {
    service: Arc<WorkerService>,
    receiver: &'a mut mpsc::Receiver<WorkerMessage>,
    inner: WorkerInner<'a>,
}

impl Drop for Worker<'_> {
    /// Cleans up the worker when it goes out of scope.
    ///
    /// The EventLoop owns its Waker, so proper drop order is ensured:
    /// Waker drops before Poll when EventLoop drops.
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            tracing::trace!("[Worker {}] Dropped", self.inner.worker_id);
        }
    }
}

/// Reason why a generator was pinned to a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinReason {
    /// Explicit `pin_stack()` call from task code.
    Explicit,
    /// Signal-based preemption (Unix signal or Windows APC).
    Preempted,
}

/// Context tracking generator state during worker execution.
///
/// This struct captures whether the worker's generator was pinned to a task
/// and provides information needed for proper cleanup and error handling.
///
/// Note: Panic handling is delegated to the generator's own mechanism (context.err).
/// We don't use catch_unwind around generator.resume() because it interferes with
/// the generator's context switching. Panics inside generators will propagate up
/// through the generator's error handling.
struct RunContext {
    /// Generator was pinned to a task - worker must create new generator.
    pinned: bool,

    /// How the generator was pinned (if `pinned` is true).
    pin_reason: Option<PinReason>,

    /// Task completed while generator was pinned (vs still pending).
    /// When true, the pinned generator can be cleaned up immediately.
    /// When false, the task will continue with its pinned generator.
    task_completed: bool,
}

impl RunContext {
    fn new() -> Self {
        Self {
            pinned: false,
            pin_reason: None,
            task_completed: false,
        }
    }

    /// Reset context for next iteration (when not pinned).
    #[inline]
    fn reset(&mut self) {
        self.pinned = false;
        self.pin_reason = None;
        self.task_completed = false;
    }

    /// Mark that the generator was pinned due to preemption.
    #[inline]
    fn mark_preempted(&mut self) {
        self.pinned = true;
        self.pin_reason = Some(PinReason::Preempted);
    }

    /// Mark that the generator was pinned explicitly via `pin_stack()`.
    #[inline]
    fn mark_explicit_pin(&mut self) {
        self.pinned = true;
        self.pin_reason = Some(PinReason::Explicit);
    }

    /// Mark that the task completed while pinned.
    #[inline]
    fn mark_task_completed(&mut self) {
        self.task_completed = true;
    }

    /// Check if we should exit the generator loop.
    #[inline]
    fn should_exit_generator(&self) -> bool {
        self.pinned
    }
}

impl<'a> Worker<'a> {
    #[inline]
    pub fn stats(&self) -> &WorkerStats {
        &self.inner.stats
    }

    /// Checks if this worker has any work to do (tasks, yields, or messages).
    /// Returns true if the worker should continue running, false if it can park.
    #[inline]
    fn has_work(&self) -> bool {
        let waker = &self.service.wakers[self.inner.worker_id as usize];

        // Check status bits (fast path):
        // - bit 63: yield queue has items
        // - bit 62: partition cache reports tasks (synced from SummaryTree)
        let status = waker.status();
        if status != 0 {
            return true;
        }

        // Check summary (cross-worker signals like messages, timers)
        let summary = waker.snapshot_summary();
        if summary != 0 {
            return true;
        }

        // Check if permits are available (missed wake scenario)
        if waker.permits() > 0 {
            return true;
        }

        false
    }

    pub fn set_wait_strategy(&mut self, strategy: WaitStrategy) {
        self.inner.wait_strategy = strategy;
    }

    #[inline(always)]
    fn run_once(&mut self, run_ctx: &mut RunContext) -> bool {
        let mut did_work = false;

        if self.poll_io() > 0 {
            did_work = true;
        }

        if self.poll_timers() {
            did_work = true;
        }

        // Process messages first (including shutdown signals)
        if self.process_messages() {
            did_work = true;
        }

        // Poll all yielded tasks first - they're ready to run
        let yielded = self.poll_yield(run_ctx, self.inner.yield_queue.len());
        if yielded > 0 {
            did_work = true;
        }
        if run_ctx.should_exit_generator() {
            return did_work;
        }

        // Partition assignment is handled by WorkerService via RebalancePartitions message
        if self.try_partition_random(run_ctx) {
            did_work = true;
        }
        if run_ctx.should_exit_generator() {
            return did_work;
        }

        // if self.try_partition_random(leaf_count) {
        //     did_work = true;
        // } else if self.try_partition_linear(leaf_count) {
        //     did_work = true;
        // }

        let rand = self.next_u64();

        if !did_work && rand & FULL_SUMMARY_SCAN_CADENCE_MASK == 0 {
            let leaf_count = self.service.arena().leaf_count();
            if self.try_any_partition_random(run_ctx, leaf_count) {
                did_work = true;
            }
            if run_ctx.should_exit_generator() {
                return did_work;
            }
            // else if self.try_any_partition_linear(leaf_count) {
            // did_work = true;
            // }
        }

        if !did_work {
            if self.poll_yield(run_ctx, self.inner.yield_queue.len() as usize) > 0 {
                did_work = true;
            }
            if run_ctx.should_exit_generator() {
                return did_work;
            }

            // if self.poll_yield_steal(1) > 0 {
            //     did_work = true;
            // }
        }

        did_work
    }

    #[inline(always)]
    fn run_once_exhaustive(&mut self, run_ctx: &mut RunContext) -> bool {
        let mut did_work = false;

        if self.poll_timers() {
            did_work = true;
        }

        // Process messages first (including shutdown signals)
        if self.process_messages() {
            did_work = true;
        }

        // Poll all yielded tasks first - they're ready to run
        let yielded = self.poll_yield(run_ctx, self.inner.yield_queue.len());
        if yielded > 0 {
            did_work = true;
        }
        if run_ctx.should_exit_generator() {
            return did_work;
        }

        // Partition assignment is handled by WorkerService via RebalancePartitions message
        if self.try_partition_random(run_ctx) {
            did_work = true;
        }
        if run_ctx.should_exit_generator() {
            return did_work;
        }

        if self.try_partition_random(run_ctx) {
            did_work = true;
        } else if self.try_partition_linear(run_ctx) {
            did_work = true;
        }
        if run_ctx.should_exit_generator() {
            return did_work;
        }

        if !did_work {
            let leaf_count = self.service.arena().leaf_count();
            if self.try_any_partition_random(run_ctx, leaf_count) {
                did_work = true;
            } else if self.try_any_partition_linear(run_ctx, leaf_count) {
                did_work = true;
            }
            if run_ctx.should_exit_generator() {
                return did_work;
            }
        }

        if !did_work {
            if self.poll_yield(run_ctx, self.inner.yield_queue.len() as usize) > 0 {
                did_work = true;
            }
            if run_ctx.should_exit_generator() {
                return did_work;
            }
            if self.poll_yield_steal(run_ctx, 1) > 0 {
                did_work = true;
            }
            if run_ctx.should_exit_generator() {
                return did_work;
            }
        }

        did_work
    }

    #[inline]
    fn run_last_before_park(&mut self) -> bool {
        false
    }

    pub(crate) fn run(&mut self) {
        const STACK_SIZE: usize = 2 * 1024 * 1024; // 2MB stack per generator

        // Outer loop: manages generator lifecycle
        // Creates new generators when current one gets pinned
        while !self.inner.shutdown.load(Ordering::Relaxed) {
            // Use raw pointer for generator closure
            let worker_ptr = self as *mut Self;

            // EXTREMELY UNSAFE: Convert pointer to usize to completely erase type and make it Send
            // This is safe because Worker is single-threaded and we never actually send across threads
            let worker_addr = worker_ptr as usize;

            let mut generator =
                crate::generator::Gn::<()>::new_scoped_opt(STACK_SIZE, move |mut scope| -> usize {
                    // Wrap in catch_unwind to ensure clean stack behavior
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // SAFETY: worker_addr is the address of a valid Worker that lives for the generator's lifetime
                        let worker = unsafe { &mut *(worker_addr as *mut Worker<'_>) };
                        let mut spin_count = 0;
                        let waker_id = worker.inner.worker_id as usize;

                        // Register this generator's scope for non-cooperative preemption
                        let scope_ptr = &mut scope as *mut _ as *mut ();

                        // Store in thread-local for signal handler
                        worker.inner.current_scope = scope_ptr;

                        let mut run_ctx = RunContext::new();

                        // Inner loop: runs inside generator context
                        loop {
                            // Check for shutdown
                            if worker.inner.shutdown.load(Ordering::Relaxed) {
                                worker.inner.current_scope = core::ptr::null_mut();
                                return 0;
                            }

                            // Process one iteration of work
                            let mut progress = worker.run_once(&mut run_ctx);

                            // Check if we need to exit the generator
                            if run_ctx.should_exit_generator() {
                                worker.inner.current_scope = core::ptr::null_mut();

                                // Log pin reason for debugging
                                #[cfg(debug_assertions)]
                                {
                                    if let Some(reason) = run_ctx.pin_reason {
                                        tracing::trace!(
                                            "[Worker {}] Generator pinned: reason={:?}, task_completed={}",
                                            worker.inner.worker_id,
                                            reason,
                                            run_ctx.task_completed
                                        );
                                    }
                                }

                                // Yield with appropriate reason based on how we got here
                                let yield_reason = if run_ctx.pin_reason == Some(PinReason::Preempted) {
                                    crate::runtime::preemption::GeneratorYieldReason::Preempted
                                } else {
                                    crate::runtime::preemption::GeneratorYieldReason::Cooperative
                                };
                                scope.yield_(yield_reason.as_usize());

                                // Return 0 for normal exit (pinned or shutdown)
                                return 0;
                            }

                            if !progress {
                                spin_count += 1;

                                if spin_count >= worker.inner.wait_strategy.spin_before_sleep {
                                    core::hint::spin_loop();
                                    progress = worker.run_once_exhaustive(&mut run_ctx);

                                    if progress {
                                        spin_count = 0;
                                        continue;
                                    }
                                } else if spin_count < worker.inner.wait_strategy.spin_before_sleep
                                {
                                    core::hint::spin_loop();
                                    continue;
                                }

                                // Calculate park duration considering timer deadlines
                                let park_duration = worker.calculate_park_duration();

                                // Sync partition summary from SummaryTree before parking
                                worker.service.wakers[waker_id].sync_partition_summary(
                                    worker.inner.partition_start,
                                    worker.inner.partition_end,
                                    &worker.service.summary().leaf_words,
                                );

                                // Park on WorkerWaker with timer-aware timeout
                                // On Windows, also use alertable wait for APC-based preemption
                                match park_duration {
                                    Some(duration) if duration.is_zero() => {}
                                    Some(duration) => {
                                        // On Windows, WorkerWaker uses alertable waits to allow APC execution
                                        worker.service.wakers[waker_id].acquire_timeout_with_io(
                                            &mut worker.inner.io,
                                            duration,
                                        );
                                    }
                                    None => {
                                        // On Windows, WorkerWaker uses alertable waits to allow APC execution
                                        worker.service.wakers[waker_id].acquire_timeout_with_io(
                                            &mut worker.inner.io,
                                            Duration::from_millis(250),
                                        );
                                    }
                                }
                                spin_count = 0;
                            } else {
                                spin_count = 0;
                            }
                        }
                    }));

                    match result {
                        Ok(r) => r,
                        Err(panic_payload) => {
                            // Log the panic for debugging
                            #[cfg(debug_assertions)]
                            {
                                if let Some(s) = panic_payload.downcast_ref::<&str>() {
                                    tracing::error!("Worker generator panicked: {}", s);
                                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                                    tracing::error!("Worker generator panicked: {}", s);
                                } else {
                                    tracing::error!("Worker generator panicked with unknown payload");
                                }
                            }
                            // Return 2 to indicate panic occurred
                            2
                        }
                    }
                });

            // Drive the generator - it will run until:
            // 1. Shutdown is requested
            // 2. A task's generator gets pinned (run_ctx.pinned becomes true)
            // 3. Preemption occurs (signal handler pins generator and yields)
            generator.resume();

            // After resume returns, check if preemption occurred.
            //
            // current_task is only non-null here if preemption happened:
            // - Normal exit: ActiveTaskGuard clears current_task when poll returns
            // - Cooperative pin: poll completes, guard clears current_task, then yields
            // - Preemption: signal fires mid-poll, yields BEFORE poll returns, guard hasn't run
            //
            // rust_preemption_helper already pinned the generator (scope pointer) to the task.
            // We just need to:
            // 1. Forget our generator variable (so it doesn't get dropped - task owns it now)
            // 2. Re-enqueue the task to the yield queue
            let task_ptr = self.inner.current_task;
            if !task_ptr.is_null() {
                // Preemption occurred - task owns the generator now
                let task = unsafe { &*task_ptr };
                std::mem::forget(generator);

                // Re-enqueue the preempted task to the yield queue
                // Preempted tasks are "hot" and should run again soon
                task.mark_yielded();
                task.record_yield();
                let handle = TaskHandle::from_task(task);
                self.inner.yield_queue.push(handle);
                self.service.wakers[self.inner.worker_id as usize].mark_yield();
                self.inner.stats.yielded_count += 1;

                #[cfg(debug_assertions)]
                tracing::trace!(
                    "[Worker {}] Preempted task re-enqueued to yield queue",
                    self.inner.worker_id
                );

                // Clear current_task since we've handled it
                self.inner.current_task = ptr::null_mut();

                // Continue to create a new generator for the worker
                continue;
            }
        }

        tracing::trace!("Worker is done running: {}", self.inner.worker_id);
    }

    /// Calculate how long the worker can park, considering both the wait strategy
    /// and the next timer deadline.
    #[inline]
    fn calculate_park_duration(&mut self) -> Option<Duration> {
        // Check if we have pending timers
        if let Some(next_deadline_ns) = self.inner.timer_wheel.next_deadline() {
            let now_ns = self.inner.timer_wheel.now_ns();

            if next_deadline_ns > now_ns {
                let timer_duration_ns = next_deadline_ns - now_ns;
                let timer_duration = Duration::from_nanos(timer_duration_ns);

                // Use the minimum of wait_strategy timeout and timer deadline
                let duration = match self.inner.wait_strategy.park_timeout {
                    Some(strategy_timeout) => Some(strategy_timeout.min(timer_duration)),
                    None => Some(timer_duration),
                };
                return duration;
            } else {
                // Timer already expired, don't sleep at all
                return Some(Duration::ZERO);
            }
        }

        // No timers scheduled - cap at 250ms to check for new timers periodically
        const MAX_PARK_DURATION: Duration = Duration::from_millis(250);
        match self.inner.wait_strategy.park_timeout {
            Some(timeout) => Some(timeout.min(MAX_PARK_DURATION)),
            None => Some(MAX_PARK_DURATION),
        }
    }

    fn process_messages(&mut self) -> bool {
        let mut progress = false;
        loop {
            match self.receiver.try_pop() {
                Ok(message) => {
                    if self.handle_message(message) {
                        progress = true;
                    }
                }
                Err(PopError::Empty) | Err(PopError::Timeout) => {
                    // Queue is empty - mpsc signal bits will be cleared automatically
                    break;
                }
                Err(PopError::Closed) => break,
            }
        }
        progress
    }

    #[inline(always)]
    fn handle_message(&mut self, message: WorkerMessage) -> bool {
        match message {
            WorkerMessage::ScheduleTimer { timer } => self.handle_timer_schedule(timer),
            WorkerMessage::ScheduleBatch { timers } => {
                let mut scheduled = false;
                for timer in timers.into_vec() {
                    scheduled |= self.handle_timer_schedule(timer);
                }
                scheduled
            }
            WorkerMessage::CancelTimer {
                worker_id,
                timer_id,
            } => self.cancel_remote_timer(worker_id, timer_id),
            WorkerMessage::ReportHealth => {
                self.handle_report_health();
                true
            }
            WorkerMessage::GracefulShutdown => {
                self.handle_graceful_shutdown();
                true
            }
            WorkerMessage::Shutdown => {
                self.inner.shutdown.store(true, Ordering::Release);
                true
            }
            WorkerMessage::Noop => true,
        }
    }

    fn handle_timer_schedule(&mut self, timer: TimerSchedule) -> bool {
        let (handle, deadline_ns) = timer.into_parts();
        if handle.worker_id() != self.inner.worker_id {
            return false;
        }
        self.enqueue_timer_entry(deadline_ns, handle).is_some()
    }

    fn handle_report_health(&mut self) {
        // Capture current state snapshot
        let snapshot = WorkerHealthSnapshot {
            worker_id: self.inner.worker_id,
            timestamp_ns: self.inner.timer_wheel.now_ns(),
            stats: self.inner.stats.clone(),
            yield_queue_len: self.inner.yield_queue.len(),
            mpsc_queue_len: 0, // TODO: Add len() method to mpsc::Receiver
            active_leaf_partitions: (self.inner.partition_start..self.inner.partition_end)
                .collect(),
            has_work: self.has_work(),
        };

        // For now, just log the health snapshot
        // TODO: Send this back to supervisor via a response channel
        tracing::trace!(
            "[Worker {}] Health: tasks_polled={}, yield_queue={}, mpsc_queue={}, partitions={:?}, has_work={}",
            snapshot.worker_id,
            snapshot.stats.tasks_polled,
            snapshot.yield_queue_len,
            snapshot.mpsc_queue_len,
            snapshot.active_leaf_partitions,
            snapshot.has_work
        );
        let _ = snapshot; // Suppress unused warning in release builds
    }

    fn handle_graceful_shutdown(&mut self) {
        // Graceful shutdown: just process a few more iterations then shutdown
        // The worker loop will naturally drain queues during normal operation

        #[cfg(debug_assertions)]
        {
            tracing::trace!(
                "[Worker {}] Received graceful shutdown signal",
                self.inner.worker_id
            );
        }

        // Process any remaining messages
        loop {
            match self.receiver.try_pop() {
                Ok(message) => match message {
                    WorkerMessage::Shutdown | WorkerMessage::GracefulShutdown => break,
                    _ => {
                        self.handle_message(message);
                    }
                },
                Err(_) => break,
            }
        }

        // Set shutdown flag - worker loop will finish current work before exiting
        self.inner.shutdown.store(true, Ordering::Release);

        #[cfg(debug_assertions)]
        {
            tracing::trace!("[Worker {}] Set shutdown flag", self.inner.worker_id);
        }
    }

    #[inline]
    fn poll_io(&mut self) -> usize {
        // tracing::trace!(
        //     "[Worker {}] Polling event loop. Sockets: {}",
        //     self.inner.worker_id,
        //     self.inner.io.socket_count()
        // );
        match self.inner.io.poll_once(Some(std::time::Duration::ZERO)) {
            Ok(num_events) => {
                if num_events > 0 {
                    tracing::trace!(
                        "[Worker {}] Poll got {} events",
                        self.inner.worker_id,
                        num_events
                    );
                }
                num_events
            }
            Err(err) => {
                log::error!("EventLoop error: {}", err);
                0
            }
        }
    }

    /// Schedule a timer directly at the given deadline (for cross-worker messages)
    fn enqueue_timer_entry(&mut self, deadline_ns: u64, handle: TimerHandle) -> Option<u64> {
        match self.inner.timer_wheel.schedule_timer(deadline_ns, handle) {
            Ok(timer_id) => Some(timer_id),
            Err(_) => None, // Log error in debug mode
        }
    }

    /// Schedule a timer using best-fit strategy
    /// Returns (timer_id, wheel_deadline_ns) if successful
    fn enqueue_timer_entry_best_fit(
        &mut self,
        target_deadline_ns: u64,
        handle: TimerHandle,
    ) -> Option<(u64, u64)> {
        match self
            .inner
            .timer_wheel
            .schedule_timer_best_fit(target_deadline_ns, handle)
        {
            Ok((timer_id, wheel_deadline_ns)) => Some((timer_id, wheel_deadline_ns)),
            Err(_) => None, // Log error in debug mode
        }
    }

    fn poll_timers(&mut self) -> bool {
        let now_ns = self.inner.timer_wheel.now_ns();
        let expired =
            self.inner
                .timer_wheel
                .poll(now_ns, TIMER_EXPIRE_BUDGET, &mut self.inner.timer_output);
        if expired == 0 {
            return false;
        }

        let mut progress = false;
        for idx in 0..expired {
            let (timer_id, deadline_ns, handle) = self.inner.timer_output[idx];
            if self.process_timer_entry(timer_id, deadline_ns, handle) {
                progress = true;
            }
        }
        progress
    }

    fn process_timer_entry(
        &mut self,
        timer_id: u64,
        _deadline_ns: u64,
        handle: TimerHandle,
    ) -> bool {
        if handle.worker_id() != self.inner.worker_id {
            return false;
        }

        let slot = unsafe { handle.task_slot().as_ref() };
        let task_ptr = slot.task_ptr();
        if task_ptr.is_null() {
            return false;
        }

        let task = unsafe { &*task_ptr };
        if task.global_id() != handle.task_id() {
            return false;
        }

        task.schedule();
        // let state = task.state().load(Ordering::Acquire);
        //
        // if state == TASK_IDLE {
        //     if task
        //         .state()
        //         .compare_exchange(
        //             TASK_IDLE,
        //             TASK_EXECUTING,
        //             Ordering::AcqRel,
        //             Ordering::Acquire,
        //         )
        //         .is_ok()
        //     {
        //         task.schedule();
        //         // self.poll_task(TaskHandle::from_task(task), task);
        //     } else {
        //         task.schedule();
        //     }
        // } else if state == TASK_EXECUTING && !task.is_yielded() {
        //     task.schedule();
        // } else {
        //     task.schedule();
        // }
        self.inner.stats.timer_fires = self.inner.stats.timer_fires.saturating_add(1);
        true
    }

    fn cancel_timer(&mut self, timer: &Timer) -> bool {
        if !timer.is_scheduled() {
            return false;
        }

        let Some(worker_id) = timer.worker_id() else {
            return false;
        };

        let timer_id = timer.timer_id();
        if timer_id == 0 {
            return false;
        }

        if worker_id == self.inner.worker_id {
            if self
                .inner
                .timer_wheel
                .cancel_timer(timer_id)
                .is_ok()
            {
                timer.mark_cancelled(timer_id);
                return true;
            }
            return false;
        }

        self.service
            .post_cancel_message(self.inner.worker_id, worker_id, timer_id)
    }

    #[inline]
    fn cancel_remote_timer(&mut self, worker_id: u32, timer_id: u64) -> bool {
        if worker_id != self.inner.worker_id {
            return false;
        }

        self.service
            .post_cancel_message(self.inner.worker_id, worker_id, timer_id)
    }

    #[inline(always)]
    pub fn next_u64(&mut self) -> u64 {
        self.inner.rng.next()
    }

    #[inline(always)]
    fn poll_yield(&mut self, run_ctx: &mut RunContext, max: usize) -> usize {
        let mut count = 0;
        while let Some(handle) = self.try_acquire_local_yield() {
            self.inner.stats.yield_queue_polls += 1;
            self.poll_handle(run_ctx, handle);
            count += 1;
            if count >= max || run_ctx.pinned {
                break;
            }
        }
        count
    }

    #[inline(always)]
    fn poll_yield_steal(&mut self, run_ctx: &mut RunContext, max: usize) -> usize {
        let mut count = 0;
        while let Some(handle) = self.try_steal_yielded() {
            self.inner.stats.yield_queue_polls += 1;
            self.poll_handle(run_ctx, handle);
            count += 1;
            if count >= max || run_ctx.pinned {
                break;
            }
        }
        count
    }

    #[inline(always)]
    fn try_acquire_task(&mut self, leaf_idx: usize) -> Option<TaskHandle> {
        let signals_per_leaf = self.service.arena().signals_per_leaf();
        if signals_per_leaf == 0 {
            return None;
        }

        let mask = if signals_per_leaf >= 64 {
            u64::MAX
        } else {
            (1u64 << signals_per_leaf) - 1
        };

        self.inner.stats.leaf_summary_checks =
            self.inner.stats.leaf_summary_checks.saturating_add(1);
        let mut available =
            self.service.summary().leaf_words[leaf_idx].load(Ordering::Acquire) & mask;
        if available == 0 {
            self.inner.stats.empty_scans = self.inner.stats.empty_scans.saturating_add(1);
            return None;
        }
        self.inner.stats.leaf_summary_hits = self.inner.stats.leaf_summary_hits.saturating_add(1);

        let signals = self.service.arena().active_signals(leaf_idx);
        let mut attempts = signals_per_leaf;

        while available != 0 && attempts > 0 {
            let start = (self.next_u64() as usize) % signals_per_leaf;

            let (signal_idx, signal) = loop {
                let candidate = bits::find_nearest(available, start as u64);
                if candidate >= 64 {
                    self.inner.stats.leaf_summary_checks =
                        self.inner.stats.leaf_summary_checks.saturating_add(1);
                    available =
                        self.service.summary().leaf_words[leaf_idx].load(Ordering::Acquire) & mask;
                    if available == 0 {
                        self.inner.stats.empty_scans =
                            self.inner.stats.empty_scans.saturating_add(1);
                        return None;
                    }
                    self.inner.stats.leaf_summary_hits =
                        self.inner.stats.leaf_summary_hits.saturating_add(1);
                    continue;
                }
                let bit_index = candidate as usize;
                let sig = unsafe { &*self.service.arena().task_signal_ptr(leaf_idx, bit_index) };
                let bits = sig.load(Ordering::Acquire);
                if bits == 0 {
                    available &= !(1u64 << bit_index);
                    self.service
                        .summary()
                        .mark_signal_inactive(leaf_idx, bit_index);
                    attempts -= 1;
                    continue;
                }
                break (bit_index, sig);
            };

            let bits = signal.load(Ordering::Acquire);
            let bit_seed = (self.next_u64() & 63) as u64;
            let bit_candidate = bits::find_nearest(bits, bit_seed);
            let bit_idx = if bit_candidate < 64 {
                bit_candidate as u64
            } else {
                available &= !(1u64 << signal_idx);
                attempts -= 1;
                continue;
            };

            let (remaining, acquired) = signal.try_acquire(bit_idx);
            if !acquired {
                self.inner.stats.cas_failures = self.inner.stats.cas_failures.saturating_add(1);
                available &= !(1u64 << signal_idx);
                attempts -= 1;
                continue;
            }

            let remaining_mask = remaining;
            if remaining_mask == 0 {
                self.service
                    .summary()
                    .mark_signal_inactive(leaf_idx, signal_idx);
            }

            self.inner.stats.signal_polls = self.inner.stats.signal_polls.saturating_add(1);
            let slot_idx = signal_idx * 64 + bit_idx as usize;
            let task = unsafe { self.service.arena().task(leaf_idx, slot_idx) };
            return Some(TaskHandle::from_task(task));
        }

        self.inner.stats.empty_scans = self.inner.stats.empty_scans.saturating_add(1);
        None
    }

    #[inline(always)]
    fn enqueue_yield(&mut self, handle: TaskHandle) {
        let was_empty = self.inner.yield_queue.push_with_status(handle);
        if was_empty {
            // Set yield_bit in WorkerWaker status
            self.service.wakers[self.inner.worker_id as usize].mark_yield();
        }
    }

    #[inline(always)]
    fn try_acquire_local_yield(&mut self) -> Option<TaskHandle> {
        let (item, was_last) = self.inner.yield_queue.pop_with_status();
        if let Some(handle) = item {
            if was_last {
                // Clear yield_bit in WorkerWaker status
                self.service.wakers[self.inner.worker_id as usize].try_unmark_yield();
            }
            return Some(handle);
        }
        None
    }

    #[inline(always)]
    fn try_steal_yielded(&mut self) -> Option<TaskHandle> {
        let next_rand = self.next_u64();
        let (attempts, success) =
            self.service
                .try_yield_steal(self.inner.worker_id as usize, 1, next_rand as usize);
        if success {
            self.inner.stats.steal_attempts += attempts;
            self.inner.stats.steal_successes += 1;
            self.try_acquire_local_yield()
        } else {
            None
        }
    }

    #[inline(always)]
    fn process_leaf(&mut self, run_ctx: &mut RunContext, leaf_idx: usize) -> bool {
        if let Some(handle) = self.try_acquire_task(leaf_idx) {
            self.poll_handle(run_ctx, handle);
            return true;
        }
        false
    }

    #[inline(always)]
    fn try_partition_random(&mut self, run_ctx: &mut RunContext) -> bool {
        let partition_len = self.inner.partition_len;

        if partition_len.is_power_of_two() {
            let mask = partition_len - 1;
            for _ in 0..partition_len {
                let start_offset = self.next_u64() as usize & mask;
                let leaf_idx = self.inner.partition_start + start_offset;
                if self.process_leaf(run_ctx, leaf_idx) {
                    return true;
                }
            }
        } else {
            for _ in 0..partition_len {
                let start_offset = self.next_u64() as usize % partition_len;
                let leaf_idx = self.inner.partition_start + start_offset;
                if self.process_leaf(run_ctx, leaf_idx) {
                    return true;
                }
            }
        }

        false
    }

    #[inline(always)]
    fn try_partition_linear(&mut self, run_ctx: &mut RunContext) -> bool {
        let partition_start = self.inner.partition_start;
        let partition_len = self.inner.partition_len;

        for offset in 0..partition_len {
            let leaf_idx = partition_start + offset;
            if self.process_leaf(run_ctx, leaf_idx) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn try_any_partition_random(&mut self, run_ctx: &mut RunContext, leaf_count: usize) -> bool {
        if leaf_count.is_power_of_two() {
            let leaf_mask = leaf_count - 1;
            for _ in 0..leaf_count {
                let leaf_idx = self.next_u64() as usize & leaf_mask;
                self.inner.stats.leaf_steal_attempts += 1;
                if self.process_leaf(run_ctx, leaf_idx) {
                    self.inner.stats.leaf_steal_successes += 1;
                    return true;
                }
            }
        } else {
            for _ in 0..leaf_count {
                let leaf_idx = self.next_u64() as usize % leaf_count;
                self.inner.stats.leaf_steal_attempts += 1;
                if self.process_leaf(run_ctx, leaf_idx) {
                    self.inner.stats.leaf_steal_successes += 1;
                    return true;
                }
            }
        }
        false
    }

    #[inline(always)]
    fn try_any_partition_linear(&mut self, run_ctx: &mut RunContext, leaf_count: usize) -> bool {
        for leaf_idx in 0..leaf_count {
            self.inner.stats.leaf_steal_attempts += 1;
            if self.process_leaf(run_ctx, leaf_idx) {
                self.inner.stats.leaf_steal_successes += 1;
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn poll_handle(&mut self, run_ctx: &mut RunContext, handle: TaskHandle) {
        let task = handle.task();

        // Check if this task has a pinned generator
        if task.has_pinned_generator() {
            // Poll the task by resuming its pinned generator
            self.poll_task_with_pinned_generator(run_ctx, handle, task);
            return;
        }

        struct ActiveTaskGuard(*mut *mut Task);
        impl Drop for ActiveTaskGuard {
            fn drop(&mut self) {
                unsafe {
                    *(self.0) = core::ptr::null_mut();
                }
            }
        }
        self.inner.current_task = unsafe { task as *const _ as *mut Task };
        let _active_guard = ActiveTaskGuard(unsafe {
            &self.inner.current_task as *const *mut Task as *mut *mut Task
        });

        self.inner.stats.tasks_polled += 1;

        if !task.is_yielded() {
            task.begin();
        } else {
            task.record_yield();
            task.clear_yielded();
        }

        self.poll_task(run_ctx, handle, task);
    }

    /// Poll a task that has a pinned generator (or wants one).
    ///
    /// If the generator pointer is null but mode is Poll, we need to create a new
    /// poll-mode generator for the task. This happens when pin_stack() was called.
    fn poll_task_with_pinned_generator(
        &mut self,
        run_ctx: &mut RunContext,
        handle: TaskHandle,
        task: &Task,
    ) {
        struct ActiveTaskGuard(*mut *mut Task);
        impl Drop for ActiveTaskGuard {
            fn drop(&mut self) {
                unsafe {
                    *(self.0) = core::ptr::null_mut();
                }
            }
        }
        self.inner.current_task = unsafe { task as *const _ as *mut Task };
        let _active_guard = ActiveTaskGuard(unsafe {
            &self.inner.current_task as *const *mut Task as *mut *mut Task
        });

        self.inner.stats.tasks_polled += 1;

        let mode = task.generator_run_mode();

        // Check if we need to create a generator (null pointer with Poll mode)
        // This happens when pin_stack() was called - it sets mode to Poll but leaves
        // the pointer null as a signal that we need to create a dedicated generator.
        let mut guard = unsafe { task.get_pinned_generator() };
        let has_actual_generator = guard.generator().is_some();
        drop(guard); // Release the guard before proceeding

        if !has_actual_generator && mode == GeneratorRunMode::Poll {
            // Need to create a poll-mode generator for this task
            self.create_poll_mode_generator(run_ctx, handle, task);
            return;
        }

        match mode {
            GeneratorRunMode::Switch => {
                // Switch mode: Generator has interrupted poll on stack
                // Resume once to complete that poll, then transition to Poll mode or finish
                self.poll_task_switch_mode(run_ctx, handle, task);
            }
            GeneratorRunMode::Poll => {
                // Poll mode: Generator contains poll loop
                // Resume generator, it will call poll once and yield
                self.poll_task_poll_mode(run_ctx, handle, task);
            }
            GeneratorRunMode::None => {
                // No generator, shouldn't happen but treat as error
                task.finish();
                self.inner.stats.completed_count =
                    self.inner.stats.completed_count.saturating_add(1);
            }
        }
    }

    /// Handle Switch mode: resume generator once to complete interrupted poll,
    /// then transition to Poll mode or finish task.
    ///
    /// Switch mode means the generator was captured mid-poll (via preemption signal).
    /// We resume once to let the interrupted poll complete, then either:
    /// - Task completes: clean up generator
    /// - Task still pending: transition to Poll mode with a new task-owned generator
    ///
    /// Note: We don't use catch_unwind here because generators have their own panic
    /// handling mechanism via context.err that propagates panics correctly. Using
    /// catch_unwind interferes with the generator's context switching.
    fn poll_task_switch_mode(&mut self, run_ctx: &mut RunContext, handle: TaskHandle, task: &Task) {
        // Mark that we're handling a preempted generator
        run_ctx.mark_preempted();

        let mut guard = unsafe { task.get_pinned_generator() };
        if let Some(generator) = guard.generator() {
            // Resume generator once - this completes the interrupted poll
            match generator.resume() {
                Some(_) => {
                    // Generator yielded after completing poll
                    // Check if task still has a future (if not, task completed during poll)
                    let future_ptr = task.take_future();
                    guard.free();

                    if future_ptr.is_none() {
                        // Task completed during the interrupted poll, clean up generator
                        task.finish();
                        self.inner.stats.completed_count =
                            self.inner.stats.completed_count.saturating_add(1);
                        run_ctx.mark_task_completed();
                    } else {
                        // Task still pending - transition to Poll mode
                        // Discard the worker generator and create a task-specific Poll mode generator
                        self.create_poll_mode_generator(run_ctx, handle, task);
                    }
                }
                None => {
                    // Generator exhausted - task must be complete
                    guard.free();
                    task.finish();
                    self.inner.stats.completed_count =
                        self.inner.stats.completed_count.saturating_add(1);
                    run_ctx.mark_task_completed();
                }
            }
        } else {
            // Generator lost, treat as error
            task.finish();
            self.inner.stats.completed_count = self.inner.stats.completed_count.saturating_add(1);
            run_ctx.mark_task_completed();

            #[cfg(debug_assertions)]
            tracing::warn!(
                "[Worker {}] Switch mode task had no generator",
                self.inner.worker_id
            );
        }
    }

    /// Handle Poll mode: generator contains poll loop, resume to continue task.
    ///
    /// Poll mode means the task has its own dedicated generator with a poll loop.
    /// Each resume polls the future once and yields the result.
    ///
    /// Note: We don't use catch_unwind here because generators have their own panic
    /// handling mechanism via context.err that propagates panics correctly. Using
    /// catch_unwind interferes with the generator's context switching.
    fn poll_task_poll_mode(&mut self, run_ctx: &mut RunContext, handle: TaskHandle, task: &Task) {
        // Mark that we're handling an explicitly pinned generator
        run_ctx.mark_explicit_pin();

        let mut guard = unsafe { task.get_pinned_generator() };
        if let Some(generator) = guard.generator() {
            // Resume generator - it will poll once and yield the result
            match generator.resume() {
                Some(status) => {
                    // Generator yielded - check status and task state
                    if status == 1 || status == 2 {
                        // Status 1 = task complete, status 2 = future dropped
                        // Task completed, clean up generator
                        guard.free();
                        task.finish();
                        self.inner.stats.completed_count =
                            self.inner.stats.completed_count.saturating_add(1);
                        run_ctx.mark_task_completed();
                    } else if task.is_yielded() {
                        // Task yielded, re-enqueue
                        task.record_yield();
                        self.inner.stats.yielded_count += 1;
                        self.enqueue_yield(handle);
                        // Task still running, don't mark completed
                    } else {
                        // Task returned Pending, waiting for wake
                        // Don't re-enqueue, task will be woken when ready
                        self.inner.stats.waiting_count += 1;
                        task.finish();
                        // Task is waiting, not completed in the "done" sense
                    }
                }
                None => {
                    // Generator completed - task is done
                    guard.free();
                    task.finish();
                    self.inner.stats.completed_count =
                        self.inner.stats.completed_count.saturating_add(1);
                    run_ctx.mark_task_completed();
                }
            }
        } else {
            // Generator lost
            task.finish();
            self.inner.stats.completed_count = self.inner.stats.completed_count.saturating_add(1);
            run_ctx.mark_task_completed();

            #[cfg(debug_assertions)]
            tracing::warn!(
                "[Worker {}] Poll mode task had no generator",
                self.inner.worker_id
            );
        }
    }

    /// Create a Poll mode generator for a task
    /// This generator wraps task.poll_future() in a loop
    fn create_poll_mode_generator(
        &mut self,
        run_ctx: &mut RunContext,
        handle: TaskHandle,
        task: &Task,
    ) {
        const STACK_SIZE: usize = 512 * 1024; // 512KB stack

        // Capture task pointer as usize for Send safety
        let task_addr = task as *const Task as usize;
        let handle_copy = handle;

        let generator =
            crate::generator::Gn::<()>::new_scoped_opt(STACK_SIZE, move |mut scope| -> usize {
                let task = unsafe { &*(task_addr as *const Task) };

                loop {
                    // Create waker and context
                    let waker = unsafe { task.waker_yield() };
                    let mut cx = Context::from_waker(&waker);

                    // Poll the task's future
                    let poll_result = unsafe { task.poll_future(&mut cx) };

                    match poll_result {
                        Some(Poll::Ready(())) => {
                            // Task complete - return from generator
                            return 1; // Status code: task complete
                        }
                        Some(Poll::Pending) => {
                            // Task still pending - yield and continue loop
                            scope.yield_(
                                crate::runtime::preemption::GeneratorYieldReason::Cooperative
                                    .as_usize(),
                            );
                        }
                        None => {
                            // Future is gone, task complete
                            return 2; // Status code: future dropped
                        }
                    }
                }
            });

        // Store generator in task with Poll mode
        unsafe {
            task.pin_generator(
                generator.into_raw(),
                GeneratorOwnership::Owned,
                GeneratorRunMode::Poll,
            );
        }

        // Re-enqueue task for next poll with the new generator
        self.enqueue_yield(handle_copy);
    }

    fn poll_task(&mut self, run_ctx: &mut RunContext, handle: TaskHandle, task: &Task) {
        let waker = unsafe { task.waker_yield() };
        let mut cx = Context::from_waker(&waker);
        let poll_result = unsafe { task.poll_future(&mut cx) };

        match poll_result {
            Some(Poll::Ready(())) => {
                task.finish();
                self.inner.stats.completed_count =
                    self.inner.stats.completed_count.saturating_add(1);

                // Check if generator was pinned during poll (e.g., via pin_stack() call)
                if task.has_pinned_generator() {
                    // Determine pin reason based on generator run mode
                    let mode = task.generator_run_mode();
                    match mode {
                        GeneratorRunMode::Switch => run_ctx.mark_preempted(),
                        GeneratorRunMode::Poll => run_ctx.mark_explicit_pin(),
                        GeneratorRunMode::None => {} // Shouldn't happen
                    }
                    run_ctx.mark_task_completed();
                }

                self.inner.current_task = core::ptr::null_mut();
                if let Some(ptr) = task.take_future() {
                    unsafe { drop_future(ptr) };
                }
            }
            Some(Poll::Pending) => {
                // Check if generator was pinned during poll
                if task.has_pinned_generator() {
                    let mode = task.generator_run_mode();
                    match mode {
                        GeneratorRunMode::Switch => run_ctx.mark_preempted(),
                        GeneratorRunMode::Poll => run_ctx.mark_explicit_pin(),
                        GeneratorRunMode::None => {}
                    }
                    // Task is still pending, not completed
                }

                if task.is_yielded() {
                    task.record_yield();
                    self.inner.stats.yielded_count += 1;
                    // Yielded Tasks stay in EXECUTING state with yielded set to true
                    // without resetting the signal. All attempts to set the signal
                    // will not set it since it is guaranteed to run via the yield queue.
                    self.enqueue_yield(handle);
                } else {
                    self.inner.stats.waiting_count += 1;
                    task.finish();
                }
            }
            None => {
                // Future is gone - task completed or was cancelled
                if task.has_pinned_generator() {
                    let mode = task.generator_run_mode();
                    match mode {
                        GeneratorRunMode::Switch => run_ctx.mark_preempted(),
                        GeneratorRunMode::Poll => run_ctx.mark_explicit_pin(),
                        GeneratorRunMode::None => {}
                    }
                    run_ctx.mark_task_completed();
                }
                self.inner.stats.completed_count += 1;
                task.finish();
            }
        }
    }

    fn schedule_timer(&mut self, timer: &Timer, delay: Duration) -> Option<u64> {
        let task_ptr = self.inner.current_task;
        if task_ptr.is_null() {
            return None;
        }

        let task = unsafe { &*task_ptr };
        let Some(slot) = task.slot() else {
            return None;
        };

        if timer.is_scheduled() {
            if let Some(worker_id) = timer.worker_id() {
                if worker_id == self.inner.worker_id {
                    let existing_id = timer.timer_id();
                    if self
                        .inner
                        .timer_wheel
                        .cancel_timer(existing_id)
                        .is_ok()
                    {
                        timer.reset();
                    }
                } else {
                    // Cancel timer on other worker.
                    self.inner.bus.post_cancel_message(
                        self.inner.worker_id,
                        worker_id,
                        timer.timer_id(),
                    );
                }
            }
        }

        let delay_ns = delay.as_nanos().min(u128::from(u64::MAX)) as u64;
        let now = self.inner.timer_wheel.now_ns();
        let target_deadline_ns = now.saturating_add(delay_ns);

        let handle = timer.prepare(slot, task.global_id(), self.inner.worker_id);

        if let Some((timer_id, wheel_deadline_ns)) =
            self.enqueue_timer_entry_best_fit(target_deadline_ns, handle)
        {
            timer.commit_schedule(timer_id, target_deadline_ns, wheel_deadline_ns);
            Some(target_deadline_ns)
        } else {
            timer.reset();
            None
        }
    }
}

/// Schedules a timer for the task currently being polled by the active worker.
///
/// Uses best-fit scheduling to support arbitrarily long timers via cascading.
/// The timer wheel will schedule at the largest wheel that fits, and TimerDelay
/// will reschedule when the wheel fires if the target hasn't been reached yet.
pub(crate) fn schedule_timer_for_task(
    _cx: &Context<'_>,
    timer: &Timer,
    delay: Duration,
) -> Option<u64> {
    let worker = current_worker();
    if worker.is_none() {
        return None;
    }
    let worker = worker.unwrap();
    let worker_id = worker.worker_id;
    let task = worker.current_task;
    if task.is_null() {
        return None;
    }
    let task = unsafe { &mut *task };
    let slot = task.slot()?;

    // Cancel existing timer if scheduled
    if timer.is_scheduled() {
        if let Some(existing_worker_id) = timer.worker_id() {
            if existing_worker_id == worker_id {
                let existing_id = timer.timer_id();
                if worker.timer_wheel.cancel_timer(existing_id).is_ok() {
                    timer.reset();
                }
            } else {
                // Cross-worker cancellation: send message to the worker that owns the timer
                let existing_id = timer.timer_id();
                if worker
                    .bus
                    .post_cancel_message(worker_id, existing_worker_id, existing_id)
                {
                    timer.reset();
                }
            }
        }
    }

    let delay_ns = delay.as_nanos().min(u128::from(u64::MAX)) as u64;
    let now = worker.timer_wheel.now_ns();
    let target_deadline_ns = now.saturating_add(delay_ns);

    let handle = timer.prepare(slot, task.global_id(), worker_id);

    // Use best-fit scheduling for cascading support
    match worker
        .timer_wheel
        .schedule_timer_best_fit(target_deadline_ns, handle)
    {
        Ok((timer_id, wheel_deadline_ns)) => {
            timer.commit_schedule(timer_id, target_deadline_ns, wheel_deadline_ns);
            Some(target_deadline_ns)
        }
        Err(_) => {
            timer.reset();
            None
        }
    }
}

/// Reschedules a timer for the remaining time to reach its target deadline.
///
/// Called by TimerDelay when the wheel timer fires but the target deadline
/// hasn't been reached yet (cascading for long timers).
pub(crate) fn reschedule_timer_for_task(_cx: &Context<'_>, timer: &Timer) -> bool {
    let worker = current_worker();
    if worker.is_none() {
        return false;
    }
    let worker = worker.unwrap();
    let worker_id = worker.worker_id;
    let task = worker.current_task;
    if task.is_null() {
        return false;
    }
    let task = unsafe { &mut *task };
    let Some(slot) = task.slot() else {
        return false;
    };

    // Verify the timer is scheduled on this worker
    let Some(timer_worker_id) = timer.worker_id() else {
        return false;
    };
    if timer_worker_id != worker_id {
        return false;
    }

    let target_deadline_ns = timer.target_deadline_ns();
    let handle = TimerHandle::new(slot, task.global_id(), worker_id, 0);

    // Schedule for the remaining time using best-fit
    match worker
        .timer_wheel
        .schedule_timer_best_fit(target_deadline_ns, handle)
    {
        Ok((timer_id, wheel_deadline_ns)) => {
            timer.commit_reschedule(timer_id, wheel_deadline_ns);
            true
        }
        Err(_) => false,
    }
}

/// Cancels a timer owned by the task currently being polled by the active worker.
pub(crate) fn cancel_timer_for_current_task(timer: &Timer) -> bool {
    if !timer.is_scheduled() {
        return false;
    }

    let Some(timer_worker_id) = timer.worker_id() else {
        return false;
    };

    let timer_id = timer.timer_id();
    if timer_id == 0 {
        return false;
    }

    let worker = current_worker();
    if worker.is_none() {
        return false;
    }
    let worker = worker.unwrap();
    let worker_id = worker.worker_id;
    let task = worker.current_task;
    if task.is_null() {
        return false;
    }
    let task = unsafe { &mut *task };

    if timer_worker_id == worker_id {
        if worker
            .timer_wheel
            .cancel_timer(timer_id)
            .is_ok()
        {
            timer.mark_cancelled(timer_id);
            return true;
        }
        return false;
    }

    // Cross-worker cancellation: send message to the worker that owns the timer
    if worker
        .bus
        .post_cancel_message(worker_id, timer_worker_id, timer_id)
    {
        timer.mark_cancelled(timer_id);
        true
    } else {
        false
    }
}
