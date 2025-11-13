# bop-storage Write/Flush Controller Design

## Context and Goals
- Integrate the new WAL segment primitives (`WalSegment`, counters, queue guards, metrics) with higher-level controllers that manage I/O.
- Preserve strict invariants around `preallocated`, `pending`, `written`, and `durable` watermarks so recovery never has to scan for the last valid record.
- Provide backpressure and queue ownership semantics so each segment appears at most once in the write and flush work queues.
- Allow flexible buffering (owned vectors, borrowed slices, externally managed memory) and reuse of Tokio runtimes across controllers or databases.

## Runtime Ownership
- Controllers receive an `Arc<Runtime>` handle. This lets a single runtime be shared across write/flush controllers or instantiated per-database without changing APIs.
- Per-controller worker threads use the runtime handle’s `spawn_blocking` to run actual disk I/O (`write`/`writev`, `fsync`/`fdatasync`).
- The decision to run controllers on dedicated threads keeps queue plumbing simple while keeping blocking work off async scheduler threads.

## WalSegment State Model
- `preallocated_size`: monotonic upper bound on on-disk capacity. Only grows after allocate/extend succeeds. Matches manifest state on recovery.
- `written_size`: bytes successfully handed to the I/O driver. Updated inside the write controller critical section with the write queue guard held. Never regresses.
- `pending_size`: transient reservations granted to writers but not yet submitted. Satisfies `pending + written <= preallocated`. Cleared each time a batch leaves the write queue.
- `durable_size`: bytes known durable after `fsync` *and* manifest update. Always `<= written_size`. Advanced only when both steps succeed.
- Queue guards: two atomics (write/flush) ensure a segment can be present in each queue at most once and expose queue depth for diagnostics.

## Write Buffer Structure
- Each `WalSegment` owns two "slots" (left/right) representing the active and standby write buffers.
- Buffer entries are stored as an enum:
  ```rust
  enum WriteChunk {
      Owned(Vec<u8>),
      Borrowed(IoVec<'static>),
      Raw { ptr: *const u8, len: usize, drop: unsafe fn(*const u8, usize) },
  }
  ```
- A `WriteBatch` is `Vec<WriteChunk>`, allowing mixtures of vectors, aligned slices, or custom-managed memory without forcing copies.
- Producers append chunks to the current slot. When they reach a page/record boundary they atomically swap buffers and enqueue the segment for writes.

## Controller Queue Types
- Write controller consumes `WriteTask` items describing write-side work.
  ```rust
  enum WriteTask {
      Segment(Arc<WalSegment>),
      // future variants (shutdown, diagnostics, etc.)
  }
  ```
- Flush controller consumes `FlushTask` items describing flush-side work.
  ```rust
  enum FlushTask {
      Segment(Arc<WalSegment>),
      // future variants (seal, maintenance, etc.)
  }
  ```
- Crossfire bounded MPSC queues carry these items to dedicated worker threads; backpressure comes from channel capacity and segment-level checks.

## WriteController Workflow
1. Writers request space via `reserve_bytes` (increments `pending_size`).
2. Producers build a batch in the active buffer. Once ready, they swap buffers, capture the batch size, and enqueue `WriteTask::Segment(segment_arc)`.
3. Worker thread dequeues item, acquires write queue guard, and swaps the ready batch out, leaving an empty buffer for further producers.
4. Convert `WriteChunk` vector into I/O vectors and submit via `spawn_blocking` (`write_at`/`writev_at`).
5. On success: decrement `pending_size`, increment `written_size`, and update metrics (last batch bytes). On partial/error: roll back `pending_size`, keep the buffer active for retry, and record errors.
6. Release guard, allowing segment to be requeued when more data arrives.

## FlushController Workflow
1. When system identifies a flush boundary, it records `pending_flush_size` atomically on the segment (multiple of page/record size) and enqueues `FlushTask::Segment(segment_arc)`.
2. Worker thread dequeues, acquires flush guard, atomically swaps `pending_flush_size` back to zero for future flush requests, and kicks off `fsync` via `spawn_blocking`.
3. After `fsync` completes, invoke manifest adapter (`FlushSink`) to commit the new durable length. Both steps occur before advancing `durable_size`.
4. On success: call `advance_durable(new_len)` and update flush metrics (duration, queue depth). On failure: leave `durable_size` unchanged, requeue with backoff.

## Manifest Integration
- Flush controller depends on a `FlushSink` trait to report durability progress. Default implementation writes through the manifest WAL state (`WalStateRecord`) within a manifested transaction.
- Failures from either `fsync` or manifest update keep `durable_size` unchanged and trigger retries under controller policy.

## Backpressure & Flow Control
- Queue capacity and per-segment guard states prevent infinite buildup.
- Controller can optionally examine `pending_size` or batch bytes before allowing another enqueue, enabling byte-based throttling.
- Additional future work items (e.g., `FlushTask::FlushAll`) can inject control messages without redesigning queues.

## Diagnostics
- `WalSegmentMetrics` already tracks queue depth, last flush duration, and last write batch size. Controllers update these after each operation.
- `Wal` remains a thin facade: it exposes diagnostics by snapshotting segments (counters + metrics) without owning controller logic.

## Testing Strategy
- Unit tests for segment invariants (already in `wal.rs`).
- Controller tests simulate enqueue/dequeue flows using mock `IoFile` implementations to cover success, partial writes, `fsync` failures, and manifest errors.
- Integration harness builds segments, pushes batches, and ensures `preallocated`, `pending`, `written`, and `durable` counters stay consistent across write/flush cycles.

## Future Extensions
- Add new `WriteTask`/`FlushTask` variants for maintenance commands (e.g., seal segment, rotate buffers, shutdown).
- Implement smarter buffer pooling (e.g., per-size freelists) so write batches reuse allocations.
- Introduce telemetry hooks (e.g., Prometheus) fed by controller events and `WalSegmentMetrics` snapshots.
- Support multi-segment scheduling policies (prioritising oldest durable gaps, batching across segments).

This document captures the production-ready design for integrating write and flush controllers with `WalSegment` primitives while keeping recovery guarantees and providing flexibility for future evolution.