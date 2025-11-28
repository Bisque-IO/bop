//! Global signal handler registry.
//!
//! This module provides a thread-safe registry for managing signal handlers
//! and waking async tasks when signals are received.

use std::collections::HashMap;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::task::Waker;

use crate::driver::{UnparkHandle, unpark::Unpark};

/// Maximum number of signals we support.
/// On Unix, signals are typically 1-31 (standard) + 32-64 (real-time).
/// On Windows, control events are 0-4.
const MAX_SIGNALS: usize = 64;

/// Global registry for signal handlers.
static REGISTRY: GlobalRegistry = GlobalRegistry::new();

/// Thread-safe global registry for signal handlers.
pub(crate) struct GlobalRegistry {
    /// Per-signal state. Each entry tracks whether a signal has been received
    /// and stores the waker for the listening task.
    signals: [SignalState; MAX_SIGNALS],
    /// Counter incremented each time any signal is received.
    /// Used for efficient polling.
    generation: AtomicU64,
    /// Initialization flag for platform-specific setup.
    init: Once,
}

/// State for a single signal.
struct SignalState {
    /// Atomic flag indicating if the signal has been received.
    /// Uses a counter to track multiple deliveries.
    pending: AtomicU64,
    /// Waker for the task waiting on this signal.
    /// NULL if no task is waiting.
    waker: AtomicPtr<Waker>,
    /// UnparkHandle to wake the runtime.
    unpark: AtomicPtr<UnparkHandle>,
}

impl SignalState {
    const fn new() -> Self {
        Self {
            pending: AtomicU64::new(0),
            waker: AtomicPtr::new(null_mut()),
            unpark: AtomicPtr::new(null_mut()),
        }
    }

    /// Mark this signal as pending and wake any waiting task.
    ///
    /// # Safety
    ///
    /// This function is called from signal handlers and must be async-signal-safe.
    fn notify(&self) {
        // Increment pending count
        self.pending.fetch_add(1, Ordering::Release);

        // Wake the task if one is waiting
        let waker_ptr = self.waker.swap(null_mut(), Ordering::AcqRel);
        if !waker_ptr.is_null() {
            // SAFETY: We have exclusive ownership of the waker pointer after the swap
            let waker = unsafe { Box::from_raw(waker_ptr) };
            waker.wake();
        }

        // Unpark the runtime
        let unpark_ptr = self.unpark.load(Ordering::Acquire);
        if !unpark_ptr.is_null() {
            // SAFETY: The pointer is valid as long as the Signal exists
            let unpark = unsafe { &*unpark_ptr };
            let _ = unpark.unpark();
        }
    }

    /// Try to consume a pending signal.
    ///
    /// Returns `true` if a signal was consumed, `false` if no signal is pending.
    fn try_recv(&self, last_seen: &mut u64) -> bool {
        let current = self.pending.load(Ordering::Acquire);
        if current > *last_seen {
            *last_seen = current;
            true
        } else {
            false
        }
    }

    /// Register a waker to be notified when the signal is received.
    fn register_waker(&self, waker: &Waker) {
        let new_waker = Box::new(waker.clone());
        let old_waker_ptr = self.waker.swap(Box::into_raw(new_waker), Ordering::AcqRel);
        if !old_waker_ptr.is_null() {
            // Drop the old waker
            let _ = unsafe { Box::from_raw(old_waker_ptr) };
        }
    }

    /// Register an UnparkHandle to wake the runtime.
    fn register_unpark(&self, unpark: &UnparkHandle) {
        let new_unpark = Box::new(unpark.clone());
        let old_unpark_ptr = self.unpark.swap(Box::into_raw(new_unpark), Ordering::AcqRel);
        if !old_unpark_ptr.is_null() {
            // Drop the old handle
            let _ = unsafe { Box::from_raw(old_unpark_ptr) };
        }
    }

    /// Clear the waker registration.
    fn clear_waker(&self) {
        let waker_ptr = self.waker.swap(null_mut(), Ordering::AcqRel);
        if !waker_ptr.is_null() {
            let _ = unsafe { Box::from_raw(waker_ptr) };
        }
    }

    /// Clear the unpark registration.
    fn clear_unpark(&self) {
        let unpark_ptr = self.unpark.swap(null_mut(), Ordering::AcqRel);
        if !unpark_ptr.is_null() {
            let _ = unsafe { Box::from_raw(unpark_ptr) };
        }
    }
}

impl GlobalRegistry {
    const fn new() -> Self {
        // Initialize all signal states
        const INIT_STATE: SignalState = SignalState::new();
        Self {
            signals: [INIT_STATE; MAX_SIGNALS],
            generation: AtomicU64::new(0),
            init: Once::new(),
        }
    }

    /// Get the signal state for a given signal number.
    fn get(&self, signum: i32) -> Option<&SignalState> {
        if signum >= 0 && (signum as usize) < MAX_SIGNALS {
            Some(&self.signals[signum as usize])
        } else {
            None
        }
    }

    /// Mark a signal as received.
    ///
    /// # Safety
    ///
    /// This function is called from signal handlers and must be async-signal-safe.
    pub(crate) fn notify(&self, signum: i32) {
        if let Some(state) = self.get(signum) {
            state.notify();
            self.generation.fetch_add(1, Ordering::Release);
        }
    }
}

/// Get the global signal registry.
pub(crate) fn registry() -> &'static GlobalRegistry {
    &REGISTRY
}

/// Handle for a registered signal listener.
pub(crate) struct SignalHandle {
    signum: i32,
    last_seen: u64,
}

impl SignalHandle {
    /// Create a new signal handle for the given signal number.
    pub(crate) fn new(signum: i32) -> Self {
        Self {
            signum,
            last_seen: 0,
        }
    }

    /// Get the signal number.
    pub(crate) fn signum(&self) -> i32 {
        self.signum
    }

    /// Try to receive a pending signal.
    pub(crate) fn try_recv(&mut self) -> bool {
        if let Some(state) = registry().get(self.signum) {
            state.try_recv(&mut self.last_seen)
        } else {
            false
        }
    }

    /// Register a waker for this signal.
    pub(crate) fn register_waker(&self, waker: &Waker) {
        if let Some(state) = registry().get(self.signum) {
            state.register_waker(waker);
        }
    }

    /// Register an unpark handle for this signal.
    pub(crate) fn register_unpark(&self, unpark: &UnparkHandle) {
        if let Some(state) = registry().get(self.signum) {
            state.register_unpark(unpark);
        }
    }
}

impl Drop for SignalHandle {
    fn drop(&mut self) {
        if let Some(state) = registry().get(self.signum) {
            state.clear_waker();
            state.clear_unpark();
        }
    }
}

