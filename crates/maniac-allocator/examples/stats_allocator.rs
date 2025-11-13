//! Example demonstrating BOP's statistics-tracking allocator
//!
//! This example shows how to use the statistics allocator to track
//! memory usage patterns in your application. This is useful for
//! debugging memory leaks and profiling memory usage.
//!
//! Note: This example requires the "alloc-stats" feature to be enabled.

#[cfg(feature = "alloc-stats")]
use maniac_allocator::BopStatsAllocator;

#[cfg(feature = "alloc-stats")]
#[global_allocator]
static GLOBAL: BopStatsAllocator = BopStatsAllocator::new();

#[cfg(feature = "alloc-stats")]
fn main() {
    println!("BOP Statistics Allocator Example");
    println!("=================================");

    // Reset stats to start fresh
    GLOBAL.reset_stats();

    // Demonstrate various allocation patterns
    demonstrate_basic_allocations();
    demonstrate_collection_growth();
    demonstrate_memory_leaks();
    demonstrate_peak_usage();

    // Final statistics report
    final_report();
}

#[cfg(not(feature = "alloc-stats"))]
fn main() {
    println!("Statistics Allocator Example");
    println!("============================");
    println!("❌ This example requires the 'alloc-stats' feature to be enabled.");
    println!("Run with: cargo run --example stats_allocator --features alloc-stats");
}

#[cfg(feature = "alloc-stats")]
fn demonstrate_basic_allocations() {
    println!("\n📊 Basic Allocations Test");
    println!("--------------------------");

    let stats_before = GLOBAL.stats();
    println!("Before: {}", stats_before);

    {
        let vec: Vec<u64> = (0..1000).collect();
        let string = "Hello, BOP allocator!".repeat(50);
        let boxed = Box::new([42u32; 100]);

        let stats_during = GLOBAL.stats();
        println!("During: {}", stats_during);

        println!("Allocated {} items in vector", vec.len());
        println!("String length: {} chars", string.len());
        println!("Boxed array size: {} elements", boxed.len());
    } // All allocations should be freed here

    let stats_after = GLOBAL.stats();
    println!("After:  {}", stats_after);

    if stats_after.has_leaks() {
        println!(
            "⚠️  Memory leaks detected: {} outstanding allocations",
            stats_after.outstanding_allocations()
        );
    } else {
        println!("✅ No memory leaks detected");
    }
}

#[cfg(feature = "alloc-stats")]
fn demonstrate_collection_growth() {
    println!("\n📈 Collection Growth Pattern");
    println!("-----------------------------");

    let mut vec = Vec::new();
    let mut stats_history = Vec::new();

    for i in 0..5 {
        let items_to_add = 1000 * (i + 1);

        for j in 0..items_to_add {
            vec.push(format!("item_{}_{}", i, j));
        }

        let stats = GLOBAL.stats();
        stats_history.push((vec.len(), stats));

        println!(
            "After adding {} items (total: {}): Current memory: {} bytes, Peak: {} bytes",
            items_to_add,
            vec.len(),
            stats.current_memory,
            stats.peak_memory
        );
    }

    // Clear the vector and check for memory release
    vec.clear();
    vec.shrink_to_fit();

    let final_stats = GLOBAL.stats();
    println!(
        "After clearing vector: Current memory: {} bytes",
        final_stats.current_memory
    );
}

#[cfg(feature = "alloc-stats")]
fn demonstrate_memory_leaks() {
    println!("\n🔍 Memory Leak Detection");
    println!("-------------------------");

    let stats_before = GLOBAL.stats();

    {
        // Intentionally create some "leaked" memory by using Box::into_raw
        let _leaked_boxes: Vec<*mut u64> = (0..10).map(|i| Box::into_raw(Box::new(i))).collect();

        // Note: We're not deallocating these boxes, simulating a memory leak
        println!("⚠️  Intentionally created 10 leaked allocations");

        let stats_with_leaks = GLOBAL.stats();
        println!("With leaks: {}", stats_with_leaks);

        if stats_with_leaks.outstanding_allocations() > stats_before.outstanding_allocations() {
            println!(
                "🔴 Leak detected: {} new outstanding allocations",
                stats_with_leaks.outstanding_allocations() - stats_before.outstanding_allocations()
            );
        }
    }

    // The vector holding the pointers is freed, but the boxes they point to are not
    let stats_after = GLOBAL.stats();
    println!("After scope: {}", stats_after);

    // In a real application, you would track down and fix these leaks
    // For this example, we'll just demonstrate detection
}

#[cfg(feature = "alloc-stats")]
fn demonstrate_peak_usage() {
    println!("\n⛰️  Peak Memory Usage Tracking");
    println!("-------------------------------");

    let initial_stats = GLOBAL.stats();
    let initial_peak = initial_stats.peak_memory;

    // Create large temporary allocations
    for size_mb in [1, 5, 10, 3, 7] {
        {
            let size_bytes = size_mb * 1024 * 1024;
            let large_vec: Vec<u8> = vec![0; size_bytes];

            let stats = GLOBAL.stats();
            println!(
                "Allocated {} MB: Current = {} MB, Peak = {} MB",
                size_mb,
                stats.current_memory / (1024 * 1024),
                stats.peak_memory / (1024 * 1024)
            );

            // Use the memory so it doesn't get optimized away
            assert_eq!(large_vec.len(), size_bytes);
        } // Memory freed here

        let stats_after = GLOBAL.stats();
        println!(
            "After freeing {} MB: Current = {} MB, Peak = {} MB",
            size_mb,
            stats_after.current_memory / (1024 * 1024),
            stats_after.peak_memory / (1024 * 1024)
        );
    }

    let final_stats = GLOBAL.stats();
    let peak_increase = final_stats.peak_memory - initial_peak;
    println!(
        "Total peak increase during test: {} MB",
        peak_increase / (1024 * 1024)
    );
}

#[cfg(feature = "alloc-stats")]
fn final_report() {
    println!("\n📋 Final Statistics Report");
    println!("==========================");

    let stats = GLOBAL.stats();

    println!("Total allocations performed: {}", stats.allocations);
    println!("Total deallocations performed: {}", stats.deallocations);
    println!(
        "Outstanding allocations: {}",
        stats.outstanding_allocations()
    );
    println!(
        "Total bytes allocated: {} ({:.2} MB)",
        stats.bytes_allocated,
        stats.bytes_allocated as f64 / (1024.0 * 1024.0)
    );
    println!(
        "Total bytes deallocated: {} ({:.2} MB)",
        stats.bytes_deallocated,
        stats.bytes_deallocated as f64 / (1024.0 * 1024.0)
    );
    println!(
        "Current memory usage: {} ({:.2} MB)",
        stats.current_memory,
        stats.current_memory as f64 / (1024.0 * 1024.0)
    );
    println!(
        "Peak memory usage: {} ({:.2} MB)",
        stats.peak_memory,
        stats.peak_memory as f64 / (1024.0 * 1024.0)
    );

    if stats.has_leaks() {
        println!("\n🔴 MEMORY LEAKS DETECTED:");
        println!(
            "  - {} outstanding allocations",
            stats.outstanding_allocations()
        );
        println!(
            "  - ~{} bytes potentially leaked",
            stats.outstanding_memory()
        );
        println!("  - Consider reviewing allocation/deallocation patterns");
    } else {
        println!("\n✅ No memory leaks detected - all allocations properly freed!");
    }

    // Efficiency metrics
    if stats.allocations > 0 {
        let avg_alloc_size = stats.bytes_allocated / stats.allocations;
        println!("\n📈 Efficiency Metrics:");
        println!("  - Average allocation size: {} bytes", avg_alloc_size);
        println!(
            "  - Allocation/deallocation ratio: {:.2}",
            stats.allocations as f64 / stats.deallocations.max(1) as f64
        );
    }
}
