//! Minimal Executor Benchmark - Polling Ceiling Test
//!
//! This benchmark measures the absolute ceiling for task polling performance
//! with a complete but minimal executor implementation including proper waker VTable.

use std::cell::UnsafeCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::Instant;

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

/// Waker data for the minimal executor
struct WakerData {
    ready: AtomicBool,
}

impl WakerData {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            ready: AtomicBool::new(false),
        })
    }
}

/// Waker VTable for the minimal executor
static MINIMAL_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |data| {
        // Clone - increment Arc ref count
        let waker_data_ptr = data as *const WakerData;
        let arc = unsafe { Arc::from_raw(waker_data_ptr) };
        let cloned = arc.clone();
        let _ = Arc::into_raw(arc); // Don't drop original
        RawWaker::new(Arc::into_raw(cloned) as *const (), &MINIMAL_WAKER_VTABLE)
    },
    |data| {
        // Wake by value - mark as ready
        let waker_data_ptr = data as *const WakerData;
        let waker_data = unsafe { Arc::from_raw(waker_data_ptr) };
        waker_data.ready.store(true, Ordering::Release);
        // Arc drops here
    },
    |data| {
        // Wake by reference - mark as ready
        let waker_data_ptr = data as *const WakerData;
        let waker_data = unsafe { &*waker_data_ptr };
        waker_data.ready.store(true, Ordering::Release);
    },
    |data| {
        // Drop - decrement Arc ref count
        let waker_data_ptr = data as *const WakerData;
        unsafe { Arc::from_raw(waker_data_ptr) };
    },
);

/// Minimal task with embedded waker
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
            let raw_waker = RawWaker::new(waker_data_ptr as *const (), &MINIMAL_WAKER_VTABLE);
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

    fn is_ready(&self) -> bool {
        self.waker_data.ready.load(Ordering::Acquire)
    }

    fn reset_ready(&self) {
        self.waker_data.ready.store(false, Ordering::Release);
    }
}

/// Minimal single-threaded executor
struct MinimalExecutor {
    pending_tasks: Vec<Task>,
}

impl MinimalExecutor {
    fn new() -> Self {
        Self {
            pending_tasks: Vec::new(),
        }
    }

    fn spawn<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.pending_tasks.push(Task::new(future));
    }

    fn run(&mut self) {
        while !self.pending_tasks.is_empty() {
            let mut i = 0;
            while i < self.pending_tasks.len() {
                match self.pending_tasks[i].poll() {
                    Poll::Ready(_) => {
                        // Task completed, remove it
                        self.pending_tasks.swap_remove(i);
                        // Don't increment i since we swapped in a new task
                    }
                    Poll::Pending => {
                        // Task still pending
                        self.pending_tasks[i].reset_ready();
                        i += 1;
                    }
                }
            }
        }
    }
}

fn main() {
    println!("🎯 Minimal Executor Benchmark - Polling Ceiling Test");
    println!("====================================================\n");

    // Test 1: Single task with 1 million yields
    {
        println!("=== Test 1: Minimal Executor - Single task, 1M yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MinimalExecutor::new();

        executor.spawn(YieldNTimes::new(1_000_000, poll_count.clone()));

        let start = Instant::now();
        executor.run();
        let duration = start.elapsed();

        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );
    }

    println!();

    // Test 2: Single task with 10 million yields
    {
        println!("=== Test 2: Minimal Executor - Single task, 10M yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MinimalExecutor::new();

        executor.spawn(YieldNTimes::new(10_000_000, poll_count.clone()));

        let start = Instant::now();
        executor.run();
        let duration = start.elapsed();

        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );
    }

    println!();

    // Test 3: Single task with 100 million yields
    {
        println!("=== Test 3: Minimal Executor - Single task, 100M yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MinimalExecutor::new();

        executor.spawn(YieldNTimes::new(100_000_000, poll_count.clone()));

        let start = Instant::now();
        executor.run();
        let duration = start.elapsed();

        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );
    }

    println!();

    // Test 4: Multiple tasks (1000 tasks × 10K yields each = 10M total polls)
    {
        println!("=== Test 4: Minimal Executor - 1000 tasks × 10K yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MinimalExecutor::new();

        for _ in 0..1000 {
            executor.spawn(YieldNTimes::new(10_000, poll_count.clone()));
        }

        let start = Instant::now();
        executor.run();
        let duration = start.elapsed();

        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Tasks: 1000");
        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );
    }

    println!();

    // Test 5: Many small tasks (10K tasks × 100 yields each = 1M total polls)
    {
        println!("=== Test 5: Minimal Executor - 10K tasks × 100 yields ===");
        let poll_count = Arc::new(AtomicUsize::new(0));
        let mut executor = MinimalExecutor::new();

        for _ in 0..10_000 {
            executor.spawn(YieldNTimes::new(100, poll_count.clone()));
        }

        let start = Instant::now();
        executor.run();
        let duration = start.elapsed();

        let polls = poll_count.load(Ordering::Relaxed);

        println!("  Tasks: 10,000");
        println!("  Polls: {}", polls);
        println!("  Duration: {:?}", duration);
        println!(
            "  Throughput: {:.2}M polls/sec",
            polls as f64 / duration.as_secs_f64() / 1_000_000.0
        );
    }

    println!("\n=== Summary ===");
    println!("This benchmark shows a complete minimal executor with:");
    println!("  • Proper waker VTable with Arc-based ref counting");
    println!("  • Task struct with embedded waker");
    println!("  • Single-threaded executor with Vec-based pending queue");
    println!("  • Zero MPSC queue overhead (local Vec only)");
    println!("\nCompare this to your MPSC-based executor to understand overhead:");
    println!("  • MPSC queue operations (pop/push with fairness/signals)");
    println!("  • Multi-threading synchronization and contention");
    println!("  • Task batching between queue and processing");
}
