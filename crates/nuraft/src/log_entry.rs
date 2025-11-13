use std::mem;
use std::ptr::NonNull;

use maniac_sys::*;

use crate::buffer::Buffer;
use crate::error::{RaftError, RaftResult};
use crate::types::Term;

/// Log entry data owned by the wrapper.
pub struct LogEntry {
    ptr: NonNull<bop_raft_log_entry>,
}

impl LogEntry {
    /// Create a new log entry.
    pub fn new(term: Term, buffer: Buffer, timestamp: u64) -> RaftResult<Self> {
        Self::from_parts(term, buffer, timestamp, None)
    }

    /// Create a new log entry including optional CRC metadata.
    pub fn with_crc32(
        term: Term,
        buffer: Buffer,
        timestamp: u64,
        crc32: Option<u32>,
    ) -> RaftResult<Self> {
        Self::from_parts(term, buffer, timestamp, crc32)
    }

    fn from_parts(
        term: Term,
        mut buffer: Buffer,
        timestamp: u64,
        crc32: Option<u32>,
    ) -> RaftResult<Self> {
        let (has_crc32, crc32_value) = match crc32 {
            Some(value) => (true, value),
            None => (false, 0),
        };
        let ptr = unsafe {
            bop_raft_log_entry_make(term.0, buffer.as_ptr(), timestamp, has_crc32, crc32_value)
        };
        mem::forget(buffer); // Buffer ownership transferred to C++

        NonNull::new(ptr)
            .map(|ptr| LogEntry { ptr })
            .ok_or(RaftError::NullPointer)
    }

    /// Get the raw pointer (for C API interop).
    pub fn as_ptr(&self) -> *const bop_raft_log_entry {
        self.ptr.as_ptr()
    }

    /// Transfer ownership of the underlying pointer back to the caller.
    pub(crate) fn into_raw(self) -> *mut bop_raft_log_entry {
        let ptr = self.ptr.as_ptr();
        mem::forget(self);
        ptr
    }
}

impl Drop for LogEntry {
    fn drop(&mut self) {
        unsafe {
            bop_raft_log_entry_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for LogEntry {}
unsafe impl Sync for LogEntry {}

/// Borrowed view over a log entry payload supplied by the C callbacks.
#[derive(Clone, Copy, Debug)]
pub struct LogEntryView<'a> {
    pub term: Term,
    pub timestamp: u64,
    pub payload: &'a [u8],
    pub crc32: Option<u32>,
}

impl<'a> LogEntryView<'a> {
    pub fn new(term: Term, timestamp: u64, payload: &'a [u8], crc32: Option<u32>) -> Self {
        Self {
            term,
            timestamp,
            payload,
            crc32,
        }
    }
}

/// Owned representation of a Raft log entry used by trait implementations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntryRecord {
    pub term: Term,
    pub timestamp: u64,
    pub payload: Vec<u8>,
    pub crc32: Option<u32>,
}

impl LogEntryRecord {
    pub fn new(term: Term, timestamp: u64, payload: Vec<u8>, crc32: Option<u32>) -> Self {
        Self {
            term,
            timestamp,
            payload,
            crc32,
        }
    }

    pub fn from_view(view: LogEntryView<'_>) -> Self {
        Self::new(view.term, view.timestamp, view.payload.to_vec(), view.crc32)
    }

    pub fn as_view(&self) -> LogEntryView<'_> {
        LogEntryView {
            term: self.term,
            timestamp: self.timestamp,
            payload: &self.payload,
            crc32: self.crc32,
        }
    }

    pub fn into_ffi_entry(self) -> RaftResult<LogEntry> {
        let buffer = Buffer::from_vec(self.payload)?;
        LogEntry::with_crc32(self.term, buffer, self.timestamp, self.crc32)
    }
}
