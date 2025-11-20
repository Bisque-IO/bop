# maniac-allocator

Custom memory allocator optimized for async workloads in the maniac runtime.

## Overview

A high-performance memory allocator designed specifically for async Rust applications. Provides efficient allocation patterns for coroutines and async tasks.

## Features

- Optimized for M:N threading patterns
- Reduced allocation overhead for async workloads
- Optional allocation statistics tracking
- Compatible with Rust's global allocator interface

## Usage

```rust
use maniac_allocator::ManiacAllocator;

#[global_allocator]
static GLOBAL: ManiacAllocator = ManiacAllocator;
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
