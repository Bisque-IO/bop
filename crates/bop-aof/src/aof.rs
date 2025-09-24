use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex as StdMutex};
use std::thread;
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::{
    runtime::Handle,
    sync::{Notify, Semaphore, watch},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};

use crate::config::{AofConfig, RecordId, SegmentId};
use crate::error::{AofError, AofResult, BackpressureKind};
use crate::flush::{
    self, FlushManager, FlushMetrics, FlushMetricsSnapshot, FlushRequest, flush_with_retry,
    persist_metadata_with_retry,
};
use crate::fs::{CURRENT_POINTER_RESERVED_BYTES, CurrentSealedPointer, Layout, SegmentFileName};
#[cfg(test)]
use crate::manager::AofManager;
use crate::manager::{AofManagerHandle, InstanceMetadata, InstanceMetadataHandle};
use crate::reader::{SegmentReader, TailFollower};
use crate::segment::{Segment, SegmentAppendResult, SegmentFooter, SegmentReservation};
#[cfg(test)]
use crate::store::DurabilityEntry;
use crate::store::{
    AdmissionGuard, InstanceId, ResidencyKind, ResidentSegment, RolloverSignal, SegmentCheckout,
    SegmentResidency, TieredCoordinatorNotifiers, TieredInstance,
};

struct AppendState {
    tail: ArcSwapOption<AdmissionGuard>,
    next_offset: AtomicU64,
    record_count: AtomicU64,
    unflushed_bytes: Arc<AtomicU64>,
    metrics: Arc<FlushMetrics>,
}

struct SegmentPreallocator {
    handle: Handle,
    semaphore: Arc<Semaphore>,
    queue: Arc<StdMutex<VecDeque<Arc<Segment>>>>,
    condvar: Arc<Condvar>,
    inflight: AtomicUsize,
}

impl SegmentPreallocator {
    fn new(handle: Handle, max_concurrent: usize) -> Arc<Self> {
        Arc::new(Self {
            handle,
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            queue: Arc::new(StdMutex::new(VecDeque::new())),
            condvar: Arc::new(Condvar::new()),
            inflight: AtomicUsize::new(0),
        })
    }

    fn ready_len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    fn inflight(&self) -> usize {
        self.inflight.load(Ordering::Acquire)
    }

    fn has_inflight(&self) -> bool {
        self.inflight() > 0
    }

    fn schedule(self: &Arc<Self>, job: PreallocationJob) {
        self.inflight.fetch_add(1, Ordering::AcqRel);
        let worker = Arc::clone(self);
        let handle = worker.handle.clone();
        handle.spawn(async move {
            let permit = match worker.semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => {
                    worker.inflight.fetch_sub(1, Ordering::AcqRel);
                    worker.condvar.notify_all();
                    return;
                }
            };

            let segment_id = job.segment_id;
            let result = tokio::task::spawn_blocking(move || job.run()).await;
            drop(permit);

            match result {
                Ok(Ok(segment)) => {
                    let mut guard = worker.queue.lock().unwrap();
                    guard.push_back(segment);
                    worker.condvar.notify_one();
                }
                Ok(Err(err)) => {
                    error!(
                        segment = segment_id.as_u64(),
                        error = %err,
                        "segment preallocation failed"
                    );
                    worker.condvar.notify_all();
                }
                Err(join_err) => {
                    error!(
                        segment = segment_id.as_u64(),
                        error = %join_err,
                        "segment preallocation task panicked"
                    );
                    worker.condvar.notify_all();
                }
            }

            worker.inflight.fetch_sub(1, Ordering::AcqRel);
            worker.condvar.notify_all();
        });
    }

    fn try_take_ready(&self) -> Option<Arc<Segment>> {
        self.queue.lock().unwrap().pop_front()
    }
}

struct PreallocationJob {
    segment_id: SegmentId,
    base_offset: u64,
    base_record_count: u64,
    created_at: i64,
    segment_bytes: u64,
    path: PathBuf,
    ext_id: u64,
}

impl PreallocationJob {
    fn run(self) -> AofResult<Arc<Segment>> {
        let segment = Arc::new(Segment::create_active(
            self.segment_id,
            self.base_offset,
            self.base_record_count,
            self.created_at,
            self.segment_bytes,
            &self.path,
        )?);
        segment.set_ext_id(self.ext_id)?;
        Ok(segment)
    }
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

pub struct AppendReserve {
    guard: AdmissionGuard,
    previous_offset: u64,
}

impl AppendReserve {
    fn new(guard: AdmissionGuard, previous_offset: u64) -> Self {
        Self {
            guard,
            previous_offset,
        }
    }

    fn previous_offset(&self) -> u64 {
        self.previous_offset
    }

    fn update_previous_offset(&mut self, value: u64) {
        self.previous_offset = value;
    }

    fn guard(&self) -> &AdmissionGuard {
        &self.guard
    }

    fn into_guard(self) -> AdmissionGuard {
        self.guard
    }
}

pub struct AppendReservation {
    reserve: Option<AppendReserve>,
    reservation: Option<SegmentReservation>,
}

impl AppendReservation {
    fn new(reserve: AppendReserve, reservation: SegmentReservation) -> Self {
        Self {
            reserve: Some(reserve),
            reservation: Some(reservation),
        }
    }

    pub fn payload_len(&self) -> usize {
        self.reservation
            .as_ref()
            .expect("reservation consumed")
            .payload_len()
    }

    pub fn payload_mut(&mut self) -> AofResult<&mut [u8]> {
        self.reservation
            .as_mut()
            .ok_or_else(|| {
                AofError::InvalidState("append reservation already completed".to_string())
            })?
            .payload_mut()
    }

    fn into_parts(mut self) -> AofResult<(AppendReserve, SegmentReservation)> {
        let reserve = self.reserve.take().ok_or_else(|| {
            AofError::InvalidState("append reservation missing reusable handle".to_string())
        })?;
        let reservation = self.reservation.take().ok_or_else(|| {
            AofError::InvalidState("append reservation missing segment allocation".to_string())
        })?;
        Ok((reserve, reservation))
    }
}

impl Drop for AppendReservation {
    fn drop(&mut self) {
        debug_assert!(
            self.reservation.is_none(),
            "append reservation dropped without completion"
        );
    }
}

#[derive(Default)]
struct SegmentCatalog {
    order: VecDeque<SegmentId>,
    entries: HashMap<SegmentId, ResidentSegment>,
}

impl SegmentCatalog {
    fn insert(&mut self, resident: ResidentSegment) {
        let segment_id = resident.segment().id();
        if self.entries.insert(segment_id, resident).is_none() {
            self.order.push_back(segment_id);
        }
    }

    fn get(&self, segment_id: SegmentId) -> Option<&ResidentSegment> {
        self.entries.get(&segment_id)
    }

    fn get_by_segment(&self, segment: &Arc<Segment>) -> Option<&ResidentSegment> {
        self.get(segment.id())
    }

    fn iter(&self) -> impl Iterator<Item = &ResidentSegment> {
        self.order
            .iter()
            .filter_map(|segment_id| self.entries.get(segment_id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SegmentQueueKey {
    instance_id: InstanceId,
    segment_id: SegmentId,
}

impl SegmentQueueKey {
    fn new(instance_id: InstanceId, segment_id: SegmentId) -> Self {
        Self {
            instance_id,
            segment_id,
        }
    }
}

#[derive(Default)]
struct SegmentQueue {
    order: VecDeque<SegmentQueueKey>,
    members: HashSet<SegmentQueueKey>,
}

impl SegmentQueue {
    fn len(&self) -> usize {
        self.members.len()
    }

    fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    fn push_back(&mut self, instance_id: InstanceId, segment_id: SegmentId) -> bool {
        let key = SegmentQueueKey::new(instance_id, segment_id);
        if self.members.insert(key) {
            self.order.push_back(key);
            true
        } else {
            false
        }
    }

    fn remove(&mut self, instance_id: InstanceId, segment_id: SegmentId) -> bool {
        let key = SegmentQueueKey::new(instance_id, segment_id);
        self.members.remove(&key)
    }

    fn iter_ids(&self) -> impl Iterator<Item = SegmentId> + '_ {
        self.order
            .iter()
            .filter_map(|key| self.members.contains(key).then_some(key.segment_id))
    }

    fn collect_residents(&self, catalog: &SegmentCatalog) -> Vec<ResidentSegment> {
        self.iter_ids()
            .filter_map(|segment_id| catalog.get(segment_id))
            .cloned()
            .collect()
    }
}

#[derive(Default)]
struct RolloverTracker {
    entries: HashMap<SegmentId, RolloverAwaiter>,
    pending: VecDeque<SegmentId>,
    pending_set: HashSet<SegmentId>,
    ready: VecDeque<SegmentId>,
    ready_set: HashSet<SegmentId>,
}

impl RolloverTracker {
    fn contains(&self, segment_id: SegmentId) -> bool {
        self.entries.contains_key(&segment_id)
    }

    fn insert_pending(&mut self, segment_id: SegmentId, signal: Arc<RolloverSignal>) {
        self.entries
            .insert(segment_id, RolloverAwaiter::Pending(signal));
        if self.pending_set.insert(segment_id) {
            self.pending.push_back(segment_id);
        }
        self.ready_set.remove(&segment_id);
    }

    fn remove(&mut self, segment_id: SegmentId) -> Option<RolloverAwaiter> {
        self.pending_set.remove(&segment_id);
        self.ready_set.remove(&segment_id);
        self.entries.remove(&segment_id)
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.pending.clear();
        self.pending_set.clear();
        self.ready.clear();
        self.ready_set.clear();
    }

    fn mark_ready(&mut self, segment_id: SegmentId) {
        if let Some(RolloverAwaiter::Pending(signal)) = self.entries.get(&segment_id) {
            if signal.is_ready() {
                self.entries.insert(segment_id, RolloverAwaiter::Ready);
                self.pending_set.remove(&segment_id);
                if self.ready_set.insert(segment_id) {
                    self.ready.push_back(segment_id);
                }
            }
        }
    }

    fn promote_ready(&mut self, segment_id: SegmentId) {
        self.pending_set.remove(&segment_id);
        if self.ready_set.insert(segment_id) {
            self.ready.push_back(segment_id);
        }
    }

    fn poll_pending(&mut self) -> Option<(SegmentId, Arc<RolloverSignal>)> {
        while let Some(&segment_id) = self.pending.front() {
            if !self.pending_set.contains(&segment_id) {
                self.pending.pop_front();
                continue;
            }
            match self.entries.get(&segment_id) {
                Some(RolloverAwaiter::Pending(signal)) => {
                    return Some((segment_id, Arc::clone(signal)));
                }
                _ => {
                    self.pending.pop_front();
                    self.pending_set.remove(&segment_id);
                }
            }
        }
        None
    }

    fn has_pending(&mut self) -> bool {
        while let Some(&segment_id) = self.pending.front() {
            if !self.pending_set.contains(&segment_id) {
                self.pending.pop_front();
                continue;
            }
            if matches!(
                self.entries.get(&segment_id),
                Some(RolloverAwaiter::Pending(_))
            ) {
                return true;
            }
            self.pending.pop_front();
            self.pending_set.remove(&segment_id);
        }
        false
    }

    fn take_ready(&mut self) -> Option<SegmentId> {
        while let Some(segment_id) = self.ready.pop_front() {
            if !self.ready_set.remove(&segment_id) {
                continue;
            }
            if matches!(self.entries.get(&segment_id), Some(RolloverAwaiter::Ready)) {
                self.entries.remove(&segment_id);
                self.pending_set.remove(&segment_id);
                return Some(segment_id);
            }
        }
        None
    }

    fn keys(&self) -> impl Iterator<Item = SegmentId> + '_ {
        self.entries.keys().copied()
    }

    fn poll_signal_result(&mut self, segment_id: SegmentId) -> Option<Result<(), String>> {
        match self.entries.get_mut(&segment_id) {
            Some(RolloverAwaiter::Pending(signal)) => {
                let result = signal.result();
                if let Some(ref outcome) = result {
                    match outcome {
                        Ok(()) => {
                            self.entries.insert(segment_id, RolloverAwaiter::Ready);
                            self.promote_ready(segment_id);
                        }
                        Err(_) => {
                            self.remove(segment_id);
                        }
                    }
                }
                result
            }
            Some(RolloverAwaiter::Ready) => Some(Ok(())),
            None => Some(Ok(())),
        }
    }

    fn is_ready(&self, segment_id: SegmentId) -> bool {
        matches!(self.entries.get(&segment_id), Some(RolloverAwaiter::Ready))
    }
}

struct AofManagement {
    instance_id: InstanceId,
    catalog: SegmentCatalog,
    pending_finalize: SegmentQueue,
    next_segment_index: u32,
    flush_queue: SegmentQueue,
    rollovers: RolloverTracker,
}

#[derive(Debug)]
enum RolloverAwaiter {
    Pending(Arc<RolloverSignal>),
    Ready,
}

impl AofManagement {
    fn resident_for_segment(&self, segment: &Arc<Segment>) -> Option<ResidentSegment> {
        self.catalog.get_by_segment(segment).cloned()
    }

    fn remove_pending(&mut self, segment_id: SegmentId) -> bool {
        self.pending_finalize.remove(self.instance_id, segment_id)
    }

    fn remove_flush(&mut self, segment_id: SegmentId) -> bool {
        self.flush_queue.remove(self.instance_id, segment_id)
    }
}

/// Lightweight snapshot of a resident segment tracked in the in-memory catalog.
#[derive(Debug, Clone)]
pub struct SegmentCatalogEntry {
    pub segment_id: SegmentId,
    pub sealed: bool,
    pub base_offset: u64,
    pub base_record_count: u64,
    pub current_size: u32,
    pub record_count: u64,
}

/// Detailed status for a segment, used for observability and tooling.
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
    fn from_segment(segment: &Segment) -> Self {
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

/// High-level events describing state transitions for the writable tail segment.
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

/// Lightweight snapshot of the active tail segment shared with followers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TailSegmentState {
    pub segment_id: SegmentId,
    pub durable_size: u32,
    pub sealed: bool,
}

impl TailSegmentState {
    fn from_segment(segment: &Segment) -> Self {
        Self {
            segment_id: segment.id(),
            durable_size: segment.durable_size(),
            sealed: segment.is_sealed(),
        }
    }
}

/// Aggregate tail state broadcast to followers via the watch channel.
#[derive(Debug, Clone, Copy, Default)]
pub struct TailState {
    pub version: u64,
    pub tail: Option<TailSegmentState>,
    pub last_event: TailEvent,
}

struct TailSignal {
    tx: watch::Sender<TailState>,
    state: Mutex<TailState>,
}

impl TailSignal {
    fn new() -> Self {
        let initial = TailState::default();
        let (tx, _rx) = watch::channel(initial);
        Self {
            tx,
            state: Mutex::new(initial),
        }
    }

    fn subscribe(&self) -> watch::Receiver<TailState> {
        self.tx.subscribe()
    }

    fn replace(&self, tail: Option<TailSegmentState>, event: TailEvent) {
        let mut state = self.state.lock();
        state.tail = tail;
        state.last_event = event;
        state.version = state.version.wrapping_add(1);
        let snapshot = *state;
        let _ = self.tx.send(snapshot);
    }
}

/// Append-only log backed by the tiered store and asynchronous flush pipeline.
pub struct Aof {
    manager: Arc<AofManagerHandle>,
    config: AofConfig,
    layout: Layout,
    tier: TieredInstance,
    instance_id: InstanceId,
    notifiers: Arc<TieredCoordinatorNotifiers>,
    admission_notify: Arc<Notify>,
    rollover_notify: Arc<Notify>,
    append: AppendState,
    management: Mutex<AofManagement>,
    metrics: Arc<FlushMetrics>,
    flush_failed: Arc<AtomicBool>,
    flush: Arc<FlushManager>,
    preallocator: Option<Arc<SegmentPreallocator>>,
    metadata: Arc<InstanceMetadata>,
    tail_signal: TailSignal,
}

impl Aof {
    /// Opens or recovers an AOF instance backed by the shared manager runtime.
    pub fn new(manager: Arc<AofManagerHandle>, config: AofConfig) -> AofResult<Self> {
        let normalized = config.normalized();
        let layout = Layout::new(&normalized);
        layout.ensure()?;
        let instance_name = format!("aof:{}", layout.root_dir().display());
        let tier = manager
            .tiered()
            .register_instance(instance_name, layout.clone(), None)?;
        let instance_id = tier.instance_id();
        let notifiers = tier.notifiers();
        let admission_notify = notifiers.admission(instance_id);
        let rollover_notify = notifiers.rollover(instance_id);
        let metrics = Arc::new(FlushMetrics::default());
        let append = AppendState::new(metrics.clone());
        let flush_failed = Arc::new(AtomicBool::new(false));
        let flush = FlushManager::new(
            manager.runtime(),
            append.unflushed_handle(),
            metrics.clone(),
            flush_failed.clone(),
            normalized.flush.max_concurrent_flushes,
        );
        let preallocator = if normalized.preallocate_segments > 0 {
            Some(SegmentPreallocator::new(
                manager.runtime_handle(),
                usize::from(normalized.preallocate_concurrency.max(1)),
            ))
        } else {
            None
        };
        let metadata = manager.register_instance_metadata(instance_id);
        let instance = Self {
            manager,
            config: normalized,
            layout,
            tier,
            instance_id,
            notifiers,
            admission_notify,
            rollover_notify,
            append,
            management: Mutex::new(AofManagement {
                instance_id,
                catalog: SegmentCatalog::default(),
                pending_finalize: SegmentQueue::default(),
                next_segment_index: 0,
                flush_queue: SegmentQueue::default(),
                rollovers: RolloverTracker::default(),
            }),
            metrics,
            flush_failed,
            flush,
            preallocator,
            metadata,
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

    /// Returns a point-in-time catalog of resident segments.
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

    /// Captures detailed status for each resident segment.
    pub fn segment_snapshot(&self) -> Vec<SegmentStatusSnapshot> {
        let state = self.management.lock();
        state
            .catalog
            .iter()
            .map(|segment| SegmentStatusSnapshot::from_segment(segment.segment().as_ref()))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn durability_snapshot(&self) -> Vec<(SegmentId, DurabilityEntry)> {
        self.tier.durability_snapshot()
    }

    #[cfg(test)]
    pub(crate) fn flush_failed_for_tests(&self) -> bool {
        self.flush_failed.load(Ordering::Acquire)
    }

    /// Returns a reader for a sealed segment, hydrating it if necessary.
    pub fn open_reader(&self, segment_id: SegmentId) -> AofResult<SegmentReader> {
        if let Some(resident) = {
            let state = self.management.lock();
            state.catalog.get(segment_id).cloned()
        } {
            return SegmentReader::new(resident);
        }

        match self.tier.checkout_sealed_segment(segment_id)? {
            SegmentCheckout::Ready(resident) => SegmentReader::new(resident),
            SegmentCheckout::Pending(_waiter) => {
                self.record_would_block(BackpressureKind::Hydration);
                Err(AofError::would_block(BackpressureKind::Hydration))
            }
        }
    }

    /// Async variant of open_reader that waits for hydration to finish.
    pub async fn open_reader_async(&self, segment_id: SegmentId) -> AofResult<SegmentReader> {
        if let Some(resident) = {
            let state = self.management.lock();
            state.catalog.get(segment_id).cloned()
        } {
            return SegmentReader::new(resident);
        }

        let checkout = self.tier.checkout_sealed_segment(segment_id)?;
        let resident = checkout.wait().await?;
        SegmentReader::new(resident)
    }

    /// Exposes the latest counters and gauges from the flush pipeline.
    pub fn flush_metrics(&self) -> FlushMetricsSnapshot {
        self.metrics.snapshot()
    }

    #[cfg(test)]
    pub(crate) fn flush_manager_for_tests(&self) -> &Arc<FlushManager> {
        &self.flush
    }

    /// Associates the provided external identifier with the active writable segment.
    pub fn set_current_ext_id(&self, ext_id: u64) -> AofResult<()> {
        self.metadata.set_current_ext_id(ext_id);
        if let Some(guard) = self.current_tail() {
            guard.segment().set_ext_id(ext_id)?;
        }
        Ok(())
    }

    /// Produces a shareable handle for manipulating this instance's metadata.
    pub fn metadata_handle(&self) -> InstanceMetadataHandle {
        InstanceMetadataHandle::new(self.metadata.clone())
    }

    /// Subscribes to tail state changes via the watch channel.
    pub fn tail_events(&self) -> watch::Receiver<TailState> {
        self.tail_signal.subscribe()
    }

    /// Creates a TailFollower stream over tail updates.
    pub fn tail_follower(&self) -> TailFollower {
        TailFollower::new(self.tail_signal.subscribe())
    }

    /// Reserves capacity for a new record and returns a handle for in-place payload writes.
    ///
    /// Callers must populate the returned buffer and finish the append via
    /// [`append_reserve_complete`](Self::append_reserve_complete) to publish the record.
    pub fn append_reserve(&self, payload_len: usize) -> AofResult<AppendReservation> {
        if payload_len == 0 {
            return Err(AofError::InvalidState(
                "record payload is empty".to_string(),
            ));
        }

        if self.flush_failed.load(Ordering::Acquire) {
            self.record_would_block(BackpressureKind::Flush);
            return Err(AofError::would_block(BackpressureKind::Flush));
        }

        let timestamp = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        let timestamp_u64 = if timestamp < 0 { 0 } else { timestamp as u64 };

        let guard = match self.try_get_writable_segment()? {
            Some(guard) => guard,
            None => {
                self.record_would_block(BackpressureKind::Admission);
                return Err(AofError::would_block(BackpressureKind::Admission));
            }
        };

        self.append_reserve_with_guard(payload_len, timestamp_u64, guard)
    }

    /// Reserves capacity using a previously returned [`AppendReserve`], allowing zero-copy
    /// appends to reuse the same segment admission guard when it remains active.
    pub fn append_reserve_with(
        &self,
        reserve: AppendReserve,
        payload_len: usize,
    ) -> AofResult<AppendReservation> {
        if payload_len == 0 {
            return Err(AofError::InvalidState(
                "record payload is empty".to_string(),
            ));
        }

        if self.flush_failed.load(Ordering::Acquire) {
            self.record_would_block(BackpressureKind::Flush);
            return Err(AofError::would_block(BackpressureKind::Flush));
        }

        let timestamp = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        let timestamp_u64 = if timestamp < 0 { 0 } else { timestamp as u64 };

        let guard = reserve.into_guard();
        self.append_reserve_with_guard(payload_len, timestamp_u64, guard)
    }

    /// Attempts to append without blocking. When backpressure applies the call returns
    /// `AofError::WouldBlock` or `AofError::FlushBackpressure`, allowing the caller to decide how to proceed.
    pub fn append_record(&self, payload: &[u8]) -> AofResult<RecordId> {
        if payload.is_empty() {
            return Err(AofError::InvalidState(
                "record payload is empty".to_string(),
            ));
        }

        if self.flush_failed.load(Ordering::Acquire) {
            self.record_would_block(BackpressureKind::Flush);
            return Err(AofError::would_block(BackpressureKind::Flush));
        }

        let timestamp = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        let timestamp_u64 = if timestamp < 0 { 0 } else { timestamp as u64 };

        let guard = match self.try_get_writable_segment()? {
            Some(guard) => guard,
            None => {
                self.record_would_block(BackpressureKind::Admission);
                return Err(AofError::would_block(BackpressureKind::Admission));
            }
        };

        self.append_with_guard(payload, timestamp_u64, guard)
    }

    /// Waits up to the provided timeout for admission before returning a backpressure error.
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

        if self.flush_failed.load(Ordering::Acquire) {
            self.record_would_block(BackpressureKind::Flush);
            return Err(AofError::would_block(BackpressureKind::Flush));
        }

        let timestamp = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        let timestamp_u64 = if timestamp < 0 { 0 } else { timestamp as u64 };

        let mut guard = self.wait_for_writable_segment(timeout)?;
        let start = Instant::now();

        loop {
            match self.append_once(payload, timestamp_u64, &guard)? {
                AppendOutcome::Completed(id, full) => {
                    if full {
                        self.handle_segment_full(guard.segment())?;
                    }
                    return Ok(id);
                }
                AppendOutcome::SegmentFull => {
                    self.handle_segment_full(guard.segment())?;
                    let remaining = if let Some(remaining) = timeout.checked_sub(start.elapsed()) {
                        remaining
                    } else {
                        self.record_would_block(BackpressureKind::Admission);
                        return Err(AofError::would_block(BackpressureKind::Admission));
                    };
                    guard = self.wait_for_writable_segment(remaining)?;
                }
            }
        }
    }

    /// Async helper that awaits admission and rollover signals before retrying appends.
    pub async fn append_record_async(&self, payload: &[u8]) -> AofResult<RecordId> {
        let shutdown = self.manager.shutdown_token();
        loop {
            match self.append_record(payload) {
                Ok(id) => return Ok(id),
                Err(AofError::FlushBackpressure {
                    record_id,
                    target_logical,
                    ..
                }) => {
                    self.await_flush(record_id, target_logical, &shutdown)
                        .await?;
                    return Ok(record_id);
                }
                Err(AofError::WouldBlock(kind)) => match kind {
                    BackpressureKind::Admission => {
                        if self.has_pending_rollover() {
                            self.await_rollover(&shutdown).await?;
                        }
                        self.await_admission(&shutdown).await?;
                    }
                    BackpressureKind::Rollover => {
                        self.await_rollover(&shutdown).await?;
                    }
                    other => return Err(AofError::WouldBlock(other)),
                },
                Err(err) => return Err(err),
            }
        }
    }

    /// Completes a previously reserved append, publishing the record and updating durability state.
    ///
    /// Returns both the newly assigned [`RecordId`] and an [`AppendReserve`] that can be reused
    /// to reserve additional payloads on the same segment when space remains.
    pub fn append_reserve_complete(
        &self,
        reservation: AppendReservation,
    ) -> AofResult<(RecordId, AppendReserve)> {
        let (mut reserve, segment_reservation) = reservation.into_parts()?;
        let previous_offset = reserve.previous_offset();
        let segment = Arc::clone(segment_reservation.segment());
        let result = segment_reservation.complete()?;

        self.append
            .next_offset
            .store(result.last_offset, Ordering::Release);
        self.append.record_count.fetch_add(1, Ordering::AcqRel);
        let appended_bytes = result.last_offset.saturating_sub(previous_offset);
        self.append.add_unflushed(appended_bytes);

        reserve.guard().record_append(result.logical_size);
        self.maybe_schedule_flush(&segment, &result)?;
        let record_id = RecordId::from_parts(segment.id().as_u32(), result.segment_offset);

        reserve.update_previous_offset(result.last_offset);

        let backpressure = self.enforce_backpressure(&segment, record_id, result.logical_size);
        if result.is_full {
            self.handle_segment_full(&segment)?;
        }

        match backpressure {
            Ok(()) => Ok((record_id, reserve)),
            Err(err) => Err(err),
        }
    }

    async fn await_flush(
        &self,
        record_id: RecordId,
        target_logical: u32,
        shutdown: &CancellationToken,
    ) -> AofResult<()> {
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let segment = self
            .segment_by_id(segment_id)
            .ok_or(AofError::RecordNotFound(record_id))?;
        let flush_state = segment.flush_state();
        tokio::select! {
            _ = flush_state.wait_for(target_logical) => Ok(()),
            _ = shutdown.cancelled() => {
                self.record_would_block(BackpressureKind::Flush);
                Err(AofError::would_block(BackpressureKind::Flush))
            }
        }
    }

    async fn await_admission(&self, shutdown: &CancellationToken) -> AofResult<()> {
        loop {
            if self.current_tail().is_some() {
                return Ok(());
            }
            if let Some(guard) = self.try_get_writable_segment()? {
                drop(guard);
                return Ok(());
            }
            tokio::select! {
                _ = self.admission_notify.notified() => {},
                _ = shutdown.cancelled() => {
                    self.record_would_block(BackpressureKind::Admission);
                    return Err(AofError::would_block(BackpressureKind::Admission));
                }
            }
        }
    }

    async fn await_rollover(&self, shutdown: &CancellationToken) -> AofResult<()> {
        loop {
            if let Some((segment_id, signal)) = self.pending_rollover_signal() {
                match signal.wait(shutdown).await {
                    Ok(()) => {
                        self.mark_rollover_ready(segment_id);
                        return Ok(());
                    }
                    Err(err) => {
                        self.remove_rollover_entry(segment_id);
                        return Err(err);
                    }
                }
            }

            tokio::select! {
                _ = self.rollover_notify.notified() => {},
                _ = shutdown.cancelled() => {
                    self.record_would_block(BackpressureKind::Rollover);
                    return Err(AofError::would_block(BackpressureKind::Rollover));
                }
            }
        }
    }

    /// Polls until a writable segment becomes available or the timeout elapses.
    pub fn wait_for_writable_segment(&self, timeout: Duration) -> AofResult<AdmissionGuard> {
        if let Some(guard) = self.current_tail() {
            return Ok(guard);
        }

        if timeout.is_zero() {
            self.record_would_block(BackpressureKind::Admission);
            return Err(AofError::would_block(BackpressureKind::Admission));
        }

        let deadline = Instant::now() + timeout;

        let sleep_step = Duration::from_millis(1);

        loop {
            if let Some(guard) = self.current_tail() {
                return Ok(guard);
            }

            let now = Instant::now();

            let remaining = match deadline.checked_duration_since(now) {
                Some(rem) if !rem.is_zero() => rem,
                _ => {
                    self.record_would_block(BackpressureKind::Admission);
                    return Err(AofError::would_block(BackpressureKind::Admission));
                }
            };

            if let Some(mut state) = self.management.try_lock() {
                if let Some(guard) =
                    self.ensure_segment_locked(&mut state, current_timestamp_nanos())?
                {
                    return Ok(guard);
                }
            }

            thread::sleep(sleep_step.min(remaining));
        }
    }

    pub fn seal_active(&self) -> AofResult<Option<SegmentId>> {
        if let Some(segment_id) = self.take_ready_rollover() {
            return Ok(Some(segment_id));
        }

        let guard = match self.current_tail() {
            Some(guard) => guard,
            None => {
                if let Some(segment_id) = self.take_ready_rollover() {
                    return Ok(Some(segment_id));
                }
                if let Some(segment_id) = self.poll_pending_rollovers()? {
                    return Ok(Some(segment_id));
                }
                return Ok(None);
            }
        };
        let segment = guard.segment();
        let segment_id = segment.id();

        if segment.is_sealed() {
            self.queue_rollover(segment)?;
            if self.is_rollover_ack_ready(segment_id)? {
                self.remove_ready_rollover(segment_id);
                return Ok(Some(segment_id));
            }
            self.record_would_block(BackpressureKind::Rollover);
            return Err(AofError::would_block(BackpressureKind::Rollover));
        }

        let previous = self.append.tail.swap(None);
        if let Some(prev) = previous {
            if !Arc::ptr_eq(prev.segment(), segment) {
                self.append.tail.store(Some(prev));
                self.notifiers.notify_admission(self.instance_id);
                self.record_would_block(BackpressureKind::Admission);
                return Err(AofError::would_block(BackpressureKind::Admission));
            }
        }

        let delta = segment.mark_durable(segment.current_size());
        if delta > 0 {
            self.append.sub_unflushed(delta as u64);
        }
        let sealed_at = current_timestamp_nanos();
        let flush_failure = self.flush_failed.load(Ordering::Acquire);
        let footer = segment.seal(sealed_at, flush_failure)?;
        self.update_current_sealed_pointer(segment_id, &footer)?;
        guard.mark_sealed();

        let mut state = self.management.lock();
        state.remove_pending(segment_id);
        state.remove_flush(segment_id);
        drop(state);
        let tail_snapshot = self
            .append
            .tail
            .load_full()
            .map(|guard| TailSegmentState::from_segment(guard.segment().as_ref()));
        self.tail_signal
            .replace(tail_snapshot, TailEvent::Sealed(segment_id));
        self.queue_rollover(segment)?;
        if self.is_rollover_ack_ready(segment_id)? {
            self.remove_ready_rollover(segment_id);
            Ok(Some(segment_id))
        } else {
            self.record_would_block(BackpressureKind::Rollover);
            Err(AofError::would_block(BackpressureKind::Rollover))
        }
    }

    pub fn force_rollover(&self) -> AofResult<Arc<Segment>> {
        let _ = self.seal_active()?;
        let mut state = self.management.lock();
        match self.ensure_segment_locked(&mut state, current_timestamp_nanos())? {
            Some(guard) => Ok(guard.segment().clone()),
            None => {
                self.record_would_block(BackpressureKind::Admission);
                Err(AofError::would_block(BackpressureKind::Admission))
            }
        }
    }

    pub fn wait_for_flush(&self, record_id: RecordId) -> AofResult<()> {
        self.block_on_flush(record_id)
    }

    pub fn flush_until(&self, record_id: RecordId) -> AofResult<()> {
        self.block_on_flush(record_id)
    }

    fn block_on_flush(&self, record_id: RecordId) -> AofResult<()> {
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let segment = self
            .segment_by_id(segment_id)
            .ok_or(AofError::RecordNotFound(record_id))?;

        let flush_state = segment.flush_state();
        let offset = segment
            .segment_offset_for(record_id)
            .ok_or_else(|| AofError::RecordNotFound(record_id))?;
        let target = segment.record_end_offset(offset)?;

        if flush_state.durable_bytes() >= target {
            self.metrics.incr_sync_flush();
            return Ok(());
        }

        self.schedule_flush(&segment)?;
        self.manager
            .runtime_handle()
            .block_on(flush_state.wait_for(target));

        Ok(())
    }

    pub fn segment_finalized(&self, segment_id: SegmentId) -> AofResult<()> {
        let mut state = self.management.lock();
        if !state.remove_pending(segment_id) {
            return Err(AofError::InvalidState(format!(
                "segment {} not pending finalization",
                segment_id
            )));
        }

        state.remove_flush(segment_id);
        let removed_rollover = state.rollovers.remove(segment_id).is_some();

        let tail_matches_segment = self
            .append
            .tail
            .load_full()
            .map(|guard| guard.segment().id() == segment_id)
            .unwrap_or(false);
        if tail_matches_segment {
            self.append.tail.store(None);
        }

        let mut activated_guard = None;
        if self.append.tail.load_full().is_none() {
            if let Some(next_id) = state.pending_finalize.iter_ids().find(|candidate| {
                state
                    .catalog
                    .get(*candidate)
                    .map(|resident| !resident.segment().is_sealed())
                    .unwrap_or(false)
            }) {
                if let Some(resident) = state.catalog.get(next_id) {
                    let guard = self.tier.admission_guard(resident);
                    self.append.tail.store(Some(Arc::new(guard.clone())));
                    activated_guard = Some(guard);
                }
            }
        }

        drop(state);

        if removed_rollover {
            self.notifiers.remove_rollover(self.instance_id, segment_id);
        }

        let tail_snapshot = self
            .append
            .tail
            .load_full()
            .map(|guard| TailSegmentState::from_segment(guard.segment().as_ref()));
        self.tail_signal
            .replace(tail_snapshot, TailEvent::Sealed(segment_id));
        self.notifiers.notify_admission(self.instance_id);

        if let Some(guard) = activated_guard {
            let activated_snapshot = TailSegmentState::from_segment(guard.segment().as_ref());
            self.tail_signal.replace(
                Some(activated_snapshot),
                TailEvent::Activated(guard.segment().id()),
            );
            self.notifiers.notify_admission(self.instance_id);
        }

        Ok(())
    }

    fn append_with_guard(
        &self,
        payload: &[u8],
        timestamp: u64,
        mut guard: AdmissionGuard,
    ) -> AofResult<RecordId> {
        loop {
            match self.append_once(payload, timestamp, &guard)? {
                AppendOutcome::Completed(id, full) => {
                    if full {
                        self.handle_segment_full(guard.segment())?;
                    }
                    return Ok(id);
                }
                AppendOutcome::SegmentFull => {
                    self.handle_segment_full(guard.segment())?;
                    guard = match self.try_get_writable_segment()? {
                        Some(next) => next,
                        None => {
                            self.record_would_block(BackpressureKind::Admission);
                            return Err(AofError::would_block(BackpressureKind::Admission));
                        }
                    };
                }
            }
        }
    }

    fn append_reserve_with_guard(
        &self,
        payload_len: usize,
        timestamp: u64,
        mut guard: AdmissionGuard,
    ) -> AofResult<AppendReservation> {
        loop {
            match self.append_reserve_once(payload_len, timestamp, &guard)? {
                ReserveOutcome::Reserved {
                    reservation,
                    previous_offset,
                } => {
                    let reserve = AppendReserve::new(guard, previous_offset);
                    return Ok(AppendReservation::new(reserve, reservation));
                }
                ReserveOutcome::SegmentFull => {
                    self.handle_segment_full(guard.segment())?;
                    guard = match self.try_get_writable_segment()? {
                        Some(next) => next,
                        None => {
                            self.record_would_block(BackpressureKind::Admission);
                            return Err(AofError::would_block(BackpressureKind::Admission));
                        }
                    };
                }
            }
        }
    }

    fn append_reserve_once(
        &self,
        payload_len: usize,
        timestamp: u64,
        guard: &AdmissionGuard,
    ) -> AofResult<ReserveOutcome> {
        let segment = guard.segment();
        let previous_offset = self.append.next_offset.load(Ordering::Acquire);
        match segment.append_reserve(payload_len, timestamp) {
            Ok(reservation) => Ok(ReserveOutcome::Reserved {
                reservation,
                previous_offset,
            }),
            Err(AofError::SegmentFull(_)) => Ok(ReserveOutcome::SegmentFull),
            Err(err) => Err(err),
        }
    }

    fn append_once(
        &self,
        payload: &[u8],
        timestamp: u64,
        guard: &AdmissionGuard,
    ) -> AofResult<AppendOutcome> {
        let segment = guard.segment();
        let previous_offset = self.append.next_offset.load(Ordering::Acquire);
        match segment.append_record(payload, timestamp) {
            Ok(result) => {
                self.append
                    .next_offset
                    .store(result.last_offset, Ordering::Release);
                self.append.record_count.fetch_add(1, Ordering::AcqRel);
                let appended_bytes = result.last_offset.saturating_sub(previous_offset);
                self.append.add_unflushed(appended_bytes);
                guard.record_append(result.logical_size);
                let record_id = RecordId::from_parts(segment.id().as_u32(), result.segment_offset);
                self.maybe_schedule_flush(segment, &result)?;
                let backpressure =
                    self.enforce_backpressure(segment, record_id, result.logical_size);
                if result.is_full {
                    self.handle_segment_full(segment)?;
                }
                match backpressure {
                    Ok(()) => Ok(AppendOutcome::Completed(record_id, result.is_full)),
                    Err(err) => Err(err),
                }
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

        let target_bytes = flush_state.requested_bytes();
        self.tier
            .record_durability_request(segment.id(), target_bytes);

        let durability = self.tier.durability_handle();

        let request = {
            let mut state = self.management.lock();
            state.remove_flush(segment.id());
            state.resident_for_segment(segment).map(|resident| {
                state.flush_queue.push_back(self.instance_id, segment.id());
                FlushRequest::new(
                    self.instance_id,
                    self.tier.clone(),
                    durability.clone(),
                    resident,
                )
            })
        };

        let Some(request) = request else {
            let result = flush_with_retry(segment, &self.metrics);
            match result {
                Ok(()) => {
                    let requested_after = flush_state.requested_bytes();
                    let durable_bytes = target_bytes.min(requested_after);
                    match persist_metadata_with_retry(
                        &self.tier,
                        &self.metrics,
                        segment.id(),
                        requested_after,
                        durable_bytes,
                    ) {
                        Ok(()) => {
                            let delta = segment.mark_durable(durable_bytes);
                            if delta > 0 {
                                self.append.sub_unflushed(delta as u64);
                            }
                            self.metrics.incr_sync_flush();
                            self.metrics.record_backlog(self.append.total_unflushed());
                            self.flush_failed.store(false, Ordering::Release);
                            flush_state.finish_flush();
                            return Ok(());
                        }
                        Err(err) => {
                            self.metrics.incr_flush_failure();
                            self.flush_failed.store(true, Ordering::Release);
                            self.record_would_block(BackpressureKind::Flush);
                            flush_state.finish_flush();
                            return Err(err);
                        }
                    }
                }
                Err(err) => {
                    self.flush_failed.store(true, Ordering::Release);
                    self.record_would_block(BackpressureKind::Flush);
                    flush_state.finish_flush();
                    return Err(err);
                }
            }
        };

        match self.flush.enqueue_segment(request) {
            Ok(()) => Ok(()),
            Err(AofError::Backpressure) => {
                flush_state.finish_flush();
                {
                    let mut state = self.management.lock();
                    state.remove_flush(segment.id());
                }
                match flush_with_retry(segment, &self.metrics) {
                    Ok(()) => {
                        let requested_after = flush_state.requested_bytes();
                        let durable_bytes = target_bytes.min(requested_after);
                        match persist_metadata_with_retry(
                            &self.tier,
                            &self.metrics,
                            segment.id(),
                            requested_after,
                            durable_bytes,
                        ) {
                            Ok(()) => {
                                let delta = segment.mark_durable(durable_bytes);
                                if delta > 0 {
                                    self.append.sub_unflushed(delta as u64);
                                }
                                self.metrics.incr_sync_flush();
                                self.flush_failed.store(false, Ordering::Release);
                                Ok(())
                            }
                            Err(err) => {
                                self.metrics.incr_flush_failure();
                                self.flush_failed.store(true, Ordering::Release);
                                self.record_would_block(BackpressureKind::Flush);
                                Err(err)
                            }
                        }
                    }
                    Err(err) => {
                        self.flush_failed.store(true, Ordering::Release);
                        self.record_would_block(BackpressureKind::Flush);
                        Err(err)
                    }
                }
            }
            Err(err) => {
                flush_state.finish_flush();
                {
                    let mut state = self.management.lock();
                    state.remove_flush(segment.id());
                }
                self.flush_failed.store(true, Ordering::Release);
                self.record_would_block(BackpressureKind::Flush);
                Err(err)
            }
        }
    }

    pub(crate) fn segment_by_id(&self, segment_id: SegmentId) -> Option<Arc<Segment>> {
        let state = self.management.lock();
        state
            .catalog
            .get(segment_id)
            .map(|segment| segment.segment().clone())
    }

    fn record_would_block(&self, kind: BackpressureKind) {
        self.tier.record_would_block(kind);
    }

    fn update_current_sealed_pointer(
        &self,
        segment_id: SegmentId,
        footer: &SegmentFooter,
    ) -> AofResult<()> {
        let pointer = CurrentSealedPointer {
            segment_id,
            durable_bytes: footer.durable_bytes,
            reserved: [0u8; CURRENT_POINTER_RESERVED_BYTES],
        };
        if let Err(err) = self.layout.store_current_sealed_pointer(pointer) {
            warn!(
                segment = segment_id.as_u64(),
                durable_bytes = footer.durable_bytes,
                "failed to persist current.sealed pointer: {err}"
            );
            return Err(err);
        }
        Ok(())
    }

    fn enforce_backpressure(
        &self,
        segment: &Arc<Segment>,
        record_id: RecordId,
        target_logical: u32,
    ) -> AofResult<()> {
        let limit = self.config.flush.max_unflushed_bytes;
        if limit == 0 {
            return Ok(());
        }
        let backlog = self.append.total_unflushed();
        if backlog <= limit {
            return Ok(());
        }

        self.metrics.incr_backpressure();
        self.record_would_block(BackpressureKind::Flush);
        self.schedule_flush(segment)?;
        Err(AofError::FlushBackpressure {
            record_id,
            pending_bytes: backlog,
            limit,
            target_logical,
        })
    }

    fn try_get_writable_segment(&self) -> AofResult<Option<AdmissionGuard>> {
        let start = Instant::now();
        if let Some(guard) = self.current_tail() {
            self.tier.record_admission_latency(start.elapsed());
            return Ok(Some(guard));
        }

        if let Some(mut state) = self.management.try_lock() {
            let timestamp = current_timestamp_nanos();
            let guard = self.ensure_segment_locked(&mut state, timestamp)?;
            if guard.is_some() {
                self.tier.record_admission_latency(start.elapsed());
            }
            return Ok(guard);
        }

        Ok(None)
    }

    fn has_pending_rollover(&self) -> bool {
        let mut state = self.management.lock();
        state.rollovers.has_pending()
    }

    fn pending_rollover_signal(&self) -> Option<(SegmentId, Arc<RolloverSignal>)> {
        let mut state = self.management.lock();
        state.rollovers.poll_pending()
    }

    fn mark_rollover_ready(&self, segment_id: SegmentId) {
        let mut state = self.management.lock();
        state.rollovers.mark_ready(segment_id);
    }

    fn remove_rollover_entry(&self, segment_id: SegmentId) {
        let mut state = self.management.lock();
        if state.rollovers.remove(segment_id).is_some() {
            self.notifiers.remove_rollover(self.instance_id, segment_id);
        }
    }

    fn current_tail(&self) -> Option<AdmissionGuard> {
        if let Some(guard) = self.append.tail.load_full() {
            if guard.segment().is_sealed() {
                self.append.tail.store(None);
                None
            } else {
                Some(guard.as_ref().clone())
            }
        } else {
            None
        }
    }

    fn ensure_segment_locked(
        &self,
        state: &mut AofManagement,
        created_at: i64,
    ) -> AofResult<Option<AdmissionGuard>> {
        if let Some(guard) = self.current_tail() {
            return Ok(Some(guard));
        }

        let unsealed_pending = state
            .pending_finalize
            .iter_ids()
            .filter_map(|segment_id| state.catalog.get(segment_id))
            .filter(|resident| !resident.segment().is_sealed())
            .count();
        if unsealed_pending >= 2 {
            return Ok(None);
        }

        if state.pending_finalize.len() >= 2 {
            return Ok(None);
        }

        if let Some(preallocator) = &self.preallocator {
            if let Some(segment) = preallocator.try_take_ready() {
                return self.activate_preallocated_segment(state, segment);
            }
            if preallocator.has_inflight() {
                return Ok(None);
            }
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

        segment.set_ext_id(self.metadata.current_ext_id())?;

        let resident = self.tier.admit_segment(
            segment.clone(),
            SegmentResidency::new(segment.current_size() as u64, ResidencyKind::Active),
        )?;
        let guard = self.tier.admission_guard(&resident);

        state.next_segment_index = segment_index.saturating_add(1);
        state.catalog.insert(resident);
        self.append.tail.store(Some(Arc::new(guard.clone())));
        self.notifiers.notify_admission(self.instance_id);
        let tail_snapshot = TailSegmentState::from_segment(guard.segment().as_ref());
        self.tail_signal.replace(
            Some(tail_snapshot),
            TailEvent::Activated(guard.segment().id()),
        );
        Ok(Some(guard))
    }

    fn queue_rollover(&self, segment: &Arc<Segment>) -> AofResult<()> {
        let mut state = self.management.lock();
        let segment_id = segment.id();
        if state.rollovers.contains(segment_id) {
            return Ok(());
        }
        let resident = state
            .resident_for_segment(segment)
            .ok_or_else(|| {
                AofError::InvalidState(format!(
                    "segment {} missing from catalog during rollover",
                    segment_id.as_u64()
                ))
            })?
            .clone();
        let receiver = self.tier.request_rollover(resident)?;
        let signal = self
            .notifiers
            .register_rollover(self.instance_id, segment_id);
        let notifiers = Arc::clone(&self.notifiers);
        let instance_id = self.instance_id;
        let segment_tag = segment_id.as_u64();
        self.manager.runtime_handle().spawn(async move {
            let result = match receiver.await {
                Ok(res) => res,
                Err(_) => Err(AofError::rollover_failed(format!(
                    "rollover ack channel closed for segment {}",
                    segment_tag
                ))),
            };
            notifiers.complete_rollover(instance_id, segment_id, result);
        });
        state.rollovers.insert_pending(segment_id, signal);
        Ok(())
    }

    fn is_rollover_ack_ready(&self, segment_id: SegmentId) -> AofResult<bool> {
        let mut state = self.management.lock();
        match state.rollovers.poll_signal_result(segment_id) {
            Some(Ok(())) => Ok(true),
            Some(Err(err)) => {
                self.notifiers.remove_rollover(self.instance_id, segment_id);
                Err(AofError::rollover_failed(err))
            }
            None => Ok(false),
        }
    }

    fn remove_ready_rollover(&self, segment_id: SegmentId) {
        let mut state = self.management.lock();
        if state.rollovers.is_ready(segment_id) {
            state.rollovers.remove(segment_id);
            self.notifiers.remove_rollover(self.instance_id, segment_id);
        }
    }

    fn take_ready_rollover(&self) -> Option<SegmentId> {
        let mut state = self.management.lock();
        if let Some(id) = state.rollovers.take_ready() {
            self.notifiers.remove_rollover(self.instance_id, id);
            Some(id)
        } else {
            None
        }
    }

    fn poll_pending_rollovers(&self) -> AofResult<Option<SegmentId>> {
        let ids: Vec<_> = {
            let state = self.management.lock();
            state.rollovers.keys().collect()
        };

        for segment_id in ids {
            if self.is_rollover_ack_ready(segment_id)? {
                self.remove_ready_rollover(segment_id);
                return Ok(Some(segment_id));
            }
        }

        Ok(None)
    }

    fn handle_segment_full(&self, segment: &Arc<Segment>) -> AofResult<()> {
        let previous = self.append.tail.swap(None);
        let enqueue = match previous {
            Some(prev) if Arc::ptr_eq(prev.segment(), segment) => true,
            Some(prev) => {
                self.append.tail.store(Some(prev));
                self.notifiers.notify_admission(self.instance_id);
                false
            }
            None => true,
        };

        if enqueue {
            {
                let mut state = self.management.lock();
                if state.resident_for_segment(segment).is_some() {
                    state
                        .pending_finalize
                        .push_back(self.instance_id, segment.id());
                }
                self.maybe_schedule_preallocation_locked(&mut state);
            }
            self.schedule_flush(segment)?;
            let tail_snapshot = self
                .append
                .tail
                .load_full()
                .map(|guard| TailSegmentState::from_segment(guard.segment().as_ref()));
            self.tail_signal
                .replace(tail_snapshot, TailEvent::Sealing(segment.id()));
        }

        if segment.is_sealed() {
            self.queue_rollover(segment)?;
        }
        Ok(())
    }

    fn maybe_schedule_preallocation_locked(&self, state: &mut AofManagement) {
        let Some(preallocator) = &self.preallocator else {
            return;
        };
        let target = self.config.preallocate_segments as usize;
        if target == 0 {
            return;
        }
        if self.append.tail.load_full().is_some() {
            return;
        }

        let unsealed_pending = state
            .pending_finalize
            .iter_ids()
            .filter_map(|segment_id| state.catalog.get(segment_id))
            .filter(|resident| !resident.segment().is_sealed())
            .count();
        if unsealed_pending >= 2 {
            return;
        }

        if state.pending_finalize.len() >= 2 {
            return;
        }
        let ready = preallocator.ready_len();
        let inflight = preallocator.inflight();
        if ready + inflight >= target {
            return;
        }

        let segment_index = state.next_segment_index;
        let segment_id = SegmentId::new(segment_index as u64);
        let base_offset = self.append.next_offset.load(Ordering::Acquire);
        let base_record_count = self.append.record_count.load(Ordering::Acquire);
        let created_at = current_timestamp_nanos();
        let segment_bytes = self.select_segment_bytes(state);
        let file_name = SegmentFileName::format(segment_id, base_offset, created_at);
        let path = self.layout.segment_path(&file_name);

        state.next_segment_index = segment_index.saturating_add(1);

        preallocator.schedule(PreallocationJob {
            segment_id,
            base_offset,
            base_record_count,
            created_at,
            segment_bytes,
            path,
            ext_id: self.metadata.current_ext_id(),
        });
    }

    fn activate_preallocated_segment(
        &self,
        state: &mut AofManagement,
        segment: Arc<Segment>,
    ) -> AofResult<Option<AdmissionGuard>> {
        segment.set_ext_id(self.metadata.current_ext_id())?;
        let resident = self.tier.admit_segment(
            segment.clone(),
            SegmentResidency::new(segment.current_size() as u64, ResidencyKind::Active),
        )?;
        let guard = self.tier.admission_guard(&resident);

        state.catalog.insert(resident);
        self.append.tail.store(Some(Arc::new(guard.clone())));
        self.notifiers.notify_admission(self.instance_id);
        let tail_snapshot = TailSegmentState::from_segment(guard.segment().as_ref());
        self.tail_signal.replace(
            Some(tail_snapshot),
            TailEvent::Activated(guard.segment().id()),
        );
        Ok(Some(guard))
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
        let recovered = self.tier.recover_segments()?;
        if recovered.is_empty() {
            return Ok(());
        }

        let pointer = match self.layout.load_current_sealed_pointer() {
            Ok(pointer) => pointer,
            Err(err) => {
                warn!(
                    path = %self.layout.current_sealed_pointer_path().display(),
                    "failed to load current.sealed pointer: {err}"
                );
                None
            }
        };

        let mut catalog = SegmentCatalog::default();
        let mut pending_finalize = SegmentQueue::default();
        let mut flush_queue = SegmentQueue::default();
        let mut tail: Option<Arc<Segment>> = None;
        let mut next_offset = pointer.as_ref().map(|p| p.durable_bytes).unwrap_or(0);
        let mut total_records = 0u64;
        let mut next_segment_index = 0u32;
        let mut unsealed_count = 0usize;
        let mut total_unflushed = 0u64;
        let mut pointer_matched = pointer.is_none();

        for entry in recovered {
            let resident = entry.resident().clone();
            let segment = entry.segment().clone();
            let segment_id = segment.id();
            let next_index = segment_id.as_u32().saturating_add(1);
            if next_index > next_segment_index {
                next_segment_index = next_index;
            }

            let segment_end_offset = segment
                .base_offset()
                .saturating_add(segment.durable_size() as u64);
            next_offset = next_offset.max(segment_end_offset);
            if let Some(ptr) = pointer.as_ref() {
                if ptr.segment_id == segment_id {
                    pointer_matched = true;
                    if segment_end_offset != ptr.durable_bytes {
                        warn!(
                            segment = ptr.segment_id.as_u64(),
                            pointer_durable = ptr.durable_bytes,
                            derived_durable = segment_end_offset,
                            "current.sealed pointer durable bytes mismatch during recovery"
                        );
                    }
                }
            }
            total_records = segment
                .base_record_count()
                .saturating_add(segment.record_count());

            let logical = segment.current_size() as u64;
            let durable = segment.durable_size() as u64;
            total_unflushed = total_unflushed.saturating_add(logical.saturating_sub(durable));

            if !segment.is_sealed() {
                unsealed_count += 1;
                if unsealed_count > 2 {
                    return Err(AofError::Corruption(format!(
                        "recovery detected {} unsealed segments; expected at most 2",
                        unsealed_count
                    )));
                }
                if tail.is_some() {
                    pending_finalize.push_back(self.instance_id, segment_id);
                    flush_queue.push_back(self.instance_id, segment_id);
                } else {
                    flush_queue.push_back(self.instance_id, segment_id);
                    tail = Some(segment.clone());
                }
            }

            catalog.insert(resident);
        }
        if let Some(ptr) = pointer.as_ref() {
            if !pointer_matched {
                warn!(
                    segment = ptr.segment_id.as_u64(),
                    "current.sealed pointer segment missing from recovered catalog"
                );
            }
        }

        let tail_guard_arc = tail.as_ref().and_then(|segment| {
            catalog
                .get_by_segment(segment)
                .map(|resident| Arc::new(self.tier.admission_guard(resident)))
        });

        let tail_snapshot = tail_guard_arc
            .as_ref()
            .map(|guard| TailSegmentState::from_segment(guard.segment().as_ref()));
        let next_pending = pending_finalize.iter_ids().next();
        let event = if let Some(snapshot) = tail_snapshot.as_ref() {
            TailEvent::Activated(snapshot.segment_id)
        } else if let Some(segment_id) = next_pending {
            TailEvent::Sealing(segment_id)
        } else {
            TailEvent::None
        };

        let pending_for_flush: Vec<ResidentSegment>;
        {
            let mut state = self.management.lock();
            state.catalog = catalog;
            state.pending_finalize = pending_finalize;
            state.next_segment_index = next_segment_index;
            state.flush_queue = flush_queue;
            state.rollovers.clear();
            pending_for_flush = state.flush_queue.collect_residents(&state.catalog);
        }

        self.append.set_unflushed(total_unflushed);
        self.append
            .next_offset
            .store(next_offset, Ordering::Release);
        self.append
            .record_count
            .store(total_records, Ordering::Release);
        self.append.tail.store(tail_guard_arc.clone());
        if tail_guard_arc.is_some() {
            self.notifiers.notify_admission(self.instance_id);
        }

        self.tail_signal.replace(tail_snapshot, event);

        if let Some(segment) = tail {
            self.metadata.set_current_ext_id(segment.ext_id());
        } else {
            self.metadata.set_current_ext_id(0);
        }
        self.flush_failed.store(false, Ordering::Release);

        let durability = self.tier.durability_handle();
        for resident in pending_for_flush {
            let request = FlushRequest::new(
                self.instance_id,
                self.tier.clone(),
                durability.clone(),
                resident,
            );
            self.flush.enqueue_segment(request)?;
        }

        Ok(())
    }
}

impl Drop for Aof {
    fn drop(&mut self) {
        self.manager.unregister_instance_metadata(self.instance_id);
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

enum ReserveOutcome {
    Reserved {
        reservation: SegmentReservation,
        previous_offset: u64,
    },
    SegmentFull,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{AofError, BackpressureKind};
    use crate::{AofManagerConfig, FlushMetricSample, FlushMetricsExporter};
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
            flush_failures: 8,
            metadata_retry_attempts: 9,
            metadata_retry_failures: 10,
            metadata_failures: 11,
            backlog_bytes: 12,
        };
        let exporter = FlushMetricsExporter::new(snapshot);
        assert_eq!(exporter.retry_attempts(), 6);
        assert_eq!(exporter.retry_failures(), 7);
        assert_eq!(exporter.flush_failures(), 8);
        assert_eq!(exporter.metadata_retry_attempts(), 9);
        assert_eq!(exporter.metadata_retry_failures(), 10);
        assert_eq!(exporter.metadata_failures(), 11);
        assert_eq!(exporter.backlog_bytes(), 12);

        let mut samples: Vec<FlushMetricSample> = exporter.samples().collect();
        samples.sort_by(|a, b| a.name.cmp(b.name));
        assert_eq!(samples.len(), 7);
        assert_eq!(
            samples,
            vec![
                FlushMetricSample {
                    name: "aof_flush_backlog_bytes",
                    value: 12
                },
                FlushMetricSample {
                    name: "aof_flush_failures_total",
                    value: 8
                },
                FlushMetricSample {
                    name: "aof_flush_metadata_failures_total",
                    value: 11
                },
                FlushMetricSample {
                    name: "aof_flush_metadata_retry_attempts_total",
                    value: 9
                },
                FlushMetricSample {
                    name: "aof_flush_metadata_retry_failures_total",
                    value: 10
                },
                FlushMetricSample {
                    name: "aof_flush_retry_attempts_total",
                    value: 6
                },
                FlushMetricSample {
                    name: "aof_flush_retry_failures_total",
                    value: 7
                },
            ]
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
        cfg.preallocate_segments = 0;
        cfg.preallocate_concurrency = 0;
        cfg
    }

    fn build_manager(config: AofManagerConfig) -> (AofManager, Arc<AofManagerHandle>) {
        let manager = AofManager::with_config(config).expect("create manager");
        let handle = manager.handle();
        (manager, handle)
    }

    async fn drain_rollover_async(manager: &AofManager) {
        manager.tiered().poll().await;
    }

    async fn seal_active_until_ready_async(
        aof: &Aof,
        manager: &AofManager,
    ) -> AofResult<Option<SegmentId>> {
        loop {
            match aof.seal_active() {
                Ok(Some(id)) => return Ok(Some(id)),
                Ok(None) => {
                    if let Some(id) = aof.poll_pending_rollovers()? {
                        return Ok(Some(id));
                    }
                    drain_rollover_async(manager).await;
                }
                Err(AofError::WouldBlock(BackpressureKind::Rollover)) => {
                    drain_rollover_async(manager).await;
                }
                Err(err) => return Err(err),
            }
        }
    }

    fn seal_active_until_ready(aof: &Aof, manager: &AofManager) -> AofResult<Option<SegmentId>> {
        manager
            .runtime_handle()
            .block_on(seal_active_until_ready_async(aof, manager))
    }

    #[test]
    fn ensure_active_segment_respects_pending_limit() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 64 * 1024;
        cfg.segment_max_bytes = 64 * 1024;
        cfg.segment_target_bytes = 64 * 1024;
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");

        let mut state = aof.management.lock();
        let created_at = 0;
        let guard1 = aof
            .ensure_segment_locked(&mut state, created_at)
            .unwrap()
            .unwrap();
        let seg1 = guard1.segment().clone();
        aof.append.tail.store(None);
        let _res1 = state.resident_for_segment(&seg1).expect("resident seg1");
        state.pending_finalize.push_back(aof.instance_id, seg1.id());
        let guard2 = aof
            .ensure_segment_locked(&mut state, created_at)
            .unwrap()
            .unwrap();
        let seg2 = guard2.segment().clone();
        aof.append.tail.store(None);
        let _res2 = state.resident_for_segment(&seg2).expect("resident seg2");
        state.pending_finalize.push_back(aof.instance_id, seg2.id());
        drop(state);
        drop(guard1);
        drop(guard2);

        assert!(matches!(aof.try_get_writable_segment(), Ok(None)));

        aof.segment_finalized(seg1.id()).expect("finalized");
        assert!(aof.try_get_writable_segment().unwrap().is_some());
        drop(manager);
    }

    #[test]
    fn seal_active_seals_tail_segment() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");

        let guard = aof.try_get_writable_segment().expect("segment");
        let guard = guard.expect("initial guard");
        let segment = guard.segment().clone();
        drop(guard);

        aof.append_record(b"payload").expect("append");

        let sealed_id = seal_active_until_ready(&aof, &manager)
            .expect("seal")
            .expect("segment sealed");
        assert_eq!(sealed_id, segment.id());
        assert!(segment.is_sealed());

        let snapshot = aof.catalog_snapshot();
        assert_eq!(snapshot.len(), 1);
        let entry = &snapshot[0];
        assert_eq!(entry.segment_id, segment.id());
        assert!(entry.sealed);

        drop(manager);
    }

    #[test]
    fn force_rollover_allocates_new_segment() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");

        let initial_guard = aof
            .try_get_writable_segment()
            .expect("segment")
            .expect("initial guard");
        let initial = initial_guard.segment().clone();
        drop(initial_guard);
        aof.append_record(b"payload").expect("append");

        let next = {
            match aof.force_rollover() {
                Err(AofError::WouldBlock(kind)) => {
                    assert_eq!(kind, BackpressureKind::Rollover);
                }
                Ok(_) => panic!("expected rollover backpressure, got success"),
                Err(other) => panic!("unexpected error: {other:?}"),
            }

            let coordinator = manager.tiered();
            let _ = manager
                .runtime_handle()
                .block_on(async { coordinator.poll().await });

            aof.force_rollover().expect("rollover acked")
        };
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

        drop(manager);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn seal_active_requires_rollover_ack() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");

        aof.append_record(b"payload").expect("append");
        let err = aof
            .seal_active()
            .expect_err("seal should apply rollover backpressure");
        assert!(matches!(
            err,
            AofError::WouldBlock(kind) if kind == BackpressureKind::Rollover
        ));

        let sealed_id = seal_active_until_ready_async(&aof, &manager)
            .await
            .expect("seal after ack")
            .expect("sealed");
        assert_eq!(sealed_id.as_u64(), 0);

        drop(manager);
    }

    #[test]
    fn append_surfaces_admission_would_block_until_guard_released() {
        let tmp = TempDir::new().expect("tempdir");

        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 128;
        cfg.segment_max_bytes = 128;
        cfg.segment_target_bytes = 128;
        cfg.preallocate_segments = 0;
        cfg.preallocate_concurrency = 0;

        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");
        aof.flush.shutdown_worker_for_tests();

        let first_guard = aof
            .try_get_writable_segment()
            .expect("initial guard result")
            .expect("initial guard");
        let first_segment = first_guard.segment().clone();
        drop(first_guard);
        aof.handle_segment_full(&first_segment)
            .expect("mark first segment full");

        let second_guard = aof
            .try_get_writable_segment()
            .expect("second guard result")
            .expect("second guard");
        let second_segment = second_guard.segment().clone();
        drop(second_guard);
        aof.handle_segment_full(&second_segment)
            .expect("mark second segment full");

        match aof.append_record(b"payload") {
            Err(AofError::WouldBlock(kind)) => {
                assert_eq!(kind, BackpressureKind::Admission);
            }
            Err(err) => panic!("unexpected append failure: {err:?}"),
            Ok(record) => panic!(
                "expected admission would block but append succeeded with {:?}",
                record
            ),
        }

        for segment in [&first_segment, &second_segment] {
            aof.segment_finalized(segment.id())
                .expect("segment finalized");
        }

        let record_id = aof
            .append_record(b"tail-release")
            .expect("append after releasing admission guard");
        assert_eq!(record_id.segment_index() as u64, 2);
    }

    #[test]
    fn append_reserve_supports_zero_copy_payloads() {
        let tmp = TempDir::new().expect("tempdir");

        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        cfg.preallocate_segments = 0;
        cfg.preallocate_concurrency = 0;

        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");
        aof.flush.shutdown_worker_for_tests();

        let payload = b"reserve me".to_vec();
        let mut reservation = aof
            .append_reserve(payload.len())
            .expect("reservation acquired");
        reservation
            .payload_mut()
            .expect("payload buffer")
            .copy_from_slice(&payload);

        let (record_id, _reserve) = aof
            .append_reserve_complete(reservation)
            .expect("reservation completed");

        let guard = aof
            .try_get_writable_segment()
            .expect("guard result")
            .expect("guard");
        let segment = guard.segment().clone();
        drop(guard);

        let segment_offset = segment
            .segment_offset_for(record_id)
            .expect("segment offset");
        let record = segment
            .read_record_slice(segment_offset)
            .expect("record slice");

        assert_eq!(record.header.length as usize, payload.len());
        assert_eq!(record.payload, payload.as_slice());
    }

    #[test]
    fn append_reserve_reuses_handle_for_same_segment() {
        let tmp = TempDir::new().expect("tempdir");

        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;

        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");
        aof.flush.shutdown_worker_for_tests();

        let mut reservation = aof.append_reserve(8).expect("initial reservation");
        reservation
            .payload_mut()
            .expect("payload")
            .copy_from_slice(b"payload1");
        let (first_id, reserve) = aof
            .append_reserve_complete(reservation)
            .expect("complete first");

        let mut reservation = aof
            .append_reserve_with(reserve, 8)
            .expect("reuse reservation");
        reservation
            .payload_mut()
            .expect("payload")
            .copy_from_slice(b"payload2");
        let (second_id, reserve) = aof
            .append_reserve_complete(reservation)
            .expect("complete second");

        assert_eq!(first_id.segment_index(), second_id.segment_index());

        let mut reservation = aof
            .append_reserve_with(reserve, 8)
            .expect("reuse reservation again");
        reservation
            .payload_mut()
            .expect("payload")
            .copy_from_slice(b"payload3");
        let (third_id, _reserve) = aof
            .append_reserve_complete(reservation)
            .expect("complete third");

        assert_eq!(third_id.segment_index(), first_id.segment_index());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn append_record_async_waits_for_admission_release() {
        use std::time::Duration;
        use tokio::time::sleep;

        let tmp = TempDir::new().expect("tempdir");

        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 128;
        cfg.segment_max_bytes = 128;
        cfg.segment_target_bytes = 128;

        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");
        aof.flush.shutdown_worker_for_tests();

        let mut state = aof.management.lock();
        let created_at = current_timestamp_nanos();
        let guard1 = aof
            .ensure_segment_locked(&mut state, created_at)
            .expect("segment")
            .expect("guard1");
        let seg1 = guard1.segment().clone();
        aof.append.tail.store(None);
        let _res1 = state.resident_for_segment(&seg1).expect("resident seg1");
        state.pending_finalize.push_back(aof.instance_id, seg1.id());

        let guard2 = aof
            .ensure_segment_locked(&mut state, created_at)
            .expect("segment")
            .expect("guard2");
        let seg2 = guard2.segment().clone();
        aof.append.tail.store(None);
        let _res2 = state.resident_for_segment(&seg2).expect("resident seg2");
        state.pending_finalize.push_back(aof.instance_id, seg2.id());
        drop(state);
        drop(guard1);
        drop(guard2);

        let payload = b"tail-release".to_vec();
        let append_future = aof.append_record_async(&payload);
        tokio::pin!(append_future);
        tokio::select! {
            res = &mut append_future => panic!("append completed early: {res:?}"),
            _ = sleep(Duration::from_millis(20)) => {}
        }

        aof.segment_finalized(seg1.id()).expect("segment finalized");

        let record_id = append_future
            .await
            .expect("append after releasing admission guard");
        assert_eq!(record_id.segment_index() as u64, seg2.id().as_u64());

        aof.segment_finalized(seg2.id()).expect("segment finalized");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn append_record_async_waits_for_rollover_ack() {
        use tokio::time::sleep;

        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof");
        cfg.segment_min_bytes = 256;
        cfg.segment_max_bytes = 256;
        cfg.segment_target_bytes = 256;
        cfg.preallocate_segments = 0;
        cfg.preallocate_concurrency = 0;
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");
        aof.flush.shutdown_worker_for_tests();

        let first_guard = aof
            .try_get_writable_segment()
            .expect("first guard result")
            .expect("first guard");
        let first_segment = first_guard.segment().clone();
        drop(first_guard);
        aof.handle_segment_full(&first_segment)
            .expect("mark first segment full");

        let second_guard = aof
            .try_get_writable_segment()
            .expect("second guard result")
            .expect("second guard");
        let second_segment = second_guard.segment().clone();
        drop(second_guard);
        aof.handle_segment_full(&second_segment)
            .expect("mark second segment full");

        for segment in [&first_segment, &second_segment] {
            let delta = segment.mark_durable(segment.current_size());
            if delta > 0 {
                aof.append.sub_unflushed(delta as u64);
            }
            if !segment.is_sealed() {
                let _ = segment.seal(current_timestamp_nanos(), false);
            }
        }

        let payload = b"after-rollover".to_vec();
        #[allow(unused_mut)]
        let append_future = aof.append_record_async(&payload);
        tokio::pin!(append_future);
        tokio::select! {
            res = &mut append_future => panic!("append completed early: {res:?}"),
            _ = sleep(Duration::from_millis(20)) => {}
        }

        for segment in [&first_segment, &second_segment] {
            let delta = segment.mark_durable(segment.current_size());
            if delta > 0 {
                aof.append.sub_unflushed(delta as u64);
            }
            if !segment.is_sealed() {
                let _ = segment.seal(current_timestamp_nanos(), false);
            }
            aof.segment_finalized(segment.id())
                .expect("segment finalized");
            aof.notifiers
                .complete_rollover(aof.instance_id, segment.id(), Ok(()));
        }

        let record_id = append_future.await.expect("append after rollover ack");
        assert_eq!(record_id.segment_index() as u64, 2);

        drop(manager);
    }

    #[test]
    fn recovery_reopens_existing_tail_segment() {
        let tmp = TempDir::new().expect("tempdir");
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());

        let aof = Aof::new(handle.clone(), test_config(tmp.path())).expect("initial");
        aof.append_record(b"one").expect("append one");
        let sealed_id = seal_active_until_ready(&aof, &manager)
            .expect("seal")
            .expect("sealed");
        assert_eq!(sealed_id.as_u64(), 0);
        let tail = aof.force_rollover().expect("rollover");
        assert_eq!(tail.id().as_u64(), 1);
        aof.append_record(b"two").expect("append two");
        drop(aof);

        let recovered = Aof::new(handle.clone(), test_config(tmp.path())).expect("recover");
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

        drop(manager);
    }

    #[test]
    fn recovery_with_sealed_segments_allows_new_segment_creation() {
        let tmp = TempDir::new().expect("tempdir");
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());

        {
            let aof = Aof::new(handle.clone(), test_config(tmp.path())).expect("initial");
            aof.append_record(b"only").expect("append only");
            seal_active_until_ready(&aof, &manager)
                .expect("seal")
                .expect("sealed");
        }

        let recovered = Aof::new(handle, test_config(tmp.path())).expect("recover");
        let snapshot = recovered.catalog_snapshot();
        assert_eq!(snapshot.len(), 1);
        assert!(snapshot[0].sealed);

        recovered.append_record(b"new").expect("append new");
        let snapshot_after = recovered.catalog_snapshot();
        assert_eq!(snapshot_after.len(), 2);
        assert!(snapshot_after.iter().any(|entry| !entry.sealed));

        drop(manager);
    }

    #[test]
    fn recovery_truncates_partial_tail_segment() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());

        {
            let aof = Aof::new(handle.clone(), test_config(tmp.path())).expect("initial");
            let tail_guard = aof
                .try_get_writable_segment()
                .expect("segment")
                .expect("tail");
            let segment = tail_guard.segment().clone();
            drop(tail_guard);
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

        let recovered = Aof::new(handle, test_config(tmp.path())).expect("recover");
        let snapshot = recovered.catalog_snapshot();
        assert!(snapshot.iter().any(|entry| !entry.sealed));
        recovered
            .append_record(b"post-truncation")
            .expect("append after recovery");
    }

    #[test]
    fn recovery_seeds_flush_queue_for_unsealed_segments() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());

        {
            let aof = Aof::new(handle.clone(), test_config(tmp.path())).expect("initial");
            aof.append_record(b"queued").expect("append queued");
        }

        let recovered = Aof::new(handle, test_config(tmp.path())).expect("recover");
        let state = recovered.management.lock();
        assert!(
            !state.flush_queue.is_empty(),
            "recovery should seed flush queue for unsealed segments"
        );
    }
    #[test]
    fn flush_metrics_tracks_backlog_bytes() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, test_config(tmp.path())).expect("aof");

        assert_eq!(aof.flush_metrics().backlog_bytes, 0);

        let record_id = aof.append_record(b"metric").expect("append");
        let snapshot = aof.flush_metrics();
        assert!(snapshot.backlog_bytes > 0);

        aof.flush_until(record_id).expect("flush_until");
        assert_eq!(aof.flush_metrics().backlog_bytes, 0);
    }

    #[test]
    fn flush_until_blocks_until_durable() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, test_config(tmp.path())).expect("aof");

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
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = test_config(tmp.path());
        cfg.flush.flush_watermark_bytes = u64::MAX;
        cfg.flush.flush_interval_ms = 0;
        cfg.flush.max_unflushed_bytes = u64::MAX;
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");

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
    fn append_returns_flush_backpressure() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let mut cfg = test_config(tmp.path());
        cfg.flush.flush_watermark_bytes = u64::MAX;
        cfg.flush.flush_interval_ms = 0;
        cfg.flush.max_unflushed_bytes = 1024;
        let aof = Aof::new(handle, cfg).expect("aof");

        let small_payload = vec![0u8; 64];
        let first = aof.append_record(&small_payload).expect("append first");
        let large_payload = vec![0u8; 2048];
        let blocked = match aof.append_record(&large_payload) {
            Err(AofError::FlushBackpressure { record_id, .. }) => record_id,
            other => panic!("expected flush backpressure, got {:?}", other),
        };

        aof.wait_for_flush(blocked).expect("flush blocked");
        aof.wait_for_flush(first).expect("flush first");

        let third = aof
            .append_record(&small_payload)
            .expect("append after flush");
        aof.wait_for_flush(third).expect("flush third");
    }

    #[test]
    fn durability_cursor_tracks_flush_progress() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, test_config(tmp.path())).expect("aof");

        let record_id = aof.append_record(b"durability-progress").expect("append");
        aof.flush_until(record_id).expect("flush_until");

        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let entry = aof
            .durability_snapshot()
            .into_iter()
            .find(|(id, _)| *id == segment_id)
            .map(|(_, entry)| entry)
            .expect("durability entry");
        let segment = aof.segment_by_id(segment_id).expect("segment");
        assert_eq!(entry.requested_bytes, segment.current_size());
        assert_eq!(entry.durable_bytes, segment.durable_size());
    }

    #[test]
    fn durability_cursor_recovers_from_segment_scan() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let config = test_config(tmp.path());

        let (record_id, durability_path) = {
            let aof = Aof::new(handle.clone(), config.clone()).expect("aof");
            let durability_path = aof.layout().warm_dir().join("durability.json");
            let id = aof.append_record(b"after-restart").expect("append");
            aof.flush_until(id).expect("flush");
            (id, durability_path)
        };

        assert!(
            !durability_path.exists(),
            "durability metadata should not be persisted to disk"
        );

        let recovered = Aof::new(handle, config).expect("recovered");
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let entry = recovered
            .durability_snapshot()
            .into_iter()
            .find(|(id, _)| *id == segment_id)
            .map(|(_, entry)| entry)
            .expect("durability entry");
        let snapshot = recovered
            .segment_snapshot()
            .into_iter()
            .find(|segment| segment.segment_id == segment_id)
            .expect("segment snapshot");
        assert_eq!(entry.durable_bytes, snapshot.durable_size);
        assert_eq!(entry.requested_bytes, snapshot.current_size);
        assert!(
            !durability_path.exists(),
            "durability metadata should not appear after recovery"
        );
    }

    #[test]
    fn flush_worker_failure_surfaces_backpressure() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let mut config = test_config(tmp.path());
        config.flush.flush_watermark_bytes = u64::MAX;
        config.flush.flush_interval_ms = 0;
        config.flush.max_unflushed_bytes = u64::MAX;
        let aof = Aof::new(handle.clone(), config).expect("aof");

        let record_id = aof.append_record(b"flush-failure").expect("append");
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let segment = aof.segment_by_id(segment_id).expect("segment");
        segment.inject_flush_error(10);

        aof.schedule_flush(&segment).expect("schedule flush");

        for _ in 0..100 {
            if aof.flush_failed_for_tests() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            aof.flush_failed_for_tests(),
            "expected flush failure flag to set"
        );

        let err = aof.append_record(b"should-block");
        assert!(matches!(
            err,
            Err(AofError::WouldBlock(BackpressureKind::Flush))
        ));

        segment.inject_flush_error(0);
        aof.flush_until(record_id).expect("flush until");
        for _ in 0..100 {
            if !aof.flush_failed_for_tests() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            !aof.flush_failed_for_tests(),
            "expected flush failure flag to clear"
        );

        aof.append_record(b"after-recover")
            .expect("append after recovery");
    }

    #[test]
    fn admission_metrics_track_latency_and_would_block() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle.clone(), test_config(tmp.path())).expect("aof");

        let coordinator = handle.tiered();
        let baseline = coordinator.admission_metrics();

        if let Some(guard) = aof.try_get_writable_segment().expect("guard acquisition") {
            drop(guard);
        }
        let latency_snapshot = coordinator.admission_metrics();
        assert!(latency_snapshot.latency_samples >= baseline.latency_samples + 1);

        {
            let _state = aof.management.lock();
            aof.append.tail.store(None);
            let result = aof.append_record(b"metrics-block");
            assert!(matches!(
                result,
                Err(AofError::WouldBlock(BackpressureKind::Admission))
            ));
        }

        let after = coordinator.admission_metrics();
        assert!(after.would_block_admission >= latency_snapshot.would_block_admission + 1);
    }

    #[test]
    fn open_reader_exposes_sealed_segment() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = test_config(tmp.path());
        cfg.segment_min_bytes = 128;
        cfg.segment_max_bytes = 128;
        cfg.segment_target_bytes = 128;
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");
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
        segment.seal(sealed_at, false).expect("seal");
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
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = test_config(tmp.path());
        cfg.flush.flush_watermark_bytes = 1;
        cfg.flush.flush_interval_ms = 0;
        cfg.flush.max_unflushed_bytes = u64::MAX;
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());

        {
            let aof = Aof::new(handle.clone(), cfg.clone()).expect("aof");
            aof.append_record(b"pre-restart").expect("append");
            aof.flush.shutdown_worker_for_tests();
        }

        let recovered = Aof::new(handle, cfg).expect("recovered");
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
        let flush_progress = after.asynchronous_flushes > before.asynchronous_flushes
            || after.synchronous_flushes > before.synchronous_flushes;
        assert!(
            flush_progress,
            "expected recovery to run at least one flush"
        );
        assert_eq!(after.backlog_bytes, 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tail_follower_reports_sealed_segment() {
        let tmp = TempDir::new().expect("tempdir");

        let mut cfg = test_config(tmp.path());
        cfg.segment_min_bytes = 128;
        cfg.segment_max_bytes = 128;
        cfg.segment_target_bytes = 128;
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());

        let aof = Aof::new(handle, cfg).expect("aof");
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

        segment
            .seal(current_timestamp_nanos(), false)
            .expect("seal");
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
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tail_notifications_follow_segment_lifecycle() {
        let tmp = TempDir::new().expect("tempdir");

        let mut cfg = test_config(tmp.path());
        cfg.segment_min_bytes = 256;
        cfg.segment_max_bytes = 256;
        cfg.segment_target_bytes = 256;
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());

        let aof = Aof::new(handle, cfg).expect("aof");
        let mut rx = aof.tail_events();

        assert!(rx.borrow().tail.is_none());

        let initial_guard = aof
            .try_get_writable_segment()
            .expect("segment")
            .expect("initial guard");
        let initial_segment = initial_guard.segment().clone();
        drop(initial_guard);

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
            .seal(current_timestamp_nanos(), false)
            .expect("seal segment");
        aof.segment_finalized(initial_segment.id())
            .expect("segment finalized");

        let sealed = wait_for_event(&mut rx).await;
        match sealed.last_event {
            TailEvent::Sealed(id) => assert_eq!(id, initial_segment.id()),
            other => panic!("expected sealed event, saw {:?}", other),
        }
        assert!(sealed.tail.is_none());

        let next_guard = aof
            .try_get_writable_segment()
            .expect("segment")
            .expect("next guard");
        let next_segment = next_guard.segment().clone();
        drop(next_guard);

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
    }

    #[test]
    fn pointer_captures_durable_bytes() {
        let tmp = TempDir::new().expect("tempdir");
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let mut cfg = test_config(tmp.path());
        cfg.segment_min_bytes = 512;
        cfg.segment_max_bytes = 512;
        cfg.segment_target_bytes = 512;

        let aof = Aof::new(handle.clone(), cfg.clone()).expect("aof");

        let record_id = aof.append_record(b"pointer-payload").expect("append");
        let sealed_id = seal_active_until_ready(&aof, &manager)
            .expect("seal")
            .expect("sealed segment");
        assert_eq!(sealed_id, SegmentId::new(record_id.segment_index() as u64));

        let pointer = aof
            .layout()
            .load_current_sealed_pointer()
            .expect("load pointer")
            .expect("pointer file present");
        assert_eq!(pointer.segment_id, sealed_id);

        let segment = aof.segment_by_id(sealed_id).expect("sealed segment state");
        let expected_durable = segment.base_offset() + segment.durable_size() as u64;
        assert_eq!(pointer.durable_bytes, expected_durable);
        assert_eq!(pointer.reserved, [0u8; CURRENT_POINTER_RESERVED_BYTES]);

        drop(aof);

        let _restarted = Aof::new(handle.clone(), cfg).expect("restart aof");
    }

    #[test]
    fn pointer_missing_segment_is_ignored_on_recovery() {
        let tmp = TempDir::new().expect("tempdir");
        let layout_cfg = test_config(tmp.path());
        let layout = Layout::new(&layout_cfg);
        layout.ensure().expect("ensure layout");
        layout
            .store_current_sealed_pointer(CurrentSealedPointer {
                segment_id: SegmentId::new(999),
                durable_bytes: 1_024,
                reserved: [0u8; CURRENT_POINTER_RESERVED_BYTES],
            })
            .expect("store pointer");

        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle.clone(), layout_cfg.clone()).expect("recover aof");

        // pointer file remains readable even though it was ignored
        let pointer = aof
            .layout()
            .load_current_sealed_pointer()
            .expect("load pointer")
            .expect("pointer present");
        assert_eq!(pointer.segment_id, SegmentId::new(999));
    }

    #[test]
    fn set_current_ext_id_updates_active_segment() {
        let tmp = TempDir::new().expect("tempdir");
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let cfg = test_config(tmp.path());
        let aof = Aof::new(handle, cfg).expect("aof");

        aof.set_current_ext_id(88).expect("set ext id");
        let record_id = aof.append_record(b"ext-id-record").expect("append");
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let segment = aof.segment_by_id(segment_id).expect("segment");
        assert_eq!(segment.ext_id(), 88);
    }

    #[test]
    fn metadata_handle_updates_runtime_values() {
        let tmp = TempDir::new().expect("tempdir");
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let cfg = test_config(tmp.path());
        let aof = Aof::new(handle, cfg).expect("aof");

        let metadata = aof.metadata_handle();
        metadata.set_current_ext_id(314);

        let record_id = aof.append_record(b"metadata-handle").expect("append");
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let segment = aof.segment_by_id(segment_id).expect("segment");
        assert_eq!(segment.ext_id(), 314);

        let sealed = seal_active_until_ready(&aof, &manager)
            .expect("seal result")
            .expect("segment sealed");
        assert_eq!(sealed, segment_id);
    }
}
