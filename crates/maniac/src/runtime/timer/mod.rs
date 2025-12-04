pub mod ticker;
#[cfg(test)]
mod ticker_tests;
pub mod wheel;

use super::task::TaskSlot;
use super::worker::{
    cancel_timer_for_current_task, current_worker_now_ns, reschedule_timer_for_task,
    schedule_timer_for_task,
};
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::ptr::NonNull;
use std::task::{Context, Poll};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerState {
    Idle,
    Scheduled,
    Cancelled,
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
    /// The user's target deadline (true expiry time)
    target_deadline_ns: Cell<u64>,
    /// The wheel's scheduled deadline (may be earlier than target for cascading)
    wheel_deadline_ns: Cell<u64>,
    task_slot: Cell<Option<NonNull<TaskSlot>>>,
    task_id: Cell<u32>,
    worker_id: Cell<Option<u32>>,
    timer_id: Cell<u64>,
}

impl Timer {
    pub const fn new() -> Self {
        Self {
            state: Cell::new(TimerState::Idle),
            target_deadline_ns: Cell::new(0),
            wheel_deadline_ns: Cell::new(0),
            task_slot: Cell::new(None),
            task_id: Cell::new(0),
            worker_id: Cell::new(None),
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

    /// Get the user's target deadline (true expiry time)
    #[inline(always)]
    pub fn target_deadline_ns(&self) -> u64 {
        self.target_deadline_ns.get()
    }

    /// Get the wheel's scheduled deadline (may be earlier than target for cascading)
    #[inline(always)]
    pub fn wheel_deadline_ns(&self) -> u64 {
        self.wheel_deadline_ns.get()
    }

    /// Backwards compatibility alias for wheel_deadline_ns
    #[inline(always)]
    pub fn deadline_ns(&self) -> u64 {
        self.wheel_deadline_ns.get()
    }

    #[inline(always)]
    pub fn is_scheduled(&self) -> bool {
        self.state() == TimerState::Scheduled
    }

    /// Check if this timer needs to cascade (wheel fired but target not reached)
    #[inline(always)]
    pub fn needs_reschedule(&self, now_ns: u64) -> bool {
        self.is_scheduled()
            && now_ns >= self.wheel_deadline_ns.get()
            && now_ns < self.target_deadline_ns.get()
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
        self.task_slot.set(Some(task_slot));
        self.task_id.set(task_id);
        self.worker_id.set(Some(worker_id));
        self.timer_id.set(0);
        self.target_deadline_ns.set(0);
        self.wheel_deadline_ns.set(0);
        self.state.set(TimerState::Idle);

        TimerHandle::new(task_slot, task_id, worker_id, 0)
    }

    #[inline(always)]
    pub(crate) fn commit_schedule(
        &self,
        timer_id: u64,
        target_deadline_ns: u64,
        wheel_deadline_ns: u64,
    ) {
        self.timer_id.set(timer_id);
        self.target_deadline_ns.set(target_deadline_ns);
        self.wheel_deadline_ns.set(wheel_deadline_ns);
        self.state.set(TimerState::Scheduled);
    }

    /// Update timer after a reschedule (same target, new wheel deadline)
    #[inline(always)]
    pub(crate) fn commit_reschedule(&self, timer_id: u64, wheel_deadline_ns: u64) {
        self.timer_id.set(timer_id);
        self.wheel_deadline_ns.set(wheel_deadline_ns);
        // target_deadline_ns stays the same
        // state stays Scheduled
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
        self.worker_id.get()
    }

    #[inline(always)]
    pub(crate) fn timer_id(&self) -> u64 {
        self.timer_id.get()
    }

    #[inline(always)]
    fn clear_identity(&self) {
        self.target_deadline_ns.set(0);
        self.wheel_deadline_ns.set(0);
        self.timer_id.set(0);
        self.worker_id.set(None);
        self.task_slot.set(None);
    }
}

// SAFETY: Timer is only accessed by a single worker thread at a time,
// as guaranteed by the scheduler. The Cell fields are not Sync, but
// we manually implement Send to allow Timer to be moved between threads
// during task migration (which only happens when no thread is accessing it).
unsafe impl Send for Timer {}
unsafe impl Sync for Timer {}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if self.is_scheduled() {
            self.cancel();
        }
    }
}

pub struct TimerDelay<'a> {
    timer: &'a Timer,
    delay: Duration,
    scheduled: bool,
}

impl<'a> TimerDelay<'a> {
    #[inline(always)]
    pub fn new(timer: &'a Timer, delay: Duration) -> Self {
        Self {
            timer,
            delay,
            scheduled: false,
        }
    }
}

impl<'a> Future for TimerDelay<'a> {
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
            if reschedule_timer_for_task(cx, self.timer) {
                // Successfully rescheduled, stay pending
                return Poll::Pending;
            }
            // Reschedule failed - this shouldn't happen normally
            // Fall through and check target again
        }

        Poll::Pending
    }
}

impl<'a> Drop for TimerDelay<'a> {
    fn drop(&mut self) {
        if self.scheduled {
            let _ = self.timer.cancel();
        }
    }
}
