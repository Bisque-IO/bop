use std::task::{Context, Poll};

use super::ready::{Direction, Ready};
use crate::future::waker::DiatomicWaker;

pub(crate) struct ScheduledIo {
    readiness: Ready,

    /// Waker used for AsyncRead (thread-safe, supports concurrent updates).
    pub(crate) reader: DiatomicWaker,
    /// Waker used for AsyncWrite (thread-safe, supports concurrent updates).
    pub(crate) writer: DiatomicWaker,
}

impl Default for ScheduledIo {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl ScheduledIo {
    pub(crate) fn new() -> Self {
        Self {
            readiness: Ready::EMPTY,
            reader: DiatomicWaker::new(),
            writer: DiatomicWaker::new(),
        }
    }

    #[allow(unused)]
    #[inline]
    pub(crate) fn set_writable(&mut self) {
        self.readiness |= Ready::WRITABLE;
    }

    #[inline]
    pub(crate) fn set_readiness(&mut self, f: impl Fn(Ready) -> Ready) {
        self.readiness = f(self.readiness);
    }

    #[inline]
    pub(crate) fn wake(&mut self, ready: Ready) {
        if ready.is_readable() {
            self.reader.notify();
        }
        if ready.is_writable() {
            self.writer.notify();
        }
    }

    #[inline]
    pub(crate) fn clear_readiness(&mut self, ready: Ready) {
        self.readiness = self.readiness - ready;
    }

    #[allow(clippy::needless_pass_by_ref_mut)]
    #[inline]
    pub(crate) fn poll_readiness(
        &mut self,
        cx: &mut Context<'_>,
        direction: Direction,
    ) -> Poll<Ready> {
        let ready = direction.mask() & self.readiness;
        if !ready.is_empty() {
            return Poll::Ready(ready);
        }
        self.set_waker(cx, direction);
        Poll::Pending
    }

    #[inline]
    pub(crate) fn set_waker(&mut self, cx: &mut Context<'_>, direction: Direction) {
        let waker = match direction {
            Direction::Read => &self.reader,
            Direction::Write => &self.writer,
        };
        // SAFETY: set_waker is only called from the polling context and there is no
        // concurrent access to register/unregister from multiple threads.
        unsafe {
            waker.register(cx.waker());
        }
    }
}
