//! Shared paged-file abstraction that higher layers (AOF/Standard) implement against.
use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::{PAGE_SIZE, StorageConfig, StorageFlavor};

/// Identifier for a logical 4 KiB page within a paged file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageId(pub u64);

/// Inclusive-exclusive page range describing an IO request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageRange {
    start: PageId,
    count: NonZeroU32,
}

impl PageRange {
    /// Creates a new page range covering `count` pages starting from `start`.
    ///
    /// # Errors
    /// Returns `None` if `count` is zero or if the resulting range would overflow `u64`.
    pub fn new(start: u64, count: u32) -> Option<Self> {
        let count = NonZeroU32::new(count)?;
        let _end = start.checked_add(count.get() as u64)?;
        Some(Self {
            start: PageId(start),
            count,
        })
    }

    pub fn start(&self) -> PageId {
        self.start
    }

    pub fn count(&self) -> u32 {
        self.count.get()
    }

    pub fn end(&self) -> PageId {
        PageId(self.start.0 + self.count.get() as u64)
    }

    pub fn byte_len(&self) -> usize {
        self.count.get() as usize * PAGE_SIZE as usize
    }
}

/// Page-oriented storage abstraction shared across bop components.
///
/// The trait is intentionally blocking and synchronous for Milestone 0. Higher layers can wrap
/// implementations in async runtimes as needed.
pub trait PagedFile: Send + Sync {
    type Error;

    /// Returns the storage configuration associated with this file.
    fn config(&self) -> &StorageConfig;

    /// Reads `range` into `dst`, returning the number of bytes copied. Implementations must fill
    /// every requested byte or return an error.
    fn read(&self, range: PageRange, dst: &mut [u8]) -> Result<usize, Self::Error>;

    /// Writes the provided payload at the given range. The payload length must equal the range
    /// length in bytes.
    fn write(&self, range: PageRange, payload: &[u8]) -> Result<(), Self::Error>;

    /// Forces outstanding writes to reach persistent storage.
    fn flush(&self) -> Result<(), Self::Error>;

    /// Signals that a durability checkpoint has completed. Defaults to invoking `flush`.
    fn complete_checkpoint(&self) -> Result<(), Self::Error> {
        self.flush()
    }
}

/// In-memory `PagedFile` used for unit tests and compile-time examples.
///
/// # Examples
///
/// ```
/// use bop_storage::paged_file::{MockPagedFile, PageRange, PagedFile};
/// use bop_storage::{StorageConfig, StorageFlavor};
/// const PAGE: usize = bop_storage::config::PAGE_SIZE as usize;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let cfg = StorageConfig::new(StorageFlavor::Aof, 64 * 1024, 2 * 1024 * 1024, 2)?;
/// let file = MockPagedFile::new(cfg);
/// let range = PageRange::new(0, 1).expect("range");
/// file.write(range, &[1u8; PAGE])?;
/// let mut buf = vec![0u8; PAGE];
/// file.read(range, &mut buf)?;
/// assert_eq!(buf[0], 1);
/// file.flush()?;
/// # Ok(()) }
/// ```
///
/// ```
/// use bop_storage::paged_file::{MockPagedFile, PageRange, PagedFile};
/// use bop_storage::{StorageConfig, StorageFlavor};
/// const PAGE: usize = bop_storage::config::PAGE_SIZE as usize;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let cfg = StorageConfig::new(StorageFlavor::Standard, 64 * 1024, 2 * 1024 * 1024, 4)?;
/// let file = MockPagedFile::new(cfg);
/// let r0 = PageRange::new(10, 2).expect("range");
/// file.write(r0, &[7u8; PAGE * 2])?;
/// let mut buf = vec![0u8; PAGE * 2];
/// file.read(r0, &mut buf)?;
/// assert!(buf.iter().all(|&b| b == 7));
/// file.complete_checkpoint()?;
/// # Ok(()) }
/// ```
#[derive(Debug)]
pub struct MockPagedFile {
    cfg: StorageConfig,
    pages: Mutex<BTreeMap<u64, Vec<u8>>>,
    /// Tracks the append position for AOF flavor; ignored for Standard.
    next_page: AtomicU64,
}

impl MockPagedFile {
    pub fn new(cfg: StorageConfig) -> Self {
        Self {
            next_page: AtomicU64::new(0),
            cfg,
            pages: Mutex::new(BTreeMap::new()),
        }
    }

    fn ensure_payload_aligned(range: PageRange, payload: &[u8]) -> Result<(), MockPagedFileError> {
        let expected = range.byte_len();
        if payload.len() != expected {
            return Err(MockPagedFileError::PayloadSizeMismatch {
                expected,
                actual: payload.len(),
            });
        }
        if payload.len() % PAGE_SIZE as usize != 0 {
            return Err(MockPagedFileError::UnalignedPayload { len: payload.len() });
        }
        Ok(())
    }
}

impl PagedFile for MockPagedFile {
    type Error = MockPagedFileError;

    fn config(&self) -> &StorageConfig {
        &self.cfg
    }

    fn read(&self, range: PageRange, dst: &mut [u8]) -> Result<usize, Self::Error> {
        if dst.len() != range.byte_len() {
            return Err(MockPagedFileError::BufferSizeMismatch {
                expected: range.byte_len(),
                actual: dst.len(),
            });
        }
        let pages = self
            .pages
            .lock()
            .map_err(|_| MockPagedFileError::Poisoned)?;
        let mut copied = 0;
        for page in range.start().0..range.end().0 {
            let buf = pages.get(&page);
            let start = copied;
            let end = start + PAGE_SIZE as usize;
            if let Some(src) = buf {
                dst[start..end].copy_from_slice(src);
            } else {
                dst[start..end].fill(0);
            }
            copied = end;
        }
        Ok(copied)
    }

    fn write(&self, range: PageRange, payload: &[u8]) -> Result<(), Self::Error> {
        Self::ensure_payload_aligned(range, payload)?;
        if matches!(self.cfg.flavor, StorageFlavor::Aof) {
            let expected = self.next_page.load(Ordering::Acquire);
            if range.start().0 != expected {
                return Err(MockPagedFileError::OutOfOrderAppend {
                    expected,
                    requested: range.start().0,
                });
            }
            self.next_page.store(range.end().0, Ordering::Release);
        }
        let mut pages = self
            .pages
            .lock()
            .map_err(|_| MockPagedFileError::Poisoned)?;
        for (idx, page) in (range.start().0..range.end().0).enumerate() {
            let start = idx * PAGE_SIZE as usize;
            let end = start + PAGE_SIZE as usize;
            pages.insert(page, payload[start..end].to_vec());
        }
        Ok(())
    }

    fn flush(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn complete_checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Errors emitted by `MockPagedFile`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MockPagedFileError {
    #[error("payload length {actual} expected {expected}")]
    PayloadSizeMismatch { expected: usize, actual: usize },
    #[error("payload length {len} is not aligned to page size")]
    UnalignedPayload { len: usize },
    #[error("destination buffer length {actual} expected {expected}")]
    BufferSizeMismatch { expected: usize, actual: usize },
    #[error("append expected page {expected} but received {requested}")]
    OutOfOrderAppend { expected: u64, requested: u64 },
    #[error("mock paged file lock poisoned")]
    Poisoned,
}

/// ```
/// use bop_storage::paged_file::{MockPagedFile, PageRange, PagedFile};
/// use bop_storage::{StorageConfig, StorageFlavor};
/// const PAGE: usize = bop_storage::config::PAGE_SIZE as usize;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let cfg = StorageConfig::new(StorageFlavor::Aof, 64 * 1024, 2 * 1024 * 1024, 2)?;
/// let file = MockPagedFile::new(cfg);
/// let range = PageRange::new(0, 1).expect("range");
/// file.write(range, &[1u8; PAGE])?;
/// let mut buf = vec![0u8; PAGE];
/// file.read(range, &mut buf)?;
/// assert_eq!(buf[0], 1);
/// file.flush()?;
/// # Ok(()) }
/// ```
///
/// ```
/// use bop_storage::paged_file::{MockPagedFile, PageRange, PagedFile};
/// use bop_storage::{StorageConfig, StorageFlavor};
/// const PAGE: usize = bop_storage::config::PAGE_SIZE as usize;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let cfg = StorageConfig::new(StorageFlavor::Standard, 64 * 1024, 2 * 1024 * 1024, 4)?;
/// let file = MockPagedFile::new(cfg);
/// let r0 = PageRange::new(10, 2).expect("range");
/// file.write(r0, &[7u8; PAGE * 2])?;
/// let mut buf = vec![0u8; PAGE * 2];
/// file.read(r0, &mut buf)?;
/// assert!(buf.iter().all(|&b| b == 7));
/// file.complete_checkpoint()?;
/// # Ok(()) }
/// ```
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_example_aof_behavior() {
        let cfg = StorageConfig::new(StorageFlavor::Aof, 64 * 1024, 2 * 1024 * 1024, 2).unwrap();
        let file = MockPagedFile::new(cfg);
        let r0 = PageRange::new(0, 1).unwrap();
        let data = vec![1u8; PAGE_SIZE as usize];
        file.write(r0, &data).unwrap();
        let mut buf = vec![0u8; PAGE_SIZE as usize];
        file.read(r0, &mut buf).unwrap();
        assert_eq!(buf, data);
    }

    #[test]
    fn aof_requires_sequential_appends() {
        let cfg = StorageConfig::new(StorageFlavor::Aof, 64 * 1024, 2 * 1024 * 1024, 2).unwrap();
        let file = MockPagedFile::new(cfg);
        let r0 = PageRange::new(0, 1).unwrap();
        file.write(r0, &[1u8; PAGE_SIZE as usize]).unwrap();
        let err = file
            .write(PageRange::new(2, 1).unwrap(), &[1u8; PAGE_SIZE as usize])
            .unwrap_err();
        assert!(matches!(err, MockPagedFileError::OutOfOrderAppend { .. }));
    }
}
