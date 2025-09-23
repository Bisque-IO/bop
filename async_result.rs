use std::ffi::c_void;
use std::ptr::NonNull;

use bop_sys::*;

use crate::buffer::Buffer;
use crate::error::{RaftError, RaftResult};

/// Async result wrapper for boolean command results.
pub struct AsyncBool {
    ptr: NonNull<bop_raft_async_bool_ptr>,
}

impl AsyncBool {
    /// Create a new async boolean result handle.
    pub fn new(
        user_data: *mut c_void,
        when_ready: Option<unsafe extern "C" fn(*mut c_void, bool, *const i8)>,
    ) -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_async_bool_make(user_data, when_ready) };

        NonNull::new(ptr)
            .map(|ptr| AsyncBool { ptr })
            .ok_or(RaftError::NullPointer)
    }

    /// Get user data pointer stored alongside the result.
    pub fn user_data(&self) -> *mut c_void {
        unsafe { bop_raft_async_bool_get_user_data(self.ptr.as_ptr()) }
    }

    /// Replace user data pointer.
    pub fn set_user_data(&mut self, data: *mut c_void) {
        unsafe { bop_raft_async_bool_set_user_data(self.ptr.as_ptr(), data) }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut bop_raft_async_bool_ptr {
        self.ptr.as_ptr()
    }
}

impl Drop for AsyncBool {
    fn drop(&mut self) {
        unsafe {
            bop_raft_async_bool_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for AsyncBool {}
unsafe impl Sync for AsyncBool {}

/// Async result wrapper that delivers a u64 payload.
pub struct AsyncU64 {
    ptr: NonNull<bop_raft_async_u64_ptr>,
}

impl AsyncU64 {
    /// Create a new async u64 result handle.
    pub fn new(
        user_data: *mut c_void,
        when_ready: Option<unsafe extern "C" fn(*mut c_void, u64, *const i8)>,
    ) -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_async_u64_make(user_data, when_ready) };
        NonNull::new(ptr)
            .map(|ptr| AsyncU64 { ptr })
            .ok_or(RaftError::NullPointer)
    }

    pub fn user_data(&self) -> *mut c_void {
        unsafe { bop_raft_async_u64_get_user_data(self.ptr.as_ptr()) }
    }

    pub fn set_user_data(&mut self, data: *mut c_void) {
        unsafe { bop_raft_async_u64_set_user_data(self.ptr.as_ptr(), data) }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut bop_raft_async_u64_ptr {
        self.ptr.as_ptr()
    }
}

impl Drop for AsyncU64 {
    fn drop(&mut self) {
        unsafe {
            bop_raft_async_u64_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for AsyncU64 {}
unsafe impl Sync for AsyncU64 {}

/// Async result wrapper that delivers a buffer payload.
pub struct AsyncBuffer {
    ptr: NonNull<bop_raft_async_buffer_ptr>,
}

impl AsyncBuffer {
    /// Create a new async buffer result handle.
    pub fn new(
        user_data: *mut c_void,
        when_ready: Option<unsafe extern "C" fn(*mut c_void, *mut bop_raft_buffer, *const i8)>,
    ) -> RaftResult<Self> {
        let ptr = unsafe { bop_raft_async_buffer_make(user_data, when_ready) };
        NonNull::new(ptr)
            .map(|ptr| AsyncBuffer { ptr })
            .ok_or(RaftError::NullPointer)
    }

    pub fn user_data(&self) -> *mut c_void {
        unsafe { bop_raft_async_buffer_get_user_data(self.ptr.as_ptr()) }
    }

    pub fn set_user_data(&mut self, data: *mut c_void) {
        unsafe { bop_raft_async_buffer_set_user_data(self.ptr.as_ptr(), data) }
    }

    /// Convert a raw buffer pointer handed over by the callback into a managed [`Buffer`].
    ///
    /// # Safety
    /// The pointer must originate from the NuRaft callback and ownership must be
    /// transferred to the caller.
    pub unsafe fn wrap_result(ptr: *mut bop_raft_buffer) -> RaftResult<Buffer> {
        Buffer::from_raw(ptr)
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut bop_raft_async_buffer_ptr {
        self.ptr.as_ptr()
    }
}

impl Drop for AsyncBuffer {
    fn drop(&mut self) {
        unsafe {
            bop_raft_async_buffer_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for AsyncBuffer {}
unsafe impl Sync for AsyncBuffer {}
