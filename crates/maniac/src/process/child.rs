//! Child process and stdio handles.

use std::future::Future;
use std::io;
use std::process::ExitStatus as StdExitStatus;

use crate::blocking::unblock;
use crate::buf::{IoBuf, IoBufMut, IoVecBuf, IoVecBufMut};
use crate::io::{AsyncReadRent, AsyncWriteRent};
use crate::BufResult;

#[cfg(unix)]
use super::unix::{ChildStderrInner, ChildStdinInner, ChildStdoutInner, WaitFuture};
#[cfg(windows)]
use super::windows::{ChildStderrInner, ChildStdinInner, ChildStdoutInner, WaitFuture};

/// The exit status of a finished process.
///
/// This struct wraps [`std::process::ExitStatus`] and provides
/// the same interface.
#[derive(Clone, Copy, Debug)]
pub struct ExitStatus(StdExitStatus);

impl ExitStatus {
    /// Creates a new `ExitStatus` from a std `ExitStatus`.
    pub(crate) fn new(status: StdExitStatus) -> Self {
        Self(status)
    }

    /// Was termination successful? Signal termination is not considered a success,
    /// and success is defined as a zero exit code.
    pub fn success(&self) -> bool {
        self.0.success()
    }

    /// Returns the exit code of the process, if any.
    ///
    /// On Unix, this will return `None` if the process was terminated by a signal.
    /// On Windows, this will always return `Some`.
    pub fn code(&self) -> Option<i32> {
        self.0.code()
    }

    /// Returns the underlying `std::process::ExitStatus`.
    pub fn into_inner(self) -> StdExitStatus {
        self.0
    }
}

impl From<StdExitStatus> for ExitStatus {
    fn from(status: StdExitStatus) -> Self {
        Self(status)
    }
}

impl std::fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// Unix-specific methods
#[cfg(unix)]
impl ExitStatus {
    /// Creates an `ExitStatus` from the raw underlying integer status value.
    pub fn from_raw(raw: i32) -> Self {
        use std::os::unix::process::ExitStatusExt;
        Self(StdExitStatus::from_raw(raw))
    }

    /// If the process was terminated by a signal, returns that signal.
    pub fn signal(&self) -> Option<i32> {
        use std::os::unix::process::ExitStatusExt;
        self.0.signal()
    }

    /// If the process was terminated by a signal, says whether it dumped core.
    pub fn core_dumped(&self) -> bool {
        use std::os::unix::process::ExitStatusExt;
        self.0.core_dumped()
    }

    /// If the process was stopped by a signal, returns that signal.
    pub fn stopped_signal(&self) -> Option<i32> {
        use std::os::unix::process::ExitStatusExt;
        self.0.stopped_signal()
    }

    /// Whether the process was continued from a stopped status.
    pub fn continued(&self) -> bool {
        use std::os::unix::process::ExitStatusExt;
        self.0.continued()
    }

    /// Returns the underlying raw wait status.
    pub fn into_raw(self) -> i32 {
        use std::os::unix::process::ExitStatusExt;
        self.0.into_raw()
    }
}

/// The output of a finished process.
///
/// This is returned by [`Child::wait_with_output`] or [`Command::output`].
#[derive(Debug)]
pub struct Output {
    /// The status (exit code) of the process.
    pub status: ExitStatus,
    /// The data that the process wrote to stdout.
    pub stdout: Vec<u8>,
    /// The data that the process wrote to stderr.
    pub stderr: Vec<u8>,
}

/// A handle to a child process.
///
/// The child process is not automatically reaped when this handle is dropped.
/// Use [`Child::wait`] to reap the child and obtain its exit status.
///
/// # Examples
///
/// ```no_run
/// use maniac::process::Command;
///
/// # async fn example() -> std::io::Result<()> {
/// let mut child = Command::new("sleep")
///     .arg("5")
///     .spawn()?;
///
/// // Do other work...
///
/// let status = child.wait().await?;
/// println!("Child exited with: {:?}", status);
/// # Ok(())
/// # }
/// ```
pub struct Child {
    /// The child's standard input (stdin) handle, if piped.
    pub stdin: Option<ChildStdin>,
    /// The child's standard output (stdout) handle, if piped.
    pub stdout: Option<ChildStdout>,
    /// The child's standard error (stderr) handle, if piped.
    pub stderr: Option<ChildStderr>,
    /// The underlying child process handle.
    inner: std::process::Child,
}

impl Child {
    /// Creates a new `Child` from a standard library `Child`.
    pub(crate) fn from_std(mut child: std::process::Child) -> io::Result<Self> {
        let stdin = child.stdin.take().map(ChildStdin::from_std).transpose()?;
        let stdout = child.stdout.take().map(ChildStdout::from_std).transpose()?;
        let stderr = child.stderr.take().map(ChildStderr::from_std).transpose()?;

        Ok(Self {
            stdin,
            stdout,
            stderr,
            inner: child,
        })
    }

    /// Returns the OS-assigned process identifier for the child.
    pub fn id(&self) -> u32 {
        self.inner.id()
    }

    /// Forces the child process to exit.
    ///
    /// If the child has already exited, this is a no-op. Note that this
    /// does not wait for the child to exit after killing it.
    ///
    /// On Unix, this sends `SIGKILL`. On Windows, this calls `TerminateProcess`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let mut child = Command::new("sleep")
    ///     .arg("1000")
    ///     .spawn()?;
    ///
    /// child.kill()?;
    /// let status = child.wait().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn kill(&mut self) -> io::Result<()> {
        self.inner.kill()
    }

    /// Attempts to collect the exit status of the child if it has already exited.
    ///
    /// This method will not block the calling thread and will instead return
    /// `Ok(None)` if the child has not yet exited.
    ///
    /// If the child has exited, `Ok(Some(status))` is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// # fn example() -> std::io::Result<()> {
    /// let mut child = Command::new("ls").spawn()?;
    ///
    /// match child.try_wait()? {
    ///     Some(status) => println!("Exited with: {:?}", status),
    ///     None => println!("Still running"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.inner.try_wait().map(|opt| opt.map(ExitStatus::new))
    }

    /// Waits for the child to exit completely, returning the status it exited with.
    ///
    /// This method will consume stdin handle if present, so that the child
    /// doesn't block waiting for input.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let mut child = Command::new("ls").spawn()?;
    /// let status = child.wait().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        // Drop stdin to avoid deadlock where child is waiting for stdin
        drop(self.stdin.take());

        #[cfg(unix)]
        {
            WaitFuture::new(&mut self.inner).await
        }
        #[cfg(windows)]
        {
            WaitFuture::new(&mut self.inner).await
        }
    }

    /// Waits for the child to exit completely, returning the status and all output.
    ///
    /// This will consume the child's stdin, stdout, and stderr handles and
    /// read all output until EOF.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::{Command, Stdio};
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let mut child = Command::new("echo")
    ///     .arg("hello")
    ///     .stdout(Stdio::piped())
    ///     .spawn()?;
    ///
    /// let output = child.wait_with_output().await?;
    /// println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wait_with_output(mut self) -> io::Result<Output> {
        // Drop stdin to signal EOF to child
        drop(self.stdin.take());

        // Take stdout and stderr handles
        let mut stdout_opt = self.stdout.take();
        let mut stderr_opt = self.stderr.take();

        // Helper to read all from a handle
        async fn read_all(handle: &mut Option<impl AsyncReadRent>) -> io::Result<Vec<u8>> {
            let mut buf = Vec::new();
            if let Some(h) = handle {
                loop {
                    let read_buf = vec![0u8; 8192];
                    let (result, returned_buf) = h.read(read_buf).await;
                    match result {
                        Ok(0) => break,
                        Ok(n) => buf.extend_from_slice(&returned_buf[..n]),
                        Err(e) => return Err(e),
                    }
                }
            }
            Ok(buf)
        }

        // Read stdout and stderr sequentially to avoid borrow issues
        let stdout_data = read_all(&mut stdout_opt).await?;
        let stderr_data = read_all(&mut stderr_opt).await?;

        // Wait for the child
        let status = WaitFuture::new(&mut self.inner).await?;

        Ok(Output {
            status,
            stdout: stdout_data,
            stderr: stderr_data,
        })
    }
}

impl std::fmt::Debug for Child {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Child")
            .field("id", &self.id())
            .field("stdin", &self.stdin)
            .field("stdout", &self.stdout)
            .field("stderr", &self.stderr)
            .finish()
    }
}

/// A handle to the standard input (stdin) of a child process.
///
/// This handle implements [`AsyncWriteRent`] for async writing.
pub struct ChildStdin {
    inner: ChildStdinInner,
}

impl ChildStdin {
    /// Creates a new `ChildStdin` from a standard library `ChildStdin`.
    pub(crate) fn from_std(stdin: std::process::ChildStdin) -> io::Result<Self> {
        Ok(Self {
            inner: ChildStdinInner::from_std(stdin)?,
        })
    }
}

impl std::fmt::Debug for ChildStdin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChildStdin").finish()
    }
}

impl AsyncWriteRent for ChildStdin {
    fn write<T: IoBuf>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        self.inner.write(buf)
    }

    fn writev<T: IoVecBuf>(&mut self, buf_vec: T) -> impl Future<Output = BufResult<usize, T>> {
        self.inner.writev(buf_vec)
    }

    fn flush(&mut self) -> impl Future<Output = io::Result<()>> {
        self.inner.flush()
    }

    fn shutdown(&mut self) -> impl Future<Output = io::Result<()>> {
        self.inner.shutdown()
    }
}

/// A handle to the standard output (stdout) of a child process.
///
/// This handle implements [`AsyncReadRent`] for async reading.
pub struct ChildStdout {
    inner: ChildStdoutInner,
}

impl ChildStdout {
    /// Creates a new `ChildStdout` from a standard library `ChildStdout`.
    pub(crate) fn from_std(stdout: std::process::ChildStdout) -> io::Result<Self> {
        Ok(Self {
            inner: ChildStdoutInner::from_std(stdout)?,
        })
    }
}

impl std::fmt::Debug for ChildStdout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChildStdout").finish()
    }
}

impl AsyncReadRent for ChildStdout {
    fn read<T: IoBufMut>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        self.inner.read(buf)
    }

    fn readv<T: IoVecBufMut>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        self.inner.readv(buf)
    }
}

/// A handle to the standard error (stderr) of a child process.
///
/// This handle implements [`AsyncReadRent`] for async reading.
pub struct ChildStderr {
    inner: ChildStderrInner,
}

impl ChildStderr {
    /// Creates a new `ChildStderr` from a standard library `ChildStderr`.
    pub(crate) fn from_std(stderr: std::process::ChildStderr) -> io::Result<Self> {
        Ok(Self {
            inner: ChildStderrInner::from_std(stderr)?,
        })
    }
}

impl std::fmt::Debug for ChildStderr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChildStderr").finish()
    }
}

impl AsyncReadRent for ChildStderr {
    fn read<T: IoBufMut>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        self.inner.read(buf)
    }

    fn readv<T: IoVecBufMut>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        self.inner.readv(buf)
    }
}

