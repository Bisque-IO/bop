# Timer Wheel Implementation

A high-performance hierarchical hashed timing wheel for efficient timer scheduling, based on the paper "Hashed and Hierarchical Timing Wheels" by Varghese and Lauck.

## Overview

The timer wheel provides O(1) timer scheduling and cancellation with efficient polling for expired timers. It uses a three-level hierarchy to support timers ranging from milliseconds to hours without excessive memory usage.

### Key Features

- **No Internal Cascading**: Timers stay in their original wheel until expiry - no movement between levels
- **Caller-Managed Cascading**: `schedule_timer_best_fit` enables arbitrarily long timers via caller reschedules
- **Stable Timer IDs**: Timer IDs remain valid until the timer expires or is cancelled
- **Per-Spoke Vectors**: Each spoke grows independently with free list for O(1) slot reuse
- **Automatic Compaction**: Spokes shrink after polling when utilization drops below threshold
- **Near-Perfect Timing**: Best-fit scheduling achieves precision via progressive wheel selection

## Architecture

### Three-Level Hierarchy

| Level | Tick Resolution | Spokes | Coverage | Poll Frequency |
|-------|-----------------|--------|----------|----------------|
| L0 | ~1.05ms (2^20 ns) | 1024 | ~1.07s | Every poll |
| L1 | ~1.07s (2^30 ns) | 1024 | ~18 min | When L1 tick advances |
| L2 | ~68.7s (2^36 ns) | 1024 | ~19.5 hr | When L2 tick advances |

Timers are placed in the appropriate wheel based on their deadline:
- Deadline < 1s -> L0
- Deadline < 18 min -> L1
- Deadline < 19.5 hr -> L2
- Deadline > 19.5 hr -> Use `schedule_timer_best_fit` for cascading

### Data Structures

```
TimerWheel<T>
├── L0: SingleWheel<T>      (fast timers)
├── L1: SingleWheel<T>      (medium timers)
├── L2: SingleWheel<T>      (slow timers)
├── prev_l0_tick: u64       (last polled L0 tick)
├── prev_l1_tick: u64       (last polled L1 tick)
├── prev_l2_tick: u64       (last polled L2 tick)
└── coverage thresholds...

SingleWheel<T>
├── spokes: Vec<Spoke<T>>   (1024 buckets)
├── num_spokes: usize
├── spoke_mask: usize       (for fast modulo)
├── tick_resolution_ns: u64
├── tick_shift: u32         (log2 of resolution)
├── timer_count: u64
└── cached_next_deadline: u64

Spoke<T>
├── entries: Vec<Option<TimerEntry<T>>>
├── free_list: Vec<usize>   (stack of free slots)
├── active_count: usize
└── generation: u32         (increments on compaction)

TimerEntry<T>
├── deadline_ns: u64
├── slot_index: u32
├── generation: u32
└── data: T
```

### Timer ID Encoding

Timer IDs are 64-bit integers encoding the timer's location:

```
+--------+----------------------+------------+-----------------------------+
| Level  | Generation           | Spoke      | Slot                        |
| 2 bits | 22 bits              | 12 bits    | 28 bits                     |
+--------+----------------------+------------+-----------------------------+
 63    62  61                40  39       28  27                          0
```

| Field | Bits | Max Value | Purpose |
|-------|------|-----------|---------|
| Level | 2 | 3 | Which wheel (0=L0, 1=L1, 2=L2) |
| Generation | 22 | 4,194,303 | Detects stale IDs after compaction |
| Spoke | 12 | 4,095 | Which spoke in the wheel |
| Slot | 28 | 268,435,455 | Which slot in the spoke |

## Operations

### Schedule Timer - O(1)

```rust
pub fn schedule_timer(&mut self, deadline_ns: u64, data: T) -> Result<u64, TimerWheelError>
```

1. Validate deadline (not zero, not too far in past, not beyond L2 coverage)
2. Calculate relative deadline from start time
3. Route to appropriate wheel (L0/L1/L2) based on deadline
4. Calculate spoke index: `spoke = (deadline_ns >> tick_shift) & spoke_mask`
5. Get free slot from spoke's free list (or grow spoke)
6. Insert timer entry with current generation
7. Return encoded timer ID

### Cancel Timer - O(1)

```rust
pub fn cancel_timer(&mut self, timer_id: u64) -> Result<T, TimerWheelError>
```

1. Decode timer ID to get level, spoke, slot, generation
2. Look up entry in the appropriate wheel/spoke/slot
3. Verify generation matches (rejects stale IDs after compaction)
4. Remove entry, add slot to free list
5. Return timer data

### Poll - O(expired_timers)

```rust
pub fn poll(&mut self, now_ns: u64, expiry_limit: usize, output: &mut Vec<(u64, u64, T)>) -> usize
```

1. Calculate current tick for each level
2. **L0**: Always poll if tick advanced or timers exist
   - Scan spokes from `prev_tick` to `current_tick` (capped at num_spokes)
   - For each entry with `deadline_ns <= now_ns`, remove and output
3. **L1**: Only poll if L1 tick advanced (~1 second intervals)
4. **L2**: Only poll if L2 tick advanced (~68 second intervals)
5. Update `prev_*_tick` for each level
6. Consider spoke compaction after polling

### Schedule Timer Best-Fit - O(1)

```rust
pub fn schedule_timer_best_fit(&mut self, target_deadline_ns: u64, data: T)
    -> Result<(u64, u64), TimerWheelError>
```

Returns `(timer_id, wheel_deadline_ns)` where `wheel_deadline_ns` may be earlier than `target_deadline_ns`.

This enables **arbitrarily long timers** via caller-managed cascading:

1. For deadlines within L2 coverage: schedules at exact target, returns `(id, target)`
2. For deadlines beyond L2 coverage: schedules at L2 boundary, returns `(id, boundary)`
3. Caller reschedules for remaining time when wheel timer fires

**Example: 1-week timer (beyond ~19.5hr L2 coverage)**

```rust
// Initial schedule - target is 1 week away
let (id, wheel_deadline) = wheel.schedule_timer_best_fit(target_ns, data)?;
// wheel_deadline is ~19.5hr from now (L2 boundary)

// When wheel timer fires at wheel_deadline:
if now_ns >= wheel_deadline && now_ns < target_ns {
    // Reschedule for remaining time
    let (new_id, new_wheel_deadline) = wheel.schedule_timer_best_fit(target_ns, data)?;
    // Repeat until target is reached
}
```

The `Timer` struct in the runtime uses this pattern automatically via `TimerDelay`:
- Tracks both `target_deadline_ns` (user's intent) and `wheel_deadline_ns` (current schedule)
- Automatically reschedules when wheel fires but target not reached
- Provides near-perfect timing by cascading through progressively finer wheels

## Memory Management

### Per-Spoke Allocation

Each spoke maintains its own vector and free list:

```rust
struct Spoke<T> {
    entries: Vec<Option<TimerEntry<T>>>,  // Grows independently
    free_list: Vec<usize>,                 // O(1) slot reuse
    active_count: usize,
    generation: u32,
}
```

**Benefits**:
- Hot spokes grow without affecting cold spokes
- Memory is only allocated where needed
- Free list enables O(1) slot reuse without fragmentation

### Compaction

Compaction reclaims memory when a spoke becomes sparse:

```rust
pub struct CompactionConfig {
    min_capacity_for_compaction: usize,  // Default: 64
    compaction_ratio: usize,             // Default: 4 (compact when <25% utilized)
    target_capacity_multiplier: usize,   // Default: 2
}
```

**Compaction triggers when**:
- `capacity >= min_capacity_for_compaction`
- `active_count * compaction_ratio < capacity`

**Compaction process**:
1. Collect all active entries
2. Increment generation (invalidates outstanding timer IDs for this spoke)
3. Rebuild entries vector at target capacity
4. Rebuild free list
5. Return slot remapping (old_slot -> new_slot)

**Important**: Compaction only happens AFTER polling removes expired entries, ensuring no timers are lost.

### Generation Counter

The generation counter protects against use-after-free bugs:

1. Each spoke has a generation counter (starts at 0)
2. Timer entries store the generation at insertion time
3. Generation increments on compaction
4. Cancel operations verify generation matches
5. Stale timer IDs (from before compaction) are rejected

With 22 bits in the timer ID, a spoke can compact ~4 million times before generation wraps.

## Correctness Guarantees

### No Timers Lost

Timers are never lost, even if the poller falls behind:

1. **Deadline-based expiration**: We check `deadline_ns <= now_ns`, not tick position
2. **Entries persist until action**: Unpolled expired timers stay in their slot
3. **Full spoke scan**: When far behind, we scan all spokes (capped at num_spokes)
4. **Cancel removes entry**: Cancelled timers are removed immediately

### Slow Poller Scenario

If the poller is 1 hour behind on L0 (1s coverage):

1. Time has advanced 3,600,000+ ticks
2. `ticks_to_scan` is capped at 1024 (num_spokes)
3. All 1024 spokes are scanned
4. Every entry with `deadline_ns <= now_ns` fires
5. No timers are lost - they've been waiting in their spokes

### Generation Wraparound

After 4+ million compactions on a single spoke, generation could wrap and match a very old timer ID. This is extremely unlikely in practice (would require sustained high churn on a single spoke).

## API Reference

### TimerWheel

```rust
impl<T> TimerWheel<T> {
    // Construction
    pub fn new(worker_id: u32) -> Self;
    pub fn with_config(l0_tick_ns: u64, l1_tick_ns: u64, l2_tick_ns: u64,
                       spokes_per_wheel: usize, worker_id: u32) -> Self;
    pub fn from_duration(tick_resolution: Duration, ticks_per_wheel: usize,
                         worker_id: u32) -> Self;

    // Configuration
    pub fn set_start_time_ns(&mut self, start_time_ns: u64);
    pub fn start_time_ns(&self) -> u64;
    pub fn max_duration_ns(&self) -> u64;

    // Timer operations
    pub fn schedule_timer(&mut self, deadline_ns: u64, data: T) -> Result<u64, TimerWheelError>;
    pub fn schedule_timer_best_fit(&mut self, target_deadline_ns: u64, data: T)
        -> Result<(u64, u64), TimerWheelError>;  // Returns (timer_id, wheel_deadline)
    pub fn cancel_timer(&mut self, timer_id: u64) -> Result<T, TimerWheelError>;
    pub fn poll(&mut self, now_ns: u64, expiry_limit: usize,
                output: &mut Vec<(u64, u64, T)>) -> usize;

    // Queries
    pub fn timer_count(&self) -> u64;
    pub fn next_deadline(&mut self) -> Option<u64>;
    pub fn now_ns(&self) -> u64;
    pub fn worker_id(&self) -> u32;
    pub fn l0_coverage_ns(&self) -> u64;
    pub fn l1_coverage_ns(&self) -> u64;
    pub fn l2_coverage_ns(&self) -> u64;

    // Maintenance
    pub fn clear(&mut self);
    pub fn compact_all(&mut self);
    pub fn memory_stats(&self) -> TimerWheelMemoryStats;
}
```

### Error Types

```rust
pub enum TimerWheelError {
    InvalidTimerId,     // Timer ID format invalid or level > 2
    InvalidDeadline,    // Deadline is zero, before start_time, or too far in past
    DeadlineTooFar,     // Deadline exceeds L2 coverage (~19.5 hours)
    CapacityExceeded,   // Spoke capacity limit reached
    Overflow,           // Integer overflow in calculations
    TimerNotFound,      // No timer at specified location, or generation mismatch
}
```

### Memory Statistics

```rust
pub struct TimerWheelMemoryStats {
    pub l0: WheelMemoryStats,
    pub l1: WheelMemoryStats,
    pub l2: WheelMemoryStats,
}

pub struct WheelMemoryStats {
    pub num_spokes: usize,
    pub total_slot_capacity: usize,
    pub active_timers: usize,
    pub utilization: f64,
}
```

## Usage Example

```rust
use std::time::Duration;

// Create timer wheel
let mut wheel = TimerWheel::new(0);  // worker_id = 0
wheel.set_start_time_ns(0);

// Schedule timers
let fast_timer = wheel.schedule_timer(10_000_000, "10ms timer").unwrap();
let slow_timer = wheel.schedule_timer(5_000_000_000, "5s timer").unwrap();

// Poll for expired timers
let mut output = Vec::new();
let now_ns = 20_000_000;  // 20ms
let expired = wheel.poll(now_ns, 100, &mut output);

// Process expired timers
for (timer_id, deadline_ns, data) in output {
    println!("Timer fired: {} at deadline {}", data, deadline_ns);
}

// Cancel a timer
if let Ok(data) = wheel.cancel_timer(slow_timer) {
    println!("Cancelled timer with data: {}", data);
}
```

## Performance Characteristics

| Operation | Time Complexity | Notes |
|-----------|-----------------|-------|
| Schedule | O(1) amortized | May grow spoke vector |
| Cancel | O(1) | Direct index lookup |
| Poll (per wheel) | O(expired + spokes_scanned) | Deadline-based filtering |
| Next Deadline | O(1) cached, O(n) recompute | Cached value invalidated on cancel |
| Compaction | O(active_entries) | Only when utilization < 25% |

## Thread Safety

**The timer wheel is NOT thread-safe.** Callers must synchronize access. The `now_ns` field uses `AtomicU64` for potential cross-thread reads of the current time, but all mutation operations require exclusive access.

## Design Trade-offs

### No Cascading

Unlike traditional hierarchical timing wheels, this implementation does NOT cascade timers from higher to lower wheels as time advances.

**Pros**:
- Stable timer IDs (timers never move)
- Simpler implementation
- No cascade overhead during poll

**Cons**:
- L1/L2 wheels polled less frequently (but only when their tick advances)
- Timers fire with resolution of their wheel (L2 timers have ~68s granularity)

### Approximate Timeouts

Timers fire when `deadline_ns <= now_ns` is checked during poll. The actual firing time depends on:
1. Which wheel the timer is in (determines poll frequency)
2. When the spoke is scanned (depends on tick advancement)
3. The `expiry_limit` parameter (may defer some expirations)

For most use cases, this approximation is acceptable. If sub-millisecond precision is required, use L0 timers only.
