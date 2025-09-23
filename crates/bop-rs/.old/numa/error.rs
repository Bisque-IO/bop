//! NUMA-specific error types and error handling

use std::fmt;

use crate::numa::NumaNodeId;

/// NUMA operation errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumaError {
    /// NUMA subsystem is not available on this system
    NumaNotAvailable,

    /// Invalid configuration provided
    InvalidConfig(String),

    /// Failed to detect NUMA topology
    TopologyDetectionFailed(String),

    /// Failed to set thread affinity
    AffinitySetFailed(String),

    /// Failed to get thread affinity
    AffinityGetFailed(String),

    /// Invalid NUMA node ID
    InvalidNodeId(u32),

    /// Invalid CPU ID
    InvalidCpuId(u32),

    /// Memory allocation failed
    AllocationFailed(String),

    /// System call failed
    SystemCallFailed(String),

    /// Permission denied for operation
    PermissionDenied(String),

    /// Feature not supported on this platform
    NotSupported(String),

    /// NUMA subsystem explicitly disabled
    NumaDisabled,

    /// NUMA subsystem already initialized
    AlreadyInitialized(String),

    /// NUMA subsystem not initialized
    NotInitialized(String),

    /// No NUMA nodes available
    NoNodesAvailable,

    /// I/O error occurred
    IoError(String),

    /// Parse error
    ParseError(String),

    /// Invalid Node
    InvalidNode(NumaNodeId),

    /// Other unexpected error
    Other(String),
}

impl fmt::Display for NumaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NumaError::NumaNotAvailable => {
                write!(f, "NUMA subsystem is not available on this system")
            }
            NumaError::InvalidConfig(msg) => write!(f, "Invalid NUMA configuration: {}", msg),
            NumaError::TopologyDetectionFailed(msg) => {
                write!(f, "Failed to detect NUMA topology: {}", msg)
            }
            NumaError::AffinitySetFailed(msg) => {
                write!(f, "Failed to set thread affinity: {}", msg)
            }
            NumaError::AffinityGetFailed(msg) => {
                write!(f, "Failed to get thread affinity: {}", msg)
            }
            NumaError::InvalidNodeId(id) => write!(f, "Invalid NUMA node ID: {}", id),
            NumaError::InvalidCpuId(id) => write!(f, "Invalid CPU ID: {}", id),
            NumaError::AllocationFailed(msg) => write!(f, "Memory allocation failed: {}", msg),
            NumaError::SystemCallFailed(msg) => write!(f, "System call failed: {}", msg),
            NumaError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            NumaError::NotSupported(msg) => write!(f, "Feature not supported: {}", msg),
            NumaError::NumaDisabled => write!(f, "NUMA subsystem is explicitly disabled"),
            NumaError::AlreadyInitialized(msg) => {
                write!(f, "NUMA subsystem already initialized: {}", msg)
            }
            NumaError::NotInitialized(msg) => write!(f, "NUMA subsystem not initialized: {}", msg),
            NumaError::NoNodesAvailable => write!(f, "No NUMA nodes available"),
            NumaError::IoError(msg) => write!(f, "I/O error: {}", msg),
            NumaError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            NumaError::InvalidNode(msg) => write!(f, "Invalid Node error: {}", msg),
            NumaError::Other(msg) => write!(f, "Unexpected error: {}", msg),
        }
    }
}

impl std::error::Error for NumaError {}

/// Result type for NUMA operations
pub type NumaResult<T> = Result<T, NumaError>;

// Conversions from standard error types
impl From<std::io::Error> for NumaError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::PermissionDenied => NumaError::PermissionDenied(err.to_string()),
            std::io::ErrorKind::NotFound => NumaError::NumaNotAvailable,
            std::io::ErrorKind::Unsupported => NumaError::NotSupported(err.to_string()),
            _ => NumaError::IoError(err.to_string()),
        }
    }
}

impl From<std::num::ParseIntError> for NumaError {
    fn from(err: std::num::ParseIntError) -> Self {
        NumaError::ParseError(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for NumaError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        NumaError::ParseError(err.to_string())
    }
}

impl From<std::str::Utf8Error> for NumaError {
    fn from(err: std::str::Utf8Error) -> Self {
        NumaError::ParseError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = NumaError::NumaNotAvailable;
        assert_eq!(
            error.to_string(),
            "NUMA subsystem is not available on this system"
        );

        let error = NumaError::InvalidConfig("test message".to_string());
        assert_eq!(
            error.to_string(),
            "Invalid NUMA configuration: test message"
        );

        let error = NumaError::InvalidNodeId(999);
        assert_eq!(error.to_string(), "Invalid NUMA node ID: 999");
    }

    #[test]
    fn test_error_from_io() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test");
        let numa_error: NumaError = io_error.into();
        assert!(matches!(numa_error, NumaError::PermissionDenied(_)));

        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let numa_error: NumaError = io_error.into();
        assert_eq!(numa_error, NumaError::NumaNotAvailable);
    }

    #[test]
    fn test_error_from_parse() {
        let parse_error = "invalid".parse::<u32>().unwrap_err();
        let numa_error: NumaError = parse_error.into();
        assert!(matches!(numa_error, NumaError::ParseError(_)));
    }

    #[test]
    fn test_result_type() {
        let result: NumaResult<i32> = Ok(42);
        assert!(result.is_ok());

        let result: NumaResult<i32> = Err(NumaError::NumaNotAvailable);
        assert!(result.is_err());
    }
}
