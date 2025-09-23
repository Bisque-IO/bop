// //! Test Timer operations using run_on_loop optimization

// use std::sync::{Arc, atomic::{AtomicU32, AtomicBool, Ordering}};
// use std::thread;
// use std::time::Duration;
// use bop_rs::usockets::{LoopInner, Timer};

// #[test]
// fn test_timer_set_inline_when_on_loop_thread() {
//     let loop_ = Arc::new(LoopInner::new().expect("create loop"));
//     let timer_set_count = Arc::new(AtomicU32::new(0));
//     let callback_fired_count = Arc::new(AtomicU32::new(0));
    
//     let loop_clone = loop_.clone();
//     let timer_set_clone = timer_set_count.clone();
//     let callback_fired_clone = callback_fired_count.clone();
    
//     // Create timer from main thread (should defer)
//     let timer = Timer::new(&loop_).expect("create timer");
    
//     // Set timer from within a deferred callback (should be inline)
//     loop_.defer(move || {
//         // Now we're on the loop thread, so timer.set should execute inline
//         let timer_clone = timer.clone();
//         timer.set(10, 0, move || {
//             callback_fired_clone.fetch_add(1, Ordering::Relaxed);
//         }).expect("set timer from loop thread");
        
//         timer_set_clone.fetch_add(1, Ordering::Relaxed);
        
//         // Set timer again - should also be inline
//         timer_clone.set(20, 0, move || {
//             // This callback won't fire since we'll override it
//         }).expect("set timer again from loop thread");
        
//         timer_set_clone.fetch_add(1, Ordering::Relaxed);
//     }).expect("defer callback");
    
//     // Run the loop to process operations
//     let handle = thread::spawn(move || {
//         loop_clone.run();
//     });
    
//     // Give time for operations to complete
//     thread::sleep(Duration::from_millis(100));
    
//     handle.join().expect("loop thread join");
    
//     // Both timer.set operations should have completed
//     assert_eq!(timer_set_count.load(Ordering::Relaxed), 2, "Both timer.set calls should have completed");
    
//     // At least one timer should have fired
//     assert!(callback_fired_count.load(Ordering::Relaxed) > 0, "Timer callback should have fired");
// }

// #[test]
// fn test_timer_close_inline_optimization() {
//     let loop_ = Arc::new(LoopInner::new().expect("create loop"));
//     let close_completed = Arc::new(AtomicBool::new(false));
    
//     let loop_clone = loop_.clone();
//     let close_clone = close_completed.clone();
    
//     // Create timer
//     let timer = Timer::new(&loop_).expect("create timer");
    
//     // Close timer from within loop thread (should be inline)
//     loop_.defer(move || {
//         // Set up the timer first
//         timer.set(1000, 0, move || {
//             // This callback should not fire since we close the timer
//         }).expect("set timer");
        
//         // Close timer - should be inline since we're on loop thread
//         timer.close().expect("close timer");
        
//         close_clone.store(true, Ordering::Relaxed);
//     }).expect("defer callback");
    
//     // Run the loop
//     let handle = thread::spawn(move || {
//         loop_clone.run();
//     });
    
//     handle.join().expect("loop thread join");
    
//     assert!(close_completed.load(Ordering::Relaxed), "Timer close should have completed");
// }

// #[test]
// fn test_timer_drop_optimization() {
//     let loop_ = Arc::new(LoopInner::new().expect("create loop"));
//     let drop_processed = Arc::new(AtomicBool::new(false));
    
//     let loop_clone = loop_.clone();
//     let drop_clone = drop_processed.clone();
    
//     // Create and drop timer from within loop thread
//     loop_.defer(move || {
//         // Create timer on loop thread
//         let timer = Timer::new(&loop_clone).expect("create timer");
        
//         // Set timer
//         timer.set(100, 0, move || {
//             // This may or may not fire depending on timing
//         }).expect("set timer");
        
//         // Drop timer by letting it go out of scope
//         // This should be handled inline since we're on the loop thread
//         drop(timer);
        
//         drop_clone.store(true, Ordering::Relaxed);
//     }).expect("defer callback");
    
//     // Run the loop
//     let handle = thread::spawn(move || {
//         loop_.run();
//     });
    
//     handle.join().expect("loop thread join");
    
//     assert!(drop_processed.load(Ordering::Relaxed), "Timer drop should have been processed");
// }

// #[test]
// fn test_timer_operations_from_different_threads() {
//     let loop_ = Arc::new(LoopInner::new().expect("create loop"));
//     let operations_completed = Arc::new(AtomicU32::new(0));
    
//     // Create timer from main thread
//     let timer = Arc::new(Timer::new(&loop_).expect("create timer"));
    
//     // Start loop in background
//     let loop_clone = loop_.clone();
//     let loop_handle = thread::spawn(move || {
//         loop_clone.run();
//     });
    
//     // Spawn multiple threads that operate on the timer
//     let mut handles = Vec::new();
    
//     for i in 0..3 {
//         let timer_clone = timer.clone();
//         let ops_clone = operations_completed.clone();
        
//         let handle = thread::spawn(move || {
//             // These operations should be deferred since we're not on loop thread
//             timer_clone.set(10 + i * 5, 0, move || {
//                 // Timer callback
//             }).expect("set timer from thread");
            
//             ops_clone.fetch_add(1, Ordering::Relaxed);
//         });
        
//         handles.push(handle);
//     }
    
//     // Wait for all threads
//     for handle in handles {
//         handle.join().expect("thread join");
//     }
    
//     // Give time for deferred operations to process
//     thread::sleep(Duration::from_millis(100));
    
//     loop_handle.join().expect("loop thread join");
    
//     assert_eq!(operations_completed.load(Ordering::Relaxed), 3, "All timer operations should complete");
// }