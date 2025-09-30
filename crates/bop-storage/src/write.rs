#![allow(dead_code)]

use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crossfire::{MTx, Rx, mpsc};
use thiserror::Error;

use crate::IoFile;
use crate::runtime::StorageRuntime;
use crate::wal::{WalSegment, WalSegmentError, WriteBatch, WriteChunk};

/// Configuration options for the write controller.
#[derive(Debug, Clone)]
pub struct WriteControllerConfig {
    pub queue_capacity: usize,
    pub max_concurrent_writes: usize,
    pub max_inflight_segments: usize,
}

impl Default for WriteControllerConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 64,
            max_concurrent_writes: 4,
            max_inflight_segments: 16,
        }
    }
}

/// Snapshot of controller metrics for diagnostics.
#[derive(Debug, Clone)]
pub struct WriteControllerSnapshot {
    pub pending_queue_depth: usize,
    pub inflight_queue_depth: usize,
    pub peak_inflight_queue_depth: usize,
    pub enqueued: u64,
    pub completed: u64,
    pub failed: u64,
    pub last_error: Option<String>,
}

/// Errors produced when scheduling segments onto the write controller.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WriteScheduleError {
    #[error("write controller is shutdown")]
    Closed,
    #[error("write controller backpressure limit reached")]
    Backpressure,
}

/// Errors surfaced while processing staged write batches.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WriteProcessError {
    #[error("write queue missing staged batch")]
    MissingBatch,
    #[error("storage runtime is shut down")]
    RuntimeClosed,
    #[error("io error: {0}")]
    Io(String),
    #[error("partial write: expected {expected} bytes, wrote {actual}")]
    Partial { expected: usize, actual: usize },
    #[error("write worker panic: {0}")]
    Panic(String),
    #[error("write worker join error: {0}")]
    Join(String),
    #[error("wal segment state error: {0}")]
    Segment(String),
}

impl From<crate::IoError> for WriteProcessError {
    fn from(value: crate::IoError) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<WalSegmentError> for WriteProcessError {
    fn from(value: WalSegmentError) -> Self {
        Self::Segment(value.to_string())
    }
}

/// Controller managing WAL write-side scheduling and execution.
#[derive(Debug)]
pub struct WriteController {
    sender: MTx<WriteTask>,
    state: Arc<WriteControllerState>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl WriteController {
    pub(crate) fn new(runtime: Arc<StorageRuntime>, config: WriteControllerConfig) -> Self {
        let capacity = config.queue_capacity.max(1);
        let (sender, receiver) = mpsc::bounded_blocking(capacity);
        let inflight_limit = config
            .max_inflight_segments
            .max(config.max_concurrent_writes)
            .max(1);
        let state = Arc::new(WriteControllerState::new(
            runtime,
            config.max_concurrent_writes,
            inflight_limit,
        ));
        let worker = spawn_write_worker(receiver, sender.clone(), state.clone());
        Self {
            sender,
            state,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn enqueue(&self, segment: Arc<WalSegment>) -> Result<(), WriteScheduleError> {
        self.schedule(segment)
    }

    pub fn snapshot(&self) -> WriteControllerSnapshot {
        self.state.snapshot()
    }

    pub fn pending_queue_depth(&self) -> usize {
        self.state.pending.load(Ordering::Acquire)
    }

    pub fn shutdown(&self) {
        if self.state.request_shutdown() {
            let _ = self.sender.send(WriteTask::Shutdown);
        }
        if let Ok(mut guard) = self.worker.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }

    fn schedule(&self, segment: Arc<WalSegment>) -> Result<(), WriteScheduleError> {
        if self.state.is_shutdown() || self.state.runtime.is_shutdown() {
            return Err(WriteScheduleError::Closed);
        }

        self.state.try_acquire_slot()?;

        let depth = self.state.pending.fetch_add(1, Ordering::AcqRel) + 1;
        if !segment.try_enqueue_write(depth) {
            self.state.pending.fetch_sub(1, Ordering::AcqRel);
            self.state.release_slot();
            return Ok(());
        }

        self.state.metrics.record_enqueued();

        if self
            .sender
            .send(WriteTask::Segment(segment.clone()))
            .is_err()
        {
            self.state.pending.fetch_sub(1, Ordering::AcqRel);
            self.state.release_slot();
            segment.clear_write_queue();
            return Err(WriteScheduleError::Closed);
        }

        Ok(())
    }
}

impl Drop for WriteController {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug)]
struct WriteControllerState {
    runtime: Arc<StorageRuntime>,
    pending: AtomicUsize,
    inflight: AtomicUsize,
    peak_inflight: AtomicUsize,
    metrics: WriteControllerMetrics,
    shutdown: AtomicBool,
    max_concurrent: usize,
    inflight_limit: usize,
}

impl WriteControllerState {
    fn new(runtime: Arc<StorageRuntime>, max_concurrent: usize, inflight_limit: usize) -> Self {
        let capped_limit = inflight_limit.max(max_concurrent).max(1);
        Self {
            runtime,
            pending: AtomicUsize::new(0),
            inflight: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
            metrics: WriteControllerMetrics::default(),
            shutdown: AtomicBool::new(false),
            max_concurrent: max_concurrent.max(1),
            inflight_limit: capped_limit,
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

    fn try_acquire_slot(&self) -> Result<(), WriteScheduleError> {
        loop {
            let current = self.inflight.load(Ordering::Acquire);
            if current >= self.inflight_limit {
                return Err(WriteScheduleError::Backpressure);
            }
            let next = current + 1;
            if self
                .inflight
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.update_peak(next);
                return Ok(());
            }
        }
    }

    fn release_slot(&self) {
        let previous = self.inflight.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "write inflight underflow");
    }

    fn update_peak(&self, value: usize) {
        let mut observed = self.peak_inflight.load(Ordering::Relaxed);
        while value > observed {
            match self.peak_inflight.compare_exchange(
                observed,
                value,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(next) => observed = next,
            }
        }
    }

    fn snapshot(&self) -> WriteControllerSnapshot {
        WriteControllerSnapshot {
            pending_queue_depth: self.pending.load(Ordering::Acquire),
            inflight_queue_depth: self.inflight.load(Ordering::Acquire),
            peak_inflight_queue_depth: self.peak_inflight.load(Ordering::Relaxed),
            enqueued: self.metrics.enqueued.load(Ordering::Relaxed),
            completed: self.metrics.completed.load(Ordering::Relaxed),
            failed: self.metrics.failed.load(Ordering::Relaxed),
            last_error: self.metrics.last_error(),
        }
    }
}

#[derive(Debug, Default)]
struct WriteControllerMetrics {
    enqueued: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    last_error: Mutex<Option<String>>,
}

impl WriteControllerMetrics {
    fn record_enqueued(&self) {
        self.enqueued.fetch_add(1, Ordering::Relaxed);
    }

    fn record_completed(&self) {
        self.completed.fetch_add(1, Ordering::Relaxed);
        let mut guard = self
            .last_error
            .lock()
            .expect("write metrics mutex poisoned");
        *guard = None;
    }

    fn record_failure(&self, error: &WriteProcessError) {
        self.failed.fetch_add(1, Ordering::Relaxed);
        let mut guard = self
            .last_error
            .lock()
            .expect("write metrics mutex poisoned");
        *guard = Some(error.to_string());
    }

    fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .expect("write metrics mutex poisoned")
            .clone()
    }
}

#[derive(Debug)]
enum WriteTask {
    Segment(Arc<WalSegment>),
    Completion {
        segment: Arc<WalSegment>,
        result: Result<usize, WriteProcessError>,
    },
    Shutdown,
}

fn spawn_write_worker(
    receiver: Rx<WriteTask>,
    sender: MTx<WriteTask>,
    state: Arc<WriteControllerState>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("bop-storage-write".into())
        .spawn(move || {
            let mut backlog: VecDeque<Arc<WalSegment>> = VecDeque::new();
            let mut active = 0usize;
            let mut shutting_down = false;
            let max_concurrent = state.max_concurrent;

            loop {
                if shutting_down && active == 0 {
                    break;
                }

                match receiver.recv() {
                    Ok(WriteTask::Segment(segment)) => {
                        if shutting_down {
                            segment.clear_write_queue();
                            state.release_slot();
                            continue;
                        }

                        let previous = state.pending.fetch_sub(1, Ordering::AcqRel);
                        let depth = previous.saturating_sub(1);
                        segment.update_write_queue_depth(depth);

                        if state.runtime.is_shutdown() {
                            state
                                .metrics
                                .record_failure(&WriteProcessError::RuntimeClosed);
                            segment.clear_write_queue();
                            state.release_slot();
                            continue;
                        }

                        if active < max_concurrent {
                            active += 1;
                            spawn_write_job(&state, &sender, segment);
                        } else {
                            backlog.push_back(segment);
                        }
                    }
                    Ok(WriteTask::Completion { segment, result }) => {
                        if active > 0 {
                            active -= 1;
                        }

                        if shutting_down {
                            segment.clear_write_queue();
                        } else {
                            match result {
                                Ok(_) => state.metrics.record_completed(),
                                Err(ref error) => state.metrics.record_failure(error),
                            }
                            segment.clear_write_queue();
                        }

                        state.release_slot();

                        while active < max_concurrent {
                            if let Some(next) = backlog.pop_front() {
                                if shutting_down || state.runtime.is_shutdown() {
                                    next.clear_write_queue();
                                    state.release_slot();
                                    continue;
                                }
                                active += 1;
                                spawn_write_job(&state, &sender, next);
                            } else {
                                break;
                            }
                        }

                        if shutting_down && active == 0 {
                            break;
                        }
                    }
                    Ok(WriteTask::Shutdown) => {
                        shutting_down = true;
                        while let Some(queued) = backlog.pop_front() {
                            queued.clear_write_queue();
                            state.release_slot();
                        }
                        if active == 0 {
                            break;
                        }
                    }
                    Err(_) => {
                        while let Some(queued) = backlog.pop_front() {
                            queued.clear_write_queue();
                            state.release_slot();
                        }
                        break;
                    }
                }
            }
        })
        .expect("failed to spawn write controller thread")
}

fn spawn_write_job(
    state: &Arc<WriteControllerState>,
    sender: &MTx<WriteTask>,
    segment: Arc<WalSegment>,
) {
    let runtime = state.runtime.clone();
    let sender = sender.clone();
    runtime.handle().spawn_blocking(move || {
        let result = run_write_job(segment.clone());
        let _ = sender.send(WriteTask::Completion { segment, result });
    });
}

fn run_write_job(segment: Arc<WalSegment>) -> Result<usize, WriteProcessError> {
    let batch = segment
        .take_staged_batch()
        .ok_or(WriteProcessError::MissingBatch)?;

    let expected = batch_len(&batch);
    segment.set_last_write_batch_bytes(expected);

    if expected == 0 {
        return Ok(0);
    }

    let io = segment.io();
    let offset = segment.write_offset();

    match blocking_write(io, offset, batch) {
        WriteExecutionResult::Completed { bytes } => {
            let bytes_u64 = bytes as u64;
            segment.release_pending(bytes_u64)?;
            segment.mark_written(bytes_u64)?;
            Ok(bytes)
        }
        WriteExecutionResult::Failed { error, batch } => {
            segment.restore_active_batch(batch);
            Err(error)
        }
    }
}

fn batch_len(batch: &[WriteChunk]) -> usize {
    batch.iter().map(WriteChunk::len).sum()
}

#[derive(Debug)]
enum WriteExecutionResult {
    Completed {
        bytes: usize,
    },
    Failed {
        error: WriteProcessError,
        batch: WriteBatch,
    },
}

fn blocking_write(io: Arc<dyn IoFile>, offset: u64, batch: WriteBatch) -> WriteExecutionResult {
    match panic::catch_unwind(AssertUnwindSafe(|| {
        perform_write(io.as_ref(), offset, &batch)
    })) {
        Ok(Ok(bytes)) => WriteExecutionResult::Completed { bytes },
        Ok(Err(error)) => WriteExecutionResult::Failed { error, batch },
        Err(payload) => WriteExecutionResult::Failed {
            error: WriteProcessError::Panic(panic_message(payload)),
            batch,
        },
    }
}

fn perform_write(
    io: &dyn IoFile,
    offset: u64,
    batch: &WriteBatch,
) -> Result<usize, WriteProcessError> {
    let expected = batch_len(batch);
    if expected == 0 {
        return Ok(0);
    }

    let mut bufs = Vec::with_capacity(batch.len());
    for chunk in batch.iter() {
        bufs.push(chunk.as_io_vec());
    }

    match io.writev_at(offset, &bufs) {
        Ok(bytes) if bytes == expected => Ok(bytes),
        Ok(bytes) => Err(WriteProcessError::Partial {
            expected,
            actual: bytes,
        }),
        Err(err) => Err(WriteProcessError::from(err)),
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

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use crate::IoVec;
    use crate::io::{IoError, IoResult, IoVecMut};
    use crate::runtime::StorageRuntimeOptions;

    #[derive(Debug)]
    struct MockIoFile {
        writes: Mutex<Vec<(u64, Vec<u8>)>>,
        fail_next: AtomicBool,
        delay: Duration,
    }

    impl Default for MockIoFile {
        fn default() -> Self {
            Self {
                writes: Mutex::new(Vec::new()),
                fail_next: AtomicBool::new(false),
                delay: Duration::default(),
            }
        }
    }

    impl MockIoFile {
        fn with_delay(delay: Duration) -> Self {
            Self {
                delay,
                ..Default::default()
            }
        }

        fn fail_once(&self) {
            self.fail_next.store(true, Ordering::SeqCst);
        }

        fn bytes_written(&self) -> usize {
            let guard = self.writes.lock().unwrap();
            guard.iter().map(|(_, data)| data.len()).sum()
        }
    }

    impl IoFile for MockIoFile {
        fn readv_at(&self, _offset: u64, _bufs: &mut [IoVecMut<'_>]) -> IoResult<usize> {
            Ok(0)
        }

        fn writev_at(&self, offset: u64, bufs: &[IoVec<'_>]) -> IoResult<usize> {
            if self.fail_next.swap(false, Ordering::SeqCst) {
                return Err(IoError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "mock failure",
                )));
            }

            if self.delay > Duration::ZERO {
                thread::sleep(self.delay);
            }

            let mut payload = Vec::new();
            for buf in bufs {
                payload.extend_from_slice(buf.as_slice());
            }
            let total = payload.len();
            self.writes.lock().unwrap().push((offset, payload));
            Ok(total)
        }

        fn allocate(&self, _offset: u64, _len: u64) -> IoResult<()> {
            Ok(())
        }

        fn flush(&self) -> IoResult<()> {
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
    fn processes_staged_batch() {
        let runtime = runtime();
        let controller = WriteController::new(runtime.clone(), WriteControllerConfig::default());
        let io = Arc::new(MockIoFile::default());
        let segment = wal_segment(io.clone(), 1024);

        segment.reserve_pending(3).unwrap();
        segment.with_active_batch(|batch| batch.push(WriteChunk::Owned(vec![1, 2, 3])));
        segment.stage_active_batch().unwrap();

        controller.enqueue(segment.clone()).unwrap();

        wait_for(|| segment.written_size() == 3, Duration::from_secs(1));

        assert_eq!(segment.pending_size(), 0);
        assert_eq!(segment.written_size(), 3);
        assert_eq!(io.bytes_written(), 3);

        controller.shutdown();
    }

    #[test]
    fn failure_restores_batch_for_retry() {
        let runtime = runtime();
        let controller = WriteController::new(runtime.clone(), WriteControllerConfig::default());
        let io = Arc::new(MockIoFile::default());
        io.fail_once();
        let segment = wal_segment(io.clone(), 1024);

        segment.reserve_pending(3).unwrap();
        segment.with_active_batch(|batch| batch.push(WriteChunk::Owned(vec![1, 2, 3])));
        segment.stage_active_batch().unwrap();

        controller.enqueue(segment.clone()).unwrap();

        // allow worker to observe failure
        wait_for(|| segment.staged_bytes() == 0, Duration::from_secs(1));

        assert_eq!(segment.pending_size(), 3);
        assert_eq!(segment.written_size(), 0);

        // restage and succeed
        segment.stage_active_batch().unwrap();
        controller.enqueue(segment.clone()).unwrap();
        wait_for(|| segment.written_size() == 3, Duration::from_secs(1));
        assert_eq!(segment.pending_size(), 0);

        controller.shutdown();
    }

    #[test]
    fn respects_backpressure_limit() {
        let runtime = runtime();
        let mut config = WriteControllerConfig::default();
        config.queue_capacity = 2;
        config.max_concurrent_writes = 1;
        config.max_inflight_segments = 2;
        let controller = WriteController::new(runtime.clone(), config);
        let io = Arc::new(MockIoFile::with_delay(Duration::from_millis(50)));

        let segments: Vec<_> = (0..3).map(|_| wal_segment(io.clone(), 1024)).collect();

        for segment in &segments {
            segment.reserve_pending(1).unwrap();
            segment.with_active_batch(|batch| batch.push(WriteChunk::Owned(vec![1])));
            segment.stage_active_batch().unwrap();
        }

        controller.enqueue(segments[0].clone()).unwrap();
        controller.enqueue(segments[1].clone()).unwrap();
        let err = controller.enqueue(segments[2].clone()).unwrap_err();
        assert_eq!(err, WriteScheduleError::Backpressure);

        wait_for(|| segments[0].written_size() == 1, Duration::from_secs(2));
        wait_for(|| segments[1].written_size() == 1, Duration::from_secs(2));

        controller.enqueue(segments[2].clone()).unwrap();
        wait_for(|| segments[2].written_size() == 1, Duration::from_secs(2));

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
