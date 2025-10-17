#![feature(portable_simd)]
#![feature(thread_id_value)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! # MPMC Queue Wrapper
//!
//! High-performance multi-producer multi-consumer queue implementation using
//! the moodycamel C++ library through BOP's C API.
//!
//! This module provides safe Rust wrappers around the high-performance moodycamel
//! concurrent queue, offering both non-blocking and blocking variants.
//!
//! ## Features
//!
//! - **Lock-free**: Non-blocking operations for maximum performance
//! - **Thread-safe**: Multiple producers and consumers can operate concurrently
//! - **Token-based optimization**: Producer/consumer tokens for better performance
//! - **Bulk operations**: Efficient batch enqueue/dequeue operations
//! - **Blocking variant**: Optional blocking operations with timeout support
//! - **Memory efficient**: Zero-copy operations where possible

pub mod bits;
pub mod deque;
pub mod loom_exports;
pub mod mpmc;
pub mod mpsc;
pub mod seg_spmc;
pub mod seg_spsc;
pub mod selector;
pub mod signal;
pub mod summary_tree;
pub mod task;
pub mod timer_wheel;
pub mod utils;
pub mod waker;
pub mod worker;

pub use summary_tree::*;
pub use utils::*;

/// Error occurring when pushing into a queue is unsuccessful.
#[derive(Debug, Eq, PartialEq)]
pub enum PushError<T> {
    /// The queue is full.
    Full(T),
    /// The receiver has been dropped.
    Closed(T),
}

/// Error occurring when popping from a queue is unsuccessful.
#[derive(Debug, Eq, PartialEq)]
pub enum PopError {
    /// The queue is empty.
    Empty,
    /// All senders have been dropped and the queue is empty.
    Closed,
    ///
    Timeout,
}
