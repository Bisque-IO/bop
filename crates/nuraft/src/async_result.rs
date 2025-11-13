use std::ffi::{CStr, c_void};
use std::future::Future;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

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

    /// Convert this async handle into a [`Future`] that resolves to the result.
    pub fn into_future(self) -> AsyncBoolFuture {
        AsyncBoolFuture::new(self)
    }

    fn set_callback(&mut self, user_data: *mut c_void, callback: bop_raft_async_bool_when_ready) {
        unsafe { bop_raft_async_bool_set_when_ready(self.ptr.as_ptr(), user_data, callback) }
    }

    fn clear_callback(&mut self) {
        self.set_callback(ptr::null_mut(), None);
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

/// Future representing the completion of an [`AsyncBool`] command.
pub struct AsyncBoolFuture {
    handle: Option<AsyncBool>,
    shared: Arc<AsyncResultShared<bool>>,
    raw_shared: *const AsyncResultShared<bool>,
}

impl AsyncBoolFuture {
    fn new(mut handle: AsyncBool) -> Self {
        let shared = Arc::new(AsyncResultShared::new());
        let raw_shared = Arc::into_raw(shared.clone());
        let user_data = raw_shared as *mut c_void;
        handle.set_callback(user_data, Some(async_bool_ready));

        Self {
            handle: Some(handle),
            shared,
            raw_shared,
        }
    }
}

impl Future for AsyncBoolFuture {
    type Output = RaftResult<bool>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.shared.poll(cx) {
            Poll::Ready(result) => {
                self.handle.take();
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for AsyncBoolFuture {
    fn drop(&mut self) {
        if let Some(mut handle) = self.handle.take() {
            handle.clear_callback();
        }
        if !self.raw_shared.is_null() {
            unsafe {
                Arc::from_raw(self.raw_shared);
            }
            self.raw_shared = ptr::null();
        }
    }
}

impl Unpin for AsyncBoolFuture {}

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

    /// Convert this async handle into a [`Future`] that resolves to the result.
    pub fn into_future(self) -> AsyncU64Future {
        AsyncU64Future::new(self)
    }

    fn set_callback(&mut self, user_data: *mut c_void, callback: bop_raft_async_u64_when_ready) {
        unsafe { bop_raft_async_u64_set_when_ready(self.ptr.as_ptr(), user_data, callback) }
    }

    fn clear_callback(&mut self) {
        self.set_callback(ptr::null_mut(), None);
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

/// Future representing the completion of an [`AsyncU64`] command.
pub struct AsyncU64Future {
    handle: Option<AsyncU64>,
    shared: Arc<AsyncResultShared<u64>>,
    raw_shared: *const AsyncResultShared<u64>,
}

impl AsyncU64Future {
    fn new(mut handle: AsyncU64) -> Self {
        let shared = Arc::new(AsyncResultShared::new());
        let raw_shared = Arc::into_raw(shared.clone());
        let user_data = raw_shared as *mut c_void;
        handle.set_callback(user_data, Some(async_u64_ready));

        Self {
            handle: Some(handle),
            shared,
            raw_shared,
        }
    }
}

impl Future for AsyncU64Future {
    type Output = RaftResult<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.shared.poll(cx) {
            Poll::Ready(result) => {
                self.handle.take();
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for AsyncU64Future {
    fn drop(&mut self) {
        if let Some(mut handle) = self.handle.take() {
            handle.clear_callback();
        }
        if !self.raw_shared.is_null() {
            unsafe {
                Arc::from_raw(self.raw_shared);
            }
            self.raw_shared = ptr::null();
        }
    }
}

impl Unpin for AsyncU64Future {}

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
        unsafe { Buffer::from_raw(ptr) }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut bop_raft_async_buffer_ptr {
        self.ptr.as_ptr()
    }

    /// Convert this async handle into a [`Future`] that resolves to the result buffer.
    pub fn into_future(self) -> AsyncBufferFuture {
        AsyncBufferFuture::new(self)
    }

    fn set_callback(&mut self, user_data: *mut c_void, callback: bop_raft_async_buffer_when_ready) {
        unsafe { bop_raft_async_buffer_set_when_ready(self.ptr.as_ptr(), user_data, callback) }
    }

    fn clear_callback(&mut self) {
        self.set_callback(ptr::null_mut(), None);
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

/// Future representing the completion of an [`AsyncBuffer`] command.
pub struct AsyncBufferFuture {
    handle: Option<AsyncBuffer>,
    shared: Arc<AsyncResultShared<Buffer>>,
    raw_shared: *const AsyncResultShared<Buffer>,
}

impl AsyncBufferFuture {
    fn new(mut handle: AsyncBuffer) -> Self {
        let shared = Arc::new(AsyncResultShared::new());
        let raw_shared = Arc::into_raw(shared.clone());
        let user_data = raw_shared as *mut c_void;
        handle.set_callback(user_data, Some(async_buffer_ready));

        Self {
            handle: Some(handle),
            shared,
            raw_shared,
        }
    }
}

impl Future for AsyncBufferFuture {
    type Output = RaftResult<Buffer>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.shared.poll(cx) {
            Poll::Ready(result) => {
                self.handle.take();
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for AsyncBufferFuture {
    fn drop(&mut self) {
        if let Some(mut handle) = self.handle.take() {
            handle.clear_callback();
        }
        if !self.raw_shared.is_null() {
            unsafe {
                Arc::from_raw(self.raw_shared);
            }
            self.raw_shared = ptr::null();
        }
    }
}

impl Unpin for AsyncBufferFuture {}

struct AsyncResultShared<T> {
    state: Mutex<AsyncResultState<T>>,
}

struct AsyncResultState<T> {
    result: Option<RaftResult<T>>,
    waker: Option<Waker>,
}

impl<T> AsyncResultShared<T> {
    fn new() -> Self {
        Self {
            state: Mutex::new(AsyncResultState {
                result: None,
                waker: None,
            }),
        }
    }

    fn poll(&self, cx: &mut Context<'_>) -> Poll<RaftResult<T>> {
        let mut guard = self.state.lock().expect("async result mutex poisoned");
        if let Some(result) = guard.result.take() {
            guard.waker = None;
            Poll::Ready(result)
        } else {
            guard.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }

    fn complete(&self, value: RaftResult<T>) {
        let waker = {
            let mut guard = self.state.lock().expect("async result mutex poisoned");
            guard.result = Some(value);
            guard.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

fn make_command_error(error: *const i8) -> Option<RaftError> {
    if error.is_null() {
        None
    } else {
        let message = unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned();
        Some(RaftError::CommandError(message))
    }
}

unsafe extern "C" fn async_bool_ready(user_data: *mut c_void, value: bool, error: *const i8) {
    let shared = unsafe { Arc::from_raw(user_data as *const AsyncResultShared<bool>) };
    let outcome = match make_command_error(error) {
        Some(err) => Err(err),
        None => Ok(value),
    };
    shared.complete(outcome);
    let _ = Arc::into_raw(shared);
}

unsafe extern "C" fn async_u64_ready(user_data: *mut c_void, value: u64, error: *const i8) {
    let shared = unsafe { Arc::from_raw(user_data as *const AsyncResultShared<u64>) };
    let outcome = match make_command_error(error) {
        Some(err) => Err(err),
        None => Ok(value),
    };
    shared.complete(outcome);
    let _ = Arc::into_raw(shared);
}

unsafe extern "C" fn async_buffer_ready(
    user_data: *mut c_void,
    result: *mut bop_raft_buffer,
    error: *const i8,
) {
    let shared = unsafe { Arc::from_raw(user_data as *const AsyncResultShared<Buffer>) };
    let outcome = match make_command_error(error) {
        Some(err) => Err(err),
        None => {
            if result.is_null() {
                Buffer::from_bytes(&[])
            } else {
                unsafe { AsyncBuffer::wrap_result(result) }
            }
        }
    };
    shared.complete(outcome);
    let _ = Arc::into_raw(shared);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bop_sys::{
        bop_raft_async_bool_get_user_data, bop_raft_async_bool_get_when_ready,
        bop_raft_async_buffer_get_user_data, bop_raft_async_buffer_get_when_ready,
        bop_raft_async_u64_get_user_data, bop_raft_async_u64_get_when_ready,
    };
    use std::ffi::CString;
    use std::future::Future;
    use std::pin::Pin;
    use std::ptr;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn noop_waker() -> Waker {
        unsafe fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(ptr::null(), &VTABLE)
        }
        unsafe fn wake(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake, wake);
        let raw = RawWaker::new(ptr::null(), &VTABLE);
        unsafe { Waker::from_raw(raw) }
    }

    fn poll_once<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        Pin::new(future).poll(&mut cx)
    }

    #[test]
    fn async_bool_future_resolves() {
        let handle = AsyncBool::new(ptr::null_mut(), None).unwrap();
        let mut future = handle.into_future();

        assert!(matches!(poll_once(&mut future), Poll::Pending));

        let handle_ref = future.handle.as_ref().expect("handle present");
        let ptr = handle_ref.ptr.as_ptr();
        let callback = unsafe { bop_raft_async_bool_get_when_ready(ptr) };
        let user_data = unsafe { bop_raft_async_bool_get_user_data(ptr) };
        assert!(callback.is_some());
        assert!(!user_data.is_null());
        if let Some(cb) = callback {
            unsafe { cb(user_data, true, ptr::null()) };
        }

        match poll_once(&mut future) {
            Poll::Ready(Ok(value)) => assert!(value),
            other => panic!("unexpected poll result: {:?}", other),
        }
    }

    #[test]
    fn async_u64_future_propagates_error() {
        let handle = AsyncU64::new(ptr::null_mut(), None).unwrap();
        let mut future = handle.into_future();

        assert!(matches!(poll_once(&mut future), Poll::Pending));

        let handle_ref = future.handle.as_ref().expect("handle present");
        let ptr = handle_ref.ptr.as_ptr();
        let callback = unsafe { bop_raft_async_u64_get_when_ready(ptr) };
        let user_data = unsafe { bop_raft_async_u64_get_user_data(ptr) };
        let error = CString::new("boom").unwrap();
        if let Some(cb) = callback {
            unsafe { cb(user_data, 0, error.as_ptr()) };
        }

        match poll_once(&mut future) {
            Poll::Ready(Err(RaftError::CommandError(msg))) => assert!(msg.contains("boom")),
            other => panic!("unexpected poll result: {:?}", other),
        }
    }

    #[test]
    fn async_buffer_future_returns_payload() {
        let handle = AsyncBuffer::new(ptr::null_mut(), None).unwrap();
        let mut future = handle.into_future();

        assert!(matches!(poll_once(&mut future), Poll::Pending));

        let handle_ref = future.handle.as_ref().expect("handle present");
        let ptr = handle_ref.ptr.as_ptr();
        let callback = unsafe { bop_raft_async_buffer_get_when_ready(ptr) };
        let user_data = unsafe { bop_raft_async_buffer_get_user_data(ptr) };
        let buffer = Buffer::from_bytes(b"hello").unwrap();
        let raw_buffer = buffer.into_raw();
        if let Some(cb) = callback {
            unsafe { cb(user_data, raw_buffer, ptr::null()) };
        }

        match poll_once(&mut future) {
            Poll::Ready(Ok(buffer)) => assert_eq!(buffer.to_vec(), b"hello"),
            Poll::Ready(Err(err)) => panic!("unexpected error: {err}"),
            Poll::Pending => panic!("future still pending"),
        }
    }
}
