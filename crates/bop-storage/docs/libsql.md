# libsql Integration

This document describes how `bop-storage` integrates with libsql through its
virtual WAL (VWAL) interface and a shared page cache. The goals are:

- Provide a WAL implementation that is format-compatible with SQLite/libsql
  while fitting our chunk-based storage pipeline.
- Expose a VFS that keeps hot pages in a global cache so multiple page stores
  (or databases) can share memory safely.
- Support advanced workflows such as branching (forks) and snapshot export
  without duplicating large amounts of data.

## Virtual WAL Overview

libsql embeds SQLite’s WAL format: every frame writes a full database page,
prefixed by a 24-byte frame header containing page number, post-frame database
size, salts, and checksums. We wrap that format inside our chunk layout:

- `ChunkHeader` (`wal_mode = Libsql`) identifies the WAL variant.
- Optional `SuperFrame::Libsql` stores global metadata (page count,
  change-counter, salts, `mxFrame`).
- Each frame starts at a page boundary and stores the standard SQLite frame
  header plus raft metadata (`term`, `index`). The payload is always exactly one
  page (e.g. 4096 B), matching the VFS contract.

Because headers are page-aligned, readers can rediscover every frame by scanning
pages. The record id packs `{page_number, 16-byte offset, span}` so WAL frames
remain strictly ordered and easy to address.

### WAL Writer Highlights

- `LibsqlWalAdapter` converts libsql VFS callbacks into chunk frames. It
  accepts the standard frame metadata and writes it via `WalWriter::append_libsql`.
- The WAL writer reuses the same slab pool and direct I/O mechanics as the
  raft WAL; only the frame header differs.
- Super-frames summarise the chunk (post-checkpoint page count, salts) so we
  can resume context quickly after a crash.

### WAL Recovery

1. Parse chunk header (`wal_mode` must be libsql) and super-frame.
2. Iterate pages; for each frame header, reconstruct the frame and deliver it to
   libsql’s connection or VFS implementation.
3. Checksums and salts are validated to detect torn writes.

## libsql VFS Design

Our VFS sits on top of the page store and WAL pipeline. Key features:

- **Global concurrent page cache** – We maintain a pool of cached pages shared
  across all libsql-backed page stores. Each cached entry records the logical
  page id, generation, and a lease counter. This reduces memory footprint when
  multiple databases or forks share a large working set.
- **Write-through semantics** – All writes go through the WAL; the page cache is
  updated eagerly so readers can see the latest data without reloading from the
  WAL chunk on every access.
- **Branch-aware** – Because chunks are immutable and generation IDs capture
  snapshots/forks, the VFS can map cache entries to specific generations. A
  fork reuses the parent cache entries until it writes new data, at which point
  it publishes a new generation.
- **Concurrent safety** – The cache uses lock-free or sharded locks to allow
  multiple readers and writers. Page buffers are reference-counted so they can
  be shared across threads without copying.

### Asynchronous Operations via `generator-rs`

SQLite’s VFS hooks are synchronous, but we often need to overlap disk and
network operations. We layer stackful coroutines (via the `generator` crate) on
top of the VFS to enable pseudo-async behaviour:

```
connection thread ──► generator::Gn::new(|| {
    loop {
        match request {
            ReadPage(id, responder) => {
                cache.get_or_fetch(id, || yield FetchIo(id));
                responder.send(page);
            }
            WritePage(id, bytes) => {
                wal.append(bytes);
                cache.update(id, bytes);
            }
        }
    }
})
```

The coroutine yields `FetchIo` requests to an executor that performs blocking
I/O on worker threads, then resumes the coroutine with the fetched page. This
approach keeps the VFS API compatible with SQLite while enabling cooperative
multitasking without a full async runtime.

### VFS Call Sequence

```
client thread          VFS coroutine             Cache/IO workers
      │                      │                          │
      │ xRead(page)          │                          │
      ├────────────────────► │ lookup(page)             │
      │                      │ ├ cache hit? ──► return  │
      │                      │ └ cache miss             │
      │                      │    yield FetchIo(page)   │
      │                      │◄─────────────────────────┤ read chunk
      │                      │ resume -> insert cache   │
      │◄─────────────────────┤ return page              │
```

Writes follow a similar path: the coroutine appends to the WAL (yielding if the
slab pool blocks), then updates the cache entry. Fork-aware generation checks
ensure the cache entry matches the caller’s generation before reuse.

## Snapshot & Fork Interactions

- Creating a snapshot/fork requires only materialising a new manifest/superblock
  (see `page_store_design.md`). The VFS simply repoints to the new manifest; the
  page cache can reuse existing entries as long as they match the target
  generation.
- Checkpointing a fork writes new chunks and updates that fork’s manifest.
  Global cache entries referencing the old generation stay valid for the parent
  fork; new generations receive new cache slots only for pages that changed.

## Flexibility & Extensibility

- **Page size independence** – While we default to 4 KiB pages, the chunk layout
  records the page size, allowing future configurations (e.g. 8 KiB) by bumping
  the chunk version.
- **Multi-tenant caching** – Because the cache is keyed by
  `(page_store_id, generation, page_id)`, different databases can opt-in to a
  shared cache or keep isolation if desired.
- **Streaming & replication** – WAL chunks are immutable and self-describing;
  they can be shipped over the network and applied on a follower with the same
  replay logic. Snapshot replication boils down to copying manifests and the
  referenced chunks.
- **Coroutine-friendly API** – All VFS primitives (`xRead`, `xWrite`,
  `xSync`) are thin shims over coroutine calls, making it straightforward to
  introduce new storage backends (e.g. S3) without rewriting the SQLite-facing
  layer.

## Summary

By aligning with SQLite’s WAL format and wrapping it in a chunked storage layer,
we get a libsql-compatible WAL that works seamlessly with our direct I/O
pipeline. The global page cache keeps memory usage in check across multiple page
stores, while immutable chunks plus manifest-based snapshots make forks and
rollbacks inexpensive. This design offers the flexibility to support diverse
workloads—from raft-backed state machines to libsql tenants—using a single
storage substrate.
