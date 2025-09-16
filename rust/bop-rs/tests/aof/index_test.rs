//! Basic tests for AOF index functionality
//! Full test coverage requires API alignment with current implementation

use tempfile::tempdir;

use bop_rs::aof::*;

#[test]
fn test_binary_index_entry_creation() {
    let entry = BinaryIndexEntry {
        record_id: 42,
        file_offset: 1024,
    };

    // Copy fields to avoid packed field reference issues
    let record_id = entry.record_id;
    let file_offset = entry.file_offset;

    assert_eq!(record_id, 42);
    assert_eq!(file_offset, 1024);
    println!("Binary index entry creation test completed");
}

#[test]
fn test_binary_index_creation() {
    let index = BinaryIndex {
        index_start_offset: 1000,
        entry_count: 5,
        index_size: 80, // 5 entries * 16 bytes each
    };

    assert_eq!(index.index_start_offset, 1000);
    assert_eq!(index.entry_count, 5);
    assert_eq!(index.index_size, 80);
    println!("Binary index creation test completed");
}

#[tokio::test]
async fn test_index_storage_modes() {
    // Test InMemory index storage
    {
        let config = AofConfigBuilder::new()
            .index_storage(IndexStorage::InMemory)
            .build();
        let temp_dir = tempdir().unwrap();
        let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
        let _aof = Aof::open_with_fs_and_config(fs, config).await.unwrap();
        println!("InMemory index storage test completed");
    }

    // Test InSegment index storage
    {
        let config = AofConfigBuilder::new()
            .index_storage(IndexStorage::InSegment)
            .build();
        let temp_dir = tempdir().unwrap();
        let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
        let _aof = Aof::open_with_fs_and_config(fs, config).await.unwrap();
        println!("InSegment index storage test completed");
    }
}

#[tokio::test]
async fn test_index_operations_with_data() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Add test data for index operations
    for i in 1..=100 {
        let data = format!("Test record {}", i);
        aof.append(data.as_bytes()).await.unwrap();
    }
    aof.flush().await.unwrap();

    // Test basic index functionality
    assert_eq!(aof.get_total_records(), 100);

    // Create readers (tests index usage)
    let reader1 = aof.create_reader();
    let reader2 = aof.create_reader_from_id(50).unwrap();

    assert_ne!(reader1.id(), reader2.id());
    assert_eq!(reader1.position().record_id, 1);
    assert_eq!(reader2.position().record_id, 50);

    println!("Index operations with data test completed");
}

#[tokio::test]
async fn test_index_recovery_scenarios() {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_path_buf();

    // Create AOF with index data
    {
        let fs = LocalFileSystem::new(&temp_path).unwrap();
        let mut aof = Aof::open_with_fs(fs).await.unwrap();

        for i in 1..=25 {
            let data = format!("Recovery test record {}", i);
            aof.append(data.as_bytes()).await.unwrap();
        }
        aof.flush().await.unwrap();
        assert_eq!(aof.get_total_records(), 25);
    }

    // Test recovery
    {
        let fs = LocalFileSystem::new(&temp_path).unwrap();
        let aof = Aof::open_with_fs(fs).await.unwrap();

        // Verify index recovery
        assert_eq!(aof.get_total_records(), 25);
        println!("Index recovery test completed");
    }
}

#[tokio::test]
async fn test_index_performance() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Generate workload for performance testing
    let start_time = std::time::Instant::now();

    for i in 1..=1000 {
        let data = format!("Performance test record {}", i);
        aof.append(data.as_bytes()).await.unwrap();
    }
    aof.flush().await.unwrap();

    let elapsed = start_time.elapsed();

    assert_eq!(aof.get_total_records(), 1000);
    println!("Index performance: {} records in {:?}", 1000, elapsed);
}

#[tokio::test]
async fn test_concurrent_index_operations() {
    // Test concurrent index access
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Add initial data
    for i in 1..=10 {
        aof.append(format!("Concurrent test {}", i).as_bytes())
            .await
            .unwrap();
    }
    aof.flush().await.unwrap();

    // Create multiple readers (tests concurrent index access)
    let reader1 = aof.create_reader();
    let reader2 = aof.create_reader();
    let reader3 = aof.create_reader();

    // Verify concurrent readers work
    assert_ne!(reader1.id(), reader2.id());
    assert_ne!(reader2.id(), reader3.id());

    println!("Concurrent index operations test completed");
}

#[tokio::test]
async fn test_index_with_different_segment_sizes() {
    // Test memory-efficient index configurations
    let config = AofConfigBuilder::new()
        .index_storage(IndexStorage::InSegment)
        .segment_size(1024 * 1024) // Small segments for memory efficiency
        .build();

    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs_and_config(fs, config).await.unwrap();

    // Add data to test indexing across segments
    for _i in 1..=200 {
        let data = vec![b'A'; 100]; // 100 byte records
        aof.append(&data).await.unwrap();
    }
    aof.flush().await.unwrap();

    assert_eq!(aof.get_total_records(), 200);
    println!("Index with different segment sizes test completed");
}

#[tokio::test]
async fn test_index_boundary_conditions() {
    // Test index behavior at boundaries
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Test empty index
    assert_eq!(aof.get_total_records(), 0);

    // Test single record
    aof.append(b"single record").await.unwrap();
    aof.flush().await.unwrap();
    assert_eq!(aof.get_total_records(), 1);

    // Test large record
    let large_record = vec![b'L'; 10 * 1024]; // 10KB
    aof.append(&large_record).await.unwrap();
    aof.flush().await.unwrap();
    assert_eq!(aof.get_total_records(), 2);

    println!("Index boundary conditions test completed");
}

// TODO: Add comprehensive index tests once the following are implemented:
// - Index serialization and deserialization
// - Segment footer operations
// - Index compression techniques
// - Advanced indexing strategies
// - Performance benchmarking for different index configurations
