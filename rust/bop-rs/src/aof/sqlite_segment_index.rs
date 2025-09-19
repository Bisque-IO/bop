//! SQLite-based persistent segment index for archive storage
//!
//! This module provides an alternative to MDBX-based indexing using SQLite with WAL mode
//! for better backup, snapshot, and archival workflow compatibility.

use std::path::Path;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult, Row};

use crate::aof::error::{AofError, AofResult};
use crate::aof::segment_store::SegmentEntry;

/// SQLite-backed persistent segment index with WAL mode for backup-friendly operations
pub struct SqliteSegmentIndex {
    conn: Connection,
    /// Index metadata version for future migrations
    version: u32,
}

impl SqliteSegmentIndex {
    /// Create or open a SQLite-based segment index
    pub fn open<P: AsRef<Path>>(path: P) -> AofResult<Self> {
        println!(
            "DEBUG: Opening SQLite segment index at path: {:?}",
            path.as_ref()
        );

        let conn = Connection::open(&path).map_err(|e| {
            AofError::IndexError(format!("Failed to open SQLite database: {}", e))
        })?;

        // Configure SQLite for optimal performance and backup compatibility
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA cache_size=10000;
             PRAGMA temp_store=memory;
             PRAGMA mmap_size=268435456;"  // 256MB mmap
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to configure SQLite: {}", e))
        })?;

        let mut index = Self {
            conn,
            version: 1,
        };

        index.initialize_schema()?;
        Ok(index)
    }

    /// Initialize database schema and handle migrations
    fn initialize_schema(&mut self) -> AofResult<()> {
        // Create metadata table for version tracking
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to create metadata table: {}", e))
        })?;

        // Create segments table with optimized schema
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS segments (
                base_id INTEGER PRIMARY KEY,
                last_id INTEGER NOT NULL,
                record_count INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                finalized_at INTEGER,
                archived_at INTEGER,
                uncompressed_checksum INTEGER NOT NULL,
                compressed_checksum INTEGER,
                original_size INTEGER NOT NULL,
                compressed_size INTEGER,
                compression_ratio REAL,
                status INTEGER NOT NULL,
                local_path TEXT,
                archive_key TEXT
            )",
            [],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to create segments table: {}", e))
        })?;

        // Create indexes for efficient queries
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_segments_created_at ON segments(created_at)",
            [],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to create created_at index: {}", e))
        })?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_segments_status ON segments(status)",
            [],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to create status index: {}", e))
        })?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_segments_finalized_at ON segments(finalized_at) WHERE finalized_at IS NOT NULL",
            [],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to create finalized_at index: {}", e))
        })?;

        // Check and set schema version
        let existing_version: Option<String> = self.conn.query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        ).optional().map_err(|e| {
            AofError::IndexError(format!("Failed to check schema version: {}", e))
        })?;

        if existing_version.is_none() {
            self.conn.execute(
                "INSERT INTO metadata (key, value) VALUES ('schema_version', '1')",
                [],
            ).map_err(|e| {
                AofError::IndexError(format!("Failed to set schema version: {}", e))
            })?;
        }

        Ok(())
    }

    /// Add a segment from metadata to the index
    pub fn add_segment(&mut self, metadata: crate::aof::record::SegmentMetadata) -> AofResult<()> {
        let entry = SegmentEntry {
            base_id: metadata.base_id,
            last_id: metadata.last_id,
            record_count: metadata.record_count,
            created_at: metadata.created_at,
            finalized_at: None,
            archived_at: None,
            uncompressed_checksum: metadata.checksum,
            compressed_checksum: None,
            original_size: metadata.size,
            compressed_size: None,
            compression_ratio: None,
            status: crate::aof::segment_store::SegmentStatus::Active,
            local_path: Some(format!("{}.log", metadata.base_id)),
            archive_key: None,
        };
        self.add_entry(entry)
    }

    /// Add an entry to the index
    pub fn add_entry(&mut self, entry: SegmentEntry) -> AofResult<()> {
        let status_int = self.status_to_int(&entry.status);
        
        self.conn.execute(
            "INSERT INTO segments (
                base_id, last_id, record_count, created_at, finalized_at, archived_at,
                uncompressed_checksum, compressed_checksum, original_size, compressed_size,
                compression_ratio, status, local_path, archive_key
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                entry.base_id,
                entry.last_id,
                entry.record_count,
                entry.created_at,
                entry.finalized_at,
                entry.archived_at,
                entry.uncompressed_checksum as i64,
                entry.compressed_checksum.map(|c| c as i64),
                entry.original_size,
                entry.compressed_size,
                entry.compression_ratio,
                status_int,
                entry.local_path,
                entry.archive_key
            ],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to insert segment entry: {}", e))
        })?;

        self.version += 1;
        Ok(())
    }

    /// Remove a segment from the index
    pub fn remove_entry(&mut self, base_id: u64) -> AofResult<Option<SegmentEntry>> {
        // First get the entry to return it
        let entry = self.get_segment(base_id)?;

        if entry.is_some() {
            self.conn.execute(
                "DELETE FROM segments WHERE base_id = ?1",
                params![base_id],
            ).map_err(|e| {
                AofError::IndexError(format!("Failed to delete segment entry: {}", e))
            })?;

            self.version += 1;
        }

        Ok(entry)
    }

    /// Find the segment containing the given record ID
    pub fn find_segment_for_id(&self, record_id: u64) -> AofResult<Option<SegmentEntry>> {
        let result = self.conn.query_row(
            "SELECT * FROM segments 
             WHERE base_id <= ?1 AND last_id >= ?1 
             LIMIT 1",
            params![record_id],
            |row| self.row_to_segment_entry(row),
        ).optional().map_err(|e| {
            AofError::IndexError(format!("Failed to find segment for ID {}: {}", record_id, e))
        })?;

        Ok(result)
    }

    /// Find segments overlapping with the given timestamp
    pub fn find_segments_for_timestamp(&self, timestamp: u64) -> AofResult<Vec<SegmentEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM segments 
             WHERE created_at <= ?1 AND (finalized_at IS NULL OR finalized_at >= ?1)
             ORDER BY created_at"
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to prepare timestamp query: {}", e))
        })?;

        let entries = stmt.query_map(params![timestamp], |row| {
            self.row_to_segment_entry(row)
        }).map_err(|e| {
            AofError::IndexError(format!("Failed to execute timestamp query: {}", e))
        })?;

        let mut results = Vec::new();
        for entry in entries {
            results.push(entry.map_err(|e| {
                AofError::IndexError(format!("Failed to parse segment entry: {}", e))
            })?);
        }

        Ok(results)
    }

    /// Get all segments older than the given base_id (for truncation)
    pub fn get_segments_before(&self, min_base_id: u64) -> AofResult<Vec<SegmentEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM segments 
             WHERE base_id < ?1 
             ORDER BY base_id"
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to prepare segments_before query: {}", e))
        })?;

        let entries = stmt.query_map(params![min_base_id], |row| {
            self.row_to_segment_entry(row)
        }).map_err(|e| {
            AofError::IndexError(format!("Failed to execute segments_before query: {}", e))
        })?;

        let mut results = Vec::new();
        for entry in entries {
            results.push(entry.map_err(|e| {
                AofError::IndexError(format!("Failed to parse segment entry: {}", e))
            })?);
        }

        Ok(results)
    }

    /// Get segment count
    pub fn len(&self) -> AofResult<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM segments",
            [],
            |row| row.get(0),
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to get segment count: {}", e))
        })?;

        Ok(count as usize)
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> AofResult<bool> {
        Ok(self.len()? == 0)
    }

    /// Get the latest finalized segment
    pub fn get_latest_finalized_segment(&self) -> AofResult<Option<SegmentEntry>> {
        let result = self.conn.query_row(
            "SELECT * FROM segments 
             WHERE finalized_at IS NOT NULL 
             ORDER BY base_id DESC 
             LIMIT 1",
            [],
            |row| self.row_to_segment_entry(row),
        ).optional().map_err(|e| {
            AofError::IndexError(format!("Failed to get latest finalized segment: {}", e))
        })?;

        Ok(result)
    }

    /// Get the maximum last_id across all segments for O(1) recovery
    pub fn get_max_last_id(&self) -> AofResult<Option<u64>> {
        let result: Option<i64> = self.conn.query_row(
            "SELECT MAX(last_id) FROM segments",
            [],
            |row| row.get(0),
        ).optional().map_err(|e| {
            AofError::IndexError(format!("Failed to get max last_id: {}", e))
        })?;

        Ok(result.map(|id| id as u64))
    }

    /// Get the active segment (if any) for recovery
    pub fn get_active_segment(&self) -> AofResult<Option<SegmentEntry>> {
        let result = self.conn.query_row(
            "SELECT * FROM segments 
             WHERE status = 0 
             LIMIT 1",
            [],
            |row| self.row_to_segment_entry(row),
        ).optional().map_err(|e| {
            AofError::IndexError(format!("Failed to get active segment: {}", e))
        })?;

        Ok(result)
    }

    /// Get all segments sorted by base_id
    pub fn get_all_segments(&self) -> AofResult<Vec<SegmentEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM segments ORDER BY base_id"
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to prepare all_segments query: {}", e))
        })?;

        let entries = stmt.query_map([], |row| {
            self.row_to_segment_entry(row)
        }).map_err(|e| {
            AofError::IndexError(format!("Failed to execute all_segments query: {}", e))
        })?;

        let mut results = Vec::new();
        for entry in entries {
            results.push(entry.map_err(|e| {
                AofError::IndexError(format!("Failed to parse segment entry: {}", e))
            })?);
        }

        Ok(results)
    }

    /// Get index version
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Sync the database to disk
    pub fn sync(&self) -> AofResult<()> {
        // Force WAL checkpoint to ensure data is written to main database
        self.conn.execute("PRAGMA wal_checkpoint(FULL)", [])
            .map_err(|e| AofError::IndexError(format!("Failed to sync database: {}", e)))?;
        Ok(())
    }

    /// List all segments in the index (alias for get_all_segments)
    pub fn list_all_segments(&self) -> AofResult<Vec<SegmentEntry>> {
        self.get_all_segments()
    }

    /// Find segment containing a specific record ID (alias for find_segment_for_id)
    pub fn find_segment_for_record(&self, record_id: u64) -> AofResult<Option<SegmentEntry>> {
        self.find_segment_for_id(record_id)
    }

    /// Get a specific segment by base_id
    pub fn get_segment(&self, base_id: u64) -> AofResult<Option<SegmentEntry>> {
        let result = self.conn.query_row(
            "SELECT * FROM segments WHERE base_id = ?1",
            params![base_id],
            |row| self.row_to_segment_entry(row),
        ).optional().map_err(|e| {
            AofError::IndexError(format!("Failed to get segment {}: {}", base_id, e))
        })?;

        Ok(result)
    }

    /// Get the next segment after the given base_id
    pub fn get_next_segment_after(&self, base_id: u64) -> AofResult<Option<SegmentEntry>> {
        let result = self.conn.query_row(
            "SELECT * FROM segments 
             WHERE base_id > ?1 
             ORDER BY base_id 
             LIMIT 1",
            params![base_id],
            |row| self.row_to_segment_entry(row),
        ).optional().map_err(|e| {
            AofError::IndexError(format!("Failed to get next segment after {}: {}", base_id, e))
        })?;

        Ok(result)
    }

    /// Update segment status (finalized flag)
    pub fn update_segment_status(&mut self, base_id: u64, finalized: bool) -> AofResult<()> {
        if finalized {
            let finalized_at = crate::aof::record::current_timestamp();
            self.conn.execute(
                "UPDATE segments 
                 SET finalized_at = ?1, status = 1 
                 WHERE base_id = ?2",
                params![finalized_at, base_id],
            ).map_err(|e| {
                AofError::IndexError(format!("Failed to update segment status: {}", e))
            })?;
        } else {
            self.conn.execute(
                "UPDATE segments 
                 SET finalized_at = NULL, status = 0 
                 WHERE base_id = ?1",
                params![base_id],
            ).map_err(|e| {
                AofError::IndexError(format!("Failed to update segment status: {}", e))
            })?;
        }

        self.version += 1;
        Ok(())
    }

    /// Update an existing segment entry
    pub fn update_entry(&mut self, entry: SegmentEntry) -> AofResult<()> {
        let status_int = self.status_to_int(&entry.status);
        
        self.conn.execute(
            "UPDATE segments SET 
                last_id = ?1, record_count = ?2, created_at = ?3, finalized_at = ?4, archived_at = ?5,
                uncompressed_checksum = ?6, compressed_checksum = ?7, original_size = ?8, 
                compressed_size = ?9, compression_ratio = ?10, status = ?11, local_path = ?12, 
                archive_key = ?13
             WHERE base_id = ?14",
            params![
                entry.last_id,
                entry.record_count,
                entry.created_at,
                entry.finalized_at,
                entry.archived_at,
                entry.uncompressed_checksum as i64,
                entry.compressed_checksum.map(|c| c as i64),
                entry.original_size,
                entry.compressed_size,
                entry.compression_ratio,
                status_int,
                entry.local_path,
                entry.archive_key,
                entry.base_id
            ],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to update segment entry: {}", e))
        })?;

        self.version += 1;
        Ok(())
    }

    /// Update segment tail (last_id and size)
    pub fn update_segment_tail(&mut self, base_id: u64, last_id: u64, size: u64) -> AofResult<()> {
        println!(
            "DEBUG: update_segment_tail - base_id={}, last_id={}, size={}",
            base_id, last_id, size
        );

        // Calculate new record count, preventing underflow
        let record_count = if last_id >= base_id {
            last_id - base_id + 1
        } else {
            0
        };

        self.conn.execute(
            "UPDATE segments 
             SET last_id = ?1, original_size = ?2, record_count = ?3
             WHERE base_id = ?4",
            params![last_id, size, record_count, base_id],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to update segment tail: {}", e))
        })?;

        println!("DEBUG: Successfully updated segment entry in database");
        self.version += 1;
        Ok(())
    }

    /// Add a segment entry to the index (alias for add_entry)
    pub fn add_segment_entry(&self, entry: &SegmentEntry) -> AofResult<()> {
        // Note: This is a read-only reference, so we can't modify self.version
        // This is a design inconsistency that should be addressed
        let status_int = self.status_to_int(&entry.status);
        
        self.conn.execute(
            "INSERT INTO segments (
                base_id, last_id, record_count, created_at, finalized_at, archived_at,
                uncompressed_checksum, compressed_checksum, original_size, compressed_size,
                compression_ratio, status, local_path, archive_key
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                entry.base_id,
                entry.last_id,
                entry.record_count,
                entry.created_at,
                entry.finalized_at,
                entry.archived_at,
                entry.uncompressed_checksum as i64,
                entry.compressed_checksum.map(|c| c as i64),
                entry.original_size,
                entry.compressed_size,
                entry.compression_ratio,
                status_int,
                entry.local_path,
                entry.archive_key
            ],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to insert segment entry: {}", e))
        })?;

        Ok(())
    }

    /// Update a segment entry in the index (alias for update_entry)
    pub fn update_segment_entry(&self, entry: &SegmentEntry) -> AofResult<()> {
        // Note: This is a read-only reference, so we can't modify self.version
        let status_int = self.status_to_int(&entry.status);
        
        self.conn.execute(
            "UPDATE segments SET 
                last_id = ?1, record_count = ?2, created_at = ?3, finalized_at = ?4, archived_at = ?5,
                uncompressed_checksum = ?6, compressed_checksum = ?7, original_size = ?8, 
                compressed_size = ?9, compression_ratio = ?10, status = ?11, local_path = ?12, 
                archive_key = ?13
             WHERE base_id = ?14",
            params![
                entry.last_id,
                entry.record_count,
                entry.created_at,
                entry.finalized_at,
                entry.archived_at,
                entry.uncompressed_checksum as i64,
                entry.compressed_checksum.map(|c| c as i64),
                entry.original_size,
                entry.compressed_size,
                entry.compression_ratio,
                status_int,
                entry.local_path,
                entry.archive_key,
                entry.base_id
            ],
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to update segment entry: {}", e))
        })?;

        Ok(())
    }

    /// Get total segment count
    pub fn get_segment_count(&self) -> AofResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM segments",
            [],
            |row| row.get(0),
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to get segment count: {}", e))
        })?;

        Ok(count as u64)
    }

    /// Get segments for archiving (oldest first, up to count)
    pub fn get_segments_for_archiving(&self, count: u64) -> AofResult<Vec<SegmentEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM segments 
             WHERE finalized_at IS NOT NULL 
             ORDER BY created_at 
             LIMIT ?1"
        ).map_err(|e| {
            AofError::IndexError(format!("Failed to prepare archiving query: {}", e))
        })?;

        let entries = stmt.query_map(params![count], |row| {
            self.row_to_segment_entry(row)
        }).map_err(|e| {
            AofError::IndexError(format!("Failed to execute archiving query: {}", e))
        })?;

        let mut results = Vec::new();
        for entry in entries {
            results.push(entry.map_err(|e| {
                AofError::IndexError(format!("Failed to parse segment entry: {}", e))
            })?);
        }

        Ok(results)
    }

    /// Helper function to convert database row to SegmentEntry
    fn row_to_segment_entry(&self, row: &Row) -> SqliteResult<SegmentEntry> {
        let status_int: i32 = row.get("status")?;
        let status = self.int_to_status(status_int);
        
        Ok(SegmentEntry {
            base_id: row.get::<_, i64>("base_id")? as u64,
            last_id: row.get::<_, i64>("last_id")? as u64,
            record_count: row.get::<_, i64>("record_count")? as u64,
            created_at: row.get::<_, i64>("created_at")? as u64,
            finalized_at: row.get::<_, Option<i64>>("finalized_at")?.map(|t| t as u64),
            archived_at: row.get::<_, Option<i64>>("archived_at")?.map(|t| t as u64),
            uncompressed_checksum: row.get::<_, i64>("uncompressed_checksum")? as u64,
            compressed_checksum: row.get::<_, Option<i64>>("compressed_checksum")?.map(|c| c as u64),
            original_size: row.get::<_, i64>("original_size")? as u64,
            compressed_size: row.get::<_, Option<i64>>("compressed_size")?.map(|s| s as u64),
            compression_ratio: row.get("compression_ratio")?,
            status,
            local_path: row.get("local_path")?,
            archive_key: row.get("archive_key")?,
        })
    }

    /// Convert SegmentStatus to integer for database storage
    fn status_to_int(&self, status: &crate::aof::segment_store::SegmentStatus) -> i32 {
        match status {
            crate::aof::segment_store::SegmentStatus::Active => 0,
            crate::aof::segment_store::SegmentStatus::Finalized => 1,
            crate::aof::segment_store::SegmentStatus::ArchivedCached => 2,
            crate::aof::segment_store::SegmentStatus::ArchivedRemote => 3,
        }
    }

    /// Convert integer to SegmentStatus from database
    fn int_to_status(&self, status_int: i32) -> crate::aof::segment_store::SegmentStatus {
        match status_int {
            0 => crate::aof::segment_store::SegmentStatus::Active,
            1 => crate::aof::segment_store::SegmentStatus::Finalized,
            2 => crate::aof::segment_store::SegmentStatus::ArchivedCached,
            3 => crate::aof::segment_store::SegmentStatus::ArchivedRemote,
            _ => crate::aof::segment_store::SegmentStatus::Active, // Default fallback
        }
    }
}

// Note: SQLite connections are not thread-safe by default, but WAL mode allows
// multiple readers with a single writer, which matches our use case.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aof::record::{SegmentMetadata, current_timestamp};
    use crate::aof::segment_store::SegmentStatus;
    use tempfile::tempdir;
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
    fn test_sqlite_index_creation_and_open() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");

        // Test creation
        let index = SqliteSegmentIndex::open(&index_path).unwrap();
        assert_eq!(index.version, 1);
        drop(index);

        // Test reopening existing index
        let index = SqliteSegmentIndex::open(&index_path).unwrap();
        assert_eq!(index.version, 1);
    }

    #[test]
    fn test_sqlite_add_and_find_single_entry() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let entry = create_test_entry(1, 50, 1000);
        index.add_entry(entry.clone()).unwrap();

        // Verify the entry was added
        assert_eq!(index.len().unwrap(), 1);
        assert!(!index.is_empty().unwrap());
        assert_eq!(index.version, 2); // Should increment after adding

        // Test finding by ID within range
        let found = index.find_segment_for_id(25).unwrap().unwrap();
        assert_eq!(found.base_id, 1);
        assert_eq!(found.last_id, 50);

        // Test finding exact boundaries
        let found = index.find_segment_for_id(1).unwrap().unwrap();
        assert_eq!(found.base_id, 1);

        let found = index.find_segment_for_id(50).unwrap().unwrap();
        assert_eq!(found.base_id, 1);

        // Test finding outside range
        assert!(index.find_segment_for_id(0).unwrap().is_none());
        assert!(index.find_segment_for_id(51).unwrap().is_none());
    }

    #[test]
    fn test_sqlite_multiple_entries_ordered() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        // Add entries in order
        let entries = vec![
            create_test_entry(1, 50, 1000),
            create_test_entry(51, 100, 1500),
            create_test_entry(101, 150, 2000),
            create_test_entry(151, 200, 2500),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }

        assert_eq!(index.len().unwrap(), 4);

        // Test finding in each segment
        let test_cases = vec![
            (25, 1),    // First segment
            (75, 51),   // Second segment
            (125, 101), // Third segment
            (175, 151), // Fourth segment
        ];

        for (record_id, expected_base_id) in test_cases {
            let found = index.find_segment_for_id(record_id).unwrap().unwrap();
            assert_eq!(
                found.base_id, expected_base_id,
                "Failed to find correct segment for record_id {}",
                record_id
            );
        }

        // Test boundary conditions
        let found = index.find_segment_for_id(50).unwrap().unwrap();
        assert_eq!(found.base_id, 1);

        let found = index.find_segment_for_id(51).unwrap().unwrap();
        assert_eq!(found.base_id, 51);

        // Test outside ranges
        assert!(index.find_segment_for_id(0).unwrap().is_none());
        assert!(index.find_segment_for_id(201).unwrap().is_none());
    }

    #[test]
    fn test_sqlite_multiple_entries_unordered() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        // Add entries in random order
        let entries = vec![
            create_test_entry(101, 150, 2000),
            create_test_entry(1, 50, 1000),
            create_test_entry(151, 200, 2500),
            create_test_entry(51, 100, 1500),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }

        assert_eq!(index.len().unwrap(), 4);

        // Should still work correctly despite unordered insertion
        let test_cases = vec![(25, 1), (75, 51), (125, 101), (175, 151)];

        for (record_id, expected_base_id) in test_cases {
            let found = index.find_segment_for_id(record_id).unwrap().unwrap();
            assert_eq!(found.base_id, expected_base_id);
        }
    }

    #[test]
    fn test_sqlite_remove_entries() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        // Add test entries
        let entries = vec![
            create_test_entry(1, 50, 1000),
            create_test_entry(51, 100, 1500),
            create_test_entry(101, 150, 2000),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }

        assert_eq!(index.len().unwrap(), 3);

        // Remove middle entry
        let removed = index.remove_entry(51).unwrap().unwrap();
        assert_eq!(removed.base_id, 51);
        assert_eq!(index.len().unwrap(), 2);

        // Should not find the removed segment
        assert!(index.find_segment_for_id(75).unwrap().is_none());

        // Other segments should still be findable
        let found = index.find_segment_for_id(25).unwrap().unwrap();
        assert_eq!(found.base_id, 1);

        let found = index.find_segment_for_id(125).unwrap().unwrap();
        assert_eq!(found.base_id, 101);

        // Remove non-existent entry
        let removed = index.remove_entry(999).unwrap();
        assert!(removed.is_none());
        assert_eq!(index.len().unwrap(), 2); // Should remain unchanged
    }

    #[test]
    fn test_sqlite_timestamp_queries() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        // Add entries with different timestamps
        let entries = vec![
            create_test_entry(1, 50, 1000),
            create_test_entry(51, 100, 1500),
            create_test_entry(101, 150, 2000),
            create_test_entry(151, 200, 2500),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }

        // Test finding by exact timestamp - segments with created_at <= timestamp
        let segments = index.find_segments_for_timestamp(1500).unwrap();
        assert_eq!(segments.len(), 2);
        assert!(segments.iter().any(|s| s.base_id == 1));
        assert!(segments.iter().any(|s| s.base_id == 51));

        // Test finding by earlier timestamp - should only match first segment
        let segments = index.find_segments_for_timestamp(1200).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].base_id, 1);

        // Test finding by later timestamp - should match all segments
        let segments = index.find_segments_for_timestamp(3000).unwrap();
        assert_eq!(segments.len(), 4);
    }

    #[test]
    fn test_sqlite_update_segment_entry() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let mut entry = create_test_entry(10, 20, 1_000);
        index.add_entry(entry.clone()).unwrap();

        // Update the entry
        entry.finalize(1500);
        entry.archive(
            "segment_10_archived.zst".to_string(),
            2000,
            Some(512),
            Some(0x87654321),
        );

        index.update_entry(entry.clone()).unwrap();

        // Verify the update
        let found = index.find_segment_for_id(15).unwrap().unwrap();
        assert_eq!(found.status, SegmentStatus::ArchivedRemote);
        assert_eq!(found.finalized_at, Some(1500));
        assert_eq!(found.archived_at, Some(2000));
        assert_eq!(found.compressed_size, Some(512));
    }

    #[test]
    fn test_sqlite_get_segments_before() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        // Add test entries
        let entries = vec![
            create_test_entry(1, 50, 1000),
            create_test_entry(51, 100, 1500),
            create_test_entry(101, 150, 2000),
            create_test_entry(151, 200, 2500),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }

        // Test getting segments before various IDs
        let segments = index.get_segments_before(51).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].base_id, 1);

        let segments = index.get_segments_before(101).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].base_id, 1);
        assert_eq!(segments[1].base_id, 51);

        let segments = index.get_segments_before(1).unwrap();
        assert_eq!(segments.len(), 0);

        let segments = index.get_segments_before(999).unwrap();
        assert_eq!(segments.len(), 4); // All segments
    }

    #[test]
    fn test_sqlite_version_tracking() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let initial_version = index.version;
        assert_eq!(initial_version, 1);

        // Adding entry should increment version
        let entry = create_test_entry(1, 50, 1000);
        index.add_entry(entry.clone()).unwrap();
        assert_eq!(index.version, initial_version + 1);

        // Updating entry should increment version
        index.update_entry(entry).unwrap();
        assert_eq!(index.version, initial_version + 2);

        // Removing entry should increment version
        let _removed = index.remove_entry(1).unwrap();
        assert_eq!(index.version, initial_version + 3);

        // Removing non-existent entry should NOT increment version
        let current_version = index.version;
        let _removed = index.remove_entry(999).unwrap();
        assert_eq!(index.version, current_version);
    }

    #[test]
    fn test_sqlite_persistence() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");

        // Create index and add data
        {
            let mut index = SqliteSegmentIndex::open(&index_path).unwrap();
            let entries = vec![
                create_test_entry(1, 50, 1000),
                create_test_entry(51, 100, 1500),
            ];
            for entry in entries {
                index.add_entry(entry).unwrap();
            }
        } // index dropped here

        // Reopen and verify data persisted
        {
            let index = SqliteSegmentIndex::open(&index_path).unwrap();
            assert_eq!(index.len().unwrap(), 2);

            let found = index.find_segment_for_id(25).unwrap().unwrap();
            assert_eq!(found.base_id, 1);

            let found = index.find_segment_for_id(75).unwrap().unwrap();
            assert_eq!(found.base_id, 51);
        }
    }

    #[test]
    fn test_sqlite_edge_cases() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        // Test with empty index
        assert_eq!(index.len().unwrap(), 0);
        assert!(index.is_empty().unwrap());
        assert!(index.find_segment_for_id(1).unwrap().is_none());

        // Test with single record segments
        let single_record_entry = create_test_entry(100, 100, 1000);
        index.add_entry(single_record_entry).unwrap();

        let found = index.find_segment_for_id(100).unwrap().unwrap();
        assert_eq!(found.base_id, 100);
        assert_eq!(found.last_id, 100);

        // Test with very large IDs
        let large_id_entry = create_test_entry(u64::MAX - 100, u64::MAX - 50, 2000);
        index.add_entry(large_id_entry).unwrap();

        let found = index.find_segment_for_id(u64::MAX - 75).unwrap().unwrap();
        assert_eq!(found.base_id, u64::MAX - 100);
    }

    #[test]
    fn test_sqlite_max_last_id() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        // Empty index
        assert!(index.get_max_last_id().unwrap().is_none());

        // Add entries with various last_ids
        let entries = vec![
            create_test_entry(1, 50, 1000),
            create_test_entry(51, 100, 1500),
            create_test_entry(101, 150, 2000),
            create_test_entry(151, 300, 2500), // Highest last_id
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }

        let max_last_id = index.get_max_last_id().unwrap().unwrap();
        assert_eq!(max_last_id, 300);
    }

    #[test]
    fn test_sqlite_active_and_finalized_segments() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let mut entry1 = create_test_entry(1, 50, 1000);
        let entry2 = create_test_entry(51, 100, 1500);
        
        // Finalize entry1
        entry1.finalize(1200);
        
        index.add_entry(entry1.clone()).unwrap();
        index.add_entry(entry2.clone()).unwrap();

        // Test getting active segment
        let active = index.get_active_segment().unwrap().unwrap();
        assert_eq!(active.base_id, 51);
        assert_eq!(active.status, SegmentStatus::Active);

        // Test getting latest finalized segment
        let finalized = index.get_latest_finalized_segment().unwrap().unwrap();
        assert_eq!(finalized.base_id, 1);
        assert_eq!(finalized.status, SegmentStatus::Finalized);
    }

    #[test]
    fn test_sqlite_update_segment_tail() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let entry = create_test_entry(1, 50, 1000);
        index.add_entry(entry).unwrap();

        // Update segment tail
        index.update_segment_tail(1, 75, 2048).unwrap();

        // Verify the update
        let found = index.get_segment(1).unwrap().unwrap();
        assert_eq!(found.last_id, 75);
        assert_eq!(found.original_size, 2048);
        assert_eq!(found.record_count, 75); // 75 - 1 + 1
    }

    #[test]
    fn test_sqlite_update_segment_status() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let entry = create_test_entry(1, 50, 1000);
        index.add_entry(entry).unwrap();

        // Finalize the segment
        index.update_segment_status(1, true).unwrap();

        let found = index.get_segment(1).unwrap().unwrap();
        assert_eq!(found.status, SegmentStatus::Finalized);
        assert!(found.finalized_at.is_some());

        // Un-finalize the segment
        index.update_segment_status(1, false).unwrap();

        let found = index.get_segment(1).unwrap().unwrap();
        assert_eq!(found.status, SegmentStatus::Active);
        assert!(found.finalized_at.is_none());
    }

    #[test]
    fn test_sqlite_get_segments_for_archiving() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let mut entry1 = create_test_entry(1, 50, 1000);
        let mut entry2 = create_test_entry(51, 100, 1500);
        let mut entry3 = create_test_entry(101, 150, 2000);
        let entry4 = create_test_entry(151, 200, 2500); // Not finalized

        // Finalize first three entries
        entry1.finalize(1200);
        entry2.finalize(1700);
        entry3.finalize(2200);

        index.add_entry(entry1).unwrap();
        index.add_entry(entry2).unwrap();
        index.add_entry(entry3).unwrap();
        index.add_entry(entry4).unwrap(); // Active, not finalized

        // Get segments for archiving (should only return finalized ones, oldest first)
        let segments = index.get_segments_for_archiving(2).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].base_id, 1);   // Oldest
        assert_eq!(segments[1].base_id, 51);  // Second oldest
    }

    #[test]
    fn test_sqlite_get_next_segment_after() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let entries = vec![
            create_test_entry(1, 50, 1000),
            create_test_entry(101, 150, 2000),
            create_test_entry(201, 250, 3000),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }

        // Test getting next segment after various IDs
        let next = index.get_next_segment_after(1).unwrap().unwrap();
        assert_eq!(next.base_id, 101);

        let next = index.get_next_segment_after(50).unwrap().unwrap();
        assert_eq!(next.base_id, 101);

        let next = index.get_next_segment_after(101).unwrap().unwrap();
        assert_eq!(next.base_id, 201);

        // No next segment after the last one
        let next = index.get_next_segment_after(201).unwrap();
        assert!(next.is_none());
    }

    #[test]
    fn test_sqlite_wal_mode_configuration() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        
        // Create the index
        let index = SqliteSegmentIndex::open(&index_path).unwrap();
        
        // Verify WAL mode is enabled by checking journal mode
        let journal_mode: String = index.conn.query_row(
            "PRAGMA journal_mode",
            [],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(journal_mode.to_lowercase(), "wal");
    }

    #[test]
    fn test_sqlite_sync_functionality() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let entry = create_test_entry(1, 50, 1000);
        index.add_entry(entry).unwrap();

        // Test sync operation
        assert!(index.sync().is_ok());
    }

    #[test]
    fn test_sqlite_schema_version_persistence() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index.db");

        // Create index
        {
            let _index = SqliteSegmentIndex::open(&index_path).unwrap();
        }

        // Reopen and verify schema version is persisted
        {
            let index = SqliteSegmentIndex::open(&index_path).unwrap();
            let version: String = index.conn.query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(version, "1");
        }
    }

    // ===== BENCHMARK TESTS =====

    #[test]
    fn benchmark_sqlite_bulk_inserts() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("benchmark_index.db");
        let mut index = SqliteSegmentIndex::open(&index_path).unwrap();

        let segment_count = 10_000;
        let start_time = std::time::Instant::now();

        // Insert 10,000 segments
        for i in 0..segment_count {
            let base_id = i * 1000;
            let last_id = base_id + 999;
            let entry = create_test_entry(base_id, last_id, current_timestamp());
            index.add_entry(entry).unwrap();
        }

        let elapsed = start_time.elapsed();
        println!("Inserted {} segments in {:?} ({:.2} segments/sec)", 
                 segment_count, elapsed, segment_count as f64 / elapsed.as_secs_f64());

        // Verify all entries were inserted
        assert_eq!(index.len().unwrap(), segment_count as usize);

        // Benchmark lookups
        let lookup_start = std::time::Instant::now();
        let lookup_count = 1000;
        
        for _ in 0..lookup_count {
            let segment_idx = rand::random::<u64>() % segment_count;
            let base_id = segment_idx * 1000;
            let record_id = base_id + 500; // Middle of segment
            let _found = index.find_segment_for_id(record_id).unwrap();
        }

        let lookup_elapsed = lookup_start.elapsed();
        println!("Performed {} lookups in {:?} ({:.2} lookups/sec)",
                 lookup_count, lookup_elapsed, lookup_count as f64 / lookup_elapsed.as_secs_f64());
    }

    #[test]
    fn benchmark_sqlite_vs_btree_performance() {
        use std::collections::BTreeMap;
        
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("perf_index.db");
        let mut sqlite_index = SqliteSegmentIndex::open(&index_path).unwrap();
        let mut btree_index: BTreeMap<u64, SegmentEntry> = BTreeMap::new();

        let segment_count = 5_000;
        let entries: Vec<_> = (0..segment_count)
            .map(|i| {
                let base_id = i * 100;
                let last_id = base_id + 99;
                create_test_entry(base_id, last_id, current_timestamp())
            })
            .collect();

        // Benchmark SQLite inserts
        let sqlite_start = std::time::Instant::now();
        for entry in &entries {
            sqlite_index.add_entry(entry.clone()).unwrap();
        }
        let sqlite_insert_time = sqlite_start.elapsed();

        // Benchmark BTreeMap inserts
        let btree_start = std::time::Instant::now();
        for entry in &entries {
            btree_index.insert(entry.base_id, entry.clone());
        }
        let btree_insert_time = btree_start.elapsed();

        println!("SQLite insert time: {:?} ({:.2} ops/sec)", 
                 sqlite_insert_time, segment_count as f64 / sqlite_insert_time.as_secs_f64());
        println!("BTreeMap insert time: {:?} ({:.2} ops/sec)", 
                 btree_insert_time, segment_count as f64 / btree_insert_time.as_secs_f64());

        // Benchmark lookups
        let lookup_count = 1000;
        
        // SQLite lookups
        let sqlite_lookup_start = std::time::Instant::now();
        for _ in 0..lookup_count {
            let segment_idx = rand::random::<u64>() % segment_count;
            let base_id = segment_idx * 100;
            let record_id = base_id + 50;
            let _found = sqlite_index.find_segment_for_id(record_id).unwrap();
        }
        let sqlite_lookup_time = sqlite_lookup_start.elapsed();

        // BTreeMap lookups (find containing segment)
        let btree_lookup_start = std::time::Instant::now();
        for _ in 0..lookup_count {
            let segment_idx = rand::random::<u64>() % segment_count;
            let base_id = segment_idx * 100;
            let record_id = base_id + 50;
            
            // Find segment containing record_id
            let _found = btree_index.range(..=record_id)
                .next_back()
                .and_then(|(_, entry)| {
                    if entry.contains_record(record_id) {
                        Some(entry)
                    } else {
                        None
                    }
                });
        }
        let btree_lookup_time = btree_lookup_start.elapsed();

        println!("SQLite lookup time: {:?} ({:.2} ops/sec)", 
                 sqlite_lookup_time, lookup_count as f64 / sqlite_lookup_time.as_secs_f64());
        println!("BTreeMap lookup time: {:?} ({:.2} ops/sec)", 
                 btree_lookup_time, lookup_count as f64 / btree_lookup_time.as_secs_f64());
    }

    // ===== BACKUP AND RESTORE TESTS =====

    #[test]
    fn test_sqlite_database_backup() {
        let temp_dir = tempdir().unwrap();
        let original_path = temp_dir.path().join("original.db");
        let backup_path = temp_dir.path().join("backup.db");

        // Create original database with data
        {
            let mut index = SqliteSegmentIndex::open(&original_path).unwrap();
            
            let entries = vec![
                create_test_entry(1, 100, 1000),
                create_test_entry(101, 200, 2000),
                create_test_entry(201, 300, 3000),
            ];

            for entry in &entries {
                index.add_entry(entry.clone()).unwrap();
            }

            // Force sync to ensure data is written
            index.sync().unwrap();
        }

        // Perform backup using SQLite's backup API
        {
            let source = rusqlite::Connection::open(&original_path).unwrap();
            let mut dest = rusqlite::Connection::open(&backup_path).unwrap();
            
            let backup = rusqlite::backup::Backup::new(&source, &mut dest).unwrap();
            backup.run_to_completion(5, std::time::Duration::from_millis(250), None).unwrap();
        }

        // Verify backup by opening and checking data
        {
            let backup_index = SqliteSegmentIndex::open(&backup_path).unwrap();
            assert_eq!(backup_index.len().unwrap(), 3);
            
            let found = backup_index.find_segment_for_id(150).unwrap().unwrap();
            assert_eq!(found.base_id, 101);
            assert_eq!(found.last_id, 200);
        }
    }

    #[test]
    fn test_sqlite_sql_dump_and_restore() {
        let temp_dir = tempdir().unwrap();
        let original_path = temp_dir.path().join("original.db");
        let restored_path = temp_dir.path().join("restored.db");
        let dump_path = temp_dir.path().join("dump.sql");

        // Create original database with data
        {
            let mut index = SqliteSegmentIndex::open(&original_path).unwrap();
            
            let entries = vec![
                create_test_entry(1, 50, 1000),
                create_test_entry(51, 100, 1500),
                create_test_entry(101, 150, 2000),
            ];

            for entry in &entries {
                index.add_entry(entry.clone()).unwrap();
            }
            index.sync().unwrap();
        }

        // Create SQL dump (simulate command line tool)
        {
            let conn = rusqlite::Connection::open(&original_path).unwrap();
            let mut dump_content = String::new();
            
            // Get schema
            let mut stmt = conn.prepare("SELECT sql FROM sqlite_master WHERE type='table'").unwrap();
            let schema_iter = stmt.query_map([], |row| {
                let sql: String = row.get(0)?;
                Ok(sql)
            }).unwrap();

            for schema in schema_iter {
                dump_content.push_str(&schema.unwrap());
                dump_content.push_str(";\n");
            }

            // Get data
            let mut stmt = conn.prepare("SELECT * FROM segments ORDER BY base_id").unwrap();
            let rows = stmt.query_map([], |row| {
                let base_id: i64 = row.get("base_id")?;
                let last_id: i64 = row.get("last_id")?;
                let record_count: i64 = row.get("record_count")?;
                let created_at: i64 = row.get("created_at")?;
                let status: i32 = row.get("status")?;
                let local_path: Option<String> = row.get("local_path")?;
                
                Ok(format!(
                    "INSERT INTO segments (base_id, last_id, record_count, created_at, finalized_at, archived_at, uncompressed_checksum, compressed_checksum, original_size, compressed_size, compression_ratio, status, local_path, archive_key) VALUES ({}, {}, {}, {}, NULL, NULL, {}, NULL, 1024, NULL, NULL, {}, '{}', NULL);",
                    base_id, last_id, record_count, created_at, 0x12345678i64, status, local_path.unwrap_or_default()
                ))
            }).unwrap();

            for row in rows {
                dump_content.push_str(&row.unwrap());
                dump_content.push('\n');
            }

            std::fs::write(&dump_path, dump_content).unwrap();
        }

        // Restore from SQL dump
        {
            let conn = rusqlite::Connection::open(&restored_path).unwrap();
            let dump_content = std::fs::read_to_string(&dump_path).unwrap();
            conn.execute_batch(&dump_content).unwrap();
        }

        // Verify restored database
        {
            let restored_index = SqliteSegmentIndex::open(&restored_path).unwrap();
            assert_eq!(restored_index.len().unwrap(), 3);
            
            let found = restored_index.find_segment_for_id(75).unwrap().unwrap();
            assert_eq!(found.base_id, 51);
            assert_eq!(found.last_id, 100);
        }
    }

    // ===== SNAPSHOT AND RECOVERY TESTS =====

    #[test]
    fn test_sqlite_point_in_time_recovery() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("recovery_test.db");
        let wal_path = temp_dir.path().join("recovery_test.db-wal");

        // Phase 1: Create initial data
        {
            let mut index = SqliteSegmentIndex::open(&db_path).unwrap();
            
            let entries = vec![
                create_test_entry(1, 100, 1000),
                create_test_entry(101, 200, 2000),
            ];

            for entry in &entries {
                index.add_entry(entry.clone()).unwrap();
            }

            // Checkpoint to ensure data is in main database file
            index.sync().unwrap();
        }

        // Create a backup of the main database file at this point
        let backup_path = temp_dir.path().join("backup_checkpoint.db");
        std::fs::copy(&db_path, &backup_path).unwrap();

        // Phase 2: Add more data (this will go to WAL)
        {
            let mut index = SqliteSegmentIndex::open(&db_path).unwrap();
            
            let additional_entries = vec![
                create_test_entry(201, 300, 3000),
                create_test_entry(301, 400, 4000),
            ];

            for entry in &additional_entries {
                index.add_entry(entry.clone()).unwrap();
            }
            // Don't sync - leave data in WAL
        }

        // Verify we have 4 entries
        {
            let index = SqliteSegmentIndex::open(&db_path).unwrap();
            assert_eq!(index.len().unwrap(), 4);
        }

        // Simulate recovery to checkpoint by replacing main db file
        std::fs::copy(&backup_path, &db_path).unwrap();
        // Remove WAL file to simulate point-in-time recovery
        if wal_path.exists() {
            std::fs::remove_file(&wal_path).unwrap();
        }

        // Verify recovery - should only have initial 2 entries
        {
            let recovered_index = SqliteSegmentIndex::open(&db_path).unwrap();
            assert_eq!(recovered_index.len().unwrap(), 2);
            
            let found = recovered_index.find_segment_for_id(150).unwrap().unwrap();
            assert_eq!(found.base_id, 101);
            
            // Entries from phase 2 should not exist
            assert!(recovered_index.find_segment_for_id(250).unwrap().is_none());
        }
    }

    #[test]
    fn test_sqlite_crash_recovery_with_wal() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("crash_test.db");

        // Create database and add initial data
        {
            let mut index = SqliteSegmentIndex::open(&db_path).unwrap();
            
            let entries = vec![
                create_test_entry(1, 50, 1000),
                create_test_entry(51, 100, 2000),
            ];

            for entry in &entries {
                index.add_entry(entry.clone()).unwrap();
            }
            
            // Force a checkpoint to move data to main database
            index.sync().unwrap();
        }

        // Add more data without explicit sync (will remain in WAL)
        {
            let mut index = SqliteSegmentIndex::open(&db_path).unwrap();
            
            let more_entries = vec![
                create_test_entry(101, 150, 3000),
                create_test_entry(151, 200, 4000),
            ];

            for entry in &more_entries {
                index.add_entry(entry.clone()).unwrap();
            }
            // Simulate crash - don't sync
        }

        // Recovery: Open database again (SQLite should recover from WAL automatically)
        {
            let recovered_index = SqliteSegmentIndex::open(&db_path).unwrap();
            
            // Should have all 4 entries - WAL recovery should restore them
            assert_eq!(recovered_index.len().unwrap(), 4);
            
            // Verify all data is accessible
            let found1 = recovered_index.find_segment_for_id(25).unwrap().unwrap();
            assert_eq!(found1.base_id, 1);
            
            let found2 = recovered_index.find_segment_for_id(175).unwrap().unwrap();
            assert_eq!(found2.base_id, 151);
        }
    }

    // ===== CONCURRENT ACCESS TESTS =====

    #[test]
    fn test_sqlite_concurrent_readers() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("concurrent_test.db");

        // Setup database with test data
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

        // Test concurrent readers
        use std::sync::{Arc, Barrier};
        use std::thread;

        let reader_count = 4;
        let barrier = Arc::new(Barrier::new(reader_count));
        let db_path = Arc::new(db_path);
        
        let handles: Vec<_> = (0..reader_count).map(|reader_id| {
            let barrier = barrier.clone();
            let db_path = db_path.clone();
            
            thread::spawn(move || {
                barrier.wait(); // Synchronize start
                
                let index = SqliteSegmentIndex::open(&*db_path).unwrap();
                let mut found_count = 0;
                
                // Each reader performs random lookups
                for _ in 0..1000 {
                    let segment_idx = rand::random::<u64>() % 100;
                    let base_id = segment_idx * 100;
                    let record_id = base_id + 50;
                    
                    if let Ok(Some(_)) = index.find_segment_for_id(record_id) {
                        found_count += 1;
                    }
                }
                
                println!("Reader {} found {} segments", reader_id, found_count);
                found_count
            })
        }).collect();

        // Wait for all readers to complete
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        
        // All readers should have found the same number of segments
        assert!(results.iter().all(|&count| count == 1000));
    }

    #[test]
    fn test_sqlite_reader_while_writing() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("read_write_test.db");

        // Initial setup
        {
            let mut index = SqliteSegmentIndex::open(&db_path).unwrap();
            
            for i in 0..50 {
                let base_id = i * 100;
                let last_id = base_id + 99;
                let entry = create_test_entry(base_id, last_id, current_timestamp());
                index.add_entry(entry).unwrap();
            }
            index.sync().unwrap();
        }

        use std::sync::{Arc, mpsc};
        use std::thread;
        use std::time::Duration;

        let db_path = Arc::new(db_path);
        let (tx, _rx) = mpsc::channel::<String>();
        
        // Writer thread
        let writer_db_path = db_path.clone();
        let writer_tx = tx.clone();
        let writer_handle = thread::spawn(move || {
            let mut index = SqliteSegmentIndex::open(&*writer_db_path).unwrap();
            
            // Add more segments while reader is running
            for i in 50..100 {
                let base_id = i * 100;
                let last_id = base_id + 99;
                let entry = create_test_entry(base_id, last_id, current_timestamp());
                index.add_entry(entry).unwrap();
                
                // Periodic sync to make data visible to readers
                if i % 10 == 0 {
                    index.sync().unwrap();
                    thread::sleep(Duration::from_millis(10));
                }
            }
            
            writer_tx.send("done".to_string()).unwrap();
        });

        // Reader thread
        let reader_db_path = db_path.clone();
        let reader_tx = tx.clone();
        let reader_handle = thread::spawn(move || {
            let index = SqliteSegmentIndex::open(&*reader_db_path).unwrap();
            let mut max_segments_seen = 0;
            
            // Continuously read while writer is working
            for _ in 0..500 {
                let current_count = index.len().unwrap();
                max_segments_seen = max_segments_seen.max(current_count);
                
                // Try random lookups
                let segment_idx = rand::random::<u64>() % 50; // Look in initial range
                let record_id = segment_idx * 100 + 50;
                let _ = index.find_segment_for_id(record_id);
                
                thread::sleep(Duration::from_millis(1));
            }
            
            reader_tx.send(format!("max_seen:{}", max_segments_seen)).unwrap();
        });

        // Wait for both threads
        writer_handle.join().unwrap();
        reader_handle.join().unwrap();

        // Verify final state
        let final_index = SqliteSegmentIndex::open(&*db_path).unwrap();
        assert_eq!(final_index.len().unwrap(), 100);
    }

    // ===== STRESS AND CORRUPTION TESTS =====

    #[test]
    fn test_sqlite_large_dataset_stress() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("stress_test.db");
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        let segment_count = 50_000;
        let batch_size = 1000;

        println!("Starting stress test with {} segments", segment_count);
        let start_time = std::time::Instant::now();

        // Insert in batches for better performance
        for batch in 0..(segment_count / batch_size) {
            let batch_start = std::time::Instant::now();
            
            for i in 0..batch_size {
                let segment_id = batch * batch_size + i;
                let base_id = segment_id * 1000;
                let last_id = base_id + 999;
                let entry = create_test_entry(base_id, last_id, current_timestamp());
                index.add_entry(entry).unwrap();
            }
            
            // Periodic sync to prevent WAL from growing too large
            if batch % 10 == 0 {
                index.sync().unwrap();
                let batch_elapsed = batch_start.elapsed();
                println!("Batch {} complete in {:?} (total: {})", 
                         batch, batch_elapsed, (batch + 1) * batch_size);
            }
        }

        let insert_time = start_time.elapsed();
        println!("Inserted {} segments in {:?}", segment_count, insert_time);

        // Verify database integrity
        assert_eq!(index.len().unwrap(), segment_count as usize);

        // Stress test lookups
        let lookup_start = std::time::Instant::now();
        let lookup_count = 10_000;
        
        for _ in 0..lookup_count {
            let segment_idx = rand::random::<u64>() % segment_count;
            let base_id = segment_idx * 1000;
            let record_id = base_id + 500;
            
            let found = index.find_segment_for_id(record_id).unwrap().unwrap();
            assert_eq!(found.base_id, base_id);
        }

        let lookup_time = lookup_start.elapsed();
        println!("Performed {} lookups in {:?} ({:.2} lookups/sec)", 
                 lookup_count, lookup_time, lookup_count as f64 / lookup_time.as_secs_f64());
    }

    #[test]
    fn test_sqlite_transaction_rollback() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("rollback_test.db");
        let mut index = SqliteSegmentIndex::open(&db_path).unwrap();

        // Add initial data
        let entry1 = create_test_entry(1, 100, 1000);
        index.add_entry(entry1).unwrap();
        assert_eq!(index.len().unwrap(), 1);

        // Simulate a transaction failure by creating an invalid entry
        // (we'll do this by trying to insert a duplicate primary key)
        let duplicate_entry = create_test_entry(1, 200, 2000); // Same base_id
        let result = index.add_entry(duplicate_entry);
        
        // Should fail due to primary key constraint
        assert!(result.is_err());
        
        // Database should still have only 1 entry (transaction rolled back)
        assert_eq!(index.len().unwrap(), 1);
        
        // Original entry should still be accessible
        let found = index.find_segment_for_id(50).unwrap().unwrap();
        assert_eq!(found.base_id, 1);
        assert_eq!(found.last_id, 100);
    }

    #[test]
    fn test_sqlite_wal_checkpoint_behavior() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("wal_test.db");
        let wal_path = temp_dir.path().join("wal_test.db-wal");
        
        {
            let mut index = SqliteSegmentIndex::open(&db_path).unwrap();
            
            // Add some data
            for i in 0..100 {
                let entry = create_test_entry(i * 10, i * 10 + 9, current_timestamp());
                index.add_entry(entry).unwrap();
            }
            
            // Before sync, WAL file should exist and have content
            assert!(wal_path.exists());
            let wal_size_before = std::fs::metadata(&wal_path).unwrap().len();
            assert!(wal_size_before > 0);
            
            // Force checkpoint
            index.sync().unwrap();
            
            // After sync, WAL file might still exist but should be smaller or reset
            // (SQLite may keep the WAL file for future transactions)
            if wal_path.exists() {
                let wal_size_after = std::fs::metadata(&wal_path).unwrap().len();
                println!("WAL size before sync: {}, after sync: {}", wal_size_before, wal_size_after);
                // Content should have been moved to main database
                assert!(wal_size_after <= wal_size_before);
            }
        }

        // Verify data is accessible after reopening (tests WAL recovery)
        {
            let index = SqliteSegmentIndex::open(&db_path).unwrap();
            assert_eq!(index.len().unwrap(), 100);
            
            let found = index.find_segment_for_id(55).unwrap().unwrap();
            assert_eq!(found.base_id, 50);
        }
    }

    #[test]
    fn test_sqlite_database_file_permissions() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("permissions_test.db");
        
        // Create database
        {
            let mut index = SqliteSegmentIndex::open(&db_path).unwrap();
            let entry = create_test_entry(1, 100, current_timestamp());
            index.add_entry(entry).unwrap();
            index.sync().unwrap();
        }

        // Check that database file was created with appropriate permissions
        let metadata = std::fs::metadata(&db_path).unwrap();
        assert!(metadata.is_file());
        assert!(metadata.len() > 0);

        // Verify we can read the database file
        let file_content = std::fs::read(&db_path).unwrap();
        assert!(!file_content.is_empty());
        
        // Check SQLite file signature
        assert_eq!(&file_content[0..16], b"SQLite format 3\0");
    }
}