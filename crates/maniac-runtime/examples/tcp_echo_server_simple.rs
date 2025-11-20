//! Simple TCP Echo Server Example
//!
//! This example demonstrates a simple TCP echo server that handles a single connection
//! using maniac-runtime's EventLoop.
//!
//! This is a simplified version to demonstrate the EventLoop API. For production use,
//! you would typically manage multiple connections and handle accepts differently.
//!
//! Usage:
//! 1. Run this server: cargo run --example tcp_echo_server_simple
//! 2. Connect with: nc 127.0.0.1 8080
//! 3. Type messages - they will be echoed back
//!
//! Features:
//! - Single connection handling
//! - Echoes back received data
//! - 60-second connection timeout
//! - Clean shutdown on connection close

use maniac_runtime::net::{EventHandler, EventLoop, Socket, SocketDescriptor};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

/// Handler for the echo connection
struct EchoHandler;

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
                println!("Client disconnected");
            }
            Ok(n) => {
                let data = String::from_utf8_lossy(&buffer[..n]);
                println!("Received {} bytes: {}", n, data.trim());

                // Echo the data back
                if let Err(e) = tcp_stream.write_all(&buffer[..n]) {
                    eprintln!("Write error: {}", e);
                } else {
                    println!("Echoed {} bytes back", n);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No data available yet, normal for non-blocking I/O
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
            }
        }

        // Leak the stream to prevent closing the FD
        std::mem::forget(stream);
    }

    fn on_writable(&mut self, _socket: &mut Socket) {
        // Socket ready for writing (not used in this example)
    }

    fn on_timeout(&mut self, _socket: &mut Socket) {
        println!("Connection timeout - closing");
    }

    fn on_close(&mut self, _socket: &mut Socket) {
        println!("Connection closed");
    }
}

fn main() -> io::Result<()> {
    println!("=== Simple TCP Echo Server ===");
    println!("Listening on 127.0.0.1:8080");
    println!("Waiting for ONE connection...\n");

    // Create TCP listener
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    println!("Server listening, waiting for connection...");

    // Accept ONE connection (blocking)
    let (stream, addr) = listener.accept()?;
    println!("Accepted connection from: {}\n", addr);

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

    // Create event loop with timer wheel (2-second ticks, 1024 buckets)
    let mut event_loop = EventLoop::with_timer_wheel()?;

    // Add the connection to event loop
    let token = event_loop.add_socket(fd, Box::new(EchoHandler), Box::new(()))?;

    // Set 60 second idle timeout
    event_loop.set_timeout(token, Duration::from_secs(60))?;

    println!("Event loop starting...");
    println!("Type messages in your client - they will be echoed back");
    println!("Connection will timeout after 60 seconds of inactivity");
    println!("Press Ctrl+C to exit\n");

    // Leak the stream so it doesn't close when it goes out of scope
    std::mem::forget(stream);

    // Run the event loop indefinitely
    event_loop.run()?;

    Ok(())
}
