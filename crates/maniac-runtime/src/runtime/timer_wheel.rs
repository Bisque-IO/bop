/// High-performance hierarchical hashed timing wheel for efficient timer scheduling
/// Based on "Hashed and Hierarchical Timing Wheels" by Varghese and Lauck
///
/// Two-level hierarchy:
/// - L0 (Fast): 1.05ms/tick × 1024 ticks = 1.07s coverage
/// - L1 (Medium): 1.07s/tick × 1024 ticks = 18.33min coverage
///
/// Complexity:
/// - Schedule: O(1) amortized, O(tick_allocation) worst-case when tick is full
/// - Cancel: O(1)
/// - Poll: O(expired_timers + ticks_per_wheel × tick_allocation) - scans from current_tick forward
/// - Next deadline: O(ticks_per_wheel × tick_allocation) when cache invalidated
///
/// Timers in same tick are not ordered relative to each other
/// NOT thread-safe - caller must synchronize
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// Error types for timer wheel operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimerWheelError {
    /// Timer ID exceeds maximum supported value
    InvalidTimerId,
    /// Timer deadline is invalid (negative, overflow, etc.)
    InvalidDeadline,
    /// Capacity limit exceeded
    CapacityExceeded,
    /// Integer overflow in calculations
    Overflow,
    /// Timer ID does not correspond to a valid timer
    TimerNotFound,
}

/// Timer entry stored in the wheel
/// 
/// Each entry contains the absolute deadline in nanoseconds, the computed deadline tick
/// (for efficient spoke calculation), and the associated data.
#[derive(Debug)]
struct TimerEntry<T> {
    /// Absolute deadline in nanoseconds since wheel start
    deadline_ns: u64,
    /// Precomputed deadline tick (deadline_ns / tick_resolution_ns)
    /// Used for efficient spoke index calculation
    deadline_tick: u64,
    /// User data associated with this timer
    data: T,
}

/// Internal single-level timing wheel
/// 
/// A hashed timing wheel uses a circular array where each slot (spoke) represents a time interval.
/// Timers are hashed into spokes based on their deadline, allowing O(1) insertion and cancellation.
/// Multiple timers can hash to the same spoke, so each spoke has multiple slots.
/// 
/// The wheel uses power-of-2 sizes for efficient bitwise operations:
/// - Spoke index = (deadline_tick & tick_mask) - uses bitwise AND instead of modulo
/// - Slot index within spoke uses bit shifts for fast indexing
/// 
/// Used as a building block for the hierarchical TimerWheel
struct SingleWheel<T> {
    /// Time resolution per tick in nanoseconds (must be power of 2)
    /// Determines the granularity of timer scheduling
    tick_resolution_ns: u64,

    /// Current tick position - tracks how far the wheel has advanced
    /// Used to optimize polling by only scanning expired ticks
    current_tick: u64,

    /// Number of active timers in this wheel
    /// Used for quick checks to avoid unnecessary work
    timer_count: u64,

    /// Cached next deadline for efficient querying
    /// Invalidated (set to NULL_DEADLINE) when timers are cancelled or cascaded
    cached_next_deadline: u64,

    /// Number of ticks/spokes per wheel (must be power of 2)
    /// Each spoke represents one tick interval
    ticks_per_wheel: usize,

    /// Mask for tick calculation (ticks_per_wheel - 1)
    /// Used for fast modulo: tick & tick_mask instead of tick % ticks_per_wheel
    tick_mask: usize,

    /// Bit shift for resolution calculations
    /// Precomputed: log2(tick_resolution_ns) for fast division: deadline_ns >> resolution_bits_to_shift
    resolution_bits_to_shift: u32,

    /// Current allocation size per tick (must be power of 2)
    /// Each spoke can hold this many timers before needing to expand
    tick_allocation: usize,

    /// Bit shift for allocation calculations
    /// Precomputed: log2(tick_allocation) for fast indexing: spoke << allocation_bits_to_shift
    allocation_bits_to_shift: u32,

    /// Current index within a tick during polling
    /// Used for incremental polling (currently not fully utilized)
    poll_index: usize,

    /// Flat array storage: [tick0_slot0, tick0_slot1, ..., tick1_slot0, ...]
    /// Index calculation: (spoke_index << allocation_bits_to_shift) + slot_index
    /// None indicates an empty slot
    wheel: Vec<Option<TimerEntry<T>>>,

    /// Per-tick next available slot hint for O(1) scheduling
    /// Tracks the last used slot index per spoke to enable fast insertion
    /// When scheduling, we first check this hint before doing a linear search
    next_free_hint: Vec<usize>,
}

impl<T> SingleWheel<T> {
    const INITIAL_TICK_ALLOCATION: usize = 16;
    const NULL_DEADLINE: u64 = u64::MAX;

    /// Create a new single-level timing wheel
    /// 
    /// # Arguments
    /// * `tick_resolution_ns` - Duration of one tick in nanoseconds (must be power of 2)
    /// * `ticks_per_wheel` - Number of spokes/ticks in the wheel (must be power of 2)
    /// * `initial_tick_allocation` - Initial number of slots per tick (must be power of 2)
    /// 
    /// # Panics
    /// Panics if any parameter is not a power of 2, as this breaks bitwise optimizations
    fn new(
        tick_resolution_ns: u64,
        ticks_per_wheel: usize,
        initial_tick_allocation: usize,
    ) -> Self {
        // Validate power-of-2 constraints for efficient bitwise operations
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

        // Precompute masks and bit shifts for fast operations
        let tick_mask = ticks_per_wheel - 1; // For fast modulo: tick & tick_mask
        let resolution_bits_to_shift = tick_resolution_ns.trailing_zeros(); // log2(tick_resolution_ns)
        let allocation_bits_to_shift = initial_tick_allocation.trailing_zeros(); // log2(tick_allocation)

        // Allocate flat array: total capacity = ticks_per_wheel * slots_per_tick
        let capacity = ticks_per_wheel * initial_tick_allocation;
        let mut wheel = Vec::with_capacity(capacity);
        wheel.resize_with(capacity, || None);

        // Initialize per-spoke hints for O(1) scheduling
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

    /// Schedule a timer into this wheel
    /// 
    /// Uses hashing to determine which spoke (tick) the timer belongs to, then finds
    /// an available slot within that spoke. Employs a hint-based optimization for
    /// O(1) average-case insertion.
    /// 
    /// # Returns
    /// Returns `(spoke_index, slot_index)` tuple that can be encoded into a timer ID
    fn schedule_internal(
        &mut self,
        deadline_ns: u64,
        start_time_ns: u64,
        data: T,
        level: u8,
    ) -> Result<(usize, usize), TimerWheelError> {
        // Validate deadline is in the future
        if deadline_ns < start_time_ns {
            return Err(TimerWheelError::InvalidDeadline);
        }

        // Compute which tick this deadline falls into
        // deadline_tick = (deadline_ns - start_time_ns) / tick_resolution_ns
        // Using bit shift instead of division for performance
        let deadline_tick =
            (deadline_ns.saturating_sub(start_time_ns)) >> self.resolution_bits_to_shift;
        
        // Hash into spoke using bitwise AND (faster than modulo)
        // spoke_index = deadline_tick % ticks_per_wheel
        let spoke_index = (deadline_tick & self.tick_mask as u64) as usize;
        
        // Calculate start index of this spoke in the flat array
        // tick_start_index = spoke_index * tick_allocation
        let mut tick_start_index = spoke_index << self.allocation_bits_to_shift;

        // Fast path: try hint first for O(1) insertion when spoke has free slots
        let hint = self.next_free_hint[spoke_index];
        let mut slot_idx = None;

        if hint < self.tick_allocation {
            let hint_index = tick_start_index + hint;
            if self.wheel[hint_index].is_none() {
                slot_idx = Some(hint);
            }
        }

        // Slow path: linear search if hint didn't work
        // Searches circularly starting from hint position
        if slot_idx.is_none() {
            for i in 0..self.tick_allocation {
                let candidate = (hint + i) % self.tick_allocation;
                let index = tick_start_index + candidate;

                if self.wheel[index].is_none() {
                    slot_idx = Some(candidate);
                    break;
                }
            }
        }

        // If spoke is full, expand capacity
        let slot_idx = match slot_idx {
            Some(idx) => idx,
            None => {
                // Double the allocation for this spoke (and all spokes)
                let new_slot = self.increase_capacity(spoke_index)
                    .map_err(|_| TimerWheelError::CapacityExceeded)?;
                // Recalculate start index after capacity increase
                tick_start_index = spoke_index << self.allocation_bits_to_shift;
                new_slot
            }
        };

        // Store timer entry
        let wheel_index = tick_start_index + slot_idx;
        self.wheel[wheel_index] = Some(TimerEntry {
            deadline_ns,
            deadline_tick,
            data,
        });
        self.timer_count += 1;

        // Update cached earliest deadline if this timer is earlier
        // This enables O(1) next_deadline() queries when cache is valid
        if deadline_ns < self.cached_next_deadline {
            self.cached_next_deadline = deadline_ns;
        }

        // Update hint to next slot for future insertions
        // Wraps around to 0 if we've reached the end of allocation
        let next_hint = if slot_idx + 1 >= self.tick_allocation {
            0
        } else {
            slot_idx + 1
        };
        self.next_free_hint[spoke_index] = next_hint;

        Ok((spoke_index, slot_idx))
    }

    /// Cancel a timer by spoke and slot index
    /// 
    /// O(1) operation - directly indexes into the wheel array using the provided indices.
    /// Invalidates the cached deadline if the cancelled timer was the earliest one.
    /// 
    /// # Returns
    /// Returns the timer data if found, or an error if the timer doesn't exist
    fn cancel_internal(&mut self, spoke_index: usize, slot_index: usize) -> Result<Option<T>, TimerWheelError> {
        // Validate bounds BEFORE computing wheel_index to prevent integer overflow
        // This is a safety check - overflow could cause incorrect indexing
        if spoke_index >= self.ticks_per_wheel || slot_index >= self.tick_allocation {
            return Err(TimerWheelError::InvalidTimerId);
        }

        // Calculate flat array index: (spoke_index * tick_allocation) + slot_index
        let wheel_index = (spoke_index << self.allocation_bits_to_shift) + slot_index;

        // Remove timer entry if present
        if let Some(entry) = self.wheel[wheel_index].take() {
            self.timer_count -= 1;
            let TimerEntry {
                deadline_ns, data, ..
            } = entry;
            
            // If we cancelled the earliest timer, invalidate the cache
            // The cache will be recomputed lazily on the next next_deadline() call
            if deadline_ns == self.cached_next_deadline {
                self.cached_next_deadline = Self::NULL_DEADLINE;
            }
            
            return Ok(Some(data));
        }
        
        // Timer not found at this location
        Err(TimerWheelError::TimerNotFound)
    }

    /// Double the capacity of all spokes in the wheel
    /// 
    /// When a spoke runs out of slots, we double the allocation for ALL spokes
    /// (not just the full one) to maintain uniform indexing. This is expensive
    /// but amortized over many insertions.
    /// 
    /// # Returns
    /// Returns the old tick allocation size (which becomes the first new slot index)
    fn increase_capacity(&mut self, spoke_index: usize) -> Result<usize, TimerWheelError> {
        // Double the allocation (must remain power of 2)
        let new_tick_allocation = self.tick_allocation.checked_mul(2)
            .ok_or(TimerWheelError::Overflow)?;
        let new_allocation_bits = new_tick_allocation.trailing_zeros();

        // Check total capacity limit (1GB max)
        let new_capacity = self.ticks_per_wheel * new_tick_allocation;
        if new_capacity > (1 << 30) {
            return Err(TimerWheelError::CapacityExceeded);
        }

        // Save old values for copying
        let old_tick_allocation = self.tick_allocation;
        let old_allocation_bits = self.allocation_bits_to_shift;

        // Allocate new larger array
        let mut new_wheel = Vec::with_capacity(new_capacity);
        new_wheel.resize_with(new_capacity, || None);

        // Copy all existing timers to new array
        // Each spoke's data is copied to the same relative position in the new spoke
        for j in 0..self.ticks_per_wheel {
            let old_start = j << old_allocation_bits;  // Old spoke start
            let new_start = j << new_allocation_bits;  // New spoke start
            // Copy all slots from old spoke to new spoke
            for k in 0..old_tick_allocation {
                new_wheel[new_start + k] = self.wheel[old_start + k].take();
            }
        }

        // Update wheel state
        self.tick_allocation = new_tick_allocation;
        self.allocation_bits_to_shift = new_allocation_bits;
        self.wheel = new_wheel;

        // Reset hints to point to the first new slot (old allocation size)
        // This ensures future insertions use the newly allocated space first
        for hint in &mut self.next_free_hint {
            *hint = old_tick_allocation;
        }

        // Return old allocation as the first available new slot
        Ok(old_tick_allocation)
    }
}

/// High-performance hierarchical timing wheel
/// 
/// Uses a two-level hierarchy to efficiently handle timers across a wide time range:
/// - **L0 (Fast wheel)**: Handles near-term timers (0 to ~1 second)
///   - Fine-grained resolution for accurate short-term scheduling
///   - Polled frequently for expired timers
/// - **L1 (Medium wheel)**: Handles long-term timers (~1 second to ~18 minutes)
///   - Coarser resolution to reduce memory overhead
///   - Timers cascade down to L0 as their deadline approaches
/// 
/// The hierarchy allows O(1) scheduling and cancellation while supporting
/// a wide range of timer durations without excessive memory usage.
pub struct TimerWheel<T> {
    /// L0 (Fast) wheel: handles timers in the near future
    /// Typical: 1.05ms ticks × 1024 ticks = 1.07s coverage
    l0: SingleWheel<T>,

    /// L1 (Medium) wheel: handles timers further in the future
    /// Typical: 1.07s ticks × 1024 ticks = 18.33min coverage
    l1: SingleWheel<T>,

    /// Null deadline sentinel value (u64::MAX)
    /// Used to represent "no deadline" in cached values
    null_deadline: u64,

    /// Start time in nanoseconds - when the wheel was created
    /// All deadlines are relative to this time
    start_time_ns: u64,

    /// Current time in nanoseconds (atomic for cross-thread updates)
    /// Updated by external tick thread, read by worker threads
    now_ns: AtomicU64,

    /// Worker ID that owns this timer wheel
    /// Used for cross-worker timer operations
    worker_id: u32,

    /// L0 coverage in nanoseconds (precalculated for efficiency)
    /// Threshold for deciding whether to schedule in L0 or L1
    l0_coverage_ns: u64,
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
    /// 
    /// # Arguments
    /// * `tick_resolution` - L0 tick duration (must be power of 2 nanoseconds)
    /// * `ticks_per_wheel` - Number of spokes per wheel (must be power of 2)
    /// * `initial_tick_allocation` - Initial slots per tick (must be power of 2)
    /// * `worker_id` - Worker ID that owns this timer wheel
    /// 
    /// # Design Notes
    /// The L1 tick resolution equals one full L0 rotation, creating a natural
    /// cascading relationship: when an L1 tick expires, all its timers are
    /// within L0's coverage range and can be cascaded down.
    pub fn with_allocation(
        tick_resolution: Duration,
        ticks_per_wheel: usize,
        initial_tick_allocation: usize,
        worker_id: u32,
    ) -> Self {
        let l0_tick_ns = tick_resolution.as_nanos() as u64;
        // L0 coverage = one full rotation of L0 wheel
        let l0_coverage_ns = l0_tick_ns * ticks_per_wheel as u64;
        // L1 tick equals one full L0 rotation - this enables efficient cascading
        let l1_tick_ns = l0_coverage_ns;

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
        }
    }

    /// Schedule a timer for an absolute deadline
    /// 
    /// The deadline is specified as nanoseconds since wheel creation. The timer
    /// is automatically placed in the appropriate wheel (L0 for near-term, L1 for
    /// long-term) based on the deadline value.
    /// 
    /// # Arguments
    /// * `deadline_ns` - Absolute deadline in nanoseconds (must be > 0 and in the future)
    /// * `data` - User data to associate with this timer
    /// 
    /// # Returns
    /// Returns a timer ID that can be used to cancel the timer, or an error if
    /// the deadline is invalid or capacity is exceeded.
    pub fn schedule_timer(&mut self, deadline_ns: u64, data: T) -> Result<u64, TimerWheelError> {
        // Validate deadline: must be > 0, >= start_time_ns, and >= now_ns
        // This prevents scheduling timers in the past
        let now_ns = self.now_ns.load(Ordering::Relaxed);
        if deadline_ns == 0 || deadline_ns < self.start_time_ns || deadline_ns < now_ns {
            return Err(TimerWheelError::InvalidDeadline);
        }

        // Route to appropriate wheel based on deadline distance
        if deadline_ns < self.l0_coverage_ns {
            // Near-term timer: schedule in L0 (fast wheel)
            let (spoke, slot) = self
                .l0
                .schedule_internal(deadline_ns, self.start_time_ns, data, 0)?;
            Ok(Self::encode_timer_id(0, spoke, slot)?)
        } else {
            // Long-term timer: schedule in L1 (medium wheel)
            // Will cascade down to L0 as deadline approaches
            let (spoke, slot) = self
                .l1
                .schedule_internal(deadline_ns, self.start_time_ns, data, 1)?;
            Ok(Self::encode_timer_id(1, spoke, slot)?)
        }
    }

    /// Cancel a previously scheduled timer
    /// 
    /// O(1) operation - directly indexes into the wheel using the timer ID.
    /// 
    /// # Arguments
    /// * `timer_id` - Timer ID returned from `schedule_timer()`
    /// 
    /// # Returns
    /// Returns the timer data if found and cancelled, or an error if the timer
    /// doesn't exist or the timer ID is invalid.
    pub fn cancel_timer(&mut self, timer_id: u64) -> Result<Option<T>, TimerWheelError> {
        // Decode timer ID to extract wheel level, spoke, and slot indices
        let (level, spoke, slot) = Self::decode_timer_id(timer_id)?;
        match level {
            0 => self.l0.cancel_internal(spoke, slot),
            1 => self.l1.cancel_internal(spoke, slot),
            _ => Err(TimerWheelError::InvalidTimerId),
        }
    }

    /// Poll for expired timers
    /// 
    /// Checks for timers that have expired by the given `now_ns` time. First cascades
    /// L1 timers that are now within L0 coverage down to L0, then polls L0 for
    /// expired timers.
    /// 
    /// # Arguments
    /// * `now_ns` - Current time in nanoseconds
    /// * `expiry_limit` - Maximum number of expired timers to return
    /// * `output` - Buffer to write expired timer tuples: `(timer_id, deadline_ns, data)`
    /// 
    /// # Returns
    /// Returns the number of expired timers found (may be less than `expiry_limit`)
    pub fn poll(
        &mut self,
        now_ns: u64,
        expiry_limit: usize,
        output: &mut Vec<(u64, u64, T)>,
    ) -> usize {
        // Update internal clock
        self.now_ns.store(now_ns, Ordering::Release);
        output.clear();

        // Step 1: Cascade L1 -> L0 for timers now within L0 coverage
        // This moves long-term timers to the fast wheel as their deadline approaches
        self.cascade_l1_to_l0(now_ns);

        // Step 2: Poll L0 for expired timers
        // All expired timers should now be in L0 (either originally or cascaded)
        self.poll_l0(now_ns, expiry_limit, output)
    }

    /// Cascade timers from L1 to L0
    /// 
    /// Moves L1 timers that are now within L0's coverage range down to L0.
    /// This ensures long-term timers are handled by the fast wheel as their
    /// deadline approaches, enabling accurate expiration.
    /// 
    /// Cascading happens when: deadline_ns < now_ns + l0_coverage_ns
    /// This means the timer will expire within one L0 rotation, so it should
    /// be in L0 for precise timing.
    fn cascade_l1_to_l0(&mut self, now_ns: u64) {
        // Update L1's current tick position
        let l1_target_tick =
            (now_ns.saturating_sub(self.start_time_ns)) >> self.l1.resolution_bits_to_shift;
        self.l1.current_tick = l1_target_tick;

        // Early exit if no timers to cascade
        if self.l1.timer_count == 0 {
            return;
        }

        // Calculate threshold: cascade timers with deadline within L0 coverage
        // Threshold = now_ns + l0_coverage_ns
        // Any timer with deadline < threshold should be in L0
        let cascade_deadline_threshold = now_ns.saturating_add(self.l0_coverage_ns);

        // Scan all L1 spokes for timers to cascade
        // Note: This is O(ticks_per_wheel × tick_allocation) but only runs
        // when there are L1 timers, and cascading is relatively infrequent
        for spoke_index in 0..self.l1.ticks_per_wheel {
            let tick_start = spoke_index << self.l1.allocation_bits_to_shift;

            // Check each slot in this spoke
            for slot_idx in 0..self.l1.tick_allocation {
                let wheel_index = tick_start + slot_idx;
                
                // Check if this timer should be cascaded to L0
                // Criteria: deadline is within L0 coverage from now
                let should_cascade = matches!(
                    self.l1.wheel[wheel_index].as_ref(),
                    Some(entry) if entry.deadline_ns < cascade_deadline_threshold
                );

                if should_cascade {
                    // Remove from L1 and reschedule in L0
                    if let Some(entry) = self.l1.wheel[wheel_index].take() {
                        self.l1.timer_count -= 1;
                        let TimerEntry {
                            deadline_ns, data, ..
                        } = entry;

                        // Invalidate L1 cache if this was the earliest timer
                        if deadline_ns == self.l1.cached_next_deadline {
                            self.l1.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
                        }

                        // Reschedule into L0 (may place in past tick if already expired)
                        let _ = self
                            .l0
                            .schedule_internal(deadline_ns, self.start_time_ns, data, 0);
                    }
                }
            }

            // Reset L1 spoke hint after scanning (timers may have been removed)
            self.l1.next_free_hint[spoke_index] = 0;
        }

        // Invalidate deadline caches after cascading
        // L0 cache must be invalidated because cascaded timers may have deadlines
        // in past ticks (already processed), requiring a full scan to recompute
        if self.l1.cached_next_deadline != SingleWheel::<T>::NULL_DEADLINE {
            self.l1.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
        }
        if self.l0.cached_next_deadline != SingleWheel::<T>::NULL_DEADLINE {
            self.l0.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
        }
    }

    /// Poll L0 wheel for expired timers
    /// 
    /// Scans L0 for timers that have expired by `now_ns`. Uses an optimized
    /// incremental scan starting from `current_tick` to avoid scanning the entire
    /// wheel when only a few ticks have expired. Falls back to a full scan if
    /// cascaded timers may exist in earlier ticks.
    /// 
    /// # Performance
    /// - Best case: O(expired_timers) - only scans expired ticks
    /// - Worst case: O(ticks_per_wheel × tick_allocation) - full wheel scan
    fn poll_l0(
        &mut self,
        now_ns: u64,
        expiry_limit: usize,
        output: &mut Vec<(u64, u64, T)>,
    ) -> usize {
        // Calculate which tick we should be at given current time
        let target_tick =
            (now_ns.saturating_sub(self.start_time_ns)) >> self.l0.resolution_bits_to_shift;

        // Early exit if no timers
        if self.l0.timer_count == 0 {
            self.l0.current_tick = target_tick;
            return 0;
        }

        // Optimized incremental scan: only scan ticks from current_tick to target_tick
        // This avoids scanning the entire wheel when only a few ticks have expired
        let start_tick = self.l0.current_tick;
        let ticks_to_scan = if target_tick >= start_tick {
            // Normal case: scan forward from current_tick to target_tick
            // Cap at ticks_per_wheel to handle wrap-around
            (target_tick - start_tick + 1).min(self.l0.ticks_per_wheel as u64) as usize
        } else {
            // Wrapped around: need to scan entire wheel
            // (This shouldn't happen in practice, but handle it safely)
            self.l0.ticks_per_wheel
        };

        // Scan ticks incrementally from current_tick forward
        for tick_offset in 0..ticks_to_scan {
            if output.len() >= expiry_limit {
                break;
            }

            // Calculate spoke index with wrap-around using bitwise AND
            let spoke = ((start_tick as usize + tick_offset) & self.l0.tick_mask) as usize;
            let tick_start = spoke << self.l0.allocation_bits_to_shift;

            // Check all slots in this spoke
            for slot_idx in 0..self.l0.tick_allocation {
                if output.len() >= expiry_limit {
                    break;
                }

                let wheel_index = tick_start + slot_idx;

                if let Some(entry) = self.l0.wheel[wheel_index].as_ref() {
                    // Use deadline-based check (not tick-based) for accuracy
                    // A timer may expire even if its tick hasn't been reached yet
                    if now_ns >= entry.deadline_ns {
                        // Remove expired timer
                        let entry = self.l0.wheel[wheel_index].take().unwrap();
                        let TimerEntry {
                            deadline_ns, data, ..
                        } = entry;
                        self.l0.timer_count -= 1;

                        // Encode timer ID and add to output
                        let timer_id = Self::encode_timer_id(0, spoke, slot_idx).unwrap_or(0);
                        output.push((timer_id, deadline_ns, data));

                        // Invalidate cache if this was the earliest timer
                        if deadline_ns == self.l0.cached_next_deadline {
                            self.l0.cached_next_deadline = self.null_deadline;
                        }
                    }
                }
            }
        }

        // Fallback: full wheel scan if we didn't find enough expired timers
        // This handles the case where cascaded timers were placed in ticks
        // before current_tick (e.g., if current_tick advanced while L0 was empty)
        if output.len() < expiry_limit && self.l0.timer_count > 0 {
            // Full scan: check all ticks for expired timers
            for spoke in 0..self.l0.ticks_per_wheel {
                if output.len() >= expiry_limit {
                    break;
                }

                let tick_start = spoke << self.l0.allocation_bits_to_shift;

                for slot_idx in 0..self.l0.tick_allocation {
                    if output.len() >= expiry_limit {
                        break;
                    }

                    let wheel_index = tick_start + slot_idx;

                    if let Some(entry) = self.l0.wheel[wheel_index].as_ref() {
                        if now_ns >= entry.deadline_ns {
                            let entry = self.l0.wheel[wheel_index].take().unwrap();
                            let TimerEntry {
                                deadline_ns, data, ..
                            } = entry;
                            self.l0.timer_count -= 1;

                            let timer_id = Self::encode_timer_id(0, spoke, slot_idx).unwrap_or(0);
                            output.push((timer_id, deadline_ns, data));

                            if deadline_ns == self.l0.cached_next_deadline {
                                self.l0.cached_next_deadline = self.null_deadline;
                            }
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

    /// Advance the wheel's current tick position without polling
    /// 
    /// Useful for synchronizing the wheel's position when time advances
    /// without immediately processing expired timers.
    pub fn advance_to(&mut self, now_ns: u64) {
        let new_tick =
            (now_ns.saturating_sub(self.start_time_ns)) >> self.l0.resolution_bits_to_shift;
        // Only advance, never go backwards
        self.l0.current_tick = self.l0.current_tick.max(new_tick);
    }

    /// Get the time corresponding to the next tick
    /// 
    /// Returns the absolute time (in nanoseconds) when the next tick will occur.
    pub fn current_tick_time_ns(&self) -> u64 {
        ((self.l0.current_tick + 1) << self.l0.resolution_bits_to_shift) + self.start_time_ns
    }

    /// Get the total number of active timers across both wheels
    pub fn timer_count(&self) -> u64 {
        self.l0.timer_count + self.l1.timer_count
    }

    /// Get the earliest deadline among all active timers
    /// 
    /// Uses cached values when available for O(1) performance. Falls back to
    /// full wheel scans when cache is invalidated (O(ticks_per_wheel × tick_allocation)).
    /// 
    /// # Returns
    /// Returns the earliest deadline in nanoseconds, or `None` if no timers are scheduled.
    pub fn next_deadline(&mut self) -> Option<u64> {
        let total_count = self.l0.timer_count + self.l1.timer_count;
        if total_count == 0 {
            return None;
        }

        // Get earliest deadline from L0 (with lazy cache recomputation)
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

        // Get earliest deadline from L1 (with lazy cache recomputation)
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

        // Return the minimum of L0 and L1 earliest deadlines
        match (l0_next, l1_next) {
            (Some(d0), Some(d1)) => Some(d0.min(d1)),
            (Some(d0), None) => Some(d0),
            (None, Some(d1)) => Some(d1),
            (None, None) => None,
        }
    }

    /// Recompute the cached earliest deadline for L0
    /// 
    /// Scans the entire L0 wheel to find the earliest deadline. This is expensive
    /// (O(ticks_per_wheel × tick_allocation)) but only runs when the cache is invalidated.
    /// 
    /// Must scan the entire wheel because cascaded timers may exist in any tick,
    /// not just those after current_tick.
    fn recompute_l0_deadline(&mut self) {
        if self.l0.timer_count == 0 {
            self.l0.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
            return;
        }

        let mut earliest = SingleWheel::<T>::NULL_DEADLINE;

        // Scan entire wheel to find earliest deadline
        // Must scan all ticks because cascaded timers can be placed anywhere
        for spoke in 0..self.l0.ticks_per_wheel {
            for slot_idx in 0..self.l0.tick_allocation {
                let wheel_index = (spoke << self.l0.allocation_bits_to_shift) + slot_idx;
                if let Some(entry) = self.l0.wheel[wheel_index].as_ref() {
                    if entry.deadline_ns < earliest {
                        earliest = entry.deadline_ns;
                    }
                }
            }
        }

        self.l0.cached_next_deadline = earliest;
    }

    /// Recompute the cached earliest deadline for L1
    /// 
    /// Scans the entire L1 wheel to find the earliest deadline. This is expensive
    /// (O(ticks_per_wheel × tick_allocation)) but only runs when the cache is invalidated.
    fn recompute_l1_deadline(&mut self) {
        if self.l1.timer_count == 0 {
            self.l1.cached_next_deadline = SingleWheel::<T>::NULL_DEADLINE;
            return;
        }

        let mut earliest = SingleWheel::<T>::NULL_DEADLINE;

        // Scan entire wheel to find earliest deadline
        for spoke in 0..self.l1.ticks_per_wheel {
            for slot_idx in 0..self.l1.tick_allocation {
                let wheel_index = (spoke << self.l1.allocation_bits_to_shift) + slot_idx;
                if let Some(entry) = self.l1.wheel[wheel_index].as_ref() {
                    if entry.deadline_ns < earliest {
                        earliest = entry.deadline_ns;
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

    /// Encode wheel level, spoke, and slot into a compact timer ID
    /// 
    /// Timer ID format (64 bits):
    /// - Bits 62-63: Level (0=L0, 1=L1)
    /// - Bits 32-61: Spoke index (30 bits, max 1<<30 spokes)
    /// - Bits 0-31:  Slot index (32 bits, max 1<<32 slots)
    /// 
    /// This encoding allows O(1) timer cancellation by directly decoding
    /// the wheel location from the timer ID.
    #[inline]
    fn encode_timer_id(level: u8, spoke: usize, slot: usize) -> Result<u64, TimerWheelError> {
        // Validate bounds to prevent truncation during encoding
        if spoke >= (1 << 30) {
            return Err(TimerWheelError::InvalidTimerId);
        }
        if slot >= (1 << 32) {
            return Err(TimerWheelError::InvalidTimerId);
        }
        if level > 1 {
            return Err(TimerWheelError::InvalidTimerId);
        }

        // Pack into 64-bit integer: level (2 bits) | spoke (30 bits) | slot (32 bits)
        Ok(((level as u64) << 62) | ((spoke as u64) << 32) | (slot as u64))
    }

    /// Decode a timer ID into wheel level, spoke, and slot indices
    /// 
    /// Reverse of `encode_timer_id()`. Extracts the three components
    /// needed to locate a timer in the wheel structure.
    #[inline]
    fn decode_timer_id(timer_id: u64) -> Result<(u8, usize, usize), TimerWheelError> {
        // Extract level from top 2 bits
        let level = ((timer_id >> 62) & 0x3) as u8;
        // Extract spoke from middle 30 bits
        let spoke = ((timer_id >> 32) & 0x3FFFFFFF) as usize;
        // Extract slot from bottom 32 bits
        let slot = (timer_id & 0xFFFFFFFF) as usize;

        // Validate decoded level
        if level > 1 {
            return Err(TimerWheelError::InvalidTimerId);
        }

        Ok((level, spoke, slot))
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

        let data = wheel.cancel_timer(timer_id).unwrap().unwrap();
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

    #[test]
    fn test_error_handling() {
        let mut wheel = TimerWheel::<u32>::new(Duration::from_nanos(1024 * 1024), 1024, 0);

        // Test invalid deadline (zero)
        assert!(matches!(
            wheel.schedule_timer(0, 42),
            Err(TimerWheelError::InvalidDeadline)
        ));

        // Test invalid timer ID
        assert!(matches!(
            wheel.cancel_timer(0xFFFFFFFFFFFFFFFF),
            Err(TimerWheelError::InvalidTimerId)
        ));
    }

    #[test]
    fn test_timer_validation() {
        let mut wheel = TimerWheel::<u32>::new(Duration::from_nanos(1024 * 1024), 1024, 0);

        let timer_id = wheel.schedule_timer(100_000_000, 42).unwrap();

        // Cancel the timer
        let data = wheel.cancel_timer(timer_id).unwrap().unwrap();
        assert_eq!(data, 42);

        // Try to cancel again - should fail
        assert!(matches!(
            wheel.cancel_timer(timer_id),
            Err(TimerWheelError::TimerNotFound)
        ));
    }
}
