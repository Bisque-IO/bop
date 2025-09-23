//! Safe(ish) Rust wrappers around uSockets (`us_*`) bindings from `bop-sys`.
//!
//! This module provides ergonomic types for loop, timers, socket contexts, TCP sockets,
//! and UDP sockets by installing static trampolines that dispatch to user-provided
//! Rust closures stored in the respective `ext` areas (uSockets extension memory).
//!
//! Safety notes:
//! - Callbacks run on the uSockets loop thread; assume single-threaded `FnMut`.
//! - We store a Box pointer in the `ext` area; do not mix these wrappers with
//!   manual `ext` usage on the same objects.
//! - All FFI calls remain `unsafe` internally; wrappers aim to reduce, not remove, risk.
//!
//!

use bop_sys::{self as sys};
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::mem::{ManuallyDrop, size_of};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use thiserror::Error;

/// Comprehensive error type for uSockets operations
#[derive(Error, Debug)]
pub enum UsError {
    #[error("Loop already running")]
    LoopAlreadyRunning,

    #[error("Loop creation failed: {message}")]
    LoopCreation { message: String },

    #[error("Loop extension is null")]
    LoopExtNull,

    #[error("Timer creation failed: {message}")]
    TimerCreation { message: String },

    #[error("Timer extension is null")]
    TimerExtNull,

    #[error("Socket context creation failed: {message}")]
    SocketContextCreation { message: String },

    #[error("Socket context extension is null")]
    SocketContextExtNull,

    #[error("Socket extension is null")]
    SocketExtNull,

    #[error("Invalid host string: {host}")]
    InvalidHostString { host: String },

    #[error("Invalid source host string: {host}")]
    InvalidSourceHostString { host: String },

    #[error("Must be called on the event loop thread")]
    NotOnLoopThread,

    #[error("Loop is not running")]
    LoopNotRunning,

    #[error("Mutex lock failed: {operation}")]
    MutexLockFailed { operation: String },

    #[error("Timer loop is null")]
    TimerLoopNull,

    #[error("System resource allocation failed")]
    AllocationFailed,

    #[error("Network operation failed: {operation}")]
    NetworkOperation { operation: String },

    #[error("Invalid string conversion")]
    InvalidStringConversion,
}

// ----- Utility: store/retrieve Box<T> pointer in ext memory -----

// New direct ext area functions
unsafe fn ext_write<T>(ext: *mut c_void, value: T) {
    unsafe { (ext as *mut T).write(value) };
}

unsafe fn ext_read<T: Copy>(ext: *mut c_void) -> T {
    unsafe { (ext as *mut T).read() }
}

unsafe fn ext_as_mut<'a, T>(ext: *mut c_void) -> &'a mut T {
    unsafe { &mut *(ext as *mut T) }
}

unsafe fn ext_as_ref<'a, T>(ext: *mut c_void) -> &'a T {
    unsafe { &*(ext as *mut T) }
}

// Convert ThreadId to u64 for atomic storage
fn thread_id_to_u64(id: thread::ThreadId) -> u64 {
    id.as_u64().get()
}

// ----- Loop -----

pub struct Loop<T>
where
    T: Default + Sized,
{
    inner: Arc<LoopInner<T>>,
}

impl<T: Default + Sized> Loop<T> {
    pub fn new() -> Result<Self, UsError> {
        Self::with_ext(T::default())
    }

    pub fn with_ext(ext_data: T) -> Result<Self, UsError> {
        let inner = Arc::new(LoopInner::<T>::with_ext(ext_data)?);
        Ok(Self { inner })
    }

    pub fn handle(&self) -> LoopHandle<T> {
        LoopHandle {
            inner: self.inner.clone(),
        }
    }

    pub fn create_timer(&self) -> Result<Timer<T>, UsError> {
        Timer::new(&self.inner)
    }

    pub fn create_socket_context<const SSL: bool, S: Default + Sized>(
        &self,
        options: SocketContextOptions,
    ) -> Result<SocketContext<SSL, T, S>, UsError> {
        SocketContext::new(&self.inner, options)
    }

    /// Run the loop
    pub fn run(self) -> Result<(), UsError> {
        self.inner.run()
    }

    pub fn run_simple(self) -> Result<(), UsError> {
        self.inner.run_simple()
    }
}

pub struct LoopMut<'a, T>
where
    T: Default + Sized,
{
    inner: &'a LoopInner<T>,
}

impl<'a, T: Default + Sized> LoopMut<'a, T> {
    pub fn handle(&self) -> LoopHandle<T> {
        LoopHandle {
            inner: self.inner.state().run_ptr.as_ref().unwrap().clone(),
        }
    }

    pub fn create_timer<S: Default + Sized>(&self) -> Result<Timer<S>, UsError> {
        Timer::new(self.inner)
    }

    pub fn create_socket_context<const SSL: bool, S: Default + Sized>(
        &self,
        options: SocketContextOptions,
    ) -> Result<SocketContext<SSL, T, S>, UsError> {
        SocketContext::new(self.inner, options)
    }

    pub fn defer<F>(&self, callback: F) -> Result<(), UsError>
    where
        F: Into<LoopCallback<T>>,
    {
        self.inner.defer(callback)
    }
}

/// LoopHandle is a handle to a loop. It can be used to defer callbacks to the loop
/// from any thread.
#[derive(Clone)]
pub struct LoopHandle<T = ()>
where
    T: Default + Sized,
{
    inner: Arc<LoopInner<T>>,
}

unsafe impl<T> Send for LoopHandle<T> where T: Default + Sized {}
unsafe impl<T> Sync for LoopHandle<T> where T: Default + Sized {}

impl<T: Default + Sized> LoopHandle<T> {
    pub fn is_on_loop_thread(&self) -> bool {
        self.inner.is_on_loop_thread()
    }

    pub fn is_loop_running(&self) -> bool {
        self.inner.is_loop_running()
    }

    pub fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }

    pub fn defer<F>(&self, callback: F) -> Result<(), UsError>
    where
        F: Into<LoopCallback<T>>,
    {
        self.inner.defer(callback)
    }
}

pub struct LoopInner<T = ()>
where
    T: Default + Sized,
{
    ptr: *mut sys::us_loop_t,
    _phantom: std::marker::PhantomData<T>,
}

unsafe impl<T> Send for LoopInner<T> where T: Default + Sized {}
unsafe impl<T> Sync for LoopInner<T> where T: Default + Sized {}

pub struct LoopDefer<T>
where
    T: Default + Sized,
{
    // Dual queue system - we swap which queue is current
    queues: [Vec<LoopCallback<T>>; 2],
    current_queue: usize, // 0 or 1, which queue receives new callbacks
    needs_wake: bool,     // whether a wakeup is needed to process callbacks
    loop_running: bool,
}

pub struct Task(pub Box<dyn FnOnce() + Send + 'static>);

/// LoopCallback is a callback that can be deferred to the loop thread
pub enum LoopCallback<T>
where
    T: Default + Sized,
{
    Box(Box<dyn FnOnce(LoopMut<T>) + Send + 'static>),
    Arc(Arc<dyn Fn(LoopMut<T>) + Send + 'static>),
    Fn(fn(LoopMut<T>)),
}

impl<T> From<Box<dyn FnOnce(LoopMut<T>) + Send + 'static>> for LoopCallback<T>
where
    T: Default + Sized,
{
    fn from(f: Box<dyn FnOnce(LoopMut<T>) + Send + 'static>) -> Self {
        LoopCallback::Box(f)
    }
}

impl<T> From<Arc<dyn Fn(LoopMut<T>) + Send + Sync>> for LoopCallback<T>
where
    T: Default + Sized,
{
    fn from(f: Arc<dyn Fn(LoopMut<T>) + Send + Sync>) -> Self {
        LoopCallback::Arc(f)
    }
}

impl<T> From<fn(LoopMut<T>)> for LoopCallback<T>
where
    T: Default + Sized,
{
    fn from(f: fn(LoopMut<T>)) -> Self {
        LoopCallback::Fn(f)
    }
}

impl<T> LoopCallback<T>
where
    T: Default + Sized,
{
    pub fn invoke(self, loop_mut: LoopMut<T>) {
        match self {
            LoopCallback::Box(f) => f(loop_mut),
            LoopCallback::Arc(f) => f(loop_mut),
            LoopCallback::Fn(f) => f(loop_mut),
        }
    }
}

unsafe impl<T> Send for LoopCallback<T> where T: Default + Sized {}
unsafe impl<T> Sync for LoopCallback<T> where T: Default + Sized {}

pub struct LoopState<T>
where
    T: Default + Sized,
{
    id_counter: AtomicU64,
    run_ptr: Option<Arc<LoopInner<T>>>,
    on_wakeup: Option<Box<dyn FnMut()>>,
    on_pre: Option<Box<dyn FnMut()>>,
    on_post: Option<Box<dyn FnMut()>>,
    // Deferred callback queues - both protected by same mutex for atomic queue swapping
    deferred: Mutex<LoopDefer<T>>,
    timer: Mutex<Option<Timer<()>>>, // Optional timer to keep loop alive
    loop_thread_id: AtomicU64,       // Track which thread is running the loop (0 = none)
    before_run: AtomicBool,
    // User ext data embedded in our state
    ext: T,
}

impl<T: Default + Sized> LoopInner<T> {
    pub fn new() -> Result<Self, UsError> {
        Self::with_ext(T::default())
    }

    /// Get a reference to the user ext data
    /// Create a new loop with custom ext data
    pub fn with_ext(ext_data: T) -> Result<Self, UsError> {
        let ptr = unsafe {
            sys::us_create_loop(
                null_mut(),
                Some(Self::wakeup_cb),
                Some(Self::pre_cb),
                Some(Self::post_cb),
                size_of::<LoopState<T>>() as c_uint,
            )
        };
        if ptr.is_null() {
            return Err(UsError::LoopCreation {
                message: "us_create_loop returned null".to_string(),
            });
        }

        // Create the state with user ext data
        let state = ManuallyDrop::new(LoopState {
            id_counter: AtomicU64::new(0),
            run_ptr: None,
            on_wakeup: None,
            on_pre: None,
            on_post: None,
            deferred: Mutex::new(LoopDefer {
                queues: [Vec::new(), Vec::new()],
                current_queue: 0,
                needs_wake: false,
                loop_running: false,
            }),
            timer: Mutex::new(None),
            loop_thread_id: AtomicU64::new(thread_id_to_u64(thread::current().id())),
            before_run: AtomicBool::new(false),
            ext: ext_data,
        });

        let loop_ = Self {
            ptr,
            _phantom: PhantomData,
        };

        // *state.timer.lock().unwrap() = Some(Timer::new(&loop_)?);

        // Write the state directly into the ext area
        unsafe {
            let ext = sys::us_loop_ext(ptr);
            if ext.is_null() {
                return Err(UsError::LoopExtNull);
            }
            ext_write(ext, state);
        }

        Ok(loop_)
    }

    fn state(&self) -> &LoopState<T> {
        unsafe {
            let ext = sys::us_loop_ext(self.ptr);
            if ext.is_null() {
                panic!("Loop ext is null");
            }
            ext_as_ref::<LoopState<T>>(ext)
        }
    }

    fn state_mut(&self) -> &mut LoopState<T> {
        unsafe {
            let ext = sys::us_loop_ext(self.ptr);
            if ext.is_null() {
                panic!("Loop ext is null");
            }
            ext_as_mut::<LoopState<T>>(ext)
        }
    }

    unsafe extern "C" fn wakeup_cb(loop_: *mut sys::us_loop_t) {
        let state = unsafe { ext_as_mut::<LoopState<T>>(sys::us_loop_ext(loop_)) };
        // Swap current defer queue (like the C++ code)
        let old_defer_queue = if let Ok(mut deferred) = state.deferred.lock() {
            let old_queue = deferred.current_queue;
            deferred.current_queue = (deferred.current_queue + 1) % 2;
            deferred.needs_wake = false;
            &mut deferred.queues[old_queue] as *mut Vec<LoopCallback<T>>
        } else {
            return; // Mutex poisoned, skip processing
        }; // Mutex is released here

        // Process callbacks from old queue - no lock needed since nobody else touches it
        let old_queue = unsafe { &mut (*old_defer_queue) };

        let inner_ref = state.run_ptr.as_ref().unwrap().as_ref();

        // Process all callbacks without any locks
        for callback in old_queue.drain(..) {
            let loop_mut = LoopMut::<T> { inner: inner_ref };
            callback.invoke(loop_mut);
        }
    }

    unsafe extern "C" fn pre_cb(loop_: *mut sys::us_loop_t) {
        println!("prec callback");
    }

    unsafe extern "C" fn post_cb(loop_: *mut sys::us_loop_t) {
        println!("post callback");
    }

    pub fn ext(&self) -> &T {
        &self.state().ext
    }

    /// Get a mutable reference to the user ext data
    pub fn ext_mut(&mut self) -> &mut T {
        &mut self.state_mut().ext
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_loop_t {
        self.ptr
    }

    pub fn on_wakeup<F>(&mut self, cb: F)
    where
        F: FnMut() + 'static,
    {
        self.state_mut().on_wakeup = Some(Box::new(cb));
    }

    pub fn on_pre<F>(&mut self, cb: F)
    where
        F: FnMut() + 'static,
    {
        self.state_mut().on_pre = Some(Box::new(cb));
    }

    pub fn on_post<F>(&mut self, cb: F)
    where
        F: FnMut() + 'static,
    {
        self.state_mut().on_post = Some(Box::new(cb));
    }

    pub fn run_simple(self: Arc<Self>) -> Result<(), UsError> {
        unsafe { sys::us_loop_run(self.ptr) }
        Ok(())
    }

    #[inline]
    fn run(self: Arc<Self>) -> Result<(), UsError> {
        if let Ok(mut deferred) = self.state().deferred.lock() {
            if deferred.loop_running {
                return Err(UsError::LoopAlreadyRunning);
            }
            deferred.loop_running = true;
        }

        // Set the current thread as the loop thread
        let current_thread_id = thread_id_to_u64(thread::current().id());
        self.state()
            .loop_thread_id
            .store(current_thread_id, Ordering::Relaxed);

        if let Ok(mut t) = self.state().timer.lock() {
            if t.is_none() {
                // Create a timer to keep the loop alive
                if let Ok(mut timer) = Timer::new(&self) {
                    let _ = timer.set(1, 0, move |_timer| {
                        // Timer callback does nothing; just keeps loop alive
                    });
                    *t = Some(timer);
                }
            }
        }

        self.state_mut().run_ptr = Some(self.clone());

        // run the loop to completion
        unsafe { sys::us_loop_run(self.ptr) }

        // flip loop running flag
        self.state().deferred.lock().unwrap().loop_running = false;

        // process deferred callbacks
        unsafe { Self::wakeup_cb(self.ptr) };

        // Clear the thread ID when loop exits
        self.state().loop_thread_id.store(0, Ordering::Relaxed);

        // Clear the run pointer to fix circular reference
        self.state_mut().run_ptr = None;

        self.state().timer.lock().unwrap().take();

        Ok(())
    }

    #[inline]
    pub fn wakeup(&self) {
        unsafe { sys::us_wakeup_loop(self.ptr) }
    }

    #[inline]
    pub fn integrate(&self) {
        // unsafe { sys::us_loop_integrate(self.ptr) }
    }

    /// Execute callback on the loop thread - inline if already on loop thread, deferred otherwise
    /// Returns Ok(true) if executed inline, Ok(false) if deferred, Err if loop not running
    pub fn run_on_loop<F>(&self, callback: F) -> Result<bool, UsError>
    where
        F: Into<LoopCallback<T>>,
    {
        // Check if we're already on the loop thread
        if self.is_on_loop_thread() {
            let loop_mut = LoopMut::<T> {
                inner: self.state().run_ptr.as_ref().unwrap().as_ref(),
            };
            // Execute inline
            callback.into().invoke(loop_mut);
            Ok(true)
        } else {
            // Check if loop is running before deferring
            if !self.is_loop_running() {
                return Err(UsError::LoopNotRunning);
            }
            // Defer to loop thread
            self.defer(callback.into())?;
            Ok(false)
        }
    }

    /// Check if the loop is currently running (has an active thread)
    pub fn is_loop_running(&self) -> bool {
        self.state().loop_thread_id.load(Ordering::Relaxed) != 0
    }

    pub fn is_valid(&self) -> bool {
        if self.ptr.is_null() {
            return false;
        }
        let state = self.state();
        if state.before_run.load(Ordering::Relaxed) {
            return true;
        }

        let current_thread_id = thread_id_to_u64(thread::current().id());
        let loop_thread_id = state.loop_thread_id.load(Ordering::Relaxed);

        if loop_thread_id != 0 && loop_thread_id == current_thread_id {
            return true;
        }

        false
    }

    /// Check if current thread is the loop thread
    pub fn is_on_loop_thread(&self) -> bool {
        let state = self.state();
        let current_thread_id = thread_id_to_u64(thread::current().id());
        let loop_thread_id = state.loop_thread_id.load(Ordering::Relaxed);
        loop_thread_id != 0 && loop_thread_id == current_thread_id
    }

    /// Queue a callback to be executed on the next wakeup in a thread-safe manner.
    /// The callback will be executed on the event loop thread when wakeup() is called.
    pub fn defer<F>(&self, callback: F) -> Result<(), UsError>
    where
        F: Into<LoopCallback<T>>,
    {
        let state = self.state();
        if state.loop_thread_id.load(Ordering::Relaxed) == 0 {
            return Err(UsError::LoopNotRunning);
        }

        // Check if loop is running
        let mut needs_wakeup = false;

        // Add callback to current queue
        if let Ok(mut deferred) = state.deferred.lock() {
            let current_idx = deferred.current_queue;
            deferred.queues[current_idx].push(callback.into());
            if !deferred.needs_wake {
                deferred.needs_wake = true;
                needs_wakeup = true;
            }
        } else {
            return Err(UsError::MutexLockFailed {
                operation: "deferred callbacks".to_string(),
            });
        }

        if needs_wakeup {
            self.wakeup();
        }

        Ok(())
    }

    #[inline]
    pub fn iteration_number(&self) -> i64 {
        unsafe { sys::us_loop_iteration_number(self.ptr) as i64 }
    }
}

// Generic implementation for custom ext types

// Generic Drop implementation for Loop<T>
impl<T: Default + Sized> Drop for LoopInner<T> {
    fn drop(&mut self) {
        unsafe {
            println!("Dropping LoopInner");
            // Drop the ext data in place
            // let ext = sys::us_loop_ext(self.ptr);
            // if !ext.is_null() {
            //     std::ptr::drop_in_place(ext as *mut LoopState<T>);
            // }
            // Free the loop
            // sys::us_loop_free(self.ptr);
        }
    }
}

// Note: Generic Drop implementation removed to avoid conflicts
// Users of Loop<T> where T != () should manually handle cleanup

// ----- Timer -----

pub struct Timer<T = ()>
where
    T: Default + Sized,
{
    ptr: *mut sys::us_timer_t,
    _phantom: PhantomData<T>,
}

pub struct TimerState<T = ()>
where
    T: Default + Sized,
{
    delay: i32,
    repeat_delay: i32,
    cb: Option<Box<dyn FnMut(&mut Timer<T>)>>,
    // User ext data embedded in our state
    ext: T,
}

impl<T: Default + Sized> Timer<T> {
    fn new<L: Default + Sized>(loop_: &LoopInner<L>) -> Result<Self, UsError> {
        unsafe {
            let ptr = sys::us_create_timer(loop_.as_ptr(), 0, size_of::<TimerState<T>>() as c_uint);
            if ptr.is_null() {
                return Err(UsError::TimerCreation {
                    message: "Failed to create timer".to_string(),
                });
            }

            let ext = sys::us_timer_ext(ptr);
            if ext.is_null() {
                return Err(UsError::TimerExtNull);
            }

            // Initialize TimerState<T> directly in the ext area
            let timer_state = ManuallyDrop::new(TimerState {
                delay: 0,
                repeat_delay: 0,
                cb: None,
                ext: T::default(),
            });
            ext_write(ext, timer_state);

            Ok(Timer {
                ptr,
                _phantom: PhantomData,
            })
        }
    }

    pub fn state(&self) -> &TimerState<T> {
        unsafe {
            let ext = sys::us_timer_ext(self.ptr);
            if ext.is_null() {
                panic!("Timer ext is null");
            }
            ext_as_ref::<TimerState<T>>(ext)
        }
    }

    pub fn state_mut(&mut self) -> &mut TimerState<T> {
        unsafe {
            let ext = sys::us_timer_ext(self.ptr);
            if ext.is_null() {
                panic!("Timer ext is null");
            }
            ext_as_mut::<TimerState<T>>(ext)
        }
    }

    /// Get a reference to the user ext data
    pub fn ext(&self) -> Option<&T> {
        unsafe {
            let ext = sys::us_timer_ext(self.ptr);
            if ext.is_null() {
                None
            } else {
                let timer_state = ext_as_ref::<TimerState<T>>(ext);
                Some(&timer_state.ext)
            }
        }
    }

    /// Get a mutable reference to the user ext data
    pub fn ext_mut(&mut self) -> Option<&mut T> {
        unsafe {
            let ext = sys::us_timer_ext(self.ptr);
            if ext.is_null() {
                None
            } else {
                let timer_state = ext_as_mut::<TimerState<T>>(ext);
                Some(&mut timer_state.ext)
            }
        }
    }

    unsafe extern "C" fn timer_cb(t: *mut sys::us_timer_t) {
        let ext = unsafe { sys::us_timer_ext(t) };
        if ext.is_null() {
            return;
        }
        // let loop_inner = LoopInner::<T> {
        //     ptr: unsafe { sys::us_timer_loop(t) },
        //     _phantom: PhantomData,
        // };
        // let loop_mut = LoopMut::<T> { inner: &loop_inner };
        let state = unsafe { ext_as_mut::<TimerState<T>>(ext) };
        let mut timer = ManuallyDrop::new(Timer {
            ptr: t,
            _phantom: PhantomData,
        });
        if let Some(cb) = state.cb.as_mut() {
            cb(&mut timer);
        }
    }

    pub fn reset<F>(&mut self, ms: i32, repeat_ms: i32) -> Result<(), UsError> {
        unsafe {
            sys::us_timer_set(
                self.ptr,
                Some(Self::timer_cb),
                ms as c_int,
                repeat_ms as c_int,
            );
        }
        Ok(())
    }

    pub fn set<F>(&mut self, ms: i32, repeat_ms: i32, cb: F) -> Result<(), UsError>
    where
        F: FnMut(&mut Timer<T>) + Send + 'static,
    {
        // Get the loop from the timer
        let loop_ptr = unsafe { sys::us_timer_loop(self.ptr) };
        if loop_ptr.is_null() {
            return Err(UsError::TimerLoopNull);
        }

        // Set the callback directly in the ext area
        let ext = unsafe { sys::us_timer_ext(self.ptr) };
        if ext.is_null() {
            return Err(UsError::TimerExtNull);
        }

        let state = unsafe { ext_as_mut::<TimerState<T>>(ext) };
        state.delay = ms;
        state.repeat_delay = repeat_ms;
        state.cb = Some(Box::new(cb));
        unsafe {
            sys::us_timer_set(
                self.ptr,
                Some(Self::timer_cb),
                ms as c_int,
                repeat_ms as c_int,
            );
        }

        Ok(())
    }

    pub fn close(self) -> Result<(), UsError> {
        // Get the loop from the timer
        let loop_ptr = unsafe { sys::us_timer_loop(self.ptr) };
        if loop_ptr.is_null() {
            return Err(UsError::TimerLoopNull);
        }

        // Create a Loop wrapper to check thread validity
        let loop_wrapper = LoopInner::<T> {
            ptr: loop_ptr,
            _phantom: PhantomData,
        };
        if !loop_wrapper.is_valid() {
            return Err(UsError::NotOnLoopThread);
        }

        unsafe {
            // Drop the TimerState in place
            let ext = sys::us_timer_ext(self.ptr);
            if !ext.is_null() {
                std::ptr::drop_in_place(ext as *mut TimerState<T>);
            }
            sys::us_timer_close(self.ptr);
        }

        // Prevent drop from running since we manually closed
        std::mem::forget(self);
        Ok(())
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_timer_t {
        self.ptr
    }
}

impl<T: Default + Sized> Drop for TimerState<T> {
    fn drop(&mut self) {
        println!("Dropping TimerState");
    }
}

impl<T: Default + Sized> Drop for Timer<T> {
    fn drop(&mut self) {
        println!("Dropping Timer");
        // Timer cleanup should happen on event loop thread
        unsafe {
            // Drop the ext data in place
            let ext = sys::us_timer_ext(self.ptr);
            if !ext.is_null() {
                std::ptr::drop_in_place(ext as *mut TimerState<T>);
            }
            sys::us_timer_close(self.ptr);
        }
    }
}

// ----- Socket & Context -----

pub struct SocketContext<const SSL: bool, T = (), S = ()>
where
    T: Default + Sized,
    S: Default + Sized,
{
    ptr: *mut sys::us_socket_context_t,
    id: u64,
    _phantom: PhantomData<(T, S)>,
}

// Type-erased callback storage that doesn't depend on generic parameters
struct SocketContextCallbacks<const SSL: bool, T, S>
where
    T: Default + Sized,
    S: Default + Sized,
{
    on_pre_open: Option<Box<dyn FnMut(sys::SOCKET) -> sys::SOCKET>>,
    on_open: Option<
        Box<
            dyn FnMut(
                &mut SocketContextState<SSL, T, S>,
                &mut Socket<SSL, S>,
                &mut SocketState<S>,
                bool,
                &[u8],
            ),
        >,
    >,
    on_close: Option<
        Box<
            dyn FnMut(
                &mut SocketContextState<SSL, T, S>,
                &mut Socket<SSL, S>,
                &mut SocketState<S>,
                i32,
            ),
        >,
    >,
    on_data: Option<
        Box<
            dyn FnMut(
                &mut SocketContextState<SSL, T, S>,
                &mut Socket<SSL, S>,
                &mut SocketState<S>,
                &mut [u8],
            ),
        >,
    >,
    on_writable: Option<
        Box<
            dyn FnMut(&mut SocketContextState<SSL, T, S>, &mut Socket<SSL, S>, &mut SocketState<S>),
        >,
    >,
    on_timeout: Option<
        Box<
            dyn FnMut(&mut SocketContextState<SSL, T, S>, &mut Socket<SSL, S>, &mut SocketState<S>),
        >,
    >,
    on_long_timeout: Option<
        Box<
            dyn FnMut(&mut SocketContextState<SSL, T, S>, &mut Socket<SSL, S>, &mut SocketState<S>),
        >,
    >,
    on_connect_error: Option<
        Box<
            dyn FnMut(
                &mut SocketContextState<SSL, T, S>,
                &mut Socket<SSL, S>,
                &mut SocketState<S>,
                i32,
            ),
        >,
    >,
    on_end: Option<
        Box<
            dyn FnMut(&mut SocketContextState<SSL, T, S>, &mut Socket<SSL, S>, &mut SocketState<S>),
        >,
    >,
    on_server_name: Option<Box<dyn FnMut(&mut SocketContextState<SSL, T, S>, &str)>>,
}

pub struct SocketContextState<const SSL: bool, T, S>
where
    T: Default + Sized,
    S: Default + Sized,
{
    // Type-erased callbacks stored first for easy access
    callbacks: SocketContextCallbacks<SSL, T, S>,
    // User ext data embedded in our state
    ext: T,
    // S is stored separately - we'll use phantom data for now
    _phantom_s: PhantomData<S>,
}

impl<const SSL: bool, T: Default + Sized, S: Default + Sized> SocketContext<SSL, T, S> {
    pub fn new(loop_: &LoopInner<T>, options: SocketContextOptions) -> Result<Self, UsError> {
        // Check if we're on the loop thread
        if !loop_.is_valid() {
            return Err(UsError::NotOnLoopThread);
        }

        // Hold CStrings alive during the FFI call; uSockets copies as needed.
        let key = options.key_file_name.and_then(|s| CString::new(s).ok());
        let cert = options.cert_file_name.and_then(|s| CString::new(s).ok());
        let pass = options.passphrase.and_then(|s| CString::new(s).ok());
        let dh = options
            .dh_params_file_name
            .and_then(|s| CString::new(s).ok());
        let ca = options.ca_file_name.and_then(|s| CString::new(s).ok());
        let ciph = options.ssl_ciphers.and_then(|s| CString::new(s).ok());
        let c_opts = sys::us_socket_context_options_t {
            key_file_name: key.as_ref().map_or(null(), |c| c.as_ptr()),
            cert_file_name: cert.as_ref().map_or(null(), |c| c.as_ptr()),
            passphrase: pass.as_ref().map_or(null(), |c| c.as_ptr()),
            dh_params_file_name: dh.as_ref().map_or(null(), |c| c.as_ptr()),
            ca_file_name: ca.as_ref().map_or(null(), |c| c.as_ptr()),
            ssl_ciphers: ciph.as_ref().map_or(null(), |c| c.as_ptr()),
            ssl_prefer_low_memory_usage: if options.ssl_prefer_low_memory_usage {
                1
            } else {
                0
            },
        };
        let ptr = unsafe {
            sys::us_create_socket_context(
                SSL as c_int,
                loop_.ptr,
                size_of::<SocketContextState<SSL, T, S>>() as c_int,
                c_opts,
            )
        };
        if ptr.is_null() {
            return Err(UsError::SocketContextCreation {
                message: "us_create_socket_context returned null".to_string(),
            });
        }

        // Create state directly in ext area
        let state = ManuallyDrop::new(SocketContextState::<SSL, T, S> {
            callbacks: SocketContextCallbacks {
                on_pre_open: None,
                on_open: None,
                on_close: None,
                on_data: None,
                on_writable: None,
                on_timeout: None,
                on_long_timeout: None,
                on_connect_error: None,
                on_end: None,
                on_server_name: None,
            },
            ext: T::default(),
            _phantom_s: PhantomData,
        });
        unsafe {
            let ext = sys::us_socket_context_ext(SSL as c_int, ptr);
            if ext.is_null() {
                return Err(UsError::SocketContextExtNull);
            }
            ext_write(ext, state);
        }

        // Register trampolines
        unsafe {
            sys::us_socket_context_on_pre_open(SSL as c_int, ptr, Some(Self::on_pre_open_cb));
            sys::us_socket_context_on_open(SSL as c_int, ptr, Some(Self::on_open_cb));
            sys::us_socket_context_on_close(SSL as c_int, ptr, Some(Self::on_close_cb));
            sys::us_socket_context_on_data(SSL as c_int, ptr, Some(Self::on_data_cb));
            sys::us_socket_context_on_writable(SSL as c_int, ptr, Some(Self::on_writable_cb));
            sys::us_socket_context_on_timeout(SSL as c_int, ptr, Some(Self::on_timeout_cb));
            sys::us_socket_context_on_long_timeout(
                SSL as c_int,
                ptr,
                Some(Self::on_long_timeout_cb),
            );
            sys::us_socket_context_on_connect_error(
                SSL as c_int,
                ptr,
                Some(Self::on_connect_error_cb),
            );
            sys::us_socket_context_on_end(SSL as c_int, ptr, Some(Self::on_end_cb));
            sys::us_socket_context_on_server_name(SSL as c_int, ptr, Some(Self::on_server_name_cb));
        }

        Ok(Self {
            ptr,
            id: loop_.state().id_counter.fetch_add(1, Ordering::Relaxed),
            _phantom: PhantomData,
        })
    }

    pub const fn is_ssl(&self) -> bool {
        SSL
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_socket_context_t {
        self.ptr
    }

    #[inline]
    pub fn loop_ptr(&self) -> *mut sys::us_loop_t {
        unsafe { sys::us_socket_context_loop(SSL as c_int, self.ptr) }
    }

    pub fn state(&self) -> &SocketContextState<SSL, T, S> {
        unsafe {
            let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
            if ext.is_null() {
                panic!("Socket context ext is null");
            }
            ext_as_ref::<SocketContextState<SSL, T, S>>(ext)
        }
    }

    pub fn state_mut(&mut self) -> &mut SocketContextState<SSL, T, S> {
        unsafe {
            let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
            if ext.is_null() {
                panic!("Socket context ext is null");
            }
            ext_as_mut::<SocketContextState<SSL, T, S>>(ext)
        }
    }

    pub fn ext(&self) -> Option<&T> {
        unsafe {
            let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
            if ext.is_null() {
                return None;
            }
            Some(&ext_as_ref::<SocketContextState<SSL, T, S>>(ext).ext)
        }
    }

    fn socket_and_context_state<'a>(
        s: *mut sys::us_socket_t,
    ) -> Option<(
        &'a mut SocketContextState<SSL, T, S>,
        &'a mut SocketContextCallbacks<SSL, T, S>,
        &'a mut SocketState<S>,
    )> {
        unsafe {
            let ext = sys::us_socket_ext(SSL as c_int, s);
            let ctx_ext =
                sys::us_socket_context_ext(SSL as c_int, sys::us_socket_context(SSL as c_int, s));
            if ext.is_null() || ctx_ext.is_null() {
                return None;
            }
            Some((
                ext_as_mut::<SocketContextState<SSL, T, S>>(ctx_ext),
                &mut ext_as_mut::<SocketContextState<SSL, T, S>>(ctx_ext).callbacks,
                ext_as_mut::<SocketState<S>>(ext),
            ))
        }
    }

    fn context_state<'a>(
        ctx: *mut sys::us_socket_context_t,
    ) -> Option<&'a mut SocketContextState<SSL, T, S>> {
        unsafe {
            let ext = sys::us_socket_context_ext(SSL as c_int, ctx);
            if ext.is_null() {
                return None;
            }
            Some(ext_as_mut::<SocketContextState<SSL, T, S>>(ext))
        }
    }

    fn context_state_and_callbacks<'a>(
        ctx: *mut sys::us_socket_context_t,
    ) -> Option<(
        &'a mut SocketContextState<SSL, T, S>,
        &'a mut SocketContextCallbacks<SSL, T, S>,
    )> {
        unsafe {
            let ctx_ext = sys::us_socket_context_ext(SSL as c_int, ctx);
            if ctx_ext.is_null() {
                return None;
            }
            Some((
                ext_as_mut::<SocketContextState<SSL, T, S>>(ctx_ext),
                &mut ext_as_mut::<SocketContextState<SSL, T, S>>(ctx_ext).callbacks,
            ))
        }
    }

    fn context_state_from_socket<'a>(
        s: *mut sys::us_socket_t,
    ) -> Option<&'a mut SocketContextState<SSL, T, S>> {
        unsafe {
            let ext =
                sys::us_socket_context_ext(SSL as c_int, sys::us_socket_context(SSL as c_int, s));
            if ext.is_null() {
                return None;
            }
            Some(ext_as_mut::<SocketContextState<SSL, T, S>>(ext))
        }
    }

    pub fn ext_mut(&mut self) -> Option<&mut T> {
        let ssl = self.is_ssl();
        unsafe {
            let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
            if ext.is_null() {
                return None;
            }
            Some(&mut ext_as_mut::<SocketContextState<SSL, T, S>>(ext).ext)
        }
    }

    unsafe extern "C" fn on_pre_open_cb(
        ctx: *mut sys::us_socket_context_t,
        fd: sys::SOCKET,
    ) -> sys::SOCKET {
        unsafe {
            let ext = sys::us_socket_context_ext(SSL as c_int, ctx);
            if !ext.is_null() {
                let state = &mut *(ext as *mut SocketContextState<SSL, T, S>);
                if let Some(cb) = state.callbacks.on_pre_open.as_mut() {
                    return cb(fd);
                }
            }
            fd
        }
    }

    unsafe extern "C" fn on_open_cb(
        s: *mut sys::us_socket_t,
        is_client: c_int,
        ip: *mut c_char,
        ip_len: c_int,
    ) -> *mut sys::us_socket_t {
        unsafe {
            if s.is_null() {
                return s;
            }

            let mut socket = if is_client == 0 {
                ManuallyDrop::new(Socket::<SSL, S>::new(s))
            } else {
                ManuallyDrop::new(Socket::<SSL, S> {
                    ptr: s,
                    _phantom: PhantomData,
                })
            };

            if let Some((ctx_state, callbacks, socket_state)) = Self::socket_and_context_state(s) {
                socket_state.is_client = is_client != 0;
                let ip_slice = if !ip.is_null() && ip_len > 0 {
                    std::slice::from_raw_parts(ip as *const u8, ip_len as usize)
                } else {
                    &[]
                };
                socket_state.remote_port = sys::us_socket_remote_port(SSL as c_int, s) as u16;

                if let Some(cb) = callbacks.on_open.as_mut() {
                    cb(
                        ctx_state,
                        &mut socket,
                        socket_state,
                        is_client != 0,
                        ip_slice,
                    );
                }
            }

            s
        }
    }

    unsafe extern "C" fn on_close_cb(
        s: *mut sys::us_socket_t,
        code: c_int,
        _reason: *mut c_void,
    ) -> *mut sys::us_socket_t {
        if s.is_null() {
            return s;
        }
        let mut socket = ManuallyDrop::new(Socket::<SSL, S> {
            ptr: s,
            _phantom: PhantomData,
        });
        if let Some((ctx_state, callbacks, socket_state)) = Self::socket_and_context_state(s) {
            if let Some(cb) = callbacks.on_close.as_mut() {
                cb(ctx_state, &mut socket, socket_state, code as i32);
            }
        }
        s
    }

    unsafe extern "C" fn on_data_cb(
        s: *mut sys::us_socket_t,
        data: *mut c_char,
        len: c_int,
    ) -> *mut sys::us_socket_t {
        if s.is_null() {
            return s;
        }
        let mut socket = ManuallyDrop::new(Socket::<SSL, S> {
            ptr: s,
            _phantom: PhantomData,
        });
        if let Some((ctx_state, callbacks, socket_state)) = Self::socket_and_context_state(s) {
            if let Some(cb) = callbacks.on_data.as_mut() {
                let slice = if !data.is_null() && len > 0 {
                    unsafe { std::slice::from_raw_parts_mut(data as *mut u8, len as usize) }
                } else {
                    &mut []
                };
                cb(ctx_state, &mut socket, socket_state, slice);
            }
        }
        s
    }

    unsafe extern "C" fn on_writable_cb(s: *mut sys::us_socket_t) -> *mut sys::us_socket_t {
        if s.is_null() {
            return s;
        }
        let mut socket = ManuallyDrop::new(Socket::<SSL, S> {
            ptr: s,
            _phantom: PhantomData,
        });
        if let Some((ctx_state, callbacks, socket_state)) = Self::socket_and_context_state(s) {
            if let Some(cb) = callbacks.on_writable.as_mut() {
                cb(ctx_state, &mut socket, socket_state);
            }
        }
        s
    }

    unsafe extern "C" fn on_timeout_cb(s: *mut sys::us_socket_t) -> *mut sys::us_socket_t {
        if s.is_null() {
            return s;
        }
        let mut socket = ManuallyDrop::new(Socket::<SSL, S> {
            ptr: s,
            _phantom: PhantomData,
        });
        if let Some((ctx_state, callbacks, socket_state)) = Self::socket_and_context_state(s) {
            if let Some(cb) = callbacks.on_timeout.as_mut() {
                cb(ctx_state, &mut socket, socket_state);
            }
        }
        s
    }

    unsafe extern "C" fn on_long_timeout_cb(s: *mut sys::us_socket_t) -> *mut sys::us_socket_t {
        if s.is_null() {
            return s;
        }
        let mut socket = ManuallyDrop::new(Socket::<SSL, S> {
            ptr: s,
            _phantom: PhantomData,
        });
        if let Some((ctx_state, callbacks, socket_state)) = Self::socket_and_context_state(s) {
            if let Some(cb) = callbacks.on_long_timeout.as_mut() {
                cb(ctx_state, &mut socket, socket_state);
            }
        }
        s
    }

    unsafe extern "C" fn on_connect_error_cb(
        s: *mut sys::us_socket_t,
        code: c_int,
    ) -> *mut sys::us_socket_t {
        if s.is_null() {
            return s;
        }
        let mut socket = ManuallyDrop::new(Socket::<SSL, S> {
            ptr: s,
            _phantom: PhantomData,
        });
        if let Some((ctx_state, callbacks, socket_state)) = Self::socket_and_context_state(s) {
            if let Some(cb) = callbacks.on_connect_error.as_mut() {
                cb(ctx_state, &mut socket, socket_state, code as i32);
            }
        }
        s
    }

    unsafe extern "C" fn on_end_cb(s: *mut sys::us_socket_t) -> *mut sys::us_socket_t {
        if s.is_null() {
            return s;
        }
        let mut socket = ManuallyDrop::new(Socket::<SSL, S> {
            ptr: s,
            _phantom: PhantomData,
        });
        if let Some((ctx_state, callbacks, socket_state)) = Self::socket_and_context_state(s) {
            if let Some(cb) = callbacks.on_end.as_mut() {
                cb(ctx_state, &mut socket, socket_state);
            }
        }
        s
    }

    unsafe extern "C" fn on_server_name_cb(
        ctx: *mut sys::us_socket_context_t,
        hostname: *const c_char,
    ) {
        if ctx.is_null() {
            return;
        }
        if let Some((ctx_state, callbacks)) = Self::context_state_and_callbacks(ctx) {
            let hostname = if hostname.is_null() {
                ""
            } else {
                unsafe { CStr::from_ptr(hostname) }.to_str().unwrap_or("")
            };
            if let Some(cb) = callbacks.on_server_name.as_mut() {
                return cb(ctx_state, hostname);
            }
        }
    }

    // pub fn on_open<F>(&mut self, cb: F) -> Result<(), &'static str>
    // where
    //     F: FnMut(&mut Socket<false>, bool, &[u8]) + 'static, // Use type-erased Socket
    // {
    //     // Check if we're on the loop thread
    //     let loop_wrapper = Loop::<T> { ptr: self.loop_ptr(), _phantom: PhantomData };
    //     if !loop_wrapper.is_valid() {
    //         return Err("Must be called on the event loop thread either before loop.run() or from within a callback ran on the loop thread");
    //     }

    //     unsafe {
    //         let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
    //         ext_as_mut::<SocketContextState<T, S>>(ext).callbacks.on_open = Some(Box::new(cb));
    //     }
    //     Ok(())
    // }

    // pub fn on_close<F>(&mut self, cb: F) -> Result<(), &'static str>
    // where
    //     F: FnMut(Socket<false>, i32) + 'static, // Use type-erased Socket
    // {
    //     // Check if we're on the loop thread
    //     let loop_wrapper = Loop::<T> { ptr: self.loop_ptr(), _phantom: PhantomData };
    //     if !loop_wrapper.is_valid() {
    //         return Err("Must be called on the event loop thread either before loop.run() or from within a callback ran on the loop thread");
    //     }

    //     unsafe {
    //         let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
    //         ext_as_mut::<SocketContextState<T, S>>(ext).callbacks.on_close = Some(Box::new(cb));
    //     }
    //     Ok(())
    // }

    // pub fn on_data<F>(&mut self, cb: F) -> Result<(), &'static str>
    // where
    //     F: FnMut(Socket<false>, &mut [u8]) + 'static,
    // {
    //     // Check if we're on the loop thread
    //     let loop_wrapper = Loop::<T> { ptr: self.loop_ptr(), _phantom: PhantomData };
    //     if !loop_wrapper.is_valid() {
    //         return Err("Must be called on the event loop thread either before loop.run() or from within a callback ran on the loop thread");
    //     }

    //     unsafe {
    //         let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
    //         ext_as_mut::<SocketContextState<T, S>>(ext).callbacks.on_data = Some(Box::new(cb));
    //     }
    //     Ok(())
    // }

    // pub fn on_writable<F>(&mut self, cb: F) -> Result<(), &'static str>
    // where
    //     F: FnMut(Socket<false>) + 'static,
    // {
    //     // Check if we're on the loop thread
    //     let loop_wrapper = Loop::<T> { ptr: self.loop_ptr(), _phantom: PhantomData };
    //     if !loop_wrapper.is_valid() {
    //         return Err("Must be called on the event loop thread either before loop.run() or from within a callback ran on the loop thread");
    //     }

    //     unsafe {
    //         let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
    //         ext_as_mut::<SocketContextState<T, S>>(ext).callbacks.on_writable = Some(Box::new(cb));
    //     }
    //     Ok(())
    // }

    // pub fn on_timeout<F>(&mut self, cb: F) -> Result<(), &'static str>
    // where
    //     F: FnMut(Socket<false>) + 'static,
    // {
    //     // Check if we're on the loop thread
    //     let loop_wrapper = Loop::<T> { ptr: self.loop_ptr(), _phantom: PhantomData };
    //     if !loop_wrapper.is_valid() {
    //         return Err("Must be called on the event loop thread either before loop.run() or from within a callback ran on the loop thread");
    //     }

    //     unsafe {
    //         let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
    //         ext_as_mut::<SocketContextState<T, S>>(ext).callbacks.on_timeout = Some(Box::new(cb));
    //     }
    //     Ok(())
    // }

    // pub fn on_long_timeout<F>(&mut self, cb: F) -> Result<(), &'static str>
    // where
    //     F: FnMut(Socket<false>) + 'static,
    // {
    //     // Check if we're on the loop thread
    //     let loop_wrapper = Loop::<T> { ptr: self.loop_ptr(), _phantom: PhantomData };
    //     if !loop_wrapper.is_valid() {
    //         return Err("Must be called on the event loop thread either before loop.run() or from within a callback ran on the loop thread");
    //     }

    //     unsafe {
    //         let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
    //         ext_as_mut::<SocketContextState<T, S>>(ext).callbacks.on_long_timeout = Some(Box::new(cb));
    //     }
    //     Ok(())
    // }

    // pub fn on_connect_error<F>(&mut self, cb: F) -> Result<(), &'static str>
    // where
    //     F: FnMut(Socket<false>, i32) + 'static,
    // {
    //     // Check if we're on the loop thread
    //     let loop_wrapper = Loop::<T> { ptr: self.loop_ptr(), _phantom: PhantomData };
    //     if !loop_wrapper.is_valid() {
    //         return Err("Must be called on the event loop thread either before loop.run() or from within a callback ran on the loop thread");
    //     }

    //     unsafe {
    //         let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
    //         ext_as_mut::<SocketContextState<T, S>>(ext).callbacks.on_connect_error = Some(Box::new(cb));
    //     }
    //     Ok(())
    // }

    // pub fn on_end<F>(&mut self, cb: F) -> Result<(), &'static str>
    // where
    //     F: FnMut(Socket<false>) + 'static,
    // {
    //     // Check if we're on the loop thread
    //     let loop_wrapper = Loop::<T> { ptr: self.loop_ptr(), _phantom: PhantomData };
    //     if !loop_wrapper.is_valid() {
    //         return Err("Must be called on the event loop thread either before loop.run() or from within a callback ran on the loop thread");
    //     }

    //     unsafe {
    //         let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
    //         ext_as_mut::<SocketContextState<T, S>>(ext).callbacks.on_end = Some(Box::new(cb));
    //     }
    //     Ok(())
    // }

    // pub fn on_server_name<F>(&mut self, cb: F) -> Result<(), &'static str>
    // where
    //     F: FnMut(&str) + 'static,
    // {
    //     // Check if we're on the loop thread
    //     let loop_wrapper = Loop::<T> { ptr: self.loop_ptr(), _phantom: PhantomData };
    //     if !loop_wrapper.is_valid() {
    //         return Err("Must be called on the event loop thread either before loop.run() or from within a callback ran on the loop thread");
    //     }

    //     unsafe {
    //         let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
    //         ext_as_mut::<SocketContextState<T, S>>(ext).callbacks.on_server_name = Some(Box::new(cb));
    //     }
    //     Ok(())
    // }

    pub fn listen(
        &self,
        host: &str,
        port: i32,
        options: i32,
    ) -> Result<Option<ListenSocket<SSL, S>>, UsError> {
        // Check if we're on the loop thread
        let loop_wrapper = LoopInner::<T> {
            ptr: self.loop_ptr(),
            _phantom: PhantomData,
        };
        if !loop_wrapper.is_valid() {
            return Err(UsError::NotOnLoopThread);
        }

        let chost = CString::new(host).map_err(|_| UsError::InvalidHostString {
            host: host.to_string(),
        })?;
        let ls = unsafe {
            sys::us_socket_context_listen(
                SSL as c_int,
                self.ptr,
                chost.as_ptr(),
                port as c_int,
                options as c_int,
                0,
            )
        };
        Ok(if ls.is_null() {
            None
        } else {
            Some(ListenSocket::<SSL, S> {
                ptr: ls,
                _phantom: PhantomData,
            })
        })
    }

    pub fn connect(
        &self,
        host: &str,
        port: i32,
        source_host: Option<&str>,
        options: i32,
    ) -> Result<Option<Socket<SSL, S>>, UsError> {
        // Check if we're on the loop thread
        let loop_wrapper = LoopInner::<T> {
            ptr: self.loop_ptr(),
            _phantom: PhantomData,
        };
        if !loop_wrapper.is_valid() {
            return Err(UsError::NotOnLoopThread);
        }

        let chost = CString::new(host).map_err(|_| UsError::InvalidHostString {
            host: host.to_string(),
        })?;
        let src = match source_host {
            Some(s) => {
                let cstring = CString::new(s).map_err(|_| UsError::InvalidSourceHostString {
                    host: s.to_string(),
                })?;
                cstring.into_raw()
            }
            None => null_mut(),
        };
        // Note: do not leak CString for src; re-wrap only if used
        let s_ptr = unsafe {
            sys::us_socket_context_connect(
                SSL as c_int,
                self.ptr,
                chost.as_ptr(),
                port as c_int,
                if src.is_null() {
                    null()
                } else {
                    src as *const c_char
                },
                options as c_int,
                0,
            )
        };
        // Reconstruct and drop src CString if any
        if !src.is_null() {
            unsafe {
                let _ = CString::from_raw(src);
            }
        }
        Ok(if s_ptr.is_null() {
            None
        } else {
            Some(Socket {
                ptr: s_ptr,
                _phantom: PhantomData,
            })
        })
    }

    // #[inline]
    // pub fn close(self) {
    //     let me = ManuallyDrop::new(self);
    //     unsafe { sys::us_socket_context_close(SSL as c_int, me.ptr) }
    // }
}

impl<const SSL: bool, T: Default + Sized, S: Default + Sized> Drop for SocketContext<SSL, T, S> {
    fn drop(&mut self) {
        unsafe {
            // State is now stored directly in ext area, no need to drop Box
            sys::us_socket_context_close(SSL as c_int, self.ptr);
            unsafe {
                // Drop the ext data in place
                let ext = sys::us_socket_context_ext(SSL as c_int, self.ptr);
                if !ext.is_null() {
                    std::ptr::drop_in_place(ext as *mut SocketContextState<SSL, T, S>);
                }
            }
            sys::us_socket_context_free(SSL as c_int, self.ptr);
        }
    }
}

pub struct ListenSocket<const SSL: bool, T>
where
    T: Default + Sized,
{
    ptr: *mut sys::us_listen_socket_t,
    _phantom: PhantomData<T>,
}

struct ListenSocketState<const SSL: bool, T> {
    // User ext data embedded
    ext: T,
}

impl<const SSL: bool, T: Default + Sized> ListenSocket<SSL, T> {
    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_listen_socket_t {
        self.ptr
    }

    pub fn close(self: Self) {}
}

impl<const SSL: bool, T: Default + Sized> Drop for ListenSocket<SSL, T> {
    fn drop(&mut self) {
        unsafe {
            // Drop the ListenSocketState in place
            let ext = sys::us_socket_ext(SSL as c_int, self.ptr as *mut sys::us_socket_t);
            if !ext.is_null() {
                std::ptr::drop_in_place(ext as *mut ListenSocketState<SSL, T>);
            }
            sys::us_listen_socket_close(SSL as c_int, self.ptr);
        }
    }
}

pub struct Socket<const SSL: bool, S = ()>
where
    S: Default + Sized,
{
    ptr: *mut sys::us_socket_t,
    _phantom: PhantomData<S>,
}

#[repr(C)]
struct SocketState<S> {
    is_closed: bool,
    is_client: bool,
    remote_port: u16,
    remote_ip: Option<Vec<u8>>,
    ext: S, // User ext data embedded
}

impl<const SSL: bool, S: Default + Sized> Socket<SSL, S> {
    fn new(ptr: *mut sys::us_socket_t) -> Self {
        // Initialize SocketState<S> directly in the ext area
        unsafe {
            let ext = sys::us_socket_ext(SSL as c_int, ptr);
            if !ext.is_null() {
                let socket_state = SocketState::<S> {
                    is_closed: false,
                    is_client: false,
                    remote_port: 0u16,
                    remote_ip: None,
                    ext: S::default(),
                };
                ext_write(ext, socket_state);
            }
        }
        Self {
            ptr,
            _phantom: PhantomData,
        }
    }

    fn from_ptr(ptr: *mut sys::us_socket_t) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self::new(ptr))
        }
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_socket_t {
        self.ptr
    }

    fn state(&self) -> &SocketState<S> {
        unsafe {
            let ext = sys::us_socket_ext(SSL as c_int, self.ptr);
            if ext.is_null() {
                panic!("us_socket_ext is null");
            }
            ext_as_ref::<SocketState<S>>(ext)
        }
    }

    fn state_mut(&mut self) -> &mut SocketState<S> {
        unsafe {
            let ext = sys::us_socket_ext(SSL as c_int, self.ptr);
            if ext.is_null() {
                panic!("us_socket_ext is null");
            }
            ext_as_mut::<SocketState<S>>(ext)
        }
    }

    fn ext(&self) -> &S {
        unsafe {
            let ext = sys::us_socket_ext(SSL as c_int, self.ptr);
            if ext.is_null() {
                panic!("us_socket_ext is null");
            }
            &ext_as_ref::<SocketState<S>>(ext).ext
        }
    }

    fn ext_mut(&mut self) -> &mut S {
        unsafe {
            let ext = sys::us_socket_ext(SSL as c_int, self.ptr);
            if ext.is_null() {
                panic!("us_socket_ext is null");
            }
            &mut ext_as_mut::<SocketState<S>>(ext).ext
        }
    }

    pub fn is_ssl(&self) -> bool {
        SSL
    }

    pub fn write(&mut self, data: &[u8], msg_more: bool) -> i32 {
        unsafe {
            sys::us_socket_write(
                SSL as c_int,
                self.ptr,
                data.as_ptr() as *const c_char,
                data.len() as c_int,
                if msg_more { 1 } else { 0 },
            ) as i32
        }
    }

    pub fn write2(&self, header: &[u8], payload: &[u8]) -> i32 {
        unsafe {
            sys::us_socket_write2(
                SSL as c_int,
                self.ptr,
                header.as_ptr() as *const c_char,
                header.len() as c_int,
                payload.as_ptr() as *const c_char,
                payload.len() as c_int,
            ) as i32
        }
    }

    #[inline]
    pub fn flush(&mut self) {
        unsafe { sys::us_socket_flush(SSL as c_int, self.ptr) }
    }

    #[inline]
    pub fn shutdown(&mut self) {
        unsafe { sys::us_socket_shutdown(SSL as c_int, self.ptr) }
    }

    #[inline]
    pub fn shutdown_read(&mut self) {
        unsafe { sys::us_socket_shutdown_read(SSL as c_int, self.ptr) }
    }

    #[inline]
    pub fn is_shut_down(&mut self) -> bool {
        unsafe { sys::us_socket_is_shut_down(SSL as c_int, self.ptr) != 0 }
    }

    #[inline]
    pub fn is_closed(&mut self) -> bool {
        unsafe { sys::us_socket_is_closed(SSL as c_int, self.ptr) != 0 }
    }

    #[inline]
    pub fn set_timeout(&mut self, seconds: u32) {
        unsafe { sys::us_socket_timeout(SSL as c_int, self.ptr, seconds as c_uint) }
    }

    #[inline]
    pub fn set_long_timeout(&mut self, minutes: u32) {
        unsafe { sys::us_socket_long_timeout(SSL as c_int, self.ptr, minutes as c_uint) }
    }

    #[inline]
    pub fn local_port(&mut self) -> i32 {
        unsafe { sys::us_socket_local_port(SSL as c_int, self.ptr) as i32 }
    }

    #[inline]
    pub fn remote_port(&mut self) -> i32 {
        unsafe { sys::us_socket_remote_port(SSL as c_int, self.ptr) as i32 }
    }

    pub fn remote_address(&mut self) -> Option<String> {
        unsafe {
            let mut buf = vec![0_i8; 256];
            let mut len: c_int = buf.len() as c_int;
            sys::us_socket_remote_address(SSL as c_int, self.ptr, buf.as_mut_ptr(), &mut len);
            if len <= 0 || (len as usize) > buf.len() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(buf.as_ptr() as *const u8, len as usize);
            std::str::from_utf8(bytes).map(|s| s.to_string()).ok()
        }
    }

    #[inline]
    pub fn close(&mut self, code: i32) {
        unsafe { sys::us_socket_close(SSL as c_int, self.ptr, code as c_int, null_mut()) };
    }
}

impl<const SSL: bool, S: Default + Sized> Drop for Socket<SSL, S> {
    fn drop(&mut self) {
        let state = self.state_mut();
        let is_closed = state.is_closed;

        // Drop the ext data in place
        unsafe { std::ptr::drop_in_place(state as *mut SocketState<S>) };
        if !is_closed {
            unsafe { sys::us_socket_close(SSL as c_int, self.ptr, 0, null_mut()) };
        }
    }
}

// ----- SocketContextOptions -----

#[derive(Default, Clone)]
pub struct SocketContextOptions {
    pub key_file_name: Option<String>,
    pub cert_file_name: Option<String>,
    pub passphrase: Option<String>,
    pub dh_params_file_name: Option<String>,
    pub ca_file_name: Option<String>,
    pub ssl_ciphers: Option<String>,
    pub ssl_prefer_low_memory_usage: bool,
}

impl SocketContextOptions {
    #[allow(dead_code)]
    fn into_ffi(self) -> sys::us_socket_context_options_t {
        // Keep CString values alive for this call only; uSockets copies as needed.
        sys::us_socket_context_options_t {
            key_file_name: self
                .key_file_name
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok())
                .map_or(null(), |c| c.into_raw() as *const c_char),
            cert_file_name: self
                .cert_file_name
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok())
                .map_or(null(), |c| c.into_raw() as *const c_char),
            passphrase: self
                .passphrase
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok())
                .map_or(null(), |c| c.into_raw() as *const c_char),
            dh_params_file_name: self
                .dh_params_file_name
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok())
                .map_or(null(), |c| c.into_raw() as *const c_char),
            ca_file_name: self
                .ca_file_name
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok())
                .map_or(null(), |c| c.into_raw() as *const c_char),
            ssl_ciphers: self
                .ssl_ciphers
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok())
                .map_or(null(), |c| c.into_raw() as *const c_char),
            ssl_prefer_low_memory_usage: if self.ssl_prefer_low_memory_usage {
                1
            } else {
                0
            },
        }
    }
}

// ----- UDP -----
// Guard UDP wrappers behind a feature to avoid unresolved symbols when
// the underlying native library is built without UDP support.
#[cfg(feature = "usockets-udp")]
pub struct UdpPacketBuffer {
    ptr: *mut sys::us_udp_packet_buffer_t,
}

#[cfg(feature = "usockets-udp")]
impl UdpPacketBuffer {
    pub fn new() -> Option<Self> {
        let ptr = unsafe { sys::us_create_udp_packet_buffer() };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_udp_packet_buffer_t {
        self.ptr
    }

    pub fn payload_mut(&self, index: i32) -> &mut [u8] {
        unsafe {
            let p = sys::us_udp_packet_buffer_payload(self.ptr, index as c_int) as *mut u8;
            let len = sys::us_udp_packet_buffer_payload_length(self.ptr, index as c_int) as usize;
            std::slice::from_raw_parts_mut(p, len)
        }
    }
}

#[cfg(feature = "usockets-udp")]
pub struct UdpSocket {
    ptr: *mut sys::us_udp_socket_t,
}

#[cfg(feature = "usockets-udp")]
struct UdpState {
    on_data: Option<Box<dyn FnMut(&mut UdpSocket, &mut UdpPacketBuffer, i32)>>,
    on_drain: Option<Box<dyn FnMut(&mut UdpSocket)>>,
}

#[cfg(feature = "usockets-udp")]
impl UdpSocket {
    pub fn create(
        loop_: &LoopInner,
        buf: &mut UdpPacketBuffer,
        host: Option<&str>,
        port: u16,
        on_data: Option<Box<dyn FnMut(&mut UdpSocket, &mut UdpPacketBuffer, i32)>>,
        on_drain: Option<Box<dyn FnMut(&mut UdpSocket)>>,
    ) -> Option<Self> {
        unsafe extern "C" fn data_cb(
            s: *mut sys::us_udp_socket_t,
            buf: *mut sys::us_udp_packet_buffer_t,
            num: c_int,
        ) {
            unsafe {
                let user = sys::us_udp_socket_user(s) as *mut UdpState;
                if user.is_null() {
                    return;
                }
                if let Some(cb) = (*user).on_data.as_mut() {
                    let mut sock = UdpSocket { ptr: s };
                    let mut pbuf = UdpPacketBuffer { ptr: buf };
                    cb(&mut sock, &mut pbuf, num as i32);
                }
            }
        }
        unsafe extern "C" fn drain_cb(s: *mut sys::us_udp_socket_t) {
            unsafe {
                let user = sys::us_udp_socket_user(s) as *mut UdpState;
                if user.is_null() {
                    return;
                }
                if let Some(cb) = (*user).on_drain.as_mut() {
                    let mut sock = UdpSocket { ptr: s };
                    cb(&mut sock);
                }
            }
        }

        let user = Box::new(UdpState { on_data, on_drain });
        let chost = host.and_then(|h| CString::new(h).ok());
        let ptr = unsafe {
            sys::us_create_udp_socket(
                loop_.ptr,
                buf.ptr,
                Some(data_cb),
                Some(drain_cb),
                chost.as_ref().map_or(null(), |c| c.as_ptr()),
                port as u16,
                Box::into_raw(user) as *mut c_void,
            )
        };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_udp_socket_t {
        self.ptr
    }

    pub fn bind(&mut self, hostname: &str, port: u32) -> i32 {
        let chost = CString::new(hostname).unwrap();
        unsafe { sys::us_udp_socket_bind(self.ptr, chost.as_ptr(), port as c_uint) as i32 }
    }

    #[inline]
    pub fn receive(&mut self, buf: &mut UdpPacketBuffer) -> i32 {
        unsafe { sys::us_udp_socket_receive(self.ptr, buf.ptr) as i32 }
    }

    pub fn send(&mut self, buf: &mut UdpPacketBuffer, num: i32) -> i32 {
        unsafe { sys::us_udp_socket_send(self.ptr, buf.ptr, num as c_int) as i32 }
    }

    #[inline]
    pub fn bound_port(&self) -> i32 {
        unsafe { sys::us_udp_socket_bound_port(self.ptr) as i32 }
    }
}

#[cfg(feature = "usockets-udp")]
impl Drop for UdpSocket {
    fn drop(&mut self) {
        // The C API lacks an explicit close for UDP sockets here; rely on loop teardown.
        // Ensure we drop user state if present.
        unsafe {
            let user = sys::us_udp_socket_user(self.ptr) as *mut UdpState;
            if !user.is_null() {
                drop(Box::from_raw(user));
            }
        }
    }
}
