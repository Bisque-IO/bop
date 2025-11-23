//! Future-based async TCP networking
//!
//! This module provides ergonomic async/await APIs for TCP networking that integrate
//! with maniac-runtime's task system and EventLoop (via monoio).

// Re-export monoio types directly.
// Note: This changes the API to be completion-based (owned buffers).
// Users must update their code to pass Vec<u8> or similar instead of &mut [u8].
pub use crate::monoio::net::{TcpListener, TcpStream};

// Re-export other useful types
pub use crate::monoio::net::{TcpStream as AsyncTcpStream, TcpListener as AsyncTcpListener};
