//! Core signal handling types.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::driver::UnparkHandle;
use crate::io::stream::Stream;

use super::SignalKind;
use super::registry::SignalHandle;

/// An async signal listener.
///
/// `Signal` implements [`Stream`] and yields `()` each time the signal is received.
/// It can also be used directly with `recv().await`.
///
/// # Examples
///
/// ```no_run
/// use maniac::signal::{signal, SignalKind};
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     let mut sig = signal(SignalKind::interrupt())?;
///     
///     // Wait for the first signal
///     sig.recv().await;
///     println!("Received signal");
///     
///     Ok(())
/// }
/// ```
#[must_use = "signals do nothing unless polled"]
pub struct Signal {
    handle: SignalHandle,
    registered: bool,
}

impl Signal {
    /// Creates a new signal listener for the specified signal kind.
    ///
    /// This registers a signal handler with the OS. The handler will be
    /// unregistered when the `Signal` is dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if the signal handler cannot be registered.
    pub fn new(kind: SignalKind) -> io::Result<Self> {
        let signum = kind.as_raw();

        // Register the signal handler with the OS
        #[cfg(unix)]
        super::unix::register_signal(signum)?;

        #[cfg(windows)]
        super::windows::register_signal(signum)?;

        let handle = SignalHandle::new(signum);

        // Try to register the unpark handle
        if let Ok(unpark) = UnparkHandle::try_current() {
            handle.register_unpark(&unpark);
        }

        Ok(Self {
            handle,
            registered: true,
        })
    }

    /// Receives the next signal notification.
    ///
    /// Returns `Some(())` when a signal is received. Returns `None` if the
    /// signal handler has been unregistered (which should not happen under
    /// normal circumstances).
    ///
    /// # Cancel Safety
    ///
    /// This method is cancel safe. If `recv` is used as part of a `select!`
    /// and another branch completes first, no signal notifications are lost.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::signal::{signal, SignalKind};
    ///
    /// #[maniac::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let mut sig = signal(SignalKind::terminate())?;
    ///     
    ///     sig.recv().await;
    ///     println!("Received SIGTERM");
    ///     
    ///     Ok(())
    /// }
    /// ```
    pub async fn recv(&mut self) {
        RecvFuture { signal: self }.await
    }

    /// Polls for the next signal notification.
    ///
    /// This is the lower-level polling interface. Most users should use
    /// `recv().await` instead.
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<()>> {
        // Check if a signal is pending
        if self.handle.try_recv() {
            return Poll::Ready(Some(()));
        }

        // Register the waker for notification
        self.handle.register_waker(cx.waker());

        // Check again after registering (avoid race condition)
        if self.handle.try_recv() {
            return Poll::Ready(Some(()));
        }

        Poll::Pending
    }
}

impl Stream for Signal {
    type Item = ();

    async fn next(&mut self) -> Option<Self::Item> {
        self.recv().await;
        Some(())
    }
}

/// Future for receiving a signal.
struct RecvFuture<'a> {
    signal: &'a mut Signal,
}

impl<'a> Future for RecvFuture<'a> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.signal.poll_recv(cx) {
            Poll::Ready(Some(())) => Poll::Ready(()),
            Poll::Ready(None) => {
                // Signal handler was unregistered, keep waiting
                // This shouldn't normally happen
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
