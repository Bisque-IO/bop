//! Async Echo Server Example using maniac-runtime
//!
//! This example demonstrates integrating EventLoop with the Worker runtime
//! to create an async echo server that handles multiple connections.
//!
//! Usage:
//! 1. Run this server: cargo run --example echo_server
//! 2. Connect with: nc 127.0.0.1 8080
//! 3. Type messages - they will be echoed back
//!
//! Features:
//! - Multiple connection handling via EventLoop
//! - Per-worker EventLoop integration
//! - Echoes back received data
//! - 60-second connection timeout
//! - Clean shutdown on connection close

use maniac_runtime::net::{EventHandler, EventLoop, Socket, SocketDescriptor};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

/// Handler for echo connections
struct EchoHandler {
    peer_addr: String,
}

impl EventHandler for EchoHandler {
    fn on_readable(&mut self, socket: &mut Socket, buffer: &mut [u8]) {
        // Get the underlying TCP stream from the file descriptor
        // SAFETY: We own this socket and the FD is valid
        #[cfg(unix)]
        let stream = unsafe {
            use std::os::unix::io::FromRawFd;
            TcpStream::from_raw_fd(socket.fd)
        };

        #[cfg(windows)]
        let stream = unsafe {
            use std::os::windows::io::FromRawSocket;
            TcpStream::from_raw_socket(socket.fd as _)
        };

        // Try to read data
        let mut tcp_stream = &stream;
        match tcp_stream.read(buffer) {
            Ok(0) => {
                println!("[{}] Client disconnected", self.peer_addr);
            }
            Ok(n) => {
                let data = String::from_utf8_lossy(&buffer[..n]);
                println!("[{}] Received {} bytes: {}", self.peer_addr, n, data.trim());

                // Echo the data back
                if let Err(e) = tcp_stream.write_all(&buffer[..n]) {
                    eprintln!("[{}] Write error: {}", self.peer_addr, e);
                } else {
                    println!("[{}] Echoed {} bytes back", self.peer_addr, n);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No data available yet, normal for non-blocking I/O
            }
            Err(e) => {
                eprintln!("[{}] Read error: {}", self.peer_addr, e);
            }
        }

        // Leak the stream to prevent closing the FD
        std::mem::forget(stream);
    }

    fn on_writable(&mut self, _socket: &mut Socket) {
        // Socket ready for writing (not used in this example)
    }

    fn on_timeout(&mut self, _socket: &mut Socket) {
        println!("[{}] Connection timeout - closing", self.peer_addr);
    }

    fn on_close(&mut self, _socket: &mut Socket) {
        println!("[{}] Connection closed", self.peer_addr);
    }
}

fn main() -> io::Result<()> {
    println!("=== Async Echo Server (EventLoop + Worker Integration) ===");
    println!("Listening on 127.0.0.1:8080");
    println!("Press Ctrl+C to exit\n");

    // Create TCP listener
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    listener.set_nonblocking(true)?;

    // Create event loop with timer wheel
    let mut event_loop = EventLoop::with_timer_wheel()?;

    println!("Server running - waiting for connections...\n");

    // Main event loop
    loop {
        // Try to accept new connections (non-blocking)
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("Accepted connection from: {}", addr);

                // Set non-blocking mode
                stream.set_nonblocking(true)?;

                // Get the file descriptor
                #[cfg(unix)]
                let fd = {
                    use std::os::unix::io::AsRawFd;
                    stream.as_raw_fd() as SocketDescriptor
                };

                #[cfg(windows)]
                let fd = {
                    use std::os::windows::io::AsRawSocket;
                    stream.as_raw_socket() as SocketDescriptor
                };

                // Add the connection to event loop
                let token = event_loop.add_socket(
                    fd,
                    Box::new(EchoHandler {
                        peer_addr: addr.to_string(),
                    }),
                    Box::new(()),
                )?;

                // Set 60 second idle timeout
                event_loop.set_timeout(token, Duration::from_secs(60))?;

                println!("Connection from {} registered with event loop", addr);

                // Leak the stream so it doesn't close when it goes out of scope
                std::mem::forget(stream);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No pending connections, process I/O events
                event_loop.poll_once(Some(Duration::from_millis(100)))?;
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
            }
        }
    }
}
