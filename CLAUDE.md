# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

BOP is a data platform for modern computing built in C++23. It includes networking capabilities (TCP/HTTP/WebSocket), distributed consensus (NuRaft), key-value storage (MDBX), and various utilities.

## Build System

This project uses **xmake** as its build system. Key commands:

### Building
```bash
# Configure and build all targets
xmake f -m release  # Configure for release mode
xmake f -m debug    # Configure for debug mode
xmake               # Build all targets
xmake -v            # Build with verbose output

# Build specific targets
xmake build bop                # Build the main library
xmake build test-uws-tcp       # Build TCP test executable
xmake build test-uws           # Build uWS test executable
```

### Running Tests
```bash
# Run specific test binaries
xmake r test-uws-tcp           # Run TCP tests
xmake r test-uws               # Run uWS tests

# Run tests with specific test cases
xmake r test-uws-tcp --test-case="Test Client and Server"
xmake r test-uws-tcp --test-case="TCP Large Data Transfer"

# Run with debug output
UWS_TESTS_DEBUG=1 xmake r test-uws-tcp
```

### Cleaning
```bash
xmake clean         # Clean build artifacts
xmake clean -a      # Clean all including cache
```

## Architecture

### Core Libraries (`lib/`)

- **uWS** (`lib/src/uws/`): Micro Web Sockets - high-performance WebSocket/HTTP implementation
  - `TCPServerApp.h`, `TCPClientApp.h`: TCP server/client applications
  - `TCPConnection.h`, `TCPContext.h`: TCP connection management
  - `HttpContext.h`, `HttpParser.h`: HTTP handling
  - `WebSocket*.h`: WebSocket protocol implementation
  - `Loop.h`, `LoopData.h`: Event loop management

- **usockets** (`lib/usockets/`): Low-level socket implementation with SSL support (OpenSSL/WolfSSL)

- **NuRaft** (`lib/nuraft/`): Raft consensus protocol implementation for distributed systems

- **MDBX** (`lib/mdbx/`): Fast embedded key-value database

- **snmalloc** (`lib/snmalloc/`): High-performance memory allocator

- **libuv** (`lib/libuv/`): Cross-platform async I/O (used on Windows)

### SSL/TLS Support

The project supports both OpenSSL and WolfSSL:
- Windows: Uses WolfSSL by default
- Linux/macOS: Can use either OpenSSL or WolfSSL
- Controlled via build configuration in `lib/xmake.lua`

### Platform-Specific Notes

- **Windows**: Uses libuv for async I/O, requires MSVC runtime
- **Linux**: Supports io_uring (optional), uses epoll by default
- **Cross-compilation**: Supports ARM64 and RISCV64 targets

## Testing

Tests use **doctest** framework and are located in `tests/`:
- `TCPTest.cpp`: Core TCP connection tests
- `uws_test.cpp`: Extended uWS functionality tests
- `HttpClientTest.cpp`: HTTP client tests

Test patterns:
- Use `TEST_CASE("Test name")` for test definitions
- Use `CHECK()` and `CHECK_EQ()` for assertions
- Debug output controlled via `TCP_TEST_DEBUG` macro

## Dependencies

External dependencies managed via xmake packages:
- Compression: zlib, zstd, brotli
- SSL/TLS: OpenSSL 3.5.1 or WolfSSL 5.7.2
- Boost: filesystem and system components
- Testing: doctest 2.4.11

## Code Style

- C++23 standard
- Header files use `.h` extension
- Implementation in `.cpp` files
- Prefer `std::string_view` for non-owning string parameters
- Use move semantics where appropriate
- Templates heavily used for compile-time polymorphism