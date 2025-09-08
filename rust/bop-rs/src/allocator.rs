//! Global allocator implementation using BOP's custom allocation functions
//!
//! This module provides a custom global allocator that uses BOP's high-performance
//! allocator instead of the system allocator. This can provide better performance
//! and memory usage characteristics for BOP applications.

use bop_sys::*;
use std::alloc::{GlobalAlloc, Layout};
use std::ptr;

/// BOP Global Allocator
///
/// A custom allocator implementation that uses BOP's allocation functions.
/// BOP uses a high-performance allocator (likely snmalloc) that can provide
/// better performance characteristics than the system allocator.
///
/// # Usage
///
/// ```rust
/// use bop_rs::allocator::BopAllocator;
///
/// #[global_allocator]
/// static GLOBAL: BopAllocator = BopAllocator;
/// ```
///
/// # Safety
///
/// This allocator assumes that:
/// - BOP's allocation functions are properly initialized
/// - The BOP library is linked and available
/// - All allocations/deallocations go through BOP's allocator consistently
pub struct BopAllocator;

unsafe impl GlobalAlloc for BopAllocator {
    /// Allocate memory using BOP's allocator
    ///
    /// This uses `bop_alloc_aligned` to ensure proper alignment requirements
    /// are met for all allocations.
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return ptr::NonNull::dangling().as_ptr();
        }

        let ptr = unsafe { bop_alloc_aligned(layout.align(), layout.size()) };
        ptr as *mut u8
    }

    /// Deallocate memory using BOP's deallocator
    ///
    /// Uses `bop_dealloc_sized` which can be more efficient as it provides
    /// size information to the allocator.
    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() == 0 {
            return;
        }

        unsafe { bop_dealloc_sized(ptr as *mut std::ffi::c_void, layout.size()); }
    }

    /// Reallocate memory using BOP's realloc function
    ///
    /// This provides an optimized reallocation path that may avoid
    /// copying data when possible.
    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size == 0 {
            self.dealloc(ptr, layout);
            return ptr::NonNull::dangling().as_ptr();
        }

        if layout.size() == 0 {
            return self.alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
        }

        let new_ptr = bop_realloc(
            ptr as *mut std::ffi::c_void,
            new_size,
        );
        new_ptr as *mut u8
    }

    /// Allocate zeroed memory using BOP's zero allocator
    ///
    /// This uses `bop_zalloc_aligned` which can be more efficient than
    /// allocating and then zeroing the memory.
    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return ptr::NonNull::dangling().as_ptr();
        }

        let ptr = bop_zalloc_aligned(layout.align(), layout.size());
        ptr as *mut u8
    }
}

/// BOP Statistics Allocator
///
/// A wrapper allocator that tracks allocation statistics while delegating
/// to BOP's allocator. Useful for debugging and profiling memory usage.
#[cfg(feature = "alloc-stats")]
pub struct BopStatsAllocator {
    allocations: std::sync::atomic::AtomicUsize,
    deallocations: std::sync::atomic::AtomicUsize,
    bytes_allocated: std::sync::atomic::AtomicUsize,
    bytes_deallocated: std::sync::atomic::AtomicUsize,
    peak_memory: std::sync::atomic::AtomicUsize,
    current_memory: std::sync::atomic::AtomicUsize,
}

#[cfg(feature = "alloc-stats")]
impl BopStatsAllocator {
    /// Create a new statistics-tracking allocator
    pub const fn new() -> Self {
        Self {
            allocations: std::sync::atomic::AtomicUsize::new(0),
            deallocations: std::sync::atomic::AtomicUsize::new(0),
            bytes_allocated: std::sync::atomic::AtomicUsize::new(0),
            bytes_deallocated: std::sync::atomic::AtomicUsize::new(0),
            peak_memory: std::sync::atomic::AtomicUsize::new(0),
            current_memory: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Get the current allocation statistics
    pub fn stats(&self) -> AllocationStats {
        use std::sync::atomic::Ordering;
        
        AllocationStats {
            allocations: self.allocations.load(Ordering::Relaxed),
            deallocations: self.deallocations.load(Ordering::Relaxed),
            bytes_allocated: self.bytes_allocated.load(Ordering::Relaxed),
            bytes_deallocated: self.bytes_deallocated.load(Ordering::Relaxed),
            peak_memory: self.peak_memory.load(Ordering::Relaxed),
            current_memory: self.current_memory.load(Ordering::Relaxed),
        }
    }

    /// Reset all statistics to zero
    pub fn reset_stats(&self) {
        use std::sync::atomic::Ordering;
        
        self.allocations.store(0, Ordering::Relaxed);
        self.deallocations.store(0, Ordering::Relaxed);
        self.bytes_allocated.store(0, Ordering::Relaxed);
        self.bytes_deallocated.store(0, Ordering::Relaxed);
        self.peak_memory.store(0, Ordering::Relaxed);
        self.current_memory.store(0, Ordering::Relaxed);
    }
}

#[cfg(feature = "alloc-stats")]
unsafe impl GlobalAlloc for BopStatsAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        use std::sync::atomic::Ordering;

        if layout.size() == 0 {
            return ptr::NonNull::dangling().as_ptr();
        }

        let ptr = bop_alloc_aligned(layout.align(), layout.size());
        
        if !ptr.is_null() {
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.bytes_allocated.fetch_add(layout.size(), Ordering::Relaxed);
            
            let current = self.current_memory.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            
            // Update peak memory usage
            let mut peak = self.peak_memory.load(Ordering::Relaxed);
            while current > peak {
                match self.peak_memory.compare_exchange_weak(peak, current, Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => break,
                    Err(new_peak) => peak = new_peak,
                }
            }
        }

        ptr as *mut u8
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        use std::sync::atomic::Ordering;

        if layout.size() == 0 {
            return;
        }

        bop_dealloc_sized(ptr as *mut std::ffi::c_void, layout.size());
        
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        self.bytes_deallocated.fetch_add(layout.size(), Ordering::Relaxed);
        self.current_memory.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        use std::sync::atomic::Ordering;

        if new_size == 0 {
            self.dealloc(ptr, layout);
            return ptr::NonNull::dangling().as_ptr();
        }

        if layout.size() == 0 {
            return self.alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
        }

        let new_ptr = bop_realloc(
            ptr as *mut std::ffi::c_void,
            new_size,
        );

        if !new_ptr.is_null() {
            // Update statistics for the size change
            let size_diff = new_size as isize - layout.size() as isize;
            if size_diff > 0 {
                self.bytes_allocated.fetch_add(size_diff as usize, Ordering::Relaxed);
                let current = self.current_memory.fetch_add(size_diff as usize, Ordering::Relaxed) + size_diff as usize;
                
                // Update peak memory usage
                let mut peak = self.peak_memory.load(Ordering::Relaxed);
                while current > peak {
                    match self.peak_memory.compare_exchange_weak(peak, current, Ordering::Relaxed, Ordering::Relaxed) {
                        Ok(_) => break,
                        Err(new_peak) => peak = new_peak,
                    }
                }
            } else if size_diff < 0 {
                self.bytes_deallocated.fetch_add((-size_diff) as usize, Ordering::Relaxed);
                self.current_memory.fetch_sub((-size_diff) as usize, Ordering::Relaxed);
            }
        }

        new_ptr as *mut u8
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        use std::sync::atomic::Ordering;

        if layout.size() == 0 {
            return ptr::NonNull::dangling().as_ptr();
        }

        let ptr = bop_zalloc_aligned(layout.align(), layout.size());
        
        if !ptr.is_null() {
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.bytes_allocated.fetch_add(layout.size(), Ordering::Relaxed);
            
            let current = self.current_memory.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            
            // Update peak memory usage
            let mut peak = self.peak_memory.load(Ordering::Relaxed);
            while current > peak {
                match self.peak_memory.compare_exchange_weak(peak, current, Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => break,
                    Err(new_peak) => peak = new_peak,
                }
            }
        }

        ptr as *mut u8
    }
}

/// Allocation statistics
#[cfg(feature = "alloc-stats")]
#[derive(Debug, Clone, Copy)]
pub struct AllocationStats {
    /// Total number of allocations performed
    pub allocations: usize,
    /// Total number of deallocations performed
    pub deallocations: usize,
    /// Total bytes allocated
    pub bytes_allocated: usize,
    /// Total bytes deallocated
    pub bytes_deallocated: usize,
    /// Peak memory usage (high watermark)
    pub peak_memory: usize,
    /// Current memory usage
    pub current_memory: usize,
}

#[cfg(feature = "alloc-stats")]
impl AllocationStats {
    /// Get the number of outstanding allocations
    pub fn outstanding_allocations(&self) -> usize {
        self.allocations.saturating_sub(self.deallocations)
    }

    /// Get the amount of outstanding memory
    pub fn outstanding_memory(&self) -> usize {
        self.bytes_allocated.saturating_sub(self.bytes_deallocated)
    }

    /// Check if there are memory leaks (outstanding allocations)
    pub fn has_leaks(&self) -> bool {
        self.outstanding_allocations() > 0
    }
}

#[cfg(feature = "alloc-stats")]
impl std::fmt::Display for AllocationStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, 
            "Allocations: {} | Deallocations: {} | Outstanding: {} | Current Memory: {} bytes | Peak Memory: {} bytes",
            self.allocations,
            self.deallocations, 
            self.outstanding_allocations(),
            self.current_memory,
            self.peak_memory
        )
    }
}

/// Utility functions for working with BOP's allocator directly
pub mod utils {
    use super::*;

    /// Get the usable size of an allocation
    ///
    /// This can be larger than the requested size due to internal
    /// allocator alignment and chunking strategies.
    #[inline]
    pub unsafe fn malloc_usable_size(ptr: *const u8) -> usize {
        unsafe { bop_malloc_usable_size(ptr as *const std::ffi::c_void) }
    }

    /// Allocate memory directly using BOP's allocator
    ///
    /// This is a low-level function that bypasses Rust's allocation APIs.
    /// Use with caution and ensure proper deallocation.
    #[inline]
    pub unsafe fn alloc(size: usize) -> *mut u8 {
        unsafe { bop_alloc(size) as *mut u8 }
    }

    /// Allocate zeroed memory directly using BOP's allocator
    #[inline]
    pub unsafe fn zalloc(size: usize) -> *mut u8 {
        unsafe { bop_zalloc(size) as *mut u8 }
    }

    /// Allocate aligned memory directly using BOP's allocator
    #[inline]
    pub unsafe fn alloc_aligned(alignment: usize, size: usize) -> *mut u8 {
        unsafe { bop_alloc_aligned(alignment, size) as *mut u8 }
    }

    /// Allocate zeroed aligned memory directly using BOP's allocator
    #[inline]
    pub unsafe fn zalloc_aligned(alignment: usize, size: usize) -> *mut u8 {
        unsafe { bop_zalloc_aligned(alignment, size) as *mut u8 }
    }

    /// Deallocate memory using BOP's allocator
    #[inline]
    pub unsafe fn dealloc(ptr: *mut u8) {
        unsafe { bop_dealloc(ptr as *mut std::ffi::c_void) }
    }

    /// Deallocate memory with size hint using BOP's allocator
    #[inline]
    pub unsafe fn dealloc_sized(ptr: *mut u8, size: usize) {
        unsafe { bop_dealloc_sized(ptr as *mut std::ffi::c_void, size) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{Layout, GlobalAlloc};

    #[test]
    fn test_bop_allocator_basic() {
        let allocator = BopAllocator;
        let layout = Layout::new::<u64>();

        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null());
            
            // Write and read to ensure the memory is valid
            *(ptr as *mut u64) = 0x1234567890ABCDEF;
            assert_eq!(*(ptr as *const u64), 0x1234567890ABCDEF);
            
            allocator.dealloc(ptr, layout);
        }
    }

    #[test]
    fn test_bop_allocator_zero_size() {
        let allocator = BopAllocator;
        let layout = Layout::from_size_align(0, 1).unwrap();

        unsafe {
            let ptr = allocator.alloc(layout);
            // Should return a non-null dangling pointer for zero-size allocations
            assert!(!ptr.is_null());
            
            // Deallocation should be safe
            allocator.dealloc(ptr, layout);
        }
    }

    #[test]
    fn test_bop_allocator_alloc_zeroed() {
        let allocator = BopAllocator;
        let layout = Layout::from_size_align(64, 8).unwrap();

        unsafe {
            let ptr = allocator.alloc_zeroed(layout);
            assert!(!ptr.is_null());
            
            // Check that memory is zeroed
            for i in 0..64 {
                assert_eq!(*ptr.offset(i), 0);
            }
            
            allocator.dealloc(ptr, layout);
        }
    }

    #[test]
    fn test_bop_allocator_realloc() {
        let allocator = BopAllocator;
        let layout = Layout::from_size_align(32, 8).unwrap();

        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null());
            
            // Write some data
            *(ptr as *mut u64) = 0xDEADBEEF;
            
            // Reallocate to larger size
            let new_ptr = allocator.realloc(ptr, layout, 64);
            assert!(!new_ptr.is_null());
            
            // Data should be preserved
            assert_eq!(*(new_ptr as *const u64), 0xDEADBEEF);
            
            // Clean up
            let new_layout = Layout::from_size_align(64, 8).unwrap();
            allocator.dealloc(new_ptr, new_layout);
        }
    }

    #[cfg(feature = "alloc-stats")]
    #[test]
    fn test_stats_allocator() {
        let allocator = BopStatsAllocator::new();
        let layout = Layout::new::<u64>();

        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null());
            
            let stats = allocator.stats();
            assert_eq!(stats.allocations, 1);
            assert_eq!(stats.deallocations, 0);
            assert_eq!(stats.outstanding_allocations(), 1);
            assert!(stats.current_memory >= layout.size());
            
            allocator.dealloc(ptr, layout);
            
            let stats = allocator.stats();
            assert_eq!(stats.allocations, 1);
            assert_eq!(stats.deallocations, 1);
            assert_eq!(stats.outstanding_allocations(), 0);
        }
    }

    #[test]
    fn test_utils() {
        unsafe {
            let ptr = utils::alloc(64);
            assert!(!ptr.is_null());
            
            let usable_size = utils::malloc_usable_size(ptr);
            assert!(usable_size >= 64);
            
            utils::dealloc_sized(ptr, 64);
        }
    }
}