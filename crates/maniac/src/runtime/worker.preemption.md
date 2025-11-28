# Worker and Task Preemption Design

This document describes the generator pinning and preemption mechanisms in the maniac runtime.

## Overview

The maniac runtime uses stackful coroutines (generators) to execute async tasks. Workers run inside generators and poll tasks cooperatively. However, tasks can acquire their own dedicated generator through two mechanisms:

1. **Cooperative Pinning** - Task explicitly calls `pin_stack()`
2. **Preemptive Pinning** - Signal-based preemption interrupts a long-running poll

Both mechanisms transfer generator ownership from the worker to the task, but they differ in when and how this transfer occurs.

## Key Data Structures

### GeneratorRunMode

```rust
pub enum GeneratorRunMode {
    /// Task has no pinned generator
    None,
    /// Preemption mode - generator captured mid-execution via signal
    Switch,
    /// Cooperative mode - task has dedicated generator with poll loop
    Poll,
}
```

### GeneratorOwnership

```rust
pub enum GeneratorOwnership {
    /// Generator was captured from a worker (preemption)
    Worker,
    /// Generator was created specifically for this task (cooperative)
    Owned,
}
```

### PinReason

```rust
pub enum PinReason {
    /// Explicit pin_stack() call from task code
    Explicit,
    /// Signal-based preemption (Unix signal or Windows APC)
    Preempted,
}
```

### RunContext

Tracks generator state during worker execution to coordinate proper exit behavior:

```rust
struct RunContext {
    /// Generator was pinned to a task - worker must create new generator
    pinned: bool,
    /// How the generator was pinned (if pinned is true)
    pin_reason: Option<PinReason>,
    /// Task completed while generator was pinned (vs still pending)
    task_completed: bool,
}
```

## Cooperative Pinning (pin_stack)

### When to Use

Tasks call `pin_stack()` when they need a dedicated stack/generator for the duration of their execution. Common use cases:

- Tasks with deep call stacks
- Tasks that need stack-local storage to persist across awaits
- Performance-critical tasks avoiding generator switching overhead

### How It Works

1. Task code calls `maniac::runtime::worker::pin_stack()`
2. `pin_stack()` sets a **null pointer sentinel** on the task with `GeneratorOwnership::Owned` and `GeneratorRunMode::Poll`
3. The current poll completes normally (task returns `Poll::Pending` or `Poll::Ready`)
4. `ActiveTaskGuard` clears `current_task` when poll returns
5. Worker's `run()` loop detects `run_ctx.pinned == true` and yields
6. Worker creates a new generator and continues processing other tasks
7. When the pinned task is next scheduled, `poll_task_with_pinned_generator()` detects the null sentinel and creates a new dedicated generator for the task

### Key Insight: Null Pointer Sentinel

`pin_stack()` cannot capture the worker's current generator scope pointer because:
- The worker's generator is still actively running
- Attempting to resume it would cause assertion failures (`parent.is_null()`)

Instead, `pin_stack()` sets a null pointer as a sentinel, signaling that a new generator should be created when the task is next polled.

```rust
pub fn pin_stack() -> bool {
    if let Some(worker) = current_worker() {
        let task = worker.current_task;
        if task.is_null() { return false; }
        let task = unsafe { &mut *task };
        if !task.has_pinned_generator() {
            unsafe {
                task.pin_generator(
                    std::ptr::null_mut(),  // Sentinel - create new generator later
                    GeneratorOwnership::Owned,
                    GeneratorRunMode::Poll,
                )
            };
        }
        true
    } else { false }
}
```

## Preemptive Pinning (Signal-Based)

### When It Occurs

Preemption happens when a task's poll takes too long:
- On Unix: SIGALRM or similar signal
- On Windows: Asynchronous Procedure Call (APC)

### How It Works

1. Task is being polled inside worker's generator
2. Signal/APC fires due to time slice expiration
3. `rust_preemption_helper()` is invoked in signal context
4. Helper pins the generator's scope pointer to the task with `GeneratorOwnership::Worker` and `GeneratorRunMode::Switch`
5. Helper yields the generator immediately (mid-poll)
6. Control returns to worker's `run()` loop after `generator.resume()`

### Post-Resume Detection

After `generator.resume()` returns, the worker checks if preemption occurred:

```rust
let task_ptr = self.inner.current_task;
if !task_ptr.is_null() {
    // Preemption occurred - task owns the generator now
    std::mem::forget(generator);
    // Re-enqueue task to yield queue...
}
```

**Why `current_task` is the definitive indicator:**

| Scenario | `current_task` after resume |
|----------|----------------------------|
| Normal exit (shutdown/pin) | `null` - `ActiveTaskGuard` cleared it |
| Cooperative pin | `null` - poll completed, guard ran |
| Preemption | **non-null** - signal yielded before poll returned |

### Generator Ownership Transfer

When preemption is detected:

1. **Don't call `into_raw()`** - `rust_preemption_helper` already stored the scope pointer
2. **Call `std::mem::forget(generator)`** - prevents the generator from being dropped
3. **Re-enqueue the task** - preempted tasks go to yield queue (high priority)
4. **Clear `current_task`** - cleanup for next iteration
5. **Create new generator** - worker continues with fresh generator

## Task Execution Paths

### Unpinned Task (Normal Path)

```
Worker.run()
  > poll_task()
        > task.poll() inside worker's generator
        > ActiveTaskGuard clears current_task
  > continue with same generator
```

### Cooperatively Pinned Task

```
Worker.run()
  > poll_task()
        > task calls pin_stack()
              > Sets null sentinel on task
        > task.poll() returns Pending/Ready
        > ActiveTaskGuard clears current_task
        > run_ctx.pinned = true
  > yield (exit generator)
  > create new generator
  > continue...

Later, when pinned task scheduled:
  > poll_task_with_pinned_generator()
        > Detects null sentinel
        > Creates new dedicated generator
        > Runs poll loop inside dedicated generator
```

### Preempted Task

```
Worker.run()
  > poll_task()
        > task.poll() starts (inside worker's generator)
        > ... long computation ...
        > SIGNAL fires
              > rust_preemption_helper()
                    > Pins scope pointer to task
                    > generator.yield()
  > generator.resume() returns
  > current_task != null (preemption indicator)
  > std::mem::forget(generator)
  > Re-enqueue task to yield queue
  > create new generator
  > continue...

Later, when preempted task scheduled:
  > poll_task_with_pinned_generator()
        > mode == Switch
        > Resumes generator (continues from preemption point)
        > Poll completes
        > Must exit generator (not run worker loop!)
```

## Critical Invariants

### 1. Pinned Generators Never Run Worker Loop

When a generator is pinned (either cooperatively or via preemption), it must **never** execute the worker's main loop again. The worker has already created a new generator and continued.

For preempted tasks (Switch mode), after the interrupted poll eventually completes:
- If `Poll::Ready`: Exit generator, task is done
- If `Poll::Pending`: Exit generator, re-enqueue task

The `RunContext` tracks this state to ensure proper exit behavior.

### 2. current_task Lifecycle

- Set when starting to poll a task
- Cleared by `ActiveTaskGuard` when poll returns
- If non-null after `generator.resume()`: preemption occurred

### 3. Generator Memory Safety

- Cooperative pin: Null sentinel, new generator created later
- Preemption: `std::mem::forget()` prevents double-free, task owns scope pointer

## Debugging

Enable tracing to see generator pinning events:

```rust
#[cfg(debug_assertions)]
tracing::trace!(
    "[Worker {}] Preempted task re-enqueued to yield queue",
    self.inner.worker_id
);
```

Common issues:
- `assertion failed: !self.context.parent.is_null()` - Attempting to resume a generator that's already running or improperly transferred
- Task stuck in yield queue - Check that preempted tasks are properly re-enqueued
- Generator leak - Ensure `std::mem::forget()` is called for preempted generators
