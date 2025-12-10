use std::{
    fs::File as StdFile,
    io,
    os::fd::{AsRawFd, IntoRawFd, RawFd},
    path::Path,
};

#[cfg(not(target_os = "linux"))]
use std::os::unix::fs::FileExt;

use super::File;
#[cfg(unix)]
use crate::fs::metadata::FileAttr;
use crate::uring_op;
use crate::{
    buf::{IoBuf, IoBufMut, IoVecBuf, IoVecBufMut},
    driver::{op::Op, shared_fd::SharedFd},
    fs::Metadata,
};

impl File {
    /// Converts a [`std::fs::File`] to a [`fs::File`](File).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // This line could block. It is not recommended to do this on the monoio
    /// // runtime.
    /// let std_file = std::fs::File::open("foo.txt").unwrap();
    /// let file = fs::File::from_std(std_file);
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
    /// use fs::File;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///     let mut f = File::open("foo.txt").await?;
    ///     let metadata = f.metadata().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn metadata(&self) -> io::Result<Metadata> {
        #[cfg(all(target_os = "linux", feature = "iouring"))]
        {
            metadata(self.fd.clone()).await
        }
        #[cfg(not(all(target_os = "linux", feature = "iouring")))]
        {
            // On non-io_uring systems (including Linux without io_uring), we use blocking thread pool for fstat
            let fd = self.fd.raw_fd();
            fstat_to_metadata(fd).await
        }
    }

}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.raw_fd()
    }
}

pub(crate) async fn metadata(fd: SharedFd) -> std::io::Result<Metadata> {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    {
        let flags = libc::AT_STATX_SYNC_AS_STAT | libc::AT_EMPTY_PATH;
        let op = Op::statx_using_fd(fd, flags)?;
        op.result().await.map(FileAttr::from).map(Metadata)
    }
    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    {
        // On non-io_uring systems, use fstat via blocking thread pool
        let raw_fd = fd.raw_fd();
        fstat_to_metadata(raw_fd).await
    }
}

/// Helper function to convert fstat result to our Metadata type
#[cfg(not(all(target_os = "linux", feature = "iouring")))]
async fn fstat_to_metadata(fd: RawFd) -> std::io::Result<Metadata> {
    let stat_result = crate::blocking::unblock(move || {
        #[cfg(target_os = "linux")]
        {
            let mut stat: libc::stat64 = unsafe { std::mem::zeroed() };
            let ret = unsafe { libc::fstat64(fd, &mut stat) };
            if ret < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(stat)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let mut stat: libc::stat = unsafe { std::mem::zeroed() };
            let ret = unsafe { libc::fstat(fd, &mut stat) };
            if ret < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(stat)
            }
        }
    })
    .await?;

    #[cfg(target_os = "linux")]
    {
        let file_attr = FileAttr {
            stat: stat_result,
            statx_extra_fields: None,
        };
        Ok(Metadata(file_attr))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let file_attr = FileAttr {
            stat: stat_result,
        };
        Ok(Metadata(file_attr))
    }
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
mod linux_impl {
    use super::*;

    uring_op!(read<IoBufMut>(read, buf));
    uring_op!(read_at<IoBufMut>(read_at, buf, pos: u64));
    uring_op!(read_vectored<IoVecBufMut>(readv, buf_vec));

    uring_op!(write<IoBuf>(write, buf));
    uring_op!(write_at<IoBuf>(write_at, buf, pos: u64));
    uring_op!(write_vectored<IoVecBuf>(writev, buf_vec));
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
pub(crate) use linux_impl::*;

#[cfg(not(all(target_os = "linux", feature = "iouring")))]
mod fallback_impl {
    use super::*;

    pub(crate) async fn read<T: IoBufMut>(fd: SharedFd, mut buf: T) -> crate::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Using zero-allocation variant with raw pointers
        let ptr = buf.write_ptr();
        let len = buf.bytes_total();

        // Safety:
        // 1. buf is owned by this async function and won't move
        // 2. The blocking thread only accesses the pointer during the read
        // 3. We wait for completion before returning
        let result = unsafe { crate::blocking::unblock_fread(raw_fd, ptr, len).await };

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
    ) -> crate::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Using zero-allocation variant with raw pointers
        let ptr = buf.write_ptr();
        let len = buf.bytes_total();

        // Safety:
        // 1. buf is owned by this async function and won't move
        // 2. The blocking thread only accesses the pointer during the read_at
        // 3. We wait for completion before returning
        let result = unsafe { crate::blocking::unblock_fread_at(raw_fd, ptr, len, pos).await };

        match result {
            Ok(n) => {
                unsafe { buf.set_init(n) };
                (Ok(n), buf)
            }
            Err(e) => (Err(e), buf),
        }
    }

    pub(crate) async fn write<T: IoBuf>(fd: SharedFd, buf: T) -> crate::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Using zero-allocation variant with raw pointers
        let ptr = buf.read_ptr();
        let len = buf.bytes_init();

        // Safety:
        // 1. buf is immutable during write and owned by this function
        // 2. The blocking thread only accesses the pointer during the write
        // 3. We wait for completion before returning
        let result = unsafe { crate::blocking::unblock_fwrite(raw_fd, ptr, len).await };

        (result, buf)
    }

    pub(crate) async fn write_at<T: IoBuf>(
        fd: SharedFd,
        buf: T,
        pos: u64,
    ) -> crate::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Using zero-allocation variant with raw pointers
        let ptr = buf.read_ptr();
        let len = buf.bytes_init();

        // Safety:
        // 1. buf is immutable during write and owned by this function
        // 2. The blocking thread only accesses the pointer during the write_at
        // 3. We wait for completion before returning
        let result = unsafe { crate::blocking::unblock_fwrite_at(raw_fd, ptr, len, pos).await };

        (result, buf)
    }

    // Wrapper type for iovec pointer to implement Send
    // Safety: The caller ensures the buffers outlive the blocking operation
    #[derive(Clone, Copy)]
    struct SendPtr(usize);  // Store as usize to avoid raw pointer Send issues
    unsafe impl Send for SendPtr {}
    unsafe impl Sync for SendPtr {}

    // Implementing vectored IO using readv/writev syscalls via blocking thread pool
    pub(crate) async fn read_vectored<T: IoVecBufMut>(
        fd: SharedFd,
        mut buf_vec: T,
    ) -> crate::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Get pointers and length before moving into closure
        // Store pointer as usize to work around Send requirements
        let iovec_ptr = SendPtr(buf_vec.write_iovec_ptr() as usize);
        let iovec_len = buf_vec.write_iovec_len().min(i32::MAX as usize);

        // Safety:
        // 1. buf_vec is owned by this function and remains valid during the operation
        // 2. The iovec structures are owned by buf_vec and remain valid
        // 3. The blocking thread only accesses the iovec structures during the readv call
        // 4. We wait for completion before returning
        // Note: We capture raw_fd and pointers by value, not buf_vec itself
        let result = crate::blocking::unblock(move || unsafe {
            let ptr = iovec_ptr.0 as *mut libc::iovec;
            let nread = libc::readv(raw_fd, ptr, iovec_len as _);
            if nread < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(nread as usize)
            }
        })
        .await;

        match result {
            Ok(n) => {
                unsafe { buf_vec.set_init(n) };
                (Ok(n), buf_vec)
            }
            Err(e) => (Err(e), buf_vec),
        }
    }

    pub(crate) async fn write_vectored<T: IoVecBuf>(
        fd: SharedFd,
        buf_vec: T,
    ) -> crate::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // Get pointers and length before moving into closure
        // Store pointer as usize to work around Send requirements
        let iovec_ptr = SendPtr(buf_vec.read_iovec_ptr() as usize);
        let iovec_len = buf_vec.read_iovec_len().min(i32::MAX as usize);

        // Safety:
        // 1. buf_vec is owned by this function and remains valid during the operation
        // 2. The iovec structures are owned by buf_vec and remain valid
        // 3. The blocking thread only accesses the iovec structures during the writev call
        // 4. We wait for completion before returning
        // Note: We capture raw_fd and pointers by value, not buf_vec itself
        let result = crate::blocking::unblock(move || unsafe {
            let ptr = iovec_ptr.0 as *const libc::iovec;
            let nwritten = libc::writev(raw_fd, ptr, iovec_len as _);
            if nwritten < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(nwritten as usize)
            }
        })
        .await;

        (result, buf_vec)
    }
}

#[cfg(not(all(target_os = "linux", feature = "iouring")))]
pub(crate) use fallback_impl::*;
