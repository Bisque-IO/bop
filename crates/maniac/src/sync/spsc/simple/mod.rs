//! A fast asynchronous, single-producer, single-consumer (SPSC) bounded channel.
//!
//! This is a simplified variant of the MPSC channel optimized for single-producer
//! scenarios. It avoids the overhead of CAS operations needed for multiple producers.
//!
//! # Disconnection
//!
//! The channel is disconnected automatically once the [`Sender`] or [`Receiver`] is
//! dropped. It can also be disconnected manually by calling the `close` method.
//!
//! # Example
//!
//! ```ignore
//! use maniac::sync::spsc::simple::channel;
//!
//! let (s, mut r) = channel(16);
//!
//! block_on(async move {
//!     s.send(42).await.unwrap();
//!     assert_eq!(r.recv().await.unwrap(), 42);
//! });
//! ```

mod queue;

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use crate::future::waker::DiatomicWaker;
use crate::parking::{Parker, Unparker};
use crate::utils::CachePadded;
use crate::{PopError, PushError};

use futures::stream::Stream;
use std::task::Wake;

use queue::Queue;

/// A waker implementation that unparks a thread.
struct ThreadUnparker {
    unparker: Unparker,
}

impl Wake for ThreadUnparker {
    fn wake(self: Arc<Self>) {
        self.unparker.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.unparker.unpark();
    }
}

/// Shared wake infrastructure for the async adapters.
struct AsyncShared {
    item_waiter: CachePadded<DiatomicWaker>,
    space_waiter: CachePadded<DiatomicWaker>,
}

impl AsyncShared {
    fn new() -> Self {
        Self {
            item_waiter: CachePadded::new(DiatomicWaker::new()),
            space_waiter: CachePadded::new(DiatomicWaker::new()),
        }
    }

    #[inline]
    fn notify_items(&self) {
        self.item_waiter.notify();
    }

    #[inline]
    fn notify_space(&self) {
        self.space_waiter.notify();
    }

    #[inline]
    unsafe fn register_items(&self, waker: &Waker) {
        unsafe { self.item_waiter.register(waker) };
    }

    #[inline]
    unsafe fn register_space(&self, waker: &Waker) {
        unsafe { self.space_waiter.register(waker) };
    }
}

/// Shared channel data.
struct Inner<T> {
    /// Non-blocking internal queue.
    queue: Queue<T>,
    /// Signalling infrastructure.
    shared: AsyncShared,
}

impl<T> Inner<T> {
    fn new(capacity: usize) -> Self {
        Self {
            queue: Queue::new(capacity),
            shared: AsyncShared::new(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Async Variants
// ══════════════════════════════════════════════════════════════════════════════

/// Asynchronous sending side of an SPSC channel.
pub struct AsyncSender<T> {
    inner: Arc<Inner<T>>,
}

impl<T> AsyncSender<T> {
    /// Returns the capacity of the queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.queue.capacity()
    }

    /// Returns `true` if the channel is closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.inner.queue.is_closed()
    }

    /// Attempts to send a message immediately.
    #[inline]
    pub fn try_send(&self, message: T) -> Result<(), PushError<T>> {
        // SAFETY: Only one sender exists (SPSC).
        match unsafe { self.inner.queue.push(message) } {
            Ok(()) => {
                self.inner.shared.notify_items();
                Ok(())
            }
            Err(queue::PushError::Full(v)) => Err(PushError::Full(v)),
            Err(queue::PushError::Closed(v)) => Err(PushError::Closed(v)),
        }
    }

    /// Sends a message asynchronously, waiting until capacity is available.
    pub async fn send(&self, message: T) -> Result<(), PushError<T>> {
        let mut message = Some(message);

        std::future::poll_fn(|cx| {
            // Try to send
            match unsafe { self.inner.queue.push(message.take().unwrap()) } {
                Ok(()) => {
                    self.inner.shared.notify_items();
                    return Poll::Ready(Ok(()));
                }
                Err(queue::PushError::Full(m)) => {
                    message = Some(m);
                }
                Err(queue::PushError::Closed(m)) => {
                    return Poll::Ready(Err(PushError::Closed(m)));
                }
            }

            // Register for notification and try again
            unsafe {
                self.inner.shared.register_space(cx.waker());
            }

            match unsafe { self.inner.queue.push(message.take().unwrap()) } {
                Ok(()) => {
                    self.inner.shared.notify_items();
                    Poll::Ready(Ok(()))
                }
                Err(queue::PushError::Full(m)) => {
                    message = Some(m);
                    Poll::Pending
                }
                Err(queue::PushError::Closed(m)) => Poll::Ready(Err(PushError::Closed(m))),
            }
        })
        .await
    }

    /// Closes the channel.
    pub fn close(&self) {
        self.inner.queue.close();
        self.inner.shared.notify_items();
        self.inner.shared.notify_space();
    }
}

impl<T> Drop for AsyncSender<T> {
    fn drop(&mut self) {
        self.inner.queue.close();
        self.inner.shared.notify_items();
    }
}

impl<T> fmt::Debug for AsyncSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncSender").finish_non_exhaustive()
    }
}

/// Asynchronous receiving side of an SPSC channel.
pub struct AsyncReceiver<T> {
    inner: Arc<Inner<T>>,
}

impl<T> AsyncReceiver<T> {
    /// Returns the capacity of the queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.queue.capacity()
    }

    /// Attempts to receive a message immediately.
    #[inline]
    pub fn try_recv(&self) -> Result<T, PopError> {
        // SAFETY: Only one receiver exists (SPSC).
        match unsafe { self.inner.queue.pop() } {
            Ok(message) => {
                self.inner.shared.notify_space();
                Ok(message)
            }
            Err(queue::PopError::Empty) => Err(PopError::Empty),
            Err(queue::PopError::Closed) => Err(PopError::Closed),
        }
    }

    /// Receives a message asynchronously, waiting until one is available.
    pub async fn recv(&mut self) -> Result<T, PopError> {
        RecvFuture { receiver: self }.await
    }

    /// Closes the channel.
    pub fn close(&self) {
        self.inner.queue.close();
        self.inner.shared.notify_space();
    }
}

impl<T> Drop for AsyncReceiver<T> {
    fn drop(&mut self) {
        self.inner.queue.close();
        self.inner.shared.notify_space();
    }
}

impl<T> fmt::Debug for AsyncReceiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncReceiver").finish_non_exhaustive()
    }
}

impl<T> Stream for AsyncReceiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY: Only one receiver exists (SPSC).
        unsafe {
            // Happy path: try to pop without registering the waker.
            match self.inner.queue.pop() {
                Ok(message) => {
                    self.inner.shared.notify_space();
                    return Poll::Ready(Some(message));
                }
                Err(queue::PopError::Closed) => {
                    return Poll::Ready(None);
                }
                Err(queue::PopError::Empty) => {}
            }

            // Slow path: register the waker and try again.
            self.inner.shared.register_items(cx.waker());

            match self.inner.queue.pop() {
                Ok(message) => {
                    self.inner.shared.notify_space();
                    Poll::Ready(Some(message))
                }
                Err(queue::PopError::Closed) => Poll::Ready(None),
                Err(queue::PopError::Empty) => Poll::Pending,
            }
        }
    }
}

/// The future returned by the `AsyncReceiver::recv` method.
struct RecvFuture<'a, T> {
    receiver: &'a mut AsyncReceiver<T>,
}

impl<'a, T> Future for RecvFuture<'a, T> {
    type Output = Result<T, PopError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.receiver).poll_next(cx) {
            Poll::Ready(Some(v)) => Poll::Ready(Ok(v)),
            Poll::Ready(None) => Poll::Ready(Err(PopError::Closed)),
            Poll::Pending => Poll::Pending,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Blocking Variants
// ══════════════════════════════════════════════════════════════════════════════

/// Blocking sending side of an SPSC channel.
pub struct BlockingSender<T> {
    inner: Arc<Inner<T>>,
    parker: Parker,
    parker_waker: Arc<ThreadUnparker>,
}

impl<T> BlockingSender<T> {
    fn new(inner: Arc<Inner<T>>) -> Self {
        let parker = Parker::new();
        let parker_waker = Arc::new(ThreadUnparker {
            unparker: parker.unparker(),
        });
        Self {
            inner,
            parker,
            parker_waker,
        }
    }

    /// Returns the capacity of the queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.queue.capacity()
    }

    /// Returns `true` if the channel is closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.inner.queue.is_closed()
    }

    /// Attempts to send a message immediately.
    #[inline]
    pub fn try_send(&self, message: T) -> Result<(), PushError<T>> {
        // SAFETY: Only one sender exists (SPSC).
        match unsafe { self.inner.queue.push(message) } {
            Ok(()) => {
                self.inner.shared.notify_items();
                Ok(())
            }
            Err(queue::PushError::Full(v)) => Err(PushError::Full(v)),
            Err(queue::PushError::Closed(v)) => Err(PushError::Closed(v)),
        }
    }

    /// Sends a message, blocking until capacity is available.
    pub fn send(&self, mut message: T) -> Result<(), PushError<T>> {
        match self.try_send(message) {
            Ok(()) => return Ok(()),
            Err(PushError::Closed(m)) => return Err(PushError::Closed(m)),
            Err(PushError::Full(m)) => message = m,
        }

        let waker = Waker::from(Arc::clone(&self.parker_waker));

        loop {
            unsafe {
                self.inner.shared.register_space(&waker);
            }

            match unsafe { self.inner.queue.push(message) } {
                Ok(()) => {
                    self.inner.shared.notify_items();
                    return Ok(());
                }
                Err(queue::PushError::Full(m)) => {
                    message = m;
                    self.parker.park();
                }
                Err(queue::PushError::Closed(m)) => {
                    return Err(PushError::Closed(m));
                }
            }
        }
    }

    /// Closes the channel.
    pub fn close(&self) {
        self.inner.queue.close();
        self.inner.shared.notify_items();
        self.inner.shared.notify_space();
    }
}

impl<T> Drop for BlockingSender<T> {
    fn drop(&mut self) {
        self.inner.queue.close();
        self.inner.shared.notify_items();
    }
}

impl<T> fmt::Debug for BlockingSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockingSender").finish_non_exhaustive()
    }
}

/// Blocking receiving side of an SPSC channel.
pub struct BlockingReceiver<T> {
    inner: Arc<Inner<T>>,
    parker: Parker,
    parker_waker: Arc<ThreadUnparker>,
}

impl<T> BlockingReceiver<T> {
    fn new(inner: Arc<Inner<T>>) -> Self {
        let parker = Parker::new();
        let parker_waker = Arc::new(ThreadUnparker {
            unparker: parker.unparker(),
        });
        Self {
            inner,
            parker,
            parker_waker,
        }
    }

    /// Returns the capacity of the queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.queue.capacity()
    }

    /// Attempts to receive a message immediately.
    #[inline]
    pub fn try_recv(&self) -> Result<T, PopError> {
        // SAFETY: Only one receiver exists (SPSC).
        match unsafe { self.inner.queue.pop() } {
            Ok(message) => {
                self.inner.shared.notify_space();
                Ok(message)
            }
            Err(queue::PopError::Empty) => Err(PopError::Empty),
            Err(queue::PopError::Closed) => Err(PopError::Closed),
        }
    }

    /// Receives a message, blocking until one is available.
    pub fn recv(&self) -> Result<T, PopError> {
        match self.try_recv() {
            Ok(v) => return Ok(v),
            Err(PopError::Closed) => return Err(PopError::Closed),
            Err(PopError::Empty) | Err(PopError::Timeout) => {}
        }

        let waker = Waker::from(Arc::clone(&self.parker_waker));

        loop {
            unsafe {
                self.inner.shared.register_items(&waker);
            }

            match unsafe { self.inner.queue.pop() } {
                Ok(message) => {
                    self.inner.shared.notify_space();
                    return Ok(message);
                }
                Err(queue::PopError::Closed) => {
                    return Err(PopError::Closed);
                }
                Err(queue::PopError::Empty) => {
                    self.parker.park();
                }
            }
        }
    }

    /// Closes the channel.
    pub fn close(&self) {
        self.inner.queue.close();
        self.inner.shared.notify_space();
    }
}

impl<T> Drop for BlockingReceiver<T> {
    fn drop(&mut self) {
        self.inner.queue.close();
        self.inner.shared.notify_space();
    }
}

impl<T> fmt::Debug for BlockingReceiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockingReceiver").finish_non_exhaustive()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Factory Functions
// ══════════════════════════════════════════════════════════════════════════════

/// Creates a new async SPSC channel.
///
/// # Panic
///
/// The function will panic if the requested capacity is 0 or if it is greater
/// than `usize::MAX/4`.
pub fn channel<T>(capacity: usize) -> (AsyncSender<T>, AsyncReceiver<T>) {
    let inner = Arc::new(Inner::new(capacity));

    let sender = AsyncSender {
        inner: inner.clone(),
    };
    let receiver = AsyncReceiver { inner };

    (sender, receiver)
}

/// Creates a new blocking SPSC channel.
///
/// # Panic
///
/// The function will panic if the requested capacity is 0 or if it is greater
/// than `usize::MAX/4`.
pub fn blocking_channel<T>(capacity: usize) -> (BlockingSender<T>, BlockingReceiver<T>) {
    let inner = Arc::new(Inner::new(capacity));

    let sender = BlockingSender::new(inner.clone());
    let receiver = BlockingReceiver::new(inner);

    (sender, receiver)
}

/// Creates a mixed SPSC channel with blocking sender and async receiver.
pub fn blocking_async_channel<T>(capacity: usize) -> (BlockingSender<T>, AsyncReceiver<T>) {
    let inner = Arc::new(Inner::new(capacity));

    let sender = BlockingSender::new(inner.clone());
    let receiver = AsyncReceiver { inner };

    (sender, receiver)
}

/// Creates a mixed SPSC channel with async sender and blocking receiver.
pub fn async_blocking_channel<T>(capacity: usize) -> (AsyncSender<T>, BlockingReceiver<T>) {
    let inner = Arc::new(Inner::new(capacity));

    let sender = AsyncSender {
        inner: inner.clone(),
    };
    let receiver = BlockingReceiver::new(inner);

    (sender, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_try_send_recv() {
        let (sender, receiver) = channel::<u64>(16);

        sender.try_send(42).unwrap();
        sender.try_send(43).unwrap();

        assert_eq!(receiver.try_recv().unwrap(), 42);
        assert_eq!(receiver.try_recv().unwrap(), 43);
        assert!(matches!(receiver.try_recv(), Err(PopError::Empty)));
    }

    #[test]
    fn test_blocking_send_recv() {
        let (sender, receiver) = blocking_channel::<u64>(16);

        sender.try_send(42).unwrap();
        sender.try_send(43).unwrap();

        assert_eq!(receiver.try_recv().unwrap(), 42);
        assert_eq!(receiver.try_recv().unwrap(), 43);
    }

    #[test]
    fn test_mixed_blocking_async() {
        let (sender, receiver) = blocking_async_channel::<u64>(16);

        sender.try_send(42).unwrap();
        assert_eq!(receiver.try_recv().unwrap(), 42);
    }

    #[test]
    fn test_mixed_async_blocking() {
        let (sender, receiver) = async_blocking_channel::<u64>(16);

        sender.try_send(42).unwrap();
        assert_eq!(receiver.try_recv().unwrap(), 42);
    }

    #[test]
    fn test_capacity() {
        let (sender, receiver) = channel::<u64>(16);

        assert_eq!(sender.capacity(), 16);
        assert_eq!(receiver.capacity(), 16);
    }

    #[test]
    fn test_close_semantics() {
        let (sender, receiver) = channel::<u64>(16);

        sender.try_send(1).unwrap();
        sender.try_send(2).unwrap();

        drop(sender);

        assert_eq!(receiver.try_recv().unwrap(), 1);
        assert_eq!(receiver.try_recv().unwrap(), 2);
        assert!(matches!(receiver.try_recv(), Err(PopError::Closed)));
    }

    #[test]
    fn test_fill_and_drain() {
        let (sender, receiver) = blocking_channel::<u64>(8);
        let capacity = sender.capacity();

        // Fill the queue
        for i in 0..capacity as u64 {
            sender.try_send(i).unwrap();
        }

        // Should be full now
        assert!(matches!(sender.try_send(999), Err(PushError::Full(_))));

        // Drain and verify order
        for i in 0..capacity as u64 {
            assert_eq!(receiver.try_recv().unwrap(), i);
        }

        // Should be empty now
        assert!(matches!(receiver.try_recv(), Err(PopError::Empty)));
    }

    #[test]
    fn test_blocking_stress() {
        use std::thread;

        const COUNT: usize = 10_000;
        let (sender, receiver) = blocking_channel::<usize>(64);

        let producer = thread::spawn(move || {
            for i in 0..COUNT {
                sender.send(i).unwrap();
            }
        });

        let consumer = thread::spawn(move || {
            for expected in 0..COUNT {
                let v = receiver.recv().unwrap();
                assert_eq!(v, expected);
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();
    }
}
