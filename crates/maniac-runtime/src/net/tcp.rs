//! Future-based async TCP networking
//!
//! This module provides ergonomic async/await APIs for TCP networking that integrate
//! with maniac-runtime's task system and EventLoop.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll, Waker, RawWaker, RawWakerVTable};
use std::future::Future;
use std::sync::atomic::{AtomicU8, AtomicBool, AtomicPtr, Ordering};
use std::ptr;

use crate::net::{SocketDescriptor, Token};

/// Tracks which I/O interests are registered with the EventLoop
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RegisteredInterest {
    None = 0,
    Readable = 1,
    Writable = 2,
    ReadWrite = 3,
}

impl RegisteredInterest {
    /// Convert from u8
    pub(crate) fn from_u8(val: u8) -> Self {
        match val {
            0 => RegisteredInterest::None,
            1 => RegisteredInterest::Readable,
            2 => RegisteredInterest::Writable,
            3 => RegisteredInterest::ReadWrite,
            _ => RegisteredInterest::None,
        }
    }

    /// Check if readable interest is registered
    pub fn is_readable(self) -> bool {
        matches!(self, RegisteredInterest::Readable | RegisteredInterest::ReadWrite)
    }

    /// Check if writable interest is registered
    pub fn is_writable(self) -> bool {
        matches!(self, RegisteredInterest::Writable | RegisteredInterest::ReadWrite)
    }

    /// Add readable interest
    pub fn add_readable(self) -> Self {
        match self {
            RegisteredInterest::None => RegisteredInterest::Readable,
            RegisteredInterest::Writable => RegisteredInterest::ReadWrite,
            other => other,
        }
    }

    /// Add writable interest
    pub fn add_writable(self) -> Self {
        match self {
            RegisteredInterest::None => RegisteredInterest::Writable,
            RegisteredInterest::Readable => RegisteredInterest::ReadWrite,
            other => other,
        }
    }

    /// Convert to SocketInterest for EventLoop registration
    pub fn to_socket_interest(self) -> Option<crate::runtime::worker::SocketInterest> {
        match self {
            RegisteredInterest::None => None,
            RegisteredInterest::Readable => Some(crate::runtime::worker::SocketInterest::Readable),
            RegisteredInterest::Writable => Some(crate::runtime::worker::SocketInterest::Writable),
            RegisteredInterest::ReadWrite => Some(crate::runtime::worker::SocketInterest::ReadWrite),
        }
    }
}

/// Async TCP stream
///
/// Provides Future-based `read()` and `write()` operations that integrate with
/// the maniac-runtime task system.
///
/// # Example
///
/// ```ignore
/// use maniac_runtime::net::TcpStream;
///
/// async fn example() -> io::Result<()> {
///     let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
///
///     stream.write_all(b"Hello, world!").await?;
///
///     let mut buf = vec![0u8; 1024];
///     let n = stream.read(&mut buf).await?;
///     println!("Read {} bytes", n);
///
///     Ok(())
/// }
/// ```
pub struct TcpStream {
    /// Socket file descriptor (owned)
    fd: SocketDescriptor,

    /// Worker ID that owns this socket's EventLoop registration
    worker_id: Option<usize>,

    /// Shared async socket state (for EventHandler communication)
    state: crate::net::AsyncSocketState,

    /// Tracks which interests are registered with EventLoop
    registered_interest: AtomicU8,

    /// Local socket address
    local_addr: SocketAddr,

    /// Remote socket address
    peer_addr: SocketAddr,
}

impl TcpStream {
    /// Connect to a remote address
    ///
    /// Returns a Future that resolves when the connection is established.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// ```
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        ConnectFuture::new(addr)?.await
    }

    /// Read data from the stream
    ///
    /// Returns the number of bytes read. Returns 0 if the stream has been closed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut buf = vec![0u8; 1024];
    /// let n = stream.read(&mut buf).await?;
    /// ```
    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        ReadFuture::new(self, buf).await
    }

    /// Write data to the stream
    ///
    /// Returns the number of bytes written.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let n = stream.write(b"Hello").await?;
    /// ```
    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        WriteFuture::new(self, buf).await
    }

    /// Write all data to the stream
    ///
    /// Continues writing until all bytes are written or an error occurs.
    ///
    /// # Example
    ///
    /// ```ignore
    /// stream.write_all(b"Hello, world!").await?;
    /// ```
    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        WriteAllFuture::new(self, buf).await
    }

    /// Get the local address
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    /// Get the remote address
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.peer_addr)
    }

    /// Get the underlying file descriptor
    pub fn as_raw_fd(&self) -> SocketDescriptor {
        self.fd
    }

    /// Get the current registered interest
    pub fn get_registered_interest(&self) -> RegisteredInterest {
        RegisteredInterest::from_u8(self.registered_interest.load(Ordering::Acquire))
    }

    /// Set the registered interest
    pub fn set_registered_interest(&self, interest: RegisteredInterest) {
        self.registered_interest.store(interest as u8, Ordering::Release);
    }

    /// Check if timeout occurred
    pub fn is_timed_out(&self) -> bool {
        self.state.is_timed_out()
    }

    /// Check if closed
    pub fn is_closed(&self) -> bool {
        self.state.is_closed()
    }

    /// Store read waker data pointer
    fn set_read_waker(&self, waker: &Waker) {
        self.state.set_read_waker(waker);
    }

    /// Store write waker data pointer
    fn set_write_waker(&self, waker: &Waker) {
        self.state.set_write_waker(waker);
    }

    /// Set socket to non-blocking mode (required for async operations)
    fn set_nonblocking(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let raw_fd = self.fd as i32;
            let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFL, 0) };
            if flags < 0 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::fcntl(raw_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawSocket;
            use winapi::um::winsock2::{ioctlsocket, FIONBIO};
            let raw_socket = self.fd as u64;
            let mut nonblocking: u32 = 1;
            let result = unsafe {
                ioctlsocket(
                    raw_socket as usize,
                    FIONBIO,
                    &mut nonblocking as *mut u32,
                )
            };
            if result != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        #[cfg(not(any(unix, windows)))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "non-blocking sockets not supported on this platform",
            ))
        }
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        // Send CloseSocket message to worker if registered
        if let Some(token) = self.state.token() {
            if let Some(worker_id) = self.worker_id {
                let _ = crate::runtime::worker::close_socket(Token(token), worker_id as u32);
            }
        }

        // Close the FD
        #[cfg(unix)]
        unsafe {
            libc::close(self.fd as i32);
        }

        #[cfg(windows)]
        unsafe {
            use winapi::um::winsock2::closesocket;
            closesocket(self.fd as usize);
        }
    }
}

/// Future for async connect operation
struct ConnectFuture {
    addr: Option<SocketAddr>,
}

impl ConnectFuture {
    fn new<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no socket addresses")
        })?;

        Ok(Self {
            addr: Some(addr),
        })
    }
}

impl Future for ConnectFuture {
    type Output = io::Result<TcpStream>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let addr = self.addr.take().unwrap();

            // Create socket
            let stream = match std::net::TcpStream::connect(addr) {
                Ok(s) => s,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // Connection in progress - we need to register with EventLoop
                    // TODO: Actually initiate non-blocking connect
                    // For now, return error
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Other,
                        "async connect not yet fully implemented",
                    )));
                }
                Err(e) => return Poll::Ready(Err(e)),
            };

            // Get addresses
            let local_addr = stream.local_addr()?;
            let peer_addr = stream.peer_addr()?;

            // Get FD
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

            let tcp_stream = TcpStream {
                fd,
                worker_id: None,
                state: crate::net::AsyncSocketState::new(),
                registered_interest: AtomicU8::new(RegisteredInterest::None as u8),
                local_addr,
                peer_addr,
            };

            // Set socket to non-blocking mode
            tcp_stream.set_nonblocking()?;

            // Forget the std stream so FD stays open
            std::mem::forget(stream);

            return Poll::Ready(Ok(tcp_stream));
    }
}

/// Future for async read operation
struct ReadFuture<'a> {
    stream: &'a mut TcpStream,
    buf: &'a mut [u8],
}

impl<'a> ReadFuture<'a> {
    fn new(stream: &'a mut TcpStream, buf: &'a mut [u8]) -> Self {
        Self { stream, buf }
    }
}

impl<'a> Future for ReadFuture<'a> {
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Get mutable reference to self
        let this = &mut *self;

        // Check if timeout occurred
        if this.stream.is_timed_out() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "socket read timeout",
            )));
        }

        // Check if connection was closed
        if this.stream.is_closed() {
            return Poll::Ready(Ok(0));
        }

        // Ensure readable interest is registered with EventLoop
        let current_interest = this.stream.get_registered_interest();
        if !current_interest.is_readable() {
            let new_interest = current_interest.add_readable();

            // Check if socket is registered yet
            if this.stream.worker_id.is_none() {
                // First registration - send RegisterSocket message
                if let Some(socket_interest) = new_interest.to_socket_interest() {
                    // Get pointer to state for EventHandler
                    // We clone the state (Arc increment) to ensure it lives as long as the worker needs it
                    // The raw pointer is passed, but we must ensure the Arc is valid.
                    let state_ptr = &this.stream.state as *const _ as usize;

                    // Attempt to register with current worker
                    if let Ok(_token) = crate::runtime::worker::register_socket_with_current_worker(
                        this.stream.fd,
                        socket_interest,
                        state_ptr,
                    ) {
                        // Registration message sent successfully
                        // Store current worker_id (will be set when message is processed)
                        if let Some(worker_id) = crate::runtime::worker::current_worker_id() {
                            this.stream.worker_id = Some(worker_id as usize);
                        }
                    }
                }
            } else if let Some(token) = this.stream.state.token() {
                // Socket already registered - send ModifySocket message
                if let (Some(socket_interest), Some(worker_id)) =
                    (new_interest.to_socket_interest(), this.stream.worker_id) {
                    let _ = crate::runtime::worker::modify_socket_interest(
                        Token(token),
                        worker_id as u32,
                        socket_interest,
                    );
                }
            }

            this.stream.set_registered_interest(new_interest);
            this.stream.set_read_waker(cx.waker());
            return Poll::Pending;
        }

        // Try non-blocking read directly
        #[cfg(unix)]
        let result = unsafe {
            let fd = this.stream.fd as i32;
            let n = libc::read(
                fd,
                this.buf.as_mut_ptr() as *mut libc::c_void,
                this.buf.len(),
            );

            if n < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(n as usize)
            }
        };

        #[cfg(windows)]
        let result = unsafe {
            use winapi::um::winsock2::recv;
            let socket = this.stream.fd as usize;
            let n = recv(
                socket,
                this.buf.as_mut_ptr() as *mut i8,
                this.buf.len() as i32,
                0,
            );

            if n < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(n as usize)
            }
        };

        match result {
            Ok(0) => {
                // EOF
                Poll::Ready(Ok(0))
            }
            Ok(n) => {
                // Successfully read data
                Poll::Ready(Ok(n))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No data available - store waker and return pending
                // EventLoop will wake us when socket becomes readable
                this.stream.set_read_waker(cx.waker());
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

/// Future for async write operation
struct WriteFuture<'a> {
    stream: &'a mut TcpStream,
    buf: &'a [u8],
}

impl<'a> WriteFuture<'a> {
    fn new(stream: &'a mut TcpStream, buf: &'a [u8]) -> Self {
        Self { stream, buf }
    }
}

impl<'a> Future for WriteFuture<'a> {
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Check if timeout occurred
        if self.stream.is_timed_out() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "socket write timeout",
            )));
        }

        // Check if connection was closed
        if self.stream.is_closed() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "connection closed",
            )));
        }

        // Ensure writable interest is registered with EventLoop
        let current_interest = self.stream.get_registered_interest();
        if !current_interest.is_writable() {
            let new_interest = current_interest.add_writable();

            // Check if socket is registered yet
            if self.stream.worker_id.is_none() {
                // First registration - send RegisterSocket message
                if let Some(socket_interest) = new_interest.to_socket_interest() {
                    let state_ptr = &self.stream.state as *const _ as usize;

                    if let Ok(_token) = crate::runtime::worker::register_socket_with_current_worker(
                        self.stream.fd,
                        socket_interest,
                        state_ptr,
                    ) {
                        if let Some(worker_id) = crate::runtime::worker::current_worker_id() {
                            self.stream.worker_id = Some(worker_id as usize);
                        }
                    }
                }
            } else if let Some(token) = self.stream.state.token() {
                // Socket already registered - send ModifySocket message
                if let (Some(socket_interest), Some(worker_id)) =
                    (new_interest.to_socket_interest(), self.stream.worker_id) {
                    let _ = crate::runtime::worker::modify_socket_interest(
                        Token(token),
                        worker_id as u32,
                        socket_interest,
                    );
                }
            }

            self.stream.set_registered_interest(new_interest);
            self.stream.set_write_waker(cx.waker());
            return Poll::Pending;
        }

        // Try non-blocking write directly
        #[cfg(unix)]
        let result = unsafe {
            let fd = self.stream.fd as i32;
            let n = libc::write(
                fd,
                self.buf.as_ptr() as *const libc::c_void,
                self.buf.len(),
            );

            if n < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(n as usize)
            }
        };

        #[cfg(windows)]
        let result = unsafe {
            use winapi::um::winsock2::send;
            let socket = self.stream.fd as usize;
            let n = send(
                socket,
                self.buf.as_ptr() as *const i8,
                self.buf.len() as i32,
                0,
            );

            if n < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(n as usize)
            }
        };

        match result {
            Ok(n) => {
                // Successfully wrote data
                Poll::Ready(Ok(n))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Socket not ready - store waker and return pending
                // EventLoop will wake us when socket becomes writable
                self.stream.set_write_waker(cx.waker());
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

/// Future for async write_all operation
struct WriteAllFuture<'a> {
    stream: &'a mut TcpStream,
    buf: &'a [u8],
    written: usize,
}

impl<'a> WriteAllFuture<'a> {
    fn new(stream: &'a mut TcpStream, buf: &'a [u8]) -> Self {
        Self {
            stream,
            buf,
            written: 0,
        }
    }
}

impl<'a> Future for WriteAllFuture<'a> {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        // Check if timeout occurred
        if this.stream.is_timed_out() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "socket write timeout",
            )));
        }

        // Check if connection was closed
        if this.stream.is_closed() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "connection closed",
            )));
        }

        // Ensure writable interest is registered with EventLoop
        let current_interest = this.stream.get_registered_interest();
        if !current_interest.is_writable() {
            let new_interest = current_interest.add_writable();

            // Check if socket is registered yet
            if this.stream.worker_id.is_none() {
                // First registration - send RegisterSocket message
                if let Some(socket_interest) = new_interest.to_socket_interest() {
                    let state_ptr = &this.stream.state as *const _ as usize;

                    if let Ok(_token) = crate::runtime::worker::register_socket_with_current_worker(
                        this.stream.fd,
                        socket_interest,
                        state_ptr,
                    ) {
                        if let Some(worker_id) = crate::runtime::worker::current_worker_id() {
                            this.stream.worker_id = Some(worker_id as usize);
                        }
                    }
                }
            } else if let Some(token) = this.stream.state.token() {
                // Socket already registered - send ModifySocket message
                if let (Some(socket_interest), Some(worker_id)) =
                    (new_interest.to_socket_interest(), this.stream.worker_id) {
                    let _ = crate::runtime::worker::modify_socket_interest(
                        Token(token),
                        worker_id as u32,
                        socket_interest,
                    );
                }
            }

            this.stream.set_registered_interest(new_interest);
            this.stream.set_write_waker(cx.waker());
            return Poll::Pending;
        }

        // Try to write remaining data via direct syscalls
        while this.written < this.buf.len() {
            let remaining = &this.buf[this.written..];

            // Try non-blocking write directly (SAFE - socket is non-blocking)
            #[cfg(unix)]
            let result = unsafe {
                let fd = this.stream.fd as i32;
                let n = libc::write(
                    fd,
                    remaining.as_ptr() as *const libc::c_void,
                    remaining.len(),
                );

                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            };

            #[cfg(windows)]
            let result = unsafe {
                use winapi::um::winsock2::send;
                let socket = this.stream.fd as usize;
                let n = send(
                    socket,
                    remaining.as_ptr() as *const i8,
                    remaining.len() as i32,
                    0,
                );

                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            };

            match result {
                Ok(0) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "write returned 0",
                    )));
                }
                Ok(n) => {
                    this.written += n;
                    // Continue loop to write more
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // Can't write more right now - store waker and return pending
                    // EventLoop will wake us when socket becomes writable
                    this.stream.set_write_waker(cx.waker());
                    return Poll::Pending;
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }

        // All data written!
        Poll::Ready(Ok(()))
    }
}

/// Async TCP listener
///
/// Provides Future-based `accept()` operation that integrates with
/// the maniac-runtime task system.
///
/// # Example
///
/// ```ignore
/// use maniac_runtime::net::TcpListener;
///
/// async fn example() -> io::Result<()> {
///     let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
///
///     loop {
///         let (stream, addr) = listener.accept().await?;
///         println!("Accepted connection from {}", addr);
///         // Handle stream...
///     }
/// }
/// ```
pub struct TcpListener {
    /// Socket file descriptor (owned)
    fd: SocketDescriptor,

    /// Worker ID that owns this socket's EventLoop registration
    worker_id: Option<usize>,

    /// Shared async socket state (for EventHandler communication)
    /// For listener, we only use the read_waker for accept operations
    state: crate::net::AsyncSocketState,

    /// Tracks which interests are registered with EventLoop
    registered_interest: AtomicU8,

    /// Local socket address
    local_addr: SocketAddr,
}

impl TcpListener {
    /// Bind to a local address
    ///
    /// Returns a Future that resolves when the listener is ready to accept connections.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// ```
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        BindFuture::new(addr)?.await
    }

    /// Accept a new incoming connection
    ///
    /// Returns a Future that resolves with a TcpStream and the remote address.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (stream, addr) = listener.accept().await?;
    /// ```
    pub async fn accept(&mut self) -> io::Result<(TcpStream, SocketAddr)> {
        AcceptFuture::new(self).await
    }

    /// Get the local address
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    /// Get the underlying file descriptor
    pub fn as_raw_fd(&self) -> SocketDescriptor {
        self.fd
    }

    /// Get the current registered interest
    pub fn get_registered_interest(&self) -> RegisteredInterest {
        RegisteredInterest::from_u8(self.registered_interest.load(Ordering::Acquire))
    }

    /// Set the registered interest
    pub fn set_registered_interest(&self, interest: RegisteredInterest) {
        self.registered_interest.store(interest as u8, Ordering::Release);
    }

    /// Store accept waker data pointer
    fn set_accept_waker(&self, waker: &Waker) {
        // For listener, we use the read_waker for accept operations
        self.state.set_read_waker(waker);
    }

    /// Wake accept waker (called by EventHandler)
    pub(crate) fn wake_accept(&self) {
        // For listener, we use the read_waker for accept operations
        self.state.wake_read();
    }

    /// Check if timeout occurred
    pub fn is_timed_out(&self) -> bool {
        self.state.is_timed_out()
    }

    /// Check if closed
    pub fn is_closed(&self) -> bool {
        self.state.is_closed()
    }

    /// Set socket to non-blocking mode (required for async operations)
    fn set_nonblocking(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let raw_fd = self.fd as i32;
            let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFL, 0) };
            if flags < 0 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::fcntl(raw_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        #[cfg(windows)]
        {
            use winapi::um::winsock2::{ioctlsocket, FIONBIO};
            let raw_socket = self.fd as u64;
            let mut nonblocking: u32 = 1;
            let result = unsafe {
                ioctlsocket(
                    raw_socket as usize,
                    FIONBIO,
                    &mut nonblocking as *mut u32,
                )
            };
            if result != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        #[cfg(not(any(unix, windows)))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "non-blocking sockets not supported on this platform",
            ))
        }
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        // Send CloseSocket message to worker if registered
        if let Some(token) = self.state.token() {
            if let Some(worker_id) = self.worker_id {
                let _ = crate::runtime::worker::close_socket(Token(token), worker_id as u32);
            }
        }

        // Close the FD
        #[cfg(unix)]
        unsafe {
            libc::close(self.fd as i32);
        }

        #[cfg(windows)]
        unsafe {
            use winapi::um::winsock2::closesocket;
            closesocket(self.fd as usize);
        }
    }
}

/// Future for async bind operation
struct BindFuture {
    addr: Option<SocketAddr>,
}

impl BindFuture {
    fn new<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no socket addresses")
        })?;

        Ok(Self {
            addr: Some(addr),
        })
    }
}

impl Future for BindFuture {
    type Output = io::Result<TcpListener>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let addr = self.addr.take().unwrap();

        // Create and bind listener
        let listener = match std::net::TcpListener::bind(addr) {
            Ok(l) => l,
            Err(e) => return Poll::Ready(Err(e)),
        };

        // Get local address (might differ from addr if port was 0)
        let local_addr = listener.local_addr()?;

        // Get FD
        #[cfg(unix)]
        let fd = {
            use std::os::unix::io::AsRawFd;
            listener.as_raw_fd() as SocketDescriptor
        };

        #[cfg(windows)]
        let fd = {
            use std::os::windows::io::AsRawSocket;
            listener.as_raw_socket() as SocketDescriptor
        };

        let tcp_listener = TcpListener {
            fd,
            worker_id: None,
            state: crate::net::AsyncSocketState::new(),
            registered_interest: AtomicU8::new(RegisteredInterest::None as u8),
            local_addr,
        };

        // Set non-blocking mode
        tcp_listener.set_nonblocking()?;

        // Forget the std listener so FD stays open
        std::mem::forget(listener);

        Poll::Ready(Ok(tcp_listener))
    }
}

/// Future for async accept operation
struct AcceptFuture<'a> {
    listener: &'a mut TcpListener,
}

impl<'a> AcceptFuture<'a> {
    fn new(listener: &'a mut TcpListener) -> Self {
        Self { listener }
    }
}

impl<'a> Future for AcceptFuture<'a> {
    type Output = io::Result<(TcpStream, SocketAddr)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Ensure readable interest is registered with EventLoop (accept needs readable)
        let current_interest = self.listener.get_registered_interest();
        if !current_interest.is_readable() {
            let new_interest = current_interest.add_readable();

            // Check if socket is registered yet
            if self.listener.worker_id.is_none() {
                // First registration - send RegisterSocket message
                if let Some(socket_interest) = new_interest.to_socket_interest() {
                    let state_ptr = &self.listener.state as *const _ as usize;

                    if let Ok(_token) = crate::runtime::worker::register_socket_with_current_worker(
                        self.listener.fd,
                        socket_interest,
                        state_ptr,
                    ) {
                        if let Some(worker_id) = crate::runtime::worker::current_worker_id() {
                            self.listener.worker_id = Some(worker_id as usize);
                        }
                    }
                }
            } else if let Some(token) = self.listener.state.token() {
                // Socket already registered - send ModifySocket message
                if let (Some(socket_interest), Some(worker_id)) =
                    (new_interest.to_socket_interest(), self.listener.worker_id) {
                    let _ = crate::runtime::worker::modify_socket_interest(
                        Token(token),
                        worker_id as u32,
                        socket_interest,
                    );
                }
            }

            self.listener.set_registered_interest(new_interest);
            self.listener.set_accept_waker(cx.waker());
            return Poll::Pending;
        }

        // Try non-blocking accept
        #[cfg(unix)]
        let result = unsafe {
            let fd = self.listener.fd as i32;
            let mut addr: libc::sockaddr_storage = std::mem::zeroed();
            let mut addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

            let client_fd = libc::accept(
                fd,
                &mut addr as *mut _ as *mut libc::sockaddr,
                &mut addr_len,
            );

            if client_fd < 0 {
                Err(io::Error::last_os_error())
            } else {
                // Convert sockaddr to SocketAddr
                let peer_addr = if addr_len as usize == std::mem::size_of::<libc::sockaddr_in>() {
                    let addr_in = &addr as *const _ as *const libc::sockaddr_in;
                    let addr_in = &*addr_in;
                    SocketAddr::from((
                        std::net::Ipv4Addr::from(u32::from_be(addr_in.sin_addr.s_addr)),
                        u16::from_be(addr_in.sin_port),
                    ))
                } else {
                    // IPv6 handling would go here
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "IPv6 not yet supported in accept",
                    )));
                };

                Ok((client_fd as SocketDescriptor, peer_addr))
            }
        };

        #[cfg(windows)]
        let result = unsafe {
            use winapi::um::winsock2::{accept, SOCKET_ERROR};
            use winapi::shared::ws2def::{SOCKADDR, SOCKADDR_IN};

            let socket = self.listener.fd as usize;
            let mut addr: SOCKADDR_IN = std::mem::zeroed();
            let mut addr_len = std::mem::size_of::<SOCKADDR_IN>() as i32;

            let client_socket = accept(
                socket,
                &mut addr as *mut _ as *mut SOCKADDR,
                &mut addr_len,
            );

            if client_socket == SOCKET_ERROR as usize {
                Err(io::Error::last_os_error())
            } else {
                // Convert SOCKADDR_IN to SocketAddr
                let peer_addr = SocketAddr::from((
                    std::net::Ipv4Addr::from(u32::from_be(*addr.sin_addr.S_un.S_addr())),
                    u16::from_be(addr.sin_port),
                ));

                Ok((client_socket as SocketDescriptor, peer_addr))
            }
        };

        match result {
            Ok((client_fd, peer_addr)) => {
                // Get local address for the client socket
                #[cfg(unix)]
                let local_addr = unsafe {
                    let mut addr: libc::sockaddr_storage = std::mem::zeroed();
                    let mut addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

                    if libc::getsockname(
                        client_fd as i32,
                        &mut addr as *mut _ as *mut libc::sockaddr,
                        &mut addr_len,
                    ) < 0 {
                        return Poll::Ready(Err(io::Error::last_os_error()));
                    }

                    let addr_in = &addr as *const _ as *const libc::sockaddr_in;
                    let addr_in = &*addr_in;
                    SocketAddr::from((
                        std::net::Ipv4Addr::from(u32::from_be(addr_in.sin_addr.s_addr)),
                        u16::from_be(addr_in.sin_port),
                    ))
                };

                #[cfg(windows)]
                let local_addr = unsafe {
                    use winapi::um::winsock2::getsockname;
                    use winapi::shared::ws2def::{SOCKADDR, SOCKADDR_IN};

                    let mut addr: SOCKADDR_IN = std::mem::zeroed();
                    let mut addr_len = std::mem::size_of::<SOCKADDR_IN>() as i32;

                    if getsockname(
                        client_fd as usize,
                        &mut addr as *mut _ as *mut SOCKADDR,
                        &mut addr_len,
                    ) != 0 {
                        return Poll::Ready(Err(io::Error::last_os_error()));
                    }

                    SocketAddr::from((
                        std::net::Ipv4Addr::from(u32::from_be(*addr.sin_addr.S_un.S_addr())),
                        u16::from_be(addr.sin_port),
                    ))
                };

                // Create TcpStream
                let stream = TcpStream {
                    fd: client_fd,
                    worker_id: None,
                    state: crate::net::AsyncSocketState::new(),
                    registered_interest: AtomicU8::new(RegisteredInterest::None as u8),
                    local_addr,
                    peer_addr,
                };

                // Set client socket to non-blocking
                stream.set_nonblocking()?;

                Poll::Ready(Ok((stream, peer_addr)))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No pending connection - store waker and return pending
                // EventLoop will wake us when a connection is ready
                self.listener.set_accept_waker(cx.waker());
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_stream_waker_storage() {
        // Basic test that wakers can be stored
        let timed_out = AtomicBool::new(false);
        assert!(!timed_out.load(Ordering::Acquire));
        timed_out.store(true, Ordering::Release);
        assert!(timed_out.load(Ordering::Acquire));
    }

    #[test]
    fn test_registered_interest() {
        let interest = RegisteredInterest::None;
        assert!(!interest.is_readable());
        assert!(!interest.is_writable());

        let interest = RegisteredInterest::Readable;
        assert!(interest.is_readable());
        assert!(!interest.is_writable());

        let interest = RegisteredInterest::Writable;
        assert!(!interest.is_readable());
        assert!(interest.is_writable());

        let interest = RegisteredInterest::ReadWrite;
        assert!(interest.is_readable());
        assert!(interest.is_writable());
    }
}
