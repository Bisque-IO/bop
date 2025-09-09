use std::net::TcpListener;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use bop_rs::usockets::{Loop, Socket, SocketContext, SocketContextOptions, SslMode, Timer};

fn free_tcp_port() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).expect("bind 127.0.0.1:0");
    l.local_addr().unwrap().port()
}

#[test]
fn usockets_timer_triggers_once() {
    // Simple timer smoke test: fires once, then loop exits.
    let ev = Loop::new().expect("create loop");
    let (tx, rx) = mpsc::channel::<()>();

    let mut t = Timer::new(&ev).expect("create timer");
    let tx2 = tx.clone();
    t.set(50, 0, move || {
        let _ = tx2.send(());
    });

    let start = Instant::now();
    ev.run();
    assert!(rx.recv_timeout(Duration::from_secs(1)).is_ok());
    assert!(start.elapsed() >= Duration::from_millis(40));
}

#[test]
fn usockets_tcp_client_server_echo() {
    // End-to-end TCP echo using one loop and two contexts.
    // Server listens on a free port, client connects and round-trips a message.
    let ev = Loop::new().expect("create loop");
    let port = free_tcp_port();

    // Channel used to signal when the echo is observed on the client.
    let (tx_done, rx_done) = mpsc::channel::<Vec<u8>>();

    // Server context & listener handle that we can close from within callbacks.
    let mut server_ctx = SocketContext::new(&ev, SslMode::Plain, SocketContextOptions::default())
        .expect("create server ctx");

    let listen_handle: Arc<Mutex<Option<_>>> = Arc::new(Mutex::new(None));
    let listen_handle_for_cb = listen_handle.clone();

    // Echo back any data received
    server_ctx.on_data(move |sock: Socket, data: &mut [u8]| {
        // Echo back
        let _ = sock.write(data, false);
        // For single-shot test we can drop the listen socket early to allow loop to exit later
        if let Ok(mut guard) = listen_handle_for_cb.lock() {
            if let Some(mut ls) = guard.take() {
                // Close the listening socket to avoid keeping the loop alive
                drop(ls);
            }
        }
    });

    // Install listener
    let ls = server_ctx
        .listen("127.0.0.1", port as i32, 0)
        .expect("listen on port");
    *listen_handle.lock().unwrap() = Some(ls);

    // Client context
    let mut client_ctx = SocketContext::new(&ev, SslMode::Plain, SocketContextOptions::default())
        .expect("create client ctx");

    // On open, send a ping
    let (tx_info, rx_info) = mpsc::channel::<i32>();
    let expected_port = port as i32;
    let txi = tx_info.clone();
    client_ctx.on_open(move |sock: Socket, _is_client: bool, _ip: &[u8]| {
        let _ = txi.send(sock.remote_port());
        let _ = sock.write(b"ping", false);
    });

    // On data, capture the echo and close the socket
    let tx_for_cb = tx_done.clone();
    client_ctx.on_data(move |sock: Socket, data: &mut [u8]| {
        let _ = tx_for_cb.send(data.to_vec());
        // Close this client socket; server side will follow
        sock.close(0);
    });

    // Connect the client to the server
    let _client = client_ctx
        .connect("127.0.0.1", port as i32, None, 0)
        .expect("client connect");

    // Drive the loop: should connect, exchange, close, and exit.
    ev.run();

    let echoed = rx_done
        .recv_timeout(Duration::from_secs(2))
        .expect("echo received");
    assert_eq!(echoed, b"ping");
    if let Ok(rp) = rx_info.try_recv() {
        assert_eq!(rp, expected_port);
    }
}
