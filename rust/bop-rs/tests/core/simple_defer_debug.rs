//! Simple debug test for defer functionality

use bop_rs::usockets::{Loop, LoopCallback, LoopInner, LoopMut, Timer};
use bop_sys as sys;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use std::thread;
use std::time::Duration;

unsafe extern "C" fn wakeup_cb(loop_: *mut sys::us_loop_t) {
    println!("Wakeup callback");
}

unsafe extern "C" fn pre_cb(loop_: *mut sys::us_loop_t) {
    println!("prec callback");
}

unsafe extern "C" fn post_cb(loop_: *mut sys::us_loop_t) {
    println!("post callback");
}

unsafe extern "C" fn timer_cb(t: *mut sys::us_timer_t) {
    println!("Timer fired");
}

pub struct LoopData {}

#[test]
fn test_basic_loop() {
    unsafe {
        let l = sys::us_create_loop(
            std::ptr::null_mut(),
            Some(wakeup_cb),
            Some(pre_cb),
            Some(post_cb),
            32,
        );
        let timer = sys::us_create_timer(l, 0, 32);
        sys::us_timer_set(timer, Some(timer_cb), 1, 0);
        sys::us_loop_run(l);
        sys::us_timer_close(timer);
    }
}

#[test]
fn test_loop() {
    unsafe {
        let loop_ = Loop::<()>::new().expect("create loop");
        println!("Loop created successfully");
        std::thread::spawn(move || {});
        loop_.run().expect("run loop");
    }
}

#[test]
fn test_basic_defer_debug() {
    let loop_ = Loop::<()>::new().expect("create loop");
    let handle = loop_.handle();
    let counter = Arc::new(AtomicU32::new(0));
    println!("Loop created successfully");

    let counter_clone = counter.clone();

    // Create a timer to keep the loop alive
    // let timer = Timer::<()>::new(&loop_).expect("create timer");
    // let _ = timer.set(10, 0, move || {
    //     println!("Timer fired");
    // });

    println!("About to run loop");

    std::thread::spawn(move || {
        // let timer = loop_.create_timer().expect("create timer");
        // let _ = timer.set(10, 0, move || {
        //     println!("Timer fired");
        // });
        loop_.run().expect("run loop");
    });

    // Give the loop a moment to process the deferred callback
    std::thread::sleep(Duration::from_millis(100));

    let callback = LoopCallback::<()>::Box(Box::new(move |loop_mut: LoopMut<()>| {
        println!("Callback executed!");
        counter_clone.fetch_add(1, Ordering::Relaxed);
    }));
    let result = handle.defer(callback);
    match result {
        Ok(_) => println!("defer() succeeded"),
        Err(e) => println!("defer() failed: {}", e),
    }

    std::thread::sleep(Duration::from_millis(100));

    println!("Loop run completed");

    println!("Counter value: {}", counter.load(Ordering::Relaxed));
    assert_eq!(counter.load(Ordering::Relaxed), 1);
}
