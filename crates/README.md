# BOP Rust Ecosystem

This directory contains the Rust components of the BOP (Bisque Orchestration Platform) - a high-performance distributed systems platform built on modern C++23 infrastructure.

## Architecture Overview

The Rust ecosystem follows a layered architecture with clear separation of concerns:

```
┌─────────────────────────────────────────────────────────────┐
│                    vertx-cluster                            │
│  (High-level clustering & state management)                │
│  • Distributed state machine                               │
│  • Wire protocol & serialization                           │
│  • Cluster management & coordination                       │
│  • Vert.x compatibility layer                              │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      bop-rs                                 │
│  (Safe Rust wrappers for BOP platform)                     │
│  • Raft consensus wrappers                                 │
│  • uWebSockets HTTP/WebSocket/TCP                          │
│  • Custom memory allocator                                 │
│  • Type-safe, memory-safe abstractions                     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     bop-sys                                 │
│  (Raw FFI bindings to C/C++ libraries)                     │
│  • NuRaft consensus (bop_raft_*)                          │
│  • uWebSockets networking (uws_*)                          │
│  • MDBX storage (bop_mdbx_*)                               │
│  • Memory allocation (bop_alloc_*)                         │
│  • Build scripts & C++ integration                        │
└─────────────────────────────────────────────────────────────┘
```

## Crates Description

### 🪛 `bop-allocator` – Global Allocator
- Custom global allocator backed by BOP's high-performance allocation primitives
- Optional `alloc-stats` feature exposes `BopStatsAllocator` and `AllocationStats`
- Examples: `examples/global_allocator.rs`, `examples/stats_allocator.rs`

### 📦 `bop-mpmc` – Concurrent Queue
- Safe wrappers over the Moodycamel multi-producer/multi-consumer queue
- Producer/consumer tokens, bulk APIs, blocking utilities, and rich error types
- Includes integration tests plus practical usage notes in `docs/mpmc.md`

### 🗄️ `bop-mdbx` – Embedded Storage
- RAII environment/transaction/cursor wrappers around the `bop_sys::mdbx_*` API
- Focuses on ergonomic error handling and zero-cost access to MDBX primitives
- Ships small examples for cursor iteration and duplicate-fixed tables

### 📈 `bop-raft` – Consensus Surface
- Strongly-typed NuRaft façade (server config builders, log buffers, helper types)
- `thiserror` based error reporting for configuration, network, and log failures
- Bundled unit tests cover the lightweight type wrappers

### 🌐 `bop-usockets` – Networking & uWS
- Ergonomic bindings for uSockets and uWebSockets with optional UDP support (`feature = "usockets-udp"`)
- Provides loop/timer/socket context primitives, high-level production socket utilities, and uWS HTTP/WebSocket helpers
- Test assets and the comprehensive `USOCKETS_TEST_PLAN.md` live alongside the crate

### 📀 `bop-aof` – Tiered Append-Only Store
- Owns the AOF2 tiered storage engine (flush pipeline, manifest log, tiered caching)
- Re-exported as `bop_rs::aof2` for backwards compatibility
- See `docs/aof2/*` for architecture notes, runbooks, and tracing guides

### 🛡️ `bop-rs` – Aggregated API Layer
- Thin façade re-exporting the specialised crates above so existing code can continue using `bop_rs::module::*`
- Centralises feature flags (`alloc-stats`, `usockets-udp`) and provides the legacy `greeting()` helper

### 🏗️ `bop-sys` – FFI Bindings Layer
- Raw `extern "C"` surfaces for NuRaft, uSockets/uWS, MDBX, the allocator, and ancillary tooling
- Build scripts handle linking the underlying C/C++ libraries across supported targets

### 🧭 `vertx-cluster` – Application Layer
- Vert.x-compatible clustering/state-management prototype built atop the Rust wrappers
- Includes FSM, wire protocol, and application builder modules with extensive documentation

## Build Requirements

### Dependencies

1. **Rust Toolchain**: Edition 2024
   ```bash
   rustup toolchain install nightly
   rustup default nightly
   ```

2. **C++ Build Environment**:
   - **xmake**: Build system for C++ components
   - **MSVC**: Windows C++ compiler (or GCC/Clang on Unix)
   - **OpenSSL/WolfSSL**: For TLS support

3. **Platform Libraries**:
   - **Windows**: libuv for async I/O
   - **Linux**: io_uring support (optional), epoll fallback
   - **macOS**: kqueue support

### Building

```bash
# Build entire workspace
cargo build --workspace

# Build specific crates
cargo build -p bop-sys      # FFI bindings only
cargo build -p bop-rs       # Safe wrappers
cargo build -p vertx-cluster # High-level application

# Run tests
cargo test --workspace

# Check code without building
cargo check --workspace
```

## Usage Examples

### Basic Cluster Application

```rust
use vertx_cluster::{App, builder};

// Simple application
let app = App::new();

// With configuration
let app = builder()
    .node_id(1)
    .address("127.0.0.1:8080")
    .build();

// Process cluster messages
let response = app.process_message(message, connection_id, node_id);
```

### HTTP Server with BOP

```rust
use bop_rs::usockets::{HttpApp, UwsResult};

fn main() -> UwsResult<()> {
    let mut app = HttpApp::new()?;
    
    app.get("/", |mut res, req| {
        res.header("Content-Type", "text/plain")?
           .end_str("Hello from BOP!", false)?;
    })?;
    
    app.listen(8080)?
       .run()
}
```

### Custom Memory Allocation

```rust
use bop_rs::allocator::BopAllocator;

#[global_allocator]
static GLOBAL: BopAllocator = BopAllocator;

fn main() {
    // All heap allocations now use BOP's high-performance allocator
    let vec = vec![1, 2, 3, 4, 5];
    let string = String::from("Optimized allocation!");
}
```

## Current Status

### ✅ **Completed**

- **Crate Structure**: Clean separation of FFI, wrappers, and application layers
- **Build Configuration**: Workspace setup with proper dependencies
- **Core Modules**: wire.rs, fsm.rs moved to vertx-cluster
- **Documentation**: Comprehensive API documentation and examples
- **Memory Allocator**: Complete global allocator implementation
- **HTTP/WebSocket Wrappers**: Full networking API coverage
- **Test Suites**: Unit and integration tests for all major components

### 🚧 **In Progress**

- **API Signature Fixes**: Resolving C API function signature mismatches in bop-rs
- **Raft Wrappers**: Completing Raft consensus integration (some compilation issues remain)
- **Rust 2024 Edition**: Adding required unsafe blocks for new edition compliance

### 📋 **TODO**

- **C++ Library Linking**: Resolve build linking with actual BOP C++ libraries
- **Performance Testing**: Benchmarks comparing to native C++ performance
- **Integration Testing**: End-to-end distributed system tests
- **Documentation**: Usage guides for distributed patterns

## Contributing

### Code Organization

- **FFI Layer** (`bop-sys`): Only add bindings, no safety logic
- **Wrapper Layer** (`bop-rs`): Focus on safety, ergonomics, and zero-cost abstractions  
- **Application Layer** (`vertx-cluster`): High-level business logic and protocols

### Development Guidelines

1. **Safety First**: All public APIs in `bop-rs` must be memory-safe
2. **Zero-Cost**: Abstractions should have minimal runtime overhead
3. **Rust Idioms**: Follow standard Rust patterns and conventions
4. **Error Handling**: Use `Result` types with descriptive error messages
5. **Documentation**: All public APIs must have examples and safety notes

## License

MIT OR Apache-2.0

---

**BOP Rust Ecosystem** - High-performance distributed systems in safe Rust 🚀