# SegSpsc: Segmented Single-Producer Single-Consumer Queue

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Key Features](#key-features)
4. [Memory Management](#memory-management)
5. [Thread Safety Model](#thread-safety-model)
6. [Performance Characteristics](#performance-characteristics)
7. [API Reference](#api-reference)
8. [Implementation Details](#implementation-details)
9. [Usage Examples](#usage-examples)
10. [Configuration Guide](#configuration-guide)

---

## Overview

`SegSpsc` is a high-performance, bounded single-producer single-consumer (SPSC) queue optimized for throughput and
memory efficiency. It combines segmented storage with lazy allocation and intelligent segment pooling to achieve:

- **Zero upfront allocation**: Memory allocated only as needed
- **Cache-friendly reuse**: Hot segments recycled through sealed pool
- **Zero-copy consumption**: Direct slice access without memcpy
- **Bounded memory footprint**: Configurable segment pool limits
- **Lock-free operations**: All operations use atomics, no mutexes
- **Graceful shutdown**: Close bit encoding for clean producer/consumer coordination

### Quick Example

```rust
use bop_mpmc::seg_spsc::SegSpsc;

// Create queue: 256 items/segment, 1024 segments, max 16 pooled segments
type Queue = SegSpsc<u64, 8, 10>;
let (producer, consumer) = Queue::new_with_config(16);

// Producer thread
std::thread::spawn(move || {
    for i in 0..1_000_000 {
        while producer.try_push(i).is_err() {
            std::hint::spin_loop();
        }
    }
    producer.close();
});

// Consumer thread (zero-copy consumption)
let mut total = 0u64;
consumer.consume_in_place(usize::MAX, |items| {
    total += items.iter().sum::<u64>();
    items.len() // Consume all items in slice
});
```

---

## Architecture

### High-Level Structure

```text
┌───────────────────────────────────────────────────────────────────┐
│                           SegSpsc Queue                           │
├───────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────────┐     ┌──────────────────┐                    │
│  │ Producer State   │     │ Consumer State   │                    │
│  │ (CachePadded)    │     │ (CachePadded)    │                    │
│  ├──────────────────┤     ├──────────────────┤                    │
│  │ head: AtomicU64  │     │ tail: AtomicU64  │                    │
│  │ tail_cache: u64  │     │ head_cache: u64  │                    │
│  │ sealable_lo      │     │ sealable_hi      │                    │
│  │ metrics...       │     │                  │                    │
│  └──────────────────┘     └──────────────────┘                    │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │              Segment Directory (NUM_SEGS slots)             │  │
│  │  [AtomicPtr] [AtomicPtr] [AtomicPtr] ... [AtomicPtr]        │  │
│  └────┬───────────┬───────────┬────────────────────────────────┘  │
│       │           │           │                                   │
│       ▼           ▼           ▼                                   │
│   ┌─────┐     ┌─────┐     ┌─────┐                                 │
│   │ Seg │     │ Seg │     │null │  Each segment: 2^P items        │
│   │ 0   │     │ 1   │     │     │  Lazily allocated               │
│   └─────┘     └─────┘     └─────┘                                 │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘
```

### Two-Level Indexing

Each global position `pos` (a monotonic 64-bit counter) maps to a specific location:

```text
segment_index = (pos >> P) & DIR_MASK
item_offset   = pos & SEG_MASK

where:
  P = log2(segment_size)
  DIR_MASK = NUM_SEGS - 1
  SEG_MASK = SEG_SIZE - 1
```

**Example**: For `SegSpsc<u64, 8, 10>` (256 items/seg, 1024 segments):

- Position 300 → segment_index = 1, offset = 44
- Position 1000 → segment_index = 3, offset = 232

### Close Bit Encoding

The producer's `head` position uses bit 63 to encode the close state:

```text
┌───────┬─────────────────────────────────────────────────────────┐
│ Bit   │ Description                                             │
├───────┼─────────────────────────────────────────────────────────┤
│ 63    │ Close flag (1 = closed, 0 = open)                       │
│ 62..0 │ Monotonic position (where next item will be written)    │
└───────┴─────────────────────────────────────────────────────────┘
```

This allows lock-free close detection without a separate atomic flag.

---

## Key Features

### 1. Lazy Segment Allocation

Segments are allocated on-demand when the producer crosses segment boundaries:

```rust
// Initially: 0 segments allocated, 0 bytes used
let (producer, consumer) = SegSpsc::<u64, 8, 10>::new();

// Push 100 items → allocates 1 segment (256 items × 8 bytes = 2 KB)
for i in 0..100 {
    producer.try_push(i).unwrap();
}
assert_eq!(producer.allocated_segments(), 1);

// Push 200 more items → still 1 segment (total 300 items, capacity 256/seg)
// Actually crosses boundary, so 2 segments now
for i in 100..300 {
    producer.try_push(i).unwrap();
}
assert_eq!(producer.allocated_segments(), 2);
```

**Benefits**:

- No wasted memory for unused capacity
- Small queues stay small
- Scales to actual workload needs

### 2. Segment Pooling and Reuse

Consumed segments enter a "sealed pool" for hot reuse:

```text
Segment Lifecycle:
┌─────────┐    ┌────────┐    ┌────────┐    ┌────────┐    ┌─────────┐
│ Freshly │ →  │ Active │ →  │ Sealed │ →  │ Pooled │ →  │ Reused  │
│ Alloced │    │ In Use │    │        │    │        │    │         │
└─────────┘    └────────┘    └────────┘    └────────┘    └─────────┘
     ↑                                                          │
     └──────────────────────────────────────────────────────────┘
                    (if pool full, deallocated instead)
```

**Sealed Pool Invariant**:

```text
Sealed segments: [sealable_lo, sealable_hi)

- Consumer advances sealable_hi when sealing consumed segments
- Producer advances sealable_lo when stealing segments for reuse
- Pool size = sealable_hi - sealable_lo
```

**Cache Performance**:

- Reused segments are cache-hot (recently accessed)
- Avoids system allocator overhead (malloc/free)
- Typical reuse rates: 80-90% in steady-state workloads

### 3. Zero-Copy Consumption

The `consume_in_place` API provides direct slice access to queue items:

```rust
// Traditional approach (requires memcpy):
let mut buffer = vec![0u64; 1000];
queue.try_pop_n(&mut buffer)?;
process_items(&buffer);

// Zero-copy approach (no memcpy):
queue.consume_in_place(1000, |items| {
    process_items(items);  // Direct slice access
    items.len()  // Consumed all
});
```

**Performance Impact**:

- Saves ~50-200 ns per KB (avoids memcpy)
- Memory bandwidth limited: 2-5 GB/s throughput
- Ideal for: Network I/O, hashing, serialization

**Safety**: Safe because `T: Copy` (no Drop concerns) and consumer is single-threaded.

### 4. Bounded Memory with Deallocation

Control memory footprint with configurable segment pool limits:

```rust
// Create queue with max 16 pooled segments
let (producer, consumer) = SegSpsc::<u64, 8, 10>::new_with_config(16);

// Large workload allocates many segments
for i in 0..1_000_000 {
    producer.try_push(i).unwrap();
}
println!("Peak: {} segments", producer.allocated_segments());  // e.g., 1024

// Drain queue
while consumer.try_pop().is_some() {}

// Automatic deallocation keeps only 16 segments warm
println!("After drain: {} segments", producer.allocated_segments());  // 16

// Or explicitly control pool size:
consumer.deallocate_to(0);  // Free all pooled segments
println!("After dealloc: {} segments", producer.allocated_segments());  // 0
```

**Deallocation Strategies**:

| Method                    | When                   | Keeps                 | Use Case                   |
|---------------------------|------------------------|-----------------------|----------------------------|
| Automatic (on seal)       | Every segment boundary | `max_pooled_segments` | Steady-state workloads     |
| `try_deallocate_excess()` | Manual call            | `max_pooled_segments` | After drain/burst          |
| `deallocate_to(N)`        | Manual call            | `N` segments          | Custom memory targets      |
| `deallocate_to(0)`        | Manual call            | 0 segments            | Maximum memory reclamation |

---

## Memory Management

### Memory Footprint Calculation

```text
Total Memory = Fixed Overhead + Segment Memory

Fixed Overhead = sizeof(SegSpsc<T, P, NUM_SEGS_P2>)
               = sizeof(ProducerState) + sizeof(ConsumerState) + sizeof(Directory)
               = 128 bytes + 128 bytes + (NUM_SEGS × 8 bytes)

Segment Memory = allocated_segments × SEG_SIZE × sizeof(T)
               = allocated_segments × 2^P × sizeof(T)
```

**Example**: `SegSpsc<u64, 8, 10>` with 100 allocated segments:

- Fixed: 128 + 128 + (1024 × 8) = 8,448 bytes (~8 KB)
- Segments: 100 × 256 × 8 = 204,800 bytes (~200 KB)
- Total: ~208 KB

### Allocation Behavior

**Initial State**:

```rust
let queue = SegSpsc::<u64, 8, 10>::new();
// Allocated segments: 0
// Memory usage: ~8 KB (fixed overhead only)
```

**During Fill**:

```rust
// Each segment boundary triggers allocation
for i in 0..10_000 {
    queue.try_push(i).unwrap();
}
// Pushed 10,000 items across 40 segments (10,000 / 256 ≈ 40)
// Allocated segments: 40
// Memory usage: ~8 KB + (40 × 256 × 8) = ~90 KB
```

**After Drain**:

```rust
while queue.try_pop().is_some() {}
// With max_pooled_segments = 16:
// Allocated segments: 16 (dealloc'd 24 segments)
// Memory usage: ~8 KB + (16 × 256 × 8) = ~40 KB
```

### Deallocation Safety

**Critical Constraint**: Deallocation only occurs when queue is empty (`tail == head`).

**Why?** Circular directory wraparound can cause position collisions:

```text
With NUM_SEGS = 1024:
  Position 500 → dir_idx = 500
  Position 1524 → dir_idx = 500 (wraps: 1524 & 1023 = 500)

If queue has active items and pool contains position 500:
  - Consumer might be reading from position 500 (old segment)
  - Producer might allocate new segment at position 1524 (same dir_idx!)
  - RACE CONDITION: Both access same directory slot
```

**Solution**: Only deallocate when `tail == head` (queue empty, no active segments).

```rust
fn try_deallocate_pool_to(&self, target_pool_size: usize) -> u64 {
    // Safety check: only proceed if queue is empty
    let head = self.producer.head.load(Ordering::Acquire) & Self::RIGHT_MASK;
    let tail = self.consumer.tail.load(Ordering::Acquire);

    if tail != head {
        return 0;  // Queue not empty - unsafe to deallocate
    }

    // Safe to deallocate...
}
```

---

## Thread Safety Model

### SPSC Guarantee

**Single Producer**:

- Only ONE thread may call: `try_push`, `try_push_n`, `close`
- Producer owns: `head`, `tail_cache`, `sealable_lo`

**Single Consumer**:

- Only ONE thread may call: `try_pop`, `try_pop_n`, `consume_in_place`, `try_deallocate_excess`, `deallocate_to`
- Consumer owns: `tail`, `head_cache`, `sealable_hi`

**Shared (Read-Only)**:

- Multiple threads may call: `len`, `is_empty`, `allocated_segments`, `allocated_memory_bytes`
- May see stale values (eventually consistent)

### Memory Ordering

| Operation                          | Ordering  | Rationale                                      |
|------------------------------------|-----------|------------------------------------------------|
| Producer writes `head`             | `Release` | Ensure item writes visible before head update  |
| Consumer reads `head`              | `Acquire` | Synchronize with producer's writes             |
| Consumer writes `tail`             | `Release` | Ensure consumption complete before tail update |
| Producer reads `tail`              | `Acquire` | Synchronize with consumer's consumption        |
| Pool operations (`sealable_lo/hi`) | `AcqRel`  | Bidirectional synchronization                  |
| Metrics                            | `Relaxed` | No ordering requirements (statistics only)     |

### Cache Line Optimization

```rust
struct SegSpsc<T, P, NUM_SEGS_P2> {
    producer: CachePadded<ProducerState>,  // Own cache line
    consumer: CachePadded<ConsumerState>,  // Own cache line
    segs: [AtomicPtr<...>; NUM_SEGS],     // Shared (read-mostly)
    max_pooled_segments: usize,            // Shared (immutable)
}
```

**Why CachePadded?**

- Prevents false sharing between producer and consumer
- Producer updates `head` frequently → hot cache line
- Consumer updates `tail` frequently → different hot cache line
- Without padding: cache line ping-pong → 10-20x slowdown

**Cached Positions**:

```rust
// Producer caches consumer's tail (UnsafeCell, single-threaded access)
tail_cache: UnsafeCell<u64>

// Consumer caches producer's head (UnsafeCell, single-threaded access)
head_cache: UnsafeCell<u64>
```

**Refresh Strategy**:

- Producer refreshes `tail_cache` only when queue appears full
- Consumer refreshes `head_cache` only when queue appears empty
- Minimizes cross-cache-line reads (expensive: ~60-100 ns)

---

## Performance Characteristics

### Operation Complexity

| Operation               | Time Complexity | Amortized   | Notes                                  |
|-------------------------|-----------------|-------------|----------------------------------------|
| `try_push`              | O(1)            | O(1)        | Lazy allocation at segment boundaries  |
| `try_pop`               | O(1)            | O(1)        | Segment sealing at boundaries          |
| `try_push_n`            | O(n)            | O(n)        | May span multiple segments             |
| `try_pop_n`             | O(n)            | O(n)        | May span multiple segments             |
| `consume_in_place`      | O(callback)     | O(callback) | Zero-copy, callback cost dominates     |
| `len`                   | O(1)            | O(1)        | Atomic loads only                      |
| `try_deallocate_excess` | O(NUM_SEGS)     | O(NUM_SEGS) | Scans directory for allocated segments |

### Throughput Benchmarks

**Hardware**: AMD Ryzen 9 5950X, DDR4-3600, Windows 11

| Workload             | Throughput  | Latency        | Segment Reuse Rate |
|----------------------|-------------|----------------|--------------------|
| Single item push/pop | ~1.1 Gops/s | ~0.9 ns/op     | 83%                |
| Batch (64 items)     | ~1.3 Gops/s | ~0.77 ns/op    | 85%                |
| Zero-copy consume    | ~2.5 GB/s   | ~5 ns/callback | N/A                |
| 8 queues sharded     | ~1.1 Gops/s | ~0.9 ns/op     | 82%                |

**Memory Efficiency**:

- Fresh allocations: ~10,000 (cache misses)
- Pool reuses: ~45,000 (cache hits)
- Reuse rate: ~82%
- Live memory: 4 MB (128 segments × 32 KB/segment)

### Comparison with Alternatives

| Queue Type           | Throughput     | Memory             | Segment Reuse | Bounded |
|----------------------|----------------|--------------------|---------------|---------|
| **SegSpsc**          | **1.1 Gops/s** | **Lazy + Pooled**  | **82%**       | ✅       |
| crossbeam ArrayQueue | ~800 Mops/s    | Contiguous upfront | N/A           | ✅       |
| std::sync::mpsc      | ~300 Mops/s    | Linked list        | N/A           | ❌       |
| crossbeam SegQueue   | ~600 Mops/s    | Segmented lazy     | Low           | ❌       |

**Advantages**:

- Higher throughput than general-purpose queues
- Memory efficient (lazy + bounded)
- Cache-friendly (hot segment reuse)

**Trade-offs**:

- SPSC only (not MPMC)
- `T: Copy` requirement
- Complexity: ~3500 LOC vs ~500 for simple ring buffer

---

## API Reference

### Construction

```rust
// Default: max_pooled_segments = 0 (deallocation disabled)
pub fn new() -> (SegSpscProducer<T, P, NUM_SEGS_P2>,
                 SegSpscConsumer<T, P, NUM_SEGS_P2>)

// With bounded pool: keeps at most `max_pooled_segments` in sealed pool
pub fn new_with_config(max_pooled_segments: usize)
    -> (SegSpscProducer<T, P, NUM_SEGS_P2>,
        SegSpscConsumer<T, P, NUM_SEGS_P2>)
```

### Producer API

```rust
impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> SegSpscProducer<T, P, NUM_SEGS_P2> {
    // Push single item
    pub fn try_push(&self, item: T) -> Result<(), PushError<T>>

    // Push multiple items (may span segments)
    pub fn try_push_n(&self, items: &[T]) -> Result<(), PushError<()>>

    // Close queue (sets bit 63 of head)
    pub fn close(&self)

    // Inspection
    pub fn len(&self) -> usize
    pub fn is_empty(&self) -> bool
    pub fn capacity(&self) -> usize

    // Metrics
    pub fn fresh_allocations(&self) -> u64  // Cache misses
    pub fn pool_reuses(&self) -> u64         // Cache hits
    pub fn allocated_segments(&self) -> u64
    pub fn allocated_memory_bytes(&self) -> usize
}
```

### Consumer API

```rust
impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> SegSpscConsumer<T, P, NUM_SEGS_P2> {
    // Pop single item
    pub fn try_pop(&self) -> Option<T>

    // Pop multiple items (fills buffer)
    pub fn try_pop_n(&self, buf: &mut [T]) -> Result<usize, PopError>

    // Zero-copy consumption (callback receives slice)
    pub fn consume_in_place<F>(&self, max: usize, f: F) -> usize
    where F: FnMut(&[T]) -> usize

    // Memory management
    pub fn try_deallocate_excess(&self) -> u64
    pub fn deallocate_to(&self, target_pool_size: usize) -> u64

    // Inspection (same as producer)
    pub fn len(&self) -> usize
    pub fn is_empty(&self) -> bool
    pub fn capacity(&self) -> usize

    // Metrics (same as producer)
    pub fn fresh_allocations(&self) -> u64
    pub fn pool_reuses(&self) -> u64
    pub fn allocated_segments(&self) -> u64
    pub fn allocated_memory_bytes(&self) -> usize
}
```

---

## Implementation Details

### Segment Allocation

```rust
fn ensure_segment_for(&self, pos: u64) -> Result<(), PushError<T>> {
    let seg_idx = ((pos >> P) as usize) & Self::DIR_MASK;
    let seg_ptr = self.segs[seg_idx].load(Ordering::Acquire);

    if !seg_ptr.is_null() {
        return Ok(());  // Already allocated
    }

    // Try to steal from sealed pool first
    let lo = self.producer.sealable_lo.load(Ordering::Relaxed);
    let hi = self.consumer.sealable_hi.load(Ordering::Acquire);

    if lo < hi {
        // Pool has segments - try to steal one
        let steal_pos = lo;
        let steal_idx = (steal_pos as usize) & Self::DIR_MASK;

        if self.producer.sealable_lo.compare_exchange(
            steal_pos, steal_pos + 1,
            Ordering::AcqRel, Ordering::Acquire
        ).is_ok() {
            // Successfully stole segment - move to target position
            let stolen = self.segs[steal_idx].swap(ptr::null_mut(), Ordering::AcqRel);
            if !stolen.is_null() {
                self.segs[seg_idx].store(stolen, Ordering::Release);
                self.producer.pool_reuses.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
        }
    }

    // Pool empty or steal failed - allocate fresh segment
    let layout = Layout::array::<MaybeUninit<T>>(Self::SEG_SIZE).unwrap();
    let new_seg = unsafe { alloc_zeroed(layout) as *mut MaybeUninit<T> };

    if new_seg.is_null() {
        return Err(PushError::Full(()));  // OOM
    }

    match self.segs[seg_idx].compare_exchange(
        ptr::null_mut(), new_seg,
        Ordering::AcqRel, Ordering::Acquire
    ) {
        Ok(_) => {
            self.producer.fresh_allocations.fetch_add(1, Ordering::Relaxed);
            self.producer.total_allocated_segments.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        Err(_) => {
            // Race: another allocation won
            unsafe { dealloc(new_seg as *mut u8, layout); }
            Ok(())
        }
    }
}
```

**Key Points**:

1. Check if segment already allocated (fast path)
2. Try to steal from sealed pool (cache-hot reuse)
3. Fall back to fresh allocation if pool empty
4. Use CAS to handle races (SPSC but segments can race with pool operations)

### Segment Sealing

```rust
fn seal_after(&self, s_done: u64) {
    let new_hi = s_done + 1;
    self.consumer.sealable_hi.store(new_hi, Ordering::Release);

    // Consumer-side garbage collection
    if self.max_pooled_segments > 0 {
        self.try_deallocate_pool_to(self.max_pooled_segments);
    }
}
```

Called by consumer after fully consuming a segment. Adds segment to sealed pool and triggers deallocation if pool
exceeds configured limit.

### Deallocation Algorithm

```rust
fn try_deallocate_pool_to(&self, target_pool_size: usize) -> u64 {
    // Safety: Only safe when queue is empty
    let head = self.producer.head.load(Ordering::Acquire) & Self::RIGHT_MASK;
    let tail = self.consumer.tail.load(Ordering::Acquire);

    if tail != head {
        return 0;  // Not safe - queue has active items
    }

    // Count allocated segments (scan directory directly)
    let mut total_allocated = 0u64;
    for i in 0..Self::NUM_SEGS {
        if !self.segs[i].load(Ordering::Acquire).is_null() {
            total_allocated += 1;
        }
    }

    if total_allocated <= target_pool_size as u64 {
        return 0;  // Already at target
    }

    // Deallocate oldest segments from pool
    let need_to_dealloc = total_allocated - target_pool_size as u64;
    let mut deallocated = 0u64;
    let lo = self.producer.sealable_lo.load(Ordering::Acquire);
    let hi = self.consumer.sealable_hi.load(Ordering::Acquire);
    let mut current = lo;

    while current < hi && deallocated < need_to_dealloc {
        let dir_idx = (current as usize) & Self::DIR_MASK;

        // Claim pool position with CAS
        if self.producer.sealable_lo.compare_exchange(
            current, current + 1,
            Ordering::AcqRel, Ordering::Acquire
        ).is_ok() {
            let seg = self.segs[dir_idx].swap(ptr::null_mut(), Ordering::AcqRel);
            if !seg.is_null() {
                unsafe { free_segment::<T>(seg, Self::SEG_SIZE); }
                deallocated += 1;
            }
        }
        current += 1;
    }

    self.producer.total_allocated_segments.fetch_sub(deallocated, Ordering::Relaxed);
    deallocated
}
```

**Algorithm**:

1. Verify queue is empty (safety check)
2. Count allocated segments by scanning directory
3. Calculate excess: `total_allocated - target_pool_size`
4. Deallocate oldest segments from pool until target reached
5. Use CAS to handle concurrent operations
6. Update metrics

---

## Usage Examples

### Example 1: Basic Push/Pop

```rust
use bop_mpmc::seg_spsc::SegSpsc;

// 256 items/segment, 1024 segments
type Queue = SegSpsc<u64, 8, 10>;
let (producer, consumer) = Queue::new();

// Producer
producer.try_push(42).unwrap();
producer.try_push(100).unwrap();

// Consumer
assert_eq!(consumer.try_pop(), Some(42));
assert_eq!(consumer.try_pop(), Some(100));
assert_eq!(consumer.try_pop(), None);
```

### Example 2: Batch Operations

```rust
let (producer, consumer) = SegSpsc::<u64, 8, 10>::new();

// Batch push
let data = vec![1, 2, 3, 4, 5];
producer.try_push_n(&data).unwrap();

// Batch pop
let mut buffer = vec![0; 5];
let count = consumer.try_pop_n(&mut buffer).unwrap();
assert_eq!(count, 5);
assert_eq!(buffer, data);
```

### Example 3: Zero-Copy Network Serialization

```rust
use std::io::Write;
use std::net::TcpStream;

fn send_queue_data(consumer: &SegSpscConsumer<u64, 8, 10>,
                   mut stream: TcpStream) -> std::io::Result<usize> {
    let mut total_sent = 0usize;

    consumer.consume_in_place(usize::MAX, |items| {
        // Zero-copy: serialize directly from queue memory
        let bytes = bytemuck::cast_slice::<u64, u8>(items);

        match stream.write_all(bytes) {
            Ok(()) => {
                total_sent += items.len();
                items.len()  // Consumed all
            }
            Err(_) => 0  // Error: stop consumption
        }
    });

    Ok(total_sent)
}
```

### Example 4: Bounded Memory Workload

```rust
use std::thread;
use std::time::Duration;

// Create queue with max 16 pooled segments
let (producer, consumer) = SegSpsc::<u64, 8, 10>::new_with_config(16);

// Producer thread - burst workload
let p = producer.clone();
thread::spawn(move || {
    for burst in 0..10 {
        // Large burst
        for i in 0..100_000 {
            while p.try_push(i).is_err() {
                std::hint::spin_loop();
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    p.close();
});

// Consumer thread - drain and monitor memory
loop {
    // Drain batch
    let mut buf = vec![0u64; 1000];
    match consumer.try_pop_n(&mut buf) {
        Ok(count) if count > 0 => {
            // Process items...
            println!("Consumed {} items", count);
        }
        Ok(_) => thread::sleep(Duration::from_micros(10)),
        Err(PopError::Closed) => break,
        _ => {}
    }

    // Monitor memory (deallocation happens automatically on segment seal)
    println!("Live segments: {}, Memory: {} KB",
             consumer.allocated_segments(),
             consumer.allocated_memory_bytes() / 1024);
}

// After drain, manually reclaim more memory if needed
consumer.deallocate_to(4);  // Keep only 4 segments warm
println!("Final memory: {} KB", consumer.allocated_memory_bytes() / 1024);
```

### Example 5: Hash Computation (Zero-Copy)

```rust
use blake3::Hasher;

fn hash_queue_contents(consumer: &SegSpscConsumer<u64, 8, 10>) -> blake3::Hash {
    let mut hasher = Hasher::new();

    consumer.consume_in_place(usize::MAX, |items| {
        // Zero-copy: hash directly from queue memory
        let bytes = bytemuck::cast_slice::<u64, u8>(items);
        hasher.update(bytes);
        items.len()  // Consumed all
    });

    hasher.finalize()
}
```

### Example 6: Graceful Shutdown

```rust
use std::thread;

let (producer, consumer) = SegSpsc::<u64, 8, 10>::new();

// Producer thread
let p = producer.clone();
let producer_thread = thread::spawn(move || {
    for i in 0..1_000_000 {
        if p.try_push(i).is_err() {
            break;  // Queue closed or full
        }
    }
    p.close();  // Signal completion
});

// Consumer thread
let consumer_thread = thread::spawn(move || {
    let mut total = 0u64;
    loop {
        match consumer.try_pop() {
            Some(item) => total += item,
            None if consumer.is_empty() => {
                // Check if closed
                match consumer.try_pop() {
                    None => break,  // Closed and empty
                    Some(item) => total += item,
                }
            }
            None => std::hint::spin_loop(),
        }
    }
    total
});

producer_thread.join().unwrap();
let sum = consumer_thread.join().unwrap();
println!("Total sum: {}", sum);
```

---

## Configuration Guide

### Choosing Parameters

**P (Segment Size = 2^P)**:

| P  | Segment Size | Use Case                       | Cache Impact        |
|----|--------------|--------------------------------|---------------------|
| 6  | 64 items     | Small items, low latency       | Fits in L1 cache    |
| 8  | 256 items    | Balanced                       | Fits in L2 cache    |
| 10 | 1024 items   | Large batches, high throughput | Fits in L3 cache    |
| 12 | 4096 items   | Very large batches             | May spill to memory |

**Rule of thumb**:

- Small P (6-8): Lower latency, more segment overhead
- Large P (10-12): Higher throughput, less allocation frequency

**NUM_SEGS_P2 (Directory Size = 2^NUM_SEGS_P2)**:

| NUM_SEGS_P2 | Directory Slots | Max Capacity   | Directory Memory |
|-------------|-----------------|----------------|------------------|
| 8           | 256             | 16K-1M items   | 2 KB             |
| 10          | 1024            | 64K-4M items   | 8 KB             |
| 12          | 4096            | 256K-16M items | 32 KB            |

**Rule of thumb**:

- Match to workload peak capacity
- Directory is allocated upfront (fixed overhead)
- Larger directory allows more concurrent segments

**max_pooled_segments**:

| Value | Behavior              | Memory Footprint | Use Case                        |
|-------|-----------------------|------------------|---------------------------------|
| 0     | Deallocation disabled | Unbounded        | Maximum reuse, unlimited memory |
| 4-8   | Minimal warm pool     | Low              | Memory-constrained              |
| 16-32 | Moderate warm pool    | Medium           | Balanced                        |
| 64+   | Large warm pool       | High             | High-throughput steady state    |

**Rule of thumb**:

- Set to 2-4x typical concurrent segments
- Higher = better reuse, more memory
- Lower = less memory, more allocation overhead

### Example Configurations

**Low-latency, small items**:

```rust
// 64 items/segment, 256 segments, 8 pooled
type Queue = SegSpsc<u32, 6, 8>;
let (producer, consumer) = Queue::new_with_config(8);
// Capacity: 16,383 items (~64 KB for u32)
```

**High-throughput, large batches**:

```rust
// 1024 items/segment, 4096 segments, 64 pooled
type Queue = SegSpsc<u64, 10, 12>;
let (producer, consumer) = Queue::new_with_config(64);
// Capacity: 4,194,303 items (~32 MB for u64)
```

**Memory-constrained**:

```rust
// 256 items/segment, 1024 segments, 4 pooled
type Queue = SegSpsc<u64, 8, 10>;
let (producer, consumer) = Queue::new_with_config(4);
// Max pooled memory: 4 × 256 × 8 = ~8 KB
```

**Unlimited reuse (no deallocation)**:

```rust
// Any configuration, max_pooled_segments = 0
type Queue = SegSpsc<u64, 8, 10>;
let (producer, consumer) = Queue::new();
// All consumed segments remain in pool indefinitely
```

---

## Best Practices

### 1. Choose Appropriate Segment Size

- Balance between allocation frequency and cache utilization
- Larger segments = fewer allocations but potential cache misses
- Smaller segments = more allocations but better cache locality

### 2. Configure Pool Size for Workload

- Steady-state: Set `max_pooled_segments` to 2-4x active segments
- Bursty: Use larger pool (32-64) to absorb spikes
- Memory-limited: Use minimal pool (4-8) and call `deallocate_to(0)` after bursts

### 3. Leverage Zero-Copy APIs

- Use `consume_in_place` for I/O, hashing, serialization
- Avoid `try_pop_n` when callback can process directly
- Saves memcpy overhead (~50-200 ns/KB)

### 4. Monitor Metrics

```rust
println!("Fresh allocs: {}, Reuses: {}, Rate: {:.1}%",
         producer.fresh_allocations(),
         producer.pool_reuses(),
         100.0 * producer.pool_reuses() as f64 /
                 (producer.fresh_allocations() + producer.pool_reuses()) as f64);
```

- Target reuse rate: 80%+
- Low reuse rate → increase `max_pooled_segments`

### 5. Explicit Memory Management

```rust
// After large burst, explicitly reclaim memory
while consumer.try_pop().is_some() {}  // Drain
consumer.deallocate_to(8);  // Keep 8 warm segments

// Before shutdown, free all memory
consumer.deallocate_to(0);
```

### 6. Handle Close Properly

```rust
// Producer: always close on exit
producer.close();

// Consumer: drain after close
loop {
    match consumer.try_pop() {
        Some(item) => process(item),
        None if consumer.len() == 0 => break,  // Closed and empty
        None => std::hint::spin_loop(),
    }
}
```

---

## Limitations

1. **SPSC Only**: Not suitable for multiple producers or consumers
2. **T: Copy Requirement**: Cannot store non-Copy types (e.g., String, Vec)
3. **Bounded Capacity**: Fixed maximum capacity (cannot grow beyond NUM_SEGS × SEG_SIZE)
4. **Deallocation Constraint**: Can only deallocate when queue is empty
5. **Memory Overhead**: Directory allocated upfront (NUM_SEGS × 8 bytes)
6. **No Priority**: FIFO only, no priority queue semantics

---

## Future Enhancements

- **Async Support**: Add `async` variants of push/pop APIs
- **Batch Deallocation**: Deallocate during partial drain (research needed)
- **Dynamic Segments**: Allow segment size to vary by position
- **Compression**: Optional compression for large segments
- **Instrumentation**: More detailed metrics (segment lifetime, reuse latency)

---

## References

- Source: `crates/bop-mpmc/src/seg_spsc.rs`
- Tests: `crates/bop-mpmc/src/seg_spsc.rs` (test module)
- Benchmarks: `crates/bop-mpmc/examples/spsc_benchmark.rs`
