# Manifest Module

The **manifest** module is the metadata management system for bop-storage. It provides ACID-compliant persistence for database metadata, change tracking, and crash recovery using LMDB as the underlying storage engine.

## Overview

The manifest system serves as the "source of truth" for all metadata in bop-storage, including:

- Database descriptors and configuration
- WAL state tracking
- Chunk and delta catalog
- Snapshot management
- Job queue
- Metrics aggregation
- Generation watermarks
- Change log for replication
- Garbage collection refcounts

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Public API                               │
│  (register_change_cursor, fetch_change_page, begin, commit...)  │
└────────────┬────────────────────────────────────────────────────┘
             │
             ├──────────────────┬──────────────────┬──────────────┐
             ▼                  ▼                  ▼              ▼
    ┌────────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────┐
    │  Change Log    │  │   Cursors   │  │   Worker    │  │ Tables  │
    │  Management    │  │ Registration│  │    Loop     │  │  LMDB   │
    └────────────────┘  └─────────────┘  └─────────────┘  └─────────┘
             │                  │                  │              │
             └──────────────────┴──────────────────┴──────────────┘
                                       │
                                       ▼
                              ┌────────────────┐
                              │   Operations   │
                              │ (apply_batch,  │
                              │ publish_snap,  │
                              │ merge_metric)  │
                              └────────────────┘
                                       │
                                       ▼
                              ┌────────────────┐
                              │      LMDB      │
                              │  (17 tables)   │
                              └────────────────┘
```

## Module Structure

The manifest module is organized into 12 focused sub-modules:

### Core Modules

- **[mod.rs](mod.rs:290)** (290 lines) - Public API, core types, re-exports
- **[api.rs](api.rs:598)** (598 lines) - Public API methods, Manifest::open(), Drop impl
- **[tables.rs](tables.rs:1055)** (1,055 lines) - LMDB schema, keys, records

### Subsystem Modules

- **[change_log.rs](change_log.rs:430)** (430 lines) - Change log state, truncation, caching
- **[cursors.rs](cursors.rs:234)** (234 lines) - Cursor registration, acknowledgment
- **[worker.rs](worker.rs:462)** (462 lines) - Worker loop, command processing, batching
- **[operations.rs](operations.rs:680)** (680 lines) - Database operations (apply_batch, snapshots, metrics)
- **[transaction.rs](transaction.rs:215)** (215 lines) - Transaction builder (ManifestTxn)
- **[state.rs](state.rs:205)** (205 lines) - Runtime state, diagnostics, ChangeSignal
- **[manifest_ops.rs](manifest_ops.rs:111)** (111 lines) - ManifestOp enum (18 variants)
- **[flush_sink.rs](flush_sink.rs:91)** (91 lines) - Flush sink implementation

### Testing

- **[tests.rs](tests.rs:995)** (995 lines) - Comprehensive test suite

**Total:** 5,202 lines across 12 well-organized modules

## Key Concepts

### 1. LMDB Tables

The manifest uses 17 named LMDB databases (tables):

| Table | Key Type | Value Type | Purpose |
|-------|----------|------------|---------|
| `db` | DbId | DbDescriptorRecord | Database metadata |
| `wal_state` | DbId | WalStateRecord | WAL durability tracking |
| `chunk_catalog` | (DbId, ChunkId) | ChunkEntryRecord | Chunk metadata |
| `chunk_delta_index` | (DbId, ChunkId, Gen) | ChunkDeltaRecord | Delta chains |
| `snapshot_index` | (DbId, SnapshotId) | SnapshotRecord | Snapshots |
| `wal_catalog` | (DbId, ArtifactId) | WalArtifactRecord | WAL artifacts |
| `job_queue` | JobId | JobRecord | Background jobs |
| `job_pending_index` | (DbId, JobKind) | JobId | Job lookup index |
| `metrics` | (Scope, MetricKind) | MetricRecord | Aggregated metrics |
| `generation_watermarks` | ComponentId | GenerationRecord | Generation tracking |
| `gc_refcounts` | ChunkId | ChunkRefcountRecord | GC refcounts |
| `remote_namespaces` | RemoteNamespaceId | RemoteNamespaceRecord | Remote storage config |
| `change_log` | ChangeSequence | ManifestChangeRecord | Change log for replication |
| `change_cursors` | ChangeCursorId | ManifestCursorRecord | Cursor tracking |
| `pending_batches` | PendingBatchId | PendingBatchRecord | Crash recovery journal |
| `runtime_state` | Fixed key | RuntimeStateRecord | Crash detection sentinel |

### 2. Change Log System

The change log provides a replication-friendly view of all manifest changes:

- **Monotonic sequence numbers** - Each committed batch gets a unique sequence
- **Cursor-based subscription** - Clients register cursors to track progress
- **Automatic truncation** - Old entries removed when all cursors have acknowledged
- **Page cache integration** - Recent changes cached for fast retrieval

**Example:**
```rust
// Register a cursor starting from the oldest available change
let cursor = manifest.register_change_cursor(ChangeCursorStart::Oldest).await?;

// Fetch a page of changes
let page = manifest.fetch_change_page(cursor.cursor_id, 100).await?;

// Process changes...
for record in page.changes {
    process_change(&record);
}

// Acknowledge processed changes
manifest.acknowledge_changes(cursor.cursor_id, page.next_sequence).await?;
```

### 3. Worker Thread Architecture

The manifest uses a single worker thread to serialize all LMDB writes:

```
User Thread                  Worker Thread
─────────────                ─────────────
   │
   │ begin_with_capacity(n)
   ├───────────────────────────────►
   │                               │
   │ txn.put_chunk(...)            │
   │ txn.put_wal_state(...)        │
   │ ...                           │
   │                               │
   │ txn.commit()                  │
   ├──────────────────────────────►│
   │                               │ persist_pending_batch()
   │                               │ (journal to LMDB)
   │                               │
   │                               │ commit_pending()
   │                               │ (apply_batch + update change_log)
   │                               │
   │◄──────────────────────────────┤
   │ CommitReceipt                 │
   │                               │
```

**Benefits:**
- No write contention
- Batching for efficiency
- Crash recovery via journaling
- Clean error handling

### 4. Crash Recovery

The manifest provides ACID durability through two mechanisms:

#### Runtime State Sentinel
- A sentinel record is written on `Manifest::open()`
- Deleted on clean `close()` or `Drop`
- Presence on startup indicates a crash

#### Pending Batch Journal
- Batches are journaled to `pending_batches` table before applying
- On startup, pending batches are replayed
- Journal entries deleted after successful commit

This ensures no data loss even if the process crashes mid-commit.

### 5. Generation Tracking

Generations are monotonically increasing numbers used for versioning:

- Each component (WAL, checkpoint, etc.) has its own generation counter
- Generations are persisted to LMDB and cached in memory
- Used for:
  - Versioning snapshots
  - Tracking checkpoint progress
  - Coordinating distributed operations

### 6. Garbage Collection

The manifest tracks chunk refcounts for garbage collection:

- **Publish snapshot** → increment refcounts for all chunks in the snapshot
- **Drop snapshot** → decrement refcounts for all chunks
- **Refcount = 0** → chunk eligible for GC
- Refcounts are ACID-compliant and crash-safe

## Public API

### Opening a Manifest

```rust
use bop_storage::manifest::{Manifest, ManifestOptions};

let options = ManifestOptions {
    map_size: 256 * 1024 * 1024, // 256 MB
    max_dbs: 16,
    queue_capacity: 128,
    commit_latency: Duration::from_millis(5),
    page_cache: Some(cache),
    change_log_cache_object_id: Some(42),
};

let manifest = Manifest::open("/path/to/manifest", options)?;
```

### Creating a Transaction

```rust
// Begin a transaction
let mut txn = manifest.begin_with_capacity(10);

// Add operations
txn.put_db(db_id, descriptor);
txn.put_wal_state(key, wal_state);
txn.upsert_chunk(chunk_key, chunk_entry);

// Commit atomically
let receipt = txn.commit().await?;
```

### Change Log Subscription

```rust
// Register a cursor
let snapshot = manifest.register_change_cursor(
    ChangeCursorStart::Sequence(last_seq)
).await?;

// Fetch changes
let page = manifest.fetch_change_page(snapshot.cursor_id, 100).await?;

// Acknowledge progress
manifest.acknowledge_changes(snapshot.cursor_id, page.next_sequence).await?;
```

### Querying State

```rust
// Get WAL state
let wal_state = manifest.wal_state(db_id).await?;

// List all databases
let databases = manifest.list_dbs().await?;

// Get current generation
let gen = manifest.current_generation(component_id).await?;

// Get diagnostics
let diag = manifest.diagnostics();
```

## Concurrency Model

### Thread Safety

- **Manifest** is `Send + !Sync` - owned by one thread, but can be moved
- **ManifestTxn** is `!Send + !Sync` - must be used on the creating thread
- **Worker thread** serializes all writes - no write contention

### Locking

- **No locks in user-facing API** - commands sent via channel to worker
- **Internal locks:**
  - `change_state` - mutex for change log state updates
  - `generation_cache` - mutex for generation watermarks
  - `change_signal` - condvar for change notifications

### Async/Await

The manifest API is primarily **synchronous**, but commit operations return futures:

```rust
let receipt = txn.commit().await?; // Async wait for commit
```

This allows the caller to await completion without blocking the worker thread.

## Error Handling

The manifest uses structured errors with context:

```rust
pub enum ManifestError {
    Io(io::Error),                    // I/O errors
    Heed(heed::Error),                // LMDB errors
    WorkerClosed,                     // Worker thread terminated
    WaitTimeout,                      // Wait operation timed out
    CommitFailed(String),             // Commit failed
    InvariantViolation(String),       // Internal invariant broken
    CacheSerialization(String),       // Cache serialization failed
}
```

All operations return `Result<T, ManifestError>` for proper error propagation.

## Performance Characteristics

### Read Performance

- **LMDB reads** - O(log n) B-tree lookups
- **Change log cache** - O(1) for recent changes
- **Generation cache** - O(1) in-memory lookup

### Write Performance

- **Batching** - Multiple operations in single transaction
- **Commit latency** - Configurable (default 5ms)
- **No write contention** - Serialized through worker thread

### Memory Usage

- **LMDB map size** - Configurable (default 256 MB)
- **Change log cache** - Optional page cache integration
- **Generation cache** - Small hash map in memory

## Testing

The manifest has comprehensive test coverage:

- **17 integration tests** covering all major features
- **Crash recovery tests** - Verify journal replay
- **Concurrency tests** - Multi-cursor scenarios
- **Edge case tests** - Truncation, batching, refcounts

Run tests with:
```bash
cargo test --lib manifest
```

## Future Enhancements

Potential improvements for future versions:

1. **Read-only transactions** - Allow concurrent reads without affecting writes
2. **Compression** - Compress change log entries for space efficiency
3. **Snapshot isolation** - Support for MVCC-style reads
4. **Metrics export** - Built-in Prometheus/OpenTelemetry integration
5. **Sharding** - Distribute metadata across multiple LMDB environments

## References

- **LMDB Documentation**: https://www.symas.com/lmdb
- **heed crate**: https://docs.rs/heed/
- **bincode**: https://docs.rs/bincode/
- **CODE_REVIEW.md**: Project-level code review and action items

---

**For detailed API documentation, run:**
```bash
cargo doc --no-deps --open --package bop-storage
```