use std::future::Future;

#[cfg(any(all(target_os = "linux", feature = "iouring"), feature = "legacy"))]
use crate::monoio::time::TimeDriver;
#[cfg(all(target_os = "linux", feature = "iouring"))]
use crate::monoio::IoUringDriver;
#[cfg(feature = "legacy")]
use crate::monoio::LegacyDriver;
use crate::monoio::{
    driver::Driver,
    time::driver::Handle as TimeHandle,
};

// #[cfg(feature = "sync")]
thread_local! {
    pub(crate) static DEFAULT_CTX: Context = Context {
        thread_id: crate::monoio::utils::thread_id::DEFAULT_THREAD_ID,
        unpark_cache: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
        // waker_sender_cache: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
        time_handle: None,
        // blocking_handle: crate::monoio::blocking::BlockingHandle::Empty(crate::monoio::blocking::BlockingStrategy::Panic),
    };
}

scoped_thread_local!(pub static CURRENT: Context);

pub struct Context {
    /// Thread id(not the kernel thread id but a generated unique number)
    pub thread_id: usize,

    /// Thread unpark handles
    // #[cfg(feature = "sync")]
    pub unpark_cache:
        std::cell::RefCell<rustc_hash::FxHashMap<usize, crate::monoio::driver::UnparkHandle>>,

    /// Waker sender cache
    // #[cfg(feature = "sync")]
    // pub waker_sender_cache:
    //     std::cell::RefCell<rustc_hash::FxHashMap<usize, flume::Sender<std::task::Waker>>>,

    /// Time Handle
    pub time_handle: Option<TimeHandle>,

    // Blocking Handle
    // #[cfg(feature = "sync")]
    // pub blocking_handle: crate::monoio::blocking::BlockingHandle,
}

impl Context {
    // #[cfg(feature = "sync")]
    // pub(crate) fn new(blocking_handle: crate::monoio::blocking::BlockingHandle) -> Self {
    //     let thread_id = crate::monoio::builder::BUILD_THREAD_ID.with(|id| *id);
    //
    //     Self {
    //         thread_id,
    //         unpark_cache: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
    //         waker_sender_cache: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
    //         time_handle: None,
    //         blocking_handle,
    //     }
    // }

    // #[cfg(not(feature = "sync"))]
    pub(crate) fn new() -> Self {
        let thread_id = crate::monoio::builder::BUILD_THREAD_ID.with(|id| *id);

        Self {
            thread_id,
            unpark_cache: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
            time_handle: None,
        }
    }

    /*
    #[allow(unused)]
    // #[cfg(feature = "sync")]
    pub(crate) fn unpark_thread(&self, id: usize) {
        use crate::monoio::driver::{thread::get_unpark_handle, unpark::Unpark};
        if let Some(handle) = self.unpark_cache.borrow().get(&id) {
            handle.unpark();
            return;
        }

        if let Some(v) = get_unpark_handle(id) {
            // Write back to local cache
            let w = v.clone();
            self.unpark_cache.borrow_mut().insert(id, w);
            v.unpark();
        }
    }
    */

    /*
    #[allow(unused)]
    #[cfg(feature = "sync")]
    pub(crate) fn send_waker(&self, id: usize, w: std::task::Waker) {
        use crate::monoio::driver::thread::get_waker_sender;
        if let Some(sender) = self.waker_sender_cache.borrow().get(&id) {
            let _ = sender.send(w);
            return;
        }

        if let Some(s) = get_waker_sender(id) {
            // Write back to local cache
            let _ = s.send(w);
            self.waker_sender_cache.borrow_mut().insert(id, s);
        }
    }
    */
}

/// Monoio runtime
pub struct Runtime<D> {
    pub context: Context,
    pub driver: D,
}

impl<D> Runtime<D> {
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

    /// Poll the runtime once
    pub fn poll_once(&mut self, timeout: Option<std::time::Duration>) -> std::io::Result<()>
    where
        D: Driver,
    {
        self.driver.with(|| {
            CURRENT.set(&self.context, || {
                if let Some(t) = timeout {
                    self.driver.park_timeout(t)?;
                } else {
                    self.driver.park()?;
                }
                Ok(())
            })
        })
    }
}

struct DummyWaker;
impl std::task::Wake for DummyWaker {
    fn wake(self: std::sync::Arc<Self>) {}
}


/// Fusion Runtime is a wrapper of io_uring driver or legacy driver based
/// runtime.
#[cfg(feature = "legacy")]
pub enum FusionRuntime<#[cfg(all(target_os = "linux", feature = "iouring"))] L, R> {
    /// Uring driver based runtime.
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    Uring(Runtime<L>),
    /// Legacy driver based runtime.
    Legacy(Runtime<R>),
}

/// Fusion Runtime is a wrapper of io_uring driver or legacy driver based
/// runtime.
#[cfg(all(target_os = "linux", feature = "iouring", not(feature = "legacy")))]
pub enum FusionRuntime<L> {
    /// Uring driver based runtime.
    Uring(Runtime<L>),
}

#[cfg(all(target_os = "linux", feature = "iouring", feature = "legacy"))]
impl<L, R> FusionRuntime<L, R>
where
    L: Driver,
    R: Driver,
    crate::monoio::driver::UnparkHandle: From<L::Unpark> + From<R::Unpark>,
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
            FusionRuntime::Legacy(inner) => {
                #[cfg(feature = "debug")]
                tracing::info!("Monoio is running with legacy driver");
                inner.block_on(future)
            }
        }
    }

    pub fn poll_once(&mut self, timeout: Option<std::time::Duration>) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.poll_once(timeout),
            FusionRuntime::Legacy(inner) => inner.poll_once(timeout),
        }
    }

    pub fn park_timeout(&mut self, duration: std::time::Duration) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.park_timeout(duration),
            FusionRuntime::Legacy(inner) => inner.driver.park_timeout(duration),
        }
    }

    pub fn submit(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.submit(),
            FusionRuntime::Legacy(inner) => inner.driver.submit(),
        }
    }

    pub fn with_context<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.with(|| CURRENT.set(&inner.context, f)),
            FusionRuntime::Legacy(inner) => inner.driver.with(|| CURRENT.set(&inner.context, f)),
        }
    }

    pub fn park(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.park(),
            FusionRuntime::Legacy(inner) => inner.driver.park(),
        }
    }

    pub fn unpark(&self) -> crate::monoio::driver::UnparkHandle {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.unpark().into(),
            FusionRuntime::Legacy(inner) => inner.driver.unpark().into(),
        }
    }
}

#[cfg(all(feature = "legacy", not(all(target_os = "linux", feature = "iouring"))))]
impl<R> FusionRuntime<R>
where
    R: Driver,
    crate::monoio::driver::UnparkHandle: From<R::Unpark>,
{
    /// Block on
    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        match self {
            FusionRuntime::Legacy(inner) => inner.block_on(future),
        }
    }

    pub fn poll_once(&mut self, timeout: Option<std::time::Duration>) -> std::io::Result<()> {
        match self {
            FusionRuntime::Legacy(inner) => inner.poll_once(timeout),
        }
    }

    pub fn park_timeout(&mut self, duration: std::time::Duration) -> std::io::Result<()> {
        match self {
            FusionRuntime::Legacy(inner) => inner.driver.park_timeout(duration),
        }
    }

    pub fn submit(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Legacy(inner) => inner.driver.submit(),
        }
    }

    pub fn with_context<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        match self {
            FusionRuntime::Legacy(inner) => inner.driver.with(|| CURRENT.set(&inner.context, f)),
        }
    }

    pub fn park(&mut self) -> std::io::Result<()> {
        match self {
            FusionRuntime::Legacy(inner) => inner.driver.park(),
        }
    }

    pub fn unpark(&self) -> crate::monoio::driver::UnparkHandle {
        match self {
            FusionRuntime::Legacy(inner) => inner.driver.unpark().into(),
        }
    }
}

#[cfg(all(not(feature = "legacy"), all(target_os = "linux", feature = "iouring")))]
impl<R> FusionRuntime<R>
where
    R: Driver,
    crate::monoio::driver::UnparkHandle: From<R::Unpark>,
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

    pub fn poll_once(&mut self, timeout: Option<std::time::Duration>) -> std::io::Result<()> {
        match self {
            FusionRuntime::Uring(inner) => inner.poll_once(timeout),
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

    pub fn unpark(&self) -> crate::monoio::driver::UnparkHandle {
        match self {
            FusionRuntime::Uring(inner) => inner.driver.unpark().into(),
        }
    }
}

// L -> Fusion<L, R>
#[cfg(all(target_os = "linux", feature = "iouring", feature = "legacy"))]
impl From<Runtime<IoUringDriver>> for FusionRuntime<IoUringDriver, LegacyDriver> {
    fn from(r: Runtime<IoUringDriver>) -> Self {
        Self::Uring(r)
    }
}

// TL -> Fusion<TL, TR>
#[cfg(all(target_os = "linux", feature = "iouring", feature = "legacy"))]
impl From<Runtime<TimeDriver<IoUringDriver>>>
    for FusionRuntime<TimeDriver<IoUringDriver>, TimeDriver<LegacyDriver>>
{
    fn from(r: Runtime<TimeDriver<IoUringDriver>>) -> Self {
        Self::Uring(r)
    }
}

// R -> Fusion<L, R>
#[cfg(all(target_os = "linux", feature = "iouring", feature = "legacy"))]
impl From<Runtime<LegacyDriver>> for FusionRuntime<IoUringDriver, LegacyDriver> {
    fn from(r: Runtime<LegacyDriver>) -> Self {
        Self::Legacy(r)
    }
}

// TR -> Fusion<TL, TR>
#[cfg(all(target_os = "linux", feature = "iouring", feature = "legacy"))]
impl From<Runtime<TimeDriver<LegacyDriver>>>
    for FusionRuntime<TimeDriver<IoUringDriver>, TimeDriver<LegacyDriver>>
{
    fn from(r: Runtime<TimeDriver<LegacyDriver>>) -> Self {
        Self::Legacy(r)
    }
}

// R -> Fusion<R>
#[cfg(all(feature = "legacy", not(all(target_os = "linux", feature = "iouring"))))]
impl From<Runtime<LegacyDriver>> for FusionRuntime<LegacyDriver> {
    fn from(r: Runtime<LegacyDriver>) -> Self {
        Self::Legacy(r)
    }
}

// TR -> Fusion<TR>
#[cfg(all(feature = "legacy", not(all(target_os = "linux", feature = "iouring"))))]
impl From<Runtime<TimeDriver<LegacyDriver>>> for FusionRuntime<TimeDriver<LegacyDriver>> {
    fn from(r: Runtime<TimeDriver<LegacyDriver>>) -> Self {
        Self::Legacy(r)
    }
}

// L -> Fusion<L>
#[cfg(all(target_os = "linux", feature = "iouring", not(feature = "legacy")))]
impl From<Runtime<IoUringDriver>> for FusionRuntime<IoUringDriver> {
    fn from(r: Runtime<IoUringDriver>) -> Self {
        Self::Uring(r)
    }
}

// TL -> Fusion<TL>
#[cfg(all(target_os = "linux", feature = "iouring", not(feature = "legacy")))]
impl From<Runtime<TimeDriver<IoUringDriver>>> for FusionRuntime<TimeDriver<IoUringDriver>> {
    fn from(r: Runtime<TimeDriver<IoUringDriver>>) -> Self {
        Self::Uring(r)
    }
}
