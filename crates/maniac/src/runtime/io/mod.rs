#![allow(stable_features)]
#![allow(clippy::macro_metavars_in_unsafe)]
#![cfg_attr(feature = "unstable", feature(io_error_more))]
#![cfg_attr(feature = "unstable", feature(lazy_cell))]
#![cfg_attr(feature = "unstable", feature(stmt_expr_attributes))]
#![cfg_attr(feature = "unstable", feature(thread_local))]

pub use crate::{join, select, try_join};

pub(crate) mod io_builder;
#[allow(dead_code)]
pub(crate) mod io_runtime;

use std::future::Future;

pub use builder::{Buildable, RuntimeBuilder};
pub use crate::driver::Driver;
#[cfg(all(target_os = "linux", feature = "iouring"))]
pub use crate::driver::IoUringDriver;
#[cfg(feature = "poll")]
pub use crate::driver::PollerDriver;

// pub use runtime::{spawn, Runtime};
pub use runtime::Runtime;
#[cfg(any(all(target_os = "linux", feature = "iouring"), feature = "poll"))]
pub use {builder::FusionDriver, runtime::FusionRuntime};

/// A specialized `Result` type for `io-uring` operations with buffers.
pub use crate::buf::BufResult;
