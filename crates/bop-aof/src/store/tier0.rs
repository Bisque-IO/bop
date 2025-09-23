use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::ops::Deref;
use std::sync::Arc;
use std::sync::Weak;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering as AtomicOrdering};

use parking_lot::Mutex;
use tokio::task::spawn_blocking;
use tracing::{debug, trace, warn};

use crate::config::SegmentId;
use crate::segment::Segment;

/// Maximum number of pending activation requests tolerated before issuing warnings.
const DEFAULT_ACTIVATION_QUEUE_WARN_THRESHOLD: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceId(u64);

impl InstanceId {
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn get(&self) -> u64 {
        self.0
    }
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

#[derive(Debug, Clone)]
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
            activation_queue_warn_threshold: DEFAULT_ACTIVATION_QUEUE_WARN_THRESHOLD,
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

#[derive(Debug, Default)]
pub struct Tier0Metrics {
    total_bytes: AtomicU64,
    activation_queue_depth: AtomicU32,
    scheduled_evictions: AtomicU64,
    drop_events: AtomicU64,
}

impl Tier0Metrics {
    pub fn snapshot(&self) -> Tier0MetricsSnapshot {
        Tier0MetricsSnapshot {
            total_bytes: self.total_bytes.load(AtomicOrdering::Relaxed),
            activation_queue_depth: self.activation_queue_depth.load(AtomicOrdering::Relaxed),
            scheduled_evictions: self.scheduled_evictions.load(AtomicOrdering::Relaxed),
            drop_events: self.drop_events.load(AtomicOrdering::Relaxed),
        }
    }

    fn update_total_bytes(&self, bytes: u64) {
        self.total_bytes.store(bytes, AtomicOrdering::Relaxed);
    }

    fn update_activation_depth(&self, depth: usize) {
        self.activation_queue_depth
            .store(depth as u32, AtomicOrdering::Relaxed);
    }

    fn incr_evictions(&self, count: usize) {
        if count > 0 {
            self.scheduled_evictions
                .fetch_add(count as u64, AtomicOrdering::Relaxed);
        }
    }

    fn incr_drop_events(&self, count: usize) {
        if count > 0 {
            self.drop_events
                .fetch_add(count as u64, AtomicOrdering::Relaxed);
        }
    }
}

#[derive(Clone)]
pub struct Tier0Cache {
    inner: Arc<Tier0Inner>,
}

impl Tier0Cache {
    pub fn new(config: Tier0CacheConfig) -> Self {
        let inner = Arc::new(Tier0Inner::new(config));
        Self { inner }
    }

    pub fn metrics(&self) -> Tier0MetricsSnapshot {
        self.inner.metrics.snapshot()
    }

    pub fn register_instance(
        &self,
        name: impl Into<String>,
        quota_override: Option<u64>,
    ) -> Tier0Instance {
        let instance_id = self.inner.register_instance(name.into(), quota_override);
        Tier0Instance {
            inner: self.inner.clone(),
            instance_id,
        }
    }

    pub fn drain_scheduled_evictions(&self) -> Vec<Tier0Eviction> {
        self.inner.drain_evictions()
    }

    pub fn drain_drop_events(&self) -> Vec<Tier0DropEvent> {
        self.inner.drain_drop_events()
    }

    pub fn drain_activation_grants(&self) -> Vec<ActivationGrant> {
        self.inner.drain_activation_grants()
    }

    pub fn pending_activation_snapshot(&self) -> PendingActivationSnapshot {
        self.inner.pending_activation_snapshot()
    }

    pub async fn drain_scheduled_evictions_async(&self) -> Vec<Tier0Eviction> {
        let inner = Arc::clone(&self.inner);
        match spawn_blocking(move || inner.drain_evictions()).await {
            Ok(evictions) => evictions,
            Err(err) => {
                warn!("tier0 eviction drain task panicked: {err}");
                Vec::new()
            }
        }
    }

    pub async fn drain_drop_events_async(&self) -> Vec<Tier0DropEvent> {
        let inner = Arc::clone(&self.inner);
        match spawn_blocking(move || inner.drain_drop_events()).await {
            Ok(events) => events,
            Err(err) => {
                warn!("tier0 drop event drain task panicked: {err}");
                Vec::new()
            }
        }
    }

    pub async fn drain_activation_grants_async(&self) -> Vec<ActivationGrant> {
        let inner = Arc::clone(&self.inner);
        match spawn_blocking(move || inner.drain_activation_grants()).await {
            Ok(grants) => grants,
            Err(err) => {
                warn!("tier0 activation grant drain task panicked: {err}");
                Vec::new()
            }
        }
    }
}

#[derive(Clone)]
pub struct Tier0Instance {
    inner: Arc<Tier0Inner>,
    instance_id: InstanceId,
}

impl Tier0Instance {
    pub fn instance_id(&self) -> InstanceId {
        self.instance_id
    }

    pub fn admit_segment(
        &self,
        segment: Arc<Segment>,
        residency: SegmentResidency,
    ) -> (ResidentSegment, Vec<Tier0Eviction>) {
        self.inner
            .admit_segment(self.instance_id, segment, residency)
    }

    pub fn record_access(&self, segment_id: SegmentId) {
        self.inner.record_access(self.instance_id, segment_id);
    }

    pub fn update_residency(&self, segment_id: SegmentId, residency: SegmentResidency) {
        self.inner
            .update_residency(self.instance_id, segment_id, residency);
    }

    pub fn request_activation(
        &self,
        request: ActivationRequest,
    ) -> (ActivationOutcome, Vec<Tier0Eviction>) {
        self.inner.request_activation(self.instance_id, request)
    }

    pub fn checkout_segment(&self, segment_id: SegmentId) -> Option<ResidentSegment> {
        self.inner.checkout_resident(self.instance_id, segment_id)
    }

    pub fn admission_guard(&self, resident: ResidentSegment) -> AdmissionGuard {
        AdmissionGuard::new(self.clone(), resident)
    }
}

#[derive(Clone)]
pub struct ResidentSegment {
    inner: Arc<ResidentSegmentInner>,
}

impl ResidentSegment {
    pub(crate) fn new(segment: Arc<Segment>, drop_guard: SegmentDropGuard) -> Self {
        Self {
            inner: Arc::new(ResidentSegmentInner {
                segment,
                drop_guard,
            }),
        }
    }

    pub fn segment(&self) -> &Arc<Segment> {
        &self.inner.segment
    }
}

#[derive(Clone)]
pub struct AdmissionGuard {
    inner: Arc<AdmissionGuardInner>,
}

struct AdmissionGuardInner {
    tier0: Tier0Instance,
    resident: ResidentSegment,
    bytes: AtomicU64,
    kind: AtomicU8,
}

impl AdmissionGuard {
    pub(crate) fn new(tier0: Tier0Instance, resident: ResidentSegment) -> Self {
        let segment = resident.segment().clone();
        let initial_bytes = segment.current_size() as u64;
        let initial_kind = if segment.is_sealed() {
            ResidencyKind::Sealed
        } else {
            ResidencyKind::Active
        };
        Self {
            inner: Arc::new(AdmissionGuardInner {
                tier0,
                resident,
                bytes: AtomicU64::new(initial_bytes),
                kind: AtomicU8::new(encode_kind(initial_kind)),
            }),
        }
    }

    pub fn segment(&self) -> &Arc<Segment> {
        self.inner.resident.segment()
    }

    pub fn resident(&self) -> ResidentSegment {
        self.inner.resident.clone()
    }

    pub fn record_append(&self, logical_size: u32) {
        self.update_residency(SegmentResidency::new(
            logical_size as u64,
            ResidencyKind::Active,
        ));
    }

    pub fn mark_sealed(&self) {
        let bytes = self.segment().current_size() as u64;
        self.update_residency(SegmentResidency::new(bytes, ResidencyKind::Sealed));
    }

    pub fn update_residency(&self, residency: SegmentResidency) {
        let new_kind = encode_kind(residency.kind);
        let prev_kind = self.inner.kind.swap(new_kind, AtomicOrdering::AcqRel);
        let prev_bytes = self
            .inner
            .bytes
            .swap(residency.bytes, AtomicOrdering::AcqRel);
        if prev_kind == new_kind && prev_bytes == residency.bytes {
            return;
        }
        self.inner
            .tier0
            .update_residency(self.segment().id(), residency);
    }
}

fn encode_kind(kind: ResidencyKind) -> u8 {
    match kind {
        ResidencyKind::Active => 0,
        ResidencyKind::Sealed => 1,
    }
}

#[cfg(test)]
impl ResidentSegment {
    pub(crate) fn new_for_tests(segment: Arc<Segment>) -> Self {
        Self {
            inner: Arc::new(ResidentSegmentInner {
                segment,
                drop_guard: SegmentDropGuard::for_tests(),
            }),
        }
    }
}

impl Deref for ResidentSegment {
    type Target = Segment;

    fn deref(&self) -> &Self::Target {
        &self.inner.segment
    }
}

struct ResidentSegmentInner {
    segment: Arc<Segment>,
    drop_guard: SegmentDropGuard,
}

impl Drop for ResidentSegmentInner {
    fn drop(&mut self) {
        self.drop_guard.on_drop();
    }
}

struct SegmentDropGuard {
    key: ResidentKey,
    inner: Weak<Tier0Inner>,
    segment: Weak<Segment>,
}

impl SegmentDropGuard {
    pub(crate) fn new(inner: &Arc<Tier0Inner>, segment: &Arc<Segment>, key: ResidentKey) -> Self {
        Self {
            key,
            inner: Arc::downgrade(inner),
            segment: Arc::downgrade(segment),
        }
    }

    #[cfg(test)]
    fn for_tests() -> Self {
        Self {
            key: ResidentKey {
                instance_id: InstanceId::new(0),
                segment_id: SegmentId::new(0),
            },
            inner: Weak::new(),
            segment: Weak::new(),
        }
    }

    fn on_drop(&self) {
        if let Some(inner) = self.inner.upgrade() {
            let segment = self.segment.upgrade();
            inner.segment_dropped(self.key, segment);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ResidentKey {
    instance_id: InstanceId,
    segment_id: SegmentId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResidentPhase {
    Resident,
    Evicting,
}

struct ResidentEntry {
    bytes: u64,
    kind: ResidencyKind,
    phase: ResidentPhase,
    last_touch_tick: u64,
}

struct InstanceState {
    #[allow(dead_code)]
    name: String,
    quota_bytes: u64,
    bytes_used: u64,
}

struct Tier0State {
    total_bytes: u64,
    pending_evict_bytes: u64,
    access_clock: u64,
    instances: HashMap<InstanceId, InstanceState>,
    residents: HashMap<ResidentKey, ResidentEntry>,
    handles: HashMap<ResidentKey, Weak<ResidentSegmentInner>>,
    lru: VecDeque<ResidentKey>,
    eviction_queue: VecDeque<Tier0Eviction>,
    drop_events: VecDeque<Tier0DropEvent>,
    pending_activation: BinaryHeap<QueuedActivation>,
    activation_grants: VecDeque<ActivationGrant>,
}

impl Tier0State {
    pub(crate) fn new() -> Self {
        Self {
            total_bytes: 0,
            pending_evict_bytes: 0,
            access_clock: 0,
            instances: HashMap::new(),
            residents: HashMap::new(),
            handles: HashMap::new(),
            lru: VecDeque::new(),
            eviction_queue: VecDeque::new(),
            drop_events: VecDeque::new(),
            pending_activation: BinaryHeap::new(),
            activation_grants: VecDeque::new(),
        }
    }
}

struct Tier0Inner {
    config: Tier0CacheConfig,
    metrics: Tier0Metrics,
    next_instance: AtomicU64,
    queue_seq: AtomicU64,
    state: Mutex<Tier0State>,
}

impl Tier0Inner {
    pub(crate) fn new(config: Tier0CacheConfig) -> Self {
        Self {
            config,
            metrics: Tier0Metrics::default(),
            next_instance: AtomicU64::new(1),
            queue_seq: AtomicU64::new(0),
            state: Mutex::new(Tier0State::new()),
        }
    }

    fn register_instance(&self, name: String, quota_override: Option<u64>) -> InstanceId {
        let instance_id = InstanceId::new(self.next_instance.fetch_add(1, AtomicOrdering::Relaxed));
        let mut state = self.state.lock();
        let quota = quota_override.unwrap_or(self.config.default_instance_quota);
        state.instances.insert(
            instance_id,
            InstanceState {
                name,
                quota_bytes: quota,
                bytes_used: 0,
            },
        );
        instance_id
    }

    fn admit_segment(
        self: &Arc<Self>,
        instance_id: InstanceId,
        segment: Arc<Segment>,
        residency: SegmentResidency,
    ) -> (ResidentSegment, Vec<Tier0Eviction>) {
        let key = ResidentKey {
            instance_id,
            segment_id: segment.id(),
        };
        let mut state = self.state.lock();
        let entry = ResidentEntry {
            bytes: residency.bytes,
            kind: residency.kind,
            phase: ResidentPhase::Resident,
            last_touch_tick: self.bump_clock(&mut state),
        };
        state.residents.insert(key, entry);
        if residency.kind == ResidencyKind::Sealed {
            state.lru.push_back(key);
        }
        state.total_bytes = state.total_bytes.saturating_add(residency.bytes);
        if let Some(instance) = state.instances.get_mut(&instance_id) {
            instance.bytes_used = instance.bytes_used.saturating_add(residency.bytes);
        }
        self.metrics.update_total_bytes(state.total_bytes);
        let evictions = self.enforce_capacity_locked(&mut state);
        let drop_guard = SegmentDropGuard::new(self, &segment, key);
        let resident = ResidentSegment::new(segment, drop_guard);
        state.handles.insert(key, Arc::downgrade(&resident.inner));
        (resident, evictions)
    }

    fn record_access(&self, instance_id: InstanceId, segment_id: SegmentId) {
        let mut state = self.state.lock();
        let key = ResidentKey {
            instance_id,
            segment_id,
        };
        if state.residents.contains_key(&key) {
            let tick = self.bump_clock(&mut state);
            if let Some(entry) = state.residents.get_mut(&key) {
                entry.last_touch_tick = tick;
                if entry.kind == ResidencyKind::Sealed {
                    Self::move_to_lru_tail(&mut state.lru, key);
                }
            }
        }
    }

    fn checkout_resident(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
    ) -> Option<ResidentSegment> {
        let mut state = self.state.lock();
        let key = ResidentKey {
            instance_id,
            segment_id,
        };
        if let Some(handle) = state.handles.get(&key) {
            if let Some(inner) = handle.upgrade() {
                let tick = self.bump_clock(&mut state);
                if let Some(entry) = state.residents.get_mut(&key) {
                    entry.last_touch_tick = tick;
                    if entry.kind == ResidencyKind::Sealed {
                        Self::move_to_lru_tail(&mut state.lru, key);
                    }
                }
                return Some(ResidentSegment { inner });
            } else {
                state.handles.remove(&key);
            }
        }
        None
    }

    fn update_residency(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
        residency: SegmentResidency,
    ) {
        let mut state = self.state.lock();
        let key = ResidentKey {
            instance_id,
            segment_id,
        };
        if let Some(entry) = state.residents.get_mut(&key) {
            let old_bytes = entry.bytes;
            let old_kind = entry.kind;
            let was_evicting = entry.phase == ResidentPhase::Evicting;
            entry.bytes = residency.bytes;
            entry.kind = residency.kind;
            let _ = entry;

            if old_bytes != residency.bytes {
                if was_evicting {
                    if old_bytes > residency.bytes {
                        state.pending_evict_bytes = state
                            .pending_evict_bytes
                            .saturating_sub(old_bytes - residency.bytes);
                    } else {
                        state.pending_evict_bytes = state
                            .pending_evict_bytes
                            .saturating_add(residency.bytes - old_bytes);
                    }
                }
                if old_bytes > residency.bytes {
                    let delta = old_bytes - residency.bytes;
                    state.total_bytes = state.total_bytes.saturating_sub(delta);
                    if let Some(instance) = state.instances.get_mut(&instance_id) {
                        instance.bytes_used = instance.bytes_used.saturating_sub(delta);
                    }
                } else {
                    let delta = residency.bytes - old_bytes;
                    state.total_bytes = state.total_bytes.saturating_add(delta);
                    if let Some(instance) = state.instances.get_mut(&instance_id) {
                        instance.bytes_used = instance.bytes_used.saturating_add(delta);
                    }
                }
                self.metrics.update_total_bytes(state.total_bytes);
            }

            if old_kind != residency.kind {
                match residency.kind {
                    ResidencyKind::Sealed => Self::move_to_lru_tail(&mut state.lru, key),
                    ResidencyKind::Active => Self::remove_from_lru(&mut state.lru, key),
                }
            }
        }
    }

    fn request_activation(
        self: &Arc<Self>,
        instance_id: InstanceId,
        request: ActivationRequest,
    ) -> (ActivationOutcome, Vec<Tier0Eviction>) {
        let mut state = self.state.lock();
        let evictions = self.ensure_capacity_for(&mut state, request.required_bytes);
        let effective_total = state.total_bytes.saturating_sub(state.pending_evict_bytes);
        let available = self
            .config
            .cluster_max_bytes
            .saturating_sub(effective_total);
        if available >= request.required_bytes {
            trace!(
                instance = instance_id.get(),
                segment = request.segment_id.as_u64(),
                bytes = request.required_bytes,
                "Tier0 activation immediately granted"
            );
            (ActivationOutcome::Admitted, evictions)
        } else {
            let queued = QueuedActivation {
                instance_id,
                segment_id: request.segment_id,
                requested_bytes: request.required_bytes,
                priority: request.priority,
                reason: request.reason,
                sequence: self.queue_seq.fetch_add(1, AtomicOrdering::Relaxed),
            };
            if state.pending_activation.len() >= self.config.activation_queue_warn_threshold {
                debug!(
                    queue_depth = state.pending_activation.len(),
                    "Tier0 activation queue above warn threshold"
                );
            }
            state.pending_activation.push(queued);
            self.metrics
                .update_activation_depth(state.pending_activation.len());
            (ActivationOutcome::Queued, evictions)
        }
    }

    fn drain_evictions(&self) -> Vec<Tier0Eviction> {
        let mut state = self.state.lock();
        let drained: Vec<_> = state.eviction_queue.drain(..).collect();
        self.metrics.incr_evictions(drained.len());
        drained
    }

    fn drain_drop_events(&self) -> Vec<Tier0DropEvent> {
        let mut state = self.state.lock();
        let drained: Vec<_> = state.drop_events.drain(..).collect();
        self.metrics.incr_drop_events(drained.len());
        drained
    }

    fn drain_activation_grants(&self) -> Vec<ActivationGrant> {
        let mut state = self.state.lock();
        let drained: Vec<_> = state.activation_grants.drain(..).collect();
        self.metrics
            .update_activation_depth(state.pending_activation.len());
        drained
    }

    fn pending_activation_snapshot(&self) -> PendingActivationSnapshot {
        let state = self.state.lock();
        let queued = state
            .pending_activation
            .iter()
            .map(|item| QueuedActivationInfo {
                segment_id: item.segment_id,
                priority: item.priority,
                reason: item.reason,
                requested_bytes: item.requested_bytes,
            })
            .collect();
        PendingActivationSnapshot { queued }
    }

    fn segment_dropped(&self, key: ResidentKey, segment: Option<Arc<Segment>>) {
        let mut state = self.state.lock();
        state.handles.remove(&key);
        if let Some(entry) = state.residents.remove(&key) {
            let was_sealed = entry.kind == ResidencyKind::Sealed;
            tracing::event!(
                target: "aof2::tier0",
                tracing::Level::INFO,
                instance = key.instance_id.get(),
                segment = key.segment_id.as_u64(),
                bytes = entry.bytes,
                sealed = was_sealed,
                "tier0_segment_dropped"
            );
            state.total_bytes = state.total_bytes.saturating_sub(entry.bytes);
            if entry.phase == ResidentPhase::Evicting {
                state.pending_evict_bytes = state.pending_evict_bytes.saturating_sub(entry.bytes);
            }
            if let Some(instance) = state.instances.get_mut(&key.instance_id) {
                instance.bytes_used = instance.bytes_used.saturating_sub(entry.bytes);
            }
            Self::remove_from_lru(&mut state.lru, key);
            state.drop_events.push_back(Tier0DropEvent {
                instance_id: key.instance_id,
                segment_id: key.segment_id,
                bytes: entry.bytes,
                was_sealed,
                segment,
            });
            self.metrics.update_total_bytes(state.total_bytes);
            self.metrics.incr_drop_events(1);
            self.service_pending_locked(&mut state);
        }
    }

    fn enforce_capacity_locked(&self, state: &mut Tier0State) -> Vec<Tier0Eviction> {
        let mut scheduled = Vec::new();
        while state.total_bytes.saturating_sub(state.pending_evict_bytes)
            > self.config.cluster_max_bytes
        {
            if let Some(candidate) = self.select_evictable(state) {
                state.eviction_queue.push_back(candidate.clone());
                scheduled.push(candidate);
            } else {
                break;
            }
        }
        self.metrics.incr_evictions(scheduled.len());
        scheduled
    }

    fn ensure_capacity_for(
        &self,
        state: &mut Tier0State,
        required_bytes: u64,
    ) -> Vec<Tier0Eviction> {
        let mut scheduled = Vec::new();
        while state
            .total_bytes
            .saturating_sub(state.pending_evict_bytes)
            .saturating_add(required_bytes)
            > self.config.cluster_max_bytes
        {
            if let Some(candidate) = self.select_evictable(state) {
                state.eviction_queue.push_back(candidate.clone());
                scheduled.push(candidate);
            } else {
                break;
            }
        }
        self.metrics.incr_evictions(scheduled.len());
        scheduled
    }

    fn select_evictable(&self, state: &mut Tier0State) -> Option<Tier0Eviction> {
        while let Some(key) = state.lru.pop_front() {
            if let Some(entry) = state.residents.get_mut(&key) {
                if entry.kind == ResidencyKind::Sealed && entry.phase == ResidentPhase::Resident {
                    entry.phase = ResidentPhase::Evicting;
                    state.pending_evict_bytes =
                        state.pending_evict_bytes.saturating_add(entry.bytes);
                    tracing::event!(
                        target: "aof2::tier0",
                        tracing::Level::INFO,
                        instance = key.instance_id.get(),
                        segment = key.segment_id.as_u64(),
                        bytes = entry.bytes,
                        "tier0_eviction_scheduled"
                    );
                    return Some(Tier0Eviction {
                        instance_id: key.instance_id,
                        segment_id: key.segment_id,
                        bytes: entry.bytes,
                    });
                }
            }
        }
        None
    }

    fn move_to_lru_tail(queue: &mut VecDeque<ResidentKey>, key: ResidentKey) {
        if let Some(pos) = queue.iter().position(|entry| *entry == key) {
            queue.remove(pos);
            queue.push_back(key);
        }
    }

    fn remove_from_lru(queue: &mut VecDeque<ResidentKey>, key: ResidentKey) {
        if let Some(pos) = queue.iter().position(|entry| *entry == key) {
            queue.remove(pos);
        }
    }

    fn service_pending_locked(&self, state: &mut Tier0State) {
        let mut granted = Vec::new();
        loop {
            if let Some(top) = state.pending_activation.peek() {
                let effective_total = state.total_bytes.saturating_sub(state.pending_evict_bytes);
                let available = self
                    .config
                    .cluster_max_bytes
                    .saturating_sub(effective_total);
                if available < top.requested_bytes {
                    break;
                }
                let request = state.pending_activation.pop().expect("pop queued");
                granted.push(ActivationGrant {
                    instance_id: request.instance_id,
                    segment_id: request.segment_id,
                    bytes: request.requested_bytes,
                    priority: request.priority,
                    reason: request.reason,
                });
            } else {
                break;
            }
        }
        if !granted.is_empty() {
            trace!(count = granted.len(), "Tier0 granting queued activations");
        }
        for grant in granted {
            state.activation_grants.push_back(grant);
        }
        self.metrics
            .update_activation_depth(state.pending_activation.len());
    }

    fn bump_clock(&self, state: &mut Tier0State) -> u64 {
        state.access_clock = state.access_clock.wrapping_add(1);
        state.access_clock
    }
}

#[derive(Clone)]
struct QueuedActivation {
    instance_id: InstanceId,
    segment_id: SegmentId,
    requested_bytes: u64,
    priority: u32,
    reason: ActivationReason,
    sequence: u64,
}

impl PartialEq for QueuedActivation {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Eq for QueuedActivation {}

impl PartialOrd for QueuedActivation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedActivation {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AofConfig;
    use crate::fs::Layout;
    use tempfile::TempDir;

    fn build_segment(dir: &TempDir, id: u64, bytes: u64) -> Arc<Segment> {
        let config = AofConfig {
            root_dir: dir.path().to_path_buf(),
            ..AofConfig::default()
        }
        .normalized();
        let layout = Layout::new(&config);
        layout.ensure().expect("ensure layout");
        let file_name = format!("segment_{id}.seg");
        let path = dir.path().join("segments").join(file_name);
        Segment::create_active(SegmentId::new(id), 0, 0, 0, bytes, path.as_path())
            .map(Arc::new)
            .expect("create segment")
    }

    #[test]
    fn admission_tracks_bytes_and_drop_events() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = Tier0Cache::new(Tier0CacheConfig::new(2048, 2048));
        let instance = cache.register_instance("test", None);
        let seg1 = build_segment(&dir, 1, 512);
        let seg2 = build_segment(&dir, 2, 512);

        let (handle1, evictions1) =
            instance.admit_segment(seg1, SegmentResidency::new(512, ResidencyKind::Active));
        assert!(evictions1.is_empty());

        let (_handle2, evictions2) =
            instance.admit_segment(seg2, SegmentResidency::new(512, ResidencyKind::Active));
        assert!(evictions2.is_empty());

        assert_eq!(cache.metrics().total_bytes, 1024);

        drop(handle1);
        let drops = cache.drain_drop_events();
        assert_eq!(drops.len(), 1);
        assert_eq!(cache.metrics().total_bytes, 512);
    }

    #[test]
    fn capacity_enforcement_schedules_lru_evicts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = Tier0Cache::new(Tier0CacheConfig::new(512, 2048));
        let instance = cache.register_instance("test", None);
        let seg_a = build_segment(&dir, 1, 256);
        let seg_b = build_segment(&dir, 2, 256);
        let seg_c = build_segment(&dir, 3, 256);

        let (_a, _) =
            instance.admit_segment(seg_a, SegmentResidency::new(256, ResidencyKind::Sealed));
        let (_b, _) =
            instance.admit_segment(seg_b, SegmentResidency::new(256, ResidencyKind::Sealed));
        let (_c, evictions) =
            instance.admit_segment(seg_c, SegmentResidency::new(256, ResidencyKind::Sealed));

        assert_eq!(evictions.len(), 1);
        let eviction = &evictions[0];
        assert_eq!(eviction.segment_id.as_u64(), 1);
    }

    #[test]
    fn activation_queue_drains_when_space_frees() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = Tier0Cache::new(Tier0CacheConfig::new(512, 2048));
        let instance = cache.register_instance("test", None);
        let seg = build_segment(&dir, 1, 512);
        let (handle, _) =
            instance.admit_segment(seg, SegmentResidency::new(512, ResidencyKind::Active));

        let (outcome, _) = instance.request_activation(ActivationRequest {
            segment_id: SegmentId::new(42),
            required_bytes: 256,
            priority: 10,
            reason: ActivationReason::ReadMiss,
        });
        assert_eq!(outcome, ActivationOutcome::Queued);
        assert_eq!(cache.pending_activation_snapshot().queued.len(), 1);

        drop(handle);
        let grants = cache.drain_activation_grants();
        assert_eq!(grants.len(), 1);
        let grant = &grants[0];
        assert_eq!(grant.segment_id.as_u64(), 42);
        assert_eq!(grant.bytes, 256);
        assert_eq!(cache.pending_activation_snapshot().queued.len(), 0);
    }
}

