#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{
    AsRawHandle, AsRawSocket, FromRawSocket, IntoRawSocket, OwnedSocket, RawHandle, RawSocket,
};
use std::{cell::UnsafeCell, io, sync::Arc};

use super::{scheduled_io::ScheduledIo, CURRENT};
#[cfg(windows)]
use std::os::windows::io::RawSocket as RawFd;
#[cfg(unix)]
use crate::current_worker_id;

// Tracks in-flight operations on a file descriptor. Ensures all in-flight
// operations complete before submitting the close.
#[derive(Clone, Debug)]
pub(crate) struct SharedFd {
    inner: Arc<Inner>,
}

unsafe impl Send for SharedFd {}
unsafe impl Sync for SharedFd {}

struct Inner {
    // Open file descriptor
    #[cfg(any(unix, windows))]
    fd: RawFd,

    // Waker to notify when the close operation completes.
    state: UnsafeCell<State>,

    worker_id: u32,
    
    // Readiness state and wakers (poller only)
    // On poller platforms, this contains the ScheduledIo (Arc so it can be weakly referenced by the slab)
    #[cfg(feature = "poll")]
    pub(crate) scheduled_io: std::sync::Arc<crate::monoio::driver::scheduled_io::ScheduledIo>,
}

enum State {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    Uring(UringState),
    #[cfg(feature = "poll")]
    Poller(Option<usize>),
}

#[cfg(feature = "poll-io")]
impl State {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    #[allow(unreachable_patterns)]
    pub(crate) fn cvt_uring_poll(&mut self, fd: RawFd) -> io::Result<()> {
        let state = match self {
            State::Uring(state) => state,
            _ => return Ok(()),
        };
        // TODO: only Init state can convert?
        if matches!(state, UringState::Init) {
            let mut source = mio::unix::SourceFd(&fd);
            crate::syscall!(fcntl@RAW(fd, libc::F_SETFL, libc::O_NONBLOCK))?;
            let reg = CURRENT
                .with(|inner| match inner {
                    #[cfg(all(target_os = "linux", feature = "iouring"))]
                    crate::monoio::driver::Inner::Uring(r) => super::IoUringDriver::register_poll_io(
                        r,
                        &mut source,
                        super::ready::RW_INTERESTS,
                    ),
                    #[cfg(feature = "poll")]
                    crate::monoio::driver::Inner::Poller(_) => panic!("unexpected poller runtime"),
                })
                .inspect_err(|_| {
                    let _ = crate::syscall!(fcntl@RAW(fd, libc::F_SETFL, 0));
                })?;
            *state = UringState::Poller(Some(reg));
        } else {
            return Err(io::Error::other("not clear uring state"));
        }
        Ok(())
    }

    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    #[inline]
    pub(crate) fn cvt_uring_poll(&mut self, _fd: RawFd) -> io::Result<()> {
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "iouring"))]
    pub(crate) fn cvt_comp(&mut self, fd: RawFd) -> io::Result<()> {
        let inner = match self {
            Self::Uring(UringState::Poller(inner)) => inner,
            _ => return Ok(()),
        };
        let Some(token) = inner else {
            return Err(io::Error::other("empty token"));
        };
        let mut source = mio::unix::SourceFd(&fd);
        crate::syscall!(fcntl@RAW(fd, libc::F_SETFL, 0))?;
        CURRENT
            .with(|inner| match inner {
                #[cfg(all(target_os = "linux", feature = "iouring"))]
                crate::monoio::driver::Inner::Uring(r) => {
                    super::IoUringDriver::deregister_poll_io(r, &mut source, *token)
                }
                #[cfg(feature = "poll")]
                crate::monoio::driver::Inner::Poller(_) => panic!("unexpected poller runtime"),
            })
            .inspect_err(|_| {
                let _ = crate::syscall!(fcntl@RAW(fd, libc::F_SETFL, libc::O_NONBLOCK));
            })?;
        *self = State::Uring(UringState::Init);
        Ok(())
    }

    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    #[inline]
    pub(crate) fn cvt_comp(&mut self, _fd: RawFd) -> io::Result<()> {
        Ok(())
    }
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner").field("fd", &self.fd).finish()
    }
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
enum UringState {
    /// Initial state
    Init,

    /// Waiting for all in-flight operation to complete.
    Waiting(Option<std::task::Waker>),

    /// The FD is closing
    Closing(super::op::Op<super::op::close::Close>),

    /// The FD is fully closed
    Closed,

    /// Poller
    #[cfg(feature = "poll-io")]
    Poller(Option<usize>),
}

#[cfg(unix)]
impl AsRawFd for SharedFd {
    fn as_raw_fd(&self) -> RawFd {
        self.raw_fd()
    }
}

#[cfg(windows)]
impl AsRawSocket for SharedFd {
    fn as_raw_socket(&self) -> RawSocket {
        self.raw_socket()
    }
}

#[cfg(windows)]
impl AsRawHandle for SharedFd {
    fn as_raw_handle(&self) -> RawHandle {
        self.raw_handle()
    }
}

impl SharedFd {
    #[cfg(unix)]
    #[allow(unreachable_code, unused)]
    pub(crate) fn new<const FORCE_POLL: bool>(fd: RawFd) -> io::Result<SharedFd> {
        enum Reg {
            Uring,
            #[cfg(feature = "poll-io")]
            UringPoller(io::Result<usize>),
            #[cfg(feature = "poll")]
            Poller(io::Result<usize>),
        }
        
        // Create ScheduledIo first (for poller platforms)
        #[cfg(feature = "poll")]
        let scheduled_io = std::sync::Arc::new(crate::monoio::driver::scheduled_io::ScheduledIo::new());

        #[cfg(all(target_os = "linux", feature = "iouring", feature = "poll"))]
        let state = match CURRENT.with(|inner| match inner {
            super::Inner::Uring(inner) => match FORCE_POLL {
                false => Reg::Uring,
                true => {
                    #[cfg(feature = "poll-io")]
                    {
                        let mut source = mio::unix::SourceFd(&fd);
                        Reg::UringPoller(super::IoUringDriver::register_poll_io(
                            inner,
                            &mut source,
                            super::ready::RW_INTERESTS,
                        ))
                    }
                    #[cfg(not(feature = "poll-io"))]
                    Reg::Uring
                }
            },
            super::Inner::Poller(inner) => {
                let mut source = mio::unix::SourceFd(&fd);
                Reg::Poller(super::poller::PollerDriver::register(
                    inner,
                    &mut source,
                    super::ready::RW_INTERESTS,
                    &scheduled_io,
                ))
            }
        }) {
            Reg::Uring => State::Uring(UringState::Init),
            #[cfg(feature = "poll-io")]
            Reg::UringPoller(idx) => State::Uring(UringState::Poller(Some(idx?))),
            #[cfg(feature = "poll")]
            Reg::Poller(idx) => State::Poller(Some(idx?)),
        };

        #[cfg(all(not(feature = "poll"), target_os = "linux", feature = "iouring"))]
        let state = State::Uring(UringState::Init);

        #[cfg(all(
            unix,
            feature = "poll",
            not(all(target_os = "linux", feature = "iouring"))
        ))]
        let scheduled_io = std::sync::Arc::new(crate::monoio::driver::scheduled_io::ScheduledIo::new());

        #[cfg(all(
            unix,
            feature = "poll",
            not(all(target_os = "linux", feature = "iouring"))
        ))]
        let state = {
            let reg = CURRENT.with(|inner| match inner {
                super::Inner::Poller(inner) => {
                    let mut source = mio::unix::SourceFd(&fd);
                    super::poller::PollerDriver::register(
                        inner,
                        &mut source,
                        super::ready::RW_INTERESTS,
                        &scheduled_io,
                    )
                }
            });

            State::Poller(Some(reg?))
        };

        #[cfg(all(
            not(feature = "poll"),
            not(all(target_os = "linux", feature = "iouring"))
        ))]
        #[allow(unused)]
        let state = super::util::feature_panic();

        #[allow(unreachable_code)]
        Ok(SharedFd {
            inner: Arc::new(Inner {
                fd,
                state: UnsafeCell::new(state),
                worker_id: current_worker_id().expect("not on worker"),
                #[cfg(feature = "poll")]
                scheduled_io,
            }),
        })
    }

    #[cfg(windows)]
    pub(crate) fn new<const FORCE_POLL: bool>(fd: RawSocket) -> io::Result<SharedFd> {
        use crate::current_worker_id;

        const RW_INTERESTS: mio::Interest = mio::Interest::READABLE.add(mio::Interest::WRITABLE);

        let scheduled_io = std::sync::Arc::new(crate::monoio::driver::scheduled_io::ScheduledIo::new());
        
        let state = {
            // Create a temporary mio::net::TcpStream for registration
            let std_stream = unsafe { std::net::TcpStream::from_raw_socket(fd) };
            std_stream.set_nonblocking(true)?;
            let mut mio_stream = mio::net::TcpStream::from_std(std_stream);
            
            let reg = CURRENT.with(|inner| match inner {
                super::Inner::Poller(inner) => {
                    super::poller::PollerDriver::register(inner, &mut mio_stream, RW_INTERESTS, &scheduled_io)
                }
            });
            
            // Extract raw socket back and forget the wrapper
            std::mem::forget(mio_stream);

            State::Poller(Some(reg?))
        };

        #[allow(unreachable_code)]
        Ok(SharedFd {
            inner: Arc::new(Inner {
                fd,
                state: UnsafeCell::new(state),
                worker_id: current_worker_id().expect("not on worker"),
                scheduled_io,
            }),
        })
    }

    #[cfg(unix)]
    #[allow(unreachable_code, unused)]
    pub(crate) fn new_without_register(fd: RawFd) -> SharedFd {
        let state = CURRENT.with(|inner| match inner {
            #[cfg(all(target_os = "linux", feature = "iouring"))]
            super::Inner::Uring(_) => State::Uring(UringState::Init),
            #[cfg(feature = "poll")]
            super::Inner::Poller(_) => State::Poller(None),
            #[cfg(all(
                not(feature = "poll"),
                not(all(target_os = "linux", feature = "iouring"))
            ))]
            _ => {
                super::util::feature_panic();
            }
        });

        SharedFd {
            inner: Arc::new(Inner {
                fd,
                state: UnsafeCell::new(state),
                worker_id: current_worker_id().expect("not on worker"),
                #[cfg(feature = "poll")]
                scheduled_io: std::sync::Arc::new(crate::monoio::driver::scheduled_io::ScheduledIo::new()),
            }),
        }
    }

    #[cfg(windows)]
    #[allow(unreachable_code, unused)]
    pub(crate) fn new_without_register(fd: RawSocket) -> SharedFd {
        use crate::current_worker_id;

        let state = CURRENT.with(|inner| match inner {
            super::Inner::Poller(_) => State::Poller(None),
        });

        SharedFd {
            inner: Arc::new(Inner {
                fd,
                state: UnsafeCell::new(state),
                worker_id: current_worker_id().expect("not on worker"),
                scheduled_io: std::sync::Arc::new(crate::monoio::driver::scheduled_io::ScheduledIo::new()),
            }),
        }
    }

    #[cfg(unix)]
    /// Returns the RawFd
    pub fn raw_fd(&self) -> RawFd {
        self.inner.fd
    }

    #[cfg(windows)]
    /// Returns the RawSocket
    pub fn raw_socket(&self) -> RawSocket {
        self.inner.fd
    }

    /// Get the reader waker for this fd
    #[cfg(feature = "poll")]
    #[inline]
    pub(crate) fn reader_waker(&self) -> &crate::future::waker::DiatomicWaker {
        &self.inner.scheduled_io.reader
    }

    /// Get the writer waker for this fd
    #[cfg(feature = "poll")]
    #[inline]
    pub(crate) fn writer_waker(&self) -> &crate::future::waker::DiatomicWaker {
        &self.inner.scheduled_io.writer
    }
    
    /// Get the scheduled_io for this fd (for ops that need readiness)
    #[cfg(feature = "poll")]
    #[inline]
    pub(crate) fn scheduled_io(&self) -> &std::sync::Arc<crate::monoio::driver::scheduled_io::ScheduledIo> {
        &self.inner.scheduled_io
    }

    /// Check if this fd is remote (owned by a different worker)
    /// On poller platforms, we need to check if the fd is registered with a different worker's epoll
    // #[cfg(feature = "poll")]
    #[inline]
    pub(crate) fn is_remote(&self) -> bool {
        // Check if the fd's worker_id differs from the current worker
        use crate::current_worker_id;
        self.inner.worker_id != current_worker_id().unwrap_or(u32::MAX)
    }

    #[cfg(windows)]
    pub fn raw_handle(&self) -> RawHandle {
        self.inner.fd as _
    }

    #[cfg(unix)]
    /// Try unwrap Rc, then deregister if registered and return rawfd.
    /// Note: this action will consume self and return rawfd without closing it.
    pub(crate) fn try_unwrap(self) -> Result<RawFd, Self> {
        use std::mem::{ManuallyDrop, MaybeUninit};

        let fd = self.inner.fd;
        match Arc::try_unwrap(self.inner) {
            Ok(inner) => {
                // Only drop Inner's state, skip its drop impl.
                let mut inner_skip_drop = ManuallyDrop::new(inner);
                #[allow(invalid_value)]
                #[allow(clippy::uninit_assumed_init)]
                let mut state = unsafe { MaybeUninit::uninit().assume_init() };
                std::mem::swap(&mut inner_skip_drop.state, &mut state);

                #[cfg(feature = "poll")]
                let state = unsafe { &*state.get() };

                #[cfg(feature = "poll")]
                #[allow(irrefutable_let_patterns)]
                if let State::Poller(idx) = state {
                    if CURRENT.is_set() {
                        CURRENT.with(|inner| {
                            match inner {
                                #[cfg(all(target_os = "linux", feature = "iouring"))]
                                super::Inner::Uring(_) => {
                                    unreachable!("try_unwrap poller fd with uring runtime")
                                }
                                super::Inner::Poller(inner) => {
                                    // deregister it from driver(Poll and slab) and close fd
                                    if let Some(idx) = idx {
                                        let mut source = mio::unix::SourceFd(&fd);
                                        let _ = super::poller::PollerDriver::deregister(
                                            inner,
                                            *idx,
                                            &mut source,
                                        );
                                    }
                                }
                            }
                        })
                    }
                }
                Ok(fd)
            }
            Err(inner) => Err(Self { inner }),
        }
    }

    #[cfg(windows)]
    /// Try unwrap Rc, then deregister if registered and return rawfd.
    /// Note: this action will consume self and return rawfd without closing it.
    pub(crate) fn try_unwrap(self) -> Result<RawSocket, Self> {
        match Arc::try_unwrap(self.inner) {
            Ok(_inner) => {
                let fd = _inner.fd;
                let state = unsafe { &*_inner.state.get() };

                #[allow(irrefutable_let_patterns)]
                if let State::Poller(idx) = state {
                    if CURRENT.is_set() {
                        CURRENT.with(|inner| {
                            match inner {
                                super::Inner::Poller(inner) => {
                                    // deregister it from driver(Poll and slab)
                                    if let Some(idx) = idx {
                                        let std_stream = unsafe { std::net::TcpStream::from_raw_socket(fd) };
                                        let mut mio_stream = mio::net::TcpStream::from_std(std_stream);
                                        let _ = super::poller::PollerDriver::deregister(
                                            inner, *idx, &mut mio_stream,
                                        );
                                        std::mem::forget(mio_stream);
                                    }
                                }
                            }
                        })
                    }
                }
                Ok(fd)
            }
            Err(inner) => Err(Self { inner }),
        }
    }

    #[allow(unused)]
    pub fn registered_index(&self) -> Option<usize> {
        let state = unsafe { &*self.inner.state.get() };
        match state {
            #[cfg(all(target_os = "linux", feature = "iouring", feature = "poll-io"))]
            State::Uring(UringState::Poller(s)) => *s,
            #[cfg(all(target_os = "linux", feature = "iouring"))]
            State::Uring(_) => None,
            #[cfg(feature = "poll")]
            State::Poller(s) => *s,
            #[cfg(all(
                not(feature = "poll"),
                not(all(target_os = "linux", feature = "iouring"))
            ))]
            _ => {
                super::util::feature_panic();
            }
        }
    }

    /// An FD cannot be closed until all in-flight operation have completed.
    /// This prevents bugs where in-flight reads could operate on the incorrect
    /// file descriptor.
    pub(crate) async fn close(self) {
        // Here we only submit close op for uring mode.
        // Fd will be closed when Inner drops for poller mode.
        #[cfg(all(target_os = "linux", feature = "iouring"))]
        {
            let fd = self.inner.fd;
            let mut this = self;
            #[allow(irrefutable_let_patterns)]
            if let State::Uring(uring_state) = unsafe { &mut *this.inner.state.get() } {
                if Arc::get_mut(&mut this.inner).is_some() {
                    *uring_state = match super::op::Op::close(fd) {
                        Ok(op) => UringState::Closing(op),
                        Err(_) => {
                            let _ = unsafe { std::fs::File::from_raw_fd(fd) };
                            return;
                        }
                    };
                }
                this.inner.closed().await;
            }
        }
    }

    #[cfg(feature = "poll-io")]
    #[inline]
    pub(crate) fn cvt_poll(&mut self) -> io::Result<()> {
        let state = unsafe { &mut *self.inner.state.get() };
        #[cfg(unix)]
        let r = state.cvt_uring_poll(self.inner.fd);
        #[cfg(windows)]
        let r = Ok(());
        r
    }

    #[cfg(feature = "poll-io")]
    #[inline]
    pub(crate) fn cvt_comp(&mut self) -> io::Result<()> {
        let state = unsafe { &mut *self.inner.state.get() };
        #[cfg(unix)]
        let r = state.cvt_comp(self.inner.fd);
        #[cfg(windows)]
        let r = Ok(());
        r
    }
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
impl Inner {
    /// Completes when the FD has been closed.
    /// Should only be called for uring mode.
    async fn closed(&self) {
        use std::task::Poll;

        crate::monoio::macros::support::poll_fn(|cx| {
            let state = unsafe { &mut *self.state.get() };

            #[allow(irrefutable_let_patterns)]
            if let State::Uring(uring_state) = state {
                use std::{future::Future, pin::Pin};

                return match uring_state {
                    UringState::Init => {
                        *uring_state = UringState::Waiting(Some(cx.waker().clone()));
                        Poll::Pending
                    }
                    UringState::Waiting(Some(waker)) => {
                        if !waker.will_wake(cx.waker()) {
                            waker.clone_from(cx.waker());
                        }

                        Poll::Pending
                    }
                    UringState::Waiting(None) => {
                        *uring_state = UringState::Waiting(Some(cx.waker().clone()));
                        Poll::Pending
                    }
                    UringState::Closing(op) => {
                        // Nothing to do if the close operation failed.
                        let _ = ready!(Pin::new(op).poll(cx));
                        *uring_state = UringState::Closed;
                        Poll::Ready(())
                    }
                    UringState::Closed => Poll::Ready(()),
                    #[cfg(feature = "poll-io")]
                    UringState::Poller(_) => Poll::Ready(()),
                };
            }
            Poll::Ready(())
        })
        .await;
    }
}

#[cfg(unix)]
impl Drop for Inner {
    fn drop(&mut self) {
        let fd = self.fd;
        let state = unsafe { &mut *self.state.get() };
        #[allow(unreachable_patterns)]
        match state {
            #[cfg(all(target_os = "linux", feature = "iouring"))]
            State::Uring(UringState::Init) | State::Uring(UringState::Waiting(..)) => {
                if super::op::Op::close(fd).is_err() {
                    let _ = unsafe { std::fs::File::from_raw_fd(fd) };
                };
            }
            #[cfg(feature = "poll")]
            State::Poller(idx) => drop_poller(fd, *idx),
            #[cfg(all(target_os = "linux", feature = "iouring", feature = "poll-io"))]
            State::Uring(UringState::Poller(idx)) => drop_uring_poller(fd, *idx),
            _ => {}
        }
    }
}

#[allow(unused_mut)]
#[cfg(feature = "poll")]
fn drop_poller(mut fd: RawFd, idx: Option<usize>) {
    if CURRENT.is_set() {
        CURRENT.with(|inner| {
            #[cfg(any(all(target_os = "linux", feature = "iouring"), feature = "poll"))]
            match inner {
                #[cfg(all(target_os = "linux", feature = "iouring"))]
                super::Inner::Uring(_) => {
                    unreachable!("close poller fd with uring runtime")
                }
                super::Inner::Poller(inner) => {
                    // deregister it from driver(Poll and slab) and close fd
                    #[cfg(not(windows))]
                    if let Some(idx) = idx {
                        let mut source = mio::unix::SourceFd(&fd);
                        let _ = super::poller::PollerDriver::deregister(inner, idx, &mut source);
                    }
                    #[cfg(windows)]
                    if let Some(idx) = idx {
                        let std_stream = unsafe { std::net::TcpStream::from_raw_socket(fd) };
                        let mut mio_stream = mio::net::TcpStream::from_std(std_stream);
                        let _ = super::poller::PollerDriver::deregister(inner, idx, &mut mio_stream);
                        std::mem::forget(mio_stream);
                    }
                }
            }
        })
    }
    #[cfg(all(unix, feature = "poll"))]
    let _ = unsafe { std::fs::File::from_raw_fd(fd) };
    #[cfg(all(windows, feature = "poll"))]
    let _ = unsafe { OwnedSocket::from_raw_socket(fd) };
}

#[cfg(feature = "poll-io")]
fn drop_uring_poller(fd: RawFd, idx: Option<usize>) {
    if CURRENT.is_set() {
        CURRENT.with(|inner| {
            match inner {
                #[cfg(feature = "poll")]
                super::Inner::Poller(_) => {
                    unreachable!("close uring fd with poller runtime")
                }
                #[cfg(all(target_os = "linux", feature = "iouring"))]
                super::Inner::Uring(inner) => {
                    // deregister it from driver(Poll and slab) and close fd
                    if let Some(idx) = idx {
                        let mut source = mio::unix::SourceFd(&fd);
                        let _ = super::IoUringDriver::deregister_poll_io(inner, &mut source, idx);
                    }
                }
            }
        })
    }
    #[cfg(unix)]
    let _ = unsafe { std::fs::File::from_raw_fd(fd) };
    #[cfg(windows)]
    let _ = unsafe { OwnedSocket::from_raw_socket(fd.socket) };
}
