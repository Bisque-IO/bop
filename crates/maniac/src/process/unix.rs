//! Unix-specific process implementations.

use std::future::Future;
use std::io;
use std::os::unix::io::{AsRawFd, IntoRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::BufResult;
use crate::buf::{IoBuf, IoBufMut, IoVecBuf, IoVecBufMut};
use crate::driver::op::Op;
use crate::driver::shared_fd::SharedFd;

use super::child::ExitStatus;
use super::child_reaper::WaitHandle;

/// Set a file descriptor to non-blocking mode
fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    let result = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Unix implementation of ChildStdin
pub(crate) struct ChildStdinInner {
    fd: SharedFd,
}

impl ChildStdinInner {
    pub(crate) fn from_std(stdin: std::process::ChildStdin) -> io::Result<Self> {
        let raw_fd = stdin.into_raw_fd();

        // Set non-blocking for poll-based I/O
        #[cfg(feature = "poll")]
        set_nonblocking(raw_fd)?;

        // Register with the I/O driver
        let fd = SharedFd::new::<false>(raw_fd)?;
        Ok(Self { fd })
    }

    pub(crate) fn write<T: IoBuf>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        let op = Op::write(self.fd.clone(), buf);
        async move {
            match op {
                Ok(op) => op.result().await,
                Err(e) => {
                    // We need to return a buffer, create empty one
                    // This is a bit awkward but matches the API
                    panic!("Failed to create write op: {}", e);
                }
            }
        }
    }

    pub(crate) fn writev<T: IoVecBuf>(
        &mut self,
        buf_vec: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        let op = Op::writev(self.fd.clone(), buf_vec);
        async move {
            match op {
                Ok(op) => op.result().await,
                Err(e) => {
                    panic!("Failed to create writev op: {}", e);
                }
            }
        }
    }

    pub(crate) fn flush(&mut self) -> impl Future<Output = io::Result<()>> {
        // Pipes don't need flushing
        std::future::ready(Ok(()))
    }

    pub(crate) fn shutdown(&mut self) -> impl Future<Output = io::Result<()>> {
        // For pipes, shutdown means closing the write end
        // We don't actually close here as drop handles it
        std::future::ready(Ok(()))
    }
}

impl AsRawFd for ChildStdinInner {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}

/// Unix implementation of ChildStdout
pub(crate) struct ChildStdoutInner {
    fd: SharedFd,
}

impl ChildStdoutInner {
    pub(crate) fn from_std(stdout: std::process::ChildStdout) -> io::Result<Self> {
        let raw_fd = stdout.into_raw_fd();

        // Set non-blocking for poll-based I/O
        #[cfg(feature = "poll")]
        set_nonblocking(raw_fd)?;

        // Register with the I/O driver
        let fd = SharedFd::new::<false>(raw_fd)?;
        Ok(Self { fd })
    }

    pub(crate) fn read<T: IoBufMut>(
        &mut self,
        buf: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        let op = Op::read(self.fd.clone(), buf);
        async move {
            match op {
                Ok(op) => op.result().await,
                Err(e) => {
                    panic!("Failed to create read op: {}", e);
                }
            }
        }
    }

    pub(crate) fn readv<T: IoVecBufMut>(
        &mut self,
        buf: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        let op = Op::readv(self.fd.clone(), buf);
        async move {
            match op {
                Ok(op) => op.result().await,
                Err(e) => {
                    panic!("Failed to create readv op: {}", e);
                }
            }
        }
    }
}

impl AsRawFd for ChildStdoutInner {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}

/// Unix implementation of ChildStderr
pub(crate) struct ChildStderrInner {
    fd: SharedFd,
}

impl ChildStderrInner {
    pub(crate) fn from_std(stderr: std::process::ChildStderr) -> io::Result<Self> {
        let raw_fd = stderr.into_raw_fd();

        // Set non-blocking for poll-based I/O
        #[cfg(feature = "poll")]
        set_nonblocking(raw_fd)?;

        // Register with the I/O driver
        let fd = SharedFd::new::<false>(raw_fd)?;
        Ok(Self { fd })
    }

    pub(crate) fn read<T: IoBufMut>(
        &mut self,
        buf: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        let op = Op::read(self.fd.clone(), buf);
        async move {
            match op {
                Ok(op) => op.result().await,
                Err(e) => {
                    panic!("Failed to create read op: {}", e);
                }
            }
        }
    }

    pub(crate) fn readv<T: IoVecBufMut>(
        &mut self,
        buf: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        let op = Op::readv(self.fd.clone(), buf);
        async move {
            match op {
                Ok(op) => op.result().await,
                Err(e) => {
                    panic!("Failed to create readv op: {}", e);
                }
            }
        }
    }
}

impl AsRawFd for ChildStderrInner {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}

/// Future that waits for a child process to exit.
///
/// Uses SIGCHLD signals for efficient, non-blocking waits.
/// This properly suspends the task until the child process exits,
/// without busy-waiting or spinning.
pub(crate) struct WaitFuture<'a> {
    /// The child process to wait on.
    child: &'a mut std::process::Child,
    /// Handle for SIGCHLD registration.
    wait_handle: Option<WaitHandle>,
    /// Tracks initialization state.
    state: WaitState,
}

/// State machine for WaitFuture.
enum WaitState {
    /// Initial state - need to create WaitHandle
    Init,
    /// Waiting for child to exit
    Waiting,
    /// An error occurred during initialization
    Error(Option<io::Error>),
}

impl<'a> WaitFuture<'a> {
    pub(crate) fn new(child: &'a mut std::process::Child) -> Self {
        Self {
            child,
            wait_handle: None,
            state: WaitState::Init,
        }
    }
}

impl<'a> Future for WaitFuture<'a> {
    type Output = io::Result<ExitStatus>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match &mut self.state {
                WaitState::Init => {
                    // Initialize the wait handle (registers SIGCHLD handler)
                    match WaitHandle::new() {
                        Ok(handle) => {
                            self.wait_handle = Some(handle);
                            self.state = WaitState::Waiting;
                            // Continue to Waiting state
                        }
                        Err(e) => {
                            self.state = WaitState::Error(Some(e));
                            // Continue to Error state
                        }
                    }
                }
                WaitState::Waiting => {
                    // First, try a non-blocking wait
                    match self.child.try_wait() {
                        Ok(Some(status)) => {
                            return Poll::Ready(Ok(ExitStatus::new(status)));
                        }
                        Ok(None) => {
                            // Child still running
                            let handle = self.wait_handle.as_mut().unwrap();

                            // Check if SIGCHLD was received since last poll
                            if handle.poll_sigchld() {
                                // SIGCHLD received - loop back to try_wait
                                continue;
                            }

                            // No pending SIGCHLD - register waker and wait
                            handle.register_waker(cx.waker());

                            // Double-check after registration to avoid race condition:
                            // SIGCHLD could have arrived between poll_sigchld and register_waker
                            if handle.poll_sigchld() {
                                continue;
                            }

                            // Also do a final try_wait to catch any zombies
                            match self.child.try_wait() {
                                Ok(Some(status)) => {
                                    return Poll::Ready(Ok(ExitStatus::new(status)));
                                }
                                Ok(None) => {
                                    // Child still running - wait for SIGCHLD
                                    return Poll::Pending;
                                }
                                Err(e) => {
                                    return Poll::Ready(Err(e));
                                }
                            }
                        }
                        Err(e) => {
                            return Poll::Ready(Err(e));
                        }
                    }
                }
                WaitState::Error(err) => {
                    return Poll::Ready(Err(err
                        .take()
                        .unwrap_or_else(|| io::Error::new(io::ErrorKind::Other, "init failed"))));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn test_set_nonblocking() {
        // Create a pipe to test with
        let mut fds = [0i32; 2];
        unsafe {
            assert_eq!(libc::pipe(fds.as_mut_ptr()), 0);
        }

        // Test setting non-blocking
        assert!(set_nonblocking(fds[0]).is_ok());
        assert!(set_nonblocking(fds[1]).is_ok());

        // Verify the flag is set
        let flags = unsafe { libc::fcntl(fds[0], libc::F_GETFL) };
        assert!(flags & libc::O_NONBLOCK != 0);

        // Clean up
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }
}
