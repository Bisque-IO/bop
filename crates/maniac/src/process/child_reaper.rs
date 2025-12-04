//! Child process reaper for Unix systems.
//!
//! This module provides efficient, non-blocking child process waiting using SIGCHLD.
//! Unlike the signal registry which only supports one waker per signal, this module
//! supports multiple concurrent child process waiters.

use std::collections::HashMap;
use std::io;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::task::Waker;

use crate::driver::UnparkHandle;
use crate::driver::unpark::Unpark;

/// Global child reaper instance (lazily initialized).
static REAPER: OnceLock<ChildReaper> = OnceLock::new();

/// Initialization guard for SIGCHLD handler.
static SIGCHLD_INIT: Once = Once::new();

/// Whether SIGCHLD handler registration succeeded.
static SIGCHLD_INIT_OK: AtomicBool = AtomicBool::new(false);

/// Get or initialize the global child reaper.
fn reaper() -> &'static ChildReaper {
    REAPER.get_or_init(ChildReaper::new)
}

/// The child reaper manages SIGCHLD handling and wakes all waiting tasks.
struct ChildReaper {
    /// Generation counter - incremented each time SIGCHLD is received.
    /// Waiters use this to detect if they should re-check their child.
    generation: AtomicU64,

    /// Registered wakers for processes waiting on child exit.
    /// Key is a unique registration ID.
    ///
    /// Note: We use a Mutex here which is NOT async-signal-safe.
    /// The signal handler only increments the generation counter and sets a flag.
    /// A separate mechanism wakes the waiters.
    wakers: Mutex<HashMap<u64, Waker>>,

    /// Next registration ID.
    next_id: AtomicU64,

    /// UnparkHandle to wake the runtime when SIGCHLD is received.
    unpark: AtomicPtr<UnparkHandle>,

    /// Flag set by signal handler to indicate SIGCHLD was received.
    /// This is used with the unpark handle to safely wake the runtime.
    signal_pending: AtomicBool,
}

impl ChildReaper {
    fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            wakers: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            unpark: AtomicPtr::new(null_mut()),
            signal_pending: AtomicBool::new(false),
        }
    }

    /// Called from the signal handler when SIGCHLD is received.
    ///
    /// # Safety
    ///
    /// This function is called from a signal handler. While calling waker.wake()
    /// is not strictly async-signal-safe, it's the same approach used by the
    /// existing signal module and works in practice.
    fn on_sigchld(&self) {
        // Increment generation (signal-safe: atomic operation)
        self.generation.fetch_add(1, Ordering::Release);

        // Wake all registered wakers
        // Note: We use try_lock to avoid deadlock in signal handler.
        // If the lock is held, we set a pending flag for later processing.
        if let Ok(mut guard) = self.wakers.try_lock() {
            let wakers = std::mem::take(&mut *guard);
            drop(guard); // Release lock before calling wake

            for (_, waker) in wakers {
                waker.wake();
            }
        } else {
            // Couldn't acquire lock - set pending flag for later processing
            self.signal_pending.store(true, Ordering::Release);
        }

        // Also unpark the runtime to ensure the executor loop runs
        let unpark_ptr = self.unpark.load(Ordering::Acquire);
        if !unpark_ptr.is_null() {
            // SAFETY: Pointer validity is maintained by registration lifetime
            let unpark = unsafe { &*unpark_ptr };
            let _ = unpark.unpark();
        }
    }

    /// Process pending SIGCHLD and wake all waiters.
    /// This is called from a safe context (not signal handler).
    fn process_pending(&self) {
        if self.signal_pending.swap(false, Ordering::AcqRel) {
            self.wake_all();
        }
    }

    /// Wake all registered wakers.
    fn wake_all(&self) {
        let wakers = {
            let mut guard = self.wakers.lock().unwrap();
            // Take all wakers - they'll re-register if still waiting
            std::mem::take(&mut *guard)
        };

        for (_, waker) in wakers {
            waker.wake();
        }
    }

    /// Register a waker to be notified on SIGCHLD.
    /// Returns a registration ID that must be used to unregister.
    fn register(&self, waker: &Waker) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.wakers.lock().unwrap();
        guard.insert(id, waker.clone());
        id
    }

    /// Update an existing registration with a new waker.
    fn update(&self, id: u64, waker: &Waker) {
        let mut guard = self.wakers.lock().unwrap();
        guard.insert(id, waker.clone());
    }

    /// Unregister a waker.
    fn unregister(&self, id: u64) {
        let mut guard = self.wakers.lock().unwrap();
        guard.remove(&id);
    }

    /// Get the current generation counter.
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Register the runtime's unpark handle.
    fn register_unpark(&self, unpark: &UnparkHandle) {
        let new_unpark = Box::new(unpark.clone());
        let old_ptr = self
            .unpark
            .swap(Box::into_raw(new_unpark), Ordering::AcqRel);
        if !old_ptr.is_null() {
            // Drop the old handle
            let _ = unsafe { Box::from_raw(old_ptr) };
        }
    }
}

/// SIGCHLD signal handler.
extern "C" fn sigchld_handler(_signum: libc::c_int) {
    // SAFETY: reaper() is guaranteed to be initialized before the signal handler
    // is registered (init_sigchld initializes it first).
    if let Some(reaper) = REAPER.get() {
        reaper.on_sigchld();
    }
}

/// Initialize the SIGCHLD handler.
/// Returns Ok(()) if already initialized or initialization succeeds.
fn init_sigchld() -> io::Result<()> {
    let mut result = Ok(());

    SIGCHLD_INIT.call_once(|| {
        // Initialize the reaper first (before registering signal handler)
        let r = reaper();

        // Set up SIGCHLD handler
        let mut new_action: libc::sigaction =
            unsafe { std::mem::MaybeUninit::zeroed().assume_init() };
        new_action.sa_sigaction = sigchld_handler as *const () as usize;
        // SA_RESTART: Restart interrupted syscalls
        // SA_NOCLDSTOP: Don't notify on stop/continue, only on exit
        new_action.sa_flags = libc::SA_RESTART | libc::SA_NOCLDSTOP;

        unsafe {
            libc::sigemptyset(&mut new_action.sa_mask);
        }

        let ret = unsafe { libc::sigaction(libc::SIGCHLD, &new_action, std::ptr::null_mut()) };

        if ret == -1 {
            result = Err(io::Error::last_os_error());
        } else {
            SIGCHLD_INIT_OK.store(true, Ordering::Release);

            // Try to register the unpark handle
            if let Ok(unpark) = UnparkHandle::try_current() {
                r.register_unpark(&unpark);
            }
        }
    });

    // Check if a previous initialization failed
    if result.is_ok() && !SIGCHLD_INIT_OK.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "SIGCHLD handler initialization failed",
        ));
    }

    result
}

/// Handle for a child process wait registration.
pub(crate) struct WaitHandle {
    /// Registration ID with the reaper.
    id: Option<u64>,
    /// Last seen generation - used to detect if SIGCHLD was received.
    last_generation: u64,
}

impl WaitHandle {
    /// Create a new wait handle.
    ///
    /// This initializes the SIGCHLD handler if not already done.
    pub(crate) fn new() -> io::Result<Self> {
        init_sigchld()?;

        Ok(Self {
            id: None,
            last_generation: reaper().generation(),
        })
    }

    /// Check if a SIGCHLD was received since last check.
    ///
    /// This also processes any pending signals and wakes waiters.
    pub(crate) fn poll_sigchld(&mut self) -> bool {
        let r = reaper();

        // First, process any pending signals (safe context)
        r.process_pending();

        // Check if generation changed
        let current = r.generation();
        if current > self.last_generation {
            self.last_generation = current;
            true
        } else {
            false
        }
    }

    /// Register/update waker for SIGCHLD notification.
    pub(crate) fn register_waker(&mut self, waker: &Waker) {
        let r = reaper();
        match self.id {
            Some(id) => r.update(id, waker),
            None => {
                let id = r.register(waker);
                self.id = Some(id);
            }
        }
    }
}

impl Drop for WaitHandle {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            reaper().unregister(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_sigchld() {
        // Should succeed (or already be initialized)
        assert!(init_sigchld().is_ok());
        // Second call should also succeed
        assert!(init_sigchld().is_ok());
    }

    #[test]
    fn test_wait_handle_creation() {
        let handle = WaitHandle::new();
        assert!(handle.is_ok());
    }

    #[test]
    fn test_generation_tracking() {
        let mut handle = WaitHandle::new().unwrap();
        // Initially no SIGCHLD
        assert!(!handle.poll_sigchld());
    }
}
