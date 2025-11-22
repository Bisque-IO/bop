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
    fs::{metadata::FileAttr, Metadata},
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
            // We need to duplicate the FD or use stat on path? 
            // Actually we can use fstat on the raw fd.
            // Since we don't have easy access to std::fs::Metadata from raw fd without wrapping,
            // let's use the blocking pool to call std::fs::File::metadata.
            // But we only have SharedFd.
            // Ideally we would use `statx` equivalent if available or fallback to `fstat`.
            // For simplicity and because we are in "blocking fallback" mode:
            crate::monoio::blocking::unblock(move || {
                // Safety: We are just using the FD to get metadata.
                // We must ensure we don't close it.
                let file = unsafe { std::fs::File::from_raw_fd(fd) };
                let meta = file.metadata();
                let _ = file.into_raw_fd(); // prevent closing
                meta.map(Metadata)
            }).await
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    pub async fn open(path: impl AsRef<Path>) -> io::Result<File> {
        let path = path.as_ref().to_owned();
        let std_file = crate::monoio::blocking::unblock(move || std::fs::File::open(path)).await?;
        File::from_std(std_file)
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn create(path: impl AsRef<Path>) -> io::Result<File> {
        let path = path.as_ref().to_owned();
        let std_file = crate::monoio::blocking::unblock(move || std::fs::File::create(path)).await?;
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
        crate::monoio::blocking::unblock(move || {
            let file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
            let meta = file.metadata();
            let _ = file.into_raw_fd();
            meta.map(Metadata)
        }).await
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
    
    pub(crate) async fn read<T: IoBufMut>(fd: SharedFd, mut buf: T) -> crate::monoio::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        // We can't easily move the buffer into the thread pool if it's not Send or if we need to return it.
        // IoBufMut is likely not Send if it contains raw pointers, but usually it's implemented for Vec<u8> etc.
        // However, `unblock` requires the closure to be Send.
        // If T is Vec<u8>, it is Send.
        
        // We need to take ownership of the buffer, pass it to the thread, fill it, and return it.
        match crate::monoio::blocking::unblock(move || {
            // Reconstruct file (borrowed)
            let mut file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
            
            // Get a slice from the buffer
            // Safety: We have ownership of buf in this closure
            let ptr = buf.write_ptr();
            let len = buf.bytes_total();
            let slice = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
            
            use std::io::Read;
            let res = file.read(slice);
            
            // Don't close the fd
            let _ = file.into_raw_fd();
            
            match res {
                Ok(n) => {
                    unsafe { buf.set_init(n) };
                    (Ok(n), buf)
                }
                Err(e) => (Err(e), buf),
            }
        }).await {
            (res, buf) => (res, buf),
        }
    }

    pub(crate) async fn read_at<T: IoBufMut>(fd: SharedFd, mut buf: T, pos: u64) -> crate::monoio::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        
        match crate::monoio::blocking::unblock(move || {
            let file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
            
            let ptr = buf.write_ptr();
            let len = buf.bytes_total();
            let slice = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
            
            let res = file.read_at(slice, pos);
            
            let _ = file.into_raw_fd();
            
            match res {
                Ok(n) => {
                    unsafe { buf.set_init(n) };
                    (Ok(n), buf)
                }
                Err(e) => (Err(e), buf),
            }
        }).await {
            (res, buf) => (res, buf),
        }
    }

    pub(crate) async fn write<T: IoBuf>(fd: SharedFd, buf: T) -> crate::monoio::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        
        match crate::monoio::blocking::unblock(move || {
            let mut file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
            
            let ptr = buf.read_ptr();
            let len = buf.bytes_init();
            let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
            
            use std::io::Write;
            let res = file.write(slice);
            
            let _ = file.into_raw_fd();
            
            (res, buf)
        }).await {
            (res, buf) => (res, buf),
        }
    }

    pub(crate) async fn write_at<T: IoBuf>(fd: SharedFd, buf: T, pos: u64) -> crate::monoio::BufResult<usize, T> {
        let raw_fd = fd.raw_fd();
        
        match crate::monoio::blocking::unblock(move || {
            let file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
            
            let ptr = buf.read_ptr();
            let len = buf.bytes_init();
            let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
            
            let res = file.write_at(slice, pos);
            
            let _ = file.into_raw_fd();
            
            (res, buf)
        }).await {
            (res, buf) => (res, buf),
        }
    }
    
    // Implementing vectored IO via loop for fallback
    pub(crate) async fn read_vectored<T: IoVecBufMut>(
        fd: SharedFd,
        mut buf_vec: T,
    ) -> crate::monoio::BufResult<usize, T> {
        // Simple implementation: read into first buffer, or implement proper vectored read via readv if available?
        // std::fs::File has read_vectored.
        let raw_fd = fd.raw_fd();
        
        match crate::monoio::blocking::unblock(move || {
            let mut file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
            
            // We need to construct IoSliceMut from the raw pointers in buf_vec
            // This is tricky because IoVecBufMut trait provides raw iovec pointers/iterators which might be OS specific
            // For simplicity, let's just fallback to reading into the first buffer or loop.
            // A proper implementation would require unsafe casting to IoSliceMut compatible layout.
            
            // Taking a safer approach: iterate and read one by one (not atomic, but this is fallback)
            // Or just use read_vectored if we can construct the slice.
            
            // Let's just do a simple read into the first available buffer for now as a basic fallback,
            // or better, loop through them.
            
            let mut total_read = 0;
            // This is hard to implement efficiently without access to the concrete type of T or unsafe hacks
            // that duplicate IoVecBufMut logic.
            
            // Placeholder:
            use std::io::Read;
            // We will cheat and just read into a temporary buffer and copy? No that's slow.
            // Let's just use the first buffer for MVP fallback.
            
            // NOTE: A robust implementation of read_vectored for fallback is non-trivial with current traits.
            // For now, we return 0 to indicate no-op or error? No, that's EOF.
            // Let's error.
            (Err(io::Error::new(io::ErrorKind::Other, "read_vectored fallback not fully implemented")), buf_vec)
        }).await {
            (res, buf) => (res, buf),
        }
    }

    pub(crate) async fn write_vectored<T: IoVecBuf>(
        fd: SharedFd,
        buf_vec: T,
    ) -> crate::monoio::BufResult<usize, T> {
        // Similar issue as read_vectored.
        (Err(io::Error::new(io::ErrorKind::Other, "write_vectored fallback not fully implemented")), buf_vec)
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) use fallback_impl::*;
