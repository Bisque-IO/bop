/// Event Loop - Hybrid implementation combining Mio with usockets optimizations
///
/// This loop uses:
/// - Mio for cross-platform event notification
/// - Optional integration with maniac-runtime's SingleWheel for efficient timeouts
/// - usockets' low-priority queue to prevent SSL starvation
/// - usockets' shared receive buffer to reduce allocations

use std::io;
use std::any::Any;
use std::time::Duration;
use mio::{Poll, Events, Interest, Token, Registry};
use mio::event::Source;
use slab::Slab;

/// Token used for waking the event loop from other threads
pub const WAKE_TOKEN: Token = Token(usize::MAX);

#[cfg(windows)]
use winapi::um::winsock2::{
    getsockopt, SOL_SOCKET, SO_TYPE, SO_ACCEPTCONN, SOCK_STREAM, SOCK_DGRAM, SOCKET_ERROR
};
#[cfg(windows)]
use std::os::windows::io::{FromRawSocket, RawSocket};
#[cfg(windows)]
use std::mem::ManuallyDrop;

use crate::net::{
    constants::{RECV_BUFFER_LENGTH, RECV_BUFFER_PADDING},
    low_prio_queue::LowPriorityQueue,
    bsd_sockets::SocketDescriptor,
};

#[cfg(feature = "std")]
use crate::runtime::timer_wheel::SingleWheel;

/// Socket timeout entry for SingleWheel integration
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy)]
struct SocketTimeout {
    token: usize,
}

#[cfg(windows)]
enum WindowsSource {
    TcpStream(ManuallyDrop<mio::net::TcpStream>),
    TcpListener(ManuallyDrop<mio::net::TcpListener>),
    UdpSocket(ManuallyDrop<mio::net::UdpSocket>),
}

#[cfg(windows)]
impl Source for WindowsSource {
    fn register(&mut self, registry: &Registry, token: Token, interest: Interest) -> io::Result<()> {
        match self {
            WindowsSource::TcpStream(s) => s.register(registry, token, interest),
            WindowsSource::TcpListener(s) => s.register(registry, token, interest),
            WindowsSource::UdpSocket(s) => s.register(registry, token, interest),
        }
    }

    fn reregister(&mut self, registry: &Registry, token: Token, interest: Interest) -> io::Result<()> {
        match self {
            WindowsSource::TcpStream(s) => s.reregister(registry, token, interest),
            WindowsSource::TcpListener(s) => s.reregister(registry, token, interest),
            WindowsSource::UdpSocket(s) => s.reregister(registry, token, interest),
        }
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        match self {
            WindowsSource::TcpStream(s) => s.deregister(registry),
            WindowsSource::TcpListener(s) => s.deregister(registry),
            WindowsSource::UdpSocket(s) => s.deregister(registry),
        }
    }
}

/// The main event loop
pub struct EventLoop {
    /// Mio poll for event notification
    poll: Poll,

    /// Event buffer (reused each iteration)
    events: Events,

    /// Optional owned SingleWheel for socket timeouts
    #[cfg(feature = "std")]
    timer_wheel: Option<SingleWheel<SocketTimeout>>,

    /// Low-priority queue (from usockets)
    low_prio_queue: LowPriorityQueue,

    /// Shared receive buffer (from usockets)
    recv_buffer: Vec<u8>,

    /// Active sockets (indexed by Token)
    sockets: Slab<Socket>,

    /// Sockets pending cleanup
    closed_sockets: Vec<Token>,

    /// Did last write fail? (determines if we poll for writable)
    last_write_failed: bool,

    /// Loop iteration number
    iteration_nr: i64,

    /// Pre-iteration callback
    pre_cb: Option<Box<dyn FnMut()>>,

    /// Post-iteration callback
    post_cb: Option<Box<dyn FnMut()>>,

    /// Default poll timeout when no timer wheel is available
    default_timeout: Option<Duration>,
}

/// A socket in the event loop
pub struct Socket {
    /// Raw file descriptor
    pub fd: SocketDescriptor,

    /// Mio token
    pub token: Token,

    /// Optional timer ID if using integrated TimerWheel
    #[cfg(feature = "std")]
    pub timer_id: Option<u64>,

    /// Low priority state (0 = normal, 1 = in queue, 2 = was in queue this iteration)
    pub low_prio_state: u8,

    /// Whether the socket is closed (but not yet freed)
    pub is_closed: bool,

    /// Async socket state for waking tasks
    pub async_state: Option<crate::net::AsyncSocketState>,

    /// Extension data
    pub ext: Box<dyn Any>,

    #[cfg(windows)]
    pub source: Option<WindowsSource>,
}

impl EventLoop {
    /// Create a new standalone event loop without timer integration
    pub fn new() -> io::Result<Self> {
        Self::with_timeout(Some(Duration::from_millis(100)))
    }

    /// Create event loop with custom default timeout
    pub fn with_timeout(default_timeout: Option<Duration>) -> io::Result<Self> {
        let poll = mio::Poll::new()?;

        // Allocate receive buffer with padding
        let buffer_size = RECV_BUFFER_LENGTH + (RECV_BUFFER_PADDING * 2);
        let recv_buffer = vec![0u8; buffer_size];

        Ok(Self {
            poll,
            events: Events::with_capacity(1024),
            #[cfg(feature = "std")]
            timer_wheel: None,
            low_prio_queue: LowPriorityQueue::with_default_budget(),
            recv_buffer,
            sockets: Slab::new(),
            closed_sockets: Vec::new(),
            last_write_failed: false,
            iteration_nr: 0,
            pre_cb: None,
            post_cb: None,
            default_timeout,
        })
    }

    /// Create event loop with owned SingleWheel for timeout management
    ///
    /// This constructor creates an EventLoop with its own SingleWheel instance.
    /// Uses a 2-second tick resolution with 1024 ticks per wheel, providing
    /// coverage up to ~34 minutes (2s × 1024 = 2048s).
    #[cfg(feature = "std")]
    pub fn with_timer_wheel() -> io::Result<Self> {
        let poll = mio::Poll::new()?;

        // Allocate receive buffer with padding
        let buffer_size = RECV_BUFFER_LENGTH + (RECV_BUFFER_PADDING * 2);
        let recv_buffer = vec![0u8; buffer_size];

        // Create owned SingleWheel with 2-second tick resolution and 1024 ticks
        // tick_resolution_ns = 2 seconds = 2_000_000_000 ns (which is 2^31 - 268435456, so we use 2^31)
        let tick_resolution_ns = 1u64 << 31; // 2,147,483,648 ns ≈ 2.147 seconds
        let ticks_per_wheel = 1024;
        let timer_wheel = SingleWheel::new(tick_resolution_ns, ticks_per_wheel, 16);

        Ok(Self {
            poll,
            events: Events::with_capacity(1024),
            timer_wheel: Some(timer_wheel),
            low_prio_queue: LowPriorityQueue::with_default_budget(),
            recv_buffer,
            sockets: Slab::new(),
            closed_sockets: Vec::new(),
            last_write_failed: false,
            iteration_nr: 0,
            pre_cb: None,
            post_cb: None,
            default_timeout: None,
        })
    }

    /// Set pre-iteration callback
    pub fn set_pre_callback(&mut self, callback: impl FnMut() + 'static) {
        self.pre_cb = Some(Box::new(callback));
    }

    /// Set post-iteration callback
    pub fn set_post_callback(&mut self, callback: impl FnMut() + 'static) {
        self.post_cb = Some(Box::new(callback));
    }

    /// Get the shared receive buffer (with padding offset)
    pub fn recv_buffer(&mut self) -> &mut [u8] {
        &mut self.recv_buffer[RECV_BUFFER_PADDING..RECV_BUFFER_LENGTH + RECV_BUFFER_PADDING]
    }

    /// Get Mio registry for registering sockets
    pub fn registry(&self) -> &mio::Registry {
        self.poll.registry()
    }

    /// Create a new waker for this event loop
    pub fn create_waker(&self) -> io::Result<mio::Waker> {
        mio::Waker::new(self.poll.registry(), WAKE_TOKEN)
    }

    /// Add a socket to the loop
    pub fn add_socket(
        &mut self,
        fd: SocketDescriptor,
        interest: Interest,
        async_state: crate::net::AsyncSocketState,
        ext: Box<dyn Any>,
    ) -> io::Result<Token> {
        let entry = self.sockets.vacant_entry();
        let token = Token(entry.key());

        #[cfg(unix)]
        {
            use mio::unix::SourceFd;
            let mut source = SourceFd(&fd);
            self.poll.registry().register(&mut source, token, interest)?;
        }

        #[cfg(windows)]
        let mut source = {
            // Detect socket type
            let mut sock_type: i32 = 0;
            let mut type_len = std::mem::size_of::<i32>() as i32;
            
            let type_res = unsafe {
                getsockopt(
                    fd as usize,
                    SOL_SOCKET,
                    SO_TYPE,
                    &mut sock_type as *mut _ as *mut i8,
                    &mut type_len,
                )
            };

            if type_res == SOCKET_ERROR {
                return Err(io::Error::last_os_error());
            }

            let source = if sock_type == SOCK_DGRAM {
                let std_socket = unsafe { std::net::UdpSocket::from_raw_socket(fd as RawSocket) };
                let mio_socket = mio::net::UdpSocket::from_std(std_socket);
                WindowsSource::UdpSocket(ManuallyDrop::new(mio_socket))
            } else if sock_type == SOCK_STREAM {
                // Check if it's a listener
                let mut accept_conn: i32 = 0;
                let mut accept_len = std::mem::size_of::<i32>() as i32;
                
                let accept_res = unsafe {
                    getsockopt(
                        fd as usize,
                        SOL_SOCKET,
                        SO_ACCEPTCONN,
                        &mut accept_conn as *mut _ as *mut i8,
                        &mut accept_len,
                    )
                };

                if accept_res == SOCKET_ERROR {
                    return Err(io::Error::last_os_error());
                }

                if accept_conn != 0 {
                    let std_listener = unsafe { std::net::TcpListener::from_raw_socket(fd as RawSocket) };
                    let mio_listener = mio::net::TcpListener::from_std(std_listener);
                    WindowsSource::TcpListener(ManuallyDrop::new(mio_listener))
                } else {
                    let std_stream = unsafe { std::net::TcpStream::from_raw_socket(fd as RawSocket) };
                    let mio_stream = mio::net::TcpStream::from_std(std_stream);
                    WindowsSource::TcpStream(ManuallyDrop::new(mio_stream))
                }
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "Unsupported socket type"));
            };

            source
        };

        #[cfg(windows)]
        {
            self.poll.registry().register(&mut source, token, interest)?;
        }

        let socket = Socket {
            fd,
            token,
            #[cfg(feature = "std")]
            timer_id: None,
            low_prio_state: 0,
            is_closed: false,
            async_state: Some(async_state),
            ext,
            #[cfg(windows)]
            source: Some(source),
        };

        entry.insert(socket);

        Ok(token)
    }

    /// Modify socket interest
    pub fn modify_socket(
        &mut self,
        token: Token,
        interest: Interest,
    ) -> io::Result<()> {
        if let Some(socket) = self.sockets.get_mut(token.0) {
            if socket.is_closed {
                return Err(io::Error::new(io::ErrorKind::Other, "Socket is closed"));
            }

            #[cfg(unix)]
            {
                use mio::unix::SourceFd;
                let mut source = SourceFd(&socket.fd);
                self.poll.registry().reregister(&mut source, token, interest)?;
            }

            #[cfg(windows)]
            {
                if let Some(source) = &mut socket.source {
                    self.poll.registry().reregister(source, token, interest)?;
                } else {
                    return Err(io::Error::new(io::ErrorKind::Other, "Socket source not available"));
                }
            }
        } else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Socket not found"));
        }
        Ok(())
    }

    /// Get a socket by token
    pub fn get_socket(&self, token: Token) -> Option<&Socket> {
        self.sockets.get(token.0)
    }

    /// Get a mutable socket by token
    pub fn get_socket_mut(&mut self, token: Token) -> Option<&mut Socket> {
        self.sockets.get_mut(token.0)
    }

    /// Set timeout for a socket (only available with SingleWheel integration)
    #[cfg(feature = "std")]
    pub fn set_timeout(&mut self, token: Token, duration: Duration) -> io::Result<()> {
        if let Some(ref mut timer_wheel) = self.timer_wheel {
            if let Some(socket) = self.sockets.get_mut(token.0) {
                // Cancel old timeout if exists
                if let Some(old_timer_id) = socket.timer_id {
                    let _ = timer_wheel.cancel_timer(old_timer_id);
                }

                // Schedule new timeout
                let now_ns = timer_wheel.now_ns();
                let deadline_ns = now_ns + duration.as_nanos() as u64;

                // Schedule timer with socket token
                let socket_timeout = SocketTimeout {
                    token: socket.token.0,
                };
                match timer_wheel.schedule_timer(deadline_ns, socket_timeout) {
                    Ok(timer_id) => {
                        socket.timer_id = Some(timer_id);
                    }
                    Err(_) => {
                        return Err(io::Error::new(io::ErrorKind::Other, "Failed to schedule timer"));
                    }
                }
            }
        }

        Ok(())
    }

    /// Cancel timeout for a socket (only available with SingleWheel integration)
    #[cfg(feature = "std")]
    pub fn cancel_timeout(&mut self, token: Token) -> io::Result<()> {
        if let Some(ref mut timer_wheel) = self.timer_wheel {
            if let Some(socket) = self.sockets.get_mut(token.0) {
                if let Some(timer_id) = socket.timer_id.take() {
                    let _ = timer_wheel.cancel_timer(timer_id);
                }
            }
        }
        Ok(())
    }

    /// Mark socket for low-priority processing
    pub fn mark_low_priority(&mut self, token: Token) -> io::Result<()> {
        if let Some(socket) = self.sockets.get(token.0) {
            if socket.low_prio_state == 0 {
                self.low_prio_queue.push(token);

                if let Some(socket) = self.sockets.get_mut(token.0) {
                    socket.low_prio_state = 1;
                }
            }
        }

        Ok(())
    }

    /// Close a socket
    pub fn close_socket(&mut self, token: Token) {
        if let Some(socket) = self.sockets.get_mut(token.0) {
            if !socket.is_closed {
                socket.is_closed = true;
                self.closed_sockets.push(token);

                // Cancel timer if using SingleWheel integration
                #[cfg(feature = "std")]
                if let Some(ref mut timer_wheel) = self.timer_wheel {
                    if let Some(timer_id) = socket.timer_id.take() {
                        let _ = timer_wheel.cancel_timer(timer_id);
                    }
                }
                
                #[cfg(windows)]
                if let Some(mut source) = socket.source.take() {
                     let _ = self.poll.registry().deregister(&mut source);
                     // Source dropped here, but ManuallyDrop prevents closing FD
                }

                // Remove from low-priority queue
                self.low_prio_queue.remove(token);

                // Wake tasks and mark socket as closed
                if let Some(state) = &socket.async_state {
                    state.set_closed(true);
                    state.wake_all();
                }
            }
        }
    }

    /// Process low-priority sockets
    fn process_low_priority(&mut self) {
        self.low_prio_queue.reset_budget();

        let tokens: Vec<Token> = std::iter::from_fn(|| self.low_prio_queue.pop()).collect();

        for token in tokens {
            if let Some(socket) = self.sockets.get_mut(token.0) {
                socket.low_prio_state = 2; // Mark as processed this iteration
            }
        }
    }

    /// Process expired timers from SingleWheel (if integrated)
    #[cfg(feature = "std")]
    fn process_timers(&mut self) {
        if let Some(ref mut timer_wheel) = self.timer_wheel {
            let now_ns = timer_wheel.now_ns();

            // Poll for expired timers
            let mut expired_timers = Vec::with_capacity(256);
            let _count = timer_wheel.poll(now_ns, 256, &mut expired_timers);

            // Call timeout handlers for expired timers
            for (_timer_id, _deadline_ns, socket_timeout) in expired_timers {
                let token = Token(socket_timeout.token);
                if let Some(socket) = self.sockets.get_mut(token.0) {
                    if socket.is_closed {
                        continue;
                    }

                    // Clear timer_id since it expired
                    socket.timer_id = None;

                    // Wake tasks and mark timeout
                    if let Some(state) = &socket.async_state {
                        state.set_timed_out(true);
                        state.wake_all();
                    }
                }
            }
        }
    }

    /// Cleanup closed sockets
    fn cleanup_closed(&mut self) {
        for token in self.closed_sockets.drain(..) {
            if self.sockets.contains(token.0) {
                self.sockets.remove(token.0);
            }
        }
    }

    /// Poll the event loop once
    ///
    /// # Arguments
    /// * `timeout` - Optional timeout for blocking. If `None`, uses non-blocking mode (Duration::ZERO).
    ///
    /// # Returns
    /// Returns the number of events processed
    pub fn poll_once(&mut self, timeout: Option<Duration>) -> io::Result<usize> {
        // Pre-iteration callback
        if let Some(ref mut cb) = self.pre_cb {
            cb();
        }

        // Process low-priority sockets first
        self.process_low_priority();

        // Process expired timers (if SingleWheel is integrated)
        #[cfg(feature = "std")]
        self.process_timers();

        // Use provided timeout or calculate from timer wheel.
        // If both are None, we block indefinitely (standard mio behavior).
        let poll_timeout = timeout.or_else(|| self.calculate_poll_timeout());

        // Poll for events
        self.poll.poll(&mut self.events, poll_timeout)?;

        // Process events directly by using unsafe to bypass borrow checker
        // SAFETY: We only read from self.events.iter() and never mutate self.events
        // during iteration. All mutable operations affect other fields (sockets, low_prio_queue, etc.)
        let events_ptr = &self.events as *const Events;
        let mut event_count = 0;

        for event in unsafe { &*events_ptr }.iter() {
            let token = event.token();

            if token == WAKE_TOKEN {
                continue;
            }

            let is_readable = event.is_readable();
            let is_writable = event.is_writable();

            if let Some(socket) = self.sockets.get(token.0) {
                if socket.is_closed {
                    continue;
                }
            }

            // Handle events
            self.handle_socket_event(token, is_readable, is_writable)?;
            event_count += 1;
        }

        // Cleanup closed sockets
        self.cleanup_closed();

        // Post-iteration callback
        if let Some(ref mut cb) = self.post_cb {
            cb();
        }

        self.iteration_nr += 1;

        Ok(event_count)
    }

    /// Run the event loop indefinitely
    pub fn run(&mut self) -> io::Result<()> {
        loop {
            self.poll_once(None)?;
        }
    }

    /// Calculate the poll timeout based on TimerWheel or default timeout
    fn calculate_poll_timeout(&mut self) -> Option<Duration> {
        #[cfg(feature = "std")]
        if let Some(ref mut timer_wheel) = self.timer_wheel {
            if let Some(next_deadline_ns) = timer_wheel.next_deadline() {
                let now_ns = timer_wheel.now_ns();
                if next_deadline_ns > now_ns {
                    let duration_ns = next_deadline_ns - now_ns;
                    return Some(Duration::from_nanos(duration_ns));
                } else {
                    // Timer already expired, poll immediately
                    return Some(Duration::ZERO);
                }
            }
        }

        // Use default timeout if no TimerWheel or no pending timers
        self.default_timeout
    }

    /// Handle a socket event
    fn handle_socket_event(&mut self, token: Token, is_readable: bool, is_writable: bool) -> io::Result<()> {
        if is_readable {
            if let Some(socket) = self.sockets.get_mut(token.0) {
                // Wake read task when socket becomes readable
                if let Some(state) = &socket.async_state {
                    state.wake_read();
                }
            }
        }

        if is_writable {
            self.last_write_failed = false;

            if let Some(socket) = self.sockets.get_mut(token.0) {
                // Wake write task when socket becomes writable
                if let Some(state) = &socket.async_state {
                    state.wake_write();
                }
            }
        }

        Ok(())
    }

    /// Get current loop iteration number
    pub fn iteration_number(&self) -> i64 {
        self.iteration_nr
    }

    /// Get number of active sockets
    pub fn socket_count(&self) -> usize {
        self.sockets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_creation() {
        let event_loop = EventLoop::new().unwrap();
        assert_eq!(event_loop.iteration_number(), 0);
        assert_eq!(event_loop.socket_count(), 0);
    }

    #[test]
    fn test_add_socket() {
        let mut event_loop = EventLoop::new().unwrap();
        let state = crate::net::AsyncSocketState::new();

        let token = event_loop
            .add_socket(
                1 as SocketDescriptor,
                Interest::READABLE,
                state,
                Box::new(()),
            )
            .unwrap();

        assert_eq!(event_loop.socket_count(), 1);
        assert_eq!(token.0, 0);
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_timeout_management() {
        use std::time::Duration as StdDuration;

        // Create event loop with owned SingleWheel
        let mut event_loop = EventLoop::with_timer_wheel().unwrap();
        
        // We need to provide a valid AsyncSocketState here, not a DummyHandler if that doesn't exist/match
        let state = crate::net::AsyncSocketState::new();

        let token = event_loop
            .add_socket(
                1 as SocketDescriptor,
                Interest::READABLE,
                state,
                Box::new(()),
            )
            .unwrap();

        event_loop
            .set_timeout(token, StdDuration::from_secs(30))
            .unwrap();

        let socket = event_loop.sockets.get(token.0).unwrap();
        assert!(socket.timer_id.is_some());
    }

    #[test]
    fn test_close_socket() {
        let mut event_loop = EventLoop::new().unwrap();
        let state = crate::net::AsyncSocketState::new();

        let token = event_loop
            .add_socket(
                1 as SocketDescriptor,
                Interest::READABLE,
                state,
                Box::new(()),
            )
            .unwrap();

        event_loop.close_socket(token);
        event_loop.cleanup_closed();

        assert_eq!(event_loop.socket_count(), 0);
    }

    #[test]
    fn test_low_priority() {
        let mut event_loop = EventLoop::new().unwrap();
        let state = crate::net::AsyncSocketState::new();

        let token = event_loop
            .add_socket(
                1 as SocketDescriptor,
                Interest::READABLE,
                state,
                Box::new(()),
            )
            .unwrap();

        event_loop.mark_low_priority(token).unwrap();

        assert_eq!(event_loop.low_prio_queue.len(), 1);
    }
}
