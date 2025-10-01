//! Full integration example demonstrating AOF creation, writing, checkpointing, and reading.
//!
//! This example shows:
//! - Manager initialization
//! - AOF creation with 128KB chunks
//! - Writing data to force chunk boundaries
//! - Checkpointing sealed chunks
//! - Reading data back with AofCursor

use std::sync::Arc;

use bop_storage::{
    AofConfig, Manager, Manifest, ManifestOptions,
    MIN_CHUNK_SIZE_BYTES, MAX_CHUNK_SIZE_BYTES, DEFAULT_CHUNK_SIZE_BYTES,
};
use tempfile::TempDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== AOF Integration Example ===\n");

    // Step 1: Set up manifest and manager
    println!("1. Creating Manager with Manifest...");
    // let temp_dir = TempDir::new()?;
    let temp_dir = std::path::PathBuf::from("./aof_example_dir");
    std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)?;
    let manifest_path = temp_dir.join("manifest");
    let options = ManifestOptions::default();
    println!("   → Initial map_size: {} MiB", options.initial_map_size / (1024 * 1024));
    let manifest = Arc::new(Manifest::open(&manifest_path, options)?);
    let manager = Manager::new(manifest.clone());
    println!("   ✓ Manager created\n");

    // Step 2: Create AOF with 128KB chunks
    println!("2. Creating AOF with 128KB chunks...");
    const CHUNK_SIZE: u64 = 128 * 1024; // 128 KB
    let aof_config = AofConfig::builder("example-aof")
        .chunk_size_bytes(CHUNK_SIZE)
        .data_dir(temp_dir.join("aof-data"))
        .wal_dir(temp_dir.join("aof-wal"))
        .build();

    let aof = manager.open_db(aof_config)?;
    println!("   ✓ AOF created with ID: {}", aof.id());
    println!("   ✓ Chunk size: {} bytes ({} KB)", aof.chunk_size_bytes(), aof.chunk_size_bytes() / 1024);
    println!();

    // Step 3: Write data across multiple chunks
    println!("3. Writing data to force multiple chunks...");
    let writes_per_chunk = 3;
    let num_chunks = 3;
    let write_size = (CHUNK_SIZE / writes_per_chunk as u64) as usize;
    let total_writes = writes_per_chunk * num_chunks;

    println!("   → Writing {} batches of {} bytes each", total_writes, write_size);
    println!("   → Expected to span {} chunks", num_chunks);
    println!();

    for i in 0..total_writes {
        // Create test data with a recognizable pattern
        let mut data = vec![0u8; write_size];
        for (idx, byte) in data.iter_mut().enumerate() {
            *byte = 'A' as u8;// ((i * 256 + idx % 256) & 0xFF) as u8;
        }

        // Append data - tail segment is managed automatically!
        let lsn = aof.append(bop_storage::WriteChunk::Owned(data.to_vec()))?;

        let chunk_id = aof.chunk_id_for_lsn(lsn);
        let chunk_offset = aof.chunk_offset(lsn);

        println!("   Write #{}: LSN={} → Chunk {} @ offset {}",
                 i + 1, lsn, chunk_id, chunk_offset);

        // Optionally sync to ensure durability
        if (i + 1) % writes_per_chunk == 0 {
            let durable = aof.sync()?;
            println!("   → Synced up to LSN {}", durable);
        }
    }

    let total_bytes = aof.tail_lsn();
    let durable_bytes = aof.durable_lsn();

    println!();
    println!("   ✓ Wrote {} bytes across {} chunks", total_bytes, num_chunks);
    println!("   ✓ Durable up to LSN {}", durable_bytes);
    println!();

    // Step 4: Demonstrate chunk addressing
    println!("4. Demonstrating chunk addressing...");
    let test_lsns = vec![0, 64 * 1024, 128 * 1024, 192 * 1024, 256 * 1024];
    for lsn in test_lsns {
        if lsn >= total_bytes {
            break;
        }
        let chunk_id = aof.chunk_id_for_lsn(lsn);
        let start_lsn = aof.chunk_start_lsn(chunk_id);
        let end_lsn = aof.chunk_end_lsn(chunk_id);
        let offset = aof.chunk_offset(lsn);

        println!("   LSN {} → Chunk {} [{}..{}), offset {}",
                 lsn, chunk_id, start_lsn, end_lsn, offset);
    }
    println!();

    // Step 5: Checkpoint (seal chunks)
    println!("5. Running checkpoint...");
    aof.checkpoint()?;
    println!("   ✓ Checkpoint completed");
    println!();

    // Step 6: Demonstrate batch append
    println!("6. Demonstrating batch append...");
    let batch_data = vec![
        bop_storage::WriteChunk::Owned((b"Hello" as &[u8]).to_vec()),
        bop_storage::WriteChunk::Owned((b"World" as &[u8]).to_vec()),
        bop_storage::WriteChunk::Owned((b"From" as &[u8]).to_vec()),
        bop_storage::WriteChunk::Owned((b"AOF" as &[u8]).to_vec()),
    ];
    let batch_lsn = aof.append_batch(&batch_data)?;
    println!("   ✓ Batch appended at LSN {}", batch_lsn);
    println!("   ✓ Total size: {} bytes", batch_data.iter().map(|d| d.len()).sum::<usize>());
    aof.sync()?;
    println!();

    // Step 7: Create a reader cursor
    println!("7. Reading data back with AofCursor...");
    let cursor = aof.open_cursor();
    println!("   ✓ Cursor opened at LSN {}", cursor.position());

    // Note: To actually read data, the tail segment needs to be sealed
    // and the chunks need to be persisted. This is handled by checkpoint.

    println!();
    println!("   ℹ️  Note: Full read demonstration requires:");
    println!("      - Checkpoint to seal tail segment");
    println!("      - Chunks persisted to disk");
    println!("      - AofCursor reading from sealed chunks");
    println!();

    // Step 8: Demonstrate chunk size validation
    println!("8. Chunk size configuration...");
    println!("   → Minimum: {} KB", MIN_CHUNK_SIZE_BYTES / 1024);
    println!("   → Maximum: {} GB", MAX_CHUNK_SIZE_BYTES / (1024 * 1024 * 1024));
    println!("   → Default: {} MB", DEFAULT_CHUNK_SIZE_BYTES / (1024 * 1024));
    println!("   → Current: {} KB", aof.chunk_size_bytes() / 1024);
    println!();

    // Step 9: Show diagnostics
    println!("9. AOF Diagnostics...");
    let diag = aof.diagnostics();
    println!("   → Name: {}", diag.name);
    println!("   → ID: {}", diag.id);
    println!("   → IO Backend: {:?}", diag.io_backend);
    println!("   → Tail LSN: {}", aof.tail_lsn());
    println!("   → Durable LSN: {}", aof.durable_lsn());
    println!("   → Closed: {}", diag.is_closed);
    println!();

    // Step 10: Cleanup
    println!("10. Shutting down...");
    aof.close();
    manager.shutdown();
    println!("   ✓ Shutdown complete");
    println!();

    println!("=== Example Complete ===");
    println!();
    println!("Key Takeaways:");
    println!("  • Simple API: aof.append(data) - tail segment managed automatically");
    println!("  • Fixed 128KB chunks enable O(1) chunk lookup");
    println!("  • chunk_id = lsn / chunk_size (simple arithmetic!)");
    println!("  • aof.sync() ensures durability");
    println!("  • Checkpoint seals tail into immutable chunks");
    println!("  • AofCursor provides sequential reads with auto-hydration");

    Ok(())
}
