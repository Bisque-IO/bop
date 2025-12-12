//! Test support utilities for multi-raft tests
//!
//! Provides helpers to run async tests inside Maniac runtime and common fixtures.

use futures::FutureExt;
use maniac::runtime::{DefaultExecutor, Executor};
use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
use maniac::future::block_on;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::panic::AssertUnwindSafe;
use tempfile::TempDir;

/// Run an async future inside a Maniac runtime
/// 
/// This creates a Maniac executor and runs the future within it.
/// This is required because `ManiacRaftTypeConfig` uses `ManiacRuntime`,
/// and `C::AsyncRuntime::spawn` (which calls `maniac::spawn`) needs to be called 
/// from within a Maniac worker task context.
/// 
/// The approach: spawn the test future on the executor, then block on the join handle.
/// The executor workers run in background threads and will process the task.
/// The JoinHandle's poll will drive executor progress.
pub fn run_async<F>(f: F) -> F::Output
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Create a single-threaded executor for tests
    // This automatically starts a worker thread that will process tasks
    let executor = DefaultExecutor::new_single_threaded();
    
    // Spawn the test future on the executor.
    // This puts it in the executor's task queue for the worker to process
    //
    // IMPORTANT: wrap the future with `catch_unwind` so a panic inside the async test
    // fails the test instead of hanging forever (a panic would otherwise abort the
    // task without notifying the join handle).
    let handle = executor
        .spawn(async move { AssertUnwindSafe(f).catch_unwind().await })
        .expect("Failed to spawn test task in Maniac runtime");
    
    // Block on the handle to completion using futures_lite block_on
    // This will poll the JoinHandle, which internally drives the executor
    // to make progress on the spawned task
    match block_on(handle) {
        Ok(v) => v,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// Port allocator for tests to avoid conflicts
pub struct PortAllocator {
    next: AtomicU16,
}

impl PortAllocator {
    pub fn new(start: u16) -> Self {
        Self {
            next: AtomicU16::new(start),
        }
    }

    pub fn next(&self) -> u16 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for PortAllocator {
    fn default() -> Self {
        Self::new(50000)
    }
}

/// Temporary directory helper for storage tests
pub struct TestTempDir {
    inner: TempDir,
}

impl TestTempDir {
    pub fn new() -> Self {
        Self {
            inner: tempfile::tempdir().expect("Failed to create temp directory"),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.inner.path().to_path_buf()
    }
}

impl Default for TestTempDir {
    fn default() -> Self {
        Self::new()
    }
}
