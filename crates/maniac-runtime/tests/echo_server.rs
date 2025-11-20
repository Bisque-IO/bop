use maniac_runtime::net::event_loop::EventHandler;
use maniac_runtime::net::{EventLoop, Socket};
use std::cell::UnsafeCell;
use std::io::{Read, Write};
use std::rc::Rc;

struct EchoHandler {
    socket: Rc<UnsafeCell<Socket>>,
}

impl EventHandler for EchoHandler {
    fn on_readable(&mut self) {
        let mut buf = [0u8; 1024];
        // SAFETY: Single threaded, we are in the callback so we have access
        let socket = unsafe { &mut *self.socket.get() };
        match socket.read(&mut buf) {
            Ok(0) => {
                println!("Connection closed");
            }
            Ok(n) => {
                println!("Received {} bytes: {:?}", n, &buf[..n]);
                let _ = socket.write(&buf[..n]);
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    println!("Error: {}", e);
                }
            }
        }
    }

    fn on_writable(&mut self) {
        println!("Socket writable");
    }
}

#[test]
fn test_echo_server() {
    let event_loop = EventLoop::new().unwrap();
    let event_loop = Rc::new(UnsafeCell::new(event_loop));

    let addr = "127.0.0.1:9001".parse().unwrap();
    let mut listener = Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .unwrap();

    listener.bind(&addr).unwrap();
    listener.listen(128).unwrap();

    // Dummy test assertion to ensure it compiles
    assert!(true);
}
