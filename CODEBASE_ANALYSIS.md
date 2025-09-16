# Codebase Analysis

This document provides an analysis of the bop repository's codebase, focusing on its structure, build process, dependencies, and testing strategies.

## Project Overview

The bop repository is a data platform for modern computing that implements a comprehensive networking and storage stack. It provides high-performance components for HTTP/WebSocket servers, TCP networking, distributed consensus (Raft), memory management, and database operations.

## Core Architecture

### Primary Components

1. **uWS (Micro WebSocket)** - High-performance HTTP/WebSocket server implementation
2. **Raft** - Distributed consensus algorithm implementation
3. **MPMC** - Multi-producer multi-consumer queue implementation
4. **SPSC** - Single-producer single-consumer queue implementation
5. **Alloc** - Memory allocation and management system
6. **Hashing** - Cryptographic and general-purpose hashing utilities

### Language Implementations

The project supports multiple programming languages:
- **C/C++23** - Primary implementation in `lib/src/`
- **Rust** - Rust bindings and implementations in `rust/bop-rs/`
- **Odin** - Odin language bindings in `odin/`
- **Go** - Go language support in `go/`
- **JVM** - Java/JVM support in `jvm/`

## Directory Structure

```
bop/
├── lib/src/           # Core C++23 implementations
│   ├── uws.cpp/h      # WebSocket/HTTP server
│   ├── raft.cpp/h     # Distributed consensus
│   ├── mpmc.cpp/h     # Multi-producer queue
│   ├── spsc.cpp/h     # Single-producer queue
│   ├── alloc.cpp/h    # Memory allocation
│   └── hash.cpp/h     # Hashing utilities
├── lib/               # Vendor dependencies
│   ├── usockets/      # Socket abstraction layer
│   ├── libuv/         # Event loop library
│   ├── wolfssl/       # SSL/TLS implementation
│   ├── snmalloc/      # Memory allocator
│   └── mdbx/          # Database engine
├── tests/             # Test suite
├── rust/bop-rs/       # Rust implementation
├── odin/              # Odin language bindings
├── docs/              # Documentation
└── build configs      # xmake.lua, CMakeLists.txt
```

## Build System

The project uses a dual build system approach:

### xmake (Primary)
- Configuration: `xmake.lua`
- Build command: `xmake f -m release && xmake`
- Test command: `xmake run test-uws`

### CMake (Alternative)
- Configuration: `CMakeLists.txt`
- Build command: `cmake -S . -B build && cmake --build build -j`

## Dependencies

### Core Dependencies
- **usockets** - Cross-platform socket abstraction
- **libuv** - Event loop and I/O operations
- **wolfssl** - SSL/TLS encryption
- **snmalloc** - High-performance memory allocator
- **mdbx** - Lightning-fast database engine

### Compression Libraries
- **zlib** - Data compression
- **zstd** - High-performance compression
- **brotli** - Web-optimized compression

### Build Tools
- **Boost** - C++ libraries (filesystem, system, etc.)
- **OpenSSL** - Alternative SSL implementation
- **c-ares** - Asynchronous DNS resolution
- **asio** - Networking and I/O

## Testing Framework

### Test Structure
- **Location**: `tests/` directory
- **Runner**: Custom `TestRunner.cpp` implementation
- **Framework**: Uses doctest for C++ testing
- **Naming**: `NameTest.cpp` pattern

### Test Categories
1. **Unit Tests** - Component-specific functionality
2. **Integration Tests** - Cross-component interactions
3. **Performance Tests** - Benchmarking and load testing
4. **Protocol Tests** - WebSocket/HTTP compliance

### Test Execution
- Single test: `xmake run test-uws`
- All tests: CTest in `tests/build`
- Custom runner with detailed reporting

## Code Style and Standards

### Naming Conventions
- **Files**: `snake_case.cpp/h`
- **Types**: `PascalCase`
- **Functions/Variables**: `lower_snake_case`
- **Constants**: `UPPER_SNAKE_CASE`

### Language Standards
- **C++23** - Modern C++ features
- **Formatting**: `.clang-format` configuration
- **Error Handling**: Explicit error codes over exceptions
- **Includes**: Relative paths in `lib/src/**`, minimal includes

## Performance Characteristics

### Optimization Features
- **Memory Management**: Custom allocators (snmalloc)
- **I/O Optimization**: io_uring support on Linux
- **Compression**: Multiple compression algorithms
- **SSL/TLS**: Hardware-accelerated encryption
- **Event Loop**: Multi-threaded event processing

### Platform Support
- **Windows** - Full support with MSVC
- **Linux** - Primary development platform
- **macOS** - Full support
- **ARM64** - Cross-platform ARM support
- **RISC-V** - Experimental RISC-V support

## Key Features

### Networking
- HTTP/1.1 and HTTP/2 support
- WebSocket protocol implementation
- SSL/TLS encryption with SNI
- TCP and UDP socket operations
- IPv4 and IPv6 support

### Storage
- LMDB-compatible database (MDBX)
- Write-ahead logging (WAL)
- Memory-mapped file I/O
- Transaction support
- Multi-version concurrency control

### Concurrency
- Lock-free data structures
- Multi-producer multi-consumer queues
- Single-producer single-consumer queues
- Atomic operations
- Thread-safe memory allocation

## Documentation

### Available Documentation
- `docs/architecture.md` - System architecture overview
- `docs/COROUTINES.md` - Coroutine implementation details
- `docs/README.md` - General project documentation
- `docs/tack-method.md` - Tack method documentation
- `docs/usockets_design.md` - Socket design documentation

### Component-Specific Docs
- `rust/bop-rs/src/allocator/README.md` - Memory allocator documentation
- `rust/bop-rs/src/mpmc/README.md` - MPMC queue documentation
- `tests/README.md` - Testing framework documentation
