# maniac-storage

Async storage layer with AOF (Append-Only File) support for the maniac runtime.

## Overview

A high-performance storage layer designed for async applications. Provides persistent storage with AOF support, compression, and efficient data structures.

## Features

- Append-Only File (AOF) persistence
- Compression with zstd
- Async I/O operations
- Integration with S3-compatible storage
- Support for various data structures (roaring bitmaps, etc.)
- Optional libsql backend

## Usage

```rust
use maniac_storage::Storage;

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    let storage = Storage::new("data.aof").await?;
    storage.write(b"key", b"value").await?;
    Ok(())
}
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

