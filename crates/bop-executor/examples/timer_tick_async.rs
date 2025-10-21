use bop_executor::runtime::Runtime;
use bop_executor::task::{ArenaConfig, ArenaOptions};
use bop_executor::timer::{TimerHandle, TimerState};
use bop_executor::worker::schedule_timer_for_current_task;
use futures_lite::future::block_on;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

struct TimerDelay<'a> {
    timer: &'a TimerHandle,
    interval: Duration,
    scheduled: bool,
}

impl<'a> TimerDelay<'a> {
    fn new(timer: &'a TimerHandle, interval: Duration) -> Self {
        Self {
            timer,
            interval,
            scheduled: false,
        }
    }
}

impl<'a> Future for TimerDelay<'a> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.scheduled {
            schedule_timer_for_current_task(cx, self.timer, self.interval);
            self.scheduled = true;
            return Poll::Pending;
        }

        match self.timer.state() {
            TimerState::Idle | TimerState::Cancelled => Poll::Ready(()),
            TimerState::Scheduled => Poll::Pending,
        }
    }
}

async fn tick_printer(timer: TimerHandle, ticks: usize, interval: Duration) {
    for remaining in (0..ticks).rev() {
        TimerDelay::new(&timer, interval).await;
        println!("tick ({} remaining)", remaining);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let runtime = Runtime::new(ArenaConfig::new(2, 16)?, ArenaOptions::default(), 1)?;

    let handle = runtime.spawn(async move {
        tick_printer(TimerHandle::new(), 50, Duration::from_millis(500)).await;
    })?;

    block_on(handle);
    Ok(())
}
