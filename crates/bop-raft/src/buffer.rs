use std::ptr::{self, NonNull};

use bop_sys::*;

use crate::error::{RaftError, RaftResult};

/// RAII wrapper for `bop_raft_buffer` with automatic cleanup.
pub struct Buffer {
    ptr: NonNull<bop_raft_buffer>,
}

impl Buffer {
    /// Create a new buffer with the specified size.
    pub fn new(size: usize) -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_buffer_new(size) };
        unsafe { Self::from_raw(ptr) }
    }

    /// Construct a buffer from a raw pointer returned by the C API.
    ///
    /// # Safety
    /// The pointer must be valid and owned by the caller. On success the
    /// returned [`Buffer`] takes ownership and will free it on drop.
    pub(crate) unsafe fn from_raw(ptr: *mut bop_raft_buffer) -> RaftResult<Self> {
        NonNull::new(ptr)
            .map(|ptr| Buffer { ptr })
            .ok_or(RaftError::NullPointer)
    }

    /// Get the raw data pointer.
    pub fn data(&mut self) -> *mut u8 {
        unsafe { bop_raft_buffer_data(self.ptr.as_ptr()) }
    }

    /// Get the container size (allocated capacity).
    pub fn container_size(&self) -> usize {
        unsafe { bop_raft_buffer_container_size(self.ptr.as_ptr()) }
    }

    /// Get the current size (amount of data).
    pub fn size(&self) -> usize {
        unsafe { bop_raft_buffer_size(self.ptr.as_ptr()) }
    }

    /// Get the current position.
    pub fn pos(&self) -> usize {
        unsafe { bop_raft_buffer_pos(self.ptr.as_ptr()) }
    }

    /// Set the current position.
    pub fn set_pos(&mut self, pos: usize) {
        unsafe { bop_raft_buffer_set_pos(self.ptr.as_ptr(), pos) }
    }

    /// Copy the buffer contents into a `Vec<u8>`.
    pub fn to_vec(&self) -> Vec<u8> {
        let size = self.size();
        let mut vec = Vec::with_capacity(size);
        unsafe {
            let data = bop_raft_buffer_data(self.ptr.as_ptr());
            ptr::copy_nonoverlapping(data, vec.as_mut_ptr(), size);
            vec.set_len(size);
        }
        vec
    }

    /// Construct a buffer from the provided byte slice by copying the data.
    pub fn from_bytes(bytes: &[u8]) -> RaftResult<Self> {
        let mut buffer = Buffer::new(bytes.len())?;
        if !bytes.is_empty() {
            unsafe {
                let dest = bop_raft_buffer_data(buffer.ptr.as_ptr());
                ptr::copy_nonoverlapping(bytes.as_ptr(), dest, bytes.len());
            }
        }
        buffer.set_pos(bytes.len());
        Ok(buffer)
    }

    /// Construct a buffer from an owned vector without additional allocation.
    pub fn from_vec(bytes: Vec<u8>) -> RaftResult<Self> {
        Buffer::from_bytes(&bytes)
    }

    /// Get the raw pointer (for C API interop).
    pub fn as_ptr(&mut self) -> *mut bop_raft_buffer {
        self.ptr.as_ptr()
    }

    /// Transfer ownership of the underlying raw pointer to the caller.
    #[allow(dead_code)]
    pub(crate) fn into_raw(self) -> *mut bop_raft_buffer {
        let ptr = self.ptr.as_ptr();
        std::mem::forget(self);
        ptr
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            bop_raft_buffer_free(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}
