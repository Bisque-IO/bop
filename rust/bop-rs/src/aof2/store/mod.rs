// Tiered segment store modules

pub mod notifiers;
pub mod tier0;
pub mod tier1;
pub mod tier2;
pub use crate::aof2::manifest::{
    ChunkHandle, ManifestLogReader, ManifestLogWriter, ManifestLogWriterConfig, ManifestRecord,
    ManifestRecordIter, ManifestRecordPayload, RecordType, SealedChunkHandle,
};
pub use tier0::{
    ActivationGrant, ActivationOutcome, ActivationReason, ActivationRequest, AdmissionGuard,
    InstanceId, PendingActivationSnapshot, ResidencyKind, ResidentSegment, SegmentResidency,
    Tier0Cache, Tier0CacheConfig, Tier0DropEvent, Tier0Eviction, Tier0Instance, Tier0Metrics,
    Tier0MetricsSnapshot,
};
pub use tier1::{
    HydrationOutcome, ManifestEntry, Tier1Cache, Tier1Config, Tier1Instance, Tier1Metrics,
    Tier1MetricsSnapshot, Tier1ResidencyState,
};

pub use notifiers::{RolloverSignal, TieredCoordinatorNotifiers};
pub use tier2::{
    Tier2Config, Tier2DeleteRequest, Tier2Event, Tier2FetchRequest, Tier2Handle, Tier2Manager,
    Tier2Metadata, Tier2Metrics, Tier2MetricsSnapshot, Tier2UploadDescriptor,
};

use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::fs::read_dir;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use tokio::sync::{Notify, oneshot};
use tracing::{trace, warn};

use crate::aof2::config::SegmentId;
use crate::aof2::error::{AofError, AofResult, BackpressureKind};
use crate::aof2::fs::{Layout, SEGMENT_FILE_EXTENSION, SegmentFileName};
use crate::aof2::segment::Segment;

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

pub struct SegmentWaiter {
    inner: Arc<TieredInstanceInner>,
    segment_id: SegmentId,
    notify: Arc<Notify>,
}

impl SegmentWaiter {
    async fn wait(mut self) -> AofResult<ResidentSegment> {
        loop {
            self.notify.notified().await;
            if let Some(resident) = self.inner.try_checkout_resident(self.segment_id)? {
                return Ok(resident);
            }
            self.notify = self.inner.register_waiter(self.segment_id);
        }
    }
}

struct TieredInstanceInner {
    coordinator: Arc<TieredCoordinator>,
    layout: Layout,
    tier0: Tier0Instance,
    tier1: Tier1Instance,
    tier2: Option<Tier2Handle>,
    waiters: Mutex<HashMap<SegmentId, Vec<Arc<Notify>>>>,
}

impl TieredInstanceInner {
    fn new(
        coordinator: Arc<TieredCoordinator>,
        layout: Layout,
        tier0: Tier0Instance,
        tier1: Tier1Instance,
        tier2: Option<Tier2Handle>,
    ) -> Arc<Self> {
        Arc::new(Self {
            coordinator,
            layout,
            tier0,
            tier1,
            tier2,
            waiters: Mutex::new(HashMap::new()),
        })
    }

    fn instance_id(&self) -> InstanceId {
        self.tier0.instance_id()
    }

    fn admit_segment(
        self: &Arc<Self>,
        segment: Arc<Segment>,
        residency: SegmentResidency,
    ) -> AofResult<ResidentSegment> {
        let (resident, evictions) = self.tier0.admit_segment(segment.clone(), residency);
        self.tier1
            .update_residency(segment.id(), Tier1ResidencyState::ResidentInTier0)?;
        self.handle_tier0_evictions(&evictions);
        Ok(resident)
    }

    fn handle_tier0_evictions(&self, evictions: &[Tier0Eviction]) {
        if evictions.is_empty() {
            return;
        }
        for eviction in evictions {
            if let Err(err) = self
                .tier1
                .update_residency(eviction.segment_id, Tier1ResidencyState::StagedForTier0)
            {
                warn!(
                    instance = eviction.instance_id.get(),
                    segment = eviction.segment_id.as_u64(),
                    "failed to update tier1 residency after tier0 eviction: {err}"
                );
            }
        }
        self.coordinator.signal_activity();
    }

    fn try_checkout_resident(
        self: &Arc<Self>,
        segment_id: SegmentId,
    ) -> AofResult<Option<ResidentSegment>> {
        if let Some(resident) = self.tier0.checkout_segment(segment_id) {
            return Ok(Some(resident));
        }
        Ok(None)
    }

    fn locate_segment_path(&self, segment_id: SegmentId) -> AofResult<Option<PathBuf>> {
        for entry in read_dir(self.layout.segments_dir()).map_err(AofError::from)? {
            let entry = entry.map_err(AofError::from)?;
            if !entry.file_type().map_err(AofError::from)?.is_file() {
                continue;
            }
            let path = entry.path();
            match path.extension().and_then(OsStr::to_str) {
                Some(ext) if ext.eq_ignore_ascii_case(&SEGMENT_FILE_EXTENSION[1..]) => {}
                _ => continue,
            }
            let name = SegmentFileName::parse(&path)?;
            if name.segment_id == segment_id {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    fn load_segment_from_path(
        self: &Arc<Self>,
        segment_id: SegmentId,
    ) -> AofResult<Option<ResidentSegment>> {
        let Some(path) = self.locate_segment_path(segment_id)? else {
            return Ok(None);
        };
        let mut scan = Segment::scan_tail(&path)?;
        if scan.truncated {
            Segment::truncate_segment(&path, &mut scan)?;
        }
        let segment = Arc::new(Segment::from_recovered(&path, &scan)?);
        let residency_kind = if segment.is_sealed() {
            ResidencyKind::Sealed
        } else {
            ResidencyKind::Active
        };
        let residency = SegmentResidency::new(segment.current_size() as u64, residency_kind);
        let resident = self.admit_segment(segment, residency)?;
        Ok(Some(resident))
    }

    fn manifest_entry(&self, segment_id: SegmentId) -> AofResult<Option<ManifestEntry>> {
        self.tier1.manifest_entry(segment_id)
    }

    fn hydrate_immediately(self: &Arc<Self>, entry: &ManifestEntry) -> AofResult<ResidentSegment> {
        let outcome = self
            .tier1
            .hydrate_segment_blocking(entry.segment_id)?
            .ok_or_else(|| AofError::SegmentNotLoaded(entry.segment_id))?;
        self.admit_hydrated(outcome)
    }

    fn schedule_activation(self: &Arc<Self>, entry: &ManifestEntry) -> AofResult<SegmentCheckout> {
        let required = entry.original_bytes.max(entry.compressed_bytes);
        let (outcome, evictions) = self.tier0.request_activation(ActivationRequest {
            segment_id: entry.segment_id,
            required_bytes: required,
            priority: 0,
            reason: ActivationReason::ReadMiss,
        });
        self.handle_tier0_evictions(&evictions);
        match outcome {
            ActivationOutcome::Admitted => {
                let resident = self.hydrate_immediately(entry)?;
                Ok(SegmentCheckout::Ready(resident))
            }
            ActivationOutcome::Queued => {
                let notify = self.register_waiter(entry.segment_id);
                Ok(SegmentCheckout::Pending(SegmentWaiter {
                    inner: Arc::clone(self),
                    segment_id: entry.segment_id,
                    notify,
                }))
            }
        }
    }

    fn register_waiter(self: &Arc<Self>, segment_id: SegmentId) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        let mut waiters = self.waiters.lock();
        waiters
            .entry(segment_id)
            .or_default()
            .push(Arc::clone(&notify));
        notify
    }

    fn notify_waiters(&self, segment_id: SegmentId) {
        if let Some(waiters) = self.waiters.lock().remove(&segment_id) {
            for waiter in waiters {
                waiter.notify_waiters();
            }
        }
    }

    fn admit_hydrated(self: &Arc<Self>, outcome: HydrationOutcome) -> AofResult<ResidentSegment> {
        trace!(
            instance = outcome.instance_id.get(),
            segment = outcome.segment_id.as_u64(),
            path = %outcome.path.display(),
            "admitting hydrated segment"
        );
        let mut scan = Segment::scan_tail(&outcome.path)?;
        if scan.truncated {
            Segment::truncate_segment(&outcome.path, &mut scan)?;
        }
        let segment = Arc::new(Segment::from_recovered(&outcome.path, &scan)?);
        let residency = SegmentResidency::new(segment.current_size() as u64, ResidencyKind::Sealed);
        let resident = self.admit_segment(segment, residency)?;
        self.notify_waiters(outcome.segment_id);
        Ok(resident)
    }

    fn recover_segments(self: &Arc<Self>) -> AofResult<Vec<RecoveredSegment>> {
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
            return Ok(Vec::new());
        }
        entries.sort_by_key(|(name, _)| name.segment_id.as_u64());

        let mut recovered = Vec::with_capacity(entries.len());
        for (_name, path) in entries {
            let mut scan = Segment::scan_tail(&path)?;
            if scan.truncated {
                Segment::truncate_segment(&path, &mut scan)?;
            }
            let segment = Arc::new(Segment::from_recovered(&path, &scan)?);
            let residency_kind = if segment.is_sealed() {
                ResidencyKind::Sealed
            } else {
                ResidencyKind::Active
            };
            let residency = SegmentResidency::new(segment.current_size() as u64, residency_kind);
            let resident = self.admit_segment(segment.clone(), residency)?;
            recovered.push(RecoveredSegment { resident, segment });
        }
        Ok(recovered)
    }

    fn checkout_sealed_segment(
        self: &Arc<Self>,
        segment_id: SegmentId,
    ) -> AofResult<SegmentCheckout> {
        if let Some(resident) = self.try_checkout_resident(segment_id)? {
            return Ok(SegmentCheckout::Ready(resident));
        }
        if let Some(resident) = self.load_segment_from_path(segment_id)? {
            return Ok(SegmentCheckout::Ready(resident));
        }
        let entry = self
            .manifest_entry(segment_id)?
            .ok_or_else(|| AofError::SegmentNotLoaded(segment_id))?;
        self.schedule_activation(&entry)
    }

    fn handle_hydration(self: &Arc<Self>, outcome: HydrationOutcome) {
        if let Err(err) = self.admit_hydrated(outcome) {
            warn!("failed to admit hydrated segment: {err}");
        }
    }
}

impl Drop for TieredInstanceInner {
    fn drop(&mut self) {
        self.coordinator.unregister_instance(self.instance_id());
    }
}

#[derive(Clone)]
pub struct TieredInstance {
    inner: Arc<TieredInstanceInner>,
}

impl TieredInstance {
    pub fn instance_id(&self) -> InstanceId {
        self.inner.instance_id()
    }

    pub fn admit_segment(
        &self,
        segment: Arc<Segment>,
        residency: SegmentResidency,
    ) -> AofResult<ResidentSegment> {
        self.inner.admit_segment(segment, residency)
    }

    pub fn admission_guard(&self, resident: &ResidentSegment) -> AdmissionGuard {
        self.inner.tier0.admission_guard(resident.clone())
    }

    pub fn recover_segments(&self) -> AofResult<Vec<RecoveredSegment>> {
        self.inner.recover_segments()
    }

    pub fn checkout_sealed_segment(&self, segment_id: SegmentId) -> AofResult<SegmentCheckout> {
        self.inner.checkout_sealed_segment(segment_id)
    }

    pub async fn await_sealed_segment(&self, segment_id: SegmentId) -> AofResult<ResidentSegment> {
        match self.checkout_sealed_segment(segment_id)? {
            SegmentCheckout::Ready(resident) => Ok(resident),
            SegmentCheckout::Pending(waiter) => waiter.wait().await,
        }
    }

    pub fn request_rollover(
        &self,
        resident: ResidentSegment,
    ) -> AofResult<oneshot::Receiver<AofResult<()>>> {
        self.inner
            .coordinator
            .enqueue_rollover(self.instance_id(), resident)
    }

    pub fn hydration_signal(&self) -> Arc<Notify> {
        self.inner.coordinator.hydration_signal()
    }

    pub fn notifiers(&self) -> Arc<TieredCoordinatorNotifiers> {
        self.inner.coordinator.notifiers()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TieredPollStats {
    pub drop_events: usize,
    pub scheduled_evictions: usize,
    pub activation_grants: usize,
    pub tier2_events: usize,
    pub hydrations: usize,
    pub rollovers: usize,
}

impl TieredPollStats {
    pub fn total_activity(&self) -> usize {
        self.drop_events
            + self.scheduled_evictions
            + self.activation_grants
            + self.tier2_events
            + self.hydrations
            + self.rollovers
    }

    pub fn is_idle(&self) -> bool {
        self.total_activity() == 0
    }

    pub fn had_hydrations(&self) -> bool {
        self.hydrations > 0
    }
}

#[derive(Debug, Default)]
pub struct TieredPollOutcome {
    pub stats: TieredPollStats,
    pub hydrations: Vec<HydrationOutcome>,
}

const COORDINATOR_COMMAND_CAPACITY: usize = 1024;

enum CoordinatorCommand {
    Rollover {
        instance_id: InstanceId,
        resident: ResidentSegment,
        ack: oneshot::Sender<AofResult<()>>,
    },
}

pub struct TieredCoordinator {
    tier0: Tier0Cache,
    tier1: Tier1Cache,
    tier2: Option<Tier2Manager>,
    activity: Arc<Notify>,
    hydration: Arc<Notify>,
    instances: Mutex<HashMap<InstanceId, Weak<TieredInstanceInner>>>,
    commands: Mutex<VecDeque<CoordinatorCommand>>,
    notifiers: Arc<TieredCoordinatorNotifiers>,
}

impl TieredCoordinator {
    pub fn new(tier0: Tier0Cache, tier1: Tier1Cache, tier2: Option<Tier2Manager>) -> Self {
        if let Some(manager) = &tier2 {
            tier1.attach_tier2(manager.handle());
        }
        let notifiers = Arc::new(TieredCoordinatorNotifiers::new());
        Self {
            tier0,
            tier1,
            tier2,
            activity: Arc::new(Notify::new()),
            hydration: Arc::new(Notify::new()),
            instances: Mutex::new(HashMap::new()),
            commands: Mutex::new(VecDeque::new()),
            notifiers,
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

    pub fn notifiers(&self) -> Arc<TieredCoordinatorNotifiers> {
        self.notifiers.clone()
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

    pub fn enqueue_rollover(
        &self,
        instance_id: InstanceId,
        resident: ResidentSegment,
    ) -> AofResult<oneshot::Receiver<AofResult<()>>> {
        let (ack, rx) = oneshot::channel();
        let mut commands = self.commands.lock();
        if commands.len() >= COORDINATOR_COMMAND_CAPACITY {
            return Err(AofError::would_block(BackpressureKind::Rollover));
        }
        commands.push_back(CoordinatorCommand::Rollover {
            instance_id,
            resident,
            ack,
        });
        drop(commands);
        self.activity.notify_one();
        Ok(rx)
    }

    pub fn register_instance(
        self: &Arc<Self>,
        name: impl Into<String>,
        layout: Layout,
        quota_override: Option<u64>,
    ) -> AofResult<TieredInstance> {
        let tier0_instance = self.tier0.register_instance(name, quota_override);
        let tier1_instance = self
            .tier1
            .register_instance(tier0_instance.instance_id(), layout.clone())?;
        let tier2_handle = self.tier2.as_ref().map(|manager| manager.handle());
        let inner = TieredInstanceInner::new(
            Arc::clone(self),
            layout,
            tier0_instance,
            tier1_instance,
            tier2_handle,
        );
        self.notifiers.register_instance(inner.instance_id());
        self.instances
            .lock()
            .insert(inner.instance_id(), Arc::downgrade(&inner));
        Ok(TieredInstance { inner })
    }

    fn unregister_instance(&self, instance_id: InstanceId) {
        self.instances.lock().remove(&instance_id);
        self.notifiers.unregister_instance(instance_id);
    }

    fn dispatch_hydrations(&self, outcomes: &[HydrationOutcome]) {
        if outcomes.is_empty() {
            return;
        }
        let mut to_process = Vec::new();
        {
            let mut instances = self.instances.lock();
            for outcome in outcomes {
                let instance_id = outcome.instance_id;
                if let Some(inner) = instances.get(&instance_id).and_then(Weak::upgrade) {
                    to_process.push((inner, outcome.clone()));
                } else {
                    instances.remove(&instance_id);
                }
            }
        }
        for (inner, outcome) in to_process {
            inner.handle_hydration(outcome);
        }
    }

    fn drain_commands(&self) -> Vec<CoordinatorCommand> {
        let mut commands = self.commands.lock();
        commands.drain(..).collect()
    }

    fn process_rollover(
        &self,
        instance_id: InstanceId,
        resident: ResidentSegment,
        ack: oneshot::Sender<AofResult<()>>,
    ) -> AofResult<()> {
        let inner = {
            let mut instances = self.instances.lock();
            match instances.get(&instance_id).and_then(Weak::upgrade) {
                Some(inner) => inner,
                None => {
                    instances.remove(&instance_id);
                    let err = AofError::InstanceNotFound(instance_id.get());
                    let _ = ack.send(Err(err));
                    return Ok(());
                }
            }
        };

        let segment = resident.segment().clone();
        if !segment.is_sealed() {
            let message = format!(
                "rollover requested for unsealed segment {}",
                segment.id().as_u64()
            );
            let err = AofError::InvalidState(message);
            let _ = ack.send(Err(err));
            return Ok(());
        }

        if let Err(err) = inner
            .tier1
            .schedule_compression_with_ack(segment, Some(ack))
        {
            warn!(
                instance = instance_id.get(),
                "failed to enqueue compression for rollover: {err}"
            );
        }
        Ok(())
    }

    pub async fn poll(&self) -> TieredPollOutcome {
        let mut stats = TieredPollStats::default();
        let mut hydrations = Vec::new();
        let mut signaled_activity = false;

        let commands = self.drain_commands();
        if !commands.is_empty() {
            stats.rollovers = commands.len();
            signaled_activity = true;
        }
        for command in commands {
            match command {
                CoordinatorCommand::Rollover {
                    instance_id,
                    resident,
                    ack,
                } => {
                    if let Err(err) = self.process_rollover(instance_id, resident, ack) {
                        warn!(
                            instance = instance_id.get(),
                            "rollover processing error: {err}"
                        );
                    }
                }
            }
        }

        let evictions = self.tier0.drain_scheduled_evictions_async().await;
        if !evictions.is_empty() {
            stats.scheduled_evictions = evictions.len();
            for eviction in evictions {
                if let Err(err) = self.tier1.update_residency(
                    eviction.instance_id,
                    eviction.segment_id,
                    Tier1ResidencyState::StagedForTier0,
                ) {
                    warn!(
                        instance = eviction.instance_id.get(),
                        segment = eviction.segment_id.as_u64(),
                        "failed to stage tier0 eviction for warm cache: {err}"
                    );
                }
            }
            signaled_activity = true;
        }

        let drop_events = self.tier0.drain_drop_events_async().await;
        if !drop_events.is_empty() {
            stats.drop_events = drop_events.len();
            self.tier1.handle_drop_events(drop_events);
            signaled_activity = true;
        }

        if let Some(tier2) = &self.tier2 {
            let events = tier2.drain_events();
            if !events.is_empty() {
                stats.tier2_events = events.len();
                self.tier1.handle_tier2_events(events);
                signaled_activity = true;
            }
            tier2.process_retention();
        }

        let activation_grants = self.tier0.drain_activation_grants_async().await;
        if !activation_grants.is_empty() {
            stats.activation_grants = activation_grants.len();
            let results = self.tier1.handle_activation_grants(activation_grants).await;
            stats.hydrations = results.len();
            if !results.is_empty() {
                hydrations = results;
                self.hydration.notify_waiters();
            }
            signaled_activity = true;
        }

        if signaled_activity {
            self.activity.notify_waiters();
        }

        self.dispatch_hydrations(&hydrations);
        TieredPollOutcome { stats, hydrations }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aof2::SegmentId;
    use crate::aof2::config::{AofConfig, FlushConfig};
    use crate::aof2::segment::Segment;
    use crate::aof2::store::tier0::{ResidencyKind, SegmentResidency};
    use chrono::Utc;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::time::{Duration, sleep};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn poll_processes_drop_and_compression() {
        let dir = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = dir.path().to_path_buf();
        cfg.flush = FlushConfig::default();
        let layout = Layout::new(&cfg);
        layout.ensure().expect("layout ensure");

        let tier0 = Tier0Cache::new(Tier0CacheConfig::new(512 * 1024, 512 * 1024));
        let tier1 = Tier1Cache::new(
            tokio::runtime::Handle::current(),
            Tier1Config::new(2 * 1024 * 1024).with_worker_threads(1),
        )
        .expect("tier1 cache");
        let coordinator = Arc::new(TieredCoordinator::new(tier0.clone(), tier1, None));

        let tiered_instance = coordinator
            .register_instance("test", layout.clone(), None)
            .expect("register instance");
        let instance_id = tiered_instance.instance_id();

        let segment_id = SegmentId::new(1);
        let created_at = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        let segment_path = layout.segment_file_path(segment_id, 0, created_at);
        let segment =
            Segment::create_active(segment_id, 0, 0, created_at, 1024 * 1024, &segment_path)
                .expect("segment create");
        segment.append_record(b"payload", 1).expect("append record");
        let size = segment.current_size();
        let _ = segment.mark_durable(size);
        segment.seal(created_at).expect("seal");
        let segment = Arc::new(segment);

        let resident = tiered_instance
            .admit_segment(
                segment.clone(),
                SegmentResidency::new(size as u64, ResidencyKind::Sealed),
            )
            .expect("tier0 admit through tier abstraction");
        drop(resident);

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut saw_drop = false;
        loop {
            let outcome = coordinator.poll().await;
            if outcome.stats.drop_events > 0 {
                saw_drop = true;
            }

            let manifest = coordinator
                .tier1()
                .manifest_snapshot(instance_id)
                .expect("manifest snapshot");
            if let Some(entry) = manifest.first() {
                assert_eq!(entry.segment_id, segment_id);
                assert_eq!(entry.residency, Tier1ResidencyState::StagedForTier0);
                break;
            }

            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for compression");
            }

            sleep(Duration::from_millis(20)).await;
        }

        assert!(saw_drop, "coordinator did not observe drop events");
        drop(segment);
    }
}
