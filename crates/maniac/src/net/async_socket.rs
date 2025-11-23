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

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use futures::task::AtomicWaker;

/// Internal state for AsyncSocketState (to allow Arc-based cloning)
#[derive(Debug)]
struct AsyncSocketStateInner {
    /// Read task waker
    read_waker: AtomicWaker,

    /// Write task waker
    write_waker: AtomicWaker,

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
/// Uses futures::task::AtomicWaker for safe and efficient waker storage.
#[derive(Clone, Debug)]
pub struct AsyncSocketState {
    inner: Arc<AsyncSocketStateInner>,
}

impl AsyncSocketState {
    /// Create a new AsyncSocketState
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AsyncSocketStateInner {
                read_waker: AtomicWaker::new(),
                write_waker: AtomicWaker::new(),
                timed_out: AtomicBool::new(false),
                closed: AtomicBool::new(false),
                token: AtomicUsize::new(usize::MAX),
            }),
        }
    }

    /// Store the read task waker for cross-worker wakeup
    pub fn set_read_waker(&self, waker: &Waker) {
        self.inner.read_waker.register(waker);
    }

    /// Store the write task waker for cross-worker wakeup
    pub fn set_write_waker(&self, waker: &Waker) {
        self.inner.write_waker.register(waker);
    }

    /// Wake the read task (called by Worker EventLoop when socket becomes readable)
    pub fn wake_read(&self) {
        self.inner.read_waker.wake();
    }

    /// Wake the write task (called by Worker EventLoop when socket becomes writable)
    pub fn wake_write(&self) {
        self.inner.write_waker.wake();
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
