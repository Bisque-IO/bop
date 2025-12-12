## `storage_impl.rs` design: Per-group segmented log storage

This document describes the **current** design implemented in `crates/raft/src/multi/storage_impl.rs`.

It focuses on:
- **On-disk format** and crash-safety.
- **Per-group sharding** and durability callback semantics.
- **Recovery** and the **disk-backed read** path.
- **Indexing** (u64-keyed, `congee`-backed).
- **Caching**, compaction, and known trade-offs.

---

### Goals

- **Per Raft group isolation**: each group has independent lifecycle and no cross-group locking.
- **High write throughput**: group appends are batched and serialized through a per-group writer task.
- **Durable flush semantics**: `IOFlushed` callbacks are completed only after the corresponding writes are **fsync’d**.
- **Crash recovery**: tail corruption/partial writes are detected and truncated; state is rebuilt by replay.
- **Disk-backed reads**: reads work after restart and after cache eviction.

---

### Storage layout

- **Base directory**: `StorageConfig.base_dir`.
- **Per-group directory**: `{base_dir}/{group_id}/`.
- **Segment naming**: `group-{group_id}-{segment_id}.log`.
  - `segment_id` increases monotonically per group.

---

### Record format (per segment)

Each record is:

```
[len: u32 LE][type: u8][group_id: u64 LE][payload...][crc64: u64 LE]
```

- `len` is the number of bytes **after** the `len` field (i.e., `type + group_id + payload + crc64`).
- `crc64` is computed over `[type + group_id + payload]` using **crc64fast-nvme**.

#### Record types

- Raft:
  - `Vote`
  - `Entry`
  - `Truncate`
  - `Purge`
- LibSQL integration (stored in the same log; ignored by Raft recovery):
  - `WalFrame`, `Changeset`, `Checkpoint`, `WalMetadata`

---

### Sharding model and concurrency

#### Per-group sharding

- There is a **1:1 mapping** between a Raft group and a shard/writer.
- `PerGroupLogStorage` maintains a `DashMap<u64, Arc<GroupState<_>>>`.
- `get_log_storage(group_id)` returns a `GroupLogStorage` handle for that group.

#### Writer serialization (no mutex on the hot path)

- Each group owns a single writer task (`ShardState::writer_loop`) that owns the group’s `SegmentedLog`.
- All writes are sent through a bounded `flume` channel.
- The writer loop opportunistically **drains** queued requests (`try_recv`) to reduce wakeups.

---

### Durability semantics (`IOFlushed`)

OpenRaft supplies an `IOFlushed<C>` callback to `append()`.

This implementation provides **durable** semantics:

- A callback is only completed after the writer performs a durable `sync_all()` that covers the corresponding write.
- Writes and callbacks are propagated through the writer channel as a single request:
  - `WriteRequest::Write { data, callbacks, sync_immediately, ... }`

#### Fsync policy

Configured by `StorageConfig.fsync_interval`:

- **`None` (fully durable)**:
  - Every write implies an immediate `sync_all()`.
  - Callback completion happens after that sync.

- **`Some(interval)` (group-commit)**:
  - Writes are appended immediately.
  - `sync_all()` happens periodically when dirty.
  - Callbacks are queued and completed together after the next sync.

#### Error propagation

- The writer maintains a **sticky failure** state (`ShardState.failed`).
- On write/sync error:
  - The shard enters failed state.
  - Pending callbacks are completed with an error.
  - Future writes fail fast.

---

### Segmentation

- A segment is rotated when `current_size + data_len > segment_size`.
- Rotation fsyncs the old segment and opens a new segment file.

Write path returns a `WritePlacement { segment_id, offset }` so the caller can build an index for the appended records.

---

### In-memory state per group (`GroupState`)

`GroupState` maintains:

- **Vote**: stored in an `AtomicVote` (packed atomics).
- **Log state**:
  - `first_index`, `last_index` (atomics)
  - `last_log_id`, `last_purged_log_id` (`AtomicLogId`)
- **Cache**: `DashMap<u64, Arc<Entry>>` storing a window of recent entries.
- **Disk index**: `LogIndex` maps entry index → on-disk location.

#### AtomicLogId packing constraints

`AtomicLogId` packs term/node_id/index into 128 bits.
This imposes limits (enforced with `debug_assert!`):
- `node_id <= 0xFF_FFFF` (24 bits)
- `term` must fit the remaining packed bits

---

### Disk index design (`congee`)

The implementation replaces `BTreeMap<u64, ...>` with a `congee`-backed structure.

#### Why a wrapper is needed

`congee::U64Congee<V>` stores values as `usize` (via `V: From<usize> + Into<usize>`), which is not suitable for storing `LogLocation` directly.

So `LogIndex` is:

- `U64Congee<usize>`: maps `u64` entry index → **slot**
- `Vec<LogLocation>`: stores the actual `{segment_id, offset, len}`
- `free: Vec<usize>`: free-list for reusing slots on deletes

Operations provided:
- `get(index) -> Option<LogLocation>`
- `insert(index, LogLocation)`
- `truncate_from(first_removed)`
- `purge_to(last_removed)`

Range deletes (`truncate_from` / `purge_to`) use `U64Congee::range()` in batches.

---

### Read path (cache + disk)

`try_get_log_entries(range)` returns a contiguous set of entries starting at `range.start`.

For each index:

1. Try `cache`.
2. On miss: consult `LogIndex` and read the record from disk.
3. Decode the entry payload and return it.
4. If the entry is within the cache window, insert into cache.

#### Why reads use std I/O

Random-offset reads using Maniac’s async file interface can hit **alignment / direct-I/O constraints** on some configurations, producing `EINVAL` / partial reads.

To avoid this, disk-backed reads (and recovery replay scanning) use:
- `std::fs::File` + `seek()` + `read_exact()`

This is a correctness-first choice.

---

### Cache policy (bounded memory)

Configured by `StorageConfig.max_cache_entries_per_group`.

- Cache is treated as a **hot window** near `last_index`.
- The start of the window is computed as:
  - `cache_window_start = max(first_index, last_index - (max_cache_entries - 1))`
- `cache_low` tracks the lowest index allowed in cache.
- After append/truncate/purge and after disk-miss read insertion, `enforce_cache_window()` evicts entries `< cache_window_start`.

This keeps memory bounded while preserving correctness via disk-backed reads.

---

### Crash recovery

Recovery happens at group creation (`GroupState::new`).

#### Phase 1: segment validation and tail truncation

- List segments and process in ascending `segment_id`.
- For each segment, scan records and verify CRC.
- Stop at the first invalid record (partial write, corruption) and treat everything after as truncated.
- Compute `valid_bytes` per segment.

#### Phase 2: replay

Using the `(segment_id, valid_bytes)` manifest:

- Iterate records in order.
- Apply:
  - `Vote`: restore vote
  - `Entry`: build `LogIndex` and optionally populate cache
  - `Truncate`: remove indices > truncation point
  - `Purge`: remove indices <= purge point, advance `first_index`
- LibSQL record types are ignored for Raft state.

The result is a rebuilt in-memory state plus a disk index sufficient to serve reads.

---

### Compaction

Triggered on `purge()` (best-effort).

**Conservative rule** (to preserve raft metadata history):
- Only delete **old** segments (never delete the most recent segment).
- Only delete a segment if it contains **only Entry records**.
- Only delete it if its maximum entry index is strictly less than the purge boundary.

This avoids deleting segments that may contain `Vote`/`Truncate`/`Purge` markers.

Note: compaction depends on `maniac::fs::remove_file`, enabled via Maniac’s `unlinkat` feature.

---

### Configuration (`StorageConfig`)

- `base_dir`: directory root
- `segment_size`: rotate threshold (default 64MB)
- `fsync_interval`:
  - `None`: fsync every write
  - `Some(d)`: group-commit flush
- `max_cache_entries_per_group`: cache window size

---

### Limitations / trade-offs

- **Blocking std I/O on read/recovery**: chosen for correctness and portability vs. direct-I/O alignment constraints.
- **LogIndex free-list**: slots are reused; the side `locations` vector can grow until reused.
- **Truncate backward-compatibility**: older logs may have `Truncate` records with just an index (u64). Recovery attempts to handle both formats.
- **Concurrency on first access**: initialization uses a best-effort double-check; concurrent initialization can still do extra work, but only one is stored.

---

### Future work

- Persist a compact **segment footer index** to speed startup (avoid full replay for large logs).
- Store per-segment metadata (min/max entry index, presence of non-entry records) to make compaction O(#segments) without scanning.
- Optional async read path with safe alignment handling if Maniac adds an aligned random-read API.
- Stronger “exactly-once initialization” for groups (avoid duplicate recovery work under races).
