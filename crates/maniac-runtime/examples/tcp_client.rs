//! TCP Client Example
//!
//! This example demonstrates a TCP client that connects to a server and sends/receives data
//! using maniac-runtime's EventLoop.
//!
//! Features:
//! - Non-blocking TCP connection
//! - Send data on connection
//! - Receive and print responses
//! - Connection timeout handling
//!
//! Run with: cargo run --example tcp_client

use maniac_runtime::net::{EventHandler, EventLoop, Socket, SocketDescriptor};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Client connection handler
struct ClientHandler {
    message_sent: bool,
}

impl ClientHandler {
    fn new() -> Self {
        Self {
            message_sent: false,
        }
    }
}

impl EventHandler for ClientHandler {
    fn on_readable(&mut self, socket: &mut Socket, buffer: &mut [u8]) {
        println!("Data available to read");

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
                println!("Server closed connection");
            }
            Ok(n) => {
                let response = String::from_utf8_lossy(&buffer[..n]);
                println!("Received {} bytes: {}", n, response.trim());
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No data available yet
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
            }
        }

        // Leak the stream to prevent it from closing the FD
        std::mem::forget(stream);
    }

    fn on_writable(&mut self, socket: &mut Socket) {
        if !self.message_sent {
            println!("Socket writable - sending message");

            // Get the underlying TCP stream
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

            let message = b"Hello from maniac-runtime TCP client!\n";
            let mut tcp_stream = &stream;

            match tcp_stream.write_all(message) {
                Ok(_) => {
                    println!("Sent {} bytes", message.len());
                    self.message_sent = true;
                }
                Err(e) => {
                    eprintln!("Write error: {}", e);
                }
            }

            // Leak the stream
            std::mem::forget(stream);
        }
    }

    fn on_timeout(&mut self, _socket: &mut Socket) {
        println!("Connection timeout");
    }

    fn on_close(&mut self, _socket: &mut Socket) {
        println!("Connection closed");
    }
}

fn main() -> io::Result<()> {
    println!("=== TCP Client Example ===");
    println!("Connecting to 127.0.0.1:8080");

    // Connect to server
    let stream = match TcpStream::connect("127.0.0.1:8080") {
        Ok(s) => {
            println!("Connected successfully!");
            s
        }
        Err(e) => {
            eprintln!("Connection failed: {}", e);
            eprintln!("\nMake sure a server is running on 127.0.0.1:8080");
            eprintln!("You can test with: nc -l 8080");
            return Err(e);
        }
    };

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

    // Create event loop with timer wheel
    let mut event_loop = EventLoop::with_timer_wheel()?;

    // Add socket to event loop
    let handler = ClientHandler::new();
    let token = event_loop.add_socket(fd, Box::new(handler), Box::new(()))?;

    // Set 30 second timeout
    event_loop.set_timeout(token, Duration::from_secs(30))?;

    println!("Event loop starting...");
    println!("Press Ctrl+C to exit\n");

    // Leak the stream so it doesn't get closed when it goes out of scope
    std::mem::forget(stream);

    // Run event loop for a limited time (10 iterations for demo)
    for i in 0..10 {
        match event_loop.poll_once(Some(Duration::from_millis(500))) {
            Ok(count) => {
                if count > 0 {
                    println!("Iteration {}: processed {} events", i + 1, count);
                }
            }
            Err(e) => {
                eprintln!("Poll error: {}", e);
                break;
            }
        }
    }

    println!("\nClient shutting down");
    Ok(())
}
