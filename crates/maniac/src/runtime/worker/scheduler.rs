use super::waker::WorkerWaker;
use super::worker::{WaitStrategy, WakeStats, Worker};
use crate::PopError;
use crate::future::waker::DiatomicWaker;
use crate::runtime::deque::{Stealer, Worker as YieldWorker};
use crate::runtime::task::SpawnError;
use crate::runtime::task::summary::Summary;
use crate::runtime::task::{TaskArena, TaskArenaStats, TaskHandle};
use crate::runtime::timer::TimerHandle;
use crate::runtime::timer::ticker::TickService;
use crate::runtime::timer::ticker::{TickHandler, TickHandlerRegistration};
use crate::runtime::timer::wheel::TimerWheel;
use crate::runtime::worker::io::IoDriver;
use crate::runtime::{mpsc, preemption};
use crate::{PushError, utils};
use parking_lot::Mutex;
use std::any::TypeId;
use std::cell::UnsafeCell;
use std::future::{Future, IntoFuture};
use std::panic::Location;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crate::utils::bits;

const MAX_WORKER_LIMIT: usize = 512;
const DEFAULT_WAKE_BURST: usize = 4;
const TIMER_TICKS_PER_WHEEL: usize = 1024 * 1;
const MESSAGE_BATCH_SIZE: usize = 4096;
const DEFAULT_TICK_DURATION_NS: u64 = 1 << 19; // ~1.05ms, power of two as required by TimerWheel

/// Trait for cross-worker operations that don't depend on const parameters
pub(crate) trait WorkerBus: Send + Sync {
    fn post_cancel_message(&self, from_worker_id: u32, to_worker_id: u32, timer_id: u64) -> bool;
}

/// Type-erased header for futures stored in tasks.
/// Contains function pointers for polling and dropping without knowing the concrete type.
#[repr(C)]
pub(crate) struct FutureHeader {
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
    _guard: Arc<Scheduler>,
}

impl<T> JoinHandle<T> {
    /// Creates a new join handle.
    ///
    /// # Arguments
    /// * `join_state_ptr` - Pointer to the JoinState<T> at the start of JoinableFuture
    /// * `guard` - Arc that keeps the executor/arena alive while this handle exists
    #[inline]
    pub fn new(join_state_ptr: *const JoinState<T>, guard: Arc<Scheduler>) -> Self {
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
pub struct SchedulerConfig {
    pub tick_duration: Duration,
    pub worker_count: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            tick_duration: Duration::from_nanos(DEFAULT_TICK_DURATION_NS),
            worker_count: utils::num_cpus(),
        }
    }
}

pub struct Scheduler {
    // Core ownership - Scheduler owns the arena and coordinates work via SummaryTree
    pub(crate) arena: TaskArena,
    pub(crate) summary_tree: Summary,

    config: SchedulerConfig,
    tick_duration: Duration,
    tick_duration_ns: u64,
    clock_ns: Arc<AtomicU64>,

    // Per-worker WorkerWakers - each tracks all three work types:
    // - summary: mpsc queue signals (bits 0-63 for different signal words)
    // - status bit 63: yield queue has items
    // - status bit 62: task partition has work
    pub(crate) wakers: Box<[Arc<WorkerWaker>]>,

    worker_actives: Box<[AtomicU64]>,
    worker_now_ns: Box<[AtomicU64]>,
    worker_shutdowns: Box<[AtomicBool]>,
    worker_threads: Box<[Mutex<Option<thread::JoinHandle<()>>>]>,
    worker_thread_handles: Box<[Mutex<Option<crate::runtime::preemption::WorkerThreadHandle>>]>,
    pub(crate) worker_stats: Box<[Mutex<WorkerStats>]>,
    pub(crate) worker_count: usize,
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

unsafe impl Send for Scheduler {}

// SAFETY: Scheduler contains UnsafeCells for receivers and timers, but each worker
// has exclusive access to its own index. The Arc-wrapped Scheduler is shared between
// threads, but the UnsafeCell data is partitioned by worker_id.
unsafe impl Sync for Scheduler {}

impl Scheduler {
    pub fn start(
        arena: TaskArena,
        config: SchedulerConfig,
        tick_service: &Arc<TickService>,
    ) -> Arc<Self> {
        let tick_duration = config.tick_duration;
        let tick_duration_ns = normalize_tick_duration_ns(config.tick_duration);
        let worker_count_val = config.worker_count.max(1).min(MAX_WORKER_LIMIT);

        // Create per-worker WorkerWakers
        let mut wakers = Vec::with_capacity(worker_count_val);
        for i in 0..worker_count_val {
            wakers.push(Arc::new(WorkerWaker::new()));
        }
        let wakers = wakers.into_boxed_slice();

        // Create SummaryTree with wakers and fixed worker_count
        let summary_tree = Summary::new(
            arena.config().leaf_count,
            arena.layout().signals_per_leaf,
            &wakers,
            worker_count_val,
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
            worker_stats.push(Mutex::new(WorkerStats::default()));
        }

        let mut receivers = Vec::with_capacity(worker_count_val);
        let mut senders =
            Vec::<mpsc::Sender<WorkerMessage>>::with_capacity(worker_count_val * worker_count_val);
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
            worker_count: worker_count_val,
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
            let service_ptr = Arc::as_ptr(&service) as *mut Scheduler;
            *(*service_ptr).tick_registration.lock() = Some(registration);
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

    /// Returns aggregate statistics across all workers and the arena.
    ///
    /// This includes waker stats (permits, sleepers, partition summaries)
    /// and worker stats (tasks_polled, completed, yielded, etc.) for each worker,
    /// plus arena-level stats.
    pub fn aggregate_stats(&self) -> SchedulerStats {
        let mut per_worker = Vec::with_capacity(self.worker_count);
        for i in 0..self.worker_count {
            let waker = &self.wakers[i];
            let stats = self.worker_stats[i].lock().clone();
            per_worker.push(WorkerSnapshot {
                worker_id: i,
                permits: waker.permits(),
                sleepers: waker.sleepers(),
                status: waker.status(),
                partition_summary: waker.partition_summary(),
                stats,
            });
        }
        SchedulerStats {
            worker_count: self.worker_count,
            arena_stats: self.arena.stats(),
            per_worker,
        }
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
        let _lock = self.register_mutex.lock();
        let now_ns = Instant::now().elapsed().as_nanos() as u64;

        // Mark worker as active
        self.worker_actives[worker_id].store(1, Ordering::SeqCst);
        self.worker_now_ns[worker_id].store(now_ns, Ordering::SeqCst);

        let service_clone = Arc::clone(service);
        let timer_resolution_ns = self.tick_duration().as_nanos().max(1) as u64;
        let partition_len = partition_end.saturating_sub(partition_start).min(1);

        let join = std::thread::spawn(move || {
            let core_ids = crate::utils::cpu_cores();
            let core_id = worker_id + 1;
            core_affinity::set_for_current(core_ids[core_id % core_ids.len()]);

            let task_slot = crate::runtime::task::TaskSlot::new(std::ptr::null_mut());
            let task_slot_ptr = &task_slot as *const _ as *mut crate::runtime::task::TaskSlot;
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

            let waker = service_clone.wakers[worker_id].as_ref();

            let io_driver = unsafe { &mut *(waker.io_driver as *mut IoDriver) };

            let mut w = Worker {
                scheduler: Arc::clone(&service_clone),
                receiver: unsafe {
                    // SAFETY: Worker thread has exclusive access to its own receiver.
                    // The service Arc is kept alive for the lifetime of the worker.
                    &mut *service_clone.receivers[worker_id].get()
                },
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
                cached_worker_count: service_clone.worker_count,
                wake_burst_limit: DEFAULT_WAKE_BURST,
                worker_id: worker_id as u32,
                timer_resolution_ns,
                timer_output,
                message_batch: message_batch.into_boxed_slice(),
                rng: crate::utils::Random::new(),
                current_task: std::ptr::null_mut(),
                current_scope: std::ptr::null_mut(),
                preemption_requested: AtomicBool::new(false),
                waker,
                io: io_driver,
                io_budget: 16, // Process up to 16 I/O events per run_once iteration
                panic_reason: None,
                counter: 0,
            };

            // Wrap execution in monoio context and set user_data
            let w_ptr = std::ptr::addr_of_mut!(w);

            super::CURRENT.with(|c| c.set(w_ptr as *mut Worker<'static>));

            // Wrap execution in monoio context and set user_data
            unsafe { &(*w_ptr).io }.with_context(|| {
                crate::runtime::worker::io::CURRENT.with(|ctx| {
                    ctx.user_data
                        .set(unsafe { &(*w_ptr) as *const _ as *mut () });
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
                        &(*w_ptr).preemption_requested
                    });

                #[cfg(not(any(unix, windows)))]
                let thread_handle_result =
                    crate::runtime::preemption::WorkerThreadHandle::current(unsafe {
                        &(*w_ptr).preemption_requested
                    });

                if let Ok(thread_handle) = thread_handle_result {
                    *service_clone.worker_thread_handles[worker_id].lock() = Some(thread_handle);
                }

                // Catch panics to prevent thread termination from propagating
                // SAFETY: Worker cleanup code (service counter updates) will still execute after a panic
                // We use a raw pointer wrapped in AssertUnwindSafe because Worker contains mutable
                // references that don't implement UnwindSafe, but the cleanup code will still execute.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    unsafe { (*w_ptr).run() };
                }));
                if let Err(e) = result {
                    tracing::trace!("[Worker {}] Panicked: {:?}", worker_id, e);
                }
            });

            // Worker exited - clean up counters
            service_clone.worker_actives[worker_id].store(0, Ordering::SeqCst);
        });

        // Store the join handle
        *self.worker_threads[worker_id].lock() = Some(join);

        Ok(())
    }

    /// Spawns a new async task on the executor.
    ///
    /// This method performs the following steps:
    /// 1. Validates that the executor is not shutting down and the arena is open
    /// 2. Reserves a task slot from the scheduler
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

        // Reserve a task slot from the scheduler
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
        // 3. Releases the task handle back to the scheduler
        let user_future = future.into_future();
        let wrapper_future = async move {
            let result = user_future.await;
            // The JoinableFuture pointer is stored in the task's future_ptr.
            // JoinState is at offset sizeof(FutureHeader) from the start.
            let task = release_handle.task();
            let base_ptr = task.future_ptr() as *const u8;
            let join_state_ptr =
                unsafe { base_ptr.add(std::mem::size_of::<FutureHeader>()) as *const JoinState<T> };
            // Safety: We know this points to a JoinState<T> after the header
            unsafe { (*join_state_ptr).complete(result) };

            // Clear the future pointer immediately after completion
            // This prevents any further polling attempts
            task.take_future();

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
        *self.tick_registration.lock() = None;
        if self.shutdown.swap(true, Ordering::Release) {
            return;
        }

        tracing::trace!("[scheduler] Shutdown initiated, sending shutdown messages...");

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

        tracing::trace!("[scheduler] Joining worker threads...");

        // Join all worker threads
        for (idx, worker_thread) in self.worker_threads.iter().enumerate() {
            if let Some(handle) = worker_thread.lock().take() {
                tracing::trace!("[scheduler] Joining worker thread {}...", idx);
                let _ = handle.join();
                tracing::trace!("[scheduler] Worker thread {} joined", idx);
            }
        }

        // Clear thread handles after joining threads to ensure proper cleanup order
        // This is especially important on Windows where handles need to be closed
        // after the threads have finished executing
        for thread_handle_mutex in self.worker_thread_handles.iter() {
            let _ = thread_handle_mutex.lock().take();
        }

        tracing::trace!("[scheduler] Shutdown complete");
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

    /// supervisor: Health monitoring - check for stuck workers and collect metrics
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
                        "[supervisor] WARNING: Worker {} appears stuck (no update for {}ns)",
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

    /// supervisor: Graceful shutdown coordination
    fn supervisor_graceful_shutdown(&self) {
        let _ = self.tick_registration.lock().take();
        let worker_count = self.config.worker_count;

        tracing::trace!("[supervisor] Shutting down, worker_count={}", worker_count);

        // Send graceful shutdown to all active workers
        for worker_id in 0..worker_count {
            let is_active = self.worker_actives[worker_id].load(Ordering::Relaxed) != 0;
            tracing::trace!("[supervisor] Worker {} active={}", worker_id, is_active);

            if self.worker_actives[worker_id].load(Ordering::Relaxed) != 0 {
                tracing::trace!("[supervisor] Sending shutdown to worker {}", worker_id);
                // SAFETY: unsafe_try_push only requires a shared reference
                let _ = unsafe {
                    self.tick_senders[worker_id].unsafe_try_push(WorkerMessage::GracefulShutdown)
                };
                // Mark work available to wake up the worker
                self.wakers[worker_id].mark_tasks();
            }
        }

        tracing::trace!("[supervisor] Sent graceful shutdown to all workers");

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
                tracing::trace!(
                    "[supervisor] Graceful shutdown timeout, {} workers still active",
                    active_count
                );
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

        let handle_guard = self.worker_thread_handles[worker_id].lock();

        if let Some(ref handle) = *handle_guard {
            handle.interrupt()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl WorkerBus for Scheduler {
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

impl TickHandler for Scheduler {
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

impl Drop for Scheduler {
    fn drop(&mut self) {
        *self.tick_registration.lock() = None;
        if !self.shutdown.load(Ordering::Acquire) {
            self.shutdown();
        }
    }
}

fn normalize_tick_duration_ns(duration: Duration) -> u64 {
    let nanos = duration.as_nanos().max(1).min(u128::from(u64::MAX)) as u64;
    nanos.next_power_of_two()
}

/// Snapshot of a single worker's state including waker and execution stats.
#[derive(Debug, Clone)]
pub struct WorkerSnapshot {
    pub worker_id: usize,
    pub permits: u64,
    pub sleepers: usize,
    pub status: u64,
    pub partition_summary: u64,
    pub stats: WorkerStats,
}

/// Aggregate statistics for the entire Scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub worker_count: usize,
    pub arena_stats: TaskArenaStats,
    pub per_worker: Vec<WorkerSnapshot>,
}

impl SchedulerStats {
    /// Returns aggregate totals across all workers.
    pub fn totals(&self) -> WorkerStats {
        let mut totals = WorkerStats::default();
        for w in &self.per_worker {
            totals.tasks_polled += w.stats.tasks_polled;
            totals.completed_count += w.stats.completed_count;
            totals.yielded_count += w.stats.yielded_count;
            totals.waiting_count += w.stats.waiting_count;
            totals.cas_failures += w.stats.cas_failures;
            totals.empty_scans += w.stats.empty_scans;
            totals.yield_queue_polls += w.stats.yield_queue_polls;
            totals.signal_polls += w.stats.signal_polls;
            totals.steal_attempts += w.stats.steal_attempts;
            totals.steal_successes += w.stats.steal_successes;
            totals.leaf_summary_checks += w.stats.leaf_summary_checks;
            totals.leaf_summary_hits += w.stats.leaf_summary_hits;
            totals.leaf_steal_attempts += w.stats.leaf_steal_attempts;
            totals.leaf_steal_successes += w.stats.leaf_steal_successes;
            totals.timer_fires += w.stats.timer_fires;
            totals.preempts += w.stats.preempts;
        }
        totals
    }
}

impl std::fmt::Display for SchedulerStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "scheduler Stats:")?;
        writeln!(f, "  Workers: {}", self.worker_count)?;
        writeln!(f, "  Arena:")?;
        writeln!(f, "    Total capacity: {}", self.arena_stats.total_capacity)?;
        writeln!(f, "    Active tasks: {}", self.arena_stats.active_tasks)?;

        // Aggregate totals
        let totals = self.totals();
        writeln!(f, "  Totals:")?;
        writeln!(f, "    Tasks polled: {}", totals.tasks_polled)?;
        writeln!(f, "    Completed: {}", totals.completed_count)?;
        writeln!(f, "    Yielded: {}", totals.yielded_count)?;
        writeln!(f, "    Waiting: {}", totals.waiting_count)?;
        writeln!(f, "    CAS failures: {}", totals.cas_failures)?;
        writeln!(f, "    Empty scans: {}", totals.empty_scans)?;
        writeln!(f, "    Yield queue polls: {}", totals.yield_queue_polls)?;
        writeln!(f, "    Signal polls: {}", totals.signal_polls)?;
        writeln!(
            f,
            "    Steal attempts: {} (successes: {})",
            totals.steal_attempts, totals.steal_successes
        )?;
        writeln!(
            f,
            "    Leaf summary checks: {} (hits: {})",
            totals.leaf_summary_checks, totals.leaf_summary_hits
        )?;
        writeln!(
            f,
            "    Leaf steal attempts: {} (successes: {})",
            totals.leaf_steal_attempts, totals.leaf_steal_successes
        )?;
        writeln!(f, "    Timer fires: {}", totals.timer_fires)?;
        writeln!(f, "    Preempts: {}", totals.preempts)?;

        writeln!(f, "  Per-worker:")?;
        for w in &self.per_worker {
            writeln!(
                f,
                "    [{}] polled={} completed={} yielded={} steals={}/{}",
                w.worker_id,
                w.stats.tasks_polled,
                w.stats.completed_count,
                w.stats.yielded_count,
                w.stats.steal_successes,
                w.stats.steal_attempts
            )?;
        }
        Ok(())
    }
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

    pub preempts: u64,

    pub generators_created: u64,
    pub generators_dropped: u64,
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
