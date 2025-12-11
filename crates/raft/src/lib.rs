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
use std::task::{Context, Poll};
use std::time::Duration;

use openraft::AsyncRuntime;
use openraft::OptionalSend;

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
        let handle = maniac::spawn(future);
        JoinHandleWrapper {
            inner: handle
                .expect("ManiacRuntime::spawn must be called from within a worker context"),
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
            Self(maniac::time::Instant::now_high_frequency())
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

    pub struct ManiacOneshotSender<T>(tokio::sync::oneshot::Sender<T>);

    pub struct ManiacOneshotReceiver<T>(tokio::sync::oneshot::Receiver<T>);

    impl oneshot::Oneshot for ManiacOneshot {
        type Sender<T: OptionalSend> = ManiacOneshotSender<T>;
        type Receiver<T: OptionalSend> = ManiacOneshotReceiver<T>;
        type ReceiverError = tokio::sync::oneshot::error::RecvError;

        #[inline]
        fn channel<T>() -> (Self::Sender<T>, Self::Receiver<T>)
        where
            T: OptionalSend,
        {
            let (tx, rx) = tokio::sync::oneshot::channel();
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
        type Output = Result<T, tokio::sync::oneshot::error::RecvError>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            Pin::new(&mut self.0).poll(cx)
        }
    }
}

mod mpsc_mod {
    use std::future::Future;
    use std::sync::Arc;

    use openraft::OptionalSend;
    use openraft::async_runtime::{
        Mpsc, MpscReceiver, MpscSender, MpscWeakSender, SendError, TryRecvError,
    };
    use tokio::sync::mpsc as tokio_mpsc;

    pub struct ManiacMpsc;

    /// Sender wrapper using tokio's mpsc (which is properly single-consumer)
    pub struct TokioMpscSender<T>(tokio_mpsc::Sender<T>);

    impl<T> Clone for TokioMpscSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    /// Receiver wrapper using tokio's mpsc
    /// We use Arc<Mutex> to allow `recv()` to take &mut self while the receiver is shared
    pub struct TokioMpscReceiver<T>(Arc<tokio::sync::Mutex<tokio_mpsc::Receiver<T>>>);

    // Weak sender using tokio's WeakSender
    pub struct TokioMpscWeakSender<T>(tokio_mpsc::WeakSender<T>);

    impl<T> Clone for TokioMpscWeakSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl Mpsc for ManiacMpsc {
        type Sender<T: OptionalSend> = TokioMpscSender<T>;
        type Receiver<T: OptionalSend> = TokioMpscReceiver<T>;
        type WeakSender<T: OptionalSend> = TokioMpscWeakSender<T>;

        #[inline]
        fn channel<T: OptionalSend>(buffer: usize) -> (Self::Sender<T>, Self::Receiver<T>) {
            let (tx, rx) = tokio_mpsc::channel::<T>(buffer);
            (
                TokioMpscSender(tx),
                TokioMpscReceiver(Arc::new(tokio::sync::Mutex::new(rx))),
            )
        }
    }

    impl<T> MpscSender<ManiacMpsc, T> for TokioMpscSender<T>
    where
        T: OptionalSend,
    {
        #[inline]
        fn send(&self, msg: T) -> impl Future<Output = Result<(), SendError<T>>> {
            let sender = self.0.clone();
            async move { sender.send(msg).await.map_err(|e| SendError(e.0)) }
        }

        #[inline]
        fn downgrade(&self) -> <ManiacMpsc as Mpsc>::WeakSender<T> {
            TokioMpscWeakSender(self.0.downgrade())
        }
    }

    impl<T: OptionalSend> MpscReceiver<T> for TokioMpscReceiver<T> {
        #[inline]
        fn recv(&mut self) -> impl Future<Output = Option<T>> + Send {
            let rx = self.0.clone();
            async move { rx.lock().await.recv().await }
        }

        #[inline]
        fn try_recv(&mut self) -> Result<T, TryRecvError> {
            // For try_recv, we need to try to lock without blocking
            match self.0.try_lock() {
                Ok(mut guard) => match guard.try_recv() {
                    Ok(v) => Ok(v),
                    Err(tokio_mpsc::error::TryRecvError::Empty) => Err(TryRecvError::Empty),
                    Err(tokio_mpsc::error::TryRecvError::Disconnected) => {
                        Err(TryRecvError::Disconnected)
                    }
                },
                Err(_) => Err(TryRecvError::Empty), // Treat lock contention as empty
            }
        }
    }

    impl<T> MpscWeakSender<ManiacMpsc, T> for TokioMpscWeakSender<T>
    where
        T: OptionalSend,
    {
        #[inline]
        fn upgrade(&self) -> Option<<ManiacMpsc as Mpsc>::Sender<T>> {
            self.0.upgrade().map(TokioMpscSender)
        }
    }
}

mod mpsc_unbounded_mod {
    use std::sync::Arc;

    use openraft::OptionalSend;
    use openraft::type_config::async_runtime::mpsc_unbounded;
    use tokio::sync::mpsc as tokio_mpsc;

    pub struct ManiacMpscUnbounded;

    /// Sender wrapper using tokio's unbounded mpsc
    pub struct TokioMpscUnboundedSender<T>(tokio_mpsc::UnboundedSender<T>);

    impl<T> Clone for TokioMpscUnboundedSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    /// Receiver wrapper using tokio's unbounded mpsc
    pub struct TokioMpscUnboundedReceiver<T>(
        Arc<tokio::sync::Mutex<tokio_mpsc::UnboundedReceiver<T>>>,
    );

    /// Weak sender using tokio's WeakUnboundedSender
    pub struct TokioMpscUnboundedWeakSender<T>(tokio_mpsc::WeakUnboundedSender<T>);

    impl<T> Clone for TokioMpscUnboundedWeakSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl mpsc_unbounded::MpscUnbounded for ManiacMpscUnbounded {
        type Sender<T: OptionalSend> = TokioMpscUnboundedSender<T>;
        type Receiver<T: OptionalSend> = TokioMpscUnboundedReceiver<T>;
        type WeakSender<T: OptionalSend> = TokioMpscUnboundedWeakSender<T>;

        #[inline]
        fn channel<T: OptionalSend>() -> (Self::Sender<T>, Self::Receiver<T>) {
            let (tx, rx) = tokio_mpsc::unbounded_channel::<T>();
            (
                TokioMpscUnboundedSender(tx),
                TokioMpscUnboundedReceiver(Arc::new(tokio::sync::Mutex::new(rx))),
            )
        }
    }

    impl<T> mpsc_unbounded::MpscUnboundedSender<ManiacMpscUnbounded, T> for TokioMpscUnboundedSender<T>
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
            TokioMpscUnboundedWeakSender(self.0.downgrade())
        }
    }

    impl<T: OptionalSend> mpsc_unbounded::MpscUnboundedReceiver<T> for TokioMpscUnboundedReceiver<T> {
        #[inline]
        fn recv(&mut self) -> impl std::future::Future<Output = Option<T>> + Send {
            let rx = self.0.clone();
            async move { rx.lock().await.recv().await }
        }

        #[inline]
        fn try_recv(&mut self) -> Result<T, mpsc_unbounded::TryRecvError> {
            match self.0.try_lock() {
                Ok(mut guard) => match guard.try_recv() {
                    Ok(v) => Ok(v),
                    Err(tokio_mpsc::error::TryRecvError::Empty) => {
                        Err(mpsc_unbounded::TryRecvError::Empty)
                    }
                    Err(tokio_mpsc::error::TryRecvError::Disconnected) => {
                        Err(mpsc_unbounded::TryRecvError::Disconnected)
                    }
                },
                Err(_) => Err(mpsc_unbounded::TryRecvError::Empty),
            }
        }
    }

    impl<T> mpsc_unbounded::MpscUnboundedWeakSender<ManiacMpscUnbounded, T>
        for TokioMpscUnboundedWeakSender<T>
    where
        T: OptionalSend,
    {
        #[inline]
        fn upgrade(
            &self,
        ) -> Option<<ManiacMpscUnbounded as mpsc_unbounded::MpscUnbounded>::Sender<T>> {
            self.0.upgrade().map(TokioMpscUnboundedSender)
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
