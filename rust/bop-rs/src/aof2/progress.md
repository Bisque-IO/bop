# AOF2 Progress

## Runtime (`rust/bop-rs/src/aof2/mod.rs`)
The append fast path still relies solely on atomics and now tracks a global unflushed-bytes gauge. Watermark and interval thresholds from `FlushConfig` feed the background `FlushManager`, synchronous fallbacks clear backlog when the queue saturates, and `Aof::flush_until` resolves exact record boundaries so callers wait only for the bytes they need. Recovery seeds the flush queue on startup so pending durability work resumes immediately.

## Segments (`rust/bop-rs/src/aof2/segment.rs`)
`SegmentAppendResult` exposes the new logical size, `Segment::mark_durable` returns the number of bytes that actually became durable, and each segment owns a `SegmentFlushState` that stays in sync during append, seal, and recovery flows. `Segment::record_end_offset` lets higher layers translate `RecordId` into concrete byte spans. Active data paths now call `flush_and_sync`, which commits mmap writes and falls back from `fdatasync` to `fsync` when needed.

## Flush Pipeline (`rust/bop-rs/src/aof2/flush.rs`)
The worker drains a lock-free command queue, guarantees one in-flight flush per segment, and keeps `FlushMetrics` counters for watermark/interval/backpressure scheduling as well as async versus sync completions. Durable flushes now wrap disk syncs with bounded exponential backoff and jitter, retry counters are captured via `retry_attempts`/`retry_failures`, and synchronous fallbacks reuse the same retry path so callers observe identical durability guarantees. `FlushMetricsSnapshot` also surfaces `backlog_bytes`, so `Aof::flush_metrics()` reflects the live unflushed gauge after every append and recovery pass. A provisional `FlushMetricsExporter` converts snapshots into named samples so telemetry consumers can scrape retry/backlog numbers without touching internal structures.

## Recovery and Tests
`recover_existing_segments` walks `segments/`, verifies filenames against headers, rebuilds the catalog, and seeds both the tail snapshot and the flush queue while priming the backlog gauge. New tests cover queue seeding on restart, validate the `backlog_bytes` snapshot, ensure `flush_until` blocks until the requested bytes are durable—even when the async flush queue is offline and the synchronous fallback path has to take over—and now exercise the recovery path so a reopened node can drain the backlog without a running flush worker.

## Configuration and Docs
`AofConfig::normalized` still clamps sizes to powers of two. Milestone 3 items F1–F4 are now complete, `FlushMetricsSnapshot` includes retry counters and the new backlog gauge for future observability work, and documentation tracks the flush/recovery progress so the next milestones can focus on visibility and crash recovery coverage.

## Reader (`rust/bop-rs/src/aof2/reader.rs`)
`SegmentReader` now verifies record checksums, reuses `Segment::read_record_slice` to borrow payloads zero-copy from sealed segments, and exposes `SegmentRecord` helpers that return offsets, payload slices, and timestamps together. The existing `RecordBounds` helper continues to surface header-plus-payload spans so follow-on reader work can compose without re-parsing headers. Fresh unit tests cover happy-path reads and checksum mismatches to keep the borrow guarantees honest, `Aof::open_reader` ties the tail notifications into the reader path so sealed segments are discoverable without racing the writer, and a lightweight `TailFollower` helper wraps `watch::Receiver<TailState>` so async consumers can react to tail lifecycle events without holding the catalog lock.

## Next Steps
- Carry `RD2a` forward by streaming sealed-segment handles through the new `TailFollower`.
- Extend `QA4` with start/stop fixtures once the cross-language harness unblocks.
- Kick off `OB1` tracing/metrics wiring so the exporter feeds real telemetry sinks.
