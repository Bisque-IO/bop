use super::task::signal::{SIGNAL_MASK, Signal, SignalGate};
use super::worker::waker::{STATUS_SUMMARY_WORDS, WorkerWaker};
use crate::utils::CachePadded;
use crate::utils::bits::find_nearest;
use crate::{PopError, PushError};
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use std::thread;

use crate::detail::spsc::{UnboundedSender, UnboundedSpsc};
use rand::RngCore;

/// Segment size power (1024 items per segment)
const P: usize = 10;
/// Number of segments power (256 segments)
const NUM_SEGS_P2: usize = 8;

/// Create a new blocking MPSC queue
pub fn new<T>() -> Receiver<T> {
    new_with_waker(Arc::new(WorkerWaker::new()))
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
pub fn new_with_waker<T>(
    waker: Arc<WorkerWaker>,
) -> Receiver<T> {
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

pub fn new_with_sender<T>() -> (Sender<T>, Receiver<T>) {
    let waker = Arc::new(WorkerWaker::new());
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

/// Maximum number of producers (limited by SignalWaker summary capacity)
const MAX_QUEUES: usize = STATUS_SUMMARY_WORDS * 64;
const QUEUES_PER_PRODUCER: usize = 1;
const MAX_PRODUCERS: usize = MAX_QUEUES / QUEUES_PER_PRODUCER;
const MAX_PRODUCERS_MASK: usize = QUEUES_PER_PRODUCER - 1;

/// Number of u64 words needed for the signal bitset
const SIGNAL_WORDS: usize = STATUS_SUMMARY_WORDS;

/// Thread-local cache of SPSC queues for each producer thread
/// Stores (producer_id, queue_ptr, close_fn) where close_fn can close the queue
type CloseFn = Box<dyn FnOnce()>;

/// The shared state of the blocking MPSC queue
struct Inner<T> {
    /// Sparse array of producer queues - always MAX_PRODUCERS size
    /// Each slot is an AtomicPtr for thread-safe registration and access
    queues: Box<[AtomicPtr<UnboundedSpsc<T, P, NUM_SEGS_P2, Arc<SignalGate>>>]>,
    queue_count: CachePadded<AtomicUsize>,
    /// Number of registered producers
    producer_count: CachePadded<AtomicUsize>,
    max_producer_id: AtomicUsize,
    /// Closed flag
    closed: CachePadded<AtomicBool>,
    waker: Arc<WorkerWaker>,
    /// Signal bitset - one bit per producer indicating which queues has data
    signals: Arc<[Signal; SIGNAL_WORDS]>,
}

impl<T> Inner<T> {
    /// Check if the queue is closed
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Get the number of registered producers
    pub fn producer_count(&self) -> usize {
        self.producer_count.load(Ordering::Relaxed)
    }

    pub fn create_sender(self: &Arc<Self>) -> Result<Sender<T>, PushError<()>> {
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
    ) -> Result<Sender<T>, PushError<()>> {
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
        let mut sender: Option<UnboundedSender<T, P, NUM_SEGS_P2, Arc<SignalGate>>> = None;

        for signal_index in 0..SIGNAL_WORDS {
            for bit_index in 0..64 {
                let queue_index = signal_index * 64 + bit_index;
                if queue_index >= MAX_QUEUES {
                    break;
                }
                if !self.queues[queue_index].load(Ordering::Acquire).is_null() {
                    continue;
                }

                let signal_gate = Arc::new(SignalGate::new(
                    bit_index as u8,
                    self.signals[signal_index].clone(),
                    Arc::clone(&self.waker),
                ));

                let (tx, _rx) =
                    UnboundedSpsc::<T, P, NUM_SEGS_P2, Arc<SignalGate>>::new_with_signal(
                        signal_gate,
                    );

                // Get the Arc pointer to the UnboundedSpsc from the sender
                let unbounded_arc = tx.unbounded_arc();
                let raw = Arc::into_raw(unbounded_arc)
                    as *mut UnboundedSpsc<T, P, NUM_SEGS_P2, Arc<SignalGate>>;

                match self.queues[queue_index].compare_exchange(
                    ptr::null_mut(),
                    raw,
                    Ordering::Release,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        self.queue_count.fetch_add(1, Ordering::Relaxed);
                        assigned_id = Some(queue_index);
                        sender = Some(tx);
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

        let sender = sender.expect("sender missing");

        Ok(Sender {
            inner: Arc::clone(&self),
            sender,
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
pub struct Sender<T> {
    inner: Arc<Inner<T>>,
    sender: UnboundedSender<T, P, NUM_SEGS_P2, Arc<SignalGate>>,
    producer_id: usize,
}

impl<T> Sender<T> {
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
    /// This will use the unbounded SPSC queue for this producer.
    /// Never blocks since the queue is unbounded.
    pub fn try_push(&mut self, value: T) -> Result<(), PushError<T>> {
        self.sender.try_push(value)
    }

    /// Push multiple values in bulk
    pub fn try_push_n(&mut self, values: &mut Vec<T>) -> Result<usize, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }
        self.sender.try_push_n(values)
    }

    /// Push a value onto the queue
    ///
    /// This will use the unbounded SPSC queue for this producer.
    pub unsafe fn unsafe_try_push(&self, value: T) -> Result<(), PushError<T>> {
        self.sender.try_push(value)
    }

    /// Push multiple values in bulk
    pub unsafe fn unsafe_try_push_n(&self, values: &mut Vec<T>) -> Result<usize, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }
        self.sender.try_push_n(values)
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

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.inner.create_sender().expect("too many senders")
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.sender.close_channel();
    }
}

pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
    misses: u64,
    seed: u64,
}

impl<T> Receiver<T> {
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

    pub fn create_sender(&self) -> Result<Sender<T>, PushError<()>> {
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
    ) -> Result<Sender<T>, PushError<()>> {
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
        self.try_pop_n_with_producer(batch).0
    }

    /// Attempts to pop a single value and returns the item along with the producer id.
    pub fn try_pop_with_id(&mut self) -> Result<(T, usize), PopError> {
        let mut slot = MaybeUninit::<T>::uninit();
        let slice = unsafe { std::slice::from_raw_parts_mut(slot.as_mut_ptr(), 1) };
        let (drained, producer_id) = self.try_pop_n_with_producer(slice);
        if drained == 0 {
            if self.is_closed() && self.inner.producer_count.load(Ordering::Acquire) == 0 {
                Err(PopError::Closed)
            } else {
                Err(PopError::Empty)
            }
        } else {
            debug_assert!(producer_id.is_some());
            let value = unsafe { slot.assume_init() };
            Ok((
                value,
                producer_id.expect("producer id missing for drained item"),
            ))
        }
    }

    fn try_pop_n_with_producer(&mut self, batch: &mut [T]) -> (usize, Option<usize>) {
        for _ in 0..64 {
            match self.acquire() {
                Some((producer_id, queue)) => {
                    match queue.try_pop_n(batch) {
                        Ok(size) => {
                            queue.unmark_and_schedule();
                            return (size, Some(producer_id));
                        }
                        Err(PopError::Closed) => {
                            queue.unmark();
                            let old_ptr = self.inner.queues[producer_id]
                                .swap(ptr::null_mut(), Ordering::AcqRel);

                            if !old_ptr.is_null() {
                                self.inner.producer_count.fetch_sub(1, Ordering::Relaxed);
                                self.inner.queue_count.fetch_sub(1, Ordering::Relaxed);

                                unsafe {
                                    Arc::from_raw(old_ptr);
                                }
                            }
                        }
                        Err(_) => {
                            queue.unmark();
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

    fn acquire(
        &mut self,
    ) -> Option<(
        usize,
        crate::detail::spsc::UnboundedReceiver<T, P, NUM_SEGS_P2, Arc<SignalGate>>,
    )> {
        let random = self.next() as usize;
        // Try selecting signal index from summary hint
        let random_word = random % SIGNAL_WORDS;
        let mut signal_index = self.inner.waker.summary_select(random_word as u64) as usize;

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
        let unbounded_arc: std::sync::Arc<UnboundedSpsc<T, P, NUM_SEGS_P2, Arc<SignalGate>>> =
            unsafe { Arc::from_raw(queue_ptr) };
        let receiver = unbounded_arc.create_receiver();

        // Mark as EXECUTING
        receiver.mark();

        // Don't drop the Arc - we're just borrowing it
        let _ = Arc::into_raw(unbounded_arc);

        Some((producer_id, receiver))
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.inner.close();
    }
}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        self.close();
    }
}

unsafe impl<T: Send> Send for Sender<T> {}
unsafe impl<T: Send> Send for Receiver<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn try_pop_drains_and_reports_closed() {
        let (mut tx, mut rx) = new_with_sender::<u64>();

        tx.try_push(42).unwrap();
        assert_eq!(rx.try_pop().unwrap(), 42);
        assert_eq!(rx.try_pop(), Err(PopError::Empty));

        assert!(rx.close());
        assert_eq!(rx.try_pop(), Err(PopError::Closed));
    }

    #[test]
    fn dropping_local_sender_clears_producer_slot() {
        let (tx, rx) = new_with_sender::<u64>();
        assert_eq!(tx.producer_count(), 1);

        drop(tx);

        // Closing will walk the slots and remove the dropped sender.
        assert!(rx.close());
        assert_eq!(rx.producer_count(), 0);
        assert_eq!(rx.inner.queue_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn single_producer_multiple_items() {
        let (mut tx, mut rx) = new_with_sender::<u64>();

        // Push multiple items
        for i in 0..100 {
            tx.try_push(i)
                .expect("push should not fail for unbounded queue");
        }

        // Pop and verify
        for i in 0..100 {
            assert_eq!(rx.try_pop().unwrap(), i);
        }

        // Queue should be empty
        assert_eq!(rx.try_pop(), Err(PopError::Empty));
    }

    #[test]
    fn multiple_producers_single_consumer() {
        use std::thread;

        let (tx, mut rx) = new_with_sender::<u64>();
        let mut received = vec![false; 30]; // Track which items we received

        // Spawn 3 producer threads, each pushing 10 items
        let handles: Vec<_> = (0..3)
            .map(|producer_id| {
                let tx = tx.clone();
                thread::spawn(move || {
                    for i in 0..10 {
                        let value = (producer_id * 10 + i) as u64;
                        tx.clone().try_push(value).expect("push should succeed");
                    }
                })
            })
            .collect();

        // Wait for all producers to finish
        for handle in handles {
            handle.join().unwrap();
        }

        // Consume all items and verify they're in range
        let mut count = 0;
        for _ in 0..100 {
            if let Ok(value) = rx.try_pop() {
                assert!(value < 30, "received unexpected value: {}", value);
                received[value as usize] = true;
                count += 1;
            } else {
                break;
            }
        }

        // Should have received all 30 items
        assert_eq!(count, 30, "expected 30 items, got {}", count);
        for (i, &received_item) in received.iter().enumerate() {
            assert!(received_item, "item {} was not received", i);
        }
    }

    #[test]
    fn unbounded_growth_with_large_batches() {
        let (mut tx, mut rx) = new_with_sender::<u64>();

        // Push many items, causing growth
        let mut items_to_push: Vec<u64> = (0..1000).collect();
        let pushed = tx
            .try_push_n(&mut items_to_push)
            .expect("bulk push should succeed");
        assert_eq!(pushed, 1000, "should push all items");
        assert!(items_to_push.is_empty(), "Vec should be drained");

        // Verify all items are received
        for i in 0..1000 {
            assert_eq!(rx.try_pop().unwrap(), i as u64);
        }

        assert_eq!(rx.try_pop(), Err(PopError::Empty));
    }

    #[test]
    fn close_stops_receives() {
        let (mut tx, mut rx) = new_with_sender::<u64>();

        tx.try_push(42).unwrap();
        assert_eq!(rx.try_pop().unwrap(), 42);

        // Close the queue
        rx.close();

        // No more receives should succeed
        assert_eq!(rx.try_pop(), Err(PopError::Closed));

        // Tries to push should fail
        assert_eq!(tx.try_push(100), Err(PushError::Closed(100)));
    }

    #[test]
    fn multiple_senders_cloning() {
        let (tx, mut rx) = new_with_sender::<u64>();
        assert_eq!(tx.producer_count(), 1);

        let mut tx2 = tx.clone();
        assert_eq!(tx2.producer_count(), 2);

        let mut tx3 = tx.clone();
        assert_eq!(tx3.producer_count(), 3);

        // All clones can push
        drop(tx);
        tx2.try_push(1).unwrap();
        tx3.try_push(2).unwrap();

        // Collect items (order may vary with MPSC)
        let mut items = Vec::new();
        for _ in 0..2 {
            if let Ok(val) = rx.try_pop() {
                items.push(val);
            }
        }

        items.sort();
        assert_eq!(items, vec![1, 2]);
    }

    #[test]
    fn interleaved_push_pop() {
        let (mut tx, mut rx) = new_with_sender::<u64>();

        // Push and pop interleaved
        let mut received = Vec::new();
        for i in 0..10 {
            tx.try_push(i * 2).unwrap();
            tx.try_push(i * 2 + 1).unwrap();
            // Try to pop immediately - may or may not get anything
            while let Ok(value) = rx.try_pop() {
                received.push(value);
            }
        }

        // Pop remaining items
        while let Ok(value) = rx.try_pop() {
            received.push(value);
        }

        // Verify we got all 20 items
        assert_eq!(received.len(), 20);
        received.sort();
        for i in 0..20 {
            assert_eq!(received[i], i as u64);
        }

        assert_eq!(rx.try_pop(), Err(PopError::Empty));
    }

    #[test]
    fn batch_push_pop() {
        let (mut tx, mut rx) = new_with_sender::<u64>();

        // Push batch
        let mut items: Vec<u64> = (0..50).collect();
        let pushed = tx.try_push_n(&mut items).expect("bulk push should succeed");
        assert_eq!(pushed, 50);
        assert!(items.is_empty());

        // Pop all items
        let mut dst = [0u64; 100];
        let popped = rx.try_pop_n(&mut dst);
        assert_eq!(popped, 50);

        for i in 0..50 {
            assert_eq!(dst[i], i as u64);
        }
    }

    #[test]
    fn concurrent_push_pop() {
        use std::sync::Arc;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
        use std::thread;

        let (tx, rx) = new_with_sender::<u64>();
        let counter = Arc::new(AtomicUsize::new(0));

        let rx = Arc::new(Mutex::new(rx));
        let rx_clone = Arc::clone(&rx);
        let counter_clone = Arc::clone(&counter);

        // Producer thread
        let producer = thread::spawn(move || {
            let mut tx = tx;
            for i in 0..1000 {
                tx.try_push(i).expect("push should succeed");
                thread::yield_now();
            }
        });

        // Consumer thread
        let consumer = thread::spawn(move || {
            let mut rx = rx_clone.lock().unwrap();
            let mut count: usize = 0;
            for _ in 0..10000 {
                if let Ok(value) = rx.try_pop() {
                    assert_eq!(value, count as u64);
                    count += 1;
                    if count >= 1000 {
                        break;
                    }
                } else {
                    thread::yield_now();
                }
            }
            counter_clone.fetch_add(count, AtomicOrdering::Relaxed);
        });

        producer.join().unwrap();
        consumer.join().unwrap();

        let final_count = counter.load(AtomicOrdering::Relaxed);
        assert_eq!(final_count, 1000, "should have received all 1000 items");
    }

    #[test]
    fn stress_test_unbounded_expansion() {
        let (mut tx, mut rx) = new_with_sender::<u64>();

        // Push 10000 items to force significant expansion
        let mut items: Vec<u64> = (0..10000).collect();
        let pushed = tx.try_push_n(&mut items).expect("bulk push should succeed");
        assert_eq!(pushed, 10000);

        // Verify all items in order
        for i in 0..10000 {
            assert_eq!(rx.try_pop().unwrap(), i as u64, "item {} mismatch", i);
        }

        assert_eq!(rx.try_pop(), Err(PopError::Empty));
    }

    #[test]
    fn producer_id_uniqueness() {
        let (tx1, _rx) = new_with_sender::<u64>();
        let tx2 = tx1.clone();
        let tx3 = tx1.clone();

        // All should have different producer IDs
        assert_ne!(tx1.producer_id(), tx2.producer_id());
        assert_ne!(tx2.producer_id(), tx3.producer_id());
        assert_ne!(tx1.producer_id(), tx3.producer_id());
    }

    #[test]
    fn receiver_count_tracking() {
        let (tx1, rx) = new_with_sender::<u64>();
        assert_eq!(rx.producer_count(), 1);

        let tx2 = tx1.clone();
        assert_eq!(rx.producer_count(), 2);

        let tx3 = tx2.clone();
        assert_eq!(rx.producer_count(), 3);

        drop(tx1);
        rx.close(); // Trigger cleanup

        // After closing, all producers should be cleaned up
        assert_eq!(rx.producer_count(), 0);
    }
}
