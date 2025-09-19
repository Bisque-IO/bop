use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::read_dir;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::{runtime::Runtime, sync::watch};

pub mod config;
pub mod error;
pub mod flush;
pub mod fs;
pub mod reader;
pub mod segment;

pub use config::{
    AofConfig, CompactionPolicy, Compression, FlushConfig, IdStrategy, RecordId, RetentionPolicy,
    SegmentId,
};
pub use fs::{
    Layout, SEGMENT_FILE_EXTENSION, SegmentFileName, TempFileGuard, create_fixed_size_file,
    fsync_dir,
};
pub use reader::{RecordBounds, SegmentReader, SegmentRecord, TailFollower};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlushMetricSample {
    pub name: &'static str,
    pub value: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct FlushMetricsExporter {
    snapshot: FlushMetricsSnapshot,
}

impl FlushMetricsExporter {
    pub fn new(snapshot: FlushMetricsSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn retry_attempts(&self) -> u64 {
        self.snapshot.retry_attempts
    }

    pub fn retry_failures(&self) -> u64 {
        self.snapshot.retry_failures
    }

    pub fn backlog_bytes(&self) -> u64 {
        self.snapshot.backlog_bytes
    }

    pub fn samples(&self) -> impl Iterator<Item = FlushMetricSample> {
        const METRIC_NAMES: [(&str, fn(&FlushMetricsSnapshot) -> u64); 3] = [
            ("aof_flush_retry_attempts_total", |s| s.retry_attempts),
            ("aof_flush_retry_failures_total", |s| s.retry_failures),
            ("aof_flush_backlog_bytes", |s| s.backlog_bytes),
        ];
        METRIC_NAMES
            .into_iter()
            .map(move |(name, accessor)| FlushMetricSample {
                name,
                value: accessor(&self.snapshot),
            })
    }

    pub fn emit<F>(&self, mut writer: F)
    where
        F: FnMut(FlushMetricSample),
    {
        for sample in self.samples() {
            writer(sample);
        }
    }
}

use error::{AofError, AofResult};
use flush::{FlushManager, FlushMetrics, FlushMetricsSnapshot, flush_with_retry};
use segment::{Segment, SegmentAppendResult};

pub struct AofManagerConfig {
    pub max_segment_cache: u64,
    pub flush: FlushConfig,
}

impl Default for AofManagerConfig {
    fn default() -> Self {
        Self {
            max_segment_cache: 128,
            flush: FlushConfig::default(),
        }
    }
}

pub struct AofManager {
    rt: Arc<Runtime>,
}

impl AofManager {
    pub fn new(rt: Arc<Runtime>) -> Self {
        Self { rt }
    }
}

struct AppendState {
    tail: ArcSwapOption<Segment>,
    next_offset: AtomicU64,
    record_count: AtomicU64,
    unflushed_bytes: Arc<AtomicU64>,
    metrics: Arc<FlushMetrics>,
}

impl AppendState {
    fn new(metrics: Arc<FlushMetrics>) -> Self {
        let state = Self {
            tail: ArcSwapOption::from(None),
            next_offset: AtomicU64::new(0),
            record_count: AtomicU64::new(0),
            unflushed_bytes: Arc::new(AtomicU64::new(0)),
            metrics,
        };
        state.metrics.record_backlog(0);
        state
    }

    fn unflushed_handle(&self) -> Arc<AtomicU64> {
        self.unflushed_bytes.clone()
    }

    fn total_unflushed(&self) -> u64 {
        self.unflushed_bytes.load(Ordering::Acquire)
    }

    fn add_unflushed(&self, bytes: u64) {
        if bytes > 0 {
            let updated = self.unflushed_bytes.fetch_add(bytes, Ordering::AcqRel) + bytes;
            self.metrics.record_backlog(updated);
        }
    }

    fn sub_unflushed(&self, bytes: u64) {
        if bytes > 0 {
            let previous = self.unflushed_bytes.fetch_sub(bytes, Ordering::AcqRel);
            let updated = previous.saturating_sub(bytes);
            self.metrics.record_backlog(updated);
        }
    }

    fn set_unflushed(&self, bytes: u64) {
        self.unflushed_bytes.store(bytes, Ordering::Release);
        self.metrics.record_backlog(bytes);
    }
}

struct AofManagement {
    catalog: Vec<Arc<Segment>>,
    pending_finalize: VecDeque<Arc<Segment>>,
    next_segment_index: u32,
    flush_queue: VecDeque<Arc<Segment>>,
}

impl AofManagement {
    fn remove_pending(&mut self, segment: &Arc<Segment>) -> bool {
        if let Some(pos) = self
            .pending_finalize
            .iter()
            .position(|pending| Arc::ptr_eq(pending, segment))
        {
            self.pending_finalize.remove(pos);
            true
        } else {
            false
        }
    }

    fn remove_flush(&mut self, segment: &Arc<Segment>) -> bool {
        if let Some(pos) = self
            .flush_queue
            .iter()
            .position(|queued| Arc::ptr_eq(queued, segment))
        {
            self.flush_queue.remove(pos);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct SegmentCatalogEntry {
    pub segment_id: SegmentId,
    pub sealed: bool,
    pub base_offset: u64,
    pub base_record_count: u64,
    pub current_size: u32,
    pub record_count: u64,
}

#[derive(Debug, Clone)]
pub struct SegmentStatusSnapshot {
    pub segment_id: SegmentId,
    pub base_offset: u64,
    pub base_record_count: u64,
    pub current_size: u32,
    pub durable_size: u32,
    pub sealed: bool,
    pub created_at: i64,
    pub max_size: u32,
}

impl SegmentStatusSnapshot {
    fn from_segment(segment: &Arc<Segment>) -> Self {
        Self {
            segment_id: segment.id(),
            base_offset: segment.base_offset(),
            base_record_count: segment.base_record_count(),
            current_size: segment.current_size(),
            durable_size: segment.durable_size(),
            sealed: segment.is_sealed(),
            created_at: segment.created_at(),
            max_size: segment.max_size(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailEvent {
    None,
    Activated(SegmentId),
    Sealing(SegmentId),
    Sealed(SegmentId),
}

impl Default for TailEvent {
    fn default() -> Self {
        TailEvent::None
    }
}

#[derive(Debug, Clone, Default)]
pub struct TailState {
    pub version: u64,
    pub tail: Option<SegmentStatusSnapshot>,
    pub pending: Vec<SegmentStatusSnapshot>,
    pub last_event: TailEvent,
}

struct TailSignal {
    tx: watch::Sender<TailState>,
    state: Mutex<TailState>,
}

impl TailSignal {
    fn new() -> Self {
        let initial = TailState::default();
        let (tx, _rx) = watch::channel(initial.clone());
        Self {
            tx,
            state: Mutex::new(initial),
        }
    }

    fn subscribe(&self) -> watch::Receiver<TailState> {
        self.tx.subscribe()
    }

    fn replace(
        &self,
        tail: Option<SegmentStatusSnapshot>,
        pending: Vec<SegmentStatusSnapshot>,
        event: TailEvent,
    ) {
        let mut state = self.state.lock();
        state.tail = tail;
        state.pending = pending;
        state.last_event = event;
        state.version = state.version.wrapping_add(1);
        let _ = self.tx.send(state.clone());
    }

    fn snapshot_pending(queue: &VecDeque<Arc<Segment>>) -> Vec<SegmentStatusSnapshot> {
        queue
            .iter()
            .map(SegmentStatusSnapshot::from_segment)
            .collect()
    }
}

pub struct Aof {
    rt: Arc<Runtime>,
    config: AofConfig,
    layout: Layout,
    append: AppendState,
    management: Mutex<AofManagement>,
    metrics: Arc<FlushMetrics>,
    flush: Arc<FlushManager>,
    tail_signal: TailSignal,
}

impl Aof {
    pub fn new(rt: Arc<Runtime>, config: AofConfig) -> AofResult<Self> {
        let normalized = config.normalized();
        let layout = Layout::new(&normalized);
        layout.ensure()?;
        let metrics = Arc::new(FlushMetrics::default());
        let append = AppendState::new(metrics.clone());
        let flush = FlushManager::new(rt.clone(), append.unflushed_handle(), metrics.clone());
        let instance = Self {
            rt,
            config: normalized,
            layout,
            append,
            management: Mutex::new(AofManagement {
                catalog: Vec::new(),
                pending_finalize: VecDeque::new(),
                next_segment_index: 0,
                flush_queue: VecDeque::new(),
            }),
            metrics,
            flush,
            tail_signal: TailSignal::new(),
        };

        instance.recover_existing_segments()?;

        Ok(instance)
    }

    pub fn config(&self) -> &AofConfig {
        &self.config
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn catalog_snapshot(&self) -> Vec<SegmentCatalogEntry> {
        let state = self.management.lock();
        state
            .catalog
            .iter()
            .map(|segment| SegmentCatalogEntry {
                segment_id: segment.id(),
                sealed: segment.is_sealed(),
                base_offset: segment.base_offset(),
                base_record_count: segment.base_record_count(),
                current_size: segment.current_size(),
                record_count: segment.record_count(),
            })
            .collect()
    }

    pub fn segment_snapshot(&self) -> Vec<SegmentStatusSnapshot> {
        let state = self.management.lock();
        state
            .catalog
            .iter()
            .map(SegmentStatusSnapshot::from_segment)
            .collect()
    }

    pub fn open_reader(&self, segment_id: SegmentId) -> AofResult<SegmentReader> {
        let segment = {
            let state = self.management.lock();
            state
                .catalog
                .iter()
                .find(|segment| segment.id() == segment_id)
                .cloned()
        }
        .ok_or(AofError::SegmentNotLoaded(segment_id))?;

        SegmentReader::new(segment)
    }

    pub fn flush_metrics(&self) -> FlushMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn tail_events(&self) -> watch::Receiver<TailState> {
        self.tail_signal.subscribe()
    }

    pub fn tail_follower(&self) -> TailFollower {
        TailFollower::new(self.tail_signal.subscribe())
    }

    pub fn append_record(&self, payload: &[u8]) -> AofResult<RecordId> {
        if payload.is_empty() {
            return Err(AofError::InvalidState(
                "record payload is empty".to_string(),
            ));
        }

        let timestamp = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        let timestamp_u64 = if timestamp < 0 { 0 } else { timestamp as u64 };

        let segment = match self.try_get_writable_segment()? {
            Some(segment) => segment,
            None => return Err(AofError::WouldBlock),
        };

        self.append_with_segment(payload, timestamp_u64, segment)
    }

    pub fn append_record_with_timeout(
        &self,
        payload: &[u8],
        timeout: Duration,
    ) -> AofResult<RecordId> {
        if payload.is_empty() {
            return Err(AofError::InvalidState(
                "record payload is empty".to_string(),
            ));
        }

        let timestamp = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        let timestamp_u64 = if timestamp < 0 { 0 } else { timestamp as u64 };

        let mut segment = self.wait_for_writable_segment(timeout)?;
        let start = Instant::now();

        loop {
            match self.append_once(payload, timestamp_u64, &segment)? {
                AppendOutcome::Completed(id, full) => {
                    if full {
                        self.handle_segment_full(&segment)?;
                    }
                    return Ok(id);
                }
                AppendOutcome::SegmentFull => {
                    self.handle_segment_full(&segment)?;
                    let remaining = timeout
                        .checked_sub(start.elapsed())
                        .ok_or(AofError::WouldBlock)?;
                    segment = self.wait_for_writable_segment(remaining)?;
                }
            }
        }
    }

    pub fn wait_for_writable_segment(&self, timeout: Duration) -> AofResult<Arc<Segment>> {
        if let Some(segment) = self.current_tail() {
            return Ok(segment);
        }

        if timeout.is_zero() {
            return Err(AofError::WouldBlock);
        }

        let deadline = Instant::now() + timeout;

        let sleep_step = Duration::from_millis(1);

        loop {
            if let Some(segment) = self.current_tail() {
                return Ok(segment);
            }

            let now = Instant::now();

            let remaining = match deadline.checked_duration_since(now) {
                Some(rem) if !rem.is_zero() => rem,

                _ => return Err(AofError::WouldBlock),
            };

            if let Some(mut state) = self.management.try_lock() {
                if let Some(segment) =
                    self.ensure_segment_locked(&mut state, current_timestamp_nanos())?
                {
                    return Ok(segment);
                }
            }

            thread::sleep(sleep_step.min(remaining));
        }
    }

    pub fn seal_active(&self) -> AofResult<Option<SegmentId>> {
        let segment = match self.current_tail() {
            Some(segment) => segment,
            None => return Ok(None),
        };

        if segment.is_sealed() {
            return Ok(Some(segment.id()));
        }

        let previous = self.append.tail.swap(None);
        if let Some(prev) = previous {
            if !Arc::ptr_eq(&prev, &segment) {
                self.append.tail.store(Some(prev));
                return Err(AofError::WouldBlock);
            }
        }

        let delta = segment.mark_durable(segment.current_size());
        if delta > 0 {
            self.append.sub_unflushed(delta as u64);
        }
        segment.seal(current_timestamp_nanos())?;

        let mut state = self.management.lock();
        state.remove_pending(&segment);
        state.remove_flush(&segment);
        let pending_snapshot = TailSignal::snapshot_pending(&state.pending_finalize);
        drop(state);
        let tail_snapshot = self
            .append
            .tail
            .load_full()
            .map(|seg| SegmentStatusSnapshot::from_segment(&seg));
        self.tail_signal.replace(
            tail_snapshot,
            pending_snapshot,
            TailEvent::Sealed(segment.id()),
        );

        Ok(Some(segment.id()))
    }

    pub fn force_rollover(&self) -> AofResult<Arc<Segment>> {
        let _ = self.seal_active()?;
        let mut state = self.management.lock();
        match self.ensure_segment_locked(&mut state, current_timestamp_nanos())? {
            Some(segment) => Ok(segment),
            None => Err(AofError::WouldBlock),
        }
    }

    pub fn flush_until(&self, record_id: RecordId) -> AofResult<()> {
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let segment = self
            .segment_by_id(segment_id)
            .ok_or(AofError::RecordNotFound(record_id))?;

        let flush_state = segment.flush_state();
        let offset = segment
            .segment_offset_for(record_id)
            .ok_or_else(|| AofError::RecordNotFound(record_id))?;
        let target = segment.record_end_offset(offset)?;

        self.schedule_flush(&segment)?;
        self.rt.block_on(flush_state.wait_for(target));

        Ok(())
    }

    pub fn segment_finalized(&self, segment_id: SegmentId) -> AofResult<()> {
        let mut state = self.management.lock();
        if let Some(segment) = state
            .pending_finalize
            .iter()
            .find(|segment| segment.id() == segment_id)
            .cloned()
        {
            if state.remove_pending(&segment) {
                state.remove_flush(&segment);
                let pending_snapshot = TailSignal::snapshot_pending(&state.pending_finalize);
                drop(state);
                let tail_snapshot = self
                    .append
                    .tail
                    .load_full()
                    .map(|seg| SegmentStatusSnapshot::from_segment(&seg));
                self.tail_signal.replace(
                    tail_snapshot,
                    pending_snapshot,
                    TailEvent::Sealed(segment_id),
                );
                return Ok(());
            }
        }
        Err(AofError::InvalidState(format!(
            "segment {} not pending finalization",
            segment_id
        )))
    }

    fn append_with_segment(
        &self,
        payload: &[u8],
        timestamp: u64,
        mut segment: Arc<Segment>,
    ) -> AofResult<RecordId> {
        loop {
            match self.append_once(payload, timestamp, &segment)? {
                AppendOutcome::Completed(id, full) => {
                    if full {
                        self.handle_segment_full(&segment)?;
                    }
                    return Ok(id);
                }
                AppendOutcome::SegmentFull => {
                    self.handle_segment_full(&segment)?;
                    segment = match self.try_get_writable_segment()? {
                        Some(next) => next,
                        None => return Err(AofError::WouldBlock),
                    };
                }
            }
        }
    }

    fn append_once(
        &self,
        payload: &[u8],
        timestamp: u64,
        segment: &Arc<Segment>,
    ) -> AofResult<AppendOutcome> {
        let previous_offset = self.append.next_offset.load(Ordering::Acquire);
        match segment.append_record(payload, timestamp) {
            Ok(result) => {
                self.append
                    .next_offset
                    .store(result.last_offset, Ordering::Release);
                self.append.record_count.fetch_add(1, Ordering::AcqRel);
                let appended_bytes = result.last_offset.saturating_sub(previous_offset);
                self.append.add_unflushed(appended_bytes);
                let record_id = RecordId::from_parts(segment.id().as_u32(), result.segment_offset);
                self.maybe_schedule_flush(segment, &result)?;
                self.enforce_backpressure(segment, result.logical_size)?;
                Ok(AppendOutcome::Completed(record_id, result.is_full))
            }
            Err(AofError::SegmentFull(_)) => Ok(AppendOutcome::SegmentFull),
            Err(err) => Err(err),
        }
    }

    fn maybe_schedule_flush(
        &self,
        segment: &Arc<Segment>,
        result: &SegmentAppendResult,
    ) -> AofResult<()> {
        let flush_state = segment.flush_state();
        let requested = flush_state.requested_bytes() as u64;
        let durable = flush_state.durable_bytes() as u64;
        let backlog = requested.saturating_sub(durable);
        let threshold = self.config.flush.flush_watermark_bytes;
        let should_flush = if threshold == 0 {
            backlog > 0
        } else {
            backlog >= threshold
        };
        if should_flush {
            self.metrics.incr_watermark();
            self.schedule_flush(segment)?;
            return Ok(());
        }

        let interval = self.config.flush.flush_interval_ms;
        if interval > 0 {
            let now_ms = flush::now_millis();
            let last_ms = flush_state.last_flush_millis();
            if now_ms.saturating_sub(last_ms) >= interval {
                // ensure we only attempt if there is outstanding data
                if result.logical_size as u64 > flush_state.durable_bytes() as u64 {
                    self.metrics.incr_interval();
                    self.schedule_flush(segment)?;
                }
            }
        }
        Ok(())
    }

    fn schedule_flush(&self, segment: &Arc<Segment>) -> AofResult<()> {
        let flush_state = segment.flush_state();
        if flush_state.requested_bytes() <= flush_state.durable_bytes() {
            return Ok(());
        }
        if !flush_state.try_begin_flush() {
            return Ok(());
        }

        {
            let mut state = self.management.lock();
            state.remove_flush(segment);
            state.flush_queue.push_back(segment.clone());
        }

        match self.flush.enqueue_segment(segment.clone()) {
            Ok(()) => Ok(()),
            Err(AofError::Backpressure) => {
                flush_state.finish_flush();
                {
                    let mut state = self.management.lock();
                    state.remove_flush(segment);
                }
                flush_with_retry(&segment, &self.metrics)?;
                let delta = segment.mark_durable(segment.current_size());
                if delta > 0 {
                    self.append.sub_unflushed(delta as u64);
                }
                self.metrics.incr_sync_flush();
                Ok(())
            }
            Err(err) => {
                flush_state.finish_flush();
                {
                    let mut state = self.management.lock();
                    state.remove_flush(segment);
                }
                Err(err)
            }
        }
    }

    fn segment_by_id(&self, segment_id: SegmentId) -> Option<Arc<Segment>> {
        let state = self.management.lock();
        state
            .catalog
            .iter()
            .find(|segment| segment.id() == segment_id)
            .cloned()
    }

    fn enforce_backpressure(&self, segment: &Arc<Segment>, target_logical: u32) -> AofResult<()> {
        let limit = self.config.flush.max_unflushed_bytes;
        if limit == 0 {
            return Ok(());
        }
        if self.append.total_unflushed() <= limit {
            return Ok(());
        }

        self.metrics.incr_backpressure();
        self.schedule_flush(segment)?;
        let flush_state = segment.flush_state();
        self.rt.block_on(flush_state.wait_for(target_logical));
        Ok(())
    }

    fn try_get_writable_segment(&self) -> AofResult<Option<Arc<Segment>>> {
        if let Some(segment) = self.current_tail() {
            return Ok(Some(segment));
        }

        if let Some(mut state) = self.management.try_lock() {
            let timestamp = current_timestamp_nanos();
            return self.ensure_segment_locked(&mut state, timestamp);
        }

        Ok(None)
    }

    fn current_tail(&self) -> Option<Arc<Segment>> {
        if let Some(segment) = self.append.tail.load_full() {
            if segment.is_sealed() {
                self.append.tail.store(None);
                None
            } else {
                Some(segment)
            }
        } else {
            None
        }
    }

    fn ensure_segment_locked(
        &self,
        state: &mut AofManagement,
        created_at: i64,
    ) -> AofResult<Option<Arc<Segment>>> {
        if let Some(segment) = self.current_tail() {
            return Ok(Some(segment));
        }

        if state.pending_finalize.len() >= 2 {
            return Ok(None);
        }

        let segment_index = state.next_segment_index;
        let segment_id = SegmentId::new(segment_index as u64);
        let base_offset = self.append.next_offset.load(Ordering::Acquire);
        let base_record_count = self.append.record_count.load(Ordering::Acquire);
        let file_name = SegmentFileName::format(segment_id, base_offset, created_at);
        let path = self.layout.segment_path(&file_name);
        let segment_bytes = self.select_segment_bytes(state);

        let segment = Arc::new(Segment::create_active(
            segment_id,
            base_offset,
            base_record_count,
            created_at,
            segment_bytes,
            path.as_path(),
        )?);

        state.next_segment_index = segment_index.saturating_add(1);
        state.catalog.push(segment.clone());
        self.append.tail.store(Some(segment.clone()));
        let tail_snapshot = SegmentStatusSnapshot::from_segment(&segment);
        let pending_snapshot = TailSignal::snapshot_pending(&state.pending_finalize);
        self.tail_signal.replace(
            Some(tail_snapshot),
            pending_snapshot,
            TailEvent::Activated(segment.id()),
        );
        Ok(Some(segment))
    }

    fn handle_segment_full(&self, segment: &Arc<Segment>) -> AofResult<()> {
        let previous = self.append.tail.swap(None);
        let enqueue = match previous {
            Some(prev) if Arc::ptr_eq(&prev, segment) => true,
            Some(prev) => {
                self.append.tail.store(Some(prev));
                false
            }
            None => true,
        };

        if enqueue {
            let pending_snapshot;
            {
                let mut state = self.management.lock();
                if !state
                    .pending_finalize
                    .iter()
                    .any(|pending| Arc::ptr_eq(pending, segment))
                {
                    state.pending_finalize.push_back(segment.clone());
                }
                pending_snapshot = TailSignal::snapshot_pending(&state.pending_finalize);
            }
            self.schedule_flush(segment)?;
            let tail_snapshot = self
                .append
                .tail
                .load_full()
                .map(|seg| SegmentStatusSnapshot::from_segment(&seg));
            self.tail_signal.replace(
                tail_snapshot,
                pending_snapshot,
                TailEvent::Sealing(segment.id()),
            );
        }

        Ok(())
    }

    fn select_segment_bytes(&self, state: &AofManagement) -> u64 {
        let mut choice = if state.pending_finalize.is_empty() {
            self.config.segment_target_bytes
        } else {
            self.config.segment_min_bytes
        };
        choice = choice.clamp(self.config.segment_min_bytes, self.config.segment_max_bytes);
        choice
    }

    fn recover_existing_segments(&self) -> AofResult<()> {
        let mut entries = Vec::new();
        for entry in read_dir(self.layout.segments_dir()).map_err(AofError::from)? {
            let entry = entry.map_err(AofError::from)?;
            let path = entry.path();
            if !entry.file_type().map_err(AofError::from)?.is_file() {
                continue;
            }
            match path.extension().and_then(OsStr::to_str) {
                Some(ext) if ext.eq_ignore_ascii_case(&SEGMENT_FILE_EXTENSION[1..]) => {}
                _ => continue,
            }
            let name = SegmentFileName::parse(&path)?;
            entries.push((name, path));
        }

        if entries.is_empty() {
            return Ok(());
        }

        entries.sort_by_key(|(name, _)| name.segment_id.as_u64());

        let mut catalog = Vec::with_capacity(entries.len());
        let mut pending_finalize = VecDeque::new();
        let mut tail: Option<Arc<Segment>> = None;
        let mut next_offset = 0u64;
        let mut total_records = 0u64;
        let mut next_segment_index = 0u32;
        let mut unsealed_count = 0usize;
        let mut flush_queue = VecDeque::new();
        let mut total_unflushed = 0u64;

        for (name, path) in entries {
            let mut scan = Segment::scan_tail(&path)?;
            if scan.truncated {
                Segment::truncate_segment(&path, &mut scan)?;
            }

            if scan.header.segment_index != name.segment_id.as_u32() {
                return Err(AofError::Corruption(format!(
                    "segment {} header index {} mismatches filename {}",
                    path.display(),
                    scan.header.segment_index,
                    name.segment_id.as_u64()
                )));
            }

            if scan.header.base_offset != name.base_offset {
                return Err(AofError::Corruption(format!(
                    "segment {} header base offset {} mismatches filename {}",
                    path.display(),
                    scan.header.base_offset,
                    name.base_offset
                )));
            }

            if scan.header.created_at != name.created_at {
                return Err(AofError::Corruption(format!(
                    "segment {} header timestamp {} mismatches filename {}",
                    path.display(),
                    scan.header.created_at,
                    name.created_at
                )));
            }

            let segment = Arc::new(Segment::from_recovered(&path, &scan)?);

            let next_index = scan.header.segment_index.saturating_add(1);
            if next_index > next_segment_index {
                next_segment_index = next_index;
            }

            let segment_end_offset = if let Some(footer) = &scan.footer {
                footer.durable_bytes
            } else {
                scan.header.base_offset + scan.logical_size as u64
            };
            next_offset = segment_end_offset;
            total_records = scan.header.base_record_count + scan.record_count;

            if scan.footer.is_none() {
                unsealed_count += 1;
                if unsealed_count > 2 {
                    return Err(AofError::Corruption(format!(
                        "recovery detected {} unsealed segments; expected at most 2",
                        unsealed_count
                    )));
                }
                if tail.is_some() {
                    pending_finalize.push_back(segment.clone());
                    flush_queue.push_back(segment.clone());
                } else {
                    flush_queue.push_back(segment.clone());
                    tail = Some(segment.clone());
                }
            }

            let logical = segment.current_size() as u64;
            let durable_bytes = segment.durable_size() as u64;
            total_unflushed = total_unflushed.saturating_add(logical.saturating_sub(durable_bytes));
            catalog.push(segment);
        }

        let tail_snapshot = tail.as_ref().map(SegmentStatusSnapshot::from_segment);
        let pending_snapshot = TailSignal::snapshot_pending(&pending_finalize);
        let event = if let Some(snapshot) = tail_snapshot.as_ref() {
            TailEvent::Activated(snapshot.segment_id)
        } else if let Some(first_pending) = pending_snapshot.first() {
            TailEvent::Sealing(first_pending.segment_id)
        } else {
            TailEvent::None
        };

        let pending_for_flush: Vec<Arc<Segment>>;
        {
            let mut state = self.management.lock();
            state.catalog = catalog;
            state.pending_finalize = pending_finalize;
            state.next_segment_index = next_segment_index;
            state.flush_queue = flush_queue;
            pending_for_flush = state.flush_queue.iter().cloned().collect();
        }

        self.append.set_unflushed(total_unflushed);

        self.append
            .next_offset
            .store(next_offset, Ordering::Release);
        self.append
            .record_count
            .store(total_records, Ordering::Release);
        self.append.tail.store(tail);

        self.tail_signal
            .replace(tail_snapshot, pending_snapshot, event);

        for segment in pending_for_flush {
            self.flush.enqueue_segment(segment)?;
        }

        Ok(())
    }
}

fn current_timestamp_nanos() -> i64 {
    let now = Utc::now();
    now.timestamp_nanos_opt()
        .unwrap_or_else(|| now.timestamp() * 1_000_000_000)
}

enum AppendOutcome {
    Completed(RecordId, bool),
    SegmentFull,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn flush_metrics_exporter_emits_retry_and_backlog() {
        let snapshot = FlushMetricsSnapshot {
            scheduled_watermark: 1,
            scheduled_interval: 2,
            scheduled_backpressure: 3,
            synchronous_flushes: 4,
            asynchronous_flushes: 5,
            retry_attempts: 6,
            retry_failures: 7,
            backlog_bytes: 8,
        };
        let exporter = FlushMetricsExporter::new(snapshot);
        assert_eq!(exporter.retry_attempts(), 6);
        assert_eq!(exporter.retry_failures(), 7);
        assert_eq!(exporter.backlog_bytes(), 8);

        let mut samples: Vec<FlushMetricSample> = exporter.samples().collect();
        samples.sort_by(|a, b| a.name.cmp(b.name));
        assert_eq!(samples.len(), 3);
        assert_eq!(
            samples[0],
            FlushMetricSample {
                name: "aof_flush_backlog_bytes",
                value: 8
            }
        );
        assert_eq!(
            samples[1],
            FlushMetricSample {
                name: "aof_flush_retry_attempts_total",
                value: 6
            }
        );
        assert_eq!(
            samples[2],
            FlushMetricSample {
                name: "aof_flush_retry_failures_total",
                value: 7
            }
        );

        let mut emitted = Vec::new();
        exporter.emit(|sample| emitted.push(sample));
        emitted.sort_by(|a, b| a.name.cmp(b.name));
        assert_eq!(samples, emitted);
    }
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::runtime::Runtime;
    use tokio::time::timeout;

    async fn wait_for_event(rx: &mut watch::Receiver<TailState>) -> TailState {
        timeout(Duration::from_millis(500), rx.changed())
            .await
            .expect("tail event not observed in time")
            .expect("tail event stream closed");
        rx.borrow().clone()
    }

    fn test_config(root: &Path) -> AofConfig {
        let mut cfg = AofConfig::default();
        cfg.root_dir = root.join("aof");
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        cfg
    }

    #[test]
    fn ensure_active_segment_respects_pending_limit() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 64 * 1024;
        cfg.segment_max_bytes = 64 * 1024;
        cfg.segment_target_bytes = 64 * 1024;
        let aof = Aof::new(rt.clone(), cfg).expect("aof");

        let mut state = aof.management.lock();
        let created_at = 0;
        let seg1 = aof
            .ensure_segment_locked(&mut state, created_at)
            .unwrap()
            .unwrap();
        aof.append.tail.store(None);
        state.pending_finalize.push_back(seg1.clone());
        let seg2 = aof
            .ensure_segment_locked(&mut state, created_at)
            .unwrap()
            .unwrap();
        aof.append.tail.store(None);
        state.pending_finalize.push_back(seg2.clone());
        drop(state);

        assert!(matches!(aof.try_get_writable_segment(), Ok(None)));

        aof.segment_finalized(seg1.id()).expect("finalized");
        assert!(aof.try_get_writable_segment().unwrap().is_some());
    }

    #[test]
    fn seal_active_seals_tail_segment() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        let aof = Aof::new(rt.clone(), cfg).expect("aof");

        let segment = aof.try_get_writable_segment().expect("segment");
        let segment = segment.expect("initial segment");

        aof.append_record(b"payload").expect("append");

        let sealed_id = aof.seal_active().expect("seal");
        assert_eq!(sealed_id, Some(segment.id()));
        assert!(segment.is_sealed());

        let snapshot = aof.catalog_snapshot();
        assert_eq!(snapshot.len(), 1);
        let entry = &snapshot[0];
        assert_eq!(entry.segment_id, segment.id());
        assert!(entry.sealed);
    }

    #[test]
    fn force_rollover_allocates_new_segment() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        let aof = Aof::new(rt.clone(), cfg).expect("aof");

        let initial = aof
            .try_get_writable_segment()
            .expect("segment")
            .expect("initial");
        aof.append_record(b"payload").expect("append");

        let next = aof.force_rollover().expect("rollover");
        assert_ne!(initial.id().as_u64(), next.id().as_u64());
        assert!(initial.is_sealed());
        assert!(!next.is_sealed());

        let snapshot = aof.catalog_snapshot();
        assert_eq!(snapshot.len(), 2);
        assert!(
            snapshot
                .iter()
                .any(|entry| entry.segment_id == initial.id() && entry.sealed)
        );
        assert!(
            snapshot
                .iter()
                .any(|entry| entry.segment_id == next.id() && !entry.sealed)
        );
    }

    #[test]
    fn recovery_reopens_existing_tail_segment() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");

        let aof = Aof::new(rt.clone(), test_config(tmp.path())).expect("initial");
        aof.append_record(b"one").expect("append one");
        let sealed_id = aof.seal_active().expect("seal").expect("sealed");
        assert_eq!(sealed_id.as_u64(), 0);
        let tail = aof.force_rollover().expect("rollover");
        assert_eq!(tail.id().as_u64(), 1);
        aof.append_record(b"two").expect("append two");
        drop(aof);

        let recovered = Aof::new(rt.clone(), test_config(tmp.path())).expect("recover");
        let snapshot = recovered.catalog_snapshot();
        assert_eq!(snapshot.len(), 2);
        let tail_id = snapshot
            .iter()
            .find(|entry| !entry.sealed)
            .map(|entry| entry.segment_id)
            .expect("tail");
        assert_eq!(tail_id.as_u64(), 1);

        let rid = recovered.append_record(b"after").expect("append after");
        assert_eq!(rid.segment_index(), tail_id.as_u32());
    }

    #[test]
    fn recovery_with_sealed_segments_allows_new_segment_creation() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");

        {
            let aof = Aof::new(rt.clone(), test_config(tmp.path())).expect("initial");
            aof.append_record(b"only").expect("append only");
            aof.seal_active().expect("seal").expect("sealed");
        }

        let recovered = Aof::new(rt, test_config(tmp.path())).expect("recover");
        let snapshot = recovered.catalog_snapshot();
        assert_eq!(snapshot.len(), 1);
        assert!(snapshot[0].sealed);

        recovered.append_record(b"new").expect("append new");
        let snapshot_after = recovered.catalog_snapshot();
        assert_eq!(snapshot_after.len(), 2);
        assert!(snapshot_after.iter().any(|entry| !entry.sealed));
    }

    #[test]
    fn recovery_truncates_partial_tail_segment() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");

        {
            let aof = Aof::new(rt.clone(), test_config(tmp.path())).expect("initial");
            let segment = aof
                .try_get_writable_segment()
                .expect("segment")
                .expect("tail");
            aof.append_record(b"good").expect("append good");

            let next_offset = segment.current_size();
            let name =
                SegmentFileName::format(segment.id(), segment.base_offset(), segment.created_at());
            let path = aof.layout().segment_path(&name);
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .expect("open segment");
            let bogus_length = segment.max_size();
            let mut header = [0u8; 16];
            header[0..4].copy_from_slice(&bogus_length.to_le_bytes());
            file.seek(SeekFrom::Start(next_offset as u64))
                .expect("seek");
            file.write_all(&header).expect("write header");
        }

        let recovered = Aof::new(rt, test_config(tmp.path())).expect("recover");
        let snapshot = recovered.catalog_snapshot();
        assert!(snapshot.iter().any(|entry| !entry.sealed));
        recovered
            .append_record(b"post-truncation")
            .expect("append after recovery");
    }

    #[test]
    fn recovery_seeds_flush_queue_for_unsealed_segments() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");

        {
            let aof = Aof::new(rt.clone(), test_config(tmp.path())).expect("initial");
            aof.append_record(b"queued").expect("append queued");
        }

        let recovered = Aof::new(rt, test_config(tmp.path())).expect("recover");
        let state = recovered.management.lock();
        assert!(
            !state.flush_queue.is_empty(),
            "recovery should seed flush queue for unsealed segments"
        );
    }
    #[test]
    fn flush_metrics_tracks_backlog_bytes() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");
        let aof = Aof::new(rt.clone(), test_config(tmp.path())).expect("aof");

        assert_eq!(aof.flush_metrics().backlog_bytes, 0);

        let record_id = aof.append_record(b"metric").expect("append");
        let snapshot = aof.flush_metrics();
        assert!(snapshot.backlog_bytes > 0);

        aof.flush_until(record_id).expect("flush_until");
        assert_eq!(aof.flush_metrics().backlog_bytes, 0);
    }

    #[test]
    fn flush_until_blocks_until_durable() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");
        let aof = Aof::new(rt.clone(), test_config(tmp.path())).expect("aof");

        let record_id = aof.append_record(b"flush-me").expect("append");
        let segment = aof
            .segment_by_id(SegmentId::new(record_id.segment_index() as u64))
            .expect("segment");
        assert!(segment.durable_size() < segment.current_size());

        aof.flush_until(record_id).expect("flush_until");

        assert_eq!(segment.durable_size(), segment.current_size());
    }

    #[test]
    fn flush_until_handles_flush_queue_backpressure() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = test_config(tmp.path());
        cfg.flush.flush_watermark_bytes = u64::MAX;
        cfg.flush.flush_interval_ms = 0;
        cfg.flush.max_unflushed_bytes = u64::MAX;
        let aof = Aof::new(rt.clone(), cfg).expect("aof");

        let record_id = aof.append_record(b"fallback").expect("append");
        let segment = aof
            .segment_by_id(SegmentId::new(record_id.segment_index() as u64))
            .expect("segment");
        assert!(segment.durable_size() < segment.current_size());

        let baseline = aof.flush_metrics();
        assert!(baseline.backlog_bytes > 0);
        aof.flush.shutdown_worker_for_tests();
        thread::sleep(std::time::Duration::from_millis(50));

        aof.flush_until(record_id).expect("flush fallback");

        let snapshot = aof.flush_metrics();
        assert_eq!(segment.durable_size(), segment.current_size());
        assert_eq!(
            snapshot.synchronous_flushes,
            baseline.synchronous_flushes + 1
        );
        assert_eq!(snapshot.backlog_bytes, 0);
    }

    #[test]
    fn open_reader_exposes_sealed_segment() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = test_config(tmp.path());
        cfg.segment_min_bytes = 512;
        cfg.segment_max_bytes = 512;
        cfg.segment_target_bytes = 512;
        let aof = Aof::new(rt.clone(), cfg).expect("aof");
        let tail_rx = aof.tail_events();

        let record_id = aof.append_record(b"reader payload").expect("append");
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let segment = aof.segment_by_id(segment_id).expect("segment");

        aof.flush.shutdown_worker_for_tests();
        aof.handle_segment_full(&segment).expect("handle full");

        let logical = segment.current_size();
        let delta = segment.mark_durable(logical);
        if delta > 0 {
            aof.append.sub_unflushed(delta as u64);
        }

        let sealed_at = current_timestamp_nanos();
        segment.seal(sealed_at).expect("seal");
        aof.segment_finalized(segment_id).expect("finalized");

        let tail_state = tail_rx.borrow().clone();
        assert!(matches!(tail_state.last_event, TailEvent::Sealed(id) if id == segment_id));

        let reader = aof.open_reader(segment_id).expect("open reader");
        assert!(reader.contains(record_id));
        let record = reader.read_record(record_id).expect("read record");
        assert_eq!(record.payload(), b"reader payload");

        let snapshots = aof.segment_snapshot();
        let sealed_snapshot = snapshots
            .into_iter()
            .find(|snapshot| snapshot.segment_id == segment_id)
            .expect("sealed snapshot present");
        assert!(sealed_snapshot.sealed);
        assert!(sealed_snapshot.current_size >= record.bounds().end());
    }

    #[test]
    fn flush_until_completes_after_recovery() {
        let rt = Arc::new(Runtime::new().expect("runtime"));
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = test_config(tmp.path());
        cfg.flush.flush_watermark_bytes = 1;
        cfg.flush.flush_interval_ms = 0;
        cfg.flush.max_unflushed_bytes = u64::MAX;

        {
            let aof = Aof::new(rt.clone(), cfg.clone()).expect("aof");
            aof.append_record(b"pre-restart").expect("append");
            aof.flush.shutdown_worker_for_tests();
        }

        let recovered = Aof::new(rt.clone(), cfg).expect("recovered");
        let record_id = recovered
            .append_record(b"post-restart")
            .expect("append after restart");
        let segment = recovered
            .segment_by_id(SegmentId::new(record_id.segment_index() as u64))
            .expect("segment");
        assert!(segment.durable_size() < segment.current_size());

        let before = recovered.flush_metrics();
        recovered.flush_until(record_id).expect("flush_until");
        let after = recovered.flush_metrics();

        assert_eq!(segment.durable_size(), segment.current_size());
        assert!(after.asynchronous_flushes >= before.asynchronous_flushes + 1);
        assert_eq!(after.backlog_bytes, 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tail_follower_reports_sealed_segment() {
        let runtime = Arc::new(Runtime::new().expect("runtime"));
        let runtime_for_drop = runtime.clone();
        let tmp = TempDir::new().expect("tempdir");

        let mut cfg = test_config(tmp.path());
        cfg.segment_min_bytes = 512;
        cfg.segment_max_bytes = 512;
        cfg.segment_target_bytes = 512;

        let aof = Aof::new(runtime.clone(), cfg).expect("aof");
        let mut follower = aof.tail_follower();

        let record_id = aof.append_record(b"tail-follower").expect("append");
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let segment = aof.segment_by_id(segment_id).expect("segment");

        aof.flush.shutdown_worker_for_tests();
        aof.handle_segment_full(&segment).expect("handle full");

        let logical = segment.current_size();
        let delta = segment.mark_durable(logical);
        if delta > 0 {
            aof.append.sub_unflushed(delta as u64);
        }

        segment.seal(current_timestamp_nanos()).expect("seal");
        aof.segment_finalized(segment_id).expect("finalized");

        let sealed_id = timeout(Duration::from_secs(1), follower.next_sealed_segment())
            .await
            .expect("tail follower timed out")
            .expect("sealed segment");
        assert_eq!(sealed_id, segment_id);

        let reader = aof.open_reader(sealed_id).expect("open reader");
        let record = reader.read_record(record_id).expect("read record");
        assert_eq!(record.payload(), b"tail-follower");

        drop(reader);
        drop(segment);
        drop(aof);
        drop(runtime);

        tokio::task::spawn_blocking(move || drop(runtime_for_drop))
            .await
            .expect("drop runtime");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tail_notifications_follow_segment_lifecycle() {
        let runtime = Arc::new(Runtime::new().expect("runtime"));
        let runtime_for_drop = runtime.clone();
        let tmp = TempDir::new().expect("tempdir");

        let mut cfg = test_config(tmp.path());
        cfg.segment_min_bytes = 256;
        cfg.segment_max_bytes = 256;
        cfg.segment_target_bytes = 256;

        let aof = Aof::new(runtime.clone(), cfg).expect("aof");
        let mut rx = aof.tail_events();

        assert!(rx.borrow().tail.is_none());

        let initial_segment = aof
            .try_get_writable_segment()
            .expect("segment")
            .expect("initial segment");

        let activated = wait_for_event(&mut rx).await;
        match activated.last_event {
            TailEvent::Activated(id) => assert_eq!(id, initial_segment.id()),
            other => panic!("expected activation event, saw {:?}", other),
        }
        let tail_snapshot = activated
            .tail
            .as_ref()
            .expect("tail snapshot after activation");
        assert_eq!(tail_snapshot.segment_id, initial_segment.id());

        aof.append_record(b"payload").expect("append record");

        aof.handle_segment_full(&initial_segment)
            .expect("enqueue sealing");

        let sealing = wait_for_event(&mut rx).await;
        match sealing.last_event {
            TailEvent::Sealing(id) => assert_eq!(id, initial_segment.id()),
            other => panic!("expected sealing event, saw {:?}", other),
        }
        assert!(sealing.tail.is_none());

        let delta = initial_segment.mark_durable(initial_segment.current_size());
        if delta > 0 {
            aof.append.sub_unflushed(delta as u64);
        }
        initial_segment
            .seal(current_timestamp_nanos())
            .expect("seal segment");
        aof.segment_finalized(initial_segment.id())
            .expect("segment finalized");

        let sealed = wait_for_event(&mut rx).await;
        match sealed.last_event {
            TailEvent::Sealed(id) => assert_eq!(id, initial_segment.id()),
            other => panic!("expected sealed event, saw {:?}", other),
        }
        assert!(sealed.tail.is_none());

        let next_segment = aof
            .try_get_writable_segment()
            .expect("segment")
            .expect("next segment");

        let next = wait_for_event(&mut rx).await;
        match next.last_event {
            TailEvent::Activated(id) => assert_eq!(id, next_segment.id()),
            other => panic!("expected activation for next segment, saw {:?}", other),
        }
        let new_tail = next
            .tail
            .as_ref()
            .expect("new tail snapshot after activation");
        assert_eq!(new_tail.segment_id, next_segment.id());
        assert_ne!(new_tail.segment_id, initial_segment.id());

        drop(next_segment);
        drop(initial_segment);
        drop(aof);
        drop(runtime);

        tokio::task::spawn_blocking(move || drop(runtime_for_drop))
            .await
            .expect("drop runtime");
    }
}
