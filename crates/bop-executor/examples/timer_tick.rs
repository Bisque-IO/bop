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

struct TickPrinter {
    timer: TimerHandle,
    interval: Duration,
    remaining: usize,
    scheduled: bool,
}

impl TickPrinter {
    fn new(ticks: usize, interval: Duration) -> Self {
        Self {
            timer: TimerHandle::new(),
            interval,
            remaining: ticks,
            scheduled: false,
        }
    }
}

impl Future for TickPrinter {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.remaining == 0 {
            return Poll::Ready(());
        }

        if !self.scheduled {
            schedule_timer_for_current_task(cx, &self.timer, self.interval);
            self.scheduled = true;
            return Poll::Pending;
        }

        match self.timer.state() {
            TimerState::Idle => {
                println!("tick");
                self.remaining -= 1;
                if self.remaining == 0 {
                    return Poll::Ready(());
                }
                schedule_timer_for_current_task(cx, &self.timer, self.interval);
                Poll::Pending
            }
            TimerState::Cancelled => Poll::Ready(()),
            TimerState::Scheduled => Poll::Pending,
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let runtime = Runtime::new(ArenaConfig::new(2, 16)?, ArenaOptions::default(), 1)?;

    let tick_future = TickPrinter::new(5, Duration::from_millis(500));
    let handle = runtime.spawn(tick_future)?;

    block_on(handle);
    Ok(())
}
