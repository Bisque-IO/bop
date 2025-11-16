//! Async and Blocking MPSC (Multi-Producer, Single-Consumer) queue implementation.
//!
//! This module provides both async and blocking adapters over the lock-free MPSC queue.
//! Multiple producers can be mixed (some async, some blocking) with either an async or
//! blocking consumer.
//!
//! # Queue Variants
//!
//! - **Async**: [`AsyncMpscSender`] / [`AsyncMpscReceiver`] - For use with async tasks
//! - **Blocking**: [`BlockingMpscSender`] / [`BlockingMpscReceiver`] - For use with threads
//! - **Mixed**: You can mix any combination of async/blocking senders with async/blocking receiver!
//!
//! All variants share the same waker infrastructure, allowing seamless interoperability.

use super::signal::AsyncSignalGate;
use super::signal::AsyncSignalWaker;
use crate::spsc::Spsc;
use crate::spsc::UnboundedSpsc;
use crate::sync::signal::Signal;
use crate::utils::bits::find_nearest;
use crate::CachePadded;
use crate::{PopError, PushError};
use std::marker::PhantomData;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::{sink::Sink, stream::Stream};

use crate::future::waker::{DiatomicWaker, WaitUntil};
use crate::parking::{Parker, Unparker};

use rand::RngCore;

use std::ptr::{self, NonNull};

/// A waker implementation that unparks a thread.
///
/// Used to integrate blocking operations with the async waker infrastructure,
/// allowing async and blocking operations to work together seamlessly.
struct ThreadUnparker {
    unparker: Unparker,
}

impl std::task::Wake for ThreadUnparker {
    fn wake(self: Arc<Self>) {
        self.unparker.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.unparker.unpark();
    }
}

/// Create a new blocking MPSC queue
pub fn new<T, const P: usize, const NUM_SEGS_P2: usize>() -> Receiver<T, P, NUM_SEGS_P2> {
    new_with_waker(Arc::new(AsyncSignalWaker::new()))
}

/// Create a new blocking MPSC queue with a custom SignalWaker
///
/// This allows integration with external notification systems. The waker's
/// callback will be invoked when work becomes available in the queue.
///
/// # Arguments
///
/// * `waker` - Custom SignalWaker (typically with a callback to update worker status)
///
/// # Example
///
/// ```ignore
/// let worker_status = Arc::new(AtomicU64::new(0));
/// let status_clone = Arc::clone(&worker_status);
/// let waker = Arc::new(SignalWaker::new_with_callback(Some(Box::new(move || {
///     status_clone.fetch_or(WORKER_BIT_QUEUE, Ordering::Release);
/// }))));
/// let receiver = mpsc::new_with_waker(waker);
/// ```
pub fn new_with_waker<T, const P: usize, const NUM_SEGS_P2: usize>(
    waker: Arc<AsyncSignalWaker>,
) -> Receiver<T, P, NUM_SEGS_P2> {
    // Create sparse array of AtomicPtr<ProducerSlot>, all initialized to null
    let mut queues = Vec::with_capacity(MAX_QUEUES);
    for _ in 0..MAX_QUEUES {
        queues.push(AtomicPtr::new(core::ptr::null_mut()));
    }

    let signals: Arc<[Signal; SIGNAL_WORDS]> =
        Arc::new(std::array::from_fn(|i| Signal::with_index(i as u64)));

    let inner = Arc::new(Inner {
        queues: queues.into_boxed_slice(),
        queue_count: CachePadded::new(AtomicUsize::new(0)),
        producer_count: CachePadded::new(AtomicUsize::new(0)),
        max_producer_id: AtomicUsize::new(0),
        closed: CachePadded::new(AtomicBool::new(false)),
        summary: waker,
        signals,
    });

    Receiver {
        inner,
        misses: 0,
        seed: rand::rng().next_u64(),
    }
}

pub fn new_with_sender<T, const P: usize, const NUM_SEGS_P2: usize>()
-> (Sender<T, P, NUM_SEGS_P2>, Receiver<T, P, NUM_SEGS_P2>) {
    let waker = Arc::new(AsyncSignalWaker::new());
    // Create sparse array of AtomicPtr<ProducerSlot>, all initialized to null
    let mut queues = Vec::with_capacity(MAX_QUEUES);
    for _ in 0..MAX_QUEUES {
        queues.push(AtomicPtr::new(core::ptr::null_mut()));
    }

    let signals: Arc<[Signal; SIGNAL_WORDS]> =
        Arc::new(std::array::from_fn(|i| Signal::with_index(i as u64)));

    let inner = Arc::new(Inner {
        queues: queues.into_boxed_slice(),
        queue_count: CachePadded::new(AtomicUsize::new(0)),
        producer_count: CachePadded::new(AtomicUsize::new(0)),
        max_producer_id: AtomicUsize::new(0),
        closed: CachePadded::new(AtomicBool::new(false)),
        summary: waker,
        signals,
    });

    (
        inner
            .create_sender()
            .expect("fatal: mpsc won't allow even 1 sender"),
        Receiver {
            inner,
            misses: 0,
            seed: rand::rng().next_u64(),
        },
    )
}

const RND_MULTIPLIER: u64 = 0x5DEECE66D;
const RND_ADDEND: u64 = 0xB;
const RND_MASK: u64 = (1 << 48) - 1;

/// Maximum number of producers (limited by SignalWaker summary capacity)
const MAX_QUEUES: usize = 64 * 64;
const QUEUES_PER_PRODUCER: usize = 1;
const MAX_PRODUCERS: usize = MAX_QUEUES / QUEUES_PER_PRODUCER;
const MAX_PRODUCERS_MASK: usize = QUEUES_PER_PRODUCER - 1;

/// Number of u64 words needed for the signal bitset
const SIGNAL_WORDS: usize = 64;

/// Thread-local cache of SPSC queues for each producer thread
/// Stores (producer_id, queue_ptr, close_fn) where close_fn can close the queue
type CloseFn = Box<dyn FnOnce()>;

/// A producer queue slot containing both the queue and its associated space waker.
/// Allocated together for optimal cache locality - the waker is right next to the queue.
struct ProducerSlot<T, const P: usize, const NUM_SEGS_P2: usize> {
    queue: Spsc<T, P, NUM_SEGS_P2, AsyncSignalGate>,
    space_waker: DiatomicWaker,
}

/// The shared state of the blocking MPSC queue
///
/// # Memory Ordering Guarantees
///
/// - `queues`: Atomic pointers use Acquire/Release for slot installation and cleanup
/// - `queue_count`: Relaxed for statistics, not used for synchronization
/// - `producer_count`: Release on decrement (Sender::drop), Relaxed on read after Acquire fence
/// - `max_producer_id`: SeqCst for consistent global ordering across all threads
/// - `closed`: Acquire/Release for happens-before between close and other operations
/// - Sender::drop uses Release fence + Release store to ensure queued items are visible
/// - Receiver::try_pop uses Acquire fence before checking producer_count == 0
struct Inner<T, const P: usize, const NUM_SEGS_P2: usize> {
    /// Sparse array of producer slot pointers - always MAX_QUEUES size
    /// Each slot is allocated together with its queue and space waker for cache-friendly access
    queues: Box<[AtomicPtr<ProducerSlot<T, P, NUM_SEGS_P2>>]>,
    queue_count: CachePadded<AtomicUsize>,
    /// Number of registered producers
    producer_count: CachePadded<AtomicUsize>,
    max_producer_id: AtomicUsize,
    /// Closed flag
    closed: CachePadded<AtomicBool>,
    summary: Arc<AsyncSignalWaker>,
    /// Signal bitset - one bit per producer indicating which queues has data
    signals: Arc<[Signal; SIGNAL_WORDS]>,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Inner<T, P, NUM_SEGS_P2> {
    /// Check if the queue is closed
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Get the number of registered producers
    pub fn producer_count(&self) -> usize {
        self.producer_count.load(Ordering::Relaxed)
    }

    pub fn create_sender(self: &Arc<Self>) -> Result<Sender<T, P, NUM_SEGS_P2>, PushError<()>> {
        self.create_sender_with_config(0)
    }

    /// Create a new producer handle that bypasses all thread-local caching
    ///
    /// This creates a direct, high-performance handle to a specific producer queue.
    /// The handle provides push-only access without any thread-local overhead, making
    /// it ideal for scenarios where you need maximum performance and want to maintain
    /// explicit control over producer instances.
    ///
    /// Unlike `get_producer_queue()`, this method:
    /// - Does not register with thread-local storage
    /// - Does not use caching mechanisms
    /// - Provides a standalone handle that can be stored and reused
    /// - Offers maximum push performance
    ///
    /// # Returns
    ///
    /// Returns a `ProducerHandle` that can be used to push values, or `PushError::Closed`
    /// if the MPSC queue is closed.
    ///
    /// # Example
    ///
    /// ```ignoreignore
    /// let mpsc = MpscBlocking::<i32, 64>::new();
    ///
    /// // Create a direct producer handle
    /// let producer = mpsc.create_producer_handle().unwrap();
    ///
    /// // Use the handle for high-performance pushes
    /// producer.push(42).unwrap();
    /// producer.push_bulk(&[1, 2, 3]).unwrap();
    /// ```ignore
    pub fn create_sender_with_config(
        self: &Arc<Self>,
        max_pooled_segments: usize,
    ) -> Result<Sender<T, P, NUM_SEGS_P2>, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }

        loop {
            let current = self.producer_count.load(Ordering::Acquire);
            if current >= MAX_PRODUCERS {
                return Err(PushError::Full(()));
            }
            if self
                .producer_count
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }

        let mut assigned_id = None;
        let mut slot_arc: Option<Arc<ProducerSlot<T, P, NUM_SEGS_P2>>> = None;

        for signal_index in 0..SIGNAL_WORDS {
            for bit_index in 0..64 {
                let queue_index = signal_index * 64 + bit_index;
                if queue_index >= MAX_QUEUES {
                    break;
                }
                if !self.queues[queue_index].load(Ordering::Acquire).is_null() {
                    continue;
                }

                let queue = unsafe {
                    Spsc::<T, P, NUM_SEGS_P2, AsyncSignalGate>::new_unsafe_with_gate_and_config(
                        AsyncSignalGate::new(
                            bit_index as u8,
                            self.signals[signal_index].clone(),
                            Arc::clone(&self.summary),
                        ),
                        max_pooled_segments,
                    )
                };

                let slot = Arc::new(ProducerSlot {
                    queue,
                    space_waker: DiatomicWaker::new(),
                });

                let raw = Arc::into_raw(Arc::clone(&slot)) as *mut ProducerSlot<T, P, NUM_SEGS_P2>;
                match self.queues[queue_index].compare_exchange(
                    ptr::null_mut(),
                    raw,
                    Ordering::Release,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        self.queue_count.fetch_add(1, Ordering::Relaxed);
                        assigned_id = Some(queue_index);
                        slot_arc = Some(slot);
                        break;
                    }
                    Err(_) => unsafe {
                        Arc::from_raw(raw);
                    },
                }
            }
            if assigned_id.is_some() {
                break;
            }
        }

        let producer_id = match assigned_id {
            Some(id) => id,
            None => {
                self.producer_count.fetch_sub(1, Ordering::Release);
                return Err(PushError::Full(()));
            }
        };

        // Update max_producer_id if needed. We use SeqCst to ensure all threads
        // see a consistent view of the maximum producer ID, which is used by
        // the receiver to determine which slots to scan.
        loop {
            let max_producer_id = self.max_producer_id.load(Ordering::SeqCst);
            // If our ID is less than or equal to the current max, we're done
            if producer_id <= max_producer_id {
                break;
            }
            if self.is_closed() {
                return Err(PushError::Closed(()));
            }
            if self
                .max_producer_id
                .compare_exchange(
                    max_producer_id,
                    producer_id,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_ok()
            {
                break;
            }
        }

        // Arc reference counting: We created Arc::new (refcount=1), then cloned it
        // and converted the clone to raw via Arc::into_raw(Arc::clone(&slot)).
        // - Raw pointer stored in queues[producer_id] represents 1 Arc reference
        // - slot_arc variable holds the other Arc reference
        // - Total: 2 references, properly balanced for array + Sender ownership
        let slot_arc = slot_arc.expect("slot arc missing");

        Ok(Sender {
            inner: Arc::clone(&self),
            slot: slot_arc,
            producer_id,
        })
    }

    /// Close the queue
    ///
    /// After closing, no more items can be pushed. This method will block until all
    /// SPSC queues are empty. Wakes any waiting consumer.
    ///
    /// Note: This waits for consumer to drain all items from all producer queues.
    pub fn close(&self) -> bool {
        let was_open = !self.closed.swap(true, Ordering::AcqRel);
        if was_open {
            let permits = self.queue_count.load(Ordering::Relaxed).max(1);
            self.summary.release(permits);
        }

        for slot_atomic in self.queues.iter() {
            let slot_ptr = slot_atomic.swap(ptr::null_mut(), Ordering::AcqRel);
            if slot_ptr.is_null() {
                continue;
            }

            unsafe {
                (*slot_ptr).queue.close();
                Arc::from_raw(slot_ptr);
            }

            self.queue_count.fetch_sub(1, Ordering::Relaxed);
        }

        self.producer_count.store(0, Ordering::Release);
        true
    }
}

/// Blocking MPMC Queue - Multi-Producer Single-Consumer with blocking operations
///
/// # Type Parameters
/// - `T`: The type of elements stored in the queue (must be Copy)
/// - `P`: log2 of segment size (default 8 = 256 items/segment)
/// - `NUM_SEGS_P2`: log2 of number of segments (default 2 = 4 segments, total capacity ~1024)
///
/// # Examples
///
/// ```ignore
/// use bop_mpmc::mpsc_blocking::MpscBlocking;
/// use std::thread;
/// use std::time::Duration;
///
/// let mpsc = MpscBlocking::<i32>::new();
///
/// // Producer thread
/// let mpsc_producer = mpsc.clone();
/// thread::spawn(move || {
///     thread::sleep(Duration::from_millis(100));
///     mpsc_producer.push(42).unwrap();
/// });
///
/// // Consumer blocks until item is available
/// let value = mpsc.pop_blocking().unwrap();
/// assert_eq!(value, 42);
/// ```ignore
pub struct Sender<T, const P: usize, const NUM_SEGS_P2: usize> {
    inner: Arc<Inner<T, P, NUM_SEGS_P2>>,
    slot: Arc<ProducerSlot<T, P, NUM_SEGS_P2>>,
    producer_id: usize,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Sender<T, P, NUM_SEGS_P2> {
    /// Check if the queue is closed
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    /// Get the number of registered producers
    pub fn producer_count(&self) -> usize {
        self.inner.producer_count.load(Ordering::Relaxed)
    }

    /// Get the producer ID for this handle
    pub fn producer_id(&self) -> usize {
        self.producer_id
    }

    /// Push a value onto the queue
    ///
    /// This will use the thread-local SPSC queue for this producer.
    /// If successful, notifies any waiting consumer.
    pub fn try_push(&mut self, value: T) -> Result<(), PushError<T>> {
        self.slot.queue.try_push(value)
    }

    /// Push a value onto the queue (spins if full)
    ///
    /// This will use the thread-local SPSC queue for this producer.
    /// If successful, notifies any waiting consumer.
    pub fn push_spin(&mut self, mut value: T) -> Result<(), PushError<T>> {
        loop {
            match self.slot.queue.try_push(value) {
                Ok(()) => return Ok(()),
                Err(PushError::Full(returned)) => {
                    value = returned;
                    std::hint::spin_loop();
                }
                Err(err @ PushError::Closed(_)) => return Err(err),
            }
        }
    }

    /// Push multiple values in bulk
    pub fn try_push_n(&mut self, values: &[T]) -> Result<usize, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }
        self.slot.queue.try_push_n(values)
    }

    /// Push a value onto the queue
    ///
    /// This will use the thread-local SPSC queue for this producer.
    /// If successful, notifies any waiting consumer.
    pub unsafe fn unsafe_try_push(&self, value: T) -> Result<(), PushError<T>> {
        self.slot.queue.try_push(value)
    }

    /// Push multiple values in bulk
    pub unsafe fn unsafe_try_push_n(&self, values: &[T]) -> Result<usize, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }
        self.slot.queue.try_push_n(values)
    }

    /// Close the queue
    ///
    /// After closing, no more items can be pushed. This method will block until all
    /// SPSC queues are empty. Wakes any waiting consumer.
    ///
    /// Note: This waits for consumer to drain all items from all producer queues.
    pub fn close(&mut self) -> bool {
        self.inner.close()
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Clone for Sender<T, P, NUM_SEGS_P2> {
    fn clone(&self) -> Self {
        self.inner.create_sender().expect("too many senders")
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Drop for Sender<T, P, NUM_SEGS_P2> {
    fn drop(&mut self) {
        unsafe {
            self.slot.queue.close();
        }

        // Release fence to ensure all writes to the queue are visible before we
        // decrement producer_count. This synchronizes with the Acquire fence in
        // Receiver::try_pop_with_waker(), ensuring the receiver sees any items
        // pushed before this sender was dropped.
        std::sync::atomic::fence(Ordering::Release);

        // Decrement the active producer count. We use Release ordering to publish
        // the fence above and all prior writes. We intentionally keep the producer
        // slot registered until the consumer observes the closed queue and cleans it up.
        self.inner.producer_count.fetch_sub(1, Ordering::Release);
    }
}

pub struct Receiver<T, const P: usize, const NUM_SEGS_P2: usize> {
    inner: Arc<Inner<T, P, NUM_SEGS_P2>>,
    misses: u64,
    seed: u64,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Receiver<T, P, NUM_SEGS_P2> {
    pub fn next(&mut self) -> u64 {
        let old_seed = self.seed;
        let next_seed = (old_seed
            .wrapping_mul(RND_MULTIPLIER)
            .wrapping_add(RND_ADDEND))
            & RND_MASK;
        self.seed = next_seed;
        next_seed >> 16
    }

    /// Get a reference to the inner shared state
    pub(crate) fn inner(&self) -> &Arc<Inner<T, P, NUM_SEGS_P2>> {
        &self.inner
    }

    /// Check if the queue is closed
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    /// Close the queue
    ///
    /// After closing, no more items can be pushed. This method will block until all
    /// SPSC queues are empty. Wakes any waiting consumer.
    ///
    /// Note: This waits for consumer to drain all items from all producer queues.
    pub fn close(&self) -> bool {
        self.inner.close()
    }

    /// Get the number of registered producers
    pub fn producer_count(&self) -> usize {
        self.inner.producer_count.load(Ordering::Relaxed)
    }

    pub fn create_sender(&self) -> Result<Sender<T, P, NUM_SEGS_P2>, PushError<()>> {
        self.create_sender_with_config(0)
    }

    /// Create a new producer handle that bypasses all thread-local caching
    ///
    /// This creates a direct, high-performance handle to a specific producer queue.
    /// The handle provides push-only access without any thread-local overhead, making
    /// it ideal for scenarios where you need maximum performance and want to maintain
    /// explicit control over producer instances.
    ///
    /// Unlike `get_producer_queue()`, this method:
    /// - Does not register with thread-local storage
    /// - Does not use caching mechanisms
    /// - Provides a standalone handle that can be stored and reused
    /// - Offers maximum push performance
    ///
    /// # Returns
    ///
    /// Returns a `ProducerHandle` that can be used to push values, or `PushError::Closed`
    /// if the MPSC queue is closed.
    ///
    /// # Example
    ///
    /// ```ignoreignore
    /// let mpsc = MpscBlocking::<i32, 64>::new();
    ///
    /// // Create a direct producer handle
    /// let producer = mpsc.create_producer_handle().unwrap();
    ///
    /// // Use the handle for high-performance pushes
    /// producer.push(42).unwrap();
    /// producer.push_bulk(&[1, 2, 3]).unwrap();
    /// ```ignore
    pub fn create_sender_with_config(
        &self,
        max_pooled_segments: usize,
    ) -> Result<Sender<T, P, NUM_SEGS_P2>, PushError<()>> {
        self.inner.create_sender_with_config(max_pooled_segments)
    }

    pub fn try_pop_n(&mut self, batch: &mut [T]) -> usize {
        let (count, waker_opt) = self.try_pop_n_with_slot(batch);
        if let Some(waker) = waker_opt {
            waker.notify();
        }
        count
    }

    /// Attempts to pop a single value and returns the item along with the producer id.
    /// Try to pop a single value, also returning the space waker to notify
    pub fn try_pop_with_waker(&mut self) -> Result<(T, &DiatomicWaker), PopError> {
        // Clone Arc to get a separate reference before the mutable borrow
        let inner = Arc::clone(&self.inner);
        let mut slot = MaybeUninit::<T>::uninit();
        let slice = unsafe { std::slice::from_raw_parts_mut(slot.as_mut_ptr(), 1) };
        let (drained, waker_opt) = self.try_pop_n_with_slot(slice);
        if drained == 0 {
            // Acquire fence BEFORE checking state to ensure we see all writes from producers.
            // This synchronizes with the Release fence in Sender::drop() and ensures we see
            // any items pushed before the last producer dropped.
            std::sync::atomic::fence(Ordering::Acquire);
            
            // Re-check state after attempting to pop (producer_count may have changed)
            let is_closed = inner.is_closed();
            // Use Relaxed here since the fence above provides the necessary synchronization
            let producer_count = inner.producer_count.load(Ordering::Relaxed);

            // Queue is closed if explicitly closed OR no producers remain (all senders dropped)
            if is_closed || producer_count == 0 {
                Err(PopError::Closed)
            } else {
                Err(PopError::Empty)
            }
        } else {
            let value = unsafe { slot.assume_init() };
            let waker = waker_opt.expect("waker missing for drained item");
            Ok((value, waker))
        }
    }

    /// Try to pop a single value (without returning the waker)
    pub fn try_pop(&mut self) -> Result<T, PopError> {
        self.try_pop_with_waker().map(|(v, _)| v)
    }

    fn try_pop_n_with_slot(&mut self, batch: &mut [T]) -> (usize, Option<&DiatomicWaker>) {
        for _ in 0..64 {
            match self.acquire() {
                Some((producer_id, slot_ptr)) => {
                    let slot = unsafe { &*slot_ptr };
                    match slot.queue.try_pop_n(batch) {
                        Ok(size) => {
                            slot.queue.unmark_and_schedule();
                            return (size, Some(&slot.space_waker));
                        }
                        Err(PopError::Closed) => {
                            slot.queue.unmark();
                            self.cleanup_closed_slot(producer_id, slot_ptr);
                        }
                        Err(_) => {
                            slot.queue.unmark();
                            self.misses += 1;
                        }
                    };
                }
                None => {
                    // No available producer this round
                }
            }
        }

        (0, None)
    }

    fn cleanup_closed_slot(
        &self,
        producer_id: usize,
        slot_ptr: *mut ProducerSlot<T, P, NUM_SEGS_P2>,
    ) {
        let slot_atomic = &self.inner.queues[producer_id];
        let current = slot_atomic.load(Ordering::Acquire);
        if current != slot_ptr {
            return;
        }

        if slot_atomic
            .compare_exchange(
                slot_ptr,
                ptr::null_mut(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            self.inner.queue_count.fetch_sub(1, Ordering::Relaxed);
            unsafe {
                Arc::from_raw(slot_ptr);
            }
        }
    }

    fn acquire(&mut self) -> Option<(usize, *mut ProducerSlot<T, P, NUM_SEGS_P2>)> {
        let random = self.next() as usize;
        // Try selecting signal index from summary hint
        let random_word = random % SIGNAL_WORDS;
        let mut signal_index = self.inner.summary.summary_select(random_word as u64) as usize;

        if signal_index >= SIGNAL_WORDS {
            signal_index = random_word;
        }

        let mut signal_bit = self.next() & 63;
        let signal = &self.inner.signals[signal_index];
        let signal_value = signal.load(Ordering::Acquire);

        // Find nearest set bit for fairness
        signal_bit = find_nearest(signal_value, signal_bit);

        // 64 and over is out of bounds
        if signal_bit >= 64 {
            self.misses += 1;
            return None;
        }

        // Atomically acquire the bit
        let (bit, expected, acquired) = signal.try_acquire(signal_bit);

        if !acquired {
            // Contention - try next
            // self.contention += 1;
            std::hint::spin_loop();
            return None;
        }

        // Is the signal empty?
        let empty = expected == bit;

        if empty {
            self.inner
                .summary
                .try_unmark_if_empty(signal.index(), signal.value());
        }

        // Compute producer id
        let producer_id = signal_index * 64 + (signal_bit as usize);

        // Load the slot pointer atomically
        let slot_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
        if slot_ptr.is_null() {
            self.misses += 1;
            if empty {
                self.inner
                    .summary
                    .try_unmark_if_empty(signal.index(), signal.value());
            }
            return None;
        }

        // SAFETY: The pointer is valid and we have exclusive consumer access
        let slot = unsafe { &*slot_ptr };

        // Mark as EXECUTING
        slot.queue.mark();

        Some((producer_id, slot_ptr))
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Drop for Receiver<T, P, NUM_SEGS_P2> {
    fn drop(&mut self) {
        self.inner.close();
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Drop for Inner<T, P, NUM_SEGS_P2> {
    fn drop(&mut self) {
        self.close();
    }
}

struct SenderPtr<'a, T, const P: usize, const NUM_SEGS_P2: usize> {
    ptr: NonNull<Sender<T, P, NUM_SEGS_P2>>,
    _marker: PhantomData<&'a ()>,
}

impl<'a, T, const P: usize, const NUM_SEGS_P2: usize> Copy for SenderPtr<'a, T, P, NUM_SEGS_P2> {}

impl<'a, T, const P: usize, const NUM_SEGS_P2: usize> Clone for SenderPtr<'a, T, P, NUM_SEGS_P2> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T: 'a, const P: usize, const NUM_SEGS_P2: usize> SenderPtr<'a, T, P, NUM_SEGS_P2> {
    #[inline]
    unsafe fn space_waker(self) -> &'a DiatomicWaker {
        let sender = self.ptr.as_ptr();
        let sender_ref: &Sender<T, P, NUM_SEGS_P2> = unsafe { &*sender };
        &sender_ref.slot.space_waker
    }

    #[inline]
    unsafe fn with_mut<R>(self, f: impl FnOnce(&mut Sender<T, P, NUM_SEGS_P2>) -> R) -> R {
        let sender = self.ptr.as_ptr();
        let sender_mut: &mut Sender<T, P, NUM_SEGS_P2> = unsafe { &mut *sender };
        f(sender_mut)
    }
}

unsafe impl<T: Send, const P: usize, const NUM_SEGS_P2: usize> Send
    for SenderPtr<'_, T, P, NUM_SEGS_P2>
{
}
unsafe impl<T: Send, const P: usize, const NUM_SEGS_P2: usize> Sync
    for SenderPtr<'_, T, P, NUM_SEGS_P2>
{
}

/// Shared state for async MPSC coordination.
///
/// Provides receiver-side waker management. Producer-side space wakers are stored
/// per-producer in `Inner::space_wakers` for targeted notification (no spurious wakeups).
struct AsyncMpscShared<T, const P: usize, const NUM_SEGS_P2: usize> {
    receiver_waiter: CachePadded<DiatomicWaker>,
    inner: Arc<Inner<T, P, NUM_SEGS_P2>>,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> AsyncMpscShared<T, P, NUM_SEGS_P2> {
    fn new(inner: Arc<Inner<T, P, NUM_SEGS_P2>>) -> Self {
        Self {
            receiver_waiter: CachePadded::new(DiatomicWaker::new()),
            inner,
        }
    }

    #[inline]
    fn notify_receiver(&self) {
        self.receiver_waiter.notify();
    }

    #[inline]
    unsafe fn wait_for_items<Pred, R>(&self, predicate: Pred) -> WaitUntil<'_, Pred, R>
    where
        Pred: FnMut() -> Option<R>,
    {
        unsafe { self.receiver_waiter.wait_until(predicate) }
    }

    #[inline]
    unsafe fn register_receiver(&self, waker: &Waker) {
        unsafe { self.receiver_waiter.register(waker) };
    }
}

pub struct AsyncMpscSender<T, const P: usize, const NUM_SEGS_P2: usize> {
    sender: Sender<T, P, NUM_SEGS_P2>,
    shared: Arc<AsyncMpscShared<T, P, NUM_SEGS_P2>>,
}

pub struct AsyncMpscReceiver<T, const P: usize, const NUM_SEGS_P2: usize> {
    receiver: Receiver<T, P, NUM_SEGS_P2>,
    shared: Arc<AsyncMpscShared<T, P, NUM_SEGS_P2>>,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> AsyncMpscSender<T, P, NUM_SEGS_P2> {
    fn new(
        sender: Sender<T, P, NUM_SEGS_P2>,
        shared: Arc<AsyncMpscShared<T, P, NUM_SEGS_P2>>,
    ) -> Self {
        // space_waker is already allocated in the ProducerSlot by create_sender
        Self { sender, shared }
    }

    /// Attempts to enqueue without blocking.
    #[inline]
    pub fn try_send(&mut self, value: T) -> Result<(), PushError<T>> {
        match self.sender.try_push(value) {
            Ok(()) => {
                self.shared.notify_receiver();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    pub async fn send(&mut self, value: T) -> Result<(), PushError<T>> {
        match self.try_send(value) {
            Ok(()) => Ok(()),
            Err(PushError::Full(item)) => {
                let mut pending = Some(item);
                let shared = Arc::clone(&self.shared);
                let sender_ptr = SenderPtr {
                    ptr: NonNull::from(&mut self.sender),
                    _marker: PhantomData,
                };
                let waker_ptr = sender_ptr;
                let wait = unsafe {
                    waker_ptr.space_waker().wait_until(move || {
                        let candidate = pending.take()?;
                        sender_ptr.with_mut(|sender| match sender.try_push(candidate) {
                            Ok(()) => {
                                shared.notify_receiver();
                                Some(Ok(()))
                            }
                            Err(PushError::Full(candidate)) => {
                                pending = Some(candidate);
                                None
                            }
                            Err(PushError::Closed(candidate)) => {
                                Some(Err(PushError::Closed(candidate)))
                            }
                        })
                    })
                };
                wait.await
            }
            Err(PushError::Closed(item)) => Err(PushError::Closed(item)),
        }
    }

    pub async fn send_slice(&mut self, values: Vec<T>) -> Result<Vec<T>, PushError<Vec<T>>> {
        let mut values = ManuallyDrop::new(values);
        let data_ptr = values.as_mut_ptr() as usize;
        let len = values.len();
        let cap = values.capacity();
        std::mem::forget(values);

        if len == 0 {
            let empty = unsafe { Vec::from_raw_parts(data_ptr as *mut T, 0, cap) };
            return Ok(empty);
        }

        let slice_from = |sent: usize| unsafe {
            std::slice::from_raw_parts((data_ptr as *const T).add(sent), len - sent)
        };
        let finish_empty = || unsafe { Vec::from_raw_parts(data_ptr as *mut T, 0, cap) };
        let finish_remaining = |sent: usize| unsafe {
            let remaining = len - sent;
            if remaining > 0 {
                ptr::copy(
                    (data_ptr as *mut T).add(sent),
                    data_ptr as *mut T,
                    remaining,
                );
            }
            Vec::from_raw_parts(data_ptr as *mut T, remaining, cap)
        };

        let mut sent = 0usize;

        match self.sender.try_push_n(slice_from(sent)) {
            Ok(written) => {
                if written > 0 {
                    self.shared.notify_receiver();
                    sent += written;
                    if sent == len {
                        return Ok(finish_empty());
                    }
                }
            }
            Err(PushError::Closed(())) => {
                let remaining_vec = finish_remaining(sent);
                return Err(PushError::Closed(remaining_vec));
            }
            Err(PushError::Full(())) => {}
        }

        let shared = Arc::clone(&self.shared);
        let sender_ptr = SenderPtr {
            ptr: NonNull::from(&mut self.sender),
            _marker: PhantomData,
        };
        let waker_ptr = sender_ptr;
        let wait = unsafe {
            waker_ptr.space_waker().wait_until(move || {
                if sent == len {
                    return Some(Ok(finish_empty()));
                }
                sender_ptr.with_mut(|sender| match sender.try_push_n(slice_from(sent)) {
                    Ok(written) => {
                        if written > 0 {
                            shared.notify_receiver();
                            sent += written;
                            if sent == len {
                                return Some(Ok(finish_empty()));
                            }
                        }
                        None
                    }
                    Err(PushError::Full(())) => None,
                    Err(PushError::Closed(())) => {
                        let remaining_vec = finish_remaining(sent);
                        Some(Err(PushError::Closed(remaining_vec)))
                    }
                })
            })
        };
        wait.await
    }

    pub async fn send_batch<I>(&mut self, iter: I) -> Result<(), PushError<T>>
    where
        I: IntoIterator<Item = T>,
    {
        for item in iter {
            self.send(item).await?;
        }
        Ok(())
    }

    pub fn close(&mut self) -> bool {
        self.sender.close()
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Clone for AsyncMpscSender<T, P, NUM_SEGS_P2> {
    fn clone(&self) -> Self {
        let sender = self.sender.clone();
        Self::new(sender, Arc::clone(&self.shared))
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Drop for AsyncMpscSender<T, P, NUM_SEGS_P2> {
    fn drop(&mut self) {
        self.shared.notify_receiver();
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Sink<T> for AsyncMpscSender<T, P, NUM_SEGS_P2> {
    type Error = PushError<T>;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), PushError<T>>> {
        let this = unsafe { self.get_unchecked_mut() };

        // Check if there's space available (not full)
        if !this.sender.slot.queue.is_full() {
            return Poll::Ready(Ok(()));
        }

        // No space available, register waker and return pending
        unsafe {
            this.sender.slot.space_waker.register(cx.waker());
        }

        // Double-check after registering (prevent missed wakeup)
        if !this.sender.slot.queue.is_full() {
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

impl<T, const P: usize, const NUM_SEGS_P2: usize> AsyncMpscReceiver<T, P, NUM_SEGS_P2> {
    fn new(
        receiver: Receiver<T, P, NUM_SEGS_P2>,
        shared: Arc<AsyncMpscShared<T, P, NUM_SEGS_P2>>,
    ) -> Self {
        Self { receiver, shared }
    }

    #[inline]
    pub fn try_recv(&mut self) -> Result<T, PopError> {
        match self.receiver.try_pop_with_waker() {
            Ok((value, waker)) => {
                waker.notify();
                Ok(value)
            }
            Err(err) => Err(err),
        }
    }

    pub async fn recv(&mut self) -> Result<T, PopError> {
        match self.try_recv() {
            Ok(value) => Ok(value),
            Err(PopError::Empty) | Err(PopError::Timeout) => {
                let shared = Arc::clone(&self.shared);
                let receiver = &mut self.receiver;
                unsafe {
                    shared
                        .wait_for_items(|| match receiver.try_pop_with_waker() {
                            Ok((value, waker)) => {
                                waker.notify();
                                Some(Ok(value))
                            }
                            Err(PopError::Empty) | Err(PopError::Timeout) => None,
                            Err(PopError::Closed) => Some(Err(PopError::Closed)),
                        })
                        .await
                }
            }
            Err(PopError::Closed) => Err(PopError::Closed),
        }
    }

    /// Asynchronously receives multiple items into a destination slice.
    ///
    /// This method fills the provided slice with items from the queue, blocking
    /// until at least one item is available. It returns as soon as any items are
    /// received (partial fill) or when the queue is closed.
    ///
    /// # Advantages over `recv()` in a loop
    ///
    /// - **Bulk operations**: More efficient than calling `recv()` repeatedly
    /// - **Partial results**: Returns immediately with whatever is available
    /// - **Zero allocation**: Slice reference lives in Future stack frame
    /// - **Automatic backpressure**: Notifies producers as space becomes available
    ///
    /// # Returns
    ///
    /// - `Ok(count)`: Number of items written to `dst` (1..=dst.len())
    /// - `Err(PopError::Closed)`: Queue closed with no items available
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut buffer = [0u32; 128];
    /// match receiver.recv_batch(&mut buffer).await {
    ///     Ok(count) => {
    ///         // Process buffer[..count]
    ///     }
    ///     Err(PopError::Closed) => {
    ///         // Queue closed
    ///     }
    /// }
    /// ```
    pub async fn recv_batch(&mut self, dst: &mut [T]) -> Result<usize, PopError> {
        if dst.is_empty() {
            return Ok(0);
        }

        let receiver = &mut self.receiver;
        let shared = &self.shared;

        // Fast path: try immediate receive
        // try_pop_n already notifies the waker internally
        let mut filled = receiver.try_pop_n(dst);
        if filled > 0 {
            return Ok(filled);
        }

        // Queue is closed if explicitly closed OR no producers remain (all senders dropped)
        if receiver.is_closed() || receiver.producer_count() == 0 {
            return Err(PopError::Closed);
        }

        // Slow path: need to block
        unsafe {
            shared
                .wait_for_items(|| {
                    if filled == dst.len() {
                        return Some(Ok(filled));
                    }

                    let count = receiver.try_pop_n(&mut dst[filled..]);
                    if count == 0 {
                        // Queue is closed if explicitly closed OR no producers remain
                        if receiver.is_closed() || receiver.producer_count() == 0 {
                            Some(if filled > 0 {
                                Ok(filled)
                            } else {
                                Err(PopError::Closed)
                            })
                        } else {
                            None
                        }
                    } else {
                        filled += count;
                        // try_pop_n already notifies the waker internally
                        if filled == dst.len() {
                            Some(Ok(filled))
                        } else {
                            None
                        }
                    }
                })
                .await
        }
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Stream for AsyncMpscReceiver<T, P, NUM_SEGS_P2> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        match this.try_recv() {
            Ok(value) => Poll::Ready(Some(value)),
            Err(PopError::Closed) => Poll::Ready(None),
            Err(PopError::Empty) | Err(PopError::Timeout) => {
                unsafe {
                    this.shared.register_receiver(cx.waker());
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

/// Blocking MPSC sender.
///
/// Multiple blocking senders can be created and used concurrently. They share the same
/// waker infrastructure as async senders, allowing perfect interoperability.
pub struct BlockingMpscSender<T, const P: usize, const NUM_SEGS_P2: usize> {
    sender: Sender<T, P, NUM_SEGS_P2>,
    shared: Arc<AsyncMpscShared<T, P, NUM_SEGS_P2>>,
    parker: Parker,
    parker_waker: Arc<ThreadUnparker>,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> BlockingMpscSender<T, P, NUM_SEGS_P2> {
    fn new(
        sender: Sender<T, P, NUM_SEGS_P2>,
        shared: Arc<AsyncMpscShared<T, P, NUM_SEGS_P2>>,
    ) -> Self {
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

    /// Register a waker for space availability notifications
    #[inline]
    unsafe fn register_space_waker(&self, waker: &Waker) {
        unsafe { self.sender.slot.space_waker.register(waker) };
    }

    #[inline]
    pub fn try_send(&mut self, value: T) -> Result<(), PushError<T>> {
        match self.sender.try_push(value) {
            Ok(()) => {
                self.shared.notify_receiver();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Blocking send that parks the thread until space is available.
    pub fn send(&mut self, mut value: T) -> Result<(), PushError<T>> {
        let sender_ptr = &mut self.sender as *mut Sender<T, P, NUM_SEGS_P2>;

        // Fast path: try immediate send
        match unsafe { (&mut *sender_ptr).try_push(value) } {
            Ok(()) => return Ok(()),
            Err(PushError::Closed(item)) => return Err(PushError::Closed(item)),
            Err(PushError::Full(item)) => value = item,
        }

        // Slow path: need to block
        let waker = Waker::from(Arc::clone(&self.parker_waker));

        loop {
            unsafe {
                self.register_space_waker(&waker);
            }

            match unsafe { (&mut *sender_ptr).try_push(value) } {
                Ok(()) => {
                    self.shared.notify_receiver();
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

    /// Sends multiple items from a slice, blocking until all are sent.
    pub fn send_slice(&mut self, values: Vec<T>) -> Result<Vec<T>, PushError<Vec<T>>> {
        let mut values = ManuallyDrop::new(values);
        let data_ptr = values.as_mut_ptr();
        let len = values.len();
        let cap = values.capacity();

        if len == 0 {
            let empty = unsafe { Vec::from_raw_parts(data_ptr, 0, cap) };
            return Ok(empty);
        }

        let slice_from =
            |sent: usize| unsafe { std::slice::from_raw_parts(data_ptr.add(sent), len - sent) };
        let finish_empty = || unsafe { Vec::from_raw_parts(data_ptr, 0, cap) };
        let finish_remaining = |sent: usize| unsafe {
            let remaining = len - sent;
            if remaining > 0 {
                ptr::copy(data_ptr.add(sent), data_ptr, remaining);
            }
            Vec::from_raw_parts(data_ptr, remaining, cap)
        };

        let waker = Waker::from(Arc::clone(&self.parker_waker));
        let sender_ptr = SenderPtr {
            ptr: NonNull::from(&mut self.sender),
            _marker: PhantomData,
        };
        let mut sent = 0usize;

        match self.sender.try_push_n(slice_from(sent)) {
            Ok(written) => {
                if written > 0 {
                    self.shared.notify_receiver();
                    sent += written;
                    if sent == len {
                        return Ok(finish_empty());
                    }
                }
            }
            Err(PushError::Closed(())) => {
                let remaining_vec = finish_remaining(sent);
                return Err(PushError::Closed(remaining_vec));
            }
            Err(PushError::Full(())) => {}
        }

        loop {
            unsafe {
                self.register_space_waker(&waker);
            }

            if sent == len {
                return Ok(finish_empty());
            }

            match unsafe { sender_ptr.with_mut(|sender| sender.try_push_n(slice_from(sent))) } {
                Ok(written) => {
                    if written > 0 {
                        self.shared.notify_receiver();
                        sent += written;
                        if sent == len {
                            return Ok(finish_empty());
                        }
                    }
                    self.parker.park();
                }
                Err(PushError::Full(())) => {
                    self.parker.park();
                }
                Err(PushError::Closed(())) => {
                    let remaining_vec = finish_remaining(sent);
                    return Err(PushError::Closed(remaining_vec));
                }
            }
        }
    }

    pub fn close(&mut self) -> bool {
        self.sender.close()
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Clone for BlockingMpscSender<T, P, NUM_SEGS_P2> {
    fn clone(&self) -> Self {
        let sender = self.sender.clone();
        Self::new(sender, Arc::clone(&self.shared))
    }
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> Drop for BlockingMpscSender<T, P, NUM_SEGS_P2> {
    fn drop(&mut self) {
        self.shared.notify_receiver();
    }
}

/// Blocking MPSC receiver.
///
/// The blocking receiver can work with both async and blocking senders.
pub struct BlockingMpscReceiver<T, const P: usize, const NUM_SEGS_P2: usize> {
    receiver: Receiver<T, P, NUM_SEGS_P2>,
    shared: Arc<AsyncMpscShared<T, P, NUM_SEGS_P2>>,
}

impl<T, const P: usize, const NUM_SEGS_P2: usize> BlockingMpscReceiver<T, P, NUM_SEGS_P2> {
    fn new(
        receiver: Receiver<T, P, NUM_SEGS_P2>,
        shared: Arc<AsyncMpscShared<T, P, NUM_SEGS_P2>>,
    ) -> Self {
        Self { receiver, shared }
    }

    #[inline]
    pub fn try_recv(&mut self) -> Result<T, PopError> {
        match self.receiver.try_pop_with_waker() {
            Ok((value, waker)) => {
                waker.notify();
                Ok(value)
            }
            Err(err) => Err(err),
        }
    }

    /// Blocking receive that parks the thread until an item is available.
    pub fn recv(&mut self) -> Result<T, PopError> {
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
                self.shared.register_receiver(&waker);
            }

            // Double-check after registering (prevent missed wakeup)
            match self.receiver.try_pop_with_waker() {
                Ok((value, waker)) => {
                    waker.notify();
                    return Ok(value);
                }
                Err(PopError::Closed) => {
                    return Err(PopError::Closed);
                }
                Err(PopError::Empty) | Err(PopError::Timeout) => {
                    parker.park();
                }
            }
        }
    }

    /// Receives multiple items into a destination slice, blocking until at least one arrives.
    ///
    /// This method fills the provided slice with items from the queue, blocking the thread
    /// until at least one item is available. It returns as soon as any items are received
    /// (partial fill) or when the queue is closed.
    ///
    /// # Advantages over `recv()` in a loop
    ///
    /// - **Bulk operations**: More efficient than calling `recv()` repeatedly
    /// - **Partial results**: Returns immediately with whatever is available
    /// - **Reduced overhead**: Fewer waker registrations and context switches
    /// - **Automatic backpressure**: Notifies producers as space becomes available
    ///
    /// # Returns
    ///
    /// - `Ok(count)`: Number of items written to `dst` (1..=dst.len())
    /// - `Err(PopError::Closed)`: Queue closed with no items available
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut buffer = [0u32; 128];
    /// match receiver.recv_batch(&mut buffer) {
    ///     Ok(count) => {
    ///         // Process buffer[..count]
    ///     }
    ///     Err(PopError::Closed) => {
    ///         // Queue closed
    ///     }
    /// }
    /// ```
    pub fn recv_batch(&mut self, dst: &mut [T]) -> Result<usize, PopError> {
        if dst.is_empty() {
            return Ok(0);
        }

        // Fast path: try immediate receive with per-producer notification
        let mut filled = 0;
        while filled < dst.len() {
            match self.receiver.try_pop_with_waker() {
                Ok((value, waker)) => {
                    dst[filled] = value;
                    filled += 1;
                    waker.notify();
                }
                Err(_) => break,
            }
        }

        if filled > 0 {
            return Ok(filled);
        }

        // Queue is closed if explicitly closed OR no producers remain (all senders dropped)
        if self.receiver.is_closed() || self.receiver.producer_count() == 0 {
            return Err(PopError::Closed);
        }

        // Slow path: need to block
        let parker = Parker::new();
        let unparker = parker.unparker();
        let waker = Waker::from(Arc::new(ThreadUnparker { unparker }));

        loop {
            // Register our waker
            unsafe {
                self.shared.register_receiver(&waker);
            }

            // Double-check and try to make progress
            while filled < dst.len() {
                match self.receiver.try_pop_with_waker() {
                    Ok((value, waker)) => {
                        dst[filled] = value;
                        filled += 1;
                        waker.notify();
                    }
                    Err(_) => break,
                }
            }

            if filled > 0 {
                // Queue is closed if explicitly closed OR no producers remain
                if filled == dst.len()
                    || self.receiver.is_closed()
                    || self.receiver.producer_count() == 0
                {
                    return Ok(filled);
                }
                // Got some but not all, park and try again
                parker.park();
            } else if self.receiver.is_closed() || self.receiver.producer_count() == 0 {
                return Err(PopError::Closed);
            } else {
                // No items, park until woken
                parker.park();
            }
        }
    }
}

pub fn async_mpsc<T, const P: usize, const NUM_SEGS_P2: usize>() -> (
    AsyncMpscSender<T, P, NUM_SEGS_P2>,
    AsyncMpscReceiver<T, P, NUM_SEGS_P2>,
) {
    let (sender, receiver) = new_with_sender();
    async_mpsc_from_parts(sender, receiver)
}

pub fn async_mpsc_from_parts<T, const P: usize, const NUM_SEGS_P2: usize>(
    sender: Sender<T, P, NUM_SEGS_P2>,
    receiver: Receiver<T, P, NUM_SEGS_P2>,
) -> (
    AsyncMpscSender<T, P, NUM_SEGS_P2>,
    AsyncMpscReceiver<T, P, NUM_SEGS_P2>,
) {
    let inner = Arc::clone(receiver.inner());
    let shared = Arc::new(AsyncMpscShared::new(inner));
    let async_sender = AsyncMpscSender::new(sender, Arc::clone(&shared));
    let async_receiver = AsyncMpscReceiver::new(receiver, shared);
    (async_sender, async_receiver)
}

/// Creates a default blocking MPSC queue.
///
/// Both senders and receiver use blocking operations that park threads.
pub fn blocking_mpsc<T, const P: usize, const NUM_SEGS_P2: usize>() -> (
    BlockingMpscSender<T, P, NUM_SEGS_P2>,
    BlockingMpscReceiver<T, P, NUM_SEGS_P2>,
) {
    let (sender, receiver) = new_with_sender();
    blocking_mpsc_from_parts(sender, receiver)
}

/// Creates a blocking MPSC queue from existing parts.
pub fn blocking_mpsc_from_parts<T, const P: usize, const NUM_SEGS_P2: usize>(
    sender: Sender<T, P, NUM_SEGS_P2>,
    receiver: Receiver<T, P, NUM_SEGS_P2>,
) -> (
    BlockingMpscSender<T, P, NUM_SEGS_P2>,
    BlockingMpscReceiver<T, P, NUM_SEGS_P2>,
) {
    let inner = Arc::clone(receiver.inner());
    let shared = Arc::new(AsyncMpscShared::new(inner));
    let blocking_sender = BlockingMpscSender::new(sender, Arc::clone(&shared));
    let blocking_receiver = BlockingMpscReceiver::new(receiver, shared);
    (blocking_sender, blocking_receiver)
}

/// Creates a mixed MPSC queue with blocking senders and async receiver.
///
/// The blocking senders and async receiver share the same waker infrastructure,
/// so they can wake each other efficiently. This is useful when you have
/// blocking threads that need to send data to an async task.
///
/// # Example
///
/// ```ignore
/// let (sender, receiver) = blocking_async_mpsc();
///
/// // Senders (blocking threads)
/// for i in 0..4 {
///     let sender = sender.clone();
///     std::thread::spawn(move || {
///         sender.send(i).unwrap();
///     });
/// }
///
/// // Receiver (async task)
/// tokio::spawn(async move {
///     while let Some(item) = receiver.next().await {
///         println!("Got: {}", item);
///     }
/// });
/// ```
pub fn blocking_async_mpsc<T, const P: usize, const NUM_SEGS_P2: usize>() -> (
    BlockingMpscSender<T, P, NUM_SEGS_P2>,
    AsyncMpscReceiver<T, P, NUM_SEGS_P2>,
) {
    let (sender, receiver) = new_with_sender();
    blocking_async_mpsc_from_parts(sender, receiver)
}

/// Creates a mixed MPSC queue with blocking senders and async receiver from existing parts.
pub fn blocking_async_mpsc_from_parts<T, const P: usize, const NUM_SEGS_P2: usize>(
    sender: Sender<T, P, NUM_SEGS_P2>,
    receiver: Receiver<T, P, NUM_SEGS_P2>,
) -> (
    BlockingMpscSender<T, P, NUM_SEGS_P2>,
    AsyncMpscReceiver<T, P, NUM_SEGS_P2>,
) {
    let inner = Arc::clone(receiver.inner());
    let shared = Arc::new(AsyncMpscShared::new(inner));
    let blocking_sender = BlockingMpscSender::new(sender, Arc::clone(&shared));
    let async_receiver = AsyncMpscReceiver::new(receiver, shared);
    (blocking_sender, async_receiver)
}

/// Creates a mixed MPSC queue with async senders and blocking receiver.
///
/// The async senders and blocking receiver share the same waker infrastructure,
/// so they can wake each other efficiently. This is useful when you have async
/// tasks that need to send data to a blocking thread.
///
/// # Example
///
/// ```ignore
/// let (sender, receiver) = async_blocking_mpsc();
///
/// // Senders (async tasks)
/// for i in 0..4 {
///     let sender = sender.clone();
///     tokio::spawn(async move {
///         sender.send(i).await.unwrap();
///     });
/// }
///
/// // Receiver (blocking thread)
/// std::thread::spawn(move || {
///     while let Ok(item) = receiver.recv() {
///         println!("Got: {}", item);
///     }
/// });
/// ```
pub fn async_blocking_mpsc<T, const P: usize, const NUM_SEGS_P2: usize>() -> (
    AsyncMpscSender<T, P, NUM_SEGS_P2>,
    BlockingMpscReceiver<T, P, NUM_SEGS_P2>,
) {
    let (sender, receiver) = new_with_sender();
    async_blocking_mpsc_from_parts(sender, receiver)
}

/// Creates a mixed MPSC queue with async senders and blocking receiver from existing parts.
pub fn async_blocking_mpsc_from_parts<T, const P: usize, const NUM_SEGS_P2: usize>(
    sender: Sender<T, P, NUM_SEGS_P2>,
    receiver: Receiver<T, P, NUM_SEGS_P2>,
) -> (
    AsyncMpscSender<T, P, NUM_SEGS_P2>,
    BlockingMpscReceiver<T, P, NUM_SEGS_P2>,
) {
    let inner = Arc::clone(receiver.inner());
    let shared = Arc::new(AsyncMpscShared::new(inner));
    let async_sender = AsyncMpscSender::new(sender, Arc::clone(&shared));
    let blocking_receiver = BlockingMpscReceiver::new(receiver, shared);
    (async_sender, blocking_receiver)
}

/// Unbounded MPSC using UnboundedSpsc internally
///
/// This variant has no capacity limits - it grows dynamically as needed.
/// Each producer has its own unbounded queue, but unlike the bounded version,
/// there's no fixed upper limit on the number of items.
///
/// # Architecture
///
/// The unbounded MPSC uses `UnboundedSpsc` queues internally, where each producer
/// gets its own dynamically-growing queue. The receiver multiplexes across all
/// producer queues using a fair acquisition strategy.
///
/// ```text
/// Producer 1 ──→ UnboundedSpsc ──┐
/// Producer 2 ──→ UnboundedSpsc ──┤
/// Producer 3 ──→ UnboundedSpsc ──┼→ Receiver (polls all queues)
///     ...                        │
/// Producer N ──→ UnboundedSpsc ──┘
/// ```
///
/// # Features
///
/// - **True unbounded capacity**: Each producer queue grows dynamically
/// - **Lock-free**: Uses atomic operations for minimal contention
/// - **Fair scheduling**: Receiver uses randomized scheduling across producer queues
/// - **Interoperable**: Async and blocking senders/receivers can be mixed
/// - **Space waker integration**: Notifies producers when space becomes available
///
/// # Variants
///
/// - `async_unbounded_mpsc()` - Both senders and receiver are async
/// - `blocking_unbounded_mpsc()` - Both senders and receiver are blocking
/// - `blocking_async_unbounded_mpsc()` - Blocking senders, async receiver
/// - `async_blocking_unbounded_mpsc()` - Async senders, blocking receiver
///
/// # Example
///
/// ```ignore
/// use maniac_runtime::sync::mpsc::unbounded;
///
/// // Create async unbounded MPSC
/// let (mut sender, mut receiver) = unbounded::async_unbounded_mpsc::<u64>();
///
/// // Send from async context
/// tokio::spawn(async move {
///     for i in 0..1000 {
///         sender.send(i).await.unwrap();
///     }
/// });
///
/// // Receive from async context
/// tokio::spawn(async move {
///     while let Ok(value) = receiver.recv().await {
///         println!("Got: {}", value);
///     }
/// });
/// ```
pub mod unbounded {
    use super::*;
    use crate::spsc::unbounded::{UnboundedSender as UnboundedSender_, UnboundedReceiver as UnboundedReceiver_};
    use crate::spsc::NoOpSignal;

    /// A producer slot for unbounded MPSC containing an UnboundedSpsc queue
    struct UnboundedProducerSlot<T> {
        sender: UnboundedSender_<T>,
        receiver: UnboundedReceiver_<T>,
        space_waker: DiatomicWaker,
    }

    /// Shared state for unbounded MPSC
    ///
    /// # Memory Ordering Guarantees
    ///
    /// - `queues`: Atomic pointers use Acquire/Release for slot installation and cleanup
    /// - `queue_count`: Relaxed for statistics, not used for synchronization
    /// - `producer_count`: Release on decrement (UnboundedSender::drop), Relaxed on read after Acquire fence
    /// - `max_producer_id`: SeqCst for consistent global ordering across all threads
    /// - `closed`: Acquire/Release for happens-before between close and other operations
    /// - UnboundedSender::drop uses Release fence + Release store to ensure queued items are visible
    /// - UnboundedReceiver::try_pop uses Acquire fence before checking producer_count == 0
    struct UnboundedInner<T> {
        /// Sparse array of producer slot pointers
        queues: Box<[AtomicPtr<UnboundedProducerSlot<T>>]>,
        queue_count: CachePadded<AtomicUsize>,
        producer_count: CachePadded<AtomicUsize>,
        max_producer_id: AtomicUsize,
        closed: CachePadded<AtomicBool>,
        summary: Arc<AsyncSignalWaker>,
        signals: Arc<[Signal; SIGNAL_WORDS]>,
    }

    impl<T> UnboundedInner<T> {
        fn is_closed(&self) -> bool {
            self.closed.load(Ordering::Acquire)
        }

        fn producer_count(&self) -> usize {
            self.producer_count.load(Ordering::Relaxed)
        }

        fn create_sender(self: &Arc<Self>) -> Result<UnboundedSender<T>, PushError<()>> {
            if self.is_closed() {
                return Err(PushError::Closed(()));
            }

            loop {
                let current = self.producer_count.load(Ordering::Acquire);
                if current >= MAX_PRODUCERS {
                    return Err(PushError::Full(()));
                }
                if self
                    .producer_count
                    .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    break;
                }
            }

            let mut assigned_id = None;
            let mut slot_arc: Option<Arc<UnboundedProducerSlot<T>>> = None;

            for signal_index in 0..SIGNAL_WORDS {
                for bit_index in 0..64 {
                    let queue_index = signal_index * 64 + bit_index;
                    if queue_index >= MAX_QUEUES {
                        break;
                    }
                    if !self.queues[queue_index].load(Ordering::Acquire).is_null() {
                        continue;
                    }

                    let (sender, receiver) = UnboundedSpsc::<T, 6, 8, NoOpSignal>::new();
                    let slot = Arc::new(UnboundedProducerSlot {
                        sender,
                        receiver,
                        space_waker: DiatomicWaker::new(),
                    });

                    let raw = Arc::into_raw(Arc::clone(&slot)) as *mut UnboundedProducerSlot<T>;
                    match self.queues[queue_index].compare_exchange(
                        ptr::null_mut(),
                        raw,
                        Ordering::Release,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            self.queue_count.fetch_add(1, Ordering::Relaxed);
                            assigned_id = Some(queue_index);
                            slot_arc = Some(slot);
                            break;
                        }
                        Err(_) => unsafe {
                            Arc::from_raw(raw);
                        },
                    }
                }
                if assigned_id.is_some() {
                    break;
                }
            }

            let producer_id = match assigned_id {
                Some(id) => id,
                None => {
                    self.producer_count.fetch_sub(1, Ordering::Release);
                    return Err(PushError::Full(()));
                }
            };

            // Update max_producer_id if needed. We use SeqCst to ensure all threads
            // see a consistent view of the maximum producer ID, which is used by
            // the receiver to determine which slots to scan.
            loop {
                let max_producer_id = self.max_producer_id.load(Ordering::SeqCst);
                // If our ID is less than or equal to the current max, we're done
                if producer_id <= max_producer_id {
                    break;
                }
                if self.is_closed() {
                    return Err(PushError::Closed(()));
                }
                if self
                    .max_producer_id
                    .compare_exchange(
                        max_producer_id,
                        producer_id,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    break;
                }
            }

            // Use the slot_arc that was already captured during the CAS operation.
            // Arc reference counting: We created an Arc::new (refcount=1), then cloned it
            // and converted the clone to raw via Arc::into_raw(Arc::clone(&slot)).
            // - Raw pointer stored in array represents 1 Arc reference
            // - Original slot_arc variable holds the other Arc reference  
            // - Total: 2 references, properly balanced for array + Sender ownership
            let slot_arc = slot_arc.expect("slot arc missing");

            Ok(UnboundedSender {
                inner: Arc::clone(self),
                slot: slot_arc,
                producer_id,
            })
        }

        fn close(&self) -> bool {
            let was_open = !self.closed.swap(true, Ordering::AcqRel);
            if was_open {
                let permits = self.queue_count.load(Ordering::Relaxed).max(1);
                self.summary.release(permits);
            }

            for slot_atomic in self.queues.iter() {
                let slot_ptr = slot_atomic.swap(ptr::null_mut(), Ordering::AcqRel);
                if slot_ptr.is_null() {
                    continue;
                }

                unsafe {
                    (*slot_ptr).sender.close_channel();
                    Arc::from_raw(slot_ptr);
                }

                self.queue_count.fetch_sub(1, Ordering::Relaxed);
            }

            self.producer_count.store(0, Ordering::Release);
            true
        }
    }

    impl<T> Drop for UnboundedInner<T> {
        fn drop(&mut self) {
            self.close();
        }
    }

    /// Create a new unbounded MPSC queue
    pub fn unbounded_new<T>() -> UnboundedReceiver<T> {
        unbounded_new_with_waker(Arc::new(AsyncSignalWaker::new()))
    }

    /// Create a new unbounded MPSC queue with a custom waker
    pub fn unbounded_new_with_waker<T>(
        waker: Arc<AsyncSignalWaker>,
    ) -> UnboundedReceiver<T> {
        let mut queues = Vec::with_capacity(MAX_QUEUES);
        for _ in 0..MAX_QUEUES {
            queues.push(AtomicPtr::new(core::ptr::null_mut()));
        }

        let signals: Arc<[Signal; SIGNAL_WORDS]> =
            Arc::new(std::array::from_fn(|i| Signal::with_index(i as u64)));

        let inner = Arc::new(UnboundedInner {
            queues: queues.into_boxed_slice(),
            queue_count: CachePadded::new(AtomicUsize::new(0)),
            producer_count: CachePadded::new(AtomicUsize::new(0)),
            max_producer_id: AtomicUsize::new(0),
            closed: CachePadded::new(AtomicBool::new(false)),
            summary: waker,
            signals,
        });

        UnboundedReceiver {
            inner,
            misses: 0,
            seed: rand::rng().next_u64(),
        }
    }

    /// Create a new unbounded MPSC queue with a sender and receiver pair
    pub fn unbounded_new_with_sender<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
        let waker = Arc::new(AsyncSignalWaker::new());
        let mut queues = Vec::with_capacity(MAX_QUEUES);
        for _ in 0..MAX_QUEUES {
            queues.push(AtomicPtr::new(core::ptr::null_mut()));
        }

        let signals: Arc<[Signal; SIGNAL_WORDS]> =
            Arc::new(std::array::from_fn(|i| Signal::with_index(i as u64)));

        let inner = Arc::new(UnboundedInner {
            queues: queues.into_boxed_slice(),
            queue_count: CachePadded::new(AtomicUsize::new(0)),
            producer_count: CachePadded::new(AtomicUsize::new(0)),
            max_producer_id: AtomicUsize::new(0),
            closed: CachePadded::new(AtomicBool::new(false)),
            summary: waker,
            signals,
        });

        let receiver = UnboundedReceiver {
            inner: inner.clone(),
            misses: 0,
            seed: rand::rng().next_u64(),
        };

        let sender = inner
            .create_sender()
            .expect("fatal: unbounded mpsc won't allow even 1 sender");

        (sender, receiver)
    }

    /// Unbounded MPSC sender
    pub struct UnboundedSender<T> {
        inner: Arc<UnboundedInner<T>>,
        slot: Arc<UnboundedProducerSlot<T>>,
        producer_id: usize,
    }

    impl<T> UnboundedSender<T> {
        pub fn is_closed(&self) -> bool {
            self.inner.is_closed()
        }

        pub fn producer_count(&self) -> usize {
            self.inner.producer_count()
        }

        pub fn producer_id(&self) -> usize {
            self.producer_id
        }

        /// Try to push a value onto the queue
        pub fn try_push(&mut self, value: T) -> Result<(), PushError<T>> {
            self.slot.sender.try_push(value)
        }

        /// Close the queue
        pub fn close(&mut self) -> bool {
            self.inner.close()
        }
    }

    impl<T> Clone for UnboundedSender<T> {
        fn clone(&self) -> Self {
            self.inner
                .create_sender()
                .expect("too many senders")
        }
    }

    impl<T> Drop for UnboundedSender<T> {
        fn drop(&mut self) {
            // Close the channel to signal no more items will be sent from this sender
            // The slot itself remains alive because the receiver holds a reference via
            // the raw pointer in the queues array, and the Arc in the array keeps it alive
            self.slot.sender.close_channel();
            
            // Release fence to ensure all writes to the queue (from close_channel and
            // any prior pushes) are visible before we decrement producer_count.
            // This synchronizes with the Acquire fence in UnboundedReceiver::try_pop_with_waker(),
            // ensuring the receiver sees any items pushed before this sender was dropped.
            std::sync::atomic::fence(Ordering::Release);
            
            // Decrement the active producer count using Release ordering to publish
            // the fence above and all prior writes.
            self.inner.producer_count.fetch_sub(1, Ordering::Release);
        }
    }

    /// Unbounded MPSC receiver
    pub struct UnboundedReceiver<T> {
        inner: Arc<UnboundedInner<T>>,
        misses: u64,
        seed: u64,
    }

    impl<T> UnboundedReceiver<T> {
        fn next(&mut self) -> u64 {
            let old_seed = self.seed;
            let next_seed = (old_seed
                .wrapping_mul(RND_MULTIPLIER)
                .wrapping_add(RND_ADDEND))
                & RND_MASK;
            self.seed = next_seed;
            next_seed >> 16
        }

        pub fn is_closed(&self) -> bool {
            self.inner.closed.load(Ordering::Acquire)
        }

        pub fn close(&self) -> bool {
            self.inner.close()
        }

        pub fn producer_count(&self) -> usize {
            self.inner.producer_count()
        }

        pub fn create_sender(&self) -> Result<UnboundedSender<T>, PushError<()>> {
            self.inner.create_sender()
        }

        /// Try to pop a single value
        pub fn try_pop(&mut self) -> Result<T, PopError> {
            self.try_pop_with_waker().map(|(v, _)| v)
        }

        /// Try to pop a single value and return the waker
        pub fn try_pop_with_waker(&mut self) -> Result<(T, &DiatomicWaker), PopError> {
            // For unbounded, iterate through all registered producer slots
            let max_producer_id = self.inner.max_producer_id.load(Ordering::Acquire);
            
            // Try to pop from any queue
            for producer_id in 0..=max_producer_id {
                let slot_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
                if slot_ptr.is_null() {
                    continue;
                }

                // Safety: The slot pointer is valid for the lifetime of UnboundedInner.
                // Slots are never freed until Inner drops, so this is safe.
                let slot = unsafe { &*slot_ptr };
                if let Some(value) = slot.receiver.try_pop() {
                    return Ok((value, &slot.space_waker));
                }
            }

            // No items found in any queue.
            // Acquire fence BEFORE checking state to ensure we see all writes from producers.
            // This synchronizes with the Release fence in UnboundedSender::drop() and ensures
            // we see any items pushed before the last producer dropped.
            std::sync::atomic::fence(Ordering::Acquire);
            
            // Check if the channel is closed or all producers are gone
            let is_closed = self.inner.is_closed();
            // Use Relaxed here since the fence above provides the necessary synchronization
            let producer_count = self.inner.producer_count.load(Ordering::Relaxed);

            if is_closed || producer_count == 0 {
                // Do one final scan after the fence to catch any items that were pushed
                // just before the last producer dropped. The fence ensures we see them.
                for producer_id in 0..=max_producer_id {
                    let slot_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
                    if slot_ptr.is_null() {
                        continue;
                    }

                    // Safety: Same as above - slots are never freed until Inner drops
                    let slot = unsafe { &*slot_ptr };
                    if let Some(value) = slot.receiver.try_pop() {
                        return Ok((value, &slot.space_waker));
                    }
                }
                
                Err(PopError::Closed)
            } else {
                Err(PopError::Empty)
            }
        }

        fn acquire(&mut self) -> Option<*mut UnboundedProducerSlot<T>> {
            let random = self.next() as usize;
            let random_word = random % SIGNAL_WORDS;
            let mut signal_index = self.inner.summary.summary_select(random_word as u64) as usize;

            if signal_index >= SIGNAL_WORDS {
                signal_index = random_word;
            }

            let mut signal_bit = self.next() & 63;
            let signal = &self.inner.signals[signal_index];
            let signal_value = signal.load(Ordering::Acquire);

            signal_bit = find_nearest(signal_value, signal_bit);

            if signal_bit >= 64 {
                self.misses += 1;
                return None;
            }

            let (bit, expected, acquired) = signal.try_acquire(signal_bit);

            if !acquired {
                std::hint::spin_loop();
                return None;
            }

            let empty = expected == bit;

            if empty {
                self.inner
                    .summary
                    .try_unmark_if_empty(signal.index(), signal.value());
            }

            let producer_id = signal_index * 64 + (signal_bit as usize);
            let slot_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
            
            if slot_ptr.is_null() {
                self.misses += 1;
                if empty {
                    self.inner
                        .summary
                        .try_unmark_if_empty(signal.index(), signal.value());
                }
                return None;
            }

            Some(slot_ptr)
        }
    }

    impl<T> Drop for UnboundedReceiver<T> {
        fn drop(&mut self) {
            self.inner.close();
        }
    }

    /// Shared state for async unbounded MPSC
    struct UnboundedAsyncMpscShared<T> {
        receiver_waiter: CachePadded<DiatomicWaker>,
        inner: Arc<UnboundedInner<T>>,
    }

    impl<T> UnboundedAsyncMpscShared<T> {
        fn new(inner: Arc<UnboundedInner<T>>) -> Self {
            Self {
                receiver_waiter: CachePadded::new(DiatomicWaker::new()),
                inner,
            }
        }

        #[inline]
        fn notify_receiver(&self) {
            self.receiver_waiter.notify();
        }

        #[inline]
        unsafe fn wait_for_items<Pred, R>(&self, predicate: Pred) -> WaitUntil<'_, Pred, R>
        where
            Pred: FnMut() -> Option<R>,
        {
            unsafe { self.receiver_waiter.wait_until(predicate) }
        }

        #[inline]
        unsafe fn register_receiver(&self, waker: &Waker) {
            unsafe { self.receiver_waiter.register(waker) };
        }
    }

    /// Async unbounded MPSC sender
    pub struct AsyncUnboundedMpscSender<T> {
        sender: UnboundedSender<T>,
        shared: Arc<UnboundedAsyncMpscShared<T>>,
    }

    /// Async unbounded MPSC receiver
    pub struct AsyncUnboundedMpscReceiver<T> {
        receiver: UnboundedReceiver<T>,
        shared: Arc<UnboundedAsyncMpscShared<T>>,
    }

    impl<T> AsyncUnboundedMpscSender<T> {
        fn new(
            sender: UnboundedSender<T>,
            shared: Arc<UnboundedAsyncMpscShared<T>>,
        ) -> Self {
            Self { sender, shared }
        }

        /// Try to send without blocking
        #[inline]
        pub fn try_send(&mut self, value: T) -> Result<(), PushError<T>> {
            match self.sender.try_push(value) {
                Ok(()) => {
                    self.shared.notify_receiver();
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }

        /// Send a value asynchronously
        pub async fn send(&mut self, value: T) -> Result<(), PushError<T>> {
            match self.try_send(value) {
                Ok(()) => Ok(()),
                Err(PushError::Full(item)) => Err(PushError::Full(item)), // Unbounded never fills
                Err(PushError::Closed(item)) => Err(PushError::Closed(item)),
            }
        }

        pub fn close(&mut self) -> bool {
            self.sender.close()
        }

        /// Create a blocking sender that shares the same queue
        pub fn create_blocking_sender(&self) -> BlockingUnboundedMpscSender<T> {
            let sender = self.sender.clone();
            BlockingUnboundedMpscSender::new(sender, Arc::clone(&self.shared))
        }
    }

    impl<T> Clone for AsyncUnboundedMpscSender<T> {
        fn clone(&self) -> Self {
            let sender = self.sender.clone();
            Self::new(sender, Arc::clone(&self.shared))
        }
    }

    impl<T> Drop for AsyncUnboundedMpscSender<T> {
        fn drop(&mut self) {
            self.shared.notify_receiver();
        }
    }

    impl<T> AsyncUnboundedMpscReceiver<T> {
        fn new(
            receiver: UnboundedReceiver<T>,
            shared: Arc<UnboundedAsyncMpscShared<T>>,
        ) -> Self {
            Self { receiver, shared }
        }

        /// Get the number of registered producers
        pub fn producer_count(&self) -> usize {
            self.receiver.producer_count()
        }

        /// Try to receive without blocking
        #[inline]
        pub fn try_recv(&mut self) -> Result<T, PopError> {
            match self.receiver.try_pop_with_waker() {
                Ok((value, waker)) => {
                    waker.notify();
                    Ok(value)
                }
                Err(err) => Err(err),
            }
        }

        /// Receive a value asynchronously
        pub async fn recv(&mut self) -> Result<T, PopError> {
            match self.try_recv() {
                Ok(value) => Ok(value),
                Err(PopError::Empty) | Err(PopError::Timeout) => {
                    let shared = Arc::clone(&self.shared);
                    let receiver = &mut self.receiver;
                    unsafe {
                        shared
                            .wait_for_items(|| match receiver.try_pop_with_waker() {
                                Ok((value, waker)) => {
                                    waker.notify();
                                    Some(Ok(value))
                                }
                                Err(PopError::Empty) | Err(PopError::Timeout) => None,
                                Err(PopError::Closed) => Some(Err(PopError::Closed)),
                            })
                            .await
                    }
                }
                Err(PopError::Closed) => Err(PopError::Closed),
            }
        }
    }

    impl<T> Stream for AsyncUnboundedMpscReceiver<T> {
        type Item = T;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let this = unsafe { self.get_unchecked_mut() };
            match this.try_recv() {
                Ok(value) => Poll::Ready(Some(value)),
                Err(PopError::Closed) => Poll::Ready(None),
                Err(PopError::Empty) | Err(PopError::Timeout) => {
                    unsafe {
                        this.shared.register_receiver(cx.waker());
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

    /// Blocking unbounded MPSC sender
    pub struct BlockingUnboundedMpscSender<T> {
        sender: UnboundedSender<T>,
        shared: Arc<UnboundedAsyncMpscShared<T>>,
        parker: Parker,
        parker_waker: Arc<ThreadUnparker>,
    }

    impl<T> BlockingUnboundedMpscSender<T> {
        fn new(
            sender: UnboundedSender<T>,
            shared: Arc<UnboundedAsyncMpscShared<T>>,
        ) -> Self {
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

        /// Try to send without blocking
        #[inline]
        pub fn try_send(&mut self, value: T) -> Result<(), PushError<T>> {
            match self.sender.try_push(value) {
                Ok(()) => {
                    self.shared.notify_receiver();
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }

        /// Send a value, blocking until space is available if needed
        pub fn send(&mut self, value: T) -> Result<(), PushError<T>> {
            // For unbounded, we never fill, so this is just try_send
            self.try_send(value)
        }

        pub fn close(&mut self) -> bool {
            self.sender.close()
        }

        /// Create an async sender that shares the same queue
        pub fn create_async_sender(&self) -> AsyncUnboundedMpscSender<T> {
            let sender = self.sender.clone();
            AsyncUnboundedMpscSender::new(sender, Arc::clone(&self.shared))
        }
    }

    impl<T> Clone for BlockingUnboundedMpscSender<T> {
        fn clone(&self) -> Self {
            let sender = self.sender.clone();
            Self::new(sender, Arc::clone(&self.shared))
        }
    }

    impl<T> Drop for BlockingUnboundedMpscSender<T> {
        fn drop(&mut self) {
            self.shared.notify_receiver();
        }
    }

    /// Blocking unbounded MPSC receiver
    pub struct BlockingUnboundedMpscReceiver<T> {
        receiver: UnboundedReceiver<T>,
        shared: Arc<UnboundedAsyncMpscShared<T>>,
    }

    impl<T> BlockingUnboundedMpscReceiver<T> {
        fn new(
            receiver: UnboundedReceiver<T>,
            shared: Arc<UnboundedAsyncMpscShared<T>>,
        ) -> Self {
            Self { receiver, shared }
        }

        /// Get the number of registered producers
        pub fn producer_count(&self) -> usize {
            self.receiver.producer_count()
        }

        /// Try to receive without blocking
        #[inline]
        pub fn try_recv(&mut self) -> Result<T, PopError> {
            match self.receiver.try_pop_with_waker() {
                Ok((value, waker)) => {
                    waker.notify();
                    Ok(value)
                }
                Err(err) => Err(err),
            }
        }

        /// Blocking receive that parks the thread until an item is available
        pub fn recv(&mut self) -> Result<T, PopError> {
            match self.try_recv() {
                Ok(value) => return Ok(value),
                Err(PopError::Closed) => return Err(PopError::Closed),
                Err(PopError::Empty) | Err(PopError::Timeout) => {}
            }

            let parker = Parker::new();
            let unparker = parker.unparker();
            let waker = Waker::from(Arc::new(ThreadUnparker { unparker }));

            loop {
                unsafe {
                    self.shared.register_receiver(&waker);
                }

                match self.receiver.try_pop_with_waker() {
                    Ok((value, waker)) => {
                        waker.notify();
                        return Ok(value);
                    }
                    Err(PopError::Closed) => {
                        return Err(PopError::Closed);
                    }
                    Err(PopError::Empty) | Err(PopError::Timeout) => {
                        parker.park();
                    }
                }
            }
        }
    }


    /// Create an unbounded async MPSC queue
    pub fn async_unbounded_mpsc<T>() -> (AsyncUnboundedMpscSender<T>, AsyncUnboundedMpscReceiver<T>) {
        let (sender, receiver) = unbounded_new_with_sender();
        let inner = receiver.inner.clone();
        let shared = Arc::new(UnboundedAsyncMpscShared::new(inner));
        let async_sender = AsyncUnboundedMpscSender::new(sender, Arc::clone(&shared));
        let async_receiver = AsyncUnboundedMpscReceiver::new(receiver, shared);
        (async_sender, async_receiver)
    }

    /// Create a blocking unbounded MPSC queue
    pub fn blocking_unbounded_mpsc<T>() -> (BlockingUnboundedMpscSender<T>, BlockingUnboundedMpscReceiver<T>) {
        let (sender, receiver) = unbounded_new_with_sender();
        let inner = receiver.inner.clone();
        let shared = Arc::new(UnboundedAsyncMpscShared::new(inner));
        let blocking_sender = BlockingUnboundedMpscSender::new(sender, Arc::clone(&shared));
        let blocking_receiver = BlockingUnboundedMpscReceiver::new(receiver, shared);
        (blocking_sender, blocking_receiver)
    }

    /// Create a mixed unbounded MPSC with blocking senders and async receiver
    pub fn blocking_async_unbounded_mpsc<T>() -> (BlockingUnboundedMpscSender<T>, AsyncUnboundedMpscReceiver<T>) {
        let (sender, receiver) = unbounded_new_with_sender();
        let inner = receiver.inner.clone();
        let shared = Arc::new(UnboundedAsyncMpscShared::new(inner));
        let blocking_sender = BlockingUnboundedMpscSender::new(sender, Arc::clone(&shared));
        let async_receiver = AsyncUnboundedMpscReceiver::new(receiver, shared);
        (blocking_sender, async_receiver)
    }

    /// Create a mixed unbounded MPSC with async senders and blocking receiver
    pub fn async_blocking_unbounded_mpsc<T>() -> (AsyncUnboundedMpscSender<T>, BlockingUnboundedMpscReceiver<T>) {
        let (sender, receiver) = unbounded_new_with_sender();
        let inner = receiver.inner.clone();
        let shared = Arc::new(UnboundedAsyncMpscShared::new(inner));
        let async_sender = AsyncUnboundedMpscSender::new(sender, Arc::clone(&shared));
        let blocking_receiver = BlockingUnboundedMpscReceiver::new(receiver, shared);
        (async_sender, blocking_receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn try_pop_drains_and_reports_closed() {
        let (mut tx, mut rx) = new_with_sender::<u64, 6, 8>();

        tx.try_push(42).unwrap();
        assert_eq!(rx.try_pop().unwrap(), 42);
        assert_eq!(rx.try_pop(), Err(PopError::Empty));

        assert!(rx.close());
        assert_eq!(rx.try_pop(), Err(PopError::Closed));
    }

    #[test]
    fn dropping_local_sender_clears_producer_slot() {
        let (tx, rx) = new_with_sender::<u64, 6, 8>();
        assert_eq!(tx.producer_count(), 1);

        drop(tx);

        // Closing will walk the slots and remove the dropped sender.
        assert!(rx.close());
        assert_eq!(rx.producer_count(), 0);
        assert_eq!(rx.inner.queue_count.load(Ordering::SeqCst), 0);
    }

    // ===== Async Sender + Async Receiver Tests =====

    #[tokio::test]
    async fn async_async_basic_send_recv() {
        let (mut sender, mut receiver) = async_mpsc::<u64, 6, 8>();

        sender.send(42).await.unwrap();
        assert_eq!(receiver.recv().await.unwrap(), 42);
    }

    #[tokio::test]
    async fn async_async_multiple_senders() {
        let (sender, mut receiver) = async_mpsc::<u64, 6, 8>();
        let mut sender1 = sender.clone();
        let mut sender2 = sender.clone();
        let mut sender3 = sender.clone();

        tokio::spawn(async move {
            sender1.send(1).await.unwrap();
        });
        tokio::spawn(async move {
            sender2.send(2).await.unwrap();
        });
        tokio::spawn(async move {
            sender3.send(3).await.unwrap();
        });

        let mut received = Vec::new();
        for _ in 0..3 {
            received.push(receiver.recv().await.unwrap());
        }
        received.sort();
        assert_eq!(received, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn async_async_batch_operations() {
        let (mut sender, mut receiver) = async_mpsc::<u64, 6, 8>();

        // Send batch
        assert!(
            sender
                .send_slice(vec![1, 2, 3, 4, 5])
                .await
                .unwrap()
                .is_empty()
        );

        // Receive batch
        let mut buf = [0u64; 5];
        let count = receiver.recv_batch(&mut buf).await.unwrap();
        assert_eq!(count, 5);
        assert_eq!(buf, [1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn async_async_closed_queue() {
        let (mut sender, mut receiver) = async_mpsc::<u64, 6, 8>();

        sender.send(42).await.unwrap();
        assert_eq!(receiver.recv().await.unwrap(), 42);

        // Drop sender - producer_count is decremented immediately in drop()
        drop(sender);

        // Try to receive - should get Closed error (producer_count == 0 means closed)
        assert_eq!(receiver.recv().await, Err(PopError::Closed));
    }

    // ===== Blocking Sender + Blocking Receiver Tests =====

    #[test]
    fn blocking_blocking_basic_send_recv() {
        let (mut sender, mut receiver) = blocking_mpsc::<u64, 6, 8>();

        sender.send(42).unwrap();
        assert_eq!(receiver.recv().unwrap(), 42);
    }

    #[test]
    fn blocking_blocking_multiple_senders() {
        let (sender, receiver) = blocking_mpsc::<u64, 6, 8>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        let mut handles = Vec::new();
        for i in 0..5 {
            let mut sender = sender.clone();
            let receiver = Arc::clone(&receiver);
            handles.push(thread::spawn(move || {
                sender.send(i).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let mut received = Vec::new();
        let mut receiver = receiver.lock().unwrap();
        for _ in 0..5 {
            received.push(receiver.recv().unwrap());
        }
        received.sort();
        assert_eq!(received, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn blocking_blocking_batch_operations() {
        let (mut sender, mut receiver) = blocking_mpsc::<u64, 6, 8>();

        // Send batch
        assert!(sender.send_slice(vec![1, 2, 3, 4, 5]).unwrap().is_empty());

        // Receive batch
        let mut buf = [0u64; 5];
        let count = receiver.recv_batch(&mut buf).unwrap();
        assert_eq!(count, 5);
        assert_eq!(buf, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn blocking_blocking_closed_queue() {
        let (mut sender, mut receiver) = blocking_mpsc::<u64, 6, 8>();

        sender.send(42).unwrap();
        assert_eq!(receiver.recv().unwrap(), 42);

        // Drop sender - producer_count is decremented immediately in drop()
        drop(sender);

        // Try to receive - should get Closed error (producer_count == 0 means closed)
        assert_eq!(receiver.recv(), Err(PopError::Closed));
    }

    // ===== Blocking Sender + Async Receiver Tests =====

    #[tokio::test]
    async fn blocking_async_basic_send_recv() {
        let (mut sender, mut receiver) = blocking_async_mpsc::<u64, 6, 8>();

        // Blocking sender in a thread
        let handle = thread::spawn(move || {
            sender.send(42).unwrap();
        });

        handle.join().unwrap();
        assert_eq!(receiver.recv().await.unwrap(), 42);
    }

    #[tokio::test]
    async fn blocking_async_multiple_blocking_senders() {
        let (sender, mut receiver) = blocking_async_mpsc::<u64, 6, 8>();

        let mut handles = Vec::new();
        for i in 0..5 {
            let mut sender = sender.clone();
            handles.push(thread::spawn(move || {
                sender.send(i).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let mut received = Vec::new();
        for _ in 0..5 {
            received.push(receiver.recv().await.unwrap());
        }
        received.sort();
        assert_eq!(received, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn blocking_async_wakeup_async_receiver() {
        let (mut sender, mut receiver) = blocking_async_mpsc::<u64, 6, 8>();

        // Start async receiver waiting
        let recv_handle = tokio::spawn(async move { receiver.recv().await.unwrap() });

        // Give async task time to register waker
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Blocking sender wakes async receiver
        thread::spawn(move || {
            sender.send(99).unwrap();
        });

        assert_eq!(recv_handle.await.unwrap(), 99);
    }

    #[tokio::test]
    async fn blocking_async_batch_operations() {
        let (mut sender, mut receiver) = blocking_async_mpsc::<u64, 6, 8>();

        // Blocking sender sends batch
        thread::spawn(move || {
            let _ = sender.send_slice(vec![1, 2, 3, 4, 5]).unwrap();
        });

        // Async receiver receives batch
        let mut buf = [0u64; 5];
        let count = receiver.recv_batch(&mut buf).await.unwrap();
        assert_eq!(count, 5);
        assert_eq!(buf, [1, 2, 3, 4, 5]);
    }

    // ===== Async Sender + Blocking Receiver Tests =====

    #[tokio::test]
    async fn async_blocking_basic_send_recv() {
        let (mut sender, receiver) = async_blocking_mpsc::<u64, 6, 8>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        // Start blocking receiver first (will wait)
        let receiver_clone = Arc::clone(&receiver);
        let handle = thread::spawn(move || receiver_clone.lock().unwrap().recv().unwrap());

        // Give blocking thread time to park
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Async sender wakes blocking receiver
        sender.send(42).await.unwrap();

        assert_eq!(handle.join().unwrap(), 42);
    }

    #[tokio::test]
    async fn async_blocking_multiple_async_senders() {
        let (sender, receiver) = async_blocking_mpsc::<u64, 6, 8>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        // Multiple async senders
        for i in 0..5 {
            let mut sender = sender.clone();
            tokio::spawn(async move {
                sender.send(i).await.unwrap();
            });
        }

        // Give async tasks time to send
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Blocking receiver receives all
        let mut received = Vec::new();
        let mut receiver = receiver.lock().unwrap();
        for _ in 0..5 {
            received.push(receiver.recv().unwrap());
        }
        received.sort();
        assert_eq!(received, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn async_blocking_wakeup_blocking_receiver() {
        let (mut sender, receiver) = async_blocking_mpsc::<u64, 6, 8>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        // Start blocking receiver waiting
        let receiver_clone = Arc::clone(&receiver);
        let recv_handle = thread::spawn(move || receiver_clone.lock().unwrap().recv().unwrap());

        // Give blocking thread time to park
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Async sender wakes blocking receiver
        let send_handle = tokio::spawn(async move {
            sender.send(88).await.unwrap();
        });

        // Wait for both to complete
        send_handle.await.unwrap();
        assert_eq!(recv_handle.join().unwrap(), 88);
    }

    #[tokio::test]
    async fn async_blocking_batch_operations() {
        let (mut sender, receiver) = async_blocking_mpsc::<u64, 6, 8>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        // Async sender sends batch
        tokio::spawn(async move {
            assert!(
                sender
                    .send_slice(vec![1, 2, 3, 4, 5])
                    .await
                    .unwrap()
                    .is_empty()
            );
        });

        // Give async task time to send
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Blocking receiver receives batch
        let receiver_clone = Arc::clone(&receiver);
        let handle = thread::spawn(move || {
            let mut buf = [0u64; 5];
            let count = receiver_clone.lock().unwrap().recv_batch(&mut buf).unwrap();
            (count, buf)
        });

        let (count, buf) = handle.join().unwrap();
        assert_eq!(count, 5);
        assert_eq!(buf, [1, 2, 3, 4, 5]);
    }

    // ===== Mixed Sender Types Tests =====
    // Note: Testing true mixed senders (async + blocking from same queue) requires
    // creating senders from the same underlying queue, which is complex with the current API.
    // The individual combination tests above demonstrate that async and blocking
    // components can interoperate correctly through the shared waker infrastructure.

    // ===== Error Handling Tests =====

    #[tokio::test]
    async fn async_async_try_send_full() {
        let (mut sender, _receiver) = async_mpsc::<u64, 6, 8>();

        // Fill the queue (this depends on queue capacity)
        // For now, just test that try_send works
        assert!(sender.try_send(1).is_ok());
    }

    #[test]
    fn blocking_blocking_try_send_full() {
        let (mut sender, _receiver) = blocking_mpsc::<u64, 6, 8>();

        assert!(sender.try_send(1).is_ok());
    }

    #[tokio::test]
    async fn async_async_closed_sender() {
        let (mut sender, mut receiver) = async_mpsc::<u64, 6, 8>();

        // Send one item and receive it
        sender.send(1).await.unwrap();
        assert_eq!(receiver.recv().await.unwrap(), 1);

        // Drop receiver to close the queue
        drop(receiver);

        // Sender should now see closed queue
        assert_eq!(sender.send(42).await, Err(PushError::Closed(42)));
    }

    #[test]
    fn blocking_blocking_closed_sender() {
        let (mut sender, mut receiver) = blocking_mpsc::<u64, 6, 8>();

        // Send one item and receive it
        sender.send(1).unwrap();
        assert_eq!(receiver.recv().unwrap(), 1);

        // Drop receiver to close the queue
        drop(receiver);

        // Sender should now see closed queue
        assert_eq!(sender.send(42), Err(PushError::Closed(42)));
    }

    // ===== Unbounded MPSC Tests (Mirror of Bounded Tests) =====

    #[test]
    fn unbounded_try_pop_drains_and_reports_closed() {
        use crate::sync::mpsc::unbounded;
        let (mut tx, mut rx) = unbounded::unbounded_new_with_sender::<u64>();

        tx.try_push(42).unwrap();
        assert_eq!(rx.try_pop().unwrap(), 42);
        assert_eq!(rx.try_pop(), Err(PopError::Empty));

        assert!(rx.close());
        assert_eq!(rx.try_pop(), Err(PopError::Closed));
    }

    #[test]
    fn unbounded_dropping_local_sender_clears_producer_slot() {
        use crate::sync::mpsc::unbounded;
        let (tx, rx) = unbounded::unbounded_new_with_sender::<u64>();
        assert_eq!(tx.producer_count(), 1);

        drop(tx);

        // Closing will walk the slots and remove the dropped sender.
        assert!(rx.close());
        assert_eq!(rx.producer_count(), 0);
    }

    // ===== Unbounded Async Sender + Async Receiver Tests =====

    #[tokio::test]
    async fn unbounded_async_async_basic_send_recv() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::async_unbounded_mpsc::<u64>();

        sender.send(42).await.unwrap();
        assert_eq!(receiver.recv().await.unwrap(), 42);
    }

    #[tokio::test]
    async fn unbounded_async_async_multiple_senders() {
        use crate::sync::mpsc::unbounded;
        let (sender, mut receiver) = unbounded::async_unbounded_mpsc::<u64>();
        let mut sender1 = sender.clone();
        let mut sender2 = sender.clone();
        let mut sender3 = sender.clone();

        tokio::spawn(async move {
            sender1.send(1).await.unwrap();
        });
        tokio::spawn(async move {
            sender2.send(2).await.unwrap();
        });
        tokio::spawn(async move {
            sender3.send(3).await.unwrap();
        });

        let mut received = Vec::new();
        for _ in 0..3 {
            received.push(receiver.recv().await.unwrap());
        }
        received.sort();
        assert_eq!(received, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn unbounded_async_async_closed_queue() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::async_unbounded_mpsc::<u64>();

        sender.send(42).await.unwrap();
        assert_eq!(receiver.recv().await.unwrap(), 42);

        // Drop sender - producer_count is decremented immediately in drop()
        drop(sender);

        // Try to receive - should get Closed error (producer_count == 0 means closed)
        assert_eq!(receiver.recv().await, Err(PopError::Closed));
    }

    #[tokio::test]
    async fn unbounded_async_async_try_send_full() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, _receiver) = unbounded::async_unbounded_mpsc::<u64>();

        // Unbounded never fills, so this should always succeed
        assert!(sender.try_send(1).is_ok());
        assert!(sender.try_send(2).is_ok());
        assert!(sender.try_send(3).is_ok());
    }

    #[tokio::test]
    async fn unbounded_async_async_closed_sender() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::async_unbounded_mpsc::<u64>();

        // Send one item and receive it
        sender.send(1).await.unwrap();
        assert_eq!(receiver.recv().await.unwrap(), 1);

        // Drop receiver to close the queue
        drop(receiver);

        // Sender should now see closed queue
        assert_eq!(sender.send(42).await, Err(PushError::Closed(42)));
    }

    // ===== Unbounded Blocking Sender + Blocking Receiver Tests =====

    #[test]
    fn unbounded_blocking_blocking_basic_send_recv() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::blocking_unbounded_mpsc::<u64>();

        sender.send(42).unwrap();
        assert_eq!(receiver.recv().unwrap(), 42);
    }

    #[test]
    fn unbounded_blocking_blocking_multiple_senders() {
        use crate::sync::mpsc::unbounded;
        let (sender, receiver) = unbounded::blocking_unbounded_mpsc::<u64>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        let mut handles = Vec::new();
        for i in 0..5 {
            let mut sender = sender.clone();
            let receiver = Arc::clone(&receiver);
            handles.push(thread::spawn(move || {
                sender.send(i).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let mut received = Vec::new();
        let mut receiver = receiver.lock().unwrap();
        for _ in 0..5 {
            received.push(receiver.recv().unwrap());
        }
        received.sort();
        assert_eq!(received, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn unbounded_blocking_blocking_closed_queue() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::blocking_unbounded_mpsc::<u64>();

        sender.send(42).unwrap();
        assert_eq!(receiver.recv().unwrap(), 42);

        // Drop sender - producer_count is decremented immediately in drop()
        drop(sender);

        // Try to receive - should get Closed error (producer_count == 0 means closed)
        assert_eq!(receiver.recv(), Err(PopError::Closed));
    }

    #[test]
    fn unbounded_blocking_blocking_try_send_full() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, _receiver) = unbounded::blocking_unbounded_mpsc::<u64>();

        // Unbounded never fills, so this should always succeed
        assert!(sender.try_send(1).is_ok());
    }

    #[test]
    fn unbounded_blocking_blocking_closed_sender() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::blocking_unbounded_mpsc::<u64>();

        // Send one item and receive it
        sender.send(1).unwrap();
        assert_eq!(receiver.recv().unwrap(), 1);

        // Drop receiver to close the queue
        drop(receiver);

        // Sender should now see closed queue
        assert_eq!(sender.send(42), Err(PushError::Closed(42)));
    }

    // ===== Unbounded Blocking Sender + Async Receiver Tests =====

    #[tokio::test]
    async fn unbounded_blocking_async_basic_send_recv() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::blocking_async_unbounded_mpsc::<u64>();

        // Blocking sender in a thread
        let handle = thread::spawn(move || {
            sender.send(42).unwrap();
        });

        handle.join().unwrap();
        assert_eq!(receiver.recv().await.unwrap(), 42);
    }

    #[tokio::test]
    async fn unbounded_blocking_async_multiple_blocking_senders() {
        use crate::sync::mpsc::unbounded;
        let (sender, mut receiver) = unbounded::blocking_async_unbounded_mpsc::<u64>();

        let mut handles = Vec::new();
        for i in 0..5 {
            let mut sender = sender.clone();
            handles.push(thread::spawn(move || {
                sender.send(i).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let mut received = Vec::new();
        for _ in 0..5 {
            received.push(receiver.recv().await.unwrap());
        }
        received.sort();
        assert_eq!(received, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn unbounded_blocking_async_wakeup_async_receiver() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::blocking_async_unbounded_mpsc::<u64>();

        // Start async receiver waiting
        let recv_handle = tokio::spawn(async move { receiver.recv().await.unwrap() });

        // Give async task time to register waker
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Blocking sender wakes async receiver
        thread::spawn(move || {
            sender.send(99).unwrap();
        });

        assert_eq!(recv_handle.await.unwrap(), 99);
    }

    // ===== Unbounded Async Sender + Blocking Receiver Tests =====

    #[tokio::test]
    async fn unbounded_async_blocking_basic_send_recv() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, receiver) = unbounded::async_blocking_unbounded_mpsc::<u64>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        // Start blocking receiver first (will wait)
        let receiver_clone = Arc::clone(&receiver);
        let handle = thread::spawn(move || receiver_clone.lock().unwrap().recv().unwrap());

        // Give blocking thread time to park
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Async sender wakes blocking receiver
        sender.send(42).await.unwrap();

        assert_eq!(handle.join().unwrap(), 42);
    }

    #[tokio::test]
    async fn unbounded_async_blocking_multiple_async_senders() {
        use crate::sync::mpsc::unbounded;
        let (sender, receiver) = unbounded::async_blocking_unbounded_mpsc::<u64>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        // Multiple async senders
        for i in 0..5 {
            let mut sender = sender.clone();
            tokio::spawn(async move {
                sender.send(i).await.unwrap();
            });
        }

        // Give async tasks time to send
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Blocking receiver receives all
        let mut received = Vec::new();
        let mut receiver = receiver.lock().unwrap();
        for _ in 0..5 {
            received.push(receiver.recv().unwrap());
        }
        received.sort();
        assert_eq!(received, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn unbounded_async_blocking_wakeup_blocking_receiver() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, receiver) = unbounded::async_blocking_unbounded_mpsc::<u64>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        // Start blocking receiver waiting
        let receiver_clone = Arc::clone(&receiver);
        let recv_handle = thread::spawn(move || receiver_clone.lock().unwrap().recv().unwrap());

        // Give blocking thread time to park
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Async sender wakes blocking receiver
        let send_handle = tokio::spawn(async move {
            sender.send(88).await.unwrap();
        });

        // Wait for both to complete
        send_handle.await.unwrap();
        assert_eq!(recv_handle.join().unwrap(), 88);
    }

    // ===== Unbounded High-Volume Stress Tests =====

    #[test]
    fn unbounded_high_volume_stress() {
        use crate::sync::mpsc::unbounded;
        let (sender, receiver) = unbounded::blocking_unbounded_mpsc::<u64>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        // Producer threads
        let mut producer_handles = Vec::new();
        for producer_id in 0..4 {
            let mut sender = sender.clone();
            let handle = thread::spawn(move || {
                for i in 0..250 {
                    let value = producer_id * 1000 + i;
                    sender.send(value).unwrap();
                }
            });
            producer_handles.push(handle);
        }

        // Consumer thread
        let receiver_clone = Arc::clone(&receiver);
        let consumer_handle = thread::spawn(move || {
            let mut received = Vec::new();
            let mut receiver = receiver_clone.lock().unwrap();
            
            loop {
                match receiver.recv() {
                    Ok(value) => {
                        received.push(value);
                    }
                    Err(PopError::Closed) => break,
                    Err(PopError::Empty) | Err(PopError::Timeout) => {
                        // Keep retrying - the channel will eventually close
                        thread::sleep(std::time::Duration::from_micros(1));
                    }
                }
            }
            received
        });

        // Wait for producers to complete
        for handle in producer_handles {
            handle.join().unwrap();
        }
        
        // Small sleep to ensure all items are settled in queues
        thread::sleep(Duration::from_millis(10));
        
        drop(sender); // Close the channel

        // Wait for consumer
        let mut received = consumer_handle.join().unwrap();
        received.sort();

        // Verify all items received
        assert_eq!(received.len(), 1000, "Expected 1000 items but got {}", received.len());
        
        // Build expected values: each producer sends 250 items in its range
        let mut expected = Vec::new();
        for producer_id in 0..4 {
            for i in 0..250 {
                expected.push(producer_id * 1000 + i);
            }
        }
        expected.sort();
        
        assert_eq!(received, expected);
    }

    #[tokio::test]
    async fn unbounded_async_high_volume_stress() {
        use crate::sync::mpsc::unbounded;
        let (sender, receiver) = unbounded::async_unbounded_mpsc::<u64>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        // Multiple async producers
        let mut producer_tasks = Vec::new();
        for producer_id in 0..4 {
            let mut sender = sender.clone();
            let task = tokio::spawn(async move {
                for i in 0..250 {
                    let value = producer_id * 1000 + i;
                    sender.send(value).await.unwrap();
                }
            });
            producer_tasks.push(task);
        }

        // Collect all received items using a loop that breaks on close
        let receiver_clone = Arc::clone(&receiver);
        let receiver_task = tokio::spawn(async move {
            let mut items: Vec<u64> = Vec::new();
            
            loop {
                let result = {
                    let mut receiver_guard = receiver_clone.lock().unwrap();
                    receiver_guard.try_recv()
                };
                
                match result {
                    Ok(value) => {
                        items.push(value);
                    }
                    Err(PopError::Closed) => break,
                    Err(PopError::Empty) | Err(PopError::Timeout) => {
                        // Keep retrying - the channel will eventually close
                        tokio::time::sleep(Duration::from_micros(1)).await;
                    }
                }
            }
            items
        });

        // Wait for producers to complete
        for task in producer_tasks {
            task.await.unwrap();
        }
        
        // Small delay to ensure all items are settled
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        drop(sender); // Close the sender

        // Wait for receiver task
        let mut recv_items = receiver_task.await.unwrap();

        // Verify all items received
        recv_items.sort();
        assert_eq!(recv_items.len(), 1000, "Expected 1000 items but got {}", recv_items.len());
        
        // Build expected values: each producer sends 250 items in its range
        let mut expected = Vec::new();
        for producer_id in 0..4 {
            for i in 0..250 {
                expected.push(producer_id * 1000 + i);
            }
        }
        expected.sort();
        
        assert_eq!(recv_items, expected);
    }

    // ===== Unbounded Capacity Tests =====

    #[test]
    fn unbounded_can_push_beyond_typical_limits() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::unbounded_new_with_sender::<u64>();

        // Push way more items than would fit in a bounded queue
        const NUM_ITEMS: u64 = 10_000;
        for i in 0..NUM_ITEMS {
            sender.try_push(i).unwrap();
        }

        // All items should be retrievable
        for i in 0..NUM_ITEMS {
            assert_eq!(receiver.try_pop().unwrap(), i);
        }

        assert_eq!(receiver.try_pop(), Err(PopError::Empty));
    }

    #[tokio::test]
    async fn unbounded_async_can_push_beyond_typical_limits() {
        use crate::sync::mpsc::unbounded;
        let (mut sender, mut receiver) = unbounded::async_unbounded_mpsc::<u64>();

        // Push many items asynchronously
        const NUM_ITEMS: u64 = 10_000;
        for i in 0..NUM_ITEMS {
            sender.send(i).await.unwrap();
        }

        // All items should be retrievable
        for i in 0..NUM_ITEMS {
            assert_eq!(receiver.recv().await.unwrap(), i);
        }

        // Close and verify
        drop(sender);
        assert_eq!(receiver.recv().await, Err(PopError::Closed));
    }

    // ===== Unbounded Producer Count Tests =====

    #[test]
    fn unbounded_producer_count_tracking() {
        use crate::sync::mpsc::unbounded;
        let (sender, rx) = unbounded::unbounded_new_with_sender::<u64>();
        assert_eq!(sender.producer_count(), 1);

        let sender2 = sender.clone();
        assert_eq!(sender2.producer_count(), 2);

        let sender3 = sender.clone();
        assert_eq!(sender3.producer_count(), 3);

        drop(sender);
        assert_eq!(sender2.producer_count(), 2);

        drop(sender2);
        assert_eq!(sender3.producer_count(), 1);

        drop(sender3);
        // After all senders dropped, closing should report 0
        rx.close();
    }

    // ===== Unified MPSC Tests (Mixed Async/Blocking) =====

    #[tokio::test]
    async fn unbounded_mixed_async_blocking_senders() {
        use crate::sync::mpsc::unbounded;
        let (async_sender, mut receiver) = unbounded::async_unbounded_mpsc::<u64>();

        // Create a blocking sender from the async sender
        let blocking_sender = async_sender.create_blocking_sender();

        // Async producer
        let mut async_sender_clone = async_sender.clone();
        let async_task = tokio::spawn(async move {
            for i in 0..100 {
                async_sender_clone.send(i).await.unwrap();
            }
        });

        // Blocking producer in a thread
        let blocking_sender_clone = blocking_sender.clone();
        let blocking_thread = thread::spawn(move || {
            let mut sender = blocking_sender_clone;
            for i in 100..200 {
                sender.send(i).unwrap();
            }
        });

        // Drop original senders
        drop(async_sender);
        drop(blocking_sender);

        // Wait for producers
        async_task.await.unwrap();
        blocking_thread.join().unwrap();

        // Collect all items using async recv
        let mut items = Vec::new();
        loop {
            match receiver.recv().await {
                Ok(value) => items.push(value),
                Err(PopError::Closed) => break,
                Err(_) => continue,
            }
        }

        // Verify
        items.sort();
        assert_eq!(items.len(), 200);
        assert_eq!(items, (0..200).collect::<Vec<_>>());
    }

    #[test]
    fn unbounded_mixed_blocking_recv() {
        use crate::sync::mpsc::unbounded;
        let (blocking_sender, mut receiver) = unbounded::blocking_unbounded_mpsc::<u64>();

        // Create an async sender from the blocking sender
        let async_sender = blocking_sender.create_async_sender();

        // Blocking producer in a thread
        let blocking_sender_clone = blocking_sender.clone();
        let blocking_thread = thread::spawn(move || {
            let mut sender = blocking_sender_clone;
            for i in 0..100 {
                sender.send(i).unwrap();
            }
        });

        // Drop original senders
        drop(async_sender);
        drop(blocking_sender);

        // Wait for producer
        blocking_thread.join().unwrap();

        // Collect all items using blocking recv
        let mut items = Vec::new();
        loop {
            match receiver.recv() {
                Ok(value) => items.push(value),
                Err(PopError::Closed) => break,
                Err(_) => continue,
            }
        }

        // Verify
        items.sort();
        assert_eq!(items.len(), 100);
        assert_eq!(items, (0..100).collect::<Vec<_>>());
    }
}
