//! Unbounded MPSC channel implementation for openraft.

use std::sync::{Arc, Mutex};

use crate::sync::mpsc::unbounded::async_unbounded_mpsc;
use crate::sync::mpsc::unbounded::{AsyncUnboundedMpscReceiver, AsyncUnboundedMpscSender};

/// Creates a new unbounded MPSC channel.
pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let (sender, receiver) = async_unbounded_mpsc::<T>();
    
    (
        UnboundedSender {
            inner: Arc::new(Mutex::new(sender)),
        },
        UnboundedReceiver {
            inner: Arc::new(Mutex::new(receiver)),
        },
    )
}

/// The sending half of an unbounded MPSC channel.
pub struct UnboundedSender<T> {
    inner: Arc<Mutex<AsyncUnboundedMpscSender<T>>>,
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Send + 'static> UnboundedSender<T> {
    /// Sends a value through the channel (synchronous, never blocks for unbounded).
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut guard = self.inner.lock().unwrap();
        match guard.try_send(value) {
            Ok(()) => Ok(()),
            Err(e) => match e {
                crate::PushError::Full(v) => Err(SendError(v)),
                crate::PushError::Closed(v) => Err(SendError(v)),
            },
        }
    }

    /// Creates a weak reference to this sender.
    pub fn downgrade(&self) -> WeakUnboundedSender<T> {
        WeakUnboundedSender {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

/// Error returned when sending fails.
#[derive(Debug)]
pub struct SendError<T>(pub T);

/// A weak reference to an unbounded MPSC sender.
pub struct WeakUnboundedSender<T> {
    inner: std::sync::Weak<Mutex<AsyncUnboundedMpscSender<T>>>,
}

impl<T> Clone for WeakUnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: std::sync::Weak::clone(&self.inner),
        }
    }
}

impl<T> WeakUnboundedSender<T> {
    /// Attempts to upgrade the weak reference to a strong reference.
    pub fn upgrade(&self) -> Option<UnboundedSender<T>> {
        self.inner.upgrade().map(|inner| UnboundedSender { inner })
    }
}

/// The receiving half of an unbounded MPSC channel.
pub struct UnboundedReceiver<T> {
    inner: Arc<Mutex<AsyncUnboundedMpscReceiver<T>>>,
}

impl<T> UnboundedReceiver<T> {
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

