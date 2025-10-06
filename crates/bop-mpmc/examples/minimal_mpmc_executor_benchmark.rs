//! Minimal MPMC Executor Benchmark
//!
//! This benchmark uses the actual MpscBlocking queue from bop-mpmc to measure
//! single-consumer executor performance with real MPMC queue operations.

use bop_mpmc::mpmc::MpmcBlocking;
use std::cell::UnsafeCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};

/// Minimal future that yields N times
struct YieldNTimes {
    remaining: usize,
    poll_count: Arc<AtomicUsize>,
}

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

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.poll_count.fetch_add(1, Ordering::Relaxed);

        if self.remaining == 0 {
            Poll::Ready(())
        } else {
            self.remaining -= 1;
            Poll::Pending
        }
    }
}

/// Waker data for the MPMC executor
struct WakerData {
    ready: AtomicUsize,
}

impl WakerData {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            ready: AtomicUsize::new(0),
        })
    }
}

/// Waker VTable for the MPMC executor
static MPMC_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |data| {
        // Clone - increment Arc ref count
        let waker_data_ptr = data as *const WakerData;
        let arc = unsafe { Arc::from_raw(waker_data_ptr) };
        let cloned = arc.clone();
        let _ = Arc::into_raw(arc); // Don't drop original
        RawWaker::new(Arc::into_raw(cloned) as *const (), &MPMC_WAKER_VTABLE)
    },
    |data| {
        // Wake by value - mark as ready
        let waker_data_ptr = data as *const WakerData;
        let waker_data = unsafe { Arc::from_raw(waker_data_ptr) };
        waker_data.ready.fetch_add(1, Ordering::Release);
        // Arc drops here
    },
    |data| {
        // Wake by reference - mark as ready
        let waker_data_ptr = data as *const WakerData;
        let waker_data = unsafe { &*waker_data_ptr };
        waker_data.ready.fetch_add(1, Ordering::Release);
    },
    |data| {
        // Drop - decrement Arc ref count
        let waker_data_ptr = data as *const WakerData;
        unsafe { Arc::from_raw(waker_data_ptr) };
    },
);

/// Task with embedded waker
struct Task {
    future: UnsafeCell<Pin<Box<dyn Future<Output = ()> + Send>>>,
    waker_data: Arc<WakerData>,
    waker: Waker,
}

impl Task {
    fn new<F>(future: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let waker_data = WakerData::new();
        let waker_data_ptr = Arc::into_raw(waker_data.clone());
        let waker = unsafe {
            let raw_waker = RawWaker::new(waker_data_ptr as *const (), &MPMC_WAKER_VTABLE);
            Waker::from_raw(raw_waker)
        };

        Self {
            future: UnsafeCell::new(Box::pin(future)),
            waker_data,
            waker,
        }
    }

    fn poll(&mut self) -> Poll<()> {
        let future = unsafe { &mut *self.future.get() };
        let mut cx = Context::from_waker(&self.waker);
        future.as_mut().poll(&mut cx)
    }
}

/// Single-consumer executor using MpscBlocking queue with separate producer/consumer threads
struct MpmcExecutor {
    task_queue: Arc<MpmcBlocking<Task, 8192>>,
    shutdown: Arc<AtomicBool>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl MpmcExecutor {
    fn new() -> Self {
        let task_queue = Arc::new(MpmcBlocking::<Task, 8192>::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        // Spawn consumer thread
        let queue_clone = task_queue.clone();
        let shutdown_clone = shutdown.clone();

        let worker_handle = thread::spawn(move || {
            let mut pending_tasks: Vec<Task> = Vec::with_capacity(8192);

            loop {
                // First, re-poll any pending tasks from previous iteration
                let mut i = 0;
                while i < pending_tasks.len() {
                    match pending_tasks[i].poll() {
                        Poll::Ready(_) => {
                            // Task completed - remove and drop it
                            pending_tasks.swap_remove(i);
                            // Don't increment i, we just swapped in a new task
                        }
                        Poll::Pending => {
                            // Still pending, keep it for next iteration
                            i += 1;
                        }
                    }
                }

                // Try to pop tasks one by one and process immediately (no intermediate vector)
                let mut popped_any = false;
                for _ in 0..8192 {
                    match queue_clone.pop() {
                        Ok(mut task) => {
                            popped_any = true;
                            match task.poll() {
                                Poll::Ready(_) => {
                                    // Task completed - drop it
                                }
                                Poll::Pending => {
                                    // Keep pending task local instead of re-queuing
                                    pending_tasks.push(task);
                                }
                            }
                        }
                        Err(_) => {
                            // Queue empty, stop trying
                            break;
                        }
                    }
                }

                if !popped_any {
                    // Queue is empty
                    if pending_tasks.is_empty() {
                        // Check for shutdown signal
                        if shutdown_clone.load(Ordering::Acquire) {
                            // No tasks left and shutdown requested, exit
                            break;
                        }
                        // Sleep briefly to avoid busy-waiting when truly idle
                        thread::sleep(Duration::from_micros(10));
                    }
                    // If we have pending tasks, continue the loop without sleeping
                }
            }
        });

        Self {
            task_queue,
            shutdown,
            worker_handle: Some(worker_handle),
        }
    }

    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Push task to MPMC queue (producer thread)
        let _ = self.task_queue.push_spin(Task::new(future));
    }

    fn shutdown(&mut self) {
        // Signal shutdown
        self.shutdown.store(true, Ordering::Release);

        // Wait for worker thread to finish
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

fn main() {
    println!("🎯 Minimal MPMC Executor Benchmark");
    println!("==================================\n");

    // Test 1: Single task with 1 million yields
    {
        println!("=== Test 1: MPMC Executor - Single task, 1M yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MpmcExecutor::new();

        let start = Instant::now();
        executor.spawn(YieldNTimes::new(1_000_000, poll_count.clone()));

        // Wait for completion
        loop {
            thread::sleep(Duration::from_millis(1));
            if poll_count.load(Ordering::Relaxed) >= 1_000_001 {
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

    println!();

    // Test 2: Single task with 10 million yields
    {
        println!("=== Test 2: MPMC Executor - Single task, 10M yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MpmcExecutor::new();

        let start = Instant::now();
        executor.spawn(YieldNTimes::new(10_000_000, poll_count.clone()));

        // Wait for completion
        loop {
            thread::sleep(Duration::from_millis(1));
            if poll_count.load(Ordering::Relaxed) >= 10_000_001 {
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

    println!();

    // Test 3: Single task with 100 million yields
    {
        println!("=== Test 3: MPMC Executor - Single task, 100M yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MpmcExecutor::new();

        let start = Instant::now();
        executor.spawn(YieldNTimes::new(100_000_000, poll_count.clone()));

        // Wait for completion
        loop {
            thread::sleep(Duration::from_millis(10));
            if poll_count.load(Ordering::Relaxed) >= 100_000_001 {
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

    println!();

    // Test 4: Multiple tasks (1000 tasks × 10K yields each = 10M total polls)
    {
        println!("=== Test 4: MPMC Executor - 1000 tasks × 10K yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MpmcExecutor::new();

        let start = Instant::now();
        for _ in 0..1000 {
            executor.spawn(YieldNTimes::new(10_000, poll_count.clone()));
        }

        // Wait for completion (1000 tasks × (10000 yields + 1 final poll) = 10,001,000 polls)
        loop {
            thread::sleep(Duration::from_millis(10));
            if poll_count.load(Ordering::Relaxed) >= 10_001_000 {
                break;
            }
        }

        let duration = start.elapsed();
        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Tasks: 1000");
        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );

        executor.shutdown();
    }

    println!();

    // Test 5: Many small tasks (1K tasks × 1K yields each = 1M total polls)
    {
        println!("=== Test 5: MPMC Executor - 1K tasks × 1K yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MpmcExecutor::new();

        let start = Instant::now();
        for _ in 0..1_000 {
            executor.spawn(YieldNTimes::new(1_000, poll_count.clone()));
        }

        // Wait for completion (1000 tasks × (1000 yields + 1 final poll) = 1,001,000 polls)
        loop {
            thread::sleep(Duration::from_millis(1));
            if poll_count.load(Ordering::Relaxed) >= 1_001_000 {
                break;
            }
        }

        let duration = start.elapsed();
        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Tasks: 1,000");
        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );

        executor.shutdown();
    }

    println!();

    // Test 6: Queue overhead test - spawn tasks then poll
    {
        println!("=== Test 6: MPMC Executor - Queue overhead (5K tasks × 200 yields) ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MpmcExecutor::new();

        // Spawn tasks (tests queue push overhead)
        let spawn_start = Instant::now();
        for _ in 0..5_000 {
            executor.spawn(YieldNTimes::new(200, poll_count.clone()));
        }
        let spawn_duration = spawn_start.elapsed();

        // Wait for completion (5000 tasks × (200 yields + 1 final poll) = 1,005,000 polls)
        let run_start = Instant::now();
        loop {
            thread::sleep(Duration::from_millis(1));
            if poll_count.load(Ordering::Relaxed) >= 1_005_000 {
                break;
            }
        }
        let run_duration = run_start.elapsed();

        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Tasks: 5,000");
        println!("  Spawn time: {:?}", spawn_duration);
        println!("  Run time: {:?}", run_duration);
        println!("  Total polls: {}", polls);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / run_duration.as_secs_f64() / 1_000_000.0
        );

        executor.shutdown();
    }

    println!("\n=== Summary ===");
    println!("This executor uses the actual MpscBlocking queue from bop-mpmc:");
    println!("  • Separate producer/consumer threads");
    println!("  • Main thread spawns tasks (producer)");
    println!("  • Worker thread consumes and executes tasks (consumer)");
    println!("  • Process tasks immediately after pop (no intermediate vector)");
    println!("  • Local Vec for pending task re-polling (avoids re-queue)");
    println!("\nThis shows the overhead of MPMC queue operations vs SPSC:");
    println!("  • Queue push with selector fairness mechanism");
    println!("  • Queue pop with multi-producer synchronization");
    println!("  • No intermediate allocation or copy");
    println!("  • No re-queue overhead (pending tasks stay local)");
    println!("  • Thread synchronization via atomic operations");
}
