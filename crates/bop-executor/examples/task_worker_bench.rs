use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use bop_executor::task::{ArenaConfig, ArenaOptions, FutureHelpers, MmapExecutorArena, TaskHandle};
use bop_executor::worker::Worker;

fn setup_arena(leaf_count: usize, tasks_per_leaf: usize) -> Arc<MmapExecutorArena> {
    let config = ArenaConfig::new(leaf_count, tasks_per_leaf).expect("invalid arena config");
    Arc::new(
        MmapExecutorArena::with_config(config, ArenaOptions::default())
            .expect("failed to create arena"),
    )
}

fn prepare_handles(arena: &Arc<MmapExecutorArena>, count: usize) -> Vec<TaskHandle> {
    let mut handles = Vec::with_capacity(count);
    for _ in 0..count {
        if let Some(handle) = arena.reserve_task() {
            let global = handle.global_id(arena.tasks_per_leaf());
            arena.init_task(global);
            let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
            let task = unsafe { arena.task(handle.leaf_idx(), slot_idx) };
            let future_ptr = FutureHelpers::box_future(async {});
            task.attach_future(future_ptr).expect("attach future");
            arena.activate_task(handle);
            handles.push(handle);
        }
    }
    handles
}

fn release_handles(arena: &Arc<MmapExecutorArena>, handles: Vec<TaskHandle>) {
    for handle in handles {
        arena.deactivate_task(handle);
        if let Some(ptr) = unsafe {
            let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
            let task = arena.task(handle.leaf_idx(), slot_idx);
            task.take_future()
        } {
            unsafe { FutureHelpers::drop_boxed(ptr) };
        }
        arena.release_task(handle);
    }
}

fn bench_task_schedule_finish(thread_count: usize, iterations: usize) -> Duration {
    let leaf_count = thread_count.next_power_of_two().max(1);
    let arena = setup_arena(leaf_count, 64);
    let handles = prepare_handles(&arena, thread_count);
    let start = Instant::now();
    thread::scope(|scope| {
        for handle in handles.iter().copied() {
            let arena = Arc::clone(&arena);
            scope.spawn(move || {
                let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
                let task = unsafe { arena.task(handle.leaf_idx(), slot_idx) };
                let signal =
                    unsafe { &*arena.task_signal_ptr(handle.leaf_idx(), handle.signal_idx()) };
                for _ in 0..iterations {
                    task.schedule();
                    let (remaining, acquired) = signal.try_acquire(handle.bit_idx());
                    if acquired {
                        if remaining == 0 {
                            arena
                                .active_tree()
                                .mark_signal_inactive(handle.leaf_idx(), handle.signal_idx());
                        }
                        task.begin();
                        task.finish();
                    }
                }
            });
        }
    });
    let elapsed = start.elapsed();
    release_handles(&arena, handles);
    elapsed
}

fn bench_worker_run_until_idle(thread_count: usize) -> Duration {
    let leaf_count = thread_count.next_power_of_two().max(1);
    let arena = setup_arena(leaf_count, 64);
    let total_tasks = leaf_count * arena.tasks_per_leaf();
    let counter = Arc::new(AtomicUsize::new(0));
    let handles = prepare_handles(&arena, total_tasks);
    let start = Instant::now();
    thread::scope(|scope| {
        for _ in 0..thread_count {
            let arena = Arc::clone(&arena);
            let counter = Arc::clone(&counter);
            scope.spawn(move || {
                let mut worker = Worker::new(arena);
                while counter.fetch_add(1, Ordering::Relaxed) < total_tasks {
                    if !worker.run_once() {
                        thread::yield_now();
                    }
                }
                worker.run_until_idle();
            });
        }
    });
    let elapsed = start.elapsed();
    release_handles(&arena, handles);
    elapsed
}

fn main() {
    let iterations = std::env::var("BENCH_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000_000);

    println!("Task schedule/begin/finish benchmark (iterations={iterations})");
    for &threads in &[1, 2, 4, 8] {
        let elapsed = bench_task_schedule_finish(threads, iterations);
        println!(
            "  threads={threads:<2} elapsed={:?} per-iter={:?}",
            elapsed,
            elapsed / iterations as u32
        );
    }

    println!("\nWorker run_until_idle benchmark");
    for &threads in &[1, 2, 4] {
        let elapsed = bench_worker_run_until_idle(threads);
        println!("  threads={threads:<2} elapsed={:?}", elapsed);
    }
}
