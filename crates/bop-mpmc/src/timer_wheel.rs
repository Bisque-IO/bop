/// High-performance hashed timing wheel for efficient timer scheduling
/// Based on "Hashed and Hierarchical Timing Wheels" by Varghese and Lauck
///
/// O(1) timer scheduling and cancellation
/// Timers in same tick are not ordered relative to each other
/// NOT thread-safe - caller must synchronize
use std::time::{Duration, Instant};

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
    /// * `start_time` - Starting instant for the wheel
    /// * `tick_resolution` - Duration of each tick (must be power of 2 nanoseconds)
    /// * `ticks_per_wheel` - Number of spokes in the wheel (must be power of 2)
    pub fn new(start_time: Instant, tick_resolution: Duration, ticks_per_wheel: usize) -> Self {
        Self::with_allocation(
            start_time,
            tick_resolution,
            ticks_per_wheel,
            Self::INITIAL_TICK_ALLOCATION,
        )
    }

    /// Create timer wheel with custom initial allocation per tick
    pub fn with_allocation(
        start_time: Instant,
        tick_resolution: Duration,
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
    pub fn schedule_timer(&mut self, deadline_ns: u64, data: T) -> Option<u64> {
        let deadline_tick = ((deadline_ns.saturating_sub(self.start_time_ns))
            >> self.resolution_bits_to_shift)
            .max(self.current_tick);
        let spoke_index = (deadline_tick & self.tick_mask as u64) as usize;
        let tick_start_index = spoke_index << self.allocation_bits_to_shift;

        // Use hint for O(1) slot finding
        let hint = self.next_free_hint[spoke_index];
        let hint_index = tick_start_index + hint;

        // Fast path: hint points to free slot
        if hint < self.tick_allocation && self.wheel[hint_index].is_none() {
            self.wheel[hint_index] = Some((deadline_ns, data));
            self.timer_count += 1;
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
                // Update hint past this slot
                self.next_free_hint[spoke_index] = (slot_idx + 1) % self.tick_allocation;
                return Some(Self::timer_id_for_slot(spoke_index, slot_idx));
            }
        }

        // Need to increase capacity
        self.increase_capacity(deadline_ns, spoke_index, data)
    }

    /// Cancel a previously scheduled timer
    /// Returns the user data if timer was found and cancelled
    pub fn cancel_timer(&mut self, timer_id: u64) -> Option<T> {
        let spoke_index = Self::tick_for_timer_id(timer_id);
        let tick_index = Self::index_in_tick_array(timer_id);
        let wheel_index = (spoke_index << self.allocation_bits_to_shift) + tick_index;

        if spoke_index < self.ticks_per_wheel && tick_index < self.tick_allocation {
            if let Some((_, data)) = self.wheel[wheel_index].take() {
                self.timer_count -= 1;
                return Some(data);
            }
        }

        None
    }

    /// Poll for expired timers
    /// Returns vector of expired (timer_id, deadline_ns, data) tuples
    pub fn poll(&mut self, now_ns: u64, expiry_limit: usize) -> Vec<(u64, u64, T)> {
        let mut expired = Vec::new();

        if self.timer_count == 0 {
            if now_ns >= self.current_tick_time_ns() {
                self.current_tick += 1;
                self.poll_index = 0;
            }
            return expired;
        }

        let spoke_index = (self.current_tick & self.tick_mask as u64) as usize;

        for _ in 0..self.tick_allocation {
            if expired.len() >= expiry_limit {
                break;
            }

            let wheel_index = (spoke_index << self.allocation_bits_to_shift) + self.poll_index;

            if let Some((deadline, data)) = &self.wheel[wheel_index] {
                if now_ns >= *deadline {
                    let (deadline, data) = self.wheel[wheel_index].take().unwrap();
                    self.timer_count -= 1;

                    let timer_id = Self::timer_id_for_slot(spoke_index, self.poll_index);
                    expired.push((timer_id, deadline, data));
                }
            }

            self.poll_index = if self.poll_index + 1 >= self.tick_allocation {
                0
            } else {
                self.poll_index + 1
            };
        }

        if expired.len() < expiry_limit && now_ns >= self.current_tick_time_ns() {
            self.current_tick += 1;
            self.poll_index = 0;
        } else if self.poll_index >= self.tick_allocation {
            self.poll_index = 0;
        }

        expired
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

    /// Clear all timers
    pub fn clear(&mut self) {
        for slot in &mut self.wheel {
            *slot = None;
        }
        self.timer_count = 0;
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

        self.tick_allocation = new_tick_allocation;
        self.allocation_bits_to_shift = new_allocation_bits;
        self.wheel = new_wheel;

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

/// HashMap-based timing wheel with single slot per tick + overflow HashMap
/// Optimized for well-distributed timers with low collision rate
/// True O(1) operations with no fragmentation
pub struct TimerHashWheel<T> {
    /// Time resolution per tick (must be power of 2)
    tick_resolution_ns: u64,

    /// Start time of the wheel in nanoseconds
    start_time_ns: u64,

    /// Current tick position
    current_tick: u64,

    /// Number of active timers
    timer_count: u64,

    /// Number of ticks/spokes per wheel (must be power of 2)
    ticks_per_wheel: usize,

    /// Mask for tick calculation (ticks_per_wheel - 1)
    tick_mask: usize,

    /// Bit shift for resolution calculations
    resolution_bits_to_shift: u32,

    /// Next timer ID to allocate
    next_timer_id: u64,

    /// Primary slots: one per tick (fast path)
    primary_slots: Vec<Option<(u64, u64, T)>>, // (deadline_ns, timer_id, data)

    /// Overflow storage: HashMap per tick for collisions
    overflow: Vec<hashbrown::HashMap<u64, (u64, T)>>, // timer_id -> (deadline_ns, data)
}

impl<T> TimerHashWheel<T> {
    /// Create new timer hash wheel
    pub fn new(start_time: Instant, tick_resolution: Duration, ticks_per_wheel: usize) -> Self {
        assert!(
            ticks_per_wheel.is_power_of_two(),
            "ticks_per_wheel must be power of 2"
        );

        let tick_resolution_ns = tick_resolution.as_nanos() as u64;
        assert!(
            tick_resolution_ns.is_power_of_two(),
            "tick_resolution must be power of 2 ns"
        );

        let tick_mask = ticks_per_wheel - 1;
        let resolution_bits_to_shift = tick_resolution_ns.trailing_zeros();

        let mut primary_slots = Vec::with_capacity(ticks_per_wheel);
        primary_slots.resize_with(ticks_per_wheel, || None);

        let mut overflow = Vec::with_capacity(ticks_per_wheel);
        overflow.resize_with(ticks_per_wheel, hashbrown::HashMap::new);

        Self {
            tick_resolution_ns,
            start_time_ns: 0,
            current_tick: 0,
            timer_count: 0,
            ticks_per_wheel,
            tick_mask,
            resolution_bits_to_shift,
            next_timer_id: 1,
            primary_slots,
            overflow,
        }
    }

    /// Schedule a timer for an absolute deadline
    /// Returns timer_id for future cancellation
    pub fn schedule_timer(&mut self, deadline_ns: u64, data: T) -> u64 {
        let deadline_tick = ((deadline_ns.saturating_sub(self.start_time_ns))
            >> self.resolution_bits_to_shift)
            .max(self.current_tick);
        let spoke_index = (deadline_tick & self.tick_mask as u64) as usize;

        // Encode spoke_index in upper 32 bits for O(1) cancellation
        let timer_id = ((spoke_index as u64) << 32) | (self.next_timer_id & 0xFFFFFFFF);
        self.next_timer_id += 1;

        // Try primary slot first (fast path)
        if self.primary_slots[spoke_index].is_none() {
            self.primary_slots[spoke_index] = Some((deadline_ns, timer_id, data));
        } else {
            // Collision: use overflow HashMap
            self.overflow[spoke_index].insert(timer_id, (deadline_ns, data));
        }

        self.timer_count += 1;
        timer_id
    }

    /// Cancel a previously scheduled timer
    /// Returns the user data if timer was found and cancelled
    pub fn cancel_timer(&mut self, timer_id: u64) -> Option<T> {
        // Encode spoke_index in upper 32 bits of timer_id for O(1) lookup
        let spoke_index = (timer_id >> 32) as usize;

        if spoke_index >= self.ticks_per_wheel {
            return None;
        }

        // Check primary slot
        if let Some((_, id, _)) = &self.primary_slots[spoke_index] {
            if *id == timer_id {
                let (_, _, data) = self.primary_slots[spoke_index].take().unwrap();
                self.timer_count -= 1;
                return Some(data);
            }
        }

        // Check overflow
        if let Some((_, data)) = self.overflow[spoke_index].remove(&timer_id) {
            self.timer_count -= 1;
            return Some(data);
        }

        None
    }

    /// Poll for expired timers
    /// Returns vector of expired (timer_id, deadline_ns, data) tuples
    pub fn poll(&mut self, now_ns: u64, expiry_limit: usize) -> Vec<(u64, u64, T)> {
        let mut expired = Vec::new();

        if self.timer_count == 0 {
            if now_ns >= self.current_tick_time_ns() {
                self.current_tick += 1;
            }
            return expired;
        }

        let spoke_index = (self.current_tick & self.tick_mask as u64) as usize;

        // Check primary slot
        if let Some((deadline, timer_id, _)) = &self.primary_slots[spoke_index] {
            if now_ns >= *deadline {
                let (deadline, timer_id, data) = self.primary_slots[spoke_index].take().unwrap();
                self.timer_count -= 1;
                expired.push((timer_id, deadline, data));
            }
        }

        // Check overflow HashMap
        if expired.len() < expiry_limit {
            let overflow_map = &mut self.overflow[spoke_index];
            let mut to_remove = Vec::new();

            for (&timer_id, (deadline, _)) in overflow_map.iter() {
                if now_ns >= *deadline {
                    to_remove.push(timer_id);
                    if to_remove.len() + expired.len() >= expiry_limit {
                        break;
                    }
                }
            }

            for timer_id in to_remove {
                if let Some((deadline, data)) = overflow_map.remove(&timer_id) {
                    self.timer_count -= 1;
                    expired.push((timer_id, deadline, data));
                }
            }
        }

        if now_ns >= self.current_tick_time_ns() {
            self.current_tick += 1;
        }

        expired
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

    /// Clear all timers
    pub fn clear(&mut self) {
        for slot in &mut self.primary_slots {
            *slot = None;
        }
        for map in &mut self.overflow {
            map.clear();
        }
        self.timer_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_scheduling() {
        let start = Instant::now();
        let mut wheel = TimerWheel::<u32>::new(start, Duration::from_millis(10), 256);

        let timer_id = wheel.schedule_timer(100, 42).unwrap();
        assert_eq!(wheel.timer_count(), 1);

        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, 42);
        assert_eq!(wheel.timer_count(), 0);
    }

    #[test]
    fn test_expiry() {
        let start = Instant::now();
        let mut wheel = TimerWheel::<&str>::new(start, Duration::from_millis(10), 256);

        wheel.schedule_timer(5, "task1").unwrap();
        wheel.schedule_timer(15, "task2").unwrap();

        let expired = wheel.poll(10, 10);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].2, "task1");

        let expired = wheel.poll(20, 10);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].2, "task2");
    }

    #[test]
    fn test_hash_wheel_basic() {
        let start = Instant::now();
        let mut wheel = TimerHashWheel::<u32>::new(start, Duration::from_millis(10), 256);

        let timer_id = wheel.schedule_timer(100, 42);
        assert_eq!(wheel.timer_count(), 1);

        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, 42);
        assert_eq!(wheel.timer_count(), 0);
    }

    #[test]
    fn test_hash_wheel_collisions() {
        let start = Instant::now();
        let mut wheel = TimerHashWheel::<u32>::new(start, Duration::from_nanos(1024), 256);

        // Schedule multiple timers to same tick
        let id1 = wheel.schedule_timer(1024, 1);
        let id2 = wheel.schedule_timer(1024, 2);
        let id3 = wheel.schedule_timer(1024, 3);
        assert_eq!(wheel.timer_count(), 3);

        // Cancel middle one
        let data = wheel.cancel_timer(id2).unwrap();
        assert_eq!(data, 2);
        assert_eq!(wheel.timer_count(), 2);

        // Poll should get the other two
        let expired = wheel.poll(1024, 10);
        assert_eq!(expired.len(), 2);
        assert_eq!(wheel.timer_count(), 0);
    }
}
