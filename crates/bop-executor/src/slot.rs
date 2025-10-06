use std::mem;
use std::ptr::null_mut;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU64, AtomicUsize, Ordering};

use crate::signal::{Signal, SignalSetResult};
use crate::waker::Waker;

pub type SlotWorker = unsafe fn(*mut ()) -> SlotState;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    Idle = 0,
    Scheduled = 1,
    Executing = 2,
}

impl SlotState {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Idle,
            1 => Self::Scheduled,
            2 => Self::Executing,
            _ => Self::Idle,
        }
    }
}

#[allow(dead_code)]
pub struct Slot {
    waker: Arc<Waker>,
    signal: Signal,
    index: u64,
    flags: AtomicU8,
    _pad: [u8; 63],
    cpu_time: AtomicU64,
    cpu_time_overhead: AtomicU64,
    counter: AtomicU64,
    _pad2: [u8; 56],
    contention: AtomicU64,
    cpu_time_enabled: AtomicBool,
    data: AtomicPtr<()>,
    // worker: AtomicUsize,
    worker: SlotWorker,
}

unsafe fn reschedule(_: *mut ()) -> SlotState {
    SlotState::Scheduled
}

impl Slot {
    pub fn new(waker: Arc<Waker>, signal: Signal, index: u64) -> Self {
        Self {
            waker,
            signal,
            index,
            flags: AtomicU8::new(SlotState::Idle as u8),
            _pad: [0; 63],
            cpu_time: AtomicU64::new(0),
            cpu_time_overhead: AtomicU64::new(0),
            counter: AtomicU64::new(0),
            _pad2: [0; 56],
            contention: AtomicU64::new(0),
            cpu_time_enabled: AtomicBool::new(false),
            data: AtomicPtr::new(null_mut()),
            // worker: AtomicUsize::new(0),
            worker: reschedule,
        }
    }

    #[inline(always)]
    pub fn count(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn count_reset(&self) -> u64 {
        self.counter.swap(0u64, Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn schedule(&self) -> bool {
        if (self.flags.load(Ordering::Acquire) & SlotState::Scheduled as u8)
            != SlotState::Idle as u8
        {
            return false;
        }

        let previous_flags = self
            .flags
            .fetch_or(SlotState::Scheduled as u8, Ordering::Relaxed);
        let scheduled_nor_executing = (previous_flags
            & ((SlotState::Scheduled as u8) | (SlotState::Executing as u8)))
            == SlotState::Idle as u8;

        if scheduled_nor_executing {
            let SignalSetResult { was_empty, was_set } = self.signal.set(self.index);
            if was_empty && was_set {
                // self.waker.increment();
            }
            true
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn execute(&self) -> SlotState {
        self.flags
            .store(SlotState::Executing as u8, Ordering::Release);
        self.counter.fetch_add(1, Ordering::Relaxed);

        // let data_ptr = self.data.load(Ordering::Acquire);
        // let result = match self.worker() {
        //     Some(worker) => unsafe { worker(std::ptr::null_mut()) },
        //     None => SlotState::Idle,
        // };
        // let result = unsafe { reschedule(std::ptr::null_mut()) };
        let result = SlotState::Scheduled;

        if result == SlotState::Scheduled {
            self.flags
                .store(SlotState::Scheduled as u8, Ordering::Release);
            let SignalSetResult { was_empty, was_set } = self.signal.set(self.index);
            if was_empty && was_set {
                // self.waker.increment();
            }
        } else {
            let after_flags = self
                .flags
                .fetch_sub(SlotState::Executing as u8, Ordering::AcqRel);
            if (after_flags & (SlotState::Scheduled as u8)) != SlotState::Idle as u8 {
                self.signal.set(self.index);
            }
        }

        result
    }

    pub fn set_worker(&self, worker: SlotWorker, data: *mut ()) {
        debug_assert_eq!(mem::size_of::<SlotWorker>(), mem::size_of::<usize>());
        // self.worker.store(worker as usize, Ordering::Release);
        self.data.store(data, Ordering::Release);
    }

    pub fn clear_worker(&self) {
        // self.worker.store(0, Ordering::Release);
        self.data.store(null_mut(), Ordering::Release);
    }

    pub fn flags(&self) -> SlotState {
        SlotState::from_u8(self.flags.load(Ordering::Acquire))
    }

    pub fn index(&self) -> u64 {
        self.index
    }

    pub fn signal(&self) -> &Signal {
        &self.signal
    }

    pub fn waker(&self) -> &Arc<Waker> {
        &self.waker
    }

    pub fn contention(&self) -> u64 {
        self.contention.load(Ordering::Relaxed)
    }

    // #[inline(always)]
    // fn worker(&self) -> Option<SlotWorker> {
    //     let ptr = self.worker.load(Ordering::Acquire);
    //     if ptr == 0 {
    //         None
    //     } else {
    //         Some(unsafe { mem::transmute::<usize, SlotWorker>(ptr) })
    //     }
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    unsafe fn noop(_: *mut ()) -> SlotState {
        SlotState::Idle
    }

    #[test]
    fn schedule_sets_signal_and_wakes() {
        let waker = Arc::new(Waker::new());
        let signal = Signal::new();
        let slot = Slot::new(waker.clone(), signal.clone(), 0);

        slot.set_worker(noop, std::ptr::null_mut());
        assert!(slot.schedule());
        assert!(signal.is_set(0));
        assert_eq!(waker.load(), 1);
    }

    #[test]
    fn execute_runs_worker() {
        let waker = Arc::new(Waker::new());
        let signal = Signal::new();
        let slot = Slot::new(waker, signal.clone(), 0);

        slot.set_worker(noop, std::ptr::null_mut());
        assert!(slot.schedule());
        assert!(signal.acquire(0));

        let state = slot.execute();
        assert_eq!(state, SlotState::Idle);
        assert!(!signal.is_set(0));
    }
}
