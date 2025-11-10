pub mod deque;
pub mod mpsc;
pub mod summary;
pub mod task;
pub mod timer;
pub mod timer_wheel;
pub mod worker;
pub mod waker;
pub mod signal;

use task::{
	FutureAllocator, SpawnError, TaskArena, TaskArenaConfig, TaskArenaOptions, TaskArenaStats,
	TaskHandle,
};
use worker::{WorkerService, WorkerServiceConfig};
use std::future::{Future, IntoFuture};
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;

/// Cooperative executor runtime backed by `MmapExecutorArena` and worker threads.
pub struct Runtime<const P: usize = 10, const NUM_SEGS_P2: usize = 6> {
	inner: Arc<RuntimeInner<P, NUM_SEGS_P2>>,
	workers: Vec<thread::JoinHandle<()>>,
}

struct RuntimeInner<const P: usize, const NUM_SEGS_P2: usize> {
	shutdown: Arc<AtomicBool>,
	service: Arc<WorkerService<P, NUM_SEGS_P2>>,
}

impl<const P: usize, const NUM_SEGS_P2: usize> RuntimeInner<P, NUM_SEGS_P2> {
	fn spawn<F, T>(&self, future: F) -> Result<JoinHandle<T>, SpawnError>
	where
			F: IntoFuture<Output = T> + Send + 'static,
			F::IntoFuture: Future<Output = T> + Send + 'static,
			T: Send + 'static,
	{
		let arena = self.service.arena();

		if self.shutdown.load(Ordering::Acquire) || arena.is_closed() {
			return Err(SpawnError::Closed);
		}

		let Some(handle) = self.service.reserve_task() else {
			return Err(if arena.is_closed() {
				SpawnError::Closed
			} else {
				SpawnError::NoCapacity
			});
		};

		let global_id = handle.global_id(arena.tasks_per_leaf());
		arena.init_task(global_id, self.service.summary() as *const _);

		let task = handle.task();
		let release_handle = TaskHandle::from_non_null(handle.as_non_null());
		let service = Arc::clone(&self.service);
		let shared = Arc::new(JoinShared::new());
		let shared_for_future = Arc::clone(&shared);
		let release_handle_for_future = release_handle;
		let service_for_future = Arc::clone(&self.service);

		let join_future = async move {
			let result = future.into_future().await;
			shared_for_future.complete(result);
			service_for_future.release_task(release_handle_for_future);
		};

		let future_ptr = FutureAllocator::box_future(join_future);
		if task.attach_future(future_ptr).is_err() {
			unsafe { FutureAllocator::drop_boxed(future_ptr) };
			self.service.release_task(release_handle);
			return Err(SpawnError::AttachFailed);
		}

		task.schedule();
		Ok(JoinHandle::new(shared))
	}

	fn shutdown(&self) {
		if self.shutdown.swap(true, Ordering::Release) {
			return;
		}
		self.service.arena().close();
		self.service.shutdown();
	}
}

impl<const P: usize, const NUM_SEGS_P2: usize> Runtime<P, NUM_SEGS_P2> {
	pub fn new(
		config: TaskArenaConfig,
		options: TaskArenaOptions,
		worker_count: usize,
	) -> io::Result<Self> {
		let arena = TaskArena::with_config(config, options)?;

		// Create config with desired worker count as min_workers
		// This ensures workers start immediately on WorkerService::start()
		let worker_config = WorkerServiceConfig {
			min_workers: worker_count,
			max_workers: worker_count.max(num_cpus::get()),
			..WorkerServiceConfig::default()
		};

		let service = WorkerService::<P, NUM_SEGS_P2>::start(arena, worker_config);
		let shutdown = Arc::new(AtomicBool::new(false));

		let inner = Arc::new(RuntimeInner::<P, NUM_SEGS_P2> {
			shutdown: Arc::clone(&shutdown),
			service: Arc::clone(&service),
		});

		let workers = Vec::new();

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
	pub fn stats(&self) -> TaskArenaStats {
		self.inner.service.arena().stats()
	}
}

impl<const P: usize, const NUM_SEGS_P2: usize> Drop for Runtime<P, NUM_SEGS_P2> {
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
	use crate::block_on;

	fn noop_waker() -> Waker {
		unsafe fn clone(_: *const ()) -> RawWaker {
			RawWaker::new(std::ptr::null(), &VTABLE)
		}
		unsafe fn wake(_: *const ()) {}
		static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake, wake);
		unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
	}

	#[test]
	fn runtime_spawn_completes() {
		let runtime = Runtime::<10, 6>::new(
			TaskArenaConfig::new(1, 8).unwrap(),
			TaskArenaOptions::default(),
			1,
		)
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
		let runtime = Runtime::<10, 6>::new(
			TaskArenaConfig::new(1, 4).unwrap(),
			TaskArenaOptions::default(),
			1,
		)
				.expect("runtime");
		let join = runtime.spawn(async {}).expect("spawn");
		assert!(!join.is_finished());
		block_on(join);
		assert_eq!(runtime.stats().active_tasks, 0);
	}
}
