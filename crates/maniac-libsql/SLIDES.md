# maniac-libsql

## A High-Performance Distributed SQLite Storage Engine

---

# Slide 1: Overview

## maniac-libsql

**Git-like branching for SQLite databases**

A high-performance, branching SQLite storage engine designed for distributed systems with:

- Raft consensus replication
- Zero-downtime schema evolution
- Async I/O integration via stackful coroutines
- Content-Addressable Storage (CAS) with automatic deduplication

---

# Slide 2: The Problem

## Traditional SQLite Limitations

| Challenge | Impact |
|-----------|--------|
| Single-writer bottleneck | Limited concurrency |
| No native branching | Can't test schema changes safely |
| Schema migrations = downtime | Business disruption |
| No built-in replication | Manual HA solutions |
| Sync I/O only | Blocks on disk operations |

**We need a better way.**

---

# Slide 3: The Solution

## maniac-libsql Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    SQLite Core                              │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│              VirtualWal / VirtualVfs                         │
│         (Bridges sync SQLite with async I/O)                 │
└─────────────────────────────────────────────────────────────┘
         ↓                                      ↓
┌──────────────────────┐              ┌─────────────────────────┐
│  In-Memory WAL       │              │  Async Storage Layer    │
│ (Lock-free DashMap)  │              │ (Pluggable backends)    │
└──────────────────────┘              └─────────────────────────┘
                                              ↓
                    ┌─────────────────────────────────────┐
                    │                                     │
         ┌──────────↓──────────┐        ┌────────────────↓────┐
         │ Local Chunk Storage │        │  Remote Storage     │
         │    (Development)    │        │   (S3, etc.)        │
         └────────────────────┘        └─────────────────────┘
                    ↓
         ┌────────────────────────┐
         │   libmdbx Metadata     │
         │   Store (Catalog)      │
         └────────────────────────┘
```

---

# Slide 4: Key Features

## What Makes maniac-libsql Different

| Feature | Description |
|---------|-------------|
| **Git-Like Branching** | O(1) branch creation with copy-on-write |
| **MVCC** | Multi-version concurrency control |
| **Auto Deduplication** | CAS with CRC64-NVME hashing |
| **Compression** | LZ4 (configurable levels 1-12) |
| **Encryption** | AES-256-GCM per chunk |
| **Raft Consensus** | Built-in distributed replication |
| **Zero-Downtime Migrations** | Schema evolution without service interruption |
| **Async I/O** | Seamless integration via sync_await |

---

# Slide 5: Git-Like Branching

## Lightweight Database Forks

```
main (v100)
  +-- feature-a (forked at v80)
  |     +-- feature-a-fix (forked at v85)
  +-- feature-b (forked at v95)
```

### How It Works

- **O(1) branch creation** - Just a metadata pointer
- **Copy-on-write semantics** - Only divergent pages consume storage
- **Full isolation** - Each branch sees its own consistent state
- **Page lookup** - Traverses branch hierarchy until found

### Use Cases

- Test schema changes on a branch before merging
- Point-in-time snapshots
- Isolated feature development environments

---

# Slide 6: Content-Addressable Storage

## Automatic Deduplication

```
┌─────────────────┐     ┌─────────────────┐
│   Branch A      │     │   Branch B      │
│  Page 1 ────────┼─────┼──► Chunk X      │ ← Same data, stored once
│  Page 2 ────────┼─────┼──► Chunk Y      │
│  Page 3 ────────┼──┐  │                 │
└─────────────────┘  │  └─────────────────┘
                     │
                     └──► Chunk Z (unique to A)
```

### Benefits

- **CRC64-NVME hashing** for fast content addressing
- **Reference counting** enables safe garbage collection
- **Transparent** - No application changes needed
- **Significant storage savings** for similar branches

---

# Slide 7: Zero-Downtime Schema Evolution

## Migrate Without Interruption

```
┌────────────┐     ┌────────────┐     ┌────────────────────┐
│  Created   │ ──► │ ApplyingDDL│ ──► │ReplayingChangesets │
└────────────┘     └────────────┘     └────────────────────┘
                                              │
┌────────────┐     ┌────────────┐             ▼
│ Completed  │ ◄── │  Cutover   │ ◄── ┌────────────────────┐
└────────────┘     └────────────┘     │ ReadyForCutover    │
                                      └────────────────────┘
```

### Process

1. **Create target branch** from source
2. **Apply DDL** (ALTER TABLE, etc.)
3. **Replay changesets** to catch up with source
4. **Cutover** - Atomic switch with final sync
5. **Rollback available** if issues detected

---

# Slide 8: Async I/O Innovation

## Bridging Sync SQLite with Async Storage

### The Challenge

SQLite VFS callbacks are synchronous, but we need async I/O for:
- Network storage (S3)
- Non-blocking operations
- High concurrency

### The Solution: sync_await

```rust
// Inside synchronous VFS callback
fn read_page(&self, page_no: u32, buffer: &mut [u8]) -> Result<()> {
    // Fast path: check cache (no async needed)
    if let Some(data) = self.cache.get(&page_no) {
        return Ok(());
    }

    // Slow path: async I/O via sync_await
    sync_await(self.load_page_async(page_no, buffer))
}
```

**Zero-cost when data is cached. Seamless async when not.**

---

# Slide 9: Raft Consensus Integration

## Built-in Distributed Replication

### Entry Types

| Type | Purpose |
|------|---------|
| WalMetadata | WAL segment persisted to S3 |
| Changeset | SQLite session changeset |
| Checkpoint | WAL applied to base layer |
| BranchCreated | New branch metadata |

### Durability Tracking

```rust
BranchDurabilityState {
    wal_durable_frame: u64,    // Persisted to S3
    checkpointed_frame: u64,   // Applied to base
    committed_index: u64,      // Raft committed
    applied_index: u64,        // Applied to state
}
```

**Strong consistency guarantees with automatic failover.**

---

# Slide 10: Two-Layer Durability

## Local + Remote Persistence

```
┌─────────────────────────────────────────────────────────────┐
│                     Write Path                               │
└─────────────────────────────────────────────────────────────┘
                            │
          ┌─────────────────┴─────────────────┐
          ▼                                   ▼
┌─────────────────────┐             ┌─────────────────────┐
│   Local WAL File    │             │   Remote Storage    │
│   (fsync on commit) │             │   (async upload)    │
│                     │             │                     │
│   Immediate         │             │   Disaster          │
│   Recovery          │             │   Recovery          │
└─────────────────────┘             └─────────────────────┘
```

### WAL Segment Format

- Magic header: `0x57414C53` ("WALS")
- Metadata: db_id, branch_id, frame range
- Frames: page_no + db_size + page data
- CRC64 checksum for integrity

---

# Slide 11: Performance Characteristics

## Designed for Speed

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Branch creation | **O(1)** | Metadata pointer only |
| Page lookup (cached) | **O(1)** | DashMap concurrent access |
| Page lookup (cross-branch) | **O(depth)** | Traverse hierarchy |
| Chunk deduplication | **O(1)** | CRC64 hash lookup |
| WAL frame lookup | **O(1)** | Lock-free DashMap |
| Async I/O | **Zero-cost** | When data cached |

### Optimized Data Structures

- **DashMap** - Lock-free concurrent hash maps
- **Congee** - Adaptive Radix Tree for indexing
- **libmdbx** - High-performance embedded key-value store

---

# Slide 12: Configuration

## Flexible & Tunable

### Database Configuration

```rust
DatabaseConfig {
    page_size: 4096,      // 4KB (power of 2, 512B - 64KB)
    chunk_size: 262144,   // 256KB (64 pages per chunk)
}
```

### Storage Configuration

```rust
StorageConfig {
    compression_enabled: true,
    compression_level: 4,    // 1-12 (higher = better ratio)
    encryption_key: None,    // Optional AES-256-GCM
}
```

### WAL Configuration

```rust
WalWriterConfig {
    flush_interval: Duration::from_secs(1),
    max_segment_size: 16 * 1024 * 1024,  // 16MB
    max_frames_per_segment: 4096,
}
```

---

# Slide 13: Module Architecture

## Clean Separation of Concerns

| Module | Responsibility |
|--------|----------------|
| `database.rs` | High-level API, connection pooling |
| `store.rs` | libmdbx metadata catalog |
| `virtual_wal.rs` | In-memory WAL, frame management |
| `page_manager.rs` | Page I/O, compression, caching |
| `async_io.rs` | sync_await bridge |
| `chunk_storage.rs` | Storage abstraction trait |
| `wal_storage.rs` | WAL persistence, segments |
| `raft_log.rs` | Raft integration, durability |
| `schema_evolution.rs` | Zero-downtime migrations |

---

# Slide 14: Use Cases

## Where maniac-libsql Shines

### 1. Multi-Tenant SaaS
- Branch per tenant for isolation
- Shared storage with deduplication
- Zero-downtime tenant migrations

### 2. CI/CD Pipelines
- Database snapshots for testing
- Schema migration validation
- Isolated feature branch testing

### 3. High-Availability Systems
- Raft replication for failover
- S3 backup for disaster recovery
- Point-in-time recovery

### 4. Edge Computing
- Lightweight local branches
- Async replication to cloud
- Offline-first with eventual consistency

---

# Slide 15: Competitive Advantages

## Why Choose maniac-libsql?

| Feature | Traditional SQLite | maniac-libsql |
|---------|-------------------|---------------|
| Branching | Manual copies | Native, O(1) |
| Replication | External tools | Built-in Raft |
| Schema migrations | Downtime required | Zero-downtime |
| Deduplication | None | Automatic CAS |
| Async I/O | Not supported | sync_await bridge |
| Encryption | External | Built-in AES-256 |
| Compression | None | LZ4 per-chunk |

---

# Slide 16: Technical Highlights

## Innovation Summary

### 1. sync_await Bridge
Unique async/sync bridging via maniac's stackful coroutines

### 2. Lightweight Branching
O(1) creation, copy-on-write isolation, Git-style semantics

### 3. Automatic Deduplication
CAS with CRC64-NVME, reference counting, transparent to apps

### 4. Zero-Downtime Migrations
Multi-stage evolution with changesets, atomic cutover

### 5. Two-Layer Durability
Local fsync + async remote replication

---

# Slide 17: Dependencies

## Carefully Selected Stack

| Dependency | Purpose |
|------------|---------|
| **libmdbx** | ACID metadata storage |
| **dashmap** | Lock-free concurrent maps |
| **maniac** | Stackful coroutine runtime |
| **lz4_flex** | Fast compression |
| **crc64fast-nvme** | Content hashing |
| **congee** | Adaptive Radix Tree |

### No External Service Requirements
- Works standalone
- S3 support is pluggable
- Local filesystem for development

---

# Slide 18: Getting Started

## Quick Example

```rust
use maniac_libsql::{DatabaseManager, Database, Branch};

// Create database manager
let manager = DatabaseManager::new(store, chunk_storage);

// Create a new database
let db = manager.create_database("myapp").await?;

// Create main branch
let main = db.create_branch("main").await?;

// Fork for feature development
let feature = main.fork("feature-auth").await?;

// Make changes on feature branch...
feature.execute("ALTER TABLE users ADD COLUMN status TEXT").await?;

// When ready, merge back or cutover
manager.start_evolution(db_id, main_id, feature_id, ddl).await?;
```

---

# Slide 19: Summary

## maniac-libsql

**A modern SQLite storage engine for distributed systems**

- **Git-like branching** - Test changes safely
- **Zero-downtime migrations** - Never interrupt service
- **Automatic deduplication** - Efficient storage
- **Raft consensus** - High availability
- **Async I/O** - High performance

### Built for

- Distributed database backends
- Multi-tenant systems
- Zero-downtime deployments
- Edge computing

---

# Slide 20: Thank You

## Questions?

**maniac-libsql** - Git-like branching for SQLite

Part of the **bisque-io/bop** ecosystem

---

*Built with Rust for performance, safety, and reliability*
