# uSockets io_uring Implementation

This directory contains a production-ready io_uring implementation for uSockets, providing a high-performance alternative to epoll/kqueue on Linux systems with kernel 5.11+.

## Files

- **us_loop_io_uring.c** - Main event loop implementation providing the us_loop_t interface
- **io_uring_context.h/c** - Core io_uring context management with feature detection
- **io_uring_submission.h/c** - SQE submission and CQE completion handling
- **buffer_pool.h/c** - High-performance buffer management with multiple size classes
- **io_error_handling.h/c** - Comprehensive error handling with automatic fallback to epoll

## Features

- **Drop-in replacement** for epoll/kqueue with the same us_loop_t interface
- **High performance** with batched submissions, zero-copy operations, and multishot operations
- **Production ready** with comprehensive error handling and automatic fallback
- **Memory efficient** with per-CPU buffer caching and size-class pools
- **Compatible** with existing uSockets BSD socket layer and SSL/crypto layer

## Building

To enable io_uring support, define `LIBUS_USE_IO_URING` and link with liburing:

```c
#define LIBUS_USE_IO_URING
```

```cmake
target_link_libraries(your_target uring)
```

## Requirements

- Linux kernel 5.11 or later
- liburing 2.0 or later
- C compiler with C11 atomics support

## Integration

The io_uring implementation integrates seamlessly with existing uSockets code:
- Socket operations use the existing BSD layer (bsd.c)
- SSL/TLS operations use the existing crypto layer (crypto/openssl.c or wolfssl.c)
- The loop interface matches epoll_kqueue.h exactly

No changes to application code are required when switching from epoll to io_uring.