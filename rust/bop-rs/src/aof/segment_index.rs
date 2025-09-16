//! MDBX-based persistent segment index for archive storage

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::aof::error::{AofError, AofResult};
use crate::aof::segment_store::SegmentEntry;
use crate::mdbx::{self, Cursor, DbFlags, Dbi, Env, EnvFlags, PutFlags, Txn, TxnFlags};

/// MDBX-backed persistent segment index
pub struct MdbxSegmentIndex {
    env: Env,
    /// Database for ID-based lookups: base_id -> SegmentEntry
    id_db: Dbi,
    /// Database for timestamp-based lookups: created_at -> base_id
    time_db: Dbi,
    /// Index metadata
    version: u32,
}

impl MdbxSegmentIndex {
    /// Create or open an MDBX-based segment index
    pub fn open<P: AsRef<Path>>(path: P) -> AofResult<Self> {
        println!(
            "DEBUG: Opening MDBX segment index at path: {:?}",
            path.as_ref()
        );
        // Create MDBX environment
        let env = Env::new()
            .map_err(|e| AofError::IndexError(format!("Failed to create MDBX env: {}", e)))?;

        // Set reasonable defaults for archive index
        env.set_option(mdbx::OptionKey::MaxDbs, 8)
            .map_err(|e| AofError::IndexError(format!("Failed to set max dbs: {}", e)))?;

        env.set_option(mdbx::OptionKey::MaxReaders, 64)
            .map_err(|e| AofError::IndexError(format!("Failed to set max readers: {}", e)))?;

        // Open environment
        let flags = EnvFlags::COALESCE | EnvFlags::LIFORECLAIM;
        env.open(path, flags, 0o644)
            .map_err(|e| AofError::IndexError(format!("Failed to open MDBX env: {}", e)))?;

        // Open databases in a transaction
        let txn = Txn::begin(&env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        let id_db = Dbi::open(&txn, Some("segments_by_id"), DbFlags::CREATE)
            .map_err(|e| AofError::IndexError(format!("Failed to open id_db: {}", e)))?;

        let time_db = Dbi::open(&txn, Some("segments_by_time"), DbFlags::CREATE)
            .map_err(|e| AofError::IndexError(format!("Failed to open time_db: {}", e)))?;

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit transaction: {}", e)))?;

        Ok(Self {
            env,
            id_db,
            time_db,
            version: 1,
        })
    }

    /// Add a segment from metadata to the index
    pub fn add_segment(&mut self, metadata: crate::aof::record::SegmentMetadata) -> AofResult<()> {
        let entry = SegmentEntry {
            base_id: metadata.base_id,
            last_id: metadata.last_id,
            record_count: metadata.record_count,
            created_at: metadata.created_at,
            finalized_at: None, // New segments start as not finalized
            archived_at: None,
            uncompressed_checksum: metadata.checksum as u64,
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

    /// Add an archive entry to the index
    pub fn add_entry(&mut self, entry: SegmentEntry) -> AofResult<()> {
        let txn = Txn::begin(&self.env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        // Serialize entry
        let entry_bytes = bincode::serialize(&entry)
            .map_err(|e| AofError::Serialization(format!("Failed to serialize entry: {}", e)))?;

        // Store in ID database: base_id -> SegmentEntry
        let id_key = entry.base_id.to_le_bytes();
        mdbx::put(&txn, self.id_db, &id_key, &entry_bytes, PutFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to put in id_db: {}", e)))?;

        // Store in time database: created_at -> base_id
        let time_key = entry.created_at.to_le_bytes();
        let id_bytes = entry.base_id.to_le_bytes();
        mdbx::put(&txn, self.time_db, &time_key, &id_bytes, PutFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to put in time_db: {}", e)))?;

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit transaction: {}", e)))?;

        self.version += 1;
        Ok(())
    }

    /// Remove a segment from the index
    pub fn remove_entry(&mut self, base_id: u64) -> AofResult<Option<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        // First, get the entry to find its timestamp
        let id_key = base_id.to_le_bytes();
        let entry = if let Some(entry_bytes) = mdbx::get(&txn, self.id_db, &id_key)
            .map_err(|e| AofError::IndexError(format!("Failed to get from id_db: {}", e)))?
        {
            let entry: SegmentEntry = bincode::deserialize(entry_bytes).map_err(|e| {
                AofError::Serialization(format!("Failed to deserialize entry: {}", e))
            })?;

            // Remove from time database
            let time_key = entry.created_at.to_le_bytes();
            mdbx::del(&txn, self.time_db, &time_key, None).map_err(|e| {
                AofError::IndexError(format!("Failed to delete from time_db: {}", e))
            })?;

            // Remove from ID database
            mdbx::del(&txn, self.id_db, &id_key, None)
                .map_err(|e| AofError::IndexError(format!("Failed to delete from id_db: {}", e)))?;

            Some(entry)
        } else {
            None
        };

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit transaction: {}", e)))?;

        if entry.is_some() {
            self.version += 1;
        }
        Ok(entry)
    }

    /// Find the segment containing the given record ID
    pub fn find_segment_for_id(&self, record_id: u64) -> AofResult<Option<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        // Use cursor to find the segment with highest base_id <= record_id
        let result = {
            let mut cursor = Cursor::open(&txn, self.id_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            let target_key = record_id.to_le_bytes();

            // Position cursor at the target key or the next smaller key
            match cursor.set_range(&target_key) {
                Ok(Some((key, value))) => {
                    // Check if this exact key works
                    let key_id = u64::from_le_bytes(key.try_into().unwrap_or([0; 8]));
                    if key_id <= record_id {
                        let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                            AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                        })?;
                        if entry.contains_record(record_id) {
                            Some(entry)
                        } else {
                            None
                        }
                    } else {
                        // Go to previous entry
                        match cursor.prev() {
                            Ok(Some((key, value))) => {
                                let key_id = u64::from_le_bytes(key.try_into().unwrap_or([0; 8]));
                                if key_id <= record_id {
                                    let entry: SegmentEntry =
                                        bincode::deserialize(value).map_err(|e| {
                                            AofError::Serialization(format!(
                                                "Failed to deserialize entry: {}",
                                                e
                                            ))
                                        })?;
                                    if entry.contains_record(record_id) {
                                        Some(entry)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    }
                }
                Ok(None) => {
                    // No key >= target, check last entry
                    match cursor.last() {
                        Ok(Some((key, value))) => {
                            let key_id = u64::from_le_bytes(key.try_into().unwrap_or([0; 8]));
                            if key_id <= record_id {
                                let entry: SegmentEntry =
                                    bincode::deserialize(value).map_err(|e| {
                                        AofError::Serialization(format!(
                                            "Failed to deserialize entry: {}",
                                            e
                                        ))
                                    })?;
                                if entry.contains_record(record_id) {
                                    Some(entry)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                }
                Err(e) => return Err(AofError::IndexError(format!("Cursor error: {}", e))),
            }
        }; // cursor is dropped here

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(result)
    }

    /// Find segments overlapping with the given timestamp
    pub fn find_segments_for_timestamp(&self, timestamp: u64) -> AofResult<Vec<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let mut results = Vec::new();
        {
            let mut cursor = Cursor::open(&txn, self.time_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            // Start from first entry and check all entries
            if let Ok(Some((mut key, mut id_bytes))) = cursor.first() {
                loop {
                    let base_id = u64::from_le_bytes(id_bytes.try_into().unwrap_or([0; 8]));

                    // Get the full entry from ID database
                    let id_key = base_id.to_le_bytes();
                    if let Ok(Some(entry_bytes)) = mdbx::get(&txn, self.id_db, &id_key) {
                        let entry: SegmentEntry =
                            bincode::deserialize(entry_bytes).map_err(|e| {
                                AofError::Serialization(format!(
                                    "Failed to deserialize entry: {}",
                                    e
                                ))
                            })?;

                        if entry.overlaps_timestamp(timestamp) {
                            results.push(entry);
                        }
                    }

                    match cursor.next() {
                        Ok(Some((next_key, next_id_bytes))) => {
                            key = next_key;
                            id_bytes = next_id_bytes;
                        }
                        _ => break,
                    }
                }
            }
        } // cursor is dropped here

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(results)
    }

    /// Get all segments older than the given base_id (for truncation)
    pub fn get_segments_before(&self, min_base_id: u64) -> AofResult<Vec<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let mut results = Vec::new();
        {
            let mut cursor = Cursor::open(&txn, self.id_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            // Iterate from first entry until we reach min_base_id
            if let Ok(Some((mut key, mut value))) = cursor.first() {
                loop {
                    let base_id = u64::from_le_bytes(key.try_into().unwrap_or([0; 8]));
                    if base_id >= min_base_id {
                        break;
                    }

                    let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                        AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                    })?;
                    results.push(entry);

                    match cursor.next() {
                        Ok(Some((next_key, next_value))) => {
                            key = next_key;
                            value = next_value;
                        }
                        _ => break,
                    }
                }
            }
        } // cursor is dropped here

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(results)
    }

    /// Get segment count
    pub fn len(&self) -> AofResult<usize> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let stat = self
            .id_db
            .stat(&txn)
            .map_err(|e| AofError::IndexError(format!("Failed to get db stats: {}", e)))?;

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(stat.ms_entries as usize)
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> AofResult<bool> {
        Ok(self.len()? == 0)
    }

    /// Get the maximum last_id across all segments for O(1) recovery
    /// This avoids loading all segments and instead scans just the keys
    pub fn get_latest_finalized_segment(&self) -> AofResult<Option<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let mut latest: Option<SegmentEntry> = None;
        {
            let mut cursor = Cursor::open(&txn, self.id_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            if let Ok(Some((mut key, mut value))) = cursor.first() {
                loop {
                    let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                        AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                    })?;

                    if entry.finalized_at.is_some() {
                        match &latest {
                            Some(current) if entry.base_id <= current.base_id => {}
                            _ => latest = Some(entry),
                        }
                    }

                    match cursor.next() {
                        Ok(Some((next_key, next_value))) => {
                            key = next_key;
                            value = next_value;
                        }
                        _ => break,
                    }
                }
            }
        }

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(latest)
    }

    pub fn get_max_last_id(&self) -> AofResult<Option<u64>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let mut max_last_id: Option<u64> = None;

        {
            let mut cursor = Cursor::open(&txn, self.id_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            // Iterate through segments to find max last_id
            if let Ok(Some((_, mut value))) = cursor.first() {
                let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                    AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                })?;
                max_last_id = Some(entry.last_id);

                while let Ok(Some((_, next_value))) = cursor.next() {
                    value = next_value;
                    let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                        AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                    })?;

                    if let Some(current_max) = max_last_id {
                        if entry.last_id > current_max {
                            max_last_id = Some(entry.last_id);
                        }
                    } else {
                        max_last_id = Some(entry.last_id);
                    }
                }
            }
        }

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(max_last_id)
    }

    /// Get the active segment (if any) for recovery
    pub fn get_active_segment(&self) -> AofResult<Option<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let mut active_segment: Option<SegmentEntry> = None;

        {
            let mut cursor = Cursor::open(&txn, self.id_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            if let Ok(Some((_, mut value))) = cursor.first() {
                let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                    AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                })?;

                if entry.status == crate::aof::segment_store::SegmentStatus::Active {
                    active_segment = Some(entry);
                } else {
                    // Continue searching for active segment
                    while let Ok(Some((_, next_value))) = cursor.next() {
                        value = next_value;
                        let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                            AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                        })?;

                        if entry.status == crate::aof::segment_store::SegmentStatus::Active {
                            active_segment = Some(entry);
                            break;
                        }
                    }
                }
            }
        }

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(active_segment)
    }

    /// Get all segments sorted by base_id
    pub fn get_all_segments(&self) -> AofResult<Vec<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let mut results = Vec::new();
        {
            let mut cursor = Cursor::open(&txn, self.id_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            if let Ok(Some((_, mut value))) = cursor.first() {
                let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                    AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                })?;
                results.push(entry);

                while let Ok(Some((_, next_value))) = cursor.next() {
                    value = next_value;
                    let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                        AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                    })?;
                    results.push(entry);
                }
            }
        } // cursor is dropped here

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(results)
    }

    /// Get index version
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Sync the database to disk
    pub fn sync(&self) -> AofResult<()> {
        self.env
            .sync(false, false)
            .map_err(|e| AofError::IndexError(format!("Failed to sync: {}", e)))?;
        Ok(())
    }

    /// List all segments in the index
    pub fn list_all_segments(&self) -> AofResult<Vec<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let mut results = Vec::new();
        {
            let mut cursor = Cursor::open(&txn, self.id_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            if let Ok(Some((mut key, mut value))) = cursor.first() {
                loop {
                    let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                        AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                    })?;
                    results.push(entry);

                    match cursor.next() {
                        Ok(Some((next_key, next_value))) => {
                            key = next_key;
                            value = next_value;
                        }
                        _ => break,
                    }
                }
            }
        }

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(results)
    }

    /// Find segment containing a specific record ID
    pub fn find_segment_for_record(&self, record_id: u64) -> AofResult<Option<SegmentEntry>> {
        self.find_segment_for_id(record_id)
    }

    /// Get a specific segment by base_id
    pub fn get_segment(&self, base_id: u64) -> AofResult<Option<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let id_key = base_id.to_le_bytes();
        let result = if let Some(entry_bytes) = mdbx::get(&txn, self.id_db, &id_key)
            .map_err(|e| AofError::IndexError(format!("Failed to get from id_db: {}", e)))?
        {
            let entry: SegmentEntry = bincode::deserialize(entry_bytes).map_err(|e| {
                AofError::Serialization(format!("Failed to deserialize entry: {}", e))
            })?;
            Some(entry)
        } else {
            None
        };

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(result)
    }

    /// Get the next segment after the given base_id
    pub fn get_next_segment_after(&self, base_id: u64) -> AofResult<Option<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let start_key = (base_id + 1).to_le_bytes();
        let result = {
            let mut cursor = Cursor::open(&txn, self.id_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            // Position cursor at or after the target key
            match cursor.set_range(&start_key) {
                Ok(Some((_key, value))) => {
                    let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                        AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                    })?;
                    Some(entry)
                }
                _ => None,
            }
        };

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(result)
    }

    /// Update segment status (finalized flag)
    pub fn update_segment_status(&mut self, base_id: u64, finalized: bool) -> AofResult<()> {
        let txn = Txn::begin(&self.env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        let id_key = base_id.to_le_bytes();
        if let Some(entry_bytes) = mdbx::get(&txn, self.id_db, &id_key)
            .map_err(|e| AofError::IndexError(format!("Failed to get from id_db: {}", e)))?
        {
            let mut entry: SegmentEntry = bincode::deserialize(entry_bytes).map_err(|e| {
                AofError::Serialization(format!("Failed to deserialize entry: {}", e))
            })?;

            if finalized {
                entry.finalized_at = Some(crate::aof::record::current_timestamp());
                entry.status = crate::aof::segment_store::SegmentStatus::Finalized;
            } else {
                entry.finalized_at = None;
                entry.status = crate::aof::segment_store::SegmentStatus::Active;
            }

            let updated_bytes = bincode::serialize(&entry).map_err(|e| {
                AofError::Serialization(format!("Failed to serialize updated entry: {}", e))
            })?;

            mdbx::put(&txn, self.id_db, &id_key, &updated_bytes, PutFlags::empty())
                .map_err(|e| AofError::IndexError(format!("Failed to update in id_db: {}", e)))?;
        }

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit transaction: {}", e)))?;

        self.version += 1;
        Ok(())
    }

    /// Update an existing segment entry
    pub fn update_entry(&mut self, entry: SegmentEntry) -> AofResult<()> {
        let txn = Txn::begin(&self.env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        // Serialize updated entry
        let entry_bytes = bincode::serialize(&entry)
            .map_err(|e| AofError::Serialization(format!("Failed to serialize entry: {}", e)))?;

        // Update in ID database: base_id -> SegmentEntry
        let id_key = entry.base_id.to_le_bytes();
        mdbx::put(&txn, self.id_db, &id_key, &entry_bytes, PutFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to update in id_db: {}", e)))?;

        // Note: We don't update the time database since created_at shouldn't change

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit transaction: {}", e)))?;

        self.version += 1;
        Ok(())
    }

    /// Update segment tail (last_id and size)
    pub fn update_segment_tail(&mut self, base_id: u64, last_id: u64, size: u64) -> AofResult<()> {
        println!(
            "DEBUG: update_segment_tail - base_id={}, last_id={}, size={}",
            base_id, last_id, size
        );
        let txn = Txn::begin(&self.env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        let id_key = base_id.to_le_bytes();
        if let Some(entry_bytes) = mdbx::get(&txn, self.id_db, &id_key)
            .map_err(|e| AofError::IndexError(format!("Failed to get from id_db: {}", e)))?
        {
            let mut entry: SegmentEntry = bincode::deserialize(entry_bytes).map_err(|e| {
                AofError::Serialization(format!("Failed to deserialize entry: {}", e))
            })?;

            println!(
                "DEBUG: Before update - last_id={}, record_count={}, size={}",
                entry.last_id, entry.record_count, entry.original_size
            );

            entry.last_id = last_id;
            entry.original_size = size;
            // Prevent underflow when last_id < base_id (can happen during rapid segment rotation)
            entry.record_count = if last_id >= entry.base_id {
                last_id - entry.base_id + 1
            } else {
                0
            };

            println!(
                "DEBUG: After update - last_id={}, record_count={}, size={}",
                entry.last_id, entry.record_count, entry.original_size
            );

            let updated_bytes = bincode::serialize(&entry).map_err(|e| {
                AofError::Serialization(format!("Failed to serialize updated entry: {}", e))
            })?;

            mdbx::put(&txn, self.id_db, &id_key, &updated_bytes, PutFlags::empty())
                .map_err(|e| AofError::IndexError(format!("Failed to update in id_db: {}", e)))?;

            println!("DEBUG: Successfully updated segment entry in database");
        } else {
            println!("DEBUG: No entry found for base_id={}", base_id);
        }

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit transaction: {}", e)))?;

        // Force sync to disk to ensure data is persisted
        self.env
            .sync(true, false)
            .map_err(|e| AofError::IndexError(format!("Failed to sync database: {}", e)))?;

        println!("DEBUG: Transaction committed and synced successfully");
        self.version += 1;
        Ok(())
    }

    /// Add a segment entry to the index
    pub fn add_segment_entry(&self, entry: &SegmentEntry) -> AofResult<()> {
        let txn = Txn::begin(&self.env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        let serialized = bincode::serialize(entry)
            .map_err(|e| AofError::Serialization(format!("Failed to serialize entry: {}", e)))?;

        mdbx::put(
            &txn,
            self.id_db,
            &entry.base_id.to_le_bytes(),
            &serialized,
            PutFlags::empty(),
        )
        .map_err(|e| AofError::IndexError(format!("Failed to put entry: {}", e)))?;

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit: {}", e)))?;

        Ok(())
    }

    /// Update a segment entry in the index
    pub fn update_segment_entry(&self, entry: &SegmentEntry) -> AofResult<()> {
        self.add_segment_entry(entry) // Same operation as add
    }

    /// Get total segment count
    pub fn get_segment_count(&self) -> AofResult<u64> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let stat = self
            .id_db
            .stat(&txn)
            .map_err(|e| AofError::IndexError(format!("Failed to get database stats: {}", e)))?;

        Ok(stat.ms_entries)
    }

    /// Get segments for archiving (oldest first, up to count)
    pub fn get_segments_for_archiving(&self, count: u64) -> AofResult<Vec<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let mut results = Vec::new();
        let mut cursor = Cursor::open(&txn, self.id_db)
            .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

        if let Ok(Some((_key, mut value))) = cursor.first() {
            let mut collected = 0;
            loop {
                if collected >= count {
                    break;
                }

                let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                    AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                })?;

                // Only include finalized segments
                if entry.finalized_at.is_some() {
                    results.push(entry);
                    collected += 1;
                }

                match cursor.next() {
                    Ok(Some((_next_key, next_value))) => {
                        value = next_value;
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }

        // Sort by creation time (oldest first)
        results.sort_by_key(|entry| entry.created_at);
        Ok(results)
    }
}

// Note: MDBX environment is automatically closed when dropped

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aof::record::{SegmentMetadata, current_timestamp};
    use crate::aof::segment_store::SegmentStatus;
    use tempfile::tempdir;

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
    fn test_mdbx_index_creation_and_open() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");

        // Test creation
        let index = MdbxSegmentIndex::open(&index_path).unwrap();
        assert_eq!(index.version, 1);
        drop(index);

        // Test reopening existing index
        let index = MdbxSegmentIndex::open(&index_path).unwrap();
        assert_eq!(index.version, 1);
    }

    #[test]
    fn test_mdbx_add_and_find_single_entry() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

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
    fn test_mdbx_multiple_entries_ordered() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

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
    fn test_mdbx_multiple_entries_unordered() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

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
    fn test_mdbx_remove_entries() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

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
    fn test_mdbx_timestamp_queries() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

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

        // Test finding by exact timestamp - since overlaps_timestamp returns true for timestamp >= created_at,
        // querying for 1500 will match segments with created_at <= 1500 (i.e., segments 1 and 51)
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
    fn test_mdbx_get_segments_before() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

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
    fn test_mdbx_update_entry() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

        let mut entry = create_test_entry(1, 50, 1000);
        index.add_entry(entry.clone()).unwrap();

        // Update the entry
        entry.finalize(1500);
        entry.archive(
            "segment_1_archived.zst".to_string(),
            2000,
            Some(512),
            Some(0x87654321),
        );

        index.update_entry(entry.clone()).unwrap();

        // Verify the update
        let found = index.find_segment_for_id(25).unwrap().unwrap();
        assert_eq!(found.status, SegmentStatus::ArchivedRemote);
        assert_eq!(found.finalized_at, Some(1500));
        assert_eq!(found.archived_at, Some(2000));
        assert_eq!(found.compressed_size, Some(512));
    }

    #[test]
    fn test_mdbx_concurrent_access() {
        // Note: MDBX doesn't support multiple concurrent writers to the same database file
        // This test verifies that we can reopen the same database sequentially
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");

        // Setup index with test data
        {
            let mut index = MdbxSegmentIndex::open(&index_path).unwrap();
            let entries = vec![
                create_test_entry(1, 50, 1000),
                create_test_entry(51, 100, 1500),
                create_test_entry(101, 150, 2000),
            ];
            for entry in entries {
                index.add_entry(entry).unwrap();
            }
        } // Close first index

        // Open and use first reader
        let found1 = {
            let index1 = MdbxSegmentIndex::open(&index_path).unwrap();
            let found = index1.find_segment_for_id(25).unwrap().unwrap();
            assert_eq!(index1.len().unwrap(), 3);
            found
        }; // Close first reader

        // Open and use second reader
        let found2 = {
            let index2 = MdbxSegmentIndex::open(&index_path).unwrap();
            let found = index2.find_segment_for_id(75).unwrap().unwrap();
            assert_eq!(index2.len().unwrap(), 3);
            found
        }; // Close second reader

        assert_eq!(found1.base_id, 1);
        assert_eq!(found2.base_id, 51);
    }

    #[test]
    fn test_mdbx_edge_cases() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

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
    fn test_mdbx_version_tracking() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

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
    fn test_mdbx_persistence() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");

        // Create index and add data
        {
            let mut index = MdbxSegmentIndex::open(&index_path).unwrap();
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
            let index = MdbxSegmentIndex::open(&index_path).unwrap();
            assert_eq!(index.len().unwrap(), 2);

            let found = index.find_segment_for_id(25).unwrap().unwrap();
            assert_eq!(found.base_id, 1);

            let found = index.find_segment_for_id(75).unwrap().unwrap();
            assert_eq!(found.base_id, 51);
        }
    }
}
