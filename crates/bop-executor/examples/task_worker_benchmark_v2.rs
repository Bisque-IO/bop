// //! High-throughput TaskWorker benchmark with millions of iterations.
// //!
// //! This benchmark continuously spawns and processes tasks to measure sustained
// //! performance over millions of polls.

// use bop_executor::task::{FutureHelpers, MmapExecutorArena};
// use bop_executor::task_worker::TaskWorker;
// use std::future::Future;
// use std::pin::Pin;
// use std::sync::Arc;
// use std::sync::atomic::{AtomicU64, Ordering};
// use std::task::{Context, Poll};
// use std::time::{Duration, Instant};

// const LEAF_BITS: u32 = 4; // 16 leaves
// const SLOT_BITS: u32 = 6; // 64 slots per leaf

// type Arena = MmapExecutorArena<LEAF_BITS, SLOT_BITS>;

// // ══════════════════════════════════════════════════════════════════════════════
// // High-Throughput Futures
// // ══════════════════════════════════════════════════════════════════════════════

// /// A task that yields a fixed number of times, designed for sustained load.
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
//             self.counter.fetch_add(1, Ordering::Relaxed);
//             Poll::Ready(())
//         }
//     }
// }

// /// A task that completes immediately (no yielding).
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

// // ══════════════════════════════════════════════════════════════════════════════
// // Continuous Task Spawner
// // ══════════════════════════════════════════════════════════════════════════════

// /// Spawns tasks continuously to maintain a sustained workload.
// struct ContinuousSpawner<'a> {
//     arena: &'a Arena,
//     counter: Arc<AtomicU64>,
//     task_slots: Vec<u64>,
//     next_slot: usize,
//     yields_per_task: usize,
// }

// impl<'a> ContinuousSpawner<'a> {
//     fn new(arena: &'a Arena, yields_per_task: usize) -> Self {
//         let allocator = arena.allocator(0);
//         let max_tasks = 1 << SLOT_BITS; // 64 tasks

//         // Pre-allocate all task slots
//         let mut task_slots = Vec::with_capacity(max_tasks);
//         for _ in 0..max_tasks {
//             if let Some(global_id) = allocator.allocate() {
//                 unsafe { arena.init_task(global_id) };
//                 task_slots.push(global_id);
//             } else {
//                 break;
//             }
//         }

//         Self {
//             arena,
//             counter: Arc::new(AtomicU64::new(0)),
//             task_slots,
//             next_slot: 0,
//             yields_per_task,
//         }
//     }

//     /// Spawns a new yielding task, reusing completed task slots.
//     fn spawn_yielding_task(&mut self) -> bool {
//         if self.task_slots.is_empty() {
//             return false;
//         }

//         let global_id = self.task_slots[self.next_slot];
//         self.next_slot = (self.next_slot + 1) % self.task_slots.len();

//         unsafe {
//             let task = self.arena.task(global_id);

//             // Only spawn if task is idle (previous task completed)
//             if !task.is_idle() {
//                 return false;
//             }

//             // Create and attach future
//             let future = YieldNTimes::new(self.yields_per_task, self.counter.clone());
//             let future_ptr = FutureHelpers::box_future(future);

//             if task.attach_future(future_ptr).is_ok() {
//                 task.schedule::<SLOT_BITS>();
//                 true
//             } else {
//                 // Future already attached, drop it
//                 FutureHelpers::drop_boxed(future_ptr);
//                 false
//             }
//         }
//     }

//     /// Spawns a new non-yielding task (immediate completion).
//     fn spawn_immediate_task(&mut self) -> bool {
//         if self.task_slots.is_empty() {
//             return false;
//         }

//         let global_id = self.task_slots[self.next_slot];
//         self.next_slot = (self.next_slot + 1) % self.task_slots.len();

//         unsafe {
//             let task = self.arena.task(global_id);

//             if !task.is_idle() {
//                 return false;
//             }

//             let future = CompleteImmediately::new(self.counter.clone());
//             let future_ptr = FutureHelpers::box_future(future);

//             if task.attach_future(future_ptr).is_ok() {
//                 task.schedule::<SLOT_BITS>();
//                 true
//             } else {
//                 FutureHelpers::drop_boxed(future_ptr);
//                 false
//             }
//         }
//     }

//     fn poll_count(&self) -> u64 {
//         self.counter.load(Ordering::Relaxed)
//     }
// }

// // ══════════════════════════════════════════════════════════════════════════════
// // High-Throughput Benchmarks
// // ══════════════════════════════════════════════════════════════════════════════

// fn bench_sustained_yielding(
//     arena: &Arena,
//     target_polls: usize,
//     yields_per_task: usize,
// ) -> (Duration, u64) {
//     let mut spawner = ContinuousSpawner::new(arena, yields_per_task);
//     let mut worker = TaskWorker::<SLOT_BITS>::new(0, arena);

//     // Initial spawn batch
//     for _ in 0..32 {
//         spawner.spawn_yielding_task();
//     }

//     let start = Instant::now();
//     let mut iterations = 0;

//     while iterations < target_polls {
//         // Process one task
//         let processed = worker.run_iterations(arena, 1);

//         if processed == 0 {
//             // No work available, spawn more tasks
//             for _ in 0..8 {
//                 if !spawner.spawn_yielding_task() {
//                     break;
//                 }
//             }
//             continue;
//         }

//         iterations += processed;

//         // Periodically spawn new tasks to maintain load
//         if iterations % 16 == 0 {
//             spawner.spawn_yielding_task();
//         }
//     }

//     let duration = start.elapsed();
//     let actual_polls = spawner.poll_count();

//     (duration, actual_polls)
// }

// fn bench_sustained_immediate(arena: &Arena, target_polls: usize) -> (Duration, u64) {
//     let mut spawner = ContinuousSpawner::new(arena, 0);
//     let mut worker = TaskWorker::<SLOT_BITS>::new(0, arena);

//     let start = Instant::now();
//     let mut iterations = 0;

//     while iterations < target_polls {
//         // Spawn a batch
//         for _ in 0..32 {
//             spawner.spawn_immediate_task();
//         }

//         // Process them
//         let processed = worker.run_iterations(arena, 32);
//         iterations += processed;

//         if processed == 0 {
//             break;
//         }
//     }

//     let duration = start.elapsed();
//     let actual_polls = spawner.poll_count();

//     (duration, actual_polls)
// }

// fn bench_sustained_mixed(
//     arena: &Arena,
//     target_polls: usize,
//     yields_per_task: usize,
// ) -> (Duration, u64) {
//     let mut spawner = ContinuousSpawner::new(arena, yields_per_task);
//     let mut worker = TaskWorker::<SLOT_BITS>::new(0, arena);

//     // Initial spawn batch (50/50 mix)
//     for i in 0..32 {
//         if i % 2 == 0 {
//             spawner.spawn_yielding_task();
//         } else {
//             spawner.spawn_immediate_task();
//         }
//     }

//     let start = Instant::now();
//     let mut iterations = 0;

//     while iterations < target_polls {
//         let processed = worker.run_iterations(arena, 1);

//         if processed == 0 {
//             // Spawn more work
//             for i in 0..8 {
//                 if i % 2 == 0 {
//                     spawner.spawn_yielding_task();
//                 } else {
//                     spawner.spawn_immediate_task();
//                 }
//             }
//             continue;
//         }

//         iterations += processed;

//         // Maintain mixed workload
//         if iterations % 16 == 0 {
//             if iterations % 32 == 0 {
//                 spawner.spawn_yielding_task();
//             } else {
//                 spawner.spawn_immediate_task();
//             }
//         }
//     }

//     let duration = start.elapsed();
//     let actual_polls = spawner.poll_count();

//     (duration, actual_polls)
// }

// // ══════════════════════════════════════════════════════════════════════════════
// // Main
// // ══════════════════════════════════════════════════════════════════════════════

// fn print_result(name: &str, duration: Duration, polls: u64) {
//     let throughput = polls as f64 / duration.as_secs_f64();
//     let avg_ns = duration.as_nanos() as f64 / polls as f64;

//     println!("┌─────────────────────────────────────────────────────────┐");
//     println!("│ {:<55} │", name);
//     println!("├─────────────────────────────────────────────────────────┤");
//     println!("│ Total polls:         {:>15}                    │", polls);
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
//     println!("║       TaskWorker High-Throughput Benchmark Suite         ║");
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
//     println!("  Target:     1,000,000+ polls per benchmark");
//     println!();
//     println!("═══════════════════════════════════════════════════════════");
//     println!();

//     let arena = Arena::new()?;

//     // Benchmark 1: 1 million polls, yielding tasks (1 yield each)
//     println!("Running: Sustained Yielding (1 yield per task)...");
//     let (dur, polls) = bench_sustained_yielding(&arena, 1_000_000, 1);
//     print_result("Sustained Yielding - 1M polls (1 yield/task)", dur, polls);

//     // Benchmark 2: 1 million polls, yielding tasks (3 yields each)
//     println!("Running: Sustained Yielding (3 yields per task)...");
//     let (dur, polls) = bench_sustained_yielding(&arena, 1_000_000, 3);
//     print_result("Sustained Yielding - 1M polls (3 yields/task)", dur, polls);

//     // Benchmark 3: 1 million polls, immediate completion
//     println!("Running: Sustained Immediate (no yields)...");
//     let (dur, polls) = bench_sustained_immediate(&arena, 1_000_000);
//     print_result("Sustained Immediate - 1M polls (no yields)", dur, polls);

//     // Benchmark 4: 1 million polls, mixed workload
//     println!("Running: Sustained Mixed (50/50)...");
//     let (dur, polls) = bench_sustained_mixed(&arena, 1_000_000, 1);
//     print_result("Sustained Mixed - 1M polls (50% yield)", dur, polls);

//     // Benchmark 5: 10 million polls, yielding tasks
//     println!("Running: High Volume (10M polls)...");
//     let (dur, polls) = bench_sustained_yielding(&arena, 10_000_000, 1);
//     print_result("High Volume - 10M polls (1 yield/task)", dur, polls);

//     println!("═══════════════════════════════════════════════════════════");
//     println!("                    Benchmark Complete                     ");
//     println!("═══════════════════════════════════════════════════════════");

//     Ok(())
// }
