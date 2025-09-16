# uSockets Rust Wrapper Documentation

## Overview

The `usockets.rs` module provides safe(ish) Rust wrappers around the uSockets (`us_*`) bindings from `bop-sys`. This module offers ergonomic types for event loops, timers, socket contexts, TCP sockets, and UDP sockets by installing static trampolines that dispatch to user-provided Rust closures stored in the respective extension memory areas.

## Safety Notes

- **Single-threaded execution**: Callbacks run on the uSockets loop thread; assume single-threaded `FnMut` closures
- **Extension memory management**: We store `Box` pointers in the `ext` area; do not mix these wrappers with manual `ext` usage on the same objects  
- **Unsafe FFI**: All FFI calls remain `unsafe` internally; wrappers aim to reduce, not remove, risk

## Error Handling

### `UsError` Enum

Comprehensive error type covering all uSockets operations:

```rust
pub enum UsError {
    LoopAlreadyRunning,
    LoopCreation { message: String },
    LoopExtNull,
    TimerCreation { message: String },
    TimerExtNull,
    SocketContextCreation { message: String },
    SocketContextExtNull,
    SocketExtNull,
    InvalidHostString { host: String },
    InvalidSourceHostString { host: String },
    NotOnLoopThread,
    LoopNotRunning,
    MutexLockFailed { operation: String },
    TimerLoopNull,
    AllocationFailed,
    NetworkOperation { operation: String },
    InvalidStringConversion,
}
```

## Core Components

### Event Loop

#### `Loop<T>`

The main event loop structure with optional user extension data.

**Creation:**
```rust
// Create with default extension data
let loop_ = Loop::new()?;

// Create with custom extension data
let loop_ = Loop::with_ext(my_data)?;
```

**Key Methods:**
- `handle() -> LoopHandle<T>`: Get a handle for cross-thread communication
- `create_timer() -> Result<Timer<T>, UsError>`: Create a timer on this loop
- `create_socket_context<SSL, S>() -> Result<SocketContext<SSL, T, S>, UsError>`: Create socket context
- `run(self) -> Result<(), UsError>`: Run the event loop to completion
- `run_simple(self) -> Result<(), UsError>`: Run without deferred callback processing

#### `LoopHandle<T>`

A thread-safe handle to communicate with the event loop from any thread.

**Key Methods:**
- `is_on_loop_thread() -> bool`: Check if current thread is the loop thread
- `is_loop_running() -> bool`: Check if loop is currently running
- `defer<F>(&self, callback: F)`: Queue callback for execution on loop thread

**Thread Safety:**
- `Send + Sync`: Can be safely shared between threads
- Callbacks are queued and executed on the next loop wakeup

#### `LoopMut<'a, T>`

Mutable reference to the loop, provided within callbacks.

**Key Methods:**
- `handle() -> LoopHandle<T>`: Get a handle to the loop
- `create_timer<S>() -> Result<Timer<S>, UsError>`: Create timer within callback
- `defer<F>(&self, callback: F)`: Defer additional callbacks

### Deferred Execution System

The loop implements a dual-queue system for thread-safe callback deferral:

- **Dual queues**: Two queues that alternate - one receives new callbacks while the other is being processed
- **Atomic queue swapping**: Thread-safe queue switching during wakeup
- **Automatic wakeup**: New callbacks trigger loop wakeup when needed

**Callback Types:**
```rust
pub enum LoopCallback<T> {
    Box(Box<dyn FnOnce(LoopMut<T>) + Send + 'static>),
    Arc(Arc<dyn Fn(LoopMut<T>) + Send + 'static>),
    Fn(fn(LoopMut<T>)),
}
```

### Timers

#### `Timer<T>`

High-precision timers integrated with the event loop.

**Creation:**
```rust
let mut timer = loop_.create_timer()?;
```

**Usage:**
```rust
// Set timer with callback
timer.set(1000, 0, |timer| {
    println!("Timer fired!");
})?;

// Reset timer without changing callback
timer.reset(2000, 1000)?;

// Close timer explicitly
timer.close()?;
```

**Key Methods:**
- `set<F>(ms: i32, repeat_ms: i32, cb: F)`: Set timer with callback
- `reset(ms: i32, repeat_ms: i32)`: Reset timer duration
- `close(self)`: Explicitly close timer (must be on loop thread)
- `ext() -> Option<&T>`: Access user extension data

### Socket Contexts

#### `SocketContext<SSL, T, S>`

Manages socket configurations and callbacks. Generic over:
- `SSL`: Boolean flag for SSL/TLS support
- `T`: Loop extension data type  
- `S`: Socket extension data type

**Creation:**
```rust
let ctx = loop_.create_socket_context::<false, MySocketData>(options)?;
```

**Configuration:**
```rust
pub struct SocketContextOptions {
    pub key_file_name: Option<String>,
    pub cert_file_name: Option<String>,
    pub passphrase: Option<String>,
    pub dh_params_file_name: Option<String>,
    pub ca_file_name: Option<String>,
    pub ssl_ciphers: Option<String>,
    pub ssl_prefer_low_memory_usage: bool,
}
```

**Server Operations:**
```rust
// Listen for connections
let listen_socket = ctx.listen("127.0.0.1", 8080, 0)?;

// Connect to remote server
let socket = ctx.connect("example.com", 80, None, 0)?;
```

**Callback System:**
The context manages various socket event callbacks through internal trampolines:
- `on_pre_open`: Called before socket is opened
- `on_open`: Called when socket opens (client/server)
- `on_close`: Called when socket closes
- `on_data`: Called when data is received
- `on_writable`: Called when socket is ready for writing
- `on_timeout`: Called on socket timeout
- `on_long_timeout`: Called on long timeout
- `on_connect_error`: Called on connection failure
- `on_end`: Called when socket ends gracefully
- `on_server_name`: Called for SNI in SSL contexts

### Sockets

#### `Socket<SSL, S>`

Represents an individual TCP socket connection.

**Key Methods:**

**Writing Data:**
```rust
// Write data with optional MSG_MORE flag
let bytes_written = socket.write(data, msg_more);

// Write header + payload in single operation
let bytes_written = socket.write2(header, payload);

// Flush pending writes
socket.flush();
```

**Connection Management:**
```rust
// Graceful shutdown
socket.shutdown();

// Shutdown read side only
socket.shutdown_read();

// Force close with code
socket.close(code);
```

**State Queries:**
```rust
let is_closed = socket.is_closed();
let is_shut_down = socket.is_shut_down();
let local_port = socket.local_port();
let remote_port = socket.remote_port();
let remote_addr = socket.remote_address();
```

**Timeouts:**
```rust
socket.set_timeout(30); // 30 seconds
socket.set_long_timeout(5); // 5 minutes
```

#### `ListenSocket<SSL, T>`

Represents a listening socket for accepting connections.

**Key Methods:**
- `as_ptr() -> *mut sys::us_listen_socket_t`: Get raw pointer
- `close(self)`: Close the listening socket

### UDP Support (Feature-gated)

#### `UdpSocket` and `UdpPacketBuffer`

UDP functionality is available when the `usockets-udp` feature is enabled.

**Packet Buffer:**
```rust
let mut buffer = UdpPacketBuffer::new()?;
let payload = buffer.payload_mut(0); // Get mutable slice to packet data
```

**UDP Socket:**
```rust
let udp_socket = UdpSocket::create(
    &loop_,
    &mut buffer,
    Some("127.0.0.1"),
    8080,
    Some(Box::new(|socket, buffer, num_packets| {
        // Handle received data
    })),
    Some(Box::new(|socket| {
        // Handle drain event
    }))
)?;

// Bind to address
udp_socket.bind("0.0.0.0", 8080);

// Send packets
let packets_sent = udp_socket.send(&mut buffer, num_packets);

// Receive packets
let packets_received = udp_socket.receive(&mut buffer);
```

## Extension Memory System

The wrapper uses a sophisticated extension memory system to store user data and callbacks:

### Direct Extension Storage

Instead of using `Box` pointers, the wrapper stores data structures directly in uSockets extension memory:

```rust
// Utility functions for type-safe ext memory access
unsafe fn ext_write<T>(ext: *mut c_void, value: T);
unsafe fn ext_read<T: Copy>(ext: *mut c_void) -> T;
unsafe fn ext_as_mut<'a, T>(ext: *mut c_void) -> &'a mut T;
unsafe fn ext_as_ref<'a, T>(ext: *mut c_void) -> &'a T;
```

### State Structures

Each major component has an associated state structure:

- `LoopState<T>`: Stores loop configuration, deferred callbacks, and user data
- `TimerState<T>`: Stores timer configuration, callback, and user data  
- `SocketContextState<SSL, T, S>`: Stores callbacks and context user data
- `SocketState<S>`: Stores socket metadata and user data

## Thread Safety

### Thread ID Tracking

The loop tracks which thread it's running on using atomic operations:

```rust
fn thread_id_to_u64(id: thread::ThreadId) -> u64;

// Stored in LoopState
loop_thread_id: AtomicU64, // 0 = not running
```

### Validation Methods

Several methods ensure operations happen on the correct thread:

- `is_on_loop_thread() -> bool`: Check if current thread is the loop thread
- `is_loop_running() -> bool`: Check if loop is actively running
- `is_valid() -> bool`: Comprehensive validity check for loop operations

### Cross-thread Communication

The `LoopHandle` enables safe cross-thread communication:

```rust
// From any thread
loop_handle.defer(|loop_mut| {
    // This runs on the loop thread
    println!("Executing on loop thread");
});
```

## Memory Management

### Automatic Cleanup

The wrapper implements `Drop` traits for automatic cleanup:

- Loops free their internal state and uSockets resources
- Timers close themselves and drop callbacks
- Socket contexts close and free resources
- Sockets close connections if not already closed

### Manual Cleanup

Some operations support explicit cleanup:

```rust
// Explicit timer close (preferred within callbacks)
timer.close()?;

// Explicit socket close
socket.close(error_code);
```

## Usage Patterns

### Basic Server

```rust
use usockets::*;

// Create loop
let loop_ = Loop::new()?;

// Create socket context
let mut ctx = loop_.create_socket_context::<false, ()>(SocketContextOptions::default())?;

// Set up callbacks (implementation details omitted for brevity)
// ctx.on_open(...);
// ctx.on_data(...);

// Start listening
let listen_socket = ctx.listen("127.0.0.1", 8080, 0)?;

// Run event loop
loop_.run()?;
```

### Client Connection

```rust
// Create socket context
let ctx = loop_.create_socket_context::<false, ()>(SocketContextOptions::default())?;

// Connect to server
if let Some(mut socket) = ctx.connect("example.com", 80, None, 0)? {
    socket.write(b"GET / HTTP/1.1\r\n\r\n", false);
}

// Run loop to handle connection
loop_.run()?;
```

### Cross-thread Operations

```rust
let handle = loop_.handle();

// From another thread
std::thread::spawn(move || {
    handle.defer(|loop_mut| {
        // Create timer on loop thread
        let mut timer = loop_mut.create_timer()?;
        timer.set(1000, 0, |_| println!("Timer from other thread!"))?;
    });
});
```

## Implementation Notes

### FFI Safety

- All FFI calls are wrapped in `unsafe` blocks
- Raw pointers are validated before dereferencing
- Extension memory is carefully managed to prevent leaks

### Performance Considerations

- Direct extension memory storage reduces allocations
- Dual-queue system minimizes lock contention for deferred callbacks
- Zero-copy data handling where possible

### SSL/TLS Support

The wrapper supports both plain TCP and SSL/TLS sockets through the generic `SSL` const parameter:

- `SocketContext<false, T, S>`: Plain TCP
- `SocketContext<true, T, S>`: SSL/TLS with certificate configuration

## Error Handling Best Practices

1. **Always handle `Result` types**: Most operations return `Result<T, UsError>`
2. **Check thread context**: Use `is_valid()` or `is_on_loop_thread()` before operations
3. **Explicit cleanup**: Use `close()` methods when appropriate
4. **Resource management**: Let `Drop` implementations handle cleanup when possible

## Limitations

1. **Single-threaded callbacks**: All callbacks execute on the loop thread
2. **Extension memory mixing**: Don't mix wrapper usage with manual `ext` operations
3. **Thread validation**: Many operations must be performed on the loop thread
4. **UDP feature dependency**: UDP functionality requires the `usockets-udp` feature

## Future Improvements

The current implementation provides a solid foundation but could benefit from:

1. **Async/await integration**: Bridge to Rust's async ecosystem
2. **Enhanced error context**: More detailed error information
3. **Performance metrics**: Built-in monitoring and profiling
4. **Memory pool optimization**: Reduce allocation overhead
5. **Documentation expansion**: More examples and use cases