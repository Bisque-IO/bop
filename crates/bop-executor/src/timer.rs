use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerState {
    Idle = 0,
    Scheduled = 1,
    Cancelled = 2,
}

impl TimerState {
    #[inline(always)]
    fn from_u8(value: u8) -> Self {
        match value {
            0 => TimerState::Idle,
            1 => TimerState::Scheduled,
            2 => TimerState::Cancelled,
            _ => TimerState::Idle,
        }
    }
}

/// Allocation-free timer that can be embedded directly inside a task or future.
#[derive(Debug)]
pub struct Timer {
    state: AtomicU8,
    worker_id: AtomicU32,
    deadline_ns: AtomicU64,
    interval_ns: AtomicU64,
    task_ptr: AtomicPtr<()>,
}

impl Timer {
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(TimerState::Idle as u8),
            worker_id: AtomicU32::new(u32::MAX),
            deadline_ns: AtomicU64::new(0),
            interval_ns: AtomicU64::new(0),
            task_ptr: AtomicPtr::new(ptr::null_mut()),
        }
    }

    #[inline(always)]
    pub fn state(&self) -> TimerState {
        TimerState::from_u8(self.state.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub fn worker_id(&self) -> Option<u32> {
        match self.worker_id.load(Ordering::Acquire) {
            id if id == u32::MAX => None,
            id => Some(id),
        }
    }

    #[inline(always)]
    pub fn deadline_ns(&self) -> u64 {
        self.deadline_ns.load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn interval_ns(&self) -> u64 {
        self.interval_ns.load(Ordering::Acquire)
    }

    /// Sets a repeating interval in nanoseconds (0 => one-shot).
    #[inline(always)]
    pub fn set_interval(&self, interval: Duration) {
        let nanos = interval.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.interval_ns.store(nanos, Ordering::Release);
    }

    /// Clears any previously configured interval.
    #[inline(always)]
    pub fn clear_interval(&self) {
        self.interval_ns.store(0, Ordering::Release);
    }

    /// Prepares the timer for scheduling on the specified worker.
    pub fn prepare_schedule(
        &self,
        worker_id: u32,
        task_ptr: *mut (),
        deadline_ns: u64,
        interval_override: Option<u64>,
    ) {
        if let Some(interval) = interval_override {
            self.interval_ns.store(interval, Ordering::Release);
        }

        self.worker_id.store(worker_id, Ordering::Release);
        self.deadline_ns.store(deadline_ns, Ordering::Release);
        self.task_ptr.store(task_ptr, Ordering::Release);
        self.state
            .store(TimerState::Scheduled as u8, Ordering::Release);
    }

    /// Attempts to cancel a scheduled timer.
    pub fn cancel(&self) -> bool {
        loop {
            let current = TimerState::from_u8(self.state.load(Ordering::Acquire));
            match current {
                TimerState::Scheduled => {
                    if self
                        .state
                        .compare_exchange(
                            TimerState::Scheduled as u8,
                            TimerState::Cancelled as u8,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        self.worker_id.store(u32::MAX, Ordering::Release);
                        self.task_ptr.store(ptr::null_mut(), Ordering::Release);
                        return true;
                    }
                }
                TimerState::Cancelled | TimerState::Idle => return false,
            }
        }
    }

    /// Takes ownership of the scheduled task pointer if the deadline matches the expectation.
    pub fn take_scheduled_task(&self, expected_deadline_ns: u64) -> Option<*mut ()> {
        if self.deadline_ns.load(Ordering::Acquire) != expected_deadline_ns {
            return None;
        }

        if self
            .state
            .compare_exchange(
                TimerState::Scheduled as u8,
                TimerState::Idle as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return None;
        }

        let ptr = self.task_ptr.swap(ptr::null_mut(), Ordering::AcqRel);
        if ptr.is_null() { None } else { Some(ptr) }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

/// Thin wrapper that embeds a timer directly inside user futures.
#[derive(Debug, Default)]
pub struct TimerHandle {
    inner: Timer,
}

impl TimerHandle {
    pub const fn new() -> Self {
        Self {
            inner: Timer::new(),
        }
    }

    #[inline(always)]
    pub fn inner(&self) -> &Timer {
        &self.inner
    }

    #[inline(always)]
    pub fn as_ptr(&self) -> NonNull<Timer> {
        // SAFETY: `inner` lives for the duration of `self`, never null.
        unsafe { NonNull::new_unchecked(&self.inner as *const Timer as *mut Timer) }
    }

    #[inline(always)]
    pub fn state(&self) -> TimerState {
        self.inner.state()
    }

    #[inline(always)]
    pub fn cancel(&self) -> bool {
        self.inner.cancel()
    }

    #[inline(always)]
    pub fn configure_interval(&self, interval: Option<Duration>) {
        match interval {
            Some(duration) if duration.as_nanos() > 0 => self.inner.set_interval(duration),
            _ => self.inner.clear_interval(),
        }
    }
}
