//! Integration tests demonstrating SqliteSegmentIndex backup, recovery, and operational benefits

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::tempdir;

use bop_rs::aof::{
    SegmentEntry, SegmentMetadata, SegmentStatus, SqliteSegmentIndex, current_timestamp,
};
use rand;

fn create_test_entry(base_id: u64, last_id: u64, created_at: u64) -> SegmentEntry {
    let metadata = SegmentMetadata {
        base_id,
        last_id,
        record_count: last_id - base_id + 1,
        created_at,
        size: 1024,
        checksum: 0x12345678,
        compressed: false,
        encrypted: false,
    };

    SegmentEntry::new_active(&metadata, format!("segment_{}.log", base_id), 0x12345678)
}

#[test]
fn test_live_backup_while_writing() {
    let temp_dir = tempdir().unwrap();
    let primary_db = temp_dir.path().join("primary.db");
    let backup_db = temp_dir.path().join("backup.db");

    // Create primary database with initial data
    {
        let mut index = SqliteSegmentIndex::open(&primary_db).unwrap();

        for i in 0..100 {
            let base_id = i * 100;
            let last_id = base_id + 99;
            let entry = create_test_entry(base_id, last_id, current_timestamp());
            index.add_entry(entry).unwrap();
        }
        index.sync().unwrap();
    }

    // Perform live backup using SQLite's backup API
    {
        let source = rusqlite::Connection::open(&primary_db).unwrap();
        let mut dest = rusqlite::Connection::open(&backup_db).unwrap();

        // This backup can happen while the database is being used
        let backup = rusqlite::backup::Backup::new(&source, &mut dest).unwrap();
        backup
            .run_to_completion(5, Duration::from_millis(250), None)
            .unwrap();
    }

    // Add more data to primary after backup
    {
        let mut index = SqliteSegmentIndex::open(&primary_db).unwrap();

        for i in 100..200 {
            let base_id = i * 100;
            let last_id = base_id + 99;
            let entry = create_test_entry(base_id, last_id, current_timestamp());
            index.add_entry(entry).unwrap();
        }
        index.sync().unwrap();
    }

    // Verify backup has point-in-time data
    {
        let backup_index = SqliteSegmentIndex::open(&backup_db).unwrap();
        let primary_index = SqliteSegmentIndex::open(&primary_db).unwrap();

        assert_eq!(backup_index.len().unwrap(), 100); // Only initial data
        assert_eq!(primary_index.len().unwrap(), 200); // All data

        // Backup should have segments 0-99
        let found = backup_index.find_segment_for_id(5000).unwrap().unwrap();
        assert_eq!(found.base_id, 5000);

        // Backup should not have segments 100+
        assert!(backup_index.find_segment_for_id(15000).unwrap().is_none());

        // Primary should have all segments
        let found = primary_index.find_segment_for_id(15000).unwrap().unwrap();
        assert_eq!(found.base_id, 15000);
    }
}

#[test]
fn test_incremental_backup_simulation() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("incremental.db");
    let backup_dir = temp_dir.path().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    // Day 1: Initial data
    {
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        for i in 0..50 {
            let base_id = i * 100;
            let last_id = base_id + 99;
            let entry = create_test_entry(base_id, last_id, 1000000 + i * 1000); // Day 1 timestamps
            index.add_entry(entry).unwrap();
        }
        index.sync().unwrap();
    }

    // Create Day 1 backup
    let day1_backup = backup_dir.join("day1.db");
    fs::copy(&db_path, &day1_backup).unwrap();

    // Day 2: Add more data
    {
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        for i in 50..100 {
            let base_id = i * 100;
            let last_id = base_id + 99;
            let entry = create_test_entry(base_id, last_id, 2000000 + i * 1000); // Day 2 timestamps
            index.add_entry(entry).unwrap();
        }
        index.sync().unwrap();
    }

    // Create Day 2 backup
    let day2_backup = backup_dir.join("day2.db");
    fs::copy(&db_path, &day2_backup).unwrap();

    // Day 3: Update some existing entries (archive them)
    {
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        // Archive first 10 segments
        for i in 0..10 {
            let base_id = i * 100;
            if let Ok(Some(mut entry)) = index.get_segment(base_id) {
                entry.finalize(current_timestamp());
                entry.archive(
                    format!("archived_segment_{}.zst", base_id),
                    current_timestamp(),
                    Some(512),
                    Some(0x87654321),
                );
                index.update_entry(entry).unwrap();
            }
        }
        index.sync().unwrap();
    }

    // Create Day 3 backup
    let day3_backup = backup_dir.join("day3.db");
    fs::copy(&db_path, &day3_backup).unwrap();

    // Test point-in-time recovery to each backup

    // Day 1 state
    {
        let day1_index = SqliteSegmentIndex::open(&day1_backup).unwrap();
        assert_eq!(day1_index.len().unwrap(), 50);

        let found = day1_index.find_segment_for_id(2500).unwrap().unwrap();
        assert_eq!(found.status, SegmentStatus::Active);
        assert!(found.archive_key.is_none());
    }

    // Day 2 state
    {
        let day2_index = SqliteSegmentIndex::open(&day2_backup).unwrap();
        assert_eq!(day2_index.len().unwrap(), 100);

        let found = day2_index.find_segment_for_id(7500).unwrap().unwrap();
        assert_eq!(found.status, SegmentStatus::Active);
    }

    // Day 3 state
    {
        let day3_index = SqliteSegmentIndex::open(&day3_backup).unwrap();
        assert_eq!(day3_index.len().unwrap(), 100);

        // First segment should be archived
        let found = day3_index.find_segment_for_id(50).unwrap().unwrap();
        assert_eq!(found.status, SegmentStatus::ArchivedRemote);
        assert!(found.archive_key.is_some());

        // Later segments should still be active
        let found = day3_index.find_segment_for_id(7500).unwrap().unwrap();
        assert_eq!(found.status, SegmentStatus::Active);
    }
}

#[test]
fn test_database_corruption_recovery() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("corruption_test.db");
    let backup_path = temp_dir.path().join("backup.db");

    // Create database with test data
    {
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        for i in 0..100 {
            let base_id = i * 100;
            let last_id = base_id + 99;
            let entry = create_test_entry(base_id, last_id, current_timestamp());
            index.add_entry(entry).unwrap();
        }
        index.sync().unwrap();
    }

    // Create clean backup
    fs::copy(&db_path, &backup_path).unwrap();

    // Simulate corruption by truncating the database file
    {
        let mut file_content = fs::read(&db_path).unwrap();
        file_content.truncate(file_content.len() / 2); // Corrupt second half
        fs::write(&db_path, file_content).unwrap();
    }

    // Verify corruption is detected
    {
        let result = SqliteSegmentIndex::open(&db_path);
        assert!(
            result.is_err() || {
                // If it opens, it should detect corruption during operations
                let index = result.unwrap();
                index.len().is_err()
            }
        );
    }

    // Restore from backup
    fs::copy(&backup_path, &db_path).unwrap();

    // Verify recovery
    {
        let recovered_index = SqliteSegmentIndex::open(&db_path).unwrap();
        assert_eq!(recovered_index.len().unwrap(), 100);

        // Verify data integrity
        let found = recovered_index.find_segment_for_id(5000).unwrap().unwrap();
        assert_eq!(found.base_id, 5000);
        assert_eq!(found.last_id, 5099);
    }
}

#[test]
fn test_cross_platform_database_portability() {
    let temp_dir = tempdir().unwrap();
    let source_db = temp_dir.path().join("source.db");
    let copied_db = temp_dir.path().join("copied.db");

    // Create database
    {
        let mut index = SqliteSegmentIndex::open(&source_db).unwrap();

        let entries = vec![
            create_test_entry(1, 100, 1000),
            create_test_entry(101, 200, 2000),
            create_test_entry(201, 300, 3000),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }
        index.sync().unwrap();
    }

    // Copy database file (simulates moving between systems)
    fs::copy(&source_db, &copied_db).unwrap();

    // Verify copied database works identically
    {
        let source_index = SqliteSegmentIndex::open(&source_db).unwrap();
        let copied_index = SqliteSegmentIndex::open(&copied_db).unwrap();

        assert_eq!(source_index.len().unwrap(), copied_index.len().unwrap());

        let source_segments = source_index.get_all_segments().unwrap();
        let copied_segments = copied_index.get_all_segments().unwrap();

        assert_eq!(source_segments.len(), copied_segments.len());

        for (source, copied) in source_segments.iter().zip(copied_segments.iter()) {
            assert_eq!(source.base_id, copied.base_id);
            assert_eq!(source.last_id, copied.last_id);
            assert_eq!(source.created_at, copied.created_at);
            assert_eq!(source.status, copied.status);
        }
    }
}

#[test]
fn test_database_vacuum_and_optimization() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("vacuum_test.db");

    // Create database with lots of data
    {
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        for i in 0..1000 {
            let base_id = i * 100;
            let last_id = base_id + 99;
            let entry = create_test_entry(base_id, last_id, current_timestamp());
            index.add_entry(entry).unwrap();
        }
        index.sync().unwrap();
    }

    let initial_size = fs::metadata(&db_path).unwrap().len();

    // Remove half the entries to create fragmentation
    {
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        for i in (0..1000).step_by(2) {
            let base_id = i * 100;
            index.remove_entry(base_id).unwrap();
        }
        index.sync().unwrap();
    }

    let fragmented_size = fs::metadata(&db_path).unwrap().len();

    // Perform VACUUM to reclaim space
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("VACUUM", []).unwrap();
    }

    let vacuumed_size = fs::metadata(&db_path).unwrap().len();

    // Verify database still works after vacuum
    {
        let index = SqliteSegmentIndex::open(&db_path).unwrap();
        assert_eq!(index.len().unwrap(), 500); // Half the original entries

        // Verify remaining entries are accessible
        let found = index.find_segment_for_id(10050).unwrap().unwrap(); // Odd segment should exist
        assert_eq!(found.base_id, 10000);
    }

    println!(
        "Database sizes - Initial: {}, Fragmented: {}, Vacuumed: {}",
        initial_size, fragmented_size, vacuumed_size
    );

    // Vacuumed size should be significantly smaller than fragmented size
    assert!(vacuumed_size < fragmented_size);
}

#[test]
fn test_schema_migration_simulation() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("migration_test.db");

    // Create "v1" database (current schema)
    {
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        let entries = vec![
            create_test_entry(1, 100, 1000),
            create_test_entry(101, 200, 2000),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }
        index.sync().unwrap();
    }

    // Simulate adding a new column (future schema)
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Add hypothetical new column
        conn.execute(
            "ALTER TABLE segments ADD COLUMN replica_count INTEGER DEFAULT 1",
            [],
        )
        .unwrap();

        // Update schema version
        conn.execute(
            "UPDATE metadata SET value = '2' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
    }

    // Verify database still works with extended schema
    {
        let index = SqliteSegmentIndex::open(&db_path).unwrap();
        assert_eq!(index.len().unwrap(), 2);

        let found = index.find_segment_for_id(150).unwrap().unwrap();
        assert_eq!(found.base_id, 101);

        // New entries should still work
        let new_entry = create_test_entry(201, 300, 3000);
        let mut mutable_index = SqliteSegmentIndex::open(&db_path).unwrap();
        mutable_index.add_entry(new_entry).unwrap();
        assert_eq!(mutable_index.len().unwrap(), 3);
    }
}

#[test]
fn test_external_tool_compatibility() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("external_test.db");

    // Create database
    {
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        let entries = vec![
            create_test_entry(1, 100, 1000),
            create_test_entry(101, 200, 2000),
            create_test_entry(201, 300, 3000),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }
        index.sync().unwrap();
    }

    // Test that we can query the database with raw SQL
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Get segment count
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM segments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);

        // Get segments in a specific range
        let mut stmt = conn
            .prepare("SELECT base_id, last_id FROM segments WHERE base_id >= ? AND base_id <= ?")
            .unwrap();
        let rows = stmt
            .query_map([100, 200], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })
            .unwrap();

        let results: Vec<_> = rows.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (101, 200));
        assert_eq!(results[1], (201, 300));

        // Test aggregate queries
        let max_last_id: i64 = conn
            .query_row("SELECT MAX(last_id) FROM segments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(max_last_id, 300);
    }
}

#[test]
fn test_performance_under_load() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("load_test.db");

    let start_time = Instant::now();
    let target_operations = 100_000;

    // High-load test
    {
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        for i in 0..target_operations {
            let base_id = i * 10;
            let last_id = base_id + 9;
            let entry = create_test_entry(base_id, last_id, current_timestamp());
            index.add_entry(entry).unwrap();

            // Periodic sync to test WAL checkpoint performance
            if i % 10000 == 0 && i > 0 {
                index.sync().unwrap();
                let elapsed = start_time.elapsed();
                println!(
                    "Processed {} operations in {:?} ({:.2} ops/sec)",
                    i,
                    elapsed,
                    i as f64 / elapsed.as_secs_f64()
                );
            }
        }

        index.sync().unwrap();
    }

    let total_time = start_time.elapsed();
    println!(
        "Completed {} operations in {:?} ({:.2} ops/sec)",
        target_operations,
        total_time,
        target_operations as f64 / total_time.as_secs_f64()
    );

    // Verify data integrity after load test
    {
        let index = SqliteSegmentIndex::open(&db_path).unwrap();
        assert_eq!(index.len().unwrap(), target_operations as usize);

        // Test random access performance
        let lookup_start = Instant::now();
        let lookup_count = 10000;

        for _ in 0..lookup_count {
            let segment_idx = rand::random::<u64>() % target_operations;
            let base_id = segment_idx * 10;
            let record_id = base_id + 5;

            let found = index.find_segment_for_id(record_id).unwrap().unwrap();
            assert_eq!(found.base_id, base_id);
        }

        let lookup_time = lookup_start.elapsed();
        println!(
            "Performed {} lookups in {:?} ({:.2} lookups/sec)",
            lookup_count,
            lookup_time,
            lookup_count as f64 / lookup_time.as_secs_f64()
        );
    }

    // Verify file size is reasonable
    let final_size = fs::metadata(&db_path).unwrap().len();
    let size_per_entry = final_size / target_operations;
    println!(
        "Database size: {} bytes ({} bytes per entry)",
        final_size, size_per_entry
    );

    // Should be under 1KB per entry for this simple test data
    assert!(size_per_entry < 1024);
}
