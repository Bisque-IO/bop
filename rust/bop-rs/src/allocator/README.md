# BOP Global Allocator

This module provides a high-performance global allocator implementation using BOP's custom allocation functions. BOP uses an advanced memory allocator (likely snmalloc) that can provide better performance characteristics than the system allocator.

## Features

- **High Performance**: Uses BOP's optimized allocator backend
- **Drop-in Replacement**: Compatible with Rust's GlobalAlloc trait
- **Memory Statistics**: Optional allocation tracking and profiling
- **Alignment Support**: Proper handling of memory alignment requirements
- **Zero Initialization**: Efficient zero-memory allocation
- **Size Hints**: Optimized deallocation with size information

## Usage

### Basic Global Allocator

```rust
use bop_rs::allocator::BopAllocator;

#[global_allocator]
static GLOBAL: BopAllocator = BopAllocator;

fn main() {
    // All heap allocations now use BOP's allocator
    let vec = vec![1, 2, 3, 4, 5];  // Uses BOP allocator
    let string = String::from("Hello, BOP!");  // Uses BOP allocator
    let boxed = Box::new(42);  // Uses BOP allocator
}
```

### Statistics Tracking Allocator

Enable the `alloc-stats` feature for memory profiling:

```toml
[dependencies]
bop-rs = { version = "0.1", features = ["alloc-stats"] }
```

```rust
#[cfg(feature = "alloc-stats")]
use bop_rs::allocator::BopStatsAllocator;

#[cfg(feature = "alloc-stats")]
#[global_allocator]
static GLOBAL: BopStatsAllocator = BopStatsAllocator::new();

fn main() {
    // Your application code...
    
    // Get allocation statistics
    let stats = GLOBAL.stats();
    println!("Memory usage: {}", stats);
    println!("Peak memory: {} bytes", stats.peak_memory);
    
    if stats.has_leaks() {
        println!("Memory leaks detected: {} allocations", 
                 stats.outstanding_allocations());
    }
}
```

### Direct Allocator Usage

For low-level control, use the utility functions:

```rust
use bop_rs::allocator::utils;

unsafe {
    // Direct allocation
    let ptr = utils::alloc(1024);
    
    // Aligned allocation  
    let aligned_ptr = utils::alloc_aligned(64, 512);  // 64-byte aligned
    
    // Zero-initialized allocation
    let zero_ptr = utils::zalloc(256);
    
    // Get actual usable size
    let usable = utils::malloc_usable_size(ptr);
    
    // Deallocation with size hint (more efficient)
    utils::dealloc_sized(ptr, 1024);
    utils::dealloc_sized(aligned_ptr, 512);  
    utils::dealloc_sized(zero_ptr, 256);
}
```

## API Reference

### BopAllocator

The main global allocator implementation.

- `unsafe fn alloc(&self, layout: Layout) -> *mut u8`
- `unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout)`
- `unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8`
- `unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8`

### BopStatsAllocator (feature = "alloc-stats")

Statistics-tracking allocator wrapper.

- All methods from `BopAllocator`
- `fn stats(&self) -> AllocationStats`
- `fn reset_stats(&self)`

### AllocationStats (feature = "alloc-stats")

Memory usage statistics.

```rust
pub struct AllocationStats {
    pub allocations: usize,          // Total allocations
    pub deallocations: usize,        // Total deallocations  
    pub bytes_allocated: usize,      // Total bytes allocated
    pub bytes_deallocated: usize,    // Total bytes deallocated
    pub peak_memory: usize,          // Peak memory usage
    pub current_memory: usize,       // Current memory usage
}

impl AllocationStats {
    pub fn outstanding_allocations(&self) -> usize;
    pub fn outstanding_memory(&self) -> usize;
    pub fn has_leaks(&self) -> bool;
}
```

### Utility Functions

Direct access to BOP's allocation functions:

- `unsafe fn alloc(size: usize) -> *mut u8`
- `unsafe fn zalloc(size: usize) -> *mut u8`  
- `unsafe fn alloc_aligned(alignment: usize, size: usize) -> *mut u8`
- `unsafe fn zalloc_aligned(alignment: usize, size: usize) -> *mut u8`
- `unsafe fn dealloc(ptr: *mut u8)`
- `unsafe fn dealloc_sized(ptr: *mut u8, size: usize)`
- `unsafe fn malloc_usable_size(ptr: *const u8) -> usize`

## Performance Considerations

1. **Alignment**: The allocator always uses aligned allocation to meet Rust's requirements
2. **Size Hints**: Deallocation with size hints (`dealloc_sized`) is more efficient
3. **Zero Allocation**: `alloc_zeroed` and `zalloc` are optimized for zero-initialized memory
4. **Reallocation**: `realloc` can avoid copying when growing/shrinking allocations
5. **Thread Safety**: All operations are thread-safe and lock-free

## Memory Safety

- All allocations are properly aligned according to the requested `Layout`
- Zero-sized allocations return non-null dangling pointers as required by Rust
- The allocator properly handles edge cases (zero size, realloc to zero, etc.)
- RAII semantics ensure automatic cleanup when using Rust's standard types

## Debugging and Profiling

The statistics allocator provides comprehensive memory usage tracking:

1. **Leak Detection**: Identifies outstanding allocations
2. **Peak Usage**: Tracks maximum memory consumption
3. **Allocation Patterns**: Monitor allocation/deallocation frequency
4. **Memory Efficiency**: Calculate average allocation sizes

Use the statistics to optimize your application's memory usage patterns and identify potential memory leaks.

## Examples

See the `examples/` directory for complete usage examples:

- `global_allocator.rs`: Basic usage as a global allocator
- `stats_allocator.rs`: Memory profiling with statistics tracking

## Thread Safety

All allocator operations are thread-safe. The underlying BOP allocator is designed for high-performance concurrent access with minimal locking overhead.

## Integration with Other Allocators  

The BOP allocator can be used alongside other specialized allocators:

```rust
use bop_rs::allocator::BopAllocator;
use some_other_crate::SpecializedAllocator;

#[global_allocator] 
static GLOBAL: BopAllocator = BopAllocator;

fn use_specialized_allocator() {
    let specialized = SpecializedAllocator::new();
    // Use specialized allocator for specific use cases
    // while BOP handles general allocations
}
```