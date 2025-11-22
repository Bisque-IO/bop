/// TLS support using rustls 0.23 + aws-lc-rs
///
/// This module provides TLS support for the Future-based TCP API,
/// wrapping TcpStream with TLS encryption/decryption.
///
/// Features:
/// - TLS 1.2 and TLS 1.3 support
/// - Client and server modes
/// - Async API matching TcpStream
/// - Seamless integration with maniac-runtime

use std::io::{self, Read as StdRead, Write as StdWrite};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::monoio::io::{AsyncReadRent, AsyncWriteRentExt};
use rustls::{ServerConfig, ClientConfig, ServerConnection, ClientConnection};
use rustls::pki_types::ServerName;

use super::tcp::TcpStream;

/// TLS configuration for server or client
pub enum TlsConfig {
    Server(Arc<ServerConfig>),
    Client(Arc<ClientConfig>),
}

/// TLS connection state
enum TlsConnection {
    Server(ServerConnection),
    Client(ClientConnection),
}

impl TlsConnection {
    /// Check if the TLS handshake is still in progress
    fn is_handshaking(&self) -> bool {
        match self {
            TlsConnection::Server(conn) => conn.is_handshaking(),
            TlsConnection::Client(conn) => conn.is_handshaking(),
        }
    }

    /// Check if we want to read from the socket
    fn wants_read(&self) -> bool {
        match self {
            TlsConnection::Server(conn) => conn.wants_read(),
            TlsConnection::Client(conn) => conn.wants_read(),
        }
    }

    /// Check if we want to write to the socket
    fn wants_write(&self) -> bool {
        match self {
            TlsConnection::Server(conn) => conn.wants_write(),
            TlsConnection::Client(conn) => conn.wants_write(),
        }
    }

    /// Process new packets read from the socket
    fn read_tls(&mut self, rd: &mut dyn StdRead) -> io::Result<usize> {
        match self {
            TlsConnection::Server(conn) => conn.read_tls(rd),
            TlsConnection::Client(conn) => conn.read_tls(rd),
        }
    }

    /// Write TLS packets to the socket
    fn write_tls(&mut self, wr: &mut dyn StdWrite) -> io::Result<usize> {
        match self {
            TlsConnection::Server(conn) => conn.write_tls(wr),
            TlsConnection::Client(conn) => conn.write_tls(wr),
        }
    }

    /// Process any new packets
    fn process_new_packets(&mut self) -> Result<rustls::IoState, rustls::Error> {
        match self {
            TlsConnection::Server(conn) => conn.process_new_packets(),
            TlsConnection::Client(conn) => conn.process_new_packets(),
        }
    }

    /// Read plaintext data after decryption
    fn reader(&mut self) -> rustls::Reader<'_> {
        match self {
            TlsConnection::Server(conn) => conn.reader(),
            TlsConnection::Client(conn) => conn.reader(),
        }
    }

    /// Write plaintext data to be encrypted
    fn writer(&mut self) -> rustls::Writer<'_> {
        match self {
            TlsConnection::Server(conn) => conn.writer(),
            TlsConnection::Client(conn) => conn.writer(),
        }
    }
}

/// A TLS stream wrapping a TcpStream with encryption/decryption
pub struct TlsStream {
    /// The underlying TCP stream
    stream: TcpStream,

    /// TLS connection state
    tls: TlsConnection,

    /// Buffer for outgoing encrypted data waiting to be sent
    write_buffer: Vec<u8>,

    /// Buffer for incoming encrypted data read from socket
    read_buffer: Vec<u8>,

    /// Whether handshake is complete
    handshake_complete: bool,
}

impl TlsStream {
    /// Create a new TLS client stream
    pub fn new_client(
        stream: TcpStream,
        config: Arc<ClientConfig>,
        server_name: ServerName<'static>,
    ) -> io::Result<Self> {
        let tls = ClientConnection::new(config, server_name)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Self {
            stream,
            tls: TlsConnection::Client(tls),
            write_buffer: Vec::with_capacity(16384),
            read_buffer: vec![0u8; 16384],
            handshake_complete: false,
        })
    }

    /// Create a new TLS server stream
    pub fn new_server(stream: TcpStream, config: Arc<ServerConfig>) -> io::Result<Self> {
        let tls = ServerConnection::new(config)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Self {
            stream,
            tls: TlsConnection::Server(tls),
            write_buffer: Vec::with_capacity(16384),
            read_buffer: vec![0u8; 16384],
            handshake_complete: false,
        })
    }

    /// Get local address
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.stream.local_addr()
    }

    /// Get peer address
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.stream.peer_addr()
    }

    /// Read plaintext data (performs handshake first if needed)
    ///
    /// # Example
    /// ```no_run
    /// use maniac_runtime::net::{TcpStream, TlsStream};
    /// use std::sync::Arc;
    /// use rustls::ClientConfig;
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let stream = TcpStream::connect("example.com:443").await?;
    /// let mut tls = TlsStream::new_client(stream, Arc::new(ClientConfig::builder().build()), "example.com".try_into().unwrap())?;
    ///
    /// let mut buf = [0u8; 1024];
    /// let n = tls.read(&mut buf).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Complete handshake first
        if !self.handshake_complete {
            self.do_handshake().await?;
        }

        // Try to read from rustls plaintext buffer first
        let mut reader = self.tls.reader();
        match reader.read(buf) {
            Ok(n) if n > 0 => return Ok(n),
            Ok(_) => {
                // No plaintext available, need to read encrypted data
            }
            Err(e) => return Err(e),
        }

        // Read encrypted data from socket
        let mut buffer = std::mem::take(&mut self.read_buffer);
        // Ensure buffer has space
        if buffer.len() < 16384 {
            buffer.resize(16384, 0);
        }

        let (res, buffer) = self.stream.read(buffer).await;
        self.read_buffer = buffer;
        let n = res?;

        if n == 0 {
            return Ok(0);
        }

        // Feed encrypted data to rustls
        let mut cursor = io::Cursor::new(&self.read_buffer[..n]);
        self.tls.read_tls(&mut cursor)?;
        self.tls.process_new_packets()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Try reading plaintext again
        let mut reader = self.tls.reader();
        reader.read(buf)
    }

    /// Write plaintext data (performs handshake first if needed)
    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Complete handshake first
        if !self.handshake_complete {
            self.do_handshake().await?;
        }

        // Write plaintext to rustls
        let mut writer = self.tls.writer();
        let n = writer.write(buf)?;

        // Get encrypted data from rustls
        self.write_buffer.clear();
        self.tls.write_tls(&mut self.write_buffer)?;

        // Write encrypted data to socket
        if !self.write_buffer.is_empty() {
            let buffer = std::mem::take(&mut self.write_buffer);
            let (res, mut buffer) = self.stream.write_all(buffer).await;
            buffer.clear();
            self.write_buffer = buffer;
            res?;
        }

        Ok(n)
    }

    /// Write all plaintext data (performs handshake first if needed)
    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        let mut written = 0;
        while written < buf.len() {
            let n = self.write(&buf[written..]).await?;
            written += n;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
        }
        Ok(())
    }

    /// Perform TLS handshake
    async fn do_handshake(&mut self) -> io::Result<()> {
        while self.tls.is_handshaking() {
            // Read encrypted data from socket if rustls wants it
            if self.tls.wants_read() {
                let mut buffer = std::mem::take(&mut self.read_buffer);
                if buffer.len() < 16384 { buffer.resize(16384, 0); }
                
                let (res, buffer) = self.stream.read(buffer).await;
                self.read_buffer = buffer;
                let n = res?;

                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "connection closed during TLS handshake",
                    ));
                }

                // Feed encrypted data to rustls
                let mut cursor = io::Cursor::new(&self.read_buffer[..n]);
                self.tls.read_tls(&mut cursor)?;
                self.tls.process_new_packets()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            }

            // Write encrypted data to socket if rustls has it
            if self.tls.wants_write() {
                self.write_buffer.clear();
                self.tls.write_tls(&mut self.write_buffer)?;

                if !self.write_buffer.is_empty() {
                    let buffer = std::mem::take(&mut self.write_buffer);
                    let (res, mut buffer) = self.stream.write_all(buffer).await;
                    buffer.clear();
                    self.write_buffer = buffer;
                    res?;
                }
            }
        }

        self.handshake_complete = true;
        Ok(())
    }
}
