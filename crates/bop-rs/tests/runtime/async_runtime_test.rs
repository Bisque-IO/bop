//! Minimal async runtime implementation for testing purposes

use std::future::Future;
use std::task::{Context, Poll, Waker};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::cell::RefCell;

/// A minimal executor that can run futures to completion
pub struct MinimalRuntime {
    ready: Arc<AtomicBool>,
}

impl MinimalRuntime {
    pub fn new() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Block on a future to completion
    pub fn block_on<F>(&mut self, future: F) -> F::Output 
    where
        F: Future,
    {
        let mut future = Box::pin(future);
        let waker = self.create_waker();
        let mut context = Context::from_waker(&waker);
        
        loop {
            self.ready.store(false, Ordering::Relaxed);
            
            match future.as_mut().poll(&mut context) {
                Poll::Ready(result) => return result,
                Poll::Pending => {
                    // Simple polling: yield and try again
                    while !self.ready.load(Ordering::Relaxed) {
                        std::thread::yield_now();
                    }
                }
            }
        }
    }
    
    fn create_waker(&self) -> Waker {
        let ready = Arc::clone(&self.ready);
        
        // Use a simple RefCell to construct the waker
        let waker_data = Arc::new(WakerData { ready });
        unsafe {
            Waker::from_raw(create_raw_waker(waker_data))
        }
    }
}

impl Default for MinimalRuntime {
    fn default() -> Self {
        Self::new()
    }
}

struct WakerData {
    ready: Arc<AtomicBool>,
}

unsafe fn create_raw_waker(waker_data: Arc<WakerData>) -> std::task::RawWaker {
    // Create a minimal vtable
    const VTABLE: std::task::RawWakerVTable = std::task::RawWakerVTable::new(
        // Clone
        |data| {
            let arc = unsafe { Arc::from_raw(data as *const WakerData) };
            std::mem::forget(arc.clone());
            std::task::RawWaker::new(data, &VTABLE)
        },
        // Wake
        |data| {
            let arc = unsafe { Arc::from_raw(data as *const WakerData) };
            arc.ready.store(true, Ordering::Relaxed);
            std::mem::forget(arc);
        },
        // Wake by ref
        |data| {
            let arc = unsafe { Arc::from_raw(data as *const WakerData) };
            arc.ready.store(true, Ordering::Relaxed);
            std::mem::forget(arc);
        },
        // Drop
        |data| {
            drop(unsafe { Arc::from_raw(data as *const WakerData) });
        },
    );
    
    std::task::RawWaker::new(Arc::into_raw(waker_data) as *const (), &VTABLE)
}

/// A minimal async function that does nothing but yield control once
async fn minimal_async() {
    // This is the simplest possible async function
    // It just does nothing and completes immediately
}

/// An async function that yields control explicitly
async fn yielding_async() {
    // Create a future that will be pending on first poll and ready on second
    let mut yielded = false;
    
    let future = std::future::poll_fn(move |cx: &mut Context<'_>| {
        if !yielded {
            yielded = true;
            // First poll: return Pending to yield control
            Poll::Pending
        } else {
            // Second poll: return Ready to complete
            Poll::Ready(())
        }
    });
    
    future.await;
}

/// An async function that returns a value
async fn async_value() -> i32 {
    42
}

#[test]
fn test_minimal_async_runtime() {
    let mut runtime = MinimalRuntime::new();
    
    // Test 1: Run a minimal async function
    runtime.block_on(minimal_async());
    
    // Test 2: Run a yielding async function
    runtime.block_on(yielding_async());
    
    // Test 3: Run an async function that returns a value
    let result = runtime.block_on(async_value());
    assert_eq!(result, 42);
}

#[test]
fn test_multiple_await_points() {
    let mut runtime = MinimalRuntime::new();
    
    let result = runtime.block_on(async {
        let mut sum = 0;
        
        // First await point
        let future1 = std::future::ready(10);
        sum += future1.await;
        
        // Second await point
        let future2 = std::future::ready(20);
        sum += future2.await;
        
        // Third await point
        let future3 = std::future::ready(12);
        sum += future3.await;
        
        sum
    });
    
    assert_eq!(result, 42);
}

#[test]
fn test_async_block() {
    let mut runtime = MinimalRuntime::new();
    
    let result = runtime.block_on(async {
        // Test that we can create and await futures within async blocks
        let x = async { 10 }.await;
        let y = async { 20 }.await;
        let z = async { 12 }.await;
        x + y + z
    });
    
    assert_eq!(result, 42);
}

#[test]
fn test_waker_functionality() {
    let mut runtime = MinimalRuntime::new();
    
    let result = runtime.block_on(async {
        // Create a future that uses waker functionality
        let future = std::future::poll_fn(|cx| {
            // Wake ourselves to be polled again
            cx.waker().wake_by_ref();
            Poll::Ready(42)
        });
        
        future.await
    });
    
    assert_eq!(result, 42);
}

#[test]
fn test_concurrent_execution_simulation() {
    let mut runtime = MinimalRuntime::new();
    
    let result = runtime.block_on(async {
        let mut results = Vec::new();
        
        // Simulate multiple async operations
        for i in 0..5 {
            let future = std::future::poll_fn(move |cx| {
                // Each future will wake up and provide its value
                cx.waker().wake_by_ref();
                Poll::Ready(i * 10)
            });
            
            results.push(future.await);
        }
        
        results.iter().sum::<i32>()
    });
    
    assert_eq!(result, 100); // 0 + 10 + 20 + 30 + 40
}