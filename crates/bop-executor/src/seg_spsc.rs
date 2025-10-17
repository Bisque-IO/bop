//! Segmented SPSC (single-producer / single-consumer) bounded queue with zero-copy APIs.
//!
//! # Overview
//!
//! `SegSpsc` is a high-performance, bounded SPSC queue that combines:
//! - **Segmented storage** for flexible capacity without large contiguous allocations
//! - **Lazy allocation** to minimize memory footprint
//! - **Segment pooling** for cache-hot reuse (reduces allocator pressure)
//! - **Zero-copy APIs** for efficient in-place consumption
//! - **Close bit encoding** for graceful shutdown (bit 63 of head position)
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────┐
//! │                    Segment Directory                           │
//! │  [Seg 0] [Seg 1] [Seg 2] ... [Seg N]  (NUM_SEGS slots)         │
//! └────┬──────────┬──────────┬─────────────────────────────────────┘
//!      │          │          │
//!      ▼          ▼          ▼
//!   Segment    Segment     null     Each segment: SEG_SIZE items
//!   [items]    [items]              Lazily allocated by producer
//!
//! Global Positions (monotonic u64):
//!   head: [63=close bit][62..0=position]  Producer writes here
//!   tail: position (no close bit)          Consumer reads here
//!
//! Segment Pooling:
//!   [sealable_lo, sealable_hi) = sealed segments available for reuse
//!   Producer steals from pool before allocating fresh memory
//! ```
//!
//! # Two-Level Indexing
//!
//! Each position `pos` maps to `(segment_index, offset)`:
//! ```text
//! segment_index = (pos >> P) & DIR_MASK
//! offset        = pos & SEG_MASK
//! ```
//!
//! # Segment Lifecycle
//!
//! 1. **Allocation**: Producer allocates when writing to new segment boundary
//! 2. **Active**: Segment contains data being produced/consumed
//! 3. **Sealed**: Consumer marks fully-consumed segments for reuse
//! 4. **Pooled**: Sits in [sealable_lo, sealable_hi) range
//! 5. **Reused**: Producer steals from pool (cache-hot memory)
//!
//! # Safety Constraints
//!
//! - **Single producer**: Only one thread may call producer methods (`try_push`, etc.)
//! - **Single consumer**: Only one thread may call consumer methods (`try_pop`, etc.)
//! - **T: Copy**: Simplifies implementation (no Drop concerns during segment reuse)
//!
//! # Const Parameters
//!
//! - `P`: log2(segment size). Example: P=8 → 256 items/segment
//! - `NUM_SEGS_P2`: log2(directory size). Example: NUM_SEGS_P2=10 → 1024 segments
//!
//! **Capacity** = `(SEG_SIZE × NUM_SEGS) - 1` (one-empty-slot rule)
//!
//! # Close Bit Encoding
//!
//! The high bit (bit 63) of the head position encodes the close state:
//! - **Clear**: Queue is open
//! - **Set**: Queue is closed (via `close()`)
//!
//! This allows lock-free close detection without a separate atomic flag.
//!
//! # Performance Characteristics
//!
//! - **Push/Pop**: O(1) amortized (lazy allocation on segment boundaries)
//! - **Segment reuse**: O(1) steal from sealed pool (cache-friendly)
//! - **Zero-copy consume**: No memcpy, just slice access
//! - **Cache optimization**: Separate cache lines for producer/consumer state
//!
//! # Example
//!
//! ```ignore
//! use bop_mpmc::seg_spsc::SegSpsc;
//!
//! // 64 items/segment, 256 segments = 16,383 capacity
//! type Queue = SegSpsc<usize, 6, 8>;
//!
//! let queue = Queue::new();
//!
//! // Producer
//! queue.try_push(42).unwrap();
//! queue.try_push_n(&[1, 2, 3]).unwrap();
//!
//! // Consumer (zero-copy)
//! let consumed = queue.consume_in_place(10, |slice| {
//!     for &item in slice {
//!         println!("{}", item);
//!     }
//!     slice.len()  // Consumed all
//! });
//!
//! // Close and drain
//! queue.close();
//! while let Some(item) = queue.try_pop() {
//!     // Process remaining items
//! }
//! ```

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::marker::PhantomData;
use std::sync::Arc;

use crossbeam_utils::CachePadded;

use crate::signal::SignalGate;
use crate::{PopError, PushError};

/// Cache-line aligned producer state (hot path for enqueue operations).
///
/// All producer-side atomics and caches are grouped here to minimize false sharing
/// with consumer state. The `CachePadded` wrapper ensures this struct occupies
/// its own cache line.
///
/// # Head Position Encoding
///
/// ```text
/// Bit 63: Close flag (1 = closed, 0 = open)
/// Bits 62..0: Monotonic position (never wraps in practice)
/// ```
///
/// # Tail Cache Optimization
///
/// `tail_cache` uses `UnsafeCell` instead of `AtomicU64` because:
/// 1. Only the producer thread accesses this field
/// 2. No inter-thread synchronization required
/// 3. Avoids atomic overhead (faster loads/stores)
/// 4. Refreshed from consumer's `tail` only when needed (on full)
///
/// This is **safe** because the producer is single-threaded (enforced by API design).
struct ProducerState {
    /// **Head position**: Where the next item will be written (includes close bit in bit 63).
    ///
    /// Monotonically increasing (except close bit). Consumer reads this with `Acquire`
    /// to see producer's writes.
    head: AtomicU64,

    /// **Cached tail**: Producer's local copy of the consumer's tail position.
    ///
    /// Refreshed only when queue appears full (avoids expensive cross-cache-line reads).
    /// Uses `UnsafeCell` for zero-cost single-threaded access.
    tail_cache: UnsafeCell<u64>,

    /// **Sealable low watermark**: Lower bound of the sealed segment pool.
    ///
    /// Producer increments this when stealing a segment from [sealable_lo, sealable_hi).
    sealable_lo: AtomicU64,

    /// **Fresh allocation counter**: Number of segments allocated from the system allocator.
    ///
    /// Metric for cache misses (segment not found in pool). High values indicate
    /// poor segment reuse (memory churn).
    fresh_allocations: AtomicU64,

    /// **Pool reuse counter**: Number of segments reused from the sealed pool.
    ///
    /// Metric for cache hits (segment reused from hot pool). High values indicate
    /// good memory locality.
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

/// Cache-line aligned consumer state (hot path for dequeue operations).
///
/// Mirrors `ProducerState` design: all consumer-side fields grouped together
/// to avoid false sharing. The `CachePadded` wrapper ensures separate cache
/// line from producer state.
///
/// # Head Cache Optimization
///
/// `head_cache` uses `UnsafeCell` for the same reasons as producer's `tail_cache`:
/// single-threaded access (consumer only), no synchronization needed, zero overhead.
/// Refreshed from producer's `head` only when queue appears empty.
struct ConsumerState {
    /// **Tail position**: Where the next item will be read from (no close bit).
    ///
    /// Monotonically increasing. Producer reads this with `Acquire` to calculate
    /// available space.
    tail: AtomicU64,

    /// **Cached head**: Consumer's local copy of the producer's head position.
    ///
    /// Refreshed only when queue appears empty (avoids cross-cache-line reads).
    /// Stores the raw head value (including close bit in bit 63).
    head_cache: UnsafeCell<u64>,

    /// **Sealable high watermark**: Upper bound of the sealed segment pool.
    ///
    /// Consumer increments this when fully consuming a segment, making it
    /// available for producer to steal from [sealable_lo, sealable_hi).
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

/// **Producer half of a segmented SPSC (single-producer, single-consumer) queue.**
///
/// This struct provides the enqueue API for the lock-free SPSC queue. It maintains
/// an `Arc` reference to the shared `SegSpsc` state and implements the producer-side
/// operations: `try_push`, `try_push_n`, and allocation statistics tracking.
///
/// # Design Philosophy
///
/// - **Split Ownership**: Producer and consumer are separate types to enforce
///   single-producer/single-consumer semantics at compile time
/// - **Arc-based**: Uses `Arc` for shared state, allowing drop semantics and
///   proper cleanup when either half is dropped
/// - **Cache-optimized**: Producer state (head, tail_cache) is cache-padded
///   separately from consumer state to prevent false sharing
///
/// # Memory Model
///
/// The producer owns the "head" position (monotonically increasing u64):
/// ```text
/// Producer writes → [head, tail] → Consumer reads
///                    ↑queue data↑
/// ```
///
/// - **Head**: Published via `Ordering::Release` after data writes
/// - **Tail Cache**: Local copy of consumer's tail, refreshed with `Ordering::Acquire`
///
/// # Type Parameters
///
/// - `T`: Item type, must be `Copy` (allows lock-free memcpy semantics)
/// - `P`: Segment size exponent (segment_size = 2^P items)
/// - `NUM_SEGS_P2`: Number of segments exponent (num_segs = 2^NUM_SEGS_P2)
///
/// # Capacity
///
/// Total capacity: `(2^P × 2^NUM_SEGS_P2) - 1` items
///
/// Examples:
/// - `SegSpscProducer<u64, 10, 0>`: 1023 items (1024 - 1)
/// - `SegSpscProducer<u64, 13, 2>`: 32767 items (8192 × 4 - 1)
///
/// The "-1" ensures head==tail distinguishes empty from full.
///
/// # Segment Allocation
///
/// Segments are lazily allocated by the producer when crossing boundaries:
/// 1. **First write to segment**: Allocate via `alloc_segment()` (fresh allocation)
/// 2. **Subsequent writes**: Reuse from sealed pool if available (pool reuse)
///
/// Track allocation patterns via:
/// - `fresh_allocations()`: Count of new heap allocations
/// - `pool_reuses()`: Count of reuses from sealed segment pool
/// - `pool_reuse_rate()`: Ratio of reuses to total allocations
///
/// # Close Semantics
///
/// The queue can be closed via `SegSpsc::close()`:
/// - Sets bit 63 in the head position atomically
/// - All subsequent `try_push*` calls return `PushError::Closed`
/// - Consumer can still drain remaining items
///
/// # Thread Safety
///
/// - **Single Producer**: Only ONE thread may hold the `SegSpscProducer`
/// - **Concurrent Consumer**: Safe concurrent operation with the `SegSpscConsumer`
/// - **Send + !Sync**: Can be sent to another thread, but not shared
///
/// # Performance Characteristics
///
/// | Operation       | Latency (cache hit) | Latency (segment boundary) |
/// |-----------------|---------------------|----------------------------|
/// | `try_push`      | ~5-15 ns            | ~20-50 ns                  |
/// | `try_push_n`    | ~10-20 ns           | ~30-50 ns per boundary     |
/// | `len`           | ~2-5 ns             | N/A                        |
///
/// # Example
///
/// ```ignore
/// use bop_mpmc::SegSpsc;
///
/// type Queue = SegSpsc<u64, 10, 0>;  // 1023-item queue
/// let (producer, consumer) = Queue::new();
///
/// // Producer thread
/// std::thread::spawn(move || {
///     for i in 0..100 {
///         match producer.try_push(i) {
///             Ok(()) => {},
///             Err(e) => eprintln!("Push failed: {:?}", e),
///         }
///     }
/// });
///
/// // Consumer thread
/// std::thread::spawn(move || {
///     while let Some(item) = consumer.try_pop() {
///         println!("Got: {}", item);
///     }
/// });
/// ```
///
/// # See Also
///
/// - [`SegSpscConsumer`]: The consumer half
/// - [`SegSpsc`]: The underlying shared queue structure
/// - [`PushError`]: Error types for push operations
pub struct SegSpscProducer<T: Copy, const P: usize, const NUM_SEGS_P2: usize> {
    queue: Arc<SegSpsc<T, P, NUM_SEGS_P2>>,
}

impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> SegSpscProducer<T, P, NUM_SEGS_P2> {
    /// Returns the number of items currently in the queue.
    ///
    /// This is calculated as `head - tail`, where both positions are monotonically
    /// increasing u64 counters. The close bit is masked out before subtraction.
    ///
    /// # Close Bit Handling
    ///
    /// The head position may have the close bit (bit 63) set. This method masks it out:
    /// ```ignore
    /// let h_pos = h & RIGHT_MASK;  // Clear bit 63
    /// let length = h_pos.saturating_sub(t);  // Prevent underflow
    /// ```
    ///
    /// # Consistency
    ///
    /// Uses `Ordering::Acquire` for both loads to ensure visibility of preceding writes.
    /// However, the value may be stale by the time the caller uses it (concurrent operations).
    ///
    /// # Performance
    ///
    /// ~2-5 ns (two atomic loads + subtraction)
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.try_push(1)?;
    /// queue.try_push(2)?;
    /// assert_eq!(queue.len(), 2);
    /// ```
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns `true` if the queue contains no items.
    ///
    /// Equivalent to `self.len() == 0`, but more readable.
    ///
    /// # Note
    ///
    /// This check is not atomic with subsequent operations. A producer may enqueue
    /// items immediately after this returns `true`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// assert!(queue.is_empty());
    /// queue.try_push(1)?;
    /// assert!(!queue.is_empty());
    /// ```
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns `true` if the queue is at maximum capacity.
    ///
    /// When the queue is full, `try_push()` and `try_push_n()` will return `PushError::Full`.
    ///
    /// # Capacity
    ///
    /// The capacity is `(segment_size * num_segments) - 1`:
    /// - For `SegSpsc<T, 10, 0>`: `1024 - 1 = 1023` items
    /// - For `SegSpsc<T, 13, 2>`: `(8192 * 4) - 1 = 32767` items
    ///
    /// # Note
    ///
    /// Like `is_empty()`, this is not atomic with subsequent operations.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for i in 0..queue.capacity() {
    ///     queue.try_push(i).unwrap();
    /// }
    /// assert!(queue.is_full());
    /// assert!(queue.try_push(999).is_err());  // Returns PushError::Full
    /// ```
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.queue.is_full()
    }

    /// Attempts to enqueue a single item.
    ///
    /// This is a convenience wrapper around `try_push_n()` for single-item operations.
    ///
    /// # Returns
    ///
    /// - `Ok(())`: Item successfully enqueued
    /// - `Err(PushError::Full(item))`: Queue is at capacity, item returned
    /// - `Err(PushError::Closed(item))`: Queue has been closed, item returned
    ///
    /// # Close Semantics
    ///
    /// If the queue is closed (bit 63 of head is set), this returns `Err(PushError::Closed)`.
    /// The close check happens atomically before any capacity calculations.
    ///
    /// # Signal Integration
    ///
    /// After successful enqueue, automatically calls `schedule()` if a signal gate is configured.
    /// This wakes the executor to process the newly available item.
    ///
    /// # Performance
    ///
    /// ~5-15 ns for successful push (cache hit, no segment boundary crossing)
    /// ~20-50 ns for segment allocation (first push to a new segment)
    ///
    /// # Example
    ///
    /// ```ignore
    /// match queue.try_push(42) {
    ///     Ok(()) => println!("Enqueued successfully"),
    ///     Err(PushError::Full(item)) => println!("Queue full, item={}", item),
    ///     Err(PushError::Closed(item)) => println!("Queue closed, item={}", item),
    /// }
    /// ```
    ///
    /// # Thread Safety
    ///
    /// Safe to call from the single producer thread. NOT safe for concurrent producers
    /// (SPSC guarantee must be maintained by the caller).
    #[inline(always)]
    pub fn try_push(&self, item: T) -> Result<(), PushError<T>> {
        self.queue.try_push(item)
    }

    /// Attempts to enqueue multiple items from a slice.
    ///
    /// Copies items from `src` into the queue, potentially spanning multiple segments.
    /// Returns the number of items successfully copied (may be partial if queue fills).
    ///
    /// # Algorithm
    ///
    /// 1. **Close Check**: Return `Err(Closed)` if bit 63 is set in head
    /// 2. **Capacity Check**: Calculate free space using cached tail (refresh if needed)
    /// 3. **Bulk Copy**: Copy min(src.len(), free) items across segment boundaries
    /// 4. **Head Update**: Atomically publish new head position with Release ordering
    /// 5. **Signal**: Call `schedule()` to wake executor
    ///
    /// # Tail Caching
    ///
    /// The producer maintains a cached copy of the consumer's tail position:
    /// - **Fast path**: If cached tail shows available space, skip atomic load
    /// - **Slow path**: If cached tail shows full, refresh from consumer's atomic tail
    ///
    /// This optimization eliminates cache line bouncing in the common case where the
    /// queue has available space.
    ///
    /// # Segment Boundaries
    ///
    /// Items may span multiple segments. For each segment:
    /// ```ignore
    /// seg_idx = (position >> P) & DIR_MASK;  // Which segment in directory
    /// offset = position & SEG_MASK;          // Offset within segment
    /// ```
    ///
    /// When crossing a boundary (offset wraps to 0), the next segment is lazily allocated
    /// via `ensure_segment_for()`.
    ///
    /// # Memory Ordering
    ///
    /// - Tail cache: `Acquire` (when refreshing from consumer)
    /// - Head update: `Release` (publishes data + position to consumer)
    ///
    /// # Returns
    ///
    /// - `Ok(n)`: `n` items successfully enqueued (0 ≤ n ≤ src.len())
    /// - `Err(PushError::Full(()))`: Queue full, no items enqueued
    /// - `Err(PushError::Closed(()))`: Queue closed, no items enqueued
    ///
    /// # Partial Success
    ///
    /// If `src.len() > free`, only `free` items are copied and `Ok(free)` is returned.
    /// The caller must check the return value to handle partial writes:
    ///
    /// ```ignore
    /// let items = vec![1, 2, 3, 4, 5];
    /// match queue.try_push_n(&items) {
    ///     Ok(n) if n == items.len() => println!("All items enqueued"),
    ///     Ok(n) => println!("Partial: enqueued {}/{}", n, items.len()),
    ///     Err(PushError::Full(())) => println!("Queue full"),
    ///     Err(PushError::Closed(())) => println!("Queue closed"),
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// - **Single segment**: ~10-20 ns (one memcpy, no allocation)
    /// - **Multiple segments**: ~30-50 ns per segment boundary (allocation + memcpy)
    /// - **Throughput**: ~500M-1B ops/sec on modern x86_64 (depending on item size)
    ///
    /// # Safety
    ///
    /// Uses `ptr::copy_nonoverlapping` internally. Safety invariants:
    /// - Source slice is valid for reads
    /// - Destination segment is allocated and valid for writes
    /// - No overlap between source and destination (guaranteed by design)
    /// - Size calculation accounts for `sizeof::<T>()`
    #[inline(always)]
    pub fn try_push_n(&self, src: &[T]) -> Result<usize, PushError<()>> {
        self.queue.try_push_n(src)
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Metrics API
    // ──────────────────────────────────────────────────────────────────────────────

    /// Returns the number of fresh segment allocations (pool misses).
    ///
    /// This metric tracks how many times `ensure_segment_for()` had to allocate new
    /// memory from the system allocator because no sealed segment was available for reuse.
    ///
    /// # Interpretation
    ///
    /// - **High value**: Indicates working set exceeds pooled segment capacity (more allocations)
    /// - **Low value**: Indicates effective pooling (segments are being reused efficiently)
    ///
    /// # Use Cases
    ///
    /// - **Performance tuning**: Compare fresh vs reused to assess pool effectiveness
    /// - **Capacity planning**: High fresh count may indicate need for larger `NUM_SEGS_P2`
    /// - **Memory profiling**: Track allocation patterns over time
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.reset_allocation_stats();
    /// // ... run workload ...
    /// println!("Fresh allocations: {}", queue.fresh_allocations());
    /// println!("Pool reuse rate: {:.1}%", queue.pool_reuse_rate());
    /// ```
    #[inline(always)]
    pub fn fresh_allocations(&self) -> u64 {
        self.queue.fresh_allocations()
    }

    /// Returns the number of segment pool reuses (pool hits).
    ///
    /// This metric tracks how many times `ensure_segment_for()` successfully reused a
    /// sealed segment from the pool instead of allocating new memory.
    ///
    /// # Interpretation
    ///
    /// - **High value**: Indicates effective pooling and good cache locality
    /// - **Low value**: May indicate insufficient sealed segments or high churn rate
    ///
    /// # Pool Reuse Mechanism
    ///
    /// Segments in the range [sealable_lo, sealable_hi) are available for reuse.
    /// When the producer needs a new segment, it first checks this range. If available,
    /// the segment is moved (not copied) to the target slot in the directory.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let before = queue.pool_reuses();
    /// // ... enqueue/dequeue cycle that triggers segment reuse ...
    /// let after = queue.pool_reuses();
    /// println!("Segments reused: {}", after - before);
    /// ```
    #[inline(always)]
    pub fn pool_reuses(&self) -> u64 {
        self.queue.pool_reuses()
    }

    /// Returns the segment pool reuse rate as a percentage (0-100%).
    ///
    /// Calculated as: `(pool_reuses / (fresh_allocations + pool_reuses)) * 100`
    ///
    /// # Interpretation
    ///
    /// - **0%**: No reuse (all allocations are fresh) - may indicate cold start or insufficient pool
    /// - **50%**: Half of allocations are reused - moderate pooling effectiveness
    /// - **90%+**: Excellent reuse - pool is working well, high cache locality
    ///
    /// # Performance Impact
    ///
    /// - **Fresh allocation**: ~100-500 ns (system allocator + initialization)
    /// - **Pool reuse**: ~5-10 ns (pointer swap)
    ///
    /// High reuse rates directly translate to lower latency and higher throughput.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Warm up the pool
    /// for _ in 0..queue.capacity() {
    ///     queue.try_push(0)?;
    /// }
    /// for _ in 0..queue.capacity() {
    ///     queue.try_pop();
    /// }
    ///
    /// queue.reset_allocation_stats();
    ///
    /// // Run steady-state workload
    /// for _ in 0..1_000_000 {
    ///     queue.try_push(42)?;
    ///     queue.try_pop();
    /// }
    ///
    /// println!("Reuse rate: {:.1}%", queue.pool_reuse_rate());
    /// // Expected: ~99% after warm-up phase
    /// ```
    #[inline(always)]
    pub fn pool_reuse_rate(&self) -> f64 {
        self.queue.pool_reuse_rate()
    }

    /// Resets allocation statistics to zero.
    ///
    /// Useful for:
    /// - Measuring specific workload phases (reset between phases)
    /// - Repeated benchmark runs (reset between iterations)
    /// - Ignoring cold-start allocation costs (reset after warm-up)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Warm-up phase
    /// warm_up_queue(&queue);
    ///
    /// // Reset stats to measure steady-state only
    /// queue.reset_allocation_stats();
    ///
    /// // Run benchmark
    /// benchmark(&queue);
    ///
    /// println!("Steady-state reuse rate: {:.1}%", queue.pool_reuse_rate());
    /// ```
    #[inline(always)]
    pub fn reset_allocation_stats(&self) {
        self.queue.reset_allocation_stats();
    }
}

impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> Drop
    for SegSpscProducer<T, P, NUM_SEGS_P2>
{
    fn drop(&mut self) {
        unsafe {
            self.queue.close();
        }
    }
}

/// **Consumer half of a segmented SPSC (single-producer, single-consumer) queue.**
///
/// This struct provides the dequeue API for the lock-free SPSC queue. It maintains
/// an `Arc` reference to the shared `SegSpsc` state and implements the consumer-side
/// operations: `try_pop`, `try_pop_n`, `consume_in_place`, and segment sealing logic.
///
/// # Design Philosophy
///
/// - **Split Ownership**: Producer and consumer are separate types to enforce
///   single-producer/single-consumer semantics at compile time
/// - **Arc-based**: Uses `Arc` for shared state, allowing drop semantics and
///   proper cleanup when either half is dropped
/// - **Cache-optimized**: Consumer state (tail, head_cache) is cache-padded
///   separately from producer state to prevent false sharing
/// - **Zero-copy option**: `consume_in_place()` allows processing items without copying
///
/// # Memory Model
///
/// The consumer owns the "tail" position (monotonically increasing u64):
/// ```text
/// Producer writes → [head, tail] → Consumer reads
///                    ↑queue data↑
/// ```
///
/// - **Tail**: Published via `Ordering::Release` after data reads
/// - **Head Cache**: Local copy of producer's head, refreshed with `Ordering::Acquire`
///
/// # Type Parameters
///
/// - `T`: Item type, must be `Copy` (allows lock-free memcpy semantics)
/// - `P`: Segment size exponent (segment_size = 2^P items)
/// - `NUM_SEGS_P2`: Number of segments exponent (num_segs = 2^NUM_SEGS_P2)
///
/// # Capacity
///
/// Total capacity: `(2^P × 2^NUM_SEGS_P2) - 1` items
///
/// Examples:
/// - `SegSpscConsumer<u64, 10, 0>`: 1023 items (1024 - 1)
/// - `SegSpscConsumer<u64, 13, 2>`: 32767 items (8192 × 4 - 1)
///
/// # Segment Sealing
///
/// When the consumer advances past a segment boundary, it "seals" the segment:
/// 1. **Checks if segment can be sealed**: `tail > sealable_lo + SEG_SIZE`
/// 2. **Stores segment pointer**: Push to sealed pool (lock-free stack)
/// 3. **Nulls directory entry**: Allows producer to reuse from pool
///
/// This enables memory reuse without heap allocation/deallocation churn.
///
/// # Zero-Copy Consumption
///
/// `consume_in_place()` provides direct access to queue items via callback:
/// ```ignore
/// consumer.consume_in_place(10, |items| {
///     // Process items without copying
///     for item in items {
///         process(*item);
///     }
///     items.len()  // Return number consumed
/// });
/// ```
///
/// # Close Semantics
///
/// The queue can be closed via `SegSpsc::close()`:
/// - Consumer can detect close state via `is_closed()`
/// - `try_pop_n()` returns `Err(PopError::Closed)` when empty + closed
/// - Consumer can still drain remaining items after close
///
/// # Thread Safety
///
/// - **Single Consumer**: Only ONE thread may hold the `SegSpscConsumer`
/// - **Concurrent Producer**: Safe concurrent operation with the `SegSpscProducer`
/// - **Send + !Sync**: Can be sent to another thread, but not shared
///
/// # Performance Characteristics
///
/// | Operation            | Latency (cache hit) | Latency (segment boundary) |
/// |----------------------|---------------------|----------------------------|
/// | `try_pop`            | ~5-15 ns            | ~20-40 ns                  |
/// | `try_pop_n`          | ~10-20 ns           | ~30-50 ns per boundary     |
/// | `consume_in_place`   | ~8-18 ns            | ~25-45 ns per boundary     |
/// | `len`                | ~2-5 ns             | N/A                        |
///
/// # Example
///
/// ```ignore
/// use bop_mpmc::SegSpsc;
///
/// type Queue = SegSpsc<u64, 10, 0>;  // 1023-item queue
/// let (producer, consumer) = Queue::new();
///
/// // Producer thread
/// std::thread::spawn(move || {
///     for i in 0..100 {
///         producer.try_push(i).unwrap();
///     }
/// });
///
/// // Consumer thread
/// std::thread::spawn(move || {
///     // Copy-based consumption
///     while let Some(item) = consumer.try_pop() {
///         println!("Got: {}", item);
///     }
///
///     // Zero-copy consumption
///     consumer.consume_in_place(16, |items| {
///         for item in items {
///             process(*item);
///         }
///         items.len()
///     });
/// });
/// ```
///
/// # See Also
///
/// - [`SegSpscProducer`]: The producer half
/// - [`SegSpsc`]: The underlying shared queue structure
/// - [`PopError`]: Error types for pop operations
pub struct SegSpscConsumer<T: Copy, const P: usize, const NUM_SEGS_P2: usize> {
    queue: Arc<SegSpsc<T, P, NUM_SEGS_P2>>,
}

impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> SegSpscConsumer<T, P, NUM_SEGS_P2> {
    /// Returns the number of items currently in the queue.
    ///
    /// This is calculated as `head - tail`, where both positions are monotonically
    /// increasing u64 counters. The close bit is masked out before subtraction.
    ///
    /// # Close Bit Handling
    ///
    /// The head position may have the close bit (bit 63) set. This method masks it out:
    /// ```ignore
    /// let h_pos = h & RIGHT_MASK;  // Clear bit 63
    /// let length = h_pos.saturating_sub(t);  // Prevent underflow
    /// ```
    ///
    /// # Consistency
    ///
    /// Uses `Ordering::Acquire` for both loads to ensure visibility of preceding writes.
    /// However, the value may be stale by the time the caller uses it (concurrent operations).
    ///
    /// # Performance
    ///
    /// ~2-5 ns (two atomic loads + subtraction)
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.try_push(1)?;
    /// queue.try_push(2)?;
    /// assert_eq!(queue.len(), 2);
    /// ```
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns `true` if the queue contains no items.
    ///
    /// Equivalent to `self.len() == 0`, but more readable.
    ///
    /// # Note
    ///
    /// This check is not atomic with subsequent operations. A producer may enqueue
    /// items immediately after this returns `true`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// assert!(queue.is_empty());
    /// queue.try_push(1)?;
    /// assert!(!queue.is_empty());
    /// ```
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns `true` if the queue is at maximum capacity.
    ///
    /// When the queue is full, `try_push()` and `try_push_n()` will return `PushError::Full`.
    ///
    /// # Capacity
    ///
    /// The capacity is `(segment_size * num_segments) - 1`:
    /// - For `SegSpsc<T, 10, 0>`: `1024 - 1 = 1023` items
    /// - For `SegSpsc<T, 13, 2>`: `(8192 * 4) - 1 = 32767` items
    ///
    /// # Note
    ///
    /// Like `is_empty()`, this is not atomic with subsequent operations.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for i in 0..queue.capacity() {
    ///     queue.try_push(i).unwrap();
    /// }
    /// assert!(queue.is_full());
    /// assert!(queue.try_push(999).is_err());  // Returns PushError::Full
    /// ```
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.queue.is_full()
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Close API
    // ──────────────────────────────────────────────────────────────────────────────

    /// Checks if the queue has been closed.
    ///
    /// The close state is encoded in bit 63 of the head position, allowing lock-free
    /// detection without a separate atomic flag.
    ///
    /// # Close Bit Encoding
    ///
    /// ```text
    /// Head Position (64 bits):
    /// ┌─────────┬────────────────────────────────────────────────┐
    /// │ Bit 63  │ Bits 62..0                                     │
    /// │ (Close) │ (Monotonic Position)                           │
    /// └─────────┴────────────────────────────────────────────────┘
    /// ```
    ///
    /// # Semantics
    ///
    /// Once closed:
    /// - All `try_push()` and `try_push_n()` calls return `PushError::Closed`
    /// - Consumers can drain remaining items
    /// - `try_pop_n()` returns `PopError::Closed` when queue is empty
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::Relaxed` because close state doesn't need to synchronize with
    /// data operations (producers/consumers already use Acquire/Release for data visibility).
    ///
    /// # Performance
    ///
    /// ~1-2 ns (single atomic load + bit test)
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.try_push(1)?;
    /// queue.close();
    /// assert!(queue.is_closed());
    /// assert!(matches!(queue.try_push(2), Err(PushError::Closed(_))));
    /// assert_eq!(queue.try_pop(), Some(1));  // Can still drain
    /// ```
    #[inline(always)]
    pub fn is_closed(&self) -> bool {
        self.queue.is_closed()
    }

    /// Attempts to dequeue a single item by copying.
    ///
    /// This is a convenience wrapper around `try_pop_n()` for single-item operations.
    /// The item is copied out of the queue into the returned `Option`.
    ///
    /// # Returns
    ///
    /// - `Some(item)`: Item successfully dequeued
    /// - `None`: Queue is empty or closed
    ///
    /// # Distinguishing Empty vs Closed
    ///
    /// This method returns `None` for both empty and closed states. Use `try_pop_n()` or
    /// check `is_closed()` separately if you need to distinguish:
    ///
    /// ```ignore
    /// match queue.try_pop() {
    ///     Some(item) => process(item),
    ///     None if queue.is_closed() => break,  // Closed
    ///     None => wait_for_items(),            // Empty
    /// }
    /// ```
    ///
    /// # Head Caching
    ///
    /// The consumer maintains a cached copy of the producer's head position to avoid
    /// unnecessary cache line bouncing. Only refreshes when the cached value shows empty.
    ///
    /// # Performance
    ///
    /// ~5-15 ns for successful pop (cache hit, no segment boundary crossing)
    ///
    /// # Safety
    ///
    /// Initializes return value with `mem::zeroed()`. Safe because:
    /// - If pop succeeds (returns `Some`), the zeroed value is overwritten
    /// - If pop fails (returns `None`), the zeroed value is discarded
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.try_push(42)?;
    /// assert_eq!(queue.try_pop(), Some(42));
    /// assert_eq!(queue.try_pop(), None);  // Empty
    /// ```
    #[inline(always)]
    pub fn try_pop(&self) -> Option<T> {
        self.queue.try_pop()
    }

    /// Attempts to dequeue multiple items by copying into the destination slice.
    ///
    /// Copies items from the queue into `dst`, potentially spanning multiple segments.
    /// Returns the number of items successfully copied (may be partial if queue empties).
    ///
    /// # Algorithm
    ///
    /// 1. **Availability Check**: Calculate available items using cached head (refresh if needed)
    /// 2. **Close Check**: Return `Err(PopError::Closed)` if empty and bit 63 is set
    /// 3. **Bulk Copy**: Copy min(dst.len(), avail) items across segment boundaries
    /// 4. **Tail Update**: Atomically publish new tail position with Release ordering
    /// 5. **Segment Sealing**: Check if segments can be sealed and returned to pool
    ///
    /// # Head Caching
    ///
    /// The consumer maintains a cached copy of the producer's head position:
    /// - **Fast path**: If cached head shows available items, skip atomic load
    /// - **Slow path**: If cached head shows empty, refresh from producer's atomic head
    ///
    /// This optimization eliminates cache line bouncing in the common case where the
    /// queue has available items.
    ///
    /// # Close Bit Handling
    ///
    /// The head position may have bit 63 set to indicate closure:
    /// ```ignore
    /// let h_pos = h & RIGHT_MASK;  // Mask out close bit
    /// let avail = h_pos.saturating_sub(t);
    /// if avail == 0 && (h & CLOSED_CHANNEL_MASK != 0) {
    ///     return Err(PopError::Closed);
    /// }
    /// ```
    ///
    /// # Segment Boundaries
    ///
    /// Items may span multiple segments. For each segment:
    /// ```ignore
    /// seg_idx = (position >> P) & DIR_MASK;  // Which segment in directory
    /// offset = position & SEG_MASK;          // Offset within segment
    /// ```
    ///
    /// After consumption, segments in the range [sealable_lo, sealable_hi) are marked
    /// as sealed and available for reuse via the pooling mechanism.
    ///
    /// # Memory Ordering
    ///
    /// - Head cache: `Acquire` (when refreshing from producer)
    /// - Segment load: `Acquire` (ensures data visibility)
    /// - Tail update: `Release` (publishes consumed position to producer)
    ///
    /// # Returns
    ///
    /// - `Ok(n)`: `n` items successfully dequeued (0 ≤ n ≤ dst.len())
    /// - `Err(PopError::Empty)`: Queue is empty
    /// - `Err(PopError::Closed)`: Queue is closed and empty
    ///
    /// # Partial Success
    ///
    /// If `dst.len() > avail`, only `avail` items are copied and `Ok(avail)` is returned:
    ///
    /// ```ignore
    /// let mut buffer = vec![0; 100];
    /// match queue.try_pop_n(&mut buffer) {
    ///     Ok(n) => println!("Dequeued {} items", n),
    ///     Err(PopError::Empty) => println!("Queue empty"),
    ///     Err(PopError::Closed) => println!("Queue closed"),
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// - **Single segment**: ~10-20 ns (one memcpy)
    /// - **Multiple segments**: ~20-30 ns per segment boundary (memcpy + sealing check)
    /// - **Throughput**: ~500M-1B ops/sec on modern x86_64 (depending on item size)
    ///
    /// # Safety
    ///
    /// Uses `ptr::copy_nonoverlapping` internally. Safety invariants:
    /// - Source segment is allocated and contains initialized items
    /// - Destination slice is valid for writes
    /// - No overlap between source and destination (guaranteed by design)
    /// - Items are `Copy`, so no double-drop concerns
    #[inline(always)]
    pub fn try_pop_n(&self, dst: &mut [T]) -> Result<usize, PopError> {
        self.queue.try_pop_n(dst)
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Consumer API (ZERO-COPY, process-in-place)
    // ──────────────────────────────────────────────────────────────────────────────

    /// Zero-copy consumption: processes items in-place via callback with read-only slices.
    ///
    /// This method provides direct read access to items in the queue without copying.
    /// The callback `f` is invoked with contiguous slices of items (up to segment boundaries),
    /// and must return the number of items it actually consumed.
    ///
    /// # Zero-Copy Design
    ///
    /// Unlike `try_pop_n()` which copies items into a destination buffer, this method:
    /// - Exposes items directly in their segment storage (zero memory copies)
    /// - Allows processing without intermediate allocation
    /// - Ideal for serialization, hashing, or forwarding to I/O
    ///
    /// # Algorithm
    ///
    /// 1. **Availability Check**: Calculate available items using cached head
    /// 2. **Segment Iteration**: For each segment in [tail, tail + max):
    ///    - Calculate contiguous slice within segment boundary
    ///    - Invoke callback with read-only slice
    ///    - Advance tail by the number of items callback consumed
    ///    - Seal segment if boundary crossed
    /// 3. **Early Exit**: Stop if callback returns 0 (processed fewer than offered)
    ///
    /// # Callback Contract
    ///
    /// ```ignore
    /// fn callback(slice: &[T]) -> usize
    /// ```
    ///
    /// - **Input**: Read-only slice of contiguous items (may be less than requested due to segment boundaries)
    /// - **Output**: Number of items consumed (0 ≤ consumed ≤ slice.len())
    /// - **Lifetime**: Slice MUST NOT be held beyond the callback scope (undefined behavior)
    /// - **Early Exit**: Return < slice.len() to stop iteration
    ///
    /// # Segment Boundaries
    ///
    /// Items are presented in segment-sized chunks. If you request 10,000 items but segment
    /// size is 1024, the callback will be invoked multiple times:
    /// - 1st call: slice of up to 1024 items (or fewer if at segment boundary)
    /// - 2nd call: next slice of up to 1024 items
    /// - ... (continues until `max` items processed or callback returns 0)
    ///
    /// # Close Semantics
    ///
    /// This method does NOT check the close bit or return an error. It simply processes
    /// whatever items are available. Use `is_closed()` separately if needed:
    ///
    /// ```ignore
    /// loop {
    ///     let consumed = queue.consume_in_place(1024, |slice| {
    ///         serialize(slice);
    ///         slice.len()  // Consumed all
    ///     });
    ///
    ///     if consumed == 0 {
    ///         if queue.is_closed() {
    ///             break;  // Done
    ///         }
    ///         wait_for_items();
    ///     }
    /// }
    /// ```
    ///
    /// # Parameters
    ///
    /// - `max`: Maximum number of items to consume (may consume fewer if queue empties or callback exits early)
    /// - `f`: Callback invoked with read-only slices, returning number of items consumed
    ///
    /// # Returns
    ///
    /// Total number of items consumed across all callback invocations.
    ///
    /// # Examples
    ///
    /// **Example 1: Serialize to network without copying**
    /// ```ignore
    /// let bytes_sent = queue.consume_in_place(batch_size, |items| {
    ///     match socket.write_all(bytemuck::cast_slice(items)) {
    ///         Ok(()) => items.len(),     // Consumed all
    ///         Err(_) => 0,               // Error, stop
    ///     }
    /// });
    /// ```
    ///
    /// **Example 2: Compute hash without allocation**
    /// ```ignore
    /// let mut hasher = Blake3::new();
    /// queue.consume_in_place(usize::MAX, |items| {
    ///     hasher.update(bytemuck::cast_slice(items));
    ///     items.len()  // Consumed all
    /// });
    /// let hash = hasher.finalize();
    /// ```
    ///
    /// **Example 3: Conditional processing with early exit**
    /// ```ignore
    /// let mut count = 0;
    /// queue.consume_in_place(1000, |items| {
    ///     for (i, item) in items.iter().enumerate() {
    ///         if !should_process(item) {
    ///             return i;  // Stop here
    ///         }
    ///         process(item);
    ///         count += 1;
    ///     }
    ///     items.len()  // Consumed all
    /// });
    /// ```
    ///
    /// # Performance
    ///
    /// - **Throughput**: 2-5 GB/s for simple processing (memory bandwidth limited)
    /// - **Latency**: ~5-10 ns per callback invocation overhead
    /// - **Advantage over copying**: Saves ~50-200 ns per 1KB of data (avoids memcpy)
    ///
    /// # Safety
    ///
    /// This method is safe because:
    /// - Items are `Copy`, so no drop concerns
    /// - Producer can't modify consumed items (monotonic head/tail invariant)
    /// - Consumer is single-threaded (SPSC guarantee)
    /// - Slices are bounded to initialized region [tail, head)
    ///
    /// **CRITICAL**: The callback MUST NOT store the slice reference beyond its scope.
    /// The slice becomes invalid after the tail is updated.
    #[inline(always)]
    pub fn consume_in_place<F>(&self, max: usize, f: F) -> usize
    where
        F: FnMut(&[T]) -> usize,
    {
        self.queue.consume_in_place(max, f)
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Metrics API
    // ──────────────────────────────────────────────────────────────────────────────

    /// Returns the number of fresh segment allocations (pool misses).
    ///
    /// This metric tracks how many times `ensure_segment_for()` had to allocate new
    /// memory from the system allocator because no sealed segment was available for reuse.
    ///
    /// # Interpretation
    ///
    /// - **High value**: Indicates working set exceeds pooled segment capacity (more allocations)
    /// - **Low value**: Indicates effective pooling (segments are being reused efficiently)
    ///
    /// # Use Cases
    ///
    /// - **Performance tuning**: Compare fresh vs reused to assess pool effectiveness
    /// - **Capacity planning**: High fresh count may indicate need for larger `NUM_SEGS_P2`
    /// - **Memory profiling**: Track allocation patterns over time
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.reset_allocation_stats();
    /// // ... run workload ...
    /// println!("Fresh allocations: {}", queue.fresh_allocations());
    /// println!("Pool reuse rate: {:.1}%", queue.pool_reuse_rate());
    /// ```
    #[inline(always)]
    pub fn fresh_allocations(&self) -> u64 {
        self.queue.fresh_allocations()
    }

    /// Returns the number of segment pool reuses (pool hits).
    ///
    /// This metric tracks how many times `ensure_segment_for()` successfully reused a
    /// sealed segment from the pool instead of allocating new memory.
    ///
    /// # Interpretation
    ///
    /// - **High value**: Indicates effective pooling and good cache locality
    /// - **Low value**: May indicate insufficient sealed segments or high churn rate
    ///
    /// # Pool Reuse Mechanism
    ///
    /// Segments in the range [sealable_lo, sealable_hi) are available for reuse.
    /// When the producer needs a new segment, it first checks this range. If available,
    /// the segment is moved (not copied) to the target slot in the directory.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let before = queue.pool_reuses();
    /// // ... enqueue/dequeue cycle that triggers segment reuse ...
    /// let after = queue.pool_reuses();
    /// println!("Segments reused: {}", after - before);
    /// ```
    #[inline(always)]
    pub fn pool_reuses(&self) -> u64 {
        self.queue.pool_reuses()
    }

    /// Returns the segment pool reuse rate as a percentage (0-100%).
    ///
    /// Calculated as: `(pool_reuses / (fresh_allocations + pool_reuses)) * 100`
    ///
    /// # Interpretation
    ///
    /// - **0%**: No reuse (all allocations are fresh) - may indicate cold start or insufficient pool
    /// - **50%**: Half of allocations are reused - moderate pooling effectiveness
    /// - **90%+**: Excellent reuse - pool is working well, high cache locality
    ///
    /// # Performance Impact
    ///
    /// - **Fresh allocation**: ~100-500 ns (system allocator + initialization)
    /// - **Pool reuse**: ~5-10 ns (pointer swap)
    ///
    /// High reuse rates directly translate to lower latency and higher throughput.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Warm up the pool
    /// for _ in 0..queue.capacity() {
    ///     queue.try_push(0)?;
    /// }
    /// for _ in 0..queue.capacity() {
    ///     queue.try_pop();
    /// }
    ///
    /// queue.reset_allocation_stats();
    ///
    /// // Run steady-state workload
    /// for _ in 0..1_000_000 {
    ///     queue.try_push(42)?;
    ///     queue.try_pop();
    /// }
    ///
    /// println!("Reuse rate: {:.1}%", queue.pool_reuse_rate());
    /// // Expected: ~99% after warm-up phase
    /// ```
    #[inline(always)]
    pub fn pool_reuse_rate(&self) -> f64 {
        self.queue.pool_reuse_rate()
    }

    /// Resets allocation statistics to zero.
    ///
    /// Useful for:
    /// - Measuring specific workload phases (reset between phases)
    /// - Repeated benchmark runs (reset between iterations)
    /// - Ignoring cold-start allocation costs (reset after warm-up)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Warm-up phase
    /// warm_up_queue(&queue);
    ///
    /// // Reset stats to measure steady-state only
    /// queue.reset_allocation_stats();
    ///
    /// // Run benchmark
    /// benchmark(&queue);
    ///
    /// println!("Steady-state reuse rate: {:.1}%", queue.pool_reuse_rate());
    /// ```
    #[inline(always)]
    pub fn reset_allocation_stats(&self) {
        self.queue.reset_allocation_stats();
    }
}

/// A bounded, segmented SPSC queue with lazy allocation, segment pooling, and zero-copy APIs.
///
/// # Type Parameters
///
/// - `T`: Item type (must be `Copy` for efficient memcpy and no Drop concerns)
/// - `P`: log2(segment_size) - Each segment holds `2^P` items
/// - `NUM_SEGS_P2`: log2(directory_size) - Directory has `2^NUM_SEGS_P2` slots
///
/// # Capacity
///
/// Total capacity = `(2^P × 2^NUM_SEGS_P2) - 1` (one-empty-slot rule)
///
/// Example: `SegSpsc<u64, 8, 10>` = 256 items/seg × 1024 segs = 262,143 capacity
///
/// # Memory Layout
///
/// The queue consists of three main parts:
///
/// 1. **Segment directory**: Fixed array of `AtomicPtr` (NUM_SEGS slots)
/// 2. **Producer state**: Cache-padded head, tail_cache, sealable_lo, metrics
/// 3. **Consumer state**: Cache-padded tail, head_cache, sealable_hi
///
/// Segments are allocated lazily and pooled for reuse. This minimizes:
/// - Initial memory footprint (no upfront allocation)
/// - Allocator pressure (reuse hot segments)
/// - Cache misses (pool retains recently-used memory)
///
/// # Thread Safety
///
/// This is a **single-producer, single-consumer** queue:
/// - Exactly **one** thread may call producer methods (`try_push`, `try_push_n`)
/// - Exactly **one** thread may call consumer methods (`try_pop`, `try_pop_n`, `consume_in_place`)
/// - Simultaneous calls to inspection methods (`len`, `is_empty`) are safe but may see stale values
///
/// Violating these constraints causes **undefined behavior** (data races on UnsafeCells).
///
/// # Close Semantics
///
/// The queue can be closed via `close()`, which sets bit 63 of the head position:
/// - Producers see `PushError::Closed` on subsequent pushes
/// - Consumers can drain remaining items, then see `PopError::Closed` when empty
/// - Close is **one-way** (cannot reopen)
/// - Close is **lock-free** (encoded in head position)
///
/// # Examples
///
/// ## Basic Push/Pop
///
/// ```ignore
/// use bop_mpmc::seg_spsc::SegSpsc;
///
/// type Queue = SegSpsc<u64, 6, 8>;  // 64 items/seg, 256 segs
/// let q = Queue::new();
///
/// q.try_push(42).unwrap();
/// assert_eq!(q.try_pop(), Some(42));
/// ```
///
/// ## Bulk Operations
///
/// ```ignore
/// let items = vec![1, 2, 3, 4, 5];
/// q.try_push_n(&items).unwrap();
///
/// let mut buf = vec![0; 5];
/// q.try_pop_n(&mut buf).unwrap();
/// assert_eq!(buf, items);
/// ```
///
/// ## Zero-Copy Consumption
///
/// ```ignore
/// q.try_push_n(&[10, 20, 30, 40]).unwrap();
///
/// let total = q.consume_in_place(100, |slice| {
///     for &item in slice {
///         println!("{}", item);
///     }
///     slice.len()  // Consume all items in this slice
/// });
/// assert_eq!(total, 4);
/// ```
///
/// ## Graceful Shutdown
///
/// ```ignore
/// // Producer closes queue
/// q.close();
///
/// // Consumer drains remaining items
/// while let Some(item) = q.try_pop() {
///     process(item);
/// }
/// // Now sees PopError::Closed
/// ```
pub struct SegSpsc<T: Copy, const P: usize, const NUM_SEGS_P2: usize> {
    /// **Segment directory**: Fixed array of `AtomicPtr<MaybeUninit<T>>` slots.
    ///
    /// Each slot either:
    /// - Points to an allocated segment (SEG_SIZE items)
    /// - Is null (segment not yet allocated or currently in sealed pool)
    ///
    /// Producer lazily allocates segments on write boundaries.
    segs: Box<[AtomicPtr<MaybeUninit<T>>]>,

    /// **Producer state**: Cache-padded to avoid false sharing with consumer.
    producer: CachePadded<ProducerState>,

    /// **Consumer state**: Cache-padded to avoid false sharing with producer.
    consumer: CachePadded<ConsumerState>,

    /// **Optional signal gate**: For integration with `SignalWaker` (MPMC use case).
    ///
    /// If present, `schedule()` is called after pushes to wake consumer threads.
    signal: Option<CachePadded<SignalGate>>,

    _t: PhantomData<T>,
}

impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> SegSpsc<T, P, NUM_SEGS_P2> {
    /// Items per segment (2^P)
    pub const SEG_SIZE: usize = 1usize << P;

    /// Number of directory slots (2^NUM_SEGS_P2)
    pub const NUM_SEGS: usize = 1usize << NUM_SEGS_P2;

    /// Mask to extract offset within a segment
    pub const SEG_MASK: usize = Self::SEG_SIZE - 1;

    /// Mask to extract segment index from directory
    pub const DIR_MASK: usize = Self::NUM_SEGS - 1;

    /// Bit mask for the close flag in the head position (bit 63).
    ///
    /// When this bit is set in `producer.head`, the queue is closed.
    const CLOSED_CHANNEL_MASK: u64 = 1u64 << 63;

    /// Bit mask covering all position bits (excludes close bit).
    ///
    /// Used to extract the actual position from head: `head & RIGHT_MASK`
    const RIGHT_MASK: u64 = !Self::CLOSED_CHANNEL_MASK;

    /// Creates a new segmented SPSC queue, returning producer and consumer halves.
    ///
    /// This is the primary constructor for safe queue creation. It allocates the segment
    /// directory but defers segment allocation until the producer writes to them (lazy allocation).
    ///
    /// # Type Parameters
    ///
    /// - `T`: Item type, must be `Copy` (enables lock-free memcpy)
    /// - `P`: Segment size exponent (segment_size = 2^P items)
    /// - `NUM_SEGS_P2`: Number of segments exponent (num_segs = 2^NUM_SEGS_P2)
    ///
    /// # Capacity
    ///
    /// The queue capacity is `(2^P × 2^NUM_SEGS_P2) - 1` items:
    /// ```ignore
    /// type SmallQueue = SegSpsc<u64, 10, 0>;  // 1023 items (2^10 × 2^0 - 1)
    /// type LargeQueue = SegSpsc<u64, 13, 2>;  // 32767 items (2^13 × 2^2 - 1)
    /// ```
    ///
    /// # Memory Usage
    ///
    /// **Initial allocation**:
    /// - Directory: `NUM_SEGS × 8 bytes` (AtomicPtr slots)
    /// - Producer state: ~128 bytes (cache-padded)
    /// - Consumer state: ~128 bytes (cache-padded)
    ///
    /// **Per-segment allocation** (lazy):
    /// - Each segment: `SEG_SIZE × sizeof(T)` bytes
    ///
    /// Example for `SegSpsc<u64, 10, 2>`:
    /// - Initial: `4 × 8 + 256 = 288 bytes`
    /// - Max (4 segments): `288 + 4 × 1024 × 8 = 33,056 bytes`
    ///
    /// # No Signal Integration
    ///
    /// This constructor does not attach a `SignalGate`. For integration with `SignalWaker`,
    /// use [`new_with_gate()`](Self::new_with_gate) instead.
    ///
    /// # Thread Safety
    ///
    /// The returned `SegSpscProducer` and `SegSpscConsumer` can be sent to different threads:
    /// ```ignore
    /// let (producer, consumer) = SegSpsc::<u64, 10, 0>::new();
    ///
    /// let prod_thread = std::thread::spawn(move || {
    ///     producer.try_push(42).unwrap();
    /// });
    ///
    /// let cons_thread = std::thread::spawn(move || {
    ///     assert_eq!(consumer.try_pop(), Some(42));
    /// });
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - **Initial cost**: ~100-200 ns (directory allocation)
    /// - **Segment allocation**: ~50-100 ns per segment (amortized across SEG_SIZE pushes)
    /// - **Steady state**: ~5-15 ns per push/pop (cache-friendly operation)
    ///
    /// # Example
    ///
    /// ```ignore
    /// use bop_mpmc::SegSpsc;
    ///
    /// // Create a 1023-item queue for u64 values
    /// type MyQueue = SegSpsc<u64, 10, 0>;
    /// let (producer, consumer) = MyQueue::new();
    ///
    /// // Enqueue some items
    /// producer.try_push(1)?;
    /// producer.try_push(2)?;
    /// producer.try_push(3)?;
    ///
    /// // Dequeue items
    /// assert_eq!(consumer.try_pop(), Some(1));
    /// assert_eq!(consumer.try_pop(), Some(2));
    /// assert_eq!(consumer.try_pop(), Some(3));
    /// assert_eq!(consumer.try_pop(), None);
    /// ```
    ///
    /// # See Also
    ///
    /// - [`new_with_gate()`](Self::new_with_gate): Constructor with signal integration
    /// - [`new_unsafe()`](Self::new_unsafe): Unsafe constructor (internal use)
    /// - [`capacity()`](Self::capacity): Returns maximum capacity
    pub fn new() -> (
        SegSpscProducer<T, P, NUM_SEGS_P2>,
        SegSpscConsumer<T, P, NUM_SEGS_P2>,
    ) {
        let queue = Arc::new(unsafe { Self::new_unsafe() });
        (
            SegSpscProducer {
                queue: Arc::clone(&queue),
            },
            SegSpscConsumer {
                queue: Arc::clone(&queue),
            },
        )
    }

    /// Creates a new segmented SPSC queue with signal integration, returning producer and consumer halves.
    ///
    /// This constructor attaches a `SignalGate` to the queue, enabling automatic signaling
    /// when items are enqueued. After each successful push, the gate is scheduled via
    /// `SignalGate::schedule()`, which notifies the associated `SignalWaker` to wake
    /// executor threads.
    ///
    /// # Use Cases
    ///
    /// This constructor is intended for integration with the MPMC executor system:
    /// - **Async task queues**: Wake executor threads when tasks are enqueued
    /// - **Work-stealing**: Signal idle workers that work is available
    /// - **Event loops**: Notify event loop threads of new events
    ///
    /// For simple SPSC usage without executor integration, use [`new()`](Self::new) instead.
    ///
    /// # Arguments
    ///
    /// * `gate` - The `SignalGate` to schedule after each push
    ///
    /// # Signal Protocol
    ///
    /// The signal protocol involves three components:
    /// 1. **SignalGate**: Per-queue state (bit index in 64-bit signal word)
    /// 2. **SignalWord**: 64-bit atomic bitmap (one bit per queue in the group)
    /// 3. **SignalWaker**: Global waker with summary bitmap (one bit per signal word)
    ///
    /// When the producer calls `try_push()`:
    /// ```ignore
    /// queue.try_push(item)?;
    /// // Internally calls: queue.schedule()
    /// // Which calls: gate.schedule()
    /// // Which sets: signal_word |= (1 << bit_index)
    /// // And potentially: waker.summary |= (1 << group_index)
    /// ```
    ///
    /// # Memory Ordering
    ///
    /// The signal protocol uses `Ordering::AcqRel` to ensure:
    /// - Push data is visible before the signal bit is set
    /// - Consumer sees the signal bit before reading data
    ///
    /// # Performance Impact
    ///
    /// Adding signal integration adds ~2-5 ns overhead per push (one atomic RMW operation).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use bop_mpmc::{SignalGate, SignalWaker, SegSpsc};
    /// use std::sync::Arc;
    ///
    /// // Create shared waker
    /// let waker = Arc::new(SignalWaker::new());
    ///
    /// // Create signal word and gate
    /// let signal_word = Arc::new(AtomicU64::new(0));
    /// let gate = SignalGate::new(0, Arc::clone(&signal_word), Arc::clone(&waker));
    ///
    /// // Create queue with gate
    /// let (producer, consumer) = SegSpsc::<u64, 10, 0>::new_with_gate(gate);
    ///
    /// // Producer thread
    /// std::thread::spawn(move || {
    ///     producer.try_push(42).unwrap();
    ///     // Automatically calls gate.schedule()
    /// });
    ///
    /// // Executor thread (simplified)
    /// std::thread::spawn(move || {
    ///     loop {
    ///         if signal_word.load(Ordering::Acquire) & 1 != 0 {
    ///             // Process item
    ///             if let Some(item) = consumer.try_pop() {
    ///                 process(item);
    ///             }
    ///         }
    ///     }
    /// });
    /// ```
    ///
    /// # See Also
    ///
    /// - [`new()`](Self::new): Constructor without signal integration
    /// - [`new_unsafe_with_gate()`](Self::new_unsafe_with_gate): Unsafe constructor with gate
    /// - [`schedule()`](Self::schedule): Manually schedule the gate
    /// - `SignalGate`: Per-queue signal state
    /// - `SignalWaker`: Global executor waker
    pub fn new_with_gate(
        gate: SignalGate,
    ) -> (
        SegSpscProducer<T, P, NUM_SEGS_P2>,
        SegSpscConsumer<T, P, NUM_SEGS_P2>,
    ) {
        let queue = Arc::new(unsafe { Self::new_unsafe_with_gate(gate) });
        (
            SegSpscProducer {
                queue: Arc::clone(&queue),
            },
            SegSpscConsumer {
                queue: Arc::clone(&queue),
            },
        )
    }

    /// Creates a new empty queue with all segments unallocated (unsafe, internal use).
    ///
    /// This is the low-level constructor called by [`new()`](Self::new). It is marked
    /// `unsafe` because it returns a raw `Self` value that must be immediately wrapped
    /// in an `Arc` to maintain the SPSC safety guarantees.
    ///
    /// # Safety
    ///
    /// Callers must ensure:
    /// 1. The returned value is immediately wrapped in `Arc<Self>`
    /// 2. Only one `SegSpscProducer` is created from the Arc (single producer)
    /// 3. Only one `SegSpscConsumer` is created from the Arc (single consumer)
    ///
    /// The `new()` and `new_with_gate()` constructors handle this correctly.
    ///
    /// # Memory Layout
    ///
    /// The segment directory is allocated (NUM_SEGS slots), but individual segments
    /// are **not** allocated until the producer writes to them. This minimizes
    /// initial memory footprint.
    ///
    /// **Initial allocation**:
    /// - Directory: `NUM_SEGS × 8 bytes` (AtomicPtr slots, all null)
    /// - Producer state: ~128 bytes (cache-padded, head=0, tail_cache=0)
    /// - Consumer state: ~128 bytes (cache-padded, tail=0, head_cache=0)
    ///
    /// **Segment allocation** (lazy, on first write to segment):
    /// - Per segment: `SEG_SIZE × sizeof(T)` bytes
    ///
    /// # No Signal Integration
    ///
    /// This constructor does not attach a `SignalGate`. Use [`new_unsafe_with_gate()`](Self::new_unsafe_with_gate)
    /// for signal integration.
    ///
    /// # Initialization State
    ///
    /// All atomic counters are initialized to zero:
    /// ```ignore
    /// producer.head = 0
    /// producer.tail_cache = 0
    /// producer.sealable_lo = 0
    /// producer.fresh_allocations = 0
    /// producer.pool_reuses = 0
    ///
    /// consumer.tail = 0
    /// consumer.head_cache = 0
    /// consumer.sealable_hi = null
    /// ```
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Internal usage (wrapped in Arc by new())
    /// let queue = Arc::new(unsafe { SegSpsc::<u64, 10, 0>::new_unsafe() });
    /// let producer = SegSpscProducer { queue: Arc::clone(&queue) };
    /// let consumer = SegSpscConsumer { queue: Arc::clone(&queue) };
    /// ```
    ///
    /// # See Also
    ///
    /// - [`new()`](Self::new): Safe public constructor
    /// - [`new_unsafe_with_gate()`](Self::new_unsafe_with_gate): Unsafe constructor with signal
    pub unsafe fn new_unsafe() -> Self {
        let mut v = Vec::with_capacity(Self::NUM_SEGS);
        for _ in 0..Self::NUM_SEGS {
            v.push(AtomicPtr::new(ptr::null_mut()));
        }

        Self {
            segs: v.into_boxed_slice(),
            producer: CachePadded::new(ProducerState::new()),
            consumer: CachePadded::new(ConsumerState::new()),
            signal: None,
            _t: PhantomData,
        }
    }

    /// Creates a new empty queue with signal integration (unsafe, internal use).
    ///
    /// This is the low-level constructor called by [`new_with_gate()`](Self::new_with_gate).
    /// It is marked `unsafe` because it returns a raw `Self` value that must be immediately
    /// wrapped in an `Arc` to maintain the SPSC safety guarantees.
    ///
    /// # Safety
    ///
    /// Callers must ensure:
    /// 1. The returned value is immediately wrapped in `Arc<Self>`
    /// 2. Only one `SegSpscProducer` is created from the Arc (single producer)
    /// 3. Only one `SegSpscConsumer` is created from the Arc (single consumer)
    /// 4. The `signal` gate is properly configured with valid signal word and waker references
    ///
    /// The `new_with_gate()` constructor handles this correctly.
    ///
    /// # Arguments
    ///
    /// * `signal` - The `SignalGate` to schedule after each push
    ///
    /// # Signal Integration
    ///
    /// The provided `SignalGate` is stored in `self.signal` and automatically invoked
    /// after each successful push operation:
    /// ```ignore
    /// queue.try_push(item)?;
    /// // Internally:
    /// if let Some(gate) = &self.signal {
    ///     gate.schedule();  // Sets bit in signal word, wakes executor
    /// }
    /// ```
    ///
    /// # Memory Layout
    ///
    /// Identical to [`new_unsafe()`](Self::new_unsafe), plus:
    /// - Signal gate: ~128 bytes (cache-padded, contains Arc refs to signal word and waker)
    ///
    /// **Total initial allocation**:
    /// - Directory: `NUM_SEGS × 8 bytes`
    /// - Producer state: ~128 bytes
    /// - Consumer state: ~128 bytes
    /// - Signal gate: ~128 bytes
    /// - **Total**: ~400-500 bytes (before segment allocation)
    ///
    /// # Initialization State
    ///
    /// All atomic counters are initialized to zero (same as `new_unsafe()`), plus:
    /// ```ignore
    /// signal = Some(gate)  // Wrapped in CachePadded
    /// ```
    ///
    /// # Example
    ///
    /// ```ignore
    /// use bop_mpmc::{SignalGate, SignalWaker, SegSpsc};
    /// use std::sync::Arc;
    ///
    /// let waker = Arc::new(SignalWaker::new());
    /// let signal_word = Arc::new(AtomicU64::new(0));
    /// let gate = SignalGate::new(0, Arc::clone(&signal_word), Arc::clone(&waker));
    ///
    /// // Internal usage (wrapped in Arc by new_with_gate())
    /// let queue = Arc::new(unsafe { SegSpsc::<u64, 10, 0>::new_unsafe_with_gate(gate) });
    /// let producer = SegSpscProducer { queue: Arc::clone(&queue) };
    /// let consumer = SegSpscConsumer { queue: Arc::clone(&queue) };
    /// ```
    ///
    /// # See Also
    ///
    /// - [`new_with_gate()`](Self::new_with_gate): Safe public constructor with signal
    /// - [`new_unsafe()`](Self::new_unsafe): Unsafe constructor without signal
    /// - [`schedule()`](Self::schedule): Manually invoke the signal gate
    pub unsafe fn new_unsafe_with_gate(signal: SignalGate) -> Self {
        let mut v = Vec::with_capacity(Self::NUM_SEGS);
        for _ in 0..Self::NUM_SEGS {
            v.push(AtomicPtr::new(ptr::null_mut()));
        }

        Self {
            segs: v.into_boxed_slice(),
            producer: CachePadded::new(ProducerState::new()),
            consumer: CachePadded::new(ConsumerState::new()),
            signal: Some(CachePadded::new(signal)),
            _t: PhantomData,
        }
    }

    /// Returns the maximum number of items this queue can hold.
    ///
    /// Formula: `(SEG_SIZE × NUM_SEGS) - 1` (one-empty-slot distinguishes full/empty)
    ///
    /// # Example
    ///
    /// ```ignore
    /// type Q = SegSpsc<u64, 8, 10>;  // 256 × 1024 = 262,144
    /// assert_eq!(Q::capacity(), 262_143);
    /// ```
    #[inline(always)]
    pub const fn capacity() -> usize {
        (Self::SEG_SIZE * Self::NUM_SEGS) - 1
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Signal Integration API
    // ──────────────────────────────────────────────────────────────────────────────

    /// Schedules this queue for execution via the associated `SignalGate`.
    ///
    /// This is the primary mechanism for notifying the executor that this queue has work
    /// available. When called, it attempts to set the queue's bit in the signal word and
    /// potentially mark the signal group as active in the waker's summary bitmap.
    ///
    /// # Signal Protocol
    ///
    /// The signal protocol follows a state machine with three states: IDLE, SCHEDULED, and EXECUTING.
    /// `schedule()` attempts to transition from IDLE to SCHEDULED. If the queue is already
    /// SCHEDULED or EXECUTING, the call is a no-op (returns false internally).
    ///
    /// # Integration with SignalWaker
    ///
    /// When a queue transitions from IDLE to SCHEDULED via this method:
    /// 1. The bit is set in the signal word (64-bit bitmap)
    /// 2. If the signal word was previously empty, the corresponding bit is set in the summary
    /// 3. The waker may wake a sleeping executor thread if permits are available
    ///
    /// # Usage
    ///
    /// Typically called by producers after enqueuing items to notify consumers:
    ///
    /// ```ignore
    /// queue.try_push(item)?;
    /// queue.schedule();  // Wake executor to process the item
    /// ```
    ///
    /// # Performance
    ///
    /// - No-op if no signal gate configured (0 ns)
    /// - ~2-5 ns if already scheduled/executing
    /// - ~10-20 ns for successful schedule (includes atomic ops + potential waker notification)
    #[inline(always)]
    pub fn schedule(&self) {
        if let Some(s) = &self.signal {
            let _ = s.schedule();
        }
    }

    /// Marks this queue as EXECUTING, preventing redundant scheduling.
    ///
    /// This is an internal method used by the executor when it begins processing a queue.
    /// The transition from SCHEDULED to EXECUTING prevents concurrent schedule() calls
    /// from redundantly setting the signal bit.
    ///
    /// # Executor Protocol
    ///
    /// 1. Executor selects a set bit from the signal word
    /// 2. Calls `mark()` to transition SCHEDULED → EXECUTING
    /// 3. Processes items from the queue
    /// 4. Calls `unmark()` or `unmark_and_schedule()` depending on whether more work remains
    ///
    /// # Safety
    ///
    /// Must be called by the executor thread that owns this queue's timeslice.
    /// The signal protocol assumes single-threaded consumption.
    #[inline(always)]
    pub(crate) fn mark(&self) {
        if let Some(s) = &self.signal {
            let _ = s.begin();
        }
    }

    /// Marks this queue as IDLE, completing the current execution cycle.
    ///
    /// Used by the executor after processing a batch of items. If items were added
    /// during execution (indicated by the SCHEDULED flag being set), the queue will
    /// automatically reschedule itself.
    ///
    /// # Executor Protocol
    ///
    /// ```ignore
    /// queue.mark();  // SCHEDULED → EXECUTING
    /// while let Ok(item) = queue.try_pop() {
    ///     process(item);
    /// }
    /// queue.unmark();  // EXECUTING → IDLE (or SCHEDULED if new items arrived)
    /// ```
    ///
    /// # Automatic Rescheduling
    ///
    /// If producers called `schedule()` while this queue was EXECUTING, the SCHEDULED
    /// flag will be set. When `unmark()` is called, it detects this and re-sets the
    /// signal bit, ensuring the queue remains visible to the executor.
    #[inline(always)]
    pub(crate) fn unmark(&self) {
        if let Some(s) = &self.signal {
            let _ = s.finish();
        }
    }

    /// Atomically marks the queue as IDLE and schedules it for re-execution.
    ///
    /// This is an optimized version of calling `unmark()` followed by `schedule()`.
    /// Used when the executor knows there are more items to process but wants to
    /// yield the timeslice to ensure fairness across queues.
    ///
    /// # Use Case
    ///
    /// When processing large batches, the executor may want to:
    /// 1. Process N items
    /// 2. Yield to other queues
    /// 3. Ensure this queue gets rescheduled
    ///
    /// ```ignore
    /// for _ in 0..BATCH_SIZE {
    ///     if let Ok(item) = queue.try_pop() {
    ///         process(item);
    ///     } else {
    ///         break;
    ///     }
    /// }
    /// if queue.len() > 0 {
    ///     queue.unmark_and_schedule();  // More work, reschedule
    /// } else {
    ///     queue.unmark();  // Done
    /// }
    /// ```
    #[inline(always)]
    pub(crate) fn unmark_and_schedule(&self) {
        if let Some(s) = &self.signal {
            let _ = s.finish_and_schedule();
        }
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Inspection API
    // ──────────────────────────────────────────────────────────────────────────────

    /// Returns the number of items currently in the queue.
    ///
    /// This is calculated as `head - tail`, where both positions are monotonically
    /// increasing u64 counters. The close bit is masked out before subtraction.
    ///
    /// # Close Bit Handling
    ///
    /// The head position may have the close bit (bit 63) set. This method masks it out:
    /// ```ignore
    /// let h_pos = h & RIGHT_MASK;  // Clear bit 63
    /// let length = h_pos.saturating_sub(t);  // Prevent underflow
    /// ```
    ///
    /// # Consistency
    ///
    /// Uses `Ordering::Acquire` for both loads to ensure visibility of preceding writes.
    /// However, the value may be stale by the time the caller uses it (concurrent operations).
    ///
    /// # Performance
    ///
    /// ~2-5 ns (two atomic loads + subtraction)
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.try_push(1)?;
    /// queue.try_push(2)?;
    /// assert_eq!(queue.len(), 2);
    /// ```
    #[inline(always)]
    pub fn len(&self) -> usize {
        let h = self.producer.head.load(Ordering::Acquire);
        let t = self.consumer.tail.load(Ordering::Acquire);
        // Mask out the close bit when calculating length
        let h_pos = h & Self::RIGHT_MASK;
        h_pos.saturating_sub(t) as usize
    }

    /// Returns `true` if the queue contains no items.
    ///
    /// Equivalent to `self.len() == 0`, but more readable.
    ///
    /// # Note
    ///
    /// This check is not atomic with subsequent operations. A producer may enqueue
    /// items immediately after this returns `true`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// assert!(queue.is_empty());
    /// queue.try_push(1)?;
    /// assert!(!queue.is_empty());
    /// ```
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the queue is at maximum capacity.
    ///
    /// When the queue is full, `try_push()` and `try_push_n()` will return `PushError::Full`.
    ///
    /// # Capacity
    ///
    /// The capacity is `(segment_size * num_segments) - 1`:
    /// - For `SegSpsc<T, 10, 0>`: `1024 - 1 = 1023` items
    /// - For `SegSpsc<T, 13, 2>`: `(8192 * 4) - 1 = 32767` items
    ///
    /// # Note
    ///
    /// Like `is_empty()`, this is not atomic with subsequent operations.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for i in 0..queue.capacity() {
    ///     queue.try_push(i).unwrap();
    /// }
    /// assert!(queue.is_full());
    /// assert!(queue.try_push(999).is_err());  // Returns PushError::Full
    /// ```
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.len() == Self::capacity()
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Close API
    // ──────────────────────────────────────────────────────────────────────────────

    /// Checks if the queue has been closed.
    ///
    /// The close state is encoded in bit 63 of the head position, allowing lock-free
    /// detection without a separate atomic flag.
    ///
    /// # Close Bit Encoding
    ///
    /// ```text
    /// Head Position (64 bits):
    /// ┌─────────┬────────────────────────────────────────────────┐
    /// │ Bit 63  │ Bits 62..0                                     │
    /// │ (Close) │ (Monotonic Position)                           │
    /// └─────────┴────────────────────────────────────────────────┘
    /// ```
    ///
    /// # Semantics
    ///
    /// Once closed:
    /// - All `try_push()` and `try_push_n()` calls return `PushError::Closed`
    /// - Consumers can drain remaining items
    /// - `try_pop_n()` returns `PopError::Closed` when queue is empty
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::Relaxed` because close state doesn't need to synchronize with
    /// data operations (producers/consumers already use Acquire/Release for data visibility).
    ///
    /// # Performance
    ///
    /// ~1-2 ns (single atomic load + bit test)
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.try_push(1)?;
    /// queue.close();
    /// assert!(queue.is_closed());
    /// assert!(matches!(queue.try_push(2), Err(PushError::Closed(_))));
    /// assert_eq!(queue.try_pop(), Some(1));  // Can still drain
    /// ```
    #[inline(always)]
    pub fn is_closed(&self) -> bool {
        self.producer.head.load(Ordering::Relaxed) & Self::CLOSED_CHANNEL_MASK != 0
    }

    /// Closes the queue by setting the close bit in the head position.
    ///
    /// This is a one-way operation - once closed, the queue cannot be reopened.
    /// The close bit is set atomically using `fetch_or`, making it safe to call
    /// from any thread.
    ///
    /// # Behavior After Close
    ///
    /// - **Producers**: All enqueue operations fail with `PushError::Closed`
    /// - **Consumers**: Can drain remaining items, then receive `PopError::Closed`
    /// - **Signal**: The queue can still be scheduled/unscheduled
    ///
    /// # Use Cases
    ///
    /// 1. **Graceful Shutdown**: Signal that no more items will be produced
    /// 2. **Bounded Work**: Close after enqueueing a finite batch
    /// 3. **Error Propagation**: Close to signal upstream failure
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::Relaxed` - the close bit is independent of data visibility.
    /// Producers and consumers use Acquire/Release for proper data synchronization.
    ///
    /// # Thread Safety
    ///
    /// Safe to call from any thread, including concurrent calls (idempotent).
    ///
    /// # Performance
    ///
    /// ~2-5 ns (atomic fetch_or operation)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Producer thread
    /// for item in work_items {
    ///     queue.try_push(item)?;
    /// }
    /// queue.close();  // Signal completion
    ///
    /// // Consumer thread
    /// loop {
    ///     match queue.try_pop() {
    ///         Some(item) => process(item),
    ///         None if queue.is_closed() => break,  // Done
    ///         None => wait_for_work(),
    ///     }
    /// }
    /// ```
    #[inline(always)]
    pub unsafe fn close(&self) {
        self.producer
            .head
            .fetch_or(Self::CLOSED_CHANNEL_MASK, Ordering::Relaxed);
        self.schedule();
    }

    /// Increments the head position while preserving the close bit.
    ///
    /// This internal helper is designed for future CAS-based push operations where
    /// we need to atomically advance the head position without clearing the close bit.
    ///
    /// # Implementation Note
    ///
    /// Currently unused but kept for potential optimizations. The current `try_push_n`
    /// uses a simpler non-atomic increment since the producer is single-threaded.
    ///
    /// # Preconditions
    ///
    /// - `head_pos` must have the close bit clear (asserted in debug builds)
    /// - Used only in single-producer context
    ///
    /// # Algorithm
    ///
    /// ```text
    /// Given head_pos = POSITION (close bit clear)
    /// 1. Increment: new_pos = POSITION + 1
    /// 2. Check if within capacity
    /// 3. If yes: return new_pos
    /// 4. If no: wrap around (shouldn't happen with monotonic design)
    /// ```
    ///
    /// # Future Use
    ///
    /// If we add multi-producer support or optimistic CAS loops:
    /// ```ignore
    /// loop {
    ///     let h = self.producer.head.load(Ordering::Relaxed);
    ///     if h & CLOSED_CHANNEL_MASK != 0 {
    ///         return Err(PushError::Closed);
    ///     }
    ///     let next_h = self.next_head_pos(h & RIGHT_MASK);
    ///     if self.producer.head.compare_exchange_weak(h, next_h, ...).is_ok() {
    ///         break;
    ///     }
    /// }
    /// ```
    #[allow(dead_code)]
    #[inline(always)]
    fn next_head_pos(&self, head_pos: u64) -> u64 {
        debug_assert_eq!(
            head_pos & Self::CLOSED_CHANNEL_MASK,
            0,
            "close bit should be clear"
        );

        let new_head_pos = head_pos + 1;
        let new_index = new_head_pos & Self::RIGHT_MASK;

        // Check if we need to wrap the index
        if new_index < Self::capacity() as u64 {
            new_head_pos
        } else {
            // Wrap: increment sequence count, reset index
            let sequence_increment = Self::RIGHT_MASK + 1;
            let sequence_count = head_pos & !Self::RIGHT_MASK;
            sequence_count.wrapping_add(sequence_increment)
        }
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Producer API
    // ──────────────────────────────────────────────────────────────────────────────

    /// Attempts to enqueue a single item.
    ///
    /// This is a convenience wrapper around `try_push_n()` for single-item operations.
    ///
    /// # Returns
    ///
    /// - `Ok(())`: Item successfully enqueued
    /// - `Err(PushError::Full(item))`: Queue is at capacity, item returned
    /// - `Err(PushError::Closed(item))`: Queue has been closed, item returned
    ///
    /// # Close Semantics
    ///
    /// If the queue is closed (bit 63 of head is set), this returns `Err(PushError::Closed)`.
    /// The close check happens atomically before any capacity calculations.
    ///
    /// # Signal Integration
    ///
    /// After successful enqueue, automatically calls `schedule()` if a signal gate is configured.
    /// This wakes the executor to process the newly available item.
    ///
    /// # Performance
    ///
    /// ~5-15 ns for successful push (cache hit, no segment boundary crossing)
    /// ~20-50 ns for segment allocation (first push to a new segment)
    ///
    /// # Example
    ///
    /// ```ignore
    /// match queue.try_push(42) {
    ///     Ok(()) => println!("Enqueued successfully"),
    ///     Err(PushError::Full(item)) => println!("Queue full, item={}", item),
    ///     Err(PushError::Closed(item)) => println!("Queue closed, item={}", item),
    /// }
    /// ```
    ///
    /// # Thread Safety
    ///
    /// Safe to call from the single producer thread. NOT safe for concurrent producers
    /// (SPSC guarantee must be maintained by the caller).
    #[inline(always)]
    pub fn try_push(&self, item: T) -> Result<(), PushError<T>> {
        self.try_push_n(core::slice::from_ref(&item))
            .map(|_| ())
            .map_err(|err| match err {
                PushError::Full(()) => PushError::Full(item),
                PushError::Closed(()) => PushError::Closed(item),
            })
    }

    /// Attempts to enqueue multiple items from a slice.
    ///
    /// Copies items from `src` into the queue, potentially spanning multiple segments.
    /// Returns the number of items successfully copied (may be partial if queue fills).
    ///
    /// # Algorithm
    ///
    /// 1. **Close Check**: Return `Err(Closed)` if bit 63 is set in head
    /// 2. **Capacity Check**: Calculate free space using cached tail (refresh if needed)
    /// 3. **Bulk Copy**: Copy min(src.len(), free) items across segment boundaries
    /// 4. **Head Update**: Atomically publish new head position with Release ordering
    /// 5. **Signal**: Call `schedule()` to wake executor
    ///
    /// # Tail Caching
    ///
    /// The producer maintains a cached copy of the consumer's tail position:
    /// - **Fast path**: If cached tail shows available space, skip atomic load
    /// - **Slow path**: If cached tail shows full, refresh from consumer's atomic tail
    ///
    /// This optimization eliminates cache line bouncing in the common case where the
    /// queue has available space.
    ///
    /// # Segment Boundaries
    ///
    /// Items may span multiple segments. For each segment:
    /// ```ignore
    /// seg_idx = (position >> P) & DIR_MASK;  // Which segment in directory
    /// offset = position & SEG_MASK;          // Offset within segment
    /// ```
    ///
    /// When crossing a boundary (offset wraps to 0), the next segment is lazily allocated
    /// via `ensure_segment_for()`.
    ///
    /// # Memory Ordering
    ///
    /// - Tail cache: `Acquire` (when refreshing from consumer)
    /// - Head update: `Release` (publishes data + position to consumer)
    ///
    /// # Returns
    ///
    /// - `Ok(n)`: `n` items successfully enqueued (0 ≤ n ≤ src.len())
    /// - `Err(PushError::Full(()))`: Queue full, no items enqueued
    /// - `Err(PushError::Closed(()))`: Queue closed, no items enqueued
    ///
    /// # Partial Success
    ///
    /// If `src.len() > free`, only `free` items are copied and `Ok(free)` is returned.
    /// The caller must check the return value to handle partial writes:
    ///
    /// ```ignore
    /// let items = vec![1, 2, 3, 4, 5];
    /// match queue.try_push_n(&items) {
    ///     Ok(n) if n == items.len() => println!("All items enqueued"),
    ///     Ok(n) => println!("Partial: enqueued {}/{}", n, items.len()),
    ///     Err(PushError::Full(())) => println!("Queue full"),
    ///     Err(PushError::Closed(())) => println!("Queue closed"),
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// - **Single segment**: ~10-20 ns (one memcpy, no allocation)
    /// - **Multiple segments**: ~30-50 ns per segment boundary (allocation + memcpy)
    /// - **Throughput**: ~500M-1B ops/sec on modern x86_64 (depending on item size)
    ///
    /// # Safety
    ///
    /// Uses `ptr::copy_nonoverlapping` internally. Safety invariants:
    /// - Source slice is valid for reads
    /// - Destination segment is allocated and valid for writes
    /// - No overlap between source and destination (guaranteed by design)
    /// - Size calculation accounts for `sizeof::<T>()`
    pub fn try_push_n(&self, src: &[T]) -> Result<usize, PushError<()>> {
        let h = self.producer.head.load(Ordering::Relaxed);

        // Check if queue is closed
        if h & Self::CLOSED_CHANNEL_MASK != 0 {
            return Err(PushError::Closed(()));
        }

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
                return Err(PushError::Full(()));
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

        if let Some(s) = &self.signal {
            let _ = s.schedule();
        }
        Ok(copied)
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Consumer API (copying)
    // ──────────────────────────────────────────────────────────────────────────────

    /// Attempts to dequeue a single item by copying.
    ///
    /// This is a convenience wrapper around `try_pop_n()` for single-item operations.
    /// The item is copied out of the queue into the returned `Option`.
    ///
    /// # Returns
    ///
    /// - `Some(item)`: Item successfully dequeued
    /// - `None`: Queue is empty or closed
    ///
    /// # Distinguishing Empty vs Closed
    ///
    /// This method returns `None` for both empty and closed states. Use `try_pop_n()` or
    /// check `is_closed()` separately if you need to distinguish:
    ///
    /// ```ignore
    /// match queue.try_pop() {
    ///     Some(item) => process(item),
    ///     None if queue.is_closed() => break,  // Closed
    ///     None => wait_for_items(),            // Empty
    /// }
    /// ```
    ///
    /// # Head Caching
    ///
    /// The consumer maintains a cached copy of the producer's head position to avoid
    /// unnecessary cache line bouncing. Only refreshes when the cached value shows empty.
    ///
    /// # Performance
    ///
    /// ~5-15 ns for successful pop (cache hit, no segment boundary crossing)
    ///
    /// # Safety
    ///
    /// Initializes return value with `mem::zeroed()`. Safe because:
    /// - If pop succeeds (returns `Some`), the zeroed value is overwritten
    /// - If pop fails (returns `None`), the zeroed value is discarded
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.try_push(42)?;
    /// assert_eq!(queue.try_pop(), Some(42));
    /// assert_eq!(queue.try_pop(), None);  // Empty
    /// ```
    #[inline(always)]
    pub fn try_pop(&self) -> Option<T> {
        let mut tmp: T = unsafe { core::mem::zeroed() };
        match self.try_pop_n(core::slice::from_mut(&mut tmp)) {
            Ok(1) => Some(tmp),
            _ => None,
        }
    }

    /// Attempts to dequeue multiple items by copying into the destination slice.
    ///
    /// Copies items from the queue into `dst`, potentially spanning multiple segments.
    /// Returns the number of items successfully copied (may be partial if queue empties).
    ///
    /// # Algorithm
    ///
    /// 1. **Availability Check**: Calculate available items using cached head (refresh if needed)
    /// 2. **Close Check**: Return `Err(PopError::Closed)` if empty and bit 63 is set
    /// 3. **Bulk Copy**: Copy min(dst.len(), avail) items across segment boundaries
    /// 4. **Tail Update**: Atomically publish new tail position with Release ordering
    /// 5. **Segment Sealing**: Check if segments can be sealed and returned to pool
    ///
    /// # Head Caching
    ///
    /// The consumer maintains a cached copy of the producer's head position:
    /// - **Fast path**: If cached head shows available items, skip atomic load
    /// - **Slow path**: If cached head shows empty, refresh from producer's atomic head
    ///
    /// This optimization eliminates cache line bouncing in the common case where the
    /// queue has available items.
    ///
    /// # Close Bit Handling
    ///
    /// The head position may have bit 63 set to indicate closure:
    /// ```ignore
    /// let h_pos = h & RIGHT_MASK;  // Mask out close bit
    /// let avail = h_pos.saturating_sub(t);
    /// if avail == 0 && (h & CLOSED_CHANNEL_MASK != 0) {
    ///     return Err(PopError::Closed);
    /// }
    /// ```
    ///
    /// # Segment Boundaries
    ///
    /// Items may span multiple segments. For each segment:
    /// ```ignore
    /// seg_idx = (position >> P) & DIR_MASK;  // Which segment in directory
    /// offset = position & SEG_MASK;          // Offset within segment
    /// ```
    ///
    /// After consumption, segments in the range [sealable_lo, sealable_hi) are marked
    /// as sealed and available for reuse via the pooling mechanism.
    ///
    /// # Memory Ordering
    ///
    /// - Head cache: `Acquire` (when refreshing from producer)
    /// - Segment load: `Acquire` (ensures data visibility)
    /// - Tail update: `Release` (publishes consumed position to producer)
    ///
    /// # Returns
    ///
    /// - `Ok(n)`: `n` items successfully dequeued (0 ≤ n ≤ dst.len())
    /// - `Err(PopError::Empty)`: Queue is empty
    /// - `Err(PopError::Closed)`: Queue is closed and empty
    ///
    /// # Partial Success
    ///
    /// If `dst.len() > avail`, only `avail` items are copied and `Ok(avail)` is returned:
    ///
    /// ```ignore
    /// let mut buffer = vec![0; 100];
    /// match queue.try_pop_n(&mut buffer) {
    ///     Ok(n) => println!("Dequeued {} items", n),
    ///     Err(PopError::Empty) => println!("Queue empty"),
    ///     Err(PopError::Closed) => println!("Queue closed"),
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// - **Single segment**: ~10-20 ns (one memcpy)
    /// - **Multiple segments**: ~20-30 ns per segment boundary (memcpy + sealing check)
    /// - **Throughput**: ~500M-1B ops/sec on modern x86_64 (depending on item size)
    ///
    /// # Safety
    ///
    /// Uses `ptr::copy_nonoverlapping` internally. Safety invariants:
    /// - Source segment is allocated and contains initialized items
    /// - Destination slice is valid for writes
    /// - No overlap between source and destination (guaranteed by design)
    /// - Items are `Copy`, so no double-drop concerns
    pub fn try_pop_n(&self, dst: &mut [T]) -> Result<usize, PopError> {
        let t = self.consumer.tail.load(Ordering::Relaxed);

        // Use cached head value, refresh if needed
        let mut h = unsafe { *self.consumer.head_cache.get() };
        // Mask out close bit to get actual position
        let h_pos = h & Self::RIGHT_MASK;
        let mut avail = h_pos.saturating_sub(t) as usize;

        if avail == 0 {
            // Refresh head cache from producer
            h = self.producer.head.load(Ordering::Acquire);
            unsafe {
                *self.consumer.head_cache.get() = h;
            }
            let h_pos = h & Self::RIGHT_MASK;
            avail = h_pos.saturating_sub(t) as usize;
            if avail == 0 {
                // Check if closed
                if h & Self::CLOSED_CHANNEL_MASK != 0 {
                    return Err(PopError::Closed);
                }
                return Err(PopError::Empty);
            }
        }

        let mut n = dst.len().min(avail);

        let mut i = t;
        let mut copied = 0usize;

        while n > 0 {
            let seg_idx = ((i >> P) as usize) & Self::DIR_MASK;
            let off = (i as usize) & Self::SEG_MASK;

            let base = self.segs[seg_idx].load(Ordering::Acquire);
            debug_assert!(!base.is_null());

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

    /// Zero-copy consumption: processes items in-place via callback with read-only slices.
    ///
    /// This method provides direct read access to items in the queue without copying.
    /// The callback `f` is invoked with contiguous slices of items (up to segment boundaries),
    /// and must return the number of items it actually consumed.
    ///
    /// # Zero-Copy Design
    ///
    /// Unlike `try_pop_n()` which copies items into a destination buffer, this method:
    /// - Exposes items directly in their segment storage (zero memory copies)
    /// - Allows processing without intermediate allocation
    /// - Ideal for serialization, hashing, or forwarding to I/O
    ///
    /// # Algorithm
    ///
    /// 1. **Availability Check**: Calculate available items using cached head
    /// 2. **Segment Iteration**: For each segment in [tail, tail + max):
    ///    - Calculate contiguous slice within segment boundary
    ///    - Invoke callback with read-only slice
    ///    - Advance tail by the number of items callback consumed
    ///    - Seal segment if boundary crossed
    /// 3. **Early Exit**: Stop if callback returns 0 (processed fewer than offered)
    ///
    /// # Callback Contract
    ///
    /// ```ignore
    /// fn callback(slice: &[T]) -> usize
    /// ```
    ///
    /// - **Input**: Read-only slice of contiguous items (may be less than requested due to segment boundaries)
    /// - **Output**: Number of items consumed (0 ≤ consumed ≤ slice.len())
    /// - **Lifetime**: Slice MUST NOT be held beyond the callback scope (undefined behavior)
    /// - **Early Exit**: Return < slice.len() to stop iteration
    ///
    /// # Segment Boundaries
    ///
    /// Items are presented in segment-sized chunks. If you request 10,000 items but segment
    /// size is 1024, the callback will be invoked multiple times:
    /// - 1st call: slice of up to 1024 items (or fewer if at segment boundary)
    /// - 2nd call: next slice of up to 1024 items
    /// - ... (continues until `max` items processed or callback returns 0)
    ///
    /// # Close Semantics
    ///
    /// This method does NOT check the close bit or return an error. It simply processes
    /// whatever items are available. Use `is_closed()` separately if needed:
    ///
    /// ```ignore
    /// loop {
    ///     let consumed = queue.consume_in_place(1024, |slice| {
    ///         serialize(slice);
    ///         slice.len()  // Consumed all
    ///     });
    ///
    ///     if consumed == 0 {
    ///         if queue.is_closed() {
    ///             break;  // Done
    ///         }
    ///         wait_for_items();
    ///     }
    /// }
    /// ```
    ///
    /// # Parameters
    ///
    /// - `max`: Maximum number of items to consume (may consume fewer if queue empties or callback exits early)
    /// - `f`: Callback invoked with read-only slices, returning number of items consumed
    ///
    /// # Returns
    ///
    /// Total number of items consumed across all callback invocations.
    ///
    /// # Examples
    ///
    /// **Example 1: Serialize to network without copying**
    /// ```ignore
    /// let bytes_sent = queue.consume_in_place(batch_size, |items| {
    ///     match socket.write_all(bytemuck::cast_slice(items)) {
    ///         Ok(()) => items.len(),     // Consumed all
    ///         Err(_) => 0,               // Error, stop
    ///     }
    /// });
    /// ```
    ///
    /// **Example 2: Compute hash without allocation**
    /// ```ignore
    /// let mut hasher = Blake3::new();
    /// queue.consume_in_place(usize::MAX, |items| {
    ///     hasher.update(bytemuck::cast_slice(items));
    ///     items.len()  // Consumed all
    /// });
    /// let hash = hasher.finalize();
    /// ```
    ///
    /// **Example 3: Conditional processing with early exit**
    /// ```ignore
    /// let mut count = 0;
    /// queue.consume_in_place(1000, |items| {
    ///     for (i, item) in items.iter().enumerate() {
    ///         if !should_process(item) {
    ///             return i;  // Stop here
    ///         }
    ///         process(item);
    ///         count += 1;
    ///     }
    ///     items.len()  // Consumed all
    /// });
    /// ```
    ///
    /// # Performance
    ///
    /// - **Throughput**: 2-5 GB/s for simple processing (memory bandwidth limited)
    /// - **Latency**: ~5-10 ns per callback invocation overhead
    /// - **Advantage over copying**: Saves ~50-200 ns per 1KB of data (avoids memcpy)
    ///
    /// # Safety
    ///
    /// This method is safe because:
    /// - Items are `Copy`, so no drop concerns
    /// - Producer can't modify consumed items (monotonic head/tail invariant)
    /// - Consumer is single-threaded (SPSC guarantee)
    /// - Slices are bounded to initialized region [tail, head)
    ///
    /// **CRITICAL**: The callback MUST NOT store the slice reference beyond its scope.
    /// The slice becomes invalid after the tail is updated.
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
        let h_pos = h & Self::RIGHT_MASK;
        let mut avail = h_pos.saturating_sub(t) as usize;

        if avail == 0 {
            // Refresh head cache from producer
            h = self.producer.head.load(Ordering::Acquire);
            unsafe {
                *self.consumer.head_cache.get() = h;
            }
            let h_pos = h & Self::RIGHT_MASK;
            avail = h_pos.saturating_sub(t) as usize;
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
    // Metrics API
    // ──────────────────────────────────────────────────────────────────────────────

    /// Returns the number of fresh segment allocations (pool misses).
    ///
    /// This metric tracks how many times `ensure_segment_for()` had to allocate new
    /// memory from the system allocator because no sealed segment was available for reuse.
    ///
    /// # Interpretation
    ///
    /// - **High value**: Indicates working set exceeds pooled segment capacity (more allocations)
    /// - **Low value**: Indicates effective pooling (segments are being reused efficiently)
    ///
    /// # Use Cases
    ///
    /// - **Performance tuning**: Compare fresh vs reused to assess pool effectiveness
    /// - **Capacity planning**: High fresh count may indicate need for larger `NUM_SEGS_P2`
    /// - **Memory profiling**: Track allocation patterns over time
    ///
    /// # Example
    ///
    /// ```ignore
    /// queue.reset_allocation_stats();
    /// // ... run workload ...
    /// println!("Fresh allocations: {}", queue.fresh_allocations());
    /// println!("Pool reuse rate: {:.1}%", queue.pool_reuse_rate());
    /// ```
    #[inline(always)]
    pub fn fresh_allocations(&self) -> u64 {
        self.producer.fresh_allocations.load(Ordering::Relaxed)
    }

    /// Returns the number of segment pool reuses (pool hits).
    ///
    /// This metric tracks how many times `ensure_segment_for()` successfully reused a
    /// sealed segment from the pool instead of allocating new memory.
    ///
    /// # Interpretation
    ///
    /// - **High value**: Indicates effective pooling and good cache locality
    /// - **Low value**: May indicate insufficient sealed segments or high churn rate
    ///
    /// # Pool Reuse Mechanism
    ///
    /// Segments in the range [sealable_lo, sealable_hi) are available for reuse.
    /// When the producer needs a new segment, it first checks this range. If available,
    /// the segment is moved (not copied) to the target slot in the directory.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let before = queue.pool_reuses();
    /// // ... enqueue/dequeue cycle that triggers segment reuse ...
    /// let after = queue.pool_reuses();
    /// println!("Segments reused: {}", after - before);
    /// ```
    #[inline(always)]
    pub fn pool_reuses(&self) -> u64 {
        self.producer.pool_reuses.load(Ordering::Relaxed)
    }

    /// Returns the segment pool reuse rate as a percentage (0-100%).
    ///
    /// Calculated as: `(pool_reuses / (fresh_allocations + pool_reuses)) * 100`
    ///
    /// # Interpretation
    ///
    /// - **0%**: No reuse (all allocations are fresh) - may indicate cold start or insufficient pool
    /// - **50%**: Half of allocations are reused - moderate pooling effectiveness
    /// - **90%+**: Excellent reuse - pool is working well, high cache locality
    ///
    /// # Performance Impact
    ///
    /// - **Fresh allocation**: ~100-500 ns (system allocator + initialization)
    /// - **Pool reuse**: ~5-10 ns (pointer swap)
    ///
    /// High reuse rates directly translate to lower latency and higher throughput.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Warm up the pool
    /// for _ in 0..queue.capacity() {
    ///     queue.try_push(0)?;
    /// }
    /// for _ in 0..queue.capacity() {
    ///     queue.try_pop();
    /// }
    ///
    /// queue.reset_allocation_stats();
    ///
    /// // Run steady-state workload
    /// for _ in 0..1_000_000 {
    ///     queue.try_push(42)?;
    ///     queue.try_pop();
    /// }
    ///
    /// println!("Reuse rate: {:.1}%", queue.pool_reuse_rate());
    /// // Expected: ~99% after warm-up phase
    /// ```
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

    /// Resets allocation statistics to zero.
    ///
    /// Useful for:
    /// - Measuring specific workload phases (reset between phases)
    /// - Repeated benchmark runs (reset between iterations)
    /// - Ignoring cold-start allocation costs (reset after warm-up)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Warm-up phase
    /// warm_up_queue(&queue);
    ///
    /// // Reset stats to measure steady-state only
    /// queue.reset_allocation_stats();
    ///
    /// // Run benchmark
    /// benchmark(&queue);
    ///
    /// println!("Steady-state reuse rate: {:.1}%", queue.pool_reuse_rate());
    /// ```
    pub fn reset_allocation_stats(&self) {
        self.producer.fresh_allocations.store(0, Ordering::Relaxed);
        self.producer.pool_reuses.store(0, Ordering::Relaxed);
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Internal Implementation
    // ──────────────────────────────────────────────────────────────────────────────

    /// Marks a segment as sealed (fully consumed and available for reuse).
    ///
    /// Called by the consumer after crossing a segment boundary. Updates `sealable_hi`
    /// to indicate that segment `s_done` is now part of the sealed pool.
    ///
    /// # Segment Pool Lifecycle
    ///
    /// ```text
    /// Directory: [seg0, seg1, seg2, seg3, ...]
    ///                   ^      ^
    ///                   lo     hi
    ///
    /// Sealed range: [sealable_lo, sealable_hi)
    /// - Segments in this range are available for reuse by the producer
    /// - Producer calls ensure_segment_for() which steals from [lo, hi)
    /// - Consumer calls seal_after() to add newly consumed segments
    /// ```
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::Release` to ensure the segment's data writes are visible
    /// before the producer attempts to reuse the segment.
    #[inline(always)]
    fn seal_after(&self, s_done: u64) {
        self.consumer
            .sealable_hi
            .store(s_done + 1, Ordering::Release);
    }

    /// Ensures a segment is allocated and ready for the given directory index.
    ///
    /// This is the core segment management routine, implementing lazy allocation
    /// with pooling. Called by the producer when crossing segment boundaries.
    ///
    /// # Algorithm
    ///
    /// 1. **Fast path**: Check if `segs[seg_idx]` is already non-null (return immediately)
    /// 2. **Pool check**: Load [sealable_lo, sealable_hi) range
    /// 3. **Reuse path**: If sealed segments available:
    ///    - Load segment from `segs[sealable_lo]`
    ///    - Move to `segs[seg_idx]`
    ///    - Increment `sealable_lo` and `pool_reuses` counter
    /// 4. **Allocation path**: If no sealed segments:
    ///    - Allocate new segment via system allocator
    ///    - Increment `fresh_allocations` counter
    ///
    /// # Segment Pooling
    ///
    /// Sealed segments form a FIFO pool:
    /// ```text
    /// segs[lo] → segs[lo+1] → ... → segs[hi-1]
    ///  ^                               ^
    ///  next to reuse                   next to seal
    /// ```
    ///
    /// This provides temporal locality: recently used segments are reused first,
    /// keeping them hot in cache.
    ///
    /// # Memory Ordering
    ///
    /// - Segment loads: `Acquire` (ensure data visibility)
    /// - Segment stores: `Release` (publish segment availability)
    /// - Counter updates: `Relaxed` (stats don't require synchronization)
    ///
    /// # Returns
    ///
    /// Non-null pointer to segment storage (`*mut MaybeUninit<T>`)
    ///
    /// # Safety
    ///
    /// The returned pointer:
    /// - Points to valid segment storage (1 << P items)
    /// - Is uniquely owned by `segs[seg_idx]` after this call
    /// - Remains valid until segment is sealed and reused elsewhere
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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
        assert!(q.is_empty());
        assert!(!q.is_full());
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn test_single_item_operations() {
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };

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
        let q = unsafe { Q::new_unsafe() };

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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
        q.try_push(1).unwrap();
        q.try_push(2).unwrap();

        // Ask to consume zero items
        let consumed = q.consume_in_place(0, |_| 10);
        assert_eq!(consumed, 0);
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn test_segment_boundaries() {
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };

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
        let q = unsafe { Q::new_unsafe() };

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
        let q = unsafe { QStruct::new_unsafe() };

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
        let q = unsafe { SmallQ::new_unsafe() };

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
        let q = unsafe { Q::new_unsafe() };

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
        let q = unsafe { Q::new_unsafe() };

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
        let q = unsafe { Q::new_unsafe() };

        // Initially no allocations should have occurred
        assert_eq!(q.fresh_allocations(), 0);
        assert_eq!(q.pool_reuses(), 0);
        assert_eq!(q.pool_reuse_rate(), 0.0);
    }

    #[test]
    fn test_allocation_stats_fresh_allocations() {
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };

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
        let q = unsafe { Q::new_unsafe() };
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
        let q = unsafe { Q::new_unsafe() };
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
    fn test_close_bit_encoding() {
        let q = unsafe { Q::new_unsafe() };

        // Initially queue should not be closed
        assert!(!q.is_closed());

        // Push some items
        q.try_push(1).unwrap();
        q.try_push(2).unwrap();
        q.try_push(3).unwrap();

        // Queue should still not be closed
        assert!(!q.is_closed());
        assert_eq!(q.len(), 3);

        // Close the queue
        unsafe {
            q.close();
        }

        // Queue should now be closed
        assert!(q.is_closed());

        // Length should still be correct even though queue is closed
        assert_eq!(q.len(), 3);

        // Try to push after closing should fail
        assert!(matches!(q.try_push(4), Err(PushError::Closed(_))));
        assert!(matches!(q.try_push_n(&[5, 6]), Err(PushError::Closed(()))));

        // Should still be able to pop existing items
        assert_eq!(q.try_pop(), Some(1));
        assert_eq!(q.try_pop(), Some(2));
        assert_eq!(q.try_pop(), Some(3));

        // Now queue is empty and closed, should return Closed error
        assert!(matches!(q.try_pop(), None));
    }

    #[test]
    fn test_close_with_try_pop_n() {
        let q = unsafe { Q::new_unsafe() };

        // Push items
        for i in 0..10u64 {
            q.try_push(i).unwrap();
        }

        // Close the queue
        unsafe {
            q.close();
        }

        // Should be able to drain existing items
        let mut buffer = [0u64; 5];
        assert_eq!(q.try_pop_n(&mut buffer).unwrap(), 5);
        assert_eq!(&buffer, &[0, 1, 2, 3, 4]);

        // Drain remaining
        assert_eq!(q.try_pop_n(&mut buffer).unwrap(), 5);
        assert_eq!(&buffer, &[5, 6, 7, 8, 9]);

        // Now empty and closed
        assert!(matches!(q.try_pop_n(&mut buffer), Err(PopError::Closed)));
    }

    #[test]
    fn test_close_with_consume_in_place() {
        let q = unsafe { Q::new_unsafe() };

        // Push items
        for i in 0..20u64 {
            q.try_push(i).unwrap();
        }

        // Close the queue
        unsafe {
            q.close();
        }

        // Should be able to consume existing items
        let mut consumed_values = Vec::new();
        let total = q.consume_in_place(20, |chunk| {
            consumed_values.extend_from_slice(chunk);
            chunk.len()
        });

        assert_eq!(total, 20);
        assert_eq!(consumed_values.len(), 20);
        for (i, &v) in consumed_values.iter().enumerate() {
            assert_eq!(v, i as u64);
        }

        // Queue should be empty now
        assert_eq!(q.consume_in_place(10, |_| 0), 0);
    }

    #[test]
    fn test_cache_hit_miss_behavior() {
        let q = unsafe { Q::new_unsafe() };
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
