// Core event loop (hybrid usockets + Mio implementation)
pub mod event_loop;

// Async socket integration for Future-based API
pub mod async_socket;

// Future-based TCP networking
pub mod tcp;

// usockets port foundation
pub mod constants;

// usockets optimizations (used by event_loop)
pub mod low_prio_queue;

// UDP support
pub mod udp;

// Re-export main types
pub use event_loop::EventLoop;
pub use async_socket::AsyncSocketState;
pub use tcp::{TcpStream, TcpListener};
pub use crate::monoio::tls::*;
pub use constants::*;
pub use low_prio_queue::LowPriorityQueue;
pub use udp::UdpSocket;

// Re-export Mio's Token for socket identification
pub use mio::Token;

/// Type of socket being registered (required for Windows/mio)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    TcpStream,
    TcpListener,
    UdpSocket,
    // Other types like UnixStream, UnixListener can be added if needed
}

// In net/mod.rs
#[cfg(unix)]
pub type SocketDescriptor = std::os::unix::io::RawFd;
#[cfg(windows)]
pub type SocketDescriptor = windows_sys::Win32::Networking::WinSock::SOCKET;