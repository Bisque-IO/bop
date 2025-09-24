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

/// Unique identifier for AOF instances within the tiered storage system.
///
/// Each AOF instance gets a unique ID that is used for coordination,
/// resource allocation, and metrics tracking across all storage tiers.
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

/// Classification of segment residency in Tier 0 cache.
///
/// Different residency types have different eviction priorities and
/// access patterns, influencing cache management decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidencyKind {
    /// Active segment currently accepting writes
    ///
    /// Highest priority for cache retention. Active segments should
    /// rarely be evicted as they are in active use.
    Active,

    /// Sealed segment that is read-only
    ///
    /// Lower eviction priority than active segments but higher than
    /// archived segments. May be evicted under memory pressure.
    Sealed,
}

/// Residency information for a segment in Tier 0 cache.
///
/// Tracks both the memory footprint and residency classification
/// for cache management decisions.
#[derive(Debug, Clone)]
pub struct SegmentResidency {
    /// Number of bytes the segment occupies in memory
    pub bytes: u64,
    /// Classification determining eviction priority
    pub kind: ResidencyKind,
}

impl SegmentResidency {
    pub fn new(bytes: u64, kind: ResidencyKind) -> Self {
        Self { bytes, kind }
    }
}

/// Reasons why a segment activation was requested.
///
/// Different activation reasons may have different priorities
/// and affect cache admission policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationReason {
    /// Segment was accessed but not in cache
    ReadMiss,
    /// Proactive loading based on access patterns
    Prefetch,
    /// Follower node catching up to leader
    FollowerCatchUp,
    /// Loading segments during crash recovery
    Recovery,
}

/// Request to activate (load) a segment into Tier 0 cache.
///
/// Activation requests may be queued if insufficient capacity exists.
/// Higher priority values are serviced first.
#[derive(Debug, Clone)]
pub struct ActivationRequest {
    /// Segment to activate
    pub segment_id: SegmentId,
    /// Estimated memory requirement
    pub required_bytes: u64,
    /// Activation priority (higher = more urgent)
    pub priority: u32,
    /// Why the activation was requested
    pub reason: ActivationReason,
}

/// Result of submitting an activation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationOutcome {
    /// Request was immediately granted (sufficient capacity)
    Admitted,
    /// Request was queued (insufficient capacity)
    Queued,
}

/// Grant authorizing segment activation.
///
/// Issued when a queued activation request can be satisfied.
/// The recipient should load the segment into cache promptly.
#[derive(Debug, Clone)]
pub struct ActivationGrant {
    /// Instance that requested the activation
    pub instance_id: InstanceId,
    /// Segment approved for activation
    pub segment_id: SegmentId,
    /// Approved memory allocation
    pub bytes: u64,
    /// Original request priority
    pub priority: u32,
    /// Original activation reason
    pub reason: ActivationReason,
}

/// Eviction notice for a segment scheduled to leave Tier 0.
///
/// The cache has decided this segment should be evicted to free space.
/// The owning instance should handle the eviction gracefully.
#[derive(Debug, Clone)]
pub struct Tier0Eviction {
    /// Instance owning the segment
    pub instance_id: InstanceId,
    /// Segment to be evicted
    pub segment_id: SegmentId,
    /// Memory that will be freed
    pub bytes: u64,
}

/// Event indicating a segment was dropped from Tier 0.
///
/// Fired when the last reference to a resident segment is dropped,
/// either due to eviction or natural cleanup.
#[derive(Clone)]
pub struct Tier0DropEvent {
    /// Instance that owned the segment
    pub instance_id: InstanceId,
    /// Dropped segment ID
    pub segment_id: SegmentId,
    /// Memory freed by the drop
    pub bytes: u64,
    /// Whether the segment was sealed when dropped
    pub was_sealed: bool,
    /// Optional reference to the dropped segment
    pub segment: Option<Arc<Segment>>,
}

/// Snapshot of pending activation requests.
///
/// Used for observability and debugging of queue state.
#[derive(Debug, Clone)]
pub struct PendingActivationSnapshot {
    /// Currently queued activation requests
    pub queued: Vec<QueuedActivationInfo>,
}

/// Information about a queued activation request.
///
/// Subset of ActivationRequest data for observability.
#[derive(Debug, Clone)]
pub struct QueuedActivationInfo {
    /// Segment waiting for activation
    pub segment_id: SegmentId,
    /// Request priority
    pub priority: u32,
    /// Why activation was requested
    pub reason: ActivationReason,
    /// Memory requirement
    pub requested_bytes: u64,
}

/// Configuration for the Tier 0 in-memory cache.
///
/// Controls memory limits, quotas, and queue behavior.
#[derive(Debug, Clone, Copy)]
pub struct Tier0CacheConfig {
    /// Maximum bytes allowed across all instances
    pub cluster_max_bytes: u64,
    /// Default per-instance memory quota
    pub default_instance_quota: u64,
    /// Queue depth that triggers warnings
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

/// Point-in-time metrics snapshot for Tier 0 cache.
///
/// Provides observability into cache behavior and performance.
#[derive(Debug, Default, Clone, Copy)]
pub struct Tier0MetricsSnapshot {
    /// Total bytes currently cached
    pub total_bytes: u64,
    /// Number of pending activation requests
    pub activation_queue_depth: u32,
    /// Total evictions scheduled (lifetime)
    pub scheduled_evictions: u64,
    /// Total segment drop events (lifetime)
    pub drop_events: u64,
}

/// Thread-safe metrics collection for Tier 0 cache.
///
/// Uses atomic operations for lock-free metric updates
/// from concurrent cache operations.
#[derive(Debug, Default)]
pub struct Tier0Metrics {
    /// Current bytes in cache
    total_bytes: AtomicU64,
    /// Current activation queue depth
    activation_queue_depth: AtomicU32,
    /// Lifetime count of scheduled evictions
    scheduled_evictions: AtomicU64,
    /// Lifetime count of segment drops
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

/// The Tier 0 in-memory cache for the tiered storage system.
///
/// Tier 0 provides the fastest access tier, keeping frequently accessed
/// segments in memory with LRU-based eviction. It coordinates admission
/// control, eviction scheduling, and activation queuing.
///
/// ## Architecture
///
/// ```text
/// ┌─────────────────────────────────────────────┐
/// │              Tier0Cache                     │
/// ├─────────────────────────────────────────────┤
/// │ • Admission Control                         │
/// │ • LRU Eviction                              │
/// │ • Activation Queuing                        │
/// │ • Instance Isolation                        │
/// └─────────────────────────────────────────────┘
/// ```
///
/// ## Thread Safety
///
/// All operations are thread-safe. Multiple instances can concurrently:
/// - Admit new segments
/// - Access existing segments
/// - Request activations
/// - Process evictions
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

/// Per-instance view of the Tier 0 cache.
///
/// Each AOF instance gets its own Tier0Instance for isolated
/// cache operations while sharing the underlying memory pool.
///
/// ## Usage Pattern
///
/// ```ignore
/// // Admit a segment to cache
/// let (resident, evictions) = instance.admit_segment(segment, residency);
///
/// // Use an admission guard for automatic residency tracking
/// let guard = instance.admission_guard(resident);
///
/// // Guard automatically updates residency as segment grows
/// guard.record_append(new_bytes);
/// guard.mark_sealed(); // When segment becomes read-only
/// ```
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

/// A segment that is resident in Tier 0 cache.
///
/// Provides access to the underlying segment while tracking its
/// residency in the cache. When dropped, automatically notifies
/// the cache system for cleanup.
///
/// ## Lifecycle
///
/// 1. Created when a segment is admitted to cache
/// 2. Can be wrapped in an AdmissionGuard for automatic tracking
/// 3. Dropped when evicted or no longer needed
/// 4. Drop triggers cache cleanup and potential activation grants
#[derive(Clone)]
pub struct ResidentSegment {
    inner: Arc<ResidentSegmentInner>,
}

impl ResidentSegment {
    /// Creates new resident segment with drop tracking.
    ///
    /// Wraps segment with cleanup coordination to ensure proper
    /// resource management when segments are evicted from Tier 0.
    /// Drop guard handles cleanup notifications to parent cache.
    fn new(segment: Arc<Segment>, drop_guard: SegmentDropGuard) -> Self {
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

/// Guard for automatic residency tracking of cache-resident segments.
///
/// Monitors segment size changes and residency state transitions,
/// automatically updating the cache's accounting. Particularly useful
/// for active segments that grow during writes.
///
/// ## Automatic Updates
///
/// - `record_append()`: Updates size for growing active segments
/// - `mark_sealed()`: Transitions from Active to Sealed residency
/// - `update_residency()`: Manual residency updates
///
/// ## Thread Safety
///
/// Uses atomic operations for lock-free residency tracking.
/// Multiple threads can safely call update methods concurrently.
#[derive(Clone)]
pub struct AdmissionGuard {
    inner: Arc<AdmissionGuardInner>,
}

/// Internal state for admission guard tracking.
struct AdmissionGuardInner {
    /// Connection back to the cache instance
    tier0: Tier0Instance,
    /// The guarded resident segment
    resident: ResidentSegment,
    /// Current size tracking (atomic for thread safety)
    bytes: AtomicU64,
    /// Current residency kind (atomic, encoded as u8)
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
        if prev_kind == new_kind {
            if prev_bytes == residency.bytes {
                return;
            }
            if new_kind == encode_kind(ResidencyKind::Active) {
                return;
            }
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

/// Internal state for resident segments.
struct ResidentSegmentInner {
    /// The actual segment data
    segment: Arc<Segment>,
    /// Cleanup handler for when segment is dropped
    drop_guard: SegmentDropGuard,
}

impl Drop for ResidentSegmentInner {
    fn drop(&mut self) {
        self.drop_guard.on_drop();
    }
}

/// Cleanup handler that fires when a resident segment is dropped.
///
/// Uses weak references to avoid circular dependencies while ensuring
/// proper cache cleanup when segments are no longer referenced.
struct SegmentDropGuard {
    /// Cache key for the dropped segment
    key: ResidentKey,
    /// Weak reference to cache internals
    inner: Weak<Tier0Inner>,
    /// Weak reference to the segment (for drop events)
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

/// Internal key for tracking resident segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ResidentKey {
    /// Instance owning the segment
    instance_id: InstanceId,
    /// Unique segment identifier
    segment_id: SegmentId,
}

/// Lifecycle phase of a resident segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResidentPhase {
    /// Normal residency, accessible for reads/writes
    Resident,
    /// Eviction scheduled, no longer accepting new references
    Evicting,
}

/// Internal tracking entry for cached segments.
struct ResidentEntry {
    /// Current memory footprint
    bytes: u64,
    /// Residency classification
    kind: ResidencyKind,
    /// Current lifecycle phase
    phase: ResidentPhase,
    /// Last access timestamp for LRU tracking
    last_touch_tick: u64,
}

/// Per-instance cache state tracking.
struct InstanceState {
    /// Human-readable instance name
    #[allow(dead_code)]
    name: String,
    /// Total bytes currently used by this instance
    bytes_used: u64,
}

/// Central state for the Tier 0 cache.
///
/// All mutable state is protected by a single mutex to ensure
/// consistency across concurrent operations.
struct Tier0State {
    /// Total bytes across all resident segments
    total_bytes: u64,
    /// Bytes in segments marked for eviction
    pending_evict_bytes: u64,
    /// Monotonic clock for LRU tracking
    access_clock: u64,
    /// Per-instance state tracking
    instances: HashMap<InstanceId, InstanceState>,
    /// All currently resident segments
    residents: HashMap<ResidentKey, ResidentEntry>,
    /// Weak references to segment handles
    handles: HashMap<ResidentKey, Weak<ResidentSegmentInner>>,
    /// LRU queue for sealed segments (eviction candidates)
    lru: VecDeque<ResidentKey>,
    /// Scheduled evictions waiting to be processed
    eviction_queue: VecDeque<Tier0Eviction>,
    /// Drop events waiting to be processed
    drop_events: VecDeque<Tier0DropEvent>,
    /// Activation requests waiting for capacity
    pending_activation: BinaryHeap<QueuedActivation>,
    /// Approved activation grants ready for processing
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

/// Core implementation of Tier 0 cache.
///
/// Coordinates all cache operations including admission, eviction,
/// activation queuing, and instance management.
struct Tier0Inner {
    /// Cache configuration and limits
    config: Tier0CacheConfig,
    /// Performance and observability metrics
    metrics: Tier0Metrics,
    /// Instance ID generation counter
    next_instance: AtomicU64,
    /// Sequence number for activation queue ordering
    queue_seq: AtomicU64,
    /// All mutable cache state (protected by mutex)
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

    fn register_instance(&self, name: String, _quota_override: Option<u64>) -> InstanceId {
        let instance_id = InstanceId::new(self.next_instance.fetch_add(1, AtomicOrdering::Relaxed));
        let mut state = self.state.lock();
        state.instances.insert(
            instance_id,
            InstanceState {
                name,
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

    /// Enforces capacity limits by selecting segments for eviction.
    ///
    /// When memory usage exceeds cluster limits, identifies segments
    /// for eviction using LRU policy. Schedules eviction operations
    /// and updates metrics for monitoring.
    ///
    /// Core capacity management to prevent memory exhaustion.
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

    /// Selects next segment for eviction using LRU policy.
    ///
    /// Scans LRU queue to find sealed segments eligible for eviction.
    /// Updates segment phase to prevent concurrent access and tracks
    /// pending eviction bytes for capacity calculations.
    ///
    /// Implements LRU eviction strategy for memory management.
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

    /// Services pending activation requests when capacity is available.
    ///
    /// Processes queued segment activation requests, granting access
    /// when sufficient memory capacity exists. Uses priority-based
    /// ordering and accounts for pending evictions in capacity calculations.
    ///
    /// Core admission control logic for Tier 0 memory management.
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

/// Internal representation of a queued activation request.
///
/// Ordered by priority (higher first), then by sequence number (FIFO).
/// Used in a binary heap for efficient priority queue operations.
#[derive(Clone)]
struct QueuedActivation {
    /// Instance that made the request
    instance_id: InstanceId,
    /// Segment to activate
    segment_id: SegmentId,
    /// Memory requirement
    requested_bytes: u64,
    /// Request priority (higher = more urgent)
    priority: u32,
    /// Why activation was requested
    reason: ActivationReason,
    /// Sequence number for FIFO ordering within priority
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
