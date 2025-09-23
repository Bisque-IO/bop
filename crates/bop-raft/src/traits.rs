use crate::buffer::Buffer;
use crate::config::{ClusterConfig, ClusterConfigView};
use crate::error::RaftResult;
use crate::log_entry::{LogEntryRecord, LogEntryView};
use crate::state::{ServerState, ServerStateView};
use crate::types::{CallbackAction, CallbackContext, LogIndex, LogLevel, ServerId, Term};

/// State Machine Interface for Raft.
/// Applications must implement this trait to integrate with the consensus engine.
pub trait StateMachine: Send {
    /// Apply a log entry to the state machine.
    ///
    /// Returning `Ok(Some(Vec<u8>))` delivers a result payload back to the
    /// caller. Returning `Ok(None)` indicates no payload should be sent.
    fn apply(&mut self, log_idx: LogIndex, data: &[u8]) -> RaftResult<Option<Vec<u8>>>;

    /// Create a snapshot of the current state.
    fn take_snapshot(&self) -> RaftResult<Buffer>;

    /// Restore state from a snapshot.
    fn restore_from_snapshot(&mut self, snapshot: &[u8]) -> RaftResult<()>;

    /// Get the last applied log index.
    fn last_applied_index(&self) -> LogIndex;

    /// Whether this state machine allows the server to transfer leadership.
    fn allow_leadership_transfer(&self) -> bool {
        true
    }
}

/// Log Store Interface for Raft.
/// Defines the interface for persistent log storage.
pub trait LogStoreInterface {
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
