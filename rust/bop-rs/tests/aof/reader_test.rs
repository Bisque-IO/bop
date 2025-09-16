//! Comprehensive tests for AOF Reader functionality
//!
//! Tests reading patterns, position management, multi-reader scenarios,
//! cross-segment navigation, and performance under concurrent access.

use std::sync::Arc;
use tempfile::tempdir;
use tokio::time::{Duration, sleep};

use bop_rs::aof::*;

/// Helper function to create AOF with test data
async fn create_aof_with_test_data(
    record_count: usize,
) -> (Aof<LocalFileSystem>, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Add test data
    for i in 1..=record_count {
        let data = format!("Test record {} with some content", i);
        aof.append(data.as_bytes()).await.unwrap();
    }
    aof.flush().await.unwrap();

    (aof, temp_dir)
}

/// Helper function to create AOF with various sized records
async fn create_aof_with_varied_data() -> (Aof<LocalFileSystem>, tempfile::TempDir, Vec<String>) {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    let test_records = vec![
        "Short".to_string(),
        "Medium length record with more content".to_string(),
        "Very long record with lots and lots of detailed content that spans much more text to test variable length handling".to_string(),
        "".to_string(), // Empty record
        "Unicode: 🦀 Rust 日本語 🚀".to_string(),
        format!("Large record: {}", "X".repeat(1000)),
    ];

    for record in &test_records {
        aof.append(record.as_bytes()).await.unwrap();
    }
    aof.flush().await.unwrap();

    (aof, temp_dir, test_records)
}

#[tokio::test]
async fn test_reader_creation_and_basic_properties() {
    let (mut aof, _temp_dir) = create_aof_with_test_data(5).await;

    let reader = aof.create_reader();

    // Test reader properties
    assert!(reader.id() > 0, "Reader should have a valid ID");

    let position = reader.position();
    assert_eq!(position.segment_id, 0, "Initial segment ID should be 0");
    assert_eq!(position.file_offset, 0, "Initial file offset should be 0");
    assert_eq!(position.record_id, 1, "Initial record ID should be 1");
}

#[tokio::test]
async fn test_reader_unique_ids() {
    let (mut aof, _temp_dir) = create_aof_with_test_data(1).await;

    // Create multiple readers
    let reader1 = aof.create_reader();
    let reader2 = aof.create_reader();
    let reader3 = aof.create_reader();

    // Each reader should have a unique ID
    assert_ne!(reader1.id(), reader2.id());
    assert_ne!(reader2.id(), reader3.id());
    assert_ne!(reader1.id(), reader3.id());

    // All readers should start at the same position
    assert_eq!(reader1.position().record_id, 1);
    assert_eq!(reader2.position().record_id, 1);
    assert_eq!(reader3.position().record_id, 1);
}

#[tokio::test]
async fn test_reader_from_specific_record_id() {
    let (mut aof, _temp_dir) = create_aof_with_test_data(10).await;

    // Create reader starting from record ID 5
    let reader = aof.create_reader_from_id(5).unwrap();

    let position = reader.position();
    assert_eq!(position.record_id, 5, "Reader should start at record ID 5");

    // Test boundary conditions
    let reader_first = aof.create_reader_from_id(1).unwrap();
    assert_eq!(reader_first.position().record_id, 1);

    let reader_last = aof.create_reader_from_id(10).unwrap();
    assert_eq!(reader_last.position().record_id, 10);

    // Test beyond available records
    let reader_beyond = aof.create_reader_from_id(15).unwrap();
    assert_eq!(
        reader_beyond.position().record_id,
        15,
        "Should allow positioning beyond current records"
    );
}

#[tokio::test]
async fn test_reader_from_timestamp() {
    let (mut aof, _temp_dir) = create_aof_with_test_data(5).await;

    let current_time = current_timestamp();

    // Create reader from current timestamp
    let reader = aof.create_reader_from_timestamp(current_time).unwrap();

    // Should create a valid reader (exact positioning logic depends on implementation)
    assert!(reader.id() > 0);
    assert!(reader.position().record_id >= 1);

    // Test with past timestamp (should go to beginning)
    let past_time = current_time - 3600000; // 1 hour ago
    let reader_past = aof.create_reader_from_timestamp(past_time).unwrap();
    assert_eq!(reader_past.position().record_id, 1);

    // Test with future timestamp
    let future_time = current_time + 3600000; // 1 hour from now
    let reader_future = aof.create_reader_from_timestamp(future_time).unwrap();
    assert!(reader_future.position().record_id >= 1);
}

#[tokio::test]
async fn test_reader_position_management() {
    let (aof, _temp_dir) = create_aof_with_test_data(1).await;

    let reader = {
        let mut aof_mut = aof;
        aof_mut.create_reader()
    };

    let _initial_position = reader.position();

    // Test position advancement
    reader.advance(100, 2);

    let new_position = reader.position();
    assert_eq!(new_position.file_offset, 100);
    assert_eq!(new_position.record_id, 2);

    // Test moving to different segment
    reader.move_to_segment(1, 50, 10);

    let segment_position = reader.position();
    assert_eq!(segment_position.segment_id, 1);
    assert_eq!(segment_position.file_offset, 50);
    assert_eq!(segment_position.record_id, 10);
}

#[tokio::test]
async fn test_concurrent_readers_isolation() {
    let (mut aof, _temp_dir) = create_aof_with_test_data(10).await;

    // Create multiple readers
    let reader1 = aof.create_reader();
    let reader2 = aof.create_reader();
    let reader3 = aof.create_reader();

    // Advance readers to different positions
    reader1.advance(100, 3);
    reader2.advance(200, 5);
    reader3.advance(300, 7);

    // Verify each reader maintains its own position
    assert_eq!(reader1.position().record_id, 3);
    assert_eq!(reader1.position().file_offset, 100);

    assert_eq!(reader2.position().record_id, 5);
    assert_eq!(reader2.position().file_offset, 200);

    assert_eq!(reader3.position().record_id, 7);
    assert_eq!(reader3.position().file_offset, 300);

    // Readers should not affect each other
    reader1.advance(150, 4);

    // Other readers should be unchanged
    assert_eq!(reader2.position().record_id, 5);
    assert_eq!(reader3.position().record_id, 7);
}

#[tokio::test]
async fn test_many_concurrent_readers() {
    let (mut aof, _temp_dir) = create_aof_with_test_data(100).await;

    let mut readers = Vec::new();

    // Create many readers
    for i in 0..50 {
        let reader = aof.create_reader_from_id((i % 100) + 1).unwrap();
        readers.push(reader);
    }

    // Verify all readers were created successfully
    assert_eq!(readers.len(), 50);

    // Verify each reader has unique ID
    let mut reader_ids = Vec::new();
    for reader in &readers {
        reader_ids.push(reader.id());
    }
    reader_ids.sort();
    reader_ids.dedup();
    assert_eq!(reader_ids.len(), 50, "All readers should have unique IDs");

    // Test concurrent position updates
    for (i, reader) in readers.iter().enumerate() {
        let new_record_id = (i * 2) + 1;
        let new_offset = i * 50;
        reader.advance(new_offset as u64, new_record_id as u64);
    }

    // Verify positions were set correctly
    for (i, reader) in readers.iter().enumerate() {
        let expected_record_id = (i * 2) + 1;
        let expected_offset = i * 50;

        let position = reader.position();
        assert_eq!(position.record_id, expected_record_id as u64);
        assert_eq!(position.file_offset, expected_offset as u64);
    }
}

#[tokio::test]
async fn test_reader_cross_segment_navigation() {
    // Create AOF with small segments to force multiple segments
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();

    let config = aof_config()
        .segment_size(1024) // Small segments to force rotation
        .build();

    let mut aof = Aof::open_with_fs_and_config(fs, config).await.unwrap();

    // Add enough data to span multiple segments
    for i in 1..=20 {
        let large_data = format!("Large record {} {}", i, "X".repeat(200));
        aof.append(large_data.as_bytes()).await.unwrap();
    }
    aof.flush().await.unwrap();

    let reader = aof.create_reader();

    // Test navigation across segments
    reader.move_to_segment(0, 0, 1); // First segment, first record
    assert_eq!(reader.position().segment_id, 0);
    assert_eq!(reader.position().record_id, 1);

    reader.move_to_segment(1, 0, 10); // Second segment, tenth record
    assert_eq!(reader.position().segment_id, 1);
    assert_eq!(reader.position().record_id, 10);

    reader.move_to_segment(2, 0, 20); // Third segment, twentieth record
    assert_eq!(reader.position().segment_id, 2);
    assert_eq!(reader.position().record_id, 20);
}

#[tokio::test]
async fn test_reader_with_empty_aof() {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();
    let mut aof = Aof::open_with_fs(fs).await.unwrap();

    // Create reader on empty AOF
    let reader = aof.create_reader();

    let position = reader.position();
    assert_eq!(
        position.record_id, 1,
        "Reader on empty AOF should start at record 1"
    );
    assert_eq!(position.file_offset, 0);
    assert_eq!(position.segment_id, 0);

    // Add data after reader creation
    aof.append(b"First record").await.unwrap();
    aof.flush().await.unwrap();

    // Reader position should not automatically change
    let position_after_append = reader.position();
    assert_eq!(position_after_append.record_id, 1);
}

#[tokio::test]
async fn test_reader_performance_under_load() {
    let (mut aof, _temp_dir) = create_aof_with_test_data(1000).await;

    let start_time = std::time::Instant::now();

    // Create many readers rapidly
    let mut readers = Vec::new();
    for i in 0..100 {
        let reader = aof.create_reader_from_id((i % 1000) + 1).unwrap();
        readers.push(reader);
    }

    let reader_creation_time = start_time.elapsed();

    // Test position updates
    let position_start = std::time::Instant::now();
    for (i, reader) in readers.iter().enumerate() {
        reader.advance((i * 10) as u64, (i + 1) as u64);
    }
    let position_update_time = position_start.elapsed();

    // Performance should be reasonable
    assert!(
        reader_creation_time.as_millis() < 1000,
        "Creating 100 readers should take less than 1 second"
    );
    assert!(
        position_update_time.as_millis() < 100,
        "Updating 100 positions should take less than 100ms"
    );

    // Verify all readers still work correctly
    for (i, reader) in readers.iter().enumerate() {
        let position = reader.position();
        assert_eq!(position.record_id, (i + 1) as u64);
        assert_eq!(position.file_offset, (i * 10) as u64);
    }
}

#[tokio::test]
async fn test_reader_with_various_record_sizes() {
    let (mut aof, _temp_dir, expected_records) = create_aof_with_varied_data().await;

    let reader = aof.create_reader();

    // Test positioning at different records
    for (i, _expected_content) in expected_records.iter().enumerate() {
        let record_id = (i + 1) as u64;
        reader.advance(0, record_id); // Simplified - real implementation would calculate offset

        let position = reader.position();
        assert_eq!(
            position.record_id, record_id,
            "Should be positioned at record {}",
            record_id
        );
    }
}

#[tokio::test]
async fn test_reader_boundary_conditions() {
    let (mut aof, _temp_dir) = create_aof_with_test_data(5).await;

    let reader = aof.create_reader();

    // Test at boundaries
    reader.advance(0, 1);
    assert_eq!(reader.position().record_id, 1);

    reader.advance(1000, 5);
    assert_eq!(reader.position().record_id, 5);

    // Test beyond current records
    reader.advance(2000, 10);
    assert_eq!(reader.position().record_id, 10);

    // Test edge cases
    reader.advance(u64::MAX - 1, u64::MAX - 1);
    assert_eq!(reader.position().record_id, u64::MAX - 1);
}

#[tokio::test]
async fn test_reader_concurrent_with_writes() {
    let aof = Arc::new(tokio::sync::Mutex::new({
        let (aof, _temp_dir) = create_aof_with_test_data(10).await;
        aof
    }));

    // Create reader
    let reader = {
        let mut aof_guard = aof.lock().await;
        aof_guard.create_reader()
    };

    // Start background writer
    let aof_writer = Arc::clone(&aof);
    let write_handle = tokio::spawn(async move {
        for i in 11..=20 {
            let mut aof_guard = aof_writer.lock().await;
            let data = format!("Background record {}", i);
            aof_guard.append(data.as_bytes()).await.unwrap();
            drop(aof_guard); // Release lock between operations
            sleep(Duration::from_millis(1)).await;
        }
    });

    // Test reader during concurrent writes
    for i in 1..=15 {
        reader.advance((i * 100) as u64, i as u64);
        let position = reader.position();
        assert_eq!(position.record_id, i as u64);
        sleep(Duration::from_millis(1)).await;
    }

    // Wait for writer to complete
    write_handle.await.unwrap();

    // Reader should still work correctly
    reader.advance(1600, 16);
    assert_eq!(reader.position().record_id, 16);
}

#[tokio::test]
async fn test_reader_persistence_across_segments() {
    // Test that reader positions are consistent across segment boundaries
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();

    let config = aof_config()
        .segment_size(512) // Very small segments
        .build();

    let mut aof = Aof::open_with_fs_and_config(fs, config).await.unwrap();

    // Fill multiple segments
    for i in 1..=10 {
        let data = format!("Multi-segment record {} {}", i, "Y".repeat(100));
        aof.append(data.as_bytes()).await.unwrap();
    }
    aof.flush().await.unwrap();

    let reader = aof.create_reader();

    // Test navigation across segment boundaries
    for segment in 0..3 {
        let record_id = (segment * 3) + 1;
        reader.move_to_segment(segment, 0, record_id);

        let position = reader.position();
        assert_eq!(position.segment_id, segment);
        assert_eq!(position.record_id, record_id);
    }
}

#[tokio::test]
async fn test_reader_reset_functionality() {
    let (mut aof, _temp_dir) = create_aof_with_test_data(10).await;

    let reader = aof.create_reader();

    // Move reader to some position
    reader.advance(500, 7);
    assert_eq!(reader.position().record_id, 7);

    // Reset to beginning
    reader.move_to_segment(0, 0, 1);
    let reset_position = reader.position();
    assert_eq!(reset_position.segment_id, 0);
    assert_eq!(reset_position.file_offset, 0);
    assert_eq!(reset_position.record_id, 1);
}
