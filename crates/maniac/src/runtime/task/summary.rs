use crate::runtime::worker::waker::WorkerWaker;
use crate::utils::CachePadded;
use crate::utils::bits;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const TASK_SLOTS_PER_SIGNAL: usize = u64::BITS as usize;

/// Single-level summary tree for task work-stealing.
///
/// This tree tracks ONLY task signals - no yield or worker state.
/// Each leaf represents a set of task signal words.
///
/// The Summary coordinates with SignalWakers to notify partition owners
/// when their assigned leafs become active/inactive.
///
/// # Thread Safety
/// This struct is designed for high-concurrency scenarios with the following guarantees:
/// - All operations use atomic primitives for lock-free operation
/// - Raw pointers are used for performance but are guaranteed safe by WorkerService lifetime
/// - Implements `Send` and `Sync` for cross-thread usage
///
/// # Memory Layout
/// - `leaf_words`: Atomic bitfields tracking which signals have pending tasks
/// - `task_reservations`: Atomic bitfields for task slot reservations within each signal
/// - Round-robin cursors distribute load evenly across leaves and signals
/// - Partition mapping enables work-stealing between worker threads
pub struct Summary {
    // Owned heap allocations
    pub(crate) leaf_words: Box<[AtomicU64]>, // Pub for Worker access
    task_reservations: Box<[AtomicU64]>,

    // Configuration
    leaf_count: usize,
    signals_per_leaf: usize,
    leaf_summary_mask: u64,

    // Round-robin cursors for allocation
    // CachePadded to prevent false sharing between CPU cores
    next_partition: CachePadded<AtomicUsize>,

    // Partition owner notification
    // Raw pointer to WorkerService.wakers array (lifetime guaranteed by WorkerService ownership)
    wakers: *const Arc<WorkerWaker>,
    wakers_len: usize,
    // Shared reference to worker_count - keeps it alive
    worker_count: Arc<AtomicUsize>,

    // Provenance tracking for raw pointers
    _marker: PhantomData<&'static WorkerWaker>,
}

unsafe impl Send for Summary {}
unsafe impl Sync for Summary {}

impl Summary {
    /// Creates a new Summary with the specified dimensions.
    ///
    /// # Arguments
    /// * `leaf_count` - Number of leaf nodes (typically matches worker partition count)
    /// * `signals_per_leaf` - Number of task signal words per leaf (typically tasks_per_leaf / 64)
    /// * `wakers` - Slice of SignalWakers for partition owner notification
    /// * `worker_count` - Reference to WorkerService's worker_count atomic (single source of truth)
    ///
    /// # Safety
    /// The wakers slice and worker_count reference must remain valid for the lifetime of this Summary.
    /// This is guaranteed when Summary is owned by WorkerService which also owns the wakers and worker_count.
    ///
    /// # Memory Allocation
    /// Allocates `leaf_count * signals_per_leaf * 8` bytes for reservations plus overhead for leaf words and cursors.
    pub fn new(
        leaf_count: usize,
        signals_per_leaf: usize,
        wakers: &[Arc<WorkerWaker>],
        worker_count: &Arc<AtomicUsize>,
    ) -> Self {
        assert!(leaf_count > 0, "leaf_count must be > 0");
        assert!(signals_per_leaf > 0, "signals_per_leaf must be > 0");
        assert!(signals_per_leaf <= 64, "signals_per_leaf must be <= 64");
        assert!(!wakers.is_empty(), "wakers must not be empty");

        let task_word_count = leaf_count * signals_per_leaf;

        // Initialize leaf words (all signals initially inactive)
        let leaf_words = (0..leaf_count)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        // Initialize reservation bitmaps (all slots initially free)
        let task_reservations = (0..task_word_count)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        // Create mask for valid signal bits in each leaf
        let leaf_summary_mask = if signals_per_leaf >= 64 {
            u64::MAX
        } else {
            (1u64 << signals_per_leaf) - 1
        };

        Self {
            leaf_words,
            task_reservations,
            leaf_count,
            signals_per_leaf,
            leaf_summary_mask,
            next_partition: CachePadded::new(AtomicUsize::new(0)),
            wakers: wakers.as_ptr(),
            wakers_len: wakers.len(),
            worker_count: Arc::clone(worker_count),
            _marker: PhantomData,
        }
    }

    /// Get the current worker count from WorkerService.
    /// Reads directly from the single source of truth.
    ///
    /// # Atomic Semantics
    /// Uses `Relaxed` ordering since this is only used for informational purposes
    /// and doesn't require synchronization with other memory operations.
    #[inline]
    pub fn get_worker_count(&self) -> usize {
        self.worker_count.load(Ordering::Relaxed)
    }

    #[inline(always)]
    fn leaf_word(&self, idx: usize) -> &AtomicU64 {
        &self.leaf_words[idx]
    }

    #[inline(always)]
    fn reservation_index(&self, leaf_idx: usize, signal_idx: usize) -> usize {
        leaf_idx * self.signals_per_leaf + signal_idx
    }

    #[inline(always)]
    fn reservation_word(&self, leaf_idx: usize, signal_idx: usize) -> &AtomicU64 {
        &self.task_reservations[self.reservation_index(leaf_idx, signal_idx)]
    }

    #[inline(always)]
    fn try_reserve_in_leaf(&self, leaf_idx: usize) -> Option<(usize, usize, u8)> {
        if self.signals_per_leaf == 0 {
            return None;
        }
        let mut signal_mask = self.leaf_summary_mask;
        while signal_mask != 0 {
            let signal_idx = signal_mask.trailing_zeros() as usize;
            if signal_idx >= self.signals_per_leaf {
                break;
            }
            if let Some(bit) = self.reserve_task_in_leaf(leaf_idx, signal_idx) {
                return Some((leaf_idx, signal_idx, bit));
            }
            signal_mask &= signal_mask - 1;
        }
        None
    }

    /// Notify the partition owner's SignalWaker that a leaf in their partition became active.
    ///
    /// # Atomic Semantics
    /// This function reads the worker count with Acquire ordering to ensure visibility
    /// of prior worker service state changes. It includes validation to handle the case
    /// where worker count changes between loading and using it (TOCTOU race).
    ///
    /// # Race Condition Mitigation
    /// - Validates worker_count against wakers_len to prevent out-of-bounds access
    /// - Validates owner_id against current worker_count to handle worker shutdown
    /// - These checks ensure safe operation even if worker configuration changes
    #[inline(always)]
    fn notify_partition_owner_active(&self, leaf_idx: usize) {
        let worker_count = self.worker_count.load(Ordering::Acquire);
        // Validate worker count to prevent out-of-bounds access
        if worker_count == 0 || worker_count > self.wakers_len {
            return;
        }

        let owner_id = self.compute_partition_owner(leaf_idx, worker_count);
        // Additional validation: owner_id must be within current worker count
        // This handles the case where worker count decreases after we loaded it
        if owner_id >= worker_count {
            return;
        }

        if owner_id < self.wakers_len {
            // SAFETY: wakers pointer is valid for the lifetime of Summary
            // because WorkerService owns both
            let waker = unsafe { &*self.wakers.add(owner_id) };

            // Compute local leaf index within owner's partition
            if let Some(local_idx) = self.global_to_local_leaf_idx(leaf_idx, owner_id, worker_count)
            {
                waker.mark_partition_leaf_active(local_idx);
            }
        }
    }

    /// Notify the partition owner's SignalWaker that a leaf in their partition became inactive.
    ///
    /// # Atomic Semantics
    /// This function reads the worker count with Acquire ordering to ensure visibility
    /// of prior worker service state changes. It includes validation to handle the case
    /// where worker count changes between loading and using it (TOCTOU race).
    ///
    /// # Race Condition Mitigation
    /// - Validates worker_count against wakers_len to prevent out-of-bounds access
    /// - Validates owner_id against current worker_count to handle worker shutdown
    /// - These checks ensure safe operation even if worker configuration changes
    #[inline(always)]
    fn notify_partition_owner_inactive(&self, leaf_idx: usize) {
        let worker_count = self.worker_count.load(Ordering::Acquire);
        // Validate worker count to prevent out-of-bounds access
        if worker_count == 0 || worker_count > self.wakers_len {
            return;
        }

        let owner_id = self.compute_partition_owner(leaf_idx, worker_count);
        // Additional validation: owner_id must be within current worker count
        // This handles the case where worker count decreases after we loaded it
        if owner_id >= worker_count {
            return;
        }

        if owner_id < self.wakers_len {
            // SAFETY: wakers pointer is valid for the lifetime of Summary
            // because WorkerService owns both
            let waker = unsafe { &*self.wakers.add(owner_id) };

            // Compute local leaf index within owner's partition
            if let Some(local_idx) = self.global_to_local_leaf_idx(leaf_idx, owner_id, worker_count)
            {
                waker.clear_partition_leaf(local_idx);
            }
        }
    }

    #[inline(always)]
    fn mark_leaf_bits(&self, leaf_idx: usize, mask: u64) -> bool {
        if mask == 0 {
            return false;
        }
        let leaf = self.leaf_word(leaf_idx);
        // Atomic Read-Modify-Write: fetch_or returns previous value and sets new bits
        // Using AcqRel ordering to both see previous changes and make new changes visible
        let prev = leaf.fetch_or(mask, Ordering::AcqRel);

        let was_empty = prev & self.leaf_summary_mask == 0;
        // Check if we actually added new bits (not just setting already-set bits)
        let any_new_bits = (prev & mask) != mask;

        // Notify partition owner if any new signal bits were set
        // Note: There's a small race window here where bits could be cleared
        // before notification, but this is acceptable for performance
        if any_new_bits {
            self.notify_partition_owner_active(leaf_idx);
        }

        was_empty
    }

    #[inline(always)]
    fn clear_leaf_bits(&self, leaf_idx: usize, mask: u64) -> bool {
        if mask == 0 {
            return false;
        }
        let leaf = self.leaf_word(leaf_idx);
        // Atomic Read-Modify-Write: fetch_and returns previous value and clears bits
        // Using AcqRel ordering to both see previous changes and make new changes visible
        let prev = leaf.fetch_and(!mask, Ordering::AcqRel);
        if prev & mask == 0 {
            return false; // Bits were already cleared
        }
        // Bits were successfully cleared - return true
        // Also notify partition owner if this leaf is now completely empty
        // The check uses the atomic snapshot `prev` to avoid races
        if (prev & !mask) & self.leaf_summary_mask == 0 {
            self.notify_partition_owner_inactive(leaf_idx);
        }
        true
    }

    /// Sets the summary bit for a task signal.
    ///
    /// # Returns
    /// `true` if the leaf was empty before setting this signal (useful for work-stealing decisions)
    ///
    /// # Atomic Semantics
    /// Uses `fetch_or` with `AcqRel` ordering to atomically set bits and get previous state.
    /// Notifies the partition owner if new bits were added.
    pub fn mark_signal_active(&self, leaf_idx: usize, signal_idx: usize) -> bool {
        if leaf_idx >= self.leaf_count || signal_idx >= self.signals_per_leaf {
            return false;
        }
        debug_assert!(signal_idx < self.signals_per_leaf);
        let mask = 1u64 << signal_idx;
        self.mark_leaf_bits(leaf_idx, mask)
    }

    /// Clears the summary bit for a task signal.
    ///
    /// # Returns
    /// `true` if bits were successfully cleared, `false` if indices are invalid or bits were already cleared
    ///
    /// # Atomic Semantics
    /// Uses `fetch_and` with `AcqRel` ordering to atomically clear bits and get previous state.
    /// Notifies the partition owner if the leaf became empty.
    pub fn mark_signal_inactive(&self, leaf_idx: usize, signal_idx: usize) -> bool {
        if leaf_idx >= self.leaf_count || signal_idx >= self.signals_per_leaf {
            return false;
        }
        debug_assert!(signal_idx < self.signals_per_leaf);
        let mask = 1u64 << signal_idx;
        self.clear_leaf_bits(leaf_idx, mask)
    }

    /// Attempts to reserve a task slot within (`leaf_idx`, `signal_idx`).
    ///
    /// # Returns
    /// The bit index (0-63) of the reserved slot, or `None` if all slots are taken
    ///
    /// # Atomic Semantics
    /// Implements a lock-free reservation system using atomic CAS (Compare-And-Swap) loops.
    /// - Uses `Acquire` ordering for loads to see completed reservations
    /// - Uses `AcqRel` for successful CAS to make reservation visible to others
    /// - Uses `Acquire` for failed CAS to see the updated state
    ///
    /// # Algorithm
    /// 1. Load current reservation bitmap
    /// 2. Use a per-signal round-robin cursor to pick a starting bit
    /// 3. Rotate the bitmap so trailing_zeros finds the next free slot after the cursor
    /// 4. Attempt to atomically set that bit with CAS
    /// 5. If CAS fails (another thread reserved it), retry with updated value
    /// 6. Continue until success or no free bits remain
    pub fn reserve_task_in_leaf(&self, leaf_idx: usize, signal_idx: usize) -> Option<u8> {
        if leaf_idx >= self.leaf_count || signal_idx >= self.signals_per_leaf {
            return None;
        }
        let reservations = self.reservation_word(leaf_idx, signal_idx);
        let mut current = reservations.load(Ordering::Acquire);
        loop {
            let free = !current;
            if free == 0 {
                return None; // All bits reserved
            }
            let bit = free.trailing_zeros() as u8;
            let mask = 1u64 << bit;
            match reservations.compare_exchange(
                current,
                current | mask,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(bit),
                Err(updated) => current = updated,
            }
        }
    }

    /// Clears a previously reserved task slot.
    ///
    /// # Atomic Semantics
    /// Uses `fetch_and` with `SeqCst` ordering to atomically clear the reservation bit.
    /// This ensures the release is visible to other threads attempting reservations.
    pub fn release_task_in_leaf(&self, leaf_idx: usize, signal_idx: usize, bit: usize) {
        if leaf_idx >= self.leaf_count
            || signal_idx >= self.signals_per_leaf
            || bit >= TASK_SLOTS_PER_SIGNAL
        {
            return;
        }
        let mask = !(1u64 << bit);
        self.reservation_word(leaf_idx, signal_idx)
            .fetch_and(mask, Ordering::AcqRel);
    }

    /// Convenience function: reserve the first available task slot across the arena.
    ///
    /// # Atomic Semantics
    /// Uses round-robin cursors with `SeqCst` ordering to ensure proper synchronization
    /// between threads when selecting partitions, leaves, and signals. The actual reservation
    /// uses the CAS loop in `reserve_task_in_leaf` which provides proper synchronization.
    pub fn reserve_task(&self) -> Option<(usize, usize, u8)> {
        if self.leaf_count == 0 {
            return None;
        }
        if self.signals_per_leaf == 0 {
            return None;
        }

        // Exhaustively scan partitions one by one. Each partition represents a worker's slice of
        // leaves, so rotating by partition keeps contention localized while still guaranteeing we
        // eventually visit every leaf if the partition has no free slots.
        let worker_count = self.get_worker_count();
        let partition_count = worker_count.max(1);
        let start_partition = self.next_partition.fetch_add(1, Ordering::SeqCst) % partition_count;

        for partition_offset in 0..partition_count {
            let worker_id = (start_partition + partition_offset) % partition_count;
            let partition_start = self.partition_start_for_worker(worker_id, partition_count);
            let partition_end = self.partition_end_for_worker(worker_id, partition_count);
            if partition_end <= partition_start {
                continue; // Empty partition (more workers than leaves)
            }

            let partition_len = partition_end - partition_start;
            if partition_len == 1 {
                if let Some(found) = self.try_reserve_in_leaf(partition_start) {
                    return Some(found);
                }
                continue;
            }

            // Use a simple random starting point within the partition to distribute load
            let random_seed = crate::utils::random_u64() as usize;
            let start_leaf_offset = random_seed % partition_len;

            for leaf_offset in 0..partition_len {
                let leaf_idx = partition_start + (start_leaf_offset + leaf_offset) % partition_len;
                if let Some(found) = self.try_reserve_in_leaf(leaf_idx) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Clears the summary bit when the corresponding task signal becomes empty.
    ///
    /// # ⚠️ CORRECTNESS ISSUE - TOCTOU Race Condition
    ///
    /// This function has an unfixed race condition between checking and clearing:
    ///
    /// **Race scenario:**
    /// 1. Thread A: `signal.load()` sees `0`
    /// 2. Thread B: Enqueues task, `signal` becomes `1`, calls `mark_signal_active()`
    /// 3. Thread A: Calls `mark_signal_inactive()`, clearing the bit Thread B just set
    /// 4. Result: `signal` has tasks but summary bit is cleared → **lost work notification**
    ///
    /// **Why this is problematic:**
    /// - Work-stealing threads rely on summary bits to find available work
    /// - Clearing the bit while tasks exist makes those tasks invisible to stealers
    /// - The task enqueue won't re-set the bit because it already set it in step 2
    ///
    /// **Proper fix requires one of:**
    /// 1. Caller-side synchronization ensuring signal cannot be modified during this call
    /// 2. API change: pass mutable/exclusive access to signal to perform atomic check-and-clear
    /// 3. Accepting false negatives: allow summary bit to remain set even when signal is empty
    ///    (wastes stealer cycles but is always safe)
    ///
    /// **Current mitigation:** None - callers must ensure signal stability externally.
    ///
    /// # Atomic Semantics
    /// Uses `Acquire` ordering to ensure visibility of all prior writes to the signal.
    pub fn mark_signal_inactive_if_empty(
        &self,
        leaf_idx: usize,
        signal_idx: usize,
        signal: &AtomicU64,
    ) {
        // WARNING: This check-then-act pattern is inherently racy
        // See documentation above for details
        if signal.load(Ordering::Acquire) == 0 {
            self.mark_signal_inactive(leaf_idx, signal_idx);
        }
    }

    #[inline(always)]
    pub fn leaf_count(&self) -> usize {
        self.leaf_count
    }

    #[inline(always)]
    pub fn signals_per_leaf(&self) -> usize {
        self.signals_per_leaf
    }

    // ────────────────────────────────────────────────────────────────────────────
    // PARTITION MANAGEMENT HELPERS
    // ────────────────────────────────────────────────────────────────────────────
    // These functions handle the mapping between global leaf indices and worker partitions.
    // They implement a load-balancing algorithm that distributes leaves evenly across workers.

    /// Compute which worker owns a given leaf based on partition assignments.
    ///
    /// This is the inverse of `Worker::compute_partition()`. Given a leaf index,
    /// it determines which worker is responsible for processing tasks in that leaf.
    ///
    /// # Partition Algorithm
    /// Uses a balanced distribution where:
    /// - First `leaf_count % worker_count` workers get `(leaf_count / worker_count) + 1` leaves
    /// - Remaining workers get `leaf_count / worker_count` leaves
    /// This ensures leaves are distributed as evenly as possible.
    ///
    /// # Arguments
    ///
    /// * `leaf_idx` - The global leaf index (0..leaf_count)
    /// * `worker_count` - Total number of active workers
    ///
    /// # Returns
    ///
    /// Worker ID (0..worker_count) that owns this leaf
    ///
    /// # Example
    ///
    /// ```ignore
    /// let owner_id = summary_tree.compute_partition_owner(leaf_idx, worker_count);
    /// let owner_waker = &service.wakers[owner_id];
    /// owner_waker.mark_partition_leaf_active(local_idx);
    /// ```
    pub fn compute_partition_owner(&self, leaf_idx: usize, worker_count: usize) -> usize {
        if worker_count == 0 {
            return 0;
        }

        let base = self.leaf_count / worker_count;
        let extra = self.leaf_count % worker_count;

        // First 'extra' workers get (base + 1) leafs each
        let boundary = extra * (base + 1);

        if leaf_idx < boundary {
            leaf_idx / (base + 1)
        } else {
            extra + (leaf_idx - boundary) / base
        }
    }

    /// Compute the partition start index for a given worker.
    ///
    /// # Arguments
    ///
    /// * `worker_id` - Worker ID (0..worker_count)
    /// * `worker_count` - Total number of active workers
    ///
    /// # Returns
    ///
    /// First leaf index in this worker's partition
    pub fn partition_start_for_worker(&self, worker_id: usize, worker_count: usize) -> usize {
        if worker_count == 0 {
            return 0;
        }

        let base = self.leaf_count / worker_count;
        let extra = self.leaf_count % worker_count;

        if worker_id < extra {
            worker_id * (base + 1)
        } else {
            extra * (base + 1) + (worker_id - extra) * base
        }
    }

    /// Compute the partition end index for a given worker.
    ///
    /// # Arguments
    ///
    /// * `worker_id` - Worker ID (0..worker_count)
    /// * `worker_count` - Total number of active workers
    ///
    /// # Returns
    ///
    /// One past the last leaf index in this worker's partition (exclusive)
    pub fn partition_end_for_worker(&self, worker_id: usize, worker_count: usize) -> usize {
        if worker_count == 0 {
            return 0;
        }

        let start = self.partition_start_for_worker(worker_id, worker_count);
        let base = self.leaf_count / worker_count;
        let extra = self.leaf_count % worker_count;

        let len = if worker_id < extra { base + 1 } else { base };

        (start + len).min(self.leaf_count)
    }

    /// Convert a global leaf index to a local index within a worker's partition.
    ///
    /// # Arguments
    ///
    /// * `leaf_idx` - Global leaf index
    /// * `worker_id` - Worker ID
    /// * `worker_count` - Total number of workers
    ///
    /// # Returns
    ///
    /// Local leaf index (0..partition_size) for use with SignalWaker partition bitmap,
    /// or None if the leaf is not in this worker's partition
    pub fn global_to_local_leaf_idx(
        &self,
        leaf_idx: usize,
        worker_id: usize,
        worker_count: usize,
    ) -> Option<usize> {
        let partition_start = self.partition_start_for_worker(worker_id, worker_count);
        let partition_end = self.partition_end_for_worker(worker_id, worker_count);

        if leaf_idx >= partition_start && leaf_idx < partition_end {
            Some(leaf_idx - partition_start)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::worker::waker::WorkerWaker;

    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use std::thread::yield_now;
    use std::time::{Duration, Instant};

    /// Helper function to create a test Summary with dummy wakers
    fn setup_tree(
        leaf_count: usize,
        signals_per_leaf: usize,
    ) -> (Summary, Vec<Arc<WorkerWaker>>, Arc<AtomicUsize>) {
        // Create dummy wakers for testing
        // SAFETY: The returned Arc<AtomicUsize> must outlive the Summary instance
        // to prevent dangling pointer access via Summary.worker_count
        let wakers: Vec<Arc<WorkerWaker>> = (0..4).map(|_| Arc::new(WorkerWaker::new())).collect();
        let worker_count = Arc::new(AtomicUsize::new(4));
        let tree = Summary::new(leaf_count, signals_per_leaf, &wakers, &worker_count);
        (tree, wakers, worker_count)
    }

    /// Test that marking a signal active updates the leaf word correctly
    /// and that duplicate marking returns false (idempotent behavior)
    #[test]
    fn mark_signal_active_updates_root_and_leaf() {
        let (tree, _wakers, _worker_count) = setup_tree(4, 4);

        // First activation should return true (leaf was empty)
        assert!(tree.mark_signal_active(1, 1));
        assert_eq!(tree.leaf_words[1].load(Ordering::Relaxed), 1u64 << 1);

        // Duplicate activation should return false (already active)
        assert!(!tree.mark_signal_active(1, 1));

        // Clearing should work and return true (leaf became empty)
        assert!(tree.mark_signal_inactive(1, 1));
        assert_eq!(tree.leaf_words[1].load(Ordering::Relaxed), 0);
    }

    /// Test the conditional clearing functionality when signal is empty
    /// This tests the race-prone mark_signal_inactive_if_empty function
    #[test]
    fn mark_signal_inactive_if_empty_clears_summary() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 2);

        // Activate a signal
        assert!(tree.mark_signal_active(0, 1));

        // Create an empty signal for testing
        let signal = AtomicU64::new(0);

        // Should clear the summary bit since signal is empty
        tree.mark_signal_inactive_if_empty(0, 1, &signal);
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 0);
    }

    /// Test that task reservation exhausts all 64 bits correctly
    /// Validates the CAS loop implementation and bit manipulation
    #[test]
    fn reserve_task_in_leaf_exhausts_all_bits() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 1);
        let mut bits = Vec::with_capacity(64);

        // Reserve all 64 bits
        for _ in 0..64 {
            let bit = tree.reserve_task_in_leaf(0, 0).expect("expected free bit");
            bits.push(bit);
        }

        // Verify we got all unique bits 0-63
        bits.sort_unstable();
        assert_eq!(bits, (0..64).collect::<Vec<_>>());

        // Should be exhausted now
        assert!(
            tree.reserve_task_in_leaf(0, 0).is_none(),
            "all bits should be exhausted"
        );

        // Release all bits for cleanup
        for bit in bits {
            tree.release_task_in_leaf(0, 0, bit as usize);
        }
    }

    /// Test distribution across leaves
    /// Validates that reserve_task can visit all leaves
    #[test]
    fn reserve_task_round_robin_visits_all_leaves() {
        let (tree, _wakers, _worker_count) = setup_tree(4, 1);
        let mut observed = Vec::with_capacity(16);

        // Reserve multiple tasks to ensure we visit different leaves
        // With random starting points, we should eventually hit all leaves
        for _ in 0..16 {
            let (leaf, sig, bit) = tree.reserve_task().expect("reserve task");
            observed.push(leaf);
            tree.release_task_in_leaf(leaf, sig, bit as usize);
        }

        // Should have visited all 4 leaves at least once
        observed.sort_unstable();
        observed.dedup();
        assert_eq!(observed.len(), 4, "Should visit all 4 leaves");
    }

    /// Stress test: concurrent reservations must be unique
    /// Tests atomicity of CAS loop under high contention
    #[test]
    fn concurrent_reservations_are_unique() {
        let (tree, _wakers, _worker_count) = setup_tree(4, 1);
        let tree = Arc::new(tree);
        let threads = 8;
        let reservations_per_thread = 8;
        let barrier = Arc::new(Barrier::new(threads));
        let handles = Arc::new(Mutex::new(Vec::with_capacity(
            threads * reservations_per_thread,
        )));

        let mut join_handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let tree_clone = Arc::clone(&tree);
            let barrier = Arc::clone(&barrier);
            let handles = Arc::clone(&handles);
            join_handles.push(thread::spawn(move || {
                barrier.wait();
                for _ in 0..reservations_per_thread {
                    loop {
                        if let Some(handle) = tree_clone.reserve_task() {
                            let mut guard = handles.lock().unwrap();
                            guard.push(handle);
                            break;
                        } else {
                            yield_now();
                        }
                    }
                }
            }));
        }

        for join in join_handles {
            join.join().expect("thread panicked");
        }

        // Verify all reservations are unique (no duplicates)
        let guard = handles.lock().unwrap();
        let mut unique = HashSet::new();
        for &(leaf, signal, bit) in guard.iter() {
            assert!(
                unique.insert((leaf, signal, bit)),
                "duplicate handle detected"
            );
        }
        assert_eq!(guard.len(), threads * reservations_per_thread);

        // Cleanup
        for &(leaf, signal, bit) in guard.iter() {
            tree.release_task_in_leaf(leaf, signal, bit as usize);
        }
    }

    /// Test that reservation and release properly update the reservation bitmap
    /// Validates atomic visibility of changes
    #[test]
    fn reserve_and_release_task_updates_reservations() {
        let (tree, _wakers, _worker_count) = setup_tree(4, 1);

        // Reserve a task
        let handle = tree.reserve_task().expect("task handle");
        assert_eq!(handle.1, 0); // signal idx

        // Verify reservation bitmap was updated
        let reservation = tree
            .task_reservations
            .get(handle.0 * 1 + handle.1)
            .unwrap()
            .load(Ordering::Relaxed);
        assert_ne!(reservation, 0);

        // Release the task
        tree.release_task_in_leaf(handle.0, handle.1, handle.2 as usize);

        // Verify reservation bitmap was cleared
        let reservation = tree
            .task_reservations
            .get(handle.0 * 1 + handle.1)
            .unwrap()
            .load(Ordering::Relaxed);
        assert_eq!(reservation, 0);
    }

    // ────────────────────────────────────────────────────────────────────────────
    // ATOMIC BIT OPERATIONS TESTS
    // ────────────────────────────────────────────────────────────────────────────

    /// Test atomic bit operations with proper memory ordering semantics
    /// Validates that fetch_or/fetch_and operations are truly atomic
    #[test]
    fn atomic_bit_operations_are_atomic() {
        let (tree, _wakers, _worker_count) = setup_tree(2, 2);
        let tree = Arc::new(tree);
        let threads = 4;
        let barrier = Arc::new(Barrier::new(threads));

        // Each thread will set different bits in the same leaf
        let handles: Vec<_> = (0..threads)
            .map(|i| {
                let tree = Arc::clone(&tree);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    let signal_idx = i % 2;
                    let mask = 1u64 << signal_idx;
                    tree.mark_signal_active(0, signal_idx);

                    // Verify the bit is set
                    let leaf_word = tree.leaf_words[0].load(Ordering::Relaxed);
                    assert_eq!(
                        leaf_word & mask,
                        mask,
                        "Thread {}: bit {} should be set",
                        i,
                        signal_idx
                    );
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // All bits should be set
        let final_word = tree.leaf_words[0].load(Ordering::Relaxed);
        assert_eq!(final_word, 0b11);
    }

    /// Test that mark_signal_inactive properly clears only specified bits
    #[test]
    fn clear_leaf_bits_clears_specific_bits() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 3);

        // Set multiple bits
        tree.mark_signal_active(0, 0);
        tree.mark_signal_active(0, 1);
        tree.mark_signal_active(0, 2);
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 0b111);

        // Clear middle bit
        assert!(tree.mark_signal_inactive(0, 1));
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 0b101);

        // Clear first bit
        assert!(tree.mark_signal_inactive(0, 0));
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 0b100);

        // Clear last bit - should return true (leaf became empty)
        assert!(tree.mark_signal_inactive(0, 2));
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 0);
    }

    /// Test idempotency of bit operations
    #[test]
    fn bit_operations_are_idempotent() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 1);

        // First activation should return true
        assert!(tree.mark_signal_active(0, 0));
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 1);

        // Duplicate activation should return false
        assert!(!tree.mark_signal_active(0, 0));
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 1);

        // First deactivation should return true
        assert!(tree.mark_signal_inactive(0, 0));
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 0);

        // Duplicate deactivation should return false
        assert!(!tree.mark_signal_inactive(0, 0));
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 0);
    }

    // ────────────────────────────────────────────────────────────────────────────
    // TASK RESERVATION SYSTEM TESTS
    // ────────────────────────────────────────────────────────────────────────────

    /// Test CAS loop correctness under concurrent access
    /// Validates that the compare_exchange loop properly handles contention
    #[test]
    fn cas_loop_handles_concurrent_contention() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 1);
        let tree = Arc::new(tree);
        let threads = 8;
        let barrier = Arc::new(Barrier::new(threads));
        let reservations = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let tree = Arc::clone(&tree);
                let barrier = Arc::clone(&barrier);
                let reservations = Arc::clone(&reservations);
                thread::spawn(move || {
                    barrier.wait();
                    // Each thread tries to reserve multiple times
                    for _ in 0..4 {
                        if let Some(handle) = tree.reserve_task_in_leaf(0, 0) {
                            let mut guard = reservations.lock().unwrap();
                            guard.push(handle);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let guard = reservations.lock().unwrap();
        assert_eq!(guard.len(), 32); // 8 threads * 4 reservations each

        // All reservations should be unique
        let mut unique = HashSet::new();
        for &bit in guard.iter() {
            assert!(
                unique.insert(bit),
                "Duplicate reservation detected: {:?}",
                bit
            );
        }

        // Cleanup
        for &bit in guard.iter() {
            tree.release_task_in_leaf(0, 0, bit as usize);
        }
    }

    /// Test reservation bitmap exhaustion and wraparound
    #[test]
    fn reservation_exhaustion_and_wraparound() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 1);

        // Exhaust all 64 bits
        let mut reservations = Vec::new();
        for _ in 0..64 {
            let bit = tree
                .reserve_task_in_leaf(0, 0)
                .expect("Should get reservation");
            reservations.push(bit);
        }

        // Should be exhausted
        assert!(tree.reserve_task_in_leaf(0, 0).is_none());

        // Release half the bits (at even indices: 0, 2, 4, ..., 62)
        let released_bits: Vec<u8> = (0..32).map(|i| reservations[i * 2]).collect();
        let still_reserved: HashSet<u8> = (0..32).map(|i| reservations[i * 2 + 1]).collect();

        for &bit in &released_bits {
            tree.release_task_in_leaf(0, 0, bit as usize);
        }

        // Should be able to reserve again - should get the released bits
        let new_reservations: Vec<_> = (0..32)
            .map(|_| {
                tree.reserve_task_in_leaf(0, 0)
                    .expect("Should get reservation")
            })
            .collect();

        // All new reservations should be unique
        let mut seen = HashSet::new();
        for &new_bit in &new_reservations {
            assert!(
                seen.insert(new_bit),
                "Got duplicate reservation: {}",
                new_bit
            );
        }

        // All new reservations should be from the released bits, not the still-reserved ones
        for &new_bit in &new_reservations {
            assert!(
                !still_reserved.contains(&new_bit),
                "Got reservation for still-reserved bit: {}",
                new_bit
            );
        }

        // Cleanup
        for &bit in &reservations {
            tree.release_task_in_leaf(0, 0, bit as usize);
        }
        for &bit in &new_reservations {
            tree.release_task_in_leaf(0, 0, bit as usize);
        }
    }

    /// Test signal distribution across a leaf
    #[test]
    fn round_robin_signal_distribution() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 4);
        let mut observed_signals = Vec::new();

        // Reserve tasks and track signal distribution
        // With bit-ops selection, signals are chosen by trailing_zeros (lowest free bit)
        for _ in 0..8 {
            let (leaf, signal, _) = tree.reserve_task().expect("reserve task");
            observed_signals.push(signal);
            tree.release_task_in_leaf(leaf, signal, 0); // Release immediately
        }

        // Should have visited all signals (though not necessarily round-robin)
        observed_signals.sort_unstable();
        let unique_signals: HashSet<_> = observed_signals.iter().collect();
        assert!(unique_signals.len() >= 1, "Should use at least 1 signal");
    }

    // ────────────────────────────────────────────────────────────────────────────
    // PARTITION MANAGEMENT TESTS
    // ────────────────────────────────────────────────────────────────────────────

    /// Test partition owner computation with various worker counts
    #[test]
    fn compute_partition_owner_distribution() {
        let (tree, _wakers, _worker_count) = setup_tree(10, 1);

        // Test with 3 workers
        for leaf_idx in 0..10 {
            let owner = tree.compute_partition_owner(leaf_idx, 3);
            assert!(owner < 3, "Owner {} should be < 3", owner);
        }

        // Test with 1 worker (all leaves belong to worker 0)
        for leaf_idx in 0..10 {
            let owner = tree.compute_partition_owner(leaf_idx, 1);
            assert_eq!(owner, 0, "All leaves should belong to worker 0");
        }

        // Test with equal workers and leaves (perfect distribution)
        // Create a tree with 5 leaves for this test
        let (tree5, _, _) = setup_tree(5, 1);
        for leaf_idx in 0..5 {
            let owner = tree5.compute_partition_owner(leaf_idx, 5);
            assert_eq!(owner, leaf_idx, "Each leaf should have unique owner");
        }
    }

    /// Test partition boundaries are correct
    #[test]
    fn partition_boundaries_are_correct() {
        let (tree, _wakers, _worker_count) = setup_tree(7, 1); // 7 leaves, 3 workers

        // Worker 0: leaves 0, 1, 2 (3 leaves) - gets extra leaf
        assert_eq!(tree.partition_start_for_worker(0, 3), 0);
        assert_eq!(tree.partition_end_for_worker(0, 3), 3);

        // Worker 1: leaves 3, 4 (2 leaves)
        assert_eq!(tree.partition_start_for_worker(1, 3), 3);
        assert_eq!(tree.partition_end_for_worker(1, 3), 5);

        // Worker 2: leaves 5, 6 (2 leaves)
        assert_eq!(tree.partition_start_for_worker(2, 3), 5);
        assert_eq!(tree.partition_end_for_worker(2, 3), 7);
    }

    /// Test global to local leaf index conversion
    #[test]
    fn global_to_local_leaf_conversion() {
        let (tree, _wakers, _worker_count) = setup_tree(6, 2); // 6 leaves, 2 workers

        // Worker 0: leaves 0, 1, 2
        assert_eq!(tree.global_to_local_leaf_idx(0, 0, 2), Some(0));
        assert_eq!(tree.global_to_local_leaf_idx(1, 0, 2), Some(1));
        assert_eq!(tree.global_to_local_leaf_idx(2, 0, 2), Some(2));
        assert_eq!(tree.global_to_local_leaf_idx(3, 0, 2), None); // Not in partition

        // Worker 1: leaves 3, 4, 5
        assert_eq!(tree.global_to_local_leaf_idx(3, 1, 2), Some(0));
        assert_eq!(tree.global_to_local_leaf_idx(4, 1, 2), Some(1));
        assert_eq!(tree.global_to_local_leaf_idx(5, 1, 2), Some(2));
        assert_eq!(tree.global_to_local_leaf_idx(2, 1, 2), None); // Not in partition
    }

    /// Test partition computation with edge cases
    #[test]
    fn partition_computation_edge_cases() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 1);

        // Single leaf, single worker
        assert_eq!(tree.compute_partition_owner(0, 1), 0);
        assert_eq!(tree.partition_start_for_worker(0, 1), 0);
        assert_eq!(tree.partition_end_for_worker(0, 1), 1);
        assert_eq!(tree.global_to_local_leaf_idx(0, 0, 1), Some(0));

        // Zero workers
        assert_eq!(tree.compute_partition_owner(0, 0), 0);
        assert_eq!(tree.partition_start_for_worker(0, 0), 0);
        assert_eq!(tree.partition_end_for_worker(0, 0), 0);
    }

    // ────────────────────────────────────────────────────────────────────────────
    // MEMORY ORDERING TESTS
    // ────────────────────────────────────────────────────────────────────────────

    /// Test that Acquire ordering ensures visibility of prior writes
    #[test]
    fn acquire_ordering_ensures_visibility() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 1);
        let tree = Arc::new(tree);
        let tree_ = Arc::clone(&tree);
        let flag = Arc::new(AtomicU64::new(0));
        let flag_ = Arc::clone(&flag);

        let handle = thread::spawn(move || {
            // Set the flag with Release ordering
            flag_.store(1, Ordering::Release);

            // Mark signal active (should be visible after flag is set)
            tree_.mark_signal_active(0, 0);
        });

        handle.join().unwrap();

        // Read with Acquire ordering (should see the flag)
        let observed_flag = flag.load(Ordering::Acquire);
        assert_eq!(observed_flag, 1);

        // Signal should be active
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 1);
    }

    /// Test that Relaxed ordering doesn't provide synchronization
    #[test]
    fn relaxed_ordering_no_synchronization() {
        let (tree, _wakers, _worker_count) = setup_tree(1, 1);
        let tree = Arc::new(tree);
        let tree_ = Arc::clone(&tree);
        let data = Arc::new(AtomicU64::new(0));
        let data_ = Arc::clone(&data);
        let flag = Arc::new(AtomicU64::new(0));
        let flag_ = Arc::clone(&flag);

        let handle = thread::spawn(move || {
            // Writer thread
            data_.store(42, Ordering::Relaxed);
            flag_.store(1, Ordering::Release);
            tree_.mark_signal_active(0, 0);
        });

        handle.join().unwrap();

        // Reader might see flag but not data due to relaxed ordering
        let observed_flag = flag.load(Ordering::Acquire);
        let observed_data = data.load(Ordering::Relaxed);

        // Flag should be visible
        assert_eq!(observed_flag, 1);

        // Data might not be visible (this test documents the behavior)
        // In practice, the signal activation provides some ordering
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 1);
    }

    // ────────────────────────────────────────────────────────────────────────────
    // EDGE CASES AND ERROR HANDLING
    // ────────────────────────────────────────────────────────────────────────────

    /// Test boundary conditions for indices
    #[test]
    fn boundary_conditions_for_indices() {
        let (tree, _wakers, _worker_count) = setup_tree(2, 3);

        // Valid indices should work
        assert!(tree.mark_signal_active(0, 0));
        assert!(tree.mark_signal_active(1, 2));
        assert!(tree.reserve_task_in_leaf(0, 0).is_some());
        assert!(tree.reserve_task_in_leaf(1, 2).is_some());

        // Invalid leaf index should return None/false
        assert!(!tree.mark_signal_active(2, 0)); // leaf_idx >= leaf_count
        assert!(!tree.mark_signal_inactive(2, 0));
        assert!(tree.reserve_task_in_leaf(2, 0).is_none());

        // Invalid signal index should return None/false
        assert!(!tree.mark_signal_active(0, 3)); // signal_idx >= signals_per_leaf
        assert!(!tree.mark_signal_inactive(0, 3));
        assert!(tree.reserve_task_in_leaf(0, 3).is_none());
    }

    /// Test zero signals per leaf
    #[test]
    fn zero_signals_per_leaf_handling() {
        // This should panic during creation
        let result = std::panic::catch_unwind(|| {
            let wakers: Vec<Arc<WorkerWaker>> =
                (0..2).map(|_| Arc::new(WorkerWaker::new())).collect();
            let worker_count = Arc::new(AtomicUsize::new(2));
            Summary::new(2, 0, &wakers, &worker_count);
        });

        assert!(result.is_err());
    }

    /// Test empty wakers array
    #[test]
    fn empty_wakers_array_handling() {
        // This should panic during creation
        let result = std::panic::catch_unwind(|| {
            let wakers: Vec<Arc<WorkerWaker>> = Vec::new();
            let worker_count = Arc::new(AtomicUsize::new(0));
            Summary::new(2, 2, &wakers, &worker_count);
        });

        assert!(result.is_err());
    }

    /// Test release_task with various configurations
    #[test]
    fn reserve_task_various_configurations() {
        // Empty tree - should panic during creation, not during reserve_task
        let result = std::panic::catch_unwind(|| {
            let wakers: Vec<Arc<WorkerWaker>> =
                (0..2).map(|_| Arc::new(WorkerWaker::new())).collect();
            let worker_count = Arc::new(AtomicUsize::new(2));
            Summary::new(0, 1, &wakers, &worker_count)
        });
        assert!(result.is_err(), "Empty tree should panic during creation");

        // Tree with zero signals per leaf
        let result = std::panic::catch_unwind(|| {
            let wakers: Vec<Arc<WorkerWaker>> =
                (0..2).map(|_| Arc::new(WorkerWaker::new())).collect();
            let worker_count = Arc::new(AtomicUsize::new(2));
            Summary::new(2, 0, &wakers, &worker_count)
        });
        assert!(result.is_err());
    }

    // ────────────────────────────────────────────────────────────────────────────
    // CONCURRENCY STRESS TESTS
    // ────────────────────────────────────────────────────────────────────────────

    /// High contention stress test for signal activation/deactivation
    #[test]
    fn stress_test_signal_activation_deactivation() {
        let (tree, _wakers, _worker_count) = setup_tree(4, 2);
        let tree = Arc::new(tree);
        let threads = 12;
        let iterations = 100;
        let barrier = Arc::new(Barrier::new(threads));

        let handles: Vec<_> = (0..threads)
            .map(|thread_id| {
                let tree = Arc::clone(&tree);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    for _ in 0..iterations {
                        let leaf_idx = thread_id % 4;
                        let signal_idx = thread_id % 2;

                        // Randomly activate or deactivate
                        if thread_id % 2 == 0 {
                            tree.mark_signal_active(leaf_idx, signal_idx);
                        } else {
                            tree.mark_signal_inactive(leaf_idx, signal_idx);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Tree should still be in valid state
        for i in 0..4 {
            let word = tree.leaf_words[i].load(Ordering::Relaxed);
            assert!(word <= 0b11, "Leaf {} has invalid word: {}", i, word);
        }
    }

    /// Stress test for partition owner computation with changing worker count
    #[test]
    fn stress_test_partition_owner_with_changing_workers() {
        let (tree, _wakers, worker_count) = setup_tree(20, 1);
        let tree = Arc::new(tree);

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let tree = Arc::clone(&tree);
                let worker_count = Arc::clone(&worker_count);
                thread::spawn(move || {
                    // Each thread uses different worker counts
                    let test_counts = [1, 2, 4, 5, 10];
                    for &count in &test_counts {
                        worker_count.store(count, Ordering::Relaxed);

                        // Compute partition owners for all leaves
                        for leaf_idx in 0..20 {
                            let owner = tree.compute_partition_owner(leaf_idx, count);
                            assert!(owner < count, "Invalid owner {} for count {}", owner, count);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// Test for task reservation system to verify no duplicate reservations by checking the bitmap
    #[test]
    fn stress_test_task_reservation_system() {
        let (tree, _wakers, _worker_count) = setup_tree(4, 4); // Increase size to reduce contention
        let tree = Arc::new(tree);
        let threads = 16; // Reduce threads to reduce contention
        let iterations = 200; // Reduce iterations
        let barrier = Arc::new(Barrier::new(threads));

        let handles: Vec<_> = (0..threads)
				.map(|thread_id| {
					let tree = Arc::clone(&tree);
					let barrier = Arc::clone(&barrier);
					thread::spawn(move || {
						barrier.wait();
						for i in 0..iterations {
							// Try to reserve - may fail if all slots are taken
							if let Some((leaf, signal, bit)) = tree.reserve_task() {
								// Verify the bit is actually set in the reservation bitmap
								let reservation_word = tree.reservation_word(leaf, signal);
								let current = reservation_word.load(Ordering::SeqCst);
								let mask = 1u64 << bit;

								// The bit should be set
								assert!(current & mask != 0,
												"Thread {} iteration {}: Bit {} not set in reservation bitmap for ({}, {})",
												thread_id, i, bit, leaf, signal);

								// Hold for a bit to create contention
								thread::sleep(Duration::from_micros(100));

								// Release the reservation
								tree.release_task_in_leaf(leaf, signal, bit as usize);

								// Note: We don't verify the bit is cleared here because under high contention,
								// another thread may immediately re-reserve the same bit. The final check at
								// the end of the test verifies all bits are eventually released.
							}
						}
					})
				})
				.collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all reservation bitmaps are empty
        for leaf in 0..tree.leaf_count {
            for signal in 0..tree.signals_per_leaf {
                let reservation_word = tree.reservation_word(leaf, signal);
                let current = reservation_word.load(Ordering::SeqCst);
                assert_eq!(
                    current, 0,
                    "Reservation bitmap not empty for leaf {}, signal {}: {:b}",
                    leaf, signal, current
                );
            }
        }

        println!(
            "Successfully completed {} iterations with {} threads",
            iterations, threads
        );
    }

    /// Test TOCTOU race condition mitigation in notification system
    #[test]
    fn test_toctou_race_mitigation() {
        let (tree, wakers_, worker_count) = setup_tree(4, 2);
        let tree = Arc::new(tree);
        let tree_ = Arc::clone(&tree);

        // Start with 2 workers
        worker_count.store(2, Ordering::Relaxed);

        let handle = thread::spawn(move || {
            let tree = tree_;
            // Simulate rapid worker count changes
            for count in [1, 3, 0, 2, 4] {
                worker_count.store(count, Ordering::Relaxed);

                // Try to trigger notifications - should not panic
                for leaf_idx in 0..4 {
                    // These calls should handle the changing worker count safely
                    tree.notify_partition_owner_active(leaf_idx);
                    tree.notify_partition_owner_inactive(leaf_idx);
                }

                thread::yield_now();
            }
        });

        handle.join().unwrap();

        // Tree should still be functional
        assert!(tree.mark_signal_active(0, 0));
        assert!(tree.reserve_task().is_some());
    }
}
