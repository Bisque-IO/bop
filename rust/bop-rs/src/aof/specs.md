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

### Segment Layout

Segments are append-only, memory-mapped files with durable checkpoints. The on-disk layout is:

```
[SegmentHeader]
[RecordEnvelope 0]
[RecordEnvelope 1]
 ...
[RecordEnvelope N]
[FlushCheckpointStamp 0]
 ...
[FlushCheckpointStamp M]
[SegmentMetadataRecord]
[BinaryIndex]
[SegmentFooter]
```

`RecordEnvelope` describes a single logical record (`RecordHeader` + payload + trailer). `FlushCheckpointStamp` entries accumulate over time; each represents a fully durable flush boundary and supersedes the previous one.

### Segment Header

The header guards segment identity, configuration, and crash recovery hints. Planned structure:

```rust
#[repr(C)]
pub struct SegmentHeader {
    pub magic: u32,           // 0x53454730 ("SEG0")
    pub version: u16,         // Segment format version
    pub flags: u16,           // Compression/encryption indicators
    pub base_id: u64,         // First record ID for this segment
    pub created_at: u64,      // Microseconds since epoch
    pub generation: u64,      // Monotonic segment instance counter
    pub writer_epoch: u64,    // Flush epoch used by recovery tooling
    pub header_checksum: u64, // CRC64-NVME over the preceding fields
}
```

The header checksum is validated before any record scanning. `generation` and `writer_epoch` allow fast detection of stale shadow copies.

### Record Envelope

Each record is written with a header, payload, and trailer. The header matches the live implementation.

```rust
#[repr(C)]
pub struct RecordHeader {
    pub checksum: u32,     // Folded CRC64-NVME of payload (u32 for cache friendliness)
    pub size: u32,         // Payload size in bytes
    pub id: u64,           // Monotonic record ID
    pub timestamp: u64,    // Microseconds since epoch
    pub version: u8,       // Header format version
    pub flags: u8,         // Compression/encryption markers
    pub reserved: [u8; 6], // Future extensions (e.g., TTL)
}
```

To enable O(1) backward iteration and robust crash detection, the payload is followed by a fixed trailer:

```rust
#[repr(C)]
pub struct RecordTrailer {
    pub checksum: u32,     // Mirror of folded CRC64 to validate backward scans
    pub size: u32,         // Mirror of payload size
    pub prev_span: u64,    // Distance in bytes to the previous RecordTrailer (0 for first record)
    pub trailer_crc64: u64,// CRC64-NVME over trailer fields for tamper detection
}
```

Writers flush the header and payload before emitting the trailer. During recovery the trailer is validated first, allowing the scanner to jump backward without re-reading the entire segment.

### Flush Checkpoint Stamp

Every durability boundary appends a stamp describing the highest fully persisted byte. Only the last valid stamp is authoritative.

```rust
#[repr(C)]
pub struct FlushCheckpointStamp {
    pub magic: u32,           // 0xF17C504 ("FCP\0")
    pub version: u16,         // Stamp version
    pub flags: u16,           // Reserved for WAL modes
    pub durable_offset: u64,  // Absolute byte offset after the final committed trailer
    pub last_record_id: u64,  // Highest record ID covered by this flush
    pub data_crc64: u64,      // CRC64-NVME of bytes [SegmentHeader..durable_offset)
    pub stamp_crc64: u64,     // CRC64-NVME over the stamp fields above
}
```

Recovery walks stamps from the end of the file until one verifies; bytes after `durable_offset` are truncated/zeroed before the segment is reopened.

### Segment Metadata Record

The metadata record, stored immediately after the final checkpoint, captures logical information for index rebuilds.

```rust
pub struct SegmentMetadataRecord {
    pub magic: u32,            // 0xABCDEF02 (metadata marker)
    pub record_count: u64,     // Number of envelopes written
    pub last_record_id: u64,   // Highest record ID
    pub base_id: u64,          // First record ID
    pub finalized_at: u64,     // Microseconds since epoch when finalized
    pub data_checksum: u64,    // CRC64-NVME over durable payload bytes
    pub version: u8,           // Metadata format version
    pub reserved: [u8; 7],     // Future expansion
}
```

### Binary Index Format

Binary indexes are optional but recommended for random access and backward iteration. Entries are sorted by `record_id` and may be duplicated for coarse fan-out.

```rust
pub struct BinaryIndexEntry {
    pub record_id: u64,
    pub file_offset: u64,  // Offset of the corresponding RecordHeader
}
```

Readers can perform binary search to reach any record, then rely on trailers for constant-time stepping backward.

### Segment Footer

The footer (`SegmentFooter`) already implemented in `index.rs` authenticates the metadata and index region. Its CRC64 fields tie the entire segment together. The footer is the final write during finalization.

### Crash Recovery Flow

1. Validate `SegmentHeader` checksum; reject the file if it fails.
2. Seek from the end to locate the newest valid `FlushCheckpointStamp`.
3. Truncate/zero bytes beyond `durable_offset`.
4. Scan forward from `SegmentHeader` to `durable_offset`, validating `(RecordHeader, payload, RecordTrailer)` triplets.
5. Recompute the rolling CRC64 using the same folding strategy used by the writer; confirm it matches `data_crc64` in both the checkpoint and metadata record.
6. If any step fails, mark the segment as corrupted and require operator intervention.

This flow prevents partially written records because a trailer is only emitted once the payload is durable, and a checkpoint is only emitted after the trailer is flushed.

### Iteration Semantics

- **Forward iteration**: Readers map the segment, start from `SegmentHeader`, and walk record headers sequentially. Each header contains the payload size and folded checksum.
- **Backward iteration**: Walk begins at the trailer preceding the checkpoint offset, using `prev_span` to jump to the previous record without scanning intermediate data. The mirrored checksum fields guarantee integrity in both directions.
- **Random access**: Binary index entries locate the nearest header; forward/backward stepping completes the read.

Together, the header, trailer, checkpoint, and footer chain ensure that every logical boundary is covered by a CRC64-NVME checksum, enabling deterministic recovery even after power loss.

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
