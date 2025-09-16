//! Basic tests for AOF Record format functionality
//! Full test coverage requires API alignment with current implementation

use tempfile::tempdir;

use bop_rs::aof::*;

/// Helper function to create test record header
fn create_test_record_header(record_id: u64, data_size: u32) -> RecordHeader {
    RecordHeader {
        checksum: 0,
        size: data_size,
        id: record_id,
        timestamp: 0, // Use 0 for test
        version: 1,
        flags: 0,
        reserved: [0; 6],
    }
}

#[test]
fn test_record_header_creation_and_properties() {
    let record_id = 42;
    let data_size = 256;
    let header = create_test_record_header(record_id, data_size);

    assert_eq!(header.id, record_id);
    assert_eq!(header.size, data_size);
    assert_eq!(header.version, 1);
    println!("Record header creation test completed");
}

#[tokio::test]
async fn test_record_operations_with_aof() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Test basic record operations
    let test_data = b"Hello, AOF Record!";
    let record_id = aof.append(test_data).await.unwrap();
    assert_eq!(record_id, 1);

    aof.flush().await.unwrap();
    assert_eq!(aof.get_total_records(), 1);

    println!("Record operations with AOF test completed");
}

#[tokio::test]
async fn test_multiple_record_operations() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Test multiple records
    let test_records = vec![
        b"First record".to_vec(),
        b"Second record with more data".to_vec(),
        b"Third record with even more content".to_vec(),
    ];

    for (i, data) in test_records.iter().enumerate() {
        let record_id = aof.append(data).await.unwrap();
        assert_eq!(record_id, (i + 1) as u64);
    }

    aof.flush().await.unwrap();
    assert_eq!(aof.get_total_records(), 3);

    println!("Multiple record operations test completed");
}

#[tokio::test]
async fn test_large_record_handling() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Test large record
    let large_data = vec![b'X'; 1024 * 1024]; // 1MB
    let record_id = aof.append(&large_data).await.unwrap();
    assert_eq!(record_id, 1);

    aof.flush().await.unwrap();
    assert_eq!(aof.get_total_records(), 1);

    println!("Large record handling test completed");
}

#[tokio::test]
async fn test_empty_record_handling() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Test empty record
    let empty_data = b"";
    let record_id = aof.append(empty_data).await.unwrap();
    assert_eq!(record_id, 1);

    aof.flush().await.unwrap();
    assert_eq!(aof.get_total_records(), 1);

    println!("Empty record handling test completed");
}

#[tokio::test]
async fn test_record_configuration_scenarios() {
    // Test with different AOF configurations
    let config = AofConfigBuilder::new()
        .segment_size(2 * 1024 * 1024) // 2MB segments
        .flush_strategy(FlushStrategy::Batched(10))
        .index_storage(IndexStorage::SeparateFile)
        .build();

    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs_and_config(fs, config).await.unwrap();

    // Test record operations with custom config
    for i in 1..=20 {
        let data = format!("Config test record {}", i);
        aof.append(data.as_bytes()).await.unwrap();
    }

    aof.flush().await.unwrap();
    assert_eq!(aof.get_total_records(), 20);

    println!("Record configuration scenarios test completed");
}

#[tokio::test]
async fn test_record_persistence() {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_path_buf();

    let test_records = vec![
        "Persistent record 1",
        "Persistent record 2",
        "Persistent record 3",
    ];

    // Write records
    {
        let fs = LocalFileSystem::new(&temp_path).unwrap();
        let mut aof = Aof::open_with_fs(fs).await.unwrap();

        for record in &test_records {
            aof.append(record.as_bytes()).await.unwrap();
        }
        aof.flush().await.unwrap();
        assert_eq!(aof.get_total_records(), test_records.len() as u64);
    }

    // Verify persistence
    {
        let fs = LocalFileSystem::new(&temp_path).unwrap();
        let aof = Aof::open_with_fs(fs).await.unwrap();
        assert_eq!(aof.get_total_records(), test_records.len() as u64);
        println!("Record persistence test completed");
    }
}

#[tokio::test]
async fn test_concurrent_record_operations() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let aof = std::sync::Arc::new(tokio::sync::Mutex::new(
        Aof::open_with_fs(fs).await.unwrap(),
    ));

    let mut handles = Vec::new();

    // Spawn concurrent record operations
    for i in 0..5 {
        let aof_clone = std::sync::Arc::clone(&aof);
        let handle = tokio::spawn(async move {
            let data = format!("Concurrent record {}", i);
            let mut aof_guard = aof_clone.lock().await;
            aof_guard.append(data.as_bytes()).await.unwrap()
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    let mut record_ids = Vec::new();
    for handle in handles {
        let record_id = handle.await.unwrap();
        record_ids.push(record_id);
    }

    // Verify all records were written
    record_ids.sort();
    for (i, &record_id) in record_ids.iter().enumerate() {
        assert_eq!(record_id, (i + 1) as u64);
    }

    let mut aof_guard = aof.lock().await;
    aof_guard.flush().await.unwrap();
    assert_eq!(aof_guard.get_total_records(), 5);

    println!("Concurrent record operations test completed");
}

#[tokio::test]
async fn test_record_performance() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    let start_time = std::time::Instant::now();

    // Test record throughput
    for i in 1..=1000 {
        let data = format!("Performance test record {}", i);
        aof.append(data.as_bytes()).await.unwrap();
    }

    aof.flush().await.unwrap();
    let elapsed = start_time.elapsed();

    assert_eq!(aof.get_total_records(), 1000);
    println!("Record performance: {} records in {:?}", 1000, elapsed);
}

#[tokio::test]
async fn test_record_boundary_conditions() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Test minimum size record
    aof.append(&[]).await.unwrap();

    // Test single byte record
    aof.append(&[0x42]).await.unwrap();

    // Test medium record
    let medium_data = vec![b'M'; 1024]; // 1KB
    aof.append(&medium_data).await.unwrap();

    aof.flush().await.unwrap();
    assert_eq!(aof.get_total_records(), 3);

    println!("Record boundary conditions test completed");
}

// TODO: Add comprehensive record tests once the following are implemented:
// - Record header serialization/deserialization
// - Checksum validation
// - Version compatibility testing
// - Compression and encryption scenarios
// - Error recovery and corruption detection
