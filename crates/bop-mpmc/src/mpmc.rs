//! # Blocking MPSC Queue
//!
//! A blocking Multi-Producer Single-Consumer queue built from thread-local SPSC queues.
//!
//! This variant provides blocking operations that efficiently wait for items to become
//! available or for space to free up, avoiding busy-waiting while maintaining high performance.
//!
//! ## Design
//!
//! - Same thread-local SPSC architecture as non-blocking MPSC
//! - Consumer can block waiting for items using `pop_blocking()`
//! - Efficient park/unpark mechanism with minimal overhead
//! - Producers notify waiting consumer when pushing items

use crate::bits::find_nearest;
use crate::seg_spsc::SegSpsc;
use crate::selector::Selector;
use crate::signal::Signal;
use crate::{PopError, PushError};

use crate::SignalGate;
use crate::waker::SignalWaker;
use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::thread;

/// Maximum number of producers
const MAX_PRODUCERS: usize = 1024;
const MAX_PRODUCERS_MASK: usize = 1023;

/// Number of u64 words needed for the signal bitset
const SIGNAL_WORDS: usize = (MAX_PRODUCERS + 63) / 64;

/// Thread-local cache of SPSC queues for each producer thread
/// Stores (producer_id, queue_ptr, close_fn) where close_fn can close the queue
type CloseFn = Box<dyn FnOnce()>;

/// Producer handle that provides direct push-only access to a SPSC queue
///
/// This handle bypasses all thread-local caching and provides the fastest
/// possible push interface for scenarios where you need high-performance
/// direct access to a specific producer queue.
pub struct Producer<T: Copy, const P: usize, const NUM_SEGS_P2: usize> {
    queue: *const SegSpsc<T, P, NUM_SEGS_P2>,
    producer_id: usize,
    signal: Arc<Signal>,
    waker: Arc<SignalWaker>,
}

impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> Producer<T, P, NUM_SEGS_P2> {
    /// Push a value onto the queue
    ///
    /// This is the fastest push operation available as it bypasses all
    /// thread-local caching and directly accesses the SPSC queue.
    pub fn push(&self, value: T) -> Result<(), PushError<T>> {
        unsafe { &*self.queue }.try_push(value)
    }

    /// Push multiple values in bulk
    ///
    /// Direct bulk push without any thread-local overhead.
    pub fn push_bulk(&self, values: &[T]) -> Result<usize, PushError<()>> {
        unsafe { &*self.queue }.try_push_n(values)
    }

    /// Get the producer ID for this handle
    pub fn producer_id(&self) -> usize {
        self.producer_id
    }
}

impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> Drop for Producer<T, P, NUM_SEGS_P2> {
    fn drop(&mut self) {
        // Note: SegSpsc doesn't have a close() method
        // Cleanup happens automatically when the SegSpsc is dropped
    }
}

struct HotQueueEntry {
    key: usize,
    producer_id: usize,
    queue_ptr: *const (),
}

impl HotQueueEntry {
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

struct QueueCache {
    // Box<HashMap> optimization benefits:
    // 1. Smaller QueueCache struct (8-byte pointer vs large embedded HashMap)
    // 2. Better cache locality for thread_local storage
    // 3. Heap allocation only when HashMap is actually used
    entries: Box<hashbrown::HashMap<usize, (usize, *const (), CloseFn)>>,
}

impl QueueCache {
    fn new() -> Self {
        Self {
            entries: Box::new(hashbrown::HashMap::new()),
        }
    }
}

impl Drop for QueueCache {
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
    static QUEUES: UnsafeCell<QueueCache> = UnsafeCell::new(QueueCache::new());
}
thread_local! {
    static SELECTORS: UnsafeCell<Selector> = UnsafeCell::new(Selector::new());
}
thread_local! {
    static HOT_QUEUE: UnsafeCell<HotQueueEntry> = UnsafeCell::new(HotQueueEntry::new());
}

/// The shared state of the blocking MPSC queue
struct MpmcBlockingInner<T: Copy, const P: usize, const NUM_SEGS_P2: usize> {
    /// Sparse array of producer queues - always MAX_PRODUCERS size
    /// Each slot is an AtomicPtr for thread-safe registration and access
    producers: [AtomicPtr<SegSpsc<T, P, NUM_SEGS_P2>>; MAX_PRODUCERS],
    /// Number of registered producers
    producer_count: AtomicUsize,
    producer_counter: AtomicUsize,
    /// Closed flag
    closed: AtomicPtr<()>,
    waker: Arc<SignalWaker>,
    /// Signal bitset - one bit per producer indicating which queues has data
    signals: Arc<[Signal; SIGNAL_WORDS]>,
}

/// Blocking MPSC Queue - Multi-Producer Single-Consumer with blocking operations
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
pub struct MpmcBlocking<T: Copy, const P: usize = 8, const NUM_SEGS_P2: usize = 2> {
    inner: Arc<MpmcBlockingInner<T, P, NUM_SEGS_P2>>,
}

impl<T: 'static + Copy, const P: usize, const NUM_SEGS_P2: usize> MpmcBlocking<T, P, NUM_SEGS_P2> {
    /// Create a new blocking MPSC queue
    pub fn new() -> Self {
        let waker = Arc::new(SignalWaker::new());
        // Create sparse array of AtomicPtr, all initialized to null
        let producers: [AtomicPtr<SegSpsc<T, P, NUM_SEGS_P2>>; MAX_PRODUCERS] =
            std::array::from_fn(|_| AtomicPtr::new(ptr::null_mut()));

        // let producer_gates: [SignalGate; MAX_PRODUCERS] = std::array::from_fn(|i| {
        //     SignalGate::new((i % MAX_PRODUCERS) as u64, Signal::new(), waker.clone())
        // });

        let signals: Arc<[Signal; SIGNAL_WORDS]> =
            Arc::new(std::array::from_fn(|i| Signal::with_index(i as u64)));

        Self {
            inner: Arc::new(MpmcBlockingInner {
                producers,
                producer_count: AtomicUsize::new(0),
                producer_counter: AtomicUsize::new(0),
                closed: AtomicPtr::new(ptr::null_mut()),
                waker: waker,
                signals,
            }),
        }
    }

    /// Execute: select a signal using the selector for fairness and atomically acquire it
    /// Returns the producer_id if successful, None if no signals or contention
    #[inline]
    fn execute_pop(&self, selector: &mut Selector) -> Result<T, PopError> {
        if self.producer_count() == 0 {
            return Err(PopError::Empty);
        }

        // Get signal index from selector for fairness
        let mut producer_id = (selector.next() as usize) & MAX_PRODUCERS_MASK;
        let map_index = producer_id / 64;
        let mut signal_bit = (producer_id - (map_index * 64)) as u64;

        let signal = &self.inner.signals[map_index];
        let signal_value = signal.load(Ordering::Acquire);

        // Any signals?
        if signal_value == 0 {
            return Err(PopError::Empty);
        }

        // Find nearest set bit for fairness
        signal_bit = find_nearest(signal_value, signal_bit);

        if signal_bit >= 64 {
            return Err(PopError::Empty);
        }

        producer_id = map_index * 64 + (signal_bit as usize);

        // Atomically acquire the bit
        let (bit, expected, acquired) = signal.try_acquire(signal_bit);

        if !acquired {
            // Contention - try next
            return Err(PopError::Empty);
        }

        // Is the signal empty?
        let empty = expected == bit;

        if empty {
            self.inner
                .waker
                .try_unmark_if_empty(signal.index(), signal.value());
            // self.inner.waker.decrement();
        }

        // Load the queue pointer atomically
        let queue_ptr = self.inner.producers[producer_id].load(Ordering::Acquire);
        if queue_ptr.is_null() {
            return Err(PopError::Closed);
        }

        // SAFETY: The pointer is valid and we have exclusive consumer access
        let queue = unsafe { &*queue_ptr };

        // Try to pop from the queue
        return match queue.try_pop() {
            Some(value) => {
                // Reschedule if queue still has items
                let prev = signal.set_with_bit(bit);
                if prev == 0 && !empty {
                    self.inner.waker.mark_active(signal.index());
                    // self.inner.waker.increment();
                }
                Ok(value)
            }
            None => {
                // Queue is empty - check if we should clean it up
                if self.is_closed() && queue.is_empty() {
                    // Queue is closed and empty - clean up the slot
                    let old_ptr =
                        self.inner.producers[producer_id].swap(ptr::null_mut(), Ordering::AcqRel);

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

    /// Get or create the thread-local SPSC queue for this producer
    pub fn get_local_producer(&self) -> (*const SegSpsc<T, P, NUM_SEGS_P2>, usize) {
        self.with_local_producer(|producer, producer_id| {
            (producer as *const SegSpsc<T, P, NUM_SEGS_P2>, producer_id)
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
    pub fn with_local_producer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&SegSpsc<T, P, NUM_SEGS_P2>, usize) -> R,
    {
        let inner_ptr = Arc::as_ptr(&self.inner) as usize;

        // Check hot entry first (fastest path)
        let hot_result = HOT_QUEUE.with(|hot_entry| {
            let hot = unsafe { &*hot_entry.get() };
            hot.get(inner_ptr)
        });

        if let Some((producer_id, queue_ptr)) = hot_result {
            // Hot entry hit - reconstruct Arc and execute closure
            let queue_ptr = queue_ptr as *const SegSpsc<T, P, NUM_SEGS_P2>;
            let queue_arc = unsafe { ManuallyDrop::new(Arc::from_raw(queue_ptr)) };

            // Execute closure with reference to avoid clone
            let result = f(&queue_arc, producer_id);

            // Note: ManuallyDrop ensures we don't double-drop when Arc goes out of scope
            return result;
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
                let queue_ptr = queue_ptr as *const SegSpsc<T, P, NUM_SEGS_P2>;
                let queue_arc = unsafe { ManuallyDrop::new(Arc::from_raw(queue_ptr)) };

                // Execute closure with reference to avoid clone
                let result = f(&queue_arc, producer_id);

                // Note: ManuallyDrop ensures we don't double-drop when Arc goes out of scope
                result
            } else {
                // Need to create or find a queue
                let mut producer_id = None;
                let mut queue: Option<*mut SegSpsc<T, P, NUM_SEGS_P2>> = None;
                let max_queues = MAX_PRODUCERS;

                'outer: for bit_index in 0..64 {
                    for signal_index in 0..max_queues / 64 {
                        let queue_index = (signal_index * 64) + bit_index;
                        if self.inner.producers[queue_index]
                            .load(Ordering::Acquire)
                            .is_null()
                        {
                            // Create new queue (SegSpsc doesn't need signals/waker)
                            let new_queue = Box::into_raw(Box::new(
                                SegSpsc::<T, P, NUM_SEGS_P2>::new_with_gate(SignalGate::new(
                                    bit_index as u64,
                                    self.inner.signals[signal_index].clone(),
                                    Arc::clone(&self.inner.waker),
                                )),
                            ));

                            let queue_raw_ptr = new_queue as *mut SegSpsc<T, P, NUM_SEGS_P2>;

                            // Try to store atomically
                            match self.inner.producers[queue_index].compare_exchange(
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
                f(unsafe { &*queue }, producer_id)
            }
        })
    }

    /// Push a value onto the queue
    ///
    /// This will use the thread-local SPSC queue for this producer.
    /// If successful, notifies any waiting consumer.
    pub fn push(&self, value: T) -> Result<(), PushError<T>> {
        if self.is_closed() {
            return Err(PushError::Closed(value));
        }

        self.with_local_producer(|queue, _| queue.try_push(value))
    }

    /// Push a value onto the queue (spins if full)
    ///
    /// This will use the thread-local SPSC queue for this producer.
    /// If successful, notifies any waiting consumer.
    pub fn push_spin(&self, value: T) -> Result<(), PushError<T>> {
        if self.is_closed() {
            return Err(PushError::Closed(value));
        }

        self.with_local_producer(move |queue, _| {
            // Spin until we can push
            loop {
                if let Ok(()) = queue.try_push(value) {
                    return Ok(());
                }
                std::hint::spin_loop();
            }
        })
    }

    /// Push multiple values in bulk
    pub fn push_bulk(&self, values: &[T]) -> Result<usize, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }

        self.with_local_producer(|queue, _| queue.try_push_n(values))
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
    pub fn create_producer_handle(&self) -> Result<Producer<T, P, NUM_SEGS_P2>, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }

        // Need to create or find a queue
        let mut producer_id = None;
        let mut queue: Option<*mut SegSpsc<T, P, NUM_SEGS_P2>> = None;
        let max_queues = MAX_PRODUCERS;

        'outer: for bit_index in 0..64 {
            for signal_index in 0..max_queues / 64 {
                let queue_index = (signal_index * 64) + bit_index;
                if self.inner.producers[queue_index]
                    .load(Ordering::Acquire)
                    .is_null()
                {
                    // Create new queue
                    let new_queue = Box::into_raw(Box::new(
                        SegSpsc::<T, P, NUM_SEGS_P2>::new_with_gate(SignalGate::new(
                            bit_index as u64,
                            self.inner.signals[signal_index].clone(),
                            Arc::clone(&self.inner.waker),
                        )),
                    ));

                    let queue_raw_ptr = new_queue as *mut SegSpsc<T, P, NUM_SEGS_P2>;

                    // Try to store atomically
                    match self.inner.producers[queue_index].compare_exchange(
                        ptr::null_mut(),
                        queue_raw_ptr,
                        Ordering::Release,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            println!("registered producer at: {}", queue_index);
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

        if producer_id.is_none() || queue.is_none() {
            Err(PushError::Full(()))
        } else {
            let pid = producer_id.unwrap();
            let signal_index = pid / 64;
            Ok(Producer {
                producer_id: pid,
                queue: queue.unwrap(),
                signal: Arc::new(self.inner.signals[signal_index].clone()),
                waker: Arc::clone(&self.inner.waker),
            })
        }
    }

    /// Pop a value from the queue (non-blocking)
    ///
    /// Returns immediately with Empty if no items are available.
    ///
    /// # Safety
    ///
    /// This method may not be called concurrently from multiple threads.
    pub fn pop(&self) -> Result<T, PopError> {
        SELECTORS.with(|selectors| {
            let selector = unsafe { &mut *selectors.get() };
            self.pop_with_selector(selector)
        })
    }

    /// Pop a value from the queue (non-blocking) using supplied Selector
    ///
    /// Returns immediately with Empty if no items are available.
    ///
    /// # Safety
    ///
    /// This method may not be called concurrently from multiple threads.
    pub fn pop_with_selector(&self, selector: &mut Selector) -> Result<T, PopError> {
        for _ in 0..MAX_PRODUCERS / 64 * 4 {
            match self.execute_pop(selector) {
                Ok(value) => return Ok(value),
                _ => {}
            }
        }

        self.execute_pop(selector)
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
    pub fn close(&self) {
        self.inner.closed.store(1 as *mut (), Ordering::Release);

        // Wait until all queues are empty (drained by consumer)
        loop {
            let mut all_empty = true;

            for slot in self.inner.producers.iter() {
                let queue_ptr = slot.load(Ordering::Acquire);
                if !queue_ptr.is_null() {
                    // SAFETY: The pointer is valid
                    let queue = unsafe { &*queue_ptr };

                    // Check if queue is empty (non-consuming check)
                    if !queue.is_empty() {
                        all_empty = false;
                        break;
                    }
                }
            }

            if all_empty {
                break;
            }

            // Yield to let consumer drain items
            thread::yield_now();
        }
    }

    /// Check if the queue is closed
    pub fn is_closed(&self) -> bool {
        !self.inner.closed.load(Ordering::Acquire).is_null()
    }

    /// Get the number of registered producers
    pub fn producer_count(&self) -> usize {
        self.inner.producer_count.load(Ordering::Relaxed)
    }

    /// Drain up to max_count items from the queue, calling the closure for each item
    ///
    /// This method efficiently drains items from multiple producer queues using a selector
    /// for fairness. It automatically cleans up producer queues that are empty and closed.
    ///
    /// Returns the total number of items drained across all producer queues.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut count = 0;
    /// let drained = mpmc.drain_with(|item| {
    ///     // Process item
    ///     count += 1;
    /// }, 100);
    /// println!("Drained {} items", drained);
    /// ```
    ///
    /// # Safety
    ///
    /// This method may not be called concurrently from multiple threads (single consumer).
    pub fn drain_with<F>(&self, f: F, max_count: usize) -> usize
    where
        F: FnMut(T),
    {
        SELECTORS.with(|selectors| {
            let selector = unsafe { &mut *selectors.get() };
            self.drain_with_selector(selector, f, max_count)
        })
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
    pub fn drain_with_selector<F>(
        &self,
        selector: &mut Selector,
        mut f: F,
        max_count: usize,
    ) -> usize
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
            if consecutive_empty >= MAX_PRODUCERS {
                break;
            }

            // Get signal index from selector for fairness
            let mut producer_id = (selector.next() as usize) & MAX_PRODUCERS_MASK;
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
                continue;
            }

            // Is the signal empty?
            let empty = expected == bit;

            if empty {
                self.inner
                    .waker
                    .try_unmark_if_empty(signal.index(), signal.value());
                // self.inner.waker.decrement();
            }

            // Load the queue pointer atomically
            let queue_ptr = self.inner.producers[producer_id].load(Ordering::Acquire);
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
                    self.inner.producers[producer_id].swap(ptr::null_mut(), Ordering::AcqRel);

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

    pub fn pop_bulk_with_selector(&self, selector: &mut Selector, batch: &mut [T]) -> usize {
        let mut total_drained = 0;
        let max_count = batch.len();
        let mut remaining = max_count;

        // Try to drain from each producer queue using the selector for fairness
        // We'll cycle through producers until we've drained max_count items or all queues are empty
        let mut consecutive_empty = 0;

        for _ in 0..8 {
            // If we've checked all producers and found them all empty, we're done
            if consecutive_empty >= MAX_PRODUCERS {
                break;
            }

            // find_nearest(&self.inner.waker, signal_index)

            // Get signal index from selector for fairness
            let mut producer_id = (selector.next() as usize) & MAX_PRODUCERS_MASK;
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
                continue;
            }

            // Is the signal empty?
            let empty = expected == bit;

            // Load the queue pointer atomically
            let queue_ptr = self.inner.producers[producer_id].load(Ordering::Acquire);
            if queue_ptr.is_null() {
                consecutive_empty += 1;
                if empty {
                    self.inner
                        .waker
                        .try_unmark_if_empty(signal.index(), signal.value());
                    // self.inner.waker.decrement();
                }
                continue;
            }

            // SAFETY: The pointer is valid and we have exclusive consumer access
            let queue = unsafe { &*queue_ptr };

            // Mark as EXECUTING
            queue.mark();

            // Drain from this producer's queue using consume_in_place
            let drained = match queue.try_pop_n(batch) {
                Ok(size) => size,
                Err(_) => 0,
            };

            total_drained += drained;
            remaining -= drained;

            // Handle cleanup and rescheduling
            let can_dispose = false;

            if can_dispose {
                // Queue is empty and closed - clean up the slot
                let old_ptr =
                    self.inner.producers[producer_id].swap(ptr::null_mut(), Ordering::AcqRel);

                if !old_ptr.is_null() {
                    // Decrement producer count
                    self.inner.producer_count.fetch_sub(1, Ordering::Relaxed);

                    // Drop the Box (last reference from producers array)
                    unsafe {
                        drop(Box::from_raw(old_ptr));
                    }
                }
            }

            if drained > 0 {
                queue.unmark_and_schedule();
                return drained;
            } else {
                queue.unmark();
                // Queue is empty
                consecutive_empty += 1;
            }
        }

        total_drained
    }
}

impl<T: 'static + Copy, const P: usize, const NUM_SEGS_P2: usize> Clone
    for MpmcBlocking<T, P, NUM_SEGS_P2>
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: 'static + Copy, const P: usize, const NUM_SEGS_P2: usize> Default
    for MpmcBlocking<T, P, NUM_SEGS_P2>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy, const P: usize, const NUM_SEGS_P2: usize> Drop
    for MpmcBlockingInner<T, P, NUM_SEGS_P2>
{
    fn drop(&mut self) {
        // Clean up all Box<SegSpsc> instances stored as raw pointers
        for slot in self.producers.iter() {
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

unsafe impl<T: Send + Copy, const P: usize, const NUM_SEGS_P2: usize> Send
    for MpmcBlocking<T, P, NUM_SEGS_P2>
{
}
unsafe impl<T: Send + Copy, const P: usize, const NUM_SEGS_P2: usize> Sync
    for MpmcBlocking<T, P, NUM_SEGS_P2>
{
}

#[cfg(test)]
mod tests {}
