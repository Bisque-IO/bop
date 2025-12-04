//! Maniac runtime implementation for openraft.

#![cfg(feature = "raft")]

pub mod multi;
pub mod type_config;

pub use type_config::ManiacRaftTypeConfig;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use maniac_raft::AsyncRuntime;
use maniac_raft::OptionalSend;

/// Wrapper for `maniac::runtime::JoinHandle` to implement `Future` and `Unpin`.
pub struct JoinHandleWrapper<T> {
    inner: crate::runtime::worker::JoinHandle<T>,
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
    type Sleep = crate::time::Sleep;
    type Instant = instant_mod::ManiacInstant;
    type TimeoutError = crate::time::error::Elapsed;
    type Timeout<R, T: Future<Output = R> + OptionalSend> = crate::time::Timeout<T>;
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
        // Spawn using the default executor
        // Note: This requires a runtime to be running
        // In practice, this should be called from within a runtime context
        let handle = crate::runtime::worker::spawn(future);
        JoinHandleWrapper {
            inner: handle.expect("failed to spawn task"),
        }
    }

    #[inline]
    fn sleep(duration: Duration) -> Self::Sleep {
        crate::time::sleep(duration)
    }

    #[inline]
    fn sleep_until(deadline: Self::Instant) -> Self::Sleep {
        crate::time::sleep_until(deadline.0)
    }

    #[inline]
    fn timeout<R, F: Future<Output = R> + OptionalSend>(
        duration: Duration,
        future: F,
    ) -> Self::Timeout<R, F> {
        crate::time::timeout(duration, future)
    }

    #[inline]
    fn timeout_at<R, F: Future<Output = R> + OptionalSend>(
        deadline: Self::Instant,
        future: F,
    ) -> Self::Timeout<R, F> {
        crate::time::timeout_at(deadline.0, future)
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

    use maniac_raft::instant;

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
    pub struct ManiacInstant(pub(crate) crate::time::Instant);

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
            Self(crate::time::Instant::now())
        }

        #[inline]
        fn elapsed(&self) -> Duration {
            self.0.elapsed()
        }
    }
}

mod oneshot_mod {
    use crate::sync::oneshot::{Receiver, RecvError, Sender, channel};
    use maniac_raft::OptionalSend;
    use maniac_raft::type_config::async_runtime::oneshot;

    pub struct ManiacOneshot;

    pub struct ManiacOneshotSender<T>(Sender<T>);

    impl oneshot::Oneshot for ManiacOneshot {
        type Sender<T: OptionalSend> = ManiacOneshotSender<T>;
        type Receiver<T: OptionalSend> = Receiver<T>;
        type ReceiverError = RecvError;

        #[inline]
        fn channel<T>() -> (Self::Sender<T>, Self::Receiver<T>)
        where
            T: OptionalSend,
        {
            let (tx, rx) = channel();
            (ManiacOneshotSender(tx), rx)
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
}

mod mpsc_mod {
    use std::future::Future;

    use crate::sync::mpsc::bounded::{
        MpscReceiver as ManiacReceiver, MpscSender as ManiacSender, PopError,
        SendError as ManiacSendError, TryRecvError as ManiacTryRecvError,
        WeakMpscSender as ManiacWeakSender, channel,
    };
    use maniac_raft::OptionalSend;
    use maniac_raft::async_runtime::{
        Mpsc, MpscReceiver, MpscSender, MpscWeakSender, SendError, TryRecvError,
    };

    pub struct ManiacMpsc;

    pub struct ManiacMpscSender<T>(ManiacSender<T>);

    impl<T> Clone for ManiacMpscSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    pub struct ManiacMpscReceiver<T>(ManiacReceiver<T>);

    pub struct ManiacMpscWeakSender<T>(ManiacWeakSender<T>);

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
            let (tx, rx) = channel();
            (ManiacMpscSender(tx), ManiacMpscReceiver(rx))
        }
    }

    impl<T> MpscSender<ManiacMpsc, T> for ManiacMpscSender<T>
    where
        T: OptionalSend,
    {
        #[inline]
        fn send(&self, msg: T) -> impl Future<Output = Result<(), SendError<T>>> {
            let mut sender = self.0.clone();
            async move {
                sender
                    .send(msg)
                    .await
                    .map_err(|e: ManiacSendError<T>| match e {
                        ManiacSendError::Full(t) => SendError(t),
                        ManiacSendError::Closed(t) => SendError(t),
                    })
            }
        }

        #[inline]
        fn downgrade(&self) -> <ManiacMpsc as Mpsc>::WeakSender<T> {
            ManiacMpscWeakSender(self.0.downgrade())
        }
    }

    impl<T: OptionalSend> MpscReceiver<T> for ManiacMpscReceiver<T> {
        #[inline]
        fn recv(&mut self) -> impl Future<Output = Option<T>> {
            async move { self.0.recv().await.ok() }
        }

        #[inline]
        fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv().map_err(|e| match e {
                PopError::Empty => TryRecvError::Empty,
                PopError::Closed => TryRecvError::Disconnected,
                _ => TryRecvError::Empty,
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
    use crate::sync::mpsc::dynamic::{
        DynSendError as ManiacSendError, DynTryRecvError as ManiacTryRecvError,
        DynUnboundedReceiver as ManiacReceiver, DynUnboundedSender as ManiacSender,
        WeakDynUnboundedSender as ManiacWeakSender, dyn_unbounded_channel_default,
    };
    use maniac_raft::OptionalSend;
    use maniac_raft::type_config::async_runtime::mpsc_unbounded;

    pub struct ManiacMpscUnbounded;

    pub struct ManiacMpscUnboundedSender<T>(ManiacSender<T>);

    impl<T> Clone for ManiacMpscUnboundedSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    pub struct ManiacMpscUnboundedReceiver<T>(ManiacReceiver<T>);

    pub struct ManiacMpscUnboundedWeakSender<T>(ManiacWeakSender<T>);

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
            let (tx, rx) = dyn_unbounded_channel_default();
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

    impl<T> mpsc_unbounded::MpscUnboundedReceiver<T> for ManiacMpscUnboundedReceiver<T> {
        #[inline]
        async fn recv(&mut self) -> Option<T> {
            self.0.recv().await
        }

        #[inline]
        fn try_recv(&mut self) -> Result<T, mpsc_unbounded::TryRecvError> {
            self.0.try_recv().map_err(|e| match e {
                ManiacTryRecvError::Empty => mpsc_unbounded::TryRecvError::Empty,
                ManiacTryRecvError::Disconnected => mpsc_unbounded::TryRecvError::Disconnected,
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

    use crate::sync::watch::{
        Receiver as ManiacReceiver, Ref as ManiacRef, Sender as ManiacSender, channel,
    };
    use maniac_raft::OptionalSend;
    use maniac_raft::OptionalSync;
    use maniac_raft::async_runtime::watch::{RecvError, SendError};
    use maniac_raft::type_config::async_runtime::watch;

    pub struct ManiacWatch;
    pub struct ManiacWatchSender<T>(ManiacSender<T>);
    pub struct ManiacWatchReceiver<T>(ManiacReceiver<T>);
    pub struct ManiacWatchRef<'a, T>(ManiacRef<'a, T>);

    impl watch::Watch for ManiacWatch {
        type Sender<T: OptionalSend + OptionalSync> = ManiacWatchSender<T>;
        type Receiver<T: OptionalSend + OptionalSync> = ManiacWatchReceiver<T>;
        type Ref<'a, T: OptionalSend + 'a> = ManiacWatchRef<'a, T>;

        #[inline]
        fn channel<T: OptionalSend + OptionalSync>(
            init: T,
        ) -> (Self::Sender<T>, Self::Receiver<T>) {
            let (tx, rx) = channel(init);
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
            self.0.deref()
        }
    }
}

mod mutex_mod {
    use std::future::Future;

    use crate::sync::mutex::{Mutex as ManiacMutexImpl, MutexGuard as ManiacMutexGuard};
    use maniac_raft::OptionalSend;
    use maniac_raft::type_config::async_runtime::mutex;

    pub struct ManiacMutex<T>(ManiacMutexImpl<T>);

    impl<T> mutex::Mutex<T> for ManiacMutex<T>
    where
        T: OptionalSend + 'static,
    {
        type Guard<'a>
            = ManiacMutexGuard<'a, T>
        where
            Self: 'a;

        #[inline]
        fn new(value: T) -> Self {
            ManiacMutex(ManiacMutexImpl::new(value))
        }

        #[inline]
        fn lock(&self) -> impl Future<Output = Self::Guard<'_>> + OptionalSend {
            self.0.lock()
        }
    }
}
