//! Direct I/O driver backed by platform specific flags and alignment rules.
//!
//! ## Alignment Contract
//! - Offsets, buffer lengths, and buffer pointers must be multiples of [`DirectIoDriver::alignment`].
//! - The driver trusts the caller and returns [`IoError`] variants for misalignment instead of copying buffers.
//!
//! [`DirectIoDriver::allocate`] returns aligned scratch space when a caller needs temporary storage.
use super::{
    IoBackendKind, IoDriver, IoError, IoFile, IoOpenOptions, IoResult, IoVec, IoVecMut,
    preallocate, sync_file_data,
};
use std::alloc::{Layout, alloc, dealloc};
use std::convert::TryFrom;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;

#[cfg(test)]
use std::path::PathBuf;

const FALLBACK_ALIGNMENT: usize = 4096;

#[derive(Clone, Copy, Debug)]
/// Records the alignment contract enforced by the direct I/O backend.
///
/// Offsets, buffer lengths, and buffer pointers must be multiples of [`alignment`].
/// The driver relies on this contract to reject misaligned operations instead of realigning
/// or copying buffers internally.
struct AlignmentContract {
    backend: IoBackendKind,
    alignment: usize,
}

impl AlignmentContract {
    fn new(backend: IoBackendKind, alignment: usize) -> Self {
        debug_assert!(alignment > 0);
        debug_assert!(alignment.is_power_of_two());
        Self { backend, alignment }
    }

    fn alignment(&self) -> usize {
        self.alignment
    }

    fn require_offset(&self, offset: u64) -> IoResult<()> {
        if offset % self.alignment as u64 != 0 {
            return Err(IoError::OffsetMisaligned {
                backend: self.backend,
                offset,
                alignment: self.alignment,
            });
        }
        Ok(())
    }

    fn require_len(&self, len: usize) -> IoResult<()> {
        if len % self.alignment != 0 {
            return Err(IoError::LengthMisaligned {
                backend: self.backend,
                len,
                alignment: self.alignment,
            });
        }
        Ok(())
    }

    fn require_ptr(&self, ptr: usize) -> IoResult<()> {
        if ptr % self.alignment != 0 {
            return Err(IoError::BufferMisaligned {
                backend: self.backend,
                ptr,
                alignment: self.alignment,
            });
        }
        Ok(())
    }

    fn require_slice(&self, slice: &[u8]) -> IoResult<()> {
        if slice.is_empty() {
            return Ok(());
        }
        self.require_len(slice.len())?;
        self.require_ptr(slice.as_ptr() as usize)
    }

    fn require_mut_slice(&self, slice: &mut [u8]) -> IoResult<()> {
        if slice.is_empty() {
            return Ok(());
        }
        self.require_len(slice.len())?;
        self.require_ptr(slice.as_ptr() as usize)
    }
}

#[derive(Clone)]
/// Direct I/O backend that issues unbuffered reads and writes with alignment guarantees.
pub struct DirectIoDriver {
    contract: AlignmentContract,
}

impl DirectIoDriver {
    pub fn new() -> io::Result<Self> {
        let alignment = platform_alignment().unwrap_or(FALLBACK_ALIGNMENT).max(1);
        Ok(Self {
            contract: AlignmentContract::new(IoBackendKind::DirectIo, alignment),
        })
    }

    pub fn alignment(&self) -> usize {
        self.contract.alignment()
    }

    /// Allocate a zeroed buffer that satisfies the driver's alignment requirements.
    ///
    /// The requested length must already be a multiple of [`Self::alignment`]; callers must pad
    /// any residual tail data themselves before issuing a direct write.
    pub fn allocate(&self, len: usize) -> IoResult<DirectIoBuffer> {
        self.contract.require_len(len)?;
        let mut buffer = DirectIoBuffer::new(len, self.alignment())?;
        if !buffer.is_empty() {
            buffer.as_mut_slice().fill(0);
        }
        Ok(buffer)
    }

    fn ensure_supported(&self, options: &IoOpenOptions) -> IoResult<()> {
        if options.append {
            return Err(IoError::UnsupportedOperation {
                backend: IoBackendKind::DirectIo,
                operation: "append",
            });
        }
        Ok(())
    }

    fn open_aligned(&self, path: &Path, options: &IoOpenOptions) -> io::Result<File> {
        #[cfg(target_os = "linux")]
        {
            open_direct_linux(path, options)
        }

        #[cfg(target_os = "macos")]
        {
            open_direct_macos(path, options)
        }

        #[cfg(target_os = "windows")]
        {
            open_direct_windows(path, options)
        }

        #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
        {
            open_direct_unix(path, options)
        }
    }
}

impl std::fmt::Debug for DirectIoDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectIoDriver")
            .field("backend", &self.backend())
            .field("alignment", &self.contract.alignment())
            .finish()
    }
}

impl IoDriver for DirectIoDriver {
    fn open(&self, path: &Path, options: &IoOpenOptions) -> IoResult<Box<dyn IoFile>> {
        self.ensure_supported(options)?;

        if options.create {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(IoError::from)?;
            }
        }

        let file = self.open_aligned(path, options).map_err(IoError::from)?;
        configure_after_open(&file).map_err(IoError::from)?;
        Ok(Box::new(DirectIoFile::new(file, self.contract)))
    }

    fn backend(&self) -> IoBackendKind {
        IoBackendKind::DirectIo
    }
}

/// Direct-I/O backed file handle that enforces alignment requirements.
struct DirectIoFile {
    file: Arc<File>,
    contract: AlignmentContract,
}

impl DirectIoFile {
    fn new(file: File, contract: AlignmentContract) -> Self {
        Self {
            file: Arc::new(file),
            contract,
        }
    }
}

impl IoFile for DirectIoFile {
    fn read_at(&self, offset: u64, len: usize) -> IoResult<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }

        self.contract.require_offset(offset)?;
        self.contract.require_len(len)?;
        Err(IoError::UnsupportedOperation {
            backend: IoBackendKind::DirectIo,
            operation: "read_at",
        })
    }

    fn readv_at(&self, offset: u64, bufs: &mut [IoVecMut<'_>]) -> IoResult<usize> {
        if bufs.is_empty() {
            return Ok(0);
        }

        self.contract.require_offset(offset)?;

        for buf in bufs.iter_mut() {
            if buf.is_empty() {
                continue;
            }
            let slice = buf.as_mut_slice();
            self.contract.require_mut_slice(slice)?;
        }

        let mut current_offset = offset;
        let mut total = 0usize;

        for buf in bufs.iter_mut() {
            if buf.is_empty() {
                continue;
            }

            let mut slice = buf.as_mut_slice();
            while !slice.is_empty() {
                let read = read_into(&self.file, current_offset, slice)?;
                if read == 0 {
                    return Ok(total);
                }
                total += read;
                current_offset += read as u64;
                slice = &mut slice[read..];
            }
        }

        Ok(total)
    }

    fn writev_at(&self, offset: u64, bufs: &[IoVec<'_>]) -> IoResult<usize> {
        if bufs.is_empty() {
            return Ok(0);
        }

        self.contract.require_offset(offset)?;

        for buf in bufs {
            if buf.is_empty() {
                continue;
            }
            self.contract.require_slice(buf.as_slice())?;
        }

        let mut current_offset = offset;
        let mut total = 0usize;

        for buf in bufs {
            if buf.is_empty() {
                continue;
            }
            let mut slice = buf.as_slice();

            while !slice.is_empty() {
                let written =
                    write_once(&self.file, current_offset, slice).map_err(IoError::from)?;
                if written == 0 {
                    return Err(IoError::Io(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "direct I/O write returned zero",
                    )));
                }
                total += written;
                current_offset += written as u64;
                slice = &slice[written..];
            }
        }

        Ok(total)
    }

    fn allocate(&self, offset: u64, len: u64) -> IoResult<()> {
        self.contract.require_offset(offset)?;
        if len == 0 {
            return Ok(());
        }

        let len_usize = usize::try_from(len).map_err(|_| {
            IoError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "allocation length exceeds usize",
            ))
        })?;

        self.contract.require_len(len_usize)?;

        preallocate(self.file.as_ref(), offset, len).map_err(IoError::from)
    }

    fn flush(&self) -> IoResult<()> {
        sync_file_data(self.file.as_ref()).map_err(IoError::from)
    }
}

fn read_into(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::FileExt;
        file.read_at(buf, offset)
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        use std::os::unix::fs::FileExt;
        file.read_at(buf, offset)
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::FileExt;
        file.seek_read(buf, offset)
    }
}

fn write_once(file: &File, offset: u64, buf: &[u8]) -> io::Result<usize> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::FileExt;
        file.write_at(buf, offset)
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        use std::os::unix::fs::FileExt;
        file.write_at(buf, offset)
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::FileExt;
        file.seek_write(buf, offset)
    }
}

/// Heap allocation that maintains sector-sized alignment for direct I/O calls.
pub struct DirectIoBuffer {
    ptr: NonNull<u8>,
    len: usize,
    layout: Layout,
}

impl DirectIoBuffer {
    /// Allocate a new buffer with the requested length and alignment.
    pub fn new(len: usize, alignment: usize) -> IoResult<Self> {
        let layout = Layout::from_size_align(len, alignment).map_err(|_| {
            IoError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid layout",
            ))
        })?;

        let ptr = unsafe { alloc(layout) };
        let ptr = NonNull::new(ptr).ok_or_else(|| {
            IoError::Io(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "failed to allocate aligned buffer",
            ))
        })?;

        Ok(Self { ptr, len, layout })
    }

    /// Returns the alignment guaranteed by this allocation.
    pub fn alignment(&self) -> usize {
        self.layout.align()
    }

    /// Returns the logical length of the buffer in bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` when the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Immutable access to the aligned byte slice.
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// Mutable access to the aligned byte slice.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl std::fmt::Debug for DirectIoBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectIoBuffer")
            .field("len", &self.len)
            .field("alignment", &self.alignment())
            .finish()
    }
}

impl Drop for DirectIoBuffer {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}

#[cfg(target_os = "linux")]
fn open_direct_linux(path: &Path, options: &IoOpenOptions) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut std_options = OpenOptions::new();
    std_options
        .read(options.read)
        .write(options.write || options.append)
        .create(options.create)
        .truncate(options.truncate);

    let mut flags = libc::O_DIRECT;
    if options.write || options.append {
        flags |= libc::O_DSYNC;
    }
    std_options.custom_flags(flags);
    std_options.mode(0o644);

    std_options.open(path)
}

#[cfg(target_os = "macos")]
fn open_direct_macos(path: &Path, options: &IoOpenOptions) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut std_options = OpenOptions::new();
    std_options
        .read(options.read)
        .write(options.write || options.append)
        .append(options.append)
        .create(options.create)
        .truncate(options.truncate)
        .mode(0o644);

    std_options.open(path)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn open_direct_unix(path: &Path, options: &IoOpenOptions) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut std_options = OpenOptions::new();
    std_options
        .read(options.read)
        .write(options.write || options.append)
        .create(options.create)
        .truncate(options.truncate);

    let mut flags = libc::O_DSYNC;
    if options.write || options.append {
        flags |= libc::O_SYNC;
    }
    std_options.custom_flags(flags);
    std_options.mode(0o644);

    std_options.open(path)
}

#[cfg(target_os = "windows")]
fn open_direct_windows(path: &Path, options: &IoOpenOptions) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_NO_BUFFERING, FILE_FLAG_WRITE_THROUGH,
    };

    let mut std_options = OpenOptions::new();
    std_options
        .read(options.read)
        .write(options.write || options.append)
        .append(options.append)
        .create(options.create)
        .truncate(options.truncate);

    let flags = FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH;
    std_options.custom_flags(flags);

    std_options.open(path)
}

fn platform_alignment() -> Option<usize> {
    #[cfg(unix)]
    {
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page_size > 0 {
            return Some(page_size as usize);
        }
    }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceW;

        unsafe {
            let mut sectors_per_cluster = 0;
            let mut bytes_per_sector = 0u32;
            let mut number_of_free_clusters = 0;
            let mut total_number_of_clusters = 0;
            if GetDiskFreeSpaceW(
                std::ptr::null(),
                &mut sectors_per_cluster,
                &mut bytes_per_sector,
                &mut number_of_free_clusters,
                &mut total_number_of_clusters,
            ) != 0
                && bytes_per_sector > 0
            {
                return Some(bytes_per_sector as usize);
            }
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn configure_after_open(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;

    let fd = file.as_raw_fd();
    let result = unsafe { libc::fcntl(fd, libc::F_NOCACHE, 1) };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn configure_after_open(_file: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn temp_path(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(name);
        (dir, path)
    }

    #[test]
    fn rejects_append_mode() {
        let driver = DirectIoDriver::new().expect("driver");
        let (_, path) = temp_path("direct-append.log");
        let mut opts = IoOpenOptions::write_only();
        opts.append = true;
        let err = match driver.open(&path, &opts) {
            Ok(_) => panic!("expected append mode to be rejected"),
            Err(err) => err,
        };
        assert!(matches!(err, IoError::UnsupportedOperation { .. }));
    }

    #[test]
    fn enforces_alignment_on_write() {
        let driver = DirectIoDriver::new().expect("driver");
        let (_, path) = temp_path("direct-align.bin");
        let mut opts = IoOpenOptions::write_only();
        opts.truncate = true;
        let file = driver.open(&path, &opts).expect("open");
        let align = driver.alignment();
        let mut aligned = driver.allocate(align).expect("buffer");
        aligned.as_mut_slice().fill(0xAA);
        let err = file.write_at(1, aligned.as_slice()).unwrap_err();
        assert!(matches!(err, IoError::OffsetMisaligned { .. }));

        let backing = vec![0u8; align + 1];
        let misaligned = &backing[1..1 + align];
        assert_ne!(misaligned.as_ptr() as usize % align, 0);
        let err = file.write_at(0, misaligned).unwrap_err();
        assert!(matches!(err, IoError::BufferMisaligned { .. }));
    }

    #[test]
    fn enforces_alignment_on_read() {
        let driver = DirectIoDriver::new().expect("driver");
        let (_, path) = temp_path("direct-read.bin");
        let mut opts = IoOpenOptions::read_write();
        opts.truncate = true;
        let file = driver.open(&path, &opts).expect("open");
        let err = file.read_at(0, driver.alignment() / 2).unwrap_err();
        assert!(matches!(err, IoError::LengthMisaligned { .. }));
    }

    #[test]
    fn read_at_requires_aligned_buffer_from_caller() {
        let driver = DirectIoDriver::new().expect("driver");
        let (_, path) = temp_path("direct-read-misaligned.bin");
        let mut opts = IoOpenOptions::read_write();
        opts.truncate = true;
        let file = driver.open(&path, &opts).expect("open");
        let err = file.read_at(0, driver.alignment()).unwrap_err();
        assert!(matches!(
            err,
            IoError::UnsupportedOperation { operation, .. } if operation == "read_at"
        ));
    }

    #[test]
    fn allocate_rejects_misaligned_length() {
        let driver = DirectIoDriver::new().expect("driver");
        let err = driver.allocate(driver.alignment() / 2).unwrap_err();
        assert!(matches!(err, IoError::LengthMisaligned { .. }));
    }

    #[test]
    fn allocate_returns_aligned_buffer() {
        let driver = DirectIoDriver::new().expect("driver");
        let align = driver.alignment();
        let len = align * 2;
        let mut buffer = driver.allocate(len).expect("allocate");
        assert_eq!(buffer.len(), len);
        assert_eq!(buffer.alignment(), align);
        assert_eq!(buffer.as_slice().as_ptr() as usize % align, 0);
        buffer.as_mut_slice().fill(0xCC);
        assert!(buffer.as_slice().iter().all(|&b| b == 0xCC));
    }

    #[test]
    fn vectored_roundtrip_reads_and_writes() {
        let driver = DirectIoDriver::new().expect("driver");
        let align = driver.alignment();
        let (_, path) = temp_path("direct-vectored.bin");

        let mut opts = IoOpenOptions::read_write();
        opts.truncate = true;
        let file = driver.open(&path, &opts).expect("open");

        let mut write_a = driver.allocate(align).expect("buf a");
        let mut write_b = driver.allocate(align).expect("buf b");
        write_a.as_mut_slice().fill(0x11);
        write_b.as_mut_slice().fill(0x22);

        let write_vecs = [
            IoVec::new(write_a.as_slice()),
            IoVec::new(write_b.as_slice()),
        ];
        file.writev_at(0, &write_vecs).expect("write");

        let mut read_a = driver.allocate(align).expect("read a");
        let mut read_b = driver.allocate(align).expect("read b");

        {
            let mut vecs = [
                IoVecMut::new(read_a.as_mut_slice()),
                IoVecMut::new(read_b.as_mut_slice()),
            ];
            file.readv_at(0, &mut vecs).expect("read");
        }

        assert!(read_a.as_slice().iter().all(|&b| b == 0x11));
        assert!(read_b.as_slice().iter().all(|&b| b == 0x22));
    }

    #[test]
    fn allocate_extends_file() {
        let driver = DirectIoDriver::new().expect("driver");
        let (_, path) = temp_path("direct-allocate.bin");

        let mut opts = IoOpenOptions::read_write();
        opts.truncate = true;
        let file = driver.open(&path, &opts).expect("open");

        let align = driver.alignment() as u64;
        file.allocate(0, align).expect("allocate");
        file.flush().expect("flush");

        let len = fs::metadata(&path).expect("metadata").len();
        assert_eq!(len, align);
    }
}
