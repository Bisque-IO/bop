#![feature(thread_id_value)]
//! Minimal Work-Stealing Executor Benchmark
//!
//! IDENTICAL to minimal_spsc_executor_benchmark.rs except:
//! - Work-stealing added to SLOW PATH only (when global queue empty)
//! - Fast path (global queue has work) is COMPLETELY UNCHANGED

use bop_mpmc::selector::Selector;
use bop_mpmc::{Mpmc, PushError};
use crossbeam_deque::{Steal, Stealer, Worker};
use std::cell::UnsafeCell;
use std::future::Future;
use std::ops::{Add, Sub};
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::thread;
use std::time::{Duration, Instant};

// Wrapper for async_task::Runnable that can be safely shared across work-stealing
// Uses Arc<AtomicPtr> to allow:
// 1. Multiple clones (needed for work-stealing and MPMC queue)
// 2. Exactly-once extraction via try_take_runnable()
// 3. No violations of from_raw() safety requirements
#[derive(Clone)]
struct TaskPtr(Arc<AtomicPtr<()>>);
unsafe impl Send for TaskPtr {}
unsafe impl Sync for TaskPtr {}

impl TaskPtr {
    fn new(runnable: async_task::Runnable) -> Self {
        let ptr = runnable.into_raw().as_ptr();
        Self(Arc::new(AtomicPtr::new(ptr)))
    }

    // Try to take the runnable - returns None if already taken by another thread
    fn try_take_runnable(&self) -> Option<async_task::Runnable> {
        let ptr = self.0.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(async_task::Runnable::from_raw(NonNull::new_unchecked(ptr))) }
        }
    }

    fn is_null(&self) -> bool {
        self.0.load(Ordering::Acquire).is_null()
    }

    fn null() -> Self {
        Self(Arc::new(AtomicPtr::new(std::ptr::null_mut())))
    }
}

impl Default for TaskPtr {
    fn default() -> Self {
        Self::null()
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
            tasks: UnsafeCell::new(std::array::from_fn(|_| TaskPtr::null())),
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
            cx.waker().wake_by_ref();
            std::thread::sleep(self.duration);
            Poll::Pending
        }
    }
}

/// Minimal future that yields N times
struct YieldNTimes {
    remaining: usize,
    poll_count: Arc<AtomicUsize>,
    task_id: usize,
}

unsafe impl Send for YieldNTimes {}

impl YieldNTimes {
    fn new(count: usize, poll_count: Arc<AtomicUsize>, task_id: usize) -> Self {
        Self {
            remaining: count,
            poll_count,
            task_id,
        }
    }
}

impl Future for YieldNTimes {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let count = self.poll_count.fetch_add(1, Ordering::Relaxed);

        if self.remaining == 0 {
            static TASKS_COMPLETED: AtomicUsize = AtomicUsize::new(0);
            let completed = TASKS_COMPLETED.fetch_add(1, Ordering::Relaxed);
            if completed % 10000 == 0 {
                eprintln!(
                    "[COMPLETE] {} tasks completed (last was task #{})",
                    completed, self.task_id
                );
            }
            Poll::Ready(())
        } else {
            self.remaining -= 1;

            // Debug first few wakes
            if self.task_id < 5 && (100_000 - self.remaining) % 10000 == 0 {
                eprintln!(
                    "[WAKE] Task #{} calling wake_by_ref (remaining: {})",
                    self.task_id, self.remaining
                );
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

/// Inner state - NOW WITH WORK-STEALING
struct ExecutorInner {
    queue: Arc<Mpmc<TaskPtr, 8, 12>>,
}

impl ExecutorInner {
    fn new() -> Self {
        Self {
            queue: Arc::new(Mpmc::<TaskPtr, 8, 12>::new()),
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
            println!(
                "Steal Worker {} on core {:?}",
                steal_worker_id, steal_core_id
            );
            let inner_clone = Arc::clone(&inner);
            let shutdown_clone = Arc::clone(&shutdown);
            let stealers_clone = Arc::clone(&stealers);
            let stats_clone = Arc::clone(&stats);
            let batches_clone = Arc::clone(&batches);

            worker_handles.push(thread::spawn(move || {
                core_affinity::set_for_current(steal_core_id);
                Self::steal_worker_loop(
                    steal_worker_id,
                    stealers_clone,
                    inner_clone,
                    shutdown_clone,
                    stats_clone,
                    batches_clone,
                );
            }));
        }

        Self {
            inner,
            worker_handles,
            shutdown,
            stealers,
            stats,
            batches,
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
        let mut stolen_batch: [TaskPtr; 256] = std::array::from_fn(|_| TaskPtr::null());
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
                let task = &stolen_batch[i];
                if !task.is_null() {
                    if let Some(runnable) = task.try_take_runnable() {
                        worker_stats.tasks_processed.fetch_add(1, Ordering::Relaxed);

                        static STEAL_RUN_COUNT: AtomicUsize = AtomicUsize::new(0);
                        let count = STEAL_RUN_COUNT.fetch_add(1, Ordering::Relaxed);
                        if count % 1000000 == 0 {
                            eprintln!("[RUN] Steal worker ran {} tasks", count);
                        }

                        // Run the task (async-task handles all the polling/rescheduling)
                        runnable.run();
                    }
                }
            }

            // Reset stolen count for next iteration
            stolen_count = 0;

            // No re-enqueue needed - async-task handles rescheduling automatically
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
        let mut temp_batch: [TaskPtr; 512] = std::array::from_fn(|_| TaskPtr::null());
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

            worker_stats
                .tasks_processed
                .fetch_add(size as u64, Ordering::Relaxed);

            // PROCESS BATCH: Now using StealableBatch (allows stealing during processing)
            while let Some(task_ptr) = my_batch.pop_front() {
                if !task_ptr.is_null() {
                    if let Some(runnable) = task_ptr.try_take_runnable() {
                        // Run the task (async-task handles all the polling/rescheduling)
                        static RUN_COUNT: AtomicUsize = AtomicUsize::new(0);
                        let count = RUN_COUNT.fetch_add(1, Ordering::Relaxed);
                        if count % 1000000 == 0 {
                            eprintln!("[RUN] Worker {} ran {} tasks", worker_id, count);
                        }

                        runnable.run();
                    }
                }
            }

            // No re-enqueue needed - async-task handles rescheduling automatically
        }
    }

    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let queue = Arc::clone(&self.inner.queue);
        static SCHEDULE_COUNT: AtomicUsize = AtomicUsize::new(0);
        static SCHEDULE_PUSH_FAILED: AtomicUsize = AtomicUsize::new(0);
        static TASKS_SPAWNED: AtomicUsize = AtomicUsize::new(0);

        // Create async-task with schedule function that pushes to global queue
        let schedule = move |runnable: async_task::Runnable| {
            let count = SCHEDULE_COUNT.fetch_add(1, Ordering::Relaxed);
            if count % 1000000 == 0 {
                eprintln!("[SCHEDULE] Called {} times", count);
            }

            let task_ptr = TaskPtr::new(runnable);
            // Use thread-local producer to push to global queue
            thread_local! {
                static LOCAL_PRODUCER: UnsafeCell<Option<*mut bop_mpmc::Producer<TaskPtr, 8, 12>>> =
                    UnsafeCell::new(None);
            }

            LOCAL_PRODUCER.with(|producer_cell| {
                let producer_ptr = unsafe { &mut *producer_cell.get() };
                let producer = if let Some(p) = producer_ptr {
                    unsafe { &**p }
                } else {
                    let p = Box::new(queue.create_producer_handle().unwrap());
                    let p_raw = Box::into_raw(p) as *mut bop_mpmc::Producer<TaskPtr, 8, 12>;
                    *producer_ptr = Some(p_raw);
                    unsafe { &*p_raw }
                };

                let mut retries = 0usize;
                loop {
                    match producer.try_push(task_ptr) {
                        Ok(()) => break,
                        Err(PushError::Closed(_)) => {
                            let failures = SCHEDULE_PUSH_FAILED.fetch_add(1, Ordering::Relaxed);
                            eprintln!("[SCHEDULE] PUSH FAILED - CLOSED! (failure #{})", failures);
                            break;
                        }
                        Err(PushError::Full(_)) => {
                            retries += 1;
                            if retries > 10000 {
                                eprintln!("[SCHEDULE] Stuck retrying push ({})", retries);
                            }
                            std::hint::spin_loop();
                        }
                    }
                }
            });
        };

        let (runnable, task) = async_task::spawn(future, schedule);

        let task_id = TASKS_SPAWNED.fetch_add(1, Ordering::Relaxed);
        if task_id % 50000 == 0 {
            eprintln!("[SPAWN] Task #{} created", task_id);
        }

        // Push the initial runnable directly to the queue (don't call schedule)
        let task_ptr = TaskPtr::new(runnable);
        let executor_inner = Arc::into_raw(Arc::clone(&self.inner));
        let executor_inner = unsafe { &*executor_inner };
        let producer = unsafe { &*get_producer(executor_inner) };

        loop {
            match producer.try_push(task_ptr) {
                Ok(()) => break,
                Err(PushError::Closed(_)) => {
                    eprintln!("[SPAWN] Initial push failed - CLOSED!");
                    break;
                }
                Err(PushError::Full(_)) => std::hint::spin_loop(),
            }
        }

        task.detach(); // Fire and forget
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
            batches: Arc::clone(&self.batches),
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
                        executor.spawn(YieldNTimes::new(
                            100_000,
                            task_counters[task_id].clone(),
                            task_id,
                        ));
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
