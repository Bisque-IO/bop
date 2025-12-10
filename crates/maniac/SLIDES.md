# maniac

## High-Performance Async Runtime for Rust

### Stackful Coroutines | Preemptive Scheduling | io_uring

---

# Slide 1: Introduction

## What is maniac?

A **next-generation async runtime** for Rust that combines:

- **M:N threading model** - Many green threads on few OS threads
- **Stackful + stackless coroutines** - Choose your model
- **Preemptive scheduling** - True fairness via signal injection
- **Signal Tree scheduler** - Scales to 512+ cores with O(1) work discovery
- **Multi-backend I/O** - io_uring, epoll, kqueue, IOCP
- **Zero-copy async file I/O** - Direct kernel buffers

**Target**: High-concurrency servers, distributed systems, game engines, real-time applications

---

# Slide 2: Why Another Runtime?

## Limitations of Existing Runtimes

| Problem | Tokio | async-std | maniac |
|---------|-------|-----------|--------|
| Deep call stacks | Stack overflow | Stack overflow | Stackful coroutines |
| CPU-bound fairness | Starvation | Starvation | Preemption |
| Sync code in async | `spawn_blocking` | `spawn_blocking` | `sync_await` |
| io_uring | Add-on crate | Not supported | Built-in |
| Raft consensus | External | External | Built-in |

### The Core Insight

**Async Rust forces a choice**: Either pay the cost of state machines (stackless) or lose async benefits entirely.

**maniac says**: Why not both?

---

# Slide 3: Architecture Overview

## Core Components

```
┌────────────────────────────────────────────────────────────────────┐
│                         maniac Runtime                             │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                      Task Arena                             │  │
│  │   (Memory-mapped task storage with huge page support)       │  │
│  └─────────────────────────────────────────────────────────────┘   │
│                              │                                      │
│         ┌────────────────────┼────────────────────┐                │
│         ▼                    ▼                    ▼                │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐          │
│  │  Worker 0   │     │  Worker 1   │     │  Worker N   │          │
│  │ (Generator) │     │ (Generator) │     │ (Generator) │          │
│  ├─────────────┤     ├─────────────┤     ├─────────────┤          │
│  │ Local Queue │     │ Local Queue │     │ Local Queue │          │
│  │ Timer Wheel │     │ Timer Wheel │     │ Timer Wheel │          │
│  │ I/O Driver  │     │ I/O Driver  │     │ I/O Driver  │          │
│  └──────┬──────┘     └──────┬──────┘     └──────┬──────┘          │
│         │                   │                   │                  │
│         └───────────────────┼───────────────────┘                  │
│                             ▼                                      │
│              ┌─────────────────────────────┐                       │
│              │   Work-Stealing Deque       │                       │
│              │   (Global load balancing)   │                       │
│              └─────────────────────────────┘                       │
└─────────────────────────────────────────────────────────────────────┘
```

---

# Slide 4: Two Coroutine Models

## Stackless vs Stackful

### Stackless (Traditional Async)

```rust
async fn fetch_data() -> Data {
    let response = client.get(url).await;  // State machine
    parse(response).await
}
```

- Compiler transforms to state machine
- Zero allocation per await point
- Limited: Can't await inside callbacks/closures easily

### Stackful (Generator-based)

```rust
fn compute_heavy() -> Result {
    // Full call stack preserved
    // Can be preempted mid-execution
    deep_recursive_function();
    sync_await(async_operation());  // Bridge to async!
}
```

- Full stack preserved across yields
- Preemptible at any instruction
- Deep call stacks without overflow

---

# Slide 5: The sync_await Bridge

## Connecting Sync and Async Worlds

### The Problem

```rust
// Inside a synchronous callback (e.g., SQLite VFS)
fn read_page(&self, page: u32, buf: &mut [u8]) -> Result<()> {
    // How do we call async storage here?
    // .await doesn't work in sync contexts!
}
```

### The Solution: sync_await

```rust
fn read_page(&self, page: u32, buf: &mut [u8]) -> Result<()> {
    // Check cache first (fast path - no async needed)
    if let Some(data) = self.cache.get(&page) {
        buf.copy_from_slice(&data);
        return Ok(());
    }

    // Slow path: async I/O via sync_await
    sync_await(self.load_page_async(page, buf))
}
```

### How It Works

- Only callable from worker/generator context
- Yields the generator until future completes
- Returns result synchronously
- **Zero-cost when future is immediately ready**

---

# Slide 6: Preemptive Scheduling

## True Fairness for CPU-Bound Work

### The Problem

```rust
// This starves other tasks in Tokio/async-std
async fn cpu_bound() {
    for i in 0..1_000_000_000 {
        heavy_computation(i);  // No await = no yield
    }
}
```

### maniac's Solution: Signal-Based Preemption

```
┌──────────────────────────────────────────────────────────────────┐
│                    Preemption Flow                               │
└──────────────────────────────────────────────────────────────────┘

Time ──────────────────────────────────────────────────────────────►

Worker:   [──── Task A (CPU-bound) ────]
                        │
                        ▼ SIGVTALRM / SuspendThread
                   ┌─────────┐
                   │Trampoline│ ← Save ALL registers
                   └────┬────┘
                        │
                        ▼ Yield generator
Scheduler: ─────────────────────────[Pick Task B]────────────────►
                                         │
                                         ▼
Worker:                              [── Task B ──]
```

---

# Slide 7: Trampoline Injection

## How Preemption Actually Works

### Step-by-Step

```
1. Scheduler Timer Fires
   └─► Send signal (Unix) or SuspendThread (Windows)

2. Thread Interrupted
   └─► Signal handler / thread suspended

3. Context Modification
   └─► Save original PC to stack
   └─► Set PC = preemption_trampoline

4. Resume Execution
   └─► Thread resumes at trampoline

5. Trampoline Executes (Assembly)
   ├─► Save ALL volatile registers (GPRs + SIMD)
   ├─► Call rust_preemption_helper()
   │     └─► Yield the generator
   ├─► [Later: generator resumes here]
   ├─► Restore all registers
   └─► Return to original instruction
```

### Safety Guarantees

- **Async-signal-safe**: Only atomic register manipulation in handler
- **Stack agnostic**: Works at any call depth
- **Register integrity**: All volatile registers preserved
- **Re-entrancy protection**: Atomic flag prevents nesting

---

# Slide 8: Platform Support

## Cross-Platform Preemption

| Platform | Interrupt | Register Preservation | Notes |
|----------|-----------|----------------------|-------|
| **x86_64 Linux** | SIGVTALRM | GPRs + XMM0-15 | Red zone aware |
| **x86_64 macOS** | SIGVTALRM | GPRs + XMM0-15 | Mach exception compat |
| **x86_64 Windows** | SuspendThread | GPRs + XMM0-15 | WriteProcessMemory |
| **ARM64 Linux** | SIGVTALRM | x0-x18 + v0-v31 | Link register emulated |
| **ARM64 macOS** | SIGVTALRM | x0-x18 + v0-v31 | Darwin ABI |
| **ARM64 Windows** | SuspendThread | x0-x18 + v0-v31 | ARM64EC support |
| **RISC-V 64** | SIGVTALRM | Volatile + s0-s11 | ra register emulated |
| **LoongArch64** | SIGVTALRM | All caller-visible | Full SIMD |

### Assembly Per Platform

Each platform has dedicated assembly:
- `preemption_trampoline_x86_64_unix.s`
- `preemption_trampoline_x86_64_windows.s`
- `preemption_trampoline_aarch64.s`
- `preemption_trampoline_riscv64.s`
- `preemption_trampoline_loongarch64.s`

---

# Slide 9: I/O Backend Architecture

## Pluggable I/O Drivers

```
┌─────────────────────────────────────────────────────────────┐
│                     I/O Abstraction Layer                   │
│              (Unified async read/write/accept)              │
└─────────────────────────────────────────────────────────────┘
                            │
            ┌───────────────┼───────────────┐
            ▼               ▼               ▼
     ┌───────────┐   ┌───────────┐   ┌───────────┐
     │ io_uring  │   │   mio     │   │   IOCP    │
     │ (Linux)   │   │(portable) │   │ (Windows) │
     └───────────┘   └───────────┘   └───────────┘
            │               │               │
            ▼               ▼               ▼
     ┌───────────┐   ┌───────────┐   ┌───────────┐
     │  Linux    │   │  epoll    │   │  Windows  │
     │  5.6+     │   │  kqueue   │   │  Kernel   │
     └───────────┘   └───────────┘   └───────────┘
```

### Runtime Selection

```rust
pub enum FusionRuntime<L, R> {
    Left(L),   // io_uring (preferred on Linux 5.6+)
    Right(R),  // Fallback to mio/epoll
}
```

---

# Slide 10: io_uring Integration

## Linux's Fastest I/O Interface

### What is io_uring?

- Shared memory ring buffers between kernel and userspace
- Submit multiple operations with single syscall
- Completion-based (not readiness-based)
- Supports files, sockets, timers, and more

### maniac's io_uring Features

```rust
// Zero-copy file read
async fn read_file(fd: Fd, buf: Vec<u8>, offset: u64) -> BufResult<usize, Vec<u8>> {
    io_uring_read_at(fd, buf, offset).await
}

// Zero-copy splice (file → file)
async fn copy_file(src: Fd, dst: Fd, len: usize) -> io::Result<usize> {
    splice(src, dst, len).await  // No userspace copy!
}
```

### Supported Operations

| Category | Operations |
|----------|------------|
| **File** | read, write, fsync, fallocate, statx |
| **Socket** | accept, connect, recv, send |
| **Advanced** | splice, tee, unlinkat, renameat, mkdirat |

---

# Slide 11: Async File I/O

## True Async for Files

### The Problem with Traditional I/O

```rust
// Tokio: Spawns to blocking thread pool
let data = tokio::fs::read("file.txt").await;  // Not truly async!
```

### maniac's Solution

```rust
// True async via io_uring
let data = maniac::fs::read("file.txt").await;  // Real async I/O!
```

### Zero-Copy Design

```rust
pub trait RawBufRead {
    fn as_buf_ptr(&self) -> *const u8;
    fn buf_len(&self) -> usize;
}

pub trait RawBufWrite {
    fn as_buf_mut_ptr(&mut self) -> *mut u8;
    fn buf_capacity(&self) -> usize;
}

// Buffer ownership returned with result
pub type BufResult<T, B> = (io::Result<T>, B);
```

### File Operations

```rust
// Positioned reads (no seek needed)
file.read_at(&mut buf, offset).await?;

// Positioned writes
file.write_at(&buf, offset).await?;

// Direct I/O (bypass page cache)
file.read_direct(&mut aligned_buf).await?;
```

---

# Slide 12: Timer System

## Hierarchical Timer Wheel

### Three-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Hierarchical Timer Wheel                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│   Level 0 (Fine)           Level 1 (Medium)        Level 2 (Coarse) │
│   ┌─────────────┐          ┌─────────────┐         ┌─────────────┐  │
│   │ Resolution: │          │ Resolution: │         │ Resolution: │  │
│   │   ~1.05ms   │          │   ~1.07s    │         │   ~68.7s    │  │
│   │             │          │             │         │             │  │
│   │ Spokes: 1024│          │ Spokes: 1024│         │ Spokes: 1024│  │
│   │             │          │             │         │             │  │
│   │ Coverage:   │          │ Coverage:   │         │ Coverage:   │  │
│   │   ~1.07s    │   ───►   │   ~18 min   │  ───►   │   ~19.5 hr  │  │
│   └─────────────┘          └─────────────┘         └─────────────┘  │
│                                                                     │
│   Poll: Every tick         Poll: On L0 wrap       Poll: On L1 wrap  │
└─────────────────────────────────────────────────────────────────────┘
```

### Performance

| Operation | Complexity |
|-----------|-----------|
| Schedule timer | O(1) |
| Cancel timer | O(1) |
| Poll expired | O(expired) |

---

# Slide 13: Timer Implementation

## Efficient Time Management

### Timer ID Encoding

```
64-bit Timer ID:
┌──────┬────────────┬────────────┬─────────────────────┐
│Level │ Generation │   Spoke    │        Slot         │
│2 bits│  22 bits   │  12 bits   │      28 bits        │
└──────┴────────────┴────────────┴─────────────────────┘
```

### Memory Management

- **Per-spoke allocation**: Independent vector + free list
- **Lazy growth**: Only allocate as needed
- **Auto-compaction**: Shrink when utilization < 25%
- **Generation counter**: Detect stale cancellations

### Long Duration Timers

```rust
// For timers > 19.5 hours:
let (timer_id, actual_deadline) = wheel.schedule_timer_best_fit(duration);
// actual_deadline = earliest wheel can handle
// Application reschedules when first timer fires
// Progressive approach achieves microsecond precision
```

---

# Slide 14: Signal Tree Scheduler

## Novel Work-Stealing for High Core Counts

### The Problem with Traditional Work-Stealing

| Approach | Issue at 64+ Cores |
|----------|-------------------|
| **Deque-based** | O(N) scanning to find work |
| **Global queue** | Contention bottleneck |
| **Random stealing** | Cache thrashing, unfair |

### maniac's Solution: Hierarchical Signal Tree

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Two-Level Signal Tree                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Level 1: WorkerWaker Status Word (64 bits per worker)              │
│           ┌─────────────────────────────────────────────┐           │
│           │ Bits 0-61: Leaf summary │ Bit 62  │ Bit 63  │           │
│           │ (which leafs have work) │Partition│ Yield   │           │
│           └─────────────────────────────────────────────┘           │
│                              │                                      │
│                              ▼                                      │
│  Level 2: Summary Tree (global, partitioned across workers)         │
│           ┌──────┬──────┬──────┬──────┬──────┬──────────┐           │
│           │Leaf 0│Leaf 1│Leaf 2│ ...  │Leaf N│          │           │
│           │64 sig│64 sig│64 sig│      │64 sig│          │           │
│           └──────┴──────┴──────┴──────┴──────┴──────────┘           │
│                              │                                      │
│  Level 3: Task Reservations (64 slots per signal)                   │
│           Each signal word → 64 task slots                          │
│                                                                     │
│  Total Capacity: leafs × 64 signals × 64 slots = millions of tasks  │
└─────────────────────────────────────────────────────────────────────┘
```

---

# Slide 15: Signal Tree - How It Works

## O(1) Work Discovery at Any Scale

### Partition Assignment

```
Workers own disjoint partitions of the Signal Tree:

Worker 0: Leafs [0, 1, 2, 3]     ← Owns partition 0
Worker 1: Leafs [4, 5, 6, 7]     ← Owns partition 1
Worker 2: Leafs [8, 9, 10, 11]   ← Owns partition 2
...
Worker N: Leafs [...]            ← Owns partition N

Each worker maintains a local cache (partition_summary) of its leafs.
```

### Work Discovery Flow

```
1. Check partition_summary (single atomic load)
   └─► If non-zero: Work in my partition! O(1)

2. Find active leaf via trailing_zeros()
   └─► O(1) bit manipulation

3. Reserve task slot via CAS
   └─► Lock-free, O(1)

4. If partition empty, steal from neighbors
   └─► Round-robin across other partitions
```

### Why This Scales

| Workers | Traditional Deque | Signal Tree |
|---------|-------------------|-------------|
| 8 | Fast | Fast |
| 64 | Slow (scanning) | Fast (O(1)) |
| 256 | Very slow | Fast (O(1)) |
| 512 | Unusable | Fast (O(1)) |

---

# Slide 16: WorkerWaker Architecture

## Cache-Optimized Coordination

### Single-Word Status for Zero Contention

```rust
#[repr(align(64))]  // Cache line aligned
pub struct WorkerWaker {
    // All state packed into one atomic word
    status: CachePadded<AtomicU64>,
    //      ├─ Bits 0-61:  Leaf summary (which have work)
    //      ├─ Bit 62:     Partition has tasks
    //      └─ Bit 63:     Yield requested

    // Counting semaphore for wakeups
    permits: CachePadded<AtomicU64>,

    // Approximate sleeper count
    sleepers: CachePadded<AtomicUsize>,

    // Local partition cache (up to 64 leafs)
    partition_summary: CachePadded<AtomicU64>,
}
```

### Key Design Decisions

| Decision | Benefit |
|----------|---------|
| **CachePadded atomics** | No false sharing between cores |
| **Single status word** | One load to check all state |
| **Permit accumulation** | No lost wakeups ever |
| **Lazy cleanup** | False positives OK, false negatives impossible |

---

# Slide 17: Task Fairness via Partitions

## Guaranteed Work Distribution

### Problem: Traditional Unfairness

```
Traditional work-stealing:
- Hot workers get more work (rich get richer)
- Cold workers starve
- No guaranteed progress for any task
```

### Solution: Partition-Based Fairness

```
┌──────────────────────────────────────────────────────────────────┐
│                   Partition-Based Fairness                       │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Task Spawned                                                    │
│       │                                                          │
│       ▼                                                          │
│  Round-Robin Partition Selection                                 │
│       │                                                          │
│       ├──► Partition 0 (Worker 0 owns) ──► Worker 0 notified     │
│       ├──► Partition 1 (Worker 1 owns) ──► Worker 1 notified     │
│       ├──► Partition 2 (Worker 2 owns) ──► Worker 2 notified     │
│       └──► ...                                                   │
│                                                                  │
│  Result: Tasks distributed evenly across all workers             │
└──────────────────────────────────────────────────────────────────┘
```

### Notification Flow

```rust
// When task scheduled to leaf in partition P:
fn notify_partition_owner(&self, leaf_idx: usize) {
    let owner_id = self.compute_partition_owner(leaf_idx);
    let waker = &self.wakers[owner_id];

    // Compute local leaf index within owner's partition
    let local_idx = leaf_idx - partition_start;

    // Mark leaf active in owner's summary (O(1))
    waker.mark_partition_leaf_active(local_idx);
    // Owner wakes up if sleeping
}
```

---

# Slide 18: Worker Configuration

## Per-Worker Resources

### Worker Structure

```rust
pub struct Worker {
    // Task execution
    current_task: *mut Task,           // Currently executing
    current_scope: *mut usize,         // Generator scope

    // I/O and timing
    io: *mut IoDriver,                 // Event loop driver
    timer_wheel: *mut TimerWheel,      // Per-worker timers

    // Preemption
    preemption_requested: AtomicBool,  // Signal pending

    // Signal Tree integration
    waker: Arc<WorkerWaker>,           // Status + permits
    partition_start: usize,            // First leaf I own
    partition_end: usize,              // Last leaf (exclusive)
}
```

### Configuration Constants

```rust
const DEFAULT_TICK_DURATION_NS: u64 = 1 << 19;  // ~1.05ms
const DEFAULT_WAKE_BURST: usize = 4;            // Tasks per wakeup
const TIMER_EXPIRY_BUDGET: usize = 4096;        // Timers per tick
const MESSAGE_BATCH_SIZE: usize = 4096;         // MPSC batch
const MAX_WORKER_LIMIT: usize = 512;            // Hard limit
const SIGNALS_PER_LEAF: usize = 64;             // Task signals per leaf
const TASK_SLOTS_PER_SIGNAL: usize = 64;        // Slots per signal
```

---

# Slide 19: Task Arena

## High-Performance Task Storage

### Memory-Mapped Arena

```
┌──────────────────────────────────────────────────────────────────┐
│                         Task Arena                               │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Leaf 0              Leaf 1              Leaf N                  │
│  ┌──────────┐        ┌──────────┐        ┌──────────┐            │
│  │ Task 0   │        │ Task 4K  │        │ Task NK  │            │
│  │ Task 1   │        │ Task 4K+1│        │ Task NK+1│            │
│  │ ...      │        │ ...      │        │ ...      │            │
│  │ Task 4095│        │ Task 8K-1│        │ Task (N+1)K-1│        │
│  └──────────┘        └──────────┘        └──────────┘            │
│                                                                  │
│  Default: 8 leaves × 4096 tasks = 32,768 concurrent tasks        │
└──────────────────────────────────────────────────────────────────┘
```

### Features

- **O(1) allocation** via free list
- **Huge page support** for TLB efficiency
- **Pre-initialization** for deterministic latency
- **Type-erased storage** via function pointers

### Task Layout

```rust
struct TaskSlot {
    header: FutureHeader,    // Poll/drop function pointers
    join_state: JoinState,   // Waker + result storage
    future: [u8; N],         // Inline future storage
}
```

---

# Slide 20: Synchronization Primitives

## Lock-Free Building Blocks

### DiatomicWaker

```rust
// Two-slot waker storage for lock-free task wakeup
pub struct DiatomicWaker {
    waker_slots: [AtomicUsize; 2],
    state: AtomicU8,
}
```

- **~40% fewer RMW operations** than `atomic-waker`
- Concurrent waker registration without spinlocks
- Supports `WakeSink` (single-consumer) and `WakeSource` (multi-producer)

### MPSC Channel

```rust
// Lock-free multi-producer single-consumer
pub struct AsyncMpscSender<T> { ... }
pub struct AsyncMpscReceiver<T> { ... }
pub struct BlockingMpscSender<T> { ... }
pub struct BlockingMpscReceiver<T> { ... }
```

- Async/blocking variants can be mixed
- Segmented queue design
- ~50M+ ops/sec under contention

---

# Slide 21: SegSpsc Queue

## Novel SPSC Data Structure

### Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                    SegSpsc Queue                               │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  Directory (64 segments max)                                   │
│  ┌─────┬─────┬─────┬─────┬─────┬─────────────────────────────┐ │
│  │ Seg0│ Seg1│ Seg2│ ... │Seg63│          (sparse)           │ │
│  └──┬──┴──┬──┴──┬──┴─────┴─────┴─────────────────────────────┘ │
│     │     │     │                                              │
│     ▼     ▼     ▼                                              │
│  ┌─────┐┌─────┐┌─────┐                                         │
│  │Slots││Slots││Slots│  Each: 2^P slots (e.g., 4096)           │
│  └─────┘└─────┘└─────┘                                         │
│                                                                │
│  Position encoding: segment_idx = (pos >> P) & 63              │
│  Close flag: bit 63 of head position                           │
└────────────────────────────────────────────────────────────────┘
```

### Features

- **Zero upfront allocation** - Memory on-demand
- **O(1) operations** - Push/pop constant time
- **Auto-compaction** - Shrink when utilization drops
- **Close bit encoding** - Graceful shutdown

---

# Slide 22: Blocking Thread Pool

## Offloading Blocking Operations

### Usage

```rust
// Run blocking code without blocking the runtime
let content = maniac::blocking::unblock(|| {
    std::fs::read_to_string("large_file.txt")
}).await?;
```

### Features

- **Dynamic thread spawning** - Up to 500 threads (configurable)
- **Thread timeout** - Idle threads cleaned up
- **Panic safety** - Task aborts, not runtime
- **Environment variable**: `BLOCKING_MAX_THREADS`

### When to Use

| Use `unblock()` | Use async directly |
|-----------------|-------------------|
| `std::fs` operations | `maniac::fs` operations |
| CPU-bound computation | I/O-bound operations |
| FFI blocking calls | Network operations |
| Legacy sync libraries | Database with async driver |

---

# Slide 23: Network I/O

## TCP, UDP, Unix Sockets

### TCP Server Example

```rust
use maniac::net::TcpListener;

#[maniac::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("Connection from {}", addr);

        maniac::spawn(async move {
            handle_connection(stream).await
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut buf = vec![0u8; 4096];

    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 { break; }
        stream.write_all(&buf[..n]).await?;
    }
    Ok(())
}
```

### Supported Protocols

- **TCP**: TcpListener, TcpStream
- **UDP**: UdpSocket
- **Unix**: UnixListener, UnixStream (Linux/BSD/macOS)
- **Pipes**: Named pipes (Windows), Unix pipes

---

# Slide 24: TLS Support

## Secure Connections

### Rustls (Default)

```rust
use maniac::net::tls::{TlsConnector, TlsAcceptor};

// Client
let connector = TlsConnector::new()?;
let stream = connector.connect("example.com", tcp_stream).await?;

// Server
let acceptor = TlsAcceptor::from_pem(cert, key)?;
let stream = acceptor.accept(tcp_stream).await?;
```

### Features

- **Rustls** with AWS-LC for FIPS compliance
- **Native TLS** option via system certificates
- **Zero-copy TLS** (experimental) for high throughput

### Feature Flags

```toml
[features]
tls-rustls = ["rustls", "aws-lc-rs"]
tls-native = ["native-tls"]
```

---

# Slide 25: Raft Integration

## Built-in Distributed Consensus

### Multi-Raft Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                    maniac-raft Integration                    │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                  Multi-Raft Manager                     │  │
│  │   (Manages multiple Raft groups on single node)         │  │
│  └─────────────────────────────────────────────────────────┘  │
│                            │                                  │
│         ┌──────────────────┼──────────────────┐               │
│         ▼                  ▼                  ▼               │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐        │
│  │ Raft Group 1│    │ Raft Group 2│    │ Raft Group N│        │
│  │ (Database A)│    │ (Database B)│    │ (Database N)│        │
│  └─────────────┘    └─────────────┘    └─────────────┘        │
│                                                               │
│  Features:                                                    │
│  - Heartbeat coalescing (reduce network traffic)              │
│  - Connection pooling (efficient resource use)                │
│  - Async filesystem storage                                   │
│  - TCP transport with multiplexing                            │
└───────────────────────────────────────────────────────────────┘
```

---

# Slide 26: Comparison with Tokio

## Feature Matrix

| Feature | maniac | Tokio |
|---------|--------|-------|
| **Stackful Coroutines** | Native | Not supported |
| **Preemptive Scheduling** | Signal-based | Not supported |
| **io_uring** | Built-in | tokio-uring (separate) |
| **Scheduler** | Signal Tree (O(1)) | Work-stealing deque |
| **High Core Scaling** | 512+ cores | ~64 cores |
| **Timer Wheel** | 3-level | 2-level |
| **Async File I/O** | True async | Thread pool |
| **M:N Threading** | Explicit | 1:1 |
| **Raft Consensus** | Built-in | External |
| **sync_await** | Native | Not possible |
| **Waker Implementation** | DiatomicWaker | AtomicWaker |

### When to Choose maniac

- Need stackful coroutines (game engines, parsers)
- CPU-bound fairness required (preemption)
- Heavy file I/O workloads (io_uring)
- Building distributed systems (Raft)
- Bridging sync/async code (sync_await)
- High core count servers (64+ cores)

---

# Slide 27: Performance Characteristics

## Benchmarks

### Throughput

| Metric | Performance |
|--------|-------------|
| Task spawn/join | ~100M+ tasks/sec |
| MPSC channel | ~50M+ ops/sec |
| Timer operations | O(1) add/cancel |
| io_uring submissions | Batch syscalls |

### Latency

| Operation | Time |
|-----------|------|
| Task scheduling | ~100-500 ns |
| Waker notification | ~50-200 ns |
| Timer precision | ±1.05ms |
| Preemption overhead | ~1-5 μs |

### Memory

| Component | Overhead |
|-----------|----------|
| Per-task | ~1 KB (arena) + future size |
| Per-worker | ~1-16 MB (generator stack) |
| Empty channel | Zero allocation |

---

# Slide 28: Configuration API

## Runtime Builder

```rust
use maniac::runtime::{Runtime, TaskArenaConfig, TaskArenaOptions};

// Full control
let runtime = Runtime::new(
    TaskArenaConfig::new(
        8,      // leaf_count (8 × 4096 = 32K tasks)
        4096,   // tasks_per_leaf
    )?,
    TaskArenaOptions::new(
        true,   // use_huge_pages
        true,   // preinitialize
    ),
    num_cpus::get() - 1,  // worker_count
)?;

// Or use builder pattern
let runtime = RuntimeBuilder::new()
    .worker_count(8)
    .max_tasks(65536)
    .huge_pages(true)
    .build()?;

// Run async code
runtime.block_on(async {
    // Your async code here
});
```

---

# Slide 29: Spawning Tasks

## Task Creation API

### Basic Spawn

```rust
// Spawn and forget
maniac::spawn(async {
    do_work().await;
});

// Spawn and await result
let handle = maniac::spawn(async {
    compute().await
});
let result = handle.await?;
```

### Spawn Blocking

```rust
// Offload to thread pool
let result = maniac::blocking::unblock(|| {
    expensive_sync_operation()
}).await;
```

### Spawn Local (Same Worker)

```rust
// Stay on current worker (cache locality)
maniac::spawn_local(async {
    local_work().await;
});
```

---

# Slide 30: Timeout and Cancellation

## Time-Bounded Operations

### Timeout

```rust
use maniac::time::{timeout, Duration};

match timeout(Duration::from_secs(5), slow_operation()).await {
    Ok(result) => println!("Completed: {:?}", result),
    Err(_) => println!("Timed out!"),
}
```

### Sleep

```rust
use maniac::time::{sleep, Instant};

// Sleep for duration
sleep(Duration::from_millis(100)).await;

// Sleep until instant
sleep_until(Instant::now() + Duration::from_secs(1)).await;
```

### Interval

```rust
use maniac::time::interval;

let mut interval = interval(Duration::from_secs(1));
loop {
    interval.tick().await;
    periodic_task();
}
```

---

# Slide 31: Error Handling

## Robust Error Management

### Task Panics

```rust
// Panics are caught and converted to errors
let handle = maniac::spawn(async {
    panic!("Something went wrong!");
});

match handle.await {
    Ok(result) => println!("Success: {:?}", result),
    Err(e) if e.is_panic() => println!("Task panicked!"),
    Err(e) => println!("Other error: {:?}", e),
}
```

### Cancellation

```rust
let handle = maniac::spawn(async {
    long_running_task().await
});

// Cancel the task
handle.abort();

// Or let it drop (implicit cancel)
drop(handle);
```

---

# Slide 32: Use Cases

## Where maniac Excels

### 1. High-Concurrency Servers

```
10K+ concurrent connections
├── Work-stealing distributes load
├── io_uring minimizes syscalls
└── Preemption ensures fairness
```

### 2. Game Engines

```
Complex game logic
├── Stackful coroutines for state machines
├── sync_await bridges game/async code
└── Predictable latency via arena allocation
```

### 3. Distributed Systems

```
Multi-node clusters
├── Built-in Raft consensus
├── Multi-group support
└── Heartbeat coalescing
```

### 4. Real-Time Systems

```
Latency-critical applications
├── Preemptive scheduling
├── Huge page support
└── Pre-initialized arenas
```

---

# Slide 33: File System Operations

## Comprehensive Async FS

### Basic Operations

```rust
use maniac::fs::{File, OpenOptions};

// Read entire file
let content = maniac::fs::read("file.txt").await?;

// Write entire file
maniac::fs::write("file.txt", b"Hello").await?;

// Open with options
let file = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .open("file.txt")
    .await?;
```

### Advanced Operations (io_uring)

```rust
// Positioned I/O (no seek needed)
file.read_at(&mut buf, offset).await?;
file.write_at(&data, offset).await?;

// Zero-copy file transfer
maniac::fs::splice(src_fd, dst_fd, length).await?;

// Directory operations
maniac::fs::create_dir("new_dir").await?;
maniac::fs::remove_file("old_file").await?;
maniac::fs::rename("old", "new").await?;
```

---

# Slide 34: Compatibility Layers

## Ecosystem Integration

### Tokio Compatibility

```toml
[features]
tokio-compat = ["tokio"]
```

```rust
// Use tokio types with maniac runtime
use maniac::compat::tokio::TokioCompat;

let stream = TokioCompat::new(tokio_stream);
```

### Hyper (HTTP) Compatibility

```toml
[features]
hyper-compat = ["hyper"]
```

```rust
// Run Hyper server on maniac
use maniac::compat::hyper::HyperExecutor;

let server = hyper::Server::builder(incoming)
    .executor(HyperExecutor)
    .serve(make_service);
```

---

# Slide 35: Feature Flags

## Modular Design

### Default Features

```toml
[features]
default = [
    "std",
    "race",
    "mio",
    "iouring",
    "poll",
    "bytes",
    "utils",
    "async-cancel",
    "raft",
    "tls-rustls",
    "hyper-compat",
    "tokio-compat",
    "tracing",
    "serde",
]
```

### Optional Features

| Feature | Purpose |
|---------|---------|
| `iouring` | Linux io_uring support |
| `mio` | Portable epoll/kqueue |
| `raft` | Multi-Raft consensus |
| `tls-rustls` | Rustls TLS |
| `tls-native` | Native TLS |
| `hyper-compat` | HTTP server support |
| `tokio-compat` | Tokio interop |
| `tracing` | Structured logging |

---

# Slide 36: Testing & Debugging

## Development Tools

### Loom Testing

```rust
#[cfg(loom)]
mod tests {
    use loom::thread;

    #[test]
    fn test_concurrent_access() {
        loom::model(|| {
            // Test all possible interleavings
        });
    }
}
```

### Introspection Features

```toml
[features]
timers-introspection = []  # Timer statistics
track-allocations = []     # Verify zero-alloc
```

### Debug Mode

- Deadlock detection in debug builds
- Task state tracking
- Queue depth monitoring

---

# Slide 37: Memory Safety

## Rust's Guarantees + More

### Safety Mechanisms

- **Type-safe task storage** via arena
- **Epoch-based reclamation** via crossbeam
- **PhantomData** for correct variance
- **Atomic operations** for lock-free structures

### Unsafe Code Audit

```rust
// Documented unsafe blocks
// SAFETY: Worker guarantees exclusive access to current_task
unsafe {
    (*self.current_task).poll()
}
```

### Testing Coverage

- Unit tests per module
- Loom tests for concurrency
- MIRI compatibility (partial)
- Integration tests for executor

---

# Slide 38: Roadmap

## Future Development

### Near-Term

- [ ] NUMA-aware task placement
- [ ] Adaptive preemption threshold
- [ ] io_uring multishot operations
- [ ] Improved documentation

### Medium-Term

- [ ] WebAssembly support (cooperative only)
- [ ] Custom allocator integration
- [ ] Distributed tracing
- [ ] Metrics export (Prometheus)

### Long-Term

- [ ] Hardware offload (DPDK, RDMA)
- [ ] GPU task scheduling
- [ ] Formal verification of lock-free code

---

# Slide 39: Getting Started

## Quick Start

### Add Dependency

```toml
[dependencies]
maniac = "0.1"
```

### Hello World

```rust
#[maniac::main]
async fn main() {
    println!("Hello from maniac!");

    let result = maniac::spawn(async {
        42
    }).await.unwrap();

    println!("Result: {}", result);
}
```

### Run

```bash
cargo run --release
```

---

# Slide 40: Example - Echo Server

## Complete TCP Echo Server

```rust
use maniac::net::{TcpListener, TcpStream};
use std::io;

#[maniac::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Listening on :8080");

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("New connection: {}", addr);

        maniac::spawn(async move {
            if let Err(e) = handle(stream).await {
                eprintln!("Error: {}", e);
            }
        });
    }
}

async fn handle(mut stream: TcpStream) -> io::Result<()> {
    let mut buf = vec![0u8; 1024];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 { return Ok(()); }
        stream.write_all(&buf[..n]).await?;
    }
}
```

---

# Slide 41: Summary

## maniac in a Nutshell

### Core Innovations

| Innovation | Benefit |
|------------|---------|
| **Stackful Coroutines** | Deep call stacks, persistent state |
| **Preemption** | CPU-bound fairness, no starvation |
| **Signal Tree Scheduler** | O(1) work discovery, scales to 512+ cores |
| **sync_await** | Bridge sync callbacks to async I/O |
| **io_uring Native** | True async file I/O |
| **DiatomicWaker** | 40% fewer atomic operations |
| **3-Level Timer Wheel** | O(1) with 19+ hour coverage |

### Target Applications

- High-concurrency servers (10K+ connections)
- Distributed systems (built-in Raft)
- Game engines (stackful coroutines)
- Real-time systems (preemptive fairness)
- Heavy file I/O (io_uring)
- High core count machines (64-512+ cores)

---

# Slide 42: Why Choose maniac?

## Decision Matrix

| If You Need... | Choose |
|----------------|--------|
| Ecosystem compatibility | Tokio |
| Simple async | async-std |
| **Stackful coroutines** | **maniac** |
| **Preemptive scheduling** | **maniac** |
| **True async file I/O** | **maniac** |
| **sync_await bridging** | **maniac** |
| **Built-in Raft** | **maniac** |
| **High core count scaling** | **maniac** |

### The Bottom Line

**maniac** is for when you need more than what stackless async provides:
- True preemption for fairness
- Stackful coroutines for complex state
- sync_await for legacy integration
- io_uring for maximum I/O performance
- Signal Tree scheduler for 512+ core scaling

---

# Slide 43: Thank You

## Questions?

**maniac** - High-Performance Async Runtime for Rust

*Stackful Coroutines | Preemptive Scheduling | Signal Tree | io_uring*

---

Part of the **bisque-io/bop** ecosystem

GitHub: `bisque-io/bop/crates/maniac`

---

*Built with Rust for performance, safety, and control*
