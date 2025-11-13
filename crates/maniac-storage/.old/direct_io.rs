//! Direct I/O plumbing for maniac-storage: opens aligned files, writes published slabs, and bridges WAL checkpoints to disk.
use std::convert::TryFrom;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io;
#[cfg(not(any(unix, windows)))]
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::config::{PAGE_SIZE, StorageConfig};
use crate::paged_file::{PageRange, PagedFile};
use crate::slab::PublishedSlab;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectIoConstraints {
    pub buffer: usize,
    pub file_offset: usize,
}

#[cfg(target_os = "linux")]
const PLATFORM_DIRECT_IO_ALIGNMENT: usize = 4096;
#[cfg(target_os = "macos")]
const PLATFORM_DIRECT_IO_ALIGNMENT: usize = 4096;
#[cfg(target_os = "windows")]
const PLATFORM_DIRECT_IO_ALIGNMENT: usize = 4096;
#[cfg(target_os = "freebsd")]
const PLATFORM_DIRECT_IO_ALIGNMENT: usize = 4096;
#[cfg(target_os = "dragonfly")]
const PLATFORM_DIRECT_IO_ALIGNMENT: usize = 4096;
#[cfg(target_os = "netbsd")]
const PLATFORM_DIRECT_IO_ALIGNMENT: usize = 4096;
#[cfg(target_os = "openbsd")]
const PLATFORM_DIRECT_IO_ALIGNMENT: usize = 4096;
#[cfg(target_os = "android")]
const PLATFORM_DIRECT_IO_ALIGNMENT: usize = 4096;
#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "android",
)))]
const PLATFORM_DIRECT_IO_ALIGNMENT: usize = 4096;

/// Returns the minimum alignment requirements for issuing direct I/O to the host platform.
pub const fn platform_constraints() -> DirectIoConstraints {
    DirectIoConstraints {
        buffer: PLATFORM_DIRECT_IO_ALIGNMENT,
        file_offset: PLATFORM_DIRECT_IO_ALIGNMENT,
    }
}

/// Aligned buffers must respect this boundary before being passed to direct I/O syscalls.
pub const fn buffer_alignment() -> usize {
    platform_constraints().buffer
}

/// File offsets must be a multiple of this value when issuing direct I/O.
pub const fn file_offset_alignment() -> usize {
    platform_constraints().file_offset
}

#[derive(Debug, Error)]
pub enum DirectFileError {
    #[error("direct I/O parent creation failed for {path}: {source}")]
    CreateParent { path: PathBuf, source: io::Error },
    #[error("direct I/O open failed for {path}: {source}")]
    Open { path: PathBuf, source: io::Error },
    #[error("direct I/O configuration failed: {source}")]
    Configure { source: io::Error },
    #[error("direct I/O mutex poisoned")]
    Poisoned,
    #[error("buffer length {actual} expected {expected}")]
    BufferMismatch { expected: usize, actual: usize },
    #[error("range length {actual} expected {expected}")]
    RangeMismatch { expected: usize, actual: usize },
    #[error("flush failed: {source}")]
    Flush { source: io::Error },
    #[error("sync failed: {source}")]
    Sync { source: io::Error },
    #[error(transparent)]
    Io {
        #[from]
        source: io::Error,
    },
}

#[derive(Debug)]
/// Synchronous direct-I/O backed paged file that enforces alignment and durability semantics.
pub struct DirectFile {
    file: Mutex<File>,
    config: StorageConfig,
}

impl DirectFile {
    /// Opens (creating ancestors if needed) a direct-I/O file and wraps it in `DirectFile`.
    pub fn open(path: impl AsRef<Path>, config: StorageConfig) -> Result<Self, DirectFileError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| DirectFileError::CreateParent {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let file = open_direct_file(path).map_err(|source| DirectFileError::Open {
            path: path.to_path_buf(),
            source,
        })?;
        configure_direct_file(&file)?;
        Ok(Self {
            file: Mutex::new(file),
            config,
        })
    }

    /// Builds a `DirectFile` around an existing handle, applying platform tweaks.
    fn with_file(file: File, config: StorageConfig) -> Result<Self, DirectFileError> {
        configure_direct_file(&file)?;
        Ok(Self {
            file: Mutex::new(file),
            config,
        })
    }

    pub fn into_inner(self) -> StorageConfig {
        self.config
    }
}

impl PagedFile for DirectFile {
    type Error = DirectFileError;

    fn config(&self) -> &StorageConfig {
        &self.config
    }

    fn read(&self, range: PageRange, dst: &mut [u8]) -> Result<usize, Self::Error> {
        let expected = range.byte_len();
        if dst.len() != expected {
            return Err(DirectFileError::BufferMismatch {
                expected,
                actual: dst.len(),
            });
        }
        let offset = range.start().0 * PAGE_SIZE;
        let guard = self.file.lock().map_err(|_| DirectFileError::Poisoned)?;
        read_exact_at(&*guard, dst, offset)?;
        Ok(expected)
    }

    fn write(&self, range: PageRange, payload: &[u8]) -> Result<(), Self::Error> {
        let expected = range.byte_len();
        if payload.len() != expected {
            return Err(DirectFileError::RangeMismatch {
                expected,
                actual: payload.len(),
            });
        }
        let offset = range.start().0 * PAGE_SIZE;
        let guard = self.file.lock().map_err(|_| DirectFileError::Poisoned)?;
        write_all_at(&*guard, payload, offset)?;
        Ok(())
    }

    fn flush(&self) -> Result<(), Self::Error> {
        self.file
            .lock()
            .map_err(|_| DirectFileError::Poisoned)?
            .sync_data()
            .map_err(|source| DirectFileError::Flush { source })
    }

    fn complete_checkpoint(&self) -> Result<(), Self::Error> {
        self.file
            .lock()
            .map_err(|_| DirectFileError::Poisoned)?
            .sync_all()
            .map_err(|source| DirectFileError::Sync { source })
    }
}

#[derive(Debug)]
pub enum DirectIoError<E> {
    PayloadNotPageAligned { len: usize, page_size: usize },
    PayloadExceedsSlab { len: usize, capacity: usize },
    PageCountOverflow { page_count: usize },
    RangeOverflow { base_page: u64, count: u32 },
    BufferMisaligned { ptr: usize, alignment: usize },
    Backend(E),
}

impl<E: fmt::Display> fmt::Display for DirectIoError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadNotPageAligned { len, page_size } => write!(
                f,
                "payload length {len} is not a multiple of page size {page_size}"
            ),
            Self::PayloadExceedsSlab { len, capacity } => {
                write!(f, "payload length {len} exceeds slab capacity {capacity}")
            }
            Self::PageCountOverflow { page_count } => {
                write!(f, "page count {page_count} exceeds addressable u32 range")
            }
            Self::RangeOverflow { base_page, count } => write!(
                f,
                "page range starting at {base_page} with count {count} overflowed"
            ),
            Self::BufferMisaligned { ptr, alignment } => write!(
                f,
                "buffer pointer {ptr:#x} is not aligned to {alignment} bytes"
            ),
            Self::Backend(err) => write!(f, "direct I/O backend error: {err}"),
        }
    }
}

impl<E> std::error::Error for DirectIoError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Backend(err) => Some(err),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
/// Helper that streams published slabs into a `PagedFile` implementation (usually `DirectFile`).
pub struct DirectIoWriter<P>
where
    P: PagedFile,
{
    target: Arc<P>,
    config: StorageConfig,
}

impl<P> DirectIoWriter<P>
where
    P: PagedFile,
{
    pub fn new(target: Arc<P>, config: StorageConfig) -> Self {
        debug_assert_eq!(
            target.config(),
            &config,
            "paged file configuration should match direct I/O writer configuration"
        );
        Self { target, config }
    }

    pub fn config(&self) -> &StorageConfig {
        &self.config
    }

    pub fn target(&self) -> Arc<P> {
        Arc::clone(&self.target)
    }

    pub fn write_slab(&self, slab: &PublishedSlab) -> Result<(), DirectIoError<P::Error>> {
        let header = slab.header();
        let payload_len = header.payload_len as usize;
        let full = slab.payload();
        if payload_len > full.len() {
            return Err(DirectIoError::PayloadExceedsSlab {
                len: payload_len,
                capacity: full.len(),
            });
        }
        if payload_len == 0 {
            return Ok(());
        }
        let ptr = full.as_ptr() as usize;
        let alignment = buffer_alignment().max(PAGE_SIZE as usize);
        if ptr % alignment != 0 {
            return Err(DirectIoError::BufferMisaligned { ptr, alignment });
        }
        let page_size = PAGE_SIZE as usize;
        if payload_len % page_size != 0 {
            return Err(DirectIoError::PayloadNotPageAligned {
                len: payload_len,
                page_size,
            });
        }
        let page_count = payload_len / page_size;
        let page_count = u32::try_from(page_count)
            .map_err(|_| DirectIoError::PageCountOverflow { page_count })?;
        let range =
            PageRange::new(header.base_page, page_count).ok_or(DirectIoError::RangeOverflow {
                base_page: header.base_page,
                count: page_count,
            })?;
        let payload = &full[..payload_len];
        self.target
            .write(range, payload)
            .map_err(DirectIoError::Backend)?;
        Ok(())
    }

    pub fn flush(&self) -> Result<(), DirectIoError<P::Error>> {
        self.target.flush().map_err(DirectIoError::Backend)
    }

    pub fn complete_checkpoint(&self) -> Result<(), DirectIoError<P::Error>> {
        self.target
            .complete_checkpoint()
            .map_err(DirectIoError::Backend)
    }
}

impl DirectIoWriter<DirectFile> {
    /// Convenience for opening a `DirectFile` and returning a writer bound to it.
    pub fn open_path(
        path: impl AsRef<Path>,
        config: StorageConfig,
    ) -> Result<Self, DirectFileError> {
        let device = Arc::new(DirectFile::open(path, config)?);
        Ok(Self::new(device, config))
    }

    /// Reuses an externally managed file handle while keeping writer semantics.
    pub fn from_file(file: File, config: StorageConfig) -> Result<Self, DirectFileError> {
        let device = Arc::new(DirectFile::with_file(file, config)?);
        Ok(Self::new(device, config))
    }
}

#[cfg(unix)]
fn open_direct_file(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    let mut flags = 0;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        flags |= libc::O_DIRECT | libc::O_DSYNC;
    }
    #[cfg(any(
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        flags |= libc::O_DSYNC;
    }
    if flags != 0 {
        options.custom_flags(flags);
    }
    options.mode(0o644);
    options.open(path)
}
#[cfg(unix)]
fn configure_direct_file(file: &File) -> Result<(), DirectFileError> {
    use std::os::unix::io::AsRawFd;

    let fd = file.as_raw_fd();
    #[cfg(not(target_os = "macos"))]
    let _ = fd;
    #[cfg(target_os = "macos")]
    {
        unsafe {
            if libc::fcntl(fd, libc::F_NOCACHE, 1) == -1 {
                return Err(DirectFileError::Configure {
                    source: io::Error::last_os_error(),
                });
            }
            if libc::fcntl(fd, libc::F_FULLFSYNC, 1) == -1 {
                return Err(DirectFileError::Configure {
                    source: io::Error::last_os_error(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn open_direct_file(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_NORMAL, FILE_FLAG_NO_BUFFERING, FILE_FLAG_WRITE_THROUGH, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    options.custom_flags((FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH) as u32);
    options.access_mode((FILE_GENERIC_READ | FILE_GENERIC_WRITE) as u32);
    options.share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE) as u32);
    options.attributes(FILE_ATTRIBUTE_NORMAL as u32);
    match options.open(path) {
        Ok(file) => Ok(file),
        Err(err) if err.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) => {
            let mut fallback = OpenOptions::new();
            fallback.read(true).write(true).create(true);
            fallback.custom_flags(FILE_FLAG_WRITE_THROUGH as u32);
            fallback.access_mode((FILE_GENERIC_READ | FILE_GENERIC_WRITE) as u32);
            fallback.share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE) as u32);
            fallback.attributes(FILE_ATTRIBUTE_NORMAL as u32);
            fallback.open(path)
        }
        Err(err) => Err(err),
    }
}

#[cfg(windows)]
fn configure_direct_file(_file: &File) -> Result<(), DirectFileError> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn open_direct_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
}

#[cfg(not(any(unix, windows)))]
fn configure_direct_file(_file: &File) -> Result<(), DirectFileError> {
    Ok(())
}

#[cfg(unix)]
fn write_all_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;

    file.write_all_at(buf, offset)?;
    Ok(())
}

#[cfg(unix)]
fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;

    file.read_exact_at(buf, offset)?;
    Ok(())
}

#[cfg(windows)]
fn write_all_at(file: &File, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;

    while !buf.is_empty() {
        let written = file.seek_write(buf, offset)?;
        if written == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "direct I/O write returned zero",
            ));
        }
        buf = &buf[written..];
        offset += written as u64;
    }
    Ok(())
}

#[cfg(windows)]
fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;

    while !buf.is_empty() {
        let read = file.seek_read(buf, offset)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "direct I/O read returned zero",
            ));
        }
        let tmp = buf;
        buf = &mut tmp[read..];
        offset += read as u64;
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn write_all_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    use std::io::Seek;

    let mut file = file.try_clone()?;
    file.seek(io::SeekFrom::Start(offset))?;
    file.write_all(buf)
}

#[cfg(not(any(unix, windows)))]
fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    use std::io::Seek;

    let mut file = file.try_clone()?;
    file.seek(io::SeekFrom::Start(offset))?;
    file.read_exact(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MIN_EXTENT_SIZE, MIN_SLAB_SIZE, StorageFlavor};
    use crate::paged_file::{MockPagedFile, PagedFile};
    use crate::slab::{SlabPool, SlabPublishParams};
    use std::sync::Arc;

    #[test]
    fn writes_published_slab_via_mock_file() {
        let config =
            StorageConfig::new(StorageFlavor::Aof, MIN_EXTENT_SIZE, MIN_SLAB_SIZE, 2).unwrap();
        let pool = SlabPool::new(config).expect("pool");
        let mut lease = pool.acquire_writer().expect("lease");
        let payload_len = MIN_EXTENT_SIZE as usize;
        let page_size = PAGE_SIZE as usize;
        let page_count = (payload_len / page_size) as u32;
        for (idx, byte) in lease.buffer_mut()[..payload_len].iter_mut().enumerate() {
            *byte = (idx % 251) as u8;
        }
        let params = SlabPublishParams::new(0, page_count, payload_len);
        let slab = lease.seal(params).expect("seal");
        let file = Arc::new(MockPagedFile::new(config));
        let writer = DirectIoWriter::new(Arc::clone(&file), config);
        writer.write_slab(&slab).expect("write slab");
        writer.flush().expect("flush");
        let range = PageRange::new(0, page_count).expect("range");
        let mut buf = vec![0u8; payload_len];
        file.read(range, &mut buf).expect("read");
        assert_eq!(&buf[..], &slab.payload()[..payload_len]);
    }

    #[test]
    fn checkpoint_delegates_to_backend() {
        let config =
            StorageConfig::new(StorageFlavor::Standard, MIN_EXTENT_SIZE, MIN_SLAB_SIZE, 2).unwrap();
        let file = Arc::new(MockPagedFile::new(config));
        let writer = DirectIoWriter::new(Arc::clone(&file), config);
        writer.complete_checkpoint().expect("checkpoint");
    }

    #[test]
    fn writes_slab_to_direct_file_device() {
        let config =
            StorageConfig::new(StorageFlavor::Aof, MIN_EXTENT_SIZE, MIN_SLAB_SIZE, 2).unwrap();
        let pool = SlabPool::new(config).expect("pool");
        let mut lease = pool.acquire_writer().expect("lease");
        let payload_len = MIN_EXTENT_SIZE as usize;
        let page_size = PAGE_SIZE as usize;
        let page_count = (payload_len / page_size) as u32;
        lease.buffer_mut()[..payload_len].fill(0x7F);
        let params = SlabPublishParams::new(0, page_count, payload_len);
        let slab = lease.seal(params).expect("seal");
        let tmp = tempfile::tempdir().expect("tempdir");
        let file_path = tmp.path().join("direct_io.dat");
        let device = Arc::new(DirectFile::open(&file_path, config).expect("open device"));
        let writer = DirectIoWriter::new(Arc::clone(&device), config);
        writer.write_slab(&slab).expect("write slab");
        writer.flush().expect("flush");
        let range = PageRange::new(0, page_count).expect("range");
        let mut buf = vec![0u8; payload_len];
        device.read(range, &mut buf).expect("read");
        assert!(buf.iter().all(|&b| b == 0x7F));
    }
}
