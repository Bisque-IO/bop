pub mod io;
pub mod scheduler;
pub mod waker;
pub mod worker;

use super::deque::{Stealer, Worker as YieldWorker};
use super::task::summary::Summary;
use super::task::{GeneratorRunMode, Task, TaskArena, TaskArenaStats, TaskHandle, TaskSlot};
use super::timer::wheel::TimerWheel;
use super::timer::{Timer, TimerHandle};
use crate::PopError;
use crate::future::waker::DiatomicWaker;
use crate::runtime::preemption::GeneratorYieldReason;
use crate::runtime::task::{GeneratorOwnership, SpawnError};
use crate::runtime::timer::ticker::{TickHandler, TickHandlerRegistration};
use crate::runtime::worker::io::IoDriver;
use crate::runtime::{mpsc, preemption};
use crate::{PushError, utils};
use std::any::TypeId;
use std::cell::{Cell, UnsafeCell};
use std::panic::Location;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};
use waker::WorkerWaker;

use crate::utils::bits;
use rand::RngCore;

use parking_lot::Mutex;

#[cfg(unix)]
use libc;

// Re-export everything from scheduler and worker modules
pub use scheduler::*;
pub use worker::*;

const MAX_WORKER_LIMIT: usize = 512;

const DEFAULT_WAKE_BURST: usize = 4;
const FULL_SUMMARY_SCAN_CADENCE_MASK: u64 = 1024 * 8 - 1;
const DEFAULT_TICK_DURATION_NS: u64 = 1 << 19; // ~1.05ms, power of two as required by TimerWheel
const TIMER_TICKS_PER_WHEEL: usize = 1024 * 1;
const TIMER_EXPIRE_BUDGET: usize = 4096;
const MESSAGE_BATCH_SIZE: usize = 4096;

// Worker status is now managed via WorkerWaker:
// - WorkerWaker.summary: mpsc queue signals (bits 0-63)
// - WorkerWaker.status bit 63: yield queue has items
// - WorkerWaker.status bit 62: partition cache has work

/// Trait for cross-worker operations that don't depend on const parameters
pub(crate) trait WorkerBus: Send + Sync {
    fn post_cancel_message(&self, from_worker_id: u32, to_worker_id: u32, timer_id: u64) -> bool;
}

pub struct WorkerTLS {}

pub enum RunOnceError {
    StackPinned,

    Raised(Box<dyn std::error::Error>),
}

thread_local! {
    pub(crate) static CURRENT: Cell<*mut worker::Worker<'static>> = Cell::new(core::ptr::null_mut());
}

#[inline]
pub fn is_worker_thread() -> bool {
    current_task().is_some()
}

#[inline]
pub fn current_worker<'a>() -> Option<&'a mut worker::Worker<'a>> {
    let worker = CURRENT.get();
    if worker.is_null() {
        None
    } else {
        Some(unsafe { &mut *(worker as *mut worker::Worker<'a>) })
    }
}

#[inline]
pub fn current_task<'a>() -> Option<&'a mut Task> {
    let worker = current_worker()?;
    unsafe { Some(&mut *worker.current_task) }
}

#[inline]
pub fn current_timer_wheel<'a>() -> Option<&'a mut TimerWheel<TimerHandle>> {
    let worker = current_worker()?;
    unsafe { Some(&mut *worker.timer_wheel) }
}

#[inline]
pub fn current_event_loop<'a>() -> Option<&'a mut IoDriver> {
    let worker = current_worker()?;
    unsafe { Some(&mut *worker.io) }
}

/// Returns the most recent `now_ns` observed by the active worker.
#[inline]
pub fn current_worker_now_ns() -> u64 {
    let timer_wheel = current_timer_wheel();
    if let Some(timer_wheel) = current_timer_wheel() {
        timer_wheel.now_ns()
    } else {
        // fallback to high frequency clock
        Instant::now().elapsed().as_nanos() as u64
    }
}

/// Returns the current worker's ID, or None if not called from within a worker context.
#[inline]
pub fn current_worker_id() -> Option<u32> {
    let timer_wheel = current_timer_wheel()?;
    Some(timer_wheel.worker_id())
}

// ============================================================================
// Internal Helper (Called by Trampoline)
// ============================================================================

/// This function is called by the assembly trampoline.
/// It runs on the worker thread's stack in a normal execution context.
/// It is safe to access TLS and yield here.
#[unsafe(no_mangle)]
pub extern "C" fn rust_preemption_helper() {
    if let Some(worker) = current_worker() {
        let task = worker.current_task;
        let scope = worker.current_scope;
        if task.is_null() || scope.is_null() {
            return;
        }

        unsafe {
            let task = &mut *task;
            if !task.has_pinned_generator() {
                unsafe {
                    task.pin_generator(
                        scope as *mut usize,
                        GeneratorOwnership::Worker,
                        GeneratorRunMode::Switch,
                    )
                };
            } else {
                task.set_generator_run_mode(GeneratorRunMode::Switch);
            }

            // 1. Check and clear flag (for cooperative correctness / hygiene)
            // We don't require it to be true to yield, because we might be here via Unix signal
            // which couldn't set the flag.
            worker.preemption_requested.store(false, Ordering::Release);

            // SAFETY: The pointer is set only while the worker generator is running and
            // cleared immediately after it exits. The trampoline only executes while the
            // worker is inside that generator, so the pointer is always valid here.
            let scope = &mut *(scope as *mut crate::generator::Scope<(), usize>);
            let _ = scope.yield_(GeneratorYieldReason::Preempted.as_usize());
        }
    } else {
        // Non-worker path: Use TLS generator scope
        // CRITICAL: Don't clear the scope during preemption, as another signal could arrive
        // between clear and restore, seeing a null pointer. The scope remains valid throughout
        // the generator's lifetime and is only cleared when the generator exits.
        let scope = preemption::get_generator_scope();
        if !scope.is_null() {
            let scope = unsafe { &mut *(scope as *mut crate::generator::Scope<(), usize>) };
            let _ = scope.yield_(GeneratorYieldReason::Preempted.as_usize());
        }
    }
}

/// Request that the current task receive its own dedicated generator (stackful coroutine).
///
/// This MUST be called from within a task that is executing inside a Worker's generator.
/// When called, it signals the Worker that after the current poll returns, the task
/// should receive its own dedicated poll-mode generator instead of sharing the worker's.
///
/// **Important**: This does NOT capture the current stack state. It simply marks that
/// the task wants a dedicated generator. The worker will create a new generator for
/// the task after the current poll completes.
///
/// For preemption (capturing the current stack mid-execution), use the signal-based
/// preemption mechanism instead (which uses Switch mode).
///
/// # Returns
/// `true` if the pin request was successfully registered, `false` if not in a task context.
///
/// # Example
/// ```ignore
/// use maniac::runtime::worker::stackify;
///
/// // Inside an async task running on a worker:
/// async {
///     // Request a dedicated generator for this task
///     if stackify() {
///         // Task will receive its own generator after this poll returns
///     }
/// }
/// ```
#[inline]
pub fn as_coroutine() -> bool {
    if let Some(worker) = current_worker() {
        let task = worker.current_task;
        if task.is_null() {
            return false;
        }
        let task = unsafe { &mut *task };
        if !task.has_pinned_generator() {
            // Mark the task as wanting a dedicated generator.
            // We use a null pointer as a sentinel value - the worker will detect this
            // and create a proper poll-mode generator for the task.
            // The Owned ownership indicates we need to CREATE a generator, not that
            // we're capturing the worker's generator.
            unsafe {
                task.pin_generator(
                    std::ptr::null_mut(),
                    GeneratorOwnership::Owned,
                    GeneratorRunMode::Poll,
                )
            };
        }
        true
    } else {
        false
    }
}

#[inline]
pub fn is_coroutine() -> bool {
    if let Some(worker) = current_worker() {
        let task = worker.current_task;
        if task.is_null() {
            return false;
        }
        let task = unsafe { &mut *task };
        task.has_pinned_generator()
    } else {
        false
    }
}

#[inline]
pub fn current_waker() -> Option<std::task::Waker> {
    if let Some(worker) = current_worker() {
        let task = worker.current_task;
        if task.is_null() {
            None
        } else {
            let task = unsafe { &mut *task };
            Some(task.waker())
        }
    } else {
        None
    }
}

/// Spawns a new async task on the scheduler.
///
/// This method performs the following steps:
/// 1. Validates that the executor is not shutting down and the arena is open
/// 2. Reserves a task slot from the scheduler
/// 3. Initializes the task in the arena with its global ID
/// 4. Wraps the user's future in a join future that handles completion and cleanup
/// 5. Attaches the future to the task and schedules it for execution
///
/// # Arguments
///
/// * `future`: The future to execute. Must implement `IntoFuture` and be `Send + 'static`.
///
/// # Returns
///
/// * `Ok(JoinHandle<T>)`: A join handle that can be awaited to get the task's result
/// * `Err(SpawnError)`: Error if spawning fails (closed executor, no capacity, or attach failed)
///
/// # Errors
///
/// - `SpawnError::Closed`: Scheduler is shutting down or arena is closed
/// - `SpawnError::NoCapacity`: No available task slots in the arena
/// - `SpawnError::AttachFailed`: Failed to attach the future to the task
#[track_caller]
pub fn spawn<F, T>(future: F) -> Result<JoinHandle<T>, super::task::SpawnError>
where
    F: IntoFuture<Output = T> + Send + 'static,
    F::IntoFuture: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let location = Location::caller();
    let type_id = TypeId::of::<F>();
    let worker = current_worker().expect("not inside a task");
    worker.scheduler.spawn(type_id, location, future)
}
