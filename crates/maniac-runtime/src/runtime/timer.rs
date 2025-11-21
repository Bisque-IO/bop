use super::task::TaskSlot;
use super::worker::{
    cancel_timer_for_current_task, current_worker_now_ns, schedule_timer_for_task,
};
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerState {
    Idle = 0,
    Scheduled = 1,
    Cancelled = 2,
}

#[derive(Clone, Copy, Debug)]
pub struct TimerHandle {
    task_slot: NonNull<TaskSlot>,
    task_id: u32,
    worker_id: u32,
    timer_id: u64,
}

impl TimerHandle {
    #[inline(always)]
    pub(crate) fn new(
        task_slot: NonNull<TaskSlot>,
        task_id: u32,
        worker_id: u32,
        timer_id: u64,
    ) -> Self {
        Self {
            task_slot,
            task_id,
            worker_id,
            timer_id,
        }
    }

    #[inline(always)]
    pub(crate) fn task_slot(&self) -> NonNull<TaskSlot> {
        self.task_slot
    }

    #[inline(always)]
    pub(crate) fn task_id(&self) -> u32 {
        self.task_id
    }

    #[inline(always)]
    pub(crate) fn worker_id(&self) -> u32 {
        self.worker_id
    }

    #[inline(always)]
    pub(crate) fn timer_id(&self) -> u64 {
        self.timer_id
    }
}

unsafe impl Send for TimerHandle {}
unsafe impl Sync for TimerHandle {}

#[derive(Debug)]
pub struct Timer {
    state: Cell<TimerState>,
    deadline_ns: Cell<u64>,
    task_slot: Cell<*mut TaskSlot>,
    task_id: Cell<u32>,
    worker_id: Cell<u32>,
    timer_id: Cell<u64>,
}

impl Timer {
    pub const fn new() -> Self {
        Self {
            state: Cell::new(TimerState::Idle),
            deadline_ns: Cell::new(0),
            task_slot: Cell::new(ptr::null_mut()),
            task_id: Cell::new(0),
            worker_id: Cell::new(u32::MAX),
            timer_id: Cell::new(0),
        }
    }

    #[inline(always)]
    pub fn delay(&self, duration: Duration) -> TimerDelay<'_> {
        TimerDelay::new(self, duration)
    }

    #[inline(always)]
    pub fn state(&self) -> TimerState {
        self.state.get()
    }

    #[inline(always)]
    pub fn deadline_ns(&self) -> u64 {
        self.deadline_ns.get()
    }

    #[inline(always)]
    pub fn is_scheduled(&self) -> bool {
        self.state.get() == TimerState::Scheduled
    }

    #[inline(always)]
    pub fn cancel(&self) -> bool {
        cancel_timer_for_current_task(self)
    }

    #[inline(always)]
    pub(crate) fn prepare(
        &self,
        task_slot: NonNull<TaskSlot>,
        task_id: u32,
        worker_id: u32,
    ) -> TimerHandle {
        self.task_slot.set(task_slot.as_ptr());
        self.task_id.set(task_id);
        self.worker_id.set(worker_id);
        self.timer_id.set(0);
        self.deadline_ns.set(0);
        self.state.set(TimerState::Idle);

        TimerHandle::new(task_slot, task_id, worker_id, 0)
    }

    #[inline(always)]
    pub(crate) fn commit_schedule(&self, timer_id: u64, deadline_ns: u64) {
        self.timer_id.set(timer_id);
        self.deadline_ns.set(deadline_ns);
        self.state.set(TimerState::Scheduled);
    }

    #[inline(always)]
    pub(crate) fn mark_cancelled(&self, timer_id: u64) -> bool {
        if self.timer_id.get() != timer_id {
            return false;
        }
        self.state.set(TimerState::Cancelled);
        self.clear_identity();
        true
    }

    #[inline(always)]
    pub(crate) fn reset(&self) {
        self.state.set(TimerState::Idle);
        self.clear_identity();
    }

    #[inline(always)]
    pub(crate) fn worker_id(&self) -> Option<u32> {
        match self.worker_id.get() {
            id if id == u32::MAX => None,
            id => Some(id),
        }
    }

    #[inline(always)]
    pub(crate) fn timer_id(&self) -> u64 {
        self.timer_id.get()
    }

    #[inline(always)]
    fn clear_identity(&self) {
        self.deadline_ns.set(0);
        self.timer_id.set(0);
        self.worker_id.set(u32::MAX);
        self.task_slot.set(ptr::null_mut());
    }
}

unsafe impl Send for Timer {}
unsafe impl Sync for Timer {}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TimerDelay<'a> {
    timer: &'a Timer,
    delay: Duration,
    deadline: Instant,
    scheduled: bool,
}

impl<'a> TimerDelay<'a> {
    #[inline(always)]
    pub fn new(timer: &'a Timer, delay: Duration) -> Self {
        Self {
            timer,
            delay,
            deadline: Instant::now().checked_add(delay).unwrap(),
            scheduled: false,
        }
    }
}

impl<'a> Future for TimerDelay<'a> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.scheduled {
            if schedule_timer_for_task(cx, self.timer, self.delay).is_some() {
                self.scheduled = true;
            }
            return Poll::Pending;
        }

        // Check timer deadline using the worker's time source (same as timer wheel)
        // rather than Instant::now() which may use a different clock on some platforms
        let deadline = self.timer.deadline_ns();
        if let Some(now_ns) = current_worker_now_ns() {
            if now_ns >= deadline {
                self.timer.reset();
                self.scheduled = false;
                return Poll::Ready(());
            }
        }

        Poll::Pending

        // match self.timer.state() {
        //     TimerState::Idle | TimerState::Cancelled => {
        //         println!("TimerDelay::Idle | Cancelled");
        //         self.scheduled = false;
        //         Poll::Ready(())
        //     }
        //     TimerState::Scheduled => {
        //         let deadline = self.timer.deadline_ns();
        //         if deadline == 0 {
        //             println!("TimerDelay deadline = 0");
        //             return Poll::Pending;
        //         }

        //         if let Some(now_ns) = current_worker_now_ns() {
        //             println!("TimerDelay now_ns = {}", now_ns);
        //             if now_ns >= deadline {
        //                 self.timer.reset();
        //                 self.scheduled = false;
        //                 println!("TimerDelay now_ns >= deadline!");
        //                 return Poll::Ready(());
        //             }
        //         }
        //         println!("TimerDelay Pending");
        //         Poll::Pending
        //     }
        // }
    }
}

impl<'a> Drop for TimerDelay<'a> {
    fn drop(&mut self) {
        if self.scheduled {
            let _ = self.timer.cancel();
        }
    }
}
