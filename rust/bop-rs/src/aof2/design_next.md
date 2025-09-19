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
- `Layout::ensure` creates `segments/` and `archive/` under the configured root and fsyncs the directories.
- `segments/` holds the active tail and all finalized segments. Files end with `.seg`; temporary files created while sealing or replacing a segment live beside their target before being atomically renamed.
- `archive/` is empty unless retention moves sealed segments out of the hot set.
- Preallocation chooses a power-of-two size between 64 MiB and 1 GiB for each segment. The runtime may slide within this window to keep pace with the filesystem.
- There is no manifest, metadata log, or side-band index in the new design; segments alone describe durable state.

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
- The append path treats `[SEGMENT_HEADER_SIZE, max_size - FOOTER_SIZE)` as writable record space so the reserved footer slot remains untouched.

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
- remaining bytes reserved and zeroed
- The header is written once via `Segment::ensure_header_written` immediately before writing the first record.

**Record entry**
- 4 bytes length (`u32`, payload only)
- 4 bytes checksum (`u32`, CRC64-NVME folded into 32 bits by XOR upper/lower halves)
- 8 bytes timestamp (`u64` nanoseconds)
- payload bytes
- There is no stored record id. The offset returned by `Segment::append_record` combined with the owning `segment_index` recreates the id.

**Footer (64 bytes)**
- Occupies the final 64 bytes of the preallocated file (`max_size - 64 .. max_size`), keeping its location deterministic for recovery.
- Written when `Segment::seal` completes flushing the file; the append path reserves the tail so record payloads never overlap the footer slot.
- Contains magic `0x464F4F54` (`"FOOT"`), `segment_index`, `last_record_id` (packed), `record_count`, `durable_bytes`, `last_timestamp`, a checksum of the written region, and `sealed_at`.
- Because the footer sits at a known offset, recovery can mmap the last 64 bytes and validate status without scanning the full payload.
- If the footer is missing or corrupted, the runtime treats the segment as the tail candidate and falls back to a header-guided scan.

## 4. Core Components
### 4.1 `AofManager`
- Owns the shared `Arc<Runtime>` and coordinates background workers for flush, seal, and retention tasks.
- Tracks active maintenance jobs per segment in a `DashMap` keyed by `SegmentId`.
- Provides a command queue used by `Aof` instances to request fsync, rollover, and archival work.
- Enforces the "two unfinalized segments" rule by delaying new allocations when finalization of the previous tail has not completed.

### 4.2 `Aof`
- Represents one logical append-only stream backed solely by the `segments/` directory under its configured root.
- Holds runtime configuration (`AofConfig`) and mutable state guarded by a `Mutex`: ordered segments, the active tail, `next_segment_index`, `next_offset`, and `record_count`.
- `ensure_active_segment` creates a new segment by calling `Segment::create_active` with the current cumulative counters and a size chosen within the configured range.
- All durable metadata lives in memory while the process runs; on restart it is reconstructed by scanning segment headers.

### 4.3 `Segment`
- Wraps a memory-mapped file and exposes `create_active`, `append_record`, `mark_durable`, and pending `seal` APIs.
- Maintains atomic counters for bytes written, durable bytes, record count, and timestamps so multiple subsystems can observe progress without locks.
- Lazily emits the segment header the first time `append_record` runs, ensuring the base timestamp reflects the first payload.
- Returns a `SegmentAppendResult` containing the segment-relative offset, the cumulative byte position, and an `is_full` flag used to trigger rollover.
- Future work will add footer writing and read-only remapping once a segment is sealed.

### 4.4 `Layout`
- Encapsulates directory handling, including parsing and formatting segment filenames.
- Creates temp files through `TempFileGuard` so sealing can write footers to a temporary copy and atomically persist the result.

## 5. Concurrency and Synchronization
- A single writer holds the `AofState` lock while selecting or rolling segments; actual payload writes are lock-free after the reservation.
- `Segment::append_record` uses atomics for reservation (`size.fetch_update`) and for state tracking (`header_written`, `record_count`, `last_timestamp`).
- Readers share `Arc<Segment>` handles; finalized segments are mapped read-only so random access requires no coordination with the writer.
- Manager commands execute on dedicated threads or blocking tasks to keep async callers from paying fsync latency.
- The writer may have at most two segments without confirmed footers: the current active segment and the segment that just sealed. Allocation of another segment waits for the older pending footer to be persisted.

## 6. Append Pipeline
1. `Aof::append_record(payload)` validates the payload and captures the current timestamp.
2. With the state lock held, `ensure_active_segment` either reuses the existing tail or allocates a new segment using cumulative counters and the dynamic size selector.
3. The append loop calls `Segment::append_record`, which writes the header on first use, copies the record header and payload into the mmap, and returns the previous offset.
4. `Aof` converts the returned offset into a logical id using `RecordId::from_parts(segment_index, segment_offset)`.
5. State counters (`next_offset`, `record_count`) update atomically. If the segment filled, the tail is cleared so the next loop iteration allocates a new segment.
6. Before activating a second successor segment, the runtime waits for the prior segment’s footer write to complete to honour the two-unfinalized rule.
7. The caller receives the `RecordId` once bytes are copied into the memory map; durability is handled asynchronously via flush commands.

## 7. Flush and Durability
- Flush jobs run per segment and advance `durable_size` via `Segment::mark_durable`.
- `FlushConfig` thresholds (watermark bytes and interval) determine when the manager schedules fsync for the active segment.
- Sealing forces a final flush, writes the footer at the reserved tail location, remaps the file read-only, and notifies waiting readers.
- `Aof::flush(record_id)` (future work) will translate the id into a byte boundary and await `durable_size >= offset`.

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
1. Enumerate `segments/`, filter for `.seg`, and parse each filename into `(segment_id, base_offset, created_at)`. Sort primarily by `segment_id`.
2. For every segment except the newest two, read the 64-byte header, validate the magic/version, and trust the stored cumulative counters to rebuild `next_segment_index`, `next_offset`, and `record_count`.
3. Inspect the tail (and penultimate segment if the tail lacks a footer) by scanning record headers: verify lengths and checksums, truncate trailing garbage, and compute the durable size.
4. If a footer is present, trust its counts; otherwise derive them from the scan. Update the in-memory state accordingly and mark the tail as active.
5. Recreate the active segment map/set entirely from these headers. No manifest replay or auxiliary metadata files are necessary.

## 11. Flow Control and Backpressure
- `FlushConfig::max_unflushed_bytes` limits acknowledged-but-not-durable data; exceeding it blocks new appends until a flush completes.
- The manager monitors outstanding flush jobs and can throttle or seal the tail early if the writer outruns I/O.
- The two-unfinalized rule provides natural pressure when the filesystem is slow: new segments will not activate until the oldest pending footer is durable.
- Retention hooks ensure archival moves happen off the hot path so they do not interfere with the writer.

## 12. Observability and Telemetry
- Counters: bytes appended, durable bytes, record count, flush operations, segment rollovers, recovery duration.
- Histograms: append latency, flush latency, bytes per segment.
- Structured events: checksum mismatch, recovery truncation, footer write failure.
- Integrate with `tracing` to surround append, flush, and seal operations.

## 13. Configuration Surface (`AofConfig`)
- `root_dir`: directory containing `segments/` and `archive/`.
- `segment_min_bytes`: lower bound for dynamically sized segments (default 64 MiB, enforced power-of-two).
- `segment_max_bytes`: upper bound for segment preallocation (default 1 GiB, enforced power-of-two).
- `segment_target_bytes`: soft rollover point; it is clamped to the active segment size and may slide within the configured range based on runtime heuristics.
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
8. **Flush coordination** *(pending)*  
   - Background flush manager with single in-flight semantics per segment and durability tracking hooked into `FlushConfig`.
9. **Retention and archival hooks** *(pending)*  
   - Policies for migrating sealed segments into `archive/`, pruning old data, and (eventually) packaging cold segments.
10. **Observability and tooling** *(pending)*  
    - Metrics, tracing integration, administrative commands, and benchmarks.
