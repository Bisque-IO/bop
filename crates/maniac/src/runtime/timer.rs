use super::task::TaskSlot;
use super::worker::{
    cancel_timer_for_current_task, current_worker_now_ns, schedule_timer_for_task,
};
use std::future::Future;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerState {
    Idle = 0,
    Scheduled = 1,
    Cancelled = 2,
}

impl From<u8> for TimerState {
    fn from(val: u8) -> Self {
        match val {
            0 => TimerState::Idle,
            1 => TimerState::Scheduled,
            2 => TimerState::Cancelled,
            _ => TimerState::Idle, // Should not happen with correct usage
        }
    }
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
    state: AtomicU8,
    deadline_ns: AtomicU64,
    task_slot: AtomicPtr<TaskSlot>,
    task_id: AtomicU32,
    worker_id: AtomicU32,
    timer_id: AtomicU64,
}

impl Timer {
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(TimerState::Idle as u8),
            deadline_ns: AtomicU64::new(0),
            task_slot: AtomicPtr::new(ptr::null_mut()),
            task_id: AtomicU32::new(0),
            worker_id: AtomicU32::new(u32::MAX),
            timer_id: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn delay(&self, duration: Duration) -> TimerDelay<'_> {
        TimerDelay::new(self, duration)
    }

    #[inline(always)]
    pub fn state(&self) -> TimerState {
        self.state.load(Ordering::Relaxed).into()
    }

    #[inline(always)]
    pub fn deadline_ns(&self) -> u64 {
        self.deadline_ns.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn is_scheduled(&self) -> bool {
        self.state() == TimerState::Scheduled
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
        self.task_slot.store(task_slot.as_ptr(), Ordering::Relaxed);
        self.task_id.store(task_id, Ordering::Relaxed);
        self.worker_id.store(worker_id, Ordering::Relaxed);
        self.timer_id.store(0, Ordering::Relaxed);
        self.deadline_ns.store(0, Ordering::Relaxed);
        self.state.store(TimerState::Idle as u8, Ordering::Relaxed);

        TimerHandle::new(task_slot, task_id, worker_id, 0)
    }

    #[inline(always)]
    pub(crate) fn commit_schedule(&self, timer_id: u64, deadline_ns: u64) {
        self.timer_id.store(timer_id, Ordering::Relaxed);
        self.deadline_ns.store(deadline_ns, Ordering::Relaxed);
        self.state
            .store(TimerState::Scheduled as u8, Ordering::Relaxed);
    }

    #[inline(always)]
    pub(crate) fn mark_cancelled(&self, timer_id: u64) -> bool {
        if self.timer_id.load(Ordering::Relaxed) != timer_id {
            return false;
        }
        self.state
            .store(TimerState::Cancelled as u8, Ordering::Relaxed);
        self.clear_identity();
        true
    }

    #[inline(always)]
    pub(crate) fn reset(&self) {
        self.state.store(TimerState::Idle as u8, Ordering::Relaxed);
        self.clear_identity();
    }

    #[inline(always)]
    pub(crate) fn worker_id(&self) -> Option<u32> {
        match self.worker_id.load(Ordering::Relaxed) {
            id if id == u32::MAX => None,
            id => Some(id),
        }
    }

    #[inline(always)]
    pub(crate) fn timer_id(&self) -> u64 {
        self.timer_id.load(Ordering::Relaxed)
    }

    #[inline(always)]
    fn clear_identity(&self) {
        self.deadline_ns.store(0, Ordering::Relaxed);
        self.timer_id.store(0, Ordering::Relaxed);
        self.worker_id.store(u32::MAX, Ordering::Relaxed);
        self.task_slot.store(ptr::null_mut(), Ordering::Relaxed);
    }
}

// Timer is now naturally Sync because Atomic types are Sync
unsafe impl Send for Timer {}
// unsafe impl Sync for Timer {}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
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
        let now_ns = current_worker_now_ns();
        if now_ns >= deadline {
            self.timer.reset();
            self.scheduled = false;
            return Poll::Ready(());
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
