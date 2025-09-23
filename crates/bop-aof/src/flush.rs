use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::config::SegmentId;
use super::error::{AofError, AofResult, BackpressureKind};
use super::segment::Segment;
use super::{InstanceId, ResidentSegment, TieredRuntime};
use crate::store::{DurabilityCursor, TieredInstance};
use crate::test_support::{MetadataPersistContext, metadata_persist_override};
use crossbeam::channel::{Receiver, Sender, TrySendError, unbounded};
use tokio::sync::Notify;
use tracing::{debug, error, warn};

pub enum FlushCommand {
    RegisterSegment { request: FlushRequest },
    Shutdown,
}

#[derive(Clone)]
pub struct FlushRequest {
    instance_id: InstanceId,
    tier: TieredInstance,
    durability: Arc<DurabilityCursor>,
    resident: ResidentSegment,
}

impl FlushRequest {
    pub fn new(
        instance_id: InstanceId,
        tier: TieredInstance,
        durability: Arc<DurabilityCursor>,
        resident: ResidentSegment,
    ) -> Self {
        Self {
            instance_id,
            tier,
            durability,
            resident,
        }
    }

    pub fn instance_id(&self) -> InstanceId {
        self.instance_id
    }

    pub fn durability(&self) -> Arc<DurabilityCursor> {
        self.durability.clone()
    }

    pub fn tier(&self) -> &TieredInstance {
        &self.tier
    }

    pub fn resident(&self) -> &ResidentSegment {
        &self.resident
    }

    pub fn segment(&self) -> &Arc<Segment> {
        self.resident.segment()
    }

    pub fn record_flush_failure(&self) {
        self.tier.record_would_block(BackpressureKind::Flush);
    }
}

pub struct SegmentFlushState {
    segment_id: SegmentId,
    requested_bytes: AtomicU32,
    durable_bytes: AtomicU32,
    notify: Notify,
    in_flight: AtomicBool,
    last_flush_millis: AtomicU64,
}

impl SegmentFlushState {
    pub fn new(segment_id: SegmentId) -> Arc<Self> {
        Arc::new(Self {
            segment_id,
            requested_bytes: AtomicU32::new(0),
            durable_bytes: AtomicU32::new(0),
            notify: Notify::new(),
            in_flight: AtomicBool::new(false),
            last_flush_millis: AtomicU64::new(now_millis()),
        })
    }

    pub fn with_progress(segment_id: SegmentId, requested: u32, durable: u32) -> Arc<Self> {
        Arc::new(Self {
            segment_id,
            requested_bytes: AtomicU32::new(requested),
            durable_bytes: AtomicU32::new(durable.min(requested)),
            notify: Notify::new(),
            in_flight: AtomicBool::new(false),
            last_flush_millis: AtomicU64::new(now_millis()),
        })
    }

    #[inline]
    pub fn segment_id(&self) -> SegmentId {
        self.segment_id
    }

    #[inline]
    pub fn requested_bytes(&self) -> u32 {
        self.requested_bytes.load(Ordering::Acquire)
    }

    #[inline]
    pub fn durable_bytes(&self) -> u32 {
        self.durable_bytes.load(Ordering::Acquire)
    }

    pub fn request_flush(&self, bytes: u32) {
        store_max(&self.requested_bytes, bytes);
        if self.durable_bytes.load(Ordering::Acquire) >= bytes {
            self.notify.notify_waiters();
        }
    }

    pub fn mark_durable(&self, bytes: u32) {
        let previous = store_max(&self.durable_bytes, bytes);
        if previous < bytes {
            self.last_flush_millis
                .store(now_millis(), Ordering::Release);
            self.notify.notify_waiters();
        }
    }

    pub async fn wait_for(&self, target_bytes: u32) {
        loop {
            if self.durable_bytes.load(Ordering::Acquire) >= target_bytes {
                return;
            }
            self.notify.notified().await;
        }
    }

    pub fn try_begin_flush(&self) -> bool {
        self.in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn finish_flush(&self) {
        self.in_flight.store(false, Ordering::Release);
    }

    pub fn restore(&self, requested: u32, durable: u32) {
        let capped = durable.min(requested);
        self.requested_bytes.store(requested, Ordering::Release);
        self.durable_bytes.store(capped, Ordering::Release);
        self.in_flight.store(false, Ordering::Release);
        self.last_flush_millis
            .store(now_millis(), Ordering::Release);
    }

    pub fn last_flush_millis(&self) -> u64 {
        self.last_flush_millis.load(Ordering::Acquire)
    }
}

fn store_max(cell: &AtomicU32, value: u32) -> u32 {
    let mut current = cell.load(Ordering::Acquire);
    while current < value {
        match cell.compare_exchange(current, value, Ordering::AcqRel, Ordering::Acquire) {
            Ok(prev) => return prev,
            Err(observed) => current = observed,
        }
    }
    current
}

pub(crate) fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::ZERO)
        .as_millis() as u64
}

const FLUSH_RETRY_MAX_ATTEMPTS: u32 = 5;
const FLUSH_RETRY_BASE_DELAY_MS: u64 = 5;
const FLUSH_RETRY_MAX_DELAY_MS: u64 = 250;

#[derive(Debug, Clone, Copy, Default)]
pub struct FlushMetricsSnapshot {
    pub scheduled_watermark: u64,
    pub scheduled_interval: u64,
    pub scheduled_backpressure: u64,
    pub synchronous_flushes: u64,
    pub asynchronous_flushes: u64,
    pub retry_attempts: u64,
    pub retry_failures: u64,
    pub flush_failures: u64,
    pub metadata_retry_attempts: u64,
    pub metadata_retry_failures: u64,
    pub metadata_failures: u64,
    pub backlog_bytes: u64,
}

#[derive(Default)]
pub struct FlushMetrics {
    scheduled_watermark: AtomicU64,
    scheduled_interval: AtomicU64,
    scheduled_backpressure: AtomicU64,
    synchronous_flushes: AtomicU64,
    asynchronous_flushes: AtomicU64,
    retry_attempts: AtomicU64,
    retry_failures: AtomicU64,
    flush_failures: AtomicU64,
    metadata_retry_attempts: AtomicU64,
    metadata_retry_failures: AtomicU64,
    metadata_failures: AtomicU64,
    backlog_bytes: AtomicU64,
}

impl FlushMetrics {
    #[inline]
    pub fn incr_watermark(&self) {
        self.scheduled_watermark.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn incr_interval(&self) {
        self.scheduled_interval.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn incr_backpressure(&self) {
        self.scheduled_backpressure.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn incr_sync_flush(&self) {
        self.synchronous_flushes.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn incr_async_flush(&self) {
        self.asynchronous_flushes.fetch_add(1, Ordering::Relaxed);
    }
    #[inline]
    pub fn add_retry_attempts(&self, attempts: u64) {
        if attempts > 0 {
            self.retry_attempts.fetch_add(attempts, Ordering::Relaxed);
        }
    }

    #[inline]
    pub fn incr_retry_failure(&self) {
        self.retry_failures.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn incr_flush_failure(&self) {
        self.flush_failures.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn add_metadata_retry_attempts(&self, attempts: u64) {
        if attempts > 0 {
            self.metadata_retry_attempts
                .fetch_add(attempts, Ordering::Relaxed);
        }
    }

    #[inline]
    pub fn incr_metadata_retry_failure(&self) {
        self.metadata_retry_failures.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn incr_metadata_failure(&self) {
        self.metadata_failures.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_backlog(&self, bytes: u64) {
        self.backlog_bytes.store(bytes, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> FlushMetricsSnapshot {
        FlushMetricsSnapshot {
            scheduled_watermark: self.scheduled_watermark.load(Ordering::Relaxed),
            scheduled_interval: self.scheduled_interval.load(Ordering::Relaxed),
            scheduled_backpressure: self.scheduled_backpressure.load(Ordering::Relaxed),
            synchronous_flushes: self.synchronous_flushes.load(Ordering::Relaxed),
            asynchronous_flushes: self.asynchronous_flushes.load(Ordering::Relaxed),
            retry_attempts: self.retry_attempts.load(Ordering::Relaxed),
            retry_failures: self.retry_failures.load(Ordering::Relaxed),
            flush_failures: self.flush_failures.load(Ordering::Relaxed),
            metadata_retry_attempts: self.metadata_retry_attempts.load(Ordering::Relaxed),
            metadata_retry_failures: self.metadata_retry_failures.load(Ordering::Relaxed),
            metadata_failures: self.metadata_failures.load(Ordering::Relaxed),
            backlog_bytes: self.backlog_bytes.load(Ordering::Relaxed),
        }
    }
}

pub(crate) fn flush_with_retry(
    segment: &Arc<Segment>,
    metrics: &Arc<FlushMetrics>,
) -> AofResult<()> {
    let mut retries = 0u32;
    loop {
        match segment.flush_to_disk() {
            Ok(()) => {
                if retries > 0 {
                    debug!(
                        segment_id = segment.id().as_u64(),
                        retries, "flush succeeded after retries"
                    );
                } else {
                    debug!(segment_id = segment.id().as_u64(), "flush succeeded");
                }
                return Ok(());
            }
            Err(err) => {
                debug!(segment_id = segment.id().as_u64(), attempt = retries + 1, error = %err, "retrying flush");
                if retries < FLUSH_RETRY_MAX_ATTEMPTS && is_retryable_error(&err) {
                    retries += 1;
                    metrics.add_retry_attempts(1);
                    thread::sleep(retry_backoff_delay(retries));
                    continue;
                }
                if retries > 0 {
                    metrics.incr_retry_failure();
                }
                warn!(segment_id = segment.id().as_u64(), retries, error = %err, "flush failed after retries");
                return Err(err);
            }
        }
    }
}

pub(crate) fn persist_metadata_with_retry(
    tier: &TieredInstance,
    metrics: &Arc<FlushMetrics>,
    segment_id: SegmentId,
    requested_bytes: u32,
    durable_bytes: u32,
) -> AofResult<()> {
    let mut retries = 0u32;
    loop {
        let ctx = MetadataPersistContext {
            instance_id: tier.instance_id(),
            segment_id,
            attempt: retries,
            requested_bytes,
            durable_bytes,
        };
        let result = metadata_persist_override(&ctx).unwrap_or_else(|| {
            tier.persist_durability_flush(segment_id, requested_bytes, durable_bytes)
        });
        match result {
            Ok(()) => {
                if retries > 0 {
                    debug!(
                        instance = tier.instance_id().get(),
                        segment = segment_id.as_u64(),
                        retries,
                        "metadata persisted after retries"
                    );
                }
                return Ok(());
            }
            Err(err) => {
                debug!(
                    instance = tier.instance_id().get(),
                    segment = segment_id.as_u64(),
                    attempt = retries + 1,
                    error = %err,
                    "retrying metadata persistence"
                );
                if retries < FLUSH_RETRY_MAX_ATTEMPTS && is_retryable_metadata_error(&err) {
                    retries += 1;
                    metrics.add_metadata_retry_attempts(1);
                    thread::sleep(retry_backoff_delay(retries));
                    continue;
                }
                if retries > 0 {
                    metrics.incr_metadata_retry_failure();
                }
                metrics.incr_metadata_failure();
                warn!(
                    instance = tier.instance_id().get(),
                    segment = segment_id.as_u64(),
                    retries,
                    error = %err,
                    "metadata persistence failed"
                );
                return Err(err);
            }
        }
    }
}

fn retry_backoff_delay(retries: u32) -> Duration {
    if retries == 0 {
        return Duration::from_millis(FLUSH_RETRY_BASE_DELAY_MS.min(FLUSH_RETRY_MAX_DELAY_MS));
    }
    let shift = retries.saturating_sub(1).min(6);
    let base = FLUSH_RETRY_BASE_DELAY_MS.saturating_mul(1u64 << shift);
    let jitter_window = base.max(1);
    let jitter_seed = now_millis() & 0x3f;
    let jitter = jitter_seed.min(jitter_window);
    let delay = base.saturating_add(jitter);
    Duration::from_millis(delay.min(FLUSH_RETRY_MAX_DELAY_MS))
}

fn is_retryable_metadata_error(err: &AofError) -> bool {
    matches!(err, AofError::Io(_) | AofError::FileSystem(_))
}

fn is_retryable_error(err: &AofError) -> bool {
    match err {
        AofError::Io(io_err) => is_retryable_io_error(io_err),
        _ => false,
    }
}

fn is_retryable_io_error(err: &io::Error) -> bool {
    match err.kind() {
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => {
            return true;
        }
        _ => {}
    }
    if let Some(code) = err.raw_os_error() {
        if matches!(
            code,
            libc::EINTR | libc::EAGAIN | libc::EBUSY | libc::ETIMEDOUT
        ) {
            return true;
        }
        if cfg!(windows) && matches!(code, 32 | 33 | 170 | 996 | 997) {
            return true;
        }
    }
    false
}
pub struct FlushManager {
    _runtime: Arc<TieredRuntime>,
    command_tx: Sender<FlushCommand>,
    total_unflushed: Arc<AtomicU64>,
    metrics: Arc<FlushMetrics>,
    flush_failed: Arc<AtomicBool>,
}

impl FlushManager {
    pub fn new(
        runtime: Arc<TieredRuntime>,
        total_unflushed: Arc<AtomicU64>,
        metrics: Arc<FlushMetrics>,
        flush_failed: Arc<AtomicBool>,
    ) -> Arc<Self> {
        let (tx, rx) = unbounded();
        let manager = Arc::new(Self {
            _runtime: runtime.clone(),
            command_tx: tx,
            total_unflushed: total_unflushed.clone(),
            metrics: metrics.clone(),
            flush_failed: flush_failed.clone(),
        });
        Self::spawn_worker(rx, total_unflushed, metrics, flush_failed);
        manager
    }

    pub fn enqueue_segment(&self, request: FlushRequest) -> AofResult<()> {
        self.command_tx
            .try_send(FlushCommand::RegisterSegment { request })
            .map_err(|err| match err {
                TrySendError::Full(_) | TrySendError::Disconnected(_) => AofError::Backpressure,
            })
    }

    #[cfg(test)]
    pub(crate) fn shutdown_worker_for_tests(&self) {
        let _ = self.command_tx.send(FlushCommand::Shutdown);
    }

    fn spawn_worker(
        rx: Receiver<FlushCommand>,
        total_unflushed: Arc<AtomicU64>,
        metrics: Arc<FlushMetrics>,
        flush_failed: Arc<AtomicBool>,
    ) {
        let _ = thread::Builder::new()
            .name("aof-flush".to_string())
            .spawn(move || Self::worker_loop(rx, total_unflushed, metrics, flush_failed));
    }

    fn worker_loop(
        rx: Receiver<FlushCommand>,
        total_unflushed: Arc<AtomicU64>,
        metrics: Arc<FlushMetrics>,
        flush_failed: Arc<AtomicBool>,
    ) {
        while let Ok(cmd) = rx.recv() {
            match cmd {
                FlushCommand::RegisterSegment { request } => {
                    if let Err(err) =
                        Self::flush_segment(&request, &total_unflushed, &metrics, &flush_failed)
                    {
                        error!(
                            instance = request.instance_id().get(),
                            segment = request.segment().id().as_u64(),
                            error = %err,
                            "flush manager failed to persist segment"
                        );
                    }
                }
                FlushCommand::Shutdown => break,
            }
        }
    }

    fn flush_segment(
        request: &FlushRequest,
        total_unflushed: &Arc<AtomicU64>,
        metrics: &Arc<FlushMetrics>,
        flush_failed: &Arc<AtomicBool>,
    ) -> AofResult<()> {
        let segment = request.segment();
        let flush_state = segment.flush_state();
        if flush_state.requested_bytes() <= flush_state.durable_bytes() {
            let snapshot = total_unflushed.load(Ordering::Acquire);
            metrics.record_backlog(snapshot);
            flush_state.finish_flush();
            return Ok(());
        }

        let target_bytes = flush_state.requested_bytes();
        let result = flush_with_retry(segment, metrics);
        let mut backlog_snapshot = total_unflushed.load(Ordering::Acquire);
        let result = match result {
            Ok(()) => {
                let requested_after = flush_state.requested_bytes();
                let durable_bytes = target_bytes.min(requested_after);
                match persist_metadata_with_retry(
                    request.tier(),
                    metrics,
                    segment.id(),
                    requested_after,
                    durable_bytes,
                ) {
                    Ok(()) => {
                        let delta = segment.mark_durable(durable_bytes);
                        backlog_snapshot = if delta > 0 {
                            total_unflushed
                                .fetch_sub(delta as u64, Ordering::AcqRel)
                                .saturating_sub(delta as u64)
                        } else {
                            backlog_snapshot
                        };
                        metrics.incr_async_flush();
                        flush_failed.store(false, Ordering::Release);
                        Ok(())
                    }
                    Err(err) => {
                        metrics.incr_flush_failure();
                        request.record_flush_failure();
                        flush_failed.store(true, Ordering::Release);
                        Err(err)
                    }
                }
            }
            Err(err) => {
                metrics.incr_flush_failure();
                request.record_flush_failure();
                flush_failed.store(true, Ordering::Release);
                Err(err)
            }
        };
        flush_state.finish_flush();
        metrics.record_backlog(backlog_snapshot);
        result
    }
}

impl Drop for FlushManager {
    fn drop(&mut self) {
        let _ = self.command_tx.send(FlushCommand::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn flush_state_progress_updates() {
        let segment_id = SegmentId::new(7);
        let state = SegmentFlushState::new(segment_id);
        assert_eq!(state.durable_bytes(), 0);
        state.request_flush(128);
        assert_eq!(state.requested_bytes(), 128);
        state.mark_durable(64);
        assert_eq!(state.durable_bytes(), 64);
        state.mark_durable(256);
        assert_eq!(state.durable_bytes(), 256);

        assert!(state.try_begin_flush());
        assert!(!state.try_begin_flush());
        state.finish_flush();
        assert!(state.try_begin_flush());
        state.finish_flush();
    }

    #[test]
    fn retryable_error_detection() {
        let transient = AofError::Io(io::Error::from_raw_os_error(libc::EINTR));
        assert!(super::is_retryable_error(&transient));
        let fatal = AofError::Io(io::Error::from_raw_os_error(libc::EIO));
        assert!(!super::is_retryable_error(&fatal));
    }

    #[test]
    fn retry_backoff_is_bounded() {
        let first = super::retry_backoff_delay(1);
        let second = super::retry_backoff_delay(2);
        assert!(second >= first);
        assert!(second <= Duration::from_millis(super::FLUSH_RETRY_MAX_DELAY_MS));
    }

    #[test]
    fn flush_retry_attempts_are_counted() {
        use crate::SegmentId;
        use crate::config::AofConfig;
        use crate::segment::Segment;
        use std::sync::Arc;
        use tempfile::TempDir;

        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let cfg = AofConfig::default();
        let segment = Arc::new(
            Segment::create_active(SegmentId::new(1), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("segment"),
        );
        segment.append_record(b"retry", 1).expect("append");
        segment.inject_flush_error(1);

        let metrics = Arc::new(FlushMetrics::default());
        flush_with_retry(&segment, &metrics).expect("flush");

        let _ = segment.mark_durable(segment.current_size());

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.retry_attempts, 1);
        assert_eq!(snapshot.retry_failures, 0);
        assert_eq!(snapshot.metadata_retry_attempts, 0);
        assert_eq!(snapshot.metadata_retry_failures, 0);
        assert_eq!(snapshot.metadata_failures, 0);
        assert_eq!(segment.durable_size(), segment.current_size());
    }

    #[test]
    fn metadata_metrics_increment() {
        let metrics = FlushMetrics::default();
        metrics.add_metadata_retry_attempts(3);
        metrics.incr_metadata_retry_failure();
        metrics.incr_metadata_failure();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.metadata_retry_attempts, 3);
        assert_eq!(snapshot.metadata_retry_failures, 1);
        assert_eq!(snapshot.metadata_failures, 1);
    }

    fn backlog_snapshot_records_bytes() {
        let metrics = FlushMetrics::default();
        metrics.record_backlog(123);
        assert_eq!(metrics.snapshot().backlog_bytes, 123);
        metrics.record_backlog(456);
        assert_eq!(metrics.snapshot().backlog_bytes, 456);
    }
}

