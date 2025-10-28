# Task Signaling Architecture

## Overview

The BOP executor uses a hierarchical signaling system to coordinate task scheduling and worker wakeups across multiple threads. This document describes how the three core components—**Task**, **SummaryTree**, and **SignalWaker**—work together to ensure efficient work distribution with no lost wakeups.

## Architecture Components

### 1. Task (Producer)

Tasks are the **sole producers** of work signals. When a task becomes runnable, it signals its availability through the system.

**Key Responsibilities:**
- Set signal bits when transitioning to SCHEDULED state
- Trigger SummaryTree updates
- Never directly wake workers

**Signal Points:**

```rust
// Task::schedule() - Initial scheduling
let (was_empty, was_set) = signal.set(self.signal_bit);
if was_set {
    arena.active_tree.mark_signal_active(leaf_idx, signal_idx);
}

// Task::finish() - Reschedule if woken during execution
let after_flags = self.state.fetch_sub(TASK_EXECUTING, ...);
if after_flags & TASK_SCHEDULED != TASK_IDLE {
    let (was_empty, was_set) = signal.set(self.signal_bit);
    if was_set {
        arena.active_tree.mark_signal_active(leaf_idx, signal_idx);
    }
}

// Task::finish_and_schedule() - Explicit reschedule
signal.set(self.signal_bit);
arena.active_tree.mark_signal_active(leaf_idx, signal_idx);
```

### 2. SummaryTree (Coordinator)

The SummaryTree is the **single source of truth** for work availability and worker wakeups. It maintains a two-level hierarchy for efficient work discovery.

**Structure:**

```
Root Words (64 bits each)
    ├─ Bit 0 → Leaf 0 has tasks
    ├─ Bit 1 → Leaf 1 has tasks
    └─ Bit N → Leaf N has tasks
         ↓
Leaf Words (64 bits each)
    ├─ Bit 0 → Signal word 0 has tasks
    ├─ Bit 1 → Signal word 1 has tasks
    └─ Bit M → Signal word M has tasks
         ↓
Task Signal Words (64 bits each)
    └─ Each bit = individual task runnable
```

**Key Responsibilities:**
- Maintain hierarchical summary of active tasks
- **Issue permits** when tasks become active
- Wake sleeping workers via condvar
- Track sleeper count for targeted wakeups

**Critical Flow:**

```rust
pub fn mark_signal_active(&self, leaf_idx: usize, signal_idx: usize) -> bool {
    let mask = 1u64 << signal_idx;
    self.mark_leaf_bits(leaf_idx, mask)
}

fn mark_leaf_bits(&self, leaf_idx: usize, mask: u64) -> bool {
    let leaf = self.leaf_word(leaf_idx);
    let prev = leaf.fetch_or(mask, Ordering::AcqRel);
    
    // If leaf was empty, activate root and RELEASE PERMIT
    if prev & self.leaf_summary_mask == 0 {
        self.activate_root(leaf_idx);  // ← Calls release(1)
        true
    } else {
        false
    }
}

fn activate_root(&self, leaf_idx: usize) {
    let root = self.root_word(word_idx);
    let prev = root.fetch_or(mask, Ordering::AcqRel);
    
    if prev & mask == 0 {
        self.release(1);  // ← SINGLE SOURCE OF PERMITS
    }
}

pub fn release(&self, n: usize) {
    self.permits.fetch_add(n as u64, Ordering::Release);
    let to_wake = n.min(self.sleepers.load(Ordering::Relaxed));
    for _ in 0..to_wake {
        self.cv.notify_one();  // Wake workers
    }
}
```

**Why This Design:**
- **No lost wakeups**: Permits accumulate even if no workers are sleeping
- **Targeted wakeups**: Only wake `min(permits, sleepers)` workers
- **Lock-free fast path**: Consumers use atomic `try_acquire()`

### 3. SignalWaker (Worker-Local State)

Each worker has a SignalWaker that tracks **worker-specific state**, not global task availability.

**Key Responsibilities:**
- Track worker's partition summary (which leafs in assigned partition have work)
- Manage worker-local yield queue status
- Provide parking/waking primitives for the worker
- **Synchronize partition state** before parking

**Status Bits:**

```rust
status: AtomicU64
├─ Bit 0: yield_bit (worker's local yield queue has tasks)
└─ Bit 1: task_bit  (worker's partition has tasks per partition_summary)
```

**Partition Summary Flow:**

```rust
// Before parking, worker syncs partition state
let has_work = waker.sync_partition_summary(
    partition_start,
    partition_end,
    &arena.active_tree().leaf_words,
);

pub fn sync_partition_summary(
    &self,
    partition_start: usize,
    partition_end: usize,
    leaf_words: &[AtomicU64],
) -> bool {
    let mut new_summary = 0u64;
    
    // Sample leaf words in our partition
    for leaf_idx in partition_start..partition_end {
        let leaf_value = leaf_words[leaf_idx].load(Ordering::Relaxed);
        if leaf_value != 0 {
            let bit_idx = leaf_idx - partition_start;
            new_summary |= 1u64 << bit_idx;
        }
    }
    
    let old_summary = self.partition_summary.swap(new_summary, ...);
    
    // Update status bit 1 based on partition state
    let had_work = old_summary != 0;
    let has_work = new_summary != 0;
    
    if has_work && !had_work {
        self.mark_tasks();  // Sets status bit 1, adds permit
    } else if !has_work && had_work {
        self.try_unmark_tasks();  // Clears status bit 1
    }
    
    has_work
}
```

## Complete Work Flow

### Task Becomes Runnable

```
1. External event (I/O, timer, user spawn)
   ↓
2. Task::schedule() called
   ↓
3. Task sets signal bit in task signal word
   ↓
4. Task calls SummaryTree::mark_signal_active(leaf_idx, signal_idx)
   ↓
5. SummaryTree updates leaf summary bit
   ↓
6. If leaf was empty:
   ├─ Update root summary bit
   └─ Call release(1) to add permit and wake worker
   ↓
7. Worker wakes up (if sleeping) or permit accumulates (if all busy)
```

### Worker Searches for Work

```
1. Worker scans its partition (partition_start..partition_end)
   ↓
2. For each leaf in partition:
   ├─ Check leaf summary in SummaryTree
   └─ If active, scan task signal words
   ↓
3. Found task signal bit set:
   ├─ Atomically acquire signal bit
   ├─ If signal word now empty, mark_signal_inactive()
   └─ Return TaskHandle
   ↓
4. Worker polls task
   ↓
5. Task finishes:
   ├─ If rescheduled during poll → Task::schedule() (goto step 3 above)
   └─ If not rescheduled → signal bit stays clear
```

### Worker Parks (No Work)

```
1. Worker exhausts local work
   ↓
2. Worker attempts to steal from other partitions
   ↓
3. No work found globally
   ↓
4. Sync partition summary from SummaryTree
   ├─ Sample leaf_words in worker's partition
   └─ Update SignalWaker::partition_summary
   ↓
5. Check status bits (yield_bit | task_bit)
   ├─ If set → Try local work again
   └─ If clear → Proceed to park
   ↓
6. Increment sleeper count
   ↓
7. Final check: try_acquire() from SummaryTree
   ├─ Success → Got permit, process work
   └─ Fail → Actually park on condvar
   ↓
8. When woken:
   ├─ try_acquire() succeeds (permit available)
   └─ Decrement sleeper count, resume work loop
```

## Critical Invariants

### ✅ Permits Flow (Correct)

```
Task becomes runnable
    ↓
SummaryTree::mark_signal_active()
    ↓
SummaryTree::release(1)  ← ONLY permit source
    ↓
Worker::try_acquire()    ← ONLY permit sink
```

### ❌ Anti-Patterns (What NOT to Do)

**Never release permits from workers:**

```rust
// ❌ WRONG - Worker manually releasing permits
fn try_partition_random(&mut self) -> bool {
    if self.process_leaf(leaf_idx) {
        self.service.wakers[self.worker_id].mark_tasks(); // ❌ BAD
        return true;
    }
}

// ✅ CORRECT - Let SummaryTree handle permits
fn try_partition_random(&mut self) -> bool {
    if self.process_leaf(leaf_idx) {
        return true;  // Task already signaled via SummaryTree
    }
}
```

**Never calculate backlog in workers:**

```rust
// ❌ WRONG - Worker guessing how many permits to add
fn maybe_release_for_backlog(&mut self, remaining: u64) {
    let backlog = remaining.count_ones();
    self.arena.active_tree().release(backlog);  // ❌ Double-counting
}

// ✅ CORRECT - Each task releases exactly 1 permit via SummaryTree
// No worker-side permit management needed
```

## Yield Queue Special Case

The yield queue is **worker-local** and doesn't go through SummaryTree.

**Flow:**

```rust
// Worker yields task for fairness
fn enqueue_yield(&mut self, handle: TaskHandle) {
    let was_empty = self.yield_queue.push(handle);
    if was_empty {
        self.service.wakers[self.worker_id].mark_yield();  // ✅ OK
    }
    // No permit release needed - worker keeps processing
}

// Worker tries local yield queue first
fn try_acquire_local_yield(&mut self) -> Option<TaskHandle> {
    let (item, was_last) = self.yield_queue.pop();
    if was_last {
        self.service.wakers[self.worker_id].try_unmark_yield();  // ✅ OK
    }
    item
}
```

**Why This is Safe:**
- Yield queue is a **work-preserving optimization**
- Task was already acquired and counted
- Worker just postpones execution for fairness
- No new permits needed

## Memory Ordering

### Task Signal Path
```
Task::schedule()
    signal.set() → Relaxed (just marking work)
    ↓
SummaryTree::mark_signal_active()
    leaf.fetch_or() → AcqRel (synchronization point)
    ↓
SummaryTree::activate_root()
    root.fetch_or() → AcqRel
    permits.fetch_add() → Release (publish to workers)
    ↓
Worker::try_acquire()
    permits.fetch_sub() → AcqRel (acquire task data)
```

### Partition Summary
```
SignalWaker::sync_partition_summary()
    leaf_words.load() → Relaxed (hint-based)
    partition_summary.swap() → Relaxed (hint-based)
    status.fetch_or() → Relaxed (hint-based)
```

Partition summary uses Relaxed ordering because:
- It's a **best-effort optimization** to avoid parking
- False positives/negatives are safe (worker will recheck)
- Actual synchronization happens via SummaryTree permits

## Performance Characteristics

### SummaryTree
- **mark_signal_active**: O(1) atomic operations
- **activate_root**: O(1) atomic + O(k) wakeups where k = min(permits, sleepers)
- **Worker scan**: O(partition_size) leaf checks, O(signals_per_leaf) signal checks

### SignalWaker
- **sync_partition_summary**: O(partition_size) relaxed loads
- **try_acquire**: O(1) atomic (lock-free)
- **acquire**: O(1) blocking (parks on condvar)

### Memory Efficiency
- Root words: `(leaf_count + 63) / 64` × 8 bytes
- Leaf words: `leaf_count` × 8 bytes  
- Task signal words: `leaf_count × signals_per_leaf` × 8 bytes
- SignalWaker: 6 cache lines per worker (384 bytes on 64-byte lines)

## Debugging Tips

### Check Permit Balance

```rust
// If workers are stuck, check:
let tree = arena.active_tree();
println!("Permits: {}", tree.permits());
println!("Sleepers: {}", tree.sleepers());
println!("Root summary: {:064b}", tree.snapshot_summary());

// Should see:
// - permits > 0 if work exists
// - permits == 0 if no work
// - sleepers == worker_count if all parked
```

### Check Partition Ownership

```rust
let owner = tree.compute_partition_owner(leaf_idx, worker_count);
let waker = &service.wakers[owner];
println!("Worker {} owns leaf {}", owner, leaf_idx);
println!("Partition summary: {:064b}", waker.partition_summary());
```

### Trace Signal Flow

```rust
// Add logging to track signal propagation
Task::schedule() → "Task scheduled at leaf={} signal={} bit={}"
SummaryTree::mark_signal_active() → "Leaf activated, releasing permit"
Worker::try_acquire() → "Worker {} acquired permit"
Worker::try_acquire_task() → "Acquired task from leaf={} signal={} bit={}"
```

## Summary

**Single Responsibility:**
- **Tasks** signal work availability
- **SummaryTree** coordinates wakeups and manages permits
- **SignalWaker** tracks worker-local state

**Permit Discipline:**
- Tasks produce permits (via SummaryTree)
- Workers consume permits
- No other permit creation allowed

**Synchronization:**
- Task → SummaryTree: AcqRel on summary updates
- SummaryTree → Worker: Release/Acquire on permits
- Worker → SignalWaker: Relaxed (hints only)

This architecture ensures **no lost wakeups**, **efficient work discovery**, and **minimal contention** across worker threads.
