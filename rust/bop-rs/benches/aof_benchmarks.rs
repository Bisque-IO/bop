use bop_rs::aof_v2::{Aof, AofConfig, FlushStrategy};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::time::Duration;
use tempfile::tempdir;

fn benchmark_append_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("aof_append_throughput");

    for &size in [64, 256, 1024, 4096, 16384].iter() {
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::new("sync_flush", size), &size, |b, &size| {
            let dir = tempdir().unwrap();
            let config = AofConfig {
                flush_strategy: FlushStrategy::Sync,
                ..Default::default()
            };
            let mut aof = Aof::open_with_config(dir.path(), config).unwrap();
            let data = vec![0u8; size];

            b.iter(|| {
                black_box(aof.append(&data).unwrap());
            });
        });

        group.bench_with_input(
            BenchmarkId::new("batched_flush", size),
            &size,
            |b, &size| {
                let dir = tempdir().unwrap();
                let config = AofConfig {
                    flush_strategy: FlushStrategy::Batched(100),
                    ..Default::default()
                };
                let mut aof = Aof::open_with_config(dir.path(), config).unwrap();
                let data = vec![0u8; size];

                b.iter(|| {
                    black_box(aof.append(&data).unwrap());
                });
            },
        );

        group.bench_with_input(BenchmarkId::new("async_flush", size), &size, |b, &size| {
            let dir = tempdir().unwrap();
            let config = AofConfig {
                flush_strategy: FlushStrategy::Async,
                ..Default::default()
            };
            let mut aof = Aof::open_with_config(dir.path(), config).unwrap();
            let data = vec![0u8; size];

            b.iter(|| {
                black_box(aof.append(&data).unwrap());
            });
        });
    }

    group.finish();
}

fn benchmark_read_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("aof_read_performance");

    // Prepare data
    let dir = tempdir().unwrap();
    let mut aof = Aof::open(dir.path()).unwrap();
    let record_count = 10000;
    let data = vec![0u8; 1024];
    let mut record_ids = Vec::new();

    for _ in 0..record_count {
        let id = aof.append(&data).unwrap();
        record_ids.push(id);
    }
    aof.flush().unwrap();

    group.bench_function("sequential_reads", |b| {
        let mut index = 0;
        b.iter(|| {
            let id = record_ids[index % record_ids.len()];
            index += 1;
            black_box(aof.read(id).unwrap());
        });
    });

    group.bench_function("random_reads", |b| {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        b.iter(|| {
            let id = record_ids[rng.gen_range(0..record_ids.len())];
            black_box(aof.read(id).unwrap());
        });
    });

    group.finish();
}

fn benchmark_reader_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("aof_reader_iteration");

    // Prepare data
    let dir = tempdir().unwrap();
    let mut aof = Aof::open(dir.path()).unwrap();
    let record_count = 1000;
    let data = vec![0u8; 256];

    for _ in 0..record_count {
        aof.append(&data).unwrap();
    }
    aof.flush().unwrap();

    group.bench_function("reader_read_records", |b| {
        let reader = aof.new_reader();
        b.iter(|| {
            let records = reader.read_new_records(&mut aof);
            black_box(records.len());
            reader.seek(0); // Reset for next iteration
        });
    });

    group.finish();
}

fn benchmark_concurrent_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("aof_concurrent_operations");

    group.bench_function("single_writer_multiple_readers", |b| {
        use std::sync::{Arc, Mutex};
        use std::thread;

        b.iter(|| {
            let dir = tempdir().unwrap();
            let aof = Arc::new(Mutex::new(Aof::open(dir.path()).unwrap()));
            let data = vec![0u8; 512];
            let num_readers = 4;
            let records_per_reader = 100;

            // Create readers
            let readers: Vec<_> = (0..num_readers)
                .map(|_| {
                    let aof_guard = aof.lock().unwrap();
                    aof_guard.new_reader()
                })
                .collect();

            // Spawn writer thread
            let aof_writer = aof.clone();
            let writer_handle = thread::spawn(move || {
                for _ in 0..(num_readers * records_per_reader) {
                    let mut aof_guard = aof_writer.lock().unwrap();
                    aof_guard.append(&data).unwrap();
                }
            });

            // Spawn reader threads
            let reader_handles: Vec<_> = readers
                .into_iter()
                .map(|reader| {
                    let aof_reader = aof.clone();
                    thread::spawn(move || {
                        let mut count = 0;
                        while count < records_per_reader {
                            let mut aof_guard = aof_reader.lock().unwrap();
                            let new_records = reader.read_new_records(&mut *aof_guard).len();
                            count += new_records;
                            drop(aof_guard);
                            if new_records == 0 {
                                thread::sleep(Duration::from_millis(1));
                            }
                        }
                    })
                })
                .collect();

            // Wait for completion
            writer_handle.join().unwrap();
            for handle in reader_handles {
                handle.join().unwrap();
            }
        });
    });

    group.finish();
}

fn benchmark_segment_management(c: &mut Criterion) {
    let mut group = c.benchmark_group("aof_segment_management");

    group.bench_function("segment_rollover", |b| {
        b.iter(|| {
            let dir = tempdir().unwrap();
            let config = AofConfig {
                segment_size: 64 * 1024, // Small segments for frequent rollover
                ..Default::default()
            };
            let mut aof = Aof::open_with_config(dir.path(), config).unwrap();
            let data = vec![0u8; 1024];

            // Force multiple segment rollovers
            for _ in 0..100 {
                aof.append(&data).unwrap();
            }

            black_box(
                aof.metrics()
                    .total_segments
                    .load(std::sync::atomic::Ordering::Relaxed),
            );
        });
    });

    group.bench_function("lru_cache_performance", |b| {
        let dir = tempdir().unwrap();
        let config = AofConfig {
            segment_size: 64 * 1024,
            segment_cache_size: 10, // Small cache to trigger evictions
            ..Default::default()
        };
        let mut aof = Aof::open_with_config(dir.path(), config).unwrap();
        let data = vec![0u8; 1024];
        let mut record_ids = Vec::new();

        // Create many segments
        for _ in 0..1000 {
            let id = aof.append(&data).unwrap();
            record_ids.push(id);
        }
        aof.flush().unwrap();

        b.iter(|| {
            // Access records across different segments to test cache
            use rand::Rng;
            let mut rng = rand::thread_rng();
            for _ in 0..50 {
                let id = record_ids[rng.gen_range(0..record_ids.len())];
                black_box(aof.read(id).unwrap());
            }
        });
    });

    group.finish();
}

fn benchmark_flush_strategies(c: &mut Criterion) {
    let mut group = c.benchmark_group("aof_flush_strategies");

    let strategies = vec![
        ("sync", FlushStrategy::Sync),
        ("async", FlushStrategy::Async),
        ("batched_10", FlushStrategy::Batched(10)),
        ("batched_100", FlushStrategy::Batched(100)),
        ("periodic_10ms", FlushStrategy::Periodic(10)),
        ("periodic_100ms", FlushStrategy::Periodic(100)),
        (
            "batched_or_periodic",
            FlushStrategy::BatchedOrPeriodic {
                batch_size: 50,
                interval_ms: 50,
            },
        ),
    ];

    for (name, strategy) in strategies {
        group.bench_function(name, |b| {
            let dir = tempdir().unwrap();
            let config = AofConfig {
                flush_strategy: strategy,
                ..Default::default()
            };
            let mut aof = Aof::open_with_config(dir.path(), config).unwrap();
            let data = vec![0u8; 512];

            b.iter(|| {
                for _ in 0..100 {
                    aof.append(&data).unwrap();
                }
                aof.flush().unwrap(); // Ensure all data is flushed for measurement
            });
        });
    }

    group.finish();
}

fn benchmark_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("aof_memory_usage");

    group.bench_function("segment_loading_unloading", |b| {
        let dir = tempdir().unwrap();
        let config = AofConfig {
            segment_size: 1024 * 1024, // 1MB segments
            segment_cache_size: 5,
            ..Default::default()
        };
        let mut aof = Aof::open_with_config(dir.path(), config).unwrap();
        let data = vec![0u8; 4096];
        let mut record_ids = Vec::new();

        // Create multiple segments
        for _ in 0..2000 {
            let id = aof.append(&data).unwrap();
            record_ids.push(id);
        }
        aof.flush().unwrap();

        b.iter(|| {
            // Random access to trigger segment loading/unloading
            use rand::Rng;
            let mut rng = rand::thread_rng();
            for _ in 0..100 {
                let id = record_ids[rng.gen_range(0..record_ids.len())];
                black_box(aof.read(id).unwrap());
            }
        });
    });

    group.finish();
}

fn benchmark_corruption_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("aof_corruption_detection");

    group.bench_function("checksum_verification", |b| {
        let dir = tempdir().unwrap();
        let mut aof = Aof::open(dir.path()).unwrap();
        let data = vec![0u8; 1024];
        let mut record_ids = Vec::new();

        for _ in 0..1000 {
            let id = aof.append(&data).unwrap();
            record_ids.push(id);
        }
        aof.flush().unwrap();

        b.iter(|| {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let id = record_ids[rng.gen_range(0..record_ids.len())];
            // The checksum verification happens automatically during read
            black_box(aof.read(id).unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_append_throughput,
    benchmark_read_performance,
    benchmark_reader_iteration,
    benchmark_concurrent_operations,
    benchmark_segment_management,
    benchmark_flush_strategies,
    benchmark_memory_usage,
    benchmark_corruption_detection
);

criterion_main!(benches);
