use crate::timers::service::TimerService;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicU32, AtomicU64, Ordering};

/// Lifecycle states for a timer tracked by the executor.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerState {
    /// Timer is idle and unscheduled.
    Idle = 0,
    /// Timer is scheduled on a worker's local wheel.
    Scheduled = 1,
    /// Timer is currently executing on a worker.
    Firing = 2,
    /// Timer has completed and delivered its payload.
    Fired = 3,
    /// Timer was cancelled by the caller.
    Cancelled = 4,
    /// Timer is migrating between workers (e.g. during shutdown/steal).
    Migrating = 5,
}

impl TimerState {
    #[inline(always)]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    #[inline(always)]
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => TimerState::Idle,
            1 => TimerState::Scheduled,
            2 => TimerState::Firing,
            3 => TimerState::Fired,
            4 => TimerState::Cancelled,
            5 => TimerState::Migrating,
            other => {
                debug_assert!(false, "unexpected timer state {other}");
                TimerState::Idle
            }
        }
    }
}

/// Shared timer metadata used by both workers and the timer thread.
///
/// Instances are reference-counted (`Arc`) so that timer handles can outlive
/// the worker that originally owned them.
#[derive(Debug)]
pub struct TimerInner {
    state: AtomicU8,
    generation: AtomicU64,
    payload: AtomicPtr<()>,
    home_worker: AtomicU32,
    stripe_hint: AtomicU32,
    last_deadline: AtomicU64,
}

impl TimerInner {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(TimerState::Idle.as_u8()),
            generation: AtomicU64::new(0),
            payload: AtomicPtr::new(ptr::null_mut()),
            home_worker: AtomicU32::new(u32::MAX),
            stripe_hint: AtomicU32::new(0),
            last_deadline: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn state(&self, ordering: Ordering) -> TimerState {
        TimerState::from_u8(self.state.load(ordering))
    }

    #[inline(always)]
    pub fn store_state(&self, new_state: TimerState, ordering: Ordering) {
        self.state.store(new_state.as_u8(), ordering);
    }

    #[inline(always)]
    pub fn compare_exchange_state(
        &self,
        current: TimerState,
        new: TimerState,
        success: Ordering,
        failure: Ordering,
    ) -> Result<TimerState, TimerState> {
        self.state
            .compare_exchange(current.as_u8(), new.as_u8(), success, failure)
            .map(TimerState::from_u8)
            .map_err(TimerState::from_u8)
    }

    #[inline(always)]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Bumps the generation counter and returns the new value.
    #[inline(always)]
    pub fn bump_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    #[inline(always)]
    pub fn set_generation(&self, value: u64) {
        self.generation.store(value, Ordering::Release);
    }

    #[inline(always)]
    pub fn payload(&self) -> *mut () {
        self.payload.load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn swap_payload(&self, value: *mut (), ordering: Ordering) -> *mut () {
        self.payload.swap(value, ordering)
    }

    #[inline(always)]
    pub fn set_home_worker(&self, worker: Option<u32>) {
        let value = worker.unwrap_or(u32::MAX);
        self.home_worker.store(value, Ordering::Release);
    }

    #[inline(always)]
    pub fn home_worker(&self) -> Option<u32> {
        let raw = self.home_worker.load(Ordering::Acquire);
        if raw == u32::MAX { None } else { Some(raw) }
    }

    #[inline(always)]
    pub fn set_stripe_hint(&self, hint: u32) {
        self.stripe_hint.store(hint, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn stripe_hint(&self) -> u32 {
        self.stripe_hint.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn record_deadline(&self, deadline: u64) {
        self.last_deadline.store(deadline, Ordering::Release);
    }

    #[inline(always)]
    pub fn last_deadline(&self) -> u64 {
        self.last_deadline.load(Ordering::Acquire)
    }
}

impl Default for TimerInner {
    fn default() -> Self {
        Self::new()
    }
}

/// Reference-counted handle shared with callers.
#[derive(Clone)]
pub struct TimerHandle {
    inner: Arc<TimerInner>,
}

impl TimerHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(TimerInner::new()),
        }
    }

    pub fn from_inner(inner: Arc<TimerInner>) -> Self {
        Self { inner }
    }

    #[inline(always)]
    pub fn inner(&self) -> &Arc<TimerInner> {
        &self.inner
    }

    #[inline(always)]
    pub fn generation(&self) -> u64 {
        self.inner.generation()
    }

    #[inline(always)]
    pub fn state(&self) -> TimerState {
        self.inner.state(Ordering::Acquire)
    }

    pub fn cancel(&self, service: &TimerService) -> bool {
        loop {
            let state = self.inner.state(Ordering::Acquire);
            match state {
                TimerState::Cancelled | TimerState::Fired | TimerState::Firing => return false,
                _ => {}
            }
            let had_entry = matches!(
                state,
                TimerState::Scheduled | TimerState::Migrating | TimerState::Firing
            );
            match self.inner.compare_exchange_state(
                state,
                TimerState::Cancelled,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.inner.bump_generation();
                    let stripe_hint = self.inner.stripe_hint() as usize;
                    let home = self.inner.home_worker();
                    self.inner.swap_payload(ptr::null_mut(), Ordering::AcqRel);
                    service.notify_cancelled(stripe_hint, home, had_entry);
                    return true;
                }
                Err(_) => continue,
            }
        }
    }

    pub fn reschedule(&self, service: &TimerService, deadline_tick: u64) -> u64 {
        let _ = self.cancel(service);
        service.schedule_handle(self, deadline_tick)
    }
}

impl Default for TimerHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TimerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimerHandle")
            .field("generation", &self.generation())
            .field("state", &self.state())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{TimerHandle, TimerInner, TimerState};
    use crate::timers::context::{TimerWheelConfig, TimerWheelContext, TimerWorkerShared};
    use crate::timers::service::{TimerConfig, TimerService};
    use std::ptr;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    #[test]
    fn timer_state_round_trip() {
        for value in 0u8..=5u8 {
            let state = TimerState::from_u8(value);
            assert_eq!(state.as_u8(), value);
        }
    }

    #[test]
    fn timer_inner_generation_bump() {
        let inner = TimerInner::new();
        assert_eq!(inner.generation(), 0);
        assert_eq!(inner.bump_generation(), 1);
        assert_eq!(inner.bump_generation(), 2);
        assert_eq!(inner.generation(), 2);
    }

    #[test]
    fn timer_handle_exposes_state() {
        let handle = TimerHandle::new();
        assert_eq!(handle.state(), TimerState::Idle);
        handle
            .inner()
            .store_state(TimerState::Scheduled, Ordering::Release);
        assert_eq!(handle.state(), TimerState::Scheduled);
    }

    #[test]
    fn timer_handle_cancel_updates_service_and_garbage() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = TimerWheelContext::new(
            0,
            Arc::clone(&shared),
            Arc::clone(&inbox),
            TimerWheelConfig::new(8, 4),
        );
        let garbage = ctx.garbage_shared();
        let _token = service.register_worker(Arc::clone(&shared), Arc::clone(&inbox), garbage);

        let handle = TimerHandle::new();
        ctx.schedule_handle(handle.inner(), 1);

        assert!(handle.cancel(&service));
        assert!(service.cancellation_count() >= 1);
        assert!(ctx.garbage().total() >= 1);

        service.shutdown();
    }

    #[test]
    fn timer_cancel_clears_active_task_payload() {
        let service = TimerService::start(TimerConfig::default());
        let handle = TimerHandle::new();
        let payload = Box::into_raw(Box::new(123u32)) as *mut ();
        handle.inner().swap_payload(payload, Ordering::Release);
        handle.inner().set_home_worker(Some(0));
        handle.inner().set_stripe_hint(1);

        assert!(handle.cancel(&service));
        assert!(handle.inner().payload().is_null());

        unsafe {
            drop(Box::from_raw(payload as *mut u32));
        }
        service.shutdown();
    }

    #[test]
    fn timer_handle_cancel_is_idempotent() {
        let service = TimerService::start(TimerConfig::default());
        let handle = TimerHandle::new();

        assert!(handle.cancel(&service));
        let after_first = service.cancellation_count();
        assert_eq!(handle.state(), TimerState::Cancelled);

        assert!(!handle.cancel(&service));
        assert_eq!(service.cancellation_count(), after_first);
        assert_eq!(handle.state(), TimerState::Cancelled);

        service.shutdown();
    }

    #[test]
    fn timer_handle_swap_payload_roundtrip() {
        let handle = TimerHandle::new();
        let first = Box::into_raw(Box::new(7u32)) as *mut ();
        let second = Box::into_raw(Box::new(11u32)) as *mut ();

        let prev = handle.inner().swap_payload(first, Ordering::AcqRel);
        assert!(prev.is_null());

        let prev = handle.inner().swap_payload(second, Ordering::AcqRel);
        assert_eq!(prev, first);

        let cleared = handle
            .inner()
            .swap_payload(ptr::null_mut(), Ordering::AcqRel);
        assert_eq!(cleared, second);
        assert!(handle.inner().payload().is_null());

        unsafe {
            drop(Box::from_raw(first as *mut u32));
            drop(Box::from_raw(second as *mut u32));
        }
    }

    #[test]
    fn timer_handle_cancel_from_migrating_state() {
        let service = TimerService::start(TimerConfig::default());
        let handle = TimerHandle::new();
        handle
            .inner()
            .store_state(TimerState::Migrating, Ordering::Release);
        handle.inner().set_home_worker(Some(0));
        handle.inner().set_stripe_hint(2);

        assert!(handle.cancel(&service));
        assert_eq!(handle.state(), TimerState::Cancelled);

        service.shutdown();
    }
}
