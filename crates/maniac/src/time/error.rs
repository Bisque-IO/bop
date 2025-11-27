//! Error types for time operations.

use std::fmt;

/// Error returned when a timeout elapses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elapsed(());

impl Elapsed {
    /// Create a new `Elapsed` error.
    pub fn new() -> Self {
        Elapsed(())
    }
}

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "deadline has elapsed")
    }
}

impl std::error::Error for Elapsed {}

impl Default for Elapsed {
    fn default() -> Self {
        Self::new()
    }
}
