//! A measurement of a monotonically nondecreasing clock.

use std::{fmt, ops, time::Duration};

/// A measurement of a monotonically nondecreasing clock.
///
/// Instants are always guaranteed to be no less than any previously measured
/// instant when created, and are often useful for tasks such as measuring
/// benchmarks or timing how long an operation takes.
///
/// Note, however, that instants are not guaranteed to be **steady**. In other
/// words, each tick of the underlying clock may not be the same length (e.g.
/// some seconds may be longer than others). An instant may jump forwards or
/// experience time dilation (slow down or speed up), but it will never go
/// backwards.
#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Instant {
    inner: std::time::Instant,
}

impl Instant {
    /// Returns an instant corresponding to "now".
    ///
    /// # Examples
    ///
    /// ```
    /// use maniac::time::Instant;
    ///
    /// let now = Instant::now();
    /// ```
    pub fn now() -> Self {
        Self {
            inner: std::time::Instant::now(),
        }
    }

    /// Create a `maniac::time::Instant` from a `std::time::Instant`.
    pub fn from_std(inner: std::time::Instant) -> Self {
        Self { inner }
    }

    /// Convert the value into a `std::time::Instant`.
    pub fn into_std(self) -> std::time::Instant {
        self.inner
    }

    /// Returns the amount of time elapsed from another instant to this one.
    ///
    /// # Panics
    ///
    /// This function will panic if `earlier` is later than `self`.
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.inner.duration_since(earlier.inner)
    }

    /// Returns the amount of time elapsed from another instant to this one, or
    /// None if that instant is later than this one.
    pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
        self.inner.checked_duration_since(earlier.inner)
    }

    /// Returns the amount of time elapsed from another instant to this one, or
    /// zero duration if that instant is later than this one.
    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.inner.saturating_duration_since(earlier.inner)
    }

    /// Returns the amount of time elapsed since this instant was created.
    ///
    /// # Panics
    ///
    /// This function may panic if the current time is earlier than this
    /// instant, which is something that can happen if an `Instant` is
    /// produced synthetically.
    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    /// Returns `Some(t)` where `t` is the time `self + duration` if `t` can be
    /// represented as `Instant` (which means it's inside the bounds of the
    /// underlying data structure), `None` otherwise.
    pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
        self.inner.checked_add(duration).map(Self::from_std)
    }

    /// Returns `Some(t)` where `t` is the time `self - duration` if `t` can be
    /// represented as `Instant` (which means it's inside the bounds of the
    /// underlying data structure), `None` otherwise.
    pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
        self.inner.checked_sub(duration).map(Self::from_std)
    }

    /// Returns an instant representing a time far in the future.
    ///
    /// This is useful for creating timeouts that effectively never expire.
    pub fn far_future() -> Self {
        // Roughly 30 years from now.
        // API does not provide a way to obtain max `Instant`
        // or convert specific date in the future to instant.
        // 1000 years overflows on macOS, 100 years overflows on FreeBSD.
        Self::now() + Duration::from_secs(86400 * 365 * 30)
    }
}

impl From<std::time::Instant> for Instant {
    fn from(inner: std::time::Instant) -> Self {
        Self::from_std(inner)
    }
}

impl From<Instant> for std::time::Instant {
    fn from(instant: Instant) -> Self {
        instant.into_std()
    }
}

impl ops::Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, other: Duration) -> Self::Output {
        Self::from_std(self.inner + other)
    }
}

impl ops::AddAssign<Duration> for Instant {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

impl ops::Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self::from_std(
            self.inner
                .checked_sub(rhs)
                .expect("overflow when subtracting duration from instant"),
        )
    }
}

impl ops::Sub<Instant> for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Self::Output {
        self.inner - rhs.inner
    }
}

impl ops::SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

impl fmt::Debug for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

