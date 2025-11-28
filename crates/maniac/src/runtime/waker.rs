use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use crate::runtime::io_driver::IoDriver;
use crate::utils::CachePadded;

use crate::utils::bits::is_set;

pub const STATUS_SUMMARY_BITS: u32 = 62;
pub const STATUS_SUMMARY_MASK: u64 = (1u64 << STATUS_SUMMARY_BITS) - 1;
pub const STATUS_SUMMARY_WORDS: usize = STATUS_SUMMARY_BITS as usize;
pub const STATUS_BIT_PARTITION: u64 = 1u64 << 62;
pub const STATUS_BIT_YIELD: u64 = 1u64 << 63;

/// A cache-optimized waker that packs queue summaries and control flags into a single status word.
///
/// # Architecture
///
/// `WorkerWaker` implements a **two-level hierarchy** for efficient work discovery while
/// keeping a single atomic source of truth for coordination data:
///
/// ```text
/// Level 1: Status Word (64 bits)
///          ┌─────────────────────────────────────┐
///          │ Bits 0-61  │ Bit 62 │ Bit 63        │
///          │ Queue map  │ Part.  │ Yield         │
///          └─────────────────────────────────────┘
///                 │           │
///                 ▼           ▼
/// Level 2: Signal Words (external AtomicU64s)
///          Word 0  Word 1  ...  Word 61
///          [64 q]  [64 q]  ...  [64 q]           Each bit = individual queue state
///
/// Total: 62 words × 64 bits = 3,968 queues
/// ```
///
/// # Core Components
///
/// 1. **Status Bitmap** (`status`): Single u64 that stores queue-word summary bits
///    (0‒61) plus control flags (partition/yield). Enables O(1) lookup without a
///    second atomic.
///
/// 2. **Counting Semaphore** (`permits`): Tracks how many threads should be awake.
///    Each queue transition from empty→non-empty adds exactly 1 permit, guaranteeing
///    **no lost wakeups**.
///
/// 3. **Sleeper Tracking** (`sleepers`): Approximate count of parked threads.
///    Used to throttle notifications (avoid waking more threads than necessary).
///
/// # Design Patterns
///
/// ## Cache Optimization
/// - `CachePadded` on struct for cache-line alignment
/// - `CachePadded` on hot atomics to prevent false sharing
/// - Producer/consumer paths access different cache lines
///
/// ## Memory Ordering Strategy
/// - **Status summary bits**: `Relaxed` - hint-based, false positives acceptable
/// - **Permits**: `AcqRel/Release` - proper synchronization for wakeups
/// - **Sleepers**: `Relaxed` - approximate count is sufficient
///
/// ## Lazy Cleanup
/// Summary bits may remain set after queues empty (false positives).
/// Consumers lazily clear bits via `try_unmark_if_empty()`. This trades
/// occasional extra checks for lower overhead on the hot path.
///
/// # Guarantees
///
/// - **No lost wakeups**: Permits accumulate even if no threads are sleeping
/// - **Bounded notifications**: Never wakes more than `sleepers` threads
/// - **Lock-free fast path**: `try_acquire()` uses only atomics
/// - **Summary consistency**: false positives OK, false negatives impossible
///
/// # Trade-offs
///
/// - **False positives**: Summary may indicate work when queues are empty (lazy cleanup)
/// - **Approximate sleeper count**: May over-notify slightly (but safely)
/// - **64-word limit**: Summary is single u64 (extensible if needed)
///
/// # Usage Example
///
/// ```ignore
/// use std::sync::Arc;
/// use maniac::WorkerWaker;
///
/// let waker = Arc::new(WorkerWaker::new());
///
/// // Producer: mark queue 5 in word 0 as active
/// waker.mark_active(0);  // Adds 1 permit, wakes 1 sleeper
///
/// // Consumer: find work via summary
/// let summary = waker.snapshot_summary();
/// for word_idx in (0..64).filter(|i| summary & (1 << i) != 0) {
///     // Process queues in word_idx
/// }
///
/// // Consumer: block when no work
/// waker.acquire();  // Waits for a permit
/// ```
///
/// # Performance Characteristics
///
/// - **mark_active**: O(1) atomic, fast if already set
/// - **mark_active_mask**: O(1) batch update for multiple queues
/// - **try_acquire**: O(1) lock-free
/// - **acquire**: O(1) amortized (blocks on contention)
/// - **snapshot_summary**: O(1) single atomic load
///
/// # Thread Safety
///
/// All methods are thread-safe. Producers and consumers can operate concurrently
/// without coordination beyond the internal atomics and the event loop waker.
#[repr(align(64))]
pub struct WorkerWaker {
    /// **Status bitmap**: Queue-word summary bits (0‒61) plus control flags.
    ///
    /// - Bits 0‒61: Queue-word hot bits (`mark_active`, `try_unmark_if_empty`, etc.)
    /// - Bit 62 (`STATUS_BIT_PARTITION`): Partition cache says work is present
    /// - Bit 63 (`STATUS_BIT_YIELD`): Worker should yield ASAP
    ///
    /// Keeping everything in one atomic avoids races between independent u64s.
    status: CachePadded<AtomicU64>,

    /// **Counting semaphore**: Number of threads that should be awake (available permits).
    ///
    /// Incremented by producers when queues become active (0→1 transitions).
    /// Decremented by consumers via `try_acquire()` or `acquire()`.
    ///
    /// **Critical invariant**: Each queue empty→non-empty transition adds exactly
    /// 1 permit, preventing lost wakeups. Permits accumulate if no threads are
    /// sleeping, ensuring late arrivals find work.
    ///
    /// - Acquire: `AcqRel` (synchronizes with Release from producers)
    /// - Release: `Release` (makes queue data visible to acquirers)
    permits: CachePadded<AtomicU64>,

    /// **Approximate sleeper count**: Best-effort tracking of parked threads.
    ///
    /// Used to throttle `cv.notify_one()` calls in `release()`. We only wake
    /// up to `min(permits, sleepers)` threads to avoid unnecessary notifications.
    ///
    /// Uses `Relaxed` ordering since exactness isn't required:
    /// - Over-estimate: Extra notify_one() calls (threads recheck permits)
    /// - Under-estimate: Permits accumulate, future wakeups succeed
    sleepers: CachePadded<AtomicUsize>,

    /// **Active worker count**: Total number of FastTaskWorker threads currently running.
    ///
    /// Updated when workers start (register_worker) and stop (unregister_worker).
    /// Workers periodically check this to reconfigure their signal partitions for
    /// optimal load distribution with minimal contention.
    ///
    /// Uses `Relaxed` ordering since workers can tolerate slightly stale values
    /// and will eventually reconfigure on the next periodic check.
    worker_count: CachePadded<AtomicUsize>,

    /// **Partition summary**: Bitmap tracking which leafs in this worker's SummaryTree partition
    /// have active tasks (up to 64 leafs per partition).
    ///
    /// Bit `i` corresponds to leaf `partition_start + i` in the global SummaryTree.
    /// This allows each worker to maintain a cache-local view of its assigned partition,
    /// enabling O(1) work checking without scanning the full tree.
    ///
    /// Updated via `sync_partition_summary()` before parking. Uses `Relaxed` ordering
    /// since it's a hint (false positives are safe, workers verify actual task availability).
    ///
    /// When partition_summary transitions 0→non-zero, `STATUS_BIT_PARTITION`
    /// is set and a permit is added to wake the worker.
    partition_summary: CachePadded<AtomicU64>,
}

impl WorkerWaker {
    pub fn new() -> Self {
        debug_assert_eq!(
            (STATUS_BIT_PARTITION | STATUS_BIT_YIELD) & STATUS_SUMMARY_MASK,
            0,
            "status control bits must not overlap summary mask"
        );
        Self {
            status: CachePadded::new(AtomicU64::new(0)),
            permits: CachePadded::new(AtomicU64::new(0)),
            sleepers: CachePadded::new(AtomicUsize::new(0)),
            worker_count: CachePadded::new(AtomicUsize::new(0)),
            partition_summary: CachePadded::new(AtomicU64::new(0)),
        }
    }

    // ────────────────────────────────────────────────────────────────────────────
    // PRODUCER-SIDE API
    // ────────────────────────────────────────────────────────────────────────────

    /// Returns the full 64-bit raw status word for this worker,
    /// which contains all control and summary bits.
    ///
    /// # Details
    /// The status word encodes:
    /// - Status control bits (e.g., yield, partition-ready)
    /// - Partition summary bits (track active leafs in this partition)
    ///
    /// This is a low-level snapshot, useful for diagnostics, debugging,
    /// or fast checks on global/partition state.
    ///
    /// # Memory Ordering
    /// Uses relaxed ordering for performance, as consumers
    /// tolerate minor staleness and correctness is ensured elsewhere.
    #[inline]
    pub fn status(&self) -> u64 {
        self.status.load(Ordering::Relaxed)
    }

    /// Returns the current state of the primary control bits ("yield" and "partition").
    ///
    /// # Returns
    /// A tuple `(is_yield, is_partition_active)` representing:
    /// - `is_yield`: Whether the yield control bit is set, instructing the worker to yield.
    /// - `is_partition_active`: Whether the partition summary bit is set, indicating there is pending work detected in this worker's assigned partition.
    ///
    /// This allows higher-level logic to react based on whether the worker
    /// should yield or has instant work available.
    ///
    /// # Memory Ordering
    /// Uses relaxed ordering for performance, as spurious
    /// staleness is benign and status is periodically refreshed.
    #[inline]
    pub fn status_bits(&self) -> (bool, bool) {
        let status = self.status.load(Ordering::Relaxed);
        // (yield, partition)
        (
            status & STATUS_BIT_YIELD != 0,
            status & STATUS_BIT_PARTITION != 0,
        )
    }

    /// Sets the `STATUS_BIT_YIELD` flag for this worker and releases a permit if it was not previously set.
    ///
    /// # Purpose
    /// Requests the worker to yield (i.e., temporarily relinquish active scheduling)
    /// so that other workers can take priority or perform balancing. This enables
    /// cooperative multitasking among workers in high-contention or handoff scenarios.
    ///
    /// # Behavior
    /// - If the yield bit was previously unset (i.e., this is the first request to yield),
    ///   this method also releases one permit to ensure the sleeping worker receives a wakeup.
    /// - If already set, does nothing except marking the yield flag again (idempotent).
    ///
    /// # Concurrency
    /// Safe for concurrent use: races to set the yield bit and release permits are benign.
    ///
    /// # Memory Ordering
    /// Uses Acquire/Release ordering to ensure that the yield bit is
    /// visible to consumers before subsequent state changes or wakeups.
    #[inline]
    pub fn mark_yield(&self) {
        let prev = self.status.fetch_or(STATUS_BIT_YIELD, Ordering::AcqRel);
        if prev & STATUS_BIT_YIELD == 0 {
            self.release(1);
        }
    }

    /// Attempts to clear the yield bit (`STATUS_BIT_YIELD`) in the status word.
    ///
    /// # Purpose
    ///
    /// This function is used to indicate that the current worker should stop yielding,
    /// i.e., it is no longer in a yielded state and is eligible to process new work.
    /// The yield bit is typically set to signal a worker to yield and released to
    /// allow the worker to resume normal operation. Clearing this bit is a
    /// coordinated operation to avoid spurious lost work or premature reactivation.
    ///
    /// # Concurrency
    ///
    /// The method uses a loop with atomic compare-and-exchange to guarantee that the
    /// yield bit is only cleared if it was previously set, handling concurrent attempts
    /// to manipulate this bit. In case there is a race and the bit has already been
    /// cleared by another thread, this function will exit quietly and make no changes.
    ///
    /// # Behavior
    ///
    /// - If the yield bit is already clear, the function returns immediately.
    /// - Otherwise, it performs a compare-and-exchange to clear the bit. If this
    ///   succeeds, it exits; if not, it reloads the word and repeats the process,
    ///   only trying again if the yield bit is still set.
    #[inline]
    pub fn try_unmark_yield(&self) {
        loop {
            // Load the current status word with Acquire ordering to observe the latest status.
            let snapshot = self.status.load(Ordering::Acquire);
            // If the yield bit is not set, no action is needed.
            if snapshot & STATUS_BIT_YIELD == 0 {
                return;
            }

            // Attempt to clear the yield bit atomically while preserving other bits.
            match self.status.compare_exchange(
                snapshot,
                snapshot & !STATUS_BIT_YIELD,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                // If successful, the yield bit was cleared; return.
                Ok(_) => return,
                // If status changed in the meantime, reload and retry if the yield bit is still set.
                Err(actual) => {
                    if actual & STATUS_BIT_YIELD == 0 {
                        return;
                    }
                }
            }
        }
    }

    /// Marks the partition bit as active, indicating there is work in the partition.
    ///
    /// This sets the `STATUS_BIT_PARTITION` bit in the status word. If this was a
    /// transition from no active partition to active (i.e., the bit was previously
    /// clear), it releases one permit to wake up a worker to process tasks in this
    /// partition.
    ///
    /// # Example
    ///
    /// ```
    /// // Called when a leaf in the partition becomes non-empty
    /// waker.mark_tasks();
    /// ```
    #[inline]
    pub fn mark_tasks(&self) {
        let prev = self.status.fetch_or(STATUS_BIT_PARTITION, Ordering::AcqRel);
        if prev & STATUS_BIT_PARTITION == 0 {
            self.release(1);
        }
    }

    /// Attempts to clear the partition active bit (`STATUS_BIT_PARTITION`) in the status word if no
    /// leaves in the partition are active.
    ///
    /// This is typically called after a partition leaf transitions to empty. If the bit was set
    /// (partition was active), this function clears it, indicating no more work is present in the partition.
    /// If new work becomes available (i.e., the partition_summary is nonzero) after the bit is cleared,
    /// it immediately re-arms the bit to avoid lost wakeups.
    ///
    /// This function is safe to call spuriously and will exit without making changes if the partition bit
    /// is already clear.
    ///
    /// # Concurrency
    ///
    /// Uses a loop with atomic compare-and-exchange to ensure the bit is only cleared if no other
    /// thread has concurrently set it again. If racing with a producer, the bit will be re-armed as needed to
    /// prevent missing new work.
    #[inline]
    pub fn try_unmark_tasks(&self) {
        loop {
            let snapshot = self.status.load(Ordering::Relaxed);
            if snapshot & STATUS_BIT_PARTITION == 0 {
                return;
            }

            match self.status.compare_exchange(
                snapshot,
                snapshot & !STATUS_BIT_PARTITION,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // If new partition work arrived concurrently, re-arm the bit
                    if self.partition_summary.load(Ordering::Acquire) != 0 {
                        self.mark_tasks();
                    }
                    return;
                }
                Err(actual) => {
                    if actual & STATUS_BIT_PARTITION == 0 {
                        return;
                    }
                }
            }
        }
    }

    /// Marks a signal word at `index` (0..63) as active in the summary.
    ///
    /// Called by producers when a queue transitions from empty to non-empty.
    /// If this is a **0→1 transition** (bit was previously clear), adds 1 permit
    /// and wakes 1 sleeping thread.
    ///
    /// # Fast Path
    ///
    /// If the bit is already set, returns immediately without touching atomics.
    /// This is the common case when multiple producers push to the same word group.
    ///
    /// # Arguments
    ///
    /// * `index` - Word index (0..63) to mark as active
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Producer pushes to queue 5 in word 0
    /// let (was_empty, was_set) = signal.set(5);
    /// if was_empty && was_set {
    ///     waker.mark_active(0);  // Wake 1 consumer
    /// }
    /// ```
    #[inline]
    pub fn mark_active(&self, index: u64) {
        debug_assert!(
            index < STATUS_SUMMARY_BITS as u64,
            "summary index {} exceeds {} bits",
            index,
            STATUS_SUMMARY_BITS
        );
        let mask = 1u64 << index;
        if self.status.load(Ordering::Relaxed) & mask != 0 {
            return;
        }
        let prev = self.status.fetch_or(mask, Ordering::Relaxed);
        if prev & mask == 0 {
            self.release(1);
        }
    }

    /// Batch version of `mark_active()`: marks multiple words as active at once.
    ///
    /// Efficiently handles multiple queues becoming active simultaneously.
    /// Releases exactly `k` permits, where `k` is the number of **0→1 transitions**
    /// (newly-active words).
    ///
    /// # Optimization
    ///
    /// Uses a single `fetch_or` instead of calling `mark_active()` in a loop,
    /// reducing atomic contention when many queues activate together.
    ///
    /// # Arguments
    ///
    /// * `mask` - Bitmap of words to mark active (bit `i` = word `i`)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Multiple queues became active
    /// let mut active_words = 0u64;
    /// for word_idx in 0..64 {
    ///     if word_became_active(word_idx) {
    ///         active_words |= 1 << word_idx;
    ///     }
    /// }
    /// waker.mark_active_mask(active_words);  // Single atomic op
    /// ```
    #[inline]
    pub fn mark_active_mask(&self, mask: u64) {
        let summary_mask = mask & STATUS_SUMMARY_MASK;
        if summary_mask == 0 {
            return;
        }
        let prev = self.status.fetch_or(summary_mask, Ordering::Relaxed);
        let newly = (!prev) & summary_mask;
        let k = newly.count_ones() as usize;
        if k > 0 {
            self.release(k);
        }
    }

    /// Clears the summary bit for `bit_index` if the corresponding signal word is empty.
    ///
    /// This is **lazy cleanup** - consumers call this after draining a word to prevent
    /// false positives in future `snapshot_summary()` calls. However, it's safe to skip
    /// this; the system remains correct with stale summary bits.
    ///
    /// # Arguments
    ///
    /// * `bit_index` - Word index (0..63) to potentially clear
    /// * `signal` - The actual signal word to check for emptiness
    ///
    /// # Example
    ///
    /// ```ignore
    /// // After draining all queues in word 3
    /// waker.try_unmark_if_empty(3, &signal_word_3);
    /// ```
    #[inline]
    pub fn try_unmark_if_empty(&self, bit_index: u64, signal: &AtomicU64) {
        debug_assert!(
            bit_index < STATUS_SUMMARY_BITS as u64,
            "summary index {} exceeds {} bits",
            bit_index,
            STATUS_SUMMARY_BITS
        );
        let mask = 1u64 << bit_index;

        loop {
            if signal.load(Ordering::Acquire) != 0 {
                return;
            }

            let snapshot = self.status.load(Ordering::Relaxed);
            if snapshot & mask == 0 {
                return;
            }

            match self.status.compare_exchange(
                snapshot,
                snapshot & !mask,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    if signal.load(Ordering::Acquire) != 0 {
                        // Re-arm summary and release if work arrived concurrently.
                        self.mark_active(bit_index);
                    }
                    return;
                }
                Err(actual) => {
                    if actual & mask == 0 {
                        return;
                    }
                }
            }
        }
    }

    /// Unconditionally clears the summary bit for `bit_index`.
    ///
    /// Faster than `try_unmark_if_empty()` when the caller already knows
    /// the word is empty (avoids checking the signal word).
    ///
    /// # Arguments
    ///
    /// * `bit_index` - Word index (0..63) to clear
    #[inline]
    pub fn try_unmark(&self, bit_index: u64) {
        debug_assert!(
            bit_index < STATUS_SUMMARY_BITS as u64,
            "summary index {} exceeds {} bits",
            bit_index,
            STATUS_SUMMARY_BITS
        );
        let mask = 1u64 << bit_index;
        if self.status.load(Ordering::Relaxed) & mask != 0 {
            self.status
                .fetch_and(!(1u64 << bit_index), Ordering::Relaxed);
        }
    }

    // ────────────────────────────────────────────────────────────────────────────
    // CONSUMER-SIDE API
    // ────────────────────────────────────────────────────────────────────────────

    /// Returns a snapshot of the current summary bitmap.
    ///
    /// Consumers use this to quickly identify which word groups have potential work.
    /// If bit `i` is set, word `i` *may* have active queues (false positives possible
    /// due to lazy cleanup).
    ///
    /// # Memory Ordering
    ///
    /// Uses `Relaxed` because this is a hint, not a synchronization point. The actual
    /// queue data is synchronized via acquire/release on the permits counter.
    ///
    /// # Returns
    ///
    /// A u64 bitmap where bit `i` indicates word `i` has potential work.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let summary = waker.snapshot_summary();
    /// for word_idx in 0..64 {
    ///     if summary & (1 << word_idx) != 0 {
    ///         // Check queues in word_idx
    ///     }
    /// }
    /// ```
    #[inline]
    pub fn snapshot_summary(&self) -> u64 {
        self.status.load(Ordering::Relaxed) & STATUS_SUMMARY_MASK
    }

    /// Finds the nearest set bit to `nearest_to_index` in the summary.
    ///
    /// Useful for maintaining **locality**: continue working on queues near
    /// the last processed index, improving cache behavior.
    ///
    /// # Arguments
    ///
    /// * `nearest_to_index` - Preferred starting point (0..63)
    ///
    /// # Returns
    ///
    /// The index of the nearest set bit, or undefined if summary is empty.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut last_word = 0;
    /// loop {
    ///     last_word = waker.summary_select(last_word);
    ///     // Process queues in word last_word
    /// }
    /// ```
    #[inline]
    pub fn summary_select(&self, nearest_to_index: u64) -> u64 {
        let summary = self.status.load(Ordering::Relaxed) & STATUS_SUMMARY_MASK;
        crate::bits::find_nearest(summary, nearest_to_index)
    }

    // ────────────────────────────────────────────────────────────────────────────
    // PERMIT SYSTEM (Counting Semaphore)
    // ────────────────────────────────────────────────────────────────────────────

    /// Non-blocking attempt to acquire a permit.
    ///
    /// Atomically decrements the permit counter if available. This is the **lock-free
    /// fast path** used by consumers before resorting to blocking.
    ///
    /// # Returns
    ///
    /// - `true` if a permit was consumed (consumer should process work)
    /// - `false` if no permits available (queue likely empty)
    ///
    /// # Memory Ordering
    ///
    /// Uses `AcqRel` to synchronize with producers' `Release` in `release()`.
    /// This ensures queue data written by producers is visible to this consumer.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if waker.try_acquire() {
    ///     // Process work (permit guarantees something is available)
    /// } else {
    ///     // No work, maybe park or spin
    /// }
    /// ```
    #[inline]
    pub fn try_acquire(&self) -> bool {
        self.permits
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |p| p.checked_sub(1))
            .is_ok()
    }

    /// Blocking acquire: parks the thread in the event loop until a permit becomes available
    /// or an I/O event occurs.
    ///
    /// Tries the fast path first (`try_acquire()`), then falls back to polling the event loop.
    ///
    /// # Arguments
    ///
    /// * `event_loop` - The worker's event loop to poll
    ///
    /// # Returns
    ///
    /// - `true` if a permit was acquired
    /// - `false` if returned due to I/O event (no permit acquired)
    pub fn acquire_with_io(&self, event_loop: &mut IoDriver) -> bool {
        if self.try_acquire() {
            return true;
        }

        self.sleepers.fetch_add(1, Ordering::Relaxed);

        loop {
            // Fast check before polling
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return true;
            }

            // Poll event loop (blocking)
            // We pass None for timeout to block indefinitely until:
            // 1. I/O event
            // 2. Waker notification (via release())
            // 3. Timer expiration (if using integrated timer wheel)
            if let Err(_) = event_loop.poll_once(None) {
                // If polling fails, we should probably return to avoid infinite loop
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return false;
            }

            // Check permits again after waking up
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return true;
            }
        }
    }

    /// Blocking acquire with timeout.
    ///
    /// Like `acquire_with_io()`, but returns after `timeout` if no permit becomes available.
    pub fn acquire_timeout_with_io(&self, event_loop: &mut IoDriver, timeout: Duration) -> bool {
        if self.try_acquire() {
            return true;
        }

        let start = std::time::Instant::now();
        self.sleepers.fetch_add(1, Ordering::Relaxed);

        loop {
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return true;
            }

            let elapsed = start.elapsed();
            if elapsed >= timeout {
                break;
            }
            let left = timeout - elapsed;

            // Poll with timeout
            if let Err(_) = event_loop.poll_once(Some(left)) {
                break;
            }
        }

        self.sleepers.fetch_sub(1, Ordering::Relaxed);
        false
    }

    /// Releases `n` permits and wakes up to `n` sleeping threads.
    ///
    /// Called by producers (indirectly via `mark_active`) when queues become active.
    /// Uses **targeted wakeups**: only notifies up to `min(n, sleepers)` threads,
    /// avoiding unnecessary `notify_one()` calls.
    ///
    /// # Permit Accumulation
    ///
    /// If no threads are sleeping, permits accumulate for future consumers.
    /// This guarantees **no lost wakeups**: late-arriving consumers find work immediately.
    ///
    /// # Arguments
    ///
    /// * `n` - Number of permits to release (typically 1 or count of newly-active queues)
    ///
    /// # Memory Ordering
    ///
    /// Uses `Release` to ensure queue data is visible to consumers who `Acquire`
    /// via `try_acquire()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Producer activates 3 queues
    /// waker.release(3);  // Wakes up to 3 sleeping consumers
    /// ```
    #[inline]
    pub fn release(&self, n: usize) {
        if n == 0 {
            return;
        }
        self.permits.fetch_add(n as u64, Ordering::Release);
        let to_wake = n.min(self.sleepers.load(Ordering::Relaxed));

        // No event loop waker needed anymore - the waker is stored in EventLoop itself
        // and will be called directly when needed
    }

    // ────────────────────────────────────────────────────────────────────────────
    // INSPECTION / DEBUGGING
    // ────────────────────────────────────────────────────────────────────────────

    /// Returns the current summary bitmap.
    ///
    /// Useful for debugging or metrics. Equivalent to `snapshot_summary()` but
    /// uses `Acquire` ordering for stronger visibility guarantees.
    #[inline]
    pub fn summary_bits(&self) -> u64 {
        self.status.load(Ordering::Acquire) & STATUS_SUMMARY_MASK
    }

    /// Returns the current number of available permits.
    ///
    /// Useful for monitoring queue health or load. A high permit count may
    /// indicate consumers are falling behind.
    #[inline]
    pub fn permits(&self) -> u64 {
        self.permits.load(Ordering::Acquire)
    }

    /// Returns the approximate number of sleeping threads.
    ///
    /// Best-effort count (uses Relaxed ordering). Useful for debugging or
    /// understanding system utilization.
    #[inline]
    pub fn sleepers(&self) -> usize {
        self.sleepers.load(Ordering::Relaxed)
    }

    /// Increments the sleeper count, indicating a thread is about to park.
    ///
    /// Should be called BEFORE checking for work the final time, to prevent
    /// lost wakeups. The calling thread must unregister via `unregister_sleeper()`
    /// after waking up.
    ///
    /// # Example
    ///
    /// ```ignore
    /// waker.register_sleeper();
    /// // Final check for work
    /// if has_work() {
    ///     waker.unregister_sleeper();
    ///     return; // Found work, don't park
    /// }
    /// // Actually park...
    /// waker.acquire();
    /// waker.unregister_sleeper();
    /// ```
    #[inline]
    pub fn register_sleeper(&self) {
        self.sleepers.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrements the sleeper count, indicating a thread has woken up.
    ///
    /// Should be called after waking up from `acquire()` or if aborting
    /// a park attempt after `register_sleeper()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// waker.register_sleeper();
    /// // ... park ...
    /// waker.unregister_sleeper(); // Woke up
    /// ```
    #[inline]
    pub fn unregister_sleeper(&self) {
        self.sleepers.fetch_sub(1, Ordering::Relaxed);
    }

    // ────────────────────────────────────────────────────────────────────────────
    // WORKER MANAGEMENT API
    // ────────────────────────────────────────────────────────────────────────────

    /// Registers a new worker thread and returns the new total worker count.
    ///
    /// Should be called when a FastTaskWorker thread starts. Workers use this
    /// count to partition the signal space for optimal load distribution.
    ///
    /// # Returns
    ///
    /// The new total worker count after registration.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let waker = arena.waker();
    /// let total_workers = unsafe { (*waker).register_worker() };
    /// println!("Now have {} workers", total_workers);
    /// ```
    #[inline]
    pub fn register_worker(&self) -> usize {
        self.worker_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Unregisters a worker thread and returns the new total worker count.
    ///
    /// Should be called when a FastTaskWorker thread stops. This allows
    /// remaining workers to reconfigure their partitions.
    ///
    /// # Returns
    ///
    /// The new total worker count after unregistration.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Worker stopping
    /// let waker = arena.waker();
    /// let remaining_workers = unsafe { (*waker).unregister_worker() };
    /// println!("{} workers remaining", remaining_workers);
    /// ```
    #[inline]
    pub fn unregister_worker(&self) -> usize {
        self.worker_count
            .fetch_sub(1, Ordering::Relaxed)
            .saturating_sub(1)
    }

    /// Returns the current number of active worker threads.
    ///
    /// Workers periodically check this value to detect when the worker count
    /// has changed and reconfigure their signal partitions accordingly.
    ///
    /// Uses Relaxed ordering since workers can tolerate slightly stale values
    /// and will eventually see the update on their next check.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let waker = arena.waker();
    /// let count = unsafe { (*waker).get_worker_count() };
    /// if count != cached_count {
    ///     // Reconfigure partition
    /// }
    /// ```
    #[inline]
    pub fn get_worker_count(&self) -> usize {
        self.worker_count.load(Ordering::Relaxed)
    }

    // ────────────────────────────────────────────────────────────────────────────
    // PARTITION SUMMARY MANAGEMENT
    // ────────────────────────────────────────────────────────────────────────────

    /// Synchronize partition summary from SummaryTree leaf range.
    ///
    /// Samples the worker's assigned partition of the SummaryTree and updates
    /// the local `partition_summary` bitmap. When the partition transitions from
    /// empty to non-empty, sets `STATUS_BIT_PARTITION` and adds a permit to wake
    /// the worker.
    ///
    /// This should be called before parking to ensure the worker doesn't sleep
    /// when tasks are available in its partition.
    ///
    /// # Arguments
    ///
    /// * `partition_start` - First leaf index in this worker's partition
    /// * `partition_end` - One past the last leaf index (exclusive)
    /// * `leaf_words` - Slice of AtomicU64 leaf words from SummaryTree
    ///
    /// # Returns
    ///
    /// `true` if the partition currently has work, `false` otherwise
    ///
    /// # Panics
    ///
    /// Panics in debug mode if partition is larger than 64 leafs
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Before parking, sync partition status
    /// let waker = &service.wakers[worker_id];
    /// let has_work = waker.sync_partition_summary(
    ///     self.partition_start,
    ///     self.partition_end,
    ///     &self.arena.active_tree().leaf_words,
    /// );
    /// ```
    pub fn sync_partition_summary(
        &self,
        partition_start: usize,
        partition_end: usize,
        leaf_words: &[AtomicU64],
    ) -> bool {
        debug_assert!(
            partition_end >= partition_start,
            "partition_end ({}) must be >= partition_start ({})",
            partition_end,
            partition_start
        );

        let partition_len = partition_end.saturating_sub(partition_start);
        if partition_len == 0 {
            let prev = self.partition_summary.swap(0, Ordering::AcqRel);
            if prev != 0 {
                self.try_unmark_tasks();
            }
            return false;
        }

        debug_assert!(
            partition_len <= 64,
            "partition size {} exceeds 64-bit bitmap capacity",
            partition_len
        );

        let partition_mask = if partition_len >= 64 {
            u64::MAX
        } else {
            (1u64 << partition_len) - 1
        };

        loop {
            let mut new_summary = 0u64;

            for (offset, leaf_idx) in (partition_start..partition_end).enumerate() {
                if let Some(leaf_word) = leaf_words.get(leaf_idx) {
                    if leaf_word.load(Ordering::Acquire) != 0 {
                        new_summary |= 1u64 << offset;
                    }
                }
            }

            let prev = self.partition_summary.load(Ordering::Acquire);
            let prev_masked = prev & partition_mask;

            if prev_masked != 0 {
                let mut to_clear = prev_masked & !new_summary;
                while to_clear != 0 {
                    let bit = to_clear.trailing_zeros() as usize;
                    let leaf_idx = partition_start + bit;
                    if let Some(leaf_word) = leaf_words.get(leaf_idx) {
                        if leaf_word.load(Ordering::Acquire) != 0 {
                            new_summary |= 1u64 << bit;
                        }
                    }
                    to_clear &= to_clear - 1;
                }
            }

            let desired = (prev & !partition_mask) | new_summary;

            match self.partition_summary.compare_exchange(
                prev,
                desired,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    let had_work = prev_masked != 0;
                    let has_work = (desired & partition_mask) != 0;

                    if has_work && !had_work {
                        self.mark_tasks();
                    } else if !has_work && had_work {
                        self.try_unmark_tasks();
                    }

                    return has_work;
                }
                Err(_) => continue,
            }
        }
    }

    /// Get current partition summary bitmap.
    ///
    /// Returns a bitmap where bit `i` indicates whether leaf `partition_start + i`
    /// has active tasks. This is a snapshot and may become stale immediately.
    ///
    /// Uses `Relaxed` ordering since this is a hint for optimization purposes.
    ///
    /// # Returns
    ///
    /// Bitmap of active leafs in this worker's partition
    #[inline]
    pub fn partition_summary(&self) -> u64 {
        self.partition_summary.load(Ordering::Relaxed)
    }

    /// Check if a specific leaf in the partition has work.
    ///
    /// # Arguments
    ///
    /// * `local_leaf_idx` - Leaf index relative to partition start (0-63)
    ///
    /// # Returns
    ///
    /// `true` if the leaf appears to have work based on the cached summary
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Check if first leaf in partition has work
    /// if waker.partition_leaf_has_work(0) {
    ///     // Try to acquire from that leaf
    /// }
    /// ```
    #[inline]
    pub fn partition_leaf_has_work(&self, local_leaf_idx: usize) -> bool {
        debug_assert!(
            local_leaf_idx < 64,
            "local_leaf_idx {} out of range",
            local_leaf_idx
        );
        let summary = self.partition_summary.load(Ordering::Relaxed);
        summary & (1u64 << local_leaf_idx) != 0
    }

    /// Directly update partition summary for a specific leaf.
    ///
    /// This is called when a task is scheduled into a leaf to immediately update
    /// the partition owner's summary without waiting for the next sync.
    ///
    /// # Arguments
    ///
    /// * `local_leaf_idx` - Leaf index relative to partition start (0-63)
    ///
    /// # Returns
    ///
    /// `true` if this was the first active leaf (partition was empty before)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // When scheduling a task, immediately update owner's partition summary
    /// let owner_waker = &service.wakers[owner_id];
    /// if owner_waker.mark_partition_leaf_active(local_leaf_idx) {
    ///     // This was the first task - worker will be woken by the partition flag
    /// }
    /// ```
    pub fn mark_partition_leaf_active(&self, local_leaf_idx: usize) -> bool {
        debug_assert!(
            local_leaf_idx < 64,
            "local_leaf_idx {} out of range",
            local_leaf_idx
        );

        let mask = 1u64 << local_leaf_idx;
        let old_summary = self.partition_summary.fetch_or(mask, Ordering::AcqRel);

        // If partition was empty, mark partition flag to wake the worker
        if old_summary == 0 {
            self.mark_tasks();
            true
        } else {
            false
        }
    }

    /// Clear partition summary for a specific leaf.
    ///
    /// Called when a leaf becomes empty. If this was the last active leaf,
    /// attempts to clear the partition status bit.
    ///
    /// # Arguments
    ///
    /// * `local_leaf_idx` - Leaf index relative to partition start (0-63)
    pub fn clear_partition_leaf(&self, local_leaf_idx: usize) {
        debug_assert!(
            local_leaf_idx < 64,
            "local_leaf_idx {} out of range",
            local_leaf_idx
        );

        let bit = 1u64 << local_leaf_idx;
        let old_summary = self.partition_summary.fetch_and(!bit, Ordering::AcqRel);

        if (old_summary & bit) != 0 && (old_summary & !bit) == 0 {
            self.try_unmark_tasks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_waker_creation() {
        let waker = WorkerWaker::new();
        assert_eq!(waker.status(), 0);
        assert_eq!(waker.permits(), 0);
        assert_eq!(waker.sleepers(), 0);
        assert_eq!(waker.get_worker_count(), 0);
    }

    #[test]
    fn test_mark_active() {
        let waker = Arc::new(WorkerWaker::new());

        // Mark word 0 active
        waker.mark_active(0);
        assert_eq!(waker.status() & 1, 1);
        assert_eq!(waker.permits(), 1);
        assert_eq!(waker.snapshot_summary(), 1);

        // Mark same word again (should not add permit)
        waker.mark_active(0);
        assert_eq!(waker.permits(), 1);

        // Mark another word
        waker.mark_active(5);
        assert_eq!(waker.permits(), 2);
        assert_eq!(waker.snapshot_summary(), (1 << 0) | (1 << 5));
    }

    #[test]
    fn test_acquire_and_release_with_io() {
        // To test acquire_with_io properly, we need a real EventLoop
        // But creating a real EventLoop in a unit test might be heavy
        // and require OS resources.
        // For now, we'll just test the try_acquire path which doesn't need IO

        let waker = Arc::new(WorkerWaker::new());

        // Initial state: empty
        assert!(!waker.try_acquire());

        // Add work
        waker.mark_active(0);
        assert_eq!(waker.permits(), 1);

        // Acquire
        assert!(waker.try_acquire());
        assert_eq!(waker.permits(), 0);

        // Empty again
        assert!(!waker.try_acquire());
    }

    #[test]
    fn test_integration_with_event_loop() {
        let waker = Arc::new(WorkerWaker::new());
        let mut event_loop = IoDriver::new().unwrap();
        let io_waker = event_loop.waker().unwrap();

        // Thread to wake us up
        let waker_clone = waker.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            waker_clone.mark_active(0);
        });

        // Should block and then return true when work arrives
        // Note: This blocks the test thread, which is fine
        assert!(waker.acquire_with_io(&mut event_loop));
        assert_eq!(waker.permits(), 0);

        // Try timeout version
        let start = std::time::Instant::now();
        assert!(!waker.acquire_timeout_with_io(&mut event_loop, Duration::from_millis(50)));
        assert!(start.elapsed() >= Duration::from_millis(50));
    }
}
