#![allow(dead_code)]

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossfire::{MTx, Rx, mpsc};
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, error, instrument, trace, warn};

use crate::runtime::StorageRuntime;
use crate::aof::{AofWalSegment, AofWalSegmentError};
use crate::manifest::{DbId, Manifest, AofStateKey, AofStateRecord};

/// Configuration options for the flush controller.
#[derive(Debug, Clone)]
pub struct FlushControllerConfig {
    pub queue_capacity: usize,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub max_concurrent_flushes: usize,
    pub max_retry_attempts: u32,
    /// Optional timeout for individual flush operations.
    /// If None, flushes will wait indefinitely.
    pub flush_timeout: Option<Duration>,
}

impl Default for FlushControllerConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 64,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_secs(1),
            max_concurrent_flushes: 2,
            max_retry_attempts: 5,
            flush_timeout: None, // No timeout by default
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
    #[error("flush retry limit exceeded after {0} attempts")]
    RetryLimitExceeded(u32),
    #[error("flush operation timed out")]
    Timeout,
}

impl From<crate::IoError> for FlushProcessError {
    fn from(value: crate::IoError) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<AofWalSegmentError> for FlushProcessError {
    fn from(value: AofWalSegmentError) -> Self {
        Self::Segment(value.to_string())
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
        manifest: Arc<Manifest>,
        db_id: DbId,
        config: FlushControllerConfig,
    ) -> Self {
        let capacity = config.queue_capacity.max(1);
        let (sender, receiver) = mpsc::bounded_blocking(capacity);
        let sender = Arc::new(sender);
        let state = Arc::new(FlushControllerState::new(
            runtime,
            manifest,
            db_id,
            config,
            sender.clone(),
        ));
        let worker = spawn_flush_worker(receiver, state.clone());
        Self {
            state,
            worker: Mutex::new(Some(worker)),
        }
    }

    #[instrument(skip(self, segment))]
    pub fn enqueue(&self, segment: Arc<AofWalSegment>) -> Result<(), FlushScheduleError> {
        trace!("enqueueing flush segment");
        self.state.push_task(segment, 0)
    }

    pub fn snapshot(&self) -> FlushControllerSnapshot {
        self.state.snapshot()
    }

    pub fn pending_queue_depth(&self) -> usize {
        self.state.pending.load(Ordering::Acquire)
    }

    #[instrument(skip(self))]
    pub fn shutdown(&self) {
        debug!("shutting down flush controller");
        if self.state.request_shutdown() {
            let _ = self.state.sender.send(FlushTask::Shutdown);
        }
        if let Ok(mut guard) = self.worker.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
        debug!("flush controller shutdown complete");
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
    manifest: Arc<Manifest>,
    db_id: DbId,
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
        manifest: Arc<Manifest>,
        db_id: DbId,
        config: FlushControllerConfig,
        sender: Arc<MTx<FlushTask>>,
    ) -> Self {
        Self {
            runtime,
            manifest,
            db_id,
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

    fn push_task(&self, segment: Arc<AofWalSegment>, attempt: u32) -> Result<(), FlushScheduleError> {
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

    fn schedule_retry(self: &Arc<Self>, segment: Arc<AofWalSegment>, attempt: u32) -> bool {
        if self.is_shutdown() || self.runtime.is_shutdown() {
            return false;
        }

        if attempt > self.config.max_retry_attempts {
            let error = FlushProcessError::RetryLimitExceeded(self.config.max_retry_attempts);
            self.metrics.record_failure(&error);
            return false;
        }

        self.metrics.record_retry();
        let backoff = self.backoff_duration(attempt);
        let state = Arc::downgrade(self);
        let handle = self.runtime.handle();
        handle.spawn(async move {
            if backoff > Duration::ZERO {
                sleep(backoff).await;
            }

            if let Some(state) = state.upgrade() {
                let _ = state.push_task(segment, attempt);
            }
        });
        true
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
        segment: Arc<AofWalSegment>,
        attempt: u32,
    },
    Completion {
        segment: Arc<AofWalSegment>,
        attempt: u32,
        result: Result<(), FlushProcessError>,
    },
    Shutdown,
}


fn spawn_flush_worker(receiver: Rx<FlushTask>, state: Arc<FlushControllerState>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("bop-storage-flush".into())
        .spawn(move || {
            debug!("flush worker thread started");
            flush_worker_main(receiver, state);
            debug!("flush worker thread exiting");
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

                if shutting_down {
                    segment.clear_flush_queue();
                } else {
                    match result {
                        Ok(()) => {
                            state.metrics.record_completed();
                            segment.clear_flush_queue();

                            // If there's more pending, re-enqueue
                            if !state.runtime.is_shutdown() && segment.pending_flush_target() > 0 {
                                let _ = state.push_task(segment.clone(), 0);
                            }
                        }
                        Err(ref error) => {
                            state.metrics.record_failure(error);
                            segment.clear_flush_queue();

                            // Schedule retry on error
                            if !state.runtime.is_shutdown() {
                                let retry_attempt = attempt.saturating_add(1);
                                state.schedule_retry(segment.clone(), retry_attempt);
                            }
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
                debug!("flush worker received shutdown signal");
                shutting_down = true;
                while let Some(item) = backlog.pop_front() {
                    item.segment.clear_flush_queue();
                }
                if active == 0 {
                    break;
                }
            }
            Err(_) => {
                warn!("flush worker channel closed");
                break;
            }
        }
    }
}


#[instrument(skip(state, segment), fields(db_id = state.db_id))]
fn run_flush_job(
    state: &Arc<FlushControllerState>,
    segment: Arc<AofWalSegment>,
    _attempt: u32,
) -> Result<(), FlushProcessError> {
    loop {
        let target = match segment.take_flush_target() {
            Some(target) => target,
            None => {
                trace!("no flush target, completing");
                return Ok(());
            }
        };

        if target <= segment.durable_size() {
            trace!(target, durable_size = segment.durable_size(), "target already durable, skipping");
            continue;
        }

        trace!(target, "flushing segment to disk");
        let io = segment.io();
        let start = Instant::now();

        // Flush to disk
        if let Err(error) = io.flush() {
            error!(error = ?error, "flush to disk failed");
            segment.restore_flush_target(target);
            return Err(FlushProcessError::from(error));
        }

        let duration = start.elapsed();
        debug!(target, ?duration, "flush to disk completed");
        segment.set_last_flush_duration(duration);

        // Update manifest asynchronously via worker (batched with other ops)
        let key = AofStateKey::new(state.db_id);
        let mut record = state
            .manifest
            .aof_state(state.db_id)
            .map_err(|err| FlushProcessError::Sink(err.to_string()))?
            .unwrap_or_else(AofStateRecord::default);

        // Only update if we're advancing the LSN
        if target > record.last_applied_lsn {
            trace!(old_lsn = record.last_applied_lsn, new_lsn = target, "updating manifest LSN");
            record.last_applied_lsn = target;

            // Fire-and-forget commit - manifest worker batches these updates
            let mut txn = state.manifest.begin_with_capacity(1);
            txn.put_aof_state(key, record);
            txn.commit_async()
                .map_err(|err| {
                    error!(error = %err, "manifest async commit failed");
                    FlushProcessError::Sink(err.to_string())
                })?;
            debug!(target, "manifest LSN updated");
        } else {
            trace!(target, current_lsn = record.last_applied_lsn, "LSN not advanced, skipping manifest update");
        }

        // Mark segment as durable (manifest update queued asynchronously)
        segment
            .mark_durable(target)
            .map_err(|err| FlushProcessError::from(err))?;
    }
}

#[derive(Debug)]
struct QueuedFlush {
    segment: Arc<AofWalSegment>,
    attempt: u32,
}

fn spawn_flush_job(state: &Arc<FlushControllerState>, segment: Arc<AofWalSegment>, attempt: u32) {
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


#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use tempfile::TempDir;

    use crate::IoFile;
    use crate::io::{IoError, IoResult, IoVec, IoVecMut};
    use crate::manifest::{Manifest, ManifestOptions};
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
    struct AlwaysFailIoFile;

    impl IoFile for AlwaysFailIoFile {
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
            Err(IoError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "always fail flush",
            )))
        }
    }

    fn runtime() -> Arc<StorageRuntime> {
        StorageRuntime::create(StorageRuntimeOptions::default()).expect("create runtime")
    }

    fn manifest() -> (Arc<Manifest>, TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest =
            Arc::new(Manifest::open(dir.path(), ManifestOptions::default()).expect("manifest"));
        (manifest, dir)
    }

    fn wal_segment(io: Arc<dyn IoFile>, preallocated: u64) -> Arc<AofWalSegment> {
        Arc::new(AofWalSegment::new(io, 0, preallocated))
    }

    #[test]
    fn flushes_segment_to_durable() {
        let runtime = runtime();
        let (manifest, _dir) = manifest();
        let io = Arc::new(MockIoFile::default());
        let db_id = 1;
        let controller = FlushController::new(
            runtime,
            manifest.clone(),
            db_id,
            FlushControllerConfig::default(),
        );
        let segment = wal_segment(io.clone(), 1024);

        segment.mark_written(512).unwrap();
        assert!(segment.request_flush(512).unwrap());

        controller.enqueue(segment.clone()).unwrap();

        wait_for(|| segment.durable_size() == 512, Duration::from_secs(1));

        assert_eq!(segment.durable_size(), 512);
        assert_eq!(io.flush_count(), 1);

        // Wait for manifest to be updated (async commit)
        wait_for(
            || {
                manifest
                    .aof_state(db_id)
                    .ok()
                    .flatten()
                    .map(|s| s.last_applied_lsn >= 512)
                    .unwrap_or(false)
            },
            Duration::from_secs(1),
        );

        // Verify manifest was updated
        let state = manifest.aof_state(db_id).expect("read aof state");
        assert!(state.is_some());
        assert_eq!(state.unwrap().last_applied_lsn, 512);

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
        let (manifest, _dir) = manifest();
        let io = Arc::new(MockIoFile::default());
        io.fail_once();
        let db_id = 2;
        let mut config = FlushControllerConfig::default();
        config.initial_backoff = Duration::from_millis(0);
        config.max_backoff = Duration::from_millis(0);
        let controller = FlushController::new(runtime, manifest.clone(), db_id, config);
        let segment = wal_segment(io.clone(), 1024);

        segment.mark_written(256).unwrap();
        assert!(segment.request_flush(256).unwrap());

        controller.enqueue(segment.clone()).unwrap();

        wait_for(|| segment.durable_size() == 256, Duration::from_secs(1));

        assert_eq!(io.flush_count(), 1);

        // Wait for manifest to be updated (async commit)
        wait_for(
            || {
                manifest
                    .aof_state(db_id)
                    .ok()
                    .flatten()
                    .map(|s| s.last_applied_lsn >= 256)
                    .unwrap_or(false)
            },
            Duration::from_secs(1),
        );

        // Verify manifest was updated
        let state = manifest.aof_state(db_id).expect("read aof state");
        assert!(state.is_some());
        assert_eq!(state.unwrap().last_applied_lsn, 256);

        assert!(controller.snapshot().failed >= 1);
        assert!(controller.snapshot().retries >= 1);

        controller.shutdown();
    }

    #[test]
    fn retry_limit_is_enforced() {
        let runtime = runtime();
        let (manifest, _dir) = manifest();
        let io = Arc::new(AlwaysFailIoFile::default());
        let db_id = 3;

        let mut config = FlushControllerConfig::default();
        config.initial_backoff = Duration::from_millis(0);
        config.max_backoff = Duration::from_millis(0);
        config.max_retry_attempts = 2;
        let controller = FlushController::new(runtime, manifest.clone(), db_id, config.clone());
        let segment = wal_segment(io.clone(), 1024);

        segment.mark_written(128).unwrap();
        assert!(segment.request_flush(128).unwrap());

        controller.enqueue(segment.clone()).unwrap();

        wait_for(
            || controller.snapshot().retries == config.max_retry_attempts as u64,
            Duration::from_secs(2),
        );

        wait_for(|| !segment.is_flush_enqueued(), Duration::from_secs(1));
        assert_eq!(segment.durable_size(), 0);
        assert!(controller.snapshot().failed >= 1);

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
