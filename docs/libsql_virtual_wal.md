# libsql Virtual WAL Frame Cache

The `LibsqlVirtualWal` wrapper keeps a small, in-memory index of recently
written frames so bop can answer `x_find_frame`/`x_read_frame` callbacks without
touching disk. Internally we maintain a per-WAL cache keyed by frame id and
also project each page into the global `PageCache`. The shared cache makes WAL
frames available to other storage components (e.g. manifest catch-up or future
page-serving APIs) without requiring another copy.

## Responsibilities

- Surface every frame observed during `x_frames` to the storage hook so higher
  layers can react to WAL traffic.
- Retain frame payloads in memory so readers can materialize the requested
  bytes directly from the cache.
- Answer `x_find_frame` using the page-to-frame map and `x_read_frame` using the
  local frame store. The stored bytes are shared with the global `PageCache` so
  other subsystems can reuse them.
- Reset all cached state when libsql issues a checkpoint.

## Global page cache

`VirtualWalConfig` optionally accepts an `Arc<PageCache<PageCacheKey>>`. When
configured the WAL publishes every stored page under the
`PageCacheKey::libsql_wal(wal_id, pgno)` namespace. Eviction in the per-WAL
cache automatically removes the page from the shared cache when the evicted
frame was the latest copy for that page. Checkpoints clear both caches.

The page cache exposes hit/miss/insert/eviction counters and supports user
supplied observers. Storage services can hook these callbacks to persist warm
entries, expose metrics, or perform background warming.

`LibsqlVirtualWal::page_cache_metrics` returns a `PageCacheMetricsSnapshot` so
callers can surface shared cache diagnostics alongside the WAL-specific in-memory
index statistics.

## Concurrency guarantees

SQLite serializes calls to `xFrames` for a database handle, but the virtual WAL
defensively tolerates concurrent writers to simplify testing. Frame metadata is
stored in `DashMap` instances so read-side lookups can run without coordination.
An internal `Mutex<VecDeque<_>>` records insertion order and total byte usage to
support deterministic eviction without blocking readers.

## Memory pressure

`VirtualWalConfig::frame_cache_bytes_limit` allows operators to bound the cache.
When the limit is set the implementation evicts the oldest frames until the
total cached payload fits under the threshold. Frames that do not fit within
the configured budget are not cached. The cache remains unbounded when the
limit is `None`.

The eviction policy only prunes cached entries; older frame ids remain
addressable until they are explicitly removed. This provides predictable
behaviour for long-running readers while still protecting against unbounded
growth.
