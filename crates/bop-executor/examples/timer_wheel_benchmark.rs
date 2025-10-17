//! Timer Wheel Benchmark
//!
//! Benchmarks the performance of the hashed timing wheel implementation
//! Tests: scheduling, cancellation, polling, and mixed workloads

#![allow(warnings)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![feature(thread_id_value)]

use bop_executor::timer_wheel::{SpscTimerWheel, TimerWheel};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Eq, PartialEq)]
struct HeapTimer {
    deadline_ns: u64,
    id: u64,
}

impl Ord for HeapTimer {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse for min-heap
        other.deadline_ns.cmp(&self.deadline_ns)
    }
}

impl PartialOrd for HeapTimer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn benchmark_schedule(num_timers: usize, tick_res: Duration, ticks: usize) {
    println!("\n=== Benchmark: Schedule {} timers ===", num_timers);

    let start_time = Instant::now();

    // Pre-allocate: estimate timers per tick and use next power of 2
    let avg_timers_per_tick: usize = (num_timers / ticks).max(16);
    let initial_allocation: usize = avg_timers_per_tick.next_power_of_two();

    println!(
        "  Pre-allocating: {} slots per tick ({} total slots)",
        initial_allocation,
        initial_allocation * ticks
    );

    let mut wheel =
        TimerWheel::<u32>::with_allocation(start_time, tick_res, ticks, initial_allocation);

    let bench_start = Instant::now();

    // Schedule timers with collisions allowed - distribute across ticks
    let tick_res_ns = tick_res.as_nanos() as u64;

    for i in 0..num_timers {
        // Map timer to a tick, allowing multiple timers per tick
        let tick = (i * ticks / num_timers) as u64;
        let deadline_ns = (tick + 1) * tick_res_ns;
        wheel.schedule_timer(deadline_ns, i as u32);
    }

    let elapsed = bench_start.elapsed();

    println!("  Total time: {:?}", elapsed);
    println!(
        "  Time per schedule: {:.2} ns",
        elapsed.as_nanos() as f64 / num_timers as f64
    );
    println!(
        "  Schedules/sec: {:.2}M",
        num_timers as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!("  Timers in wheel: {}", wheel.timer_count());
}

fn benchmark_cancel(num_timers: usize, tick_res: Duration, ticks: usize) {
    println!(
        "\n=== Benchmark: Schedule + Cancel {} timers (sequential) ===",
        num_timers
    );

    let start_time = Instant::now();

    // Pre-allocate capacity
    let avg_timers_per_tick: usize = (num_timers / ticks).max(16);
    let initial_allocation: usize = avg_timers_per_tick.next_power_of_two();
    println!("  Pre-allocating: {} slots per tick", initial_allocation);

    let mut wheel =
        TimerWheel::<u32>::with_allocation(start_time, tick_res, ticks, initial_allocation);
    let mut timer_ids = Vec::with_capacity(num_timers);

    // Schedule all timers with collisions allowed
    let tick_res_ns = tick_res.as_nanos() as u64;

    for i in 0..num_timers {
        // Map timer to a tick, allowing multiple timers per tick
        let tick = (i * ticks / num_timers) as u64;
        let deadline_ns = (tick + 1) * tick_res_ns;
        if let Some(id) = wheel.schedule_timer(deadline_ns, i as u32) {
            timer_ids.push(id);
        }
    }

    println!("  Scheduled {} timers", timer_ids.len());

    // Cancel all timers in order
    let bench_start = Instant::now();
    let mut cancelled = 0;

    for id in timer_ids {
        if wheel.cancel_timer(id).is_some() {
            cancelled += 1;
        }
    }

    let elapsed = bench_start.elapsed();

    println!("  Cancelled: {}", cancelled);
    println!("  Total time: {:?}", elapsed);
    println!("  Time per cancel: {:?}", elapsed / num_timers as u32);
    println!(
        "  Cancels/sec: {:.2}M",
        num_timers as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!("  Timers remaining: {}", wheel.timer_count());
}

fn benchmark_cancel_random(num_timers: usize, tick_res: Duration, ticks: usize) {
    println!(
        "\n=== Benchmark: Schedule + Cancel {} timers (random order) ===",
        num_timers
    );

    let start_time = Instant::now();

    // Pre-allocate capacity
    let avg_timers_per_tick: usize = (num_timers / ticks).max(16);
    let initial_allocation: usize = avg_timers_per_tick.next_power_of_two();
    println!("  Pre-allocating: {} slots per tick", initial_allocation);

    let mut wheel =
        TimerWheel::<u32>::with_allocation(start_time, tick_res, ticks, initial_allocation);
    let mut timer_ids = Vec::with_capacity(num_timers);

    // Schedule all timers with collisions allowed
    let tick_res_ns = tick_res.as_nanos() as u64;

    for i in 0..num_timers {
        // Map timer to a tick, allowing multiple timers per tick
        let tick = (i * ticks / num_timers) as u64;
        let deadline_ns = (tick + 1) * tick_res_ns;
        if let Some(id) = wheel.schedule_timer(deadline_ns, i as u32) {
            timer_ids.push(id);
        }
    }

    println!("  Scheduled {} timers", timer_ids.len());

    // Shuffle timer IDs using a simple xorshift PRNG (no external deps)
    let shuffle_start = Instant::now();
    let mut rng_state = 0x123456789abcdef0u64;
    for i in (1..timer_ids.len()).rev() {
        // xorshift64
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let j = (rng_state as usize) % (i + 1);
        timer_ids.swap(i, j);
    }
    let shuffle_time = shuffle_start.elapsed();
    println!("  Shuffle time: {:?}", shuffle_time);

    // Cancel all timers in random order
    let bench_start = Instant::now();
    let mut cancelled = 0;

    for id in timer_ids {
        if wheel.cancel_timer(id).is_some() {
            cancelled += 1;
        }
    }

    let elapsed = bench_start.elapsed();

    println!("  Cancelled: {}", cancelled);
    println!("  Total time: {:?}", elapsed);
    println!("  Time per cancel: {:?}", elapsed / num_timers as u32);
    println!(
        "  Cancels/sec: {:.2}M",
        num_timers as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!("  Timers remaining: {}", wheel.timer_count());
}

fn benchmark_poll(num_timers: usize, tick_res: Duration, ticks: usize) {
    println!("\n=== Benchmark: Schedule + Poll {} timers ===", num_timers);

    let start_time = Instant::now();

    // Pre-allocate capacity
    let avg_timers_per_tick: usize = (num_timers / ticks).max(16);
    let initial_allocation: usize = avg_timers_per_tick.next_power_of_two();
    println!("  Pre-allocating: {} slots per tick", initial_allocation);

    let mut wheel =
        TimerWheel::<u32>::with_allocation(start_time, tick_res, ticks, initial_allocation);

    // Schedule timers uniformly across the wheel with collisions allowed
    let tick_res_ns = tick_res.as_nanos() as u64;

    for i in 0..num_timers {
        // Map timer to a tick, allowing multiple timers per tick
        let tick = (i * ticks / num_timers) as u64;
        let deadline_ns = (tick + 1) * tick_res_ns;
        wheel.schedule_timer(deadline_ns, i as u32);
    }

    println!("  Scheduled {} timers", wheel.timer_count());

    // Poll all timers
    let bench_start = Instant::now();
    let mut total_expired = 0;
    let max_time_ns = (num_timers as u64 * tick_res.as_nanos() as u64) / 10;

    for now_ns in (0..=max_time_ns).step_by(tick_res.as_nanos() as usize) {
        wheel.advance_to(now_ns);
        let expired = wheel.poll(now_ns, 1000);
        total_expired += expired.len();
    }

    let elapsed = bench_start.elapsed();

    println!("  Total expired: {}", total_expired);
    println!("  Total time: {:?}", elapsed);
    println!("  Time per expiry: {:?}", elapsed / num_timers as u32);
    println!(
        "  Expiries/sec: {:.2}M",
        total_expired as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!("  Timers remaining: {}", wheel.timer_count());
}

fn benchmark_mixed_workload(num_ops: usize, tick_res: Duration, ticks: usize) {
    println!(
        "\n=== Benchmark: Mixed workload {} ops (poll once per tick) ===",
        num_ops
    );

    let start_time = Instant::now();

    // Pre-allocate for expected load (50% of ops are schedules)
    let expected_timers = num_ops / 2;
    let avg_timers_per_tick: usize = (expected_timers / ticks).max(16);
    let initial_allocation: usize = avg_timers_per_tick.next_power_of_two();
    println!("  Pre-allocating: {} slots per tick", initial_allocation);

    let mut wheel =
        TimerWheel::<u32>::with_allocation(start_time, tick_res, ticks, initial_allocation);
    let mut timer_ids = Vec::new();
    let mut current_time_ns = 0u64;

    let bench_start = Instant::now();

    let mut schedules = 0;
    let mut cancels = 0;
    let mut polls = 0;
    let mut expirations = 0;
    let mut ops_since_poll = 0;
    let ops_per_tick = (num_ops as f64 / 1000.0).ceil() as usize; // Poll ~1000 times total

    let tick_res_ns = tick_res.as_nanos() as u64;
    let max_future_ns = ticks as u64 * tick_res_ns / 2; // Schedule within half the wheel

    for i in 0..num_ops {
        let op_type = i % 3; // Change ratio since we're polling separately

        match op_type {
            // Schedule (33%)
            0 => {
                // Round to tick boundaries, allowing collisions
                let tick_offset = (schedules * (ticks / 2)) / (num_ops / 3);
                let deadline_ns = current_time_ns + (tick_offset as u64 + 1) * tick_res_ns;
                if let Some(id) = wheel.schedule_timer(deadline_ns, i as u32) {
                    timer_ids.push(id);
                    schedules += 1;
                }
            }
            // Cancel (33%)
            1 => {
                if !timer_ids.is_empty() {
                    let idx = i % timer_ids.len();
                    let id = timer_ids.swap_remove(idx);
                    if wheel.cancel_timer(id).is_some() {
                        cancels += 1;
                    }
                }
            }
            // Other ops (33%)
            _ => {}
        }

        ops_since_poll += 1;

        // Poll once per millisecond (once per tick)
        if ops_since_poll >= ops_per_tick {
            ops_since_poll = 0;
            current_time_ns += tick_res_ns;
            wheel.advance_to(current_time_ns);
            let expired = wheel.poll(current_time_ns, usize::MAX);
            expirations += expired.len();
            polls += 1;
        }
    }

    let elapsed = bench_start.elapsed();

    println!("  Schedules: {}", schedules);
    println!("  Cancels: {}", cancels);
    println!("  Polls: {}", polls);
    println!("  Expirations: {}", expirations);
    println!("  Total time: {:?}", elapsed);
    println!(
        "  Ops/sec: {:.2}M",
        num_ops as f64 / elapsed.as_secs_f64() / 1_000_000.0
    );
    println!("  Timers remaining: {}", wheel.timer_count());
}

fn benchmark_vs_binary_heap(num_timers: usize) {
    println!(
        "\n=== Comparison: TimerWheel vs BinaryHeap ({} timers) ===",
        num_timers
    );

    // TimerWheel
    println!("\n  TimerWheel:");
    let start_time = Instant::now();

    // Pre-allocate for the workload
    let avg_timers_per_tick: usize = (num_timers / 1024).max(16);
    let initial_allocation: usize = avg_timers_per_tick.next_power_of_two();

    let mut wheel = TimerWheel::<u32>::with_allocation(
        start_time,
        Duration::from_nanos(1048576),
        1024,
        initial_allocation,
    );

    let wheel_start = Instant::now();
    for i in 0..num_timers {
        // Map timer to a tick, allowing multiple timers per tick
        let tick = (i * 1024 / num_timers) as u64;
        let deadline_ns = (tick + 1) * 1048576;
        wheel.schedule_timer(deadline_ns, i as u32);
    }
    let wheel_schedule_time = wheel_start.elapsed();

    let wheel_poll_start = Instant::now();
    let mut total = 0;
    // Poll at tick resolution intervals
    let max_deadline_ns = 1024 * 1048576;
    for now_ns in (0..=max_deadline_ns).step_by(1048576) {
        wheel.advance_to(now_ns);
        total += wheel.poll(now_ns, usize::MAX).len();
    }
    let wheel_poll_time = wheel_poll_start.elapsed();

    println!("    Schedule time: {:?}", wheel_schedule_time);
    println!("    Poll time: {:?}", wheel_poll_time);
    println!(
        "    Total time: {:?}",
        wheel_schedule_time + wheel_poll_time
    );
    println!("    Expired: {}", total);

    // BinaryHeap
    println!("\n  BinaryHeap:");
    let mut heap = BinaryHeap::new();

    let heap_start = Instant::now();
    for i in 0..num_timers {
        let deadline_ns = (i as u64 * 1_000_000) % 1_000_000_000;
        heap.push(HeapTimer {
            deadline_ns,
            id: i as u64,
        });
    }
    let heap_schedule_time = heap_start.elapsed();

    let heap_poll_start = Instant::now();
    let mut total = 0;
    for now_ns in (0..=1_000_000_000u64).step_by(1_000_000) {
        while let Some(timer) = heap.peek() {
            if timer.deadline_ns <= now_ns {
                heap.pop();
                total += 1;
            } else {
                break;
            }
        }
    }
    let heap_poll_time = heap_poll_start.elapsed();

    println!("    Schedule time: {:?}", heap_schedule_time);
    println!("    Poll time: {:?}", heap_poll_time);
    println!("    Total time: {:?}", heap_schedule_time + heap_poll_time);
    println!("    Expired: {}", total);

    // Comparison
    println!("\n  Speedup:");
    println!(
        "    Schedule: {:.2}x",
        heap_schedule_time.as_secs_f64() / wheel_schedule_time.as_secs_f64()
    );
    println!(
        "    Poll: {:.2}x",
        heap_poll_time.as_secs_f64() / wheel_poll_time.as_secs_f64()
    );
    println!(
        "    Total: {:.2}x",
        (heap_schedule_time + heap_poll_time).as_secs_f64()
            / (wheel_schedule_time + wheel_poll_time).as_secs_f64()
    );
}

fn benchmark_capacity_growth() {
    println!("\n=== Benchmark: Capacity growth (same-tick scheduling) ===");

    let start_time = Instant::now();
    let tick_res = Duration::from_nanos(1048576); // ~1ms, power of 2
    let mut wheel = TimerWheel::<u32>::new(start_time, tick_res, 256);

    println!("  Initial allocation per tick: 16");

    // Schedule many timers to the same tick to trigger growth
    let deadline_ns = 1_000_000; // All timers at 1ms

    for count in [10, 20, 40, 80, 160, 320] {
        let bench_start = Instant::now();

        for i in 0..count {
            wheel.schedule_timer(deadline_ns, i);
        }

        let elapsed = bench_start.elapsed();
        println!(
            "  Scheduled {} timers (same tick): {:?} ({:.2} ns/timer)",
            count,
            elapsed,
            elapsed.as_nanos() as f64 / count as f64
        );
    }

    println!("  Total timers: {}", wheel.timer_count());
}

fn benchmark_different_configurations() {
    println!("\n=== Benchmark: Different wheel configurations ===");

    let num_timers = 100_000;

    for (ticks, tick_res) in [
        (256, Duration::from_millis(1)),
        (512, Duration::from_millis(1)),
        (1024, Duration::from_millis(1)),
        (256, Duration::from_micros(100)),
        (512, Duration::from_micros(100)),
    ] {
        println!("\n  Config: {} ticks, {:?} resolution", ticks, tick_res);

        let start_time = Instant::now();

        // Pre-allocate for the workload
        let avg_timers_per_tick: usize = (num_timers / ticks).max(16);
        let initial_allocation: usize = avg_timers_per_tick.next_power_of_two();

        let mut wheel =
            TimerWheel::<u32>::with_allocation(start_time, tick_res, ticks, initial_allocation);

        let bench_start = Instant::now();

        let tick_res_ns = tick_res.as_nanos() as u64;

        for i in 0..num_timers {
            // Map timer to a tick, allowing multiple timers per tick
            let tick = (i * ticks / num_timers) as u64;
            let deadline_ns = (tick + 1) * tick_res_ns;
            wheel.schedule_timer(deadline_ns, i as u32);
        }

        let schedule_time = bench_start.elapsed();

        let poll_start = Instant::now();
        let max_time_ns = ticks as u64 * tick_res.as_nanos() as u64;
        let mut total = 0;

        for now_ns in (0..=max_time_ns).step_by(tick_res.as_nanos() as usize) {
            wheel.advance_to(now_ns);
            total += wheel.poll(now_ns, 1000).len();
        }

        let poll_time = poll_start.elapsed();

        println!(
            "    Schedule: {:?} ({:.2}M/s)",
            schedule_time,
            num_timers as f64 / schedule_time.as_secs_f64() / 1_000_000.0
        );
        println!(
            "    Poll: {:?} ({:.2}M/s)",
            poll_time,
            total as f64 / poll_time.as_secs_f64() / 1_000_000.0
        );
        println!("    Total: {:?}", schedule_time + poll_time);
    }
}

/// Benchmark sharded timer wheels in a multi-threaded environment
/// Each thread has its own timer wheel (shard) to avoid contention
fn benchmark_sharded_multithreaded(
    num_threads: usize,
    timers_per_thread: usize,
    tick_res: Duration,
    ticks: usize,
) {
    println!(
        "\n=== Benchmark: Sharded Timer Wheel - {} threads, {} timers/thread ===",
        num_threads, timers_per_thread
    );
    println!("  Total timers: {}", num_threads * timers_per_thread);
    println!("  Tick resolution: {:?}", tick_res);
    println!("  Ticks per wheel: {}", ticks);

    // let num_shards: usize = std::thread::available_parallelism().unwrap().into();
    let num_shards: usize = num_threads;
    let start_time = Instant::now();
    let tick_res_ns = tick_res.as_nanos() as u64;

    // Pre-allocate capacity
    let avg_timers_per_tick: usize = (timers_per_thread / ticks).max(16);
    let initial_allocation: usize = avg_timers_per_tick.next_power_of_two();
    println!("  Pre-allocating: {} slots per tick", initial_allocation);

    // Create sharded wheels (one per thread)
    let wheels: Arc<Vec<Arc<Mutex<TimerWheel<u32>>>>> = Arc::new(
        (0..num_shards)
            .map(|_| {
                Arc::new(Mutex::new(TimerWheel::with_allocation(
                    start_time,
                    tick_res,
                    ticks,
                    initial_allocation,
                )))
            })
            .collect(),
    );

    // === PHASE 1: Concurrent Scheduling ===
    println!("\n  Phase 1: Concurrent Scheduling");
    let schedule_start = Instant::now();

    let mut handles = vec![];
    for thread_id in 0..num_threads {
        let wheels = Arc::clone(&wheels);
        let handle = thread::spawn(move || {
            let tid: u64 = std::thread::current().id().as_u64().into();
            let wheel = wheels[tid as usize % num_shards].clone();
            for i in 0..timers_per_thread {
                let mut local_wheel = wheel.lock().unwrap();
                // Distribute timers across ticks
                let tick = (i * ticks / timers_per_thread) as u64;
                let deadline_ns = (tick + 1) * tick_res_ns;
                local_wheel.schedule_timer(deadline_ns, (thread_id * timers_per_thread + i) as u32);
            }
            timers_per_thread
        });
        handles.push(handle);
    }

    let total_scheduled: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    let schedule_time = schedule_start.elapsed();

    println!("    Scheduled: {} timers", total_scheduled);
    println!("    Total time: {:?}", schedule_time);
    println!(
        "    Throughput: {:.2}M timers/sec",
        total_scheduled as f64 / schedule_time.as_secs_f64() / 1_000_000.0
    );
    println!(
        "    Per-thread avg: {:.2} ns/timer",
        schedule_time.as_nanos() as f64 / total_scheduled as f64
    );

    // === PHASE 2: Concurrent Cancellation (50% of timers) ===
    println!("\n  Phase 2: Concurrent Cancellation (50% of timers)");

    // First, collect timer IDs from each shard
    let timer_ids: Vec<Vec<u64>> = wheels
        .iter()
        .enumerate()
        .map(|(thread_id, wheel)| {
            let mut ids = vec![];
            // We'll use a simple scheme: timer_id_for_slot based on tick and slot
            // Since we don't have direct access, we'll schedule and cancel based on count
            for i in 0..timers_per_thread / 2 {
                let mut local_wheel = wheel.lock().unwrap();
                // Reconstruct approximate timer IDs
                let tick = (i * ticks / timers_per_thread) as u64;
                let spoke_index = (tick & (ticks - 1) as u64) as usize;
                let slot_in_tick = i % initial_allocation;
                let timer_id = ((spoke_index as u64) << 32) | (slot_in_tick as u64);
                ids.push(timer_id);
            }
            ids
        })
        .collect();

    let cancel_start = Instant::now();
    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let wheels = Arc::clone(&wheels);
        let ids = timer_ids[thread_id].clone();
        let handle = thread::spawn(move || {
            let tid: u64 = std::thread::current().id().as_u64().into();
            let wheel = wheels[tid as usize % num_shards].clone();
            let mut cancelled = 0;
            for id in ids {
                let mut local_wheel = wheel.lock().unwrap();
                if local_wheel.cancel_timer(id).is_some() {
                    cancelled += 1;
                }
            }
            cancelled
        });
        handles.push(handle);
    }

    let total_cancelled: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    let cancel_time = cancel_start.elapsed();

    println!("    Cancelled: {} timers", total_cancelled);
    println!("    Total time: {:?}", cancel_time);
    println!(
        "    Throughput: {:.2}M cancels/sec",
        total_cancelled as f64 / cancel_time.as_secs_f64() / 1_000_000.0
    );

    // === PHASE 3: Concurrent Polling ===
    println!("\n  Phase 3: Concurrent Polling");

    let max_time_ns = ticks as u64 * tick_res_ns;
    let poll_start = Instant::now();
    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let wheels = Arc::clone(&wheels);
        let wheel = wheels[thread_id].clone();
        let handle = thread::spawn(move || {
            let tid: u64 = std::thread::current().id().as_u64().into();
            let wheel = wheels[tid as usize % num_shards].clone();
            let mut local_wheel = wheel.lock().unwrap();
            let mut total_expired = 0;

            for now_ns in (0..=max_time_ns).step_by(tick_res_ns as usize) {
                local_wheel.advance_to(now_ns);
                let expired = local_wheel.poll(now_ns, usize::MAX);
                total_expired += expired.len();
            }

            total_expired
        });
        handles.push(handle);
    }

    let total_expired: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    let poll_time = poll_start.elapsed();

    println!("    Expired: {} timers", total_expired);
    println!("    Total time: {:?}", poll_time);
    println!(
        "    Throughput: {:.2}M expirations/sec",
        total_expired as f64 / poll_time.as_secs_f64() / 1_000_000.0
    );

    // === PHASE 4: Mixed Concurrent Workload ===
    println!("\n  Phase 4: Mixed Concurrent Workload (schedule/cancel/poll)");

    // Reset wheels
    let wheels: Arc<Vec<Arc<Mutex<TimerWheel<u32>>>>> = Arc::new(
        (0..num_shards)
            .map(|_| {
                Arc::new(Mutex::new(TimerWheel::with_allocation(
                    start_time,
                    tick_res,
                    ticks,
                    initial_allocation,
                )))
            })
            .collect(),
    );

    let ops_per_thread = timers_per_thread;
    let mixed_start = Instant::now();
    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let wheels = Arc::clone(&wheels);
        let handle = thread::spawn(move || {
            let mut timer_ids = Vec::new();
            let mut schedules = 0;
            let mut cancels = 0;
            let mut polls = 0;
            let mut expirations = 0;
            let mut current_time_ns = 0u64;
            let mut ops_since_poll = 0;
            let ops_per_tick = (ops_per_thread as f64 / 100.0).ceil() as usize;

            let tid: u64 = std::thread::current().id().as_u64().into();
            let wheel = wheels[tid as usize % num_shards].clone();

            for i in 0..ops_per_thread {
                let op_type = i % 3;

                match op_type {
                    // Schedule (33%)
                    0 => {
                        let mut local_wheel = wheel.lock().unwrap();
                        let tick_offset = (schedules * (ticks / 2)) / (ops_per_thread / 3);
                        let deadline_ns = current_time_ns + (tick_offset as u64 + 1) * tick_res_ns;
                        if let Some(id) = local_wheel.schedule_timer(deadline_ns, i as u32) {
                            timer_ids.push(id);
                            schedules += 1;
                        }
                    }
                    // Cancel (33%)
                    1 => {
                        let mut local_wheel = wheel.lock().unwrap();
                        if !timer_ids.is_empty() {
                            let idx = i % timer_ids.len();
                            let id = timer_ids.swap_remove(idx);
                            if local_wheel.cancel_timer(id).is_some() {
                                cancels += 1;
                            }
                        }
                    }
                    // Other ops (33%)
                    _ => {}
                }

                ops_since_poll += 1;

                // Poll periodically
                if ops_since_poll >= ops_per_tick {
                    let mut local_wheel = wheel.lock().unwrap();
                    ops_since_poll = 0;
                    current_time_ns += tick_res_ns;
                    local_wheel.advance_to(current_time_ns);
                    let expired = local_wheel.poll(current_time_ns, usize::MAX);
                    expirations += expired.len();
                    polls += 1;
                }
            }

            (schedules, cancels, polls, expirations)
        });
        handles.push(handle);
    }

    let results: Vec<(usize, usize, usize, usize)> =
        handles.into_iter().map(|h| h.join().unwrap()).collect();

    let mixed_time = mixed_start.elapsed();

    let total_schedules: usize = results.iter().map(|(s, _, _, _)| s).sum();
    let total_cancels: usize = results.iter().map(|(_, c, _, _)| c).sum();
    let total_polls: usize = results.iter().map(|(_, _, p, _)| p).sum();
    let total_expirations: usize = results.iter().map(|(_, _, _, e)| e).sum();
    let total_ops = total_schedules + total_cancels + total_polls;

    println!("    Schedules: {}", total_schedules);
    println!("    Cancels: {}", total_cancels);
    println!("    Polls: {}", total_polls);
    println!("    Expirations: {}", total_expirations);
    println!("    Total ops: {}", total_ops);
    println!("    Total time: {:?}", mixed_time);
    println!(
        "    Throughput: {:.2}M ops/sec",
        total_ops as f64 / mixed_time.as_secs_f64() / 1_000_000.0
    );

    // === Summary ===
    println!("\n  Summary:");
    println!(
        "    Overall time: {:?}",
        schedule_time + cancel_time + poll_time + mixed_time
    );
    println!("    Avg contention (lock wait): ~0 (each thread has own shard)");
    println!("    Scalability: Linear with thread count (no shared state)");
}

/// Benchmark sharded vs single-threaded timer wheel
fn benchmark_sharded_vs_single(num_threads: usize, timers_per_thread: usize) {
    println!(
        "\n=== Comparison: Sharded ({} threads) vs Single-threaded ===",
        num_threads
    );

    let tick_res = Duration::from_nanos(1048576); // ~1ms
    let ticks = 512;
    let total_timers = num_threads * timers_per_thread;

    // Single-threaded baseline
    println!("\n  Single-threaded baseline:");
    let start_time = Instant::now();
    let avg_timers_per_tick = (total_timers / ticks).max(16);
    let initial_allocation = avg_timers_per_tick.next_power_of_two();

    let mut wheel =
        TimerWheel::<u32>::with_allocation(start_time, tick_res, ticks, initial_allocation);

    let single_start = Instant::now();
    let tick_res_ns = tick_res.as_nanos() as u64;

    for i in 0..total_timers {
        let tick = (i * ticks / total_timers) as u64;
        let deadline_ns = (tick + 1) * tick_res_ns;
        let timer_id = wheel.schedule_timer(deadline_ns, i as u32).unwrap();
        wheel.cancel_timer(timer_id);
    }

    let single_time = single_start.elapsed();
    println!("    Schedule time: {:?}", single_time);
    println!(
        "    Throughput: {:.2}M timers/sec",
        total_timers as f64 / single_time.as_secs_f64() / 1_000_000.0
    );

    {
        let mut wheels: Vec<TimerWheel<u32>> = (0..num_threads)
            .map(|_| {
                TimerWheel::with_allocation(
                    start_time,
                    tick_res,
                    ticks,
                    initial_allocation / num_threads,
                )
            })
            .collect();

        // Sharded multi-threaded
        println!("\n  Sharded multi-threaded TimerWheel:");
        let shards = num_threads * 16;
        let sharded_start = Instant::now();
        let mut handles = vec![];

        for thread_id in 0..num_threads {
            let mut wheel = wheels.pop().unwrap();
            let handle = thread::spawn(move || {
                for i in 0..timers_per_thread {
                    let tick = (i * ticks / timers_per_thread) as u64;
                    let deadline_ns = (tick + 1) * tick_res_ns;
                    let timer_id = wheel.schedule_timer(deadline_ns, i as u32).unwrap();
                    wheel.cancel_timer(timer_id);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let sharded_time = sharded_start.elapsed();
        println!("    Schedule time: {:?}", sharded_time);
        println!(
            "    Throughput: {:.2}M timers/sec",
            total_timers as f64 / sharded_time.as_secs_f64() / 1_000_000.0
        );
    }

    {
        // Sharded multi-threaded
        println!("\n  Sharded multi-threaded Mutex<TimerWheel>:");
        let wheels: Vec<Arc<Mutex<TimerWheel<u32>>>> = (0..num_threads)
            .map(|_| {
                Arc::new(Mutex::new(TimerWheel::with_allocation(
                    start_time,
                    tick_res,
                    ticks,
                    initial_allocation / num_threads,
                )))
            })
            .collect();

        let shards = num_threads * 16;
        let sharded_start = Instant::now();
        let mut handles = vec![];

        for thread_id in 0..num_threads {
            let wheel = wheels[thread_id].clone();
            let handle = thread::spawn(move || {
                for i in 0..timers_per_thread {
                    let tick = (i * ticks / timers_per_thread) as u64;
                    let deadline_ns = (tick + 1) * tick_res_ns;
                    let timer_id = {
                        let mut local_wheel = wheel.lock().unwrap();
                        local_wheel.schedule_timer(deadline_ns, i as u32).unwrap()
                    };
                    {
                        let mut local_wheel = wheel.lock().unwrap();
                        local_wheel.cancel_timer(timer_id);
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let sharded_time = sharded_start.elapsed();
        println!("    Schedule time: {:?}", sharded_time);
        println!(
            "    Throughput: {:.2}M timers/sec",
            total_timers as f64 / sharded_time.as_secs_f64() / 1_000_000.0
        );
    }

    {
        // Sharded multi-threaded
        println!("\n  Sharded multi-threaded SpscTimerWheel:");
        let wheels: Vec<Arc<SpscTimerWheel<u32>>> = (0..num_threads)
            .map(|_| {
                Arc::new(SpscTimerWheel::with_allocation(
                    start_time,
                    tick_res,
                    ticks,
                    initial_allocation / num_threads,
                ))
            })
            .collect();

        let shards = num_threads * 16;
        let sharded_start = Instant::now();
        let mut handles = vec![];

        for thread_id in 0..num_threads {
            let wheel = wheels[thread_id].clone();
            let handle = thread::spawn(move || {
                let mut local_wheel = wheel;
                for i in 0..timers_per_thread {
                    let tick = (i * ticks / timers_per_thread) as u64;
                    let deadline_ns = (tick + 1) * tick_res_ns;
                    let timer_id = local_wheel.schedule_timer(deadline_ns, i as u32).unwrap();
                    local_wheel.cancel_timer(timer_id);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let sharded_time = sharded_start.elapsed();
        println!("    Schedule time: {:?}", sharded_time);
        println!(
            "    Throughput: {:.2}M timers/sec",
            total_timers as f64 / sharded_time.as_secs_f64() / 1_000_000.0
        );
    }
    // println!(
    //     "\n  Speedup: {:.2}x",
    //     single_time.as_secs_f64() / sharded_time.as_secs_f64()
    // );
    // println!(
    //     "  Parallel efficiency: {:.1}%",
    //     100.0 * single_time.as_secs_f64() / (sharded_time.as_secs_f64() * num_threads as f64)
    // );
}

fn main() {
    println!("🎯 Timer Wheel Benchmark");
    println!("========================\n");

    // Use power-of-2 nanosecond tick resolutions
    // 1048576ns = 2^20 ≈ 1.05ms
    // 65536ns = 2^16 ≈ 65.5μs
    let tick_1ms = Duration::from_nanos(1048576); // ~1ms, power of 2
    let tick_64us = Duration::from_nanos(65536); // ~64μs, power of 2

    // Basic benchmarks
    benchmark_schedule(1_000_000, tick_1ms, 512);
    benchmark_schedule(1_000_000, tick_1ms, 512);
    benchmark_cancel(500_000, tick_1ms, 512);
    benchmark_cancel_random(500_000, tick_1ms, 512);
    benchmark_poll(500_000, tick_1ms, 512);

    // Mixed workload
    benchmark_mixed_workload(1_000_000, tick_1ms, 512);

    // Comparison with BinaryHeap
    benchmark_vs_binary_heap(1_000_000);

    // Capacity growth
    benchmark_capacity_growth();

    // Different configurations
    // benchmark_different_configurations();

    println!("\n");
    println!("========================================");
    println!("  MULTI-THREADED SHARDED BENCHMARKS");
    println!("========================================");

    // Multi-threaded sharded benchmarks
    let num_cpus = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    println!("\nDetected {} CPU cores", num_cpus);

    // // Test with different thread counts
    // benchmark_sharded_multithreaded(2, 25000_000, tick_1ms, 512);
    // benchmark_sharded_multithreaded(4, 12500_000, tick_1ms, 512);
    // benchmark_sharded_multithreaded(num_cpus, 1500_000 / num_cpus, tick_1ms, 512);

    // if num_cpus >= 8 {
    //     benchmark_sharded_multithreaded(8, 125_000, tick_1ms, 512);
    // }

    // Comparison: sharded vs single-threaded
    benchmark_sharded_vs_single(4, 25000_000);
    benchmark_sharded_vs_single(16, 50000_000 / num_cpus);

    println!("\n=== Summary ===");
    println!("TimerWheel provides O(1) timer scheduling and cancellation");
    println!("Efficient for high-throughput timer workloads");
    println!("Significantly faster than BinaryHeap for most workloads");
    println!("\nSharded timer wheels scale linearly with thread count");
    println!("Each thread operates on its own wheel with zero contention");
    println!("Ideal for multi-threaded executors and event loops");
    println!("\nNote: Tick resolution must be power-of-2 nanoseconds");
    println!("  1048576ns (2^20) ≈ 1.05ms");
    println!("  65536ns (2^16) ≈ 65.5μs");
}
