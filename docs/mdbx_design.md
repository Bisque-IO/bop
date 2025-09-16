# MDBX.rs Design Document

## Overview

The `mdbx.rs` module provides ergonomic Rust wrappers around libmdbx, a high-performance embedded database library. It offers safe, type-safe interfaces for database operations including environments, transactions, cursors, and CRUD operations with proper resource management and error handling.

### Purpose

This module serves as a bridge between the high-performance libmdbx C library and Rust applications, providing:

- **Type Safety**: Strongly-typed database operations with compile-time guarantees
- **Memory Safety**: Automatic resource management with RAII patterns
- **Ergonomic API**: Rust-idiomatic interfaces with Result-based error handling
- **Zero-Copy Operations**: Direct access to database memory without unnecessary allocations
- **ACID Transactions**: Full support for atomic, consistent, isolated, and durable transactions
- **Concurrent Access**: Multi-reader, single-writer concurrency model

### Safety Model

The module implements a "safe(ish)" approach similar to other system-level bindings:

#### Safety Guarantees
- **Resource Management**: Automatic cleanup of MDBX resources via `Drop` implementations
- **Type Safety**: Strong typing for database handles, transactions, and cursors
- **Lifetime Management**: Proper lifetime relationships between environments, transactions, and cursors
- **Memory Safety**: Zero-copy access to database values with lifetime guarantees
- **Thread Safety**: Send/Sync traits where appropriate for concurrent access

#### Safety Limitations
- **FFI Boundaries**: All underlying MDBX calls remain `unsafe` internally
- **Raw Pointers**: MDBX_val structures contain raw pointers to database memory
- **Lifetime Dependencies**: Database values are only valid during transaction lifetime
- **Manual Memory Management**: Some advanced operations require manual memory management

#### Critical Safety Notes
1. **Transaction Lifetime**: Database values (`&[u8]`) are only valid during the transaction that retrieved them
2. **Cursor Ownership**: Cursors are bound to specific transactions and cannot outlive them
3. **Environment Threading**: Single-writer, multi-reader model must be respected
4. **Resource Cleanup**: Proper cleanup order (cursors → transactions → environments)

## Core Architecture

### MDBX_val Pattern

The module extensively uses MDBX's `MDBX_val` structure for key/value representation:

```rust
#[inline]
fn to_val(bytes: &[u8]) -> sys::MDBX_val {
    sys::MDBX_val {
        iov_base: bytes.as_ptr() as *mut c_void,
        iov_len: bytes.len()
    }
}

#[inline]
unsafe fn from_val(val: &sys::MDBX_val) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(val.iov_base as *const u8, val.iov_len)
    }
}
```

**Benefits:**
- **Zero-Copy**: Direct access to database memory without copying
- **Lifetime Safety**: Returned slices are tied to transaction lifetime
- **Performance**: Minimal overhead for data access
- **Compatibility**: Direct mapping to C API expectations

### RAII Resource Management

The module implements comprehensive RAII patterns for all MDBX resources:

```rust
pub struct Env { ptr: *mut sys::MDBX_env }
pub struct Txn<'e> { ptr: *mut sys::MDBX_txn, _marker: PhantomData<&'e mut Env> }
pub struct Cursor<'t> { ptr: *mut sys::MDBX_cursor, _marker: PhantomData<&'t Txn<'t>> }
```

**Resource Hierarchy:**
- **Environment**: Top-level container, manages database files and settings
- **Transaction**: Scoped operations, provides ACID guarantees
- **Cursor**: Positioned access within a transaction
- **DBI**: Database handle for named tables/subdatabases

### Bitflags Configuration System

The module uses bitflags for type-safe configuration:

```rust
bitflags::bitflags! {
    pub struct EnvFlags: i32 {
        const DEFAULTS = sys::MDBX_env_flags_MDBX_ENV_DEFAULTS as i32;
        const RDONLY = sys::MDBX_env_flags_MDBX_RDONLY as i32;
        const WRITEMAP = sys::MDBX_env_flags_MDBX_WRITEMAP as i32;
        // ... additional flags
    }
}
```

**Benefits:**
- **Type Safety**: Compile-time flag validation
- **Ergonomic API**: Easy flag combination with `|` operator
- **FFI Compatibility**: Direct mapping to C bitfield values
- **Extensibility**: Easy addition of new flags

## Core Types

### Environment (Env)

The `Env` type represents a MDBX database environment:

```rust
pub struct Env {
    ptr: *mut sys::MDBX_env,
}
```

**Key Features:**
- **File Management**: Handles database file creation and management
- **Configuration**: Runtime options for max databases, readers, sync behavior
- **Statistics**: Environment and transaction statistics
- **Backup/Copy**: Database backup and copy operations
- **Reader Management**: Thread registration and reader table management

**Design Patterns:**
- **Singleton Pattern**: Typically one environment per database file
- **Builder Pattern**: Configuration through option setting methods
- **Observer Pattern**: Reader list enumeration with callbacks

### Transaction (Txn)

Transactions provide ACID operations with proper lifetime management:

```rust
pub struct Txn<'e> {
    ptr: *mut sys::MDBX_txn,
    _marker: PhantomData<&'e mut Env>,
}
```

**Key Features:**
- **ACID Properties**: Atomic, Consistent, Isolated, Durable operations
- **Read/Write Modes**: Separate read-only and read-write transaction types
- **Nested Transactions**: Parent-child transaction relationships
- **Snapshot Isolation**: MVCC-based concurrency control
- **Canary Values**: Environment metadata storage

**Transaction Types:**
- **Read-Only**: Multiple concurrent readers allowed
- **Read-Write**: Single writer, exclusive access
- **Nested**: Child transactions for complex operations

### Database Handle (Dbi)

DBI represents a named database/table within an environment:

```rust
#[derive(Copy, Clone)]
pub struct Dbi(sys::MDBX_dbi);
```

**Key Features:**
- **Named Databases**: Support for multiple named tables
- **Flags**: DUPSORT, INTEGERKEY, etc. for specialized behavior
- **Statistics**: Per-database statistics and information
- **Lifecycle**: Open/close operations with proper cleanup

### Cursor

Cursors provide positioned access and iteration within transactions:

```rust
pub struct Cursor<'t> {
    ptr: *mut sys::MDBX_cursor,
    _marker: PhantomData<&'t Txn<'t>>,
}
```

**Key Features:**
- **Positioned Access**: Efficient access to specific keys/values
- **Iteration**: Forward/backward traversal with various ordering options
- **DUPSORT Support**: Specialized operations for duplicate keys
- **Bulk Operations**: Multiple value operations for DUPFIXED tables

## Transaction System and ACID Properties

### ACID Implementation

MDBX provides full ACID guarantees through its transaction system:

**Atomicity:**
- All operations within a transaction succeed or fail together
- Transaction commit/abort ensures consistency
- Nested transactions with proper rollback support

**Consistency:**
- Database constraints maintained across operations
- Version consistency through MVCC snapshots
- Corruption detection and recovery mechanisms

**Isolation:**
- Snapshot isolation prevents dirty reads
- Multi-version concurrency control (MVCC)
- Reader/writer separation for high concurrency

**Durability:**
- Configurable sync behavior (durable, no-sync, etc.)
- Write-ahead logging for crash recovery
- Memory-mapped persistence

### Transaction Lifecycle

```rust
// Transaction lifecycle pattern
let txn = Txn::begin(&env, None, TxnFlags::RDONLY)?;
// ... operations ...
txn.commit()?; // or txn.abort()?
```

**Lifecycle States:**
- **Active**: Operations can be performed
- **Committed**: Changes durable, transaction complete
- **Aborted**: Changes discarded, transaction complete
- **Reset**: Read-only transaction can be reused

### Concurrency Model

**Reader/Writer Separation:**
- Multiple read-only transactions can execute concurrently
- Single read-write transaction at a time
- No blocking between readers and writers in most cases

**MVCC Implementation:**
- Writers create new versions without blocking readers
- Readers see consistent snapshot at transaction start
- Garbage collection reclaims old versions

## Error Handling

### Error Type System

The module defines a comprehensive error handling system:

```rust
#[derive(Debug, Clone)]
pub struct Error {
    code: i32,
    message: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorCode {
    Success = sys::MDBX_error_MDBX_SUCCESS as i32,
    NotFound = sys::MDBX_error_MDBX_NOTFOUND as i32,
    KeyExist = sys::MDBX_error_MDBX_KEYEXIST as i32,
    // ... comprehensive error codes
}
```

**Error Categories:**
- **Success Conditions**: Success, ResultTrue for special cases
- **Key/Value Errors**: NotFound, KeyExist for data operations
- **Resource Errors**: MapFull, ReadersFull, TxnFull for capacity issues
- **Corruption/Integrity**: Corrupted, Panic, VersionMismatch
- **Concurrency**: Busy, ThreadMismatch, TxnOverlapping
- **System Errors**: Standard errno values (EIO, ENOMEM, etc.)

### Error Propagation

All operations return `Result<T>` with consistent error handling:

```rust
pub type Result<T> = std::result::Result<T, Error>;

fn check(code: i32) -> Result<()> {
    if code == 0 { Ok(()) } else { Err(mk_error(code)) }
}
```

**Benefits:**
- **Consistency**: Uniform error handling across all operations
- **Rich Information**: Error codes and descriptive messages
- **Interoperability**: Standard Rust error handling patterns
- **Debugging**: Detailed error context for troubleshooting

## Cursor and Iteration Patterns

### Cursor Operations

The module provides comprehensive cursor operations through the `CursorOp` enum:

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum CursorOp {
    First = sys::MDBX_cursor_op_MDBX_FIRST as i32,
    Last = sys::MDBX_cursor_op_MDBX_LAST as i32,
    Next = sys::MDBX_cursor_op_MDBX_NEXT as i32,
    // ... extensive operation set
}
```

**Operation Categories:**
- **Positioning**: First, Last, Set, SetRange
- **Traversal**: Next, Prev, NextNoDup, PrevNoDup
- **DUPSORT**: FirstDup, LastDup, NextDup, PrevDup
- **Range Queries**: SetLowerBound, SetUpperBound
- **Bulk Operations**: GetMultiple, NextMultiple

### Iteration Patterns

**Forward Iteration:**
```rust
let mut cursor = Cursor::open(&txn, dbi)?;
while let Some((key, value)) = cursor.next()? {
    // Process key-value pair
}
```

**Range Queries:**
```rust
let mut cursor = Cursor::open(&txn, dbi)?;
if let Some((key, value)) = cursor.set_range(b"prefix")? {
    // Process range starting point
}
```

**DUPSORT Iteration:**
```rust
let mut cursor = Cursor::open(&txn, dbi)?;
if let Some(_) = cursor.seek_key(b"key")? {
    while let Some((key, value)) = cursor.next_dup()? {
        // Process duplicates for this key
    }
}
```

## Database Configuration and Tuning

### Environment Configuration

**Runtime Options:**
```rust
let mut env = Env::new()?;
env.set_option(OptionKey::MaxDbs, 100)?;
env.set_option(OptionKey::MaxReaders, 1000)?;
```

**Creation Flags:**
```rust
let flags = EnvFlags::DEFAULTS | EnvFlags::WRITEMAP | EnvFlags::NOMETASYNC;
env.open(path, flags, 0o644)?;
```

### Database Flags

**Table Configuration:**
```rust
let dbi = Dbi::open(&txn, Some("mytable"),
    DbFlags::DEFAULTS | DbFlags::DUPSORT | DbFlags::CREATE)?;
```

**Specialized Behaviors:**
- **DUPSORT**: Allow duplicate keys with sorted values
- **INTEGERKEY/INTEGERDUP**: Optimized integer key/value handling
- **REVERSEKEY/REVERSEDUP**: Reverse lexicographic ordering

### Performance Tuning

**Memory Mapping:**
- **WRITEMAP**: Direct write-through to mapped memory
- **NORDAHEAD**: Disable read-ahead for sequential access patterns

**Synchronization:**
- **NOMETASYNC**: Reduce sync overhead for bulk operations
- **SAFE_NOSYNC**: Disable all synchronization (dangerous)

## Memory Management and Zero-Copy Operations

### Zero-Copy Design

The module maximizes zero-copy operations for performance:

**Value Access:**
```rust
pub fn get<'txn>(txn: &'txn Txn<'_>, dbi: Dbi, key: &[u8]) -> Result<Option<&'txn [u8]>> {
    // Returns direct reference to database memory
    // Valid only for transaction lifetime
}
```

**Cursor Operations:**
```rust
pub fn first<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
    // Zero-copy access to key/value pairs
}
```

### Memory Lifetime Management

**Lifetime Guarantees:**
- Database values are valid for the lifetime of their transaction
- Cursor operations return references tied to cursor lifetime
- Environment and transaction cleanup invalidates all associated data

**Safety Patterns:**
```rust
// Correct: value lifetime tied to transaction
let value = get(&txn, dbi, b"key")?;
use_value(value); // Safe while txn is alive

// Incorrect: value outlives transaction
let value = get(&txn, dbi, b"key")?;
drop(txn); // Invalidates value
use_value(value); // Undefined behavior
```

### Memory Pool Management

MDBX uses sophisticated memory management:

**Memory-Mapped Files:**
- Database files mapped into process address space
- Copy-on-write for efficient snapshots
- Automatic page management and prefetching

**Dirty Page Tracking:**
- Only modified pages written to disk
- Efficient commit operations
- Background page eviction

## Concurrent Access Patterns

### Multi-Reader, Single-Writer Model

**Reader Transactions:**
- Multiple read-only transactions can execute concurrently
- No blocking between readers
- Snapshot isolation ensures consistency

**Writer Transactions:**
- Single read-write transaction at a time
- Blocks other writers but not readers
- Exclusive access to modify data

### Thread Safety

**Send/Sync Traits:**
```rust
unsafe impl Send for Env {}  // Can be moved between threads
unsafe impl Sync for Env {}  // Can be shared between threads
```

**Thread Registration:**
```rust
env.thread_register()?;  // Register current thread as reader
env.thread_unregister()?; // Unregister thread
```

### Reader Table Management

**Reader Tracking:**
```rust
env.reader_list(|entry| {
    println!("PID: {}, TXN: {}", entry.pid, entry.txnid);
    0 // Continue enumeration
})?;
```

**Stale Reader Cleanup:**
```rust
let (had_issues, dead_count) = env.reader_check()?;
if had_issues {
    // Handle stale readers
}
```

## Usage Patterns

### Basic CRUD Operations

```rust
// Open environment and database
let env = Env::new()?;
env.open("mydb.mdbx", EnvFlags::DEFAULTS, 0o644)?;
let txn = Txn::begin(&env, None, TxnFlags::RDONLY)?;
let dbi = Dbi::open(&txn, Some("mytable"), DbFlags::DEFAULTS)?;

// CRUD operations
put(&txn, dbi, b"key", b"value", PutFlags::empty())?;
let value = get(&txn, dbi, b"key")?;
del(&txn, dbi, b"key", None)?;

txn.commit()?;
```

### Cursor-Based Iteration

```rust
let txn = Txn::begin(&env, None, TxnFlags::RDONLY)?;
let dbi = Dbi::open(&txn, Some("mytable"), DbFlags::DEFAULTS)?;
let mut cursor = Cursor::open(&txn, dbi)?;

// Iterate all key-value pairs
while let Some((key, value)) = cursor.next()? {
    process_key_value(key, value);
}
```

### Transaction Patterns

**Read-Only Transaction:**
```rust
let txn = Txn::begin(&env, None, TxnFlags::RDONLY)?;
// ... read operations ...
txn.reset()?;  // Reuse for another read
txn.renew()?;  // Refresh snapshot
```

**Read-Write Transaction:**
```rust
let txn = Txn::begin(&env, None, TxnFlags::RDONLY)?;
// ... operations ...
if success {
    txn.commit()?;
} else {
    txn.abort()?;
}
```

### DUPSORT Operations

```rust
// Insert duplicates
put(&txn, dbi, b"key1", b"value1", PutFlags::empty())?;
put(&txn, dbi, b"key1", b"value2", PutFlags::empty())?;

// Iterate duplicates
let mut cursor = Cursor::open(&txn, dbi)?;
cursor.seek_key(b"key1")?;
while let Some((key, value)) = cursor.next_dup()? {
    // Process each duplicate
}
```

## Design Rationale and Trade-offs

### Zero-Copy vs. Safety

**Rationale:**
- MDBX's memory-mapped architecture enables zero-copy access
- Database values are stable during transaction lifetime
- Performance-critical for high-throughput applications

**Trade-offs:**
- Complex lifetime management required
- Raw pointers in MDBX_val structures
- Transaction lifetime dependencies

### RAII Resource Management

**Rationale:**
- Automatic resource cleanup prevents leaks
- Proper cleanup order ensures consistency
- Exception safety through Drop implementations

**Trade-offs:**
- Drop order dependencies between resources
- Cannot leak resources for manual management
- Potential double-free if misused

### Bitflags Configuration

**Rationale:**
- Type-safe flag combinations
- Compile-time validation of flag usage
- Ergonomic API for complex configurations

**Trade-offs:**
- Runtime overhead for flag checking
- Cannot extend flags without recompilation
- Bitflag arithmetic can be error-prone

### Comprehensive Cursor Operations

**Rationale:**
- MDBX cursor API is operation-rich
- Efficient positioned access patterns
- Support for complex iteration requirements

**Trade-offs:**
- Large enum with many variants
- Complex operation semantics
- Steep learning curve for new users

## Future Considerations

### Potential Improvements

1. **Async Support**: Futures-based API for modern Rust async patterns
2. **ORM Integration**: Higher-level object mapping capabilities
3. **Advanced Indexing**: Secondary indexes and complex queries
4. **Replication**: Multi-node replication support
5. **Metrics/Observability**: Built-in performance monitoring and metrics
6. **Backup/Restore**: Enhanced backup and restore capabilities

### Compatibility

- **Rust Version**: Requires Rust 1.40+ for const generics in some features
- **MDBX Version**: Compatible with MDBX 0.11+
- **Platform Support**: Windows, Linux, macOS via bop-sys bindings

## Conclusion

The `mdbx.rs` module provides a comprehensive, safe(ish) Rust interface to the MDBX embedded database library while maintaining high performance and low overhead. The design balances safety, ergonomics, and performance through careful use of Rust's type system, lifetime management, and strategic unsafe code blocks.

The zero-copy architecture, RAII resource management, and comprehensive cursor operations form the core architectural elements that enable high-performance, ACID-compliant database operations with minimal runtime overhead.