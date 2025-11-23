//! A thread pool for isolating blocking I/O in async programs.
//!
//! Sometimes there's no way to avoid blocking I/O. Consider files or stdin, which have weak async
//! support on modern operating systems. While [IOCP], [AIO], and [io_uring] are possible
//! solutions, they're not always available or ideal.
//!
//! Since blocking is not allowed inside futures, we must move blocking I/O onto a special thread
//! pool provided by this crate. The pool dynamically spawns and stops threads depending on the
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
use std::os::unix::io::{FromRawFd, IntoRawFd};

// Platform-specific imports
#[cfg(unix)]
use std::os::unix::prelude::RawFd;
#[cfg(windows)]
use std::os::windows::io::{FromRawHandle, IntoRawHandle, RawHandle};

use flume;
use std::cell::Cell;
use crate::sync::multishot::{self, Receiver, Sender};
use once_cell::sync::Lazy;

/// A handle to a blocking task.
pub struct Task<T> {
    receiver: Option<Receiver<T>>,
    recycler: Option<fn(Receiver<T>)>,
}

impl<T> Task<T> {
    fn new(receiver: Receiver<T>, recycler: Option<fn(Receiver<T>)>) -> Self {
        Self {
            receiver: Some(receiver),
            recycler,
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
        let recv = this.receiver.as_mut().expect("Task polled after completion").recv();
        
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
        if let Some(rx) = self.receiver.take() {
            if let Some(recycler) = self.recycler {
                recycler(rx);
            }
        }
    }
}

// Pools for reusing channels to avoid allocation
macro_rules! define_pool {
    ($name:ident, $type:ty, $recycle_fn:ident, $get_fn:ident) => {
        thread_local! {
            static $name: Cell<Vec<Receiver<$type>>> = Cell::new(Vec::new());
        }

        fn $recycle_fn(rx: Receiver<$type>) {
            $name.with(|pool| {
                let mut vec = pool.take();
                vec.push(rx);
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
define_pool!(METADATA_POOL, io::Result<Metadata>, recycle_metadata, get_metadata_chan);
define_pool!(VOID_POOL, io::Result<()>, recycle_void, get_void_chan);
define_pool!(IO_SIZE_POOL, io::Result<usize>, recycle_io_size, get_io_size_chan);


// Helper type to make raw pointers Send
pub struct Ptr<T>(*mut T);

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
    FReadAt(
        RawFd,
        Ptr<u8>,
        usize,
        u64,
        Sender<io::Result<usize>>,
    ),
    #[cfg(unix)]
    FWrite(RawFd, Ptr<u8>, usize, Sender<io::Result<usize>>),
    #[cfg(unix)]
    FWriteAt(
        RawFd,
        Ptr<u8>,
        usize,
        u64,
        Sender<io::Result<usize>>,
    ),
    #[cfg(unix)]
    FSync(RawFd, bool, Sender<io::Result<()>>),
    // Zero-allocation file descriptor operations - Windows
    #[cfg(windows)]
    FMetadata(RawHandle, Sender<io::Result<Metadata>>),
    #[cfg(windows)]
    FRead(
        RawHandle,
        Ptr<u8>,
        usize,
        Sender<io::Result<usize>>,
    ),
    #[cfg(windows)]
    FReadAt(
        RawHandle,
        Ptr<u8>,
        usize,
        u64,
        Sender<io::Result<usize>>,
    ),
    #[cfg(windows)]
    FWrite(
        RawHandle,
        Ptr<u8>,
        usize,
        Sender<io::Result<usize>>,
    ),
    #[cfg(windows)]
    FWriteAt(
        RawHandle,
        Ptr<u8>,
        usize,
        u64,
        Sender<io::Result<usize>>,
    ),
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
    let executor = Executor::get();
    executor.thread_limit.store(threads.get(), Ordering::SeqCst);
}

/// The blocking executor.
struct Executor {
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

impl Executor {
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
            static EXECUTOR: Lazy<Executor> = Lazy::new(|| {
                let (sender, receiver) = flume::unbounded();
                Executor {
                    sender,
                    receiver,
                    thread_count: AtomicUsize::new(0),
                    idle_count: AtomicUsize::new(0),
                    thread_limit: AtomicUsize::new(Executor::max_threads().get()),
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
                            let _ = tx.send(fs::File::open(path));
                        }
                        BlockingTask::Create(path, tx) => {
                            let _ = tx.send(fs::File::create(path));
                        }
                        BlockingTask::RemoveFile(path, tx) => {
                            let _ = tx.send(fs::remove_file(path));
                        }
                        BlockingTask::RemoveDir(path, tx) => {
                            let _ = tx.send(fs::remove_dir(path));
                        }
                        BlockingTask::CreateDir(path, tx) => {
                            let _ = tx.send(fs::create_dir(path));
                        }
                        BlockingTask::Rename(from, to, tx) => {
                            let _ = tx.send(fs::rename(from, to));
                        }
                        BlockingTask::Metadata(path, tx) => {
                            let _ = tx.send(fs::metadata(path));
                        }
                        // Zero-allocation handlers for file descriptor operations
                        #[cfg(unix)]
                        BlockingTask::FMetadata(fd, tx) => {
                            // Zero allocation - uses stack-allocated file
                            let file = unsafe { std::fs::File::from_raw_fd(fd) };
                            let result = file.metadata();
                            // Prevent closing the file descriptor
                            let _ = file.into_raw_fd();
                            let _ = tx.send(result);
                        }
                        #[cfg(unix)]
                        BlockingTask::FRead(fd, ptr, len, tx) => {
                            // Zero allocation - constructs slice on stack
                            let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
                            let slice = unsafe { std::slice::from_raw_parts_mut(ptr.0, len) };
                            use std::io::Read;
                            let result = file.read(slice);
                            // Prevent closing the file descriptor
                            let _ = file.into_raw_fd();
                            let _ = tx.send(result);
                        }
                        #[cfg(unix)]
                        BlockingTask::FReadAt(fd, ptr, len, offset, tx) => {
                            let file = unsafe { std::fs::File::from_raw_fd(fd) };
                            let slice = unsafe { std::slice::from_raw_parts_mut(ptr.0, len) };
                            use std::os::unix::fs::FileExt;
                            let result = file.read_at(slice, offset);
                            // Prevent closing the file descriptor
                            let _ = file.into_raw_fd();
                            let _ = tx.send(result);
                        }
                        #[cfg(unix)]
                        BlockingTask::FWrite(fd, ptr, len, tx) => {
                            let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
                            let slice = unsafe { std::slice::from_raw_parts(ptr.0, len) };
                            use std::io::Write;
                            let result = file.write(slice);
                            // Prevent closing the file descriptor
                            let _ = file.into_raw_fd();
                            let _ = tx.send(result);
                        }
                        #[cfg(unix)]
                        BlockingTask::FWriteAt(fd, ptr, len, offset, tx) => {
                            let file = unsafe { std::fs::File::from_raw_fd(fd) };
                            let slice = unsafe { std::slice::from_raw_parts(ptr.0, len) };
                            use std::os::unix::fs::FileExt;
                            let result = file.write_at(slice, offset);
                            // Prevent closing the file descriptor
                            let _ = file.into_raw_fd();
                            let _ = tx.send(result);
                        }
                        #[cfg(unix)]
                        BlockingTask::FSync(fd, data_only, tx) => {
                            let file = unsafe { std::fs::File::from_raw_fd(fd) };
                            let result = if data_only {
                                file.sync_data()
                            } else {
                                file.sync_all()
                            };
                            // Prevent closing the file descriptor
                            let _ = file.into_raw_fd();
                            let _ = tx.send(result);
                        }
                        // Zero-allocation handlers for file descriptor operations - Windows
                        #[cfg(windows)]
                        BlockingTask::FMetadata(handle, tx) => {
                            // Zero allocation - uses stack-allocated file
                            let file = unsafe { std::fs::File::from_raw_handle(handle) };
                            let result = file.metadata();
                            // Prevent closing the file handle
                            let _ = file.into_raw_handle();
                            let _ = tx.send(result);
                        }
                        #[cfg(windows)]
                        BlockingTask::FRead(handle, ptr, len, tx) => {
                            // Zero allocation - constructs slice on stack
                            let mut file = unsafe { std::fs::File::from_raw_handle(handle) };
                            let slice = unsafe { std::slice::from_raw_parts_mut(ptr.0, len) };
                            use std::io::Read;
                            let result = file.read(slice);
                            // Prevent closing the file handle
                            let _ = file.into_raw_handle();
                            let _ = tx.send(result);
                        }
                        #[cfg(windows)]
                        BlockingTask::FReadAt(handle, ptr, len, offset, tx) => {
                            let file = unsafe { std::fs::File::from_raw_handle(handle) };
                            let slice = unsafe { std::slice::from_raw_parts_mut(ptr.0, len) };
                            use std::os::windows::fs::FileExt;
                            let result = file.seek_read(slice, offset);
                            // Prevent closing the file handle
                            let _ = file.into_raw_handle();
                            let _ = tx.send(result);
                        }
                        #[cfg(windows)]
                        BlockingTask::FWrite(handle, ptr, len, tx) => {
                            let mut file = unsafe { std::fs::File::from_raw_handle(handle) };
                            let slice = unsafe { std::slice::from_raw_parts(ptr.0, len) };
                            use std::io::Write;
                            let result = file.write(slice);
                            // Prevent closing the file handle
                            let _ = file.into_raw_handle();
                            let _ = tx.send(result);
                        }
                        #[cfg(windows)]
                        BlockingTask::FWriteAt(handle, ptr, len, offset, tx) => {
                            let file = unsafe { std::fs::File::from_raw_handle(handle) };
                            let slice = unsafe { std::slice::from_raw_parts(ptr.0, len) };
                            use std::os::windows::fs::FileExt;
                            let result = file.seek_write(slice, offset);
                            // Prevent closing the file handle
                            let _ = file.into_raw_handle();
                            let _ = tx.send(result);
                        }
                        #[cfg(windows)]
                        BlockingTask::FSync(handle, data_only, tx) => {
                            let file = unsafe { std::fs::File::from_raw_handle(handle) };
                            let result = if data_only {
                                file.sync_data()
                            } else {
                                file.sync_all()
                            };
                            // Prevent closing the file handle
                            let _ = file.into_raw_handle();
                            let _ = tx.send(result);
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
    Executor::spawn(move || f())
}

pub fn unblock_open(path: impl Into<PathBuf>) -> Task<io::Result<File>> {
    let (tx, rx) = get_file_chan();
    Executor::get().schedule(BlockingTask::Open(path.into(), tx));
    Task::new(rx, Some(recycle_file))
}

pub fn unblock_create(path: impl Into<PathBuf>) -> Task<io::Result<File>> {
    let (tx, rx) = get_file_chan();
    Executor::get().schedule(BlockingTask::Create(path.into(), tx));
    Task::new(rx, Some(recycle_file))
}

pub fn unblock_remove_file(path: impl Into<PathBuf>) -> Task<io::Result<()>> {
    let (tx, rx) = get_void_chan();
    Executor::get().schedule(BlockingTask::RemoveFile(path.into(), tx));
    Task::new(rx, Some(recycle_void))
}

pub fn unblock_remove_dir(path: impl Into<PathBuf>) -> Task<io::Result<()>> {
    let (tx, rx) = get_void_chan();
    Executor::get().schedule(BlockingTask::RemoveDir(path.into(), tx));
    Task::new(rx, Some(recycle_void))
}

pub fn unblock_create_dir(path: impl Into<PathBuf>) -> Task<io::Result<()>> {
    let (tx, rx) = get_void_chan();
    Executor::get().schedule(BlockingTask::CreateDir(path.into(), tx));
    Task::new(rx, Some(recycle_void))
}

pub fn unblock_rename(from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> Task<io::Result<()>> {
    let (tx, rx) = get_void_chan();
    Executor::get().schedule(BlockingTask::Rename(from.into(), to.into(), tx));
    Task::new(rx, Some(recycle_void))
}

pub fn unblock_metadata(path: impl Into<PathBuf>) -> Task<io::Result<Metadata>> {
    let (tx, rx) = get_metadata_chan();
    Executor::get().schedule(BlockingTask::Metadata(path.into(), tx));
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
pub unsafe fn unblock_fmetadata(fd: RawFd) -> Task<io::Result<Metadata>> {
    let (tx, rx) = get_metadata_chan();
    Executor::get().schedule(BlockingTask::FMetadata(fd, tx));
    Task::new(rx, Some(recycle_metadata))
}

#[cfg(unix)]
pub unsafe fn unblock_fread(fd: RawFd, ptr: *mut u8, len: usize) -> Task<io::Result<usize>> {
    let (tx, rx) = get_io_size_chan();
    Executor::get().schedule(BlockingTask::FRead(fd, Ptr(ptr), len, tx));
    Task::new(rx, Some(recycle_io_size))
}

#[cfg(unix)]
pub unsafe fn unblock_fread_at(
    fd: RawFd,
    ptr: *mut u8,
    len: usize,
    offset: u64,
) -> Task<io::Result<usize>> {
    let (tx, rx) = get_io_size_chan();
    Executor::get().schedule(BlockingTask::FReadAt(fd, Ptr(ptr), len, offset, tx));
    Task::new(rx, Some(recycle_io_size))
}

#[cfg(unix)]
pub unsafe fn unblock_fwrite(fd: RawFd, ptr: *const u8, len: usize) -> Task<io::Result<usize>> {
    let (tx, rx) = get_io_size_chan();
    Executor::get().schedule(BlockingTask::FWrite(fd, Ptr(ptr as *mut u8), len, tx));
    Task::new(rx, Some(recycle_io_size))
}

#[cfg(unix)]
pub unsafe fn unblock_fwrite_at(
    fd: RawFd,
    ptr: *const u8,
    len: usize,
    offset: u64,
) -> Task<io::Result<usize>> {
    let (tx, rx) = get_io_size_chan();
    Executor::get().schedule(BlockingTask::FWriteAt(
        fd,
        Ptr(ptr as *mut u8),
        len,
        offset,
        tx,
    ));
    Task::new(rx, Some(recycle_io_size))
}

#[cfg(unix)]
pub unsafe fn unblock_fsync(fd: RawFd, data_only: bool) -> Task<io::Result<()>> {
    let (tx, rx) = get_void_chan();
    Executor::get().schedule(BlockingTask::FSync(fd, data_only, tx));
    Task::new(rx, Some(recycle_void))
}

// Windows variants
#[cfg(windows)]
pub unsafe fn unblock_fmetadata(handle: RawHandle) -> Task<io::Result<Metadata>> {
    let (tx, rx) = get_metadata_chan();
    Executor::get().schedule(BlockingTask::FMetadata(handle, tx));
    Task::new(rx, Some(recycle_metadata))
}

#[cfg(windows)]
pub unsafe fn unblock_fread(
    handle: RawHandle,
    ptr: *mut u8,
    len: usize,
) -> Task<io::Result<usize>> {
    let (tx, rx) = get_io_size_chan();
    Executor::get().schedule(BlockingTask::FRead(handle, Ptr(ptr), len, tx));
    Task::new(rx, Some(recycle_io_size))
}

#[cfg(windows)]
pub unsafe fn unblock_fread_at(
    handle: RawHandle,
    ptr: *mut u8,
    len: usize,
    offset: u64,
) -> Task<io::Result<usize>> {
    let (tx, rx) = get_io_size_chan();
    Executor::get().schedule(BlockingTask::FReadAt(handle, Ptr(ptr), len, offset, tx));
    Task::new(rx, Some(recycle_io_size))
}

#[cfg(windows)]
pub unsafe fn unblock_fwrite(
    handle: RawHandle,
    ptr: *const u8,
    len: usize,
) -> Task<io::Result<usize>> {
    let (tx, rx) = get_io_size_chan();
    Executor::get().schedule(BlockingTask::FWrite(handle, Ptr(ptr as *mut u8), len, tx));
    Task::new(rx, Some(recycle_io_size))
}

#[cfg(windows)]
pub unsafe fn unblock_fwrite_at(
    handle: RawHandle,
    ptr: *const u8,
    len: usize,
    offset: u64,
) -> Task<io::Result<usize>> {
    let (tx, rx) = get_io_size_chan();
    Executor::get().schedule(BlockingTask::FWriteAt(
        handle,
        Ptr(ptr as *mut u8),
        len,
        offset,
        tx,
    ));
    Task::new(rx, Some(recycle_io_size))
}

#[cfg(windows)]
pub unsafe fn unblock_fsync(handle: RawHandle, data_only: bool) -> Task<io::Result<()>> {
    let (tx, rx) = get_void_chan();
    Executor::get().schedule(BlockingTask::FSync(handle, data_only, tx));
    Task::new(rx, Some(recycle_void))
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
        assert_eq!(100, Executor::max_threads().get());

        // passed value below minimum, so we set it to minimum
        unsafe { env::set_var(MAX_THREADS_ENV, "0") };
        assert_eq!(1, Executor::max_threads().get());

        // passed value above maximum, so we set to allowed maximum
        unsafe { env::set_var(MAX_THREADS_ENV, "50000") };
        assert_eq!(10000, Executor::max_threads().get());

        // no env var, use default
        unsafe { env::set_var(MAX_THREADS_ENV, "") };
        assert_eq!(500, Executor::max_threads().get());

        // not a number, use default
        unsafe { env::set_var(MAX_THREADS_ENV, "NOTINT") };
        assert_eq!(500, Executor::max_threads().get());
    }
}
