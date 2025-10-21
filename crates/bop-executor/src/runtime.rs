use crate::task::{
    ArenaConfig, ArenaOptions, ArenaStats, FutureHelpers, MmapExecutorArena, SpawnError, TaskHandle,
};
use crate::worker::{WaitStrategy, Worker};
use crate::worker_service::{WorkerService, WorkerServiceConfig};
use std::future::{Future, IntoFuture};
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;

/// Cooperative executor runtime backed by `MmapExecutorArena` and worker threads.
pub struct Runtime {
    inner: Arc<RuntimeInner>,
    workers: Vec<thread::JoinHandle<()>>,
}

struct RuntimeInner {
    arena: Arc<MmapExecutorArena>,
    shutdown: Arc<AtomicBool>,
    service: Arc<WorkerService>,
}

impl RuntimeInner {
    fn spawn<F, T>(&self, future: F) -> Result<JoinHandle<T>, SpawnError>
    where
        F: IntoFuture<Output = T> + Send + 'static,
        F::IntoFuture: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        if self.shutdown.load(Ordering::Acquire) || self.arena.is_closed() {
            return Err(SpawnError::Closed);
        }

        let Some(handle) = self.arena.reserve_task() else {
            return Err(if self.arena.is_closed() {
                SpawnError::Closed
            } else {
                SpawnError::NoCapacity
            });
        };

        let global_id = handle.global_id(self.arena.tasks_per_leaf());
        self.arena.init_task(global_id);

        let task = handle.task();
        let release_handle = TaskHandle::from_non_null(handle.as_non_null());
        let arena = Arc::clone(&self.arena);
        let shared = Arc::new(JoinShared::new());
        let shared_for_future = Arc::clone(&shared);
        let release_handle_for_future = release_handle;
        let arena_for_future = Arc::clone(&self.arena);

        let join_future = async move {
            let result = future.into_future().await;
            shared_for_future.complete(result);
            arena_for_future.release_task(release_handle_for_future);
        };

        let future_ptr = FutureHelpers::box_future(join_future);
        if task.attach_future(future_ptr).is_err() {
            unsafe { FutureHelpers::drop_boxed(future_ptr) };
            self.arena.release_task(release_handle);
            return Err(SpawnError::AttachFailed);
        }

        task.schedule();
        Ok(JoinHandle::new(shared))
    }

    fn shutdown(&self) {
        if self.shutdown.swap(true, Ordering::Release) {
            return;
        }
        self.arena.close();
        self.service.shutdown();
    }
}

impl Runtime {
    pub fn new(
        config: ArenaConfig,
        options: ArenaOptions,
        worker_count: usize,
    ) -> io::Result<Self> {
        let arena = Arc::new(MmapExecutorArena::with_config(config, options)?);
        let service = WorkerService::start(WorkerServiceConfig::default());
        let shutdown = Arc::new(AtomicBool::new(false));

        let inner = Arc::new(RuntimeInner {
            arena: Arc::clone(&arena),
            shutdown: Arc::clone(&shutdown),
            service: Arc::clone(&service),
        });

        let worker_threads = worker_count.max(1);
        let mut workers = Vec::with_capacity(worker_threads);
        for _ in 0..worker_threads {
            let mut worker = Worker::with_shutdown(
                Arc::clone(&inner.arena),
                Some(Arc::clone(&service)),
                Arc::clone(&shutdown),
            );
            let handle = thread::spawn(move || worker.run());
            workers.push(handle);
        }

        Ok(Self { inner, workers })
    }

    /// Spawns an asynchronous task on the runtime, returning an awaitable join handle.
    pub fn spawn<F, T>(&self, future: F) -> Result<JoinHandle<T>, SpawnError>
    where
        F: IntoFuture<Output = T> + Send + 'static,
        F::IntoFuture: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.inner.spawn(future)
    }

    /// Returns current arena statistics.
    pub fn stats(&self) -> ArenaStats {
        self.inner.arena.stats()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.inner.shutdown();

        for worker in &self.workers {
            worker.thread().unpark();
        }

        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

struct JoinShared<T> {
    ready: AtomicBool,
    state: Mutex<JoinSharedState<T>>,
}

struct JoinSharedState<T> {
    result: Option<T>,
    waker: Option<Waker>,
}

impl<T> JoinShared<T> {
    fn new() -> Self {
        Self {
            ready: AtomicBool::new(false),
            state: Mutex::new(JoinSharedState {
                result: None,
                waker: None,
            }),
        }
    }

    fn complete(&self, value: T) {
        let mut state = self.state.lock().unwrap();
        if state.result.is_some() {
            return;
        }
        state.result = Some(value);
        self.ready.store(true, Ordering::Release);
        let waker = state.waker.take();
        drop(state);
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn poll(&self, cx: &mut Context<'_>) -> Poll<T> {
        if self.ready.load(Ordering::Acquire) {
            let mut state = self.state.lock().unwrap();
            if let Some(value) = state.result.take() {
                return Poll::Ready(value);
            }
        }

        let mut state = self.state.lock().unwrap();
        if let Some(value) = state.result.take() {
            return Poll::Ready(value);
        }
        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }

    fn is_finished(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

/// Awaitable join handle returned from [`Runtime::spawn`].
pub struct JoinHandle<T> {
    shared: Arc<JoinShared<T>>,
}

impl<T> JoinHandle<T> {
    fn new(shared: Arc<JoinShared<T>>) -> Self {
        Self { shared }
    }

    /// Returns true if the task has completed.
    pub fn is_finished(&self) -> bool {
        self.shared.is_finished()
    }
}

impl<T: Send + 'static> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.shared.poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, RawWaker, RawWakerVTable};
    use std::time::Duration;

    fn noop_waker() -> Waker {
        unsafe fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        unsafe fn wake(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake, wake);
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = noop_waker();
        let mut future = Box::pin(future);
        loop {
            let mut cx = Context::from_waker(&waker);
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(output) => return output,
                Poll::Pending => thread::sleep(Duration::from_millis(1)),
            }
        }
    }

    #[test]
    fn runtime_spawn_completes() {
        let runtime = Runtime::new(ArenaConfig::new(1, 8).unwrap(), ArenaOptions::default(), 1)
            .expect("runtime");
        let counter = Arc::new(AtomicUsize::new(0));
        let cloned = counter.clone();

        let join = runtime
            .spawn(async move { cloned.fetch_add(1, Ordering::Relaxed) + 1 })
            .expect("spawn");

        let result = block_on(join);
        assert_eq!(result, 1);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        assert_eq!(runtime.stats().active_tasks, 0);
    }

    #[test]
    fn join_handle_reports_completion() {
        let runtime = Runtime::new(ArenaConfig::new(1, 4).unwrap(), ArenaOptions::default(), 1)
            .expect("runtime");
        let join = runtime.spawn(async {}).expect("spawn");
        assert!(!join.is_finished());
        block_on(join);
        assert_eq!(runtime.stats().active_tasks, 0);
    }
}
