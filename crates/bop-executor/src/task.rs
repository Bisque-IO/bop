use crate::bits;
use crate::summary_tree::{SummaryInit, SummaryTree};
use crate::timer::TimerHandle;
use std::cell::UnsafeCell;
use std::fmt;
use std::future::{Future, IntoFuture};
use std::io;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU64, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{Duration, Instant};

#[cfg(unix)]
use libc::{MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, PROT_READ, PROT_WRITE, mmap, munmap};

#[cfg(target_os = "linux")]
use libc::{MAP_HUGE_2MB, MAP_HUGETLB};

#[cfg(windows)]
use winapi::um::memoryapi::{VirtualAlloc, VirtualFree};
#[cfg(windows)]
use winapi::um::winnt::{MEM_COMMIT, MEM_LARGE_PAGES, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE};

pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

pub const TASK_IDLE: u8 = 0;
pub const TASK_SCHEDULED: u8 = 1;
pub const TASK_EXECUTING: u8 = 2;

#[derive(Clone, Copy, Debug)]
pub struct ArenaOptions {
    pub use_huge_pages: bool,
    pub preinitialize_tasks: bool,
}

impl Default for ArenaOptions {
    fn default() -> Self {
        Self {
            use_huge_pages: false,
            preinitialize_tasks: false,
        }
    }
}

impl ArenaOptions {
    pub fn with_preinitialized_tasks(mut self, enabled: bool) -> Self {
        self.preinitialize_tasks = enabled;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArenaConfig {
    pub leaf_count: usize,
    pub tasks_per_leaf: usize,
    pub max_workers: usize,
    pub yield_bit_index: u32,
}

impl ArenaConfig {
    pub fn new(leaf_count: usize, tasks_per_leaf: usize) -> io::Result<Self> {
        let leaf_count = if !leaf_count.is_power_of_two() {
            leaf_count.next_power_of_two()
        } else {
            leaf_count
        };
        // let tasks_per_leaf = if !tasks_per_leaf.is_power_of_two() {
        //     tasks_per_leaf.next_power_of_two()
        // } else {
        //     tasks_per_leaf
        // };
        if leaf_count == 0 || tasks_per_leaf == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "leaf_count and tasks_per_leaf must be > 0",
            ));
        }
        Ok(Self {
            leaf_count,
            tasks_per_leaf,
            max_workers: leaf_count,
            yield_bit_index: 63,
        })
    }

    pub fn with_max_workers(mut self, workers: usize) -> Self {
        self.max_workers = workers.max(1).min(self.leaf_count);
        self
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct TaskSignal {
    value: AtomicU64,
}

impl TaskSignal {
    pub const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn load(&self, ordering: Ordering) -> u64 {
        self.value.load(ordering)
    }

    #[inline(always)]
    pub fn is_set(&self, bit_index: u32) -> bool {
        let mask = 1u64 << bit_index;
        (self.value.load(Ordering::Relaxed) & mask) != 0
    }

    #[inline(always)]
    pub fn set(&self, bit_index: u32) -> (bool, bool) {
        let mask = 1u64 << bit_index;
        let prev = self.value.fetch_or(mask, Ordering::AcqRel);
        // was empty; was_set
        (prev == 0, (prev & mask) == 0)
    }

    #[inline]
    pub fn clear(&self, bit_index: u32) -> (u64, bool) {
        let mask = 1u64 << bit_index;
        let previous = self.value.fetch_and(!mask, Ordering::AcqRel);
        let remaining = previous & !mask;
        (remaining, remaining == 0)
    }

    #[inline]
    pub fn try_acquire(&self, bit_index: u32) -> (u64, bool) {
        if !self.is_set(bit_index) {
            return (0, false);
        }
        let mask = 1u64 << bit_index;
        let (_, previous, acquired) = bits::try_acquire(&self.value, bit_index as u64);
        let remaining = previous & !mask;
        (remaining, acquired)
    }

    #[inline]
    pub fn try_acquire_from(&self, start_bit: u32) -> Option<(u32, u64)> {
        let start = (start_bit as u64).min(63);
        for _ in 0..64 {
            let current = self.value.load(Ordering::Acquire);
            if current == 0 {
                return None;
            }

            let candidate = bits::find_nearest(current, start);
            let bit_index = if candidate < 64 {
                candidate as u32
            } else {
                current.trailing_zeros()
            };

            let (bit_mask, previous, acquired) = bits::try_acquire(&self.value, bit_index as u64);
            if !acquired {
                std::hint::spin_loop();
                continue;
            }

            let remaining = previous & !bit_mask;
            return Some((bit_index, remaining));
        }
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TaskHandle(NonNull<Task>);

impl TaskHandle {
    #[inline(always)]
    pub fn from_task(task: &Task) -> Self {
        TaskHandle(NonNull::from(task))
    }

    #[inline(always)]
    pub fn from_non_null(task: NonNull<Task>) -> Self {
        TaskHandle(task)
    }

    #[inline(always)]
    pub fn as_ptr(&self) -> *mut Task {
        self.0.as_ptr()
    }

    #[inline(always)]
    pub fn as_non_null(&self) -> NonNull<Task> {
        self.0
    }

    #[inline(always)]
    pub fn task(&self) -> &Task {
        unsafe { self.0.as_ref() }
    }

    #[inline(always)]
    pub fn leaf_idx(&self) -> usize {
        self.task().leaf_idx as usize
    }

    #[inline(always)]
    pub fn signal_idx(&self) -> usize {
        self.task().signal_idx as usize
    }

    #[inline(always)]
    pub fn bit_idx(&self) -> u32 {
        self.task().signal_bit
    }

    #[inline(always)]
    pub fn slot_index(&self) -> usize {
        self.task().slot_idx as usize
    }

    #[inline(always)]
    pub fn global_id(&self, _tasks_per_leaf: usize) -> u64 {
        self.task().global_id()
    }
}

unsafe impl Send for TaskHandle {}
unsafe impl Sync for TaskHandle {}

#[repr(C)]
pub struct TaskSlot {
    task_ptr: AtomicPtr<Task>,
    active_task_ptr: AtomicPtr<Task>,
}

impl TaskSlot {
    #[inline(always)]
    pub fn new(task_ptr: *mut Task) -> Self {
        Self {
            task_ptr: AtomicPtr::new(task_ptr),
            active_task_ptr: AtomicPtr::new(ptr::null_mut()),
        }
    }

    #[inline(always)]
    pub fn task_ptr(&self) -> *mut Task {
        self.task_ptr.load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn set_task_ptr(&self, ptr: *mut Task) {
        self.task_ptr.store(ptr, Ordering::Release);
    }

    #[inline(always)]
    pub fn clear_task_ptr(&self) {
        self.task_ptr.store(ptr::null_mut(), Ordering::Release);
        self.active_task_ptr
            .store(ptr::null_mut(), Ordering::Release);
    }

    #[inline(always)]
    pub fn set_active_task_ptr(&self, ptr: *mut Task) {
        self.active_task_ptr.store(ptr, Ordering::Release);
    }

    #[inline(always)]
    pub fn active_task_ptr(&self) -> *mut Task {
        self.active_task_ptr.load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn clear_active_task_ptr(&self) {
        self.active_task_ptr
            .store(ptr::null_mut(), Ordering::Release);
    }
}

#[derive(Debug)]
struct ArenaLayout {
    root_offset: usize,
    leaf_offset: usize,
    signal_offset: usize,
    reservation_offset: usize,
    worker_bitmap_offset: usize,
    task_slot_offset: usize,
    task_offset: usize,
    total_size: usize,
    signals_per_leaf: usize,
    worker_bitmap_words: usize,
}

impl ArenaLayout {
    fn new(config: &ArenaConfig) -> Self {
        let signals_per_leaf = (config.tasks_per_leaf + 63) / 64;
        let root_count = ((config.leaf_count + 63) / 64).max(1);

        let root_size = root_count * std::mem::size_of::<AtomicU64>();
        let leaf_size = config.leaf_count * std::mem::size_of::<AtomicU64>();
        let signal_size = config.leaf_count * signals_per_leaf * std::mem::size_of::<TaskSignal>();
        let reservation_size =
            config.leaf_count * signals_per_leaf * std::mem::size_of::<AtomicU64>();
        let worker_bitmap_words = (config.max_workers + 63) / 64;
        let worker_bitmap_size = worker_bitmap_words * std::mem::size_of::<AtomicU64>();
        let task_slot_size =
            config.leaf_count * config.tasks_per_leaf * std::mem::size_of::<TaskSlot>();
        let task_size = config.leaf_count * config.tasks_per_leaf * std::mem::size_of::<Task>();

        let mut offset = 0usize;
        let root_offset = offset;
        offset += root_size;
        let leaf_offset = offset;
        offset += leaf_size;
        let signal_offset = offset;
        offset += signal_size;
        let reservation_offset = offset;
        offset += reservation_size;
        let worker_bitmap_offset = offset;
        offset += worker_bitmap_size;
        let task_slot_offset = offset;
        offset += task_slot_size;
        let task_offset = offset;
        offset += task_size;

        Self {
            root_offset,
            leaf_offset,
            signal_offset,
            reservation_offset,
            worker_bitmap_offset,
            task_slot_offset,
            task_offset,
            total_size: offset,
            signals_per_leaf,
            worker_bitmap_words,
        }
    }
}

pub struct FutureHelpers;

impl FutureHelpers {
    pub fn box_future<F>(future: F) -> *mut ()
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let boxed: BoxFuture = Box::pin(future);
        Box::into_raw(Box::new(boxed)) as *mut ()
    }

    pub unsafe fn drop_boxed(ptr: *mut ()) {
        if ptr.is_null() {
            return;
        }
        unsafe {
            drop(Box::from_raw(ptr as *mut BoxFuture));
        }
    }

    pub unsafe fn poll_boxed(ptr: *mut (), cx: &mut Context<'_>) -> Option<Poll<()>> {
        if ptr.is_null() {
            return None;
        }
        unsafe {
            let future = &mut *(ptr as *mut BoxFuture);
            Some(future.as_mut().poll(cx))
        }
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskStats {
    pub polls: u64,
    pub yields: u64,
    pub cpu_time_ns: u64,
}

impl TaskStats {
    #[inline(always)]
    pub fn reset(&mut self) {
        self.polls = 0;
        self.yields = 0;
        self.cpu_time_ns = 0;
    }
}

#[repr(C)]
pub struct Task {
    global_id: u64,
    leaf_idx: u32,
    signal_idx: u32,
    slot_idx: u32,
    signal_bit: u32,
    signal_ptr: *const TaskSignal,
    slot_ptr: AtomicPtr<TaskSlot>,
    arena_ptr: AtomicPtr<MmapExecutorArena>,
    state: AtomicU8,
    yielded: AtomicBool,
    cpu_time_enabled: AtomicBool,
    future_ptr: AtomicPtr<()>,
    // Safety: stats are mutated without synchronization based on executor guarantees that
    // only the owning worker thread records updates while other threads may only clone/copy.
    stats: UnsafeCell<TaskStats>,
}

unsafe impl Send for Task {}
unsafe impl Sync for Task {}

impl Task {
    const WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
        Self::waker_clone,
        Self::waker_wake,
        Self::waker_wake_by_ref,
        Self::waker_drop,
    );

    const WAKER_YIELD_VTABLE: RawWakerVTable = RawWakerVTable::new(
        Self::waker_clone,
        Self::waker_yield_wake,
        Self::waker_yield_wake_by_ref,
        Self::waker_drop,
    );

    unsafe fn construct(
        ptr: *mut Task,
        global_id: u64,
        leaf_idx: u32,
        signal_idx: u32,
        slot_idx: u32,
        signal_bit: u32,
        signal_ptr: *const TaskSignal,
        slot_ptr: *mut TaskSlot,
    ) {
        unsafe {
            ptr::write(
                ptr,
                Task {
                    global_id,
                    leaf_idx,
                    signal_idx,
                    slot_idx,
                    signal_bit,
                    signal_ptr,
                    slot_ptr: AtomicPtr::new(slot_ptr),
                    arena_ptr: AtomicPtr::new(ptr::null_mut()),
                    state: AtomicU8::new(TASK_IDLE),
                    yielded: AtomicBool::new(false),
                    cpu_time_enabled: AtomicBool::new(false),
                    future_ptr: AtomicPtr::new(ptr::null_mut()),
                    stats: UnsafeCell::new(TaskStats::default()),
                },
            );
            (*slot_ptr).set_task_ptr(ptr);
        }
    }

    unsafe fn bind_arena(&self, arena: *const MmapExecutorArena) {
        self.arena_ptr
            .store(arena as *mut MmapExecutorArena, Ordering::Release);
    }

    pub fn global_id(&self) -> u64 {
        self.global_id
    }

    #[inline(always)]
    pub fn stats(&self) -> TaskStats {
        unsafe { *self.stats.get() }
    }

    #[inline(always)]
    pub fn set_cpu_time_tracking(&self, enabled: bool) {
        self.cpu_time_enabled.store(enabled, Ordering::Relaxed);
    }

    #[inline(always)]
    fn record_poll(&self) {
        unsafe {
            let stats = &mut *self.stats.get();
            stats.polls = stats.polls.saturating_add(1);
        }
    }

    #[inline(always)]
    pub(crate) fn record_yield(&self) {
        unsafe {
            let stats = &mut *self.stats.get();
            stats.yields = stats.yields.saturating_add(1);
        }
    }

    #[inline(always)]
    fn record_cpu_time(&self, duration: Duration) {
        let nanos = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
        unsafe {
            let stats = &mut *self.stats.get();
            stats.cpu_time_ns = stats.cpu_time_ns.saturating_add(nanos);
        }
    }

    #[inline(always)]
    pub fn slot(&self) -> Option<NonNull<TaskSlot>> {
        NonNull::new(self.slot_ptr.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub fn clear_slot(&self) {
        self.slot_ptr.store(ptr::null_mut(), Ordering::Release);
    }

    /// Attempts to schedule this queue for execution (IDLE -> SCHEDULED transition).
    ///
    /// Called by producers after enqueuing items to notify the executor. Uses atomic
    /// operations to ensure only one successful schedule per work batch.
    ///
    /// # Algorithm
    ///
    /// 1. **Fast check**: If already SCHEDULED, return false immediately (idempotent)
    /// 2. **Atomic set**: `fetch_or(SCHEDULED)` to set the SCHEDULED flag
    /// 3. **State check**: If previous state was IDLE (neither SCHEDULED nor EXECUTING):
    ///    - Set bit in signal word via `signal.set(bit_index)`
    ///    - If signal transitioned from empty, update summary via `waker.mark_active()`
    ///    - Return true (successful schedule)
    /// 4. **Otherwise**: Return false (already scheduled or executing)
    ///
    /// # Returns
    ///
    /// - `true`: Successfully transitioned from IDLE to SCHEDULED (work will be processed)
    /// - `false`: Already scheduled/executing, or concurrent schedule won (idempotent)
    ///
    /// # Concurrent Behavior
    ///
    /// - **Multiple producers**: Only the first `schedule()` succeeds (returns true)
    /// - **During EXECUTING**: Sets SCHEDULED flag, which `finish()` will detect and reschedule
    ///
    /// # Memory Ordering
    ///
    /// - Initial load: `Acquire` (see latest state)
    /// - `fetch_or`: `Release` (publish enqueued items to executor)
    ///
    /// # Performance
    ///
    /// - **Already scheduled**: ~2-3 ns (fast path, single atomic load)
    /// - **Successful schedule**: ~10-20 ns (fetch_or + signal update + potential summary update)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Producer 1
    /// queue.try_push(item)?;
    /// if gate.schedule() {
    ///     println!("Successfully scheduled");  // First producer
    /// }
    ///
    /// // Producer 2 (concurrent)
    /// queue.try_push(another_item)?;
    /// if !gate.schedule() {
    ///     println!("Already scheduled");  // Idempotent, no action needed
    /// }
    /// ```
    #[inline(always)]
    pub fn schedule(&self) {
        if (self.state.load(Ordering::Acquire) & TASK_SCHEDULED) != TASK_IDLE {
            return;
        }

        let previous_flags = self.state.fetch_or(TASK_SCHEDULED, Ordering::Release);
        let scheduled_nor_executing =
            (previous_flags & (TASK_SCHEDULED | TASK_EXECUTING)) == TASK_IDLE;

        if scheduled_nor_executing {
            let signal = unsafe { &*self.signal_ptr };
            let (was_empty, was_set) = signal.set(self.signal_bit);
            if was_empty && was_set {
                let arena_ptr = self.arena_ptr.load(Ordering::Acquire);
                if !arena_ptr.is_null() {
                    unsafe {
                        (*arena_ptr)
                            .active_tree
                            .mark_signal_active(self.leaf_idx as usize, self.signal_idx as usize);
                    }
                }
            }
        }
    }

    /// Marks the queue as EXECUTING (SCHEDULED -> EXECUTING transition).
    ///
    /// Called by the executor when it begins processing this queue. This transition
    /// prevents redundant scheduling while work is being processed.
    ///
    /// # State Transition
    ///
    /// Unconditionally stores EXECUTING, which clears any SCHEDULED flags and sets EXECUTING.
    /// ```text
    /// Before: SCHEDULED (1)
    /// After:  EXECUTING (2)
    /// ```
    ///
    /// If a producer calls `schedule()` after `begin()` but before `finish()`, the
    /// SCHEDULED flag will be set again (creating state 3 = EXECUTING | SCHEDULED),
    /// which `finish()` detects and handles.
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::Release` to ensure the state change is visible to concurrent
    /// producers calling `schedule()`.
    ///
    /// # Performance
    ///
    /// ~1-2 ns (single atomic store)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Executor discovers ready queue
    /// if signal.acquire(queue_bit) {
    ///     gate.begin();  // Mark as executing
    ///     process_queue();
    ///     gate.finish();
    /// }
    /// ```
    #[inline(always)]
    pub fn begin(&self) {
        self.state.store(TASK_EXECUTING, Ordering::Release);
    }

    /// Marks the queue as IDLE and handles concurrent schedules (EXECUTING -> IDLE/SCHEDULED).
    ///
    /// Called by the executor after processing a batch of items. Automatically detects
    /// if new work arrived during processing (SCHEDULED flag set concurrently) and
    /// reschedules if needed.
    ///
    /// # Algorithm
    ///
    /// 1. **Clear EXECUTING**: `fetch_sub(EXECUTING)` atomically transitions to IDLE
    /// 2. **Check SCHEDULED**: If the SCHEDULED flag is set in the result:
    ///    - Means a producer called `schedule()` during execution
    ///    - Re-set the signal bit to ensure executor sees the work
    ///    - Queue remains/becomes SCHEDULED
    ///
    /// # Automatic Rescheduling
    ///
    /// This method implements a key correctness property: if a producer enqueues work
    /// while the executor is processing, that work will not be lost. The SCHEDULED flag
    /// acts as a handoff mechanism.
    ///
    /// ```text
    /// Timeline:
    /// T0: Executor calls begin()           -> EXECUTING (2)
    /// T1: Producer calls schedule()        -> EXECUTING | SCHEDULED (3)
    /// T2: Executor calls finish()          -> SCHEDULED (1) [bit re-set in signal]
    /// T3: Executor sees bit, processes     -> ...
    /// ```
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::AcqRel`:
    /// - **Acquire**: See all producer writes (enqueued items)
    /// - **Release**: Publish state transition to future readers
    ///
    /// # Performance
    ///
    /// - **No concurrent schedule**: ~2-3 ns (fetch_sub only)
    /// - **With concurrent schedule**: ~10-15 ns (fetch_sub + signal.set)
    ///
    /// # Example
    ///
    /// ```ignore
    /// gate.begin();
    /// while let Some(item) = queue.try_pop() {
    ///     process(item);
    /// }
    /// gate.finish();  // Automatically reschedules if more work arrived
    /// ```
    #[inline(always)]
    pub fn finish(&self) {
        let after_flags = self.state.fetch_sub(TASK_EXECUTING, Ordering::AcqRel);
        if after_flags & TASK_SCHEDULED != TASK_IDLE {
            let signal = unsafe { &*self.signal_ptr };
            let (was_empty, was_set) = signal.set(self.signal_bit);
            if was_empty && was_set {
                let arena_ptr = self.arena_ptr.load(Ordering::Relaxed);
                if !arena_ptr.is_null() {
                    unsafe {
                        (*arena_ptr)
                            .active_tree
                            .mark_signal_active(self.leaf_idx as usize, self.signal_idx as usize);
                    }
                }
            }
        }
    }

    /// Atomically marks the queue as SCHEDULED, ensuring re-execution.
    ///
    /// Called by the executor when it knows more work exists but wants to yield the
    /// timeslice for fairness. This is an optimization over `finish()` followed by
    /// external `schedule()`.
    ///
    /// # Use Cases
    ///
    /// 1. **Batch size limiting**: Process N items, then yield to other queues
    /// 2. **Fairness**: Prevent queue starvation by rotating execution
    /// 3. **Latency control**: Ensure all queues get regular timeslices
    ///
    /// # Algorithm
    ///
    /// 1. **Set state**: Store SCHEDULED unconditionally
    /// 2. **Update signal**: Set bit in signal word
    /// 3. **Update summary**: If signal was empty, mark active in waker
    ///
    /// # Comparison with finish() + schedule()
    ///
    /// ```ignore
    /// // Separate calls (2 atomic ops)
    /// gate.finish();      // EXECUTING -> IDLE
    /// gate.schedule();    // IDLE -> SCHEDULED
    ///
    /// // Combined call (1 atomic op + signal update)
    /// gate.finish_and_schedule();  // EXECUTING -> SCHEDULED
    /// ```
    ///
    /// # Memory Ordering
    ///
    /// Uses `Ordering::Release` to publish both the state change and enqueued items.
    ///
    /// # Performance
    ///
    /// ~10-15 ns (store + signal.set + potential summary update)
    ///
    /// # Example
    ///
    /// ```ignore
    /// gate.begin();
    /// let mut processed = 0;
    /// while processed < BATCH_SIZE {
    ///     if let Some(item) = queue.try_pop() {
    ///         process(item);
    ///         processed += 1;
    ///     } else {
    ///         break;
    ///     }
    /// }
    ///
    /// if queue.len() > 0 {
    ///     gate.finish_and_schedule();  // More work, stay scheduled
    /// } else {
    ///     gate.finish();  // Done, go idle
    /// }
    /// ```
    #[inline(always)]
    pub fn finish_and_schedule(&self) {
        self.state.store(TASK_SCHEDULED, Ordering::Release);
        let signal = unsafe { &*self.signal_ptr };
        let (was_empty, was_set) = signal.set(self.signal_bit);
        if was_empty && was_set {
            let arena_ptr = self.arena_ptr.load(Ordering::Relaxed);
            if !arena_ptr.is_null() {
                unsafe {
                    (*arena_ptr)
                        .active_tree
                        .mark_signal_active(self.leaf_idx as usize, self.signal_idx as usize);
                }
            }
        }
    }

    #[inline(always)]
    pub fn clear_yielded(&self) {
        self.yielded.store(false, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn is_yielded(&self) -> bool {
        self.yielded.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub unsafe fn waker_yield(&self) -> Waker {
        let slot_ptr = self.slot_ptr.load(Ordering::Acquire);
        debug_assert!(!slot_ptr.is_null(), "task is missing slot pointer");
        let ptr = slot_ptr as *const ();
        unsafe { Waker::from_raw(RawWaker::new(ptr, &Self::WAKER_YIELD_VTABLE)) }
    }

    #[inline(always)]
    unsafe fn waker_clone(ptr: *const ()) -> RawWaker {
        RawWaker::new(ptr, &Self::WAKER_VTABLE)
    }

    #[inline(always)]
    unsafe fn waker_yield_wake(ptr: *const ()) {
        let slot = unsafe { &*(ptr as *const TaskSlot) };
        let task_ptr = slot.task_ptr();
        if task_ptr.is_null() {
            return;
        }
        let task = unsafe { &*task_ptr };
        task.yielded.store(true, Ordering::Relaxed);
    }

    #[inline(always)]
    unsafe fn waker_yield_wake_by_ref(ptr: *const ()) {
        let slot = unsafe { &*(ptr as *const TaskSlot) };
        let task_ptr = slot.task_ptr();
        if task_ptr.is_null() {
            return;
        }
        let task = unsafe { &*task_ptr };
        task.yielded.store(true, Ordering::Relaxed);
    }

    #[inline(always)]
    unsafe fn waker_wake(ptr: *const ()) {
        let slot = unsafe { &*(ptr as *const TaskSlot) };
        let task_ptr = slot.task_ptr();
        if task_ptr.is_null() {
            return;
        }
        let task = unsafe { &*task_ptr };
        task.schedule();
    }

    #[inline(always)]
    unsafe fn waker_wake_by_ref(ptr: *const ()) {
        let slot = unsafe { &*(ptr as *const TaskSlot) };
        let task_ptr = slot.task_ptr();
        if task_ptr.is_null() {
            return;
        }
        let task = unsafe { &*task_ptr };
        task.schedule();
    }

    #[inline(always)]
    unsafe fn waker_drop(_: *const ()) {}

    #[inline(always)]
    pub unsafe fn poll_future(&self, cx: &mut Context<'_>) -> Option<Poll<()>> {
        let ptr = self.future_ptr.load(Ordering::Acquire);
        if ptr.is_null() {
            return None;
        }
        self.record_poll();
        if self.cpu_time_enabled.load(Ordering::Relaxed) {
            let start = Instant::now();
            let result = unsafe { FutureHelpers::poll_boxed(ptr, cx) };
            self.record_cpu_time(start.elapsed());
            result
        } else {
            unsafe { FutureHelpers::poll_boxed(ptr, cx) }
        }
    }

    #[inline(always)]
    pub fn attach_future(&self, future_ptr: *mut ()) -> Result<(), *mut ()> {
        self.future_ptr
            .compare_exchange(
                ptr::null_mut(),
                future_ptr,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|existing| existing)
    }

    #[inline(always)]
    pub fn take_future(&self) -> Option<*mut ()> {
        let ptr = self.future_ptr.swap(ptr::null_mut(), Ordering::AcqRel);
        if ptr.is_null() { None } else { Some(ptr) }
    }

    // pub unsafe fn reset(&self) {
    //     self.state.store(TASK_IDLE, Ordering::Relaxed);
    //     self.yielded.store(false, Ordering::Relaxed);
    //     self.future_ptr.store(ptr::null_mut(), Ordering::Relaxed);
    // }

    #[inline(always)]
    pub unsafe fn reset(
        &mut self,
        global_id: u64,
        leaf_idx: u32,
        signal_idx: u32,
        slot_idx: u32,
        signal_bit: u32,
        signal_ptr: *const TaskSignal,
        slot_ptr: *mut TaskSlot,
    ) {
        self.global_id = global_id;
        self.leaf_idx = leaf_idx;
        self.signal_idx = signal_idx;
        self.slot_idx = slot_idx;
        self.signal_bit = signal_bit;
        self.signal_ptr = signal_ptr;
        self.slot_ptr.store(slot_ptr, Ordering::Release);
        let slot = unsafe { &*slot_ptr };
        slot.set_task_ptr(self as *mut Task);
        self.state.store(TASK_IDLE, Ordering::Relaxed);
        self.yielded.store(false, Ordering::Relaxed);
        self.cpu_time_enabled.store(false, Ordering::Relaxed);
        self.future_ptr.store(ptr::null_mut(), Ordering::Relaxed);
        unsafe {
            let stats = &mut *self.stats.get();
            stats.reset();
        }
    }
}

pub struct MmapExecutorArena {
    memory: NonNull<u8>,
    size: usize,
    config: ArenaConfig,
    layout: ArenaLayout,
    active_tree: SummaryTree,
    total_tasks: AtomicU64,
    is_closed: AtomicBool,
}

/// Errors that can occur when spawning a new task into the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    /// The arena is closed and no longer accepts new tasks.
    Closed,
    /// All task slots are currently in use.
    NoCapacity,
    /// The reserved task slot already had an attached future.
    AttachFailed,
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpawnError::Closed => write!(f, "executor arena is closed"),
            SpawnError::NoCapacity => write!(f, "no task slots available"),
            SpawnError::AttachFailed => write!(f, "task slot already has a future attached"),
        }
    }
}

impl std::error::Error for SpawnError {}

unsafe impl Send for MmapExecutorArena {}
unsafe impl Sync for MmapExecutorArena {}

impl MmapExecutorArena {
    pub fn with_config(config: ArenaConfig, options: ArenaOptions) -> io::Result<Self> {
        let layout = ArenaLayout::new(&config);
        let memory_ptr = Self::allocate_memory(layout.total_size, &options)?;
        let memory = NonNull::new(memory_ptr)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "allocation returned null"))?;

        if options.preinitialize_tasks {
            unsafe {
                ptr::write_bytes(memory.as_ptr(), 0, layout.total_size);
            }
        }

        let init = SummaryInit {
            root_words: unsafe { memory.as_ptr().add(layout.root_offset) } as *const AtomicU64,
            root_count: ((config.leaf_count + 63) / 64).max(1),
            leaf_words: unsafe { memory.as_ptr().add(layout.leaf_offset) } as *const AtomicU64,
            leaf_count: config.leaf_count,
            signals_per_leaf: layout.signals_per_leaf,
            task_reservations: unsafe {
                memory.as_ptr().add(layout.reservation_offset) as *const AtomicU64
            },
            worker_bitmap: unsafe {
                memory.as_ptr().add(layout.worker_bitmap_offset) as *const AtomicU64
            },
            worker_bitmap_words: layout.worker_bitmap_words,
            max_workers: config.max_workers,
            yield_bit_index: config.yield_bit_index,
        };

        let active_tree = unsafe { SummaryTree::new(init) };

        let arena = MmapExecutorArena {
            memory,
            size: layout.total_size,
            config,
            layout,
            active_tree,
            total_tasks: AtomicU64::new(0),
            is_closed: AtomicBool::new(false),
        };

        arena.initialize_task_slots();
        if options.preinitialize_tasks {
            arena.initialize_tasks();
        }
        Ok(arena)
    }

    pub fn new(leaf_count: usize, tasks_per_leaf: usize) -> io::Result<Self> {
        Self::with_config(
            ArenaConfig::new(leaf_count, tasks_per_leaf)?,
            ArenaOptions::default(),
        )
    }

    #[inline]
    pub fn is_closed(&self) -> bool {
        self.is_closed.load(Ordering::Acquire)
    }

    #[inline]
    pub fn close(&self) {
        self.is_closed.store(true, Ordering::Release);
    }

    /// Spawns a new asynchronous task onto the arena.
    ///
    /// The returned [`TaskHandle`] remains reserved until
    /// [`MmapExecutorArena::release_task`] is called. Callers should release
    /// the handle once the future has completed so the slot can be reused.
    ///
    /// # Errors
    ///
    /// Returns [`SpawnError::Closed`] if the arena has been closed, or
    /// [`SpawnError::NoCapacity`] if all task slots are currently reserved.
    /// [`SpawnError::AttachFailed`] is returned if the reserved task already has
    /// a future attached (which should not happen under normal usage).
    pub fn spawn<F>(&self, future: F) -> Result<TaskHandle, SpawnError>
    where
        F: IntoFuture<Output = ()>,
        F::IntoFuture: Future<Output = ()> + Send + 'static,
    {
        if self.is_closed.load(Ordering::Acquire) {
            return Err(SpawnError::Closed);
        }

        let Some(handle) = self.reserve_task() else {
            return Err(if self.is_closed.load(Ordering::Acquire) {
                SpawnError::Closed
            } else {
                SpawnError::NoCapacity
            });
        };

        let global_id = handle.global_id(self.config.tasks_per_leaf);
        self.init_task(global_id);

        let task = handle.task();
        let future = future.into_future();
        let future_ptr = FutureHelpers::box_future(future);

        if task.attach_future(future_ptr).is_err() {
            unsafe { FutureHelpers::drop_boxed(future_ptr) };
            self.release_task(handle);
            return Err(SpawnError::AttachFailed);
        }

        task.schedule();

        Ok(handle)
    }

    fn initialize_task_slots(&self) {
        let total = self.config.leaf_count * self.config.tasks_per_leaf;
        let slots_ptr = self.task_slots_ptr();

        unsafe {
            for idx in 0..total {
                let slot_ptr = slots_ptr.add(idx);
                ptr::write(slot_ptr, TaskSlot::new(ptr::null_mut()));
            }
        }
    }

    fn initialize_tasks(&self) {
        let tasks_per_leaf = self.config.tasks_per_leaf;
        let tasks_ptr = self.tasks_ptr();

        unsafe {
            for leaf in 0..self.config.leaf_count {
                for slot in 0..tasks_per_leaf {
                    let idx = leaf * tasks_per_leaf + slot;
                    let signal_idx = slot / 64;
                    let signal_bit = (slot % 64) as u32;
                    let signal_ptr = self.task_signal_ptr(leaf, signal_idx);
                    let global_id = (leaf * tasks_per_leaf + slot) as u64;
                    let task_ptr = tasks_ptr.add(idx);
                    let slot_ptr = self.task_slot_ptr(leaf, slot);
                    Task::construct(
                        task_ptr,
                        global_id,
                        leaf as u32,
                        signal_idx as u32,
                        slot as u32,
                        signal_bit,
                        signal_ptr,
                        slot_ptr,
                    );
                    (*task_ptr).bind_arena(self as *const _);
                }
            }
        }
    }

    #[inline]
    fn tasks_ptr(&self) -> *mut Task {
        unsafe { self.memory.as_ptr().add(self.layout.task_offset) as *mut Task }
    }

    #[inline]
    fn task_slots_ptr(&self) -> *mut TaskSlot {
        unsafe { self.memory.as_ptr().add(self.layout.task_slot_offset) as *mut TaskSlot }
    }

    #[inline]
    fn task_slot_ptr(&self, leaf_idx: usize, slot_idx: usize) -> *mut TaskSlot {
        debug_assert!(leaf_idx < self.config.leaf_count);
        debug_assert!(slot_idx < self.config.tasks_per_leaf);
        unsafe {
            self.task_slots_ptr()
                .add(leaf_idx * self.config.tasks_per_leaf + slot_idx)
        }
    }

    #[inline]
    pub fn task_signal_ptr(&self, leaf_idx: usize, signal_idx: usize) -> *const TaskSignal {
        unsafe {
            (self.memory.as_ptr().add(self.layout.signal_offset) as *const TaskSignal)
                .add(leaf_idx * self.layout.signals_per_leaf + signal_idx)
        }
    }

    #[inline]
    pub fn active_summary(&self, leaf_idx: usize) -> &AtomicU64 {
        debug_assert!(leaf_idx < self.config.leaf_count);
        unsafe {
            &*(self.memory.as_ptr().add(self.layout.leaf_offset) as *const AtomicU64).add(leaf_idx)
        }
    }

    #[inline]
    pub fn active_signals(&self, leaf_idx: usize) -> *const TaskSignal {
        debug_assert!(leaf_idx < self.config.leaf_count);
        unsafe {
            (self.memory.as_ptr().add(self.layout.signal_offset) as *const TaskSignal)
                .add(leaf_idx * self.layout.signals_per_leaf)
        }
    }

    #[inline]
    pub fn active_tree(&self) -> &SummaryTree {
        &self.active_tree
    }

    #[inline]
    pub fn leaf_count(&self) -> usize {
        self.config.leaf_count
    }

    #[inline]
    pub fn signals_per_leaf(&self) -> usize {
        self.layout.signals_per_leaf
    }

    #[inline]
    pub fn tasks_per_leaf(&self) -> usize {
        self.config.tasks_per_leaf
    }

    #[inline]
    pub fn compose_id(&self, leaf_idx: usize, slot_idx: usize) -> u64 {
        (leaf_idx * self.config.tasks_per_leaf + slot_idx) as u64
    }

    #[inline]
    pub fn decompose_id(&self, global_id: u64) -> (usize, usize) {
        let tasks_per_leaf = self.config.tasks_per_leaf;
        let leaf_idx = (global_id as usize) / tasks_per_leaf;
        let slot_idx = (global_id as usize) % tasks_per_leaf;
        (leaf_idx, slot_idx)
    }

    #[inline]
    pub unsafe fn task(&self, leaf_idx: usize, slot_idx: usize) -> &Task {
        debug_assert!(leaf_idx < self.config.leaf_count);
        debug_assert!(slot_idx < self.config.tasks_per_leaf);
        let signal_idx = slot_idx / 64;
        let bit_idx = (slot_idx % 64) as u32;
        let task_ptr = self
            .ensure_task_initialized(leaf_idx, signal_idx, bit_idx)
            .expect("task slot not initialized");
        unsafe { &*task_ptr.as_ptr() }
    }

    fn ensure_task_initialized(
        &self,
        leaf_idx: usize,
        signal_idx: usize,
        bit_idx: u32,
    ) -> Option<NonNull<Task>> {
        let slot_idx = signal_idx * 64 + bit_idx as usize;
        if slot_idx >= self.config.tasks_per_leaf {
            return None;
        }

        let slot_ptr = self.task_slot_ptr(leaf_idx, slot_idx);
        let slot = unsafe { &*slot_ptr };
        let existing = slot.task_ptr();
        if !existing.is_null() {
            return NonNull::new(existing);
        }

        debug_assert!(signal_idx < self.layout.signals_per_leaf);
        debug_assert!(bit_idx < 64);

        let idx = leaf_idx * self.config.tasks_per_leaf + slot_idx;
        let task_ptr = unsafe { self.tasks_ptr().add(idx) };
        let signal_ptr = self.task_signal_ptr(leaf_idx, signal_idx);
        let global_id = self.compose_id(leaf_idx, slot_idx);

        unsafe {
            Task::construct(
                task_ptr,
                global_id,
                leaf_idx as u32,
                signal_idx as u32,
                slot_idx as u32,
                bit_idx,
                signal_ptr,
                slot_ptr,
            );
            (*task_ptr).bind_arena(self as *const _);
        }

        NonNull::new(task_ptr)
    }

    #[inline]
    fn handle_for_location(
        &self,
        leaf_idx: usize,
        signal_idx: usize,
        bit_idx: u32,
    ) -> Option<TaskHandle> {
        self.ensure_task_initialized(leaf_idx, signal_idx, bit_idx)
            .map(TaskHandle::from_non_null)
    }

    pub fn init_task(&self, global_id: u64) {
        let (leaf_idx, slot_idx) = self.decompose_id(global_id);
        let signal_idx = slot_idx / 64;
        let signal_bit = (slot_idx % 64) as u32;

        debug_assert!(signal_idx < self.layout.signals_per_leaf);

        let task_ptr = self
            .ensure_task_initialized(leaf_idx, signal_idx, signal_bit)
            .expect("failed to initialize task slot");

        unsafe {
            let task = &mut *task_ptr.as_ptr();
            let slot_ptr = self.task_slot_ptr(leaf_idx, slot_idx);
            let signal_ptr = self.task_signal_ptr(leaf_idx, signal_idx);
            task.reset(
                global_id,
                leaf_idx as u32,
                signal_idx as u32,
                slot_idx as u32,
                signal_bit,
                signal_ptr,
                slot_ptr,
            );
            if task.arena_ptr.load(Ordering::Relaxed).is_null() {
                task.bind_arena(self as *const _);
            }
        }
    }

    pub fn reserve_task(&self) -> Option<TaskHandle> {
        if self.is_closed.load(Ordering::Acquire) {
            return None;
        }
        let (leaf_idx, signal_idx, bit) = self.active_tree.reserve_task()?;
        let handle = match self.handle_for_location(leaf_idx, signal_idx, bit) {
            Some(handle) => handle,
            None => {
                self.active_tree
                    .release_task_in_leaf(leaf_idx, signal_idx, bit);
                return None;
            }
        };
        self.total_tasks.fetch_add(1, Ordering::Relaxed);
        Some(handle)
    }

    pub fn reserve_task_in_leaf(&self, leaf_idx: usize) -> Option<TaskHandle> {
        for signal_idx in 0..self.layout.signals_per_leaf {
            if let Some(bit) = self.active_tree.reserve_task_in_leaf(leaf_idx, signal_idx) {
                let handle = match self.handle_for_location(leaf_idx, signal_idx, bit) {
                    Some(handle) => handle,
                    None => {
                        self.active_tree
                            .release_task_in_leaf(leaf_idx, signal_idx, bit);
                        continue;
                    }
                };
                self.total_tasks.fetch_add(1, Ordering::Relaxed);
                return Some(handle);
            }
        }
        None
    }

    pub fn release_task(&self, handle: TaskHandle) {
        let task = handle.task();
        self.active_tree.release_task_in_leaf(
            task.leaf_idx as usize,
            task.signal_idx as usize,
            task.signal_bit,
        );
        self.total_tasks.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn activate_task(&self, handle: TaskHandle) {
        let task = handle.task();
        let signal =
            unsafe { &*self.task_signal_ptr(task.leaf_idx as usize, task.signal_idx as usize) };
        let (was_set, was_empty) = signal.set(task.signal_bit);
        if was_set && was_empty {
            self.active_tree
                .mark_signal_active(task.leaf_idx as usize, task.signal_idx as usize);
        }
    }

    pub fn deactivate_task(&self, handle: TaskHandle) {
        let task = handle.task();
        let signal =
            unsafe { &*self.task_signal_ptr(task.leaf_idx as usize, task.signal_idx as usize) };
        let (_, now_empty) = signal.clear(task.signal_bit);
        if now_empty {
            self.active_tree
                .mark_signal_inactive(task.leaf_idx as usize, task.signal_idx as usize);
        }
    }

    #[allow(dead_code)]
    pub fn schedule_task_timer(
        &self,
        task: TaskHandle,
        timer: &TimerHandle,
        worker_id: u32,
        deadline_ns: u64,
    ) {
        let task_ptr = task.as_ptr() as *mut ();
        timer
            .inner()
            .prepare_schedule(worker_id, task_ptr, deadline_ns, None);
    }

    #[inline(always)]
    pub(crate) fn task_handle_from_payload(ptr: *mut ()) -> Option<TaskHandle> {
        NonNull::new(ptr as *mut Task).map(TaskHandle::from_non_null)
    }

    pub fn reserve_worker(&self) -> Option<usize> {
        self.active_tree.reserve_worker()
    }

    pub fn release_worker(&self, worker_idx: usize) {
        self.active_tree.release_worker(worker_idx);
    }

    pub fn mark_yield_active(&self, leaf_idx: usize) {
        self.active_tree.mark_yield_active(leaf_idx);
    }

    pub fn mark_yield_inactive(&self, leaf_idx: usize) {
        self.active_tree.mark_yield_inactive(leaf_idx);
    }

    pub fn stats(&self) -> ArenaStats {
        ArenaStats {
            total_capacity: self.config.leaf_count * self.config.tasks_per_leaf,
            active_tasks: self.total_tasks.load(Ordering::Relaxed) as usize,
            worker_count: self.active_tree.worker_count(),
        }
    }

    fn allocate_memory(size: usize, options: &ArenaOptions) -> io::Result<*mut u8> {
        #[cfg(unix)]
        {
            let mut flags = MAP_PRIVATE | MAP_ANONYMOUS;
            #[cfg(target_os = "linux")]
            if options.use_huge_pages {
                flags |= MAP_HUGETLB | MAP_HUGE_2MB;
            }
            let ptr = unsafe { mmap(ptr::null_mut(), size, PROT_READ | PROT_WRITE, flags, -1, 0) };
            if ptr == MAP_FAILED {
                Err(io::Error::last_os_error())
            } else {
                Ok(ptr as *mut u8)
            }
        }

        #[cfg(windows)]
        {
            let mut flags = MEM_RESERVE | MEM_COMMIT;
            if options.use_huge_pages {
                flags |= MEM_LARGE_PAGES;
            }
            let ptr = unsafe { VirtualAlloc(ptr::null_mut(), size, flags, PAGE_READWRITE) };
            if ptr.is_null() {
                Err(io::Error::last_os_error())
            } else {
                Ok(ptr as *mut u8)
            }
        }
    }
}

impl Drop for MmapExecutorArena {
    fn drop(&mut self) {
        unsafe {
            #[cfg(unix)]
            {
                munmap(self.memory.as_ptr() as *mut _, self.size);
            }

            #[cfg(windows)]
            {
                VirtualFree(self.memory.as_ptr() as *mut _, 0, MEM_RELEASE);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArenaStats {
    pub total_capacity: usize,
    pub active_tasks: usize,
    pub worker_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::Worker;
    use std::future::poll_fn;
    use std::mem::{self, MaybeUninit};
    use std::ptr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    unsafe fn noop_clone(_: *const ()) -> RawWaker {
        RawWaker::new(ptr::null(), &NOOP_WAKER_VTABLE)
    }

    unsafe fn noop(_: *const ()) {}

    static NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);

    fn noop_waker() -> Waker {
        unsafe { Waker::from_raw(RawWaker::new(ptr::null(), &NOOP_WAKER_VTABLE)) }
    }

    fn setup_arena(leaf_count: usize, tasks_per_leaf: usize) -> Arc<MmapExecutorArena> {
        let config = ArenaConfig::new(leaf_count, tasks_per_leaf).unwrap();
        Arc::new(MmapExecutorArena::with_config(config, ArenaOptions::default()).unwrap())
    }

    #[test]
    fn task_signal_basic_operations() {
        let signal = TaskSignal::new();
        let (was_empty, was_set) = signal.set(5);
        assert!(was_empty);
        assert!(was_set);
        assert!(signal.is_set(5));

        let (remaining, acquired) = signal.try_acquire(5);
        assert!(acquired);
        assert_eq!(remaining & (1 << 5), 0);
        assert!(!signal.is_set(5));

        let (remaining, now_empty) = signal.clear(5);
        assert_eq!(remaining, 0);
        assert!(now_empty);
    }

    #[test]
    fn task_signal_set_idempotent() {
        let signal = TaskSignal::new();
        assert_eq!(signal.set(3), (true, true));
        assert_eq!(signal.set(3), (false, false));
        assert!(signal.is_set(3));
    }

    #[test]
    fn task_signal_clear_noop_when_absent() {
        let signal = TaskSignal::new();
        let (remaining, now_empty) = signal.clear(7);
        assert_eq!(remaining, 0);
        assert!(now_empty);
    }

    #[test]
    fn task_signal_try_acquire_unset_bit() {
        let signal = TaskSignal::new();
        let (remaining, acquired) = signal.try_acquire(12);
        assert_eq!(remaining, 0);
        assert!(!acquired);
    }

    #[test]
    fn task_signal_try_acquire_from_wraps() {
        let signal = TaskSignal::new();
        signal.set(2);
        signal.set(60);

        let (bit, _) = signal
            .try_acquire_from(59)
            .expect("expected to acquire bit after wrap");
        assert_eq!(bit, 60);

        let (bit, _) = signal
            .try_acquire_from(61)
            .expect("expected to wrap to remaining bit");
        assert_eq!(bit, 2);
        assert!(signal.try_acquire_from(0).is_none());
    }

    #[test]
    fn task_signal_try_acquire_from_until_empty() {
        let signal = TaskSignal::new();
        signal.set(0);
        signal.set(1);

        assert!(signal.try_acquire_from(0).is_some());
        assert!(signal.try_acquire_from(0).is_some());
        assert!(signal.try_acquire_from(0).is_none());
        assert_eq!(signal.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn task_signal_try_acquire_from_selects_nearest() {
        let signal = TaskSignal::new();
        for bit in [2u32, 6, 10] {
            signal.set(bit);
        }
        let (bit, remaining) = signal
            .try_acquire_from(5)
            .expect("expected to acquire a bit");
        assert_eq!(bit, 6);
        assert_ne!(remaining & (1 << 2), 0);
    }

    #[test]
    fn schedule_begin_finish_flow_clears_summary() {
        let arena = setup_arena(1, 64);
        let handle = arena.reserve_task().expect("reserve task");
        let leaf = handle.leaf_idx();
        let signal_idx = handle.signal_idx();
        let bit_idx = handle.bit_idx();
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);

        let slot_idx = signal_idx * 64 + bit_idx as usize;
        let task = unsafe { arena.task(leaf, slot_idx) };
        let signal = unsafe { &*task.signal_ptr };

        assert_eq!(signal.load(Ordering::Relaxed), 0);
        task.schedule();
        assert!(signal.is_set(task.signal_bit));
        assert_ne!(
            arena.active_summary(leaf).load(Ordering::Acquire) & (1 << signal_idx),
            0
        );

        let (remaining, acquired) = signal.try_acquire(bit_idx);
        assert!(acquired);
        if remaining == 0 {
            arena.active_tree().mark_signal_inactive(leaf, signal_idx);
        }
        task.begin();
        task.finish();
        assert_eq!(
            arena.active_summary(leaf).load(Ordering::Acquire) & (1 << signal_idx),
            0
        );

        arena.release_task(handle);
    }

    #[test]
    fn finish_reschedules_when_work_arrives_during_execution() {
        let arena = setup_arena(1, 64);
        let handle = arena.reserve_task().expect("reserve task");
        let leaf = handle.leaf_idx();
        let signal_idx = handle.signal_idx();
        let bit_idx = handle.bit_idx();
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let slot_idx = signal_idx * 64 + bit_idx as usize;
        let task = unsafe { arena.task(leaf, slot_idx) };
        let signal = unsafe { &*task.signal_ptr };

        task.schedule();
        let (remaining, acquired) = signal.try_acquire(bit_idx);
        assert!(acquired);
        if remaining == 0 {
            arena.active_tree().mark_signal_inactive(leaf, signal_idx);
        }
        task.begin();
        // concurrent producer schedules additional work
        task.schedule();
        task.finish();
        assert_ne!(
            arena.active_summary(leaf).load(Ordering::Acquire) & (1 << signal_idx),
            0,
            "queue should remain visible after concurrent schedule"
        );

        arena.deactivate_task(handle);
        arena.release_task(handle);
    }

    #[test]
    fn task_handle_reports_task_fields() {
        let arena = setup_arena(4, 128);
        let leaf = 2;
        let slot = 113;
        let signal = slot / 64;
        let bit = (slot % 64) as u32;
        let task = unsafe { arena.task(leaf, slot) };
        let handle = TaskHandle::from_task(task);
        assert_eq!(handle.leaf_idx(), leaf);
        assert_eq!(handle.signal_idx(), signal);
        assert_eq!(handle.bit_idx(), bit);
    }

    #[test]
    fn task_handle_global_id_matches_components() {
        let arena = setup_arena(2, 128);
        let leaf = 1;
        let slot = 70;
        let task = unsafe { arena.task(leaf, slot) };
        let handle = TaskHandle::from_task(task);
        assert_eq!(handle.global_id(arena.tasks_per_leaf()), task.global_id());
    }

    #[test]
    fn future_helpers_drop_boxed_accepts_null() {
        unsafe {
            FutureHelpers::drop_boxed(ptr::null_mut());
        }
    }

    #[test]
    fn future_helpers_poll_boxed_accepts_null() {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert!(unsafe { FutureHelpers::poll_boxed(ptr::null_mut(), &mut cx) }.is_none());
    }

    #[test]
    fn task_construct_initializes_fields() {
        let mut storage = MaybeUninit::<Task>::uninit();
        let mut slot_storage = MaybeUninit::<TaskSlot>::uninit();
        let signal = TaskSignal::new();
        unsafe {
            slot_storage
                .as_mut_ptr()
                .write(TaskSlot::new(storage.as_mut_ptr()));
            Task::construct(
                storage.as_mut_ptr(),
                42,
                1,
                2,
                3,
                4,
                &signal as *const _,
                slot_storage.as_mut_ptr(),
            );
            let task = &*storage.as_ptr();
            assert_eq!(task.global_id, 42);
            assert_eq!(task.leaf_idx, 1);
            assert_eq!(task.signal_idx, 2);
            assert_eq!(task.slot_idx, 3);
            assert_eq!(task.signal_bit, 4);
            assert_eq!(task.state.load(Ordering::Relaxed), TASK_IDLE);
            assert!(!task.yielded.load(Ordering::Relaxed));
            assert!(task.future_ptr.load(Ordering::Relaxed).is_null());
            ptr::drop_in_place(storage.as_mut_ptr());
            ptr::drop_in_place(slot_storage.as_mut_ptr());
        }
    }

    #[test]
    fn task_bind_arena_sets_pointer() {
        let mut storage = MaybeUninit::<Task>::uninit();
        let mut slot_storage = MaybeUninit::<TaskSlot>::uninit();
        let signal = TaskSignal::new();
        let arena = setup_arena(1, 64);
        unsafe {
            slot_storage
                .as_mut_ptr()
                .write(TaskSlot::new(storage.as_mut_ptr()));
            Task::construct(
                storage.as_mut_ptr(),
                5,
                0,
                0,
                0,
                0,
                &signal as *const _,
                slot_storage.as_mut_ptr(),
            );
            let task = &*storage.as_ptr();
            assert!(task.arena_ptr.load(Ordering::Relaxed).is_null());
            task.bind_arena(Arc::as_ptr(&arena));
            assert_eq!(
                task.arena_ptr.load(Ordering::Relaxed),
                Arc::as_ptr(&arena) as *mut MmapExecutorArena
            );
            ptr::drop_in_place(storage.as_mut_ptr());
            ptr::drop_in_place(slot_storage.as_mut_ptr());
        }
    }

    #[test]
    fn task_schedule_is_idempotent() {
        let arena = setup_arena(1, 64);
        let handle = arena.reserve_task().expect("reserve task");
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
        let task = unsafe { arena.task(handle.leaf_idx(), slot_idx) };
        let signal = unsafe { &*task.signal_ptr };

        task.schedule();
        let summary_after_first = arena
            .active_summary(handle.leaf_idx())
            .load(Ordering::Relaxed);
        assert!(summary_after_first & (1 << handle.signal_idx()) != 0);

        task.schedule();
        let summary_after_second = arena
            .active_summary(handle.leaf_idx())
            .load(Ordering::Relaxed);
        assert_eq!(summary_after_first, summary_after_second);
        assert_eq!(signal.load(Ordering::Relaxed), 1 << task.signal_bit);

        arena.deactivate_task(handle);
        arena.release_task(handle);
    }

    #[test]
    fn task_begin_overwrites_state() {
        let arena = setup_arena(1, 64);
        let handle = arena.reserve_task().expect("reserve task");
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
        let task = unsafe { arena.task(handle.leaf_idx(), slot_idx) };

        task.schedule();
        task.begin();
        assert_eq!(task.state.load(Ordering::Relaxed), TASK_EXECUTING);

        arena.deactivate_task(handle);
        arena.release_task(handle);
    }

    #[test]
    fn task_finish_without_new_schedule_clears_signal() {
        let arena = setup_arena(1, 64);
        let handle = arena.reserve_task().expect("reserve task");
        let leaf = handle.leaf_idx();
        let signal_idx = handle.signal_idx();
        let bit_idx = handle.bit_idx();
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let slot_idx = signal_idx * 64 + bit_idx as usize;
        let task = unsafe { arena.task(leaf, slot_idx) };
        let signal = unsafe { &*task.signal_ptr };

        task.schedule();
        let (remaining, acquired) = signal.try_acquire(bit_idx);
        assert!(acquired);
        if remaining == 0 {
            arena.active_tree().mark_signal_inactive(leaf, signal_idx);
        }
        task.begin();
        task.finish();

        assert_eq!(task.state.load(Ordering::Relaxed), TASK_IDLE);
        assert_eq!(signal.load(Ordering::Relaxed), 0);
        assert_eq!(
            arena.active_summary(leaf).load(Ordering::Relaxed) & (1 << signal_idx),
            0
        );

        arena.release_task(handle);
    }

    #[test]
    fn task_finish_and_schedule_sets_signal() {
        let arena = setup_arena(1, 64);
        let handle = arena.reserve_task().expect("reserve task");
        let leaf = handle.leaf_idx();
        let signal_idx = handle.signal_idx();
        let bit_idx = handle.bit_idx();
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let slot_idx = signal_idx * 64 + bit_idx as usize;
        let task = unsafe { arena.task(leaf, slot_idx) };
        let signal = unsafe { &*task.signal_ptr };

        task.finish_and_schedule();
        assert_eq!(task.state.load(Ordering::Relaxed), TASK_SCHEDULED);
        assert!(signal.is_set(task.signal_bit));
        assert_ne!(
            arena.active_summary(leaf).load(Ordering::Relaxed) & (1 << signal_idx),
            0
        );

        arena.deactivate_task(handle);
        arena.release_task(handle);
    }

    #[test]
    fn task_clear_yielded_and_is_yielded() {
        let arena = setup_arena(1, 64);
        let handle = arena.reserve_task().expect("reserve task");
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
        let task = unsafe { arena.task(handle.leaf_idx(), slot_idx) };

        task.yielded.store(true, Ordering::Relaxed);
        assert!(task.is_yielded());
        task.clear_yielded();
        assert!(!task.is_yielded());

        arena.release_task(handle);
    }

    #[test]
    fn task_attach_future_rejects_second_future() {
        let arena = setup_arena(1, 64);
        let handle = arena.reserve_task().expect("reserve task");
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
        let task = unsafe { arena.task(handle.leaf_idx(), slot_idx) };

        let first_ptr = FutureHelpers::box_future(async {});
        task.attach_future(first_ptr).unwrap();
        let second_ptr = FutureHelpers::box_future(async {});
        let existing = task.attach_future(second_ptr).unwrap_err();
        assert_eq!(existing, first_ptr);
        unsafe { FutureHelpers::drop_boxed(second_ptr) };

        let ptr = task.take_future().unwrap();
        unsafe { FutureHelpers::drop_boxed(ptr) };
        arena.release_task(handle);
    }

    #[test]
    fn task_take_future_clears_pointer() {
        let arena = setup_arena(1, 64);
        let handle = arena.reserve_task().expect("reserve task");
        let global = handle.global_id(arena.tasks_per_leaf());
        arena.init_task(global);
        let slot_idx = handle.signal_idx() * 64 + handle.bit_idx() as usize;
        let task = unsafe { arena.task(handle.leaf_idx(), slot_idx) };

        let future_ptr = FutureHelpers::box_future(async {});
        task.attach_future(future_ptr).unwrap();
        let returned = task.take_future().unwrap();
        assert_eq!(returned, future_ptr);
        assert!(task.take_future().is_none());
        unsafe { FutureHelpers::drop_boxed(returned) };
        arena.release_task(handle);
    }

    #[test]
    fn arena_spawn_executes_future() {
        let arena = setup_arena(1, 8);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let handle = arena
            .spawn(async move {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            })
            .expect("spawn task");

        {
            let mut worker = Worker::new(arena.clone(), None);
            worker.run_until_idle();
        }

        assert_eq!(counter.load(Ordering::Relaxed), 1);
        arena.release_task(handle);
    }

    #[test]
    fn arena_spawn_returns_no_capacity_error() {
        let arena = setup_arena(1, 1);

        let handle = arena.spawn(async {}).expect("first spawn succeeds");
        let err = arena.spawn(async {}).expect_err("second spawn should fail");
        assert_eq!(err, SpawnError::NoCapacity);

        {
            let mut worker = Worker::new(arena.clone(), None);
            worker.run_until_idle();
        }

        arena.release_task(handle);
    }

    #[test]
    fn reserve_task_in_leaf_exhaustion() {
        let arena = setup_arena(1, 64);
        let mut bits = Vec::with_capacity(64);
        for _ in 0..64 {
            let bit = arena
                .active_tree()
                .reserve_task_in_leaf(0, 0)
                .expect("expected available bit");
            bits.push(bit);
        }
        assert!(arena.active_tree().reserve_task_in_leaf(0, 0).is_none());
        for bit in &bits {
            arena.active_tree().release_task_in_leaf(0, 0, *bit);
        }
    }

    #[test]
    fn reserve_task_in_leaf_after_release() {
        let arena = setup_arena(1, 64);
        let bit = arena
            .active_tree()
            .reserve_task_in_leaf(0, 0)
            .expect("expected bit");
        arena.active_tree().release_task_in_leaf(0, 0, bit);
        let new_bit = arena
            .active_tree()
            .reserve_task_in_leaf(0, 0)
            .expect("bit after release");
        arena.active_tree().release_task_in_leaf(0, 0, new_bit);
    }
}
