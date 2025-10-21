use crate::timer::Timer;
use crate::{PushError, mpsc};
use std::ptr::NonNull;

/// Remote scheduling request for a timer.
#[derive(Clone, Copy, Debug)]
pub struct TimerSchedule {
    timer: NonNull<Timer>,
    deadline_ns: u64,
}

impl TimerSchedule {
    pub fn new(timer: NonNull<Timer>, deadline_ns: u64) -> Self {
        Self { timer, deadline_ns }
    }

    #[inline(always)]
    pub fn timer(&self) -> NonNull<Timer> {
        self.timer
    }

    #[inline(always)]
    pub fn deadline_ns(&self) -> u64 {
        self.deadline_ns
    }

    #[inline(always)]
    pub fn into_parts(self) -> (NonNull<Timer>, u64) {
        (self.timer, self.deadline_ns)
    }
}

unsafe impl Send for TimerSchedule {}
unsafe impl Sync for TimerSchedule {}

/// Batch of scheduling requests.
#[derive(Debug, Clone)]
pub struct TimerBatch {
    entries: Box<[TimerSchedule]>,
}

impl TimerBatch {
    pub fn new(entries: Vec<TimerSchedule>) -> Self {
        Self {
            entries: entries.into_boxed_slice(),
        }
    }

    pub fn empty() -> Self {
        Self {
            entries: Box::new([]),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &TimerSchedule> {
        self.entries.iter()
    }

    pub fn into_vec(self) -> Vec<TimerSchedule> {
        self.entries.into_vec()
    }
}

/// Worker control-plane messages.
#[derive(Clone, Debug)]
pub enum WorkerMessage {
    ScheduleTimer { timer: TimerSchedule },
    ScheduleBatch { timers: TimerBatch },
    CancelTimer { timer: NonNull<Timer> },
    Shutdown,
}

#[derive(Clone, Copy)]
pub struct MessageSlot(*mut WorkerMessage);

impl MessageSlot {
    #[inline]
    pub fn from_message(message: WorkerMessage) -> Self {
        Self(Box::into_raw(Box::new(message)))
    }

    #[inline]
    pub unsafe fn into_message(self) -> WorkerMessage {
        unsafe { *Box::from_raw(self.0) }
    }

    #[inline]
    pub unsafe fn drop_slot(self) {
        unsafe { drop(Box::from_raw(self.0)) };
    }
}

unsafe impl Send for MessageSlot {}
unsafe impl Sync for MessageSlot {}

pub fn push_message(
    sender: &mut mpsc::Sender<MessageSlot>,
    message: WorkerMessage,
) -> Result<(), WorkerMessage> {
    let slot = MessageSlot::from_message(message);
    match sender.try_push(slot) {
        Ok(()) => Ok(()),
        Err(PushError::Full(returned)) => match sender.push_spin(returned) {
            Ok(()) => Ok(()),
            Err(PushError::Full(returned)) | Err(PushError::Closed(returned)) => unsafe {
                Err(returned.into_message())
            },
        },
        Err(PushError::Closed(returned)) => unsafe { Err(returned.into_message()) },
    }
}
