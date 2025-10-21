use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::task::{Context, Poll};
use std::thread;
use std::time::{Duration, Instant};

use bop_executor::task::{ArenaConfig, ArenaOptions, FutureHelpers, MmapExecutorArena};
use bop_executor::timer::{TimerHandle, TimerState};
use bop_executor::worker::{Worker, schedule_timer_for_current_task};
use bop_executor::worker_service::{WorkerService, WorkerServiceConfig};

struct OneShotFuture {
    timer: Arc<TimerHandle>,
    scheduled: bool,
    completed: Arc<AtomicBool>,
}

impl Future for OneShotFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.scheduled {
            schedule_timer_for_current_task(cx, &self.timer, Duration::from_millis(5));
            self.scheduled = true;
            return Poll::Pending;
        }

        if self.timer.state() == TimerState::Idle {
            self.completed.store(true, AtomicOrdering::Relaxed);
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

struct IntervalFuture {
    timer: Arc<TimerHandle>,
    target: usize,
    fired: Arc<AtomicUsize>,
    scheduled: bool,
}

impl Future for IntervalFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.scheduled {
            self.timer
                .configure_interval(Some(Duration::from_millis(3)));
            schedule_timer_for_current_task(cx, &self.timer, Duration::from_millis(3));
            self.scheduled = true;
            return Poll::Pending;
        }

        if self.timer.state() == TimerState::Idle {
            let previous = self.fired.fetch_add(1, AtomicOrdering::Relaxed) + 1;
            if previous >= self.target {
                self.timer.cancel();
                return Poll::Ready(());
            }
            schedule_timer_for_current_task(cx, &self.timer, Duration::from_millis(3));
        }

        Poll::Pending
    }
}

struct CancelFuture {
    timer: Arc<TimerHandle>,
    scheduled: bool,
}

impl Future for CancelFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.scheduled {
            schedule_timer_for_current_task(cx, &self.timer, Duration::from_millis(5));
            self.scheduled = true;
            return Poll::Pending;
        }

        match self.timer.state() {
            TimerState::Cancelled => Poll::Ready(()),
            TimerState::Idle => Poll::Ready(()),
            _ => Poll::Pending,
        }
    }
}

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

    let timer = Arc::new(TimerHandle::new());
    let completed = Arc::new(AtomicBool::new(false));
    let future_ptr = FutureHelpers::box_future(OneShotFuture {
        timer: Arc::clone(&timer),
        scheduled: false,
        completed: Arc::clone(&completed),
    });

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
fn timer_interval_fires_multiple_times() {
    let arena = Arc::new(
        MmapExecutorArena::with_config(ArenaConfig::new(2, 8).unwrap(), ArenaOptions::default())
            .unwrap(),
    );
    let mut worker = make_worker(Arc::clone(&arena));

    let fired = Arc::new(AtomicUsize::new(0));
    let handle = arena.reserve_task().expect("reserve task");
    let global = handle.global_id(arena.tasks_per_leaf());
    arena.init_task(global);

    let timer = Arc::new(TimerHandle::new());
    let future_ptr = FutureHelpers::box_future(IntervalFuture {
        timer: Arc::clone(&timer),
        target: 2,
        fired: Arc::clone(&fired),
        scheduled: false,
    });

    let task = handle.task();
    task.attach_future(future_ptr).unwrap();
    arena.activate_task(handle);

    run_worker_until(&mut worker, &|| fired.load(AtomicOrdering::Relaxed) >= 2);

    assert!(fired.load(AtomicOrdering::Relaxed) >= 2);
    assert_eq!(worker.stats().completed_count, 1);

    arena.release_task(handle);
}

struct SustainedIntervalFuture {
    timer: Arc<TimerHandle>,
    interval: Duration,
    target: usize,
    fired: Arc<AtomicUsize>,
    completed: Arc<AtomicUsize>,
    scheduled: bool,
}

impl Future for SustainedIntervalFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.scheduled {
            self.timer.configure_interval(Some(self.interval));
            schedule_timer_for_current_task(cx, &self.timer, self.interval);
            self.scheduled = true;
            return Poll::Pending;
        }

        if self.timer.state() == TimerState::Idle {
            let count = self.fired.fetch_add(1, AtomicOrdering::Relaxed) + 1;
            if count >= self.target {
                self.timer.cancel();
                self.completed.fetch_add(1, AtomicOrdering::Relaxed);
                return Poll::Ready(());
            }
            schedule_timer_for_current_task(cx, &self.timer, self.interval);
        }

        Poll::Pending
    }
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

    let mut fire_counters = Vec::with_capacity(task_count);
    let mut handles = Vec::with_capacity(task_count);

    for leaf in 0..task_count {
        let handle = arena.reserve_task().expect("reserve task");
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);

        let timer = Arc::new(TimerHandle::new());
        let fired = Arc::new(AtomicUsize::new(0));

        let future_ptr = FutureHelpers::box_future(SustainedIntervalFuture {
            timer: Arc::clone(&timer),
            interval: Duration::from_millis(3),
            target: target_per_task,
            fired: Arc::clone(&fired),
            completed: Arc::clone(&completed),
            scheduled: false,
        });

        let task = handle.task();
        task.attach_future(future_ptr).unwrap();
        arena.activate_task(handle);

        fire_counters.push((timer, fired));
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
    for (timer, fired) in &fire_counters {
        assert!(
            fired.load(AtomicOrdering::Relaxed) >= target_per_task,
            "timer {:?} fired {} times",
            Arc::as_ptr(timer),
            fired.load(AtomicOrdering::Relaxed)
        );
        assert!(matches!(
            timer.state(),
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

    let timer = Arc::new(TimerHandle::new());
    let future_ptr = FutureHelpers::box_future(CancelFuture {
        timer: Arc::clone(&timer),
        scheduled: false,
    });

    let task = handle.task();
    task.attach_future(future_ptr).unwrap();
    arena.activate_task(handle);

    worker.run_once();
    timer.cancel();

    run_worker_until(&mut worker, &|| timer.state() == TimerState::Cancelled);

    assert!(matches!(
        timer.state(),
        TimerState::Cancelled | TimerState::Idle
    ));

    arena.release_task(handle);
}
