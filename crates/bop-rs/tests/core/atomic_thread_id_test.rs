// //! Test atomic thread ID functionality

// use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
// use std::thread;
// use std::time::Duration;
// use bop_rs::usockets::LoopInner;

// #[test]
// fn test_atomic_thread_id_detection() {
//     let loop_ = Arc::new(LoopInner::new().expect("create loop"));
    
//     // Initially should not be on loop thread
//     assert!(!loop_.is_on_loop_thread(), "Should not be on loop thread initially");
    
//     let loop_clone = loop_.clone();
//     let on_loop_detected = Arc::new(AtomicBool::new(false));
//     let on_loop_clone = on_loop_detected.clone();
    
//     // Test from within a deferred callback
//     loop_.defer(move || {
//         if loop_clone.is_on_loop_thread() {
//             on_loop_clone.store(true, Ordering::Relaxed);
//         }
//     }).expect("defer callback");
    
//     // Start loop in background thread
//     let loop_clone2 = loop_.clone();
//     let handle = thread::spawn(move || {
//         // Should not be on loop thread before run()
//         assert!(!loop_clone2.is_on_loop_thread(), "Should not be on loop thread before run()");
        
//         // Run the loop - this will process our deferred callback
//         loop_clone2.run();
        
//         // Should not be on loop thread after run() exits
//         assert!(!loop_clone2.is_on_loop_thread(), "Should not be on loop thread after run()");
//     });
    
//     // Wait for loop to complete
//     handle.join().expect("loop thread join");
    
//     // Main thread still should not be on loop thread
//     assert!(!loop_.is_on_loop_thread(), "Main thread should not be loop thread");
    
//     // But the deferred callback should have detected it was on the loop thread
//     assert!(on_loop_detected.load(Ordering::Relaxed), "Deferred callback should have been on loop thread");
// }

// #[test]
// fn test_run_on_loop_with_atomic_thread_id() {
//     let loop_ = Arc::new(LoopInner::new().expect("create loop"));
//     let executed = Arc::new(AtomicBool::new(false));
//     let was_inline = Arc::new(AtomicBool::new(false));
    
//     let loop_clone = loop_.clone();
//     let executed_clone = executed.clone();
//     let was_inline_clone = was_inline.clone();
    
//     // Test run_on_loop from different thread (should defer)
//     let result = loop_.run_on_loop(move || {
//         executed_clone.store(true, Ordering::Relaxed);
        
//         // Test nested run_on_loop (should be inline since we're now on loop thread)
//         let nested_result = loop_clone.run_on_loop(move || {
//             was_inline_clone.store(true, Ordering::Relaxed);
//         }).expect("nested run_on_loop");
        
//         // Nested call should have been inline
//         assert!(nested_result, "Nested run_on_loop should be inline when on loop thread");
//     }).expect("run_on_loop");
    
//     // Initial call should have been deferred
//     assert!(!result, "run_on_loop should defer when not on loop thread");
    
//     // Start loop to process the deferred callback
//     let handle = thread::spawn(move || {
//         loop_.run();
//     });
    
//     handle.join().expect("loop thread join");
    
//     // Both callbacks should have executed
//     assert!(executed.load(Ordering::Relaxed), "Callback should have executed");
//     assert!(was_inline.load(Ordering::Relaxed), "Nested callback should have executed inline");
// }

// #[test]
// fn test_thread_id_hash_consistency() {
//     // Test that the same thread ID always hashes to the same value
//     let thread_id = thread::current().id();
    
//     use std::collections::hash_map::DefaultHasher;
//     use std::hash::{Hash, Hasher};
    
//     let hash1 = {
//         let mut hasher = DefaultHasher::new();
//         thread_id.hash(&mut hasher);
//         hasher.finish()
//     };
    
//     let hash2 = {
//         let mut hasher = DefaultHasher::new();
//         thread_id.hash(&mut hasher);
//         hasher.finish()
//     };
    
//     assert_eq!(hash1, hash2, "Thread ID should hash consistently");
//     assert_ne!(hash1, 0, "Thread ID hash should not be 0");
// }