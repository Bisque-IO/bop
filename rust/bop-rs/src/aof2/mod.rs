#[cfg(test)]
use crate::aof2::store::DurabilityEntry;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::aof2::metrics::AdmissionMetricsSnapshot;
use crate::aof2::store::{
    AdmissionGuard, InstanceId, ResidencyKind, ResidentSegment, RolloverSignal, SegmentCheckout,
    SegmentResidency, Tier0Cache, Tier0CacheConfig, Tier1Cache, Tier1Config, Tier2Config,
    Tier2Manager, TieredCoordinator, TieredCoordinatorNotifiers, TieredInstance,
    TieredObservabilitySnapshot,
};
use arc_swap::ArcSwapOption;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::{
    runtime::{Builder, Handle, Runtime},
    sync::{Notify, watch},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{trace, warn};

pub mod config;
pub mod error;
pub mod flush;
pub mod fs;
pub mod manifest;
pub mod metrics;
pub mod reader;
pub mod segment;
pub mod store;

pub use config::{
    AofConfig, CompactionPolicy, Compression, FlushConfig, IdStrategy, RecordId, RetentionPolicy,
    SegmentId,
};
pub use fs::{
    Layout, SEGMENT_FILE_EXTENSION, SegmentFileName, TempFileGuard, create_fixed_size_file,
    fsync_dir,
};
pub use metrics::manifest_replay::{
    METRIC_MANIFEST_REPLAY_CHUNK_COUNT, METRIC_MANIFEST_REPLAY_CHUNK_LAG_SECONDS,
    METRIC_MANIFEST_REPLAY_CORRUPTION_EVENTS, METRIC_MANIFEST_REPLAY_JOURNAL_LAG_BYTES,
    ManifestReplayMetrics, ManifestReplaySnapshot,
};
pub use reader::{RecordBounds, SegmentReader, SegmentRecord, TailFollower};

use self::fs::CurrentSealedPointer;

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

    pub fn flush_failures(&self) -> u64 {
        self.snapshot.flush_failures
    }

    pub fn metadata_retry_attempts(&self) -> u64 {
        self.snapshot.metadata_retry_attempts
    }

    pub fn metadata_retry_failures(&self) -> u64 {
        self.snapshot.metadata_retry_failures
    }

    pub fn metadata_failures(&self) -> u64 {
        self.snapshot.metadata_failures
    }

    pub fn backlog_bytes(&self) -> u64 {
        self.snapshot.backlog_bytes
    }

    pub fn samples(&self) -> impl Iterator<Item = FlushMetricSample> {
        const METRIC_NAMES: [(&str, fn(&FlushMetricsSnapshot) -> u64); 7] = [
            ("aof_flush_retry_attempts_total", |s| s.retry_attempts),
            ("aof_flush_retry_failures_total", |s| s.retry_failures),
            ("aof_flush_failures_total", |s| s.flush_failures),
            ("aof_flush_metadata_retry_attempts_total", |s| {
                s.metadata_retry_attempts
            }),
            ("aof_flush_metadata_retry_failures_total", |s| {
                s.metadata_retry_failures
            }),
            ("aof_flush_metadata_failures_total", |s| s.metadata_failures),
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

use error::{AofError, AofResult, BackpressureKind};
use flush::{
    FlushManager, FlushMetrics, FlushMetricsSnapshot, FlushRequest, flush_with_retry,
    persist_metadata_with_retry,
};
use segment::{Segment, SegmentAppendResult, SegmentFooter};

const DEFAULT_TIER0_CLUSTER_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB
const DEFAULT_TIER0_INSTANCE_QUOTA_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB
const DEFAULT_TIER1_CACHE_BYTES: u64 = 8 * 1024 * 1024 * 1024; // 8 GiB
const DEFAULT_RUNTIME_SHUTDOWN_TIMEOUT_SECS: u64 = 5;
const TIERED_SERVICE_IDLE_BACKOFF_MS: u64 = 10;

#[derive(Debug, Clone)]
pub struct TieredStoreConfig {
    pub tier0: Tier0CacheConfig,
    pub tier1: Tier1Config,
    pub tier2: Option<Tier2Config>,
}

impl Default for TieredStoreConfig {
    fn default() -> Self {
        Self {
            tier0: Tier0CacheConfig::new(
                DEFAULT_TIER0_CLUSTER_BYTES,
                DEFAULT_TIER0_INSTANCE_QUOTA_BYTES,
            ),
            tier1: Tier1Config::new(DEFAULT_TIER1_CACHE_BYTES),
            tier2: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AofManagerConfig {
    pub runtime_worker_threads: Option<usize>,
    pub runtime_shutdown_timeout: Duration,
    pub flush: FlushConfig,
    pub store: TieredStoreConfig,
}

impl Default for AofManagerConfig {
    fn default() -> Self {
        Self {
            runtime_worker_threads: None,
            runtime_shutdown_timeout: Duration::from_secs(DEFAULT_RUNTIME_SHUTDOWN_TIMEOUT_SECS),
            flush: FlushConfig::default(),
            store: TieredStoreConfig::default(),
        }
    }
}

impl AofManagerConfig {
    pub fn without_tier2(mut self) -> Self {
        self.store.tier2 = None;
        self
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        let mut cfg = Self::default();
        cfg.runtime_worker_threads = Some(2);
        cfg.store.tier0 = Tier0CacheConfig::new(32 * 1024 * 1024, 32 * 1024 * 1024);
        cfg.store.tier1 = Tier1Config::new(64 * 1024 * 1024).with_worker_threads(1);
        cfg.store.tier2 = None;
        cfg
    }
}

#[derive(Debug)]
pub struct InstanceMetadata {
    coordinator_watermark: AtomicU64,
    current_ext_id: AtomicU64,
}

impl InstanceMetadata {
    fn new() -> Self {
        Self {
            coordinator_watermark: AtomicU64::new(0),
            current_ext_id: AtomicU64::new(0),
        }
    }

    pub fn coordinator_watermark(&self) -> u64 {
        self.coordinator_watermark.load(Ordering::Acquire)
    }

    pub fn set_coordinator_watermark(&self, watermark: u64) {
        self.coordinator_watermark
            .store(watermark, Ordering::Release);
    }

    pub fn current_ext_id(&self) -> u64 {
        self.current_ext_id.load(Ordering::Acquire)
    }

    pub fn set_current_ext_id(&self, ext_id: u64) {
        self.current_ext_id.store(ext_id, Ordering::Release);
    }
}

#[derive(Default)]
pub struct CoordinatorMetadataRegistry {
    instances: Mutex<HashMap<InstanceId, Arc<InstanceMetadata>>>,
}

impl CoordinatorMetadataRegistry {
    pub fn register(&self, instance_id: InstanceId) -> Arc<InstanceMetadata> {
        let mut instances = self.instances.lock();
        Arc::clone(
            instances
                .entry(instance_id)
                .or_insert_with(|| Arc::new(InstanceMetadata::new())),
        )
    }

    pub fn get(&self, instance_id: InstanceId) -> Option<Arc<InstanceMetadata>> {
        let instances = self.instances.lock();
        instances.get(&instance_id).cloned()
    }

    pub fn unregister(&self, instance_id: InstanceId) {
        self.instances.lock().remove(&instance_id);
    }
}

#[derive(Clone)]
pub struct InstanceMetadataHandle {
    metadata: Arc<InstanceMetadata>,
}

impl InstanceMetadataHandle {
    fn new(metadata: Arc<InstanceMetadata>) -> Self {
        Self { metadata }
    }

    pub fn set_coordinator_watermark(&self, watermark: u64) {
        self.metadata.set_coordinator_watermark(watermark);
    }

    pub fn coordinator_watermark(&self) -> u64 {
        self.metadata.coordinator_watermark()
    }

    pub fn set_current_ext_id(&self, ext_id: u64) {
        self.metadata.set_current_ext_id(ext_id);
    }

    pub fn current_ext_id(&self) -> u64 {
        self.metadata.current_ext_id()
    }
}

pub(crate) struct TieredRuntime {
    runtime: Mutex<Option<Runtime>>,
    handle: Handle,
    shutdown_token: CancellationToken,
    shutdown_timeout: Duration,
}

impl TieredRuntime {
    pub fn create(worker_threads: Option<usize>, shutdown_timeout: Duration) -> AofResult<Self> {
        let mut builder = Builder::new_multi_thread();
        builder.enable_all();
        if let Some(threads) = worker_threads {
            builder.worker_threads(threads.max(1));
        }
        let runtime = builder
            .build()
            .map_err(|err| AofError::other(format!("failed to build Tokio runtime: {err}")))?;
        Ok(Self::from_runtime(runtime, shutdown_timeout))
    }

    pub fn from_runtime(runtime: Runtime, shutdown_timeout: Duration) -> Self {
        let handle = runtime.handle().clone();
        Self {
            runtime: Mutex::new(Some(runtime)),
            handle,
            shutdown_token: CancellationToken::new(),
            shutdown_timeout,
        }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    pub fn shutdown(&self) {
        self.shutdown_inner();
    }

    fn shutdown_inner(&self) {
        if !self.shutdown_token.is_cancelled() {
            self.shutdown_token.cancel();
        }
        let mut guard = self.runtime.lock();
        if let Some(runtime) = guard.take() {
            if tokio::runtime::Handle::try_current().is_ok() {
                runtime.shutdown_background();
            } else {
                runtime.shutdown_timeout(self.shutdown_timeout);
            }
        }
    }
}

impl Drop for TieredRuntime {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}

pub struct AofManager {
    runtime: Arc<TieredRuntime>,
    flush_config: FlushConfig,
    coordinator: Arc<TieredCoordinator>,
    handle: Arc<AofManagerHandle>,
    tiered_task: Mutex<Option<JoinHandle<()>>>,
}

impl AofManager {
    pub fn with_config(config: AofManagerConfig) -> AofResult<Self> {
        let AofManagerConfig {
            runtime_worker_threads,
            runtime_shutdown_timeout,
            flush,
            store,
        } = config;

        let runtime = TieredRuntime::create(runtime_worker_threads, runtime_shutdown_timeout)?;
        Self::from_parts(Arc::new(runtime), flush, store)
    }

    pub fn with_runtime(runtime: Runtime, config: AofManagerConfig) -> AofResult<Self> {
        let AofManagerConfig {
            runtime_worker_threads: _,
            runtime_shutdown_timeout,
            flush,
            store,
        } = config;

        let runtime = TieredRuntime::from_runtime(runtime, runtime_shutdown_timeout);
        Self::from_parts(Arc::new(runtime), flush, store)
    }

    fn from_parts(
        runtime: Arc<TieredRuntime>,
        flush: FlushConfig,
        store: TieredStoreConfig,
    ) -> AofResult<Self> {
        let TieredStoreConfig {
            tier0,
            tier1,
            tier2,
        } = store;
        let tier0_cache = Tier0Cache::new(tier0);
        let tier1_cache = Tier1Cache::new(runtime.handle(), tier1)?;
        let tier2_manager = match tier2 {
            Some(cfg) => Some(Tier2Manager::new(runtime.handle(), cfg)?),
            None => None,
        };
        let coordinator = Arc::new(TieredCoordinator::new(
            tier0_cache,
            tier1_cache,
            tier2_manager,
        ));
        let handle = Arc::new(AofManagerHandle::new(
            runtime.clone(),
            coordinator.clone(),
            flush,
        ));
        let tiered_task = Self::spawn_tiered_service(&runtime, coordinator.clone());
        Ok(Self {
            runtime,
            flush_config: flush,
            coordinator,
            handle,
            tiered_task: Mutex::new(Some(tiered_task)),
        })
    }

    fn spawn_tiered_service(
        runtime: &Arc<TieredRuntime>,
        coordinator: Arc<TieredCoordinator>,
    ) -> JoinHandle<()> {
        let shutdown = runtime.shutdown_token();
        let activity = coordinator.activity_signal();
        runtime
            .handle()
            .spawn(Self::run_tiered_service(coordinator, activity, shutdown))
    }

    async fn run_tiered_service(
        coordinator: Arc<TieredCoordinator>,
        activity: Arc<Notify>,
        shutdown: CancellationToken,
    ) {
        let mut idle = false;
        loop {
            if idle {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = activity.notified() => {},
                    _ = tokio::time::sleep(Duration::from_millis(TIERED_SERVICE_IDLE_BACKOFF_MS)) => {},
                };
            } else if shutdown.is_cancelled() {
                break;
            }

            let outcome = tokio::select! {
                _ = shutdown.cancelled() => None,
                outcome = coordinator.poll() => Some(outcome),
            };

            let Some(outcome) = outcome else {
                break;
            };

            if !outcome.stats.is_idle() {
                trace!(
                    drop_events = outcome.stats.drop_events,
                    scheduled_evictions = outcome.stats.scheduled_evictions,
                    activation_grants = outcome.stats.activation_grants,
                    tier2_events = outcome.stats.tier2_events,
                    hydrations = outcome.stats.hydrations,
                    "tiered coordinator poll processed events",
                );
            }

            idle = outcome.stats.is_idle();
        }
    }

    pub fn runtime_handle(&self) -> Handle {
        self.runtime.handle()
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.runtime.shutdown_token()
    }

    pub fn tiered(&self) -> Arc<TieredCoordinator> {
        self.coordinator.clone()
    }

    pub fn handle(&self) -> Arc<AofManagerHandle> {
        self.handle.clone()
    }

    pub fn flush_config(&self) -> &FlushConfig {
        &self.flush_config
    }
}

impl Drop for AofManager {
    fn drop(&mut self) {
        if let Some(handle) = self.tiered_task.lock().take() {
            handle.abort();
        }
        self.runtime.shutdown();
    }
}

pub struct AofManagerHandle {
    runtime: Arc<TieredRuntime>,
    coordinator: Arc<TieredCoordinator>,
    flush_config: FlushConfig,
    metadata: Arc<CoordinatorMetadataRegistry>,
}

impl AofManagerHandle {
    fn new(
        runtime: Arc<TieredRuntime>,
        coordinator: Arc<TieredCoordinator>,
        flush_config: FlushConfig,
    ) -> Self {
        Self {
            runtime,
            coordinator,
            flush_config,
            metadata: Arc::new(CoordinatorMetadataRegistry::default()),
        }
    }

    pub fn runtime(&self) -> Arc<TieredRuntime> {
        self.runtime.clone()
    }

    pub fn runtime_handle(&self) -> Handle {
        self.runtime.handle()
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.runtime.shutdown_token()
    }

    pub fn tiered(&self) -> Arc<TieredCoordinator> {
        self.coordinator.clone()
    }

    pub fn flush_config(&self) -> FlushConfig {
        self.flush_config
    }

    pub fn admission_metrics(&self) -> AdmissionMetricsSnapshot {
        self.coordinator.admission_metrics()
    }

    pub fn observability_snapshot(&self) -> TieredObservabilitySnapshot {
        self.coordinator.observability_snapshot()
    }

    pub(crate) fn register_instance_metadata(
        &self,
        instance_id: InstanceId,
    ) -> Arc<InstanceMetadata> {
        self.metadata.register(instance_id)
    }

    pub(crate) fn unregister_instance_metadata(&self, instance_id: InstanceId) {
        self.metadata.unregister(instance_id);
    }

    pub fn metadata_handle(&self, instance_id: InstanceId) -> Option<InstanceMetadataHandle> {
        self.metadata
            .get(instance_id)
            .map(InstanceMetadataHandle::new)
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;
    use tokio::runtime::Builder;

    #[test]
    fn manager_initializes_with_default_store() {
        let manager =
            AofManager::with_config(AofManagerConfig::for_tests()).expect("create manager");
        let tiered = manager.tiered();
        let metrics = tiered.tier0().metrics();
        assert_eq!(metrics.total_bytes, 0);
        drop(tiered);
    }

    #[test]
    fn manager_accepts_existing_runtime() {
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("build runtime");
        let manager = AofManager::with_runtime(runtime, AofManagerConfig::for_tests())
            .expect("create manager");
        let _handle = manager.runtime_handle();
    }
}

struct AppendState {
    tail: ArcSwapOption<AdmissionGuard>,
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
    catalog: Vec<ResidentSegment>,
    pending_finalize: VecDeque<ResidentSegment>,
    next_segment_index: u32,
    flush_queue: VecDeque<ResidentSegment>,
    rollovers: HashMap<SegmentId, RolloverAwaiter>,
}

#[derive(Debug)]
enum RolloverAwaiter {
    Pending(Arc<RolloverSignal>),
    Ready,
}

impl AofManagement {
    fn resident_for(&self, segment: &Arc<Segment>) -> Option<ResidentSegment> {
        self.catalog
            .iter()
            .find(|resident| Arc::ptr_eq(resident.segment(), segment))
            .cloned()
    }

    fn remove_pending(&mut self, segment: &Arc<Segment>) -> bool {
        if let Some(pos) = self
            .pending_finalize
            .iter()
            .position(|pending| Arc::ptr_eq(pending.segment(), segment))
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
            .position(|queued| Arc::ptr_eq(queued.segment(), segment))
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

    fn snapshot_pending(queue: &VecDeque<ResidentSegment>) -> Vec<SegmentStatusSnapshot> {
        queue
            .iter()
            .map(|seg| SegmentStatusSnapshot::from_segment(seg.segment().as_ref()))
            .collect()
    }
}

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
    metadata: Arc<InstanceMetadata>,
    tail_signal: TailSignal,
}

impl Aof {
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
        );
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
                catalog: Vec::new(),
                pending_finalize: VecDeque::new(),
                next_segment_index: 0,
                flush_queue: VecDeque::new(),
                rollovers: HashMap::new(),
            }),
            metrics,
            flush_failed,
            flush,
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

    pub fn open_reader(&self, segment_id: SegmentId) -> AofResult<SegmentReader> {
        if let Some(resident) = {
            let state = self.management.lock();
            state
                .catalog
                .iter()
                .find(|segment| segment.id() == segment_id)
                .cloned()
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

    pub async fn open_reader_async(&self, segment_id: SegmentId) -> AofResult<SegmentReader> {
        if let Some(resident) = {
            let state = self.management.lock();
            state
                .catalog
                .iter()
                .find(|segment| segment.id() == segment_id)
                .cloned()
        } {
            return SegmentReader::new(resident);
        }

        let checkout = self.tier.checkout_sealed_segment(segment_id)?;
        let resident = checkout.wait().await?;
        SegmentReader::new(resident)
    }

    pub fn flush_metrics(&self) -> FlushMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn coordinator_watermark(&self) -> u64 {
        self.metadata.coordinator_watermark()
    }

    pub fn set_coordinator_watermark(&self, watermark: u64) {
        self.metadata.set_coordinator_watermark(watermark);
    }

    pub fn set_current_ext_id(&self, ext_id: u64) -> AofResult<()> {
        self.metadata.set_current_ext_id(ext_id);
        if let Some(guard) = self.current_tail() {
            guard.segment().set_ext_id(ext_id)?;
        }
        Ok(())
    }

    pub fn metadata_handle(&self) -> InstanceMetadataHandle {
        InstanceMetadataHandle::new(self.metadata.clone())
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

    pub async fn append_record_async(&self, payload: &[u8]) -> AofResult<RecordId> {
        let shutdown = self.manager.shutdown_token();
        loop {
            match self.append_record(payload) {
                Ok(id) => return Ok(id),
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
        let coordinator_watermark = self.metadata.coordinator_watermark();
        let flush_failure = self.flush_failed.load(Ordering::Acquire);
        let footer = segment.seal(sealed_at, coordinator_watermark, flush_failure)?;
        self.update_current_sealed_pointer(segment_id, &footer)?;
        guard.mark_sealed();

        let mut state = self.management.lock();
        state.remove_pending(segment);
        state.remove_flush(segment);
        let pending_snapshot = TailSignal::snapshot_pending(&state.pending_finalize);
        drop(state);
        let tail_snapshot = self
            .append
            .tail
            .load_full()
            .map(|guard| SegmentStatusSnapshot::from_segment(guard.segment().as_ref()));
        self.tail_signal.replace(
            tail_snapshot,
            pending_snapshot,
            TailEvent::Sealed(segment_id),
        );
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
        if let Some(segment) = state
            .pending_finalize
            .iter()
            .find(|segment| segment.id() == segment_id)
            .cloned()
        {
            if state.remove_pending(segment.segment()) {
                state.remove_flush(segment.segment());
                let removed_rollover = state.rollovers.remove(&segment_id).is_some();

                let tail_matches_segment = self
                    .append
                    .tail
                    .load_full()
                    .map(|guard| Arc::ptr_eq(guard.segment(), segment.segment()))
                    .unwrap_or(false);
                if tail_matches_segment {
                    self.append.tail.store(None);
                }

                let mut activated_guard = None;
                if self.append.tail.load_full().is_none() {
                    if let Some(resident) = state
                        .pending_finalize
                        .iter()
                        .find(|resident| !resident.segment().is_sealed())
                        .cloned()
                    {
                        let guard = self.tier.admission_guard(&resident);
                        self.append.tail.store(Some(Arc::new(guard.clone())));
                        activated_guard = Some(guard);
                    }
                }

                let pending_snapshot = TailSignal::snapshot_pending(&state.pending_finalize);
                drop(state);

                if removed_rollover {
                    self.notifiers.remove_rollover(self.instance_id, segment_id);
                }

                let tail_snapshot = self
                    .append
                    .tail
                    .load_full()
                    .map(|guard| SegmentStatusSnapshot::from_segment(guard.segment().as_ref()));
                self.tail_signal.replace(
                    tail_snapshot,
                    pending_snapshot.clone(),
                    TailEvent::Sealed(segment_id),
                );
                self.notifiers.notify_admission(self.instance_id);

                if let Some(guard) = activated_guard {
                    let pending_snapshot = {
                        let state = self.management.lock();
                        TailSignal::snapshot_pending(&state.pending_finalize)
                    };
                    let activated_snapshot =
                        SegmentStatusSnapshot::from_segment(guard.segment().as_ref());
                    self.tail_signal.replace(
                        Some(activated_snapshot),
                        pending_snapshot,
                        TailEvent::Activated(guard.segment().id()),
                    );
                    self.notifiers.notify_admission(self.instance_id);
                }

                return Ok(());
            }
        }
        Err(AofError::InvalidState(format!(
            "segment {} not pending finalization",
            segment_id
        )))
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

        let target_bytes = flush_state.requested_bytes();
        self.tier
            .record_durability_request(segment.id(), target_bytes);

        let durability = self.tier.durability_handle();

        let request = {
            let mut state = self.management.lock();
            state.remove_flush(segment);
            state.resident_for(segment).map(|resident| {
                state.flush_queue.push_back(resident.clone());
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
                    state.remove_flush(segment);
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
                    state.remove_flush(segment);
                }
                self.flush_failed.store(true, Ordering::Release);
                self.record_would_block(BackpressureKind::Flush);
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
            coordinator_watermark: footer.coordinator_watermark,
            durable_bytes: footer.durable_bytes,
        };
        if let Err(err) = self.layout.store_current_sealed_pointer(pointer) {
            warn!(
                segment = segment_id.as_u64(),
                durable_bytes = footer.durable_bytes,
                watermark = footer.coordinator_watermark,
                "failed to persist current.sealed pointer: {err}"
            );
            return Err(err);
        }
        Ok(())
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
        self.manager
            .runtime_handle()
            .block_on(flush_state.wait_for(target_logical));
        Ok(())
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
        let state = self.management.lock();
        state
            .rollovers
            .values()
            .any(|awaiter| matches!(awaiter, RolloverAwaiter::Pending(_)))
    }

    fn pending_rollover_signal(&self) -> Option<(SegmentId, Arc<RolloverSignal>)> {
        let state = self.management.lock();
        state
            .rollovers
            .iter()
            .find_map(|(segment_id, awaiter)| match awaiter {
                RolloverAwaiter::Pending(signal) => Some((*segment_id, Arc::clone(signal))),
                RolloverAwaiter::Ready => None,
            })
    }

    fn mark_rollover_ready(&self, segment_id: SegmentId) {
        let mut state = self.management.lock();
        if let Some(entry) = state.rollovers.get_mut(&segment_id) {
            if let RolloverAwaiter::Pending(signal) = entry {
                if signal.is_ready() {
                    *entry = RolloverAwaiter::Ready;
                }
            }
        }
    }

    fn remove_rollover_entry(&self, segment_id: SegmentId) {
        let mut state = self.management.lock();
        if state.rollovers.remove(&segment_id).is_some() {
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
            .iter()
            .filter(|resident| !resident.segment().is_sealed())
            .count();
        if unsealed_pending >= 2 {
            return Ok(None);
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

        segment.set_ext_id(self.metadata.current_ext_id())?;

        let resident = self.tier.admit_segment(
            segment.clone(),
            SegmentResidency::new(segment.current_size() as u64, ResidencyKind::Active),
        )?;
        let guard = self.tier.admission_guard(&resident);

        state.next_segment_index = segment_index.saturating_add(1);
        state.catalog.push(resident);
        self.append.tail.store(Some(Arc::new(guard.clone())));
        self.notifiers.notify_admission(self.instance_id);
        let tail_snapshot = SegmentStatusSnapshot::from_segment(guard.segment().as_ref());
        let pending_snapshot = TailSignal::snapshot_pending(&state.pending_finalize);
        self.tail_signal.replace(
            Some(tail_snapshot),
            pending_snapshot,
            TailEvent::Activated(guard.segment().id()),
        );
        Ok(Some(guard))
    }

    fn queue_rollover(&self, segment: &Arc<Segment>) -> AofResult<()> {
        let mut state = self.management.lock();
        let segment_id = segment.id();
        if state.rollovers.contains_key(&segment_id) {
            return Ok(());
        }
        let resident = state
            .resident_for(segment)
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
        state
            .rollovers
            .insert(segment_id, RolloverAwaiter::Pending(signal));
        Ok(())
    }

    fn is_rollover_ack_ready(&self, segment_id: SegmentId) -> AofResult<bool> {
        let mut state = self.management.lock();
        let Some(entry) = state.rollovers.get_mut(&segment_id) else {
            return Ok(true);
        };
        match entry {
            RolloverAwaiter::Ready => Ok(true),
            RolloverAwaiter::Pending(signal) => {
                if let Some(result) = signal.result() {
                    match result {
                        Ok(()) => {
                            *entry = RolloverAwaiter::Ready;
                            Ok(true)
                        }
                        Err(err) => {
                            state.rollovers.remove(&segment_id);
                            Err(AofError::rollover_failed(err))
                        }
                    }
                } else {
                    Ok(false)
                }
            }
        }
    }

    fn remove_ready_rollover(&self, segment_id: SegmentId) {
        let mut state = self.management.lock();
        if matches!(
            state.rollovers.get(&segment_id),
            Some(RolloverAwaiter::Ready)
        ) {
            state.rollovers.remove(&segment_id);
            self.notifiers.remove_rollover(self.instance_id, segment_id);
        }
    }

    fn take_ready_rollover(&self) -> Option<SegmentId> {
        let mut state = self.management.lock();
        let ready_id = state
            .rollovers
            .iter()
            .find_map(|(id, awaiter)| matches!(awaiter, RolloverAwaiter::Ready).then_some(*id));
        if let Some(id) = ready_id {
            state.rollovers.remove(&id);
            self.notifiers.remove_rollover(self.instance_id, id);
            Some(id)
        } else {
            None
        }
    }

    fn poll_pending_rollovers(&self) -> AofResult<Option<SegmentId>> {
        let ids: Vec<_> = {
            let state = self.management.lock();
            state.rollovers.keys().copied().collect()
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
            let pending_snapshot;
            {
                let mut state = self.management.lock();
                if let Some(resident) = state.resident_for(segment) {
                    if !state
                        .pending_finalize
                        .iter()
                        .any(|pending| Arc::ptr_eq(pending.segment(), segment))
                    {
                        state.pending_finalize.push_back(resident);
                    }
                }
                pending_snapshot = TailSignal::snapshot_pending(&state.pending_finalize);
            }
            self.schedule_flush(segment)?;
            let tail_snapshot = self
                .append
                .tail
                .load_full()
                .map(|guard| SegmentStatusSnapshot::from_segment(guard.segment().as_ref()));
            self.tail_signal.replace(
                tail_snapshot,
                pending_snapshot,
                TailEvent::Sealing(segment.id()),
            );
        }

        if segment.is_sealed() {
            self.queue_rollover(segment)?;
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

        let mut catalog = Vec::with_capacity(recovered.len());
        let mut pending_finalize = VecDeque::new();
        let mut flush_queue = VecDeque::new();
        let mut tail: Option<Arc<Segment>> = None;
        let mut next_offset = pointer.as_ref().map(|p| p.durable_bytes).unwrap_or(0);
        let mut total_records = 0u64;
        let mut next_segment_index = 0u32;
        let mut unsealed_count = 0usize;
        let mut total_unflushed = 0u64;
        let mut pointer_matched = pointer.is_none();
        let mut highest_watermark = 0u64;

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
                    highest_watermark = ptr.coordinator_watermark;
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
                    pending_finalize.push_back(resident.clone());
                    flush_queue.push_back(resident.clone());
                } else {
                    flush_queue.push_back(resident.clone());
                    tail = Some(segment.clone());
                }
            }

            catalog.push(resident);
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
                .iter()
                .find(|resident| Arc::ptr_eq(resident.segment(), segment))
                .map(|resident| Arc::new(self.tier.admission_guard(resident)))
        });

        let tail_snapshot = tail_guard_arc
            .as_ref()
            .map(|guard| SegmentStatusSnapshot::from_segment(guard.segment().as_ref()));
        let pending_snapshot = TailSignal::snapshot_pending(&pending_finalize);
        let event = if let Some(snapshot) = tail_snapshot.as_ref() {
            TailEvent::Activated(snapshot.segment_id)
        } else if let Some(first_pending) = pending_snapshot.first() {
            TailEvent::Sealing(first_pending.segment_id)
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
            pending_for_flush = state.flush_queue.iter().cloned().collect();
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

        self.tail_signal
            .replace(tail_snapshot, pending_snapshot, event);

        if let Some(segment) = tail {
            self.metadata.set_current_ext_id(segment.ext_id());
        } else {
            self.metadata.set_current_ext_id(0);
        }
        self.metadata.set_coordinator_watermark(highest_watermark);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aof2::error::{AofError, BackpressureKind};
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

    fn drain_rollover(manager: &AofManager) {
        manager
            .runtime_handle()
            .block_on(drain_rollover_async(manager));
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
        let res1 = state.resident_for(&seg1).expect("resident seg1");
        state.pending_finalize.push_back(res1);
        let guard2 = aof
            .ensure_segment_locked(&mut state, created_at)
            .unwrap()
            .unwrap();
        let seg2 = guard2.segment().clone();
        aof.append.tail.store(None);
        let res2 = state.resident_for(&seg2).expect("resident seg2");
        state.pending_finalize.push_back(res2);
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
        let res1 = state.resident_for(&seg1).expect("resident seg1");
        state.pending_finalize.push_back(res1);

        let guard2 = aof
            .ensure_segment_locked(&mut state, created_at)
            .expect("segment")
            .expect("guard2");
        let seg2 = guard2.segment().clone();
        aof.append.tail.store(None);
        let res2 = state.resident_for(&seg2).expect("resident seg2");
        state.pending_finalize.push_back(res2);
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
                let _ = segment.seal(current_timestamp_nanos(), 0, false);
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
                let _ = segment.seal(current_timestamp_nanos(), 0, false);
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
        segment.seal(sealed_at, 0, false).expect("seal");
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
            .seal(current_timestamp_nanos(), 0, false)
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
            .seal(current_timestamp_nanos(), 0, false)
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
    fn pointer_captures_watermark_and_durable_bytes() {
        let tmp = TempDir::new().expect("tempdir");
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let mut cfg = test_config(tmp.path());
        cfg.segment_min_bytes = 512;
        cfg.segment_max_bytes = 512;
        cfg.segment_target_bytes = 512;

        let aof = Aof::new(handle.clone(), cfg.clone()).expect("aof");
        aof.set_coordinator_watermark(123);

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
        assert_eq!(pointer.coordinator_watermark, 123);

        let segment = aof.segment_by_id(sealed_id).expect("sealed segment state");
        let expected_durable = segment.base_offset() + segment.durable_size() as u64;
        assert_eq!(pointer.durable_bytes, expected_durable);

        drop(aof);

        let restarted = Aof::new(handle.clone(), cfg).expect("restart aof");
        assert_eq!(restarted.coordinator_watermark(), 123);
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
                coordinator_watermark: 77,
                durable_bytes: 1_024,
            })
            .expect("store pointer");

        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle.clone(), layout_cfg.clone()).expect("recover aof");
        assert_eq!(aof.coordinator_watermark(), 0);

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
        metadata.set_coordinator_watermark(911);

        let record_id = aof.append_record(b"metadata-handle").expect("append");
        let segment_id = SegmentId::new(record_id.segment_index() as u64);
        let segment = aof.segment_by_id(segment_id).expect("segment");
        assert_eq!(segment.ext_id(), 314);

        let sealed = seal_active_until_ready(&aof, &manager)
            .expect("seal result")
            .expect("segment sealed");
        assert_eq!(sealed, segment_id);
        assert_eq!(aof.coordinator_watermark(), 911);

        let pointer = aof
            .layout()
            .load_current_sealed_pointer()
            .expect("load pointer")
            .expect("pointer present");
        assert_eq!(pointer.coordinator_watermark, 911);
    }
}
