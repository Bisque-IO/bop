//! Basic tests for AOF Appender functionality
//! Full test coverage requires API alignment with current implementation

use std::sync::Arc;
use tempfile::tempdir;

use bop_rs::aof::*;

/// Helper function to create a test appender
async fn create_test_appender() -> (Appender<LocalFileSystem>, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let fs = Arc::new(LocalFileSystem::new(temp_dir.path()).unwrap());

    let config = AofConfig::default();
    let segment_path = "test_segment.log";

    let appender = Appender::create_new(
        segment_path,
        fs,
        4 * 1024 * 1024, // 4MB segment size
        1,               // base_id
        config.flush_strategy,
        config.index_storage,
    )
    .await
    .unwrap();

    (appender, temp_dir)
}

#[tokio::test]
async fn test_basic_appender_creation() {
    let (_appender, _temp_dir) = create_test_appender().await;
    // Basic verification that appender was created
    println!("Appender created successfully");
}

#[tokio::test]
async fn test_appender_with_sync_flush() {
    let temp_dir = tempdir().unwrap();
    let fs = Arc::new(LocalFileSystem::new(temp_dir.path()).unwrap());
    let segment_path = "sync_segment.log";

    let _appender = Appender::create_new(
        segment_path,
        fs,
        4 * 1024 * 1024,
        1,
        FlushStrategy::Sync, // Use Sync instead of Immediate
        IndexStorage::InSegment,
    )
    .await
    .unwrap();

    println!("Sync appender created successfully");
}

#[tokio::test]
async fn test_appender_with_batched_flush() {
    let temp_dir = tempdir().unwrap();
    let fs = Arc::new(LocalFileSystem::new(temp_dir.path()).unwrap());
    let segment_path = "batch_segment.log";

    let _appender = Appender::create_new(
        segment_path,
        fs,
        4 * 1024 * 1024,
        1,
        FlushStrategy::Batched(10),
        IndexStorage::InSegment,
    )
    .await
    .unwrap();

    println!("Batched appender created successfully");
}

#[tokio::test]
async fn test_appender_with_periodic_flush() {
    let temp_dir = tempdir().unwrap();
    let fs = Arc::new(LocalFileSystem::new(temp_dir.path()).unwrap());
    let segment_path = "periodic_segment.log";

    let _appender = Appender::create_new(
        segment_path,
        fs,
        4 * 1024 * 1024,
        1,
        FlushStrategy::Periodic(100), // 100ms
        IndexStorage::InSegment,
    )
    .await
    .unwrap();

    println!("Periodic appender created successfully");
}

// TODO: Add comprehensive appender tests once the following are implemented:
// - Proper append() method signature and behavior
// - FlushController access methods
// - Batch append operations
// - Error handling and recovery
// - Performance metrics collection
// - Concurrent append operations
// - Memory management tests
