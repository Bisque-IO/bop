#![feature(thread_id_value)]
//! Minimal Work-Stealing Executor Benchmark
//!
//! IDENTICAL to minimal_spsc_executor_benchmark.rs except:
//! - Work-stealing added to SLOW PATH only (when global queue empty)
//! - Fast path (global queue has work) is COMPLETELY UNCHANGED

use bop_mpmc::selector::Selector;
use bop_mpmc::timer_wheel::TimerWheel;
use bop_mpmc::{EXECUTING, IDLE, Mpmc, PushError, SCHEDULED};
use crossbeam_deque::{Steal, Stealer, Worker};
use futures_lite::FutureExt;
use std::cell::UnsafeCell;
use std::future::Future;
use std::mem::MaybeUninit;
use std::ops::{Add, Sub};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};

// Send-safe wrapper for raw pointers
#[derive(Clone, Copy)]
struct TaskPtr(*mut TaskInner);
unsafe impl Send for TaskPtr {}
unsafe impl Sync for TaskPtr {}

impl TaskPtr {
    fn new(ptr: *mut TaskInner) -> Self {
        Self(ptr)
    }
    fn get(&self) -> *mut TaskInner {
        self.0
    }
}

/// Per-worker statistics
struct WorkerStats {
    fast_path_hits: AtomicU64,        // Tasks from global MPMC queue
    local_queue_hits: AtomicU64,      // Tasks from local queue
    steal_attempts: AtomicU64,        // Number of steal attempts
    steal_successes: AtomicU64,       // Successful steals
    batch_steal_attempts: AtomicU64,  // Attempts to steal from batch
    batch_steal_successes: AtomicU64, // Successful batch steals
    tasks_processed: AtomicU64,       // Total tasks processed
}

impl WorkerStats {
    fn new() -> Self {
        Self {
            fast_path_hits: AtomicU64::new(0),
            local_queue_hits: AtomicU64::new(0),
            steal_attempts: AtomicU64::new(0),
            steal_successes: AtomicU64::new(0),
            batch_steal_attempts: AtomicU64::new(0),
            batch_steal_successes: AtomicU64::new(0),
            tasks_processed: AtomicU64::new(0),
        }
    }
}

/// Work-stealing capable batch with double-ended cursors
/// Worker processes from head (left), stealers take from tail (right)
struct StealableBatch {
    tasks: UnsafeCell<[TaskPtr; 512]>,
    head: AtomicUsize, // Worker processes from here (increments)
    tail: AtomicUsize, // Stealers take from here (decrements)
}

unsafe impl Sync for StealableBatch {}

impl StealableBatch {
    fn new() -> Self {
        Self {
            tasks: UnsafeCell::new(std::array::from_fn(|_| TaskPtr(std::ptr::null_mut()))),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Fill batch from slice, return number filled
    fn fill_from_slice(&self, source: &[TaskPtr], count: usize) -> usize {
        let to_fill = count.min(512);
        unsafe {
            let tasks = &mut *self.tasks.get();
            tasks[..to_fill].copy_from_slice(&source[..to_fill]);
        }
        self.head.store(0, Ordering::Release);
        self.tail.store(to_fill, Ordering::Release);
        to_fill
    }

    /// Get next task for worker (from head)
    #[inline]
    fn pop_front(&self) -> Option<TaskPtr> {
        let h = self.head.fetch_add(1, Ordering::Relaxed);
        let t = self.tail.load(Ordering::Acquire);

        if h >= t {
            // Restore head if we went past tail
            self.head.fetch_sub(1, Ordering::Relaxed);
            None
        } else {
            unsafe { Some((*self.tasks.get())[h]) }
        }
    }

    /// Steal from tail (for other workers)
    #[inline]
    fn steal_back(&self) -> Option<TaskPtr> {
        loop {
            let t = self.tail.load(Ordering::Acquire);
            let h = self.head.load(Ordering::Acquire);

            if t <= h {
                return None; // Empty or all claimed
            }

            // Try to steal from tail
            if self
                .tail
                .compare_exchange(t, t - 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return unsafe { Some((*self.tasks.get())[t - 1]) };
            }
            // CAS failed, retry
        }
    }

    /// Steal a percentage of remaining tasks from tail (bulk stealing)
    /// percentage: 0.0 to 1.0 (e.g., 0.5 = steal half, 1.0 = steal all)
    /// Returns number of tasks stolen and writes them to output buffer
    fn steal_bulk(&self, output: &mut [TaskPtr], percentage: f64) -> usize {
        let percentage = percentage.clamp(0.0, 1.0);

        loop {
            let t = self.tail.load(Ordering::Acquire);
            let h = self.head.load(Ordering::Acquire);

            if t <= h {
                return 0; // Empty or all claimed
            }

            let available = t - h;
            let to_steal = if percentage >= 1.0 {
                available // Steal all
            } else {
                ((available as f64 * percentage).ceil() as usize).max(1)
            };

            // Limit by output buffer size
            let to_steal = to_steal.min(output.len()).min(available);

            if to_steal == 0 {
                return 0;
            }

            let new_tail = t - to_steal;

            // Try to claim the range [new_tail..t)
            if self
                .tail
                .compare_exchange(t, new_tail, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                // Successfully claimed, copy tasks
                unsafe {
                    let tasks = &*self.tasks.get();
                    for i in 0..to_steal {
                        output[i] = tasks[new_tail + i];
                    }
                }
                return to_steal;
            }
            // CAS failed, retry
        }
    }

    /// Get count of remaining tasks (approximate, may race)
    #[inline]
    fn remaining_count(&self) -> usize {
        let t = self.tail.load(Ordering::Relaxed);
        let h = self.head.load(Ordering::Relaxed);
        t.saturating_sub(h)
    }

    /// Check if batch has available work
    #[inline]
    fn has_work(&self) -> bool {
        self.head.load(Ordering::Relaxed) < self.tail.load(Ordering::Acquire)
    }

    /// Reset batch
    fn reset(&self) {
        self.head.store(0, Ordering::Relaxed);
        self.tail.store(0, Ordering::Relaxed);
    }
}

/// Zero-cost yield future that uses the optimized yield path
/// Just calls wake_by_ref which sets the yielded flag via VTABLE_YIELD
/// Tracks state with a single bool - still very lightweight
struct Yield {
    yielded: bool,
}

impl Yield {
    #[inline(always)]
    fn new() -> Self {
        Self { yielded: false }
    }
}

impl Future for Yield {
    type Output = ();

    #[inline(always)]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            // Second poll - we've already yielded, return Ready
            Poll::Ready(())
        } else {
            // First poll - call wake_by_ref which triggers VTABLE_YIELD
            // VTABLE_YIELD sets TaskInner.yielded = true
            // Then return Pending so the executor will:
            // 1. Check the yielded flag (line 852)
            // 2. Fast-path re-enqueue the task without going through full schedule()
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Minimal future that yields N times
struct BadNTimes {
    remaining: usize,
    poll_count: Arc<AtomicUsize>,
    duration: Duration,
}

unsafe impl Send for BadNTimes {}

impl BadNTimes {
    fn new(count: usize, duration: Duration, poll_count: Arc<AtomicUsize>) -> Self {
        Self {
            remaining: count,
            poll_count,
            duration,
        }
    }
}

impl Future for BadNTimes {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.poll_count.fetch_add(1, Ordering::Relaxed);

        if self.remaining == 0 {
            Poll::Ready(())
        } else {
            self.remaining -= 1;
            if self.remaining > 0 {
                cx.waker().wake_by_ref();
            }
            std::thread::sleep(self.duration);
            Poll::Pending
        }
    }
}

/// Minimal future that yields N times
struct YieldNTimes {
    remaining: usize,
    poll_count: Arc<AtomicUsize>,
}

unsafe impl Send for YieldNTimes {}

impl YieldNTimes {
    fn new(count: usize, poll_count: Arc<AtomicUsize>) -> Self {
        Self {
            remaining: count,
            poll_count,
        }
    }
}

impl Future for YieldNTimes {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.poll_count.fetch_add(1, Ordering::Relaxed);

        if self.remaining == 0 {
            Poll::Ready(())
        } else {
            self.remaining -= 1;
            if self.remaining > 0 {
                cx.waker().wake_by_ref();
            }
            Poll::Pending
        }
    }
}

/// Async function version of YieldNTimes using zero-cost Yield
async fn yield_n_times(count: usize, poll_count: Arc<AtomicUsize>) {
    for _ in 0..count {
        poll_count.fetch_add(1, Ordering::Relaxed);
        Yield::new().await;
    }
}

/// Thread-safe wrapper around high-performance hashed TimerWheel
struct TimerWheelShared {
    wheel: Mutex<TimerWheel<TaskPtr>>,
    shutdown: AtomicBool,
    start_time: Instant,
}

impl TimerWheelShared {
    fn new() -> Arc<Self> {
        let start_time = Instant::now();
        Arc::new(Self {
            // 1ms tick resolution, 512 ticks = 512ms wheel coverage
            wheel: Mutex::new(TimerWheel::new(Duration::from_millis(1), 512, 0)),
            shutdown: AtomicBool::new(false),
            start_time,
        })
    }

    fn schedule_timer(&self, deadline: Instant) -> Option<u64> {
        let deadline_ns = deadline.duration_since(self.start_time).as_nanos() as u64;
        let mut wheel = self.wheel.lock().unwrap();
        wheel.schedule_timer(deadline_ns, TaskPtr(std::ptr::null_mut()))
    }

    fn set_task(&self, timer_id: u64, task: TaskPtr) {
        let mut wheel = self.wheel.lock().unwrap();
        // Cancel and re-schedule with task ptr
        if let Some(_) = wheel.cancel_timer(timer_id) {
            // Re-insert with task
            let deadline_ns = (timer_id >> 32) as u64; // Approximate from timer_id
            wheel.schedule_timer(deadline_ns, task);
        }
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    fn run_timer_thread(self: Arc<Self>) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            while !self.shutdown.load(Ordering::Relaxed) {
                let now = Instant::now();
                let now_ns = now.duration_since(self.start_time).as_nanos() as u64;

                let expired = {
                    let mut wheel = self.wheel.lock().unwrap();
                    wheel.advance_to(now_ns);
                    wheel.poll(now_ns, 1000) // Process up to 1000 timers per iteration
                };

                // Wake all expired tasks
                for (_timer_id, _deadline_ns, task) in expired {
                    if !task.get().is_null() {
                        unsafe {
                            (*task.get()).schedule();
                        }
                    }
                }

                // Sleep briefly to avoid busy-waiting
                thread::sleep(Duration::from_micros(100));
            }
        })
    }
}

/// Global timer wheel instance
static TIMER_WHEEL: std::sync::OnceLock<Arc<TimerWheelShared>> = std::sync::OnceLock::new();

fn get_timer_wheel() -> &'static Arc<TimerWheelShared> {
    TIMER_WHEEL.get_or_init(|| TimerWheelShared::new())
}

/// Efficient sleep future that registers with the timer wheel
/// No busy-waiting - task is parked until the deadline
struct SleepTimer {
    deadline: Instant,
    timer_id: Option<u64>,
}

impl SleepTimer {
    #[inline(always)]
    fn new(duration: Duration) -> Self {
        Self {
            deadline: Instant::now() + duration,
            timer_id: None,
        }
    }

    #[inline(always)]
    fn until(deadline: Instant) -> Self {
        Self {
            deadline,
            timer_id: None,
        }
    }
}

impl Future for SleepTimer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let now = Instant::now();

        if now >= self.deadline {
            Poll::Ready(())
        } else if self.timer_id.is_none() {
            // First poll - register with timer wheel and park
            let timer_wheel = get_timer_wheel();

            if let Some(timer_id) = timer_wheel.schedule_timer(self.deadline) {
                // Extract TaskPtr from the waker using unsafe cast
                // Waker contains a RawWaker with TaskPtr as data pointer
                let task_ptr = unsafe {
                    let waker_ref = cx.waker();
                    // Cast &Waker to access internal data pointer
                    // Waker's first field is the data pointer from RawWaker
                    let ptr_to_data = waker_ref as *const Waker as *const *const ();
                    TaskPtr(*ptr_to_data as *mut TaskInner)
                };

                timer_wheel.set_task(timer_id, task_ptr);
                self.timer_id = Some(timer_id);
            }

            Poll::Pending
        } else {
            // Already registered - we were woken up, check if deadline passed
            if now >= self.deadline {
                Poll::Ready(())
            } else {
                // Spurious wakeup - stay pending
                Poll::Pending
            }
        }
    }
}

/// Simple busy-wait sleep (for comparison)
struct SleepBusy {
    deadline: Instant,
    started: bool,
}

impl SleepBusy {
    #[inline(always)]
    fn new(duration: Duration) -> Self {
        Self {
            deadline: Instant::now() + duration,
            started: false,
        }
    }
}

impl Future for SleepBusy {
    type Output = ();

    #[inline(always)]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let now = Instant::now();

        if now >= self.deadline {
            Poll::Ready(())
        } else {
            if !self.started {
                self.started = true;
            }
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

thread_local! {
    static PRODUCER: UnsafeCell<*mut bop_mpmc::Producer<TaskPtr, 8, 12>> = UnsafeCell::new(std::ptr::null_mut());
}

fn get_producer(executor: &ExecutorInner) -> *mut bop_mpmc::Producer<TaskPtr, 8, 12> {
    PRODUCER.with(|producer_entry| {
        let producer = unsafe { *producer_entry.get() };
        if producer.is_null() {
            let p = Box::new(executor.queue.create_producer_handle().unwrap());
            let p =
                unsafe { Box::into_raw(p) as *mut _ as *mut bop_mpmc::Producer<TaskPtr, 8, 12> };
            unsafe {
                *producer_entry.get() = p;
            }
            p
        } else {
            producer
        }
    })
}

/// Task using raw pointers - no Arc overhead
struct TaskInner {
    future: UnsafeCell<Pin<Box<dyn Future<Output = ()> + Send>>>,
    executor: *const ExecutorInner,
    flags: AtomicU8,
    yielded: UnsafeCell<bool>,
}

impl TaskInner {
    pub fn schedule(&self) -> bool {
        let flags = self.flags.load(Ordering::Relaxed);
        if flags & SCHEDULED != IDLE {
            return false;
        }
        if flags & EXECUTING != IDLE {
            let previous_flags = self.flags.fetch_or(SCHEDULED, Ordering::Release);
            if previous_flags & EXECUTING == IDLE {
                return false;
            }
            let scheduled_nor_executing = (previous_flags & (SCHEDULED | EXECUTING)) == IDLE;
            if scheduled_nor_executing {
                let producer = unsafe { &*get_producer(unsafe { &*self.executor }) };
                loop {
                    match producer.try_push(TaskPtr(self as *const _ as *mut TaskInner)) {
                        Ok(_) => return true,
                        Err(PushError::Closed(_)) => return false,
                        Err(PushError::Full(_)) => std::hint::spin_loop(),
                    }
                }
            } else {
                return false;
            }
        }

        let previous_flags = self.flags.fetch_or(SCHEDULED, Ordering::Release);
        let scheduled_nor_executing = (previous_flags & (SCHEDULED | EXECUTING)) == IDLE;

        if scheduled_nor_executing {
            let producer = unsafe { &*get_producer(unsafe { &*self.executor }) };
            loop {
                match producer.try_push(TaskPtr(self as *const _ as *mut TaskInner)) {
                    Ok(_) => return true,
                    Err(PushError::Closed(_)) => return false,
                    Err(PushError::Full(_)) => std::hint::spin_loop(),
                }
            }
        } else {
            false
        }
    }
}

unsafe impl Send for TaskInner {}
unsafe impl Sync for TaskInner {}

impl TaskInner {
    fn new(
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
        executor: *const ExecutorInner,
    ) -> *mut Self {
        Box::into_raw(Box::new(Self {
            future: UnsafeCell::new(future),
            executor,
            flags: AtomicU8::new(SCHEDULED),
            yielded: UnsafeCell::new(false),
        }))
    }

    unsafe fn poll(task_ptr: *mut Self) -> Poll<()> {
        let task = &*task_ptr;
        *task.yielded.get() = false;
        let future = &mut *task.future.get();
        let waker = {
            let raw_waker = RawWaker::new(task_ptr as *const (), &VTABLE_YIELD);
            Waker::from_raw(raw_waker)
        };
        let mut cx = Context::from_waker(&waker);
        future.as_mut().poll(&mut cx)
    }

    unsafe fn drop_task(_task_ptr: *mut Self) {
        // No-op for now
    }
}

static VTABLE_YIELD: RawWakerVTable = RawWakerVTable::new(
    |data| RawWaker::new(data, &VTABLE),
    |data| unsafe {
        let task_ptr = data as *mut TaskInner;
        *(*task_ptr).yielded.get() = true;
    },
    |data| unsafe {
        let task_ptr = data as *mut TaskInner;
        *(*task_ptr).yielded.get() = true;
    },
    |_data| {},
);

static VTABLE: RawWakerVTable = RawWakerVTable::new(
    |data| RawWaker::new(data, &VTABLE),
    |data| unsafe {
        let task_ptr = data as *mut TaskInner;
        (*task_ptr).schedule();
    },
    |data| unsafe {
        let task_ptr = data as *mut TaskInner;
        (*task_ptr).schedule();
    },
    |_data| {},
);

/// Inner state - NOW WITH WORK-STEALING
struct ExecutorInner {
    queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    tasks: dashmap::DashMap<usize, TaskPtr>,
}

impl ExecutorInner {
    fn new() -> Self {
        Self {
            queue: Arc::new(Mpmc::<TaskPtr, 8, 12>::new()),
            tasks: dashmap::DashMap::new(),
        }
    }
}

struct Executor {
    inner: Arc<ExecutorInner>,
    worker_handles: Vec<thread::JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    stealers: Arc<Vec<Stealer<TaskPtr>>>, // NEW: For work-stealing
    stats: Arc<Vec<WorkerStats>>,         // NEW: Per-worker statistics
    batches: Arc<Vec<StealableBatch>>,    // NEW: Per-worker batches for stealing
    timer_handle: Option<thread::JoinHandle<()>>, // Timer thread handle
}

impl Executor {
    fn new(num_threads: usize) -> Self {
        let inner = Arc::new(ExecutorInner::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        // NEW: Create local workers and stealers
        let local_workers: Vec<Worker<TaskPtr>> =
            (0..num_threads).map(|_| Worker::new_lifo()).collect();
        let stealers: Vec<Stealer<TaskPtr>> = local_workers.iter().map(|w| w.stealer()).collect();
        let stealers = Arc::new(stealers);

        // NEW: Create per-worker statistics (num_threads + 1 for steal worker)
        let stats: Arc<Vec<WorkerStats>> =
            Arc::new((0..num_threads + 1).map(|_| WorkerStats::new()).collect());

        // NEW: Create per-worker stealable batches (only for regular workers, not steal worker)
        let batches: Arc<Vec<StealableBatch>> =
            Arc::new((0..num_threads).map(|_| StealableBatch::new()).collect());

        let core_ids = core_affinity::get_core_ids().unwrap();

        let mut worker_handles: Vec<_> = local_workers
            .into_iter()
            .enumerate()
            .zip(
                core_ids
                    .clone()
                    .into_iter()
                    .filter(|id| id.lt(&core_affinity::CoreId { id: 8 })),
            )
            .map(|((worker_id, local_worker), core_id)| {
                println!("Worker {} on core {:?}", worker_id, core_id);
                let inner_clone = Arc::clone(&inner);
                let shutdown_clone = Arc::clone(&shutdown);
                let stealers_clone = Arc::clone(&stealers);
                let stats_clone = Arc::clone(&stats);
                let batches_clone = Arc::clone(&batches);

                thread::spawn(move || {
                    core_affinity::set_for_current(core_id);
                    Self::worker_loop(
                        worker_id,
                        local_worker,
                        stealers_clone,
                        inner_clone,
                        shutdown_clone,
                        stats_clone,
                        batches_clone,
                    );
                })
            })
            .collect();

        // Add specialized steal-only worker
        let steal_worker_id = num_threads;
        if let Some(steal_core_id) = core_ids
            .into_iter()
            .filter(|id| id.gt(&core_affinity::CoreId { id: 8 }))
            .nth(1)
        {
            // println!(
            //     "Steal Worker {} on core {:?}",
            //     steal_worker_id, steal_core_id
            // );
            // let inner_clone = Arc::clone(&inner);
            // let shutdown_clone = Arc::clone(&shutdown);
            // let stealers_clone = Arc::clone(&stealers);
            // let stats_clone = Arc::clone(&stats);
            // let batches_clone = Arc::clone(&batches);

            // worker_handles.push(thread::spawn(move || {
            //     core_affinity::set_for_current(steal_core_id);
            //     Self::steal_worker_loop(
            //         steal_worker_id,
            //         stealers_clone,
            //         inner_clone,
            //         shutdown_clone,
            //         stats_clone,
            //         batches_clone,
            //     );
            // }));
        }

        // Start timer thread
        let timer_wheel = get_timer_wheel().clone();
        let timer_handle = Some(timer_wheel.run_timer_thread());

        Self {
            inner,
            worker_handles,
            shutdown,
            stealers,
            stats,
            batches,
            timer_handle,
        }
    }

    /// Specialized worker that ONLY steals (no global queue access)
    fn steal_worker_loop(
        worker_id: usize,
        stealers: Arc<Vec<Stealer<TaskPtr>>>, // Steal from other workers' local queues
        inner: Arc<ExecutorInner>,
        shutdown: Arc<AtomicBool>,
        stats: Arc<Vec<WorkerStats>>,
        batches: Arc<Vec<StealableBatch>>, // Steal from other workers' batches
    ) {
        let worker_stats = &stats[worker_id];
        let executor_inner = Arc::into_raw(Arc::clone(&inner));
        let executor_inner = unsafe { &*executor_inner };
        let mut enqueue = Vec::<TaskPtr>::with_capacity(1024);
        let producer = unsafe { &*get_producer(executor_inner) };
        let mut stolen_batch: [TaskPtr; 256] =
            std::array::from_fn(|_| TaskPtr(std::ptr::null_mut()));
        let mut stolen_count = 0usize;

        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            // Strategy 1: Try bulk stealing from other workers' batches (50% of remaining)
            if stolen_count == 0 {
                for (i, other_batch) in batches.iter().enumerate() {
                    if i == worker_id {
                        continue;
                    }

                    let remaining = other_batch.remaining_count();
                    if remaining > 10 {
                        // Only bulk steal if significant work available
                        worker_stats
                            .batch_steal_attempts
                            .fetch_add(1, Ordering::Relaxed);

                        stolen_count = other_batch.steal_bulk(&mut stolen_batch, 0.5); // Steal 50%

                        if stolen_count > 0 {
                            worker_stats
                                .batch_steal_successes
                                .fetch_add(stolen_count as u64, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }

            // Strategy 2: If bulk steal failed, try single-task batch stealing
            if stolen_count == 0 {
                for (i, other_batch) in batches.iter().enumerate() {
                    if i == worker_id {
                        continue;
                    }

                    worker_stats
                        .batch_steal_attempts
                        .fetch_add(1, Ordering::Relaxed);
                    if let Some(stolen) = other_batch.steal_back() {
                        stolen_batch[0] = stolen;
                        stolen_count = 1;
                        worker_stats
                            .batch_steal_successes
                            .fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                }
            }

            // Strategy 3: Try stealing from other workers' local queues
            if stolen_count == 0 {
                for (i, stealer) in stealers.iter().enumerate() {
                    if i == worker_id {
                        continue;
                    }

                    worker_stats.steal_attempts.fetch_add(1, Ordering::Relaxed);
                    match stealer.steal() {
                        Steal::Success(stolen) => {
                            stolen_batch[0] = stolen;
                            stolen_count = 1;
                            worker_stats.steal_successes.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                        Steal::Empty => continue,
                        Steal::Retry => continue,
                    }
                }
            }

            // No work found, spin briefly
            if stolen_count == 0 {
                std::hint::spin_loop();
                continue;
            }

            // Process all stolen tasks
            for i in 0..stolen_count {
                let task = stolen_batch[i];
                worker_stats.tasks_processed.fetch_add(1, Ordering::Relaxed);

                unsafe {
                    (*task.get()).flags.store(EXECUTING, Ordering::Release);
                    let result = TaskInner::poll(task.get());
                    match result {
                        Poll::Ready(()) => {
                            let task_key = task.get() as *mut _ as usize;
                            inner.tasks.remove(&task_key);
                            TaskInner::drop_task(task.get());
                        }
                        Poll::Pending => {
                            if *(*task.get()).yielded.get() {
                                (*task.get()).flags.store(SCHEDULED, Ordering::Release);
                                enqueue.push(task);
                            } else {
                                let after_flags =
                                    (*task.get()).flags.fetch_sub(EXECUTING, Ordering::AcqRel);
                                if after_flags & SCHEDULED != IDLE {
                                    enqueue.push(task);
                                }
                            }
                        }
                    }
                }
            }

            // Reset stolen count for next iteration
            stolen_count = 0;

            // RE-ENQUEUE: Push to global queue with retry limit
            if !enqueue.is_empty() {
                let mut remaining = 0;
                let mut retry_count = 0;
                const MAX_RETRIES: usize = 64;

                while remaining < enqueue.len() {
                    match producer.try_push_n(&mut enqueue[remaining..]) {
                        Ok(pushed) => {
                            remaining += pushed;
                            retry_count = 0;
                        }
                        Err(_) => {
                            retry_count += 1;
                            if retry_count >= MAX_RETRIES {
                                // Give up, drop tasks (or could store for later)
                                break;
                            }
                            if retry_count & 15 == 0 {
                                std::hint::spin_loop();
                            }
                        }
                    }
                }
                enqueue.clear();
            }
        }
    }

    /// Worker loop with work-stealing in SLOW PATH ONLY
    fn worker_loop(
        worker_id: usize,
        local_worker: Worker<TaskPtr>,        // NEW
        stealers: Arc<Vec<Stealer<TaskPtr>>>, // NEW
        inner: Arc<ExecutorInner>,
        shutdown: Arc<AtomicBool>,
        stats: Arc<Vec<WorkerStats>>,      // NEW: Statistics
        batches: Arc<Vec<StealableBatch>>, // NEW: Stealable batches
    ) {
        let worker_stats = &stats[worker_id];
        let my_batch = &batches[worker_id];
        let mut selector = Selector::new();
        let mut temp_batch: [TaskPtr; 512] = std::array::from_fn(|_| TaskPtr(std::ptr::null_mut()));
        let executor_inner = Arc::into_raw(Arc::clone(&inner));
        let executor_inner = unsafe { &*executor_inner };
        let mut enqueue = Vec::<TaskPtr>::with_capacity(1024);
        let producer = unsafe { &*get_producer(executor_inner) };
        let mut count = 0;
        let mut next_target = 5000000;

        let yield_enqueue =
            |task_ptr: TaskPtr, local_worker: &Worker<TaskPtr>| match producer.try_push(task_ptr) {
                Ok(_) => true,
                Err(_) => {
                    local_worker.push(task_ptr);
                    false
                }
            };

        loop {
            // FAST PATH: Try global queue first (COMPLETELY UNCHANGED)
            let mut size = inner
                .queue
                .try_pop_n_with_selector(&mut selector, &mut temp_batch);

            let mut local_count = 0;
            let mut steal_count = 0;
            let mut batch_steal_count = 0;

            // SLOW PATH: Only runs when global queue is empty
            if size == 0 {
                // NEW: Try local queue
                while size < 512 {
                    match local_worker.pop() {
                        Some(task) => {
                            temp_batch[size] = task;
                            size += 1;
                            local_count += 1;
                        }
                        None => break,
                    }
                }

                // NEW: If local empty, try stealing from other workers' local queues
                if size == 0 {
                    for (i, stealer) in stealers.iter().enumerate() {
                        if i == worker_id {
                            continue;
                        }

                        worker_stats.steal_attempts.fetch_add(1, Ordering::Relaxed);
                        match stealer.steal_batch_and_pop(&local_worker) {
                            Steal::Success(task) => {
                                temp_batch[0] = task;
                                size = 1;
                                steal_count = 1;
                                worker_stats.steal_successes.fetch_add(1, Ordering::Relaxed);
                                break;
                            }
                            Steal::Empty => continue,
                            Steal::Retry => continue,
                        }
                    }
                }

                // NEW: If still empty, try stealing from other workers' batches
                if size == 0 {
                    // std::hint::spin_loop();
                    size = inner
                        .queue
                        .try_pop_n_with_selector(&mut selector, &mut temp_batch);

                    if size == 0 {
                        for (i, other_batch) in batches.iter().enumerate() {
                            if i == worker_id {
                                continue;
                            }

                            worker_stats
                                .batch_steal_attempts
                                .fetch_add(1, Ordering::Relaxed);
                            if let Some(task) = other_batch.steal_back() {
                                temp_batch[0] = task;
                                size = 1;
                                batch_steal_count = 1;
                                worker_stats
                                    .batch_steal_successes
                                    .fetch_add(1, Ordering::Relaxed);
                                break;
                            }
                        }
                    }
                }

                // Still nothing? Spin (same as before)
                if size == 0 {
                    std::hint::spin_loop();
                    continue;
                }
            } else {
                // Fast path hit
                worker_stats
                    .fast_path_hits
                    .fetch_add(size as u64, Ordering::Relaxed);
            }

            // Track local queue hits
            if local_count > 0 {
                worker_stats
                    .local_queue_hits
                    .fetch_add(local_count, Ordering::Relaxed);
            }

            // Fill my_batch from temp_batch for work-stealing capability
            my_batch.fill_from_slice(&temp_batch, size);

            count += size;
            worker_stats
                .tasks_processed
                .fetch_add(size as u64, Ordering::Relaxed);

            // PROCESS BATCH: Now using StealableBatch (allows stealing during processing)
            while let Some(task_ptr) = my_batch.pop_front() {
                unsafe {
                    (*task_ptr.get()).flags.store(EXECUTING, Ordering::Release);
                    let result = TaskInner::poll(task_ptr.get());
                    match result {
                        Poll::Ready(()) => {
                            let task_key = task_ptr.get() as *mut _ as usize;
                            inner.tasks.remove(&task_key);
                            TaskInner::drop_task(task_ptr.get());
                        }
                        Poll::Pending => {
                            if *(*task_ptr.get()).yielded.get() {
                                (*task_ptr.get()).flags.store(SCHEDULED, Ordering::Release);
                                enqueue.push(task_ptr);
                            } else {
                                let after_flags = (*task_ptr.get())
                                    .flags
                                    .fetch_sub(EXECUTING, Ordering::AcqRel);
                                if after_flags & SCHEDULED != IDLE {
                                    enqueue.push(task_ptr);
                                }
                            }
                        }
                    }
                }
            }

            // for i in 0..size {
            //     let task_ptr = temp_batch[i];

            //     unsafe {
            //         (*task_ptr.get()).flags.store(EXECUTING, Ordering::Release);
            //         let result = TaskInner::poll(task_ptr.get());
            //         match result {
            //             Poll::Ready(()) => {
            //                 let task_key = task_ptr.get() as *mut _ as usize;
            //                 inner.tasks.remove(&task_key);
            //                 TaskInner::drop_task(task_ptr.get());
            //             }
            //             Poll::Pending => {
            //                 if *(*task_ptr.get()).yielded.get() {
            //                     (*task_ptr.get()).flags.store(SCHEDULED, Ordering::Release);
            //                     enqueue.push(task_ptr);
            //                 } else {
            //                     let after_flags = (*task_ptr.get())
            //                         .flags
            //                         .fetch_sub(EXECUTING, Ordering::AcqRel);
            //                     if after_flags & SCHEDULED != IDLE {
            //                         enqueue.push(task_ptr);
            //                     }
            //                 }
            //             }
            //         }
            //     }
            // }

            // RE-ENQUEUE: Try global first, fallback to local if spinning too long
            if !enqueue.is_empty() {
                let mut remaining = 0;
                let mut retry_count = 0;
                const MAX_RETRIES: usize = 64; // Limit spins before falling back to local

                // Try to push to global queue with retry limit
                while remaining < enqueue.len() {
                    match producer.try_push_n(&mut enqueue[remaining..]) {
                        Ok(pushed) => {
                            remaining += pushed;
                            retry_count = 0; // Reset on success
                        }
                        Err(_) => {
                            retry_count += 1;

                            if retry_count >= MAX_RETRIES {
                                // Spinning too long, push remaining to local_worker
                                for i in remaining..enqueue.len() {
                                    local_worker.push(enqueue[i]);
                                }
                                remaining = enqueue.len();
                                break;
                            }

                            if retry_count & 31 == 0 {
                                std::hint::spin_loop();
                            }
                        }
                    }
                }
                enqueue.clear();
            }

            if count >= next_target {
                next_target += 5000000;
            }
        }
    }

    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let executor_inner = Arc::into_raw(Arc::clone(&self.inner));
        let executor_inner = unsafe { &*executor_inner };
        let producer = unsafe { &*get_producer(executor_inner) };
        let executor_ptr = Arc::as_ptr(&self.inner);
        let task_ptr = TaskInner::new(Box::pin(future), executor_ptr);
        let task_ptr_wrapper = TaskPtr::new(task_ptr);
        self.inner
            .tasks
            .insert(task_ptr as usize, TaskPtr::new(task_ptr));

        loop {
            match producer.try_push(task_ptr_wrapper) {
                Ok(()) => return,
                Err(_) => std::hint::spin_loop(),
            }
        }
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        get_timer_wheel().shutdown();
        eprintln!("waiting for all workers to stop");
        while !self.inner.queue.close() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Clone for Executor {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            worker_handles: Vec::new(),
            shutdown: Arc::clone(&self.shutdown),
            stealers: Arc::clone(&self.stealers),
            stats: Arc::clone(&self.stats),
            batches: Arc::clone(&self.batches),
            timer_handle: None, // Clones don't own the timer thread
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 && !self.worker_handles.is_empty() {
            self.shutdown();
        }
    }
}

fn main() {
    println!("🎯 Minimal Work-Stealing Executor Benchmark");
    println!("============================================\n");

    // Example: Timer-based sleep demo
    {
        println!("=== Demo: Timer-based sleep (efficient, no busy-wait) ===");
        let executor = Arc::new(Executor::new(4));
        let completed = Arc::new(AtomicUsize::new(0));

        // Spawn tasks with different sleep durations
        for i in 0..10 {
            let completed = Arc::clone(&completed);
            let sleep_ms = (i + 1) * 100;
            executor.spawn(async move {
                println!("Task {} starting, will sleep {}ms", i, sleep_ms);
                SleepTimer::new(Duration::from_millis(sleep_ms)).await;
                println!("Task {} woke up after {}ms", i, sleep_ms);
                completed.fetch_add(1, Ordering::Relaxed);
            });
        }

        // Wait for all tasks to complete
        while completed.load(Ordering::Relaxed) < 10 {
            thread::sleep(Duration::from_millis(10));
        }
        println!("All sleep tasks completed!\n");
        executor.shutdown();
    }

    {
        println!("=== Test: 8 threads - 200K tasks × 100K yields (Zero-Cost Yield) ===");
        let tasks_count = 200_000;
        let task_counters: Arc<Vec<_>> = Arc::new(
            (0..tasks_count)
                .map(|_| Arc::new(AtomicUsize::new(0)))
                .collect(),
        );
        let executor = Arc::new(Executor::new(8));

        let start_threads: Vec<_> = (0..1)
            .map(|_i| {
                let task_counters = Arc::clone(&task_counters);
                let executor = Arc::clone(&executor);
                std::thread::spawn(move || {
                    let bad_actors_10ms = 0;
                    for task_id in 0..bad_actors_10ms {
                        executor.spawn(BadNTimes::new(
                            100_000,
                            Duration::from_millis(10),
                            task_counters[task_id].clone(),
                        ));
                    }
                    let bad_actors = bad_actors_10ms;
                    for task_id in bad_actors..tasks_count {
                        // Option 1: Use async function (cleaner, same performance)
                        executor.spawn(yield_n_times(100_000, task_counters[task_id].clone()));

                        // Option 2: Direct async block (inline version)
                        // let counter = task_counters[task_id].clone();
                        // executor.spawn(async move {
                        //     for _ in 0..100_000 {
                        //         counter.fetch_add(1, Ordering::Relaxed);
                        //         Yield::new().await;
                        //     }
                        // });

                        // Option 3: Manual Future implementation (original)
                        // executor.spawn(YieldNTimes::new(100_000, task_counters[task_id].clone()));
                    }
                })
            })
            .collect();

        for h in start_threads {
            h.join().unwrap();
        }
        println!("All tasks spawned");

        let mut starting_count = 0;
        for counter in task_counters.iter() {
            starting_count += counter.load(Ordering::Relaxed);
        }
        let start = Instant::now();

        let mut last_count = 0;
        let mut current_window = 0;
        let mut next_target = start.add(Duration::from_secs(1));
        let mut current_start = start;

        loop {
            thread::sleep(Duration::from_millis(1));

            let mut current = 0;
            for counter in task_counters.iter() {
                current += counter.load(Ordering::Relaxed);
            }

            if current >= 10_000_000_000 {
                break;
            }

            if current == last_count {
                // No progress check could go here
            }
            last_count = current;

            let now = Instant::now();
            if now.ge(&next_target) {
                // Collect statistics from all workers (separate regular vs steal worker)
                let mut total_fast_path = 0u64;
                let mut total_local = 0u64;
                let mut total_steal_attempts = 0u64;
                let mut total_steal_successes = 0u64;
                let mut total_batch_steal_attempts = 0u64;
                let mut total_batch_steal_successes = 0u64;
                let mut total_tasks = 0u64;

                // Steal worker stats (last worker)
                let steal_worker_idx = executor.stats.len() - 1;
                let steal_worker_stats = &executor.stats[steal_worker_idx];
                let sw_steal_attempts = steal_worker_stats.steal_attempts.load(Ordering::Relaxed);
                let sw_steal_successes = steal_worker_stats.steal_successes.load(Ordering::Relaxed);
                let sw_batch_steal_attempts = steal_worker_stats
                    .batch_steal_attempts
                    .load(Ordering::Relaxed);
                let sw_batch_steal_successes = steal_worker_stats
                    .batch_steal_successes
                    .load(Ordering::Relaxed);
                let sw_tasks = steal_worker_stats.tasks_processed.load(Ordering::Relaxed);

                // Regular workers stats
                for worker_stats in executor.stats.iter().take(steal_worker_idx) {
                    total_fast_path += worker_stats.fast_path_hits.load(Ordering::Relaxed);
                    total_local += worker_stats.local_queue_hits.load(Ordering::Relaxed);
                    total_steal_attempts += worker_stats.steal_attempts.load(Ordering::Relaxed);
                    total_steal_successes += worker_stats.steal_successes.load(Ordering::Relaxed);
                    total_batch_steal_attempts +=
                        worker_stats.batch_steal_attempts.load(Ordering::Relaxed);
                    total_batch_steal_successes +=
                        worker_stats.batch_steal_successes.load(Ordering::Relaxed);
                    total_tasks += worker_stats.tasks_processed.load(Ordering::Relaxed);
                }

                // Add steal worker to totals
                total_steal_attempts += sw_steal_attempts;
                total_steal_successes += sw_steal_successes;
                total_batch_steal_attempts += sw_batch_steal_attempts;
                total_batch_steal_successes += sw_batch_steal_successes;
                total_tasks += sw_tasks;

                let total_work = total_fast_path
                    + total_local
                    + total_steal_successes
                    + total_batch_steal_successes;
                let fast_path_pct = if total_work > 0 {
                    (total_fast_path as f64 / total_work as f64) * 100.0
                } else {
                    0.0
                };
                let local_pct = if total_work > 0 {
                    (total_local as f64 / total_work as f64) * 100.0
                } else {
                    0.0
                };
                let steal_pct = if total_work > 0 {
                    (total_steal_successes as f64 / total_work as f64) * 100.0
                } else {
                    0.0
                };
                let batch_steal_pct = if total_work > 0 {
                    (total_batch_steal_successes as f64 / total_work as f64) * 100.0
                } else {
                    0.0
                };
                let steal_success_rate = if total_steal_attempts > 0 {
                    (total_steal_successes as f64 / total_steal_attempts as f64) * 100.0
                } else {
                    0.0
                };
                let batch_steal_success_rate = if total_batch_steal_attempts > 0 {
                    (total_batch_steal_successes as f64 / total_batch_steal_attempts as f64) * 100.0
                } else {
                    0.0
                };

                let sw_contribution_pct = if total_tasks > 0 {
                    (sw_tasks as f64 / total_tasks as f64) * 100.0
                } else {
                    0.0
                };

                println!(
                    "  {:.2}M p/s | Fast: {:.1}% | Local: {:.1}% | QSteal: {:.1}%({}/{}) | BSteal: {:.1}%({}/{}) | StealWorker: {:.1}%",
                    (current - current_window) as f64
                        / (now.sub(current_start)).as_secs_f64()
                        / 1_000_000.0,
                    fast_path_pct,
                    local_pct,
                    steal_pct,
                    total_steal_successes,
                    total_steal_attempts,
                    batch_steal_pct,
                    total_batch_steal_successes,
                    total_batch_steal_attempts,
                    sw_contribution_pct
                );
                next_target = now.add(Duration::from_secs(1));
                current_start = now;
                current_window = current;
            }
        }

        let duration = start.elapsed();
        let mut polls = 0;
        for counter in task_counters.iter() {
            polls += counter.load(Ordering::Relaxed);
        }
        polls -= starting_count;

        println!("  Threads: 8");
        println!("  Tasks: {}", tasks_count);
        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );

        executor.shutdown();
    }

    println!("\n=== Summary ===");
    println!("Work-stealing added to slow path only:");
    println!("  • Fast path: Global MPMC queue (unchanged)");
    println!("  • Slow path: Local queue → Steal → Spin");
    println!("  • Re-enqueue: 64 local, rest global");
}
