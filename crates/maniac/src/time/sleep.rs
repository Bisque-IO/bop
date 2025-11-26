//! Sleep future for async time delays.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use pin_project_lite::pin_project;

use crate::runtime::timer::{Timer, TimerDelay};
use crate::time::Instant;

/// A future that completes after a specified duration.
///
/// This is created by [`sleep`] or [`sleep_until`].
///
/// # Examples
///
/// ```
/// use maniac::time::{sleep, Duration};
///
/// #[maniac::main]
/// async fn main() {
///     sleep(Duration::from_secs(1)).await;
///     println!("1 second elapsed");
/// }
/// ```
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Sleep {
    timer: Timer,
    delay: Option<TimerDelay<'static>>,
}

// SAFETY: Sleep contains TimerDelay which has a lifetime parameter, but we ensure
// the Timer outlives the TimerDelay by owning it. We use unsafe to extend the lifetime
// which is safe because Timer is owned and won't be dropped until Sleep is dropped.
unsafe impl Send for Sleep {}
unsafe impl Sync for Sleep {}

impl Sleep {
    /// Creates a new `Sleep` that will complete after the specified duration.
    ///
    /// # Examples
    ///
    /// ```
    /// use maniac::time::{sleep, Duration};
    ///
    /// #[maniac::main]
    /// async fn main() {
    ///     let sleep = Sleep::new(Duration::from_secs(1));
    ///     sleep.await;
    /// }
    /// ```
    pub fn new(duration: Duration) -> Self {
        let timer = Timer::new();
        let delay = timer.delay(duration);
        
        // SAFETY: We own the Timer, so it will live as long as Sleep.
        // We extend the lifetime to 'static because Timer is owned and won't be dropped.
        let delay = unsafe { std::mem::transmute::<TimerDelay<'_>, TimerDelay<'static>>(delay) };
        
        Self {
            timer,
            delay: Some(delay),
        }
    }

    /// Creates a new `Sleep` that will complete at the specified instant.
    ///
    /// # Examples
    ///
    /// ```
    /// use maniac::time::{sleep_until, Duration, Instant};
    ///
    /// #[maniac::main]
    /// async fn main() {
    ///     let deadline = Instant::now() + Duration::from_secs(1);
    ///     let sleep = Sleep::new_timeout(deadline);
    ///     sleep.await;
    /// }
    /// ```
    pub fn new_timeout(deadline: Instant) -> Self {
        let now = Instant::now();
        if deadline <= now {
            // Already elapsed, use zero duration
            Self::new(Duration::ZERO)
        } else {
            Self::new(deadline - now)
        }
    }

    /// Creates a new `Sleep` that will never complete.
    ///
    /// This is useful for creating timeouts that effectively never expire.
    pub fn far_future() -> Self {
        Self::new(Duration::from_secs(86400 * 365 * 30)) // ~30 years
    }

    /// Resets the `Sleep` to complete at a new deadline.
    ///
    /// This method can change the instant at which the `Sleep` completes without
    /// having to create a new `Sleep` instance.
    ///
    /// Calling `reset` before the `Sleep` has completed is equivalent to creating
    /// a new `Sleep` with the new deadline. The new deadline will be used to
    /// determine when the `Sleep` completes.
    ///
    /// Calling `reset` after the `Sleep` has completed is equivalent to creating
    /// a new `Sleep`.
    pub fn reset(&mut self, deadline: Instant) {
        let now = Instant::now();
        let duration = if deadline <= now {
            Duration::ZERO
        } else {
            deadline - now
        };
        
        // Cancel the existing timer
        let _ = self.timer.cancel();
        
        // Create a new delay
        let delay = self.timer.delay(duration);
        let delay = unsafe { std::mem::transmute::<TimerDelay<'_>, TimerDelay<'static>>(delay) };
        self.delay = Some(delay);
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(ref mut delay) = self.delay {
            match Pin::new(delay).poll(cx) {
                Poll::Ready(()) => {
                    self.delay = None;
                    Poll::Ready(())
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            // Already completed
            Poll::Ready(())
        }
    }
}

impl std::fmt::Debug for Sleep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sleep")
            .field("timer", &self.timer)
            .field("delay", if self.delay.is_some() { &"Some" } else { &"None" })
            .finish()
    }
}

/// Sleep for the specified duration.
///
/// This function will return a future that completes after the given duration
/// has elapsed.
///
/// # Examples
///
/// ```
/// use maniac::time::{sleep, Duration};
///
/// #[maniac::main]
/// async fn main() {
///     sleep(Duration::from_secs(1)).await;
///     println!("1 second elapsed");
/// }
/// ```
pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}

/// Sleep until the specified instant.
///
/// This function will return a future that completes when the specified instant
/// is reached.
///
/// # Examples
///
/// ```
/// use maniac::time::{sleep_until, Duration, Instant};
///
/// #[maniac::main]
/// async fn main() {
///     let deadline = Instant::now() + Duration::from_secs(1);
///     sleep_until(deadline).await;
///     println!("deadline reached");
/// }
/// ```
pub fn sleep_until(deadline: Instant) -> Sleep {
    Sleep::new_timeout(deadline)
}

