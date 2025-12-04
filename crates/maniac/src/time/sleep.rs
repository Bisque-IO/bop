//! Sleep future for async time delays.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use pin_project_lite::pin_project;

use crate::runtime::timer::{Timer, TimerDelay};
use crate::runtime::worker::{
    current_worker_now_ns, reschedule_timer_for_task, schedule_timer_for_task,
};
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
    delay: Duration,
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

        Self {
            timer,
            delay: duration,
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

    /// Returns whether the sleep timer has been scheduled.
    ///
    /// This method returns `true` if the timer has been registered with the
    /// runtime's timer wheel, and `false` otherwise. A sleep timer becomes
    /// scheduled when it is first polled.
    #[inline]
    pub fn is_scheduled(&self) -> bool {
        self.timer.is_scheduled()
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

        if self.timer.is_scheduled() {
            // Cancel the existing timer
            let _ = self.timer.cancel();
        }

        // Create a new delay
        self.delay = duration;
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.timer.is_scheduled() {
            if schedule_timer_for_task(cx, &self.timer, self.delay).is_some() {}
            return Poll::Pending;
        }

        let now_ns = current_worker_now_ns();
        // Check if we've reached the target deadline
        let target_deadline = self.timer.target_deadline_ns();
        if now_ns >= target_deadline {
            self.timer.reset();
            return Poll::Ready(());
        }

        // Check if the wheel timer fired but we haven't reached target yet (cascading)
        // This happens for long timers that exceed a single wheel's coverage
        if self.timer.needs_reschedule(now_ns) {
            // Reschedule for the remaining time
            if reschedule_timer_for_task(cx, &self.timer) {
                // Successfully rescheduled, stay pending
                return Poll::Pending;
            }
            // Reschedule failed - this shouldn't happen normally
            // Fall through and check target again
        }

        Poll::Pending
    }
}

impl std::fmt::Debug for Sleep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sleep")
            .field("timer", &self.timer)
            .field("delay", &self.delay)
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
