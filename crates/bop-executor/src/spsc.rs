// Copyright (c) 2024 Andrew Drogalis
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.

//! # DRO SPSC Queue
//!
//! A bounded lock-free Single-Producer Single-Consumer queue implementation
//! based on the original C++ design by Andrew Drogalis.
//!
//! This implementation provides:
//! - Lock-free operations with proper memory ordering
//! - Cache-line alignment to prevent false sharing
//! - Support for both stack-based and heap-based buffers
//! - Buffer overflow protection with padding
//! - Zero-copy operations where possible
//! - Simple move semantics (all moves are "nothrow" in Rust)
//!
//! ## Memory Layout
//!
//! The queue uses ring buffer with cache-line aligned read/write indices:
//! ```text
//! +----------------+----------------+----------------+----------------+
//! | Padding        | Buffer Data    | Padding        | Consumer Data  |
//! +----------------+----------------+----------------+----------------+
//! ```
//!
//! ## Buffer Types
//!
//! - **HeapBuffer**: Dynamically allocated buffer with capacity + 1 slot
//! - **StackBuffer**: Stack-allocated buffer for compile-time known sizes

use std::alloc::{Layout, alloc, dealloc};
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{PopError, PushError};

use crossbeam_utils::CachePadded;

/// Cache line size for padding to avoid false sharing
const CACHE_LINE_SIZE: usize = 64;

/// Maximum bytes that can be safely allocated on stack (2MB)
const MAX_BYTES_ON_STACK: usize = 2 * 1024 * 1024;

/// Trait for types that can be stored in the SPSC queue
pub trait SPSCType: Default + Send + Sync {}

/// Blanket implementation for most types
impl<T> SPSCType for T where T: Default + Send + Sync {}

/// Prevents stack overflow by limiting stack buffer size
pub trait MaxStackSize<T> {
    /// Check if N instances of T fit within stack limits
    fn fits_on_stack() -> bool;
}

impl<T, const N: usize> MaxStackSize<T> for [T; N] {
    fn fits_on_stack() -> bool {
        N <= MAX_BYTES_ON_STACK / std::mem::size_of::<T>()
    }
}

/// Internal buffer storage trait
pub trait BufferStorage<T: SPSCType> {
    /// Get the capacity of the buffer (usable slots)
    fn capacity(&self) -> usize;

    /// Get a mutable reference to the underlying buffer
    fn buffer_ptr(&self) -> *mut T;

    /// Get the padding size
    fn padding(&self) -> usize;
}

/// Heap-allocated buffer with dynamic capacity
pub struct HeapBuffer<T: SPSCType> {
    capacity: usize,
    buffer: *mut T,
    layout: Layout,
    padding: usize,
}

impl<T: SPSCType> HeapBuffer<T> {
    /// Create a new heap buffer with the specified capacity
    ///
    /// # Panics
    /// Panics if capacity is 0 or too large
    pub fn new(capacity: usize) -> Self {
        if capacity == 0 {
            panic!("Capacity must be a positive number for heap allocations");
        }

        let padding = ((CACHE_LINE_SIZE - 1) / std::mem::size_of::<T>()) + 1;
        let total_capacity = capacity + 1 + (2 * padding);

        // Check for overflow
        if total_capacity > usize::MAX / std::mem::size_of::<T>() {
            panic!("Capacity with padding exceeds usize::MAX");
        }

        let layout =
            Layout::array::<T>(total_capacity).expect("Invalid layout for buffer allocation");

        let buffer = unsafe { alloc(layout) as *mut T };
        if buffer.is_null() {
            panic!("Failed to allocate memory for buffer");
        }

        Self {
            capacity,
            buffer,
            layout,
            padding,
        }
    }
}

impl<T: SPSCType> Drop for HeapBuffer<T> {
    fn drop(&mut self) {
        // Drop all initialized elements
        unsafe {
            for i in 0..(self.capacity + 1 + 2 * self.padding) {
                ptr::drop_in_place(self.buffer.add(i));
            }
            dealloc(self.buffer as *mut u8, self.layout);
        }
    }
}

unsafe impl<T: SPSCType> Send for HeapBuffer<T> {}
unsafe impl<T: SPSCType> Sync for HeapBuffer<T> {}

impl<T: SPSCType> BufferStorage<T> for HeapBuffer<T> {
    fn capacity(&self) -> usize {
        self.capacity + 1
    }

    fn buffer_ptr(&self) -> *mut T {
        self.buffer
    }

    fn padding(&self) -> usize {
        self.padding
    }
}

/// Stack-allocated buffer for compile-time known sizes
pub struct StackBuffer<T: SPSCType, const N: usize> {
    buffer: [MaybeUninit<T>; N],
    capacity: usize,
    padding: usize,
}

impl<T: SPSCType, const N: usize> StackBuffer<T, N> {
    /// Create a new stack buffer
    ///
    /// # Panics
    /// Panics if N doesn't fit within stack limits
    pub fn new() -> Self {
        assert!(
            N <= MAX_BYTES_ON_STACK / std::mem::size_of::<T>(),
            "Stack buffer size exceeds safe limits"
        );

        let padding = ((CACHE_LINE_SIZE - 1) / std::mem::size_of::<T>()) + 1;
        let capacity = N + 1 + (2 * padding);

        Self {
            buffer: unsafe { MaybeUninit::uninit().assume_init() },
            capacity,
            padding,
        }
    }
}

impl<T: SPSCType, const N: usize> Drop for StackBuffer<T, N> {
    fn drop(&mut self) {
        // Note: Simple StackBuffer doesn't track initialization state
        // In practice, only initialized elements should be dropped
        // This is a simplified implementation
    }
}

unsafe impl<T: SPSCType, const N: usize> Send for StackBuffer<T, N> {}
unsafe impl<T: SPSCType, const N: usize> Sync for StackBuffer<T, N> {}

impl<T: SPSCType, const N: usize> BufferStorage<T> for StackBuffer<T, N> {
    fn capacity(&self) -> usize {
        self.capacity
    }

    fn buffer_ptr(&self) -> *mut T {
        self.buffer.as_ptr() as *mut T
    }

    fn padding(&self) -> usize {
        self.padding
    }
}

/// Cache-line aligned writer state
///
/// The read_index_cache uses UnsafeCell instead of AtomicUsize because:
/// 1. Only the producer thread writes to this field
/// 2. Only the producer thread reads from this field
/// 3. No synchronization is needed for single-threaded access
/// 4. UnsafeCell provides interior mutability without atomic overhead
struct WriterState<T: SPSCType> {
    write_index: AtomicUsize,
    read_index_cache: UnsafeCell<usize>,
    padding_cache: usize,
    _phantom: std::marker::PhantomData<T>,
}

// SAFETY: WriterState is only accessed by the single producer thread,
// so the UnsafeCell access is safe. The atomic write_index provides
// the necessary synchronization between producer and consumer.
unsafe impl<T: SPSCType> Send for WriterState<T> {}
unsafe impl<T: SPSCType> Sync for WriterState<T> {}

impl<T: SPSCType> WriterState<T> {
    fn new(padding: usize) -> Self {
        Self {
            write_index: AtomicUsize::new(0),
            read_index_cache: UnsafeCell::new(0),
            padding_cache: padding,
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Cache-line aligned reader state
///
/// The write_index_cache uses UnsafeCell instead of AtomicUsize because:
/// 1. Only the consumer thread writes to this field
/// 2. Only the consumer thread reads from this field
/// 3. No synchronization is needed for single-threaded access
/// 4. UnsafeCell provides interior mutability without atomic overhead
struct ReaderState<T: SPSCType> {
    read_index: AtomicUsize,
    write_index_cache: UnsafeCell<usize>,
    capacity_cache: usize,
    _phantom: std::marker::PhantomData<T>,
}

// SAFETY: ReaderState is only accessed by the single consumer thread,
// so the UnsafeCell access is safe. The atomic read_index provides
// the necessary synchronization between producer and consumer.
unsafe impl<T: SPSCType> Send for ReaderState<T> {}
unsafe impl<T: SPSCType> Sync for ReaderState<T> {}

impl<T: SPSCType> ReaderState<T> {
    fn new(capacity: usize) -> Self {
        Self {
            read_index: AtomicUsize::new(0),
            write_index_cache: UnsafeCell::new(0),
            capacity_cache: capacity,
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Single Producer Single Consumer queue
pub struct SpscQueue<T: SPSCType, B: BufferStorage<T>> {
    buffer: B,
    writer: CachePadded<WriterState<T>>,
    reader: CachePadded<ReaderState<T>>,
}

impl<T: SPSCType> SpscQueue<T, HeapBuffer<T>> {
    /// Create a new SPSC queue with heap allocation
    ///
    /// # Arguments
    /// * `capacity` - Number of elements the queue can hold
    ///
    /// # Panics
    /// Panics if capacity is 0 or too large
    pub fn new_heap(capacity: usize) -> Self {
        let buffer = HeapBuffer::new(capacity);
        let padding = buffer.padding();
        let capacity = buffer.capacity();

        Self {
            writer: CachePadded::new(WriterState::new(padding)),
            reader: CachePadded::new(ReaderState::new(capacity)),
            buffer,
        }
    }
}

impl<T: SPSCType, const N: usize> SpscQueue<T, StackBuffer<T, N>> {
    /// Create a new SPSC queue with stack allocation
    ///
    /// # Returns
    /// A queue with N elements capacity (minus 1 for livelock prevention)
    ///
    /// # Panics
    /// Panics if N doesn't fit within stack limits
    pub fn new_stack() -> Self {
        let buffer = StackBuffer::new();
        let padding = buffer.padding();
        let capacity = buffer.capacity();

        Self {
            writer: CachePadded::new(WriterState::new(padding)),
            reader: CachePadded::new(ReaderState::new(capacity)),
            buffer,
        }
    }
}

impl<T: SPSCType, B: BufferStorage<T>> SpscQueue<T, B> {
    /// Emplace a value into the queue
    ///
    /// Blocks until space is available
    pub fn push(&self, value: T) {
        let write_index = self.writer.write_index.load(Ordering::Relaxed);
        let next_write_index = if write_index == self.buffer.capacity() - 1 {
            0
        } else {
            write_index + 1
        };

        // Wait for reader to catch up
        //
        // Note: We use UnsafeCell for read_index_cache because only the producer
        // thread accesses this field, so no atomic synchronization is needed.
        while next_write_index == unsafe { *self.writer.read_index_cache.get() } {
            unsafe {
                *self.writer.read_index_cache.get() =
                    self.reader.read_index.load(Ordering::Acquire);
            }
        }

        unsafe {
            self.write_value(write_index, value);
        }

        self.writer
            .write_index
            .store(next_write_index, Ordering::Release);
    }

    /// Force emplace a value (may overwrite existing data)
    pub fn force_push(&self, value: T) {
        let write_index = self.writer.write_index.load(Ordering::Relaxed);
        let next_write_index = if write_index == self.buffer.capacity() - 1 {
            0
        } else {
            write_index + 1
        };

        unsafe {
            self.write_value(write_index, value);
        }

        self.writer
            .write_index
            .store(next_write_index, Ordering::Release);
    }

    /// Try to emplace a value into the queue
    ///
    /// # Returns
    /// `true` if successful, `false` if queue is full
    pub fn try_push(&self, value: T) -> bool {
        let write_index = self.writer.write_index.load(Ordering::Relaxed);
        let next_write_index = if write_index == self.buffer.capacity() - 1 {
            0
        } else {
            write_index + 1
        };

        // Check if queue is full using cached read index
        //
        // Note: UnsafeCell provides zero-cost interior mutability since only
        // the producer thread accesses read_index_cache.
        if next_write_index == unsafe { *self.writer.read_index_cache.get() } {
            unsafe {
                *self.writer.read_index_cache.get() =
                    self.reader.read_index.load(Ordering::Acquire);
            }
            if next_write_index == unsafe { *self.writer.read_index_cache.get() } {
                return false;
            }
        }

        unsafe {
            self.write_value(write_index, value);
        }

        self.writer
            .write_index
            .store(next_write_index, Ordering::Release);
        true
    }

    /// Try to emplace a value into the queue
    ///
    /// # Returns
    /// `true` if successful, `false` if queue is full
    pub fn try_push_owned(&self, value: T) -> Option<T> {
        let write_index = self.writer.write_index.load(Ordering::Relaxed);
        let next_write_index = if write_index == self.buffer.capacity() - 1 {
            0
        } else {
            write_index + 1
        };

        // Check if queue is full using cached read index
        //
        // Note: UnsafeCell provides zero-cost interior mutability since only
        // the producer thread accesses read_index_cache.
        if next_write_index == unsafe { *self.writer.read_index_cache.get() } {
            unsafe {
                *self.writer.read_index_cache.get() =
                    self.reader.read_index.load(Ordering::Acquire);
            }
            if next_write_index == unsafe { *self.writer.read_index_cache.get() } {
                return Some(value);
            }
        }

        unsafe {
            self.write_value(write_index, value);
        }

        self.writer
            .write_index
            .store(next_write_index, Ordering::Release);
        None
    }

    /// Pop a value from the queue
    ///
    /// Blocks until a value is available
    pub fn pop(&self) -> T {
        let read_index = self.reader.read_index.load(Ordering::Relaxed);

        // Wait for writer to enqueue
        //
        // Note: We use UnsafeCell for write_index_cache because only the consumer
        // thread accesses this field, so no atomic synchronization is needed.
        while read_index == unsafe { *self.reader.write_index_cache.get() } {
            unsafe {
                *self.reader.write_index_cache.get() =
                    self.writer.write_index.load(Ordering::Acquire);
            }
        }

        let value = unsafe { self.read_value(read_index) };
        let next_read_index = if read_index == self.reader.capacity_cache - 1 {
            0
        } else {
            read_index + 1
        };

        self.reader
            .read_index
            .store(next_read_index, Ordering::Release);
        value
    }

    /// Try to pop a value from the queue
    ///
    /// # Returns
    /// `Some(value)` if successful, `None` if queue is empty
    pub fn try_pop(&self) -> Option<T> {
        let read_index = self.reader.read_index.load(Ordering::Relaxed);

        // Check if queue is empty using cached write index
        //
        // Note: UnsafeCell provides zero-cost interior mutability since only
        // the consumer thread accesses write_index_cache.
        if read_index == unsafe { *self.reader.write_index_cache.get() } {
            unsafe {
                *self.reader.write_index_cache.get() =
                    self.writer.write_index.load(Ordering::Acquire);
            }
            if read_index == unsafe { *self.reader.write_index_cache.get() } {
                return None;
            }
        }

        let value = unsafe { self.read_value(read_index) };
        let next_read_index = if read_index == self.reader.capacity_cache - 1 {
            0
        } else {
            read_index + 1
        };

        self.reader
            .read_index
            .store(next_read_index, Ordering::Release);
        Some(value)
    }

    /// Get the current size of the queue
    pub fn size(&self) -> usize {
        let write_index = self.writer.write_index.load(Ordering::Acquire);
        let read_index = self.reader.read_index.load(Ordering::Acquire);

        if write_index >= read_index {
            write_index - read_index
        } else {
            (self.buffer.capacity() - read_index) + write_index
        }
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.writer.write_index.load(Ordering::Acquire)
            == self.reader.read_index.load(Ordering::Acquire)
    }

    /// Get the capacity of the queue
    pub fn capacity(&self) -> usize {
        self.buffer.capacity() - 1
    }

    /// Write a value to the buffer at the specified index
    unsafe fn write_value(&self, write_index: usize, value: T) {
        let buffer_ptr = self.buffer.buffer_ptr();
        unsafe {
            let target_ptr = buffer_ptr.add(write_index + self.writer.padding_cache);
            ptr::write(target_ptr, value);
        }
    }

    /// Read a value from the buffer at the specified index
    unsafe fn read_value(&self, read_index: usize) -> T {
        let buffer_ptr = self.buffer.buffer_ptr();
        unsafe {
            let source_ptr = buffer_ptr.add(read_index + self.buffer.padding());

            ptr::read(source_ptr)
        }
    }

    /// Drain items from the queue and apply a function to each item
    ///
    /// # Arguments
    /// * `target` - Function to apply to each drained item
    /// * `max_items` - Maximum number of items to drain
    ///
    /// # Returns
    /// A tuple of (number_of_items_drained, was_closed)
    ///
    /// This method efficiently drains multiple items from the queue in a single pass,
    /// applying the provided function to each item. It's more efficient than calling
    /// `try_pop()` repeatedly for bulk operations.
    pub fn drain_with<F>(&self, mut target: F, max_items: usize) -> (usize, bool)
    where
        F: FnMut(T),
    {
        let mut count: usize = 0;
        let mut read_index = self.reader.read_index.load(Ordering::Relaxed);

        // Process items in a single pass
        while count < max_items {
            // Check if we have items available by comparing with writer cache
            //
            // Note: UnsafeCell provides zero-cost interior mutability since only
            // the consumer thread accesses write_index_cache in drain_with.
            if read_index == unsafe { *self.reader.write_index_cache.get() } {
                // Refresh the writer cache
                unsafe {
                    *self.reader.write_index_cache.get() =
                        self.writer.write_index.load(Ordering::Acquire);
                }
                if read_index == unsafe { *self.reader.write_index_cache.get() } {
                    // No more items available
                    break;
                }
            }

            // Read the value from the buffer
            let value = unsafe { self.read_value(read_index) };

            // Calculate next read index (with wrap-around)
            let next_read_index = if read_index == self.reader.capacity_cache - 1 {
                0
            } else {
                read_index + 1
            };

            // Update read index BEFORE processing the value
            self.reader
                .read_index
                .store(next_read_index, Ordering::Relaxed);

            // Apply the function to the value
            target(value);

            read_index = next_read_index;
            count += 1;
        }

        (count, false) // DRO queue doesn't have close functionality
    }
}

// Convenience type aliases
pub type Spsc<T> = SpscQueue<T, HeapBuffer<T>>;
pub type SpscOnStack<T, const N: usize> = SpscQueue<T, StackBuffer<T, N>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let queue = Spsc::new_heap(4);

        assert_eq!(queue.capacity(), 4);
        assert_eq!(queue.size(), 0);
        assert!(queue.is_empty());

        // Push values
        queue.push(1);
        queue.push(2);
        assert_eq!(queue.size(), 2);

        // Pop values
        assert_eq!(queue.pop(), 1);
        assert_eq!(queue.pop(), 2);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_try_operations() {
        let queue = Spsc::new_heap(2);

        // Try push
        assert!(queue.try_push(1));
        assert!(queue.try_push(2));
        assert!(!queue.try_push(3)); // Full

        // Try pop
        assert_eq!(queue.try_pop(), Some(1));
        assert_eq!(queue.try_pop(), Some(2));
        assert_eq!(queue.try_pop(), None); // Empty
    }

    #[test]
    fn test_stack_queue() {
        let queue: SpscOnStack<i32, 4> = SpscOnStack::new_stack();

        assert!(queue.try_push(1));
        assert!(queue.try_push(2));
        assert_eq!(queue.size(), 2);

        assert_eq!(queue.try_pop(), Some(1));
        assert_eq!(queue.try_pop(), Some(2));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_wrap_around() {
        let queue = Spsc::new_heap(2);

        // Fill and empty multiple times to test wrap-around
        for i in 0..10 {
            queue.push(i);
            queue.push(i + 1);
            assert_eq!(queue.pop(), i);
            assert_eq!(queue.pop(), i + 1);
        }
    }

    #[test]
    fn test_force_operations() {
        let queue = Spsc::new_heap(1);

        queue.force_push(1);
        queue.force_push(2); // Overwrites

        assert_eq!(queue.pop(), 2);
    }

    #[test]
    fn test_drain_with() {
        let queue = Spsc::new_heap(10);

        // Push some items
        for i in 1..=6 {
            queue.push(i);
        }

        assert_eq!(queue.size(), 6);

        // Drain with a function that sums the values
        let mut sum = 0;
        let (drained, _was_closed) = queue.drain_with(|value| sum += value, 4);

        assert_eq!(drained, 4);
        assert_eq!(sum, 1 + 2 + 3 + 4);
        assert_eq!(queue.size(), 2);

        // Drain remaining items
        let mut collected = Vec::new();
        let (drained, _was_closed) = queue.drain_with(|value| collected.push(value), 10);

        assert_eq!(drained, 2);
        assert_eq!(collected, vec![5, 6]);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_drain_with_empty() {
        let queue = Spsc::new_heap(5);

        let mut called = false;
        let (drained, _was_closed) = queue.drain_with(|_: i32| called = true, 5);

        assert_eq!(drained, 0);
        assert!(!called);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_drain_with_wrap_around() {
        let queue = Spsc::new_heap(4);

        // Fill and drain multiple times to test wrap-around
        for round in 0..3 {
            // Fill queue
            for i in 0..4 {
                queue.push(round * 10 + i as i32);
            }

            // Drain and collect
            let mut collected: Vec<i32> = Vec::new();
            let (drained, _was_closed) = queue.drain_with(|value: i32| collected.push(value), 10);

            assert_eq!(drained, 4);
            assert_eq!(
                collected,
                vec![round * 10, round * 10 + 1, round * 10 + 2, round * 10 + 3]
            );
        }
    }
}
