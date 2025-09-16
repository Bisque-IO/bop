// //! Test the run_on_loop functionality

// use std::sync::{Arc, atomic::{AtomicU32, AtomicBool, Ordering}};
// use std::thread;
// use std::time::Duration;
// use bop_rs::usockets::LoopInner;

// #[test]
// fn test_run_on_loop_from_loop_thread() {
//     let loop_ = Arc::new(LoopInner::new().expect("create loop"));
//     let executed = Arc::new(AtomicBool::new(false));
//     let was_inline = Arc::new(AtomicBool::new(false));
    
//     let loop_clone = loop_.clone();
//     let executed_clone = executed.clone();
//     let was_inline_clone = was_inline.clone();
    
//     // Run from within the loop thread
//     thread::spawn(move || {
//         loop_clone.run();
//     });
    
//     // Give the loop time to start
//     thread::sleep(Duration::from_millis(50));
    
//     // This should be deferred since we're not on the loop thread
//     let result = loop_.run_on_loop(move || {
//         executed_clone.store(true, Ordering::Relaxed);
        
//         // Now we're on the loop thread, so another run_on_loop should be inline
//         let inner_result = loop_.run_on_loop(move || {
//             was_inline_clone.store(true, Ordering::Relaxed);
//         });
        
//         // This should have executed inline
//         assert_eq!(inner_result, Ok(true), "Should execute inline when on loop thread");
//     }).expect("run_on_loop");
    
//     // This should have been deferred
//     assert_eq!(result, false, "Should defer when not on loop thread");
    
//     // Give time for deferred callback to execute
//     thread::sleep(Duration::from_millis(100));
    
//     assert!(executed.load(Ordering::Relaxed), "Callback should have executed");
//     assert!(was_inline.load(Ordering::Relaxed), "Inner callback should have executed inline");
// }

// #[test]
// fn test_run_on_loop_optimization() {
//     let loop_ = LoopInner::new().expect("create loop");
//     let counter = Arc::new(AtomicU32::new(0));
    
//     // Track execution method
//     let inline_count = Arc::new(AtomicU32::new(0));
//     let deferred_count = Arc::new(AtomicU32::new(0));
    
//     // From different thread - should defer
//     let counter_clone = counter.clone();
//     let inline_clone = inline_count.clone();
//     let deferred_clone = deferred_count.clone();
    
//     let handle = thread::spawn(move || {
//         let result = loop_.run_on_loop(move || {
//             counter_clone.fetch_add(1, Ordering::Relaxed);
//         }).expect("run_on_loop");
        
//         if result {
//             inline_clone.fetch_add(1, Ordering::Relaxed);
//         } else {
//             deferred_clone.fetch_add(1, Ordering::Relaxed);
//         }
//     });
    
//     handle.join().expect("thread join");
    
//     // Should have deferred since called from different thread
//     assert_eq!(inline_count.load(Ordering::Relaxed), 0, "Should not execute inline from different thread");
//     assert_eq!(deferred_count.load(Ordering::Relaxed), 1, "Should defer from different thread");
// }

// #[test]
// fn test_is_on_loop_thread() {
//     let loop_ = Arc::new(LoopInner::new().expect("create loop"));
    
//     // Initially not on loop thread
//     assert!(!loop_.is_on_loop_thread(), "Should not be on loop thread initially");
    
//     let loop_clone = loop_.clone();
//     let on_loop_thread = Arc::new(AtomicBool::new(false));
//     let on_loop_clone = on_loop_thread.clone();
    
//     // Start loop in background
//     let handle = thread::spawn(move || {
//         // Still not on loop thread until run() is called
//         assert!(!loop_clone.is_on_loop_thread(), "Should not be on loop thread before run()");
        
//         // Defer a callback to check from within the loop
//         loop_clone.defer(move || {
//             // Now we should be on the loop thread
//             if loop_clone.is_on_loop_thread() {
//                 on_loop_clone.store(true, Ordering::Relaxed);
//             }
//         }).expect("defer");
        
//         loop_clone.run();
//     });
    
//     // Give time for loop to start and process
//     thread::sleep(Duration::from_millis(100));
    
//     // Still not on loop thread in main thread
//     assert!(!loop_.is_on_loop_thread(), "Main thread should not be loop thread");
    
//     // But the callback should have detected it was on the loop thread
//     assert!(on_loop_thread.load(Ordering::Relaxed), "Callback should have been on loop thread");
// }