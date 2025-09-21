# Flush Lifecycle & Tier Metadata Integration

## Current Execution Path
1. `Aof::append_record` updates the active `Segment` and calls `SegmentFlushState::request_flush` with the new logical size. The state lives on each segment and tracks `requested_bytes`, `durable_bytes`, and the in-flight flag.
2. `Aof` pushes the owning `ResidentSegment` into `AofManagement::flush_queue` (see `rust/bop-rs/src/aof2/mod.rs:1230` and recovery seeding at `:1645`). The queue reflects scheduler ordering for asynchronous flush work.
3. `FlushManager::enqueue_segment` ( `rust/bop-rs/src/aof2/flush.rs:308` ) hands segments to a single worker thread. The worker respects `SegmentFlushState::try_begin_flush`/`finish_flush` to avoid concurrent disk commits for the same segment.
4. `FlushManager::flush_segment` verifies the flush is still needed, runs `flush_with_retry`, and calls `Segment::mark_durable` to advance durability markers inside the segment (`segment.rs:536`). Any delta is subtracted from `AppendState::total_unflushed` so backlog metrics stay accurate.
5. Successful flushes trigger `SegmentFlushState::mark_durable`, which wakes waiters (notably blocking reads) and records timestamps for observability.
6. When a segment seals, `Aof::queue_rollover` awaits the flush watermark before scheduling the coordinator to compress/upload the artifact. Recovery walks the same metadata in `Aof::recover_existing_segments` to reconstruct `flush_queue` ordering.

## Tier Metadata Touch Points
- **Requested vs durable bytes**: the tier needs both numbers to decide which segments can graduate to Tier1/Tier2 and to surface reader-facing watermarks.
- **Instance identifiers**: tier metadata is keyed by instance ID + segment ID. Flush events currently only know about the `Segment`; they must also carry `ResidentSegment` context so tier persistence can update the per-instance view.
- **Backpressure and error mapping**: repeated failures writing tier metadata should bubble up as `BackpressureKind::Flush` to match admission/rollover handling.
- **Recovery fidelity**: restart must hydrate tier metadata before readers resume. Today we only persist segment state; the new marker has to be serialized alongside catalog snapshots.

## Proposed Enhancements
- Thread the `ResidentSegment` (or a thin handle holding instance + segment IDs) through `FlushCommand::RegisterSegment` so the worker can update tier metadata immediately after `mark_durable` succeeds.
- Introduce a `DurabilityCursor` structure owned by `TieredInstance`. `SegmentFlushState::mark_durable` would inform the cursor, which then persists `{requested, durable}` pairs via the coordinator or a lightweight metadata file.
- Extend `Aof::segment_snapshot`/`recover_existing_segments` to include durability cursors, ensuring restarted processes rebuild the tier view before serving reads.
- Add retry and metrics plumbing around tier metadata persistence to mirror the existing disk flush retry loop. Failures should increment dedicated counters and surface a `WouldBlock(BackpressureKind::Flush)` when the queue saturates.

## Open Questions
- Should durability metadata live inside the existing Tier1 manifest, or do we introduce a dedicated sidecar to avoid bloating hot paths?
- Can we batch metadata updates per segment to amortize Tier2 latency, or do we require synchronous persistence for every flush acknowledgement?
- How does the cursor interact with multi-segment flush queues when backpressure forces out-of-order commits?
- Do we need coordinator notifications (similar to admission/rollover) for readers waiting on durability markers, or will existing `Notify` handles suffice once the cursor is persisted?

## Next Steps
- Validate the handle that accompanies `FlushCommand::RegisterSegment` provides enough information to update tier metadata without re-locking `AofManagement`.
- Prototype the `DurabilityCursor` API and decide whether it belongs in `TieredInstance` or a dedicated tier metadata service.
- Sketch persistence format options (local JSON/CBOR vs manifest extension) and run them by the recovery stakeholders before coding.
