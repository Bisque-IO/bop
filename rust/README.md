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

### 🔧 `bop-sys` - FFI Bindings Layer

**Purpose**: Raw, unsafe FFI bindings to BOP's C++ libraries.

**Contents**:
- Raw `extern "C"` function declarations
- C struct definitions via bindgen
- Build scripts for linking C++ libraries
- No safety guarantees - purely mechanical bindings

**Key Libraries**:
- **NuRaft**: Distributed consensus (`bop_raft_*` functions)
- **uWebSockets**: High-performance networking (`uws_*` functions)  
- **MDBX**: Embedded key-value database (`bop_mdbx_*` functions)
- **snmalloc**: High-performance allocator (`bop_alloc_*` functions)

### 🛡️ `bop-rs` - Safe Wrappers Layer

**Purpose**: Idiomatic, memory-safe Rust wrappers around `bop-sys`.

**Modules**:
- **`allocator`**: Custom global allocator using BOP's high-performance allocator
- **`usockets`**: HTTP/WebSocket/TCP server and client wrappers with SSL support
- **`raft`**: Raft consensus wrappers with strong typing and RAII cleanup
- **`raft_integration`**: OpenRaft integration traits (when available)

**Key Features**:
- ✅ **Memory Safety**: RAII patterns with automatic resource cleanup
- ✅ **Type Safety**: Strong Rust types prevent C API misuse
- ✅ **Error Handling**: Comprehensive error types with `thiserror`
- ✅ **Zero-Cost**: Minimal overhead abstractions
- ✅ **Thread Safety**: Safe concurrent access patterns

### 🏗️ `vertx-cluster` - Application Layer

**Purpose**: High-level distributed clustering and state management.

**Modules**:
- **`fsm`**: Finite state machine for cluster state management
- **`wire`**: Binary wire protocol with CRC validation
- **`lib`**: Application builder and coordination logic

**Key Features**:
- 📡 **Message-driven Architecture**: Event-sourced state transitions
- 🔐 **Binary Protocol**: Efficient wire format with integrity checks
- 🎯 **Vert.x Compatibility**: Drop-in replacement for Vert.x cluster manager
- 📊 **State Management**: Distributed maps, sets, locks, and counters
- 🔄 **Consensus Integration**: Optional Raft backend for strong consistency

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