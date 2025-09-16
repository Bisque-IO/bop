//! Archive storage abstraction for different backends
//!
//! This module provides the ArchiveStorage trait for pluggable storage backends.
//! The actual segment lifecycle management is handled by SegmentStore.

use crate::aof::error::AofResult;

/// Archive storage abstraction for different backends
pub trait ArchiveStorage: Send + Sync {
    /// Store a compressed segment in the archive
    async fn store_segment(
        &self,
        segment_path: &str,
        compressed_data: &[u8],
        archive_key: &str,
    ) -> AofResult<()>;

    /// Retrieve a compressed segment from the archive
    async fn retrieve_segment(&self, archive_key: &str) -> AofResult<Vec<u8>>;

    /// Delete a segment from the archive
    async fn delete_segment(&self, archive_key: &str) -> AofResult<()>;

    /// Check if a segment exists in the archive
    async fn segment_exists(&self, archive_key: &str) -> AofResult<bool>;

    /// Store the segment index
    async fn store_index(&self, index_data: &[u8]) -> AofResult<()>;

    /// Retrieve the segment index
    async fn retrieve_index(&self) -> AofResult<Option<Vec<u8>>>;
}
