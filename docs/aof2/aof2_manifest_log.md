# AOF2 Manifest Log

## Goals
- Provide an append-only manifest that captures every tier transition for each segment.
- Encode records in a compact, little-endian binary format that supports zero-copy replay.
- Stream sealed chunks to Tier 2 for durability while keeping restart disks bounded.
- Enable deterministic recovery by combining optional snapshots with chunk replay.

## Binary Record Layout
All records are little-endian and start with a fixed header aligned to eight bytes.

```
struct RecordHeader {
    u16 record_type;      // see Record Types table
    u16 flags;            // reserved for future use (must be 0)
    u32 payload_len;      // number of payload bytes following the header
    u64 segment_id;       // unique per stream
    u64 logical_offset;   // interpretation depends on record type
    u64 timestamp_ms;     // ms since UNIX_EPOCH
    u64 payload_crc64;    // crc64-nvme over the payload bytes
}
```

The payload is type specific. Fixed-size payloads are simple structs; variable data is length-prefixed inside the payload and also covered by the CRC.

### Record Types
| Type | Id | Payload | Notes |
| --- | --- | --- | --- |
| `SegmentOpened` | 0 | `struct { u64 base_offset; u32 epoch; }` | Emitted when Tier 0 starts a new segment. |
| `SealSegment` | 1 | `struct { u64 durable_bytes; u64 segment_crc64; }` | Marks segment sealed in Tier 0; durable bytes used for rollover accounting. |
| `CompressionStarted` | 2 | `struct { u32 job_id; }` | Tier 1 compression admitted. |
| `CompressionDone` | 3 | `struct { u64 compressed_bytes; u32 dictionary_id; }` | Compression finished and warm artifact durable. |
| `CompressionFailed` | 4 | `struct { u32 reason_code; }` | Background compression errored; reason follows existing enum. |
| `UploadStarted` | 5 | `struct { u16 key_len; /* key bytes */ }` | Tier 2 upload began for the warm artifact. |
| `UploadDone` | 6 | `struct { u16 key_len; u16 etag_len; /* strings */ }` | Upload completed; includes Tier 2 object key and ETag. |
| `UploadFailed` | 7 | `struct { u16 key_len; u32 reason_code; }` | Upload failed; key allows retry correlation. |
| `Tier2DeleteQueued` | 8 | `struct { u16 key_len; }` | Delete requested for Tier 2 object. |
| `Tier2Deleted` | 9 | `struct { u16 key_len; }` | Delete succeeded and cache eviction allowed. |
| `HydrationStarted` | 10 | `struct { u64 source_tier; }` | Hydration requested from Tier 1 or Tier 2. |
| `HydrationDone` | 11 | `struct { u64 resident_bytes; }` | Hydration satisfied and Tier 0 residency restored. |
| `HydrationFailed` | 12 | `struct { u32 reason_code; }` | Hydration failed; triggers retry/backoff. |
| `LocalEvicted` | 13 | `struct { u64 reclaimed_bytes; }` | Tier 0 eviction completed. |
| `SnapshotMarker` | 14 | `struct { u64 chunk_index; u64 chunk_offset; u64 snapshot_id; }` | Associates a catalog snapshot with a log position. |
| `CustomEvent` | 15 | `struct { u32 schema_id; u16 blob_len; /* bytes */ }` | Escape hatch for future tier events. |

Payload sizes stay power-of-two aligned to simplify writing. `logical_offset` carries context (for example Tier 2 upload uses the warm artifact base offset; `LocalEvicted` uses the last durable byte). Empty records are not allowed.

## Chunk Format
Chunks are preallocated regions sized between 16 KiB and 1 MiB (configurable). Each chunk is backed by a writable mmap while active and becomes read-only once sealed.

```
struct ChunkHeader {
    u64 magic;            // 0x414F46324D4C4F47 ("AOF2MLOG")
    u32 version;          // currently 1
    u32 header_len;       // sizeof(ChunkHeader)
    u64 chunk_index;      // monotonically increasing per stream
    u64 base_record;      // record ordinal of the first entry in this chunk
    u64 tail_offset;      // first unwritten byte within the chunk
    u64 committed_len;    // bytes flushed via fsync
    u64 chunk_crc64;      // crc64-nvme of [header_len, committed_len)
    u64 flags;            // bit 0 = sealed, bit 1 = uploaded
}
```

Writers stage records in-memory, copy them into the mmap, and advance `tail_offset`. On commit they update `committed_len`, recompute the chunk CRC, and call `fsync`. Readers verify both the header and payload CRCs before yielding records.

### Metadata and Object Layout
- `chunk_capacity` defaults to 256 KiB; rotation triggers when `tail_offset + max_record_size > chunk_capacity` or when a timer (default 250 ms) fires.
- Sealed files upload to Tier 2 using the key pattern `manifest/<stream_id>/<chunk_index>.mlog`.
- Object metadata includes `chunk_crc64`, `base_record`, `record_count`, and the latest `SnapshotMarker` id to aid remote validation.
- Local retention keeps the most recent two sealed chunks plus the active writer. Additional chunks delete once Tier 2 upload and snapshot checkpoint succeed.

### Chunk Lifecycle
- **Creation**: the writer preallocates `manifest/<stream_id>/<chunk_index>.mlog.tmp` to the configured `chunk_capacity`, applies `ftruncate` growth in 64 KiB steps if a record would overflow the reservation, and then mmaps the file with read/write access. The file is renamed to `.mlog` once the first record commits so crash recovery never replays empty chunks.
- **Active writes**: record headers and payloads are copied into the mmap using relaxed ordering so the payload lands first; the writer then updates `tail_offset` and folds the payload bytes into the running `crc64` accumulator. Header fields stay in little-endian layout and are not included in the payload checksum.
- **CRC coverage**: `payload_crc64` lives in the record header and covers exactly the payload slice. When committing, the writer recomputes `chunk_crc64` over the byte range `[header_len, committed_len)`; the header is excluded so we can rewrite `tail_offset` and `committed_len` without invalidating prior data. Readers verify the header separately with a fixed-magic check and validate each payload CRC before yielding the record.
- **Sealing**: once rotation criteria trip, the writer sets the header `flags` bit, flushes the mmap, and fsyncs the descriptor. The sealed file stays read-only and is streamed to Tier 2 using the final `.mlog` name.
- **Eviction**: after Tier 2 confirms durability and retention logic observes a newer snapshot, the chunk is unlinked locally; if Tier 2 compaction later tombstones the object, the manifest emits paired delete records instead of mutating the chunk in place.

## Writer Workflow
1. `ManifestLogWriter::append` acquires the active chunk mmap, encodes the record header and payload, and updates a running CRC64 using the zero-copy checksum helper shared with segment files.
2. `commit()` flushes the active chunk to disk based on either byte budget or a periodic durability timer. Commits update the chunk header atomically under a file lock.
3. `rotate()` seals the chunk, sets the header flag, schedules a Tier 2 upload task, and allocates the next chunk file.
4. Tier 2 uploader streams the sealed bytes directly from the mmap, leveraging zero-copy sends on platforms that support it. On success it appends an `UploadDone` record to the next chunk (or a small rotation chunk) to keep history linear.
5. When Tier 2 confirms durability, the retention policy marks older chunks eligible for deletion once referenced snapshots advance.

### MAN1 Chunk Writer Scaffolding
- **ChunkHandle**: RAII wrapper bundling the file descriptor, mmap pointer, and `ChunkHeader`. Constructor performs `ftruncate` to `chunk_capacity`, mmaps the region, and zeroes the header. Drop unmaps unless ownership transfers to a sealed handle.
- **CrcAccumulator**: lightweight helper that wraps the shared `crc64_nvme` table to stream payload bytes as they are encoded. Exposes `fold_bytes(span)` for header-adjacent helpers and `finalize()` that returns the checksum without mutating internal state.
- **AppendPath**: `ManifestLogWriter::append` uses a `RecordEncoder` to serialize headers into a stack buffer, copies payload + header into the chunk, and records the `tail_offset`. Prior to the copy it runs `ensure_capacity(max_record_len)` which either grows the file in-place (`ftruncate` + `mremap` on Linux) or triggers `rotate()`.
- **CommitPath**: `commit()` writes `tail_offset` and `committed_len` to the header with `writeback()` using `pwrite` so partial updates never corrupt the header. It then recomputes `chunk_crc64` by feeding `[header_len, committed_len)` into the shared CRC helper, stores the value, and fsyncs when the feature flag enables durability.
- **RotationTriggers**: `needs_rotation()` evaluates `(tail_offset + max_record_size > chunk_capacity)` or `now_ms() - last_commit_ms >= rotation_period_ms`. When true we set header flag bits, downgrade the mmap to read-only, and enqueue the resulting `SealedChunkHandle` for Tier 2 upload.
- **Feature Flag guard**: all public entrypoints are wrapped with `if (!aof2_manifest_log_enabled()) return;` so MAN1 lands dormant. Tests override the flag via the config surface described below.

## Append and Replay Examples

### Appending and Rotation
The writer API keeps I/O sequencing explicit: append => commit => optional rotation. A typical Tier 0 transition looks like:

```cpp
ManifestLogWriter writer(stream_manifest_path);

SegmentOpened opened{
    .base_offset = segment.base_offset(),
    .epoch = segment.epoch(),
};
writer.append(RecordType::SegmentOpened, segment.id(), opened);

SealSegment seal{
    .durable_bytes = segment.durable_bytes(),
    .segment_crc64 = segment.content_crc64(),
};
writer.append(RecordType::SealSegment, segment.id(), seal);

// The durability timer fired; make the writes visible.
writer.commit(/*timestamp_ms=*/now_ms());

if (writer.needs_rotation()) {
    writer.rotate();       // seals mmap, flips header flag, fsyncs
    tier2_enqueue_upload(writer.sealed_chunk_path());
}
```

`append` rejects writes that would overflow the active chunk; backing storage grows via `ensure_capacity()` beforehand, so rotation is deterministic. `commit` recalculates `chunk_crc64` for the committed range and persists the header before releasing the chunk back to producers.

### Replay and Integrity Checks
Readers perform structural validation before yielding records. The minimal replay loop demonstrates CRC enforcement and gap handling:

```cpp
ManifestLogReader reader(stream_manifest_path);

while (auto chunk = reader.next_chunk()) {
    reader.verify_chunk_header(*chunk);   // magic + chunk_crc64

    for (auto rec = chunk->next_record(); rec; rec = chunk->next_record()) {
        const auto& header = rec->header();
        if (!rec->verify_payload_crc()) {
            throw ManifestIntegrityError{
                chunk->index(), header.segment_id, header.logical_offset};
        }

        replay_manifest_record(*rec);
    }
}
```

`verify_chunk_header` recomputes the chunk CRC before exposing the iterator. Individual records use the embedded `payload_crc64`; checksum failures short-circuit replay so the coordinator can quarantine the stream. Rotation boundaries appear as sealed chunks; readers treat missing uploads as recoverable gaps because all Tier 2 retries are serialized via log records.

## Replay and Startup Reconciliation
1. On boot locate the latest `ManifestSnapshot`. The snapshot contains the in-memory manifest map plus `chunk_index` and `chunk_offset` of the associated `SnapshotMarker`.
2. Map and verify all chunks with index greater than or equal to the snapshot chunk. For the snapshot chunk, start replaying at the recorded offset.
3. Replay records sequentially, applying them to `ManifestIndex`. Records that reference future segments (for example uploads of a segment not yet opened) enqueue reconciliation work rather than failing.
4. After local replay, reconcile Tier 2 by listing `manifest/<stream_id>/` and validating checksums. Missing objects trigger a rebuild from local chunks if available or mark segments as `NeedsRepair`.
5. Followers optionally download missing sealed chunks during replay based on retention data embedded in `Tier2Deleted` records.

## Snapshot Alignment
`ManifestSnapshot` persists the catalog alongside the manifest log watermark. The snapshot file stores:
- `snapshot_id`, `chunk_index`, and `chunk_offset`.
- `HashMap<SegmentId, ManifestEntry>` containing warm artifact metadata, residency flags, Tier 2 keys, and outstanding jobs.
- Pending Tier 2 uploads and deletes so the coordinator can resume the queue without double-issuing work.

Snapshots live beside chunk files (`manifest/<stream_id>/snapshot_<snapshot_id>.bin`). Writers append a `SnapshotMarker` record immediately after a snapshot is durable so replay can fast-forward.

## Retention and Tier 2 Merge
- Local disks keep the active chunk, the previous sealed chunk, and any chunk younger than the most recent snapshot. Additional sealed chunks are deleted after Tier 2 upload and snapshot advancement succeed.
- Tier 2 eventually compacts many small manifest chunks into weekly bundles using the object store's compose API. Compaction writes a new chunk containing a `SnapshotMarker` and then tombstones the superseded objects via `Tier2DeleteQueued` and `Tier2Deleted` records.
- `ManifestIndex` exposes iterators that fold older records to produce a minimal state map for diagnostic tooling, ensuring compaction does not lose history.

## Module Scaffolding
- `manifest::writer`: chunk allocator, append path, CRC helpers, rotation policy, Tier 2 streaming hook.
- `manifest::reader`: chunk iterator, checksum verification, record decoding, bounded replay cursors.
- `manifest::snapshot`: snapshot encoder/decoder plus glue that ties catalog checkpoints to chunk offsets.
- `manifest::index`: in-memory map keyed by `SegmentId`, integrating with Tier 1 residency tracking.
- All modules live under `rust/bop-rs/src/aof2/store/manifest/` with relative includes to existing CRC and mmap utilities.

## Integration Plan
- Emit manifest log records from existing tier transitions (`seal_segment`, `Tier1Cache` compression completions, upload lifecycles, hydrations, evictions).
- Replace the unused JSON manifest plumbing in `Tier1Instance` with replay from `ManifestIndex`, removing the legacy serialization stub once the log is wired up.
- Update `aof2_store.md` milestones under MAN1-MAN4 to reference the log and link to this design.
- Coordinator services append records as part of the same critical section that updates in-memory state to maintain atomicity.

## Recovery, Tooling, and Metrics
- `manifest inspect <stream_id>`: dump decoded records and highlight divergent Tier 2 objects.
- `manifest verify`: recompute CRCs, confirm Tier 2 presence, and ensure local retention policy invariants hold.
- Metrics: `manifest_chunks_active`, `manifest_replay_seconds`, `manifest_crc_error_count`, upload retry counters, and snapshot age gauges.
- Logging: structured fields include segment id, tier actions, chunk index, and chunk offsets for quick debugging.

## Testing and Rollout
- Unit tests cover header encode/decode, CRC mismatches, rotation boundaries, and payload round-trips.
- Integration tests simulate crash mid-chunk, restart using snapshot plus tail replay, Tier 2 upload failure with retry, and chunk compaction.
- Rollout keeps the manifest log behind a feature flag until Tier 1 replay burn-in completes; once validated we delete the dormant JSON manifest code paths (no data migration required).

### Integration Test Matrix (MAN1/MAN3)
- **Rotation boundaries**: drive append workloads that produce records with varying payload sizes to force multiple rotations, verifying `chunk_crc64` continuity and that replays stitch the sequence number without gaps.
- **Crash on commit**: simulate process termination after writing payload bytes but before header update; restart must detect the uncommitted tail, trim it, and replay the committed prefix without double-appending.
- **Tier 2 retry**: inject upload failures that force `UploadFailed` followed by retry; confirm the next chunk records `UploadDone` with consistent keys and that replay enqueues reconciliation once durability lands.
- **Snapshot replay**: take a `ManifestSnapshot`, append additional chunks, then replay from the snapshot to assert the snapshot marker offset works and hydration tasks schedule once.
- **Follower catch-up**: mirror chunks to a follower instance, drop intermediate Tier 2 objects, and ensure replay queues hydrations based on manifest delete records.

## Rollout and Observability Plan
- **Feature flag**: `aof2_manifest_log_enabled` gates append paths and replay on coordinators. Flag defaults to `false` in production; we stage rollout per-cell by enabling the flag for shadow writes first (append without replay) and then flipping readers once telemetry is clean. The flag lives in the shared coordinator config so we can atomically roll back.
- **Telemetry**: existing counters (`manifest_chunks_active`, `manifest_replay_seconds`, `manifest_crc_error_count`) emit under the `aof2.manifest_log` namespace. During shadow mode we also track `manifest_shadow_divergence`—a gauge capturing mismatches between JSON and manifest-derived state—to ensure the new log matches legacy behavior. Grafana dashboards group these with Tier 2 upload metrics so SREs can correlate failures with chunk rotations.
- **Legacy JSON removal**: when the flag flips to true for both writers and readers, the unused JSON manifest plumbing in Tier 1 transitions into a no-op that simply records instrumentation stubs. After two releases without regressions we remove the JSON structs entirely as documented under `docs/aof2/aof2_store.md` (see MAN3 milestone). The cleanup CL rewrites call sites to rely solely on `ManifestIndex`, ensuring the removal is a behavioral no-op for enabled cells.

### Feature Flag Plumbing Plan
- **Config surface**: add `manifest_log_enabled` boolean to `TieredStoreConfig` with per-environment overrides; manager loads the value and passes it into `ManifestLogWriter::new`.
- **Runtime toggle**: expose the flag through the runtime config API so we can flip it without restart. Coordinators cache the value but listen for change notifications to toggle append/replay accordingly.
- **Testing hooks**: unit tests call `TestManager::with_manifest_log()` to force-enable the flag, while integration tests use the runtime toggle to simulate rollout sequences.
- **Telemetry validation**: shadow mode double-writes to the log and legacy JSON, exporting `manifest_shadow_divergence` gauges; alerting fires on non-zero values. Once the flag is fully enabled we add a one-off SLO to ensure manifest replay latency stays within thresholds and that chunk CRC errors remain zero.


