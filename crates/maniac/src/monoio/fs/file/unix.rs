use std::{
    fs::File as StdFile,
    io,
    os::fd::{AsRawFd, IntoRawFd, RawFd},
    path::Path,
};

#[cfg(not(target_os = "linux"))]
use std::os::unix::fs::FileExt;

use super::File;
use crate::monoio::{
    buf::{IoBuf, IoBufMut, IoVecBuf, IoVecBufMut},
    driver::{op::Op, shared_fd::SharedFd},
    fs::{Metadata, metadata::FileAttr},
};
use crate::uring_op;

impl File {
    /// Converts a [`std::fs::File`] to a [`monoio::fs::File`](File).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // This line could block. It is not recommended to do this on the monoio
    /// // runtime.
    /// let std_file = std::fs::File::open("foo.txt").unwrap();
    /// let file = monoio::fs::File::from_std(std_file);
    /// ```
    pub fn from_std(std: StdFile) -> std::io::Result<File> {
        Ok(File {
            fd: SharedFd::new_without_register(std.into_raw_fd()),
        })
    }

    /// Queries metadata about the underlying file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use monoio::fs::File;
    ///
    /// #[monoio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let mut f = File::open("foo.txt").await?;
    ///     let metadata = f.metadata().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn metadata(&self) -> io::Result<Metadata> {
        #[cfg(target_os = "linux")]
        {
            metadata(self.fd.clone()).await
        }
        #[cfg(not(target_os = "linux"))]
        {
            // On non-Linux Unix (macOS/BSD), we use blocking thread pool for metadata
            let fd = self.fd.raw_fd();
            // Zero-allocation version using the dedicated unblock_fmetadata function
            // Safety: File descriptor remains valid for the duration of the operation
            unsafe { crate::monoio::blocking::unblock_fmetadata(fd).await? }
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn open(path: impl AsRef<Path>) -> io::Result<File> {
        let path = path.as_ref().to_owned();
        let std_file = crate::monoio::blocking::unblock_open(path).await?;
        File::from_std(std_file)
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn create(path: impl AsRef<Path>) -> io::Result<File> {
        let path = path.as_ref().to_owned();
        let std_file = crate::monoio::blocking::unblock_create(path).await?;
        File::from_std(std_file)
    }
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}

pub(crate) async fn metadata(fd: SharedFd) -> std::io::Result<Metadata> {
    #[cfg(target_os = "linux")]
    {
        let flags = libc::AT_STATX_SYNC_AS_STAT | libc::AT_EMPTY_PATH;
        let op = Op::statx_using_fd(fd, flags)?;
        op.result().await.map(FileAttr::from).map(Metadata)
    }
    #[cfg(not(target_os = "linux"))]
    {
        // This fallback is handled inside File::metadata for non-linux now
        // But if called directly with SharedFd:
        let raw_fd = fd.raw_fd();
        // Zero-allocation version using the dedicated unblock_fmetadata function
        // Safety: File descriptor remains valid for the duration of the operation
        unsafe { crate::monoio::blocking::unblock_fmetadata(raw_fd).await? }
    }
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;

    uring_op!(read<IoBufMut>(read, buf));
    uring_op!(read_at<IoBufMut>(read_at, buf, pos: u64));
    uring_op!(read_vectored<IoVecBufMut>(readv, buf_vec));

    uring_op!(write<IoBuf>(write, buf));
    uring_op!(write_at<IoBuf>(write_at, buf, pos: u64));
    uring_op!(write_vectored<IoVecBuf>(writev, buf_vec));
}

#[cfg(target_os = "linux")]
pub(crate) use linux_impl::*;

#[cfg(not(target_os = "linux"))]
mod fallback_impl {
    use super::*;

    pub(crate) async fn read<T: IoBufMut>(
        fd: SharedFd,
        mut buf: T,
    ) -> crate::monoio::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Using zero-allocation variant with raw pointers
        let ptr = buf.write_ptr();
        let len = buf.bytes_total();

        // Safety:
        // 1. buf is owned by this async function and won't move
        // 2. The blocking thread only accesses the pointer during the read
        // 3. We wait for completion before returning
        let result = unsafe { crate::monoio::blocking::unblock_fread(raw_fd, ptr, len).await };

        match result {
            Ok(n) => {
                unsafe { buf.set_init(n) };
                (Ok(n), buf)
            }
            Err(e) => (Err(e), buf),
        }
    }

    pub(crate) async fn read_at<T: IoBufMut>(
        fd: SharedFd,
        mut buf: T,
        pos: u64,
    ) -> crate::monoio::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Using zero-allocation variant with raw pointers
        let ptr = buf.write_ptr();
        let len = buf.bytes_total();

        // Safety:
        // 1. buf is owned by this async function and won't move
        // 2. The blocking thread only accesses the pointer during the read_at
        // 3. We wait for completion before returning
        let result =
            unsafe { crate::monoio::blocking::unblock_fread_at(raw_fd, ptr, len, pos).await };

        match result {
            Ok(n) => {
                unsafe { buf.set_init(n) };
                (Ok(n), buf)
            }
            Err(e) => (Err(e), buf),
        }
    }

    pub(crate) async fn write<T: IoBuf>(
        fd: SharedFd,
        buf: T,
    ) -> crate::monoio::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Using zero-allocation variant with raw pointers
        let ptr = buf.read_ptr();
        let len = buf.bytes_init();

        // Safety:
        // 1. buf is immutable during write and owned by this function
        // 2. The blocking thread only accesses the pointer during the write
        // 3. We wait for completion before returning
        let result = unsafe { crate::monoio::blocking::unblock_fwrite(raw_fd, ptr, len).await };

        (result, buf)
    }

    pub(crate) async fn write_at<T: IoBuf>(
        fd: SharedFd,
        buf: T,
        pos: u64,
    ) -> crate::monoio::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Using zero-allocation variant with raw pointers
        let ptr = buf.read_ptr();
        let len = buf.bytes_init();

        // Safety:
        // 1. buf is immutable during write and owned by this function
        // 2. The blocking thread only accesses the pointer during the write_at
        // 3. We wait for completion before returning
        let result =
            unsafe { crate::monoio::blocking::unblock_fwrite_at(raw_fd, ptr, len, pos).await };

        (result, buf)
    }

    // Implementing vectored IO via loop for fallback
    pub(crate) async fn read_vectored<T: IoVecBufMut>(
        fd: SharedFd,
        mut buf_vec: T,
    ) -> crate::monoio::BufResult<usize, T> {
        // Simple implementation: read into first buffer, or implement proper vectored read via readv if available?
        // std::fs::File has read_vectored.
        let raw_fd = fd.raw_fd();

        // NOTE: A robust implementation of read_vectored for fallback is non-trivial with current traits.
        // For now, return an error indicating the operation is not implemented.
        // We could implement this in the future using zero-allocation operations if needed.
        (
            Err(io::Error::new(
                io::ErrorKind::Other,
                "read_vectored fallback not fully implemented",
            )),
            buf_vec,
        )
    }

    pub(crate) async fn write_vectored<T: IoVecBuf>(
        fd: SharedFd,
        buf_vec: T,
    ) -> crate::monoio::BufResult<usize, T> {
        // Similar issue as read_vectored.
        // We could implement this in the future using zero-allocation operations if needed.
        (
            Err(io::Error::new(
                io::ErrorKind::Other,
                "write_vectored fallback not fully implemented",
            )),
            buf_vec,
        )
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) use fallback_impl::*;
