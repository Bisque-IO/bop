//! Windows-specific process implementations.

use std::future::Future;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, RawHandle};
use std::pin::Pin;
use std::process::ExitStatus as StdExitStatus;
use std::task::{Context, Poll};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF, GetLastError, HANDLE, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};

use crate::BufResult;
use crate::buf::{IoBuf, IoBufMut, IoVecBuf, IoVecBufMut};

use super::child::ExitStatus;

/// Windows implementation of ChildStdin
pub(crate) struct ChildStdinInner {
    handle: RawHandle,
}

// SAFETY: Windows handles are safe to send between threads
unsafe impl Send for ChildStdinInner {}
unsafe impl Sync for ChildStdinInner {}

impl ChildStdinInner {
    pub(crate) fn from_std(stdin: std::process::ChildStdin) -> io::Result<Self> {
        let handle = stdin.into_raw_handle();
        Ok(Self { handle })
    }

    pub(crate) fn write<T: IoBuf>(&mut self, buf: T) -> impl Future<Output = BufResult<usize, T>> {
        let handle = self.handle as HANDLE;

        async move {
            // Synchronous write - pipes typically don't block for writes
            let mut bytes_written: u32 = 0;
            let success = unsafe {
                WriteFile(
                    handle,
                    buf.read_ptr() as *const _,
                    buf.bytes_init() as u32,
                    &mut bytes_written,
                    std::ptr::null_mut(),
                )
            };

            if success == 0 {
                let err = unsafe { GetLastError() };
                (Err(io::Error::from_raw_os_error(err as i32)), buf)
            } else {
                (Ok(bytes_written as usize), buf)
            }
        }
    }

    pub(crate) fn writev<T: IoVecBuf>(
        &mut self,
        buf_vec: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        let handle = self.handle as HANDLE;

        async move {
            let wsabuf_len = buf_vec.read_wsabuf_len();
            if wsabuf_len == 0 {
                return (Ok(0), buf_vec);
            }

            let wsabuf_ptr = buf_vec.read_wsabuf_ptr();
            if wsabuf_ptr.is_null() {
                return (Ok(0), buf_vec);
            }

            let wsabuf = unsafe { &*wsabuf_ptr };
            let mut bytes_written: u32 = 0;
            let success = unsafe {
                WriteFile(
                    handle,
                    wsabuf.buf as *const _,
                    wsabuf.len as u32,
                    &mut bytes_written,
                    std::ptr::null_mut(),
                )
            };

            if success == 0 {
                let err = unsafe { GetLastError() };
                (Err(io::Error::from_raw_os_error(err as i32)), buf_vec)
            } else {
                (Ok(bytes_written as usize), buf_vec)
            }
        }
    }

    pub(crate) fn flush(&mut self) -> impl Future<Output = io::Result<()>> {
        std::future::ready(Ok(()))
    }

    pub(crate) fn shutdown(&mut self) -> impl Future<Output = io::Result<()>> {
        std::future::ready(Ok(()))
    }
}

impl AsRawHandle for ChildStdinInner {
    fn as_raw_handle(&self) -> RawHandle {
        self.handle
    }
}

impl Drop for ChildStdinInner {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { CloseHandle(self.handle as HANDLE) };
        }
    }
}

/// Windows implementation of ChildStdout - uses polling for non-blocking reads
pub(crate) struct ChildStdoutInner {
    handle: RawHandle,
}

unsafe impl Send for ChildStdoutInner {}
unsafe impl Sync for ChildStdoutInner {}

impl ChildStdoutInner {
    pub(crate) fn from_std(stdout: std::process::ChildStdout) -> io::Result<Self> {
        let handle = stdout.into_raw_handle();
        Ok(Self { handle })
    }

    pub(crate) fn read<T: IoBufMut>(
        &mut self,
        buf: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        PipeReadFuture {
            handle: self.handle as usize,
            buf: Some(buf),
        }
    }

    pub(crate) fn readv<T: IoVecBufMut>(
        &mut self,
        buf: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        PipeReadVecFuture {
            handle: self.handle as usize,
            buf: Some(buf),
        }
    }
}

impl AsRawHandle for ChildStdoutInner {
    fn as_raw_handle(&self) -> RawHandle {
        self.handle
    }
}

impl Drop for ChildStdoutInner {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { CloseHandle(self.handle as HANDLE) };
        }
    }
}

/// Windows implementation of ChildStderr
pub(crate) struct ChildStderrInner {
    handle: RawHandle,
}

unsafe impl Send for ChildStderrInner {}
unsafe impl Sync for ChildStderrInner {}

impl ChildStderrInner {
    pub(crate) fn from_std(stderr: std::process::ChildStderr) -> io::Result<Self> {
        let handle = stderr.into_raw_handle();
        Ok(Self { handle })
    }

    pub(crate) fn read<T: IoBufMut>(
        &mut self,
        buf: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        PipeReadFuture {
            handle: self.handle as usize,
            buf: Some(buf),
        }
    }

    pub(crate) fn readv<T: IoVecBufMut>(
        &mut self,
        buf: T,
    ) -> impl Future<Output = BufResult<usize, T>> {
        PipeReadVecFuture {
            handle: self.handle as usize,
            buf: Some(buf),
        }
    }
}

impl AsRawHandle for ChildStderrInner {
    fn as_raw_handle(&self) -> RawHandle {
        self.handle
    }
}

impl Drop for ChildStderrInner {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { CloseHandle(self.handle as HANDLE) };
        }
    }
}

/// Future for reading from a pipe with polling
struct PipeReadFuture<T> {
    handle: usize, // Store as usize for Send safety
    buf: Option<T>,
}

// SAFETY: We store the handle as usize and only use it from the polling thread
unsafe impl<T: Send> Send for PipeReadFuture<T> {}

impl<T: IoBufMut> Future for PipeReadFuture<T> {
    type Output = BufResult<usize, T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let handle = self.handle as HANDLE;
        let buf = self.buf.as_mut().unwrap();
        let ptr = buf.write_ptr();
        let len = buf.bytes_total();

        // Check if there's data available using PeekNamedPipe
        let mut bytes_available: u32 = 0;
        let peek_result = unsafe {
            PeekNamedPipe(
                handle,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut bytes_available,
                std::ptr::null_mut(),
            )
        };

        if peek_result == 0 {
            let err = unsafe { GetLastError() };
            // Pipe closed - return EOF
            if err == ERROR_BROKEN_PIPE {
                return Poll::Ready((Ok(0), self.buf.take().unwrap()));
            }
            return Poll::Ready((
                Err(io::Error::from_raw_os_error(err as i32)),
                self.buf.take().unwrap(),
            ));
        }

        if bytes_available == 0 {
            // No data available - reschedule
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        // Data available - read it
        let mut bytes_read: u32 = 0;
        let read_result = unsafe {
            ReadFile(
                handle,
                ptr as *mut _,
                len.min(bytes_available as usize) as u32,
                &mut bytes_read,
                std::ptr::null_mut(),
            )
        };

        let mut buf = self.buf.take().unwrap();

        if read_result == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_HANDLE_EOF || err == ERROR_BROKEN_PIPE {
                Poll::Ready((Ok(0), buf))
            } else {
                Poll::Ready((Err(io::Error::from_raw_os_error(err as i32)), buf))
            }
        } else {
            unsafe { buf.set_init(bytes_read as usize) };
            Poll::Ready((Ok(bytes_read as usize), buf))
        }
    }
}

/// Future for vectored reading from a pipe
struct PipeReadVecFuture<T> {
    handle: usize, // Store as usize for Send safety
    buf: Option<T>,
}

// SAFETY: We store the handle as usize and only use it from the polling thread
unsafe impl<T: Send> Send for PipeReadVecFuture<T> {}

impl<T: IoVecBufMut> Future for PipeReadVecFuture<T> {
    type Output = BufResult<usize, T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let handle = self.handle as HANDLE;
        let buf = self.buf.as_mut().unwrap();
        let wsabuf_len = buf.write_wsabuf_len();

        if wsabuf_len == 0 {
            return Poll::Ready((Ok(0), self.buf.take().unwrap()));
        }

        let wsabuf_ptr = buf.write_wsabuf_ptr();
        if wsabuf_ptr.is_null() {
            return Poll::Ready((Ok(0), self.buf.take().unwrap()));
        }

        let wsabuf = unsafe { &*wsabuf_ptr };
        let ptr = wsabuf.buf;
        let len = wsabuf.len as usize;

        // Check if there's data available
        let mut bytes_available: u32 = 0;
        let peek_result = unsafe {
            PeekNamedPipe(
                handle,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut bytes_available,
                std::ptr::null_mut(),
            )
        };

        if peek_result == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_BROKEN_PIPE {
                return Poll::Ready((Ok(0), self.buf.take().unwrap()));
            }
            return Poll::Ready((
                Err(io::Error::from_raw_os_error(err as i32)),
                self.buf.take().unwrap(),
            ));
        }

        if bytes_available == 0 {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        let mut bytes_read: u32 = 0;
        let read_result = unsafe {
            ReadFile(
                handle,
                ptr as *mut _,
                len.min(bytes_available as usize) as u32,
                &mut bytes_read,
                std::ptr::null_mut(),
            )
        };

        let mut buf = self.buf.take().unwrap();

        if read_result == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_HANDLE_EOF || err == ERROR_BROKEN_PIPE {
                Poll::Ready((Ok(0), buf))
            } else {
                Poll::Ready((Err(io::Error::from_raw_os_error(err as i32)), buf))
            }
        } else {
            unsafe { buf.set_init(bytes_read as usize) };
            Poll::Ready((Ok(bytes_read as usize), buf))
        }
    }
}

/// Future that waits for a child process to exit on Windows.
pub(crate) struct WaitFuture<'a> {
    child: &'a mut std::process::Child,
}

impl<'a> WaitFuture<'a> {
    pub(crate) fn new(child: &'a mut std::process::Child) -> Self {
        Self { child }
    }
}

impl<'a> Future for WaitFuture<'a> {
    type Output = io::Result<ExitStatus>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // First, try a non-blocking wait
        match self.child.try_wait() {
            Ok(Some(status)) => {
                return Poll::Ready(Ok(ExitStatus::new(status)));
            }
            Ok(None) => {
                let handle = self.child.as_raw_handle() as HANDLE;

                // Wait with 0 timeout
                let wait_result = unsafe { WaitForSingleObject(handle, 0) };

                if wait_result == WAIT_OBJECT_0 {
                    let mut exit_code: u32 = 0;
                    let success = unsafe { GetExitCodeProcess(handle, &mut exit_code) };

                    if success == 0 {
                        let err = unsafe { GetLastError() };
                        return Poll::Ready(Err(io::Error::from_raw_os_error(err as i32)));
                    }

                    let status = std::os::windows::process::ExitStatusExt::from_raw(exit_code);
                    return Poll::Ready(Ok(ExitStatus::new(status)));
                } else if wait_result == WAIT_TIMEOUT {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                } else {
                    let err = unsafe { GetLastError() };
                    return Poll::Ready(Err(io::Error::from_raw_os_error(err as i32)));
                }
            }
            Err(e) => return Poll::Ready(Err(e)),
        }
    }
}
