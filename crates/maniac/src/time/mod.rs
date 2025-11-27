//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.
//!
//! * [`Sleep`] is a future that does no work and completes at a specific [`Instant`] in time.
//!
//! * [`Interval`] is a stream yielding a value at a fixed period. It is initialized with a
//!   [`Duration`] and repeatedly yields each time the duration elapses.
//!
//! * [`Timeout`]: Wraps a future or stream, setting an upper bound to the amount of time it is
//!   allowed to execute. If the future or stream does not complete in time, then it is canceled and
//!   an error is returned.
//!
//! These types are sufficient for handling a large number of scenarios
//! involving time.

pub mod error;
pub mod instant;
pub mod interval;
pub mod sleep;
pub mod timeout;

// Re-export for convenience
#[doc(no_inline)]
pub use std::time::Duration;

#[doc(inline)]
pub use error::Elapsed;
#[doc(inline)]
pub use instant::Instant;
#[doc(inline)]
pub use interval::{Interval, MissedTickBehavior, interval, interval_at};
#[doc(inline)]
pub use sleep::{Sleep, sleep, sleep_until};
#[doc(inline)]
pub use timeout::{Timeout, timeout, timeout_at};
