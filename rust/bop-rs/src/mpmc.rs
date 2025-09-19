//! # MPMC Queue Wrapper
//!
//! High-performance multi-producer multi-consumer queue implementation using
//! the moodycamel C++ library through BOP's C API.
//!
//! This module provides safe Rust wrappers around the high-performance moodycamel
//! concurrent queue, offering both non-blocking and blocking variants.
//!
//! ## Features
//!
//! - **Lock-free**: Non-blocking operations for maximum performance
//! - **Thread-safe**: Multiple producers and consumers can operate concurrently
//! - **Token-based optimization**: Producer/consumer tokens for better performance
//! - **Bulk operations**: Efficient batch enqueue/dequeue operations
//! - **Blocking variant**: Optional blocking operations with timeout support
//! - **Memory efficient**: Zero-copy operations where possible
//!
//! ## Usage
//!
//! ### Basic Non-blocking Queue
//!
//! ```rust,no_run
//! use bop_rs::mpmc::MpmcQueue;
//!
//! let queue = MpmcQueue::new()?;
//!
//! // Producer thread
//! if queue.try_enqueue(42) {
//!     println!("Enqueued successfully");
//! }
//!
//! // Consumer thread
//! if let Some(item) = queue.try_dequeue() {
//!     println!("Dequeued: {}", item);
//! }
//! ```
//!
//! ### Token-based Optimization
//!
//! ```rust,no_run
//! use bop_rs::mpmc::{MpmcQueue, ProducerToken, ConsumerToken};
//!
//! let queue = MpmcQueue::new()?;
//! let producer_token = queue.create_producer_token()?;
//! let consumer_token = queue.create_consumer_token()?;
//!
//! // More efficient operations with tokens
//! producer_token.try_enqueue(42)?;
//! if let Some(item) = consumer_token.try_dequeue()? {
//!     println!("Got: {}", item);
//! }
//! ```
//!
//! ### Blocking Queue with Timeouts
//!
//! ```rust,no_run
//! use bop_rs::mpmc::BlockingMpmcQueue;
//! use std::time::Duration;
//!
//! let queue = BlockingMpmcQueue::new()?;
//!
//! // Blocks until item is available or timeout expires
//! if let Some(item) = queue.dequeue_wait(Duration::from_millis(1000))? {
//!     println!("Got item within timeout: {}", item);
//! }
//! ```

use bop_sys::*;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// MPMC queue specific errors
#[derive(Debug, Error)]
pub enum MpmcError {
    #[error("Failed to create MPMC queue")]
    CreationFailed,

    #[error("Failed to create producer token")]
    ProducerTokenCreationFailed,

    #[error("Failed to create consumer token")]
    ConsumerTokenCreationFailed,

    #[error("Queue operation failed")]
    OperationFailed,

    #[error("Invalid timeout value")]
    InvalidTimeout,
}

pub type MpmcResult<T> = Result<T, MpmcError>;

/// Low-level MPMC queue for u64 values
pub struct Queue {
    ptr: NonNull<bop_mpmc>,
}

unsafe impl Send for Queue {}
unsafe impl Sync for Queue {}

impl Queue {
    pub fn new() -> MpmcResult<Self> {
        let ptr = unsafe { bop_mpmc_create() };
        if ptr.is_null() {
            return Err(MpmcError::CreationFailed);
        }

        Ok(Queue {
            ptr: NonNull::new(ptr).unwrap(),
        })
    }

    pub fn size_approx(&self) -> usize {
        unsafe { bop_mpmc_size_approx(self.ptr.as_ptr()) }
    }

    pub fn try_enqueue(&self, item: u64) -> bool {
        unsafe { bop_mpmc_enqueue(self.ptr.as_ptr(), item) }
    }

    pub fn try_dequeue(&self) -> Option<u64> {
        let mut item: u64 = 0;
        let success = unsafe { bop_mpmc_dequeue(self.ptr.as_ptr(), &mut item as *mut u64) };

        if success { Some(item) } else { None }
    }

    /// Zero-copy bulk enqueue - directly uses the slice memory layout
    pub fn try_enqueue_bulk(&self, items: &[u64]) -> bool {
        if items.is_empty() {
            return true;
        }

        unsafe {
            bop_mpmc_enqueue_bulk(
                self.ptr.as_ptr(),
                items.as_ptr() as *mut u64, // Cast away const for C API
                items.len(),
            )
        }
    }

    /// Zero-copy bulk dequeue - directly uses the slice memory layout
    pub fn try_dequeue_bulk(&self, buffer: &mut [u64]) -> usize {
        if buffer.is_empty() {
            return 0;
        }

        unsafe {
            let result =
                bop_mpmc_dequeue_bulk(self.ptr.as_ptr(), buffer.as_mut_ptr(), buffer.len());
            result.max(0) as usize
        }
    }

    /// Create a producer token for optimized enqueue operations
    pub fn create_producer_token(&self) -> MpmcResult<ProducerToken<'_>> {
        ProducerToken::new(self)
    }

    /// Create a consumer token for optimized dequeue operations
    pub fn create_consumer_token(&self) -> MpmcResult<ConsumerToken<'_>> {
        ConsumerToken::new(self)
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        unsafe {
            bop_mpmc_destroy(self.ptr.as_ptr());
        }
    }
}

/// Producer token for optimized enqueue operations on Queue
pub struct ProducerToken<'a> {
    ptr: NonNull<bop_mpmc_producer_token>,
    _queue: PhantomData<&'a Queue>,
}

unsafe impl<'a> Send for ProducerToken<'a> {}

impl<'a> ProducerToken<'a> {
    fn new(queue: &'a Queue) -> MpmcResult<Self> {
        let ptr = unsafe { bop_mpmc_create_producer_token(queue.ptr.as_ptr()) };
        if ptr.is_null() {
            return Err(MpmcError::ProducerTokenCreationFailed);
        }

        Ok(ProducerToken {
            ptr: NonNull::new(ptr).unwrap(),
            _queue: PhantomData,
        })
    }

    /// Try to enqueue an item using this producer token (non-blocking)
    pub fn enqueue(&self, item: u64) -> bool {
        unsafe { bop_mpmc_enqueue_token(self.ptr.as_ptr(), item) }
    }

    /// Try to enqueue multiple items in bulk using this token (non-blocking)
    pub fn enqueue_bulk(&self, items: &[u64]) -> bool {
        if items.is_empty() {
            return true;
        }

        unsafe {
            bop_mpmc_enqueue_bulk_token(self.ptr.as_ptr(), items.as_ptr() as *mut u64, items.len())
        }
    }
}

impl<'a> Drop for ProducerToken<'a> {
    fn drop(&mut self) {
        unsafe {
            bop_mpmc_destroy_producer_token(self.ptr.as_ptr());
        }
    }
}

/// Consumer token for optimized dequeue operations on Queue
pub struct ConsumerToken<'a> {
    ptr: NonNull<bop_mpmc_consumer_token>,
    _queue: PhantomData<&'a Queue>,
}

unsafe impl<'a> Send for ConsumerToken<'a> {}

impl<'a> ConsumerToken<'a> {
    fn new(queue: &'a Queue) -> MpmcResult<Self> {
        let ptr = unsafe { bop_mpmc_create_consumer_token(queue.ptr.as_ptr()) };
        if ptr.is_null() {
            return Err(MpmcError::ConsumerTokenCreationFailed);
        }

        Ok(ConsumerToken {
            ptr: NonNull::new(ptr).unwrap(),
            _queue: PhantomData,
        })
    }

    /// Try to dequeue an item using this consumer token (non-blocking)
    pub fn dequeue(&self) -> Option<u64> {
        let mut item: u64 = 0;
        let success = unsafe { bop_mpmc_dequeue_token(self.ptr.as_ptr(), &mut item as *mut u64) };

        if success { Some(item) } else { None }
    }

    /// Try to dequeue multiple items in bulk using this token (non-blocking)
    pub fn dequeue_bulk(&self, buffer: &mut [u64]) -> usize {
        if buffer.is_empty() {
            return 0;
        }

        unsafe {
            let result =
                bop_mpmc_dequeue_bulk_token(self.ptr.as_ptr(), buffer.as_mut_ptr(), buffer.len());
            result.max(0) as usize
        }
    }
}

impl<'a> Drop for ConsumerToken<'a> {
    fn drop(&mut self) {
        unsafe {
            bop_mpmc_destroy_consumer_token(self.ptr.as_ptr());
        }
    }
}

/// High-performance queue for `Box<T>` items
pub struct BoxQueue<T> {
    queue: Queue,
    _phantom: PhantomData<T>,
}

unsafe impl<T: Send> Send for BoxQueue<T> {}
unsafe impl<T: Send> Sync for BoxQueue<T> {}

impl<T: Send + 'static> BoxQueue<T> {
    /// Create a new BoxQueue
    pub fn new() -> MpmcResult<Self> {
        Ok(Self {
            queue: Queue::new()?,
            _phantom: PhantomData,
        })
    }

    /// Get approximate size of the queue
    pub fn size_approx(&self) -> usize {
        self.queue.size_approx()
    }

    /// Enqueue a boxed item (transfers ownership)
    pub fn enqueue(&self, boxed: Box<T>) -> bool {
        let ptr = Box::into_raw(boxed) as u64;
        self.queue.try_enqueue(ptr)
    }

    /// Dequeue as a boxed item
    pub fn dequeue(&self) -> Option<Box<T>> {
        self.queue
            .try_dequeue()
            .map(|ptr| unsafe { Box::from_raw(ptr as *mut T) })
    }

    /// Zero-copy bulk enqueue - directly converts slice of Box<T> to slice of u64
    ///
    /// On 64-bit systems, `&[Box<T>]` has the same memory layout as `&[u64]`
    pub fn enqueue_bulk(&self, boxes: Vec<Box<T>>) -> bool {
        if boxes.is_empty() {
            return true;
        }

        // Convert Vec<Box<T>> to Vec<u64> by transmuting pointers
        let ptrs: Vec<u64> = boxes
            .into_iter()
            .map(|boxed| Box::into_raw(boxed) as u64)
            .collect();

        if self.queue.try_enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs); // Don't drop the ptrs as they're now owned by the queue
            true
        } else {
            // Failed to enqueue - need to reclaim ownership to prevent leaks
            for ptr in ptrs {
                unsafe { drop(Box::from_raw(ptr as *mut T)) };
            }
            false
        }
    }

    /// Zero-copy bulk dequeue - up to buffer.len() items
    pub fn dequeue_bulk(&self, max_items: usize) -> Vec<Box<T>> {
        if max_items == 0 {
            return Vec::new();
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.queue.try_dequeue_bulk(&mut buffer);

        // Convert u64 pointers back to Box<T>
        buffer.truncate(count);
        buffer
            .into_iter()
            .map(|ptr| unsafe { Box::from_raw(ptr as *mut T) })
            .collect()
    }

    /// Create a producer token for optimized enqueue operations
    pub fn create_producer_token(&self) -> MpmcResult<BoxProducerToken<'_, T>> {
        BoxProducerToken::new(&self.queue)
    }

    /// Create a consumer token for optimized dequeue operations
    pub fn create_consumer_token(&self) -> MpmcResult<BoxConsumerToken<'_, T>> {
        BoxConsumerToken::new(&self.queue)
    }
}

impl<T> Drop for BoxQueue<T> {
    fn drop(&mut self) {
        // Clean up any remaining items to prevent leaks
        while let Some(ptr) = self.queue.try_dequeue() {
            unsafe { drop(Box::from_raw(ptr as *mut T)) };
        }
    }
}

/// Producer token for optimized enqueue operations on BoxQueue
pub struct BoxProducerToken<'a, T> {
    token: ProducerToken<'a>,
    _phantom: PhantomData<T>,
}

impl<'a, T: Send + 'static> BoxProducerToken<'a, T> {
    fn new(queue: &'a Queue) -> MpmcResult<Self> {
        Ok(Self {
            token: ProducerToken::new(queue)?,
            _phantom: PhantomData,
        })
    }

    /// Try to enqueue a Box with producer token (optimized)
    pub fn try_enqueue(&self, item: Box<T>) -> bool {
        let ptr = Box::into_raw(item) as u64;
        if self.token.enqueue(ptr) {
            true
        } else {
            // Failed to enqueue - reclaim ownership
            unsafe { drop(Box::from_raw(ptr as *mut T)) };
            false
        }
    }

    /// Try to enqueue multiple Box items with producer token (zero-copy)
    pub fn try_enqueue_bulk(&self, items: Vec<Box<T>>) -> bool {
        if items.is_empty() {
            return true;
        }

        // Convert Vec<Box<T>> to Vec<u64> without allocation
        let ptrs = unsafe {
            let mut ptrs = std::mem::ManuallyDrop::new(items);
            Vec::from_raw_parts(ptrs.as_mut_ptr() as *mut u64, ptrs.len(), ptrs.capacity())
        };

        if self.token.enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs);
            true
        } else {
            // Failed to enqueue - need to reclaim ownership
            for ptr in ptrs {
                unsafe { drop(Box::from_raw(ptr as *mut T)) };
            }
            false
        }
    }
}

/// Consumer token for optimized dequeue operations on BoxQueue
pub struct BoxConsumerToken<'a, T> {
    token: ConsumerToken<'a>,
    _phantom: PhantomData<T>,
}

impl<'a, T: Send + 'static> BoxConsumerToken<'a, T> {
    fn new(queue: &'a Queue) -> MpmcResult<Self> {
        Ok(Self {
            token: ConsumerToken::new(queue)?,
            _phantom: PhantomData,
        })
    }

    /// Try to dequeue a Box with consumer token (optimized)
    pub fn try_dequeue(&self) -> Option<Box<T>> {
        self.token
            .dequeue()
            .map(|ptr| unsafe { Box::from_raw(ptr as *mut T) })
    }

    /// Try to dequeue multiple Box items with consumer token (zero-copy)
    pub fn try_dequeue_bulk(&self, max_items: usize) -> Vec<Box<T>> {
        if max_items == 0 {
            return Vec::new();
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.token.dequeue_bulk(&mut buffer);

        // Convert u64 pointers back to Box<T>
        buffer.truncate(count);
        buffer
            .into_iter()
            .map(|ptr| unsafe { Box::from_raw(ptr as *mut T) })
            .collect()
    }
}

/// High-performance queue for `Arc<T>` items
pub struct ArcQueue<T> {
    queue: Queue,
    _phantom: PhantomData<T>,
}

unsafe impl<T: Send + Sync> Send for ArcQueue<T> {}
unsafe impl<T: Send + Sync> Sync for ArcQueue<T> {}

impl<T: Send + Sync + 'static> ArcQueue<T> {
    /// Create a new ArcQueue
    pub fn new() -> MpmcResult<Self> {
        Ok(Self {
            queue: Queue::new()?,
            _phantom: PhantomData,
        })
    }

    /// Get approximate size of the queue
    pub fn size_approx(&self) -> usize {
        self.queue.size_approx()
    }

    /// Enqueue an Arc (transfers ownership)
    pub fn enqueue(&self, arc: Arc<T>) -> bool {
        let ptr = Arc::into_raw(arc) as u64;
        self.queue.try_enqueue(ptr)
    }

    /// Enqueue a clone of an Arc (increments reference count)
    pub fn enqueue_cloned(&self, arc: &Arc<T>) -> bool {
        self.enqueue(Arc::clone(arc))
    }

    /// Dequeue as an Arc
    pub fn dequeue(&self) -> Option<Arc<T>> {
        self.queue
            .try_dequeue()
            .map(|ptr| unsafe { Arc::from_raw(ptr as *const T) })
    }

    /// Zero-copy bulk enqueue of Arc clones
    pub fn enqueue_bulk_cloned(&self, arcs: &[Arc<T>]) -> bool {
        if arcs.is_empty() {
            return true;
        }

        // Clone all Arcs and convert to u64 pointers
        let ptrs: Vec<u64> = arcs
            .iter()
            .map(|arc| Arc::into_raw(Arc::clone(arc)) as u64)
            .collect();

        if self.queue.try_enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs);
            true
        } else {
            // Failed to enqueue - need to reclaim ownership
            for ptr in ptrs {
                unsafe { drop(Arc::from_raw(ptr as *const T)) };
            }
            false
        }
    }

    /// Zero-copy bulk dequeue - up to max_items
    pub fn dequeue_bulk(&self, max_items: usize) -> Vec<Arc<T>> {
        if max_items == 0 {
            return Vec::new();
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.queue.try_dequeue_bulk(&mut buffer);

        // Convert u64 pointers back to Arc<T>
        buffer.truncate(count);
        buffer
            .into_iter()
            .map(|ptr| unsafe { Arc::from_raw(ptr as *const T) })
            .collect()
    }

    /// Create a producer token for optimized enqueue operations
    pub fn create_producer_token(&self) -> MpmcResult<ArcProducerToken<'_, T>> {
        ArcProducerToken::new(&self.queue)
    }

    /// Create a consumer token for optimized dequeue operations
    pub fn create_consumer_token(&self) -> MpmcResult<ArcConsumerToken<'_, T>> {
        ArcConsumerToken::new(&self.queue)
    }
}

impl<T> Drop for ArcQueue<T> {
    fn drop(&mut self) {
        // Clean up any remaining items to prevent leaks
        while let Some(ptr) = self.queue.try_dequeue() {
            unsafe { drop(Arc::from_raw(ptr as *const T)) };
        }
    }
}

/// Producer token for optimized enqueue operations on ArcQueue
pub struct ArcProducerToken<'a, T> {
    token: ProducerToken<'a>,
    _phantom: PhantomData<T>,
}

impl<'a, T: Send + Sync + 'static> ArcProducerToken<'a, T> {
    fn new(queue: &'a Queue) -> MpmcResult<Self> {
        Ok(Self {
            token: ProducerToken::new(queue)?,
            _phantom: PhantomData,
        })
    }

    /// Try to enqueue an Arc with producer token (optimized)
    pub fn try_enqueue(&self, arc: Arc<T>) -> bool {
        let ptr = Arc::into_raw(arc) as u64;
        if self.token.enqueue(ptr) {
            true
        } else {
            // Failed to enqueue - reclaim ownership
            unsafe { drop(Arc::from_raw(ptr as *const T)) };
            false
        }
    }

    /// Try to enqueue a clone of an Arc with producer token
    pub fn try_enqueue_cloned(&self, arc: &Arc<T>) -> bool {
        self.try_enqueue(Arc::clone(arc))
    }

    /// Try to enqueue multiple Arc clones with producer token (zero-copy)
    pub fn try_enqueue_bulk_cloned(&self, arcs: &[Arc<T>]) -> bool {
        if arcs.is_empty() {
            return true;
        }

        // Clone all Arcs and convert to u64 pointers
        let ptrs: Vec<u64> = arcs
            .iter()
            .map(|arc| Arc::into_raw(Arc::clone(arc)) as u64)
            .collect();

        if self.token.enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs);
            true
        } else {
            // Failed to enqueue - need to reclaim ownership
            for ptr in ptrs {
                unsafe { drop(Arc::from_raw(ptr as *const T)) };
            }
            false
        }
    }
}

/// Consumer token for optimized dequeue operations on ArcQueue
pub struct ArcConsumerToken<'a, T> {
    token: ConsumerToken<'a>,
    _phantom: PhantomData<T>,
}

impl<'a, T: Send + Sync + 'static> ArcConsumerToken<'a, T> {
    fn new(queue: &'a Queue) -> MpmcResult<Self> {
        Ok(Self {
            token: ConsumerToken::new(queue)?,
            _phantom: PhantomData,
        })
    }

    /// Try to dequeue an Arc with consumer token (optimized)
    pub fn try_dequeue(&self) -> Option<Arc<T>> {
        self.token
            .dequeue()
            .map(|ptr| unsafe { Arc::from_raw(ptr as *const T) })
    }

    /// Try to dequeue multiple Arc items with consumer token (zero-copy)
    pub fn try_dequeue_bulk(&self, max_items: usize) -> Vec<Arc<T>> {
        if max_items == 0 {
            return Vec::new();
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.token.dequeue_bulk(&mut buffer);

        // Convert u64 pointers back to Arc<T>
        buffer.truncate(count);
        buffer
            .into_iter()
            .map(|ptr| unsafe { Arc::from_raw(ptr as *const T) })
            .collect()
    }
}

/// Low-level blocking MPMC queue for u64 values
pub struct BlockingQueue {
    ptr: NonNull<bop_mpmc_blocking>,
}

unsafe impl Send for BlockingQueue {}
unsafe impl Sync for BlockingQueue {}

impl BlockingQueue {
    pub fn new() -> MpmcResult<Self> {
        let ptr = unsafe { bop_mpmc_blocking_create() };
        if ptr.is_null() {
            return Err(MpmcError::CreationFailed);
        }

        Ok(BlockingQueue {
            ptr: NonNull::new(ptr).unwrap(),
        })
    }

    pub fn size_approx(&self) -> usize {
        unsafe { bop_mpmc_blocking_size_approx(self.ptr.as_ptr()) }
    }

    pub fn enqueue(&self, item: u64) -> bool {
        unsafe { bop_mpmc_blocking_enqueue(self.ptr.as_ptr(), item) }
    }

    pub fn dequeue(&self) -> Option<u64> {
        let mut item: u64 = 0;
        let success =
            unsafe { bop_mpmc_blocking_dequeue(self.ptr.as_ptr(), &mut item as *mut u64) };

        if success { Some(item) } else { None }
    }

    pub fn dequeue_wait(&self, timeout: Duration) -> MpmcResult<Option<u64>> {
        let timeout_ms = timeout.as_millis() as u64;
        if timeout_ms > u64::MAX {
            return Err(MpmcError::InvalidTimeout);
        }

        let mut item: u64 = 0;
        let result = unsafe {
            bop_mpmc_blocking_dequeue_wait(
                self.ptr.as_ptr(),
                &mut item as *mut u64,
                timeout_ms as i64,
            )
        };

        if result {
            Ok(Some(item))
        } else {
            Ok(None) // Timeout or failure
        }
    }

    /// Zero-copy bulk enqueue - directly uses the slice memory layout
    pub fn enqueue_bulk(&self, items: &[u64]) -> bool {
        if items.is_empty() {
            return true;
        }

        unsafe {
            bop_mpmc_blocking_enqueue_bulk(
                self.ptr.as_ptr(),
                items.as_ptr() as *mut u64, // Cast away const for C API
                items.len(),
            )
        }
    }

    /// Zero-copy bulk dequeue - directly uses the slice memory layout
    pub fn dequeue_bulk(&self, buffer: &mut [u64]) -> usize {
        if buffer.is_empty() {
            return 0;
        }

        unsafe {
            let result = bop_mpmc_blocking_dequeue_bulk(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
            );
            result.max(0) as usize
        }
    }

    /// Bulk dequeue with timeout
    pub fn dequeue_bulk_wait(&self, buffer: &mut [u64], timeout: Duration) -> MpmcResult<usize> {
        let timeout_ms = timeout.as_millis() as u64;
        if timeout_ms > u64::MAX {
            return Err(MpmcError::InvalidTimeout);
        }

        if buffer.is_empty() {
            return Ok(0);
        }

        unsafe {
            let result = bop_mpmc_blocking_dequeue_bulk_wait(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                timeout_ms as i64,
            );
            Ok(result.max(0) as usize)
        }
    }

    /// Create a producer token for optimized enqueue operations
    pub fn create_producer_token(&self) -> MpmcResult<BlockingProducerToken<'_>> {
        BlockingProducerToken::new(self)
    }

    /// Create a consumer token for optimized dequeue operations
    pub fn create_consumer_token(&self) -> MpmcResult<BlockingConsumerToken<'_>> {
        BlockingConsumerToken::new(self)
    }
}

impl Drop for BlockingQueue {
    fn drop(&mut self) {
        unsafe {
            bop_mpmc_blocking_destroy(self.ptr.as_ptr());
        }
    }
}

/// Producer token for optimized enqueue operations on BlockingQueue
pub struct BlockingProducerToken<'a> {
    ptr: NonNull<bop_mpmc_blocking_producer_token>,
    _queue: PhantomData<&'a BlockingQueue>,
}

unsafe impl Send for BlockingProducerToken<'_> {}
unsafe impl Sync for BlockingProducerToken<'_> {}

impl<'a> BlockingProducerToken<'a> {
    fn new(queue: &'a BlockingQueue) -> MpmcResult<Self> {
        let ptr = unsafe { bop_mpmc_blocking_create_producer_token(queue.ptr.as_ptr()) };
        if ptr.is_null() {
            return Err(MpmcError::CreationFailed);
        }

        Ok(BlockingProducerToken {
            ptr: NonNull::new(ptr).unwrap(),
            _queue: PhantomData,
        })
    }

    /// Enqueue item with producer token (blocking)
    pub fn enqueue(&self, item: u64) -> bool {
        unsafe { bop_mpmc_blocking_enqueue_token(self.ptr.as_ptr(), item) }
    }

    /// Bulk enqueue with producer token (blocking)
    pub fn enqueue_bulk(&self, items: &[u64]) -> bool {
        if items.is_empty() {
            return true;
        }

        unsafe {
            bop_mpmc_blocking_enqueue_bulk_token(
                self.ptr.as_ptr(),
                items.as_ptr() as *mut u64,
                items.len(),
            )
        }
    }

    /// Try bulk enqueue with producer token (non-blocking)
    pub fn try_enqueue_bulk(&self, items: &[u64]) -> bool {
        if items.is_empty() {
            return true;
        }

        unsafe {
            bop_mpmc_blocking_try_enqueue_bulk_token(
                self.ptr.as_ptr(),
                items.as_ptr() as *mut u64,
                items.len(),
            )
        }
    }
}

impl Drop for BlockingProducerToken<'_> {
    fn drop(&mut self) {
        unsafe {
            bop_mpmc_blocking_destroy_producer_token(self.ptr.as_ptr());
        }
    }
}

/// Consumer token for optimized dequeue operations on BlockingQueue
pub struct BlockingConsumerToken<'a> {
    ptr: NonNull<bop_mpmc_blocking_consumer_token>,
    _queue: PhantomData<&'a BlockingQueue>,
}

unsafe impl Send for BlockingConsumerToken<'_> {}
unsafe impl Sync for BlockingConsumerToken<'_> {}

impl<'a> BlockingConsumerToken<'a> {
    fn new(queue: &'a BlockingQueue) -> MpmcResult<Self> {
        let ptr = unsafe { bop_mpmc_blocking_create_consumer_token(queue.ptr.as_ptr()) };
        if ptr.is_null() {
            return Err(MpmcError::CreationFailed);
        }

        Ok(BlockingConsumerToken {
            ptr: NonNull::new(ptr).unwrap(),
            _queue: PhantomData,
        })
    }

    /// Dequeue item with consumer token (blocking)
    pub fn dequeue(&self) -> Option<u64> {
        let mut item: u64 = 0;
        let success =
            unsafe { bop_mpmc_blocking_dequeue_token(self.ptr.as_ptr(), &mut item as *mut u64) };

        if success { Some(item) } else { None }
    }

    /// Dequeue item with timeout using consumer token
    pub fn dequeue_wait(&self, timeout: Duration) -> MpmcResult<Option<u64>> {
        let timeout_ms = timeout.as_millis() as u64;
        if timeout_ms > u64::MAX {
            return Err(MpmcError::InvalidTimeout);
        }

        let mut item: u64 = 0;
        let result = unsafe {
            bop_mpmc_blocking_dequeue_wait_token(
                self.ptr.as_ptr(),
                &mut item as *mut u64,
                timeout_ms as i64,
            )
        };

        if result {
            Ok(Some(item))
        } else {
            Ok(None) // Timeout or failure
        }
    }

    /// Bulk dequeue with consumer token (blocking)
    pub fn dequeue_bulk(&self, buffer: &mut [u64]) -> usize {
        if buffer.is_empty() {
            return 0;
        }

        unsafe {
            let result = bop_mpmc_blocking_dequeue_bulk_token(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
            );
            result.max(0) as usize
        }
    }

    /// Bulk dequeue with timeout using consumer token
    pub fn dequeue_bulk_wait(&self, buffer: &mut [u64], timeout: Duration) -> MpmcResult<usize> {
        let timeout_ms = timeout.as_millis() as u64;
        if timeout_ms > u64::MAX {
            return Err(MpmcError::InvalidTimeout);
        }

        if buffer.is_empty() {
            return Ok(0);
        }

        unsafe {
            let result = bop_mpmc_blocking_dequeue_bulk_wait_token(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                timeout_ms as i64,
            );
            Ok(result.max(0) as usize)
        }
    }
}

impl Drop for BlockingConsumerToken<'_> {
    fn drop(&mut self) {
        unsafe {
            bop_mpmc_blocking_destroy_consumer_token(self.ptr.as_ptr());
        }
    }
}

/// Blocking variant of BoxQueue
pub struct BlockingBoxQueue<T> {
    queue: BlockingQueue,
    _phantom: PhantomData<T>,
}

unsafe impl<T: Send> Send for BlockingBoxQueue<T> {}
unsafe impl<T: Send> Sync for BlockingBoxQueue<T> {}

impl<T: Send + 'static> BlockingBoxQueue<T> {
    /// Create a new blocking BoxQueue
    pub fn new() -> MpmcResult<Self> {
        Ok(Self {
            queue: BlockingQueue::new()?,
            _phantom: PhantomData,
        })
    }

    /// Enqueue a boxed item (non-blocking)
    pub fn enqueue(&self, boxed: Box<T>) -> bool {
        let ptr = Box::into_raw(boxed) as u64;
        self.queue.enqueue(ptr)
    }

    /// Dequeue with timeout
    pub fn dequeue_wait(&self, timeout: Duration) -> MpmcResult<Option<Box<T>>> {
        self.queue
            .dequeue_wait(timeout)
            .map(|opt| opt.map(|ptr| unsafe { Box::from_raw(ptr as *mut T) }))
    }

    /// Zero-copy bulk enqueue - directly converts slice of Box<T> to slice of u64
    pub fn enqueue_bulk(&self, boxes: Vec<Box<T>>) -> bool {
        if boxes.is_empty() {
            return true;
        }

        // Convert Vec<Box<T>> to Vec<u64> by transmuting pointers
        let ptrs: Vec<u64> = boxes
            .into_iter()
            .map(|boxed| Box::into_raw(boxed) as u64)
            .collect();

        if self.queue.enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs); // Don't drop the ptrs as they're now owned by the queue
            true
        } else {
            // Failed to enqueue - need to reclaim ownership to prevent leaks
            for ptr in ptrs {
                unsafe { drop(Box::from_raw(ptr as *mut T)) };
            }
            false
        }
    }

    /// Zero-copy bulk dequeue - up to max_items
    pub fn dequeue_bulk(&self, max_items: usize) -> Vec<Box<T>> {
        if max_items == 0 {
            return Vec::new();
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.queue.dequeue_bulk(&mut buffer);

        // Convert u64 pointers back to Box<T>
        buffer.truncate(count);
        buffer
            .into_iter()
            .map(|ptr| unsafe { Box::from_raw(ptr as *mut T) })
            .collect()
    }

    /// Bulk dequeue with timeout
    pub fn dequeue_bulk_wait(
        &self,
        max_items: usize,
        timeout: Duration,
    ) -> MpmcResult<Vec<Box<T>>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.queue.dequeue_bulk_wait(&mut buffer, timeout)?;

        // Convert u64 pointers back to Box<T>
        buffer.truncate(count);
        Ok(buffer
            .into_iter()
            .map(|ptr| unsafe { Box::from_raw(ptr as *mut T) })
            .collect())
    }

    /// Create a producer token for optimized enqueue operations
    pub fn create_producer_token(&self) -> MpmcResult<BlockingBoxProducerToken<'_, T>> {
        BlockingBoxProducerToken::new(&self.queue)
    }

    /// Create a consumer token for optimized dequeue operations
    pub fn create_consumer_token(&self) -> MpmcResult<BlockingBoxConsumerToken<'_, T>> {
        BlockingBoxConsumerToken::new(&self.queue)
    }
}

impl<T> Drop for BlockingBoxQueue<T> {
    fn drop(&mut self) {
        // Clean up any remaining items
        while let Some(ptr) = self.queue.dequeue() {
            unsafe { drop(Box::from_raw(ptr as *mut T)) };
        }
    }
}

/// Producer token for optimized enqueue operations on BlockingBoxQueue
pub struct BlockingBoxProducerToken<'a, T> {
    token: BlockingProducerToken<'a>,
    _phantom: PhantomData<T>,
}

impl<'a, T: Send + 'static> BlockingBoxProducerToken<'a, T> {
    fn new(queue: &'a BlockingQueue) -> MpmcResult<Self> {
        Ok(Self {
            token: BlockingProducerToken::new(queue)?,
            _phantom: PhantomData,
        })
    }

    /// Enqueue a Box with producer token (blocking)
    pub fn enqueue(&self, item: Box<T>) -> bool {
        let ptr = Box::into_raw(item) as u64;
        if self.token.enqueue(ptr) {
            true
        } else {
            // Failed to enqueue - reclaim ownership
            unsafe { drop(Box::from_raw(ptr as *mut T)) };
            false
        }
    }

    /// Enqueue multiple Box items with producer token (blocking)
    pub fn enqueue_bulk(&self, items: Vec<Box<T>>) -> bool {
        if items.is_empty() {
            return true;
        }

        // Convert Vec<Box<T>> to Vec<u64> without allocation
        let ptrs: Vec<u64> = items
            .into_iter()
            .map(|boxed| Box::into_raw(boxed) as u64)
            .collect();

        if self.token.enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs);
            true
        } else {
            // Failed to enqueue - need to reclaim ownership
            for ptr in ptrs {
                unsafe { drop(Box::from_raw(ptr as *mut T)) };
            }
            false
        }
    }

    /// Try enqueue multiple Box items with producer token (non-blocking)
    pub fn try_enqueue_bulk(&self, items: Vec<Box<T>>) -> bool {
        if items.is_empty() {
            return true;
        }

        // Convert Vec<Box<T>> to Vec<u64> without allocation
        let ptrs: Vec<u64> = items
            .into_iter()
            .map(|boxed| Box::into_raw(boxed) as u64)
            .collect();

        if self.token.enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs);
            true
        } else {
            // Failed to enqueue - need to reclaim ownership
            for ptr in ptrs {
                unsafe { drop(Box::from_raw(ptr as *mut T)) };
            }
            false
        }
    }
}

/// Consumer token for optimized dequeue operations on BlockingBoxQueue
pub struct BlockingBoxConsumerToken<'a, T> {
    token: BlockingConsumerToken<'a>,
    _phantom: PhantomData<T>,
}

impl<'a, T: Send + 'static> BlockingBoxConsumerToken<'a, T> {
    fn new(queue: &'a BlockingQueue) -> MpmcResult<Self> {
        Ok(Self {
            token: BlockingConsumerToken::new(queue)?,
            _phantom: PhantomData,
        })
    }

    /// Dequeue a Box with consumer token (blocking)
    pub fn dequeue(&self) -> Option<Box<T>> {
        self.token
            .dequeue()
            .map(|ptr| unsafe { Box::from_raw(ptr as *mut T) })
    }

    /// Dequeue a Box with timeout using consumer token
    pub fn dequeue_wait(&self, timeout: Duration) -> MpmcResult<Option<Box<T>>> {
        self.token
            .dequeue_wait(timeout)
            .map(|opt| opt.map(|ptr| unsafe { Box::from_raw(ptr as *mut T) }))
    }

    /// Dequeue multiple Box items with consumer token (blocking)
    pub fn dequeue_bulk(&self, max_items: usize) -> Vec<Box<T>> {
        if max_items == 0 {
            return Vec::new();
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.token.dequeue_bulk(&mut buffer);

        // Convert u64 pointers back to Box<T>
        buffer.truncate(count);
        buffer
            .into_iter()
            .map(|ptr| unsafe { Box::from_raw(ptr as *mut T) })
            .collect()
    }

    /// Dequeue multiple Box items with timeout using consumer token
    pub fn dequeue_bulk_wait(
        &self,
        max_items: usize,
        timeout: Duration,
    ) -> MpmcResult<Vec<Box<T>>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.token.dequeue_bulk_wait(&mut buffer, timeout)?;

        // Convert u64 pointers back to Box<T>
        buffer.truncate(count);
        Ok(buffer
            .into_iter()
            .map(|ptr| unsafe { Box::from_raw(ptr as *mut T) })
            .collect())
    }
}

/// Blocking variant of ArcQueue  
pub struct BlockingArcQueue<T> {
    queue: BlockingQueue,
    _phantom: PhantomData<T>,
}

unsafe impl<T: Send + Sync> Send for BlockingArcQueue<T> {}
unsafe impl<T: Send + Sync> Sync for BlockingArcQueue<T> {}

impl<T: Send + Sync + 'static> BlockingArcQueue<T> {
    /// Create a new blocking ArcQueue
    pub fn new() -> MpmcResult<Self> {
        Ok(Self {
            queue: BlockingQueue::new()?,
            _phantom: PhantomData,
        })
    }

    /// Enqueue an Arc (non-blocking)
    pub fn enqueue(&self, arc: Arc<T>) -> bool {
        let ptr = Arc::into_raw(arc) as u64;
        self.queue.enqueue(ptr)
    }

    /// Enqueue a cloned Arc (non-blocking)
    pub fn enqueue_cloned(&self, arc: &Arc<T>) -> bool {
        self.enqueue(Arc::clone(arc))
    }

    /// Dequeue with timeout
    pub fn dequeue_wait(&self, timeout: Duration) -> MpmcResult<Option<Arc<T>>> {
        self.queue
            .dequeue_wait(timeout)
            .map(|opt| opt.map(|ptr| unsafe { Arc::from_raw(ptr as *const T) }))
    }

    /// Zero-copy bulk enqueue of Arc clones
    pub fn enqueue_bulk_cloned(&self, arcs: &[Arc<T>]) -> bool {
        if arcs.is_empty() {
            return true;
        }

        // Clone all Arcs and convert to u64 pointers
        let ptrs: Vec<u64> = arcs
            .iter()
            .map(|arc| Arc::into_raw(Arc::clone(arc)) as u64)
            .collect();

        if self.queue.enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs);
            true
        } else {
            // Failed to enqueue - need to reclaim ownership
            for ptr in ptrs {
                unsafe { drop(Arc::from_raw(ptr as *const T)) };
            }
            false
        }
    }

    /// Zero-copy bulk dequeue - up to max_items
    pub fn dequeue_bulk(&self, max_items: usize) -> Vec<Arc<T>> {
        if max_items == 0 {
            return Vec::new();
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.queue.dequeue_bulk(&mut buffer);

        // Convert u64 pointers back to Arc<T>
        buffer.truncate(count);
        buffer
            .into_iter()
            .map(|ptr| unsafe { Arc::from_raw(ptr as *const T) })
            .collect()
    }

    /// Bulk dequeue with timeout
    pub fn dequeue_bulk_wait(
        &self,
        max_items: usize,
        timeout: Duration,
    ) -> MpmcResult<Vec<Arc<T>>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.queue.dequeue_bulk_wait(&mut buffer, timeout)?;

        // Convert u64 pointers back to Arc<T>
        buffer.truncate(count);
        Ok(buffer
            .into_iter()
            .map(|ptr| unsafe { Arc::from_raw(ptr as *const T) })
            .collect())
    }

    /// Create a producer token for optimized enqueue operations
    pub fn create_producer_token(&self) -> MpmcResult<BlockingArcProducerToken<'_, T>> {
        BlockingArcProducerToken::new(&self.queue)
    }

    /// Create a consumer token for optimized dequeue operations
    pub fn create_consumer_token(&self) -> MpmcResult<BlockingArcConsumerToken<'_, T>> {
        BlockingArcConsumerToken::new(&self.queue)
    }
}

impl<T> Drop for BlockingArcQueue<T> {
    fn drop(&mut self) {
        // Clean up any remaining items
        while let Some(ptr) = self.queue.dequeue() {
            unsafe { drop(Arc::from_raw(ptr as *const T)) };
        }
    }
}

/// Producer token for optimized enqueue operations on BlockingArcQueue
pub struct BlockingArcProducerToken<'a, T> {
    token: BlockingProducerToken<'a>,
    _phantom: PhantomData<T>,
}

impl<'a, T: Send + Sync + 'static> BlockingArcProducerToken<'a, T> {
    fn new(queue: &'a BlockingQueue) -> MpmcResult<Self> {
        Ok(Self {
            token: BlockingProducerToken::new(queue)?,
            _phantom: PhantomData,
        })
    }

    /// Enqueue an Arc with producer token (blocking)
    pub fn enqueue(&self, arc: Arc<T>) -> bool {
        let ptr = Arc::into_raw(arc) as u64;
        if self.token.enqueue(ptr) {
            true
        } else {
            // Failed to enqueue - reclaim ownership
            unsafe { drop(Arc::from_raw(ptr as *const T)) };
            false
        }
    }

    /// Enqueue a clone of an Arc with producer token (blocking)
    pub fn enqueue_cloned(&self, arc: &Arc<T>) -> bool {
        self.enqueue(Arc::clone(arc))
    }

    /// Enqueue multiple Arc clones with producer token (blocking)
    pub fn enqueue_bulk_cloned(&self, arcs: &[Arc<T>]) -> bool {
        if arcs.is_empty() {
            return true;
        }

        // Clone all Arcs and convert to u64 pointers
        let ptrs: Vec<u64> = arcs
            .iter()
            .map(|arc| Arc::into_raw(Arc::clone(arc)) as u64)
            .collect();

        if self.token.enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs);
            true
        } else {
            // Failed to enqueue - need to reclaim ownership
            for ptr in ptrs {
                unsafe { drop(Arc::from_raw(ptr as *const T)) };
            }
            false
        }
    }

    /// Try enqueue multiple Arc clones with producer token (non-blocking)
    pub fn try_enqueue_bulk_cloned(&self, arcs: &[Arc<T>]) -> bool {
        if arcs.is_empty() {
            return true;
        }

        // Clone all Arcs and convert to u64 pointers
        let ptrs: Vec<u64> = arcs
            .iter()
            .map(|arc| Arc::into_raw(Arc::clone(arc)) as u64)
            .collect();

        if self.token.enqueue_bulk(&ptrs) {
            // Successfully enqueued all items
            std::mem::forget(ptrs);
            true
        } else {
            // Failed to enqueue - need to reclaim ownership
            for ptr in ptrs {
                unsafe { drop(Arc::from_raw(ptr as *const T)) };
            }
            false
        }
    }
}

/// Consumer token for optimized dequeue operations on BlockingArcQueue
pub struct BlockingArcConsumerToken<'a, T> {
    token: BlockingConsumerToken<'a>,
    _phantom: PhantomData<T>,
}

impl<'a, T: Send + Sync + 'static> BlockingArcConsumerToken<'a, T> {
    fn new(queue: &'a BlockingQueue) -> MpmcResult<Self> {
        Ok(Self {
            token: BlockingConsumerToken::new(queue)?,
            _phantom: PhantomData,
        })
    }

    /// Dequeue an Arc with consumer token (blocking)
    pub fn dequeue(&self) -> Option<Arc<T>> {
        self.token
            .dequeue()
            .map(|ptr| unsafe { Arc::from_raw(ptr as *const T) })
    }

    /// Dequeue an Arc with timeout using consumer token
    pub fn dequeue_wait(&self, timeout: Duration) -> MpmcResult<Option<Arc<T>>> {
        self.token
            .dequeue_wait(timeout)
            .map(|opt| opt.map(|ptr| unsafe { Arc::from_raw(ptr as *const T) }))
    }

    /// Dequeue multiple Arc items with consumer token (blocking)
    pub fn dequeue_bulk(&self, max_items: usize) -> Vec<Arc<T>> {
        if max_items == 0 {
            return Vec::new();
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.token.dequeue_bulk(&mut buffer);

        // Convert u64 pointers back to Arc<T>
        buffer.truncate(count);
        buffer
            .into_iter()
            .map(|ptr| unsafe { Arc::from_raw(ptr as *const T) })
            .collect()
    }

    /// Dequeue multiple Arc items with timeout using consumer token
    pub fn dequeue_bulk_wait(
        &self,
        max_items: usize,
        timeout: Duration,
    ) -> MpmcResult<Vec<Arc<T>>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u64; max_items];
        let count = self.token.dequeue_bulk_wait(&mut buffer, timeout)?;

        // Convert u64 pointers back to Arc<T>
        buffer.truncate(count);
        Ok(buffer
            .into_iter()
            .map(|ptr| unsafe { Arc::from_raw(ptr as *const T) })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::thread;
    use std::sync::Arc as StdArc;
    use std::time::Duration;

    #[test]
    fn test_box_queue_basic() {
        let queue: BoxQueue<i32> = BoxQueue::new().unwrap();

        assert!(queue.enqueue(Box::new(42)));
        assert_eq!(queue.size_approx(), 1);

        let result = queue.dequeue().unwrap();
        assert_eq!(*result, 42);
        assert_eq!(queue.size_approx(), 0);
    }

    #[test]
    fn test_arc_queue_basic() {
        let queue: ArcQueue<String> = ArcQueue::new().unwrap();

        let data = StdArc::new("Hello".to_string());
        assert!(queue.enqueue_cloned(&data));

        let result = queue.dequeue().unwrap();
        assert_eq!(*result, "Hello");

        // Original Arc should still be valid
        assert_eq!(*data, "Hello");
    }

    #[test]
    fn test_box_queue_bulk() {
        let queue: BoxQueue<u64> = BoxQueue::new().unwrap();

        let items: Vec<Box<u64>> = (0..10).map(|i| Box::new(i)).collect();
        assert!(queue.enqueue_bulk(items));

        let results = queue.dequeue_bulk(15); // Request more than available
        assert_eq!(results.len(), 10);

        for (_i, boxed) in results.iter().enumerate() {
            // Note: order might not be preserved in concurrent queue
            assert!(**boxed < 10);
        }
    }

    #[test]
    fn test_arc_queue_bulk() {
        let queue: ArcQueue<i32> = ArcQueue::new().unwrap();

        let arcs: Vec<StdArc<i32>> = (0..5).map(|i| StdArc::new(i)).collect();
        assert!(queue.enqueue_bulk_cloned(&arcs));

        let results = queue.dequeue_bulk(5);
        assert_eq!(results.len(), 5);

        // Original arcs should still be valid
        for (i, arc) in arcs.iter().enumerate() {
            assert_eq!(**arc, i as i32);
        }
    }

    #[test]
    fn test_blocking_queue_bulk() {
        let queue = BlockingQueue::new().unwrap();

        let items: Vec<u64> = (0..10).collect();
        assert!(queue.enqueue_bulk(&items));

        let mut buffer = vec![0u64; 15]; // Larger than available
        let count = queue.dequeue_bulk(&mut buffer);
        assert_eq!(count, 10);

        // Verify items (order may vary in concurrent queue)
        let mut dequeued = buffer[..count].to_vec();
        dequeued.sort_unstable();
        assert_eq!(dequeued, items);
    }

    #[test]
    fn test_blocking_queue_bulk_timeout() {
        let queue = BlockingQueue::new().unwrap();

        // Test timeout on empty queue
        let mut buffer = vec![0u64; 5];
        let count = queue
            .dequeue_bulk_wait(&mut buffer, Duration::from_millis(50))
            .unwrap();
        assert_eq!(count, 0);

        // Add some items
        let items = vec![1, 2, 3];
        assert!(queue.enqueue_bulk(&items));

        // Should get items immediately
        let mut buffer = vec![0u64; 5];
        let count = queue
            .dequeue_bulk_wait(&mut buffer, Duration::from_millis(100))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_blocking_box_queue_bulk() {
        let queue = BlockingBoxQueue::<String>::new().unwrap();

        let items: Vec<Box<String>> = (0..3).map(|i| Box::new(format!("item-{}", i))).collect();
        assert!(queue.enqueue_bulk(items));

        let results = queue.dequeue_bulk(5); // Request more than available
        assert_eq!(results.len(), 3);

        for (i, item) in results.iter().enumerate() {
            // Note: order might not be preserved
            assert!(item.starts_with("item-"));
        }
    }

    #[test]
    fn test_blocking_box_queue_bulk_timeout() {
        let queue = BlockingBoxQueue::<i32>::new().unwrap();

        // Test timeout on empty queue
        let results = queue
            .dequeue_bulk_wait(3, Duration::from_millis(50))
            .unwrap();
        assert_eq!(results.len(), 0);

        // Add items and test successful bulk dequeue with timeout
        let items: Vec<Box<i32>> = vec![Box::new(10), Box::new(20)];
        assert!(queue.enqueue_bulk(items));

        let results = queue
            .dequeue_bulk_wait(5, Duration::from_millis(100))
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_blocking_arc_queue_bulk() {
        let queue = BlockingArcQueue::<i32>::new().unwrap();

        let arcs: Vec<StdArc<i32>> = (0..3).map(|i| StdArc::new(i * 10)).collect();
        assert!(queue.enqueue_bulk_cloned(&arcs));

        let results = queue.dequeue_bulk(5); // Request more than available
        assert_eq!(results.len(), 3);

        // Original arcs should still be valid
        for (i, arc) in arcs.iter().enumerate() {
            assert_eq!(**arc, i as i32 * 10);
        }
    }
}
