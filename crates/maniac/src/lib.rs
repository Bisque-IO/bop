// #![feature(portable_simd)]
// #![feature(thread_id_value)]
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

pub mod future;
pub mod generator;
mod loom_exports;
pub mod monoio;
pub mod net;
pub mod runtime;
mod spsc;
pub mod sync;
pub mod utils;

pub use crate::utils::*;

#[macro_use]
pub mod macros;

// Re-export socket registration functions for async networking
pub use crate::runtime::worker::current_worker_id;

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
