use crate::bits;
use crate::utils::CachePadded;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct SummaryInit {
    pub root_words: *const AtomicU64,
    pub root_count: usize,
    pub leaf_words: *const AtomicU64,
    pub leaf_count: usize,
    pub signals_per_leaf: usize,
    pub task_reservations: *const AtomicU64,
    pub worker_bitmap: *const AtomicU64,
    pub worker_bitmap_words: usize,
    pub max_workers: usize,
    pub yield_bit_index: u32,
}

/// Arena-backed two level (root → leaf) summary tree with reservation maps.
pub struct SummaryTree {
    root_words: *const AtomicU64,
    root_count: usize,
    leaf_words: *const AtomicU64,
    leaf_count: usize,
    signals_per_leaf: usize,
    task_reservations: *const AtomicU64,
    task_word_count: usize,
    worker_bitmap: *const AtomicU64,
    worker_bitmap_words: usize,
    max_workers: usize,
    yield_bit_mask: u64,
    leaf_summary_mask: u64,
    activity_mask: u64,
    next_leaf: CachePadded<AtomicUsize>,
    next_signal_per_leaf: Box<[CachePadded<AtomicUsize>]>,

    permits: CachePadded<AtomicU64>,
    sleepers: CachePadded<AtomicUsize>,
    worker_count: CachePadded<AtomicUsize>,
    lock: Mutex<()>,
    cv: Condvar,
}

unsafe impl Send for SummaryTree {}
unsafe impl Sync for SummaryTree {}

impl SummaryTree {
    /// # Safety
    ///
    /// All pointers supplied via `init` must remain valid for the lifetime of the tree.
    pub unsafe fn new(init: SummaryInit) -> Self {
        debug_assert!(!init.root_words.is_null());
        debug_assert!(!init.leaf_words.is_null());
        debug_assert!(!init.task_reservations.is_null());
        debug_assert!(!init.worker_bitmap.is_null());
        debug_assert!(init.yield_bit_index < 64);
        debug_assert!(init.signals_per_leaf <= 63);
        debug_assert!(init.worker_bitmap_words > 0);
        debug_assert!(init.max_workers > 0);

        let task_word_count = init.leaf_count * init.signals_per_leaf;
        let yield_bit_mask = 1u64 << init.yield_bit_index;
        let leaf_summary_mask = if init.signals_per_leaf == 0 {
            0
        } else {
            (1u64 << init.signals_per_leaf.min(63)) - 1
        };
        let activity_mask = leaf_summary_mask | yield_bit_mask;
        let mut next_signal_per_leaf = Vec::with_capacity(init.leaf_count);
        for _ in 0..init.leaf_count {
            next_signal_per_leaf.push(CachePadded::new(AtomicUsize::new(0)));
        }

        Self {
            root_words: init.root_words,
            root_count: init.root_count,
            leaf_words: init.leaf_words,
            leaf_count: init.leaf_count,
            signals_per_leaf: init.signals_per_leaf,
            task_reservations: init.task_reservations,
            task_word_count,
            worker_bitmap: init.worker_bitmap,
            worker_bitmap_words: init.worker_bitmap_words,
            max_workers: init.max_workers,
            yield_bit_mask,
            leaf_summary_mask,
            activity_mask,
            next_leaf: CachePadded::new(AtomicUsize::new(0)),
            next_signal_per_leaf: next_signal_per_leaf.into_boxed_slice(),
            permits: CachePadded::new(AtomicU64::new(0)),
            sleepers: CachePadded::new(AtomicUsize::new(0)),
            worker_count: CachePadded::new(AtomicUsize::new(0)),
            lock: Mutex::new(()),
            cv: Condvar::new(),
        }
    }

    #[inline(always)]
    fn root_word(&self, idx: usize) -> &AtomicU64 {
        debug_assert!(idx < self.root_count);
        unsafe { &*self.root_words.add(idx) }
    }

    #[inline(always)]
    fn leaf_word(&self, idx: usize) -> &AtomicU64 {
        debug_assert!(idx < self.leaf_count);
        unsafe { &*self.leaf_words.add(idx) }
    }

    #[inline(always)]
    fn reservation_word(&self, leaf_idx: usize, signal_idx: usize) -> &AtomicU64 {
        debug_assert!(leaf_idx < self.leaf_count);
        debug_assert!(signal_idx < self.signals_per_leaf);
        let index = leaf_idx * self.signals_per_leaf + signal_idx;
        debug_assert!(index < self.task_word_count);
        unsafe { &*self.task_reservations.add(index) }
    }

    #[inline(always)]
    fn worker_word(&self, idx: usize) -> &AtomicU64 {
        debug_assert!(idx < self.worker_bitmap_words);
        unsafe { &*self.worker_bitmap.add(idx) }
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
        if prev & self.activity_mask == 0 {
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
        if (prev & !mask) & self.activity_mask == 0 {
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

    /// Marks the reserved yield bit as active for `leaf_idx`.
    pub fn mark_yield_active(&self, leaf_idx: usize) -> bool {
        self.mark_leaf_bits(leaf_idx, self.yield_bit_mask)
    }

    /// Clears the reserved yield bit for `leaf_idx`.
    pub fn mark_yield_inactive(&self, leaf_idx: usize) -> bool {
        self.clear_leaf_bits(leaf_idx, self.yield_bit_mask)
    }

    #[inline(always)]
    pub fn yield_bit_mask(&self) -> u64 {
        self.yield_bit_mask
    }

    /// Reserve a worker slot; returns the worker index on success.
    pub fn reserve_worker(&self) -> Option<usize> {
        for word_idx in 0..self.worker_bitmap_words {
            let word = self.worker_word(word_idx);
            let mut current = word.load(Ordering::Acquire);
            loop {
                let mut free = !current;
                if word_idx == self.worker_bitmap_words - 1 {
                    let remaining = self.max_workers.saturating_sub(word_idx * 64);
                    if remaining < 64 {
                        let mask = if remaining == 0 {
                            0
                        } else {
                            (1u64 << remaining) - 1
                        };
                        free &= mask;
                    }
                }
                if free == 0 {
                    break;
                }
                let bit = free.trailing_zeros() as usize;
                let mask = 1u64 << bit;
                match word.compare_exchange(
                    current,
                    current | mask,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        let worker_idx = word_idx * 64 + bit;
                        if worker_idx >= self.max_workers {
                            word.fetch_and(!mask, Ordering::Release);
                            return None;
                        }
                        self.worker_count.fetch_add(1, Ordering::Relaxed);
                        return Some(worker_idx);
                    }
                    Err(updated) => current = updated,
                }
            }
        }
        None
    }

    /// Release a previously reserved worker slot.
    pub fn release_worker(&self, worker_idx: usize) {
        if worker_idx >= self.max_workers {
            return;
        }
        let word_idx = worker_idx / 64;
        let bit = worker_idx % 64;
        let mask = !(1u64 << bit);
        self.worker_word(word_idx).fetch_and(mask, Ordering::AcqRel);
        self.worker_count.fetch_sub(1, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn worker_count(&self) -> usize {
        self.worker_count.load(Ordering::Relaxed)
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
        if idx >= self.root_count {
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
    pub fn active_leaves(&self) -> usize {
        self.leaf_count
    }

    #[inline(always)]
    pub fn active_yield_slots(&self) -> usize {
        self.leaf_count
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

    struct TestArena {
        roots: Arc<[AtomicU64]>,
        leaves: Arc<[AtomicU64]>,
        reservations: Arc<[AtomicU64]>,
        workers: Arc<[AtomicU64]>,
        tree: SummaryTree,
    }

    impl TestArena {
        fn new(leaf_count: usize, signals_per_leaf: usize, max_workers: usize) -> Self {
            let root_words = ((leaf_count + 63) / 64).max(1);
            let roots: Arc<[AtomicU64]> = (0..root_words)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>()
                .into();
            let leaves: Arc<[AtomicU64]> = (0..leaf_count)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>()
                .into();
            let reservations: Arc<[AtomicU64]> = (0..leaf_count * signals_per_leaf)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>()
                .into();
            let worker_words = (max_workers + 63) / 64;
            let workers: Arc<[AtomicU64]> = (0..worker_words)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>()
                .into();

            let tree = unsafe {
                SummaryTree::new(SummaryInit {
                    root_words: roots.as_ptr(),
                    root_count: root_words,
                    leaf_words: leaves.as_ptr(),
                    leaf_count,
                    signals_per_leaf,
                    task_reservations: reservations.as_ptr(),
                    worker_bitmap: workers.as_ptr(),
                    worker_bitmap_words: worker_words,
                    max_workers,
                    yield_bit_index: 63,
                })
            };

            Self {
                roots,
                leaves,
                reservations,
                workers,
                tree,
            }
        }

        fn tree(&self) -> &SummaryTree {
            &self.tree
        }
    }

    #[test]
    fn mark_signal_active_updates_root_and_leaf() {
        let arena = TestArena::new(4, 4, 4);
        assert!(arena.tree.mark_signal_active(1, 1));
        assert_eq!(arena.leaves[1].load(Ordering::Relaxed), 1u64 << 1);
        assert_ne!(arena.roots[0].load(Ordering::Relaxed) & (1u64 << 1), 0);

        assert!(!arena.tree.mark_signal_active(1, 1));
        assert!(arena.tree.mark_signal_inactive(1, 1));
        assert_eq!(arena.leaves[1].load(Ordering::Relaxed), 0);
        assert_eq!(arena.roots[0].load(Ordering::Relaxed) & (1u64 << 1), 0);
    }

    #[test]
    fn mark_yield_bits_toggle_activity() {
        let arena = TestArena::new(2, 2, 2);
        let mask = arena.tree.yield_bit_mask();
        assert!(arena.tree.mark_yield_active(0));
        assert_eq!(arena.leaves[0].load(Ordering::Relaxed) & mask, mask);
        assert_ne!(arena.roots[0].load(Ordering::Relaxed) & 1, 0);

        assert!(arena.tree.mark_yield_inactive(0));
        assert_eq!(arena.leaves[0].load(Ordering::Relaxed) & mask, 0);
        assert_eq!(arena.roots[0].load(Ordering::Relaxed) & 1, 0);
    }

    #[test]
    fn mark_signal_inactive_if_empty_clears_summary() {
        let arena = TestArena::new(1, 2, 1);
        assert!(arena.tree.mark_signal_active(0, 1));
        let signal = AtomicU64::new(0);
        arena.tree.mark_signal_inactive_if_empty(0, 1, &signal);
        assert_eq!(arena.leaves[0].load(Ordering::Relaxed), 0);
        assert_eq!(arena.roots[0].load(Ordering::Relaxed), 0);
    }

    #[test]
    fn reserve_task_in_leaf_exhausts_all_bits() {
        let arena = TestArena::new(1, 1, 1);
        let mut bits = Vec::with_capacity(64);
        for _ in 0..64 {
            let bit = arena
                .tree
                .reserve_task_in_leaf(0, 0)
                .expect("expected free bit");
            bits.push(bit);
        }
        bits.sort_unstable();
        assert_eq!(bits, (0..64).collect::<Vec<_>>());
        assert!(
            arena.tree.reserve_task_in_leaf(0, 0).is_none(),
            "all bits should be exhausted"
        );
        for bit in bits {
            arena.tree.release_task_in_leaf(0, 0, bit);
        }
    }

    #[test]
    fn reserve_task_round_robin_visits_all_leaves() {
        let arena = TestArena::new(4, 1, 4);
        let mut observed = Vec::with_capacity(4);
        for _ in 0..4 {
            let (leaf, sig, bit) = arena.tree.reserve_task().expect("reserve task");
            observed.push(leaf);
            arena.tree.release_task_in_leaf(leaf, sig, bit);
        }
        observed.sort_unstable();
        assert_eq!(observed, vec![0, 1, 2, 3]);
    }

    #[test]
    fn summary_select_prefers_nearest_active_leaf() {
        let arena = TestArena::new(8, 1, 4);
        assert!(arena.tree.mark_signal_active(0, 0));
        assert!(arena.tree.mark_signal_active(5, 0));

        let idx0 = arena.tree.summary_select(0);
        assert_eq!(idx0, 0);

        let idx_near_five = arena.tree.summary_select(6);
        assert_eq!(idx_near_five, 5);

        assert!(arena.tree.mark_signal_inactive(0, 0));
        assert!(arena.tree.mark_signal_inactive(5, 0));
    }

    #[test]
    fn concurrent_reservations_are_unique() {
        let arena = Arc::new(TestArena::new(4, 1, 4));
        let threads = 8;
        let reservations_per_thread = 8;
        let barrier = Arc::new(Barrier::new(threads));
        let handles = Arc::new(Mutex::new(Vec::with_capacity(
            threads * reservations_per_thread,
        )));

        let mut join_handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let arena = Arc::clone(&arena);
            let barrier = Arc::clone(&barrier);
            let handles = Arc::clone(&handles);
            join_handles.push(thread::spawn(move || {
                barrier.wait();
                for _ in 0..reservations_per_thread {
                    loop {
                        if let Some(handle) = arena.tree.reserve_task() {
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
            arena.tree.release_task_in_leaf(leaf, signal, bit);
        }
    }

    #[test]
    fn acquire_timeout_unblocks_after_release() {
        let arena = Arc::new(TestArena::new(1, 1, 1));
        let barrier = Arc::new(Barrier::new(2));

        let arena_thread = Arc::clone(&arena);
        let barrier_thread = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            let tree = arena_thread.tree();
            barrier_thread.wait();
            let start = Instant::now();
            let ok = tree.acquire_timeout(Duration::from_millis(200));
            (ok, start.elapsed())
        });

        barrier.wait();
        thread::sleep(Duration::from_millis(50));
        arena.tree.release(1);

        let (ok, elapsed) = handle.join().expect("thread panicked");
        assert!(ok, "acquire_timeout should succeed after release");
        assert!(
            elapsed < Duration::from_millis(200),
            "acquire should unblock before timeout"
        );
        assert_eq!(arena.tree.sleepers(), 0);
    }

    #[test]
    fn reserve_worker_assigns_unique_slots() {
        let arena = TestArena::new(8, 2, 8);
        let mut slots = Vec::new();
        for _ in 0..8 {
            let slot = arena.tree.reserve_worker().expect("worker slot");
            slots.push(slot);
        }
        slots.sort_unstable();
        assert_eq!(slots, (0..8).collect::<Vec<_>>());
        assert!(arena.tree.reserve_worker().is_none());
        for slot in slots {
            arena.tree.release_worker(slot);
        }
        assert_eq!(arena.tree.worker_count(), 0);
    }

    #[test]
    fn reserve_and_release_task_updates_reservations() {
        let arena = TestArena::new(4, 1, 4);
        let handle = arena.tree.reserve_task().expect("task handle");
        assert_eq!(handle.1, 0); // signal idx

        let reservation = arena
            .reservations
            .get(handle.0 * 1 + handle.1)
            .unwrap()
            .load(Ordering::Relaxed);
        assert_ne!(reservation, 0);

        arena
            .tree
            .release_task_in_leaf(handle.0, handle.1, handle.2);
        let reservation = arena
            .reservations
            .get(handle.0 * 1 + handle.1)
            .unwrap()
            .load(Ordering::Relaxed);
        assert_eq!(reservation, 0);
    }
}
