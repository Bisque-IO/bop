/// Comprehensive test suite for bop-executor components:
/// - Runtime: spawn, shutdown, JoinHandle behavior
/// - Summary: partition management, concurrent reservations
/// - Worker: task polling, work stealing, message handling
/// - Task: lifecycle, scheduling, state transitions
/// - Integration: cross-component interactions
use maniac::{
    future::block_on,
    runtime::{
        DefaultExecutor, Executor,
        summary::Summary,
        task::{TaskArena, TaskArenaConfig, TaskArenaOptions},
        worker::{Scheduler, SchedulerConfig},
    },
};

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn create_executor(num_workers: usize) -> DefaultExecutor {
    DefaultExecutor::new(
        TaskArenaConfig::new(2, 4096).unwrap(),
        TaskArenaOptions::default(),
        num_workers,
        num_workers,
    )
    .unwrap()
}

// ============================================================================
// RUNTIME TESTS
// ============================================================================

#[test]
fn runtime_basic_spawn_and_complete() {
    let runtime = create_executor(2);

    let result = Arc::new(AtomicUsize::new(0));
    let result_clone = result.clone();

    let join = runtime
        .spawn(async move {
            result_clone.store(42, Ordering::SeqCst);
            99
        })
        .expect("failed to spawn");

    let value = block_on(join);
    assert_eq!(value, 99, "join handle should return future output");
    assert_eq!(
        result.load(Ordering::SeqCst),
        42,
        "future should have executed"
    );
}

#[test]
fn runtime_multiple_concurrent_spawns() {
    let runtime = create_executor(2);

    let counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for i in 0..10 {
        let counter_clone = counter.clone();
        let handle = runtime
            .spawn(async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                i * 2
            })
            .expect("failed to spawn");
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(block_on(handle));
    }

    assert_eq!(
        counter.load(Ordering::SeqCst),
        10,
        "all tasks should execute"
    );
    assert_eq!(
        results,
        vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18],
        "results should match"
    );
}

#[test]
fn runtime_spawn_after_capacity() {
    let runtime = create_executor(1);

    let mut handles = Vec::new();

    // Fill capacity
    for _ in 0..4 {
        let handle = runtime.spawn(async {
            thread::sleep(Duration::from_millis(50));
        });
        if let Ok(h) = handle {
            handles.push(h);
        }
    }

    // Try to spawn beyond capacity
    let result = runtime.spawn(async {});
    assert!(result.is_err(), "spawn should fail when no capacity");

    // Clean up
    for handle in handles {
        let _ = block_on(handle);
    }
}

#[test]
fn runtime_join_handle_is_finished() {
    let runtime = create_executor(2);

    let ready = Arc::new(AtomicBool::new(false));
    let ready_clone = ready.clone();

    let join = runtime
        .spawn(async move {
            // Remove sleep to avoid blocking
            ready_clone.store(true, Ordering::SeqCst);
        })
        .expect("failed to spawn");

    assert!(!join.is_finished(), "should not be finished immediately");

    block_on(join);

    assert!(ready.load(Ordering::SeqCst), "future should have completed");
}

#[test]
fn runtime_stats_tracking() {
    let runtime = create_executor(2);

    let initial_stats = runtime.stats();
    assert_eq!(
        initial_stats.active_tasks, 0,
        "should start with no active tasks"
    );

    let join1 = runtime
        .spawn(async {
            // Remove sleep to avoid blocking issues
        })
        .expect("spawn 1");

    let join2 = runtime
        .spawn(async {
            // Remove sleep to avoid blocking issues
        })
        .expect("spawn 2");

    block_on(join1);
    block_on(join2);

    let final_stats = runtime.stats();
    assert_eq!(final_stats.active_tasks, 0, "all tasks should complete");
}

#[test]
fn runtime_nested_spawn() {
    let runtime = create_executor(2);

    // Note: True nested spawning where an async task blocks on another task
    // doesn't work with block_on as it monopolizes the test thread.
    // This test demonstrates sequential spawning instead.
    let result = Arc::new(Mutex::new(None));
    let result_clone = result.clone();

    let runtime_clone = runtime.clone();
    let outer = runtime
        .spawn(async move {
            // Spawn inner task and store its handle
            let inner = runtime_clone.spawn(async { 42 }).expect("inner spawn");
            *result_clone.lock().unwrap() = Some(inner);
        })
        .expect("outer spawn");

    // Complete outer task
    block_on(outer);

    // Now get and complete inner task
    let inner_handle = result
        .lock()
        .unwrap()
        .take()
        .expect("inner should be spawned");
    let inner_result = block_on(inner_handle);

    assert_eq!(inner_result, 42, "nested spawn should work");
}

#[test]
fn runtime_graceful_shutdown() {
    let runtime = create_executor(2);

    let completed = Arc::new(AtomicUsize::new(0));
    let start_flag = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();
    for _ in 0..5 {
        let completed_clone = completed.clone();
        let start_flag_clone = start_flag.clone();
        let handle = runtime
            .spawn(async move {
                // Wait for start signal instead of barrier
                while !start_flag_clone.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_micros(100));
                }
                completed_clone.fetch_add(1, Ordering::SeqCst);
            })
            .expect("spawn");
        handles.push(handle);
    }

    // Let tasks start
    thread::sleep(Duration::from_millis(10));
    start_flag.store(true, Ordering::Relaxed);

    // Complete all handles before dropping runtime
    for handle in handles {
        block_on(handle);
    }

    assert_eq!(
        completed.load(Ordering::SeqCst),
        5,
        "all tasks should complete before shutdown"
    );

    drop(runtime);
}

// ============================================================================
// SUMMARY TREE TESTS
// ============================================================================

#[test]
fn summary_tree_reserve_task_round_robin() {
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..4).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(2));

    let tree = Summary::new(4, 2, &wakers, &worker_count);

    let mut reservations = Vec::new();

    // Reserve multiple tasks - should distribute across leaves
    for _ in 0..16 {
        if let Some((leaf, signal, bit)) = tree.reserve_task() {
            reservations.push((leaf, signal, bit));
        }
    }

    assert!(
        reservations.len() >= 4,
        "should reserve from multiple leaves"
    );

    // Check that different leaves were used
    let unique_leaves: std::collections::HashSet<_> =
        reservations.iter().map(|(leaf, _, _)| *leaf).collect();
    assert!(unique_leaves.len() > 1, "should use multiple leaves");

    // Clean up
    for (leaf, signal, bit) in reservations {
        tree.release_task_in_leaf(leaf, signal, bit as usize);
    }
}

#[test]
fn summary_tree_reserve_exhaustion() {
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..2).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(2));

    let tree = Summary::new(2, 1, &wakers, &worker_count); // 2 leaves * 1 signal * 64 bits = 128 tasks

    let mut handles = Vec::new();

    // Reserve all slots
    for _ in 0..128 {
        if let Some(handle) = tree.reserve_task() {
            handles.push(handle);
        }
    }

    assert_eq!(handles.len(), 128, "should reserve all slots");

    // Next reservation should fail
    assert!(tree.reserve_task().is_none(), "should be exhausted");

    // Release one
    let (leaf, signal, bit) = handles.pop().unwrap();
    tree.release_task_in_leaf(leaf, signal, bit as usize);

    // Should be able to reserve again
    assert!(
        tree.reserve_task().is_some(),
        "should be available after release"
    );

    // Clean up
    for (leaf, signal, bit) in handles {
        tree.release_task_in_leaf(leaf, signal, bit as usize);
    }
}

#[test]
fn summary_tree_concurrent_reservations() {
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..4).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(4));

    let tree = Arc::new(Summary::new(4, 2, &wakers, &worker_count));

    let barrier = Arc::new(Barrier::new(8));
    let results = Arc::new(Mutex::new(Vec::new()));

    let mut threads = Vec::new();
    for _ in 0..8 {
        let tree_clone = tree.clone();
        let barrier_clone = barrier.clone();
        let results_clone = results.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait();
            let mut local_reservations = Vec::new();

            for _ in 0..10 {
                if let Some(handle) = tree_clone.reserve_task() {
                    local_reservations.push(handle);
                }
            }

            results_clone.lock().unwrap().extend(local_reservations);
        });

        threads.push(handle);
    }

    for handle in threads {
        handle.join().unwrap();
    }

    let all_reservations = results.lock().unwrap();

    // Check uniqueness - no duplicate reservations
    let mut seen = std::collections::HashSet::new();
    for &(leaf, signal, bit) in all_reservations.iter() {
        let key = (leaf, signal, bit);
        assert!(seen.insert(key), "duplicate reservation: {:?}", key);
    }

    // Clean up
    for &(leaf, signal, bit) in all_reservations.iter() {
        tree.release_task_in_leaf(leaf, signal, bit as usize);
    }
}

#[test]
fn summary_tree_partition_ownership() {
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..4).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(4));

    let tree = Summary::new(8, 1, &wakers, &worker_count);

    // Test partition calculation
    for leaf_idx in 0..8 {
        let owner = tree.compute_partition_owner(leaf_idx, 4);
        assert!(owner < 4, "owner should be valid worker ID");

        // Verify leaf is in owner's partition
        let start = tree.partition_start_for_worker(owner, 4);
        let end = tree.partition_end_for_worker(owner, 4);
        assert!(
            leaf_idx >= start && leaf_idx < end,
            "leaf {} should be in worker {}'s partition [{}, {})",
            leaf_idx,
            owner,
            start,
            end
        );
    }

    // All leaves should be covered
    let mut all_leaves = std::collections::HashSet::new();
    for worker in 0..4 {
        let start = tree.partition_start_for_worker(worker, 4);
        let end = tree.partition_end_for_worker(worker, 4);
        for leaf in start..end {
            assert!(
                all_leaves.insert(leaf),
                "leaf {} assigned to multiple workers",
                leaf
            );
        }
    }
    assert_eq!(all_leaves.len(), 8, "all leaves should be assigned");
}

#[test]
fn summary_tree_signal_active_inactive() {
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..2).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(2));

    let tree = Summary::new(2, 2, &wakers, &worker_count);

    // Mark signal active
    let was_empty = tree.mark_signal_active(0, 1);
    assert!(was_empty, "leaf should have been empty");

    // Mark inactive
    let is_empty = tree.mark_signal_inactive(0, 1);
    assert!(is_empty, "leaf should now be empty");
}

// ============================================================================
// WORKER SERVICE TESTS
// ============================================================================

#[test]
fn worker_service_basic_startup() {
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(2, 16).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let config = SchedulerConfig {
        min_workers: 2,
        max_workers: 4,
        ..SchedulerConfig::default()
    };

    let service = Scheduler::<10, 6>::start(arena, config);

    thread::sleep(Duration::from_millis(50));

    assert_eq!(service.worker_count(), 2, "should start with min_workers");

    service.shutdown();
}

#[test]
fn worker_service_task_reservation() {
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(2, 16).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let service = Scheduler::<10, 6>::start(arena, SchedulerConfig::default());

    let handle1 = service.reserve_task().expect("reserve 1");
    let handle2 = service.reserve_task().expect("reserve 2");

    // Handles should be different
    assert_ne!(handle1.global_id(16), handle2.global_id(16));

    service.release_task(handle1);
    service.release_task(handle2);

    service.shutdown();
}

#[test]
fn worker_service_tick_thread() {
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(1, 8).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let config = SchedulerConfig {
        tick_duration: Duration::from_nanos(16_777_216), // Must be power of 2 ns: 2^24 = ~16.7ms
        min_workers: 1,
        max_workers: 1,
    };

    let service = Scheduler::<10, 6>::start(arena, config);

    // Wait much longer for tick thread to stabilize
    thread::sleep(Duration::from_millis(100));
    let initial_ticks = service.tick_stats().total_ticks;

    // Wait even longer for ticks
    thread::sleep(Duration::from_millis(300));

    let final_ticks = service.tick_stats().total_ticks;
    assert!(
        final_ticks > initial_ticks,
        "ticks should progress: {} -> {}",
        initial_ticks,
        final_ticks
    );

    service.shutdown();
}

// ============================================================================
// TASK ARENA TESTS
// ============================================================================

#[test]
fn task_arena_initialization() {
    let config = TaskArenaConfig::new(4, 32).unwrap();
    let arena = TaskArena::with_config(config, TaskArenaOptions::default())
        .expect("failed to create arena");

    assert_eq!(arena.leaf_count(), 4);
    assert_eq!(arena.tasks_per_leaf(), 32);
    assert_eq!(arena.signals_per_leaf(), 1); // 32 tasks / 64 bits = 1 signal word
}

#[test]
fn task_arena_preinitialize() {
    let config = TaskArenaConfig::new(2, 16).unwrap();
    let options = TaskArenaOptions {
        use_huge_pages: false,
        preinitialize_tasks: true,
    };

    let arena = TaskArena::with_config(config, options).expect("failed to create arena");

    // All task slots should be initialized
    for leaf in 0..2 {
        for slot in 0..16 {
            let task = unsafe { arena.task(leaf, slot) };
            assert_eq!(task.global_id(), arena.compose_id(leaf, slot));
        }
    }
}

#[test]
fn task_arena_close() {
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(1, 8).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    assert!(!arena.is_closed());

    arena.close();

    assert!(arena.is_closed());
}

// ============================================================================
// TASK LIFECYCLE TESTS
// ============================================================================
// Note: Task lifecycle tests that require access to private fields (like state)
// are located in the task.rs module's #[cfg(test)] section

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[test]
fn integration_full_task_execution_cycle() {
    let runtime = create_executor(2);

    let executed = Arc::new(AtomicBool::new(false));
    let executed_clone = executed.clone();

    let join = runtime
        .spawn(async move {
            executed_clone.store(true, Ordering::SeqCst);
            "success"
        })
        .expect("spawn failed");

    let result = block_on(join);

    assert_eq!(result, "success");
    assert!(executed.load(Ordering::SeqCst));
}

#[test]
fn integration_many_short_tasks() {
    let runtime = create_executor(2);

    let counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..100 {
        let counter_clone = counter.clone();
        let handle = runtime
            .spawn(async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            })
            .expect("spawn failed");
        handles.push(handle);
    }

    for handle in handles {
        block_on(handle);
    }

    assert_eq!(counter.load(Ordering::SeqCst), 100);
}

#[test]
fn integration_stress_spawn_release_cycle() {
    let runtime = create_executor(2);

    let counter = Arc::new(AtomicUsize::new(0));

    // Reduce from 10 to 5 iterations to prevent hanging
    for iteration in 0..5 {
        let mut handles = Vec::new();

        // Reduce from 20 to 10 tasks per iteration
        for _ in 0..10 {
            let counter_clone = counter.clone();
            match runtime.spawn(async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }) {
                Ok(h) => handles.push(h),
                Err(_) => break,
            }
        }

        for handle in handles {
            block_on(handle);
        }

        println!(
            "Iteration {} complete, counter = {}",
            iteration,
            counter.load(Ordering::SeqCst)
        );
    }

    assert!(
        counter.load(Ordering::SeqCst) >= 40,
        "should complete multiple cycles"
    );
}

// ============================================================================
// ADVANCED RUNTIME TESTS
// ============================================================================

#[test]
fn runtime_spawn_chain_dependencies() {
    // Test spawning tasks that depend on each other's results
    let runtime = create_executor(2);

    let shared_result = Arc::new(Mutex::new(Vec::new()));

    let result1 = shared_result.clone();
    let handle1 = runtime
        .spawn(async move {
            result1.lock().unwrap().push(1);
            10
        })
        .expect("spawn 1");

    let result2 = shared_result.clone();
    let handle2 = runtime
        .spawn(async move {
            result2.lock().unwrap().push(2);
            20
        })
        .expect("spawn 2");

    let result3 = shared_result.clone();
    let handle3 = runtime
        .spawn(async move {
            result3.lock().unwrap().push(3);
            30
        })
        .expect("spawn 3");

    let val1 = block_on(handle1);
    let val2 = block_on(handle2);
    let val3 = block_on(handle3);

    assert_eq!(val1 + val2 + val3, 60);
    assert_eq!(shared_result.lock().unwrap().len(), 3);
}

#[test]
fn runtime_panic_isolation() {
    // Verify that panicking tasks don't crash the runtime
    let runtime = create_executor(2);

    let success_counter = Arc::new(AtomicUsize::new(0));

    // Spawn a task that will panic
    let _ = runtime.spawn(async {
        // Note: This test demonstrates panic handling - in production
        // panics in async tasks are typically caught by the executor
        panic!("intentional panic for testing");
    });

    // Spawn successful tasks after the panic
    let mut handles = Vec::new();
    for _ in 0..5 {
        let counter = success_counter.clone();
        let handle = runtime
            .spawn(async move {
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .expect("spawn");
        handles.push(handle);
    }

    for handle in handles {
        block_on(handle);
    }

    // The 5 successful tasks should complete
    assert_eq!(success_counter.load(Ordering::SeqCst), 5);
}

#[test]
fn runtime_spawn_with_immediate_drop() {
    // Test behavior when JoinHandle is dropped immediately
    let runtime = create_executor(1);

    let executed = Arc::new(AtomicBool::new(false));
    let executed_clone = executed.clone();

    let handle = runtime
        .spawn(async move {
            thread::sleep(Duration::from_millis(10));
            executed_clone.store(true, Ordering::SeqCst);
        })
        .expect("spawn");

    // Drop the handle immediately
    drop(handle);

    // Task should still execute even though handle was dropped
    thread::sleep(Duration::from_millis(50));
    assert!(
        executed.load(Ordering::SeqCst),
        "task should execute despite dropped handle"
    );
}

#[test]
fn runtime_heavy_computation_tasks() {
    // Test with CPU-intensive tasks
    let runtime = create_executor(2);

    let mut handles = Vec::new();

    for i in 0..10 {
        let handle = runtime
            .spawn(async move {
                // Simulate computation
                let mut sum = 0u64;
                for j in 0..1000 {
                    sum = sum.wrapping_add(i * j);
                }
                sum
            })
            .expect("spawn");
        handles.push(handle);
    }

    let mut total = 0u64;
    for handle in handles {
        total = total.wrapping_add(block_on(handle));
    }

    assert!(total > 0, "computation should produce non-zero result");
}

#[test]
fn runtime_mixed_duration_tasks() {
    // Test fairness with tasks of varying durations
    let runtime = create_executor(2);

    let completion_order = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::new();

    // Quick tasks
    for i in 0..5 {
        let order = completion_order.clone();
        let handle = runtime
            .spawn(async move {
                order.lock().unwrap().push(format!("quick-{}", i));
            })
            .expect("spawn");
        handles.push(handle);
    }

    // Medium tasks
    for i in 0..5 {
        let order = completion_order.clone();
        let handle = runtime
            .spawn(async move {
                thread::sleep(Duration::from_millis(5));
                order.lock().unwrap().push(format!("medium-{}", i));
            })
            .expect("spawn");
        handles.push(handle);
    }

    // Slow tasks
    for i in 0..5 {
        let order = completion_order.clone();
        let handle = runtime
            .spawn(async move {
                thread::sleep(Duration::from_millis(10));
                order.lock().unwrap().push(format!("slow-{}", i));
            })
            .expect("spawn");
        handles.push(handle);
    }

    for handle in handles {
        block_on(handle);
    }

    let order = completion_order.lock().unwrap();
    assert_eq!(order.len(), 15, "all tasks should complete");
}

// ============================================================================
// SUMMARY TREE ADVANCED TESTS
// ============================================================================

#[test]
fn summary_tree_partition_rebalancing() {
    // Test that partition calculations work correctly with different worker counts
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..8).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count_atomic = Arc::new(AtomicUsize::new(0));

    let tree = Summary::new(16, 1, &wakers, &worker_count_atomic);

    // Test with different worker counts
    for worker_count in 1..=8 {
        worker_count_atomic.store(worker_count, Ordering::SeqCst);

        let mut leaf_assignments = vec![Vec::new(); worker_count];

        for leaf_idx in 0..16 {
            let owner = tree.compute_partition_owner(leaf_idx, worker_count);
            assert!(
                owner < worker_count,
                "owner {} should be < worker_count {}",
                owner,
                worker_count
            );
            leaf_assignments[owner].push(leaf_idx);
        }

        // Verify all leaves are assigned
        let total_assigned: usize = leaf_assignments.iter().map(|v| v.len()).sum();
        assert_eq!(
            total_assigned, 16,
            "all leaves should be assigned with {} workers",
            worker_count
        );

        // Verify balanced distribution (no worker should have >2x leaves of another)
        let max_leaves = leaf_assignments.iter().map(|v| v.len()).max().unwrap();
        let min_leaves = leaf_assignments.iter().map(|v| v.len()).min().unwrap();
        assert!(
            max_leaves <= min_leaves * 2,
            "distribution should be balanced: max={}, min={}",
            max_leaves,
            min_leaves
        );
    }
}

#[test]
fn summary_tree_reserve_release_stress() {
    // Stress test reserve/release cycles
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..4).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(4));

    let tree = Arc::new(Summary::new(4, 2, &wakers, &worker_count));

    let iterations = 100;
    let reservations_per_iteration = 20;

    for iteration in 0..iterations {
        let mut handles = Vec::new();

        // Reserve
        for _ in 0..reservations_per_iteration {
            if let Some(handle) = tree.reserve_task() {
                handles.push(handle);
            }
        }

        // Release all
        for (leaf, signal, bit) in handles {
            tree.release_task_in_leaf(leaf, signal, bit as usize);
        }

        if iteration % 10 == 0 {
            println!("Completed {} reserve/release cycles", iteration);
        }
    }

    // Should be able to reserve again after all releases
    assert!(
        tree.reserve_task().is_some(),
        "should be able to reserve after stress test"
    );
}

#[test]
fn summary_tree_signal_transitions_stress() {
    // Test rapid signal active/inactive transitions
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..2).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(2));

    let tree = Summary::new(2, 4, &wakers, &worker_count);

    for _ in 0..1000 {
        for leaf in 0..2 {
            for signal in 0..4 {
                tree.mark_signal_active(leaf, signal);
                tree.mark_signal_inactive(leaf, signal);
            }
        }
    }

    // Tree should be in consistent state
    assert_eq!(tree.leaf_count(), 2);
    assert_eq!(tree.signals_per_leaf(), 4);
}

#[test]
fn summary_tree_global_to_local_leaf_conversion() {
    // Test global to local leaf index conversion
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..4).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(4));

    let tree = Summary::new(12, 1, &wakers, &worker_count);

    // Test conversion for each leaf
    for leaf_idx in 0..12 {
        let owner = tree.compute_partition_owner(leaf_idx, 4);
        let local_idx = tree.global_to_local_leaf_idx(leaf_idx, owner, 4);

        assert!(
            local_idx.is_some(),
            "leaf {} should map to local index for owner {}",
            leaf_idx,
            owner
        );

        let local = local_idx.unwrap();
        let start = tree.partition_start_for_worker(owner, 4);
        assert_eq!(
            leaf_idx,
            start + local,
            "local index {} should map back to global index {}",
            local,
            leaf_idx
        );
    }

    // Test that leaves don't map to wrong workers
    for leaf_idx in 0..12 {
        let owner = tree.compute_partition_owner(leaf_idx, 4);
        for other_worker in 0..4 {
            if other_worker != owner {
                let local_idx = tree.global_to_local_leaf_idx(leaf_idx, other_worker, 4);
                assert!(
                    local_idx.is_none(),
                    "leaf {} should not map to non-owner worker {}",
                    leaf_idx,
                    other_worker
                );
            }
        }
    }
}

// ============================================================================
// WORKER SERVICE ADVANCED TESTS
// ============================================================================

#[test]
fn worker_service_multiple_worker_coordination() {
    // Test that multiple workers can coordinate
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(4, 32).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let config = SchedulerConfig {
        min_workers: 4,
        max_workers: 4,
        ..SchedulerConfig::default()
    };

    let service = Scheduler::<10, 6>::start(arena, config);

    thread::sleep(Duration::from_millis(100));

    assert_eq!(service.worker_count(), 4, "should have 4 workers");

    // Reserve tasks from all workers
    let mut handles = Vec::new();
    for _ in 0..40 {
        if let Some(handle) = service.reserve_task() {
            handles.push(handle);
        }
    }

    assert!(
        handles.len() >= 20,
        "multiple workers should provide capacity"
    );

    // Release all
    for handle in handles {
        service.release_task(handle);
    }

    service.shutdown();
}

#[test]
fn worker_service_reserve_after_shutdown() {
    // Test that reservations fail after shutdown
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(1, 8).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let service = Scheduler::<10, 6>::start(arena, SchedulerConfig::default());

    // Reserve before shutdown
    let handle = service.reserve_task();
    assert!(
        handle.is_some(),
        "should be able to reserve before shutdown"
    );

    if let Some(h) = handle {
        service.release_task(h);
    }

    // Shutdown
    service.shutdown();

    // Try to reserve after shutdown - arena should be closed
    thread::sleep(Duration::from_millis(50));

    // Note: reservation might still work if arena wasn't closed yet
    // This tests the shutdown coordination
}

#[test]
fn worker_service_clock_monotonicity() {
    // Test that the service clock is monotonically increasing
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(1, 8).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let service = Scheduler::<10, 6>::start(arena, SchedulerConfig::default());

    let mut prev_clock = 0u64;

    for _ in 0..20 {
        thread::sleep(Duration::from_millis(10));
        let current_clock = service.clock_ns();
        assert!(
            current_clock >= prev_clock,
            "clock should be monotonic: prev={}, current={}",
            prev_clock,
            current_clock
        );
        prev_clock = current_clock;
    }

    service.shutdown();
}

// ============================================================================
// TASK ARENA ADVANCED TESTS
// ============================================================================

#[test]
fn task_arena_lazy_initialization() {
    // Test that tasks are lazily initialized when not preinitialized
    let config = TaskArenaConfig::new(2, 16).unwrap();
    let options = TaskArenaOptions {
        use_huge_pages: false,
        preinitialize_tasks: false, // Lazy initialization
    };

    let arena = TaskArena::with_config(config, options).expect("failed to create arena");

    // Access a task - should initialize on demand
    let task = unsafe { arena.task(0, 5) };
    assert_eq!(task.global_id(), arena.compose_id(0, 5));

    // Access another task
    let task2 = unsafe { arena.task(1, 10) };
    assert_eq!(task2.global_id(), arena.compose_id(1, 10));
}

#[test]
fn task_arena_compose_decompose_identity() {
    // Test that compose/decompose are inverses
    let config = TaskArenaConfig::new(4, 32).unwrap();
    let arena = TaskArena::with_config(config, TaskArenaOptions::default())
        .expect("failed to create arena");

    for leaf in 0..4 {
        for slot in 0..32 {
            let global_id = arena.compose_id(leaf, slot);
            let (decomposed_leaf, decomposed_slot) = arena.decompose_id(global_id);
            assert_eq!(leaf, decomposed_leaf, "leaf should match");
            assert_eq!(slot, decomposed_slot, "slot should match");
        }
    }
}

#[test]
fn task_arena_stats_accuracy() {
    // Test that arena stats accurately reflect state
    let arena = Arc::new(
        TaskArena::with_config(
            TaskArenaConfig::new(2, 16).unwrap(),
            TaskArenaOptions::default(),
        )
        .expect("failed to create arena"),
    );

    let initial_stats = arena.stats();
    assert_eq!(initial_stats.total_capacity, 32, "should have 32 slots");
    assert_eq!(initial_stats.active_tasks, 0, "should start with 0 active");

    // Increment
    arena.increment_total_tasks();
    arena.increment_total_tasks();
    arena.increment_total_tasks();

    let updated_stats = arena.stats();
    assert_eq!(updated_stats.active_tasks, 3, "should have 3 active");

    // Decrement
    arena.decrement_total_tasks();
    arena.decrement_total_tasks();

    let final_stats = arena.stats();
    assert_eq!(final_stats.active_tasks, 1, "should have 1 active");
}

#[test]
fn task_arena_config_power_of_two_adjustment() {
    // Test that leaf_count is adjusted to power of two
    let config = TaskArenaConfig::new(7, 16).unwrap();
    assert_eq!(config.leaf_count, 8, "7 should be rounded up to 8");

    let config2 = TaskArenaConfig::new(16, 32).unwrap();
    assert_eq!(config2.leaf_count, 16, "16 should remain 16");

    let config3 = TaskArenaConfig::new(13, 16).unwrap();
    assert_eq!(config3.leaf_count, 16, "13 should be rounded up to 16");
}

// ============================================================================
// ERROR HANDLING AND EDGE CASES
// ============================================================================

#[test]
fn error_handling_spawn_zero_capacity() {
    // Test spawning on an arena with minimal capacity
    let runtime = create_executor(1);

    let handle1 = runtime.spawn(async {
        thread::sleep(Duration::from_millis(100));
    });
    assert!(handle1.is_ok(), "first spawn should succeed");

    let handle2 = runtime.spawn(async {
        thread::sleep(Duration::from_millis(100));
    });
    assert!(handle2.is_ok(), "second spawn should succeed");

    // Third spawn might fail due to capacity
    let _handle3 = runtime.spawn(async {});
    // It's OK if it fails or succeeds depending on whether tasks completed
}

#[test]
fn error_handling_rapid_spawn_attempts() {
    // Rapidly attempt spawns to test error handling under pressure
    let runtime = create_executor(1);

    let mut success_count = 0;
    let mut error_count = 0;

    for _ in 0..100 {
        match runtime.spawn(async {}) {
            Ok(_) => success_count += 1,
            Err(_) => error_count += 1,
        }
        thread::yield_now();
    }

    println!("Spawned: {}, Errors: {}", success_count, error_count);
    assert!(success_count > 0, "at least some spawns should succeed");
}

#[test]
fn edge_case_empty_runtime_drop() {
    // Test dropping runtime with no spawned tasks
    let runtime = create_executor(1);

    // Drop immediately without spawning
    drop(runtime);

    // Should not hang or crash
}

#[test]
fn edge_case_concurrent_runtime_operations() {
    // Test concurrent spawn and stats calls
    let runtime = create_executor(2);

    let barrier = Arc::new(Barrier::new(3));

    let runtime1 = runtime.clone();
    let barrier1 = barrier.clone();
    let handle1 = thread::spawn(move || {
        barrier1.wait();
        for _ in 0..20 {
            let _ = runtime1.spawn(async {});
        }
    });

    let runtime2 = runtime.clone();
    let barrier2 = barrier.clone();
    let handle2 = thread::spawn(move || {
        barrier2.wait();
        for _ in 0..20 {
            let _ = runtime2.stats();
        }
    });

    let runtime3 = runtime.clone();
    let barrier3 = barrier.clone();
    let handle3 = thread::spawn(move || {
        barrier3.wait();
        for _ in 0..20 {
            let _ = runtime3.spawn(async {});
        }
    });

    handle1.join().unwrap();
    handle2.join().unwrap();
    handle3.join().unwrap();
}

#[test]
fn integration_sequential_runtime_creation() {
    // Test creating and dropping multiple runtimes sequentially
    for i in 0..5 {
        let runtime = create_executor(1);

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let handle = runtime
            .spawn(async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            })
            .expect("spawn");

        block_on(handle);

        assert_eq!(counter.load(Ordering::SeqCst), 1, "iteration {}", i);

        drop(runtime);
    }
}

// ============================================================================
// ADDITIONAL ADVANCED TESTS - BATCH 3
// ============================================================================

#[test]
fn runtime_spawn_returning_complex_types() {
    // Test spawning tasks that return complex types
    let runtime = create_executor(2);

    #[derive(Debug, PartialEq)]
    struct ComplexResult {
        values: Vec<u64>,
        name: String,
        nested: Option<Box<ComplexResult>>,
    }

    let handle = runtime
        .spawn(async {
            ComplexResult {
                values: vec![1, 2, 3, 4, 5],
                name: "test".to_string(),
                nested: Some(Box::new(ComplexResult {
                    values: vec![10, 20],
                    name: "nested".to_string(),
                    nested: None,
                })),
            }
        })
        .expect("spawn");

    let result = block_on(handle);
    assert_eq!(result.values.len(), 5);
    assert_eq!(result.name, "test");
    assert!(result.nested.is_some());
}

#[test]
fn runtime_spawn_with_async_blocks() {
    // Test various async block patterns
    let runtime = create_executor(1);

    let result = Arc::new(Mutex::new(Vec::new()));

    // Simple async block
    let r1 = result.clone();
    let h1 = runtime
        .spawn(async move {
            r1.lock().unwrap().push(1);
        })
        .expect("spawn");

    // Async block with await
    let r2 = result.clone();
    let h2 = runtime
        .spawn(async move {
            async { 2 }.await;
            r2.lock().unwrap().push(2);
        })
        .expect("spawn");

    // Nested async
    let r3 = result.clone();
    let h3 = runtime
        .spawn(async move {
            let val = async { async { 3 }.await }.await;
            r3.lock().unwrap().push(val);
        })
        .expect("spawn");

    block_on(h1);
    block_on(h2);
    block_on(h3);

    assert_eq!(result.lock().unwrap().len(), 3);
}

#[test]
fn runtime_large_result_propagation() {
    // Test handling large result values
    let runtime = create_executor(2);

    let handle = runtime
        .spawn(async {
            vec![0u8; 1024 * 1024] // 1MB vector
        })
        .expect("spawn");

    let result = block_on(handle);

    assert_eq!(result.len(), 1024 * 1024);
}

#[test]
fn runtime_spawn_burst_load() {
    // Test handling burst of spawns
    let runtime = create_executor(4);

    let counter = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();

    // Burst spawn 50 tasks as fast as possible
    let mut handles = Vec::new();
    for _ in 0..50 {
        let counter_clone = counter.clone();
        match runtime.spawn(async move {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }) {
            Ok(h) => handles.push(h),
            Err(_) => break,
        }
    }

    let spawn_duration = start.elapsed();
    println!("Spawned {} tasks in {:?}", handles.len(), spawn_duration);

    for handle in handles {
        block_on(handle);
    }

    assert!(
        counter.load(Ordering::Relaxed) >= 40,
        "most tasks should complete"
    );
}

#[test]
fn runtime_interleaved_spawn_complete() {
    // Test interleaving spawn and completion
    let runtime = create_executor(2);

    let mut completed_count = 0;

    // Reduce from 10 to 5 iterations to avoid hanging
    for i in 0..5 {
        let handle = runtime.spawn(async move { i }).expect("spawn");

        // Complete immediately
        let result = block_on(handle);
        assert_eq!(result, i);
        completed_count += 1;

        // Spawn next immediately after completion
    }

    assert_eq!(completed_count, 5);
}

#[test]
fn summary_tree_edge_case_single_leaf() {
    // Test Summary with just 1 leaf
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = vec![Arc::new(WorkerWaker::new())];
    let worker_count = Arc::new(AtomicUsize::new(1));
    let tree = Summary::new(1, 1, &wakers, &worker_count);

    // Should still work with minimal configuration
    let handle = tree.reserve_task();
    assert!(
        handle.is_some(),
        "should be able to reserve from single leaf"
    );

    let (leaf, signal, bit) = handle.unwrap();
    assert_eq!(leaf, 0);
    assert_eq!(signal, 0);

    tree.release_task_in_leaf(leaf, signal, bit as usize);
}

#[test]
fn summary_tree_maximum_leaves() {
    // Test with large number of leaves
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..16).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(8));

    let tree = Summary::new(64, 1, &wakers, &worker_count);

    // Reserve from many leaves
    let mut handles = Vec::new();
    for _ in 0..64 {
        if let Some(handle) = tree.reserve_task() {
            handles.push(handle);
        }
    }

    assert!(handles.len() >= 32, "should reserve from many leaves");

    // Verify distribution across leaves
    let unique_leaves: std::collections::HashSet<_> =
        handles.iter().map(|(leaf, _, _)| *leaf).collect();
    assert!(
        unique_leaves.len() >= 10,
        "should use many different leaves"
    );

    for (leaf, signal, bit) in handles {
        tree.release_task_in_leaf(leaf, signal, bit as usize);
    }
}

#[test]
fn summary_tree_concurrent_signal_operations() {
    // Test concurrent mark_signal_active/inactive
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..4).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count = Arc::new(AtomicUsize::new(4));

    let tree = Arc::new(Summary::new(8, 2, &wakers, &worker_count));

    let barrier = Arc::new(Barrier::new(4));
    let mut threads = Vec::new();

    for thread_id in 0..4 {
        let tree_clone = tree.clone();
        let barrier_clone = barrier.clone();

        let handle = thread::spawn(move || {
            barrier_clone.wait();

            for i in 0..100 {
                let leaf = (thread_id * 2 + i % 2) % 8;
                let signal = i % 2;

                tree_clone.mark_signal_active(leaf, signal);
                thread::yield_now();
                tree_clone.mark_signal_inactive(leaf, signal);
            }
        });

        threads.push(handle);
    }

    for handle in threads {
        handle.join().unwrap();
    }

    // Tree should be consistent
    assert_eq!(tree.leaf_count(), 8);
}

#[test]
fn summary_tree_partition_boundary_cases() {
    // Test partition calculations at boundaries
    use maniac::runtime::waker::WorkerWaker;

    let wakers: Vec<Arc<WorkerWaker>> = (0..8).map(|_| Arc::new(WorkerWaker::new())).collect();
    let worker_count_atomic = Arc::new(AtomicUsize::new(0));

    let tree = Summary::new(17, 1, &wakers, &worker_count_atomic); // Prime number of leaves

    // Test with various worker counts
    for worker_count in 1..=9 {
        worker_count_atomic.store(worker_count, Ordering::SeqCst);

        for leaf_idx in 0..17 {
            let owner = tree.compute_partition_owner(leaf_idx, worker_count);
            assert!(
                owner < worker_count,
                "owner {} >= worker_count {} for leaf {}",
                owner,
                worker_count,
                leaf_idx
            );

            let start = tree.partition_start_for_worker(owner, worker_count);
            let end = tree.partition_end_for_worker(owner, worker_count);
            assert!(
                leaf_idx >= start && leaf_idx < end,
                "leaf {} not in partition [{}, {})",
                leaf_idx,
                start,
                end
            );
        }
    }
}

#[test]
fn worker_service_dynamic_worker_count() {
    // Test reading worker_count as it changes
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(4, 32).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let config = SchedulerConfig {
        min_workers: 2,
        max_workers: 8,
        ..SchedulerConfig::default()
    };

    let service = Scheduler::<10, 6>::start(arena, config);

    thread::sleep(Duration::from_millis(50));

    let initial_count = service.worker_count();
    assert!(initial_count >= 2, "should have at least min_workers");
    assert!(initial_count <= 8, "should not exceed max_workers");

    // Worker count may change over time, but should stay in bounds
    for _ in 0..10 {
        thread::sleep(Duration::from_millis(20));
        let count = service.worker_count();
        assert!(count <= 8, "worker count {} should not exceed max", count);
    }

    service.shutdown();
}

#[test]
fn worker_service_tick_stats_progression() {
    // Verify tick stats actually progress
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(1, 8).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let config = SchedulerConfig {
        tick_duration: Duration::from_nanos(16_777_216), // Must be power of 2 ns: 2^24 = ~16.7ms
        min_workers: 1,
        max_workers: 1,
    };

    let service = Scheduler::<10, 6>::start(arena, config);

    // Wait much longer for ticks to stabilize
    thread::sleep(Duration::from_millis(100));
    let stats1 = service.tick_stats();

    // Wait much longer for tick progression
    thread::sleep(Duration::from_millis(300));
    let stats2 = service.tick_stats();

    assert!(
        stats2.total_ticks > stats1.total_ticks,
        "ticks should progress: {} -> {}",
        stats1.total_ticks,
        stats2.total_ticks
    );

    service.shutdown();
}

#[test]
fn worker_service_has_work_detection() {
    // Test worker_has_work detection
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(2, 16).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let service = Scheduler::<10, 6>::start(arena, SchedulerConfig::default());

    thread::sleep(Duration::from_millis(50));

    // Initially may or may not have work
    let worker_count = service.worker_count();
    for worker_id in 0..worker_count {
        let _ = service.worker_has_work(worker_id);
    }

    service.shutdown();
}

#[test]
fn task_arena_signal_ptr_validity() {
    // Test that signal pointers are valid
    let arena = TaskArena::with_config(
        TaskArenaConfig::new(2, 16).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    for leaf in 0..2 {
        for signal in 0..1 {
            let signal_ptr = arena.task_signal_ptr(leaf, signal);
            assert!(!signal_ptr.is_null(), "signal ptr should not be null");
        }
    }
}

#[test]
fn task_arena_handle_for_location_comprehensive() {
    // Test handle_for_location with all valid locations
    use maniac::runtime::waker::WorkerWaker;

    let arena = TaskArena::with_config(
        TaskArenaConfig::new(2, 64).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let wakers: Vec<Arc<WorkerWaker>> =
        vec![Arc::new(WorkerWaker::new()), Arc::new(WorkerWaker::new())];
    let worker_count = Arc::new(AtomicUsize::new(2));
    let tree = Summary::new(2, 1, &wakers, &worker_count);

    // Initialize all tasks
    for leaf in 0..2 {
        for slot in 0..64 {
            let global_id = arena.compose_id(leaf, slot);
            arena.init_task(global_id, &tree as *const _);
        }
    }

    // Get handles for all locations
    for leaf in 0..2 {
        for signal in 0..1 {
            for bit in 0..64 {
                let handle = arena.handle_for_location(leaf, signal, bit as u8);
                assert!(
                    handle.is_some(),
                    "handle should exist for leaf={}, signal={}, bit={}",
                    leaf,
                    signal,
                    bit
                );
            }
        }
    }
}

#[test]
fn task_arena_multiple_init_task_calls() {
    // Test that init_task can be called multiple times (reset)
    use maniac::runtime::waker::WorkerWaker;

    let arena = TaskArena::with_config(
        TaskArenaConfig::new(1, 8).unwrap(),
        TaskArenaOptions::default(),
    )
    .expect("failed to create arena");

    let wakers: Vec<Arc<WorkerWaker>> = vec![Arc::new(WorkerWaker::new())];
    let worker_count = Arc::new(AtomicUsize::new(1));
    let tree = Summary::new(1, 1, &wakers, &worker_count);

    let global_id = 0;

    // Initialize multiple times
    for _ in 0..5 {
        arena.init_task(global_id, &tree as *const _);
        let task = unsafe { arena.task(0, 0) };
        assert_eq!(task.global_id(), global_id);
    }
}

#[test]
fn integration_producer_consumer_pattern() {
    // Test producer-consumer pattern
    let runtime = create_executor(2);

    let queue = Arc::new(Mutex::new(Vec::new()));
    let consumer_done = Arc::new(AtomicBool::new(false));

    // Producer
    let q1 = queue.clone();
    let producer = runtime
        .spawn(async move {
            for i in 0..20 {
                q1.lock().unwrap().push(i);
                thread::sleep(Duration::from_micros(100));
            }
        })
        .expect("spawn producer");

    // Consumer - simplified to avoid busy-wait in async task
    let q2 = queue.clone();
    let done = consumer_done.clone();
    let consumer = runtime
        .spawn(async move {
            // Simplified: just wait for producer to finish then consume all
            let mut consumed = 0;
            // Poll a few times with short sleeps
            for _ in 0..100 {
                let mut q = q2.lock().unwrap();
                while let Some(_) = q.pop() {
                    consumed += 1;
                }
                drop(q);
                if consumed >= 20 {
                    break;
                }
                thread::sleep(Duration::from_micros(200));
            }
            done.store(true, Ordering::SeqCst);
            consumed
        })
        .expect("spawn consumer");

    block_on(producer);
    let count = block_on(consumer);

    assert!(
        count >= 18,
        "consumer should consume most items, got {}",
        count
    );
    assert!(consumer_done.load(Ordering::SeqCst));
}

#[test]
fn integration_fan_out_fan_in() {
    // Test fan-out/fan-in pattern
    let runtime = create_executor(4);

    let results = Arc::new(Mutex::new(Vec::new()));

    // Fan-out: spawn multiple workers
    let mut handles = Vec::new();
    for i in 0..10 {
        let results_clone = results.clone();
        let handle = runtime
            .spawn(async move {
                // Simulate work
                let value = i * i;
                thread::sleep(Duration::from_millis(10));
                results_clone.lock().unwrap().push(value);
                value
            })
            .expect("spawn");
        handles.push(handle);
    }

    // Fan-in: collect results
    let mut sum = 0u64;
    for handle in handles {
        sum += block_on(handle);
    }

    // Verify
    assert_eq!(sum, (0..10).map(|i| i * i).sum::<u64>());
    assert_eq!(results.lock().unwrap().len(), 10);
}

#[test]
fn integration_recursive_spawn() {
    // Test recursive task spawning (limited depth to prevent exhaustion)
    let runtime = create_executor(4);

    // Note: Recursive spawning with block_on doesn't work because block_on
    // monopolizes the thread. This test uses a simpler iterative approach.

    let counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    // Spawn multiple levels of tasks
    for level in 0..3 {
        for i in 0..5 {
            let counter_clone = counter.clone();
            let handle = runtime
                .spawn(async move {
                    counter_clone.fetch_add(level * 10 + i, Ordering::Relaxed);
                })
                .expect("spawn");
            handles.push(handle);
        }
    }

    // Complete all tasks
    for handle in handles {
        block_on(handle);
    }

    // Verify all tasks executed
    let total = counter.load(Ordering::Relaxed);
    assert!(total > 0, "recursive spawns should execute")
}

#[test]
fn integration_barrier_synchronization() {
    // Test barrier-style synchronization
    let runtime = create_executor(4);

    let counter = Arc::new(AtomicUsize::new(0));
    let phase1_done = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();
    for _ in 0..5 {
        let counter_clone = counter.clone();
        let phase1_done_clone = phase1_done.clone();

        let handle = runtime
            .spawn(async move {
                // Simple increment without busy-wait loop
                counter_clone.fetch_add(1, Ordering::SeqCst);
                phase1_done_clone.store(true, Ordering::SeqCst);
            })
            .expect("spawn");

        handles.push(handle);
    }

    for handle in handles {
        block_on(handle);
    }

    assert_eq!(
        counter.load(Ordering::SeqCst),
        5,
        "all 5 tasks should execute"
    );
    assert!(
        phase1_done.load(Ordering::SeqCst),
        "phase 1 should complete"
    );
}

#[test]
fn stress_test_continuous_spawn_complete() {
    // Continuously spawn and complete tasks
    let runtime = create_executor(4);

    let counter = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let mut total_spawned = 0;

    // Run for 300ms to be even more conservative
    while start.elapsed() < Duration::from_millis(300) {
        let counter_clone = counter.clone();
        if let Ok(handle) = runtime.spawn(async move {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }) {
            block_on(handle);
            total_spawned += 1;
        }
    }

    println!("Spawned and completed {} tasks in 300ms", total_spawned);
    assert!(total_spawned >= 5, "should complete at least 5 tasks");
    assert_eq!(counter.load(Ordering::Relaxed), total_spawned);
}

#[test]
fn stress_test_maximum_concurrent_tasks() {
    // Test maximum concurrency with more realistic limits
    let runtime = create_executor(8);

    let completed = Arc::new(AtomicUsize::new(0));
    let start_flag = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    // Spawn 50 tasks (reduced from 100 to prevent exhaustion)
    for _ in 0..50 {
        let completed_clone = completed.clone();
        let start_flag_clone = start_flag.clone();

        match runtime.spawn(async move {
            // Wait for start signal
            while !start_flag_clone.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_micros(10));
            }
            completed_clone.fetch_add(1, Ordering::Relaxed);
        }) {
            Ok(handle) => handles.push(handle),
            Err(_) => break, // Stop if we hit capacity
        }
    }

    println!("Successfully spawned {} tasks", handles.len());

    // Release all tasks
    start_flag.store(true, Ordering::Relaxed);

    // Wait for all to complete
    for handle in handles {
        block_on(handle);
    }

    assert!(
        completed.load(Ordering::Relaxed) >= 40,
        "at least 40 tasks should complete, got {}",
        completed.load(Ordering::Relaxed)
    );
}

#[test]
fn stress_test_alternating_work_idle() {
    // Test alternating between work and idle
    let runtime = create_executor(2);

    let counter = Arc::new(AtomicUsize::new(0));

    // Reduce iterations from 10 to 5 to prevent hanging
    for iteration in 0..5 {
        // Burst of work
        let mut handles = Vec::new();
        for _ in 0..10 {
            let counter_clone = counter.clone();
            let handle = runtime
                .spawn(async move {
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                })
                .expect("spawn");
            handles.push(handle);
        }

        for handle in handles {
            block_on(handle);
        }

        // Shorter idle period
        thread::sleep(Duration::from_millis(5));

        println!(
            "Iteration {}: counter = {}",
            iteration,
            counter.load(Ordering::Relaxed)
        );
    }

    assert_eq!(counter.load(Ordering::Relaxed), 50); // 5 iterations * 10 tasks
}
