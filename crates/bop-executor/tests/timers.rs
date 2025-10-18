use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bop_executor::task::{ArenaConfig, ArenaOptions, FutureHelpers, MmapExecutorArena};
use bop_executor::timers::{TimerConfig, TimerHandle, TimerService, TimerState, sleep};
use bop_executor::worker::{Worker, schedule_timer_for_current_task};

#[test]
fn sleep_future_wakes_task() {
    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(2, 8).unwrap(), ArenaOptions::default())
            .unwrap(),
    );

    let timer_service = TimerService::start(TimerConfig::default());

    let mut worker = Worker::new_with_timers(Arc::clone(&arena), Some(Arc::clone(&timer_service)));

    let handle = arena.reserve_task().expect("reserve task");
    let global = handle.global_id(arena.tasks_per_leaf());
    arena.init_task(global);

    let future_ptr = FutureHelpers::box_future(async {
        sleep::sleep(Duration::from_millis(1)).await;
    });
    let task = handle.task();
    task.attach_future(future_ptr).unwrap();
    arena.activate_task(handle);

    for _ in 0..200 {
        thread::sleep(Duration::from_micros(200));
        worker.run_once();
    }

    assert!(
        worker.stats().completed_count >= 1,
        "sleep future should complete the task"
    );

    arena.release_task(handle);
    timer_service.shutdown();
}

#[test]
fn timers_migrate_across_workers_integration() {
    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(2, 8).unwrap(), ArenaOptions::default())
            .unwrap(),
    );
    let service = TimerService::start(TimerConfig {
        tick_duration: Duration::from_millis(1),
    });
    let counter = Arc::new(AtomicUsize::new(0));

    let handle = {
        let mut worker_a = Worker::new_with_timers(Arc::clone(&arena), Some(Arc::clone(&service)));
        let task = arena.reserve_task().expect("reserve task");
        let global = task.global_id(arena.tasks_per_leaf());
        arena.init_task(global);

        let future = FutureHelpers::box_future({
            let counter = Arc::clone(&counter);
            async move {
                sleep::sleep(Duration::from_millis(10)).await;
                counter.fetch_add(1, AtomicOrdering::Relaxed);
            }
        });
        let task_ref = task.task();
        task_ref.attach_future(future).unwrap();
        arena.activate_task(task);
        worker_a.run_once();
        task
    };

    let mut worker_b = Worker::new_with_timers(Arc::clone(&arena), Some(Arc::clone(&service)));
    let start = std::time::Instant::now();
    while counter.load(AtomicOrdering::Relaxed) == 0 && start.elapsed() < Duration::from_secs(2) {
        worker_b.run_once();
        thread::sleep(Duration::from_millis(2));
    }

    assert_eq!(counter.load(AtomicOrdering::Relaxed), 1);

    arena.release_task(handle);
    service.shutdown();
}

#[test]
fn interleaved_cancellation_against_worker_poll() {
    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(2, 8).unwrap(), ArenaOptions::default())
            .unwrap(),
    );
    let service = TimerService::start(TimerConfig {
        tick_duration: Duration::from_millis(1),
    });
    let mut worker = Worker::new_with_timers(Arc::clone(&arena), Some(Arc::clone(&service)));

    let timer_handle = Arc::new(TimerHandle::new());
    let service_slot: Arc<Mutex<Option<Arc<TimerService>>>> = Arc::new(Mutex::new(None));
    let cancelled = Arc::new(AtomicBool::new(false));

    struct ContendedTimer {
        handle: Arc<TimerHandle>,
        service_slot: Arc<Mutex<Option<Arc<TimerService>>>>,
        completed: bool,
        scheduled: bool,
    }

    impl std::future::Future for ContendedTimer {
        type Output = ();

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            if self.completed {
                return std::task::Poll::Ready(());
            }
            if !self.scheduled {
                if let Some((_, service)) =
                    schedule_timer_for_current_task(cx, &self.handle, Duration::from_millis(20))
                {
                    *self.service_slot.lock().unwrap() = Some(service);
                    self.scheduled = true;
                }
                return std::task::Poll::Pending;
            }

            match self.handle.state() {
                TimerState::Idle | TimerState::Cancelled => {
                    self.completed = true;
                    std::task::Poll::Ready(())
                }
                _ => std::task::Poll::Pending,
            }
        }
    }

    let task = arena.reserve_task().expect("reserve task");
    let global = task.global_id(arena.tasks_per_leaf());
    arena.init_task(global);
    let future = FutureHelpers::box_future(ContendedTimer {
        handle: Arc::clone(&timer_handle),
        service_slot: Arc::clone(&service_slot),
        completed: false,
        scheduled: false,
    });
    let task_ref = task.task();
    task_ref.attach_future(future).unwrap();
    arena.activate_task(task);

    let canceller = {
        let handle = Arc::clone(&timer_handle);
        let service_slot = Arc::clone(&service_slot);
        let cancelled = Arc::clone(&cancelled);
        thread::spawn(move || {
            for _ in 0..50 {
                if let Some(service) = service_slot.lock().unwrap().as_ref().cloned() {
                    let _ = handle.cancel(&service);
                    cancelled.store(true, AtomicOrdering::Relaxed);
                    break;
                }
                thread::sleep(Duration::from_millis(1));
            }
        })
    };

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(1) {
        worker.run_once();
        if cancelled.load(AtomicOrdering::Relaxed)
            && matches!(timer_handle.state(), TimerState::Cancelled)
        {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    canceller.join().unwrap();

    assert!(cancelled.load(AtomicOrdering::Relaxed));
    assert_eq!(timer_handle.state(), TimerState::Cancelled);

    arena.release_task(task);
    service.shutdown();
}

#[test]
#[ignore]
fn timers_high_volume_soak() {
    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(4, 32).unwrap(), ArenaOptions::default())
            .unwrap(),
    );
    let service = TimerService::start(TimerConfig {
        tick_duration: Duration::from_micros(100),
    });
    let mut worker = Worker::new_with_timers(Arc::clone(&arena), Some(Arc::clone(&service)));

    let mut tasks = Vec::new();
    for idx in 0..256 {
        let task = arena.reserve_task().expect("reserve task");
        let global = task.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let future = FutureHelpers::box_future({
            let duration_ms = (idx % 5) as u64 + 1;
            async move {
                sleep::sleep(Duration::from_millis(duration_ms)).await;
            }
        });
        task.task().attach_future(future).unwrap();
        arena.activate_task(task);
        tasks.push(task);

        worker.run_once();
    }

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        worker.run_once();
        thread::sleep(Duration::from_millis(1));
    }

    for task in tasks.drain(..) {
        arena.release_task(task);
    }
    service.shutdown();
}
