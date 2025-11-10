//! Async and Blocking SPSC (Single-Producer, Single-Consumer) queue implementation.
//!
//! This module provides both async and blocking adapters over the lock-free SPSC queue.
//! The async variants implement `futures::Sink` and `futures::Stream` traits for
//! seamless integration with async/await code. The blocking variants use efficient
//! thread parking for synchronous operations.
//!
//! # Queue Variants
//!
//! - **Async**: [`AsyncSpscProducer`] / [`AsyncSpscConsumer`] - For use with async tasks
//! - **Blocking**: [`BlockingSpscProducer`] / [`BlockingSpscConsumer`] - For use with threads
//! - **Mixed**: You can mix async and blocking ends on the same queue!
//!
//! All variants share the same waker infrastructure, allowing seamless interoperability.
//! A blocking producer can wake up an async consumer and vice versa.
//!
//! # Design Principles
//!
//! ## Correctness Guarantees
//!
//! The implementation uses the **double-check pattern** to prevent missed wakeups:
//! 1. Check if operation is possible (space available / items available)
//! 2. Register waker if not
//! 3. Double-check after registering (catches races)
//!
//! This pattern, combined with `DiatomicWaker`'s acquire/release memory ordering,
//! guarantees that no items are lost and no wakeups are missed, even in the presence
//! of concurrent operations between producer and consumer.
//!
//! ## Memory Ordering
//!
//! The queue operations synchronize via acquire/release semantics:
//! - Producer writes data (Release) → Consumer reads data (Acquire)
//! - Consumer updates tail (Release) → Producer checks space (Acquire)
//! - Notifications use `DiatomicWaker` with proper ordering
//!
//! ## Zero-Copy Design
//!
//! Items waiting to be sent are stored in the Future's stack frame, not in the
//! `AsyncSpscProducer` struct. This eliminates the need for `T: Unpin` and keeps
//! the implementation simple and efficient.
//!
//! ## Performance Characteristics
//!
//! - **Fast path**: ~5-15ns for non-blocking operations
//! - **Notification overhead**: ~1-2ns when no waker registered
//! - **Zero allocation**: All state lives on stack or in shared queue
//! - **Cache-friendly**: Wakers are cache-padded to prevent false sharing
//!
//! # Examples
//!
//! ## Pure Async
//!
//! ```ignore
//! use futures::{SinkExt, StreamExt};
//!
//! let (mut producer, mut consumer) = new_async_spsc(signal);
//!
//! // Producer task
//! tokio::spawn(async move {
//!     producer.send(42).await.unwrap();
//!     producer.send(43).await.unwrap();
//! });
//!
//! // Consumer task
//! tokio::spawn(async move {
//!     while let Some(item) = consumer.next().await {
//!         println!("Got: {}", item);
//!     }
//! });
//! ```
//!
//! ## Pure Blocking
//!
//! ```ignore
//! let (producer, consumer) = new_blocking_spsc(signal);
//!
//! // Producer thread
//! std::thread::spawn(move || {
//!     producer.send(42).unwrap();
//!     producer.send(43).unwrap();
//! });
//!
//! // Consumer thread
//! std::thread::spawn(move || {
//!     while let Ok(item) = consumer.recv() {
//!         println!("Got: {}", item);
//!     }
//! });
//! ```
//!
//! ## Mixed: Blocking Producer + Async Consumer
//!
//! ```ignore
//! let (producer, mut consumer) = new_blocking_async_spsc(signal);
//!
//! // Producer thread (blocking)
//! std::thread::spawn(move || {
//!     producer.send(42).unwrap();  // Parks thread if full
//! });
//!
//! // Consumer task (async)
//! tokio::spawn(async move {
//!     let item = consumer.recv().await.unwrap();  // Wakes up blocking thread
//! });
//! ```
//!
//! ## Mixed: Async Producer + Blocking Consumer
//!
//! ```ignore
//! let (mut producer, consumer) = new_async_blocking_spsc(signal);
//!
//! // Producer task (async)
//! tokio::spawn(async move {
//!     producer.send(42).await.unwrap();  // Wakes up blocking thread
//! });
//!
//! // Consumer thread (blocking)
//! std::thread::spawn(move || {
//!     let item = consumer.recv().unwrap();  // Parks thread if empty
//! });
//! ```

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::marker::PhantomData;
use std::sync::Arc;

use crate::utils::CachePadded;

use super::signal::{AsyncSignalGate, AsyncSignalWaker};

use crate::{PopError, PushError};

use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::{sink::Sink, stream::Stream};

use crate::parking::{Parker, Unparker};
use crate::waker::DiatomicWaker;
use std::task::Wake;

/// A waker implementation that unparks a thread.
///
/// Used to integrate blocking operations with the async waker infrastructure,
/// allowing async and blocking operations to work together seamlessly.
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
///
/// Maintains separate wakers for the producer (waiting for space) and consumer
/// (waiting for items). The wakers are cache-padded to prevent false sharing
/// between producer and consumer threads.
///
/// # Memory Ordering
///
/// The `DiatomicWaker` provides acquire/release semantics that synchronize with
/// the underlying queue's memory ordering:
/// - Producer: write data (Release) → notify_items → Consumer: read data (Acquire)
/// - Consumer: read data → notify_space → Producer: check space (Acquire)
///
/// # Correctness
///
/// The double-check pattern (check → register → check) prevents missed wakeups:
/// - If state changes before register, the second check catches it
/// - If state changes after register, the waker gets notified
/// - If state changes during register, `DiatomicWaker`'s state machine guarantees
///   either the second check sees the change or the notifier wakes the waker
struct AsyncSpscShared {
    item_waiter: CachePadded<DiatomicWaker>,
    space_waiter: CachePadded<DiatomicWaker>,
}

impl AsyncSpscShared {
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
    unsafe fn wait_for_items<P, T>(&self, predicate: P) -> crate::waker::WaitUntil<'_, P, T>
    where
        P: FnMut() -> Option<T>,
    {
        unsafe { self.item_waiter.wait_until(predicate) }
    }

    #[inline]
    unsafe fn wait_for_space<P, T>(&self, predicate: P) -> crate::waker::WaitUntil<'_, P, T>
    where
        P: FnMut() -> Option<T>,
    {
        unsafe { self.space_waiter.wait_until(predicate) }
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

/// Asynchronous producer façade for [`SegSpsc`].
pub struct AsyncSpscProducer<T, const P: usize, const NUM_SEGS_P2: usize> {
    sender: crate::spsc::Sender<T, P, NUM_SEGS_P2, AsyncSignalGate>,
    shared: Arc<AsyncSpscShared>,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> AsyncSpscProducer<T, P, NUM_SEGS_P2> {
    fn new(
        sender: crate::spsc::Sender<T, P, NUM_SEGS_P2, AsyncSignalGate>,
        shared: Arc<AsyncSpscShared>,
    ) -> Self {
        Self { sender, shared }
    }

    /// Capacity of the underlying queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        crate::spsc::Spsc::<T, P, NUM_SEGS_P2, AsyncSignalGate>::capacity()
    }

    /// Fast-path send without suspension.
    ///
    /// Attempts to send an item immediately without blocking. Always notifies
    /// waiting consumers on success to prevent missed wakeups.
    ///
    /// # Performance
    ///
    /// - Success path: ~5-15ns (queue write + notify check)
    /// - Notify overhead: ~1-2ns when no consumer waiting
    #[inline]
    pub fn try_send(&self, value: T) -> Result<(), PushError<T>> {
        match self.sender.try_push(value) {
            Ok(()) => {
                // Always notify consumers after successful write.
                // This is cheap (~1-2ns) when no waker is registered.
                self.shared.notify_items();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Asynchronously sends a single item.
    ///
    /// Tries to send immediately; if the queue is full, suspends until space
    /// becomes available. The item is held in the Future's stack frame while
    /// waiting, avoiding the need for a `pending` field in the struct.
    ///
    /// # Correctness
    ///
    /// The item is never dropped or lost:
    /// - On success: item is in the queue
    /// - On `Full`: item is stored in `pending` (Future stack frame)
    /// - On `Closed`: item is returned in the error
    ///
    /// The predicate is called on each wakeup to retry sending. The `wait_for_space`
    /// future uses the double-check pattern internally to prevent missed wakeups.
    ///
    /// # Safety
    ///
    /// The `wait_for_space` call is safe because:
    /// - `AsyncSpscProducer` is `!Clone` and `!Sync` (single-threaded access)
    /// - SPSC guarantees only one producer thread
    /// - Therefore, no concurrent calls to `register` or `wait_until` on `space_waiter`
    pub async fn send(&mut self, value: T) -> Result<(), PushError<T>> {
        match self.try_send(value) {
            Ok(()) => Ok(()),
            Err(PushError::Full(item)) => {
                // Store item in Future's stack frame (not in struct).
                // This avoids needing T: Unpin and keeps the struct simple.
                let mut pending = Some(item);
                let sender = &self.sender;
                let shared = &self.shared;
                unsafe {
                    shared
                        .wait_for_space(|| {
                            // Try to send on each wakeup.
                            let candidate = pending.take()?;
                            match sender.try_push(candidate) {
                                Ok(()) => {
                                    // Success! Notify waiting consumers.
                                    shared.notify_items();
                                    Some(Ok(()))
                                }
                                Err(PushError::Full(candidate)) => {
                                    // Still full, restore item and keep waiting.
                                    pending = Some(candidate);
                                    None
                                }
                                Err(PushError::Closed(candidate)) => {
                                    // Channel closed, return error with item.
                                    Some(Err(PushError::Closed(candidate)))
                                }
                            }
                        })
                        .await
                }
            }
            Err(PushError::Closed(item)) => Err(PushError::Closed(item)),
        }
    }

    /// Sends an entire slice, awaiting at most once if the queue fills.
    ///
    /// Makes progress whenever space is available, writing as many items as possible
    /// in each attempt. Notifies consumers after each batch write (not just at the end),
    /// allowing the consumer to start processing while the producer is still sending.
    ///
    /// # Efficiency
    ///
    /// - Amortizes notification overhead across batch (single notify per write batch)
    /// - Allows progressive consumption (consumer doesn't wait for entire batch)
    /// - Zero allocation (slice reference lives in Future stack frame)
    pub async fn send_slice(&mut self, values: &[T]) -> Result<(), PushError<()>> {
        if values.is_empty() {
            return Ok(());
        }

        let sender = &self.sender;
        let shared = &self.shared;
        let mut remaining = values;

        match sender.try_push_n(remaining) {
            Ok(written) => {
                if written > 0 {
                    shared.notify_items();
                    remaining = &remaining[written..];
                    if remaining.is_empty() {
                        return Ok(());
                    }
                }
            }
            Err(PushError::Closed(())) => return Err(PushError::Closed(())),
            Err(PushError::Full(())) => {}
        }

        unsafe {
            shared
                .wait_for_space(|| {
                    if remaining.is_empty() {
                        return Some(Ok(()));
                    }

                    match sender.try_push_n(remaining) {
                        Ok(written) => {
                            if written > 0 {
                                shared.notify_items();
                                remaining = &remaining[written..];
                                if remaining.is_empty() {
                                    return Some(Ok(()));
                                }
                            }
                            None
                        }
                        Err(PushError::Full(())) => None,
                        Err(PushError::Closed(())) => Some(Err(PushError::Closed(()))),
                    }
                })
                .await
        }
    }

    /// Sends every item from the iterator, awaiting as required.
    pub async fn send_batch<I>(&mut self, iter: I) -> Result<(), PushError<T>>
    where
        I: IntoIterator<Item = T>,
    {
        for item in iter {
            self.send(item).await?;
        }
        Ok(())
    }

    /// Closes the queue and wakes any waiters.
    pub fn close(&mut self) {
        self.sender.close_channel();
        self.shared.notify_items();
        self.shared.notify_space();
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Sink<T> for AsyncSpscProducer<T, P, NUM_SEGS_P2> {
    type Error = PushError<T>;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        // Safety: AsyncSpscProducer is not self-referential, so get_unchecked_mut is safe.
        // The Pin guarantee gives us exclusive mutable access.
        let this = unsafe { self.get_unchecked_mut() };

        // Fast path: check if there's space available
        if !this.sender.is_full() {
            return Poll::Ready(Ok(()));
        }

        // No space available. Register waker to be notified when space frees up.
        //
        // Safety: This is safe because:
        // - AsyncSpscProducer is !Clone and !Sync (single-threaded access)
        // - SPSC guarantees only one producer thread
        // - Therefore no concurrent calls to register_space or wait_for_space
        unsafe {
            this.shared.register_space(cx.waker());
        }

        // Double-check after registering to prevent missed wakeups.
        //
        // Race scenarios:
        // 1. Consumer frees space BEFORE register: double-check catches it → Ready
        // 2. Consumer frees space AFTER register: waker gets notified → will poll again
        // 3. Consumer frees space DURING register: DiatomicWaker state machine guarantees
        //    either we see the change here, or the consumer sees our waker → no missed wakeup
        if !this.sender.is_full() {
            return Poll::Ready(Ok(()));
        }

        Poll::Pending
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), PushError<T>> {
        // Safety: Same as poll_ready - AsyncSpscProducer is not self-referential.
        let this = unsafe { self.get_unchecked_mut() };

        // For SPSC with single producer, if poll_ready returned Ready, the queue
        // cannot become full before start_send (no other producers to race with).
        // However, the channel might be closed, so we still handle errors properly.
        this.try_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        // SPSC queue has no buffering at the Sink level (items go directly to queue),
        // so flush is always immediately complete.
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        // Safety: Same as poll_ready - AsyncSpscProducer is not self-referential.
        let this = unsafe { self.get_unchecked_mut() };
        this.close();
        Poll::Ready(Ok(()))
    }
}

/// Asynchronous consumer façade for [`SegSpsc`].
pub struct AsyncSpscConsumer<T, const P: usize, const NUM_SEGS_P2: usize> {
    receiver: crate::spsc::Receiver<T, P, NUM_SEGS_P2, AsyncSignalGate>,
    shared: Arc<AsyncSpscShared>,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> AsyncSpscConsumer<T, P, NUM_SEGS_P2> {
    fn new(
        receiver: crate::spsc::Receiver<T, P, NUM_SEGS_P2, AsyncSignalGate>,
        shared: Arc<AsyncSpscShared>,
    ) -> Self {
        Self { receiver, shared }
    }

    /// Capacity of the underlying queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        crate::spsc::Spsc::<T, P, NUM_SEGS_P2, AsyncSignalGate>::capacity()
    }

    /// Attempts to receive without awaiting.
    ///
    /// Always notifies waiting producers on success to prevent missed wakeups.
    ///
    /// # Performance
    ///
    /// - Success path: ~5-15ns (queue read + notify check)
    /// - Notify overhead: ~1-2ns when no producer waiting
    #[inline]
    pub fn try_recv(&self) -> Result<T, PopError> {
        match self.receiver.try_pop() {
            Some(value) => {
                // Always notify producers after successful read.
                // This is cheap (~1-2ns) when no waker is registered.
                self.shared.notify_space();
                Ok(value)
            }
            None if self.receiver.is_closed() => Err(PopError::Closed),
            None => Err(PopError::Empty),
        }
    }

    /// Asynchronously receives a single item.
    ///
    /// Tries to receive immediately; if the queue is empty, suspends until an
    /// item becomes available or the channel is closed.
    ///
    /// # Correctness
    ///
    /// The predicate is called on each wakeup to retry receiving. The `wait_for_items`
    /// future uses the double-check pattern internally to prevent missed wakeups.
    ///
    /// # Safety
    ///
    /// The `wait_for_items` call is safe because:
    /// - `AsyncSpscConsumer` is `!Clone` and `!Sync` (single-threaded access)
    /// - SPSC guarantees only one consumer thread
    /// - Therefore, no concurrent calls to `register` or `wait_until` on `item_waiter`
    pub async fn recv(&mut self) -> Result<T, PopError> {
        // Fast path: try to receive immediately
        match self.try_recv() {
            Ok(value) => return Ok(value),
            Err(PopError::Empty) | Err(PopError::Timeout) => {}
            Err(PopError::Closed) => return Err(PopError::Closed),
        }

        let receiver = &self.receiver;
        let shared = &self.shared;
        unsafe {
            shared
                .wait_for_items(|| match receiver.try_pop() {
                    Some(value) => {
                        // Success! Notify waiting producers.
                        shared.notify_space();
                        Some(Ok(value))
                    }
                    None if receiver.is_closed() => Some(Err(PopError::Closed)),
                    None => None, // Still empty, keep waiting
                })
                .await
        }
    }

    /// Receives up to `dst.len()` items.
    ///
    /// Makes progress whenever items are available, reading as many as possible
    /// in each attempt. Returns when the buffer is full or the channel is closed.
    /// Notifies producers after each batch read to free up space progressively.
    ///
    /// # Returns
    ///
    /// - `Ok(count)`: Number of items read (may be less than `dst.len()`)
    /// - `Err(PopError::Closed)`: Channel closed and no items available
    ///
    /// # Efficiency
    ///
    /// - Amortizes notification overhead across batch (single notify per read batch)
    /// - Allows progressive production (producer can send more while consumer processes)
    pub async fn recv_batch(&mut self, dst: &mut [T]) -> Result<usize, PopError> {
        if dst.is_empty() {
            return Ok(0);
        }

        let receiver = &self.receiver;
        let shared = &self.shared;
        let mut filled = match receiver.try_pop_n(dst) {
            Ok(count) => {
                if count > 0 {
                    shared.notify_space();
                }
                count
            }
            Err(PopError::Empty) | Err(PopError::Timeout) => 0,
            Err(PopError::Closed) => return Err(PopError::Closed),
        };

        if filled == dst.len() {
            return Ok(filled);
        }

        unsafe {
            shared
                .wait_for_items(|| {
                    if filled == dst.len() {
                        return Some(Ok(filled));
                    }

                    match receiver.try_pop_n(&mut dst[filled..]) {
                        Ok(0) => {
                            if receiver.is_closed() {
                                Some(if filled > 0 {
                                    Ok(filled)
                                } else {
                                    Err(PopError::Closed)
                                })
                            } else {
                                None
                            }
                        }
                        Ok(count) => {
                            filled += count;
                            shared.notify_space();
                            if filled == dst.len() {
                                Some(Ok(filled))
                            } else {
                                None
                            }
                        }
                        Err(PopError::Empty) | Err(PopError::Timeout) => {
                            if receiver.is_closed() {
                                Some(if filled > 0 {
                                    Ok(filled)
                                } else {
                                    Err(PopError::Closed)
                                })
                            } else {
                                None
                            }
                        }
                        Err(PopError::Closed) => Some(if filled > 0 {
                            Ok(filled)
                        } else {
                            Err(PopError::Closed)
                        }),
                    }
                })
                .await
        }
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Stream for AsyncSpscConsumer<T, P, NUM_SEGS_P2> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        // Safety: AsyncSpscConsumer is not self-referential, so get_unchecked_mut is safe.
        // The Pin guarantee gives us exclusive mutable access.
        let this = unsafe { self.get_unchecked_mut() };

        // Fast path: check if there's an item available
        match this.try_recv() {
            Ok(value) => Poll::Ready(Some(value)),
            Err(PopError::Closed) => Poll::Ready(None),
            Err(PopError::Empty) | Err(PopError::Timeout) => {
                // No items available. Register waker to be notified when items arrive.
                //
                // Safety: This is safe because:
                // - AsyncSpscConsumer is !Clone and !Sync (single-threaded access)
                // - SPSC guarantees only one consumer thread
                // - Therefore no concurrent calls to register_items or wait_for_items
                unsafe {
                    this.shared.register_items(cx.waker());
                }

                // Double-check after registering to prevent missed wakeups.
                //
                // Race scenarios:
                // 1. Producer sends item BEFORE register: double-check catches it → Ready
                // 2. Producer sends item AFTER register: waker gets notified → will poll again
                // 3. Producer sends item DURING register: DiatomicWaker state machine guarantees
                //    either we see the item here, or the producer sees our waker → no missed wakeup
                match this.try_recv() {
                    Ok(value) => Poll::Ready(Some(value)),
                    Err(PopError::Closed) => Poll::Ready(None),
                    Err(PopError::Empty) | Err(PopError::Timeout) => Poll::Pending,
                }
            }
        }
    }
}

/// Blocking producer for SPSC queue.
///
/// This type provides blocking send operations that park the thread until space
/// is available. It shares the same waker infrastructure as `AsyncSpscProducer`,
/// allowing blocking and async operations to interoperate seamlessly.
///
/// # Interoperability
///
/// A `BlockingSpscProducer` can wake up an `AsyncSpscConsumer` and vice versa.
/// This allows mixing blocking threads with async tasks in the same queue.
///
/// # Example
///
/// ```ignore
/// // Create mixed queue: blocking producer, async consumer
/// let (blocking_producer, async_consumer) = new_blocking_async_spsc(signal);
///
/// // Producer thread (blocking)
/// std::thread::spawn(move || {
///     blocking_producer.send(42).unwrap();
/// });
///
/// // Consumer task (async)
/// tokio::spawn(async move {
///     let item = async_consumer.recv().await.unwrap();
/// });
/// ```
pub struct BlockingSpscProducer<T, const P: usize, const NUM_SEGS_P2: usize> {
    sender: crate::spsc::Sender<T, P, NUM_SEGS_P2, AsyncSignalGate>,
    shared: Arc<AsyncSpscShared>,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> BlockingSpscProducer<T, P, NUM_SEGS_P2> {
    fn new(
        sender: crate::spsc::Sender<T, P, NUM_SEGS_P2, AsyncSignalGate>,
        shared: Arc<AsyncSpscShared>,
    ) -> Self {
        Self { sender, shared }
    }

    /// Capacity of the underlying queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        crate::spsc::Spsc::<T, P, NUM_SEGS_P2, AsyncSignalGate>::capacity()
    }

    /// Fast-path send without blocking.
    ///
    /// Returns immediately with success or error. Does not block the thread.
    #[inline]
    pub fn try_send(&self, value: T) -> Result<(), PushError<T>> {
        match self.sender.try_push(value) {
            Ok(()) => {
                self.shared.notify_items();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Blocking send that parks the thread until space is available.
    ///
    /// Uses efficient thread parking (no busy-waiting). The thread will be
    /// unparked when the consumer (async or blocking) frees up space.
    ///
    /// # Correctness
    ///
    /// Uses the double-check pattern to prevent missed wakeups:
    /// 1. Try to send
    /// 2. Register waker if full
    /// 3. Double-check after registering (catches races)
    /// 4. Park if still full
    ///
    /// # Performance
    ///
    /// - Fast path (space available): ~5-15ns
    /// - Blocking path: Efficient thread parking (no spinning)
    pub fn send(&self, mut value: T) -> Result<(), PushError<T>> {
        // Fast path: try immediate send
        match self.try_send(value) {
            Ok(()) => return Ok(()),
            Err(PushError::Closed(item)) => return Err(PushError::Closed(item)),
            Err(PushError::Full(item)) => value = item,
        }

        // Slow path: need to block
        let parker = Parker::new();
        let unparker = parker.unparker();
        let waker = Waker::from(Arc::new(ThreadUnparker { unparker }));

        loop {
            // Register our waker
            unsafe {
                self.shared.register_space(&waker);
            }

            // Double-check after registering (prevent missed wakeup)
            match self.sender.try_push(value) {
                Ok(()) => {
                    self.shared.notify_items();
                    return Ok(());
                }
                Err(PushError::Full(item)) => {
                    value = item;
                    // Still full, park until woken
                    parker.park();
                    // Loop again after wakeup
                }
                Err(PushError::Closed(item)) => {
                    return Err(PushError::Closed(item));
                }
            }
        }
    }

    /// Blocking send of a slice.
    ///
    /// Makes progress whenever space is available. More efficient than calling
    /// `send()` in a loop due to bulk operations.
    pub fn send_slice(&self, values: &[T]) -> Result<(), PushError<()>> {
        if values.is_empty() {
            return Ok(());
        }

        let mut remaining = values;

        // Try immediate send of as much as possible
        match self.sender.try_push_n(remaining) {
            Ok(written) => {
                if written > 0 {
                    self.shared.notify_items();
                    remaining = &remaining[written..];
                    if remaining.is_empty() {
                        return Ok(());
                    }
                }
            }
            Err(PushError::Closed(())) => return Err(PushError::Closed(())),
            Err(PushError::Full(())) => {}
        }

        // Slow path: need to block
        let parker = Parker::new();
        let unparker = parker.unparker();
        let waker = Waker::from(Arc::new(ThreadUnparker { unparker }));

        loop {
            // Register our waker
            unsafe {
                self.shared.register_space(&waker);
            }

            // Double-check and try to make progress
            if remaining.is_empty() {
                return Ok(());
            }

            match self.sender.try_push_n(remaining) {
                Ok(written) => {
                    if written > 0 {
                        self.shared.notify_items();
                        remaining = &remaining[written..];
                        if remaining.is_empty() {
                            return Ok(());
                        }
                    }
                    // Made progress but not done, park and try again
                    parker.park();
                }
                Err(PushError::Full(())) => {
                    // No progress, park until woken
                    parker.park();
                }
                Err(PushError::Closed(())) => {
                    return Err(PushError::Closed(()));
                }
            }
        }
    }

    /// Closes the queue and wakes any waiters.
    pub fn close(&mut self) {
        self.sender.close_channel();
        self.shared.notify_items();
        self.shared.notify_space();
    }
}

/// Blocking consumer for SPSC queue.
///
/// This type provides blocking receive operations that park the thread until
/// items are available. It shares the same waker infrastructure as `AsyncSpscConsumer`,
/// allowing blocking and async operations to interoperate seamlessly.
///
/// # Interoperability
///
/// A `BlockingSpscConsumer` can wake up an `AsyncSpscProducer` and vice versa.
/// This allows mixing blocking threads with async tasks in the same queue.
///
/// # Example
///
/// ```ignore
/// // Create mixed queue: async producer, blocking consumer
/// let (async_producer, blocking_consumer) = new_async_blocking_spsc(signal);
///
/// // Producer task (async)
/// tokio::spawn(async move {
///     async_producer.send(42).await.unwrap();
/// });
///
/// // Consumer thread (blocking)
/// std::thread::spawn(move || {
///     let item = blocking_consumer.recv().unwrap();
/// });
/// ```
pub struct BlockingSpscConsumer<T, const P: usize, const NUM_SEGS_P2: usize> {
    receiver: crate::spsc::Receiver<T, P, NUM_SEGS_P2, AsyncSignalGate>,
    shared: Arc<AsyncSpscShared>,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> BlockingSpscConsumer<T, P, NUM_SEGS_P2> {
    fn new(
        receiver: crate::spsc::Receiver<T, P, NUM_SEGS_P2, AsyncSignalGate>,
        shared: Arc<AsyncSpscShared>,
    ) -> Self {
        Self { receiver, shared }
    }

    /// Capacity of the underlying queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        crate::spsc::Spsc::<T, P, NUM_SEGS_P2, AsyncSignalGate>::capacity()
    }

    /// Fast-path receive without blocking.
    ///
    /// Returns immediately with success or error. Does not block the thread.
    #[inline]
    pub fn try_recv(&self) -> Result<T, PopError> {
        match self.receiver.try_pop() {
            Some(value) => {
                self.shared.notify_space();
                Ok(value)
            }
            None if self.receiver.is_closed() => Err(PopError::Closed),
            None => Err(PopError::Empty),
        }
    }

    /// Blocking receive that parks the thread until an item is available.
    ///
    /// Uses efficient thread parking (no busy-waiting). The thread will be
    /// unparked when the producer (async or blocking) sends an item.
    ///
    /// # Correctness
    ///
    /// Uses the double-check pattern to prevent missed wakeups:
    /// 1. Try to receive
    /// 2. Register waker if empty
    /// 3. Double-check after registering (catches races)
    /// 4. Park if still empty
    ///
    /// # Performance
    ///
    /// - Fast path (item available): ~5-15ns
    /// - Blocking path: Efficient thread parking (no spinning)
    pub fn recv(&self) -> Result<T, PopError> {
        // Fast path: try immediate receive
        match self.try_recv() {
            Ok(value) => return Ok(value),
            Err(PopError::Closed) => return Err(PopError::Closed),
            Err(PopError::Empty) | Err(PopError::Timeout) => {}
        }

        // Slow path: need to block
        let parker = Parker::new();
        let unparker = parker.unparker();
        let waker = Waker::from(Arc::new(ThreadUnparker { unparker }));

        loop {
            // Register our waker
            unsafe {
                self.shared.register_items(&waker);
            }

            // Double-check after registering (prevent missed wakeup)
            match self.receiver.try_pop() {
                Some(value) => {
                    self.shared.notify_space();
                    return Ok(value);
                }
                None if self.receiver.is_closed() => {
                    return Err(PopError::Closed);
                }
                None => {
                    // Still empty, park until woken
                    parker.park();
                    // Loop again after wakeup
                }
            }
        }
    }

    /// Blocking receive of multiple items.
    ///
    /// Receives up to `dst.len()` items, blocking until at least one is available.
    /// Returns the number of items actually received.
    pub fn recv_batch(&self, dst: &mut [T]) -> Result<usize, PopError> {
        if dst.is_empty() {
            return Ok(0);
        }

        let mut filled = match self.receiver.try_pop_n(dst) {
            Ok(count) => {
                if count > 0 {
                    self.shared.notify_space();
                    return Ok(count);
                }
                0
            }
            Err(PopError::Empty) | Err(PopError::Timeout) => 0,
            Err(PopError::Closed) => return Err(PopError::Closed),
        };

        // Slow path: need to block
        let parker = Parker::new();
        let unparker = parker.unparker();
        let waker = Waker::from(Arc::new(ThreadUnparker { unparker }));

        loop {
            // Register our waker
            unsafe {
                self.shared.register_items(&waker);
            }

            // Double-check and try to make progress
            match self.receiver.try_pop_n(&mut dst[filled..]) {
                Ok(0) => {
                    if self.receiver.is_closed() {
                        return if filled > 0 {
                            Ok(filled)
                        } else {
                            Err(PopError::Closed)
                        };
                    }
                    // No items, park until woken
                    parker.park();
                }
                Ok(count) => {
                    filled += count;
                    self.shared.notify_space();
                    if filled == dst.len() || self.receiver.is_closed() {
                        return Ok(filled);
                    }
                    // Got some but not all, park and try again
                    parker.park();
                }
                Err(PopError::Empty) | Err(PopError::Timeout) => {
                    if self.receiver.is_closed() {
                        return if filled > 0 {
                            Ok(filled)
                        } else {
                            Err(PopError::Closed)
                        };
                    }
                    parker.park();
                }
                Err(PopError::Closed) => {
                    return if filled > 0 {
                        Ok(filled)
                    } else {
                        Err(PopError::Closed)
                    };
                }
            }
        }
    }
}

/// Creates a default async segmented SPSC queue.
pub fn new_async_spsc<T, const P: usize, const NUM_SEGS_P2: usize>(
    signal: AsyncSignalGate,
) -> (
    AsyncSpscProducer<T, P, NUM_SEGS_P2>,
    AsyncSpscConsumer<T, P, NUM_SEGS_P2>,
) {
    new_async_with_config::<T, P, NUM_SEGS_P2>(signal, usize::MAX)
}

/// Creates an async segmented SPSC queue with a custom pooling target.
pub fn new_async_with_config<T, const P: usize, const NUM_SEGS_P2: usize>(
    signal: AsyncSignalGate,
    max_pooled_segments: usize,
) -> (
    AsyncSpscProducer<T, P, NUM_SEGS_P2>,
    AsyncSpscConsumer<T, P, NUM_SEGS_P2>,
) {
    let shared = Arc::new(AsyncSpscShared::new());
    let (sender, receiver) =
        crate::spsc::Spsc::<T, P, NUM_SEGS_P2, AsyncSignalGate>::new_with_gate_and_config(
            signal,
            max_pooled_segments,
        );
    (
        AsyncSpscProducer::new(sender, Arc::clone(&shared)),
        AsyncSpscConsumer::new(receiver, shared),
    )
}

/// Creates a default blocking segmented SPSC queue.
///
/// Both producer and consumer use blocking operations that park the thread.
///
/// # Example
///
/// ```ignore
/// let (producer, consumer) = new_blocking_spsc(signal);
///
/// // Producer thread
/// std::thread::spawn(move || {
///     producer.send(42).unwrap();
/// });
///
/// // Consumer thread
/// std::thread::spawn(move || {
///     let item = consumer.recv().unwrap();
/// });
/// ```
pub fn new_blocking_spsc<T, const P: usize, const NUM_SEGS_P2: usize>(
    signal: AsyncSignalGate,
) -> (
    BlockingSpscProducer<T, P, NUM_SEGS_P2>,
    BlockingSpscConsumer<T, P, NUM_SEGS_P2>,
) {
    new_blocking_with_config::<T, P, NUM_SEGS_P2>(signal, usize::MAX)
}

/// Creates a blocking segmented SPSC queue with a custom pooling target.
pub fn new_blocking_with_config<T, const P: usize, const NUM_SEGS_P2: usize>(
    signal: AsyncSignalGate,
    max_pooled_segments: usize,
) -> (
    BlockingSpscProducer<T, P, NUM_SEGS_P2>,
    BlockingSpscConsumer<T, P, NUM_SEGS_P2>,
) {
    let shared = Arc::new(AsyncSpscShared::new());
    let (sender, receiver) =
        crate::spsc::Spsc::<T, P, NUM_SEGS_P2, AsyncSignalGate>::new_with_gate_and_config(
            signal,
            max_pooled_segments,
        );
    (
        BlockingSpscProducer::new(sender, Arc::clone(&shared)),
        BlockingSpscConsumer::new(receiver, shared),
    )
}

/// Creates a mixed SPSC queue with blocking producer and async consumer.
///
/// The blocking producer and async consumer share the same waker infrastructure,
/// so they can wake each other efficiently. This is useful when you have a
/// blocking thread that needs to send data to an async task.
///
/// # Example
///
/// ```ignore
/// let (producer, consumer) = new_blocking_async_spsc(signal);
///
/// // Producer thread (blocking)
/// std::thread::spawn(move || {
///     producer.send(42).unwrap();
///     producer.send(43).unwrap();
/// });
///
/// // Consumer task (async)
/// tokio::spawn(async move {
///     while let Some(item) = consumer.next().await {
///         println!("Got: {}", item);
///     }
/// });
/// ```
pub fn new_blocking_async_spsc<T, const P: usize, const NUM_SEGS_P2: usize>(
    signal: AsyncSignalGate,
) -> (
    BlockingSpscProducer<T, P, NUM_SEGS_P2>,
    AsyncSpscConsumer<T, P, NUM_SEGS_P2>,
) {
    new_blocking_async_with_config::<T, P, NUM_SEGS_P2>(signal, usize::MAX)
}

/// Creates a mixed SPSC queue with blocking producer and async consumer, with custom pooling.
pub fn new_blocking_async_with_config<T, const P: usize, const NUM_SEGS_P2: usize>(
    signal: AsyncSignalGate,
    max_pooled_segments: usize,
) -> (
    BlockingSpscProducer<T, P, NUM_SEGS_P2>,
    AsyncSpscConsumer<T, P, NUM_SEGS_P2>,
) {
    let shared = Arc::new(AsyncSpscShared::new());
    let (sender, receiver) =
        crate::spsc::Spsc::<T, P, NUM_SEGS_P2, AsyncSignalGate>::new_with_gate_and_config(
            signal,
            max_pooled_segments,
        );
    (
        BlockingSpscProducer::new(sender, Arc::clone(&shared)),
        AsyncSpscConsumer::new(receiver, shared),
    )
}

/// Creates a mixed SPSC queue with async producer and blocking consumer.
///
/// The async producer and blocking consumer share the same waker infrastructure,
/// so they can wake each other efficiently. This is useful when you have an
/// async task that needs to send data to a blocking thread.
///
/// # Example
///
/// ```ignore
/// let (producer, consumer) = new_async_blocking_spsc(signal);
///
/// // Producer task (async)
/// tokio::spawn(async move {
///     producer.send(42).await.unwrap();
///     producer.send(43).await.unwrap();
/// });
///
/// // Consumer thread (blocking)
/// std::thread::spawn(move || {
///     while let Ok(item) = consumer.recv() {
///         println!("Got: {}", item);
///     }
/// });
/// ```
pub fn new_async_blocking_spsc<T, const P: usize, const NUM_SEGS_P2: usize>(
    signal: AsyncSignalGate,
) -> (
    AsyncSpscProducer<T, P, NUM_SEGS_P2>,
    BlockingSpscConsumer<T, P, NUM_SEGS_P2>,
) {
    new_async_blocking_with_config::<T, P, NUM_SEGS_P2>(signal, usize::MAX)
}

/// Creates a mixed SPSC queue with async producer and blocking consumer, with custom pooling.
pub fn new_async_blocking_with_config<T, const P: usize, const NUM_SEGS_P2: usize>(
    signal: AsyncSignalGate,
    max_pooled_segments: usize,
) -> (
    AsyncSpscProducer<T, P, NUM_SEGS_P2>,
    BlockingSpscConsumer<T, P, NUM_SEGS_P2>,
) {
    let shared = Arc::new(AsyncSpscShared::new());
    let (sender, receiver) =
        crate::spsc::Spsc::<T, P, NUM_SEGS_P2, AsyncSignalGate>::new_with_gate_and_config(
            signal,
            max_pooled_segments,
        );
    (
        AsyncSpscProducer::new(sender, Arc::clone(&shared)),
        BlockingSpscConsumer::new(receiver, shared),
    )
}
