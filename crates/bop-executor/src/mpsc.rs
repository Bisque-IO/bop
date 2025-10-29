use crate::CachePadded;
use crate::bits::find_nearest;
use crate::seg_spsc::SegSpsc;
use crate::selector::Selector;
use crate::signal::{SIGNAL_MASK, Signal, SignalGate};
use crate::signal_waker::SignalWaker;
use crate::{PopError, PushError};
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use std::thread;

use rand::RngCore;

/// Create a new blocking MPSC queue
pub fn new<T, const P: usize, const NUM_SEGS_P2: usize>() -> Receiver<T, P, NUM_SEGS_P2> {
    new_with_waker(Arc::new(SignalWaker::new()))
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
    waker: Arc<SignalWaker>,
) -> Receiver<T, P, NUM_SEGS_P2> {
    // Create sparse array of AtomicPtr, all initialized to null
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
        waker,
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
    let waker = Arc::new(SignalWaker::new());
    // Create sparse array of AtomicPtr, all initialized to null
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
        waker: waker,
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

/// Maximum number of producers
const MAX_QUEUES: usize = 4096;
const MAX_QUEUES_MASK: usize = MAX_QUEUES - 1;
const QUEUES_PER_PRODUCER: usize = 1;
const MAX_PRODUCERS: usize = MAX_QUEUES / QUEUES_PER_PRODUCER;
const MAX_PRODUCERS_MASK: usize = QUEUES_PER_PRODUCER - 1;

/// Number of u64 words needed for the signal bitset
const SIGNAL_WORDS: usize = (MAX_QUEUES + 63) / 64;

/// Thread-local cache of SPSC queues for each producer thread
/// Stores (producer_id, queue_ptr, close_fn) where close_fn can close the queue
type CloseFn = Box<dyn FnOnce()>;

/// The shared state of the blocking MPSC queue
struct Inner<T, const P: usize, const NUM_SEGS_P2: usize> {
    /// Sparse array of producer queues - always MAX_PRODUCERS size
    /// Each slot is an AtomicPtr for thread-safe registration and access
    queues: Box<[AtomicPtr<SegSpsc<T, P, NUM_SEGS_P2>>]>,
    queue_count: CachePadded<AtomicUsize>,
    /// Number of registered producers
    producer_count: CachePadded<AtomicUsize>,
    max_producer_id: AtomicUsize,
    /// Closed flag
    closed: CachePadded<AtomicBool>,
    waker: Arc<SignalWaker>,
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
        let mut queue_arc: Option<Arc<SegSpsc<T, P, NUM_SEGS_P2>>> = None;

        for signal_index in 0..(MAX_QUEUES / 64) {
            for bit_index in 0..64 {
                let queue_index = signal_index * 64 + bit_index;
                if queue_index >= MAX_QUEUES {
                    break;
                }
                if !self.queues[queue_index].load(Ordering::Acquire).is_null() {
                    continue;
                }

                let arc = Arc::new(unsafe {
                    SegSpsc::<T, P, NUM_SEGS_P2>::new_unsafe_with_gate_and_config(
                        SignalGate::new(
                            bit_index as u8,
                            self.signals[signal_index].clone(),
                            Arc::clone(&self.waker),
                        ),
                        max_pooled_segments,
                    )
                });

                let raw = Arc::into_raw(Arc::clone(&arc)) as *mut SegSpsc<T, P, NUM_SEGS_P2>;
                match self.queues[queue_index].compare_exchange(
                    ptr::null_mut(),
                    raw,
                    Ordering::Release,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        self.queue_count.fetch_add(1, Ordering::Relaxed);
                        assigned_id = Some(queue_index);
                        queue_arc = Some(arc);
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

        loop {
            let max_producer_id = self.max_producer_id.load(Ordering::SeqCst);
            if producer_id < max_producer_id {
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

        let queue_arc = queue_arc.expect("queue arc missing");

        Ok(Sender {
            inner: Arc::clone(&self),
            queue: queue_arc,
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
            self.waker.release(permits);
        }

        for slot in self.queues.iter() {
            let queue_ptr = slot.swap(ptr::null_mut(), Ordering::AcqRel);
            if queue_ptr.is_null() {
                continue;
            }

            unsafe {
                (&*queue_ptr).close();
                Arc::from_raw(queue_ptr);
            }

            self.queue_count.fetch_sub(1, Ordering::Relaxed);
            self.producer_count.fetch_sub(1, Ordering::Relaxed);
        }

        self.producer_count.load(Ordering::Acquire) == 0
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
    queue: Arc<SegSpsc<T, P, NUM_SEGS_P2>>,
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
        self.queue.try_push(value)
    }

    /// Push a value onto the queue (spins if full)
    ///
    /// This will use the thread-local SPSC queue for this producer.
    /// If successful, notifies any waiting consumer.
    pub fn push_spin(&mut self, mut value: T) -> Result<(), PushError<T>> {
        loop {
            match self.queue.try_push(value) {
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
        self.queue.try_push_n(values)
    }

    /// Push a value onto the queue
    ///
    /// This will use the thread-local SPSC queue for this producer.
    /// If successful, notifies any waiting consumer.
    pub unsafe fn unsafe_try_push(&self, value: T) -> Result<(), PushError<T>> {
        self.queue.try_push(value)
    }

    /// Push multiple values in bulk
    pub unsafe fn unsafe_try_push_n(&self, values: &[T]) -> Result<usize, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }
        self.queue.try_push_n(values)
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
            self.queue.close();
        }
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

    /// Pop a value from the queue (non-blocking) using supplied Selector
    ///
    /// Returns immediately with Empty if no items are available.
    ///
    /// # Safety
    ///
    /// This method may not be called concurrently from multiple threads.
    pub fn try_pop(&mut self) -> Result<T, PopError> {
        let mut slot = MaybeUninit::<T>::uninit();
        let slice = unsafe { std::slice::from_raw_parts_mut(slot.as_mut_ptr(), 1) };

        let drained = self.try_pop_n(slice);
        if drained == 0 {
            if self.is_closed() && self.inner.producer_count.load(Ordering::Acquire) == 0 {
                Err(PopError::Closed)
            } else {
                Err(PopError::Empty)
            }
        } else {
            Ok(unsafe { slot.assume_init() })
        }
    }

    // /// Drain up to max_count items from the queue with a provided selector
    // ///
    // /// This method efficiently drains items from multiple producer queues, automatically
    // /// cleaning up producer queues that are empty and closed.
    // ///
    // /// Returns the total number of items drained across all producer queues.
    // ///
    // /// # Multi-Consumer Safety
    // ///
    // /// This method IS safe to call concurrently from multiple consumer threads.
    // /// The implementation uses atomic signal acquisition and CAS-based draining to coordinate.
    // pub fn consume_in_place<F>(&mut self, mut f: F, max_count: usize) -> usize
    // where
    //     F: FnMut(T),
    // {
    //     let mut total_drained = 0;
    //     let mut remaining = max_count;

    //     // Try to drain from each producer queue using the selector for fairness
    //     // We'll cycle through producers until we've drained max_count items or all queues are empty
    //     let mut consecutive_empty = 0;

    //     for _ in 0..8 {
    //         // If we've checked all producers and found them all empty, we're done
    //         if consecutive_empty >= MAX_QUEUES {
    //             break;
    //         }

    //         // Get signal index from selector for fairness
    //         let mut producer_id = (self.next() as usize) & MAX_QUEUES_MASK;
    //         let signal_index = producer_id / 64;
    //         let mut signal_bit = (producer_id - (signal_index * 64)) as u64;

    //         let signal = &self.inner.signals[signal_index];
    //         let signal_value = signal.load(Ordering::Acquire);

    //         // Any signals?
    //         if signal_value == 0 {
    //             consecutive_empty += 1;
    //             continue;
    //         }

    //         // Find nearest set bit for fairness
    //         signal_bit = find_nearest(signal_value, signal_bit);

    //         if signal_bit >= 64 {
    //             consecutive_empty += 1;
    //             continue;
    //         }

    //         producer_id = signal_index * 64 + (signal_bit as usize);

    //         // Atomically acquire the bit
    //         let (bit, expected, acquired) = signal.try_acquire(signal_bit);

    //         if !acquired {
    //             // Contention - try next
    //             std::hint::spin_loop();
    //             continue;
    //         }

    //         // Is the signal empty?
    //         let empty = expected == bit;

    //         if empty {
    //             self.inner
    //                 .waker
    //                 .try_unmark_if_empty(signal.index(), signal.value());
    //         }

    //         // Load the queue pointer atomically
    //         let queue_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
    //         if queue_ptr.is_null() {
    //             consecutive_empty += 1;
    //             continue;
    //         }

    //         // SAFETY: The pointer is valid and we have exclusive consumer access
    //         let queue = unsafe { &*queue_ptr };

    //         // Drain from this producer's queue using consume_in_place
    //         let drained = queue.consume_in_place(remaining, |chunk| {
    //             for item in chunk {
    //                 f(*item);
    //             }
    //             chunk.len()
    //         });

    //         total_drained += drained;
    //         remaining -= drained;

    //         // Handle cleanup and rescheduling
    //         let is_empty = queue.is_empty();
    //         let can_dispose = self.is_closed() && is_empty;

    //         if can_dispose {
    //             // Queue is empty and closed - clean up the slot
    //             let old_ptr =
    //                 self.inner.queues[producer_id].swap(ptr::null_mut(), Ordering::AcqRel);

    //             if !old_ptr.is_null() {
    //                 // Decrement producer count
    //                 self.inner.producer_count.fetch_sub(1, Ordering::Relaxed);
    //                 self.inner.queue_count.fetch_sub(1, Ordering::Relaxed);

    //                 unsafe {
    //                     Arc::from_raw(old_ptr);
    //                 }
    //             }

    //             consecutive_empty += 1;
    //         } else if drained > 0 && !is_empty {
    //             // Queue still has items - reschedule
    //             let prev = signal.set_with_bit(bit);
    //             if prev == 0 && !empty {
    //                 self.inner.waker.mark_active(signal.index());
    //                 // self.inner.waker.increment();
    //             }
    //         } else if drained == 0 {
    //             // Queue is empty
    //             consecutive_empty += 1;
    //         }
    //     }

    //     total_drained
    // }

    pub fn try_pop_n(&mut self, batch: &mut [T]) -> usize {
        let total_drained = 0;
        let max_count = batch.len();

        for _ in 0..64 {
            match self.acquire() {
                Some((producer_id, queue)) => {
                    // Drain from this producer's queue using consume_in_place
                    match queue.try_pop_n(batch) {
                        Ok(size) => {
                            // total_drained += size;
                            // remaining -= size;
                            queue.unmark_and_schedule();
                            return size;
                        }
                        Err(PopError::Closed) => {
                            queue.unmark();
                            // Queue is empty and closed - clean up the slot
                            let old_ptr = self.inner.queues[producer_id]
                                .swap(ptr::null_mut(), Ordering::AcqRel);

                            if !old_ptr.is_null() {
                                // Decrement producer count
                                self.inner.producer_count.fetch_sub(1, Ordering::Relaxed);
                                self.inner.queue_count.fetch_sub(1, Ordering::Relaxed);

                                unsafe {
                                    Arc::from_raw(old_ptr);
                                }
                            }
                        }
                        Err(_) => {
                            queue.unmark();
                            // Queue is empty
                            self.misses += 1;
                        }
                    };
                }
                None => {
                    // std::hint::spin_loop();
                }
            }
        }

        total_drained
    }

    fn acquire<'a>(&mut self) -> Option<(usize, &'a SegSpsc<T, P, NUM_SEGS_P2>)> {
        let random = self.next() as usize;
        // Try selecting signal index from summary hint
        // let thread_id: u64 = std::thread::current().id().as_u64().into();
        // let mut signal_index = self.inner.waker.summary_select((thread_id as u64) & 63) as usize;
        let mut signal_index = self.inner.waker.summary_select((random as u64) & 63) as usize;
        // let mut signal_index = (thread_id as usize) & 3;
        // let mut signal_index = 64;
        // let mut signal_index = 0;

        if signal_index >= 64 {
            signal_index = random & 63;
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
                .waker
                .try_unmark_if_empty(signal.index(), signal.value());
        }

        // Compute producer id
        let producer_id = signal_index * 64 + (signal_bit as usize);

        // Load the queue pointer atomically
        let queue_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
        if queue_ptr.is_null() {
            self.misses += 1;
            if empty {
                self.inner
                    .waker
                    .try_unmark_if_empty(signal.index(), signal.value());
            }
            return None;
        }

        // SAFETY: The pointer is valid and we have exclusive consumer access
        let queue = unsafe { &*queue_ptr };

        // Mark as EXECUTING
        queue.mark();

        Some((producer_id, queue))
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

unsafe impl<T: Send, const P: usize, const NUM_SEGS_P2: usize> Send for Sender<T, P, NUM_SEGS_P2> {}
unsafe impl<T: Send, const P: usize, const NUM_SEGS_P2: usize> Send
    for Receiver<T, P, NUM_SEGS_P2>
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

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
}
