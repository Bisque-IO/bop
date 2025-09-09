//! Simple debug test for defer functionality

use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use bop_rs::usockets::{Loop, Timer};

#[test]
fn test_basic_defer_debug() {
    let loop_ = Loop::new().expect("create loop");
    let counter = Arc::new(AtomicU32::new(0));
    println!("Loop created successfully");
    
    let counter_clone = counter.clone();
    let result = loop_.defer(move || {
        println!("Callback executed!");
        counter_clone.fetch_add(1, Ordering::Relaxed);
    });
    
    match result {
        Ok(_) => println!("defer() succeeded"),
        Err(e) => println!("defer() failed: {}", e),
    }
    
    // Create a timer to keep the loop alive
    let mut timer = Timer::new(&loop_).expect("create timer");
    timer.set(10, 0, move || {
        println!("Timer fired");
    });
    
    loop_.wakeup();
    println!("About to run loop");
    loop_.run();
    println!("Loop run completed");
    
    println!("Counter value: {}", counter.load(Ordering::Relaxed));
    assert_eq!(counter.load(Ordering::Relaxed), 1);
}