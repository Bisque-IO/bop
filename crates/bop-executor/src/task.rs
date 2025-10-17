use crate::bits;
use crate::summary_tree::{SummaryInit, SummaryTree};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU64, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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
}

impl Default for ArenaOptions {
    fn default() -> Self {
        Self {
            use_huge_pages: false,
        }
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

#[derive(Debug)]
struct ArenaLayout {
    root_offset: usize,
    leaf_offset: usize,
    signal_offset: usize,
    reservation_offset: usize,
    worker_bitmap_offset: usize,
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
        let task_offset = offset;
        offset += task_size;

        Self {
            root_offset,
            leaf_offset,
            signal_offset,
            reservation_offset,
            worker_bitmap_offset,
            task_offset,
            total_size: offset,
            signals_per_leaf,
            worker_bitmap_words,
        }
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
pub struct TaskHandle(u64);

impl TaskHandle {
    const LEAF_BITS: u32 = 20;
    const SIGNAL_BITS: u32 = 10;
    const BIT_BITS: u32 = 6;
    const BIT_SHIFT: u32 = 0;
    const SIGNAL_SHIFT: u32 = Self::BIT_BITS;
    const LEAF_SHIFT: u32 = Self::BIT_BITS + Self::SIGNAL_BITS;
    const BIT_MASK: u64 = (1u64 << Self::BIT_BITS) - 1;
    const SIGNAL_MASK: u64 = (1u64 << Self::SIGNAL_BITS) - 1;
    const LEAF_MASK: u64 = (1u64 << Self::LEAF_BITS) - 1;

    pub fn new(leaf_idx: usize, signal_idx: usize, bit_idx: u32) -> Self {
        let leaf = (leaf_idx as u64 & Self::LEAF_MASK) << Self::LEAF_SHIFT;
        let signal = (signal_idx as u64 & Self::SIGNAL_MASK) << Self::SIGNAL_SHIFT;
        let bit = (bit_idx as u64 & Self::BIT_MASK) << Self::BIT_SHIFT;
        TaskHandle(leaf | signal | bit)
    }

    pub fn leaf_idx(&self) -> usize {
        ((self.0 >> Self::LEAF_SHIFT) & Self::LEAF_MASK) as usize
    }

    pub fn signal_idx(&self) -> usize {
        ((self.0 >> Self::SIGNAL_SHIFT) & Self::SIGNAL_MASK) as usize
    }

    pub fn bit_idx(&self) -> u32 {
        ((self.0 >> Self::BIT_SHIFT) & Self::BIT_MASK) as u32
    }

    pub fn global_id(&self, tasks_per_leaf: usize) -> u64 {
        (self.leaf_idx() * tasks_per_leaf + self.signal_idx() * 64 + self.bit_idx() as usize) as u64
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
pub struct Task {
    global_id: u64,
    leaf_idx: u32,
    signal_idx: u32,
    slot_idx: u32,
    signal_bit: u32,
    signal_ptr: *const TaskSignal,
    arena_ptr: AtomicPtr<MmapExecutorArena>,
    state: AtomicU8,
    yielded: AtomicBool,
    future_ptr: AtomicPtr<()>,
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
                    arena_ptr: AtomicPtr::new(ptr::null_mut()),
                    state: AtomicU8::new(TASK_IDLE),
                    yielded: AtomicBool::new(false),
                    future_ptr: AtomicPtr::new(ptr::null_mut()),
                },
            );
        }
    }

    unsafe fn bind_arena(&self, arena: *const MmapExecutorArena) {
        self.arena_ptr
            .store(arena as *mut MmapExecutorArena, Ordering::Release);
    }

    pub fn global_id(&self) -> u64 {
        self.global_id
    }

    /// Attempts to schedule this queue for execution (IDLE → SCHEDULED transition).
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

    /// Marks the queue as EXECUTING (SCHEDULED → EXECUTING transition).
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

    /// Marks the queue as IDLE and handles concurrent schedules (EXECUTING → IDLE/SCHEDULED).
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
    /// T0: Executor calls begin()           → EXECUTING (2)
    /// T1: Producer calls schedule()        → EXECUTING | SCHEDULED (3)
    /// T2: Executor calls finish()          → SCHEDULED (1) [bit re-set in signal]
    /// T3: Executor sees bit, processes     → ...
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
    /// gate.finish();      // EXECUTING → IDLE
    /// gate.schedule();    // IDLE → SCHEDULED
    ///
    /// // Combined call (1 atomic op + signal update)
    /// gate.finish_and_schedule();  // EXECUTING → SCHEDULED
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
        let ptr = self as *const Task as *const ();
        unsafe { Waker::from_raw(RawWaker::new(ptr, &Self::WAKER_YIELD_VTABLE)) }
    }

    #[inline(always)]
    unsafe fn waker_clone(ptr: *const ()) -> RawWaker {
        RawWaker::new(ptr, &Self::WAKER_VTABLE)
    }

    #[inline(always)]
    unsafe fn waker_yield_wake(ptr: *const ()) {
        let task = unsafe { &*(ptr as *const Task) };
        task.yielded.store(true, Ordering::Relaxed);
    }

    #[inline(always)]
    unsafe fn waker_yield_wake_by_ref(ptr: *const ()) {
        let task = unsafe { &*(ptr as *const Task) };
        task.yielded.store(true, Ordering::Relaxed);
    }

    #[inline(always)]
    unsafe fn waker_wake(ptr: *const ()) {
        let task = unsafe { &*(ptr as *const Task) };
        task.schedule();
    }

    #[inline(always)]
    unsafe fn waker_wake_by_ref(ptr: *const ()) {
        let task = unsafe { &*(ptr as *const Task) };
        task.schedule();
    }

    #[inline(always)]
    unsafe fn waker_drop(_: *const ()) {}

    #[inline(always)]
    pub unsafe fn poll_future(&self, cx: &mut Context<'_>) -> Option<Poll<()>> {
        let ptr = self.future_ptr.load(Ordering::Acquire);
        unsafe { FutureHelpers::poll_boxed(ptr, cx) }
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
    ) {
        self.global_id = global_id;
        self.leaf_idx = leaf_idx;
        self.signal_idx = signal_idx;
        self.slot_idx = slot_idx;
        self.signal_bit = signal_bit;
        self.signal_ptr = signal_ptr;
        self.state.store(TASK_IDLE, Ordering::Relaxed);
        self.yielded.store(false, Ordering::Relaxed);
        self.future_ptr.store(ptr::null_mut(), Ordering::Relaxed);
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

unsafe impl Send for MmapExecutorArena {}
unsafe impl Sync for MmapExecutorArena {}

impl MmapExecutorArena {
    pub fn with_config(config: ArenaConfig, options: ArenaOptions) -> io::Result<Self> {
        let layout = ArenaLayout::new(&config);
        let memory_ptr = Self::allocate_memory(layout.total_size, &options)?;
        let memory = NonNull::new(memory_ptr)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "allocation returned null"))?;

        unsafe {
            ptr::write_bytes(memory.as_ptr(), 0, layout.total_size);
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

        arena.initialize_tasks();
        Ok(arena)
    }

    pub fn new(leaf_count: usize, tasks_per_leaf: usize) -> io::Result<Self> {
        Self::with_config(
            ArenaConfig::new(leaf_count, tasks_per_leaf)?,
            ArenaOptions::default(),
        )
    }

    fn initialize_tasks(&self) {
        let tasks_per_leaf = self.config.tasks_per_leaf;
        let signals_per_leaf = self.layout.signals_per_leaf;
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
                    Task::construct(
                        task_ptr,
                        global_id,
                        leaf as u32,
                        signal_idx as u32,
                        slot as u32,
                        signal_bit,
                        signal_ptr,
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
        unsafe {
            &*self
                .tasks_ptr()
                .add(leaf_idx * self.config.tasks_per_leaf + slot_idx)
        }
    }

    pub fn init_task(&self, global_id: u64) {
        let (leaf_idx, slot_idx) = self.decompose_id(global_id);
        let signal_idx = slot_idx / 64;
        let signal_bit = (slot_idx % 64) as u32;

        debug_assert!(signal_idx < self.layout.signals_per_leaf);

        unsafe {
            let task_ptr = self
                .tasks_ptr()
                .add(leaf_idx * self.config.tasks_per_leaf + slot_idx);
            let task = &mut *task_ptr;
            let signal_ptr = self.task_signal_ptr(leaf_idx, signal_idx);
            task.reset(
                global_id,
                leaf_idx as u32,
                signal_idx as u32,
                slot_idx as u32,
                signal_bit,
                signal_ptr,
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
        self.total_tasks.fetch_add(1, Ordering::Relaxed);
        Some(TaskHandle::new(leaf_idx, signal_idx, bit))
    }

    pub fn reserve_task_in_leaf(&self, leaf_idx: usize) -> Option<TaskHandle> {
        for signal_idx in 0..self.layout.signals_per_leaf {
            if let Some(bit) = self.active_tree.reserve_task_in_leaf(leaf_idx, signal_idx) {
                self.total_tasks.fetch_add(1, Ordering::Relaxed);
                return Some(TaskHandle::new(leaf_idx, signal_idx, bit));
            }
        }
        None
    }

    pub fn release_task(&self, handle: TaskHandle) {
        self.active_tree.release_task_in_leaf(
            handle.leaf_idx(),
            handle.signal_idx(),
            handle.bit_idx(),
        );
        self.total_tasks.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn activate_task(&self, handle: TaskHandle) {
        let signal = unsafe { &*self.task_signal_ptr(handle.leaf_idx(), handle.signal_idx()) };
        let (was_set, was_empty) = signal.set(handle.bit_idx());
        if was_set && was_empty {
            self.active_tree
                .mark_signal_active(handle.leaf_idx(), handle.signal_idx());
        }
    }

    pub fn deactivate_task(&self, handle: TaskHandle) {
        let signal = unsafe { &*self.task_signal_ptr(handle.leaf_idx(), handle.signal_idx()) };
        let (_, now_empty) = signal.clear(handle.bit_idx());
        if now_empty {
            self.active_tree
                .mark_signal_inactive(handle.leaf_idx(), handle.signal_idx());
        }
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
    use std::future::poll_fn;
    use std::mem::{self, MaybeUninit};
    use std::ptr;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
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
    fn task_handle_roundtrip_preserves_indices() {
        let leaf = (1 << TaskHandle::LEAF_BITS) - 1;
        let signal = (1 << TaskHandle::SIGNAL_BITS) - 1;
        let bit = (1 << TaskHandle::BIT_BITS) - 1;
        let handle = TaskHandle::new(leaf as usize, signal as usize, bit as u32);
        assert_eq!(handle.leaf_idx(), leaf as usize);
        assert_eq!(handle.signal_idx(), signal as usize);
        assert_eq!(handle.bit_idx(), bit as u32);
    }

    #[test]
    fn task_handle_global_id_matches_components() {
        let handle = TaskHandle::new(3, 5, 7);
        let tasks_per_leaf = 512usize;
        let expected = 3 * tasks_per_leaf + 5 * 64 + 7;
        assert_eq!(handle.global_id(tasks_per_leaf), expected as u64);
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
        let signal = TaskSignal::new();
        unsafe {
            Task::construct(storage.as_mut_ptr(), 42, 1, 2, 3, 4, &signal as *const _);
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
        }
    }

    #[test]
    fn task_bind_arena_sets_pointer() {
        let mut storage = MaybeUninit::<Task>::uninit();
        let signal = TaskSignal::new();
        let arena = setup_arena(1, 64);
        unsafe {
            Task::construct(storage.as_mut_ptr(), 5, 0, 0, 0, 0, &signal as *const _);
            let task = &*storage.as_ptr();
            assert!(task.arena_ptr.load(Ordering::Relaxed).is_null());
            task.bind_arena(Arc::as_ptr(&arena));
            assert_eq!(
                task.arena_ptr.load(Ordering::Relaxed),
                Arc::as_ptr(&arena) as *mut MmapExecutorArena
            );
            ptr::drop_in_place(storage.as_mut_ptr());
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
