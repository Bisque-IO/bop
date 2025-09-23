use thiserror::Error;

/// Result alias for Raft operations.
pub type RaftResult<T> = Result<T, RaftError>;

/// Raft-specific errors surfaced by the safe wrapper layer.
#[derive(Debug, Error)]
pub enum RaftError {
    #[error("Null pointer returned from C API")]
    NullPointer,

    #[error("Invalid UTF-8 string: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    #[error("Invalid C string: {0}")]
    InvalidCString(#[from] std::ffi::NulError),

    #[error("Server operation failed")]
    ServerError,

    #[error("Raft command failed: {0}")]
    CommandError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("State machine error: {0}")]
    StateMachineError(String),

    #[error("Log store error: {0}")]
    LogStoreError(String),
}
