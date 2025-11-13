use std::collections::HashMap;
use std::fmt;

use crate::buffer::Buffer;
use crate::config::{ClusterConfig, ClusterConfigView};
use crate::error::{RaftError, RaftResult};
use crate::log_entry::{LogEntryRecord, LogEntryView};
use crate::state::{ServerState, ServerStateView};
use crate::storage::StorageBackendKind;
use crate::types::{CallbackAction, CallbackContext, LogIndex, LogLevel, ServerId, Term};
use maniac_sys::bop_raft_snapshot;

/// Snapshot type forwarded from the NuRaft layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotType(pub u8);

/// Chunk of snapshot data streamed by NuRaft.
#[derive(Clone, Copy)]
pub struct SnapshotMetadata<'a> {
    pub log_idx: LogIndex,
    pub log_term: Term,
    pub cluster_config: Option<ClusterConfigView<'a>>,
    pub snapshot_size: u64,
    pub snapshot_type: SnapshotType,
}

impl<'a> fmt::Debug for SnapshotMetadata<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let config_summary = self.cluster_config.map(|cfg| {
            (
                cfg.log_idx(),
                cfg.prev_log_idx(),
                cfg.servers_size(),
                cfg.is_async_replication(),
            )
        });
        f.debug_struct("SnapshotMetadata")
            .field("log_idx", &self.log_idx)
            .field("log_term", &self.log_term)
            .field("snapshot_size", &self.snapshot_size)
            .field("snapshot_type", &self.snapshot_type)
            .field("cluster_config", &config_summary)
            .finish()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotChunk<'a> {
    pub metadata: SnapshotMetadata<'a>,
    pub is_first_chunk: bool,
    pub is_last_chunk: bool,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotCreation<'a> {
    pub metadata: SnapshotMetadata<'a>,
    pub data: &'a [u8],
    pub snapshot: Option<SnapshotRef>,
}

/// Handle returned to NuRaft when it requests the most recent snapshot.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotRef {
    ptr: *mut bop_raft_snapshot,
}

impl SnapshotRef {
    /// Create a borrowed snapshot reference from a raw pointer.
    ///
    /// # Safety
    /// Caller must guarantee the pointer is valid for the duration required by NuRaft.
    pub unsafe fn new(ptr: *mut bop_raft_snapshot) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(SnapshotRef { ptr })
        }
    }

    pub fn as_ptr(self) -> *mut bop_raft_snapshot {
        self.ptr
    }
}

/// Streaming result for snapshot reads.
#[derive(Debug)]
pub enum SnapshotReadResult {
    Chunk { data: Vec<u8>, is_last: bool },
    End,
}

/// Snapshot reader returned by the state machine implementation.
pub trait SnapshotReader: Send {
    fn next(&mut self, object_id: u64) -> RaftResult<SnapshotReadResult>;
}

/// Collected commit index adjustment data passed from NuRaft.
#[derive(Debug, Clone)]
pub struct AdjustCommitIndex {
    pub current: LogIndex,
    pub expected: LogIndex,
    pub peer_indexes: HashMap<ServerId, LogIndex>,
}

impl AdjustCommitIndex {
    pub fn new(
        current: LogIndex,
        expected: LogIndex,
        peer_indexes: HashMap<ServerId, LogIndex>,
    ) -> Self {
        Self {
            current,
            expected,
            peer_indexes,
        }
    }
}

/// State Machine Interface for Raft.
/// Applications must implement this trait to integrate with the consensus engine.
pub trait StateMachine: Send {
    /// Apply a log entry to the state machine.
    ///
    /// Returning `Ok(Some(Buffer))` delivers a result payload back to the
    /// caller. Returning `Ok(None)` indicates no payload should be sent.
    fn apply(&mut self, log_idx: LogIndex, data: &[u8]) -> RaftResult<Option<Buffer>>;

    /// Notification that a cluster configuration entry was committed.
    fn commit_config(
        &mut self,
        _log_idx: LogIndex,
        _new_config: ClusterConfigView<'_>,
    ) -> RaftResult<()> {
        Ok(())
    }

    /// Pre-commit stage invoked before replication quorum is reached.
    fn pre_commit(&mut self, _log_idx: LogIndex, _data: &[u8]) -> RaftResult<Option<Buffer>> {
        Ok(None)
    }

    /// Roll back an uncommitted entry.
    fn rollback(&mut self, _log_idx: LogIndex, _data: &[u8]) -> RaftResult<()> {
        Ok(())
    }

    /// Roll back a pending configuration change.
    fn rollback_config(
        &mut self,
        _log_idx: LogIndex,
        _config: ClusterConfigView<'_>,
    ) -> RaftResult<()> {
        Ok(())
    }

    /// Create a snapshot of the current state.
    fn take_snapshot(&self) -> RaftResult<Buffer>;

    /// Restore state from a snapshot.
    fn restore_from_snapshot(&mut self, snapshot: &[u8]) -> RaftResult<()>;

    /// Get the last applied log index.
    fn last_applied_index(&self) -> LogIndex;

    /// Preferred outbound log batch size hint.
    fn next_batch_size_hint(&self) -> i64 {
        0
    }

    /// Persist a snapshot chunk supplied by NuRaft.
    fn save_snapshot_chunk(&mut self, _chunk: SnapshotChunk<'_>) -> RaftResult<()> {
        Ok(())
    }

    /// Apply a received snapshot.
    fn apply_snapshot(&mut self, _snapshot: SnapshotMetadata<'_>) -> RaftResult<bool> {
        Ok(true)
    }

    /// Provide a snapshot reader for streaming snapshot data.
    fn snapshot_reader(&mut self) -> RaftResult<Box<dyn SnapshotReader>> {
        Err(RaftError::StateMachineError(
            "snapshot streaming not implemented".to_string(),
        ))
    }

    /// Clean up resources associated with a previously created snapshot reader.
    fn finalize_snapshot_reader(&mut self, _reader: Box<dyn SnapshotReader>) -> RaftResult<()> {
        Ok(())
    }

    /// Return the last durable snapshot, if available.
    fn last_snapshot(&self) -> RaftResult<Option<SnapshotRef>> {
        Ok(None)
    }

    /// Execute snapshot creation for the provided snapshot data.
    fn create_snapshot(&mut self, _snapshot: SnapshotCreation<'_>) -> RaftResult<()> {
        Ok(())
    }

    /// Whether NuRaft should attempt to create a snapshot now.
    fn should_create_snapshot(&self) -> bool {
        true
    }

    /// Whether this state machine allows the server to transfer leadership.
    fn allow_leadership_transfer(&self) -> bool {
        true
    }

    /// Adjust the commit index before NuRaft finalizes it.
    fn adjust_commit_index(&mut self, info: AdjustCommitIndex) -> RaftResult<LogIndex> {
        Ok(info.expected)
    }
}

/// Log Store Interface for Raft.
/// Defines the interface for persistent log storage.
pub trait LogStoreInterface {
    /// Preferred backend for this implementation.
    fn storage_backend(&self) -> StorageBackendKind {
        StorageBackendKind::Callbacks
    }

    /// Index of the first entry stored.
    fn start_index(&self) -> LogIndex;

    /// Next slot that will be assigned to an appended entry.
    fn next_slot(&self) -> LogIndex;

    /// Get the last entry stored, if any.
    fn last_entry(&self) -> RaftResult<Option<LogEntryRecord>>;

    /// Append a single entry and return its index.
    fn append(&mut self, entry: LogEntryView<'_>) -> RaftResult<LogIndex>;

    /// Write/update an entry at the given index.
    fn write_at(&mut self, idx: LogIndex, entry: LogEntryView<'_>) -> RaftResult<()>;

    /// Log store hint that a batch append finished.
    fn end_of_append_batch(&mut self, start: LogIndex, count: u64) -> RaftResult<()>;

    /// Return entries in the half-open range `[start, end)`.
    fn log_entries(&self, start: LogIndex, end: LogIndex) -> RaftResult<Vec<LogEntryRecord>>;

    /// Return the entry at `idx`, if present.
    fn entry_at(&self, idx: LogIndex) -> RaftResult<Option<LogEntryRecord>>;

    /// Return the term stored at `idx`, if any.
    fn term_at(&self, idx: LogIndex) -> RaftResult<Option<Term>>;

    /// Pack entries for efficient transfer.
    fn pack(&self, idx: LogIndex, count: i32) -> RaftResult<Buffer>;

    /// Apply a packed blob at the given index.
    fn apply_pack(&mut self, idx: LogIndex, pack: &[u8]) -> RaftResult<()>;

    /// Compact the log up to and including `last_log_index`.
    fn compact(&mut self, last_log_index: LogIndex) -> RaftResult<bool>;

    /// Schedule asynchronous compaction.
    fn compact_async(&mut self, last_log_index: LogIndex) -> RaftResult<bool>;

    /// Flush durable state to storage.
    fn flush(&mut self) -> RaftResult<bool>;

    /// Last index durably persisted.
    fn last_durable_index(&self) -> LogIndex;
}

/// State Manager Interface for Raft.
/// Defines the interface for managing persistent server state.
pub trait StateManagerInterface: Send {
    /// Preferred backend for this implementation.
    fn storage_backend(&self) -> StorageBackendKind {
        StorageBackendKind::Callbacks
    }

    /// Load the server state.
    fn load_state(&self) -> RaftResult<Option<ServerState>>;

    /// Save the server state.
    fn save_state(&mut self, state: ServerStateView<'_>) -> RaftResult<()>;

    /// Load the cluster configuration.
    fn load_config(&self) -> RaftResult<Option<ClusterConfig>>;

    /// Save the cluster configuration.
    fn save_config(&mut self, config: ClusterConfigView<'_>) -> RaftResult<()>;

    /// Optionally supply a log store implementation to the server.
    fn load_log_store(&self) -> RaftResult<Option<Box<dyn LogStoreInterface>>> {
        Ok(None)
    }

    /// Return the ID of the server managed by this state manager.
    fn server_id(&self) -> ServerId {
        ServerId::new(0)
    }

    /// Handle system exit callbacks from the underlying engine.
    fn system_exit(&self, _code: i32) {}
}

/// Application-provided logger implementation.
pub trait Logger: Send + Sync {
    fn log(&self, level: LogLevel, source: &str, func: &str, line: usize, message: &str);
}

/// Observer for Raft server events emitted from NuRaft.
pub trait ServerCallbacks: Send + Sync {
    fn handle_event(&self, context: CallbackContext<'_>) -> RaftResult<CallbackAction>;
}
