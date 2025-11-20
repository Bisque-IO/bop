use std::future::Future;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Context, Poll};

#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;

use crate::net::{BsdAddr, RegisteredInterest, SocketDescriptor, Token};

/// Async UDP socket
pub struct UdpSocket {
    fd: SocketDescriptor,
    worker_id: Option<usize>,
    state: crate::net::AsyncSocketState,
    registered_interest: AtomicU8,
    local_addr: SocketAddr,
    peer_addr: Option<SocketAddr>,
}

impl UdpSocket {
    /// Bind to a local address
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        BindFuture::new(addr)?.await
    }

    /// Get local address
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    /// Get connected peer (if any)
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer_addr
    }

    /// Connect the socket to a remote peer (sets default send target)
    pub fn connect(&mut self, peer: SocketAddr) -> io::Result<()> {
        let bsd_addr = BsdAddr::from_socket_addr(&peer);
        crate::net::bsd_sockets::connect_socket(self.fd, &bsd_addr)?;
        self.peer_addr = Some(peer);
        Ok(())
    }

    /// Receive data from the socket
    pub async fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        RecvFromFuture::new(self, buf).await
    }

    /// Send data to a specific peer
    pub async fn send_to(&mut self, buf: &[u8], target: SocketAddr) -> io::Result<usize> {
        SendToFuture::new(self, buf, target).await
    }

    /// Send data to the connected peer (requires `connect`)
    pub async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        let peer = self.peer_addr.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "UDP socket is not connected")
        })?;
        self.send_to(buf, peer).await
    }

    fn get_registered_interest(&self) -> RegisteredInterest {
        RegisteredInterest::from_u8(self.registered_interest.load(Ordering::Acquire))
    }

    fn set_registered_interest(&self, interest: RegisteredInterest) {
        self.registered_interest
            .store(interest as u8, Ordering::Release);
    }

    fn set_read_waker(&self, waker: &std::task::Waker) {
        self.state.set_read_waker(waker);
    }

    fn set_write_waker(&self, waker: &std::task::Waker) {
        self.state.set_write_waker(waker);
    }

    fn set_nonblocking(&self) -> io::Result<()> {
        crate::net::bsd_sockets::set_nonblocking(self.fd, true)
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        if let Some(token) = self.state.token() {
            if let Some(worker_id) = self.worker_id {
                let _ = crate::runtime::worker::close_socket(Token(token), worker_id as u32);
            }
        }

        crate::net::bsd_sockets::close_socket(self.fd);
    }
}

struct BindFuture {
    addr: Option<SocketAddr>,
}

impl BindFuture {
    fn new<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let socket_addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no socket addresses"))?;

        Ok(Self {
            addr: Some(socket_addr),
        })
    }
}

impl Future for BindFuture {
    type Output = io::Result<UdpSocket>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let addr = this.addr.take().unwrap();
        let socket = std::net::UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        let local_addr = socket.local_addr()?;

        #[cfg(unix)]
        let fd = socket.as_raw_fd() as SocketDescriptor;

        #[cfg(windows)]
        let fd = socket.as_raw_socket() as SocketDescriptor;

        std::mem::forget(socket);

        Poll::Ready(Ok(UdpSocket {
            fd,
            worker_id: None,
            state: crate::net::AsyncSocketState::new(),
            registered_interest: AtomicU8::new(RegisteredInterest::None as u8),
            local_addr,
            peer_addr: None,
        }))
    }
}

struct RecvFromFuture<'a> {
    socket: &'a mut UdpSocket,
    buf: &'a mut [u8],
    addr: BsdAddr,
}

impl<'a> RecvFromFuture<'a> {
    fn new(socket: &'a mut UdpSocket, buf: &'a mut [u8]) -> Self {
        Self {
            socket,
            buf,
            addr: BsdAddr::new(),
        }
    }
}

impl<'a> Future for RecvFromFuture<'a> {
    type Output = io::Result<(usize, SocketAddr)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.socket.state.is_timed_out() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "socket read timeout",
            )));
        }

        if self.socket.state.is_closed() {
            return Poll::Ready(Ok((0, self.socket.local_addr)));
        }

        let current_interest = self.socket.get_registered_interest();
        if !current_interest.is_readable() {
            let new_interest = current_interest.add_readable();

            if self.socket.worker_id.is_none() {
                if let Some(socket_interest) = new_interest.to_socket_interest() {
                    let state_ptr = &self.socket.state as *const _ as usize;
                    if let Ok(_) = crate::runtime::worker::register_socket_with_current_worker(
                        self.socket.fd,
                        socket_interest,
                        state_ptr,
                    ) {
                        if let Some(worker_id) = crate::runtime::worker::current_worker_id() {
                            self.socket.worker_id = Some(worker_id as usize);
                        }
                    }
                }
            } else if let Some(token) = self.socket.state.token() {
                if let Some(socket_interest) = new_interest.to_socket_interest() {
                    if let Some(worker_id) = self.socket.worker_id {
                        let _ = crate::runtime::worker::modify_socket_interest(
                            Token(token),
                            worker_id as u32,
                            socket_interest,
                        );
                    }
                }
            }

            self.socket.set_registered_interest(new_interest);
            self.socket.set_read_waker(cx.waker());
            return Poll::Pending;
        }

        #[cfg(unix)]
        let result = unsafe {
            libc::recvfrom(
                self.socket.fd,
                self.buf.as_mut_ptr() as *mut libc::c_void,
                self.buf.len(),
                0,
                self.addr.as_mut_ptr(),
                self.addr.len_ptr(),
            )
        };
        #[cfg(windows)]
        let result = unsafe {
            winapi::um::winsock2::recvfrom(
                self.socket.fd,
                self.buf.as_mut_ptr() as *mut i8,
                self.buf.len() as i32,
                0,
                self.addr.as_mut_ptr() as *mut _,
                self.addr.len_ptr(),
            )
        };

        if result < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                self.socket.set_read_waker(cx.waker());
                return Poll::Pending;
            }
            return Poll::Ready(Err(err));
        }

        let size = result as usize;
        let peer = self
            .addr
            .to_socket_addr()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "invalid peer address"))?;

        Poll::Ready(Ok((size, peer)))
    }
}

struct SendToFuture<'a> {
    socket: &'a mut UdpSocket,
    buf: &'a [u8],
    addr: BsdAddr,
}

impl<'a> SendToFuture<'a> {
    fn new(socket: &'a mut UdpSocket, buf: &'a [u8], target: SocketAddr) -> Self {
        Self {
            socket,
            buf,
            addr: BsdAddr::from_socket_addr(&target),
        }
    }
}

impl<'a> Future for SendToFuture<'a> {
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.socket.state.is_timed_out() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "socket write timeout",
            )));
        }

        if self.socket.state.is_closed() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "socket closed",
            )));
        }

        let current_interest = self.socket.get_registered_interest();
        if !current_interest.is_writable() {
            let new_interest = current_interest.add_writable();

            if self.socket.worker_id.is_none() {
                if let Some(socket_interest) = new_interest.to_socket_interest() {
                    let state_ptr = &self.socket.state as *const _ as usize;
                    if let Ok(_) = crate::runtime::worker::register_socket_with_current_worker(
                        self.socket.fd,
                        socket_interest,
                        state_ptr,
                    ) {
                        if let Some(worker_id) = crate::runtime::worker::current_worker_id() {
                            self.socket.worker_id = Some(worker_id as usize);
                        }
                    }
                }
            } else if let Some(token) = self.socket.state.token() {
                if let Some(socket_interest) = new_interest.to_socket_interest() {
                    if let Some(worker_id) = self.socket.worker_id {
                        let _ = crate::runtime::worker::modify_socket_interest(
                            Token(token),
                            worker_id as u32,
                            socket_interest,
                        );
                    }
                }
            }

            self.socket.set_registered_interest(new_interest);
            self.socket.set_write_waker(cx.waker());
            return Poll::Pending;
        }

        #[cfg(unix)]
        let result = unsafe {
            libc::sendto(
                self.socket.fd,
                self.buf.as_ptr() as *const libc::c_void,
                self.buf.len(),
                0,
                self.addr.as_ptr(),
                self.addr.len_ptr(),
            )
        };
        #[cfg(windows)]
        let result = unsafe {
            winapi::um::winsock2::sendto(
                self.socket.fd,
                self.buf.as_ptr() as *const i8,
                self.buf.len() as i32,
                0,
                self.addr.as_ptr() as *const _,
                self.addr.len(),
            )
        };

        if result < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                self.socket.set_write_waker(cx.waker());
                return Poll::Pending;
            }
            return Poll::Ready(Err(err));
        }

        Poll::Ready(Ok(result as usize))
    }
}
