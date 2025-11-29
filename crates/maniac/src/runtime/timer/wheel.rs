/// High-performance hierarchical hashed timing wheel for efficient timer scheduling
/// Based on "Hashed and Hierarchical Timing Wheels" by Varghese and Lauck
///
/// Three-level hierarchy with tick-based polling (no cascading):
/// - L0 (Fast): ~1ms ticks × 1024 = ~1s coverage
/// - L1 (Medium): ~1s ticks × 1024 = ~17min coverage
/// - L2 (Slow): ~64s ticks × 1024 = ~18hr coverage
///
/// Key design decisions:
/// - **No cascading**: Timers stay in their original wheel, polled when tick advances
/// - **Stable timer IDs**: Since timers don't move, IDs remain valid until expiry/cancel
/// - **Approximate timeouts**: Deadlines rounded up to spoke boundary
/// - **Per-spoke vectors**: Each spoke grows independently with free list for slot reuse
/// - **Automatic compaction**: Spokes shrink after polling when utilization is low
///
/// Complexity:
/// - Schedule: O(1) amortized
/// - Cancel: O(1)
/// - Poll: O(expired_timers) per wheel, wheels polled based on tick advancement
///
/// NOT thread-safe - caller must synchronize
use std::{sync::atomic::AtomicU64, time::Duration};

/// Error types for timer wheel operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimerWheelError {
    /// Timer ID exceeds maximum supported value
    InvalidTimerId,
    /// Timer deadline is invalid (zero, in the past, or overflow)
    InvalidDeadline,
    /// Timer deadline exceeds maximum supported duration
    DeadlineTooFar,
    /// Capacity limit exceeded
    CapacityExceeded,
    /// Integer overflow in calculations
    Overflow,
    /// Timer ID does not correspond to a valid timer
    TimerNotFound,
}

/// Configuration for spoke compaction behavior
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Minimum spoke capacity before compaction is considered
    pub min_capacity_for_compaction: usize,
    /// Compact when active entries < capacity / compaction_ratio
    pub compaction_ratio: usize,
    /// Target capacity after compaction (as multiple of active entries)
    pub target_capacity_multiplier: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            min_capacity_for_compaction: 64,
            compaction_ratio: 4, // Compact when < 25% utilized
            target_capacity_multiplier: 2,
        }
    }
}

/// Timer entry stored in a spoke
#[derive(Debug)]
struct TimerEntry<T> {
    /// Absolute deadline in nanoseconds since wheel start
    deadline_ns: u64,
    /// Slot index within the spoke (for free list management)
    slot_index: u32,
    /// Generation counter to detect stale references after compaction
    generation: u32,
    /// User data associated with this timer
    data: T,
}

/// A single spoke (bucket) in the timing wheel
///
/// Each spoke holds timers that hash to the same tick. Uses a free list
/// for O(1) slot reuse and supports compaction to reclaim memory.
#[derive(Debug)]
struct Spoke<T> {
    /// Timer entries - Some for active, None for free slots
    entries: Vec<Option<TimerEntry<T>>>,
    /// Stack of free slot indices for O(1) allocation
    free_list: Vec<usize>,
    /// Number of active timers in this spoke
    active_count: usize,
    /// Generation counter incremented on compaction
    generation: u32,
}

impl<T> Spoke<T> {
    fn new(initial_capacity: usize) -> Self {
        let mut entries = Vec::with_capacity(initial_capacity);
        entries.resize_with(initial_capacity, || None);

        // Initialize free list with all slots
        let free_list: Vec<usize> = (0..initial_capacity).rev().collect();

        Self {
            entries,
            free_list,
            active_count: 0,
            generation: 0,
        }
    }

    /// Insert a timer entry, returns slot index
    #[inline]
    fn insert(&mut self, deadline_ns: u64, data: T) -> usize {
        let slot_index = if let Some(idx) = self.free_list.pop() {
            idx
        } else {
            // Grow the spoke
            let new_idx = self.entries.len();
            self.entries.push(None);
            new_idx
        };

        self.entries[slot_index] = Some(TimerEntry {
            deadline_ns,
            slot_index: slot_index as u32,
            generation: self.generation,
            data,
        });
        self.active_count += 1;

        slot_index
    }

    /// Remove a timer by slot index, returns data if found and generation matches
    #[inline]
    fn remove(&mut self, slot_index: usize, expected_generation: u32) -> Option<T> {
        if slot_index >= self.entries.len() {
            return None;
        }

        if let Some(entry) = self.entries[slot_index].take() {
            if entry.generation == expected_generation {
                self.free_list.push(slot_index);
                self.active_count -= 1;
                return Some(entry.data);
            } else {
                // Generation mismatch - put it back
                self.entries[slot_index] = Some(entry);
            }
        }

        None
    }

    /// Remove a timer by slot index without generation check (for polling)
    #[inline]
    fn remove_unchecked(&mut self, slot_index: usize) -> Option<(u64, T)> {
        if let Some(entry) = self.entries[slot_index].take() {
            self.free_list.push(slot_index);
            self.active_count -= 1;
            Some((entry.deadline_ns, entry.data))
        } else {
            None
        }
    }

    /// Check if compaction would be beneficial
    #[inline]
    fn should_compact(&self, config: &CompactionConfig) -> bool {
        let capacity = self.entries.len();
        capacity >= config.min_capacity_for_compaction
            && self.active_count * config.compaction_ratio < capacity
    }

    /// Compact the spoke to reclaim memory
    ///
    /// Moves all active entries to the front and shrinks the vector.
    /// Increments generation to invalidate outstanding timer IDs.
    fn compact(&mut self, config: &CompactionConfig) -> Vec<(usize, usize)> {
        if self.active_count == 0 {
            // Reset to minimal state
            self.entries.clear();
            self.entries.shrink_to(config.min_capacity_for_compaction);
            self.free_list.clear();
            self.generation = self.generation.wrapping_add(1);
            return Vec::new();
        }

        let target_capacity = (self.active_count * config.target_capacity_multiplier)
            .max(config.min_capacity_for_compaction)
            .next_power_of_two();

        // Collect active entries
        let mut active_entries: Vec<TimerEntry<T>> = Vec::with_capacity(self.active_count);
        let mut slot_remapping = Vec::new();

        for (old_idx, slot) in self.entries.iter_mut().enumerate() {
            if let Some(entry) = slot.take() {
                let new_idx = active_entries.len();
                slot_remapping.push((old_idx, new_idx));
                active_entries.push(entry);
            }
        }

        // Increment generation
        self.generation = self.generation.wrapping_add(1);

        // Rebuild entries vec
        self.entries.clear();
        self.entries.reserve(target_capacity);

        for (new_idx, mut entry) in active_entries.into_iter().enumerate() {
            entry.slot_index = new_idx as u32;
            entry.generation = self.generation;
            self.entries.push(Some(entry));
        }

        // Fill remaining capacity with None
        self.entries.resize_with(target_capacity, || None);

        // Rebuild free list
        self.free_list.clear();
        for i in (self.active_count..target_capacity).rev() {
            self.free_list.push(i);
        }

        slot_remapping
    }

    /// Get the current generation
    #[inline]
    fn generation(&self) -> u32 {
        self.generation
    }

    /// Get capacity
    #[inline]
    fn capacity(&self) -> usize {
        self.entries.len()
    }
}

/// Single-level timing wheel with per-spoke storage
pub struct SingleWheel<T> {
    /// Spokes (buckets) of the wheel
    spokes: Vec<Spoke<T>>,

    /// Number of spokes (must be power of 2)
    num_spokes: usize,

    /// Mask for spoke calculation (num_spokes - 1)
    spoke_mask: usize,

    /// Tick resolution in nanoseconds (must be power of 2)
    tick_resolution_ns: u64,

    /// Bit shift for tick calculation (log2 of tick_resolution_ns)
    tick_shift: u32,

    /// Current tick position
    current_tick: u64,

    /// Total number of active timers
    timer_count: u64,

    /// Cached earliest deadline (NULL_DEADLINE if invalid)
    cached_next_deadline: u64,

    /// Start time in nanoseconds
    start_time_ns: u64,

    /// Compaction configuration
    compaction_config: CompactionConfig,
}

impl<T> SingleWheel<T> {
    const NULL_DEADLINE: u64 = u64::MAX;
    const DEFAULT_INITIAL_SPOKE_CAPACITY: usize = 16;

    /// Create a new single-level timing wheel
    pub fn new(tick_resolution_ns: u64, num_spokes: usize) -> Self {
        Self::with_config(
            tick_resolution_ns,
            num_spokes,
            Self::DEFAULT_INITIAL_SPOKE_CAPACITY,
            CompactionConfig::default(),
        )
    }

    /// Create with custom configuration
    pub fn with_config(
        tick_resolution_ns: u64,
        num_spokes: usize,
        initial_spoke_capacity: usize,
        compaction_config: CompactionConfig,
    ) -> Self {
        assert!(
            num_spokes.is_power_of_two(),
            "num_spokes must be power of 2"
        );
        assert!(
            tick_resolution_ns.is_power_of_two(),
            "tick_resolution_ns must be power of 2"
        );

        let spokes: Vec<Spoke<T>> = (0..num_spokes)
            .map(|_| Spoke::new(initial_spoke_capacity))
            .collect();

        Self {
            spokes,
            num_spokes,
            spoke_mask: num_spokes - 1,
            tick_resolution_ns,
            tick_shift: tick_resolution_ns.trailing_zeros(),
            current_tick: 0,
            timer_count: 0,
            cached_next_deadline: Self::NULL_DEADLINE,
            start_time_ns: 0,
            compaction_config,
        }
    }

    /// Set start time
    #[inline]
    pub fn set_start_time_ns(&mut self, start_time_ns: u64) {
        self.start_time_ns = start_time_ns;
    }

    /// Get tick resolution
    #[inline]
    pub fn tick_resolution_ns(&self) -> u64 {
        self.tick_resolution_ns
    }

    /// Get tick shift
    #[inline]
    pub fn tick_shift(&self) -> u32 {
        self.tick_shift
    }

    /// Get coverage in nanoseconds
    #[inline]
    pub fn coverage_ns(&self) -> u64 {
        self.tick_resolution_ns * self.num_spokes as u64
    }

    /// Schedule a timer, returns (spoke_index, slot_index, generation)
    #[inline]
    pub fn schedule(
        &mut self,
        deadline_ns: u64,
    ) -> Result<(usize, usize, u32), TimerWheelError>
    where
        T: Default,
    {
        self.schedule_with_data(deadline_ns, T::default())
    }

    /// Schedule a timer with data, returns (spoke_index, slot_index, generation)
    #[inline]
    pub fn schedule_with_data(
        &mut self,
        deadline_ns: u64,
        data: T,
    ) -> Result<(usize, usize, u32), TimerWheelError> {
        // Calculate which tick/spoke this deadline belongs to
        let relative_ns = deadline_ns.saturating_sub(self.start_time_ns);
        let tick = relative_ns >> self.tick_shift;
        let spoke_index = (tick as usize) & self.spoke_mask;

        let spoke = &mut self.spokes[spoke_index];
        let slot_index = spoke.insert(deadline_ns, data);
        let generation = spoke.generation();

        self.timer_count += 1;

        // Update cached deadline
        if deadline_ns < self.cached_next_deadline {
            self.cached_next_deadline = deadline_ns;
        }

        Ok((spoke_index, slot_index, generation))
    }

    /// Cancel a timer by spoke, slot, and generation
    #[inline]
    pub fn cancel(
        &mut self,
        spoke_index: usize,
        slot_index: usize,
        generation: u32,
    ) -> Result<T, TimerWheelError> {
        if spoke_index >= self.num_spokes {
            return Err(TimerWheelError::InvalidTimerId);
        }

        let spoke = &mut self.spokes[spoke_index];

        if let Some(data) = spoke.remove(slot_index, generation) {
            self.timer_count -= 1;
            self.cached_next_deadline = Self::NULL_DEADLINE; // Invalidate cache
            Ok(data)
        } else {
            Err(TimerWheelError::TimerNotFound)
        }
    }

    /// Poll for expired timers in spokes from previous_tick to current_tick
    ///
    /// Returns number of expired timers found.
    pub fn poll(
        &mut self,
        now_ns: u64,
        previous_tick: u64,
        current_tick: u64,
        expiry_limit: usize,
        output: &mut Vec<(usize, usize, u32, u64, T)>, // (spoke, slot, gen, deadline, data)
    ) -> usize {
        if self.timer_count == 0 {
            self.current_tick = current_tick;
            return 0;
        }

        let mut expired_count = 0;

        // Calculate how many ticks to scan
        let ticks_to_scan = if current_tick >= previous_tick {
            ((current_tick - previous_tick) as usize + 1).min(self.num_spokes)
        } else {
            self.num_spokes
        };

        // Scan spokes for the ticks that have passed
        for tick_offset in 0..ticks_to_scan {
            if expired_count >= expiry_limit {
                break;
            }

            let spoke_index = ((previous_tick as usize) + tick_offset) & self.spoke_mask;
            let spoke = &mut self.spokes[spoke_index];

            // Scan all entries in this spoke
            for slot_idx in 0..spoke.entries.len() {
                if expired_count >= expiry_limit {
                    break;
                }

                if let Some(entry) = spoke.entries[slot_idx].as_ref() {
                    if entry.deadline_ns <= now_ns {
                        let generation = entry.generation;
                        if let Some((deadline_ns, data)) = spoke.remove_unchecked(slot_idx) {
                            self.timer_count -= 1;
                            output.push((spoke_index, slot_idx, generation, deadline_ns, data));
                            expired_count += 1;
                        }
                    }
                }
            }

            // Consider compaction after polling a spoke
            if spoke.should_compact(&self.compaction_config) {
                let _ = spoke.compact(&self.compaction_config);
            }
        }

        self.current_tick = current_tick;

        if expired_count > 0 {
            self.cached_next_deadline = Self::NULL_DEADLINE;
        }

        expired_count
    }

    /// Get the earliest deadline (recomputes if cache invalid)
    pub fn next_deadline(&mut self) -> Option<u64> {
        if self.timer_count == 0 {
            return None;
        }

        if self.cached_next_deadline != Self::NULL_DEADLINE {
            return Some(self.cached_next_deadline);
        }

        // Recompute by scanning all spokes
        let mut earliest = Self::NULL_DEADLINE;

        for spoke in &self.spokes {
            for entry in &spoke.entries {
                if let Some(e) = entry {
                    if e.deadline_ns < earliest {
                        earliest = e.deadline_ns;
                    }
                }
            }
        }

        self.cached_next_deadline = earliest;

        if earliest != Self::NULL_DEADLINE {
            Some(earliest)
        } else {
            None
        }
    }

    /// Get timer count
    #[inline]
    pub fn timer_count(&self) -> u64 {
        self.timer_count
    }

    /// Get current tick
    #[inline]
    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    /// Clear all timers
    pub fn clear(&mut self) {
        for spoke in &mut self.spokes {
            spoke.entries.clear();
            spoke.free_list.clear();
            spoke.active_count = 0;
        }
        self.timer_count = 0;
        self.cached_next_deadline = Self::NULL_DEADLINE;
        self.current_tick = 0;
    }

    /// Force compaction of all spokes
    pub fn compact_all(&mut self) {
        for spoke in &mut self.spokes {
            if spoke.active_count < spoke.capacity() {
                spoke.compact(&self.compaction_config);
            }
        }
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> WheelMemoryStats {
        let mut total_capacity = 0;
        let mut total_active = 0;

        for spoke in &self.spokes {
            total_capacity += spoke.capacity();
            total_active += spoke.active_count;
        }

        WheelMemoryStats {
            num_spokes: self.num_spokes,
            total_slot_capacity: total_capacity,
            active_timers: total_active,
            utilization: if total_capacity > 0 {
                total_active as f64 / total_capacity as f64
            } else {
                0.0
            },
        }
    }
}

/// Memory statistics for a wheel
#[derive(Debug, Clone)]
pub struct WheelMemoryStats {
    pub num_spokes: usize,
    pub total_slot_capacity: usize,
    pub active_timers: usize,
    pub utilization: f64,
}

/// High-performance hierarchical timing wheel
///
/// Three-level hierarchy with no cascading:
/// - L0: ~1ms resolution, ~1s coverage (polled every tick)
/// - L1: ~1s resolution, ~17min coverage (polled when L1 tick advances)
/// - L2: ~64s resolution, ~18hr coverage (polled when L2 tick advances)
///
/// Timer IDs are stable - timers never move between wheels.
pub struct TimerWheel<T> {
    /// L0 wheel: fast, fine-grained timers
    l0: SingleWheel<T>,
    /// L1 wheel: medium-term timers
    l1: SingleWheel<T>,
    /// L2 wheel: long-term timers
    l2: SingleWheel<T>,

    /// Start time in nanoseconds
    start_time_ns: u64,

    /// Current time (atomic for potential cross-thread reads)
    now_ns: AtomicU64,

    /// Previous tick positions for each level
    prev_l0_tick: u64,
    prev_l1_tick: u64,
    prev_l2_tick: u64,

    /// Worker ID
    worker_id: u32,

    /// Coverage thresholds (precomputed)
    l0_coverage_ns: u64,
    l1_coverage_ns: u64,
    l2_coverage_ns: u64,
}

impl<T> TimerWheel<T> {
    /// Default tick resolutions (power of 2 nanoseconds)
    pub const L0_TICK_NS: u64 = 1 << 20; // ~1.05ms
    pub const L1_TICK_NS: u64 = 1 << 30; // ~1.07s
    pub const L2_TICK_NS: u64 = 1 << 36; // ~68.7s

    /// Default number of spokes per wheel
    pub const DEFAULT_SPOKES: usize = 1024;

    /// Create a new hierarchical timer wheel with default configuration
    pub fn new(worker_id: u32) -> Self {
        Self::with_config(
            Self::L0_TICK_NS,
            Self::L1_TICK_NS,
            Self::L2_TICK_NS,
            Self::DEFAULT_SPOKES,
            worker_id,
        )
    }

    /// Create with custom tick resolutions
    pub fn with_config(
        l0_tick_ns: u64,
        l1_tick_ns: u64,
        l2_tick_ns: u64,
        spokes_per_wheel: usize,
        worker_id: u32,
    ) -> Self {
        let l0 = SingleWheel::new(l0_tick_ns, spokes_per_wheel);
        let l1 = SingleWheel::new(l1_tick_ns, spokes_per_wheel);
        let l2 = SingleWheel::new(l2_tick_ns, spokes_per_wheel);

        let l0_coverage_ns = l0.coverage_ns();
        let l1_coverage_ns = l1.coverage_ns();
        let l2_coverage_ns = l2.coverage_ns();

        Self {
            l0,
            l1,
            l2,
            start_time_ns: 0,
            now_ns: AtomicU64::new(0),
            prev_l0_tick: 0,
            prev_l1_tick: 0,
            prev_l2_tick: 0,
            worker_id,
            l0_coverage_ns,
            l1_coverage_ns,
            l2_coverage_ns,
        }
    }

    /// Create from Duration-based configuration (for backwards compatibility)
    pub fn from_duration(tick_resolution: Duration, ticks_per_wheel: usize, worker_id: u32) -> Self {
        let l0_tick_ns = tick_resolution.as_nanos() as u64;
        let l1_tick_ns = l0_tick_ns * ticks_per_wheel as u64;
        let l2_tick_ns = l1_tick_ns * ticks_per_wheel as u64;

        Self::with_config(l0_tick_ns, l1_tick_ns, l2_tick_ns, ticks_per_wheel, worker_id)
    }

    /// Set start time
    #[inline]
    pub fn set_start_time_ns(&mut self, start_time_ns: u64) {
        self.start_time_ns = start_time_ns;
        self.now_ns
            .store(start_time_ns, std::sync::atomic::Ordering::Release);
        self.l0.set_start_time_ns(start_time_ns);
        self.l1.set_start_time_ns(start_time_ns);
        self.l2.set_start_time_ns(start_time_ns);
    }

    /// Get start time
    #[inline]
    pub fn start_time_ns(&self) -> u64 {
        self.start_time_ns
    }

    /// Get maximum supported timer duration in nanoseconds
    #[inline]
    pub fn max_duration_ns(&self) -> u64 {
        self.l2_coverage_ns
    }

    /// Schedule a timer for an absolute deadline
    ///
    /// Returns a timer ID that remains stable until the timer expires or is cancelled.
    #[inline]
    pub fn schedule_timer(&mut self, deadline_ns: u64, data: T) -> Result<u64, TimerWheelError> {
        let now_ns = self.now_ns.load(std::sync::atomic::Ordering::Acquire);

        // Validate deadline
        if deadline_ns == 0 || deadline_ns < self.start_time_ns {
            return Err(TimerWheelError::InvalidDeadline);
        }

        // Allow scheduling slightly in the past (will fire immediately on next poll)
        // but reject if too far in the past
        if deadline_ns < now_ns.saturating_sub(self.l0.tick_resolution_ns()) {
            return Err(TimerWheelError::InvalidDeadline);
        }

        // Determine which wheel based on relative deadline
        let relative_ns = deadline_ns.saturating_sub(self.start_time_ns);

        let (level, spoke, slot, generation) = if relative_ns < self.l0_coverage_ns {
            let (spoke, slot, generation) = self.l0.schedule_with_data(deadline_ns, data)?;
            (0u8, spoke, slot, generation)
        } else if relative_ns < self.l1_coverage_ns {
            let (spoke, slot, generation) = self.l1.schedule_with_data(deadline_ns, data)?;
            (1u8, spoke, slot, generation)
        } else if relative_ns < self.l2_coverage_ns {
            let (spoke, slot, generation) = self.l2.schedule_with_data(deadline_ns, data)?;
            (2u8, spoke, slot, generation)
        } else {
            return Err(TimerWheelError::DeadlineTooFar);
        };

        Ok(Self::encode_timer_id(level, spoke, slot, generation))
    }

    /// Schedule a timer using best-fit strategy for cascading support
    ///
    /// Given a target deadline, schedules into the largest wheel whose coverage
    /// expires at or before the target. This allows the caller (e.g., Timer) to
    /// handle cascading for arbitrarily long durations with near-perfect precision.
    ///
    /// Returns `(timer_id, wheel_deadline_ns)` where:
    /// - `timer_id`: The timer ID for cancellation
    /// - `wheel_deadline_ns`: When this timer will actually fire (may be < target_deadline_ns)
    ///
    /// The caller should reschedule when `wheel_deadline_ns` fires if `target_deadline_ns`
    /// hasn't been reached yet.
    #[inline]
    pub fn schedule_timer_best_fit(
        &mut self,
        target_deadline_ns: u64,
        data: T,
    ) -> Result<(u64, u64), TimerWheelError> {
        let now_ns = self.now_ns.load(std::sync::atomic::Ordering::Acquire);

        // Validate deadline
        if target_deadline_ns == 0 || target_deadline_ns < self.start_time_ns {
            return Err(TimerWheelError::InvalidDeadline);
        }

        // If deadline is in the past or very near, schedule for immediate expiry
        if target_deadline_ns <= now_ns {
            // Schedule at now_ns + 1 tick so it fires on next poll
            let immediate_deadline = now_ns + 1;
            let (spoke, slot, generation) = self.l0.schedule_with_data(immediate_deadline, data)?;
            let timer_id = Self::encode_timer_id(0, spoke, slot, generation);
            return Ok((timer_id, immediate_deadline));
        }

        let remaining_ns = target_deadline_ns - now_ns;

        // Find the largest wheel that fits within the remaining duration
        // We want to schedule at a deadline that is <= target_deadline_ns
        // but as close to target as possible using the coarsest wheel available

        if remaining_ns <= self.l0_coverage_ns {
            // Fits in L0 - schedule directly at target
            let (spoke, slot, generation) = self.l0.schedule_with_data(target_deadline_ns, data)?;
            let timer_id = Self::encode_timer_id(0, spoke, slot, generation);
            Ok((timer_id, target_deadline_ns))
        } else if remaining_ns <= self.l1_coverage_ns {
            // Fits in L1 - schedule directly at target
            let (spoke, slot, generation) = self.l1.schedule_with_data(target_deadline_ns, data)?;
            let timer_id = Self::encode_timer_id(1, spoke, slot, generation);
            Ok((timer_id, target_deadline_ns))
        } else if remaining_ns <= self.l2_coverage_ns {
            // Fits in L2 - schedule directly at target
            let (spoke, slot, generation) = self.l2.schedule_with_data(target_deadline_ns, data)?;
            let timer_id = Self::encode_timer_id(2, spoke, slot, generation);
            Ok((timer_id, target_deadline_ns))
        } else {
            // Exceeds L2 coverage - schedule at L2 coverage boundary
            // Caller will need to reschedule when this fires
            let wheel_deadline_ns = now_ns + self.l2_coverage_ns - self.l2.tick_resolution_ns();
            let (spoke, slot, generation) = self.l2.schedule_with_data(wheel_deadline_ns, data)?;
            let timer_id = Self::encode_timer_id(2, spoke, slot, generation);
            Ok((timer_id, wheel_deadline_ns))
        }
    }

    /// Get L0 coverage in nanoseconds
    #[inline]
    pub fn l0_coverage_ns(&self) -> u64 {
        self.l0_coverage_ns
    }

    /// Get L1 coverage in nanoseconds
    #[inline]
    pub fn l1_coverage_ns(&self) -> u64 {
        self.l1_coverage_ns
    }

    /// Get L2 coverage in nanoseconds
    #[inline]
    pub fn l2_coverage_ns(&self) -> u64 {
        self.l2_coverage_ns
    }

    /// Cancel a timer
    #[inline]
    pub fn cancel_timer(&mut self, timer_id: u64) -> Result<T, TimerWheelError> {
        let (level, spoke, slot, generation) = Self::decode_timer_id(timer_id)?;

        match level {
            0 => self.l0.cancel(spoke, slot, generation),
            1 => self.l1.cancel(spoke, slot, generation),
            2 => self.l2.cancel(spoke, slot, generation),
            _ => Err(TimerWheelError::InvalidTimerId),
        }
    }

    /// Poll for expired timers
    ///
    /// Only polls wheels whose tick has advanced since last poll.
    /// Returns number of expired timers.
    pub fn poll(
        &mut self,
        now_ns: u64,
        expiry_limit: usize,
        output: &mut Vec<(u64, u64, T)>, // (timer_id, deadline_ns, data)
    ) -> usize {
        self.now_ns
            .store(now_ns, std::sync::atomic::Ordering::Release);
        output.clear();

        let relative_ns = now_ns.saturating_sub(self.start_time_ns);
        let mut total_expired = 0;

        // Calculate current ticks for each level
        let l0_tick = relative_ns >> self.l0.tick_shift();
        let l1_tick = relative_ns >> self.l1.tick_shift();
        let l2_tick = relative_ns >> self.l2.tick_shift();

        // Temporary buffer for wheel output
        let mut wheel_output: Vec<(usize, usize, u32, u64, T)> = Vec::new();

        // Poll L0 if tick advanced
        if l0_tick > self.prev_l0_tick || self.l0.timer_count() > 0 {
            let remaining = expiry_limit.saturating_sub(total_expired);
            wheel_output.clear();
            let count = self.l0.poll(now_ns, self.prev_l0_tick, l0_tick, remaining, &mut wheel_output);

            for (spoke, slot, generation, deadline_ns, data) in wheel_output.drain(..) {
                let timer_id = Self::encode_timer_id(0, spoke, slot, generation);
                output.push((timer_id, deadline_ns, data));
            }
            total_expired += count;
        }
        self.prev_l0_tick = l0_tick;

        // Poll L1 only if L1 tick advanced
        if l1_tick > self.prev_l1_tick {
            let remaining = expiry_limit.saturating_sub(total_expired);
            if remaining > 0 {
                wheel_output.clear();
                let count = self.l1.poll(now_ns, self.prev_l1_tick, l1_tick, remaining, &mut wheel_output);

                for (spoke, slot, generation, deadline_ns, data) in wheel_output.drain(..) {
                    let timer_id = Self::encode_timer_id(1, spoke, slot, generation);
                    output.push((timer_id, deadline_ns, data));
                }
                total_expired += count;
            }
        }
        self.prev_l1_tick = l1_tick;

        // Poll L2 only if L2 tick advanced
        if l2_tick > self.prev_l2_tick {
            let remaining = expiry_limit.saturating_sub(total_expired);
            if remaining > 0 {
                wheel_output.clear();
                let count = self.l2.poll(now_ns, self.prev_l2_tick, l2_tick, remaining, &mut wheel_output);

                for (spoke, slot, generation, deadline_ns, data) in wheel_output.drain(..) {
                    let timer_id = Self::encode_timer_id(2, spoke, slot, generation);
                    output.push((timer_id, deadline_ns, data));
                }
                total_expired += count;
            }
        }
        self.prev_l2_tick = l2_tick;

        total_expired
    }

    /// Get total timer count across all wheels
    #[inline]
    pub fn timer_count(&self) -> u64 {
        self.l0.timer_count() + self.l1.timer_count() + self.l2.timer_count()
    }

    /// Get earliest deadline across all wheels
    pub fn next_deadline(&mut self) -> Option<u64> {
        let d0 = self.l0.next_deadline();
        let d1 = self.l1.next_deadline();
        let d2 = self.l2.next_deadline();

        [d0, d1, d2]
            .into_iter()
            .flatten()
            .min()
    }

    /// Get current time
    #[inline]
    pub fn now_ns(&self) -> u64 {
        self.now_ns.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Set current time
    #[inline]
    pub fn set_now_ns(&mut self, now_ns: u64) {
        self.now_ns
            .store(now_ns, std::sync::atomic::Ordering::Release);
    }

    /// Get worker ID
    #[inline]
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// Clear all timers
    pub fn clear(&mut self) {
        self.l0.clear();
        self.l1.clear();
        self.l2.clear();
        self.prev_l0_tick = 0;
        self.prev_l1_tick = 0;
        self.prev_l2_tick = 0;
    }

    /// Force compaction of all wheels
    pub fn compact_all(&mut self) {
        self.l0.compact_all();
        self.l1.compact_all();
        self.l2.compact_all();
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> TimerWheelMemoryStats {
        TimerWheelMemoryStats {
            l0: self.l0.memory_stats(),
            l1: self.l1.memory_stats(),
            l2: self.l2.memory_stats(),
        }
    }

    /// Encode timer ID: level (2 bits) | generation (22 bits) | spoke (12 bits) | slot (28 bits)
    ///
    /// Layout supports:
    /// - 3 levels (L0, L1, L2)
    /// - 4M generations before wrap (sufficient for long-running systems)
    /// - 4096 spokes per wheel
    /// - 256M slots per spoke
    #[inline]
    fn encode_timer_id(level: u8, spoke: usize, slot: usize, generation: u32) -> u64 {
        ((level as u64) << 62)
            | (((generation as u64) & 0x3FFFFF) << 40)
            | (((spoke as u64) & 0xFFF) << 28)
            | ((slot as u64) & 0xFFFFFFF)
    }

    /// Decode timer ID
    #[inline]
    fn decode_timer_id(timer_id: u64) -> Result<(u8, usize, usize, u32), TimerWheelError> {
        let level = ((timer_id >> 62) & 0x3) as u8;
        let generation = ((timer_id >> 40) & 0x3FFFFF) as u32;
        let spoke = ((timer_id >> 28) & 0xFFF) as usize;
        let slot = (timer_id & 0xFFFFFFF) as usize;

        if level > 2 {
            return Err(TimerWheelError::InvalidTimerId);
        }

        Ok((level, spoke, slot, generation))
    }
}

/// Memory statistics for the hierarchical timer wheel
#[derive(Debug, Clone)]
pub struct TimerWheelMemoryStats {
    pub l0: WheelMemoryStats,
    pub l1: WheelMemoryStats,
    pub l2: WheelMemoryStats,
}

impl TimerWheelMemoryStats {
    /// Get total capacity across all wheels
    pub fn total_capacity(&self) -> usize {
        self.l0.total_slot_capacity + self.l1.total_slot_capacity + self.l2.total_slot_capacity
    }

    /// Get total active timers
    pub fn total_active(&self) -> usize {
        self.l0.active_timers + self.l1.active_timers + self.l2.active_timers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spoke_insert_remove() {
        let mut spoke: Spoke<u32> = Spoke::new(16);

        let slot1 = spoke.insert(100, 42);
        let slot2 = spoke.insert(200, 43);

        assert_eq!(spoke.active_count, 2);

        let generation = spoke.generation();
        let data = spoke.remove(slot1, generation);
        assert_eq!(data, Some(42));
        assert_eq!(spoke.active_count, 1);

        // Wrong generation should fail
        let data = spoke.remove(slot2, generation + 1);
        assert_eq!(data, None);
        assert_eq!(spoke.active_count, 1);

        // Correct generation
        let data = spoke.remove(slot2, generation);
        assert_eq!(data, Some(43));
        assert_eq!(spoke.active_count, 0);
    }

    #[test]
    fn test_spoke_compaction() {
        let config = CompactionConfig {
            min_capacity_for_compaction: 8,
            compaction_ratio: 4,
            target_capacity_multiplier: 2,
        };

        let mut spoke: Spoke<u32> = Spoke::new(64);

        // Insert many timers
        for i in 0..32 {
            spoke.insert(i as u64 * 100, i);
        }
        assert_eq!(spoke.active_count, 32);

        // Remove most of them
        let generation = spoke.generation();
        for i in 0..30 {
            spoke.remove(i, generation);
        }
        assert_eq!(spoke.active_count, 2);

        // Should trigger compaction
        assert!(spoke.should_compact(&config));

        let remapping = spoke.compact(&config);
        assert!(!remapping.is_empty());
        assert_eq!(spoke.active_count, 2);
        assert!(spoke.capacity() < 64); // Should have shrunk
    }

    #[test]
    fn test_single_wheel_schedule_cancel() {
        let mut wheel: SingleWheel<u32> = SingleWheel::new(1 << 20, 1024);
        wheel.set_start_time_ns(0);

        let (spoke, slot, generation) = wheel.schedule_with_data(100_000_000, 42).unwrap();
        assert_eq!(wheel.timer_count(), 1);

        let data = wheel.cancel(spoke, slot, generation).unwrap();
        assert_eq!(data, 42);
        assert_eq!(wheel.timer_count(), 0);
    }

    #[test]
    fn test_single_wheel_poll() {
        let mut wheel: SingleWheel<&str> = SingleWheel::new(1 << 20, 1024);
        wheel.set_start_time_ns(0);

        wheel.schedule_with_data(10_000_000, "timer1").unwrap();
        wheel.schedule_with_data(20_000_000, "timer2").unwrap();

        let mut output = Vec::new();
        let count = wheel.poll(15_000_000, 0, 15, 100, &mut output);

        assert_eq!(count, 1);
        assert_eq!(output[0].4, "timer1");
    }

    #[test]
    fn test_timer_wheel_l0_scheduling() {
        let mut wheel: TimerWheel<u32> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);

        // Schedule in L0 range (~1s)
        let timer_id = wheel.schedule_timer(500_000_000, 42).unwrap();
        assert_eq!(wheel.l0.timer_count(), 1);
        assert_eq!(wheel.l1.timer_count(), 0);
        assert_eq!(wheel.l2.timer_count(), 0);

        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, 42);
    }

    #[test]
    fn test_timer_wheel_l1_scheduling() {
        let mut wheel: TimerWheel<u32> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);

        // Schedule in L1 range (~17min)
        let timer_id = wheel.schedule_timer(5_000_000_000, 42).unwrap(); // 5 seconds
        assert_eq!(wheel.l0.timer_count(), 0);
        assert_eq!(wheel.l1.timer_count(), 1);
        assert_eq!(wheel.l2.timer_count(), 0);

        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, 42);
    }

    #[test]
    fn test_timer_wheel_l2_scheduling() {
        let mut wheel: TimerWheel<u32> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);

        // Schedule in L2 range (~18hr)
        let deadline = 2_000_000_000_000u64; // 2000 seconds
        let timer_id = wheel.schedule_timer(deadline, 42).unwrap();
        assert_eq!(wheel.l0.timer_count(), 0);
        assert_eq!(wheel.l1.timer_count(), 0);
        assert_eq!(wheel.l2.timer_count(), 1);

        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, 42);
    }

    #[test]
    fn test_timer_wheel_poll() {
        let mut wheel: TimerWheel<&str> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);

        wheel.schedule_timer(10_000_000, "fast").unwrap();
        wheel.schedule_timer(5_000_000_000, "medium").unwrap();

        let mut output = Vec::new();

        // Poll at 20ms - should get "fast"
        wheel.poll(20_000_000, 100, &mut output);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].2, "fast");

        // Poll at 6s - should get "medium"
        output.clear();
        wheel.poll(6_000_000_000, 100, &mut output);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].2, "medium");
    }

    #[test]
    fn test_timer_wheel_deadline_too_far() {
        let mut wheel: TimerWheel<u32> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);

        // Try to schedule beyond L2 coverage
        let result = wheel.schedule_timer(u64::MAX / 2, 42);
        assert!(matches!(result, Err(TimerWheelError::DeadlineTooFar)));
    }

    #[test]
    fn test_timer_id_encoding() {
        // Test round-trip encoding/decoding
        // Layout: level (2 bits) | generation (22 bits) | spoke (12 bits) | slot (28 bits)
        let test_cases = [
            (0u8, 0usize, 0usize, 0u32),
            (1, 100, 200, 300),
            (2, 1023, 1000000, 65535),
            (2, 4095, 0xFFFFFFF, 0x3FFFFF), // Max values for spoke, slot, generation
        ];

        for (level, spoke, slot, generation) in test_cases {
            let id = TimerWheel::<()>::encode_timer_id(level, spoke, slot, generation);
            let (l, s, sl, g) = TimerWheel::<()>::decode_timer_id(id).unwrap();
            assert_eq!(level, l);
            assert_eq!(spoke & 0xFFF, s);      // 12 bits for spoke
            assert_eq!(slot & 0xFFFFFFF, sl);  // 28 bits for slot
            assert_eq!(generation & 0x3FFFFF, g); // 22 bits for generation
        }
    }

    #[test]
    fn test_stable_timer_ids() {
        let mut wheel: TimerWheel<u32> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);

        // Schedule a timer in L1
        let timer_id = wheel.schedule_timer(5_000_000_000, 42).unwrap();

        // Advance time and poll (but timer shouldn't expire yet)
        let mut output = Vec::new();
        wheel.poll(1_000_000_000, 100, &mut output);
        assert!(output.is_empty());

        // Timer ID should still be valid for cancellation
        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, 42);
    }

    #[test]
    fn test_memory_stats() {
        let mut wheel: TimerWheel<u32> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);

        let stats = wheel.memory_stats();
        assert_eq!(stats.total_active(), 0);

        wheel.schedule_timer(100_000_000, 1).unwrap();
        wheel.schedule_timer(5_000_000_000, 2).unwrap();

        let stats = wheel.memory_stats();
        assert_eq!(stats.total_active(), 2);
        assert_eq!(stats.l0.active_timers, 1);
        assert_eq!(stats.l1.active_timers, 1);
    }

    #[test]
    fn test_schedule_timer_best_fit() {
        let mut wheel: TimerWheel<u32> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);

        // Schedule timer that fits in L0 - should schedule at target
        let (timer_id, wheel_deadline) = wheel.schedule_timer_best_fit(500_000_000, 1).unwrap();
        assert_eq!(wheel_deadline, 500_000_000);
        assert_eq!(wheel.l0.timer_count(), 1);
        wheel.cancel_timer(timer_id).unwrap();

        // Schedule timer that fits in L1 - should schedule at target
        let (timer_id, wheel_deadline) = wheel.schedule_timer_best_fit(5_000_000_000, 2).unwrap();
        assert_eq!(wheel_deadline, 5_000_000_000);
        assert_eq!(wheel.l1.timer_count(), 1);
        wheel.cancel_timer(timer_id).unwrap();

        // Schedule timer that fits in L2 - should schedule at target
        let l2_deadline = 2_000_000_000_000u64; // 2000 seconds
        let (timer_id, wheel_deadline) = wheel.schedule_timer_best_fit(l2_deadline, 3).unwrap();
        assert_eq!(wheel_deadline, l2_deadline);
        assert_eq!(wheel.l2.timer_count(), 1);
        wheel.cancel_timer(timer_id).unwrap();
    }

    #[test]
    fn test_schedule_timer_best_fit_cascading() {
        let mut wheel: TimerWheel<u32> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);

        // Schedule timer that exceeds L2 coverage - should schedule at L2 boundary
        let far_deadline = wheel.l2_coverage_ns() * 2; // 2x L2 coverage
        let (timer_id, wheel_deadline) = wheel.schedule_timer_best_fit(far_deadline, 42).unwrap();

        // wheel_deadline should be less than far_deadline (at L2 boundary)
        assert!(wheel_deadline < far_deadline);
        // Should be close to L2 coverage minus one tick
        let expected_wheel_deadline = wheel.l2_coverage_ns() - wheel.l2.tick_resolution_ns();
        assert_eq!(wheel_deadline, expected_wheel_deadline);
        assert_eq!(wheel.l2.timer_count(), 1);

        // Poll and verify timer fires at wheel_deadline
        let mut output = Vec::new();
        wheel.poll(wheel_deadline + 1, 100, &mut output);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].2, 42);

        // Now a caller could reschedule for the remaining time to far_deadline
    }

    #[test]
    fn test_schedule_timer_best_fit_past_deadline() {
        let mut wheel: TimerWheel<u32> = TimerWheel::new(0);
        wheel.set_start_time_ns(0);
        wheel.set_now_ns(1_000_000_000); // 1 second

        // Schedule timer with deadline in the past
        let (timer_id, wheel_deadline) = wheel.schedule_timer_best_fit(500_000_000, 42).unwrap();

        // Should schedule for immediate expiry
        assert!(wheel_deadline > 500_000_000); // Scheduled after the past deadline
        assert_eq!(wheel.l0.timer_count(), 1);

        // Should fire immediately on poll
        let mut output = Vec::new();
        wheel.poll(wheel_deadline, 100, &mut output);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].2, 42);
    }
}
