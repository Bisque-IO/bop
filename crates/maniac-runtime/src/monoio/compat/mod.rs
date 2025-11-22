//! For compat with tokio AsyncRead and AsyncWrite.
//!
//! Note: To use this module, you must have `tokio` with the `io-util` feature  
//! in your own dependencies.

pub mod box_future;
mod buf;

mod safe_wrapper;
mod tcp_unsafe;
mod tokio_wrapper;

#[cfg(feature = "hyper")]
pub mod hyper;

pub use safe_wrapper::StreamWrapper;
pub use tcp_unsafe::TcpStreamCompat as TcpStreamCompatUnsafe;
pub use tokio_wrapper::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub type TcpStreamCompat = StreamWrapper<crate::monoio::net::TcpStream>;
#[cfg(unix)]
pub type UnixStreamCompat = StreamWrapper<crate::monoio::net::UnixStream>;

// Tests removed - rely on maniac-runtime's own tests
