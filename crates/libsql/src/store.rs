use crate::schema::*;
use anyhow::Result;
use libmdbx::{Database, DatabaseOptions, NoWriteMap, Table, TableFlags, WriteFlags};
// rkyv removed
use std::borrow::Cow;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
struct TableHandles {
    sys_meta: u32,
    databases: u32,
    branches: u32,
    segment_versions: u32,
    chunk_ref_counts: u32,
    chunks: u32,
    wal_map: u32,
    checkpoints: u32,
    raft_log: u32,
    raft_segments: u32,
    raft_state: u32,
    evolutions: u32,
}

// StoreCache removed

// Format: [ Metadata (32 bits) | ID (32 bits) ]
// Metadata layout:
//   - Page Shift KB (8 bits): log2(page_size / 1024)
//   - PPC Shift (8 bits): log2(pages_per_chunk)
//   - Reserved (16 bits)
//
// Page shift max 15 -> 32MB page size (1024 * 2^15)
// PPC shift max 15 -> 32768 pages per chunk

/// Encodes database configuration into the database ID.
/// Returns encoded u64: Top 32 bits metadata, Bottom 32 bits ID
fn encode_db_id(id: u32, page_size: u32, chunk_size: u32) -> Result<u64> {
    if !page_size.is_power_of_two() || page_size < 1024 {
        anyhow::bail!("Page size must be power of two and at least 1KB");
    }
    let pages_per_chunk = chunk_size / page_size;
    if !pages_per_chunk.is_power_of_two() || pages_per_chunk == 0 {
        anyhow::bail!("Chunk size / Page size must be power of two and > 0");
    }

    let page_shift_kb = (page_size / 1024).trailing_zeros();
    let ppc_shift = pages_per_chunk.trailing_zeros();

    // Check limits (8 bits each)
    if page_shift_kb > 255 {
        anyhow::bail!("Page size too large");
    }
    if ppc_shift > 255 {
        anyhow::bail!("Pages per chunk too large");
    }

    // Metadata in top 32 bits: [page_shift(8) | ppc_shift(8) | reserved(16)]
    let meta = ((page_shift_kb as u64) << 56) | ((ppc_shift as u64) << 48);
    Ok(meta | (id as u64))
}

/// Decodes a database ID into its components.
/// Returns (raw_id, page_size, ppc_shift)
fn decode_db_id(encoded: u64) -> (u32, u32, u32) {
    let page_shift_kb = (encoded >> 56) & 0xFF;
    let ppc_shift = (encoded >> 48) & 0xFF;
    let raw_id = (encoded & 0xFFFFFFFF) as u32;

    let page_size = 1024 * (1 << page_shift_kb);
    (raw_id, page_size as u32, ppc_shift as u32)
}

// Helpers removed

#[derive(Clone)]
pub struct Store {
    // Root DB / Environment
    root_db: Arc<Database<NoWriteMap>>,
    handles: Arc<TableHandles>,
}

impl Store {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        // Ensure directory exists
        std::fs::create_dir_all(&path)?;

        // Open the root database (Environment implicitly)
        let mut opts = DatabaseOptions::default();
        opts.max_tables = Some(50);
        let root_db = Database::<NoWriteMap>::open_with_options(path.as_ref(), opts)?;

        // Initialize tables and cache handles
        let txn = root_db.begin_rw_txn()?;

        let sys_meta = txn.create_table(Some(SYS_META_TABLE), TableFlags::default())?;
        let databases = txn.create_table(Some(DB_INFO_TABLE), TableFlags::default())?;
        let branches = txn.create_table(Some(BRANCHES_TABLE), TableFlags::default())?;
        let segment_versions =
            txn.create_table(Some(SEGMENT_VERSIONS_TABLE), TableFlags::default())?;
        let chunk_ref_counts = txn
            .create_table(Some(CHUNK_REF_COUNTS_TABLE), TableFlags::default())?
            .dbi();
        let chunks = txn
            .create_table(Some(CHUNKS_TABLE), TableFlags::default())?
            .dbi();
        let wal_map = txn.create_table(Some(WAL_MAP_TABLE), TableFlags::default())?;
        let checkpoints = txn.create_table(Some(CHECKPOINTS_TABLE), TableFlags::default())?;
        let raft_log = txn.create_table(Some(RAFT_LOG_TABLE), TableFlags::default())?;
        let raft_segments = txn.create_table(Some(RAFT_SEGMENTS_TABLE), TableFlags::default())?;
        let raft_state = txn.create_table(Some(RAFT_STATE_TABLE), TableFlags::default())?;
        let evolutions = txn.create_table(Some(EVOLUTIONS_TABLE), TableFlags::default())?;

        let handles = TableHandles {
            sys_meta: sys_meta.dbi(),
            databases: databases.dbi(),
            branches: branches.dbi(),
            segment_versions: segment_versions.dbi(),
            chunk_ref_counts,
            chunks,
            wal_map: wal_map.dbi(),
            checkpoints: checkpoints.dbi(),
            raft_log: raft_log.dbi(),
            raft_segments: raft_segments.dbi(),
            raft_state: raft_state.dbi(),
            evolutions: evolutions.dbi(),
        };

        // Cache initialization removed

        txn.commit()?;

        Ok(Store {
            root_db: Arc::new(root_db),
            handles: Arc::new(handles),
            // cache removed
        })
    }

    /// Helper to reconstruct a Table from a raw DBI handle.
    /// SAFETY: The DBI handle must be valid and belong to this environment.
    /// The Table lifetime is bound to the transaction.
    unsafe fn table<'txn>(
        &self,
        _txn: &libmdbx::Transaction<'txn, libmdbx::RO, NoWriteMap>,
        dbi: u32,
    ) -> Table<'txn> {
        // libmdbx-rs 0.6 uses a newtype wrapper around MDBX_dbi with a PhantomData marker.
        // We can reconstruct it using transmute or by reproducing the struct layout if it's visible.
        // Looking at libmdbx source, Table has field `dbi: ffi::MDBX_dbi` and `_marker`.
        // We can transmute.

        #[repr(C)]
        struct RawTable<'txn> {
            dbi: u32,
            _marker: PhantomData<&'txn ()>,
        }

        let raw = RawTable {
            dbi,
            _marker: PhantomData,
        };
        unsafe { std::mem::transmute(raw) }
    }

    // Unsafe rw version
    unsafe fn table_rw<'txn>(
        &self,
        _txn: &libmdbx::Transaction<'txn, libmdbx::RW, NoWriteMap>,
        dbi: u32,
    ) -> Table<'txn> {
        #[repr(C)]
        struct RawTable<'txn> {
            dbi: u32,
            _marker: PhantomData<&'txn ()>,
        }

        let raw = RawTable {
            dbi,
            _marker: PhantomData,
        };
        unsafe { std::mem::transmute(raw) }
    }

    pub fn create_database(&self, name: String, page_size: u32, chunk_size: u32) -> Result<u64> {
        let txn = self.root_db.begin_rw_txn()?;
        let sys_meta = unsafe { self.table_rw(&txn, self.handles.sys_meta) };
        let databases = unsafe { self.table_rw(&txn, self.handles.databases) };

        // Generate ID (u32 PK)
        let id_val: Option<Cow<[u8]>> = txn.get(&sys_meta, "next_db_id".as_bytes())?;
        let id_val = id_val.unwrap_or_else(|| Cow::Borrowed(&[0; 8]));

        let id = u64::from_be_bytes(id_val.as_ref().try_into().unwrap_or([0; 8])) as u32 + 1;
        txn.put(
            &sys_meta,
            "next_db_id".as_bytes(),
            &(id as u64).to_be_bytes(),
            WriteFlags::empty(),
        )?;

        let db_id = encode_db_id(id, page_size, chunk_size)?;

        let db_info = DatabaseInfo {
            name,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
            page_size,
            chunk_size,
        };

        let db_info_bytes = db_info.to_bytes();

        // Key by the FULL encoded ID (so we can iterate if needed, but primarily for lookup)
        txn.put(
            &databases,
            &db_id.to_be_bytes(),
            &db_info_bytes,
            WriteFlags::empty(),
        )?;
        txn.commit()?;

        Ok(db_id)
    }

    pub fn create_branch(
        &self,
        db_id: u64,
        name: String,
        parent_branch_id: Option<u32>,
    ) -> Result<u32> {
        let txn = self.root_db.begin_rw_txn()?;
        let sys_meta = unsafe { self.table_rw(&txn, self.handles.sys_meta) };
        let branches = unsafe { self.table_rw(&txn, self.handles.branches) };

        let id_val: Option<Cow<[u8]>> = txn.get(&sys_meta, "next_branch_id".as_bytes())?;
        let id_val = id_val.unwrap_or_else(|| Cow::Borrowed(&[0; 8]));

        let branch_id =
            (u64::from_be_bytes(id_val.as_ref().try_into().unwrap_or([0; 8])) + 1) as u32;
        txn.put(
            &sys_meta,
            "next_branch_id".as_bytes(),
            &(branch_id as u64).to_be_bytes(),
            WriteFlags::empty(),
        )?;

        let fork_at = if let Some(pid) = parent_branch_id {
            // Look up parent branch info to get CURRENT head version
            let parent_key = BranchKey {
                database_id: db_id,
                branch_id: pid,
            };
            let parent_bytes: Option<Cow<[u8]>> = txn.get(&branches, &parent_key.to_bytes())?;
            let parent_bytes =
                parent_bytes.ok_or_else(|| anyhow::anyhow!("Parent branch {} not found", pid))?;

            let parent_info = BranchInfo::from_bytes(&parent_bytes)
                .map_err(|e| anyhow::anyhow!("Validation error: {}", e))?;

            if parent_info.database_id != db_id {
                anyhow::bail!("Parent branch belongs to different database");
            }
            parent_info.current_head_version
        } else {
            0
        };

        let branch_info = BranchInfo {
            database_id: db_id,
            name,
            parent_branch_id,
            fork_at_version: fork_at,
            current_head_version: fork_at,
        };

        let branch_info_bytes = branch_info.to_bytes();
        let key = BranchKey {
            database_id: db_id,
            branch_id,
        };
        txn.put(
            &branches,
            &key.to_bytes(),
            &branch_info_bytes,
            WriteFlags::empty(),
        )?;
        txn.commit()?;

        Ok(branch_id)
    }

    pub fn commit_segment(
        &self,
        db_id: u64,
        branch_id: u32,
        segment_id: u32,
        chunk: Vec<u8>,
        new_version: u64,
    ) -> Result<()> {
        let txn = self.root_db.begin_rw_txn()?;
        let branches = unsafe { self.table_rw(&txn, self.handles.branches) };
        let segment_versions = unsafe { self.table_rw(&txn, self.handles.segment_versions) };
        let chunk_ref_counts = unsafe { self.table_rw(&txn, self.handles.chunk_ref_counts) };
        let chunks_table = unsafe { self.table_rw(&txn, self.handles.chunks) };

        let key = BranchKey {
            database_id: db_id,
            branch_id,
        };
        let branch_bytes: Option<Cow<[u8]>> = txn.get(&branches, &key.to_bytes())?;
        let branch_bytes =
            branch_bytes.ok_or_else(|| anyhow::anyhow!("Branch {} not found", branch_id))?;
        let mut branch_info = BranchInfo::from_bytes(&branch_bytes)
            .map_err(|e| anyhow::anyhow!("Validation error: {}", e))?;

        if branch_info.database_id != db_id {
            anyhow::bail!("Branch does not belong to database");
        }

        // 1. Calculate CAS Hash
        let crc = crc::Crc::<u64>::new(&crc::CRC_64_XZ);
        let chunk_id = crc.checksum(&chunk);

        let chunk_id_bytes = chunk_id.to_be_bytes();

        // 2. Manage Ref Counts
        let count_bytes: Option<Cow<[u8]>> = txn.get(&chunk_ref_counts, &chunk_id_bytes)?;
        let mut count: u32 = if let Some(bytes) = count_bytes {
            u32::from_be_bytes(bytes.as_ref().try_into()?)
        } else {
            0
        };

        // If this is a new chunk, store its data
        if count == 0 {
            txn.put(&chunks_table, &chunk_id_bytes, &chunk, WriteFlags::empty())?;
        }

        count += 1;
        txn.put(
            &chunk_ref_counts,
            &chunk_id_bytes,
            &count.to_be_bytes(),
            WriteFlags::empty(),
        )?;

        // 3. Write Key to segment_versions
        let key = SegmentVersionKey {
            database_id: db_id,
            branch_id,
            segment_id,
            version_inverted: u64::MAX - new_version,
        };
        // Value is just the ChunkId (u64)
        txn.put(
            &segment_versions,
            &key.to_bytes(),
            &chunk_id_bytes,
            WriteFlags::empty(),
        )?;

        // Update branch head version
        branch_info.current_head_version = new_version;
        let branch_val = branch_info.to_bytes();
        let key = BranchKey {
            database_id: db_id,
            branch_id,
        };
        txn.put(&branches, &key.to_bytes(), &branch_val, WriteFlags::empty())?;

        // Note: In a real system we would verify previous version usage and DecRef if overwriting,
        // but here we just append to history (MVCC). Gc handles cleanup.

        txn.commit()?;
        Ok(())
    }

    pub fn find_chunk_for_page(
        &self,
        db_id: u64,
        branch_id: u32,
        page_id: u64,
    ) -> Result<Option<FoundChunk>> {
        let txn = self.root_db.begin_ro_txn()?;
        let branches = unsafe { self.table(&txn, self.handles.branches) };
        let segment_versions = unsafe { self.table(&txn, self.handles.segment_versions) };

        // 1. Get DB Info (Decoded from ID)
        let (_raw_id, _page_size, ppc_shift) = decode_db_id(db_id);

        // Use shift for division by pages_per_chunk
        let segment_id = (page_id >> ppc_shift) as u32;

        let mut current_search_branch = branch_id;
        let mut search_limit_version = u64::MAX;

        // Loop up the tree
        loop {
            // Check current layer
            let search_key = SegmentVersionKey {
                database_id: db_id,
                branch_id: current_search_branch,
                segment_id,
                version_inverted: 0,
            };
            let search_bytes = search_key.to_bytes();

            let mut cursor = txn.cursor(&segment_versions)?;
            let mut iter = cursor.iter_from::<Cow<[u8]>, Cow<[u8]>>(&search_bytes);

            if let Some(Ok((k, v))) = iter.next() {
                if let Some(found_key) = SegmentVersionKey::from_bytes(&k) {
                    // Check strict match for DB/Branch/Segment
                    if found_key.database_id == db_id
                        && found_key.branch_id == current_search_branch
                        && found_key.segment_id == segment_id
                        && (u64::MAX - found_key.version_inverted) <= search_limit_version
                    {
                        let chunk_id = u64::from_be_bytes(v.as_ref().try_into()?);
                        return Ok(Some(FoundChunk { chunk_id }));
                    }
                }
            }

            // Not found in this layer, look up parent info
            let branch_key = BranchKey {
                database_id: db_id,
                branch_id: current_search_branch,
            };
            let branch_bytes: Option<Cow<[u8]>> = txn.get(&branches, &branch_key.to_bytes())?;
            if let Some(b_bytes) = branch_bytes {
                let info = BranchInfo::from_bytes(&b_bytes)?;
                if let Some(parent_id) = info.parent_branch_id {
                    current_search_branch = parent_id;
                    search_limit_version = info.fork_at_version;
                    continue;
                }
            }

            // No parent, stop
            break;
        }

        // Not found in any layer
        Ok(None)
    }

    pub fn append_wal_metadata(
        &self,
        db_id: u64,
        branch_id: u32,
        segment: WalSegment,
    ) -> Result<()> {
        let txn = self.root_db.begin_rw_txn()?;
        let wal_map = unsafe { self.table_rw(&txn, self.handles.wal_map) };

        let key = WalMapKey {
            database_id: db_id,
            branch_id,
            start_frame: segment.start_frame,
        };
        let val = segment.to_bytes();

        txn.put(&wal_map, &key.to_bytes(), &val, WriteFlags::empty())?;
        txn.commit()?;
        Ok(())
    }

    pub fn get_wal_segments(
        &self,
        db_id: u64,
        branch_id: u32,
        start_frame: u64,
    ) -> Result<Vec<WalSegment>> {
        let txn = self.root_db.begin_ro_txn()?;
        let wal_map = unsafe { self.table(&txn, self.handles.wal_map) };

        let key = WalMapKey {
            database_id: db_id,
            branch_id,
            start_frame,
        };
        let start_key_bytes = key.to_bytes();

        let mut segments = Vec::new();
        let mut cursor = txn.cursor(&wal_map)?;
        let mut iter = cursor.iter_from::<Cow<[u8]>, Cow<[u8]>>(&start_key_bytes);

        while let Some(Ok((k, v))) = iter.next() {
            // Check if DB ID and Branch ID match.
            // WalMapKey bytes: [db_id (8)] [branch_id (4)] [start_frame (8)]
            if let Some(found_key) = WalMapKey::from_bytes(&k) {
                if found_key.database_id != db_id || found_key.branch_id != branch_id {
                    break;
                }
                let seg = WalSegment::from_bytes(&v)?;
                segments.push(seg);
            } else {
                break;
            }
        }

        Ok(segments)
    }

    pub fn set_checkpoint(&self, db_id: u64, branch_id: u32, info: CheckpointInfo) -> Result<()> {
        let txn = self.root_db.begin_rw_txn()?;
        let checkpoints = unsafe { self.table_rw(&txn, self.handles.checkpoints) };

        // Use BranchKey as the checkpoint key (per-branch checkpoints)
        let key = BranchKey {
            database_id: db_id,
            branch_id,
        };
        txn.put(
            &checkpoints,
            &key.to_bytes(),
            &info.to_bytes(),
            WriteFlags::empty(),
        )?;
        txn.commit()?;
        Ok(())
    }

    pub fn get_checkpoint(&self, db_id: u64, branch_id: u32) -> Result<Option<CheckpointInfo>> {
        let txn = self.root_db.begin_ro_txn()?;
        let checkpoints = unsafe { self.table(&txn, self.handles.checkpoints) };

        let key = BranchKey {
            database_id: db_id,
            branch_id,
        };
        let val: Option<Cow<[u8]>> = txn.get(&checkpoints, &key.to_bytes())?;
        if let Some(bytes) = val {
            Ok(Some(CheckpointInfo::from_bytes(&bytes)?))
        } else {
            Ok(None)
        }
    }

    /// Retrieves the raw chunk data by its CAS ID.
    pub fn get_chunk(&self, chunk_id: ChunkId) -> Result<Option<Vec<u8>>> {
        let txn = self.root_db.begin_ro_txn()?;
        let chunks_table = unsafe { self.table(&txn, self.handles.chunks) };

        let val: Option<Cow<[u8]>> = txn.get(&chunks_table, &chunk_id.to_be_bytes())?;
        Ok(val.map(|v| v.into_owned()))
    }

    /// Retrieves database info by its encoded ID.
    pub fn get_database(&self, db_id: u64) -> Result<Option<DatabaseInfo>> {
        let txn = self.root_db.begin_ro_txn()?;
        let databases = unsafe { self.table(&txn, self.handles.databases) };

        let val: Option<Cow<[u8]>> = txn.get(&databases, &db_id.to_be_bytes())?;
        if let Some(bytes) = val {
            Ok(Some(DatabaseInfo::from_bytes(&bytes)?))
        } else {
            Ok(None)
        }
    }

    /// Retrieves branch info.
    pub fn get_branch(&self, db_id: u64, branch_id: u32) -> Result<Option<BranchInfo>> {
        let txn = self.root_db.begin_ro_txn()?;
        let branches = unsafe { self.table(&txn, self.handles.branches) };

        let key = BranchKey {
            database_id: db_id,
            branch_id,
        };
        let val: Option<Cow<[u8]>> = txn.get(&branches, &key.to_bytes())?;
        if let Some(bytes) = val {
            Ok(Some(BranchInfo::from_bytes(&bytes)?))
        } else {
            Ok(None)
        }
    }

    // ========================================================================
    // Raft Log Operations
    // ========================================================================

    /// Append a raft log entry.
    pub fn append_raft_entry(
        &self,
        db_id: u64,
        branch_id: u32,
        log_index: u64,
        entry: &RaftLogEntry,
    ) -> Result<()> {
        let txn = self.root_db.begin_rw_txn()?;
        let raft_log = unsafe { self.table_rw(&txn, self.handles.raft_log) };

        let key = RaftLogKey {
            database_id: db_id,
            branch_id,
            log_index,
        };
        txn.put(
            &raft_log,
            &key.to_bytes(),
            &entry.to_bytes(),
            WriteFlags::empty(),
        )?;
        txn.commit()?;
        Ok(())
    }

    /// Get a raft log entry by index.
    pub fn get_raft_entry(
        &self,
        db_id: u64,
        branch_id: u32,
        log_index: u64,
    ) -> Result<Option<RaftLogEntry>> {
        let txn = self.root_db.begin_ro_txn()?;
        let raft_log = unsafe { self.table(&txn, self.handles.raft_log) };

        let key = RaftLogKey {
            database_id: db_id,
            branch_id,
            log_index,
        };
        let val: Option<Cow<[u8]>> = txn.get(&raft_log, &key.to_bytes())?;
        if let Some(bytes) = val {
            Ok(Some(RaftLogEntry::from_bytes(&bytes)?))
        } else {
            Ok(None)
        }
    }

    /// Get raft log entries in a range [start_index, end_index].
    pub fn get_raft_entries(
        &self,
        db_id: u64,
        branch_id: u32,
        start_index: u64,
        end_index: u64,
    ) -> Result<Vec<(u64, RaftLogEntry)>> {
        let txn = self.root_db.begin_ro_txn()?;
        let raft_log = unsafe { self.table(&txn, self.handles.raft_log) };

        let start_key = RaftLogKey {
            database_id: db_id,
            branch_id,
            log_index: start_index,
        };

        let mut entries = Vec::new();
        let mut cursor = txn.cursor(&raft_log)?;
        let mut iter = cursor.iter_from::<Cow<[u8]>, Cow<[u8]>>(&start_key.to_bytes());

        while let Some(Ok((k, v))) = iter.next() {
            if let Some(found_key) = RaftLogKey::from_bytes(&k) {
                if found_key.database_id != db_id || found_key.branch_id != branch_id {
                    break;
                }
                if found_key.log_index > end_index {
                    break;
                }
                let entry = RaftLogEntry::from_bytes(&v)?;
                entries.push((found_key.log_index, entry));
            } else {
                break;
            }
        }

        Ok(entries)
    }

    /// Register a raft segment file.
    pub fn add_raft_segment(
        &self,
        db_id: u64,
        branch_id: u32,
        segment: &RaftSegment,
    ) -> Result<()> {
        let txn = self.root_db.begin_rw_txn()?;
        let raft_segments = unsafe { self.table_rw(&txn, self.handles.raft_segments) };

        let key = RaftSegmentKey {
            database_id: db_id,
            branch_id,
            start_index: segment.start_index,
        };
        txn.put(
            &raft_segments,
            &key.to_bytes(),
            &segment.to_bytes(),
            WriteFlags::empty(),
        )?;
        txn.commit()?;
        Ok(())
    }

    /// Get raft segments for a branch.
    pub fn get_raft_segments(&self, db_id: u64, branch_id: u32) -> Result<Vec<RaftSegment>> {
        let txn = self.root_db.begin_ro_txn()?;
        let raft_segments = unsafe { self.table(&txn, self.handles.raft_segments) };

        let start_key = RaftSegmentKey {
            database_id: db_id,
            branch_id,
            start_index: 0,
        };

        let mut segments = Vec::new();
        let mut cursor = txn.cursor(&raft_segments)?;
        let mut iter = cursor.iter_from::<Cow<[u8]>, Cow<[u8]>>(&start_key.to_bytes());

        while let Some(Ok((k, v))) = iter.next() {
            if let Some(found_key) = RaftSegmentKey::from_bytes(&k) {
                if found_key.database_id != db_id || found_key.branch_id != branch_id {
                    break;
                }
                let segment = RaftSegment::from_bytes(&v)?;
                segments.push(segment);
            } else {
                break;
            }
        }

        Ok(segments)
    }

    /// Find the raft segment containing a given log index.
    pub fn find_raft_segment_for_index(
        &self,
        db_id: u64,
        branch_id: u32,
        log_index: u64,
    ) -> Result<Option<RaftSegment>> {
        let segments = self.get_raft_segments(db_id, branch_id)?;
        for segment in segments {
            if log_index >= segment.start_index && log_index <= segment.end_index {
                return Ok(Some(segment));
            }
        }
        Ok(None)
    }

    /// Get raft state for a branch.
    pub fn get_raft_state(&self, db_id: u64, branch_id: u32) -> Result<Option<RaftState>> {
        let txn = self.root_db.begin_ro_txn()?;
        let raft_state = unsafe { self.table(&txn, self.handles.raft_state) };

        let key = BranchKey {
            database_id: db_id,
            branch_id,
        };
        let val: Option<Cow<[u8]>> = txn.get(&raft_state, &key.to_bytes())?;
        if let Some(bytes) = val {
            Ok(Some(RaftState::from_bytes(&bytes)?))
        } else {
            Ok(None)
        }
    }

    /// Set raft state for a branch.
    pub fn set_raft_state(&self, db_id: u64, branch_id: u32, state: &RaftState) -> Result<()> {
        let txn = self.root_db.begin_rw_txn()?;
        let raft_state = unsafe { self.table_rw(&txn, self.handles.raft_state) };

        let key = BranchKey {
            database_id: db_id,
            branch_id,
        };
        txn.put(
            &raft_state,
            &key.to_bytes(),
            &state.to_bytes(),
            WriteFlags::empty(),
        )?;
        txn.commit()?;
        Ok(())
    }

    // ========================================================================
    // Schema Evolution Operations
    // ========================================================================

    /// Create a new schema evolution.
    pub fn create_evolution(&self, db_id: u64, evolution: &Evolution) -> Result<()> {
        let txn = self.root_db.begin_rw_txn()?;
        let evolutions = unsafe { self.table_rw(&txn, self.handles.evolutions) };

        let key = EvolutionKey {
            database_id: db_id,
            evolution_id: evolution.id,
        };
        txn.put(
            &evolutions,
            &key.to_bytes(),
            &evolution.to_bytes(),
            WriteFlags::empty(),
        )?;
        txn.commit()?;
        Ok(())
    }

    /// Get an evolution by ID.
    pub fn get_evolution(
        &self,
        db_id: u64,
        evolution_id: EvolutionId,
    ) -> Result<Option<Evolution>> {
        let txn = self.root_db.begin_ro_txn()?;
        let evolutions = unsafe { self.table(&txn, self.handles.evolutions) };

        let key = EvolutionKey {
            database_id: db_id,
            evolution_id,
        };
        let val: Option<Cow<[u8]>> = txn.get(&evolutions, &key.to_bytes())?;
        if let Some(bytes) = val {
            Ok(Some(Evolution::from_bytes(&bytes)?))
        } else {
            Ok(None)
        }
    }

    /// Update an existing evolution.
    pub fn update_evolution(&self, db_id: u64, evolution: &Evolution) -> Result<()> {
        let txn = self.root_db.begin_rw_txn()?;
        let evolutions = unsafe { self.table_rw(&txn, self.handles.evolutions) };

        let key = EvolutionKey {
            database_id: db_id,
            evolution_id: evolution.id,
        };

        // Verify it exists
        let existing: Option<Cow<[u8]>> = txn.get(&evolutions, &key.to_bytes())?;
        if existing.is_none() {
            anyhow::bail!("Evolution {} not found", evolution.id);
        }

        txn.put(
            &evolutions,
            &key.to_bytes(),
            &evolution.to_bytes(),
            WriteFlags::empty(),
        )?;
        txn.commit()?;
        Ok(())
    }

    /// List all evolutions for a database.
    pub fn list_evolutions(&self, db_id: u64) -> Result<Vec<Evolution>> {
        let txn = self.root_db.begin_ro_txn()?;
        let evolutions = unsafe { self.table(&txn, self.handles.evolutions) };

        let start_key = EvolutionKey {
            database_id: db_id,
            evolution_id: 0,
        };

        let mut result = Vec::new();
        let mut cursor = txn.cursor(&evolutions)?;
        let mut iter = cursor.iter_from::<Cow<[u8]>, Cow<[u8]>>(&start_key.to_bytes());

        while let Some(Ok((k, v))) = iter.next() {
            if let Some(found_key) = EvolutionKey::from_bytes(&k) {
                if found_key.database_id != db_id {
                    break;
                }
                let evolution = Evolution::from_bytes(&v)?;
                result.push(evolution);
            } else {
                break;
            }
        }

        Ok(result)
    }

    /// List active (non-terminal) evolutions for a database.
    pub fn list_active_evolutions(&self, db_id: u64) -> Result<Vec<Evolution>> {
        let all = self.list_evolutions(db_id)?;
        Ok(all.into_iter().filter(|e| e.state.is_active()).collect())
    }

    /// Delete an evolution (usually after completion or cancellation).
    pub fn delete_evolution(&self, db_id: u64, evolution_id: EvolutionId) -> Result<()> {
        let txn = self.root_db.begin_rw_txn()?;
        let evolutions = unsafe { self.table_rw(&txn, self.handles.evolutions) };

        let key = EvolutionKey {
            database_id: db_id,
            evolution_id,
        };
        txn.del(&evolutions, &key.to_bytes(), None)?;
        txn.commit()?;
        Ok(())
    }

    /// Generate a new evolution ID.
    pub fn next_evolution_id(&self, db_id: u64) -> Result<EvolutionId> {
        let txn = self.root_db.begin_rw_txn()?;
        let sys_meta = unsafe { self.table_rw(&txn, self.handles.sys_meta) };

        let key = format!("next_evolution_id:{}", db_id);
        let id_val: Option<Cow<[u8]>> = txn.get(&sys_meta, key.as_bytes())?;
        let id_val = id_val.unwrap_or_else(|| Cow::Borrowed(&[0; 8]));

        let id = u64::from_be_bytes(id_val.as_ref().try_into().unwrap_or([0; 8])) + 1;
        txn.put(
            &sys_meta,
            key.as_bytes(),
            &id.to_be_bytes(),
            WriteFlags::empty(),
        )?;
        txn.commit()?;

        Ok(id)
    }
}
