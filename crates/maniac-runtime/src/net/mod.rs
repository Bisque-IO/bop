// Core event loop (hybrid usockets + Mio implementation)
pub mod event_loop;
pub mod timer;

// Async socket integration for Future-based API
pub mod async_socket;

// Future-based TCP networking
pub mod tcp;

// usockets port foundation
pub mod constants;
pub mod bsd_sockets;

// usockets optimizations (used by event_loop)
pub mod low_prio_queue;

// TLS support (rustls 0.23 + aws-lc-rs) - Future-based API
pub mod tls;

// UDP support
pub mod udp;

// Re-export main types
pub use event_loop::EventLoop;
pub use async_socket::AsyncSocketState;
pub use tcp::{TcpStream, TcpListener};
pub use tls::{TlsStream, TlsConfig};
pub use constants::*;
pub use bsd_sockets::{SocketDescriptor, BsdAddr};
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

// Note: For timer functionality, use the runtime::timer_wheel::SingleWheel
// via EventLoop::with_timer_wheel() for integration with maniac-runtime
