use futures::io::{AsyncReadExt, AsyncWriteExt};
use maniac_runtime::future::block_on;
use maniac_runtime::net::{TcpListener, TcpStream};
use maniac_runtime::runtime::task::{TaskArenaConfig, TaskArenaOptions};
use maniac_runtime::runtime::{DefaultExecutor, Executor};
use std::io;

#[test]
fn test_socket_optimization_flow() {
    // Create executor with 1 worker to ensure local optimization is always possible if scheduled on that worker
    let executor = DefaultExecutor::new(
        TaskArenaConfig::new(2, 1024).unwrap(),
        TaskArenaOptions::default(),
        2, // 2 workers
        2,
    )
    .expect("Failed to create executor");

    let executor_clone = executor.clone();

    let handle = executor
        .spawn(async move {
            // 1. Bind a listener
            let mut listener = TcpListener::bind("127.0.0.1:0").await.expect("bind failed");
            let addr = listener.local_addr().expect("local_addr failed");

            // 2. Connect a client (this will register the client socket)
            // We spawn a separate task for the client to avoid deadlocks if single-threaded
            // But since we are in async, we can just use join or similar, but here we'll just do it sequentially-ish
            // or use a background task.

            // Let's accept in a loop in background
            let server_handle = executor_clone
                .spawn(async move {
                    let (mut stream, _) = listener.accept().await.expect("accept failed");
                    let mut buf = [0u8; 1024];
                    let n = stream.read(&mut buf).await.expect("read failed");
                    stream
                        .write_all(&buf[0..n])
                        .await
                        .expect("write_all failed");
                })
                .expect("server spawn failed");

            // Client
            let mut stream = TcpStream::connect(addr).await.expect("connect failed");
            stream
                .write_all(b"hello optimization")
                .await
                .expect("client write failed");

            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.expect("client read failed");
            assert_eq!(&buf[0..n], b"hello optimization");

            server_handle.await;
        })
        .expect("spawn failed");

    block_on(handle);
}
