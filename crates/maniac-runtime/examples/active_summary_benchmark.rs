// use maniac_runtime::runtime::task::{TaskArena, TaskArenaConfig, TaskArenaOptions};
// use std::sync::Arc;
// use std::thread;
// use std::time::Instant;
//
// #[derive(Clone, Copy)]
// enum Pattern {
//     ReserveRelease,
//     ActivateDeactivate,
//     YieldHeavy,
//     Mixed,
// }
//
// struct Scenario {
//     name: &'static str,
//     leaf_count: usize,
//     max_tasks: usize,
//     threads: usize,
//     ops_per_thread: usize,
//     pattern: Pattern,
// }
//
// fn run_scenario(s: &Scenario) {
//     println!(
//         "\n=== {}: leaves={}, tasks={}, threads={}, ops/thread={} ===",
//         s.name, s.leaf_count, s.max_tasks, s.threads, s.ops_per_thread
//     );
//
//     let config = TaskArenaConfig::new(s.leaf_count, s.max_tasks)
//         .expect("invalid arena configuration")
//         .with_max_workers(s.leaf_count);
//
//     let arena = Arc::new(
//         TaskArena::with_config(config, TaskArenaOptions::default())
//             .expect("failed to create arena"),
//     );
//
//     let start = Instant::now();
//     thread::scope(|scope| {
//         for worker_id in 0..s.threads {
//             let arena = Arc::clone(&arena);
//             scope.spawn(move || {
//                 // TODO: Worker reservation removed - this benchmark needs updating
//                 let worker_slot = worker_id;
//
//                 for iter in 0..s.ops_per_thread {
//                     match s.pattern {
//                         Pattern::ReserveRelease => reserve_release(&arena),
//                         Pattern::ActivateDeactivate => activate_deactivate(&arena),
//                         Pattern::YieldHeavy => yield_flip(&arena, worker_slot),
//                         Pattern::Mixed => {
//                             if iter % 3 == 0 {
//                                 reserve_release(&arena);
//                             } else if iter % 3 == 1 {
//                                 activate_deactivate(&arena);
//                             } else {
//                                 yield_flip(&arena, worker_slot);
//                             }
//                         }
//                     }
//                 }
//             });
//         }
//     });
//
//     let duration = start.elapsed();
//     let total_ops = s.ops_per_thread as u64 * s.threads as u64;
//     let throughput = total_ops as f64 / duration.as_secs_f64();
//
//     let tree = arena.active_tree();
//     while tree.try_acquire() {}
//     assert_eq!(
//         tree.snapshot_summary(),
//         0,
//         "summary should be idle at end of scenario"
//     );
//
//     println!(
//         "  duration: {:>8.3} ms  throughput: {:>12.0} ops/s",
//         duration.as_secs_f64() * 1000.0,
//         throughput
//     );
// }
//
// fn reserve_release(arena: &TaskArena) {
//     if let Some(handle) = arena.reserve_task() {
//         arena.release_task(handle);
//     }
// }
//
// fn activate_deactivate(arena: &TaskArena) {
//     if let Some(handle) = arena.reserve_task() {
//         arena.activate_task(handle);
//         arena.deactivate_task(handle);
//         arena.release_task(handle);
//     }
// }
//
// fn yield_flip(arena: &TaskArena, worker_slot: usize) {
//     // TODO: This benchmark needs update - yield bits moved to WorkerService
//     let _ = (arena, worker_slot);
// }
//
// fn main() {
//     let scenarios = [
//         Scenario {
//             name: "reserve-release",
//             leaf_count: 64,
//             max_tasks: 16_384,
//             threads: 8,
//             ops_per_thread: 1_000_000,
//             pattern: Pattern::ReserveRelease,
//         },
//         Scenario {
//             name: "activate-deactivate",
//             leaf_count: 128,
//             max_tasks: 32_768,
//             threads: 8,
//             ops_per_thread: 500_000,
//             pattern: Pattern::ActivateDeactivate,
//         },
//         Scenario {
//             name: "yield-heavy",
//             leaf_count: 256,
//             max_tasks: 16_384,
//             threads: 16,
//             ops_per_thread: 250_000,
//             pattern: Pattern::YieldHeavy,
//         },
//         Scenario {
//             name: "mixed",
//             leaf_count: 128,
//             max_tasks: 24_000,
//             threads: 8,
//             ops_per_thread: 750_000,
//             pattern: Pattern::Mixed,
//         },
//     ];
//
//     for scenario in &scenarios {
//         run_scenario(scenario);
//     }
//
//     println!("\nBenchmark complete.");
// }

fn main() {}
