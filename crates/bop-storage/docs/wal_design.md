# WAL Design

This document captures the current write-ahead-log designs supported by
`bop-storage`. Two variants share the same chunk/segment plumbing but differ in
how they encode logical records:

1. **Record-Based WAL** – used by the raft log pipeline.
2. **libsql WAL** – compatible with the libsql virtual WAL interface.

Both designs write to the same physical artifact: a *chunk* made of 4 KiB pages
that is persisted via direct I/O. Each chunk starts with a `ChunkHeader`, may
contain an optional super-frame with global metadata, and then stores a
sequence of records. Records always start on a page boundary; payloads may span
multiple pages.

## Shared Chunk Structure

Every persisted segment begins with:

| Field | Size | Notes |
|-------|------|-------|
| `magic` | 4 B | ASCII `BOPC` |
| `version` | 2 B | Currently `1` |
| `wal_mode` | 2 B | `0 = Record`, `1 = Libsql` |
| `slab_count` | 2 B | Number of pages in the chunk |
| `slab_size` | 4 B | Page size in bytes (`4096`) |
| `sequence` | 8 B | Monotonic chunk sequence |
| `generation` | 8 B | Epoch / checkpoint generation |
| `super_frame_bytes` | 4 B | Length of optional super frame payload |
| `reserved` | 4 B | Currently zero |

If `super_frame_bytes > 0`, the following bytes encode global metadata. Two
encodings exist:

- **Base super-frame** (24 B): `{snapshot_generation, wal_sequence, bitmap_hash}`.
- **libsql super-frame** (48 B): base fields plus `{db_page_count, change_counter,
  salt[2], schema_cookie, mx_frame}`.

After the header (and optional super-frame), records follow back-to-back with
zero padding added only when a new record header would otherwise straddle a
page boundary.

## Record-Based WAL

Designed for raft entries. Each record consists of a header and a payload, and
every header is guaranteed to start at a page boundary. Payloads can span
multiple pages without further framing.

### Record Identifier

A 64-bit `record_id` packs placement metadata so IDs are naturally ordered:

```
record_id = (page_number << 16) | (offset_slots << 8) | span_minus_one
```

- `page_number` (48 bits): first 4 KiB page containing the record.
- `offset_slots` (8 bits): header offset in 16-byte increments within the page.
- `span_minus_one` (8 bits): number of pages touched by the record minus one.

This scheme yields over 1 EiB of addressable log space while keeping IDs
monotonic even when multiple records share a page.

### Record Header

The header occupies a fixed 16 B slot and stores:

| Field | Size | Notes |
|-------|------|-------|
| `term` | 8 B | Raft term |
| `index` | 8 B | Raft log index |
| `entry_len` | 4 B | Payload length in bytes |
| `checksum` | 4 B | CRC32 over the payload |

The header is followed immediately by the payload. Prior to writing a new
record the writer checks remaining space in the current page; if fewer than 16
bytes remain, the page is padded with zeros and the header is emitted at the
next page boundary.

### Writer Behaviour

- Batches records into chunks using direct I/O slabs.
- Maintains a `SuperFrame::Base` summarising the chunk (snapshot generation,
  WAL sequence and roaring bitmap hash) – this is written once per chunk.
- Applies raft metadata (`term`, `index`) per record; no per-page framing is
  required.

### Recovery

1. Parse the chunk header and super-frame to validate chunk provenance.
2. Iterate page by page:
   - Read the 16 B header at `offset_slots * 16`.
   - Verify the checksum if desired.
   - Advance by `ceil((header_len + entry_len)/PAGE_SIZE)` pages using the
     embedded span.
3. Replay entries in order of `record_id`.

Padding pages always contain zeros; any zero page between records indicates
alignment padding and is skipped.

## libsql WAL

The libsql integration reuses the same chunk foundation but records the metadata
required by the libsql virtual WAL interface.

### Frame Structure

Each entry header keeps the fields SQLite expects:

| Field | Size | Notes |
|-------|------|-------|
| `page_no` | 4 B | SQLite page number |
| `db_size_after_frame` | 4 B | Database size (in pages) after applying frame |
| `directory_value` | 8 B | Packed checksum/len/zstd bits |
| `salt[2]` | 8 B | Frame salt copied from libsql VFS |
| `frame_checksum[2]` | 8 B | Frame checksum pair |
| `payload_len` | 4 B | Bytes of payload |
| `reserved` | 4 B | Zero |
| `raft.term/index` | 16 B | Raft metadata |

Headers still start at page boundaries; the payload follows immediately. Large
frames spill into subsequent pages with no additional framing—libsql consumers
reassemble payload by reading `payload_len` bytes starting at the header’s
page/offset.

### Writer Behaviour

- Calls `append_libsql` with the frame metadata supplied by the libsql VFS.
- Writes a `SuperFrame::Libsql` containing the WAL-wide metadata required by
  libsql (page count, salts, schema cookie, `mx_frame`).
- Uses the same padding rule as the raft WAL: if a header would straddle a
  page, pad the remainder of the page with zeros first.

### Recovery

1. Parse chunk header (expect `wal_mode = 1`) and libsql super-frame.
2. Walk pages sequentially, reading each header and payload based on
   `payload_len`.
3. Rebuild libsql’s WAL index or stream frames to the libsql VFS as required.

## Invariants and Validation

- Headers are **never** split across pages.
- Zero padding only appears between records; no record contains embedded zeros
  unless the payload itself has them.
- Chunk headers and super-frames are validated before any replay.
- Checksums (raft payload CRC32 or libsql frame checksums) can be used to detect
  torn writes during recovery.
- Record IDs remain monotonic, enabling O(log n) binary search or partial
  truncation via simple comparisons.

