//! Simple high-throughput FastTaskWorker benchmark.
//!
//! This benchmark spawns all tasks upfront and measures how quickly
//! the worker can process millions of polls.

use bop_executor::fast_task_worker::FastTaskWorker;
use bop_executor::task::{ArenaOptions, FutureHelpers, MmapExecutorArena, TaskAllocator};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

const LEAF_COUNT: u32 = 256;
const SLOT_COUNT: u32 = 256;
const SIGNALS_PER_LEAF: usize = (SLOT_COUNT / 64) as usize;

// ══════════════════════════════════════════════════════════════════════════════
// Benchmark Futures
// ══════════════════════════════════════════════════════════════════════════════

/// A task that yields exactly N times before completing.
struct YieldNTimes {
    remaining: usize,
    counter: Arc<AtomicU64>,
}

impl Future for YieldNTimes {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.counter.fetch_add(1, Ordering::Relaxed);
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            self.counter.fetch_add(1, Ordering::Relaxed);
            Poll::Ready(())
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Benchmark Functions
// ══════════════════════════════════════════════════════════════════════════════

fn bench_yielding(
    arena: &MmapExecutorArena,
    num_tasks: usize,
    yields_per_task: usize,
) -> (Duration, u64, u64) {
    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let allocator = TaskAllocator::new(arena, num_cpus);
    let counter = Arc::new(AtomicU64::new(0));
    let mut task_ids = Vec::with_capacity(num_tasks);

    // Spawn all tasks
    for _ in 0..num_tasks {
        if let Some(global_id) = allocator.allocate() {
            let (leaf_idx, slot_idx) = arena.decompose_id(global_id);
            unsafe {
                arena.init_task(global_id);
                let task = arena.task(leaf_idx, slot_idx);
                let future = YieldNTimes {
                    remaining: yields_per_task,
                    counter: counter.clone(),
                };
                let future_ptr = FutureHelpers::box_future(future);
                task.attach_future(future_ptr).expect("attach failed");
                task.schedule();
                task_ids.push(global_id);
            }
        } else {
            break;
        }
    }

    println!("  Spawned {} tasks", task_ids.len());

    // Register worker
    arena.active_tree().register_worker();

    // Run worker
    let mut worker = FastTaskWorker::new(0, SIGNALS_PER_LEAF, arena);
    let expected_polls = task_ids.len() * (yields_per_task + 1);

    println!("  Expected {} polls...", expected_polls);

    let start = Instant::now();
    let mut total_processed = 0;
    let mut empty_count = 0;

    // Run until all work is done - scan all leaves
    loop {
        let mut found_work = false;

        for leaf_idx in 0..(LEAF_COUNT as usize) {
            if worker.try_poll_task(arena, leaf_idx) {
                total_processed += 1;
                found_work = true;
                empty_count = 0;
            }
        }

        if !found_work {
            empty_count += 1;
            if empty_count > 100 {
                break;
            }
        }

        if total_processed >= expected_polls {
            break;
        }
    }

    let duration = start.elapsed();
    let actual_polls = counter.load(Ordering::Relaxed);

    (duration, total_processed as u64, actual_polls)
}

// ══════════════════════════════════════════════════════════════════════════════
// Main
// ══════════════════════════════════════════════════════════════════════════════

fn print_result(name: &str, duration: Duration, worker_polls: u64, actual_polls: u64) {
    let throughput = worker_polls as f64 / duration.as_secs_f64();
    let avg_ns = duration.as_nanos() as f64 / worker_polls as f64;

    println!("┌─────────────────────────────────────────────────────────┐");
    println!("│ {:<55} │", name);
    println!("├─────────────────────────────────────────────────────────┤");
    println!(
        "│ Worker polls:        {:>15}                    │",
        worker_polls
    );
    println!(
        "│ Actual polls:        {:>15}                    │",
        actual_polls
    );
    println!(
        "│ Duration:            {:>15.3} ms               │",
        duration.as_secs_f64() * 1000.0
    );
    println!(
        "│ Throughput:          {:>15.0} polls/sec        │",
        throughput
    );
    println!("│ Avg poll time:       {:>15.1} ns               │", avg_ns);
    println!("└─────────────────────────────────────────────────────────┘");
    println!();
}

fn main() {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║      FastTaskWorker High-Throughput Benchmark (Simple)   ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();
    println!("Configuration:");
    println!("  Leaves:     {} ", LEAF_COUNT);
    println!("  Slots:      {} slots/leaf", SLOT_COUNT);
    println!("  Capacity:   {} total tasks", LEAF_COUNT * SLOT_COUNT);
    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!();

    let arena = MmapExecutorArena::new(LEAF_COUNT, SLOT_COUNT, ArenaOptions::default())
        .expect("Failed to create arena");

    // Benchmark 1: 10K tasks, 10 yields each = 110K polls
    println!("Benchmark 1: 10K tasks × 10 yields = 110K polls");
    let (dur, worker_polls, actual) = bench_yielding(&arena, 10_000, 10);
    print_result("10K tasks × 10 yields", dur, worker_polls, actual);

    // Benchmark 2: 10K tasks, 50 yields each = 510K polls
    println!("Benchmark 2: 10K tasks × 50 yields = 510K polls");
    let (dur, worker_polls, actual) = bench_yielding(&arena, 10_000, 50);
    print_result("10K tasks × 50 yields", dur, worker_polls, actual);

    // Benchmark 3: 10K tasks, 100 yields each = 1.01M polls
    println!("Benchmark 3: 10K tasks × 100 yields = 1.01M polls");
    let (dur, worker_polls, actual) = bench_yielding(&arena, 10_000, 100);
    print_result("10K tasks × 100 yields", dur, worker_polls, actual);

    // Benchmark 4: 20K tasks, 100 yields each = 2.02M polls
    println!("Benchmark 4: 20K tasks × 100 yields = 2.02M polls");
    let (dur, worker_polls, actual) = bench_yielding(&arena, 20_000, 100);
    print_result("20K tasks × 100 yields", dur, worker_polls, actual);

    println!("═══════════════════════════════════════════════════════════");
    println!("                    Benchmark Complete                     ");
    println!("═══════════════════════════════════════════════════════════");
}
