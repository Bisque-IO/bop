use maniac_executor::runtime::waker::WorkerWaker;
use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::sync::atomic::{AtomicU64, Ordering};

fn bench_mark_active_paths(c: &mut Criterion) {
    let mut group = c.benchmark_group("signal_waker_mark_active");
    group.bench_function("cold_path", |b| {
        let waker = WorkerWaker::new();
        let bit = 0u64;
        b.iter(|| {
            waker.try_unmark(bit);
            waker.mark_active(bit);
            black_box(waker.try_acquire());
        });
    });

    group.bench_function("hot_path", |b| {
        let waker = WorkerWaker::new();
        let bit = 0u64;
        waker.mark_active(bit);
        let _ = waker.try_acquire();
        b.iter(|| {
            black_box(waker.mark_active(bit));
        });
    });

    group.finish();
}

fn bench_sync_partition_summary(c: &mut Criterion) {
    let mut group = c.benchmark_group("signal_waker_sync_partition_summary");
    for &len in &[4usize, 16, 64] {
        group.bench_with_input(
            BenchmarkId::from_parameter(len),
            &len,
            |b, &partition_len| {
                let waker = WorkerWaker::new();
                let leaves: Vec<AtomicU64> = (0..partition_len)
                    .map(|idx| AtomicU64::new(if idx % 2 == 0 { !0u64 } else { 0 }))
                    .collect();

                waker.sync_partition_summary(0, partition_len, &leaves);

                b.iter(|| {
                    for (idx, leaf) in leaves.iter().enumerate() {
                        if idx % 3 == 0 {
                            leaf.store(!leaf.load(Ordering::Relaxed), Ordering::Relaxed);
                        }
                    }
                    black_box(waker.sync_partition_summary(0, partition_len, &leaves));
                });
            },
        );
    }
    group.finish();
}

fn bench_worker_mixed_loop(c: &mut Criterion) {
    const SIGNAL_WORDS: usize = 32;
    const LEAF_WORDS: usize = 16;
    const ITERATIONS: usize = 128;

    c.bench_function("worker_simulated_mixed_loop", |b| {
        b.iter_batched(
            || {
                let waker = WorkerWaker::new();
                let signals: Vec<AtomicU64> =
                    (0..SIGNAL_WORDS).map(|_| AtomicU64::new(0)).collect();
                let leaves: Vec<AtomicU64> = (0..LEAF_WORDS).map(|_| AtomicU64::new(0)).collect();

                for idx in (0..SIGNAL_WORDS).step_by(3) {
                    signals[idx].store(1, Ordering::Relaxed);
                    waker.mark_active(idx as u64);
                    let _ = waker.try_acquire();
                }

                for idx in (0..LEAF_WORDS).step_by(2) {
                    leaves[idx].store(1, Ordering::Relaxed);
                }

                (waker, signals, leaves)
            },
            |(waker, signals, leaves)| {
                for iteration in 0..ITERATIONS {
                    let produce_idx = iteration % SIGNAL_WORDS;
                    if iteration % 7 == 0 {
                        signals[produce_idx].store(1, Ordering::Relaxed);
                        waker.mark_active(produce_idx as u64);
                    }

                    if iteration % 11 == 0 {
                        let leaf_idx = iteration % LEAF_WORDS;
                        leaves[leaf_idx].store(iteration as u64, Ordering::Relaxed);
                    }

                    if waker.try_acquire() {
                        let summary = waker.snapshot_summary();
                        if summary != 0 {
                            let bit = summary.trailing_zeros() as usize;
                            signals[bit].store(0, Ordering::Relaxed);
                            waker.try_unmark(bit as u64);
                        } else {
                            waker.try_unmark_tasks();
                        }
                    } else {
                        let _ = waker.sync_partition_summary(0, LEAF_WORDS, &leaves);
                    }
                }
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    signal_waker_perf,
    bench_mark_active_paths,
    bench_sync_partition_summary,
    bench_worker_mixed_loop
);
criterion_main!(signal_waker_perf);
