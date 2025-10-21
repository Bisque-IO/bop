use crate::PopError;
use crate::mpsc;
use crate::mpsc::Receiver;
use crate::task::{FutureHelpers, MmapExecutorArena, Task, TaskSlot};
use crate::timer::{Timer, TimerHandle, TimerState};
use crate::timer_wheel::TimerWheel;
use crate::worker_message::{MessageSlot, TimerBatch, TimerSchedule, WorkerMessage};
use crate::worker_service::{WorkerRegistration, WorkerService};
use std::cell::Cell;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_QUEUE_SEG_SHIFT: usize = 7;
const DEFAULT_QUEUE_NUM_SEGS_SHIFT: usize = 10;
const DEFAULT_TICK_DURATION_NS: u64 = 1 << 20; // ~1.05ms, power of two as required by TimerWheel
const TIMER_TICKS_PER_WHEEL: usize = 1024;
const TIMER_EXPIRE_BUDGET: usize = 256;

thread_local! {
    static CURRENT_WORKER: Cell<*mut Worker> = Cell::new(ptr::null_mut());
    static CURRENT_TASK: Cell<*const Task> = Cell::new(ptr::null());
}

#[derive(Clone, Copy, Debug)]
pub enum WaitStrategy {
    Spin,
    Yield,
    Sleep(Duration),
}

impl WaitStrategy {
    fn wait(self) {
        match self {
            WaitStrategy::Spin => std::hint::spin_loop(),
            WaitStrategy::Yield => thread::yield_now(),
            WaitStrategy::Sleep(duration) => thread::sleep(duration),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WorkerStats {
    pub polls: u64,
    pub completed_count: u64,
    pub timer_fires: u64,
}

#[derive(Clone, Copy)]
struct TimerEntry {
    timer: NonNull<Timer>,
    deadline_ns: u64,
}

impl TimerEntry {
    fn new(timer: NonNull<Timer>, deadline_ns: u64) -> Self {
        Self { timer, deadline_ns }
    }
}

unsafe impl Send for TimerEntry {}
unsafe impl Sync for TimerEntry {}

pub struct Worker {
    arena: Arc<MmapExecutorArena>,
    service: Option<Arc<WorkerService>>,
    shutdown: Arc<AtomicBool>,
    wait_strategy: WaitStrategy,
    sender: mpsc::Sender<MessageSlot>,
    receiver: Receiver<MessageSlot>,
    now_ns: Arc<AtomicU64>,
    worker_id: u32,
    timer_resolution_ns: u64,
    timer_wheel: TimerWheel<TimerEntry>,
    timer_output: Vec<(u64, u64, TimerEntry)>,
    stats: WorkerStats,
}

impl Worker {
    pub fn new(arena: Arc<MmapExecutorArena>, service: Option<Arc<WorkerService>>) -> Self {
        Self::with_shutdown(arena, service, Arc::new(AtomicBool::new(false)))
    }

    pub fn with_shutdown(
        arena: Arc<MmapExecutorArena>,
        service: Option<Arc<WorkerService>>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        let registration = service
            .as_ref()
            .map(|svc| svc.register_worker())
            .unwrap_or_else(|| WorkerRegistration::standalone());

        let timer_resolution_ns = registration.tick_duration_ns.max(1);
        let timer_wheel = TimerWheel::new(
            Instant::now(),
            Duration::from_nanos(timer_resolution_ns),
            TIMER_TICKS_PER_WHEEL,
        );

        Self {
            arena,
            service,
            shutdown,
            wait_strategy: WaitStrategy::Yield,
            sender: registration.sender,
            receiver: registration.receiver,
            now_ns: registration.now_ns,
            worker_id: registration.worker_id,
            timer_resolution_ns,
            timer_wheel,
            timer_output: Vec::with_capacity(TIMER_EXPIRE_BUDGET),
            stats: WorkerStats::default(),
        }
    }

    #[inline]
    pub fn sender(&self) -> mpsc::Sender<MessageSlot> {
        self.sender.clone()
    }

    #[inline]
    pub fn stats(&self) -> WorkerStats {
        self.stats
    }

    #[inline]
    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    pub fn set_wait_strategy(&mut self, strategy: WaitStrategy) {
        self.wait_strategy = strategy;
    }

    pub fn run_until_idle(&mut self) {
        for _ in 0..1024 {
            if !self.run_once() {
                break;
            }
        }
    }

    pub fn run(&mut self) {
        while !self.shutdown.load(Ordering::Acquire) {
            let progress = self.run_once();
            if !progress {
                self.wait_strategy.wait();
            }
        }
    }

    pub fn run_once(&mut self) -> bool {
        let mut progress = false;
        progress |= self.process_messages();
        progress |= self.poll_timers();
        progress |= self.poll_tasks();
        progress
    }

    fn process_messages(&mut self) -> bool {
        let mut progress = false;
        loop {
            match self.receiver.try_pop() {
                Ok(slot) => {
                    let message = unsafe { slot.into_message() };
                    if self.handle_message(message) {
                        progress = true;
                    }
                }
                Err(PopError::Empty) | Err(PopError::Timeout) => break,
                Err(PopError::Closed) => break,
            }
        }
        progress
    }

    fn handle_message(&mut self, message: WorkerMessage) -> bool {
        match message {
            WorkerMessage::ScheduleTimer { timer } => self.handle_timer_schedule(timer),
            WorkerMessage::ScheduleBatch { timers } => {
                let mut scheduled = false;
                for timer in timers.into_vec() {
                    scheduled |= self.handle_timer_schedule(timer);
                }
                scheduled
            }
            WorkerMessage::CancelTimer { timer } => {
                let timer_ref = unsafe { timer.as_ref() };
                timer_ref.cancel()
            }
            WorkerMessage::Shutdown => {
                self.shutdown.store(true, Ordering::Release);
                true
            }
        }
    }

    fn handle_timer_schedule(&mut self, timer: TimerSchedule) -> bool {
        let (timer_ptr, deadline_ns) = timer.into_parts();
        let timer_ref = unsafe { timer_ptr.as_ref() };
        if timer_ref.state() == TimerState::Cancelled {
            return false;
        }
        self.enqueue_timer_entry(deadline_ns, TimerEntry::new(timer_ptr, deadline_ns))
    }

    fn enqueue_timer_entry(&mut self, deadline_ns: u64, entry: TimerEntry) -> bool {
        self.timer_wheel
            .schedule_timer(deadline_ns, entry)
            .is_some()
    }

    fn poll_timers(&mut self) -> bool {
        let now_ns = self.now_ns.load(Ordering::Acquire);
        let expired = self
            .timer_wheel
            .poll(now_ns, TIMER_EXPIRE_BUDGET, &mut self.timer_output);
        if expired == 0 {
            return false;
        }

        let mut progress = false;
        for idx in 0..expired {
            let (_, deadline_ns, entry) = self.timer_output[idx];
            if self.process_timer_entry(deadline_ns, entry) {
                progress = true;
            }
        }
        progress
    }

    fn process_timer_entry(&mut self, deadline_ns: u64, entry: TimerEntry) -> bool {
        let timer = unsafe { entry.timer.as_ref() };
        let Some(task_ptr) = timer.take_scheduled_task(deadline_ns) else {
            return false;
        };

        let task = unsafe { &*(task_ptr as *mut Task) };
        task.schedule();
        self.stats.timer_fires = self.stats.timer_fires.saturating_add(1);

        let interval_ns = timer.interval_ns();
        if interval_ns > 0 {
            let next_deadline = deadline_ns.saturating_add(interval_ns);
            timer.prepare_schedule(self.worker_id, task_ptr, next_deadline, Some(interval_ns));
            let _ = self
                .enqueue_timer_entry(next_deadline, TimerEntry::new(entry.timer, next_deadline));
        }

        true
    }

    fn poll_tasks(&mut self) -> bool {
        let mut progress = false;
        let leaf_count = self.arena.leaf_count();
        let signals_per_leaf = self.arena.signals_per_leaf();

        for leaf_idx in 0..leaf_count {
            let summary = self.arena.active_summary(leaf_idx).load(Ordering::Acquire);
            if summary == 0 {
                continue;
            }

            for signal_idx in 0..signals_per_leaf {
                let mask = 1u64 << signal_idx;
                if summary & mask == 0 {
                    continue;
                }

                let signal = unsafe { &*self.arena.task_signal_ptr(leaf_idx, signal_idx) };
                loop {
                    let Some((bit_idx, remaining)) = signal.try_acquire_from(0) else {
                        break;
                    };
                    let slot_idx = signal_idx * 64 + bit_idx as usize;
                    if slot_idx >= self.arena.tasks_per_leaf() {
                        break;
                    }

                    let task_ptr = {
                        let arena = &self.arena;
                        unsafe { arena.task(leaf_idx, slot_idx) as *const Task }
                    };
                    let task = unsafe { &*task_ptr };
                    if self.poll_single_task(task) {
                        progress = true;
                    }

                    if remaining == 0 {
                        self.arena
                            .active_tree()
                            .mark_signal_inactive(leaf_idx, signal_idx);
                        break;
                    }
                }
            }
        }

        progress
    }

    fn poll_single_task(&mut self, task: &Task) -> bool {
        let Some(slot) = task.slot() else {
            return false;
        };

        self.stats.polls = self.stats.polls.saturating_add(1);

        task.begin();
        CURRENT_WORKER.with(|cell| cell.set(self as *mut Worker));
        CURRENT_TASK.with(|cell| cell.set(task as *const Task));

        let waker = unsafe { make_task_waker(slot.as_ref()) };
        let mut cx = Context::from_waker(&waker);
        let poll_result = unsafe { task.poll_future(&mut cx) };

        CURRENT_TASK.with(|cell| cell.set(ptr::null()));
        CURRENT_WORKER.with(|cell| cell.set(ptr::null_mut()));

        match poll_result {
            Some(Poll::Ready(())) => {
                task.finish();
                if let Some(ptr) = task.take_future() {
                    unsafe { FutureHelpers::drop_boxed(ptr) };
                }
                self.stats.completed_count = self.stats.completed_count.saturating_add(1);
                true
            }
            Some(Poll::Pending) => {
                task.finish();
                true
            }
            None => {
                task.finish();
                false
            }
        }
    }

    fn schedule_timer_for_current_task(
        &mut self,
        timer: &TimerHandle,
        delay: Duration,
    ) -> Option<u64> {
        let task_ptr = CURRENT_TASK.with(|cell| cell.get()) as *mut Task;
        if task_ptr.is_null() {
            return None;
        }

        let delay_ns = delay.as_nanos().min(u128::from(u64::MAX)) as u64;
        let now = self.now_ns.load(Ordering::Acquire);
        let deadline_ns = now.saturating_add(delay_ns);
        let timer_ptr = timer.as_ptr();

        timer.cancel();
        timer
            .inner()
            .prepare_schedule(self.worker_id, task_ptr as *mut (), deadline_ns, None);

        if self.enqueue_timer_entry(deadline_ns, TimerEntry::new(timer_ptr, deadline_ns)) {
            Some(deadline_ns)
        } else {
            timer.cancel();
            None
        }
    }
}

fn make_task_waker(slot: &TaskSlot) -> Waker {
    let ptr = slot as *const TaskSlot as *const ();
    unsafe { Waker::from_raw(RawWaker::new(ptr, &TASK_WAKER_VTABLE)) }
}

unsafe fn task_waker_clone(ptr: *const ()) -> RawWaker {
    RawWaker::new(ptr, &TASK_WAKER_VTABLE)
}

unsafe fn task_waker_wake(ptr: *const ()) {
    unsafe {
        let slot = &*(ptr as *const TaskSlot);
        let task_ptr = slot.task_ptr();
        if task_ptr.is_null() {
            return;
        }
        let task = &*task_ptr;
        task.schedule();
    }
}

unsafe fn task_waker_wake_by_ref(ptr: *const ()) {
    unsafe {
        task_waker_wake(ptr);
    }
}

unsafe fn task_waker_drop(_: *const ()) {}

static TASK_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    task_waker_clone,
    task_waker_wake,
    task_waker_wake_by_ref,
    task_waker_drop,
);

/// Schedules a timer for the task currently being polled by the active worker.
pub fn schedule_timer_for_current_task(
    _cx: &Context<'_>,
    timer: &TimerHandle,
    delay: Duration,
) -> Option<u64> {
    CURRENT_WORKER.with(|cell| {
        let worker_ptr = cell.get();
        if worker_ptr.is_null() {
            return None;
        }
        unsafe { (&mut *worker_ptr).schedule_timer_for_current_task(timer, delay) }
    })
}

impl WorkerRegistration {
    fn standalone() -> Self {
        let (sender, receiver) = mpsc::new(DEFAULT_QUEUE_SEG_SHIFT, DEFAULT_QUEUE_NUM_SEGS_SHIFT);
        Self {
            worker_id: 0,
            sender,
            receiver,
            now_ns: Arc::new(AtomicU64::new(0)),
            tick_duration_ns: DEFAULT_TICK_DURATION_NS,
        }
    }
}
