# Hierarchical TimerWheel Design (2-Level)

## Overview
Refactor TimerWheel from single-level to 2-level hierarchical design for better handling of diverse timeout ranges (10ms - 18 minutes).

## Configuration

```
L0 (Fast):   1.05ms/tick × 1024 ticks = 1.07s coverage
L1 (Medium): 1.07s/tick  × 1024 ticks = 18.33min coverage
```

## Memory
- Current: 0.38 MB per TimerWheel
- Hierarchical: 0.77 MB per TimerWheel (2x current)
- 8 workers: 6.16 MB total (vs 3.06 MB current, acceptable tradeoff)

## Data Structure Changes

### Option 1: Nested Wheels (Simpler)
```rust
pub struct TimerWheel<T> {
    // L0 (fast) wheel fields (keep current structure)
    l0_tick_resolution_ns: u64,
    l0_current_tick: u64,
    l0_wheel: Vec<Option<(u64, T)>>,
    l0_next_free_hint: Vec<usize>,
    // ... other L0 fields
    
    // L1 (medium) wheel fields (mirror L0 structure)
    l1_tick_resolution_ns: u64,  // = l0_tick_resolution_ns * 1024
    l1_current_tick: u64,
    l1_wheel: Vec<Option<(u64, T)>>,
    l1_next_free_hint: Vec<usize>,
    // ... other L1 fields
    
    // Shared fields
    null_deadline: u64,
    start_time_ns: u64,
    now_ns: AtomicU64,
    worker_id: u32,
    ticks_per_wheel: usize,  // 1024 for both levels
}
```

### Option 2: Extract SingleWheel (Cleaner but more refactoring)
```rust
struct SingleWheel<T> {
    tick_resolution_ns: u64,
    current_tick: u64,
    wheel: Vec<Option<(u64, T)>>,
    next_free_hint: Vec<usize>,
    tick_allocation: usize,
    poll_index: usize,
    // ... wheel-specific state
}

pub struct TimerWheel<T> {
    l0: SingleWheel<T>,
    l1: SingleWheel<T>,
    // Shared state
    null_deadline: u64,
    start_time_ns: u64,
    now_ns: AtomicU64,
    worker_id: u32,
    total_timer_count: u64,
    cached_next_deadline: u64,
}
```

**Decision: Use Option 2** - Cleaner separation, easier to test individual wheels.

## Timer ID Encoding

Current: `timer_id = (spoke_index << 32) | slot_index`

New: `timer_id = (level << 62) | (spoke_index << 32) | slot_index`
- Bits 62-63: wheel level (0 = L0, 1 = L1)
- Bits 32-61: spoke index (30 bits, supports up to 1B spokes)
- Bits 0-31: slot index (32 bits, supports up to 4B slots)

## Key Algorithms

### schedule_timer(deadline_ns, data)
```rust
// Determine which wheel
let l0_coverage_ns = l0_tick_resolution_ns * 1024;
if deadline_ns < l0_coverage_ns {
    // Schedule in L0
    let timer_id = l0.schedule_internal(deadline_ns, data)?;
    Some(encode_timer_id(0, timer_id))
} else {
    // Schedule in L1
    let timer_id = l1.schedule_internal(deadline_ns, data)?;
    Some(encode_timer_id(1, timer_id))
}
```

### poll(now_ns, expiry_limit, output)
```rust
// 1. Check if L1 tick expired and cascade
let l1_tick = now_ns / l1_tick_resolution_ns;
while l1_current_tick < l1_tick {
    cascade_l1_to_l0(l1_current_tick);
    l1_current_tick += 1;
}

// 2. Poll L0 for expired timers
l0.poll_internal(now_ns, expiry_limit, output)
```

### cascade_l1_to_l0(l1_tick)
```rust
let spoke_index = (l1_tick & tick_mask) as usize;
let tick_start = spoke_index << allocation_bits;

// Collect all timers from L1 spoke
for slot in tick_start..(tick_start + tick_allocation) {
    if let Some((deadline_ns, data)) = l1_wheel[slot].take() {
        // Reschedule into L0
        l0.schedule_internal(deadline_ns, data);
        l1_timer_count -= 1;
    }
}

// Reset L1 spoke hints
l1_next_free_hint[spoke_index] = 0;
```

### cancel_timer(timer_id)
```rust
let level = (timer_id >> 62) & 0x3;
let inner_id = timer_id & 0x3FFFFFFFFFFFFFFF;

match level {
    0 => l0.cancel_internal(inner_id),
    1 => l1.cancel_internal(inner_id),
    _ => None,
}
```

### next_deadline()
```rust
let l0_next = l0.next_deadline_internal();
let l1_next = l1.next_deadline_internal();

match (l0_next, l1_next) {
    (Some(d0), Some(d1)) => Some(d0.min(d1)),
    (Some(d0), None) => Some(d0),
    (None, Some(d1)) => Some(d1),
    (None, None) => None,
}
```

## Implementation Plan

1. Extract SingleWheel struct from TimerWheel
2. Implement SingleWheel with all current TimerWheel logic
3. Refactor TimerWheel to contain l0 and l1 SingleWheels
4. Update schedule_timer() with wheel selection logic
5. Implement cascade_l1_to_l0()
6. Update poll() to handle cascading
7. Update cancel_timer() to check both wheels
8. Update next_deadline() to check both wheels
9. Update timer_id encoding/decoding helpers
10. Test with existing timer_tick_async example

## Testing Strategy

1. Unit test: Timer < 1s goes to L0
2. Unit test: Timer > 1s goes to L1
3. Unit test: Cascade happens at correct time
4. Unit test: Cascaded timer fires at correct deadline
5. Integration test: Mix of L0 and L1 timers
6. Regression test: timer_tick_async still works
7. Regression test: All existing timer tests pass

## Performance Impact

- Poll: No change (still scan ~15K slots/sec in L0)
- Cascade: ~931 times/1000s, moving ~10-20 timers each time (negligible)
- Memory: 2x increase (0.77 MB vs 0.38 MB per worker)
- Cache locality: Better (two smaller wheels vs one large wheel)

## Backwards Compatibility

Public API remains unchanged:
- `new()`, `with_allocation()` - same signatures
- `schedule_timer()` - same signature
- `poll()` - same signature
- `cancel_timer()` - same signature (timer_id encoding changes internally)

No breaking changes for callers.
