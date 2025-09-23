# Asynchronous Segmented AOF (AOF2) Design

## 1. Goals and Non-Goals
- Deliver a high-throughput append-only log with predictable latency under heavy write pressure.
- Permit zero-copy or near-zero-copy reads of finalized segments via memory maps.
- Guarantee durability once fsync/fdatasync completes, even across crashes.
- Support concurrent readers while a single writer appends.
- Enable background maintenance (fsync, segment rotation, retention, archival) without blocking the hot path.
- Keep implementation dependency-light (Tokio runtime, memmap2, crossbeam) and compatible with async Rust ecosystems.
- Minimize persistent metadata: segment files are the source of truth and no manifest or metadata log is required.
- Non-goals: distributed replication, transaction semantics, automatic compaction/defragmentation, multi-writer coordination across processes.

## 2. Workload Assumptions and Constraints
- Segments live in a dedicated directory on fast local storage with an optional tier for archived segments.
- Writes arrive from a single thread or executor per `Aof` instance, but multiple instances may share one `AofManager`.
- Typical record sizes range from 128 B to 64 KiB; the maximum record size must be less than the configured segment size.
- Crash recovery must handle torn writes, truncated files, and metadata that was never flushed.
- Tokio runtime is provided externally or spun up per manager; blocking syscalls must use `spawn_blocking`.
- Historical data is rarely accessed; design choices prioritise hot-tail performance while allowing cold segments to move to archives with minimal overhead.
- The storage root contains the `segments/` directory, an optional `archive/` directory, and transient temp files created during sealing.

## 3. Storage Layout
### 3.1 Directory Structure
```
<root>/
  segments/
    <segment_id>_<base_offset>_<created_at>.seg
  archive/
    <segment_id>_<sealed_at>.seg   # optional, retention dependent
```
- `Layout::ensure` creates `segments/`, `archive/`, and `manifest/` under the configured root and fsyncs the directories.
- `segments/` holds the active tail and all finalized segments. Files end with `.seg`; temporary files created while sealing or replacing a segment live beside their target before being atomically renamed. The directory also hosts `current.sealed`, an atomic pointer updated after each successful seal so recovery can jump directly to the latest durable segment.
- `archive/` is empty unless retention moves sealed segments out of the hot set.
- `manifest/` stores the MAN1 chunked manifest log used by Tier1/Tier2 residency. Each chunk records seal/compression/upload events and survives crash recovery alongside the enriched headers/footers.
- Preallocation chooses a power-of-two size between 64 MiB and 1 GiB for each segment. The runtime may slide within this window to keep pace with the filesystem while respecting Tier 0 budget.

### 3.2 Segment Naming and IDs
- `SegmentId` is a monotonically increasing `u64` assigned by the runtime; the high-level manager keeps the next id in memory.
- Filenames follow `SegmentFileName::format` and encode three values separated by underscores: `{segment_id}_{base_offset}_{created_at}.seg`. All numeric components are zero-padded to 20 digits to preserve lexical ordering.
- `base_offset` is the cumulative byte count of all bytes durable before this segment (including previous headers, bodies, and footers).
- `created_at` captures the UTC timestamp (nanoseconds) when the segment became active; it aids debugging and recovery heuristics.

### 3.3 Packed Record Identifiers
- `RecordId::from_parts(segment_index, segment_offset)` packs a `{u32, u32}` tuple into a single `u64`.
- `segment_index` is `SegmentId::as_u32()` and maps directly to the file chosen at read time.
- `segment_offset` is the byte offset within the file, measured from the start of the segment (offset `SEGMENT_HEADER_SIZE` corresponds to the first record).
- Helpers expose `record_id.segment_index()` and `record_id.segment_offset()` so readers can derive file names and seek positions without auxiliary structures.
- Because offsets live inside the id, recovery can rebuild the monotonic record sequence without parsing every record body.

### 3.4 Segment File Format
```
+----------+---------------------------+--------+
| Header   | Record entries            | Footer |
+----------+---------------------------+--------+
```
- Segments are preallocated to `segment_max_bytes` but start zero-initialized.
- `Segment::create_active` maps the file and defers writing the header until the first append supplies the initial timestamp.
- The append path treats `[SEGMENT_HEADER_SIZE, max_size - SEGMENT_FOOTER_SIZE)` as writable record space so the reserved footer slot remains untouched.

**Header (64 bytes)**
- magic `0x414F4632` (`"AOF2"`)
- version `u16`
- header_length `u16` (always 64 to simplify compatibility checks)
- segment_index `u32`
- base_record_count `u64` (number of committed records before this segment)
- base_offset `u64` (cumulative durable bytes before this segment)
- max_size `u32`
- created_at `i64` (UTC nanoseconds)
- base_timestamp `u64` (timestamp of the first record appended to this segment; zero until known)
- ext_id `u64` (external cohort identifier propagated from the runtime; defaults to zero)
- the remaining bytes are reserved and zeroed so future schema bumps stay binary compatible
- The header is written once via `Segment::ensure_header_written` immediately before writing the first record.

**Record entry**
- 4 bytes length (`u32`, payload only)
- 4 bytes checksum (`u32`, CRC64-NVME folded into 32 bits by XOR upper/lower halves)
- 8 bytes timestamp (`u64` nanoseconds)
- 8 bytes ext_id (`u64`; zero when the upstream log is anonymous)
- payload bytes
- There is no stored record id. The offset returned by `Segment::append_record` combined with the owning `segment_index` recreates the id.

**Footer (96 bytes)**
- Occupies the final `SEGMENT_FOOTER_SIZE` bytes of the preallocated file, keeping its location deterministic for recovery.
- Written when `Segment::seal` completes flushing the file; the append path reserves the tail so record payloads never overlap the footer slot.
- Fields: magic `0x464F4F54` (`"FOOT"`), version, `segment_index`, `base_record_count`, `record_count`, `durable_bytes`, `last_record_id` (packed), `last_timestamp`, `sealed_at`, payload checksum, `ext_id`, `coordinator_watermark`, and a single-byte `flush_failure` flag.
- Because the footer sits at a known offset, recovery can mmap the last 96 bytes and validate status without scanning the full payload. Footer enrichment means the runtime no longer needs an external durability snapshot to rebuild catalog state.
- If the footer is missing or corrupted, the runtime treats the segment as the tail candidate and falls back to a header-guided scan.

**Current sealed pointer**
- `segments/current.sealed` stores `{segment_id, durable_bytes, coordinator_watermark}` for the latest acknowledged seal.
- The pointer file is updated atomically via a temp-file + rename sequence once the footer and fsync succeed, giving restart a constant-time jump to the live sealed tail.
- Tier1 tracks a `NeedsRecovery` residency state for warm artifacts that fail to materialize. When the warm file is missing, the cache logs `HydrationFailed`, queues a Tier2 fetch, and keeps the entry pinned until the fetch-complete event logs `HydrationDone` and returns the entry to `StagedForTier0`.

## 4. Core Components
### 4.1 `AofManager`
- Owns the shared `Arc<Runtime>` and coordinates background workers for flush, seal, and retention tasks.
- Tracks active maintenance jobs via `SegmentFlushState` handles contributed by every `Aof`; a concurrent registry lets the manager service many streams in parallel.
- Uses lock-free command queues so enqueue/dequeue never contend with appenders; commands carry `Arc<SegmentFlushState>` instances or archival directives and are processed on a dedicated worker pool.
- Emits lightweight notifications when flushes or seals complete so individual `Aof` instances can release backpressure or advance to the next segment.
- Enforces the "two unfinalized segments" rule by delaying new allocations when finalization of the previous tail has not completed.

### 4.2 `Aof`
- Represents one logical append-only stream backed solely by the `segments/` directory under its configured root.
- Splits its internal state into two tiers:
  * **Hot append state** stored in atomics (`tail_ptr`, `next_offset`, `record_count`, backpressure gauges) that can be read/written without locking.
  * **Management state** (pending finalisations, preallocation queue, recovery metadata) protected by an async-aware mutex so background workers can coordinate without blocking appenders.
- Provides `append_record` (non-blocking) and `append_record_with_timeout` which delegates to `wait_for_writable_segment(timeout)` when the tail is missing or backpressure requires waiting.
- On restart it reconstructs the hot state by scanning segment headers/footers using the recovery helpers introduced in this design.

### 4.3 `Segment`
- Wraps a memory-mapped file and exposes `create_active`, `append_record`, `mark_durable`, `seal`, `load_header`, and `scan_tail`.
- Maintains atomic counters so multiple subsystems can observe progress without locks; sealing verifies durability and writes the footer at the reserved tail slot.
- `Segment::load_header` and `Segment::scan_tail` feed recovery with header/footer metadata and per-record checksums, ensuring manifest-free bootstrap.
- Returns a `SegmentAppendResult` containing the segment-relative offset, the cumulative byte position, and an `is_full` flag used to trigger rollover.

### 4.4 `Layout`
- Encapsulates directory handling, including parsing and formatting segment filenames.
- Creates temp files through `TempFileGuard` so sealing can write footers to a temporary copy and atomically persist the result.

## 5. Concurrency and Synchronization
- Append threads interact solely with lock-free hot-path state: atomics hold the active tail pointer, `next_offset`, logical record count, and backpressure gauges. `Segment::append_record` never touches the management mutex.
- Management tasks (pending finalisations, preallocated segments, recovery metadata) live behind a separate async-aware mutex. Background workers operate on this lock without affecting append throughput.
- `wait_for_writable_segment(timeout)` bridges the two domains. It first checks the atomic tail; only if no writable handle exists does it briefly acquire the management lock to trigger allocation or wait for a flush. When the timeout expires it returns `AofError::WouldBlock(BackpressureKind::Admission)` instead of blocking the caller.
- Backpressure bookkeeping is entirely atomic. Exceeding thresholds flips a flag that causes `append_record` to fail fast with `WouldBlock`, leaving higher layers to decide when to retry or wait.
- Manager commands run on dedicated threads, working directly with `SegmentFlushState` handles so fsync and allocation latency never block appenders.
- Readers continue to share `Arc<Segment>` handles; finalized segments are mapped read-only so random access requires no coordination with the writer.

## 6. Append Pipeline
1. `Aof::append_record(payload)` validates the payload and reads the current tail atomically.
2. If no writable tail exists, it returns `AofError::WouldBlock(BackpressureKind::Admission)`; callers that need blocking semantics invoke `wait_for_writable_segment(timeout)` to drive allocation or wait within the provided deadline.
3. Once a writable segment handle is available, the append loop writes through `Segment::append_record`, which updates atomics but never touches the management mutex.
4. `RecordId` values are synthesised from the segment index and returned offset and handed back immediately.
5. When a segment reports `is_full`, the handle enqueues its `SegmentFlushState` for the manager and clears the hot tail pointer; the management lock is touched only during this enqueue.
6. If atomic backpressure gauges indicate thresholds were exceeded, `append_record` fails fast with `WouldBlock`, leaving decisions about waiting or retrying to the caller.
7. Durability work happens on the background queue; appenders never wait on a flush just to progress.

## 7. Flush and Durability
- Each segment exposes a `SegmentFlushState` describing its durability goal and providing async notifiers. The `AofManager` polls these states concurrently without touching the append lock.
- `FlushConfig` thresholds still determine when to enqueue a flush, but enqueues happen via lock-free command queues so appenders never contend with background work.
- `FlushManager::enqueue_segment` accepts a `FlushRequest` bundling `ResidentSegment` and `InstanceId` (`crates/bop-aof/src/flush.rs:30`), giving the worker everything it needs to flush the segment to disk and then persist the {requested, durable} pair to `warm/durability/<instance>.json`. `Segment::mark_durable` only runs after that persistence succeeds, keeping the tier snapshot, JSON metadata, and segment footer in lockstep.
- Sealing consumes a flush-state handle: once durable bytes reach the logical size, `Segment::seal` writes the footer, marks the state finalized, and the manager moves on to the next segment.
- `Aof::flush(record_id)` (future work) will map the record to its `SegmentFlushState` and await its notifier instead of blocking on a global mutex.


See also [AOF2 Read Path Guide](aof2_read_path.md) for reader retry guidance aligned with these tiers.

## 8. Segment Lifecycle
1. **Allocation**: preallocate a zeroed file under `segments/` using a power-of-two size within the configured 64 MiB–1 GiB window. No header is emitted yet.
2. **Activation**: mark the preallocated file as the tail in `AofState` and expose it to the writer. The active writer remembers whether another unfinalized segment already exists.
3. **First append**: the initial call to `append_record` stores the first timestamp, writes the header, and appends the record body.
4. **Active**: subsequent appends reserve space via atomics, update counters, and stream payloads into the mmap. Record ids return immediately after the copy.
5. **Sealing**: once the segment hits the target size (or upon explicit rollover), the manager flushes outstanding bytes, writes the footer into the fixed tail slot, and remaps the file read-only.
6. **Finalize confirmation**: when the footer write completes and is synced, the manager marks the segment finalized, allowing the next preallocated segment to become active.
7. **Archival**: retention may rename or copy the sealed `.seg` file into `archive/`. Active metadata remains entirely in memory, so no manifest update is required.

## 9. Reader Workflow
- Readers decompose `RecordId` into `(segment_index, segment_offset)` to locate the file and starting position.
- Finalized segments are memory-mapped read-only; random access replays the record header, recomputes the checksum, and then yields the payload.
- Tail followers use async notifications while their cursor still points at the active segment; once it seals they reopen the next file.
- Optional sparse indexes can still be introduced later, but the packed ids mean cold reads already know which segment to open.

## 10. Startup and Recovery
1. Load `segments/current.sealed`. When present, the pointer seeds the latest sealed segment id, durable byte offset, and coordinator watermark. The value is trusted only if the referenced segment still exists; otherwise recovery logs a warning and falls back to discovery.
2. Enumerate `segments/*.seg` in lexical order. For each sealed segment, read the 64-byte header and 96-byte footer to rebuild base offsets, record counts, ext_id, watermark, and flush-failure state without consulting auxiliary metadata.
3. Inspect the active tail (and the penultimate segment if the tail lacks a footer) by scanning record headers: verify lengths and checksums, truncate trailing garbage, and compute the live durable size. Tail inspection also seeds the backlog gauge used by the flush manager.
4. Rehydrate the in-memory catalog, pending finalize queue, and durability cursors directly from the collected header/footer data. The highest coordinator watermark observed in the pointer/footer set is written back into the runtime metadata so the append path persists it on the next seal.
5. Replay manifest log chunks (Tier1/Tier2 residency) and upstream operation-log entries starting at `coordinator_watermark + 1` to catch up the warm tiers.

### 10.1 Segment Metadata Enrichment (RC1)
- Segment headers gain an `ext_id` field so every record inherits a cohort identifier without consulting external metadata.
- Record headers mirror the field so replication layers (e.g. Raft) can tag individual entries.
- Segment footers capture durable bytes, record count, coordinator watermark, and the flush-failure flag when sealing.

### 10.2 Atomic Sealed-Segment Pointer (RC2)
- Maintain a `current.sealed` pointer (symlink or small binary blob) that records the latest sealed segment index and watermark.
- Writers update the pointer via `TempFileGuard` + rename after each successful seal so recovery can discover global counters without scanning every segment.

### 10.3 Manifest Alignment (RC3)
- Manifest entries for sealed segments include the same ext_id and coordinator watermark, keeping Tier1/Tier2 replay aligned with on-disk headers.
- Replay prefers the header/footer data and uses manifest deltas exclusively for warm/cold residency.

## 11. Flow Control and Backpressure
### 11.1 Rollover Handshake (AP2)
- **Active**: the writer holds an `AdmissionGuard` and appends freely. When the flush path decides to seal (size, time, or durability budget), it transitions to `RolloverRequested`.
- **RolloverRequested**: `TieredInstance::request_rollover` enqueues a `CoordinatorCommand::Rollover` carrying a oneshot ack. The sync API immediately returns `WouldBlock(BackpressureKind::Rollover)` until the ack fires.
- **CoordinatorProcessing**: the coordinator drains the command, reserves Tier 1 capacity, schedules compression, and stages Tier 2 upload intents. Failures map to `AofError::RolloverFailed` and complete the ack with an error.
- **AckReady**: once Tier 1 metadata is durable and upload work accepted, the coordinator fulfils the ack. `Aof::seal_current_segment` observes the ready signal, promotes the next segment, and wakes admission waiters.
- **PostAckCleanup**: dropping the final `AdmissionGuard` releases Tier 0 residency, allowing queued writers to proceed. Tier 1/Tier 2 workers execute the staged jobs asynchronously.

`Aof::seal_current_segment` polls the ack without blocking the executor. The synchronous path surfaces `WouldBlock` until the ack is resolved, while `append_record_async` awaits the shared notifier before retrying. See [`aof2_store.md`](aof2_store.md#append-path-and-handshake) for event sequencing and [`aof2_implementation_plan.md`](aof2_implementation_plan.md#milestone-4--append-path-integration) for outstanding tasks.

### 11.2 Async Append Retry Loop (AP3)
The async helper is defined in `Aof::append_record_async` (planned under AP3):
- Attempt `append_record` in a loop.
- On `WouldBlock(Admission)`, await the Tier 0 admission notifier published by the coordinator.
- On `WouldBlock(Rollover)`, await the rollover notifier tied to the in-flight ack.
- Propagate other errors immediately.

This retry state machine stays cancel-safe by checking the manager cancellation token between awaits. Writers that do not want to block synchronously should call the async helper directly.

**Notifier matrix.** The helper maps each `BackpressureKind` surfaced by `append_record` to a specific coordinator signal so wakeups align with the tiered store's state transitions:

| WouldBlock kind | Wait primitive | Resume condition |
| --- | --- | --- |
| `Admission` | `TieredCoordinatorNotifiers::admission(instance_id)` | A writable tail exists again (new segment activated, flush freed capacity, or rollover completed). The helper checks for a pending rollover before awaiting admission so it does not race past the ack that activates the new tail. |
| `Rollover` | `TieredCoordinatorNotifiers::rollover(instance_id)` + per-segment `RolloverSignal` | The coordinator resolves the rollover ack (success or error). On success the helper clears the pending entry so subsequent admission waits can observe the promoted tail. |
| `Flush` | _Not yet wired_ | Future work will attach a durability notifier so callers can await completion instead of spinning. Until then the helper bubbles the error. |
| `Hydration`, `Tail`, `Unknown` | _None_ | These indicate a higher-level coordination issue (reader hydration, tail followers, etc.) and are returned immediately for the caller to route appropriately. |

The explicit mapping documents the steady-state expectations for reviewers: admission pressure resolves through Tier 0 capacity signals, rollover pressure resolves through the coordinator ack, and all other cases bubble for now. The async helper continues to watch the manager cancellation token inside each await, ensuring shutdown propagates as `WouldBlock(kind)` with the original context.

- `FlushConfig::max_unflushed_bytes` limits acknowledged-but-not-durable data; exceeding it blocks new appends until a flush completes.
- The manager monitors outstanding flush jobs and can throttle or seal the tail early if the writer outruns I/O.
- The two-unfinalized rule provides natural pressure when the filesystem is slow: new segments will not activate until the oldest pending footer is durable.
- Retention hooks ensure archival moves happen off the hot path so they do not interfere with the writer.

## 12. Observability and Telemetry
- `TieredObservabilitySnapshot` aggregates Tier 0/1/2 metrics (activation queue depth, residency gauge, hydration latency histogram, Tier 2 retry counters) for exporters and the admin CLI.
- Append/flush metrics continue to track bytes appended, durable bytes, record counts, flush operations, segment rollovers, recovery duration, `aof_flush_metadata_*` retry totals, and `would_block_*` frequency per `BackpressureKind`.
- Histograms now cover append latency, flush latency, bytes per segment, admission latency (`AdmissionMetrics`), and hydration latency (p50/p90/p99) derived from Tier 1 transitions.
- Structured `tracing` events capture admissions (`tiered_admit_segment`), Tier 0 evictions/drops, Tier 1 compression/hydration success and failure (including retries), and Tier 2 upload/delete/fetch outcomes with attempt counts.
- `aof2-admin dump --root <path>` exposes the same metrics, residency state, and queue depths as JSON plus a console table for on-demand diagnostics.

## 13. Configuration Surface (`AofConfig`)
- `root_dir`: directory containing `segments/` and `archive/`.
- `segment_min_bytes`: lower bound for dynamically sized segments (default 64 KiB, enforced power-of-two).
- `segment_max_bytes`: upper bound for segment preallocation (default ~4 GiB, enforced power-of-two).
- `segment_target_bytes`: soft rollover point; it is clamped to the active segment size and may slide within the configured range based on runtime heuristics (defaults to 64 KiB).
- `preallocate_segments`: number of spare segments to create ahead of time (respected by the two-unfinalized constraint).
- `id_strategy`: strategy for generating ids (currently the packed `{segment, offset}` form).
- `compression`: optional compression mode applied during sealing (future enhancement).
- `retention`: policy describing how sealed segments move to `archive/` or expire.
- `compaction`: hook for cold segment compaction (future work).
- `flush`: bundle of flush watermarks, intervals, and durability limits.
- `enable_sparse_index`: toggle for optional per-segment index files.

## 14. Testing Strategy
- Unit tests for `RecordId` packing helpers, segment header/record encoding, CRC64 folding, and footer serialization.
- Integration tests that append and read across multiple segments, exercise rollover boundaries, and verify recovery after simulated crashes with partial writes.
- Property tests that mutate tail bytes and confirm recovery truncates back to the last valid record.
- Stress tests that hammer append and flush concurrently to detect data races or lost wakeups, especially around the two-unfinalized constraint.
- Benchmarks that measure append throughput under different segment sizes and flush cadences to validate the dynamic sizing heuristics.

## 15. Detailed Implementation Plan
1. **Configuration and ID packing** *(done)*  
   - Provide `SegmentId`/`RecordId` types plus packing helpers with serde support.
2. **Error surface** *(done)*  
   - Expand `AofError` for recovery, filesystem, and flush semantics.
3. **Filesystem scaffolding** *(done)*  
   - Implement `Layout`, directory creation, fsync helpers, temp file guards, and segment filename parsing.
4. **Segment primitives** *(in progress)*  
   - Finalize `Segment::create_active`, lazy header emission, `append_record`, checksum handling, durable byte tracking, reserved footer slot management, and footer writing.
5. **AOF runtime** *(in progress)*  
   - Manage in-memory state only (no manifest), drive append pipeline, honor the two-unfinalized rule, and expose `RecordId::from_parts` derived ids.
6. **Recovery scanning** *(pending)*  
   - Walk headers plus the tail segment to rebuild cumulative counters and truncate partial records.
7. **Reader API** *(pending)*  
   - Implement synchronous/asynchronous readers that derive offsets from `RecordId` and follow the sealing notifications.
8. **Flush coordination** *(in progress)*  
   - Background flush manager with single in-flight semantics per segment, durability tracking hooked into `FlushConfig`, and a persisted durability cursor snapshot under `warm/durability/<instance>.json` so restart seeds tier metadata before hydrating the catalog.
   - Flush backpressure now toggles a shared failure flag when the worker exhausts retries, surfacing `WouldBlock(BackpressureKind::Flush)` to producers until a successful flush clears it. The flag feeds the new `FlushMetricsSnapshot::flush_failures` counter for alerting.
9. **Retention and archival hooks** *(pending)*  
   - Policies for migrating sealed segments into `archive/`, pruning old data, and (eventually) packaging cold segments.
10. **Observability and tooling** *(done)*  
    - Tiered metrics snapshot (queue depths, latency histogram, retry counters), structured log events for admissions/hydration/Tier2 transfers, and the `aof2-admin` CLI for residency dumps.


