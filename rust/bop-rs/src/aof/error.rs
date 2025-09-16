/// A specialized error type for AOF operations.
#[derive(Debug, thiserror::Error)]
pub enum AofError {
    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The provided segment size is invalid.
    #[error("Invalid segment size: {0}")]
    InvalidSegmentSize(String),
    /// A record could not be parsed or is corrupted.
    #[error("Corrupted record: {0}")]
    CorruptedRecord(String),
    /// No current segment is available for writing.
    #[error("No current segment available for writing")]
    NoCurrentSegment,
    /// Failed to append to a new segment after rollover.
    #[error("Failed to append to new segment after rollover")]
    AppendToNewSegmentFailed,
    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
    /// Memory management error
    #[error("Memory management error: {0}")]
    MemoryManagement(String),
    /// File system operation error
    #[error("File system error: {0}")]
    FileSystem(String),
    /// Remote storage error
    #[error("Remote storage error: {0}")]
    RemoteStorage(String),
    /// Index operation error
    #[error("Index error: {0}")]
    IndexError(String),
    /// Compression/decompression error
    #[error("Compression error: {0}")]
    CompressionError(String),
    /// Invalid state transition or operation
    #[error("Invalid state: {0}")]
    InvalidState(String),
    /// Data corruption detected
    #[error("Data corruption: {0}")]
    Corruption(String),
    /// Segment is full and cannot accept more data
    #[error("Segment full: {0}")]
    SegmentFull(String),
    /// Record not found
    #[error("Record not found: {0}")]
    RecordNotFound(u64),
    /// AOF instance not found
    #[error("Instance not found: {0}")]
    InstanceNotFound(String),
    /// Operation would block (e.g., append when no active segment ready)
    #[error("Operation would block: {0}")]
    WouldBlock(String),
    /// Backpressure - system overloaded
    #[error("Backpressure: {0}")]
    Backpressure(String),
    /// Internal error (lock poisoning, etc.)
    #[error("Internal error: {0}")]
    InternalError(String),
    /// Segment not loaded
    #[error("Segment not loaded: {0}")]
    SegmentNotLoaded(String),
    /// A generic error occurred.
    #[error("Other error: {0}")]
    Other(String),
}

/// A Result type alias for AOF operations.
pub type AofResult<T> = Result<T, AofError>;
