# Maniac Driver Module - Design and Thread-Safety Documentation

## Overview

The driver module provides asynchronous I/O abstractions for the maniac work-stealing runtime. It supports two underlying I/O mechanisms:

- **PollerDriver**: Uses epoll (Linux) or kqueue (macOS/BSD) for event-driven I/O
- **IoUringDriver**: Uses Linux's io_uring for high-performance asynchronous I/O

Both drivers implement the `Driver` trait and are designed to work seamlessly with maniac's work-stealing runtime where tasks can migrate between worker threads.

## Architecture

### Driver Types

#### PollerDriver
- Uses system poll APIs (epoll/kqueue)
- Each worker has its own `PollerDriver` instance
- Only accessed by the owning worker thread
- Event loop (`park()`, `dispatch()`) runs exclusively on owning thread

#### IoUringDriver
- Uses Linux io_uring
- Can be accessed by any thread (io_uring is thread-agnostic)
- Uses mutex-protected internal state for thread safety

### Key Components

- **Inner**: Enum wrapping either `UringInner` or `PollerInner`, stored in `Arc<UnsafeCell<...>>`
- **CURRENT**: Scoped thread-local storage (TLS) holding the current driver instance
- **Op<T>**: In-flight I/O operation future
- **ScheduledIo**: I/O registration and wake-up mechanism

## Thread-Safety Model

### Work-Stealing Runtime Semantics

Maniac uses a work-stealing scheduler where:
- Each worker has its own task queue
- Workers can steal tasks from each other's queues
- A task may execute on different workers over its lifetime
- **Crucially**: Only one thread executes a given task at any given time

### PollerDriver Thread-Safety

#### Single-Threaded Access Contract

`PollerInner` is **only accessed by its owning worker thread**:

1. **park()**: Called only by owning worker to wait for I/O events
2. **dispatch()**: Called only by owning worker to dispatch events to tasks
3. **Slab access**: No mutex protection because access is never concurrent

```rust
// PollerInner structure
pub(crate) struct PollerInner {
    pub(crate) io_dispatch: Slab<std::sync::Arc<ScheduledIo>>,  // No mutex!
    events: mio::Events,
    poll: mio::Poll,
    shared_waker: std::sync::Arc<waker::EventWaker>,
}
```

#### Why This Is Safe

The thread-safety guarantee comes from **runtime architecture**, not from synchronization primitives:

1. **Scoped TLS boundary**: `CURRENT` stores the driver instance in scoped thread-local storage
2. **Single-threaded park**: Only the owning worker calls `park()` to wait for events
3. **Single-threaded dispatch**: Only the owning worker calls `dispatch()` to wake tasks
4. **Work-stealing doesn't affect driver**: When tasks are stolen, they don't access PollerInner directly

#### Task Migration with Poller

When a task with I/O is stolen:

1. **Original worker (Worker A)**:
   - Creates task with I/O operation
   - `Op` captures `driver: Inner::Poller(worker_a_inner)`
   - Task yields (I/O not ready) → put in Worker A's queue
   - Registers FD with epoll/kqueue via Worker A's Poller

2. **Task stolen by Worker B**:
   - Task moved to Worker B's thread
   - Worker B polls the task
   - `Op::poll()` calls `me.driver.poll_op()` → accesses Worker A's PollerInner!
   - **This is safe** because `poll_op()` only does **read/write syscalls** directly
   - No concurrent access to Worker A's PollerInner

3. **Event arrival (Worker A's thread)**:
   - FD becomes ready
   - Worker A's `park()` returns with event
   - Worker A's `dispatch()` is called
   - `dispatch()` wakes up the task via `waker::wake()`
   - Task can now be polled again (by whichever worker owns it)

**Key insight**: The actual Poller (event loop) remains on Worker A's thread, but the task's I/O syscalls happen on whatever worker currently owns the task. This is safe because:
- Read/write syscalls are thread-safe for the same FD
- Only one worker executes the task at a time (work-stealing guarantee)
- The event notification mechanism (`waker`) is thread-safe

### IoUringDriver Thread-Safety

#### Multi-Threaded Access Contract

`UringInner` is **designed to be accessed by any thread**:

```rust
// IoUringInner structure
struct UringInner {
    ops: Ops,  // Mutex-protected!
    #[cfg(feature = "poll-io")]
    poll: super::poll::Poll,
    #[cfg(feature = "poll-io")]
    poller_installed: bool,
    uring: ManuallyDrop<IoUring>,
    shared_waker: std::sync::Arc<waker::EventWaker>,
    eventfd_installed: bool,
    ext_arg: bool,
}

struct Ops {
    slab: parking_lot::Mutex<Slab<MaybeFdLifecycle>>,  // Protected by mutex!
}
```

#### Why Mutex is Needed

io_uring has different semantics from epoll/kqueue:
- io_uring is thread-agnostic - any thread can submit operations
- The submission queue (SQ) and completion queue (CQ) can be accessed from multiple threads
- Therefore, the slab tracking in-flight operations needs mutex protection

#### Task Migration with IoUring

When a task with I/O is stolen:

1. **Any worker** can submit io_uring operations via the shared ring
2. **Any worker** can process completion events from the CQ
3. The mutex-protected slab ensures thread-safe access to operation metadata
4. Wakers ensure tasks are woken up safely across threads

## Send and Sync Guarantees

### Inner Enum

```rust
#[derive(Clone)]
pub(crate) enum Inner {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    Uring(std::sync::Arc<std::cell::UnsafeCell<UringInner>>),
    #[cfg(feature = "poll")]
    Poller(std::sync::Arc<std::cell::UnsafeCell<PollerInner>>),
}

// SAFETY: Inner is Send + Sync because:
//
// For UringInner (io_uring):
// - Accessed through Arc<UnsafeCell<...>>
// - io_uring can be accessed by any thread (thread-agnostic design)
// - The slab inside UringInner is protected by a mutex for thread-safe access
//
// For PollerInner (epoll/kqueue):
// - Accessed through Arc<UnsafeCell<...>>
// - Only accessed by its owning worker thread (single-threaded access)
// - park() and dispatch() are only called by the owning thread
// - The slab does NOT need mutex protection because access is never concurrent
//
// Work-stealing semantics:
// - Tasks can be stolen and migrated between workers
// - When a task with I/O is stolen, Op holds onto original worker's Inner
// - Polling I/O operation (doing actual read/write syscall) happens on stealing worker's thread
// - This is safe because only one thread executes a task at a time (work-stealing guarantee)
// - PollerInner::dispatch() only wakes up task (via waker), not polling I/O
//
// Synchronization boundaries:
// - ScopedKey (scoped TLS) in CURRENT provides the primary synchronization boundary
// - Each worker accesses its own driver via CURRENT, ensuring single-threaded driver operations
// - Cross-thread communication happens through safe abstractions (wakers, Arc)
unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}
```

### Why UnsafeCell is Safe

`UnsafeCell` is used instead of `RefCell` or `Mutex` for performance reasons:

- **No runtime overhead**: No borrow checking or locking cost
- **Correctness enforced by design**: Access patterns are guaranteed by runtime architecture
- **Scoped TLS provides safety**: `CURRENT.set()` establishes a clear critical section

The safety invariant is:
> During `CURRENT.set(&inner, || { ... })`, no other thread can access this `inner` instance.

This holds because:
1. Each worker has its own `PollerDriver` (or shares `IoUringDriver`)
2. Workers access driver via scoped TLS, which is thread-local
3. The runtime ensures drivers are only accessed through `CURRENT`

## I/O Operation Lifecycle

### PollerDriver Flow

1. **Task starts I/O operation**:
   ```rust
   let op = Op::submit_with(data)?;
   ```
   - Calls `driver::CURRENT.with(|this| this.submit_with(data))`
   - PollerInner registers FD with epoll/kqueue
   - Returns `Op` future capturing the current driver

2. **Task yields (I/O not ready)**:
   ```rust
   impl<T> Future for Op<T> {
       fn poll(...) -> Poll<...> {
           let meta = ready!(me.driver.poll_op::<T>(data_mut, me.index, cx));
           // Returns Pending if FD not ready
       }
   }
   ```
   - Task goes to worker's yield queue
   - Worker's `park()` waits for events

3. **Event arrives**:
   - Worker's `park()` receives epoll/kqueue event
   - `dispatch()` is called to wake up the task
   - Task's waker is called (cross-thread safe)

4. **Task polled again**:
   - Read/write syscall performed
   - If successful, operation completes

### IoUringDriver Flow

1. **Task starts I/O operation**:
   - Submits operation to io_uring SQ
   - Stores operation in mutex-protected slab

2. **Task yields (pending)**:
   - Operation is in-flight in kernel
   - Task waits for completion

3. **Completion arrives**:
   - Any worker can process CQ entries
   - Finds operation in mutex-protected slab
   - Wakes up the task

4. **Task polled again**:
   - Returns completion result from kernel

## Cross-Thread Communication

### Wakers

Wakers provide the primary mechanism for cross-thread communication:

- **EventWaker**: mio-based waker for epoll/kqueue
- **UnparkHandle**: Weak reference to waker that can be sent across threads
- **Thread-safe**: `wake()` can be called from any thread

```rust
// waker::UnparkHandle can be sent across threads
impl unpark::Unpark for UnparkHandle {
    fn unpark(&self) -> io::Result<()> {
        if let Some(w) = self.0.upgrade() {
            w.wake()  // Thread-safe wake-up
        }
        Ok(())
    }
}
```

### ScheduledIo

`ScheduledIo` wraps I/O registration and wake-up state:

- Stored in `Arc<...>>` for shared ownership
- Mutations happen through `UnsafeCell` (single-threaded via contract)
- Used by both PollerDriver and IoUringDriver

## Key Design Decisions

### 1. Op Captures Driver Reference

```rust
pub(crate) struct Op<T: 'static + OpAble> {
    pub(super) driver: driver::Inner,  // Captured at creation
    pub(super) index: usize,
    pub(super) data: Option<T>,
}
```

**Rationale**: 
- Simplicity: No need to look up driver via TLS on every poll
- Efficiency: Direct reference, no indirection
- Safety: Correct due to work-stealing semantics

**Trade-off**: When task is stolen, it still references original worker's driver, which is fine because:
- Poller: `poll_op()` does syscalls directly (thread-safe)
- io_uring: Can access any worker's operations (thread-safe)

### 2. Scoped TLS for Driver Access

```rust
scoped_thread_local!(pub(crate) static CURRENT: Inner);

impl Driver for PollerDriver {
    fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        let inner = Inner::Poller(self.inner.clone());
        CURRENT.set(&inner, f)  // Establish safety boundary
    }
}
```

**Rationale**:
- Clear critical sections
- No global mutable state
- Type-safe access to driver
- Zero-cost at runtime

### 3. Different Synchronization for Different Drivers

- **PollerDriver**: No mutex, relies on single-threaded access contract
- **IoUringDriver**: Mutex for multi-threaded access

**Rationale**:
- Optimize for each driver's characteristics
- Poller: Only accessed by one thread → no synchronization needed
- io_uring: Designed for multi-threaded access → mutex required
- Performance: Unnecessary mutexes are expensive

## Comparison with Thread-Per-Core (monoio)

### monoio Design

- Each thread has its own dedicated runtime and driver
- No work-stealing
- Tasks never migrate between threads
- Simple thread-safety: each driver accessed by exactly one thread

### maniac Design

- Work-stealing scheduler
- Tasks can migrate between workers
- Poller: Single-threaded driver access (by owning worker)
- io_uring: Multi-threaded driver access (any worker)
- Complex thread-safety: must handle task migration

### Key Differences

| Aspect | monoio | maniac |
|---------|---------|---------|
| Work-stealing | No | Yes |
| Task migration | Never | Yes |
| Poller access | Single thread | Single thread (by owning worker) |
| io_uring access | Single thread | Multi-threaded (any worker) |
| Synchronization | Not needed | Poller: none, io_uring: mutex |

## Safety Invariants

### Invariant 1: Driver Access Through CURRENT

**Statement**: All driver access must happen through `CURRENT.set(&inner, || { ... })`

**Proof**:
1. `Driver::with()` sets `CURRENT` before calling closure
2. All `Op::poll()` implementations use `me.driver`, not `CURRENT`
3. However, `Op` itself is created via `CURRENT.with()` in `submit_with()`
4. Runtime ensures `CURRENT` is set before creating operations

**Verification**: The pattern ensures:
- Driver is accessible in context
- No direct access to UnsafeCell without TLS boundary
- Clear ownership semantics

### Invariant 2: Single-Threaded Task Execution

**Statement**: Only one thread executes a given task at any given time

**Proof**:
1. Work-stealing scheduler moves tasks between queues atomically
2. A task is either:
   - In a worker's local queue (accessed by that worker)
   - Being executed by a worker (not in any queue)
3. No task can be in multiple queues simultaneously

**Verification**: This invariant is critical for I/O safety:
- Read/write syscalls on same FD from multiple threads would race
- Work-stealing ensures this never happens
- Even if task moves between workers, only one executes it at a time

### Invariant 3: PollerInner Only Accessed by Owner

**Statement**: `PollerInner` is only accessed by its owning worker thread

**Proof**:
1. Each worker has its own `PollerDriver`
2. `park()` and `dispatch()` are the only methods that access PollerInner
3. These are only called by the worker's main loop
4. No other thread has direct access

**Verification**: When a task is stolen:
- It doesn't access PollerInner directly
- It only does syscalls (which are thread-safe)
- It uses wakers for wake-up (which are thread-safe)

### Invariant 4: UringInner Mutex Protects Slab

**Statement**: All access to `UringInner.ops.slab` is protected by mutex

**Proof**:
1. Slab is wrapped in `parking_lot::Mutex`
2. All slab operations (`insert`, `get`, `remove`) lock mutex first
3. Mutex provides mutual exclusion

**Verification**: Allows safe multi-threaded access to io_uring:
- Multiple workers can submit operations
- Multiple workers can process completions
- Mutex ensures slab is accessed safely

## Performance Considerations

### PollerDriver Overhead

- **Event loop**: Single-threaded, no synchronization overhead
- **Registration/Deregistration**: No mutex, direct slab access
- **Dispatch**: No mutex, direct waker calls
- **Optimized**: Best when workers have balanced I/O workloads

### IoUringDriver Overhead

- **Event loop**: Multi-threaded, can process from any worker
- **Operation tracking**: Mutex-protected slab (contention possible)
- **Submission/Completion**: Can scale across workers
- **Optimized**: Best when many workers share same io_uring instance

### Work-Stealing Benefits

- **Load balancing**: Tasks migrate to idle workers
- **Cache locality**: May improve or hurt depending on access patterns
- **I/O efficiency**: Workers don't block waiting for each other's I/O

## Future Improvements

### Potential Optimizations

1. **Per-worker io_uring instances**: Reduce mutex contention
   - Trade-off: More kernel resources, less efficient batching

2. **Adaptive slab sizing**: Adjust to workload
   - Trade-off: Complexity vs. benefit

3. **NUMA-aware driver allocation**: Improve cache locality
   - Trade-off: Complexity vs. benefit

### Design Evolution

1. **Hybrid polling**: Combine Poller and io_uring approaches
   - Use Poller for simple I/O
   - Use io_uring for high-throughput scenarios

2. **Direct I/O bypass**: Skip Poller for certain operations
   - Trade-off: Loss of unified event model

## Conclusion

The maniac driver module is designed to work seamlessly with a work-stealing runtime while maintaining high performance and correct thread-safety:

- **PollerDriver**: Zero-synchronization overhead, relies on runtime guarantees
- **IoUringDriver**: Mutex-protected for safe multi-threaded access
- **Work-stealing compatible**: Tasks can migrate safely between workers
- **Send + Safe**: Both drivers are thread-safe without sacrificing performance

The key insight is that thread-safety is achieved through a combination of:
1. Runtime architecture (work-stealing semantics)
2. Driver-specific synchronization (Poller: none, io_uring: mutex)
3. Scoped TLS boundaries (clear access patterns)
4. Safe cross-thread communication (wakers, Arc)

This design provides the best of both worlds: the simplicity of single-threaded drivers where possible, and the flexibility of multi-threaded drivers where needed.