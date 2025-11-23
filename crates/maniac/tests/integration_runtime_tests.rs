use maniac::{
    future::block_on,
    runtime::{
        DefaultExecutor, Executor,
        task::{TaskArenaConfig, TaskArenaOptions},
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn create_executor(num_workers: usize) -> DefaultExecutor {
    DefaultExecutor::new(
        TaskArenaConfig::new(2, 4096).unwrap(),
        TaskArenaOptions::default(),
        num_workers,
        num_workers,
    )
    .unwrap()
}

#[test]
fn integration_single_worker_runtime_lifecycle() {
    let runtime = create_executor(1);

    let result = Arc::new(AtomicUsize::new(0));
    let mut joins = Vec::new();
    for i in 0..16 {
        let result_clone = Arc::clone(&result);
        let handle = runtime
            .spawn(async move {
                result_clone.fetch_add(i, Ordering::SeqCst);
                i
            })
            .expect("spawn");
        joins.push(handle);
    }

    let mut total = 0usize;
    for handle in joins {
        total += block_on(handle);
    }

    assert_eq!(total, result.load(Ordering::SeqCst));
}

#[test]
fn integration_multi_worker_runtime_throughput() {
    let runtime = create_executor(4);

    let counter = Arc::new(AtomicUsize::new(0));
    let mut joins = Vec::new();

    for _ in 0..128 {
        let counter_clone = Arc::clone(&counter);
        let handle = runtime
            .spawn(async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            })
            .expect("spawn");
        joins.push(handle);
    }

    for handle in joins {
        block_on(handle);
    }

    assert_eq!(
        counter.load(Ordering::SeqCst),
        128,
        "all tasks should complete across workers"
    );
}

#[test]
fn integration_runtime_mixed_spawn_patterns() {
    let runtime = create_executor(2);

    let results = Arc::new(Mutex::new(Vec::new()));
    let mut joins = Vec::new();

    for outer in 0..8 {
        let results_clone = Arc::clone(&results);
        let handle = runtime
            .spawn(async move {
                let mut inner_values = Vec::new();
                for inner in 0..4 {
                    inner_values.push(outer * 10 + inner);
                }
                results_clone.lock().unwrap().extend(inner_values);
            })
            .expect("spawn");
        joins.push(handle);
    }

    for handle in joins {
        block_on(handle);
    }

    let mut guard = results.lock().unwrap();
    guard.sort();
    assert_eq!(guard.len(), 32);
    let expected: Vec<_> = (0..8)
        .flat_map(|outer| (0..4).map(move |inner| outer * 10 + inner))
        .collect();
    assert_eq!(*guard, expected);
}

#[test]
fn integration_runtime_shutdown_under_load() {
    let runtime = create_executor(2);

    let counter = Arc::new(AtomicUsize::new(0));

    let mut joins = Vec::new();
    for _ in 0..32 {
        let counter_clone = Arc::clone(&counter);
        let handle = runtime
            .spawn(async move {
                // Simulate work by sleeping in a blocking section
                thread::sleep(Duration::from_millis(2));
                counter_clone.fetch_add(1, Ordering::SeqCst);
            })
            .expect("spawn");
        joins.push(handle);
    }

    for handle in joins {
        block_on(handle);
    }

    assert_eq!(
        counter.load(Ordering::SeqCst),
        32,
        "all work should complete before runtime drop"
    );
    // Runtime drops here, ensuring graceful shutdown under load.
}

#[test]
fn integration_runtime_randomized_load() {
    use rand::{Rng, SeedableRng, rngs::SmallRng};

    let runtime = create_executor(3);

    let counter = Arc::new(AtomicUsize::new(0));
    let mut joins = Vec::new();
    let mut rng = SmallRng::seed_from_u64(0xDEADBEEF);
    let mut expected_total = 0usize;

    for _ in 0..200 {
        let action = rng.random_range(0..3);
        let counter_clone = Arc::clone(&counter);
        let increments = match action {
            0 => 1,
            1 => 2,
            _ => 3,
        };
        expected_total += increments;
        let sleep_duration = if action == 1 {
            Some(Duration::from_millis(1))
        } else {
            None
        };
        let handle = runtime
            .spawn(async move {
                for _ in 0..increments {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                    if let Some(dur) = sleep_duration {
                        thread::sleep(dur);
                    }
                }
            })
            .expect("spawn");
        joins.push(handle);
    }

    for handle in joins {
        block_on(handle);
    }

    assert_eq!(counter.load(Ordering::SeqCst), expected_total);
}
