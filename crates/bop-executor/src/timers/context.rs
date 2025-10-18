use crate::timers::handle::{TimerInner, TimerState};
use crate::utils::StripedAtomicU64;
use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Configuration for building a `TimerWheelContext`.
#[derive(Clone, Copy, Debug)]
pub struct TimerWheelConfig {
    pub ticks_per_wheel: usize,
    pub garbage_stripes: usize,
}

impl TimerWheelConfig {
    pub const DEFAULT_TICKS_PER_WHEEL: usize = 1024;
    pub const DEFAULT_GARBAGE_STRIPES: usize = 32;

    pub fn new(ticks_per_wheel: usize, garbage_stripes: usize) -> Self {
        assert!(
            ticks_per_wheel.is_power_of_two(),
            "ticks per wheel must be power of two"
        );
        assert!(
            garbage_stripes.is_power_of_two(),
            "garbage stripes must be power of two"
        );
        Self {
            ticks_per_wheel,
            garbage_stripes,
        }
    }
}

impl Default for TimerWheelConfig {
    fn default() -> Self {
        Self::new(Self::DEFAULT_TICKS_PER_WHEEL, Self::DEFAULT_GARBAGE_STRIPES)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GarbagePolicy {
    pub absolute_min: usize,
    pub ratio_numer: usize,
    pub ratio_denom: usize,
}

impl GarbagePolicy {
    pub fn new(absolute_min: usize, ratio_numer: usize, ratio_denom: usize) -> Self {
        assert!(ratio_numer > 0, "ratio numerator must be > 0");
        assert!(ratio_denom > 0, "ratio denominator must be > 0");
        Self {
            absolute_min,
            ratio_numer,
            ratio_denom,
        }
    }

    #[inline]
    pub fn should_compact(&self, stale: usize, live: usize) -> bool {
        if stale == 0 {
            return false;
        }
        if stale >= self.absolute_min {
            return true;
        }
        if live == 0 {
            return true;
        }
        let stale_scaled = (stale as u128) * (self.ratio_denom as u128);
        let live_scaled = (live as u128) * (self.ratio_numer as u128);
        stale_scaled >= live_scaled
    }
}

pub struct TimerEntry {
    pub handle: Arc<TimerInner>,
    pub generation: u64,
    pub deadline_tick: u64,
    pub stripe_hint: u32,
}

impl TimerEntry {
    pub fn new(
        handle: Arc<TimerInner>,
        generation: u64,
        deadline_tick: u64,
        stripe_hint: u32,
    ) -> Self {
        Self {
            handle,
            generation,
            deadline_tick,
            stripe_hint,
        }
    }
}

impl fmt::Debug for TimerEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TimerEntry")
            .field("handle", &Arc::as_ptr(&self.handle))
            .field("generation", &self.generation)
            .field("deadline_tick", &self.deadline_tick)
            .field("stripe_hint", &self.stripe_hint)
            .finish()
    }
}

#[derive(Debug)]
pub struct MergeEntry {
    pub deadline_tick: u64,
    pub generation: u64,
    pub stripe_hint: u32,
    pub handle: Arc<TimerInner>,
}

impl MergeEntry {
    pub fn new(
        deadline_tick: u64,
        generation: u64,
        stripe_hint: u32,
        handle: Arc<TimerInner>,
    ) -> Self {
        Self {
            deadline_tick,
            generation,
            stripe_hint,
            handle,
        }
    }
}

#[derive(Default, Debug)]
pub struct TimerBucket {
    entries: VecDeque<TimerEntry>,
    stale: usize,
}

impl TimerBucket {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            stale: 0,
        }
    }

    #[inline]
    pub fn entries(&self) -> &VecDeque<TimerEntry> {
        &self.entries
    }

    #[inline]
    pub fn entries_mut(&mut self) -> &mut VecDeque<TimerEntry> {
        &mut self.entries
    }

    #[inline]
    pub fn stale(&self) -> usize {
        self.stale
    }

    #[inline]
    pub fn increment_stale(&mut self) {
        self.stale += 1;
    }

    #[inline]
    pub fn reset_stale(&mut self) {
        self.stale = 0;
    }
}

#[derive(Default)]
struct BucketDrainStats {
    stale: usize,
    live: usize,
}

#[derive(Debug)]
pub struct TimerWorkerShared {
    now: AtomicU64,
    needs_poll: AtomicBool,
}

impl TimerWorkerShared {
    pub fn new() -> Self {
        Self {
            now: AtomicU64::new(0),
            needs_poll: AtomicBool::new(false),
        }
    }

    #[inline]
    pub fn update_now(&self, tick: u64) {
        self.now.store(tick, Ordering::Release);
    }

    #[inline]
    pub fn now(&self) -> u64 {
        self.now.load(Ordering::Acquire)
    }

    #[inline]
    pub fn mark_needs_poll(&self) {
        self.needs_poll.store(true, Ordering::Release);
    }

    #[inline]
    pub fn needs_poll(&self) -> bool {
        self.needs_poll.load(Ordering::Acquire)
    }

    #[inline]
    pub fn take_needs_poll(&self) -> bool {
        self.needs_poll.swap(false, Ordering::AcqRel)
    }

    #[inline]
    pub fn clock(&self) -> &AtomicU64 {
        &self.now
    }
}

#[derive(Debug)]
pub struct LocalTimerWheel {
    ticks_per_wheel: usize,
    tick_mask: usize,
    buckets: Vec<TimerBucket>,
    current_tick: u64,
}

impl LocalTimerWheel {
    pub fn new(ticks_per_wheel: usize) -> Self {
        assert!(
            ticks_per_wheel.is_power_of_two(),
            "ticks per wheel must be power of two"
        );
        let buckets = (0..ticks_per_wheel)
            .map(|_| TimerBucket::new())
            .collect::<Vec<_>>();

        Self {
            ticks_per_wheel,
            tick_mask: ticks_per_wheel - 1,
            buckets,
            current_tick: 0,
        }
    }

    #[inline]
    pub fn bucket(&self, index: usize) -> &TimerBucket {
        &self.buckets[index & self.tick_mask]
    }

    #[inline]
    pub fn bucket_mut(&mut self, index: usize) -> &mut TimerBucket {
        let masked = index & self.tick_mask;
        &mut self.buckets[masked]
    }

    #[inline]
    pub fn ticks_per_wheel(&self) -> usize {
        self.ticks_per_wheel
    }

    #[inline]
    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    #[inline]
    pub fn advance_to(&mut self, tick: u64) {
        self.current_tick = tick;
    }

    #[inline]
    pub fn push_entry(&mut self, entry: TimerEntry) -> usize {
        let bucket_index = (entry.deadline_tick as usize) & self.tick_mask;
        self.buckets[bucket_index].entries.push_back(entry);
        bucket_index
    }

    pub fn collect_due(&mut self, now_tick: u64, bucket_budget: usize) -> Vec<(usize, TimerEntry)> {
        let mut ready = Vec::new();
        let mut processed = 0usize;
        while self.current_tick <= now_tick && processed < bucket_budget {
            let bucket_index = (self.current_tick as usize) & self.tick_mask;
            let bucket = &mut self.buckets[bucket_index];
            if !bucket.entries.is_empty() {
                let mut future_entries = VecDeque::with_capacity(bucket.entries.len());
                let mut drained = std::mem::take(&mut bucket.entries);
                while let Some(entry) = drained.pop_front() {
                    if entry.deadline_tick <= now_tick {
                        ready.push((bucket_index, entry));
                    } else {
                        future_entries.push_back(entry);
                    }
                }
                bucket.entries = future_entries;
            }
            processed += 1;
            self.current_tick = self.current_tick.wrapping_add(1);
        }
        ready
    }

    pub fn drain_all(&mut self) -> Vec<TimerEntry> {
        let mut all = Vec::new();
        for bucket in &mut self.buckets {
            while let Some(entry) = bucket.entries.pop_front() {
                all.push(entry);
            }
            bucket.reset_stale();
        }
        all
    }
}

/// Worker-local timer state that pairs the wheel with helper counters.
pub struct TimerWheelContext {
    worker_id: usize,
    shared: Arc<TimerWorkerShared>,
    merge_inbox: Arc<Mutex<Vec<MergeEntry>>>,
    wheel: LocalTimerWheel,
    garbage: Arc<StripedAtomicU64>,
    stale_scratch: Vec<usize>,
    scrub_cursor: usize,
}

impl TimerWheelContext {
    pub fn new(
        worker_id: usize,
        shared: Arc<TimerWorkerShared>,
        merge_inbox: Arc<Mutex<Vec<MergeEntry>>>,
        config: TimerWheelConfig,
    ) -> Self {
        let wheel = LocalTimerWheel::new(config.ticks_per_wheel);
        let garbage = Arc::new(StripedAtomicU64::new(config.garbage_stripes));

        Self {
            worker_id,
            shared,
            merge_inbox,
            wheel,
            garbage,
            stale_scratch: vec![0; config.ticks_per_wheel],
            scrub_cursor: 0,
        }
    }

    #[inline]
    pub fn worker_id(&self) -> usize {
        self.worker_id
    }

    #[inline]
    pub fn set_worker_id(&mut self, worker_id: usize) {
        self.worker_id = worker_id;
    }

    #[inline]
    pub fn shared(&self) -> &Arc<TimerWorkerShared> {
        &self.shared
    }

    #[inline]
    pub fn merge_inbox(&self) -> &Arc<Mutex<Vec<MergeEntry>>> {
        &self.merge_inbox
    }

    #[inline]
    pub fn drain_merge_inbox(&self) -> Vec<MergeEntry> {
        let mut guard = self.merge_inbox.lock().unwrap();
        std::mem::take(&mut *guard)
    }

    #[inline]
    fn reset_stale_scratch(&mut self) {
        for slot in &mut self.stale_scratch {
            *slot = 0;
        }
    }

    #[inline]
    fn apply_stale_counts(&mut self) {
        let len = self.stale_scratch.len();
        for idx in 0..len {
            let count = self.stale_scratch[idx];
            if count > 0 {
                self.wheel.bucket_mut(idx).stale += count;
                self.stale_scratch[idx] = 0;
            }
        }
    }

    #[inline]
    fn retire_garbage(&self, stripe_hint: u32) {
        self.garbage
            .saturating_sub_hint(stripe_hint as usize, 1, Ordering::Relaxed);
    }

    fn evaluate_bucket_compaction(
        &mut self,
        bucket_idx: usize,
        observed_stale: usize,
        observed_live: usize,
        policy: &GarbagePolicy,
    ) {
        let (live_count, stale_metric) = {
            let bucket = self.wheel.bucket(bucket_idx);
            (bucket.entries().len(), bucket.stale())
        };
        let effective_live = live_count + observed_live;
        if policy.should_compact(stale_metric.max(observed_stale), effective_live) {
            self.compact_bucket(bucket_idx);
        } else {
            self.wheel.bucket_mut(bucket_idx).reset_stale();
        }
    }

    fn compact_bucket(&mut self, index: usize) {
        let mut drained = {
            let bucket = self.wheel.bucket_mut(index);
            if bucket.entries.is_empty() {
                bucket.reset_stale();
                return;
            }
            std::mem::take(&mut bucket.entries)
        };
        let mut retained = VecDeque::with_capacity(drained.len());
        while let Some(entry) = drained.pop_front() {
            if entry.handle.generation() == entry.generation
                && matches!(entry.handle.state(Ordering::Acquire), TimerState::Scheduled)
            {
                retained.push_back(entry);
            } else {
                self.retire_garbage(entry.stripe_hint);
            }
        }
        let bucket = self.wheel.bucket_mut(index);
        bucket.entries = retained;
        bucket.reset_stale();
    }

    pub fn schedule_handle(&mut self, handle: &Arc<TimerInner>, deadline_tick: u64) -> u64 {
        let generation = handle.bump_generation();
        handle.record_deadline(deadline_tick);
        handle.set_home_worker(Some(self.worker_id as u32));
        if handle.stripe_hint() == 0 {
            let default_hint = (self.worker_id as u32).wrapping_add(1).max(1);
            handle.set_stripe_hint(default_hint);
        }
        handle.store_state(TimerState::Scheduled, Ordering::Release);
        let entry = TimerEntry::new(
            Arc::clone(handle),
            generation,
            deadline_tick,
            handle.stripe_hint(),
        );
        self.wheel.push_entry(entry);
        self.set_needs_poll();
        generation
    }

    pub fn absorb_merges(&mut self) {
        let merges = self.drain_merge_inbox();
        if merges.is_empty() {
            return;
        }
        for entry in merges {
            let MergeEntry {
                deadline_tick,
                generation,
                stripe_hint,
                handle,
            } = entry;
            if handle.generation() != generation {
                continue;
            }
            match handle.state(Ordering::Acquire) {
                TimerState::Cancelled | TimerState::Fired => continue,
                _ => {}
            }
            handle.set_home_worker(Some(self.worker_id as u32));
            handle.record_deadline(deadline_tick);
            if stripe_hint != 0 {
                handle.set_stripe_hint(stripe_hint);
            } else if handle.stripe_hint() == 0 {
                let default_hint = (self.worker_id as u32).wrapping_add(1).max(1);
                handle.set_stripe_hint(default_hint);
            }
            handle.store_state(TimerState::Scheduled, Ordering::Release);
            let final_hint = handle.stripe_hint();
            let scheduled = TimerEntry::new(handle, generation, deadline_tick, final_hint);
            self.wheel.push_entry(scheduled);
        }
        self.set_needs_poll();
    }

    pub fn poll_ready(
        &mut self,
        now_tick: u64,
        bucket_budget: usize,
        policy: &GarbagePolicy,
    ) -> Vec<TimerEntry> {
        let due_entries = self.wheel.collect_due(now_tick, bucket_budget);
        let processed_all = self.wheel.current_tick() > now_tick;
        if due_entries.is_empty() {
            self.shared.take_needs_poll();
            return Vec::new();
        }

        self.reset_stale_scratch();
        let mut ready = Vec::new();
        let mut bucket_stats: Vec<(usize, BucketDrainStats)> =
            Vec::with_capacity(due_entries.len().min(8));

        for (bucket_idx, entry) in due_entries {
            let stats = if let Some((_, stats)) =
                bucket_stats.iter_mut().find(|(idx, _)| *idx == bucket_idx)
            {
                stats
            } else {
                bucket_stats.push((bucket_idx, BucketDrainStats::default()));
                &mut bucket_stats.last_mut().unwrap().1
            };
            let current_generation = entry.handle.generation();
            if current_generation != entry.generation {
                self.stale_scratch[bucket_idx] += 1;
                stats.stale += 1;
                self.retire_garbage(entry.stripe_hint);
                continue;
            }

            match entry.handle.state(Ordering::Acquire) {
                TimerState::Scheduled => {
                    stats.live += 1;
                    entry
                        .handle
                        .store_state(TimerState::Fired, Ordering::Release);
                    ready.push(entry);
                }
                TimerState::Cancelled
                | TimerState::Fired
                | TimerState::Migrating
                | TimerState::Idle
                | TimerState::Firing => {
                    self.stale_scratch[bucket_idx] += 1;
                    stats.stale += 1;
                    self.retire_garbage(entry.stripe_hint);
                }
            }
        }

        self.apply_stale_counts();
        for (bucket_idx, stats) in bucket_stats {
            self.evaluate_bucket_compaction(bucket_idx, stats.stale, stats.live, policy);
        }

        self.shared.take_needs_poll();

        ready
    }

    #[inline]
    pub fn now(&self) -> u64 {
        self.shared.now()
    }

    #[inline]
    pub fn set_needs_poll(&self) {
        self.shared.mark_needs_poll();
    }

    #[inline]
    pub fn clear_needs_poll(&self) -> bool {
        self.shared.take_needs_poll()
    }

    #[inline]
    pub fn needs_poll(&self) -> bool {
        self.shared.needs_poll()
    }

    #[inline]
    pub fn wheel(&self) -> &LocalTimerWheel {
        &self.wheel
    }

    #[inline]
    pub fn wheel_mut(&mut self) -> &mut LocalTimerWheel {
        &mut self.wheel
    }

    #[inline]
    pub fn garbage(&self) -> &StripedAtomicU64 {
        self.garbage.as_ref()
    }

    #[inline]
    pub fn garbage_shared(&self) -> Arc<StripedAtomicU64> {
        Arc::clone(&self.garbage)
    }

    #[inline]
    pub fn scrub_cursor(&self) -> usize {
        self.scrub_cursor
    }

    #[inline]
    pub fn advance_scrub_cursor(&mut self) {
        self.scrub_cursor = (self.scrub_cursor + 1) & (self.wheel.ticks_per_wheel - 1);
    }

    pub fn scrub_next_bucket(&mut self, policy: &GarbagePolicy) {
        let index = self.scrub_cursor & (self.wheel.ticks_per_wheel - 1);
        self.scrub_cursor = (self.scrub_cursor + 1) & (self.wheel.ticks_per_wheel - 1);
        let (stale, live) = {
            let bucket = self.wheel.bucket(index);
            (bucket.stale(), bucket.entries().len())
        };
        if stale == 0 && live > 0 && self.garbage.total() > 0 {
            self.compact_bucket(index);
            return;
        }
        if policy.should_compact(stale, live) {
            self.compact_bucket(index);
        } else {
            self.wheel.bucket_mut(index).reset_stale();
        }
    }

    pub fn drain_all_entries(&mut self) -> Vec<TimerEntry> {
        self.wheel.drain_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timers::handle::{TimerHandle, TimerState};
    use crate::timers::service::{TimerConfig, TimerService};
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use std::time::Instant;

    #[test]
    fn timer_wheel_context_schedules_poll_and_merges() {
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(8, 4),
        );
        assert!(!ctx.needs_poll());
        ctx.set_needs_poll();
        assert!(ctx.needs_poll());
        assert!(ctx.clear_needs_poll());
        assert!(!ctx.needs_poll());
        shared.update_now(42);
        assert_eq!(ctx.now(), 42);

        let handle = Arc::new(TimerInner::new());
        ctx.schedule_handle(&handle, 42);
        let policy = GarbagePolicy::new(1, 1, 1);
        assert!(ctx.needs_poll());
        let ready = ctx.poll_ready(42, 8, &policy);
        assert_eq!(ready.len(), 1);
        assert_eq!(handle.state(Ordering::Acquire), TimerState::Fired);
        assert!(!ctx.needs_poll());

        let merge_handle = Arc::new(TimerInner::new());
        let merge_generation = merge_handle.bump_generation();
        merge_handle.store_state(TimerState::Migrating, Ordering::Release);
        inbox.lock().unwrap().push(MergeEntry::new(
            45,
            merge_generation,
            0,
            Arc::clone(&merge_handle),
        ));
        ctx.absorb_merges();
        let merged_ready = ctx.poll_ready(45, 8, &policy);
        assert_eq!(merged_ready.len(), 1);
        assert_eq!(merge_handle.state(Ordering::Acquire), TimerState::Fired);
    }

    #[test]
    fn poll_ready_respects_bucket_budget() {
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(8, 4),
        );
        let policy = GarbagePolicy::new(1, 1, 1);

        let first = TimerHandle::new();
        let second = TimerHandle::new();

        ctx.schedule_handle(first.inner(), 5);
        ctx.schedule_handle(second.inner(), 6);
        ctx.wheel_mut().advance_to(5);
        shared.update_now(6);

        let ready_first = ctx.poll_ready(6, 1, &policy);
        assert_eq!(ready_first.len(), 1);
        assert!(Arc::ptr_eq(&ready_first[0].handle, first.inner()));

        let ready_second = ctx.poll_ready(6, 1, &policy);
        assert_eq!(ready_second.len(), 1);
        assert!(Arc::ptr_eq(&ready_second[0].handle, second.inner()));
    }

    #[test]
    fn stale_generation_retire_garbage_and_skip_entry() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(16, 4),
        );
        let garbage = ctx.garbage_shared();
        let _token = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );
        let policy = GarbagePolicy::new(1, 1, 1);

        let handle = TimerHandle::new();
        let deadline = 4u64;
        let generation = ctx.schedule_handle(handle.inner(), deadline);
        assert_eq!(handle.generation(), generation);

        ctx.garbage()
            .fetch_add_hint(handle.inner().stripe_hint() as usize, 1, Ordering::Relaxed);
        handle.inner().bump_generation();

        ctx.wheel_mut().advance_to(deadline);
        shared.update_now(deadline);
        let ready = ctx.poll_ready(deadline, 8, &policy);
        assert!(ready.is_empty());
        assert_eq!(ctx.garbage().total(), 0);

        service.shutdown();
    }

    #[test]
    fn scrub_cursor_wraps_after_full_cycle() {
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(4, 4),
        );
        let policy = GarbagePolicy::new(1, 1, 1);

        assert_eq!(ctx.scrub_cursor(), 0);
        ctx.scrub_next_bucket(&policy);
        assert_eq!(ctx.scrub_cursor(), 1);

        while ctx.scrub_cursor() != 3 {
            ctx.advance_scrub_cursor();
        }
        ctx.scrub_next_bucket(&policy);
        assert_eq!(ctx.scrub_cursor(), 0);
    }

    #[test]
    fn dropping_handle_before_poll_keeps_entry_alive() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(32, 4),
        );
        let garbage = ctx.garbage_shared();
        let _token = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );
        let policy = GarbagePolicy::new(1, 1, 1);

        let handle = TimerHandle::new();
        let inner = Arc::clone(handle.inner());
        ctx.schedule_handle(handle.inner(), 9);
        assert!(Arc::strong_count(&inner) >= 2);
        drop(handle);

        ctx.wheel_mut().advance_to(9);
        shared.update_now(9);
        let ready = ctx.poll_ready(9, 16, &policy);
        assert_eq!(ready.len(), 1);
        assert!(Arc::ptr_eq(&ready[0].handle, &inner));

        service.shutdown();
    }

    #[test]
    fn tick_skew_does_not_skip_due_entries() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(32, 4),
        );
        let garbage = ctx.garbage_shared();
        let _token = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );
        let policy = GarbagePolicy::new(1, 1, 1);

        let handle = TimerHandle::new();
        ctx.schedule_handle(handle.inner(), 12);
        ctx.wheel_mut().advance_to(12);

        shared.update_now(120);
        let ready = ctx.poll_ready(120, 64, &policy);
        assert_eq!(ready.len(), 1);
        assert!(Arc::ptr_eq(&ready[0].handle, handle.inner()));

        service.shutdown();
    }
    #[test]
    fn local_timer_wheel_masks_indices() {
        let mut wheel = LocalTimerWheel::new(8);
        wheel.advance_to(5);
        assert_eq!(wheel.current_tick(), 5);
        assert_eq!(wheel.ticks_per_wheel(), 8);
        let bucket = wheel.bucket(9);
        assert_eq!(bucket.entries().len(), 0);
        let bucket_mut = wheel.bucket_mut(9);
        assert_eq!(bucket_mut.entries().len(), 0);
    }

    #[test]
    fn stale_cleanup_decrements_garbage_counter() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(8, 4),
        );
        let garbage = ctx.garbage_shared();
        let _token = service.register_worker(Arc::clone(&shared), Arc::clone(&inbox), garbage);

        let handle = TimerHandle::new();
        ctx.schedule_handle(handle.inner(), 1);
        shared.update_now(1);

        assert!(handle.cancel(&service));
        assert!(ctx.garbage().total() > 0);

        let policy = GarbagePolicy::new(1, 1, 1);
        let ready = ctx.poll_ready(1, 8, &policy);
        assert!(ready.is_empty());
        assert_eq!(ctx.garbage().total(), 0);

        service.shutdown();
    }

    #[test]
    fn due_bucket_compaction_clears_future_stale_entries() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(16, 4),
        );
        let garbage = ctx.garbage_shared();
        let _token = service.register_worker(Arc::clone(&shared), Arc::clone(&inbox), garbage);
        let policy = GarbagePolicy::new(1, 1, 1);

        let live = TimerHandle::new();
        let stale_due = TimerHandle::new();
        let stale_future = TimerHandle::new();

        let base_deadline = 4u64;
        let future_deadline = base_deadline + ctx.wheel().ticks_per_wheel() as u64;

        ctx.schedule_handle(live.inner(), base_deadline);
        ctx.schedule_handle(stale_due.inner(), base_deadline);
        ctx.schedule_handle(stale_future.inner(), future_deadline);

        shared.update_now(future_deadline);

        assert!(stale_due.cancel(&service));
        assert!(stale_future.cancel(&service));
        assert!(ctx.garbage().total() >= 2);

        let ready = ctx.poll_ready(future_deadline, 64, &policy);
        assert_eq!(ready.len(), 1);
        assert!(Arc::ptr_eq(&ready[0].handle, live.inner()));
        assert_eq!(ctx.garbage().total(), 0);

        let bucket_idx = (base_deadline as usize) & (ctx.wheel().ticks_per_wheel() - 1);
        assert_eq!(ctx.wheel().bucket(bucket_idx).entries().len(), 0);

        service.shutdown();
    }

    #[test]
    fn background_scrub_removes_far_future_stale_entries() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(16, 4),
        );
        let garbage = ctx.garbage_shared();
        let _token = service.register_worker(Arc::clone(&shared), Arc::clone(&inbox), garbage);
        let policy = GarbagePolicy::new(1, 1, 1);

        let future_tick = 24u64;
        let bucket_idx = (future_tick as usize) & (ctx.wheel().ticks_per_wheel() - 1);

        let handle_a = TimerHandle::new();
        let handle_b = TimerHandle::new();

        ctx.schedule_handle(handle_a.inner(), future_tick);
        ctx.schedule_handle(handle_b.inner(), future_tick);

        assert!(handle_a.cancel(&service));
        assert!(handle_b.cancel(&service));
        assert!(ctx.garbage().total() >= 2);

        while ctx.scrub_cursor() != bucket_idx {
            ctx.advance_scrub_cursor();
        }

        ctx.scrub_next_bucket(&policy);

        assert_eq!(ctx.garbage().total(), 0);
        assert_eq!(ctx.wheel().bucket(bucket_idx).entries().len(), 0);

        service.shutdown();
    }

    #[test]
    fn timer_handle_lifecycle_schedule_cancel_reschedule() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(32, 4),
        );
        let garbage = ctx.garbage_shared();
        let token = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );
        ctx.set_worker_id(token.id());

        shared.update_now(10);
        let handle = TimerHandle::new();
        let policy = GarbagePolicy::new(1, 1, 1);

        let gen1 = ctx.schedule_handle(handle.inner(), 12);
        assert_eq!(handle.generation(), gen1);
        assert_eq!(handle.state(), TimerState::Scheduled);

        assert!(handle.cancel(&service));
        assert_eq!(handle.state(), TimerState::Cancelled);

        let gen2 = handle.reschedule(&service, 16);
        assert!(gen2 > gen1);

        for _ in 0..100 {
            {
                let ready = inbox.lock().unwrap();
                if !ready.is_empty() {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(1));
        }

        ctx.absorb_merges();
        shared.update_now(16);
        let ready = ctx.poll_ready(16, 32, &policy);
        assert_eq!(ready.len(), 1);
        assert!(Arc::ptr_eq(&ready[0].handle, handle.inner()));
        assert_eq!(handle.state(), TimerState::Fired);

        service.shutdown();
    }

    #[test]
    fn absorb_merges_skips_entries_with_stale_generation() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(16, 4),
        );
        let garbage = ctx.garbage_shared();
        let token = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );
        ctx.set_worker_id(token.id());

        let handle = TimerHandle::new();
        handle
            .inner()
            .store_state(TimerState::Migrating, Ordering::Release);
        let stale_generation = handle.inner().bump_generation();
        let deadline = 24u64;

        inbox.lock().unwrap().push(MergeEntry::new(
            deadline,
            stale_generation,
            0,
            Arc::clone(handle.inner()),
        ));

        handle.inner().bump_generation();
        ctx.absorb_merges();
        let bucket_idx = (deadline as usize) & (ctx.wheel().ticks_per_wheel() - 1);
        assert_eq!(ctx.wheel().bucket(bucket_idx).entries().len(), 0);

        service.shutdown();
    }

    #[test]
    fn scrub_policy_obeys_ratio_before_compacting() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(32, 8),
        );
        let garbage = ctx.garbage_shared();
        let token = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );
        ctx.set_worker_id(token.id());

        let policy = GarbagePolicy::new(8, 3, 2);
        let future_tick = 48u64;
        let handle_live = TimerHandle::new();
        let handle_cancelled = TimerHandle::new();

        ctx.schedule_handle(handle_live.inner(), future_tick);
        ctx.schedule_handle(handle_cancelled.inner(), future_tick);

        assert!(handle_cancelled.cancel(&service));
        let bucket_idx = (future_tick as usize) & (ctx.wheel().ticks_per_wheel() - 1);
        assert!(ctx.garbage().total() >= 1);

        while ctx.scrub_cursor() != bucket_idx {
            ctx.advance_scrub_cursor();
        }
        ctx.scrub_next_bucket(&policy);

        let entries = ctx.wheel().bucket(bucket_idx).entries().len();
        assert_eq!(entries, 1);
        assert_eq!(ctx.garbage().total(), 0);

        service.shutdown();
    }

    #[test]
    fn multi_worker_merge_burst_distributes_evenly() {
        let service = TimerService::start(TimerConfig::default());
        let mut workers = Vec::new();
        let policy = GarbagePolicy::new(1, 1, 1);

        for id in 0..3 {
            let shared = Arc::new(TimerWorkerShared::new());
            let inbox = Arc::new(Mutex::new(Vec::new()));
            let mut ctx = TimerWheelContext::new(
                id,
                Arc::clone(&shared),
                Arc::clone(&inbox),
                TimerWheelConfig::new(64, 8),
            );
            let garbage = ctx.garbage_shared();
            let token = service.register_worker(
                Arc::clone(&shared),
                Arc::clone(&inbox),
                Arc::clone(&garbage),
            );
            ctx.set_worker_id(token.id());
            workers.push((ctx, shared, inbox, token));
        }

        let handles: Vec<_> = (0..45).map(|_| TimerHandle::new()).collect();
        for (idx, handle) in handles.iter().enumerate() {
            let deadline = 10 + (idx as u64 % 6);
            service.schedule_handle(handle, deadline);
        }

        let start = Instant::now();
        loop {
            let total_inbox: usize = workers
                .iter()
                .map(|(_, _, inbox, _)| inbox.lock().unwrap().len())
                .sum();
            if total_inbox >= handles.len() {
                break;
            }
            if start.elapsed() > Duration::from_millis(500) {
                panic!("merge entries did not arrive in time");
            }
            thread::sleep(Duration::from_millis(5));
        }

        let mut fired = 0usize;
        for (ctx, shared, inbox, _token) in &mut workers {
            assert!(!inbox.lock().unwrap().is_empty());
            ctx.absorb_merges();
            shared.update_now(10_000);
            let ready = ctx.poll_ready(10_000, 256, &policy);
            fired += ready.len();
        }

        assert_eq!(fired, handles.len());

        service.shutdown();
    }

    #[test]
    fn timers_migrate_between_workers() {
        let service = TimerService::start(TimerConfig::default());

        let shared_a = Arc::new(TimerWorkerShared::new());
        let inbox_a = Arc::new(Mutex::new(Vec::new()));
        let mut ctx_a = TimerWheelContext::new(
            0,
            Arc::clone(&shared_a),
            Arc::clone(&inbox_a),
            TimerWheelConfig::new(32, 4),
        );
        let garbage_a = ctx_a.garbage_shared();
        let token_a = service.register_worker(
            Arc::clone(&shared_a),
            Arc::clone(&inbox_a),
            Arc::clone(&garbage_a),
        );
        ctx_a.set_worker_id(token_a.id());

        let shared_b = Arc::new(TimerWorkerShared::new());
        let inbox_b = Arc::new(Mutex::new(Vec::new()));
        let mut ctx_b = TimerWheelContext::new(
            1,
            Arc::clone(&shared_b),
            Arc::clone(&inbox_b),
            TimerWheelConfig::new(32, 4),
        );
        let garbage_b = ctx_b.garbage_shared();
        let token_b = service.register_worker(
            Arc::clone(&shared_b),
            Arc::clone(&inbox_b),
            Arc::clone(&garbage_b),
        );
        ctx_b.set_worker_id(token_b.id());

        shared_a.update_now(10);
        let handle = TimerHandle::new();
        let policy = GarbagePolicy::new(1, 1, 1);

        let generation = ctx_a.schedule_handle(handle.inner(), 15);
        assert_eq!(generation, handle.generation());

        let drained = ctx_a.drain_all_entries();
        for entry in drained {
            let handle = entry.handle;
            if handle.generation() != entry.generation {
                continue;
            }
            match handle.state(Ordering::Acquire) {
                TimerState::Scheduled | TimerState::Migrating => {
                    handle.set_home_worker(None);
                    handle.store_state(TimerState::Migrating, Ordering::Release);
                    inbox_b.lock().unwrap().push(MergeEntry::new(
                        entry.deadline_tick,
                        entry.generation,
                        entry.stripe_hint,
                        Arc::clone(&handle),
                    ));
                    shared_b.mark_needs_poll();
                }
                _ => {}
            }
        }

        for _ in 0..100 {
            {
                let ready = inbox_b.lock().unwrap();
                if !ready.is_empty() {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(1));
        }

        ctx_b.absorb_merges();
        shared_b.update_now(15);
        let ready = ctx_b.poll_ready(15, 32, &policy);
        assert_eq!(ready.len(), 1);
        assert!(Arc::ptr_eq(&ready[0].handle, handle.inner()));
        assert_eq!(handle.state(), TimerState::Fired);

        service.shutdown();
    }
}
