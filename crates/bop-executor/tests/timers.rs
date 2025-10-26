use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::thread;
use std::time::{Duration, Instant};

use bop_executor::task::{ArenaConfig, ArenaOptions, FutureHelpers, MmapExecutorArena};
use bop_executor::timer::{Timer, TimerState};
use bop_executor::worker::Worker;
use bop_executor::worker_service::{WorkerService, WorkerServiceConfig};
use futures_lite::{future::poll_fn, pin};
use std::task::Poll;

fn make_worker(arena: Arc<MmapExecutorArena>) -> Worker {
    let service = WorkerService::start(WorkerServiceConfig::default());
    Worker::new(arena, Some(service))
}

fn run_worker_until(worker: &mut Worker, done: &impl Fn() -> bool) {
    let start = Instant::now();
    while !done() && start.elapsed() < Duration::from_secs(2) {
        worker.run_once();
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn timer_one_shot_wakes_task() {
    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(2, 8).unwrap(), ArenaOptions::default())
            .unwrap(),
    );
    let mut worker = make_worker(Arc::clone(&arena));

    let handle = arena.reserve_task().expect("reserve task");
    let global = handle.global_id(arena.tasks_per_leaf());
    arena.init_task(global);

    let timer = Arc::new(Timer::new());
    let completed = Arc::new(AtomicBool::new(false));
    let future_ptr = {
        let timer = Arc::clone(&timer);
        let completed = Arc::clone(&completed);
        FutureHelpers::box_future(async move {
            timer.delay(Duration::from_millis(5)).await;
            completed.store(true, AtomicOrdering::Relaxed);
        })
    };

    let task = handle.task();
    task.attach_future(future_ptr).unwrap();
    arena.activate_task(handle);

    run_worker_until(&mut worker, &|| completed.load(AtomicOrdering::Relaxed));

    assert!(completed.load(AtomicOrdering::Relaxed));
    assert_eq!(worker.stats().completed_count, 1);
    assert_eq!(timer.state(), TimerState::Idle);

    arena.release_task(handle);
}

#[test]
fn timer_repeats_until_cancel() {
    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(2, 8).unwrap(), ArenaOptions::default())
            .unwrap(),
    );
    let mut worker = make_worker(Arc::clone(&arena));

    let fired = Arc::new(AtomicUsize::new(0));
    let handle = arena.reserve_task().expect("reserve task");
    let global = handle.global_id(arena.tasks_per_leaf());
    arena.init_task(global);

    let timer = Arc::new(Timer::new());
    let future_ptr = {
        let timer = Arc::clone(&timer);
        let fired = Arc::clone(&fired);
        FutureHelpers::box_future(async move {
            loop {
                timer.delay(Duration::from_millis(3)).await;
                let count = fired.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                if count >= 2 {
                    timer.cancel();
                    break;
                }
            }
        })
    };

    let task = handle.task();
    task.attach_future(future_ptr).unwrap();
    arena.activate_task(handle);

    run_worker_until(&mut worker, &|| fired.load(AtomicOrdering::Relaxed) >= 2);

    assert!(fired.load(AtomicOrdering::Relaxed) >= 2);
    assert_eq!(worker.stats().completed_count, 1);
    assert!(matches!(
        timer.state(),
        TimerState::Idle | TimerState::Cancelled
    ));

    arena.release_task(handle);
}

#[test]
fn timer_interval_multi_worker_sustained() {
    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(4, 32).unwrap(), ArenaOptions::default())
            .unwrap(),
    );
    let service = WorkerService::start(WorkerServiceConfig {
        tick_duration: Duration::from_millis(1),
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    let mut worker_a = Worker::with_shutdown(
        Arc::clone(&arena),
        Some(Arc::clone(&service)),
        Arc::clone(&shutdown),
    );
    let mut worker_b = Worker::with_shutdown(
        Arc::clone(&arena),
        Some(Arc::clone(&service)),
        Arc::clone(&shutdown),
    );

    let task_count = 8usize;
    let target_per_task = 6usize;
    let completed = Arc::new(AtomicUsize::new(0));

    struct TimerRecord {
        timer: Arc<Timer>,
        fired: Arc<AtomicUsize>,
    }

    let mut records = Vec::with_capacity(task_count);
    let mut handles = Vec::with_capacity(task_count);

    for _ in 0..task_count {
        let handle = arena.reserve_task().expect("reserve task");
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);

        let timer = Arc::new(Timer::new());
        let fired = Arc::new(AtomicUsize::new(0));
        let interval = Duration::from_millis(3);

        let future_ptr = {
            let timer = Arc::clone(&timer);
            let fired = Arc::clone(&fired);
            let completed = Arc::clone(&completed);
            FutureHelpers::box_future(async move {
                loop {
                    timer.delay(interval).await;
                    let count = fired.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                    if count >= target_per_task {
                        completed.fetch_add(1, AtomicOrdering::Relaxed);
                        break;
                    }
                }
            })
        };

        let task = handle.task();
        task.attach_future(future_ptr).unwrap();
        arena.activate_task(handle);

        records.push(TimerRecord { timer, fired });
        handles.push(handle);
    }

    let total = task_count;
    let completed_clone = Arc::clone(&completed);

    let worker_thread_a = thread::spawn(move || {
        while completed_clone.load(AtomicOrdering::Relaxed) < total {
            worker_a.run_once();
            thread::sleep(Duration::from_millis(1));
        }
    });

    let completed_clone = Arc::clone(&completed);
    let worker_thread_b = thread::spawn(move || {
        while completed_clone.load(AtomicOrdering::Relaxed) < total {
            worker_b.run_once();
            thread::sleep(Duration::from_millis(1));
        }
    });

    worker_thread_a.join().unwrap();
    worker_thread_b.join().unwrap();

    assert_eq!(completed.load(AtomicOrdering::Relaxed), total);
    for record in &records {
        assert!(
            record.fired.load(AtomicOrdering::Relaxed) >= target_per_task,
            "timer fired {} times",
            record.fired.load(AtomicOrdering::Relaxed)
        );
        assert!(matches!(
            record.timer.state(),
            TimerState::Idle | TimerState::Cancelled
        ));
    }

    for handle in handles {
        arena.release_task(handle);
    }
}

#[test]
fn timer_cancel_prevents_fire() {
    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(2, 8).unwrap(), ArenaOptions::default())
            .unwrap(),
    );
    let mut worker = make_worker(Arc::clone(&arena));

    let handle = arena.reserve_task().expect("reserve task");
    let global = handle.global_id(arena.tasks_per_leaf());
    arena.init_task(global);

    let timer = Arc::new(Timer::new());
    let fired = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let future_ptr = {
        let timer = Arc::clone(&timer);
        let fired = Arc::clone(&fired);
        let done = Arc::clone(&done);
        FutureHelpers::box_future(async move {
            let delay = timer.delay(Duration::from_millis(5));
            pin!(delay);
            poll_fn(|cx| match delay.as_mut().poll(cx) {
                Poll::Pending => {
                    timer.cancel();
                    Poll::Ready(())
                }
                Poll::Ready(()) => {
                    fired.store(true, AtomicOrdering::Relaxed);
                    Poll::Ready(())
                }
            })
            .await;
            done.store(true, AtomicOrdering::Relaxed);
        })
    };

    let task = handle.task();
    task.attach_future(future_ptr).unwrap();
    arena.activate_task(handle);

    run_worker_until(&mut worker, &|| done.load(AtomicOrdering::Relaxed));

    assert!(!fired.load(AtomicOrdering::Relaxed));
    assert!(matches!(timer.state(), TimerState::Cancelled));

    arena.release_task(handle);
}
