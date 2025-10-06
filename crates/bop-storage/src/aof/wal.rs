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

#[derive(Debug, Default, Clone)]
pub struct AofWalDiagnostics {
    pub last_sequence: u64,
}

/// Owned chunk of WAL bytes scheduled for I/O.
pub enum WriteChunk {
    Owned(Vec<u8>),
    Raw {
        ptr: *const u8,
        len: usize,
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

/// Conveniece alias used by buffering code. See [`WriteChunk`].
pub type WriteBatch = Vec<WriteChunk>;

/// Errors surfaced by write buffer operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WriteBufferError {
    #[error("write buffer already staged for I/O")]
    AlreadyStaged,
    #[error("active buffer is empty")]
    Empty,
    #[error("standby buffer expected to be empty")]
    StandbyDirty,
}

const STAGED_NONE: usize = usize::MAX;

#[derive(Debug)]
struct WalSegmentBuffers {
    slots: [Mutex<WriteBatch>; 2],
    active: AtomicUsize,
    staged: AtomicUsize,
}

impl WalSegmentBuffers {
    fn new() -> Self {
        Self {
            slots: [Mutex::new(Vec::new()), Mutex::new(Vec::new())],
            active: AtomicUsize::new(0),
            staged: AtomicUsize::new(STAGED_NONE),
        }
    }

    fn with_active<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut WriteBatch) -> R,
    {
        let index = self.active.load(Ordering::Acquire);
        let mut guard = self.slots[index].lock().expect("wal buffer mutex poisoned");
        f(&mut guard)
    }

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

    /// Atomically swap active and standby buffers and take the data for writing.
    /// This is called by the write controller to get data without explicit staging.
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

    fn staged_len(&self) -> usize {
        let index = self.staged.load(Ordering::Acquire);
        if index == STAGED_NONE {
            return 0;
        }
        let guard = self.slots[index].lock().expect("wal buffer mutex poisoned");
        total_chunk_len(&guard)
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagedBatchStats {
    pub bytes: usize,
    pub chunks: usize,
}

#[derive(Debug, Default)]
struct QueueInstrumentation {
    queued: AtomicBool,
    depth: AtomicUsize,
}

impl QueueInstrumentation {
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

    fn clear(&self) {
        self.queued.store(false, Ordering::Release);
        self.depth.store(0, Ordering::Relaxed);
    }

    fn update_depth(&self, depth: usize) {
        self.depth.store(depth, Ordering::Relaxed);
    }

    fn is_queued(&self) -> bool {
        self.queued.load(Ordering::Acquire)
    }

    fn depth(&self) -> usize {
        self.depth.load(Ordering::Relaxed)
    }
}

/// AOF WAL segment state machine tracking logical offsets and buffering for writes/flushes.
pub struct AofWalSegment {
    io: Arc<dyn IoFile>,
    base_offset: u64,
    preallocated_size: AtomicU64,
    pending_size: AtomicU64,
    written_size: AtomicU64,
    durable_size: AtomicU64,
    pending_flush_target: AtomicU64,

    buffers: WalSegmentBuffers,

    write_queue: QueueInstrumentation,
    flush_queue: QueueInstrumentation,

    last_write_batch_bytes: AtomicUsize,
    last_flush_duration_ns: AtomicU64,

    // Write rate tracking for adaptive chunk sizing
    created_at_micros: u64,
    total_bytes_written: AtomicU64,
    write_rate_bytes_per_sec: AtomicU64,
}

impl AofWalSegment {
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

    pub fn preallocated_size(&self) -> u64 {
        self.preallocated_size.load(Ordering::Relaxed)
    }

    pub fn base_offset(&self) -> u64 {
        self.base_offset
    }

    pub fn write_offset(&self) -> u64 {
        self.base_offset
            .saturating_add(self.written_size.load(Ordering::Acquire))
    }

    pub fn pending_size(&self) -> u64 {
        self.pending_size.load(Ordering::Relaxed)
    }

    pub fn written_size(&self) -> u64 {
        self.written_size.load(Ordering::Relaxed)
    }

    pub fn durable_size(&self) -> u64 {
        self.durable_size.load(Ordering::Relaxed)
    }

    pub fn staged_bytes(&self) -> usize {
        self.buffers.staged_len()
    }

    #[instrument(skip(self))]
    pub fn extend_preallocated(&self, bytes: u64) {
        let new_size = self.preallocated_size.fetch_add(bytes, Ordering::AcqRel) + bytes;
        debug!(bytes, new_size, "extended preallocated size");
    }

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

    pub fn with_active_batch<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut WriteBatch) -> R,
    {
        self.buffers.with_active(f)
    }

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

    #[instrument(skip(self))]
    pub fn take_staged_batch(&self) -> Option<WriteBatch> {
        let batch = self.buffers.take_staged();
        if let Some(ref b) = batch {
            let bytes = total_chunk_len(b);
            trace!(bytes, chunks = b.len(), "took staged batch");
        }
        batch
    }

    /// Atomically take the active batch for writing (no staging required).
    #[instrument(skip(self))]
    pub fn take_active_batch(&self) -> Option<WriteBatch> {
        let batch = self.buffers.take_active_batch();
        if let Some(ref b) = batch {
            let bytes = total_chunk_len(b);
            trace!(bytes, chunks = b.len(), "took active batch");
        }
        batch
    }

    #[instrument(skip(self, batch))]
    pub fn restore_active_batch(&self, batch: WriteBatch) {
        let bytes = total_chunk_len(&batch);
        trace!(bytes, chunks = batch.len(), "restoring active batch");
        self.buffers.restore_active(batch)
    }

    pub fn io(&self) -> Arc<dyn IoFile> {
        self.io.clone()
    }

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

    #[instrument(skip(self))]
    pub fn clear_write_queue(&self) {
        trace!("clearing write queue");
        self.write_queue.clear();
    }

    #[instrument(skip(self))]
    pub fn update_write_queue_depth(&self, depth: usize) {
        trace!(depth, "updated write queue depth");
        self.write_queue.update_depth(depth);
    }

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

    #[instrument(skip(self))]
    pub fn clear_flush_queue(&self) {
        trace!("clearing flush queue");
        self.flush_queue.clear();
    }

    #[instrument(skip(self))]
    pub fn update_flush_queue_depth(&self, depth: usize) {
        trace!(depth, "updated flush queue depth");
        self.flush_queue.update_depth(depth);
    }

    pub fn is_write_enqueued(&self) -> bool {
        self.write_queue.is_queued()
    }

    pub fn is_flush_enqueued(&self) -> bool {
        self.flush_queue.is_queued()
    }

    pub fn write_queue_depth(&self) -> usize {
        self.write_queue.depth()
    }

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

    #[instrument(skip(self))]
    pub fn restore_flush_target(&self, target: u64) {
        if target == 0 {
            return;
        }
        trace!(target, "restoring flush target");
        self.pending_flush_target
            .fetch_max(target, Ordering::AcqRel);
    }

    pub fn pending_flush_target(&self) -> u64 {
        self.pending_flush_target.load(Ordering::Acquire)
    }

    pub fn set_last_write_batch_bytes(&self, bytes: usize) {
        self.last_write_batch_bytes.store(bytes, Ordering::Relaxed);
    }

    pub fn last_write_batch_bytes(&self) -> usize {
        self.last_write_batch_bytes.load(Ordering::Relaxed)
    }

    pub fn set_last_flush_duration(&self, duration: Duration) {
        let nanos = duration.as_nanos().min(u64::MAX as u128) as u64;
        self.last_flush_duration_ns.store(nanos, Ordering::Relaxed);
    }

    pub fn last_flush_duration(&self) -> Duration {
        Duration::from_nanos(self.last_flush_duration_ns.load(Ordering::Relaxed))
    }

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

#[derive(Debug, Clone)]
pub struct AofWalSegmentSnapshot {
    pub preallocated_size: u64,
    pub pending_size: u64,
    pub written_size: u64,
    pub durable_size: u64,
    pub write_queue_depth: usize,
    pub flush_queue_depth: usize,
    pub pending_flush_target: u64,
    pub last_write_batch_bytes: usize,
    pub last_flush_duration: Duration,
    pub staged_bytes: usize,
    pub created_at_micros: u64,
    pub write_rate_bytes_per_sec: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AofWalSegmentError {
    #[error("integer overflow while updating segment state")]
    IntegerOverflow,
    #[error("segment lacks capacity for reservation: requested={bytes} available={available}")]
    InsufficientCapacity { bytes: u64, available: u64 },
    #[error("pending underflow: current={current} attempted={bytes}")]
    PendingUnderflow { current: u64, bytes: u64 },
    #[error("written bytes exceed preallocated length: next={next}")]
    WrittenExceedsPreallocated { next: u64 },
    #[error("durable advance exceeds written length: written={written} durable={new_durable}")]
    DurableBeyondWritten { written: u64, new_durable: u64 },
    #[error("durable size regression: current={current} new={new_durable}")]
    DurableRegression { current: u64, new_durable: u64 },
}

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
