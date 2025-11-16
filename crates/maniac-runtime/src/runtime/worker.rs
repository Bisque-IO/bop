use super::deque::{Stealer, Worker as YieldWorker};
use super::summary::Summary;
use super::task::{FutureAllocator, Task, TaskArena, TaskHandle, TaskSlot};
use super::timer::{Timer, TimerHandle};
use super::timer_wheel::TimerWheel;
use super::waker::WorkerWaker;
use crate::PopError;
use crate::runtime::mpsc;
use crate::runtime::ticker::{TickHandler, TickHandlerRegistration};
use crate::{PushError, utils};
use std::cell::{Cell, UnsafeCell};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crate::utils::bits;
use rand::RngCore;

const DEFAULT_QUEUE_SEG_SHIFT: usize = 8;
const DEFAULT_QUEUE_NUM_SEGS_SHIFT: usize = 8;
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
trait CrossWorkerOps: Send + Sync {
    fn post_cancel_message(&self, from_worker_id: u32, to_worker_id: u32, timer_id: u64) -> bool;
}

thread_local! {
    static CURRENT_TIMER_WHEEL: Cell<*mut TimerWheel<TimerHandle>> = Cell::new(ptr::null_mut());
    static CURRENT_TASK: Cell<*mut Task> = Cell::new(ptr::null_mut());
    static CROSS_WORKER_OPS: Cell<Option<&'static dyn CrossWorkerOps>> = const { Cell::new(None) };
}

pub struct WorkerTLS {}

// pub fn current_task<'a>() -> Option<&'a mut Task> {
//     let worker = CURRENT_WORKER.with(|cell| cell.get());
//     if worker.is_null() {
//         return None;
//     }
//     let worker = unsafe { &*worker };
//     let task = worker.current_task;
//     if task.is_null() {
//         return None;
//     }
//     unsafe { Some(&mut *task) }
// }

#[inline]
pub fn is_worker_thread() -> bool {
    current_task().is_some()
}

#[inline]
pub fn current_task<'a>() -> Option<&'a mut Task> {
    let task = CURRENT_TASK.with(|cell| cell.get());
    if task.is_null() {
        return None;
    } else {
        unsafe { Some(&mut *task) }
    }
}

pub fn current_timer_wheel<'a>() -> Option<&'a mut TimerWheel<TimerHandle>> {
    let timer_wheel = CURRENT_TIMER_WHEEL.with(|cell| cell.get());
    if timer_wheel.is_null() {
        None
    } else {
        unsafe { Some(&mut *timer_wheel) }
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
#[derive(Clone, Debug)]
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
    WorkerCountChanged {
        new_worker_count: u16,
    },

    /// Rebalance task partitions (reassign which leaves this worker processes)
    RebalancePartitions {
        partition_start: usize,
        partition_end: usize,
    },

    /// Migrate specific tasks to another worker
    MigrateTasks {
        task_handles: Vec<TaskHandle>,
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
    pub min_workers: usize,
    pub max_workers: usize,
}

impl Default for WorkerServiceConfig {
    fn default() -> Self {
        Self {
            tick_duration: Duration::from_nanos(DEFAULT_TICK_DURATION_NS),
            min_workers: utils::num_cpus(),
            max_workers: utils::num_cpus(),
        }
    }
}

pub struct WorkerService<
    const P: usize = DEFAULT_QUEUE_SEG_SHIFT,
    const NUM_SEGS_P2: usize = DEFAULT_QUEUE_NUM_SEGS_SHIFT,
> {
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
    worker_stats: Box<[WorkerStats]>,
    worker_count: Arc<AtomicUsize>,
    worker_max_id: AtomicUsize,
    receivers: Box<[UnsafeCell<mpsc::Receiver<WorkerMessage, P, NUM_SEGS_P2>>]>,
    senders: Box<[mpsc::Sender<WorkerMessage, P, NUM_SEGS_P2>]>,
    tick_senders: Box<[mpsc::Sender<WorkerMessage, P, NUM_SEGS_P2>]>,
    yield_queues: Box<[YieldWorker<TaskHandle>]>,
    yield_stealers: Box<[Stealer<TaskHandle>]>,
    timers: Box<[UnsafeCell<TimerWheel<TimerHandle>>]>,
    shutdown: AtomicBool,
    register_mutex: Mutex<()>,
    // RAII guard for tick service registration - automatically unregisters on drop
    tick_registration: Mutex<Option<TickHandlerRegistration>>,
    // Flag to request immediate partition rebalancing on next tick (when workers spawn/exit)
    rebalance_requested: AtomicBool,
    // Tick-related counters for on_tick logic
    tick_health_check_interval: u64,
    tick_partition_rebalance_interval: u64,
    tick_scaling_check_interval: u64,
}

unsafe impl<const P: usize, const NUM_SEGS_P2: usize> Send for WorkerService<P, NUM_SEGS_P2> {}

// SAFETY: WorkerService contains UnsafeCells for receivers and timers, but each worker
// has exclusive access to its own index. The Arc-wrapped WorkerService is shared between
// threads, but the UnsafeCell data is partitioned by worker_id.
unsafe impl<const P: usize, const NUM_SEGS_P2: usize> Sync for WorkerService<P, NUM_SEGS_P2> {}

impl<const P: usize, const NUM_SEGS_P2: usize> WorkerService<P, NUM_SEGS_P2> {
    pub fn start(
        arena: TaskArena,
        config: WorkerServiceConfig,
        tick_service: &Arc<super::ticker::TickService>,
    ) -> Arc<Self> {
        let tick_duration = config.tick_duration;
        let tick_duration_ns = normalize_tick_duration_ns(config.tick_duration);
        let min_workers = config.min_workers.max(1);
        let max_workers = config.max_workers.max(min_workers).min(MAX_WORKER_LIMIT);

        // Create per-worker WorkerWakers
        let mut wakers = Vec::with_capacity(max_workers);
        for _ in 0..max_workers {
            wakers.push(Arc::new(WorkerWaker::new()));
        }
        let wakers = wakers.into_boxed_slice();

        // Create worker_count early so we can pass it to SummaryTree (single source of truth)
        let worker_count = Arc::new(AtomicUsize::new(0));

        // Create SummaryTree with reference to wakers and worker_count
        let summary_tree = Summary::new(
            arena.config().leaf_count,
            arena.layout().signals_per_leaf,
            &wakers,
            &worker_count,
        );

        let mut worker_actives = Vec::with_capacity(max_workers);
        for _ in 0..max_workers {
            worker_actives.push(AtomicU64::new(0));
        }
        let mut worker_now_ns = Vec::with_capacity(max_workers);
        for _ in 0..max_workers {
            worker_now_ns.push(AtomicU64::new(0));
        }
        let mut worker_shutdowns = Vec::with_capacity(max_workers);
        for _ in 0..max_workers {
            worker_shutdowns.push(AtomicBool::new(false));
        }
        let mut worker_threads = Vec::with_capacity(max_workers);
        for _ in 0..max_workers {
            worker_threads.push(Mutex::new(None));
        }
        let mut worker_stats = Vec::with_capacity(max_workers);
        for _ in 0..max_workers {
            worker_stats.push(WorkerStats::default());
        }

        let mut receivers = Vec::with_capacity(max_workers);
        let mut senders = Vec::<mpsc::Sender<WorkerMessage, P, NUM_SEGS_P2>>::with_capacity(
            max_workers * max_workers,
        );
        for worker_id in 0..max_workers {
            // Use the worker's WorkerWaker for mpsc queue
            let rx = mpsc::new_with_waker(Arc::clone(&wakers[worker_id]));
            receivers.push(UnsafeCell::new(rx));
        }
        for worker_id in 0..max_workers {
            for other_worker_id in 0..max_workers {
                senders.push(
                    unsafe { &*receivers[worker_id].get() }
                        .create_sender_with_config(0)
                        .expect("ran out of producer slots"),
                )
            }
        }

        // Create tick_senders for on_tick communication
        let mut tick_senders = Vec::with_capacity(max_workers);
        for worker_id in 0..max_workers {
            tick_senders.push(
                unsafe { &*receivers[worker_id].get() }
                    .create_sender_with_config(0)
                    .expect("ran out of producer slots"),
            );
        }

        let mut yield_queues = Vec::with_capacity(max_workers);
        for _ in 0..max_workers {
            yield_queues.push(YieldWorker::new_fifo());
        }
        let mut yield_stealers = Vec::with_capacity(max_workers);
        for worker_id in 0..max_workers {
            yield_stealers.push(yield_queues[worker_id].stealer());
        }
        let mut timers = Vec::with_capacity(max_workers);
        for worker_id in 0..max_workers {
            timers.push(UnsafeCell::new(TimerWheel::new(
                tick_duration,
                TIMER_TICKS_PER_WHEEL,
                worker_id as u32,
            )));
        }

        let service = Arc::new(Self {
            arena,
            summary_tree,
            config,
            tick_duration: tick_duration,
            tick_duration_ns,
            clock_ns: Arc::new(AtomicU64::new(0)),
            wakers,
            worker_actives: worker_actives.into_boxed_slice(),
            worker_now_ns: worker_now_ns.into_boxed_slice(),
            worker_shutdowns: worker_shutdowns.into_boxed_slice(),
            worker_threads: worker_threads.into_boxed_slice(),
            worker_stats: worker_stats.into_boxed_slice(),
            worker_count, // Use the worker_count we created earlier and passed to SummaryTree
            worker_max_id: AtomicUsize::new(0),
            receivers: receivers.into_boxed_slice(),
            senders: senders.into_boxed_slice(),
            tick_senders: tick_senders.into_boxed_slice(),
            yield_queues: yield_queues.into_boxed_slice(),
            yield_stealers: yield_stealers.into_boxed_slice(),
            timers: timers.into_boxed_slice(),
            shutdown: AtomicBool::new(false),
            register_mutex: Mutex::new(()),
            tick_registration: Mutex::new(None),
            rebalance_requested: AtomicBool::new(false),
            tick_health_check_interval: 100,
            tick_partition_rebalance_interval: 1000,
            tick_scaling_check_interval: 500,
        });

        // Spawn min_workers on startup with pre-set count to avoid rebalancing
        // Each worker recalculates partitions when worker_count changes, so we
        // pre-set it to the final value before spawning any workers
        service.worker_count.store(min_workers, Ordering::SeqCst);
        // SummaryTree now references worker_count directly - no need to set it separately

        let mut spawned = 0;
        for _ in 0..min_workers {
            if service.spawn_worker_internal(&service, false).is_err() {
                break;
            }
            spawned += 1;
        }

        // Adjust worker_count if we failed to spawn all min_workers
        if spawned < min_workers {
            service.worker_count.store(spawned, Ordering::SeqCst);
            // SummaryTree now references worker_count directly - no need to set it separately
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
            let service_ptr = Arc::as_ptr(&service) as *mut WorkerService<P, NUM_SEGS_P2>;
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

    pub fn spawn_worker(&self) -> Result<(), PushError<()>>
    where
        Self: Sized,
    {
        // This method requires an Arc<Self> context to spawn threads safely
        // In the context of TickHandler, this will be called via Arc<WorkerService>
        // We need to use unsafe here to get the Arc from &self
        let service_arc = unsafe {
            // SAFETY: WorkerService is always stored in an Arc when spawn_worker is called
            // This is guaranteed by the API design where start() returns Arc<Self>
            // and TickHandler is only implemented for Arc-wrapped services
            Arc::from_raw(self as *const Self)
        };
        let result = self.spawn_worker_internal(&service_arc, true);
        // Prevent Arc from being dropped (we only borrowed it)
        std::mem::forget(service_arc);
        result
    }

    fn spawn_worker_internal(
        &self,
        service: &Arc<Self>,
        increment_count: bool,
    ) -> Result<(), PushError<()>> {
        let _lock = self.register_mutex.lock().expect("register_mutex poisoned");
        let now_ns = Instant::now().elapsed().as_nanos() as u64;

        // Find first empty slot.
        let mut worker_id: Option<usize> = None;
        for id in 0..self.worker_actives.len() {
            if self.worker_actives[id]
                .compare_exchange(0, now_ns, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                worker_id = Some(id);
                break;
            }
        }
        if worker_id.is_none() {
            return Err(PushError::Full(()));
        }
        let worker_id = worker_id.unwrap();
        // Update worker_max_id to be the highest worker ID + 1 (used as range upper bound)
        if worker_id >= self.worker_max_id.load(Ordering::SeqCst) {
            self.worker_max_id.store(worker_id + 1, Ordering::SeqCst);
        }

        if increment_count {
            self.worker_count.fetch_add(1, Ordering::SeqCst);
            // SummaryTree now references worker_count directly - no need to set it separately
            // Request immediate partition rebalancing on next tick
            self.rebalance_requested.store(true, Ordering::Release);
        }

        // Mark worker as active
        self.worker_actives[worker_id].store(1, Ordering::SeqCst);

        let service_clone = Arc::clone(service);
        let shutdown = Arc::new(AtomicBool::new(false));
        let timer_resolution_ns = self.tick_duration().as_nanos().max(1) as u64;
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

            let mut w = Worker {
                service: Arc::clone(&service_clone),
                wait_strategy: WaitStrategy::default(),
                shutdown: &service_clone.worker_shutdowns[worker_id],
                receiver: unsafe {
                    // SAFETY: Worker thread has exclusive access to its own receiver.
                    // The service Arc is kept alive for the lifetime of the worker.
                    &mut *service_clone.receivers[worker_id].get()
                },
                yield_queue: &service_clone.yield_queues[worker_id],
                timer_wheel: unsafe {
                    // SAFETY: Worker thread has exclusive access to its own timer wheel.
                    // The service Arc is kept alive for the lifetime of the worker.
                    &mut *service_clone.timers[worker_id].get()
                },
                wake_stats: WakeStats::default(),
                stats: WorkerStats::default(),
                partition_start: 0,
                partition_end: 0,
                partition_len: 0,
                cached_worker_count: 0,
                wake_burst_limit: DEFAULT_WAKE_BURST,
                worker_id: worker_id as u32,
                timer_resolution_ns,
                timer_output,
                message_batch: message_batch.into_boxed_slice(),
                rng: crate::utils::Random::new(),
                current_task: std::ptr::null_mut(),
            };
            CURRENT_TIMER_WHEEL.set(w.timer_wheel as *mut TimerWheel<TimerHandle>);
            // SAFETY: The service Arc is kept alive for the lifetime of the worker thread
            let ops_ref: &'static dyn CrossWorkerOps =
                unsafe { &*(&*service_clone as *const dyn CrossWorkerOps) };
            CROSS_WORKER_OPS.set(Some(ops_ref));

            // Catch panics to prevent thread termination from propagating
            // SAFETY: Worker cleanup code (service counter updates) will still execute after a panic
            // We use a raw pointer wrapped in AssertUnwindSafe because Worker contains mutable
            // references that don't implement UnwindSafe, but the cleanup code will still execute.
            let w_ptr = &mut w as *mut Worker<'_, P, NUM_SEGS_P2>;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                unsafe { (*w_ptr).run() };
            }));
            if let Err(e) = result {
                eprintln!("[Worker {}] Panicked: {:?}", worker_id, e);
            }
            // w.run();

            // Worker exited - clean up counters
            service_clone.worker_count.fetch_sub(1, Ordering::SeqCst);
            // SummaryTree now references worker_count directly - no need to set it separately
            service_clone.worker_actives[worker_id].store(0, Ordering::SeqCst);
            // Request immediate partition rebalancing on next tick
            service_clone
                .rebalance_requested
                .store(true, Ordering::Release);
        });

        // Store the join handle
        *self.worker_threads[worker_id]
            .lock()
            .expect("worker_threads lock poisoned") = Some(join);

        Ok(())
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
                [((from_worker_id as usize) * self.config.max_workers) + (to_worker_id as usize)]
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
        let max_workers = self.worker_max_id.load(Ordering::Relaxed);
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

        #[cfg(debug_assertions)]
        {
            eprintln!("[WorkerService] Shutdown initiated, sending shutdown messages...");
        }

        // Send shutdown messages to all active workers
        let max_workers = self.worker_max_id.load(Ordering::Relaxed);
        for worker_id in 0..=max_workers {
            if worker_id < self.worker_actives.len()
                && self.worker_actives[worker_id].load(Ordering::Relaxed) != 0
            {
                // SAFETY: unsafe_try_push only requires a shared reference
                let _ = unsafe {
                    self.tick_senders[worker_id].unsafe_try_push(WorkerMessage::Shutdown)
                };
                // Mark work available to wake up the worker
                self.wakers[worker_id].mark_tasks();
            }
        }

        #[cfg(debug_assertions)]
        {
            eprintln!("[WorkerService] Joining worker threads...");
        }

        // Join all worker threads
        for (idx, worker_thread) in self.worker_threads.iter().enumerate() {
            if let Some(handle) = worker_thread.lock().unwrap().take() {
                #[cfg(debug_assertions)]
                {
                    eprintln!("[WorkerService] Joining worker thread {}...", idx);
                }
                let _ = handle.join();
                #[cfg(debug_assertions)]
                {
                    eprintln!("[WorkerService] Worker thread {} joined", idx);
                }
            }
        }

        #[cfg(debug_assertions)]
        {
            eprintln!("[WorkerService] Shutdown complete");
        }
    }

    /// Returns the current worker count
    #[inline]
    pub fn worker_count(&self) -> usize {
        self.worker_count.load(Ordering::Relaxed)
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
        let max_workers = self.worker_max_id.load(Ordering::Relaxed);
        let tick_duration_ns = self.tick_duration_ns;
        let stuck_threshold_ns = tick_duration_ns * 10; // Worker is stuck if no progress for 10 ticks

        for worker_id in 0..max_workers {
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
                    eprintln!(
                        "[Supervisor] WARNING: Worker {} appears stuck (no update for {}ns)",
                        worker_id, time_since_update
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

    /// Supervisor: Rebalance task partitions across workers
    fn supervisor_rebalance_partitions(&self) {
        let active_workers = self.worker_count.load(Ordering::Relaxed);
        if active_workers == 0 {
            return;
        }

        let total_leaves = self.arena.leaf_count();
        let leaves_per_worker = (total_leaves + active_workers - 1) / active_workers;

        let max_workers = self.worker_max_id.load(Ordering::Relaxed);
        let mut active_worker_ids = Vec::with_capacity(max_workers);

        // Collect active worker IDs
        for worker_id in 0..max_workers {
            if self.worker_actives[worker_id].load(Ordering::Relaxed) != 0 {
                active_worker_ids.push(worker_id);
            }
        }

        // Assign partitions to active workers
        for (idx, &worker_id) in active_worker_ids.iter().enumerate() {
            let partition_start = idx * leaves_per_worker;
            let partition_end = ((idx + 1) * leaves_per_worker).min(total_leaves);

            // Assign contiguous range of leaves [partition_start, partition_end)
            // SAFETY: unsafe_try_push only requires a shared reference
            let _ = unsafe {
                self.tick_senders[worker_id].unsafe_try_push(WorkerMessage::RebalancePartitions {
                    partition_start,
                    partition_end,
                })
            };
        }

        #[cfg(debug_assertions)]
        {
            eprintln!(
                "[Supervisor] Rebalanced {} leaves across {} active workers",
                total_leaves, active_workers
            );
        }
    }

    /// Supervisor: Graceful shutdown coordination
    fn supervisor_graceful_shutdown(&self) {
        let _ = self.tick_registration.lock().unwrap().take();
        let max_workers = self.worker_max_id.load(Ordering::Relaxed);

        #[cfg(debug_assertions)]
        {
            eprintln!("[Supervisor] Shutting down, max_workers={}", max_workers);
        }

        // Send graceful shutdown to all active workers
        for worker_id in 0..=max_workers {
            #[cfg(debug_assertions)]
            {
                let is_active = worker_id < self.worker_actives.len()
                    && self.worker_actives[worker_id].load(Ordering::Relaxed) != 0;
                eprintln!("[Supervisor] Worker {} active={}", worker_id, is_active);
            }
            if worker_id < self.worker_actives.len()
                && self.worker_actives[worker_id].load(Ordering::Relaxed) != 0
            {
                #[cfg(debug_assertions)]
                {
                    eprintln!("[Supervisor] Sending shutdown to worker {}", worker_id);
                }
                // SAFETY: unsafe_try_push only requires a shared reference
                let _ = unsafe {
                    self.tick_senders[worker_id].unsafe_try_push(WorkerMessage::GracefulShutdown)
                };
                // Mark work available to wake up the worker
                self.wakers[worker_id].mark_tasks();
            }
        }

        #[cfg(debug_assertions)]
        {
            eprintln!("[Supervisor] Sent graceful shutdown to all workers");
        }

        // Wait for workers to finish (with timeout)
        let shutdown_timeout = std::time::Duration::from_secs(5);
        let shutdown_start = std::time::Instant::now();

        loop {
            let active_count = self.worker_count.load(Ordering::Relaxed);
            if active_count == 0 {
                break;
            }

            if shutdown_start.elapsed() > shutdown_timeout {
                #[cfg(debug_assertions)]
                {
                    eprintln!(
                        "[Supervisor] Graceful shutdown timeout, {} workers still active",
                        active_count
                    );
                }
                // Force shutdown remaining workers
                for worker_id in 0..max_workers {
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

    /// Supervisor: Dynamic worker scaling based on load
    fn supervisor_dynamic_scaling(&self) {
        let active_workers = self.worker_count.load(Ordering::Relaxed);
        let min_workers = self.config.min_workers;
        let max_workers_config = self.config.max_workers;

        // Calculate total work pressure by checking WorkerWakers
        let mut total_work_signals = 0u64;
        let max_worker_id = self.worker_max_id.load(Ordering::Relaxed);

        for worker_id in 0..max_worker_id {
            if self.worker_actives[worker_id].load(Ordering::Relaxed) == 0 {
                continue;
            }

            let waker = &self.wakers[worker_id];
            let summary = waker.snapshot_summary();
            let status = waker.status();

            // Count work signals: summary bits + status bits
            total_work_signals += summary.count_ones() as u64 + status.count_ones() as u64;
        }

        // Scale up: if average work per worker > threshold, add workers
        if active_workers > 0 {
            let avg_work_per_worker = total_work_signals / active_workers as u64;
            let scale_up_threshold = 4; // If avg work signals > 4 per worker, scale up

            if avg_work_per_worker > scale_up_threshold && active_workers < max_workers_config {
                // Try to spawn a new worker
                match self.spawn_worker() {
                    Ok(()) => {
                        let new_count = self.worker_count.load(Ordering::Relaxed);

                        #[cfg(debug_assertions)]
                        {
                            eprintln!(
                                "[Supervisor] Scaled UP: {} -> {} workers (avg_work={})",
                                active_workers, new_count, avg_work_per_worker
                            );
                        }

                        // Notify all workers of count change
                        for sender in self.tick_senders.iter() {
                            // SAFETY: unsafe_try_push only requires a shared reference
                            let _ = unsafe {
                                sender.unsafe_try_push(WorkerMessage::WorkerCountChanged {
                                    new_worker_count: new_count as u16,
                                })
                            };
                        }
                    }
                    Err(_) => {
                        #[cfg(debug_assertions)]
                        {
                            eprintln!("[Supervisor] Failed to scale up: no available worker slots");
                        }
                    }
                }
            }
        }

        // Scale down: if work pressure is very low and we have more than min_workers
        if active_workers > min_workers && total_work_signals == 0 {
            // Find a worker to shut down (prefer higher worker IDs)
            for worker_id in (0..max_worker_id).rev() {
                if self.worker_actives[worker_id].load(Ordering::Relaxed) != 0 {
                    // Send graceful shutdown to this worker
                    // SAFETY: unsafe_try_push only requires a shared reference
                    let _ = unsafe {
                        self.tick_senders[worker_id]
                            .unsafe_try_push(WorkerMessage::GracefulShutdown)
                    };

                    #[cfg(debug_assertions)]
                    {
                        eprintln!(
                            "[Supervisor] Scaled DOWN: removed worker {} (no work detected)",
                            worker_id
                        );
                    }
                    break; // Only remove one worker at a time
                }
            }
        }
    }

    /// Supervisor: Task migration for load balancing
    /// Migrates tasks from overloaded workers to underloaded workers
    #[allow(dead_code)]
    fn supervisor_task_migration(&self) {
        let max_worker_id = self.worker_max_id.load(Ordering::Relaxed);

        // Collect load information for each worker
        let mut worker_loads: Vec<(usize, u64)> = Vec::new();

        for worker_id in 0..max_worker_id {
            if self.worker_actives[worker_id].load(Ordering::Relaxed) == 0 {
                continue;
            }

            let waker = &self.wakers[worker_id];
            let summary = waker.snapshot_summary();
            let status = waker.status();
            let load = summary.count_ones() as u64 + status.count_ones() as u64;

            worker_loads.push((worker_id, load));
        }

        if worker_loads.len() < 2 {
            return; // Need at least 2 workers for migration
        }

        // Sort by load
        worker_loads.sort_by_key(|&(_, load)| load);

        // Find most loaded and least loaded workers
        let (most_loaded_id, most_loaded_load) = worker_loads[worker_loads.len() - 1];
        let (least_loaded_id, least_loaded_load) = worker_loads[0];

        // Migration threshold: only migrate if imbalance is significant
        let imbalance = most_loaded_load.saturating_sub(least_loaded_load);
        let migration_threshold = 3;

        if imbalance >= migration_threshold {
            // In a real implementation, we would:
            // 1. Steal tasks from the overloaded worker's yield queue
            // 2. Send MigrateTasks message to the underloaded worker
            //
            // For now, we just log the decision
            // TODO: Implement actual task stealing from yield queues

            #[cfg(debug_assertions)]
            {
                eprintln!(
                    "[Supervisor] Task migration opportunity: worker {} (load={}) -> worker {} (load={})",
                    most_loaded_id, most_loaded_load, least_loaded_id, least_loaded_load
                );
            }

            // We can't easily steal from another worker's yield queue from the supervisor
            // This would require exposing the yield_stealers or implementing a different mechanism
            // For now, leave this as a placeholder for future implementation
            let _ = (most_loaded_id, least_loaded_id); // Suppress unused warnings
        }
    }
}

impl<const P: usize, const NUM_SEGS_P2: usize> CrossWorkerOps for WorkerService<P, NUM_SEGS_P2> {
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

impl<const P: usize, const NUM_SEGS_P2: usize> TickHandler for WorkerService<P, NUM_SEGS_P2> {
    fn tick_duration(&self) -> Duration {
        self.tick_duration
    }

    fn on_tick(&self, tick_count: u64, now_ns: u64) {
        // Update clock for all workers
        self.clock_ns.store(now_ns, Ordering::Release);
        let max_workers = self.worker_max_id.load(Ordering::Relaxed);
        for i in 0..=max_workers {
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

        // Perform initial partition rebalance on first tick
        if tick_count == 0 {
            self.supervisor_rebalance_partitions();
        }

        // Immediate rebalancing if requested (worker spawned/exited)
        if self.rebalance_requested.swap(false, Ordering::AcqRel) {
            self.supervisor_rebalance_partitions();
        }

        // Periodic health monitoring
        if tick_count % self.tick_health_check_interval == 0 {
            self.supervisor_health_check(now_ns);
        }

        // Periodic dynamic worker scaling
        if tick_count % self.tick_scaling_check_interval == 0 {
            self.supervisor_dynamic_scaling();
        }

        // Periodic partition rebalancing (backup in case flag mechanism fails)
        if tick_count % self.tick_partition_rebalance_interval == 0 {
            self.supervisor_rebalance_partitions();
        }
    }

    fn on_shutdown(&self) {
        // Graceful shutdown: notify all workers
        self.supervisor_graceful_shutdown();
    }
}

impl<const P: usize, const NUM_SEGS_P2: usize> Drop for WorkerService<P, NUM_SEGS_P2> {
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

pub struct Worker<
    'a,
    const P: usize = DEFAULT_QUEUE_SEG_SHIFT,
    const NUM_SEGS_P2: usize = DEFAULT_QUEUE_NUM_SEGS_SHIFT,
> {
    service: Arc<WorkerService<P, NUM_SEGS_P2>>,
    wait_strategy: WaitStrategy,
    shutdown: &'a AtomicBool,
    receiver: &'a mut mpsc::Receiver<WorkerMessage, P, NUM_SEGS_P2>,
    yield_queue: &'a YieldWorker<TaskHandle>,
    timer_wheel: &'a mut TimerWheel<TimerHandle>,
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
    pub(crate) current_task: *mut Task,
}

impl<'a, const P: usize, const NUM_SEGS_P2: usize> Worker<'a, P, NUM_SEGS_P2> {
    #[inline]
    pub fn stats(&self) -> &WorkerStats {
        &self.stats
    }

    /// Checks if this worker has any work to do (tasks, yields, or messages).
    /// Returns true if the worker should continue running, false if it can park.
    #[inline]
    fn has_work(&self) -> bool {
        let waker = &self.service.wakers[self.worker_id as usize];

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
        self.wait_strategy = strategy;
    }

    #[inline(always)]
    pub fn run_once(&mut self) -> bool {
        let mut did_work = false;

        if self.poll_timers() {
            did_work = true;
        }

        // Process messages first (including shutdown signals)
        if self.process_messages() {
            did_work = true;
        }

        // Poll all yielded tasks first - they're ready to run
        let yielded = self.poll_yield(self.yield_queue.len());
        if yielded > 0 {
            did_work = true;
        }

        // Partition assignment is handled by WorkerService via RebalancePartitions message
        if self.try_partition_random() {
            did_work = true;
        }

        // if self.try_partition_random(leaf_count) {
        //     did_work = true;
        // } else if self.try_partition_linear(leaf_count) {
        //     did_work = true;
        // }

        let rand = self.next_u64();

        if !did_work && rand & FULL_SUMMARY_SCAN_CADENCE_MASK == 0 {
            let leaf_count = self.service.arena().leaf_count();
            if self.try_any_partition_random(leaf_count) {
                did_work = true;
            }
            // else if self.try_any_partition_linear(leaf_count) {
            // did_work = true;
            // }
        }

        if !did_work {
            if self.poll_yield(self.yield_queue.len() as usize) > 0 {
                did_work = true;
            }

            // if self.poll_yield_steal(1) > 0 {
            //     did_work = true;
            // }
        }

        did_work
    }

    #[inline(always)]
    pub fn run_once_exhaustive(&mut self) -> bool {
        let mut did_work = false;

        if self.poll_timers() {
            did_work = true;
        }

        // Process messages first (including shutdown signals)
        if self.process_messages() {
            did_work = true;
        }

        // Poll all yielded tasks first - they're ready to run
        let yielded = self.poll_yield(self.yield_queue.len());
        if yielded > 0 {
            did_work = true;
        }

        // Partition assignment is handled by WorkerService via RebalancePartitions message
        if self.try_partition_random() {
            did_work = true;
        }

        if self.try_partition_random() {
            did_work = true;
        } else if self.try_partition_linear() {
            did_work = true;
        }

        let rand = self.next_u64();

        if !did_work {
            let leaf_count = self.service.arena().leaf_count();
            if self.try_any_partition_random(leaf_count) {
                did_work = true;
            } else if self.try_any_partition_linear(leaf_count) {
                did_work = true;
            }
        }

        if !did_work {
            if self.poll_yield(self.yield_queue.len() as usize) > 0 {
                did_work = true;
            }
            if self.poll_yield_steal(1) > 0 {
                did_work = true;
            }
        }

        did_work
    }

    #[inline]
    fn run_last_before_park(&mut self) -> bool {
        false
    }

    pub(crate) fn run(&mut self) {
        let mut spin_count = 0;
        let waker_id = self.worker_id as usize;

        while !self.shutdown.load(Ordering::Relaxed) {
            let mut progress = self.run_once();

            if !progress {
                spin_count += 1;

                if spin_count >= self.wait_strategy.spin_before_sleep {
                    core::hint::spin_loop();
                    progress = self.run_once_exhaustive();

                    if progress {
                        spin_count = 0;
                        continue;
                    }
                } else if spin_count < self.wait_strategy.spin_before_sleep {
                    core::hint::spin_loop();
                    continue;
                }

                // Calculate park duration considering timer deadlines
                let park_duration = self.calculate_park_duration();

                // Sync partition summary from SummaryTree before parking
                self.service.wakers[waker_id].sync_partition_summary(
                    self.partition_start,
                    self.partition_end,
                    &self.service.summary().leaf_words,
                );

                // Park on WorkerWaker with timer-aware timeout
                match park_duration {
                    Some(duration) if duration.is_zero() => {
                        // Timer ready - don't park, continue immediately
                    }
                    Some(duration) => {
                        // Park with timeout for timer deadline
                        self.service.wakers[waker_id].acquire_timeout(duration);
                    }
                    None => {
                        // No timer deadline - park with periodic wakeup for shutdown check
                        self.service.wakers[waker_id].acquire_timeout(Duration::from_millis(250));
                    }
                }
                spin_count = 0;
            } else {
                spin_count = 0;
            }
        }
    }

    /// Calculate how long the worker can park, considering both the wait strategy
    /// and the next timer deadline.
    #[inline]
    fn calculate_park_duration(&mut self) -> Option<Duration> {
        // Check if we have pending timers
        if let Some(next_deadline_ns) = self.timer_wheel.next_deadline() {
            let now_ns = self.timer_wheel.now_ns();

            if next_deadline_ns > now_ns {
                let timer_duration_ns = next_deadline_ns - now_ns;
                let timer_duration = Duration::from_nanos(timer_duration_ns);

                // Use the minimum of wait_strategy timeout and timer deadline
                let duration = match self.wait_strategy.park_timeout {
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
        match self.wait_strategy.park_timeout {
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
            WorkerMessage::WorkerCountChanged { new_worker_count } => {
                self.cached_worker_count = new_worker_count as usize;
                true
            }
            WorkerMessage::RebalancePartitions {
                partition_start,
                partition_end,
            } => self.handle_rebalance_partitions(partition_start, partition_end),
            WorkerMessage::MigrateTasks { task_handles } => self.handle_migrate_tasks(task_handles),
            WorkerMessage::ReportHealth => {
                self.handle_report_health();
                true
            }
            WorkerMessage::GracefulShutdown => {
                self.handle_graceful_shutdown();
                true
            }
            WorkerMessage::Shutdown => {
                self.shutdown.store(true, Ordering::Release);
                true
            }
            WorkerMessage::Noop => true,
        }
    }

    fn handle_timer_schedule(&mut self, timer: TimerSchedule) -> bool {
        let (handle, deadline_ns) = timer.into_parts();
        if handle.worker_id() != self.worker_id {
            return false;
        }
        self.enqueue_timer_entry(deadline_ns, handle).is_some()
    }

    fn handle_rebalance_partitions(
        &mut self,
        partition_start: usize,
        partition_end: usize,
    ) -> bool {
        self.partition_start = partition_start;
        self.partition_end = partition_end;
        self.partition_len = partition_end.saturating_sub(partition_start);

        // Sync partition summary to reflect new partition boundaries.
        // This will also update the tasks-available bit based on actual partition state.
        let waker_id = self.worker_id as usize;
        self.service.wakers[waker_id].sync_partition_summary(
            partition_start,
            partition_end,
            &self.service.summary().leaf_words,
        );

        true
    }

    fn handle_migrate_tasks(&mut self, task_handles: Vec<TaskHandle>) -> bool {
        // Receive migrated tasks from another worker
        // Add them to our yield queue for processing
        if task_handles.is_empty() {
            return false;
        }

        let count = task_handles.len();
        for handle in task_handles {
            self.enqueue_yield(handle);
        }

        #[cfg(debug_assertions)]
        {
            eprintln!(
                "[Worker {}] Received {} migrated tasks",
                self.worker_id, count
            );
        }

        true
    }

    fn handle_report_health(&mut self) {
        // Capture current state snapshot
        let snapshot = WorkerHealthSnapshot {
            worker_id: self.worker_id,
            timestamp_ns: self.timer_wheel.now_ns(),
            stats: self.stats.clone(),
            yield_queue_len: self.yield_queue.len(),
            mpsc_queue_len: 0, // TODO: Add len() method to mpsc::Receiver
            active_leaf_partitions: (self.partition_start..self.partition_end).collect(),
            has_work: self.has_work(),
        };

        // For now, just log the health snapshot
        // TODO: Send this back to supervisor via a response channel
        #[cfg(debug_assertions)]
        {
            eprintln!(
                "[Worker {}] Health: tasks_polled={}, yield_queue={}, mpsc_queue={}, partitions={:?}, has_work={}",
                snapshot.worker_id,
                snapshot.stats.tasks_polled,
                snapshot.yield_queue_len,
                snapshot.mpsc_queue_len,
                snapshot.active_leaf_partitions,
                snapshot.has_work
            );
        }
        let _ = snapshot; // Suppress unused warning in release builds
    }

    fn handle_graceful_shutdown(&mut self) {
        // Graceful shutdown: just process a few more iterations then shutdown
        // The worker loop will naturally drain queues during normal operation

        #[cfg(debug_assertions)]
        {
            eprintln!(
                "[Worker {}] Received graceful shutdown signal",
                self.worker_id
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
        self.shutdown.store(true, Ordering::Release);

        #[cfg(debug_assertions)]
        {
            eprintln!("[Worker {}] Set shutdown flag", self.worker_id);
        }
    }

    fn enqueue_timer_entry(&mut self, deadline_ns: u64, handle: TimerHandle) -> Option<u64> {
        match self.timer_wheel.schedule_timer(deadline_ns, handle) {
            Ok(timer_id) => Some(timer_id),
            Err(_) => None, // Log error in debug mode
        }
    }

    fn poll_timers(&mut self) -> bool {
        let now_ns = self.timer_wheel.now_ns();
        let expired = self
            .timer_wheel
            .poll(now_ns, TIMER_EXPIRE_BUDGET, &mut self.timer_output);
        if expired == 0 {
            return false;
        }

        let mut progress = false;
        for idx in 0..expired {
            let (timer_id, deadline_ns, handle) = self.timer_output[idx];
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
        if handle.worker_id() != self.worker_id {
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
        self.stats.timer_fires = self.stats.timer_fires.saturating_add(1);
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

        if worker_id == self.worker_id {
            if self
                .timer_wheel
                .cancel_timer(timer_id)
                .unwrap_or(None)
                .is_some()
            {
                timer.mark_cancelled(timer_id);
                return true;
            }
            return false;
        }

        self.service
            .post_cancel_message(self.worker_id, worker_id, timer_id)
    }

    #[inline]
    fn cancel_remote_timer(&mut self, worker_id: u32, timer_id: u64) -> bool {
        if worker_id != self.worker_id {
            return false;
        }

        self.service
            .post_cancel_message(self.worker_id, worker_id, timer_id)
    }

    #[inline(always)]
    pub fn next_u64(&mut self) -> u64 {
        self.rng.next()
    }

    #[inline(always)]
    pub fn poll_yield(&mut self, max: usize) -> usize {
        let mut count = 0;
        while let Some(handle) = self.try_acquire_local_yield() {
            self.stats.yield_queue_polls += 1;
            self.poll_handle(handle);
            count += 1;
            if count >= max {
                break;
            }
        }
        count
    }

    #[inline(always)]
    pub fn poll_yield_steal(&mut self, max: usize) -> usize {
        let mut count = 0;
        while let Some(handle) = self.try_steal_yielded() {
            self.stats.yield_queue_polls += 1;
            self.poll_handle(handle);
            count += 1;
            if count >= max {
                break;
            }
        }
        count
    }

    #[inline(always)]
    pub fn run_until_idle(&mut self) -> usize {
        let mut processed = 0;
        while self.run_once() {
            processed += 1;
        }
        processed += self.poll_yield_steal(32);
        while self.run_once() {
            processed += 1;
        }
        processed
    }

    #[inline(always)]
    pub fn poll_blocking(&mut self, strategy: &WaitStrategy) -> bool {
        let mut spins = 0usize;
        loop {
            if self.run_once() {
                return true;
            }

            // Fast path: check if we have any work before parking
            if !self.has_work() {
                if strategy.spin_before_sleep > 0 && spins < strategy.spin_before_sleep {
                    spins += 1;
                    core::hint::spin_loop();
                    continue;
                }

                spins = 0;

                // Calculate park duration considering both strategy and timer deadlines
                let park_duration = self.calculate_park_duration();

                // Sync partition summary from SummaryTree before parking
                let waker_id = self.worker_id as usize;
                self.service.wakers[waker_id].sync_partition_summary(
                    self.partition_start,
                    self.partition_end,
                    &self.service.summary().leaf_words,
                );

                match park_duration {
                    Some(timeout) if timeout.is_zero() => {
                        // Timer ready or zero timeout - don't park
                        return false;
                    }
                    Some(timeout) => {
                        // Park with timeout on WorkerWaker
                        if !self.service.wakers[waker_id].acquire_timeout(timeout) {
                            // Timed out - possibly for timer deadline
                            return false;
                        }
                    }
                    None => {
                        // No timeout - park indefinitely on WorkerWaker
                        self.service.wakers[waker_id].acquire();
                    }
                }
            }
        }
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

        self.stats.leaf_summary_checks = self.stats.leaf_summary_checks.saturating_add(1);
        let mut available =
            self.service.summary().leaf_words[leaf_idx].load(Ordering::Acquire) & mask;
        if available == 0 {
            self.stats.empty_scans = self.stats.empty_scans.saturating_add(1);
            return None;
        }
        self.stats.leaf_summary_hits = self.stats.leaf_summary_hits.saturating_add(1);

        let signals = self.service.arena().active_signals(leaf_idx);
        let mut attempts = signals_per_leaf;

        while available != 0 && attempts > 0 {
            let start = (self.next_u64() as usize) % signals_per_leaf;

            let (signal_idx, signal) = loop {
                let candidate = bits::find_nearest(available, start as u64);
                if candidate >= 64 {
                    self.stats.leaf_summary_checks =
                        self.stats.leaf_summary_checks.saturating_add(1);
                    available =
                        self.service.summary().leaf_words[leaf_idx].load(Ordering::Acquire) & mask;
                    if available == 0 {
                        self.stats.empty_scans = self.stats.empty_scans.saturating_add(1);
                        return None;
                    }
                    self.stats.leaf_summary_hits = self.stats.leaf_summary_hits.saturating_add(1);
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
                self.stats.cas_failures = self.stats.cas_failures.saturating_add(1);
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

            self.stats.signal_polls = self.stats.signal_polls.saturating_add(1);
            let slot_idx = signal_idx * 64 + bit_idx as usize;
            let task = unsafe { self.service.arena().task(leaf_idx, slot_idx) };
            return Some(TaskHandle::from_task(task));
        }

        self.stats.empty_scans = self.stats.empty_scans.saturating_add(1);
        None
    }

    #[inline(always)]
    fn enqueue_yield(&mut self, handle: TaskHandle) {
        let was_empty = self.yield_queue.push_with_status(handle);
        if was_empty {
            // Set yield_bit in WorkerWaker status
            self.service.wakers[self.worker_id as usize].mark_yield();
        }
    }

    #[inline(always)]
    fn try_acquire_local_yield(&mut self) -> Option<TaskHandle> {
        let (item, was_last) = self.yield_queue.pop_with_status();
        if let Some(handle) = item {
            if was_last {
                // Clear yield_bit in WorkerWaker status
                self.service.wakers[self.worker_id as usize].try_unmark_yield();
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
                .try_yield_steal(self.worker_id as usize, 1, next_rand as usize);
        if success {
            self.stats.steal_attempts += attempts;
            self.stats.steal_successes += 1;
            self.try_acquire_local_yield()
        } else {
            None
        }
    }

    #[inline(always)]
    fn process_leaf(&mut self, leaf_idx: usize) -> bool {
        if let Some(handle) = self.try_acquire_task(leaf_idx) {
            self.poll_handle(handle);
            return true;
        }
        false
    }

    #[inline(always)]
    fn try_partition_random(&mut self) -> bool {
        let partition_len = self.partition_len;

        if partition_len.is_power_of_two() {
            let mask = partition_len - 1;
            for _ in 0..partition_len {
                let start_offset = self.next_u64() as usize & mask;
                let leaf_idx = self.partition_start + start_offset;
                if self.process_leaf(leaf_idx) {
                    return true;
                }
            }
        } else {
            for _ in 0..partition_len {
                let start_offset = self.next_u64() as usize % partition_len;
                let leaf_idx = self.partition_start + start_offset;
                if self.process_leaf(leaf_idx) {
                    return true;
                }
            }
        }

        false
    }

    #[inline(always)]
    fn try_partition_linear(&mut self) -> bool {
        let partition_start = self.partition_start;
        let partition_len = self.partition_len;

        for offset in 0..partition_len {
            let leaf_idx = partition_start + offset;
            if self.process_leaf(leaf_idx) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn try_any_partition_random(&mut self, leaf_count: usize) -> bool {
        if leaf_count.is_power_of_two() {
            let leaf_mask = leaf_count - 1;
            for _ in 0..leaf_count {
                let leaf_idx = self.next_u64() as usize & leaf_mask;
                self.stats.leaf_steal_attempts += 1;
                if self.process_leaf(leaf_idx) {
                    self.stats.leaf_steal_successes += 1;
                    return true;
                }
            }
        } else {
            for _ in 0..leaf_count {
                let leaf_idx = self.next_u64() as usize % leaf_count;
                self.stats.leaf_steal_attempts += 1;
                if self.process_leaf(leaf_idx) {
                    self.stats.leaf_steal_successes += 1;
                    return true;
                }
            }
        }
        false
    }

    #[inline(always)]
    fn try_any_partition_linear(&mut self, leaf_count: usize) -> bool {
        for leaf_idx in 0..leaf_count {
            self.stats.leaf_steal_attempts += 1;
            if self.process_leaf(leaf_idx) {
                self.stats.leaf_steal_successes += 1;
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn poll_handle(&mut self, handle: TaskHandle) {
        let task = handle.task();
        struct ActiveTaskGuard;
        impl Drop for ActiveTaskGuard {
            fn drop(&mut self) {
                CURRENT_TASK.set(std::ptr::null_mut());
            }
        }
        CURRENT_TASK.set(task as *const Task as *mut Task);
        let _active_guard = ActiveTaskGuard;

        self.stats.tasks_polled += 1;

        if !task.is_yielded() {
            task.begin();
        } else {
            task.record_yield();
            task.clear_yielded();
        }

        self.poll_task(handle, task);
    }

    fn poll_task(&mut self, handle: TaskHandle, task: &Task) {
        let waker = unsafe { task.waker_yield() };
        let mut cx = Context::from_waker(&waker);
        let poll_result = unsafe { task.poll_future(&mut cx) };

        match poll_result {
            Some(Poll::Ready(())) => {
                task.finish();
                self.stats.completed_count = self.stats.completed_count.saturating_add(1);
                CURRENT_TASK.set(std::ptr::null_mut());
                if let Some(ptr) = task.take_future() {
                    unsafe { FutureAllocator::drop_boxed(ptr) };
                }
            }
            Some(Poll::Pending) => {
                if task.is_yielded() {
                    task.record_yield();
                    self.stats.yielded_count += 1;
                    // Yielded Tasks stay in EXECUTING state with yielded set to true
                    // without resetting the signal. All attempts to set the signal
                    // will not set it since it is guaranteed to run via the yield queue.
                    self.enqueue_yield(handle);
                } else {
                    self.stats.waiting_count += 1;
                    task.finish();
                }
            }
            None => {
                self.stats.completed_count += 1;
                task.finish();
            }
        }
    }

    fn schedule_timer_for_current_task(&mut self, timer: &Timer, delay: Duration) -> Option<u64> {
        let task_ptr = CURRENT_TASK.with(|cell| cell.get()) as *mut Task;
        if task_ptr.is_null() {
            return None;
        }

        let task = unsafe { &*task_ptr };
        let Some(slot) = task.slot() else {
            return None;
        };

        if timer.is_scheduled() {
            if let Some(worker_id) = timer.worker_id() {
                if worker_id == self.worker_id {
                    let existing_id = timer.timer_id();
                    if self
                        .timer_wheel
                        .cancel_timer(existing_id)
                        .unwrap_or(None)
                        .is_some()
                    {
                        timer.reset();
                    }
                } else {
                    // TODO: support cross-worker cancellation when necessary.
                }
            }
        }

        let delay_ns = delay.as_nanos().min(u128::from(u64::MAX)) as u64;
        let now = self.timer_wheel.now_ns();
        let deadline_ns = now.saturating_add(delay_ns);

        let handle = timer.prepare(slot, task.global_id(), self.worker_id);

        if let Some(timer_id) = self.enqueue_timer_entry(deadline_ns, handle) {
            timer.commit_schedule(timer_id, deadline_ns);
            Some(deadline_ns)
        } else {
            timer.reset();
            None
        }
    }
}

fn make_task_waker(slot: &TaskSlot) -> Waker {
    let ptr = slot as *const TaskSlot as *const ();
    unsafe { Waker::from_raw(RawWaker::new(ptr, &TASK_WAKER_VTABLE)) }
}

unsafe fn task_waker_clone(ptr: *const ()) -> RawWaker {
    RawWaker::new(ptr, &TASK_WAKER_VTABLE)
}

unsafe fn task_waker_wake(ptr: *const ()) {
    unsafe {
        let slot = &*(ptr as *const TaskSlot);
        let task_ptr = slot.task_ptr();
        if task_ptr.is_null() {
            return;
        }
        let task = &*task_ptr;
        task.schedule();
    }
}

unsafe fn task_waker_wake_by_ref(ptr: *const ()) {
    unsafe {
        task_waker_wake(ptr);
    }
}

unsafe fn task_waker_drop(_: *const ()) {}

static TASK_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    task_waker_clone,
    task_waker_wake,
    task_waker_wake_by_ref,
    task_waker_drop,
);

/// Schedules a timer for the task currently being polled by the active worker.
pub fn schedule_timer_for_current_task(
    _cx: &Context<'_>,
    timer: &Timer,
    delay: Duration,
) -> Option<u64> {
    let timer_wheel = current_timer_wheel()?;
    let task = current_task()?;
    let worker_id = timer_wheel.worker_id();
    let slot = task.slot()?;

    // Cancel existing timer if scheduled
    if timer.is_scheduled() {
        if let Some(existing_worker_id) = timer.worker_id() {
            if existing_worker_id == worker_id {
                let existing_id = timer.timer_id();
                if timer_wheel
                    .cancel_timer(existing_id)
                    .unwrap_or(None)
                    .is_some()
                {
                    timer.reset();
                }
            } else {
                // Cross-worker cancellation: send message to the worker that owns the timer
                let existing_id = timer.timer_id();
                let ops = CROSS_WORKER_OPS.with(|cell| cell.get());
                if let Some(ops) = ops {
                    if ops.post_cancel_message(worker_id, existing_worker_id, existing_id) {
                        timer.reset();
                    }
                }
            }
        }
    }

    let delay_ns = delay.as_nanos().min(u128::from(u64::MAX)) as u64;
    let now = timer_wheel.now_ns();
    let deadline_ns = now.saturating_add(delay_ns);

    let handle = timer.prepare(slot, task.global_id(), worker_id);

    match timer_wheel.schedule_timer(deadline_ns, handle) {
        Ok(timer_id) => {
            timer.commit_schedule(timer_id, deadline_ns);
            Some(deadline_ns)
        }
        Err(_) => {
            timer.reset();
            None
        }
    }
}

/// Cancels a timer owned by the task currently being polled by the active worker.
pub fn cancel_timer_for_current_task(timer: &Timer) -> bool {
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

    let timer_wheel = current_timer_wheel();
    let Some(timer_wheel) = timer_wheel else {
        return false;
    };

    let worker_id = timer_wheel.worker_id();

    if timer_worker_id == worker_id {
        if timer_wheel.cancel_timer(timer_id).unwrap_or(None).is_some() {
            timer.mark_cancelled(timer_id);
            return true;
        }
        return false;
    }

    let ops = CROSS_WORKER_OPS.with(|cell| cell.get());
    let Some(ops) = ops else {
        return false;
    };

    // Cross-worker cancellation: send message to the worker that owns the timer
    if ops.post_cancel_message(worker_id, timer_worker_id, timer_id) {
        timer.mark_cancelled(timer_id);
        true
    } else {
        false
    }
}

/// Returns the most recent `now_ns` observed by the active worker.
pub fn current_worker_now_ns() -> Option<u64> {
    let timer_wheel = current_timer_wheel()?;
    Some(timer_wheel.now_ns())
}
