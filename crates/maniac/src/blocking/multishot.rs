// Inline multishot module with cancellation support

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
use libc::{pthread_kill, pthread_t, SIGUSR1};

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
            self.waker[idx].with_mut(|waker| *waker = new);
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
                        Ok(_) => {}  // Signaled cancellation
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
