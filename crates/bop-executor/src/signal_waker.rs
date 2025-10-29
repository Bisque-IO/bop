use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::utils::CachePadded;

use crate::bits::is_set;

/// A cache-optimized, summary-driven waker for coordinating up to 4096 lock-free queues.
///
/// # Architecture
///
/// `SignalWaker` implements a **two-level hierarchy** for efficient work discovery:
///
/// ```text
/// Level 1: Summary Word (64 bits)
///          ┌─────────────────────────────────────┐
///          │ Bit 0  Bit 1  ...  Bit 63           │  Each bit = "word has work"
///          └─────────────────────────────────────┘
///                 │      │           │
///                 ▼      ▼           ▼
/// Level 2: Signal Words (external AtomicU64s)
///          Word 0  Word 1  ...  Word 63
///          [64 q]  [64 q]  ...  [64 q]           Each bit = individual queue state
///
/// Total: 64 words × 64 bits = 4096 queues
/// ```
///
/// # Core Components
///
/// 1. **Summary Bitmap** (`summary`): Single u64 where bit `i` indicates whether
///    word `i` has any active queues. Enables O(1) lookup instead of O(N) scanning.
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
/// - `#[repr(align(64))]` on struct for cache-line alignment
/// - `CachePadded` on hot atomics to prevent false sharing
/// - Producer/consumer paths access different cache lines
///
/// ## Memory Ordering Strategy
/// - **Summary**: `Relaxed` - hint-based, false positives acceptable
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
/// ✅ **No lost wakeups**: Permits accumulate even if no threads are sleeping
/// ✅ **Bounded notifications**: Never wakes more than `sleepers` threads
/// ✅ **Lock-free fast path**: `try_acquire()` uses only atomics
/// ✅ **Summary consistency**: false positives OK, false negatives impossible
///
/// # Trade-offs
///
/// ⚠️ **False positives**: Summary may indicate work when queues are empty (lazy cleanup)
/// ⚠️ **Approximate sleeper count**: May over-notify slightly (but safely)
/// ⚠️ **64-word limit**: Summary is single u64 (extensible if needed)
///
/// # Usage Example
///
/// ```ignore
/// use std::sync::Arc;
/// use bop_mpmc::SignalWaker;
///
/// let waker = Arc::new(SignalWaker::new());
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
/// without coordination beyond the internal atomics and mutex/condvar for blocking.
#[repr(align(64))]
pub struct SignalWaker {
    /// **Summary bitmap**: Bit `i` set means word `i` has at least one active queue.
    ///
    /// This enables O(1) work discovery: consumers check this u64 instead of
    /// scanning all 4096 queues. Updated with `Relaxed` ordering since it's a
    /// hint (false positives are safe, cleaned up lazily).
    ///
    /// - Producer: Sets bits via `mark_active()` or `mark_active_mask()`
    /// - Consumer: Reads via `snapshot_summary()`, clears via `try_unmark_if_empty()`
    summary: CachePadded<AtomicU64>,

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
    /// When partition_summary transitions 0→non-zero, status bit 1 (STATUS_TASKS_AVAILABLE)
    /// is set and a permit is added to wake the worker.
    partition_summary: CachePadded<AtomicU64>,

    /// Mutex for condvar (only used in blocking paths)
    m: Mutex<()>,

    /// Condvar for parking/waking threads
    cv: Condvar,
}

impl SignalWaker {
    pub fn new() -> Self {
        Self {
            summary: CachePadded::new(AtomicU64::new(0)),
            status: CachePadded::new(AtomicU64::new(0)),
            permits: CachePadded::new(AtomicU64::new(0)),
            sleepers: CachePadded::new(AtomicUsize::new(0)),
            worker_count: CachePadded::new(AtomicUsize::new(0)),
            partition_summary: CachePadded::new(AtomicU64::new(0)),
            m: Mutex::new(()),
            cv: Condvar::new(),
        }
    }

    // ────────────────────────────────────────────────────────────────────────────
    // PRODUCER-SIDE API
    // ────────────────────────────────────────────────────────────────────────────

    #[inline]
    pub fn status(&self) -> u64 {
        self.status.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn status_bits(&self) -> (bool, bool) {
        let status = self.status.load(Ordering::Relaxed);
        // (yield, tasks)
        (status & (1u64 << 0) != 0, status & (1u64 << 1) != 0)
    }

    #[inline]
    pub fn mark_yield(&self) {
        if is_set(&self.status, 0) {
            return;
        }
        let prev = self.status.fetch_or(1u64 << 0, Ordering::Relaxed);
        if prev & (1u64 << 0) == 0 {
            self.release(1);
        }
    }

    #[inline]
    pub fn try_unmark_yield(&self) {
        if is_set(&self.status, 0) {
            self.status.fetch_and(!(1u64 << 0), Ordering::Relaxed);
        }
    }

    #[inline]
    pub fn mark_tasks(&self) {
        if is_set(&self.status, 1) {
            return;
        }
        let prev = self.status.fetch_or(1u64 << 1, Ordering::Relaxed);
        if prev & (1u64 << 1) == 0 {
            self.release(1);
        }
    }

    #[inline]
    pub fn try_unmark_tasks(&self) {
        if is_set(&self.status, 1) {
            self.status.fetch_and(!(1u64 << 1), Ordering::Relaxed);
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
        if is_set(&self.summary, index) {
            return;
        }
        let mask = 1u64 << index;
        let prev = self.summary.fetch_or(mask, Ordering::Relaxed);
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
        if mask == 0 {
            return;
        }
        let prev = self.summary.fetch_or(mask, Ordering::Relaxed);
        let newly = (!prev) & mask;
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
        if signal.load(Ordering::Relaxed) == 0 {
            self.summary
                .fetch_and(!(1u64 << bit_index), Ordering::Relaxed);
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
        if is_set(&self.summary, bit_index) {
            self.summary
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
        self.summary.load(Ordering::Relaxed)
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
        crate::bits::find_nearest(self.summary.load(Ordering::Relaxed), nearest_to_index)
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

    /// Blocking acquire: parks the thread until a permit becomes available.
    ///
    /// Tries the fast path first (`try_acquire()`), then falls back to parking
    /// on a condvar. Handles spurious wakeups by rechecking permits in a loop.
    ///
    /// # Blocking Behavior
    ///
    /// 1. Increment `sleepers` count
    /// 2. Wait on condvar (releases mutex)
    /// 3. Recheck permits after wakeup
    /// 4. Decrement `sleepers` on exit
    ///
    /// # Panics
    ///
    /// Panics if the mutex or condvar is poisoned (indicates a panic in another thread
    /// while holding the lock).
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     waker.acquire();  // Blocks until work available
    ///     process_work();
    /// }
    /// ```
    pub fn acquire(&self) {
        if self.try_acquire() {
            return;
        }
        let mut g = self.m.lock().expect("waker mutex poisoned");
        self.sleepers.fetch_add(1, Ordering::Relaxed);
        loop {
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            g = self.cv.wait(g).expect("waker condvar wait poisoned");
        }
    }

    /// Blocking acquire with timeout.
    ///
    /// Like `acquire()`, but returns after `timeout` if no permit becomes available.
    /// Useful for implementing shutdown or periodic maintenance.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum duration to wait
    ///
    /// # Returns
    ///
    /// - `true` if a permit was acquired
    /// - `false` if timed out without acquiring
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// loop {
    ///     if waker.acquire_timeout(Duration::from_secs(1)) {
    ///         process_work();
    ///     } else {
    ///         // Timeout - check for shutdown signal
    ///         if should_shutdown() { break; }
    ///     }
    /// }
    /// ```
    pub fn acquire_timeout(&self, timeout: Duration) -> bool {
        if self.try_acquire() {
            return true;
        }
        let start = std::time::Instant::now();
        let mut g = self.m.lock().expect("waker mutex poisoned");
        self.sleepers.fetch_add(1, Ordering::Relaxed);
        while start.elapsed() < timeout {
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return true;
            }
            let left = timeout.saturating_sub(start.elapsed());
            let (gg, res) = self
                .cv
                .wait_timeout(g, left)
                .expect("waker condvar wait poisoned");
            g = gg;
            if res.timed_out() {
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
        for _ in 0..to_wake {
            self.cv.notify_one();
        }
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
        self.summary.load(Ordering::Acquire)
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
    /// empty to non-empty, sets status bit 1 (STATUS_TASKS_AVAILABLE) and adds
    /// a permit to wake the worker.
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
            partition_end.saturating_sub(partition_start) <= 64,
            "partition size {} exceeds 64-bit bitmap capacity",
            partition_end.saturating_sub(partition_start)
        );

        let mut new_summary = 0u64;

        // Sample leaf words in our partition
        for leaf_idx in partition_start..partition_end {
            if let Some(leaf_word) = leaf_words.get(leaf_idx) {
                let leaf_value = leaf_word.load(Ordering::Relaxed);
                if leaf_value != 0 {
                    let bit_idx = leaf_idx - partition_start;
                    new_summary |= 1u64 << bit_idx;
                }
            }
        }

        // Update partition summary
        let old_summary = self.partition_summary.swap(new_summary, Ordering::Relaxed);

        // Update status bit 1 based on summary
        let had_work = old_summary != 0;
        let has_work = new_summary != 0;

        if has_work && !had_work {
            // 0→1 transition: mark tasks available
            self.mark_tasks();
        } else if !has_work && had_work {
            // 1→0 transition: try unmark
            self.try_unmark_tasks();
        }

        has_work
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
    ///     // This was the first task - worker will be woken by mark_tasks()
    /// }
    /// ```
    pub fn mark_partition_leaf_active(&self, local_leaf_idx: usize) -> bool {
        debug_assert!(
            local_leaf_idx < 64,
            "local_leaf_idx {} out of range",
            local_leaf_idx
        );

        let mask = 1u64 << local_leaf_idx;
        let old_summary = self.partition_summary.fetch_or(mask, Ordering::Relaxed);

        // If partition was empty, mark tasks available
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
    /// attempts to clear the tasks status bit.
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

        let mask = !(1u64 << local_leaf_idx);
        let new_summary = self.partition_summary.fetch_and(mask, Ordering::Relaxed) & mask;

        // If partition is now empty, try to unmark tasks
        if new_summary == 0 {
            self.try_unmark_tasks();
        }
    }
}

impl Default for SignalWaker {
    fn default() -> Self {
        Self::new()
    }
}
