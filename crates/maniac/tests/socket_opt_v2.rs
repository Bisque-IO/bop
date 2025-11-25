use maniac::buf::IoBuf;
use maniac::future::block_on;
use maniac::io::{AsyncReadRent, AsyncWriteRentExt};
use maniac::net::{TcpListener, TcpStream};
use maniac::runtime::DefaultExecutor;
use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};

#[test]
fn test_socket_optimization_flow() {
    // Create executor with 1 worker to ensure local optimization is always possible if scheduled on that worker
    let executor = DefaultExecutor::new(
        TaskArenaConfig::new(2, 1024).unwrap(),
        TaskArenaOptions::default(),
        1,
    )
    .expect("Failed to create executor");

    let handle = executor
        .spawn(async move {
            eprintln!("Main task started");
            // 1. Bind a listener
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind failed");
            let addr = listener.local_addr().expect("local_addr failed");
            eprintln!("Bound to {}", addr);

            // 2. Server task (local)
            let server_fut = async move {
                eprintln!("Server task started");
                let (mut stream, _) = listener.accept().await.expect("accept failed");
                eprintln!("Server accepted connection");
                let buf = vec![0u8; 1024];
                let (res, buf) = stream.read(buf).await;
                let n = res.expect("read failed");
                eprintln!("Server read {} bytes", n);
                let (res, _) = stream.write_all(buf.slice(0..n)).await;
                res.expect("write_all failed");
                eprintln!("Server wrote back");
            };

            // Client
            let client_fut = async move {
                eprintln!("Client connecting...");
                let mut stream = TcpStream::connect(addr).await.expect("connect failed");
                eprintln!("Client connected");
                let (res, _) = stream
                    .write_all(Vec::from(b"hello optimization".as_slice()))
                    .await;
                res.expect("client write failed");
                eprintln!("Client wrote data");

                let buf = vec![0u8; 1024];
                let (res, buf) = stream.read(buf).await;
                let n = res.expect("client read failed");
                eprintln!("Client read {} bytes", n);
                assert_eq!(&buf[0..n], b"hello optimization");
            };

            futures::join!(server_fut, client_fut);
        })
        .expect("spawn failed");

    eprintln!("Blocking on main task...");
    block_on(handle);
    eprintln!("Main task finished");
}
