/// BSD socket abstraction layer - cross-platform socket operations
///
/// This module provides a thin, safe abstraction over BSD sockets that works
/// across Unix and Windows platforms.

use std::io::{self, Error as IoError, ErrorKind};
use std::mem::{self, MaybeUninit};
use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};
use std::ptr;

#[cfg(unix)]
pub type SocketDescriptor = std::os::unix::io::RawFd;

#[cfg(windows)]
pub type SocketDescriptor = winapi::um::winsock2::SOCKET;

#[cfg(windows)]
use winapi::um::winsock2::{
    SOCKET, INVALID_SOCKET, SOCKET_ERROR,
    closesocket, setsockopt, getsockopt, bind, listen, accept, connect,
    send, recv, shutdown, getsockname, getpeername, ioctlsocket, socket,
    SO_REUSEADDR, SO_ERROR, SOL_SOCKET, FIONBIO,
    SOCK_STREAM, SOCK_DGRAM, SD_SEND, SD_RECEIVE, SD_BOTH,
};

#[cfg(windows)]
use winapi::shared::ws2def::{SOCKADDR, SOCKADDR_IN, SOCKADDR_STORAGE};

// SOCKADDR_IN6 definition for Windows
// Note: winapi doesn't export ws2ipdef properly, so we define it manually
#[cfg(windows)]
#[repr(C)]
pub struct SOCKADDR_IN6_COMPAT {
    pub sin6_family: u16,
    pub sin6_port: u16,
    pub sin6_flowinfo: u32,
    pub sin6_addr: [u8; 16],
    pub sin6_scope_id: u32,
}

#[cfg(windows)]
#[allow(non_camel_case_types)]
type sockaddr = SOCKADDR;
#[cfg(windows)]
#[allow(non_camel_case_types)]
type sockaddr_in = SOCKADDR_IN;
#[cfg(windows)]
#[allow(non_camel_case_types)]
type sockaddr_in6 = SOCKADDR_IN6_COMPAT;
#[cfg(windows)]
#[allow(non_camel_case_types)]
type sockaddr_storage = SOCKADDR_STORAGE;

#[cfg(windows)]
#[allow(non_camel_case_types)]
type socklen_t = i32;

#[cfg(windows)]
const TCP_NODELAY: i32 = 1; // winapi doesn't export this constant
#[cfg(windows)]
const IPPROTO_TCP: i32 = 6;
#[cfg(windows)]
const IPPROTO_UDP: i32 = 17;
#[cfg(windows)]
const AF_INET: i32 = 2;
#[cfg(windows)]
const AF_INET6: i32 = 23;

#[cfg(unix)]
use libc::{
    c_int, c_void, socklen_t,
    close, setsockopt, getsockopt, bind, listen, accept, connect,
    send, recv, shutdown, getsockname, getpeername,
    socket, fcntl, ioctl,
    SO_REUSEADDR, SO_REUSEPORT, SO_ERROR, TCP_NODELAY, MSG_NOSIGNAL, MSG_MORE,
    SOCK_STREAM, SOCK_DGRAM, SOCK_NONBLOCK, SOCK_CLOEXEC,
    IPPROTO_TCP, IPPROTO_UDP,
    AF_INET, AF_INET6, AF_UNIX,
    SHUT_WR, SHUT_RD, SHUT_RDWR,
    F_GETFL, F_SETFL, O_NONBLOCK,
    FIONBIO, EINPROGRESS, EWOULDBLOCK, EAGAIN,
    sockaddr, sockaddr_in, sockaddr_in6, sockaddr_un, sockaddr_storage,
};

pub const SOCKET_ERROR_CODE: i32 = -1;

#[cfg(windows)]
const MSG_NOSIGNAL: i32 = 0;

#[cfg(windows)]
use std::os::raw::c_void;

/// Socket address abstraction
#[derive(Clone)]
pub struct BsdAddr {
    storage: sockaddr_storage,
    len: socklen_t,
}

impl BsdAddr {
    pub fn new() -> Self {
        Self {
            storage: unsafe { mem::zeroed() },
            len: mem::size_of::<sockaddr_storage>() as socklen_t,
        }
    }

    pub fn from_socket_addr(addr: &SocketAddr) -> Self {
        let mut bsd_addr = Self::new();

        match addr {
            SocketAddr::V4(v4) => {
                let mut sin: sockaddr_in = unsafe { mem::zeroed() };
                #[cfg(unix)]
                {
                    sin.sin_family = AF_INET as u16;
                }
                #[cfg(windows)]
                {
                    sin.sin_family = AF_INET as u16;
                }
                sin.sin_port = v4.port().to_be();
                #[cfg(unix)]
                {
                    sin.sin_addr.s_addr = u32::from_ne_bytes(v4.ip().octets()).to_be();
                }
                #[cfg(windows)]
                unsafe {
                    *sin.sin_addr.S_un.S_addr_mut() = u32::from_ne_bytes(v4.ip().octets()).to_be();
                }

                unsafe {
                    ptr::copy_nonoverlapping(
                        &sin as *const _ as *const u8,
                        &mut bsd_addr.storage as *mut _ as *mut u8,
                        mem::size_of::<sockaddr_in>(),
                    );
                }
                bsd_addr.len = mem::size_of::<sockaddr_in>() as socklen_t;
            }
            SocketAddr::V6(v6) => {
                let mut sin6: sockaddr_in6 = unsafe { mem::zeroed() };
                #[cfg(unix)]
                {
                    sin6.sin6_family = AF_INET6 as u16;
                }
                #[cfg(windows)]
                {
                    sin6.sin6_family = AF_INET6 as u16;
                }
                sin6.sin6_port = v6.port().to_be();
                #[cfg(unix)]
                {
                    sin6.sin6_addr.s6_addr = v6.ip().octets();
                }
                #[cfg(windows)]
                {
                    sin6.sin6_addr = v6.ip().octets();
                }
                sin6.sin6_flowinfo = v6.flowinfo();
                sin6.sin6_scope_id = v6.scope_id();

                unsafe {
                    ptr::copy_nonoverlapping(
                        &sin6 as *const _ as *const u8,
                        &mut bsd_addr.storage as *mut _ as *mut u8,
                        mem::size_of::<sockaddr_in6>(),
                    );
                }
                bsd_addr.len = mem::size_of::<sockaddr_in6>() as socklen_t;
            }
        }

        bsd_addr
    }

    pub fn as_ptr(&self) -> *const sockaddr {
        &self.storage as *const _ as *const sockaddr
    }

    pub fn as_mut_ptr(&mut self) -> *mut sockaddr {
        &mut self.storage as *mut _ as *mut sockaddr
    }

    pub fn len_ptr(&mut self) -> *mut socklen_t {
        &mut self.len as *mut socklen_t
    }

    pub fn len(&self) -> socklen_t {
        self.len
    }

    pub fn to_socket_addr(&self) -> Option<SocketAddr> {
        unsafe {
            match self.storage.ss_family as i32 {
                #[cfg(unix)]
                AF_INET => {
                    let sin = &*(&self.storage as *const _ as *const sockaddr_in);
                    let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr).to_ne_bytes());
                    let port = u16::from_be(sin.sin_port);
                    Some(SocketAddr::new(IpAddr::V4(ip), port))
                }
                #[cfg(unix)]
                AF_INET6 => {
                    let sin6 = &*(&self.storage as *const _ as *const sockaddr_in6);
                    let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
                    let port = u16::from_be(sin6.sin6_port);
                    Some(SocketAddr::new(IpAddr::V6(ip), port))
                }
                #[cfg(windows)]
                2 => { // AF_INET on Windows
                    let sin = &*(&self.storage as *const _ as *const sockaddr_in);
                    let ip = Ipv4Addr::from(u32::from_be(*sin.sin_addr.S_un.S_addr()).to_ne_bytes());
                    let port = u16::from_be(sin.sin_port);
                    Some(SocketAddr::new(IpAddr::V4(ip), port))
                }
                #[cfg(windows)]
                23 => { // AF_INET6 on Windows
                    let sin6 = &*(&self.storage as *const _ as *const sockaddr_in6);
                    let ip = Ipv6Addr::from(sin6.sin6_addr);
                    let port = u16::from_be(sin6.sin6_port);
                    Some(SocketAddr::new(IpAddr::V6(ip), port))
                }
                _ => None,
            }
        }
    }

    pub fn port(&self) -> u16 {
        unsafe {
            match self.storage.ss_family as i32 {
                #[cfg(unix)]
                AF_INET => {
                    let sin = &*(&self.storage as *const _ as *const sockaddr_in);
                    u16::from_be(sin.sin_port)
                }
                #[cfg(unix)]
                AF_INET6 => {
                    let sin6 = &*(&self.storage as *const _ as *const sockaddr_in6);
                    u16::from_be(sin6.sin6_port)
                }
                #[cfg(windows)]
                2 | 23 => {
                    let sin = &*(&self.storage as *const _ as *const sockaddr_in);
                    u16::from_be(sin.sin_port)
                }
                _ => 0,
            }
        }
    }
}

/// Create a TCP socket
pub fn create_socket(domain: i32, sock_type: i32, protocol: i32) -> io::Result<SocketDescriptor> {
    #[cfg(unix)]
    {
        let fd = unsafe { socket(domain, sock_type | SOCK_NONBLOCK | SOCK_CLOEXEC, protocol) };
        if fd == -1 {
            return Err(IoError::last_os_error());
        }
        Ok(fd)
    }

    #[cfg(windows)]
    {
        let sock = unsafe { socket(domain, sock_type, protocol) };
        if sock == INVALID_SOCKET {
            return Err(IoError::last_os_error());
        }

        // Set non-blocking
        let mut nonblocking: u32 = 1;
        if unsafe { ioctlsocket(sock, FIONBIO, &mut nonblocking) } != 0 {
            unsafe { closesocket(sock) };
            return Err(IoError::last_os_error());
        }

        Ok(sock)
    }
}

/// Close a socket
pub fn close_socket(fd: SocketDescriptor) {
    #[cfg(unix)]
    unsafe {
        close(fd);
    }

    #[cfg(windows)]
    unsafe {
        closesocket(fd);
    }
}

/// Set socket to non-blocking mode
pub fn set_nonblocking(fd: SocketDescriptor, nonblocking: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        let flags = unsafe { fcntl(fd, F_GETFL, 0) };
        if flags == -1 {
            return Err(IoError::last_os_error());
        }

        let flags = if nonblocking {
            flags | O_NONBLOCK
        } else {
            flags & !O_NONBLOCK
        };

        if unsafe { fcntl(fd, F_SETFL, flags) } == -1 {
            return Err(IoError::last_os_error());
        }

        Ok(())
    }

    #[cfg(windows)]
    {
        let mut mode: u32 = if nonblocking { 1 } else { 0 };
        if unsafe { ioctlsocket(fd, FIONBIO, &mut mode) } != 0 {
            return Err(IoError::last_os_error());
        }
        Ok(())
    }
}

/// Enable TCP_NODELAY
pub fn set_nodelay(fd: SocketDescriptor, nodelay: bool) -> io::Result<()> {
    let optval: i32 = if nodelay { 1 } else { 0 };

    #[cfg(unix)]
    {
        if unsafe {
            setsockopt(
                fd,
                IPPROTO_TCP,
                TCP_NODELAY,
                &optval as *const _ as *const c_void,
                mem::size_of::<i32>() as socklen_t,
            )
        } == -1
        {
            return Err(IoError::last_os_error());
        }
    }

    #[cfg(windows)]
    {
        if unsafe {
            setsockopt(
                fd,
                IPPROTO_TCP,
                TCP_NODELAY,
                &optval as *const _ as *const i8,
                mem::size_of::<i32>() as i32,
            )
        } == SOCKET_ERROR
        {
            return Err(IoError::last_os_error());
        }
    }

    Ok(())
}

/// Enable SO_REUSEADDR
pub fn set_reuseaddr(fd: SocketDescriptor, reuse: bool) -> io::Result<()> {
    let optval: i32 = if reuse { 1 } else { 0 };

    #[cfg(unix)]
    {
        if unsafe {
            setsockopt(
                fd,
                libc::SOL_SOCKET,
                SO_REUSEADDR,
                &optval as *const _ as *const c_void,
                mem::size_of::<i32>() as socklen_t,
            )
        } == -1
        {
            return Err(IoError::last_os_error());
        }
    }

    #[cfg(windows)]
    {
        if unsafe {
            setsockopt(
                fd,
                SOL_SOCKET,
                SO_REUSEADDR,
                &optval as *const _ as *const i8,
                mem::size_of::<i32>() as i32,
            )
        } == SOCKET_ERROR
        {
            return Err(IoError::last_os_error());
        }
    }

    Ok(())
}

/// Enable SO_REUSEPORT (Unix only)
#[cfg(unix)]
pub fn set_reuseport(fd: SocketDescriptor, reuse: bool) -> io::Result<()> {
    let optval: i32 = if reuse { 1 } else { 0 };

    if unsafe {
        setsockopt(
            fd,
            libc::SOL_SOCKET,
            SO_REUSEPORT,
            &optval as *const _ as *const c_void,
            mem::size_of::<i32>() as socklen_t,
        )
    } == -1
    {
        return Err(IoError::last_os_error());
    }

    Ok(())
}

/// Bind socket to address
pub fn bind_socket(fd: SocketDescriptor, addr: &BsdAddr) -> io::Result<()> {
    #[cfg(unix)]
    {
        if unsafe { bind(fd, addr.as_ptr(), addr.len) } == -1 {
            return Err(IoError::last_os_error());
        }
    }

    #[cfg(windows)]
    {
        if unsafe { bind(fd, addr.as_ptr(), addr.len as i32) } == SOCKET_ERROR {
            return Err(IoError::last_os_error());
        }
    }

    Ok(())
}

/// Listen on socket
pub fn listen_socket(fd: SocketDescriptor, backlog: i32) -> io::Result<()> {
    if unsafe { listen(fd, backlog) } == SOCKET_ERROR_CODE {
        return Err(IoError::last_os_error());
    }
    Ok(())
}

/// Accept connection
pub fn accept_socket(fd: SocketDescriptor) -> io::Result<(SocketDescriptor, BsdAddr)> {
    let mut addr = BsdAddr::new();

    #[cfg(unix)]
    {
        let client_fd = unsafe { accept(fd, addr.as_mut_ptr(), addr.len_ptr()) };
        if client_fd == -1 {
            return Err(IoError::last_os_error());
        }

        // Set non-blocking and close-on-exec
        set_nonblocking(client_fd, true)?;

        Ok((client_fd, addr))
    }

    #[cfg(windows)]
    {
        let client_sock = unsafe { accept(fd, addr.as_mut_ptr(), addr.len_ptr()) };
        if client_sock == INVALID_SOCKET {
            return Err(IoError::last_os_error());
        }

        set_nonblocking(client_sock, true)?;

        Ok((client_sock, addr))
    }
}

/// Connect socket to address
pub fn connect_socket(fd: SocketDescriptor, addr: &BsdAddr) -> io::Result<()> {
    #[cfg(unix)]
    {
        let ret = unsafe { connect(fd, addr.as_ptr(), addr.len) };
        if ret == -1 {
            let err = IoError::last_os_error();
            // EINPROGRESS is expected for non-blocking connects
            if err.raw_os_error() == Some(EINPROGRESS) {
                return Ok(());
            }
            return Err(err);
        }
    }

    #[cfg(windows)]
    {
        let ret = unsafe { connect(fd, addr.as_ptr(), addr.len as i32) };
        if ret == SOCKET_ERROR {
            let err = IoError::last_os_error();
            // WSAEWOULDBLOCK is expected for non-blocking connects
            if err.kind() == ErrorKind::WouldBlock {
                return Ok(());
            }
            return Err(err);
        }
    }

    Ok(())
}

/// Send data on socket
pub fn send_socket(fd: SocketDescriptor, buf: &[u8], flags: i32) -> io::Result<usize> {
    #[cfg(unix)]
    {
        let ret = unsafe {
            send(fd, buf.as_ptr() as *const c_void, buf.len(), flags | MSG_NOSIGNAL)
        };
        if ret == -1 {
            return Err(IoError::last_os_error());
        }
        Ok(ret as usize)
    }

    #[cfg(windows)]
    {
        let ret = unsafe {
            send(fd, buf.as_ptr() as *const i8, buf.len() as i32, flags)
        };
        if ret == SOCKET_ERROR {
            return Err(IoError::last_os_error());
        }
        Ok(ret as usize)
    }
}

/// Receive data from socket
pub fn recv_socket(fd: SocketDescriptor, buf: &mut [u8], flags: i32) -> io::Result<usize> {
    #[cfg(unix)]
    {
        let ret = unsafe {
            recv(fd, buf.as_mut_ptr() as *mut c_void, buf.len(), flags)
        };
        if ret == -1 {
            return Err(IoError::last_os_error());
        }
        Ok(ret as usize)
    }

    #[cfg(windows)]
    {
        let ret = unsafe {
            recv(fd, buf.as_mut_ptr() as *mut i8, buf.len() as i32, flags)
        };
        if ret == SOCKET_ERROR {
            return Err(IoError::last_os_error());
        }
        Ok(ret as usize)
    }
}

/// Shutdown socket
pub fn shutdown_socket(fd: SocketDescriptor, how: i32) -> io::Result<()> {
    #[cfg(unix)]
    {
        if unsafe { shutdown(fd, how) } == -1 {
            return Err(IoError::last_os_error());
        }
    }

    #[cfg(windows)]
    {
        if unsafe { shutdown(fd, how) } == SOCKET_ERROR {
            return Err(IoError::last_os_error());
        }
    }

    Ok(())
}

/// Get local address
pub fn local_addr(fd: SocketDescriptor) -> io::Result<BsdAddr> {
    let mut addr = BsdAddr::new();

    #[cfg(unix)]
    {
        if unsafe { getsockname(fd, addr.as_mut_ptr(), addr.len_ptr()) } == -1 {
            return Err(IoError::last_os_error());
        }
    }

    #[cfg(windows)]
    {
        if unsafe { getsockname(fd, addr.as_mut_ptr(), addr.len_ptr()) } == SOCKET_ERROR {
            return Err(IoError::last_os_error());
        }
    }

    Ok(addr)
}

/// Get peer address
pub fn peer_addr(fd: SocketDescriptor) -> io::Result<BsdAddr> {
    let mut addr = BsdAddr::new();

    #[cfg(unix)]
    {
        if unsafe { getpeername(fd, addr.as_mut_ptr(), addr.len_ptr()) } == -1 {
            return Err(IoError::last_os_error());
        }
    }

    #[cfg(windows)]
    {
        if unsafe { getpeername(fd, addr.as_mut_ptr(), addr.len_ptr()) } == SOCKET_ERROR {
            return Err(IoError::last_os_error());
        }
    }

    Ok(addr)
}

/// Check if socket has an error
pub fn socket_error(fd: SocketDescriptor) -> io::Result<Option<i32>> {
    let mut error: i32 = 0;
    let len = mem::size_of::<i32>() as socklen_t;

    #[cfg(unix)]
    {
        if unsafe {
            getsockopt(
                fd,
                libc::SOL_SOCKET,
                SO_ERROR,
                &mut error as *mut _ as *mut c_void,
                &mut len,
            )
        } == -1
        {
            return Err(IoError::last_os_error());
        }
    }

    #[cfg(windows)]
    {
        let mut len = mem::size_of::<i32>() as i32;
        if unsafe {
            getsockopt(
                fd,
                SOL_SOCKET,
                SO_ERROR,
                &mut error as *mut _ as *mut i8,
                &mut len,
            )
        } == SOCKET_ERROR
        {
            return Err(IoError::last_os_error());
        }
    }

    Ok(if error == 0 { None } else { Some(error) })
}

/// Check if error is EWOULDBLOCK or EAGAIN
pub fn would_block(err: &IoError) -> bool {
    err.kind() == ErrorKind::WouldBlock
}

/// Constants for shutdown()
pub mod shutdown_mode {
    #[cfg(unix)]
    pub const READ: i32 = libc::SHUT_RD;
    #[cfg(unix)]
    pub const WRITE: i32 = libc::SHUT_WR;
    #[cfg(unix)]
    pub const BOTH: i32 = libc::SHUT_RDWR;

    #[cfg(windows)]
    use winapi::um::winsock2::{SD_RECEIVE, SD_SEND, SD_BOTH};

    #[cfg(windows)]
    pub const READ: i32 = SD_RECEIVE;
    #[cfg(windows)]
    pub const WRITE: i32 = SD_SEND;
    #[cfg(windows)]
    pub const BOTH: i32 = SD_BOTH;
}

/// MSG_MORE for TCP cork (Linux only)
#[cfg(target_os = "linux")]
pub const MSG_MORE_FLAG: i32 = MSG_MORE;

#[cfg(not(target_os = "linux"))]
pub const MSG_MORE_FLAG: i32 = 0;
