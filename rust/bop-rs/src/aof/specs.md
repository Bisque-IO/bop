# AOF (Append-Only File) Design Specifications

## Architecture Overview

The AOF system is designed as a high-performance, thread-safe append-only log with the following key characteristics:

- **Synchronous append operations** with memory-mapped I/O for zero-copy writes
- **Five independent background processes** for system maintenance
- **Thread-safe reader creation** allowing readers from multiple threads
- **Direct memory-mapped reading** with Arc<Segment> references
- **Comprehensive error handling** with backpressure mechanisms
- **Durable indexing** through segment metadata records

## Core Components

### 1. AOF Main Structure

```rust
pub struct Aof<FS: FileSystem> {
    config: AofConfig,
    fs: Arc<FS>,
    active_segment: Option<ActiveSegment<FS>>,
    segments: HashMap<u64, Arc<Segment<FS>>>,
    next_record_id: AtomicU64,
    background_tasks: BackgroundTaskChannels,
    shared_state: Arc<SharedAofState>,
}
```

**Key Design Decisions:**
- `append()` takes `&mut self` for data integrity
- Reader creation methods take `&self` for thread-safety
- Uses `Arc<Segment<FS>>` for shared segment access
- Atomic operations for thread-safe ID generation

### 2. Background Task Architecture

Five independent background processes run on separate tokio tasks:

#### Process 1: Pre-allocation (Critical Priority)
- **Purpose**: Pre-allocate next segment when current segment reaches threshold
- **Trigger**: Segment reaches `pre_allocate_threshold` (default 80% full)
- **Error Handling**: Failure blocks `append()` operations until resolved
- **Implementation**: Blocking task on tokio thread pool

#### Process 2: Segment Finalization (High Priority)
- **Purpose**: Finalize rotated ActiveSegments, write segment metadata
- **Trigger**: ActiveSegment rotation
- **Error Handling**: Failure stops forward progress in `append()`
- **Implementation**: Blocking task on tokio thread pool

#### Process 3: Segment Flushing (Medium Priority)
- **Purpose**: Flush segment data to disk, write durable index
- **Trigger**: Segment finalization completion
- **Error Handling**: Failure stops forward progress in `append()`
- **Implementation**: Blocking task on tokio thread pool

#### Process 4: Archiving (Low Priority)
- **Purpose**: Compress and archive old segments incrementally
- **Trigger**: Number of segments exceeds `archive_threshold`
- **Error Handling**: Errors accumulate until backpressure threshold reached
- **Implementation**: Blocking task on tokio thread pool

#### Process 5: Reader Notification (Async)
- **Purpose**: Notify tailing readers of new data
- **Trigger**: Successful `append()` operations
- **Error Handling**: Best-effort delivery, failures ignored
- **Implementation**: Async task, max 1 in-flight message

### 3. Background Task Communication

```rust
pub struct BackgroundTaskChannels {
    pre_allocation_tx: mpsc::Sender<PreAllocationRequest>,
    finalization_tx: mpsc::Sender<FinalizationRequest>,
    flush_tx: mpsc::Sender<FlushRequest>,
    archive_tx: mpsc::Sender<ArchiveRequest>,
    reader_notify_tx: mpsc::Sender<ReaderNotifyRequest>,
}

pub struct SharedAofState {
    pre_allocation_error: Arc<Mutex<Option<AofError>>>,
    finalization_error: Arc<Mutex<Option<AofError>>>,
    flush_error: Arc<Mutex<Option<AofError>>>,
    archive_error_count: AtomicU64,
    archive_queue_segments: AtomicU64,
    archive_queue_bytes: AtomicU64,
}
```

## Append Operation Flow

### Synchronous Append with Backpressure

```rust
pub fn append(&mut self, data: &[u8]) -> AofResult<u64> {
    // 1. Check background task errors
    self.check_background_task_errors()?;

    // 2. Check archive backpressure (if enabled)
    if self.config.archive_enabled {
        self.check_archive_backpressure()?;
    }

    // 3. Ensure active segment is ready
    let active_segment = match &mut self.active_segment {
        Some(active) if active.is_mmap_ready() => active,
        _ => {
            self.trigger_pre_allocation();
            return Err(AofError::WouldBlock("No active segment ready".to_string()));
        }
    };

    // 4. Check if segment needs rotation
    if active_segment.needs_rotation(data.len() as u64) {
        self.rotate_segment()?;
        return self.append(data); // Retry with new segment
    }

    // 5. Write record to memory-mapped segment
    let record_id = self.next_record_id.fetch_add(1, Ordering::SeqCst);
    active_segment.append_record(record_id, data)?;

    // 6. Trigger background processes
    self.trigger_pre_allocation_if_needed();
    self.trigger_reader_notification(record_id);

    Ok(record_id)
}
```

### Error Types and Backpressure

- **WouldBlock**: Non-blocking operation cannot proceed immediately
- **Backpressure**: System overloaded, must wait for background processes
- **Critical Errors**: Pre-allocation, finalization, flush failures stop progress
- **Archive Errors**: Accumulate until configured limits exceeded

## Reader Architecture

### Thread-Safe Reader Creation

```rust
impl<FS: FileSystem> Aof<FS> {
    pub fn create_reader(&self, start_id: u64) -> AofResult<Reader<FS>> {
        // Thread-safe reader creation using &self
    }

    pub fn create_tailing_reader(&self, start_id: u64) -> AofResult<Reader<FS>> {
        // Thread-safe tailing reader creation
    }
}
```

### Reader Structure with Arc<Segment>

```rust
pub struct Reader<FS: FileSystem> {
    pub id: u64,
    current_segment: Option<Arc<Segment<FS>>>,
    segment_offset: AtomicU64,
    record_id: AtomicU64,
    pub is_tailing: AtomicBool,
    tailing_notify: Option<Arc<Notify>>,
}
```

**Key Features:**
- **Arc<Segment<FS>> references**: Shared ownership of segments
- **Direct mmap reading**: Zero-copy reads from memory-mapped files
- **Atomic positioning**: Thread-safe offset and ID tracking
- **Tailing support**: Real-time notification of new data

### Reader Operation Flow

```rust
pub fn read_next_record(&mut self) -> AofResult<Option<(u64, Vec<u8>)>> {
    // 1. Ensure current segment is loaded
    // 2. Read directly from segment mmap
    // 3. Handle segment transitions
    // 4. Detect end-of-file vs. incomplete data
    // 5. For tailing: wait for notifications if no data
}
```

## File Format Specification

### Segment Structure

```
[Record 1][Record 2]...[Record N][Metadata Record][Binary Index][Footer]
```

### Record Format

```rust
#[repr(C)]
pub struct RecordHeader {
    pub checksum: u32,     // CRC32 of record data
    pub size: u32,         // Size of record data
    pub id: u64,           // Monotonic record ID
    pub timestamp: u64,    // Microseconds since epoch
    pub version: u8,       // Format version
    pub flags: u8,         // Compression, encryption flags
    pub reserved: [u8; 6], // Future use
}
```

### Segment Metadata Record (Durable Index)

```rust
pub struct SegmentMetadataRecord {
    pub magic: u32,            // 0xABCDEF02 (identifies metadata record)
    pub record_count: u64,     // Number of records in segment
    pub last_record_id: u64,   // Last record ID in segment
    pub base_id: u64,          // First record ID in segment
    pub finalized_at: u64,     // Finalization timestamp
    pub data_checksum: u64,    // Checksum of all record data
    pub version: u8,           // Metadata version
    pub reserved: [u8; 7],     // Future use
}
```

**Purpose:**
- **Durable indexing**: Provides segment bounds without scanning
- **EOF detection**: Distinguishes complete vs. incomplete segments
- **Integrity verification**: Data checksum for corruption detection
- **Tailing support**: Enables efficient tail detection

### Binary Index Format

```rust
pub struct BinaryIndexEntry {
    pub record_id: u64,
    pub file_offset: u64,
}
```

Optional binary index for O(log n) record lookup within segments.

## Thread Safety Guarantees

### Concurrent Operations

1. **Multiple readers**: Safe to create and use readers from different threads
2. **Single writer**: Only one `append()` operation at a time (requires `&mut self`)
3. **Background tasks**: Independent tasks with proper synchronization
4. **Segment sharing**: Arc<Segment> enables safe shared access

### Synchronization Mechanisms

- **Atomic operations**: For counters, flags, and simple state
- **Mutex protection**: For complex shared state
- **Channel communication**: For background task coordination
- **Memory mapping**: OS-level synchronization for file access

## Configuration Options

### Core Settings

```rust
pub struct AofConfig {
    pub segment_size: u64,                    // Default: 64MB
    pub flush_strategy: FlushStrategy,        // Batched or periodic
    pub segment_cache_size: usize,           // LRU cache size
    pub enable_compression: bool,            // zstd compression
    pub enable_encryption: bool,             // Future: encryption support
    pub ttl_seconds: Option<u64>,           // Record TTL
}
```

### Pre-allocation Settings

```rust
pub pre_allocate_threshold: f32,     // Default: 0.8 (80% full)
pub pre_allocate_enabled: bool,      // Default: true
```

### Archive Settings

```rust
pub archive_enabled: bool,                   // Default: false
pub archive_compression_level: i32,          // Default: 3 (zstd level)
pub archive_threshold: u64,                  // Default: 100 segments
pub archive_backpressure_segments: u64,      // Default: 500 segments
pub archive_backpressure_bytes: u64,         // Default: 10GB
```

## Performance Characteristics

### Write Performance

- **Memory-mapped I/O**: Zero-copy writes directly to memory
- **Batch flushing**: Configurable flush strategies
- **Pre-allocation**: Eliminates file growth overhead
- **Asynchronous background tasks**: Non-blocking maintenance

### Read Performance

- **Memory-mapped reads**: Zero-copy data access
- **Binary indexing**: O(log n) record lookup
- **Segment caching**: LRU cache for hot segments
- **Direct mmap access**: Bypass kernel copies

### Memory Usage

- **Segment caching**: Configurable LRU cache size
- **Lazy loading**: Load segments on demand
- **Arc sharing**: Shared segment references reduce memory
- **Compression**: Optional zstd compression for archives

## Error Handling Strategy

### Error Categories

1. **Critical Errors**: Stop forward progress immediately
   - Pre-allocation failures
   - Finalization failures
   - Flush failures

2. **Recoverable Errors**: Retry or graceful degradation
   - Temporary I/O errors
   - Memory allocation failures
   - Lock contention

3. **Background Errors**: Accumulate until threshold
   - Archive failures
   - Reader notification failures

4. **Backpressure**: Signal system overload
   - Pre-allocation not ready
   - Archive queue full
   - Memory pressure

### Error Propagation

```rust
pub enum AofError {
    WouldBlock(String),      // Non-blocking operation cannot proceed
    Backpressure(String),    // System overloaded
    InternalError(String),   // Lock poisoning, etc.
    CorruptedRecord(String), // Data integrity violation
    // ... other variants
}
```

## Future Enhancements

### Planned Features

1. **Encryption**: Record-level encryption support
2. **Compression**: Per-record compression options
3. **Remote Storage**: Cloud storage backends
4. **Replication**: Multi-node replication
5. **Transactions**: Multi-record atomic operations

### Scalability Improvements

1. **Parallel segments**: Multiple active segments per AOF
2. **NUMA awareness**: Thread and memory locality
3. **Tiered storage**: Hot/cold data separation
4. **Incremental compaction**: Online segment merging

## Operational Considerations

### Monitoring

- Background task health
- Archive queue depth
- Segment cache hit rates
- Error rates by category

### Maintenance

- Segment cleanup policies
- Archive retention policies
- Index rebuild procedures
- Corruption recovery tools

### Configuration Tuning

- Segment size optimization
- Cache size tuning
- Flush strategy selection
- Backpressure threshold tuning