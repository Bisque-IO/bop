//! Minimal SPSC Executor Benchmark
//!
//! This benchmark uses the actual SPSC queue from bop-mpmc to measure
//! single-threaded executor performance with real queue operations.
//!
//! This version uses raw pointers instead of Arc for zero-overhead task management.
//! Uses thread-local executor model similar to MpmcBlocking's producer management.

use bop_mpmc::SpscChainSimple;
use bop_mpmc::selector::Selector;
use bop_mpmc::signal::Signal;
use bop_mpmc::waker::SignalWaker as BopWaker;
use std::cell::UnsafeCell;
use std::future::Future;
use std::pin::Pin;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicUsize, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};

// #[global_allocator]
// static allocator: bop_allocator::BopAllocator = bop_allocator::BopAllocator;

// Send-safe wrapper for raw pointers
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

// Queue holds raw pointers to tasks - zero Arc overhead
type TaskQueue = SpscChainSimple<TaskPtr, 4096>;

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
            // Only wake if we'll remain pending
            if self.remaining > 0 {}
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Task using raw pointers - no Arc overhead
struct TaskInner {
    future: UnsafeCell<Pin<Box<dyn Future<Output = ()> + Send>>>,
    executor: *const ExecutorInner,
    flags: AtomicU8,
}

impl TaskInner {
    /// Schedule this task for execution
    /// Returns true if it was successfully scheduled
    pub fn schedule(&self) -> bool {
        if (self.flags.load(Ordering::Acquire) & SCHEDULED) != IDLE {
            return false;
        }

        let previous_flags = self.flags.fetch_or(SCHEDULED, Ordering::Release);
        let scheduled_nor_executing = (previous_flags & (SCHEDULED | EXECUTING)) == IDLE;

        if scheduled_nor_executing {
            unsafe {
                let (queue_id, queue) = get_or_create_queue_raw(self.executor);
                if let Err(_err) = queue.push(TaskPtr(self as *const _ as *mut TaskInner)) {
                    eprintln!("Task schedule push failed queue_id {}", queue_id);
                }
            }
            true
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
        }))
    }

    unsafe fn poll(task_ptr: *mut Self) -> Poll<()> {
        unsafe {
            let task = &*task_ptr;
            let future = &mut *task.future.get();
            let waker = Self::create_waker(task_ptr);
            let mut cx = Context::from_waker(&waker);
            future.as_mut().poll(&mut cx)
        }
    }

    fn create_waker(task_ptr: *mut Self) -> Waker {
        unsafe {
            let raw_waker = RawWaker::new(task_ptr as *const (), &VTABLE);
            Waker::from_raw(raw_waker)
        }
    }

    unsafe fn drop_task(task_ptr: *mut Self) {
        unsafe {
            // let _ = Box::from_raw(task_ptr);
        }
    }
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(
    |data| {
        // Clone - just clone the pointer
        RawWaker::new(data, &VTABLE)
    },
    |data| {
        // Wake by value - enqueue task for wakeup
        unsafe {
            let task_ptr = data as *mut TaskInner;
            let task = &*task_ptr;
            task.schedule();
        }
    },
    |data| {
        // Wake by reference - enqueue task for wakeup
        unsafe {
            let task_ptr = data as *mut TaskInner;
            let task = &*task_ptr;
            task.schedule();
        }
    },
    |_data| {
        // Drop waker - noop
    },
);

/// Thread-local queue cache - similar to MpmcBlocking's producer management
thread_local! {
    static QUEUE_CACHE: UnsafeCell<Option<(usize, *const TaskQueue)>> = UnsafeCell::new(None);
}

/// Get or create a queue for the current thread (raw pointer version)
unsafe fn get_or_create_queue_raw(inner_ptr: *const ExecutorInner) -> (usize, &'static TaskQueue) {
    let inner = unsafe { &*inner_ptr };
    QUEUE_CACHE.with(|cache_tls| {
        let cache_slot = unsafe { &mut *cache_tls.get() };

        if let Some((queue_id, queue_ptr)) = cache_slot {
            // Verify cached queue is still valid
            let stored_ptr = inner.queues[*queue_id].load(Ordering::Acquire);
            if stored_ptr == *queue_ptr as *mut TaskQueue {
                return (*queue_id, unsafe { &**queue_ptr });
            }
        }

        // Need to create or find a queue
        let mut queue_id = None;
        let mut queue: Option<*mut TaskQueue> = None;
        let max_queues = inner.max_queues;

        'outer: for bit_index in 0..64 {
            for signal_index in 0..max_queues / 64 {
                let queue_index = (signal_index * 64) + bit_index;
                if inner.queues[queue_index].load(Ordering::Acquire).is_null() {
                    let signal = inner.signals[signal_index].clone();

                    // Create new queue
                    let new_queue = Box::into_raw(Box::new(TaskQueue::new(
                        bit_index as u64,
                        Arc::clone(&inner.waker),
                        signal,
                    )));

                    let queue_raw_ptr = new_queue as *mut TaskQueue;

                    // Try to store atomically
                    match inner.queues[queue_index].compare_exchange(
                        ptr::null_mut(),
                        queue_raw_ptr,
                        Ordering::Release,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            // Successfully registered
                            inner.queue_count.fetch_add(1, Ordering::Relaxed);
                            queue_id = Some(queue_index);
                            queue = Some(new_queue);
                            break 'outer;
                        }
                        Err(_) => {
                            // Race condition: another thread registered here
                            unsafe { Arc::from_raw(queue_raw_ptr) };
                            continue;
                        }
                    }
                }
            }
        }

        let queue_id = queue_id.expect("max queues exceeded");
        let queue = queue.expect("max queues exceeded");

        // Leak the Arc to keep it alive
        let queue_static = unsafe { &*(queue as *const TaskQueue) };

        unsafe {
            *cache_tls.get() = Some((queue_id, queue));
        }

        (queue_id, queue_static)
    })
}

/// Get or create a queue for the current thread (Arc version)
fn get_or_create_queue(inner: &Arc<ExecutorInner>) -> (usize, &'static TaskQueue) {
    unsafe { get_or_create_queue_raw(Arc::as_ptr(inner)) }
}

/// Inner state holding Vec of queues - similar to MpmcBlockingInner
struct ExecutorInner {
    max_queues: usize,
    queues: Vec<AtomicPtr<TaskQueue>>,
    queue_count: AtomicUsize,
    /// Shared waker for all queues
    waker: Arc<BopWaker>,
    tasks: dashmap::DashMap<usize, TaskPtr>,
    /// Signal bitset - one signal bit per queue
    signals: Vec<Signal>,
}

impl ExecutorInner {
    fn new(max_queues: usize) -> Self {
        let queues = (0..max_queues)
            .map(|_| AtomicPtr::new(ptr::null_mut()))
            .collect();

        let signals = (0..max_queues / 64)
            .map(|i| Signal::with_index(i as u64))
            .collect();

        Self {
            max_queues,
            queues,
            queue_count: AtomicUsize::new(0),
            waker: Arc::new(BopWaker::new()),
            tasks: dashmap::DashMap::new(),
            signals,
        }
    }
}

/// Global executor with thread-local spawning and multiple worker threads
struct Executor {
    inner: Arc<ExecutorInner>,
    worker_handles: Vec<thread::JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl Executor {
    fn new(num_threads: usize) -> Self {
        let inner = Arc::new(ExecutorInner::new(512));
        let shutdown = Arc::new(AtomicBool::new(false));

        // Spawn worker threads
        let mut worker_handles = Vec::new();
        for worker_id in 0..num_threads {
            let inner_clone = Arc::clone(&inner);
            let shutdown_clone = Arc::clone(&shutdown);

            let handle = thread::spawn(move || {
                Self::worker_loop(worker_id, inner_clone, shutdown_clone);
            });
            worker_handles.push(handle);
        }

        Self {
            inner,
            worker_handles,
            shutdown,
        }
    }

    /// Worker thread main loop - uses atomic signal selection like MpmcBlocking
    fn worker_loop(worker_id: usize, inner: Arc<ExecutorInner>, shutdown: Arc<AtomicBool>) {
        eprintln!("Worker {} starting", worker_id);
        let mut enqueue = Vec::<TaskPtr>::new();
        let mut found_work = false;
        let mut selector = Selector::new();
        let (_, local_queue) = get_or_create_queue(&inner);

        let max_queues = inner.max_queues;
        let max_queues_mask = max_queues - 1;
        let signal_count = inner.max_queues / 64;

        loop {
            if shutdown.load(Ordering::Relaxed) {
                eprintln!("shutting down worker {}", worker_id);
                break;
            }

            // Cycle through signal words
            for _i in 0..signal_count {
                // Get signal index from selector for fairness
                let mut queue_index = (selector.next() as usize) & max_queues_mask;
                let map_index = queue_index / 64;
                let mut signal_bit = (queue_index - (map_index * 64)) as u64;

                let signal = &inner.signals[map_index];
                let signal_value = signal.load(Ordering::Acquire);

                // eprintln!(
                //     "worker {} searching for queue queue index {}  signal_index {}  signal bit {}",
                //     worker_id, queue_index, map_index, signal_bit
                // );

                if signal_value == 0 {
                    continue;
                }

                // Find nearest set bit for fairness
                signal_bit = bop_mpmc::find_nearest(signal_value, signal_bit);

                if signal_bit >= 64 {
                    continue;
                }

                queue_index = map_index * 64 + (signal_bit as usize);

                // eprintln!(
                //     "worker {} selected queue index {}  signal_index {}  signal bit {}",
                //     worker_id, queue_index, map_index, signal_bit
                // );

                // Atomically acquire the bit
                let (bit, expected, acquired) = signal.try_acquire(signal_bit);

                if !acquired {
                    continue;
                }

                // Load queue pointer
                let queue_ptr = inner.queues[queue_index].load(Ordering::Acquire);
                if queue_ptr.is_null() {
                    // Check if signal was empty before
                    let empty = expected == bit;
                    if empty {
                        inner.waker.decrement();
                        eprintln!(
                            "Worker {} queue_id {} is null and waker decremented!",
                            worker_id, queue_index
                        );
                    } else {
                        eprintln!("Worker {} queue_id {} is null!", worker_id, queue_index);
                    }
                    continue;
                }

                // Check if signal was empty before
                let empty = expected == bit;
                if empty {
                    inner.waker.decrement();
                }

                // SAFETY: pointer is valid
                let queue = unsafe { &*queue_ptr };

                // Set DRAINING flag before draining
                queue.flags().store(EXECUTING, Ordering::Release);

                let enqueue_ptr = &mut enqueue as *mut Vec<TaskPtr>;

                // Drain tasks from this queue
                let (drained, can_dispose) = queue.drain_with(
                    |task_ptr| unsafe {
                        (*task_ptr.get()).flags.store(EXECUTING, Ordering::Release);
                        let result = TaskInner::poll(task_ptr.get());
                        match result {
                            Poll::Ready(()) => {
                                unsafe {
                                    let task_key = task_ptr.get() as *mut _ as usize;
                                    inner.tasks.remove(&task_key);
                                }
                                TaskInner::drop_task(task_ptr.get());
                            }
                            Poll::Pending => {
                                let after_flags = (*task_ptr.get())
                                    .flags
                                    .fetch_sub(EXECUTING, Ordering::AcqRel);
                                if after_flags & SCHEDULED != IDLE {
                                    (*enqueue_ptr).push(task_ptr);
                                }
                            }
                        }
                    },
                    1024,
                );

                for task in enqueue.drain(..) {
                    let _ = local_queue.push(task);
                }

                if drained > 0 {
                    found_work = true;
                    // Queue still has items or isn't closed - reschedule
                    let _ = queue.flags().fetch_sub(EXECUTING, Ordering::AcqRel);
                    queue.flags().store(SCHEDULED, Ordering::Release);

                    let prev = signal.set_with_bit(bit);
                    if prev == 0 && !empty {
                        inner.waker.increment();
                    }
                } else if !can_dispose {
                    let after_flags = queue.flags().fetch_sub(EXECUTING, Ordering::AcqRel);
                    if after_flags & SCHEDULED != IDLE {
                        let prev = signal.set_with_bit(bit);
                        if prev == 0 {
                            inner.waker.increment();
                        }
                    }
                }

                // Handle cleanup and rescheduling
                if can_dispose {
                    // Queue is empty and closed - clean up
                    let _ = queue.flags().fetch_sub(EXECUTING, Ordering::AcqRel);

                    let old_ptr = inner.queues[queue_index].swap(ptr::null_mut(), Ordering::AcqRel);
                    if !old_ptr.is_null() {
                        inner.queue_count.fetch_sub(1, Ordering::Relaxed);
                        unsafe {
                            drop(Box::from_raw(old_ptr));
                        }
                    }
                }
            }

            if !found_work {
                // Wait for signal
                inner.waker.wait_timeout_ms(Duration::from_millis(1));
                // std::hint::spin_loop();
            }
        }
    }

    /// Get or assign a queue for this thread, using thread-local caching
    pub fn get_queue(&self) -> (usize, &TaskQueue) {
        get_or_create_queue(&self.inner)
    }

    /// Spawn a task on the thread-local queue
    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let (queue_index, queue) = self.get_queue();
        let signal_idx = queue_index / 64;
        let signal_value_before = self.inner.signals[signal_idx].load(Ordering::Relaxed);
        let executor_ptr = Arc::as_ptr(&self.inner);
        let task_ptr = TaskInner::new(Box::pin(future), executor_ptr);
        let task_ptr_wrapper = TaskPtr::new(task_ptr);
        self.inner
            .tasks
            .insert(task_ptr as usize, TaskPtr::new(task_ptr));
        match queue.push(task_ptr_wrapper) {
            Ok(()) => {
                let signal_value_after = self.inner.signals[signal_idx].load(Ordering::Relaxed);
                // eprintln!(
                //     "Spawn push ok queue_id {queue_index} signal_after {signal_value_after} flags {}",
                //     queue.flags().load(Ordering::Relaxed)
                // );
            }
            Err(err) => {
                eprintln!("Spawn push error queue_id {queue_index}");
                unsafe {
                    TaskInner::drop_task(task_ptr);
                }
                panic!("Failed to spawn task: {:?}", err);
            }
        }
    }

    fn shutdown(&mut self) {
        // Signal shutdown to all worker threads
        self.shutdown.store(true, Ordering::SeqCst);

        // Wake all workers by incrementing waker
        for _ in 0..self.worker_handles.len() {
            self.inner.waker.wake();
        }

        eprintln!("waiting for all workers to stop");

        // Wait for worker threads to finish
        while let Some(handle) = self.worker_handles.pop() {
            let _ = handle.join();
        }

        eprintln!("all workers stopped");

        // Clean up queue instances
        for queue_slot in &self.inner.queues {
            let queue_ptr = queue_slot.swap(ptr::null_mut(), Ordering::AcqRel);
            if !queue_ptr.is_null() {
                unsafe {
                    let queue = Box::from_raw(queue_ptr);
                    queue.close();
                    drop(queue);
                }
            }
        }
    }
}

impl Clone for Executor {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            worker_handles: Vec::new(), // Clone doesn't duplicate worker threads
            shutdown: Arc::clone(&self.shutdown),
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        // Only shutdown if this is the last reference and we have worker threads
        if Arc::strong_count(&self.inner) == 1 && !self.worker_handles.is_empty() {
            self.shutdown();
        }
    }
}

fn main() {
    println!("🎯 Minimal SPSC Executor Benchmark");
    println!("===================================\n");

    // {
    //     let mut executor = Executor::new(1);
    //     executor.shutdown();
    // }

    // Test 1: Single thread, single task with 1 million yields
    {
        println!("=== Test 1: 1 thread - Single task, 1M yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        println!("Creating executor...");
        let mut executor = Executor::new(1);
        println!("Executor created");

        let start = Instant::now();
        println!("Spawning task...");
        executor.spawn(YieldNTimes::new(10_000_000, poll_count.clone()));
        println!("Task spawned");
        loop {
            thread::sleep(Duration::from_millis(1));
            if poll_count.load(Ordering::Relaxed) >= 10_000_000 {
                break;
            }
        }

        let duration = start.elapsed();
        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );

        executor.shutdown();
    }

    // println!();

    // // Test 2: 2 threads, single task with 10 million yields
    // {
    //     println!("=== Test 2: 2 threads - Single task, 10M yields ===");
    //     let poll_count = Box::leak(Box::new(AtomicUsize::new(0)));
    //     let poll_count_ptr = poll_count as *const AtomicUsize;
    //     let executor = Executor::new(2);

    //     let start = Instant::now();
    //     executor.spawn(YieldNTimes::new(10_000_000, poll_count_ptr));

    //     loop {
    //         thread::sleep(Duration::from_millis(1));
    //         if poll_count.load(Ordering::Relaxed) >= 10_000_001 {
    //             break;
    //         }
    //     }

    //     let duration = start.elapsed();
    //     let polls = poll_count.load(Ordering::Relaxed);

    //     println!("  Polls: {}", polls);
    //     println!("  Duration: {:?}", duration);
    //     println!(
    //         "  Throughput: {:.2}M polls/sec",
    //         polls as f64 / duration.as_secs_f64() / 1_000_000.0
    //     );
    // }

    // println!();

    // Test 3: 4 threads, 1000 tasks × 10K yields each
    {
        println!("=== Test 3: 4 threads - 1000 tasks × 10K yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = Executor::new(8);

        let start = Instant::now();
        for _ in 0..1000 {
            executor.spawn(YieldNTimes::new(10_000, poll_count.clone()));
        }

        let mut last_count = 0;
        let mut _stall_iterations = 0;
        loop {
            thread::sleep(Duration::from_millis(10));
            let current = poll_count.load(Ordering::Relaxed);

            if current >= 10_001_000 {
                break;
            }

            if current == last_count {
                _stall_iterations += 1;
            } else {
                _stall_iterations = 0;
            }

            last_count = current;
        }

        let duration = start.elapsed();
        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Threads: 4");
        println!("  Tasks: 1000");
        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );

        executor.shutdown();
    }

    println!("\n=== Summary ===");
    println!("Multi-queue executor with dynamic SpscChainSimple queues");
    println!("  • Tasks are *mut TaskInner (raw pointers)");
    println!("  • Thread-local queue caching with round-robin assignment");
    println!("  • Worker threads use atomic signal selection (like MpmcBlocking)");
    println!("  • No task refcounting overhead");
    println!("  • Waker re-enqueues tasks on wake (self-waking via queue pointer)");
    println!("  • drain_with() processes 1024 tasks per batch");
    println!("  • Shared BopWaker and individual Signal per queue");
}
