//! For compat with tokio AsyncRead and AsyncWrite.

pub mod box_future;
mod buf;

mod safe_wrapper;
mod tcp_unsafe;

#[cfg(feature = "hyper")]
pub mod hyper;

pub use safe_wrapper::StreamWrapper;
pub use tcp_unsafe::TcpStreamCompat as TcpStreamCompatUnsafe;
pub use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub type TcpStreamCompat = StreamWrapper<crate::net::TcpStream>;
#[cfg(unix)]
pub type UnixStreamCompat = StreamWrapper<crate::net::UnixStream>;

// Tests removed - rely on maniac-runtime's own tests
