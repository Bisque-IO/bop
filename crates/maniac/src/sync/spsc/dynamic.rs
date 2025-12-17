//! Dynamic Async and Blocking SPSC (Single-Producer, Single-Consumer) queue implementation.
//!
//! This module provides async and blocking adapters over the runtime-configured `DynSpsc` queue.
//! Unlike the const-generic version, queue sizes are determined at runtime via `DynSpscConfig`.
//!
//! # Queue Variants
//!
//! - **Async**: [`AsyncDynSpscProducer`] / [`AsyncDynSpscConsumer`] - For use with async tasks
//! - **Blocking**: [`BlockingDynSpscProducer`] / [`BlockingDynSpscConsumer`] - For use with threads
//! - **Mixed**: You can mix async and blocking ends on the same queue!
//!
//! # Trade-offs vs Const Generic Version
//!
//! | Aspect | Const Generic (`AsyncSpscProducer<T, P, N>`) | Dynamic (`AsyncDynSpscProducer<T>`) |
//! |--------|----------------------------------------------|-------------------------------------|
//! | Configuration | Compile-time | Runtime |
//! | Performance | Optimal (constants inlined) | Slightly slower (runtime loads) |
//! | Binary size | Larger (monomorphization) | Smaller |
//! | Flexibility | Fixed at compile time | Configurable |
//!
//! # Examples
//!
//! ## Pure Async with Runtime Configuration
//!
//! ```ignore
//! use maniac::spsc::DynSpscConfig;
//! use maniac::sync::dyn_spsc::new_async_dyn_spsc;
//!
//! // Configure at runtime: 64 items/segment, 256 segments
//! let config = DynSpscConfig::new(6, 8);
//! let (mut producer, mut consumer) = new_async_dyn_spsc(config, signal);
//!
//! producer.send(42).await.unwrap();
//! let item = consumer.recv().await.unwrap();
//! ```
//!
//! ## Mixed: Blocking Producer + Async Consumer
//!
//! ```ignore
//! let config = DynSpscConfig::new(6, 8);
//! let (producer, mut consumer) = new_blocking_async_dyn_spsc(config, signal);
//!
//! std::thread::spawn(move || {
//!     producer.send(42).unwrap();
//! });
//!
//! let item = consumer.recv().await.unwrap();
//! ```

use std::sync::Arc;

use crate::utils::CachePadded;

use crate::sync::signal::AsyncSignalGate;

pub use crate::detail::spsc::dynamic::DynSpscConfig;
use crate::detail::spsc::dynamic::{DynSpsc, Receiver, Sender};
use crate::{PopError, PushError};

use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::{sink::Sink, stream::Stream};

use crate::future::waker::DiatomicWaker;
use crate::parking::{Parker, Unparker};
use std::task::Wake;

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
    unsafe fn wait_for_items<P, T>(&self, predicate: P) -> crate::future::waker::WaitUntil<'_, P, T>
    where
        P: FnMut() -> Option<T>,
    {
        unsafe { self.item_waiter.wait_until(predicate) }
    }

    #[inline]
    unsafe fn wait_for_space<P, T>(&self, predicate: P) -> crate::future::waker::WaitUntil<'_, P, T>
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

/// Asynchronous producer for a dynamically-configured SPSC queue.
pub struct AsyncProducer<T> {
    sender: Sender<T, AsyncSignalGate>,
    shared: Arc<AsyncShared>,
}

impl<T> AsyncProducer<T> {
    fn new(sender: Sender<T, AsyncSignalGate>, shared: Arc<AsyncShared>) -> Self {
        Self { sender, shared }
    }

    /// Capacity of the underlying queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.sender.capacity()
    }

    /// Fast-path send without suspension.
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

    /// Asynchronously sends a single item.
    pub async fn send(&mut self, value: T) -> Result<(), PushError<T>> {
        match self.try_send(value) {
            Ok(()) => Ok(()),
            Err(PushError::Full(item)) => {
                let mut pending = Some(item);
                let sender = &self.sender;
                let shared = &self.shared;
                unsafe {
                    shared
                        .wait_for_space(|| {
                            let candidate = pending.take()?;
                            match sender.try_push(candidate) {
                                Ok(()) => {
                                    shared.notify_items();
                                    Some(Ok(()))
                                }
                                Err(PushError::Full(candidate)) => {
                                    pending = Some(candidate);
                                    None
                                }
                                Err(PushError::Closed(candidate)) => {
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

    /// Sends an entire Vec, awaiting at most once if the queue fills.
    pub async fn send_batch(&mut self, values: &mut Vec<T>) -> Result<(), PushError<()>> {
        if values.is_empty() {
            return Ok(());
        }

        let sender = &self.sender;
        let shared = &self.shared;

        match sender.try_push_n(values) {
            Ok(written) => {
                if written > 0 {
                    shared.notify_items();
                    if values.is_empty() {
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
                    if values.is_empty() {
                        return Some(Ok(()));
                    }

                    match sender.try_push_n(values) {
                        Ok(written) => {
                            if written > 0 {
                                shared.notify_items();
                                if values.is_empty() {
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
    pub async fn send_iter<I>(&mut self, iter: I) -> Result<(), PushError<T>>
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

impl<T> Sink<T> for AsyncProducer<T> {
    type Error = PushError<T>;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        let this = unsafe { self.get_unchecked_mut() };

        if !this.sender.is_full() {
            return Poll::Ready(Ok(()));
        }

        unsafe {
            this.shared.register_space(cx.waker());
        }

        if !this.sender.is_full() {
            return Poll::Ready(Ok(()));
        }

        Poll::Pending
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), PushError<T>> {
        let this = unsafe { self.get_unchecked_mut() };
        this.try_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        let this = unsafe { self.get_unchecked_mut() };
        this.close();
        Poll::Ready(Ok(()))
    }
}

/// Asynchronous consumer for a dynamically-configured SPSC queue.
pub struct AsyncConsumer<T> {
    receiver: Receiver<T, AsyncSignalGate>,
    shared: Arc<AsyncShared>,
}

impl<T> AsyncConsumer<T> {
    fn new(receiver: Receiver<T, AsyncSignalGate>, shared: Arc<AsyncShared>) -> Self {
        Self { receiver, shared }
    }

    /// Capacity of the underlying queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.receiver.capacity()
    }

    /// Attempts to receive without awaiting.
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

    /// Asynchronously receives a single item.
    pub async fn recv(&mut self) -> Result<T, PopError> {
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
                        shared.notify_space();
                        Some(Ok(value))
                    }
                    None if receiver.is_closed() => Some(Err(PopError::Closed)),
                    None => None,
                })
                .await
        }
    }

    /// Receives up to `dst.len()` items.
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

impl<T> Stream for AsyncConsumer<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = unsafe { self.get_unchecked_mut() };

        match this.try_recv() {
            Ok(value) => Poll::Ready(Some(value)),
            Err(PopError::Closed) => Poll::Ready(None),
            Err(PopError::Empty) | Err(PopError::Timeout) => {
                unsafe {
                    this.shared.register_items(cx.waker());
                }

                match this.try_recv() {
                    Ok(value) => Poll::Ready(Some(value)),
                    Err(PopError::Closed) => Poll::Ready(None),
                    Err(PopError::Empty) | Err(PopError::Timeout) => Poll::Pending,
                }
            }
        }
    }
}

/// Blocking producer for a dynamically-configured SPSC queue.
pub struct BlockingProducer<T> {
    sender: Sender<T, AsyncSignalGate>,
    shared: Arc<AsyncShared>,
    parker: Parker,
    parker_waker: Arc<ThreadUnparker>,
}

impl<T> BlockingProducer<T> {
    fn new(sender: Sender<T, AsyncSignalGate>, shared: Arc<AsyncShared>) -> Self {
        let parker = Parker::new();
        let parker_waker = Arc::new(ThreadUnparker {
            unparker: parker.unparker(),
        });
        Self {
            sender,
            shared,
            parker,
            parker_waker,
        }
    }

    /// Capacity of the underlying queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.sender.capacity()
    }

    /// Fast-path send without blocking.
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
    pub fn send(&self, mut value: T) -> Result<(), PushError<T>> {
        match self.try_send(value) {
            Ok(()) => return Ok(()),
            Err(PushError::Closed(item)) => return Err(PushError::Closed(item)),
            Err(PushError::Full(item)) => value = item,
        }

        let waker = Waker::from(Arc::clone(&self.parker_waker));

        loop {
            unsafe {
                self.shared.register_space(&waker);
            }

            match self.sender.try_push(value) {
                Ok(()) => {
                    self.shared.notify_items();
                    return Ok(());
                }
                Err(PushError::Full(item)) => {
                    value = item;
                    self.parker.park();
                }
                Err(PushError::Closed(item)) => {
                    return Err(PushError::Closed(item));
                }
            }
        }
    }

    /// Blocking send of a Vec.
    pub fn send_slice(&self, values: &mut Vec<T>) -> Result<(), PushError<()>> {
        if values.is_empty() {
            return Ok(());
        }

        match self.sender.try_push_n(values) {
            Ok(written) => {
                if written > 0 {
                    self.shared.notify_items();
                    if values.is_empty() {
                        return Ok(());
                    }
                }
            }
            Err(PushError::Closed(())) => return Err(PushError::Closed(())),
            Err(PushError::Full(())) => {}
        }

        let waker = Waker::from(Arc::clone(&self.parker_waker));

        loop {
            unsafe {
                self.shared.register_space(&waker);
            }

            if values.is_empty() {
                return Ok(());
            }

            match self.sender.try_push_n(values) {
                Ok(written) => {
                    if written > 0 {
                        self.shared.notify_items();
                        if values.is_empty() {
                            return Ok(());
                        }
                    }
                    self.parker.park();
                }
                Err(PushError::Full(())) => {
                    self.parker.park();
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

/// Blocking consumer for a dynamically-configured SPSC queue.
pub struct BlockingConsumer<T> {
    receiver: Receiver<T, AsyncSignalGate>,
    shared: Arc<AsyncShared>,
    parker: Parker,
    parker_waker: Arc<ThreadUnparker>,
}

impl<T> BlockingConsumer<T> {
    fn new(receiver: Receiver<T, AsyncSignalGate>, shared: Arc<AsyncShared>) -> Self {
        let parker = Parker::new();
        let parker_waker = Arc::new(ThreadUnparker {
            unparker: parker.unparker(),
        });
        Self {
            receiver,
            shared,
            parker,
            parker_waker,
        }
    }

    /// Capacity of the underlying queue.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.receiver.capacity()
    }

    /// Fast-path receive without blocking.
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
    pub fn recv(&self) -> Result<T, PopError> {
        match self.try_recv() {
            Ok(value) => return Ok(value),
            Err(PopError::Closed) => return Err(PopError::Closed),
            Err(PopError::Empty) | Err(PopError::Timeout) => {}
        }

        let waker = Waker::from(Arc::clone(&self.parker_waker));

        loop {
            unsafe {
                self.shared.register_items(&waker);
            }

            match self.receiver.try_pop() {
                Some(value) => {
                    self.shared.notify_space();
                    return Ok(value);
                }
                None if self.receiver.is_closed() => {
                    return Err(PopError::Closed);
                }
                None => {
                    self.parker.park();
                }
            }
        }
    }

    /// Blocking receive of multiple items.
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

        let waker = Waker::from(Arc::clone(&self.parker_waker));

        loop {
            unsafe {
                self.shared.register_items(&waker);
            }

            match self.receiver.try_pop_n(&mut dst[filled..]) {
                Ok(0) => {
                    if self.receiver.is_closed() {
                        return if filled > 0 {
                            Ok(filled)
                        } else {
                            Err(PopError::Closed)
                        };
                    }
                    self.parker.park();
                }
                Ok(count) => {
                    filled += count;
                    self.shared.notify_space();
                    if filled == dst.len() || self.receiver.is_closed() {
                        return Ok(filled);
                    }
                    self.parker.park();
                }
                Err(PopError::Empty) | Err(PopError::Timeout) => {
                    if self.receiver.is_closed() {
                        return if filled > 0 {
                            Ok(filled)
                        } else {
                            Err(PopError::Closed)
                        };
                    }
                    self.parker.park();
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

// ══════════════════════════════════════════════════════════════════════════════
// Factory Functions
// ══════════════════════════════════════════════════════════════════════════════

/// Creates a default async dynamically-configured SPSC queue.
///
/// # Arguments
///
/// * `config` - Runtime configuration for segment size and directory size
/// * `signal` - Signal gate for executor integration
///
/// # Example
///
/// ```ignore
/// let config = DynSpscConfig::new(6, 8); // 64 items/segment, 256 segments
/// let (mut producer, mut consumer) = new_async_dyn_spsc(config, signal);
///
/// producer.send(42).await.unwrap();
/// ```
pub fn new<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
) -> (AsyncProducer<T>, AsyncConsumer<T>) {
    new_with_config(config, signal, usize::MAX)
}

/// Creates an async dynamically-configured SPSC queue with a custom pooling target.
pub fn new_with_config<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
    max_pooled_segments: usize,
) -> (AsyncProducer<T>, AsyncConsumer<T>) {
    let config = DynSpscConfig::with_max_pooled(config.p, config.num_segs_p2, max_pooled_segments);
    let shared = Arc::new(AsyncShared::new());
    let (sender, receiver) = DynSpsc::<T, AsyncSignalGate>::new_with_config(config, signal);
    (
        AsyncProducer::new(sender, Arc::clone(&shared)),
        AsyncConsumer::new(receiver, shared),
    )
}

/// Creates a default blocking dynamically-configured SPSC queue.
///
/// # Example
///
/// ```ignore
/// let config = DynSpscConfig::new(6, 8);
/// let (producer, consumer) = new_blocking_dyn_spsc(config, signal);
///
/// std::thread::spawn(move || {
///     producer.send(42).unwrap();
/// });
///
/// std::thread::spawn(move || {
///     let item = consumer.recv().unwrap();
/// });
/// ```
pub fn new_blocking<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
) -> (BlockingProducer<T>, BlockingConsumer<T>) {
    new_blocking_dyn_with_config(config, signal, usize::MAX)
}

/// Creates a blocking dynamically-configured SPSC queue with a custom pooling target.
pub fn new_blocking_dyn_with_config<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
    max_pooled_segments: usize,
) -> (BlockingProducer<T>, BlockingConsumer<T>) {
    let config = DynSpscConfig::with_max_pooled(config.p, config.num_segs_p2, max_pooled_segments);
    let shared = Arc::new(AsyncShared::new());
    let (sender, receiver) = DynSpsc::<T, AsyncSignalGate>::new_with_config(config, signal);
    (
        BlockingProducer::new(sender, Arc::clone(&shared)),
        BlockingConsumer::new(receiver, shared),
    )
}

/// Creates a mixed SPSC queue with blocking producer and async consumer.
///
/// # Example
///
/// ```ignore
/// let config = DynSpscConfig::new(6, 8);
/// let (producer, consumer) = new_blocking_async_dyn_spsc(config, signal);
///
/// std::thread::spawn(move || {
///     producer.send(42).unwrap();
/// });
///
/// maniac::spawn(async move {
///     let item = consumer.recv().await.unwrap();
/// });
/// ```
pub fn new_blocking_async<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
) -> (BlockingProducer<T>, AsyncConsumer<T>) {
    new_blocking_async_with_config(config, signal, usize::MAX)
}

/// Creates a mixed SPSC queue with blocking producer and async consumer, with custom pooling.
pub fn new_blocking_async_with_config<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
    max_pooled_segments: usize,
) -> (BlockingProducer<T>, AsyncConsumer<T>) {
    let config = DynSpscConfig::with_max_pooled(config.p, config.num_segs_p2, max_pooled_segments);
    let shared = Arc::new(AsyncShared::new());
    let (sender, receiver) = DynSpsc::<T, AsyncSignalGate>::new_with_config(config, signal);
    (
        BlockingProducer::new(sender, Arc::clone(&shared)),
        AsyncConsumer::new(receiver, shared),
    )
}

/// Creates a mixed SPSC queue with async producer and blocking consumer.
///
/// # Example
///
/// ```ignore
/// let config = DynSpscConfig::new(6, 8);
/// let (producer, consumer) = new_async_blocking_dyn_spsc(config, signal);
///
/// maniac::spawn(async move {
///     producer.send(42).await.unwrap();
/// });
///
/// std::thread::spawn(move || {
///     let item = consumer.recv().unwrap();
/// });
/// ```
pub fn new_async_blocking<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
) -> (AsyncProducer<T>, BlockingConsumer<T>) {
    new_async_blocking_with_config(config, signal, usize::MAX)
}

/// Creates a mixed SPSC queue with async producer and blocking consumer, with custom pooling.
pub fn new_async_blocking_with_config<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
    max_pooled_segments: usize,
) -> (AsyncProducer<T>, BlockingConsumer<T>) {
    let config = DynSpscConfig::with_max_pooled(config.p, config.num_segs_p2, max_pooled_segments);
    let shared = Arc::new(AsyncShared::new());
    let (sender, receiver) = DynSpsc::<T, AsyncSignalGate>::new_with_config(config, signal);
    (
        AsyncProducer::new(sender, Arc::clone(&shared)),
        BlockingConsumer::new(receiver, shared),
    )
}

// ══════════════════════════════════════════════════════════════════════════════
// Unbounded Dynamic SPSC Variants
// ══════════════════════════════════════════════════════════════════════════════

use crate::detail::spsc::dynamic::unbounded::{
    DynUnboundedReceiver as DynUnboundedReceiver_, DynUnboundedSender as DynUnboundedSender_,
    DynUnboundedSpsc,
};

/// Asynchronous producer for an unbounded dynamically-configured SPSC queue.
///
/// This type provides async send operations for an unbounded queue that automatically
/// expands by creating new segments as needed. Never blocks on full queue since the
/// queue grows dynamically.
pub struct AsyncUnboundedProducer<T> {
    sender: DynUnboundedSender_<T, Arc<AsyncSignalGate>>,
    shared: Arc<AsyncShared>,
}

impl<T> AsyncUnboundedProducer<T> {
    fn new(sender: DynUnboundedSender_<T, Arc<AsyncSignalGate>>, shared: Arc<AsyncShared>) -> Self {
        Self { sender, shared }
    }

    /// Returns the runtime configuration.
    #[inline]
    pub fn config(&self) -> DynSpscConfig {
        self.sender.config()
    }

    /// Fast-path send without suspension.
    ///
    /// For unbounded queues, this always succeeds unless the channel is closed.
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

    /// Asynchronously sends a single item.
    ///
    /// For unbounded queues, this typically completes immediately unless the channel
    /// is closed. The async interface is provided for API consistency.
    pub async fn send(&mut self, value: T) -> Result<(), PushError<T>> {
        self.try_send(value)
    }

    /// Sends an entire Vec, moving items out using bulk operations.
    ///
    /// On return, the Vec will be empty if all items were sent, or contain only
    /// the items that were not sent (if the channel closed).
    pub async fn send_batch(&mut self, values: &mut Vec<T>) -> Result<(), PushError<()>> {
        if values.is_empty() {
            return Ok(());
        }

        match self.sender.try_push_n(values) {
            Ok(_) => {
                self.shared.notify_items();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Sends every item from the iterator.
    pub async fn send_iter<I>(&mut self, iter: I) -> Result<(), PushError<T>>
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

    /// Returns the number of nodes in the unbounded queue.
    pub fn node_count(&self) -> usize {
        self.sender.node_count()
    }

    /// Returns the total capacity across all nodes.
    pub fn total_capacity(&self) -> usize {
        self.sender.total_capacity()
    }

    /// Returns the current length (approximate).
    pub fn len(&self) -> usize {
        self.sender.len()
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.sender.is_empty()
    }
}

impl<T> Sink<T> for AsyncUnboundedProducer<T> {
    type Error = PushError<T>;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        // Unbounded queue is always ready (unless closed)
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), PushError<T>> {
        let this = unsafe { self.get_unchecked_mut() };
        this.try_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        let this = unsafe { self.get_unchecked_mut() };
        this.close();
        Poll::Ready(Ok(()))
    }
}

/// Asynchronous consumer for an unbounded dynamically-configured SPSC queue.
pub struct AsyncUnboundedConsumer<T> {
    receiver: DynUnboundedReceiver_<T, Arc<AsyncSignalGate>>,
    shared: Arc<AsyncShared>,
}

impl<T> AsyncUnboundedConsumer<T> {
    fn new(
        receiver: DynUnboundedReceiver_<T, Arc<AsyncSignalGate>>,
        shared: Arc<AsyncShared>,
    ) -> Self {
        Self { receiver, shared }
    }

    /// Returns the runtime configuration.
    #[inline]
    pub fn config(&self) -> DynSpscConfig {
        self.receiver.config()
    }

    /// Attempts to receive without awaiting.
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

    /// Asynchronously receives a single item.
    pub async fn recv(&mut self) -> Result<T, PopError> {
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
                        shared.notify_space();
                        Some(Ok(value))
                    }
                    None if receiver.is_closed() => Some(Err(PopError::Closed)),
                    None => None,
                })
                .await
        }
    }

    /// Receives up to `dst.len()` items.
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

    /// Returns the number of nodes in the unbounded queue.
    pub fn node_count(&self) -> usize {
        self.receiver.node_count()
    }

    /// Returns the total capacity across all nodes.
    pub fn total_capacity(&self) -> usize {
        self.receiver.total_capacity()
    }

    /// Returns the current length (approximate).
    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }

    /// Returns true if the channel is closed.
    pub fn is_closed(&self) -> bool {
        self.receiver.is_closed()
    }
}

impl<T> Stream for AsyncUnboundedConsumer<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = unsafe { self.get_unchecked_mut() };

        match this.try_recv() {
            Ok(value) => Poll::Ready(Some(value)),
            Err(PopError::Closed) => Poll::Ready(None),
            Err(PopError::Empty) | Err(PopError::Timeout) => {
                unsafe {
                    this.shared.register_items(cx.waker());
                }

                match this.try_recv() {
                    Ok(value) => Poll::Ready(Some(value)),
                    Err(PopError::Closed) => Poll::Ready(None),
                    Err(PopError::Empty) | Err(PopError::Timeout) => Poll::Pending,
                }
            }
        }
    }
}

/// Blocking producer for an unbounded dynamically-configured SPSC queue.
pub struct BlockingUnboundedProducer<T> {
    sender: DynUnboundedSender_<T, Arc<AsyncSignalGate>>,
    shared: Arc<AsyncShared>,
}

impl<T> BlockingUnboundedProducer<T> {
    fn new(sender: DynUnboundedSender_<T, Arc<AsyncSignalGate>>, shared: Arc<AsyncShared>) -> Self {
        Self { sender, shared }
    }

    /// Returns the runtime configuration.
    #[inline]
    pub fn config(&self) -> DynSpscConfig {
        self.sender.config()
    }

    /// Fast-path send without blocking.
    ///
    /// For unbounded queues, this always succeeds unless the channel is closed.
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

    /// Sends a single item.
    ///
    /// For unbounded queues, this never blocks since the queue grows dynamically.
    pub fn send(&self, value: T) -> Result<(), PushError<T>> {
        self.try_send(value)
    }

    /// Sends a Vec of items using bulk operations.
    ///
    /// On return, the Vec will be empty if all items were sent, or contain only
    /// the items that were not sent (if the channel closed).
    pub fn send_slice(&self, values: &mut Vec<T>) -> Result<(), PushError<()>> {
        if values.is_empty() {
            return Ok(());
        }

        match self.sender.try_push_n(values) {
            Ok(_) => {
                self.shared.notify_items();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Closes the queue and wakes any waiters.
    pub fn close(&mut self) {
        self.sender.close_channel();
        self.shared.notify_items();
        self.shared.notify_space();
    }

    /// Returns the number of nodes in the unbounded queue.
    pub fn node_count(&self) -> usize {
        self.sender.node_count()
    }

    /// Returns the total capacity across all nodes.
    pub fn total_capacity(&self) -> usize {
        self.sender.total_capacity()
    }

    /// Returns the current length (approximate).
    pub fn len(&self) -> usize {
        self.sender.len()
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.sender.is_empty()
    }
}

/// Blocking consumer for an unbounded dynamically-configured SPSC queue.
pub struct BlockingUnboundedConsumer<T> {
    receiver: DynUnboundedReceiver_<T, Arc<AsyncSignalGate>>,
    shared: Arc<AsyncShared>,
    parker: Parker,
    parker_waker: Arc<ThreadUnparker>,
}

impl<T> BlockingUnboundedConsumer<T> {
    fn new(
        receiver: DynUnboundedReceiver_<T, Arc<AsyncSignalGate>>,
        shared: Arc<AsyncShared>,
    ) -> Self {
        let parker = Parker::new();
        let parker_waker = Arc::new(ThreadUnparker {
            unparker: parker.unparker(),
        });
        Self {
            receiver,
            shared,
            parker,
            parker_waker,
        }
    }

    /// Returns the runtime configuration.
    #[inline]
    pub fn config(&self) -> DynSpscConfig {
        self.receiver.config()
    }

    /// Fast-path receive without blocking.
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
    pub fn recv(&self) -> Result<T, PopError> {
        match self.try_recv() {
            Ok(value) => return Ok(value),
            Err(PopError::Closed) => return Err(PopError::Closed),
            Err(PopError::Empty) | Err(PopError::Timeout) => {}
        }

        let waker = Waker::from(Arc::clone(&self.parker_waker));

        loop {
            unsafe {
                self.shared.register_items(&waker);
            }

            match self.receiver.try_pop() {
                Some(value) => {
                    self.shared.notify_space();
                    return Ok(value);
                }
                None if self.receiver.is_closed() => {
                    return Err(PopError::Closed);
                }
                None => {
                    self.parker.park();
                }
            }
        }
    }

    /// Blocking receive of multiple items.
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

        let waker = Waker::from(Arc::clone(&self.parker_waker));

        loop {
            unsafe {
                self.shared.register_items(&waker);
            }

            match self.receiver.try_pop_n(&mut dst[filled..]) {
                Ok(0) => {
                    if self.receiver.is_closed() {
                        return if filled > 0 {
                            Ok(filled)
                        } else {
                            Err(PopError::Closed)
                        };
                    }
                    self.parker.park();
                }
                Ok(count) => {
                    filled += count;
                    self.shared.notify_space();
                    if filled == dst.len() || self.receiver.is_closed() {
                        return Ok(filled);
                    }
                    self.parker.park();
                }
                Err(PopError::Empty) | Err(PopError::Timeout) => {
                    if self.receiver.is_closed() {
                        return if filled > 0 {
                            Ok(filled)
                        } else {
                            Err(PopError::Closed)
                        };
                    }
                    self.parker.park();
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

    /// Returns the number of nodes in the unbounded queue.
    pub fn node_count(&self) -> usize {
        self.receiver.node_count()
    }

    /// Returns the total capacity across all nodes.
    pub fn total_capacity(&self) -> usize {
        self.receiver.total_capacity()
    }

    /// Returns the current length (approximate).
    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }

    /// Returns true if the channel is closed.
    pub fn is_closed(&self) -> bool {
        self.receiver.is_closed()
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Unbounded Factory Functions
// ══════════════════════════════════════════════════════════════════════════════

/// Creates a default async unbounded dynamically-configured SPSC queue.
///
/// The queue automatically grows by creating new segments as needed, so it never
/// blocks on full. This is ideal for scenarios where you want to avoid backpressure.
///
/// # Arguments
///
/// * `config` - Runtime configuration for segment size and directory size
/// * `signal` - Signal gate for executor integration
///
/// # Example
///
/// ```ignore
/// let config = DynSpscConfig::new(6, 8);
/// let (mut producer, mut consumer) = new_unbounded(config, signal);
///
/// // Producer never blocks on full
/// producer.send(42).await.unwrap();
/// producer.send(43).await.unwrap();
///
/// // Consumer receives items
/// assert_eq!(consumer.recv().await.unwrap(), 42);
/// ```
pub fn new_unbounded<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
) -> (AsyncUnboundedProducer<T>, AsyncUnboundedConsumer<T>) {
    let shared = Arc::new(AsyncShared::new());
    let signal_arc = Arc::new(signal);
    let (sender, receiver) =
        DynUnboundedSpsc::<T, Arc<AsyncSignalGate>>::new_with_signal(config, signal_arc);
    (
        AsyncUnboundedProducer::new(sender, Arc::clone(&shared)),
        AsyncUnboundedConsumer::new(receiver, shared),
    )
}

/// Creates a default blocking unbounded dynamically-configured SPSC queue.
///
/// The queue automatically grows by creating new segments as needed, so the producer
/// never blocks on full. The consumer blocks when empty.
///
/// # Example
///
/// ```ignore
/// let config = DynSpscConfig::new(6, 8);
/// let (producer, consumer) = new_blocking_unbounded(config, signal);
///
/// // Producer thread
/// std::thread::spawn(move || {
///     producer.send(42).unwrap();  // Never blocks on full
/// });
///
/// // Consumer thread
/// std::thread::spawn(move || {
///     let item = consumer.recv().unwrap();  // Blocks until item available
/// });
/// ```
pub fn new_blocking_unbounded<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
) -> (BlockingUnboundedProducer<T>, BlockingUnboundedConsumer<T>) {
    let shared = Arc::new(AsyncShared::new());
    let signal_arc = Arc::new(signal);
    let (sender, receiver) =
        DynUnboundedSpsc::<T, Arc<AsyncSignalGate>>::new_with_signal(config, signal_arc);
    (
        BlockingUnboundedProducer::new(sender, Arc::clone(&shared)),
        BlockingUnboundedConsumer::new(receiver, shared),
    )
}

/// Creates a mixed unbounded SPSC queue with blocking producer and async consumer.
///
/// # Example
///
/// ```ignore
/// let config = DynSpscConfig::new(6, 8);
/// let (producer, consumer) = new_blocking_async_unbounded(config, signal);
///
/// // Producer thread (blocking)
/// std::thread::spawn(move || {
///     producer.send(42).unwrap();  // Never blocks on full
/// });
///
/// // Consumer task (async)
/// maniac::spawn(async move {
///     let item = consumer.recv().await.unwrap();
/// });
/// ```
pub fn new_blocking_async_unbounded<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
) -> (BlockingUnboundedProducer<T>, AsyncUnboundedConsumer<T>) {
    let shared = Arc::new(AsyncShared::new());
    let signal_arc = Arc::new(signal);
    let (sender, receiver) =
        DynUnboundedSpsc::<T, Arc<AsyncSignalGate>>::new_with_signal(config, signal_arc);
    (
        BlockingUnboundedProducer::new(sender, Arc::clone(&shared)),
        AsyncUnboundedConsumer::new(receiver, shared),
    )
}

/// Creates a mixed unbounded SPSC queue with async producer and blocking consumer.
///
/// # Example
///
/// ```ignore
/// let config = DynSpscConfig::new(6, 8);
/// let (producer, consumer) = new_async_blocking_unbounded(config, signal);
///
/// // Producer task (async)
/// maniac::spawn(async move {
///     producer.send(42).await.unwrap();  // Never blocks on full
/// });
///
/// // Consumer thread (blocking)
/// std::thread::spawn(move || {
///     let item = consumer.recv().unwrap();
/// });
/// ```
pub fn new_async_blocking_unbounded<T>(
    config: DynSpscConfig,
    signal: AsyncSignalGate,
) -> (AsyncUnboundedProducer<T>, BlockingUnboundedConsumer<T>) {
    let shared = Arc::new(AsyncShared::new());
    let signal_arc = Arc::new(signal);
    let (sender, receiver) =
        DynUnboundedSpsc::<T, Arc<AsyncSignalGate>>::new_with_signal(config, signal_arc);
    (
        AsyncUnboundedProducer::new(sender, Arc::clone(&shared)),
        BlockingUnboundedConsumer::new(receiver, shared),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::signal::{AsyncSignalWaker, Signal};

    fn test_config() -> DynSpscConfig {
        DynSpscConfig::new(6, 4) // 64 items/segment, 16 segments = 1023 capacity
    }

    fn test_signal() -> AsyncSignalGate {
        let waker = Arc::new(AsyncSignalWaker::new());
        let signal = Signal::with_index(0);
        AsyncSignalGate::new(0, signal, waker)
    }

    #[test]
    fn test_async_try_send_recv() {
        let (producer, consumer) = new::<u64>(test_config(), test_signal());

        producer.try_send(42).unwrap();
        producer.try_send(43).unwrap();

        assert_eq!(consumer.try_recv().unwrap(), 42);
        assert_eq!(consumer.try_recv().unwrap(), 43);
        assert!(matches!(consumer.try_recv(), Err(PopError::Empty)));
    }

    #[test]
    fn test_blocking_send_recv() {
        let (producer, consumer) = new_blocking::<u64>(test_config(), test_signal());

        producer.try_send(42).unwrap();
        producer.try_send(43).unwrap();

        assert_eq!(consumer.try_recv().unwrap(), 42);
        assert_eq!(consumer.try_recv().unwrap(), 43);
    }

    #[test]
    fn test_mixed_blocking_async() {
        let (producer, consumer) = new_blocking_async::<u64>(test_config(), test_signal());

        producer.try_send(42).unwrap();
        assert_eq!(consumer.try_recv().unwrap(), 42);
    }

    #[test]
    fn test_mixed_async_blocking() {
        let (producer, consumer) = new_async_blocking::<u64>(test_config(), test_signal());

        producer.try_send(42).unwrap();
        assert_eq!(consumer.try_recv().unwrap(), 42);
    }

    #[test]
    fn test_capacity() {
        let config = DynSpscConfig::new(6, 4); // 64 * 16 - 1 = 1023
        let (producer, consumer) = new::<u64>(config, test_signal());

        assert_eq!(producer.capacity(), 1023);
        assert_eq!(consumer.capacity(), 1023);
    }

    #[test]
    fn test_close_semantics() {
        let (producer, consumer) = new::<u64>(test_config(), test_signal());

        producer.try_send(1).unwrap();
        producer.try_send(2).unwrap();

        drop(producer);

        assert_eq!(consumer.try_recv().unwrap(), 1);
        assert_eq!(consumer.try_recv().unwrap(), 2);
        assert!(matches!(consumer.try_recv(), Err(PopError::Closed)));
    }

    #[test]
    fn test_different_configs() {
        // Small queue
        let small_config = DynSpscConfig::new(4, 2); // 16 * 4 - 1 = 63
        let (producer, _) = new::<u64>(small_config, test_signal());
        assert_eq!(producer.capacity(), 63);

        // Large queue
        let large_config = DynSpscConfig::new(10, 4); // 1024 * 16 - 1 = 16383
        let (producer, _) = new::<u64>(large_config, test_signal());
        assert_eq!(producer.capacity(), 16383);
    }

    #[test]
    fn test_fill_and_drain() {
        let config = DynSpscConfig::new(4, 2); // capacity = 63
        let (producer, consumer) = new_blocking::<u64>(config, test_signal());

        // Fill the queue
        for i in 0..63u64 {
            producer.try_send(i).unwrap();
        }

        // Should be full now
        assert!(matches!(producer.try_send(999), Err(PushError::Full(_))));

        // Drain and verify order
        for i in 0..63u64 {
            assert_eq!(consumer.try_recv().unwrap(), i);
        }

        // Should be empty now
        assert!(matches!(consumer.try_recv(), Err(PopError::Empty)));
    }
}
