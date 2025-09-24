use std::fmt::{Display, Formatter};

use super::config::{RecordId, SegmentId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressureKind {
    Admission,
    Rollover,
    Hydration,
    Flush,
    Tail,
    Unknown,
}

impl Display for BackpressureKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BackpressureKind::Admission => write!(f, "admission"),
            BackpressureKind::Rollover => write!(f, "rollover"),
            BackpressureKind::Hydration => write!(f, "hydration"),
            BackpressureKind::Flush => write!(f, "flush"),
            BackpressureKind::Tail => write!(f, "tail"),
            BackpressureKind::Unknown => write!(f, "unknown"),
        }
    }
}

/// A specialized error type for AOF operations.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum AofError {
    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The provided segment size is invalid.
    #[error("invalid segment size: expected {0}, found {1}")]
    InvalidSegmentSize(u64, u64),
    /// Configuration value was invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    /// A record could not be parsed or is corrupted.
    #[error("corrupted record: {0}")]
    CorruptedRecord(String),
    /// No current segment is available for writing.
    #[error("no current segment available for writing")]
    NoCurrentSegment,
    /// Failed to append to a new segment after rollover.
    #[error("failed to append to new segment after rollover")]
    AppendToNewSegmentFailed,
    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// Memory management error.
    #[error("memory management error: {0}")]
    MemoryManagement(String),
    /// File system operation error.
    #[error("file system error: {0}")]
    FileSystem(String),
    /// Remote storage error.
    #[error("remote storage error: {0}")]
    RemoteStorage(String),
    /// Index operation error.
    #[error("index error: {0}")]
    IndexError(String),
    /// Compression/decompression error.
    #[error("compression error: {0}")]
    CompressionError(String),
    /// Invalid state transition or operation.
    #[error("invalid state: {0}")]
    InvalidState(String),
    /// Data corruption detected.
    #[error("data corruption: {0}")]
    Corruption(String),
    /// Segment is full and cannot accept more data.
    #[error("segment full: {0}")]
    SegmentFull(u64),
    /// Rollover coordination failed.
    #[error("rollover failed: {0}")]
    RolloverFailed(String),
    /// Segment not loaded.
    #[error("segment not loaded: {0}")]
    SegmentNotLoaded(SegmentId),
    /// Record not found.
    #[error("record not found: {0}")]
    RecordNotFound(RecordId),
    /// AOF instance not found.
    #[error("instance not found: {0}")]
    InstanceNotFound(u64),
    /// Operation would block (e.g., append when no active segment ready).
    #[error("would block: {0}")]
    WouldBlock(BackpressureKind),
    /// Backpressure - system overloaded.
    #[error("backpressure")]
    Backpressure,
    /// Flush is pending durability and the append cannot proceed without waiting.
    #[error(
        "flush backpressure for record {record_id}: unflushed {pending_bytes} exceeds limit {limit}"
    )]
    FlushBackpressure {
        record_id: RecordId,
        pending_bytes: u64,
        limit: u64,
        target_logical: u32,
    },
    /// Flush request is already in flight.
    #[error("flush already in flight")]
    FlushInFlight,
    /// Internal error (lock poisoning, etc.).
    #[error("internal error: {0}")]
    InternalError(String),
    /// A generic error occurred.
    #[error("other error: {0}")]
    Other(String),
}

impl AofError {
    /// Create a would-block error annotated with the given backpressure kind.
    pub fn would_block(kind: BackpressureKind) -> Self {
        Self::WouldBlock(kind)
    }

    /// Create an invalid configuration error from a displayable value.
    pub fn invalid_config<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::InvalidConfig(msg.to_string())
    }

    /// Create an internal error from a displayable value.
    pub fn internal<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::InternalError(msg.to_string())
    }

    /// Create a rollover failure error from a displayable value.
    pub fn rollover_failed<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::RolloverFailed(msg.to_string())
    }

    /// Create an opaque error from a displayable value.
    pub fn other<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::Other(msg.to_string())
    }
}

/// A Result type alias for AOF operations.
pub type AofResult<T> = Result<T, AofError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_config_helper() {
        let err = AofError::invalid_config("bad path");
        assert!(matches!(err, AofError::InvalidConfig(msg) if msg == "bad path"));
    }
}
