# maniac-runtime

High-performance async runtime with M:N threading and stackful coroutines.

## Features

- **M:N Threading**: Maps M user-level green threads onto N OS threads
- **Stackful Coroutines**: Full stack support for async operations
- **Preemptive Scheduling**: Cooperative and preemptive task scheduling
- **Work Stealing**: Efficient load balancing across worker threads
- **Zero-cost Abstractions**: Minimal overhead for async operations

## Usage

```rust
use maniac::Runtime;

fn main() {
    let runtime = Runtime::new();
    runtime.block_on(async {
        // Your async code here
    });
}
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

