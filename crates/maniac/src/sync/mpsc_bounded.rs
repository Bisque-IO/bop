//! Bounded MPSC channel implementation for openraft.

use std::future::Future;
use std::sync::{Arc, Mutex};

use crate::sync::mpsc::{async_mpsc, AsyncMpscReceiver, AsyncMpscSender};
use crate::PushError;

/// Creates a new bounded MPSC channel.
///
/// Note: The underlying implementation doesn't support a capacity parameter,
/// but provides efficient bounded behavior through its internal structure.
pub fn channel<T>(_capacity: usize) -> (MpscSender<T>, MpscReceiver<T>) {
    let (sender, receiver) = async_mpsc::<T, 6, 8>();
    
    (
        MpscSender {
            inner: Arc::new(Mutex::new(sender)),
        },
        MpscReceiver {
            inner: Arc::new(Mutex::new(receiver)),
        },
    )
}

/// The sending half of a bounded MPSC channel.
pub struct MpscSender<T> {
    inner: Arc<Mutex<AsyncMpscSender<T, 6, 8>>>,
}

impl<T> Clone for MpscSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Send + 'static> MpscSender<T> {
    /// Sends a value through the channel.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut guard = self.inner.lock().unwrap();
        guard.send(value).await.map_err(|e| match e {
            PushError::Full(v) => SendError(v),
            PushError::Closed(v) => SendError(v),
        })
    }

    /// Creates a weak reference to this sender.
    pub fn downgrade(&self) -> WeakMpscSender<T> {
        WeakMpscSender {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

/// Error returned when sending fails.
#[derive(Debug)]
pub struct SendError<T>(pub T);

/// A weak reference to an MPSC sender.
pub struct WeakMpscSender<T> {
    inner: std::sync::Weak<Mutex<AsyncMpscSender<T, 6, 8>>>,
}

impl<T> Clone for WeakMpscSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: std::sync::Weak::clone(&self.inner),
        }
    }
}

impl<T> WeakMpscSender<T> {
    /// Attempts to upgrade the weak reference to a strong reference.
    pub fn upgrade(&self) -> Option<MpscSender<T>> {
        self.inner.upgrade().map(|inner| MpscSender { inner })
    }
}

/// The receiving half of a bounded MPSC channel.
pub struct MpscReceiver<T> {
    inner: Arc<Mutex<AsyncMpscReceiver<T, 6, 8>>>,
}

impl<T> MpscReceiver<T> {
    /// Receives a value from the channel.
    pub async fn recv(&mut self) -> Option<T> {
        let mut guard = self.inner.lock().unwrap();
        guard.recv().await.ok()
    }

    /// Attempts to receive a value without blocking.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let mut guard = self.inner.lock().unwrap();
        guard.try_recv().map_err(|e| match e {
            crate::PopError::Empty => TryRecvError::Empty,
            crate::PopError::Closed => TryRecvError::Disconnected,
            crate::PopError::Timeout => TryRecvError::Empty,
        })
    }
}

/// Error returned when trying to receive without blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryRecvError {
    /// The channel is empty.
    Empty,
    /// The channel is disconnected.
    Disconnected,
}
