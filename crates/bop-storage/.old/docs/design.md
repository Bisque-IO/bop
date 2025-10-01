# bop-storage Design Overview

This document summarizes the storage architecture; see
`wal_design.md`, `page_store_design.md`, and `libsql.md` for deep dives.

## Goals
- Unified chunk format for WAL and page-store artifacts.
- Strict direct-I/O path with pooled slabs; avoid `mmap`.
- Support raft (record-based WAL), libsql WAL, snapshots, forks, and copy-on-write.
- Keep WAL append hot; all mutations to the base store happen via checkpoints.
- Scale across local disks and S3 using deterministic layouts and manifests.

## Architecture

```
┌─────────────┐    append     ┌─────────────┐
│ Raft / libsql│─────────────►│  WAL Chunks │
└─────┬───────┘                └─────────────┘
      │ checkpoint merge                     │ replay
      ▼                                       ▼
┌─────────────┐    manifest    ┌─────────────┐
│ Page Store   │◄──────────────►│ Superblock │
└─────────────┘                 └─────────────┘
```

## Chunk & Record Layout
- `ChunkHeader` (magic, version, wal_mode, slab_count, slab_size, sequence,
  generation, super_frame_bytes).
- Optional super-frame: base (24 B) or libsql (48 B) metadata.
- Record-based WAL: 16 B header `{term,index,entry_len,checksum}` + payload.
- libsql WAL: SQLite frame header (24 B) + page payload; includes raft term/index.
- Record IDs pack `{page_number, offset_slots, span_minus_one}` → monotonic keys.
- Headers always start at page boundaries; payload may span pages.

## Page Store & Checkpoints
- Page-store chunks use the same header; each page stores data + `DirectoryValue` footer.
- Checkpoints: scan WAL → apply updates → emit new chunks → flush → update manifest/superblock → truncate WAL.
- Manifests track chunk generations; superblock points to active manifest & WAL sequence.
- Snapshots/forks copy manifests into new namespaces; chunks are immutable so copy-on-write is cheap.
- Recovery: read superblock → load manifest → replay WAL records `> wal_sequence`.

## libsql Integration
- `LibsqlWalAdapter` wraps libsql VFS callbacks, emitting libsql-compatible frames using our chunk infrastructure.
- Global concurrent page cache keyed by `(page_store, generation, page_id)`; supports shared caches across forks/databases.
- Stackful coroutines (`generator-rs`) layer pseudo-async behaviour over VFS calls; allows overlapping IO without full async runtime.

## Direct I/O & Slab Pool
- Slabs sized by config; alignment enforced per platform.
- Writers acquire slabs, copy payloads into page-aligned buffers, and flush via direct I/O.
- Readers borrow published slabs or load historical chunks into read slabs.

## StorageFleet Management Layer
- **StorageFleet** – top-level coordinator hosting the async runtime, blocking IO pool, and FlushController; owns lifecycle for every StoragePod.
- **StoragePod** – encapsulated WAL + PageStore pair with its manifest namespace, checkpoint pipeline, and Wal/flush state. Pods borrow the fleet pools for fsync, manifest writes, and superblock swaps.
- **Runtime split** – async runtime drives orchestration (WAL ingest, checkpoint scheduling, recovery) while a bounded blocking pool handles disk IO via `spawn_blocking`.
- **FlushController** – serializes durability transitions: slab flush → chunk fsync → manifest append → superblock swap across pods; exposes back-pressure signals to writers.
- **Extensibility hooks** – StoragePods register async commands (append_wal, run_checkpoint, fetch_page) and health metrics with the fleet; fleet routes external API calls to the correct pod.
- **Failure handling** – fleet supervises pod tasks, retries recoverable IO faults, escalates fatal errors, and records restart metadata for rapid recovery.

## Task Plan
| Step | Description | Status |
|------|-------------|--------|
| P0 | Chunk format + directories | Completed |
| P1 | Record-based WAL writer/parser | Completed |
| P2 | Checkpoint pipeline + manifest | Completed |
| P3 | libsql VFS + global cache | Completed |
| P4 | Snapshot/fork tooling & GC | Planned |
| P5 | S3/object-store integration & metrics | Planned |
| P6 | StorageFleet management layer | Planned |

## Task Tracker
| ID | Task | Milestone | Owner | Status | Notes |
|----|------|-----------|-------|--------|-------|
| T0 | Scaffold crate & CI | M0 | TBD | Completed | `cargo test -p bop-storage` passes |
| T1 | Chunk parser + DirectoryValue updates | M0 | Completed | Unified chunk parsing + raft metadata |
| T2 | Direct I/O slab pool | M1 | Completed | Alignment enforced; pooled writers/readers |
| T3 | Record WAL pipeline & tests | M1 | Completed | Raft record support + end-to-end tests |
| T4 | Page-store checkpoint & manifest | M2 | Completed | Manifest delta log, checkpoint coroutines, crash recovery tests |
| T5 | libsql WAL + VFS + coroutine cache | M3 | Completed | Chunk encoder integration, global cache, coroutine VFS hooks |
| T6 | Snapshot/fork CLI + retention GC | M3 | In Progress | Namespace helpers and generation refcounts in manifest |
| T7 | S3 layout & replication tooling | M4 | Planned | Object-store manifests, range fetch |
| T8 | StorageFleet scaffold & API surface | M3 | Completed | Fleet drives runtime lifecycle, shared flush metrics, pod registry/teardown |
| T9 | FlushController integration | M3 | Completed | Ordered stage queue with back-pressure, failure propagation, and coverage tests |
| T10 | StoragePod recovery & health reporting | M3 | Completed | Metadata persistence, flush waiters, snapshot publish/health transitions |

## Implementation Prompt

When continuing development, seed the coding session with the following prompt:

```
You are implementing bop-storage according to the docs in crates/bop-storage/docs.

Focus areas:
1. Stand up the StorageFleet manager:
   - Define StoragePod registration APIs and shared runtime/blocking pools.
   - Expose pod lookups, health reporting, and graceful shutdown hooks.

2. Integrate FlushController with StoragePods:
   - Schedule fsync/manifest/superblock work on the blocking pool via the controller.
   - Provide back-pressure and failure reporting to WAL writers.

3. Extend StoragePods with recovery & snapshot plumbing:
   - Persist per-pod restart metadata and health metrics.
   - Prepare hooks for forthcoming snapshot/fork commands.

Guidelines:
- Keep record and chunk formats aligned with docs/wal_design.md.
- All mutations flow through checkpoints; WAL append remains fast.
- Update Task Tracker statuses/notes after each working session.
```

## References
- `docs/wal_design.md` – detailed WAL formats.
- `docs/page_store_design.md` – checkpoint pipeline, snapshots, forks.
- `docs/libsql.md` – VFS integration and cache design.
