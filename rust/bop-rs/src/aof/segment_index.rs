//! MDBX-based persistent segment index for archive storage

// serde derive not required in this file currently
use std::mem::transmute;
use std::path::Path;

use crate::aof::error::{AofError, AofResult};
use crate::aof::segment_store::{SegmentEntry, SegmentStatus};
use crate::mdbx::{self, CopyFlags, Cursor, DbFlags, Dbi, Env, EnvFlags, PutFlags, Txn, TxnFlags};
use bop_sys as mdbx_sys;

/// MDBX-backed persistent segment index
pub struct MdbxSegmentIndex {
    env: Env,
    /// Database for ID-based lookups: base_id -> SegmentEntry
    id_db: Dbi,
    /// Database for timestamp-based lookups: created_at -> base_id
    time_db: Dbi,
    /// Database for status lookups: status code -> base_id (dup sort)
    status_db: Dbi,
    /// Index metadata
    version: u32,
}

#[inline]
fn encode_u64(value: u64) -> [u8; 8] {
    unsafe { transmute::<u64, [u8; 8]>(value) }
}

#[inline]
fn decode_u64(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[..8]);
    unsafe { transmute::<[u8; 8], u64>(buf) }
}

#[inline]
fn encode_status_key(status: SegmentStatus) -> [u8; 4] {
    let code = status_to_code(&status);
    unsafe { transmute::<u32, [u8; 4]>(code) }
}

#[allow(dead_code)]
#[inline]
fn decode_status_key(bytes: &[u8]) -> SegmentStatus {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes[..4]);
    let code = unsafe { transmute::<[u8; 4], u32>(buf) };
    code_to_status(code)
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
        let flags = EnvFlags::COALESCE | EnvFlags::LIFORECLAIM | EnvFlags::SYNC_DURABLE;
        env.open(path, flags, 0o644)
            .map_err(|e| AofError::IndexError(format!("Failed to open MDBX env: {}", e)))?;

        // Open databases in a transaction
        let txn = Txn::begin(&env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        let id_db = Dbi::open(
            &txn,
            Some("segments_by_id"),
            DbFlags::CREATE | DbFlags::INTEGERKEY,
        )
        .map_err(|e| AofError::IndexError(format!("Failed to open id_db: {}", e)))?;

        let time_db = Dbi::open(
            &txn,
            Some("segments_by_time"),
            DbFlags::CREATE | DbFlags::INTEGERKEY,
        )
        .map_err(|e| AofError::IndexError(format!("Failed to open time_db: {}", e)))?;

        let status_db = Dbi::open(
            &txn,
            Some("segments_by_status"),
            DbFlags::CREATE | DbFlags::DUPSORT | DbFlags::DUPFIXED,
        )
        .map_err(|e| AofError::IndexError(format!("Failed to open status_db: {}", e)))?;

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit transaction: {}", e)))?;

        Ok(Self {
            env,
            id_db,
            time_db,
            status_db,
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
            uncompressed_checksum: metadata.checksum,
            compressed_checksum: None,
            original_size: metadata.size,
            compressed_size: None,
            compression_ratio: None,
            status: SegmentStatus::Active,
            local_path: Some(format!("{}.log", metadata.base_id)),
            archive_key: None,
        };
        self.add_entry(entry)
    }

    /// Add an archive entry to the index
    pub fn add_entry(&mut self, entry: SegmentEntry) -> AofResult<()> {
        self.store_entry(&entry, true)?;
        self.version += 1;
        Ok(())
    }

    fn store_entry(&self, entry: &SegmentEntry, treat_as_new: bool) -> AofResult<()> {
        let txn = Txn::begin(&self.env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        let existing_entry = if treat_as_new {
            None
        } else {
            self.fetch_entry(&txn, entry.base_id)?
        };

        if let Some(previous) = existing_entry {
            // Remove previous time mapping if timestamp changed
            if previous.created_at != entry.created_at {
                let old_time_key = encode_u64(previous.created_at);
                let base_bytes = encode_u64(previous.base_id);
                self.delete_optional(&txn, self.time_db, &old_time_key, Some(&base_bytes))?;
            }

            // Remove previous status mapping (always remove to avoid duplicates)
            let prev_status = previous.status;
            let prev_base_id = previous.base_id;
            self.remove_status_mapping(&txn, prev_status, prev_base_id)?;
        }

        // Serialize entry for primary DB
        let entry_bytes = bincode::serialize(entry)
            .map_err(|e| AofError::Serialization(format!("Failed to serialize entry: {}", e)))?;

        let id_key = encode_u64(entry.base_id);
        mdbx::put(&txn, self.id_db, &id_key, &entry_bytes, PutFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to put in id_db: {}", e)))?;

        let time_key = encode_u64(entry.created_at);
        let id_value = encode_u64(entry.base_id);
        mdbx::put(&txn, self.time_db, &time_key, &id_value, PutFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to put in time_db: {}", e)))?;

        self.put_status_mapping(&txn, entry.status.clone(), entry.base_id)?;

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    fn fetch_entry(&self, txn: &Txn<'_>, base_id: u64) -> AofResult<Option<SegmentEntry>> {
        let key = encode_u64(base_id);
        let value = mdbx::get(txn, self.id_db, &key)
            .map_err(|e| AofError::IndexError(format!("Failed to read entry: {}", e)))?;
        if let Some(bytes) = value {
            let entry: SegmentEntry = bincode::deserialize(bytes).map_err(|e| {
                AofError::Serialization(format!("Failed to deserialize entry: {}", e))
            })?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    fn delete_optional(
        &self,
        txn: &Txn<'_>,
        dbi: Dbi,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> AofResult<()> {
        match mdbx::del(txn, dbi, key, value) {
            Ok(()) => Ok(()),
            Err(err) if err.code() == mdbx_sys::MDBX_error_MDBX_NOTFOUND => Ok(()),
            Err(err) => Err(AofError::IndexError(format!("Failed to delete entry: {}", err))),
        }
    }

    fn put_status_mapping(
        &self,
        txn: &Txn<'_>,
        status: SegmentStatus,
        base_id: u64,
    ) -> AofResult<()> {
        let key = encode_status_key(status);
        let value = encode_u64(base_id);
        mdbx::put(txn, self.status_db, &key, &value, PutFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to update status index: {}", e)))
    }

    fn remove_status_mapping(
        &self,
        txn: &Txn<'_>,
        status: SegmentStatus,
        base_id: u64,
    ) -> AofResult<()> {
        let key = encode_status_key(status);
        let value = encode_u64(base_id);
        self.delete_optional(txn, self.status_db, &key, Some(&value))
    }

    /// Remove a segment from the index
    pub fn remove_entry(&mut self, base_id: u64) -> AofResult<Option<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        // First, get the entry to find its timestamp
        let id_key = encode_u64(base_id);
        let entry = if let Some(entry_bytes) = mdbx::get(&txn, self.id_db, &id_key)
            .map_err(|e| AofError::IndexError(format!("Failed to get from id_db: {}", e)))?
        {
            let entry: SegmentEntry = bincode::deserialize(entry_bytes).map_err(|e| {
                AofError::Serialization(format!("Failed to deserialize entry: {}", e))
            })?;

            // Remove from time database
            let time_key = encode_u64(entry.created_at);
            let base_bytes = encode_u64(entry.base_id);
            self.delete_optional(&txn, self.time_db, &time_key, Some(&base_bytes))?;

            // Remove from ID database
            mdbx::del(&txn, self.id_db, &id_key, None)
                .map_err(|e| AofError::IndexError(format!("Failed to delete from id_db: {}", e)))?;

            // Remove status mapping
            let status = entry.status.clone();
            let base_id_copy = entry.base_id;
            self.remove_status_mapping(&txn, status, base_id_copy)?;

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

            let target_key = encode_u64(record_id);

            // Position cursor at the target key or the next smaller key
            match cursor.set_range(&target_key) {
                Ok(Some((key, value))) => {
                    // Check if this exact key works
                    let key_id = decode_u64(key);
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
                                let key_id = decode_u64(key);
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
                            let key_id = decode_u64(key);
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
            if let Ok(Some((mut _key, mut id_bytes))) = cursor.first() {
                loop {
                    let base_id = decode_u64(id_bytes);

                    // Get the full entry from ID database
                    let id_key = encode_u64(base_id);
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
                            _key = next_key;
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

            // Iterate from first entry until we reach or pass min_base_id
            if let Ok(Some((mut _key, mut value))) = cursor.first() {
                loop {
                    let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                        AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                    })?;

                    // Stop when we reach a segment with base_id >= min_base_id
                    if entry.base_id >= min_base_id {
                        break;
                    }

                    results.push(entry);

                    match cursor.next() {
                        Ok(Some((next_key, next_value))) => {
                            _key = next_key;
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

            if let Ok(Some((mut _key, mut value))) = cursor.first() {
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
                               _key = next_key;
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

                if entry.status == SegmentStatus::Active {
                    active_segment = Some(entry);
                } else {
                    // Continue searching for active segment
                    while let Ok(Some((_, next_value))) = cursor.next() {
                        value = next_value;
                        let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                            AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                        })?;

                        if entry.status == SegmentStatus::Active {
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

            if let Ok(Some((mut _key, mut value))) = cursor.first() {
                loop {
                    let entry: SegmentEntry = bincode::deserialize(value).map_err(|e| {
                        AofError::Serialization(format!("Failed to deserialize entry: {}", e))
                    })?;
                    results.push(entry);

                    match cursor.next() {
                        Ok(Some((next_key, next_value))) => {
                            _key = next_key;
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

        let id_key = encode_u64(base_id);
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

        let start_key = encode_u64(base_id + 1);
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

    /// Retrieve all segments for a given status using the dedicated status index.
    pub fn get_segments_by_status(&self, status: SegmentStatus) -> AofResult<Vec<SegmentEntry>> {
        let txn = Txn::begin(&self.env, None, TxnFlags::RDONLY).map_err(|e| {
            AofError::IndexError(format!("Failed to begin read transaction: {}", e))
        })?;

        let mut results = Vec::new();
        {
            let mut cursor = Cursor::open(&txn, self.status_db)
                .map_err(|e| AofError::IndexError(format!("Failed to open status cursor: {}", e)))?;

            if let Some((_key, mut value)) = cursor
                .seek_key(&encode_status_key(status))
                .map_err(|e| AofError::IndexError(format!("Failed to seek status cursor: {}", e)))?
            {
                loop {
                    let base_id = decode_u64(value);
                    if let Some(entry) = self.fetch_entry(&txn, base_id)? {
                        results.push(entry);
                    }

                    match cursor.next_dup() {
                        Ok(Some((_next_key, next_value))) => {
                            value = next_value;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            return Err(AofError::IndexError(format!(
                                "Failed to advance status cursor: {}",
                                e
                            )));
                        }
                    }
                }
            }
        }

        txn.abort()
            .map_err(|e| AofError::IndexError(format!("Failed to abort transaction: {}", e)))?;

        Ok(results)
    }

    /// Update segment status (finalized flag)
    pub fn update_segment_status(&mut self, base_id: u64, finalized: bool) -> AofResult<()> {
        if let Some(mut entry) = self.get_segment(base_id)? {
            if finalized {
                let now = crate::aof::record::current_timestamp();
                entry.finalize(now);
            } else {
                entry.finalized_at = None;
                entry.status = SegmentStatus::Active;
            }
            self.update_entry(entry)?;
        }
        Ok(())
    }

    /// Update an existing segment entry
    pub fn update_entry(&mut self, entry: SegmentEntry) -> AofResult<()> {
        self.store_entry(&entry, false)?;
        self.version += 1;
        Ok(())
    }

    /// Update segment tail (last_id and size)
    pub fn update_segment_tail(&mut self, base_id: u64, last_id: u64, size: u64) -> AofResult<()> {
        println!(
            "DEBUG: update_segment_tail - base_id={}, last_id={}, size={}",
            base_id, last_id, size
        );
        if let Some(mut entry) = self.get_segment(base_id)? {
            println!(
                "DEBUG: Before update - last_id={}, record_count={}, size={}",
                entry.last_id, entry.record_count, entry.original_size
            );

            entry.last_id = last_id;
            entry.original_size = size;
            entry.record_count = if last_id >= entry.base_id {
                last_id - entry.base_id + 1
            } else {
                0
            };

            println!(
                "DEBUG: After update - last_id={}, record_count={}, size={}",
                entry.last_id, entry.record_count, entry.original_size
            );

            self.update_entry(entry)?;

            // Force sync to disk to ensure data is persisted
            self.env
                .sync(true, false)
                .map_err(|e| AofError::IndexError(format!("Failed to sync database: {}", e)))?;

            println!("DEBUG: Transaction committed and synced successfully");
        } else {
            println!("DEBUG: No entry found for base_id={}", base_id);
        }

        Ok(())
    }

    /// Add a segment entry to the index
    pub fn add_segment_entry(&self, entry: &SegmentEntry) -> AofResult<()> {
        self.store_entry(entry, true)
    }

    /// Update a segment entry in the index
    pub fn update_segment_entry(&self, entry: &SegmentEntry) -> AofResult<()> {
        self.store_entry(entry, false)
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
        let mut segments = self.get_segments_by_status(SegmentStatus::Finalized)?;
        segments.sort_by_key(|entry| entry.created_at);
        if segments.len() as u64 > count {
            segments.truncate(count as usize);
        }
        Ok(segments)
    }

    /// Create a consistent backup of the index file at the specified path.
    pub fn backup_to_path<P: AsRef<Path>>(&self, dest: P, flags: CopyFlags) -> AofResult<()> {
        self.env
            .copy_to_path(dest, flags)
            .map_err(|e| AofError::IndexError(format!("Failed to backup index: {}", e)))
    }

    /// Copy the index to an existing file descriptor (useful for streaming backups).
    pub fn backup_to_fd(&self, fd: mdbx_sys::mdbx_filehandle_t, flags: CopyFlags) -> AofResult<()> {
        self.env
            .copy_to_fd(fd, flags)
            .map_err(|e| AofError::IndexError(format!("Failed to backup index fd: {}", e)))
    }

    /// Export the complete segment index state as a serialized snapshot for RAFT replication.
    pub fn export_snapshot(&self) -> AofResult<Vec<u8>> {
        let segments = self.get_all_segments()?;
        bincode::serialize(&segments)
            .map_err(|e| AofError::Serialization(format!("Failed to serialize snapshot: {}", e)))
    }

    /// Import a serialized snapshot, replacing the current index contents.
    pub fn import_snapshot(&mut self, snapshot: &[u8]) -> AofResult<()> {
        let entries: Vec<SegmentEntry> = bincode::deserialize(snapshot).map_err(|e| {
            AofError::Serialization(format!("Failed to deserialize snapshot: {}", e))
        })?;

        self.clear_database(self.id_db, false)?;
        self.clear_database(self.time_db, false)?;
        self.clear_database(self.status_db, true)?;

        self.version = 1;
        for entry in entries {
            self.store_entry(&entry, true)?;
            self.version += 1;
        }

        self.env
            .sync(true, false)
            .map_err(|e| AofError::IndexError(format!("Failed to sync after snapshot import: {}", e)))?;
        Ok(())
    }

    fn clear_database(&self, dbi: Dbi, remove_all_dups: bool) -> AofResult<()> {
        let txn = Txn::begin(&self.env, None, TxnFlags::empty())
            .map_err(|e| AofError::IndexError(format!("Failed to begin transaction: {}", e)))?;

        {
            let mut cursor = Cursor::open(&txn, dbi)
                .map_err(|e| AofError::IndexError(format!("Failed to open cursor: {}", e)))?;

            loop {
                match cursor.first() {
                    Ok(Some(_)) => {
                        cursor
                            .del_current(remove_all_dups)
                            .map_err(|e| {
                                AofError::IndexError(format!(
                                    "Failed to clear database: {}",
                                    e
                                ))
                            })?;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(AofError::IndexError(format!(
                            "Failed to iterate database during clear: {}",
                            e
                        )));
                    }
                }
            }
        }

        txn.commit()
            .map_err(|e| AofError::IndexError(format!("Failed to commit database clear: {}", e)))
    }
}

// Note: MDBX environment is automatically closed when dropped

fn status_to_code(status: &SegmentStatus) -> u32 {
    match status {
        SegmentStatus::Active => 0,
        SegmentStatus::Finalized => 1,
        SegmentStatus::ArchivedCached => 2,
        SegmentStatus::ArchivedRemote => 3,
    }
}

fn code_to_status(code: u32) -> SegmentStatus {
    match code {
        0 => SegmentStatus::Active,
        1 => SegmentStatus::Finalized,
        2 => SegmentStatus::ArchivedCached,
        3 => SegmentStatus::ArchivedRemote,
        _ => SegmentStatus::Active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aof::record::SegmentMetadata;
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
    fn test_update_segment_entry_refreshes_timestamp_index() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("test_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

        let mut entry = create_test_entry(10, 20, 1_000);
        index.add_entry(entry.clone()).unwrap();

        let initial = index.find_segments_for_timestamp(1_500).unwrap();
        assert_eq!(initial.len(), 1);

        entry.created_at = 4_000;
        index.update_segment_entry(&entry).unwrap();

        let old_results = index.find_segments_for_timestamp(2_000).unwrap();
        assert!(
            old_results.is_empty(),
            "timestamp index should reflect updated created_at"
        );

        let new_results = index.find_segments_for_timestamp(4_500).unwrap();
        assert_eq!(new_results.len(), 1);
        assert_eq!(new_results[0].base_id, entry.base_id);
        assert_eq!(new_results[0].created_at, 4_000);
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
    fn test_mdbx_status_index_lookup() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("status_index");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

        let active_entry = create_test_entry(1, 50, 1000);
        let mut finalized_entry = create_test_entry(51, 100, 1500);
        finalized_entry.finalize(2000);

        let mut archived_entry = create_test_entry(101, 150, 2500);
        archived_entry.finalize(2800);
        archived_entry.archive(
            "segment_101.zst".to_string(),
            3000,
            Some(512),
            Some(0xAA55AA55),
        );

        index.add_entry(active_entry.clone()).unwrap();
        index.add_entry(finalized_entry.clone()).unwrap();
        index.add_entry(archived_entry.clone()).unwrap();

        let active_segments = index.get_segments_by_status(SegmentStatus::Active).unwrap();
        assert_eq!(active_segments.len(), 1);
        assert_eq!(active_segments[0].base_id, active_entry.base_id);

        let finalized_segments = index.get_segments_by_status(SegmentStatus::Finalized).unwrap();
        assert_eq!(finalized_segments.len(), 1);
        assert_eq!(finalized_segments[0].base_id, finalized_entry.base_id);

        let archived_segments = index
            .get_segments_by_status(SegmentStatus::ArchivedRemote)
            .unwrap();
        assert_eq!(archived_segments.len(), 1);
        assert_eq!(archived_segments[0].base_id, archived_entry.base_id);
    }

    #[test]
    fn test_mdbx_snapshot_roundtrip() {
        let temp_dir = tempdir().unwrap();
        let index_path = temp_dir.path().join("primary.db");
        let mut index = MdbxSegmentIndex::open(&index_path).unwrap();

        let entries = vec![
            create_test_entry(1, 50, 1000),
            create_test_entry(51, 100, 1500),
        ];

        for entry in &entries {
            index.add_entry(entry.clone()).unwrap();
        }

        let snapshot = index.export_snapshot().unwrap();

        let replica_path = temp_dir.path().join("replica.db");
        let mut replica = MdbxSegmentIndex::open(&replica_path).unwrap();
        replica.import_snapshot(&snapshot).unwrap();

        assert_eq!(replica.len().unwrap(), entries.len());
        let restored = replica.get_all_segments().unwrap();
        assert_eq!(restored.len(), entries.len());
        assert_eq!(restored[0].base_id, 1);
        assert_eq!(restored[1].base_id, 51);
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
