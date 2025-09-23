// //! Test the new Timer architecture with deferred operations

// use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
// use std::thread;
// use std::time::Duration;
// use bop_rs::usockets::{LoopInner, Timer};

// #[test]
// fn test_timer_creation_from_different_thread() {
//     // Create loop on main thread
//     let loop_ = LoopInner::new().expect("create loop");
//     let counter = Arc::new(AtomicU32::new(0));
    
//     // Clone for thread
//     let counter_clone = counter.clone();
    
//     // Create timer from a different thread
//     let timer_thread = thread::spawn(move || {
//         // This should work even though we're on a different thread
//         // because Timer::new will defer the actual creation
//         let timer = Timer::new(&loop_).expect("create timer");
        
//         // Set timer from this thread (also deferred)
//         let counter_for_timer = counter_clone.clone();
//         timer.set(10, 0, move || {
//             println!("Timer fired from thread-created timer!");
//             counter_for_timer.fetch_add(1, Ordering::Relaxed);
//         }).expect("set timer");
        
//         timer
//     });
    
//     // Get the timer from the thread
//     let timer = timer_thread.join().expect("timer thread");
    
//     // Give time for defer to process
//     thread::sleep(Duration::from_millis(50));
    
//     // Run the loop to process deferred operations and timer
//     loop_.run();
    
//     // Check that timer fired
//     assert_eq!(counter.load(Ordering::Relaxed), 1, "Timer should have fired once");
    
//     // Close timer (also deferred)
//     timer.close().expect("close timer");
// }

// #[test]
// fn test_timer_modification_from_multiple_threads() {
//     let loop_ = LoopInner::new().expect("create loop");
//     let counter = Arc::new(AtomicU32::new(0));
    
//     // Create timer
//     let timer = Arc::new(Timer::new(&loop_).expect("create timer"));
    
//     // Spawn multiple threads that modify the timer
//     let mut handles = Vec::new();
//     for i in 0..3 {
//         let timer_clone = timer.clone();
//         let counter_clone = counter.clone();
        
//         let handle = thread::spawn(move || {
//             // Each thread sets the timer with different values
//             let delay = 10 + i * 5;
//             timer_clone.set(delay, 0, move || {
//                 println!("Timer {} fired!", i);
//                 counter_clone.fetch_add(1, Ordering::Relaxed);
//             }).expect("set timer");
//         });
        
//         handles.push(handle);
//     }
    
//     // Wait for all threads
//     for handle in handles {
//         handle.join().expect("thread join");
//     }
    
//     // Give time for defer to process
//     thread::sleep(Duration::from_millis(50));
    
//     // Run loop
//     loop_.run();
    
//     // At least one timer callback should have fired
//     assert!(counter.load(Ordering::Relaxed) > 0, "At least one timer should have fired");
// }