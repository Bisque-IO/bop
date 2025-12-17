use maniac_libsql::Store;
use tempfile::tempdir;

#[test]
fn test_branching_and_mvcc() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let store = Store::open(dir.path())?;

    // 0. Create Database (4KB pages, 4MB chunks)
    // Returns u32 now
    let db_id = store.create_database("test_db".to_string(), 4096, 4 * 1024 * 1024)?;

    // 1. Create Main Branch
    // Returns u32
    let main_branch_id = store.create_branch(db_id, "main".to_string(), None)?;
    assert_eq!(main_branch_id, 1);

    // 2. Commit Base Chunk to Segment 0 on Main
    let chunk_data_1 = vec![0xAA; 4096]; // Example chunk data
    let version_1 = 1;
    store.commit_segment(db_id, main_branch_id, 0, chunk_data_1.clone(), version_1)?;

    // 3. Read Page 10 from Main (Should find Chunk1)
    let found = store
        .find_chunk_for_page(db_id, main_branch_id, 10)?
        .expect("Should find chunk");

    // 4. Create Dev Branch (Fork from Main)
    // Pass main_branch_id as parent
    let dev_branch_id = store.create_branch(db_id, "dev".to_string(), Some(main_branch_id))?;
    assert_eq!(dev_branch_id, 2);

    // 5. Read Page 10 from Dev (Should see same data via Overlay)
    let found_dev = store
        .find_chunk_for_page(db_id, dev_branch_id, 10)?
        .expect("Should find chunk in dev");
    assert_eq!(found.chunk_id, found_dev.chunk_id); // CAS Deduplication

    // 6. Commit NEW Chunk to Dev (Shadow)
    let chunk_data_2 = vec![0xBB; 4096];
    let version_2 = version_1 + 1; // 2
    store.commit_segment(db_id, dev_branch_id, 0, chunk_data_2, version_2)?;

    // 7. Verify Dev sees new data
    let found_dev_2 = store
        .find_chunk_for_page(db_id, dev_branch_id, 10)?
        .expect("Should find new chunk");
    assert_ne!(found_dev.chunk_id, found_dev_2.chunk_id);

    // 8. Verify Main sees old data (Isolation)
    let found_main = store
        .find_chunk_for_page(db_id, main_branch_id, 10)?
        .expect("Should find chunk");
    assert_eq!(found_main.chunk_id, found.chunk_id);

    Ok(())
}

#[test]
fn test_wal_and_checkpoint() -> anyhow::Result<()> {
    use maniac_libsql::schema::{CheckpointInfo, WalSegment};

    let dir = tempdir()?;
    let store = Store::open(dir.path())?;
    // Sizes don't matter for WAL test, but must be provided
    let db_id = store.create_database("wal_test_db".to_string(), 4096, 4096 * 1024)?;

    // Create a branch for the WAL operations
    let branch_id = store.create_branch(db_id, "main".to_string(), None)?;

    // 1. Append WAL Segments
    let seg1 = WalSegment {
        start_frame: 1,
        end_frame: 10,
        s3_key: "wal/1-10".to_string(),
        timestamp: 100,
    };
    store.append_wal_metadata(db_id, branch_id, seg1.clone())?;

    let seg2 = WalSegment {
        start_frame: 11,
        end_frame: 20,
        s3_key: "wal/11-20".to_string(),
        timestamp: 105,
    };
    store.append_wal_metadata(db_id, branch_id, seg2.clone())?;

    // 2. Read WAL Segments
    let segments = store.get_wal_segments(db_id, branch_id, 1)?;
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].start_frame, 1);
    assert_eq!(segments[1].start_frame, 11);

    // 3. Set Checkpoint
    let cp = CheckpointInfo {
        last_applied_frame: 20,
        base_layer_version: 1,
    };
    store.set_checkpoint(db_id, branch_id, cp.clone())?;

    // 4. Get Checkpoint
    let cp_read = store
        .get_checkpoint(db_id, branch_id)?
        .expect("Should have checkpoint");
    assert_eq!(cp_read.last_applied_frame, 20);

    Ok(())
}
