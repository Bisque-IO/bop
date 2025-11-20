# maniac-usockets

High-performance async networking layer with zero-copy operations for the maniac runtime.

## Overview

Provides efficient network socket operations with minimal overhead. Built on top of high-performance networking libraries with support for TCP, UDP, and advanced socket features.

## Features

- Zero-copy networking where possible
- Async socket operations
- TCP and UDP support (UDP via feature flag)
- Integration with maniac runtime

## Usage

```rust
use maniac_usockets::{TcpListener, TcpStream};

async fn server() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    loop {
        let (stream, addr) = listener.accept().await?;
        // Handle connection
    }
}
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

