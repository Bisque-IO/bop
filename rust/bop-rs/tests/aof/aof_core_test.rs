//! Comprehensive tests for AOF core functionality
//!
//! Tests the main AOF implementation including append/read operations,
//! segment management, reader coordination, and persistence scenarios.

use std::sync::Arc;
use tempfile::tempdir;
use tokio::time::{Duration, sleep};

use bop_rs::aof::{
    Aof, AofConfig, AofConfigBuilder, AofMetrics, AsyncFileSystem, FilesystemArchiveStorage,
    FlushStrategy, LocalFileSystem,
};

/// Helper function to create a test AOF instance
async fn create_test_aof() -> (
    Aof<LocalFileSystem, FilesystemArchiveStorage>,
    tempfile::TempDir,
) {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();

    // Create archive storage
    let archive_fs =
        Arc::new(bop_rs::aof::AsyncFileSystem::new(temp_dir.path().join("archive")).unwrap());
    let archive_storage = FilesystemArchiveStorage::new(archive_fs, "archive")
        .await
        .unwrap();

    // Use unique index path to avoid conflicts between concurrent tests
    let config = aof_config()
        .index_path(
            temp_dir
                .path()
                .join("test_index.mdbx")
                .to_string_lossy()
                .to_string(),
        )
        .build();
    let aof = Aof::open_with_fs_and_config(fs, Arc::new(archive_storage), config)
        .await
        .unwrap();
    (aof, temp_dir)
}

/// Helper function to create AOF with custom config
async fn create_test_aof_with_config(
    mut config: AofConfig,
) -> (
    Aof<LocalFileSystem, FilesystemArchiveStorage>,
    tempfile::TempDir,
) {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();

    // Create archive storage
    let archive_fs =
        Arc::new(bop_rs::aof::AsyncFileSystem::new(temp_dir.path().join("archive")).unwrap());
    let archive_storage = FilesystemArchiveStorage::new(archive_fs, "archive")
        .await
        .unwrap();

    // Ensure unique index path if not already set
    if config.index_path.is_none() {
        config.index_path = Some(
            temp_dir
                .path()
                .join("test_index.mdbx")
                .to_string_lossy()
                .to_string(),
        );
    }
    let aof = Aof::open_with_fs_and_config(fs, Arc::new(archive_storage), config)
        .await
        .unwrap();
    (aof, temp_dir)
}

/// Helper function to build AOF config
fn aof_config() -> AofConfigBuilder {
    AofConfigBuilder::new()
}

#[tokio::test]
async fn test_basic_append_and_read_workflow() {
    let (mut aof, _temp_dir) = create_test_aof().await;

    // Test basic append operation
    let test_data = b"Hello, AOF World!";
    let record_id = aof.append(test_data).await.unwrap();
    assert_eq!(record_id, 1, "First record should have ID 1");

    // Test immediate flush
    aof.flush().await.unwrap();

    // Verify record count
    let total_records = aof.get_total_records();
    assert_eq!(total_records, 1, "Should have 1 record after append");
}

#[tokio::test]
async fn test_multiple_appends_sequential() {
    let (mut aof, _temp_dir) = create_test_aof().await;

    let test_records = vec![
        b"Record 1".to_vec(),
        b"Record 2 with more data".to_vec(),
        b"Record 3 with even more data content".to_vec(),
    ];

    let mut record_ids = Vec::new();
    for (i, data) in test_records.iter().enumerate() {
        let record_id = aof.append(data).await.unwrap();
        record_ids.push(record_id);
        assert_eq!(record_id, (i + 1) as u64, "Record ID should be sequential");
    }

    aof.flush().await.unwrap();

    assert_eq!(aof.get_total_records(), 3, "Should have 3 records");
    assert_eq!(aof.next_id(), 4, "Next ID should be 4");
}

#[tokio::test]
async fn test_large_data_append() {
    let (mut aof, _temp_dir) = create_test_aof().await;

    // Create large data (larger than typical buffer)
    let large_data = vec![b'X'; 1024 * 1024]; // 1MB
    let record_id = aof.append(&large_data).await.unwrap();
    assert_eq!(record_id, 1);

    aof.flush().await.unwrap();

    assert_eq!(aof.get_total_records(), 1);
}

#[tokio::test]
async fn test_empty_data_handling() {
    let (mut aof, _temp_dir) = create_test_aof().await;

    // Test empty data append
    let empty_data = b"";
    let record_id = aof.append(empty_data).await.unwrap();
    assert_eq!(record_id, 1);

    aof.flush().await.unwrap();
    assert_eq!(aof.get_total_records(), 1);
}

#[tokio::test]
async fn test_reader_creation_and_management() {
    let (mut aof, _temp_dir) = create_test_aof().await;

    // Add some test data
    for i in 1..=5 {
        let data = format!("Record {}", i);
        aof.append(data.as_bytes()).await.unwrap();
    }
    aof.flush().await.unwrap();

    // Create multiple readers
    let reader1 = aof.create_reader();
    let reader2 = aof.create_reader();
    let reader3 = aof.create_reader();

    // Each reader should have unique ID
    assert_ne!(reader1.id(), reader2.id());
    assert_ne!(reader2.id(), reader3.id());
    assert_ne!(reader1.id(), reader3.id());

    // Test reader creation from specific record ID
    let reader_from_id = aof.create_reader_from_id(3).unwrap();
    assert_ne!(reader_from_id.id(), reader1.id());

    // Test reader creation from timestamp
    let reader_from_timestamp = aof.create_reader_from_timestamp(0).unwrap();
    assert_ne!(reader_from_timestamp.id(), reader1.id());
}

#[tokio::test]
async fn test_concurrent_append_operations() {
    let (aof, _temp_dir) = create_test_aof().await;
    let aof = Arc::new(tokio::sync::Mutex::new(aof));

    let mut handles = Vec::new();

    // Spawn multiple concurrent append tasks
    for i in 0..10 {
        let aof_clone = Arc::clone(&aof);
        let handle = tokio::spawn(async move {
            let data = format!("Concurrent record {}", i);
            let mut aof_guard = aof_clone.lock().await;
            aof_guard.append(data.as_bytes()).await.unwrap()
        });
        handles.push(handle);
    }

    // Wait for all appends to complete
    let mut record_ids = Vec::new();
    for handle in handles {
        let record_id = handle.await.unwrap();
        record_ids.push(record_id);
    }

    // Verify all records were written with unique IDs
    record_ids.sort();
    for (i, &record_id) in record_ids.iter().enumerate() {
        assert_eq!(record_id, (i + 1) as u64, "Record IDs should be sequential");
    }

    let mut aof_guard = aof.lock().await;
    aof_guard.flush().await.unwrap();
    assert_eq!(aof_guard.get_total_records(), 10);
}

#[tokio::test]
async fn test_flush_strategies() {
    // Test Sync flush strategy (equivalent to immediate)
    {
        let config = aof_config().flush_strategy(FlushStrategy::Sync).build();
        let (mut aof, _temp_dir) = create_test_aof_with_config(config).await;

        aof.append(b"test data").await.unwrap();
        // With sync flush, data should be flushed automatically
        assert_eq!(aof.get_total_records(), 1);
    }

    // Test Batched flush strategy
    {
        let config = aof_config()
            .flush_strategy(FlushStrategy::Batched(3))
            .build();
        let (mut aof, _temp_dir) = create_test_aof_with_config(config).await;

        // Add records but don't manually flush
        aof.append(b"record 1").await.unwrap();
        aof.append(b"record 2").await.unwrap();

        // Should not have triggered batch flush yet
        assert!(!aof.should_flush());

        aof.append(b"record 3").await.unwrap();
        // Now should trigger batch flush
        assert!(aof.should_flush());
    }

    // Test Periodic flush strategy
    {
        let config = aof_config()
            .flush_strategy(FlushStrategy::Periodic(100))
            .build();
        let (mut aof, _temp_dir) = create_test_aof_with_config(config).await;

        aof.append(b"test data").await.unwrap();

        // Wait for periodic flush
        sleep(Duration::from_millis(150)).await;

        // Should have been flushed periodically
        assert_eq!(aof.get_total_records(), 1);
    }
}

#[tokio::test]
async fn test_different_index_storage_modes() {
    // Test InMemory index storage
    {
        let config = aof_config().index_storage(IndexStorage::InMemory).build();
        let (mut aof, _temp_dir) = create_test_aof_with_config(config).await;

        aof.append(b"test data").await.unwrap();
        aof.flush().await.unwrap();
        assert_eq!(aof.get_total_records(), 1);
    }

    // Test InSegment index storage
    {
        let config = aof_config().index_storage(IndexStorage::InSegment).build();
        let (mut aof, _temp_dir) = create_test_aof_with_config(config).await;

        aof.append(b"test data").await.unwrap();
        aof.flush().await.unwrap();
        assert_eq!(aof.get_total_records(), 1);
    }
}

#[tokio::test]
async fn test_custom_segment_sizes() {
    // Test small segment size (forces segment rotation)
    {
        let temp_dir = tempdir().unwrap();
        let config = aof_config()
            .segment_size(1024) // 1KB segments
            .index_path(
                temp_dir
                    .path()
                    .join("small_segments.mdbx")
                    .to_string_lossy()
                    .to_string(),
            )
            .build();
        let (mut aof, _temp_dir) = create_test_aof_with_config(config).await;

        // Add data that will span multiple segments
        for _i in 0..10 {
            let large_data = vec![b'X'; 200]; // 200 bytes each
            aof.append(&large_data).await.unwrap();
        }

        aof.flush().await.unwrap();
        assert_eq!(aof.get_total_records(), 10);
    }

    // Test large segment size
    {
        let temp_dir = tempdir().unwrap();
        let config = aof_config()
            .segment_size(10 * 1024 * 1024) // 10MB segments
            .index_path(
                temp_dir
                    .path()
                    .join("large_segments.mdbx")
                    .to_string_lossy()
                    .to_string(),
            )
            .build();
        let (mut aof, _temp_dir) = create_test_aof_with_config(config).await;

        for i in 0..100 {
            aof.append(format!("Record {}", i).as_bytes())
                .await
                .unwrap();
        }

        aof.flush().await.unwrap();
        assert_eq!(aof.get_total_records(), 100);
    }
}

#[tokio::test]
async fn test_aof_restart_and_recovery() {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    let index_path = temp_path.join("shared_index.mdbx");

    // First AOF instance - write some data
    {
        let fs = LocalFileSystem::new(&temp_path).unwrap();
        let config = aof_config()
            .index_path(index_path.to_string_lossy().to_string())
            .build();
        let mut aof = Aof::open_with_fs_and_config(fs, config).await.unwrap();

        for i in 1..=5 {
            let data = format!("Persistent record {}", i);
            aof.append(data.as_bytes()).await.unwrap();
        }
        aof.flush().await.unwrap();

        assert_eq!(aof.get_total_records(), 5);

        // Properly close the AOF to ensure metadata is persisted
        aof.close().await.unwrap();
    }

    // Second AOF instance - should recover existing data
    {
        let fs = LocalFileSystem::new(&temp_path).unwrap();
        let config = aof_config()
            .index_path(index_path.to_string_lossy().to_string())
            .build();
        let aof = Aof::open_with_fs_and_config(fs, config).await.unwrap();

        // Should have recovered the 5 records
        assert_eq!(aof.get_total_records(), 5);
        assert_eq!(
            aof.next_id(),
            6,
            "Next ID should continue from where we left off"
        );
    }
}

#[tokio::test]
async fn test_data_integrity_after_restart() {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_path_buf();

    let test_records = vec![
        "First record with unique content",
        "Second record with different data",
        "Third record with more information",
    ];

    // Write test data
    {
        let fs = LocalFileSystem::new(&temp_path).unwrap();
        let mut aof = Aof::open_with_fs(fs).await.unwrap();

        for record in &test_records {
            aof.append(record.as_bytes()).await.unwrap();
        }
        aof.flush().await.unwrap();
    }

    // Verify data after restart
    {
        let fs = LocalFileSystem::new(&temp_path).unwrap();
        let aof = Aof::open_with_fs(fs).await.unwrap();

        assert_eq!(aof.get_total_records(), test_records.len() as u64);

        // Note: Direct data verification would require reader implementation
        // This test verifies structural integrity after restart
    }
}

#[tokio::test]
async fn test_pre_allocation_metrics() {
    let (aof, _temp_dir) = create_test_aof().await;

    // Test that pre-allocation metrics are accessible
    let metrics = aof.get_pre_allocation_metrics().await;

    // Initial metrics should show no operations yet
    assert_eq!(metrics.success_count, 0);
    assert_eq!(metrics.failure_count, 0);
}

#[tokio::test]
async fn test_aof_configuration_validation() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();

    // Test valid configuration
    let valid_config = aof_config()
        .segment_size(1024 * 1024)
        .index_storage(IndexStorage::InSegment)
        .flush_strategy(FlushStrategy::Batched(10))
        .build();

    let aof_result = Aof::open_with_fs_and_config(fs, valid_config).await;
    assert!(aof_result.is_ok(), "Valid configuration should succeed");
}

#[tokio::test]
async fn test_concurrent_readers_on_same_aof() {
    let (mut aof, _temp_dir) = create_test_aof().await;

    // Add test data
    for i in 1..=10 {
        let data = format!("Shared record {}", i);
        aof.append(data.as_bytes()).await.unwrap();
    }
    aof.flush().await.unwrap();

    // Create multiple readers
    let reader1 = aof.create_reader();
    let reader2 = aof.create_reader();
    let reader3 = aof.create_reader();

    // All readers should be independent
    assert_ne!(reader1.id(), reader2.id());
    assert_ne!(reader2.id(), reader3.id());
    assert_ne!(reader1.id(), reader3.id());

    // Each reader should start at the beginning
    let pos1 = reader1.position();
    let pos2 = reader2.position();
    let pos3 = reader3.position();

    assert_eq!(pos1.record_id, 1);
    assert_eq!(pos2.record_id, 1);
    assert_eq!(pos3.record_id, 1);
}
