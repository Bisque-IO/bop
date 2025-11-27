//! Async semaphore implementation.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::future::event::Event;

/// An asynchronous semaphore.
///
/// Semaphores are a synchronization primitive that can be used to control access
/// to a common resource by multiple processes in a concurrent system.
///
/// # Examples
///
/// ```
/// use maniac::sync::Semaphore;
/// use std::sync::Arc;
///
/// #[maniac::main]
/// async fn main() {
///     let semaphore = Arc::new(Semaphore::new(3));
///     let mut join_handles = Vec::new();
///
///     for _ in 0..5 {
///         let permit = semaphore.clone().acquire_owned().await;
///         join_handles.push(maniac::spawn(async move {
///             // perform task...
///             drop(permit);
///         }));
///     }
///
///     for handle in join_handles {
///         handle.await;
///     }
/// }
/// ```
pub struct Semaphore {
    /// Event used to notify waiters when permits become available.
    event: Event,
    /// The current number of permits available.
    permits: AtomicUsize,
}

impl Semaphore {
    /// Creates a new semaphore with the specified number of permits.
    pub fn new(permits: usize) -> Self {
        Self {
            event: Event::new(),
            permits: AtomicUsize::new(permits),
        }
    }

    /// Acquires a permit from the semaphore.
    ///
    /// If no permits are available, this will wait until one becomes available.
    pub async fn acquire(&self) -> SemaphorePermit<'_> {
        // Fast path: try to acquire without waiting
        if let Some(permit) = self.try_acquire() {
            return permit;
        }

        // Slow path: wait for a permit
        self.event
            .wait_until(|| {
                if self.try_acquire_internal() {
                    Some(())
                } else {
                    None
                }
            })
            .await;

        SemaphorePermit { semaphore: self }
    }

    pub async fn acquire_owned(self: Arc<Self>) -> OwnedSemaphorePermit {
        // Fast path
        if self.try_acquire_internal() {
            return OwnedSemaphorePermit { semaphore: self };
        }

        // Slow path
        self.event
            .wait_until(|| {
                if self.try_acquire_internal() {
                    Some(())
                } else {
                    None
                }
            })
            .await;

        OwnedSemaphorePermit { semaphore: self }
    }

    /// Attempts to acquire a permit without waiting.
    pub fn try_acquire(&self) -> Option<SemaphorePermit<'_>> {
        if self.try_acquire_internal() {
            Some(SemaphorePermit { semaphore: self })
        } else {
            None
        }
    }

    /// Attempts to acquire a permit without waiting, returning an owned permit.
    pub fn try_acquire_owned(self: Arc<Self>) -> Option<OwnedSemaphorePermit> {
        if self.try_acquire_internal() {
            Some(OwnedSemaphorePermit { semaphore: self })
        } else {
            None
        }
    }

    /// Adds `n` permits to the semaphore.
    pub fn add_permits(&self, n: usize) {
        self.permits.fetch_add(n, Ordering::Release);
        // Notify waiters. We notify `n` waiters because `n` permits were added.
        // However, since `Event::notify` is just a hint to check the predicate,
        // and we don't know exactly how many are waiting, notifying `n` is a
        // reasonable heuristic.
        self.event.notify(n);
    }

    /// Returns the current number of available permits.
    pub fn available_permits(&self) -> usize {
        self.permits.load(Ordering::Acquire)
    }

    fn try_acquire_internal(&self) -> bool {
        let mut curr = self.permits.load(Ordering::Acquire);
        loop {
            if curr == 0 {
                return false;
            }
            match self.permits.compare_exchange(
                curr,
                curr - 1,
                Ordering::Acquire,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => curr = actual,
            }
        }
    }
}

/// A permit from the semaphore.
///
/// When the permit is dropped, it is returned to the semaphore.
pub struct SemaphorePermit<'a> {
    semaphore: &'a Semaphore,
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.semaphore.add_permits(1);
    }
}

unsafe impl Send for SemaphorePermit<'_> {}
unsafe impl Sync for SemaphorePermit<'_> {}

/// An owned permit from the semaphore.
///
/// When the permit is dropped, it is returned to the semaphore.
pub struct OwnedSemaphorePermit {
    semaphore: Arc<Semaphore>,
}

impl Drop for OwnedSemaphorePermit {
    fn drop(&mut self) {
        self.semaphore.add_permits(1);
    }
}

unsafe impl Send for Semaphore {}
unsafe impl Sync for Semaphore {}
