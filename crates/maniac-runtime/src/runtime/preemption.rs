//! Preemptive scheduling support for worker threads.
//!
//! This module provides platform-specific mechanisms to **interrupt** worker threads
//! without terminating them, allowing generator context switching:
//!
//! - **Unix (Linux/macOS/BSD)**: `pthread_kill()` sends SIGALRM to interrupt thread
//!   - Note: Despite the name, pthread_kill does NOT kill the thread!
//!   - It only delivers a signal, similar to sending SIGINT or SIGALRM
//!   - The thread continues running after the signal handler completes
//!   - Signal handler runs **on the worker thread** and sets the worker's atomic flag
//!
//! - **Windows**: `SuspendThread()`/`ResumeThread()` to interrupt execution
//!   - Thread is suspended momentarily by the **timer thread**
//!   - Timer thread sets the worker's preemption flag while suspended
//!   - Thread immediately resumes and checks the flag
//!   - No termination occurs
//!
//! ## Threading Model
//!
//! **Unix**: Signal handler executes on the worker thread's stack
//! - Worker thread stores pointer to its preemption flag in thread-local storage
//! - Signal handler reads thread-local to get the flag and sets it
//! - Worker thread checks its own flag in the generator loop
//!
//! **Windows**: Timer thread directly accesses worker's preemption flag
//! - WorkerThreadHandle stores pointer to the worker's preemption flag
//! - Timer thread suspends worker, sets the shared atomic flag, resumes worker
//! - Worker thread checks its own flag in the generator loop
//!
//! ## Preemption Flow (Thread Continues Running)
//!
//! 1. External thread (e.g., timer) calls `interrupt_worker(id)`
//! 2. Signal/suspension interrupts the worker thread's execution
//! 3. Worker's preemption flag is set (by signal handler on Unix, by timer on Windows)
//! 4. Worker checks flag in its generator loop
//! 5. Worker pins current generator to current task (saves stack state)
//! 6. Worker creates a NEW generator for itself
//! 7. **Worker thread continues running** with the new generator
//! 8. Original task can later resume with its pinned generator
//!
//! The worker thread is **never terminated** - it just switches generator contexts!

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::cell::Cell;
use std::ptr;

// Thread-local storage for the current worker's preemption flag pointer
// This allows the Unix signal handler (which runs on the worker thread) to access the flag
thread_local! {
    static CURRENT_WORKER_PREEMPTION_FLAG: Cell<*const AtomicBool> = const { Cell::new(ptr::null()) };
    // For non-cooperative preemption: Store pointer to generator's Scope
    // Signal handler can call yield_() directly to force a context switch
    static CURRENT_GENERATOR_SCOPE: Cell<*mut ()> = const { Cell::new(ptr::null_mut()) };
}

/// Initialize preemption for the current worker thread
/// Must be called when worker thread starts, before entering the main loop
#[inline]
pub(crate) fn init_worker_thread_preemption(flag: &AtomicBool) {
    CURRENT_WORKER_PREEMPTION_FLAG.with(|cell| {
        cell.set(flag as *const AtomicBool);
    });
}

/// Set the current generator scope (called at start of generator loop)
/// This allows the signal handler to force a yield by calling scope.yield_()
#[inline]
pub(crate) fn set_generator_scope(scope_ptr: *mut ()) {
    CURRENT_GENERATOR_SCOPE.with(|cell| cell.set(scope_ptr));
}

/// Clear the current generator scope (called when generator exits)
#[inline]
pub(crate) fn clear_generator_scope() {
    CURRENT_GENERATOR_SCOPE.with(|cell| cell.set(ptr::null_mut()));
}

/// Request preemption on the current worker thread (called by signal handler on Unix)
/// On Unix with non-cooperative preemption: This forces an immediate yield by calling scope.yield_()
/// On Windows: Falls back to setting the flag (cooperative)
#[inline]
fn request_preemption_current_thread() {
    // First, set the flag for cooperative fallback
    CURRENT_WORKER_PREEMPTION_FLAG.with(|cell| {
        let ptr = cell.get();
        if !ptr.is_null() {
            unsafe {
                (*ptr).store(true, Ordering::Release);
            }
        }
    });
    
    // On Unix, force an immediate yield if we have a generator scope
    #[cfg(unix)]
    {
        CURRENT_GENERATOR_SCOPE.with(|cell| {
            let scope_ptr = cell.get();
            if !scope_ptr.is_null() {
                unsafe {
                    // SAFETY: scope_ptr points to a valid generator::Scope<(), usize>
                    // The signal handler runs on the generator's stack, so calling yield_() is safe
                    let scope = &mut *(scope_ptr as *mut generator::Scope<(), usize>);
                    scope.yield_(1); // Yield with status 1 to indicate forced preemption
                }
            }
        });
    }
}

/// Request preemption on a specific worker (called by timer thread on Windows)
#[inline]
fn request_preemption_on_flag(flag: &AtomicBool) {
    flag.store(true, Ordering::Release);
}

/// Check and clear the preemption flag for the current worker
#[inline]
pub(crate) fn check_and_clear_preemption(flag: &AtomicBool) -> bool {
    flag.swap(false, Ordering::AcqRel)
}

pub struct WorkerThreadHandle {
    #[cfg(unix)]
    pthread: libc::pthread_t,
    #[cfg(windows)]
    thread_handle: winapi::um::winnt::HANDLE,
    #[cfg(windows)]
    thread_id: winapi::shared::minwindef::DWORD,
    #[cfg(windows)]
    preemption_flag: *const AtomicBool, // Pointer to worker's preemption flag
    #[cfg(windows)]
    generator_scope: *const AtomicPtr<()>, // Pointer to worker's generator scope atomic
}

unsafe impl Send for WorkerThreadHandle {}
unsafe impl Sync for WorkerThreadHandle {}

impl WorkerThreadHandle {
    #[cfg(unix)]
    pub fn current() -> Result<Self, PreemptionError> {
        Ok(Self {
            pthread: unsafe { libc::pthread_self() },
        })
    }
    
    #[cfg(windows)]
    pub fn current(preemption_flag: &AtomicBool, generator_scope: &AtomicPtr<()>) -> Result<Self, PreemptionError> {
        use winapi::um::processthreadsapi::{GetCurrentThread, GetCurrentThreadId};
        use winapi::um::handleapi::DuplicateHandle;
        use winapi::um::processthreadsapi::GetCurrentProcess;
        use winapi::um::winnt::DUPLICATE_SAME_ACCESS;
        
        unsafe {
            let mut real_handle: winapi::um::winnt::HANDLE = std::ptr::null_mut();
            let pseudo_handle = GetCurrentThread();
            let current_process = GetCurrentProcess();
            
            let result = DuplicateHandle(
                current_process,
                pseudo_handle,
                current_process,
                &mut real_handle,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            );
            
            if result == 0 {
                return Err(PreemptionError::ThreadSetupFailed);
            }
            
            Ok(Self {
                thread_handle: real_handle,
                thread_id: GetCurrentThreadId(),
                preemption_flag: preemption_flag as *const AtomicBool,
                generator_scope: generator_scope as *const AtomicPtr<()>,
            })
        }
    }
    
    #[cfg(not(any(unix, windows)))]
    pub fn current(_preemption_flag: &AtomicBool) -> Result<Self, PreemptionError> {
        Err(PreemptionError::UnsupportedPlatform)
    }
    
    /// Interrupt this worker thread to trigger generator switching.
    /// 
    /// **Important:** This does NOT terminate the thread! It only interrupts
    /// its current execution to allow generator context switching. The worker
    /// thread continues running with a new generator after this call.
    /// 
    /// Can be called from any thread (e.g., a timer thread).
    /// Thread-safe and can be called concurrently from multiple threads.
    pub fn interrupt(&self) -> Result<(), PreemptionError> {
        #[cfg(unix)]
        {
            unix::interrupt_thread(self.pthread)
        }
        
        #[cfg(windows)]
        {
            windows::interrupt_thread(self.thread_handle, self.thread_id, self.preemption_flag, self.generator_scope)
        }
        
        #[cfg(not(any(unix, windows)))]
        {
            Err(PreemptionError::UnsupportedPlatform)
        }
    }
}

impl Drop for WorkerThreadHandle {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            winapi::um::handleapi::CloseHandle(self.thread_handle);
        }
    }
}

pub(crate) fn init_worker_preemption() -> Result<PreemptionHandle, PreemptionError> {
    #[cfg(unix)]
    return unix::init_preemption();
    
    #[cfg(windows)]
    return Ok(PreemptionHandle {
        _marker: std::marker::PhantomData,
    });
    
    #[cfg(not(any(unix, windows)))]
    return Err(PreemptionError::UnsupportedPlatform);
}

pub(crate) struct PreemptionHandle {
    #[cfg(unix)]
    unix_handle: unix::UnixPreemptionHandle,
    #[cfg(windows)]
    _marker: std::marker::PhantomData<()>,
}

#[derive(Debug)]
pub enum PreemptionError {
    SignalSetupFailed,
    ThreadSetupFailed,
    InterruptFailed,
    UnsupportedPlatform,
}

impl std::fmt::Display for PreemptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreemptionError::SignalSetupFailed => write!(f, "Failed to set up signal handler"),
            PreemptionError::ThreadSetupFailed => write!(f, "Failed to set up thread handle"),
            PreemptionError::InterruptFailed => write!(f, "Failed to interrupt thread"),
            PreemptionError::UnsupportedPlatform => write!(f, "Preemption not supported on this platform"),
        }
    }
}

impl std::error::Error for PreemptionError {}

// ============================================================================
// Unix Implementation (pthread_kill with SIGALRM)
// ============================================================================

#[cfg(unix)]
mod unix {
    use super::*;
    use std::mem::MaybeUninit;
    
    pub(super) struct UnixPreemptionHandle {
        old_handler: libc::sigaction,
    }
    
    impl Drop for UnixPreemptionHandle {
        fn drop(&mut self) {
            unsafe {
                libc::sigaction(libc::SIGALRM, &self.old_handler, std::ptr::null_mut());
            }
        }
    }
    
    /// Signal handler for SIGALRM
    /// **Important**: This runs on the worker thread's stack!
    /// It accesses the thread-local to get the worker's preemption flag and sets it.
    extern "C" fn sigalrm_handler(_signum: libc::c_int) {
        super::request_preemption_current_thread();
    }
    
    pub(super) fn init_preemption() -> Result<super::PreemptionHandle, super::PreemptionError> {
        unsafe {
            let mut sa: libc::sigaction = MaybeUninit::zeroed().assume_init();
            sa.sa_sigaction = sigalrm_handler as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_flags = libc::SA_RESTART;
            
            let mut old_sa: libc::sigaction = MaybeUninit::zeroed().assume_init();
            
            if libc::sigaction(libc::SIGALRM, &sa, &mut old_sa) != 0 {
                return Err(super::PreemptionError::SignalSetupFailed);
            }
            
            Ok(super::PreemptionHandle {
                unix_handle: UnixPreemptionHandle {
                    old_handler: old_sa,
                },
            })
        }
    }
    
    /// Send SIGALRM to a specific thread to interrupt it (NOT kill it!)
    /// 
    /// Despite the misleading name, pthread_kill() does NOT terminate the thread.
    /// It only delivers a signal (SIGALRM) to the thread, which triggers our
    /// signal handler. The thread continues running after the handler completes.
    /// 
    /// The signal handler runs **on the worker thread's stack**, so it can safely
    /// access thread-local storage to get the worker's preemption flag.
    /// 
    /// This is equivalent to: kill(getpid(), SIGALRM) but targeted to one thread.
    /// 
    /// This can be called from any thread (e.g., a timer thread)
    pub(super) fn interrupt_thread(pthread: libc::pthread_t) -> Result<(), super::PreemptionError> {
        unsafe {
            let result = libc::pthread_kill(pthread, libc::SIGALRM);
            if result == 0 {
                Ok(())
            } else {
                Err(super::PreemptionError::InterruptFailed)
            }
        }
    }
}

// ============================================================================
// Windows Implementation (SuspendThread/ResumeThread)
// ============================================================================

#[cfg(windows)]
mod windows {
    use super::*;
    use winapi::um::processthreadsapi::QueueUserAPC;
    use winapi::shared::minwindef::DWORD;
    use winapi::um::winnt::HANDLE;
    use winapi::shared::basetsd::ULONG_PTR;
    use std::sync::atomic::{AtomicBool, AtomicPtr};
    
    /// APC callback context passed as parameter
    /// Contains pointers to the worker's preemption flag and generator scope
    #[repr(C)]
    struct ApcContext {
        preemption_flag: *const AtomicBool,
        generator_scope: *const AtomicPtr<()>,
    }
    
    /// APC callback that runs on the worker thread
    /// This is similar to Unix signal handler - it runs on worker's stack!
    /// 
    /// **CRITICAL**: This executes on the worker thread when it enters an alertable wait.
    /// We can safely call scope.yield_() from here just like the Unix signal handler.
    unsafe extern "system" fn preemption_apc_callback(param: ULONG_PTR) {
        if param == 0 {
            return;
        }
        
        unsafe {
            let ctx = &*(param as *const ApcContext);
            
            // Set the preemption flag for cooperative fallback
            (*ctx.preemption_flag).store(true, std::sync::atomic::Ordering::Release);
            
            // Get the generator scope pointer from the atomic
            let scope_ptr = (*ctx.generator_scope).load(std::sync::atomic::Ordering::Acquire);
            
            if !scope_ptr.is_null() {
                // FORCE IMMEDIATE YIELD (non-cooperative)
                // SAFETY: We're on the worker thread's stack (which is the generator's stack)
                // This is exactly like the Unix signal handler
                let scope = &mut *(scope_ptr as *mut generator::Scope<(), usize>);
                scope.yield_(1); // Force generator to yield NOW!
            }
        }
    }
    
    /// Interrupt a Windows thread using QueueUserAPC.
    /// The APC callback will run on the worker thread when it enters an alertable wait.
    /// 
    /// **This achieves non-cooperative preemption on Windows!**
    /// 
    /// Implementation:
    /// 1. Timer thread calls QueueUserAPC with our callback
    /// 2. When worker enters alertable wait (WaitForSingleObjectEx with bAlertable=TRUE),
    ///    Windows executes our APC callback ON THE WORKER'S STACK
    /// 3. APC callback calls scope.yield_() to force immediate generator switch
    /// 4. Worker wakes from wait, continues in new generator
    /// 
    /// The worker MUST use alertable waits (WaitForSingleObjectEx, SleepEx, etc.)
    /// for this to work. Regular Sleep() or park() won't execute APCs.
    pub(super) fn interrupt_thread(
        thread_handle: HANDLE,
        _thread_id: DWORD,
        preemption_flag: *const AtomicBool,
        generator_scope: *const AtomicPtr<()>,
    ) -> Result<(), super::PreemptionError> {
        unsafe {
            // Create APC context on the heap (must outlive the APC call)
            // We'll leak it since we can't know when the APC executes
            // This is acceptable - it's only 16 bytes and happens rarely
            let ctx = Box::into_raw(Box::new(ApcContext {
                preemption_flag,
                generator_scope,
            }));
            
            let result = QueueUserAPC(
                Some(preemption_apc_callback),
                thread_handle,
                ctx as ULONG_PTR,
            );
            
            if result == 0 {
                // Failed to queue APC, clean up the context
                drop(Box::from_raw(ctx));
                return Err(super::PreemptionError::InterruptFailed);
            }
            
            Ok(())
        }
    }
}

// ============================================================================
// Alertable Wait Support (Windows)
// ============================================================================

#[cfg(windows)]
pub(crate) mod alertable_wait {
    use std::time::Duration;
    use winapi::um::synchapi::WaitForSingleObjectEx;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::winbase::{INFINITE, WAIT_OBJECT_0, WAIT_IO_COMPLETION};
    use winapi::um::synchapi::CreateEventW;
    use winapi::um::winnt::HANDLE;
    
    /// An event handle for alertable waits
    pub struct AlertableEvent {
        handle: HANDLE,
    }
    
    impl AlertableEvent {
        pub fn new() -> Result<Self, std::io::Error> {
            unsafe {
                let handle = CreateEventW(
                    std::ptr::null_mut(),  // default security
                    0,                      // auto-reset
                    0,                      // initially non-signaled
                    std::ptr::null(),       // no name
                );
                
                if handle.is_null() {
                    return Err(std::io::Error::last_os_error());
                }
                
                Ok(Self { handle })
            }
        }
        
        /// Wait in an alertable state, allowing APCs to execute
        /// This is how QueueUserAPC can inject our preemption callback
        pub fn wait_alertable(&self, timeout: Option<Duration>) -> WaitResult {
            unsafe {
                let timeout_ms = match timeout {
                    Some(d) => d.as_millis().min(INFINITE as u128) as u32,
                    None => INFINITE,
                };
                
                let result = WaitForSingleObjectEx(self.handle, timeout_ms, 1 /* bAlertable=TRUE */);
                
                match result {
                    WAIT_OBJECT_0 => WaitResult::Signaled,
                    WAIT_IO_COMPLETION => WaitResult::ApcExecuted,  // APC was executed!
                    _ => WaitResult::Timeout,
                }
            }
        }
        
        /// Signal the event to wake one waiting thread
        /// This is used by WorkerWaker::release() to wake parked workers
        pub fn signal(&self) {
            unsafe {
                winapi::um::synchapi::SetEvent(self.handle);
            }
        }
    }
    
    impl Drop for AlertableEvent {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
    
    unsafe impl Send for AlertableEvent {}
    unsafe impl Sync for AlertableEvent {}
    
    #[derive(Debug, PartialEq, Eq)]
    pub enum WaitResult {
        Signaled,
        ApcExecuted,  // This means our preemption APC ran!
        Timeout,
    }
}
