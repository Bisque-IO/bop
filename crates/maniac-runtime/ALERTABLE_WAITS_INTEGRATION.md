# Alertable Waits Integration with WorkerWaker

## Problem Statement

The initial implementation of non-cooperative preemption on Windows had a **critical flaw**: the alertable wait was implemented using a separate `AlertableEvent` on the `Worker` struct, completely isolated from the `WorkerWaker`'s condvar-based parking mechanism.

### Why This Was Broken

```rust
// BROKEN: Worker had separate alertable event
struct Worker {
    // ...
    alertable_event: AlertableEvent,  // ❌ Separate from WorkerWaker!
}

// Worker would park on the separate event:
worker.alertable_event.wait_alertable(Some(timeout));  // ❌

// But task scheduling would signal the WorkerWaker:
worker.service.wakers[waker_id].release(1);  // ❌ Different mechanism!
```

**Result**: The worker would never wake up when tasks were scheduled, MPSC messages arrived, or the partition summary changed, because these all signaled the `WorkerWaker`, not the `alertable_event`.

## Solution: Integrate Alertable Waits into WorkerWaker

The fix was to move the alertable wait mechanism **directly into `WorkerWaker`**, so the same parking mechanism handles:

1. **Task scheduling** (signal queue)
2. **MPSC messages** (cross-worker communication)
3. **Partition summary updates** (work-stealing)
4. **Preemption APCs** (non-cooperative scheduling)

### Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                        WorkerWaker                           │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Permit System (Counting Semaphore)                    │ │
│  │  • permits: AtomicU64                                  │ │
│  │  • sleepers: AtomicUsize                               │ │
│  └────────────────────────────────────────────────────────┘ │
│                            │                                 │
│                            ▼                                 │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Platform-Specific Parking (cfg-gated)                 │ │
│  │                                                         │ │
│  │  Unix:                                                  │ │
│  │    Condvar (cv: Condvar)                                │ │
│  │    • cv.wait() / cv.wait_timeout()                      │ │
│  │    • cv.notify_one()                                    │ │
│  │                                                         │ │
│  │  Windows:                                               │ │
│  │    AlertableEvent (alertable_event: AlertableEvent)    │ │
│  │    • wait_alertable() - WaitForSingleObjectEx          │ │
│  │    • signal() - SetEvent                                │ │
│  │    • bAlertable=TRUE → APCs can execute!               │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  • mark_active() / release() → Signals via platform mech   │
│  • acquire() / acquire_timeout() → Parks via platform mech │
└──────────────────────────────────────────────────────────────┘
```

## Implementation Details

### 1. Platform-Specific Parking Mechanism

```rust
pub struct WorkerWaker {
    status: CachePadded<AtomicU64>,
    permits: CachePadded<AtomicU64>,
    sleepers: CachePadded<AtomicUsize>,
    // ... other fields ...
    m: Mutex<()>,
    
    // Unix: Standard condvar
    #[cfg(not(windows))]
    cv: Condvar,
    
    // Windows: Alertable event for APC support
    #[cfg(windows)]
    alertable_event: AlertableEvent,
}
```

### 2. Unified Release (Producer Side)

When work arrives (tasks, messages, etc.), `WorkerWaker::release()` signals the appropriate platform mechanism:

```rust
pub fn release(&self, n: usize) {
    if n == 0 { return; }
    
    // Add permits (same on all platforms)
    self.permits.fetch_add(n as u64, Ordering::Release);
    let to_wake = n.min(self.sleepers.load(Ordering::Relaxed));
    
    #[cfg(not(windows))]
    {
        // Unix: Notify condvar
        for _ in 0..to_wake {
            self.cv.notify_one();
        }
    }
    
    #[cfg(windows)]
    {
        // Windows: Signal alertable event
        // Each signal wakes one thread (auto-reset event)
        for _ in 0..to_wake {
            self.alertable_event.signal();  // SetEvent
        }
    }
}
```

### 3. Unified Acquire (Consumer Side)

Workers park using `WorkerWaker::acquire()` or `acquire_timeout()`, which automatically use alertable waits on Windows:

```rust
pub fn acquire(&self) {
    if self.try_acquire() { return; }
    
    let mut g = self.m.lock().expect("waker mutex poisoned");
    self.sleepers.fetch_add(1, Ordering::Relaxed);
    
    #[cfg(not(windows))]
    {
        loop {
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            g = self.cv.wait(g).expect("waker condvar wait poisoned");
        }
    }
    
    #[cfg(windows)]
    {
        loop {
            if self.try_acquire() {
                self.sleepers.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            // Release mutex during wait
            drop(g);
            // Alertable wait - allows APCs to execute!
            let _ = self.alertable_event.wait_alertable(None);
            // Reacquire mutex
            g = self.m.lock().expect("waker mutex poisoned");
        }
    }
}
```

### 4. Worker Code Simplification

The worker code is now **platform-agnostic** - it just calls `WorkerWaker` methods:

```rust
// Before (BROKEN):
#[cfg(not(windows))]
worker.service.wakers[waker_id].acquire_timeout(timeout);

#[cfg(windows)]
{
    // Separate mechanism - never wakes for task scheduling!
    worker.alertable_event.wait_alertable(Some(timeout));
    worker.service.wakers[waker_id].try_acquire();
}

// After (CORRECT):
// Same code on all platforms - alertable waits are internal to WorkerWaker
worker.service.wakers[waker_id].acquire_timeout(timeout);
```

## How Preemption Works with This Integration

### Unix (SIGALRM)

1. Timer thread sends `SIGALRM` to worker thread via `pthread_kill()`
2. Signal handler runs on worker's stack:
   - Loads generator scope from thread-local
   - Calls `scope.yield_(1)` to force immediate generator switch
3. Generator yields, returns to worker's outer loop
4. Worker pins generator to task, re-enqueues to yield queue
5. Worker creates new generator and continues

**No special `WorkerWaker` involvement** - signal handler directly yields generator.

### Windows (QueueUserAPC + Alertable Waits)

1. Timer thread calls `QueueUserAPC` to queue APC on worker thread
2. **Worker must be in alertable wait** for APC to execute
3. Worker parks via `WorkerWaker::acquire_timeout()`:
   - Internally calls `alertable_event.wait_alertable()`
   - Which calls `WaitForSingleObjectEx(handle, timeout, TRUE /* alertable */)`
4. APC callback executes on worker's stack:
   - Loads generator scope from atomic
   - Calls `scope.yield_(1)` to force immediate generator switch
5. `WaitForSingleObjectEx` returns `WAIT_IO_COMPLETION`
6. Worker checks permits, finds work or continues
7. Generator has yielded, returns to worker's outer loop
8. Worker pins generator to task, re-enqueues to yield queue
9. Worker creates new generator and continues

**Critical**: The `alertable_event` in `WorkerWaker` serves **dual purposes**:
- **Normal wakeups**: Signaled by `release()` when work arrives
- **APC execution**: Alertable wait allows preemption APCs to run

## Benefits of This Integration

### ✅ Unified Wakeup Mechanism

All wakeup sources go through the same path:

```
Task Scheduled ──┐
MPSC Message   ──┼──> WorkerWaker::release() ──> Platform-specific signal
Partition Work ──┤
Yield Queue    ──┘

                      ↓

Worker Wakes ────> WorkerWaker::acquire() ──> Platform-specific wait
                                                (alertable on Windows!)
```

### ✅ No Lost Wakeups

- Worker parks on the **same mechanism** that gets signaled for all work
- Task scheduling, messages, timers, preemption - all wake the worker correctly
- Permit-based semaphore ensures fairness

### ✅ Platform-Agnostic Worker Code

```rust
// No cfg gates needed in worker loop!
self.service.wakers[waker_id].acquire_timeout(timeout);
```

### ✅ Preemption Integration

On Windows, every worker park is **automatically alertable**:
- No special "enter alertable state" API needed
- Preemption works everywhere the worker parks
- Same parking mechanism for all scenarios

## Comparison: Before vs After

### Before (Broken)

```
┌──────────────────┐     ┌──────────────────┐
│   WorkerWaker    │     │  AlertableEvent  │
│                  │     │   (on Worker)    │
│  • Signaled by   │     │  • Used for      │
│    task schedule │     │    parking ONLY  │
│  • Condvar/permit│     │  • Never signaled│
│  • Worker never  │     │    for tasks!    │
│    uses this!    │     │                  │
└──────────────────┘     └──────────────────┘
         ❌                        ❌
    Never wakes!            Wrong mechanism!
```

### After (Correct)

```
┌─────────────────────────────────────────────┐
│            WorkerWaker                      │
│                                             │
│  • Unix: Condvar for parking                │
│  • Windows: AlertableEvent for parking      │
│  • Signaled by: tasks, messages, timers     │
│  • Used by: Worker for ALL parking          │
│  • Alertable on Windows: APCs can execute   │
│                                             │
│           ✅ Single Source of Truth          │
└─────────────────────────────────────────────┘
```

## Testing

To verify this integration works:

1. **Task Scheduling**: Spawn tasks - worker should wake and execute them
2. **MPSC Messages**: Send cross-worker messages - receiver should wake
3. **Preemption**: Run CPU-bound task - timer should interrupt and migrate task
4. **Work Stealing**: Preempted task should be stealable by other workers

Example:
```rust
// Preemption test
executor.spawn(async {
    loop {
        // CPU-bound work
        for _ in 0..1_000_000 { /* compute */ }
    }
});

// Timer thread periodically calls:
service.interrupt_worker(worker_id);

// Expected: Task gets preempted, re-enqueued to yield queue,
//           other workers can steal it, and worker can handle
//           NEW tasks immediately (not blocked on CPU-bound task)
```

## Conclusion

Integrating alertable waits directly into `WorkerWaker` solved the critical bug where workers wouldn't wake for task scheduling on Windows. Now:

- ✅ Single unified parking mechanism
- ✅ All wakeup sources work correctly
- ✅ Platform-agnostic worker code
- ✅ Preemption integrated seamlessly
- ✅ No lost wakeups
- ✅ Work stealing enabled

**The runtime now has a robust, cross-platform, non-cooperative preemptive scheduler!**

