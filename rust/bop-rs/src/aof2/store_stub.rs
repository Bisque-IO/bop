use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::sync::Notify;

use crate::aof2::config::SegmentId;
use crate::aof2::error::{AofError, AofResult};
use crate::aof2::fs::{Layout, SEGMENT_FILE_EXTENSION, SegmentFileName};
use crate::aof2::segment::Segment;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct InstanceId(u64);

impl InstanceId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn get(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationReason {
    ReadMiss,
    Prefetch,
    FollowerCatchUp,
    Recovery,
}

#[derive(Debug, Clone)]
pub struct ActivationRequest {
    pub segment_id: SegmentId,
    pub required_bytes: u64,
    pub priority: u32,
    pub reason: ActivationReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationOutcome {
    Admitted,
    Queued,
}

#[derive(Debug, Clone)]
pub struct ActivationGrant {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub bytes: u64,
    pub priority: u32,
    pub reason: ActivationReason,
}

#[derive(Debug, Clone)]
pub struct Tier0Eviction {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub bytes: u64,
}

#[derive(Clone)]
pub struct Tier0DropEvent {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub bytes: u64,
    pub was_sealed: bool,
    pub segment: Option<Arc<Segment>>,
}

#[derive(Debug, Clone, Default)]
pub struct PendingActivationSnapshot {
    pub queued: Vec<QueuedActivationInfo>,
}

#[derive(Debug, Clone)]
pub struct QueuedActivationInfo {
    pub segment_id: SegmentId,
    pub priority: u32,
    pub reason: ActivationReason,
    pub requested_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidencyKind {
    Active,
    Sealed,
}

#[derive(Debug, Clone)]
pub struct SegmentResidency {
    pub bytes: u64,
    pub kind: ResidencyKind,
}

impl SegmentResidency {
    pub fn new(bytes: u64, kind: ResidencyKind) -> Self {
        Self { bytes, kind }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Tier0CacheConfig {
    pub cluster_max_bytes: u64,
    pub default_instance_quota: u64,
    pub activation_queue_warn_threshold: usize,
}

impl Tier0CacheConfig {
    pub fn new(cluster_max_bytes: u64, default_instance_quota: u64) -> Self {
        Self {
            cluster_max_bytes,
            default_instance_quota,
            activation_queue_warn_threshold: 0,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Tier0MetricsSnapshot {
    pub total_bytes: u64,
    pub activation_queue_depth: u32,
    pub scheduled_evictions: u64,
    pub drop_events: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Tier0Metrics;

#[derive(Clone)]
pub struct ResidentSegment {
    segment: Arc<Segment>,
}

impl ResidentSegment {
    pub fn new(segment: Arc<Segment>) -> Self {
        Self { segment }
    }

    pub fn segment(&self) -> &Arc<Segment> {
        &self.segment
    }
}

impl std::ops::Deref for ResidentSegment {
    type Target = Segment;

    fn deref(&self) -> &Self::Target {
        &self.segment
    }
}

#[derive(Clone)]
pub struct Tier0Cache {
    _config: Tier0CacheConfig,
}

impl Tier0Cache {
    pub fn new(config: Tier0CacheConfig) -> Self {
        Self { _config: config }
    }

    pub fn metrics(&self) -> Tier0MetricsSnapshot {
        Tier0MetricsSnapshot::default()
    }

    pub fn register_instance(
        &self,
        _name: impl Into<String>,
        _quota_override: Option<u64>,
    ) -> Tier0Instance {
        Tier0Instance {
            instance_id: InstanceId::new(0),
        }
    }

    pub fn drain_scheduled_evictions(&self) -> Vec<Tier0Eviction> {
        Vec::new()
    }

    pub fn drain_drop_events(&self) -> Vec<Tier0DropEvent> {
        Vec::new()
    }

    pub fn drain_activation_grants(&self) -> Vec<ActivationGrant> {
        Vec::new()
    }

    pub async fn drain_scheduled_evictions_async(&self) -> Vec<Tier0Eviction> {
        Vec::new()
    }

    pub async fn drain_drop_events_async(&self) -> Vec<Tier0DropEvent> {
        Vec::new()
    }

    pub async fn drain_activation_grants_async(&self) -> Vec<ActivationGrant> {
        Vec::new()
    }
}

#[derive(Clone)]
pub struct Tier0Instance {
    instance_id: InstanceId,
}

impl Tier0Instance {
    pub fn instance_id(&self) -> InstanceId {
        self.instance_id
    }

    pub fn admit_segment(
        &self,
        segment: Arc<Segment>,
        _residency: SegmentResidency,
    ) -> (ResidentSegment, Vec<Tier0Eviction>) {
        (ResidentSegment::new(segment), Vec::new())
    }

    pub fn record_access(&self, _segment_id: SegmentId) {}

    pub fn update_residency(&self, _segment_id: SegmentId, _residency: SegmentResidency) {}

    pub fn request_activation(
        &self,
        _request: ActivationRequest,
    ) -> (ActivationOutcome, Vec<Tier0Eviction>) {
        (ActivationOutcome::Queued, Vec::new())
    }

    pub fn checkout_segment(&self, _segment_id: SegmentId) -> Option<ResidentSegment> {
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Tier1Config {
    pub max_bytes: u64,
    pub worker_threads: usize,
}

impl Tier1Config {
    pub fn new(max_bytes: u64) -> Self {
        Self {
            max_bytes,
            worker_threads: 1,
        }
    }

    pub fn with_worker_threads(mut self, workers: usize) -> Self {
        self.worker_threads = workers.max(1);
        self
    }

    pub fn with_compression_level(self, _level: i32) -> Self {
        self
    }

    pub fn with_retry_policy(self, _max_retries: u32, _backoff: Duration) -> Self {
        self
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Tier1MetricsSnapshot {
    pub stored_bytes: u64,
    pub compression_jobs: u64,
    pub compression_failures: u64,
    pub hydration_failures: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Tier1Metrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier1ResidencyState {
    ResidentInTier0,
    StagedForTier0,
    UploadedToTier2,
}

impl Default for Tier1ResidencyState {
    fn default() -> Self {
        Tier1ResidencyState::StagedForTier0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tier2Metadata {
    pub object_key: String,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub uploaded_epoch_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManifestEntry {
    pub segment_id: SegmentId,
    pub base_offset: u64,
    pub base_record_count: u64,
    pub created_at: i64,
    pub sealed_at: i64,
    pub checksum: u32,
    pub original_bytes: u64,
    pub compressed_bytes: u64,
    pub compressed_path: String,
    pub dictionary: Option<String>,
    pub offset_index: Vec<u32>,
    pub residency: Tier1ResidencyState,
    pub tier2: Option<Tier2Metadata>,
    pub last_access_epoch_ms: u64,
}

#[derive(Clone)]
pub struct Tier1Cache {
    _runtime: Handle,
    _config: Tier1Config,
}

impl Tier1Cache {
    pub fn new(runtime: Handle, config: Tier1Config) -> AofResult<Self> {
        Ok(Self {
            _runtime: runtime,
            _config: config,
        })
    }

    pub fn metrics(&self) -> Tier1MetricsSnapshot {
        Tier1MetricsSnapshot::default()
    }

    pub fn register_instance(
        &self,
        instance_id: InstanceId,
        _layout: Layout,
    ) -> AofResult<Tier1Instance> {
        Ok(Tier1Instance { instance_id })
    }

    pub fn attach_tier2(&self, _handle: Tier2Handle) {}

    pub fn handle_tier2_events(&self, _events: Vec<Tier2Event>) {}

    pub fn handle_drop_events(&self, _events: Vec<Tier0DropEvent>) {}

    pub fn update_residency(
        &self,
        _instance_id: InstanceId,
        _segment_id: SegmentId,
        _residency: Tier1ResidencyState,
    ) -> AofResult<()> {
        Ok(())
    }

    pub async fn handle_activation_grants(
        &self,
        _grants: Vec<ActivationGrant>,
    ) -> Vec<HydrationOutcome> {
        Vec::new()
    }

    pub fn manifest_snapshot(&self, _instance_id: InstanceId) -> AofResult<Vec<ManifestEntry>> {
        Ok(Vec::new())
    }
}

impl Drop for Tier1Cache {
    fn drop(&mut self) {}
}

#[derive(Clone)]
pub struct Tier1Instance {
    instance_id: InstanceId,
}

impl Tier1Instance {
    pub fn instance_id(&self) -> InstanceId {
        self.instance_id
    }

    pub fn schedule_compression(&self, _segment: Arc<Segment>) -> AofResult<()> {
        Ok(())
    }

    pub fn update_residency(
        &self,
        _segment_id: SegmentId,
        _residency: Tier1ResidencyState,
    ) -> AofResult<()> {
        Ok(())
    }

    pub fn manifest_entry(&self, _segment_id: SegmentId) -> AofResult<Option<ManifestEntry>> {
        Ok(None)
    }

    pub fn hydrate_segment_blocking(
        &self,
        _segment_id: SegmentId,
    ) -> AofResult<Option<HydrationOutcome>> {
        Ok(None)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Tier2MetricsSnapshot {
    pub upload_attempts: u64,
    pub upload_failures: u64,
    pub downloads: u64,
    pub download_failures: u64,
    pub deletes: u64,
    pub delete_failures: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Tier2Metrics;

#[derive(Debug, Clone, Default)]
pub struct Tier2DeleteRequest;

#[derive(Debug, Clone, Default)]
pub struct Tier2UploadDescriptor;

#[derive(Debug, Clone, Default)]
pub struct Tier2FetchRequest {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub sealed_at: i64,
    pub destination: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Tier2Config;

impl Tier2Config {
    pub fn new(
        _endpoint: impl Into<String>,
        _region: impl Into<String>,
        _bucket: impl Into<String>,
        _access_key: impl Into<String>,
        _secret_key: impl Into<String>,
    ) -> Self {
        Self
    }

    pub fn with_prefix(self, _prefix: impl Into<String>) -> Self {
        self
    }

    pub fn with_session_token(self, _token: impl Into<Option<String>>) -> Self {
        self
    }

    pub fn with_concurrency(self, _workers: usize) -> Self {
        self
    }

    pub fn with_retry(self, _retry: RetryPolicy) -> Self {
        self
    }

    pub fn with_retention_ttl(self, _ttl: Option<Duration>) -> Self {
        self
    }

    pub fn with_security(self, _security: Option<Tier2Security>) -> Self {
        self
    }
}

#[derive(Debug, Clone)]
pub struct RetryPolicy;

impl RetryPolicy {
    pub fn new(_max_attempts: u32, _base_delay: Duration) -> Self {
        Self
    }
}

#[derive(Debug, Clone)]
pub enum Tier2Event {}

#[derive(Clone)]
pub struct Tier2Handle;

#[derive(Clone)]
pub struct Tier2Manager;

impl Tier2Manager {
    pub fn new(_runtime: Handle, _config: Tier2Config) -> AofResult<Self> {
        Err(AofError::invalid_config(
            "Tier 2 storage requires enabling the tiered-store feature",
        ))
    }

    pub fn handle(&self) -> Tier2Handle {
        Tier2Handle
    }

    pub fn drain_events(&self) -> Vec<Tier2Event> {
        Vec::new()
    }

    pub fn process_retention(&self) {}

    pub fn metrics(&self) -> Tier2MetricsSnapshot {
        Tier2MetricsSnapshot::default()
    }
}

#[derive(Debug, Clone)]
pub struct HydrationOutcome {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub path: PathBuf,
    pub base_offset: u64,
    pub base_record_count: u64,
    pub created_at: i64,
    pub sealed_at: i64,
    pub checksum: u32,
    pub logical_size: u32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TieredPollStats {
    pub drop_events: usize,
    pub scheduled_evictions: usize,
    pub activation_grants: usize,
    pub tier2_events: usize,
    pub hydrations: usize,
}

impl TieredPollStats {
    pub fn total_activity(&self) -> usize {
        0
    }

    pub fn is_idle(&self) -> bool {
        true
    }

    pub fn had_hydrations(&self) -> bool {
        false
    }
}

#[derive(Debug, Default)]
pub struct TieredPollOutcome {
    pub stats: TieredPollStats,
    pub hydrations: Vec<HydrationOutcome>,
}

#[derive(Clone)]
pub struct RecoveredSegment {
    resident: ResidentSegment,
    segment: Arc<Segment>,
}

impl RecoveredSegment {
    pub fn resident(&self) -> &ResidentSegment {
        &self.resident
    }

    pub fn segment(&self) -> &Arc<Segment> {
        &self.segment
    }
}

pub enum SegmentCheckout {
    Ready(ResidentSegment),
    Pending(SegmentWaiter),
}

impl SegmentCheckout {
    pub async fn wait(self) -> AofResult<ResidentSegment> {
        match self {
            SegmentCheckout::Ready(resident) => Ok(resident),
            SegmentCheckout::Pending(waiter) => waiter.wait().await,
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, SegmentCheckout::Ready(_))
    }
}

pub struct SegmentWaiter;

impl SegmentWaiter {
    async fn wait(self) -> AofResult<ResidentSegment> {
        Err(AofError::InvalidState(
            "async hydration is not supported by the tiered-store stub".to_string(),
        ))
    }
}

#[derive(Clone)]
pub struct TieredInstance {
    coordinator: Arc<TieredCoordinator>,
    layout: Layout,
    tier0: Tier0Instance,
    tier1: Tier1Instance,
    tier2: Option<Tier2Handle>,
}

impl TieredInstance {
    pub fn instance_id(&self) -> InstanceId {
        self.tier0.instance_id()
    }

    pub fn admit_segment(
        &self,
        segment: Arc<Segment>,
        residency: SegmentResidency,
    ) -> AofResult<ResidentSegment> {
        Ok(self.tier0.admit_segment(segment, residency).0)
    }

    pub fn recover_segments(&self) -> AofResult<Vec<RecoveredSegment>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(self.layout.segments_dir()).map_err(AofError::from)? {
            let entry = entry.map_err(AofError::from)?;
            let path = entry.path();
            if !entry.file_type().map_err(AofError::from)?.is_file() {
                continue;
            }
            match path.extension().and_then(std::ffi::OsStr::to_str) {
                Some(ext) if ext.eq_ignore_ascii_case(&SEGMENT_FILE_EXTENSION[1..]) => {}
                _ => continue,
            }
            let name = SegmentFileName::parse(&path)?;
            entries.push((name, path));
        }
        entries.sort_by_key(|(name, _)| name.segment_id.as_u64());
        let mut recovered = Vec::with_capacity(entries.len());
        for (_name, path) in entries {
            let mut scan = Segment::scan_tail(&path)?;
            if scan.truncated {
                Segment::truncate_segment(&path, &mut scan)?;
            }
            let segment = Arc::new(Segment::from_recovered(&path, &scan)?);
            let resident = self
                .tier0
                .admit_segment(
                    segment.clone(),
                    SegmentResidency::new(segment.current_size() as u64, ResidencyKind::Sealed),
                )
                .0;
            recovered.push(RecoveredSegment { resident, segment });
        }
        Ok(recovered)
    }

    pub fn checkout_sealed_segment(&self, segment_id: SegmentId) -> AofResult<SegmentCheckout> {
        if let Some(resident) = self.tier0.checkout_segment(segment_id) {
            return Ok(SegmentCheckout::Ready(resident));
        }
        let path = self
            .layout
            .segments_dir()
            .read_dir()
            .map_err(AofError::from)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                if let Ok(name) = SegmentFileName::parse(path) {
                    name.segment_id == segment_id
                } else {
                    false
                }
            })
            .ok_or_else(|| AofError::SegmentNotLoaded(segment_id))?;
        let mut scan = Segment::scan_tail(&path)?;
        if scan.truncated {
            Segment::truncate_segment(&path, &mut scan)?;
        }
        let segment = Arc::new(Segment::from_recovered(&path, &scan)?);
        let resident = self
            .tier0
            .admit_segment(
                segment,
                SegmentResidency::new(scan.logical_size as u64, ResidencyKind::Sealed),
            )
            .0;
        Ok(SegmentCheckout::Ready(resident))
    }

    pub async fn await_sealed_segment(&self, segment_id: SegmentId) -> AofResult<ResidentSegment> {
        match self.checkout_sealed_segment(segment_id)? {
            SegmentCheckout::Ready(resident) => Ok(resident),
            SegmentCheckout::Pending(waiter) => waiter.wait().await,
        }
    }

    pub fn hydration_signal(&self) -> Arc<Notify> {
        self.coordinator.hydration_signal()
    }
}
pub struct TieredCoordinator {
    tier0: Tier0Cache,
    tier1: Tier1Cache,
    tier2: Option<Tier2Manager>,
    activity: Arc<Notify>,
    hydration: Arc<Notify>,
}

impl TieredCoordinator {
    pub fn new(tier0: Tier0Cache, tier1: Tier1Cache, tier2: Option<Tier2Manager>) -> Self {
        Self {
            tier0,
            tier1,
            tier2,
            activity: Arc::new(Notify::new()),
            hydration: Arc::new(Notify::new()),
        }
    }

    pub fn tier0(&self) -> &Tier0Cache {
        &self.tier0
    }

    pub fn tier1(&self) -> &Tier1Cache {
        &self.tier1
    }

    pub fn tier2(&self) -> Option<&Tier2Manager> {
        self.tier2.as_ref()
    }

    pub fn activity_signal(&self) -> Arc<Notify> {
        self.activity.clone()
    }

    pub fn hydration_signal(&self) -> Arc<Notify> {
        self.hydration.clone()
    }

    pub fn signal_activity(&self) {
        self.activity.notify_one();
    }

    pub fn register_instance(
        self: &Arc<Self>,
        _name: impl Into<String>,
        layout: Layout,
        _quota_override: Option<u64>,
    ) -> AofResult<TieredInstance> {
        let tier0 = self.tier0.register_instance("stub", None);
        let tier1 = self
            .tier1
            .register_instance(tier0.instance_id(), layout.clone())?;
        Ok(TieredInstance {
            coordinator: Arc::clone(self),
            layout,
            tier0,
            tier1,
            tier2: self.tier2.as_ref().map(|manager| manager.handle()),
        })
    }

    pub async fn poll(&self) -> TieredPollOutcome {
        TieredPollOutcome::default()
    }
}

#[derive(Debug, Clone)]
pub struct Tier2Security;
