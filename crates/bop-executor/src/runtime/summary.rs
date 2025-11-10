use crate::bits;
use crate::utils::CachePadded;
use super::waker::WorkerWaker;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Single-level summary tree for task work-stealing.
///
/// This tree tracks ONLY task signals - no yield or worker state.
/// Each leaf represents a set of task signal words.
///
/// The SummaryTree coordinates with SignalWakers to notify partition owners
/// when their assigned leafs become active/inactive.
pub struct Summary {
    // Owned heap allocations
    pub(crate) leaf_words: Box<[AtomicU64]>, // Pub for Worker access
    task_reservations: Box<[AtomicU64]>,

    // Configuration
    leaf_count: usize,
    signals_per_leaf: usize,
    leaf_summary_mask: u64,

    // Round-robin cursors for allocation
    next_leaf: CachePadded<AtomicUsize>,
    next_signal_per_leaf: Box<[CachePadded<AtomicUsize>]>,

    // Partition owner notification
    // Raw pointer to WorkerService.wakers array (lifetime guaranteed by WorkerService ownership)
    wakers: *const Arc<WorkerWaker>,
    wakers_len: usize,
    // Reference to WorkerService.worker_count - single source of truth
    worker_count: *const AtomicUsize,
}

unsafe impl Send for Summary {}
unsafe impl Sync for Summary {}

impl Summary {
    /// Creates a new SummaryTree with the specified dimensions.
    ///
    /// # Arguments
    /// * `leaf_count` - Number of leaf nodes (typically matches worker partition count)
    /// * `signals_per_leaf` - Number of task signal words per leaf (typically tasks_per_leaf / 64)
    /// * `wakers` - Slice of SignalWakers for partition owner notification
    /// * `worker_count` - Reference to WorkerService's worker_count atomic (single source of truth)
    ///
    /// # Safety
    /// The wakers slice and worker_count reference must remain valid for the lifetime of this SummaryTree.
    /// This is guaranteed when SummaryTree is owned by WorkerService which also owns the wakers and worker_count.
    pub fn new(
        leaf_count: usize,
        signals_per_leaf: usize,
        wakers: &[Arc<WorkerWaker>],
        worker_count: &AtomicUsize,
    ) -> Self {
        assert!(leaf_count > 0, "leaf_count must be > 0");
        assert!(signals_per_leaf > 0, "signals_per_leaf must be > 0");
        assert!(signals_per_leaf <= 64, "signals_per_leaf must be <= 64");
        assert!(!wakers.is_empty(), "wakers must not be empty");

        let task_word_count = leaf_count * signals_per_leaf;

        let leaf_words = (0..leaf_count)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let task_reservations = (0..task_word_count)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let leaf_summary_mask = if signals_per_leaf >= 64 {
            u64::MAX
        } else {
            (1u64 << signals_per_leaf) - 1
        };

        let next_signal_per_leaf = (0..leaf_count)
            .map(|_| CachePadded::new(AtomicUsize::new(0)))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            leaf_words,
            task_reservations,
            leaf_count,
            signals_per_leaf,
            leaf_summary_mask,
            next_leaf: CachePadded::new(AtomicUsize::new(0)),
            next_signal_per_leaf,
            wakers: wakers.as_ptr(),
            wakers_len: wakers.len(),
            worker_count: worker_count as *const AtomicUsize,
        }
    }

    /// Get the current worker count from WorkerService.
    /// Reads directly from the single source of truth.
    #[inline]
    pub fn get_worker_count(&self) -> usize {
        unsafe { &*self.worker_count }.load(Ordering::Relaxed)
    }

    #[inline(always)]
    fn leaf_word(&self, idx: usize) -> &AtomicU64 {
        &self.leaf_words[idx]
    }

    #[inline(always)]
    fn reservation_word(&self, leaf_idx: usize, signal_idx: usize) -> &AtomicU64 {
        let index = leaf_idx * self.signals_per_leaf + signal_idx;
        &self.task_reservations[index]
    }

    /// Notify the partition owner's SignalWaker that a leaf in their partition became active.
    #[inline(always)]
    fn notify_partition_owner_active(&self, leaf_idx: usize) {
        let worker_count = unsafe { &*self.worker_count }.load(Ordering::Relaxed);
        if worker_count == 0 {
            return;
        }

        let owner_id = self.compute_partition_owner(leaf_idx, worker_count);
        if owner_id < self.wakers_len {
            // SAFETY: wakers pointer is valid for the lifetime of SummaryTree
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
    #[inline(always)]
    fn notify_partition_owner_inactive(&self, leaf_idx: usize) {
        let worker_count = unsafe { &*self.worker_count }.load(Ordering::Relaxed);
        if worker_count == 0 {
            return;
        }

        let owner_id = self.compute_partition_owner(leaf_idx, worker_count);
        if owner_id < self.wakers_len {
            // SAFETY: wakers pointer is valid for the lifetime of SummaryTree
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
        let prev = leaf.fetch_or(mask, Ordering::AcqRel);

        let was_empty = prev & self.leaf_summary_mask == 0;
        let any_new_bits = (prev & mask) != mask;

        // Notify partition owner if any new signal bits were set
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
        let prev = leaf.fetch_and(!mask, Ordering::AcqRel);
        if prev & mask == 0 {
            return false;
        }
        // Notify partition owner if this leaf is now empty
        if (prev & !mask) & self.leaf_summary_mask == 0 {
            self.notify_partition_owner_inactive(leaf_idx);
            true
        } else {
            false
        }
    }

    /// Sets the summary bit for a task signal.
    pub fn mark_signal_active(&self, leaf_idx: usize, signal_idx: usize) -> bool {
        debug_assert!(signal_idx < self.signals_per_leaf);
        let mask = 1u64 << signal_idx;
        self.mark_leaf_bits(leaf_idx, mask)
    }

    /// Clears the summary bit for a task signal.
    pub fn mark_signal_inactive(&self, leaf_idx: usize, signal_idx: usize) -> bool {
        debug_assert!(signal_idx < self.signals_per_leaf);
        let mask = 1u64 << signal_idx;
        self.clear_leaf_bits(leaf_idx, mask)
    }

    /// Attempts to reserve a task slot within (`leaf_idx`, `signal_idx`).
    /// Returns the bit index on success.
    pub fn reserve_task_in_leaf(&self, leaf_idx: usize, signal_idx: usize) -> Option<u8> {
        if leaf_idx >= self.leaf_count || signal_idx >= self.signals_per_leaf {
            return None;
        }
        let reservations = self.reservation_word(leaf_idx, signal_idx);
        let mut current = reservations.load(Ordering::Acquire);
        loop {
            let free = !current;
            if free == 0 {
                return None;
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
    pub fn release_task_in_leaf(&self, leaf_idx: usize, signal_idx: usize, bit: usize) {
        if leaf_idx >= self.leaf_count || signal_idx >= self.signals_per_leaf || bit >= 64 {
            return;
        }
        let mask = !(1u64 << bit);
        self.reservation_word(leaf_idx, signal_idx)
            .fetch_and(mask, Ordering::AcqRel);
    }

    /// Convenience function: reserve the first available task slot across the arena.
    pub fn reserve_task(&self) -> Option<(usize, usize, u8)> {
        if self.leaf_count == 0 {
            return None;
        }
        if self.signals_per_leaf == 0 {
            return None;
        }

        let start_leaf = self.next_leaf.fetch_add(1, Ordering::Relaxed) % self.leaf_count;
        for offset in 0..self.leaf_count {
            let leaf_idx = (start_leaf + offset) % self.leaf_count;
            let signal_cursor = &self.next_signal_per_leaf[leaf_idx];
            let start_signal =
                signal_cursor.fetch_add(1, Ordering::Relaxed) % self.signals_per_leaf;
            for signal_offset in 0..self.signals_per_leaf {
                let signal_idx = (start_signal + signal_offset) % self.signals_per_leaf;
                if let Some(bit) = self.reserve_task_in_leaf(leaf_idx, signal_idx) {
                    return Some((leaf_idx, signal_idx, bit));
                }
            }
        }
        None
    }

    /// Clears the summary bit when the corresponding task signal becomes empty.
    pub fn mark_signal_inactive_if_empty(
        &self,
        leaf_idx: usize,
        signal_idx: usize,
        signal: &AtomicU64,
    ) {
        if signal.load(Ordering::Relaxed) == 0 {
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

    /// Compute which worker owns a given leaf based on partition assignments.
    ///
    /// This is the inverse of `Worker::compute_partition()`. Given a leaf index,
    /// it determines which worker is responsible for processing tasks in that leaf.
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
    use crate::runtime::waker::WorkerWaker;

    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use std::thread::yield_now;
    use std::time::{Duration, Instant};

    fn setup_tree(leaf_count: usize, signals_per_leaf: usize) -> (Summary, AtomicUsize) {
        // Create dummy wakers for testing
        let wakers: Vec<Arc<WorkerWaker>> = (0..4).map(|_| Arc::new(WorkerWaker::new())).collect();
        let worker_count = AtomicUsize::new(4);
        let tree = Summary::new(leaf_count, signals_per_leaf, &wakers, &worker_count);
        (tree, worker_count)
    }

    #[test]
    fn mark_signal_active_updates_root_and_leaf() {
        let (tree, _worker_count) = setup_tree(4, 4);
        assert!(tree.mark_signal_active(1, 1));
        assert_eq!(tree.leaf_words[1].load(Ordering::Relaxed), 1u64 << 1);
        // assert_ne!(tree.root_words[0].load(Ordering::Relaxed) & (1u64 << 1), 0); // root_words removed

        assert!(!tree.mark_signal_active(1, 1));
        assert!(tree.mark_signal_inactive(1, 1));
        assert_eq!(tree.leaf_words[1].load(Ordering::Relaxed), 0);
        // assert_eq!(tree.root_words[0].load(Ordering::Relaxed) & (1u64 << 1), 0); // root_words removed
    }

    #[test]
    fn mark_signal_inactive_if_empty_clears_summary() {
        let (tree, _worker_count) = setup_tree(1, 2);
        assert!(tree.mark_signal_active(0, 1));
        let signal = AtomicU64::new(0);
        tree.mark_signal_inactive_if_empty(0, 1, &signal);
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 0);
        // assert_eq!(tree.root_words[0].load(Ordering::Relaxed), 0); // root_words removed
    }

    #[test]
    fn reserve_task_in_leaf_exhausts_all_bits() {
        let (tree, _worker_count) = setup_tree(1, 1);
        let mut bits = Vec::with_capacity(64);
        for _ in 0..64 {
            let bit = tree.reserve_task_in_leaf(0, 0).expect("expected free bit");
            bits.push(bit);
        }
        bits.sort_unstable();
        assert_eq!(bits, (0..64).collect::<Vec<_>>());
        assert!(
            tree.reserve_task_in_leaf(0, 0).is_none(),
            "all bits should be exhausted"
        );
        for bit in bits {
            tree.release_task_in_leaf(0, 0, bit as usize);
        }
    }

    #[test]
    fn reserve_task_round_robin_visits_all_leaves() {
        let (tree, _worker_count) = setup_tree(4, 1);
        let mut observed = Vec::with_capacity(4);
        for _ in 0..4 {
            let (leaf, sig, bit) = tree.reserve_task().expect("reserve task");
            observed.push(leaf);
            tree.release_task_in_leaf(leaf, sig, bit as usize);
        }
        observed.sort_unstable();
        assert_eq!(observed, vec![0, 1, 2, 3]);
    }

    #[test]
    fn concurrent_reservations_are_unique() {
        let (tree, _worker_count) = setup_tree(4, 1);
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

        let guard = handles.lock().unwrap();
        let mut unique = HashSet::new();
        for &(leaf, signal, bit) in guard.iter() {
            assert!(
                unique.insert((leaf, signal, bit)),
                "duplicate handle detected"
            );
        }
        assert_eq!(guard.len(), threads * reservations_per_thread);

        for &(leaf, signal, bit) in guard.iter() {
            tree.release_task_in_leaf(leaf, signal, bit as usize);
        }
    }

    #[test]
    fn reserve_and_release_task_updates_reservations() {
        let (tree, _worker_count) = setup_tree(4, 1);
        let handle = tree.reserve_task().expect("task handle");
        assert_eq!(handle.1, 0); // signal idx

        let reservation = tree
            .task_reservations
            .get(handle.0 * 1 + handle.1)
            .unwrap()
            .load(Ordering::Relaxed);
        assert_ne!(reservation, 0);

        tree.release_task_in_leaf(handle.0, handle.1, handle.2 as usize);
        let reservation = tree
            .task_reservations
            .get(handle.0 * 1 + handle.1)
            .unwrap()
            .load(Ordering::Relaxed);
        assert_eq!(reservation, 0);
    }
}
