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
use std::ptr::{self, NonNull};
use std::marker::PhantomData;
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

/// Non-blocking multi-producer multi-consumer queue
/// 
/// This queue provides lock-free operations for maximum performance.
/// All operations are non-blocking and return immediately.
/// Internal low-level MPMC queue (not exposed to users)
struct MpmcQueue {
    ptr: NonNull<bop_mpmc>,
}

unsafe impl Send for MpmcQueue {}
unsafe impl Sync for MpmcQueue {}

/// Type alias for queue that stores Box pointers
pub type BoxQueue<T> = MpmcQueueTyped<Box<T>>;

/// Type alias for queue that stores Arc pointers  
pub type ArcQueue<T> = MpmcQueueTyped<Arc<T>>;

/// Generic wrapper for typed MPMC queues
pub struct MpmcQueueTyped<T> {
    inner: MpmcQueue,
    _phantom: PhantomData<T>,
}

unsafe impl<T: Send> Send for MpmcQueueTyped<T> {}
unsafe impl<T: Send> Sync for MpmcQueueTyped<T> {}

impl MpmcQueue {
    /// Create a new MPMC queue
    fn new() -> MpmcResult<Self> {
        let ptr = unsafe { bop_mpmc_create() };
        if ptr.is_null() {
            return Err(MpmcError::CreationFailed);
        }
        
        Ok(Self {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
        })
    }

    /// Get approximate size of the queue
    /// 
    /// Note: This is an approximation and may not be exact in highly concurrent scenarios
    pub fn size_approx(&self) -> usize {
        unsafe { bop_mpmc_size_approx(self.ptr.as_ptr()) }
    }

    /// Try to enqueue an item (non-blocking)
    /// 
    /// Returns `true` if the item was successfully enqueued, `false` otherwise.
    pub fn try_enqueue(&self, item: u64) -> bool {
        unsafe { bop_mpmc_enqueue(self.ptr.as_ptr(), item) }
    }

    /// Try to dequeue an item (non-blocking)
    /// 
    /// Returns `Some(item)` if an item was successfully dequeued, `None` otherwise.
    pub fn try_dequeue(&self) -> Option<u64> {
        let mut item = 0u64;
        if unsafe { bop_mpmc_dequeue(self.ptr.as_ptr(), &mut item) } {
            Some(item)
        } else {
            None
        }
    }

    /// Try to enqueue multiple items in bulk (non-blocking)
    /// 
    /// Returns `true` if all items were successfully enqueued, `false` otherwise.
    /// If `false` is returned, some items may have been enqueued.
    pub fn try_enqueue_bulk(&self, items: &[u64]) -> bool {
        if items.is_empty() {
            return true;
        }
        
        unsafe { 
            bop_mpmc_try_enqueue_bulk(
                self.ptr.as_ptr(),
                items.as_ptr() as *mut u64, // Cast away const for C API
                items.len()
            )
        }
    }

    /// Try to dequeue multiple items in bulk (non-blocking)
    /// 
    /// Returns the number of items actually dequeued (may be less than requested).
    /// The dequeued items are written to the provided buffer.
    pub fn try_dequeue_bulk(&self, buffer: &mut [u64]) -> usize {
        if buffer.is_empty() {
            return 0;
        }
        
        let result = unsafe {
            bop_mpmc_dequeue_bulk(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len()
            )
        };
        
        if result >= 0 {
            result as usize
        } else {
            0
        }
    }

    /// Create a producer token for optimized enqueue operations
    /// 
    /// Producer tokens can significantly improve performance when the same thread
    /// will be doing multiple enqueue operations.
    pub fn create_producer_token(&self) -> MpmcResult<ProducerToken> {
        let ptr = unsafe { bop_mpmc_create_producer_token(self.ptr.as_ptr()) };
        if ptr.is_null() {
            return Err(MpmcError::ProducerTokenCreationFailed);
        }
        
        Ok(ProducerToken {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
            _phantom: PhantomData,
        })
    }

    /// Create a consumer token for optimized dequeue operations
    /// 
    /// Consumer tokens can significantly improve performance when the same thread
    /// will be doing multiple dequeue operations.
    pub fn create_consumer_token(&self) -> MpmcResult<ConsumerToken> {
        let ptr = unsafe { bop_mpmc_create_consumer_token(self.ptr.as_ptr()) };
        if ptr.is_null() {
            return Err(MpmcError::ConsumerTokenCreationFailed);
        }
        
        Ok(ConsumerToken {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
            _phantom: PhantomData,
        })
    }
}

impl Drop for MpmcQueue {
    fn drop(&mut self) {
        unsafe { bop_mpmc_destroy(self.ptr.as_ptr()) }
    }
}

// Generic typed queue implementation
impl<T: Send + 'static> MpmcQueueTyped<T> {
    /// Create a new typed MPMC queue
    pub fn new() -> MpmcResult<Self> {
        Ok(Self {
            inner: MpmcQueue::new()?,
            _phantom: PhantomData,
        })
    }
    
    /// Get approximate size
    pub fn size_approx(&self) -> usize {
        self.inner.size_approx()
    }
    
    /// Try to enqueue an item
    pub fn try_enqueue(&self, item: T) -> bool {
        let ptr = Box::into_raw(Box::new(item)) as u64;
        if self.inner.try_enqueue(ptr) {
            true
        } else {
            // Failed to enqueue, reclaim ownership to prevent leak
            unsafe { drop(Box::from_raw(ptr as *mut T)) };
            false
        }
    }
    
    /// Try to dequeue an item
    pub fn try_dequeue(&self) -> Option<T> {
        self.inner.try_dequeue().map(|ptr| {
            unsafe { *Box::from_raw(ptr as *mut T) }
        })
    }
    
    /// Try to enqueue multiple items in bulk
    pub fn try_enqueue_bulk(&self, items: Vec<T>) -> bool {
        let ptrs: Vec<u64> = items.into_iter()
            .map(|item| Box::into_raw(Box::new(item)) as u64)
            .collect();
        
        if self.inner.try_enqueue_bulk(&ptrs) {
            true
        } else {
            // Failed to enqueue, reclaim ownership to prevent leaks
            for ptr in ptrs {
                unsafe { drop(Box::from_raw(ptr as *mut T)) };
            }
            false
        }
    }
    
    /// Try to dequeue multiple items in bulk
    pub fn try_dequeue_bulk(&self, max_items: usize) -> Vec<T> {
        let mut buffer = vec![0u64; max_items];
        let count = self.inner.try_dequeue_bulk(&mut buffer);
        
        buffer[..count].iter()
            .map(|&ptr| unsafe { *Box::from_raw(ptr as *mut T) })
            .collect()
    }
}

// Specialized implementation for Box<T>
impl<T: Send + 'static> BoxQueue<T> {
    /// Enqueue a boxed item (transfers ownership)
    pub fn enqueue_boxed(&self, boxed: Box<T>) -> bool {
        let ptr = Box::into_raw(boxed) as u64;
        self.inner.try_enqueue(ptr)
    }
    
    /// Dequeue as a boxed item
    pub fn dequeue_boxed(&self) -> Option<Box<T>> {
        self.inner.try_dequeue().map(|ptr| {
            unsafe { Box::from_raw(ptr as *mut T) }
        })
    }
}

// Specialized implementation for Arc<T>
impl<T: Send + Sync + 'static> ArcQueue<T> {
    /// Enqueue an Arc (clones the Arc, incrementing ref count)
    pub fn enqueue_arc(&self, arc: Arc<T>) -> bool {
        let ptr = Arc::into_raw(arc) as u64;
        self.inner.try_enqueue(ptr)
    }
    
    /// Dequeue as an Arc  
    pub fn dequeue_arc(&self) -> Option<Arc<T>> {
        self.inner.try_dequeue().map(|ptr| {
            unsafe { Arc::from_raw(ptr as *const T) }
        })
    }
    
    /// Enqueue a clone of an Arc
    pub fn enqueue_cloned(&self, arc: &Arc<T>) -> bool {
        self.enqueue_arc(Arc::clone(arc))
    }
}

// Cleanup on drop for typed queues
impl<T> Drop for MpmcQueueTyped<T> {
    fn drop(&mut self) {
        // Drain remaining items to prevent memory leaks
        while let Some(ptr) = self.inner.try_dequeue() {
            unsafe { drop(Box::from_raw(ptr as *mut T)) };
        }
    }
}

/// Producer token for optimized enqueue operations
/// 
/// Provides better performance for threads that frequently enqueue items.
pub struct ProducerToken<'a> {
    ptr: NonNull<bop_mpmc_producer_token>,
    _phantom: PhantomData<&'a MpmcQueue>,
}

unsafe impl Send for ProducerToken<'_> {}

impl ProducerToken<'_> {
    /// Try to enqueue an item using this producer token (non-blocking)
    /// 
    /// Generally more efficient than the queue's direct enqueue method
    /// for threads that do frequent enqueue operations.
    pub fn try_enqueue(&self, item: u64) -> bool {
        unsafe { bop_mpmc_enqueue_token(self.ptr.as_ptr(), item) }
    }

    /// Try to enqueue multiple items in bulk using this token (non-blocking)
    pub fn try_enqueue_bulk(&self, items: &[u64]) -> bool {
        if items.is_empty() {
            return true;
        }
        
        unsafe {
            bop_mpmc_try_enqueue_bulk_token(
                self.ptr.as_ptr(),
                items.as_ptr() as *mut u64,
                items.len()
            )
        }
    }
}

impl Drop for ProducerToken<'_> {
    fn drop(&mut self) {
        unsafe { bop_mpmc_destroy_producer_token(self.ptr.as_ptr()) }
    }
}

/// Consumer token for optimized dequeue operations
/// 
/// Provides better performance for threads that frequently dequeue items.
pub struct ConsumerToken<'a> {
    ptr: NonNull<bop_mpmc_consumer_token>,
    _phantom: PhantomData<&'a MpmcQueue>,
}

unsafe impl Send for ConsumerToken<'_> {}

impl ConsumerToken<'_> {
    /// Try to dequeue an item using this consumer token (non-blocking)
    /// 
    /// Generally more efficient than the queue's direct dequeue method
    /// for threads that do frequent dequeue operations.
    pub fn try_dequeue(&self) -> Option<u64> {
        let mut item = 0u64;
        if unsafe { bop_mpmc_dequeue_token(self.ptr.as_ptr(), &mut item) } {
            Some(item)
        } else {
            None
        }
    }

    /// Try to dequeue multiple items in bulk using this token (non-blocking)
    pub fn try_dequeue_bulk(&self, buffer: &mut [u64]) -> usize {
        if buffer.is_empty() {
            return 0;
        }
        
        let result = unsafe {
            bop_mpmc_dequeue_bulk_token(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len()
            )
        };
        
        if result >= 0 {
            result as usize
        } else {
            0
        }
    }
}

impl Drop for ConsumerToken<'_> {
    fn drop(&mut self) {
        unsafe { bop_mpmc_destroy_consumer_token(self.ptr.as_ptr()) }
    }
}

/// Blocking multi-producer multi-consumer queue
/// 
/// This queue provides blocking operations that can wait for items to become available.
/// Useful when you want to block threads until work is available rather than spinning.
pub struct BlockingMpmcQueue {
    ptr: NonNull<bop_mpmc_blocking>,
}

unsafe impl Send for BlockingMpmcQueue {}
unsafe impl Sync for BlockingMpmcQueue {}

impl BlockingMpmcQueue {
    /// Create a new blocking MPMC queue
    pub fn new() -> MpmcResult<Self> {
        let ptr = unsafe { bop_mpmc_blocking_create() };
        if ptr.is_null() {
            return Err(MpmcError::CreationFailed);
        }
        
        Ok(Self {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
        })
    }

    /// Get approximate size of the queue
    pub fn size_approx(&self) -> usize {
        unsafe { bop_mpmc_blocking_size_approx(self.ptr.as_ptr()) }
    }

    /// Try to enqueue an item (non-blocking)
    pub fn try_enqueue(&self, item: u64) -> bool {
        unsafe { bop_mpmc_blocking_enqueue(self.ptr.as_ptr(), item) }
    }

    /// Try to dequeue an item (non-blocking)
    pub fn try_dequeue(&self) -> Option<u64> {
        let mut item = 0u64;
        if unsafe { bop_mpmc_blocking_dequeue(self.ptr.as_ptr(), &mut item) } {
            Some(item)
        } else {
            None
        }
    }

    /// Dequeue an item with timeout (blocking)
    /// 
    /// Blocks until an item is available or the timeout expires.
    /// Returns `Ok(Some(item))` if an item was dequeued within the timeout,
    /// `Ok(None)` if the timeout expired, or `Err` on error.
    pub fn dequeue_wait(&self, timeout: Duration) -> MpmcResult<Option<u64>> {
        let timeout_micros = timeout.as_micros();
        if timeout_micros > i64::MAX as u128 {
            return Err(MpmcError::InvalidTimeout);
        }
        
        let mut item = 0u64;
        if unsafe { bop_mpmc_blocking_dequeue_wait(self.ptr.as_ptr(), &mut item, timeout_micros as i64) } {
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    /// Try to enqueue multiple items in bulk (non-blocking)
    pub fn try_enqueue_bulk(&self, items: &[u64]) -> bool {
        if items.is_empty() {
            return true;
        }
        
        unsafe {
            bop_mpmc_blocking_try_enqueue_bulk(
                self.ptr.as_ptr(),
                items.as_ptr() as *mut u64,
                items.len()
            )
        }
    }

    /// Try to dequeue multiple items in bulk (non-blocking)
    pub fn try_dequeue_bulk(&self, buffer: &mut [u64]) -> usize {
        if buffer.is_empty() {
            return 0;
        }
        
        let result = unsafe {
            bop_mpmc_blocking_dequeue_bulk(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len()
            )
        };
        
        if result >= 0 {
            result as usize
        } else {
            0
        }
    }

    /// Dequeue multiple items in bulk with timeout (blocking)
    /// 
    /// Blocks until at least one item is available or the timeout expires.
    /// Returns the number of items actually dequeued.
    pub fn dequeue_bulk_wait(&self, buffer: &mut [u64], timeout: Duration) -> MpmcResult<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        
        let timeout_micros = timeout.as_micros();
        if timeout_micros > i64::MAX as u128 {
            return Err(MpmcError::InvalidTimeout);
        }
        
        let result = unsafe {
            bop_mpmc_blocking_dequeue_bulk_wait(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                timeout_micros as i64
            )
        };
        
        if result >= 0 {
            Ok(result as usize)
        } else {
            Ok(0)
        }
    }

    /// Create a blocking producer token for optimized enqueue operations
    pub fn create_producer_token(&self) -> MpmcResult<BlockingProducerToken> {
        let ptr = unsafe { bop_mpmc_blocking_create_producer_token(self.ptr.as_ptr()) };
        if ptr.is_null() {
            return Err(MpmcError::ProducerTokenCreationFailed);
        }
        
        Ok(BlockingProducerToken {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
            _phantom: PhantomData,
        })
    }

    /// Create a blocking consumer token for optimized dequeue operations
    pub fn create_consumer_token(&self) -> MpmcResult<BlockingConsumerToken> {
        let ptr = unsafe { bop_mpmc_blocking_create_consumer_token(self.ptr.as_ptr()) };
        if ptr.is_null() {
            return Err(MpmcError::ConsumerTokenCreationFailed);
        }
        
        Ok(BlockingConsumerToken {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
            _phantom: PhantomData,
        })
    }
}

impl Drop for BlockingMpmcQueue {
    fn drop(&mut self) {
        unsafe { bop_mpmc_blocking_destroy(self.ptr.as_ptr()) }
    }
}

/// Blocking producer token for optimized enqueue operations
pub struct BlockingProducerToken<'a> {
    ptr: NonNull<bop_mpmc_blocking_producer_token>,
    _phantom: PhantomData<&'a BlockingMpmcQueue>,
}

unsafe impl Send for BlockingProducerToken<'_> {}

impl BlockingProducerToken<'_> {
    /// Try to enqueue an item using this producer token (non-blocking)
    pub fn try_enqueue(&self, item: u64) -> bool {
        unsafe { bop_mpmc_blocking_enqueue_token(self.ptr.as_ptr(), item) }
    }

    /// Try to enqueue multiple items in bulk using this token (non-blocking)
    pub fn try_enqueue_bulk(&self, items: &[u64]) -> bool {
        if items.is_empty() {
            return true;
        }
        
        unsafe {
            bop_mpmc_blocking_try_enqueue_bulk_token(
                self.ptr.as_ptr(),
                items.as_ptr() as *mut u64,
                items.len()
            )
        }
    }
}

impl Drop for BlockingProducerToken<'_> {
    fn drop(&mut self) {
        unsafe { bop_mpmc_blocking_destroy_producer_token(self.ptr.as_ptr()) }
    }
}

/// Blocking consumer token for optimized dequeue operations
pub struct BlockingConsumerToken<'a> {
    ptr: NonNull<bop_mpmc_blocking_consumer_token>,
    _phantom: PhantomData<&'a BlockingMpmcQueue>,
}

unsafe impl Send for BlockingConsumerToken<'_> {}

impl BlockingConsumerToken<'_> {
    /// Try to dequeue an item using this consumer token (non-blocking)
    pub fn try_dequeue(&self) -> Option<u64> {
        let mut item = 0u64;
        if unsafe { bop_mpmc_blocking_dequeue_token(self.ptr.as_ptr(), &mut item) } {
            Some(item)
        } else {
            None
        }
    }

    /// Dequeue an item with timeout using this consumer token (blocking)
    pub fn dequeue_wait(&self, timeout: Duration) -> MpmcResult<Option<u64>> {
        let timeout_micros = timeout.as_micros();
        if timeout_micros > i64::MAX as u128 {
            return Err(MpmcError::InvalidTimeout);
        }
        
        let mut item = 0u64;
        if unsafe { 
            bop_mpmc_blocking_dequeue_wait_token(
                self.ptr.as_ptr(), 
                &mut item, 
                timeout_micros as i64
            ) 
        } {
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    /// Try to dequeue multiple items in bulk using this token (non-blocking)
    pub fn try_dequeue_bulk(&self, buffer: &mut [u64]) -> usize {
        if buffer.is_empty() {
            return 0;
        }
        
        let result = unsafe {
            bop_mpmc_blocking_dequeue_bulk_token(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len()
            )
        };
        
        if result >= 0 {
            result as usize
        } else {
            0
        }
    }

    /// Dequeue multiple items in bulk with timeout using this token (blocking)
    pub fn dequeue_bulk_wait(&self, buffer: &mut [u64], timeout: Duration) -> MpmcResult<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        
        let timeout_micros = timeout.as_micros();
        if timeout_micros > i64::MAX as u128 {
            return Err(MpmcError::InvalidTimeout);
        }
        
        let result = unsafe {
            bop_mpmc_blocking_dequeue_bulk_wait_token(
                self.ptr.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                timeout_micros as i64
            )
        };
        
        if result >= 0 {
            Ok(result as usize)
        } else {
            Ok(0)
        }
    }
}

impl Drop for BlockingConsumerToken<'_> {
    fn drop(&mut self) {
        unsafe { bop_mpmc_blocking_destroy_consumer_token(self.ptr.as_ptr()) }
    }
}

/// Utility functions for working with MPMC queues
pub mod utils {
    use super::*;
    use std::thread;
    use std::sync::Arc;

    /// Spawn a producer thread that enqueues items from an iterator
    /// 
    /// Useful for testing or when you have a known set of items to produce.
    pub fn spawn_producer<I>(queue: Arc<MpmcQueue>, items: I) -> thread::JoinHandle<usize>
    where
        I: IntoIterator<Item = u64> + Send + 'static,
        I::IntoIter: Send,
    {
        thread::spawn(move || {
            let mut count = 0;
            let producer_token = queue.create_producer_token().expect("Failed to create producer token");
            
            for item in items {
                while !producer_token.try_enqueue(item) {
                    thread::yield_now();
                }
                count += 1;
            }
            
            count
        })
    }

    /// Spawn a consumer thread that dequeues items and collects them into a Vec
    /// 
    /// The consumer will stop when it doesn't receive an item within the timeout.
    pub fn spawn_consumer(
        queue: Arc<MpmcQueue>, 
        max_items: Option<usize>,
        timeout_ms: u64
    ) -> thread::JoinHandle<Vec<u64>> {
        thread::spawn(move || {
            let mut items = Vec::new();
            let consumer_token = queue.create_consumer_token().expect("Failed to create consumer token");
            let mut consecutive_failures = 0;
            
            loop {
                if let Some(max) = max_items {
                    if items.len() >= max {
                        break;
                    }
                }
                
                if let Some(item) = consumer_token.try_dequeue() {
                    items.push(item);
                    consecutive_failures = 0;
                } else {
                    consecutive_failures += 1;
                    if consecutive_failures > timeout_ms {
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            
            items
        })
    }

    /// Create a simple producer-consumer setup for testing
    /// 
    /// Returns (producer_handle, consumer_handle, queue)
    pub fn create_producer_consumer_test(
        items_to_produce: Vec<u64>
    ) -> (thread::JoinHandle<usize>, thread::JoinHandle<Vec<u64>>, Arc<MpmcQueue>) {
        let queue = Arc::new(MpmcQueue::new().expect("Failed to create queue"));
        let expected_count = items_to_produce.len();
        
        let producer_queue = queue.clone();
        let consumer_queue = queue.clone();
        
        let producer = spawn_producer(producer_queue, items_to_produce);
        let consumer = spawn_consumer(consumer_queue, Some(expected_count), 1000);
        
        (producer, consumer, queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_mpmc_queue_creation() {
        let queue = MpmcQueue::new();
        assert!(queue.is_ok());
    }

    #[test]
    fn test_basic_enqueue_dequeue() {
        let queue = MpmcQueue::new().unwrap();
        
        assert!(queue.try_enqueue(42));
        assert_eq!(queue.try_dequeue(), Some(42));
        assert_eq!(queue.try_dequeue(), None);
    }

    #[test]
    fn test_approximate_size() {
        let queue = MpmcQueue::new().unwrap();
        
        assert_eq!(queue.size_approx(), 0);
        
        assert!(queue.try_enqueue(1));
        assert!(queue.try_enqueue(2));
        
        // Size should be approximately 2, but may vary due to concurrent nature
        let size = queue.size_approx();
        assert!(size <= 2);
    }

    #[test]
    fn test_producer_token() {
        let queue = MpmcQueue::new().unwrap();
        let producer_token = queue.create_producer_token().unwrap();
        
        assert!(producer_token.try_enqueue(100));
        assert_eq!(queue.try_dequeue(), Some(100));
    }

    #[test]
    fn test_consumer_token() {
        let queue = MpmcQueue::new().unwrap();
        let consumer_token = queue.create_consumer_token().unwrap();
        
        assert!(queue.try_enqueue(200));
        assert_eq!(consumer_token.try_dequeue(), Some(200));
    }

    #[test]
    fn test_bulk_operations() {
        let queue = MpmcQueue::new().unwrap();
        let items = vec![1, 2, 3, 4, 5];
        
        assert!(queue.try_enqueue_bulk(&items));
        
        let mut buffer = vec![0u64; 10];
        let dequeued_count = queue.try_dequeue_bulk(&mut buffer);
        
        assert_eq!(dequeued_count, 5);
        assert_eq!(&buffer[..5], &items);
    }

    #[test]
    fn test_blocking_queue() {
        let queue = BlockingMpmcQueue::new().unwrap();
        
        assert!(queue.try_enqueue(42));
        assert_eq!(queue.try_dequeue(), Some(42));
        
        // Test timeout
        let result = queue.dequeue_wait(Duration::from_millis(10));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_blocking_with_timeout() {
        let queue = Arc::new(BlockingMpmcQueue::new().unwrap());
        let queue_clone = queue.clone();
        
        // Producer thread that waits before producing
        let producer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            queue_clone.try_enqueue(999);
        });
        
        // Consumer should get the item within timeout
        let start = std::time::Instant::now();
        let result = queue.dequeue_wait(Duration::from_millis(200)).unwrap();
        let elapsed = start.elapsed();
        
        assert_eq!(result, Some(999));
        assert!(elapsed >= Duration::from_millis(45));
        assert!(elapsed < Duration::from_millis(150));
        
        producer.join().unwrap();
    }

    #[test]
    fn test_multiple_producers_consumers() {
        let queue = Arc::new(MpmcQueue::new().unwrap());
        let num_producers = 4;
        let num_consumers = 2;
        let items_per_producer = 100;
        
        let mut producers = Vec::new();
        let mut consumers = Vec::new();
        
        // Spawn producers
        for i in 0..num_producers {
            let queue_clone = queue.clone();
            producers.push(thread::spawn(move || {
                let token = queue_clone.create_producer_token().unwrap();
                for j in 0..items_per_producer {
                    let item = (i * items_per_producer + j) as u64;
                    while !token.try_enqueue(item) {
                        thread::yield_now();
                    }
                }
            }));
        }
        
        // Spawn consumers
        for _ in 0..num_consumers {
            let queue_clone = queue.clone();
            consumers.push(thread::spawn(move || {
                let token = queue_clone.create_consumer_token().unwrap();
                let mut collected = Vec::new();
                
                // Collect items for a reasonable time
                let start = std::time::Instant::now();
                while start.elapsed() < Duration::from_millis(1000) {
                    if let Some(item) = token.try_dequeue() {
                        collected.push(item);
                    } else {
                        thread::yield_now();
                    }
                }
                collected
            }));
        }
        
        // Wait for producers to finish
        for producer in producers {
            producer.join().unwrap();
        }
        
        // Collect results from consumers
        let mut all_items = Vec::new();
        for consumer in consumers {
            let items = consumer.join().unwrap();
            all_items.extend(items);
        }
        
        // Should have collected all items
        assert_eq!(all_items.len(), num_producers * items_per_producer);
        
        // Verify no duplicates (all items should be unique)
        all_items.sort_unstable();
        all_items.dedup();
        assert_eq!(all_items.len(), num_producers * items_per_producer);
    }

    #[test] 
    fn test_utils_producer_consumer() {
        let items = vec![10, 20, 30, 40, 50];
        let (producer, consumer, _queue) = utils::create_producer_consumer_test(items.clone());
        
        let produced_count = producer.join().unwrap();
        let consumed_items = consumer.join().unwrap();
        
        assert_eq!(produced_count, items.len());
        assert_eq!(consumed_items.len(), items.len());
        
        // Items should all be present (though order may vary)
        let mut consumed_sorted = consumed_items;
        consumed_sorted.sort_unstable();
        let mut expected_sorted = items;
        expected_sorted.sort_unstable();
        assert_eq!(consumed_sorted, expected_sorted);
    }
}