use bop_executor::task::{
    ArenaConfig, ArenaOptions, FutureAllocator, MmapExecutorArena, TaskHandle,
};
use bop_executor::timer::Timer;
use bop_executor::worker::Worker;
use bop_executor::worker_service::{WorkerService, WorkerServiceConfig};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

fn release_tasks(arena: &Arc<MmapExecutorArena>, tasks: Vec<TaskHandle>) {
    for handle in tasks {
        arena.release_task(handle);
    }
}

fn main() {
    let worker_count = num_cpus::get().max(2);
    let timer_tasks = worker_count * 4;
    let duration = Duration::from_secs(3);
    let tick_duration = Duration::from_micros(500);

    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(8, 64).unwrap(), ArenaOptions::default())
            .expect("arena"),
    );

    let service = WorkerService::start(WorkerServiceConfig { tick_duration });
    let shutdown = Arc::new(AtomicBool::new(false));

    let mut workers = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        workers.push(Worker::with_shutdown(
            Arc::clone(&arena),
            Some(Arc::clone(&service)),
            Arc::clone(&shutdown),
        ));
    }

    let fired_total = Arc::new(AtomicUsize::new(0));
    let completed_total = Arc::new(AtomicUsize::new(0));
    let stop_flag = Arc::new(AtomicBool::new(false));

    let mut task_handles = Vec::with_capacity(timer_tasks);
    for _ in 0..timer_tasks {
        let handle = arena.reserve_task().expect("reserve task");
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);

        let fired = Arc::clone(&fired_total);
        let completed = Arc::clone(&completed_total);
        let stop = Arc::clone(&stop_flag);
        let interval = Duration::from_millis(2);

        let future_ptr = FutureAllocator::box_future(async move {
            let timer = Timer::new();
            loop {
                if stop.load(Ordering::Acquire) {
                    timer.cancel();
                    completed.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                timer.delay(interval).await;
                fired.fetch_add(1, Ordering::Relaxed);
            }
        });

        let task = handle.task();
        task.attach_future(future_ptr).expect("attach future");
        arena.activate_task(handle);
        task_handles.push(handle);
    }

    let mut threads = Vec::with_capacity(worker_count);
    for mut worker in workers {
        let shutdown_flag = Arc::clone(&shutdown);
        let stop_flag = Arc::clone(&stop_flag);
        threads.push(thread::spawn(move || {
            while !shutdown_flag.load(Ordering::Acquire) {
                let progress = worker.run_once();
                if !progress && stop_flag.load(Ordering::Acquire) {
                    break;
                }
                if !progress {
                    thread::sleep(Duration::from_micros(100));
                }
            }
        }));
    }

    let start = Instant::now();
    while start.elapsed() < duration {
        thread::sleep(Duration::from_millis(50));
    }

    stop_flag.store(true, Ordering::Release);

    let wait_start = Instant::now();
    while completed_total.load(Ordering::Relaxed) < timer_tasks
        && wait_start.elapsed() < Duration::from_secs(1)
    {
        thread::sleep(Duration::from_millis(10));
    }

    shutdown.store(true, Ordering::Release);
    for handle in threads {
        let _ = handle.join();
    }

    println!(
        "Timers fired {} times across {} workers in {:?}",
        fired_total.load(Ordering::Relaxed),
        worker_count,
        duration
    );

    release_tasks(&arena, task_handles);
}
