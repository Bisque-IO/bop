//! MPMC Blocking Queue Benchmark
//!
//! Stress test for the Mpmc (Multi-Producer Single-Consumer) queue implementation.
//! Measures throughput and latency with multiple producers using blocking operations.

use bop_mpmc::mpmc::Mpmc;
use bop_mpmc::selector::Selector;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

// use bop_allocator::BopAllocator;
//
// #[global_allocator]
// static GLOBAL: BopAllocator = BopAllocator;

/// Format a large number with thousand separators for readability
fn humanize_number(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }

    let mut result = String::new();
    let mut count = 0;

    while n > 0 {
        if count > 0 && count % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, char::from_digit((n % 10) as u32, 10).unwrap());
        n /= 10;
        count += 1;
    }

    result
}

/// Bulk drain benchmark using drain_with
/// P: log2(segment_size), NUM_SEGS_P2: log2(num_segments)
/// Capacity = (1 << P) * (1 << NUM_SEGS_P2) - 1
fn bulk_drain_mpmc_blocking_benchmark<const P: usize, const NUM_SEGS_P2: usize>(
    num_producers: usize,
    items_per_producer: usize,
    batch_size: usize,
) {
    let capacity = ((1 << P) * (1 << NUM_SEGS_P2)) - 1;
    println!(
        "\nMPMC drain_with: P={} (seg_size={}), NUM_SEGS_P2={} (num_segs={}), CAPACITY={}, PRODUCERS = {}, ITEMS_PER_PRODUCER = {}, BATCH_SIZE = {}",
        P,
        1 << P,
        NUM_SEGS_P2,
        1 << NUM_SEGS_P2,
        capacity,
        num_producers,
        humanize_number(items_per_producer as u64),
        batch_size
    );

    let queue = Arc::new(Mpmc::<usize, P, NUM_SEGS_P2>::new());
    let total_items = num_producers * items_per_producer;

    let start = Instant::now();

    // Producer threads
    let producers: Vec<_> = (0..num_producers)
        .map(|id| {
            let queue = queue.clone();
            thread::spawn(move || {
                let producer = queue.create_producer_handle().unwrap();
                let mut total_pushed = 0;

                while total_pushed < items_per_producer {
                    match producer.try_push(id * items_per_producer) {
                        Ok(_) => {
                            total_pushed += 1;
                        }
                        Err(_) => {
                            std::hint::spin_loop();
                        }
                    }
                }

                println!("pushed all");
            })
        })
        .collect();

    // Consumer using drain_with
    let mut count = 0;

    while count < total_items {
        let drained = queue.consume_in_place(
            |_item| {
                // Process item (in this benchmark, just count)
            },
            batch_size,
        );

        if drained > 0 {
            count += drained;
        } else {
            std::hint::spin_loop();
        }
    }

    let duration = start.elapsed();

    for p in producers {
        p.join().unwrap();
    }

    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / total_items as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "drain_with MPMC<P={},NUM_SEGS_P2={},{} prod>: {:>10.2} ns/op    {:>15} ops/sec",
        P,
        NUM_SEGS_P2,
        num_producers,
        ns_per_op,
        humanize_number(ops_per_sec)
    );
}

/// Bulk drain benchmark with multiple drain threads (blocking)
fn multi_mpmc_blocking_benchmark<const P: usize, const NUM_SEGS_P2: usize>(
    num_producers: usize,
    items_per_producer: usize,
    num_drainers: usize,
    batch_size: usize,
) {
    let capacity = ((1 << P) * (1 << NUM_SEGS_P2)) - 1;
    println!(
        "\nMPMC blocking multi-drain: P={} (seg_size={}), NUM_SEGS_P2={} (num_segs={}), CAPACITY={}, PRODUCERS = {}, DRAINERS = {}, ITEMS_PER_PRODUCER = {}, BATCH_SIZE = {}",
        P,
        1 << P,
        NUM_SEGS_P2,
        1 << NUM_SEGS_P2,
        capacity,
        num_producers,
        num_drainers,
        humanize_number(items_per_producer as u64),
        batch_size
    );

    let queue = Arc::new(Mpmc::<usize, P, NUM_SEGS_P2>::new());
    let total_items = num_producers * items_per_producer;
    let items_drained = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();

    // Producer threads
    let producers: Vec<_> = (0..num_producers)
        .map(|id| {
            let queue = queue.clone();
            thread::spawn(move || {
                let producer = queue.create_producer_handle().unwrap();
                let mut total_pushed = 0;

                while total_pushed < items_per_producer {
                    // match queue.push(id * items_per_producer + total_pushed) {
                    match producer.try_push(id * items_per_producer + total_pushed) {
                        Ok(_) => {
                            total_pushed += 1;
                        }
                        Err(_) => {
                            std::hint::spin_loop();
                        }
                    }
                }

                println!("Producer {} pushed all", id);
            })
        })
        .collect();

    // Multiple drain threads using drain_with
    let drainers: Vec<_> = (0..num_drainers)
        .map(|drainer_id| {
            let queue = queue.clone();
            let items_drained = items_drained.clone();
            thread::spawn(move || {
                let mut local_drained = 0;

                let mut zero_count = 0;
                loop {
                    let current_total = items_drained.load(Ordering::Relaxed);
                    if current_total >= total_items {
                        break;
                    }

                    let drained = queue.consume_in_place(
                        |_item| {
                            // Process item (in this benchmark, just count)
                        },
                        batch_size,
                    );

                    if drained > 0 {
                        local_drained += drained;
                        items_drained.fetch_add(drained, Ordering::Relaxed);
                        zero_count = 0;
                    } else {
                        zero_count += 1;
                        if zero_count >= 1000 {
                            println!(
                                "Drainer {} stuck: {} zeros, total={}, target={}",
                                drainer_id, zero_count, current_total, total_items
                            );
                            zero_count = 0;
                        }
                        // Yield when queue is empty to avoid spinning
                        std::thread::yield_now();
                    }
                }

                println!("Drainer {} drained {} items", drainer_id, local_drained);
            })
        })
        .collect();

    // Wait for all threads to complete
    for p in producers {
        p.join().unwrap();
    }
    for d in drainers {
        d.join().unwrap();
    }

    let duration = start.elapsed();
    let final_drained = items_drained.load(Ordering::Relaxed);

    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / final_drained as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "Multi-drain_with MPMC<P={},NUM_SEGS_P2={},{} prod,{} drain>: {:>10.2} ns/op    {:>15} ops/sec",
        P,
        NUM_SEGS_P2,
        num_producers,
        num_drainers,
        ns_per_op,
        humanize_number(ops_per_sec)
    );
    println!(
        "Expected items: {}, Actual drained: {}",
        total_items, final_drained
    );
}

/// Bulk drain benchmark with multiple drain threads (blocking)
fn bulk_drain_multi_mpmc_blocking_benchmark<const P: usize, const NUM_SEGS_P2: usize>(
    num_producers: usize,
    items_per_producer: usize,
    num_drainers: usize,
    batch_size: usize,
) {
    let capacity = ((1 << P) * (1 << NUM_SEGS_P2)) - 1;
    println!(
        "\nMPMC blocking multi-drain: P={} (seg_size={}), NUM_SEGS_P2={} (num_segs={}), CAPACITY={}, PRODUCERS = {}, DRAINERS = {}, ITEMS_PER_PRODUCER = {}, BATCH_SIZE = {}",
        P,
        1 << P,
        NUM_SEGS_P2,
        1 << NUM_SEGS_P2,
        capacity,
        num_producers,
        num_drainers,
        humanize_number(items_per_producer as u64),
        batch_size
    );

    let queue = Arc::new(Mpmc::<usize, P, NUM_SEGS_P2>::new());
    let total_items = num_producers * items_per_producer;
    let items_drained = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();

    // Producer threads
    let producers: Vec<_> = (0..num_producers)
        .map(|id| {
            let queue = queue.clone();
            thread::spawn(move || {
                let producer = queue.create_producer_handle().unwrap();
                let mut total_pushed = 0;

                while total_pushed < items_per_producer {
                    match producer.try_push(id * items_per_producer + total_pushed) {
                        // match producer.push(id * items_per_producer + total_pushed) {
                        Ok(_) => {
                            total_pushed += 1;
                        }
                        Err(_) => {
                            std::hint::spin_loop();
                        }
                    }
                }

                println!("Producer {} pushed all", id);
            })
        })
        .collect();

    // Multiple drain threads using drain_with
    let drainers: Vec<_> = (0..num_drainers)
        .map(|drainer_id| {
            let queue = queue.clone();
            let items_drained = items_drained.clone();
            let mut consumer = queue.get_producer_at(0);
            while consumer.is_none() {
                consumer = queue.get_producer_at(0);
            }
            let consumer = consumer.unwrap();
            thread::spawn(move || {
                let mut local_drained = 0;

                let mut selector = Selector::new();
                let mut zero_count = 0;
                let mut batch = Vec::<usize>::new();
                batch.resize(batch_size, 0);
                loop {
                    let current_total = items_drained.load(Ordering::Relaxed);
                    if current_total >= total_items {
                        break;
                    }
                    //
                    // if local_drained >= total_items {
                    //     items_drained.fetch_add(local_drained, Ordering::Relaxed);
                    //     break;
                    // }
                    // match queue.pop_with_selector(&mut selector) {
                    //     Ok(_) => {
                    //         local_drained += 1;
                    //         items_drained.fetch_add(1, Ordering::Relaxed);
                    //         zero_count = 0;
                    //     }
                    //     Err(_) => {
                    //         zero_count += 1;
                    //         if zero_count >= 1000 {
                    //             println!(
                    //                 "Drainer {} stuck: {} zeros, total={}, target={}",
                    //                 drainer_id, zero_count, current_total, total_items
                    //             );
                    //             zero_count = 0;
                    //             std::thread::sleep(Duration::from_millis(1));
                    //         }
                    //         // Yield when queue is empty to avoid spinning
                    //         std::hint::spin_loop();
                    //     }
                    // }

                    // let drained = mpmc.drain_with_selector(
                    //     &mut selector,
                    //     |_item| {
                    //         // Process item (in this benchmark, just count)
                    //     },
                    //     batch_size,
                    // );
                    //

                    // match queue.pop_with_selector(&mut selector) {
                    //     Ok(_) => {
                    //         local_drained += 1;
                    //         // items_drained.fetch_add(1, Ordering::Relaxed);
                    //         zero_count = 0;
                    //     }
                    //     Err(_) => {
                    //         zero_count += 1;
                    //         if zero_count >= 1000 {
                    //             // println!(
                    //             //     "Drainer {} stuck: {} zeros, total={}, target={}",
                    //             //     drainer_id, zero_count, current_total, total_items
                    //             // );
                    //             zero_count = 0;
                    //             // std::thread::sleep(Duration::from_millis(3));
                    //         }
                    //         // Yield when queue is empty to avoid spinning
                    //         std::hint::spin_loop();
                    //     }
                    // }

                    // match consumer.try_pop() {
                    //     Some(_) => {
                    //         local_drained += 1;
                    //         // items_drained.fetch_add(1, Ordering::Relaxed);
                    //         zero_count = 0;
                    //     }
                    //     None => {
                    //         zero_count += 1;
                    //         if zero_count >= 1000 {
                    //             // println!(
                    //             //     "Drainer {} stuck: {} zeros, total={}, target={}",
                    //             //     drainer_id, zero_count, current_total, total_items
                    //             // );
                    //             zero_count = 0;
                    //             // std::thread::sleep(Duration::from_millis(3));
                    //         }
                    //         // Yield when queue is empty to avoid spinning
                    //         std::hint::spin_loop();
                    //     }
                    // }
                    // match consumer.try_pop_n(&mut batch) {
                    //     Ok(size) => {
                    //         local_drained += size;
                    //         // items_drained.fetch_add(size, Ordering::SeqCst);
                    //         zero_count = 0;
                    //     }
                    //     Err(_) => {
                    //         zero_count += 1;
                    //         if zero_count >= 1000 {
                    //             // println!(
                    //             //     "Drainer {} stuck: {} zeros, total={}, target={}",
                    //             //     drainer_id, zero_count, current_total, total_items
                    //             // );
                    //             zero_count = 0;
                    //             // std::thread::sleep(Duration::from_millis(3));
                    //         }
                    //         // Yield when queue is empty to avoid spinning
                    //         std::hint::spin_loop();
                    //     }
                    // }

                    let drained = queue.try_pop_n_with_selector(&mut selector, &mut batch);

                    if drained > 0 {
                        local_drained += drained;
                        items_drained.fetch_add(drained, Ordering::Relaxed);
                        zero_count = 0;
                    } else {
                        zero_count += 1;
                        if zero_count >= 1000 {
                            // println!(
                            //     "Drainer {} stuck: {} zeros, total={}, target={}",
                            //     drainer_id, zero_count, current_total, total_items
                            // );
                            zero_count = 0;
                            // std::thread::sleep(Duration::from_millis(3));
                        }
                        // Yield when queue is empty to avoid spinning
                        std::hint::spin_loop();
                    }
                }

                println!("Drainer {} drained {} items", drainer_id, local_drained);
            })
        })
        .collect();

    // Wait for all threads to complete
    for p in producers {
        p.join().unwrap();
    }
    for d in drainers {
        d.join().unwrap();
    }

    let duration = start.elapsed();
    let final_drained = items_drained.load(Ordering::Relaxed);

    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / final_drained as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "Multi-drain_with MPMC<P={},NUM_SEGS_P2={},{} prod,{} drain>: {:>10.2} ns/op    {:>15} ops/sec",
        P,
        NUM_SEGS_P2,
        num_producers,
        num_drainers,
        ns_per_op,
        humanize_number(ops_per_sec)
    );
    println!(
        "Expected items: {}, Actual drained: {}",
        total_items, final_drained
    );
}

fn bulk_drain_multi_moody_blocking_benchmark<const P: usize, const NUM_SEGS_P2: usize>(
    num_producers: usize,
    items_per_producer: usize,
    num_drainers: usize,
    batch_size: usize,
) {
    let capacity = ((1 << P) * (1 << NUM_SEGS_P2)) - 1;
    println!(
        "\nMoodycamel MPMC multi-drain: P={} (seg_size={}), NUM_SEGS_P2={} (num_segs={}), CAPACITY={}, PRODUCERS = {}, DRAINERS = {}, ITEMS_PER_PRODUCER = {}, BATCH_SIZE = {}",
        P,
        1 << P,
        NUM_SEGS_P2,
        1 << NUM_SEGS_P2,
        capacity,
        num_producers,
        num_drainers,
        humanize_number(items_per_producer as u64),
        batch_size
    );

    let q = Arc::new(bop_mpmc::moody::Queue::new().unwrap());

    let total_items = num_producers * items_per_producer;
    let items_drained = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();

    // Producer threads
    let producers: Vec<_> = (0..num_producers)
        .map(|id| {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                let producer = q.create_producer_token().unwrap();
                let mut total_pushed = 0;

                while total_pushed < items_per_producer {
                    if producer.enqueue(1) {
                        total_pushed += 1;
                    } else {
                        std::hint::spin_loop();
                    }
                }

                println!("Producer {} pushed all", id);
            })
        })
        .collect();

    // Multiple drain threads using drain_with
    let drainers: Vec<_> = (0..num_drainers)
        .map(|drainer| {
            let q = Arc::clone(&q);
            let items_drained = items_drained.clone();
            thread::spawn(move || {
                let consumer = q.create_consumer_token().unwrap();
                let mut local_drained = 0;
                let mut zero_count = 0;
                let mut batch = Vec::<u64>::new();
                batch.resize(batch_size, 0);
                loop {
                    let current_total = items_drained.load(Ordering::Relaxed);
                    if current_total >= total_items {
                        break;
                    }

                    let drained = consumer.dequeue_bulk(&mut batch);

                    if drained > 0 {
                        local_drained += drained;
                        items_drained.fetch_add(drained, Ordering::Relaxed);
                        zero_count = 0;
                    } else {
                        zero_count += 1;
                        if zero_count >= 1000 {
                            // println!(
                            //     "Drainer {} stuck: {} zeros, total={}, target={}",
                            //     drainer_id, zero_count, current_total, total_items
                            // );
                            zero_count = 0;
                            // std::thread::sleep(Duration::from_millis(3));
                        }
                        // Yield when queue is empty to avoid spinning
                        std::hint::spin_loop();
                    }
                }

                println!("Drainer {} drained {} items", drainer, local_drained);
            })
        })
        .collect();

    // Wait for all threads to complete
    for p in producers {
        p.join().unwrap();
    }
    for d in drainers {
        d.join().unwrap();
    }

    let duration = start.elapsed();
    let final_drained = items_drained.load(Ordering::Relaxed);

    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / final_drained as f64;
    let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

    println!(
        "Multi-drain_with MPMC<P={},NUM_SEGS_P2={},{} prod,{} drain>: {:>10.2} ns/op    {:>15} ops/sec",
        P,
        NUM_SEGS_P2,
        num_producers,
        num_drainers,
        ns_per_op,
        humanize_number(ops_per_sec)
    );
    println!(
        "Expected items: {}, Actual drained: {}",
        total_items, final_drained
    );
}

// Note: Arc<T> is NOT Copy, so we can't use it with SegSpsc
// The arc_drain_multi_mpmc_blocking_benchmark has been removed because
// SegSpsc requires T: Copy, and Arc<T> doesn't implement Copy

fn main() {
    println!("{}", "=".repeat(70));
    println!("MPMC Blocking Queue Benchmark Suite");
    println!("{}", "=".repeat(70));

    // Bulk drain tests
    // println!("\n--- Bulk Blocking Drain Tests ---");
    // P=13, NUM_SEGS_P2=0 -> seg_size=8192, num_segs=1, capacity=8191
    // bulk_drain_mpmc_blocking_benchmark::<13, 0>(1, 100_000_000, 1024);
    // P=10, NUM_SEGS_P2=0 -> seg_size=1024, num_segs=1, capacity=1023
    // bulk_drain_mpmc_blocking_benchmark::<10, 0>(4, 100_000_000, 1024);
    // P=14, NUM_SEGS_P2=0 -> seg_size=16384, num_segs=1, capacity=16383
    // bulk_drain_mpmc_blocking_benchmark::<14, 0>(8, 100_000_000, 1024);

    // Multi-drain tests
    println!("\n--- Multi-Drain Blocking Tests ---");

    // P=8, NUM_SEGS_P2=4 -> seg_size=256, num_segs=16, capacity=4095
    // bulk_drain_multi_mpmc_blocking_benchmark::<6, 14>(8, 100_000_000, 4, 256);
    // bulk_drain_multi_mpmc_blocking_benchmark::<6, 14>(4, 100_000_000, 4, 256);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(4, 100_000_000, 1, 256);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(4, 100_000_000, 2, 256);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(4, 100_000_000, 4, 256);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 100_000_000, 5, 256);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 100_000_000, 5, 8192);
    // bulk_drain_multi_mpmc_blocking_benchmark::<6, 14>(1, 100_000_000, 4, 256);
    // bulk_drain_multi_mpmc_blocking_benchmark::<6, 14>(1, 100_000_000, 1, 256);
    //
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 10_000_000, 1, 1024);
    // bulk_drain_multi_moody_blocking_benchmark::<8, 10>(8, 10_000_000, 1, 1024);

    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 10_000_000, 2, 1024);
    // bulk_drain_multi_moody_blocking_benchmark::<8, 10>(8, 10_000_000, 2, 1024);

    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 10_000_000, 4, 4096);
    // bulk_drain_multi_moody_blocking_benchmark::<8, 10>(8, 10_000_000, 4, 4096);

    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 10_000_000, 5, 4096);
    // bulk_drain_multi_moody_blocking_benchmark::<8, 10>(8, 10_000_000, 5, 4096);

    bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(32, 10_000_000, 32, 4096);
    bulk_drain_multi_moody_blocking_benchmark::<8, 10>(32, 10_000_000, 32, 4096);
    // bulk_drain_multi_moody_blocking_benchmark::<8, 12>(1, 10_000_000, 1, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(1, 10_000_000, 1, 1024);
    // bulk_drain_multi_moody_blocking_benchmark::<8, 12>(1, 10_000_000, 2, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(1, 10_000_000, 2, 1024);

    // bulk_drain_multi_moody_blocking_benchmark::<8, 12>(1, 10_000_000, 2, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(1, 10_000_000, 2, 1024);

    // bulk_drain_multi_moody_blocking_benchmark::<8, 12>(8, 10_000_000, 2, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 10_000_000, 2, 1024);

    // bulk_drain_multi_moody_blocking_benchmark::<8, 12>(8, 10_000_000, 8, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 10_000_000, 8, 1024);

    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(1, 100_000_000, 2, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(1, 100_000_000, 4, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(1, 100_000_000, 8, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(2, 50_000_000, 1, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(2, 50_000_000, 2, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(2, 50_000_000, 4, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(2, 50_000_000, 8, 1024);
    // // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(1, 10_000_000, 8, 32);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 100_000_000, 1, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 100_000_000, 2, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 100_000_000, 4, 1024);
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 12>(8, 100_000_000, 8, 1024);

    // More examples:
    // P=13, NUM_SEGS_P2=0 -> seg_size=8192, num_segs=1, capacity=8191
    // bulk_drain_multi_mpmc_blocking_benchmark::<13, 0>(4, 100_000_000, 2, 1024);
    // P=16, NUM_SEGS_P2=0 -> seg_size=65536, num_segs=1, capacity=65535
    // bulk_drain_multi_mpmc_blocking_benchmark::<16, 0>(1, 100_000_000, 1, 8192);
    // P=8, NUM_SEGS_P2=0 -> seg_size=256, num_segs=1, capacity=255
    // bulk_drain_multi_mpmc_blocking_benchmark::<8, 0>(8, (25_000_000 / 256) * 256, 1, 8192);

    // Bulk both tests (push + drain)
    // println!("\n--- Bulk Both Blocking Tests ---");
    // P=13, NUM_SEGS_P2=0 -> seg_size=8192, num_segs=1, capacity=8191
    // bulk_both_mpmc_blocking_benchmark::<13, 0>(8, 20_000_000, 8192);

    println!("\n{}", "=".repeat(70));
    println!("Benchmark complete!");
    println!("{}", "=".repeat(70));
}
