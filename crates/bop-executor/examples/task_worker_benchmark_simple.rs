// //! Simple high-throughput TaskWorker benchmark.
// //!
// //! This benchmark spawns all tasks upfront and measures how quickly
// //! the worker can process millions of polls.

// use bop_executor::fast_task_worker::FastTaskWorker;
// use bop_executor::task::{FutureHelpers, MmapExecutorArena};
// use std::future::Future;
// use std::pin::Pin;
// use std::sync::Arc;
// use std::sync::atomic::{AtomicU64, Ordering};
// use std::task::{Context, Poll};
// use std::time::{Duration, Instant};

// const LEAF_BITS: u32 = 8; // 256 leaves
// const SLOT_BITS: u32 = 8; // 256 slots per leaf = 65,536 total tasks

// type Arena = MmapExecutorArena;

// // ══════════════════════════════════════════════════════════════════════════════
// // Benchmark Futures
// // ══════════════════════════════════════════════════════════════════════════════

// /// A task that yields exactly N times before completing.
// struct YieldNTimes {
//     remaining: usize,
//     counter: Arc<AtomicU64>,
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
//             self.counter.fetch_add(1, Ordering::Relaxed);
//             Poll::Ready(())
//         }
//     }
// }

// // ══════════════════════════════════════════════════════════════════════════════
// // Benchmark Functions
// // ══════════════════════════════════════════════════════════════════════════════

// fn bench_yielding(arena: &Arena, num_tasks: usize, yields_per_task: usize) -> (Duration, u64, u64) {
//     let allocator = arena.allocator(0);
//     let counter = Arc::new(AtomicU64::new(0));
//     let mut task_ids = Vec::with_capacity(num_tasks);

//     // Spawn all tasks
//     for _ in 0..num_tasks {
//         if let Some(global_id) = allocator.allocate() {
//             unsafe {
//                 arena.init_task(global_id);
//                 let task = arena.task(global_id);
//                 let future = YieldNTimes {
//                     remaining: yields_per_task,
//                     counter: counter.clone(),
//                 };
//                 let future_ptr = FutureHelpers::box_future(future);
//                 task.attach_future(future_ptr).expect("attach failed");
//                 task.schedule::<SLOT_BITS>();
//                 task_ids.push(global_id);
//             }
//         } else {
//             break;
//         }
//     }

//     println!("  Spawned {} tasks", task_ids.len());

//     // Run worker
//     let mut worker = TaskWorker::<SLOT_BITS>::new(0, arena);
//     let expected_polls = task_ids.len() * (yields_per_task + 1);

//     println!("  Expected {} polls...", expected_polls);

//     let start = Instant::now();
//     let mut total_processed = 0;

//     // Run until all work is done
//     loop {
//         let processed = worker.run_iterations(arena, 10000);
//         if processed == 0 {
//             break;
//         }
//         total_processed += processed;

//         if total_processed >= expected_polls {
//             break;
//         }
//     }

//     let duration = start.elapsed();
//     let actual_polls = counter.load(Ordering::Relaxed);

//     (duration, total_processed as u64, actual_polls)
// }

// // ══════════════════════════════════════════════════════════════════════════════
// // Main
// // ══════════════════════════════════════════════════════════════════════════════

// fn print_result(name: &str, duration: Duration, worker_polls: u64, actual_polls: u64) {
//     let throughput = worker_polls as f64 / duration.as_secs_f64();
//     let avg_ns = duration.as_nanos() as f64 / worker_polls as f64;

//     println!("┌─────────────────────────────────────────────────────────┐");
//     println!("│ {:<55} │", name);
//     println!("├─────────────────────────────────────────────────────────┤");
//     println!(
//         "│ Worker polls:        {:>15}                    │",
//         worker_polls
//     );
//     println!(
//         "│ Actual polls:        {:>15}                    │",
//         actual_polls
//     );
//     println!(
//         "│ Duration:            {:>15.3} ms               │",
//         duration.as_secs_f64() * 1000.0
//     );
//     println!(
//         "│ Throughput:          {:>15.0} polls/sec        │",
//         throughput
//     );
//     println!("│ Avg poll time:       {:>15.1} ns               │", avg_ns);
//     println!("└─────────────────────────────────────────────────────────┘");
//     println!();
// }

// fn main() -> std::io::Result<()> {
//     println!("╔═══════════════════════════════════════════════════════════╗");
//     println!("║       TaskWorker High-Throughput Benchmark (Simple)      ║");
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
//     println!(
//         "  Capacity:   {} total tasks",
//         (1 << LEAF_BITS) * (1 << SLOT_BITS)
//     );
//     println!();
//     println!("═══════════════════════════════════════════════════════════");
//     println!();

//     let arena = Arena::new()?;

//     // Benchmark 1: 10K tasks, 10 yields each = 110K polls
//     println!("Benchmark 1: 10K tasks × 10 yields = 110K polls");
//     let (dur, worker_polls, actual) = bench_yielding(&arena, 10_000, 10);
//     print_result("10K tasks × 10 yields", dur, worker_polls, actual);

//     // Benchmark 2: 10K tasks, 50 yields each = 510K polls
//     println!("Benchmark 2: 10K tasks × 50 yields = 510K polls");
//     let (dur, worker_polls, actual) = bench_yielding(&arena, 10_000, 50);
//     print_result("10K tasks × 50 yields", dur, worker_polls, actual);

//     // Benchmark 3: 10K tasks, 100 yields each = 1.01M polls
//     println!("Benchmark 3: 10K tasks × 100 yields = 1.01M polls");
//     let (dur, worker_polls, actual) = bench_yielding(&arena, 10_000, 100);
//     print_result("10K tasks × 100 yields", dur, worker_polls, actual);

//     println!("═══════════════════════════════════════════════════════════");
//     println!("                    Benchmark Complete                     ");
//     println!("═══════════════════════════════════════════════════════════");

//     Ok(())
// }
