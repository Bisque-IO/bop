# Preempted Task Scheduling and Work Stealing

## Design Decision: Preempted Tasks → Yield Queue

When a task is preempted (via signal/APC-based non-cooperative preemption), it's **immediately placed in the yield queue** for work-stealing.

## Why the Yield Queue?

### 1. Work Stealing Enabled
```rust
// After preemption:
task.mark_yielded();
task.record_yield();
self.enqueue_yield(handle);  // ← Into yield queue!
```

**Result**: Any worker can steal the preempted task, enabling dynamic load balancing.

### 2. Semantics Match

Preempted tasks are similar to cooperatively yielded tasks:
- ✅ Ready to run (not waiting for I/O)
- ✅ Need CPU time soon
- ✅ Should be scheduled fairly
- ✅ Are "hot" (actively computing)

### 3. Fair Scheduling

CPU-bound tasks that get preempted frequently will:
- Circulate through the yield queue
- Get stolen by less-loaded workers
- Naturally migrate to available cores
- Avoid monopolizing a single worker

### 4. Prevents Starvation

Without yield queue placement, preempted tasks would:
- ❌ Need to be re-signaled to run
- ❌ Compete in signal queue with newly-spawned tasks
- ❌ Risk getting starved by high task spawn rate
- ❌ Not benefit from work stealing

## Implementation

### When Task is Preempted

```rust
// In Worker::run() after generator pinning
if !task_ptr.is_null() {
    let task = unsafe { &*task_ptr };
    
    // 1. Pin generator to task (Switch mode)
    task.pin_generator(boxed_iter, GeneratorRunMode::Switch);
    
    // 2. Get task handle
    let handle = TaskHandle::from_task(task);
    
    // 3. Mark as yielded (stays in EXECUTING state)
    task.mark_yielded();
    task.record_yield();
    
    // 4. Enqueue to yield queue - ENABLES WORK STEALING!
    self.enqueue_yield(handle);
    
    // 5. Update stats
    self.stats.yielded_count += 1;
}
```

### When Task is Resumed

```rust
// Worker polls from yield queue
let handle = self.yield_queue.pop();
let task = handle.task();

if task.has_pinned_generator() {
    // Resume via pinned generator
    self.poll_task_with_pinned_generator(handle, task);
} else {
    // Normal poll
    self.poll_task(handle, task);
}
```

## Work Stealing Flow

### Scenario: CPU-Bound Task on Busy Worker

```
Worker 0: Processing task A (CPU-bound, long-running)
    ↓
Timer fires preemption for Worker 0
    ↓
Worker 0's generator is pinned to task A
    ↓
Task A is marked as yielded
    ↓
Task A is enqueued to Worker 0's yield queue
    ↓
Worker 1 has no work, attempts steal
    ↓
Worker 1 steals task A from Worker 0's yield queue
    ↓
Worker 1 resumes task A with its pinned generator
    ↓
Task A continues on Worker 1!
```

**Result**: CPU-bound tasks naturally migrate to less-loaded workers!

## Benefits

### 1. Dynamic Load Balancing
- CPU-bound tasks don't monopolize a single worker
- Preemption + work stealing = automatic migration
- No manual task affinity needed

### 2. Fair CPU Allocation
- All tasks get time slices
- CPU-bound tasks can't starve I/O-bound tasks
- Preemption ensures fairness within a worker
- Work stealing ensures fairness across workers

### 3. Optimal Throughput
- Workers stay busy (stealing preempted tasks when idle)
- No head-of-line blocking (preemption breaks up long tasks)
- Parallel execution of independent CPU-bound tasks

### 4. Simple Implementation
- Reuses existing yield queue infrastructure
- No special "preempted task queue" needed
- Works with existing work-stealing logic
- Consistent task state management

## Alternative Designs Considered

### ❌ Signal Queue (Re-signal the task)
```rust
// After preemption:
task.schedule();  // Back to signal queue
```

**Problems**:
- Competes with newly spawned tasks
- No work stealing (signal queue is per-worker-partition)
- Risk of starvation under high spawn rate
- Extra signaling overhead

### ❌ Special Preempted Task Queue
```rust
// New queue just for preempted tasks:
self.preempted_queue.push(handle);
```

**Problems**:
- Duplicates yield queue functionality
- More complex work-stealing logic
- Scheduling policy fragmentation
- Extra memory overhead

### ✅ Yield Queue (Chosen)
```rust
// Treat like cooperative yield:
self.enqueue_yield(handle);
```

**Advantages**:
- Reuses existing infrastructure
- Work stealing already implemented
- Consistent scheduling semantics
- Optimal for CPU-bound tasks

## Performance Characteristics

### Latency
- **Preemption to re-schedule**: O(1) - single enqueue operation
- **Work steal attempt**: O(1) - deque pop from random worker
- **Resume latency**: O(1) - direct generator resume

### Throughput
- **No contention**: Yield queue is work-stealable deque (lock-free)
- **Efficient migration**: Task only moves once when stolen
- **Minimal overhead**: Same cost as cooperative yield

### Fairness
- **Within worker**: Time slicing via preemption timer
- **Across workers**: Load balancing via work stealing
- **Global fairness**: Combination of both mechanisms

## Example Scenario

### Single CPU-Bound Task Monopolizing a Worker

**Before (No Preemption)**:
```
Worker 0: [==========================Task A==========================] (10s)
Worker 1: [idle......................................................] 
Worker 2: [idle......................................................] 
Worker 3: [idle......................................................] 

Result: 3 idle workers, 1 busy worker, poor CPU utilization
```

**After (Preemption + Work Stealing)**:
```
Worker 0: [Task A][Task B][Task C][Task D]...
Worker 1: [Task A][Task E][Task F]...
Worker 2: [Task A][Task G]...
Worker 3: [Task A][Task H]...

Where Task A is preempted and stolen multiple times

Result: All workers busy, fair CPU allocation, optimal throughput
```

## Integration with Generator Run Modes

### Switch Mode (Preempted Tasks)
1. Task is preempted → Generator pinned in Switch mode
2. Task marked as yielded → Enqueued to yield queue
3. Worker steals task → Resumes in Switch mode (complete interrupted poll)
4. Poll completes → Transition to Poll mode or completion

### Poll Mode (Explicit Pinning)
1. Task pins generator explicitly → Generator in Poll mode
2. Task continues polling in loop
3. If preempted while in Poll mode → Mode changes to Switch
4. Resume completes poll → Back to Poll mode if still pending

## Monitoring

Track preempted task behavior via worker stats:

```rust
struct WorkerStats {
    yielded_count: u64,  // Includes both cooperative yields AND preemptions
    // ... other stats
}
```

To distinguish preemptions from cooperative yields, check if task has a pinned generator:

```rust
if task.has_pinned_generator() && task.is_yielded() {
    // This is a preempted task in yield queue
}
```

## Conclusion

Placing preempted tasks in the yield queue is the **optimal scheduling policy** because:

1. ✅ **Enables work stealing** - critical for load balancing
2. ✅ **Fair scheduling** - treats CPU-bound tasks fairly
3. ✅ **Simple implementation** - reuses existing infrastructure
4. ✅ **Optimal performance** - no additional queues or complexity
5. ✅ **Natural migration** - hot tasks automatically load balance

This design ensures that non-cooperative preemption doesn't just enable fairness within a worker, but also enables **global fairness and optimal throughput across all workers**!

