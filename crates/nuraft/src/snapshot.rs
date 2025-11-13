use std::mem;
use std::ptr::NonNull;

use maniac_sys::*;

use crate::buffer::Buffer;
use crate::error::{RaftError, RaftResult};

/// Snapshot data container owned by the wrapper.
pub struct Snapshot {
    ptr: NonNull<bop_raft_snapshot>,
}

impl Snapshot {
    /// Wrap a raw pointer returned by the C API.
    pub(crate) unsafe fn from_raw(ptr: *mut bop_raft_snapshot) -> RaftResult<Self> {
        NonNull::new(ptr)
            .map(|ptr| Snapshot { ptr })
            .ok_or(RaftError::NullPointer)
    }

    /// Serialize snapshot to buffer.
    pub fn serialize(&self) -> RaftResult<Buffer> {
        let ptr = unsafe { bop_raft_snapshot_serialize(self.ptr.as_ptr()) };
        unsafe { Buffer::from_raw(ptr) }
    }

    /// Deserialize snapshot from buffer.
    pub fn deserialize(buffer: Buffer) -> RaftResult<Self> {
        let mut buffer = buffer;
        let ptr = unsafe { bop_raft_snapshot_deserialize(buffer.as_ptr()) };
        mem::forget(buffer); // Buffer ownership transferred

        unsafe { Snapshot::from_raw(ptr) }
    }

    /// Get the raw pointer (for C API interop).
    pub fn as_ptr(&self) -> *mut bop_raft_snapshot {
        self.ptr.as_ptr()
    }
}

unsafe impl Send for Snapshot {}
unsafe impl Sync for Snapshot {}
