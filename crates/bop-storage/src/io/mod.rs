use std::fs::File;
use std::path::Path;
use std::sync::Arc;

#[cfg(any(unix, target_os = "windows"))]
mod direct;
#[cfg(any(unix, target_os = "windows"))]
pub use direct::{DirectIoBuffer, DirectIoDriver};

use thiserror::Error;
use tracing::{debug, error, info, instrument, trace, warn};

/// Enumeration of supported storage I/O backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IoBackendKind {
    Std,
    DirectIo,
    IoUring,
}

/// Borrowed immutable buffer used for vectored I/O operations.
///
/// Callers must ensure the underlying slice already satisfies any alignment contract required
/// by the active backend (for example, direct I/O requires sector-sized alignment).
#[derive(Clone, Copy)]
pub struct IoVec<'a> {
    slice: &'a [u8],
}

impl<'a> IoVec<'a> {
    pub fn new(slice: &'a [u8]) -> Self {
        Self { slice }
    }

    pub fn empty() -> Self {
        Self { slice: &[] }
    }

    pub fn len(&self) -> usize {
        self.slice.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slice.is_empty()
    }

    pub fn as_slice(&self) -> &'a [u8] {
        self.slice
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.slice.as_ptr()
    }
}

impl<'a> From<&'a [u8]> for IoVec<'a> {
    fn from(slice: &'a [u8]) -> Self {
        Self::new(slice)
    }
}

/// Borrowed mutable buffer used for vectored reads.
///
/// Backend-specific alignment contracts apply to the mutable slice before I/O is issued.
pub struct IoVecMut<'a> {
    slice: &'a mut [u8],
}

impl<'a> IoVecMut<'a> {
    pub fn new(slice: &'a mut [u8]) -> Self {
        Self { slice }
    }

    pub fn len(&self) -> usize {
        self.slice.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slice.is_empty()
    }

    pub fn as_slice(&self) -> &[u8] {
        self.slice
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.slice
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.slice.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.slice.as_mut_ptr()
    }
}

impl<'a> From<&'a mut [u8]> for IoVecMut<'a> {
    fn from(slice: &'a mut [u8]) -> Self {
        Self::new(slice)
    }
}

/// Options used when opening a file through an [`IoDriver`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoOpenOptions {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub create: bool,
    pub truncate: bool,
}

impl IoOpenOptions {
    pub fn read_only() -> Self {
        Self {
            read: true,
            write: false,
            append: false,
            create: false,
            truncate: false,
        }
    }

    pub fn write_only() -> Self {
        Self {
            read: false,
            write: true,
            append: false,
            create: true,
            truncate: false,
        }
    }

    pub fn read_write() -> Self {
        Self {
            read: true,
            write: true,
            append: false,
            create: true,
            truncate: false,
        }
    }
}

impl Default for IoOpenOptions {
    fn default() -> Self {
        Self::read_only()
    }
}

#[derive(Debug, Error)]
pub enum IoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("I/O backend {backend:?} is not available on this platform")]
    BackendUnavailable { backend: IoBackendKind },
    #[error("{backend:?} backend does not support {operation}")]
    UnsupportedOperation {
        backend: IoBackendKind,
        operation: &'static str,
    },
    #[error("{backend:?} backend requires offsets aligned to {alignment} bytes (offset={offset})")]
    OffsetMisaligned {
        backend: IoBackendKind,
        offset: u64,
        alignment: usize,
    },
    #[error(
        "{backend:?} backend requires buffer lengths as multiples of {alignment} bytes (len={len})"
    )]
    LengthMisaligned {
        backend: IoBackendKind,
        len: usize,
        alignment: usize,
    },
    #[error(
        "{backend:?} backend requires buffer pointers aligned to {alignment} bytes (ptr={ptr:#x})"
    )]
    BufferMisaligned {
        backend: IoBackendKind,
        ptr: usize,
        alignment: usize,
    },
}

pub type IoResult<T> = Result<T, IoError>;

pub trait IoFile: Send + Sync {
    /// Read into the provided vectored buffers starting at `offset`.
    ///
    /// # Alignment
    /// Backends may enforce offset, length, and pointer alignment. Direct I/O rejects
    /// misaligned buffers instead of copying them, so callers must uphold the contract
    /// before invoking this method.
    fn readv_at(&self, offset: u64, bufs: &mut [IoVecMut<'_>]) -> IoResult<usize>;
    /// Write the provided vectored buffers starting at `offset`.
    ///
    /// # Alignment
    /// Backends with direct I/O semantics require the caller to provide aligned slices
    /// and lengths. The implementation surfaces [`IoError`] variants for misuse rather
    /// than padding or copying data.
    fn writev_at(&self, offset: u64, bufs: &[IoVec<'_>]) -> IoResult<usize>;

    /// Convenience wrapper around [`Self::readv_at`] that allocates a single buffer.
    ///
    /// The buffer allocated here is not aligned for direct I/O; the direct backend signals
    /// misuse with [`IoError::UnsupportedOperation`] and expects callers to pass an aligned
    /// buffer through [`Self::readv_at`].
    fn read_at(&self, offset: u64, len: usize) -> IoResult<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }

        let mut buf = vec![0u8; len];
        let mut io_vec = IoVecMut::new(buf.as_mut_slice());
        let read = self.readv_at(offset, std::slice::from_mut(&mut io_vec))?;
        buf.truncate(read);
        Ok(buf)
    }

    /// Convenience wrapper around [`Self::writev_at`].
    ///
    /// Direct I/O callers must ensure that `data` already satisfies the backend alignment
    /// contract. This method does not pad or copy buffers on their behalf.
    fn write_at(&self, offset: u64, data: &[u8]) -> IoResult<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        let io_vec = IoVec::new(data);
        self.writev_at(offset, &[io_vec])
    }

    fn allocate(&self, offset: u64, len: u64) -> IoResult<()>;

    fn flush(&self) -> IoResult<()>;
}

pub trait IoDriver: Send + Sync {
    fn open(&self, path: &Path, options: &IoOpenOptions) -> IoResult<Box<dyn IoFile>>;
    fn backend(&self) -> IoBackendKind;
}

pub type SharedIoDriver = Arc<dyn IoDriver>;

#[derive(Clone)]
pub struct IoRegistry {
    default_backend: IoBackendKind,
    std_driver: Arc<StdIoDriver>,
    #[cfg(any(unix, target_os = "windows"))]
    direct: Option<Arc<DirectIoDriver>>,
}

impl IoRegistry {
    #[instrument(level = "info", skip(default_backend), fields(backend = ?default_backend))]
    pub fn new(default_backend: IoBackendKind) -> Self {
        #[cfg(any(unix, target_os = "windows"))]
        let direct = DirectIoDriver::new().ok().map(Arc::new);

        #[cfg(any(unix, target_os = "windows"))]
        if direct.is_some() {
            info!(backend = ?IoBackendKind::DirectIo, "Direct I/O driver initialized");
        } else {
            warn!(backend = ?IoBackendKind::DirectIo, "Direct I/O driver unavailable");
        }

        info!(default_backend = ?default_backend, "I/O registry initialized");

        Self {
            default_backend,
            std_driver: Arc::new(StdIoDriver::new()),
            #[cfg(any(unix, target_os = "windows"))]
            direct,
        }
    }

    pub fn default_backend(&self) -> IoBackendKind {
        self.default_backend
    }

    #[instrument(level = "debug", skip(self), fields(requested = ?requested, default = ?self.default_backend))]
    pub fn resolve(&self, requested: Option<IoBackendKind>) -> IoResult<SharedIoDriver> {
        let backend = requested.unwrap_or(self.default_backend);
        debug!(backend = ?backend, "Resolving I/O driver");

        match backend {
            IoBackendKind::Std => {
                debug!(backend = ?IoBackendKind::Std, "Using standard I/O driver");
                Ok(self.std_driver.clone())
            }
            IoBackendKind::DirectIo => self.resolve_direct(),
            IoBackendKind::IoUring => {
                warn!(backend = ?IoBackendKind::IoUring, "I/O URing backend not available");
                Err(IoError::BackendUnavailable { backend })
            }
        }
    }
}

impl IoRegistry {
    #[cfg(any(unix, target_os = "windows"))]
    fn resolve_direct(&self) -> IoResult<SharedIoDriver> {
        match &self.direct {
            Some(driver) => {
                debug!(backend = ?IoBackendKind::DirectIo, "Using direct I/O driver");
                Ok(driver.clone())
            }
            None => {
                warn!(backend = ?IoBackendKind::DirectIo, "Direct I/O driver unavailable");
                Err(IoError::BackendUnavailable {
                    backend: IoBackendKind::DirectIo,
                })
            }
        }
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    fn resolve_direct(&self) -> IoResult<SharedIoDriver> {
        warn!(backend = ?IoBackendKind::DirectIo, "Direct I/O not supported on this platform");
        Err(IoError::BackendUnavailable {
            backend: IoBackendKind::DirectIo,
        })
    }
}

#[derive(Clone, Default)]
pub struct StdIoDriver;

impl StdIoDriver {
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Debug for StdIoDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdIoDriver")
            .field("backend", &self.backend())
            .finish()
    }
}

impl IoDriver for StdIoDriver {
    #[instrument(level = "debug", skip(self, options), fields(path = ?path, backend = "Std"))]
    fn open(&self, path: &Path, options: &IoOpenOptions) -> IoResult<Box<dyn IoFile>> {
        let path = path.to_path_buf();
        let options = options.clone();

        debug!(path = ?path, read = options.read, write = options.write, "Opening file with standard I/O");

        let file = open_file(&path, &options).map_err(|e| {
            error!(path = ?path, error = %e, "Failed to open file");
            IoError::from(e)
        })?;

        debug!(path = ?path, "File opened successfully");
        Ok(Box::new(StdIoFile::new(file)))
    }

    fn backend(&self) -> IoBackendKind {
        IoBackendKind::Std
    }
}

struct StdIoFile {
    file: Arc<File>,
}

impl StdIoFile {
    fn new(file: File) -> Self {
        Self {
            file: Arc::new(file),
        }
    }
}

impl IoFile for StdIoFile {
    #[instrument(level = "trace", skip(self, bufs), fields(offset, num_bufs = bufs.len()))]
    fn readv_at(&self, offset: u64, bufs: &mut [IoVecMut<'_>]) -> IoResult<usize> {
        let total_len: usize = bufs.iter().map(|b| b.len()).sum();
        trace!(
            offset,
            size_bytes = total_len,
            num_bufs = bufs.len(),
            "Reading vectored data"
        );

        let mut current_offset = offset;
        let mut total = 0usize;

        for buf in bufs.iter_mut() {
            let slice = buf.as_mut_slice();
            let mut consumed = 0usize;
            while consumed < slice.len() {
                let read = read_at(self.file.as_ref(), &mut slice[consumed..], current_offset)
                    .map_err(|e| {
                        error!(offset = current_offset, error = %e, "Read failed");
                        IoError::from(e)
                    })?;
                if read == 0 {
                    trace!(bytes_read = total, "Read complete (EOF)");
                    return Ok(total);
                }
                consumed += read;
                total += read;
                current_offset += read as u64;
            }
        }

        trace!(bytes_read = total, "Read complete");
        Ok(total)
    }

    #[instrument(level = "trace", skip(self, bufs), fields(offset, num_bufs = bufs.len()))]
    fn writev_at(&self, offset: u64, bufs: &[IoVec<'_>]) -> IoResult<usize> {
        use std::io::{Error, ErrorKind};

        let total_len: usize = bufs.iter().map(|b| b.len()).sum();
        trace!(
            offset,
            size_bytes = total_len,
            num_bufs = bufs.len(),
            "Writing vectored data"
        );

        let mut current_offset = offset;
        let mut total = 0usize;

        for buf in bufs {
            let slice = buf.as_slice();
            let mut written = 0usize;
            while written < slice.len() {
                let count = write_at(self.file.as_ref(), &slice[written..], current_offset)
                    .map_err(|e| {
                        error!(offset = current_offset, error = %e, "Write failed");
                        IoError::from(e)
                    })?;
                if count == 0 {
                    error!(offset = current_offset, "Write returned zero bytes");
                    return Err(IoError::Io(Error::new(
                        ErrorKind::WriteZero,
                        "write returned zero bytes",
                    )));
                }
                written += count;
                total += count;
                current_offset += count as u64;
            }
        }

        trace!(bytes_written = total, "Write complete");
        Ok(total)
    }

    #[instrument(level = "debug", skip(self), fields(offset, len))]
    fn allocate(&self, offset: u64, len: u64) -> IoResult<()> {
        debug!(offset, size_bytes = len, "Allocating file space");
        preallocate(self.file.as_ref(), offset, len).map_err(|e| {
            error!(offset, len, error = %e, "File allocation failed");
            IoError::from(e)
        })?;
        debug!(offset, size_bytes = len, "File space allocated");
        Ok(())
    }

    #[instrument(level = "debug", skip(self))]
    fn flush(&self) -> IoResult<()> {
        debug!("Flushing file to disk");
        sync_file_data(self.file.as_ref()).map_err(|e| {
            error!(error = %e, "File flush failed");
            IoError::from(e)
        })?;
        debug!("File flushed successfully");
        Ok(())
    }
}

fn open_file(path: &Path, options: &IoOpenOptions) -> std::io::Result<File> {
    let mut std_options = std::fs::OpenOptions::new();
    std_options
        .read(options.read)
        .write(options.write || options.append)
        .append(options.append)
        .create(options.create)
        .truncate(options.truncate);
    std_options.open(path)
}

pub(super) fn sync_file_data(file: &File) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::io::AsRawFd;

        let fd = file.as_raw_fd();
        let result = unsafe { libc::fcntl(fd, libc::F_FULLFSYNC, 0) };
        if result == 0 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(code) if code == libc::ENOTSUP || code == libc::EINVAL => return file.sync_all(),
            _ => return Err(err),
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::os::unix::io::AsRawFd;

        let fd = file.as_raw_fd();
        let result = unsafe { libc::fdatasync(fd) };
        if result == 0 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(code) if code == libc::ENOSYS || code == libc::EINVAL => file.sync_all(),
            _ => Err(err),
        }
    }

    #[cfg(not(unix))]
    {
        file.sync_all()
    }
}

pub(super) fn preallocate(file: &File, offset: u64, len: u64) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};

    if len == 0 {
        return Ok(());
    }

    let end = offset
        .checked_add(len)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "allocation range overflow"))?;

    #[cfg(target_os = "windows")]
    {
        if end > file.metadata()?.len() {
            file.set_len(end)?;
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;

        let fd = file.as_raw_fd();
        let res = unsafe { libc::fallocate(fd, 0, offset as libc::off_t, len as libc::off_t) };
        if res == 0 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(code)
                if code == libc::EOPNOTSUPP || code == libc::ENOSYS || code == libc::EINVAL =>
            {
                // fall through to posix_fallocate or set_len
            }
            _ => return Err(err),
        }
    }

    #[cfg(all(unix))]
    {
        use std::os::unix::io::AsRawFd;

        let fd = file.as_raw_fd();
        let res = unsafe { libc::posix_fallocate(fd, offset as libc::off_t, len as libc::off_t) };
        if res == 0 {
            return Ok(());
        }
        if res != libc::ENOSYS && res != libc::EOPNOTSUPP {
            return Err(std::io::Error::from_raw_os_error(res));
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::os::unix::io::AsRawFd;

        let fd = file.as_raw_fd();
        let mut store = libc::fstore_t {
            fst_flags: libc::F_ALLOCATECONTIG,
            fst_posmode: libc::F_VOLPOSMODE,
            fst_offset: offset as libc::off_t,
            fst_length: len as libc::off_t,
            fst_bytesalloc: 0,
        };

        let res = unsafe { libc::fcntl(fd, libc::F_PREALLOCATE, &store) };
        if res == 0 {
            return Ok(());
        }

        store.fst_flags = libc::F_ALLOCATEALL;
        store.fst_posmode = libc::F_VOLPOSMODE;
        let res = unsafe { libc::fcntl(fd, libc::F_PREALLOCATE, &store) };
        if res == 0 {
            return Ok(());
        }
    }

    // Fallback: extend the file size manually.
    #[cfg(not(target_os = "windows"))]
    {
        if end > file.metadata()?.len() {
            file.set_len(end)?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buf, offset)
}

#[cfg(windows)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buf, offset)
}

#[cfg(unix)]
fn write_at(file: &File, buf: &[u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.write_at(buf, offset)
}

#[cfg(windows)]
fn write_at(file: &File, buf: &[u8], offset: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_write(buf, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn std_driver_read_write_roundtrip() {
        let driver = StdIoDriver::new();

        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("io-roundtrip.bin");

        let mut options = IoOpenOptions::read_write();
        options.truncate = true;

        let file = driver.open(&path, &options).expect("open file");
        let write_vecs = [IoVec::new(&[1, 2]), IoVec::new(&[3, 4])];
        file.writev_at(0, &write_vecs).expect("write");

        let mut read_buf = [0u8; 4];
        {
            let (left, right) = read_buf.split_at_mut(2);
            let mut read_vecs = [IoVecMut::new(left), IoVecMut::new(right)];
            file.readv_at(0, &mut read_vecs).expect("read");
        }
        assert_eq!(read_buf, [1, 2, 3, 4]);
        file.flush().expect("flush");
    }

    #[test]
    fn std_driver_allocate_extends_file() {
        let driver = StdIoDriver::new();

        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("io-allocate.bin");

        let mut options = IoOpenOptions::write_only();
        options.truncate = true;

        let file = driver.open(&path, &options).expect("open file");
        file.allocate(0, 8192).expect("allocate");
        file.flush().expect("flush");

        let len = std::fs::metadata(&path).expect("metadata").len();
        assert_eq!(len, 8192);
    }

    #[cfg(any(unix, target_os = "windows"))]
    #[test]
    fn registry_resolves_direct_backend() {
        let registry = IoRegistry::new(IoBackendKind::Std);
        let driver = registry
            .resolve(Some(IoBackendKind::DirectIo))
            .expect("direct backend");
        assert_eq!(driver.backend(), IoBackendKind::DirectIo);
    }
}
