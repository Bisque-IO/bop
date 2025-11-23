//! Monoio Poller Driver.

use std::{
    cell::UnsafeCell,
    io,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use super::{
    CURRENT, Driver, Inner,
    op::{CompletionMeta, Op, OpAble},
    ready::{self, Ready},
    scheduled_io::ScheduledIo,
};
use crate::monoio::utils::slab::Slab;

mod waker;
pub(crate) use waker::UnparkHandle;

const TOKEN_WAKEUP: mio::Token = mio::Token(1 << 31);

pub(crate) struct PollerInner {
    // Maps token -> Arc reference to ScheduledIo
    // This allows epoll events to find the correct ScheduledIo to wake
    // Shared ownership between slab and SharedFd
    pub(crate) io_dispatch: Slab<std::sync::Arc<ScheduledIo>>,
    events: mio::Events,
    poll: mio::Poll,

    shared_waker: std::sync::Arc<waker::EventWaker>,
}

/// Driver with Poll-like syscall.
#[allow(unreachable_pub)]
pub struct PollerDriver {
    pub(crate) inner: Arc<UnsafeCell<PollerInner>>,
}

#[allow(dead_code)]
impl PollerDriver {
    const DEFAULT_ENTRIES: u32 = 1024;

    pub(crate) fn new() -> io::Result<Self> {
        Self::new_with_entries(Self::DEFAULT_ENTRIES)
    }

    pub(crate) fn new_with_entries(entries: u32) -> io::Result<Self> {
        let poll = mio::Poll::new()?;

        let shared_waker = std::sync::Arc::new(waker::EventWaker::new(mio::Waker::new(
            poll.registry(),
            TOKEN_WAKEUP,
        )?));

        let inner = PollerInner {
            io_dispatch: Slab::new(),
            events: mio::Events::with_capacity(entries as usize),
            poll,
            shared_waker,
        };
        let driver = Self {
            inner: Arc::new(UnsafeCell::new(inner)),
        };

        Ok(driver)
    }

    fn inner_park(&self, timeout: Option<Duration>) -> io::Result<()> {
        let inner = unsafe { &mut *self.inner.get() };

        // here we borrow 2 mut self, but its safe.
        let events = unsafe { &mut (*self.inner.get()).events };
        match inner.poll.poll(events, timeout) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
        let iter = events.iter();
        for event in iter {
            let token = event.token();

            inner.dispatch(token, Ready::from_mio(event));
        }
        Ok(())
    }

    pub(crate) fn register(
        this: &Arc<UnsafeCell<PollerInner>>,
        source: &mut impl mio::event::Source,
        interest: mio::Interest,
        scheduled_io: &std::sync::Arc<ScheduledIo>,
    ) -> io::Result<usize> {
        let inner = unsafe { &mut *this.get() };
        let token = inner.io_dispatch.insert(scheduled_io.clone());

        let registry = inner.poll.registry();
        match registry.register(source, mio::Token(token), interest) {
            Ok(_) => Ok(token),
            Err(e) => {
                inner.io_dispatch.remove(token);
                Err(e)
            }
        }
    }

    pub(crate) fn deregister(
        this: &Arc<UnsafeCell<PollerInner>>,
        token: usize,
        source: &mut impl mio::event::Source,
    ) -> io::Result<()> {
        let inner = unsafe { &mut *this.get() };

        // try to deregister fd first, on success we will remove it from slab.
        match inner.poll.registry().deregister(source) {
            Ok(_) => {
                inner.io_dispatch.remove(token);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

impl PollerInner {
    fn dispatch(&mut self, token: mio::Token, ready: Ready) {
        let sio_ref = match self.io_dispatch.get(token.0) {
            Some(sio_ref) => sio_ref,
            None => {
                return;
            }
        };
        // sio_ref is Ref<'_, Arc<ScheduledIo>>, dereference once to get &Arc<ScheduledIo>
        let sio: &std::sync::Arc<ScheduledIo> = &*sio_ref;
        // Now we have &Arc<ScheduledIo>, we need mutable access
        // SAFETY: We're the only one who can mutate ScheduledIo (we own the epoll/kqueue thread)
        let sio_mut = unsafe { &mut *(std::sync::Arc::as_ptr(sio) as *mut ScheduledIo) };
        sio_mut.set_readiness(|curr| curr | ready);
        sio_mut.wake(ready);
    }

    pub(crate) fn poll_op<T: OpAble>(
        this: &Arc<UnsafeCell<Self>>,
        data: &mut T,
        cx: &mut Context<'_>,
    ) -> Poll<CompletionMeta> {
        let inner = unsafe { &mut *this.get() };
        let (direction, index) = match data.legacy_interest() {
            Some(x) => x,
            None => {
                // if there is no index provided, it means the action does not rely on fd
                // readiness. do syscall right now.
                return Poll::Ready(CompletionMeta {
                    result: OpAble::legacy_call(data),
                    flags: 0,
                });
            }
        };

        // wait io ready and do syscall
        let sio_ref = inner.io_dispatch.get(index).expect("scheduled_io lost");
        let sio: &std::sync::Arc<ScheduledIo> = &*sio_ref; // Deref Ref<Arc<ScheduledIo>> to &Arc<ScheduledIo>
        // SAFETY: We're polling from the owning thread
        let sio_mut = unsafe { &mut *(std::sync::Arc::as_ptr(sio) as *mut ScheduledIo) };

        let readiness = ready!(sio_mut.poll_readiness(cx, direction));

        // check if canceled
        if readiness.is_canceled() {
            // clear CANCELED part only
            sio_mut.clear_readiness(readiness & Ready::CANCELED);
            return Poll::Ready(CompletionMeta {
                result: Err(io::Error::from_raw_os_error(125)),
                flags: 0,
            });
        }

        match OpAble::legacy_call(data) {
            Ok(n) => Poll::Ready(CompletionMeta {
                result: Ok(n),
                flags: 0,
            }),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                sio_mut.clear_readiness(direction.mask());
                sio_mut.set_waker(cx, direction);
                Poll::Pending
            }
            Err(e) => Poll::Ready(CompletionMeta {
                result: Err(e),
                flags: 0,
            }),
        }
    }

    pub(crate) fn cancel_op(
        this: &Arc<UnsafeCell<PollerInner>>,
        index: usize,
        direction: ready::Direction,
    ) {
        let inner = unsafe { &mut *this.get() };
        let ready = match direction {
            ready::Direction::Read => Ready::READ_CANCELED,
            ready::Direction::Write => Ready::WRITE_CANCELED,
        };
        inner.dispatch(mio::Token(index), ready);
    }

    pub(crate) fn submit_with_data<T>(
        this: &Arc<UnsafeCell<PollerInner>>,
        data: T,
    ) -> io::Result<Op<T>>
    where
        T: OpAble,
    {
        Ok(Op {
            driver: Inner::Poller(this.clone()),
            // useless for poller
            index: 0,
            data: Some(data),
        })
    }


    pub(crate) fn unpark(this: &Arc<UnsafeCell<PollerInner>>) -> waker::UnparkHandle {
        let inner = unsafe { &*this.get() };
        let weak = std::sync::Arc::downgrade(&inner.shared_waker);
        waker::UnparkHandle(weak)
    }
}

impl Driver for PollerDriver {
    fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        let inner = Inner::Poller(self.inner.clone());
        CURRENT.set(&inner, f)
    }

    fn submit(&self) -> io::Result<()> {
        // wait with timeout = 0
        self.park_timeout(Duration::ZERO)
    }

    fn park(&self) -> io::Result<()> {
        self.inner_park(None)
    }

    fn park_timeout(&self, duration: Duration) -> io::Result<()> {
        self.inner_park(Some(duration))
    }

    type Unpark = waker::UnparkHandle;

    fn unpark(&self) -> Self::Unpark {
        PollerInner::unpark(&self.inner)
    }
}

impl Drop for PollerDriver {
    fn drop(&mut self) {
        // Clean up any resources if necessary
    }
}

