use crate::bits;
use crate::utils::CachePadded;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// Two-level (root → leaf) summary tree for task work-stealing.
///
/// This tree tracks ONLY task signals - no yield or worker state.
/// Each leaf represents a set of task signal words, and the root provides
/// a summary bitmap of which leaves have active tasks.
pub struct SummaryTree {
    // Owned heap allocations
    root_words: Box<[AtomicU64]>,
    pub(crate) leaf_words: Box<[AtomicU64]>,  // Pub for ExecutorArena access
    task_reservations: Box<[AtomicU64]>,

    // Configuration
    leaf_count: usize,
    signals_per_leaf: usize,
    leaf_summary_mask: u64,

    // Round-robin cursors for allocation
    next_leaf: CachePadded<AtomicUsize>,
    next_signal_per_leaf: Box<[CachePadded<AtomicUsize>]>,

    // Wake/sleep synchronization
    permits: CachePadded<AtomicU64>,
    sleepers: CachePadded<AtomicUsize>,
    lock: Mutex<()>,
    cv: Condvar,
}

unsafe impl Send for SummaryTree {}
unsafe impl Sync for SummaryTree {}

impl SummaryTree {
    /// Creates a new SummaryTree with the specified dimensions.
    ///
    /// # Arguments
    /// * `leaf_count` - Number of leaf nodes (typically matches worker partition count)
    /// * `signals_per_leaf` - Number of task signal words per leaf (typically tasks_per_leaf / 64)
    pub fn new(leaf_count: usize, signals_per_leaf: usize) -> Self {
        assert!(leaf_count > 0, "leaf_count must be > 0");
        assert!(signals_per_leaf > 0, "signals_per_leaf must be > 0");
        assert!(signals_per_leaf <= 64, "signals_per_leaf must be <= 64");

        let root_count = ((leaf_count + 63) / 64).max(1);
        let task_word_count = leaf_count * signals_per_leaf;

        let root_words = (0..root_count)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

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
            root_words,
            leaf_words,
            task_reservations,
            leaf_count,
            signals_per_leaf,
            leaf_summary_mask,
            next_leaf: CachePadded::new(AtomicUsize::new(0)),
            next_signal_per_leaf,
            permits: CachePadded::new(AtomicU64::new(0)),
            sleepers: CachePadded::new(AtomicUsize::new(0)),
            lock: Mutex::new(()),
            cv: Condvar::new(),
        }
    }

    #[inline(always)]
    fn root_word(&self, idx: usize) -> &AtomicU64 {
        &self.root_words[idx]
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

    #[inline(always)]
    fn activate_root(&self, leaf_idx: usize) {
        let word_idx = leaf_idx / 64;
        let bit = leaf_idx % 64;
        let mask = 1u64 << bit;
        let root = self.root_word(word_idx);
        let prev = root.fetch_or(mask, Ordering::AcqRel);
        if prev & mask == 0 {
            self.release(1);
        }
    }

    #[inline(always)]
    fn deactivate_root(&self, leaf_idx: usize) {
        let word_idx = leaf_idx / 64;
        let bit = leaf_idx % 64;
        let mask = !(1u64 << bit);
        self.root_word(word_idx).fetch_and(mask, Ordering::AcqRel);
    }

    #[inline(always)]
    fn mark_leaf_bits(&self, leaf_idx: usize, mask: u64) -> bool {
        if mask == 0 {
            return false;
        }
        let leaf = self.leaf_word(leaf_idx);
        let prev = leaf.fetch_or(mask, Ordering::AcqRel);
        // Activate root if this leaf was previously empty
        if prev & self.leaf_summary_mask == 0 {
            self.activate_root(leaf_idx);
            true
        } else {
            false
        }
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
        // Deactivate root if this leaf is now empty
        if (prev & !mask) & self.leaf_summary_mask == 0 {
            self.deactivate_root(leaf_idx);
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
    pub fn reserve_task_in_leaf(&self, leaf_idx: usize, signal_idx: usize) -> Option<u32> {
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
            let bit = free.trailing_zeros() as u32;
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
    pub fn release_task_in_leaf(&self, leaf_idx: usize, signal_idx: usize, bit: u32) {
        if leaf_idx >= self.leaf_count || signal_idx >= self.signals_per_leaf || bit >= 64 {
            return;
        }
        let mask = !(1u64 << bit);
        self.reservation_word(leaf_idx, signal_idx)
            .fetch_and(mask, Ordering::AcqRel);
    }

    /// Convenience function: reserve the first available task slot across the arena.
    pub fn reserve_task(&self) -> Option<(usize, usize, u32)> {
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
    pub fn snapshot_summary_word(&self, idx: usize) -> u64 {
        if idx >= self.root_words.len() {
            return 0;
        }
        self.root_word(idx).load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn snapshot_summary(&self) -> u64 {
        self.snapshot_summary_word(0)
    }

    #[inline(always)]
    pub fn summary_select(&self, nearest_to_index: u64) -> u64 {
        bits::find_nearest(self.snapshot_summary(), nearest_to_index)
    }

    #[inline(always)]
    pub fn summary(&self) -> u64 {
        self.snapshot_summary()
    }

    #[inline]
    pub fn try_acquire(&self) -> bool {
        self.permits
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |p| p.checked_sub(1))
            .is_ok()
    }

    pub fn acquire(&self) {
        if self.try_acquire() {
            return;
        }
        let mut guard = self.lock.lock().expect("active summary mutex poisoned");
        self.sleepers.fetch_add(1, Ordering::Relaxed);
        loop {
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            guard = self
                .cv
                .wait(guard)
                .expect("active summary condvar wait poisoned");
        }
    }

    pub fn acquire_timeout(&self, timeout: Duration) -> bool {
        if self.try_acquire() {
            return true;
        }
        let start = std::time::Instant::now();
        let mut guard = self.lock.lock().expect("active summary mutex poisoned");
        self.sleepers.fetch_add(1, Ordering::Relaxed);
        while start.elapsed() < timeout {
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return true;
            }
            let remaining = timeout.saturating_sub(start.elapsed());
            let (g, res) = self
                .cv
                .wait_timeout(guard, remaining)
                .expect("active summary condvar wait poisoned");
            guard = g;
            if res.timed_out() {
                break;
            }
        }
        self.sleepers.fetch_sub(1, Ordering::Relaxed);
        false
    }

    #[inline(always)]
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

    #[inline(always)]
    pub fn sleepers(&self) -> usize {
        self.sleepers.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn leaf_count(&self) -> usize {
        self.leaf_count
    }

    #[inline(always)]
    pub fn signals_per_leaf(&self) -> usize {
        self.signals_per_leaf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use std::thread::yield_now;
    use std::time::{Duration, Instant};

    fn setup_tree(leaf_count: usize, signals_per_leaf: usize) -> SummaryTree {
        SummaryTree::new(leaf_count, signals_per_leaf)
    }

    #[test]
    fn mark_signal_active_updates_root_and_leaf() {
        let tree = setup_tree(4, 4);
        assert!(tree.mark_signal_active(1, 1));
        assert_eq!(tree.leaf_words[1].load(Ordering::Relaxed), 1u64 << 1);
        assert_ne!(tree.root_words[0].load(Ordering::Relaxed) & (1u64 << 1), 0);

        assert!(!tree.mark_signal_active(1, 1));
        assert!(tree.mark_signal_inactive(1, 1));
        assert_eq!(tree.leaf_words[1].load(Ordering::Relaxed), 0);
        assert_eq!(tree.root_words[0].load(Ordering::Relaxed) & (1u64 << 1), 0);
    }

    #[test]
    fn mark_signal_inactive_if_empty_clears_summary() {
        let tree = setup_tree(1, 2);
        assert!(tree.mark_signal_active(0, 1));
        let signal = AtomicU64::new(0);
        tree.mark_signal_inactive_if_empty(0, 1, &signal);
        assert_eq!(tree.leaf_words[0].load(Ordering::Relaxed), 0);
        assert_eq!(tree.root_words[0].load(Ordering::Relaxed), 0);
    }

    #[test]
    fn reserve_task_in_leaf_exhausts_all_bits() {
        let tree = setup_tree(1, 1);
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
            tree.release_task_in_leaf(0, 0, bit);
        }
    }

    #[test]
    fn reserve_task_round_robin_visits_all_leaves() {
        let tree = setup_tree(4, 1);
        let mut observed = Vec::with_capacity(4);
        for _ in 0..4 {
            let (leaf, sig, bit) = tree.reserve_task().expect("reserve task");
            observed.push(leaf);
            tree.release_task_in_leaf(leaf, sig, bit);
        }
        observed.sort_unstable();
        assert_eq!(observed, vec![0, 1, 2, 3]);
    }

    #[test]
    fn summary_select_prefers_nearest_active_leaf() {
        let tree = setup_tree(8, 1);
        assert!(tree.mark_signal_active(0, 0));
        assert!(tree.mark_signal_active(5, 0));

        let idx0 = tree.summary_select(0);
        assert_eq!(idx0, 0);

        let idx_near_five = tree.summary_select(6);
        assert_eq!(idx_near_five, 5);

        assert!(tree.mark_signal_inactive(0, 0));
        assert!(tree.mark_signal_inactive(5, 0));
    }

    #[test]
    fn concurrent_reservations_are_unique() {
        let tree = Arc::new(setup_tree(4, 1));
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
            tree.release_task_in_leaf(leaf, signal, bit);
        }
    }

    #[test]
    fn acquire_timeout_unblocks_after_release() {
        let tree = Arc::new(setup_tree(1, 1));
        let barrier = Arc::new(Barrier::new(2));

        let tree_thread = Arc::clone(&tree);
        let barrier_thread = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            barrier_thread.wait();
            let start = Instant::now();
            let ok = tree_thread.acquire_timeout(Duration::from_millis(200));
            (ok, start.elapsed())
        });

        barrier.wait();
        thread::sleep(Duration::from_millis(50));
        tree.release(1);

        let (ok, elapsed) = handle.join().expect("thread panicked");
        assert!(ok, "acquire_timeout should succeed after release");
        assert!(
            elapsed < Duration::from_millis(200),
            "acquire should unblock before timeout"
        );
        assert_eq!(tree.sleepers(), 0);
    }

    #[test]
    fn reserve_and_release_task_updates_reservations() {
        let tree = setup_tree(4, 1);
        let handle = tree.reserve_task().expect("task handle");
        assert_eq!(handle.1, 0); // signal idx

        let reservation = tree.task_reservations
            .get(handle.0 * 1 + handle.1)
            .unwrap()
            .load(Ordering::Relaxed);
        assert_ne!(reservation, 0);

        tree.release_task_in_leaf(handle.0, handle.1, handle.2);
        let reservation = tree.task_reservations
            .get(handle.0 * 1 + handle.1)
            .unwrap()
            .load(Ordering::Relaxed);
        assert_eq!(reservation, 0);
    }
}
