//! Shared type definitions for the segmented log orchestrator.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;

/// Identifier for a partition/segment.
pub type PartitionId = u64;

/// Monotonic index scoped to a partition log.
pub type PartitionLogIndex = u64;

/// Index in the shared parent Raft log.
pub type GlobalLogIndex = u64;

/// Identifier for a replica participating in partition quorums.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReplicaId(pub u64);

impl From<u64> for ReplicaId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<ReplicaId> for u64 {
    fn from(id: ReplicaId) -> Self {
        id.0
    }
}

/// Identifier used to bind a partition to its state machine implementation.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PartitionStateMachineId(String);

impl PartitionStateMachineId {
    /// Construct a partition state machine identifier from an arbitrary string-like value.
    pub fn new<S: Into<String>>(value: S) -> Self {
        Self(value.into())
    }

    /// Borrow the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PartitionStateMachineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PartitionStateMachineId")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for PartitionStateMachineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for PartitionStateMachineId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for PartitionStateMachineId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Serialization version for segmented log record headers.
pub const SEGMENTED_RECORD_VERSION: u8 = 1;

/// Flags carried alongside each segmented log record.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SegmentedRecordFlags(u8);

impl SegmentedRecordFlags {
    pub const NONE: Self = Self(0);
    /// Payload is a partition-specific apply blob (default for replicated entries).
    pub const HAS_PAYLOAD: Self = Self(0b0000_0001);
    /// Record marks a logical partition snapshot boundary.
    pub const SNAPSHOT_MARKER: Self = Self(0b0000_0010);

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl fmt::Debug for SegmentedRecordFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SegmentedRecordFlags")
            .field("bits", &format_args!("0b{:08b}", self.0))
            .finish()
    }
}

/// Fixed prefix encoded ahead of every segmented AOF record payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentedRecordHeader {
    pub version: u8,
    pub flags: SegmentedRecordFlags,
    pub partition_id: PartitionId,
    pub partition_index: PartitionLogIndex,
    pub raft_index: GlobalLogIndex,
    pub payload_len: u32,
    pub checksum: Option<u32>,
}

impl SegmentedRecordHeader {
    pub fn new(
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        raft_index: GlobalLogIndex,
        payload_len: u32,
    ) -> Self {
        Self {
            version: SEGMENTED_RECORD_VERSION,
            flags: if payload_len > 0 {
                SegmentedRecordFlags::HAS_PAYLOAD
            } else {
                SegmentedRecordFlags::NONE
            },
            partition_id,
            partition_index,
            raft_index,
            payload_len,
            checksum: None,
        }
    }

    pub fn with_flags(mut self, flags: SegmentedRecordFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_checksum(mut self, checksum: Option<u32>) -> Self {
        self.checksum = checksum;
        self
    }
}

/// User-supplied quorum predicate for custom policies.
pub type QuorumFn =
    dyn Fn(&HashSet<ReplicaId>, &HashSet<ReplicaId>) -> bool + Send + Sync + 'static;

/// Policy controlling how a partition selects its state machine master.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionMasterPolicy {
    /// Do not assign a master for this partition.
    Disabled,
    /// Always assign the specified replica when it is part of the cluster membership.
    Fixed(ReplicaId),
    /// Select a master automatically from the current cluster membership set.
    Automatic,
}

/// Strategy used to determine when a quorum is satisfied.
#[derive(Clone)]
pub enum QuorumDecision {
    Majority,
    Threshold { count: NonZeroUsize },
    Custom(Arc<QuorumFn>),
}

impl std::fmt::Debug for QuorumDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuorumDecision::Majority => f.write_str("Majority"),
            QuorumDecision::Threshold { count } => {
                f.debug_struct("Threshold").field("count", count).finish()
            }
            QuorumDecision::Custom(_) => f.write_str("Custom"),
        }
    }
}

/// Per-partition quorum policy describing explicit members and evaluation semantics.
#[derive(Clone)]
pub struct PartitionQuorumRule {
    members: HashSet<ReplicaId>,
    decision: QuorumDecision,
}

impl PartitionQuorumRule {
    /// Construct a quorum rule from explicit members and a decision policy.
    pub fn new(members: HashSet<ReplicaId>, decision: QuorumDecision) -> Self {
        Self { members, decision }
    }

    /// Convenience helper for majority-based rules.
    pub fn majority<I>(members: I) -> Self
    where
        I: IntoIterator<Item = ReplicaId>,
    {
        Self::new(members.into_iter().collect(), QuorumDecision::Majority)
    }

    /// Convenience helper for threshold-based rules.
    pub fn threshold<I>(members: I, count: NonZeroUsize) -> Self
    where
        I: IntoIterator<Item = ReplicaId>,
    {
        let members: HashSet<ReplicaId> = members.into_iter().collect();
        assert!(
            count.get() <= members.len(),
            "quorum threshold cannot exceed membership size"
        );
        Self::new(members, QuorumDecision::Threshold { count })
    }

    /// Returns a rule that accepts immediately (no explicit membership).
    pub fn allow_all() -> Self {
        Self::new(HashSet::new(), QuorumDecision::Majority)
    }

    /// Members participating in the quorum.
    pub fn members(&self) -> &HashSet<ReplicaId> {
        &self.members
    }

    /// Decision logic for the quorum.
    pub fn decision(&self) -> &QuorumDecision {
        &self.decision
    }

    /// Checks whether a replica is part of this quorum rule.
    pub fn contains(&self, replica: &ReplicaId) -> bool {
        self.members.contains(replica)
    }

    /// Total number of members declared in this rule.
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Minimum acknowledgements required to satisfy the quorum, when known.
    pub fn required_for_quorum(&self) -> Option<usize> {
        match &self.decision {
            QuorumDecision::Majority => {
                if self.members.is_empty() {
                    Some(0)
                } else {
                    Some((self.members.len() / 2) + 1)
                }
            }
            QuorumDecision::Threshold { count } => Some(count.get()),
            QuorumDecision::Custom(_) => None,
        }
    }

    /// Evaluate the quorum rule against the supplied acknowledgement set.
    pub fn evaluate(&self, acknowledged: &HashSet<ReplicaId>) -> bool {
        if self.members.is_empty() {
            // No explicit membership implies quorum is immediately satisfied.
            return true;
        }

        let acknowledged_members = acknowledged
            .iter()
            .filter(|replica| self.members.contains(*replica))
            .count();

        match &self.decision {
            QuorumDecision::Majority => acknowledged_members >= ((self.members.len() / 2) + 1),
            QuorumDecision::Threshold { count } => acknowledged_members >= count.get(),
            QuorumDecision::Custom(predicate) => predicate(acknowledged, &self.members),
        }
    }
}

impl std::fmt::Debug for PartitionQuorumRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartitionQuorumRule")
            .field("members", &self.members)
            .field("decision", &self.decision)
            .finish()
    }
}

impl Default for PartitionQuorumRule {
    fn default() -> Self {
        Self::allow_all()
    }
}

/// Composite key identifying a partition log entry that is awaiting acknowledgements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartitionAckKey {
    pub partition_id: PartitionId,
    pub partition_index: PartitionLogIndex,
}

impl PartitionAckKey {
    pub const fn new(partition_id: PartitionId, partition_index: PartitionLogIndex) -> Self {
        Self {
            partition_id,
            partition_index,
        }
    }
}

/// Snapshot of acknowledgement state for a partition log index.
#[derive(Debug, Clone)]
pub struct PartitionAckStatus {
    pub key: PartitionAckKey,
    pub global_index: GlobalLogIndex,
    pub acknowledgements: HashSet<ReplicaId>,
    pub quorum_satisfied: bool,
    pub required_for_quorum: Option<usize>,
}

impl PartitionAckStatus {
    /// Create a new acknowledgement tracker seeded with the target quorum definition.
    pub fn new(
        key: PartitionAckKey,
        global_index: GlobalLogIndex,
        quorum: &PartitionQuorumRule,
    ) -> Self {
        let required_for_quorum = quorum.required_for_quorum();
        let quorum_satisfied = required_for_quorum == Some(0);
        Self {
            key,
            global_index,
            acknowledgements: HashSet::new(),
            quorum_satisfied,
            required_for_quorum,
        }
    }

    /// Total acknowledgements recorded so far.
    pub fn ack_count(&self) -> usize {
        self.acknowledgements.len()
    }
}

/// Configuration governing how much work a partition may queue before the dispatcher
/// begins rejecting new entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionQueueConfig {
    /// Maximum number of queued-but-unapplied entries allowed for the partition.
    pub max_inflight_entries: NonZeroUsize,
    /// Optional bound on how far behind the partition may lag the parent Raft log.
    pub max_global_lag: Option<GlobalLogIndex>,
}

impl PartitionQueueConfig {
    /// Construct a new queue configuration.
    pub fn new(max_inflight_entries: NonZeroUsize, max_global_lag: Option<GlobalLogIndex>) -> Self {
        Self {
            max_inflight_entries,
            max_global_lag,
        }
    }
}

impl Default for PartitionQueueConfig {
    fn default() -> Self {
        Self {
            max_inflight_entries: NonZeroUsize::new(128).expect("non-zero"),
            max_global_lag: None,
        }
    }
}

/// Minimum metadata required to configure a partition under the segmented pipeline.
#[derive(Debug, Clone)]
pub struct PartitionDescriptor {
    pub partition_id: PartitionId,
    pub state_machine: PartitionStateMachineId,
    pub queue: PartitionQueueConfig,
    pub quorum: PartitionQuorumRule,
    pub master_policy: PartitionMasterPolicy,
}

impl PartitionDescriptor {
    pub fn new(
        partition_id: PartitionId,
        state_machine: PartitionStateMachineId,
        queue: PartitionQueueConfig,
        quorum: PartitionQuorumRule,
        master_policy: PartitionMasterPolicy,
    ) -> Self {
        Self {
            partition_id,
            state_machine,
            queue,
            quorum,
            master_policy,
        }
    }
}

/// Metadata persisted alongside each partition-specific bop-aof stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionAofMetadata {
    pub partition_id: PartitionId,
    pub highest_durable_index: PartitionLogIndex,
    pub next_partition_index: PartitionLogIndex,
    pub last_applied_raft_index: GlobalLogIndex,
}

impl PartitionAofMetadata {
    pub const fn new(
        partition_id: PartitionId,
        highest_durable_index: PartitionLogIndex,
        next_partition_index: PartitionLogIndex,
        last_applied_raft_index: GlobalLogIndex,
    ) -> Self {
        Self {
            partition_id,
            highest_durable_index,
            next_partition_index,
            last_applied_raft_index,
        }
    }

    pub fn mark_durable(&mut self, partition_index: PartitionLogIndex, raft_index: GlobalLogIndex) {
        if partition_index > self.highest_durable_index {
            self.highest_durable_index = partition_index;
        }
        if partition_index >= self.next_partition_index {
            self.next_partition_index = partition_index + 1;
        }
        if raft_index > self.last_applied_raft_index {
            self.last_applied_raft_index = raft_index;
        }
    }
}

/// Dispatcher configuration capturing default and per-partition queue/quorum/master policies.
#[derive(Debug, Clone)]
pub struct PartitionDispatcherConfig {
    pub default_queue: PartitionQueueConfig,
    pub queue_overrides: HashMap<PartitionId, PartitionQueueConfig>,
    pub default_quorum: PartitionQuorumRule,
    pub quorum_overrides: HashMap<PartitionId, PartitionQuorumRule>,
    pub default_master: PartitionMasterPolicy,
    pub master_overrides: HashMap<PartitionId, PartitionMasterPolicy>,
}

impl PartitionDispatcherConfig {
    pub fn new(default_queue: PartitionQueueConfig) -> Self {
        Self {
            default_queue,
            queue_overrides: HashMap::new(),
            default_quorum: PartitionQuorumRule::default(),
            quorum_overrides: HashMap::new(),
            default_master: PartitionMasterPolicy::Disabled,
            master_overrides: HashMap::new(),
        }
    }

    pub fn with_override(mut self, partition_id: PartitionId, cfg: PartitionQueueConfig) -> Self {
        self.queue_overrides.insert(partition_id, cfg);
        self
    }

    pub fn with_queue_override(
        mut self,
        partition_id: PartitionId,
        cfg: PartitionQueueConfig,
    ) -> Self {
        self.queue_overrides.insert(partition_id, cfg);
        self
    }

    pub fn override_for(&self, partition_id: PartitionId) -> Option<PartitionQueueConfig> {
        self.queue_override_for(partition_id)
    }

    pub fn queue_override_for(&self, partition_id: PartitionId) -> Option<PartitionQueueConfig> {
        self.queue_overrides.get(&partition_id).copied()
    }

    pub fn with_quorum_override(
        mut self,
        partition_id: PartitionId,
        rule: PartitionQuorumRule,
    ) -> Self {
        self.quorum_overrides.insert(partition_id, rule);
        self
    }

    pub fn quorum_rule_for(&self, partition_id: PartitionId) -> PartitionQuorumRule {
        self.quorum_overrides
            .get(&partition_id)
            .cloned()
            .unwrap_or_else(|| self.default_quorum.clone())
    }

    pub fn with_master_override(
        mut self,
        partition_id: PartitionId,
        policy: PartitionMasterPolicy,
    ) -> Self {
        self.master_overrides.insert(partition_id, policy);
        self
    }

    pub fn master_policy_for(&self, partition_id: PartitionId) -> PartitionMasterPolicy {
        self.master_overrides
            .get(&partition_id)
            .copied()
            .unwrap_or(self.default_master)
    }
}

impl Default for PartitionDispatcherConfig {
    fn default() -> Self {
        Self {
            default_queue: PartitionQueueConfig::default(),
            queue_overrides: HashMap::new(),
            default_quorum: PartitionQuorumRule::default(),
            quorum_overrides: HashMap::new(),
            default_master: PartitionMasterPolicy::Disabled,
            master_overrides: HashMap::new(),
        }
    }
}

/// Durability checkpoint for a partition log flush event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionDurability {
    pub partition_id: PartitionId,
    pub partition_index: PartitionLogIndex,
    pub raft_index: GlobalLogIndex,
}

impl PartitionDurability {
    pub const fn new(
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        raft_index: GlobalLogIndex,
    ) -> Self {
        Self {
            partition_id,
            partition_index,
            raft_index,
        }
    }
}

/// Per-partition watermark describing the lowest Raft index still required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionWatermark {
    pub partition_id: PartitionId,
    pub min_required_raft_index: GlobalLogIndex,
}

impl PartitionWatermark {
    pub const fn new(partition_id: PartitionId, min_required_raft_index: GlobalLogIndex) -> Self {
        Self {
            partition_id,
            min_required_raft_index,
        }
    }
}

/// Aggregated truncation hints derived from segmented + partition durability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentedWatermarks {
    pub segmented_durable_index: GlobalLogIndex,
    pub min_required_raft_index: GlobalLogIndex,
}

impl SegmentedWatermarks {
    pub const fn new(
        segmented_durable_index: GlobalLogIndex,
        min_required_raft_index: GlobalLogIndex,
    ) -> Self {
        Self {
            segmented_durable_index,
            min_required_raft_index,
        }
    }

    /// Returns true when the segmented log can safely truncate at least one entry.
    pub const fn can_truncate(&self) -> bool {
        self.segmented_durable_index > self.min_required_raft_index
    }
}
