#![allow(dead_code)]

use std::collections::VecDeque;
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossfire::{MTx, Rx, mpsc};
use thiserror::Error;

use crate::runtime::StorageRuntime;
use crate::wal::{WalSegment, WalSegmentError};

/// Configuration options for the flush controller.
#[derive(Debug, Clone)]
pub struct FlushControllerConfig {
    pub queue_capacity: usize,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub max_concurrent_flushes: usize,
}

impl Default for FlushControllerConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 64,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_secs(1),
            max_concurrent_flushes: 2,
        }
    }
}

/// Snapshot of controller metrics for diagnostics.
#[derive(Debug, Clone)]
pub struct FlushControllerSnapshot {
    pub pending_queue_depth: usize,
    pub enqueued: u64,
    pub completed: u64,
    pub failed: u64,
    pub retries: u64,
    pub last_error: Option<String>,
}

/// Errors produced when scheduling segments onto the flush controller.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FlushScheduleError {
    #[error("flush controller is shutdown")]
    Closed,
}

/// Errors surfaced while processing pending flush requests.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FlushProcessError {
    #[error("storage runtime is shut down")]
    RuntimeClosed,
    #[error("io error: {0}")]
    Io(String),
    #[error("flush sink error: {0}")]
    Sink(String),
    #[error("flush worker panic: {0}")]
    Panic(String),
    #[error("flush worker join error: {0}")]
    Join(String),
    #[error("wal segment state error: {0}")]
    Segment(String),
}

impl From<crate::IoError> for FlushProcessError {
    fn from(value: crate::IoError) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<WalSegmentError> for FlushProcessError {
    fn from(value: WalSegmentError) -> Self {
        Self::Segment(value.to_string())
    }
}

/// Trait used by the flush controller to persist durability progress to an external manifest.
pub trait FlushSink: Send + Sync {
    fn apply_flush(&self, request: FlushSinkRequest) -> Result<(), FlushSinkError>;
}

/// Error returned by [`FlushSink`] implementations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FlushSinkError {
    #[error("{0}")]
    Message(String),
}

impl From<String> for FlushSinkError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl From<&str> for FlushSinkError {
    fn from(value: &str) -> Self {
        Self::Message(value.to_string())
    }
}

/// Controller coordinating WAL durability flushes.
pub struct FlushController {
    state: Arc<FlushControllerState>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl FlushController {
    pub(crate) fn new(
        runtime: Arc<StorageRuntime>,
        sink: Arc<dyn FlushSink>,
        config: FlushControllerConfig,
    ) -> Self {
        let capacity = config.queue_capacity.max(1);
        let (sender, receiver) = mpsc::bounded_blocking(capacity);
        let sender = Arc::new(sender);
        let state = Arc::new(FlushControllerState::new(
            runtime,
            sink,
            config,
            sender.clone(),
        ));
        let worker = spawn_flush_worker(receiver, state.clone());
        Self {
            state,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn enqueue(&self, segment: Arc<WalSegment>) -> Result<(), FlushScheduleError> {
        self.state.push_task(segment, 0)
    }

    pub fn snapshot(&self) -> FlushControllerSnapshot {
        self.state.snapshot()
    }

    pub fn pending_queue_depth(&self) -> usize {
        self.state.pending.load(Ordering::Acquire)
    }

    pub fn shutdown(&self) {
        if self.state.request_shutdown() {
            let _ = self.state.sender.send(FlushTask::Shutdown);
        }
        if let Ok(mut guard) = self.worker.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for FlushController {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl fmt::Debug for FlushController {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlushController").finish()
    }
}

struct FlushControllerState {
    runtime: Arc<StorageRuntime>,
    sink: Arc<dyn FlushSink>,
    sender: Arc<MTx<FlushTask>>,
    pending: AtomicUsize,
    metrics: FlushControllerMetrics,
    shutdown: AtomicBool,
    config: FlushControllerConfig,
    max_concurrent: usize,
}

impl FlushControllerState {
    fn new(
        runtime: Arc<StorageRuntime>,
        sink: Arc<dyn FlushSink>,
        config: FlushControllerConfig,
        sender: Arc<MTx<FlushTask>>,
    ) -> Self {
        Self {
            runtime,
            sink,
            sender,
            pending: AtomicUsize::new(0),
            metrics: FlushControllerMetrics::default(),
            shutdown: AtomicBool::new(false),
            max_concurrent: config.max_concurrent_flushes.max(1),
            config,
        }
    }

    fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    fn request_shutdown(&self) -> bool {
        if self.shutdown.swap(true, Ordering::SeqCst) {
            return false;
        }
        true
    }

    fn snapshot(&self) -> FlushControllerSnapshot {
        FlushControllerSnapshot {
            pending_queue_depth: self.pending.load(Ordering::Acquire),
            enqueued: self.metrics.enqueued.load(Ordering::Relaxed),
            completed: self.metrics.completed.load(Ordering::Relaxed),
            failed: self.metrics.failed.load(Ordering::Relaxed),
            retries: self.metrics.retries.load(Ordering::Relaxed),
            last_error: self.metrics.last_error(),
        }
    }

    fn push_task(&self, segment: Arc<WalSegment>, attempt: u32) -> Result<(), FlushScheduleError> {
        if self.is_shutdown() || self.runtime.is_shutdown() {
            return Err(FlushScheduleError::Closed);
        }

        let depth = self.pending.fetch_add(1, Ordering::AcqRel) + 1;
        if !segment.try_enqueue_flush(depth) {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            return Ok(());
        }

        self.metrics.record_enqueued();

        if self
            .sender
            .send(FlushTask::Segment {
                segment: segment.clone(),
                attempt,
            })
            .is_err()
        {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            segment.clear_flush_queue();
            return Err(FlushScheduleError::Closed);
        }

        Ok(())
    }

    fn schedule_retry(self: &Arc<Self>, segment: Arc<WalSegment>, attempt: u32) {
        if self.is_shutdown() || self.runtime.is_shutdown() {
            return;
        }

        self.metrics.record_retry();
        let backoff = self.backoff_duration(attempt);
        let state = Arc::downgrade(self);
        thread::spawn(move || {
            if backoff > Duration::ZERO {
                thread::sleep(backoff);
            }

            if let Some(state) = state.upgrade() {
                let _ = state.push_task(segment, attempt);
            }
        });
    }

    fn backoff_duration(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        let base_ns = self.config.initial_backoff.as_nanos();
        if base_ns == 0 {
            return Duration::ZERO;
        }

        let max_ns = self
            .config
            .max_backoff
            .max(self.config.initial_backoff)
            .as_nanos();
        let shift = attempt.saturating_sub(1).min(32);
        let factor = 1u128 << shift;
        let delay_ns = base_ns.saturating_mul(factor).min(max_ns);
        let clamped = delay_ns.min(u64::MAX as u128);
        Duration::from_nanos(clamped as u64)
    }
}

impl fmt::Debug for FlushControllerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlushControllerState")
            .field("pending", &self.pending.load(Ordering::Acquire))
            .field("shutdown", &self.shutdown.load(Ordering::Acquire))
            .finish()
    }
}

#[derive(Debug, Default)]
struct FlushControllerMetrics {
    enqueued: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    retries: AtomicU64,
    last_error: Mutex<Option<String>>,
}

impl FlushControllerMetrics {
    fn record_enqueued(&self) {
        self.enqueued.fetch_add(1, Ordering::Relaxed);
    }

    fn record_completed(&self) {
        self.completed.fetch_add(1, Ordering::Relaxed);
        let mut guard = self
            .last_error
            .lock()
            .expect("flush metrics mutex poisoned");
        *guard = None;
    }

    fn record_failure(&self, error: &FlushProcessError) {
        self.failed.fetch_add(1, Ordering::Relaxed);
        let mut guard = self
            .last_error
            .lock()
            .expect("flush metrics mutex poisoned");
        *guard = Some(error.to_string());
    }

    fn record_retry(&self) {
        self.retries.fetch_add(1, Ordering::Relaxed);
    }

    fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .expect("flush metrics mutex poisoned")
            .clone()
    }
}

#[derive(Debug)]
pub(crate) enum FlushTask {
    Segment {
        segment: Arc<WalSegment>,
        attempt: u32,
    },
    Completion {
        segment: Arc<WalSegment>,
        attempt: u32,
        result: Result<ProcessOutcome, FlushProcessError>,
    },
    SinkAck {
        segment: Arc<WalSegment>,
        attempt: u32,
        target: u64,
        result: Result<(), FlushSinkError>,
    },
    Shutdown,
}

pub struct FlushSinkRequest {
    pub segment: Arc<WalSegment>,
    pub target: u64,
    pub responder: FlushSinkResponder,
}

struct FlushSinkResponderInner {
    sender: Arc<MTx<FlushTask>>,
    segment: Arc<WalSegment>,
    attempt: u32,
    target: u64,
    responded: AtomicBool,
}

pub struct FlushSinkResponder {
    inner: Arc<FlushSinkResponderInner>,
    notify_on_drop: bool,
}

impl Clone for FlushSinkResponder {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            notify_on_drop: self.notify_on_drop,
        }
    }
}

impl FlushSinkResponder {
    pub(crate) fn new(
        sender: Arc<MTx<FlushTask>>,
        segment: Arc<WalSegment>,
        attempt: u32,
        target: u64,
    ) -> Self {
        Self {
            inner: Arc::new(FlushSinkResponderInner {
                sender,
                segment,
                attempt,
                target,
                responded: AtomicBool::new(false),
            }),
            notify_on_drop: false,
        }
    }

    pub(crate) fn for_request(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            notify_on_drop: true,
        }
    }

    pub fn succeed(&self) {
        self.send_result(Ok(()));
    }

    pub fn fail(&self, error: FlushSinkError) {
        self.send_result(Err(error));
    }

    fn send_result(&self, result: Result<(), FlushSinkError>) {
        if self.inner.responded.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = self.inner.sender.send(FlushTask::SinkAck {
            segment: self.inner.segment.clone(),
            attempt: self.inner.attempt,
            target: self.inner.target,
            result,
        });
    }
}

impl Drop for FlushSinkResponder {
    fn drop(&mut self) {
        if !self.notify_on_drop {
            return;
        }
        if self.inner.responded.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = self.inner.sender.send(FlushTask::SinkAck {
            segment: self.inner.segment.clone(),
            attempt: self.inner.attempt,
            target: self.inner.target,
            result: Err(FlushSinkError::Message(
                "flush sink responder dropped without completion".to_string(),
            )),
        });
    }
}

fn spawn_flush_worker(receiver: Rx<FlushTask>, state: Arc<FlushControllerState>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("bop-storage-flush".into())
        .spawn(move || {
            flush_worker_main(receiver, state);
        })
        .expect("failed to spawn flush controller thread")
}

fn flush_worker_main(receiver: Rx<FlushTask>, state: Arc<FlushControllerState>) {
    let mut backlog: VecDeque<QueuedFlush> = VecDeque::new();
    let mut active = 0usize;
    let mut shutting_down = false;
    let max_concurrent = state.max_concurrent;

    loop {
        if shutting_down && active == 0 {
            break;
        }

        match receiver.recv() {
            Ok(FlushTask::Segment { segment, attempt }) => {
                if shutting_down {
                    segment.clear_flush_queue();
                    continue;
                }

                let previous = state.pending.fetch_sub(1, Ordering::AcqRel);
                let depth = previous.saturating_sub(1);
                segment.update_flush_queue_depth(depth);

                if state.runtime.is_shutdown() {
                    state
                        .metrics
                        .record_failure(&FlushProcessError::RuntimeClosed);
                    segment.clear_flush_queue();
                    continue;
                }

                if active < max_concurrent {
                    active += 1;
                    spawn_flush_job(&state, segment, attempt);
                } else {
                    backlog.push_back(QueuedFlush { segment, attempt });
                    if let Some(last) = backlog.back() {
                        last.segment.update_flush_queue_depth(backlog.len());
                    }
                }
            }
            Ok(FlushTask::Completion {
                segment,
                attempt,
                result,
            }) => {
                if active > 0 {
                    active -= 1;
                }

                let mut clear_queue = true;

                if shutting_down {
                    segment.clear_flush_queue();
                    clear_queue = false;
                } else {
                    match result {
                        Ok(ProcessOutcome::Completed) | Ok(ProcessOutcome::Idle) => {
                            state.metrics.record_completed();
                        }
                        Ok(ProcessOutcome::Reschedule) => {
                            state.metrics.record_completed();
                            if !state.runtime.is_shutdown() {
                                let depth_hint = backlog.len();
                                let _ = segment.try_enqueue_flush(depth_hint);
                                backlog.push_front(QueuedFlush {
                                    segment: segment.clone(),
                                    attempt: 0,
                                });
                                segment.update_flush_queue_depth(backlog.len());
                            }
                        }
                        Ok(ProcessOutcome::AwaitAck) => {
                            clear_queue = false;
                        }
                        Err(ref error) => {
                            state.metrics.record_failure(error);
                            if !state.runtime.is_shutdown() {
                                let retry_attempt = attempt.saturating_add(1);
                                state.schedule_retry(segment.clone(), retry_attempt);
                            }
                        }
                    }
                }

                if clear_queue {
                    segment.clear_flush_queue();
                }

                drain_flush_backlog(
                    &state,
                    &mut backlog,
                    &mut active,
                    max_concurrent,
                    shutting_down,
                );

                if shutting_down && active == 0 {
                    break;
                }
            }
            Ok(FlushTask::SinkAck {
                segment,
                attempt,
                target,
                result,
            }) => {
                match result {
                    Ok(()) => match segment.mark_durable(target) {
                        Ok(()) => {
                            state.metrics.record_completed();
                            segment.clear_flush_queue();
                            if !state.runtime.is_shutdown()
                                && segment.pending_flush_target() > 0
                            {
                                let _ = state.push_task(segment.clone(), 0);
                            }
                        }
                        Err(err) => {
                            let process_error = FlushProcessError::from(err);
                            state.metrics.record_failure(&process_error);
                            segment.restore_flush_target(target);
                            segment.clear_flush_queue();
                            if !state.runtime.is_shutdown() {
                                let retry_attempt = attempt.saturating_add(1);
                                state.schedule_retry(segment.clone(), retry_attempt);
                            }
                        }
                    },
                    Err(error) => {
                        state
                            .metrics
                            .record_failure(&FlushProcessError::Sink(error.to_string()));
                        segment.restore_flush_target(target);
                        segment.clear_flush_queue();
                        if !state.runtime.is_shutdown() {
                            let retry_attempt = attempt.saturating_add(1);
                            state.schedule_retry(segment.clone(), retry_attempt);
                        }
                    }
                }

                drain_flush_backlog(
                    &state,
                    &mut backlog,
                    &mut active,
                    max_concurrent,
                    shutting_down,
                );

                if shutting_down && active == 0 {
                    break;
                }
            }
            Ok(FlushTask::Shutdown) => {
                shutting_down = true;
                while let Some(item) = backlog.pop_front() {
                    item.segment.clear_flush_queue();
                }
                if active == 0 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ProcessOutcome {
    Idle,
    Completed,
    Reschedule,
    AwaitAck,
}

fn run_flush_job(
    state: &Arc<FlushControllerState>,
    segment: Arc<WalSegment>,
    attempt: u32,
) -> Result<ProcessOutcome, FlushProcessError> {
    let mut processed = false;
    let mut awaiting_ack = false;

    loop {
        let target = match segment.take_flush_target() {
            Some(target) => target,
            None => break,
        };

        if target <= segment.durable_size() {
            processed = true;
            break;
        }

        let io = segment.io();
        let start = Instant::now();

        if let Err(error) = io.flush() {
            segment.restore_flush_target(target);
            return Err(FlushProcessError::from(error));
        }

        let duration = start.elapsed();
        segment.set_last_flush_duration(duration);

        let responder =
            FlushSinkResponder::new(state.sender.clone(), segment.clone(), attempt, target);
        let request = FlushSinkRequest {
            segment: segment.clone(),
            target,
            responder: responder.for_request(),
        };

        if let Err(error) = apply_flush_sink(state.sink.as_ref(), request) {
            responder.fail(error.clone());
            awaiting_ack = true;
            processed = true;
            break;
        }

        awaiting_ack = true;
        processed = true;
    }

    if awaiting_ack {
        Ok(ProcessOutcome::AwaitAck)
    } else if segment.pending_flush_target() > 0 {
        Ok(ProcessOutcome::Reschedule)
    } else if processed {
        Ok(ProcessOutcome::Completed)
    } else {
        Ok(ProcessOutcome::Idle)
    }
}

#[derive(Debug)]
struct QueuedFlush {
    segment: Arc<WalSegment>,
    attempt: u32,
}

fn spawn_flush_job(state: &Arc<FlushControllerState>, segment: Arc<WalSegment>, attempt: u32) {
    let runtime = state.runtime.clone();
    let sender = state.sender.clone();
    let state = state.clone();
    runtime.handle().spawn_blocking(move || {
        let result = run_flush_job(&state, segment.clone(), attempt);
        let _ = sender.send(FlushTask::Completion {
            segment,
            attempt,
            result,
        });
    });
}

fn drain_flush_backlog(
    state: &Arc<FlushControllerState>,
    backlog: &mut VecDeque<QueuedFlush>,
    active: &mut usize,
    max_concurrent: usize,
    shutting_down: bool,
) {
    while *active < max_concurrent {
        let entry = match backlog.pop_front() {
            Some(entry) => entry,
            None => break,
        };

        if shutting_down || state.runtime.is_shutdown() {
            entry.segment.clear_flush_queue();
            continue;
        }

        *active += 1;
        spawn_flush_job(state, entry.segment, entry.attempt);
    }
}

fn apply_flush_sink(sink: &dyn FlushSink, request: FlushSinkRequest) -> Result<(), FlushSinkError> {
    match panic::catch_unwind(AssertUnwindSafe(|| sink.apply_flush(request))) {
        Ok(result) => result,
        Err(payload) => Err(FlushSinkError::Message(panic_message(payload))),
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(msg) = payload.downcast_ref::<&str>() {
        msg.to_string()
    } else if let Some(msg) = payload.downcast_ref::<String>() {
        msg.clone()
    } else {
        "unknown panic".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use crate::IoFile;
    use crate::io::{IoError, IoResult, IoVec, IoVecMut};
    use crate::runtime::StorageRuntimeOptions;

    #[derive(Debug, Default)]
    struct MockIoFile {
        flushes: AtomicUsize,
        fail_next: AtomicBool,
    }

    impl MockIoFile {
        fn fail_once(&self) {
            self.fail_next.store(true, Ordering::SeqCst);
        }

        fn flush_count(&self) -> usize {
            self.flushes.load(Ordering::SeqCst)
        }
    }

    impl IoFile for MockIoFile {
        fn readv_at(&self, _offset: u64, _bufs: &mut [IoVecMut<'_>]) -> IoResult<usize> {
            Ok(0)
        }

        fn writev_at(&self, _offset: u64, bufs: &[IoVec<'_>]) -> IoResult<usize> {
            Ok(bufs.iter().map(IoVec::len).sum())
        }

        fn allocate(&self, _offset: u64, _len: u64) -> IoResult<()> {
            Ok(())
        }

        fn flush(&self) -> IoResult<()> {
            if self.fail_next.swap(false, Ordering::SeqCst) {
                return Err(IoError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "mock flush failure",
                )));
            }
            self.flushes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct MockFlushSink {
        calls: Mutex<Vec<u64>>,
        fail_next: AtomicBool,
    }

    impl MockFlushSink {
        fn fail_once(&self) {
            self.fail_next.store(true, Ordering::SeqCst);
        }

        fn calls(&self) -> Vec<u64> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl FlushSink for MockFlushSink {
        fn apply_flush(&self, request: FlushSinkRequest) -> Result<(), FlushSinkError> {
            let FlushSinkRequest {
                target, responder, ..
            } = request;
            if self.fail_next.swap(false, Ordering::SeqCst) {
                responder.fail(FlushSinkError::Message("sink failure".into()));
            } else {
                self.calls.lock().unwrap().push(target);
                responder.succeed();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct AsyncAckFlushSink {
        calls: Mutex<Vec<u64>>,
        responders: Mutex<Vec<FlushSinkResponder>>,
    }

    impl AsyncAckFlushSink {
        fn pending_count(&self) -> usize {
            self.responders.lock().unwrap().len()
        }

        fn ack_next(&self) -> bool {
            match self.responders.lock().unwrap().pop() {
                Some(responder) => {
                    responder.succeed();
                    true
                }
                None => false,
            }
        }

        fn calls(&self) -> Vec<u64> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl FlushSink for AsyncAckFlushSink {
        fn apply_flush(&self, request: FlushSinkRequest) -> Result<(), FlushSinkError> {
            let FlushSinkRequest {
                target, responder, ..
            } = request;
            self.calls.lock().unwrap().push(target);
            self.responders.lock().unwrap().push(responder);
            Ok(())
        }
    }
    fn runtime() -> Arc<StorageRuntime> {
        StorageRuntime::create(StorageRuntimeOptions::default()).expect("create runtime")
    }

    fn wal_segment(io: Arc<dyn IoFile>, preallocated: u64) -> Arc<WalSegment> {
        Arc::new(WalSegment::new(io, 0, preallocated))
    }

    #[test]
    fn flushes_segment_to_durable() {
        let runtime = runtime();
        let io = Arc::new(MockIoFile::default());
        let sink = Arc::new(MockFlushSink::default());
        let controller =
            FlushController::new(runtime, sink.clone(), FlushControllerConfig::default());
        let segment = wal_segment(io.clone(), 1024);

        segment.mark_written(512).unwrap();
        assert!(segment.request_flush(512).unwrap());

        controller.enqueue(segment.clone()).unwrap();

        wait_for(|| segment.durable_size() == 512, Duration::from_secs(1));

        assert_eq!(segment.durable_size(), 512);
        assert_eq!(io.flush_count(), 1);
        assert_eq!(sink.calls(), vec![512]);
        let snapshot = controller.snapshot();
        assert_eq!(snapshot.enqueued, 1);
        assert_eq!(snapshot.completed, 1);
        assert_eq!(snapshot.failed, 0);
        assert_eq!(snapshot.retries, 0);
        assert_eq!(snapshot.pending_queue_depth, 0);

        controller.shutdown();
    }

    #[test]
    fn flush_failure_retries_until_success() {
        let runtime = runtime();
        let io = Arc::new(MockIoFile::default());
        io.fail_once();
        let sink = Arc::new(MockFlushSink::default());
        let mut config = FlushControllerConfig::default();
        config.initial_backoff = Duration::from_millis(0);
        config.max_backoff = Duration::from_millis(0);
        let controller = FlushController::new(runtime, sink.clone(), config);
        let segment = wal_segment(io.clone(), 1024);

        segment.mark_written(256).unwrap();
        assert!(segment.request_flush(256).unwrap());

        controller.enqueue(segment.clone()).unwrap();

        wait_for(|| segment.durable_size() == 256, Duration::from_secs(1));

        assert_eq!(io.flush_count(), 1);
        assert_eq!(sink.calls(), vec![256]);
        assert!(controller.snapshot().failed >= 1);
        assert!(controller.snapshot().retries >= 1);

        controller.shutdown();
    }

    #[test]
    fn sink_failure_triggers_retry() {
        let runtime = runtime();
        let io = Arc::new(MockIoFile::default());
        let sink = Arc::new(MockFlushSink::default());
        sink.fail_once();
        let mut config = FlushControllerConfig::default();
        config.initial_backoff = Duration::from_millis(0);
        config.max_backoff = Duration::from_millis(0);
        let controller = FlushController::new(runtime, sink.clone(), config);
        let segment = wal_segment(io.clone(), 2048);

        segment.mark_written(300).unwrap();
        assert!(segment.request_flush(300).unwrap());

        controller.enqueue(segment.clone()).unwrap();

        wait_for(|| segment.durable_size() == 300, Duration::from_secs(1));

        assert_eq!(segment.durable_size(), 300);
        assert_eq!(io.flush_count(), 2);
        assert_eq!(sink.calls(), vec![300]);
        assert!(controller.snapshot().failed >= 1);

        controller.shutdown();
    }

    #[test]
    fn awaits_sink_ack_before_marking_durable() {
        let runtime = runtime();
        let io = Arc::new(MockIoFile::default());
        let sink = Arc::new(AsyncAckFlushSink::default());
        let controller =
            FlushController::new(runtime, sink.clone(), FlushControllerConfig::default());
        let segment = wal_segment(io.clone(), 1024);

        segment.mark_written(512).unwrap();
        assert!(segment.request_flush(512).unwrap());

        controller.enqueue(segment.clone()).unwrap();

        wait_for(|| sink.pending_count() == 1, Duration::from_secs(1));

        assert_eq!(segment.durable_size(), 0);
        let snapshot_before_ack = controller.snapshot();
        assert_eq!(snapshot_before_ack.enqueued, 1);
        assert_eq!(snapshot_before_ack.completed, 0);

        assert!(sink.ack_next());

        wait_for(|| segment.durable_size() == 512, Duration::from_secs(1));
        wait_for(
            || controller.snapshot().completed == 1,
            Duration::from_secs(1),
        );

        let snapshot = controller.snapshot();
        assert_eq!(snapshot.enqueued, 1);
        assert_eq!(snapshot.completed, 1);
        assert_eq!(snapshot.failed, 0);
        assert_eq!(snapshot.retries, 0);
        assert_eq!(snapshot.pending_queue_depth, 0);
        assert_eq!(io.flush_count(), 1);
        assert_eq!(sink.calls(), vec![512]);

        controller.shutdown();
    }

    fn wait_for<F: Fn() -> bool>(predicate: F, timeout: Duration) {
        let start = std::time::Instant::now();
        while !predicate() {
            if start.elapsed() > timeout {
                panic!("condition not met within {:?}", timeout);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
