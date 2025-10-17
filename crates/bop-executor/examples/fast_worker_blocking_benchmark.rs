// //! Benchmark for blocking poll with thread parking.
// //!
// //! This benchmark demonstrates the thread parking feature by spawning tasks
// //! in batches with delays between batches, triggering worker parking behavior.

// use bop_executor::fast_task_worker::{FastTaskWorker, FastWorkerStats, PollConfig};
// use bop_executor::task::{ArenaOptions, FutureHelpers, MmapExecutorArena, TaskAllocator};
// use std::future::Future;
// use std::pin::Pin;
// use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
// use std::sync::{Arc, Barrier};
// use std::task::{Context, Poll};
// use std::thread;
// use std::time::{Duration, Instant};

// const LEAF_COUNT: u32 = 256;
// const SLOT_COUNT: u32 = 1024;
// const SIGNALS_PER_LEAF: usize = (SLOT_COUNT / 64) as usize;

// /// Simple future that yields N times then completes.
// struct YieldNTimes {
//     remaining: usize,
// }

// impl Future for YieldNTimes {
//     type Output = ();

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         if self.remaining > 0 {
//             self.remaining -= 1;
//             cx.waker().wake_by_ref();
//             Poll::Pending
//         } else {
//             Poll::Ready(())
//         }
//     }
// }

// /// Benchmark with blocking poll and slow task spawning.
// fn bench_blocking(
//     num_workers: usize,
//     tasks_per_batch: usize,
//     batches: usize,
//     batch_interval_ms: u64,
//     yields_per_task: usize,
//     poll_config: PollConfig,
//     config_name: &str,
// ) {
//     println!("\n┌─────────────────────────────────────────────────────────────┐");
//     println!("│ Config: {:<52} │", config_name);
//     println!("├─────────────────────────────────────────────────────────────┤");
//     println!(
//         "│ Workers:               {:>8}                            │",
//         num_workers
//     );
//     println!(
//         "│ Tasks per batch:       {:>8}                            │",
//         tasks_per_batch
//     );
//     println!(
//         "│ Batches:               {:>8}                            │",
//         batches
//     );
//     println!(
//         "│ Batch interval:        {:>8} ms                         │",
//         batch_interval_ms
//     );
//     println!(
//         "│ Yields per task:       {:>8}                            │",
//         yields_per_task
//     );
//     println!("└─────────────────────────────────────────────────────────────┘");

//     // Create arena
//     let arena = MmapExecutorArena::new(LEAF_COUNT, SLOT_COUNT, ArenaOptions::default())
//         .expect("Failed to create arena");

//     // Register workers
//     {
//         let tree = arena.active_tree();
//         for _ in 0..num_workers {
//             tree.register_worker();
//         }
//     }

//     let allocator = TaskAllocator::new(&arena, LEAF_COUNT as usize);

//     // Shared state
//     let shutdown = Arc::new(AtomicBool::new(false));
//     let tasks_spawned = Arc::new(AtomicUsize::new(0));
//     let barrier = Arc::new(Barrier::new(num_workers + 1));
//     let arena_addr = &arena as *const MmapExecutorArena as usize;

//     // Spawn worker threads
//     let mut handles = Vec::new();
//     for worker_idx in 0..num_workers {
//         let barrier = Arc::clone(&barrier);
//         let shutdown = Arc::clone(&shutdown);
//         let config = poll_config;

//         let handle = thread::spawn(move || {
//             let arena = unsafe { &*(arena_addr as *const MmapExecutorArena) };
//             let mut worker = FastTaskWorker::new(worker_idx, SIGNALS_PER_LEAF, arena);

//             // Wait for all workers
//             barrier.wait();

//             let mut idle_iterations = 0;
//             let mut shutdown_drain_iterations = 0;
//             const MAX_SHUTDOWN_DRAIN: usize = 10000;

//             loop {
//                 // Check shutdown AFTER attempting work
//                 let is_shutdown = shutdown.load(Ordering::Relaxed);

//                 if is_shutdown {
//                     // Shutdown signaled - drain remaining work before exiting
//                     shutdown_drain_iterations += 1;

//                     // Try to get work without blocking
//                     let found_work =
//                         worker.try_poll_with_leaf_partition(arena, LEAF_COUNT as usize);

//                     if !found_work {
//                         idle_iterations += 1;
//                         if idle_iterations > 100 || shutdown_drain_iterations > MAX_SHUTDOWN_DRAIN {
//                             break; // Exit after 100 consecutive empty checks
//                         }
//                     } else {
//                         idle_iterations = 0;
//                     }
//                     continue;
//                 }

//                 // Normal operation - use blocking poll
//                 let found_work =
//                     worker.poll_with_leaf_partition_blocking(arena, LEAF_COUNT as usize, &config);

//                 if found_work {
//                     idle_iterations = 0;
//                 } else {
//                     idle_iterations += 1;
//                 }
//             }

//             (worker_idx, worker.stats)
//         });

//         handles.push(handle);
//     }

//     // Main thread: wait for workers to be ready
//     barrier.wait();

//     println!("\n  Workers started, beginning task spawning...");
//     let start = Instant::now();

//     // Spawn tasks in batches
//     for batch_num in 0..batches {
//         let batch_start = Instant::now();

//         for _ in 0..tasks_per_batch {
//             if let Some(global_id) = allocator.allocate() {
//                 let (leaf_idx, slot_idx) = arena.decompose_id(global_id);
//                 unsafe {
//                     arena.init_task(global_id);
//                     let task = arena.task(leaf_idx, slot_idx);
//                     let future = YieldNTimes {
//                         remaining: yields_per_task,
//                     };
//                     let future_ptr = FutureHelpers::box_future(future);
//                     task.attach_future(future_ptr).expect("attach failed");
//                     task.schedule();
//                     tasks_spawned.fetch_add(1, Ordering::Relaxed);
//                 }
//             }
//         }

//         println!(
//             "  Batch {}/{}: Spawned {} tasks ({}ms)",
//             batch_num + 1,
//             batches,
//             tasks_per_batch,
//             batch_start.elapsed().as_millis()
//         );

//         // Wait before next batch
//         if batch_num < batches - 1 {
//             thread::sleep(Duration::from_millis(batch_interval_ms));
//         }
//     }

//     // Wait for completion (check that all tasks are done)
//     let total_spawned = tasks_spawned.load(Ordering::Relaxed);
//     println!(
//         "\n  Spawned {} total tasks, waiting for completion...",
//         total_spawned
//     );

//     // Give workers time to finish
//     thread::sleep(Duration::from_millis(500));

//     let duration = start.elapsed();

//     // Signal shutdown
//     shutdown.store(true, Ordering::Relaxed);

//     // Wake all workers to ensure they see shutdown
//     {
//         let tree = arena.active_tree();
//         for _ in 0..num_workers * 2 {
//             tree.release(1);
//         }
//     }

//     // Wait for workers
//     let mut all_stats = Vec::new();
//     for handle in handles {
//         let (idx, stats) = handle.join().unwrap();
//         all_stats.push((idx, stats));
//     }

//     // Aggregate stats
//     let mut total_stats = FastWorkerStats::new();
//     for (_, stats) in all_stats.iter() {
//         total_stats.tasks_polled += stats.tasks_polled;
//         total_stats.completed_count += stats.completed_count;
//         total_stats.yielded_count += stats.yielded_count;
//         total_stats.empty_scans += stats.empty_scans;
//         total_stats.leaf_summary_checks += stats.leaf_summary_checks;
//         total_stats.leaf_summary_hits += stats.leaf_summary_hits;
//         total_stats.leaf_steal_attempts += stats.leaf_steal_attempts;
//         total_stats.leaf_steal_successes += stats.leaf_steal_successes;
//     }

//     // Get sleeper count (workers still registered as sleepers would be a bug)
//     let final_sleepers = arena.active_tree().sleepers();

//     // Print results
//     println!("\n┌─────────────────────────────────────────────────────────────┐");
//     println!("│ Results                                                     │");
//     println!("├─────────────────────────────────────────────────────────────┤");
//     println!(
//         "│ Total duration:        {:>8.2} ms                       │",
//         duration.as_secs_f64() * 1000.0
//     );
//     println!(
//         "│ Tasks spawned:         {:>8}                            │",
//         total_spawned
//     );
//     println!(
//         "│ Tasks completed:       {:>8}                            │",
//         total_stats.completed_count
//     );
//     println!(
//         "│ Total polls:           {:>8}                            │",
//         total_stats.tasks_polled
//     );
//     println!(
//         "│ Empty scans:           {:>8}                            │",
//         total_stats.empty_scans
//     );
//     println!(
//         "│ Final sleepers:        {:>8} (should be 0)              │",
//         final_sleepers
//     );
//     println!("├─────────────────────────────────────────────────────────────┤");
//     println!(
//         "│ Aggregate:             {:>8.0} polls/sec                  │",
//         total_stats.tasks_polled as f64 / duration.as_secs_f64()
//     );
//     println!(
//         "│ Per-worker:            {:>8.0} polls/sec                  │",
//         total_stats.tasks_polled as f64 / duration.as_secs_f64() / num_workers as f64
//     );
//     println!(
//         "│ Avg ns/poll/thread:    {:>8.1} ns                        │",
//         duration.as_nanos() as f64 / total_stats.tasks_polled as f64
//     );
//     println!("└─────────────────────────────────────────────────────────────┘");

//     // Print per-worker stats
//     println!("\n  Per-Worker Stats:");
//     for (idx, stats) in all_stats.iter() {
//         println!(
//             "    Worker {}: {:>8} polls, {:>6} completed, {:>6} empty scans",
//             idx, stats.tasks_polled, stats.completed_count, stats.empty_scans
//         );
//     }
// }

// fn main() {
//     println!("═════════════════════════════════════════════════════════════");
//     println!("         Fast Worker Blocking Poll Benchmark Suite           ");
//     println!("═════════════════════════════════════════════════════════════");

//     // Scenario 1: Immediate parking (no spinning)
//     println!("\n\n[Scenario 1: Immediate Parking - No Spinning]");
//     bench_blocking(
//         4,                              // 4 workers
//         20,                             // 20 tasks per batch
//         5,                              // 5 batches
//         200,                            // 200ms between batches
//         100,                            // 100 yields per task
//         PollConfig::park_immediately(), // No spinning, immediate park
//         "Park Immediately",
//     );

//     // Scenario 2: Low-latency mode (spin 100 times)
//     println!("\n\n[Scenario 2: Low-Latency Mode - Spin 100x]");
//     bench_blocking(
//         4,                         // 4 workers
//         20,                        // 20 tasks per batch
//         5,                         // 5 batches
//         200,                       // 200ms between batches
//         100,                       // 100 yields per task
//         PollConfig::low_latency(), // Spin 100x before parking
//         "Low Latency (Spin 100x)",
//     );

//     // Scenario 3: Balanced mode
//     println!("\n\n[Scenario 3: Balanced Mode - Spin 10x, 1s timeout]");
//     bench_blocking(
//         4,                      // 4 workers
//         20,                     // 20 tasks per batch
//         5,                      // 5 batches
//         200,                    // 200ms between batches
//         100,                    // 100 yields per task
//         PollConfig::balanced(), // Spin 10x, park with 1s timeout
//         "Balanced (Spin 10x, 1s timeout)",
//     );

//     // Scenario 4: More workers (test scaling)
//     println!("\n\n[Scenario 4: Scaling Test - 8 Workers]");
//     bench_blocking(
//         8,                         // 8 workers
//         40,                        // 40 tasks per batch
//         5,                         // 5 batches
//         200,                       // 200ms between batches
//         100,                       // 100 yields per task
//         PollConfig::low_latency(), // Low-latency mode
//         "8 Workers Low Latency",
//     );

//     // Scenario 5: High-frequency arrivals (less parking expected)
//     println!("\n\n[Scenario 5: High-Frequency Arrivals - Minimal Parking Expected]");
//     bench_blocking(
//         4,                              // 4 workers
//         50,                             // 50 tasks per batch
//         10,                             // 10 batches
//         50,                             // Only 50ms between batches
//         50,                             // 50 yields per task
//         PollConfig::park_immediately(), // Immediate park (but should rarely park)
//         "High Frequency Arrivals",
//     );

//     println!("\n\n═════════════════════════════════════════════════════════════");
//     println!("                    Benchmark Complete                        ");
//     println!("═════════════════════════════════════════════════════════════\n");
// }
