# maniac

Massively scalable Rust async runtime with advanced features for building high-performance distributed systems.

## Features

- Built on top of `maniac-runtime` for M:N threading and stackful coroutines
- Integrated async storage layer
- High-performance networking with zero-copy operations
- Custom memory allocator for reduced allocation overhead
- Support for async/await syntax

## Components

- **maniac-runtime**: Core async runtime with M:N threading
- **maniac-allocator**: Custom memory allocator
- **maniac-storage**: Async storage layer
- **maniac-usockets**: High-performance networking
- **maniac-sys**: Low-level system primitives

## Usage

```rust
use maniac::Runtime;

#[maniac::main]
async fn main() {
    // Your async code here
    println!("Hello from maniac!");
}
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

