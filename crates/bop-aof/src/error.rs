use std::fmt::{Display, Formatter};

use super::config::{RecordId, SegmentId};

/// Types of backpressure that can occur in the AOF system.
///
/// Backpressure indicates which subsystem is currently limiting throughput,
/// helping with both debugging and adaptive behavior in client applications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressureKind {
    /// Admission control is limiting new writes to manage memory usage.
    ///
    /// Occurs when the tiered storage system determines it cannot safely
    /// admit more data without risking memory exhaustion or performance
    /// degradation.
    Admission,

    /// Segment rollover is in progress, temporarily blocking writes.
    ///
    /// Short-duration backpressure that occurs when transitioning from
    /// a full segment to a new one. Usually resolves quickly.
    Rollover,

    /// Data hydration from lower tiers is blocking the operation.
    ///
    /// Occurs when needed data must be retrieved from slower storage
    /// tiers before the operation can proceed.
    Hydration,

    /// Flush pipeline is overwhelmed and cannot accept more data.
    ///
    /// The durability subsystem is falling behind write load, indicating
    /// I/O bandwidth limitations or storage performance issues.
    Flush,

    /// Tail operations are blocked due to coordination issues.
    ///
    /// Occurs when tail following or live read operations cannot keep
    /// up with write throughput or encounter synchronization delays.
    Tail,

    /// Unknown or unclassified backpressure source.
    ///
    /// Fallback category for backpressure that doesn't fit other types.
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
///
/// Provides detailed error information for all AOF subsystems, enabling
/// precise error handling and diagnostic capabilities. Each variant includes
/// context-specific information to aid in debugging and recovery.
///
/// # Design Principles
///
/// - **Structured errors**: Each variant has semantic meaning and appropriate context
/// - **Non-exhaustive**: New error types can be added without breaking changes
/// - **Actionable**: Error messages and types guide appropriate responses
/// - **Hierarchical**: Related errors are grouped for easier handling
///
/// # Error Categories
///
/// - **I/O errors**: File system and storage-related failures
/// - **Validation errors**: Configuration and input validation failures
/// - **State errors**: Invalid operations or consistency violations
/// - **Resource errors**: Memory, space, or capacity limitations
/// - **Backpressure errors**: Flow control and load management
///
/// # Example Usage
///
/// ```rust
/// use bop_aof::{AofError, BackpressureKind};
///
/// match error {
///     AofError::WouldBlock(BackpressureKind::Flush) => {
///         // Retry after brief delay
///     }
///     AofError::SegmentFull(_) => {
///         // Trigger segment rollover
///     }
///     AofError::Corruption(msg) => {
///         // Log corruption and attempt recovery
///     }
///     _ => {
///         // Generic error handling
///     }
/// }
/// ```
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum AofError {
    /// An I/O error occurred during file system operations.
    ///
    /// Wraps standard I/O errors from file operations, network I/O, or
    /// other system-level failures. Usually indicates hardware issues,
    /// permission problems, or resource exhaustion.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The provided segment size is invalid.
    ///
    /// Occurs when segment size validation fails, typically during
    /// configuration validation or segment creation. The expected
    /// and actual sizes are provided for debugging.
    #[error("invalid segment size: expected {0}, found {1}")]
    InvalidSegmentSize(u64, u64),

    /// Configuration value was invalid.
    ///
    /// Indicates that a configuration parameter is outside acceptable
    /// bounds or incompatible with other settings. The error message
    /// provides specific details about the invalid configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// A record could not be parsed or is corrupted.
    ///
    /// Indicates data corruption, invalid record formats, or checksum
    /// mismatches. This error suggests potential storage issues or
    /// software bugs that require investigation.
    #[error("corrupted record: {0}")]
    CorruptedRecord(String),

    /// No current segment is available for writing.
    ///
    /// Occurs when an append operation is attempted but no active
    /// segment exists. May indicate initialization issues or problems
    /// with segment allocation.
    #[error("no current segment available for writing")]
    NoCurrentSegment,

    /// Failed to append to a new segment after rollover.
    ///
    /// A critical error indicating that segment rollover completed
    /// but the subsequent append failed. This may leave the AOF
    /// in an inconsistent state requiring recovery.
    #[error("failed to append to new segment after rollover")]
    AppendToNewSegmentFailed,
    /// Serialization error occurred during data encoding/decoding.
    ///
    /// Indicates failures in converting data structures to/from their
    /// on-disk or wire representations. May suggest version compatibility
    /// issues or data format corruption.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Memory management error occurred.
    ///
    /// Indicates issues with memory allocation, memory mapping, or
    /// other memory-related operations. May suggest resource exhaustion
    /// or system-level memory management problems.
    #[error("memory management error: {0}")]
    MemoryManagement(String),

    /// File system operation error.
    ///
    /// Covers file system operations beyond basic I/O, such as
    /// directory creation, file metadata operations, or atomic
    /// file operations.
    #[error("file system error: {0}")]
    FileSystem(String),

    /// Remote storage error occurred.
    ///
    /// Indicates failures in Tier 2 remote storage operations,
    /// such as network timeouts, authentication failures, or
    /// remote service unavailability.
    #[error("remote storage error: {0}")]
    RemoteStorage(String),

    /// Index operation error.
    ///
    /// Indicates failures in index creation, maintenance, or
    /// queries. May suggest index corruption or compatibility
    /// issues with index formats.
    #[error("index error: {0}")]
    IndexError(String),

    /// Compression/decompression error occurred.
    ///
    /// Indicates failures in data compression or decompression
    /// operations, potentially due to corrupted compressed data
    /// or unsupported compression formats.
    #[error("compression error: {0}")]
    CompressionError(String),

    /// Invalid state transition or operation.
    ///
    /// Indicates that an operation was attempted when the system
    /// was in an inappropriate state, such as writing to a
    /// finalized segment or reading from an uninitialized AOF.
    #[error("invalid state: {0}")]
    InvalidState(String),

    /// Data corruption detected.
    ///
    /// Indicates corruption beyond individual records, such as
    /// segment header corruption, manifest corruption, or
    /// structural integrity violations.
    #[error("data corruption: {0}")]
    Corruption(String),

    /// Segment is full and cannot accept more data.
    ///
    /// Indicates that a segment has reached its capacity limit.
    /// The segment size is included for debugging purposes.
    /// Usually triggers automatic segment rollover.
    #[error("segment full: {0}")]
    SegmentFull(u64),

    /// Rollover coordination failed.
    ///
    /// Indicates that the segment rollover process encountered
    /// an error, which may leave the system in an inconsistent
    /// state requiring recovery or intervention.
    #[error("rollover failed: {0}")]
    RolloverFailed(String),

    /// Segment not loaded in memory.
    ///
    /// Indicates that a required segment is not currently
    /// resident in the cache hierarchy and cannot be loaded.
    /// May trigger hydration or indicate storage issues.
    #[error("segment not loaded: {0}")]
    SegmentNotLoaded(SegmentId),

    /// Record not found at the specified location.
    ///
    /// Indicates that a record lookup failed, either because
    /// the record ID is invalid or the record has been
    /// deleted/compacted away.
    #[error("record not found: {0}")]
    RecordNotFound(RecordId),

    /// AOF instance not found.
    ///
    /// Indicates that an operation was attempted on a non-existent
    /// or already-destroyed AOF instance. Usually indicates a
    /// programming error or lifecycle management issue.
    #[error("instance not found: {0}")]
    InstanceNotFound(u64),

    /// Operation would block due to backpressure.
    ///
    /// Indicates that an operation cannot proceed immediately
    /// due to system load or resource constraints. The specific
    /// backpressure type provides guidance on the limiting factor.
    #[error("would block: {0}")]
    WouldBlock(BackpressureKind),

    /// Generic backpressure indication.
    ///
    /// A general backpressure signal without specific subsystem
    /// information. Clients should implement retry logic or
    /// adaptive rate limiting.
    #[error("backpressure")]
    Backpressure,
    /// Flush backpressure prevents append due to durability lag.
    ///
    /// Occurs when the amount of unflushed data exceeds configured limits,
    /// requiring the caller to wait for durability to catch up. Provides
    /// detailed information about the backpressure condition.
    #[error(
        "flush backpressure for record {record_id}: unflushed {pending_bytes} exceeds limit {limit}"
    )]
    FlushBackpressure {
        /// The record ID that triggered the backpressure
        record_id: RecordId,
        /// Current number of unflushed bytes
        pending_bytes: u64,
        /// Maximum allowed unflushed bytes
        limit: u64,
        /// Target logical position for flush completion
        target_logical: u32,
    },

    /// Flush request is already in flight for the target.
    ///
    /// Indicates that a flush operation is already in progress for
    /// the specified target, preventing duplicate flush requests.
    /// Callers should wait for the existing flush to complete.
    #[error("flush already in flight")]
    FlushInFlight,

    /// Internal error occurred (lock poisoning, etc.).
    ///
    /// Indicates internal consistency violations, lock poisoning,
    /// or other programming errors that suggest bugs in the AOF
    /// implementation rather than environmental issues.
    #[error("internal error: {0}")]
    InternalError(String),

    /// A generic error occurred that doesn't fit other categories.
    ///
    /// Catch-all for errors that don't have specific types yet
    /// or that represent unusual failure modes. Should be avoided
    /// in favor of more specific error types when possible.
    #[error("other error: {0}")]
    Other(String),
}

impl AofError {
    /// Create a would-block error annotated with the given backpressure kind.
    ///
    /// Convenience method for creating backpressure errors with specific
    /// subsystem information. Helps with diagnostic and retry logic.
    ///
    /// # Arguments
    ///
    /// * `kind` - The type of backpressure causing the block
    ///
    /// # Example
    ///
    /// ```rust
    /// use bop_aof::{AofError, BackpressureKind};
    ///
    /// let error = AofError::would_block(BackpressureKind::Flush);
    /// ```
    pub fn would_block(kind: BackpressureKind) -> Self {
        Self::WouldBlock(kind)
    }

    /// Create an invalid configuration error from a displayable value.
    ///
    /// Convenience method for creating configuration validation errors
    /// with descriptive messages.
    ///
    /// # Arguments
    ///
    /// * `msg` - Description of the invalid configuration
    pub fn invalid_config<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::InvalidConfig(msg.to_string())
    }

    /// Create an internal error from a displayable value.
    ///
    /// Convenience method for creating internal consistency errors,
    /// typically used for assertion failures or unexpected conditions.
    ///
    /// # Arguments
    ///
    /// * `msg` - Description of the internal error condition
    pub fn internal<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::InternalError(msg.to_string())
    }

    /// Create a rollover failure error from a displayable value.
    ///
    /// Convenience method for creating segment rollover errors with
    /// contextual information about the failure.
    ///
    /// # Arguments
    ///
    /// * `msg` - Description of the rollover failure
    pub fn rollover_failed<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::RolloverFailed(msg.to_string())
    }

    /// Create a generic error from a displayable value.
    ///
    /// Convenience method for creating catch-all errors. Should be
    /// avoided in favor of more specific error types when possible.
    ///
    /// # Arguments
    ///
    /// * `msg` - Description of the error condition
    pub fn other<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::Other(msg.to_string())
    }
}

/// A Result type alias for AOF operations.
///
/// Convenient shorthand for `Result<T, AofError>` used throughout the AOF
/// codebase. Provides consistent error handling patterns and reduces
/// boilerplate in function signatures.
///
/// # Example
///
/// ```rust
/// use bop_aof::{AofResult, AofError};
///
/// fn append_data(data: &[u8]) -> AofResult<u64> {
///     if data.is_empty() {
///         return Err(AofError::invalid_config("empty data"));
///     }
///     Ok(data.len() as u64)
/// }
/// ```
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
