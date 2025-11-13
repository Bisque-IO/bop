use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;
use tracing::{debug, error, instrument, trace, warn};

use crate::{IoFile, IoVec};

/// Write-ahead log for AOF (Append-Only File) instances.
///
/// The AOF WAL is broken down into segments. There is always 1 tail segment where new records
/// are appended to and 1 segment that is archiving when checkpointing.
///
/// Checkpointing involves sealing the current tail segment and starting a new tail segment.
/// Then, the old tail segment can be archived by merging Slabs (zstd compressed) in with Chunk files
/// and uploading to remote storage.
#[derive(Debug)]
pub struct AofWal {
    last_sequence: AtomicU64,
}

impl AofWal {
    /// Create a new AOF WAL handle.
    #[instrument]
    pub fn new() -> Self {
        debug!("creating new AOF WAL");
        Self {
            last_sequence: AtomicU64::new(0),
        }
    }

    /// Record the latest observed sequence number.
    #[instrument(skip(self))]
    pub fn mark_progress(&self, sequence: u64) {
        trace!(sequence, "marking WAL progress");
        self.last_sequence.store(sequence, Ordering::Relaxed);
    }

    /// Produce a diagnostic snapshot of AOF WAL progress.
    pub fn diagnostics(&self) -> AofWalDiagnostics {
        AofWalDiagnostics {
            last_sequence: self.last_sequence(),
        }
    }

    /// Return the most recent sequence number tracked by this AOF WAL.
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence.load(Ordering::Relaxed)
    }
}

/// Diagnostic snapshot of AOF WAL state.
///
/// Contains high-level progress indicators for monitoring and debugging.
#[derive(Debug, Default, Clone)]
pub struct AofWalDiagnostics {
    /// The most recent sequence number written to the WAL.
    pub last_sequence: u64,
}

/// A chunk of data to be written to the WAL segment.
///
/// `WriteChunk` represents a contiguous byte buffer that can be either:
/// - **Owned**: A standard `Vec<u8>` that owns its allocation
/// - **Raw**: An unsafe raw pointer with custom cleanup, useful for zero-copy writes
///
/// The `Raw` variant allows integration with external memory management systems
/// while ensuring proper cleanup through the provided drop function.
///
/// # Safety
///
/// When using the `Raw` variant:
/// - The pointer must be valid for reads of `len` bytes for the lifetime of the chunk
/// - The `drop` function must be safe to call with the provided `ptr` and `len`
/// - The memory region must not be mutated while the `WriteChunk` exists
pub enum WriteChunk {
    /// Standard heap-allocated byte vector.
    Owned(Vec<u8>),
    /// Raw pointer to external memory with custom cleanup.
    Raw {
        /// Pointer to the start of the byte buffer.
        ptr: *const u8,
        /// Length of the buffer in bytes.
        len: usize,
        /// Custom drop function to clean up the allocation.
        drop: unsafe fn(*const u8, usize),
    },
}

unsafe impl Send for WriteChunk {}
unsafe impl Sync for WriteChunk {}

impl fmt::Debug for WriteChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteChunk::Owned(data) => f.debug_tuple("Owned").field(&data.len()).finish(),
            WriteChunk::Raw { len, .. } => f.debug_tuple("Raw").field(len).finish(),
        }
    }
}

impl Clone for WriteChunk {
    fn clone(&self) -> Self {
        match self {
            WriteChunk::Owned(data) => WriteChunk::Owned(data.clone()),
            WriteChunk::Raw { ptr, len, .. } => {
                // SAFETY: ptr is valid for reads of len bytes
                let slice = unsafe { std::slice::from_raw_parts(*ptr, *len) };
                WriteChunk::Owned(slice.to_vec())
            }
        }
    }
}

impl WriteChunk {
    /// Length of the chunk in bytes.
    pub fn len(&self) -> usize {
        match self {
            WriteChunk::Owned(data) => data.len(),
            WriteChunk::Raw { len, .. } => *len,
        }
    }

    /// Returns true if the chunk has no data.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Produce an immutable I/O view of the chunk.
    pub fn as_io_vec(&self) -> IoVec<'_> {
        match self {
            WriteChunk::Owned(data) => IoVec::new(data.as_slice()),
            WriteChunk::Raw { ptr, len, .. } => {
                // The caller promises the pointer is valid for reads while the chunk is alive.
                // SAFETY: guaranteed by WriteChunk::Raw construction contract.
                let slice = unsafe { std::slice::from_raw_parts(*ptr, *len) };
                IoVec::new(slice)
            }
        }
    }
}

impl Drop for WriteChunk {
    fn drop(&mut self) {
        if let WriteChunk::Raw { ptr, len, drop } = *self {
            // SAFETY: the constructor provided a matching destructor for the allocation.
            unsafe { drop(ptr, len) }
        }
    }
}

/// A batch of write chunks to be written atomically to the WAL.
///
/// `WriteBatch` groups multiple [`WriteChunk`]s together for efficient batched I/O operations.
/// The WAL segment can gather multiple small writes into a single batch to reduce system call
/// overhead and improve write throughput.
pub type WriteBatch = Vec<WriteChunk>;

/// Errors that can occur during write buffer operations.
///
/// The WAL uses a double-buffering scheme to allow concurrent writes and I/O.
/// These errors indicate failures in the buffer staging and swapping logic.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WriteBufferError {
    /// Attempted to stage a buffer when one is already staged for I/O.
    ///
    /// This indicates a logic error where staging was requested before the
    /// previous staged batch was consumed.
    #[error("write buffer already staged for I/O")]
    AlreadyStaged,

    /// Attempted to stage an empty active buffer.
    ///
    /// There's no data to write, so staging should be skipped.
    #[error("active buffer is empty")]
    Empty,

    /// The standby buffer contained unexpected data during a swap operation.
    ///
    /// This indicates a corruption of the double-buffering invariant where
    /// the standby buffer should always be empty before becoming active.
    #[error("standby buffer expected to be empty")]
    StandbyDirty,
}

/// Sentinel value indicating no buffer is currently staged.
const STAGED_NONE: usize = usize::MAX;

/// Double-buffering system for WAL writes to enable concurrent writing and I/O.
///
/// This structure maintains two buffer slots:
/// - **Active buffer**: Receives new write chunks from the application
/// - **Staged buffer**: Holds a batch ready for I/O operations
///
/// The buffers can be atomically swapped, allowing the I/O system to write one
/// buffer while the application continues appending to the other. This minimizes
/// write latency and maximizes throughput.
///
/// # Concurrency
///
/// - Multiple threads can append to the active buffer (protected by mutex)
/// - Staging operations atomically swap active and standby buffers
/// - Only one buffer can be staged at a time
#[derive(Debug)]
struct WalSegmentBuffers {
    /// Two buffer slots for double-buffering.
    slots: [Mutex<WriteBatch>; 2],
    /// Index (0 or 1) of the currently active buffer.
    active: AtomicUsize,
    /// Index of the staged buffer, or [`STAGED_NONE`] if no buffer is staged.
    staged: AtomicUsize,
}

impl WalSegmentBuffers {
    /// Creates a new double-buffer system with both buffers empty.
    fn new() -> Self {
        Self {
            slots: [Mutex::new(Vec::new()), Mutex::new(Vec::new())],
            active: AtomicUsize::new(0),
            staged: AtomicUsize::new(STAGED_NONE),
        }
    }

    /// Executes a closure with exclusive access to the active buffer.
    ///
    /// # Panics
    ///
    /// Panics if the buffer mutex is poisoned.
    fn with_active<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut WriteBatch) -> R,
    {
        let index = self.active.load(Ordering::Acquire);
        let mut guard = self.slots[index].lock().expect("wal buffer mutex poisoned");
        f(&mut guard)
    }

    /// Stages the active buffer for I/O by swapping it with the standby buffer.
    ///
    /// This operation atomically:
    /// 1. Validates that no buffer is currently staged
    /// 2. Validates that the active buffer is not empty
    /// 3. Validates that the standby buffer is empty
    /// 4. Swaps the contents of active and standby buffers
    /// 5. Marks the (now-standby) buffer as staged
    ///
    /// # Returns
    ///
    /// - `Ok(StagedBatchStats)`: Statistics about the staged batch
    /// - `Err(WriteBufferError)`: If staging fails due to buffer state
    ///
    /// # Errors
    ///
    /// - [`WriteBufferError::AlreadyStaged`]: A buffer is already staged
    /// - [`WriteBufferError::Empty`]: The active buffer has no data
    /// - [`WriteBufferError::StandbyDirty`]: The standby buffer is not empty
    fn stage_active(&self) -> Result<StagedBatchStats, WriteBufferError> {
        let staged = self.staged.load(Ordering::Acquire);
        if staged != STAGED_NONE {
            trace!("buffer already staged");
            return Err(WriteBufferError::AlreadyStaged);
        }

        let active_idx = self.active.load(Ordering::Acquire);
        let standby_idx = 1 - active_idx;

        let (mut active_guard, mut standby_guard) = if active_idx == 0 {
            let first = self.slots[0].lock().expect("wal buffer mutex poisoned");
            let second = self.slots[1].lock().expect("wal buffer mutex poisoned");
            (first, second)
        } else {
            let first = self.slots[0].lock().expect("wal buffer mutex poisoned");
            let second = self.slots[1].lock().expect("wal buffer mutex poisoned");
            (second, first)
        };

        if active_guard.is_empty() {
            trace!("active buffer is empty");
            return Err(WriteBufferError::Empty);
        }

        if !standby_guard.is_empty() {
            warn!("standby buffer is not empty");
            return Err(WriteBufferError::StandbyDirty);
        }

        std::mem::swap(&mut *active_guard, &mut *standby_guard);

        let staged_bytes = total_chunk_len(&standby_guard);
        let staged_chunks = standby_guard.len();

        self.staged.store(standby_idx, Ordering::Release);

        trace!(
            bytes = staged_bytes,
            chunks = staged_chunks,
            "staged active buffer"
        );

        Ok(StagedBatchStats {
            bytes: staged_bytes,
            chunks: staged_chunks,
        })
    }

    /// Consumes and returns the staged buffer, if one exists.
    ///
    /// This operation:
    /// 1. Atomically clears the staged buffer marker
    /// 2. Extracts the batch from the previously staged slot
    /// 3. Returns ownership of the batch to the caller
    ///
    /// The returned batch should be written to disk, then either discarded or
    /// restored via [`restore_active`](Self::restore_active) if the write fails.
    ///
    /// # Returns
    ///
    /// - `Some(WriteBatch)`: The staged batch, if one exists
    /// - `None`: If no buffer was staged or it was empty
    fn take_staged(&self) -> Option<WriteBatch> {
        let index = self.staged.swap(STAGED_NONE, Ordering::AcqRel);
        if index == STAGED_NONE {
            return None;
        }

        let mut guard = self.slots[index].lock().expect("wal buffer mutex poisoned");
        if guard.is_empty() {
            return None;
        }
        let batch = std::mem::take(&mut *guard);
        trace!(
            bytes = total_chunk_len(&batch),
            chunks = batch.len(),
            "took staged buffer"
        );
        Some(batch)
    }

    /// Atomically swaps active and standby buffers and takes the data for writing.
    ///
    /// This is an alternative to the stage-then-take pattern, allowing the write
    /// controller to grab data directly without explicit staging. This is useful
    /// for optimistic write scheduling where staging overhead is undesirable.
    ///
    /// # Returns
    ///
    /// - `Some(WriteBatch)`: The active batch, if non-empty
    /// - `None`: If the active buffer is empty
    fn take_active_batch(&self) -> Option<WriteBatch> {
        let active_idx = self.active.load(Ordering::Acquire);
        let standby_idx = 1 - active_idx;

        // Lock both buffers (we need to hold both locks to prevent races)
        let (mut active_guard, _standby_guard) = if active_idx == 0 {
            let first = self.slots[0].lock().expect("wal buffer mutex poisoned");
            let second = self.slots[1].lock().expect("wal buffer mutex poisoned");
            (first, second)
        } else {
            let first = self.slots[0].lock().expect("wal buffer mutex poisoned");
            let second = self.slots[1].lock().expect("wal buffer mutex poisoned");
            (second, first)
        };

        // If active is empty, nothing to take
        if active_guard.is_empty() {
            return None;
        }

        // Swap active to standby atomically
        self.active.store(standby_idx, Ordering::Release);

        // Take the data from what was active (now standby)
        let batch = std::mem::take(&mut *active_guard);
        trace!(
            bytes = total_chunk_len(&batch),
            chunks = batch.len(),
            "took active buffer (swapped)"
        );
        Some(batch)
    }

    /// Returns the total byte length of the staged buffer.
    ///
    /// # Returns
    ///
    /// - `0` if no buffer is staged
    /// - The total byte count across all chunks in the staged buffer
    fn staged_len(&self) -> usize {
        let index = self.staged.load(Ordering::Acquire);
        if index == STAGED_NONE {
            return 0;
        }
        let guard = self.slots[index].lock().expect("wal buffer mutex poisoned");
        total_chunk_len(&guard)
    }

    /// Restores a batch to the active buffer, typically after a failed write.
    ///
    /// If the active buffer already contains data (from concurrent writes),
    /// this method merges the restored batch with the existing data, placing
    /// the restored batch first to maintain write ordering.
    ///
    /// # Arguments
    ///
    /// * `batch` - The write batch to restore to the active buffer
    fn restore_active(&self, mut batch: WriteBatch) {
        let bytes = total_chunk_len(&batch);
        let active_idx = self.active.load(Ordering::Acquire);
        let mut guard = self.slots[active_idx]
            .lock()
            .expect("wal buffer mutex poisoned");
        if guard.is_empty() {
            trace!(
                bytes,
                chunks = batch.len(),
                "restored batch to active buffer"
            );
            *guard = batch;
            return;
        }

        batch.extend(guard.drain(..));
        trace!(
            bytes,
            chunks = batch.len(),
            "restored batch to active buffer (merged)"
        );
        *guard = batch;
    }
}

/// Statistics about a staged write batch.
///
/// Returned by [`WalSegmentBuffers::stage_active`] to provide insight into
/// the size and composition of the staged batch without requiring the caller
/// to take ownership of the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagedBatchStats {
    /// Total size in bytes of all chunks in the batch.
    pub bytes: usize,
    /// Number of individual chunks in the batch.
    pub chunks: usize,
}

/// Tracks whether a segment is enqueued in a work queue and its queue depth.
///
/// This structure provides atomic coordination between the segment state machine
/// and the I/O controller's work queues. It ensures that:
/// - A segment is only enqueued once at a time
/// - Queue depth information is available for scheduling decisions
/// - Queue state can be atomically cleared when work completes
#[derive(Debug, Default)]
struct QueueInstrumentation {
    /// Whether this segment is currently in the queue.
    queued: AtomicBool,
    /// The queue depth at the time of enqueueing.
    depth: AtomicUsize,
}

impl QueueInstrumentation {
    /// Attempts to mark this segment as enqueued.
    ///
    /// # Arguments
    ///
    /// * `depth` - The current depth of the work queue
    ///
    /// # Returns
    ///
    /// - `true` if the segment was successfully enqueued (was not already queued)
    /// - `false` if the segment was already enqueued (prevents duplicate work)
    fn try_enqueue(&self, depth: usize) -> bool {
        match self
            .queued
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {
                self.depth.store(depth, Ordering::Relaxed);
                true
            }
            Err(_) => false,
        }
    }

    /// Clears the enqueued flag and resets the depth counter.
    ///
    /// Called when work completes or is cancelled.
    fn clear(&self) {
        self.queued.store(false, Ordering::Release);
        self.depth.store(0, Ordering::Relaxed);
    }

    /// Updates the recorded queue depth.
    ///
    /// Used by the scheduler to track queue state changes.
    fn update_depth(&self, depth: usize) {
        self.depth.store(depth, Ordering::Relaxed);
    }

    /// Returns whether this segment is currently enqueued.
    fn is_queued(&self) -> bool {
        self.queued.load(Ordering::Acquire)
    }

    /// Returns the queue depth recorded at enqueue time.
    fn depth(&self) -> usize {
        self.depth.load(Ordering::Relaxed)
    }
}

/// State machine for an AOF WAL segment, tracking logical offsets and I/O operations.
///
/// A `AofWalSegment` represents a contiguous region of the write-ahead log on disk.
/// It manages:
/// - **Offset tracking**: Base offset, written bytes, durable (flushed) bytes
/// - **Capacity management**: Preallocated space and pending reservations
/// - **Write buffering**: Double-buffered batching system for efficient I/O
/// - **Queue coordination**: Integration with write and flush work queues
/// - **Performance metrics**: Write rates and operation latencies
///
/// # Segment Lifecycle
///
/// 1. **Creation**: Segment is created with a base offset and preallocated size
/// 2. **Writing**: Application appends chunks to the active buffer
/// 3. **Staging**: Buffer is staged for I/O when a threshold is reached
/// 4. **Write I/O**: Staged batch is written to disk, advancing `written_size`
/// 5. **Flush**: Written data is flushed to durable storage, advancing `durable_size`
/// 6. **Sealing**: Segment is sealed when full or during checkpointing
///
/// # Offset State Machine
///
/// ```text
/// [pending_size] → reserve space for future writes
/// [written_size] → bytes written to OS page cache (not durable)
/// [durable_size] → bytes confirmed on persistent storage (fsync'd)
/// [preallocated_size] → total space reserved for this segment
/// ```
///
/// Invariants:
/// - `durable_size <= written_size <= written_size + pending_size <= preallocated_size`
/// - Offsets never regress (only advance forward)
pub struct AofWalSegment {
    /// I/O operations interface for reading/writing this segment.
    io: Arc<dyn IoFile>,
    /// Starting logical offset of this segment in the global WAL.
    base_offset: u64,
    /// Total bytes preallocated (via fallocate or equivalent).
    preallocated_size: AtomicU64,
    /// Bytes reserved for in-flight writes (not yet written).
    pending_size: AtomicU64,
    /// Bytes successfully written to the file (may not be durable).
    written_size: AtomicU64,
    /// Bytes confirmed as durable on persistent storage.
    durable_size: AtomicU64,
    /// Target offset for the next flush operation.
    pending_flush_target: AtomicU64,

    /// Double-buffering system for write batches.
    buffers: WalSegmentBuffers,

    /// Coordination for write work queue.
    write_queue: QueueInstrumentation,
    /// Coordination for flush work queue.
    flush_queue: QueueInstrumentation,

    /// Size of the most recent write batch (for metrics).
    last_write_batch_bytes: AtomicUsize,
    /// Duration of the most recent flush operation (nanoseconds).
    last_flush_duration_ns: AtomicU64,

    /// Timestamp when this segment was created (microseconds since UNIX epoch).
    created_at_micros: u64,
    /// Cumulative bytes written to this segment.
    total_bytes_written: AtomicU64,
    /// Estimated write throughput in bytes per second.
    write_rate_bytes_per_sec: AtomicU64,
}

impl AofWalSegment {
    /// Creates a new WAL segment with the specified I/O backend and capacity.
    ///
    /// # Arguments
    ///
    /// * `io` - I/O operations interface for this segment's file
    /// * `base_offset` - Starting logical offset in the global WAL
    /// * `preallocated_size` - Initial capacity in bytes (should be preallocated on disk)
    ///
    /// # Returns
    ///
    /// A new segment with all offsets at zero and empty buffers.
    #[instrument(skip(io))]
    pub fn new(io: Arc<dyn IoFile>, base_offset: u64, preallocated_size: u64) -> Self {
        debug!(base_offset, preallocated_size, "creating new WAL segment");
        let created_at_micros = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        Self {
            io,
            base_offset,
            preallocated_size: AtomicU64::new(preallocated_size),
            pending_size: AtomicU64::new(0),
            written_size: AtomicU64::new(0),
            durable_size: AtomicU64::new(0),
            pending_flush_target: AtomicU64::new(0),
            buffers: WalSegmentBuffers::new(),
            write_queue: QueueInstrumentation::default(),
            flush_queue: QueueInstrumentation::default(),
            last_write_batch_bytes: AtomicUsize::new(0),
            last_flush_duration_ns: AtomicU64::new(0),
            created_at_micros,
            total_bytes_written: AtomicU64::new(0),
            write_rate_bytes_per_sec: AtomicU64::new(0),
        }
    }

    /// Returns the total preallocated capacity of this segment in bytes.
    pub fn preallocated_size(&self) -> u64 {
        self.preallocated_size.load(Ordering::Relaxed)
    }

    /// Returns the base logical offset of this segment in the global WAL.
    pub fn base_offset(&self) -> u64 {
        self.base_offset
    }

    /// Returns the current write offset (base + written bytes).
    ///
    /// This represents the next logical offset where data will be written.
    pub fn write_offset(&self) -> u64 {
        self.base_offset
            .saturating_add(self.written_size.load(Ordering::Acquire))
    }

    /// Returns the number of bytes reserved for in-flight writes.
    pub fn pending_size(&self) -> u64 {
        self.pending_size.load(Ordering::Relaxed)
    }

    /// Returns the number of bytes written to the file (may not be durable).
    pub fn written_size(&self) -> u64 {
        self.written_size.load(Ordering::Relaxed)
    }

    /// Returns the number of bytes confirmed as durable on persistent storage.
    pub fn durable_size(&self) -> u64 {
        self.durable_size.load(Ordering::Relaxed)
    }

    /// Returns the number of bytes in the currently staged buffer.
    pub fn staged_bytes(&self) -> usize {
        self.buffers.staged_len()
    }

    /// Increases the preallocated capacity of this segment.
    ///
    /// This should be called after successfully extending the underlying file
    /// with `fallocate` or equivalent.
    ///
    /// # Arguments
    ///
    /// * `bytes` - Additional bytes to add to the preallocated size
    #[instrument(skip(self))]
    pub fn extend_preallocated(&self, bytes: u64) {
        let new_size = self.preallocated_size.fetch_add(bytes, Ordering::AcqRel) + bytes;
        debug!(bytes, new_size, "extended preallocated size");
    }

    /// Reserves capacity for a pending write operation.
    ///
    /// This ensures that the segment has sufficient preallocated space before
    /// buffering write data. The reservation must be released via
    /// [`release_pending`](Self::release_pending) if the write is cancelled,
    /// or converted to written bytes via [`mark_written`](Self::mark_written).
    ///
    /// # Arguments
    ///
    /// * `bytes` - Number of bytes to reserve
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the reservation succeeded
    /// - `Err(AofWalSegmentError)` if insufficient capacity or overflow
    ///
    /// # Errors
    ///
    /// - [`AofWalSegmentError::InsufficientCapacity`]: Not enough preallocated space
    /// - [`AofWalSegmentError::IntegerOverflow`]: Arithmetic overflow
    #[instrument(skip(self))]
    pub fn reserve_pending(&self, bytes: u64) -> Result<(), AofWalSegmentError> {
        loop {
            let written = self.written_size.load(Ordering::Acquire);
            let pending = self.pending_size.load(Ordering::Acquire);
            let preallocated = self.preallocated_size.load(Ordering::Acquire);

            let new_pending = pending
                .checked_add(bytes)
                .ok_or(AofWalSegmentError::IntegerOverflow)?;
            let committed = written
                .checked_add(new_pending)
                .ok_or(AofWalSegmentError::IntegerOverflow)?;

            if committed > preallocated {
                let occupied = written.saturating_add(pending);
                let available = preallocated.saturating_sub(occupied);
                warn!(
                    bytes,
                    available,
                    pending_size = pending,
                    written_size = written,
                    preallocated_size = preallocated,
                    "insufficient capacity for reservation"
                );
                return Err(AofWalSegmentError::InsufficientCapacity { bytes, available });
            }

            match self.pending_size.compare_exchange(
                pending,
                new_pending,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    trace!(
                        bytes,
                        new_pending,
                        pending_size = new_pending,
                        "reserved pending capacity"
                    );
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// Releases a pending capacity reservation.
    ///
    /// Call this when a write operation is cancelled or fails before being
    /// written to disk. The released capacity becomes available for other writes.
    ///
    /// # Arguments
    ///
    /// * `bytes` - Number of bytes to release (must not exceed pending size)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the release succeeded
    /// - `Err(AofWalSegmentError::PendingUnderflow)` if attempting to release more than reserved
    #[instrument(skip(self))]
    pub fn release_pending(&self, bytes: u64) -> Result<(), AofWalSegmentError> {
        loop {
            let current = self.pending_size.load(Ordering::Acquire);
            if current < bytes {
                error!(
                    bytes,
                    current, "pending underflow: attempting to release more than reserved"
                );
                return Err(AofWalSegmentError::PendingUnderflow { current, bytes });
            }
            let next = current - bytes;
            match self.pending_size.compare_exchange(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    trace!(bytes, pending_size = next, "released pending capacity");
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// Marks bytes as successfully written to the file.
    ///
    /// This advances the `written_size` counter and updates write rate metrics.
    /// Written bytes are in the OS page cache but may not yet be durable.
    /// Call [`mark_durable`](Self::mark_durable) after fsyncing.
    ///
    /// # Arguments
    ///
    /// * `bytes` - Number of bytes that were written
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the update succeeded
    /// - `Err(AofWalSegmentError)` if the update violates segment invariants
    ///
    /// # Errors
    ///
    /// - [`AofWalSegmentError::WrittenExceedsPreallocated`]: Write would exceed preallocated size
    /// - [`AofWalSegmentError::IntegerOverflow`]: Arithmetic overflow
    #[instrument(skip(self))]
    pub fn mark_written(&self, bytes: u64) -> Result<(), AofWalSegmentError> {
        loop {
            let current = self.written_size.load(Ordering::Acquire);
            let next = current
                .checked_add(bytes)
                .ok_or(AofWalSegmentError::IntegerOverflow)?;
            if next > self.preallocated_size() {
                error!(
                    bytes,
                    next,
                    preallocated_size = self.preallocated_size(),
                    "written size exceeds preallocated size"
                );
                return Err(AofWalSegmentError::WrittenExceedsPreallocated { next });
            }
            match self.written_size.compare_exchange(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    // Update write rate tracking
                    self.update_write_rate(bytes);
                    debug!(
                        bytes,
                        offset = current,
                        written_size = next,
                        "marked bytes as written"
                    );
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// Updates the write rate metric based on cumulative writes.
    ///
    /// Called internally by [`mark_written`](Self::mark_written) to maintain
    /// an estimate of throughput in bytes per second. This metric can be used
    /// for adaptive chunk sizing and performance monitoring.
    fn update_write_rate(&self, bytes_written: u64) {
        // Update total bytes written
        self.total_bytes_written
            .fetch_add(bytes_written, Ordering::Relaxed);

        // Calculate elapsed time since segment creation
        let now_micros = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let elapsed_micros = now_micros.saturating_sub(self.created_at_micros);
        if elapsed_micros == 0 {
            return;
        }

        // Calculate write rate: bytes per second
        let total_bytes = self.total_bytes_written.load(Ordering::Relaxed);
        let elapsed_secs = elapsed_micros as f64 / 1_000_000.0;
        let rate_bytes_per_sec = (total_bytes as f64 / elapsed_secs) as u64;

        self.write_rate_bytes_per_sec
            .store(rate_bytes_per_sec, Ordering::Relaxed);
    }

    /// Marks bytes as durable on persistent storage.
    ///
    /// This advances the `durable_size` counter after a successful `fsync` or
    /// equivalent operation. Durable bytes are guaranteed to survive system crashes.
    ///
    /// # Arguments
    ///
    /// * `new_durable` - The new durable size (absolute, not delta)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the update succeeded
    /// - `Err(AofWalSegmentError)` if the update violates segment invariants
    ///
    /// # Errors
    ///
    /// - [`AofWalSegmentError::DurableBeyondWritten`]: Durable exceeds written size
    /// - [`AofWalSegmentError::DurableRegression`]: Durable size decreased (forbidden)
    #[instrument(skip(self))]
    pub fn mark_durable(&self, new_durable: u64) -> Result<(), AofWalSegmentError> {
        let written = self.written_size();
        if new_durable > written {
            error!(
                new_durable,
                written_size = written,
                "durable size exceeds written size"
            );
            return Err(AofWalSegmentError::DurableBeyondWritten {
                written,
                new_durable,
            });
        }
        loop {
            let current = self.durable_size.load(Ordering::Acquire);
            if new_durable < current {
                error!(
                    new_durable,
                    durable_size = current,
                    "durable size regression detected"
                );
                return Err(AofWalSegmentError::DurableRegression {
                    current,
                    new_durable,
                });
            }
            match self.durable_size.compare_exchange(
                current,
                new_durable,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    let advanced = new_durable - current;
                    debug!(
                        new_durable,
                        durable_size = new_durable,
                        advanced,
                        "marked bytes as durable"
                    );
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// Executes a closure with exclusive access to the active write buffer.
    ///
    /// Use this to append chunks to the buffer. The closure receives a mutable
    /// reference to the active [`WriteBatch`].
    ///
    /// # Arguments
    ///
    /// * `f` - Closure to execute with the active buffer
    ///
    /// # Returns
    ///
    /// The value returned by the closure
    pub fn with_active_batch<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut WriteBatch) -> R,
    {
        self.buffers.with_active(f)
    }

    /// Stages the active buffer for I/O.
    ///
    /// See [`WalSegmentBuffers::stage_active`] for details.
    ///
    /// # Returns
    ///
    /// - `Ok(StagedBatchStats)`: Statistics about the staged batch
    /// - `Err(WriteBufferError)`: If staging fails
    #[instrument(skip(self))]
    pub fn stage_active_batch(&self) -> Result<StagedBatchStats, WriteBufferError> {
        let stats = self.buffers.stage_active()?;
        debug!(
            bytes = stats.bytes,
            chunks = stats.chunks,
            "staged active batch for write"
        );
        Ok(stats)
    }

    /// Consumes and returns the staged buffer.
    ///
    /// See [`WalSegmentBuffers::take_staged`] for details.
    ///
    /// # Returns
    ///
    /// - `Some(WriteBatch)`: The staged batch
    /// - `None`: If no batch is staged
    #[instrument(skip(self))]
    pub fn take_staged_batch(&self) -> Option<WriteBatch> {
        let batch = self.buffers.take_staged();
        if let Some(ref b) = batch {
            let bytes = total_chunk_len(b);
            trace!(bytes, chunks = b.len(), "took staged batch");
        }
        batch
    }

    /// Atomically takes the active batch for writing (no staging required).
    ///
    /// See [`WalSegmentBuffers::take_active_batch`] for details.
    ///
    /// # Returns
    ///
    /// - `Some(WriteBatch)`: The active batch
    /// - `None`: If the active buffer is empty
    #[instrument(skip(self))]
    pub fn take_active_batch(&self) -> Option<WriteBatch> {
        let batch = self.buffers.take_active_batch();
        if let Some(ref b) = batch {
            let bytes = total_chunk_len(b);
            trace!(bytes, chunks = b.len(), "took active batch");
        }
        batch
    }

    /// Restores a batch to the active buffer after a failed write.
    ///
    /// See [`WalSegmentBuffers::restore_active`] for details.
    ///
    /// # Arguments
    ///
    /// * `batch` - The write batch to restore
    #[instrument(skip(self, batch))]
    pub fn restore_active_batch(&self, batch: WriteBatch) {
        let bytes = total_chunk_len(&batch);
        trace!(bytes, chunks = batch.len(), "restoring active batch");
        self.buffers.restore_active(batch)
    }

    /// Returns a clone of the I/O operations interface for this segment.
    pub fn io(&self) -> Arc<dyn IoFile> {
        self.io.clone()
    }

    /// Attempts to enqueue this segment in the write work queue.
    ///
    /// # Arguments
    ///
    /// * `queue_depth` - Current depth of the write queue
    ///
    /// # Returns
    ///
    /// - `true` if successfully enqueued (was not already queued)
    /// - `false` if already enqueued (prevents duplicate work)
    #[instrument(skip(self))]
    pub fn try_enqueue_write(&self, queue_depth: usize) -> bool {
        let enqueued = self.write_queue.try_enqueue(queue_depth);
        if enqueued {
            trace!(queue_depth, "enqueued for write");
        } else {
            trace!(queue_depth, "already enqueued for write");
        }
        enqueued
    }

    /// Clears the write queue flag, indicating write work has completed.
    #[instrument(skip(self))]
    pub fn clear_write_queue(&self) {
        trace!("clearing write queue");
        self.write_queue.clear();
    }

    /// Updates the recorded write queue depth.
    #[instrument(skip(self))]
    pub fn update_write_queue_depth(&self, depth: usize) {
        trace!(depth, "updated write queue depth");
        self.write_queue.update_depth(depth);
    }

    /// Attempts to enqueue this segment in the flush work queue.
    ///
    /// # Arguments
    ///
    /// * `queue_depth` - Current depth of the flush queue
    ///
    /// # Returns
    ///
    /// - `true` if successfully enqueued (was not already queued)
    /// - `false` if already enqueued (prevents duplicate work)
    #[instrument(skip(self))]
    pub fn try_enqueue_flush(&self, queue_depth: usize) -> bool {
        let enqueued = self.flush_queue.try_enqueue(queue_depth);
        if enqueued {
            trace!(queue_depth, "enqueued for flush");
        } else {
            trace!(queue_depth, "already enqueued for flush");
        }
        enqueued
    }

    /// Clears the flush queue flag, indicating flush work has completed.
    #[instrument(skip(self))]
    pub fn clear_flush_queue(&self) {
        trace!("clearing flush queue");
        self.flush_queue.clear();
    }

    /// Updates the recorded flush queue depth.
    #[instrument(skip(self))]
    pub fn update_flush_queue_depth(&self, depth: usize) {
        trace!(depth, "updated flush queue depth");
        self.flush_queue.update_depth(depth);
    }

    /// Returns whether this segment is currently enqueued for write I/O.
    pub fn is_write_enqueued(&self) -> bool {
        self.write_queue.is_queued()
    }

    /// Returns whether this segment is currently enqueued for flush I/O.
    pub fn is_flush_enqueued(&self) -> bool {
        self.flush_queue.is_queued()
    }

    /// Returns the write queue depth at the time of enqueueing.
    pub fn write_queue_depth(&self) -> usize {
        self.write_queue.depth()
    }

    /// Returns the flush queue depth at the time of enqueueing.
    pub fn flush_queue_depth(&self) -> usize {
        self.flush_queue.depth()
    }

    /// Returns the current write rate in bytes per second.
    pub fn write_rate_bytes_per_sec(&self) -> u64 {
        self.write_rate_bytes_per_sec.load(Ordering::Relaxed)
    }

    /// Returns the timestamp when this segment was created (microseconds since UNIX epoch).
    pub fn created_at_micros(&self) -> u64 {
        self.created_at_micros
    }

    /// Requests that data be flushed up to the specified durable target.
    ///
    /// This method atomically updates the pending flush target, ensuring that
    /// at least `durable_target` bytes will be flushed. If a higher target is
    /// already pending, that target is preserved.
    ///
    /// # Arguments
    ///
    /// * `durable_target` - The desired durable size after flushing
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if a new flush is needed
    /// - `Ok(false)` if the target is already durable or pending
    /// - `Err(AofWalSegmentError)` if the target is invalid
    ///
    /// # Errors
    ///
    /// - [`AofWalSegmentError::DurableBeyondWritten`]: Target exceeds written size
    #[instrument(skip(self))]
    pub fn request_flush(&self, durable_target: u64) -> Result<bool, AofWalSegmentError> {
        let written = self.written_size();
        let durable = self.durable_size();

        if durable_target > written {
            error!(
                durable_target,
                written_size = written,
                "flush target exceeds written size"
            );
            return Err(AofWalSegmentError::DurableBeyondWritten {
                written,
                new_durable: durable_target,
            });
        }

        if durable_target <= durable {
            trace!(
                durable_target,
                durable_size = durable,
                "flush target already durable"
            );
            return Ok(false);
        }

        let previous = self
            .pending_flush_target
            .fetch_max(durable_target, Ordering::AcqRel);

        let needs_flush = durable_target > previous;
        if needs_flush {
            debug!(
                durable_target,
                previous,
                durable_size = durable,
                "flush requested"
            );
        } else {
            trace!(
                durable_target,
                previous, "flush already pending with higher target"
            );
        }
        Ok(needs_flush)
    }

    /// Consumes and returns the pending flush target.
    ///
    /// This atomically clears the flush target, transferring ownership to the
    /// caller. The caller should perform the flush operation, then call
    /// [`mark_durable`](Self::mark_durable) with the flushed size.
    ///
    /// # Returns
    ///
    /// - `Some(u64)` if a flush target was set
    /// - `None` if no flush is pending
    #[instrument(skip(self))]
    pub fn take_flush_target(&self) -> Option<u64> {
        let target = self.pending_flush_target.swap(0, Ordering::AcqRel);
        if target == 0 {
            None
        } else {
            trace!(target, "took flush target");
            Some(target)
        }
    }

    /// Restores a flush target after a failed flush operation.
    ///
    /// If a higher target is already pending, it is preserved.
    ///
    /// # Arguments
    ///
    /// * `target` - The flush target to restore
    #[instrument(skip(self))]
    pub fn restore_flush_target(&self, target: u64) {
        if target == 0 {
            return;
        }
        trace!(target, "restoring flush target");
        self.pending_flush_target
            .fetch_max(target, Ordering::AcqRel);
    }

    /// Returns the current pending flush target, if any.
    pub fn pending_flush_target(&self) -> u64 {
        self.pending_flush_target.load(Ordering::Acquire)
    }

    /// Records the size of the most recent write batch (for metrics).
    pub fn set_last_write_batch_bytes(&self, bytes: usize) {
        self.last_write_batch_bytes.store(bytes, Ordering::Relaxed);
    }

    /// Returns the size of the most recent write batch.
    pub fn last_write_batch_bytes(&self) -> usize {
        self.last_write_batch_bytes.load(Ordering::Relaxed)
    }

    /// Records the duration of the most recent flush operation (for metrics).
    pub fn set_last_flush_duration(&self, duration: Duration) {
        let nanos = duration.as_nanos().min(u64::MAX as u128) as u64;
        self.last_flush_duration_ns.store(nanos, Ordering::Relaxed);
    }

    /// Returns the duration of the most recent flush operation.
    pub fn last_flush_duration(&self) -> Duration {
        Duration::from_nanos(self.last_flush_duration_ns.load(Ordering::Relaxed))
    }

    /// Creates a point-in-time snapshot of this segment's state.
    ///
    /// Useful for monitoring, diagnostics, and making scheduling decisions.
    pub fn snapshot(&self) -> AofWalSegmentSnapshot {
        AofWalSegmentSnapshot {
            preallocated_size: self.preallocated_size(),
            pending_size: self.pending_size(),
            written_size: self.written_size(),
            durable_size: self.durable_size(),
            write_queue_depth: self.write_queue_depth(),
            flush_queue_depth: self.flush_queue_depth(),
            pending_flush_target: self.pending_flush_target(),
            last_write_batch_bytes: self.last_write_batch_bytes(),
            last_flush_duration: self.last_flush_duration(),
            staged_bytes: self.staged_bytes(),
            created_at_micros: self.created_at_micros,
            write_rate_bytes_per_sec: self.write_rate_bytes_per_sec(),
        }
    }
}

impl fmt::Debug for AofWalSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalSegment")
            .field("base_offset", &self.base_offset)
            .field("preallocated_size", &self.preallocated_size())
            .field("pending_size", &self.pending_size())
            .field("written_size", &self.written_size())
            .field("durable_size", &self.durable_size())
            .field("staged_bytes", &self.staged_bytes())
            .finish()
    }
}

/// Point-in-time snapshot of a WAL segment's state.
///
/// This structure captures all key metrics and counters from an [`AofWalSegment`]
/// at a specific moment. It's useful for:
/// - Monitoring and observability
/// - Making scheduling decisions (e.g., which segment to write/flush next)
/// - Diagnostics and debugging
/// - Testing and verification
#[derive(Debug, Clone)]
pub struct AofWalSegmentSnapshot {
    /// Total preallocated capacity in bytes.
    pub preallocated_size: u64,
    /// Bytes reserved for in-flight writes.
    pub pending_size: u64,
    /// Bytes written to the file (may not be durable).
    pub written_size: u64,
    /// Bytes confirmed as durable on persistent storage.
    pub durable_size: u64,
    /// Write queue depth at time of snapshot.
    pub write_queue_depth: usize,
    /// Flush queue depth at time of snapshot.
    pub flush_queue_depth: usize,
    /// Pending flush target, if any.
    pub pending_flush_target: u64,
    /// Size of the most recent write batch.
    pub last_write_batch_bytes: usize,
    /// Duration of the most recent flush operation.
    pub last_flush_duration: Duration,
    /// Bytes currently staged for I/O.
    pub staged_bytes: usize,
    /// Timestamp when the segment was created (microseconds since UNIX epoch).
    pub created_at_micros: u64,
    /// Estimated write throughput in bytes per second.
    pub write_rate_bytes_per_sec: u64,
}

/// Errors that can occur during WAL segment operations.
///
/// These errors represent violations of segment invariants or capacity constraints.
/// All errors are unrecoverable and indicate either programming bugs or resource exhaustion.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AofWalSegmentError {
    /// An arithmetic operation resulted in integer overflow.
    ///
    /// This typically indicates attempting to write beyond addressable limits.
    #[error("integer overflow while updating segment state")]
    IntegerOverflow,

    /// Insufficient preallocated capacity for the requested operation.
    ///
    /// The segment does not have enough space to satisfy a reservation request.
    /// The caller should either extend the segment or create a new one.
    #[error("segment lacks capacity for reservation: requested={bytes} available={available}")]
    InsufficientCapacity {
        /// Bytes requested for reservation.
        bytes: u64,
        /// Bytes currently available.
        available: u64,
    },

    /// Attempted to release more pending bytes than currently reserved.
    ///
    /// This indicates a programming error where reservations and releases are mismatched.
    #[error("pending underflow: current={current} attempted={bytes}")]
    PendingUnderflow {
        /// Current pending size.
        current: u64,
        /// Bytes attempted to release.
        bytes: u64,
    },

    /// Written size would exceed preallocated capacity.
    ///
    /// This indicates an attempt to write beyond the segment's capacity without
    /// first extending it via [`AofWalSegment::extend_preallocated`].
    #[error("written bytes exceed preallocated length: next={next}")]
    WrittenExceedsPreallocated {
        /// The written size that would result.
        next: u64,
    },

    /// Attempted to mark bytes as durable beyond what has been written.
    ///
    /// This violates the invariant: `durable_size <= written_size`.
    #[error("durable advance exceeds written length: written={written} durable={new_durable}")]
    DurableBeyondWritten {
        /// Current written size.
        written: u64,
        /// Attempted new durable size.
        new_durable: u64,
    },

    /// Durable size cannot decrease (time flows forward).
    ///
    /// Once bytes are marked durable, they remain durable. This error indicates
    /// a programming error or corrupted state.
    #[error("durable size regression: current={current} new={new_durable}")]
    DurableRegression {
        /// Current durable size.
        current: u64,
        /// Attempted new durable size (which is less).
        new_durable: u64,
    },
}

/// Calculates the total byte length of all chunks in a batch.
fn total_chunk_len(chunks: &[WriteChunk]) -> usize {
    chunks.iter().map(WriteChunk::len).sum()
}

impl fmt::Display for AofWalSegmentSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AofWalSegmentSnapshot(preallocated={}, pending={}, written={}, durable={}, staged={}, write_queue={}, flush_queue={}, pending_flush_target={}, last_batch={}, last_flush_ns={})",
            self.preallocated_size,
            self.pending_size,
            self.written_size,
            self.durable_size,
            self.staged_bytes,
            self.write_queue_depth,
            self.flush_queue_depth,
            self.pending_flush_target,
            self.last_write_batch_bytes,
            self.last_flush_duration.as_nanos(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{IoResult, IoVecMut};

    #[derive(Debug, Default)]
    struct NoopIoFile;

    impl IoFile for NoopIoFile {
        fn readv_at(&self, _offset: u64, _bufs: &mut [IoVecMut<'_>]) -> IoResult<usize> {
            Ok(0)
        }

        fn writev_at(&self, _offset: u64, bufs: &[IoVec<'_>]) -> IoResult<usize> {
            Ok(bufs.iter().map(IoVec::len).sum())
        }

        fn allocate(&self, _offset: u64, _len: u64) -> IoResult<()> {
            Ok(())
        }

        fn flush(&self) -> IoResult<()> {
            Ok(())
        }
    }

    fn segment(preallocated: u64) -> AofWalSegment {
        AofWalSegment::new(Arc::new(NoopIoFile::default()), 0, preallocated)
    }

    #[test]
    fn buffers_stage_and_restore() {
        let segment = segment(1024);
        segment.with_active_batch(|batch| {
            batch.push(WriteChunk::Owned(vec![1, 2, 3]));
        });

        let stats = segment.stage_active_batch().expect("stage should succeed");
        assert_eq!(stats.bytes, 3);
        assert_eq!(stats.chunks, 1);

        let staged = segment.take_staged_batch().expect("staged batch available");
        assert_eq!(staged.len(), 1);
        assert!(segment.take_staged_batch().is_none());

        segment.restore_active_batch(staged);
        let stats = segment
            .stage_active_batch()
            .expect("restage should succeed");
        assert_eq!(stats.bytes, 3);
        assert_eq!(stats.chunks, 1);
    }

    #[test]
    fn queue_instrumentation_guards_duplicates() {
        let segment = segment(0);
        assert!(segment.try_enqueue_write(1));
        assert!(!segment.try_enqueue_write(2));
        assert_eq!(segment.write_queue_depth(), 1);
        segment.clear_write_queue();
        assert!(segment.try_enqueue_write(2));
        assert_eq!(segment.write_queue_depth(), 2);
    }

    #[test]
    fn reserve_and_release_pending_capacity() {
        let segment = segment(10);
        segment.mark_written(4).unwrap();
        segment.reserve_pending(4).unwrap();
        assert_eq!(segment.pending_size(), 4);
        assert!(segment.reserve_pending(3).is_err());
        segment.release_pending(4).unwrap();
        assert_eq!(segment.pending_size(), 0);
    }

    #[test]
    fn request_and_take_flush_target() {
        let segment = segment(1024);
        segment.mark_written(512).unwrap();
        segment.mark_durable(128).unwrap();

        assert!(segment.request_flush(256).unwrap());
        assert!(!segment.request_flush(200).unwrap());
        assert!(segment.request_flush(400).unwrap());

        assert_eq!(segment.pending_flush_target(), 400);

        assert_eq!(segment.take_flush_target(), Some(400));
        assert_eq!(segment.take_flush_target(), None);

        segment.restore_flush_target(512);
        assert_eq!(segment.pending_flush_target(), 512);
    }

    #[test]
    fn request_flush_rejects_beyond_written() {
        let segment = segment(512);
        segment.mark_written(256).unwrap();
        match segment.request_flush(300) {
            Err(AofWalSegmentError::DurableBeyondWritten {
                written,
                new_durable,
            }) => {
                assert_eq!(written, 256);
                assert_eq!(new_durable, 300);
            }
            other => panic!("unexpected result: {:?}", other),
        }
    }
}
