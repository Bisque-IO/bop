use std::ffi::{CStr, CString};
use std::ptr::NonNull;

use bop_sys::*;

use crate::error::{RaftError, RaftResult};

/// Statistics counter wrapper.
pub struct Counter {
    ptr: NonNull<bop_raft_counter>,
}

impl Counter {
    /// Create a counter handle for the given metric name.
    pub fn new(name: &str) -> RaftResult<Self> {
        let c_name = CString::new(name)?;
        let ptr = unsafe { bop_raft_counter_make(c_name.as_ptr(), name.len()) };
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr })
    }

    /// Get counter name.
    pub fn name(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_counter_name(self.ptr.as_ptr());
            if ptr.is_null() {
                return Ok(String::new());
            }
            let cstr = CStr::from_ptr(ptr);
            Ok(cstr.to_str()?.to_owned())
        }
    }

    /// Get counter value.
    pub fn value(&self) -> u64 {
        unsafe { bop_raft_counter_value(self.ptr.as_ptr()) }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut bop_raft_counter {
        self.ptr.as_ptr()
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        unsafe {
            bop_raft_counter_delete(self.ptr.as_ptr());
        }
    }
}

/// Statistics gauge wrapper.
pub struct Gauge {
    ptr: NonNull<bop_raft_gauge>,
}

impl Gauge {
    /// Create a gauge handle.
    pub fn new(name: &str) -> RaftResult<Self> {
        let c_name = CString::new(name)?;
        let ptr = unsafe { bop_raft_gauge_make(c_name.as_ptr(), name.len()) };
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr })
    }

    /// Get gauge name.
    pub fn name(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_gauge_name(self.ptr.as_ptr());
            if ptr.is_null() {
                return Ok(String::new());
            }
            let cstr = CStr::from_ptr(ptr);
            Ok(cstr.to_str()?.to_owned())
        }
    }

    /// Get gauge value.
    pub fn value(&self) -> i64 {
        unsafe { bop_raft_gauge_value(self.ptr.as_ptr()) }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut bop_raft_gauge {
        self.ptr.as_ptr()
    }
}

impl Drop for Gauge {
    fn drop(&mut self) {
        unsafe {
            bop_raft_gauge_delete(self.ptr.as_ptr());
        }
    }
}

/// Statistics histogram wrapper.
pub struct Histogram {
    ptr: NonNull<bop_raft_histogram>,
}

impl Histogram {
    /// Create a histogram handle.
    pub fn new(name: &str) -> RaftResult<Self> {
        let c_name = CString::new(name)?;
        let ptr = unsafe { bop_raft_histogram_make(c_name.as_ptr(), name.len()) };
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr })
    }

    /// Get histogram name.
    pub fn name(&self) -> RaftResult<String> {
        unsafe {
            let ptr = bop_raft_histogram_name(self.ptr.as_ptr());
            if ptr.is_null() {
                return Ok(String::new());
            }
            Ok(CStr::from_ptr(ptr).to_str()?.to_owned())
        }
    }

    /// Number of tracked buckets.
    pub fn size(&self) -> usize {
        unsafe { bop_raft_histogram_size(self.ptr.as_ptr()) }
    }

    /// Read the count for a bucket capped at `upper_bound`.
    pub fn bucket(&self, upper_bound: f64) -> u64 {
        unsafe { bop_raft_histogram_get(self.ptr.as_ptr(), upper_bound) }
    }

    /// Snapshot a set of bucket boundaries.
    pub fn snapshot(&self, bounds: &[f64]) -> Vec<(f64, u64)> {
        bounds
            .iter()
            .map(|&bound| (bound, self.bucket(bound)))
            .collect()
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut bop_raft_histogram {
        self.ptr.as_ptr()
    }
}

impl Drop for Histogram {
    fn drop(&mut self) {
        unsafe {
            bop_raft_histogram_delete(self.ptr.as_ptr());
        }
    }
}
