//! Public API methods for the Manifest.
//!
//! This module contains the public-facing methods of the Manifest struct,
//! including change cursor management, transaction creation, and database operations.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use bincode::serde::{decode_from_slice, encode_to_vec};
use heed::{EnvOpenOptions, RoTxn};
use tracing::{debug, error, info, instrument, trace, warn};

use super::change_log::ChangeLogState;
use super::cursors::{CursorAckRequest, CursorRegistrationRequest};
use super::operations::load_pending_batches;
use super::state::{ChangeSignal, ManifestDiagnostics, ManifestRuntimeState};
use super::tables::ManifestTables;
use super::worker::{ManifestCommand, WorkerHandle, worker_loop};
use super::*;

#[cfg(feature = "libsql")]
#[derive(Debug, Clone)]
pub struct PageLocation {
    pub base_chunk: ChunkEntryRecord,
    pub base_chunk_key: ChunkKey,
    pub base_start_page: u64,
    pub deltas: Vec<DeltaLocation>,
}

#[cfg(feature = "libsql")]
#[derive(Debug, Clone)]
pub struct DeltaLocation {
    pub delta_key: ChunkDeltaKey,
    pub delta_record: LibSqlChunkDeltaRecord,
    pub generation: Generation,
}

/// Starting position for a new change cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeCursorStart {
    /// Start at the oldest available sequence (before any entries).
    Oldest,

    /// Start at the latest sequence (won't see any existing entries).
    Latest,

    /// Start just before the specified sequence number.
    Sequence(ChangeSequence),
}

/// Snapshot of a change cursor's current state.
///
/// Returned when registering or acknowledging a cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeCursorSnapshot {
    /// Unique identifier for this cursor.
    pub cursor_id: ChangeCursorId,

    /// Last sequence number acknowledged by this cursor.
    pub acked_sequence: ChangeSequence,

    /// Next sequence number this cursor should read.
    pub next_sequence: ChangeSequence,

    /// Latest available sequence number.
    pub latest_sequence: ChangeSequence,

    /// Oldest available sequence number (may be > 1 due to truncation).
    pub oldest_sequence: ChangeSequence,
}

/// A page of change log entries fetched for a cursor.
#[derive(Debug, Clone)]
pub struct ChangeBatchPage {
    /// The cursor this page was fetched for.
    pub cursor_id: ChangeCursorId,

    /// The change records in this page.
    pub changes: Vec<ManifestChangeRecord>,

    /// Next sequence number to fetch (first sequence not in this page).
    pub next_sequence: ChangeSequence,

    /// Latest available sequence number.
    pub latest_sequence: ChangeSequence,

    /// Oldest available sequence number.
    pub oldest_sequence: ChangeSequence,
}

impl Manifest {
    /// Open or create a manifest at the specified path.
    #[instrument(skip(options), fields(
        path = %path.as_ref().display(),
        initial_map_size = options.initial_map_size,
        max_map_size = options.max_map_size
    ))]
    pub fn open(path: impl AsRef<Path>, options: ManifestOptions) -> Result<Self, ManifestError> {
        info!("Opening manifest");
        let path_ref = path.as_ref();
        std::fs::create_dir_all(path_ref)?;

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(options.initial_map_size)
                .max_dbs(options.max_dbs)
                .open(path_ref)?
        };

        let tables = Arc::new(ManifestTables::open(&env)?);

        let generation_cache = Arc::new(Mutex::new(load_generations(&env, &tables)?));
        let job_seed = generation_cache
            .lock()
            .ok()
            .and_then(|map| map.get(&JOB_ID_COMPONENT).copied())
            .unwrap_or(0);
        let job_id_counter = Arc::new(AtomicU64::new(job_seed));
        let diagnostics = Arc::new(ManifestDiagnostics::default());
        let (pending_replay, last_pending_id) = load_pending_batches(&env, &tables)?;
        let batch_journal_counter = Arc::new(AtomicU64::new(last_pending_id));
        let mut change_bootstrap = load_change_state(&env, &tables)?;
        apply_startup_truncation(&env, &tables, &mut change_bootstrap.state)?;

        let initial_change_state = change_bootstrap.state.clone();
        let change_state = Arc::new(Mutex::new(change_bootstrap.state));
        let change_signal = Arc::new(ChangeSignal::new(initial_change_state.latest_sequence()));
        let cursor_id_counter = Arc::new(AtomicU64::new(change_bootstrap.next_cursor_id));

        let (command_tx, command_rx) = std::sync::mpsc::sync_channel(options.queue_capacity.max(1));

        let worker_env = env.clone();
        let worker_tables = tables.clone();
        let worker_diagnostics = diagnostics.clone();
        let worker_generation_cache = generation_cache.clone();
        let worker_job_counter = job_id_counter.clone();
        let initial_generations = generation_cache
            .lock()
            .map(|map| map.clone())
            .unwrap_or_default();
        let worker_change_state = change_state.clone();
        let worker_change_signal = change_signal.clone();
        let worker_cursor_counter = cursor_id_counter.clone();
        let worker_initial_change_state = initial_change_state.clone();
        let pending_replay_len = pending_replay.len();
        let worker_pending_replay = pending_replay;
        let worker_batch_counter = batch_journal_counter.clone();
        let commit_latency = options.commit_latency;

        let runtime_state = ManifestRuntimeState::initialize(&env, &tables)?;

        let worker_current_map_size = Arc::new(AtomicU64::new(options.initial_map_size as u64));
        let current_map_size = worker_current_map_size.clone();
        let worker_max_map_size = options.max_map_size;
        let worker_path = path_ref.to_path_buf();

        let join = thread::Builder::new()
            .name("bop-manifest-writer".into())
            .spawn(move || {
                worker_loop(
                    worker_env,
                    worker_tables,
                    worker_diagnostics,
                    worker_generation_cache,
                    worker_job_counter,
                    worker_change_state,
                    worker_change_signal,
                    worker_cursor_counter,
                    command_rx,
                    initial_generations,
                    worker_initial_change_state,
                    worker_pending_replay,
                    worker_batch_counter,
                    commit_latency,
                    worker_current_map_size,
                    worker_max_map_size,
                    worker_path,
                )
            })
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;

        let manifest = Self {
            env,
            tables,
            command_tx,
            worker: WorkerHandle { join: Some(join) },
            diagnostics,
            generation_cache,
            job_id_counter,
            change_state,
            change_signal,
            runtime_state,
            current_map_size,
            max_map_size: options.max_map_size,
            path: path_ref.to_path_buf(),
        };

        info!(
            crash_detected = manifest.runtime_state.crash_detected,
            pending_batches = pending_replay_len,
            "Manifest opened successfully"
        );

        Ok(manifest)
    }

    /// Begin a new transaction with default capacity.
    #[instrument(skip(self))]
    pub fn begin(&self) -> ManifestTxn<'_> {
        trace!("Beginning transaction with default capacity");
        self.begin_with_capacity(16)
    }

    /// Begin a new transaction with the specified operation capacity.
    #[instrument(skip(self))]
    pub fn begin_with_capacity(&self, capacity: usize) -> ManifestTxn<'_> {
        trace!(capacity, "Beginning transaction with capacity");
        ManifestTxn {
            manifest: self,
            ops: Vec::with_capacity(capacity),
        }
    }

    /// Attempt to expand the LMDB map size when MDB_FULL is encountered.
    ///
    /// Doubles the current map size up to the maximum configured limit.
    /// Returns true if expansion succeeded, false if already at maximum.
    #[instrument(skip(self))]
    pub(crate) fn try_expand_map_size(&self) -> Result<bool, ManifestError> {
        use std::sync::atomic::Ordering;

        let current = self.current_map_size.load(Ordering::Acquire) as usize;

        // Try to double the size, but cap at max
        let new_size = (current * 2).min(self.max_map_size);

        if new_size <= current {
            // Already at maximum
            warn!(
                current_size = current,
                max_size = self.max_map_size,
                "Cannot expand LMDB map size - already at maximum"
            );
            return Ok(false);
        }

        // Resize the environment
        unsafe {
            self.env.resize(new_size)?;
        }

        self.current_map_size.store(new_size as u64, Ordering::Release);

        info!(
            old_size = current,
            new_size = new_size,
            max_size = self.max_map_size,
            "Expanded LMDB map size"
        );

        Ok(true)
    }

    /// Register a new change cursor.
    ///
    /// Returns a snapshot of the cursor state after registration.
    #[instrument(skip(self))]
    pub fn register_change_cursor(
        &self,
        start: ChangeCursorStart,
    ) -> Result<ChangeCursorSnapshot, ManifestError> {
        debug!(?start, "Registering change cursor");
        let (tx, rx) = mpsc::sync_channel(0);
        self.send_command(ManifestCommand::RegisterCursor(CursorRegistrationRequest {
            start,
            responder: tx,
        }))?;
        match rx.recv() {
            Ok(result) => {
                match &result {
                    Ok(snapshot) => {
                        info!(
                            cursor_id = snapshot.cursor_id,
                            acked_sequence = snapshot.acked_sequence,
                            next_sequence = snapshot.next_sequence,
                            latest_sequence = snapshot.latest_sequence,
                            "Change cursor registered"
                        );
                    }
                    Err(e) => {
                        error!(error = ?e, "Failed to register change cursor");
                    }
                }
                result
            }
            Err(_) => Err(ManifestError::WorkerClosed),
        }
    }

    /// Acknowledge changes up to a sequence number for a cursor.
    ///
    /// Returns the updated cursor state.
    #[instrument(skip(self))]
    pub fn acknowledge_changes(
        &self,
        cursor_id: ChangeCursorId,
        upto_sequence: ChangeSequence,
    ) -> Result<ChangeCursorSnapshot, ManifestError> {
        debug!(cursor_id, upto_sequence, "Acknowledging changes");
        let (tx, rx) = mpsc::sync_channel(0);
        self.send_command(ManifestCommand::AcknowledgeCursor(CursorAckRequest {
            cursor_id,
            sequence: upto_sequence,
            responder: tx,
        }))?;
        match rx.recv() {
            Ok(result) => {
                match &result {
                    Ok(snapshot) => {
                        debug!(
                            cursor_id = snapshot.cursor_id,
                            acked_sequence = snapshot.acked_sequence,
                            next_sequence = snapshot.next_sequence,
                            "Changes acknowledged"
                        );
                    }
                    Err(e) => {
                        error!(cursor_id, error = ?e, "Failed to acknowledge changes");
                    }
                }
                result
            }
            Err(_) => Err(ManifestError::WorkerClosed),
        }
    }

    /// Fetch a page of change log entries for a cursor.
    ///
    /// Returns up to `page_size` change records starting from the cursor's
    /// current position.
    #[instrument(skip(self))]
    pub fn fetch_change_page(
        &self,
        cursor_id: ChangeCursorId,
        page_size: usize,
    ) -> Result<ChangeBatchPage, ManifestError> {
        trace!(cursor_id, page_size, "Fetching change page");
        let state = self.change_state_snapshot()?;
        let cursor_state = state.cursors.get(&cursor_id).cloned().ok_or_else(|| {
            ManifestError::InvariantViolation(format!("cursor {cursor_id} not registered"))
        })?;

        let start_sequence = cursor_state
            .acked_sequence
            .saturating_add(1)
            .max(state.oldest_sequence);
        let mut remaining = page_size;
        let mut changes = Vec::new();

        let latest_sequence = state.latest_sequence();

        if remaining > 0 && start_sequence <= latest_sequence {
            let txn = self.env.read_txn()?;
            let mut sequence = start_sequence;
            let config = bincode::config::standard();

            while remaining > 0 && sequence <= latest_sequence {
                match self.tables.change_log.get(&txn, &sequence)? {
                    Some(record) => {
                        changes.push(record);
                        remaining -= 1;
                    }
                    None => break,
                }

                sequence = sequence.saturating_add(1);
            }

            txn.commit()?;
        }

        let next_sequence = changes
            .last()
            .map(|record| record.sequence.saturating_add(1))
            .unwrap_or(start_sequence);

        debug!(
            cursor_id,
            changes_fetched = changes.len(),
            next_sequence,
            latest_sequence = state.latest_sequence(),
            "Change page fetched"
        );

        Ok(ChangeBatchPage {
            cursor_id,
            changes,
            next_sequence,
            latest_sequence: state.latest_sequence(),
            oldest_sequence: state.oldest_sequence,
        })
    }

    /// Wait for a change sequence greater than `since`.
    ///
    /// Blocks until a new change is committed or the deadline is reached.
    pub fn wait_for_change(
        &self,
        since: ChangeSequence,
        deadline: Option<Instant>,
    ) -> Result<ChangeSequence, ManifestError> {
        self.change_signal.wait_for(since, deadline)
    }

    /// Get the latest change sequence number.
    pub fn latest_change_sequence(&self) -> ChangeSequence {
        self.change_signal.current()
    }

    /// Wait for a component's generation to reach a target value.
    pub fn wait_for_generation(
        &self,
        component: ComponentId,
        target: Generation,
        deadline: Instant,
    ) -> Result<(), ManifestError> {
        if self.current_generation(component).unwrap_or(0) >= target {
            return Ok(());
        }

        let (tx, rx) = mpsc::sync_channel(0);
        self.send_command(ManifestCommand::Wait(WaitRequest {
            component,
            target,
            deadline,
            responder: tx,
        }))?;

        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err(ManifestError::WorkerClosed),
        }
    }

    /// Fork a database to a new database ID.
    ///
    /// Copies all chunks, deltas, WAL state, and artifacts from the source
    /// database to the target database. Reference counts are adjusted accordingly.
    #[instrument(skip(self, descriptor))]
    pub fn fork_db(
        &self,
        source_db: DbId,
        target_db: DbId,
        descriptor: AofDescriptorRecord,
    ) -> Result<(), ManifestError> {
        info!(source_db, target_db, "Forking database");
        let data = self.read(|tables, txn| {
            if tables.aof_db.get(txn, &(target_db as u64))?.is_some() {
                return Err(ManifestError::InvariantViolation(format!(
                    "db {target_db} already exists"
                )));
            }

            if tables.aof_db.get(txn, &(source_db as u64))?.is_none() {
                return Err(ManifestError::InvariantViolation(format!(
                    "source db {source_db} missing"
                )));
            }

            let mut chunk_entries = Vec::new();
            let mut chunk_cursor = tables.chunk_catalog.iter(txn)?;
            while let Some((raw_key, value)) = chunk_cursor.next().transpose()? {
                let key = ChunkKey::decode(raw_key);
                if key.db_id == source_db {
                    chunk_entries.push((key, value));
                }
            }

            #[cfg(feature = "libsql")]
            let mut chunk_deltas = Vec::new();
            #[cfg(feature = "libsql")]
            {
                let mut delta_cursor = tables.libsql_chunk_delta_index.iter(txn)?;
                while let Some((raw_key, value)) = delta_cursor.next().transpose()? {
                    let key = ChunkDeltaKey::decode(raw_key);
                    if key.db_id == source_db {
                        chunk_deltas.push((key, value));
                    }
                }
            }

            let mut aof_states = Vec::new();
            let mut aof_cursor = tables.aof_state.iter(txn)?;
            while let Some((raw_key, value)) = aof_cursor.next().transpose()? {
                let key = AofStateKey::decode(raw_key);
                if key.db_id == source_db {
                    aof_states.push((key, value));
                }
            }

            let mut wal_artifacts = Vec::new();
            let mut artifact_cursor = tables.aof_wal.iter(txn)?;
            while let Some((raw_key, value)) = artifact_cursor.next().transpose()? {
                let key = WalArtifactKey::decode(raw_key);
                if key.db_id == source_db {
                    wal_artifacts.push((key, value));
                }
            }

            Ok(ForkData {
                chunk_entries,
                #[cfg(feature = "libsql")]
                chunk_deltas,
                aof_states,
                wal_artifacts,
            })
        })?;

        let capacity = 1
            + data.aof_states.len()
            + data.chunk_entries.len() * 2
            + {
                #[cfg(feature = "libsql")]
                { data.chunk_deltas.len() }
                #[cfg(not(feature = "libsql"))]
                { 0 }
            }
            + data.wal_artifacts.len();
        let mut txn = self.begin_with_capacity(capacity.max(4));

        txn.put_aof_db(target_db, descriptor);

        for (key, record) in &data.aof_states {
            let new_key = key.with_db(target_db);
            txn.put_aof_state(new_key, record.clone());
        }

        for (key, record) in &data.chunk_entries {
            let new_key = key.with_db(target_db);
            txn.upsert_chunk(new_key, record.clone());
            txn.adjust_refcount(key.chunk_id, 1);
        }

        #[cfg(feature = "libsql")]
        for (key, record) in &data.chunk_deltas {
            let new_key = key.with_db(target_db);
            txn.upsert_chunk_delta(new_key, record.clone());
        }

        for (key, record) in &data.wal_artifacts {
            let new_key = key.with_db(target_db);
            let mut clone = record.clone();
            clone.db_id = target_db;
            txn.register_wal_artifact(new_key, clone);
        }

        txn.commit()?;

        info!(
            source_db,
            target_db,
            chunks_forked = data.chunk_entries.len(),
            aof_states_forked = data.aof_states.len(),
            wal_artifacts_forked = data.wal_artifacts.len(),
            "Database forked successfully"
        );

        Ok(())
    }

    /// Reserve a new job ID without creating a job.
    pub fn reserve_job_id(&self) -> JobId {
        self.next_job_id()
    }

    /// Get all AOF WAL artifacts for a database.
    pub fn wal_artifacts(
        &self,
        db_id: DbId,
    ) -> Result<Vec<(WalArtifactKey, AofWalArtifactRecord)>, ManifestError> {
        self.read(|tables, txn| {
            let mut out = Vec::new();
            let mut cursor = tables.aof_wal.iter(txn)?;
            while let Some((raw_key, record)) = cursor.next().transpose()? {
                let key = WalArtifactKey::decode(raw_key);
                if key.db_id == db_id {
                    out.push((key, record));
                }
            }
            Ok(out)
        })
    }

    /// Get the AOF state for a database.
    pub fn aof_state(&self, db_id: DbId) -> Result<Option<AofStateRecord>, ManifestError> {
        self.read(|tables, txn| {
            let key = AofStateKey::new(db_id).encode();
            Ok(tables.aof_state.get(txn, &key)?)
        })
    }

    /// Resolve the location of a page (base chunk + delta chain) for PageStore reads.
    ///
    /// This is the Phase 2 implementation that enables T4c delta layering.
    #[cfg(feature = "libsql")]
    pub fn resolve_page_location(
        &self,
        db_id: u32,
        page_no: u64,
    ) -> Result<PageLocation, ManifestError> {
        self.read(|tables, txn| {
            let mut chunk_entries = Vec::new();
            let mut chunk_cursor = tables.chunk_catalog.iter(txn)?;
            while let Some((raw_key, record)) = chunk_cursor.next().transpose()? {
                let key = ChunkKey::decode(raw_key);
                if key.db_id == db_id {
                    chunk_entries.push((key, record));
                }
            }

            if chunk_entries.is_empty() {
                return Err(ManifestError::InvariantViolation(format!(
                    "no chunks found for db {}",
                    db_id
                )));
            }

            chunk_entries.sort_by_key(|(key, _)| key.chunk_id);

            let requested_page = page_no;
            let mut start_page_index = 0u64;
            let mut selected: Option<(ChunkKey, ChunkEntryRecord, u64)> = None;

            for (key, record) in chunk_entries.into_iter() {
                let page_count = record.effective_page_count();

                let end_page_index = start_page_index.checked_add(page_count).ok_or_else(|| {
                    ManifestError::InvariantViolation(format!(
                        "page index overflow while scanning chunks for db {}",
                        db_id
                    ))
                })?;

                if page_count > 0 && requested_page < end_page_index {
                    selected = Some((key, record, start_page_index));
                    break;
                }

                start_page_index = end_page_index;
            }

            let (chunk_key, base_chunk, base_start_page) = selected.ok_or_else(|| {
                ManifestError::InvariantViolation(format!(
                    "page {} not covered by any chunk in db {}",
                    page_no, db_id
                ))
            })?;

            let mut deltas = Vec::new();
            let start_key = ChunkDeltaKey::new(db_id, chunk_key.chunk_id, 0).encode();
            let end_key = ChunkDeltaKey::new(db_id, chunk_key.chunk_id, u32::MAX).encode();
            let mut delta_cursor = tables
                .libsql_chunk_delta_index
                .range(txn, &(start_key..=end_key))?;
            while let Some((raw_key, record)) = delta_cursor.next().transpose()? {
                let delta_key = ChunkDeltaKey::decode(raw_key);

                if delta_key.db_id != db_id || delta_key.chunk_id != chunk_key.chunk_id {
                    break;
                }

                deltas.push(DeltaLocation {
                    delta_key,
                    delta_record: record,
                    generation: delta_key.generation as Generation,
                });
            }

            deltas.sort_by_key(|d| d.generation);

            Ok(PageLocation {
                base_chunk,
                base_chunk_key: chunk_key,
                base_start_page,
                deltas,
            })
        })
    }

    /// Get the maximum database ID currently in use.
    pub fn max_db_id(&self) -> Result<Option<u32>, ManifestError> {
        self.read(|tables, txn| {
            let mut cursor = tables.aof_db.iter(txn)?;
            let mut max_id: Option<u32> = None;
            while let Some((raw_key, _)) = cursor.next().transpose()? {
                let current = raw_key as u32;
                max_id = Some(match max_id {
                    Some(existing) => existing.max(current),
                    None => current,
                });
            }
            Ok(max_id)
        })
    }

    /// Get the current generation for a component.
    pub fn current_generation(&self, component: ComponentId) -> Option<Generation> {
        self.generation_cache
            .lock()
            .ok()
            .and_then(|map| map.get(&component).copied())
    }

    /// Get a job record by ID (T6b).
    pub fn get_job(&self, job_id: JobId) -> Result<Option<JobRecord>, ManifestError> {
        self.read(|tables, txn| Ok(tables.job_queue.get(txn, &job_id)?))
    }

    /// Get a snapshot of manifest diagnostics.
    pub fn diagnostics(&self) -> ManifestDiagnosticsSnapshot {
        ManifestDiagnosticsSnapshot::from_manifest(&self.diagnostics)
    }

    /// Check if a crash was detected on startup.
    pub fn crash_detected(&self) -> bool {
        self.runtime_state.crash_detected
    }

    /// Get the runtime state record.
    pub fn runtime_state(&self) -> &RuntimeStateRecord {
        &self.runtime_state.record
    }

    /// Look up an AOF chunk record by AOF ID and LSN.
    ///
    /// This performs an efficient range query to find the chunk containing the given LSN.
    /// Returns None if no chunk is found for the given AOF and LSN.
    pub fn get_aof_chunk(
        &self,
        aof_id: crate::aof::AofId,
        lsn: u64,
    ) -> Result<Option<AofChunkRecord>, ManifestError> {
        self.read(|tables, txn| {
            // Create a range query starting from (aof_id, lsn)
            let search_key = AofChunkRecord::make_key(aof_id, lsn);

            // Find the chunk with start_lsn <= lsn
            let mut iter = tables.aof_chunks.rev_iter(txn)?;

            // Seek to our search key or the first key before it
            if let Some(result) = iter.next() {
                let (key, record) = result?;
                let (record_aof_id, start_lsn) = AofChunkRecord::decode_key(key);

                // Check if this chunk belongs to our AOF and contains the LSN
                if record_aof_id == aof_id && start_lsn <= lsn && lsn < record.end_lsn {
                    return Ok(Some(record));
                }
            }

            Ok(None)
        })
    }

    /// List all AOF chunks for a given AOF ID.
    ///
    /// Returns chunks in ascending order by start_lsn.
    pub fn list_aof_chunks(
        &self,
        aof_id: crate::aof::AofId,
    ) -> Result<Vec<AofChunkRecord>, ManifestError> {
        self.read(|tables, txn| {
            let mut chunks = Vec::new();

            // Create range for this AOF: from (aof_id, 0) to (aof_id, u64::MAX)
            let start_key = AofChunkRecord::make_key(aof_id, 0);
            let end_key = AofChunkRecord::make_key(aof_id, u64::MAX);

            let range = start_key..=end_key;
            let mut iter = tables.aof_chunks.range(txn, &range)?;

            while let Some(result) = iter.next() {
                let (_key, record) = result?;
                chunks.push(record);
            }

            Ok(chunks)
        })
    }

    /// Look up a LibSQL chunk record by LibSQL ID and chunk ID.
    #[cfg(feature = "libsql")]
    pub fn get_libsql_chunk(
        &self,
        libsql_id: crate::libsql::LibSqlId,
        chunk_id: ChunkId,
    ) -> Result<Option<LibSqlChunkRecord>, ManifestError> {
        self.read(|tables, txn| {
            let key = LibSqlChunkRecord::make_key(libsql_id, chunk_id);
            Ok(tables.libsql_chunks.get(txn, &key)?)
        })
    }

    /// List all LibSQL chunks for a given LibSQL database ID.
    ///
    /// Returns chunks in ascending order by chunk_id.
    #[cfg(feature = "libsql")]
    pub fn list_libsql_chunks(
        &self,
        libsql_id: crate::libsql::LibSqlId,
    ) -> Result<Vec<LibSqlChunkRecord>, ManifestError> {
        self.read(|tables, txn| {
            let mut chunks = Vec::new();

            // Create range for this LibSQL instance: from (libsql_id, 0) to (libsql_id, u32::MAX)
            let start_key = LibSqlChunkRecord::make_key(libsql_id, 0);
            let end_key = LibSqlChunkRecord::make_key(libsql_id, u32::MAX);

            let range = start_key..=end_key;
            let mut iter = tables.libsql_chunks.range(txn, &range)?;

            while let Some(result) = iter.next() {
                let (_key, record) = result?;
                chunks.push(record);
            }

            Ok(chunks)
        })
    }

    /// Execute a read-only operation on the manifest.
    pub(crate) fn read<T, F>(&self, f: F) -> Result<T, ManifestError>
    where
        F: FnOnce(&ManifestTables, &RoTxn<'_>) -> Result<T, ManifestError>,
    {
        let txn = self.env.read_txn()?;
        let result = f(&self.tables, &txn)?;
        txn.commit()?;
        Ok(result)
    }

    /// Send a command to the worker thread.
    pub(super) fn send_command(&self, command: ManifestCommand) -> Result<(), ManifestError> {
        self.command_tx
            .send(command)
            .map_err(|_| ManifestError::WorkerClosed)
    }

    /// Allocate a new job ID.
    pub(super) fn next_job_id(&self) -> JobId {
        self.job_id_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1
    }

    /// Get a snapshot of current generations.
    pub(super) fn current_generation_snapshot(&self) -> HashMap<ComponentId, Generation> {
        self.generation_cache
            .lock()
            .map(|map| map.clone())
            .unwrap_or_default()
    }

    /// Get a snapshot of the change log state.
    pub(super) fn change_state_snapshot(&self) -> Result<ChangeLogState, ManifestError> {
        self.change_state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| ManifestError::InvariantViolation("change-log state poisoned".into()))
    }
}

impl Drop for Manifest {
    fn drop(&mut self) {
        let _ = self.runtime_state.clear(&self.env, &self.tables);
        let _ = self.command_tx.send(ManifestCommand::Shutdown);
        self.worker.stop();
    }
}
