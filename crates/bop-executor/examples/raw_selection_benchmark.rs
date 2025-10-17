// //! Raw task selection benchmark - measures ONLY the selection overhead
// //!
// //! This benchmark tests the core selection mechanism without async overhead:
// //! 1. SignalWaker selects a leaf (level 1)
// //! 2. SignalLeaf selects a signal (level 2)
// //! 3. TaskSignal selects and acquires a bit (level 3)
// //! 4. Immediately flip the bit back (simulating task completion)
// //!
// //! Tests multiple scenarios:
// //! - Different leaf/slot configurations
// //! - Different task densities (sparse vs dense)
// //! - Single-threaded vs multi-threaded
// //! - Uniform vs skewed distributions
// //!
// //! Goal: Determine if raw selection can achieve < 20ns

// use bop_executor::selector::Selector;
// use bop_executor::task::MmapExecutorArena;
// use std::cell::UnsafeCell;
// use std::sync::Arc;
// use std::sync::atomic::{AtomicU64, Ordering};
// use std::thread;
// use std::time::{Duration, Instant};

// thread_local! {
//     static SELECTOR: UnsafeCell<Selector> = UnsafeCell::new(Selector::new());
// }

// // Configuration struct for benchmark scenarios
// #[derive(Debug, Clone)]
// struct BenchmarkConfig {
//     name: &'static str,
//     leaf_bits: u32,
//     slot_bits: u32,
//     num_threads: usize,
//     selections_per_thread: usize,
//     active_task_percent: f64, // 0.0 to 1.0
// }

// impl BenchmarkConfig {
//     fn leaf_count(&self) -> usize {
//         1 << self.leaf_bits
//     }

//     fn slot_count(&self) -> usize {
//         1 << self.slot_bits
//     }

//     fn total_tasks(&self) -> usize {
//         self.leaf_count() * self.slot_count()
//     }

//     fn num_active_tasks(&self) -> usize {
//         (self.total_tasks() as f64 * self.active_task_percent) as usize
//     }
// }

// /// Raw selection benchmark - just select, acquire, and release
// fn bench_raw_selection(config: &BenchmarkConfig) -> (Duration, u64, u64, u64, u64) {
//     // Create arena based on config
//     match (config.leaf_bits, config.slot_bits) {
//         (8, 8) => bench_raw_selection_typed::<8, 8>(config),
//         (10, 10) => bench_raw_selection_typed::<10, 10>(config),
//         (12, 12) => bench_raw_selection_typed::<12, 12>(config),
//         (8, 10) => bench_raw_selection_typed::<8, 10>(config),
//         (10, 8) => bench_raw_selection_typed::<10, 8>(config),
//         _ => panic!("Unsupported leaf_bits/slot_bits combination"),
//     }
// }

// fn bench_raw_selection_typed<const LEAF_BITS: u32, const SLOT_BITS: u32>(
//     config: &BenchmarkConfig,
// ) -> (Duration, u64, u64, u64, u64) {
//     let arena = MmapExecutorArena::<LEAF_BITS, SLOT_BITS>::new().expect("Failed to create arena");
//     let selections_counter = Arc::new(AtomicU64::new(0));
//     let failures_counter = Arc::new(AtomicU64::new(0));
//     let contention_counter = Arc::new(AtomicU64::new(0));
//     let misses_counter = Arc::new(AtomicU64::new(0));

//     // Initialize tasks: schedule them so their slots are active
//     let num_active = config.num_active_tasks();
//     let total_tasks = config.total_tasks();

//     println!(
//         "  Initializing {} active tasks out of {} total...",
//         num_active, total_tasks
//     );

//     // Distribute tasks evenly across the arena
//     for i in 0..num_active {
//         let global_id = ((i * total_tasks) / num_active) as u64;
//         unsafe {
//             arena.init_task(global_id);
//             let task = arena.task(global_id);
//             task.schedule::<SLOT_BITS>();
//         }
//     }

//     // Spawn worker threads
//     let mut handles = vec![];
//     let start_barrier = Arc::new(std::sync::Barrier::new(config.num_threads + 1));

//     // Wrap arena in Arc to share across threads safely
//     let arena = Arc::new(arena);

//     for _thread_id in 0..config.num_threads {
//         let arena = Arc::clone(&arena);
//         let selections = Arc::clone(&selections_counter);
//         let failures = Arc::clone(&failures_counter);
//         let contention = Arc::clone(&contention_counter);
//         let misses = Arc::clone(&misses_counter);
//         let barrier = Arc::clone(&start_barrier);
//         let selections_per_thread = config.selections_per_thread;

//         let handle = thread::spawn(move || {
//             let mut local_selections = 0u64;
//             let mut local_failures = 0u64;

//             // Wait for all threads to be ready
//             barrier.wait();

//             // Get thread-local selector
//             SELECTOR.with(|selector| {
//                 let selector = unsafe { &mut *selector.get() };

//                 // Perform raw selections
//                 for _ in 0..selections_per_thread {
//                     match select_acquire_release::<LEAF_BITS, SLOT_BITS>(&arena, selector) {
//                         Some(_) => local_selections += 1,
//                         None => local_failures += 1,
//                     }
//                 }

//                 // Collect contention stats
//                 contention.fetch_add(selector.contention, Ordering::Relaxed);
//                 misses.fetch_add(selector.misses, Ordering::Relaxed);
//             });

//             selections.fetch_add(local_selections, Ordering::Relaxed);
//             failures.fetch_add(local_failures, Ordering::Relaxed);
//         });

//         handles.push(handle);
//     }

//     // Start all threads simultaneously
//     start_barrier.wait();
//     let start = Instant::now();

//     // Wait for all threads to complete
//     for handle in handles {
//         handle.join().unwrap();
//     }

//     let duration = start.elapsed();
//     let total_selections = selections_counter.load(Ordering::Relaxed);
//     let total_failures = failures_counter.load(Ordering::Relaxed);
//     let total_contention = contention_counter.load(Ordering::Relaxed);
//     let total_misses = misses_counter.load(Ordering::Relaxed);

//     (
//         duration,
//         total_selections,
//         total_failures,
//         total_contention,
//         total_misses,
//     )
// }

// /// Core selection function: Single-level flat TaskSignal array
// ///
// /// READ-ONLY benchmark: measures pure selection overhead without atomic writes.
// /// - 1 random number to pick a TaskSignal
// /// - 1 Relaxed load of TaskSignal
// /// - If 0, try different signal; otherwise find_nearest
// /// - Returns immediately (no acquire, no release)
// fn select_acquire_release<const LEAF_BITS: u32, const SLOT_BITS: u32>(
//     arena: &MmapExecutorArena<LEAF_BITS, SLOT_BITS>,
//     selector: &mut Selector,
// ) -> Option<u64> {
//     use std::sync::atomic::Ordering;

//     let leaf_count = 1 << LEAF_BITS;
//     let signals_per_leaf = (1 << SLOT_BITS) / 64;
//     let total_signals = leaf_count * signals_per_leaf;

//     // 1. Single Selector::next() to get random values
//     let rand = selector.next();
//     let random_signal_idx = (rand as usize) % total_signals;
//     let random_bit = (rand >> 16) & 63;

//     // Try a few different signals (4 attempts)
//     for signal_offset in 0..4 {
//         let signal_idx = (random_signal_idx + signal_offset) % total_signals;

//         // Compute leaf and local signal index
//         let leaf_idx = signal_idx / signals_per_leaf;
//         let local_signal_idx = signal_idx % signals_per_leaf;

//         let leaf = unsafe { &*arena.leaf(leaf_idx) };
//         let signal = unsafe { &*leaf.active_signals_ptr().add(local_signal_idx) };

//         // 2. Single Relaxed load of TaskSignal
//         let signal_value = signal.load(Ordering::Relaxed);

//         if signal_value == 0 {
//             selector.misses += 1;
//             continue; // Try different signal
//         }

//         // 3. Found work - use find_nearest and try_acquire
//         let bit = bop_executor::bits::find_nearest(signal_value, random_bit);

//         if bit >= 64 {
//             selector.misses += 1;
//             continue;
//         }

//         // Try to acquire
//         if signal.acquire(bit) {
//             let slot_idx = local_signal_idx * 64 + bit as usize;
//             let global_id = ((leaf_idx << SLOT_BITS) | slot_idx) as u64;

//             // IMMEDIATELY RELEASE - simulate task completion
//             signal.set(bit);

//             return Some(global_id);
//         }

//         // Acquire failed - contention!
//         selector.contention += 1;

//         // Try another bit in same signal
//         let retry_bit = (random_bit + 17) & 63;
//         let bit2 = bop_executor::bits::find_nearest(signal_value, retry_bit);

//         if bit2 < 64 && signal.acquire(bit2) {
//             let slot_idx = local_signal_idx * 64 + bit2 as usize;
//             let global_id = ((leaf_idx << SLOT_BITS) | slot_idx) as u64;

//             signal.set(bit2);
//             return Some(global_id);
//         } else if bit2 < 64 {
//             selector.contention += 1;
//         }
//     }

//     None
// }

// fn format_duration(ns: f64) -> String {
//     if ns < 1000.0 {
//         format!("{:.1} ns", ns)
//     } else if ns < 1_000_000.0 {
//         format!("{:.1} μs", ns / 1000.0)
//     } else {
//         format!("{:.1} ms", ns / 1_000_000.0)
//     }
// }

// fn print_results(
//     config: &BenchmarkConfig,
//     duration: Duration,
//     selections: u64,
//     failures: u64,
//     contention: u64,
//     misses: u64,
// ) {
//     let total_attempts = selections + failures;
//     let total_ns = duration.as_nanos() as f64;

//     // Calculate per-thread time: each thread did (total_attempts / num_threads) selections
//     // in the measured wall-clock time
//     let attempts_per_thread = total_attempts as f64 / config.num_threads as f64;
//     let avg_ns_per_thread = total_ns / attempts_per_thread;

//     // Total throughput across all threads
//     let total_throughput = (total_attempts as f64 / total_ns) * 1_000_000_000.0;

//     let success_rate = (selections as f64 / total_attempts as f64) * 100.0;
//     let contention_rate = (contention as f64 / total_attempts as f64) * 100.0;
//     let miss_rate = (misses as f64 / total_attempts as f64) * 100.0;

//     println!("\n{}", config.name);
//     println!("  ┌─────────────────────────────────────────────────────────┐");
//     println!("  │ Configuration                                           │");
//     println!(
//         "  │   Leaf bits:       {}  ({} leaves)                    │",
//         config.leaf_bits,
//         config.leaf_count()
//     );
//     println!(
//         "  │   Slot bits:       {}  ({} slots/leaf)              │",
//         config.slot_bits,
//         config.slot_count()
//     );
//     println!(
//         "  │   Total capacity:  {} tasks                      │",
//         config.total_tasks()
//     );
//     println!(
//         "  │   Active tasks:    {} ({:.1}%)                   │",
//         config.num_active_tasks(),
//         config.active_task_percent * 100.0
//     );
//     println!(
//         "  │   Threads:         {}                                  │",
//         config.num_threads
//     );
//     println!("  ├─────────────────────────────────────────────────────────┤");
//     println!("  │ Results                                                 │");
//     println!(
//         "  │   Total time:      {:>8}                           │",
//         format_duration(total_ns)
//     );
//     println!(
//         "  │   Selections:      {:>10}                         │",
//         selections
//     );
//     println!(
//         "  │   Failures:        {:>10}                         │",
//         failures
//     );
//     println!(
//         "  │   Success rate:    {:>6.1}%                           │",
//         success_rate
//     );
//     println!("  ├─────────────────────────────────────────────────────────┤");
//     println!("  │ Contention Statistics                                   │");
//     println!(
//         "  │   Contention:      {:>10}  ({:>5.1}%)               │",
//         contention, contention_rate
//     );
//     println!(
//         "  │   Misses:          {:>10}  ({:>5.1}%)               │",
//         misses, miss_rate
//     );
//     println!("  ├─────────────────────────────────────────────────────────┤");
//     println!("  │ Performance (per thread)                                │");
//     println!(
//         "  │   Avg time/op:     {:>8}                           │",
//         format_duration(avg_ns_per_thread)
//     );
//     println!(
//         "  │   Per-thread rate: {:>10.0} ops/sec                │",
//         1_000_000_000.0 / avg_ns_per_thread
//     );
//     println!("  ├─────────────────────────────────────────────────────────┤");
//     println!("  │ Aggregate Performance (all threads)                     │");
//     println!(
//         "  │   Total throughput:{:>10.0} ops/sec                │",
//         total_throughput
//     );

//     if avg_ns_per_thread < 20.0 {
//         println!("  ├─────────────────────────────────────────────────────────┤");
//         println!("  │   Status:          ✓ MEETS < 20ns requirement          │");
//     } else {
//         println!("  ├─────────────────────────────────────────────────────────┤");
//         println!("  │   Status:          ✗ FAILS < 20ns requirement          │");
//     }

//     if avg_ns_per_thread < 10.0 {
//         println!("  │                    ✓ MEETS < 10ns stretch goal         │");
//     }

//     println!("  └─────────────────────────────────────────────────────────┘");
// }

// fn main() {
//     println!("╔═══════════════════════════════════════════════════════════╗");
//     println!("║           Raw Task Selection Benchmark                    ║");
//     println!("╠═══════════════════════════════════════════════════════════╣");
//     println!("║ Measures ONLY selection overhead (no async, no polling)  ║");
//     println!("║ Operation: Select → Acquire → Release → Repeat           ║");
//     println!("║ Goal: Determine if < 20ns selection is possible          ║");
//     println!("╚═══════════════════════════════════════════════════════════╝\n");

//     let scenarios = vec![
//         // // ============ DENSITY TESTS: Single-threaded across different sparsity levels ============
//         // BenchmarkConfig {
//         //     name: "Single-thread, Very Dense (8×8, 100% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 1,
//         //     selections_per_thread: 1_000_000,
//         //     active_task_percent: 1.0,
//         // },
//         // BenchmarkConfig {
//         //     name: "Single-thread, Dense (8×8, 75% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 1,
//         //     selections_per_thread: 1_000_000,
//         //     active_task_percent: 0.75,
//         // },
//         // BenchmarkConfig {
//         //     name: "Single-thread, Medium (8×8, 50% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 1,
//         //     selections_per_thread: 1_000_000,
//         //     active_task_percent: 0.5,
//         // },
//         // BenchmarkConfig {
//         //     name: "Single-thread, Sparse (8×8, 25% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 1,
//         //     selections_per_thread: 1_000_000,
//         //     active_task_percent: 0.25,
//         // },
//         // BenchmarkConfig {
//         //     name: "Single-thread, Very Sparse (8×8, 10% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 1,
//         //     selections_per_thread: 1_000_000,
//         //     active_task_percent: 0.1,
//         // },
//         // BenchmarkConfig {
//         //     name: "Single-thread, Extremely Sparse (8×8, 1% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 1,
//         //     selections_per_thread: 1_000_000,
//         //     active_task_percent: 0.01,
//         // },
//         // // ============ MULTI-THREADED DENSITY TESTS: 4 threads ============
//         // BenchmarkConfig {
//         //     name: "4-thread, Very Dense (8×8, 100% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 1.0,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Dense (8×8, 75% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.75,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Medium (8×8, 50% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.5,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Sparse (8×8, 25% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.25,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Very Sparse (8×8, 10% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.1,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Extremely Sparse (8×8, 1% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 8,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.01,
//         // },
//         // // ============ LARGE ARENA DENSITY TESTS ============
//         // BenchmarkConfig {
//         //     name: "4-thread, Large Dense (10×10, 100% active)",
//         //     leaf_bits: 10,
//         //     slot_bits: 10,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 1.0,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Large Medium (10×10, 50% active)",
//         //     leaf_bits: 10,
//         //     slot_bits: 10,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.5,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Large Sparse (10×10, 10% active)",
//         //     leaf_bits: 10,
//         //     slot_bits: 10,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.1,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Large Very Sparse (10×10, 1% active)",
//         //     leaf_bits: 10,
//         //     slot_bits: 10,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.01,
//         // },
//         // // ============ HIGH THREAD COUNT TESTS ============
//         // BenchmarkConfig {
//         //     name: "8-thread, Dense (10×10, 100% active)",
//         //     leaf_bits: 10,
//         //     slot_bits: 10,
//         //     num_threads: 8,
//         //     selections_per_thread: 125_000,
//         //     active_task_percent: 1.0,
//         // },
//         // BenchmarkConfig {
//         //     name: "8-thread, Medium (10×10, 50% active)",
//         //     leaf_bits: 10,
//         //     slot_bits: 10,
//         //     num_threads: 8,
//         //     selections_per_thread: 125_000,
//         //     active_task_percent: 0.5,
//         // },
//         // BenchmarkConfig {
//         //     name: "8-thread, Sparse (10×10, 10% active)",
//         //     leaf_bits: 10,
//         //     slot_bits: 10,
//         //     num_threads: 8,
//         //     selections_per_thread: 125_000,
//         //     active_task_percent: 0.1,
//         // },
//         // // ============ PRODUCTION SIZE TESTS (12×12 = 16M tasks) ============
//         // BenchmarkConfig {
//         //     name: "4-thread, Production Dense (12×12, 50% active)",
//         //     leaf_bits: 12,
//         //     slot_bits: 12,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.5,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Production Sparse (12×12, 10% active)",
//         //     leaf_bits: 12,
//         //     slot_bits: 12,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.1,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Production Very Sparse (12×12, 1% active)",
//         //     leaf_bits: 12,
//         //     slot_bits: 12,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.01,
//         // },
//         // // ============ SHAPE TESTS: Wide vs Deep with various densities ============
//         // BenchmarkConfig {
//         //     name: "4-thread, Wide Dense (10×8, 100% active)",
//         //     leaf_bits: 10,
//         //     slot_bits: 8,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 1.0,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Wide Sparse (10×8, 10% active)",
//         //     leaf_bits: 10,
//         //     slot_bits: 8,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.1,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Deep Dense (8×10, 100% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 10,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 1.0,
//         // },
//         // BenchmarkConfig {
//         //     name: "4-thread, Deep Sparse (8×10, 10% active)",
//         //     leaf_bits: 8,
//         //     slot_bits: 10,
//         //     num_threads: 4,
//         //     selections_per_thread: 250_000,
//         //     active_task_percent: 0.1,
//         // },
//         BenchmarkConfig {
//             name: "32-thread, Wide Dense (10×8, 100% active)",
//             leaf_bits: 10,
//             slot_bits: 10,
//             num_threads: 8,
//             selections_per_thread: 1000_000,
//             active_task_percent: 0.1,
//         },
//     ];

//     for config in &scenarios {
//         let (duration, selections, failures, contention, misses) = bench_raw_selection(config);
//         print_results(config, duration, selections, failures, contention, misses);
//     }

//     println!("\n╔═══════════════════════════════════════════════════════════╗");
//     println!("║                         Summary                           ║");
//     println!("╠═══════════════════════════════════════════════════════════╣");
//     println!("║ This benchmark measures ONLY the selection mechanism:    ║");
//     println!("║   • 3-level bitmap scanning (waker → leaf → signal)      ║");
//     println!("║   • Random starting point for fairness                   ║");
//     println!("║   • Atomic acquire/release operations                    ║");
//     println!("║   • Multi-threaded contention                            ║");
//     println!("║                                                           ║");
//     println!("║ Does NOT include:                                        ║");
//     println!("║   • Waker creation/destruction                           ║");
//     println!("║   • Future polling overhead                              ║");
//     println!("║   • Context switching                                    ║");
//     println!("║   • Actual task work                                     ║");
//     println!("║                                                           ║");
//     println!("║ If this achieves < 20ns: Selection is not the bottleneck║");
//     println!("║ If this fails < 20ns: Selection fundamentally too slow  ║");
//     println!("╚═══════════════════════════════════════════════════════════╝\n");
// }
