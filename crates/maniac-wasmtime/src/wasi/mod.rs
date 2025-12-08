//! WASI Preview 1 and Preview 2 implementation for Maniac runtime.
//!
//! This module provides complete WASI host function implementations that
//! use Maniac's async runtime instead of Tokio. The implementations are
//! designed to work with Maniac's stackful coroutines for efficient I/O.
//!
//! # Features
//!
//! - `wasi-preview1`: Enable WASI Preview 1 support
//! - `wasi-preview2`: Enable WASI Preview 2 (component model) support  
//! - `wasi-net`: Enable socket support (TCP/UDP)
//!
//! # Example
//!
//! ```ignore
//! use maniac::wasi::preview1::{WasiCtx, WasiCtxBuilder, add_to_linker};
//! use wasmtime::{Engine, Linker, Module, Store};
//!
//! let engine = Engine::default();
//! let mut linker = Linker::new(&engine);
//!
//! // Add WASI functions to linker
//! add_to_linker(&mut linker)?;
//!
//! // Build WASI context
//! let wasi = WasiCtxBuilder::new()
//!     .inherit_stdio()
//!     .inherit_args()
//!     .inherit_env()
//!     .preopen_dir(".", "/")?
//!     .build();
//!
//! let mut store = Store::new(&engine, wasi);
//! ```

pub mod types;

#[cfg(feature = "wasi-preview1")]
pub mod preview1;

#[cfg(feature = "wasi-preview2")]
pub mod preview2;

// Re-exports for convenience
#[cfg(feature = "wasi-preview1")]
pub use preview1::{WasiCtx, WasiCtxBuilder, WasiView, add_to_linker};
