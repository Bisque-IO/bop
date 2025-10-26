//! SPSC Queue Benchmark
//!
//! Stress test for the SPSC (Single Producer Single Consumer) queue implementation.
//! Measures throughput and latency of enqueue/dequeue operations.
//!
//! SegSpsc benchmark uses true SPSC pattern with Arc for shared access:
//! - Producer thread runs on its own thread
//! - Consumer runs on the current thread immediately after spawning the producer
//! - Both threads share access via Arc

use bop_mpmc::PopError;
use bop_mpmc::seg_spsc::SegSpsc;
use bop_mpmc::seg_spsc_dynamic::SegSpsc as SegSpscDyn;
use bop_mpmc::spsc::{HeapBuffer, SpscQueue};
use crossbeam_deque::{Injector, Worker};

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
    let (producer, consumer) = SegSpsc::<usize, P, NUM_SEGS_P2>::new();
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer_thread = thread::spawn(move || {
        for i in 0..iters {
            while producer.try_push(i).is_err() {
                std::hint::spin_loop();
            }
            if i % rate == 0 {
                std::thread::sleep(sleep);
            }
        }
    });

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    while count < iters {
        if consumer.try_pop().is_some() {
            count += 1;
        } else {
            std::hint::spin_loop();
        }
    }

    producer_thread.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = consumer.fresh_allocations();
    let pool_reuses = consumer.pool_reuses();

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
        let (producer, consumer) = SegSpsc::<usize, P, NUM_SEGS_P2>::new();
        let start = Instant::now();

        // Producer thread - starts on its own thread
        let producer_thread = thread::spawn(move || {
            for i in 0..iters {
                while producer.try_push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        });

        // Consumer on main thread - runs immediately after spawning producer
        let mut count = 0;
        while count < iters {
            if consumer.try_pop().is_some() {
                count += 1;
            } else {
                std::hint::spin_loop();
            }
        }

        producer_thread.join().unwrap();
    }

    // SegSpsc benchmark with Arc and concurrent access
    let (producer, consumer) = SegSpsc::<usize, P, NUM_SEGS_P2>::new();
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer_thread = thread::spawn(move || {
        for i in 0..iters {
            while producer.try_push(i).is_err() {
                std::hint::spin_loop();
            }
        }
    });

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    while count < iters {
        if consumer.try_pop().is_some() {
            count += 1;
        } else {
            std::hint::spin_loop();
        }
    }

    producer_thread.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = consumer.fresh_allocations();
    let pool_reuses = consumer.pool_reuses();

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
    let (producer, consumer) = SegSpsc::<usize, P, NUM_SEGS_P2>::new();
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer_thread = thread::spawn(move || {
        for i in 0..iters {
            while producer.try_push(i).is_err() {
                std::hint::spin_loop();
            }
        }
    });

    // Consumer on main thread - runs immediately after spawning producer
    let mut consumed = 0;
    let mut batch = Vec::<usize>::new();
    batch.resize(batch_size, 0);
    while consumed < iters {
        loop {
            match consumer.try_pop_n(&mut batch) {
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

    producer_thread.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = consumer.fresh_allocations();
    let pool_reuses = consumer.pool_reuses();
    let live_segments = consumer.allocated_segments();
    let live_memory_kb = consumer.allocated_memory_bytes() / 1024;

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

    println!(
        "  ↳ Live segments: {}, Live memory: {} KB",
        live_segments, live_memory_kb
    );
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
    let (producer, consumer) = SegSpsc::<usize, P, NUM_SEGS_P2>::new();
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer_thread = thread::spawn(move || {
        let mut produced = 0;
        let mut batch = Vec::<usize>::new();
        batch.resize(batch_size, 0);
        while produced < iters {
            loop {
                match producer.try_push_n(&batch) {
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
    });

    // Consumer on main thread - runs immediately after spawning producer
    let mut consumed = 0;
    let mut batch = Vec::<usize>::new();
    batch.resize(batch_size, 0);
    while consumed < iters {
        loop {
            match consumer.try_pop_n(&mut batch) {
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

    producer_thread.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = consumer.fresh_allocations();
    let pool_reuses = consumer.pool_reuses();
    let live_segments = consumer.allocated_segments();
    let live_memory_kb = consumer.allocated_memory_bytes() / 1024;

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

    println!(
        "  ↳ Live segments: {}, Live memory: {} KB",
        live_segments, live_memory_kb
    );
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
    let (producer, consumer) = SegSpsc::<usize, P, NUM_SEGS_P2>::new();
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer_thread = thread::spawn(move || {
        for i in 0..iters {
            while producer.try_push(i).is_err() {
                std::hint::spin_loop();
            }
        }
    });

    // Consumer on main thread - runs immediately after spawning producer
    let mut consumed = 0;
    let mut buffer = vec![0usize; batch_size];
    while consumed < iters {
        loop {
            match consumer.try_pop_n(&mut buffer) {
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

    producer_thread.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = consumer.fresh_allocations();
    let pool_reuses = consumer.pool_reuses();

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
    {
        let (producer, consumer) = SegSpsc::<usize, P, NUM_SEGS_P2>::new();
        let iters = 1_000_000;
        let start = Instant::now();
        // Producer thread - starts on its own thread
        let producer_thread = thread::spawn(move || {
            for i in 0..iters {
                while producer.try_push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        });

        // Consumer on main thread - runs immediately after spawning producer with zero-copy
        let mut batch = Vec::<usize>::new();
        batch.resize(4096, 0);
        let mut consumed = 0;
        while consumed < iters {
            let result = consumer.consume_in_place(consume_batch, |chunk| chunk.len());
            consumed += result;
            if result == 0 {
                std::hint::spin_loop();
            }
        }

        producer_thread.join().unwrap();
    }

    let (producer, consumer) = SegSpsc::<usize, P, NUM_SEGS_P2>::new();
    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer_thread = {
        thread::spawn(move || {
            for i in 0..iters {
                while producer.try_push(i).is_err() {
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
        let result = consumer.consume_in_place(consume_batch, |chunk| chunk.len());
        consumed += result;
        if result == 0 {
            std::hint::spin_loop();
        }
    }

    producer_thread.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Get allocation statistics
    let fresh_allocs = consumer.fresh_allocations();
    let pool_reuses = consumer.pool_reuses();

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

fn stress_test_injector(iters: usize) {
    println!(
        "\nRunning Injector stress test: ITERS = {}",
        humanize_number(iters as u64)
    );

    let injector = Arc::new(Injector::<usize>::new());
    let worker = Worker::new_fifo();

    {
        // let iters = 1_000_000;
        let injector_ = Arc::clone(&injector);

        // Producer thread - starts on its own thread
        let producer_thread = thread::spawn(move || {
            for i in 0..iters {
                injector_.push(i as usize);
            }
        });

        // Consumer on main thread - runs immediately after spawning producer
        let mut count = 0;
        while count < iters {
            if injector
                .steal_batch_with_limit_and_pop(&worker, 1024)
                .is_success()
            {
                count += 1;
                while let Some(_) = worker.pop() {
                    count += 1;
                }
            }
        }

        producer_thread.join().unwrap();
    }

    let start = Instant::now();

    let injector_ = Arc::clone(&injector);
    // Producer thread - starts on its own thread
    let producer_thread = thread::spawn(move || {
        for i in 0..iters {
            injector_.push(i as usize);
        }
    });

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    while count < iters {
        if injector
            .steal_batch_with_limit_and_pop(&worker, 1024)
            .is_success()
        {
            count += 1;
            while let Some(_) = worker.pop() {
                count += 1;
            }
        }
    }

    producer_thread.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "Injector: {:>10.2} ns/op    {:>15} ops/sec",
        ns_per_op,
        humanize_number(ops_per_sec),
    );
}

fn stress_test_mpsc(iters: usize) {
    println!(
        "\nRunning MPSC stress test: ITERS = {}",
        humanize_number(iters as u64)
    );

    let queue = Arc::new(bop_mpmc::mpsc::MpscQueue::<usize>::new(65536));

    {
        // let iters = 1_000_000;
        let queue_ = Arc::clone(&queue);

        // Producer thread - starts on its own thread
        let producer_thread = thread::spawn(move || {
            for i in 0..iters {
                while let Err(_) = queue_.push(i as usize) {
                    core::hint::spin_loop();
                }
            }
        });

        // Consumer on main thread - runs immediately after spawning producer
        let mut count = 0;
        while count < iters {
            while let Err(_) = unsafe { queue.pop() } {
                core::hint::spin_loop();
            }
            count += 1;
        }

        producer_thread.join().unwrap();
    }

    let start = Instant::now();

    let queue_ = Arc::clone(&queue);
    // Producer thread - starts on its own thread
    let producer_thread = thread::spawn(move || {
        for i in 0..iters {
            while let Err(_) = queue_.push(i as usize) {
                core::hint::spin_loop();
            }
        }
    });

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    while count < iters {
        while let Err(_) = unsafe { queue.pop() } {
            core::hint::spin_loop();
        }
        count += 1;
    }

    producer_thread.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "Injector: {:>10.2} ns/op    {:>15} ops/sec",
        ns_per_op,
        humanize_number(ops_per_sec),
    );
}

fn stress_test_crossfire_mpsc(iters: usize) {
    println!(
        "\nRunning Crossfire MPSC stress test: ITERS = {}",
        humanize_number(iters as u64)
    );

    let (mut tx, rx) = crossfire::mpsc::bounded_blocking(65536);

    {
        // let iters = 1_000_000;

        // Producer thread - starts on its own thread
        let producer_thread = thread::spawn(move || {
            for i in 0..iters {
                while let Err(_) = tx.send(i as usize) {
                    core::hint::spin_loop();
                }
            }
            tx
        });

        // Consumer on main thread - runs immediately after spawning producer
        let mut count = 0;
        while count < iters {
            while let Err(_) = rx.recv() {
                core::hint::spin_loop();
            }
            count += 1;
        }

        tx = producer_thread.join().unwrap();
    }

    let start = Instant::now();

    // Producer thread - starts on its own thread
    let producer_thread = thread::spawn(move || {
        for i in 0..iters {
            while let Err(_) = tx.send(i as usize) {
                core::hint::spin_loop();
            }
        }
    });

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    while count < iters {
        while let Err(_) = rx.recv() {
            core::hint::spin_loop();
        }
        count += 1;
    }

    producer_thread.join().unwrap();
    let duration = start.elapsed();

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "Injector: {:>10.2} ns/op    {:>15} ops/sec",
        ns_per_op,
        humanize_number(ops_per_sec),
    );
}

fn stress_test_crossfire_mpsc_multi(iters_per_producer: usize, num_producers: usize) {
    let iters = iters_per_producer * num_producers;
    println!(
        "\nRunning Crossfire MPSC stress test: ITERS = {}",
        humanize_number(iters as u64)
    );

    let (mut tx, rx) = crossfire::mpsc::bounded_blocking(65536 * 8);
    // let (mut tx, rx) = crossfire::mpsc::unbounded_blocking();

    {
        // let iters = 1_000_000;

        // Producer thread - starts on its own thread
        let producer_thread = thread::spawn(move || {
            for i in 0..iters_per_producer {
                while let Err(_) = tx.send(i as usize) {
                    core::hint::spin_loop();
                }
            }
            tx
        });

        // Consumer on main thread - runs immediately after spawning producer
        let mut count = 0;
        while count < iters_per_producer {
            while let Err(_) = rx.recv() {
                core::hint::spin_loop();
            }
            count += 1;
        }

        tx = producer_thread.join().unwrap();
    }

    let start = Instant::now();

    let mut producer_threads = Vec::new();

    for _ in 0..num_producers {
        let tx = tx.clone();
        producer_threads.push(thread::spawn(move || {
            for i in 0..iters_per_producer {
                while let Err(_) = tx.send(i as usize) {
                    core::hint::spin_loop();
                }
            }
        }));
    }

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    while count < iters {
        while let Err(_) = rx.recv() {
            core::hint::spin_loop();
        }
        count += 1;
    }

    let duration = start.elapsed();
    for producer_thread in producer_threads.drain(..) {
        producer_thread.join().unwrap();
    }

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "Injector: {:>10.2} ns/op    {:>15} ops/sec",
        ns_per_op,
        humanize_number(ops_per_sec),
    );
}

fn stress_test_crossfire_mpsc_sharded(iters_per_producer: usize, num_producers: usize) {
    let iters = iters_per_producer * num_producers;
    println!(
        "\nRunning Crossfire MPSC Sharded stress test: ITERS = {}",
        humanize_number(iters as u64)
    );

    let mut queues = Vec::with_capacity(num_producers);
    for _ in 0..num_producers {
        queues.push(crossfire::mpsc::bounded_blocking(65536 * 8));
    }
    // let (mut tx, rx) = crossfire::mpsc::bounded_blocking(65536 * 8);
    // let (mut tx, rx) = crossfire::mpsc::unbounded_blocking();

    // {
    //     // let iters = 1_000_000;

    //     // Producer thread - starts on its own thread
    //     let producer_thread = thread::spawn(move || {
    //         for i in 0..iters_per_producer {
    //             while let Err(_) = tx.send(i as usize) {
    //                 core::hint::spin_loop();
    //             }
    //         }
    //         tx
    //     });

    //     // Consumer on main thread - runs immediately after spawning producer
    //     let mut count = 0;
    //     while count < iters_per_producer {
    //         while let Err(_) = rx.recv() {
    //             core::hint::spin_loop();
    //         }
    //         count += 1;
    //     }

    //     tx = producer_thread.join().unwrap();
    // }

    let start = Instant::now();

    let mut producer_threads = Vec::new();

    for i in 0..num_producers {
        let tx = queues[i].0.clone();
        producer_threads.push(thread::spawn(move || {
            for i in 0..iters_per_producer {
                while let Err(_) = tx.send(i as usize) {
                    core::hint::spin_loop();
                }
            }
        }));
    }

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    let mut counts = Vec::with_capacity(num_producers);
    for _ in 0..num_producers {
        counts.push(0);
    }
    let batch_size = 100;
    while count < iters {
        for i in 0..num_producers {
            let q = &queues[i].1;
            let end_count = (counts[i] + batch_size).min(iters_per_producer);
            while counts[i] < end_count {
                while let Err(_) = q.recv() {
                    core::hint::spin_loop();
                }
                counts[i] += 1;
                count += 1;
            }
        }
    }

    let duration = start.elapsed();
    for producer_thread in producer_threads.drain(..) {
        producer_thread.join().unwrap();
    }

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "Injector: {:>10.2} ns/op    {:>15} ops/sec",
        ns_per_op,
        humanize_number(ops_per_sec),
    );
}

fn stress_test_spsc_sharded<const P: usize, const NUM_SEGS_P2: usize>(
    iters_per_producer: usize,
    num_producers: usize,
    batch_size: usize,
) {
    let iters = iters_per_producer * num_producers;
    println!(
        "\nRunning SPSC Sharded stress test: ITERS = {}",
        humanize_number(iters as u64)
    );

    let mut queues = Vec::with_capacity(num_producers);
    for _ in 0..num_producers {
        let (tx, rx) = SegSpsc::<usize, P, NUM_SEGS_P2>::new_with_config(0);
        queues.push((Some(tx), rx));
    }
    // let (mut tx, rx) = crossfire::mpsc::bounded_blocking(65536 * 8);
    // let (mut tx, rx) = crossfire::mpsc::unbounded_blocking();

    // {
    //     // let iters = 1_000_000;

    //     // Producer thread - starts on its own thread
    //     let producer_thread = thread::spawn(move || {
    //         for i in 0..iters_per_producer {
    //             while let Err(_) = tx.send(i as usize) {
    //                 core::hint::spin_loop();
    //             }
    //         }
    //         tx
    //     });

    //     // Consumer on main thread - runs immediately after spawning producer
    //     let mut count = 0;
    //     while count < iters_per_producer {
    //         while let Err(_) = rx.recv() {
    //             core::hint::spin_loop();
    //         }
    //         count += 1;
    //     }

    //     tx = producer_thread.join().unwrap();
    // }

    let mut batch = Vec::with_capacity(batch_size);
    for _ in 0..batch_size {
        batch.push(0usize);
    }
    let start = Instant::now();

    let mut producer_threads = Vec::new();

    for i in 0..num_producers {
        let tx = queues[i].0.take().unwrap();
        producer_threads.push(thread::spawn(move || {
            for i in 0..iters_per_producer {
                while let Err(_) = tx.try_push(i as usize) {
                    core::hint::spin_loop();
                }
            }
        }));
    }

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    let mut counts = Vec::with_capacity(num_producers);
    for _ in 0..num_producers {
        counts.push(0);
    }
    while count < iters {
        for i in 0..num_producers {
            let q = &queues[i].1;
            if counts[i] < iters_per_producer {
                match q.try_pop_n(&mut batch) {
                    Ok(size) => {
                        counts[i] += size;
                        count += size;
                    }
                    Err(PopError::Empty) => {
                        core::hint::spin_loop();
                    }
                    Err(PopError::Closed) | Err(PopError::Timeout) => unreachable!(),
                }
            }
        }
    }

    let duration = start.elapsed();
    for producer_thread in producer_threads.drain(..) {
        producer_thread.join().unwrap();
    }

    // Trigger deallocation now that queues are drained
    // Option 1: Deallocate to configured max (keeps warm pool of 16 segments per queue = 128 total)
    for (_, consumer) in queues.iter() {
        consumer.try_deallocate_excess();
    }

    // Option 2: Deallocate all pooled segments (maximum memory reclamation)
    // for (_, consumer) in queues.iter() {
    //     consumer.deallocate_to(0);
    // }

    // Option 3: Keep specific number (e.g., 4 segments per queue for minimal warm pool)
    // for (_, consumer) in queues.iter() {
    //     consumer.deallocate_to(4);
    // }

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    // Collect allocation statistics from all queues
    let mut total_fresh_allocs = 0u64;
    let mut total_pool_reuses = 0u64;
    let mut total_allocated_segments = 0u64;
    let mut total_allocated_memory = 0usize;

    for (_, consumer) in queues.iter() {
        total_fresh_allocs += consumer.fresh_allocations();
        total_pool_reuses += consumer.pool_reuses();
        total_allocated_segments += consumer.allocated_segments();
        total_allocated_memory += consumer.allocated_memory_bytes();
    }

    let total_allocs = total_fresh_allocs + total_pool_reuses;
    let reuse_rate = if total_allocs > 0 {
        (total_pool_reuses as f64 / total_allocs as f64) * 100.0
    } else {
        0.0
    };

    println!(
        "Injector: {:>10.2} ns/op    {:>15} ops/sec",
        ns_per_op,
        humanize_number(ops_per_sec),
    );
    println!("  Stats:");
    println!(
        "    Fresh allocations: {}",
        humanize_number(total_fresh_allocs / num_producers as u64)
    );
    println!(
        "    Pool reuses:       {}",
        humanize_number(total_pool_reuses / num_producers as u64)
    );
    println!("    Reuse rate:        {:.1}%", reuse_rate);
    println!(
        "    Live segments:     {}",
        total_allocated_segments / num_producers as u64
    );
    println!(
        "    Live memory:       {} KB",
        total_allocated_memory / 1024
    );
}

fn stress_test_spsc_dyn_sharded<const P: usize, const NUM_SEGS_P2: usize>(
    iters_per_producer: usize,
    num_producers: usize,
    batch_size: usize,
) {
    let iters = iters_per_producer * num_producers;
    println!(
        "\nRunning SPSC Sharded Dynamic stress test: ITERS = {}",
        humanize_number(iters as u64)
    );

    let mut queues = Vec::with_capacity(num_producers);
    for _ in 0..num_producers {
        let (tx, rx) = SegSpscDyn::<usize>::new(P, NUM_SEGS_P2);
        queues.push((Some(tx), rx));
    }
    // let (mut tx, rx) = crossfire::mpsc::bounded_blocking(65536 * 8);
    // let (mut tx, rx) = crossfire::mpsc::unbounded_blocking();

    // {
    //     // let iters = 1_000_000;

    //     // Producer thread - starts on its own thread
    //     let producer_thread = thread::spawn(move || {
    //         for i in 0..iters_per_producer {
    //             while let Err(_) = tx.send(i as usize) {
    //                 core::hint::spin_loop();
    //             }
    //         }
    //         tx
    //     });

    //     // Consumer on main thread - runs immediately after spawning producer
    //     let mut count = 0;
    //     while count < iters_per_producer {
    //         while let Err(_) = rx.recv() {
    //             core::hint::spin_loop();
    //         }
    //         count += 1;
    //     }

    //     tx = producer_thread.join().unwrap();
    // }

    let mut batch = Vec::with_capacity(batch_size);
    for _ in 0..batch_size {
        batch.push(0usize);
    }
    let start = Instant::now();

    let mut producer_threads = Vec::new();

    for i in 0..num_producers {
        let tx = queues[i].0.take().unwrap();
        producer_threads.push(thread::spawn(move || {
            for i in 0..iters_per_producer {
                while let Err(_) = tx.try_push(i as usize) {
                    core::hint::spin_loop();
                }
            }
        }));
    }

    // Consumer on main thread - runs immediately after spawning producer
    let mut count = 0;
    let mut counts = Vec::with_capacity(num_producers);
    for _ in 0..num_producers {
        counts.push(0);
    }
    while count < iters {
        for i in 0..num_producers {
            let q = &queues[i].1;
            if counts[i] < iters_per_producer {
                match q.try_pop_n(&mut batch) {
                    Ok(size) => {
                        counts[i] += size;
                        count += size;
                    }
                    Err(PopError::Empty) => {
                        core::hint::spin_loop();
                    }
                    Err(PopError::Closed) | Err(PopError::Timeout) => unreachable!(),
                }
            }
        }
    }

    let duration = start.elapsed();
    for producer_thread in producer_threads.drain(..) {
        producer_thread.join().unwrap();
    }

    // Calculate metrics
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / iters as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "Injector: {:>10.2} ns/op    {:>15} ops/sec",
        ns_per_op,
        humanize_number(ops_per_sec),
    );
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
    // stress_test_seg_spsc_push_slow::<6, 14>(5_000, 32, Duration::from_millis(1));
    // stress_test_seg_spsc_push_slow::<6, 14>(5_000, 64, Duration::from_millis(1));
    // stress_test_seg_spsc_push_slow::<6, 14>(50_000, 256, Duration::from_millis(1));
    //
    println!("\n--- SegSpsc Zero-Copy Consume Tests ---");
    // seg_spsc_consume_in_place_benchmark::<8, 8>(50_000_000, 64);
    // seg_spsc_consume_in_place_benchmark::<8, 8>(50_000_000, 256);
    // seg_spsc_consume_in_place_benchmark::<8, 12>(50_000_000, 1);

    // println!("\n--- Basic DRO SPSC Tests ---");
    // stress_test_dro_spsc_push::<256>(100_000_000);
    // drain_with_benchmark_dro::<256>(100_000_000);

    // SegSpsc benchmarks
    println!("\n--- SegSpsc Single Item Tests ---");
    // stress_test_seg_spsc_push::<6, 6>(50_000_000); // 64 * 256 = 16K capacity
    // stress_test_seg_spsc_push::<8, 8>(50_000_000); // 256 * 256 = 64K capacity
    // stress_test_seg_spsc_push::<4, 14>(50_000_000);
    // stress_test_seg_spsc_push::<12, 5>(50_000_000);
    // stress_test_seg_spsc_push::<6, 14>(50_000_000);
    // stress_test_seg_spsc_push::<5, 14>(50_000_000);
    // stress_test_seg_spsc_push::<5, 12>(50_000_000);
    // stress_test_seg_spsc_push::<8, 12>(50_000_000);

    // println!("\n--- SegSpsc Batch Pop Operations Tests ---");
    // stress_test_seg_spsc_batch_pop_single_push::<8, 8>(50_000_000, 64);
    // stress_test_seg_spsc_batch_pop_single_push::<8, 8>(50_000_000, 256);
    // stress_test_seg_spsc_batch_pop_single_push::<8, 8>(50_000_000, 1024);
    //
    // println!("\n--- SegSpsc Batch Pop Tests ---");
    // stress_test_seg_spsc_batch_pop::<8, 8>(50_000_000, 4096);

    println!("\n--- SegSpsc Batch Push Tests ---");
    // stress_test_seg_spsc_batch_push::<8, 8>(50_000_000, 64);
    // stress_test_seg_spsc_batch_push::<8, 8>(50_000_000, 256);
    // stress_test_seg_spsc_batch_push::<12, 8>(50_000_000, 4096);

    // stress_test_injector(5_000_000);
    // stress_test_injector(5_000_000);
    // stress_test_injector(5_000_000);
    // stress_test_mpsc(5_000_000);
    // stress_test_mpsc(5_000_000);
    // stress_test_mpsc(5_000_000);
    // stress_test_crossfire_mpsc(5_000_000);
    // stress_test_crossfire_mpsc(50_000_000);
    // stress_test_crossfire_mpsc(50_000_000);
    // stress_test_crossfire_mpsc_multi(5_000_000, 4);
    // stress_test_crossfire_mpsc_multi(5_000_000, 4);
    // stress_test_crossfire_mpsc_multi(5_000_000, 4);
    // stress_test_crossfire_mpsc_multi(25_000_000, 2);

    // stress_test_crossfire_mpsc_multi(10_000_000, 4);
    // stress_test_crossfire_mpsc_multi(5_000_000, 8);
    // stress_test_crossfire_mpsc_sharded(5_000_000, 8);
    // stress_test_crossfire_mpsc_sharded(5_000_000, 8);
    // stress_test_crossfire_mpsc_sharded(5_000_000, 8);

    // stress_test_spsc_sharded::<8, 10>(5_000_000, 4, 1);
    // stress_test_spsc_sharded::<8, 10>(5_000_000, 4, 64);
    stress_test_spsc_dyn_sharded::<10, 6>(50_000_000, 8, 4096);
    stress_test_spsc_dyn_sharded::<10, 6>(50_000_000, 8, 4096);
    stress_test_spsc_dyn_sharded::<10, 6>(50_000_000, 8, 4096);

    // stress_test_spsc_sharded::<10, 10>(50_000_000, 8, 4096);
    // stress_test_spsc_sharded::<8, 12>(50_000_000, 8, 4096);
    stress_test_spsc_sharded::<10, 6>(50_000_000, 8, 4096);
    stress_test_spsc_sharded::<10, 6>(50_000_000, 8, 4096);
    stress_test_spsc_sharded::<10, 6>(50_000_000, 8, 4096);
    // stress_test_spsc_sharded::<8, 10>(50_000_000, 8, 8192);

    // stress_test_spsc_dyn_sharded::<8, 10>(50_000_000, 4, 1);
    // stress_test_spsc_dyn_sharded::<8, 10>(50_000_000, 4, 64);

    // stress_test_spsc_dyn_sharded::<8, 10>(50_000_000, 8, 8192);

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
