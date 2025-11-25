#[cfg(all(target_os = "linux", feature = "iouring"))]
use crate::driver::IoUringDriver;
use crate::driver::PollerDriver;
use crate::runtime::{
    io_builder::{Buildable, FusionDriver, RuntimeBuilder},
    io_runtime::FusionRuntime,
};
use std::io;
use std::time::Duration;

#[cfg(all(target_os = "linux", feature = "iouring"))]
type RuntimeType = FusionRuntime<IoUringDriver, PollerDriver>;

#[cfg(not(all(target_os = "linux", feature = "iouring")))]
type RuntimeType = FusionRuntime<PollerDriver>;

/// Event Loop - Monoio integration
///
/// This replaces the custom mio-based loop with monoio's runtime.
/// It adapts monoio to function as the reactor for maniac-runtime's workers.
pub struct IoDriver {
    // We use TimeDriver wrap to support timeouts
    pub(crate) runtime: RuntimeType,
}

impl IoDriver {
    pub fn new() -> io::Result<Self> {
        // Build monoio runtime
        let runtime = RuntimeBuilder::<FusionDriver>::new().build()?;

        #[cfg(all(target_os = "linux", feature = "iouring"))]
        let runtime = FusionRuntime::Uring(runtime);

        #[cfg(not(all(target_os = "linux", feature = "iouring")))]
        let runtime = FusionRuntime::Poller(runtime);

        Ok(Self { runtime })
    }

    /// Poll the event loop once
    ///
    /// # Arguments
    /// * `timeout` - Optional timeout. If None, blocks indefinitely.
    pub fn poll_once(&mut self, timeout: Option<Duration>) -> io::Result<usize> {
        // Since we removed poll_once from Runtime due to borrow issues,
        // we use park/park_timeout which doesn't set the context.
        // This assumes the context is managed elsewhere or not needed for this poll.
        if let Some(t) = timeout {
            self.runtime.park_timeout(t)?;
        } else {
            self.runtime.park()?;
        }
        Ok(1)
    }

    /// Poll the event loop once, assuming context is already set.
    ///
    /// # Safety
    /// This assumes the runtime context is already set up via `with_context()`.
    /// Only use this when calling from within a `with_context()` block.
    #[inline]
    pub unsafe fn poll_once_unchecked(&mut self, timeout: Option<Duration>) -> io::Result<usize> {
        self.runtime.poll_once_unchecked(timeout)?;
        Ok(1)
    }

    /// Execute a closure within the context of the inner runtime
    pub fn with_context<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        self.runtime.with_context(f)
    }

    /// Wake the event loop from another thread
    pub fn waker(&self) -> io::Result<crate::driver::UnparkHandle> {
        Ok(self.runtime.unpark())
    }
}

// Re-export UnparkHandle as it's used by waker()
pub use crate::driver::UnparkHandle;
