# AOF2: Append-Only File Storage Engine

AOF2 provides an append-only log with tiered caching, background flush, and sealed segment readers.

## Key Types

- `AofManager` / `AofManagerConfig`: bootstrap the background runtime and tiered store
- `Aof`: per-instance append interface bound to an on-disk layout
- `AofConfig`: configuration knobs (segment sizing, flush policy, retention, compression)
- `RecordId`, `SegmentId`: record and segment identifiers
- `SegmentReader`, `SegmentCursor`: read sealed segments
- `TailFollower`, `SealedSegmentStream`: follow tail lifecycle and consume records

## Quick Start

```rust
use bop_rs::aof2::{Aof, AofManager, AofManagerConfig};
use bop_rs::aof2::config::{AofConfig, SegmentId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = AofManager::with_config(AofManagerConfig::default())?;
    let handle = manager.handle();

    let mut cfg = AofConfig::default();
    cfg.root_dir = std::path::PathBuf::from("/tmp/aof2-demo");
    let aof = Aof::new(handle, cfg)?;

    let rid = aof.append_record(b"hello world")?;
    aof.flush_until(rid)?;

    let seg_id = SegmentId::new(rid.segment_index() as u64);
    if let Ok(reader) = aof.open_reader(seg_id) {
        let rec = reader.read_record(rid)?;
        assert_eq!(rec.payload(), b"hello world");
    }
    Ok(())
}
```

## Async Append

```rust
use bop_rs::aof2::{Aof, AofManager, AofManagerConfig};
use bop_rs::aof2::config::AofConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = AofManager::with_config(AofManagerConfig::default())?;
    let handle = manager.handle();
    let aof = Aof::new(handle, AofConfig::default())?;
    let _rid = aof.append_record_async(b"payload").await?;
    Ok(())
}
```

## Configuring Segment Sizes and Flush

```rust
use bop_rs::aof2::config::{AofConfig, FlushConfig, Compression, RetentionPolicy};

let mut cfg = AofConfig::default();
cfg.flush = FlushConfig { flush_watermark_bytes: 8 * 1024 * 1024, flush_interval_ms: 5, max_unflushed_bytes: 64 * 1024 * 1024 };
cfg.segment_min_bytes = 2 * 1024 * 1024;
cfg.segment_max_bytes = 128 * 1024 * 1024;
cfg.segment_target_bytes = 16 * 1024 * 1024;
cfg.compression = Compression::Zstd;
cfg.retention = RetentionPolicy::KeepAll;
let cfg = cfg.normalized();
```

## Reading Sealed Segments

```rust
use bop_rs::aof2::reader::SegmentCursor;
use bop_rs::aof2::config::SegmentId;

fn dump_segment(aof: &bop_rs::aof2::Aof, id: SegmentId) -> bop_rs::aof2::error::AofResult<()> {
    let reader = aof.open_reader(id)?;
    let mut cursor = SegmentCursor::new(reader);
    while let Some(rec) = cursor.next()? {
        println!("{} bytes", rec.payload().len());
    }
    Ok(())
}
```

## Stream Sealed Then Active Tail

```rust
use bop_rs::aof2::reader::{SealedSegmentStream, StreamSegment};

async fn consume(aof: &bop_rs::aof2::Aof) -> bop_rs::aof2::error::AofResult<()> {
    let mut stream = SealedSegmentStream::new(aof)?;
    while let Some(seg) = stream.next_segment().await? {
        match seg {
            StreamSegment::Sealed(mut cursor) => {
                while let Some(rec) = cursor.next()? {
                    // handle sealed record
                }
            }
            StreamSegment::Active(mut cursor) => {
                while let Some(rec) = cursor.next()? {
                    // handle active record
                }
            }
        }
    }
    Ok(())
}
```

## Metrics

```rust
use bop_rs::aof2::{FlushMetricsExporter, Aof};

fn export(aof: &Aof) {
    let snapshot = aof.flush_metrics();
    let exporter = FlushMetricsExporter::new(snapshot);
    exporter.emit(|sample| println!("{} {}", sample.name, sample.value));
}
```

