#![feature(thread_id_value)]
//! Minimal SPSC Executor Benchmark
//!
//! This benchmark uses the actual SPSC queue from bop-mpmc to measure
//! single-threaded executor performance with real queue operations.
//!
//! This version uses raw pointers instead of Arc for zero-overhead task management.
//! Uses thread-local executor model similar to MpmcBlocking's producer management.

use bop_mpmc::selector::Selector;
use bop_mpmc::{Mpmc, PushError, EXECUTING, IDLE, SCHEDULED};
use futures_lite::FutureExt;
use std::cell::UnsafeCell;
use std::future::Future;
use std::mem::MaybeUninit;
use std::ops::{Add, Sub};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};

// #[global_allocator]
// static allocator: bop_allocator::BopAllocator = bop_allocator::BopAllocator;

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
            // let h = unsafe { &mut *producer_entry };
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
    /// Schedule this task for execution
    /// Returns true if it was successfully scheduled
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
                        Err(PushError::Full(_)) => {
                            std::hint::spin_loop();
                        }
                    }
                }
                return true;
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
                    Err(PushError::Full(_)) => {
                        std::hint::spin_loop();
                    }
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
        unsafe {
            let task = &*task_ptr;
            *task.yielded.get() = false;
            let future = &mut *task.future.get();
            let waker = unsafe {
                let raw_waker = RawWaker::new(task_ptr as *const (), &VTABLE_YIELD);
                Waker::from_raw(raw_waker)
            };
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

static VTABLE_YIELD: RawWakerVTable = RawWakerVTable::new(
    |data| {
        // Clone - just clone the pointer
        RawWaker::new(data, &VTABLE)
    },
    |data| {
        // Wake by value - enqueue task for wakeup
        unsafe {
            let task_ptr = data as *mut TaskInner;
            let task = &*task_ptr;
            *task.yielded.get() = true;
        }
    },
    |data| {
        // Wake by reference - enqueue task for wakeup
        unsafe {
            let task_ptr = data as *mut TaskInner;
            let task = &*task_ptr;
            *task.yielded.get() = true;
        }
    },
    |_data| {
        // Drop waker - noop
    },
);

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
            let thread_id: u64 = std::thread::current().id().as_u64().into();
            task.schedule();
        }
    },
    |_data| {
        // Drop waker - noop
    },
);

static VTABLE_NO_OP: RawWakerVTable = RawWakerVTable::new(
    |data| {
        // Clone - just clone the pointer
        RawWaker::new(data, &VTABLE)
    },
    |data| {},
    |data| {},
    |_data| {
        // Drop waker - noop
    },
);

/// Inner state holding Vec of queues - similar to MpmcBlockingInner
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

/// Global executor with thread-local spawning and multiple worker threads
struct Executor {
    inner: Arc<ExecutorInner>,
    worker_handles: Vec<thread::JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl Executor {
    fn new(num_threads: usize) -> Self {
        let inner = Arc::new(ExecutorInner::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        let core_ids = core_affinity::get_core_ids().unwrap();

        // Create a thread for each active CPU core.
        let worker_handles = core_ids.into_iter().filter(|id| id.lt(&core_affinity::CoreId { id: 8 })).map(|id| {


            println!("core id: {:?}", id);
            let inner_clone = Arc::clone(&inner);
            let shutdown_clone = Arc::clone(&shutdown);
            thread::spawn(move || {
                // Pin this thread to a single CPU core.
                let res = core_affinity::set_for_current(id);
                // if (res) {
                //     // Do more work after this.
                // }
                Self::worker_loop(0, inner_clone, shutdown_clone);
            })
        }).collect::<Vec<_>>();

        // Spawn worker threads
        // let mut worker_handles = Vec::new();
        // for worker_id in 0..num_threads {
        //     let inner_clone = Arc::clone(&inner);
        //     let shutdown_clone = Arc::clone(&shutdown);
        //
        //     let handle = thread::spawn(move || {
        //         Self::worker_loop(worker_id, inner_clone, shutdown_clone);
        //     });
        //     worker_handles.push(handle);
        // }

        Self {
            inner,
            worker_handles,
            shutdown,
        }
    }

    /// Worker thread main loop - uses atomic signal selection like MpmcBlocking
    fn worker_loop(worker_id: usize, inner: Arc<ExecutorInner>, shutdown: Arc<AtomicBool>) {
        let mut selector = Selector::new();

        let mut batch: [TaskPtr; 512] = std::array::from_fn(|_| TaskPtr(std::ptr::null_mut()));

        let executor_inner = Arc::into_raw(Arc::clone(&inner));
        let executor_inner = unsafe { &*executor_inner };
        let mut enqueue = Vec::<TaskPtr>::with_capacity(1024);
        // let producer = inner.queue.create_producer_handle().unwrap();
        let producer = unsafe { &*get_producer(executor_inner) };

        let mut count = 0;
        let mut next_target = 5000000;

        // let thread_id = std::thread::current().id().as_u64().into();

        loop {
            let size = inner
                .queue
                .try_pop_n_with_selector(&mut selector, &mut batch);

            if size == 0 {
                std::hint::spin_loop();
                continue;
            }
            count += size;
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
                                (*task_ptr.get())
                                    .flags
                                    .store(SCHEDULED, Ordering::Release);
                                // loop {
                                //     match producer.try_push(task_ptr) {
                                //         Ok(_) => break,
                                //         Err(PushError::Closed(_)) => break,
                                //         Err(PushError::Full(_)) => std::hint::spin_loop(),
                                //     }
                                // }
                                enqueue.push(task_ptr);
                            } else {
                                let after_flags = (*task_ptr.get())
                                    .flags
                                    .fetch_sub(EXECUTING, Ordering::AcqRel);
                                if after_flags & SCHEDULED != IDLE {
                                    enqueue.push(task_ptr);
                                    // loop {
                                    //     match producer.try_push(task_ptr) {
                                    //         Ok(_) => break,
                                    //         Err(PushError::Closed(_)) => break,
                                    //         Err(PushError::Full(_)) => std::hint::spin_loop(),
                                    //     }
                                    // }
                                }
                            }

                            // loop {
                            //     match producer.try_push(task_ptr) {
                            //         Ok(_) => break,
                            //         Err(PushError::Closed(_)) => break,
                            //         Err(PushError::Full(_)) => std::hint::spin_loop(),
                            //     }
                            // }
                            // enqueue.push(task_ptr);
                        }
                    }
                }
            }

            if !enqueue.is_empty() {
                let mut remaining = 0;
                let mut count = 0;
                while remaining < enqueue.len() {
                    match producer.try_push_n(&mut enqueue[remaining..]) {
                        Ok(pushed) => {
                            remaining += pushed;
                        }
                        Err(_) => {
                            // std::hint::spin_loop();
                            count += 1;
                            if count & 31 == 0 {
                                std::hint::spin_loop();
                            }
                            // if count >= 10 {
                            //     // process
                            //     while remaining < enqueue.len() {
                            //
                            //     }
                            //     break;
                            // }
                        }
                    }
                }


                enqueue.clear();
            }

            if count >= next_target {
                // println!("{} - {}", worker_id, count);
                next_target += 5000000;
            }
        }
    }

    /// Spawn a task on the thread-local queue
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
                Err(_) => {
                    std::hint::spin_loop();
                }
            }
        }
    }

    fn shutdown(&self) {
        // Signal shutdown to all worker threads
        self.shutdown.store(true, Ordering::SeqCst);

        // Wake all workers by incrementing waker
        for _ in 0..self.worker_handles.len() {
            // self.inner.waker.wake();
        }

        eprintln!("waiting for all workers to stop");

        // Wait for worker threads to finish
        // while let Some(handle) = self.worker_handles.pop() {
        //     let _ = handle.join();
        // }

        eprintln!("all workers stopped");

        while !self.inner.queue.close() {
            std::thread::sleep(Duration::from_millis(1));
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
    // {
    //     println!("=== Test 1: 1 thread - Single task, 1M yields ===");
    //     let poll_count = Arc::new(AtomicUsize::new(0));
    //     println!("Creating executor...");
    //     let mut executor = Executor::new(1);
    //     println!("Executor created");

    //     let start = Instant::now();
    //     println!("Spawning task...");
    //     executor.spawn(YieldNTimes::new(10_000_000, poll_count.clone()));
    //     println!("Task spawned");
    //     loop {
    //         thread::sleep(Duration::from_millis(1));
    //         if poll_count.load(Ordering::Relaxed) >= 10_000_000 {
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

    //     executor.shutdown();
    // }

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
    //
    // let poll_count = Arc::new(AtomicUsize::new(0));
    //
    // std::thread::sleep(Duration::from_millis(10));
    // let start = Instant::now();
    //
    // let waker = unsafe {
    //     let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE_NO_OP);
    //     Waker::from_raw(raw_waker)
    // };
    //
    // let mut future = Box::pin(YieldNTimes::new(100_000_000, Arc::clone(&poll_count)));
    //
    // let mut cx: Context<'_> = unsafe { Context::from_waker(&waker) };
    //
    // let mut last_count = 0;
    // let mut _stall_iterations = 0;
    // loop {
    //     // thread::sleep(Duration::from_millis(10));
    //
    //     match future.poll(&mut cx) {
    //         Poll::Pending => {}
    //         Poll::Ready(_) => break,
    //     }
    //
    //     let current = poll_count.load(Ordering::Relaxed);
    //
    //     if current >= 100_000_000 {
    //         break;
    //     }
    //
    //     if current == last_count {
    //         _stall_iterations += 1;
    //     } else {
    //         _stall_iterations = 0;
    //     }
    //
    //     last_count = current;
    // }
    //
    // let duration = start.elapsed();
    // let polls = poll_count.load(Ordering::Relaxed);
    //
    // println!("  Threads: 4");
    // println!("  Tasks: 1000");
    // println!("  Polls: {}", polls);
    // println!("  Duration: {:?}", duration);
    // println!(
    //     "  Throughput: {:.2}M polls/sec",
    //     polls as f64 / duration.as_secs_f64() / 1_000_000.0
    // );

    // {
    //     println!("=== Tokio: 4 threads - 1000 tasks × 10K yields ===");
    //     let tasks_count = 200_000;
    //     let task_counters: Arc<Vec<_>> = Arc::new((0..tasks_count).map(|_| Arc::new(AtomicUsize::new(0))).collect());
    //     let runtime = Arc::new(tokio::runtime::Builder::new_multi_thread().worker_threads(8).enable_all().build().unwrap());
    //     // let runtime = Arc::new(tokio::runtime::Builder::new_current_thread().build().unwrap());
    //
    //     let start_threads: Vec<_> = (0..1)
    //         .map(|_i| {
    //             let task_counters = Arc::clone(&task_counters);
    //             let runtime = Arc::clone(&runtime);
    //             std::thread::spawn(move || {
    //                 for task_id in 0..tasks_count {
    //                     runtime.spawn(YieldNTimes::new(100_000, task_counters[task_id].clone()));
    //                 }
    //             })
    //         })
    //         .collect();
    //
    //     // for _ in 0..1000 {
    //     //     executor.spawn(YieldNTimes::new(10_000, poll_count.clone()));
    //     // }
    //     for h in start_threads {
    //         h.join().unwrap();
    //     }
    //     println!("started threads");
    //
    //     let mut starting_count = 0;
    //     for counter in task_counters.iter() {
    //         starting_count += counter.load(Ordering::Relaxed);
    //     }
    //     let start = Instant::now();
    //
    //     let mut last_count = 0;
    //     let mut _stall_iterations = 0;
    //
    //     let mut current_window = 0;
    //     let mut next_target = start.add(Duration::from_secs(1));
    //     let mut current_start = start;
    //
    //     loop {
    //         thread::sleep(Duration::from_millis(1));
    //
    //         let mut current = 0;
    //         for counter in task_counters.iter() {
    //             current += counter.load(Ordering::Relaxed);
    //         }
    //
    //         if current >= 10_000_000_000 {
    //             break;
    //         }
    //
    //         if current == last_count {
    //             _stall_iterations += 1;
    //         } else {
    //             _stall_iterations = 0;
    //         }
    //
    //         last_count = current;
    //
    //         let now = Instant::now();
    //         if now.ge(&next_target) {
    //             println!(
    //                 "  Throughput: {:.2}M polls/sec",
    //                 (current - current_window) as f64 / (now.sub(current_start)).as_secs_f64() / 1_000_000.0
    //             );
    //             next_target = now.add(Duration::from_secs(1));
    //             current_start = now;
    //             current_window = current;
    //         }
    //     }
    //
    //     let duration = start.elapsed();
    //     let mut polls = 0;
    //     for counter in task_counters.iter() {
    //         polls += counter.load(Ordering::Relaxed);
    //     }
    //     polls -= starting_count;
    //
    //     println!("  Threads: 4");
    //     println!("  Tasks: 1000");
    //     println!("  Polls: {}", polls);
    //     println!("  Duration: {:?}", duration);
    //     println!(
    //         "  Throughput: {:.2}M polls/sec",
    //         polls as f64 / duration.as_secs_f64() / 1_000_000.0
    //     );
    //
    // }

    // Test 3: 4 threads, 1000 tasks × 10K yields each
    {
        println!("=== Test 3: 4 threads - 1000 tasks × 10K yields ===");
        let tasks_count = 200_000;
        let task_counters: Arc<Vec<_>> = Arc::new((0..tasks_count).map(|_| Arc::new(AtomicUsize::new(0))).collect());
        let executor = Arc::new(Executor::new(24));

        let start_threads: Vec<_> = (0..1)
            .map(|_i| {
                let task_counters = Arc::clone(&task_counters);
                let executor = Arc::clone(&executor);
                std::thread::spawn(move || {
                    for task_id in 0..tasks_count {
                        executor.spawn(YieldNTimes::new(100_000, task_counters[task_id].clone()));
                    }
                })
            })
            .collect();

        // for _ in 0..1000 {
        //     executor.spawn(YieldNTimes::new(10_000, poll_count.clone()));
        // }
        for h in start_threads {
            h.join().unwrap();
        }
        println!("started threads");

        let mut starting_count = 0;
        for counter in task_counters.iter() {
            starting_count += counter.load(Ordering::Relaxed);
        }
        let start = Instant::now();

        let mut last_count = 0;
        let mut _stall_iterations = 0;

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
                _stall_iterations += 1;
            } else {
                _stall_iterations = 0;
            }

            last_count = current;

            let now = Instant::now();
            if now.ge(&next_target) {
                println!(
                    "  Throughput: {:.2}M polls/sec",
                    (current - current_window) as f64 / (now.sub(current_start)).as_secs_f64() / 1_000_000.0
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
