# EventLoop Integration with Worker System - Design Document

## Overview

This document describes the integration of `EventLoop` (maniac-runtime's networking layer) with the `Worker` and `WorkerWaker` systems to create a unified async I/O and task execution runtime.

## Current Architecture

### WorkerWaker (waker.rs)
- **Blocking mechanism**: EventLoop polling (`acquire_with_io()`)
- **Status tracking**: 64-bit status word with:
  - Bits 0-61: Queue summary (mpsc signals)
  - Bit 62: Partition has work (STATUS_BIT_PARTITION)
  - Bit 63: Yield queue has items (STATUS_BIT_YIELD)
- **Permit system**: Counting semaphore for wakeups
- **Waker integration**: Registers a `mio::Waker` to wake the EventLoop from other threads

### Worker (worker.rs)
Each worker thread contains:
- `timer_wheel`: TimerWheel for task timers
- `receiver`: mpsc::Receiver for WorkerMessage
- `yield_queue`: YieldWorker for yielded tasks
- `partition_start/end`: Assigned SummaryTree leaf range
- `current_task`: Pointer to currently executing task
- `event_loop`: Per-worker EventLoop for async I/O

### EventLoop (net/event_loop.rs)
- **Mio-based**: Cross-platform event notification
- **SingleWheel integration**: Optional owned SingleWheel for socket timeouts
- **Low-priority queue**: Prevents SSL starvation
- **Shared receive buffer**: 512KB buffer for zero-copy I/O
- **Waker support**: `WAKE_TOKEN` for cross-thread waking

## Integration Design

### Phase 1: Per-Worker EventLoop

Each Worker owns an EventLoop instance for I/O operations:

```rust
pub struct Worker<'a, const P: usize, const NUM_SEGS_P2: usize> {
    // ... existing fields ...

    /// Per-worker EventLoop for async I/O
    event_loop: EventLoop,

    /// I/O budget per iteration (prevent I/O starvation of tasks)
    io_budget: usize,
}
```

**Rationale:**
- Each worker handles its own I/O independently
- No cross-worker I/O contention
- Locality: tasks and their I/O stay on same worker
- Simpler than shared EventLoop across workers

### Phase 2: Replace Condvar/Mutex in WorkerWaker

Current blocking in WorkerWaker has been replaced with EventLoop polling:

```rust
// New implementation
pub fn acquire_with_io(&self, event_loop: &mut EventLoop) -> bool {
    // 1. Fast path: try to acquire permit immediately
    if self.try_acquire() {
        return true;
    }

    // 2. Block on EventLoop until I/O event or Waker notification
    match event_loop.poll_once(None) {
        Ok(_) => {
            // I/O events processed or Waker triggered
            // Check for permits again
            self.try_acquire()
        }
        Err(_) => false,
    }
}
```

**Key insight**: EventLoop.poll_once() replaces Condvar.wait() as the blocking mechanism. When a worker has no tasks to run, it blocks in poll_once() waiting for:
- Network I/O events (via Mio)
- Socket timeouts (via SingleWheel)
- Task wakeups (via permits -> Waker -> EventLoop)

### Phase 3: Socket Event → Task Waking Integration

When a socket becomes readable/writable, we need to wake the task waiting on it:

```rust
// EventHandler for async sockets
struct AsyncSocketHandler {
    task_waker: Waker,  // std::task::Waker for the waiting task
}

impl EventHandler for AsyncSocketHandler {
    fn on_readable(&mut self, socket: &mut Socket, buffer: &mut [u8]) {
        // Wake the task that's waiting on this socket
        self.task_waker.wake_by_ref();
    }

    fn on_writable(&mut self, socket: &mut Socket) {
        self.task_waker.wake_by_ref();
    }

    fn on_timeout(&mut self, socket: &mut Socket) {
        // Wake with timeout error
        self.task_waker.wake_by_ref();
    }
}
```

**Waker flow:**
1. Task calls `socket.read().await`
2. Future stores task's Waker in socket handler
3. Future returns Poll::Pending
4. Worker parks in EventLoop.poll_once()
5. Socket becomes readable → on_readable() → waker.wake()
6. Waker adds task to Worker's yield_queue
7. Worker wakes from poll_once(), processes yield queue
8. Task resumes execution

### Phase 4: WorkerMessage Extensions for I/O

Add new WorkerMessage variants for socket operations:

```rust
pub enum WorkerMessage {
    // ... existing variants ...

    /// Register a socket with this worker's EventLoop
    RegisterSocket {
        fd: SocketDescriptor,
        handler: Box<dyn EventHandler>,
        interest: Interest,  // Read, Write, or both
    },

    /// Modify socket interest (e.g., switch from Read to Write)
    ModifySocket {
        token: Token,
        interest: Interest,
    },

    /// Deregister and close a socket
    CloseSocket {
        token: Token,
    },

    /// Set socket timeout
    SetSocketTimeout {
        token: Token,
        duration: Duration,
    },
}
```

**Cross-worker socket operations:**
- Worker A creates socket, sends RegisterSocket to Worker B
- Worker B's EventLoop owns the socket
- Worker A can send ModifySocket/CloseSocket messages
- WorkerService routes messages via existing mpsc channels

### Phase 5: Future-based Networking API

Create ergonomic async/await API:

```rust
// Future-based TcpStream
pub struct TcpStream {
    token: Token,
    worker_id: usize,
    state: Arc<Mutex<TcpStreamState>>,
}

enum TcpStreamState {
    Connecting,
    Connected { fd: SocketDescriptor },
    Reading { waker: Option<Waker>, buffer: Vec<u8> },
    Writing { waker: Option<Waker>, buffer: Vec<u8>, written: usize },
    Closed,
}

impl TcpStream {
    pub async fn connect(addr: SocketAddr) -> io::Result<Self> {
        // 1. Create non-blocking socket
        // 2. Initiate connect()
        // 3. Send RegisterSocket to current worker
        // 4. Return Future that polls connection status
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        ReadFuture { stream: self, buf }.await
    }

    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        WriteFuture { stream: self, buf }.await
    }
}

// Future implementation
struct ReadFuture<'a> {
    stream: &'a mut TcpStream,
    buf: &'a mut [u8],
}

impl Future for ReadFuture<'_> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // 1. Try non-blocking read
        // 2. If WouldBlock, store cx.waker() in stream state
        // 3. Return Poll::Pending
        // 4. EventLoop will wake us when readable
    }
}
```

## Implementation Phases

### Phase 1: Foundation (Design + Worker struct changes)
- [x] Design document (this file)
- [x] Add EventLoop field to Worker struct
- [x] Modify Worker::new() to create EventLoop
- [x] Add io_budget field for fairness

### Phase 2: Blocking Integration
- [x] Add acquire_with_io() to WorkerWaker
- [x] Modify Worker run loop to use poll_once() instead of acquire()
- [x] Remove Condvar/Mutex from WorkerWaker (breaking change)
- [ ] Update tests

### Phase 3: Message Handlers
- [ ] Add socket WorkerMessage variants
- [ ] Implement RegisterSocket handler in Worker
- [ ] Implement ModifySocket handler
- [ ] Implement CloseSocket handler
- [ ] Add cross-worker socket routing

### Phase 4: Future API - TcpStream
- [ ] Create TcpStream struct with state machine
- [ ] Implement Future for connect()
- [ ] Implement Future for read()
- [ ] Implement Future for write()
- [ ] Add timeout support
- [ ] Tests for TcpStream

### Phase 5: Future API - TcpListener
- [ ] Create TcpListener struct
- [ ] Implement Future for accept()
- [ ] Handle multiple connections
- [ ] Tests for TcpListener

### Phase 6: Integration Testing
- [ ] End-to-end async echo server test
- [ ] End-to-end async client test
- [ ] Multi-connection stress test
- [ ] Timeout tests
- [ ] Cross-worker socket migration test

### Phase 7: Examples
- [ ] async_tcp_echo_server.rs - Future-based echo server
- [ ] async_tcp_client.rs - Future-based client
- [ ] async_http_server.rs - Basic HTTP server
- [ ] Update examples/README.md

## Performance Considerations

### Zero-Copy I/O
- EventLoop already has 512KB shared receive buffer
- Futures can read directly into user-provided buffers
- No intermediate allocations

### Fairness
- Worker run loop: `poll_timers()` → `process_messages()` → `poll_yield()` → `try_partition()`
- Add: `poll_io()` with budget limit
- Prevents I/O from starving task execution

### Low-Priority I/O
- EventLoop already has low-priority queue for SSL handshakes
- Budget-limited processing prevents one slow connection from blocking others

### Timer Integration
- Worker already has TimerWheel for task timers
- EventLoop has SingleWheel for socket timeouts
- Keep separate: task timers vs socket timeouts
- Different domains, different resolution needs
- No benefit to unifying them

## Breaking Changes

### WorkerWaker API
- Removed `m: Mutex<()>` field
- Removed `cv: Condvar` field
- `acquire()` replaced by `acquire_with_io(&self, event_loop: &mut EventLoop) -> bool`
- `acquire_timeout()` replaced by `acquire_timeout_with_io(&self, event_loop: &mut EventLoop, timeout: Duration) -> bool`

## Open Questions

### Q1: Should EventLoop be per-worker or shared?
**Decision: Per-worker**
- Simpler: no coordination needed
- Better locality: task and its I/O stay together
- No contention on EventLoop mutex
- Downside: more EventLoops (more OS resources)

### Q2: How to handle socket migration between workers?
**Decision: Deregister + Reregister**
- Worker A: deregister socket from EventLoop
- Worker A: send RegisterSocket message to Worker B
- Worker B: register socket with its EventLoop
- File descriptor stays open, just changes ownership
- Rare operation, simplicity over performance

### Q3: How to integrate with existing TimerWheel?
**Decision: Keep separate**
- Worker.timer_wheel: task timers (sleep, timeout)
- EventLoop.timer_wheel: socket timeouts (idle, read/write timeout)
- Different domains, different resolution needs
- No benefit to unifying them

### Q4: Should we remove Condvar entirely or keep as fallback?
**Decision: Removed**
- EventLoop.poll_once() provides equivalent blocking
- Simpler code with one blocking mechanism
- Provides I/O awareness that Condvar lacks

## Alternative Approaches Considered

### Alt 1: Shared Global EventLoop
- **Rejected**: Contention, complex coordination, poor locality

### Alt 2: Dedicated I/O Worker Pool
- **Rejected**: Adds latency (cross-worker communication), complexity

### Alt 3: Callback-based API (not Future-based)
- **Rejected**: Not idiomatic Rust, harder to use, no async/await

### Alt 4: Integration with tokio/async-std
- **Rejected**: Incompatible with generator-based tasks, too heavyweight

## Success Criteria

1. **Ergonomic API**: `async fn` networking feels natural
2. **Zero allocations**: Hot path has no allocations (like EventLoop today)
3. **Fairness**: I/O doesn't starve task execution
4. **Performance**: Comparable to tokio for I/O-bound workloads
5. **Integration**: Works seamlessly with existing task system
6. **Tests**: >90% coverage, stress tests pass

## References

- [EventLoop implementation](../src/net/event_loop.rs)
- [SingleWheel timer](../src/runtime/timer_wheel.rs)
- [WorkerWaker](../src/runtime/waker.rs)
- [Worker](../src/runtime/worker.rs)
- [TCP examples](../examples/README.md)
