use std::future::Future;
use std::{io, marker::PhantomData};

#[cfg(all(target_os = "linux", feature = "iouring"))]
use crate::driver::IoUringDriver;
#[cfg(feature = "poll")]
use crate::driver::PollerDriver;
#[cfg(any(feature = "poll", feature = "iouring"))]
use crate::utils::thread_id::gen_id;
use crate::{driver::Driver, scoped_thread_local};

// ============================================================================
// io_runtime module
// ============================================================================

thread_local! {
    pub(crate) static DEFAULT_CTX: Context = Context {
        thread_id: crate::utils::thread_id::DEFAULT_THREAD_ID,
        user_data: std::cell::Cell::new(std::ptr::null_mut()),
    };
}

#[cfg(any(feature = "poll", feature = "iouring"))]
thread_local! {
    pub(crate) static BUILD_THREAD_ID: usize = gen_id();
}

scoped_thread_local!(pub static CURRENT: Context);

pub struct Context {
    /// Thread id(not the kernel thread id but a generated unique number)
    pub thread_id: usize,

    /// User data (e.g. for storing Worker reference)
    pub user_data: std::cell::Cell<*mut ()>,
}

impl Context {
    pub(crate) fn new() -> Self {
        #[cfg(any(feature = "poll", feature = "iouring"))]
        let thread_id = BUILD_THREAD_ID.with(|id| *id);
        #[cfg(not(any(feature = "poll", feature = "iouring")))]
        let thread_id = crate::utils::thread_id::DEFAULT_THREAD_ID;

        Self {
            thread_id,
            user_data: std::cell::Cell::new(std::ptr::null_mut()),
        }
    }
}

/// Monoio runtime
pub struct IoRuntime<D> {
    pub context: Context,
    pub driver: D,
}

impl<D> IoRuntime<D> {
    pub fn new(context: Context, driver: D) -> Self {
        Self { context, driver }
    }

    /// Block on
    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
        D: Driver,
    {
        assert!(
            !CURRENT.is_set(),
            "Can not start a runtime inside a runtime"
        );

        let waker = std::task::Waker::from(std::sync::Arc::new(DummyWaker));
        let cx = &mut std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        self.driver.with(|| {
            CURRENT.set(&self.context, || {
                loop {
                    // check if ready
                    if let std::task::Poll::Ready(t) = future.as_mut().poll(cx) {
                        return t;
                    }

                    // Wait and Process CQ
                    if let Err(e) = self.driver.park() {
                        #[cfg(all(debug_assertions, feature = "debug"))]
                        tracing::trace!("park error: {:?}", e);
                    }
                }
            })
        })
    }

    /// Poll the runtime once, assuming context is already set up.
    ///
    /// # Safety
    /// This method assumes that the driver and CURRENT contexts are already set up
    /// via a prior call to `with_context()` or similar. Calling this without proper
    /// context setup will lead to undefined behavior.
    ///
    /// This is used internally by the worker integration where context is set once
    /// at the worker level, avoiding redundant setup on every poll.
    #[inline]
    pub unsafe fn poll_once_unchecked(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> std::io::Result<()>
    where
        D: Driver,
    {
        if let Some(t) = timeout {
            self.driver.park_timeout(t)?;
        } else {
            self.driver.park()?;
        }
        Ok(())
    }
}

struct DummyWaker;
impl std::task::Wake for DummyWaker {
    fn wake(self: std::sync::Arc<Self>) {}
}

/// Fusion Runtime is a wrapper of io_uring driver or poller driver based
/// runtime.
#[cfg(feature = "poll")]
pub enum FusionRuntime<#[cfg(all(target_os = "linux", feature = "iouring"))] L, R> {
    /// Uring driver based runtime.
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    Uring(IoRuntime<L>),
    /// Poller driver based runtime.
    Poller(IoRuntime<R>),
}

/// Fusion Runtime is a wrapper of io_uring driver or poller driver based
/// runtime.
#[cfg(all(target_os = "linux", feature = "iouring", not(feature = "poll")))]
pub enum FusionRuntime<L> {
    /// Uring driver based runtime.
    Uring(IoRuntime<L>),
}

#[cfg(all(target_os = "linux", feature = "iouring", feature = "poll"))]
impl<L, R> FusionRuntime<L, R>
where
    L: Driver,
    R: Driver,
    crate::driver::UnparkHandle: From<L::Unpark> + From<R::Unpark>,
{
    /// Block on
    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        match self {
            FusionRuntime::Uring(inner) => {
                #[cfg(feature = "debug")]
                tracing::info!("Monoio is running with io_uring driver");
                inner.block_on(future)
            }
            FusionRuntime::Poller(inner) => {
                #[cfg(feature = "debug")]
                tracing::info!("Monoio is running with poller driver");
                inner.block_on(future)
            }
        }
    }

    pub unsafe fn poll_once_unchecked(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => unsafe { inner.poll_once_unchecked(timeout) },
            FusionRuntime::Poller(inner) => unsafe { inner.poll_once_unchecked(timeout) },
        }
    }

    pub fn park_timeout(&mut self, duration: std::time::Duration) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.park_timeout(duration),
            FusionRuntime::Poller(inner) => inner.driver.park_timeout(duration),
        }
    }

    pub fn submit(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.submit(),
            FusionRuntime::Poller(inner) => inner.driver.submit(),
        }
    }

    pub fn with_context<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.with(|| CURRENT.set(&inner.context, f)),
            FusionRuntime::Poller(inner) => inner.driver.with(|| CURRENT.set(&inner.context, f)),
        }
    }

    pub fn park(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.park(),
            FusionRuntime::Poller(inner) => inner.driver.park(),
        }
    }

    pub fn unpark(&self) -> crate::driver::UnparkHandle {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.unpark().into(),
            FusionRuntime::Poller(inner) => inner.driver.unpark().into(),
        }
    }
}

#[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
impl<R> FusionRuntime<R>
where
    R: Driver,
    crate::driver::UnparkHandle: From<R::Unpark>,
{
    /// Block on
    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        match self {
            FusionRuntime::Poller(inner) => inner.block_on(future),
        }
    }

    pub unsafe fn poll_once_unchecked(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> std::io::Result<()> {
        match self {
            FusionRuntime::Poller(inner) => inner.poll_once_unchecked(timeout),
        }
    }

    pub fn park_timeout(&mut self, duration: std::time::Duration) -> std::io::Result<()> {
        match self {
            FusionRuntime::Poller(inner) => inner.driver.park_timeout(duration),
        }
    }

    pub fn submit(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Poller(inner) => inner.driver.submit(),
        }
    }

    pub fn with_context<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        match self {
            FusionRuntime::Poller(inner) => inner.driver.with(|| CURRENT.set(&inner.context, f)),
        }
    }

    pub fn park(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Poller(inner) => inner.driver.park(),
        }
    }

    pub fn unpark(&self) -> crate::driver::UnparkHandle {
        match self {
            FusionRuntime::Poller(inner) => inner.driver.unpark().into(),
        }
    }
}

#[cfg(all(not(feature = "poll"), all(target_os = "linux", feature = "iouring")))]
impl<R> FusionRuntime<R>
where
    R: Driver,
    crate::driver::UnparkHandle: From<R::Unpark>,
{
    /// Block on
    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        match self {
            FusionRuntime::Uring(inner) => inner.block_on(future),
        }
    }

    pub unsafe fn poll_once_unchecked(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.poll_once_unchecked(timeout),
        }
    }

    pub fn park_timeout(&mut self, duration: std::time::Duration) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.park_timeout(duration),
        }
    }

    pub fn submit(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.submit(),
        }
    }

    pub fn with_context<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.with(|| CURRENT.set(&inner.context, f)),
        }
    }

    pub fn park(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.park(),
        }
    }

    pub fn unpark(&self) -> crate::driver::UnparkHandle {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.unpark().into(),
        }
    }
}

// L -> Fusion<L, R>
#[cfg(all(target_os = "linux", feature = "iouring", feature = "poll"))]
impl From<IoRuntime<IoUringDriver>> for FusionRuntime<IoUringDriver, PollerDriver> {
    fn from(r: IoRuntime<IoUringDriver>) -> Self {
        Self::Uring(r)
    }
}

// R -> Fusion<L, R>
#[cfg(all(target_os = "linux", feature = "iouring", feature = "poll"))]
impl From<IoRuntime<PollerDriver>> for FusionRuntime<IoUringDriver, PollerDriver> {
    fn from(r: IoRuntime<PollerDriver>) -> Self {
        Self::Poller(r)
    }
}

// R -> Fusion<R>
#[cfg(all(feature = "poll", not(all(target_os = "linux", feature = "iouring"))))]
impl From<IoRuntime<PollerDriver>> for FusionRuntime<PollerDriver> {
    fn from(r: IoRuntime<PollerDriver>) -> Self {
        Self::Poller(r)
    }
}

// L -> Fusion<L>
#[cfg(all(target_os = "linux", feature = "iouring", not(feature = "poll")))]
impl From<IoRuntime<IoUringDriver>> for FusionRuntime<IoUringDriver> {
    fn from(r: IoRuntime<IoUringDriver>) -> Self {
        Self::Uring(r)
    }
}

// ============================================================================
// io_builder module
// ============================================================================

#[cfg(all(target_os = "linux", feature = "iouring"))]
pub type FusionDriver = IoUringDriver;
#[cfg(all(not(all(target_os = "linux", feature = "iouring")), feature = "poll"))]
pub type FusionDriver = PollerDriver;

pub trait Buildable {
    type Output;
    fn build(self) -> io::Result<Self::Output>;
}

pub struct RuntimeBuilder<D> {
    _mark: PhantomData<D>,
}

impl<D> RuntimeBuilder<D> {
    pub fn new() -> Self {
        Self { _mark: PhantomData }
    }
}

pub trait DriverNew: Sized {
    fn new() -> io::Result<Self>;
}

#[cfg(feature = "poll")]
impl DriverNew for PollerDriver {
    fn new() -> io::Result<Self> {
        PollerDriver::new()
    }
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
impl DriverNew for IoUringDriver {
    fn new() -> io::Result<Self> {
        let builder = io_uring::IoUring::builder();
        IoUringDriver::new(&builder)
    }
}

impl<D> Buildable for RuntimeBuilder<D>
where
    D: Driver + DriverNew,
{
    type Output = IoRuntime<D>;

    fn build(self) -> io::Result<Self::Output> {
        let driver = D::new()?;
        let context = Context::new();

        Ok(IoRuntime::new(context, driver))
    }
}

// ============================================================================
// io_driver module
// ============================================================================

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
    pub fn poll_once(&mut self, timeout: Option<std::time::Duration>) -> io::Result<usize> {
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
    pub unsafe fn poll_once_unchecked(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> io::Result<usize> {
        unsafe {
            self.runtime.poll_once_unchecked(timeout)?;
        }
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
