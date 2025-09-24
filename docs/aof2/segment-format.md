# AOF2 Segment & Manifest Format

## Summary
This specification describes the on-disk formats for AOF2 segment files and the manifest log so engineers can reason about compatibility, recovery, and tooling. It supersedes scattered notes in `aof2_manifest_log.md` and `aof2_segment_metadata_plan.md` by providing a single schema reference.

## Goals
- Capture record layouts and checksum strategy for both segment files and manifest chunks.
- Document versioning rules so future schema changes remain backward compatible.
- Provide quick lookup tables for tooling and test authors.

## Non-Goals
- Explaining runtime scheduling or coordinator behaviour (see `architecture.md`).
- Operational runbooks or CLI usage (covered elsewhere).

## Segment File Layout
Segments are memory-mapped files with the following layout:

```
+------------------------------+
| SegmentHeader                |
+------------------------------+
| Record[0] header + payload   |
+------------------------------+
| ...                          |
+------------------------------+
| SegmentFooter                |
+------------------------------+
```

### `SegmentHeader`
| Field | Type | Notes |
| --- | --- | --- |
| `magic` | `u64` (`0x414F46325345474D`, "AOF2SEGM") | Identifies segment files. |
| `version` | `u32` (current `2`) | Bumped when record encoding changes. |
| `stream_id` | `u64` | Logical stream. |
| `segment_id` | `u64` | Monotonic per stream. |
| `creation_ts_ms` | `u64` | UTC milliseconds, informs retention. |
| `reserved` | `u64[2]` | Must be zero; used for alignment/future fields. |

### Records
Each record is written atomically and covered by checksums.

```
struct SegmentRecordHeader {
    u32 payload_len;
    u32 record_crc32;
    u64 logical_offset;
    u64 append_ts_ms;
    u32 flags; // bit 0 = snapshot boundary, bit 1 = control frame
}
```

Payload is opaque to the segment store; higher layers interpret it as application frames. Logical offsets advance monotonically and correspond to manifest offsets.

### `SegmentFooter`
| Field | Type | Purpose |
| --- | --- | --- |
| `record_count` | `u64` | Number of records appended. |
| `durable_bytes` | `u64` | Bytes persisted (used during recovery). |
| `segment_crc64` | `u64` | crc64-nvme over header + records. |
| `ext_id` | `u64` | External identifier (e.g., Raft index). |
| `flags` | `u64` | Bit 0 = sealed, Bit 1 = compacted, Bit 2 = partial. |

## Manifest Log Schema
The manifest log is chunked; each chunk is a fixed-capacity mmap described by the header below.

### `ChunkHeader`
| Field | Type | Notes |
| --- | --- | --- |
| `magic` | `u64` (`0x414F46324D4C4F47`, "AOF2MLOG") | Identifies manifest chunks. |
| `version` | `u32` (`1`) | Bump on incompatible header changes. |
| `header_len` | `u32` | Size of header in bytes. |
| `chunk_index` | `u64` | Sequential per stream. |
| `base_record` | `u64` | Ordinal of first record in chunk. |
| `tail_offset` | `u64` | First unwritten byte (for writers). |
| `committed_len` | `u64` | Bytes fsynced and safe to replay. |
| `chunk_crc64` | `u64` | crc64-nvme over [header_len, committed_len). |
| `flags` | `u64` | Bit 0 sealed, Bit 1 uploaded to Tier 2. |

### Record Header
```
struct ManifestRecordHeader {
    u16 record_type;
    u16 flags; // reserved
    u32 payload_len;
    u64 segment_id;
    u64 logical_offset;
    u64 timestamp_ms;
    u64 payload_crc64;
}
```

### Record Types
### Reason Codes
| Code | Context | Meaning |
| --- | --- | --- |
| 0 | CompressionFailed / UploadFailed / HydrationFailed | Transient error; eligible for automatic retry. |
| 1 | CompressionFailed | Permanent codec error (corrupt segment). |
| 2 | UploadFailed | Authentication/authorization failure; requires operator action. |
| 3 | UploadFailed | Bucket or network unavailable. |
| 4 | HydrationFailed | Tier 2 object missing; triggers repair workflow. |

Reason codes align with `TieredFailureReason` in `crates/bop-aof/src/store/errors.rs`. Future codes must update this table.

| Name | Id | Payload | Notes |
| --- | --- | --- | --- |
| `SegmentOpened` | 0 | `{ u64 base_offset; u32 epoch; }` | New segment admitted to Tier 0. |
| `SealSegment` | 1 | `{ u64 durable_bytes; u64 segment_crc64; u64 ext_id; u64 coordinator_watermark; u8 flush_failure; }` | Segment sealed; captures durability metadata. |
| `CompressionStarted` | 2 | `{ u32 job_id; }` | Tier 1 compression enqueued. |
| `CompressionDone` | 3 | `{ u64 compressed_bytes; u32 dictionary_id; }` | Warm artifact durable. |
| `CompressionFailed` | 4 | `{ u32 reason_code; }` | Background compression failed. |
| `UploadStarted` | 5 | `{ u16 key_len; bytes[key_len]; }` | Tier 2 upload initiated. |
| `UploadDone` | 6 | `{ u16 key_len; u16 etag_len; bytes[key_len + etag_len]; }` | Upload finished; includes object key + ETag. |
| `UploadFailed` | 7 | `{ u16 key_len; u32 reason_code; }` | Upload failure for retries. |
| `Tier2DeleteQueued` | 8 | `{ u16 key_len; }` | Delete requested for Tier 2 object. |
| `Tier2Deleted` | 9 | `{ u16 key_len; }` | Delete completed. |
| `HydrationStarted` | 10 | `{ u64 source_tier; }` | Hydration began from Tier 1 or Tier 2. |
| `HydrationDone` | 11 | `{ u64 resident_bytes; }` | Hydration successful; Tier 0 residency restored. |
| `HydrationFailed` | 12 | `{ u32 reason_code; }` | Hydration failure. |
| `LocalEvicted` | 13 | `{ u64 reclaimed_bytes; }` | Tier 0 eviction finished. |
| `SnapshotMarker` | 14 | `{ u64 chunk_index; u64 chunk_offset; u64 snapshot_id; }` | Associates catalog snapshot with log position. |
| `CustomEvent` | 15 | `{ u32 schema_id; u16 blob_len; bytes[blob_len]; }` | Extension point for future tier events. |

## Versioning Guidance
- New record types must append to the table with unique ids; readers ignore unknown ids but record `flags` for diagnostics.
- Header `flags` field remains zero until we introduce negotiated capabilities; toggling bits requires corresponding replay guards.
- Payload schemas follow little-endian encoding and must include length prefixes for variable data.

## Checksums & Validation
- Segment payload CRC32 detects corruption per record; segment footer CRC64 protects the entire file.
- Manifest log uses CRC64 both at chunk and record payload level; replay must verify before mutating state.
- Tier 2 uploads include object metadata (crc, record counts) to validate after download.

## Snapshot Files
Snapshots serialise the manifest map along with the associated chunk index/offset. The file is versioned separately (`snapshot_version = 3`) and contains:
- Header: `magic = 0x414F4632534E4150` ("AOF2SNAP"), `version`, `entry_count`, `chunk_index`, `chunk_offset`.
- Entries: fixed-size structures keyed by `segment_id`, containing residency state, ext ids, and Tier 2 object keys.

## Tooling Notes
- `manifest-inspect` utility consumes this schema to render human-readable tables.
- Python integration tests (`aof2_python_test_runner.md`) parse manifest chunks directly; update their schemas when new record types appear.

## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-25 | Added reason-code table per SME request | Docs Team |

## Decision Log
| Date | Decision | Owner | Notes |
| --- | --- | --- | --- |
| 2024-06-11 | Adopt crc64-nvme for manifest payloads | Storage Platform Architecture | Selected for hardware-accelerated support. |
| 2024-09-02 | Reserve `CustomEvent` record type for future features | Storage Platform Architecture | Enables feature-gating without schema churn. |
| 2025-02-14 | Snapshot version bumped to 3 with residency gauge | Reliability Architecture | Required for TieredObservabilitySnapshot integration. |

## Related Resources
- [Milestone 2 Review Notes](../bop-aof/review_notes.md)
- [Architecture](architecture.md)
- [Maintenance Handbook](maintenance-handbook.md)
- [Testing & Validation](testing-validation.md)
- [Tiered Operations Runbook](runbooks/tiered-operations.md)
- [Roadmap](roadmap.md)

