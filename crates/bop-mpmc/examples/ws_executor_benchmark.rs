#![feature(thread_id_value)]
//! Minimal Work-Stealing Executor Benchmark
//!
//! IDENTICAL to minimal_spsc_executor_benchmark.rs except:
//! - Work-stealing added to SLOW PATH only (when global queue empty)
//! - Fast path (global queue has work) is COMPLETELY UNCHANGED

use bop_mpmc::selector::Selector;
use bop_mpmc::{EXECUTING, IDLE, Mpmc, PushError, SCHEDULED};
use crossbeam_deque::{Steal, Stealer, Worker};
use futures_lite::FutureExt;
use std::cell::UnsafeCell;
use std::future::Future;
use std::mem::MaybeUninit;
use std::ops::{Add, Sub};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
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
    fast_path_hits: AtomicU64,   // Tasks from global MPMC queue
    local_queue_hits: AtomicU64, // Tasks from local queue
    steal_attempts: AtomicU64,   // Number of steal attempts
    steal_successes: AtomicU64,  // Successful steals
    tasks_processed: AtomicU64,  // Total tasks processed
}

impl WorkerStats {
    fn new() -> Self {
        Self {
            fast_path_hits: AtomicU64::new(0),
            local_queue_hits: AtomicU64::new(0),
            steal_attempts: AtomicU64::new(0),
            steal_successes: AtomicU64::new(0),
            tasks_processed: AtomicU64::new(0),
        }
    }
}

/// Minimal future that yields N times
struct BadNTimes {
    remaining: usize,
    poll_count: Arc<AtomicUsize>,
}

unsafe impl Send for BadNTimes {}

impl BadNTimes {
    fn new(count: usize, poll_count: Arc<AtomicUsize>) -> Self {
        Self {
            remaining: count,
            poll_count,
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
            std::thread::sleep(Duration::from_millis(500));
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

        // NEW: Create per-worker statistics
        let stats: Arc<Vec<WorkerStats>> =
            Arc::new((0..num_threads).map(|_| WorkerStats::new()).collect());

        let core_ids = core_affinity::get_core_ids().unwrap();

        let worker_handles = local_workers
            .into_iter()
            .enumerate()
            .zip(
                core_ids
                    .into_iter()
                    .filter(|id| id.lt(&core_affinity::CoreId { id: 8 })),
            )
            .map(|((worker_id, local_worker), core_id)| {
                println!("Worker {} on core {:?}", worker_id, core_id);
                let inner_clone = Arc::clone(&inner);
                let shutdown_clone = Arc::clone(&shutdown);
                let stealers_clone = Arc::clone(&stealers);
                let stats_clone = Arc::clone(&stats);

                thread::spawn(move || {
                    core_affinity::set_for_current(core_id);
                    Self::worker_loop(
                        worker_id,
                        local_worker,
                        stealers_clone,
                        inner_clone,
                        shutdown_clone,
                        stats_clone,
                    );
                })
            })
            .collect::<Vec<_>>();

        Self {
            inner,
            worker_handles,
            shutdown,
            stealers,
            stats,
        }
    }

    /// Worker loop with work-stealing in SLOW PATH ONLY
    fn worker_loop(
        worker_id: usize,
        local_worker: Worker<TaskPtr>,        // NEW
        stealers: Arc<Vec<Stealer<TaskPtr>>>, // NEW
        inner: Arc<ExecutorInner>,
        shutdown: Arc<AtomicBool>,
        stats: Arc<Vec<WorkerStats>>, // NEW: Statistics
    ) {
        let worker_stats = &stats[worker_id];
        let mut selector = Selector::new();
        let mut batch: [TaskPtr; 512] = std::array::from_fn(|_| TaskPtr(std::ptr::null_mut()));
        let executor_inner = Arc::into_raw(Arc::clone(&inner));
        let executor_inner = unsafe { &*executor_inner };
        let mut enqueue = Vec::<TaskPtr>::with_capacity(1024);
        let producer = unsafe { &*get_producer(executor_inner) };
        let mut count = 0;
        let mut next_target = 5000000;

        loop {
            // FAST PATH: Try global queue first (COMPLETELY UNCHANGED)
            let mut size = inner
                .queue
                .try_pop_n_with_selector(&mut selector, &mut batch);

            let mut local_count = 0;
            let mut steal_count = 0;

            // SLOW PATH: Only runs when global queue is empty
            if size == 0 {
                // NEW: Try local queue
                while size < 512 {
                    match local_worker.pop() {
                        Some(task) => {
                            batch[size] = task;
                            size += 1;
                            local_count += 1;
                        }
                        None => break,
                    }
                }

                // NEW: If local empty, try stealing
                if size == 0 {
                    for (i, stealer) in stealers.iter().enumerate() {
                        if i == worker_id {
                            continue;
                        }

                        worker_stats.steal_attempts.fetch_add(1, Ordering::Relaxed);
                        match stealer.steal_batch_and_pop(&local_worker) {
                            Steal::Success(task) => {
                                batch[0] = task;
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

            count += size;
            worker_stats
                .tasks_processed
                .fetch_add(size as u64, Ordering::Relaxed);

            // PROCESS BATCH: COMPLETELY UNCHANGED
            for i in 0..size {
                let task_ptr = batch[i];
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

            // RE-ENQUEUE: Modified to use local queue for small amounts
            if !enqueue.is_empty() {
                let local_count = enqueue.len().min(64);

                // Push small portion to local queue
                for i in 0..local_count {
                    local_worker.push(enqueue[i]);
                }

                // Rest goes to global (same logic as before)
                if enqueue.len() > local_count {
                    let mut remaining = local_count;
                    let mut retry_count = 0;
                    while remaining < enqueue.len() {
                        match producer.try_push_n(&mut enqueue[remaining..]) {
                            Ok(pushed) => {
                                remaining += pushed;
                            }
                            Err(_) => {
                                retry_count += 1;
                                if retry_count & 31 == 0 {
                                    std::hint::spin_loop();
                                }
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

    {
        println!("=== Test: 8 threads - 200K tasks × 100K yields ===");
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
                    for task_id in 0..tasks_count {
                        executor.spawn(YieldNTimes::new(100_000, task_counters[task_id].clone()));
                    }
                    executor.spawn(BadNTimes::new(100_000, task_counters[0].clone()));
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
                // Collect statistics from all workers
                let mut total_fast_path = 0u64;
                let mut total_local = 0u64;
                let mut total_steal_attempts = 0u64;
                let mut total_steal_successes = 0u64;
                let mut total_tasks = 0u64;

                for worker_stats in executor.stats.iter() {
                    total_fast_path += worker_stats.fast_path_hits.load(Ordering::Relaxed);
                    total_local += worker_stats.local_queue_hits.load(Ordering::Relaxed);
                    total_steal_attempts += worker_stats.steal_attempts.load(Ordering::Relaxed);
                    total_steal_successes += worker_stats.steal_successes.load(Ordering::Relaxed);
                    total_tasks += worker_stats.tasks_processed.load(Ordering::Relaxed);
                }

                let total_work = total_fast_path + total_local + total_steal_successes;
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
                let steal_success_rate = if total_steal_attempts > 0 {
                    (total_steal_successes as f64 / total_steal_attempts as f64) * 100.0
                } else {
                    0.0
                };

                println!(
                    "  Throughput: {:.2}M polls/sec | Fast: {:.1}% | Local: {:.1}% | Steal: {:.1}% ({}/{} = {:.1}%)",
                    (current - current_window) as f64
                        / (now.sub(current_start)).as_secs_f64()
                        / 1_000_000.0,
                    fast_path_pct,
                    local_pct,
                    steal_pct,
                    total_steal_successes,
                    total_steal_attempts,
                    steal_success_rate
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
