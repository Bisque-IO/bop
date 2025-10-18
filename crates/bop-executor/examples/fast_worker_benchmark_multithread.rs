//! Multi-threaded benchmark for the standard Worker/Task executor.
//!
//! Creates a configurable number of tasks that cooperatively yield a set
//! number of times. Multiple workers run in parallel until every task
//! completes. Throughput and per-worker statistics are printed for each run.

use std::future::Future;
use std::sync::{Arc, Barrier, Mutex};
use std::task::{Context, Poll};
use std::thread;
use std::time::{Duration, Instant};

use bop_executor::deque::Stealer;
use bop_executor::task::{ArenaConfig, ArenaOptions, FutureHelpers, MmapExecutorArena, TaskHandle};
use bop_executor::worker::{Worker, WorkerStats};

const DEFAULT_LEAVES: usize = 16;
const DEFAULT_TASKS_PER_LEAF: usize = 2048;

#[derive(Debug)]
struct YieldNTimes {
    remaining: usize,
}

impl Future for YieldNTimes {
    type Output = ();

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.remaining > 0 {
            self.remaining -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

fn setup_arena(leaf_count: usize, tasks_per_leaf: usize) -> Arc<MmapExecutorArena> {
    let config = ArenaConfig::new(leaf_count, tasks_per_leaf).expect("invalid arena config");
    Arc::new(
        MmapExecutorArena::with_config(config, ArenaOptions::default())
            .expect("failed to create arena"),
    )
}

fn task_from_handle<'a>(
    arena: &'a Arc<MmapExecutorArena>,
    handle: &TaskHandle,
) -> &'a bop_executor::task::Task {
    let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
    unsafe { arena.task(handle.leaf_idx(), slot_idx) }
}

fn worker_loop(
    arena: Arc<MmapExecutorArena>,
    worker_idx: usize,
    num_workers: usize,
    task_count: usize,
    yields_per_task: usize,
    stealer_slots: Arc<Vec<Mutex<Option<Stealer<TaskHandle>>>>>,
    setup_barrier: Arc<Barrier>,
) -> (WorkerStats, Vec<TaskHandle>) {
    let mut worker = Worker::new(Arc::clone(&arena));
    let my_stealer = worker.yield_stealer();
    {
        let mut slot = stealer_slots[worker_idx]
            .lock()
            .expect("stealer slot poisoned");
        *slot = Some(my_stealer);
    }

    let core = core_affinity::get_core_ids().unwrap()[worker_idx];
    // core_affinity::set_for_current(core);

    setup_barrier.wait();

    let mut peer_stealers = Vec::with_capacity(num_workers.saturating_sub(1));
    for idx in 0..num_workers {
        if idx == worker_idx {
            continue;
        }
        let guard = stealer_slots[idx].lock().expect("stealer slot poisoned");
        if let Some(ref stealer) = *guard {
            peer_stealers.push(stealer.clone());
        }
    }
    worker.set_yield_stealers(peer_stealers);

    setup_barrier.wait();

    let leaf_count = arena.leaf_count();
    let mut leaf = worker_idx % leaf_count;
    let mut handles = Vec::with_capacity(task_count);
    for _ in 0..task_count {
        let handle = loop {
            if let Some(handle) = arena.reserve_task_in_leaf(leaf) {
                break handle;
            }
            leaf = (leaf + 1) % leaf_count;
        };

        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let task = task_from_handle(&arena, &handle);
        let future_ptr = FutureHelpers::box_future(YieldNTimes {
            remaining: yields_per_task,
        });
        task.attach_future(future_ptr).expect("attach future");
        arena.activate_task(handle);

        handles.push(handle);
        leaf = (leaf + num_workers) % leaf_count;
    }

    setup_barrier.wait();

    worker.run_until_idle();

    setup_barrier.wait();

    (worker.stats().clone(), handles)
}

fn run_workers(
    arena: Arc<MmapExecutorArena>,
    total_tasks: usize,
    yields_per_task: usize,
    num_workers: usize,
) -> Vec<WorkerStats> {
    let mut stats = Vec::with_capacity(num_workers);
    let mut all_handles: Vec<Vec<TaskHandle>> = Vec::with_capacity(num_workers);
    let stealer_slots = Arc::new(
        (0..num_workers)
            .map(|_| Mutex::new(None))
            .collect::<Vec<Mutex<Option<Stealer<TaskHandle>>>>>(),
    );
    let setup_barrier = Arc::new(Barrier::new(num_workers));
    thread::scope(|scope| {
        let mut joins = Vec::with_capacity(num_workers);
        let base = total_tasks / num_workers;
        let remainder = total_tasks % num_workers;
        for idx in 0..num_workers {
            let arena = Arc::clone(&arena);
            let mut count = base;
            if idx < remainder {
                count += 1;
            }
            let stealer_slots = Arc::clone(&stealer_slots);
            let setup_barrier = Arc::clone(&setup_barrier);
            joins.push(scope.spawn(move || {
                worker_loop(
                    arena,
                    idx,
                    num_workers,
                    count,
                    yields_per_task,
                    stealer_slots,
                    setup_barrier,
                )
            }));
        }
        for handle in joins {
            let (worker_stats, handles) = handle.join().expect("worker thread panicked");
            stats.push(worker_stats);
            all_handles.push(handles);
        }
    });

    for handle_set in all_handles {
        for handle in handle_set {
            arena.release_task(handle);
        }
    }

    stats
}

fn summarize(stats: &[WorkerStats], duration: Duration) {
    let total_polls: u64 = stats.iter().map(|s| s.tasks_polled).sum();
    let throughput = total_polls as f64 / duration.as_secs_f64();
    println!(
        "    Total polls: {:>12} | Throughput: {:>12.0} polls/sec",
        total_polls, throughput
    );
    for (idx, stat) in stats.iter().enumerate() {
        println!(
            "      worker {:>2}: polls {:>8} completed {:>8} yield {:>6}",
            idx, stat.tasks_polled, stat.completed_count, stat.yielded_count
        );
    }
}

fn run_benchmark(
    leaf_count: usize,
    tasks_per_leaf: usize,
    num_workers: usize,
    total_tasks: usize,
    yields_per_task: usize,
) {
    println!(
        "> workers={} tasks={} yields/task={}",
        num_workers, total_tasks, yields_per_task
    );
    let arena = setup_arena(leaf_count, tasks_per_leaf);

    let start = Instant::now();
    let stats = run_workers(
        Arc::clone(&arena),
        total_tasks,
        yields_per_task,
        num_workers,
    );
    let elapsed = start.elapsed();

    summarize(&stats, elapsed);
    println!();
}

fn main() {
    println!(
        "Worker/Task multithread benchmark
"
    );

    let leaf_count = DEFAULT_LEAVES;
    let tasks_per_leaf = DEFAULT_TASKS_PER_LEAF;
    let configs = [
        (1, 64, 10_000),
        (2, 1024, 50_000),
        (4, 1024, 50_000),
        (8, 1024, 50_000),
    ];

    for (workers, tasks, yields) in configs {
        run_benchmark(leaf_count, tasks_per_leaf, workers, tasks, yields);
    }
}
