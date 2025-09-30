//! Error types and codes for bop-storage operations.
//!
//! This module provides structured error handling with error codes for programmatic
//! error handling and context preservation for debugging.

use std::fmt;
use thiserror::Error;

/// Error codes for major failure classes in bop-storage.
///
/// These codes enable programmatic error handling and monitoring/alerting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// I/O operation failed (disk, network, etc.)
    IoFailure,
    /// I/O operation timed out
    IoTimeout,
    /// Manifest database inconsistency detected
    ManifestInconsistency,
    /// S3 or remote storage operation failed
    S3Failure,
    /// S3 or remote storage operation timed out
    S3Timeout,
    /// Write operation failed
    WriteFailure,
    /// Flush operation failed
    FlushFailure,
    /// Resource exhaustion (memory, disk space, file descriptors)
    ResourceExhaustion,
    /// Configuration error
    ConfigurationError,
    /// Worker thread panic
    WorkerPanic,
    /// Retry limit exceeded
    RetryLimitExceeded,
    /// Concurrency control violation
    ConcurrencyViolation,
    /// Data corruption detected
    DataCorruption,
    /// Operation cancelled or aborted
    OperationCancelled,
    /// Unknown or unclassified error
    Unknown,
}

impl ErrorCode {
    /// Returns a string identifier for this error code.
    ///
    /// Useful for metrics, logging, and monitoring systems.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::IoFailure => "io_failure",
            ErrorCode::IoTimeout => "io_timeout",
            ErrorCode::ManifestInconsistency => "manifest_inconsistency",
            ErrorCode::S3Failure => "s3_failure",
            ErrorCode::S3Timeout => "s3_timeout",
            ErrorCode::WriteFailure => "write_failure",
            ErrorCode::FlushFailure => "flush_failure",
            ErrorCode::ResourceExhaustion => "resource_exhaustion",
            ErrorCode::ConfigurationError => "configuration_error",
            ErrorCode::WorkerPanic => "worker_panic",
            ErrorCode::RetryLimitExceeded => "retry_limit_exceeded",
            ErrorCode::ConcurrencyViolation => "concurrency_violation",
            ErrorCode::DataCorruption => "data_corruption",
            ErrorCode::OperationCancelled => "operation_cancelled",
            ErrorCode::Unknown => "unknown",
        }
    }

    /// Returns a human-readable description of this error code.
    pub fn description(&self) -> &'static str {
        match self {
            ErrorCode::IoFailure => "I/O operation failed",
            ErrorCode::IoTimeout => "I/O operation timed out",
            ErrorCode::ManifestInconsistency => "Manifest database inconsistency detected",
            ErrorCode::S3Failure => "S3 or remote storage operation failed",
            ErrorCode::S3Timeout => "S3 or remote storage operation timed out",
            ErrorCode::WriteFailure => "Write operation failed",
            ErrorCode::FlushFailure => "Flush operation failed",
            ErrorCode::ResourceExhaustion => "Resource exhaustion (memory, disk, etc.)",
            ErrorCode::ConfigurationError => "Configuration error",
            ErrorCode::WorkerPanic => "Worker thread panic",
            ErrorCode::RetryLimitExceeded => "Retry limit exceeded",
            ErrorCode::ConcurrencyViolation => "Concurrency control violation",
            ErrorCode::DataCorruption => "Data corruption detected",
            ErrorCode::OperationCancelled => "Operation cancelled or aborted",
            ErrorCode::Unknown => "Unknown or unclassified error",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A wrapper that adds error code and contextual information to errors.
///
/// This type preserves the original error while adding classification
/// and context for better debugging and monitoring.
#[derive(Debug, Error)]
pub struct ErrorWithContext<E> {
    /// The error code classifying this error
    pub code: ErrorCode,
    /// Additional context about the error
    pub context: String,
    /// The underlying error
    #[source]
    pub source: E,
}

impl<E: fmt::Display> fmt::Display for ErrorWithContext<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}: {}",
            self.code.as_str(),
            self.context,
            self.source
        )
    }
}

/// Extension trait for adding context and error codes to Result types.
pub trait ResultExt<T, E> {
    /// Add an error code and context string to this result.
    fn with_error_context(
        self,
        code: ErrorCode,
        context: impl Into<String>,
    ) -> Result<T, ErrorWithContext<E>>;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn with_error_context(
        self,
        code: ErrorCode,
        context: impl Into<String>,
    ) -> Result<T, ErrorWithContext<E>> {
        self.map_err(|source| ErrorWithContext {
            code,
            context: context.into(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_as_str() {
        assert_eq!(ErrorCode::IoFailure.as_str(), "io_failure");
        assert_eq!(ErrorCode::IoTimeout.as_str(), "io_timeout");
        assert_eq!(
            ErrorCode::ManifestInconsistency.as_str(),
            "manifest_inconsistency"
        );
    }

    #[test]
    fn error_code_display() {
        assert_eq!(format!("{}", ErrorCode::IoFailure), "io_failure");
        assert_eq!(format!("{}", ErrorCode::S3Timeout), "s3_timeout");
    }

    #[test]
    fn error_with_context_display() {
        use std::io;
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let wrapped = ErrorWithContext {
            code: ErrorCode::IoFailure,
            context: "failed to open WAL segment".to_string(),
            source: io_err,
        };
        let display = format!("{}", wrapped);
        assert!(display.contains("io_failure"));
        assert!(display.contains("failed to open WAL segment"));
        assert!(display.contains("file not found"));
    }

    #[test]
    fn result_ext_with_error_context() {
        use std::io;
        let result: Result<(), io::Error> = Err(io::Error::new(io::ErrorKind::TimedOut, "timeout"));

        let with_context =
            result.with_error_context(ErrorCode::IoTimeout, "write operation timed out after 30s");

        assert!(with_context.is_err());
        let err = with_context.unwrap_err();
        assert_eq!(err.code, ErrorCode::IoTimeout);
        assert_eq!(err.context, "write operation timed out after 30s");
    }
}
