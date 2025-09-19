# AOF Codex Design Notes

## Overview
- **Module**: `rust/bop-rs/src/aof`
- **Purpose**: Append-only log with asynchronous writes, background maintenance, reader support, archiving, and persistent indexing.
- **Key Components**:
  - `Aof<FS, A>` orchestrates write path, segment lifecycle, readers, metrics.
  - `ActiveSegment` owns mutable mmap + shadow reader segment.
  - `ReaderAwareSegmentCache` keeps `Arc<Segment<FS>>` handles with reader pin counts.
  - `MdbxSegmentIndex` persists segment metadata; supports lookups by ID and time.
  - Background tasks: pre-allocation, finalization, flush, archive, reader notification.

## Current Implementation Summary
- **Segment Handling**
  - Active segment recovery reconstructs state from MDBX entry (`recover_active_segment`).
  - Shadow segments are wrapped in `Arc` and cached when created/updated.
  - `wait_for_active_segment` ensures mmap init, refreshes cache, and keeps pre-allocation flowing.
- **Pre-allocation Pipeline**
  - `pending_preallocation` stores `oneshot::Receiver` results until consumed.
  - `prepare_active_segment_for_append` blocks synchronously to rotate/create segments when needed.
- **Flush & Durability**
  - Flush path flushes mmap, file, and sync; records metrics and triggers incremental archiving.
- **Reader Path**
  - Reader constructors position via `SegmentEntry` lookups (`find_segment_for_id`, `find_segments_for_timestamp`, `get_latest_finalized_segment`).
  - `position_reader_in_segment` acquires segments through cache, scans for offsets, and seeds `SegmentPosition`.
  - Tail readers track IDs in `tail_readers` set; appended records notify via background channel.
  - `Reader::seek_to_record` scans current segment for a target ID.
- **Caching**
  - Cache stores `Arc<Segment<FS>>`; `insert_segment` invoked after shadow updates and recoveries.
- **Error Handling**
  - Background tasks write string errors into shared state; append path surfaces them.

## Completed Stages / Tasks
1. **Stage 1** (compilation fixes)
   - Removed non-`Send` reader tracking from `Segment`.
   - Updated binary index lookup.
2. **Stage 2** (write-path readiness)
   - Implemented pre-allocation result handling, automatic rotation, mmap flush/sync.
   - Surfaced background task errors.
3. **Stage 3** (read-path + recovery)
   - Reader construction via index + cache.
   - Tail reader tracking for notifications.
   - Active segment recovery and cache seeding.
   - Reader segment cache rewritten to use `Arc` handles.
   - Added `Reader::seek_to_record` helper.

4. **Stage 4** (reader lifecycle hygiene)
   - Added reader lifecycle guard wiring to drop tail IDs and release cache pins automatically.
   - Introduced reader registry + per-reader `Notify` handles so background tasks wake tail readers.
   - Added tail streaming helpers (`wait_for_tail_notification`, `tail_next_record`) so readers can await new data easily.
   - Flush windows now track background completion/backoff and requeue pending metadata on failure.
   - Finalization persists CRC64 checksums; archives optionally compress via zstd and record CRC64/size metadata.



## Remaining Work / Next Steps
- **Reader/Tail Management**
  - Extend tail helpers to handle segment rollover and reader lifecycle metrics.
- **Write/Background Enhancements**
  - Surface background flush completion metrics/backoff and reconcile with `FlushController` state.
  - Ensure `trigger_background_flush` aligns with flush strategy (future async flush worker).
  - Compute and persist record checksums and compressed metadata for finalization/archive.

- **Recovery & Indexing**
  - Continue active segment recovery to detect partially written tail records (scan mmap to find durable last record).
  - Update timestamp index when adding/updating segment entries to avoid drift.
- **Testing & Tooling**
  - Add integration tests covering reader positioning, recovery, cache reuse, tail notifications.
  - Stress tests for rotation + archiving.
  - Benchmarks for append/read throughput.
- **Notification Backlog**
  - Implement reader subscription/unsubscription APIs.
  - Provide explicit wake-up mechanism (e.g., per-reader channel or Notify).
- **Documentation & API**
  - Keep updating this codex as stages progress.
  - Document config knobs (segment size, cache size, flush strategy, archive settings).

## Open Questions / Decisions Needed
- Exact contract for reader notifications (push vs poll).
- Whether to provide synchronous seek/read API beyond tailing readers.
- Strategy for byte-level backpressure before archiving (config exist but not implemented).

## File & Command References
- Primary implementation: `rust/bop-rs/src/aof/aof.rs`.
- Reader utilities: `rust/bop-rs/src/aof/reader.rs`.
- Segment index: `rust/bop-rs/src/aof/segment_index.rs`.
- Cache logic in `ReaderAwareSegmentCache` (same file as `Aof`).

## How to Resume Work
1. Review this document for current context.
2. Prioritize remaining tasks above (e.g., notification wiring and recovery scanning).
3. Continue using `cargo check` for quick validation (`cargo test` when tests are added).
4. Update `aof_codex.md` after notable milestones.

## Immediate Implementation Plan
1. (DONE) Implement tail reader unregistration (clear from tail_readers and segment cache) when readers drop.
2. (DONE) Wire reader_notification_task to actual reader waiters (store per-reader Arc<Notify>).
3. (DONE) Feed real segment info to background flush task and align with flush strategy.
4. (DONE) Add checksum calculations for segment finalization/archive metadata.
5. (DONE) Extend recovery to scan active segment for last durable record, updating index as needed.
6. (DONE) Introduce integration tests covering reader positioning, recovery, and tail notifications.

