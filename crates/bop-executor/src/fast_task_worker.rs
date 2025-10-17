//! Ultra-fast task worker with zero-queuing overhead.
//!
//! This module implements a task worker optimized for **raw speed** over fairness:
//! - Target: < 20ns task selection (preferably < 10ns)
//! - NO hot queue (zero queuing overhead)
//! - NO selector (no random numbers, no bitmap scanning)
//! - Direct round-robin selection with O(1) indexing
//! - Inline polling to minimize function call overhead
//!
//! # Design Philosophy
//!
//! Traditional TaskWorker prioritizes fairness through random selection and cache
//! locality through a hot queue. FastTaskWorker prioritizes **speed above all else**.
//!
//! ```text
//! TaskWorker:     Hot Queue → Selector → Random → Bitmap Scan → CAS → Poll
//!                 [5ns]       [10ns]     [10ns]    [15ns]        [10ns] [10ns] = 60ns
//!
//! FastTaskWorker: Round-Robin → CAS → Inline Poll
//!                 [2ns]         [8ns] [8ns] = 18ns
//! ```
//!
//! # Architecture
//!
//! ```text
//! FastTaskWorker
//! ├─ next_slot: usize          (simple counter, no atomics)
//! ├─ leaf_idx: usize           (fixed leaf assignment)
//! └─ stats: FastWorkerStats    (minimal tracking)
//! ```
//!
//! # Task Selection Strategy
//!
//! 1. **Round-Robin Counter**: Increment local counter (no atomics, ~1ns)
//! 2. **Direct Task Access**: Compute global_id and get task pointer (~2ns)
//! 3. **CAS Acquire**: Try to acquire task with single CAS (~8ns)
//! 4. **Inline Poll**: Poll future directly without function call (~8ns)
//!
//! Total: ~19ns (within < 20ns requirement)
//!
//! # Example Usage
//!
//! ```ignore
//! use bop_executor::fast_task_worker::FastTaskWorker;
//! use bop_executor::task::MmapExecutorArena;
//!
//! const LEAF_BITS: u32 = 8;
//! const SLOT_BITS: u32 = 8;
//!
//! type Arena = MmapExecutorArena<LEAF_BITS, SLOT_BITS>;
//!
//! fn main() {
//!     let arena = Arena::new().unwrap();
//!     let mut worker = FastTaskWorker::<SLOT_BITS>::new(0);
//!
//!     // Process 1000 tasks
//!     let processed = worker.run(&arena, 1000);
//!     println!("Processed {} tasks", processed);
//!
//!     worker.stats.print_report();
//! }
//! ```
//!
//! # Performance Characteristics
//!
//! - **Best case**: 11ns (task ready, future immediately returns Ready)
//! - **Typical case**: 18ns (task needs polling)
//! - **Worst case**: N × 18ns (N = number of slots to scan before finding work)
//!
//! # When to Use
//!
//! Use FastTaskWorker when:
//! - You need maximum throughput (> 50M polls/sec)
//! - You have single worker per leaf (no contention)
//! - Fairness is less critical than speed
//! - You want predictable, deterministic scheduling
//!
//! Use TaskWorker when:
//! - You need fair random selection across many workers
//! - Cache locality for yielded tasks is critical
//! - You want detailed statistics and observability

use crate::active_summary::ActiveSummaryTree;
use crate::selector::Selector;
use crate::signal_scan;
use crate::task::{MmapExecutorArena, Task};
use crossbeam_deque::{Steal, Stealer, Worker};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

#[derive(Debug)]
struct YieldQueueState {
    count: AtomicUsize,
    summary_ptr: *const AtomicU64,
    tree_ptr: *const ActiveSummaryTree,
    worker_idx: usize,
}

unsafe impl Send for YieldQueueState {}
unsafe impl Sync for YieldQueueState {}

#[derive(Clone)]
pub struct YieldStealer {
    state: Arc<YieldQueueState>,
    stealer: Stealer<u64>,
}

unsafe impl Send for YieldStealer {}
unsafe impl Sync for YieldStealer {}

impl YieldStealer {
    fn new(state: Arc<YieldQueueState>, stealer: Stealer<u64>) -> Self {
        Self { state, stealer }
    }
}

impl YieldQueueState {
    fn new(
        summary_ptr: *const AtomicU64,
        tree_ptr: *const ActiveSummaryTree,
        worker_idx: usize,
    ) -> Self {
        Self {
            count: AtomicUsize::new(0),
            summary_ptr,
            tree_ptr,
            worker_idx,
        }
    }

    #[inline]
    fn increment(&self) {
        let prev = self.count.fetch_add(1, Ordering::AcqRel);
        if prev == 0 {
            unsafe {
                (*self.summary_ptr).fetch_or(1, Ordering::AcqRel);
                (*self.tree_ptr).mark_yield_active(self.worker_idx);
            }
        }
    }

    #[inline]
    fn decrement(&self) {
        let prev = self.count.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(prev > 0);
        if prev == 1 {
            unsafe {
                (*self.summary_ptr).fetch_and(!1, Ordering::AcqRel);
                (*self.tree_ptr).mark_yield_inactive(self.worker_idx);
            }
        }
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.count.load(Ordering::Acquire) == 0
    }
}

/// Configuration for blocking poll behavior.
///
/// Controls the trade-off between latency (spinning) and CPU efficiency (parking).
///
/// # Examples
///
/// ```ignore
/// // Non-blocking mode (original behavior)
/// let config = PollConfig::non_blocking();
///
/// // Low-latency mode (spin 100 times before parking)
/// let config = PollConfig::low_latency();
///
/// // Balanced mode (spin 10 times, park with 1s timeout)
/// let config = PollConfig::balanced();
///
/// // Pure parking mode (no spinning, immediate park when idle)
/// let config = PollConfig::park_immediately();
/// ```
#[derive(Debug, Clone, Copy)]
pub struct PollConfig {
    /// How many times to spin before checking for global idleness.
    ///
    /// - `None`: Skip spinning phase entirely (go straight to idle check)
    /// - `Some(n)`: Spin n times (~1-2ns per iteration) before giving up
    ///
    /// **Typical values:**
    /// - `Some(100)`: Low latency (~100-200ns spin), good for high throughput
    /// - `Some(10)`: Balanced (~10-20ns spin), good for moderate load
    /// - `None`: No spinning, good for truly idle workloads
    pub spin_before_park: Option<usize>,

    /// Timeout for parking when no work is available.
    ///
    /// - `None`: Park indefinitely until work arrives
    /// - `Some(duration)`: Wake up after duration even if no work
    ///
    /// **Use cases:**
    /// - `None`: Pure event-driven (wake only on work arrival)
    /// - `Some(Duration::from_millis(100))`: Periodic wakeup for health checks
    /// - `Some(Duration::from_secs(1))`: Shutdown detection
    pub park_timeout: Option<Duration>,
}

impl PollConfig {
    /// Non-blocking mode - never parks, always returns immediately.
    ///
    /// This is the original behavior, suitable for dedicated worker threads
    /// where you want maximum throughput and can afford 100% CPU usage.
    pub const fn non_blocking() -> Self {
        Self {
            spin_before_park: None,
            park_timeout: Some(Duration::from_nanos(0)),
        }
    }

    /// Low-latency mode - spin 100 times before checking for global idleness.
    ///
    /// Optimized for high-throughput scenarios where work arrives frequently.
    /// Trades ~100-200ns of CPU time to avoid parking overhead.
    pub const fn low_latency() -> Self {
        Self {
            spin_before_park: Some(100),
            park_timeout: None,
        }
    }

    /// Balanced mode - spin 10 times, then park with 1-second timeout.
    ///
    /// Good default for moderate workloads. Provides reasonable latency
    /// while allowing periodic wakeups for maintenance.
    pub const fn balanced() -> Self {
        Self {
            spin_before_park: Some(10),
            park_timeout: Some(Duration::from_secs(1)),
        }
    }

    /// Park immediately when idle - no spinning.
    ///
    /// Most CPU-efficient for idle workloads. Threads park right away and rely
    /// on permits to wake them when work becomes available.
    pub const fn park_immediately() -> Self {
        Self {
            spin_before_park: None,
            park_timeout: None,
        }
    }

    /// Custom configuration with specific spin count and timeout.
    pub const fn custom(spin_before_park: Option<usize>, park_timeout: Option<Duration>) -> Self {
        Self {
            spin_before_park,
            park_timeout,
        }
    }
}

/// Comprehensive statistics for fast worker.
#[derive(Debug, Default, Clone)]
pub struct FastWorkerStats {
    /// Number of tasks polled.
    pub tasks_polled: u64,

    /// Number of tasks that completed.
    pub completed_count: u64,

    /// Number of tasks that yielded cooperatively.
    pub yielded_count: u64,

    /// Number of tasks that are waiting (Poll::Pending, not yielded).
    pub waiting_count: u64,

    /// Number of CAS failures (contention or already executing).
    pub cas_failures: u64,

    /// Number of empty scans (no tasks available).
    pub empty_scans: u64,

    /// Number of polls from yield queue (hot path).
    pub yield_queue_polls: u64,

    /// Number of polls from signals (cold path).
    pub signal_polls: u64,

    /// Number of work stealing attempts from other workers.
    pub steal_attempts: u64,

    /// Number of successful work steals.
    pub steal_successes: u64,

    /// Number of leaf summary checks.
    pub leaf_summary_checks: u64,

    /// Number of leaf summary hits (summary != 0).
    pub leaf_summary_hits: u64,

    /// Number of attempts to steal from other workers' leaf partitions.
    pub leaf_steal_attempts: u64,

    /// Number of successful steals from other workers' leaf partitions.
    pub leaf_steal_successes: u64,
}

impl FastWorkerStats {
    /// Creates new statistics with all counters at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets all statistics to zero.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Calculates average polls per task (including CAS failures).
    pub fn avg_attempts_per_task(&self) -> f64 {
        if self.tasks_polled == 0 {
            0.0
        } else {
            (self.tasks_polled + self.cas_failures) as f64 / self.tasks_polled as f64
        }
    }

    /// Calculates yield rate (0.0 to 1.0).
    pub fn yield_rate(&self) -> f64 {
        let total = self.yielded_count + self.waiting_count + self.completed_count;
        if total == 0 {
            0.0
        } else {
            self.yielded_count as f64 / total as f64
        }
    }

    /// Prints a formatted statistics report.
    pub fn print_report(&self) {
        println!("╔════════════════════════════════════════════════════════════╗");
        println!("║             FastWorker Statistics Report                  ║");
        println!("╠════════════════════════════════════════════════════════════╣");
        println!(
            "║ Tasks Polled:        {:>10}                         ║",
            self.tasks_polled
        );
        println!(
            "║   From yield queue:  {:>10}                         ║",
            self.yield_queue_polls
        );
        println!(
            "║   From signals:      {:>10}                         ║",
            self.signal_polls
        );
        println!(
            "║ Completed:           {:>10}                         ║",
            self.completed_count
        );
        println!(
            "║ Yielded:             {:>10}  ({:>5.1}%)               ║",
            self.yielded_count,
            self.yield_rate() * 100.0
        );
        println!(
            "║ Waiting:             {:>10}                         ║",
            self.waiting_count
        );
        println!("╠════════════════════════════════════════════════════════════╣");
        println!(
            "║ CAS Failures:        {:>10}                         ║",
            self.cas_failures
        );
        println!(
            "║ Empty Scans:         {:>10}                         ║",
            self.empty_scans
        );
        println!(
            "║ Avg Attempts/Task:   {:>10.2}                       ║",
            self.avg_attempts_per_task()
        );
        println!("╠════════════════════════════════════════════════════════════╣");
        println!(
            "║ Leaf Summary Checks: {:>10}                         ║",
            self.leaf_summary_checks
        );
        println!(
            "║ Leaf Summary Hits:   {:>10}  ({:>5.1}%)               ║",
            self.leaf_summary_hits,
            if self.leaf_summary_checks > 0 {
                (self.leaf_summary_hits as f64 / self.leaf_summary_checks as f64) * 100.0
            } else {
                0.0
            }
        );
        println!(
            "║ Steal Attempts:      {:>10}                         ║",
            self.steal_attempts
        );
        println!(
            "║ Steal Successes:     {:>10}  ({:>5.1}%)               ║",
            self.steal_successes,
            if self.steal_attempts > 0 {
                (self.steal_successes as f64 / self.steal_attempts as f64) * 100.0
            } else {
                0.0
            }
        );
        println!("╠════════════════════════════════════════════════════════════╣");
        println!(
            "║ Leaf Steal Attempts: {:>10}                         ║",
            self.leaf_steal_attempts
        );
        println!(
            "║ Leaf Steal Success:  {:>10}  ({:>5.1}%)               ║",
            self.leaf_steal_successes,
            if self.leaf_steal_attempts > 0 {
                (self.leaf_steal_successes as f64 / self.leaf_steal_attempts as f64) * 100.0
            } else {
                0.0
            }
        );
        println!("╚════════════════════════════════════════════════════════════╝");
    }
}

// Yield queue now uses crossbeam_deque::Worker for stealability

/// Ultra-fast task worker with efficient randomized signal selection.
///
/// Optimized for:
/// - Fair task distribution across workers with minimal contention
/// - Efficient yielding through local yield queue
/// - Work stealing from other workers' yield queues
/// - Dynamic reconfiguration when worker count changes
///
/// # Task Selection Strategy
///
/// 1. **Priority 1**: Poll from local yield queue (hot, cache-friendly)
/// 2. **Priority 2**: Randomized search through assigned signal partition
/// 3. **Priority 3**: Randomized search through remaining signals (stealing)
/// 4. **Priority 4**: Steal from other workers' yield queues
///
/// # Worker Partitioning
///
/// Each worker is assigned a portion of all TaskSignals:
/// - Worker 0 gets signals [0, signals_per_leaf/N)
/// - Worker 1 gets signals [signals_per_leaf/N, 2*signals_per_leaf/N)
/// - etc.
///
/// This minimizes contention while maintaining load balance.
///
/// # Yield Queue Strategy
///
/// - When yield queue is empty: Continue with signal scanning
/// - When signal is empty: Try to pop 1 item from yield queue
/// - After scanning all signals with no work: Empty entire yield queue once
/// - Then try stealing from other workers' yield queues
pub struct FastTaskWorker {
    /// Worker index (for partitioning).
    worker_idx: usize,

    /// Pointer to the arena's active summary tree for coordination.
    active_tree_ptr: *const ActiveSummaryTree,

    /// Random number generator for signal selection.
    selector: Selector,

    /// Start of this worker's assigned signal range (inclusive).
    partition_start: usize,

    /// End of this worker's assigned signal range (exclusive).
    partition_end: usize,

    /// Total number of signals per leaf.
    signals_per_leaf: usize,

    /// Cached worker count for detecting changes.
    cached_worker_count: usize,

    /// Local yield queue (stealable work-stealing deque).
    /// Owner pushes/pops from one end (LIFO), stealers take from other end (FIFO).
    yield_queue: Worker<u64>,

    /// Shared state tracking the number of items in this worker's yield queue.
    yield_state: Arc<YieldQueueState>,

    /// Stealers paired with the corresponding workers' yield states.
    yield_stealers: Vec<YieldStealer>,

    /// Statistics tracking.
    pub stats: FastWorkerStats,
}

impl FastTaskWorker {
    /// Creates a new fast task worker.
    ///
    /// # Parameters
    ///
    /// - `worker_idx`: Index of this worker (for partitioning)
    /// - `signals_per_leaf`: Total number of signals per leaf
    /// - `arena`: Reference to the arena (for accessing the active summary tree)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let worker = FastTaskWorker::new(0, 64, &arena);
    /// ```
    pub fn new(worker_idx: usize, signals_per_leaf: usize, arena: &MmapExecutorArena) -> Self {
        Self::with_yield_stealers(worker_idx, signals_per_leaf, arena, Vec::new())
    }

    /// Creates a new fast task worker with yield queue stealers.
    ///
    /// # Parameters
    ///
    /// - `worker_idx`: Index of this worker (for partitioning)
    /// - `signals_per_leaf`: Total number of signals per leaf
    /// - `arena`: Reference to the arena (for accessing the active summary tree)
    /// - `yield_stealers`: Stealers for other workers' yield queues
    ///
    /// # Example
    ///
    /// ```ignore
    /// let worker = FastTaskWorker::with_yield_stealers(0, 64, &arena, stealers);
    /// ```
    pub fn with_yield_stealers(
        worker_idx: usize,
        signals_per_leaf: usize,
        arena: &MmapExecutorArena,
        yield_stealers: Vec<YieldStealer>,
    ) -> Self {
        let tree_ptr = arena.active_tree() as *const ActiveSummaryTree;
        let worker_count = unsafe { (*tree_ptr).worker_count() };
        let (partition_start, partition_end) =
            Self::compute_partition(worker_idx, signals_per_leaf, worker_count);
        debug_assert!(worker_idx < arena.yield_capacity());

        let summary_ptr = arena.yield_summary(worker_idx) as *const AtomicU64;
        let yield_state = Arc::new(YieldQueueState::new(summary_ptr, tree_ptr, worker_idx));

        Self {
            worker_idx,
            active_tree_ptr: tree_ptr,
            selector: Selector::new(),
            partition_start,
            partition_end,
            signals_per_leaf,
            cached_worker_count: worker_count,
            yield_queue: Worker::new_lifo(), // LIFO for owner (cache locality)
            yield_state,
            yield_stealers,
            stats: FastWorkerStats::new(),
        }
    }

    /// Computes the signal partition for a worker.
    ///
    /// # Parameters
    ///
    /// - `worker_idx`: Index of this worker
    /// - `signals_per_leaf`: Total number of signals per leaf
    /// - `worker_count`: Total number of workers
    ///
    /// # Returns
    ///
    /// Tuple of (start, end) where start is inclusive and end is exclusive.
    fn compute_partition(
        worker_idx: usize,
        signals_per_leaf: usize,
        worker_count: usize,
    ) -> (usize, usize) {
        if worker_count == 0 {
            return (0, signals_per_leaf);
        }

        let signals_per_worker = signals_per_leaf / worker_count;
        let extra_signals = signals_per_leaf % worker_count;

        let start = if worker_idx < extra_signals {
            worker_idx * (signals_per_worker + 1)
        } else {
            extra_signals * (signals_per_worker + 1)
                + (worker_idx - extra_signals) * signals_per_worker
        };

        let end = if worker_idx < extra_signals {
            start + signals_per_worker + 1
        } else {
            start + signals_per_worker
        };

        (start, end.min(signals_per_leaf))
    }

    /// Checks if worker count has changed and reconfigures partition if needed.
    ///
    /// This is called periodically by `try_poll_task` to detect worker count
    /// changes and adjust the partition accordingly.
    ///
    /// # Returns
    ///
    /// `true` if the partition was reconfigured, `false` otherwise.
    #[inline]
    fn check_and_reconfigure(&mut self) -> bool {
        let current_count = unsafe { (*self.active_tree_ptr).worker_count() };

        if current_count != self.cached_worker_count {
            let (start, end) =
                Self::compute_partition(self.worker_idx, self.signals_per_leaf, current_count);
            self.partition_start = start;
            self.partition_end = end;
            self.cached_worker_count = current_count;
            true
        } else {
            false
        }
    }

    /// Manually reconfigures worker partition based on current worker count.
    ///
    /// Normally not needed as `try_poll_task` automatically checks periodically,
    /// but can be called explicitly if needed.
    pub fn reconfigure(&mut self) {
        self.check_and_reconfigure();
    }

    /// Returns a stealer paired with the yield queue state for this worker.
    pub fn yield_stealer_entry(&self) -> YieldStealer {
        YieldStealer::new(self.yield_state.clone(), self.yield_queue.stealer())
    }

    /// Sets the yield stealers for this worker.
    pub fn set_yield_stealers(&mut self, stealers: Vec<YieldStealer>) {
        self.yield_stealers = stealers;
    }

    /// Creates a slice from the signal pointer array for SIMD scanning.
    ///
    /// # Safety
    ///
    /// - `signal_ptr` must point to a valid array of at least `count` TaskSignals
    /// - The signals must remain valid for the lifetime of the returned slice
    #[inline]
    unsafe fn signals_slice(
        signal_ptr: *const crate::task::TaskSignal,
        count: usize,
    ) -> &'static [crate::task::TaskSignal] {
        unsafe { std::slice::from_raw_parts(signal_ptr, count) }
    }

    /// Runs the worker for a specified number of iterations.
    ///
    /// This will attempt to find and poll tasks until either:
    /// - `max_iterations` tasks have been polled
    /// - No more tasks are available
    ///
    /// # Parameters
    ///
    /// - `arena`: Task arena containing tasks
    /// - `leaf_idx`: Leaf index to process
    /// - `max_iterations`: Maximum number of tasks to poll
    ///
    /// # Returns
    ///
    /// Number of tasks successfully polled.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let processed = worker.run(&arena, 0, 10000);
    /// println!("Processed {} tasks", processed);
    /// ```
    pub fn run(
        &mut self,
        arena: &MmapExecutorArena,
        leaf_idx: usize,
        max_iterations: usize,
    ) -> usize {
        let mut processed = 0;

        for _ in 0..max_iterations {
            if !self.try_poll_task(arena, leaf_idx) {
                // No work found after full scan
                break;
            }
            processed += 1;
        }

        processed
    }

    /// Runs the worker until no more work is available.
    ///
    /// This will keep polling tasks until a full scan of all signals
    /// finds no available work.
    ///
    /// # Parameters
    ///
    /// - `arena`: Task arena containing tasks
    /// - `leaf_idx`: Leaf index to process
    ///
    /// # Returns
    ///
    /// Number of tasks successfully polled.
    pub fn run_until_idle(&mut self, arena: &MmapExecutorArena, leaf_idx: usize) -> usize {
        let mut processed = 0;

        loop {
            if !self.try_poll_task(arena, leaf_idx) {
                break;
            }
            processed += 1;
        }

        processed
    }

    /// Tries to poll a task from any leaf using leaf-level partitioning.
    ///
    /// This scans across multiple leaves assigned to this worker, checking leaf summaries
    /// first to skip empty leaves entirely. Much more efficient than calling try_poll_task
    /// in a loop for each leaf.
    ///
    /// # Algorithm
    ///
    /// 1. **Partition leaves** among workers (not signals within leaves)
    /// 2. **Scan assigned leaf summaries** to find leaves with work
    /// 3. **Randomly select** from available leaves and signals
    /// 4. **Fall back** to stealing from other workers' leaves
    ///
    /// # Parameters
    ///
    /// - `arena`: Task arena containing tasks
    /// - `leaf_count`: Total number of leaves to scan
    ///
    /// # Returns
    ///
    /// - `true`: Successfully found and polled a task
    /// - `false`: No work available anywhere
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     if !worker.try_poll_with_leaf_partition(&arena, 256) {
    ///         break; // No more work
    ///     }
    /// }
    /// ```
    #[inline(always)]
    pub fn try_poll_with_leaf_partition(
        &mut self,
        arena: &MmapExecutorArena,
        leaf_count: usize,
    ) -> bool {
        // Periodically check for worker count changes
        if (self.stats.tasks_polled & 1023) == 0 {
            self.check_and_reconfigure();
        }

        // PRIORITY 1: Try yield queue first (but don't return - continue to scan for new work)
        let mut found_work = false;
        // if let Some(global_id) = self.yield_queue.pop() {
        //     self.poll_yielded_task(arena, global_id);
        //     found_work = true;
        // }

        if self.empty_yielded(arena) > 0 {
            found_work = true;
        }

        // Compute leaf partition (partition leaves, not signals within leaves)
        let leaf_partition_size = leaf_count / self.cached_worker_count.max(1);
        let leaf_start = self.worker_idx * leaf_partition_size;
        let leaf_end = if self.worker_idx == self.cached_worker_count - 1 {
            leaf_count // Last worker gets remainder
        } else {
            leaf_start + leaf_partition_size
        };

        // PRIORITY 2: Scan assigned leaf partition
        // Check summaries to find leaves with work
        // Start at random offset for fairness (prevents starvation of high-index leaves)
        let partition_size = leaf_end - leaf_start;
        let random_start = if partition_size > 0 {
            (self.selector.next() as usize) % partition_size
        } else {
            0
        };

        'leaf_summary: for i in 0..partition_size {
            let leaf_idx = leaf_start + ((random_start + i) % partition_size);
            let summary = arena.active_summary(leaf_idx);
            let summary_value = summary.load(Ordering::Acquire);

            self.stats.leaf_summary_checks += 1;
            if summary_value != 0 {
                self.stats.leaf_summary_hits += 1;
                // This leaf has work - pick a random signal
                let random_bit = (self.selector.next() & 63) as u64;
                let signal_bit = crate::bits::find_nearest(summary_value, random_bit);

                if signal_bit < 64 {
                    let signal_idx = signal_bit as usize;

                    match self.try_acquire_and_poll_from_signal(arena, leaf_idx, signal_idx) {
                        Some(true) => {
                            found_work = true;
                            return true;
                            // break 'leaf_summary; // Found work, move to yield queue processing
                        }
                        Some(false) | None => {
                            if self.empty_yielded(arena) > 0 {
                                found_work = true;
                            }
                            // CAS failed or empty - try yield queue
                            // if let Some(global_id) = self.yield_queue.pop() {
                            //     let _ = self.poll_yielded_task(arena, global_id);
                            // }
                        }
                    }
                }
            }
        }

        if !found_work {
            // PRIORITY 3: Steal from random leaves across entire arena
            // No partition boundaries - randomly sample leaves for better load balancing
            const MAX_STEAL_ATTEMPTS: usize = 8; // Limit attempts to avoid excessive scanning

            'steal_random_leaves: for _ in 0..MAX_STEAL_ATTEMPTS {
                // Pick a completely random leaf from the entire arena
                let leaf_idx = (self.selector.next() as usize) % leaf_count;
                let summary = arena.active_summary(leaf_idx);
                let summary_value = summary.load(Ordering::Acquire);

                self.stats.leaf_summary_checks += 1;
                if summary_value != 0 {
                    self.stats.leaf_summary_hits += 1;
                    let random_bit = (self.selector.next() & 63) as u64;
                    let signal_bit = crate::bits::find_nearest(summary_value, random_bit);

                    if signal_bit < 64 {
                        let signal_idx = signal_bit as usize;

                        self.stats.leaf_steal_attempts += 1;
                        match self.try_acquire_and_poll_from_signal(arena, leaf_idx, signal_idx) {
                            Some(true) => {
                                self.stats.leaf_steal_successes += 1;
                                found_work = true;
                                break 'steal_random_leaves;
                            }
                            Some(false) | None => {
                                if self.empty_yielded(arena) > 0 {
                                    found_work = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        // PRIORITY 4: Empty entire yield queue once
        if self.empty_yielded(arena) > 0 {
            found_work = true;
            return true;
        }
        // let mut max_pop = self.yield_queue.len() as i32;
        // while let Some(global_id) = self.yield_queue.pop() {
        //     self.poll_yielded_task(arena, global_id);
        //     found_work = true;
        //     max_pop -= 1;
        //     if max_pop <= 0 {
        //         break;
        //     }
        // }

        // PRIORITY 5: Steal from other workers' yield queues
        if let Some(stolen_id) = self.try_steal_yielded_task() {
            self.poll_yielded_task(arena, stolen_id);
            found_work = true;
        }

        if !found_work {
            self.stats.empty_scans += 1;
        }

        found_work
    }

    /// Blocking variant of `try_poll_with_leaf_partition`.
    ///
    /// This method implements efficient thread parking when no work is available,
    /// allowing workers to sleep instead of spinning and wasting CPU.
    ///
    /// # Algorithm
    ///
    /// 1. **Fast path**: Try normal poll (zero overhead when work available)
    /// 2. **Spin phase**: Optionally spin N times before giving up (low latency)
    /// 3. **Global idle check**: Scan all leaf summaries to verify truly idle
    /// 4. **Park phase**: Sleep on the active summary tree until work arrives
    ///
    /// # Performance
    ///
    /// - **Work available**: Identical to `try_poll_with_leaf_partition()` (37.9ns)
    /// - **Brief wait**: Spin overhead (~1-2ns per spin iteration)
    /// - **Long idle**: Park/wake overhead (~1-10µs, but saves 100% CPU)
    ///
    /// # Arguments
    ///
    /// * `arena` - The task arena to poll from
    /// * `config` - Configuration controlling spin/park behavior
    ///
    /// # Returns
    ///
    /// - `true` if work was found and processed
    /// - `false` if timeout expired or no work available
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Low-latency mode (spin before parking)
    /// let config = PollConfig::low_latency();
    /// while worker.poll_with_leaf_partition_blocking(&arena, &config) {
    ///     // Process work...
    /// }
    ///
    /// // Immediate parking (most CPU efficient)
    /// let config = PollConfig::park_immediately();
    /// while worker.poll_with_leaf_partition_blocking(&arena, &config) {
    ///     // Process work...
    /// }
    /// ```
    pub fn poll_with_leaf_partition_blocking(
        &mut self,
        arena: &MmapExecutorArena,
        leaf_count: usize,
        config: &PollConfig,
    ) -> bool {
        // FAST PATH: Direct poll - zero overhead when work is available
        // This maintains the 37.9ns/poll performance from benchmarks
        if self.try_poll_with_leaf_partition(arena, leaf_count) {
            return true;
        }

        // SPIN PHASE: Brief spinning for low latency (optional)
        // Trade a small amount of CPU time to avoid parking overhead
        if let Some(spin_count) = config.spin_before_park {
            for _ in 0..spin_count {
                if self.try_poll_with_leaf_partition(arena, leaf_count) {
                    return true; // Found work during spin
                }
                std::hint::spin_loop(); // CPU hint: we're spinning
            }
        }

        // Check if we should even attempt to park (timeout = 0 means non-blocking)
        if let Some(timeout) = config.park_timeout {
            if timeout.is_zero() {
                return false; // Non-blocking mode, return immediately
            }
        }

        // PARK PHASE: Block until a permit becomes available
        let tree = arena.active_tree();

        // If the global summary is empty and our yield queue is empty, nothing to do.
        if tree.snapshot_summary() == 0 && self.yield_state.is_empty() {
            return false;
        }

        // Try to acquire a permit (blocking)
        // acquire() and acquire_timeout() internally manage sleeper registration
        let acquired_permit = if let Some(timeout) = config.park_timeout {
            tree.acquire_timeout(timeout)
        } else {
            tree.acquire();
            true
        };

        // After waking up, try to find work
        if acquired_permit {
            // We consumed a permit, so work should exist
            self.try_poll_with_leaf_partition(arena, leaf_count)
        } else {
            // Timeout expired without acquiring permit
            false
        }
    }

    #[inline]
    pub fn empty_yielded(&mut self, arena: &MmapExecutorArena) -> usize {
        let mut count = 0;
        while let Some(global_id) = self.yield_queue.pop() {
            self.yield_state.decrement();
            if !self.poll_yielded_task(arena, global_id) {
                return count;
            }
            count += 1;
        }
        count
    }

    /// Tries to poll a task from a specific signal index (optimized single-signal variant).
    ///
    /// This is a specialized version of `try_poll_task` that only checks a single TaskSignal
    /// instead of scanning all signals. Much faster for dedicated-worker scenarios where
    /// each worker has its own signal with zero contention.
    ///
    /// # Algorithm
    ///
    /// 1. Check if signal has any tasks (atomic load)
    /// 2. Select random bit from signal
    /// 3. Try to acquire the bit (CAS)
    /// 4. If successful, poll the task inline
    ///
    /// # Parameters
    ///
    /// - `arena`: Task arena containing tasks
    /// - `leaf_idx`: Leaf index to check
    /// - `signal_idx`: Specific signal index to poll from
    ///
    /// # Returns
    ///
    /// - `true`: Successfully found and polled a task
    /// - `false`: No task available or CAS failed
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Dedicated worker for signal 0 in leaf 0
    /// loop {
    ///     if !worker.try_poll_signal(&arena, 0, 0) {
    ///         // No work available
    ///         break;
    ///     }
    /// }
    /// ```
    #[inline(always)]
    pub fn try_poll_signal(
        &mut self,
        arena: &MmapExecutorArena,
        leaf_idx: usize,
        signal_idx: usize,
    ) -> bool {
        let signals = arena.active_signals(leaf_idx);
        let signal = unsafe { &*signals.add(signal_idx) };

        // Check if signal has any tasks
        let signal_value = signal.load(Ordering::Acquire);
        if signal_value == 0 {
            return false; // Signal is empty
        }

        // Find a random bit to try
        let random_bit = (self.selector.next() & 63) as u64;
        let bit = crate::bits::find_nearest(signal_value, random_bit);

        if bit >= 64 {
            return false; // No valid bit found
        }

        // Try to acquire this bit
        if !signal.acquire(bit) {
            self.stats.cas_failures += 1;
            return false; // CAS failed (contention)
        }

        // Successfully acquired - compute slot and poll
        let slot_idx = signal_idx * 64 + bit as usize;
        let global_id = arena.compose_id(leaf_idx, slot_idx);
        let task = unsafe { arena.task(leaf_idx, slot_idx) };

        // Mark as executing
        task.begin();
        self.stats.tasks_polled += 1;
        self.stats.signal_polls += 1;

        // Clear yielded flag before poll
        unsafe { task.clear_yielded() };

        // Create yield waker and poll
        let waker = unsafe { task.waker_yield() };
        let mut cx = Context::from_waker(&waker);
        let poll_result = unsafe { task.poll_future(&mut cx) };

        // Handle result inline
        match poll_result {
            Some(Poll::Ready(_)) => {
                // Task completed - release signal bit
                task.take_future();
                task.finish();
                self.stats.completed_count += 1;
            }
            Some(Poll::Pending) => {
                // Check if yielded or waiting
                if unsafe { task.take_yielded() } {
                    // Task yielded - add to yield queue WITHOUT releasing signal bit
                    // self.stats.yielded_count += 1;
                    // self.yield_queue.push(global_id);

                    task.finish_and_schedule();
                    self.stats.waiting_count += 1;
                    // Signal bit stays acquired - DON'T call finish()
                } else {
                    // Waiting for external event - release signal bit
                    task.finish();
                    self.stats.waiting_count += 1;
                }
            }
            None => {
                // No future attached (shouldn't happen)
                task.finish();
            }
        }

        true // Successfully polled a task
    }

    /// Tries to acquire and poll a task from a specific signal.
    ///
    /// # Returns
    ///
    /// - `Some(true)`: Successfully found and polled a task
    /// - `Some(false)`: Signal was empty (no tasks available)
    /// - `None`: CAS failed (contention)
    #[inline]
    fn try_acquire_and_poll_from_signal(
        &mut self,
        arena: &MmapExecutorArena,
        leaf_idx: usize,
        signal_idx: usize,
    ) -> Option<bool> {
        let signals = arena.active_signals(leaf_idx);
        let signal = unsafe { &*signals.add(signal_idx) };

        // Check if signal has any tasks
        let signal_value = signal.load(Ordering::Acquire);
        if signal_value == 0 {
            return Some(false); // Signal is empty
        }

        // Find a random bit to try
        let random_bit = (self.selector.next() & 63) as u64;
        let bit = crate::bits::find_nearest(signal_value, random_bit);

        if bit >= 64 {
            return Some(false); // No valid bit found
        }

        // Try to acquire this bit
        if !signal.acquire(bit) {
            self.stats.cas_failures += 1;
            return None; // CAS failed (contention)
        }

        // Successfully acquired - compute slot and poll
        let slot_idx = signal_idx * 64 + bit as usize;
        let global_id = arena.compose_id(leaf_idx, slot_idx);
        let task = unsafe { arena.task(leaf_idx, slot_idx) };

        // Mark as executing
        task.begin();
        self.stats.tasks_polled += 1;

        // Clear yielded flag before poll
        unsafe { task.clear_yielded() };

        // Create yield waker and poll
        let waker = unsafe { task.waker_yield() };
        let mut cx = Context::from_waker(&waker);
        let poll_result = unsafe { task.poll_future(&mut cx) };

        // Handle result inline
        match poll_result {
            Some(Poll::Ready(_)) => {
                // Task completed - release signal bit
                task.take_future();
                task.finish();
                self.stats.completed_count += 1;
            }
            Some(Poll::Pending) => {
                // Check if yielded or waiting
                if unsafe { task.take_yielded() } {
                    task.finish_and_schedule();
                    // Task yielded - add to yield queue WITHOUT releasing signal bit
                    // self.stats.yielded_count += 1;
                    // self.yield_queue.push(global_id);
                    // self.yield_state.increment();
                    // Signal bit stays acquired - DON'T call finish()
                } else {
                    // Waiting for external event - release signal bit
                    task.finish();
                    self.stats.waiting_count += 1;
                }
            }
            None => {
                // No future attached (shouldn't happen)
                task.finish();
            }
        }

        arena.handle_signal_deactivation(leaf_idx, signal_idx, signal);

        Some(true) // Successfully polled a task
    }

    /// Polls a yielded task from the yield queue.
    ///
    /// This is a fast path for tasks that already have their signal bit acquired.
    /// No CAS operation needed, just direct polling.
    ///
    /// # Returns
    ///
    /// - `true`: Successfully polled the task
    /// - `false`: Task not found or polling failed (shouldn't happen)
    #[inline(always)]
    fn poll_yielded_task(&mut self, arena: &MmapExecutorArena, global_id: u64) -> bool {
        let (leaf_idx, slot_idx) = arena.decompose_id(global_id);
        let task = unsafe { arena.task(leaf_idx, slot_idx) };

        // Task already has signal bit acquired, just mark as executing and poll
        // task.begin();
        self.stats.tasks_polled += 1;
        self.stats.yield_queue_polls += 1;

        let signals = arena.active_signals(leaf_idx);
        let signal_idx = slot_idx / 64;
        let signal = unsafe { &*signals.add(signal_idx) };

        // Clear yielded flag before poll
        unsafe { task.clear_yielded() };

        // Create yield waker and poll
        let waker = unsafe { task.waker_yield() };
        let mut cx = Context::from_waker(&waker);
        let poll_result = unsafe { task.poll_future(&mut cx) };

        // Handle result
        match poll_result {
            Some(Poll::Ready(_)) => {
                // Task completed - NOW we release signal bit
                task.take_future();
                task.finish();
                self.stats.completed_count += 1;
            }
            Some(Poll::Pending) => {
                if unsafe { task.take_yielded() } {
                    // Yielded AGAIN - push back to yield queue
                    self.stats.yielded_count += 1;
                    self.yield_queue.push(global_id);
                    // self.yield_state.increment();
                    // Keep signal bit acquired
                } else {
                    // Task now waiting for external event - release signal bit
                    task.finish();
                    self.stats.waiting_count += 1;
                }
            }
            None => {
                // No future attached (shouldn't happen)
                task.finish();
            }
        }

        arena.handle_signal_deactivation(leaf_idx, signal_idx, signal);

        true
    }

    /// Tries to steal a yielded task from another worker's yield queue.
    ///
    /// This is called when this worker has no local work and wants to
    /// help process yielded tasks from busy workers.
    ///
    /// # Returns
    ///
    /// - `Some(global_id)`: Successfully stole a yielded task
    /// - `None`: No yielded tasks available to steal
    #[inline]
    fn try_steal_yielded_task(&mut self) -> Option<u64> {
        for entry in &self.yield_stealers {
            self.stats.steal_attempts += 1;
            match entry.stealer.steal() {
                Steal::Success(global_id) => {
                    self.stats.steal_successes += 1;
                    entry.state.decrement();
                    return Some(global_id);
                }
                Steal::Empty => continue,
                Steal::Retry => continue,
            }
        }
        None
    }

    /// Resets the worker statistics.
    pub fn reset_stats(&mut self) {
        self.stats.reset();
    }

    /// Gets the worker index.
    pub fn worker_idx(&self) -> usize {
        self.worker_idx
    }

    /// Gets the partition range for this worker.
    pub fn partition_range(&self) -> (usize, usize) {
        (self.partition_start, self.partition_end)
    }

    /// Gets the total number of signals per leaf.
    pub fn signals_per_leaf(&self) -> usize {
        self.signals_per_leaf
    }
}
