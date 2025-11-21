//! Async socket state for EventLoop integration
//!
//! This module provides state tracking and waking for async sockets.
//! The EventLoop monitors socket readiness and wakes tasks when sockets become ready.
//! Futures perform I/O directly using non-blocking syscalls with user-provided buffers.
//!
//! NOTE: This is designed for a work-stealing runtime where tasks can migrate between
//! workers. The EventLoop wakes tasks cross-worker when sockets become ready.

use std::sync::Arc;
use std::task::Waker;

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

/// Internal state for AsyncSocketState (to allow Arc-based cloning)
#[derive(Debug)]
struct AsyncSocketStateInner {
    /// Read task waker (stored as RawWaker data pointer)
    read_waker_data: AtomicPtr<()>,

    /// Write task waker (stored as RawWaker data pointer)
    write_waker_data: AtomicPtr<()>,

    /// Timeout occurred flag (atomic for lock-free access)
    timed_out: AtomicBool,

    /// Closed flag (EOF received or socket closed) (atomic for lock-free access)
    closed: AtomicBool,

    /// Token assigned by EventLoop (usize::MAX if not set)
    token: AtomicUsize,
}

/// State shared between async socket Futures and the Worker EventLoop
///
/// This structure tracks socket state and stores wakers for cross-worker communication.
/// Separate read/write wakers since they may be on different tasks.
/// NO data buffering - user code provides buffers when calling read/write.
///
/// Waker storage is lock-free using AtomicPtr - we only store the data pointer from
/// RawWaker and reconstruct using Task::waker_clone when waking.
#[derive(Clone, Debug)]
pub struct AsyncSocketState {
    inner: Arc<AsyncSocketStateInner>,
}

impl AsyncSocketState {
    /// Create a new AsyncSocketState
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AsyncSocketStateInner {
                read_waker_data: AtomicPtr::new(std::ptr::null_mut()),
                write_waker_data: AtomicPtr::new(std::ptr::null_mut()),
                timed_out: AtomicBool::new(false),
                closed: AtomicBool::new(false),
                token: AtomicUsize::new(usize::MAX),
            }),
        }
    }

    /// Store the read task waker for cross-worker wakeup
    /// Extracts and stores only the data pointer from the waker
    pub fn set_read_waker(&self, waker: &Waker) {
        // SAFETY: Waker is 2 pointers (data + vtable)
        let data = unsafe {
            let waker_ptr = waker as *const Waker;
            *(waker_ptr as *const *const ())
        };
        self.inner
            .read_waker_data
            .store(data as *mut (), Ordering::Release);
    }

    /// Store the write task waker for cross-worker wakeup
    /// Extracts and stores only the data pointer from the waker
    pub fn set_write_waker(&self, waker: &Waker) {
        // SAFETY: Waker is 2 pointers (data + vtable)
        let data = unsafe {
            let waker_ptr = waker as *const Waker;
            *(waker_ptr as *const *const ())
        };
        self.inner
            .write_waker_data
            .store(data as *mut (), Ordering::Release);
    }

    /// Wake the read task (called by Worker EventLoop when socket becomes readable)
    pub fn wake_read(&self) {
        let data = self.inner.read_waker_data.load(Ordering::Acquire);
        if !data.is_null() {
            // Reconstruct waker from data pointer using Task::waker_clone
            let raw_waker = unsafe { crate::runtime::task::Task::waker_clone(data) };
            let waker = unsafe { Waker::from_raw(raw_waker) };
            waker.wake();
        }
    }

    /// Wake the write task (called by Worker EventLoop when socket becomes writable)
    pub fn wake_write(&self) {
        let data = self.inner.write_waker_data.load(Ordering::Acquire);
        if !data.is_null() {
            // Reconstruct waker from data pointer using Task::waker_clone
            let raw_waker = unsafe { crate::runtime::task::Task::waker_clone(data) };
            let waker = unsafe { Waker::from_raw(raw_waker) };
            waker.wake();
        }
    }

    /// Wake both read and write tasks
    pub fn wake_all(&self) {
        self.wake_read();
        self.wake_write();
    }

    /// Check if timeout occurred
    pub fn is_timed_out(&self) -> bool {
        self.inner.timed_out.load(Ordering::Acquire)
    }

    /// Set timeout occurred flag
    pub fn set_timed_out(&self, val: bool) {
        self.inner.timed_out.store(val, Ordering::Release);
    }

    /// Check if socket is closed
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    /// Set closed flag
    pub fn set_closed(&self, val: bool) {
        self.inner.closed.store(val, Ordering::Release);
    }

    /// Get the token
    pub fn token(&self) -> Option<usize> {
        let val = self.inner.token.load(Ordering::Acquire);
        if val == usize::MAX { None } else { Some(val) }
    }

    /// Set the token
    pub fn set_token(&self, token: usize) {
        self.inner.token.store(token, Ordering::Release);
    }
}

impl Default for AsyncSocketState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_socket_state_creation() {
        let state = AsyncSocketState::new();
        assert!(!state.is_timed_out());
        assert!(!state.is_closed());
    }

    #[test]
    fn test_state_flags() {
        let state = AsyncSocketState::new();

        state.set_timed_out(true);
        assert!(state.is_timed_out());

        state.set_closed(true);
        assert!(state.is_closed());
    }

    #[test]
    fn test_token() {
        let state = AsyncSocketState::new();
        assert_eq!(state.token(), None);

        state.set_token(123);
        assert_eq!(state.token(), Some(123));
    }
}
