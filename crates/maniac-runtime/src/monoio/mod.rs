#![allow(stable_features)]
#![allow(clippy::macro_metavars_in_unsafe)]
#![cfg_attr(feature = "unstable", feature(io_error_more))]
#![cfg_attr(feature = "unstable", feature(lazy_cell))]
#![cfg_attr(feature = "unstable", feature(stmt_expr_attributes))]
#![cfg_attr(feature = "unstable", feature(thread_local))]

#[macro_use]
pub mod macros;

#[macro_use]
pub mod driver;
pub(crate) mod builder;
#[allow(dead_code)]
pub(crate) mod runtime;
// mod scheduler;
pub mod time;

extern crate alloc;

#[cfg(feature = "sync")]
pub mod blocking;

pub mod buf;
#[cfg(feature = "tokio-compat")]
pub mod compat;
pub mod fs;
pub mod io;
pub mod net;
// pub mod task;
#[cfg(any(feature = "tls-rustls", feature = "tls-native"))]
pub mod tls;
pub mod utils;

use std::future::Future;

#[cfg(feature = "sync")]
pub use blocking::spawn_blocking;
pub use builder::{Buildable, RuntimeBuilder};
pub use driver::Driver;
#[cfg(all(target_os = "linux", feature = "iouring"))]
pub use driver::IoUringDriver;
#[cfg(feature = "legacy")]
pub use driver::LegacyDriver;

// pub use runtime::{spawn, Runtime};
pub use runtime::Runtime;
#[cfg(any(all(target_os = "linux", feature = "iouring"), feature = "legacy"))]
pub use {builder::FusionDriver, runtime::FusionRuntime};

/// A specialized `Result` type for `io-uring` operations with buffers.
pub type BufResult<T, B> = (std::io::Result<T>, B);
