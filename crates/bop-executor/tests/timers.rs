use std::sync::Arc;
use std::thread;
use std::time::Duration;

use bop_executor::task::{ArenaConfig, ArenaOptions, FutureHelpers, MmapExecutorArena};
use bop_executor::timers::{TimerConfig, TimerService, sleep};
use bop_executor::worker::Worker;

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
