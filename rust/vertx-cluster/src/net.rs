// use std::net::SocketAddr;
// use std::path::Path;
// use std::sync::Arc;
// use std::time::Duration;

// use compio_buf::BufResult;
// use compio_io::{AsyncRead, AsyncWrite};
// use compio_net::{TcpListener, TcpStream};
// use compio_runtime::spawn;
// use compio_tls::TlsAcceptor;
// use log::{debug, error, info, warn};
// use thiserror::Error;

// /// Networking error type for the Vertx Cluster server.
// #[derive(Debug, Error)]
// pub enum NetError {
//     #[error("io error: {0}")]
//     Io(#[from] std::io::Error),

//     #[error("tls error: {0}")]
//     Tls(#[from] rustls::Error),

//     #[error("tls config error: {0}")]
//     TlsConfig(String),
// }

// /// Convenience result alias for networking.
// pub type NetResult<T> = Result<T, NetError>;

// pub trait ConnectionEvents {
//     fn peer_addr(&self) -> SocketAddr;

//     fn is_connected(&self) -> bool;

//     /// Called when the connection is opened (either side)
//     fn on_open(&self, is_client: bool);

//     /// Called when the connection is closed (either side)
//     fn on_close(&self, reason: i32, message: &str);

//     /// Called when the connection is ended by the peer
//     fn on_end(&self, reason: i32, message: &str);

//     /// Called when a connection error occurs (only for client connections)
//     fn on_connect_error(&self, error: &str);

//     /// Called when data is received from the peer
//     fn on_data(&self, data: &[u8]);

//     /// Called when the connection is writable again after OS socket write buffer was full.
//     /// Backpressure handling needs to be built on top of this.
//     fn on_writable(&self);

//     /// Called when a timeout occurs.
//     fn on_timeout(&self);

//     /// Called when a long timeout occurs. This is for less frequent tasks
//     /// including connection migration was TTL expires.
//     fn on_long_timeout(&self);
// }

// /// Connection handler factory that creates a per-connection event handler.
// pub trait ConnectionHandler: Send + Sync + 'static {
//     fn new_connection(&self, peer_addr: SocketAddr, is_tls: bool)
//         -> Arc<dyn ConnectionEvents + Send + Sync>;
// }

// /// Simple example handler that logs events and acts like an echo server at the event level.
// /// Note: This handler only logs received data; the transport write is owned by the server
// /// loop and is not exposed via the ConnectionEvents trait.
// #[derive(Clone, Default)]
// pub struct EchoHandler;

// impl ConnectionHandler for EchoHandler {
//     fn new_connection(
//         &self,
//         peer_addr: SocketAddr,
//         _is_tls: bool,
//     ) -> Arc<dyn ConnectionEvents + Send + Sync> {
//         Arc::new(EchoConnection::new(peer_addr))
//     }
// }

// struct EchoConnection {
//     peer: SocketAddr,
//     connected: std::sync::atomic::AtomicBool,
// }

// impl EchoConnection {
//     fn new(peer: SocketAddr) -> Self {
//         Self {
//             peer: peer,
//             connected: std::sync::atomic::AtomicBool::new(true),
//         }
//     }
// }

// impl ConnectionEvents for EchoConnection {
//     fn peer_addr(&self) -> SocketAddr {
//         self.peer
//     }

//     fn is_connected(&self) -> bool {
//         self.connected
//             .load(std::sync::atomic::Ordering::Acquire)
//     }

//     fn on_open(&self, is_client: bool) {
//         info!(
//             target: "vertx-cluster::net",
//             "connection opened from {peer} (client={is_client})",
//             peer = self.peer
//         );
//     }

//     fn on_close(&self, reason: i32, message: &str) {
//         self.connected
//             .store(false, std::sync::atomic::Ordering::Release);
//         info!(
//             target: "vertx-cluster::net",
//             "connection closed from {peer} reason={reason} msg={message}",
//             peer = self.peer
//         );
//     }

//     fn on_end(&self, reason: i32, message: &str) {
//         info!(
//             target: "vertx-cluster::net",
//             "connection ended from {peer} reason={reason} msg={message}",
//             peer = self.peer
//         );
//     }

//     fn on_connect_error(&self, error: &str) {
//         warn!(
//             target: "vertx-cluster::net",
//             "connect error for {peer}: {error}",
//             peer = self.peer
//         );
//     }

//     fn on_data(&self, data: &[u8]) {
//         debug!(
//             target: "vertx-cluster::net",
//             "received {} bytes from {}",
//             data.len(),
//             self.peer
//         );
//     }

//     fn on_writable(&self) {
//         debug!(target: "vertx-cluster::net", "socket writable: {}", self.peer);
//     }

//     fn on_timeout(&self) {
//         debug!(target: "vertx-cluster::net", "timeout: {}", self.peer);
//     }

//     fn on_long_timeout(&self) {
//         debug!(target: "vertx-cluster::net", "long timeout: {}", self.peer);
//     }
// }

// /// TLS configuration provider using rustls.
// pub struct TlsConfig {
//     cert_chain: Vec<rustls::pki_types::CertificateDer<'static>>,
//     private_key: rustls::pki_types::PrivateKeyDer<'static>,
//     alpn_protocols: Vec<Vec<u8>>,
// }

// impl TlsConfig {
//     /// Load a TLS config from PEM-encoded certificate chain and private key files.
//     pub fn from_pem_files<P: AsRef<Path>>(cert_path: P, key_path: P) -> NetResult<Self> {
//         // Load certs
//         let mut cert_file = std::io::BufReader::new(std::fs::File::open(cert_path)?);
//         let certs: Result<Vec<_>, _> = rustls_pemfile::certs(&mut cert_file).collect();
//         let cert_chain = certs.map_err(|e| NetError::TlsConfig(format!("invalid certs: {e}")))?;

//         // Load private key
//         let mut key_file = std::io::BufReader::new(std::fs::File::open(key_path)?);
//         let key = rustls_pemfile::private_key(&mut key_file)
//             .map_err(|e| NetError::TlsConfig(format!("invalid private key: {e}")))?
//             .ok_or_else(|| NetError::TlsConfig("missing private key".into()))?;

//         Ok(Self {
//             cert_chain,
//             private_key: key,
//             alpn_protocols: Vec::new(),
//         })
//     }

//     /// Set ALPN protocols (e.g., [b"h2".to_vec(), b"http/1.1".to_vec()]).
//     pub fn with_alpn_protocols(mut self, protos: Vec<Vec<u8>>) -> Self {
//         self.alpn_protocols = protos;
//         self
//     }

//     fn build_acceptor(&self) -> NetResult<TlsAcceptor> {
//         let mut cfg = rustls::ServerConfig::builder()
//             .with_no_client_auth()
//             .with_single_cert(self.cert_chain.clone(), self.private_key.clone())?;
//         if !self.alpn_protocols.is_empty() {
//             cfg.alpn_protocols = self.alpn_protocols.clone();
//         }
//         Ok(TlsAcceptor::from(Arc::new(cfg)))
//     }
// }

// /// TCP server configuration
// pub struct TcpServerConfig {
//     pub addr: SocketAddr,
//     pub nodelay: bool,
//     pub read_buffer_size: usize,
//     pub idle_timeout: Option<Duration>,
//     pub long_timeout: Option<Duration>,
//     pub tls: Option<TlsConfig>,
// }

// impl TcpServerConfig {
//     pub fn new(addr: SocketAddr) -> Self {
//         Self {
//             addr,
//             nodelay: true,
//             read_buffer_size: 16 * 1024,
//             idle_timeout: None,
//             long_timeout: Some(Duration::from_secs(30)),
//             tls: None,
//         }
//     }
// }

// /// A TCP server that accepts plain or TLS connections and dispatches ConnectionEvents.
// pub struct TcpServer<H: ConnectionHandler> {
//     config: TcpServerConfig,
//     handler: Arc<H>,
// }

// impl<H: ConnectionHandler> TcpServer<H> {
//     pub fn new(config: TcpServerConfig, handler: H) -> Self {
//         Self {
//             config,
//             handler: Arc::new(handler),
//         }
//     }

//     /// Run the server accept loop until the task is cancelled.
//     pub async fn run(&self) -> NetResult<()> {
//         let listener = TcpListener::bind(self.config.addr).await?;
//         info!(
//             target: "vertx-cluster::net",
//             "listening on {}{}",
//             self.config.addr,
//             if self.config.tls.is_some() { " (tls)" } else { "" }
//         );
//         let tls_acceptor = match &self.config.tls {
//             Some(tls) => Some(tls.build_acceptor()?),
//             None => None,
//         };

//         loop {
//             let (mut stream, peer) = match listener.accept().await {
//                 Ok(x) => x,
//                 Err(e) => {
//                     error!(target: "vertx-cluster::net", "accept error: {e}");
//                     continue;
//                 }
//             };

//             // Note: TCP_NODELAY can be set on the underlying socket if needed.
//             // compio provides access to the inner socket via PollFd/SharedFd,
//             // but we keep it simple here and rely on system defaults.

//             let handler = self.handler.clone();
//             let read_buf_sz = self.config.read_buffer_size;
//             let idle_to = self.config.idle_timeout;
//             let long_to = self.config.long_timeout;
//             let is_tls = tls_acceptor.is_some();

//             match tls_acceptor.clone() {
//                 Some(acceptor) => {
//                     spawn(async move {
//                         if let Err(e) = handle_tls_conn(acceptor, stream, peer, handler, read_buf_sz, idle_to, long_to).await {
//                             error!(target: "vertx-cluster::net", "tls connection error from {peer}: {e}");
//                         }
//                     });
//                 }
//                 None => {
//                     spawn(async move {
//                         if let Err(e) = handle_plain_conn(stream, peer, handler, read_buf_sz, idle_to, long_to).await {
//                             error!(target: "vertx-cluster::net", "connection error from {peer}: {e}");
//                         }
//                     });
//                 }
//             }
//         }
//     }
// }

// async fn handle_plain_conn<H: ConnectionHandler>(
//     mut stream: TcpStream,
//     peer: SocketAddr,
//     handler: Arc<H>,
//     read_buf_sz: usize,
//     idle_to: Option<Duration>,
//     long_to: Option<Duration>,
// ) -> NetResult<()> {
//     let conn = handler.new_connection(peer, false);
//     conn.on_open(false);

//     // Timers
//     if let Some(dur) = idle_to {
//         let conn_cloned = conn.clone();
//         spawn(async move {
//             loop {
//                 compio_runtime::time::sleep(dur).await;
//                 conn_cloned.on_timeout();
//             }
//         });
//     }
//     if let Some(dur) = long_to {
//         let conn_cloned = conn.clone();
//         spawn(async move {
//             loop {
//                 compio_runtime::time::sleep(dur).await;
//                 conn_cloned.on_long_timeout();
//             }
//         });
//     }

//     loop {
//         let BufResult(res, buf) = stream.read(Vec::with_capacity(read_buf_sz)).await;
//         match res {
//             Ok(0) => {
//                 conn.on_end(0, "eof");
//                 conn.on_close(0, "peer closed");
//                 return Ok(());
//             }
//             Ok(n) => {
//                 conn.on_data(&buf[..n]);
//             }
//             Err(e) => {
//                 conn.on_close(-1, &format!("read error: {e}"));
//                 return Err(NetError::Io(e));
//             }
//         }
//     }
// }

// async fn handle_tls_conn<H: ConnectionHandler>(
//     acceptor: TlsAcceptor,
//     stream: TcpStream,
//     peer: SocketAddr,
//     handler: Arc<H>,
//     read_buf_sz: usize,
//     idle_to: Option<Duration>,
//     long_to: Option<Duration>,
// ) -> NetResult<()> {
//     let mut tls_stream = acceptor.accept(stream).await?;
//     let conn = handler.new_connection(peer, true);
//     conn.on_open(false);

//     // Timers
//     if let Some(dur) = idle_to {
//         let conn_cloned = conn.clone();
//         spawn(async move {
//             loop {
//                 compio_runtime::time::sleep(dur).await;
//                 conn_cloned.on_timeout();
//             }
//         });
//     }
//     if let Some(dur) = long_to {
//         let conn_cloned = conn.clone();
//         spawn(async move {
//             loop {
//                 compio_runtime::time::sleep(dur).await;
//                 conn_cloned.on_long_timeout();
//             }
//         });
//     }

//     loop {
//         let BufResult(res, buf) = tls_stream.read(Vec::with_capacity(read_buf_sz)).await;
//         match res {
//             Ok(0) => {
//                 conn.on_end(0, "eof");
//                 conn.on_close(0, "peer closed");
//                 return Ok(());
//             }
//             Ok(n) => {
//                 conn.on_data(&buf[..n]);
//             }
//             Err(e) => {
//                 conn.on_close(-1, &format!("read error: {e}"));
//                 return Err(NetError::Io(e));
//             }
//         }
//     }
// }
