//! Interval stream for periodic async execution.

use std::time::Duration;

use crate::io::stream::Stream;
use crate::time::{Instant, Sleep};

/// Behavior when a tick is missed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissedTickBehavior {
    /// Ticks as fast as possible until caught up.
    Burst,
    /// Skips missed ticks and tick on the next multiple of `period` from `start`.
    Skip,
    /// Tick immediately whenever this happens.
    Delay,
}

/// A stream representing notifications at fixed interval.
///
/// The `Interval` stream ticks at a fixed period. After each tick completes,
/// the next tick is immediately scheduled.
///
/// An `Interval` can be used as a `Stream` to repeatedly execute work on a schedule.
///
/// # Examples
///
/// ```
/// use maniac::time::{interval, Duration};
///
/// #[maniac::main]
/// async fn main() {
///     let mut interval = interval(Duration::from_secs(1));
///     loop {
///         interval.tick().await;
///         println!("tick");
///     }
/// }
/// ```
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Interval {
    /// The sleep future for the next tick.
    sleep: Sleep,
    /// The duration between ticks.
    period: Duration,
    /// The instant at which the interval was created.
    start: Instant,
    /// The behavior when a tick is missed.
    missed_tick_behavior: MissedTickBehavior,
}

impl Interval {
    /// Creates a new `Interval` that yields with the first tick at `start` and
    /// then every `period` after that.
    ///
    /// # Examples
    ///
    /// ```
    /// use maniac::time::{interval_at, Duration, Instant};
    ///
    /// #[maniac::main]
    /// async fn main() {
    ///     let start = Instant::now() + Duration::from_secs(5);
    ///     let mut interval = interval_at(start, Duration::from_secs(1));
    ///     interval.tick().await;
    ///     println!("5 seconds later");
    /// }
    /// ```
    pub fn new_at(start: Instant, period: Duration) -> Self {
        let sleep = Sleep::new_timeout(start);
        Self {
            sleep,
            period,
            start,
            missed_tick_behavior: MissedTickBehavior::Burst,
        }
    }

    /// Creates a new `Interval` that yields immediately and then every `period`
    /// after that.
    ///
    /// # Examples
    ///
    /// ```
    /// use maniac::time::{interval, Duration};
    ///
    /// #[maniac::main]
    /// async fn main() {
    ///     let mut interval = interval(Duration::from_secs(1));
    ///     interval.tick().await; // ticks immediately
    ///     interval.tick().await; // ticks after 1 second
    /// }
    /// ```
    pub fn new(period: Duration) -> Self {
        Self::new_at(Instant::now(), period)
    }

    /// Sets the behavior when a tick is missed.
    ///
    /// See [`MissedTickBehavior`] for more details.
    pub fn set_missed_tick_behavior(&mut self, behavior: MissedTickBehavior) {
        self.missed_tick_behavior = behavior;
    }

    /// Completes when the next interval tick completes.
    ///
    /// The first call to `tick()` completes immediately. Subsequent calls
    /// wait for the next period to elapse.
    ///
    /// # Examples
    ///
    /// ```
    /// use maniac::time::{interval, Duration};
    ///
    /// #[maniac::main]
    /// async fn main() {
    ///     let mut interval = interval(Duration::from_secs(1));
    ///     interval.tick().await; // ticks immediately
    ///     interval.tick().await; // ticks after 1 second
    /// }
    /// ```
    pub async fn tick(&mut self) {
        self.next().await;
    }
}

impl crate::io::stream::Stream for Interval {
    type Item = Instant;

    async fn next(&mut self) -> Option<Self::Item> {
        // Wait for the sleep to complete
        (&mut self.sleep).await;
        
        let now = Instant::now();
        let next_tick = match self.missed_tick_behavior {
            MissedTickBehavior::Burst => {
                // Schedule next tick immediately
                now + self.period
            }
            MissedTickBehavior::Skip => {
                // Calculate the next tick that is a multiple of period from start
                let elapsed = now - self.start;
                let elapsed_periods = elapsed.as_nanos() / self.period.as_nanos();
                let next_period = elapsed_periods + 1;
                let next_period_duration = Duration::from_nanos(
                    self.period.as_nanos().saturating_mul(next_period).min(u64::MAX as u128) as u64
                );
                self.start + next_period_duration
            }
            MissedTickBehavior::Delay => {
                // Schedule next tick normally
                now + self.period
            }
        };

        // Reset the sleep for the next tick
        self.sleep = Sleep::new_timeout(next_tick);

        Some(now)
    }
}

/// Creates a new `Interval` that yields immediately and then every `period`
/// after that.
///
/// # Examples
///
/// ```
/// use maniac::time::{interval, Duration};
///
/// #[maniac::main]
/// async fn main() {
///     let mut interval = interval(Duration::from_secs(1));
///     interval.tick().await;
/// }
/// ```
pub fn interval(period: Duration) -> Interval {
    Interval::new(period)
}

/// Creates a new `Interval` that yields with the first tick at `start` and
/// then every `period` after that.
///
/// # Examples
///
/// ```
/// use maniac::time::{interval_at, Duration, Instant};
///
/// #[maniac::main]
/// async fn main() {
///     let start = Instant::now() + Duration::from_secs(5);
///     let mut interval = interval_at(start, Duration::from_secs(1));
///     interval.tick().await;
/// }
/// ```
pub fn interval_at(start: Instant, period: Duration) -> Interval {
    Interval::new_at(start, period)
}

