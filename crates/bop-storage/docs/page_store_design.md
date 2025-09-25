# Page Store Design

This document explains the storage layer that backs bop’s sparse page store and
how write-ahead logging feeds it. The key concepts are:

1. **Chunks** – contiguous 4 KiB-aligned files persisted via direct I/O.
2. **Records** – logical updates captured in the WAL.
3. **Checkpoints** – merge operations that fold WAL records into durable page
   state. *Checkpoints are the only mutation path for the base store.*

## Storage Overview

```
┌─────────────────────┐
│  WAL Chunks (log)   │  append-only, record oriented
└────────┬────────────┘
         │ checkpoint merges (read + apply)
┌────────▼────────────┐
│ Page Store Chunks   │  indexed by logical page id
└─────────────────────┘
```

The live dataset is a set of chunks containing 4 KiB pages. Each chunk begins
with a `ChunkHeader` identical to the WAL format (see `docs/wal_design.md`).
Page payloads follow the header directly; there is no per-page metadata beyond
what is embedded in the `DirectoryValue` footer. Lookups rely on a roaring
bitmap directory to determine which pages exist and how to fetch them.

## Chunk Layout

Page-store chunks share the same header/super-frame encoding as the WAL. When a
checkpoint emits a new chunk the super-frame records:

- `snapshot_generation` – monotonically increasing generation stamp.
- `wal_sequence` – highest WAL record incorporated.
- `bitmap_hash` – checksum of the roaring bitmap snapshot (used for integrity).

Individual pages are written sequentially. Each page stores:

```
[4 KiB payload] [DirectoryValue footer]
```

The footer carries checksum, logical length, compression flag and reserved bits.
Because WAL checkpoints are the only mutation path, the page-store never
mutates in-place: a checkpoint writes an entirely new chunk (or set of chunks)
and then atomically swaps the generation reference.

## WAL Interaction

- The WAL captures every logical update as a record (raft-style) or a libsql
  frame. Records include the full page body plus metadata.
- The `super_frame` at the start of each WAL chunk records the `wal_sequence`
  value that will become durable once the chunk is applied.
- The roaring bitmap tracks which logical pages exist. WAL replay updates the
  bitmap in-memory; checkpointing snapshots it and embeds the hash in the
  chunk’s super-frame.

Refer to `docs/wal_design.md` for precise record encodings.

## Checkpoint Pipeline

1. **Scan WAL** since the last durable `wal_sequence`. Collect records grouped
   by logical page id.
2. **Apply** updates to an in-memory image (page cache or staging buffers).
3. **Emit new chunks**:
   - Allocate a direct-I/O slab.
   - Write the chunk header with the next generation (`snapshot_generation+1`).
   - Serialize the current roaring bitmap hash into the super-frame.
   - Write pages sequentially, appending `DirectoryValue` footers.
4. **Sync and publish**:
   - `fdatasync`/`FlushFileBuffers` the chunk files.
   - Persist a small superblock pointing to the new generation and the latest
     `wal_sequence`.
   - Only after the superblock update is durable may the WAL truncate the
     applied records.

Because the checkpoint is the only mutator, the page store enjoys the same
durability guarantees as the WAL: either the new generation becomes visible (and
the WAL can be truncated) or the previous generation remains intact and replay
continues on restart.

### Checkpoint Call Sequence

```
┌────────────────┐      ┌────────────────┐      ┌────────────────┐
│   Checkpointer │      │   WAL Reader   │      │  Page Writer   │
└──────┬─────────┘      └────────┬───────┘      └────────┬───────┘
       │ scan_since(seq)         │                       │
       ├────────────────────────>│                       │
       │                         │stream_records()       │
       │                         ├──────────────────────>│ apply(record)
       │                         │                       │ buffer page
       │                         │                       │
       │                         │                       │flush_chunk()
       │                         │                       ├───────────────►
       │ persist_manifest(gen+1) │                       │ write header
       ├────────────────────────>│                       │ write pages
       │ update_superblock(seq+) │                       │ fsync
       ├───────────────────────────────► manifest store  │
       │ truncate_wal(seq+)      │                       │
       └────────────────────────>│                       │
```

In code, the checkpointer issues asynchronous tasks so page flushes can overlap
with WAL streaming. We implement the async orchestration with
[`generator-rs`](https://crates.io/crates/generator) stackful coroutines: each
phase suspends when it would block on I/O, allowing other coroutines to make
progress without embedding a full async runtime.

## Snapshot Lifecycle

- **Active manifest** – The superblock references a manifest describing the
  latest generation of each chunk. The manifest can be a compact snapshot (one
  entry per chunk) or a delta log (append-only entries for chunks rewritten by
  each checkpoint). Recovery walks the manifest to select, for every chunk id,
  the highest generation ≤ the manifest’s generation.
- **Snapshot exports** – To capture a point-in-time snapshot without truncating
  the WAL, freeze the current manifest/superblock pair and retain the WAL
  sequence. Consumers can recreate the snapshot by loading that manifest and
  ignoring later WAL entries.
- **Retention** – Old manifests remain read-only; garbage collection removes
  chunk generations no manifests reference.

## Forks and Branches

A *fork* is a snapshot that may diverge with new WAL activity. To create one:

1. Choose a source generation (`G`) and `wal_sequence`.
2. Copy (or logically reference) the manifest entries for `G` into a new
   namespace, e.g. `forks/{id}/manifest`.
3. Write a fork-specific superblock pointing at that manifest and sequence.
4. Subsequent WAL/checkpoints for the fork produce new chunk generations and
   append delta entries to the fork manifest; the parent manifest remains
   untouched.

Forks allow multiple timelines to coexist while sharing immutable chunk data.
Only chunks modified after the fork consume additional space.

### Fork Creation Sequence

```
Parent manifest        Fork manager             Storage backend
      │                      │                          │
      │  snapshot_manifest() │                          │
      ├─────────────────────►│                          │
      │                      │ write fork manifest      │
      │                      ├─────────────────────────►│
      │                      │ write fork superblock    │
      │                      ├─────────────────────────►│
      │                      │                          │
      │                      │  spawn WAL pipeline (generator)
      │                      └───────────────────────────────► new fork WAL
```

Both checkpointing and fork management share the same coroutine-friendly APIs:
`ManifestWriter::append_delta`, `CheckpointWriter::flush_chunk`, and
`SuperblockWriter::commit`. Coroutines resume only after their underlying I/O
completes, letting us interleave WAL reads, chunk writes, and manifest updates
without blocking the executor thread pool.

## Recovery

1. Read the superblock to determine the active generation and last durable
   `wal_sequence`.
2. Load the roaring bitmap snapshot embedded in the chunk(s) or recompute it by
   scanning pages.
3. Replay WAL records with `record_id > wal_sequence` to rebuild the in-memory
   state.

Since records are strictly ordered the replay process is deterministic. Any
chunk header or super-frame checksum mismatch aborts recovery and leaves the
prior generation intact.

### Recovery Flow Diagram

```
┌────────────┐   read   ┌─────────────────┐   select  ┌────────────────┐
│ Superblock │────────► │ Manifest Reader │──────────►│ Chunk Registry │
└────┬───────┘          └───────┬─────────┘           └──────┬─────────┘
     │                          │ load entries               │ map chunk
     │                          │                            │
     │          replay WAL      │                            │
     └─────────────────────────►│                            │
                                │ validate bitmap hash       │
                                └───────────────────────────►│
```

Recovery uses the same coroutine primitives: the WAL replayer yields after each
chunk to allow the chunk registry to load metadata lazily. This keeps restart
latency predictable even for multi-terabyte stores.

## Invariants

- Page-store chunks are immutable once written; garbage collection only involves
  deleting obsolete generations.
- Checkpoints must never skip a WAL sequence – they either consume a suffix or
  stop short of an incomplete record.
- DirectoryValue footers always correspond to the page bytes preceding them.
- WAL truncation happens *after* the new chunk generation and superblock are
  durable.

This design keeps the hot path (append) simple, enables predictable recovery,
and allows space reclamation via generation-level garbage collection.
