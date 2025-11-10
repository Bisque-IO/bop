use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use crate::utils::CachePadded;

use std::sync::atomic::AtomicUsize;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::bits::is_set;

pub const STATUS_SUMMARY_BITS: u32 = 64;

/// Number of bits per signal word (64-bit atomic).
///
/// Each signal word can track up to 64 queues. This matches the width of
/// `AtomicU64` and provides efficient bit manipulation via hardware instructions
/// (POPCNT, BSF, etc.).
pub const SIGNAL_CAPACITY: u64 = 64;

/// Bitmask for extracting bit index within a signal word.
///
/// Equivalent to `index % 64`, used for fast modulo via bitwise AND:
/// ```ignore
/// bit_index = queue_id & SIGNAL_MASK;
/// ```
pub const SIGNAL_MASK: u64 = SIGNAL_CAPACITY - 1;

/// A cache-line padded 64-bit atomic bitmap for tracking queue readiness.
///
/// Each `Signal` represents a group of up to 64 queues, where each bit indicates
/// whether the corresponding queue has work available. Multiple `Signal` instances
/// are coordinated via a `SignalWaker` to form a complete two-level bitmap.
///
/// # Design
///
/// ```text
/// Signal (64-bit AtomicU64)
/// ┌───┬───┬───┬───┬─────┬───┐
/// │ 0 │ 1 │ 0 │ 1 │ ... │ 0 │  Each bit = one queue's scheduled state
/// └───┴───┴───┴───┴─────┴───┘
///   Q0  Q1  Q2  Q3  ...  Q63
/// ```
///
/// # Cache Optimization
///
/// The inner state is wrapped in `Arc<CachePadded<...>>` to:
/// - Allow cheap cloning (single pointer copy)
/// - Prevent false sharing between different signals
/// - Optimize for hot paths (producers setting bits, executor clearing bits)
///
/// # Thread Safety
///
/// All operations use atomic instructions. Multiple producers can concurrently set
/// bits (via `set()`), and the executor can concurrently acquire/clear bits (via
/// `acquire()` or `try_acquire()`).
///
/// # Cloning
///
/// `Signal` is cheaply clonable via `Arc`. All clones share the same underlying
/// atomic bitmap, making it suitable for distribution across multiple producer threads.
#[derive(Clone)]
pub struct Signal {
    /// Shared, cache-line padded inner state.
    inner: Arc<CachePadded<SignalInner>>,
}

impl Signal {
    /// Returns the signal's index within the SignalWaker's signal array.
    ///
    /// This index is used to:
    /// - Map this signal to the corresponding bit in the summary bitmap
    /// - Identify which group of 64 queues this signal represents
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::with_index(5);
    /// assert_eq!(signal.index(), 5);
    /// // This signal controls queues 320-383 (5 * 64 through (5+1) * 64 - 1)
    /// ```
    #[inline(always)]
    pub fn index(&self) -> u64 {
        return self.inner.index;
    }

    /// Returns a reference to the underlying atomic value.
    ///
    /// Provides direct access to the 64-bit bitmap for advanced use cases
    /// that need custom atomic operations beyond the provided methods.
    ///
    /// # Use Cases
    ///
    /// - Custom bit manipulation patterns
    /// - Debugging (observing raw bitmap state)
    /// - Integration with external synchronization primitives
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::new();
    /// signal.set(10);
    /// let raw_value = signal.value().load(Ordering::Relaxed);
    /// assert_eq!(raw_value & (1 << 10), 1 << 10);  // Bit 10 is set
    /// ```
    #[inline(always)]
    pub fn value(&self) -> &AtomicU64 {
        &self.inner.value
    }
}

/// Internal state for a Signal, cache-line padded to prevent false sharing.
///
/// # Fields
///
/// - `index`: The signal's position in the SignalWaker's signal array (0-61)
/// - `value`: 64-bit atomic bitmap where each bit represents a queue's readiness
struct SignalInner {
    /// Signal index in the SignalWaker array.
    pub index: u64,
    /// Atomic bitmap tracking up to 64 queues (bit N = queue ready state).
    value: AtomicU64,
}

impl Signal {
    /// Creates a new Signal with index 0 and all bits cleared.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::new();
    /// assert_eq!(signal.index(), 0);
    /// assert!(signal.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::with_value(0, 0)
    }

    /// Creates a new Signal with the specified index and all bits cleared.
    ///
    /// # Parameters
    ///
    /// - `index`: Position in the SignalWaker's signal array (0-61)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::with_index(10);
    /// assert_eq!(signal.index(), 10);
    /// assert!(signal.is_empty());
    /// ```
    pub fn with_index(index: u64) -> Self {
        debug_assert!(
            index < SIGNAL_CAPACITY as u64,
            "signal index {} exceeds status summary capacity {}",
            index,
            SIGNAL_CAPACITY
        );
        Self::with_value(index, 0)
    }

    /// Creates a new Signal with the specified index and initial bitmap value.
    ///
    /// This is primarily used for testing or restoring state. In normal operation,
    /// signals start with all bits cleared.
    ///
    /// # Parameters
    ///
    /// - `index`: Position in the SignalWaker's signal array (0-61)
    /// - `value`: Initial 64-bit bitmap value
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create signal with bits 0, 5, and 10 already set
    /// let signal = Signal::with_value(3, (1 << 0) | (1 << 5) | (1 << 10));
    /// assert_eq!(signal.size(), 3);
    /// assert!(signal.is_set(0));
    /// assert!(signal.is_set(5));
    /// assert!(signal.is_set(10));
    /// ```
    pub fn with_value(index: u64, value: u64) -> Self {
        debug_assert!(
            index < SIGNAL_CAPACITY as u64,
            "signal index {} exceeds status summary capacity {}",
            index,
            SIGNAL_CAPACITY
        );
        Self {
            inner: Arc::new(CachePadded::new(SignalInner {
                index,
                value: AtomicU64::new(value),
            })),
        }
    }

    /// Loads the current bitmap value with the specified memory ordering.
    ///
    /// # Parameters
    ///
    /// - `ordering`: Memory ordering for the load operation
    ///
    /// # Returns
    ///
    /// The 64-bit bitmap value where each set bit represents a ready queue.
    ///
    /// # Example
    ///
    /// ```ignore
    /// signal.set(5);
    /// signal.set(10);
    /// let value = signal.load(Ordering::Acquire);
    /// assert_eq!(value, (1 << 5) | (1 << 10));
    /// ```
    #[inline(always)]
    pub fn load(&self, ordering: Ordering) -> u64 {
        self.inner.value.load(ordering)
    }

    /// Returns the number of set bits (ready queues) in this signal.
    ///
    /// Equivalent to `popcount(bitmap)`, this counts how many queues in this
    /// signal group currently have work available.
    ///
    /// # Performance
    ///
    /// Uses the `POPCNT` instruction on x86_64 (~3 cycles), making it very efficient.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::new();
    /// signal.set(0);
    /// signal.set(5);
    /// signal.set(63);
    /// assert_eq!(signal.size(), 3);
    /// ```
    #[inline(always)]
    pub fn size(&self) -> u64 {
        self.load(Ordering::Relaxed).count_ones() as u64
    }

    /// Returns `true` if no bits are set (no ready queues).
    ///
    /// This is more efficient than `size() == 0` for checking emptiness.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::new();
    /// assert!(signal.is_empty());
    /// signal.set(10);
    /// assert!(!signal.is_empty());
    /// ```
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.load(Ordering::Relaxed).count_ones() == 0
    }

    /// Atomically sets a bit in the bitmap using fetch_or.
    ///
    /// This is the primary method for producers to signal that a queue has work available.
    ///
    /// # Parameters
    ///
    /// - `index`: Bit position to set (0-63)
    ///
    /// # Returns
    ///
    /// A tuple `(was_empty, was_set)`:
    /// - `was_empty`: `true` if this was the first bit set (signal transitioned from empty to non-empty)
    /// - `was_set`: `true` if the bit was successfully set (wasn't already set)
    ///
    /// # Use Cases
    ///
    /// The return values are used for summary bitmap updates:
    /// ```ignore
    /// let (was_empty, was_set) = signal.set(queue_bit);
    /// if was_empty && was_set {
    ///     // This signal was empty, now has work - update summary
    ///     waker.mark_active(signal.index());
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// ~5-10 ns (one atomic fetch_or operation)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::new();
    /// let (was_empty, was_set) = signal.set(5);
    /// assert!(was_empty);  // Signal was empty
    /// assert!(was_set);    // Bit 5 was not previously set
    ///
    /// let (was_empty, was_set) = signal.set(5);
    /// assert!(!was_empty); // Signal already had bits set
    /// assert!(!was_set);   // Bit 5 was already set
    /// ```
    #[inline(always)]
    pub fn set(&self, index: u64) -> (bool, bool) {
        crate::bits::set(&self.inner.value, index)
    }

    /// Atomically sets a bit using a precomputed bitmask.
    ///
    /// Similar to `set()`, but takes a precomputed `1 << index` value for cases
    /// where the bit position is computed once and reused.
    ///
    /// # Parameters
    ///
    /// - `bit`: Precomputed bitmask with exactly one bit set (e.g., `1 << 5`)
    ///
    /// # Returns
    ///
    /// The previous bitmap value before setting the bit.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::new();
    /// let bit_mask = 1u64 << 10;
    /// let prev = signal.set_with_bit(bit_mask);
    /// assert_eq!(prev, 0);  // Was empty
    /// assert!(signal.is_set(10));
    /// ```
    #[inline(always)]
    pub fn set_with_bit(&self, bit: u64) -> u64 {
        crate::bits::set_with_bit(&self.inner.value, bit)
    }

    /// Atomically clears a bit if it is currently set (CAS-based).
    ///
    /// This is the primary method for the executor to claim ownership of a ready queue.
    /// Uses a CAS loop to ensure the bit is cleared atomically.
    ///
    /// # Parameters
    ///
    /// - `index`: Bit position to clear (0-63)
    ///
    /// # Returns
    ///
    /// - `true`: Bit was set and has been successfully cleared (queue acquired)
    /// - `false`: Bit was not set (queue not ready or already acquired)
    ///
    /// # Use Cases
    ///
    /// ```ignore
    /// // Executor loop
    /// if signal.acquire(queue_bit) {
    ///     // Successfully acquired queue, process it
    ///     process_queue(queue_id);
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// ~10-20 ns (CAS loop, typically succeeds on first iteration)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::new();
    /// signal.set(5);
    /// assert!(signal.acquire(5));  // Successfully cleared bit 5
    /// assert!(!signal.acquire(5)); // Bit already clear, returns false
    /// ```
    #[inline(always)]
    pub fn acquire(&self, index: u64) -> bool {
        crate::bits::acquire(&self.inner.value, index)
    }

    /// Attempts to atomically clear a bit, returning detailed state information.
    ///
    /// Similar to `acquire()`, but provides additional information about the
    /// before/after state of the bitmap, useful for debugging or advanced scheduling.
    ///
    /// # Parameters
    ///
    /// - `index`: Bit position to clear (0-63)
    ///
    /// # Returns
    ///
    /// A tuple `(before, after, success)`:
    /// - `before`: Bitmap value before the operation
    /// - `after`: Bitmap value after the operation (if successful)
    /// - `success`: `true` if the bit was cleared, `false` if it wasn't set
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::with_value(0, 0b101010);  // Bits 1, 3, 5 set
    /// let (before, after, success) = signal.try_acquire(3);
    /// assert_eq!(before, 0b101010);
    /// assert_eq!(after, 0b100010);   // Bit 3 cleared
    /// assert!(success);
    /// ```
    #[inline(always)]
    pub fn try_acquire(&self, index: u64) -> (u64, u64, bool) {
        crate::bits::try_acquire(&self.inner.value, index)
    }

    /// Checks if a specific bit is set without modifying the bitmap.
    ///
    /// Non-atomic read followed by bit test. Suitable for non-critical checks
    /// where races are acceptable.
    ///
    /// # Parameters
    ///
    /// - `index`: Bit position to check (0-63)
    ///
    /// # Returns
    ///
    /// `true` if the bit is set, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let signal = Signal::new();
    /// assert!(!signal.is_set(5));
    /// signal.set(5);
    /// assert!(signal.is_set(5));
    /// ```
    #[inline(always)]
    pub fn is_set(&self, index: u64) -> bool {
        crate::bits::is_set(&self.inner.value, index)
    }
}

/// A cache-optimized waker that packs queue summaries and control flags into a single status word.
#[repr(align(64))]
pub struct AsyncSignalWaker {
    /// **Status bitmap**: Queue-word summary bits (0‒61) plus control flags.
    ///
    /// - Bits 0‒63: Queue-word hot bits (`mark_active`, `try_unmark_if_empty`, etc.)
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
}

impl AsyncSignalWaker {
    pub fn new() -> Self {
        Self {
            status: CachePadded::new(AtomicU64::new(0)),
            permits: CachePadded::new(AtomicU64::new(0)),
        }
    }

    // ────────────────────────────────────────────────────────────────────────────
    // PRODUCER-SIDE API
    // ────────────────────────────────────────────────────────────────────────────

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
        let summary_mask = mask;
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
        self.status.load(Ordering::Relaxed)
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
        let summary = self.status.load(Ordering::Relaxed);
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
        // TODO: Implement
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
        // TODO: Implement
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
        // TODO: Implement
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
        self.status.load(Ordering::Acquire)
    }

    /// Returns the current number of available permits.
    ///
    /// Useful for monitoring queue health or load. A high permit count may
    /// indicate consumers are falling behind.
    #[inline]
    pub fn permits(&self) -> u64 {
        self.permits.load(Ordering::Acquire)
    }
}

impl Default for AsyncSignalWaker {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// SignalGate State Machine Constants
// ──────────────────────────────────────────────────────────────────────────────

/// Queue has no work scheduled and is not being processed.
///
/// This is the initial state. Transitions to SCHEDULED when work is enqueued.
pub const IDLE: u8 = 0;

/// Queue has work available and is waiting for the executor to process it.
///
/// Transitions from IDLE when `schedule()` is called. Transitions to EXECUTING
/// when the executor calls `begin()`.
pub const SCHEDULED: u8 = 1;

/// Queue is currently being processed by the executor.
///
/// Transitions from SCHEDULED when executor calls `begin()`. Transitions back to
/// IDLE (via `finish()`) or SCHEDULED (via `finish_and_schedule()`) when processing completes.
pub const EXECUTING: u8 = 2;

/// Per-queue gate coordinating scheduling between producers and executor.
///
/// `SignalGate` implements a lock-free state machine that prevents redundant scheduling
/// and ensures proper handoff of work from producers to the executor. Each queue has
/// exactly one `SignalGate` instance.
///
/// # State Machine
///
/// ```text
/// ┌──────────────────────────────────────────────────────────────┐
/// │                                                              │
/// │  IDLE (0)  ──schedule()──▶  SCHEDULED (1)  ──begin()──▶  EXECUTING (2)
/// │     ▲                            │                           │
/// │     │                            │                           │
/// │     └────────finish()────────────┴───────────────────────────┘
/// │                                  │                           │
/// │                                  └──finish_and_schedule()────┘
/// │                                              │               │
/// │                                              ▼               │
/// │                                         SCHEDULED (1)        │
/// └──────────────────────────────────────────────────────────────┘
/// ```
///
/// # State Transitions
///
/// - **IDLE → SCHEDULED**: Producer calls `schedule()` after enqueuing items
/// - **SCHEDULED → EXECUTING**: Executor calls `begin()` before processing
/// - **EXECUTING → IDLE**: Executor calls `finish()` when done (queue empty)
/// - **EXECUTING → SCHEDULED**: Executor calls `finish_and_schedule()` when more work remains
/// - **Any → SCHEDULED**: Concurrent `schedule()` during EXECUTING sets flag, processed in `finish()`
///
/// # Concurrency Guarantees
///
/// - **Multiple producers**: Safe (atomic flags ensure only one schedule succeeds)
/// - **Producer + executor**: Safe (state transitions are atomic and properly ordered)
/// - **Multiple executors**: NOT SAFE (single-threaded consumption assumption)
///
/// # Integration with Signal and SignalWaker
///
/// When a queue transitions IDLE → SCHEDULED:
/// 1. Sets bit in the associated `Signal` (64-bit bitmap)
/// 2. If signal was empty, sets bit in `SignalWaker` summary (64-bit bitmap)
/// 3. May wake sleeping executor thread via permit system
///
/// # Memory Layout
///
/// ```text
/// SignalGate (40 bytes on x86_64)
/// ┌─────────────┬───────────┬─────────┬─────────┐
/// │ flags (1B)  │ bit_index │ signal  │ waker   │
/// │ AtomicU8    │ u64 (8B)  │ Arc (8B)│ Arc (8B)│
/// └─────────────┴───────────┴─────────┴─────────┘
/// ```
///
/// # Example Usage
///
/// ```ignore
/// // Setup
/// let waker = Arc::new(SignalWaker::new());
/// let signal = Signal::with_index(5);
/// let gate = SignalGate::new(10, signal, waker);
///
/// // Producer thread
/// queue.try_push(item)?;
/// gate.schedule();  // Signal work available
///
/// // Executor thread
/// gate.begin();     // Mark as executing
/// while let Some(item) = queue.try_pop() {
///     process(item);
/// }
/// if queue.is_empty() {
///     gate.finish();  // Done, back to IDLE
/// } else {
///     gate.finish_and_schedule();  // More work, stay SCHEDULED
/// }
/// ```
pub struct AsyncSignalGate {
    /// Atomic state flags (IDLE, SCHEDULED, EXECUTING).
    ///
    /// Uses bitwise OR to combine flags, allowing detection of concurrent schedules
    /// during execution (EXECUTING | SCHEDULED = 3).
    flags: AtomicU8,

    /// Bit position within the Signal's 64-bit bitmap (0-63).
    ///
    /// This queue's ready state is represented by bit `1 << bit_index` in the signal.
    bit_index: u8,

    /// Reference to the Signal word containing this queue's bit.
    ///
    /// Shared among up to 64 queues (all queues in the same signal group).
    signal: Signal,

    /// Reference to the top-level SignalWaker for summary updates.
    ///
    /// Shared among all queues in the executor (up to 4096 queues).
    waker: Arc<AsyncSignalWaker>,
}

impl AsyncSignalGate {
    /// Creates a new SignalGate in the IDLE state.
    ///
    /// # Parameters
    ///
    /// - `bit_index`: Position of this queue's bit within the signal (0-63)
    /// - `signal`: Reference to the Signal word containing this queue's bit
    /// - `waker`: Reference to the SignalWaker for summary updates
    ///
    /// # Example
    ///
    /// ```ignore
    /// let waker = Arc::new(SignalWaker::new());
    /// let signal = Signal::with_index(0);
    /// let gate = SignalGate::new(5, signal, waker);
    /// // This gate controls bit 5 in signal[0]
    /// ```
    pub fn new(bit_index: u8, signal: Signal, waker: Arc<AsyncSignalWaker>) -> Self {
        Self {
            flags: AtomicU8::new(IDLE),
            bit_index,
            signal,
            waker,
        }
    }

    /// Attempts to schedule this queue for execution (IDLE → SCHEDULED transition).
    ///
    /// Called by producers after enqueuing items to notify the executor. Uses atomic
    /// operations to ensure only one successful schedule per work batch.
    ///
    /// # Algorithm
    ///
    /// 1. **Fast check**: If already SCHEDULED, return false immediately (idempotent)
    /// 2. **Atomic set**: `fetch_or(SCHEDULED)` to set the SCHEDULED flag
    /// 3. **State check**: If previous state was IDLE (neither SCHEDULED nor EXECUTING):
    ///    - Set bit in signal word via `signal.set(bit_index)`
    ///    - If signal transitioned from empty, update summary via `waker.mark_active()`
    ///    - Return true (successful schedule)
    /// 4. **Otherwise**: Return false (already scheduled or executing)
    ///
    /// # Returns
    ///
    /// - `true`: Successfully transitioned from IDLE to SCHEDULED (work will be processed)
    /// - `false`: Already scheduled/executing, or concurrent schedule won (idempotent)
    ///
    /// # Concurrent Behavior
    ///
    /// - **Multiple producers**: Only the first `schedule()` succeeds (returns true)
    /// - **During EXECUTING**: Sets SCHEDULED flag, which `finish()` will detect and reschedule
    ///
    /// # Memory Ordering
    ///
    /// - Initial load: `Acquire` (see latest state)
    /// - `fetch_or`: `Release` (publish enqueued items to executor)
    ///
    /// # Performance
    ///
    /// - **Already scheduled**: ~2-3 ns (fast path, single atomic load)
    /// - **Successful schedule**: ~10-20 ns (fetch_or + signal update + potential summary update)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Producer 1
    /// queue.try_push(item)?;
    /// if gate.schedule() {
    ///     println!("Successfully scheduled");  // First producer
    /// }
    ///
    /// // Producer 2 (concurrent)
    /// queue.try_push(another_item)?;
    /// if !gate.schedule() {
    ///     println!("Already scheduled");  // Idempotent, no action needed
    /// }
    /// ```
    #[inline(always)]
    pub fn schedule(&self) -> bool {
        if (self.flags.load(Ordering::Acquire) & SCHEDULED) != IDLE {
            return false;
        }

        let previous_flags = self.flags.fetch_or(SCHEDULED, Ordering::Release);
        let scheduled_nor_executing = (previous_flags & (SCHEDULED | EXECUTING)) == IDLE;

        if scheduled_nor_executing {
            let (was_empty, was_set) = self.signal.set(self.bit_index as u64);
            if was_empty && was_set {
                self.waker.mark_active(self.signal.index());
            }
            true
        } else {
            false
        }
    }

    /// Marks the queue as EXECUTING (SCHEDULED → EXECUTING transition).
    ///
    /// Called by the executor when it begins processing this queue. This transition
    /// prevents redundant scheduling while work is being processed.
    ///
    /// # State Transition
    ///
    /// Unconditionally stores EXECUTING, which clears any SCHEDULED flags and sets EXECUTING.
    /// ```text
    /// Before: SCHEDULED (1)
    /// After:  EXECUTING (2)
    /// ```
    ///
    /// If a producer calls `schedule()` after `begin()` but before `finish()`, the
    /// SCHEDULED flag will be set again (creating state 3 = EXECUTING | SCHEDULED),
    /// which `finish()` detects and handles.
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::Release` to ensure the state change is visible to concurrent
    /// producers calling `schedule()`.
    ///
    /// # Performance
    ///
    /// ~1-2 ns (single atomic store)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Executor discovers ready queue
    /// if signal.acquire(queue_bit) {
    ///     gate.begin();  // Mark as executing
    ///     process_queue();
    ///     gate.finish();
    /// }
    /// ```
    #[inline(always)]
    pub fn mark(&self) {
        self.flags.store(EXECUTING, Ordering::Release);
    }

    /// Marks the queue as IDLE and handles concurrent schedules (EXECUTING → IDLE/SCHEDULED).
    ///
    /// Called by the executor after processing a batch of items. Automatically detects
    /// if new work arrived during processing (SCHEDULED flag set concurrently) and
    /// reschedules if needed.
    ///
    /// # Algorithm
    ///
    /// 1. **Clear EXECUTING**: `fetch_sub(EXECUTING)` atomically transitions to IDLE
    /// 2. **Check SCHEDULED**: If the SCHEDULED flag is set in the result:
    ///    - Means a producer called `schedule()` during execution
    ///    - Re-set the signal bit to ensure executor sees the work
    ///    - Queue remains/becomes SCHEDULED
    ///
    /// # Automatic Rescheduling
    ///
    /// This method implements a key correctness property: if a producer enqueues work
    /// while the executor is processing, that work will not be lost. The SCHEDULED flag
    /// acts as a handoff mechanism.
    ///
    /// ```text
    /// Timeline:
    /// T0: Executor calls begin()           → EXECUTING (2)
    /// T1: Producer calls schedule()        → EXECUTING | SCHEDULED (3)
    /// T2: Executor calls finish()          → SCHEDULED (1) [bit re-set in signal]
    /// T3: Executor sees bit, processes     → ...
    /// ```
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::AcqRel`:
    /// - **Acquire**: See all producer writes (enqueued items)
    /// - **Release**: Publish state transition to future readers
    ///
    /// # Performance
    ///
    /// - **No concurrent schedule**: ~2-3 ns (fetch_sub only)
    /// - **With concurrent schedule**: ~10-15 ns (fetch_sub + signal.set)
    ///
    /// # Example
    ///
    /// ```ignore
    /// gate.begin();
    /// while let Some(item) = queue.try_pop() {
    ///     process(item);
    /// }
    /// gate.finish();  // Automatically reschedules if more work arrived
    /// ```
    #[inline(always)]
    pub fn unmark(&self) {
        let after_flags = self.flags.fetch_sub(EXECUTING, Ordering::AcqRel);
        if after_flags & SCHEDULED != IDLE {
            self.signal.set(self.bit_index as u64);
        }
    }

    /// Atomically marks the queue as SCHEDULED, ensuring re-execution.
    ///
    /// Called by the executor when it knows more work exists but wants to yield the
    /// timeslice for fairness. This is an optimization over `finish()` followed by
    /// external `schedule()`.
    ///
    /// # Use Cases
    ///
    /// 1. **Batch size limiting**: Process N items, then yield to other queues
    /// 2. **Fairness**: Prevent queue starvation by rotating execution
    /// 3. **Latency control**: Ensure all queues get regular timeslices
    ///
    /// # Algorithm
    ///
    /// 1. **Set state**: Store SCHEDULED unconditionally
    /// 2. **Update signal**: Set bit in signal word
    /// 3. **Update summary**: If signal was empty, mark active in waker
    ///
    /// # Comparison with finish() + schedule()
    ///
    /// ```ignore
    /// // Separate calls (2 atomic ops)
    /// gate.finish();      // EXECUTING → IDLE
    /// gate.schedule();    // IDLE → SCHEDULED
    ///
    /// // Combined call (1 atomic op + signal update)
    /// gate.finish_and_schedule();  // EXECUTING → SCHEDULED
    /// ```
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::Release` to publish both the state change and enqueued items.
    ///
    /// # Performance
    ///
    /// ~10-15 ns (store + signal.set + potential summary update)
    ///
    /// # Example
    ///
    /// ```ignore
    /// gate.begin();
    /// let mut processed = 0;
    /// while processed < BATCH_SIZE {
    ///     if let Some(item) = queue.try_pop() {
    ///         process(item);
    ///         processed += 1;
    ///     } else {
    ///         break;
    ///     }
    /// }
    ///
    /// if queue.len() > 0 {
    ///     gate.finish_and_schedule();  // More work, stay scheduled
    /// } else {
    ///     gate.finish();  // Done, go idle
    /// }
    /// ```
    #[inline(always)]
    pub fn unmark_and_schedule(&self) {
        self.flags.store(SCHEDULED, Ordering::Release);
        let (was_empty, was_set) = self.signal.set(self.bit_index as u64);
        if was_empty && was_set {
            self.waker.mark_active(self.signal.index());
        }
    }
}

impl crate::spsc::SignalSchedule for AsyncSignalGate {
    fn schedule(&self) -> bool {
        self.schedule()
    }

    fn mark(&self) {
        self.mark();
    }

    fn unmark(&self) {
        self.unmark();
    }

    fn unmark_and_schedule(&self) {
        self.unmark_and_schedule();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
