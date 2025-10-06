//! SPSC Queue Benchmark
//!
//! Stress test for the SPSC (Single Producer Single Consumer) queue implementation.
//! Measures throughput and latency of enqueue/dequeue operations.
//!
//! SegSpsc benchmark uses true SPSC pattern with Arc for shared access:
//! - Producer thread runs on its own thread
//! - Consumer runs on the current thread immediately after spawning the producer
//! - Both threads share access via Arc

use bop_mpmc::seg_spsc::SegSpsc;
use bop_mpmc::spsc::{HeapBuffer, SpscQueue};

use std::sync::Arc;

use std::thread;
use std::time::{Duration, Instant};

fn humanize_number(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn format_cache_performance(pool_reuses: u64, fresh_allocs: u64) -> String {
    if pool_reuses == 0 && fresh_allocs == 0 {
        "No allocations recorded".to_string()
    } else {
        format!(
            "Pool reuses: {} | Fresh allocations: {} | Hit rate: {:.1}%",
            pool_reuses,
            fresh_allocs,
            (pool_reuses as f64 / (pool_reuses + fresh_allocs) as f64) * 100.0
        )
    }
}

fn format_memory_efficiency(pool_reuses: u64, fresh_allocs: u64) -> Option<String> {
    let total = pool_reuses + fresh_allocs;
    if total > 0 {
        Some(format!(
            "Memory efficiency: {:.1}%",
            (pool_reuses as f64 / total as f64) * 100.0
        ))
    } else {
        None
    }
}

fn stress_test_dro_spsc_push<const P: usize>(iters: usize) {
    let _queue_size = P;

    println!(
        "\nRunning DRO SPSC stress test: SIZE = {}, ITERS = {}",
        P,
        humanize_number(iters as u64)
    );

    let queue = Arc::new(SpscQueue::<usize, HeapBuffer<usize>>::new_heap(P));
    let start = Instant::now();

    let producer = {
        let queue = queue.clone();
        thread::spawn(move || {
            for i in 0..iters {
                queue.push(i);
            }
        })
    };

    // Consumer on main thread
    let mut count = 0;
    while count < iters {
        queue.pop();
        count += 1;
    }

    producer.join().unwrap();
    let duration = start.elapsed();
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "DroSpsc<{}>: {:>10.2} ns/op    {:>15} ops/sec    Capacity: {}",
        P,
        ns_per_op,
        humanize_number(ops_per_sec),
        _queue_size
    );
}

fn drain_with_benchmark_dro<const P: usize>(iters: usize) {
    let _queue_size = P;

    println!(
        "\nRunning DRO SPSC drain benchmark: SIZE = {}, ITERS = {}",
        P,
        humanize_number(iters as u64)
    );

    let queue = Arc::new(SpscQueue::<usize, HeapBuffer<usize>>::new_heap(P));
    let start = Instant::now();

    let producer = {
        let queue = queue.clone();
        thread::spawn(move || {
            for i in 0..iters {
                queue.push(i);
            }
        })
    };

    // Consumer on main thread
    let mut count = 0;
    while count < iters {
        queue.pop();
        count += 1;

        // let (drained, _) = queue.drain_with(|_| {}, 1024);
        // count += drained;
    }

    let duration = start.elapsed();
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "DroDrain<{}>: {:>10.2} ns/op    {:>15} ops/sec    Capacity: {}",
        P,
        ns_per_op,
        humanize_number(ops_per_sec),
        _queue_size
    );
}

// SegSpsc benchmark functions
fn stress_test_seg_spsc_push_slow<const P: usize, const NUM_SEGS_P2: usize>(
    iters: usize,
    rate: usize,
    sleep: Duration,
) {
    let _queue_size = (1 << P) * (1 << NUM_SEGS_P2);

    println!(
        "\nRunning SegSpsc stress test: P = {}, NUM_SEGS = {}, ITERS = {}",
        P,
        1 << NUM_SEGS_P2,
        humanize_number(iters as u64)
    );

    // SegSpsc benchmark with Arc and concurrent access
    let queue = Arc::new(SegSpsc::<usize, P, NUM_SEGS_P2>::new());
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer = {
        let queue = queue.clone();
        thread::spawn(move || {
            for i in 0..iters {
                while queue.try_push(i).is_err() {
                    std::hint::spin_loop();
                }
                if i % rate == 0 {
                    std::thread::sleep(sleep);
                }
            }
        })
    };

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    while count < iters {
        if queue.try_pop().is_some() {
            count += 1;
        } else {
            std::hint::spin_loop();
        }
    }

    producer.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = queue.fresh_allocations();
    let pool_reuses = queue.pool_reuses();

    println!(
        "SegSpsc<P={}, SEG={}>: {:>10.2} ns/op    {:>15} ops/sec    Capacity: {}",
        P,
        1 << NUM_SEGS_P2,
        ns_per_op,
        humanize_number(ops_per_sec),
        _queue_size
    );

    // Debug info to understand allocation patterns
    if fresh_allocs == 0 && pool_reuses == 0 {
        println!(
            "  🤔 No allocations occurred - test may be too small or pattern not triggering segment allocation"
        );
    } else if pool_reuses == 0 {
        println!(
            "  🔍 Cache hits: 0 - segments not being reused. Check if patterns cross segment boundaries enough."
        );
    }

    println!(
        "  ↳ {}",
        format_cache_performance(pool_reuses, fresh_allocs)
    );

    if let Some(efficiency) = format_memory_efficiency(pool_reuses, fresh_allocs) {
        println!("  ↳ {}", efficiency);
    }
}

// SegSpsc benchmark functions
fn stress_test_seg_spsc_push<const P: usize, const NUM_SEGS_P2: usize>(iters: usize) {
    let _queue_size = (1 << P) * (1 << NUM_SEGS_P2);

    println!(
        "\nRunning SegSpsc stress test: P = {}, NUM_SEGS = {}, ITERS = {}",
        P,
        1 << NUM_SEGS_P2,
        humanize_number(iters as u64)
    );

    {
        let iters = 1_000_000;
        // SegSpsc benchmark with Arc and concurrent access
        let queue = Arc::new(SegSpsc::<usize, P, NUM_SEGS_P2>::new());
        let start = Instant::now();

        // Producer thread - starts on its own thread
        let producer = {
            let queue = queue.clone();
            thread::spawn(move || {
                for i in 0..iters {
                    while queue.try_push(i).is_err() {
                        std::hint::spin_loop();
                    }
                }
            })
        };

        // Consumer on main thread - runs immediately after spawning producer
        let mut count = 0;
        while count < iters {
            if queue.try_pop().is_some() {
                count += 1;
            } else {
                std::hint::spin_loop();
            }
        }

        producer.join().unwrap();
    }

    // SegSpsc benchmark with Arc and concurrent access
    let queue = Arc::new(SegSpsc::<usize, P, NUM_SEGS_P2>::new());
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer = {
        let queue = queue.clone();
        thread::spawn(move || {
            for i in 0..iters {
                while queue.try_push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        })
    };

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    while count < iters {
        if queue.try_pop().is_some() {
            count += 1;
        } else {
            std::hint::spin_loop();
        }
    }

    producer.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = queue.fresh_allocations();
    let pool_reuses = queue.pool_reuses();

    println!(
        "SegSpsc<P={}, SEG={}>: {:>10.2} ns/op    {:>15} ops/sec    Capacity: {}",
        P,
        1 << NUM_SEGS_P2,
        ns_per_op,
        humanize_number(ops_per_sec),
        _queue_size
    );

    // Debug info to understand allocation patterns
    if fresh_allocs == 0 && pool_reuses == 0 {
        println!(
            "  🤔 No allocations occurred - test may be too small or pattern not triggering segment allocation"
        );
    } else if pool_reuses == 0 {
        println!(
            "  🔍 Cache hits: 0 - segments not being reused. Check if patterns cross segment boundaries enough."
        );
    }

    println!(
        "  ↳ {}",
        format_cache_performance(pool_reuses, fresh_allocs)
    );

    if let Some(efficiency) = format_memory_efficiency(pool_reuses, fresh_allocs) {
        println!("  ↳ {}", efficiency);
    }
}

fn stress_test_seg_spsc_batch_pop<const P: usize, const NUM_SEGS_P2: usize>(
    iters: usize,
    batch_size: usize,
) {
    let _queue_size = (1 << P) * (1 << NUM_SEGS_P2);

    println!(
        "\nRunning SegSpsc batch stress test: P = {}, NUM_SEGS = {}, ITERS = {}, BATCH = {}",
        P,
        1 << NUM_SEGS_P2,
        humanize_number(iters as u64),
        batch_size
    );

    // SegSpsc batch benchmark with Arc and concurrent access
    let queue = Arc::new(SegSpsc::<usize, P, NUM_SEGS_P2>::new());
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer = {
        let queue = queue.clone();
        thread::spawn(move || {
            for i in 0..iters {
                while queue.try_push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        })
    };

    // Consumer on main thread - runs immediately after spawning producer
    let mut consumed = 0;
    let mut batch = Vec::<usize>::new();
    batch.resize(batch_size, 0);
    while consumed < iters {
        loop {
            match queue.try_pop_n(&mut batch) {
                Ok(popped) => {
                    consumed += popped;
                    batch.resize(batch_size, 0);
                    if popped > 0 {
                        break;
                    }
                }
                Err(_) => {
                    std::hint::spin_loop();
                }
            }
        }
    }

    producer.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = queue.fresh_allocations();
    let pool_reuses = queue.pool_reuses();

    println!(
        "SegSpscBatch<P={}, SEG={}>: {:>10.2} ns/op    {:>15} ops/sec    Batch: {}",
        P,
        1 << NUM_SEGS_P2,
        ns_per_op,
        humanize_number(ops_per_sec),
        batch_size
    );
    println!(
        "  ↳ {}",
        format_cache_performance(pool_reuses, fresh_allocs)
    );

    if let Some(efficiency) = format_memory_efficiency(pool_reuses, fresh_allocs) {
        println!("  ↳ {}", efficiency);
    }
}

fn stress_test_seg_spsc_batch_push<const P: usize, const NUM_SEGS_P2: usize>(
    iters: usize,
    batch_size: usize,
) {
    let _queue_size = (1 << P) * (1 << NUM_SEGS_P2);

    println!(
        "\nRunning SegSpsc batch stress test: P = {}, NUM_SEGS = {}, ITERS = {}, BATCH = {}",
        P,
        1 << NUM_SEGS_P2,
        humanize_number(iters as u64),
        batch_size
    );

    // SegSpsc batch benchmark with Arc and concurrent access
    let queue = Arc::new(SegSpsc::<usize, P, NUM_SEGS_P2>::new());
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer = {
        let queue = queue.clone();
        thread::spawn(move || {
            let mut produced = 0;
            let mut batch = Vec::<usize>::new();
            batch.resize(batch_size, 0);
            while produced < iters {
                loop {
                    match queue.try_push_n(&batch) {
                        Ok(pushed) => {
                            produced += pushed;
                            batch.resize(batch_size, 0);
                            if pushed > 0 {
                                break;
                            }
                        }
                        Err(_) => {
                            std::hint::spin_loop();
                        }
                    }
                }
            }
        })
    };

    // Consumer on main thread - runs immediately after spawning producer
    let mut consumed = 0;
    let mut batch = Vec::<usize>::new();
    batch.resize(batch_size, 0);
    while consumed < iters {
        loop {
            match queue.try_pop_n(&mut batch) {
                Ok(popped) => {
                    consumed += popped;
                    batch.resize(batch_size, 0);
                    if popped > 0 {
                        break;
                    }
                }
                Err(_) => {
                    std::hint::spin_loop();
                }
            }
        }
    }

    producer.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = queue.fresh_allocations();
    let pool_reuses = queue.pool_reuses();

    println!(
        "SegSpscBatch<P={}, SEG={}>: {:>10.2} ns/op    {:>15} ops/sec    Batch: {}",
        P,
        1 << NUM_SEGS_P2,
        ns_per_op,
        humanize_number(ops_per_sec),
        batch_size
    );
    println!(
        "  ↳ {}",
        format_cache_performance(pool_reuses, fresh_allocs)
    );

    if let Some(efficiency) = format_memory_efficiency(pool_reuses, fresh_allocs) {
        println!("  ↳ {}", efficiency);
    }
}

fn stress_test_seg_spsc_batch_pop_single_push<const P: usize, const NUM_SEGS_P2: usize>(
    iters: usize,
    batch_size: usize,
) {
    let _queue_size = (1 << P) * (1 << NUM_SEGS_P2);

    println!(
        "\nRunning SegSpsc batch stress test: P = {}, NUM_SEGS = {}, ITERS = {}, BATCH = {}",
        P,
        1 << NUM_SEGS_P2,
        humanize_number(iters as u64),
        batch_size
    );

    // SegSpsc batch benchmark with Arc and concurrent access
    let queue = Arc::new(SegSpsc::<usize, P, NUM_SEGS_P2>::new());
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer = {
        let queue = queue.clone();
        thread::spawn(move || {
            for i in 0..iters {
                while queue.try_push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        })
    };

    // Consumer on main thread - runs immediately after spawning producer
    let mut consumed = 0;
    let mut buffer = vec![0usize; batch_size];
    while consumed < iters {
        loop {
            match queue.try_pop_n(&mut buffer) {
                Ok(popped) => {
                    buffer.clear();
                    consumed += popped;
                    if popped > 0 {
                        break;
                    }
                }
                Err(_) => {
                    std::hint::spin_loop();
                }
            }
        }
    }

    producer.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = queue.fresh_allocations();
    let pool_reuses = queue.pool_reuses();

    println!(
        "SegSpscBatch<P={}, SEG={}>: {:>10.2} ns/op    {:>15} ops/sec    Batch: {}",
        P,
        1 << NUM_SEGS_P2,
        ns_per_op,
        humanize_number(ops_per_sec),
        batch_size
    );
    println!(
        "  ↳ {}",
        format_cache_performance(pool_reuses, fresh_allocs)
    );

    if let Some(efficiency) = format_memory_efficiency(pool_reuses, fresh_allocs) {
        println!("  ↳ {}", efficiency);
    }
}

fn seg_spsc_consume_in_place_benchmark<const P: usize, const NUM_SEGS_P2: usize>(
    iters: usize,
    consume_batch: usize,
) {
    let _queue_size = (1 << P) * (1 << NUM_SEGS_P2);

    println!(
        "\nRunning SegSpsc consume_in_place benchmark: P = {}, NUM_SEGS = {}, ITERS = {}, CONSUME_BATCH = {}",
        P,
        1 << NUM_SEGS_P2,
        humanize_number(iters as u64),
        consume_batch
    );

    // SegSpsc zero-copy benchmark with Arc and concurrent access
    let queue = Arc::new(SegSpsc::<usize, P, NUM_SEGS_P2>::new());

    {
        let iters = 1_000_000;
        let start = Instant::now();
        // Producer thread - starts on its own thread
        let producer = {
            let queue = queue.clone();
            thread::spawn(move || {
                for i in 0..iters {
                    while queue.try_push(i).is_err() {
                        std::hint::spin_loop();
                    }
                }
            })
        };

        // Consumer on main thread - runs immediately after spawning producer with zero-copy
        let mut batch = Vec::<usize>::new();
        batch.resize(4096, 0);
        let mut consumed = 0;
        while consumed < iters {
            let result = queue.consume_in_place(consume_batch, |chunk| chunk.len());
            consumed += result;
            if result == 0 {
                std::hint::spin_loop();
            }
        }

        producer.join().unwrap();
    }

    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer = {
        let queue = queue.clone();
        thread::spawn(move || {
            for i in 0..iters {
                while queue.try_push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        })
    };

    // Consumer on main thread - runs immediately after spawning producer with zero-copy
    let mut batch = Vec::<usize>::new();
    batch.resize(4096, 0);
    let mut consumed = 0;
    while consumed < iters {
        let result = queue.consume_in_place(consume_batch, |chunk| chunk.len());
        consumed += result;
        if result == 0 {
            std::hint::spin_loop();
        }
    }

    producer.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = queue.fresh_allocations();
    let pool_reuses = queue.pool_reuses();

    println!(
        "SegSpscConsume<P={}, SEG={}>: {:>10.2} ns/op    {:>15} ops/sec    ConsumeBatch: {}",
        P,
        1 << NUM_SEGS_P2,
        ns_per_op,
        humanize_number(ops_per_sec),
        consume_batch
    );
    println!(
        "  ↳ {}",
        format_cache_performance(pool_reuses, fresh_allocs)
    );

    if let Some(efficiency) = format_memory_efficiency(pool_reuses, fresh_allocs) {
        println!("  ↳ {}", efficiency);
    }
}

fn main() {
    println!("{}", "=".repeat(70));
    println!("SPSC Queue Benchmark Suite");
    println!("{}", "=".repeat(70));

    // Baseline SPSC tests
    // println!("\n--- Basic SPSC Tests ---");
    // stress_test_spsc_push::<256>(10_000_000);
    //
    println!("\n--- SegSpsc Single Item Slow Tests ---");
    stress_test_seg_spsc_push_slow::<6, 14>(5_000, 32, Duration::from_millis(1));
    stress_test_seg_spsc_push_slow::<6, 14>(5_000, 64, Duration::from_millis(1));
    stress_test_seg_spsc_push_slow::<6, 14>(50_000, 256, Duration::from_millis(1));
    //
    println!("\n--- SegSpsc Zero-Copy Consume Tests ---");
    // seg_spsc_consume_in_place_benchmark::<8, 8>(50_000_000, 64);
    // seg_spsc_consume_in_place_benchmark::<8, 8>(50_000_000, 256);
    seg_spsc_consume_in_place_benchmark::<4, 12>(50_000_000, 32);

    println!("\n--- Basic DRO SPSC Tests ---");
    stress_test_dro_spsc_push::<256>(100_000_000);
    drain_with_benchmark_dro::<256>(100_000_000);

    // SegSpsc benchmarks
    println!("\n--- SegSpsc Single Item Tests ---");
    // stress_test_seg_spsc_push::<6, 6>(50_000_000); // 64 * 256 = 16K capacity
    // stress_test_seg_spsc_push::<8, 8>(50_000_000); // 256 * 256 = 64K capacity
    stress_test_seg_spsc_push::<4, 14>(50_000_000);
    stress_test_seg_spsc_push::<6, 14>(50_000_000);
    stress_test_seg_spsc_push::<5, 14>(50_000_000);

    // println!("\n--- SegSpsc Batch Pop Operations Tests ---");
    // stress_test_seg_spsc_batch_pop_single_push::<8, 8>(50_000_000, 64);
    // stress_test_seg_spsc_batch_pop_single_push::<8, 8>(50_000_000, 256);
    // stress_test_seg_spsc_batch_pop_single_push::<8, 8>(50_000_000, 1024);
    //
    println!("\n--- SegSpsc Batch Pop Tests ---");
    stress_test_seg_spsc_batch_pop::<8, 8>(50_000_000, 32);

    println!("\n--- SegSpsc Batch Push Tests ---");
    // stress_test_seg_spsc_batch_push::<8, 8>(50_000_000, 64);
    // stress_test_seg_spsc_batch_push::<8, 8>(50_000_000, 256);
    stress_test_seg_spsc_batch_push::<8, 8>(50_000_000, 32);

    // seg_spsc_consume_in_place_benchmark::<8, 8>(50_000_000, 4096);
    // seg_spsc_consume_in_place_benchmark::<8, 8>(50_000_000, 8192);

    // Quick test for SegSpsc with smaller numbers to see cache behavior
    // println!("\n--- Quick SegSpsc Test (Cache Behavior Analysis) ---");
    // println!("Testing with parameters that should trigger segment reuse...\n");
    // stress_test_seg_spsc_push::<6, 6>(50_000); // P=6 (64 items), NUM_SEGS=6 (64 segs) = 4K capacity
    // stress_test_seg_spsc_push::<4, 4>(5_000); // P=4 (16 items), NUM_SEGS=4 (16 segs) = 256 capacity

    println!("\n{}", "=".repeat(70));

    // All SegSpsc tests completed - check individual test results for cache performance details

    println!("Benchmark complete!");
}
