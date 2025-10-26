use crate::PushError;
use crate::deque::{StealStatus, Stealer, Worker as YieldWorker};
use crate::mpsc;
use crate::mpsc::Receiver;
use crate::task::{
    ExecutorArena, FutureAllocator, TASK_EXECUTING, TASK_SCHEDULED, Task, TaskHandle, TaskSlot,
};
use crate::timer::{Timer, TimerHandle};
use crate::timer_wheel::TimerWheel;
use crate::{PopError, bits};
use std::cell::Cell;
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};

use rand::RngCore;

const DEFAULT_QUEUE_SEG_SHIFT: usize = 10;
const DEFAULT_QUEUE_NUM_SEGS_SHIFT: usize = 6;
const MAX_WORKER_LIMIT: usize = 512;

const RND_MULTIPLIER: u64 = 0x5DEECE66D;
const RND_ADDEND: u64 = 0xB;
const RND_MASK: u64 = (1 << 48) - 1;
const DEFAULT_WAKE_BURST: usize = 4;
const FULL_SUMMARY_SCAN_CADENCE_MASK: u64 = 1023;
const DEFAULT_TICK_DURATION_NS: u64 = 1 << 20; // ~1.05ms, power of two as required by TimerWheel
const TIMER_TICKS_PER_WHEEL: usize = 1024;
const TIMER_EXPIRE_BUDGET: usize = 256;
const MESSAGE_BATCH_SIZE: usize = 4096;

thread_local! {
    static CURRENT_WORKER: Cell<*mut ()> = Cell::new(ptr::null_mut());
    static CURRENT_TASK: Cell<*mut Task> = Cell::new(ptr::null_mut());
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

pub fn current_task<'a>() -> Option<&'a mut Task> {
    let task = CURRENT_TASK.with(|cell| cell.get());
    if task.is_null() {
        return None;
    } else {
        unsafe { Some(&mut *task) }
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
    ScheduleTimer { timer: TimerSchedule },
    ScheduleBatch { timers: TimerBatch },
    CancelTimer { worker_id: u32, timer_id: u64 },
    WorkerCountChanged { new_worker_count: u16 },
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
            tick_duration: Duration::from_micros(500),
            min_workers: num_cpus::get(),
            max_workers: num_cpus::get(),
        }
    }
}

pub struct WorkerRegistration<
    const P: usize = DEFAULT_QUEUE_SEG_SHIFT,
    const NUM_SEGS_P2: usize = DEFAULT_QUEUE_NUM_SEGS_SHIFT,
> {
    pub worker_id: u32,
    pub sender: mpsc::Sender<WorkerMessage, P, NUM_SEGS_P2>,
    pub receiver: mpsc::Receiver<WorkerMessage, P, NUM_SEGS_P2>,
    pub yield_queue: YieldWorker<TaskHandle>,
    pub now_ns: Arc<AtomicU64>,
    pub tick_duration_ns: u64,
}

struct WorkerEntry<
    const P: usize = DEFAULT_QUEUE_SEG_SHIFT,
    const NUM_SEGS_P2: usize = DEFAULT_QUEUE_NUM_SEGS_SHIFT,
> {
    id: u32,
    active: AtomicU64,
    now_ns: AtomicU64,
    shutdown: AtomicU64,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
}

pub struct WorkerService<
    const P: usize = DEFAULT_QUEUE_SEG_SHIFT,
    const NUM_SEGS_P2: usize = DEFAULT_QUEUE_NUM_SEGS_SHIFT,
> {
    arena: Arc<ExecutorArena>,
    config: WorkerServiceConfig,
    tick_duration: Duration,
    tick_duration_ns: u64,
    clock_ns: Arc<AtomicU64>,
    worker_actives: Box<[AtomicU64]>,
    worker_now_ns: Box<[AtomicU64]>,
    worker_shutdowns: Box<[AtomicBool]>,
    worker_threads: Box<[Mutex<Option<thread::JoinHandle<()>>>]>,
    worker_stats: Box<[WorkerStats]>,
    worker_count: AtomicUsize,
    worker_max_id: AtomicUsize,
    receivers: Box<[mpsc::Receiver<WorkerMessage, P, NUM_SEGS_P2>]>,
    senders: Box<[mpsc::Sender<WorkerMessage, P, NUM_SEGS_P2>]>,
    yield_queues: Box<[YieldWorker<TaskHandle>]>,
    yield_stealers: Box<[Stealer<TaskHandle>]>,
    timers: Box<[TimerWheel<TimerHandle>]>,
    shutdown: AtomicBool,
    tick_thread: Mutex<Option<thread::JoinHandle<()>>>,
    register_mutex: Mutex<()>,
}

unsafe impl<const P: usize, const NUM_SEGS_P2: usize> Send for WorkerService<P, NUM_SEGS_P2> {}

impl<const P: usize, const NUM_SEGS_P2: usize> WorkerService<P, NUM_SEGS_P2> {
    pub fn start(arena: Arc<ExecutorArena>, config: WorkerServiceConfig) -> Arc<Self> {
        let tick_duration = config.tick_duration;
        let tick_duration_ns = normalize_tick_duration_ns(config.tick_duration);
        let min_workers = config.min_workers.max(1);
        let max_workers = config.max_workers.max(min_workers).min(MAX_WORKER_LIMIT);

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
            let rx = mpsc::new();
            receivers.push(rx);
        }
        for worker_id in 0..max_workers {
            for other_worker_id in 0..max_workers {
                senders.push(
                    receivers[worker_id]
                        .create_sender_with_config(0)
                        .expect("ran out of producer slots"),
                )
            }
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
        for _ in 0..max_workers {
            timers.push(TimerWheel::new(
                Instant::now(),
                tick_duration,
                TIMER_TICKS_PER_WHEEL,
            ));
        }

        let service = Arc::new(Self {
            arena,
            config,
            tick_duration: tick_duration,
            tick_duration_ns,
            clock_ns: Arc::new(AtomicU64::new(0)),
            worker_actives: worker_actives.into_boxed_slice(),
            worker_now_ns: worker_now_ns.into_boxed_slice(),
            worker_shutdowns: worker_shutdowns.into_boxed_slice(),
            worker_threads: worker_threads.into_boxed_slice(),
            worker_stats: worker_stats.into_boxed_slice(),
            worker_count: AtomicUsize::new(0),
            worker_max_id: AtomicUsize::new(0),
            receivers: receivers.into_boxed_slice(),
            senders: senders.into_boxed_slice(),
            yield_queues: yield_queues.into_boxed_slice(),
            yield_stealers: yield_stealers.into_boxed_slice(),
            timers: timers.into_boxed_slice(),
            shutdown: AtomicBool::new(false),
            tick_thread: Mutex::new(None),
            register_mutex: Mutex::new(()),
        });

        Self::spawn_tick_thread(&service);
        service
    }

    pub fn spawn_worker(self: &Arc<Self>) -> Result<(), PushError<()>> {
        let _ = self.register_mutex.lock().expect("register_mutex poisoned");
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
        if worker_id > self.worker_max_id.load(Ordering::SeqCst) {
            self.worker_max_id.store(worker_id, Ordering::SeqCst);
        }
        self.worker_count.fetch_add(1, Ordering::SeqCst);

        let arena = Arc::clone(&self.arena);
        let service = Arc::clone(self);
        let shutdown = Arc::new(AtomicBool::new(false));
        let timer_resolution_ns = service.tick_duration().as_nanos().max(1) as u64;
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
                arena,
                service: Arc::clone(&service),
                wait_strategy: WaitStrategy::default(),
                shutdown: &service.worker_shutdowns[worker_id],
                now_ns: &service.worker_now_ns[worker_id],
                receiver: unsafe {
                    &mut *(&service.receivers[worker_id] as *const _
                        as *mut mpsc::Receiver<WorkerMessage, P, NUM_SEGS_P2>)
                },
                yield_queue: &service.yield_queues[worker_id],
                timer_wheel: unsafe {
                    &mut *(&service.timers[worker_id] as *const _ as *mut TimerWheel<TimerHandle>)
                },
                wake_stats: WakeStats::default(),
                stats: WorkerStats::default(),
                partition_start: 0,
                partition_end: 0,
                cached_worker_count: 0,
                wake_burst_limit: DEFAULT_WAKE_BURST,
                worker_id: worker_id as u32,
                timer_resolution_ns,
                timer_output,
                message_batch: message_batch.into_boxed_slice(),
                seed: rand::rng().next_u64(),
                current_task: std::ptr::null_mut(),
            };
            std::panic::catch_unwind(|| {});
            w.run();
        });

        Ok(())
    }

    pub(crate) fn post_message(
        &self,
        from_worker_id: u32,
        to_worker_id: u32,
        message: WorkerMessage,
    ) -> Result<(), PushError<WorkerMessage>> {
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
        if self.shutdown.swap(true, Ordering::Release) {
            return;
        }

        if let Some(handle) = self.tick_thread.lock().unwrap().take() {
            let _ = handle.join();
        }
    }

    fn spawn_tick_thread(self: &Arc<Self>) {
        let service = Arc::clone(self);
        let tick_duration = service.tick_duration;
        let handle = thread::spawn(move || {
            let start = Instant::now();
            loop {
                if service.shutdown.load(Ordering::Acquire) {
                    break;
                }

                let now_ns = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                service.clock_ns.store(now_ns, Ordering::Release);
                let max_workers = service.worker_max_id.load(Ordering::Relaxed);
                for i in 0..max_workers {
                    service.worker_now_ns[i].store(now_ns, Ordering::Release);
                }
                thread::sleep(tick_duration);
            }
        });

        *self.tick_thread.lock().unwrap() = Some(handle);
    }
}

impl<const P: usize, const NUM_SEGS_P2: usize> Drop for WorkerService<P, NUM_SEGS_P2> {
    fn drop(&mut self) {
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

#[derive(Clone, Copy, Debug, Default)]
pub struct WakeStats {
    pub release_calls: u64,
    pub released_permits: u64,
    pub last_backlog: usize,
    pub max_backlog: usize,
    pub queue_release_calls: u64,
}

// #[derive(Clone, Copy, Debug)]
// pub struct WaitStrategy {
//     pub spin_before_sleep: usize,
//     pub park_timeout: Option<Duration>,
// }

// impl WaitStrategy {
//     pub fn new(spin_before_sleep: usize, park_timeout: Option<Duration>) -> Self {
//         Self {
//             spin_before_sleep,
//             park_timeout,
//         }
//     }

//     pub fn non_blocking() -> Self {
//         Self::new(0, Some(Duration::from_secs(0)))
//     }

//     pub fn park_immediately() -> Self {
//         Self::new(0, None)
//     }
// }

// impl Default for WaitStrategy {
//     fn default() -> Self {
//         Self::new(64, Some(Duration::from_millis(1)))
//     }
// }

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
        Self::new(64, Some(Duration::from_millis(1)))
    }
}

pub struct Worker<
    'a,
    const P: usize = DEFAULT_QUEUE_SEG_SHIFT,
    const NUM_SEGS_P2: usize = DEFAULT_QUEUE_NUM_SEGS_SHIFT,
> {
    arena: Arc<ExecutorArena>,
    service: Arc<WorkerService<P, NUM_SEGS_P2>>,
    wait_strategy: WaitStrategy,
    shutdown: &'a AtomicBool,
    now_ns: &'a AtomicU64,
    receiver: &'a mut mpsc::Receiver<WorkerMessage, P, NUM_SEGS_P2>,
    yield_queue: &'a YieldWorker<TaskHandle>,
    timer_wheel: &'a mut TimerWheel<TimerHandle>,
    wake_stats: WakeStats,
    stats: WorkerStats,
    partition_start: usize,
    partition_end: usize,
    cached_worker_count: usize,
    wake_burst_limit: usize,
    worker_id: u32,
    timer_resolution_ns: u64,
    timer_output: Vec<(u64, u64, TimerHandle)>,
    message_batch: Box<[WorkerMessage]>,
    seed: u64,
    pub(crate) current_task: *mut Task,
}

impl<'a, const P: usize, const NUM_SEGS_P2: usize> Worker<'a, P, NUM_SEGS_P2> {
    // pub fn new(arena: Arc<ExecutorArena>, service: Arc<WorkerService<P, NUM_SEGS_P2>>) -> Self {
    //     Self::with_shutdown(arena, service, Arc::new(AtomicBool::new(false)))
    // }

    // pub fn with_shutdown(
    //     arena: Arc<ExecutorArena>,
    //     service: Arc<WorkerService<P, NUM_SEGS_P2>>,
    //     shutdown: Arc<AtomicBool>,
    // ) -> Self {
    //     let timer_resolution_ns = service.tick_duration().as_nanos().max(1) as u64;
    //     let timer_wheel = TimerWheel::new(
    //         Instant::now(),
    //         Duration::from_nanos(timer_resolution_ns),
    //         TIMER_TICKS_PER_WHEEL,
    //     );

    //     let message_batch = Vec::<WorkerMessage>::with_capacity(MESSAGE_BATCH_SIZE);
    //     for _ in 0..MESSAGE_BATCH_SIZE {
    //         message_batch.push(WorkerMessage::Noop);
    //     }

    //     Self {
    //         arena,
    //         service,
    //         shutdown,
    //         wait_strategy: WaitStrategy::default(),
    //         sender: registration.sender,
    //         receiver: registration.receiver,
    //         now_ns: registration.now_ns,
    //         worker_id: registration.worker_id,
    //         timer_resolution_ns,
    //         timer_wheel,
    //         timer_output: Vec::with_capacity(TIMER_EXPIRE_BUDGET),
    //         message_batch: message_batch.into_boxed_slice(),
    //         wake_stats: WakeStats::default(),
    //         stats: WorkerStats::default(),
    //         seed: rand::rng().next_u64(),
    //         current_task: std::ptr::null_mut(),
    //     }
    // }

    #[inline]
    pub fn stats(&self) -> &WorkerStats {
        &self.stats
    }

    pub fn set_wait_strategy(&mut self, strategy: WaitStrategy) {
        self.wait_strategy = strategy;
    }

    pub fn run(&mut self) {
        while !self.shutdown.load(Ordering::Acquire) {
            let progress = self.run_once();
            // if !progress {
            //     self.wait_strategy.wait();
            // }
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
                Err(PopError::Empty) | Err(PopError::Timeout) => break,
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

    fn enqueue_timer_entry(&mut self, deadline_ns: u64, handle: TimerHandle) -> Option<u64> {
        self.timer_wheel.schedule_timer(deadline_ns, handle)
    }

    fn poll_timers(&mut self) -> bool {
        let now_ns = self.now_ns.load(Ordering::Acquire);
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
            if self.timer_wheel.cancel_timer(timer_id).is_some() {
                timer.mark_cancelled(timer_id);
                return true;
            }
            return false;
        }

        // TODO (Phase 2): support cross-worker cancellation.
        false
    }

    fn cancel_remote_timer(&mut self, worker_id: u32, timer_id: u64) -> bool {
        if worker_id != self.worker_id {
            return false;
        }

        self.timer_wheel.cancel_timer(timer_id).is_some()
    }

    #[inline(always)]
    pub fn next_u64(&mut self) -> u64 {
        let old_seed = self.seed;
        let next_seed = (old_seed
            .wrapping_mul(RND_MULTIPLIER)
            .wrapping_add(RND_ADDEND))
            & RND_MASK;
        self.seed = next_seed;
        next_seed >> 16
    }

    #[inline(always)]
    pub fn run_once(&mut self) -> bool {
        let leaf_count = self.arena.leaf_count();
        if leaf_count == 0 {
            return false;
        }

        let mut did_work = false;

        // if self.poll_yield(self.yield_queue.len()) > 0 {
        if self.poll_yield(4) > 0 {
            did_work = true;
        }

        self.refresh_partition(leaf_count);

        if self.try_partition_random(leaf_count) {
            did_work = true;
        }

        // if self.try_partition_random(leaf_count) {
        //     did_work = true;
        // } else if self.try_partition_linear(leaf_count) {
        //     did_work = true;
        // }

        let mask = leaf_count - 1;
        let rand = self.next_u64();

        if !did_work || rand & FULL_SUMMARY_SCAN_CADENCE_MASK == 0 {
            // if self.try_any_partition_random(leaf_count) {
            //     did_work = true;
            // }

            if self.try_any_partition_random(leaf_count) {
                did_work = true;
            } else if self.try_any_partition_linear(leaf_count) {
                did_work = true;
            }
        }

        if self.poll_timers() {
            did_work = true;
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

            if strategy.spin_before_sleep > 0 && spins < strategy.spin_before_sleep {
                spins += 1;
                core::hint::spin_loop();
                continue;
            }

            spins = 0;

            match strategy.park_timeout {
                Some(timeout) => {
                    if timeout.is_zero() {
                        return false;
                    }
                    if !self.arena.active_tree().acquire_timeout(timeout) {
                        return false;
                    }
                }
                None => {
                    self.arena.active_tree().acquire();
                }
            }
        }
    }

    pub fn run_blocking(&mut self, strategy: &WaitStrategy) {
        while self.poll_blocking(strategy) {}
    }

    #[inline(always)]
    fn refresh_partition(&mut self, leaf_count: usize) {
        let worker_count = self.arena.active_tree().worker_count().max(1);
        if worker_count == self.cached_worker_count {
            return;
        }

        let (start, end) =
            Self::compute_partition(self.worker_id as usize, leaf_count, worker_count);
        self.partition_start = start.min(leaf_count);
        self.partition_end = end.min(leaf_count);
        self.cached_worker_count = worker_count;
    }

    #[inline(always)]
    fn compute_partition(
        worker_slot: usize,
        leaf_count: usize,
        worker_count: usize,
    ) -> (usize, usize) {
        if leaf_count == 0 || worker_count == 0 {
            return (0, 0);
        }
        let effective_idx = worker_slot % worker_count;
        let base = leaf_count / worker_count;
        let extra = leaf_count % worker_count;

        let start = if effective_idx < extra {
            effective_idx * (base + 1)
        } else {
            extra * (base + 1) + (effective_idx - extra) * base
        };
        let len = if effective_idx < extra {
            base + 1
        } else {
            base
        };
        (start, (start + len).min(leaf_count))
    }

    #[inline(always)]
    fn try_acquire_task(&mut self, leaf_idx: usize) -> Option<TaskHandle> {
        let signals_per_leaf = self.arena.signals_per_leaf();
        if signals_per_leaf == 0 {
            return None;
        }

        let mask = if signals_per_leaf >= 64 {
            u64::MAX
        } else {
            (1u64 << signals_per_leaf) - 1
        };

        self.stats.leaf_summary_checks += 1;
        let mut available = self.arena.active_summary(leaf_idx).load(Ordering::Acquire) & mask;
        if available == 0 {
            self.stats.empty_scans += 1;
            return None;
        }
        self.stats.leaf_summary_hits += 1;

        let signals = self.arena.active_signals(leaf_idx);
        let mut attempts = signals_per_leaf;

        while available != 0 && attempts > 0 {
            let start = (self.next_u64() as usize) % signals_per_leaf;

            let (signal_idx, signal) = loop {
                let candidate = bits::find_nearest(available, start as u64);
                if candidate >= 64 {
                    self.stats.leaf_summary_checks += 1;
                    available = self.arena.active_summary(leaf_idx).load(Ordering::Acquire) & mask;
                    if available == 0 {
                        self.stats.empty_scans += 1;
                        return None;
                    }
                    self.stats.leaf_summary_hits += 1;
                    continue;
                }
                let bit_index = candidate as usize;
                let sig = unsafe { &*self.arena.task_signal_ptr(leaf_idx, bit_index) };
                let bits = sig.load(Ordering::Acquire);
                if bits == 0 {
                    available &= !(1u64 << bit_index);
                    self.arena
                        .active_tree()
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
                bit_candidate as u32
            } else {
                available &= !(1u64 << signal_idx);
                attempts -= 1;
                continue;
            };

            let (remaining, acquired) = signal.try_acquire(bit_idx);
            if !acquired {
                self.stats.cas_failures += 1;
                available &= !(1u64 << signal_idx);
                attempts -= 1;
                continue;
            }

            let remaining_mask = remaining;
            if remaining_mask == 0 {
                self.arena
                    .active_tree()
                    .mark_signal_inactive(leaf_idx, signal_idx);
            }
            self.maybe_release_for_backlog(leaf_idx, signal_idx, remaining_mask);

            self.stats.signal_polls += 1;
            let slot_idx = signal_idx * 64 + bit_idx as usize;
            let task = unsafe { self.arena.task(leaf_idx, slot_idx) };
            return Some(TaskHandle::from_task(task));
        }

        self.stats.empty_scans += 1;
        None
    }

    #[inline(always)]
    fn enqueue_yield(&mut self, handle: TaskHandle) {
        let leaf_idx = handle.leaf_idx();
        let was_empty = self.yield_queue.push_with_status(handle);
        if was_empty {
            self.arena.mark_yield_active(leaf_idx);
        }
        self.maybe_release_for_queue(was_empty);
    }

    #[inline(always)]
    fn try_acquire_local_yield(&mut self) -> Option<TaskHandle> {
        let (item, was_last) = self.yield_queue.pop_with_status();
        if let Some(handle) = item {
            if was_last {
                self.arena.mark_yield_inactive(handle.leaf_idx());
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
    fn maybe_release_for_backlog(&mut self, leaf_idx: usize, signal_idx: usize, remaining: u64) {
        let sleepers = self.arena.active_tree().sleepers();
        if sleepers == 0 {
            return;
        }

        let mut backlog = remaining.count_ones() as usize;

        let summary =
            self.arena.active_summary(leaf_idx).load(Ordering::Relaxed) & !(1u64 << signal_idx);
        backlog += summary.count_ones() as usize;
        backlog += self.yield_queue.len();

        self.wake_stats.last_backlog = backlog;
        if backlog > self.wake_stats.max_backlog {
            self.wake_stats.max_backlog = backlog;
        }

        let to_release = backlog.min(sleepers).min(self.wake_burst_limit);
        if to_release > 0 {
            self.wake_stats.release_calls += 1;
            self.wake_stats.released_permits += to_release as u64;
            self.arena.active_tree().release(to_release);
        }
    }

    #[inline(always)]
    fn maybe_release_for_queue(&mut self, _was_empty: bool) {
        let sleepers = self.arena.active_tree().sleepers();
        if sleepers == 0 {
            return;
        }
        let backlog = self.yield_queue.len();

        self.wake_stats.last_backlog = backlog;
        if backlog > self.wake_stats.max_backlog {
            self.wake_stats.max_backlog = backlog;
        }

        let to_release = backlog.min(sleepers).min(self.wake_burst_limit);
        if to_release > 0 {
            self.wake_stats.queue_release_calls += 1;
            self.wake_stats.released_permits += to_release as u64;
            self.arena.active_tree().release(to_release);
        }
    }

    #[inline(always)]
    fn try_partition_random(&mut self, leaf_count: usize) -> bool {
        let partition_len = self.partition_end.saturating_sub(self.partition_start);
        if partition_len == 0 {
            return false;
        }

        for _ in 0..partition_len {
            let start_offset = self.next_u64() as usize % partition_len;
            let leaf_idx = self.partition_start + start_offset;
            if self.process_leaf(leaf_idx) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn try_partition_linear(&mut self, leaf_count: usize) -> bool {
        let partition_len = self.partition_end.saturating_sub(self.partition_start);
        if partition_len == 0 {
            return false;
        }

        for offset in 0..partition_len {
            let leaf_idx = self.partition_start + offset;
            if self.process_leaf(leaf_idx) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn try_any_partition_random(&mut self, leaf_count: usize) -> bool {
        let leaf_mask = leaf_count - 1;
        for _ in 0..leaf_count {
            let leaf_idx = self.next_u64() as usize & leaf_mask;
            self.stats.leaf_steal_attempts += 1;
            if self.process_leaf(leaf_idx) {
                self.stats.leaf_steal_successes += 1;
                return true;
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

    // fn poll_single_task(&mut self, task: &Task) -> bool {
    //     let Some(slot) = task.slot() else {
    //         return false;
    //     };

    //     self.stats.tasks_polled = self.stats.tasks_polled.saturating_add(1);

    //     task.begin();
    //     CURRENT_WORKER.with(|cell| cell.set(self as *mut Worker));
    //     CURRENT_TASK.with(|cell| cell.set(task as *const Task));

    //     let waker = unsafe { make_task_waker(slot.as_ref()) };
    //     let mut cx = Context::from_waker(&waker);
    //     let poll_result = unsafe { task.poll_future(&mut cx) };

    //     CURRENT_TASK.with(|cell| cell.set(ptr::null()));
    //     CURRENT_WORKER.with(|cell| cell.set(ptr::null_mut()));

    //     match poll_result {
    //         Some(Poll::Ready(())) => {
    //             task.finish();
    //             if let Some(ptr) = task.take_future() {
    //                 unsafe { FutureHelpers::drop_boxed(ptr) };
    //             }
    //             self.stats.completed_count = self.stats.completed_count.saturating_add(1);
    //             true
    //         }
    //         Some(Poll::Pending) => {
    //             task.finish();
    //             true
    //         }
    //         None => {
    //             task.finish();
    //             false
    //         }
    //     }
    // }

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
                    if self.timer_wheel.cancel_timer(existing_id).is_some() {
                        timer.reset();
                    }
                } else {
                    // TODO: support cross-worker cancellation when necessary.
                }
            }
        }

        let delay_ns = delay.as_nanos().min(u128::from(u64::MAX)) as u64;
        let now = self.now_ns.load(Ordering::Acquire);
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
    CURRENT_WORKER.with(|cell| {
        let worker_ptr = cell.get();
        if worker_ptr.is_null() {
            return None;
        }
        unsafe {
            (&mut *(worker_ptr
                as *mut Worker<'static, DEFAULT_QUEUE_SEG_SHIFT, DEFAULT_QUEUE_NUM_SEGS_SHIFT>))
                .schedule_timer_for_current_task(timer, delay)
        }
    })
}

/// Cancels a timer owned by the task currently being polled by the active worker.
pub fn cancel_timer_for_current_task(timer: &Timer) -> bool {
    CURRENT_WORKER.with(|cell| {
        let worker_ptr = cell.get();
        if worker_ptr.is_null() {
            return false;
        }
        unsafe {
            (&mut *(worker_ptr
                as *mut Worker<'static, DEFAULT_QUEUE_SEG_SHIFT, DEFAULT_QUEUE_NUM_SEGS_SHIFT>))
                .cancel_timer(timer)
        }
    })
}

/// Returns the most recent `now_ns` observed by the active worker.
pub fn current_worker_now_ns() -> Option<u64> {
    CURRENT_WORKER.with(|cell| {
        let worker_ptr = cell.get();
        if worker_ptr.is_null() {
            None
        } else {
            Some(unsafe {
                (&mut *(worker_ptr
                    as *mut Worker<'static, DEFAULT_QUEUE_SEG_SHIFT, DEFAULT_QUEUE_NUM_SEGS_SHIFT>))
                    .now_ns
                    .load(Ordering::Acquire)
            })
        }
    })
}
