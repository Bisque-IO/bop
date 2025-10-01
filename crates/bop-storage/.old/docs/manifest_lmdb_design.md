# LMDB Manifest Design

This document codifies the design of the LMDB-backed manifest that orchestrates
chunk metadata, WAL progress, retention, and job orchestration for every
`bop-storage` page store. It complements `docs/manager_design.md` by providing a
complete specification for the schema, transactional model, and lifecycle of the
manifest environment.

## Goals & Scope
- Centralise all durable control-plane state (chunk catalogue, WAL sequences,
  snapshot directories, job queue, metrics) in a single LMDB environment.
- Provide transactional guarantees between WAL, checkpoint, and retention
  operations while supporting high read concurrency.
- Enable copy-on-write snapshots and forks without duplicating chunk objects.
- Track object residency across local disk and remote backends (for example S3)
  and drive clean-up automatically once references vanish.
- Expose a manifest API that remains stable as record schemas evolve via version
  bumps and migrations.

The manifest stores metadata only; chunk payloads and WAL files live on disk or
remote object storage. Every storage pod (page store plus WAL pair) owns a
logical namespace inside the shared manifest.

## LMDB Environment Layout
- **Single environment**: the manager opens one LMDB environment (see
  `docs/manager_design.md`) with `NOSUBDIR | SYNC_DURABLE` flags. The map size is
  tuned for metadata (under a few GiB) and auto-resized when free space drops
  beneath a configurable watermark.
- **Identifier widths**: `DbId`, `ChunkId`, and `SnapshotId` are 32-bit values.
  Packing two of them into a single 64-bit integer yields space-efficient
  composite keys that respect LMDB's lexicographical ordering while leaving head
  room for shard or prefix bits.
- **Single writer**: a dedicated manifest writer thread owns the lone write
  transaction stream. Other components interact through typed async APIs that
  enqueue operations and await completion.
- **Multiple DBIs**: each logical table lives in its own LMDB database handle.
  Tables are keyed by `db_id` (page store id) so a single environment can host
  many pods. Read transactions are cheap snapshots that callers open on demand.

### Table Summary
| Table | Key | Value | Purpose |
|-------|-----|-------|---------|
| `db` | `DbId` (`u32`) | `DbDescriptorRecord` | Page store identity, config hash, lifecycle state. |
| `wal_state` | `{db_id: u32, shard: u16}` | `WalStateRecord` | WAL tail position, checkpoint generation, flush gate state. |
| `chunk_catalog` | `{db_id: u32, chunk_id: u32}` | `ChunkEntryRecord` | Chunk location, residency, compression, WAL coverage. |
| `chunk_delta_index` | `{db_id: u32, chunk_id: u32, generation: u32}` | `ChunkDeltaRecord` | Optional delta objects overlaying a base chunk. |
| `remote_namespace` | `RemoteNamespaceId` (`u32`) | `RemoteNamespaceRecord` | Canonical bucket/prefix definitions for remote object storage. |
| `snapshot_index` | `{db_id: u32, snapshot_id: u32}` | `SnapshotRecord` | Immutable manifests for snapshots or forks with WAL anchor. |
| `wal_manifest` | `{db_id: u32, generation: u32, chunk_id: u32}` | `WalChunkPointer` | Ordering of WAL chunks per generation. |
| `job_queue` | `JobId` (`u64`) | `JobRecord` | Durable work items (checkpoint, retention, upload, delete). |
| `job_pending_index` | `{db_id: u32, job_kind: u16}` | `JobId` | Unique-job gate ensuring one outstanding job per kind. |
| `metrics` | `{scope: u32, metric_kind: u32}` | `MetricRecord` | Rolling metrics and histograms. |
| `generation_watermarks` | `ComponentId` (`u32`) | `GenerationRecord` | Cross-component generation tracking for awaiters. |
| `gc_refcounts` | `ChunkId` (`u32`) | `ChunkRefcount` | Reference count across manifests and snapshots; drives retention. |

Records follow the conventions laid out in `docs/manager_design.md`:
- Keys: fixed-width structs (annotated with `#[repr(C)]`) encoded as raw bytes so
  LMDB ordering matches tuple ordering.
- Values: `SerdeBincode<T>` payloads prefixed with a `record_version` (`u16`).
  Readers must tolerate older versions; writers bump `record_version` when
  incompatible schema changes ship.

## Typed API Layer
The crate exposes a `ManifestStore` facade that provides strongly typed methods
instead of raw LMDB calls. Core entry points include:

- `register_db(descriptor)` and `update_db_status(...)`
- `reserve_job_id()`, `enqueue_job(JobRecord)`, `complete_job(job_id, state)`
- `append_wal_chunk(db_id, shard, WalChunkUpdate)` records a newly sealed WAL
  chunk, updates `wal_state` and `chunk_catalog`, and optionally schedules
  uploads.
- `checkpoint_apply(db_id, generation, ChunkMutationBatch)` registers a full
  chunk rewrite or a delta overlay after a checkpoint.
- `publish_snapshot(db_id, snapshot_id, SnapshotRecord)` atomically captures a
  manifest view and increments chunk refcounts.
- `drop_snapshot(db_id, snapshot_id)` decrements refcounts and enqueues GC jobs
  if counts reach zero.
- `record_metric(scope, metric_kind, observation)` merges metric updates inside
  the manifest transaction so counters stay in sync with state changes.

Each API method constructs a manifest operation that the writer thread applies
within a single LMDB write transaction, ensuring cross-table invariants hold.

## Transaction Model
- **Batching**: the writer thread drains an internal channel, coalescing multiple
  operations into one write transaction up to a latency or size budget. This
  minimises `mdb_txn_begin`/`commit` overhead while keeping flush paths fast.
- **Ordering**: operations are applied in the order received. Invariants (for
  example, a chunk refcount must exist before a snapshot references it) are
  enforced by sequencing updates within the transaction.
- **Awaiters**: callers receive a `ManifestGuard` that resolves once the writer
  commits the transaction containing their update. Guards return the new
  `GenerationRecord` so callers can await specific durability milestones.
- **Read access**: readers open short-lived read transactions (`RoTxn`). Because
  LMDB snapshots at transaction start, callers should avoid long-lived reads to
  keep freelist reclamation moving.

## Chunk and Delta Tracking
- **Chunk identities**: `chunk_id` (`u32`) is monotonically allocated per
  database. Each catalog record stores generation, `size_bytes`, compression,
  residency (`Local`, `RemoteArchived`, `Evicted`), and the WAL coverage range
  (`lsn_start`, `lsn_end`).
- **Delta companions**: small checkpoints can emit `ChunkDeltaRecord` entries
  that reference a base chunk. Each delta stores:
  - `base_chunk_id`: immutable chunk being patched.
  - `delta_generation`: unique identifier for the overlay (`u32`).
  - `dirty_pages`: roaring bitmap (or page list) of 4 KiB pages affected.
  - `payload_locator`: filesystem path or remote object covering the delta.
  The manifest ensures delta chains never exceed a configured length by
  scheduling a merge job once the combined size crosses a threshold.
- **Refcounts**: `gc_refcounts` holds `in_use` and `pending_upload` counters per
  chunk or delta. Snapshot publication increments counts; release operations
  decrement and trigger GC jobs when both counters hit zero.

## WAL Integration
- Sealing a WAL chunk calls `append_wal_chunk`, which:
  1. Writes a `WalChunkPointer` entry ordered by generation and `chunk_id`.
  2. Updates `wal_state.last_sealed_segment` and `last_applied_lsn`.
  3. Inserts or updates the `chunk_catalog` row, marking residency.
  4. Increments refcounts if the chunk immediately participates in an active
     manifest (for example, tail streaming replicas).
- WAL checkpoints may consume a chunk well before the nominal 64 MiB boundary.
  Raft workloads incrementally accumulate records until a full archival chunk is
  ready, while libsql workloads may emit deltas and archive them immediately.
  When a raft chunk reaches capacity it is already in its final form—no rewrite
  is required—so checkpointing can simply publish the existing on-disk chunk.
  The manifest must therefore tolerate multiple partially filled chunks in flight
  and reflect their archival status explicitly.
- The tail chunk remains open until sealing; transient offsets live in memory and
  do not touch LMDB.

## Checkpoint Pipeline
1. The checkpoint scan groups WAL frames by logical chunk.
2. For each chunk, decide whether to rewrite or create a delta:
   - **Full rewrite**: allocate a new `chunk_id`, write the chunk file, upload if
     required, then call `checkpoint_apply` with `ChunkMutation::Full`.
   - **Delta**: emit a delta object and call `checkpoint_apply` with
     `ChunkMutation::Delta` pointing at the base chunk.
3. The manifest transaction updates:
   - `chunk_catalog` (new chunk or delta metadata).
   - `gc_refcounts` (increment new references, decrement superseded ones).
   - `wal_state.last_checkpoint_generation`.
   - `snapshot_index` when checkpoints publish a new active manifest.
4. Once the manifest commit completes, the checkpoint orchestrator truncates the
   WAL to `last_applied_lsn`.

## Snapshots and Forks
- **Snapshot creation**: in one transaction, copy the active manifest entries for
  a `db_id` into `snapshot_index` keyed by `snapshot_id`, then increment
  refcounts for referenced chunks and deltas.
- **Fork creation**: allocate a new `DbId` (`u32`) for the fork, clone relevant
  manifest rows (chunks, WAL state, snapshot pointers) into the new namespace, and
  record fork-specific options. Subsequent WAL and checkpoint activity for the
  fork uses the same APIs while sharing chunk refcounts until divergence.
- **Deletion**: dropping a snapshot or fork decrements refcounts and enqueues GC
  jobs for any chunk or delta whose count reaches zero. Job payloads include
  local paths or remote locators so workers can remove artifacts.

## Retention and Garbage Collection
- A background worker scans `gc_refcounts` for zeroed entries. For each:
  - `Local` residency: delete the file from disk.
  - `RemoteArchived` residency: issue a remote delete (optionally retaining
    recent versions through bucket policy).
  - After deletion, remove the catalog or delta entry.
- Retention policies (for example, keep N checkpoint generations) translate into
  manifest operations that drop old snapshot entries, allowing the GC pass to
  collect unreachable chunks.

## Remote Storage Metadata
- Remote object storage metadata is normalized into two layers:
  1. `RemoteNamespaceRecord` keyed by `RemoteNamespaceId` defines the bucket,
     static prefix, and optional encryption profile. Namespaces are reference
     counted so they can be garbage collected when no chunks or deltas point at
     them.
  2. `RemoteObjectKey` values embedded in `ChunkEntryRecord`/`ChunkDeltaRecord`
     reference a namespace id plus a compact object suffix.
- `ChunkEntryRecord` tracks storage backend (`Local`, `LocalAndRemote`,
  `RemoteArchived`, `Evicted`). Remote variants persist a `RemoteObjectKey` that
  expands to the canonical object name anchored to the object's origin database id:
  ```text
  {origin_db_id:08x}/{chunk_id:08x}/{variant}:{generation:08x}
  ```
  - `origin_db_id` captures the database that first materialised the object so
    forks and clones reuse the same remote path.
  - `variant` is one of `base`, `delta-{delta_id:04x}`, or other reserved codes
    (for example `wal`).
  - The tuple uniquely identifies every materialized artifact: successive
    rewrites bump `generation`, deltas carry both the delta id and generation,
    and different artifact classes hash into distinct variants.
- Compression configuration, integrity data (SHA-256, chunk bitmap hash),
  upload timestamps, and remote verification state continue to live in the chunk
  records so GC can reason about retention without materializing the namespace.
- Delta records mirror the namespace id + suffix model. As a result GC workers
  reconstruct full object paths by joining `RemoteNamespaceRecord` with the
  compact suffix, avoiding string duplication across millions of entries.

## Startup and Recovery
1. Open the LMDB environment and run migrations, bumping `record_version`
   handlers as needed.
2. Rehydrate memory by scanning namespaces for active `db_id`s:
   - Build chunk refcount state from `snapshot_index` and live manifests.
   - Load `wal_state` to determine replay starting points.
   - Resume pending jobs from `job_queue` (`Pending` or `InFlight`).
3. Spawn the manifest writer thread and begin serving API calls.
4. Start retention and snapshot workers with the rehydrated state.

## Observability
- Each manifest transaction emits structured events (operations applied, new
  generation number, duration, page count touched).
- The metrics table records rolling counters and histograms for WAL lag,
  checkpoint latency, and snapshot counts. Observability clients can open read
  transactions to export metrics without disturbing the writer.
- Audit logs capture significant state transitions (snapshot created or dropped,
  chunk uploaded or evicted) and reference the manifest generation for
  traceability.

## Future Extensions
- **Manifest sharding**: if the single LMDB environment becomes a bottleneck,
  shard by `db_id` into multiple environments that follow the same schema.
- **Schema evolution tooling**: maintain a migration ledger so manifest upgrades
  are replay safe and idempotent.
- **Encrypted metadata**: optionally encrypt sensitive fields (for example remote
  locators) using per-pod keys stored as sealed blobs in the manifest.
- **Streaming subscribers**: implement an append-only change table that the
  writer thread updates for every committed batch, exposing a cursor-based feed
  so external components can tail manifest updates without polling. Downstream
  readers checkpoint offsets and the table can be truncated once all consumers
  acknowledge a prefix.

This LMDB manifest underpins all durability, retention, and replication features
in `bop-storage`. By constraining every state transition to a single transactional
pipeline we guarantee correctness while keeping the system extensible for future
storage backends and workloads.
