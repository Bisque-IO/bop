use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::signal::CACHE_LINE_SIZE;

/// Cache-line aligned wake counter paired with a condvar to unblock parked threads.
#[repr(align(64))]
pub struct Waker {
    counter: AtomicU64,
    _pad: [u8; CACHE_LINE_SIZE - 8],
    mutex: Mutex<()>,
    condvar: Condvar,
}

impl Waker {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
            _pad: [0; CACHE_LINE_SIZE - 8],
            mutex: Mutex::new(()),
            condvar: Condvar::new(),
        }
    }

    /// Blocks the current thread while the counter remains at zero.
    pub fn wait(&self) {
        // Hold mutex while checking and waiting to avoid lost wakeups
        let mut guard = self.mutex.lock().expect("waker mutex poisoned");
        while self.counter.load(Ordering::Acquire) == 0 {
            guard = self
                .condvar
                .wait(guard)
                .expect("waker condvar wait poisoned");
        }
    }

    pub fn wake(&self) {
        let _guard = self.mutex.lock().expect("waker mutex poisoned");
        self.condvar.notify_all();
    }

    /// Blocks the current thread while the counter remains at zero
    /// for up to the timeout duration.
    ///
    /// Returns `true` if the counter was incremented, `false` if the timeout was reached.
    pub fn wait_timeout_ms(&self, timeout: Duration) -> bool {
        // Hold mutex while checking and waiting to avoid lost wakeups
        let mut guard = self.mutex.lock().expect("waker mutex poisoned");
        while self.counter.load(Ordering::Acquire) == 0 {
            let (next_guard, result) = self
                .condvar
                .wait_timeout(guard, timeout)
                .expect("waker condvar wait poisoned");
            if result.timed_out() {
                return false;
            }
            guard = next_guard;
        }
        true
    }

    /// Increments the counter and wakes all waiters when transitioning from zero.
    pub fn increment(&self) {
        let previous = self.counter.fetch_add(1, Ordering::Release);
        if previous == 0 {
            let _guard = self.mutex.lock().expect("waker mutex poisoned");
            self.condvar.notify_all();
        }
    }

    /// Decrements the counter, asserting that it was previously non-zero.
    pub fn decrement(&self) {
        self.counter.fetch_sub(1, Ordering::Release);
    }

    pub fn load(&self) -> u64 {
        self.counter.load(Ordering::Acquire)
    }
}

impl Default for Waker {
    fn default() -> Self {
        Self::new()
    }
}
