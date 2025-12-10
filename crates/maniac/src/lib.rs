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

extern crate alloc;

pub mod blocking;
pub mod buf;
mod byteview;

pub use {byteview::ByteView, byteview::StrView};

#[doc(hidden)]
pub use byteview::{Builder, Mutator};
#[cfg(feature = "tokio-compat")]
pub mod compat;

mod detail;
#[macro_use]
pub mod driver;
pub mod fs;
pub mod future;
pub mod generator;
pub mod io;
mod loom_exports;
pub mod net;
pub mod process;
pub mod runtime;

pub mod signal;
pub mod sync;
pub mod time;
#[cfg(any(feature = "tls-rustls", feature = "tls-native"))]
pub mod tls;
pub mod utils;

// NOTE: Raft support has been moved to the separate maniac-raft crate.
// Use `maniac-raft` directly instead of enabling a raft feature on maniac.

pub use crate::utils::*;

#[macro_use]
pub mod macros;

pub use crate::blocking::unblock;

pub use buf::*;

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

pub fn now() -> u64 {
    runtime::worker::current_worker_now_ns()
}

pub use runtime::worker::{as_coroutine, current_worker, spawn, sync_await, try_sync_await};
