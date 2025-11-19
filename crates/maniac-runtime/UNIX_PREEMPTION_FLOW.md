# Unix Preemption: How Generator Switching Works

## Overview

On Unix, preemption uses **signal-based non-cooperative context switching**. The key insight is that the **signal handler runs on the generator's stack**, allowing it to directly call `scope.yield_()` to force an immediate generator switch.

## The Complete Flow

### 1. Setup Phase (Worker Thread Startup)

```rust
// In Worker::run(), before creating generator:
crate::runtime::preemption::init_worker_thread_preemption(&self.preemption_requested);
```

This stores a pointer to the worker's `preemption_requested: AtomicBool` in thread-local storage:

```rust
thread_local! {
    static CURRENT_WORKER_PREEMPTION_FLAG: Cell<*const AtomicBool> = ...;
}
```

### 2. Generator Creation with Scope Registration

```rust
// In Worker::run() outer loop:
let generator = generator::Gn::<()>::new_scoped_opt(STACK_SIZE, move |scope| {
    let worker = unsafe { &mut *(worker_addr as *mut Worker<P, NUM_SEGS_P2>) };
    
    // CRITICAL: Register generator scope in thread-local!
    let scope_ptr = &mut scope as *mut _ as *mut ();
    crate::runtime::preemption::set_generator_scope(scope_ptr);
    //                                               ^^^^^^^^^^
    //                      Stored in thread-local: CURRENT_GENERATOR_SCOPE
    
    // Inner loop: task polling happens here
    loop {
        // ... poll tasks, check queues, etc ...
    }
});
```

The `set_generator_scope()` function:

```rust
pub(crate) fn set_generator_scope(scope_ptr: *mut ()) {
    CURRENT_GENERATOR_SCOPE.with(|cell| cell.set(scope_ptr));
}
```

Now the thread-local `CURRENT_GENERATOR_SCOPE` contains a pointer to the generator's `Scope`.

### 3. Preemption Request (Timer Thread)

```rust
// Timer thread decides to interrupt worker 0:
service.interrupt_worker(0);
```

This calls:

```rust
impl WorkerThreadHandle {
    pub fn interrupt(&self) -> Result<(), PreemptionError> {
        #[cfg(unix)]
        {
            unix::interrupt_thread(self.pthread)
            //                     ^^^^^^^^^^^^^
            //                     libc::pthread_t (worker thread ID)
        }
    }
}
```

Which sends SIGALRM:

```rust
pub(super) fn interrupt_thread(pthread: libc::pthread_t) -> Result<(), PreemptionError> {
    unsafe {
        let result = libc::pthread_kill(pthread, libc::SIGALRM);
        //           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        //           Send SIGALRM to specific worker thread
        if result == 0 {
            Ok(())
        } else {
            Err(super::PreemptionError::InterruptFailed)
        }
    }
}
```

### 4. Signal Delivery (On Worker Thread's Stack!)

The kernel delivers `SIGALRM` to the worker thread. The signal handler executes **on the worker thread's stack** (which is the generator's stack):

```rust
extern "C" fn sigalrm_handler(_signum: libc::c_int) {
    super::request_preemption_current_thread();
    //     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //     This function has access to thread-locals!
}
```

### 5. Force Generator Yield (The Magic Happens Here!)

```rust
fn request_preemption_current_thread() {
    // First, set the flag for cooperative fallback
    CURRENT_WORKER_PREEMPTION_FLAG.with(|cell| {
        let ptr = cell.get();
        if !ptr.is_null() {
            unsafe {
                (*ptr).store(true, Ordering::Release);
            }
        }
    });
    
    // THE KEY: Force an immediate yield on Unix!
    #[cfg(unix)]
    {
        CURRENT_GENERATOR_SCOPE.with(|cell| {
            let scope_ptr = cell.get();
            if !scope_ptr.is_null() {
                unsafe {
                    // CRITICAL: Cast back to generator::Scope<(), usize>
                    let scope = &mut *(scope_ptr as *mut generator::Scope<(), usize>);
                    
                    // FORCE IMMEDIATE YIELD!
                    scope.yield_(1);
                    //   ^^^^^^^^^^
                    //   Generator context switch happens RIGHT HERE!
                    //   Stack is suspended, control returns to outer loop!
                }
            }
        });
    }
}
```

**This is the magic!** The signal handler calls `scope.yield_(1)`, which:
1. Saves the generator's stack state
2. Returns control to the outer loop (where `for _ in &mut generator` is running)
3. The generator iterator returns `Some(1)` (the yield value)

### 6. Generator Pinning (Back in Worker::run())

```rust
// In Worker::run() outer loop:
let generator = generator::Gn::<()>::new_scoped_opt(...);

// Drive the generator until completion or yield
for yield_value in &mut generator {
    //              ^^^^^^^^^^^^^^^
    //              This receives the `1` from scope.yield_(1)!
    
    // Generator yielded - check if it was for pinning
}

// Check if a task captured the generator for pinning
let task_ptr = GENERATOR_PIN_TASK.with(|cell| {
    let ptr = cell.get();
    if !ptr.is_null() {
        cell.set(ptr::null_mut());
    }
    ptr
});

if !task_ptr.is_null() {
    let task = unsafe { &*task_ptr };
    
    // Convert generator to a boxed iterator
    let boxed_iter: Box<dyn Iterator<Item = usize> + 'static> = unsafe {
        std::mem::transmute(Box::new(generator) as Box<dyn Iterator<Item = usize>>)
    };
    
    // Pin generator to task with Switch mode (one-shot resume)
    unsafe {
        task.pin_generator(boxed_iter, GeneratorRunMode::Switch);
    }
    
    // Re-schedule the preempted task in the yield queue for work-stealing
    let handle = TaskHandle::from_task(task);
    task.mark_yielded();
    task.record_yield();
    self.enqueue_yield(handle);
    self.stats.yielded_count += 1;
    
    // Continue outer loop to create a NEW generator!
}
```

### 7. New Generator Created

The outer loop continues, creating a **new generator** for the worker:

```rust
// Back to top of outer loop:
loop {
    let generator = generator::Gn::<()>::new_scoped_opt(...);
    //              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //              NEW generator with fresh stack!
    
    for _ in &mut generator {
        // Worker continues with new tasks!
    }
}
```

## Key Technical Points

### Why This Works on Unix

1. **Signal Handler Runs on Generator Stack**
   - The SIGALRM handler executes on the worker thread
   - Which is currently executing the generator's stack
   - So `scope` is a valid reference in the signal handler's context

2. **Direct Scope Access**
   - We store the raw pointer to `Scope` in thread-local storage
   - Signal handler retrieves it and casts back
   - Calls `scope.yield_(1)` directly - no need for cooperative check!

3. **Immediate Context Switch**
   - `scope.yield_()` is a **synchronous** operation
   - Returns control immediately to the iterator loop
   - No polling or flag checking needed

### What Gets Saved

When `scope.yield_(1)` is called:

```
Generator Stack:
┌─────────────────────────────────────┐
│  Signal Handler (sigalrm_handler)   │ ← Currently executing
│  ↓ calls scope.yield_(1)            │
├─────────────────────────────────────┤
│  poll_task() in progress             │ ← Suspended mid-execution!
│  • Local variables preserved         │
│  • Return address saved              │
│  • Register state captured           │
├─────────────────────────────────────┤
│  Worker loop inside generator        │
│  • spin_count, waker_id, etc.        │
├─────────────────────────────────────┤
│  Generator machinery                 │
└─────────────────────────────────────┘
         ↓ yield_() saves this ↓
   Stored in generator object
         (pinned to task)
```

The entire call stack from the generator's entry point up to the signal handler is **suspended and saved** in the generator object.

### Resume Flow (When Task Runs Again)

```rust
// Later, when worker picks up the preempted task:
fn poll_task_switch_mode(task: &Task, generator: &mut BoxedGenerator) {
    // Resume the generator (continues from yield point)
    let result = generator.next();
    //           ^^^^^^^^^^^^^^^^
    //           Restores stack, returns to where yield_() was called
    //           Signal handler returns, poll_task() continues!
    
    match result {
        Some(_) => {
            // Generator yielded again - check task state
            if !task.take_future().is_none() {
                // Task still pending - transition to Poll mode
                self.create_poll_mode_generator(task);
            } else {
                // Task completed!
                self.complete_task(task);
            }
        }
        None => {
            // Generator returned - task completed
            self.complete_task(task);
        }
    }
}
```

## Visualization: Complete Flow

```
Timer Thread                Worker Thread (Generator Stack)
─────────────────────────────────────────────────────────────

                           ┌─ Generator Loop ──────────────┐
                           │ loop {                        │
                           │   poll_task(handle);          │ ← Executing
                           │     ↓                         │
                           │   task.poll(cx)               │ ← In progress
                           │     ↓                         │
                           │   [CPU-bound computation]     │ ← Stuck here!
                           │ }                             │
                           └───────────────────────────────┘

interrupt_worker(0)
     ↓
pthread_kill(worker_pthread, SIGALRM)
     ↓                     
  ─────────────────────>  Kernel delivers SIGALRM
                                 ↓
                          ┌─ Signal Handler ─────────────┐
                          │ sigalrm_handler()            │
                          │   ↓                          │
                          │ request_preemption_current() │
                          │   ↓                          │
                          │ scope.yield_(1) ──────────┐  │
                          └───────────────────────────│──┘
                                                      │
                                Stack saved ◄─────────┘
                                Generator returns Some(1)
                                      ↓
                          ┌─ Outer Loop (Worker::run) ───┐
                          │ for yield_val in &mut gen {  │
                          │   // Got yield_val == 1      │
                          │ }                            │
                          │ // Check GENERATOR_PIN_TASK │
                          │ task.pin_generator(gen);    │
                          │ enqueue_yield(task);        │
                          └──────────────────────────────┘
                                      ↓
                          ┌─ New Generator ──────────────┐
                          │ generator::Gn::new_scoped()  │
                          │ loop {                       │
                          │   // Process other tasks!    │
                          │ }                            │
                          └──────────────────────────────┘
```

## Safety Considerations

### Async-Signal-Safety

The signal handler is **NOT** async-signal-safe in the strict POSIX sense because:
- It calls `scope.yield_()` which manipulates stack state
- It accesses thread-local storage

**However**, this is safe in our context because:
1. **Single-threaded worker**: Only one signal can be delivered at a time to this thread
2. **Generator stack**: We're operating on the generator's stack, which we own
3. **No heap allocation**: Thread-locals are already allocated
4. **No locks**: No mutexes or condition variables in signal handler
5. **Controlled environment**: We control when signals are sent

### Why Raw Pointers are Safe

```rust
let scope_ptr = &mut scope as *mut _ as *mut ();
CURRENT_GENERATOR_SCOPE.with(|cell| cell.set(scope_ptr));
```

This is safe because:
- **Lifetime**: The pointer is valid for the generator's lifetime
- **Thread-local**: Only accessed from the worker thread
- **Cleared on exit**: We call `clear_generator_scope()` when generator exits
- **Single writer**: Only the worker thread writes this pointer

## Comparison with Cooperative Preemption

### Cooperative (Flag-Based)

```rust
// Worker must check flag explicitly
loop {
    if check_and_clear_preemption(&self.preemption_requested) {
        // Pin generator and create new one
    }
    poll_task(); // ← Must complete before check!
}
```

**Problem**: Task must finish polling before preemption happens.

### Non-Cooperative (Signal-Based)

```rust
// Signal handler forces immediate yield
extern "C" fn sigalrm_handler(_sig: i32) {
    scope.yield_(1); // ← Interrupts poll_task() MID-EXECUTION!
}
```

**Advantage**: Task is interrupted **immediately**, even mid-poll!

## Summary

Unix preemption achieves **true non-cooperative scheduling** by:

1. ✅ **Registering generator scope** in thread-local storage
2. ✅ **Sending SIGALRM** to worker thread via `pthread_kill()`
3. ✅ **Signal handler runs on generator's stack**
4. ✅ **Direct call to `scope.yield_()`** forces immediate context switch
5. ✅ **Generator stack saved** in generator object
6. ✅ **Generator pinned to task** with `Switch` mode
7. ✅ **New generator created** for worker to continue
8. ✅ **Preempted task re-enqueued** in yield queue for work-stealing

**Result**: Worker can interrupt CPU-bound tasks at any point, achieving OS-level preemptive multitasking in user-space!

