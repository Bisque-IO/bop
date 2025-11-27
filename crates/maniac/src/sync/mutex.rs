//! Async mutex implementation.

use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use crate::future::event::Event;

/// An asynchronous mutex.
///
/// This mutex will block tasks waiting for the lock to become available. The mutex
/// can be held across await points.
///
/// # Examples
///
/// ```
/// use maniac::sync::Mutex;
///
/// #[maniac::main]
/// async fn main() {
///     let mutex = Mutex::new(5);
///
///     {
///         let mut guard = mutex.lock().await;
///         *guard = 10;
///     } // guard is dropped here
///
///     let guard = mutex.lock().await;
///     assert_eq!(*guard, 10);
/// }
/// ```
pub struct Mutex<T> {
    /// Event used to notify waiters when the lock becomes available.
    event: Event,
    /// Whether the lock is currently held.
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

impl<T> Mutex<T> {
    /// Creates a new mutex.
    ///
    /// # Examples
    ///
    /// ```
    /// use maniac::sync::Mutex;
    ///
    /// let mutex = Mutex::new(5);
    /// ```
    pub fn new(value: T) -> Self {
        Self {
            event: Event::new(),
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Attempts to acquire the lock without blocking.
    ///
    /// If the lock is currently held, this will return `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use maniac::sync::Mutex;
    ///
    /// #[maniac::main]
    /// async fn main() {
    ///     let mutex = Mutex::new(5);
    ///
    ///     if let Some(guard) = mutex.try_lock() {
    ///         *guard = 10;
    ///     }
    /// }
    /// ```
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if !self.locked.swap(true, Ordering::Release) {
            Some(MutexGuard {
                mutex: unsafe { self as *const _ as *mut Self },
                _phanton: PhantomData,
            })
        } else {
            None
        }
    }

    /// Acquires the lock asynchronously.
    ///
    /// This will wait until the lock becomes available.
    ///
    /// # Examples
    ///
    /// ```
    /// use maniac::sync::Mutex;
    ///
    /// #[maniac::main]
    /// async fn main() {
    ///     let mutex = Mutex::new(5);
    ///
    ///     let mut guard = mutex.lock().await;
    ///     *guard = 10;
    /// }
    /// ```
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            // Try to acquire the lock
            if let Some(guard) = self.try_lock() {
                return guard;
            }

            // Wait until the lock becomes available
            // We check if locked is false, meaning the lock is free
            let lock_acquired = self
                .event
                .wait_until(|| {
                    // Check if lock is free
                    if !self.locked.load(Ordering::Acquire) {
                        Some(())
                    } else {
                        None
                    }
                })
                .await;

            // After being notified, try to acquire the lock again
            // Note: There's a race condition here, but that's okay - we'll loop
            // and try again if we don't get it
        }
    }
}

unsafe impl<T: Send> Send for Mutex<T> {}
unsafe impl<T: Send> Sync for Mutex<T> {}

/// A guard that holds the mutex lock.
///
/// When the guard is dropped, the lock is released.
pub struct MutexGuard<'a, T> {
    mutex: *mut Mutex<T>,
    _phanton: PhantomData<&'a ()>,
}

impl<'a, T> std::ops::Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: guard is always non null
        unsafe { &*(&*self.mutex).value.get() }
    }
}

impl<'a, T> std::ops::DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: guard is always non null
        unsafe { (&mut *self.mutex).value.get_mut() }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        // Release the lock
        if !unsafe { &*self.mutex }
            .locked
            .swap(false, Ordering::Release)
        {
            panic!("mutex poisoned");
        }
        // Notify waiters
        unsafe { &*self.mutex }.event.notify_one();
    }
}
