use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};

/// Cache-line aligned wake counter paired with a condvar to unblock parked threads.
#[repr(align(64))]
pub struct Waker {
    counter: AtomicU64,
    mutex: Mutex<()>,
    condvar: Condvar,
}

impl Waker {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
            mutex: Mutex::new(()),
            condvar: Condvar::new(),
        }
    }

    /// Blocks the current thread while the counter remains at zero.
    pub fn wait(&self) {
        if self.counter.load(Ordering::Acquire) == 0 {
            let mut guard = self.mutex.lock().expect("waker mutex poisoned");
            while self.counter.load(Ordering::Acquire) == 0 {
                guard = self
                    .condvar
                    .wait(guard)
                    .expect("waker condvar wait poisoned");
            }
        }
    }

    /// Increments the counter and wakes all waiters when transitioning from zero.
    pub fn increment(&self) {
        if self.counter.fetch_add(1, Ordering::Release) == 0 {
            let _guard = self.mutex.lock().expect("waker mutex poisoned");
            self.condvar.notify_all();
        }
    }

    /// Decrements the counter, asserting that it was previously non-zero.
    pub fn decrement(&self) {
        let previous = self.counter.fetch_sub(1, Ordering::SeqCst);
        debug_assert!(previous > 0, "waker counter underflow");
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
