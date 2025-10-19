use crate::bits::find_nearest;
use crate::seg_spsc_dynamic::SegSpsc;
use crate::selector::Selector;
use crate::signal::{SIGNAL_MASK, Signal, SignalGate};
use crate::signal_waker::SignalWaker;
use crate::{PopError, PushError};
use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::thread;

use rand::RngCore;

/// Create a new blocking MPSC queue
pub fn new<T: Copy>(seg_shift: usize, num_segs_shift: usize) -> (Sender<T>, Receiver<T>) {
    let waker = Arc::new(SignalWaker::new());
    // Create sparse array of AtomicPtr, all initialized to null
    let queues: [AtomicPtr<SegSpsc<T>>; MAX_QUEUES] =
        std::array::from_fn(|_| AtomicPtr::new(ptr::null_mut()));

    let signals: Arc<[Signal; SIGNAL_WORDS]> =
        Arc::new(std::array::from_fn(|i| Signal::with_index(i as u64)));

    let inner = Arc::new(Inner {
        queues,
        queue_count: AtomicUsize::new(0),
        producer_count: AtomicUsize::new(0),
        closed: AtomicPtr::new(ptr::null_mut()),
        waker: waker,
        signals,
        seg_shift,
        num_segs_shift,
    });

    (
        Sender {
            inner: Arc::clone(&inner),
        },
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

/// Producer handle that provides direct push-only access to a SPSC queue
///
/// This handle bypasses all thread-local caching and provides the fastest
/// possible push interface for scenarios where you need high-performance
/// direct access to a specific producer queue.
pub struct LocalSender<T: Copy> {
    _handle: Arc<Inner<T>>,
    producer: *const SegSpsc<T>,
    producer_id: usize,
}

impl<T: Copy> LocalSender<T> {
    /// Push a value onto the queue
    ///
    /// This is the fastest push operation available as it bypasses all
    /// thread-local caching and directly accesses the SPSC queue.
    pub fn try_push(&self, value: T) -> Result<(), PushError<T>> {
        unsafe { &*self.producer }.try_push(value)
    }

    /// Push multiple values in bulk
    ///
    /// Direct bulk push without any thread-local overhead.
    pub fn try_push_n(&self, values: &[T]) -> Result<usize, PushError<()>> {
        unsafe { &*self.producer }.try_push_n(values)
    }

    /// Get the producer ID for this handle
    pub fn producer_id(&self) -> usize {
        self.producer_id
    }
}

impl<T: Copy> Drop for LocalSender<T> {
    fn drop(&mut self) {
        unsafe {
            // Closes the queue and schedules for a worker to cleanup.
            (&*self.producer).close();
        }
    }
}

struct TLSHot {
    key: usize,
    producer_id: usize,
    queue_ptr: *const (),
}

impl TLSHot {
    fn new() -> Self {
        Self {
            key: usize::MAX, // Sentinel for empty
            producer_id: 0,
            queue_ptr: ptr::null(),
        }
    }

    fn is_match(&self, key: usize) -> bool {
        self.key == key
    }

    fn get(&self, key: usize) -> Option<(usize, *const ())> {
        if self.is_match(key) {
            Some((self.producer_id, self.queue_ptr))
        } else {
            None
        }
    }

    fn update(&mut self, key: usize, producer_id: usize, queue_ptr: *const ()) {
        self.key = key;
        self.producer_id = producer_id;
        self.queue_ptr = queue_ptr;
    }
}

struct TLS {
    // Box<HashMap> optimization benefits:
    // 1. Smaller QueueCache struct (8-byte pointer vs large embedded HashMap)
    // 2. Better cache locality for thread_local storage
    // 3. Heap allocation only when HashMap is actually used
    entries: Box<std::collections::HashMap<usize, (usize, *const (), CloseFn)>>,
}

impl TLS {
    fn new() -> Self {
        Self {
            entries: Box::new(std::collections::HashMap::new()),
        }
    }
}

impl Drop for TLS {
    fn drop(&mut self) {
        // Producer thread exiting - close all its queues and drop TLS references
        for (_, (_, queue_ptr, close_fn)) in self.entries.drain() {
            if !queue_ptr.is_null() {
                // Call the close function which knows the concrete type
                close_fn();
            }
        }
    }
}

thread_local! {
    static QUEUES: UnsafeCell<TLS> = UnsafeCell::new(TLS::new());
}
thread_local! {
    static HOT_QUEUE: UnsafeCell<TLSHot> = UnsafeCell::new(TLSHot::new());
}

/// The shared state of the blocking MPSC queue
struct Inner<T: Copy> {
    /// Sparse array of producer queues - always MAX_PRODUCERS size
    /// Each slot is an AtomicPtr for thread-safe registration and access
    queues: [AtomicPtr<SegSpsc<T>>; MAX_QUEUES],
    queue_count: AtomicUsize,
    /// Number of registered producers
    producer_count: AtomicUsize,
    /// Closed flag
    closed: AtomicPtr<()>,
    waker: Arc<SignalWaker>,
    /// Signal bitset - one bit per producer indicating which queues has data
    signals: Arc<[Signal; SIGNAL_WORDS]>,

    seg_shift: usize,
    num_segs_shift: usize,
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
/// ```
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
/// ```
pub struct Sender<T: Copy> {
    inner: Arc<Inner<T>>,
}

impl<T: 'static + Copy> Sender<T> {
    /// Check if the queue is closed
    pub fn is_closed(&self) -> bool {
        !self.inner.closed.load(Ordering::Acquire).is_null()
    }

    /// Get the number of registered producers
    pub fn producer_count(&self) -> usize {
        self.inner.producer_count.load(Ordering::Relaxed)
    }

    /// Get or create the thread-local SPSC queue for this producer
    pub fn get_local_producer(&self) -> Result<(*const SegSpsc<T>, usize), PushError<()>> {
        self.with_local_producer(|producer, producer_id| {
            (producer as *const SegSpsc<T>, producer_id)
        })
    }

    /// Execute an operation with the thread-local SPSC queue for this producer
    ///
    /// This is a faster alternative to `get_producer_queue` that avoids cloning the Arc.
    /// The closure receives a reference to the queue and the producer ID.
    ///
    /// # Safety
    ///
    /// The closure should not store the queue reference beyond its execution.
    pub fn with_local_producer<F, R>(&self, f: F) -> Result<R, PushError<()>>
    where
        F: FnOnce(&SegSpsc<T>, usize) -> R,
    {
        let inner_ptr = Arc::as_ptr(&self.inner) as usize;

        // Check hot entry first (fastest path)
        let hot_result = HOT_QUEUE.with(|hot_entry| {
            let hot = unsafe { &*hot_entry.get() };
            hot.get(inner_ptr)
        });

        if let Some((producer_id, queue_ptr)) = hot_result {
            // Hot entry hit - reconstruct Arc and execute closure
            let queue_ptr = queue_ptr as *mut SegSpsc<T>;
            let queue_arc = unsafe { ManuallyDrop::new(Box::from_raw(queue_ptr)) };

            // Execute closure with reference to avoid clone
            let result = f(&queue_arc, producer_id);

            // Note: ManuallyDrop ensures we don't double-drop when Arc goes out of scope
            return Ok(result);
        }

        // Hot entry miss - check full cache
        QUEUES.with(|queues| {
            let cache = unsafe { &mut *queues.get() };

            if let Some(&(producer_id, queue_ptr, _)) = cache.entries.get(&inner_ptr) {
                // Promote to hot entry for next time
                HOT_QUEUE.with(|hot_entry| {
                    let hot = unsafe { &mut *hot_entry.get() };
                    hot.update(inner_ptr, producer_id, queue_ptr);
                });

                // Reconstruct Arc from raw pointer without cloning
                let queue_ptr = queue_ptr as *mut SegSpsc<T>;
                let queue_arc = unsafe { ManuallyDrop::new(Box::from_raw(queue_ptr)) };

                // Execute closure with reference to avoid clone
                let result = f(&queue_arc, producer_id);

                // Note: ManuallyDrop ensures we don't double-drop when Arc goes out of scope
                Ok(result)
            } else {
                loop {
                    let producer_count = self.inner.producer_count.load(Ordering::SeqCst);
                    if producer_count + 1 >= MAX_PRODUCERS {
                        return Err(PushError::Full(()));
                    }
                    if self
                        .inner
                        .producer_count
                        .compare_exchange(
                            producer_count,
                            producer_count + 1,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        break;
                    }
                }

                let mut producer_id = None;
                let mut queue: Option<*mut SegSpsc<T>> = None;
                let max_queues = MAX_QUEUES;

                'outer: for bit_index in 0..64 {
                    for signal_index in 0..max_queues / 64 {
                        let queue_index = (signal_index * 64) + bit_index;
                        if self.inner.queues[queue_index]
                            .load(Ordering::Acquire)
                            .is_null()
                        {
                            // Create new queue (SegSpsc doesn't need signals/waker)
                            let new_queue = Box::into_raw(Box::new(unsafe {
                                SegSpsc::<T>::new_unsafe_with_gate(
                                    self.inner.seg_shift,
                                    self.inner.num_segs_shift,
                                    SignalGate::new(
                                        bit_index as u64,
                                        self.inner.signals[signal_index].clone(),
                                        Arc::clone(&self.inner.waker),
                                    ),
                                )
                            }));

                            let queue_raw_ptr = new_queue as *mut SegSpsc<T>;

                            // Try to store atomically
                            match self.inner.queues[queue_index].compare_exchange(
                                ptr::null_mut(),
                                queue_raw_ptr,
                                Ordering::Release,
                                Ordering::Acquire,
                            ) {
                                Ok(_) => {
                                    eprintln!("registered producer at: {}", queue_index);
                                    // Successfully registered
                                    self.inner.producer_count.fetch_add(1, Ordering::Relaxed);
                                    producer_id = Some(queue_index);
                                    queue = Some(new_queue);
                                    break 'outer;
                                }
                                Err(_) => {
                                    // Race condition: another thread registered here
                                    unsafe { drop(Box::from_raw(queue_raw_ptr)) };
                                    continue;
                                }
                            }
                        }
                    }
                }

                if producer_id.is_none() {
                    self.inner.producer_count.fetch_sub(1, Ordering::Release);
                    return Err(PushError::Full(()));
                }

                let producer_id = producer_id.expect("max queues exceeded");
                let queue = queue.expect("max queues exceeded");

                // SegSpsc doesn't have a close() method, so we don't need a close_fn
                let close_fn: CloseFn = Box::new(move || {
                    // No-op: SegSpsc cleanup happens automatically on drop
                });

                cache
                    .entries
                    .insert(inner_ptr, (producer_id, queue as *const (), close_fn));

                // Update hot entry with new queue
                HOT_QUEUE.with(|hot_entry| {
                    let hot = unsafe { &mut *hot_entry.get() };
                    hot.update(inner_ptr, producer_id, queue as *const ());
                });

                // Execute closure with reference to avoid final clone
                Ok(f(unsafe { &*queue }, producer_id))
            }
        })
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
    /// ```rust
    /// let mpsc = MpscBlocking::<i32, 64>::new();
    ///
    /// // Create a direct producer handle
    /// let producer = mpsc.create_producer_handle().unwrap();
    ///
    /// // Use the handle for high-performance pushes
    /// producer.push(42).unwrap();
    /// producer.push_bulk(&[1, 2, 3]).unwrap();
    /// ```
    pub fn create_sender(&self) -> Result<LocalSender<T>, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }

        loop {
            let producer_count = self.inner.producer_count.load(Ordering::SeqCst);
            if producer_count + 1 >= MAX_PRODUCERS {
                return Err(PushError::Full(()));
            }
            if self
                .inner
                .producer_count
                .compare_exchange(
                    producer_count,
                    producer_count + 1,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_ok()
            {
                break;
            }
        }

        let mut producer_id = None;
        let mut queue: Option<*mut SegSpsc<T>> = None;
        let max_queues = MAX_QUEUES;

        'outer: for bit_index in 0..64 {
            for signal_index in 0..MAX_QUEUES / 64 {
                let queue_index = (signal_index * 64) + bit_index;
                if self.inner.queues[queue_index]
                    .load(Ordering::Acquire)
                    .is_null()
                {
                    // Create new queue
                    let new_queue = Box::into_raw(Box::new(unsafe {
                        SegSpsc::<T>::new_unsafe_with_gate(
                            self.inner.seg_shift,
                            self.inner.num_segs_shift,
                            SignalGate::new(
                                bit_index as u64,
                                self.inner.signals[signal_index].clone(),
                                Arc::clone(&self.inner.waker),
                            ),
                        )
                    }));

                    let queue_raw_ptr = new_queue as *mut SegSpsc<T>;

                    // Try to store atomically
                    match self.inner.queues[queue_index].compare_exchange(
                        ptr::null_mut(),
                        queue_raw_ptr,
                        Ordering::Release,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            // println!("registered queue at: {}-{}", producer_id, queue_index);
                            // Successfully registered
                            self.inner.queue_count.fetch_add(1, Ordering::Relaxed);
                            producer_id = Some(queue_index);
                            queue = Some(queue_raw_ptr);
                            break 'outer;
                        }
                        Err(_) => {
                            // Race condition: another thread registered here
                            unsafe { drop(Box::from_raw(queue_raw_ptr)) };
                            continue;
                        }
                    }
                }
            }
        }

        if producer_id.is_none() || queue.is_none() {
            self.inner.producer_count.fetch_sub(1, Ordering::Release);
            return Err(PushError::Full(()));
        }

        Ok(LocalSender {
            _handle: Arc::clone(&self.inner),
            producer: queue.unwrap(),
            producer_id: producer_id.unwrap(),
        })
    }

    /// Push a value onto the queue
    ///
    /// This will use the thread-local SPSC queue for this producer.
    /// If successful, notifies any waiting consumer.
    pub fn try_push(&self, value: T) -> Result<(), PushError<T>> {
        // if self.is_closed() {
        //     return Err(PushError::Closed(value));
        // }
        match self.get_local_producer() {
            Ok((queue, _)) => unsafe { &*queue }.try_push(value),
            Err(PushError::Full(())) => Err(PushError::Full(value)),
            Err(PushError::Closed(())) => Err(PushError::Closed(value)),
        }
    }

    /// Push a value onto the queue (spins if full)
    ///
    /// This will use the thread-local SPSC queue for this producer.
    /// If successful, notifies any waiting consumer.
    pub fn push_spin(&self, value: T) -> Result<(), PushError<T>> {
        // if self.is_closed() {
        //     return Err(PushError::Closed(value));
        // }
        match self.get_local_producer() {
            Ok((queue, _)) => loop {
                if let Ok(()) = unsafe { &*queue }.try_push(value) {
                    return Ok(());
                }
                std::hint::spin_loop();
            },
            Err(PushError::Full(())) => Err(PushError::Full(value)),
            Err(PushError::Closed(())) => Err(PushError::Closed(value)),
        }
    }

    /// Push multiple values in bulk
    pub fn try_push_n(&self, values: &[T]) -> Result<usize, PushError<()>> {
        // if self.is_closed() {
        //     return Err(PushError::Closed(()));
        // }
        match self.get_local_producer() {
            Ok((queue, _)) => unsafe { &*queue }.try_push_n(values),
            Err(err) => Err(err),
        }
    }

    /// Close the queue immediately without waiting
    ///
    /// After closing, no more items can be pushed. Wakes any waiting consumer.
    /// This is a non-blocking close that returns immediately.
    pub fn close_immediate(&self) {
        self.inner.closed.store(1 as *mut (), Ordering::Release);
    }

    /// Close the queue
    ///
    /// After closing, no more items can be pushed. This method will block until all
    /// SPSC queues are empty. Wakes any waiting consumer.
    ///
    /// Note: This waits for consumer to drain all items from all producer queues.
    pub fn close(&self) -> bool {
        self.inner.closed.store(1 as *mut (), Ordering::Release);

        for slot in self.inner.queues.iter() {
            let queue_ptr = slot.load(Ordering::Acquire);
            if !queue_ptr.is_null() {
                return false;
            }
        }

        true
    }
}

impl<T: 'static + Copy> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub struct Receiver<T: Copy> {
    inner: Arc<Inner<T>>,
    misses: u64,
    seed: u64,
}

impl<T: 'static + Copy> Receiver<T> {
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
        !self.inner.closed.load(Ordering::Acquire).is_null()
    }

    /// Get the number of registered producers
    pub fn producer_count(&self) -> usize {
        self.inner.producer_count.load(Ordering::Relaxed)
    }

    /// Execute: select a signal using the selector for fairness and atomically acquire it
    /// Returns the producer_id if successful, None if no signals or contention
    #[inline]
    fn execute_pop(&mut self) -> Result<T, PopError> {
        let queue = self.acquire();
        if queue.is_none() {
            return Err(PopError::Empty);
        }
        let (producer_id, queue) = queue.unwrap();

        // Try to pop from the queue
        return match queue.try_pop() {
            Some(value) => {
                queue.unmark_and_schedule();
                Ok(value)
            }
            None => {
                queue.unmark();
                // Queue is empty - check if we should clean it up
                if self.is_closed() && queue.is_empty() {
                    // Queue is closed and empty - clean up the slot
                    let old_ptr =
                        self.inner.queues[producer_id].swap(ptr::null_mut(), Ordering::AcqRel);

                    if !old_ptr.is_null() {
                        // Decrement producer count
                        self.inner.producer_count.fetch_sub(1, Ordering::Relaxed);

                        // Drop the Box (last reference from producers array)
                        unsafe {
                            drop(Box::from_raw(old_ptr));
                        }
                    }
                }
                Err(PopError::Empty)
            }
        };
    }

    /// Pop a value from the queue (non-blocking) using supplied Selector
    ///
    /// Returns immediately with Empty if no items are available.
    ///
    /// # Safety
    ///
    /// This method may not be called concurrently from multiple threads.
    pub fn pop(&mut self) -> Result<T, PopError> {
        self.execute_pop()
    }

    /// Close the queue immediately without waiting
    ///
    /// After closing, no more items can be pushed. Wakes any waiting consumer.
    /// This is a non-blocking close that returns immediately.
    pub fn close_immediate(&mut self) {
        self.inner.closed.store(1 as *mut (), Ordering::Release);
    }

    /// Close the queue
    ///
    /// After closing, no more items can be pushed. This method will block until all
    /// SPSC queues are empty. Wakes any waiting consumer.
    ///
    /// Note: This waits for consumer to drain all items from all producer queues.
    pub fn close(&mut self) -> bool {
        self.inner.closed.store(1 as *mut (), Ordering::Release);

        for slot in self.inner.queues.iter() {
            let queue_ptr = slot.load(Ordering::Acquire);
            if !queue_ptr.is_null() {
                return false;
            }
        }

        true
    }

    /// Drain up to max_count items from the queue with a provided selector
    ///
    /// This method efficiently drains items from multiple producer queues, automatically
    /// cleaning up producer queues that are empty and closed.
    ///
    /// Returns the total number of items drained across all producer queues.
    ///
    /// # Multi-Consumer Safety
    ///
    /// This method IS safe to call concurrently from multiple consumer threads.
    /// The implementation uses atomic signal acquisition and CAS-based draining to coordinate.
    pub fn consume_in_place<F>(&mut self, mut f: F, max_count: usize) -> usize
    where
        F: FnMut(T),
    {
        let mut total_drained = 0;
        let mut remaining = max_count;

        // Try to drain from each producer queue using the selector for fairness
        // We'll cycle through producers until we've drained max_count items or all queues are empty
        let mut consecutive_empty = 0;

        for _ in 0..8 {
            // If we've checked all producers and found them all empty, we're done
            if consecutive_empty >= MAX_QUEUES {
                break;
            }

            // Get signal index from selector for fairness
            let mut producer_id = (self.next() as usize) & MAX_QUEUES_MASK;
            let signal_index = producer_id / 64;
            let mut signal_bit = (producer_id - (signal_index * 64)) as u64;

            let signal = &self.inner.signals[signal_index];
            let signal_value = signal.load(Ordering::Acquire);

            // Any signals?
            if signal_value == 0 {
                consecutive_empty += 1;
                continue;
            }

            // Find nearest set bit for fairness
            signal_bit = find_nearest(signal_value, signal_bit);

            if signal_bit >= 64 {
                consecutive_empty += 1;
                continue;
            }

            producer_id = signal_index * 64 + (signal_bit as usize);

            // Atomically acquire the bit
            let (bit, expected, acquired) = signal.try_acquire(signal_bit);

            if !acquired {
                // Contention - try next
                std::hint::spin_loop();
                continue;
            }

            // Is the signal empty?
            let empty = expected == bit;

            if empty {
                self.inner
                    .waker
                    .try_unmark_if_empty(signal.index(), signal.value());
            }

            // Load the queue pointer atomically
            let queue_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
            if queue_ptr.is_null() {
                consecutive_empty += 1;
                continue;
            }

            // SAFETY: The pointer is valid and we have exclusive consumer access
            let queue = unsafe { &*queue_ptr };

            // Drain from this producer's queue using consume_in_place
            let drained = queue.consume_in_place(remaining, |chunk| {
                for item in chunk {
                    f(*item);
                }
                chunk.len()
            });

            total_drained += drained;
            remaining -= drained;

            // Handle cleanup and rescheduling
            let is_empty = queue.is_empty();
            let can_dispose = self.is_closed() && is_empty;

            if can_dispose {
                // Queue is empty and closed - clean up the slot
                let old_ptr =
                    self.inner.queues[producer_id].swap(ptr::null_mut(), Ordering::AcqRel);

                if !old_ptr.is_null() {
                    // Decrement producer count
                    self.inner.producer_count.fetch_sub(1, Ordering::Relaxed);

                    // Drop the Box (last reference from producers array)
                    unsafe {
                        drop(Box::from_raw(old_ptr));
                    }
                }

                consecutive_empty += 1;
            } else if drained > 0 && !is_empty {
                // Queue still has items - reschedule
                let prev = signal.set_with_bit(bit);
                if prev == 0 && !empty {
                    self.inner.waker.mark_active(signal.index());
                    // self.inner.waker.increment();
                }
            } else if drained == 0 {
                // Queue is empty
                consecutive_empty += 1;
            }
        }

        total_drained
    }

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

                                // Drop the Box (last reference from producers array)
                                unsafe {
                                    drop(Box::from_raw(old_ptr));
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

    fn get_producer_at<'a>(&self, index: usize) -> Option<&'a SegSpsc<T>> {
        let queue_ptr = self.inner.queues[index].load(Ordering::Acquire);
        if queue_ptr.is_null() {
            None
        } else {
            Some(unsafe { &*queue_ptr })
        }
    }

    fn acquire<'a>(&mut self) -> Option<(usize, &'a SegSpsc<T>)> {
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

impl<T: Copy> Drop for Inner<T> {
    fn drop(&mut self) {
        // Clean up all Box<SegSpsc> instances stored as raw pointers
        for slot in self.queues.iter() {
            let queue_ptr = slot.load(Ordering::Acquire);
            if !queue_ptr.is_null() {
                // SAFETY: We own the last reference to MpmcBlockingInner, so we can safely
                // reconstruct and drop the Box
                unsafe {
                    drop(Box::from_raw(queue_ptr));
                }
            }
        }
    }
}

unsafe impl<T: Send + Copy> Send for Sender<T> {}
unsafe impl<T: Send + Copy> Sync for Sender<T> {}

#[cfg(test)]
mod tests {}
