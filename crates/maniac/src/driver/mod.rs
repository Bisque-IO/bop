/// Monoio Driver.
#[allow(dead_code)]
pub(crate) mod op;
#[cfg(feature = "poll-io")]
pub(crate) mod poll;
#[cfg(any(feature = "poll", feature = "poll-io"))]
pub(crate) mod ready;
#[cfg(any(feature = "poll", feature = "poll-io"))]
pub(crate) mod scheduled_io;
#[allow(dead_code)]
pub(crate) mod shared_fd;

#[cfg(feature = "poll")]
pub(crate) mod poller;
#[cfg(all(target_os = "linux", feature = "iouring"))]
mod uring;

mod util;

use std::{
    io,
    task::{Context, Poll},
    time::Duration,
};

use crate::scoped_thread_local;

use self::op::{CompletionMeta, Op, OpAble};
#[allow(unreachable_pub)]
#[cfg(feature = "poll")]
pub use self::poller::PollerDriver;
#[cfg(feature = "poll")]
use self::poller::PollerInner;
#[cfg(all(target_os = "linux", feature = "iouring"))]
pub use self::uring::IoUringDriver;
#[cfg(all(target_os = "linux", feature = "iouring"))]
use self::uring::UringInner;

/// Unpark a runtime of another thread.
pub(crate) mod unpark {
    #[allow(unreachable_pub, dead_code)]
    pub trait Unpark: Sync + Send + 'static {
        /// Unblocks a thread that is blocked by the associated `Park` handle.
        ///
        /// Calling `unpark` atomically makes available the unpark token, if it
        /// is not already available.
        ///
        /// # Panics
        ///
        /// This function **should** not panic, but ultimately, panics are left
        /// as an implementation detail. Refer to the documentation for
        /// the specific `Unpark` implementation
        fn unpark(&self) -> std::io::Result<()>;
    }
}

impl unpark::Unpark for Box<dyn unpark::Unpark> {
    fn unpark(&self) -> io::Result<()> {
        (**self).unpark()
    }
}

impl unpark::Unpark for std::sync::Arc<dyn unpark::Unpark> {
    fn unpark(&self) -> io::Result<()> {
        (**self).unpark()
    }
}

/// Core driver trait.
pub trait Driver {
    /// Run with driver TLS.
    fn with<R>(&self, f: impl FnOnce() -> R) -> R;
    /// Submit ops to kernel and process returned events.
    fn submit(&self) -> io::Result<()>;
    /// Wait infinitely and process returned events.
    fn park(&self) -> io::Result<()>;
    /// Wait with timeout and process returned events.
    fn park_timeout(&self, duration: Duration) -> io::Result<()>;

    /// The struct to wake thread from another.
    // #[cfg(feature = "sync")]
    type Unpark: unpark::Unpark;

    /// Get Unpark.
    // #[cfg(feature = "sync")]
    fn unpark(&self) -> Self::Unpark;
}

scoped_thread_local!(pub(crate) static CURRENT: Inner);

#[derive(Clone)]
pub(crate) enum Inner {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    Uring(std::sync::Arc<std::cell::UnsafeCell<UringInner>>),
    #[cfg(feature = "poll")]
    Poller(std::sync::Arc<std::cell::UnsafeCell<PollerInner>>),
}

// SAFETY: Inner is Send + Sync because:
// 1. The UringInner/PollerInner are accessed through Arc<UnsafeCell<...>>
// 2. The slab inside UringInner is protected by a mutex for thread-safe access
// 3. All operations are properly synchronized
unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl Inner {
    fn submit_with<T: OpAble>(&self, data: T) -> io::Result<Op<T>> {
        match self {
            #[cfg(all(target_os = "linux", feature = "iouring"))]
            Inner::Uring(this) => UringInner::submit_with_data(this, data),
            #[cfg(feature = "poll")]
            Inner::Poller(this) => PollerInner::submit_with_data(this, data),
            #[cfg(all(
                not(feature = "poll"),
                not(all(target_os = "linux", feature = "iouring"))
            ))]
            _ => {
                util::feature_panic();
            }
        }
    }

    #[allow(unused)]
    fn poll_op<T: OpAble>(
        &self,
        data: &mut T,
        index: usize,
        cx: &mut Context<'_>,
    ) -> Poll<CompletionMeta> {
        match self {
            #[cfg(all(target_os = "linux", feature = "iouring"))]
            Inner::Uring(this) => UringInner::poll_op(this, index, cx),
            #[cfg(feature = "poll")]
            Inner::Poller(this) => PollerInner::poll_op::<T>(this, data, cx),
            #[cfg(all(
                not(feature = "poll"),
                not(all(target_os = "linux", feature = "iouring"))
            ))]
            _ => {
                util::feature_panic();
            }
        }
    }

    #[cfg(feature = "poll-io")]
    fn poll_legacy_op<T: OpAble>(
        &self,
        data: &mut T,
        cx: &mut Context<'_>,
    ) -> Poll<CompletionMeta> {
        match self {
            #[cfg(all(target_os = "linux", feature = "iouring"))]
            Inner::Uring(this) => UringInner::poll_legacy_op(this, data, cx),
            #[cfg(feature = "poll")]
            Inner::Poller(this) => PollerInner::poll_op::<T>(this, data, cx),
            #[cfg(all(
                not(feature = "poll"),
                not(all(target_os = "linux", feature = "iouring"))
            ))]
            _ => {
                util::feature_panic();
            }
        }
    }

    #[cfg(all(target_os = "linux", feature = "iouring"))]
    #[inline]
    fn drop_op<T: 'static>(&self, index: usize, data: &mut Option<T>, skip_cancel: bool) {
        match self {
            Inner::Uring(this) => UringInner::drop_op(this, index, data, skip_cancel),
            #[cfg(feature = "poll")]
            Inner::Poller(_) => {}
        }
    }

    #[allow(unused)]
    pub(super) unsafe fn cancel_op(&self, op_canceller: &op::OpCanceller) {
        match self {
            #[cfg(all(target_os = "linux", feature = "iouring"))]
            Inner::Uring(this) => UringInner::cancel_op(this, op_canceller.index),
            #[cfg(feature = "poll")]
            Inner::Poller(this) => {
                if let Some(direction) = op_canceller.direction {
                    PollerInner::cancel_op(this, op_canceller.index, direction)
                }
            }
            #[cfg(all(
                not(feature = "poll"),
                not(all(target_os = "linux", feature = "iouring"))
            ))]
            _ => {
                util::feature_panic();
            }
        }
    }

    #[cfg(all(target_os = "linux", feature = "iouring", feature = "poll"))]
    fn is_legacy(&self) -> bool {
        matches!(self, Inner::Poller(..))
    }

    #[cfg(all(target_os = "linux", feature = "iouring", not(feature = "poll")))]
    fn is_legacy(&self) -> bool {
        false
    }

    #[allow(unused)]
    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    fn is_legacy(&self) -> bool {
        true
    }
}

/// The unified UnparkHandle.
// #[cfg(feature = "sync")]
#[derive(Clone)]
pub enum UnparkHandle {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    Uring(self::uring::UnparkHandle),
    #[cfg(feature = "poll")]
    Poller(self::poller::UnparkHandle),
}

// #[cfg(feature = "sync")]
impl unpark::Unpark for UnparkHandle {
    fn unpark(&self) -> io::Result<()> {
        match self {
            #[cfg(all(target_os = "linux", feature = "iouring"))]
            UnparkHandle::Uring(inner) => inner.unpark(),
            #[cfg(feature = "poll")]
            UnparkHandle::Poller(inner) => inner.unpark(),
            #[cfg(all(
                not(feature = "poll"),
                not(all(target_os = "linux", feature = "iouring"))
            ))]
            _ => {
                util::feature_panic();
            }
        }
    }
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
impl From<self::uring::UnparkHandle> for UnparkHandle {
    fn from(inner: self::uring::UnparkHandle) -> Self {
        Self::Uring(inner)
    }
}

#[cfg(feature = "poll")]
impl From<self::poller::UnparkHandle> for UnparkHandle {
    fn from(inner: self::poller::UnparkHandle) -> Self {
        Self::Poller(inner)
    }
}

impl UnparkHandle {
    #[allow(unused)]
    pub(crate) fn current() -> Self {
        CURRENT.with(|inner| match inner {
            #[cfg(all(target_os = "linux", feature = "iouring"))]
            Inner::Uring(this) => UringInner::unpark(this).into(),
            #[cfg(feature = "poll")]
            Inner::Poller(this) => PollerInner::unpark(this).into(),
        })
    }

    /// Try to get the current UnparkHandle without panicking.
    ///
    /// Returns `Ok(handle)` if running within a runtime context,
    /// or `Err(())` if not in a runtime context.
    #[allow(unused)]
    pub(crate) fn try_current() -> Result<Self, ()> {
        CURRENT
            .try_with(|inner| {
                inner.map(|inner| match inner {
                    #[cfg(all(target_os = "linux", feature = "iouring"))]
                    Inner::Uring(this) => UringInner::unpark(this).into(),
                    #[cfg(feature = "poll")]
                    Inner::Poller(this) => PollerInner::unpark(this).into(),
                })
            })
            .ok_or(())
    }
}
