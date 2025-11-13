//! Slab allocator, pooled writer leases, and tail-window coordination used by maniac-storage.
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::collections::VecDeque;
use std::ptr::NonNull;
use std::slice;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::config::{PAGE_SIZE, StorageConfig};
use crate::direct_io;
#[cfg(test)]
use std::sync::atomic::AtomicBool;

#[cfg(test)]
static FORCE_ALLOC_FAILURE: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
fn should_fail_alloc() -> bool {
    FORCE_ALLOC_FAILURE.load(Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn set_force_alloc_failure(flag: bool) {
    FORCE_ALLOC_FAILURE.store(flag, Ordering::Relaxed);
}

/// Errors produced when allocating slabs from the system allocator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlabAllocationError {
    Layout,
    Oom { size: usize },
}

/// Errors produced when sealing a writer lease into a published slab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlabPublishError {
    PayloadTooLarge { requested: usize, capacity: usize },
    PayloadNotPageAligned { requested: usize, page: usize },
    PayloadNotExtentAligned { requested: usize, extent: usize },
    PageCountMismatch { provided: u32, expected: u32 },
}

/// Heap-backed slab aligned for direct I/O.
#[derive(Debug)]
/// Heap allocation that satisfies direct-I/O alignment requirements for a single slab.
pub struct AlignedSlab {
    ptr: NonNull<u8>,
    len: usize,
    layout: Layout,
}

impl AlignedSlab {
    pub fn allocate(config: &StorageConfig) -> Result<Self, SlabAllocationError> {
        let size = config.slab_size().get() as usize;
        #[cfg(test)]
        if should_fail_alloc() {
            return Err(SlabAllocationError::Oom { size });
        }
        let alignment = direct_io::buffer_alignment().max(PAGE_SIZE as usize);
        let layout =
            Layout::from_size_align(size, alignment).map_err(|_| SlabAllocationError::Layout)?;
        let ptr = unsafe { alloc_zeroed(layout) };
        let ptr = NonNull::new(ptr).ok_or(SlabAllocationError::Oom { size })?;
        debug_assert!(
            ptr.as_ptr() as usize % alignment == 0,
            "allocated slab pointer must satisfy direct I/O alignment"
        );
        Ok(Self {
            ptr,
            len: size,
            layout,
        })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for AlignedSlab {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}

/// Flags stored alongside slab headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SlabFlags(u32);

impl SlabFlags {
    pub const NONE: Self = Self(0);
    pub const CHECKPOINT: Self = Self(1 << 0);
    pub const COMPRESSED: Self = Self(1 << 1);

    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }
}

/// Metadata describing a published slab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlabHeader {
    pub sequence: u64,
    pub base_page: u64,
    pub page_count: u32,
    pub payload_len: u32,
    pub checksum: u32,
    pub flags: SlabFlags,
}

/// Parameters supplied when sealing a writer lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlabPublishParams {
    pub base_page: u64,
    pub page_count: u32,
    pub payload_len: usize,
    pub checksum: u32,
    pub flags: SlabFlags,
}

impl SlabPublishParams {
    pub fn new(base_page: u64, page_count: u32, payload_len: usize) -> Self {
        Self {
            base_page,
            page_count,
            payload_len,
            checksum: 0,
            flags: SlabFlags::NONE,
        }
    }

    pub fn with_checksum(mut self, checksum: u32) -> Self {
        self.checksum = checksum;
        self
    }

    pub fn with_flags(mut self, flags: SlabFlags) -> Self {
        self.flags = flags;
        self
    }
}

/// Captures the live tail window state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Snapshot of how far the pool has published slabs and how many can be reclaimed.
pub struct TailMetrics {
    pub published: u64,
    pub reclaim: u64,
}

/// Slab pool providing writer leases and shared reader views.
#[derive(Debug, Clone)]
/// Shared pool handing out writer leases and tracking tail readers.
pub struct SlabPool {
    inner: Arc<PoolInner>,
}

impl SlabPool {
    pub fn new(config: StorageConfig) -> Result<Self, SlabAllocationError> {
        let policy = ReadaheadPolicy::from_config(&config);
        Ok(Self {
            inner: Arc::new(PoolInner::new(config, policy)?),
        })
    }

    pub fn config(&self) -> StorageConfig {
        self.inner.config
    }

    pub fn acquire_writer(&self) -> Result<WriterLease, SlabAllocationError> {
        PoolInner::acquire_writer(&self.inner)
    }

    pub fn register_reader(&self) -> ReaderCursor {
        TailWindow::register_reader(&self.inner.tail)
    }

    pub fn tail_metrics(&self) -> TailMetrics {
        self.inner.tail.metrics()
    }
}

/// Exclusive writer view over a pooled slab.
#[derive(Debug)]
/// Exclusive handle that lets a writer fill a slab before publishing or recycling it.
pub struct WriterLease {
    slot: Option<Arc<PoolSlot>>,
    pool: Arc<PoolInner>,
}

impl WriterLease {
    pub fn capacity(&self) -> usize {
        self.slot
            .as_ref()
            .expect("slot should be present")
            .capacity()
    }

    pub fn buffer_mut(&mut self) -> &mut [u8] {
        let arc = self.slot.as_mut().expect("slot already sealed");
        Arc::get_mut(arc)
            .expect("writer lease must have exclusive access")
            .payload_mut()
    }

    pub fn seal(mut self, params: SlabPublishParams) -> Result<PublishedSlab, SlabPublishError> {
        let slot = self
            .slot
            .take()
            .expect("writer lease can only be sealed once");
        let extent = self.pool.config.extent_size().get() as usize;
        let page_size = PAGE_SIZE as usize;
        if params.payload_len > slot.capacity() {
            return Err(SlabPublishError::PayloadTooLarge {
                requested: params.payload_len,
                capacity: slot.capacity(),
            });
        }
        if params.payload_len % page_size != 0 {
            return Err(SlabPublishError::PayloadNotPageAligned {
                requested: params.payload_len,
                page: page_size,
            });
        }
        let expected_pages = (params.payload_len / page_size) as u32;
        if params.page_count != expected_pages {
            return Err(SlabPublishError::PageCountMismatch {
                provided: params.page_count,
                expected: expected_pages,
            });
        }
        if params.payload_len % extent != 0 {
            return Err(SlabPublishError::PayloadNotExtentAligned {
                requested: params.payload_len,
                extent,
            });
        }
        Ok(PoolInner::publish(&self.pool, slot, params))
    }
}

impl Drop for WriterLease {
    fn drop(&mut self) {
        if let Some(slot) = self.slot.take() {
            self.pool.recycle(slot);
        }
    }
}

/// Shared, reference counted view of a published slab.
#[derive(Debug, Clone)]
/// Reference-counted view of a sealed slab visible to readers and WAL replay.
pub struct PublishedSlab {
    handle: SlabHandle,
    header: SlabHeader,
}

impl PublishedSlab {
    fn from_handle(handle: SlabHandle) -> Self {
        let header = handle.header();
        Self { handle, header }
    }

    pub fn header(&self) -> &SlabHeader {
        &self.header
    }

    pub fn payload(&self) -> &[u8] {
        self.handle.payload()
    }
}

#[derive(Debug)]
struct SlabHandle {
    slot: Arc<PoolSlot>,
    pool: Weak<PoolInner>,
}

impl SlabHandle {
    fn new(slot: Arc<PoolSlot>, pool: &Arc<PoolInner>) -> Self {
        slot.retain();
        Self {
            slot,
            pool: Arc::downgrade(pool),
        }
    }

    fn header(&self) -> SlabHeader {
        self.slot.header()
    }

    fn payload(&self) -> &[u8] {
        self.slot.payload()
    }
}

impl Clone for SlabHandle {
    fn clone(&self) -> Self {
        self.slot.retain();
        Self {
            slot: Arc::clone(&self.slot),
            pool: Weak::clone(&self.pool),
        }
    }
}

impl Drop for SlabHandle {
    fn drop(&mut self) {
        if self.slot.release() == 0 {
            if let Some(pool) = self.pool.upgrade() {
                pool.recycle(Arc::clone(&self.slot));
            }
        }
    }
}

#[derive(Debug)]
struct PoolSlot {
    slab: AlignedSlab,
    refcount: AtomicUsize,
    sequence: AtomicU64,
    base_page: AtomicU64,
    page_count: AtomicU32,
    payload_len: AtomicUsize,
    checksum: AtomicU32,
    flags: AtomicU32,
}

impl PoolSlot {
    fn new(slab: AlignedSlab) -> Self {
        Self {
            slab,
            refcount: AtomicUsize::new(0),
            sequence: AtomicU64::new(0),
            base_page: AtomicU64::new(0),
            page_count: AtomicU32::new(0),
            payload_len: AtomicUsize::new(0),
            checksum: AtomicU32::new(0),
            flags: AtomicU32::new(0),
        }
    }

    fn capacity(&self) -> usize {
        self.slab.len()
    }

    fn payload(&self) -> &[u8] {
        self.slab.as_slice()
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        self.slab.as_mut_slice()
    }

    fn retain(&self) {
        self.refcount.fetch_add(1, Ordering::AcqRel);
    }

    fn release(&self) -> usize {
        self.refcount.fetch_sub(1, Ordering::AcqRel) - 1
    }

    fn reset(&self) {
        self.refcount.store(0, Ordering::Release);
        self.sequence.store(0, Ordering::Release);
        self.base_page.store(0, Ordering::Release);
        self.page_count.store(0, Ordering::Release);
        self.payload_len.store(0, Ordering::Release);
        self.checksum.store(0, Ordering::Release);
        self.flags.store(0, Ordering::Release);
    }

    fn set_metadata(&self, sequence: u64, params: &SlabPublishParams) {
        self.sequence.store(sequence, Ordering::Release);
        self.base_page.store(params.base_page, Ordering::Release);
        self.page_count.store(params.page_count, Ordering::Release);
        self.payload_len
            .store(params.payload_len, Ordering::Release);
        self.checksum.store(params.checksum, Ordering::Release);
        self.flags.store(params.flags.bits(), Ordering::Release);
    }

    fn header(&self) -> SlabHeader {
        SlabHeader {
            sequence: self.sequence.load(Ordering::Acquire),
            base_page: self.base_page.load(Ordering::Acquire),
            page_count: self.page_count.load(Ordering::Acquire),
            payload_len: self.payload_len.load(Ordering::Acquire) as u32,
            checksum: self.checksum.load(Ordering::Acquire),
            flags: SlabFlags::from_bits(self.flags.load(Ordering::Acquire)),
        }
    }
}

#[derive(Debug)]
struct PoolInner {
    config: StorageConfig,
    free: Mutex<VecDeque<Arc<PoolSlot>>>,
    next_sequence: AtomicU64,
    tail: Arc<TailWindow>,
}

impl PoolInner {
    fn new(config: StorageConfig, policy: ReadaheadPolicy) -> Result<Self, SlabAllocationError> {
        Ok(Self {
            config,
            free: Mutex::new(VecDeque::new()),
            next_sequence: AtomicU64::new(0),
            tail: TailWindow::new(policy),
        })
    }

    fn acquire_writer(pool: &Arc<Self>) -> Result<WriterLease, SlabAllocationError> {
        let slot = {
            let mut free = pool.free.lock().expect("poisoned free list");
            free.pop_front()
        };
        let slot = match slot {
            Some(slot) => slot,
            None => Arc::new(PoolSlot::new(AlignedSlab::allocate(&pool.config)?)),
        };
        slot.reset();
        Ok(WriterLease {
            slot: Some(slot),
            pool: Arc::clone(pool),
        })
    }

    fn recycle(&self, slot: Arc<PoolSlot>) {
        slot.reset();
        self.free
            .lock()
            .expect("poisoned free list")
            .push_back(slot);
    }

    fn publish(pool: &Arc<Self>, slot: Arc<PoolSlot>, params: SlabPublishParams) -> PublishedSlab {
        debug_assert_eq!(
            slot.refcount.load(Ordering::Acquire),
            0,
            "published slot should not have outstanding references"
        );
        let sequence = pool.next_sequence.fetch_add(1, Ordering::AcqRel);
        slot.set_metadata(sequence, &params);
        let handle = SlabHandle::new(slot, pool);
        let published = PublishedSlab::from_handle(handle);
        pool.tail.publish(published.clone());
        published
    }
}

/// Reader cursor registered against the pool tail window.
#[derive(Debug, Clone)]
/// Per-reader state used to issue readahead and advance reclaim watermarks.
pub struct ReaderCursor {
    inner: Arc<ReaderCursorInner>,
    window: Arc<TailWindow>,
}

impl ReaderCursor {
    pub fn next_sequence(&self) -> u64 {
        self.inner.position.load(Ordering::Acquire)
    }

    pub fn readahead(&self) -> Vec<PublishedSlab> {
        let limit = self.window.policy.slabs_ahead();
        self.window.snapshot_from(self.next_sequence(), limit)
    }

    pub fn readahead_with_limit(&self, limit: u16) -> Vec<PublishedSlab> {
        self.window.snapshot_from(self.next_sequence(), limit)
    }

    pub fn advance_to(&self, sequence: u64) {
        self.inner.position.store(sequence, Ordering::Release);
        self.window.update_reclaim();
    }
}

#[derive(Debug)]
struct ReaderCursorInner {
    position: AtomicU64,
}

#[derive(Debug, Clone)]
/// Configuration describing how many slabs each reader should pull ahead.
pub struct ReadaheadPolicy {
    slabs_ahead: u16,
}

impl ReadaheadPolicy {
    pub fn from_config(config: &StorageConfig) -> Self {
        let slabs_ahead = match config.flavor {
            crate::config::StorageFlavor::Aof => config.readahead_slabs,
            crate::config::StorageFlavor::Standard => config.readahead_slabs.saturating_mul(2),
        };
        Self {
            slabs_ahead: slabs_ahead.max(1),
        }
    }

    pub fn slabs_ahead(&self) -> u16 {
        self.slabs_ahead
    }
}

#[derive(Debug)]
/// Tracks published slabs and reclaim progress for all registered readers.
struct TailWindow {
    policy: ReadaheadPolicy,
    published: AtomicU64,
    reclaim: AtomicU64,
    active: Mutex<VecDeque<PublishedSlab>>,
    readers: Mutex<Vec<Weak<ReaderCursorInner>>>,
}

impl TailWindow {
    fn new(policy: ReadaheadPolicy) -> Arc<Self> {
        Arc::new(Self {
            policy,
            published: AtomicU64::new(0),
            reclaim: AtomicU64::new(0),
            active: Mutex::new(VecDeque::new()),
            readers: Mutex::new(Vec::new()),
        })
    }

    fn publish(&self, slab: PublishedSlab) {
        let sequence = slab.header().sequence;
        self.published.store(sequence + 1, Ordering::Release);
        self.active
            .lock()
            .expect("poisoned active queue")
            .push_back(slab);
    }

    fn register_reader(pool: &Arc<Self>) -> ReaderCursor {
        let start = pool.reclaim.load(Ordering::Acquire);
        let inner = Arc::new(ReaderCursorInner {
            position: AtomicU64::new(start),
        });
        pool.readers
            .lock()
            .expect("poisoned readers list")
            .push(Arc::downgrade(&inner));
        ReaderCursor {
            inner,
            window: Arc::clone(pool),
        }
    }

    fn snapshot_from(&self, start_sequence: u64, limit: u16) -> Vec<PublishedSlab> {
        let mut out = Vec::new();
        if limit == 0 {
            return out;
        }
        let guard = self.active.lock().expect("poisoned active queue");
        for slab in guard.iter() {
            if slab.header().sequence >= start_sequence {
                out.push(slab.clone());
                if out.len() == limit as usize {
                    break;
                }
            }
        }
        out
    }

    fn update_reclaim(&self) {
        let min_pos = self.min_reader_position();
        self.reclaim.store(min_pos, Ordering::Release);
        let mut guard = self.active.lock().expect("poisoned active queue");
        while let Some(front) = guard.front() {
            if front.header().sequence < min_pos {
                guard.pop_front();
            } else {
                break;
            }
        }
    }

    fn min_reader_position(&self) -> u64 {
        let mut min = self.published.load(Ordering::Acquire);
        let mut readers = self.readers.lock().expect("poisoned readers list");
        readers.retain(|weak| weak.strong_count() > 0);
        for weak in readers.iter() {
            if let Some(reader) = weak.upgrade() {
                let pos = reader.position.load(Ordering::Acquire);
                min = min.min(pos);
            }
        }
        min
    }

    fn metrics(&self) -> TailMetrics {
        TailMetrics {
            published: self.published.load(Ordering::Acquire),
            reclaim: self.reclaim.load(Ordering::Acquire),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        MAX_EXTENT_SIZE, MAX_SLAB_SIZE, MIN_EXTENT_SIZE, MIN_SLAB_SIZE, StorageFlavor,
    };

    #[test]
    fn aligned_slab_satisfies_direct_io_alignment() {
        let config =
            StorageConfig::new(StorageFlavor::Aof, MIN_EXTENT_SIZE, MIN_SLAB_SIZE, 2).unwrap();
        let pool = SlabPool::new(config).expect("pool");
        let mut lease = pool.acquire_writer().expect("lease");
        let ptr = lease.buffer_mut().as_ptr() as usize;
        let alignment = crate::direct_io::buffer_alignment();
        assert_eq!(
            ptr % alignment,
            0,
            "writer buffer must match direct I/O alignment"
        );
    }

    #[test]
    fn writer_round_trip_reuses_slots() {
        let pool = SlabPool::new(
            StorageConfig::new(StorageFlavor::Aof, MIN_EXTENT_SIZE, MIN_SLAB_SIZE, 2).unwrap(),
        )
        .unwrap();
        let mut lease = pool.acquire_writer().expect("lease");
        let buf = lease.buffer_mut();
        buf[..MIN_EXTENT_SIZE as usize].fill(0xAB);
        let page_count = (MIN_EXTENT_SIZE / PAGE_SIZE) as u32;
        let params = SlabPublishParams::new(0, page_count, MIN_EXTENT_SIZE as usize);
        let published = lease.seal(params).expect("seal");
        assert_eq!(published.header().sequence, 0);
        assert_eq!(published.header().payload_len, MIN_EXTENT_SIZE as u32);
        let reader = pool.register_reader();
        let readahead = reader.readahead();
        assert_eq!(readahead.len(), 1);
        assert_eq!(readahead[0].header().sequence, 0);
        reader.advance_to(1);
        assert_eq!(pool.tail_metrics().published, 1);
    }

    #[test]
    fn max_config_allocation_capacity_matches() {
        let config =
            StorageConfig::new(StorageFlavor::Standard, MAX_EXTENT_SIZE, MAX_SLAB_SIZE, 4).unwrap();
        let pool = SlabPool::new(config).expect("pool");
        let lease = pool.acquire_writer().expect("lease");
        assert_eq!(lease.capacity(), MAX_SLAB_SIZE as usize);
    }

    #[test]
    fn allocation_failure_is_propagated() {
        let config =
            StorageConfig::new(StorageFlavor::Aof, MIN_EXTENT_SIZE, MIN_SLAB_SIZE, 2).unwrap();
        let pool = SlabPool::new(config).expect("pool");
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                super::set_force_alloc_failure(false);
            }
        }
        let _reset = Reset;
        super::set_force_alloc_failure(true);
        let err = pool.acquire_writer().expect_err("allocation should fail");
        assert!(matches!(err, SlabAllocationError::Oom { .. }));
    }

    #[test]
    fn readahead_respects_policy_limit() {
        let config =
            StorageConfig::new(StorageFlavor::Aof, MIN_EXTENT_SIZE, MIN_SLAB_SIZE, 2).unwrap();
        let pool = SlabPool::new(config).expect("pool");
        let mut published = Vec::new();
        let page_count = (MIN_EXTENT_SIZE / PAGE_SIZE) as u32;
        for seq in 0..3u64 {
            let mut lease = pool.acquire_writer().expect("lease");
            lease.buffer_mut()[..MIN_EXTENT_SIZE as usize].fill(seq as u8);
            let base_page = seq * page_count as u64;
            let slab = lease
                .seal(SlabPublishParams::new(
                    base_page,
                    page_count,
                    MIN_EXTENT_SIZE as usize,
                ))
                .expect("seal");
            published.push(slab);
        }
        let reader = pool.register_reader();
        let slabs = reader.readahead();
        assert_eq!(slabs.len(), pool.config().readahead_slabs as usize);
        assert_eq!(slabs[0].header().sequence, 0);
        assert_eq!(slabs.last().unwrap().header().sequence, 1);
        drop(slabs);
        reader.advance_to(3);
        drop(published);
        assert!(pool.tail_metrics().reclaim >= 1);
    }
}
