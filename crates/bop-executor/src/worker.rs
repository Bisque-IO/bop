use crate::bits;
use crate::deque::{StealStatus, Stealer, Worker as QueueWorker};
use crate::task::{FutureHelpers, MmapExecutorArena, TaskHandle};
use std::hint::spin_loop;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use std::time::Duration;

use rand::RngCore;

const RND_MULTIPLIER: u64 = 0x5DEECE66D;
const RND_ADDEND: u64 = 0xB;
const RND_MASK: u64 = (1 << 48) - 1;
const DEFAULT_WAKE_BURST: usize = 4;
const FULL_SUMMARY_SCAN_CADENCE_MASK: u64 = 1023;

/// Comprehensive statistics for fast worker.
#[derive(Debug, Default, Clone)]
pub struct WorkerStats {
    /// Number of tasks polled.
    pub tasks_polled: u64,

    /// Number of tasks that completed.
    pub completed_count: u64,

    /// Number of tasks that yielded cooperatively.
    pub yielded_count: u64,

    /// Number of tasks that are waiting (Poll::Pending, not yielded).
    pub waiting_count: u64,

    /// Number of CAS failures (contention or already executing).
    pub cas_failures: u64,

    /// Number of empty scans (no tasks available).
    pub empty_scans: u64,

    /// Number of polls from yield queue (hot path).
    pub yield_queue_polls: u64,

    /// Number of polls from signals (cold path).
    pub signal_polls: u64,

    /// Number of work stealing attempts from other workers.
    pub steal_attempts: u64,

    /// Number of successful work steals.
    pub steal_successes: u64,

    /// Number of leaf summary checks.
    pub leaf_summary_checks: u64,

    /// Number of leaf summary hits (summary != 0).
    pub leaf_summary_hits: u64,

    /// Number of attempts to steal from other workers' leaf partitions.
    pub leaf_steal_attempts: u64,

    /// Number of successful steals from other workers' leaf partitions.
    pub leaf_steal_successes: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WakeStats {
    pub release_calls: u64,
    pub released_permits: u64,
    pub last_backlog: usize,
    pub max_backlog: usize,
    pub queue_release_calls: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct WaitStrategy {
    pub spin_before_sleep: usize,
    pub park_timeout: Option<Duration>,
}

impl WaitStrategy {
    pub fn new(spin_before_sleep: usize, park_timeout: Option<Duration>) -> Self {
        Self {
            spin_before_sleep,
            park_timeout,
        }
    }

    pub fn non_blocking() -> Self {
        Self::new(0, Some(Duration::from_secs(0)))
    }

    pub fn park_immediately() -> Self {
        Self::new(0, None)
    }
}

impl Default for WaitStrategy {
    fn default() -> Self {
        Self::new(64, Some(Duration::from_millis(1)))
    }
}

pub struct Worker {
    arena: Arc<MmapExecutorArena>,
    worker_slot: usize,
    yield_queue: QueueWorker<TaskHandle>,
    yield_stealers: Vec<Stealer<TaskHandle>>,
    partition_start: usize,
    partition_end: usize,
    cached_worker_count: usize,
    wake_burst_limit: usize,
    wake_stats: WakeStats,
    stats: WorkerStats,
    pub seed: u64,
}

impl Worker {
    pub fn new(arena: Arc<MmapExecutorArena>) -> Self {
        let slot = arena
            .reserve_worker()
            .expect("failed to reserve worker slot");
        let leaf_count = arena.leaf_count();
        let worker_count = arena.active_tree().worker_count().max(1);
        let (partition_start, partition_end) =
            Self::compute_partition(slot, leaf_count, worker_count);
        Self {
            arena,
            worker_slot: slot,
            yield_queue: QueueWorker::new_lifo(),
            yield_stealers: Vec::new(),
            partition_start,
            partition_end,
            cached_worker_count: worker_count,
            wake_burst_limit: DEFAULT_WAKE_BURST,
            wake_stats: WakeStats::default(),
            stats: WorkerStats::default(),
            seed: rand::rng().next_u64(),
        }
    }

    #[inline(always)]
    pub fn yield_stealer(&self) -> Stealer<TaskHandle> {
        self.yield_queue.stealer()
    }

    #[inline(always)]
    pub fn stats(&self) -> &WorkerStats {
        &self.stats
    }

    #[inline(always)]
    pub fn stats_mut(&mut self) -> &mut WorkerStats {
        &mut self.stats
    }

    #[inline(always)]
    pub fn set_yield_stealers(&mut self, stealers: Vec<Stealer<TaskHandle>>) {
        self.yield_stealers = stealers;
    }

    #[inline(always)]
    pub fn set_wake_burst_limit(&mut self, limit: usize) {
        self.wake_burst_limit = limit.max(1);
    }

    #[inline(always)]
    pub fn wake_stats(&self) -> WakeStats {
        self.wake_stats
    }

    #[inline(always)]
    pub fn queue_depth(&self) -> usize {
        self.yield_queue.len()
    }

    #[inline(always)]
    pub fn next_u64(&mut self) -> u64 {
        let old_seed = self.seed;
        let next_seed = (old_seed
            .wrapping_mul(RND_MULTIPLIER)
            .wrapping_add(RND_ADDEND))
            & RND_MASK;
        self.seed = next_seed;
        next_seed >> 16
    }

    #[inline(always)]
    pub fn run_once(&mut self) -> bool {
        let leaf_count = self.arena.leaf_count();
        if leaf_count == 0 {
            return false;
        }

        let mut did_work = false;

        // if self.poll_yield(self.yield_queue.len()) > 0 {
        if self.poll_yield(4) > 0 {
            did_work = true;
        }

        self.refresh_partition(leaf_count);

        if self.try_partition_random(leaf_count) {
            did_work = true;
        }

        // if self.try_partition_random(leaf_count) {
        //     did_work = true;
        // } else if self.try_partition_linear(leaf_count) {
        //     did_work = true;
        // }

        let mask = leaf_count - 1;
        let rand = self.next_u64();

        if !did_work || rand & FULL_SUMMARY_SCAN_CADENCE_MASK == 0 {
            // if self.try_any_partition_random(leaf_count) {
            //     did_work = true;
            // }

            if self.try_any_partition_random(leaf_count) {
                did_work = true;
            } else if self.try_any_partition_linear(leaf_count) {
                did_work = true;
            }

            // if self.poll_yield_steal(1) > 0 {
            //     did_work = true;
            // }
        }

        if !did_work {
            if self.poll_yield(self.yield_queue.len() as usize) > 0 {
                did_work = true;
            }

            // if self.poll_yield_steal(1) > 0 {
            //     did_work = true;
            // }
        }

        did_work
    }

    #[inline(always)]
    pub fn poll_yield(&mut self, max: usize) -> usize {
        let mut count = 0;
        while let Some(handle) = self.try_acquire_local_yield() {
            self.stats.yield_queue_polls += 1;
            self.poll_handle(handle);
            count += 1;
            if count >= max {
                break;
            }
        }
        count
    }

    #[inline(always)]
    pub fn poll_yield_steal(&mut self, max: usize) -> usize {
        let mut count = 0;
        while let Some(handle) = self.try_steal_yielded() {
            self.stats.yield_queue_polls += 1;
            self.poll_handle(handle);
            count += 1;
            if count >= max {
                break;
            }
        }
        count
    }

    #[inline(always)]
    pub fn run_until_idle(&mut self) -> usize {
        let mut processed = 0;
        while self.run_once() {
            processed += 1;
        }
        // processed += self.poll_yield_steal(32);
        // while self.run_once() {
        //     processed += 1;
        // }
        processed
    }

    #[inline(always)]
    pub fn poll_blocking(&mut self, strategy: &WaitStrategy) -> bool {
        let mut spins = 0usize;
        loop {
            if self.run_once() {
                return true;
            }

            if strategy.spin_before_sleep > 0 && spins < strategy.spin_before_sleep {
                spins += 1;
                spin_loop();
                continue;
            }

            spins = 0;

            match strategy.park_timeout {
                Some(timeout) => {
                    if timeout.is_zero() {
                        return false;
                    }
                    if !self.arena.active_tree().acquire_timeout(timeout) {
                        return false;
                    }
                }
                None => {
                    self.arena.active_tree().acquire();
                }
            }
        }
    }

    pub fn run_blocking(&mut self, strategy: &WaitStrategy) {
        while self.poll_blocking(strategy) {}
    }

    #[inline(always)]
    fn try_acquire_local_yield(&mut self) -> Option<TaskHandle> {
        let (item, was_last) = self.yield_queue.pop_with_status();
        if let Some(handle) = item {
            if was_last {
                self.arena.mark_yield_inactive(handle.leaf_idx());
            }
            return Some(handle);
        }
        None
    }

    #[inline(always)]
    fn try_steal_yielded(&mut self) -> Option<TaskHandle> {
        let stealers: Vec<_> = self.yield_stealers.iter().cloned().collect();
        for stealer in stealers {
            self.stats.steal_attempts += 1;
            match stealer.steal_with_status() {
                StealStatus::Success { task, was_last } => {
                    self.stats.steal_successes += 1;
                    if was_last {
                        self.arena.mark_yield_inactive(task.leaf_idx());
                    }
                    return Some(task);
                }
                StealStatus::Retry => continue,
                StealStatus::Empty => continue,
            }
        }
        None
    }

    #[inline(always)]
    fn process_leaf(&mut self, leaf_idx: usize) -> bool {
        if let Some(handle) = self.try_acquire_task(leaf_idx) {
            self.poll_handle(handle);
            return true;
        }
        false
    }

    #[inline(always)]
    fn maybe_release_for_backlog(&mut self, leaf_idx: usize, signal_idx: usize, remaining: u64) {
        let sleepers = self.arena.active_tree().sleepers();
        if sleepers == 0 {
            return;
        }

        let mut backlog = remaining.count_ones() as usize;

        let summary =
            self.arena.active_summary(leaf_idx).load(Ordering::Relaxed) & !(1u64 << signal_idx);
        backlog += summary.count_ones() as usize;
        backlog += self.yield_queue.len();

        self.wake_stats.last_backlog = backlog;
        if backlog > self.wake_stats.max_backlog {
            self.wake_stats.max_backlog = backlog;
        }

        let to_release = backlog.min(sleepers).min(self.wake_burst_limit);
        if to_release > 0 {
            self.wake_stats.release_calls += 1;
            self.wake_stats.released_permits += to_release as u64;
            self.arena.active_tree().release(to_release);
        }
    }

    #[inline(always)]
    fn maybe_release_for_queue(&mut self, _was_empty: bool) {
        let sleepers = self.arena.active_tree().sleepers();
        if sleepers == 0 {
            return;
        }
        let backlog = self.yield_queue.len();

        self.wake_stats.last_backlog = backlog;
        if backlog > self.wake_stats.max_backlog {
            self.wake_stats.max_backlog = backlog;
        }

        let to_release = backlog.min(sleepers).min(self.wake_burst_limit);
        if to_release > 0 {
            self.wake_stats.queue_release_calls += 1;
            self.wake_stats.released_permits += to_release as u64;
            self.arena.active_tree().release(to_release);
        }
    }

    #[inline(always)]
    fn try_partition_random(&mut self, leaf_count: usize) -> bool {
        let partition_len = self.partition_end.saturating_sub(self.partition_start);
        if partition_len == 0 {
            return false;
        }

        for _ in 0..partition_len {
            let start_offset = self.next_u64() as usize % partition_len;
            let leaf_idx = self.partition_start + start_offset;
            if self.process_leaf(leaf_idx) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn try_partition_linear(&mut self, leaf_count: usize) -> bool {
        let partition_len = self.partition_end.saturating_sub(self.partition_start);
        if partition_len == 0 {
            return false;
        }

        for offset in 0..partition_len {
            let leaf_idx = self.partition_start + offset;
            if self.process_leaf(leaf_idx) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn try_any_partition_random(&mut self, leaf_count: usize) -> bool {
        let leaf_mask = leaf_count - 1;
        for _ in 0..leaf_count {
            let leaf_idx = self.next_u64() as usize & leaf_mask;
            self.stats.leaf_steal_attempts += 1;
            if self.process_leaf(leaf_idx) {
                self.stats.leaf_steal_successes += 1;
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn try_any_partition_linear(&mut self, leaf_count: usize) -> bool {
        for leaf_idx in 0..leaf_count {
            self.stats.leaf_steal_attempts += 1;
            if self.process_leaf(leaf_idx) {
                self.stats.leaf_steal_successes += 1;
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn refresh_partition(&mut self, leaf_count: usize) {
        let worker_count = self.arena.active_tree().worker_count().max(1);
        if worker_count == self.cached_worker_count {
            return;
        }

        let (start, end) = Self::compute_partition(self.worker_slot, leaf_count, worker_count);
        self.partition_start = start.min(leaf_count);
        self.partition_end = end.min(leaf_count);
        self.cached_worker_count = worker_count;
    }

    #[inline(always)]
    fn compute_partition(
        worker_slot: usize,
        leaf_count: usize,
        worker_count: usize,
    ) -> (usize, usize) {
        if leaf_count == 0 || worker_count == 0 {
            return (0, 0);
        }
        let effective_idx = worker_slot % worker_count;
        let base = leaf_count / worker_count;
        let extra = leaf_count % worker_count;

        let start = if effective_idx < extra {
            effective_idx * (base + 1)
        } else {
            extra * (base + 1) + (effective_idx - extra) * base
        };
        let len = if effective_idx < extra {
            base + 1
        } else {
            base
        };
        (start, (start + len).min(leaf_count))
    }

    #[inline(always)]
    fn try_acquire_task(&mut self, leaf_idx: usize) -> Option<TaskHandle> {
        let signals_per_leaf = self.arena.signals_per_leaf();
        if signals_per_leaf == 0 {
            return None;
        }

        let mask = if signals_per_leaf >= 64 {
            u64::MAX
        } else {
            (1u64 << signals_per_leaf) - 1
        };

        self.stats.leaf_summary_checks += 1;
        let mut available = self.arena.active_summary(leaf_idx).load(Ordering::Acquire) & mask;
        if available == 0 {
            self.stats.empty_scans += 1;
            return None;
        }
        self.stats.leaf_summary_hits += 1;

        let signals = self.arena.active_signals(leaf_idx);
        let mut attempts = signals_per_leaf;

        while available != 0 && attempts > 0 {
            let start = (self.next_u64() as usize) % signals_per_leaf;

            let (signal_idx, signal) = loop {
                let candidate = bits::find_nearest(available, start as u64);
                if candidate >= 64 {
                    self.stats.leaf_summary_checks += 1;
                    available = self.arena.active_summary(leaf_idx).load(Ordering::Acquire) & mask;
                    if available == 0 {
                        self.stats.empty_scans += 1;
                        return None;
                    }
                    self.stats.leaf_summary_hits += 1;
                    continue;
                }
                let bit_index = candidate as usize;
                let sig = unsafe { &*self.arena.task_signal_ptr(leaf_idx, bit_index) };
                let bits = sig.load(Ordering::Acquire);
                if bits == 0 {
                    available &= !(1u64 << bit_index);
                    self.arena
                        .active_tree()
                        .mark_signal_inactive(leaf_idx, bit_index);
                    attempts -= 1;
                    continue;
                }
                break (bit_index, sig);
            };

            let bits = signal.load(Ordering::Acquire);
            let bit_seed = (self.next_u64() & 63) as u64;
            let bit_candidate = bits::find_nearest(bits, bit_seed);
            let bit_idx = if bit_candidate < 64 {
                bit_candidate as u32
            } else {
                available &= !(1u64 << signal_idx);
                attempts -= 1;
                continue;
            };

            let (remaining, acquired) = signal.try_acquire(bit_idx);
            if !acquired {
                self.stats.cas_failures += 1;
                available &= !(1u64 << signal_idx);
                attempts -= 1;
                continue;
            }

            let remaining_mask = remaining;
            if remaining_mask == 0 {
                self.arena
                    .active_tree()
                    .mark_signal_inactive(leaf_idx, signal_idx);
            }
            self.maybe_release_for_backlog(leaf_idx, signal_idx, remaining_mask);

            self.stats.signal_polls += 1;
            return Some(TaskHandle::new(leaf_idx, signal_idx, bit_idx));
        }

        self.stats.empty_scans += 1;
        None
    }

    #[inline(always)]
    fn enqueue_yield(&mut self, handle: TaskHandle) {
        let leaf_idx = handle.leaf_idx();
        let was_empty = self.yield_queue.push_with_status(handle);
        if was_empty {
            self.arena.mark_yield_active(leaf_idx);
        }
        self.maybe_release_for_queue(was_empty);
    }

    #[inline(always)]
    fn poll_handle(&mut self, handle: TaskHandle) {
        let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
        let task = unsafe { self.arena.task(handle.leaf_idx(), slot_idx) };

        self.stats.tasks_polled += 1;

        if !task.is_yielded() {
            task.begin();
        } else {
            task.clear_yielded();
        }

        let waker = unsafe { task.waker_yield() };
        let mut cx = Context::from_waker(&waker);
        let poll_result = unsafe { task.poll_future(&mut cx) };

        match poll_result {
            Some(Poll::Ready(())) => {
                self.stats.completed_count += 1;
                if let Some(ptr) = task.take_future() {
                    unsafe { FutureHelpers::drop_boxed(ptr) };
                }
                task.finish();
            }
            Some(Poll::Pending) => {
                if task.is_yielded() {
                    self.stats.yielded_count += 1;
                    // Yielded Tasks stay in EXECUTING state with yielded set to true
                    // without resetting the signal. All attempts to set the signal
                    // will not set it since it is guaranteed to run via the yield queue.
                    self.enqueue_yield(handle);
                } else {
                    self.stats.waiting_count += 1;
                    task.finish();
                }
            }
            None => {
                self.stats.completed_count += 1;
                task.finish();
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        loop {
            let (item, was_last) = self.yield_queue.pop_with_status();
            match item {
                Some(handle) => {
                    if was_last {
                        self.arena.mark_yield_inactive(handle.leaf_idx());
                    }
                }
                None => break,
            }
        }
        self.arena.release_worker(self.worker_slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{ArenaConfig, ArenaOptions, FutureHelpers, Task};
    use std::future::{Future, poll_fn};
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::thread::yield_now;
    use std::time::{Duration, Instant};

    async fn incr(counter: Arc<AtomicUsize>) {
        counter.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn build_arena(leaf_count: usize, tasks_per_leaf: usize) -> Arc<MmapExecutorArena> {
        Arc::new(
            MmapExecutorArena::with_config(
                ArenaConfig::new(leaf_count, tasks_per_leaf).unwrap(),
                ArenaOptions::default(),
            )
            .unwrap(),
        )
    }

    #[test]
    fn worker_stats_default_is_zeroed() {
        let stats = WorkerStats::default();
        assert_eq!(stats.tasks_polled, 0);
        assert_eq!(stats.completed_count, 0);
        assert_eq!(stats.yielded_count, 0);
        assert_eq!(stats.waiting_count, 0);
        assert_eq!(stats.signal_polls, 0);
        assert_eq!(stats.steal_attempts, 0);
    }

    fn task_by_handle<'a>(arena: &'a Arc<MmapExecutorArena>, handle: &TaskHandle) -> &'a Task {
        let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
        unsafe { arena.task(handle.leaf_idx(), slot_idx) }
    }

    fn prepare_task<F>(arena: &Arc<MmapExecutorArena>, future: F) -> TaskHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let handle = arena.reserve_task().expect("reserve task");
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let future_ptr = FutureHelpers::box_future(future);
        let task = task_by_handle(arena, &handle);
        task.attach_future(future_ptr).unwrap();
        arena.activate_task(handle);
        handle
    }

    fn drop_future(arena: &Arc<MmapExecutorArena>, handle: &TaskHandle) {
        let task = task_by_handle(arena, handle);
        if let Some(ptr) = task.take_future() {
            unsafe { FutureHelpers::drop_boxed(ptr) };
        }
    }

    #[test]
    fn worker_polls_tasks() {
        let config = ArenaConfig::new(4, 64).unwrap();
        let arena =
            Arc::new(MmapExecutorArena::with_config(config, ArenaOptions::default()).unwrap());

        let counter = Arc::new(AtomicUsize::new(0));
        let total = arena.leaf_count() * arena.tasks_per_leaf();
        let mut handles = Vec::with_capacity(total);

        for _ in 0..total {
            let handle = arena.reserve_task().expect("exhaust tasks");
            let global = handle.global_id(arena.tasks_per_leaf());
            arena.init_task(global);
            let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
            let task = unsafe { arena.task(handle.leaf_idx(), slot_idx) };
            let future_ptr = FutureHelpers::box_future(incr(counter.clone()));
            task.attach_future(future_ptr).unwrap();
            arena.activate_task(handle);
            handles.push(handle);
        }

        let mut worker = Worker::new(Arc::clone(&arena));
        worker.run_until_idle();
        let strategy = WaitStrategy::non_blocking();
        assert!(!worker.poll_blocking(&strategy));

        assert_eq!(counter.load(AtomicOrdering::Relaxed), total);

        let stats = worker.wake_stats();
        assert!(stats.max_backlog >= stats.last_backlog);

        let worker_stats = worker.stats();
        assert_eq!(worker_stats.tasks_polled as usize, total);
        assert_eq!(worker_stats.completed_count as usize, total);
        assert_eq!(worker_stats.signal_polls as usize, total);
        assert_eq!(worker_stats.yielded_count, 0);
        assert_eq!(worker_stats.waiting_count, 0);

        for handle in handles {
            arena.release_task(handle);
        }
    }

    #[test]
    fn stats_record_mixed_outcomes() {
        let arena = build_arena(1, 64);
        let mut worker = Worker::new(Arc::clone(&arena));

        let ready_counter = Arc::new(AtomicUsize::new(0));
        let ready_handle = prepare_task(&arena, {
            let ready_counter = Arc::clone(&ready_counter);
            async move {
                ready_counter.fetch_add(1, AtomicOrdering::Relaxed);
            }
        });

        let pending_state = Arc::new(AtomicUsize::new(0));
        let pending_handle = prepare_task(&arena, {
            let pending_state = Arc::clone(&pending_state);
            poll_fn(move |_cx| {
                let prev = pending_state.fetch_add(1, AtomicOrdering::Relaxed);
                if prev == 0 {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        });

        let yield_state = Arc::new(AtomicUsize::new(0));
        let yield_handle = prepare_task(&arena, {
            let yield_state_cloned = Arc::clone(&yield_state);
            poll_fn(
                move |cx| match yield_state_cloned.load(AtomicOrdering::Relaxed) {
                    0 => {
                        yield_state_cloned.store(1, AtomicOrdering::Relaxed);
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    _ => {
                        yield_state_cloned.store(2, AtomicOrdering::Relaxed);
                        Poll::Ready(())
                    }
                },
            )
        });

        worker.run_until_idle();

        let stats = worker.stats();
        assert_eq!(stats.tasks_polled, 4);
        assert_eq!(stats.completed_count, 2);
        assert_eq!(stats.yielded_count, 1);
        assert_eq!(stats.waiting_count, 1);
        assert_eq!(stats.yield_queue_polls, 1);
        assert_eq!(stats.signal_polls, 3);
        assert!(stats.empty_scans >= 1);

        drop_future(&arena, &pending_handle);
        arena.release_task(ready_handle);
        arena.release_task(pending_handle);
        arena.release_task(yield_handle);
    }

    #[test]
    fn poll_blocking_wakes_on_task_activation() {
        let config = ArenaConfig::new(1, 64).unwrap();
        let arena =
            Arc::new(MmapExecutorArena::with_config(config, ArenaOptions::default()).unwrap());

        let counter = Arc::new(AtomicUsize::new(0));
        let handle = arena.reserve_task().expect("reserve task");
        let leaf = handle.leaf_idx();
        let signal = handle.signal_idx();
        let bit = handle.bit_idx();
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let slot_idx = signal * 64 + bit as usize;
        let task = unsafe { arena.task(leaf, slot_idx) };
        let future_ptr = FutureHelpers::box_future(incr(counter.clone()));
        task.attach_future(future_ptr).unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let strategy = WaitStrategy::new(0, Some(Duration::from_millis(200)));
        let arena_for_thread = Arc::clone(&arena);
        let barrier_for_thread = Arc::clone(&barrier);

        let start = Instant::now();
        let worker_thread = thread::spawn(move || {
            let mut worker = Worker::new(arena_for_thread);
            barrier_for_thread.wait();
            let did_work = worker.poll_blocking(&strategy);
            assert!(did_work, "worker should observe newly activated task");
            worker.run_until_idle();
            (worker.worker_slot, start.elapsed())
        });

        barrier.wait();
        thread::sleep(Duration::from_millis(20));

        let activation_handle = TaskHandle::new(leaf, signal, bit);
        arena.activate_task(activation_handle);

        let (slot, elapsed) = worker_thread.join().expect("worker thread panicked");
        assert!(
            elapsed < Duration::from_millis(200),
            "poll_blocking should wake before timing out"
        );
        assert_eq!(
            counter.load(AtomicOrdering::Relaxed),
            1,
            "future executes exactly once"
        );
        arena.release_task(activation_handle);
        arena.release_worker(slot);
    }

    #[test]
    fn try_acquire_task_handles_stale_summary_bit() {
        let config = ArenaConfig::new(1, 64).unwrap();
        let arena =
            Arc::new(MmapExecutorArena::with_config(config, ArenaOptions::default()).unwrap());

        // Manually mark the summary bit as active without setting any task signal bits.
        assert!(arena.active_tree().mark_signal_active(0, 0));
        let signal = unsafe { &*arena.active_signals(0) };
        assert_eq!(
            signal.load(AtomicOrdering::Relaxed),
            0,
            "signal must stay empty to emulate a drained queue"
        );

        let mut worker = Worker::new(Arc::clone(&arena));
        worker.seed = 0;

        let handle = worker.try_acquire_task(0);
        assert!(
            handle.is_none(),
            "stale summaries must not yield task handles when the signal word is empty"
        );
        // The worker should clear the stale summary bit.
        assert_eq!(
            arena.active_summary(0).load(AtomicOrdering::Relaxed) & 1,
            0,
            "stale summary bit should get cleared after inspection"
        );
    }

    #[test]
    fn run_once_without_tasks_is_idle() {
        let config = ArenaConfig::new(1, 64).unwrap();
        let arena =
            Arc::new(MmapExecutorArena::with_config(config, ArenaOptions::default()).unwrap());
        let mut worker = Worker::new(Arc::clone(&arena));
        worker.seed = 0;

        assert!(
            !worker.run_once(),
            "worker without active signals should not report work"
        );
    }

    #[test]
    fn compute_partition_distributes_leaves_evenly() {
        // Degenerate case: no leaves or workers.
        assert_eq!(Worker::compute_partition(0, 0, 0), (0, 0));

        // Single worker owns the full range.
        assert_eq!(Worker::compute_partition(0, 8, 1), (0, 8));

        // Two workers over five leaves: worker 0 gets 3, worker 1 gets 2.
        assert_eq!(Worker::compute_partition(0, 5, 2), (0, 3));
        assert_eq!(Worker::compute_partition(1, 5, 2), (3, 5));

        // Worker slots wrap modulo worker_count.
        assert_eq!(Worker::compute_partition(4, 6, 3), (2, 4));
        assert_eq!(Worker::compute_partition(5, 6, 3), (4, 6));
    }

    #[test]
    fn multiple_workers_process_all_tasks() {
        let config = ArenaConfig::new(4, 64).unwrap().with_max_workers(4);
        let arena =
            Arc::new(MmapExecutorArena::with_config(config, ArenaOptions::default()).unwrap());

        let counter = Arc::new(AtomicUsize::new(0));
        let tasks_to_schedule = 64usize;
        let mut handles_meta = Vec::with_capacity(tasks_to_schedule);

        for _ in 0..tasks_to_schedule {
            let handle = arena.reserve_task().expect("reserve task");
            let leaf = handle.leaf_idx();
            let signal = handle.signal_idx();
            let bit = handle.bit_idx();
            let global = handle.global_id(arena.tasks_per_leaf());
            arena.init_task(global);
            let slot_idx = signal * 64 + bit as usize;
            let task = unsafe { arena.task(leaf, slot_idx) };
            let future_ptr = FutureHelpers::box_future(incr(counter.clone()));
            task.attach_future(future_ptr).unwrap();
            arena.activate_task(handle);
            handles_meta.push((leaf, signal, bit));
        }

        let mut worker_a = Worker::new(Arc::clone(&arena));
        let mut worker_b = Worker::new(Arc::clone(&arena));

        let stealer_a = worker_a.yield_stealer();
        let stealer_b = worker_b.yield_stealer();
        worker_a.set_yield_stealers(vec![stealer_b.clone()]);
        worker_b.set_yield_stealers(vec![stealer_a.clone()]);

        let total = tasks_to_schedule;
        let counter_a = Arc::clone(&counter);
        let counter_b = Arc::clone(&counter);

        let thread_a = thread::spawn(move || {
            let mut idle_spins = 0u32;
            while counter_a.load(AtomicOrdering::Acquire) < total {
                if !worker_a.run_once() {
                    idle_spins += 1;
                    if idle_spins % 256 == 0 {
                        yield_now();
                    }
                } else {
                    idle_spins = 0;
                }
            }
            worker_a.run_until_idle();
        });

        let thread_b = thread::spawn(move || {
            let mut idle_spins = 0u32;
            while counter_b.load(AtomicOrdering::Acquire) < total {
                if !worker_b.run_once() {
                    idle_spins += 1;
                    if idle_spins % 256 == 0 {
                        yield_now();
                    }
                } else {
                    idle_spins = 0;
                }
            }
            worker_b.run_until_idle();
        });

        thread_a.join().expect("worker A panic");
        thread_b.join().expect("worker B panic");

        assert_eq!(
            counter.load(AtomicOrdering::Relaxed),
            total,
            "all tasks should be processed exactly once"
        );
        assert_eq!(
            arena.active_tree().worker_count(),
            0,
            "worker slots should be released on drop"
        );

        for (leaf, signal, bit) in handles_meta {
            let handle = TaskHandle::new(leaf, signal, bit);
            arena.release_task(handle);
        }
    }

    #[test]
    fn worker_drop_releases_slot_and_clears_yield() {
        let config = ArenaConfig::new(1, 64).unwrap();
        let arena =
            Arc::new(MmapExecutorArena::with_config(config, ArenaOptions::default()).unwrap());

        let slot_index;
        let handle_bits;
        {
            let mut worker = Worker::new(Arc::clone(&arena));
            slot_index = worker.worker_slot;

            let handle = arena.reserve_task().expect("reserve task");
            let leaf = handle.leaf_idx();
            let signal = handle.signal_idx();
            let bit = handle.bit_idx();
            handle_bits = (leaf, signal, bit);

            arena.init_task(handle.global_id(arena.tasks_per_leaf()));
            worker.enqueue_yield(handle);
        }

        assert_eq!(
            arena.active_tree().worker_count(),
            0,
            "worker drop should release its slot"
        );
        let new_slot = arena.reserve_worker().expect("slot should be reusable");
        assert_eq!(new_slot, slot_index, "same slot should become available");
        arena.release_worker(new_slot);

        let (leaf, signal, bit) = handle_bits;
        arena.release_task(TaskHandle::new(leaf, signal, bit));
    }

    #[test]
    fn yield_queue_polls_increment_after_requeue() {
        let arena = build_arena(1, 64);
        let mut worker = Worker::new(Arc::clone(&arena));

        let yield_state = Arc::new(AtomicUsize::new(0));
        let handle = prepare_task(&arena, {
            let yield_state_cloned = Arc::clone(&yield_state);
            poll_fn(
                move |cx| match yield_state_cloned.load(AtomicOrdering::Relaxed) {
                    0 => {
                        yield_state_cloned.store(1, AtomicOrdering::Relaxed);
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    _ => {
                        yield_state_cloned.store(2, AtomicOrdering::Relaxed);
                        Poll::Ready(())
                    }
                },
            )
        });

        worker.run_until_idle();

        let stats = worker.stats();
        assert_eq!(stats.yielded_count, 1);
        assert_eq!(stats.yield_queue_polls, 1);
        assert_eq!(stats.tasks_polled, 2);

        arena.release_task(handle);
    }

    #[test]
    fn poll_yield_steal_counts_success() {
        let arena = build_arena(2, 64);
        let mut worker_a = Worker::new(Arc::clone(&arena));
        let mut worker_b = Worker::new(Arc::clone(&arena));

        let stealer_from_a = worker_a.yield_stealer();
        worker_b.set_yield_stealers(vec![stealer_from_a]);

        let handle = prepare_task(&arena, async {});
        let leaf = handle.leaf_idx();
        let signal = handle.signal_idx();
        let bit = handle.bit_idx();

        worker_a.enqueue_yield(handle);

        let stolen = worker_b.poll_yield_steal(1);
        assert_eq!(stolen, 1);

        let stats = worker_b.stats();
        assert_eq!(stats.steal_attempts, 1);
        assert_eq!(stats.steal_successes, 1);
        assert_eq!(stats.yield_queue_polls, 1);

        arena.release_task(TaskHandle::new(leaf, signal, bit));
    }

    #[test]
    fn poll_yield_steal_counts_attempt_when_empty() {
        let arena = build_arena(1, 64);
        let mut worker = Worker::new(Arc::clone(&arena));
        let stealer = worker.yield_stealer();
        worker.set_yield_stealers(vec![stealer]);

        assert_eq!(worker.poll_yield_steal(1), 0);

        let stats = worker.stats();
        assert_eq!(stats.steal_attempts, 1);
        assert_eq!(stats.steal_successes, 0);
        assert_eq!(stats.yield_queue_polls, 0);
    }

    #[test]
    fn poll_yield_empty_returns_zero() {
        let arena = build_arena(1, 64);
        let mut worker = Worker::new(Arc::clone(&arena));

        assert_eq!(worker.poll_yield(4), 0);

        let stats = worker.stats();
        assert_eq!(stats.yield_queue_polls, 0);
        assert_eq!(stats.tasks_polled, 0);
    }

    #[test]
    fn try_acquire_local_yield_clears_yield_bit() {
        let arena = build_arena(1, 64);
        let mut worker = Worker::new(Arc::clone(&arena));

        let handle = prepare_task(&arena, async {});
        let leaf = handle.leaf_idx();
        let signal = handle.signal_idx();
        let bit = handle.bit_idx();

        worker.enqueue_yield(handle);
        let summary_before = arena.active_summary(leaf).load(AtomicOrdering::Relaxed);
        assert_ne!(summary_before & arena.active_tree().yield_bit_mask(), 0);

        let acquired = worker
            .try_acquire_local_yield()
            .expect("expected to acquire handle");
        worker.poll_handle(acquired);

        let summary_after = arena.active_summary(leaf).load(AtomicOrdering::Relaxed);
        assert_eq!(
            summary_after & arena.active_tree().yield_bit_mask(),
            0,
            "yield bit should be cleared after draining queue"
        );

        arena.release_task(TaskHandle::new(leaf, signal, bit));
    }

    #[test]
    fn try_any_partition_linear_records_steal_stats() {
        let arena = build_arena(4, 64);
        let mut worker = Worker::new(Arc::clone(&arena));

        worker.partition_start = 0;
        worker.partition_end = 0;

        let handle = prepare_task(&arena, async {});
        assert!(worker.try_any_partition_linear(arena.leaf_count()));

        let stats = worker.stats();
        assert!(stats.leaf_steal_attempts >= 1);
        assert!(stats.leaf_steal_successes >= 1);

        arena.release_task(handle);
    }

    #[test]
    fn try_any_partition_random_records_steal_stats() {
        let arena = build_arena(4, 64);
        let mut worker = Worker::new(Arc::clone(&arena));

        worker.partition_start = 0;
        worker.partition_end = 0;

        let handle = prepare_task(&arena, async {});
        assert!(worker.try_any_partition_random(arena.leaf_count()));

        let stats = worker.stats();
        assert!(stats.leaf_steal_attempts >= 1);
        assert!(stats.leaf_steal_successes >= 1);

        arena.release_task(handle);
    }

    #[test]
    fn run_once_triggers_full_scan_cadence() {
        let arena = build_arena(4, 64);
        let mut worker = Worker::new(Arc::clone(&arena));

        worker.partition_start = 0;
        worker.partition_end = 0;
        worker.seed = 0;

        let handle = prepare_task(&arena, async {});
        assert!(worker.run_once());

        let stats = worker.stats();
        assert!(stats.leaf_steal_attempts >= 1);
        assert_eq!(stats.tasks_polled, 1);

        arena.release_task(handle);
    }

    #[test]
    fn run_until_idle_returns_zero_when_no_work() {
        let arena = build_arena(1, 64);
        let mut worker = Worker::new(Arc::clone(&arena));

        assert_eq!(worker.run_until_idle(), 0);
        assert_eq!(worker.stats().tasks_polled, 0);
    }

    #[test]
    fn poll_blocking_non_blocking_returns_false_when_idle() {
        let arena = build_arena(1, 64);
        let mut worker = Worker::new(Arc::clone(&arena));
        let strategy = WaitStrategy::non_blocking();

        assert!(!worker.poll_blocking(&strategy));
        assert_eq!(worker.stats().tasks_polled, 0);
    }

    #[test]
    fn refresh_partition_updates_bounds() {
        let arena = build_arena(8, 64);
        let mut worker = Worker::new(Arc::clone(&arena));

        let slot = arena.reserve_worker().expect("reserve second worker");
        worker.refresh_partition(arena.leaf_count());

        assert_eq!(worker.partition_start, 0);
        assert_eq!(worker.partition_end, 4);

        arena.release_worker(slot);
    }
}
