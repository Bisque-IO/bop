//! Maniac runtime implementation for OpenRaft.
//!
//! This crate provides the `ManiacRuntime` implementation of OpenRaft's `AsyncRuntime` trait,
//! allowing OpenRaft to use the Maniac async runtime instead of Tokio.
//!
//! # Features
//!
//! - `multi` (default): Enables the multi-raft module for running multiple Raft groups

#[cfg(feature = "multi")]
pub mod multi;
pub mod type_config;

pub use type_config::ManiacRaftTypeConfig;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use maniac::runtime::worker::Scheduler;
use openraft::AsyncRuntime;
use openraft::OptionalSend;

// Global scheduler for spawning tasks from outside worker context
static GLOBAL_SCHEDULER: AtomicPtr<Scheduler> = AtomicPtr::new(std::ptr::null_mut());

/// Set the global scheduler used for spawning tasks outside of worker context.
///
/// This must be called during runtime initialization before any Raft operations.
/// The scheduler reference must remain valid for the lifetime of the program.
///
/// # Safety
/// The caller must ensure the scheduler remains valid for the program's lifetime.
pub fn set_global_scheduler(scheduler: &Arc<Scheduler>) {
    let ptr = Arc::as_ptr(scheduler) as *mut Scheduler;
    GLOBAL_SCHEDULER.store(ptr, Ordering::Release);
}

/// Clear the global scheduler (call on shutdown).
pub fn clear_global_scheduler() {
    GLOBAL_SCHEDULER.store(std::ptr::null_mut(), Ordering::Release);
}

/// Get the global scheduler if set.
fn global_scheduler() -> Option<Arc<Scheduler>> {
    let ptr = GLOBAL_SCHEDULER.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        // SAFETY: We only store valid Arc pointers and increment refcount
        unsafe {
            Arc::increment_strong_count(ptr);
            Some(Arc::from_raw(ptr))
        }
    }
}

/// Wrapper for `maniac::runtime::worker::JoinHandle` to implement `Future` and `Unpin`.
pub struct JoinHandleWrapper<T> {
    inner: maniac::runtime::worker::JoinHandle<T>,
}

impl<T: Send + 'static> Future for JoinHandleWrapper<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx).map(Ok)
    }
}

impl<T> Unpin for JoinHandleWrapper<T> {}

/// [`AsyncRuntime`] implementation for Maniac.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ManiacRuntime;

impl AsyncRuntime for ManiacRuntime {
    type JoinError = JoinError;
    type JoinHandle<T: OptionalSend + 'static> = JoinHandleWrapper<T>;
    type Sleep = maniac::time::Sleep;
    type Instant = instant_mod::ManiacInstant;
    type TimeoutError = maniac::time::error::Elapsed;
    type Timeout<R, T: Future<Output = R> + OptionalSend> = maniac::time::Timeout<T>;
    type ThreadLocalRng = rand::rngs::ThreadRng;
    type Mpsc = mpsc_mod::ManiacMpsc;
    type MpscUnbounded = mpsc_unbounded_mod::ManiacMpscUnbounded;
    type Watch = watch_mod::ManiacWatch;
    type Oneshot = oneshot_mod::ManiacOneshot;
    type Mutex<T: OptionalSend + 'static> = mutex_mod::ManiacMutex<T>;

    #[inline]
    fn spawn<T>(future: T) -> Self::JoinHandle<T::Output>
    where
        T: Future + OptionalSend + 'static,
        T::Output: OptionalSend + 'static,
    {
        // Try to spawn using the current worker if we're in a worker context
        // Otherwise, fallback to a panicking implementation (caller should ensure context)
        if let Some(worker) = maniac::current_worker() {
            let handle = maniac::spawn(future);
            JoinHandleWrapper {
                inner: handle.expect("failed to spawn task"),
            }
        } else {
            // We're not in a worker context - this can happen during Raft initialization
            // Use the global scheduler if available
            if let Some(scheduler) = global_scheduler() {
                let handle = scheduler.spawn(
                    std::any::TypeId::of::<T>(),
                    std::panic::Location::caller(),
                    future,
                );
                JoinHandleWrapper {
                    inner: handle.expect("failed to spawn task"),
                }
            } else {
                panic!(
                    "ManiacRuntime::spawn called outside of worker context and no global scheduler set. \
                        Call set_global_scheduler() during runtime initialization."
                );
            }
        }
    }

    #[inline]
    fn sleep(duration: Duration) -> Self::Sleep {
        maniac::time::sleep(duration)
    }

    #[inline]
    fn sleep_until(deadline: Self::Instant) -> Self::Sleep {
        maniac::time::sleep_until(deadline.0)
    }

    #[inline]
    fn timeout<R, F: Future<Output = R> + OptionalSend>(
        duration: Duration,
        future: F,
    ) -> Self::Timeout<R, F> {
        maniac::time::timeout(duration, future)
    }

    #[inline]
    fn timeout_at<R, F: Future<Output = R> + OptionalSend>(
        deadline: Self::Instant,
        future: F,
    ) -> Self::Timeout<R, F> {
        maniac::time::timeout_at(deadline.0, future)
    }

    #[inline]
    fn is_panic(join_error: &Self::JoinError) -> bool {
        match *join_error {}
    }

    #[inline]
    fn thread_rng() -> Self::ThreadLocalRng {
        rand::rng()
    }
}

/// Join error type (infallible for maniac since tasks always succeed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {}

impl std::fmt::Display for JoinError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for JoinError {}

// Put the wrapper types in private modules
mod instant_mod {
    use std::ops::{Add, AddAssign, Sub, SubAssign};
    use std::time::Duration;

    use openraft::instant;

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
    pub struct ManiacInstant(pub(crate) maniac::time::Instant);

    impl Add<Duration> for ManiacInstant {
        type Output = Self;

        #[inline]
        fn add(self, rhs: Duration) -> Self::Output {
            Self(self.0 + rhs)
        }
    }

    impl AddAssign<Duration> for ManiacInstant {
        #[inline]
        fn add_assign(&mut self, rhs: Duration) {
            self.0 += rhs;
        }
    }

    impl Sub<Duration> for ManiacInstant {
        type Output = Self;

        #[inline]
        fn sub(self, rhs: Duration) -> Self::Output {
            Self(self.0 - rhs)
        }
    }

    impl Sub<Self> for ManiacInstant {
        type Output = Duration;

        #[inline]
        fn sub(self, rhs: Self) -> Self::Output {
            self.0 - rhs.0
        }
    }

    impl SubAssign<Duration> for ManiacInstant {
        #[inline]
        fn sub_assign(&mut self, rhs: Duration) {
            self.0 -= rhs;
        }
    }

    impl instant::Instant for ManiacInstant {
        #[inline]
        fn now() -> Self {
            Self(maniac::time::Instant::now())
        }

        #[inline]
        fn elapsed(&self) -> Duration {
            self.0.elapsed()
        }
    }
}

mod oneshot_mod {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use openraft::OptionalSend;
    use openraft::type_config::async_runtime::oneshot;

    pub struct ManiacOneshot;

    pub struct ManiacOneshotSender<T>(maniac::sync::oneshot::Sender<T>);

    pub struct ManiacOneshotReceiver<T>(maniac::sync::oneshot::Receiver<T>);

    impl oneshot::Oneshot for ManiacOneshot {
        type Sender<T: OptionalSend> = ManiacOneshotSender<T>;
        type Receiver<T: OptionalSend> = ManiacOneshotReceiver<T>;
        type ReceiverError = maniac::sync::oneshot::RecvError;

        #[inline]
        fn channel<T>() -> (Self::Sender<T>, Self::Receiver<T>)
        where
            T: OptionalSend,
        {
            let (tx, rx) = maniac::sync::oneshot::channel();
            (ManiacOneshotSender(tx), ManiacOneshotReceiver(rx))
        }
    }

    impl<T> oneshot::OneshotSender<T> for ManiacOneshotSender<T>
    where
        T: OptionalSend,
    {
        #[inline]
        fn send(self, t: T) -> Result<(), T> {
            self.0.send(t);
            Ok(())
        }
    }

    impl<T> Future for ManiacOneshotReceiver<T> {
        type Output = Result<T, maniac::sync::oneshot::RecvError>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            Pin::new(&mut self.0).poll(cx)
        }
    }
}

mod mpsc_mod {
    use std::future::Future;

    use openraft::OptionalSend;
    use openraft::async_runtime::{
        Mpsc, MpscReceiver, MpscSender, MpscWeakSender, SendError, TryRecvError,
    };

    pub struct ManiacMpsc;

    pub struct ManiacMpscSender<T>(flume::Sender<T>);

    impl<T> Clone for ManiacMpscSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    pub struct ManiacMpscReceiver<T>(flume::Receiver<T>);

    pub struct ManiacMpscWeakSender<T>(flume::WeakSender<T>);

    impl<T> Clone for ManiacMpscWeakSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl Mpsc for ManiacMpsc {
        type Sender<T: OptionalSend> = ManiacMpscSender<T>;
        type Receiver<T: OptionalSend> = ManiacMpscReceiver<T>;
        type WeakSender<T: OptionalSend> = ManiacMpscWeakSender<T>;

        #[inline]
        fn channel<T: OptionalSend>(buffer: usize) -> (Self::Sender<T>, Self::Receiver<T>) {
            let (tx, rx) = flume::bounded(buffer.max(1));
            (ManiacMpscSender(tx), ManiacMpscReceiver(rx))
        }
    }

    impl<T> MpscSender<ManiacMpsc, T> for ManiacMpscSender<T>
    where
        T: OptionalSend,
    {
        #[inline]
        fn send(&self, msg: T) -> impl Future<Output = Result<(), SendError<T>>> {
            let sender = self.0.clone();
            async move { sender.send_async(msg).await.map_err(|e| SendError(e.0)) }
        }

        #[inline]
        fn downgrade(&self) -> <ManiacMpsc as Mpsc>::WeakSender<T> {
            ManiacMpscWeakSender(self.0.downgrade())
        }
    }

    impl<T: OptionalSend> MpscReceiver<T> for ManiacMpscReceiver<T> {
        #[inline]
        fn recv(&mut self) -> impl Future<Output = Option<T>> + Send {
            let receiver = self.0.clone();
            async move { receiver.recv_async().await.ok() }
        }

        #[inline]
        fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv().map_err(|e| match e {
                flume::TryRecvError::Empty => TryRecvError::Empty,
                flume::TryRecvError::Disconnected => TryRecvError::Disconnected,
            })
        }
    }

    impl<T> MpscWeakSender<ManiacMpsc, T> for ManiacMpscWeakSender<T>
    where
        T: OptionalSend,
    {
        #[inline]
        fn upgrade(&self) -> Option<<ManiacMpsc as Mpsc>::Sender<T>> {
            self.0.upgrade().map(ManiacMpscSender)
        }
    }
}

mod mpsc_unbounded_mod {
    use openraft::OptionalSend;
    use openraft::type_config::async_runtime::mpsc_unbounded;

    pub struct ManiacMpscUnbounded;

    pub struct ManiacMpscUnboundedSender<T>(flume::Sender<T>);

    impl<T> Clone for ManiacMpscUnboundedSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    pub struct ManiacMpscUnboundedReceiver<T>(flume::Receiver<T>);

    pub struct ManiacMpscUnboundedWeakSender<T>(flume::WeakSender<T>);

    impl<T> Clone for ManiacMpscUnboundedWeakSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl mpsc_unbounded::MpscUnbounded for ManiacMpscUnbounded {
        type Sender<T: OptionalSend> = ManiacMpscUnboundedSender<T>;
        type Receiver<T: OptionalSend> = ManiacMpscUnboundedReceiver<T>;
        type WeakSender<T: OptionalSend> = ManiacMpscUnboundedWeakSender<T>;

        #[inline]
        fn channel<T: OptionalSend>() -> (Self::Sender<T>, Self::Receiver<T>) {
            let (tx, rx) = flume::unbounded();
            (
                ManiacMpscUnboundedSender(tx),
                ManiacMpscUnboundedReceiver(rx),
            )
        }
    }

    impl<T> mpsc_unbounded::MpscUnboundedSender<ManiacMpscUnbounded, T> for ManiacMpscUnboundedSender<T>
    where
        T: OptionalSend,
    {
        #[inline]
        fn send(&self, msg: T) -> Result<(), mpsc_unbounded::SendError<T>> {
            self.0.send(msg).map_err(|e| mpsc_unbounded::SendError(e.0))
        }

        #[inline]
        fn downgrade(
            &self,
        ) -> <ManiacMpscUnbounded as mpsc_unbounded::MpscUnbounded>::WeakSender<T> {
            ManiacMpscUnboundedWeakSender(self.0.downgrade())
        }
    }

    impl<T: OptionalSend> mpsc_unbounded::MpscUnboundedReceiver<T> for ManiacMpscUnboundedReceiver<T> {
        #[inline]
        fn recv(&mut self) -> impl std::future::Future<Output = Option<T>> + Send {
            let receiver = self.0.clone();
            async move { receiver.recv_async().await.ok() }
        }

        #[inline]
        fn try_recv(&mut self) -> Result<T, mpsc_unbounded::TryRecvError> {
            self.0.try_recv().map_err(|e| match e {
                flume::TryRecvError::Empty => mpsc_unbounded::TryRecvError::Empty,
                flume::TryRecvError::Disconnected => mpsc_unbounded::TryRecvError::Disconnected,
            })
        }
    }

    impl<T> mpsc_unbounded::MpscUnboundedWeakSender<ManiacMpscUnbounded, T>
        for ManiacMpscUnboundedWeakSender<T>
    where
        T: OptionalSend,
    {
        #[inline]
        fn upgrade(
            &self,
        ) -> Option<<ManiacMpscUnbounded as mpsc_unbounded::MpscUnbounded>::Sender<T>> {
            self.0.upgrade().map(ManiacMpscUnboundedSender)
        }
    }
}

mod watch_mod {
    use std::ops::Deref;

    use openraft::OptionalSend;
    use openraft::OptionalSync;
    use openraft::async_runtime::watch::{RecvError, SendError};
    use openraft::type_config::async_runtime::watch;

    pub struct ManiacWatch;

    pub struct ManiacWatchSender<T>(maniac::sync::watch::Sender<T>);

    pub struct ManiacWatchReceiver<T>(maniac::sync::watch::Receiver<T>);

    pub struct ManiacWatchRef<'a, T>(maniac::sync::watch::Ref<'a, T>);

    impl watch::Watch for ManiacWatch {
        type Sender<T: OptionalSend + OptionalSync> = ManiacWatchSender<T>;
        type Receiver<T: OptionalSend + OptionalSync> = ManiacWatchReceiver<T>;
        type Ref<'a, T: OptionalSend + 'a> = ManiacWatchRef<'a, T>;

        #[inline]
        fn channel<T: OptionalSend + OptionalSync>(
            init: T,
        ) -> (Self::Sender<T>, Self::Receiver<T>) {
            let (tx, rx) = maniac::sync::watch::channel(init);
            (ManiacWatchSender(tx), ManiacWatchReceiver(rx))
        }
    }

    impl<T> Clone for ManiacWatchSender<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> watch::WatchSender<ManiacWatch, T> for ManiacWatchSender<T>
    where
        T: OptionalSend + OptionalSync,
    {
        #[inline]
        fn send(&self, value: T) -> Result<(), SendError<T>> {
            self.0.send(value).map_err(|e| SendError(e.0))
        }

        #[inline]
        fn send_if_modified<F>(&self, modify: F) -> bool
        where
            F: FnOnce(&mut T) -> bool,
        {
            self.0.send_if_modified(modify)
        }

        #[inline]
        fn borrow_watched(&self) -> <ManiacWatch as watch::Watch>::Ref<'_, T> {
            ManiacWatchRef(self.0.borrow())
        }

        #[inline]
        fn subscribe(&self) -> <ManiacWatch as watch::Watch>::Receiver<T> {
            ManiacWatchReceiver(self.0.subscribe())
        }
    }

    impl<T> Clone for ManiacWatchReceiver<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> watch::WatchReceiver<ManiacWatch, T> for ManiacWatchReceiver<T>
    where
        T: OptionalSend + OptionalSync,
    {
        #[inline]
        async fn changed(&mut self) -> Result<(), RecvError> {
            self.0.changed().await.map_err(|_| RecvError(()))
        }

        #[inline]
        fn borrow_watched(&self) -> <ManiacWatch as watch::Watch>::Ref<'_, T> {
            ManiacWatchRef(self.0.borrow())
        }
    }

    impl<'a, T> Deref for ManiacWatchRef<'a, T> {
        type Target = T;

        #[inline]
        fn deref(&self) -> &Self::Target {
            &*self.0
        }
    }
}

mod mutex_mod {
    use std::future::Future;

    use openraft::OptionalSend;
    use openraft::type_config::async_runtime::mutex;

    pub struct ManiacMutex<T>(maniac::sync::mutex::Mutex<T>);

    impl<T> mutex::Mutex<T> for ManiacMutex<T>
    where
        T: OptionalSend + 'static,
    {
        type Guard<'a>
            = maniac::sync::mutex::MutexGuard<'a, T>
        where
            Self: 'a;

        #[inline]
        fn new(value: T) -> Self {
            ManiacMutex(maniac::sync::mutex::Mutex::new(value))
        }

        #[inline]
        fn lock(&self) -> impl Future<Output = Self::Guard<'_>> + OptionalSend {
            self.0.lock()
        }
    }
}
