use crate::monoio::{FusionRuntime, LegacyDriver, RuntimeBuilder, FusionDriver};
use crate::monoio::time::driver::TimeDriver;
#[cfg(all(target_os = "linux", feature = "iouring"))]
use crate::monoio::IoUringDriver;
use std::io;
use std::time::Duration;

#[cfg(all(target_os = "linux", feature = "iouring"))]
type RuntimeType = FusionRuntime<TimeDriver<IoUringDriver>, TimeDriver<LegacyDriver>>;

#[cfg(not(all(target_os = "linux", feature = "iouring")))]
type RuntimeType = FusionRuntime<TimeDriver<LegacyDriver>>;

/// Event Loop - Monoio integration
///
/// This replaces the custom mio-based loop with monoio's runtime.
/// It adapts monoio to function as the reactor for maniac-runtime's workers.
pub struct EventLoop {
    // We use TimeDriver wrap to support timeouts
    pub(crate) runtime: RuntimeType,
}

impl EventLoop {
    pub fn new() -> io::Result<Self> {
        // Build monoio runtime
        let runtime = RuntimeBuilder::<FusionDriver>::new()
            .enable_all()
            .build()?;
            
        Ok(Self { runtime })
    }

    /// Poll the event loop once
    ///
    /// # Arguments
    /// * `timeout` - Optional timeout. If None, blocks indefinitely.
    pub fn poll_once(&mut self, timeout: Option<Duration>) -> io::Result<usize> {
        self.runtime.poll_once(timeout)?;
        Ok(1)
    }
    
    /// Wake the event loop from another thread
    pub fn waker(&self) -> io::Result<crate::monoio::driver::UnparkHandle> {
         Ok(self.runtime.unpark())
    }
    
    pub fn with_timer_wheel() -> io::Result<Self> {
        Self::new()
    }

    pub fn modify_socket(&self, _token: mio::Token, _interest: mio::Interest) -> io::Result<()> {
        Ok(())
    }

    pub fn close_socket(&self, _token: mio::Token) {
    }

    pub fn set_timeout(&self, _token: mio::Token, _delay: Duration) -> io::Result<()> {
        Ok(())
    }

    pub fn cancel_timeout(&self, _token: mio::Token) -> io::Result<()> {
        Ok(())
    }

    // Legacy methods to support existing tests/code temporarily
    
    pub fn iteration_number(&self) -> i64 {
        0
    }
    
    pub fn socket_count(&self) -> usize {
        0
    }
}

// Re-export UnparkHandle as it's used by waker()
pub use crate::monoio::driver::UnparkHandle;
