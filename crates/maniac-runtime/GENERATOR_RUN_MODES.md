# Generator Run Modes

This document explains the two-mode generator system and how preemption works with each mode.

## Overview

When a task has a pinned generator, it operates in one of two modes:

1. **Switch Mode**: Generator has an interrupted `poll()` call on its stack (one-shot)
2. **Poll Mode**: Generator contains a `poll()` loop (multi-shot)

## Mode 1: Switch Mode

### Characteristics
- Generator has **incomplete `poll()` on its stack** (interrupted mid-execution)
- **One-shot execution**: Resume generator once to complete the interrupted poll
- After poll completes, either:
  - Task is complete → clean up generator
  - Task is still pending → transition to Poll mode

### Creation Scenarios
1. **Worker Generator Promotion** (Preemption on unpinned task)
   - Worker is running normally (no pinned generator)
   - Preemption signal fires
   - Worker generator is pinned to current task in Switch mode
   
2. **Poll Mode → Switch Mode** (Preemption on pinned task)
   - Task already has a Poll mode generator
   - Preemption signal fires while Poll mode generator is running
   - Generator mode changes from Poll → Switch
   - Worker continues with its own generator

### Execution Flow
```rust
// Resume generator ONCE
generator.next() → Completes interrupted poll
    ↓
Check if task completed (future is None)
    ↓
Yes: Clean up generator, mark task complete
No:  Discard worker generator, create Poll mode generator
```

### Key Implementation Detail
After resuming in Switch mode:
- The interrupted `poll()` call completes
- Task state is updated based on poll result
- If task is still pending, we **must transition to Poll mode** because:
  - Worker generator is specific to the worker loop, not the task
  - Task needs its own generator with a poll loop for future resumes

## Mode 2: Poll Mode

### Characteristics
- Generator contains a **loop that calls `poll()` and yields**
- **Multi-shot execution**: Resume generator multiple times until task completes
- Each resume = one poll cycle

### Creation Scenarios
1. **Transition from Switch Mode**
   - Switch mode completes interrupted poll
   - Task is still pending
   - Create new Poll mode generator with poll loop

2. **Spawn with Generator** (Future feature)
   - Task is spawned directly with a Poll mode generator
   - Generator is created at spawn time with poll loop

### Generator Structure
```rust
loop {
    let waker = task.waker_yield();
    let mut cx = Context::from_waker(&waker);
    let poll_result = task.poll_future(&mut cx);
    
    match poll_result {
        Some(Poll::Ready(())) => return 1, // Task complete
        Some(Poll::Pending) => yield 0,    // Yield, resume next poll
        None => return 2,                   // Future dropped
    }
}
```

### Execution Flow
```rust
// Resume generator (may happen many times)
loop {
    generator.next() → Poll task once
        ↓
    Check status code and task state
        ↓
    Status 1/2: Task complete → clean up
    Task yielded: Re-enqueue to yield queue
    Task pending: Wait for wake
}
```

## Preemption: Two Variants

Preemption behavior depends on whether the task already has a pinned generator.

### Variant 1: Task Has No Pinned Generator

**Scenario**: Worker is running normally, preemption signal fires

**Actions**:
1. Check if current task has pinned generator → No
2. **Promote worker generator** to current task
3. Set mode to **Switch**
4. Worker creates new generator for itself
5. Next time task is polled, resume in Switch mode

**Code Flow**:
```rust
if preemption_requested {
    let task = current_task();
    if !task.has_pinned_generator() {
        // Variant 1: Promote worker generator
        pin_worker_generator_to_task(task, Switch);
        create_new_worker_generator();
    }
}
```

### Variant 2: Task Has Pinned Generator (Poll Mode)

**Scenario**: Task is being polled via Poll mode generator, preemption signal fires

**Actions**:
1. Check if current task has pinned generator → Yes (Poll mode)
2. **Update generator mode** from Poll → Switch
3. **Do NOT pin worker generator** (task already has one)
4. Worker continues with its current generator
5. Next time task is polled, resume in Switch mode

**Code Flow**:
```rust
if preemption_requested {
    let task = current_task();
    if task.has_pinned_generator() {
        // Variant 2: Change mode Poll → Switch
        task.set_generator_run_mode(Switch);
        // Worker continues normally
        continue;
    }
}
```

**Why this matters**: When a Poll mode generator is preempted, it has an interrupted poll on its stack, so it becomes Switch mode. After resuming once to complete that poll, it transitions back to Poll mode (if task is still pending).

## State Transitions

```
No Generator
    ↓ (Preemption Variant 1: Worker promotion)
Switch Mode
    ↓ (Resume once to complete poll)
    ├─→ Task Complete (clean up generator)
    └─→ Task Pending (transition to Poll mode)
            ↓
        Poll Mode
            ↓ (Preemption Variant 2: Mode change)
        Switch Mode
            ↓ (Resume once to complete poll)
            ├─→ Task Complete (clean up generator)
            └─→ Task Pending (stay in Poll mode)
                    ↓
                Poll Mode (loop continues...)
```

## Key Design Principles

1. **Worker Generator is Worker-Specific**: When promoted to a task (Switch mode), it completes one poll then is discarded. Task gets its own Poll mode generator.

2. **Poll Mode is Task-Specific**: Each task in Poll mode has its own generator with a poll loop, independent of the worker.

3. **Switch Mode is Transitional**: Always transitions to either completion or Poll mode after one resume.

4. **Preemption is Mode-Aware**: Checks if task already has a generator to determine which variant to use.

5. **Zero Cost for Unpinned Tasks**: Tasks without pinned generators run directly in the worker loop with no generator overhead.

## Implementation Files

- **`task.rs`**: `GeneratorRunMode` enum, generator pinning methods
- **`worker.rs`**: Mode handling logic, preemption variants, mode transitions
- **`preemption.rs`**: Platform-specific preemption signals

## Testing

- **`generator_pinning_example.rs`**: Demonstrates manual pinning (Variant 1)
- **`preemptive_generator_example.rs`**: Demonstrates timer-based preemption (both variants)

