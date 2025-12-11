use super::scheduler::{
    Scheduler, TimerSchedule, WorkerBus, WorkerHealthSnapshot, WorkerMessage, WorkerStats,
    drop_future,
};
use crate::PopError;
use crate::future::waker;
use crate::runtime::deque::Worker as YieldWorker;
use crate::runtime::preemption::GeneratorYieldReason;
use crate::runtime::task::{GeneratorOwnership, Task, TaskHandle};
use crate::runtime::timer::wheel::TimerWheel;
use crate::runtime::timer::{Timer, TimerHandle};
use crate::runtime::{mpsc, preemption};
use crate::utils::bits;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

const TIMER_EXPIRE_BUDGET: usize = 4096;
const DEFAULT_WAKE_BURST: usize = 4;
#[derive(Clone, Copy, Debug, Default)]
pub struct WakeStats {
    pub release_calls: u64,
    pub released_permits: u64,
    pub last_backlog: usize,
    pub max_backlog: usize,
    pub queue_release_calls: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct WaitStrategy {
    pub spin_before_sleep: usize,
    pub park_timeout: Option<Duration>,
}

impl WaitStrategy {
    pub fn new(spin_before_sleep: usize, park_timeout: Option<Duration>) -> Self {
        Self {
            spin_before_sleep,
            park_timeout,
        }
    }

    pub fn non_blocking() -> Self {
        Self::new(0, Some(Duration::from_secs(0)))
    }

    pub fn park_immediately() -> Self {
        Self::new(0, None)
    }
}

impl Default for WaitStrategy {
    fn default() -> Self {
        Self::new(0, Some(Duration::from_millis(2000)))
    }
}
pub struct Worker<'a> {
    pub(crate) scheduler: Arc<Scheduler>,
    pub(crate) receiver: &'a mut mpsc::Receiver<WorkerMessage>,
    pub(crate) wait_strategy: WaitStrategy,
    pub(crate) shutdown: &'a AtomicBool,
    pub(crate) yield_queue: &'a YieldWorker<TaskHandle>,
    pub(crate) timer_wheel: &'a mut TimerWheel<TimerHandle>,
    pub(crate) bus: &'static dyn WorkerBus,
    pub(crate) wake_stats: WakeStats,
    pub(crate) stats: WorkerStats,
    pub(crate) partition_start: usize,
    pub(crate) partition_end: usize,
    pub(crate) partition_len: usize,
    pub(crate) cached_worker_count: usize,
    pub(crate) wake_burst_limit: usize,
    pub(crate) worker_id: u32,
    pub(crate) timer_resolution_ns: u64,
    pub(crate) timer_output: Vec<(u64, u64, TimerHandle)>,
    pub(crate) message_batch: Box<[WorkerMessage]>,
    pub(crate) rng: crate::utils::Random,
    pub(crate) current_task: *mut Task,
    pub(crate) current_scope: *mut (),
    /// Preemption flag for this worker - set by signal handler (Unix) or timer thread (Windows)
    pub(crate) preemption_requested: AtomicBool,
    /// Per-worker EventLoop for async I/O operations (boxed to avoid stack overflow)
    pub(crate) io: Box<crate::runtime::worker::io::IoDriver>,
    /// I/O budget per run_once iteration (prevents I/O from starving tasks)
    pub(crate) io_budget: usize,

    pub(crate) panic_reason: Option<Box<dyn std::any::Any + Send>>,
}

impl Drop for Worker<'_> {
    /// Cleans up the worker when it goes out of scope.
    ///
    /// Copies worker stats back to the service for later retrieval.
    /// The EventLoop owns its Waker, so proper drop order is ensured:
    /// Waker drops before Poll when EventLoop drops.
    fn drop(&mut self) {
        // Copy stats back to the service
        let worker_id = self.worker_id as usize;
        *self.scheduler.worker_stats[worker_id].lock() = self.stats.clone();
        tracing::trace!(
            "[worker {}] dropped with stats: {:?}",
            self.worker_id,
            self.stats
        );
    }
}

/// Context tracking generator state during worker execution.
///
/// This struct captures whether the worker's generator was pinned to a task
/// and provides information needed for proper cleanup and error handling.
///
/// Note: Panic handling is delegated to the generator's own mechanism (context.err).
/// We don't use catch_unwind around generator.resume() because it interferes with
/// the generator's context switching. Panics inside generators will propagate up
/// through the generator's error handling.
struct RunContext {
    /// Generator was pinned to a task - worker must create new generator.
    pinned: bool,
}

impl RunContext {
    fn new() -> Self {
        Self { pinned: false }
    }

    /// Mark that the generator was pinned to a task.
    #[inline]
    fn mark_pinned(&mut self) {
        self.pinned = true;
    }

    /// Check if we should exit the generator loop.
    #[inline]
    fn should_exit_generator(&self) -> bool {
        self.pinned
    }
}

impl<'a> Worker<'a> {
    #[inline]
    pub fn stats(&self) -> &WorkerStats {
        &self.stats
    }

    /// Checks if this worker has any work to do (tasks, yields, or messages).
    /// Returns true if the worker should continue running, false if it can park.
    #[inline]
    fn has_work(&self) -> bool {
        let waker = &self.scheduler.wakers[self.worker_id as usize];

        // Check status bits (fast path):
        // - bit 63: yield queue has items
        // - bit 62: partition cache reports tasks (synced from SummaryTree)
        let status = waker.status();
        if status != 0 {
            return true;
        }

        // Check summary (cross-worker signals like messages, timers)
        let summary = waker.snapshot_summary();
        if summary != 0 {
            return true;
        }

        // Check if permits are available (missed wake scenario)
        if waker.permits() > 0 {
            return true;
        }

        false
    }

    pub fn set_wait_strategy(&mut self, strategy: WaitStrategy) {
        self.wait_strategy = strategy;
    }

    #[inline(always)]
    pub fn now_ns(&self) -> u64 {
        self.timer_wheel.now_ns()
    }

    #[inline(always)]
    fn poll_system_tasks(&mut self, run_ctx: &mut RunContext) -> usize {
        let mut count = self.poll_io();

        if self.poll_timers() {
            count += 1;
        }

        if self.process_messages() {
            count += 1;
        }
        count
    }

    #[inline(always)]
    fn poll_tasks(&mut self, run_ctx: &mut RunContext) -> usize {
        let mut now = self.now_ns();
        let ticks = 0u64;
        let mut count = 0usize;
        let mut miss_count = 0usize;
        let leaf_count = self.scheduler.arena.leaf_count();
        const CONSECUTIVE_MISSES: usize = 10;

        count += self.poll_system_tasks(run_ctx);

        for _ in 0..10 {
            if self.try_partition_random(run_ctx) {
                // if self.try_any_partition_random(run_ctx, leaf_count) {
                count += 1;
                miss_count = 0;
                if run_ctx.should_exit_generator() {
                    return count;
                }
            } else {
                miss_count += 1;
                // core::hint::spin_loop();
            }
            if CONSECUTIVE_MISSES >= miss_count {
                break;
            }
            let next_now = self.now_ns();
            if now != next_now {
                now = next_now;
                if self.shutdown.load(Ordering::Relaxed) {
                    self.current_scope = core::ptr::null_mut();
                    return count;
                }
                // ticks += 1;
                count += self.poll_system_tasks(run_ctx);
                // count += self.poll_yield(run_ctx, self.yield_queue.len());
                // count += self.poll_yield(run_ctx, 1024*1024);
                // if run_ctx.should_exit_generator() {
                //     return count;
                // }
            }
        }

        count += self.poll_system_tasks(run_ctx);
        // count += self.poll_yield(run_ctx, self.yield_queue.len());

        count
    }

    #[inline(always)]
    fn run_once(&mut self, run_ctx: &mut RunContext) -> bool {
        let mut did_work = false;

        // if self.poll_system_tasks(run_ctx) > 0 {
        //     if run_ctx.should_exit_generator() {
        //         return true;
        //     }
        //     did_work = true;
        // }

        let leaf_count = self.scheduler.arena.leaf_count();
        let worker_count = self.scheduler.worker_count;
        let partition_len = leaf_count / worker_count;
        let partition_start = self.worker_id as usize * partition_len;
        let mut now = self.timer_wheel.now_ns();
        let ticks = 0u64;
        let start_idx = self.next_u64();
        let mut empty_tries = 0u64;
        let mut full_empty_tries = 0u64;
        const MAX_EMPTY_TRIES: u64 = 8;
        const ALLOW_WORK_STEALING: bool = false;
        const WORK_STEALING_CADENCE: usize = 127;
        // let partition_start = self.partition_start;
        // let partition_len = self.partition_len.min(1);
        for i in 0..(MAX_EMPTY_TRIES * 2) {
            let next_idx = self.next_u64() as usize;
            if self.process_leaf(run_ctx, partition_start + (next_idx % partition_len)) {
                // if self.process_leaf(run_ctx, 0 + (next_idx % leaf_count)) {
                // if self.process_leaf(run_ctx, i % leaf_count) {
                did_work = true;
                // self.stats.leaf_steal_successes += 1;
                if run_ctx.should_exit_generator() {
                    return did_work;
                }

                empty_tries = 0;
            } else {
                empty_tries += 1;
            }

            if ALLOW_WORK_STEALING {
                if next_idx & WORK_STEALING_CADENCE == 0 {
                    if self.shutdown.load(Ordering::Relaxed) {
                        self.current_scope = core::ptr::null_mut();
                        return did_work;
                    }

                    for x in 0..4 {
                        self.stats.steal_attempts += 1;
                        if self.process_leaf(run_ctx, next_idx % leaf_count) {
                            self.stats.steal_successes += 1;
                            // if self.process_leaf(run_ctx, 0 + (next_idx % leaf_count)) {
                            // if self.process_leaf(run_ctx, i % leaf_count) {
                            did_work = true;
                            // self.stats.leaf_steal_successes += 1;
                            if run_ctx.should_exit_generator() {
                                return did_work;
                            }

                            full_empty_tries = 0;
                            break;
                        } else {
                            full_empty_tries += 1;
                        }
                    }
                }
            }

            // if self.poll_io() > 0 {
            //     if run_ctx.should_exit_generator() {
            //         return true;
            //     }
            //     did_work = true;
            // }

            // if next_idx & 2047 == 0 {
            //     if self.poll_system_tasks(run_ctx) > 0 {
            //         if run_ctx.should_exit_generator() {
            //             return true;
            //         }
            //         did_work = true;
            //     }

            //     if self.shutdown.load(Ordering::Relaxed) {
            //         self.current_scope = core::ptr::null_mut();
            //         return did_work;
            //     }
            // }

            if ALLOW_WORK_STEALING {
                if empty_tries >= MAX_EMPTY_TRIES {
                    for x in 0..MAX_EMPTY_TRIES {
                        self.stats.steal_attempts += 1;
                        if self.process_leaf(run_ctx, next_idx % leaf_count) {
                            self.stats.steal_successes += 1;
                            // if self.process_leaf(run_ctx, 0 + (next_idx % leaf_count)) {
                            // if self.process_leaf(run_ctx, i % leaf_count) {
                            did_work = true;
                            // self.stats.leaf_steal_successes += 1;
                            if run_ctx.should_exit_generator() {
                                return did_work;
                            }

                            full_empty_tries = 0;
                        } else {
                            full_empty_tries += 1;
                        }
                    }

                    if full_empty_tries >= MAX_EMPTY_TRIES {
                        break;
                    }
                }
            }

            // // if self.try_partition_random(run_ctx) {
            // if self.try_any_partition_random(run_ctx, self.service.arena.leaf_count()) {
            //     did_work = true;
            // }

            // Ensure we give system tasks and work stealing some CPU on tick boundaries
            // let next_now = self.timer_wheel.now_ns();
            // if now != next_now {
            //     now = next_now;

            //     if self.shutdown.load(Ordering::Relaxed) {
            //         self.current_scope = core::ptr::null_mut();
            //         return did_work;
            //     }

            //     if self.poll_system_tasks(run_ctx) > 0 {
            //         if run_ctx.should_exit_generator() {
            //             return true;
            //         }
            //         did_work = true;
            //     }

            //     if ALLOW_WORK_STEALING {
            //         for x in 0..8 {
            //             let next_idx = self.next_u64() as usize;
            //             self.stats.steal_attempts += 1;
            //             if self.process_leaf(run_ctx, next_idx % leaf_count) {
            //                 self.stats.steal_successes += 1;
            //                 // if self.process_leaf(run_ctx, 0 + (next_idx % leaf_count)) {
            //                 // if self.process_leaf(run_ctx, i % leaf_count) {
            //                 did_work = true;
            //                 // self.stats.leaf_steal_successes += 1;
            //                 if run_ctx.should_exit_generator() {
            //                     return did_work;
            //                 }
            //                 break;
            //             }
            //         }
            //     }

            //     // let yielded = self.poll_yield(run_ctx, self.yield_queue.len());
            //     // // let yielded = self.poll_yield(run_ctx, 1024*32);
            //     // if yielded > 0 {
            //     //     did_work = true;
            //     // }
            //     // if run_ctx.should_exit_generator() {
            //     //     return did_work;
            //     // }
            // }
        }

        // if self.poll_system_tasks(run_ctx) > 0 {
        //     if run_ctx.should_exit_generator() {
        //         return true;
        //     }
        //     did_work = true;
        // }

        // let yielded = self.poll_yield(run_ctx, 1024*4096);
        // // let yielded = self.poll_yield(run_ctx, 1024*32);
        // if yielded > 0 {
        //     did_work = true;
        // }
        // if run_ctx.should_exit_generator() {
        //     return did_work;
        // }

        // if self.try_partition_random(leaf_count) {
        //     did_work = true;
        // } else if self.try_partition_linear(leaf_count) {
        //     did_work = true;
        // }

        // let rand = self.next_u64();

        // if !did_work && rand & FULL_SUMMARY_SCAN_CADENCE_MASK == 0 {
        //     let leaf_count = self.service.arena().leaf_count();
        //     if self.try_any_partition_random(run_ctx, leaf_count) {
        //         did_work = true;
        //     }
        //     if run_ctx.should_exit_generator() {
        //         return did_work;
        //     }
        //     // else if self.try_any_partition_linear(leaf_count) {
        //     // did_work = true;
        //     // }
        // }

        if !did_work {
            // if self.poll_yield(run_ctx, self.yield_queue.len() as usize) > 0 {
            //     did_work = true;
            // }
            // if run_ctx.should_exit_generator() {
            //     return did_work;
            // }

            // if self.poll_yield_steal(run_ctx, 256) > 0 {
            //     did_work = true;
            // }
        }

        did_work
    }

    #[inline(always)]
    fn run_once_exhaustive(&mut self, run_ctx: &mut RunContext) -> bool {
        let mut did_work = false;

        if self.poll_io() > 0 {
            did_work = true;
        }

        if self.poll_timers() {
            did_work = true;
        }

        // Process messages first (including shutdown signals)
        if self.process_messages() {
            did_work = true;
        }

        // Poll all yielded tasks first - they're ready to run
        // let yielded = self.poll_yield(run_ctx, self.yield_queue.len());
        // if yielded > 0 {
        //     did_work = true;
        // }
        if run_ctx.should_exit_generator() {
            return did_work;
        }

        if self.try_partition_random(run_ctx) {
            did_work = true;
        }
        if run_ctx.should_exit_generator() {
            return did_work;
        }

        if self.try_partition_random(run_ctx) {
            did_work = true;
        } else if self.try_partition_linear(run_ctx) {
            did_work = true;
        }
        if run_ctx.should_exit_generator() {
            return did_work;
        }

        if !did_work {
            let leaf_count = self.scheduler.arena().leaf_count();
            if self.try_any_partition_random(run_ctx, leaf_count) {
                did_work = true;
            } else if self.try_any_partition_linear(run_ctx, leaf_count) {
                did_work = true;
            }
            if run_ctx.should_exit_generator() {
                return did_work;
            }
        }

        if !did_work {
            // if self.poll_yield(run_ctx, self.yield_queue.len() as usize) > 0 {
            //     did_work = true;
            // }
            if run_ctx.should_exit_generator() {
                return did_work;
            }
            if self.poll_yield_steal(run_ctx, 1) > 0 {
                did_work = true;
            }
            if run_ctx.should_exit_generator() {
                return did_work;
            }
        }

        did_work
    }

    #[inline]
    fn run_last_before_park(&mut self) -> bool {
        false
    }

    pub(crate) fn run(&mut self) {
        const STACK_SIZE: usize = 2 * 1024 * 1024; // 2MB stack per generator

        // Outer loop: manages generator lifecycle
        // Creates new generators when current one gets pinned
        while !self.shutdown.load(Ordering::Relaxed) {
            // Use raw pointer for generator closure
            let worker_ptr = self as *mut Self;

            // Convert pointer to usize to completely erase type and make it Send
            // This is safe because Worker is single-threaded and we never actually
            // send across threads
            let worker_addr = worker_ptr as usize;

            let mut generator = crate::generator::Gn::<()>::new_scoped_opt(
                STACK_SIZE,
                move |mut scope| -> GeneratorYieldReason {
                    // Wrap in catch_unwind to ensure clean stack behavior
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // SAFETY: worker_addr is the address of a valid Worker that lives for the generator's lifetime
                        let worker = unsafe { &mut *(worker_addr as *mut Worker<'_>) };
                        worker.stats.generators_created += 1;
                        let mut spin_count = 0;
                        let waker_id = worker.worker_id as usize;

                        // Register this generator's scope for non-cooperative preemption
                        let scope_ptr = &mut scope as *mut _ as *mut ();

                        // Store in thread-local for signal handler
                        worker.current_scope = scope_ptr;

                        let mut run_ctx = RunContext::new();

                        let iteration = 0u64;

                        let mut now = worker.timer_wheel.now_ns();
                        // let mut now = std::time::Instant::now();

                        // Inner loop: runs inside generator context
                        loop {
                            // Check for shutdown
                            if worker.shutdown.load(Ordering::Relaxed) {
                                worker.current_scope = core::ptr::null_mut();
                                return GeneratorYieldReason::Shutdown;
                            }

                            // Process one iteration of work
                            // let mut progress = worker.poll_tasks(&mut run_ctx) > 0;
                            let mut progress = worker.run_once(&mut run_ctx);

                            // Check if we need to exit the generator
                            if run_ctx.should_exit_generator() {
                                worker.current_scope = core::ptr::null_mut();

                                #[cfg(debug_assertions)]
                                tracing::trace!(
                                    "[worker {}] Generator pinned to task",
                                    worker.worker_id
                                );

                                // Worker's generator was pinned to a task - exit so we can
                                // create a new generator for the worker
                                return crate::runtime::preemption::GeneratorYieldReason::Cooperative;
                            }

                            // if worker.poll_io() > 0 {
                            //     progress = true;
                            // }

                            // if !progress {
                            //     if worker.poll_system_tasks(&mut run_ctx) > 0 {
                            //         if run_ctx.should_exit_generator() {
                            //             worker.current_scope = core::ptr::null_mut();
                            //             return GeneratorYieldReason::Cooperative;
                            //         }
                            //         progress = true;
                            //     }
                            // }

                            // if worker.poll_system_tasks(&mut run_ctx) > 0 {
                            //     if run_ctx.should_exit_generator() {
                            //         worker.current_scope = core::ptr::null_mut();
                            //         return GeneratorYieldReason::Cooperative;
                            //     }
                            //     progress = true;
                            // }

                            // if worker.poll_io() > 0 {
                            //     progress = true;
                            // }
                            // Ensure we give system tasks and work stealing some CPU on tick boundaries
                            // let next_now = Instant::now();
                            let next_now = worker.timer_wheel.now_ns();
                            if now != next_now {
                                // tracing::info!("next_now tick");
                                now = next_now;

                                if worker.shutdown.load(Ordering::Relaxed) {
                                    worker.current_scope = core::ptr::null_mut();
                                    return GeneratorYieldReason::Cooperative;
                                }

                                if worker.poll_io() > 0 {
                                    progress = true;
                                }

                                worker.run_once_exhaustive(&mut run_ctx);

                                if worker.poll_timers() {
                                    progress = true;
                                }

                                if worker.process_messages() {
                                    progress = true;
                                }

                                // if worker.poll_system_tasks(&mut run_ctx) > 0 {
                                //     if run_ctx.should_exit_generator() {
                                //         worker.current_scope = core::ptr::null_mut();
                                //         return GeneratorYieldReason::Cooperative;
                                //     }
                                //     progress = true;
                                // }

                                // if true {
                                //     for x in 0..8 {
                                //         let next_idx = self.next_u64() as usize;
                                //         self.stats.steal_attempts += 1;
                                //         if self.process_leaf(run_ctx, next_idx % leaf_count) {
                                //             self.stats.steal_successes += 1;
                                //             // if self.process_leaf(run_ctx, 0 + (next_idx % leaf_count)) {
                                //             // if self.process_leaf(run_ctx, i % leaf_count) {
                                //             did_work = true;
                                //             // self.stats.leaf_steal_successes += 1;
                                //             if run_ctx.should_exit_generator() {
                                //                 self.current_scope = core::ptr::null_mut();
                                //                 return GeneratorYieldReason::Cooperative;
                                //             }
                                //             break;
                                //         }
                                //     }
                                // }
                            }

                            // if worker.scheduler.wakers[waker_id].partition_summary() > 0 {
                            //     progress = true;
                            // }

                            if !progress {
                                spin_count += 1;

                                // if spin_count >= worker.wait_strategy.spin_before_sleep {
                                //     core::hint::spin_loop();

                                //     if progress {
                                //         spin_count = 0;
                                //         continue;
                                //     }
                                // } else if spin_count < worker.wait_strategy.spin_before_sleep {
                                //     core::hint::spin_loop();
                                //     continue;
                                // }

                                progress = worker.run_once_exhaustive(&mut run_ctx);

                                // if progress {
                                //     spin_count = 0;
                                //     continue;
                                // }

                                // Calculate park duration considering timer deadlines
                                // let park_duration = worker.calculate_park_duration();
                                let park_duration = Some(Duration::from_millis(1));

                                // Park on WorkerWaker with timer-aware timeout
                                // On Windows, also use alertable wait for APC-based preemption
                                match park_duration {
                                    Some(duration) if duration.is_zero() => {
                                        // Timer already expired - process timers to avoid busy spin
                                        // This can happen if a timer fires between run_once and
                                        // calculate_park_duration
                                        if worker.poll_system_tasks(&mut run_ctx) > 0 {
                                            // Made progress, skip the spin
                                            continue;
                                        }
                                        // No timers actually ready - do a minimal yield to avoid
                                        // pegging CPU when timer_wheel.next_deadline() returns
                                        // stale data
                                        std::thread::yield_now();
                                    }
                                    Some(duration) => {
                                        // On Windows, WorkerWaker uses alertable waits to allow APC execution
                                        worker.scheduler.wakers[waker_id]
                                            .acquire_timeout_with_io(&mut worker.io, duration);
                                    }
                                    None => {
                                        // On Windows, WorkerWaker uses alertable waits to allow APC execution
                                        worker.scheduler.wakers[waker_id].acquire_timeout_with_io(
                                            &mut worker.io,
                                            Duration::from_millis(250),
                                        );
                                    }
                                }
                                spin_count = 0;
                            } else {
                                spin_count = 0;
                            }
                        }
                    }));

                    match result {
                        Ok(r) => r,
                        Err(panic_payload) => {
                            // Log the panic for debugging
                            if let Some(s) = panic_payload.downcast_ref::<&str>() {
                                tracing::error!("Worker generator panicked: {}", s);
                            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                                tracing::error!("Worker generator panicked: {}", s);
                            } else {
                                tracing::error!("Worker generator panicked with unknown payload");
                            }

                            let worker = unsafe { &mut *(worker_addr as *mut Worker<'_>) };
                            worker.panic_reason = Some(panic_payload);

                            // Return 2 to indicate panic occurred
                            return GeneratorYieldReason::Panic;
                        }
                    }
                },
            );

            // Drive the generator - it will run until:
            // 1. Shutdown is requested
            // 2. A task's generator gets pinned (run_ctx.pinned becomes true)
            // 3. Preemption occurs (signal handler pins generator and yields)
            let result = generator.resume().unwrap_or(GeneratorYieldReason::Panic);

            // Shutdown signal found
            if result == GeneratorYieldReason::Shutdown {
                self.stats.generators_dropped += 1;
                break;
            }

            // After resume returns, check if a generator yield (stack switch occurred)
            // without returning from poll.
            let task_ptr = self.current_task;
            if !task_ptr.is_null() {
                // Clear current_task since we've handled it
                self.current_task = ptr::null_mut();

                match result {
                    GeneratorYieldReason::Shutdown => unreachable!(),

                    GeneratorYieldReason::Cooperative => {
                        #[cfg(debug_assertions)]
                        tracing::trace!(
                            "[worker {}] task was pinned and cooperatively parked",
                            self.worker_id
                        );

                        // as_coroutine/sync_await already pinned the generator (scope pointer) to the task.
                        // We just need to:
                        // 1. Forget our generator variable (so it doesn't get dropped - task owns it now)
                        // 2. Re-enqueue the task to the yield queue if also yielded
                        let task = unsafe { &*task_ptr };
                        std::mem::forget(generator);

                        if task.is_yielded() {
                            task.record_yield();
                            let handle = TaskHandle::from_task(task);
                            self.yield_queue.push(handle);
                            self.scheduler.wakers[self.worker_id as usize].mark_yield();
                            self.stats.yielded_count += 1;
                        }
                    }

                    GeneratorYieldReason::Preempted => {
                        // rust_preemption_helper already pinned the generator (scope pointer) to the task.
                        // We just need to:
                        // 1. Forget our generator variable (so it doesn't get dropped - task owns it now)
                        // 2. Re-enqueue the task to the yield queue
                        let task = unsafe { &*task_ptr };
                        std::mem::forget(generator);

                        // Re-enqueue the preempted task to the yield queue
                        // Preempted tasks are "hot" and should run again soon
                        task.mark_yielded();
                        task.record_yield();
                        let handle = TaskHandle::from_task(task);
                        self.yield_queue.push(handle);
                        self.scheduler.wakers[self.worker_id as usize].mark_yield();
                        self.stats.yielded_count += 1;
                        self.stats.preempts += 1;

                        #[cfg(debug_assertions)]
                        tracing::trace!(
                            "[worker {}] preempted task re-enqueued to yield queue",
                            self.worker_id
                        );
                    }

                    GeneratorYieldReason::Panic => {
                        // let the generator drop
                    }
                }

                // Continue to create a new generator for the worker
                continue;
            }

            self.stats.generators_dropped += 1;
        }

        tracing::trace!("Worker is done running: {}", self.worker_id);
    }

    /// Calculate how long the worker can park, considering both the wait strategy
    /// and the next timer deadline.
    #[inline]
    fn calculate_park_duration(&mut self) -> Option<Duration> {
        // Check if we have pending timers
        if let Some(next_deadline_ns) = self.timer_wheel.next_deadline() {
            let now_ns = self.timer_wheel.now_ns();

            if next_deadline_ns > now_ns {
                let timer_duration_ns = next_deadline_ns - now_ns;
                let timer_duration = Duration::from_nanos(timer_duration_ns);

                // Use the minimum of wait_strategy timeout and timer deadline
                let duration = match self.wait_strategy.park_timeout {
                    Some(strategy_timeout) => Some(strategy_timeout.min(timer_duration)),
                    None => Some(timer_duration),
                };
                // tracing::info!("calculate_park_duration: {}", duration.unwrap().as_nanos());
                return duration;
            } else {
                // Timer already expired, don't sleep at all
                return Some(Duration::ZERO);
            }
        }

        // No timers scheduled - cap at 250ms to check for new timers periodically
        const MAX_PARK_DURATION: Duration = Duration::from_millis(250);
        match self.wait_strategy.park_timeout {
            Some(timeout) => Some(timeout.min(MAX_PARK_DURATION)),
            None => Some(MAX_PARK_DURATION),
        }
    }

    #[inline]
    fn process_messages(&mut self) -> bool {
        if self.scheduler.wakers[self.worker_id as usize].status() == 0 {
            return false;
        }
        let mut progress = false;
        loop {
            match self.receiver.try_pop() {
                Ok(message) => {
                    if self.handle_message(message) {
                        progress = true;
                    }
                }
                Err(PopError::Empty) | Err(PopError::Timeout) => {
                    // Queue is empty - mpsc signal bits will be cleared automatically
                    break;
                }
                Err(PopError::Closed) => break,
            }
        }
        progress
    }

    #[inline(always)]
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
            WorkerMessage::CancelTimer {
                worker_id,
                timer_id,
            } => self.cancel_remote_timer(worker_id, timer_id),
            WorkerMessage::ReportHealth => {
                self.handle_report_health();
                true
            }
            WorkerMessage::GracefulShutdown => {
                self.handle_graceful_shutdown();
                true
            }
            WorkerMessage::Shutdown => {
                self.shutdown.store(true, Ordering::Release);
                true
            }
            WorkerMessage::Noop => true,
        }
    }

    fn handle_timer_schedule(&mut self, timer: TimerSchedule) -> bool {
        let (handle, deadline_ns) = timer.into_parts();
        if handle.worker_id() != self.worker_id {
            return false;
        }
        self.enqueue_timer_entry(deadline_ns, handle).is_some()
    }

    fn handle_report_health(&mut self) {
        // Capture current state snapshot
        let snapshot = WorkerHealthSnapshot {
            worker_id: self.worker_id,
            timestamp_ns: self.timer_wheel.now_ns(),
            stats: self.stats.clone(),
            yield_queue_len: self.yield_queue.len(),
            mpsc_queue_len: 0, // TODO: Add len() method to mpsc::Receiver
            active_leaf_partitions: (self.partition_start..self.partition_end).collect(),
            has_work: self.has_work(),
        };

        // For now, just log the health snapshot
        // TODO: Send this back to supervisor via a response channel
        tracing::trace!(
            "[worker {}] Health: tasks_polled={}, yield_queue={}, mpsc_queue={}, partitions={:?}, has_work={}",
            snapshot.worker_id,
            snapshot.stats.tasks_polled,
            snapshot.yield_queue_len,
            snapshot.mpsc_queue_len,
            snapshot.active_leaf_partitions,
            snapshot.has_work
        );
        let _ = snapshot; // Suppress unused warning in release builds
    }

    fn handle_graceful_shutdown(&mut self) {
        // Graceful shutdown: just process a few more iterations then shutdown
        // The worker loop will naturally drain queues during normal operation

        #[cfg(debug_assertions)]
        {
            tracing::trace!(
                "[worker {}] Received graceful shutdown signal",
                self.worker_id
            );
        }

        // Process any remaining messages
        loop {
            match self.receiver.try_pop() {
                Ok(message) => match message {
                    WorkerMessage::Shutdown | WorkerMessage::GracefulShutdown => break,
                    _ => {
                        self.handle_message(message);
                    }
                },
                Err(_) => break,
            }
        }

        // Set shutdown flag - worker loop will finish current work before exiting
        self.shutdown.store(true, Ordering::Release);

        #[cfg(debug_assertions)]
        {
            tracing::trace!("[worker {}] Set shutdown flag", self.worker_id);
        }
    }

    #[inline]
    fn poll_io(&mut self) -> usize {
        // tracing::trace!(
        //     "[worker {}] Polling event loop. Sockets: {}",
        //     self.worker_id,
        //     self.io.socket_count()
        // );
        match self.io.poll_once(Some(std::time::Duration::ZERO)) {
            Ok(num_events) => {
                if num_events > 0 {
                    tracing::trace!("[worker {}] Poll got {} events", self.worker_id, num_events);
                }
                num_events
            }
            Err(err) => {
                log::error!("EventLoop error: {}", err);
                0
            }
        }
    }

    /// Schedule a timer directly at the given deadline (for cross-worker messages)
    fn enqueue_timer_entry(&mut self, deadline_ns: u64, handle: TimerHandle) -> Option<u64> {
        match self.timer_wheel.schedule_timer(deadline_ns, handle) {
            Ok(timer_id) => Some(timer_id),
            Err(_) => None, // Log error in debug mode
        }
    }

    /// Schedule a timer using best-fit strategy
    /// Returns (timer_id, wheel_deadline_ns) if successful
    fn enqueue_timer_entry_best_fit(
        &mut self,
        target_deadline_ns: u64,
        handle: TimerHandle,
    ) -> Option<(u64, u64)> {
        match self
            .timer_wheel
            .schedule_timer_best_fit(target_deadline_ns, handle)
        {
            Ok((timer_id, wheel_deadline_ns)) => Some((timer_id, wheel_deadline_ns)),
            Err(_) => None, // Log error in debug mode
        }
    }

    fn poll_timers(&mut self) -> bool {
        let now_ns = self.timer_wheel.now_ns();
        let expired = self
            .timer_wheel
            .poll(now_ns, TIMER_EXPIRE_BUDGET, &mut self.timer_output);
        if expired == 0 {
            return false;
        }

        let mut progress = false;
        for idx in 0..expired {
            let (timer_id, deadline_ns, handle) = self.timer_output[idx];
            if self.process_timer_entry(timer_id, deadline_ns, handle) {
                progress = true;
            }
        }
        progress
    }

    fn process_timer_entry(
        &mut self,
        timer_id: u64,
        _deadline_ns: u64,
        handle: TimerHandle,
    ) -> bool {
        if handle.worker_id() != self.worker_id {
            return false;
        }

        let slot = unsafe { handle.task_slot().as_ref() };
        let task_ptr = slot.task_ptr();
        if task_ptr.is_null() {
            return false;
        }

        let task = unsafe { &*task_ptr };
        if task.global_id() != handle.task_id() {
            return false;
        }

        task.schedule();
        self.stats.timer_fires = self.stats.timer_fires.saturating_add(1);
        true
    }

    fn cancel_timer(&mut self, timer: &Timer) -> bool {
        if !timer.is_scheduled() {
            return false;
        }

        let Some(worker_id) = timer.worker_id() else {
            return false;
        };

        let timer_id = timer.timer_id();
        if timer_id == 0 {
            return false;
        }

        if worker_id == self.worker_id {
            if self.timer_wheel.cancel_timer(timer_id).is_ok() {
                timer.mark_cancelled(timer_id);
                return true;
            }
            return false;
        }

        self.scheduler
            .post_cancel_message(self.worker_id, worker_id, timer_id)
    }

    #[inline]
    fn cancel_remote_timer(&mut self, worker_id: u32, timer_id: u64) -> bool {
        if worker_id != self.worker_id {
            return false;
        }

        self.scheduler
            .post_cancel_message(self.worker_id, worker_id, timer_id)
    }

    #[inline(always)]
    pub fn next_u64(&mut self) -> u64 {
        self.rng.next()
    }

    #[inline(always)]
    fn poll_yield(&mut self, run_ctx: &mut RunContext, max: usize) -> usize {
        // let max = 1024*8;
        let mut count = 0;
        let mut now = self.timer_wheel.now_ns();
        while let Some(handle) = self.try_acquire_local_yield() {
            self.stats.yield_queue_polls += 1;
            self.poll_handle(run_ctx, handle);
            count += 1;
            if count >= max || run_ctx.pinned {
                // if count >= max {
                break;
            }

            let next_now = self.timer_wheel.now_ns();

            if now != next_now {
                now = next_now;
                self.poll_system_tasks(run_ctx);

                if self.try_partition_random(run_ctx) {
                    count += 1;
                }
                if run_ctx.should_exit_generator() {
                    return count;
                }
            }
        }
        count
    }

    #[inline(always)]
    fn poll_yield_steal(&mut self, run_ctx: &mut RunContext, max: usize) -> usize {
        let mut count = 0;
        while let Some(handle) = self.try_steal_yielded() {
            self.stats.yield_queue_polls += 1;
            self.poll_handle(run_ctx, handle);
            count += 1;
            if count >= max || run_ctx.pinned {
                break;
            }
        }
        count
    }

    #[inline(always)]
    fn try_acquire_task(&mut self, leaf_idx: usize) -> Option<TaskHandle> {
        let signals_per_leaf = self.scheduler.arena().signals_per_leaf();
        if signals_per_leaf == 0 {
            return None;
        }

        let mask = if signals_per_leaf >= 64 {
            u64::MAX
        } else {
            (1u64 << signals_per_leaf) - 1
        };

        self.stats.leaf_summary_checks = self.stats.leaf_summary_checks.saturating_add(1);
        let mut available =
            self.scheduler.summary().leaf_words[leaf_idx].load(Ordering::Acquire) & mask;
        if available == 0 {
            self.stats.empty_scans = self.stats.empty_scans.saturating_add(1);
            return None;
        }
        self.stats.leaf_summary_hits = self.stats.leaf_summary_hits.saturating_add(1);

        // let mut attempts = signals_per_leaf;
        let mut attempts = 2;

        while available != 0 && attempts > 0 {
            let start = (self.next_u64() as usize) % signals_per_leaf;

            // Inner loop to find a signal with available tasks.
            // Returns Some((signal_idx, signal)) if found, None if exhausted.
            let found = loop {
                // Check attempts at the start of each inner loop iteration
                // to prevent infinite spinning under contention.
                if attempts == 0 {
                    break None;
                }

                let candidate = bits::find_nearest(available, start as u64);
                if candidate >= 64 {
                    // No bits set in local `available` - reload from atomic
                    self.stats.leaf_summary_checks =
                        self.stats.leaf_summary_checks.saturating_add(1);
                    available = self.scheduler.summary().leaf_words[leaf_idx]
                        .load(Ordering::Acquire)
                        & mask;
                    if available == 0 {
                        self.stats.empty_scans = self.stats.empty_scans.saturating_add(1);
                        return None;
                    }
                    self.stats.leaf_summary_hits = self.stats.leaf_summary_hits.saturating_add(1);
                    // Decrement attempts on reload to bound total iterations
                    attempts -= 1;
                    continue;
                }
                let bit_index = candidate as usize;
                let sig = unsafe { &*self.scheduler.arena().task_signal_ptr(leaf_idx, bit_index) };
                let bits = sig.load(Ordering::Acquire);
                if bits == 0 {
                    // Don't call mark_signal_inactive here - there's a TOCTOU race:
                    // A task might have just set the signal bit after we loaded.
                    // We only clear from local `available` to skip this signal for now.
                    // The summary bit will be properly cleared when remaining == 0
                    // after a successful acquire (see below).
                    available &= !(1u64 << bit_index);
                    attempts -= 1;
                    continue;
                }
                break Some((bit_index, sig));
            };

            // If inner loop exhausted attempts without finding a valid signal, exit
            let (signal_idx, signal) = match found {
                Some(result) => result,
                None => break,
            };

            let bits = signal.load(Ordering::Acquire);
            let bit_seed = (self.next_u64() & 63) as u64;
            let bit_candidate = bits::find_nearest(bits, bit_seed);
            let bit_idx = if bit_candidate < 64 {
                bit_candidate as u64
            } else {
                available &= !(1u64 << signal_idx);
                attempts -= 1;
                continue;
            };

            let (remaining, acquired) = signal.try_acquire(bit_idx);
            if !acquired {
                self.stats.cas_failures = self.stats.cas_failures.saturating_add(1);
                available &= !(1u64 << signal_idx);
                attempts -= 1;
                continue;
            }

            let remaining_mask = remaining;
            if remaining_mask == 0 {
                self.scheduler
                    .summary()
                    .mark_signal_inactive(leaf_idx, signal_idx);
                // Re-check: A task might have set a bit between our CAS and
                // mark_signal_inactive. If so, re-activate the summary bit.
                let recheck = signal.load(Ordering::Acquire);
                if recheck != 0 {
                    self.scheduler
                        .summary()
                        .mark_signal_active(leaf_idx, signal_idx);
                }
            }

            self.stats.signal_polls = self.stats.signal_polls.saturating_add(1);
            let slot_idx = signal_idx * 64 + bit_idx as usize;
            let task = unsafe { self.scheduler.arena().task(leaf_idx, slot_idx) };
            return Some(TaskHandle::from_task(task));
        }

        self.stats.empty_scans = self.stats.empty_scans.saturating_add(1);
        None
    }

    #[inline(always)]
    fn enqueue_yield(&mut self, handle: TaskHandle) {
        let was_empty = self.yield_queue.push_with_status(handle);
        if was_empty {
            // Set yield_bit in WorkerWaker status
            self.scheduler.wakers[self.worker_id as usize].mark_yield();
        }
    }

    #[inline(always)]
    fn try_acquire_local_yield(&mut self) -> Option<TaskHandle> {
        let (item, was_last) = self.yield_queue.pop_with_status();
        if let Some(handle) = item {
            if was_last {
                // Clear yield_bit in WorkerWaker status
                self.scheduler.wakers[self.worker_id as usize].try_unmark_yield();
            }
            return Some(handle);
        }
        None
    }

    #[inline(always)]
    fn try_steal_yielded(&mut self) -> Option<TaskHandle> {
        let next_rand = self.next_u64();
        let (attempts, success) =
            self.scheduler
                .try_yield_steal(self.worker_id as usize, 1, next_rand as usize);
        if success {
            self.stats.steal_attempts += attempts;
            self.stats.steal_successes += 1;
            self.try_acquire_local_yield()
        } else {
            None
        }
    }

    #[inline(always)]
    fn process_leaf(&mut self, run_ctx: &mut RunContext, leaf_idx: usize) -> bool {
        if let Some(handle) = self.try_acquire_task(leaf_idx) {
            self.poll_handle(run_ctx, handle);
            return true;
        }
        false
    }

    #[inline(always)]
    fn try_partition_random(&mut self, run_ctx: &mut RunContext) -> bool {
        let partition_len = self.partition_len;

        if partition_len.is_power_of_two() {
            let mask = partition_len - 1;
            for _ in 0..partition_len {
                let start_offset = self.next_u64() as usize & mask;
                let leaf_idx = self.partition_start + start_offset;
                if self.process_leaf(run_ctx, leaf_idx) {
                    return true;
                }
            }
        } else {
            for _ in 0..partition_len {
                let start_offset = self.next_u64() as usize % partition_len;
                let leaf_idx = self.partition_start + start_offset;
                if self.process_leaf(run_ctx, leaf_idx) {
                    return true;
                }
            }
        }

        false
    }

    #[inline(always)]
    fn try_partition_linear(&mut self, run_ctx: &mut RunContext) -> bool {
        let partition_start = self.partition_start;
        let partition_len = self.partition_len;

        for offset in 0..partition_len {
            let leaf_idx = partition_start + offset;
            if self.process_leaf(run_ctx, leaf_idx) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn try_any_partition_random(&mut self, run_ctx: &mut RunContext, leaf_count: usize) -> bool {
        if leaf_count.is_power_of_two() {
            let leaf_mask = leaf_count - 1;
            for _ in 0..leaf_count {
                let leaf_idx = self.next_u64() as usize & leaf_mask;
                self.stats.leaf_steal_attempts += 1;
                if self.process_leaf(run_ctx, leaf_idx) {
                    self.stats.leaf_steal_successes += 1;
                    return true;
                }
            }
        } else {
            for _ in 0..leaf_count {
                let leaf_idx = self.next_u64() as usize % leaf_count;
                self.stats.leaf_steal_attempts += 1;
                if self.process_leaf(run_ctx, leaf_idx) {
                    self.stats.leaf_steal_successes += 1;
                    return true;
                }
            }
        }
        false
    }

    #[inline(always)]
    fn try_any_partition_linear(&mut self, run_ctx: &mut RunContext, leaf_count: usize) -> bool {
        for leaf_idx in 0..leaf_count {
            self.stats.leaf_steal_attempts += 1;
            if self.process_leaf(run_ctx, leaf_idx) {
                self.stats.leaf_steal_successes += 1;
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn poll_handle(&mut self, run_ctx: &mut RunContext, handle: TaskHandle) {
        let task = handle.task();

        // Check if this task has a pinned generator
        if task.has_pinned_generator() {
            // Poll the task by resuming its pinned generator
            self.poll_task_with_pinned_generator(run_ctx, handle, task);
            return;
        }

        struct ActiveTaskGuard(*mut *mut Task);
        impl Drop for ActiveTaskGuard {
            fn drop(&mut self) {
                unsafe {
                    *(self.0) = core::ptr::null_mut();
                }
            }
        }
        self.current_task = unsafe { task as *const _ as *mut Task };
        let _active_guard =
            ActiveTaskGuard(unsafe { &self.current_task as *const *mut Task as *mut *mut Task });

        self.stats.tasks_polled += 1;

        if !task.is_yielded() {
            task.begin();
        } else {
            task.record_yield();
            task.clear_yielded();
        }

        self.poll_task(run_ctx, handle, task);
    }

    /// Poll a task that has a pinned generator.
    ///
    /// When a task has a pinned generator, we resume it to continue the interrupted poll.
    /// The generator will poll the task's future and either:
    /// - Return Ready: task is complete, clean up generator
    /// - Return Pending: use normal task scheduling (finish/finish_and_schedule)
    fn poll_task_with_pinned_generator(
        &mut self,
        run_ctx: &mut RunContext,
        handle: TaskHandle,
        task: &Task,
    ) {
        struct ActiveTaskGuard(*mut *mut Task);
        impl Drop for ActiveTaskGuard {
            fn drop(&mut self) {
                unsafe {
                    *(self.0) = core::ptr::null_mut();
                }
            }
        }
        self.current_task = unsafe { task as *const _ as *mut Task };
        let _active_guard =
            ActiveTaskGuard(unsafe { &self.current_task as *const *mut Task as *mut *mut Task });

        self.stats.tasks_polled += 1;

        let mut guard = unsafe { task.get_pinned_generator() };
        if let Some(generator) = guard.generator() {
            // Resume the generator - it will continue the interrupted poll
            match generator.resume() {
                Some(status) => {
                    // Generator yielded - check if task completed
                    // Status 1 = task complete, status 2 = future dropped
                    if status == 1 || status == 2 {
                        // Task completed, clean up generator
                        guard.free();
                        task.finish();
                        self.stats.completed_count = self.stats.completed_count.saturating_add(1);
                        if let Some(ptr) = task.take_future() {
                            unsafe { drop_future(ptr) };
                        }
                    } else {
                        // Task still pending - use normal scheduling
                        if task.is_yielded() {
                            task.clear_yielded();
                            self.stats.yielded_count += 1;
                            task.finish_and_schedule();
                        } else {
                            self.stats.waiting_count += 1;
                            task.finish();
                        }
                    }
                }
                None => {
                    // Generator exhausted/completed - task is done
                    guard.free();
                    task.finish();
                    self.stats.completed_count = self.stats.completed_count.saturating_add(1);
                    if let Some(ptr) = task.take_future() {
                        unsafe { drop_future(ptr) };
                    }
                }
            }
        } else {
            // No generator found - shouldn't happen, but finish the task
            task.finish();
            self.stats.completed_count = self.stats.completed_count.saturating_add(1);

            #[cfg(debug_assertions)]
            tracing::warn!(
                "[worker {}] Task had pinned generator flag but no generator",
                self.worker_id
            );
        }
    }

    fn poll_task(&mut self, run_ctx: &mut RunContext, _handle: TaskHandle, task: &Task) {
        let waker = unsafe { task.waker_yield() };
        let mut cx = Context::from_waker(&waker);
        let poll_result = unsafe { task.poll_future(&mut cx) };

        match poll_result {
            Some(Poll::Ready(())) => {
                task.finish();
                self.stats.completed_count = self.stats.completed_count.saturating_add(1);

                // Check if generator was pinned during poll (e.g., via pin_stack() call)
                if task.has_pinned_generator() {
                    run_ctx.mark_pinned();
                }

                self.current_task = core::ptr::null_mut();
                if let Some(ptr) = task.take_future() {
                    unsafe { drop_future(ptr) };
                }
            }
            Some(Poll::Pending) => {
                // Check if generator was pinned during poll
                if task.has_pinned_generator() {
                    run_ctx.mark_pinned();
                }

                if task.is_yielded() {
                    task.clear_yielded();
                    self.stats.yielded_count += 1;
                    task.finish_and_schedule();
                } else {
                    self.stats.waiting_count += 1;
                    task.finish();
                }
            }
            None => {
                // Future is gone - task completed or was cancelled
                if task.has_pinned_generator() {
                    run_ctx.mark_pinned();
                }
                self.stats.completed_count += 1;
                task.finish();
            }
        }
    }

    fn schedule_timer(&mut self, timer: &Timer, delay: Duration) -> Option<u64> {
        let task_ptr = self.current_task;
        if task_ptr.is_null() {
            return None;
        }

        let task = unsafe { &*task_ptr };
        let Some(slot) = task.slot() else {
            return None;
        };

        if timer.is_scheduled() {
            if let Some(worker_id) = timer.worker_id() {
                if worker_id == self.worker_id {
                    let existing_id = timer.timer_id();
                    if self.timer_wheel.cancel_timer(existing_id).is_ok() {
                        timer.reset();
                    }
                } else {
                    // Cancel timer on other worker.
                    self.bus
                        .post_cancel_message(self.worker_id, worker_id, timer.timer_id());
                }
            }
        }

        let delay_ns = delay.as_nanos().min(u128::from(u64::MAX)) as u64;
        let now = self.timer_wheel.now_ns();
        let target_deadline_ns = now.saturating_add(delay_ns);

        let handle = timer.prepare(slot, task.global_id(), self.worker_id);

        if let Some((timer_id, wheel_deadline_ns)) =
            self.enqueue_timer_entry_best_fit(target_deadline_ns, handle)
        {
            timer.commit_schedule(timer_id, target_deadline_ns, wheel_deadline_ns);
            Some(target_deadline_ns)
        } else {
            timer.reset();
            None
        }
    }
}

/// Schedules a timer for the task currently being polled by the active worker.
///
/// Uses best-fit scheduling to support arbitrarily long timers via cascading.
/// The timer wheel will schedule at the largest wheel that fits, and TimerDelay
/// will reschedule when the wheel fires if the target hasn't been reached yet.
pub(crate) fn schedule_timer_for_task(
    _cx: &Context<'_>,
    timer: &Timer,
    delay: Duration,
) -> Option<u64> {
    let worker = super::current_worker();
    if worker.is_none() {
        return None;
    }
    let worker = worker.unwrap();
    let worker_id = worker.worker_id;
    let task = worker.current_task;
    if task.is_null() {
        return None;
    }
    let task = unsafe { &mut *task };
    let slot = task.slot()?;

    // Cancel existing timer if scheduled
    if timer.is_scheduled() {
        if let Some(existing_worker_id) = timer.worker_id() {
            if existing_worker_id == worker_id {
                let existing_id = timer.timer_id();
                if worker.timer_wheel.cancel_timer(existing_id).is_ok() {
                    timer.reset();
                }
            } else {
                // Cross-worker cancellation: send message to the worker that owns the timer
                let existing_id = timer.timer_id();
                if worker
                    .bus
                    .post_cancel_message(worker_id, existing_worker_id, existing_id)
                {
                    timer.reset();
                }
            }
        }
    }

    let delay_ns = delay.as_nanos().min(u128::from(u64::MAX)) as u64;
    let now = worker.timer_wheel.now_ns();
    let target_deadline_ns = now.saturating_add(delay_ns);

    let handle = timer.prepare(slot, task.global_id(), worker_id);

    // Use best-fit scheduling for cascading support
    match worker
        .timer_wheel
        .schedule_timer_best_fit(target_deadline_ns, handle)
    {
        Ok((timer_id, wheel_deadline_ns)) => {
            timer.commit_schedule(timer_id, target_deadline_ns, wheel_deadline_ns);
            Some(target_deadline_ns)
        }
        Err(_) => {
            timer.reset();
            None
        }
    }
}

/// Reschedules a timer for the remaining time to reach its target deadline.
///
/// Called by TimerDelay when the wheel timer fires but the target deadline
/// hasn't been reached yet (cascading for long timers).
pub(crate) fn reschedule_timer_for_task(_cx: &Context<'_>, timer: &Timer) -> bool {
    let worker = super::current_worker();
    if worker.is_none() {
        return false;
    }
    let worker = worker.unwrap();
    let worker_id = worker.worker_id;
    let task = worker.current_task;
    if task.is_null() {
        return false;
    }
    let task = unsafe { &mut *task };
    let Some(slot) = task.slot() else {
        return false;
    };

    // Verify the timer is scheduled on this worker
    let Some(timer_worker_id) = timer.worker_id() else {
        return false;
    };
    if timer_worker_id != worker_id {
        return false;
    }

    let target_deadline_ns = timer.target_deadline_ns();
    let handle = TimerHandle::new(slot, task.global_id(), worker_id, 0);

    // Schedule for the remaining time using best-fit
    match worker
        .timer_wheel
        .schedule_timer_best_fit(target_deadline_ns, handle)
    {
        Ok((timer_id, wheel_deadline_ns)) => {
            timer.commit_reschedule(timer_id, wheel_deadline_ns);
            true
        }
        Err(_) => false,
    }
}

/// Cancels a timer owned by the task currently being polled by the active worker.
pub(crate) fn cancel_timer_for_current_task(timer: &Timer) -> bool {
    if !timer.is_scheduled() {
        return false;
    }

    let Some(timer_worker_id) = timer.worker_id() else {
        return false;
    };

    let timer_id = timer.timer_id();
    if timer_id == 0 {
        return false;
    }

    let worker = super::current_worker();
    if worker.is_none() {
        return false;
    }
    let worker = worker.unwrap();
    let worker_id = worker.worker_id;
    let task = worker.current_task;
    if task.is_null() {
        return false;
    }
    let task = unsafe { &mut *task };

    if timer_worker_id == worker_id {
        if worker.timer_wheel.cancel_timer(timer_id).is_ok() {
            timer.mark_cancelled(timer_id);
            return true;
        }
        return false;
    }

    // Cross-worker cancellation: send message to the worker that owns the timer
    if worker
        .bus
        .post_cancel_message(worker_id, timer_worker_id, timer_id)
    {
        timer.mark_cancelled(timer_id);
        true
    } else {
        false
    }
}
