//! Watch channel for broadcasting the latest value to multiple receivers.

use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock};
use std::task::Waker;

/// Error returned when trying to receive from a closed watch channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecvError;

/// Error returned when trying to send to a closed watch channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendError<T>(pub T);

/// Creates a new watch channel with an initial value.
///
/// # Examples
///
/// ```
/// use maniac::sync::watch;
///
/// #[maniac::main]
/// async fn main() {
///     let (tx, mut rx) = watch::channel(0);
///     
///     tx.send(42).unwrap();
///     assert_eq!(*rx.borrow(), 42);
/// }
/// ```
pub fn channel<T: Clone>(init: T) -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner {
        value: RwLock::new(init),
        wakers: Mutex::new(Vec::new()),
        version: Mutex::new(0u64),
    });

    (
        Sender {
            inner: Arc::clone(&inner),
        },
        Receiver {
            inner,
            last_version: 0,
        },
    )
}

struct Inner<T> {
    value: RwLock<T>,
    wakers: Mutex<Vec<Waker>>,
    version: Mutex<u64>,
}

/// The sending half of a watch channel.
pub struct Sender<T> {
    inner: Arc<Inner<T>>,
}

impl<T: Clone> Sender<T> {
    /// Sends a new value through the channel.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        *self.inner.value.write().unwrap() = value;
        *self.inner.version.lock().unwrap() += 1;
        
        // Wake all receivers
        let wakers = self.inner.wakers.lock().unwrap();
        for waker in wakers.iter() {
            waker.wake_by_ref();
        }
        Ok(())
    }

    /// Sends a new value if the provided closure returns true.
    pub fn send_if_modified<F>(&self, modify: F) -> bool
    where
        F: FnOnce(&mut T) -> bool,
    {
        let mut guard = self.inner.value.write().unwrap();
        if modify(&mut guard) {
            drop(guard);
            *self.inner.version.lock().unwrap() += 1;
            
            // Wake all receivers
            let wakers = self.inner.wakers.lock().unwrap();
            for waker in wakers.iter() {
                waker.wake_by_ref();
            }
            true
        } else {
            false
        }
    }

    /// Returns a reference to the current value.
    pub fn borrow(&self) -> Ref<'_, T> {
        Ref {
            inner: self.inner.value.read().unwrap(),
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// The receiving half of a watch channel.
pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
    last_version: u64,
}

impl<T: Clone> Receiver<T> {
    /// Waits for the value to change.
    pub async fn changed(&mut self) -> Result<(), RecvError> {
        use std::future::Future;
        use std::pin::Pin;
        use std::task::{Context, Poll};

        struct Changed<'a, T> {
            receiver: &'a mut Receiver<T>,
        }

        impl<T: Clone> Future for Changed<'_, T> {
            type Output = Result<(), RecvError>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let current_version = *self.receiver.inner.version.lock().unwrap();
                if current_version != self.receiver.last_version {
                    self.receiver.last_version = current_version;
                    return Poll::Ready(Ok(()));
                }

                // Register waker
                let mut wakers = self.receiver.inner.wakers.lock().unwrap();
                wakers.push(cx.waker().clone());
                Poll::Pending
            }
        }

        Changed { receiver: self }.await
    }

    /// Returns a reference to the current value.
    pub fn borrow(&self) -> Ref<'_, T> {
        Ref {
            inner: self.inner.value.read().unwrap(),
        }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            last_version: self.last_version,
        }
    }
}

/// A reference to the current value in a watch channel.
pub struct Ref<'a, T> {
    inner: std::sync::RwLockReadGuard<'a, T>,
}

impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

