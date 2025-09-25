use std::hint::spin_loop;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bop_executor::{Executor, SlotState, signal_nearest, signal_nearest_branchless};

const ZERO_U64X8: [u64; 8] = [0; 8];
const ZERO_U64X4: [u64; 4] = [0; 4];

fn find_first_set_bit_x8(bitmap: &[[u64; 8]]) -> Option<usize> {
    bitmap.iter().position(|block| block != &ZERO_U64X8)
}

fn find_first_set_bit_x4(bitmap: &[[u64; 4]]) -> Option<usize> {
    bitmap.iter().position(|block| block != &ZERO_U64X4)
}

fn find_first_set_bit_x1(bitmap: &[u64]) -> Option<(usize, usize)> {
    for (block_idx, block) in bitmap.iter().enumerate() {
        if *block != 0 {
            return Some((block_idx, block.trailing_zeros() as usize));
        }
    }
    None
}

fn find_first_set_bit_x4_2(bitmap: &[[u64; 4]]) -> Option<(usize, usize, usize)> {
    for (block_idx, lanes) in bitmap.iter().enumerate() {
        if lanes != &ZERO_U64X4 {
            for (lane_idx, &lane) in lanes.iter().enumerate() {
                if lane != 0 {
                    let bit = lane.trailing_zeros() as usize;
                    return Some((
                        block_idx,
                        block_idx * 8 + lane_idx,
                        block_idx * 512 + (lane_idx * 64 + bit),
                    ));
                }
            }
            return Some((block_idx, block_idx * 4, 0));
        }
    }
    None
}

fn find_first_set_bit_x8_2(bitmap: &[[u64; 8]]) -> Option<(usize, usize, usize)> {
    for (block_idx, lanes) in bitmap.iter().enumerate() {
        if lanes != &ZERO_U64X8 {
            for (lane_idx, &lane) in lanes.iter().enumerate() {
                if lane != 0 {
                    let bit = lane.trailing_zeros() as usize;
                    return Some((block_idx, block_idx * 8 + lane_idx, bit));
                }
            }
            return Some((block_idx, block_idx * 8, 0));
        }
    }
    None
}

fn find_first_set_bit_x8_16<const SIZE: usize>(
    bitmap: &[[u64; 8]; SIZE],
) -> Option<(usize, usize, usize)> {
    for (block_idx, lanes) in bitmap.iter().enumerate() {
        if lanes != &ZERO_U64X8 {
            for (lane_idx, &lane) in lanes.iter().enumerate() {
                if lane != 0 {
                    let bit = lane.trailing_zeros() as usize;
                    return Some((block_idx, block_idx * 8 + lane_idx, bit));
                }
            }
            return Some((block_idx, block_idx * 8, 0));
        }
    }
    None
}

fn benchmark<F>(label: &'static str, threads: usize, cycles: usize, iters: usize, f: F)
where
    F: Fn(usize, usize, usize) + Send + Sync + 'static,
{
    let f = Arc::new(f);
    let mut totals = Duration::ZERO;

    for cycle in 0..cycles {
        if threads == 1 {
            let start = Instant::now();
            for iter in 0..iters {
                f(0, cycle, iter);
            }
            totals += start.elapsed();
        } else {
            let mut joins = Vec::with_capacity(threads);
            for thread_idx in 0..threads {
                let f = Arc::clone(&f);
                joins.push(thread::spawn(move || {
                    let start = Instant::now();
                    for iter in 0..iters {
                        f(thread_idx, cycle, iter);
                    }
                    start.elapsed()
                }));
            }
            for handle in joins {
                totals += handle.join().expect("benchmark thread panicked");
            }
        }
    }

    let total_ops = (cycles * threads * iters) as f64;
    let avg_secs = if total_ops > 0.0 {
        totals.as_secs_f64() / total_ops
    } else {
        0.0
    };
    let avg_secs = avg_secs.max(f64::MIN_POSITIVE);
    let ops_per_sec = if avg_secs > 0.0 {
        (1.0 / avg_secs) * threads as f64
    } else {
        0.0
    };

    println!(
        "{label}: avg {:.3} ns/op, ops per sec {:.0}",
        avg_secs * 1e9,
        ops_per_sec
    );
}

fn run_bitset_demo() {
    println!("-- bitset demos --");

    let mut bitmap = vec![ZERO_U64X8; 16];
    bitmap[15][7] = 897_898_790_454_654_654;

    println!(
        "first non-zero block (x8): {:?}",
        find_first_set_bit_x8(&bitmap)
    );
    let bitmap4_view: Vec<[u64; 4]> = bitmap
        .iter()
        .map(|lane| [lane[0], lane[1], lane[2], lane[3]])
        .collect();
    println!(
        "first non-zero block (x4): {:?}",
        find_first_set_bit_x4(&bitmap4_view)
    );

    println!(
        "nearest bit (branchy): {}",
        signal_nearest(bitmap[15][7], 41)
    );
    println!(
        "nearest bit (branchless): {}",
        signal_nearest_branchless(bitmap[15][7], 41)
    );
    if let Some(hit) = find_first_set_bit_x8_2(&bitmap) {
        println!("first set via x8_2: {:?}", hit);
    }

    let mut bitmap16 = [[0u64; 8]; 16];
    bitmap16[15][7] = 878_978;
    println!(
        "first set via x8_16: {:?}",
        find_first_set_bit_x8_16(&bitmap16)
    );

    let mut bitmap4 = vec![ZERO_U64X4; 32];
    bitmap4[18][2] = 1;
    println!(
        "first set via x4_2: {:?}",
        find_first_set_bit_x4_2(&bitmap4)
    );

    let mut bitmap1 = vec![0u64; 128];
    bitmap1[72] = 1;
    println!("first set via x1: {:?}", find_first_set_bit_x1(&bitmap1));
}

fn run_mutex_benchmark() {
    println!("-- mutex benchmark --");
    let mutex = Arc::new(Mutex::new(()));
    benchmark("mutex lock/unlock", 1, 10, 10_000, {
        let mutex = Arc::clone(&mutex);
        move |_, _, _| {
            let _guard = mutex.lock().expect("mutex poisoned");
        }
    });
}

fn run_slot_demo() {
    println!("-- slot scheduling demo --");

    let executor = Executor::<1, true>::new();
    let group = &executor.groups()[0];
    let slot = group
        .reserve()
        .expect("signal group should provide at least one slot");

    unsafe fn reschedule(_: *mut ()) -> SlotState {
        SlotState::Idle
    }
    slot.set_worker(reschedule, std::ptr::null_mut());

    let signal = slot.signal().clone();
    let index = slot.index();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_worker = Arc::clone(&stop);
    let worker_slot = Arc::clone(&slot);

    thread::scope(|scope| {
        scope.spawn(move || {
            while !stop_worker.load(Ordering::Relaxed) {
                if signal.acquire(index) {
                    worker_slot.execute();
                } else {
                    spin_loop();
                }
            }
        });

        let schedule_slot = Arc::clone(&slot);
        benchmark("slot::schedule", 8, 5, 100_000_000, move |_, _, _| {
            let _ = schedule_slot.schedule();
        });

        stop.store(true, Ordering::Relaxed);
    });

    println!("waker counter {}", slot.waker().load());
}

fn main() {
    run_bitset_demo();
    run_mutex_benchmark();
    run_slot_demo();
}
