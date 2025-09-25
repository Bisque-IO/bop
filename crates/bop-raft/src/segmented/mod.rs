//! Building blocks for segmented log store orchestration.
//!
//! This module exposes types that drive the segmented log/state-machine
//! architecture layered on top of the shared Raft log.

pub mod ack_tracker;
pub mod coordinator;
pub mod dispatcher;
pub mod log_store;
pub mod manifest;
pub mod manifest_store;
pub mod partition_store;
pub mod types;

pub use ack_tracker::{AckError, AckOutcome, SegmentedAckTracker};
pub use coordinator::{CoordinatorError, CoordinatorResult, SegmentedCoordinator};
pub use dispatcher::{AdmissionError, PartitionDispatcher, QuorumEvaluation, QuorumStatus};
pub use log_store::{PartitionPointer, SegmentedLogStore, SegmentedLogStoreConfig};
pub use manifest::{PartitionManifest, PartitionManifestUpdate};
pub use manifest_store::PartitionManifestStore;
pub use partition_store::PartitionLogStore;
pub use types::{
    GlobalLogIndex, PartitionDispatcherConfig, PartitionId, PartitionLogIndex,
    PartitionMasterPolicy, PartitionQueueConfig, PartitionQuorumRule, QuorumDecision, ReplicaId,
};
