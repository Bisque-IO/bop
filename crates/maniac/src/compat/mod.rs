//! For compat with tokio AsyncRead and AsyncWrite.
//!
//! Note: To use this module, you must have `tokio` with the `io-util` feature  
//! in your own dependencies.

#[cfg(feature = "tokio-compat")]
pub mod box_future;
mod buf;

#[cfg(feature = "tokio-compat")]
mod safe_wrapper;
#[cfg(feature = "tokio-compat")]
mod tcp_unsafe;
#[cfg(feature = "tokio-compat")]
mod tokio_wrapper;

#[cfg(feature = "hyper")]
pub mod hyper;

#[cfg(feature = "tokio-compat")]
pub use safe_wrapper::StreamWrapper;
#[cfg(feature = "tokio-compat")]
pub use tcp_unsafe::TcpStreamCompat as TcpStreamCompatUnsafe;
#[cfg(feature = "tokio-compat")]
pub use tokio_wrapper::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[cfg(feature = "tokio-compat")]
pub type TcpStreamCompat = StreamWrapper<crate::net::TcpStream>;
#[cfg(all(unix, feature = "tokio-compat"))]
pub type UnixStreamCompat = StreamWrapper<crate::net::UnixStream>;

// Tests removed - rely on maniac-runtime's own tests
