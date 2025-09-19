//! AOF (Append-Only File) implementation for high-performance logging and data persistence.
//!
//! This module provides a modular, efficient append-only file system with support for
//! multiple storage backends, indexing strategies, and flush policies.

pub mod archive;
pub mod archive_fs;
pub mod archive_s3;
pub mod error;
pub mod filesystem;
pub mod flush;
pub mod index;
pub mod manager;
pub mod reader;
pub mod record;
pub mod s3;
pub mod segment;
pub mod segment_index;
pub mod sqlite_segment_index;
pub mod segment_store;

// Re-export the main types and traits
pub use archive::ArchiveStorage;
pub use archive_fs::FilesystemArchiveStorage;
pub use archive_s3::{S3ArchiveStorage, S3Config};
pub use error::{AofError, AofResult};
pub use filesystem::{
    AsyncFileHandle, AsyncFileSystem, FileHandle, FileMetadata, FileSystem, FileSystemMetrics,
    LocalFileSystem,
};
pub use flush::{FlushController, FlushControllerMetrics};
pub use index::{SegmentFooter, SerializableIndex};
pub use reader::{
    AofMetrics, AofSegmentReader, BinaryIndex, BinaryIndexEntry, Reader, ReaderInternal,
    SegmentPosition,
};
pub use record::{
    AofConfig, AofConfigBuilder, FlushStrategy, Record, RecordHeader, SegmentMetadata,
    current_timestamp,
};
pub use s3::{S3Client, S3ObjectMetadata};
pub use segment::Segment;
pub use segment_index::MdbxSegmentIndex;
pub use sqlite_segment_index::SqliteSegmentIndex;
pub use segment_store::{SegmentEntry, SegmentStatus};

// Main AOF implementation
pub mod aof;
pub use aof::{Aof, AofPerformanceMetrics};

// AOF Manager for multiple instances
pub use manager::{
    AofInstanceConfig, AofManager, AofManagerBuilder, AofManagerMetrics, SelectionStrategy,
};

/// Convenience function to create a new AOF with default configuration (requires archive storage)
pub async fn create_aof<FS: FileSystem + 'static, A: ArchiveStorage + 'static>(
    fs: FS,
    archive_storage: std::sync::Arc<A>,
) -> AofResult<Aof<FS, A>> {
    Aof::open_with_fs_and_config(fs, archive_storage, AofConfig::default()).await
}

/// Convenience function to create a new AOF with custom configuration
pub async fn create_aof_with_config<FS: FileSystem + 'static, A: ArchiveStorage + 'static>(
    fs: FS,
    archive_storage: std::sync::Arc<A>,
    config: AofConfig,
) -> AofResult<Aof<FS, A>> {
    Aof::open_with_fs_and_config(fs, archive_storage, config).await
}

/// Convenience function to create a new AOF Manager with default configuration
pub fn create_aof_manager<
    FS: FileSystem + Send + Sync + Clone + 'static,
    A: ArchiveStorage + Send + Sync + Clone + 'static,
>(
    fs: FS,
    archive_storage: A,
) -> AofResult<AofManager<FS, A>> {
    AofManagerBuilder::new()
        .with_filesystem(fs)
        .with_archive_storage(archive_storage)
        .build()
}

/// Convenience function to create a new AOF Manager with custom configuration
pub fn create_aof_manager_with_config<
    FS: FileSystem + Send + Sync + Clone + 'static,
    A: ArchiveStorage + Send + Sync + Clone + 'static,
>(
    fs: FS,
    archive_storage: A,
    instance_config: AofInstanceConfig,
) -> AofResult<AofManager<FS, A>> {
    AofManagerBuilder::new()
        .with_filesystem(fs)
        .with_archive_storage(archive_storage)
        .with_default_config(instance_config)
        .build()
}

/// Builder pattern for AOF configuration
pub fn aof_config() -> AofConfigBuilder {
    AofConfigBuilder::new()
}

/// Builder pattern for AOF Manager
pub fn aof_manager<
    FS: FileSystem + Send + Sync + Clone + 'static,
    A: ArchiveStorage + Send + Sync + Clone + 'static,
>() -> AofManagerBuilder<FS, A> {
    AofManagerBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_aof_creation() {
        let temp_dir = tempdir().unwrap();
        let fs = LocalFileSystem::new(temp_dir.path()).unwrap();

        // Create archive storage
        let archive_fs = Arc::new(AsyncFileSystem::new(temp_dir.path().join("archive")).unwrap());
        let archive_storage = Arc::new(
            FilesystemArchiveStorage::new(archive_fs, "archive")
                .await
                .unwrap(),
        );

        // Use a unique index path to avoid test conflicts
        let config = AofConfig {
            index_path: Some(
                temp_dir
                    .path()
                    .join("test_creation_index.mdbx")
                    .to_string_lossy()
                    .to_string(),
            ),
            ..AofConfig::default()
        };

        let aof = create_aof_with_config(fs, archive_storage, config).await;
        assert!(aof.is_ok());
    }

    #[tokio::test]
    async fn test_aof_with_config() {
        let temp_dir = tempdir().unwrap();
        let fs = LocalFileSystem::new(temp_dir.path()).unwrap();

        // Create archive storage
        let archive_fs = Arc::new(AsyncFileSystem::new(temp_dir.path().join("archive")).unwrap());
        let archive_storage = Arc::new(
            FilesystemArchiveStorage::new(archive_fs, "archive")
                .await
                .unwrap(),
        );

        let config = aof_config()
            .segment_size(1024 * 1024)
            .flush_strategy(FlushStrategy::Batched(10))
            .index_path(
                temp_dir
                    .path()
                    .join("test_config_index.mdbx")
                    .to_string_lossy()
                    .to_string(),
            )
            .build();

        let aof = create_aof_with_config(fs, archive_storage, config).await;
        assert!(aof.is_ok());
    }
}
