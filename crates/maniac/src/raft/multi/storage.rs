//! Storage traits for Multi-Raft
//!
//! The storage layer is designed to multiplex multiple raft groups:
//! - A single LogStorage instance can handle entries for multiple groups
//! - Multiple LogStorage instances can be used to shard groups
//! - State machines are external - each group provides its own

use maniac_raft::OptionalSend;
use maniac_raft::OptionalSync;
use maniac_raft::RaftTypeConfig;
use maniac_raft::storage::RaftLogStorage;

/// Trait for a storage backend that provides log storage for multiple Raft groups.
///
/// Each storage instance can handle multiple groups. The system can have
/// multiple storage instances to shard groups across different backends.
///
/// State machines are NOT part of this trait - they are provided externally
/// per group since different groups may have completely different state machines.
pub trait MultiRaftLogStorage<C>: OptionalSend + OptionalSync + 'static
where
    C: RaftTypeConfig,
{
    /// The log storage type for individual groups
    type GroupLogStorage: RaftLogStorage<C>;

    /// Get the log storage for a specific group.
    ///
    /// This returns a handle to the log storage for a single group.
    /// Multiple calls with the same group_id should return handles
    /// that share the same underlying state.
    fn get_log_storage(&self, group_id: u64) -> Self::GroupLogStorage;

    /// Remove a group from this storage.
    ///
    /// Called when a group is deleted or moved to another storage.
    fn remove_group(&self, group_id: u64);

    /// Get list of all group IDs currently in this storage.
    fn group_ids(&self) -> Vec<u64>;
}
