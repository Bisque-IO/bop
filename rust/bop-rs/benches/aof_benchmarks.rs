#![cfg(not(feature = "tiered-store"))]

use std::path::Path;

use bop_rs::aof2::{Aof, AofConfig, AofManager, AofManagerConfig, FlushConfig};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use tempfile::{Builder as TempBuilder, TempDir};
use tokio::task::yield_now;

const BATCH: usize = 64;
const PAYLOAD_SIZES: [usize; 3] = [128, 1024, 4096];

fn bench_config(root: &Path) -> AofConfig {
    let mut cfg = AofConfig::default();
    cfg.root_dir = root.to_path_buf();
    cfg.segment_min_bytes = 4 * 1024 * 1024;
    cfg.segment_max_bytes = 4 * 1024 * 1024;
    cfg.segment_target_bytes = 4 * 1024 * 1024;
    cfg.flush = FlushConfig {
        flush_watermark_bytes: 1 * 1024 * 1024,
        flush_interval_ms: 5,
        max_unflushed_bytes: 16 * 1024 * 1024,
    };
    cfg
}

fn bench_append_flush(c: &mut Criterion) {
    let mut group = c.benchmark_group("aof2_append_flush");

    for &payload_size in &PAYLOAD_SIZES {
        group.throughput(Throughput::Bytes((payload_size * BATCH) as u64));

        group.bench_with_input(
            BenchmarkId::new("append_background", payload_size),
            &payload_size,
            |b, &size| {
                let manager =
                    AofManager::with_config(AofManagerConfig::default()).expect("create manager");
                let handle = manager.handle();
                let root = TempDir::new().expect("root tempdir");
                let payload = vec![0u8; size];

                b.iter(|| {
                    let iteration = TempBuilder::new()
                        .prefix("aof2_background")
                        .tempdir_in(root.path())
                        .expect("iteration tempdir");
                    let cfg = bench_config(iteration.path());
                    let aof = Aof::new(handle.clone(), cfg).expect("aof");

                    for _ in 0..BATCH {
                        aof.append_record(&payload).expect("append");
                    }

                    handle.runtime_handle().block_on(yield_now());
                });
                drop(manager);
            },
        );

        group.bench_with_input(
            BenchmarkId::new("append_flush_until", payload_size),
            &payload_size,
            |b, &size| {
                let manager =
                    AofManager::with_config(AofManagerConfig::default()).expect("create manager");
                let handle = manager.handle();
                let root = TempDir::new().expect("root tempdir");
                let payload = vec![0u8; size];

                b.iter(|| {
                    let iteration = TempBuilder::new()
                        .prefix("aof2_sync")
                        .tempdir_in(root.path())
                        .expect("iteration tempdir");
                    let mut cfg = bench_config(iteration.path());
                    cfg.flush.flush_watermark_bytes = u64::MAX;
                    cfg.flush.flush_interval_ms = u64::MAX;
                    cfg.flush.max_unflushed_bytes = u64::MAX;
                    let aof = Aof::new(handle.clone(), cfg).expect("aof");

                    for _ in 0..BATCH {
                        let id = aof.append_record(&payload).expect("append");
                        aof.flush_until(id).expect("flush_until");
                    }
                });
                drop(manager);
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_append_flush);
criterion_main!(benches);
