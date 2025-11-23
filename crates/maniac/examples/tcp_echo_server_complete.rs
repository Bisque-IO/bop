//! Complete Multi-Connection TCP Echo Server
//!
//! This example shows a working multi-connection server using a manual event loop
//! that works around the current EventLoop API limitation.
//!
//! Features:
//! - Multiple concurrent connections
//! - Per-connection timeouts
//! - Echo functionality
//! - Proper connection lifecycle management
//!
//! Run with: cargo run --example tcp_echo_server_complete

use maniac::net::{EventHandler, EventLoop, Socket, SocketDescriptor};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

/// Handler for echo connections
struct EchoHandler {
    id: usize,
}

impl EventHandler for EchoHandler {
    fn on_readable(&mut self, socket: &mut Socket, buffer: &mut [u8]) {
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

        let mut tcp_stream = &stream;
        match tcp_stream.read(buffer) {
            Ok(0) => {
                println!("[Conn {}] Client disconnected", self.id);
            }
            Ok(n) => {
                let data = String::from_utf8_lossy(&buffer[..n]);
                print!("[Conn {}] << {}", self.id, data);

                // Echo back
                if let Err(e) = tcp_stream.write_all(&buffer[..n]) {
                    eprintln!("[Conn {}] Write error: {}", self.id, e);
                } else {
                    print!("[Conn {}] >> {}", self.id, data);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => {
                eprintln!("[Conn {}] Read error: {}", self.id, e);
            }
        }

        std::mem::forget(stream);
    }

    fn on_writable(&mut self, _socket: &mut Socket) {}

    fn on_timeout(&mut self, _socket: &mut Socket) {
        println!("[Conn {}] Idle timeout", self.id);
    }

    fn on_close(&mut self, _socket: &mut Socket) {
        println!("[Conn {}] Closed", self.id);
    }
}

/// Pending connection awaiting addition to event loop
struct PendingConnection {
    stream: TcpStream,
    id: usize,
}

fn main() -> io::Result<()> {
    println!("=== Complete Multi-Connection TCP Echo Server ===");
    println!("Listening on 127.0.0.1:8080\n");

    // Create listener
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    listener.set_nonblocking(true)?;

    println!("Server ready - accepting connections...");
    println!("Test with: nc 127.0.0.1 8080");
    println!("Press Ctrl+C to exit\n");

    // Pending connections queue
    let mut pending_connections: VecDeque<PendingConnection> = VecDeque::new();
    let mut next_id = 1usize;

    // Create event loop with timer wheel
    let mut event_loop = EventLoop::with_timer_wheel()?;

    // Main server loop
    loop {
        // Check for new connections (non-blocking)
        loop {
            match listener.accept() {
                Ok((stream, addr)) => {
                    println!("New connection {} from {}", next_id, addr);

                    stream.set_nonblocking(true)?;

                    pending_connections.push_back(PendingConnection {
                        stream,
                        id: next_id,
                    });

                    next_id += 1;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    break; // No more pending connections
                }
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                    break;
                }
            }
        }

        // Add pending connections to event loop
        while let Some(conn) = pending_connections.pop_front() {
            #[cfg(unix)]
            let fd = {
                use std::os::unix::io::AsRawFd;
                conn.stream.as_raw_fd() as SocketDescriptor
            };

            #[cfg(windows)]
            let fd = {
                use std::os::windows::io::AsRawSocket;
                conn.stream.as_raw_socket() as SocketDescriptor
            };

            // Add to event loop
            let token =
                event_loop.add_socket(fd, Box::new(EchoHandler { id: conn.id }), Box::new(()))?;

            // Set 60-second idle timeout
            event_loop.set_timeout(token, Duration::from_secs(60))?;

            println!("[Conn {}] Added to event loop with 60s timeout", conn.id);

            // Leak the stream so FD stays open
            std::mem::forget(conn.stream);
        }

        // Poll the event loop (100ms timeout)
        match event_loop.poll_once(Some(Duration::from_millis(100))) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Poll error: {}", e);
                return Err(e);
            }
        }
    }
}
