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

use crate::{seg_spmc::SegSpmc, seg_spsc::SegSpsc, waker::SignalWaker};

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

/// High-performance SPSC (Single Producer Single Consumer) timer wheel
///
/// This timer wheel is designed for concurrent use by exactly two threads:
/// - **Scheduler thread**: Calls `schedule_timer()` and `cancel_timer()`
/// - **Poller thread**: Calls `poll()` to retrieve expired timers
///
/// # Thread Safety Guarantees
///
/// The implementation achieves lock-free thread safety through:
///
/// 1. **Spatial Separation**: Timers are always scheduled to `current_tick + 1` or later,
///    ensuring the scheduler never writes to slots the poller is currently reading.
///    The poller only accesses the current tick spoke, while the scheduler only writes
///    to future tick spokes.
///
/// 2. **Memory Ordering**: Uses `Acquire`/`Release` memory fences to establish happens-before
///    relationships between threads:
///    - Scheduler: `Release` fence after writing timer data ensures visibility to poller
///    - Poller: `Acquire` fence before reading ensures it sees scheduler's writes
///
/// 3. **Atomic Coordination**: Only cross-thread state (`current_tick`, `timer_count`,
///    `notify_poller`) uses atomics. The wheel data itself requires no atomic operations
///    due to spatial separation.
///
/// 4. **Capacity Growth Safety**: When the scheduler increases capacity, it creates a new
///    wheel and uses `Release` fence to ensure the new structure is visible before
///    notification. The poller never accesses future spokes being modified.
///
/// # Performance Characteristics
///
/// - O(1) timer scheduling (amortized)
/// - O(1) timer cancellation
/// - O(n) polling where n = timers per tick
/// - Zero locks, only memory fences (nearly free on x86, cheap on ARM)
/// - No contention in hot paths due to spatial separation
///
/// # Example
///
/// ```rust
/// use std::time::{Duration, Instant};
/// use std::thread;
/// use std::sync::Arc;
/// use bop_executor::SpscTimerWheel;
///
/// let wheel = Arc::new(SpscTimerWheel::new(
///     Instant::now(),
///     Duration::from_millis(1),
///     256,
/// ));
///
/// let wheel_clone = wheel.clone();
/// let scheduler = thread::spawn(move || {
///     // Schedule timers from scheduler thread
///     // Safe because SpscTimerWheel implements Sync via UnsafeCell
///     wheel_clone.schedule_timer(1000, "task1");
///     wheel_clone.schedule_timer(2000, "task2");
/// });
///
/// let wheel_clone = wheel.clone();
/// let poller = thread::spawn(move || {
///     // Poll for expired timers from poller thread
///     // Safe because SpscTimerWheel implements Sync via UnsafeCell
///     loop {
///         let expired = wheel_clone.poll(Instant::now().elapsed().as_nanos() as u64, 10);
///         if !expired.is_empty() {
///             break;
///         }
///     }
/// });
///
/// scheduler.join().unwrap();
/// poller.join().unwrap();
/// ```
///
/// # Safety Note
///
/// This structure IS thread-safe for SPSC (Single Producer Single Consumer) usage.
/// It uses `UnsafeCell` for interior mutability combined with spatial separation
/// and memory ordering to ensure safe concurrent access without data races.
///
/// Safe to use from exactly two threads:
/// - Scheduler thread: calls schedule_timer(), cancel_timer()
/// - Poller thread: calls poll()
pub struct SpscTimerWheel<T> {
    /// Time resolution per tick (must be power of 2) - immutable
    tick_resolution_ns: u64,

    /// Start time of the wheel in nanoseconds since UNIX_EPOCH - immutable
    start_time_ns: u64,

    /// Number of ticks/spokes per wheel (must be power of 2) - immutable
    ticks_per_wheel: usize,

    /// Mask for tick calculation (ticks_per_wheel - 1) - immutable
    tick_mask: usize,

    /// Bit shift for resolution calculations - immutable
    resolution_bits_to_shift: u32,

    /// Current allocation size per tick (power of 2) - atomic coordination
    tick_allocation: AtomicUsize,

    /// Bit shift for allocation calculations - atomic coordination
    allocation_bits_to_shift: AtomicU32,

    /// Current tick position - owned by poller thread, read by scheduler
    current_tick: AtomicU64,

    /// Number of active timers - updated by both threads
    timer_count: AtomicU64,

    /// Core wheel data structure - protected by spatial separation
    /// SAFETY: Scheduler writes to future ticks (current_tick+1 or later)
    /// Poller only reads current tick. No overlap => no data race.
    wheel: UnsafeCell<Vec<Option<(u64, T)>>>,

    /// Per-tick next allocation hint - exclusively accessed by scheduler thread
    /// SAFETY: Only scheduler thread accesses this, so UnsafeCell is safe
    next_free_hint: UnsafeCell<Vec<usize>>,

    /// Poll position within current tick - owned by poller thread
    poll_position: AtomicUsize,

    /// Notification channel for poller when new timers are added
    notify_poller: AtomicBool,

    /// Cache line padding to avoid false sharing
    _padding: [u8; 64],
}

// SAFETY: SpscTimerWheel is Send because:
// - All atomic types are Send
// - UnsafeCell<Vec<T>> is Send when T is Send
// - The spatial separation guarantees no concurrent access to the same data
unsafe impl<T: Send> Send for SpscTimerWheel<T> {}

// SAFETY: SpscTimerWheel is Sync because:
// - Scheduler and poller threads access disjoint memory regions (spatial separation)
// - All cross-thread coordination uses atomics with proper ordering
// - UnsafeCell is never accessed concurrently for the same memory location
unsafe impl<T: Send> Sync for SpscTimerWheel<T> {}

impl<T> SpscTimerWheel<T> {
    const INITIAL_TICK_ALLOCATION: usize = 16;
    const NULL_DEADLINE: u64 = u64::MAX;

    /// Create new SPSC timer wheel
    pub fn new(start_time: Instant, tick_resolution: Duration, ticks_per_wheel: usize) -> Self {
        Self::with_allocation(
            start_time,
            tick_resolution,
            ticks_per_wheel,
            Self::INITIAL_TICK_ALLOCATION,
        )
    }

    /// Create SPSC timer wheel with custom initial allocation
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

        // Initialize free slot hints
        let mut next_free_hint = Vec::with_capacity(ticks_per_wheel);
        next_free_hint.resize(ticks_per_wheel, 0);

        let start_time_ns = 0; // Relative to start_time

        Self {
            tick_resolution_ns,
            start_time_ns,
            ticks_per_wheel,
            tick_mask,
            resolution_bits_to_shift,
            tick_allocation: AtomicUsize::new(initial_tick_allocation),
            allocation_bits_to_shift: AtomicU32::new(allocation_bits_to_shift),
            current_tick: AtomicU64::new(0),
            timer_count: AtomicU64::new(0),
            wheel: UnsafeCell::new(wheel),
            next_free_hint: UnsafeCell::new(next_free_hint),
            poll_position: AtomicUsize::new(0),
            notify_poller: AtomicBool::new(false),
            _padding: [0; 64],
        }
    }

    /// Schedule a timer (scheduler thread only)
    ///
    /// Adds a timer to the wheel for the given absolute deadline in nanoseconds.
    /// Returns a unique timer_id that can be used to cancel the timer later.
    ///
    /// # Thread Safety
    ///
    /// This method must only be called from the scheduler thread. It is safe to call
    /// concurrently with `poll()` running on the poller thread due to:
    ///
    /// 1. **Spatial separation**: Always schedules to `current_tick + 1` or later,
    ///    ensuring no overlap with the poller's current tick access
    /// 2. **Memory ordering**: Uses `Acquire` to read `current_tick` and `Release`
    ///    fence before notification to ensure timer data visibility
    ///
    /// # Arguments
    ///
    /// * `deadline_ns` - Absolute deadline in nanoseconds (relative to wheel start time)
    /// * `data` - User data to associate with this timer
    ///
    /// # Returns
    ///
    /// * `Some(timer_id)` - Unique ID for this timer, use with `cancel_timer()`
    /// * `None` - Failed to schedule (maximum capacity reached)
    pub fn schedule_timer(&self, deadline_ns: u64, data: T) -> Option<u64> {
        // THREAD SAFETY: Use Acquire ordering to establish happens-before with poller's
        // current_tick updates. Add 1 to ensure we never write to the spoke the poller
        // is currently reading (spatial separation guarantee).
        let deadline_tick = (deadline_ns.saturating_sub(self.start_time_ns)
            >> self.resolution_bits_to_shift)
            .max(self.current_tick.load(Ordering::Acquire) + 1);
        let spoke_index = (deadline_tick & self.tick_mask as u64) as usize;
        let allocation_bits = self.allocation_bits_to_shift.load(Ordering::Acquire);
        let tick_allocation = self.tick_allocation.load(Ordering::Acquire);
        let tick_start = spoke_index << allocation_bits;

        // SAFETY: We have exclusive access to:
        // 1. wheel[future_spoke] - spatial separation ensures poller never reads future spokes
        // 2. next_free_hint - only scheduler thread accesses this
        let wheel = unsafe { &mut *self.wheel.get() };
        let next_free_hint = unsafe { &mut *self.next_free_hint.get() };

        // Use hint for O(1) slot finding
        let hint = next_free_hint[spoke_index];
        let hint_index = tick_start + hint;

        // Fast path: hint points to free slot
        if hint < tick_allocation && wheel[hint_index].is_none() {
            wheel[hint_index] = Some((deadline_ns, data));
            self.timer_count.fetch_add(1, Ordering::Relaxed);
            next_free_hint[spoke_index] = (hint + 1) % tick_allocation;

            // THREAD SAFETY: Release fence establishes happens-before relationship with
            // poller's Acquire fence, ensuring timer data write is visible to poller thread.
            std::sync::atomic::fence(Ordering::Release);
            self.notify_poller.store(true, Ordering::Release);
            return Some(Self::timer_id_for_slot(spoke_index, hint));
        }

        // Slow path: linear search from hint position
        for i in 0..tick_allocation {
            let slot_idx = (hint + i) % tick_allocation;
            let index = tick_start + slot_idx;

            if wheel[index].is_none() {
                wheel[index] = Some((deadline_ns, data));
                self.timer_count.fetch_add(1, Ordering::Relaxed);
                next_free_hint[spoke_index] = (slot_idx + 1) % tick_allocation;

                // THREAD SAFETY: Release fence establishes happens-before relationship with
                // poller's Acquire fence, ensuring timer data write is visible to poller thread.
                std::sync::atomic::fence(Ordering::Release);
                self.notify_poller.store(true, Ordering::Release);
                return Some(Self::timer_id_for_slot(spoke_index, slot_idx));
            }
        }

        // Need to increase capacity
        self.increase_capacity(deadline_ns, spoke_index, data)
    }

    /// Cancel a previously scheduled timer (scheduler thread only)
    ///
    /// Removes a timer from the wheel and returns its associated user data.
    ///
    /// # Thread Safety
    ///
    /// This method must only be called from the scheduler thread. It is safe to call
    /// concurrently with `poll()` due to spatial separation - cancellation only affects
    /// future tick spokes (tick >= current_tick + 1) that the poller never accesses.
    ///
    /// Note: Cancelling a timer that has already expired (and been removed by poll) is
    /// safe and will simply return `None`. Attempting to cancel a timer in the current
    /// tick (being actively polled) will also return `None` to maintain spatial separation.
    ///
    /// # Arguments
    ///
    /// * `timer_id` - The ID returned by `schedule_timer()`
    ///
    /// # Returns
    ///
    /// * `Some(data)` - The user data associated with the cancelled timer
    /// * `None` - Timer not found (already expired, cancelled, invalid ID, or in current tick)
    pub fn cancel_timer(&self, timer_id: u64) -> Option<T> {
        let spoke_index = Self::tick_for_timer_id(timer_id);
        let tick_index = Self::index_in_tick_array(timer_id);

        // THREAD SAFETY: Cancellation relies on the same spatial separation as scheduling.
        // Timers are always scheduled to current_tick+1 or later, so they're never in the
        // tick being actively polled. As long as cancellation happens promptly after scheduling
        // (before the poller advances to that tick), it's safe.
        //
        // Note: If you cancel a timer after the poller has already advanced to its tick,
        // the cancellation may fail (return None) or succeed if the timer hasn't been polled yet.
        // This is acceptable behavior for a lock-free structure.

        let allocation_bits = self.allocation_bits_to_shift.load(Ordering::Acquire);
        let tick_allocation = self.tick_allocation.load(Ordering::Acquire);
        let wheel_index = (spoke_index << allocation_bits) + tick_index;

        if spoke_index < self.ticks_per_wheel && tick_index < tick_allocation {
            // SAFETY: We have exclusive access to wheel[future_spoke] due to spatial separation.
            // The scheduler only writes to current_tick+1 or later, and poller only reads current_tick.
            let wheel = unsafe { &mut *self.wheel.get() };

            // Use Release ordering to ensure the cancellation is visible to the poller
            std::sync::atomic::fence(Ordering::Release);
            if let Some((_, data)) = wheel[wheel_index].take() {
                self.timer_count.fetch_sub(1, Ordering::Relaxed);
                return Some(data);
            }
        }

        None
    }

    /// Poll for expired timers (poller thread only)
    ///
    /// Checks the current tick for timers that have reached their deadline and
    /// returns them. Automatically advances to the next tick when appropriate.
    ///
    /// # Thread Safety
    ///
    /// This method must only be called from the poller thread. It is safe to call
    /// concurrently with `schedule_timer()` and `cancel_timer()` running on the
    /// scheduler thread due to:
    ///
    /// 1. **Spatial separation**: Only accesses the current tick spoke, while the
    ///    scheduler only writes to `current_tick + 1` and later spokes
    /// 2. **Memory ordering**: Uses `Acquire` fence at the start to ensure visibility
    ///    of all timer data written by the scheduler thread before this call
    ///
    /// # Arguments
    ///
    /// * `now_ns` - Current time in nanoseconds (relative to wheel start time)
    /// * `expiry_limit` - Maximum number of timers to return in a single poll
    ///
    /// # Returns
    ///
    /// Vector of tuples containing `(timer_id, deadline_ns, user_data)` for each
    /// expired timer, up to `expiry_limit` entries.
    ///
    /// # Performance Note
    ///
    /// Setting `expiry_limit` appropriately prevents unbounded work per poll call.
    /// If more timers are ready than the limit, subsequent polls will return them.
    pub fn poll(&self, now_ns: u64, expiry_limit: usize) -> Vec<(u64, u64, T)> {
        let mut expired = Vec::with_capacity(expiry_limit);

        // THREAD SAFETY: Acquire fence establishes happens-before relationship with
        // scheduler's Release fence, ensuring we see all timer data written by scheduler.
        std::sync::atomic::fence(Ordering::Acquire);

        if self.timer_count.load(Ordering::Relaxed) == 0 {
            if now_ns >= self.current_tick_time_ns() {
                self.current_tick.fetch_add(1, Ordering::Release);
                self.poll_position.store(0, Ordering::Relaxed);
            }
            self.notify_poller.store(false, Ordering::Release);
            return expired;
        }

        let current_tick = self.current_tick.load(Ordering::Relaxed);
        let spoke_index = (current_tick & self.tick_mask as u64) as usize;
        let poll_pos = self.poll_position.load(Ordering::Relaxed);
        let allocation_bits = self.allocation_bits_to_shift.load(Ordering::Acquire);
        let tick_allocation = self.tick_allocation.load(Ordering::Acquire);
        let tick_start = spoke_index << allocation_bits;

        // SAFETY: We have exclusive access to wheel[current_tick] because:
        // 1. Only poller thread accesses current tick
        // 2. Scheduler only writes to current_tick+1 or later (spatial separation)
        let wheel = unsafe { &mut *self.wheel.get() };

        let mut processed = 0;
        for i in 0..tick_allocation {
            if expired.len() >= expiry_limit {
                break;
            }

            let slot_idx = (poll_pos + i) % tick_allocation;
            let wheel_index = tick_start + slot_idx;

            if let Some((deadline, data)) = &wheel[wheel_index] {
                if now_ns >= *deadline {
                    let (deadline, data) = wheel[wheel_index].take().unwrap();
                    self.timer_count.fetch_sub(1, Ordering::Relaxed);
                    let timer_id = Self::timer_id_for_slot(spoke_index, slot_idx);
                    expired.push((timer_id, deadline, data));
                }
            }
            processed += 1;
        }

        // Update poll position
        let new_poll_pos = (poll_pos + processed) % tick_allocation;
        self.poll_position.store(new_poll_pos, Ordering::Relaxed);

        // Advance tick if we've processed all slots and time has passed
        // Use Release ordering so scheduler sees the updated current_tick
        if new_poll_pos == 0 && now_ns >= self.current_tick_time_ns() {
            self.current_tick.fetch_add(1, Ordering::Release);
        }

        // Clear notification flag
        self.notify_poller.store(false, Ordering::Release);

        expired
    }

    /// Check if poller should wake up (poller thread)
    pub fn should_poll(&self) -> bool {
        self.notify_poller.load(Ordering::Acquire) || self.timer_count.load(Ordering::Relaxed) > 0
    }

    /// Force advancement of current tick (poller thread only)
    pub fn advance_to(&self, now_ns: u64) {
        let new_tick = (now_ns.saturating_sub(self.start_time_ns)) >> self.resolution_bits_to_shift;
        let current = self.current_tick.load(Ordering::Relaxed);
        if new_tick > current {
            self.current_tick.store(new_tick, Ordering::Release);
            self.poll_position.store(0, Ordering::Relaxed);
        }
    }

    /// Get current tick time in nanoseconds
    pub fn current_tick_time_ns(&self) -> u64 {
        let current_tick = self.current_tick.load(Ordering::Relaxed);
        ((current_tick + 1) << self.resolution_bits_to_shift) + self.start_time_ns
    }

    /// Get number of active timers
    pub fn timer_count(&self) -> u64 {
        self.timer_count.load(Ordering::Relaxed)
    }

    /// Clear all timers (scheduler thread only)
    pub fn clear(&self) {
        // SAFETY: Only scheduler thread calls this, and it has exclusive access
        // to all wheel slots it writes to (current_tick+1 and later due to spatial separation)
        let wheel = unsafe { &mut *self.wheel.get() };
        for slot in wheel {
            *slot = None;
        }
        let count = self.timer_count.swap(0, Ordering::Relaxed);
        if count > 0 {
            self.notify_poller.store(false, Ordering::Release);
        }
    }

    /// Increase wheel capacity during scheduling (scheduler thread only)
    fn increase_capacity(&self, deadline_ns: u64, spoke_index: usize, data: T) -> Option<u64> {
        let old_tick_allocation = self.tick_allocation.load(Ordering::Relaxed);
        let old_allocation_bits = self.allocation_bits_to_shift.load(Ordering::Relaxed);
        let new_tick_allocation = old_tick_allocation << 1;
        let new_allocation_bits = new_tick_allocation.trailing_zeros();

        let new_capacity = self.ticks_per_wheel * new_tick_allocation;
        if new_capacity > (1 << 30) {
            return None; // Max capacity reached
        }

        let mut new_wheel = Vec::with_capacity(new_capacity);
        new_wheel.resize_with(new_capacity, || None);

        // SAFETY: Only scheduler thread calls increase_capacity, and it has exclusive
        // access to future tick spokes. The poller only reads current tick.
        let old_wheel = unsafe { &mut *self.wheel.get() };
        let next_free_hint = unsafe { &mut *self.next_free_hint.get() };

        // Copy old data to new wheel
        for j in 0..self.ticks_per_wheel {
            let old_start = j << old_allocation_bits;
            let new_start = j << new_allocation_bits;

            for k in 0..old_tick_allocation {
                new_wheel[new_start + k] = old_wheel[old_start + k].take();
            }
        }

        // Add new timer
        let new_index = (spoke_index << new_allocation_bits) + old_tick_allocation;
        new_wheel[new_index] = Some((deadline_ns, data));

        let timer_id = Self::timer_id_for_slot(spoke_index, old_tick_allocation);
        self.timer_count.fetch_add(1, Ordering::Relaxed);

        // Replace the wheel
        *old_wheel = new_wheel;

        // Reset free hints
        for hint in next_free_hint {
            *hint = 0;
        }

        // CRITICAL: Update allocation fields with Release ordering AFTER wheel is updated
        // This ensures poller sees the new wheel before using the new allocation values
        self.tick_allocation
            .store(new_tick_allocation, Ordering::Release);
        self.allocation_bits_to_shift
            .store(new_allocation_bits, Ordering::Release);

        // Release fence ensures new wheel structure is visible before notification
        std::sync::atomic::fence(Ordering::Release);
        self.notify_poller.store(true, Ordering::Release);

        Some(timer_id)
    }

    /// Create timer ID from spoke and slot indices
    #[inline]
    fn timer_id_for_slot(tick_on_wheel: usize, tick_array_index: usize) -> u64 {
        ((tick_on_wheel as u64) << 32) | (tick_array_index as u64)
    }

    /// Extract spoke index from timer ID
    #[inline]
    fn tick_for_timer_id(timer_id: u64) -> usize {
        (timer_id >> 32) as usize
    }

    /// Extract slot index from timer ID
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
        let mut wheel = TimerWheel::<u32>::new(start, Duration::from_nanos(1024 * 1024), 256);

        let timer_id = wheel.schedule_timer(100, 42).unwrap();
        assert_eq!(wheel.timer_count(), 1);

        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, 42);
        assert_eq!(wheel.timer_count(), 0);
    }

    #[test]
    fn test_expiry() {
        let start = Instant::now();
        let mut wheel = TimerWheel::<&str>::new(start, Duration::from_nanos(1024 * 1024), 256);

        wheel.schedule_timer(5, "task1").unwrap();
        wheel.schedule_timer(15, "task2").unwrap();

        let expired = wheel.poll(10, 10);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].2, "task1");

        let expired = wheel.poll(20, 10);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].2, "task2");
    }

    // SpscTimerWheel tests
    #[test]
    fn test_spsc_basic_functionality() {
        let start = Instant::now();
        let wheel = SpscTimerWheel::<u32>::new(
            start,
            Duration::from_nanos(1024), // 2^10 ns = 1.024 µs
            256,
        );

        // Schedule a timer
        let timer_id = wheel.schedule_timer(5_000, 42).unwrap();
        assert_eq!(wheel.timer_count(), 1);

        // Poll progressively - poll() will automatically advance ticks
        let mut all_expired = Vec::new();
        let mut time = 0;
        while all_expired.is_empty() && time < 100_000 {
            let expired = wheel.poll(time, 10);
            all_expired.extend(expired);
            time += 1024; // Advance by one tick resolution
        }

        assert_eq!(all_expired.len(), 1);
        assert_eq!(all_expired[0].2, 42);
        assert_eq!(wheel.timer_count(), 0);
    }

    #[test]
    fn test_spsc_cancellation() {
        let start = Instant::now();
        let wheel = SpscTimerWheel::<&str>::new(
            start,
            Duration::from_nanos(1024), // 2^10 ns
            256,
        );

        let timer_id = wheel.schedule_timer(5_000, "test").unwrap();
        assert_eq!(wheel.timer_count(), 1);

        let data = wheel.cancel_timer(timer_id).unwrap();
        assert_eq!(data, "test");
        assert_eq!(wheel.timer_count(), 0);

        // Should not be in expired list
        let expired = wheel.poll(10_000, 10);
        assert_eq!(expired.len(), 0);
    }

    #[test]
    fn test_spsc_multiple_timers_same_tick() {
        let start = Instant::now();
        let wheel = SpscTimerWheel::<usize>::new(
            start,
            Duration::from_nanos(1024), // 2^10 ns
            256,
        );

        // Schedule multiple timers for the same tick
        for i in 0..10 {
            wheel.schedule_timer(5_000, i).unwrap();
        }
        assert_eq!(wheel.timer_count(), 10);

        // Poll progressively - poll() will automatically advance ticks
        let mut all_expired = Vec::new();
        let mut time = 0;
        while all_expired.len() < 10 && time < 100_000 {
            let expired = wheel.poll(time, 20);
            all_expired.extend(expired);
            time += 1024; // Advance by one tick resolution
        }

        assert_eq!(all_expired.len(), 10);

        // Verify all values are present
        let mut values: Vec<usize> = all_expired.iter().map(|(_, _, v)| *v).collect();
        values.sort();
        assert_eq!(values, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    // Concurrent SPSC tests
    #[test]
    fn test_spsc_concurrent_scheduling_and_polling() {
        let start = Instant::now();
        let wheel = Arc::new(SpscTimerWheel::<usize>::new(
            start,
            Duration::from_nanos(1024),
            256,
        ));

        let wheel_sched = wheel.clone();
        let scheduler = thread::spawn(move || {
            // Schedule 100 timers
            for i in 0..100 {
                let deadline = 10_000 + (i * 1024);
                wheel_sched.schedule_timer(deadline as u64, i).unwrap();
                thread::sleep(Duration::from_micros(10));
            }
        });

        let wheel_poll = wheel.clone();
        let poller = thread::spawn(move || {
            let mut all_expired = Vec::new();
            let start_time = Instant::now();

            while all_expired.len() < 100 && start_time.elapsed() < Duration::from_secs(5) {
                let now = start_time.elapsed().as_nanos() as u64;
                let expired = wheel_poll.poll(now, 10);
                all_expired.extend(expired);
                thread::sleep(Duration::from_micros(100));
            }

            all_expired
        });

        scheduler.join().unwrap();
        let expired = poller.join().unwrap();

        assert_eq!(expired.len(), 100, "Expected 100 expired timers");

        // Verify all values are present
        let mut values: Vec<usize> = expired.iter().map(|(_, _, v)| *v).collect();
        values.sort();
        assert_eq!(values, (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn test_spsc_cancel_during_concurrent_poll() {
        let start = Instant::now();
        let wheel = Arc::new(SpscTimerWheel::<usize>::new(
            start,
            Duration::from_nanos(1024),
            256,
        ));

        // Schedule 20 timers, then cancel half before starting poller
        let timer_ids: Vec<_> = (0..20)
            .map(|i| {
                let deadline = 10_000 + (i * 1024);
                wheel.schedule_timer(deadline as u64, i).unwrap()
            })
            .collect();

        // Cancel every other timer (10 timers)
        for i in (0..20).step_by(2) {
            wheel.cancel_timer(timer_ids[i]).unwrap();
        }

        let wheel_poll = wheel.clone();
        let poller = thread::spawn(move || {
            let mut all_expired = Vec::new();
            let start_time = Instant::now();

            while all_expired.len() < 10 && start_time.elapsed() < Duration::from_secs(5) {
                let now = start_time.elapsed().as_nanos() as u64;
                let expired = wheel_poll.poll(now, 20);
                all_expired.extend(expired);
                thread::sleep(Duration::from_micros(100));
            }

            all_expired
        });

        let expired = poller.join().unwrap();

        // Should get exactly 10 timers (the ones not cancelled)
        assert_eq!(
            expired.len(),
            10,
            "Expected exactly 10 timers, got {}",
            expired.len()
        );

        // Verify we got the odd-numbered timers (1, 3, 5, ..., 19)
        let mut values: Vec<usize> = expired.iter().map(|(_, _, v)| *v).collect();
        values.sort();
        let expected: Vec<usize> = (0..20).filter(|i| i % 2 == 1).collect();
        assert_eq!(values, expected);
    }

    #[test]
    fn test_spsc_capacity_growth_concurrent() {
        let start = Instant::now();
        let wheel = Arc::new(SpscTimerWheel::<usize>::with_allocation(
            start,
            Duration::from_nanos(1024),
            256,
            4, // Small initial allocation to force growth
        ));

        let wheel_sched = wheel.clone();
        let scheduler = thread::spawn(move || {
            // Schedule many timers to same far-future deadline to trigger capacity growth
            // Use a far-future deadline to ensure they won't expire before we finish scheduling
            for i in 0..50 {
                wheel_sched.schedule_timer(100_000, i).unwrap();
            }
        });

        // Wait for scheduler to finish before starting poller
        scheduler.join().unwrap();

        let wheel_poll = wheel.clone();
        let poller = thread::spawn(move || {
            let mut all_expired = Vec::new();
            let start_time = Instant::now();

            // Now poll until we get all 50 timers
            while all_expired.len() < 50 && start_time.elapsed() < Duration::from_secs(5) {
                let now = start_time.elapsed().as_nanos() as u64;
                let expired = wheel_poll.poll(now, 10);
                all_expired.extend(expired);
                thread::sleep(Duration::from_micros(100));
            }

            all_expired
        });

        let expired = poller.join().unwrap();

        assert_eq!(expired.len(), 50, "Expected 50 expired timers");

        // Verify all values are present
        let mut values: Vec<usize> = expired.iter().map(|(_, _, v)| *v).collect();
        values.sort();
        assert_eq!(values, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn test_spsc_stress_many_timers() {
        let start = Instant::now();
        let wheel = Arc::new(SpscTimerWheel::<usize>::new(
            start,
            Duration::from_nanos(1024),
            256,
        ));

        let wheel_sched = wheel.clone();
        let scheduler = thread::spawn(move || {
            // Schedule 1000 timers across various deadlines
            for i in 0..1000 {
                let deadline = 10_000 + ((i % 100) * 1024);
                wheel_sched.schedule_timer(deadline as u64, i).unwrap();
            }
        });

        let wheel_poll = wheel.clone();
        let poller = thread::spawn(move || {
            let mut all_expired = Vec::new();
            let start_time = Instant::now();

            while all_expired.len() < 1000 && start_time.elapsed() < Duration::from_secs(10) {
                let now = start_time.elapsed().as_nanos() as u64;
                let expired = wheel_poll.poll(now, 50);
                all_expired.extend(expired);
                thread::sleep(Duration::from_micros(50));
            }

            all_expired
        });

        scheduler.join().unwrap();
        let expired = poller.join().unwrap();

        assert_eq!(expired.len(), 1000, "Expected 1000 expired timers");

        // Verify all values are present
        let mut values: Vec<usize> = expired.iter().map(|(_, _, v)| *v).collect();
        values.sort();
        assert_eq!(values, (0..1000).collect::<Vec<_>>());
    }
}
