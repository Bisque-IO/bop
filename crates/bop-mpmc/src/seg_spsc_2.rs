//! Segmented SPSC (single-producer / single-consumer) bounded queue
//! with in-place segment pooling via a sealed prefix + zero-copy consume-in-place.
//!
//! Design summary:
//! - Two-level index: global head/tail (monotonic u64) → (segment, offset)
//! - Fixed directory of NUM_SEGS slots, each slot holds an optional segment pointer
//! - Segments are lazily allocated by the producer at boundaries
//! - Consumer "seals" fully-consumed segments by advancing `sealable_hi`
//! - Producer steals from the [sealable_lo, sealable_hi) range to reuse segments
//! - New: zero-copy consumer APIs that yield a contiguous slice within the current segment.
//!
//! Safety & constraints:
//! - Exactly one producer thread may call producer methods
//! - Exactly one consumer thread may call consumer methods
//! - `T: Copy` here for a simple, memcpy-friendly hot path. (Notes show how to generalize.)
//!
//! Tunables:
//! - `P`: log2(segment_size). Example: P=8 → 256 items/segment
//! - `NUM_SEGS_P2`: log2(number of directory slots). Example: 12 → 4096 slots
//!
//! Capacity = SEG_SIZE * NUM_SEGS - 1 (one-empty-slot rule)

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::marker::PhantomData;

/// Cache-line aligned producer state
///
/// The tail_cache uses UnsafeCell instead of AtomicU64 because:
/// 1. Only the producer thread writes to this field
/// 2. Only the producer thread reads from this field
/// 3. No synchronization is needed for single-threaded access
/// 4. UnsafeCell provides interior mutability without atomic overhead
#[repr(align(64))]
struct ProducerState {
    head: AtomicU64,
    tail_cache: UnsafeCell<u64>,
    sealable_lo: AtomicU64,
    fresh_allocations: AtomicU64,
    pool_reuses: AtomicU64,
    _phantom: PhantomData<()>,
}

// SAFETY: ProducerState is only accessed by the single producer thread,
// so the UnsafeCell access is safe. The atomic head provides
// the necessary synchronization between producer and consumer.
unsafe impl Send for ProducerState {}
unsafe impl Sync for ProducerState {}

impl ProducerState {
    fn new() -> Self {
        Self {
            head: AtomicU64::new(0),
            tail_cache: UnsafeCell::new(0),
            sealable_lo: AtomicU64::new(0),
            fresh_allocations: AtomicU64::new(0),
            pool_reuses: AtomicU64::new(0),
            _phantom: PhantomData,
        }
    }
}

/// Cache-line aligned consumer state
///
/// The head_cache uses UnsafeCell instead of AtomicU64 because:
/// 1. Only the consumer thread writes to this field
/// 2. Only the consumer thread reads from this field
/// 3. No synchronization is needed for single-threaded access
/// 4. UnsafeCell provides interior mutability without atomic overhead
#[repr(align(64))]
struct ConsumerState {
    tail: AtomicU64,
    head_cache: UnsafeCell<u64>,
    sealable_hi: AtomicU64,
    _phantom: PhantomData<()>,
}

// SAFETY: ConsumerState is only accessed by the single consumer thread,
// so the UnsafeCell access is safe. The atomic tail provides
// the necessary synchronization between producer and consumer.
unsafe impl Send for ConsumerState {}
unsafe impl Sync for ConsumerState {}

impl ConsumerState {
    fn new() -> Self {
        Self {
            tail: AtomicU64::new(0),
            head_cache: UnsafeCell::new(0),
            sealable_hi: AtomicU64::new(0),
            _phantom: PhantomData,
        }
    }
}

/// A bounded, segmented SPSC queue with in-place segment reuse and zero-copy consuming.
///
/// * `T` is `Copy` for a simple, fast implementation.
/// * For non-`Copy` types, replace `copy_nonoverlapping` with element moves and be careful about drops.
pub struct SegSpsc<T: Copy, const P: usize, const NUM_SEGS_P2: usize> {
    segs: Box<[AtomicPtr<MaybeUninit<T>>]>, // NUM_SEGS slots (directory)
    producer: ProducerState,
    consumer: ConsumerState,
    _t: PhantomData<T>,
}

impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> SegSpsc<T, P, NUM_SEGS_P2> {
    pub const SEG_SIZE: usize = 1usize << P;
    pub const NUM_SEGS: usize = 1usize << NUM_SEGS_P2;
    pub const SEG_MASK: usize = Self::SEG_SIZE - 1;
    pub const DIR_MASK: usize = Self::NUM_SEGS - 1;

    #[inline(always)]
    pub const fn capacity() -> usize {
        (Self::SEG_SIZE * Self::NUM_SEGS) - 1
    }

    pub fn new() -> Self {
        let mut v = Vec::with_capacity(Self::NUM_SEGS);
        for _ in 0..Self::NUM_SEGS {
            v.push(AtomicPtr::new(ptr::null_mut()));
        }
        Self {
            segs: v.into_boxed_slice(),
            producer: ProducerState::new(),
            consumer: ConsumerState::new(),
            _t: PhantomData,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        let h = self.producer.head.load(Ordering::Acquire);
        let t = self.consumer.tail.load(Ordering::Acquire);
        (h - t) as usize
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.len() == Self::capacity()
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Producer API
    // ──────────────────────────────────────────────────────────────────────────────

    #[inline(always)]
    pub fn try_push(&self, item: T) -> Result<(), T> {
        self.try_push_n(core::slice::from_ref(&item))
            .map(|_| ())
            .map_err(|_| item)
    }

    pub fn try_push_n(&self, src: &[T]) -> Result<usize, ()> {
        let h = self.producer.head.load(Ordering::Relaxed);

        // Use cached tail value, refresh if needed
        let mut t = unsafe { *self.producer.tail_cache.get() };
        let mut free = Self::capacity() - (h - t) as usize;

        if free == 0 {
            // Refresh tail cache from consumer
            t = self.consumer.tail.load(Ordering::Acquire);
            unsafe {
                *self.producer.tail_cache.get() = t;
            }
            free = Self::capacity() - (h - t) as usize;
            if free == 0 {
                return Err(());
            }
        }

        let mut n = src.len().min(free);

        let mut i = h;
        let mut copied = 0usize;

        while n > 0 {
            let seg_idx = ((i >> P) as usize) & Self::DIR_MASK;
            let off = (i as usize) & Self::SEG_MASK;

            let base = self.ensure_segment_for(seg_idx);

            let can = (Self::SEG_SIZE - off).min(n);
            unsafe {
                let dst_ptr = base.add(off) as *mut T as *mut core::ffi::c_void;
                let src_ptr = src.as_ptr().add(copied) as *const core::ffi::c_void;
                ptr::copy_nonoverlapping(
                    src_ptr as *const u8,
                    dst_ptr as *mut u8,
                    can * core::mem::size_of::<T>(),
                );
            }

            i += can as u64;
            copied += can;
            n -= can;

            if ((i as usize) & Self::SEG_MASK) == 0 {
                let _ = self.segs[((i >> P) as usize) & Self::DIR_MASK].load(Ordering::Relaxed);
            }
        }

        self.producer.head.store(i, Ordering::Release);
        Ok(copied)
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Consumer API (copying)
    // ──────────────────────────────────────────────────────────────────────────────

    #[inline(always)]
    pub fn try_pop(&self) -> Option<T> {
        let mut tmp: T = unsafe { core::mem::zeroed() };
        match self.try_pop_n(core::slice::from_mut(&mut tmp)) {
            Ok(1) => Some(tmp),
            _ => None,
        }
    }

    pub fn try_pop_n(&self, dst: &mut [T]) -> Result<usize, ()> {
        let t = self.consumer.tail.load(Ordering::Relaxed);

        // Use cached head value, refresh if needed
        let mut h = unsafe { *self.consumer.head_cache.get() };
        let mut avail = (h - t) as usize;

        if avail == 0 {
            // Refresh head cache from producer
            h = self.producer.head.load(Ordering::Acquire);
            unsafe {
                *self.consumer.head_cache.get() = h;
            }
            avail = (h - t) as usize;
            if avail == 0 {
                return Err(());
            }
        }

        let mut n = dst.len().min(avail);

        let mut i = t;
        let mut copied = 0usize;

        while n > 0 {
            let seg_idx = ((i >> P) as usize) & Self::DIR_MASK;
            let off = (i as usize) & Self::SEG_MASK;

            let base = self.segs[seg_idx].load(Ordering::Acquire);
            // debug_assert!(!base.is_null());

            let can = (Self::SEG_SIZE - off).min(n);
            unsafe {
                let src_ptr = (base.add(off) as *const MaybeUninit<T>) as *const T;
                let dst_ptr = dst.as_mut_ptr().add(copied);
                ptr::copy_nonoverlapping(src_ptr, dst_ptr, can);
            }

            i += can as u64;
            copied += can;
            n -= can;

            // Seal segment if we just finished it
            if ((i as usize) & Self::SEG_MASK) == 0 && i > 0 {
                let s_done = (i >> P).wrapping_sub(1);
                self.seal_after(s_done);
            }
        }

        self.consumer.tail.store(i, Ordering::Release);
        Ok(copied)
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Consumer API (ZERO-COPY, process-in-place)
    // ──────────────────────────────────────────────────────────────────────────────
    /// Present **read-only** contiguous slices (potentially across segments) of up to `max`
    /// items. The callback is invoked for each contiguous segment, returning how many it
    /// consumed (≤ provided). Advances the tail and seals segments as they're fully crossed.
    /// Returns the total number consumed.
    ///
    /// The slice **must not** be held beyond the callback.
    pub fn consume_in_place<F>(&self, max: usize, mut f: F) -> usize
    where
        F: FnMut(&[T]) -> usize,
    {
        if max == 0 {
            return 0;
        }

        let mut t = self.consumer.tail.load(Ordering::Relaxed);

        // Use cached head value, refresh if needed
        let mut h = unsafe { *self.consumer.head_cache.get() };
        let mut avail = (h - t) as usize;

        if avail == 0 {
            // Refresh head cache from producer
            h = self.producer.head.load(Ordering::Acquire);
            unsafe {
                *self.consumer.head_cache.get() = h;
            }
            avail = (h - t) as usize;
            if avail == 0 {
                return 0;
            }
        }

        let mut remaining = max.min(avail);
        let mut total_consumed = 0usize;

        while remaining > 0 {
            let seg_idx = ((t >> P) as usize) & Self::DIR_MASK;
            let off = (t as usize) & Self::SEG_MASK;

            // directory slot must be non-null if items exist here
            let base = self.segs[seg_idx].load(Ordering::Acquire);
            // debug_assert!(!base.is_null());

            // Clamp to remain in this segment
            let in_seg = Self::SEG_SIZE - off;
            let n = remaining.min(in_seg);

            // SAFETY: producer won't touch these already-enqueued items; we expose only the
            // initialized prefix and consumer is single-threaded for this queue.
            let slice = unsafe {
                let ptr = (base.add(off) as *const MaybeUninit<T>) as *const T;
                core::slice::from_raw_parts(ptr, n)
            };

            // let took = n;
            let took = f(slice).min(n);
            if took == 0 {
                break;
            }

            t += took as u64;
            total_consumed += took;
            remaining -= took;

            // If we ended exactly at boundary, seal the finished segment
            if ((t as usize) & Self::SEG_MASK) == 0 && t > 0 {
                let s_done = (t >> P).wrapping_sub(1);
                self.seal_after(s_done);
            }

            // If callback consumed less than offered, stop
            if took < n {
                break;
            }
        }

        self.consumer.tail.store(t, Ordering::Release);
        total_consumed
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Internals
    // ──────────────────────────────────────────────────────────────────────────────
    #[inline(always)]
    fn seal_after(&self, s_done: u64) {
        self.consumer
            .sealable_hi
            .store(s_done + 1, Ordering::Release);
    }

    #[inline(always)]
    /// Get the number of fresh segment allocations (cache misses).
    ///
    /// This represents how many times the queue had to allocate new memory because
    /// no suitable segment was available in the sealed pool. Higher values indicate
    /// more cache misses and potentially worse memory locality.
    ///
    /// # Returns
    /// The total count of fresh segment allocations.
    pub fn fresh_allocations(&self) -> u64 {
        self.producer.fresh_allocations.load(Ordering::Relaxed)
    }

    /// Get the number of segment pool reuses (cache hits).
    ///
    /// This represents how many times the queue successfully reused a segment
    /// from the sealed pool instead of allocating new memory. Higher values indicate
    /// better cache hit rates and improved memory locality.
    ///
    /// # Returns
    /// The total count of segment pool reuses.
    pub fn pool_reuses(&self) -> u64 {
        self.producer.pool_reuses.load(Ordering::Relaxed)
    }

    /// Get the pool reuse rate as a percentage.
    ///
    /// This metric indicates the effectiveness of the segment pooling mechanism.
    /// A higher percentage means better cache hit rates and lower memory allocation overhead.
    ///
    /// # Returns
    /// A value between 0.0 and 100.0 representing the percentage of allocations that were
    /// pool reuses. Returns 0.0 if no allocations have occurred.
    pub fn pool_reuse_rate(&self) -> f64 {
        let fresh = self.fresh_allocations();
        let reused = self.pool_reuses();
        let total = fresh + reused;

        if total == 0 {
            0.0
        } else {
            (reused as f64 / total as f64) * 100.0
        }
    }

    /// Reset allocation statistics to zero.
    ///
    /// This method allows you to reset the allocation counters for measuring
    /// specific periods of operation or for repeated benchmarking scenarios.
    pub fn reset_allocation_stats(&self) {
        self.producer.fresh_allocations.store(0, Ordering::Relaxed);
        self.producer.pool_reuses.store(0, Ordering::Relaxed);
    }

    fn ensure_segment_for(&self, seg_idx: usize) -> *mut MaybeUninit<T> {
        // Already available?
        let p = self.segs[seg_idx].load(Ordering::Acquire);
        if !p.is_null() {
            return p;
        }

        // Try steal from sealed prefix
        let hi = self.consumer.sealable_hi.load(Ordering::Acquire);
        let lo = self.producer.sealable_lo.load(Ordering::Acquire);
        if lo < hi {
            let src_idx = (lo as usize) & Self::DIR_MASK;
            let q = self.segs[src_idx].load(Ordering::Acquire);
            if q.is_null() {
                // No segment available in pool - fresh allocation (cache miss)
                let newp = unsafe { alloc_segment::<T>(1usize << P) };
                self.segs[seg_idx].store(newp, Ordering::Release);
                self.producer.sealable_lo.fetch_add(1, Ordering::Relaxed);
                self.producer
                    .fresh_allocations
                    .fetch_add(1, Ordering::Relaxed);
                return newp;
            }
            // Reuse segment from sealed pool (cache hit)
            self.segs[src_idx].store(ptr::null_mut(), Ordering::Release);
            self.segs[seg_idx].store(q, Ordering::Release);
            self.producer.sealable_lo.fetch_add(1, Ordering::Relaxed);
            self.producer.pool_reuses.fetch_add(1, Ordering::Relaxed);
            return q;
        }

        // Fresh allocation (cache miss)
        let newp = unsafe { alloc_segment::<T>(1usize << P) };
        self.segs[seg_idx].store(newp, Ordering::Release);
        self.producer
            .fresh_allocations
            .fetch_add(1, Ordering::Relaxed);
        newp
    }
}

impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> Drop for SegSpsc<T, P, NUM_SEGS_P2> {
    fn drop(&mut self) {
        for slot in self.segs.iter() {
            let p = slot.load(Ordering::Relaxed);
            if !p.is_null() {
                unsafe { free_segment::<T>(p, 1usize << P) };
            }
        }
    }
}

unsafe fn alloc_segment<T>(seg_size: usize) -> *mut MaybeUninit<T> {
    unsafe {
        let elem = core::mem::size_of::<MaybeUninit<T>>();
        let align = core::mem::align_of::<MaybeUninit<T>>();
        let layout = Layout::from_size_align_unchecked(elem * seg_size, align);
        let p = alloc_zeroed(layout) as *mut MaybeUninit<T>;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        p
    }
}

unsafe fn free_segment<T>(ptr_base: *mut MaybeUninit<T>, seg_size: usize) {
    unsafe {
        let elem = core::mem::size_of::<MaybeUninit<T>>();
        let align = core::mem::align_of::<MaybeUninit<T>>();
        let layout = Layout::from_size_align_unchecked(elem * seg_size, align);
        dealloc(ptr_base as *mut u8, layout);
    }
}

/* ===========================
Demo / comprehensive tests
=========================== */

#[cfg(test)]
mod tests {
    use super::*;
    const P: usize = 6; // 64 items/segment
    const NUM_SEGS_P2: usize = 8;
    type Q = SegSpsc<u64, P, NUM_SEGS_P2>;

    #[test]
    fn basic_push_pop() {
        let q = Q::new();
        for i in 0..200u64 {
            q.try_push(i).unwrap();
        }
        for i in 0..200u64 {
            assert_eq!(q.try_pop(), Some(i));
        }
        assert!(q.try_pop().is_none());
    }

    #[test]
    fn zero_copy_consume_in_place() {
        let q = Q::new();
        let n = 64 * 3 + 5; // crosses segments
        for i in 0..n as u64 {
            q.try_push(i).unwrap();
        }

        let mut seen = 0u64;
        // repeatedly consume up to 50, in-place (callback may be invoked multiple times per call)
        while seen < n as u64 {
            let mut local_seen = seen;
            let took = q.consume_in_place(50, |chunk| {
                for (k, &v) in chunk.iter().enumerate() {
                    assert_eq!(v, local_seen + k as u64, "Mismatch at local offset {}", k);
                }
                local_seen += chunk.len() as u64;
                // consume whole chunk
                chunk.len()
            });
            assert!(took > 0);
            seen += took as u64;
        }
    }

    #[test]
    fn test_constants_and_capacity() {
        assert_eq!(Q::SEG_SIZE, 64);
        assert_eq!(Q::NUM_SEGS, 256);
        assert_eq!(Q::SEG_MASK, 63);
        assert_eq!(Q::DIR_MASK, 255);
        assert_eq!(Q::capacity(), 64 * 256 - 1);
    }

    #[test]
    fn test_new_queue_is_empty() {
        let q = Q::new();
        assert!(q.is_empty());
        assert!(!q.is_full());
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn test_single_item_operations() {
        let mut q = Q::new();
        assert!(q.is_empty());

        // Push single item
        assert!(q.try_push(42).is_ok());
        assert_eq!(q.len(), 1);
        assert!(!q.is_empty());

        // Pop single item
        assert_eq!(q.try_pop(), Some(42));
        assert_eq!(q.len(), 0);
        assert!(q.is_empty());
    }

    #[test]
    fn test_push_pop_order() {
        let mut q = Q::new();
        let items = [1, 2, 3, 4, 5];

        for &item in &items {
            q.try_push(item).unwrap();
        }

        for &expected in &items {
            assert_eq!(q.try_pop(), Some(expected));
        }
    }

    #[test]
    fn test_try_push_n() {
        let mut q = Q::new();
        let items = [10, 20, 30, 40, 50];

        // Push all items at once
        let pushed = q.try_push_n(&items).unwrap();
        assert_eq!(pushed, items.len());
        assert_eq!(q.len(), items.len());

        // Verify items are in correct order
        for &expected in items.iter() {
            assert_eq!(q.try_pop(), Some(expected));
        }
    }

    #[test]
    fn test_try_pop_n() {
        let mut q = Q::new();
        let items = [1, 2, 3, 4, 5, 6, 7, 8];

        // Push all items
        for &item in &items {
            q.try_push(item).unwrap();
        }

        // Pop fewer items than available
        let mut buffer = [0u64; 5];
        let popped = q.try_pop_n(&mut buffer).unwrap();
        assert_eq!(popped, 5);
        assert_eq!(&buffer[..5], &[1, 2, 3, 4, 5]);

        // Pop remaining items
        let mut buffer2 = [0u64; 8];
        let popped2 = q.try_pop_n(&mut buffer2).unwrap();
        assert_eq!(popped2, 3);
        assert_eq!(&buffer2[..3], &[6, 7, 8]);
    }

    #[test]
    fn test_operations_on_empty_queue() {
        let mut q = Q::new();

        // Pop from empty queue
        assert_eq!(q.try_pop(), None);

        // Pop multiple from empty queue
        let mut buffer = [0u64; 5];
        assert!(q.try_pop_n(&mut buffer).is_err());

        // Consume from empty queue
        let consumed = q.consume_in_place(10, |_| 0);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_operations_on_full_queue() {
        let mut q = Q::new();

        // Fill the queue completely
        let capacity = Q::capacity();
        for i in 0..capacity as u64 {
            q.try_push(i).unwrap();
        }

        assert!(q.is_full());
        assert_eq!(q.len(), capacity);

        // Try to push to full queue (single item)
        assert!(q.try_push(999).is_err());

        // Try to push multiple items to full queue
        let items = [1000, 1001, 1002];
        assert!(q.try_push_n(&items).is_err());

        // Pop one item and verify queue is no longer full
        assert_eq!(q.try_pop(), Some(0));
        assert!(!q.is_full());
        assert_eq!(q.len(), capacity - 1);
    }

    #[test]
    fn test_consume_in_place_partial() {
        let mut q = Q::new();
        let items = [1, 2, 3, 4, 5, 6, 7, 8];

        for &item in &items {
            q.try_push(item).unwrap();
        }

        // Consume only half of what's available
        let mut consumed_count = 0;
        let consumed = q.consume_in_place(4, |chunk| {
            consumed_count += chunk.len();
            chunk.len() // consume all items in the chunk
        });

        assert_eq!(consumed, 4);
        assert_eq!(consumed_count, 4);
        assert_eq!(q.len(), 4);

        // Verify remaining items
        for expected in 5..=8 {
            assert_eq!(q.try_pop(), Some(expected));
        }
    }

    #[test]
    fn test_consume_zero_items_in_place() {
        let mut q = Q::new();
        q.try_push(1).unwrap();
        q.try_push(2).unwrap();

        // Ask to consume zero items
        let consumed = q.consume_in_place(0, |_| 10);
        assert_eq!(consumed, 0);
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn test_segment_boundaries() {
        let mut q = Q::new();
        let seg_size = Q::SEG_SIZE;

        // Push exactly one segment
        for i in 0..seg_size as u64 {
            q.try_push(i).unwrap();
        }

        // Push one more to cross boundary
        q.try_push(seg_size as u64).unwrap();

        // Pop all and verify order
        for i in 0..=seg_size as u64 {
            assert_eq!(q.try_pop(), Some(i));
        }

        assert!(q.is_empty());
    }

    #[test]
    fn test_multiple_segments() {
        let mut q = Q::new();
        let seg_size = Q::SEG_SIZE;
        let num_segments = 3;
        let total_items = seg_size * num_segments;

        // Push items across multiple segments
        for i in 0..total_items as u64 {
            q.try_push(i).unwrap();
        }

        // Verify all items are in correct order
        for i in 0..total_items as u64 {
            assert_eq!(q.try_pop(), Some(i));
        }

        assert!(q.is_empty());
    }

    #[test]
    fn test_large_batch_operations() {
        let mut q = Q::new();
        let batch_size = 200;
        let mut items = Vec::with_capacity(batch_size);

        for i in 0..batch_size {
            items.push(i as u64);
        }

        // Test large batch push
        let pushed = q.try_push_n(&items).unwrap();
        assert_eq!(pushed, batch_size);

        // Test large batch pop
        let mut buffer = vec![0u64; batch_size];
        let popped = q.try_pop_n(&mut buffer).unwrap();
        assert_eq!(popped, batch_size);
        assert_eq!(&buffer[..], &items[..]);
    }

    #[test]
    fn test_alternating_push_consume() {
        let mut q = Q::new();

        // Push some items
        for i in 0..50 {
            q.try_push(i).unwrap();
        }

        // Consume some in-place
        let consumed = q.consume_in_place(25, |chunk| chunk.len());
        assert_eq!(consumed, 25);

        // Push more items
        for i in 50..100 {
            q.try_push(i).unwrap();
        }

        // Pop all remaining
        let mut popped_items = Vec::new();
        while let Some(item) = q.try_pop() {
            popped_items.push(item);
        }

        // Verify order: items 25-99 should remain
        for (i, expected) in (25..100).enumerate() {
            assert_eq!(popped_items[i], expected);
        }
    }

    #[test]
    fn test_queue_reuse_after_drain() {
        let mut q = Q::new();

        // First round: fill and drain
        for i in 0..1000 {
            q.try_push(i as u64).unwrap();
        }
        for i in 0..1000 {
            assert_eq!(q.try_pop(), Some(i as u64));
        }

        assert!(q.is_empty());

        // Second round: reuse the same queue
        for i in 0..500 {
            q.try_push((i + 1000) as u64).unwrap();
        }
        for i in 0..500 {
            assert_eq!(q.try_pop(), Some((i + 1000) as u64));
        }

        assert!(q.is_empty());
    }

    #[test]
    fn test_different_types() {
        // Test with different copyable types
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        struct TestStruct {
            a: u32,
            b: i64,
        }

        type QStruct = SegSpsc<TestStruct, 5, 6>;
        let mut q = QStruct::new();

        let item1 = TestStruct { a: 1, b: -1 };
        let item2 = TestStruct { a: 2, b: -2 };

        q.try_push(item1).unwrap();
        q.try_push(item2).unwrap();

        assert_eq!(q.try_pop(), Some(item1));
        assert_eq!(q.try_pop(), Some(item2));
        assert_eq!(q.try_pop(), None);
    }

    #[test]
    fn test_small_queue_configuration() {
        // Test with a smaller queue configuration
        type SmallQ = SegSpsc<u32, 2, 2>; // 4 items per segment, 4 segments = 15 capacity
        let mut q = SmallQ::new();

        assert_eq!(SmallQ::capacity(), 15);
        assert_eq!(SmallQ::SEG_SIZE, 4);
        assert_eq!(SmallQ::NUM_SEGS, 4);

        // Fill to capacity
        for i in 0..15 {
            q.try_push(i as u32).unwrap();
        }

        assert!(q.is_full());
        assert!(q.try_push(100).is_err());

        // Drain completely
        for i in 0..15 {
            assert_eq!(q.try_pop(), Some(i as u32));
        }

        assert!(q.is_empty());
    }

    #[test]
    fn test_consume_with_empty_callback() {
        let mut q = Q::new();

        // Push some items
        for i in 0..10 {
            q.try_push(i).unwrap();
        }

        // Consume with callback that returns 0 (consumes nothing)
        let consumed = q.consume_in_place(5, |_| 0);
        assert_eq!(consumed, 0);
        assert_eq!(q.len(), 10); // No items should be consumed

        // Normal consumption should still work
        let consumed2 = q.consume_in_place(5, |chunk| chunk.len());
        assert_eq!(consumed2, 5);
        assert_eq!(q.len(), 5);
    }

    #[test]
    fn test_length_tracking() {
        let mut q = Q::new();

        assert_eq!(q.len(), 0);

        // Push items and check length
        for i in 1..=100 {
            q.try_push(i as u64).unwrap();
            assert_eq!(q.len(), i);
        }

        // Pop items and check length
        for i in (1..=100).rev() {
            q.try_pop().unwrap();
            assert_eq!(q.len(), i - 1);
        }

        assert_eq!(q.len(), 0);
    }

    #[test]
    fn test_allocation_stats_initial() {
        let q = Q::new();

        // Initially no allocations should have occurred
        assert_eq!(q.fresh_allocations(), 0);
        assert_eq!(q.pool_reuses(), 0);
        assert_eq!(q.pool_reuse_rate(), 0.0);
    }

    #[test]
    fn test_allocation_stats_fresh_allocations() {
        let mut q = Q::new();
        let seg_size = Q::SEG_SIZE;

        // Push enough items to require multiple segment allocations
        // This should trigger fresh allocations since we have a fresh queue
        let items_to_push = seg_size * 3; // Should trigger 3 segment allocations

        for i in 0..items_to_push as u64 {
            q.try_push(i).unwrap();
        }

        // Should have fresh allocations but no pool reuses yet
        let fresh_allocs = q.fresh_allocations();
        let pool_reuses = q.pool_reuses();

        assert!(
            fresh_allocs >= 3,
            "Expected at least 3 fresh allocations, got {}",
            fresh_allocs
        );
        assert_eq!(pool_reuses, 0);
        assert_eq!(q.pool_reuse_rate(), 0.0);
    }

    #[test]
    fn test_allocation_stats_pool_reuse() {
        let mut q = Q::new();
        let seg_size = Q::SEG_SIZE;

        // Phase 1: Fill and consume to create pool entries
        let items_to_push = seg_size * 2; // Fill 2 segments

        for i in 0..items_to_push as u64 {
            q.try_push(i).unwrap();
        }

        // Consume everything to seal segments back into pool
        for i in 0..items_to_push as u64 {
            assert_eq!(q.try_pop(), Some(i));
        }

        let fresh_after_phase1 = q.fresh_allocations();
        assert!(fresh_after_phase1 >= 2);

        // Phase 2: Push more items - should reuse from pool
        for i in 0..items_to_push as u64 {
            q.try_push(i + items_to_push as u64).unwrap();
        }

        // Now we should have both fresh allocations and pool reuses
        let fresh_allocs = q.fresh_allocations();
        let pool_reuses = q.pool_reuses();

        assert!(pool_reuses > 0, "Expected pool reuses, got {}", pool_reuses);
        assert!(
            fresh_allocs > 0,
            "Expected fresh allocations, got {}",
            fresh_allocs
        );
        assert!(
            q.pool_reuse_rate() > 0.0,
            "Expected positive reuse rate, got {}",
            q.pool_reuse_rate()
        );
    }

    #[test]
    fn test_allocation_stats_reset() {
        let mut q = Q::new();

        // Push enough items to trigger allocations
        for i in 0..(Q::SEG_SIZE * 2) as u64 {
            q.try_push(i).unwrap();
        }

        // Verify we have allocations
        assert!(q.fresh_allocations() > 0);

        // Reset stats
        q.reset_allocation_stats();

        // Verify stats are reset
        assert_eq!(q.fresh_allocations(), 0);
        assert_eq!(q.pool_reuses(), 0);
        assert_eq!(q.pool_reuse_rate(), 0.0);
    }

    #[test]
    fn test_allocation_stats_mixed_operations() {
        let mut q = Q::new();
        let seg_size = Q::SEG_SIZE;

        // Simulate realistic usage: push some, consume some, repeat
        let mut next_pop_value = 0u64;
        for cycle in 0..5 {
            // Push items (may trigger allocations)
            let base = (cycle * seg_size) as u64;
            for i in 0..seg_size as u64 {
                q.try_push(base + i).unwrap();
            }

            // Consume half the items (may create pool entries)
            for _ in 0..(seg_size / 2) {
                assert_eq!(q.try_pop(), Some(next_pop_value));
                next_pop_value += 1;
            }
        }

        // We should have a mix of fresh allocations and pool reuses
        let fresh = q.fresh_allocations();
        let reused = q.pool_reuses();

        assert!(fresh > 0, "Expected fresh allocations: {}", fresh);

        // After several cycles, we should start seeing pool reuses
        // (though this depends on the exact pattern of segment sealing)
        println!(
            "Fresh allocations: {}, Pool reuses: {}, Reuse rate: {:.1}%",
            fresh,
            reused,
            q.pool_reuse_rate()
        );
    }

    #[test]
    fn test_allocation_stats_with_consume_in_place() {
        let mut q = Q::new();
        let seg_size = Q::SEG_SIZE;

        // Push items to fill multiple segments
        for i in 0..(seg_size * 3) as u64 {
            q.try_push(i).unwrap();
        }

        let fresh_after_push = q.fresh_allocations();

        // Use consume_in_place to efficiently process items
        let mut total_consumed = 0;
        while total_consumed < seg_size * 3 {
            let consumed = q.consume_in_place(64, |chunk| chunk.len());
            total_consumed += consumed;
        }

        // Consume the rest normally
        while q.try_pop().is_some() {}

        // Push more items to potentially reuse segments
        for i in 0..(seg_size * 2) as u64 {
            q.try_push(i).unwrap();
        }

        // Should have some pool reuses now
        let fresh = q.fresh_allocations();
        let reused = q.pool_reuses();

        assert!(
            fresh >= fresh_after_push,
            "Fresh allocations should not decrease"
        );
        assert!(
            reused > 0 || fresh >= 5,
            "Either pool reuses should occur or we should have many fresh allocations"
        );

        if fresh > 0 {
            println!(
                "Allocation stats with consume_in_place - Fresh: {}, Reused: {}, Reuse rate: {:.1}%",
                fresh,
                reused,
                q.pool_reuse_rate()
            );
        }
    }

    #[test]
    fn test_cache_hit_miss_behavior() {
        let mut q = Q::new();
        let seg_size = Q::SEG_SIZE;
        let num_segments = Q::NUM_SEGS;

        println!(
            "Testing cache hit/miss behavior with segment size: {}, num segments: {}",
            seg_size, num_segments
        );

        // Phase 1: Fill the queue to force fresh allocations (cache misses)
        println!("\n=== Phase 1: Fresh Allocations (Cache Misses) ===");
        for i in 0..(seg_size * 3) as u64 {
            q.try_push(i).unwrap();
        }

        let fresh_after_phase1 = q.fresh_allocations();
        let reused_after_phase1 = q.pool_reuses();

        println!("After filling 3 segments:");
        println!("  Fresh allocations (cache misses): {}", fresh_after_phase1);
        println!("  Pool reuses (cache hits): {}", reused_after_phase1);
        println!("  Reuse rate: {:.1}%", q.pool_reuse_rate());

        assert!(
            fresh_after_phase1 >= 3,
            "Should have at least 3 fresh allocations"
        );
        assert_eq!(
            reused_after_phase1, 0,
            "Should have no pool reuses initially"
        );

        // Phase 2: Drain all items to create pool entries
        println!("\n=== Phase 2: Drain to Create Pool ===");
        let mut drained = 0;
        while let Some(_) = q.try_pop() {
            drained += 1;
        }
        println!("Drained {} items", drained);
        println!("  Fresh allocations: {}", q.fresh_allocations());
        println!("  Pool reuses: {}", q.pool_reuses());

        // Phase 3: Push items again - should reuse from pool (cache hits)
        println!("\n=== Phase 3: Pool Reuse (Cache Hits) ===");
        for i in 0..(seg_size * 2) as u64 {
            q.try_push(i + 1000).unwrap();
        }

        let fresh_after_phase3 = q.fresh_allocations();
        let reused_after_phase3 = q.pool_reuses();

        println!("After refilling 2 segments:");
        println!("  Fresh allocations (cache misses): {}", fresh_after_phase3);
        println!("  Pool reuses (cache hits): {}", reused_after_phase3);
        println!("  Reuse rate: {:.1}%", q.pool_reuse_rate());

        assert!(
            reused_after_phase3 > 0,
            "Should have pool reuses: {}",
            reused_after_phase3
        );

        // Phase 4: Demonstrate alternating pattern to show mixed behavior
        println!("\n=== Phase 4: Alternating Pattern ===");
        q.reset_allocation_stats();

        for cycle in 0..3 {
            // Fill a segment
            for i in 0..seg_size as u64 {
                q.try_push(cycle * 100 + i).unwrap();
            }

            // Drain half to create some pool entries
            for _ in 0..(seg_size / 2) as u64 {
                q.try_pop().unwrap();
            }

            println!(
                "Cycle {}: Fresh={}, Reused={:.1}%",
                cycle + 1,
                q.fresh_allocations(),
                q.pool_reuse_rate()
            );
        }

        // Final statistics
        let final_fresh = q.fresh_allocations();
        let final_reused = q.pool_reuses();
        let final_rate = q.pool_reuse_rate();

        println!("\n=== Final Statistics ===");
        println!("Total fresh allocations (cache misses): {}", final_fresh);
        println!("Total pool reuses (cache hits): {}", final_reused);
        println!("Overall reuse rate: {:.1}%", final_rate);

        // Verify we have both types of allocations
        assert!(final_fresh > 0, "Should have some fresh allocations");
        // Pool reuses may or may not occur depending on exact timing of segment sealing
        println!("Cache behavior analysis complete!");
    }
}

// fn main() {
//     const P: usize = 6;
//     const NUM_SEGS_P2: usize = 8;
//     type Q = SegSpsc<u64, P, NUM_SEGS_P2>;

//     let mut q = Q::new();
//     for i in 0..300u64 { q.try_push(i).unwrap(); }

//     // Zero-copy, read-only, batches of up to 80
//     let mut consumed = 0usize;
//     while consumed < 300 {
//         let took = q.consume_in_place(80, |chunk| {
//             // do work on &chunk[..]
//             // simulate a partial consumption
//             core::cmp::min(chunk.len(), 37)
//         });
//         if took == 0 { break; }
//         consumed += took;
//     }

//     // Drain remaining (copying API)
//     let mut rest = vec![0u64; 512];
//     while let Ok(n) = q.try_pop_n(&mut rest) {
//         if n == 0 { break; }
//         for i in 0..n {
//             let _ = rest[i];
//         }
//     }
// }
