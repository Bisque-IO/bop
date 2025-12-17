# maniac-libsql Design Document

A high-performance, branching SQLite storage engine designed for distributed systems with Raft consensus, zero-downtime schema evolutions, and async I/O integration via maniac's stackful coroutines.

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Core Components](#core-components)
4. [Storage Model](#storage-model)
5. [Branching System](#branching-system)
6. [Virtual WAL](#virtual-wal)
7. [Raft Log Integration](#raft-log-integration)
8. [Schema Evolution](#schema-evolution)
9. [Async I/O Integration](#async-io-integration)
10. [Chunk Storage](#chunk-storage)
11. [Page Management](#page-management)
12. [Compaction](#compaction)

---

## Overview

maniac-libsql provides a layered Content-Addressable Storage (CAS) architecture for SQLite databases with support for:

- **Git-like branching**: Fork databases at any point, creating isolated branches
- **MVCC versioning**: Multiple versions of data coexist without conflicts
- **Raft consensus**: Distributed replication with durable log segments
- **Zero-downtime schema migrations**: Evolve schemas without service interruption
- **Async I/O**: Integration with maniac runtime's stackful coroutines via `sync_await`
- **Compression & encryption**: LZ4 compression and AES-256-GCM encryption support

## Architecture

```
+-----------------------------------------------------------------------+
|                           SQLite Core                                 |
|               (Synchronous VFS/WAL callbacks)                         |
+-----------------------------------------------------------------------+
                                  |
                                  v
+-----------------------------------------------------------------------+
|                     VirtualWal / VirtualVfs                           |
|          (Implements libsql WAL interface, uses sync_await)           |
+-----------------------------------------------------------------------+
                    |                               |
                    v                               v
+------------------------------+    +----------------------------------+
|       In-Memory WAL          |    |      Async Storage Layer         |
|    (DashMap frame index)     |    |  (AsyncChunkStorage + sync_await)|
+------------------------------+    +----------------------------------+
                                                    |
                    +-------------------------------+-------------------------------+
                    |                                                               |
                    v                                                               v
+----------------------------------+                    +----------------------------+
|      Local Chunk Storage         |                    |    Remote Storage (S3)     |
|    (LocalFsChunkStorage)         |                    |    (Future extension)      |
+----------------------------------+                    +----------------------------+
                    |
                    v
+-----------------------------------------------------------------------+
|                          libmdbx Store                                |
|                (Metadata, Branch info, Raft state)                    |
+-----------------------------------------------------------------------+
```

## Core Components

### Store (`store.rs`)

The central metadata store backed by libmdbx, managing:

| Table | Purpose |
|-------|---------|
| `databases` | Database info (name, page_size, chunk_size) |
| `branches` | Branch metadata with parent/fork relationships |
| `segment_versions` | Chunk mappings per segment version |
| `chunks` | Raw chunk data (CAS by CRC64 hash) |
| `chunk_ref_counts` | Reference counting for deduplication |
| `wal_map` | WAL segment S3 key mappings |
| `checkpoints` | Checkpoint state per branch |
| `raft_log` | Raft log entries |
| `raft_segments` | Raft segment file metadata |
| `raft_state` | Raft consensus state |
| `evolutions` | Schema evolution records |

#### Database ID Encoding

Database IDs encode configuration in the upper 32 bits:

```
[ Page Shift KB (8 bits) | PPC Shift (8 bits) | Reserved (16 bits) | ID (32 bits) ]
```

- **Page Shift KB**: `log2(page_size / 1024)` - supports up to 32MB pages
- **PPC Shift**: `log2(pages_per_chunk)` - supports up to 32768 pages/chunk

### Schema Types (`schema.rs`)

Core data structures:

```rust
pub struct DatabaseInfo {
    pub name: String,
    pub created_at: u64,
    pub page_size: u32,
    pub chunk_size: u32,
}

pub struct BranchInfo {
    pub database_id: DatabaseId,
    pub name: String,
    pub parent_branch_id: Option<u32>,
    pub fork_at_version: u64,
    pub current_head_version: u64,
}

pub struct WalSegment {
    pub start_frame: u64,
    pub end_frame: u64,
    pub s3_key: String,
    pub timestamp: u64,
}
```

## Storage Model

### Content-Addressable Storage (CAS)

Chunks are deduplicated using CRC64-NVME hashing:

1. **Hash calculation**: CRC64 of raw chunk data becomes the chunk ID
2. **Reference counting**: Track references to enable safe garbage collection
3. **Deduplication**: Identical chunks share storage automatically

### Layered Lookup

Page lookup traverses the branch hierarchy:

```
1. Check in-memory WAL frames
2. Check page cache (DashMap)
3. Search current branch's segment_versions
4. If not found, search parent branch (up to fork_at_version)
5. Repeat up the tree until found or root reached
```

## Branching System

### Branch Creation

```rust
let branch_id = store.create_branch(
    db_id,
    "feature-branch".to_string(),
    Some(parent_branch_id),  // Fork from parent
)?;
```

Branches are created as lightweight forks:
- Point to parent at current version
- Only divergent pages consume new storage
- Copy-on-write semantics

### Branch Hierarchy

```
main (v100)
  +-- feature-a (forked at v80)
  |     +-- feature-a-fix (forked at v85)
  +-- feature-b (forked at v95)
```

## Virtual WAL

### WalState (`virtual_wal.rs`)

Shared state across connections to a branch:

```rust
pub struct WalState {
    pub branch: BranchRef,
    pub page_size: u32,
    pub max_frame: AtomicU32,
    pub frames: DashMap<u32, Arc<WalFrame>>,      // frame_no -> data
    pub page_index: DashMap<u32, u32>,            // page_no -> frame_no
    pub chunk_index: DashMap<u32, ChunkLocation>, // page_no -> storage location
    pub checkpointed_frame: AtomicU32,
    pub db_size: AtomicU32,
    pub write_lock: AtomicU64,
    pub read_txn_count: AtomicU32,
}
```

### VirtualWalManager

Generic over async storage backend:

```rust
pub struct VirtualWalManager<S: AsyncChunkStorage = NoopAsyncStorage> {
    store: Store,
    wal_states: DashMap<(u64, u32), Arc<WalState>>,
    async_bridge: Option<SyncAsyncBridge<S>>,
    chunk_config: ChunkConfig,
    pages_per_chunk: u32,
}
```

### Page Lookup Path

```
SQLite read_page()
    |-> find_frame()
        |-> Check WAL frames (up to read_mark)
    |-> Not in WAL?
        |-> fallback_read_page()
        |-> sync_await(load_page_async())
        |-> Yield coroutine on I/O
        |-> Resume when data ready
        |-> Return to SQLite
```

## Raft Log Integration

### Entry Types

```rust
pub enum RaftEntryType {
    WalMetadata = 1,   // WAL segment persisted to S3
    Changeset = 2,     // SQLite session changeset
    Checkpoint = 3,    // WAL applied to base layer
    BranchCreated = 4, // New branch created
}
```

### RaftLogManager

```rust
pub struct RaftLogManager<S: ChunkStorage, R: RaftSegmentStorage> {
    store: Store,
    chunk_storage: S,        // S3-like storage for WAL chunks
    segment_storage: R,      // Segment files for changeset data
    durability_cache: HashMap<BranchRef, BranchDurabilityState>,
}
```

### Durability Tracking

Compaction eligibility requires ALL conditions:

1. **WAL Durable**: Frames persisted to S3
2. **Checkpoint Complete**: WAL applied to base layer
3. **Committed**: Entry committed via Raft

```rust
pub struct BranchDurabilityState {
    pub wal_durable_frame: u64,
    pub checkpointed_frame: u64,
    pub committed_index: u64,
    pub applied_index: u64,
}
```

## Schema Evolution

### Evolution States

```rust
pub enum EvolutionState {
    Created = 1,           // Target branch forked
    ApplyingDdl = 2,       // DDL changes in progress
    ReplayingChangesets = 3, // Catching up with source
    ReadyForCutover = 4,   // Caught up, awaiting cutover
    Cutover = 5,           // Final sync in progress
    Completed = 6,         // Evolution complete
    Failed = 7,            // Rolled back
    Cancelled = 8,         // User cancelled
}
```

### Evolution Flow

```
1. Start Evolution
   +-> Fork target branch from source at current version

2. Apply DDL
   +-> Execute DDL statements on target branch

3. Replay Changesets
   +-> Apply changesets from source since fork

4. Ready for Cutover
   +-> Caught up, waiting for cutover command

5. Cutover
   +-> Stop source writes
   +-> Final changeset sync
   +-> Complete evolution

6. Completed
   +-> Target is now the active branch
```

### Usage Example

```rust
let manager = EvolutionManager::new(store);

// Start evolution
let evolution_id = manager.start_evolution(
    db_id,
    source_branch_id,
    "Add user_status column",
    vec!["ALTER TABLE users ADD COLUMN status TEXT DEFAULT 'active'"],
)?;

// Apply DDL
manager.apply_ddl(db_id, evolution_id, &ddl_executor).await?;

// Replay changesets until caught up
while manager.replay_changesets(db_id, evolution_id, &applier, 100).await? {
    // Continue replaying
}

// Cutover when ready
manager.cutover(db_id, evolution_id, &applier).await?;
```

## Async I/O Integration

### sync_await Bridge

The key innovation: using maniac's stackful coroutines to bridge sync SQLite callbacks with async I/O:

```rust
// Inside synchronous VFS/WAL callback
fn read_page(&self, page_no: u32, buffer: &mut [u8]) -> Result<()> {
    // Fast path: check in-memory cache
    if let Some(data) = self.cache.get(&page_no) {
        buffer.copy_from_slice(&data);
        return Ok(());
    }

    // Slow path: async I/O via sync_await
    sync_await(self.load_page_async(page_no, buffer))
}
```

### AsyncChunkStorage Trait

```rust
pub trait AsyncChunkStorage: Send + Sync + 'static {
    fn read_chunk(&self, key: &ChunkKey)
        -> impl Future<Output = StorageResult<Vec<u8>>> + Send;

    fn write_chunk(&self, key: &ChunkKey, data: Vec<u8>)
        -> impl Future<Output = StorageResult<()>> + Send;

    fn chunk_exists(&self, key: &ChunkKey)
        -> impl Future<Output = StorageResult<bool>> + Send;

    fn delete_chunk(&self, key: &ChunkKey)
        -> impl Future<Output = StorageResult<()>> + Send;
}
```

### SyncAsyncBridge

```rust
pub struct SyncAsyncBridge<S: AsyncChunkStorage> {
    loader: Arc<AsyncPageLoader<S>>,
}

impl<S: AsyncChunkStorage> SyncAsyncBridge<S> {
    // Blocks coroutine until async operation completes
    #[cfg(feature = "maniac-runtime")]
    pub fn load_page_sync(&self, branch: BranchRef, start_frame: u64,
                          end_frame: u64, page_no: u32) -> io::Result<Vec<u8>> {
        maniac::runtime::worker::sync_await(
            self.loader.load_page(branch, start_frame, end_frame, page_no)
        )
    }

    // Non-panicking variant
    pub fn try_load_page_sync(...) -> Option<io::Result<Vec<u8>>> {
        maniac::runtime::worker::try_sync_await(...)
    }
}
```

## Chunk Storage

### ChunkKey

S3-style path format:

```rust
pub struct ChunkKey {
    pub db_id: u64,
    pub branch_id: u32,
    pub start_frame: u64,
    pub end_frame: u64,
}

// Path format: {db_id}/{branch_id}/wal/{start_frame}-{end_frame}
// Example: 0000000000000001/00000002/wal/0000000000000064-00000000000000c8
```

### LocalFsChunkStorage

File-based implementation for development:

```rust
pub struct LocalFsChunkStorage {
    root: PathBuf,
}

impl ChunkStorage for LocalFsChunkStorage {
    async fn put(&self, key: &ChunkKey, data: &[u8]) -> StorageResult<()>;
    async fn get(&self, key: &ChunkKey) -> StorageResult<Vec<u8>>;
    async fn delete(&self, key: &ChunkKey) -> StorageResult<()>;
    async fn exists(&self, key: &ChunkKey) -> StorageResult<bool>;
    async fn list(&self, db_id: u64, branch_id: u32) -> StorageResult<Vec<ChunkKey>>;
}
```

## Page Management

### ChunkConfig

```rust
pub struct ChunkConfig {
    pub compression_enabled: bool,  // LZ4 compression
    pub compression_level: u32,     // 1-12
    pub encryption_enabled: bool,   // AES-256-GCM
    pub encryption_key: Option<[u8; 32]>,
}
```

### Chunk Header Format

```
[flags (1 byte)] [original_size (4 bytes, if compressed)]

Flags:
  0x01 = compressed
  0x02 = encrypted
```

### PageCache

Universal page cache using DashMap:

```rust
pub struct PageCache {
    cache: DashMap<PageCacheKey, CachedPage>,
    max_pages: usize,
    page_size: u32,
}

pub struct PageCacheKey {
    pub db_id: u64,
    pub branch_id: u32,
    pub page_id: u64,
}
```

### AsyncPageManager

High-level async page manager with `sync_await` support:

```rust
pub struct AsyncPageManager<S: AsyncChunkStorage> {
    storage: Arc<S>,
    cache: Arc<PageCache>,
    config: ChunkConfig,
    page_size: u32,
    pages_per_chunk: u32,
}

impl<S: AsyncChunkStorage> AsyncPageManager<S> {
    // Async page read
    pub async fn read_page<F>(&self, branch: BranchRef,
        start_frame: u64, end_frame: u64, page_no: u32,
        wal_lookup: F) -> anyhow::Result<Option<Arc<Vec<u8>>>>;

    // Sync wrapper using sync_await
    #[cfg(feature = "maniac-runtime")]
    pub fn read_page_sync<F>(...) -> anyhow::Result<Option<Arc<Vec<u8>>>>;
}
```

## Compaction

### Compactor

Garbage collection for CAS storage:

```rust
pub struct Compactor {
    store: Store,
}

impl Compactor {
    pub fn compact(&self, db_id: u64, branch_id: u64,
                   segment_id: u32, max_deltas: usize) -> Result<Vec<u32>>;
}
```

### Raft Log Segment Compaction

Segments can be deleted when ALL entries meet these criteria:

1. Committed via Raft (`commit_index >= log_index`)
2. WAL persisted to S3 (`wal_durable_frame >= end_frame`)
3. Checkpointed (`checkpointed_frame >= end_frame`)

---

## Feature Flags

| Feature | Description |
|---------|-------------|
| `maniac-runtime` | Enables `sync_await` integration for async I/O |

## Dependencies

| Crate | Purpose |
|-------|---------|
| `libmdbx` | Metadata storage |
| `dashmap` | Concurrent in-memory data structures |
| `lz4_flex` | LZ4 compression |
| `crc64fast-nvme` | CRC64 hashing for CAS |
| `congee` | Adaptive Radix Tree (concurrent indexing) |
| `maniac` | Stackful coroutine runtime |
| `maniac-libsql-ffi` | libsql FFI bindings |
| `maniac-libsql-sys` | libsql WAL system interface |

## Performance Characteristics

- **Branching**: O(1) - just a metadata pointer
- **Page lookup**: O(depth) where depth is branch hierarchy depth
- **Chunk deduplication**: O(1) via CRC64 hash
- **WAL frame lookup**: O(1) via DashMap
- **Async I/O**: Zero-cost bridging via stackful coroutines

## Future Work

- S3 async storage backend implementation
- Encryption key management integration
- Background compaction scheduler
- Read replica support
- Point-in-time recovery
