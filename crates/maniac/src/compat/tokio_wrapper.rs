//! Re-exports of tokio types for compat module

// When tokio-compat feature is enabled, use the renamed tokio-io dependency
#[cfg(feature = "tokio-compat")]
pub use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

// In tests, use the tokio from dev-dependencies
#[cfg(all(test, not(feature = "tokio-compat")))]
pub use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
