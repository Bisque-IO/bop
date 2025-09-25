use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use bop_mpmc::BlockingQueue;
use crossfire::mpsc;

const DEFAULT_OPS_PER_THREAD: usize = 100_000;
const DEFAULT_QUEUE_CAPACITY: usize = 1024;

fn main() {
    let args: Vec<String> = env::args().collect();
    let ops_per_thread = args
        .get(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_OPS_PER_THREAD);
    let queue_capacity = args
        .get(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_QUEUE_CAPACITY);

    println!(
        "# crossfire::mpsc bounded_blocking throughput\n# ops_per_thread={} queue_capacity={}",
        ops_per_thread, queue_capacity
    );
    println!("threads,total_ops,elapsed_ms,ops_per_sec");
    for producers in 1..=8 {
        run_crossfire_case(producers, ops_per_thread, queue_capacity);
    }

    println!(
        "\n# bop-mpmc::BlockingQueue throughput\n# ops_per_thread={}",
        ops_per_thread
    );
    println!("threads,total_ops,elapsed_ms,ops_per_sec");
    for producers in 1..=8 {
        run_bop_mpmc_case(producers, ops_per_thread);
    }
}

fn run_crossfire_case(producers: usize, ops_per_thread: usize, queue_capacity: usize) {
    let (tx, rx) = mpsc::bounded_blocking(queue_capacity.max(1));
    let expected = producers * ops_per_thread;
    let counter = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(producers + 1));

    let mut handles = Vec::with_capacity(producers);
    for _ in 0..producers {
        let tx = tx.clone();
        let barrier = barrier.clone();
        let counter = counter.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            for _ in 0..ops_per_thread {
                tx.send(()).expect("send failed");
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    drop(tx);

    barrier.wait();
    let start = Instant::now();
    for _ in 0..expected {
        rx.recv().expect("recv failed");
    }
    let elapsed = start.elapsed();

    for handle in handles {
        handle.join().expect("producer thread panicked");
    }

    report(producers, counter.load(Ordering::Relaxed), elapsed);
}

fn run_bop_mpmc_case(producers: usize, ops_per_thread: usize) {
    let queue = Arc::new(BlockingQueue::new().expect("failed to create bop-mpmc queue"));
    let expected = producers * ops_per_thread;
    let counter = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(producers + 1));

    let mut handles = Vec::with_capacity(producers);
    for _ in 0..producers {
        let queue = queue.clone();
        let barrier = barrier.clone();
        let counter = counter.clone();
        handles.push(thread::spawn(move || {
            let producer = queue
                .create_producer_token()
                .expect("failed to create producer token");
            barrier.wait();
            for _ in 0..ops_per_thread {
                producer.enqueue(1);
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    let consumer = queue
        .create_consumer_token()
        .expect("failed to create consumer token");

    barrier.wait();
    let start = Instant::now();
    for _ in 0..expected {
        while consumer.dequeue().is_none() {
            std::hint::spin_loop();
        }
    }
    let elapsed = start.elapsed();

    for handle in handles {
        handle.join().expect("producer thread panicked");
    }

    report(producers, counter.load(Ordering::Relaxed), elapsed);
}

fn report(producers: usize, total_ops: usize, elapsed: Duration) {
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    let ops_per_sec = if elapsed.as_secs_f64() > 0.0 {
        total_ops as f64 / elapsed.as_secs_f64()
    } else {
        f64::INFINITY
    };

    println!(
        "{producers},{total_ops},{elapsed_ms:.3},{ops_per_sec:.0}",
        producers = producers,
        total_ops = total_ops,
        elapsed_ms = elapsed_ms,
        ops_per_sec = ops_per_sec,
    );
}
