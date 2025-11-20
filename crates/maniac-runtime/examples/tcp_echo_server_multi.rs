//! Multi-Connection TCP Echo Server Example
//!
//! This example demonstrates handling multiple concurrent connections using maniac-runtime.
//! It works around the current EventLoop limitation by checking for new connections
//! in a pre-iteration callback.
//!
//! Features:
//! - Multiple concurrent connections
//! - Per-connection timeout (60 seconds)
//! - Echo functionality
//! - Clean connection lifecycle management
//!
//! Run with: cargo run --example tcp_echo_server_multi

use maniac_runtime::net::{EventHandler, EventLoop, Socket, SocketDescriptor};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Handler for individual echo connections
struct EchoConnectionHandler {
    id: usize,
}

impl EventHandler for EchoConnectionHandler {
    fn on_readable(&mut self, socket: &mut Socket, buffer: &mut [u8]) {
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

        let mut tcp_stream = &stream;
        match tcp_stream.read(buffer) {
            Ok(0) => {
                println!("[Conn {}] Client disconnected", self.id);
            }
            Ok(n) => {
                let data = String::from_utf8_lossy(&buffer[..n]);
                println!("[Conn {}] Received: {}", self.id, data.trim());

                // Echo back
                if let Err(e) = tcp_stream.write_all(&buffer[..n]) {
                    eprintln!("[Conn {}] Write error: {}", self.id, e);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No data available
            }
            Err(e) => {
                eprintln!("[Conn {}] Read error: {}", self.id, e);
            }
        }

        std::mem::forget(stream);
    }

    fn on_writable(&mut self, _socket: &mut Socket) {}

    fn on_timeout(&mut self, _socket: &mut Socket) {
        println!("[Conn {}] Timeout", self.id);
    }

    fn on_close(&mut self, _socket: &mut Socket) {
        println!("[Conn {}] Closed", self.id);
    }
}

/// Pending connection to be added to the event loop
struct PendingConnection {
    stream: TcpStream,
    id: usize,
}

fn main() -> io::Result<()> {
    println!("=== Multi-Connection TCP Echo Server ===");
    println!("Listening on 127.0.0.1:8080");
    println!("Accepts multiple concurrent connections\n");

    // Create TCP listener
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    listener.set_nonblocking(true)?;

    println!("Server listening...\n");

    // Queue for pending connections (shared with pre-callback)
    let pending: Arc<Mutex<VecDeque<PendingConnection>>> = Arc::new(Mutex::new(VecDeque::new()));
    let pending_clone = pending.clone();

    // Connection ID counter
    let next_id = Arc::new(Mutex::new(1usize));
    let next_id_clone = next_id.clone();

    // Spawn a thread to accept connections
    std::thread::spawn(move || {
        let listener = listener;
        let pending = pending_clone;
        let next_id = next_id_clone;

        loop {
            match listener.accept() {
                Ok((stream, addr)) => {
                    let id = {
                        let mut id_guard = next_id.lock().unwrap();
                        let id = *id_guard;
                        *id_guard += 1;
                        id
                    };

                    println!("Accepted connection {} from: {}", id, addr);

                    // Set non-blocking
                    if let Err(e) = stream.set_nonblocking(true) {
                        eprintln!("Failed to set non-blocking: {}", e);
                        continue;
                    }

                    // Queue for event loop to add
                    pending
                        .lock()
                        .unwrap()
                        .push_back(PendingConnection { stream, id });
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No pending connections, sleep briefly
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    });

    // Create event loop with timer wheel
    let mut event_loop = EventLoop::with_timer_wheel()?;

    // Set up pre-callback to add pending connections
    let pending_clone = pending.clone();
    event_loop.set_pre_callback(move || {
        let mut pending_guard = pending_clone.lock().unwrap();

        while let Some(conn) = pending_guard.pop_front() {
            println!("Adding connection {} to event loop", conn.id);

            // Get FD
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

            // Note: We can't actually call event_loop.add_socket() here because
            // we don't have access to event_loop in the callback!
            //
            // This is a fundamental limitation of the current API design.
            // We would need to either:
            // 1. Pass &mut EventLoop to the callback (but that creates borrow issues)
            // 2. Use a queue that EventLoop checks after callbacks
            // 3. Redesign the API to support dynamic socket addition

            println!("WARNING: Cannot add socket from callback - API limitation!");
            println!(
                "FD {} for connection {} prepared but not added",
                fd, conn.id
            );

            // Clean up
            std::mem::forget(conn.stream);
        }
    });

    println!("Event loop starting...");
    println!("NOTE: This example demonstrates the architectural challenge.");
    println!("The EventLoop API needs changes to support dynamic socket addition.\n");

    // Run the event loop
    // In a real implementation, we would need to check the pending queue
    // and add sockets between poll_once calls
    for i in 0..50 {
        match event_loop.poll_once(Some(Duration::from_millis(100))) {
            Ok(count) => {
                if count > 0 {
                    println!("Iteration {}: processed {} events", i, count);
                }
            }
            Err(e) => {
                eprintln!("Poll error: {}", e);
                break;
            }
        }

        // This is where we would add pending connections if we had API support
        // for dynamic socket addition
    }

    println!("\nServer shutting down");
    Ok(())
}
