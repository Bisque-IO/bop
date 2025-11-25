# maniac-runtime TCP Examples

This directory contains working examples of TCP client and server applications using the `maniac-runtime` event loop.

## Examples

### 1. Complete Multi-Connection Echo Server (`tcp_echo_server_complete.rs`) ⭐ **Recommended**

A production-ready echo server that handles **multiple concurrent connections**.

**Features:**

- Multiple concurrent connections
- Per-connection 60-second idle timeout
- Non-blocking I/O with EventLoop + SingleWheel timers
- Dynamic connection acceptance
- Proper FD management

**Run:**

```bash
cargo run --example tcp_echo_server_complete
```

**Test:**

```bash
# Open multiple terminals and connect
nc 127.0.0.1 8080
# Type messages - they will be echoed back with [Conn N] prefix
```

### 2. Simple Single-Connection Echo Server (`tcp_echo_server_simple.rs`)

A minimal echo server for learning the basics - accepts **one** connection only.

**Features:**

- Single connection handling
- 60-second idle timeout
- Simplest possible EventLoop usage
- Great starting point for learning

**Run:**

```bash
cargo run --example tcp_echo_server_simple
```

**Test:**

```bash
nc 127.0.0.1 8080
```

### 3. TCP Client (`tcp_client.rs`)

A TCP client that connects, sends a message, and receives responses.

**Features:**

- Non-blocking TCP connection
- 30-second connection timeout
- Automatic message sending on connect
- Response printing

**Run:**

```bash
# First, start the echo server
cargo run --example tcp_echo_server_complete

# Then run the client (in another terminal)
cargo run --example tcp_client
```

### 4. Multi-Connection Architecture Demo (`tcp_echo_server_multi.rs`)

Demonstrates the architectural challenges of dynamic socket addition from within callbacks. **For educational purposes
only** - see `tcp_echo_server_complete.rs` for the working solution.

**Run:**

```bash
cargo run --example tcp_echo_server_multi
```

## Architecture Overview

These examples demonstrate the core maniac-runtime networking components:

### EventLoop

The `EventLoop` is the heart of the networking system:

```rust
// Create event loop with timer wheel support
let mut event_loop = EventLoop::with_timer_wheel()?;

// Add a socket
let token = event_loop.add_socket(
    fd,                      // File descriptor
    Box::new(MyHandler),     // Event handler
    Box::new(()),           // Extension data
)?;

// Set timeout (requires timer wheel)
event_loop.set_timeout(token, Duration::from_secs(30))?;

// Run the loop
event_loop.run()?;  // Run forever

// Or poll manually
event_loop.poll_once(Some(Duration::from_millis(100)))?;
```

### EventHandler Trait

All socket handlers must implement `EventHandler`:

```rust
impl EventHandler for MyHandler {
    fn on_readable(&mut self, socket: &mut Socket, buffer: &mut [u8]) {
        // Handle incoming data
    }

    fn on_writable(&mut self, socket: &mut Socket) {
        // Socket ready for writing
    }

    fn on_timeout(&mut self, socket: &mut Socket) {
        // Timeout expired
    }

    fn on_close(&mut self, socket: &mut Socket) {
        // Socket closed
    }
}
```

### Timer Integration

The examples use `EventLoop::with_timer_wheel()` which creates an integrated SingleWheel:

- **Tick resolution**: ~2.147 seconds (2^31 nanoseconds)
- **Wheel size**: 1024 ticks
- **Coverage**: ~34 minutes (2s × 1024)
- **Performance**: O(1) timer operations

## Key Concepts

### Non-blocking I/O

All sockets must be set to non-blocking mode:

```rust
stream.set_nonblocking(true)?;
```

### File Descriptor Management

The EventLoop doesn't own the underlying file descriptors. You must:

1. Keep the socket alive (or use `std::mem::forget`)
2. Handle cleanup properly on shutdown

### Low-Priority Processing

For CPU-intensive operations (like TLS handshakes), implement:

```rust
fn is_low_priority(&self) -> bool {
    true  // This handler will be budget-limited
}
```

This prevents starvation of other connections during expensive operations.

## Architecture Patterns

### Multi-Connection Server Pattern

See [`tcp_echo_server_complete.rs`](tcp_echo_server_complete.rs) for the recommended pattern:

1. **Non-blocking listener**: Keep `TcpListener` in non-blocking mode
2. **Accept loop**: Check for new connections before each `poll_once()`
3. **Dynamic addition**: Add new sockets to EventLoop between poll iterations
4. **Timeout management**: Set per-socket timeouts using SingleWheel

```rust
loop {
    // Accept new connections
    while let Ok((stream, _)) = listener.accept() {
        stream.set_nonblocking(true)?;
        let token = event_loop.add_socket(fd, handler, ext)?;
        event_loop.set_timeout(token, Duration::from_secs(60))?;
    }

    // Process events
    event_loop.poll_once(Some(Duration::from_millis(100)))?;
}
```

### Current Limitations

1. **Manual FD management**: Users must use `std::mem::forget` to prevent socket closure. A future RAII wrapper would
   improve this.

2. **No callback-based socket addition**: Event handlers cannot directly add new sockets to the EventLoop. Use the
   pattern above instead.

### Future Improvements

- **RAII socket wrappers**: Automatic FD lifecycle management
- **Socket addition queue**: API for handlers to request socket additions
- **TLS integration examples**: Demonstrate built-in rustls support

## Performance Characteristics

- **Zero allocations** in the event loop hot path (uses `unsafe` for direct iteration)
- **O(1) timer operations** via SingleWheel
- **Budget-limited low-priority processing** to prevent starvation
- **Shared receive buffer** (512KB) to reduce per-socket allocations

## Testing

Run all examples:

```bash
# Build all examples
cargo build --package maniac-runtime --examples

# List all examples
cargo run --package maniac-runtime --example
```

## See Also

- [EventLoop API documentation](../src/runtime/io_driver.rs)
- [SingleWheel timer documentation](../src/runtime/timer_wheel.rs)
- [TLS support documentation](../src/net/tls.rs)
