use std::{
    cell::UnsafeCell,
    future::Future,
    io,
    net::{SocketAddr, ToSocketAddrs},
    task::ready,
    time::Duration,
};

#[cfg(unix)]
use {
    libc::{AF_INET, AF_INET6, SOCK_STREAM},
    std::os::unix::prelude::{AsRawFd, FromRawFd, IntoRawFd, RawFd},
};
#[cfg(windows)]
use {
    std::os::windows::prelude::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket},
    windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, SOCK_STREAM},
};

use crate::{
    BufResult,
    buf::{IoBuf, IoBufMut, IoVecBuf, IoVecBufMut},
    driver::{op::Op, shared_fd::SharedFd},
    io::{
        AsyncReadRent, AsyncWriteRent, CancelHandle, CancelableAsyncReadRent,
        CancelableAsyncWriteRent, Split,
        as_fd::{AsReadFd, AsWriteFd, SharedFdWrapper},
        operation_canceled,
    },
};

/// Custom tcp connect options
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct TcpConnectOpts {
    /// TCP fast open.
    pub tcp_fast_open: bool,
}

impl Default for TcpConnectOpts {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl TcpConnectOpts {
    /// Create a default TcpConnectOpts.
    #[inline]
    pub const fn new() -> Self {
        Self {
            tcp_fast_open: false,
        }
    }

    /// Specify FastOpen
    /// Note: This option only works for linux 4.1+
    /// and macos/ios 9.0+.
    /// If it is enabled, the connection will be
    /// established on the first call to write.
    #[must_use]
    #[inline]
    pub fn tcp_fast_open(mut self, fast_open: bool) -> Self {
        self.tcp_fast_open = fast_open;
        self
    }
}
/// TcpStream
pub struct TcpStream {
    pub(crate) fd: SharedFd,
    meta: StreamMeta,
}

unsafe impl Send for TcpStream {}
unsafe impl Sync for TcpStream {}

/// TcpStream is safe to split to two parts
unsafe impl Split for TcpStream {}

impl TcpStream {
    pub(crate) fn from_shared_fd(fd: SharedFd) -> Self {
        #[cfg(unix)]
        let meta = StreamMeta::new(fd.raw_fd());
        #[cfg(windows)]
        let meta = StreamMeta::new(fd.raw_socket());
        #[cfg(feature = "zero-copy")]
        // enable SOCK_ZEROCOPY
        meta.set_zero_copy();

        Self { fd, meta }
    }

    /// Open a TCP connection to a remote host.
    /// Note: This function may block the current thread while resolution is
    /// performed.
    // TODO(chihai): Fix it, maybe spawn_blocking like tokio.
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        // TODO(chihai): loop for all addrs
        let addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::other("empty address"))?;

        Self::connect_addr(addr).await
    }

    /// Establish a connection to the specified `addr`.
    pub async fn connect_addr(addr: SocketAddr) -> io::Result<Self> {
        const DEFAULT_OPTS: TcpConnectOpts = TcpConnectOpts {
            tcp_fast_open: false,
        };
        Self::connect_addr_with_config(addr, &DEFAULT_OPTS).await
    }

    /// Establish a connection to the specified `addr` with given config.
    pub async fn connect_addr_with_config(
        addr: SocketAddr,
        opts: &TcpConnectOpts,
    ) -> io::Result<Self> {
        let domain = match addr {
            SocketAddr::V4(_) => AF_INET,
            SocketAddr::V6(_) => AF_INET6,
        };
        let socket = crate::net::new_socket(domain, SOCK_STREAM)?;
        #[allow(unused_mut)]
        let mut tfo = opts.tcp_fast_open;

        if tfo {
            #[cfg(any(target_os = "linux", target_os = "android"))]
            super::tfo::try_set_tcp_fastopen_connect(&socket);
            #[cfg(any(target_os = "ios", target_os = "macos"))]
            // if we cannot set force tcp fastopen, we will not use it.
            if super::tfo::set_tcp_fastopen_force_enable(&socket).is_err() {
                tfo = false;
            }
        }
        let completion = Op::connect(SharedFd::new::<false>(socket)?, addr, tfo)?.await;
        completion.meta.result?;

        let stream = TcpStream::from_shared_fd(completion.data.fd);
        // wait write ready on epoll branch
        if crate::driver::op::is_legacy() {
            #[cfg(all(any(target_os = "ios", target_os = "macos"), feature = "poll"))]
            if !tfo {
                stream.writable(true).await?;
            } else {
                // set writable as init state
                crate::driver::CURRENT.with(|inner| match inner {
                    crate::driver::Inner::Poller(inner) => {
                        let idx = stream.fd.registered_index().unwrap();
                        if let Some(mut readiness) =
                            unsafe { &mut *inner.get() }.io_dispatch.get(idx)
                        {
                            readiness.set_writable();
                        }
                    }
                    #[allow(unreachable_patterns)]
                    _ => unreachable!("should never happens"),
                })
            }
            #[cfg(not(any(target_os = "ios", target_os = "macos")))]
            stream.writable(true).await?;

            // getsockopt libc::SO_ERROR
            #[cfg(unix)]
            let sys_socket = unsafe { std::net::TcpStream::from_raw_fd(stream.fd.raw_fd()) };
            #[cfg(windows)]
            let sys_socket =
                unsafe { std::net::TcpStream::from_raw_socket(stream.fd.raw_socket()) };
            let err = sys_socket.take_error();
            #[cfg(unix)]
            let _ = sys_socket.into_raw_fd();
            #[cfg(windows)]
            let _ = sys_socket.into_raw_socket();
            if let Some(e) = err? {
                return Err(e);
            }
        }
        Ok(stream)
    }

    /// Return the local address that this stream is bound to.
    #[inline]
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.meta.local_addr()
    }

    /// Return the remote address that this stream is connected to.
    #[inline]
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.meta.peer_addr()
    }

    /// Get the value of the `TCP_NODELAY` option on this socket.
    #[inline]
    pub fn nodelay(&self) -> io::Result<bool> {
        self.meta.no_delay()
    }

    /// Set the value of the `TCP_NODELAY` option on this socket.
    #[inline]
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.meta.set_no_delay(nodelay)
    }

    /// Set the value of the `SO_KEEPALIVE` option on this socket.
    #[inline]
    pub fn set_tcp_keepalive(
        &self,
        time: Option<Duration>,
        interval: Option<Duration>,
        retries: Option<u32>,
    ) -> io::Result<()> {
        self.meta.set_tcp_keepalive(time, interval, retries)
    }

    /// Creates new `TcpStream` from a `std::net::TcpStream`.
    pub fn from_std(stream: std::net::TcpStream) -> io::Result<Self> {
        #[cfg(unix)]
        let fd = stream.as_raw_fd();
        #[cfg(windows)]
        let fd = stream.as_raw_socket();
        match SharedFd::new::<false>(fd) {
            Ok(shared) => {
                #[cfg(unix)]
                let _ = stream.into_raw_fd();
                #[cfg(windows)]
                let _ = stream.into_raw_socket();
                Ok(Self::from_shared_fd(shared))
            }
            Err(e) => Err(e),
        }
    }

    /// Wait for read readiness.
    /// Note: Do not use it before every io. It is different from other runtimes!
    ///
    /// Everytime call to this method may pay a syscall cost.
    /// In uring impl, it will push a PollAdd op; in epoll impl, it will use use
    /// inner readiness state; if !relaxed, it will call syscall poll after that.
    ///
    /// If relaxed, on poller driver it may return false positive result.
    /// If you want to do io by your own, you must maintain io readiness and wait
    /// for io ready with relaxed=false.
    pub async fn readable(&self, relaxed: bool) -> io::Result<()> {
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                return RemotePollReadOp::new(self.fd.clone(), relaxed).await;
            }
        }
        let op = Op::poll_read(&self.fd, relaxed).unwrap();
        op.wait().await
    }

    /// Wait for write readiness.
    /// Note: Do not use it before every io. It is different from other runtimes!
    ///
    /// Everytime call to this method may pay a syscall cost.
    /// In uring impl, it will push a PollAdd op; in epoll impl, it will use use
    /// inner readiness state; if !relaxed, it will call syscall poll after that.
    ///
    /// If relaxed, on poller driver it may return false positive result.
    /// If you want to do io by your own, you must maintain io readiness and wait
    /// for io ready with relaxed=false.
    pub async fn writable(&self, relaxed: bool) -> io::Result<()> {
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                return RemotePollWriteOp::new(self.fd.clone(), relaxed).await;
            }
        }
        let op = Op::poll_write(&self.fd, relaxed).unwrap();
        op.wait().await
    }
}

impl AsReadFd for TcpStream {
    #[inline]
    fn as_reader_fd(&mut self) -> &SharedFdWrapper {
        SharedFdWrapper::new(&self.fd)
    }
}

impl AsWriteFd for TcpStream {
    #[inline]
    fn as_writer_fd(&mut self) -> &SharedFdWrapper {
        SharedFdWrapper::new(&self.fd)
    }
}

#[cfg(unix)]
impl IntoRawFd for TcpStream {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.fd
            .try_unwrap()
            .expect("unexpected multiple reference to rawfd")
    }
}
#[cfg(unix)]
impl AsRawFd for TcpStream {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}

#[cfg(windows)]
impl IntoRawSocket for TcpStream {
    #[inline]
    fn into_raw_socket(self) -> RawSocket {
        self.fd
            .try_unwrap()
            .expect("unexpected multiple reference to rawfd")
    }
}

#[cfg(windows)]
impl AsRawSocket for TcpStream {
    #[inline]
    fn as_raw_socket(&self) -> RawSocket {
        self.fd.raw_socket()
    }
}

impl std::fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpStream").field("fd", &self.fd).finish()
    }
}

impl AsyncWriteRent for TcpStream {
    #[inline]
    fn write<T: IoBuf>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                return futures::future::Either::Left(RemoteWriteOp::new(self.fd.clone(), buf));
            }
        }
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        return futures::future::Either::Right(Op::send(self.fd.clone(), buf).unwrap().result());
        #[cfg(all(target_os = "linux", feature = "iouring"))]
        Op::send(self.fd.clone(), buf).unwrap().result()
    }

    #[inline]
    fn writev<T: IoVecBuf>(&mut self, buf_vec: T) -> impl Future<Output = BufResult<usize, T>> {
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                return futures::future::Either::Left(RemoteWritevOp::new(
                    self.fd.clone(),
                    buf_vec,
                ));
            }
        }
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        return futures::future::Either::Right(
            Op::writev(self.fd.clone(), buf_vec).unwrap().result(),
        );
        #[cfg(all(target_os = "linux", feature = "iouring"))]
        Op::writev(self.fd.clone(), buf_vec).unwrap().result()
    }

    #[inline]
    fn flush(&mut self) -> impl Future<Output = std::io::Result<()>> {
        // Tcp stream does not need flush.
        std::future::ready(Ok(()))
    }

    fn shutdown(&mut self) -> impl Future<Output = std::io::Result<()>> {
        // We could use shutdown op here, which requires kernel 5.11+.
        // However, for simplicity, we just close the socket using direct syscall.
        let res = {
            #[cfg(unix)]
            let stream = unsafe { std::net::TcpStream::from_raw_fd(self.as_raw_fd()) };
            #[cfg(windows)]
            let stream = unsafe { std::net::TcpStream::from_raw_socket(self.as_raw_socket()) };
            let ret = stream.shutdown(std::net::Shutdown::Write);
            std::mem::forget(stream);
            ret
        };
        std::future::ready(res)
    }
}

impl CancelableAsyncWriteRent for TcpStream {
    #[inline]
    async fn cancelable_write<T: IoBuf>(
        &mut self,
        buf: T,
        c: CancelHandle,
    ) -> crate::BufResult<usize, T> {
        if c.canceled() {
            return (Err(operation_canceled()), buf);
        }

        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                // Remote operations now support cancellation!
                return RemoteWriteOp::new_cancelable(self.fd.clone(), buf, c).await;
            }
        }

        let fd = self.fd.clone();
        let op = Op::send(fd, buf).unwrap();
        let _guard = c.associate_op(op.op_canceller());
        op.result().await
    }

    #[inline]
    async fn cancelable_writev<T: IoVecBuf>(
        &mut self,
        buf_vec: T,
        c: CancelHandle,
    ) -> crate::BufResult<usize, T> {
        if c.canceled() {
            return (Err(operation_canceled()), buf_vec);
        }

        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                // Remote operations now support cancellation!
                return RemoteWritevOp::new_cancelable(self.fd.clone(), buf_vec, c).await;
            }
        }

        let fd = self.fd.clone();
        let op = Op::writev(fd.clone(), buf_vec).unwrap();
        let _guard = c.associate_op(op.op_canceller());
        op.result().await
    }

    #[inline]
    async fn cancelable_flush(&mut self, _c: CancelHandle) -> io::Result<()> {
        // Tcp stream does not need flush.
        Ok(())
    }

    fn cancelable_shutdown(&mut self, _c: CancelHandle) -> impl Future<Output = io::Result<()>> {
        // We could use shutdown op here, which requires kernel 5.11+.
        // However, for simplicity, we just close the socket using direct syscall.
        let res = {
            #[cfg(unix)]
            let stream = unsafe { std::net::TcpStream::from_raw_fd(self.as_raw_fd()) };
            #[cfg(windows)]
            let stream = unsafe { std::net::TcpStream::from_raw_socket(self.as_raw_socket()) };
            let ret = stream.shutdown(std::net::Shutdown::Write);
            std::mem::forget(stream);
            ret
        };
        std::future::ready(res)
    }
}

impl AsyncReadRent for TcpStream {
    #[inline]
    fn read<T: IoBufMut>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                return futures::future::Either::Left(RemoteReadOp::new(self.fd.clone(), buf));
            }
        }
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        return futures::future::Either::Right(Op::recv(self.fd.clone(), buf).unwrap().result());
        #[cfg(all(target_os = "linux", feature = "iouring"))]
        Op::recv(self.fd.clone(), buf).unwrap().result()
    }

    #[inline]
    fn readv<T: IoVecBufMut>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                return futures::future::Either::Left(RemoteReadvOp::new(self.fd.clone(), buf));
            }
        }
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        return futures::future::Either::Right(Op::readv(self.fd.clone(), buf).unwrap().result());
        #[cfg(all(target_os = "linux", feature = "iouring"))]
        Op::readv(self.fd.clone(), buf).unwrap().result()
    }
}

#[cfg(feature = "poll")]
struct RemoteReadOp<T> {
    fd: SharedFd,
    buf: Option<T>,
    cancel: Option<CancelHandle>,
}

impl<T> RemoteReadOp<T> {
    fn new(fd: SharedFd, buf: T) -> Self {
        Self {
            fd,
            buf: Some(buf),
            cancel: None,
        }
    }

    fn new_cancelable(fd: SharedFd, buf: T, cancel: CancelHandle) -> Self {
        Self {
            fd,
            buf: Some(buf),
            cancel: Some(cancel),
        }
    }
}

#[cfg(feature = "poll")]
impl<T: IoBufMut> Future for RemoteReadOp<T> {
    type Output = BufResult<usize, T>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // Check for cancellation first
        if let Some(ref cancel) = self.cancel {
            if cancel.canceled() {
                return std::task::Poll::Ready((
                    Err(operation_canceled()),
                    self.buf.take().unwrap(),
                ));
            }
        }

        // Register waker FIRST to avoid lost wakeup race
        // If data arrives between registration and syscall, we'll just get it on the syscall
        unsafe {
            self.fd.reader_waker().register(cx.waker());
        }

        let mut stream = unsafe {
            #[cfg(unix)]
            {
                std::net::TcpStream::from_raw_fd(self.fd.raw_fd())
            }
            #[cfg(windows)]
            {
                std::net::TcpStream::from_raw_socket(self.fd.raw_socket())
            }
        };

        let buf = self.buf.as_mut().unwrap();
        let ptr = buf.write_ptr();
        let len = buf.bytes_total();
        let slice = unsafe { std::slice::from_raw_parts_mut(ptr, len) };

        let res = std::io::Read::read(&mut stream, slice);
        std::mem::forget(stream);

        match res {
            Ok(n) => {
                unsafe { buf.set_init(n) };
                std::task::Poll::Ready((Ok(n), self.buf.take().unwrap()))
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => std::task::Poll::Pending,
            Err(e) => std::task::Poll::Ready((Err(e), self.buf.take().unwrap())),
        }
    }
}

#[cfg(feature = "poll")]
struct RemoteReadvOp<T> {
    fd: SharedFd,
    buf: Option<T>,
    cancel: Option<CancelHandle>,
}

impl<T> RemoteReadvOp<T> {
    fn new(fd: SharedFd, buf: T) -> Self {
        Self {
            fd,
            buf: Some(buf),
            cancel: None,
        }
    }

    fn new_cancelable(fd: SharedFd, buf: T, cancel: CancelHandle) -> Self {
        Self {
            fd,
            buf: Some(buf),
            cancel: Some(cancel),
        }
    }
}

#[cfg(feature = "poll")]
impl<T: IoVecBufMut> Future for RemoteReadvOp<T> {
    type Output = BufResult<usize, T>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // Check for cancellation first
        if let Some(ref cancel) = self.cancel {
            if cancel.canceled() {
                return std::task::Poll::Ready((
                    Err(operation_canceled()),
                    self.buf.take().unwrap(),
                ));
            }
        }

        // Register waker FIRST to avoid lost wakeup race
        unsafe {
            self.fd.reader_waker().register(cx.waker());
        }

        let mut stream = unsafe {
            #[cfg(unix)]
            {
                std::net::TcpStream::from_raw_fd(self.fd.raw_fd())
            }
            #[cfg(windows)]
            {
                std::net::TcpStream::from_raw_socket(self.fd.raw_socket())
            }
        };

        #[cfg(unix)]
        let res = {
            let buf = self.buf.as_mut().unwrap();
            let iovec_ptr = buf.write_iovec_ptr();
            let iovec_len = buf.write_iovec_len();
            let slices = unsafe {
                std::slice::from_raw_parts_mut(iovec_ptr as *mut std::io::IoSliceMut, iovec_len)
            };
            std::io::Read::read_vectored(&mut stream, slices)
        };

        #[cfg(windows)]
        let res = {
            let buf = self.buf.as_mut().unwrap();
            let wsabuf_ptr = buf.write_wsabuf_ptr();
            let wsabuf_len = buf.write_wsabuf_len();
            let slices = unsafe {
                std::slice::from_raw_parts_mut(wsabuf_ptr as *mut std::io::IoSliceMut, wsabuf_len)
            };
            std::io::Read::read_vectored(&mut stream, slices)
        };

        std::mem::forget(stream);

        match res {
            Ok(n) => {
                unsafe { self.buf.as_mut().unwrap().set_init(n) };
                std::task::Poll::Ready((Ok(n), self.buf.take().unwrap()))
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => std::task::Poll::Pending,
            Err(e) => std::task::Poll::Ready((Err(e), self.buf.take().unwrap())),
        }
    }
}

#[cfg(feature = "poll")]
struct RemoteWriteOp<T> {
    fd: SharedFd,
    buf: Option<T>,
    cancel: Option<CancelHandle>,
}

impl<T> RemoteWriteOp<T> {
    fn new(fd: SharedFd, buf: T) -> Self {
        Self {
            fd,
            buf: Some(buf),
            cancel: None,
        }
    }

    fn new_cancelable(fd: SharedFd, buf: T, cancel: CancelHandle) -> Self {
        Self {
            fd,
            buf: Some(buf),
            cancel: Some(cancel),
        }
    }
}

#[cfg(feature = "poll")]
impl<T: IoBuf> Future for RemoteWriteOp<T> {
    type Output = BufResult<usize, T>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // Check for cancellation first
        if let Some(ref cancel) = self.cancel {
            if cancel.canceled() {
                return std::task::Poll::Ready((
                    Err(operation_canceled()),
                    self.buf.take().unwrap(),
                ));
            }
        }

        // Register waker FIRST to avoid lost wakeup race
        unsafe {
            self.fd.writer_waker().register(cx.waker());
        }

        let mut stream = unsafe {
            #[cfg(unix)]
            {
                std::net::TcpStream::from_raw_fd(self.fd.raw_fd())
            }
            #[cfg(windows)]
            {
                std::net::TcpStream::from_raw_socket(self.fd.raw_socket())
            }
        };

        let buf = self.buf.as_ref().unwrap();
        let ptr = buf.read_ptr();
        let len = buf.bytes_init();
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };

        let res = std::io::Write::write(&mut stream, slice);
        std::mem::forget(stream);

        match res {
            Ok(n) => std::task::Poll::Ready((Ok(n), self.buf.take().unwrap())),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Register waker directly via SharedFd
                unsafe {
                    self.fd.writer_waker().register(cx.waker());
                }
                std::task::Poll::Pending
            }
            Err(e) => std::task::Poll::Ready((Err(e), self.buf.take().unwrap())),
        }
    }
}

#[cfg(feature = "poll")]
struct RemoteWritevOp<T> {
    fd: SharedFd,
    buf: Option<T>,
    cancel: Option<CancelHandle>,
}

impl<T> RemoteWritevOp<T> {
    fn new(fd: SharedFd, buf: T) -> Self {
        Self {
            fd,
            buf: Some(buf),
            cancel: None,
        }
    }

    fn new_cancelable(fd: SharedFd, buf: T, cancel: CancelHandle) -> Self {
        Self {
            fd,
            buf: Some(buf),
            cancel: Some(cancel),
        }
    }
}

#[cfg(feature = "poll")]
impl<T: IoVecBuf> Future for RemoteWritevOp<T> {
    type Output = BufResult<usize, T>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // Check for cancellation first
        if let Some(ref cancel) = self.cancel {
            if cancel.canceled() {
                return std::task::Poll::Ready((
                    Err(operation_canceled()),
                    self.buf.take().unwrap(),
                ));
            }
        }

        // Register waker FIRST to avoid lost wakeup race
        unsafe {
            self.fd.writer_waker().register(cx.waker());
        }

        let mut stream = unsafe {
            #[cfg(unix)]
            {
                std::net::TcpStream::from_raw_fd(self.fd.raw_fd())
            }
            #[cfg(windows)]
            {
                std::net::TcpStream::from_raw_socket(self.fd.raw_socket())
            }
        };

        #[cfg(unix)]
        let res = {
            let buf = self.buf.as_ref().unwrap();
            let iovec_ptr = buf.read_iovec_ptr();
            let iovec_len = buf.read_iovec_len();
            let slices = unsafe {
                std::slice::from_raw_parts(iovec_ptr as *const std::io::IoSlice, iovec_len)
            };
            std::io::Write::write_vectored(&mut stream, slices)
        };

        #[cfg(windows)]
        let res = {
            let buf = self.buf.as_ref().unwrap();
            let wsabuf_ptr = buf.read_wsabuf_ptr();
            let wsabuf_len = buf.read_wsabuf_len();
            let slices = unsafe {
                std::slice::from_raw_parts(wsabuf_ptr as *const std::io::IoSlice, wsabuf_len)
            };
            std::io::Write::write_vectored(&mut stream, slices)
        };

        std::mem::forget(stream);

        match res {
            Ok(n) => std::task::Poll::Ready((Ok(n), self.buf.take().unwrap())),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                unsafe {
                    self.fd.writer_waker().register(cx.waker());
                }
                std::task::Poll::Pending
            }
            Err(e) => std::task::Poll::Ready((Err(e), self.buf.take().unwrap())),
        }
    }
}

#[cfg(feature = "poll")]
struct RemotePollReadOp {
    fd: SharedFd,
    relaxed: bool,
}

#[cfg(feature = "poll")]
impl RemotePollReadOp {
    fn new(fd: SharedFd, relaxed: bool) -> Self {
        Self { fd, relaxed }
    }
}

#[cfg(feature = "poll")]
impl Future for RemotePollReadOp {
    type Output = io::Result<()>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // For remote poll operations, we just register the waker and return ready immediately
        // The remote FD is not registered with our local poller, so we can't check actual readiness
        // Users should handle WouldBlock errors themselves
        unsafe {
            self.fd.reader_waker().register(cx.waker());
        }
        std::task::Poll::Ready(Ok(()))
    }
}

#[cfg(feature = "poll")]
struct RemotePollWriteOp {
    fd: SharedFd,
    relaxed: bool,
}

#[cfg(feature = "poll")]
impl RemotePollWriteOp {
    fn new(fd: SharedFd, relaxed: bool) -> Self {
        Self { fd, relaxed }
    }
}

#[cfg(feature = "poll")]
impl Future for RemotePollWriteOp {
    type Output = io::Result<()>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // For remote poll operations, we just register the waker and return ready immediately
        // The remote FD is not registered with our local poller, so we can't check actual readiness
        // Users should handle WouldBlock errors themselves
        unsafe {
            self.fd.writer_waker().register(cx.waker());
        }
        std::task::Poll::Ready(Ok(()))
    }
}

impl CancelableAsyncReadRent for TcpStream {
    #[inline]
    async fn cancelable_read<T: IoBufMut>(
        &mut self,
        buf: T,
        c: CancelHandle,
    ) -> crate::BufResult<usize, T> {
        if c.canceled() {
            return (Err(operation_canceled()), buf);
        }

        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                // Remote operations now support cancellation!
                return RemoteReadOp::new_cancelable(self.fd.clone(), buf, c).await;
            }
        }

        let fd = self.fd.clone();
        let op = Op::recv(fd, buf).unwrap();
        let _guard = c.associate_op(op.op_canceller());
        op.result().await
    }

    #[inline]
    async fn cancelable_readv<T: IoVecBufMut>(
        &mut self,
        buf: T,
        c: CancelHandle,
    ) -> crate::BufResult<usize, T> {
        if c.canceled() {
            return (Err(operation_canceled()), buf);
        }

        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            if self.fd.is_remote() {
                // Remote operations now support cancellation!
                return RemoteReadvOp::new_cancelable(self.fd.clone(), buf, c).await;
            }
        }

        let fd = self.fd.clone();
        let op = Op::readv(fd, buf).unwrap();
        let _guard = c.associate_op(op.op_canceller());
        op.result().await
    }
}

#[cfg(all(feature = "poll", feature = "tokio-compat"))]
impl tokio::io::AsyncRead for TcpStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            let is_remote = self.fd.is_remote();
            if is_remote {
                // Register waker FIRST to avoid lost wakeup race
                unsafe {
                    self.fd.reader_waker().register(cx.waker());
                }

                let mut stream = unsafe {
                    #[cfg(unix)]
                    {
                        std::net::TcpStream::from_raw_fd(self.fd.raw_fd())
                    }
                    #[cfg(windows)]
                    {
                        std::net::TcpStream::from_raw_socket(self.fd.raw_socket())
                    }
                };

                // Remote read: use direct syscall
                let slice = unsafe { buf.unfilled_mut() };
                // SAFETY: MaybeUninit<u8> has the same layout as u8, and we're writing to the buffer
                let slice = unsafe {
                    std::slice::from_raw_parts_mut(slice.as_mut_ptr() as *mut u8, slice.len())
                };

                let res = std::io::Read::read(&mut stream, slice);
                std::mem::forget(stream);

                return match res {
                    Ok(n) => {
                        unsafe {
                            buf.assume_init(n);
                            buf.advance(n);
                        }
                        std::task::Poll::Ready(Ok(()))
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::task::Poll::Pending
                    }
                    Err(e) => std::task::Poll::Ready(Err(e)),
                };
            }
        }
        // Local read: use standard Op (or iouring path on Linux with iouring)
        unsafe {
            let slice = buf.unfilled_mut();
            let raw_buf = crate::buf::RawBuf::new(slice.as_ptr() as *const u8, slice.len());
            let mut recv = Op::recv_raw(&self.fd, raw_buf);
            let ret = ready!(crate::driver::op::PollLegacy::poll_legacy(&mut recv, cx));

            std::task::Poll::Ready(ret.result.map(|n: crate::driver::op::MaybeFd| {
                let n = n.into_inner();
                buf.assume_init(n as usize);
                buf.advance(n as usize);
            }))
        }
    }
}

#[cfg(all(feature = "poll", feature = "tokio-compat"))]
impl tokio::io::AsyncWrite for TcpStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, io::Error>> {
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            let is_remote = self.fd.is_remote();
            if is_remote {
                // Register waker FIRST to avoid lost wakeup race
                unsafe {
                    self.fd.writer_waker().register(cx.waker());
                }

                let mut stream = unsafe {
                    #[cfg(unix)]
                    {
                        std::net::TcpStream::from_raw_fd(self.fd.raw_fd())
                    }
                    #[cfg(windows)]
                    {
                        std::net::TcpStream::from_raw_socket(self.fd.raw_socket())
                    }
                };

                let res = std::io::Write::write(&mut stream, buf);
                std::mem::forget(stream);

                return match res {
                    Ok(n) => std::task::Poll::Ready(Ok(n)),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::task::Poll::Pending
                    }
                    Err(e) => std::task::Poll::Ready(Err(e)),
                };
            }
        }
        // Local write: use standard Op (or iouring path on Linux with iouring)
        unsafe {
            let raw_buf = crate::buf::RawBuf::new(buf.as_ptr(), buf.len());
            let mut send = Op::send_raw(&self.fd, raw_buf);
            let ret = ready!(crate::driver::op::PollLegacy::poll_legacy(&mut send, cx));

            std::task::Poll::Ready(
                ret.result
                    .map(|n: crate::driver::op::MaybeFd| n.into_inner() as usize),
            )
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), io::Error>> {
        let res = {
            #[cfg(unix)]
            let stream = unsafe { std::net::TcpStream::from_raw_fd(self.as_raw_fd()) };
            #[cfg(windows)]
            let stream = unsafe { std::net::TcpStream::from_raw_socket(self.as_raw_socket()) };
            let ret = stream.shutdown(std::net::Shutdown::Write);
            std::mem::forget(stream);
            ret
        };
        std::task::Poll::Ready(res)
    }

    fn poll_write_vectored(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<Result<usize, io::Error>> {
        #[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
        {
            let is_remote = self.fd.is_remote();
            if is_remote {
                // Register waker FIRST to avoid lost wakeup race
                unsafe {
                    self.fd.writer_waker().register(cx.waker());
                }

                let mut stream = unsafe {
                    #[cfg(unix)]
                    {
                        std::net::TcpStream::from_raw_fd(self.fd.raw_fd())
                    }
                    #[cfg(windows)]
                    {
                        std::net::TcpStream::from_raw_socket(self.fd.raw_socket())
                    }
                };

                let res = std::io::Write::write_vectored(&mut stream, bufs);
                std::mem::forget(stream);

                return match res {
                    Ok(n) => std::task::Poll::Ready(Ok(n)),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::task::Poll::Pending
                    }
                    Err(e) => std::task::Poll::Ready(Err(e)),
                };
            }
        }
        // Local writev: use standard Op (or iouring path on Linux with iouring)
        unsafe {
            let raw_buf = crate::buf::RawBufVectored::new(bufs.as_ptr() as _, bufs.len());
            let mut writev = Op::writev_raw(&self.fd, raw_buf);
            let ret = ready!(crate::driver::op::PollLegacy::poll_legacy(&mut writev, cx));

            std::task::Poll::Ready(
                ret.result
                    .map(|n: crate::driver::op::MaybeFd| n.into_inner() as usize),
            )
        }
    }

    fn is_write_vectored(&self) -> bool {
        true
    }
}

struct StreamMeta {
    socket: Option<socket2::Socket>,
    meta: UnsafeCell<Meta>,
}

unsafe impl Send for StreamMeta {}
unsafe impl Sync for StreamMeta {}

#[derive(Debug, Default, Clone)]
struct Meta {
    local_addr: Option<SocketAddr>,
    peer_addr: Option<SocketAddr>,
}

impl StreamMeta {
    #[cfg(unix)]
    fn new(fd: RawFd) -> Self {
        Self {
            socket: unsafe { Some(socket2::Socket::from_raw_fd(fd)) },
            meta: Default::default(),
        }
    }

    /// When operating files, we should use RawHandle;
    /// When operating sockets, we should use RawSocket;
    #[cfg(windows)]
    fn new(fd: RawSocket) -> Self {
        Self {
            socket: unsafe { Some(socket2::Socket::from_raw_socket(fd)) },
            meta: Default::default(),
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        let meta = unsafe { &mut *self.meta.get() };
        if let Some(addr) = meta.local_addr {
            return Ok(addr);
        }

        let ret = self
            .socket
            .as_ref()
            .unwrap()
            .local_addr()
            .map(|addr| addr.as_socket().expect("tcp socket is expected"));
        if let Ok(addr) = ret {
            meta.local_addr = Some(addr);
        }
        ret
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        let meta = unsafe { &mut *self.meta.get() };
        if let Some(addr) = meta.peer_addr {
            return Ok(addr);
        }

        let ret = self
            .socket
            .as_ref()
            .unwrap()
            .peer_addr()
            .map(|addr| addr.as_socket().expect("tcp socket is expected"));
        if let Ok(addr) = ret {
            meta.peer_addr = Some(addr);
        }
        ret
    }

    fn no_delay(&self) -> io::Result<bool> {
        self.socket.as_ref().unwrap().tcp_nodelay()
    }

    fn set_no_delay(&self, no_delay: bool) -> io::Result<()> {
        self.socket.as_ref().unwrap().set_tcp_nodelay(no_delay)
    }

    #[allow(unused_variables)]
    fn set_tcp_keepalive(
        &self,
        time: Option<Duration>,
        interval: Option<Duration>,
        retries: Option<u32>,
    ) -> io::Result<()> {
        let mut t = socket2::TcpKeepalive::new();
        if let Some(time) = time {
            t = t.with_time(time)
        }
        if let Some(interval) = interval {
            t = t.with_interval(interval)
        }
        #[cfg(unix)]
        if let Some(retries) = retries {
            t = t.with_retries(retries)
        }
        self.socket.as_ref().unwrap().set_tcp_keepalive(&t)
    }

    #[cfg(feature = "zero-copy")]
    fn set_zero_copy(&self) {
        #[cfg(target_os = "linux")]
        unsafe {
            let fd = self.socket.as_ref().unwrap().as_raw_fd();
            let v: libc::c_int = 1;
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_ZEROCOPY,
                &v as *const _ as *const _,
                std::mem::size_of::<libc::c_int>() as _,
            );
        }
    }
}

impl Drop for StreamMeta {
    fn drop(&mut self) {
        let socket = self.socket.take().unwrap();
        #[cfg(unix)]
        let _ = socket.into_raw_fd();
        #[cfg(windows)]
        let _ = socket.into_raw_socket();
    }
}
