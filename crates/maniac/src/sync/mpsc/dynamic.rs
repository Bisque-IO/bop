//! Dynamic Unbounded MPSC channel implementation.
//!
//! This module provides unbounded multi-producer, single-consumer channels with
//! runtime-configurable queue sizes. Unlike the const-generic version, queue
//! parameters are determined at runtime via `DynSpscConfig`.
//!
//! # Design
//!
//! The implementation wraps the underlying async unbounded MPSC with `Arc<Mutex<...>>`
//! for simple, safe access patterns. This trades some performance for simplicity
//! and compatibility with frameworks like openraft that expect this API style.
//!
//! # Example
//!
//! ```ignore
//! use maniac::sync::dyn_mpsc_unbounded::{dyn_unbounded_channel, DynMpscConfig};
//!
//! let config = DynMpscConfig::default();
//! let (sender, mut receiver) = dyn_unbounded_channel::<u64>(config);
//!
//! // Clone sender for multiple producers
//! let sender2 = sender.clone();
//!
//! sender.send(42).unwrap();
//! sender2.send(43).unwrap();
//!
//! assert_eq!(receiver.recv().await, Some(42));
//! assert_eq!(receiver.recv().await, Some(43));
//! ```

use std::sync::{Arc, Mutex, Weak};

use rand::RngCore;

use crate::future::waker::DiatomicWaker;
use crate::parking::{Parker, Unparker};
use crate::spsc::dynamic::DynSpscConfig;
use crate::spsc::NoOpSignal;
use crate::sync::signal::{AsyncSignalWaker, Signal};
use crate::utils::CachePadded;
use crate::{PopError, PushError};

use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use std::task::{Wake, Waker};

// Constants for MPSC
const MAX_QUEUES: usize = 4096;
const MAX_PRODUCERS: usize = MAX_QUEUES;
const SIGNAL_WORDS: usize = 64;

// Random number generator constants (same as mpsc.rs)
const RND_MULTIPLIER: u64 = 0x5DEECE66D;
const RND_ADDEND: u64 = 0xB;
const RND_MASK: u64 = (1 << 48) - 1;

/// Configuration for the dynamic MPSC queue.
///
/// This wraps `DynSpscConfig` and provides MPSC-specific defaults.
#[derive(Debug, Clone, Copy)]
pub struct DynMpscConfig {
    /// Configuration for each producer's SPSC queue
    pub spsc_config: DynSpscConfig,
}

impl Default for DynMpscConfig {
    fn default() -> Self {
        // Default: 64 items/segment (2^6), 256 segments (2^8)
        Self {
            spsc_config: DynSpscConfig::new(6, 8),
        }
    }
}

impl DynMpscConfig {
    /// Create a new configuration with custom SPSC parameters.
    ///
    /// # Arguments
    ///
    /// * `p` - Log2 of items per segment (e.g., 6 = 64 items/segment)
    /// * `num_segs_p2` - Log2 of number of segments (e.g., 8 = 256 segments)
    pub fn new(p: usize, num_segs_p2: usize) -> Self {
        Self {
            spsc_config: DynSpscConfig::new(p, num_segs_p2),
        }
    }

    /// Create from an existing `DynSpscConfig`.
    pub fn from_spsc_config(spsc_config: DynSpscConfig) -> Self {
        Self { spsc_config }
    }
}

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

/// A single segment in the unbounded linked list
struct DynSegment<T> {
    /// Items storage
    items: Box<[AtomicPtr<T>]>,
    /// Next segment in the linked list
    next: AtomicPtr<DynSegment<T>>,
    /// Capacity of this segment
    capacity: usize,
}

impl<T> DynSegment<T> {
    fn new(capacity: usize) -> *mut Self {
        let mut items = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            items.push(AtomicPtr::new(ptr::null_mut()));
        }

        let segment = Box::new(Self {
            items: items.into_boxed_slice(),
            next: AtomicPtr::new(ptr::null_mut()),
            capacity,
        });
        Box::into_raw(segment)
    }
}

/// Dynamic unbounded SPSC node for a single producer
///
/// Implemented as a linked list of segments that grows as needed.
struct DynUnboundedNode<T> {
    /// Runtime configuration
    config: DynSpscConfig,
    /// Head segment (consumer reads from here)
    head_segment: AtomicPtr<DynSegment<T>>,
    /// Head position within the segment
    head_pos: CachePadded<AtomicUsize>,
    /// Tail segment (producer writes here) - cached for producer
    tail_segment: AtomicPtr<DynSegment<T>>,
    /// Tail position within the segment
    tail_pos: CachePadded<AtomicUsize>,
    /// Closed flag
    closed: AtomicBool,
    /// Space waker for producer
    space_waker: DiatomicWaker,
}

impl<T> DynUnboundedNode<T> {
    fn new(config: DynSpscConfig) -> Self {
        let segment_capacity = config.capacity().max(64); // At least 64 items per segment
        let first_segment = DynSegment::new(segment_capacity);

        Self {
            config,
            head_segment: AtomicPtr::new(first_segment),
            head_pos: CachePadded::new(AtomicUsize::new(0)),
            tail_segment: AtomicPtr::new(first_segment),
            tail_pos: CachePadded::new(AtomicUsize::new(0)),
            closed: AtomicBool::new(false),
            space_waker: DiatomicWaker::new(),
        }
    }

    fn segment_capacity(&self) -> usize {
        self.config.capacity().max(64)
    }

    fn try_push(&self, value: T) -> Result<(), PushError<T>> {
        if self.closed.load(Ordering::Acquire) {
            return Err(PushError::Closed(value));
        }

        let tail_segment = self.tail_segment.load(Ordering::Acquire);
        let tail_pos = self.tail_pos.load(Ordering::Relaxed);
        let segment = unsafe { &*tail_segment };

        if tail_pos < segment.capacity {
            // Space in current segment
            let boxed = Box::new(value);
            let ptr = Box::into_raw(boxed);
            segment.items[tail_pos].store(ptr, Ordering::Release);
            self.tail_pos.store(tail_pos + 1, Ordering::Release);
            Ok(())
        } else {
            // Need a new segment
            let new_segment = DynSegment::new(self.segment_capacity());
            segment.next.store(new_segment, Ordering::Release);

            // Store the value in the new segment
            let new_seg_ref = unsafe { &*new_segment };
            let boxed = Box::new(value);
            let ptr = Box::into_raw(boxed);
            new_seg_ref.items[0].store(ptr, Ordering::Release);

            // Update tail to new segment
            self.tail_segment.store(new_segment, Ordering::Release);
            self.tail_pos.store(1, Ordering::Release);
            Ok(())
        }
    }

    /// Push multiple values from a Vec, draining successfully pushed items.
    ///
    /// Returns the number of items pushed. Since this is unbounded, all items
    /// will be pushed unless the channel is closed.
    fn try_push_n(&self, values: &mut Vec<T>) -> Result<usize, PushError<()>> {
        if self.closed.load(Ordering::Acquire) {
            return Err(PushError::Closed(()));
        }

        if values.is_empty() {
            return Ok(0);
        }

        let mut pushed = 0;

        while !values.is_empty() {
            let tail_segment = self.tail_segment.load(Ordering::Acquire);
            let mut tail_pos = self.tail_pos.load(Ordering::Relaxed);
            let segment = unsafe { &*tail_segment };

            // Push as many as we can into current segment
            while tail_pos < segment.capacity && !values.is_empty() {
                let value = values.remove(0);
                let boxed = Box::new(value);
                let ptr = Box::into_raw(boxed);
                segment.items[tail_pos].store(ptr, Ordering::Release);
                tail_pos += 1;
                pushed += 1;
            }
            self.tail_pos.store(tail_pos, Ordering::Release);

            // If we still have values and segment is full, allocate new segment
            if !values.is_empty() && tail_pos >= segment.capacity {
                let new_segment = DynSegment::new(self.segment_capacity());
                segment.next.store(new_segment, Ordering::Release);
                self.tail_segment.store(new_segment, Ordering::Release);
                self.tail_pos.store(0, Ordering::Release);
            }
        }

        Ok(pushed)
    }

    fn try_pop(&self) -> Option<T> {
        let head_segment = self.head_segment.load(Ordering::Acquire);
        let head_pos = self.head_pos.load(Ordering::Relaxed);
        let segment = unsafe { &*head_segment };

        // Check if we have items in current segment
        if head_pos < segment.capacity {
            let ptr = segment.items[head_pos].load(Ordering::Acquire);
            if !ptr.is_null() {
                segment.items[head_pos].store(ptr::null_mut(), Ordering::Release);
                self.head_pos.store(head_pos + 1, Ordering::Release);
                self.space_waker.notify();
                return Some(unsafe { *Box::from_raw(ptr) });
            }
        }

        // Check if there's a next segment
        let next_segment = segment.next.load(Ordering::Acquire);
        if !next_segment.is_null() {
            // Move to next segment
            self.head_segment.store(next_segment, Ordering::Release);
            self.head_pos.store(0, Ordering::Release);

            // Free the old segment
            unsafe {
                drop(Box::from_raw(head_segment));
            }

            // Try to pop from new segment
            let new_segment = unsafe { &*next_segment };
            let ptr = new_segment.items[0].load(Ordering::Acquire);
            if !ptr.is_null() {
                new_segment.items[0].store(ptr::null_mut(), Ordering::Release);
                self.head_pos.store(1, Ordering::Release);
                self.space_waker.notify();
                return Some(unsafe { *Box::from_raw(ptr) });
            }
        }

        None
    }

    /// Pop multiple values into a slice.
    ///
    /// Returns the number of items popped (may be less than dst.len() if fewer items available).
    fn try_pop_n(&self, dst: &mut [T]) -> Result<usize, PopError> {
        if dst.is_empty() {
            return Ok(0);
        }

        let mut popped = 0;

        while popped < dst.len() {
            let head_segment = self.head_segment.load(Ordering::Acquire);
            let mut head_pos = self.head_pos.load(Ordering::Relaxed);
            let segment = unsafe { &*head_segment };

            // Pop as many as we can from current segment
            while head_pos < segment.capacity && popped < dst.len() {
                let ptr = segment.items[head_pos].load(Ordering::Acquire);
                if ptr.is_null() {
                    // No more items in this position, check next segment or return
                    break;
                }
                segment.items[head_pos].store(ptr::null_mut(), Ordering::Release);
                dst[popped] = unsafe { *Box::from_raw(ptr) };
                head_pos += 1;
                popped += 1;
            }
            self.head_pos.store(head_pos, Ordering::Release);

            // Check if there's a next segment
            let next_segment = segment.next.load(Ordering::Acquire);
            if next_segment.is_null() {
                // No more segments
                break;
            }

            // Move to next segment if current is exhausted
            if head_pos >= segment.capacity
                || segment.items[head_pos].load(Ordering::Acquire).is_null()
            {
                self.head_segment.store(next_segment, Ordering::Release);
                self.head_pos.store(0, Ordering::Release);

                // Free the old segment
                unsafe {
                    drop(Box::from_raw(head_segment));
                }
            } else {
                break;
            }
        }

        if popped > 0 {
            self.space_waker.notify();
            Ok(popped)
        } else if self.closed.load(Ordering::Acquire) {
            Err(PopError::Closed)
        } else {
            Err(PopError::Empty)
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.space_waker.notify();
    }

    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        let head_segment = self.head_segment.load(Ordering::Acquire);
        let head_pos = self.head_pos.load(Ordering::Acquire);
        let tail_segment = self.tail_segment.load(Ordering::Acquire);
        let tail_pos = self.tail_pos.load(Ordering::Acquire);

        head_segment == tail_segment && head_pos == tail_pos
    }
}

impl<T> Drop for DynUnboundedNode<T> {
    fn drop(&mut self) {
        // Clean up all segments and remaining items
        let mut current = *self.head_segment.get_mut();
        let mut head_pos = *self.head_pos.get_mut();

        while !current.is_null() {
            let segment = unsafe { &mut *current };

            // Clean up items in this segment
            while head_pos < segment.capacity {
                let ptr = segment.items[head_pos].load(Ordering::Relaxed);
                if !ptr.is_null() {
                    unsafe {
                        drop(Box::from_raw(ptr));
                    }
                } else {
                    break; // No more items
                }
                head_pos += 1;
            }

            // Move to next segment
            let next = segment.next.load(Ordering::Relaxed);
            unsafe {
                drop(Box::from_raw(current));
            }
            current = next;
            head_pos = 0;
        }
    }
}

/// Producer slot containing a node
struct DynProducerSlot<T> {
    node: DynUnboundedNode<T>,
}

/// Inner shared state for dynamic unbounded MPSC
struct DynUnboundedInner<T> {
    /// Configuration for new queues
    config: DynMpscConfig,
    /// Sparse array of producer slot pointers
    queues: Box<[AtomicPtr<DynProducerSlot<T>>]>,
    /// Number of active queues
    queue_count: CachePadded<AtomicUsize>,
    /// Number of active producers
    producer_count: CachePadded<AtomicUsize>,
    /// Maximum producer ID seen
    max_producer_id: AtomicUsize,
    /// Closed flag
    closed: CachePadded<AtomicBool>,
    /// Summary waker for receiver
    summary: Arc<AsyncSignalWaker>,
    /// Signal words
    signals: Arc<[Signal; SIGNAL_WORDS]>,
    /// Receiver waker
    receiver_waker: CachePadded<DiatomicWaker>,
}

impl<T> DynUnboundedInner<T> {
    fn new(config: DynMpscConfig) -> Self {
        let mut queues = Vec::with_capacity(MAX_QUEUES);
        for _ in 0..MAX_QUEUES {
            queues.push(AtomicPtr::new(ptr::null_mut()));
        }

        let signals: Arc<[Signal; SIGNAL_WORDS]> =
            Arc::new(std::array::from_fn(|i| Signal::with_index(i as u64)));

        Self {
            config,
            queues: queues.into_boxed_slice(),
            queue_count: CachePadded::new(AtomicUsize::new(0)),
            producer_count: CachePadded::new(AtomicUsize::new(0)),
            max_producer_id: AtomicUsize::new(0),
            closed: CachePadded::new(AtomicBool::new(false)),
            summary: Arc::new(AsyncSignalWaker::new()),
            signals,
            receiver_waker: CachePadded::new(DiatomicWaker::new()),
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn producer_count(&self) -> usize {
        self.producer_count.load(Ordering::Relaxed)
    }

    fn create_sender(self: &Arc<Self>) -> Result<DynUnboundedSenderInner<T>, PushError<()>> {
        if self.is_closed() {
            return Err(PushError::Closed(()));
        }

        // Increment producer count
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

        // Find an empty slot
        let mut assigned_id = None;
        let mut slot_arc: Option<Arc<DynProducerSlot<T>>> = None;

        for signal_index in 0..SIGNAL_WORDS {
            for bit_index in 0..64 {
                let queue_index = signal_index * 64 + bit_index;
                if queue_index >= MAX_QUEUES {
                    break;
                }
                if !self.queues[queue_index].load(Ordering::Acquire).is_null() {
                    continue;
                }

                let slot = Arc::new(DynProducerSlot {
                    node: DynUnboundedNode::new(self.config.spsc_config),
                });

                let raw = Arc::into_raw(Arc::clone(&slot)) as *mut DynProducerSlot<T>;
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

        // Update max_producer_id
        loop {
            let max_producer_id = self.max_producer_id.load(Ordering::SeqCst);
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

        let slot_arc = slot_arc.expect("slot arc missing");

        Ok(DynUnboundedSenderInner {
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
            self.receiver_waker.notify();
        }

        for slot_atomic in self.queues.iter() {
            let slot_ptr = slot_atomic.swap(ptr::null_mut(), Ordering::AcqRel);
            if slot_ptr.is_null() {
                continue;
            }

            unsafe {
                (*slot_ptr).node.close();
                Arc::from_raw(slot_ptr);
            }

            self.queue_count.fetch_sub(1, Ordering::Relaxed);
        }

        self.producer_count.store(0, Ordering::Release);
        true
    }
}

impl<T> Drop for DynUnboundedInner<T> {
    fn drop(&mut self) {
        self.close();
    }
}

/// Inner sender implementation (not behind Mutex)
struct DynUnboundedSenderInner<T> {
    inner: Arc<DynUnboundedInner<T>>,
    slot: Arc<DynProducerSlot<T>>,
    producer_id: usize,
}

impl<T> DynUnboundedSenderInner<T> {
    fn try_push(&self, value: T) -> Result<(), PushError<T>> {
        self.slot.node.try_push(value)
    }

    fn try_push_n(&self, values: &mut Vec<T>) -> Result<usize, PushError<()>> {
        self.slot.node.try_push_n(values)
    }

    fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    fn close(&self) -> bool {
        self.inner.close()
    }
}

impl<T> Clone for DynUnboundedSenderInner<T> {
    fn clone(&self) -> Self {
        self.inner.create_sender().expect("too many senders")
    }
}

impl<T> Drop for DynUnboundedSenderInner<T> {
    fn drop(&mut self) {
        self.slot.node.close();
        std::sync::atomic::fence(Ordering::Release);
        self.inner.producer_count.fetch_sub(1, Ordering::Release);
        self.inner.receiver_waker.notify();
    }
}

/// Inner receiver implementation (not behind Mutex)
struct DynUnboundedReceiverInner<T> {
    inner: Arc<DynUnboundedInner<T>>,
    seed: u64,
}

impl<T> DynUnboundedReceiverInner<T> {
    fn new(inner: Arc<DynUnboundedInner<T>>) -> Self {
        Self {
            inner,
            seed: rand::rng().next_u64(),
        }
    }

    fn next(&mut self) -> u64 {
        let old_seed = self.seed;
        let next_seed = (old_seed
            .wrapping_mul(RND_MULTIPLIER)
            .wrapping_add(RND_ADDEND))
            & RND_MASK;
        self.seed = next_seed;
        next_seed >> 16
    }

    fn try_pop(&mut self) -> Result<T, PopError> {
        let max_producer_id = self.inner.max_producer_id.load(Ordering::Acquire);

        // Try to pop from any queue
        for producer_id in 0..=max_producer_id {
            let slot_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
            if slot_ptr.is_null() {
                continue;
            }

            let slot = unsafe { &*slot_ptr };
            if let Some(value) = slot.node.try_pop() {
                return Ok(value);
            }
        }

        // No items found
        std::sync::atomic::fence(Ordering::Acquire);

        let is_closed = self.inner.is_closed();
        let producer_count = self.inner.producer_count.load(Ordering::Relaxed);

        if is_closed || producer_count == 0 {
            // Final scan
            for producer_id in 0..=max_producer_id {
                let slot_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
                if slot_ptr.is_null() {
                    continue;
                }

                let slot = unsafe { &*slot_ptr };
                if let Some(value) = slot.node.try_pop() {
                    return Ok(value);
                }
            }

            Err(PopError::Closed)
        } else {
            Err(PopError::Empty)
        }
    }

    fn try_pop_n(&mut self, dst: &mut [T]) -> Result<usize, PopError> {
        if dst.is_empty() {
            return Ok(0);
        }

        let max_producer_id = self.inner.max_producer_id.load(Ordering::Acquire);
        let mut total_popped = 0;

        // Try to pop from all queues
        for producer_id in 0..=max_producer_id {
            if total_popped >= dst.len() {
                break;
            }

            let slot_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
            if slot_ptr.is_null() {
                continue;
            }

            let slot = unsafe { &*slot_ptr };
            match slot.node.try_pop_n(&mut dst[total_popped..]) {
                Ok(popped) => {
                    total_popped += popped;
                }
                Err(PopError::Empty) => continue,
                Err(PopError::Closed) => continue,
                Err(e) => return Err(e),
            }
        }

        if total_popped > 0 {
            return Ok(total_popped);
        }

        // No items found
        std::sync::atomic::fence(Ordering::Acquire);

        let is_closed = self.inner.is_closed();
        let producer_count = self.inner.producer_count.load(Ordering::Relaxed);

        if is_closed || producer_count == 0 {
            // Final scan
            for producer_id in 0..=max_producer_id {
                if total_popped >= dst.len() {
                    break;
                }

                let slot_ptr = self.inner.queues[producer_id].load(Ordering::Acquire);
                if slot_ptr.is_null() {
                    continue;
                }

                let slot = unsafe { &*slot_ptr };
                if let Ok(popped) = slot.node.try_pop_n(&mut dst[total_popped..]) {
                    total_popped += popped;
                }
            }

            if total_popped > 0 {
                Ok(total_popped)
            } else {
                Err(PopError::Closed)
            }
        } else {
            Err(PopError::Empty)
        }
    }

    fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    fn close(&self) -> bool {
        self.inner.close()
    }

    fn producer_count(&self) -> usize {
        self.inner.producer_count()
    }

    fn create_sender(&self) -> Result<DynUnboundedSenderInner<T>, PushError<()>> {
        self.inner.create_sender()
    }
}

impl<T> Drop for DynUnboundedReceiverInner<T> {
    fn drop(&mut self) {
        self.inner.close();
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Public API (Mutex-wrapped for simple usage)
// ══════════════════════════════════════════════════════════════════════════════

/// Creates a new dynamic unbounded MPSC channel.
///
/// # Arguments
///
/// * `config` - Runtime configuration for the underlying queues
///
/// # Returns
///
/// A tuple of `(DynUnboundedSender<T>, DynUnboundedReceiver<T>)`
///
/// # Example
///
/// ```ignore
/// let config = DynMpscConfig::default();
/// let (sender, mut receiver) = dyn_unbounded_channel::<u64>(config);
///
/// sender.send(42).unwrap();
/// assert_eq!(receiver.recv().await, Some(42));
/// ```
pub fn dyn_unbounded_channel<T>(config: DynMpscConfig) -> (DynUnboundedSender<T>, DynUnboundedReceiver<T>) {
    let inner = Arc::new(DynUnboundedInner::new(config));
    let receiver_inner = DynUnboundedReceiverInner::new(Arc::clone(&inner));
    let sender_inner = inner.create_sender().expect("fatal: cannot create initial sender");

    (
        DynUnboundedSender {
            inner: Arc::new(Mutex::new(sender_inner)),
        },
        DynUnboundedReceiver {
            inner: Arc::new(Mutex::new(receiver_inner)),
        },
    )
}

/// Creates a new dynamic unbounded MPSC channel with default configuration.
pub fn dyn_unbounded_channel_default<T>() -> (DynUnboundedSender<T>, DynUnboundedReceiver<T>) {
    dyn_unbounded_channel(DynMpscConfig::default())
}

/// The sending half of a dynamic unbounded MPSC channel.
pub struct DynUnboundedSender<T> {
    inner: Arc<Mutex<DynUnboundedSenderInner<T>>>,
}

impl<T> Clone for DynUnboundedSender<T> {
    fn clone(&self) -> Self {
        let guard = self.inner.lock().unwrap();
        let new_inner = guard.clone();
        Self {
            inner: Arc::new(Mutex::new(new_inner)),
        }
    }
}

impl<T: Send + 'static> DynUnboundedSender<T> {
    /// Sends a value through the channel (synchronous, never blocks for unbounded).
    pub fn send(&self, value: T) -> Result<(), DynSendError<T>> {
        let guard = self.inner.lock().unwrap();
        match guard.try_push(value) {
            Ok(()) => {
                guard.inner.receiver_waker.notify();
                Ok(())
            }
            Err(PushError::Full(v)) => Err(DynSendError(v)),
            Err(PushError::Closed(v)) => Err(DynSendError(v)),
        }
    }

    /// Sends multiple values through the channel, draining from the provided Vec.
    ///
    /// Returns the number of items sent. Since this is unbounded, all items
    /// will be sent unless the channel is closed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut values = vec![1, 2, 3, 4, 5];
    /// let sent = sender.send_batch(&mut values).unwrap();
    /// assert_eq!(sent, 5);
    /// assert!(values.is_empty());
    /// ```
    pub fn send_batch(&self, values: &mut Vec<T>) -> Result<usize, DynSendBatchError> {
        let guard = self.inner.lock().unwrap();
        match guard.try_push_n(values) {
            Ok(count) => {
                if count > 0 {
                    guard.inner.receiver_waker.notify();
                }
                Ok(count)
            }
            Err(PushError::Full(())) => Err(DynSendBatchError::Full),
            Err(PushError::Closed(())) => Err(DynSendBatchError::Closed),
        }
    }

    /// Creates a weak reference to this sender.
    pub fn downgrade(&self) -> WeakDynUnboundedSender<T> {
        WeakDynUnboundedSender {
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// Check if the channel is closed.
    pub fn is_closed(&self) -> bool {
        let guard = self.inner.lock().unwrap();
        guard.is_closed()
    }
}

/// Error returned when batch sending fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynSendBatchError {
    /// The channel is full (shouldn't happen for unbounded).
    Full,
    /// The channel is closed.
    Closed,
}

/// Error returned when sending fails.
#[derive(Debug)]
pub struct DynSendError<T>(pub T);

/// A weak reference to a dynamic unbounded MPSC sender.
pub struct WeakDynUnboundedSender<T> {
    inner: Weak<Mutex<DynUnboundedSenderInner<T>>>,
}

impl<T> Clone for WeakDynUnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Weak::clone(&self.inner),
        }
    }
}

impl<T> WeakDynUnboundedSender<T> {
    /// Attempts to upgrade the weak reference to a strong reference.
    pub fn upgrade(&self) -> Option<DynUnboundedSender<T>> {
        self.inner.upgrade().map(|inner| DynUnboundedSender { inner })
    }
}

/// The receiving half of a dynamic unbounded MPSC channel.
pub struct DynUnboundedReceiver<T> {
    inner: Arc<Mutex<DynUnboundedReceiverInner<T>>>,
}

impl<T> DynUnboundedReceiver<T> {
    /// Receives a value from the channel.
    pub async fn recv(&mut self) -> Option<T> {
        // Fast path
        {
            let mut guard = self.inner.lock().unwrap();
            match guard.try_pop() {
                Ok(value) => return Some(value),
                Err(PopError::Closed) => return None,
                Err(PopError::Empty) | Err(PopError::Timeout) => {}
            }
        }

        // Slow path with parking
        let parker = Parker::new();
        let unparker = parker.unparker();
        let waker = Waker::from(Arc::new(ThreadUnparker { unparker }));

        loop {
            {
                let mut guard = self.inner.lock().unwrap();

                // Register waker
                unsafe {
                    guard.inner.receiver_waker.register(&waker);
                }

                // Check again
                match guard.try_pop() {
                    Ok(value) => return Some(value),
                    Err(PopError::Closed) => return None,
                    Err(PopError::Empty) | Err(PopError::Timeout) => {}
                }
            }

            parker.park();
        }
    }

    /// Attempts to receive a value without blocking.
    pub fn try_recv(&mut self) -> Result<T, DynTryRecvError> {
        let mut guard = self.inner.lock().unwrap();
        guard.try_pop().map_err(|e| match e {
            PopError::Empty => DynTryRecvError::Empty,
            PopError::Closed => DynTryRecvError::Disconnected,
            PopError::Timeout => DynTryRecvError::Empty,
        })
    }

    /// Attempts to receive multiple values without blocking.
    ///
    /// Returns the number of items received (may be less than `dst.len()`).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut buffer = [0u64; 10];
    /// let received = receiver.try_recv_batch(&mut buffer).unwrap();
    /// println!("Received {} items", received);
    /// ```
    pub fn try_recv_batch(&mut self, dst: &mut [T]) -> Result<usize, DynTryRecvError> {
        let mut guard = self.inner.lock().unwrap();
        guard.try_pop_n(dst).map_err(|e| match e {
            PopError::Empty => DynTryRecvError::Empty,
            PopError::Closed => DynTryRecvError::Disconnected,
            PopError::Timeout => DynTryRecvError::Empty,
        })
    }

    /// Receives multiple values from the channel, blocking until at least one is available.
    ///
    /// Returns the number of items received. Will block until at least one item
    /// is available or the channel is closed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut buffer = [0u64; 10];
    /// let received = receiver.recv_batch(&mut buffer).await;
    /// match received {
    ///     Some(count) => println!("Received {} items", count),
    ///     None => println!("Channel closed"),
    /// }
    /// ```
    pub async fn recv_batch(&mut self, dst: &mut [T]) -> Option<usize> {
        if dst.is_empty() {
            return Some(0);
        }

        // Fast path
        {
            let mut guard = self.inner.lock().unwrap();
            match guard.try_pop_n(dst) {
                Ok(count) if count > 0 => return Some(count),
                Ok(_) => {}
                Err(PopError::Closed) => return None,
                Err(PopError::Empty) | Err(PopError::Timeout) => {}
            }
        }

        // Slow path with parking
        let parker = Parker::new();
        let unparker = parker.unparker();
        let waker = Waker::from(Arc::new(ThreadUnparker { unparker }));

        loop {
            {
                let mut guard = self.inner.lock().unwrap();

                // Register waker
                unsafe {
                    guard.inner.receiver_waker.register(&waker);
                }

                // Check again
                match guard.try_pop_n(dst) {
                    Ok(count) if count > 0 => return Some(count),
                    Ok(_) => {}
                    Err(PopError::Closed) => return None,
                    Err(PopError::Empty) | Err(PopError::Timeout) => {}
                }
            }

            parker.park();
        }
    }

    /// Get the number of active producers.
    pub fn producer_count(&self) -> usize {
        let guard = self.inner.lock().unwrap();
        guard.producer_count()
    }
}

/// Error returned when trying to receive without blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynTryRecvError {
    /// The channel is empty.
    Empty,
    /// The channel is disconnected.
    Disconnected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_send_recv() {
        let config = DynMpscConfig::default();
        let (sender, mut receiver) = dyn_unbounded_channel::<u64>(config);

        sender.send(42).unwrap();
        sender.send(43).unwrap();

        assert_eq!(receiver.try_recv().unwrap(), 42);
        assert_eq!(receiver.try_recv().unwrap(), 43);
        assert!(matches!(receiver.try_recv(), Err(DynTryRecvError::Empty)));
    }

    #[test]
    fn test_clone_sender() {
        let config = DynMpscConfig::default();
        let (sender1, mut receiver) = dyn_unbounded_channel::<u64>(config);
        let sender2 = sender1.clone();

        sender1.send(1).unwrap();
        sender2.send(2).unwrap();

        let v1 = receiver.try_recv().unwrap();
        let v2 = receiver.try_recv().unwrap();

        // Order may vary due to MPSC nature
        assert!(v1 == 1 || v1 == 2);
        assert!(v2 == 1 || v2 == 2);
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_weak_sender() {
        let config = DynMpscConfig::default();
        let (sender, _receiver) = dyn_unbounded_channel::<u64>(config);
        let weak = sender.downgrade();

        assert!(weak.upgrade().is_some());

        drop(sender);

        // Weak reference should still be upgradeable if receiver holds inner
        // But since sender is dropped, it depends on implementation
    }

    #[test]
    fn test_close_on_drop() {
        let config = DynMpscConfig::default();
        let (sender, mut receiver) = dyn_unbounded_channel::<u64>(config);

        sender.send(1).unwrap();
        drop(sender);

        assert_eq!(receiver.try_recv().unwrap(), 1);
        assert!(matches!(receiver.try_recv(), Err(DynTryRecvError::Disconnected)));
    }

    #[test]
    fn test_custom_config() {
        // Small config
        let config = DynMpscConfig::new(4, 2); // 16 items/segment, 4 segments
        let (sender, mut receiver) = dyn_unbounded_channel::<u64>(config);

        // Send many items
        for i in 0..100u64 {
            sender.send(i).unwrap();
        }

        // Receive all
        for i in 0..100u64 {
            assert_eq!(receiver.try_recv().unwrap(), i);
        }
    }

    #[test]
    fn test_producer_count() {
        let config = DynMpscConfig::default();
        let (sender1, receiver) = dyn_unbounded_channel::<u64>(config);

        assert_eq!(receiver.producer_count(), 1);

        let sender2 = sender1.clone();
        assert_eq!(receiver.producer_count(), 2);

        drop(sender1);
        assert_eq!(receiver.producer_count(), 1);

        drop(sender2);
        assert_eq!(receiver.producer_count(), 0);
    }

    #[test]
    fn test_send_batch() {
        let config = DynMpscConfig::default();
        let (sender, mut receiver) = dyn_unbounded_channel::<u64>(config);

        let mut values = vec![1, 2, 3, 4, 5];
        let sent = sender.send_batch(&mut values).unwrap();
        assert_eq!(sent, 5);
        assert!(values.is_empty());

        for i in 1..=5u64 {
            assert_eq!(receiver.try_recv().unwrap(), i);
        }
        assert!(matches!(receiver.try_recv(), Err(DynTryRecvError::Empty)));
    }

    #[test]
    fn test_try_recv_batch() {
        let config = DynMpscConfig::default();
        let (sender, mut receiver) = dyn_unbounded_channel::<u64>(config);

        // Send some values
        for i in 0..10u64 {
            sender.send(i).unwrap();
        }

        // Receive in batch
        let mut buffer = [0u64; 5];
        let received = receiver.try_recv_batch(&mut buffer).unwrap();
        assert_eq!(received, 5);
        for i in 0..5 {
            assert_eq!(buffer[i], i as u64);
        }

        // Receive remaining
        let received = receiver.try_recv_batch(&mut buffer).unwrap();
        assert_eq!(received, 5);
        for i in 0..5 {
            assert_eq!(buffer[i], (i + 5) as u64);
        }

        // Should be empty now
        assert!(matches!(receiver.try_recv_batch(&mut buffer), Err(DynTryRecvError::Empty)));
    }

    #[test]
    fn test_batch_with_segment_overflow() {
        // Use small segments to test overflow behavior
        let config = DynMpscConfig::new(4, 2); // 16 items/segment minimum becomes 64
        let (sender, mut receiver) = dyn_unbounded_channel::<u64>(config);

        // Send more than one segment's worth
        let mut values: Vec<u64> = (0..200).collect();
        let sent = sender.send_batch(&mut values).unwrap();
        assert_eq!(sent, 200);
        assert!(values.is_empty());

        // Receive all in batches
        let mut total_received = 0;
        let mut buffer = [0u64; 50];
        while total_received < 200 {
            match receiver.try_recv_batch(&mut buffer) {
                Ok(count) => {
                    for i in 0..count {
                        assert_eq!(buffer[i], (total_received + i) as u64);
                    }
                    total_received += count;
                }
                Err(DynTryRecvError::Empty) => break,
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
        assert_eq!(total_received, 200);
    }

    #[test]
    fn test_partial_recv_batch() {
        let config = DynMpscConfig::default();
        let (sender, mut receiver) = dyn_unbounded_channel::<u64>(config);

        // Send only 3 items
        sender.send(1).unwrap();
        sender.send(2).unwrap();
        sender.send(3).unwrap();

        // Try to receive into larger buffer
        let mut buffer = [0u64; 10];
        let received = receiver.try_recv_batch(&mut buffer).unwrap();
        assert_eq!(received, 3);
        assert_eq!(buffer[0], 1);
        assert_eq!(buffer[1], 2);
        assert_eq!(buffer[2], 3);
    }
}
