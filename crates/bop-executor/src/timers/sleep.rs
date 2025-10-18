use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::timers::handle::{TimerHandle, TimerState};
use crate::timers::service::TimerService;
use crate::worker::schedule_timer_for_current_task;

pub struct Sleep {
    timer: TimerHandle,
    duration: Duration,
    generation: Option<u64>,
    service: Option<Arc<TimerService>>,
    completed: bool,
}

impl Sleep {
    pub fn new(duration: Duration) -> Self {
        Self {
            timer: TimerHandle::new(),
            duration,
            generation: None,
            service: None,
            completed: false,
        }
    }

    pub fn reset(&mut self, duration: Duration) {
        self.duration = duration;
        self.generation = None;
        self.completed = false;
        self.service = None;
    }
}

pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.completed {
            return Poll::Ready(());
        }

        if let Some(stored_generation) = self.generation {
            if self.timer.inner().generation() == stored_generation
                && self.timer.state() == TimerState::Idle
            {
                self.completed = true;
                return Poll::Ready(());
            }
        }

        if self.generation.is_none() {
            match schedule_timer_for_current_task(cx, &self.timer, self.duration) {
                Some((generation, service)) => {
                    self.generation = Some(generation);
                    self.service = Some(service);
                }
                None => panic!("sleep() must be awaited inside a bop worker with timers"),
            }
        }

        Poll::Pending
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if !self.completed {
            if let Some(service) = self.service.as_ref() {
                let _ = self.timer.cancel(service.as_ref());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timers::service::{TimerConfig, TimerService};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    #[test]
    fn sleep_reset_clears_internal_state() {
        let mut sleep = Sleep::new(Duration::from_millis(5));
        sleep.generation = Some(42);
        sleep.completed = true;
        sleep.service = Some(TimerService::start(TimerConfig::default()));

        sleep.reset(Duration::from_millis(2));

        assert!(sleep.generation.is_none());
        assert!(!sleep.completed);
        assert!(sleep.service.is_none());
        assert_eq!(sleep.duration, Duration::from_millis(2));
    }

    #[test]
    fn dropping_sleep_cancels_pending_timer() {
        let service = TimerService::start(TimerConfig::default());
        {
            let mut sleep = Sleep::new(Duration::from_millis(10));
            sleep.generation = Some(1);
            sleep.service = Some(Arc::clone(&service));
            sleep
                .timer
                .inner()
                .store_state(TimerState::Scheduled, Ordering::Release);
            sleep.timer.inner().set_home_worker(Some(0));
            sleep.timer.inner().set_stripe_hint(1);
        }
        assert!(service.cancellation_count() >= 1);
        service.shutdown();
    }
}
