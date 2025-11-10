/// High-performance hierarchical hashed timing wheel for efficient timer scheduling
/// Based on "Hashed and Hierarchical Timing Wheels" by Varghese and Lauck
///
/// Two-level hierarchy:
/// - L0 (Fast): 1.05ms/tick × 1024 ticks = 1.07s coverage
/// - L1 (Medium): 1.07s/tick × 1024 ticks = 18.33min coverage
///
/// O(1) timer scheduling and cancellation
/// Timers in same tick are not ordered relative to each other
/// NOT thread-safe - caller must synchronize
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// Internal single-level timing wheel
/// Used as a building block for the hierarchical TimerWheel
struct SingleWheel<T> {
    /// Time resolution per tick (must be power of 2)
    tick_resolution_ns: u64,

    /// Current tick position
    current_tick: u64,

    /// Number of active timers in this wheel
    timer_count: u64,

    /// Cached next deadline for efficient querying
    cached_next_deadline: u64,

    /// Number of ticks/spokes per wheel (must be power of 2)
    ticks_per_wheel: usize,

    /// Mask for tick calculation (ticks_per_wheel - 1)
    tick_mask: usize,

    /// Bit shift for resolution calculations
    resolution_bits_to_shift: u32,

    /// Current allocation size per tick (must be power of 2)
    tick_allocation: usize,

    /// Bit shift for allocation calculations
    allocation_bits_to_shift: u32,

    /// Current index within a tick during polling
    poll_index: usize,

    /// Flat array: [tick0_slot0, tick0_slot1, ..., tick1_slot0, ...]
    wheel: Vec<Option<(u64, T)>>,

    /// Per-tick next available slot hint for O(1) scheduling
    next_free_hint: Vec<usize>,
}

impl<T> SingleWheel<T> {
    const INITIAL_TICK_ALLOCATION: usize = 16;
    const NULL_DEADLINE: u64 = u64::MAX;

    fn new(
        tick_resolution_ns: u64,
        ticks_per_wheel: usize,
        initial_tick_allocation: usize,
    ) -> Self {
        assert!(
            ticks_per_wheel.is_power_of_two(),
            "ticks_per_wheel must be power of 2"
        );
        assert!(
            initial_tick_allocation.is_power_of_two(),
            "tick_allocation must be power of 2"
        );
        assert!(
            tick_resolution_ns.is_power_of_two(),
            "tick_resolution must be power of 2 ns"
        );

        let tick_mask = ticks_per_wheel - 1;
        let resolution_bits_to_shift = tick_resolution_ns.trailing_zeros();
        let allocation_bits_to_shift = initial_tick_allocation.trailing_zeros();

        let capacity = ticks_per_wheel * initial_tick_allocation;
        let mut wheel = Vec::with_capacity(capacity);
        wheel.resize_with(capacity, || None);

        let mut next_free_hint = Vec::with_capacity(ticks_per_wheel);
        next_free_hint.resize(ticks_per_wheel, 0);

        Self {
            tick_resolution_ns,
            current_tick: 0,
            timer_count: 0,
            cached_next_deadline: Self::NULL_DEADLINE,
            ticks_per_wheel,
            tick_mask,
            resolution_bits_to_shift,
            tick_allocation: initial_tick_allocation,
            allocation_bits_to_shift,
            poll_index: 0,
            wheel,
            next_free_hint,
        }
    }

    /// Schedule a timer, returns (spoke_index, slot_index)
    fn schedule_internal(
        &mut self,
        deadline_ns: u64,
        start_time_ns: u64,
        data: T,
    ) -> Option<(usize, usize)> {
        let deadline_tick =
            (deadline_ns.saturating_sub(start_time_ns)) >> self.resolution_bits_to_shift;
        let spoke_index = (deadline_tick & self.tick_mask as u64) as usize;
        let tick_start_index = spoke_index << self.allocation_bits_to_shift;

        // Use hint for O(1) slot finding
        let hint = self.next_free_hint[spoke_index];
        let hint_index = tick_start_index + hint;

        // Fast path: hint points to free slot
        if hint < self.tick_allocation && self.wheel[hint_index].is_none() {
            self.wheel[hint_index] = Some((deadline_ns, data));
            self.timer_count += 1;
            if deadline_ns < self.cached_next_deadline {
                self.cached_next_deadline = deadline_ns;
            }
            self.next_free_hint[spoke_index] = hint + 1;
            return Some((spoke_index, hint));
        }

        // Slow path: linear search
        for i in 0..self.tick_allocation {
            let slot_idx = (hint + i) % self.tick_allocation;
            let index = tick_start_index + slot_idx;

            if self.wheel[index].is_none() {
                self.wheel[index] = Some((deadline_ns, data));
                self.timer_count += 1;
                if deadline_ns < self.cached_next_deadline {
                    self.cached_next_deadline = deadline_ns;
                }
                self.next_free_hint[spoke_index] = (slot_idx + 1) % self.tick_allocation;
                return Some((spoke_index, slot_idx));
            }
        }

        // Need to increase capacity
        self.increase_capacity(deadline_ns, spoke_index, data)
    }

    fn cancel_internal(&mut self, spoke_index: usize, slot_index: usize) -> Option<T> {
        let wheel_index = (spoke_index << self.allocation_bits_to_shift) + slot_index;

        if spoke_index < self.ticks_per_wheel && slot_index < self.tick_allocation {
            if let Some((deadline_ns, data)) = self.wheel[wheel_index].take() {
                self.timer_count -= 1;
                if deadline_ns == self.cached_next_deadline {
                    self.cached_next_deadline = Self::NULL_DEADLINE;
                }
                return Some(data);
            }
        }
        None
    }

    fn increase_capacity(
        &mut self,
        deadline_ns: u64,
        spoke_index: usize,
        data: T,
    ) -> Option<(usize, usize)> {
        let new_tick_allocation = self.tick_allocation << 1;
        let new_allocation_bits = new_tick_allocation.trailing_zeros();

        let new_capacity = self.ticks_per_wheel * new_tick_allocation;
        if new_capacity > (1 << 30) {
            return None;
        }

        let mut new_wheel = Vec::with_capacity(new_capacity);
        new_wheel.resize_with(new_capacity, || None);

        // Copy old data
        for j in 0..self.ticks_per_wheel {
            let old_start = j << self.allocation_bits_to_shift;
            let new_start = j << new_allocation_bits;
            for k in 0..self.tick_allocation {
                new_wheel[new_start + k] = self.wheel[old_start + k].take();
            }
        }

        // Add new timer
        let new_index = (spoke_index << new_allocation_bits) + self.tick_allocation;
        new_wheel[new_index] = Some((deadline_ns, data));
        self.timer_count += 1;

        let old_allocation = self.tick_allocation;
        self.tick_allocation = new_tick_allocation;
        self.allocation_bits_to_shift = new_allocation_bits;
        self.wheel = new_wheel;

        // Reset hints
        for hint in &mut self.next_free_hint {
            *hint = old_allocation;
        }

        Some((spoke_index, old_allocation))
    }
}

/// High-performance hierarchical timing wheel
pub struct TimerWheel<T> {
    /// L0 (Fast) wheel: 1.05ms ticks, covers 0-1.07s
    l0: SingleWheel<T>,

    /// L1 (Medium) wheel: 1.07s ticks, covers 1.07s-18.33min
    l1: SingleWheel<T>,

    /// Null deadline sentinel
    null_deadline: u64,

    /// Start time in nanoseconds
    start_time_ns: u64,

    /// Current time in nanoseconds (atomic for cross-thread updates)
    now_ns: AtomicU64,

    /// Worker ID that owns this timer wheel
    worker_id: u32,

    /// L0 coverage in nanoseconds (precalculated)
    l0_coverage_ns: u64,

    /// Last L1 tick that was fully cascaded (to avoid re-cascading)
    /// Uses Option to handle the case where no ticks have been cascaded yet
    last_cascaded_l1_tick: Option<u64>,
}

impl<T> TimerWheel<T> {
    const INITIAL_TICK_ALLOCATION: usize = 16;
    const NULL_DEADLINE: u64 = u64::MAX;
    const TICKS_PER_WHEEL: usize = 1024;

    /// Create new hierarchical timer wheel
    ///
    /// # Arguments
    /// * `tick_resolution` - L0 tick duration (must be power of 2 nanoseconds, typically 1.05ms)
    /// * `ticks_per_wheel` - Number of spokes per wheel (must be power of 2, typically 1024)
    /// * `worker_id` - Worker ID that owns this timer wheel
    ///
    /// # Timer Deadlines
    /// All timer deadlines are specified as nanoseconds elapsed since wheel creation.
    pub fn new(tick_resolution: Duration, ticks_per_wheel: usize, worker_id: u32) -> Self {
        Self::with_allocation(
            tick_resolution,
            ticks_per_wheel,
            Self::INITIAL_TICK_ALLOCATION,
            worker_id,
        )
    }

    /// Create hierarchical timer wheel with custom initial allocation per tick
    pub fn with_allocation(
        tick_resolution: Duration,
        ticks_per_wheel: usize,
        initial_tick_allocation: usize,
        worker_id: u32,
    ) -> Self {
        let l0_tick_ns = tick_resolution.as_nanos() as u64;
        let l0_coverage_ns = l0_tick_ns * ticks_per_wheel as u64;
        let l1_tick_ns = l0_coverage_ns; // Each L1 tick = one full L0 rotation

        let l0 = SingleWheel::new(l0_tick_ns, ticks_per_wheel, initial_tick_allocation);
        let l1 = SingleWheel::new(l1_tick_ns, ticks_per_wheel, initial_tick_allocation);

        Self {
            l0,
            l1,
            null_deadline: Self::NULL_DEADLINE,
            start_time_ns: 0,
            now_ns: AtomicU64::new(0),
            worker_id,
            l0_coverage_ns,
            last_cascaded_l1_tick: None,
        }
    }

    /// Schedule a timer for an absolute deadline
    /// Returns timer_id for future cancellation, or None if failed
    pub fn schedule_timer(&mut self, deadline_ns: u64, data: T) -> Option<u64> {
        // Determine which wheel based on deadline
        if deadline_ns < self.l0_coverage_ns {
            // Schedule in L0
            let (spoke, slot) = self
                .l0
                .schedule_internal(deadline_ns, self.start_time_ns, data)?;
            Some(Self::encode_timer_id(0, spoke, slot))
        } else {
            // Schedule in L1
            let (spoke, slot) = self
                .l1
                .schedule_internal(deadline_ns, self.start_time_ns, data)?;
            Some(Self::encode_timer_id(1, spoke, slot))
        }
    }

    /// Cancel a previously scheduled timer
    pub fn cancel_timer(&mut self, timer_id: u64) -> Option<T> {
        let (level, spoke, slot) = Self::decode_timer_id(timer_id);
        match level {
            0 => self.l0.cancel_internal(spoke, slot),
            1 => self.l1.cancel_internal(spoke, slot),
            _ => None,
        }
    }

    /// Poll for expired timers
    /// Writes expired `(timer_id, deadline_ns, data)` tuples into output buffer
    pub fn poll(
        &mut self,
        now_ns: u64,
        expiry_limit: usize,
        output: &mut Vec<(u64, u64, T)>,
    ) -> usize {
        self.now_ns.store(now_ns, Ordering::Release);
        output.clear();

        // Cascade L1 -> L0 if L1 ticks have expired
        self.cascade_l1_to_l0(now_ns);

        // Poll L0 for expired timers
        self.poll_l0(now_ns, expiry_limit, output)
    }

    fn cascade_l1_to_l0(&mut self, now_ns: u64) {
        let l1_target_tick =
            (now_ns.saturating_sub(self.start_time_ns)) >> self.l1.resolution_bits_to_shift;

        // If no L1 timers, nothing to cascade
        if self.l1.timer_count == 0 {
            // Still update last_cascaded to avoid re-scanning empty ticks
            self.last_cascaded_l1_tick = Some(l1_target_tick);
            return;
        }

        // Cascade ALL L1 ticks from 0 to target_tick
        // We can't skip ticks based on last_cascaded because new timers can be
        // scheduled into already-cascaded ticks at any time
        for current_tick in 0..=l1_target_tick {
            let spoke_index = (current_tick & self.l1.tick_mask as u64) as usize;
            let tick_start = spoke_index << self.l1.allocation_bits_to_shift;

            // Collect all timers from this L1 spoke
            for slot_idx in 0..self.l1.tick_allocation {
                let wheel_index = tick_start + slot_idx;
                if let Some((deadline_ns, data)) = self.l1.wheel[wheel_index].take() {
                    self.l1.timer_count -= 1;

                    // Reschedule into L0
                    let _ = self
                        .l0
                        .schedule_internal(deadline_ns, self.start_time_ns, data);
                }
            }

            // Reset L1 spoke hint
            self.l1.next_free_hint[spoke_index] = 0;
        }

        // Update last cascaded tick
        self.last_cascaded_l1_tick = Some(l1_target_tick);

        // Invalidate both L0 and L1 caches after cascading
        // L0 cache must be invalidated because cascaded timers may have deadlines
        // in past ticks (already processed), and recompute needs to scan all ticks to find them
        if self.l1.cached_next_deadline != SingleWheel::<T>::NULL_DEADLINE {
            self.l1.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
        }
        if self.l0.cached_next_deadline != SingleWheel::<T>::NULL_DEADLINE {
            self.l0.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
        }
    }

    fn poll_l0(
        &mut self,
        now_ns: u64,
        expiry_limit: usize,
        output: &mut Vec<(u64, u64, T)>,
    ) -> usize {
        let target_tick =
            (now_ns.saturating_sub(self.start_time_ns)) >> self.l0.resolution_bits_to_shift;

        // If no timers, just return without advancing current_tick
        // current_tick will naturally catch up when timers are added via cascade
        if self.l0.timer_count == 0 {
            return 0;
        }

        // CRITICAL: After cascading, timers may exist in ticks BEFORE current_tick
        // (if current_tick advanced while L0 was empty). We must scan ALL expired timers
        // in the entire wheel, not just from current_tick forward.
        // Use deadline-based expiry check, not tick-based.

        // Scan all ticks that could contain expired timers
        for spoke in 0..self.l0.ticks_per_wheel {
            if output.len() >= expiry_limit {
                break;
            }

            for slot_idx in 0..self.l0.tick_allocation {
                if output.len() >= expiry_limit {
                    break;
                }

                let wheel_index = (spoke << self.l0.allocation_bits_to_shift) + slot_idx;

                if let Some((deadline, _)) = &self.l0.wheel[wheel_index] {
                    if now_ns >= *deadline {
                        let (deadline, data) = self.l0.wheel[wheel_index].take().unwrap();
                        self.l0.timer_count -= 1;

                        let timer_id = Self::encode_timer_id(0, spoke, slot_idx);
                        output.push((timer_id, deadline, data));

                        if deadline == self.l0.cached_next_deadline {
                            self.l0.cached_next_deadline = self.null_deadline;
                        }
                    }
                }
            }
        }

        // Advance current_tick to target_tick since we've processed all expired timers
        self.l0.current_tick = target_tick;
        self.l0.poll_index = 0;

        output.len()
    }

    pub fn advance_to(&mut self, now_ns: u64) {
        let new_tick =
            (now_ns.saturating_sub(self.start_time_ns)) >> self.l0.resolution_bits_to_shift;
        self.l0.current_tick = self.l0.current_tick.max(new_tick);
    }

    pub fn current_tick_time_ns(&self) -> u64 {
        ((self.l0.current_tick + 1) << self.l0.resolution_bits_to_shift) + self.start_time_ns
    }

    pub fn timer_count(&self) -> u64 {
        self.l0.timer_count + self.l1.timer_count
    }

    pub fn next_deadline(&mut self) -> Option<u64> {
        let total_count = self.l0.timer_count + self.l1.timer_count;
        if total_count == 0 {
            return None;
        }

        let l0_next = if self.l0.timer_count > 0 {
            if self.l0.cached_next_deadline == SingleWheel::<T>::NULL_DEADLINE {
                self.recompute_l0_deadline();
            }
            if self.l0.cached_next_deadline != SingleWheel::<T>::NULL_DEADLINE {
                Some(self.l0.cached_next_deadline)
            } else {
                None
            }
        } else {
            None
        };

        let l1_next = if self.l1.timer_count > 0 {
            if self.l1.cached_next_deadline == SingleWheel::<T>::NULL_DEADLINE {
                self.recompute_l1_deadline();
            }
            if self.l1.cached_next_deadline != SingleWheel::<T>::NULL_DEADLINE {
                Some(self.l1.cached_next_deadline)
            } else {
                None
            }
        } else {
            None
        };

        match (l0_next, l1_next) {
            (Some(d0), Some(d1)) => Some(d0.min(d1)),
            (Some(d0), None) => Some(d0),
            (None, Some(d1)) => Some(d1),
            (None, None) => None,
        }
    }

    fn recompute_l0_deadline(&mut self) {
        if self.l0.timer_count == 0 {
            self.l0.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
            return;
        }

        let mut earliest = SingleWheel::<T>::NULL_DEADLINE;

        // CRITICAL: Must scan ENTIRE wheel since poll_l0 does full-wheel scans
        // and doesn't maintain current_tick/poll_index in a meaningful way
        for spoke in 0..self.l0.ticks_per_wheel {
            for slot_idx in 0..self.l0.tick_allocation {
                let wheel_index = (spoke << self.l0.allocation_bits_to_shift) + slot_idx;
                if let Some((deadline_ns, _)) = &self.l0.wheel[wheel_index] {
                    if *deadline_ns < earliest {
                        earliest = *deadline_ns;
                    }
                }
            }
        }

        self.l0.cached_next_deadline = earliest;
    }

    fn recompute_l1_deadline(&mut self) {
        if self.l1.timer_count == 0 {
            self.l1.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
            return;
        }

        let mut earliest = SingleWheel::<T>::NULL_DEADLINE;

        for spoke in 0..self.l1.ticks_per_wheel {
            for slot_idx in 0..self.l1.tick_allocation {
                let wheel_index = (spoke << self.l1.allocation_bits_to_shift) + slot_idx;
                if let Some((deadline_ns, _)) = &self.l1.wheel[wheel_index] {
                    if *deadline_ns < earliest {
                        earliest = *deadline_ns;
                    }
                }
            }
        }

        self.l1.cached_next_deadline = earliest;
    }

    #[inline]
    pub fn now_ns(&self) -> u64 {
        self.now_ns.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set_now_ns(&mut self, now_ns: u64) {
        self.now_ns.store(now_ns, Ordering::Release);
    }

    #[inline]
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    pub fn clear(&mut self) {
        for slot in &mut self.l0.wheel {
            *slot = None;
        }
        for slot in &mut self.l1.wheel {
            *slot = None;
        }
        self.l0.timer_count = 0;
        self.l1.timer_count = 0;
        self.l0.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
        self.l1.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
    }

    #[inline]
    fn encode_timer_id(level: u8, spoke: usize, slot: usize) -> u64 {
        ((level as u64) << 62) | ((spoke as u64) << 32) | (slot as u64)
    }

    #[inline]
    fn decode_timer_id(timer_id: u64) -> (u8, usize, usize) {
        let level = ((timer_id >> 62) & 0x3) as u8;
        let spoke = ((timer_id >> 32) & 0x3FFFFFFF) as usize;
        let slot = (timer_id & 0xFFFFFFFF) as usize;
        (level, spoke, slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_scheduling() {
        let mut wheel = TimerWheel::<u32>::new(Duration::from_nanos(1024 * 1024), 1024, 0);

        let timer_id = wheel.schedule_timer(100_000_000, 42).unwrap(); // 100ms
        assert_eq!(wheel.timer_count(), 1);

        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, 42);
        assert_eq!(wheel.timer_count(), 0);
    }

    #[test]
    fn test_l0_timer() {
        let mut wheel = TimerWheel::<&str>::new(Duration::from_nanos(1024 * 1024), 1024, 0);

        // 500ms timer should go to L0
        wheel.schedule_timer(500_000_000, "fast").unwrap();
        assert_eq!(wheel.l0.timer_count, 1);
        assert_eq!(wheel.l1.timer_count, 0);
    }

    #[test]
    fn test_l1_timer() {
        let mut wheel = TimerWheel::<&str>::new(Duration::from_nanos(1024 * 1024), 1024, 0);

        // 5s timer should go to L1
        wheel.schedule_timer(5_000_000_000, "slow").unwrap();
        assert_eq!(wheel.l0.timer_count, 0);
        assert_eq!(wheel.l1.timer_count, 1);
    }

    #[test]
    fn test_cascade() {
        let mut wheel = TimerWheel::<&str>::new(Duration::from_nanos(1024 * 1024), 1024, 0);

        // Schedule timer at 2 seconds (in L1 tick 1, which covers 1.07s-2.15s)
        wheel.schedule_timer(2_000_000_000, "cascaded").unwrap();
        assert_eq!(wheel.l1.timer_count, 1);
        assert_eq!(wheel.l0.timer_count, 0);

        // Poll at 2.2 seconds - should cascade tick 1 (we're now at tick 2)
        // L1 tick 2 starts at 2.147s, so polling at 2.2s triggers cascade of tick 1
        let mut output = Vec::new();
        wheel.poll(2_200_000_000, 100, &mut output);
        // Timer cascaded to L0 but hasn't fired yet (deadline is 2.0s, we're at 2.2s so it fires)
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].2, "cascaded");
    }
}
