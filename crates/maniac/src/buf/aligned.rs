use std::alloc::Layout;
use std::ptr::NonNull;

use super::{IoBuf, IoBufMut};

/// A heap buffer with a configurable alignment.
///
/// Intended for Direct I/O usage (e.g. Linux `O_DIRECT`, Windows `FILE_FLAG_NO_BUFFERING`),
/// which often requires aligned buffers and aligned offsets/lengths.
#[derive(Debug)]
pub struct AlignedBuf {
    ptr: NonNull<u8>,
    cap: usize,
    init: usize,
    align: usize,
}

impl Default for AlignedBuf {
    fn default() -> Self {
        Self {
            ptr: NonNull::dangling(),
            cap: 0,
            init: 0,
            align: 1,
        }
    }
}

// Safety: `AlignedBuf` uniquely owns its allocation and does not expose shared mutable aliasing
// across threads. Moving it between threads is safe.
unsafe impl Send for AlignedBuf {}
unsafe impl Sync for AlignedBuf {}

impl AlignedBuf {
    /// Allocate a new aligned buffer with capacity `cap` bytes.
    ///
    /// - `align` must be a power of two.
    /// - The buffer is initially uninitialized from the perspective of `IoBuf` (`init = 0`).
    pub fn new(cap: usize, align: usize) -> Self {
        assert!(align.is_power_of_two(), "align must be power of two");
        let cap = cap.max(1);
        let layout = Layout::from_size_align(cap, align).expect("invalid layout");
        // Safety: layout is valid.
        let raw = unsafe { std::alloc::alloc(layout) };
        let ptr = NonNull::new(raw).expect("alloc failed");
        Self {
            ptr,
            cap,
            init: 0,
            align,
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }

    #[inline]
    pub fn alignment(&self) -> usize {
        self.align
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.init
    }

    /// Set how many bytes are initialized.
    ///
    /// # Safety
    /// Caller must ensure `[0..len)` is initialized.
    pub unsafe fn set_len(&mut self, len: usize) {
        assert!(len <= self.cap);
        self.init = len;
    }

    /// View initialized bytes.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.init) }
    }

    /// View total capacity as mutable bytes.
    #[inline]
    pub fn as_mut_slice_total(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.cap) }
    }

    /// Convert the initialized portion into a `Vec<u8>` (copying).
    pub fn to_vec(&self) -> Vec<u8> {
        self.as_slice().to_vec()
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }
        let layout = Layout::from_size_align(self.cap, self.align).expect("invalid layout");
        unsafe {
            std::alloc::dealloc(self.ptr.as_ptr(), layout);
        }
    }
}

unsafe impl IoBuf for AlignedBuf {
    #[inline]
    fn read_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    #[inline]
    fn bytes_init(&self) -> usize {
        self.init
    }
}

unsafe impl IoBufMut for AlignedBuf {
    #[inline]
    fn write_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    #[inline]
    fn bytes_total(&mut self) -> usize {
        self.cap
    }

    #[inline]
    unsafe fn set_init(&mut self, pos: usize) {
        assert!(pos <= self.cap);
        self.init = pos;
    }
}

