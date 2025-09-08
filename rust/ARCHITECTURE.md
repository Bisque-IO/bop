# BOP Rust Implementation Architecture

## Overview

BOP (Bisque Operations Platform) is a data platform for modern computing built with Rust, providing distributed cluster management capabilities compatible with Vert.x 3. This document outlines the current architecture, design patterns, and features of the Rust implementation.

## Project Structure

```
rust/
├── Cargo.toml              # Workspace configuration
├── bop-sys/                # C FFI bindings and system integration
│   ├── Cargo.toml
│   ├── ffi/bindings.h      # C header definitions
│   └── src/lib.rs          # Rust FFI bindings
└── bop-rs/                 # Main Rust implementation
    ├── Cargo.toml
    ├── src/
    │   ├── main.rs         # Binary entry point and tests
    │   ├── lib.rs          # Library API and exports
    │   ├── fsm.rs          # Finite State Machine and core logic
    │   └── wire.rs         # Wire protocol implementation
    └── tests/              # Integration tests
        ├── ffi_smoke.rs
        └── smoke.rs
```

## Core Components

### 1. Message-Based Wire Protocol (`wire.rs`)

The wire protocol implements a unified message system that replaces the legacy Request/Response pattern with a single `Message` enum.

#### Key Features:
- **Unified Message Enum**: Single enum handles both requests and responses
- **Action ID-based Serialization**: Messages are serialized/deserialized using unique action IDs
- **CRC32 Checksums**: Built-in data integrity validation
- **Cross-platform Compatibility**: Little-endian byte order for consistent behavior
- **State Synchronization**: Comprehensive sync operations for client-side caching

#### Message Types:
```rust
pub enum Message {
    // Cluster Management
    JoinRequest { node_id: String },
    JoinResponse { connection_id: u64 },
    LeaveRequest { node_id: String },
    LeaveResponse,
    GetNodesRequest,
    GetNodesResponse { nodes: Vec<String> },
    
    // AsyncMap Operations
    CreateMapRequest { name: String },
    CreateMapResponse { map_name: String },
    MapPutRequest { map_name: String, key: Vec<u8>, value: Vec<u8> },
    MapPutResponse,
    MapGetRequest { map_name: String, key: Vec<u8> },
    MapGetResponse { value: Option<Vec<u8>> },
    
    // AsyncSet Operations
    CreateSetRequest { name: String },
    SetAddRequest { set_name: String, value: Vec<u8> },
    SetContainsResponse { contains: bool },
    
    // AsyncCounter Operations
    CreateCounterRequest { name: String },
    CounterIncrementRequest { counter_name: String, delta: i64 },
    CounterGetResponse { value: i64 },
    
    // AsyncLock Operations
    CreateLockRequest { name: String },
    LockAcquireRequest { lock_name: String },
    LockAcquireResponse,
    LockReleaseRequest { lock_name: String },
    LockSyncRequest { held_locks: Vec<String> },
    
    // AsyncMultimap Operations (Vert.x 3 Compatible)
    CreateMultimapRequest { name: String },
    MultimapPutRequest { multimap_name: String, key: Vec<u8>, value: Vec<u8> },
    MultimapGetResponse { values: HashMap<Vec<u8>, HashSet<Vec<u8>>> },
    
    // System Messages
    Ping,
    Pong,
    ErrorResponse { message: String },
    
    // State Synchronization
    StateSync { sync_id: u64, operations: Vec<StateSyncOperation> },
    StateSyncAck { sync_id: u64 },
}
```

#### Wire Protocol Format:
```
[CRC32 Checksum: 4 bytes] [Message Size: 4 bytes] [Protocol Version: 1 byte] 
[Flags: 1 byte] [Action ID: 2 bytes] [Request ID: 4 bytes] [Payload: Variable]
```

#### State Synchronization Operations:
The system supports 21 different state synchronization operations for comprehensive client-side caching:
- Map operations (created, updated, removed, cleared)
- Set operations (created, added, removed, cleared) 
- Counter operations (created, incremented)
- Lock operations (created, acquired, released, expired)
- Multimap operations (created, put, removed, cleared)
- Node management (added, removed, connection events)

### 2. Finite State Machine (`fsm.rs`)

The FSM manages the distributed state and handles message processing.

#### Core State Components:
```rust
pub struct State {
    pub nodes: Mutex<Vec<String>>,                    // Active cluster nodes
    pub maps: Mutex<HashMap<String, HashMap<Vec<u8>, Vec<u8>>>>,  // AsyncMap storage
    pub sets: Mutex<HashMap<String, HashSet<Vec<u8>>>>,          // AsyncSet storage  
    pub counters: Mutex<HashMap<String, i64>>,                   // AsyncCounter storage
    pub multimaps: Mutex<HashMap<String, HashMap<Vec<u8>, HashSet<Vec<u8>>>>>, // Vert.x 3 compatible
    pub raft_node: Mutex<RaftNode>,                             // Raft consensus state
    pub node_manager: NodeManager,                              // Connection management
    pub lock_manager: LockManager,                              // Distributed locking
    pub state_sync_manager: StateSyncManager,                   // Client sync coordination
}
```

#### Raft Integration:
- **Leader Election**: Supports Leader, Follower, and Learner roles
- **Consensus Protocol**: Built-in Raft state management
- **Leadership Validation**: Operations requiring consensus are leader-only
- **State Replication**: Automatic state synchronization across cluster

#### Node Management:
```rust
pub struct NodeManager {
    connections: Mutex<HashMap<u64, Connection>>,     // Active connections
    node_connections: Mutex<HashMap<String, Vec<u64>>>, // Node->Connection mapping
    connection_counter: AtomicU64,                    // Unique connection IDs
    timeout_duration: Duration,                       // Connection timeout
}
```

#### Features:
- **Connection Lifecycle**: Automatic connection creation, tracking, and cleanup
- **Timeout Processing**: Configurable connection timeouts with automatic cleanup
- **Ping/Pong Protocol**: Keep-alive mechanism for connection health
- **Statistics Tracking**: Real-time connection and node statistics

#### Distributed Locking:
```rust
pub struct LockManager {
    locks: Mutex<HashMap<String, DistributedLock>>,   // Active locks
    lock_counter: AtomicU64,                          // Unique lock IDs
}

pub struct DistributedLock {
    pub id: u64,
    pub name: String,
    pub owner: Option<String>,                        // Current lock holder
    pub queue: Vec<String>,                          // Waiting nodes
    pub created_at: Instant,
}
```

#### Features:
- **Queued Locking**: Fair queuing system for lock acquisition
- **Lock Synchronization**: Bulk lock validation for connection recovery
- **Ownership Tracking**: Node-based lock ownership
- **Automatic Cleanup**: Lock release on node disconnection

### 3. Application API (`lib.rs`)

The main application interface provides a clean API for cluster operations.

```rust
pub struct App {
    state: Arc<State>,
}

impl App {
    pub fn new() -> Self
    pub fn new_with_raft_node(node_id: i32, address: String) -> Self
    pub fn process_message(&self, message: FramedMessage<Message>, connection_id: Option<u64>, node_id: Option<String>) -> FramedMessage<Message>
    pub fn create_timeout_processor(&self) -> impl Fn() -> usize + '_
    pub fn state(&self) -> &Arc<State>
}
```

### 4. FFI Integration (`bop-sys`)

Provides C-compatible FFI bindings for cross-language integration.

#### Key Functions:
```rust
#[no_mangle] pub extern "C" fn create_app() -> *mut App
#[no_mangle] pub extern "C" fn create_app_with_raft_node(node_id: i32, address: *const c_char) -> *mut App
#[no_mangle] pub extern "C" fn process_request_bytes(app: *mut App, request_bytes: *const u8, len: usize) -> *mut c_char
#[no_mangle] pub extern "C" fn free_response(response: *mut c_char)
#[no_mangle] pub extern "C" fn free_app(app: *mut App)
```

## Design Patterns

### 1. Message-Centric Architecture
- Single unified message type eliminates request/response duplication
- Action ID-based dispatching enables efficient routing
- Structured request/response pairing maintains type safety

### 2. String-Based Resource Naming
- Maps, sets, counters, locks, and multimaps use string names instead of numeric IDs
- Better compatibility with Vert.x 3 naming conventions
- More intuitive for client applications

### 3. HashSet-Based Multimap Values
- Multimaps use `HashMap<Vec<u8>, HashSet<Vec<u8>>>` for proper Vert.x 3 compatibility  
- Prevents duplicate values per key
- Efficient set operations (contains, add, remove)

### 4. Comprehensive State Synchronization
- 21 different sync operation types cover all state mutations
- Client-side caching support through state sync messages
- Atomic sync batches with acknowledgment protocol

### 5. Arc-Based State Sharing
- Thread-safe state sharing using `Arc<State>`
- Fine-grained locking with individual `Mutex` guards
- Minimized lock contention through targeted locking

### 6. Connection-Based Resource Tracking
- Resources tied to connection IDs for automatic cleanup
- Connection timeout processing for fault tolerance
- Ping/pong keep-alive protocol

## Current Implementation Status

### ✅ Completed Features:

1. **Unified Message Protocol**
   - Complete Message enum with 50+ message types
   - Action ID-based serialization/deserialization
   - CRC32 checksum validation
   - Cross-platform compatibility

2. **Core Data Structures**
   - AsyncMap with string-based naming
   - AsyncSet with HashSet storage
   - AsyncCounter with atomic operations
   - AsyncMultimap with Vert.x 3 compatible structure
   - Distributed locking with queuing

3. **State Management**
   - Raft node integration
   - Leader/follower role management
   - Connection lifecycle management
   - Timeout processing

4. **State Synchronization**
   - 21 sync operation types
   - StateSyncManager implementation
   - Atomic sync batching

5. **Testing Infrastructure**
   - 105+ comprehensive tests
   - Wire protocol round-trip tests
   - Integration tests for all major features
   - FFI smoke tests

6. **FFI Bindings**
   - C-compatible interface
   - Memory management
   - Cross-language serialization

### 🚧 In Progress:

1. **Message Processing Logic**
   - Current `process_message` returns `ErrorResponse` for non-ping messages
   - Full business logic implementation needed
   - Leadership validation for mutation operations

### 📋 Future Enhancements:

1. **Advanced Raft Features**
   - Log replication
   - Snapshot management
   - Dynamic membership changes

2. **Performance Optimizations**
   - Connection pooling
   - Batch message processing
   - Memory-mapped storage

3. **Monitoring & Observability**
   - Metrics collection
   - Health checks
   - Performance profiling

## Testing Strategy

### Unit Tests (87 tests in `wire.rs`):
- Message serialization/deserialization round-trips
- All message type variants
- Error handling and edge cases
- Checksum validation
- FramedMessage construction

### Integration Tests (12 tests in `main.rs`):
- Cluster operations (join/leave/get-nodes)
- Map operations (create/put/get/remove/size)
- Distributed locking (acquire/release/sync/queue)
- Leader/follower behavior
- Connection management and timeouts
- State snapshots
- Wire protocol integration

### System Tests (4 tests in `fsm.rs`):
- State persistence (save/load from file)
- Snapshot round-trip validation
- Error handling in file operations

## Dependencies

### Core Dependencies:
- `multimap = "0.8"` - Multimap data structure
- `serde = { version = "1.0", features = ["derive"] }` - Serialization framework
- `serde_json = "1.0"` - JSON serialization for snapshots
- `base64 = "0.21"` - Base64 encoding for binary data
- `byteorder = "1.5"` - Endian-aware binary I/O
- `crc32fast = "1.4"` - Fast CRC32 checksums

### System Integration:
- `bop-sys` - Local C FFI bindings for system integration

## Performance Characteristics

### Memory Usage:
- State stored in memory with periodic snapshots
- Connection-based resource cleanup prevents memory leaks
- HashMap/HashSet structures for O(1) lookups

### Network Protocol:
- Binary wire protocol with minimal overhead
- CRC32 checksums for integrity (4-byte overhead)
- Variable-length encoding for strings and byte arrays

### Concurrency:
- Fine-grained locking minimizes contention
- Arc-based sharing enables multiple readers
- Atomic counters for high-frequency operations

## Security Considerations

### Data Integrity:
- CRC32 checksums on all wire messages
- Protocol version validation
- Bounds checking on all deserialization

### Resource Management:
- Connection timeouts prevent resource exhaustion
- Automatic cleanup of abandoned resources
- Memory-safe Rust implementation

### Access Control:
- Leader-only operations for cluster mutations
- Connection-based resource isolation
- Node-based lock ownership validation

## Compatibility

### Vert.x 3 Compatibility:
- String-based resource naming matches Vert.x conventions
- HashSet multimap values prevent duplicates
- Wire protocol supports all Vert.x 3 cluster manager operations

### Cross-Platform:
- Little-endian byte order for consistent serialization
- C FFI bindings for language interoperability
- Platform-agnostic Rust implementation

### Version Compatibility:
- Protocol version field enables future evolution
- Backward-compatible message format
- Graceful error handling for unknown message types

---

*Last updated: 2025-01-08*
*Implementation: BOP Rust v0.1.0*