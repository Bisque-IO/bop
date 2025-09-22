# Segment Metadata & Recovery

## Overview
- The standalone catalog snapshot has been replaced by self-describing segment files.
- Runtime state required for restart (catalog, durability watermark, log position) now lives in headers/footers written exactly once.
- An atomic pointer (`segments/current.sealed`) is updated after each successful seal so recovery jumps directly to the latest sealed segment.
- External identifiers (ext_id, e.g. Raft log ids) flow from the append path into headers, record headers, manifest entries, and seal records.

## Segment Header (written on first record)
| Field | Type | Notes |
| ----- | ---- | ----- |
| magic / version | [u8; 8] / u16 | Identifies the header layout; unchanged today. |
| segment_index | u32 | Existing field. |
| base_offset | u64 | Existing field. |
| base_record_count | u64 | Existing field. |
| max_size | u32 | Existing field. |
| created_at | i64 | Existing field (first record timestamp). |
| ext_id | u64 | **New.** External sequence shared by all records in the segment. |

## Record Header
| Field | Type | Notes |
| ----- | ---- | ----- |
| length | u32 | Payload length (existing). |
| checksum | u32 | Payload checksum (existing). |
| timestamp | u64 | Logical timestamp (existing). |
| ext_id | u64 | **New.** Optional external record identifier (e.g. Raft log index); zero if unused. |

## Segment Footer (written exactly once when sealing)
| Field | Type | Notes |
| ----- | ---- | ----- |
| magic / version | [u8; 8] / u16 | Footer discriminator. |
| segment_index | u32 | Sanity check. |
| ext_id | u64 | Mirrors the header; allows footer-only inspection to know cohort. |
| durable_bytes | u64 | Byte offset (relative to base_offset) known to be durable at seal time. |
| record_count | u64 | Total records at seal time. |
| last_record_timestamp | i64 | Timestamp of the final record. |
| coordinator_watermark | u64 | Operation-log sequence applied when sealing. |
| flush_failure_flag | u8 | 1 if flush failure backpressure was active when sealing, otherwise 0. |
| reserved | [u8; 7] | Future flags (compression, encryption). |

Because sealing happens once, enriching the footer preserves append-only semantics.

## current.sealed Pointer
- `segments/current.sealed` stores `{segment_id, coordinator_watermark, durable_bytes}` and is rewritten via a `TempFileGuard` + rename + fsync sequence to stay crash-safe on all platforms.
- Pointer writes happen only after segment data, footer, and fsync succeed.
- Recovery loads the pointer first and trusts the durable_bytes/watermark only if the referenced segment still exists; otherwise a warning is emitted and the pointer is ignored.

## Recovery Workflow
1. Read the current-sealed pointer to obtain the latest sealed segment index and global watermark.
2. Load that segment's header/footer to reconstruct global counters (next segment index, total record count, flush-failure flag).
3. Enumerate newer files, if any, scanning only the active tail segment to handle partial writes.
4. Rebuild in-memory state (SegmentFlushState, catalog entries) directly from header/footer data; durability cursors and flush queues are no longer persisted.
5. Replay operation-log entries starting at `coordinator_watermark + 1`.

## Interaction with the Manifest
- Tier1/Tier2 residency continues to live in the manifest log; sealed segment metadata becomes authoritative via headers/footers.
- Manifest entries should include the segment ext_id so warm/cold tiers remain aligned with the cohorts recorded on disk.
- Older manifest entries can be compacted once the corresponding sealed segments and pointer file are durable and acknowledged by the warm tiers.

## Implementation Summary
1. Added `ext_id` plumbing to segments, record headers, manifest entries, and `SealSegment` log records. Defaults remain zero until callers propagate real IDs.
2. `Segment::seal` now captures the durable byte position, coordinator watermark, and the flush failure flag observed at seal time.
3. `Layout::store_current_sealed_pointer` writes the pointer file atomically; recovery warns but proceeds if the pointer references a missing or stale segment.
4. Recovery relies exclusively on headers/footers plus the pointer to rebuild catalog state and durability cursors.
5. Manifest snapshots/log replay honour the enriched metadata so Tier1/Tier2 state stays aligned with on-disk segments.
6. Documentation and tests were refreshed to describe the new metadata contract and pointer semantics.

## Verification
- `aof2::segment` tests cover header/footer round-trips, trusted footer validation failures, and ext_id persistence.
- `aof2::fs` tests exercise pointer round-tripping, invalid magic detection, and atomic replacement behaviour.
- `aof2::tests` integration scenarios verify pointer-driven recovery (watermark propagation, missing/mismatched pointers) and `set_current_ext_id` plumbing.
- `tests/aof2_manifest_log.rs` ensures `SealSegment` records round-trip the new metadata across the log reader/writer.

## Open Questions
- Do we want to store additional rolling metrics (e.g. cumulative bytes appended) in the footer for observability?
- How do downstream services consume ext_id (metrics, tracing, manifest replay) so tooling stays consistent?
