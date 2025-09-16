# uSockets.rs Design Document

## Overview

The `usockets.rs` module provides safe(ish) Rust wrappers around the uSockets C library bindings from `bop-sys`. It offers ergonomic, type-safe interfaces for core uSockets functionality including event loops, timers, TCP sockets, SSL/TLS support, and UDP sockets (optional).

### Purpose

This module serves as a bridge between the high-performance uSockets C library and Rust applications, providing:

- **Type Safety**: Generic types for user data storage and retrieval
- **Memory Safety**: Automatic resource management with RAII patterns
- **Ergonomic API**: Rust-idiomatic interfaces with proper error handling
- **Thread Safety**: Thread validation and synchronization primitives
- **Callback Safety**: Trampoline-based callback dispatch with proper lifetime management

### Safety Model

The module implements a "safe(ish)" approach rather than fully safe Rust:

#### Safety Guarantees
- **Memory Management**: Automatic cleanup of uSockets resources via `Drop` implementations
- **Type Safety**: Generic parameters ensure type-safe access to user data
- **Thread Validation**: Runtime checks ensure operations occur on the correct event loop thread
- **Resource Ownership**: Clear ownership semantics for sockets, contexts, and loops

#### Safety Limitations
- **FFI Boundaries**: All underlying uSockets calls remain `unsafe` internally
- **Callback Execution**: Callbacks run on the uSockets event loop thread with `FnMut` assumptions
- **Extension Area**: Manual memory management for the uSockets extension area
- **Thread Assumptions**: Single-threaded execution model assumed for event loop operations

#### Critical Safety Notes
1. **Thread Model**: All callbacks execute on the event loop thread; assume single-threaded `FnMut`
2. **Extension Mixing**: Do not mix these wrappers with manual extension area usage
3. **Resource Lifetime**: Ensure proper cleanup order (sockets before contexts before loops)

## Core Architecture

### Extension Area Pattern

The module heavily utilizes uSockets' "extension area" - a configurable memory region attached to each uSockets object (loops, sockets, contexts) for storing user data.

```rust
// Extension area stores Rust state directly
unsafe fn ext_write<T>(ext: *mut c_void, value: T) {
    (ext as *mut T).write(value);
}

unsafe fn ext_read<T: Copy>(ext: *mut c_void) -> T {
    (ext as *mut T).read()
}
```

**Benefits:**
- Zero-copy access to user data
- Type-safe storage and retrieval
- Automatic cleanup via `Drop` implementations
- No additional heap allocations for state

### Generic Type System

The module uses const generics and type parameters for compile-time configuration:

```rust
pub struct SocketContext<const SSL: bool, T = (), S = ()> {
    // SSL: Compile-time SSL enable/disable
    // T: Context-level user data type
    // S: Socket-level user data type
}
```

**Design Benefits:**
- **SSL Configuration**: `const SSL: bool` enables compile-time SSL specialization
- **User Data**: Generic parameters `T` and `S` provide type-safe user data storage
- **Zero Cost**: No runtime overhead for generic instantiations
- **Type Safety**: Compile-time guarantees for data access

### Callback System

The module implements a trampoline-based callback system for type-safe callback dispatch:

```rust
unsafe extern "C" fn trampoline_on_data(s: *mut sys::us_socket_t, data: *mut c_char, len: c_int) {
    if let Some((ctx_state, callbacks, socket_state)) = Self::socket_and_context_state(s) {
        if let Some(cb) = callbacks.on_data.as_mut() {
            let slice = if !data.is_null() && len > 0 {
                unsafe { std::slice::from_raw_parts_mut(data as *mut u8, len as usize) }
            } else { &mut [] };
            cb(ctx_state, &mut socket, socket_state, slice);
        }
    }
}
```

**Key Features:**
- **Type Erasure**: Callbacks stored as `Box<dyn FnMut>` for flexibility
- **State Access**: Automatic provision of context and socket state to callbacks
- **Memory Safety**: Proper lifetime management for callback data
- **Thread Safety**: Callbacks execute on event loop thread

## Core Types

### Loop<T>

The `Loop<T>` type represents a uSockets event loop with optional user data.

```rust
pub struct Loop<T = ()> {
    ptr: *mut sys::us_loop_t,
    _phantom: PhantomData<T>,
}
```

**Key Features:**
- **Generic User Data**: Type `T` for storing loop-level state
- **Deferred Callbacks**: Thread-safe callback queuing system
- **Thread Tracking**: Atomic tracking of the loop thread
- **Resource Management**: Automatic cleanup of associated resources

**Design Patterns:**
- **RAII**: Automatic resource cleanup via `Drop`
- **Thread Validation**: Runtime checks for loop thread operations
- **Deferred Execution**: Queue system for cross-thread callback execution

### Timer<T>

Timers provide periodic or one-shot callback execution tied to a specific loop.

```rust
pub struct Timer<T> {
    ptr: *mut sys::us_timer_t,
    _phantom: PhantomData<T>,
}
```

**Key Features:**
- **Loop Association**: Timers are bound to specific event loops
- **Callback Storage**: Type-safe callback storage in extension area
- **Thread Safety**: Automatic thread validation
- **Resource Management**: Proper cleanup and loop integration

### SocketContext<SSL, T, S>

Socket contexts manage socket creation and configuration.

```rust
pub struct SocketContext<const SSL: bool, T = (), S = ()> {
    ptr: *mut sys::us_socket_context_t,
    _phantom: PhantomData<(T, S)>,
}
```

**Key Features:**
- **SSL Configuration**: Compile-time SSL enable/disable
- **Dual User Data**: Context-level (`T`) and socket-level (`S`) data types
- **Callback Management**: Centralized callback registration
- **Socket Factory**: Creates TCP and listening sockets

**Design Benefits:**
- **Type Safety**: Separate types for context and socket data
- **SSL Specialization**: Zero-cost SSL configuration
- **Callback Centralization**: All socket callbacks managed at context level

### Socket<SSL, S>

Individual sockets represent TCP connections.

```rust
pub struct Socket<const SSL: bool, S = ()> {
    ptr: *mut sys::us_socket_t,
    _phantom: PhantomData<S>,
}
```

**Key Features:**
- **SSL Support**: Compile-time SSL configuration
- **User Data**: Socket-specific state storage
- **I/O Operations**: Read/write with proper error handling
- **Connection Management**: Timeout, shutdown, and close operations

## Error Handling

The module defines a comprehensive `UsError` enum for all error conditions:

```rust
#[derive(Error, Debug)]
pub enum UsError {
    #[error("Loop creation failed: {message}")]
    LoopCreation { message: String },

    #[error("Timer creation failed: {message}")]
    TimerCreation { message: String },

    // ... additional variants
}
```

**Error Categories:**
- **Resource Creation**: Failures in creating loops, timers, contexts
- **Thread Safety**: Operations attempted on wrong thread
- **Network Operations**: Connection and I/O failures
- **State Validation**: Invalid object states
- **Memory Management**: Allocation and extension area issues

## Thread Safety and Synchronization

### Thread Model

The module operates under a single-threaded event loop model:

- **Event Loop Thread**: All I/O and callbacks execute on the loop thread
- **Thread Validation**: Runtime checks ensure operations occur on correct thread
- **Cross-Thread Communication**: Deferred callback system for thread-safe communication

### Synchronization Primitives

```rust
struct LoopState<T> {
    // Deferred callback queues protected by mutex
    deferred: Arc<Mutex<Deferred>>,
    // Atomic thread tracking
    loop_thread_id: AtomicU64,
    // ... other fields
}
```

**Key Mechanisms:**
- **Mutex Protection**: Deferred queues use `Mutex` for thread safety
- **Atomic Operations**: Thread ID tracking uses `AtomicU64`
- **Arc Sharing**: Shared state across threads via `Arc`

### Deferred Callback System

The deferred callback system enables thread-safe communication with the event loop:

```rust
pub fn defer<F>(&self, callback: F) -> Result<(), UsError>
where
    F: FnOnce() + Send + 'static
{
    // Queue callback for execution on loop thread
    // Wake loop if necessary
}
```

**Features:**
- **Thread Safety**: Callbacks can be queued from any thread
- **Execution Ordering**: FIFO execution order
- **Wakeup Mechanism**: Automatic loop wakeup when callbacks are queued
- **Memory Safety**: Proper ownership transfer of callbacks

## SSL/TLS Support

SSL support is implemented via const generics for zero-cost abstraction:

```rust
// SSL-enabled context
let ctx: SocketContext<true, MyData> = SocketContext::new(&loop_, options)?;

// Non-SSL context
let ctx: SocketContext<false, MyData> = SocketContext::new(&loop_, options)?;
```

**Design Benefits:**
- **Zero Cost**: No runtime overhead for SSL configuration
- **Type Safety**: Compile-time SSL guarantees
- **API Consistency**: Same API for SSL and non-SSL sockets
- **FFI Integration**: Direct mapping to uSockets SSL functions

## UDP Support (Optional)

UDP functionality is gated behind a feature flag:

```rust
#[cfg(feature = "usockets-udp")]
pub struct UdpSocket {
    ptr: *mut sys::us_udp_socket_t,
}
```

**Features:**
- **Packet Buffers**: Efficient packet buffer management
- **Callback System**: Data and drain callbacks
- **Bind/Connect**: Standard UDP socket operations
- **Send/Receive**: Packet-based I/O operations

## Usage Patterns

### Basic TCP Server

```rust
// Create event loop with user data
let mut loop_: Loop<MyLoopData> = Loop::new()?;

// Create socket context
let mut ctx: SocketContext<false, MyCtxData, MySocketData> =
    SocketContext::new(&loop_, SocketContextOptions::default())?;

// Set up callbacks
// ... callback setup ...

// Start listening
let listener = ctx.listen("127.0.0.1", 8080, 0)?;

// Run event loop
loop_.run();
```

### Deferred Callbacks

```rust
// Queue callback from any thread
loop_.defer(|| {
    println!("Executed on loop thread");
})?;

// Or run inline if already on loop thread
loop_.run_on_loop(|| {
    println!("Inline execution");
})?;
```

## Design Rationale and Trade-offs

### Extension Area Usage

**Rationale:**
- uSockets provides extension area for user data storage
- Avoids additional heap allocations
- Enables zero-copy access to user data

**Trade-offs:**
- Manual memory management required
- Unsafe code for extension area access
- Limited to single pointer-sized region per object

### Const Generics for SSL

**Rationale:**
- Compile-time SSL configuration
- Zero runtime overhead
- Type safety for SSL operations

**Trade-offs:**
- Monomorphization increases binary size
- Cannot change SSL mode at runtime
- Requires explicit type annotations

### Trampoline-Based Callbacks

**Rationale:**
- Type-safe callback dispatch
- Automatic state management
- Rust-idiomatic callback signatures

**Trade-offs:**
- Function pointer overhead
- Type erasure reduces static dispatch
- Complex unsafe code for FFI boundaries

### Single-Threaded Assumption

**Rationale:**
- uSockets is single-threaded by design
- Simplifies synchronization
- Matches underlying C library model

**Trade-offs:**
- Cannot utilize multiple cores for I/O
- All callbacks execute on single thread
- Cross-thread communication requires deferral

## Future Considerations

### Potential Improvements

1. **Async/Await Integration**: Futures-based API for modern Rust
2. **Multi-Threading**: Support for multi-threaded uSockets configurations
3. **Zero-Copy Optimizations**: Further reduce memory allocations
4. **Advanced SSL**: Certificate pinning, custom validation
5. **Metrics/Observability**: Built-in performance monitoring

### Compatibility

- **Rust Version**: Requires Rust 1.51+ for const generics
- **uSockets Version**: Compatible with uSockets 0.8+
- **Platform Support**: Windows, Linux, macOS via bop-sys bindings

## Conclusion

The `usockets.rs` module provides a comprehensive, safe(ish) Rust interface to the uSockets library while maintaining high performance and low overhead. The design balances safety, ergonomics, and performance through careful use of Rust's type system, const generics, and strategic unsafe code blocks.

The extension area pattern, trampoline callbacks, and deferred execution system form the core architectural elements that enable type-safe, thread-aware networking operations with minimal runtime overhead.