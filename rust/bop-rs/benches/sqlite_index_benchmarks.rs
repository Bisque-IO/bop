//! Benchmark comparisons between SqliteSegmentIndex and MdbxSegmentIndex

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;
use tempfile::tempdir;

use bop_rs::aof::{
    MdbxSegmentIndex, SegmentEntry, SegmentMetadata, SegmentStatus, SqliteSegmentIndex,
    current_timestamp,
};
use rand;

fn create_test_entry(base_id: u64, last_id: u64, created_at: u64) -> SegmentEntry {
    let metadata = SegmentMetadata {
        base_id,
        last_id,
        record_count: last_id - base_id + 1,
        created_at,
        size: 1024,
        checksum: 0x12345678,
        compressed: false,
        encrypted: false,
    };

    SegmentEntry::new_active(&metadata, format!("segment_{}.log", base_id), 0x12345678)
}

fn benchmark_bulk_inserts(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_inserts");
    group.measurement_time(Duration::from_secs(30));

    for segment_count in [1000, 5000, 10000].iter() {
        let entries: Vec<_> = (0..*segment_count)
            .map(|i| {
                let base_id = i * 100;
                let last_id = base_id + 99;
                create_test_entry(base_id, last_id, current_timestamp())
            })
            .collect();

        // SQLite benchmark
        group.bench_with_input(
            BenchmarkId::new("sqlite", segment_count),
            segment_count,
            |b, _| {
                b.iter_batched(
                    || {
                        let temp_dir = tempdir().unwrap();
                        let db_path = temp_dir.path().join("bench.db");
                        SqliteSegmentIndex::open(&db_path).unwrap()
                    },
                    |mut index| {
                        for entry in &entries {
                            black_box(index.add_entry(entry.clone()).unwrap());
                        }
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );

        // MDBX benchmark
        group.bench_with_input(
            BenchmarkId::new("mdbx", segment_count),
            segment_count,
            |b, _| {
                b.iter_batched(
                    || {
                        let temp_dir = tempdir().unwrap();
                        let db_path = temp_dir.path().join("bench.mdbx");
                        MdbxSegmentIndex::open(&db_path).unwrap()
                    },
                    |mut index| {
                        for entry in &entries {
                            black_box(index.add_entry(entry.clone()).unwrap());
                        }
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

fn benchmark_random_lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_lookups");
    group.measurement_time(Duration::from_secs(20));

    let segment_count = 10000;
    let lookup_count = 1000;

    // Prepare test data
    let entries: Vec<_> = (0..segment_count)
        .map(|i| {
            let base_id = i * 100;
            let last_id = base_id + 99;
            create_test_entry(base_id, last_id, current_timestamp())
        })
        .collect();

    // Setup SQLite index
    let temp_dir = tempdir().unwrap();
    let sqlite_path = temp_dir.path().join("lookup_bench.db");
    let mut sqlite_index = SqliteSegmentIndex::open(&sqlite_path).unwrap();
    for entry in &entries {
        sqlite_index.add_entry(entry.clone()).unwrap();
    }
    sqlite_index.sync().unwrap();

    // Setup MDBX index
    let mdbx_path = temp_dir.path().join("lookup_bench.mdbx");
    let mut mdbx_index = MdbxSegmentIndex::open(&mdbx_path).unwrap();
    for entry in &entries {
        mdbx_index.add_entry(entry.clone()).unwrap();
    }
    mdbx_index.sync().unwrap();

    // Generate random record IDs for lookup
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let lookup_ids: Vec<u64> = (0..lookup_count)
        .map(|_| {
            let segment_idx = rng.gen_range(0..segment_count);
            let base_id = segment_idx * 100;
            base_id + 50 // Middle of segment
        })
        .collect();

    // SQLite lookup benchmark
    group.bench_function("sqlite_lookups", |b| {
        b.iter(|| {
            for &record_id in &lookup_ids {
                black_box(sqlite_index.find_segment_for_id(record_id).unwrap());
            }
        });
    });

    // MDBX lookup benchmark
    group.bench_function("mdbx_lookups", |b| {
        b.iter(|| {
            for &record_id in &lookup_ids {
                black_box(mdbx_index.find_segment_for_id(record_id).unwrap());
            }
        });
    });

    group.finish();
}

fn benchmark_sequential_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_operations");
    group.measurement_time(Duration::from_secs(15));

    let operation_count = 5000;

    // SQLite sequential operations
    group.bench_function("sqlite_sequential", |b| {
        b.iter_batched(
            || {
                let temp_dir = tempdir().unwrap();
                let db_path = temp_dir.path().join("seq_bench.db");
                SqliteSegmentIndex::open(&db_path).unwrap()
            },
            |mut index| {
                // Mix of operations: 70% inserts, 20% lookups, 10% updates
                for i in 0..operation_count {
                    match i % 10 {
                        0..=6 => {
                            // Insert
                            let base_id = i * 100;
                            let last_id = base_id + 99;
                            let entry = create_test_entry(base_id, last_id, current_timestamp());
                            black_box(index.add_entry(entry).unwrap());
                        }
                        7..=8 => {
                            // Lookup (if we have data)
                            if i > 0 {
                                let segment_idx = i / 2;
                                let base_id = segment_idx * 100;
                                let record_id = base_id + 50;
                                black_box(index.find_segment_for_id(record_id));
                            }
                        }
                        9 => {
                            // Update (if we have data)
                            if i > 0 {
                                let base_id = (i / 2) * 100;
                                black_box(index.update_segment_tail(base_id, base_id + 150, 2048));
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // MDBX sequential operations
    group.bench_function("mdbx_sequential", |b| {
        b.iter_batched(
            || {
                let temp_dir = tempdir().unwrap();
                let db_path = temp_dir.path().join("seq_bench.mdbx");
                MdbxSegmentIndex::open(&db_path).unwrap()
            },
            |mut index| {
                // Same operation mix as SQLite
                for i in 0..operation_count {
                    match i % 10 {
                        0..=6 => {
                            // Insert
                            let base_id = i * 100;
                            let last_id = base_id + 99;
                            let entry = create_test_entry(base_id, last_id, current_timestamp());
                            black_box(index.add_entry(entry).unwrap());
                        }
                        7..=8 => {
                            // Lookup (if we have data)
                            if i > 0 {
                                let segment_idx = i / 2;
                                let base_id = segment_idx * 100;
                                let record_id = base_id + 50;
                                black_box(index.find_segment_for_id(record_id));
                            }
                        }
                        9 => {
                            // Update (if we have data)
                            if i > 0 {
                                let base_id = (i / 2) * 100;
                                black_box(index.update_segment_tail(base_id, base_id + 150, 2048));
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn benchmark_database_size_vs_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("size_vs_performance");
    group.measurement_time(Duration::from_secs(25));

    // Test performance degradation as database size increases
    for &existing_segments in [0, 10000, 50000, 100000].iter() {
        let test_operations = 1000;

        // SQLite with varying database sizes
        if existing_segments > 0 {
            group.bench_with_input(
                BenchmarkId::new("sqlite_with_existing", existing_segments),
                &existing_segments,
                |b, &segment_count| {
                    b.iter_batched(
                        || {
                            let temp_dir = tempdir().unwrap();
                            let db_path = temp_dir.path().join("size_bench.db");
                            let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

                            // Pre-populate with existing segments
                            for i in 0..segment_count {
                                let base_id = i * 100;
                                let last_id = base_id + 99;
                                let entry =
                                    create_test_entry(base_id, last_id, current_timestamp());
                                index.add_entry(entry).unwrap();
                            }
                            index.sync().unwrap();
                            index
                        },
                        |mut index| {
                            // Perform test operations on populated database
                            for i in 0..test_operations {
                                let base_id = (existing_segments + i) * 100;
                                let last_id = base_id + 99;
                                let entry =
                                    create_test_entry(base_id, last_id, current_timestamp());
                                black_box(index.add_entry(entry).unwrap());

                                // Every 10th operation, do a lookup
                                if i % 10 == 0 && existing_segments > 0 {
                                    let lookup_segment = existing_segments / 2;
                                    let record_id = lookup_segment * 100 + 50;
                                    black_box(index.find_segment_for_id(record_id));
                                }
                            }
                        },
                        criterion::BatchSize::SmallInput,
                    );
                },
            );
        }

        // MDBX with varying database sizes
        if existing_segments > 0 {
            group.bench_with_input(
                BenchmarkId::new("mdbx_with_existing", existing_segments),
                &existing_segments,
                |b, &segment_count| {
                    b.iter_batched(
                        || {
                            let temp_dir = tempdir().unwrap();
                            let db_path = temp_dir.path().join("size_bench.mdbx");
                            let mut index = MdbxSegmentIndex::open(&db_path).unwrap();

                            // Pre-populate with existing segments
                            for i in 0..segment_count {
                                let base_id = i * 100;
                                let last_id = base_id + 99;
                                let entry =
                                    create_test_entry(base_id, last_id, current_timestamp());
                                index.add_entry(entry).unwrap();
                            }
                            index.sync().unwrap();
                            index
                        },
                        |mut index| {
                            // Perform test operations on populated database
                            for i in 0..test_operations {
                                let base_id = (existing_segments + i) * 100;
                                let last_id = base_id + 99;
                                let entry =
                                    create_test_entry(base_id, last_id, current_timestamp());
                                black_box(index.add_entry(entry).unwrap());

                                // Every 10th operation, do a lookup
                                if i % 10 == 0 && existing_segments > 0 {
                                    let lookup_segment = existing_segments / 2;
                                    let record_id = lookup_segment * 100 + 50;
                                    black_box(index.find_segment_for_id(record_id));
                                }
                            }
                        },
                        criterion::BatchSize::SmallInput,
                    );
                },
            );
        }
    }

    group.finish();
}

fn benchmark_sync_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("sync_operations");
    group.measurement_time(Duration::from_secs(10));

    let batch_size = 1000;

    // SQLite sync benchmark
    group.bench_function("sqlite_sync", |b| {
        b.iter_batched(
            || {
                let temp_dir = tempdir().unwrap();
                let db_path = temp_dir.path().join("sync_bench.db");
                let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

                // Add some data before syncing
                for i in 0..batch_size {
                    let base_id = i * 100;
                    let last_id = base_id + 99;
                    let entry = create_test_entry(base_id, last_id, current_timestamp());
                    index.add_entry(entry).unwrap();
                }
                index
            },
            |mut index| {
                black_box(index.sync().unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // MDBX sync benchmark
    group.bench_function("mdbx_sync", |b| {
        b.iter_batched(
            || {
                let temp_dir = tempdir().unwrap();
                let db_path = temp_dir.path().join("sync_bench.mdbx");
                let mut index = MdbxSegmentIndex::open(&db_path).unwrap();

                // Add some data before syncing
                for i in 0..batch_size {
                    let base_id = i * 100;
                    let last_id = base_id + 99;
                    let entry = create_test_entry(base_id, last_id, current_timestamp());
                    index.add_entry(entry).unwrap();
                }
                index
            },
            |mut index| {
                black_box(index.sync().unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_bulk_inserts,
    benchmark_random_lookups,
    benchmark_sequential_operations,
    benchmark_database_size_vs_performance,
    benchmark_sync_operations
);

criterion_main!(benches);
