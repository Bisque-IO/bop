//! A bounded SPSC queue optimized for single-producer, single-consumer use.
//!
//! This is a simpler and faster variant than the MPSC queue since it doesn't
//! need to handle multiple producers racing to enqueue.
//!
//! Optimizations:
//! - Power-of-two capacity for branchless index calculation
//! - No CAS operations needed for enqueue (single producer)
//! - Aggressive inlining of hot paths
//! - Unchecked array indexing in release builds

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::utils::CachePadded;

/// A queue slot containing a value.
#[repr(C)]
struct Slot<T> {
    value: UnsafeCell<MaybeUninit<T>>,
}

/// An SPSC queue.
///
/// This queue is optimized for single-producer, single-consumer scenarios.
/// It uses a simple ring buffer with cached positions to minimize cache line
/// contention between producer and consumer.
pub(super) struct Queue<T> {
    /// Buffer position for the next enqueue.
    /// Only written by the producer.
    head: CachePadded<AtomicUsize>,

    /// Cached tail position for the producer to check for space.
    /// Only accessed by the producer.
    tail_cache: CachePadded<UnsafeCell<usize>>,

    /// Buffer position for the next dequeue.
    /// Only written by the consumer.
    tail: CachePadded<AtomicUsize>,

    /// Cached head position for the consumer to check for items.
    /// Only accessed by the consumer.
    head_cache: CachePadded<UnsafeCell<usize>>,

    /// Buffer holding the values.
    buffer: Box<[Slot<T>]>,

    /// Bit mask for the buffer index (capacity - 1 for power-of-two).
    index_mask: usize,

    /// Whether the channel has been closed.
    closed: AtomicBool,
}

impl<T> Queue<T> {
    /// Creates a new SPSC queue.
    ///
    /// The capacity will be rounded up to the next power of two for optimal performance.
    #[inline]
    pub(super) fn new(capacity: usize) -> Queue<T> {
        assert!(capacity >= 1, "the capacity must be 1 or greater");

        assert!(
            capacity <= (1 << (usize::BITS - 2)),
            "the capacity may not exceed {}",
            1usize << (usize::BITS - 2)
        );

        // Round up to power of two for branchless index calculation
        let actual_capacity = capacity.next_power_of_two();
        let index_mask = actual_capacity - 1;

        // Allocate a buffer
        let mut buffer = Vec::with_capacity(actual_capacity);
        for _ in 0..actual_capacity {
            buffer.push(Slot {
                value: UnsafeCell::new(MaybeUninit::uninit()),
            });
        }

        Queue {
            head: CachePadded::new(AtomicUsize::new(0)),
            tail_cache: CachePadded::new(UnsafeCell::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
            head_cache: CachePadded::new(UnsafeCell::new(0)),
            buffer: buffer.into(),
            index_mask,
            closed: AtomicBool::new(false),
        }
    }

    /// Attempts to push an item in the queue.
    ///
    /// # Safety
    ///
    /// This method may not be called concurrently from multiple threads.
    #[inline(always)]
    pub(super) unsafe fn push(&self, value: T) -> Result<(), PushError<T>> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(PushError::Closed(value));
        }

        let head = self.head.load(Ordering::Relaxed);

        // Check if we have space using cached tail
        let tail_cached = unsafe { *self.tail_cache.get() };
        let len = head.wrapping_sub(tail_cached);

        if len >= self.buffer.len() {
            // Cache miss - reload tail from atomic
            let tail = self.tail.load(Ordering::Acquire);
            unsafe {
                *self.tail_cache.get() = tail;
            }
            let len = head.wrapping_sub(tail);
            if len >= self.buffer.len() {
                return Err(PushError::Full(value));
            }
        }

        // SAFETY: index_mask ensures we're always in bounds (power-of-two capacity)
        let index = head & self.index_mask;
        let slot = unsafe { self.buffer.get_unchecked(index) };

        // Write the value into the slot
        unsafe {
            (*slot.value.get()).write(value);
        }

        // Publish the write by updating head
        self.head.store(head.wrapping_add(1), Ordering::Release);

        Ok(())
    }

    /// Attempts to push multiple items into the queue.
    ///
    /// Returns the number of items successfully pushed.
    ///
    /// # Safety
    ///
    /// This method may not be called concurrently from multiple threads.
    #[inline]
    pub(super) unsafe fn push_n(&self, src: &[T]) -> Result<usize, PushError<()>>
    where
        T: Copy,
    {
        if src.is_empty() {
            return Ok(0);
        }

        if self.closed.load(Ordering::Relaxed) {
            return Err(PushError::Closed(()));
        }

        let head = self.head.load(Ordering::Relaxed);

        // Check available space using cached tail
        let tail_cached = unsafe { *self.tail_cache.get() };
        let mut free = self.buffer.len().wrapping_sub(head.wrapping_sub(tail_cached));

        if free == 0 {
            // Cache miss - reload tail from atomic
            let tail = self.tail.load(Ordering::Acquire);
            unsafe {
                *self.tail_cache.get() = tail;
            }
            free = self.buffer.len().wrapping_sub(head.wrapping_sub(tail));
            if free == 0 {
                return Err(PushError::Full(()));
            }
        }

        let to_push = src.len().min(free);
        let mut pushed = 0;
        let mut current_head = head;

        while pushed < to_push {
            let index = current_head & self.index_mask;
            let contiguous = (self.buffer.len() - index).min(to_push - pushed);

            unsafe {
                let dst_ptr = self.buffer.get_unchecked(index).value.get() as *mut T;
                let src_ptr = src.as_ptr().add(pushed);
                std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, contiguous);
            }

            pushed += contiguous;
            current_head = current_head.wrapping_add(contiguous);
        }

        // Publish the writes by updating head
        self.head.store(current_head, Ordering::Release);

        Ok(pushed)
    }

    /// Attempts to pop an item from the queue.
    ///
    /// # Safety
    ///
    /// This method may not be called concurrently from multiple threads.
    #[inline(always)]
    pub(super) unsafe fn pop(&self) -> Result<T, PopError> {
        let tail = self.tail.load(Ordering::Relaxed);

        // Check if we have items using cached head
        let head_cached = unsafe { *self.head_cache.get() };

        if tail == head_cached {
            // Cache miss - reload head from atomic
            let head = self.head.load(Ordering::Acquire);
            unsafe {
                *self.head_cache.get() = head;
            }
            if tail == head {
                if self.closed.load(Ordering::Relaxed) {
                    return Err(PopError::Closed);
                }
                return Err(PopError::Empty);
            }
        }

        // SAFETY: index_mask ensures we're always in bounds
        let index = tail & self.index_mask;
        let slot = unsafe { self.buffer.get_unchecked(index) };

        // Read the value from the slot
        let value = unsafe { (*slot.value.get()).assume_init_read() };

        // Publish the read by updating tail
        self.tail.store(tail.wrapping_add(1), Ordering::Release);

        Ok(value)
    }

    /// Attempts to pop multiple items from the queue into a buffer.
    ///
    /// Returns the number of items successfully popped.
    ///
    /// # Safety
    ///
    /// This method may not be called concurrently from multiple threads.
    #[inline]
    pub(super) unsafe fn pop_n(&self, dst: &mut [T]) -> Result<usize, PopError>
    where
        T: Copy,
    {
        if dst.is_empty() {
            return Ok(0);
        }

        let tail = self.tail.load(Ordering::Relaxed);

        // Check available items using cached head
        let head_cached = unsafe { *self.head_cache.get() };
        let mut avail = head_cached.wrapping_sub(tail);

        if avail == 0 {
            // Cache miss - reload head from atomic
            let head = self.head.load(Ordering::Acquire);
            unsafe {
                *self.head_cache.get() = head;
            }
            avail = head.wrapping_sub(tail);
            if avail == 0 {
                if self.closed.load(Ordering::Relaxed) {
                    return Err(PopError::Closed);
                }
                return Err(PopError::Empty);
            }
        }

        let to_pop = dst.len().min(avail);
        let mut popped = 0;
        let mut current_tail = tail;

        while popped < to_pop {
            let index = current_tail & self.index_mask;
            let contiguous = (self.buffer.len() - index).min(to_pop - popped);

            unsafe {
                let src_ptr = self.buffer.get_unchecked(index).value.get() as *const T;
                let dst_ptr = dst.as_mut_ptr().add(popped);
                std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, contiguous);
            }

            popped += contiguous;
            current_tail = current_tail.wrapping_add(contiguous);
        }

        // Publish the reads by updating tail
        self.tail.store(current_tail, Ordering::Release);

        Ok(popped)
    }

    /// Closes the queue.
    #[inline]
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    /// Checks if the channel has been closed.
    #[inline]
    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Returns the capacity of the queue (always a power of two).
    #[inline]
    pub(super) fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Returns the current number of items in the queue.
    #[inline]
    pub(super) fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }

    /// Returns true if the queue is empty.
    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the queue is full.
    #[inline]
    pub(super) fn is_full(&self) -> bool {
        self.len() >= self.buffer.len()
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        // Drop all values in the queue.
        // Safety: single-thread access is guaranteed since the dropping thread
        // holds exclusive ownership.
        unsafe {
            while self.pop().is_ok() {}
        }
    }
}

unsafe impl<T: Send> Send for Queue<T> {}
unsafe impl<T: Send> Sync for Queue<T> {}

/// Error occurring when pushing into a queue is unsuccessful.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum PushError<T> {
    /// The queue is full.
    Full(T),
    /// The receiver has been dropped.
    Closed(T),
}

/// Error occurring when popping from a queue is unsuccessful.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum PopError {
    /// The queue is empty.
    Empty,
    /// All senders have been dropped and the queue is empty.
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn basic_push_pop() {
        let queue = Queue::<u64>::new(16);
        unsafe {
            queue.push(42).unwrap();
            queue.push(43).unwrap();
            assert_eq!(queue.pop().unwrap(), 42);
            assert_eq!(queue.pop().unwrap(), 43);
            assert!(matches!(queue.pop(), Err(PopError::Empty)));
        }
    }

    #[test]
    fn capacity_is_power_of_two() {
        let queue = Queue::<u64>::new(10);
        assert_eq!(queue.capacity(), 16);

        let queue = Queue::<u64>::new(16);
        assert_eq!(queue.capacity(), 16);

        let queue = Queue::<u64>::new(17);
        assert_eq!(queue.capacity(), 32);
    }

    #[test]
    fn fill_and_drain() {
        let queue = Queue::<u64>::new(8);
        let capacity = queue.capacity();

        unsafe {
            // Fill the queue
            for i in 0..capacity as u64 {
                queue.push(i).unwrap();
            }

            // Should be full now
            assert!(matches!(queue.push(999), Err(PushError::Full(999))));

            // Drain and verify order
            for i in 0..capacity as u64 {
                assert_eq!(queue.pop().unwrap(), i);
            }

            // Should be empty now
            assert!(matches!(queue.pop(), Err(PopError::Empty)));
        }
    }

    #[test]
    fn close_semantics() {
        let queue = Queue::<u64>::new(16);
        unsafe {
            queue.push(1).unwrap();
            queue.push(2).unwrap();

            queue.close();

            // Can still pop existing items
            assert_eq!(queue.pop().unwrap(), 1);
            assert_eq!(queue.pop().unwrap(), 2);

            // Now returns Closed
            assert!(matches!(queue.pop(), Err(PopError::Closed)));

            // Push fails with Closed
            assert!(matches!(queue.push(3), Err(PushError::Closed(3))));
        }
    }

    #[test]
    fn spsc_stress_test() {
        const COUNT: usize = 100_000;
        let queue = Arc::new(Queue::<usize>::new(1024));

        let producer_queue = Arc::clone(&queue);
        let producer = thread::spawn(move || {
            for i in 0..COUNT {
                loop {
                    match unsafe { producer_queue.push(i) } {
                        Ok(()) => break,
                        Err(PushError::Full(_)) => {
                            thread::yield_now();
                            continue;
                        }
                        Err(PushError::Closed(_)) => panic!("unexpected closed"),
                    }
                }
            }
        });

        let consumer_queue = Arc::clone(&queue);
        let consumer = thread::spawn(move || {
            for expected in 0..COUNT {
                loop {
                    match unsafe { consumer_queue.pop() } {
                        Ok(v) => {
                            assert_eq!(v, expected);
                            break;
                        }
                        Err(PopError::Empty) => {
                            thread::yield_now();
                            continue;
                        }
                        Err(PopError::Closed) => panic!("unexpected closed"),
                    }
                }
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();
    }

    #[test]
    fn batch_operations() {
        let queue = Queue::<u64>::new(64);
        let src = [1u64, 2, 3, 4, 5, 6, 7, 8];

        unsafe {
            let pushed = queue.push_n(&src).unwrap();
            assert_eq!(pushed, 8);

            let mut dst = [0u64; 5];
            let popped = queue.pop_n(&mut dst).unwrap();
            assert_eq!(popped, 5);
            assert_eq!(&dst, &[1, 2, 3, 4, 5]);

            let mut dst2 = [0u64; 10];
            let popped2 = queue.pop_n(&mut dst2).unwrap();
            assert_eq!(popped2, 3);
            assert_eq!(&dst2[..3], &[6, 7, 8]);
        }
    }

    #[test]
    fn len_and_full_empty() {
        let queue = Queue::<u64>::new(4);
        let capacity = queue.capacity();

        assert!(queue.is_empty());
        assert!(!queue.is_full());
        assert_eq!(queue.len(), 0);

        unsafe {
            queue.push(1).unwrap();
            assert!(!queue.is_empty());
            assert_eq!(queue.len(), 1);

            for i in 1..capacity as u64 {
                queue.push(i + 1).unwrap();
            }
            assert!(queue.is_full());
            assert_eq!(queue.len(), capacity);

            queue.pop().unwrap();
            assert!(!queue.is_full());
            assert_eq!(queue.len(), capacity - 1);
        }
    }
}
