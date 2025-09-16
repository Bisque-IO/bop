//! Example demonstrating BOP's global allocator
//!
//! This example shows how to use BOP's high-performance allocator as the
//! global allocator for your Rust application.

use bop_rs::allocator::BopAllocator;
use std::{
    alloc::{GlobalAlloc, Layout},
    collections::HashMap,
};

// Set BOP's allocator as the global allocator
#[global_allocator]
static GLOBAL: BopAllocator = BopAllocator;

fn main() {
    println!("BOP Global Allocator Example");
    println!("============================");

    // All heap allocations will now use BOP's allocator
    demonstrate_heap_allocations();
    demonstrate_collections();
    demonstrate_custom_types();
    demonstrate_direct_allocation();

    println!("✅ All allocations completed successfully using BOP's allocator!");
}

fn demonstrate_heap_allocations() {
    println!("\n📦 Demonstrating heap allocations:");

    // Box allocations use the global allocator
    let boxed_value = Box::new(42u64);
    println!("Boxed value: {}", boxed_value);

    // String allocations use the global allocator
    let mut string = String::with_capacity(100);
    string.push_str("Hello from BOP's allocator!");
    println!("String: {}", string);

    // Vector allocations use the global allocator
    let mut vec = Vec::with_capacity(1000);
    for i in 0..100 {
        vec.push(i);
    }
    println!(
        "Vector with {} elements, capacity: {}",
        vec.len(),
        vec.capacity()
    );
}

fn demonstrate_collections() {
    println!("\n🗂️  Demonstrating collection allocations:");

    use std::collections::{BTreeMap, HashMap, HashSet};

    // HashMap uses the global allocator
    let mut map = HashMap::new();
    for i in 0..50 {
        map.insert(format!("key_{}", i), i * 2);
    }
    println!("HashMap with {} entries", map.len());

    // BTreeMap uses the global allocator
    let mut btree = BTreeMap::new();
    for i in 0..30 {
        btree.insert(i, format!("value_{}", i));
    }
    println!("BTreeMap with {} entries", btree.len());

    // HashSet uses the global allocator
    let mut set = HashSet::new();
    for i in 0..20 {
        set.insert(i * 3);
    }
    println!("HashSet with {} entries", set.len());
}

fn demonstrate_custom_types() {
    println!("\n🏗️  Demonstrating custom type allocations:");

    #[derive(Debug)]
    struct CustomStruct {
        data: Vec<String>,
        metadata: HashMap<String, u64>,
    }

    impl CustomStruct {
        fn new() -> Self {
            let mut data = Vec::new();
            let mut metadata = HashMap::new();

            for i in 0..10 {
                data.push(format!("item_{}", i));
                metadata.insert(format!("meta_{}", i), i as u64);
            }

            Self { data, metadata }
        }
    }

    let custom = CustomStruct::new();
    println!(
        "Custom struct with {} data items and {} metadata entries",
        custom.data.len(),
        custom.metadata.len()
    );

    // Allocate many instances
    let instances: Vec<CustomStruct> = (0..10).map(|_| CustomStruct::new()).collect();

    println!("Created {} custom struct instances", instances.len());
}

fn demonstrate_direct_allocation() {
    println!("\n⚙️  Demonstrating direct allocator usage:");

    use bop_rs::allocator::utils;

    unsafe {
        // Direct allocation using BOP's functions
        let ptr = utils::alloc(1024);
        if !ptr.is_null() {
            println!("✅ Direct allocation of 1KB successful");

            // Get usable size (may be larger than requested)
            let usable_size = utils::malloc_usable_size(ptr);
            println!("Usable size: {} bytes (requested 1024)", usable_size);

            // Write some data
            std::ptr::write_bytes(ptr, 0xAB, 1024);

            // Read it back to verify
            let first_byte = *ptr;
            let last_byte = *ptr.offset(1023);
            println!(
                "First byte: 0x{:02X}, Last byte: 0x{:02X}",
                first_byte, last_byte
            );

            // Deallocate
            utils::dealloc_sized(ptr, 1024);
            println!("✅ Deallocation successful");
        }

        // Aligned allocation
        let aligned_ptr = utils::alloc_aligned(64, 512); // 64-byte aligned, 512 bytes
        if !aligned_ptr.is_null() {
            println!("✅ Aligned allocation (64-byte alignment, 512 bytes) successful");
            println!("Pointer address: {:p}", aligned_ptr);
            println!("Is 64-byte aligned: {}", (aligned_ptr as usize) % 64 == 0);

            utils::dealloc_sized(aligned_ptr, 512);
            println!("✅ Aligned deallocation successful");
        }

        // Zero-initialized allocation
        let zero_ptr = utils::zalloc(256);
        if !zero_ptr.is_null() {
            println!("✅ Zero-initialized allocation (256 bytes) successful");

            // Verify it's zeroed
            let is_zeroed = (0..256).all(|i| *zero_ptr.offset(i) == 0);
            println!("Memory is properly zeroed: {}", is_zeroed);

            utils::dealloc_sized(zero_ptr, 256);
            println!("✅ Zero-initialized deallocation successful");
        }
    }
}

/// Example of using the allocator with custom GlobalAlloc trait
fn demonstrate_custom_allocation() {
    println!("\n🔧 Demonstrating custom GlobalAlloc usage:");

    unsafe {
        let layout = Layout::from_size_align(128, 8).unwrap();

        // Use the allocator directly
        let ptr = GLOBAL.alloc(layout);
        if !ptr.is_null() {
            println!("✅ Custom allocation successful");

            // Write and verify
            std::ptr::write(ptr as *mut u64, 0x1234567890ABCDEF);
            let value = std::ptr::read(ptr as *const u64);
            println!("Written and read value: 0x{:016X}", value);

            GLOBAL.dealloc(ptr, layout);
            println!("✅ Custom deallocation successful");
        }

        // Test alloc_zeroed
        let zero_ptr = GLOBAL.alloc_zeroed(layout);
        if !zero_ptr.is_null() {
            let value = std::ptr::read(zero_ptr as *const u64);
            println!("Zero-allocated value: 0x{:016X} (should be 0)", value);

            GLOBAL.dealloc(zero_ptr, layout);
        }

        // Test realloc
        let initial_ptr = GLOBAL.alloc(layout);
        if !initial_ptr.is_null() {
            std::ptr::write(initial_ptr as *mut u64, 0xDEADBEEF);

            let expanded_ptr = GLOBAL.realloc(initial_ptr, layout, 256);
            if !expanded_ptr.is_null() {
                let preserved_value = std::ptr::read(expanded_ptr as *const u64);
                println!(
                    "Reallocated and preserved value: 0x{:08X}",
                    preserved_value as u32
                );

                let new_layout = Layout::from_size_align(256, 8).unwrap();
                GLOBAL.dealloc(expanded_ptr, new_layout);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_allocator() {
        // Simple test that the global allocator works
        let vec: Vec<u32> = (0..1000).collect();
        assert_eq!(vec.len(), 1000);
        assert_eq!(vec[999], 999);
    }

    #[test]
    fn test_string_allocation() {
        let mut s = String::with_capacity(1000);
        s.push_str("Testing BOP allocator with strings");
        assert!(s.len() > 0);
        assert!(s.capacity() >= 1000);
    }
}
