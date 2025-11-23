//! A thread pool for isolating blocking I/O in async programs.
//!
//! Sometimes there's no way to avoid blocking I/O. Consider files or stdin, which have weak async
//! support on modern operating systems. While [IOCP], [AIO], and [io_uring] are possible
//! solutions, they're not always available or ideal.
//!
//! Since blocking is not allowed inside futures, we must move blocking I/O onto a special thread
//! pool provided by this module. The pool dynamically spawns and stops threads depending on the
//! current number of running I/O jobs.
//!
//! Note that there is a limit on the number of active threads. Once that limit is hit, a running
//! job has to finish before others get a chance to run. When a thread is idle, it waits for the
//! next job or shuts down after a certain timeout.
//!
//! The default number of threads (set to 500) can be altered by setting `BLOCKING_MAX_THREADS` environment
//! variable with value between 1 and 10000. This can also be set, at runtime, via the
//! [`set_max_blocking_threads`] function.
//!
//! [IOCP]: https://en.wikipedia.org/wiki/Input/output_completion_port
//! [AIO]: http://man7.org/linux/man-pages/man2/io_submit.2.html
//! [io_uring]: https://lwn.net/Articles/776703
//!
//! # Examples
//!
//! Read the contents of a file:
//!
//! ```no_run
//! use blocking::unblock;
//! use std::fs;
//!
//! # futures_lite::future::block_on(async {
//! let contents = unblock(|| fs::read_to_string("file.txt")).await?;
//! println!("{}", contents);
//! # std::io::Result::Ok(()) });
//! ```
//!
//! Read a file and pipe its contents to stdout:
//!
//! ```no_run
//! use blocking::{unblock, Unblock};
//! use futures_lite::io;
//! use std::fs::File;
//!
//! # futures_lite::future::block_on(async {
//! let input = unblock(|| File::open("file.txt")).await?;
//! let input = Unblock::new(input);
//! let mut output = Unblock::new(std::io::stdout());
//!
//! io::copy(input, &mut output).await?;
//! # std::io::Result::Ok(()) });
//! ```
//!
//! Iterate over the contents of a directory:
//!
//! ```no_run
//! use blocking::Unblock;
//! use futures_lite::prelude::*;
//! use std::fs;
//!
//! # futures_lite::future::block_on(async {
//! let mut dir = Unblock::new(fs::read_dir(".")?);
//! while let Some(item) = dir.next().await {
//!     println!("{}", item?.file_name().to_string_lossy());
//! }
//! # std::io::Result::Ok(()) });
//! ```
//!
//! Spawn a process:
//!
//! ```no_run
//! use blocking::unblock;
//! use std::process::Command;
//!
//! # futures_lite::future::block_on(async {
//! let out = unblock(|| Command::new("dir").output()).await?;
//! # std::io::Result::Ok(()) });
//! ```
use std::any::Any;
use std::cell::Cell;
use std::fmt;
use std::fs::{self, File, Metadata};
use std::future::Future;
use std::io;
use std::num::NonZeroUsize;
use std::panic;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;

#[cfg(not(target_family = "wasm"))]
use std::env;

#[cfg(unix)]
use libc::{SIGUSR1, pthread_kill, pthread_self, pthread_t};
#[cfg(unix)]
use std::os::unix::io::{FromRawFd, IntoRawFd};

// Platform-specific imports
#[cfg(unix)]
use std::os::unix::prelude::RawFd;
#[cfg(windows)]
use std::os::windows::io::{FromRawHandle, IntoRawHandle, RawHandle};

use flume;
use once_cell::sync::Lazy;

// Inline multishot module with cancellation support
mod multishot {
    // Loom exports inline
    mod loom_exports {
        #[cfg(all(test, multishot_loom))]
        pub(crate) mod sync {
            pub(crate) mod atomic {
                pub(crate) use loom::sync::atomic::AtomicUsize;
            }
        }
        #[cfg(not(all(test, multishot_loom)))]
        pub(crate) mod sync {
            pub(crate) mod atomic {
                pub(crate) use std::sync::atomic::AtomicUsize;
            }
        }

        #[cfg(all(test, multishot_loom))]
        pub(crate) mod cell {
            pub(crate) use loom::cell::UnsafeCell;
        }
        #[cfg(not(all(test, multishot_loom)))]
        pub(crate) mod cell {
            #[derive(Debug)]
            pub(crate) struct UnsafeCell<T>(std::cell::UnsafeCell<T>);

            impl<T> UnsafeCell<T> {
                pub(crate) fn new(data: T) -> UnsafeCell<T> {
                    UnsafeCell(std::cell::UnsafeCell::new(data))
                }
                pub(crate) fn with<R>(&self, f: impl FnOnce(*const T) -> R) -> R {
                    f(self.0.get())
                }
                pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
                    f(self.0.get())
                }
            }
        }
    }

    use self::loom_exports::cell::UnsafeCell;
    use self::loom_exports::sync::atomic::AtomicUsize;
    use std::error::Error;
    use std::fmt;
    use std::future::Future;
    use std::marker::PhantomData;
    use std::mem::{ManuallyDrop, MaybeUninit};
    use std::panic::{RefUnwindSafe, UnwindSafe};
    use std::pin::Pin;
    use std::ptr::{self, NonNull};
    use std::sync::atomic::Ordering;
    use std::task::{Context, Poll, Waker};

    #[cfg(unix)]
    use libc::{SIGUSR1, pthread_kill, pthread_t};

    const EMPTY: usize = 0b001;
    const OPEN: usize = 0b010;
    const INDEX: usize = 0b100;
    pub const EXECUTING: usize = 0b1000;
    const CANCELLED: usize = 0b10000;

    struct Inner<T> {
        state: AtomicUsize,
        value: UnsafeCell<MaybeUninit<T>>,
        waker: [UnsafeCell<Option<Waker>>; 2],
        // ID of the thread currently executing the sender (or 0)
        // On Windows, this stores the RawHandle for CancelIoEx
        cancel_handle: AtomicUsize,
    }

    impl<T> Inner<T> {
        unsafe fn write_value(&self, t: T) {
            unsafe {
                self.value.with_mut(|value| (*value).write(t));
            }
        }

        unsafe fn read_value(&self) -> T {
            unsafe { self.value.with(|value| (*value).as_ptr().read()) }
        }

        unsafe fn drop_value_in_place(&self) {
            unsafe {
                self.value
                    .with_mut(|value| ptr::drop_in_place((*value).as_mut_ptr()));
            }
        }

        unsafe fn set_waker(&self, idx: usize, new: Option<Waker>) {
            unsafe {
                self.waker[idx].with_mut(|waker| (*waker) = new);
            }
        }

        unsafe fn take_waker(&self, idx: usize) -> Option<Waker> {
            unsafe { self.waker[idx].with_mut(|waker| (*waker).take()) }
        }
    }

    #[derive(Debug)]
    pub struct Receiver<T> {
        inner: NonNull<Inner<T>>,
        _phantom: PhantomData<Inner<T>>,
    }

    impl<T> Receiver<T> {
        pub fn new() -> Self {
            Self {
                inner: NonNull::new(Box::into_raw(Box::new(Inner {
                    state: AtomicUsize::new(EMPTY),
                    value: UnsafeCell::new(MaybeUninit::uninit()),
                    waker: [UnsafeCell::new(None), UnsafeCell::new(None)],
                    cancel_handle: AtomicUsize::new(0),
                })))
                .unwrap(),
                _phantom: PhantomData,
            }
        }

        pub fn sender(&mut self) -> Option<Sender<T>> {
            let state = unsafe { self.inner.as_ref().state.load(Ordering::Acquire) };
            if state & OPEN == 0 {
                Some(unsafe { self.sender_with_waker(state, None) })
            } else {
                None
            }
        }

        pub fn recv(&mut self) -> Recv<'_, T> {
            Recv { receiver: self }
        }

        unsafe fn sender_with_waker(&mut self, state: usize, waker: Option<Waker>) -> Sender<T> {
            debug_assert!(state & OPEN == 0);
            if state & EMPTY == 0 {
                unsafe { self.inner.as_ref().drop_value_in_place() };
            }
            unsafe { self.inner.as_ref().set_waker(0, waker) };
            unsafe {
                self.inner
                    .as_ref()
                    .state
                    .store(OPEN | EMPTY, Ordering::Relaxed)
            };
            Sender {
                inner: self.inner,
                _phantom: PhantomData,
            }
        }

        pub fn is_executing(&self) -> bool {
            let state = unsafe { self.inner.as_ref().state.load(Ordering::Acquire) };
            state & EXECUTING != 0
        }

        pub fn is_open(&self) -> bool {
            let state = unsafe { self.inner.as_ref().state.load(Ordering::Acquire) };
            state & OPEN != 0
        }

        pub fn close_if_not_executing(self) -> Result<Option<Self>, Self> {
            let this = ManuallyDrop::new(self);
            let inner = unsafe { this.inner.as_ref() };
            let mut state = inner.state.load(Ordering::Acquire);

            loop {
                if state & OPEN == 0 {
                    return Ok(Some(ManuallyDrop::into_inner(this)));
                }
                if state & EXECUTING != 0 {
                    // If executing, we cannot close fully (because Sender is using Inner).
                    // But we can signal cancellation by setting CANCELLED bit.
                    if state & CANCELLED == 0 {
                        match inner.state.compare_exchange_weak(
                            state,
                            state | CANCELLED,
                            Ordering::AcqRel,
                            Ordering::Relaxed,
                        ) {
                            Ok(_) => {} // Signaled cancellation
                            Err(_) => {} // Retry loop next time
                        }
                    }
                    return Err(ManuallyDrop::into_inner(this));
                }
                match inner.state.compare_exchange_weak(
                    state,
                    0,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        unsafe {
                            if state & EMPTY == 0 {
                                inner.drop_value_in_place();
                            }
                            // We do NOT drop Inner here because OPEN was set.
                            // By clearing OPEN (setting state to 0), we transfer responsibility
                            // to the Sender (who will see OPEN gone and drop Inner).
                        }
                        return Ok(None);
                    }
                    Err(s) => state = s,
                }
            }
        }

        pub fn cancel_execution(&self) {
            let inner = unsafe { self.inner.as_ref() };
            let handle = inner.cancel_handle.load(Ordering::SeqCst);
            if handle != 0 {
                #[cfg(unix)]
                unsafe {
                    pthread_kill(handle as pthread_t, SIGUSR1);
                }
                #[cfg(windows)]
                unsafe {
                    use windows_sys::Win32::System::IO::CancelIoEx;
                    CancelIoEx(handle as _, std::ptr::null());
                }
            }
        }
    }

    unsafe impl<T: Send> Send for Receiver<T> {}
    unsafe impl<T: Send> Sync for Receiver<T> {}
    impl<T> UnwindSafe for Receiver<T> {}
    impl<T> RefUnwindSafe for Receiver<T> {}
    impl<T> Default for Receiver<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> Drop for Receiver<T> {
        fn drop(&mut self) {
            let state = unsafe { self.inner.as_ref().state.swap(0, Ordering::AcqRel) };
            if state & OPEN == OPEN {
                return;
            }
            unsafe {
                if state & EMPTY == 0 {
                    self.inner.as_ref().drop_value_in_place();
                }
                drop(Box::from_raw(self.inner.as_ptr()));
            }
        }
    }

    #[derive(Debug)]
    pub struct Recv<'a, T> {
        receiver: &'a mut Receiver<T>,
    }

    impl<'a, T> Recv<'a, T> {
        fn poll_complete(self: Pin<&mut Self>, state: usize) -> Poll<Result<T, RecvError>> {
            debug_assert!(state & OPEN == 0);
            let ret = if state & EMPTY == 0 {
                let value = unsafe { self.receiver.inner.as_ref().read_value() };
                Ok(value)
            } else {
                Err(RecvError {})
            };
            unsafe {
                self.receiver
                    .inner
                    .as_ref()
                    .state
                    .store(EMPTY, Ordering::Relaxed);
            }
            Poll::Ready(ret)
        }
    }

    impl<'a, T> Future for Recv<'a, T> {
        type Output = Result<T, RecvError>;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut state = unsafe { self.receiver.inner.as_ref().state.load(Ordering::Acquire) };
            if state & OPEN == 0 {
                return self.poll_complete(state);
            }
            if state & EMPTY == 0 {
                unsafe {
                    state = self
                        .receiver
                        .inner
                        .as_ref()
                        .state
                        .fetch_or(EMPTY, Ordering::Acquire);
                }
                if state & OPEN == 0 {
                    return self.poll_complete(state);
                }
            }
            let current_idx = state_to_index(state);
            let new_idx = 1 - current_idx;
            unsafe {
                self.receiver
                    .inner
                    .as_ref()
                    .set_waker(new_idx, Some(cx.waker().clone()));
            }
            let state = unsafe {
                self.receiver
                    .inner
                    .as_ref()
                    .state
                    .swap(index_to_state(current_idx) | OPEN, Ordering::AcqRel)
            };
            if state & OPEN == 0 {
                return self.poll_complete(state);
            }
            Poll::Pending
        }
    }

    #[derive(Debug)]
    pub struct Sender<T> {
        inner: NonNull<Inner<T>>,
        _phantom: PhantomData<Inner<T>>,
    }

    impl<T> Sender<T> {
        pub fn is_closed(&self) -> bool {
            let inner = unsafe { self.inner.as_ref() };
            let state = inner.state.load(Ordering::Acquire);
            (state & OPEN == 0) || (state & CANCELLED != 0)
        }

        pub fn enter_executing(&self, handle: usize) -> bool {
            let inner = unsafe { self.inner.as_ref() };
            // Register handle for cancellation
            unsafe { inner.cancel_handle.store(handle, Ordering::SeqCst) };

            let mut state = inner.state.load(Ordering::Acquire);
            loop {
                if state & OPEN == 0 {
                    return false;
                }
                match inner.state.compare_exchange_weak(
                    state,
                    state | EXECUTING,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return true,
                    Err(s) => state = s,
                }
            }
        }

        pub fn send(self, t: T) {
            let this = ManuallyDrop::new(self);
            unsafe { this.inner.as_ref().write_value(t) };
            let mut idx =
                state_to_index(unsafe { this.inner.as_ref().state.load(Ordering::Relaxed) });
            loop {
                let waker = unsafe { this.inner.as_ref().take_waker(idx) };
                let state = unsafe {
                    this.inner
                        .as_ref()
                        .state
                        .fetch_and(!(OPEN | EMPTY | EXECUTING | CANCELLED), Ordering::AcqRel)
                };
                unsafe {
                    if state & OPEN == 0 {
                        this.inner.as_ref().drop_value_in_place();
                        drop(Box::from_raw(this.inner.as_ptr()));
                        return;
                    }
                }
                if state & EMPTY == EMPTY {
                    if let Some(waker) = waker {
                        waker.wake()
                    }
                    return;
                }
                idx = 1 - idx;
            }
        }
    }

    unsafe impl<T: Send> Send for Sender<T> {}
    unsafe impl<T: Send> Sync for Sender<T> {}
    impl<T> UnwindSafe for Sender<T> {}
    impl<T> RefUnwindSafe for Sender<T> {}

    impl<T> Drop for Sender<T> {
        fn drop(&mut self) {
            let mut state = unsafe { self.inner.as_ref().state.load(Ordering::Relaxed) };
            let mut idx = state_to_index(state);
            loop {
                let waker = unsafe { self.inner.as_ref().take_waker(idx) };
                loop {
                    let new_state = if state & EMPTY == EMPTY {
                        EMPTY
                    } else {
                        state ^ (EMPTY | INDEX)
                    };
                    unsafe {
                        match self.inner.as_ref().state.compare_exchange_weak(
                            state,
                            new_state & !EXECUTING,
                            Ordering::AcqRel,
                            Ordering::Relaxed,
                        ) {
                            Ok(s) => {
                                state = s;
                                break;
                            }
                            Err(s) => state = s,
                        }
                    }
                }
                unsafe {
                    if state & OPEN == 0 {
                        drop(Box::from_raw(self.inner.as_ptr()));
                        return;
                    }
                }
                if state & EMPTY == EMPTY {
                    if let Some(waker) = waker {
                        waker.wake()
                    }
                    return;
                }
                idx = 1 - idx;
            }
        }
    }

    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub struct RecvError {}
    impl fmt::Display for RecvError {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(fmt, "channel closed")
        }
    }
    impl Error for RecvError {}

    pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
        let mut receiver = Receiver::new();
        let sender = receiver.sender().unwrap();
        (sender, receiver)
    }

    fn state_to_index(state: usize) -> usize {
        (state & INDEX) >> 2
    }
    fn index_to_state(index: usize) -> usize {
        index << 2
    }
}

pub use multishot::{Receiver, Recv, RecvError, Sender, channel};

#[cfg(unix)]
fn install_signal_handler() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_flags = 0; // Ensure SA_RESTART is NOT set
            sa.sa_sigaction = dummy_handler as usize;
            libc::sigaction(SIGUSR1, &sa, std::ptr::null_mut());
        }
    });
}

#[cfg(unix)]
extern "C" fn dummy_handler(_: libc::c_int) {}

/// A handle to a blocking task.
pub struct Task<T> {
    receiver: Option<Receiver<T>>,
    recycler: Option<fn(Receiver<T>)>,
    must_wait_on_drop: bool,
}

impl<T> Task<T> {
    fn new(receiver: Receiver<T>, recycler: Option<fn(Receiver<T>)>) -> Self {
        Self {
            receiver: Some(receiver),
            recycler,
            must_wait_on_drop: false,
        }
    }

    fn new_wait(receiver: Receiver<T>, recycler: Option<fn(Receiver<T>)>) -> Self {
        Self {
            receiver: Some(receiver),
            recycler,
            must_wait_on_drop: true,
        }
    }
}

impl<T> Future for Task<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Safety: We never pin the receiver or recycler.
        // They are just handles/pointers.
        // The inner state is on the heap.
        let this = unsafe { self.get_unchecked_mut() };
        let recv = this
            .receiver
            .as_mut()
            .expect("Task polled after completion")
            .recv();

        // Create a pinned reference to the Recv future on the stack
        let mut recv = recv;
        let pinned = Pin::new(&mut recv);
        match pinned.poll(cx) {
            Poll::Ready(Ok(val)) => Poll::Ready(val),
            Poll::Ready(Err(_)) => panic!("Blocking task panicked or executor shut down"),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> Task<T> {
    /// Detach the task to let it run in the background.
    pub fn detach(self) {}
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        if let Some(mut rx) = self.receiver.take() {
            if self.must_wait_on_drop {
                // Safe cancellation loop for unsafe operations (e.g. raw pointers).
                // We repeatedly try to close the channel.
                // If executing, we receive the receiver back and spin.
                // If queued, we close it (Ok(None)) and stop.
                // If finished, we receive it back (Ok(Some)) and recycle.
                loop {
                    match rx.close_if_not_executing() {
                        Ok(None) => {
                            // Successfully closed (Queued task cancelled).
                            // Inner dropped. We are done.
                            break;
                        }
                        Ok(Some(rx_back)) => {
                            // Already closed (Task finished).
                            // Recycle.
                            if let Some(recycler) = self.recycler {
                                recycler(rx_back);
                            }
                            break;
                        }
                        Err(rx_back) => {
                            // Executing. Wait and retry.
                            rx_back.cancel_execution();
                            std::hint::spin_loop();
                            rx = rx_back;
                        }
                    }
                }
            } else {
                // Safe task (e.g. Execute, Open).
                // If Open (Queued or Executing): Drop (Cancel/Detach).
                // If Closed (Finished): Recycle.
                // Note: We rely on drop(rx) to clear OPEN flag if it was set.
                if rx.is_open() {
                    drop(rx);
                } else if let Some(recycler) = self.recycler {
                    recycler(rx);
                }
            }
        }
    }
}

const MAX_POOL_SIZE: usize = 10;

// Pools for reusing channels to avoid allocation
macro_rules! define_pool {
    ($name:ident, $type:ty, $recycle_fn:ident, $get_fn:ident) => {
        thread_local! {
            static $name: Cell<Vec<Receiver<$type>>> = Cell::new(Vec::new());
        }

        fn $recycle_fn(rx: Receiver<$type>) {
            $name.with(|pool| {
                let mut vec = pool.take();
                if vec.len() < MAX_POOL_SIZE {
                    vec.push(rx);
                }
                pool.set(vec);
            });
        }

        fn $get_fn() -> (Sender<$type>, Receiver<$type>) {
            let rx = $name.with(|pool| {
                let mut vec = pool.take();
                let item = vec.pop();
                pool.set(vec);
                item
            });

            if let Some(mut rx) = rx {
                if let Some(tx) = rx.sender() {
                    return (tx, rx);
                }
            }
            multishot::channel()
        }
    };
}

define_pool!(FILE_POOL, io::Result<File>, recycle_file, get_file_chan);
define_pool!(
    METADATA_POOL,
    io::Result<Metadata>,
    recycle_metadata,
    get_metadata_chan
);
define_pool!(VOID_POOL, io::Result<()>, recycle_void, get_void_chan);
define_pool!(
    IO_SIZE_POOL,
    io::Result<usize>,
    recycle_io_size,
    get_io_size_chan
);

// Helper type to make raw pointers Send
pub struct Ptr<T>(pub *mut T);

unsafe impl<T> Send for Ptr<T> {}

pub enum BlockingTask {
    Execute(Box<dyn FnOnce() + Send>),
    Open(PathBuf, Sender<io::Result<File>>),
    Create(PathBuf, Sender<io::Result<File>>),
    RemoveFile(PathBuf, Sender<io::Result<()>>),
    RemoveDir(PathBuf, Sender<io::Result<()>>),
    CreateDir(PathBuf, Sender<io::Result<()>>),
    Rename(PathBuf, PathBuf, Sender<io::Result<()>>),
    Metadata(PathBuf, Sender<io::Result<Metadata>>),
    // Zero-allocation file descriptor operations - Unix
    #[cfg(unix)]
    FMetadata(RawFd, Sender<io::Result<Metadata>>),
    #[cfg(unix)]
    FRead(RawFd, Ptr<u8>, usize, Sender<io::Result<usize>>),
    #[cfg(unix)]
    FReadAt(RawFd, Ptr<u8>, usize, u64, Sender<io::Result<usize>>),
    #[cfg(unix)]
    FWrite(RawFd, Ptr<u8>, usize, Sender<io::Result<usize>>),
    #[cfg(unix)]
    FWriteAt(RawFd, Ptr<u8>, usize, u64, Sender<io::Result<usize>>),
    #[cfg(unix)]
    FSync(RawFd, bool, Sender<io::Result<()>>),
    // Zero-allocation file descriptor operations - Windows
    #[cfg(windows)]
    FMetadata(RawHandle, Sender<io::Result<Metadata>>),
    #[cfg(windows)]
    FRead(RawHandle, Ptr<u8>, usize, Sender<io::Result<usize>>),
    #[cfg(windows)]
    FReadAt(RawHandle, Ptr<u8>, usize, u64, Sender<io::Result<usize>>),
    #[cfg(windows)]
    FWrite(RawHandle, Ptr<u8>, usize, Sender<io::Result<usize>>),
    #[cfg(windows)]
    FWriteAt(RawHandle, Ptr<u8>, usize, u64, Sender<io::Result<usize>>),
    #[cfg(windows)]
    FSync(RawHandle, bool, Sender<io::Result<()>>),
}

unsafe impl Send for BlockingTask {}

/// Default value for max threads that Executor can grow to
#[cfg(not(target_family = "wasm"))]
const DEFAULT_MAX_THREADS: NonZeroUsize = {
    if let Some(size) = NonZeroUsize::new(500) {
        size
    } else {
        panic!("DEFAULT_MAX_THREADS is non-zero");
    }
};

/// Minimum value for max threads config
#[cfg(not(target_family = "wasm"))]
const MIN_MAX_THREADS: usize = 1;

/// Maximum value for max threads config
#[cfg(not(target_family = "wasm"))]
const MAX_MAX_THREADS: usize = 10000;

/// Env variable that allows to override default value for max threads.
#[cfg(not(target_family = "wasm"))]
const MAX_THREADS_ENV: &str = "BLOCKING_MAX_THREADS";

/// Set the maximum number of threads used by the backing thread pool.
///
/// # Example
///
/// ```no_run
/// use blocking::unblock;
/// use std::fs::{read_dir, File};
/// use std::io::prelude::*;
/// # use std::num::NonZeroUsize;
///
/// blocking::set_max_blocking_threads(NonZeroUsize::new(100).unwrap());
///
/// # fn test() -> std::io::Result<()> {
/// let mut files = Vec::new();
/// for entry in read_dir("/path/to/large/directory").unwrap() {
///     files.push(unblock(move || -> std::io::Result<String> {
///         let mut contents = String::new();
///         let mut file = File::open(entry?.path())?;
///         file.read_to_string(&mut contents)?;
///         Ok(contents)
///     }));
/// }
/// # Ok(())
/// # }
/// ```
pub fn set_max_blocking_threads(threads: NonZeroUsize) {
    let executor = BlockingExecutor::get();
    executor.thread_limit.store(threads.get(), Ordering::SeqCst);
}

/// The blocking executor.
pub struct BlockingExecutor {
    /// The queue of blocking tasks.
    sender: flume::Sender<BlockingTask>,
    receiver: flume::Receiver<BlockingTask>,

    /// Current number of threads in the pool.
    thread_count: AtomicUsize,

    /// Number of idle threads in the pool.
    idle_count: AtomicUsize,

    /// Maximum number of threads in the pool.
    thread_limit: AtomicUsize,
}

impl BlockingExecutor {
    #[cfg(not(target_family = "wasm"))]
    fn max_threads() -> NonZeroUsize {
        match env::var(MAX_THREADS_ENV) {
            Ok(v) => v
                .parse::<usize>()
                .ok()
                .and_then(|v| NonZeroUsize::new(v.clamp(MIN_MAX_THREADS, MAX_MAX_THREADS)))
                .unwrap_or(DEFAULT_MAX_THREADS),
            Err(_) => DEFAULT_MAX_THREADS,
        }
    }

    #[cfg(target_family = "wasm")]
    fn max_threads() -> NonZeroUsize {
        NonZeroUsize::new(1).unwrap()
    }

    /// Get a reference to the global executor.
    #[inline]
    fn get() -> &'static Self {
        #[cfg(not(target_family = "wasm"))]
        {
            static EXECUTOR: Lazy<BlockingExecutor> = Lazy::new(|| {
                let (sender, receiver) = flume::unbounded();
                BlockingExecutor {
                    sender,
                    receiver,
                    thread_count: AtomicUsize::new(0),
                    idle_count: AtomicUsize::new(0),
                    thread_limit: AtomicUsize::new(BlockingExecutor::max_threads().get()),
                }
            });

            &EXECUTOR
        }

        #[cfg(target_family = "wasm")]
        panic!("cannot spawn a blocking task on WASM")
    }

    /// Spawns a future onto this executor.
    ///
    /// Returns a [`Task`] handle for the spawned task.
    fn spawn<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Task<T> {
        let executor = Self::get();

        let (tx, rx) = multishot::channel();

        let task = BlockingTask::Execute(Box::new(move || {
            let res = f();
            let _ = tx.send(res);
        }));

        executor.schedule(task);

        // Return handle (typed)
        Task::new(rx, None)
    }

    /// Runs the main loop on the current thread.
    ///
    /// This function runs blocking tasks until it becomes idle and times out.
    fn main_loop(&'static self) {
        #[cfg(feature = "tracing")]
        let _span = tracing::trace_span!("blocking::main_loop").entered();

        // Ensure signal handler is installed for this process (once)
        #[cfg(unix)]
        install_signal_handler();

        loop {
            // Wait for the next task.
            self.idle_count.fetch_add(1, Ordering::SeqCst);
            let res = self.receiver.recv_timeout(Duration::from_secs(5));
            self.idle_count.fetch_sub(1, Ordering::SeqCst);

            match res {
                Ok(task) => {
                    // Run the task.
                    match task {
                        BlockingTask::Execute(f) => {
                            panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok();
                        }
                        BlockingTask::Open(path, tx) => {
                            if tx.enter_executing(0) {
                                let _ = tx.send(fs::File::open(path));
                            }
                        }
                        BlockingTask::Create(path, tx) => {
                            if tx.enter_executing(0) {
                                let _ = tx.send(fs::File::create(path));
                            }
                        }
                        BlockingTask::RemoveFile(path, tx) => {
                            if tx.enter_executing(0) {
                                let _ = tx.send(fs::remove_file(path));
                            }
                        }
                        BlockingTask::RemoveDir(path, tx) => {
                            if tx.enter_executing(0) {
                                let _ = tx.send(fs::remove_dir(path));
                            }
                        }
                        BlockingTask::CreateDir(path, tx) => {
                            if tx.enter_executing(0) {
                                let _ = tx.send(fs::create_dir(path));
                            }
                        }
                        BlockingTask::Rename(from, to, tx) => {
                            if tx.enter_executing(0) {
                                let _ = tx.send(fs::rename(from, to));
                            }
                        }
                        BlockingTask::Metadata(path, tx) => {
                            if tx.enter_executing(0) {
                                let _ = tx.send(fs::metadata(path));
                            }
                        }
                        // Zero-allocation handlers for file descriptor operations
                        #[cfg(unix)]
                        BlockingTask::FMetadata(fd, tx) => {
                            if tx.enter_executing(0) {
                                // Zero allocation - uses stack-allocated file
                                let file = unsafe { std::fs::File::from_raw_fd(fd) };
                                let result = file.metadata();
                                // Prevent closing the file descriptor
                                let _ = file.into_raw_fd();
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(unix)]
                        BlockingTask::FRead(fd, ptr, len, tx) => {
                            if tx.enter_executing(unsafe { pthread_self() } as usize) {
                                let result = unsafe { sys::read(fd, ptr, len, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(unix)]
                        BlockingTask::FReadAt(fd, ptr, len, offset, tx) => {
                            if tx.enter_executing(unsafe { pthread_self() } as usize) {
                                let result = unsafe { sys::pread(fd, ptr, len, offset, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(unix)]
                        BlockingTask::FWrite(fd, ptr, len, tx) => {
                            if tx.enter_executing(unsafe { pthread_self() } as usize) {
                                let result = unsafe { sys::write(fd, ptr, len, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(unix)]
                        BlockingTask::FWriteAt(fd, ptr, len, offset, tx) => {
                            if tx.enter_executing(unsafe { pthread_self() } as usize) {
                                let result = unsafe { sys::pwrite(fd, ptr, len, offset, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(unix)]
                        BlockingTask::FSync(fd, data_only, tx) => {
                            if tx.enter_executing(unsafe { pthread_self() } as usize) {
                                let result = unsafe { sys::fsync(fd, data_only, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                        // Zero-allocation handlers for file descriptor operations - Windows
                        #[cfg(windows)]
                        BlockingTask::FMetadata(handle, tx) => {
                            if tx.enter_executing(0) {
                                // Zero allocation - uses stack-allocated file
                                let file = unsafe { std::fs::File::from_raw_handle(handle) };
                                let result = file.metadata();
                                // Prevent closing the file handle
                                let _ = file.into_raw_handle();
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(windows)]
                        BlockingTask::FRead(handle, ptr, len, tx) => {
                            if tx.enter_executing(handle as usize) {
                                let result = unsafe { sys::read(handle, ptr, len, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(windows)]
                        BlockingTask::FReadAt(handle, ptr, len, offset, tx) => {
                            if tx.enter_executing(handle as usize) {
                                let result = unsafe { sys::pread(handle, ptr, len, offset, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(windows)]
                        BlockingTask::FWrite(handle, ptr, len, tx) => {
                            if tx.enter_executing(handle as usize) {
                                let result = unsafe { sys::write(handle, ptr, len, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(windows)]
                        BlockingTask::FWriteAt(handle, ptr, len, offset, tx) => {
                            if tx.enter_executing(handle as usize) {
                                let result = unsafe { sys::pwrite(handle, ptr, len, offset, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                        #[cfg(windows)]
                        BlockingTask::FSync(handle, data_only, tx) => {
                            if tx.enter_executing(handle as usize) {
                                let result = unsafe { sys::fsync(handle, data_only, &tx) };
                                let _ = tx.send(result);
                            }
                        }
                    }
                }
                Err(_) => {
                    // Timeout. Check if we should stop this thread.
                    // We always keep at least one thread alive for responsiveness.
                    let current_threads = self.thread_count.load(Ordering::SeqCst);
                    if current_threads > 1 {
                        // Try to decrement.
                        if self
                            .thread_count
                            .compare_exchange(
                                current_threads,
                                current_threads - 1,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            )
                            .is_ok()
                        {
                            break;
                        }
                    }
                }
            }

            #[cfg(feature = "tracing")]
            tracing::trace!("shutting down due to lack of tasks");
        }
    }

    /// Schedules a runnable task for execution.
    fn schedule(&'static self, task: BlockingTask) {
        // Push the task.
        if let Err(_) = self.sender.send(task) {
            return;
        }

        // If there are no idle threads, we should spawn one if we can.
        if self.idle_count.load(Ordering::SeqCst) == 0 {
            let limit = self.thread_limit.load(Ordering::SeqCst);

            // Use a loop to handle race conditions on thread_count
            loop {
                let current = self.thread_count.load(Ordering::SeqCst);
                if current >= limit {
                    break;
                }

                if self
                    .thread_count
                    .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    static ID: AtomicUsize = AtomicUsize::new(1);
                    let id = ID.fetch_add(1, Ordering::Relaxed);

                    if let Err(e) = thread::Builder::new()
                        .name(format!("blocking-{id}"))
                        .spawn(move || self.main_loop())
                    {
                        #[cfg(feature = "tracing")]
                        tracing::error!("failed to spawn a blocking thread: {}", e);
                        self.thread_count.fetch_sub(1, Ordering::SeqCst);
                    }
                    break;
                }
            }
        }
    }

    /// Spawns a future onto this executor.
    ///
    /// Returns a [`Task`] handle for the spawned task.
    fn spawn_internal<T: Send + 'static>(
        &'static self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Task<T> {
        let executor = Self::get();

        let (tx, rx) = multishot::channel();

        let task = BlockingTask::Execute(Box::new(move || {
            let res = f();
            let _ = tx.send(res);
        }));

        self.schedule(task);

        // Return handle (typed)
        Task::new(rx, None)
    }

    /// Runs blocking code on a thread pool.
    ///
    /// # Examples
    ///
    /// Read the contents of a file:
    ///
    /// ```no_run
    /// use maniac::unblock;
    /// use std::fs;
    ///
    /// # maniac::future::block_on(async {
    /// let contents = unblock(|| fs::read_to_string("file.txt")).await?;
    /// # std::io::Result::Ok(()) });
    /// ```
    ///
    /// Spawn a process:
    ///
    /// ```no_run
    /// use maniac::unblock;
    /// use std::process::Command;
    ///
    /// # maniac::future::block_on(async {
    /// let out = unblock(|| Command::new("dir").output()).await?;
    /// # std::io::Result::Ok(()) });
    /// ```
    pub fn unblock<T, F>(&'static self, f: F) -> Task<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = multishot::channel();

        let task = BlockingTask::Execute(Box::new(move || {
            let res = f();
            let _ = tx.send(res);
        }));

        self.schedule(task);

        // Return handle (typed)
        Task::new(rx, None)
    }

    pub fn unblock_open(&'static self, path: impl Into<PathBuf>) -> Task<io::Result<File>> {
        let (tx, rx) = get_file_chan();
        self.schedule(BlockingTask::Open(path.into(), tx));
        Task::new(rx, Some(recycle_file))
    }

    pub fn unblock_create(&'static self, path: impl Into<PathBuf>) -> Task<io::Result<File>> {
        let (tx, rx) = get_file_chan();
        self.schedule(BlockingTask::Create(path.into(), tx));
        Task::new(rx, Some(recycle_file))
    }

    pub fn unblock_remove_file(&'static self, path: impl Into<PathBuf>) -> Task<io::Result<()>> {
        let (tx, rx) = get_void_chan();
        self.schedule(BlockingTask::RemoveFile(path.into(), tx));
        Task::new(rx, Some(recycle_void))
    }

    pub fn unblock_remove_dir(&'static self, path: impl Into<PathBuf>) -> Task<io::Result<()>> {
        let (tx, rx) = get_void_chan();
        self.schedule(BlockingTask::RemoveDir(path.into(), tx));
        Task::new(rx, Some(recycle_void))
    }

    pub fn unblock_create_dir(&'static self, path: impl Into<PathBuf>) -> Task<io::Result<()>> {
        let (tx, rx) = get_void_chan();
        self.schedule(BlockingTask::CreateDir(path.into(), tx));
        Task::new(rx, Some(recycle_void))
    }

    pub fn unblock_rename(
        &'static self,
        from: impl Into<PathBuf>,
        to: impl Into<PathBuf>,
    ) -> Task<io::Result<()>> {
        let (tx, rx) = get_void_chan();
        self.schedule(BlockingTask::Rename(from.into(), to.into(), tx));
        Task::new(rx, Some(recycle_void))
    }

    pub fn unblock_metadata(&'static self, path: impl Into<PathBuf>) -> Task<io::Result<Metadata>> {
        let (tx, rx) = get_metadata_chan();
        self.schedule(BlockingTask::Metadata(path.into(), tx));
        Task::new(rx, Some(recycle_metadata))
    }

    // Zero-allocation file descriptor operations
    //
    // Safety: All these functions are unsafe because they work with raw pointers
    // and raw file descriptors/handles. Callers must ensure:
    // 1. The file descriptor/handle remains valid for the duration of the operation
    // 2. Pointers are valid and aligned for the entire duration
    // 3. No other references to the same memory exist
    // 4. The memory pointed to has sufficient capacity

    // Unix variants
    #[cfg(unix)]
    pub unsafe fn unblock_fmetadata(&'static self, fd: RawFd) -> Task<io::Result<Metadata>> {
        let (tx, rx) = get_metadata_chan();
        self.schedule(BlockingTask::FMetadata(fd, tx));
        Task::new(rx, Some(recycle_metadata))
    }

    #[cfg(unix)]
    pub unsafe fn unblock_fread(
        &'static self,
        fd: RawFd,
        ptr: *mut u8,
        len: usize,
    ) -> Task<io::Result<usize>> {
        let (tx, rx) = get_io_size_chan();
        self.schedule(BlockingTask::FRead(fd, Ptr(ptr), len, tx));
        Task::new_wait(rx, Some(recycle_io_size))
    }

    #[cfg(unix)]
    pub unsafe fn unblock_fread_at(
        &'static self,
        fd: RawFd,
        ptr: *mut u8,
        len: usize,
        offset: u64,
    ) -> Task<io::Result<usize>> {
        let (tx, rx) = get_io_size_chan();
        self.schedule(BlockingTask::FReadAt(fd, Ptr(ptr), len, offset, tx));
        Task::new_wait(rx, Some(recycle_io_size))
    }

    #[cfg(unix)]
    pub unsafe fn unblock_fwrite(
        &'static self,
        fd: RawFd,
        ptr: *const u8,
        len: usize,
    ) -> Task<io::Result<usize>> {
        let (tx, rx) = get_io_size_chan();
        self.schedule(BlockingTask::FWrite(fd, Ptr(ptr as *mut u8), len, tx));
        Task::new_wait(rx, Some(recycle_io_size))
    }

    #[cfg(unix)]
    pub unsafe fn unblock_fwrite_at(
        &'static self,
        fd: RawFd,
        ptr: *const u8,
        len: usize,
        offset: u64,
    ) -> Task<io::Result<usize>> {
        let (tx, rx) = get_io_size_chan();
        self.schedule(BlockingTask::FWriteAt(
            fd,
            Ptr(ptr as *mut u8),
            len,
            offset,
            tx,
        ));
        Task::new_wait(rx, Some(recycle_io_size))
    }

    #[cfg(unix)]
    pub unsafe fn unblock_fsync(&'static self, fd: RawFd, data_only: bool) -> Task<io::Result<()>> {
        let (tx, rx) = get_void_chan();
        self.schedule(BlockingTask::FSync(fd, data_only, tx));
        Task::new(rx, Some(recycle_void))
    }

    // Windows variants
    #[cfg(windows)]
    pub unsafe fn unblock_fmetadata(
        &'static self,
        handle: RawHandle,
    ) -> Task<io::Result<Metadata>> {
        let (tx, rx) = get_metadata_chan();
        self.schedule(BlockingTask::FMetadata(handle, tx));
        Task::new(rx, Some(recycle_metadata))
    }

    #[cfg(windows)]
    pub unsafe fn unblock_fread(
        &'static self,
        handle: RawHandle,
        ptr: *mut u8,
        len: usize,
    ) -> Task<io::Result<usize>> {
        let (tx, rx) = get_io_size_chan();
        self.schedule(BlockingTask::FRead(handle, Ptr(ptr), len, tx));
        Task::new_wait(rx, Some(recycle_io_size))
    }

    #[cfg(windows)]
    pub unsafe fn unblock_fread_at(
        &'static self,
        handle: RawHandle,
        ptr: *mut u8,
        len: usize,
        offset: u64,
    ) -> Task<io::Result<usize>> {
        let (tx, rx) = get_io_size_chan();
        self.schedule(BlockingTask::FReadAt(handle, Ptr(ptr), len, offset, tx));
        Task::new_wait(rx, Some(recycle_io_size))
    }

    #[cfg(windows)]
    pub unsafe fn unblock_fwrite(
        &'static self,
        handle: RawHandle,
        ptr: *const u8,
        len: usize,
    ) -> Task<io::Result<usize>> {
        let (tx, rx) = get_io_size_chan();
        self.schedule(BlockingTask::FWrite(handle, Ptr(ptr as *mut u8), len, tx));
        Task::new_wait(rx, Some(recycle_io_size))
    }

    #[cfg(windows)]
    pub unsafe fn unblock_fwrite_at(
        &'static self,
        handle: RawHandle,
        ptr: *const u8,
        len: usize,
        offset: u64,
    ) -> Task<io::Result<usize>> {
        let (tx, rx) = get_io_size_chan();
        self.schedule(BlockingTask::FWriteAt(
            handle,
            Ptr(ptr as *mut u8),
            len,
            offset,
            tx,
        ));
        Task::new_wait(rx, Some(recycle_io_size))
    }

    #[cfg(windows)]
    pub unsafe fn unblock_fsync(
        &'static self,
        handle: RawHandle,
        data_only: bool,
    ) -> Task<io::Result<()>> {
        let (tx, rx) = get_void_chan();
        self.schedule(BlockingTask::FSync(handle, data_only, tx));
        Task::new(rx, Some(recycle_void))
    }
}

/// Runs blocking code on a thread pool.
///
/// # Examples
///
/// Read the contents of a file:
///
/// ```no_run
/// use blocking::unblock;
/// use std::fs;
///
/// # futures_lite::future::block_on(async {
/// let contents = unblock(|| fs::read_to_string("file.txt")).await?;
/// # std::io::Result::Ok(()) });
/// ```
///
/// Spawn a process:
///
/// ```no_run
/// use blocking::unblock;
/// use std::process::Command;
///
/// # futures_lite::future::block_on(async {
/// let out = unblock(|| Command::new("dir").output()).await?;
/// # std::io::Result::Ok(()) });
/// ```
pub fn unblock<T, F>(f: F) -> Task<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    BlockingExecutor::get().unblock(f)
}

pub fn unblock_open(path: impl Into<PathBuf>) -> Task<io::Result<File>> {
    unsafe { BlockingExecutor::get().unblock_open(path) }
}

pub fn unblock_create(path: impl Into<PathBuf>) -> Task<io::Result<File>> {
    unsafe { BlockingExecutor::get().unblock_create(path) }
}

pub fn unblock_remove_file(path: impl Into<PathBuf>) -> Task<io::Result<()>> {
    unsafe { BlockingExecutor::get().unblock_remove_file(path) }
}

pub fn unblock_remove_dir(path: impl Into<PathBuf>) -> Task<io::Result<()>> {
    unsafe { BlockingExecutor::get().unblock_remove_dir(path) }
}

pub fn unblock_create_dir(path: impl Into<PathBuf>) -> Task<io::Result<()>> {
    unsafe { BlockingExecutor::get().unblock_create_dir(path) }
}

pub fn unblock_rename(from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> Task<io::Result<()>> {
    unsafe { BlockingExecutor::get().unblock_rename(from, to) }
}

pub fn unblock_metadata(path: impl Into<PathBuf>) -> Task<io::Result<Metadata>> {
    unsafe { BlockingExecutor::get().unblock_metadata(path) }
}

// Zero-allocation file descriptor operations
//
// Safety: All these functions are unsafe because they work with raw pointers
// and raw file descriptors/handles. Callers must ensure:
// 1. The file descriptor/handle remains valid for the duration of the operation
// 2. Pointers are valid and aligned for the entire duration
// 3. No other references to the same memory exist
// 4. The memory pointed to has sufficient capacity

// Unix variants
#[cfg(unix)]
pub unsafe fn unblock_fmetadata(fd: RawFd) -> Task<io::Result<Metadata>> {
    unsafe { BlockingExecutor::get().unblock_fmetadata(fd) }
}

#[cfg(unix)]
pub unsafe fn unblock_fread(fd: RawFd, ptr: *mut u8, len: usize) -> Task<io::Result<usize>> {
    unsafe { BlockingExecutor::get().unblock_fread(fd, ptr, len) }
}

#[cfg(unix)]
pub unsafe fn unblock_fread_at(
    fd: RawFd,
    ptr: *mut u8,
    len: usize,
    offset: u64,
) -> Task<io::Result<usize>> {
    unsafe { BlockingExecutor::get().unblock_fread_at(fd, ptr, len, offset) }
}

#[cfg(unix)]
pub unsafe fn unblock_fwrite(fd: RawFd, ptr: *const u8, len: usize) -> Task<io::Result<usize>> {
    unsafe { BlockingExecutor::get().unblock_fwrite(fd, ptr, len) }
}

#[cfg(unix)]
pub unsafe fn unblock_fwrite_at(
    fd: RawFd,
    ptr: *const u8,
    len: usize,
    offset: u64,
) -> Task<io::Result<usize>> {
    unsafe { BlockingExecutor::get().unblock_fwrite_at(fd, ptr, len, offset) }
}

#[cfg(unix)]
pub unsafe fn unblock_fsync(fd: RawFd, data_only: bool) -> Task<io::Result<()>> {
    unsafe { BlockingExecutor::get().unblock_fsync(fd, data_only) }
}

// Windows variants
#[cfg(windows)]
pub unsafe fn unblock_fmetadata(handle: RawHandle) -> Task<io::Result<Metadata>> {
    unsafe { BlockingExecutor::get().unblock_fmetadata(handle) }
}

#[cfg(windows)]
pub unsafe fn unblock_fread(
    handle: RawHandle,
    ptr: *mut u8,
    len: usize,
) -> Task<io::Result<usize>> {
    unsafe { BlockingExecutor::get().unblock_fread(handle, ptr, len) }
}

#[cfg(windows)]
pub unsafe fn unblock_fread_at(
    handle: RawHandle,
    ptr: *mut u8,
    len: usize,
    offset: u64,
) -> Task<io::Result<usize>> {
    unsafe { BlockingExecutor::get().unblock_fread_at(handle, ptr, len, offset) }
}

#[cfg(windows)]
pub unsafe fn unblock_fwrite(
    handle: RawHandle,
    ptr: *const u8,
    len: usize,
) -> Task<io::Result<usize>> {
    unsafe { BlockingExecutor::get().unblock_fwrite(handle, ptr, len) }
}

#[cfg(windows)]
pub unsafe fn unblock_fwrite_at(
    handle: RawHandle,
    ptr: *const u8,
    len: usize,
    offset: u64,
) -> Task<io::Result<usize>> {
    unsafe { BlockingExecutor::get().unblock_fwrite_at(handle, ptr, len, offset) }
}

#[cfg(windows)]
pub unsafe fn unblock_fsync(handle: RawHandle, data_only: bool) -> Task<io::Result<()>> {
    unsafe { BlockingExecutor::get().unblock_fsync(handle, data_only) }
}

// System call wrappers for direct I/O and EINTR handling
mod sys {
    use super::*;
    use crate::monoio::blocking::multishot::Sender;
    use std::io;

    #[cfg(unix)]
    pub unsafe fn read(
        fd: RawFd,
        ptr: Ptr<u8>,
        len: usize,
        tx: &Sender<io::Result<usize>>,
    ) -> io::Result<usize> {
        loop {
            let res = unsafe { libc::read(fd, ptr.0 as *mut _, len) };
            if res == -1 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    if tx.is_closed() {
                        return Err(io::Error::new(io::ErrorKind::Other, "operation cancelled"));
                    }
                    continue;
                }
                return Err(err);
            }
            return Ok(res as usize);
        }
    }

    #[cfg(unix)]
    pub unsafe fn write(
        fd: RawFd,
        ptr: Ptr<u8>,
        len: usize,
        tx: &Sender<io::Result<usize>>,
    ) -> io::Result<usize> {
        loop {
            let res = unsafe { libc::write(fd, ptr.0 as *const _, len) };
            if res == -1 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    if tx.is_closed() {
                        return Err(io::Error::new(io::ErrorKind::Other, "operation cancelled"));
                    }
                    continue;
                }
                return Err(err);
            }
            return Ok(res as usize);
        }
    }

    #[cfg(unix)]
    pub unsafe fn pread(
        fd: RawFd,
        ptr: Ptr<u8>,
        len: usize,
        offset: u64,
        tx: &Sender<io::Result<usize>>,
    ) -> io::Result<usize> {
        loop {
            let res = unsafe { libc::pread(fd, ptr.0 as *mut _, len, offset as i64) };
            if res == -1 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    if tx.is_closed() {
                        return Err(io::Error::new(io::ErrorKind::Other, "operation cancelled"));
                    }
                    continue;
                }
                return Err(err);
            }
            return Ok(res as usize);
        }
    }

    #[cfg(unix)]
    pub unsafe fn pwrite(
        fd: RawFd,
        ptr: Ptr<u8>,
        len: usize,
        offset: u64,
        tx: &Sender<io::Result<usize>>,
    ) -> io::Result<usize> {
        loop {
            let res = unsafe { libc::pwrite(fd, ptr.0 as *const _, len, offset as i64) };
            if res == -1 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    if tx.is_closed() {
                        return Err(io::Error::new(io::ErrorKind::Other, "operation cancelled"));
                    }
                    continue;
                }
                return Err(err);
            }
            return Ok(res as usize);
        }
    }

    #[cfg(unix)]
    pub unsafe fn fsync(
        fd: RawFd,
        data_only: bool,
        _tx: &Sender<io::Result<()>>,
    ) -> io::Result<()> {
        loop {
            let res = unsafe {
                if data_only {
                    libc::fdatasync(fd)
                } else {
                    libc::fsync(fd)
                }
            };
            if res == -1 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    if _tx.is_closed() {
                        return Err(io::Error::new(io::ErrorKind::Other, "operation cancelled"));
                    }
                    continue;
                }
                return Err(err);
            }
            return Ok(());
        }
    }

    #[cfg(windows)]
    pub unsafe fn read(
        handle: RawHandle,
        ptr: Ptr<u8>,
        len: usize,
        _tx: &Sender<io::Result<usize>>,
    ) -> io::Result<usize> {
        use std::ptr;
        use windows_sys::Win32::Foundation::{ERROR_HANDLE_EOF, GetLastError, HANDLE};
        use windows_sys::Win32::Storage::FileSystem::ReadFile;

        let mut bytes_read = 0;
        let res = unsafe {
            ReadFile(
                handle as HANDLE,
                ptr.0 as *mut _,
                len as u32,
                &mut bytes_read,
                ptr::null_mut(),
            )
        };
        if res == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_HANDLE_EOF {
                return Ok(0);
            }
            return Err(io::Error::from_raw_os_error(err as i32));
        }
        Ok(bytes_read as usize)
    }

    #[cfg(windows)]
    pub unsafe fn write(
        handle: RawHandle,
        ptr: Ptr<u8>,
        len: usize,
        _tx: &Sender<io::Result<usize>>,
    ) -> io::Result<usize> {
        use std::ptr;
        use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
        use windows_sys::Win32::Storage::FileSystem::WriteFile;

        let mut bytes_written = 0;
        let res = unsafe {
            WriteFile(
                handle as HANDLE,
                ptr.0 as *const _,
                len as u32,
                &mut bytes_written,
                ptr::null_mut(),
            )
        };
        if res == 0 {
            let err = unsafe { GetLastError() };
            return Err(io::Error::from_raw_os_error(err as i32));
        }
        Ok(bytes_written as usize)
    }

    #[cfg(windows)]
    pub unsafe fn pread(
        handle: RawHandle,
        ptr: Ptr<u8>,
        len: usize,
        offset: u64,
        _tx: &Sender<io::Result<usize>>,
    ) -> io::Result<usize> {
        use windows_sys::Win32::Foundation::{ERROR_HANDLE_EOF, GetLastError, HANDLE};
        use windows_sys::Win32::Storage::FileSystem::ReadFile;
        use windows_sys::Win32::System::IO::OVERLAPPED;

        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.Anonymous.Anonymous.Offset = offset as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (offset >> 32) as u32;

        let mut bytes_read = 0;
        let res = unsafe {
            ReadFile(
                handle as HANDLE,
                ptr.0 as *mut _,
                len as u32,
                &mut bytes_read,
                &mut overlapped,
            )
        };
        if res == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_HANDLE_EOF {
                return Ok(0);
            }
            return Err(io::Error::from_raw_os_error(err as i32));
        }
        Ok(bytes_read as usize)
    }

    #[cfg(windows)]
    pub unsafe fn pwrite(
        handle: RawHandle,
        ptr: Ptr<u8>,
        len: usize,
        offset: u64,
        _tx: &Sender<io::Result<usize>>,
    ) -> io::Result<usize> {
        use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
        use windows_sys::Win32::Storage::FileSystem::WriteFile;
        use windows_sys::Win32::System::IO::OVERLAPPED;

        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.Anonymous.Anonymous.Offset = offset as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (offset >> 32) as u32;

        let mut bytes_written = 0;
        let res = unsafe {
            WriteFile(
                handle as HANDLE,
                ptr.0 as *const _,
                len as u32,
                &mut bytes_written,
                &mut overlapped,
            )
        };
        if res == 0 {
            let err = unsafe { GetLastError() };
            return Err(io::Error::from_raw_os_error(err as i32));
        }
        Ok(bytes_written as usize)
    }

    #[cfg(windows)]
    pub unsafe fn fsync(
        handle: RawHandle,
        _data_only: bool,
        _tx: &Sender<io::Result<()>>,
    ) -> io::Result<()> {
        use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
        use windows_sys::Win32::Storage::FileSystem::FlushFileBuffers;

        let res = unsafe { FlushFileBuffers(handle as HANDLE) };
        if res == 0 {
            let err = unsafe { GetLastError() };
            return Err(io::Error::from_raw_os_error(err as i32));
        }
        Ok(())
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use std::env;

    #[test]
    #[ignore]
    fn test_max_threads() {
        // properly set env var
        unsafe { env::set_var(MAX_THREADS_ENV, "100") };
        assert_eq!(100, BlockingExecutor::max_threads().get());

        // passed value below minimum, so we set it to minimum
        unsafe { env::set_var(MAX_THREADS_ENV, "0") };
        assert_eq!(1, BlockingExecutor::max_threads().get());

        // passed value above maximum, so we set to allowed maximum
        unsafe { env::set_var(MAX_THREADS_ENV, "50000") };
        assert_eq!(10000, BlockingExecutor::max_threads().get());

        // no env var, use default
        unsafe { env::set_var(MAX_THREADS_ENV, "") };
        assert_eq!(500, BlockingExecutor::max_threads().get());

        // not a number, use default
        unsafe { env::set_var(MAX_THREADS_ENV, "NOTINT") };
        assert_eq!(500, BlockingExecutor::max_threads().get());
    }
}
