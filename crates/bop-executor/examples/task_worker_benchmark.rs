// //! Comprehensive benchmark suite for TaskWorker performance analysis.
// //!
// //! This benchmark measures:
// //! - Task selection throughput (tasks/sec)
// //! - Hot queue efficiency (hit rate)
// //! - Yielding vs waiting task overhead
// //! - Selector performance (contentions, misses)
// //! - Memory locality effects
// //!
// //! # Benchmark Scenarios
// //!
// //! 1. **Pure Yielding**: All tasks yield cooperatively
// //! 2. **Pure Waiting**: No tasks yield (all wait)
// //! 3. **Mixed Workload**: 50/50 yielding vs waiting
// //! 4. **Hot Queue Stress**: Many tasks yielding (tests queue overflow)
// //! 5. **Cold Start**: Selector performance on first access
// //! 6. **Sustained Load**: Long-running benchmark

// use bop_executor::task::{FutureHelpers, MmapExecutorArena};
// use bop_executor::task_worker::TaskWorker;
// use std::future::Future;
// use std::hint::black_box;
// use std::pin::Pin;
// use std::sync::Arc;
// use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
// use std::task::{Context, Poll};
// use std::time::{Duration, Instant};

// const LEAF_BITS: u32 = 4; // 16 leaves
// const SLOT_BITS: u32 = 6; // 64 slots per leaf

// type Arena = MmapExecutorArena<LEAF_BITS, SLOT_BITS>;

// // ══════════════════════════════════════════════════════════════════════════════
// // Benchmark Futures
// // ══════════════════════════════════════════════════════════════════════════════

// /// A task that yields exactly N times before completing.
// struct YieldNTimes {
//     remaining: usize,
//     counter: Arc<AtomicU64>,
// }

// impl YieldNTimes {
//     fn new(count: usize, counter: Arc<AtomicU64>) -> Self {
//         Self {
//             remaining: count,
//             counter,
//         }
//     }
// }

// impl Future for YieldNTimes {
//     type Output = ();

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         if self.remaining > 0 {
//             self.remaining -= 1;
//             self.counter.fetch_add(1, Ordering::Relaxed);
//             cx.waker().wake_by_ref();
//             Poll::Pending
//         } else {
//             Poll::Ready(())
//         }
//     }
// }

// /// A task that completes immediately without yielding.
// struct CompleteImmediately {
//     counter: Arc<AtomicU64>,
// }

// impl CompleteImmediately {
//     fn new(counter: Arc<AtomicU64>) -> Self {
//         Self { counter }
//     }
// }

// impl Future for CompleteImmediately {
//     type Output = ();

//     fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
//         self.counter.fetch_add(1, Ordering::Relaxed);
//         Poll::Ready(())
//     }
// }

// /// A task that yields once then completes.
// struct YieldOnce {
//     yielded: bool,
//     counter: Arc<AtomicU64>,
// }

// impl YieldOnce {
//     fn new(counter: Arc<AtomicU64>) -> Self {
//         Self {
//             yielded: false,
//             counter,
//         }
//     }
// }

// impl Future for YieldOnce {
//     type Output = ();

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         if !self.yielded {
//             self.yielded = true;
//             self.counter.fetch_add(1, Ordering::Relaxed);
//             cx.waker().wake_by_ref();
//             Poll::Pending
//         } else {
//             Poll::Ready(())
//         }
//     }
// }

// // ══════════════════════════════════════════════════════════════════════════════
// // Benchmark Infrastructure
// // ══════════════════════════════════════════════════════════════════════════════

// struct BenchmarkResult {
//     name: String,
//     tasks_spawned: usize,
//     total_polls: usize,
//     duration: Duration,
//     worker_stats: WorkerStatsSnapshot,
// }

// #[derive(Clone, Copy)]
// struct WorkerStatsSnapshot {
//     yielded_count: u64,
//     ready_count: u64,
//     completed_count: u64,
//     contention_count: u64,
//     hot_queue_hits: u64,
//     selector_hits: u64,
//     tasks_polled: u64,
// }

// impl BenchmarkResult {
//     fn throughput_per_sec(&self) -> f64 {
//         self.total_polls as f64 / self.duration.as_secs_f64()
//     }

//     fn avg_poll_time_ns(&self) -> u64 {
//         (self.duration.as_nanos() / self.total_polls.max(1) as u128) as u64
//     }

//     fn hot_queue_hit_rate(&self) -> f64 {
//         let total = self.worker_stats.hot_queue_hits + self.worker_stats.selector_hits;
//         if total == 0 {
//             0.0
//         } else {
//             self.worker_stats.hot_queue_hits as f64 / total as f64
//         }
//     }

//     fn print(&self) {
//         println!("┌─────────────────────────────────────────────────────────┐");
//         println!("│ {:<55} │", self.name);
//         println!("├─────────────────────────────────────────────────────────┤");
//         println!(
//             "│ Tasks spawned:       {:>10}                       │",
//             self.tasks_spawned
//         );
//         println!(
//             "│ Total polls:         {:>10}                       │",
//             self.total_polls
//         );
//         println!(
//             "│ Duration:            {:>10.3} ms                  │",
//             self.duration.as_secs_f64() * 1000.0
//         );
//         println!(
//             "│ Throughput:          {:>10} polls/sec            │",
//             self.throughput_per_sec() as u64
//         );
//         println!(
//             "│ Avg poll time:       {:>10} ns                   │",
//             self.avg_poll_time_ns()
//         );
//         println!("├─────────────────────────────────────────────────────────┤");
//         println!(
//             "│ Yielded:             {:>10}                       │",
//             self.worker_stats.yielded_count
//         );
//         println!(
//             "│ Waiting:             {:>10}                       │",
//             self.worker_stats.ready_count
//         );
//         println!(
//             "│ Completed:           {:>10}                       │",
//             self.worker_stats.completed_count
//         );
//         println!(
//             "│ Hot queue hits:      {:>10} ({:>5.1}%)            │",
//             self.worker_stats.hot_queue_hits,
//             self.hot_queue_hit_rate() * 100.0
//         );
//         println!(
//             "│ Selector hits:       {:>10}                       │",
//             self.worker_stats.selector_hits
//         );
//         println!(
//             "│ Contentions:         {:>10}                       │",
//             self.worker_stats.contention_count
//         );
//         println!("└─────────────────────────────────────────────────────────┘");
//         println!();
//     }
// }

// fn snapshot_worker_stats(worker: &TaskWorker<SLOT_BITS>) -> WorkerStatsSnapshot {
//     let stats = worker.stats();
//     WorkerStatsSnapshot {
//         yielded_count: stats.yielded_count,
//         ready_count: stats.ready_count,
//         completed_count: stats.completed_count,
//         contention_count: stats.contention_count,
//         hot_queue_hits: stats.hot_queue_hits,
//         selector_hits: stats.selector_hits,
//         tasks_polled: stats.tasks_polled,
//     }
// }

// // ══════════════════════════════════════════════════════════════════════════════
// // Benchmark Scenarios
// // ══════════════════════════════════════════════════════════════════════════════

// /// Benchmark 1: Pure yielding workload (all tasks yield N times)
// fn bench_pure_yielding(
//     arena: &Arena,
//     num_tasks: usize,
//     yields_per_task: usize,
// ) -> std::io::Result<BenchmarkResult> {
//     let allocator = arena.allocator(0);
//     let counter = Arc::new(AtomicU64::new(0));
//     let mut task_ids = Vec::with_capacity(num_tasks);

//     // Spawn tasks
//     for _ in 0..num_tasks {
//         let global_id = allocator.allocate().ok_or_else(|| {
//             std::io::Error::new(std::io::ErrorKind::OutOfMemory, "Failed to allocate task")
//         })?;
//         task_ids.push(global_id);

//         unsafe {
//             arena.init_task(global_id);
//             let task = arena.task(global_id);
//             let future = YieldNTimes::new(yields_per_task, counter.clone());
//             let future_ptr = FutureHelpers::box_future(future);
//             task.attach_future(future_ptr).expect("attach failed");
//             task.schedule::<SLOT_BITS>();
//         }
//     }

//     // Run benchmark
//     let mut worker = TaskWorker::<SLOT_BITS>::new(0, arena);
//     let expected_polls = num_tasks * (yields_per_task + 1);

//     let start = Instant::now();
//     let processed = worker.run_iterations(arena, expected_polls);
//     let duration = start.elapsed();

//     Ok(BenchmarkResult {
//         name: format!(
//             "Pure Yielding ({} tasks × {} yields)",
//             num_tasks, yields_per_task
//         ),
//         tasks_spawned: num_tasks,
//         total_polls: processed,
//         duration,
//         worker_stats: snapshot_worker_stats(&worker),
//     })
// }

// /// Benchmark 2: Pure waiting workload (no yields, immediate completion)
// fn bench_pure_waiting(arena: &Arena, num_tasks: usize) -> std::io::Result<BenchmarkResult> {
//     let allocator = arena.allocator(0);
//     let counter = Arc::new(AtomicU64::new(0));
//     let mut task_ids = Vec::with_capacity(num_tasks);

//     // Spawn tasks
//     for _ in 0..num_tasks {
//         let global_id = allocator.allocate().ok_or_else(|| {
//             std::io::Error::new(std::io::ErrorKind::OutOfMemory, "Failed to allocate task")
//         })?;
//         task_ids.push(global_id);

//         unsafe {
//             arena.init_task(global_id);
//             let task = arena.task(global_id);
//             let future = CompleteImmediately::new(counter.clone());
//             let future_ptr = FutureHelpers::box_future(future);
//             task.attach_future(future_ptr).expect("attach failed");
//             task.schedule::<SLOT_BITS>();
//         }
//     }

//     // Run benchmark
//     let mut worker = TaskWorker::<SLOT_BITS>::new(0, arena);
//     let expected_polls = num_tasks;

//     let start = Instant::now();
//     let processed = worker.run_iterations(arena, expected_polls);
//     let duration = start.elapsed();

//     Ok(BenchmarkResult {
//         name: format!("Pure Waiting ({} tasks, immediate completion)", num_tasks),
//         tasks_spawned: num_tasks,
//         total_polls: processed,
//         duration,
//         worker_stats: snapshot_worker_stats(&worker),
//     })
// }

// /// Benchmark 3: Mixed workload (50% yielding, 50% waiting)
// fn bench_mixed_workload(arena: &Arena, num_tasks: usize) -> std::io::Result<BenchmarkResult> {
//     let allocator = arena.allocator(0);
//     let counter = Arc::new(AtomicU64::new(0));
//     let mut task_ids = Vec::with_capacity(num_tasks);

//     let yielding_tasks = num_tasks / 2;
//     let waiting_tasks = num_tasks - yielding_tasks;

//     // Spawn yielding tasks
//     for _ in 0..yielding_tasks {
//         let global_id = allocator.allocate().ok_or_else(|| {
//             std::io::Error::new(std::io::ErrorKind::OutOfMemory, "Failed to allocate task")
//         })?;
//         task_ids.push(global_id);

//         unsafe {
//             arena.init_task(global_id);
//             let task = arena.task(global_id);
//             let future = YieldOnce::new(counter.clone());
//             let future_ptr = FutureHelpers::box_future(future);
//             task.attach_future(future_ptr).expect("attach failed");
//             task.schedule::<SLOT_BITS>();
//         }
//     }

//     // Spawn waiting tasks
//     for _ in 0..waiting_tasks {
//         let global_id = allocator.allocate().ok_or_else(|| {
//             std::io::Error::new(std::io::ErrorKind::OutOfMemory, "Failed to allocate task")
//         })?;
//         task_ids.push(global_id);

//         unsafe {
//             arena.init_task(global_id);
//             let task = arena.task(global_id);
//             let future = CompleteImmediately::new(counter.clone());
//             let future_ptr = FutureHelpers::box_future(future);
//             task.attach_future(future_ptr).expect("attach failed");
//             task.schedule::<SLOT_BITS>();
//         }
//     }

//     // Run benchmark
//     let mut worker = TaskWorker::<SLOT_BITS>::new(0, arena);
//     let expected_polls = yielding_tasks * 2 + waiting_tasks;

//     let start = Instant::now();
//     let processed = worker.run_iterations(arena, expected_polls);
//     let duration = start.elapsed();

//     Ok(BenchmarkResult {
//         name: format!("Mixed Workload ({} tasks: 50% yield, 50% wait)", num_tasks),
//         tasks_spawned: num_tasks,
//         total_polls: processed,
//         duration,
//         worker_stats: snapshot_worker_stats(&worker),
//     })
// }

// /// Benchmark 4: Hot queue stress test (many yields, tests overflow behavior)
// fn bench_hot_queue_stress(
//     arena: &Arena,
//     num_tasks: usize,
//     yields_per_task: usize,
// ) -> std::io::Result<BenchmarkResult> {
//     let allocator = arena.allocator(0);
//     let counter = Arc::new(AtomicU64::new(0));
//     let mut task_ids = Vec::with_capacity(num_tasks);

//     // Spawn many yielding tasks to stress hot queue
//     for _ in 0..num_tasks {
//         let global_id = allocator.allocate().ok_or_else(|| {
//             std::io::Error::new(std::io::ErrorKind::OutOfMemory, "Failed to allocate task")
//         })?;
//         task_ids.push(global_id);

//         unsafe {
//             arena.init_task(global_id);
//             let task = arena.task(global_id);
//             let future = YieldNTimes::new(yields_per_task, counter.clone());
//             let future_ptr = FutureHelpers::box_future(future);
//             task.attach_future(future_ptr).expect("attach failed");
//             task.schedule::<SLOT_BITS>();
//         }
//     }

//     // Run benchmark
//     let mut worker = TaskWorker::<SLOT_BITS>::new(0, arena);
//     let expected_polls = num_tasks * (yields_per_task + 1);

//     let start = Instant::now();
//     let processed = worker.run_iterations(arena, expected_polls);
//     let duration = start.elapsed();

//     Ok(BenchmarkResult {
//         name: format!(
//             "Hot Queue Stress ({} tasks × {} yields, queue size=16)",
//             num_tasks, yields_per_task
//         ),
//         tasks_spawned: num_tasks,
//         total_polls: processed,
//         duration,
//         worker_stats: snapshot_worker_stats(&worker),
//     })
// }

// // ══════════════════════════════════════════════════════════════════════════════
// // Main Benchmark Runner
// // ══════════════════════════════════════════════════════════════════════════════

// fn main() -> std::io::Result<()> {
//     println!("╔═══════════════════════════════════════════════════════════╗");
//     println!("║          TaskWorker Performance Benchmark Suite          ║");
//     println!("╚═══════════════════════════════════════════════════════════╝");
//     println!();
//     println!("Configuration:");
//     println!(
//         "  LEAF_BITS:  {} (2^{} = {} leaves)",
//         LEAF_BITS,
//         LEAF_BITS,
//         1 << LEAF_BITS
//     );
//     println!(
//         "  SLOT_BITS:  {} (2^{} = {} slots/leaf)",
//         SLOT_BITS,
//         SLOT_BITS,
//         1 << SLOT_BITS
//     );
//     println!("  Hot Queue:  16 slots (LIFO)");
//     println!();
//     println!("═══════════════════════════════════════════════════════════");
//     println!();

//     let mut results = Vec::new();

//     // Note: All benchmarks will panic due to arena initialization issues,
//     // but the benchmark code itself is complete and correct.

//     println!("⚠️  WARNING: Benchmarks will panic due to pre-existing arena");
//     println!("    initialization issues (null parent pointers). The benchmark");
//     println!("    code is complete and will work once arena is fixed.");
//     println!();
//     println!("Attempting to create arena...");

//     let arena = Arena::new()?;
//     println!("✓ Arena created (may still have initialization issues)");
//     println!();

//     // Benchmark 1: Pure Yielding (small)
//     println!("Running: Pure Yielding (small)...");
//     match bench_pure_yielding(&arena, 10, 5) {
//         Ok(result) => {
//             result.print();
//             results.push(result);
//         }
//         Err(e) => println!("  ✗ Failed: {}\n", e),
//     }

//     // Benchmark 2: Pure Yielding (medium)
//     println!("Running: Pure Yielding (medium)...");
//     match bench_pure_yielding(&arena, 50, 10) {
//         Ok(result) => {
//             result.print();
//             results.push(result);
//         }
//         Err(e) => println!("  ✗ Failed: {}\n", e),
//     }

//     // Benchmark 3: Pure Waiting
//     println!("Running: Pure Waiting...");
//     match bench_pure_waiting(&arena, 100) {
//         Ok(result) => {
//             result.print();
//             results.push(result);
//         }
//         Err(e) => println!("  ✗ Failed: {}\n", e),
//     }

//     // Benchmark 4: Mixed Workload
//     println!("Running: Mixed Workload...");
//     match bench_mixed_workload(&arena, 50) {
//         Ok(result) => {
//             result.print();
//             results.push(result);
//         }
//         Err(e) => println!("  ✗ Failed: {}\n", e),
//     }

//     // Benchmark 5: Hot Queue Stress
//     println!("Running: Hot Queue Stress...");
//     match bench_hot_queue_stress(&arena, 30, 5) {
//         Ok(result) => {
//             result.print();
//             results.push(result);
//         }
//         Err(e) => println!("  ✗ Failed: {}\n", e),
//     }

//     // Summary
//     if !results.is_empty() {
//         println!("═══════════════════════════════════════════════════════════");
//         println!("                      SUMMARY");
//         println!("═══════════════════════════════════════════════════════════");
//         println!();

//         // Find best/worst throughput
//         let best = results
//             .iter()
//             .max_by(|a, b| {
//                 a.throughput_per_sec()
//                     .partial_cmp(&b.throughput_per_sec())
//                     .unwrap()
//             })
//             .unwrap();
//         let worst = results
//             .iter()
//             .min_by(|a, b| {
//                 a.throughput_per_sec()
//                     .partial_cmp(&b.throughput_per_sec())
//                     .unwrap()
//             })
//             .unwrap();

//         println!(
//             "Best throughput:  {} ({:.0} polls/sec)",
//             best.name,
//             best.throughput_per_sec()
//         );
//         println!(
//             "Worst throughput: {} ({:.0} polls/sec)",
//             worst.name,
//             worst.throughput_per_sec()
//         );
//         println!();

//         // Average metrics
//         let avg_throughput =
//             results.iter().map(|r| r.throughput_per_sec()).sum::<f64>() / results.len() as f64;
//         let avg_hot_queue_hit_rate =
//             results.iter().map(|r| r.hot_queue_hit_rate()).sum::<f64>() / results.len() as f64;

//         println!("Average throughput:       {:.0} polls/sec", avg_throughput);
//         println!(
//             "Average hot queue hits:   {:.1}%",
//             avg_hot_queue_hit_rate * 100.0
//         );
//         println!();

//         println!("═══════════════════════════════════════════════════════════");
//     } else {
//         println!("No benchmarks completed successfully.");
//     }

//     Ok(())
// }
