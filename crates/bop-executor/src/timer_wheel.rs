/// High-performance hashed timing wheel for efficient timer scheduling
/// Based on "Hashed and Hierarchical Timing Wheels" by Varghese and Lauck
///
/// O(1) timer scheduling and cancellation
/// Timers in same tick are not ordered relative to each other
/// NOT thread-safe - caller must synchronize
use std::{
    cell::UnsafeCell,
    collections::HashMap,
    mem::ManuallyDrop,
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
    task::Poll,
    time::{Duration, Instant},
};

use crossbeam_deque::{Stealer, Worker};

use crate::{seg_spmc::SegSpmc, seg_spsc::SegSpsc, signal_waker::SignalWaker};

pub struct TimerWheel<T> {
    /// Represents a deadline slot not set in the wheel
    null_deadline: u64,

    /// Time resolution per tick (must be power of 2)
    tick_resolution_ns: u64,

    /// Start time of the wheel in nanoseconds since UNIX_EPOCH
    start_time_ns: u64,

    /// Current tick position
    current_tick: u64,

    /// Number of active timers
    timer_count: u64,

    /// Cached next deadline for efficient querying
    /// Set to null_deadline when no timers scheduled or cache is invalid
    cached_next_deadline: u64,

    /// Current time in nanoseconds (updated during poll and by supervisor thread)
    /// Atomic because supervisor thread writes while worker thread reads
    now_ns: AtomicU64,

    /// Worker ID that owns this timer wheel
    worker_id: u32,

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
    /// Each entry is Option<(deadline_ns, user_data)>
    wheel: Vec<Option<(u64, T)>>,

    /// Per-tick next available slot hint for O(1) scheduling
    /// Tracks the next potentially free slot in each tick
    next_free_hint: Vec<usize>,
}

impl<T> TimerWheel<T> {
    const INITIAL_TICK_ALLOCATION: usize = 16;
    const NULL_DEADLINE: u64 = u64::MAX;

    /// Create new timer wheel
    ///
    /// # Arguments
    /// * `tick_resolution` - Duration of each tick (must be power of 2 nanoseconds)
    /// * `ticks_per_wheel` - Number of spokes in the wheel (must be power of 2)
    /// * `worker_id` - Worker ID that owns this timer wheel
    ///
    /// # Timer Deadlines
    /// All timer deadlines are specified as nanoseconds elapsed since wheel creation.
    /// This allows the wheel to track time independently without needing an external clock source.
    pub fn new(tick_resolution: Duration, ticks_per_wheel: usize, worker_id: u32) -> Self {
        Self::with_allocation(
            tick_resolution,
            ticks_per_wheel,
            Self::INITIAL_TICK_ALLOCATION,
            worker_id,
        )
    }

    /// Create timer wheel with custom initial allocation per tick
    ///
    /// # Arguments
    /// * `tick_resolution` - Duration of each tick (must be power of 2 nanoseconds)
    /// * `ticks_per_wheel` - Number of spokes in the wheel (must be power of 2)
    /// * `initial_tick_allocation` - Initial number of timer slots per spoke (must be power of 2)
    /// * `worker_id` - Worker ID that owns this timer wheel
    pub fn with_allocation(
        tick_resolution: Duration,
        ticks_per_wheel: usize,
        initial_tick_allocation: usize,
        worker_id: u32,
    ) -> Self {
        assert!(
            ticks_per_wheel.is_power_of_two(),
            "ticks_per_wheel must be power of 2"
        );
        assert!(
            initial_tick_allocation.is_power_of_two(),
            "tick_allocation must be power of 2"
        );

        let tick_resolution_ns = tick_resolution.as_nanos() as u64;
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

        // Initialize free slot hints (all ticks start at slot 0)
        let mut next_free_hint = Vec::with_capacity(ticks_per_wheel);
        next_free_hint.resize(ticks_per_wheel, 0);

        // Use elapsed time as our time base (nanoseconds since start)
        let start_time_ns = 0; // We'll measure everything relative to start_time

        Self {
            null_deadline: Self::NULL_DEADLINE,
            tick_resolution_ns,
            start_time_ns,
            current_tick: 0,
            timer_count: 0,
            cached_next_deadline: Self::NULL_DEADLINE,
            now_ns: AtomicU64::new(0),
            worker_id,
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

    /// Schedule a timer for an absolute deadline
    /// Returns timer_id for future cancellation, or None if failed
    ///
    /// Timers with deadlines in the past will be placed in their natural spoke
    /// and will fire on the next poll() call.
    pub fn schedule_timer(&mut self, deadline_ns: u64, data: T) -> Option<u64> {
        let deadline_tick =
            (deadline_ns.saturating_sub(self.start_time_ns)) >> self.resolution_bits_to_shift;
        // Don't use .max(current_tick) - let the hash table work naturally
        // Expired timers go in their correct spoke and will be found when poll() scans
        let spoke_index = (deadline_tick & self.tick_mask as u64) as usize;
        let tick_start_index = spoke_index << self.allocation_bits_to_shift;

        // Use hint for O(1) slot finding
        let hint = self.next_free_hint[spoke_index];
        let hint_index = tick_start_index + hint;

        // Fast path: hint points to free slot
        if hint < self.tick_allocation && self.wheel[hint_index].is_none() {
            self.wheel[hint_index] = Some((deadline_ns, data));
            self.timer_count += 1;
            // Update cached deadline if this is earlier
            if deadline_ns < self.cached_next_deadline {
                self.cached_next_deadline = deadline_ns;
            }
            // Update hint to next slot
            self.next_free_hint[spoke_index] = hint + 1;
            return Some(Self::timer_id_for_slot(spoke_index, hint));
        }

        // Slow path: hint was wrong, linear search from hint position
        for i in 0..self.tick_allocation {
            let slot_idx = (hint + i) % self.tick_allocation;
            let index = tick_start_index + slot_idx;

            if self.wheel[index].is_none() {
                self.wheel[index] = Some((deadline_ns, data));
                self.timer_count += 1;
                // Update cached deadline if this is earlier
                if deadline_ns < self.cached_next_deadline {
                    self.cached_next_deadline = deadline_ns;
                }
                // Update hint past this slot
                self.next_free_hint[spoke_index] = (slot_idx + 1) % self.tick_allocation;
                return Some(Self::timer_id_for_slot(spoke_index, slot_idx));
            }
        }

        // Need to increase capacity
        let result = self.increase_capacity(deadline_ns, spoke_index, data);
        // Update cached deadline if successful and this is earlier
        if result.is_some() && deadline_ns < self.cached_next_deadline {
            self.cached_next_deadline = deadline_ns;
        }
        result
    }

    /// Cancel a previously scheduled timer
    /// Returns the user data if timer was found and cancelled
    pub fn cancel_timer(&mut self, timer_id: u64) -> Option<T> {
        let spoke_index = Self::tick_for_timer_id(timer_id);
        let tick_index = Self::index_in_tick_array(timer_id);
        let wheel_index = (spoke_index << self.allocation_bits_to_shift) + tick_index;

        if spoke_index < self.ticks_per_wheel && tick_index < self.tick_allocation {
            if let Some((deadline_ns, data)) = self.wheel[wheel_index].take() {
                self.timer_count -= 1;

                // If we canceled the cached next deadline, we need to recompute
                if deadline_ns == self.cached_next_deadline {
                    self.recompute_cached_deadline();
                }

                return Some(data);
            }
        }

        None
    }

    /// Poll for expired timers
    /// Writes expired `(timer_id, deadline_ns, data)` tuples into the provided output buffer.
    /// The buffer is cleared before writing, and at most `expiry_limit` items are emitted.
    pub fn poll(
        &mut self,
        now_ns: u64,
        expiry_limit: usize,
        output: &mut Vec<(u64, u64, T)>,
    ) -> usize {
        // Update current time
        self.now_ns.store(now_ns, Ordering::Release);

        output.clear();

        if self.timer_count == 0 {
            self.advance_to(now_ns);
            return 0;
        }

        // Don't skip ticks - scan each spoke that needs to be checked
        let target_tick =
            (now_ns.saturating_sub(self.start_time_ns)) >> self.resolution_bits_to_shift;

        // Continue scanning until we reach the target tick or hit expiry limit
        while self.current_tick <= target_tick
            && output.len() < expiry_limit
            && self.timer_count > 0
        {
            let spoke_index = (self.current_tick & self.tick_mask as u64) as usize;

            // Scan all slots in this spoke starting from poll_index
            while self.poll_index < self.tick_allocation {
                if output.len() >= expiry_limit {
                    // Hit expiry limit - return without advancing tick
                    // Next poll() will continue from current position
                    return output.len();
                }

                let wheel_index = (spoke_index << self.allocation_bits_to_shift) + self.poll_index;

                if let Some((deadline, data)) = &self.wheel[wheel_index] {
                    if now_ns >= *deadline {
                        let (deadline, data) = self.wheel[wheel_index].take().unwrap();
                        self.timer_count -= 1;

                        let timer_id = Self::timer_id_for_slot(spoke_index, self.poll_index);
                        output.push((timer_id, deadline, data));

                        // If we polled the cached next deadline, mark cache as needing update
                        if deadline == self.cached_next_deadline {
                            self.cached_next_deadline = self.null_deadline;
                        }
                    }
                }

                self.poll_index += 1;
            }

            // Finished scanning this spoke, move to next tick
            self.current_tick += 1;
            self.poll_index = 0;
        }

        output.len()
    }

    /// Advance current tick to the given time
    pub fn advance_to(&mut self, now_ns: u64) {
        let new_tick = (now_ns.saturating_sub(self.start_time_ns)) >> self.resolution_bits_to_shift;
        self.current_tick = self.current_tick.max(new_tick);
    }

    /// Get current tick time in nanoseconds
    pub fn current_tick_time_ns(&self) -> u64 {
        ((self.current_tick + 1) << self.resolution_bits_to_shift) + self.start_time_ns
    }

    /// Number of active timers
    pub fn timer_count(&self) -> u64 {
        self.timer_count
    }

    /// Returns the deadline of the next timer to expire, or None if no timers are scheduled.
    ///
    /// Uses a cached value that is maintained during schedule/cancel/poll operations.
    /// The cache is lazily recomputed only when necessary (after canceling/polling the
    /// earliest timer).
    ///
    /// # Performance
    ///
    /// O(1) in the common case (cache hit).
    /// O(n) when cache needs recomputation (after canceling/polling the earliest timer).
    #[inline]
    pub fn next_deadline(&mut self) -> Option<u64> {
        if self.timer_count == 0 {
            return None;
        }

        // Lazy recomputation if cache is invalid
        if self.cached_next_deadline == self.null_deadline {
            self.recompute_cached_deadline();
        }

        if self.cached_next_deadline == self.null_deadline {
            None
        } else {
            Some(self.cached_next_deadline)
        }
    }

    /// Get the current time in nanoseconds
    /// This is updated each time poll() is called
    /// Uses Relaxed ordering since exact synchronization with updates is not required
    #[inline]
    pub fn now_ns(&self) -> u64 {
        self.now_ns.load(Ordering::Relaxed)
    }

    /// Update the current time in nanoseconds
    /// Typically updated via poll(), but can be set manually by supervisor thread
    /// Uses Release ordering so timer scheduling sees up-to-date time
    #[inline]
    pub fn set_now_ns(&mut self, now_ns: u64) {
        self.now_ns.store(now_ns, Ordering::Release);
    }

    /// Get the worker ID that owns this timer wheel
    #[inline]
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// Recompute the cached next deadline by scanning all slots.
    /// Called when the cache is invalidated (after cancel/poll of earliest timer).
    fn recompute_cached_deadline(&mut self) {
        if self.timer_count == 0 {
            self.cached_next_deadline = self.null_deadline;
            return;
        }

        let mut earliest = self.null_deadline;
        let spoke_index = (self.current_tick & self.tick_mask as u64) as usize;

        // Start from current position and scan the entire wheel
        // This matches the poll order: current spoke from poll_index onwards,
        // then remaining spokes, wrapping around
        for spoke_offset in 0..self.ticks_per_wheel {
            let spoke = (spoke_index + spoke_offset) % self.ticks_per_wheel;
            let start_slot = if spoke == spoke_index {
                self.poll_index
            } else {
                0
            };

            for slot_idx in start_slot..self.tick_allocation {
                let wheel_index = (spoke << self.allocation_bits_to_shift) + slot_idx;
                if let Some((deadline_ns, _)) = &self.wheel[wheel_index] {
                    if *deadline_ns < earliest {
                        earliest = *deadline_ns;
                    }
                }
            }
        }

        self.cached_next_deadline = earliest;
    }

    /// Clear all timers
    pub fn clear(&mut self) {
        for slot in &mut self.wheel {
            *slot = None;
        }
        self.timer_count = 0;
        self.cached_next_deadline = self.null_deadline;
    }

    fn increase_capacity(&mut self, deadline_ns: u64, spoke_index: usize, data: T) -> Option<u64> {
        let new_tick_allocation = self.tick_allocation << 1;
        let new_allocation_bits = new_tick_allocation.trailing_zeros();

        let new_capacity = self.ticks_per_wheel * new_tick_allocation;
        if new_capacity > (1 << 30) {
            return None; // Max capacity reached
        }

        let mut new_wheel = Vec::with_capacity(new_capacity);
        new_wheel.resize_with(new_capacity, || None);

        // Copy old data to new wheel
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

        let timer_id = Self::timer_id_for_slot(spoke_index, self.tick_allocation);
        self.timer_count += 1;

        let old_allocation = self.tick_allocation;
        self.tick_allocation = new_tick_allocation;
        self.allocation_bits_to_shift = new_allocation_bits;
        self.wheel = new_wheel;

        // Reset all free hints to point to the start of newly allocated region
        // This ensures O(1) performance for subsequent schedules
        for hint in &mut self.next_free_hint {
            *hint = old_allocation;
        }

        Some(timer_id)
    }

    #[inline]
    fn timer_id_for_slot(tick_on_wheel: usize, tick_array_index: usize) -> u64 {
        ((tick_on_wheel as u64) << 32) | (tick_array_index as u64)
    }

    #[inline]
    fn tick_for_timer_id(timer_id: u64) -> usize {
        (timer_id >> 32) as usize
    }

    #[inline]
    fn index_in_tick_array(timer_id: u64) -> usize {
        timer_id as u32 as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_scheduling() {
        let start = Instant::now();
        let mut wheel = TimerWheel::<u32>::new(start, Duration::from_nanos(1024 * 1024), 256, 0);

        let timer_id = wheel.schedule_timer(100, 42).unwrap();
        assert_eq!(wheel.timer_count(), 1);

        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, 42);
        assert_eq!(wheel.timer_count(), 0);
    }

    #[test]
    fn test_expiry() {
        let start = Instant::now();
        let mut wheel = TimerWheel::<&str>::new(start, Duration::from_nanos(1024 * 1024), 256, 0);

        wheel.schedule_timer(5, "task1").unwrap();
        wheel.schedule_timer(15, "task2").unwrap();

        let mut expired = Vec::new();
        wheel.poll(10, 10, &mut expired);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].2, "task1");

        wheel.poll(20, 10, &mut expired);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].2, "task2");
    }
}
