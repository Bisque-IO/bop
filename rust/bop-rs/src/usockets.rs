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

use bop_sys as sys;
use std::ffi::{CStr, CString};
use std::mem::{size_of, ManuallyDrop};
use std::sync::{Arc, Mutex};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::ptr::{null, null_mut};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SslMode {
    Plain,
    Tls,
}

impl SslMode {
    #[inline]
    fn as_c_int(self) -> c_int {
        match self {
            SslMode::Plain => 0,
            SslMode::Tls => 1,
        }
    }
}

// ----- Utility: store/retrieve Box<T> pointer in ext memory -----

unsafe fn ext_set_ptr(ext: *mut c_void, ptr: *mut c_void) {
    // ext holds a single pointer-sized slot
    unsafe { (ext as *mut *mut c_void).write(ptr) };
}

unsafe fn ext_get_ptr<T>(ext: *mut c_void) -> *mut T {
    unsafe { (ext as *mut *mut c_void).read() as *mut T }
}

// ----- Loop -----

pub struct Loop {
    ptr: *mut sys::us_loop_t,
}

struct DeferredCallbacks {
    // Dual queue system - we swap which queue is current
    queues: [Vec<Box<dyn FnOnce() + Send>>; 2],
    queues_arcs: [Vec<Arc<dyn Fn() + Send>>; 2],
    current_queue: usize, // 0 or 1, which queue receives new callbacks
    needs_wake: bool,  // whether a wakeup is needed to process callbacks
}

struct LoopState {
    on_wakeup: Option<Box<dyn FnMut()>>,
    on_pre: Option<Box<dyn FnMut()>>,
    on_post: Option<Box<dyn FnMut()>>,
    // Deferred callback queues - both protected by same mutex for atomic queue swapping
    deferred_callbacks: Arc<Mutex<DeferredCallbacks>>,
}

impl Loop {
    pub fn new() -> Result<Self, &'static str> {
        unsafe extern "C" fn wakeup_trampoline(loop_: *mut sys::us_loop_t) {
            // First process deferred callbacks by swapping queues
            unsafe { 
                let ext = sys::us_loop_ext(loop_);
                if !ext.is_null() {
                    let state_box = ext_get_ptr::<LoopState>(ext);
                    if !state_box.is_null() {
                        let state = &mut *state_box;
                        // let mut old_queue_ptr: *mut Vec<Box<dyn FnOnce() + Send>> = core::ptr::null_mut();

                        // Swap current defer queue (like the C++ code)
                        let (old_defer_queue, old_defer_queue_arcs) = if let Ok(mut deferred) = state.deferred_callbacks.lock() {
                            let old_queue = deferred.current_queue;
                            deferred.current_queue = (deferred.current_queue + 1) % 2;
                            deferred.needs_wake = false;
                            (
                                &mut deferred.queues[old_queue] as *mut Vec<Box<dyn FnOnce() + Send>>, 
                                &mut deferred.queues_arcs[old_queue] as *mut Vec<Arc<dyn Fn() + Send>>
                            )
                        } else {
                            return; // Mutex poisoned, skip processing
                        }; // Mutex is released here
                        
                        // Process callbacks from old queue - no lock needed since nobody else touches it
                        let old_queue = &mut (*old_defer_queue);
                        let old_queue_arcs = &mut (*old_defer_queue_arcs);

                        // Process all callbacks without any locks (like C++)
                        for callback in old_queue.drain(..) {
                            callback();
                        }
                        for callback in old_queue_arcs.drain(..) {
                            callback();
                        }
                    }
                }
            }
            
            // Then run the normal wakeup handler if set
            unsafe { loop_dispatch(loop_, |s| &mut s.on_wakeup) }
        }
        unsafe extern "C" fn pre_trampoline(loop_: *mut sys::us_loop_t) {
            unsafe { loop_dispatch(loop_, |s| &mut s.on_pre) }
        }
        unsafe extern "C" fn post_trampoline(loop_: *mut sys::us_loop_t) {
            unsafe { loop_dispatch(loop_, |s| &mut s.on_post) }
        }

        unsafe fn loop_dispatch<F>(loop_: *mut sys::us_loop_t, sel: F)
        where
            F: FnOnce(&mut LoopState) -> &mut Option<Box<dyn FnMut()>>,
        {
            let ext = unsafe { sys::us_loop_ext(loop_) };
            if ext.is_null() {
                return;
            }
            let state_ptr = unsafe { ext_get_ptr::<LoopState>(ext) };
            if state_ptr.is_null() {
                return;
            }
            let state = unsafe { &mut *state_ptr };
            if let Some(cb) = sel(state).as_mut() {
                cb();
            }
        }

        let ptr = unsafe {
            sys::us_create_loop(
                null_mut(),
                Some(wakeup_trampoline),
                Some(pre_trampoline),
                Some(post_trampoline),
                size_of::<*mut c_void>() as c_uint,
            )
        };
        if ptr.is_null() {
            return Err("us_create_loop returned null");
        }
        // Install state into ext
        let state = Box::new(LoopState {
            on_wakeup: None,
            on_pre: None,
            on_post: None,
            deferred_callbacks: Arc::new(Mutex::new(DeferredCallbacks {
                queues: [Vec::new(), Vec::new()],
                queues_arcs: [Vec::new(), Vec::new()],
                current_queue: 0,
                needs_wake: false,
            })),
        });
        unsafe {
            let ext = sys::us_loop_ext(ptr);
            if ext.is_null() {
                return Err("us_loop_ext returned null");
            }
            ext_set_ptr(ext, Box::into_raw(state) as *mut c_void);
        }
        Ok(Self { ptr })
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_loop_t {
        self.ptr
    }

    pub fn on_wakeup<F>(&mut self, cb: F)
    where
        F: FnMut() + 'static,
    {
        unsafe {
            let ext = sys::us_loop_ext(self.ptr);
            let state_ptr = ext_get_ptr::<LoopState>(ext);
            if !state_ptr.is_null() {
                (*state_ptr).on_wakeup = Some(Box::new(cb));
            }
        }
    }

    pub fn on_pre<F>(&mut self, cb: F)
    where
        F: FnMut() + 'static,
    {
        unsafe {
            let ext = sys::us_loop_ext(self.ptr);
            let state_ptr = ext_get_ptr::<LoopState>(ext);
            if !state_ptr.is_null() {
                (*state_ptr).on_pre = Some(Box::new(cb));
            }
        }
    }

    pub fn on_post<F>(&mut self, cb: F)
    where
        F: FnMut() + 'static,
    {
        unsafe {
            let ext = sys::us_loop_ext(self.ptr);
            let state_ptr = ext_get_ptr::<LoopState>(ext);
            if !state_ptr.is_null() {
                (*state_ptr).on_post = Some(Box::new(cb));
            }
        }
    }

    #[inline]
    pub fn run(&self) {
        unsafe { sys::us_loop_run(self.ptr) }
    }

    #[inline]
    pub fn wakeup(&self) { unsafe { sys::us_wakeup_loop(self.ptr) } }

    #[inline]
    pub fn integrate(&self) {
        // unsafe { sys::us_loop_integrate(self.ptr) }
    }
    
    /// Queue a callback to be executed on the next wakeup in a thread-safe manner.
    /// The callback will be executed on the event loop thread when wakeup() is called.
    pub fn defer<F>(&self, callback: F) -> Result<(), &'static str> 
    where 
        F: FnOnce() + Send + 'static 
    {
        unsafe {
            let ext = sys::us_loop_ext(self.ptr);
            if ext.is_null() {
                return Err("Loop ext is null");
            }
            
            let state_box = ext_get_ptr::<LoopState>(ext);
            if state_box.is_null() {
                return Err("Loop state is null");
            }
            
            let state = &mut *state_box;
            let mut needs_wakeup = false;
            
            // Add callback to current queue
            if let Ok(mut deferred) = state.deferred_callbacks.lock() {
                let current_idx = deferred.current_queue;
                deferred.queues[current_idx].push(Box::new(callback));
                if !deferred.needs_wake {
                    deferred.needs_wake = true;
                    needs_wakeup = true;
                }
            } else {
                return Err("Failed to lock deferred callbacks mutex")
            }

            if needs_wakeup {
                self.wakeup();
            }

            Ok(())
        }
    }

    /// Queue a callback to be executed on the next wakeup in a thread-safe manner.
    /// The callback will be executed on the event loop thread when wakeup() is called.
    pub fn defer_arc<F>(&self, callback: Arc<F>) -> Result<(), &'static str> 
    where 
        F: Fn() + Send + 'static 
    {
        unsafe {
            let ext = sys::us_loop_ext(self.ptr);
            if ext.is_null() {
                return Err("Loop ext is null");
            }
            
            let state_box = ext_get_ptr::<LoopState>(ext);
            if state_box.is_null() {
                return Err("Loop state is null");
            }
            
            let state = &mut *state_box;
            let mut needs_wakeup = false;
            
            // Add callback to current queue
            if let Ok(mut deferred) = state.deferred_callbacks.lock() {
                let current_idx = deferred.current_queue;
                deferred.queues_arcs[current_idx].push(callback);
                if !deferred.needs_wake {
                    deferred.needs_wake = true;
                    needs_wakeup = true;
                }
            } else {
                return Err("Failed to lock deferred callbacks mutex")
            }

            if needs_wakeup {
                self.wakeup();
            }

            Ok(())
        }
    }

    #[inline]
    pub fn iteration_number(&self) -> i64 {
        unsafe { sys::us_loop_iteration_number(self.ptr) as i64 }
    }
}

impl Drop for Loop {
    fn drop(&mut self) {
        unsafe {
            let ext = sys::us_loop_ext(self.ptr);
            if !ext.is_null() {
                let state_ptr = ext_get_ptr::<LoopState>(ext);
                if !state_ptr.is_null() {
                    drop(Box::from_raw(state_ptr));
                }
            }
            sys::us_loop_free(self.ptr);
        }
    }
}

// ----- Timer -----

pub struct Timer {
    ptr: *mut sys::us_timer_t,
}

struct TimerState {
    cb: Option<Box<dyn FnMut()>>,
}

impl Timer {
    pub fn new(loop_: &Loop) -> Result<Self, &'static str> {
        unsafe extern "C" fn on_timer(t: *mut sys::us_timer_t) {
            unsafe {
                let ext = sys::us_timer_ext(t);
                if ext.is_null() {
                    return;
                }
                let state_ptr = ext_get_ptr::<TimerState>(ext);
                if state_ptr.is_null() {
                    return;
                }
                if let Some(cb) = (*state_ptr).cb.as_mut() {
                    cb();
                }
            }
        }

        let ptr = unsafe { sys::us_create_timer(loop_.ptr, 0, size_of::<*mut c_void>() as c_uint) };
        if ptr.is_null() {
            return Err("us_create_timer returned null");
        }
        let state = Box::new(TimerState { cb: None });
        unsafe {
            let ext = sys::us_timer_ext(ptr);
            if ext.is_null() {
                return Err("us_timer_ext returned null");
            }
            ext_set_ptr(ext, Box::into_raw(state) as *mut c_void);
            // Install the single trampoline once; actual closure can be swapped later
            sys::us_timer_set(ptr, Some(on_timer), 0, 0);
        }
        Ok(Self { ptr })
    }

    pub fn set<F>(&mut self, ms: i32, repeat_ms: i32, cb: F)
    where
        F: FnMut() + 'static,
    {
        unsafe {
            let ext = sys::us_timer_ext(self.ptr);
            let state_ptr = ext_get_ptr::<TimerState>(ext);
            if !state_ptr.is_null() {
                (*state_ptr).cb = Some(Box::new(cb));
            }
            sys::us_timer_set(self.ptr, None, ms as c_int, repeat_ms as c_int);
        }
    }

    #[inline]
    pub fn close(self) {
        let me = ManuallyDrop::new(self);
        unsafe { sys::us_timer_close(me.ptr) }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        unsafe {
            let ext = sys::us_timer_ext(self.ptr);
            if !ext.is_null() {
                let state_ptr = ext_get_ptr::<TimerState>(ext);
                if !state_ptr.is_null() {
                    drop(Box::from_raw(state_ptr));
                }
            }
            // Best-effort close; safe if already closed
            sys::us_timer_close(self.ptr);
        }
    }
}

// ----- Socket & Context -----

pub struct SocketContext {
    ptr: *mut sys::us_socket_context_t,
    ssl: SslMode,
}

struct SocketContextState {
    on_pre_open: Option<Box<dyn FnMut(sys::SOCKET) -> sys::SOCKET>>,
    on_open: Option<Box<dyn FnMut(Socket, bool, &[u8])>>,
    on_close: Option<Box<dyn FnMut(Socket, i32)>>,
    on_data: Option<Box<dyn FnMut(Socket, &mut [u8])>>,
    on_writable: Option<Box<dyn FnMut(Socket)>>,
    on_timeout: Option<Box<dyn FnMut(Socket)>>,
    on_long_timeout: Option<Box<dyn FnMut(Socket)>>,
    on_connect_error: Option<Box<dyn FnMut(Socket, i32)>>,
    on_end: Option<Box<dyn FnMut(Socket)>>,
    on_server_name: Option<Box<dyn FnMut(&str)>>,
}

impl SocketContext {
    pub fn new(loop_: &Loop, ssl: SslMode, options: SocketContextOptions) -> Result<Self, &'static str> {
        // Hold CStrings alive during the FFI call; uSockets copies as needed.
        let key = options.key_file_name.and_then(|s| CString::new(s).ok());
        let cert = options.cert_file_name.and_then(|s| CString::new(s).ok());
        let pass = options.passphrase.and_then(|s| CString::new(s).ok());
        let dh   = options.dh_params_file_name.and_then(|s| CString::new(s).ok());
        let ca   = options.ca_file_name.and_then(|s| CString::new(s).ok());
        let ciph = options.ssl_ciphers.and_then(|s| CString::new(s).ok());
        let c_opts = sys::us_socket_context_options_t {
            key_file_name: key.as_ref().map_or(null(), |c| c.as_ptr()),
            cert_file_name: cert.as_ref().map_or(null(), |c| c.as_ptr()),
            passphrase: pass.as_ref().map_or(null(), |c| c.as_ptr()),
            dh_params_file_name: dh.as_ref().map_or(null(), |c| c.as_ptr()),
            ca_file_name: ca.as_ref().map_or(null(), |c| c.as_ptr()),
            ssl_ciphers: ciph.as_ref().map_or(null(), |c| c.as_ptr()),
            ssl_prefer_low_memory_usage: if options.ssl_prefer_low_memory_usage { 1 } else { 0 },
        };
        let ptr = unsafe {
            sys::us_create_socket_context(
                ssl.as_c_int(),
                loop_.ptr,
                size_of::<*mut c_void>() as c_int,
                c_opts,
            )
        };
        if ptr.is_null() {
            return Err("us_create_socket_context returned null");
        }

        // State in ext
        let state = Box::new(SocketContextState {
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
        });
        unsafe {
            let ext = sys::us_socket_context_ext(ssl.as_c_int(), ptr);
            if ext.is_null() {
                return Err("us_socket_context_ext returned null");
            }
            ext_set_ptr(ext, Box::into_raw(state) as *mut c_void);
        }

        // Register static trampolines
        match ssl {
            SslMode::Plain => unsafe { self_register_trampolines::<Plain>(ptr) },
            SslMode::Tls => unsafe { self_register_trampolines::<Tls>(ptr) },
        }

        Ok(Self { ptr, ssl })
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_socket_context_t { self.ptr }

    #[inline]
    pub fn ssl_mode(&self) -> SslMode { self.ssl }

    #[inline]
    pub fn loop_ptr(&self) -> *mut sys::us_loop_t { unsafe { sys::us_socket_context_loop(self.ssl.as_c_int(), self.ptr) } }

    pub fn on_pre_open<F>(&mut self, cb: F)
    where
        F: FnMut(sys::SOCKET) -> sys::SOCKET + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_pre_open = Some(Box::new(cb)); }
    }

    pub fn on_open<F>(&mut self, cb: F)
    where
        F: FnMut(Socket, bool, &[u8]) + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_open = Some(Box::new(cb)); }
    }

    pub fn on_close<F>(&mut self, cb: F)
    where
        F: FnMut(Socket, i32) + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_close = Some(Box::new(cb)); }
    }

    pub fn on_data<F>(&mut self, cb: F)
    where
        F: FnMut(Socket, &mut [u8]) + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_data = Some(Box::new(cb)); }
    }

    pub fn on_writable<F>(&mut self, cb: F)
    where
        F: FnMut(Socket) + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_writable = Some(Box::new(cb)); }
    }

    pub fn on_timeout<F>(&mut self, cb: F)
    where
        F: FnMut(Socket) + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_timeout = Some(Box::new(cb)); }
    }

    pub fn on_long_timeout<F>(&mut self, cb: F)
    where
        F: FnMut(Socket) + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_long_timeout = Some(Box::new(cb)); }
    }

    pub fn on_connect_error<F>(&mut self, cb: F)
    where
        F: FnMut(Socket, i32) + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_connect_error = Some(Box::new(cb)); }
    }

    pub fn on_end<F>(&mut self, cb: F)
    where
        F: FnMut(Socket) + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_end = Some(Box::new(cb)); }
    }

    pub fn on_server_name<F>(&mut self, cb: F)
    where
        F: FnMut(&str) + 'static,
    {
        unsafe { ctx_state_mut(self.ptr, self.ssl).on_server_name = Some(Box::new(cb)); }
    }

    pub fn listen(&self, host: &str, port: i32, options: i32) -> Option<ListenSocket> {
        let chost = CString::new(host).ok()?;
        let ls = unsafe {
            sys::us_socket_context_listen(
                self.ssl.as_c_int(),
                self.ptr,
                chost.as_ptr(),
                port as c_int,
                options as c_int,
                0,
            )
        };
        if ls.is_null() { None } else { Some(ListenSocket { ptr: ls, ssl: self.ssl }) }
    }

    pub fn connect(&self, host: &str, port: i32, source_host: Option<&str>, options: i32) -> Option<Socket> {
        let chost = CString::new(host).ok()?;
        let src = match source_host { Some(s) => CString::new(s).ok()?.into_raw(), None => null_mut() };
        // Note: do not leak CString for src; re-wrap only if used
        let s_ptr = unsafe {
            sys::us_socket_context_connect(
                self.ssl.as_c_int(),
                self.ptr,
                chost.as_ptr(),
                port as c_int,
                if src.is_null() { null() } else { src as *const c_char },
                options as c_int,
                0,
            )
        };
        // Reconstruct and drop src CString if any
        if !src.is_null() { unsafe { let _ = CString::from_raw(src); } }
        if s_ptr.is_null() { None } else { Some(Socket { ptr: s_ptr, ssl: self.ssl }) }
    }

    #[inline]
    pub fn close(self) {
        let me = ManuallyDrop::new(self);
        unsafe { sys::us_socket_context_close(me.ssl.as_c_int(), me.ptr) }
    }
}

impl Drop for SocketContext {
    fn drop(&mut self) {
        unsafe {
            let ext = sys::us_socket_context_ext(self.ssl.as_c_int(), self.ptr);
            if !ext.is_null() {
                let state_ptr = ext_get_ptr::<SocketContextState>(ext);
                if !state_ptr.is_null() {
                    drop(Box::from_raw(state_ptr));
                }
            }
            sys::us_socket_context_free(self.ssl.as_c_int(), self.ptr);
        }
    }
}

pub struct ListenSocket {
    ptr: *mut sys::us_listen_socket_t,
    ssl: SslMode,
}

impl ListenSocket {
    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_listen_socket_t { self.ptr }
}

impl Drop for ListenSocket {
    fn drop(&mut self) {
        unsafe { sys::us_listen_socket_close(self.ssl.as_c_int(), self.ptr) }
    }
}

#[derive(Copy, Clone)]
pub struct Socket {
    ptr: *mut sys::us_socket_t,
    ssl: SslMode,
}

impl Socket {
    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_socket_t { self.ptr }

    pub fn write(&self, data: &[u8], msg_more: bool) -> i32 {
        unsafe {
            sys::us_socket_write(
                self.ssl.as_c_int(),
                self.ptr,
                data.as_ptr() as *const c_char,
                data.len() as c_int,
                if msg_more { 1 } else { 0 },
            ) as i32
        }
    }

    // Additional methods are implemented further below in a separate impl
}

// Marker types for trampoline selection
struct Plain;
struct Tls;

// Internal helpers to access context state
unsafe fn ctx_state_mut<'a>(ctx: *mut sys::us_socket_context_t, ssl: SslMode) -> &'a mut SocketContextState {
    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
    let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
    unsafe { &mut *state_ptr }
}

// Install all trampolines for a given context/ssl variant
unsafe fn self_register_trampolines<T>(_ctx: *mut sys::us_socket_context_t)
where
    T: TrampolineTag,
{
    // on_pre_open
    unsafe { sys::us_socket_context_on_pre_open(T::ssl(), _ctx, Some(T::on_pre_open())) };
    // on_open
    unsafe { sys::us_socket_context_on_open(T::ssl(), _ctx, Some(T::on_open())) };
    // on_close
    unsafe { sys::us_socket_context_on_close(T::ssl(), _ctx, Some(T::on_close())) };
    // on_data
    unsafe { sys::us_socket_context_on_data(T::ssl(), _ctx, Some(T::on_data())) };
    // on_writable
    unsafe { sys::us_socket_context_on_writable(T::ssl(), _ctx, Some(T::on_writable())) };
    // on_timeout
    unsafe { sys::us_socket_context_on_timeout(T::ssl(), _ctx, Some(T::on_timeout())) };
    // on_long_timeout
    unsafe { sys::us_socket_context_on_long_timeout(T::ssl(), _ctx, Some(T::on_long_timeout())) };
    // on_connect_error
    unsafe { sys::us_socket_context_on_connect_error(T::ssl(), _ctx, Some(T::on_connect_error())) };
    // on_end
    unsafe { sys::us_socket_context_on_end(T::ssl(), _ctx, Some(T::on_end())) };
    // on_server_name
    unsafe { sys::us_socket_context_on_server_name(T::ssl(), _ctx, Some(T::on_server_name())) };
}

trait TrampolineTag {
    fn ssl() -> c_int;
    // event callbacks
    fn on_pre_open() -> unsafe extern "C" fn(*mut sys::us_socket_context_t, sys::SOCKET) -> sys::SOCKET;
    fn on_open() -> unsafe extern "C" fn(*mut sys::us_socket_t, c_int, *mut c_char, c_int) -> *mut sys::us_socket_t;
    fn on_close() -> unsafe extern "C" fn(*mut sys::us_socket_t, c_int, *mut c_void) -> *mut sys::us_socket_t;
    fn on_data() -> unsafe extern "C" fn(*mut sys::us_socket_t, *mut c_char, c_int) -> *mut sys::us_socket_t;
    fn on_writable() -> unsafe extern "C" fn(*mut sys::us_socket_t) -> *mut sys::us_socket_t;
    fn on_timeout() -> unsafe extern "C" fn(*mut sys::us_socket_t) -> *mut sys::us_socket_t;
    fn on_long_timeout() -> unsafe extern "C" fn(*mut sys::us_socket_t) -> *mut sys::us_socket_t;
    fn on_connect_error() -> unsafe extern "C" fn(*mut sys::us_socket_t, c_int) -> *mut sys::us_socket_t;
    fn on_end() -> unsafe extern "C" fn(*mut sys::us_socket_t) -> *mut sys::us_socket_t;
    fn on_server_name() -> unsafe extern "C" fn(*mut sys::us_socket_context_t, *const c_char);
}

macro_rules! trampolines {
    ($name:ident, $ssl:expr) => {
        impl TrampolineTag for $name {
            #[inline] fn ssl() -> c_int { $ssl }

            fn on_pre_open() -> unsafe extern "C" fn(*mut sys::us_socket_context_t, sys::SOCKET) -> sys::SOCKET {
                unsafe extern "C" fn f(ctx: *mut sys::us_socket_context_t, fd: sys::SOCKET) -> sys::SOCKET {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_pre_open.as_mut() } {
                                return cb(fd);
                            }
                        }
                    }
                    fd
                }
                f
            }

            fn on_open() -> unsafe extern "C" fn(*mut sys::us_socket_t, c_int, *mut c_char, c_int) -> *mut sys::us_socket_t {
                unsafe extern "C" fn f(s: *mut sys::us_socket_t, is_client: c_int, ip: *mut c_char, ip_len: c_int) -> *mut sys::us_socket_t {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ctx = unsafe { sys::us_socket_context(ssl.as_c_int(), s) };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_open.as_mut() } {
                                let addr = if !ip.is_null() && ip_len > 0 {
                                    unsafe { std::slice::from_raw_parts(ip as *const u8, ip_len as usize) }
                                } else { &[] };
                                cb(Socket { ptr: s, ssl }, is_client != 0, addr);
                            }
                        }
                    }
                    s
                }
                f
            }

            fn on_close() -> unsafe extern "C" fn(*mut sys::us_socket_t, c_int, *mut c_void) -> *mut sys::us_socket_t {
                unsafe extern "C" fn f(s: *mut sys::us_socket_t, code: c_int, _reason: *mut c_void) -> *mut sys::us_socket_t {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ctx = unsafe { sys::us_socket_context(ssl.as_c_int(), s) };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_close.as_mut() } {
                                cb(Socket { ptr: s, ssl }, code as i32);
                            }
                        }
                    }
                    s
                }
                f
            }

            fn on_data() -> unsafe extern "C" fn(*mut sys::us_socket_t, *mut c_char, c_int) -> *mut sys::us_socket_t {
                unsafe extern "C" fn f(s: *mut sys::us_socket_t, data: *mut c_char, len: c_int) -> *mut sys::us_socket_t {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ctx = unsafe { sys::us_socket_context(ssl.as_c_int(), s) };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_data.as_mut() } {
                                let slice = if !data.is_null() && len > 0 {
                                    unsafe { std::slice::from_raw_parts_mut(data as *mut u8, len as usize) }
                                } else { &mut [] };
                                cb(Socket { ptr: s, ssl }, slice);
                            }
                        }
                    }
                    s
                }
                f
            }

            fn on_writable() -> unsafe extern "C" fn(*mut sys::us_socket_t) -> *mut sys::us_socket_t {
                unsafe extern "C" fn f(s: *mut sys::us_socket_t) -> *mut sys::us_socket_t {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ctx = unsafe { sys::us_socket_context(ssl.as_c_int(), s) };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_writable.as_mut() } {
                                cb(Socket { ptr: s, ssl });
                            }
                        }
                    }
                    s
                }
                f
            }

            fn on_timeout() -> unsafe extern "C" fn(*mut sys::us_socket_t) -> *mut sys::us_socket_t {
                unsafe extern "C" fn f(s: *mut sys::us_socket_t) -> *mut sys::us_socket_t {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ctx = unsafe { sys::us_socket_context(ssl.as_c_int(), s) };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_timeout.as_mut() } {
                                cb(Socket { ptr: s, ssl });
                            }
                        }
                    }
                    s
                }
                f
            }

            fn on_long_timeout() -> unsafe extern "C" fn(*mut sys::us_socket_t) -> *mut sys::us_socket_t {
                unsafe extern "C" fn f(s: *mut sys::us_socket_t) -> *mut sys::us_socket_t {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ctx = unsafe { sys::us_socket_context(ssl.as_c_int(), s) };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_long_timeout.as_mut() } {
                                cb(Socket { ptr: s, ssl });
                            }
                        }
                    }
                    s
                }
                f
            }

            fn on_connect_error() -> unsafe extern "C" fn(*mut sys::us_socket_t, c_int) -> *mut sys::us_socket_t {
                unsafe extern "C" fn f(s: *mut sys::us_socket_t, code: c_int) -> *mut sys::us_socket_t {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ctx = unsafe { sys::us_socket_context(ssl.as_c_int(), s) };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_connect_error.as_mut() } {
                                cb(Socket { ptr: s, ssl }, code as i32);
                            }
                        }
                    }
                    s
                }
                f
            }

            fn on_end() -> unsafe extern "C" fn(*mut sys::us_socket_t) -> *mut sys::us_socket_t {
                unsafe extern "C" fn f(s: *mut sys::us_socket_t) -> *mut sys::us_socket_t {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ctx = unsafe { sys::us_socket_context(ssl.as_c_int(), s) };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_end.as_mut() } {
                                cb(Socket { ptr: s, ssl });
                            }
                        }
                    }
                    s
                }
                f
            }

            fn on_server_name() -> unsafe extern "C" fn(*mut sys::us_socket_context_t, *const c_char) {
                unsafe extern "C" fn f(ctx: *mut sys::us_socket_context_t, hostname: *const c_char) {
                    let ssl = if $ssl == 0 { SslMode::Plain } else { SslMode::Tls };
                    let ext = unsafe { sys::us_socket_context_ext(ssl.as_c_int(), ctx) };
                    if !ext.is_null() {
                        let state_ptr = unsafe { ext_get_ptr::<SocketContextState>(ext) };
                        if !state_ptr.is_null() {
                            if let Some(cb) = unsafe { (*state_ptr).on_server_name.as_mut() } {
                                if !hostname.is_null() {
                                    if let Ok(s) = unsafe { CStr::from_ptr(hostname).to_str() } {
                                        cb(s);
                                    }
                                }
                            }
                        }
                    }
                }
                f
            }
        }
    };
}

trampolines!(Plain, 0);
trampolines!(Tls, 1);

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
            ssl_prefer_low_memory_usage: if self.ssl_prefer_low_memory_usage { 1 } else { 0 },
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
        if ptr.is_null() { None } else { Some(Self { ptr }) }
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_udp_packet_buffer_t { self.ptr }

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
        loop_: &Loop,
        buf: &mut UdpPacketBuffer,
        host: Option<&str>,
        port: u16,
        on_data: Option<Box<dyn FnMut(&mut UdpSocket, &mut UdpPacketBuffer, i32)>>,
        on_drain: Option<Box<dyn FnMut(&mut UdpSocket)>>,
    ) -> Option<Self> {
        unsafe extern "C" fn data_cb(s: *mut sys::us_udp_socket_t, buf: *mut sys::us_udp_packet_buffer_t, num: c_int) {
            unsafe {
                let user = sys::us_udp_socket_user(s) as *mut UdpState;
                if user.is_null() { return; }
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
                if user.is_null() { return; }
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
        if ptr.is_null() { None } else { Some(Self { ptr }) }
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::us_udp_socket_t { self.ptr }

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
    pub fn bound_port(&self) -> i32 { unsafe { sys::us_udp_socket_bound_port(self.ptr) as i32 } }
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

impl Socket {
    pub fn write2(&self, header: &[u8], payload: &[u8]) -> i32 {
        unsafe {
            sys::us_socket_write2(
                self.ssl.as_c_int(),
                self.ptr,
                header.as_ptr() as *const c_char,
                header.len() as c_int,
                payload.as_ptr() as *const c_char,
                payload.len() as c_int,
            ) as i32
        }
    }

    #[inline]
    pub fn flush(&self) { unsafe { sys::us_socket_flush(self.ssl.as_c_int(), self.ptr) } }

    #[inline]
    pub fn shutdown(&self) { unsafe { sys::us_socket_shutdown(self.ssl.as_c_int(), self.ptr) } }

    #[inline]
    pub fn shutdown_read(&self) { unsafe { sys::us_socket_shutdown_read(self.ssl.as_c_int(), self.ptr) } }

    #[inline]
    pub fn is_shut_down(&self) -> bool { unsafe { sys::us_socket_is_shut_down(self.ssl.as_c_int(), self.ptr) != 0 } }

    #[inline]
    pub fn is_closed(&self) -> bool { unsafe { sys::us_socket_is_closed(self.ssl.as_c_int(), self.ptr) != 0 } }

    #[inline]
    pub fn set_timeout(&self, seconds: u32) { unsafe { sys::us_socket_timeout(self.ssl.as_c_int(), self.ptr, seconds as c_uint) } }

    #[inline]
    pub fn set_long_timeout(&self, minutes: u32) { unsafe { sys::us_socket_long_timeout(self.ssl.as_c_int(), self.ptr, minutes as c_uint) } }

    #[inline]
    pub fn local_port(&self) -> i32 { unsafe { sys::us_socket_local_port(self.ssl.as_c_int(), self.ptr) as i32 } }

    #[inline]
    pub fn remote_port(&self) -> i32 { unsafe { sys::us_socket_remote_port(self.ssl.as_c_int(), self.ptr) as i32 } }

    pub fn remote_address(&self) -> Option<String> {
        unsafe {
            let mut buf = vec![0_i8; 256];
            let mut len: c_int = buf.len() as c_int;
            sys::us_socket_remote_address(self.ssl.as_c_int(), self.ptr, buf.as_mut_ptr(), &mut len);
            if len <= 0 || (len as usize) > buf.len() { return None; }
            let bytes = std::slice::from_raw_parts(buf.as_ptr() as *const u8, len as usize);
            std::str::from_utf8(bytes).map(|s| s.to_string()).ok()
        }
    }

    #[inline]
    pub fn close(&self, code: i32) { unsafe { sys::us_socket_close(self.ssl.as_c_int(), self.ptr, code as c_int, null_mut()) }; }
}
