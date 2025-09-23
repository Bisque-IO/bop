# MPMC Queue

High-performance multi-producer multi-consumer queue implementation using the moodycamel C++ library through BOP's C API.

This module provides safe Rust wrappers around the high-performance moodycamel concurrent queue, offering both non-blocking and blocking variants.

## Features

-   **Lock-free**: Non-blocking operations for maximum performance
-   **Thread-safe**: Multiple producers and consumers can operate concurrently
-   **Token-based optimization**: Producer/consumer tokens for better performance
-   **Bulk operations**: Efficient batch enqueue/dequeue operations
-   **Blocking variant**: Optional blocking operations with timeout support
-   **Memory efficient**: Zero-copy operations where possible

## Usage

### Basic Non-blocking Queue

```rust,no_run
use bop_mpmc::mpmc::MpmcQueue;

let queue = MpmcQueue::new()?;

// Producer thread
if queue.try_enqueue(42) {
    println!("Enqueued successfully");
}

// Consumer thread
if let Some(item) = queue.try_dequeue() {
    println!("Dequeued: {}", item);
}
```

### Token-based Optimization

```rust,no_run
use bop_mpmc::mpmc::{MpmcQueue, ProducerToken, ConsumerToken};

let queue = MpmcQueue::new()?;
let producer_token = queue.create_producer_token()?;
let consumer_token = queue.create_consumer_token()?;

// More efficient operations with tokens
producer_token.try_enqueue(42)?;
if let Some(item) = consumer_token.try_dequeue()? {
    println!("Got: {}", item);
}
```

### Blocking Queue with Timeouts

```rust,no_run
use bop_mpmc::mpmc::BlockingMpmcQueue;
use std::time::Duration;

let queue = BlockingMpmcQueue::new()?;

// Blocks until item is available or timeout expires
if let Some(item) = queue.dequeue_wait(Duration::from_millis(1000))? {
    println!("Got item within timeout: {}", item);
}
```

## `MpmcError`

This enum represents the possible errors that can occur when working with MPMC queues.

```rust
pub enum MpmcError {
    CreationFailed,
    ProducerTokenCreationFailed,
    ConsumerTokenCreationFailed,
    OperationFailed,
    InvalidTimeout,
}
```

## `Queue`

A low-level MPMC queue for `u64` values. This is the foundation for other queue types.

**Methods**

-   `new() -> MpmcResult<Self>`: Creates a new queue.
-   `size_approx(&self) -> usize`: Returns the approximate number of items in the queue.
-   `try_enqueue(&self, item: u64) -> bool`: Attempts to enqueue an item.
-   `try_dequeue(&self) -> Option<u64>`: Attempts to dequeue an item.
-   `try_enqueue_bulk(&self, items: &[u64]) -> bool`: Attempts to enqueue a slice of items.
-   `try_dequeue_bulk(&self, buffer: &mut [u64]) -> usize`: Attempts to dequeue items into a buffer.
-   `create_producer_token(&self) -> MpmcResult<ProducerToken<'_>>`: Creates a producer token.
-   `create_consumer_token(&self) -> MpmcResult<ConsumerToken<'_>>`: Creates a consumer token.

## `ProducerToken` and `ConsumerToken`

These tokens provide optimized enqueue and dequeue operations.

**`ProducerToken` Methods**

-   `enqueue(&self, item: u64) -> bool`: Enqueues an item.
-   `enqueue_bulk(&self, items: &[u64]) -> bool`: Enqueues a slice of items.

**`ConsumerToken` Methods**

-   `dequeue(&self) -> Option<u64>`: Dequeues an item.
-   `dequeue_bulk(&self, buffer: &mut [u64]) -> usize`: Dequeues items into a buffer.

## `BoxQueue<T>`

A queue for `Box<T>` items, providing a safe way to handle owned data.

**Methods**

-   `new() -> MpmcResult<Self>`: Creates a new `BoxQueue`.
-   `enqueue(&self, boxed: Box<T>) -> bool`: Enqueues a `Box<T>`.
-   `dequeue(&self) -> Option<Box<T>>`: Dequeues a `Box<T>`.
-   `enqueue_bulk(&self, boxes: Vec<Box<T>>) -> bool`: Enqueues a vector of `Box<T>`.
-   `dequeue_bulk(&self, max_items: usize) -> Vec<Box<T>>`: Dequeues a vector of `Box<T>`.

## `ArcQueue<T>`

A queue for `Arc<T>` items, for shared ownership.

**Methods**

-   `new() -> MpmcResult<Self>`: Creates a new `ArcQueue`.
-   `enqueue(&self, arc: Arc<T>) -> bool`: Enqueues an `Arc<T>`.
-   `enqueue_cloned(&self, arc: &Arc<T>) -> bool`: Enqueues a clone of an `Arc<T>`.
-   `dequeue(&self) -> Option<Arc<T>>`: Dequeues an `Arc<T>`.
-   `enqueue_bulk_cloned(&self, arcs: &[Arc<T>]) -> bool`: Enqueues a slice of cloned `Arc<T>`.
-   `dequeue_bulk(&self, max_items: usize) -> Vec<Arc<T>>`: Dequeues a vector of `Arc<T>`.

## Blocking Queues

The module also provides blocking versions of the queues: `BlockingQueue`, `BlockingBoxQueue`, and `BlockingArcQueue`. These queues have similar methods to their non-blocking counterparts, but with `wait` variants for blocking operations with timeouts.
