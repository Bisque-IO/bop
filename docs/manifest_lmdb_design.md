# LMDB Manifest Design

This document describes the layout of the LMDB-backed manifest and the
append-only change feed used by streaming subscribers.

## Overview

The manifest persists core catalog state (databases, shards, jobs, metrics, and
generations) inside a dedicated LMDB environment. Writes are funneled through a
single worker thread that batches requests and commits them in a single LMDB
transaction. Readers operate using snapshot transactions and never block each
other.

## Streaming change log

Streaming subscribers observe manifest mutations via a dedicated change log. The
worker append this log after every successful batch so that replicas can rebuild
state transitions in order.

### Databases

| Database         | Key type | Value type                   | Notes                                  |
|------------------|----------|------------------------------|----------------------------------------|
| `change_log`     | `U64`    | `ManifestChangeRecord`       | Append-only sequence of committed ops. |
| `change_cursors` | `U64`    | `ManifestCursorRecord`       | Per-subscriber acknowledgement state.  |

The `change_log` key is a monotonically increasing `ChangeSequence` assigned by
the writer. Values carry:

- `record_version` – currently `1`.
- `sequence` – the assigned sequence number (also stored as the key).
- `committed_at_epoch_ms` – wall clock timestamp captured when the batch was
  written.
- `operations` – the exact list of `ManifestOp` values that were applied. The
  payload matches the serialized structures already used by other manifest
  tables (Serde + bincode), so subscribers can deserialize without special
  adapters.
- `generation_updates` – a list of `(component, generation)` pairs describing
  the resulting generation watermark for every component touched in the batch.

The companion `change_cursors` table tracks each subscriber and the highest
sequence it has acknowledged. Records contain creation and update timestamps for
observability.

### Writer integration

`commit_pending` now writes both the manifest tables and the change log inside a
single LMDB transaction. Each batch receives the next sequence, the change
record is serialized, and the transaction commits. This preserves the
all-or-nothing semantics subscribers require: if a batch is visible in the log,
all associated manifest state changes are durable.

## Chunk and WAL catalogs

Chunks are immutable storage objects. Each entry in `chunk_catalog` now records
only chunk attributes (paths, residency, compression, purge timers, etc.). The
manifest no longer stores WAL metadata inside chunk rows; instead a dedicated
`wal_catalog` database tracks WAL artifacts independently. This keeps the two
lifecycles orthogonal:

- `chunk_catalog` rows describe snapshot/delta objects that may be promoted or
  replaced over time.
- `wal_catalog` rows capture WAL artifacts (append-only segments for raft or
  pager bundles for libsql) along with a local filesystem path and per-format
  metadata (page ranges for append-only, frame counts for pager bundles). The
  worker updates these rows as checkpoints seal WAL files, and retention runs can
  truncate the WAL catalog without touching chunk metadata.

When a checkpoint produces a chunk directly from an append-only WAL segment, the
corresponding `wal_catalog` entry can be promoted or deleted once the planner
records that the chunk now covers the same page span. For pager-based systems the
WAL entry survives until the planner merges it into the appropriate chunk(s).

## Subscriber workflow

1. **Register** – Clients call `register_change_cursor` with a
   `ChangeCursorStart`:
   - `Oldest` starts at the earliest retained sequence.
   - `Latest` starts after the newest committed sequence.
   - `Sequence(n)` clamps to the available window so callers can resume from a
     known sequence after restarts.

   Registration allocates a unique `ChangeCursorId`, persists it, and returns a
   `ChangeCursorSnapshot` describing the acknowledgement watermark, the next
   sequence to request, and the current retained bounds.

2. **Fetch** – `fetch_change_page` streams the next page of
   `ManifestChangeRecord`s beginning at the acknowledged offset (clamped to the
   current `oldest_sequence`). The helper returns both the materialized changes
   and watermarks so clients know whether they reached the head.

3. **Acknowledge** – After processing a page, clients call
   `acknowledge_changes(cursor_id, upto_sequence)`. The worker updates the cursor
   record and recomputes the global low-watermark (`min_acked_sequence + 1`).
   If all subscribers covered a prefix, the corresponding entries are deleted
   from the change log within the same transaction.

4. **Wait** – `wait_for_change` exposes a blocking helper backed by a
   `Condvar`. It allows efficient long-polling without spinning and integrates
   cleanly with async runtimes via a small blocking shim.

## Retention and truncation

The change table grows only while subscribers lag. Each acknowledgement updates
the per-cursor high watermark and the worker trims the prefix that every active
cursor has already consumed. When no cursors exist, the log is left intact so
operators can attach a new subscriber to inspect history.

On startup the manifest:

1. Loads all change log metadata into memory.
2. Clamps cursor acknowledgements to the available range in case the log was
   truncated by an older build.
3. Immediately prunes the log according to the stored acknowledgements so state
   remains compact across restarts.

## Failure and recovery

- Change sequences are monotonically increasing and gaps only appear when the
  log is truncated.
- Cursor registrations are durable. After a restart callers can resume by reusing
  the `ChangeCursorId` they were given originally.
- The worker shares the latest change state with readers via a mutex-protected
  snapshot so non-mutating APIs (`fetch_change_page`, `latest_change_sequence`,
  etc.) never need to coordinate with the writer thread.

## Operational notes

- All change records reuse the existing manifest serialization stack
  (`SerdeBincode`) to avoid bespoke encoders.
- Truncation work stays proportional to the amount of log being dropped because
  we only delete acknowledged prefixes.
- The change feed inherits manifest batching behaviour: even when multiple
  manifest transactions share a commit, each submitted batch receives its own
  change record so ordering matches caller expectations.
