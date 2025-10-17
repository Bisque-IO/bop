// //! Comprehensive benchmark for task registration and deregistration in MmapExecutorArena.
// //!
// //! This benchmark tests:
// //! - Single-threaded allocation performance
// //! - Multi-threaded allocation performance with varying thread counts
// //! - Deallocation performance
// //! - Round-robin distribution verification
// //! - CPU-aware allocation optimization
// //! - Memory layout analysis

// use bop_executor::task::{ArenaOptions, MmapExecutorArena, Task};
// use std::sync::atomic::{AtomicU64, Ordering};
// use std::sync::{Arc, Barrier};
// use std::time::{Duration, Instant};

// // ──────────────────────────────────────────────────────────────────────────────
// // Benchmark Configuration
// // ──────────────────────────────────────────────────────────────────────────────

// /// Arena configuration: 12 bits = 4096 leaves, 12 bits = 4096 slots/leaf
// /// Total: 16,777,216 tasks (~640MB)
// const LEAF_BITS: u32 = 8;
// const SLOT_BITS: u32 = 12;

// type TestArena = MmapExecutorArena<LEAF_BITS, SLOT_BITS>;

// /// Number of tasks to allocate per benchmark
// const TASKS_PER_BENCHMARK: usize = 1_000_000;

// /// Number of warmup iterations
// const WARMUP_ITERATIONS: usize = 3;

// /// Number of benchmark iterations
// const BENCH_ITERATIONS: usize = 10;

// // ──────────────────────────────────────────────────────────────────────────────
// // Benchmark Results
// // ──────────────────────────────────────────────────────────────────────────────

// #[derive(Debug, Clone)]
// struct BenchmarkResult {
//     name: String,
//     thread_count: usize,
//     tasks_allocated: usize,
//     duration: Duration,
//     ops_per_sec: f64,
//     ns_per_op: f64,
// }

// impl BenchmarkResult {
//     fn new(name: &str, thread_count: usize, tasks: usize, duration: Duration) -> Self {
//         let secs = duration.as_secs_f64();
//         let ops_per_sec = tasks as f64 / secs;
//         let ns_per_op = duration.as_nanos() as f64 / tasks as f64;

//         Self {
//             name: name.to_string(),
//             thread_count,
//             tasks_allocated: tasks,
//             duration,
//             ops_per_sec,
//             ns_per_op,
//         }
//     }

//     fn print(&self) {
//         println!(
//             "{:<40} | threads: {:>3} | tasks: {:>8} | time: {:>8.2}ms | {:>12.0} ops/s | {:>6.1} ns/op",
//             self.name,
//             self.thread_count,
//             self.tasks_allocated,
//             self.duration.as_secs_f64() * 1000.0,
//             self.ops_per_sec,
//             self.ns_per_op
//         );
//     }
// }

// // ──────────────────────────────────────────────────────────────────────────────
// // Distribution Analysis
// // ──────────────────────────────────────────────────────────────────────────────

// struct DistributionStats {
//     leaf_counts: Vec<usize>,
//     min_tasks_per_leaf: usize,
//     max_tasks_per_leaf: usize,
//     avg_tasks_per_leaf: f64,
//     std_dev: f64,
// }

// impl DistributionStats {
//     fn analyze(arena: &TestArena, allocated_ids: &[u64]) -> Self {
//         let mut leaf_counts = vec![0usize; TestArena::LEAF_COUNT];

//         // Count tasks per leaf
//         for &global_id in allocated_ids {
//             let (leaf_idx, _) = TestArena::decompose_id(global_id);
//             leaf_counts[leaf_idx] += 1;
//         }

//         // Calculate statistics
//         let total_tasks: usize = leaf_counts.iter().sum();
//         let leaves_used = leaf_counts.iter().filter(|&&c| c > 0).count();

//         let min_tasks_per_leaf = *leaf_counts.iter().filter(|&&c| c > 0).min().unwrap_or(&0);
//         let max_tasks_per_leaf = *leaf_counts.iter().max().unwrap_or(&0);
//         let avg_tasks_per_leaf = if leaves_used > 0 {
//             total_tasks as f64 / leaves_used as f64
//         } else {
//             0.0
//         };

//         // Calculate standard deviation
//         let variance: f64 = leaf_counts
//             .iter()
//             .filter(|&&c| c > 0)
//             .map(|&c| {
//                 let diff = c as f64 - avg_tasks_per_leaf;
//                 diff * diff
//             })
//             .sum::<f64>()
//             / leaves_used as f64;
//         let std_dev = variance.sqrt();

//         Self {
//             leaf_counts,
//             min_tasks_per_leaf,
//             max_tasks_per_leaf,
//             avg_tasks_per_leaf,
//             std_dev,
//         }
//     }

//     fn print(&self, num_cpus: usize) {
//         println!("\n📊 Distribution Analysis:");
//         println!("  Primary Leaves (0-{}):", num_cpus - 1);

//         // Show distribution of first num_cpus leaves
//         for i in 0..num_cpus.min(16) {
//             println!("    Leaf {:>4}: {:>6} tasks", i, self.leaf_counts[i]);
//         }

//         if num_cpus > 16 {
//             println!("    ... ({} more primary leaves)", num_cpus - 16);
//         }

//         println!("\n  Statistics:");
//         println!("    Min tasks/leaf: {}", self.min_tasks_per_leaf);
//         println!("    Max tasks/leaf: {}", self.max_tasks_per_leaf);
//         println!("    Avg tasks/leaf: {:.2}", self.avg_tasks_per_leaf);
//         println!("    Std deviation:  {:.2}", self.std_dev);

//         let balance_ratio = if self.max_tasks_per_leaf > 0 {
//             self.min_tasks_per_leaf as f64 / self.max_tasks_per_leaf as f64
//         } else {
//             0.0
//         };
//         println!("    Balance ratio:  {:.2}% (1.0 = perfect)", balance_ratio);
//     }
// }

// // ──────────────────────────────────────────────────────────────────────────────
// // Benchmarks
// // ──────────────────────────────────────────────────────────────────────────────

// /// Benchmark 1: Single-threaded allocation
// fn bench_single_threaded_allocation(
//     arena: &TestArena,
//     num_cpus: usize,
// ) -> (BenchmarkResult, Vec<u64>) {
//     let allocator = arena.allocator(num_cpus);
//     let mut allocated_ids = Vec::with_capacity(TASKS_PER_BENCHMARK);

//     let start = Instant::now();

//     for _ in 0..TASKS_PER_BENCHMARK {
//         if let Some(global_id) = allocator.allocate() {
//             unsafe { arena.init_task(global_id) };
//             allocated_ids.push(global_id);
//         } else {
//             break;
//         }
//     }

//     let duration = start.elapsed();

//     let result = BenchmarkResult::new(
//         "Single-threaded allocation",
//         1,
//         allocated_ids.len(),
//         duration,
//     );

//     (result, allocated_ids)
// }

// /// Benchmark 2: Multi-threaded allocation
// fn bench_multi_threaded_allocation(
//     arena: &TestArena,
//     num_cpus: usize,
//     num_threads: usize,
// ) -> (BenchmarkResult, Vec<u64>) {
//     let allocator = Arc::new(arena.allocator(num_cpus));
//     let allocated_ids = Arc::new(std::sync::Mutex::new(Vec::with_capacity(
//         TASKS_PER_BENCHMARK,
//     )));
//     let barrier = Arc::new(Barrier::new(num_threads));
//     let tasks_per_thread = TASKS_PER_BENCHMARK / num_threads;

//     let start = Instant::now();

//     std::thread::scope(|s| {
//         for _ in 0..num_threads {
//             let allocator = Arc::clone(&allocator);
//             let allocated_ids = Arc::clone(&allocated_ids);
//             let barrier = Arc::clone(&barrier);

//             s.spawn(move || {
//                 // Wait for all threads to be ready
//                 barrier.wait();

//                 let mut local_ids = Vec::with_capacity(tasks_per_thread);

//                 for _ in 0..tasks_per_thread {
//                     if let Some(global_id) = allocator.allocate() {
//                         unsafe { arena.init_task(global_id) };
//                         local_ids.push(global_id);
//                     } else {
//                         break;
//                     }
//                 }

//                 // Merge into shared vector
//                 let mut ids = allocated_ids.lock().unwrap();
//                 ids.extend(local_ids);
//             });
//         }
//     });

//     let duration = start.elapsed();

//     let allocated_ids = Arc::try_unwrap(allocated_ids)
//         .unwrap()
//         .into_inner()
//         .unwrap();

//     let result = BenchmarkResult::new(
//         &format!("Multi-threaded allocation ({})", num_threads),
//         num_threads,
//         allocated_ids.len(),
//         duration,
//     );

//     (result, allocated_ids)
// }

// /// Benchmark 3: Deallocation
// fn bench_deallocation(
//     arena: &TestArena,
//     num_cpus: usize,
//     allocated_ids: Vec<u64>,
// ) -> BenchmarkResult {
//     let allocator = arena.allocator(num_cpus);
//     let count = allocated_ids.len();

//     let start = Instant::now();

//     for global_id in allocated_ids {
//         unsafe { allocator.deallocate(global_id) };
//     }

//     let duration = start.elapsed();

//     BenchmarkResult::new("Deallocation", 1, count, duration)
// }

// /// Benchmark 4: Mixed allocation/deallocation
// fn bench_mixed_operations(arena: &TestArena, num_cpus: usize) -> BenchmarkResult {
//     let allocator = arena.allocator(num_cpus);
//     let mut allocated_ids = Vec::with_capacity(TASKS_PER_BENCHMARK / 2);

//     let start = Instant::now();

//     // Allocate half
//     for _ in 0..(TASKS_PER_BENCHMARK / 2) {
//         if let Some(global_id) = allocator.allocate() {
//             unsafe { arena.init_task(global_id) };
//             allocated_ids.push(global_id);
//         }
//     }

//     // Deallocate quarter
//     let to_dealloc = allocated_ids.len() / 2;
//     for _ in 0..to_dealloc {
//         let global_id = allocated_ids.pop().unwrap();
//         unsafe { allocator.deallocate(global_id) };
//     }

//     // Allocate quarter
//     for _ in 0..(TASKS_PER_BENCHMARK / 4) {
//         if let Some(global_id) = allocator.allocate() {
//             unsafe { arena.init_task(global_id) };
//             allocated_ids.push(global_id);
//         }
//     }

//     let duration = start.elapsed();

//     // Clean up
//     for global_id in allocated_ids {
//         unsafe { allocator.deallocate(global_id) };
//     }

//     BenchmarkResult::new("Mixed alloc/dealloc", 1, TASKS_PER_BENCHMARK, duration)
// }

// /// Benchmark 5: Contention stress test
// fn bench_contention_stress(arena: &TestArena, num_cpus: usize) -> BenchmarkResult {
//     let allocator = Arc::new(arena.allocator(num_cpus));
//     let total_allocated = Arc::new(AtomicU64::new(0));
//     let num_threads = num_cpus * 2; // More threads than CPUs to stress contention
//     let barrier = Arc::new(Barrier::new(num_threads));

//     let start = Instant::now();

//     std::thread::scope(|s| {
//         for _ in 0..num_threads {
//             let allocator = Arc::clone(&allocator);
//             let total_allocated = Arc::clone(&total_allocated);
//             let barrier = Arc::clone(&barrier);

//             s.spawn(move || {
//                 barrier.wait();

//                 let mut local_ids = Vec::new();

//                 // Allocate
//                 for _ in 0..(TASKS_PER_BENCHMARK / num_threads) {
//                     if let Some(global_id) = allocator.allocate() {
//                         unsafe { arena.init_task(global_id) };
//                         local_ids.push(global_id);
//                     }
//                 }

//                 total_allocated.fetch_add(local_ids.len() as u64, Ordering::Relaxed);

//                 // Deallocate
//                 for global_id in local_ids {
//                     unsafe { allocator.deallocate(global_id) };
//                 }
//             });
//         }
//     });

//     let duration = start.elapsed();
//     let total = total_allocated.load(Ordering::Relaxed);

//     BenchmarkResult::new(
//         &format!("Contention stress ({})", num_threads),
//         num_threads,
//         total as usize * 2, // Count both alloc + dealloc
//         duration,
//     )
// }

// // ──────────────────────────────────────────────────────────────────────────────
// // Main
// // ──────────────────────────────────────────────────────────────────────────────

// fn main() -> std::io::Result<()> {
//     println!("╔═══════════════════════════════════════════════════════════════════════════╗");
//     println!("║          Task Allocation Benchmark - MmapExecutorArena                    ║");
//     println!("╚═══════════════════════════════════════════════════════════════════════════╝\n");

//     // Get CPU count
//     let num_cpus = num_cpus::get();
//     println!("🖥️  System Info:");
//     println!("  CPU cores:      {}", num_cpus);
//     println!(
//         "  Arena config:   {} leaves × {} slots = {} total tasks",
//         TestArena::LEAF_COUNT,
//         TestArena::SLOT_COUNT,
//         TestArena::TOTAL_TASKS
//     );
//     println!(
//         "  Arena size:     {:.2} MB",
//         TestArena::TOTAL_SIZE as f64 / 1024.0 / 1024.0
//     );
//     println!("  Primary leaves: {}", num_cpus.min(TestArena::LEAF_COUNT));
//     println!("  Tasks/bench:    {}\n", TASKS_PER_BENCHMARK);

//     // Create arena with optimal settings
//     println!("📦 Creating arena...");
//     let arena = match TestArena::with_options(ArenaOptions::low_latency()) {
//         Ok(arena) => {
//             println!("✅ Arena created with huge pages\n");
//             arena
//         }
//         Err(_) => {
//             println!("⚠️  Huge pages not available, using regular pages");
//             TestArena::new()?
//         }
//     };

//     println!("═══════════════════════════════════════════════════════════════════════════════");
//     println!("                              BENCHMARK RESULTS");
//     println!("═══════════════════════════════════════════════════════════════════════════════\n");

//     let mut all_results = Vec::new();

//     // ──────────────────────────────────────────────────────────────────────────
//     // Benchmark 1: Single-threaded allocation
//     // ──────────────────────────────────────────────────────────────────────────
//     println!("🔄 Running: Single-threaded allocation...");
//     let (result, allocated_ids) = bench_single_threaded_allocation(&arena, num_cpus);
//     result.print();
//     all_results.push(result);

//     // Analyze distribution
//     let dist_stats = DistributionStats::analyze(&arena, &allocated_ids);
//     dist_stats.print(num_cpus);

//     // Clean up
//     println!("\n🧹 Cleaning up allocated tasks...");
//     let cleanup_result = bench_deallocation(&arena, num_cpus, allocated_ids);
//     cleanup_result.print();
//     all_results.push(cleanup_result);

//     println!();

//     // ──────────────────────────────────────────────────────────────────────────
//     // Benchmark 2: Multi-threaded allocation (varying thread counts)
//     // ──────────────────────────────────────────────────────────────────────────
//     for &num_threads in &[2, 4, 8, 16, 32] {
//         if num_threads > num_cpus * 4 {
//             break; // Skip excessive thread counts
//         }

//         println!(
//             "🔄 Running: Multi-threaded allocation ({} threads)...",
//             num_threads
//         );
//         let (result, allocated_ids) =
//             bench_multi_threaded_allocation(&arena, num_cpus, num_threads);
//         result.print();
//         all_results.push(result);

//         // Clean up
//         let cleanup_result = bench_deallocation(&arena, num_cpus, allocated_ids);
//         cleanup_result.print();
//         all_results.push(cleanup_result);
//         println!();
//     }

//     // ──────────────────────────────────────────────────────────────────────────
//     // Benchmark 3: Mixed operations
//     // ──────────────────────────────────────────────────────────────────────────
//     println!("🔄 Running: Mixed allocation/deallocation...");
//     let result = bench_mixed_operations(&arena, num_cpus);
//     result.print();
//     all_results.push(result);
//     println!();

//     // ──────────────────────────────────────────────────────────────────────────
//     // Benchmark 4: Contention stress test
//     // ──────────────────────────────────────────────────────────────────────────
//     println!("🔄 Running: Contention stress test...");
//     let result = bench_contention_stress(&arena, num_cpus);
//     result.print();
//     all_results.push(result);
//     println!();

//     // ──────────────────────────────────────────────────────────────────────────
//     // Summary
//     // ──────────────────────────────────────────────────────────────────────────
//     println!("═══════════════════════════════════════════════════════════════════════════════");
//     println!("                              SUMMARY");
//     println!("═══════════════════════════════════════════════════════════════════════════════\n");

//     // Find fastest allocation
//     let fastest_alloc = all_results
//         .iter()
//         .filter(|r| r.name.contains("allocation") && !r.name.contains("dealloc"))
//         .max_by(|a, b| a.ops_per_sec.partial_cmp(&b.ops_per_sec).unwrap())
//         .unwrap();

//     println!(
//         "🏆 Fastest allocation: {} ({:.0} ops/s)",
//         fastest_alloc.name, fastest_alloc.ops_per_sec
//     );

//     // Find fastest deallocation
//     let fastest_dealloc = all_results
//         .iter()
//         .filter(|r| r.name.contains("Deallocation"))
//         .max_by(|a, b| a.ops_per_sec.partial_cmp(&b.ops_per_sec).unwrap())
//         .unwrap();

//     println!(
//         "🏆 Fastest deallocation: {} ({:.0} ops/s)",
//         fastest_dealloc.name, fastest_dealloc.ops_per_sec
//     );

//     println!("\n✅ All benchmarks completed successfully!\n");

//     Ok(())
// }
