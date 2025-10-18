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
