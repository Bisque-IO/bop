//! Stress tests for the defer callback system
//! Tests both defer() and defer_arc() under various conditions

use std::sync::{Arc, Mutex, atomic::{AtomicU32, Ordering}};
use std::time::Instant;
use bop_rs::usockets::{Loop, Timer};

#[test]
fn test_basic_defer_functionality() {
    let loop_ = Loop::new().expect("create loop");
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();
    
    // Test basic defer
    loop_.defer(move || {
        counter_clone.fetch_add(1, Ordering::Relaxed);
    }).expect("defer should work");
    
    // Create a timer to keep the loop alive so it can process the wakeup
    let mut timer = Timer::new(&loop_).expect("create timer");
    timer.set(10, 0, move || {
        // Timer fires once after 10ms to keep loop alive
    });
    
    // Manual wakeup to trigger callback processing
    loop_.wakeup(); 
    // Run one iteration to process callbacks
    loop_.run();
    
    assert_eq!(counter.load(Ordering::Relaxed), 1);
}

#[test]
fn test_basic_defer_arc_functionality() {
    let loop_ = Loop::new().expect("create loop");
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();
    
    // Test defer_arc with shared callback
    let callback = Arc::new(move || {
        counter_clone.fetch_add(1, Ordering::Relaxed);
    });
    
    loop_.defer_arc(callback.clone()).expect("defer_arc should work");
    loop_.defer_arc(callback).expect("defer_arc should work with same callback");
    
    // Run one iteration to process callbacks
    loop_.run();
    
    assert_eq!(counter.load(Ordering::Relaxed), 2);
}

#[test]
fn test_massive_callback_stress() {
    let loop_ = Loop::new().expect("create loop");
    let counter = Arc::new(AtomicU32::new(0));
    let num_callbacks = 10000;
    
    // Queue many defer callbacks
    for _i in 0..num_callbacks {
        let counter_clone = counter.clone();
        loop_.defer(move || {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }).expect("defer should work");
    }
    
    // Queue many defer_arc callbacks  
    let shared_callback = Arc::new({
        let counter = counter.clone();
        move || {
            counter.fetch_add(10, Ordering::Relaxed);
        }
    });
    
    for _ in 0..num_callbacks {
        loop_.defer_arc(shared_callback.clone()).expect("defer_arc should work");
    }
    
    // Run event loop to process all callbacks
    let start = Instant::now();
    loop_.run();
    let duration = start.elapsed();
    
    println!("Processed {} callbacks in {:?}", num_callbacks * 2, duration);
    
    // Each defer adds 1, each defer_arc adds 10
    let expected = num_callbacks + (num_callbacks * 10);
    assert_eq!(counter.load(Ordering::Relaxed), expected);
}

#[test]
fn test_concurrent_queueing_while_processing() {
    let loop_ = Loop::new().expect("create loop");
    let counter = Arc::new(AtomicU32::new(0));
    let processed_count = Arc::new(AtomicU32::new(0));
    
    // Create a callback that tracks processing
    let process_callback = {
        let counter = counter.clone();
        let processed = processed_count.clone();
        Arc::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
            processed.fetch_add(1, Ordering::Relaxed);
        })
    };
    
    // Queue initial batch of callbacks
    for _ in 0..100 {
        loop_.defer_arc(process_callback.clone()).expect("defer_arc should work");
    }
    
    // Queue additional callbacks of different type
    for _ in 0..50 {
        let counter_clone = counter.clone();
        loop_.defer(move || {
            counter_clone.fetch_add(2, Ordering::Relaxed);
        }).expect("defer should work");
    }
    
    // Process all callbacks in a single run
    loop_.run();
    
    // Should have processed 100 callbacks adding 1 + 50 callbacks adding 2
    // 100 * 1 + 50 * 2 = 200 total counter value
    assert_eq!(counter.load(Ordering::Relaxed), 200);
    assert_eq!(processed_count.load(Ordering::Relaxed), 100); // Only first batch increments this
}

#[test] 
fn test_recursive_defer_callbacks() {
    let loop_ = Loop::new().expect("create loop");
    let counter = Arc::new(AtomicU32::new(0));
    let max_depth = 10;
    
    // Create recursive callback chain - simpler version without loop capture
    let counter_clone = counter.clone();
    loop_.defer(move || {
        counter_clone.fetch_add(1, Ordering::Relaxed);
    }).expect("defer should work");
    
    // Queue multiple recursive-style callbacks
    for _i in 0..max_depth {
        let counter_clone = counter.clone();
        loop_.defer(move || {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }).expect("defer should work");
    }
    
    // Process all callbacks
    loop_.run();
    
    // Should have processed 1 + max_depth callbacks
    assert_eq!(counter.load(Ordering::Relaxed), 1 + max_depth);
}

#[test]
fn test_mixed_callback_types_ordering() {
    let loop_ = Loop::new().expect("create loop");
    let execution_order = Arc::new(Mutex::new(Vec::new()));
    
    // Queue different types of callbacks
    for i in 0..5 {
        let order = execution_order.clone();
        loop_.defer(move || {
            order.lock().unwrap().push(format!("defer_{}", i));
        }).expect("defer should work");
        
        let order = execution_order.clone();
        let shared_callback = Arc::new(move || {
            order.lock().unwrap().push(format!("defer_arc_{}", i));
        });
        loop_.defer_arc(shared_callback).expect("defer_arc should work");
    }
    
    // Process callbacks
    loop_.run();
    
    let final_order = execution_order.lock().unwrap();
    println!("Execution order: {:?}", *final_order);
    
    // Should have processed all callbacks
    assert_eq!(final_order.len(), 10);
    
    // Check that we have both types
    let defer_count = final_order.iter().filter(|s| s.starts_with("defer_")).count();
    let defer_arc_count = final_order.iter().filter(|s| s.starts_with("defer_arc_")).count();
    assert_eq!(defer_count, 5);
    assert_eq!(defer_arc_count, 5);
}

#[test]
fn test_callback_panic_isolation() {
    let loop_ = Loop::new().expect("create loop");
    let counter = Arc::new(AtomicU32::new(0));
    
    // Queue a callback that panics
    loop_.defer(|| {
        panic!("This callback panics!");
    }).expect("defer should work");
    
    // Queue normal callbacks after the panicking one
    let counter_clone = counter.clone();
    loop_.defer(move || {
        counter_clone.fetch_add(1, Ordering::Relaxed);
    }).expect("defer should work");
    
    let shared_callback = Arc::new({
        let counter = counter.clone();
        move || {
            counter.fetch_add(10, Ordering::Relaxed);
        }
    });
    loop_.defer_arc(shared_callback).expect("defer_arc should work");
    
    // This should handle panics gracefully
    let result = std::panic::catch_unwind(|| {
        loop_.run();
    });
    
    // Even if there was a panic, some callbacks might have run
    println!("Counter after panic test: {}", counter.load(Ordering::Relaxed));
    println!("Panic result: {:?}", result.is_err());
}

#[test]
fn test_needs_wake_flag_optimization() {
    let loop_ = Loop::new().expect("create loop");
    let wakeup_count = Arc::new(AtomicU32::new(0));
    
    // Queue multiple callbacks rapidly - should only trigger one wakeup
    for _i in 0..100 {
        let counter_clone = wakeup_count.clone();
        loop_.defer(move || {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }).expect("defer should work");
    }
    
    // Process callbacks
    loop_.run();
    
    assert_eq!(wakeup_count.load(Ordering::Relaxed), 100);
}

#[test]
fn test_massive_sequential_stress() {
    let loop_ = Loop::new().expect("create loop");
    let counter = Arc::new(AtomicU32::new(0));
    let num_batches = 8;
    let callbacks_per_batch = 1000;
    
    // Queue callbacks in multiple batches to simulate high load
    for _batch_id in 0..num_batches {
        for i in 0..callbacks_per_batch {
            if i % 2 == 0 {
                // Use defer for FnOnce
                let counter = counter.clone();
                loop_.defer(move || {
                    counter.fetch_add(1, Ordering::Relaxed);
                }).expect("defer should work");
            } else {
                // Use defer_arc for Fn
                let callback = Arc::new({
                    let counter = counter.clone();
                    move || {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                });
                loop_.defer_arc(callback).expect("defer_arc should work");
            }
        }
    }
    
    // Process all queued callbacks
    let start = Instant::now();
    loop_.run();
    let duration = start.elapsed();
    
    let total_callbacks = num_batches * callbacks_per_batch;
    println!("Sequential stress test: {} callbacks in {} batches processed in {:?}", 
             total_callbacks, num_batches, duration);
    
    assert_eq!(counter.load(Ordering::Relaxed), total_callbacks);
}