//! Basic AOF usage example
//!
//! This example demonstrates:
//! - Creating an AOF instance directly
//! - Writing entries and flushing
//! - Basic functionality without complex features

use std::sync::Arc;
use tempfile::tempdir;

use bop_rs::aof::{
    AofResult, AsyncFileSystem, FilesystemArchiveStorage, FlushStrategy, aof_config,
    create_aof_with_config,
};

#[tokio::main]
async fn main() -> AofResult<()> {
    println!("🚀 AOF Basic Example - Creating, Writing, and Flushing");

    // Create temporary directories for this example
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let archive_dir = temp_dir.path().join("archive");

    std::fs::create_dir_all(&archive_dir).expect("Failed to create archive dir");

    println!("📁 Created temporary directories:");
    println!("   Data: {}", temp_dir.path().display());
    println!("   Archive: {}", archive_dir.display());

    // Step 1: Create filesystem and archive storage
    let local_fs = AsyncFileSystem::new(temp_dir.path())?;
    let archive_fs = Arc::new(AsyncFileSystem::new(&archive_dir)?);
    let archive_storage = Arc::new(FilesystemArchiveStorage::new(archive_fs, "aof_archive").await?);

    println!("✅ Created filesystem and archive storage");

    // Step 2: Create AOF configuration
    let config = aof_config()
        .segment_size(1024 * 64) // 64KB segments for demo
        .flush_strategy(FlushStrategy::Batched(3)) // Flush every 3 records
        .segment_cache_size(10)
        .archive_enabled(true)
        .archive_threshold(5) // Archive when 5 segments exist
        .index_path(
            temp_dir
                .path()
                .join("basic_demo_index.mdbx")
                .to_string_lossy()
                .to_string(),
        )
        .build();

    println!("✅ Created AOF configuration");

    // Step 3: Create AOF instance directly
    let mut aof = create_aof_with_config(local_fs, archive_storage, config).await?;

    println!("✅ Created AOF instance");

    // Step 4: Write some entries
    println!("\n📝 Writing entries to AOF...");

    let entries = vec![
        b"user:1001 logged in".to_vec(),
        b"user:1001 viewed dashboard".to_vec(),
        b"user:1001 updated profile".to_vec(),
        b"user:1002 logged in".to_vec(),
        b"user:1001 logged out".to_vec(),
        b"user:1002 created post".to_vec(),
        b"user:1002 shared post".to_vec(),
        b"user:1003 logged in".to_vec(),
    ];

    let mut record_ids = Vec::new();

    // Ensure active segment is ready before writing
    aof.wait_for_active_segment().await?;

    for (i, entry) in entries.iter().enumerate() {
        // Use synchronous append now
        let record_id = loop {
            match aof.append(entry) {
                Ok(id) => break id,
                Err(bop_rs::aof::AofError::WouldBlock(_)) => {
                    // Wait for segment to be ready and try again
                    aof.wait_for_active_segment().await?;
                    continue;
                }
                Err(e) => return Err(e),
            }
        };

        record_ids.push(record_id);
        println!(
            "   Wrote record {}: '{}' (ID: {})",
            i + 1,
            String::from_utf8_lossy(entry),
            record_id
        );
    }

    // Step 5: Explicit flush to ensure all data is persisted
    println!("\n💾 Flushing AOF to ensure persistence...");
    aof.flush().await?;
    println!("✅ Flush completed");

    // Step 6: Show AOF status
    println!("\n📊 AOF Status:");
    println!("   Next Record ID: {}", aof.next_id());
    println!("   Configuration: {:?}", aof.config());

    // Step 7: Write more entries to demonstrate continued operation
    println!("\n📝 Writing additional entries:");

    let additional_entries = vec![
        b"user:1003 updated settings".to_vec(),
        b"user:1001 logged in again".to_vec(),
        b"user:1004 registered".to_vec(),
    ];

    for (i, entry) in additional_entries.iter().enumerate() {
        // Use synchronous append with WouldBlock handling
        let record_id = loop {
            match aof.append(entry) {
                Ok(id) => break id,
                Err(bop_rs::aof::AofError::WouldBlock(_)) => {
                    // Wait for segment to be ready and try again
                    aof.wait_for_active_segment().await?;
                    continue;
                }
                Err(e) => return Err(e),
            }
        };

        record_ids.push(record_id);
        println!(
            "   New record {}: '{}' (ID: {})",
            i + 1,
            String::from_utf8_lossy(entry),
            record_id
        );
    }

    // Final flush
    aof.flush().await?;

    // Step 8: Show final status
    println!("\n📊 Final AOF Status:");
    println!("   Next Record ID: {}", aof.next_id());
    println!("   Total records written: {}", record_ids.len());
    println!("   Record IDs: {:?}", record_ids);

    // Step 9: Close the first AOF to release the MDBX lock
    println!("\n🔄 Demonstrating O(1) recovery by closing and reopening AOF...");

    let original_next_id = aof.next_id();
    println!("   Original Next Record ID: {}", original_next_id);

    // Close the AOF properly to release resources (in Rust, just drop it)
    drop(aof);
    println!("   ✅ Closed original AOF instance");

    // Create a new AOF instance with the same configuration for recovery
    let archive_fs2 = Arc::new(AsyncFileSystem::new(&archive_dir)?);
    let archive_storage2 =
        Arc::new(FilesystemArchiveStorage::new(archive_fs2, "aof_archive").await?);
    let local_fs2 = AsyncFileSystem::new(temp_dir.path())?;

    let config2 = aof_config()
        .segment_size(1024 * 64)
        .flush_strategy(FlushStrategy::Batched(3))
        .segment_cache_size(10)
        .archive_enabled(true)
        .archive_threshold(5)
        .index_path(
            temp_dir
                .path()
                .join("basic_demo_index.mdbx")
                .to_string_lossy()
                .to_string(),
        )
        .build();

    let aof2 = create_aof_with_config(local_fs2, archive_storage2, config2).await?;

    println!("✅ Reopened AOF instance using O(1) recovery");
    println!("   Recovered Next Record ID: {}", aof2.next_id());

    if aof2.next_id() == original_next_id {
        println!("✅ Recovery successful! Record IDs match.");
        println!("   This demonstrates O(1) recovery - the AOF quickly found the");
        println!("   last committed record ID from the SegmentIndex without loading all segments.");
    } else {
        println!("❌ Recovery issue: Record IDs don't match.");
        println!("   Expected: {}, Got: {}", original_next_id, aof2.next_id());
    }

    println!("\n🎉 AOF Basic Example completed successfully!");
    println!("   Temporary files will be cleaned up automatically");

    Ok(())
}
