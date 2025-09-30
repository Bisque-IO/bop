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
use heed::{Env, EnvOpenOptions, RoTxn};

use super::change_log::{
    ChangeLogState, apply_startup_truncation, compute_change_log_cache_id,
    hydrate_change_log_cache, load_change_state,
};
use super::cursors::{CursorAckRequest, CursorRegistrationRequest};
use super::manifest_ops::ManifestOp;
use super::operations::{ForkData, load_generations, load_pending_batches};
use super::state::{
    ChangeSignal, ChangeStateBootstrap, ManifestDiagnostics, ManifestDiagnosticsSnapshot,
    ManifestRuntimeState,
};
use super::tables::{JOB_ID_COMPONENT, ManifestTables, epoch_millis};
use super::transaction::ManifestTxn;
use super::worker::{ManifestCommand, WaitRequest, WorkerHandle, worker_loop};
use super::*;
use crate::page_cache::{PageCache, PageCacheKey, PageCacheMetricsSnapshot};

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
    pub fn open(path: impl AsRef<Path>, options: ManifestOptions) -> Result<Self, ManifestError> {
        let path_ref = path.as_ref();
        std::fs::create_dir_all(path_ref)?;

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(options.map_size)
                .max_dbs(options.max_dbs)
                .open(path_ref)?
        };

        let tables = Arc::new(ManifestTables::open(&env)?);

        let page_cache = options.page_cache.clone();
        let change_log_cache_object_id = options.change_log_cache_object_id.or_else(|| {
            page_cache
                .as_ref()
                .map(|_| compute_change_log_cache_id(path_ref))
        });

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
        if let (Some(cache), Some(object_id)) = (page_cache.as_ref(), change_log_cache_object_id) {
            hydrate_change_log_cache(
                &env,
                &tables,
                cache,
                object_id,
                change_bootstrap.state.oldest_sequence,
                change_bootstrap.state.latest_sequence(),
            )?;
        }
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
        let worker_pending_replay = pending_replay;
        let worker_batch_counter = batch_journal_counter.clone();
        let commit_latency = options.commit_latency;
        let worker_page_cache = page_cache.clone();
        let worker_cache_object_id = change_log_cache_object_id;

        let runtime_state = ManifestRuntimeState::initialize(&env, &tables)?;

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
                    worker_page_cache,
                    worker_cache_object_id,
                )
            })
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;

        Ok(Self {
            env,
            tables,
            command_tx,
            worker: WorkerHandle { join: Some(join) },
            diagnostics,
            generation_cache,
            job_id_counter,
            change_state,
            change_signal,
            page_cache,
            change_log_cache_object_id,
            runtime_state,
        })
    }

    /// Begin a new transaction with default capacity.
    pub fn begin(&self) -> ManifestTxn<'_> {
        self.begin_with_capacity(16)
    }

    /// Begin a new transaction with the specified operation capacity.
    pub fn begin_with_capacity(&self, capacity: usize) -> ManifestTxn<'_> {
        ManifestTxn {
            manifest: self,
            ops: Vec::with_capacity(capacity),
        }
    }

    /// Register a new change cursor.
    ///
    /// Returns a snapshot of the cursor state after registration.
    pub fn register_change_cursor(
        &self,
        start: ChangeCursorStart,
    ) -> Result<ChangeCursorSnapshot, ManifestError> {
        let (tx, rx) = mpsc::sync_channel(0);
        self.send_command(ManifestCommand::RegisterCursor(CursorRegistrationRequest {
            start,
            responder: tx,
        }))?;
        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err(ManifestError::WorkerClosed),
        }
    }

    /// Acknowledge changes up to a sequence number for a cursor.
    ///
    /// Returns the updated cursor state.
    pub fn acknowledge_changes(
        &self,
        cursor_id: ChangeCursorId,
        upto_sequence: ChangeSequence,
    ) -> Result<ChangeCursorSnapshot, ManifestError> {
        let (tx, rx) = mpsc::sync_channel(0);
        self.send_command(ManifestCommand::AcknowledgeCursor(CursorAckRequest {
            cursor_id,
            sequence: upto_sequence,
            responder: tx,
        }))?;
        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err(ManifestError::WorkerClosed),
        }
    }

    /// Fetch a page of change log entries for a cursor.
    ///
    /// Returns up to `page_size` change records starting from the cursor's
    /// current position.
    pub fn fetch_change_page(
        &self,
        cursor_id: ChangeCursorId,
        page_size: usize,
    ) -> Result<ChangeBatchPage, ManifestError> {
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

        let change_log_cache = self.change_log_cache();
        let latest_sequence = state.latest_sequence();

        if remaining > 0 && start_sequence <= latest_sequence {
            let txn = self.env.read_txn()?;
            let mut sequence = start_sequence;
            let config = bincode::config::standard();

            while remaining > 0 && sequence <= latest_sequence {
                if let Some((cache, object_id)) = change_log_cache.as_ref() {
                    if let Some(frame) = cache.get(&PageCacheKey::manifest(*object_id, sequence)) {
                        let (record, _len) = decode_from_slice(frame.as_slice(), config)
                            .map_err(|err| ManifestError::CacheSerialization(err.to_string()))?;
                        changes.push(record);
                        remaining -= 1;
                        sequence = sequence.saturating_add(1);
                        continue;
                    }
                }

                match self.tables.change_log.get(&txn, &sequence)? {
                    Some(record) => {
                        if let Some((cache, object_id)) = change_log_cache.as_ref() {
                            if let Ok(bytes) = encode_to_vec(&record, config) {
                                cache.insert(
                                    PageCacheKey::manifest(*object_id, sequence),
                                    Arc::from(bytes.into_boxed_slice()),
                                );
                            }
                        }
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

    /// Get the change log cache if configured.
    pub(super) fn change_log_cache(&self) -> Option<(Arc<PageCache<PageCacheKey>>, u64)> {
        self.page_cache.as_ref().and_then(|cache| {
            self.change_log_cache_object_id
                .map(|id| (cache.clone(), id))
        })
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
    pub fn fork_db(
        &self,
        source_db: DbId,
        target_db: DbId,
        descriptor: DbDescriptorRecord,
    ) -> Result<(), ManifestError> {
        let data = self.read(|tables, txn| {
            if tables.db.get(txn, &(target_db as u64))?.is_some() {
                return Err(ManifestError::InvariantViolation(format!(
                    "db {target_db} already exists"
                )));
            }

            if tables.db.get(txn, &(source_db as u64))?.is_none() {
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

            let mut chunk_deltas = Vec::new();
            let mut delta_cursor = tables.chunk_delta_index.iter(txn)?;
            while let Some((raw_key, value)) = delta_cursor.next().transpose()? {
                let key = ChunkDeltaKey::decode(raw_key);
                if key.db_id == source_db {
                    chunk_deltas.push((key, value));
                }
            }

            let mut wal_states = Vec::new();
            let mut wal_cursor = tables.wal_state.iter(txn)?;
            while let Some((raw_key, value)) = wal_cursor.next().transpose()? {
                let key = WalStateKey::decode(raw_key);
                if key.db_id == source_db {
                    wal_states.push((key, value));
                }
            }

            let mut wal_artifacts = Vec::new();
            let mut artifact_cursor = tables.wal_catalog.iter(txn)?;
            while let Some((raw_key, value)) = artifact_cursor.next().transpose()? {
                let key = WalArtifactKey::decode(raw_key);
                if key.db_id == source_db {
                    wal_artifacts.push((key, value));
                }
            }

            Ok(ForkData {
                chunk_entries,
                chunk_deltas,
                wal_states,
                wal_artifacts,
            })
        })?;

        let capacity = 1
            + data.wal_states.len()
            + data.chunk_entries.len() * 2
            + data.chunk_deltas.len()
            + data.wal_artifacts.len();
        let mut txn = self.begin_with_capacity(capacity.max(4));

        txn.put_db(target_db, descriptor);

        for (key, record) in &data.wal_states {
            let mut clone = record.clone();
            clone.flush_gate_state.active_job_id = None;
            clone.flush_gate_state.errored_since_epoch_ms = None;
            let new_key = key.with_db(target_db);
            txn.put_wal_state(new_key, clone);
        }

        for (key, record) in &data.chunk_entries {
            let new_key = key.with_db(target_db);
            txn.upsert_chunk(new_key, record.clone());
            txn.adjust_refcount(key.chunk_id, 1);
        }

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
        Ok(())
    }

    /// Reserve a new job ID without creating a job.
    pub fn reserve_job_id(&self) -> JobId {
        self.next_job_id()
    }

    /// Get all WAL artifacts for a database.
    pub fn wal_artifacts(
        &self,
        db_id: DbId,
    ) -> Result<Vec<(WalArtifactKey, WalArtifactRecord)>, ManifestError> {
        self.read(|tables, txn| {
            let mut out = Vec::new();
            let mut cursor = tables.wal_catalog.iter(txn)?;
            while let Some((raw_key, record)) = cursor.next().transpose()? {
                let key = WalArtifactKey::decode(raw_key);
                if key.db_id == db_id {
                    out.push((key, record));
                }
            }
            Ok(out)
        })
    }

    /// Get the WAL state for a database.
    pub fn wal_state(&self, db_id: DbId) -> Result<Option<WalStateRecord>, ManifestError> {
        self.read(|tables, txn| {
            let key = WalStateKey::new(db_id).encode();
            Ok(tables.wal_state.get(txn, &key)?)
        })
    }

    /// Get the maximum database ID currently in use.
    pub fn max_db_id(&self) -> Result<Option<u32>, ManifestError> {
        self.read(|tables, txn| {
            let mut cursor = tables.db.iter(txn)?;
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

    /// Get a snapshot of manifest diagnostics.
    pub fn diagnostics(&self) -> ManifestDiagnosticsSnapshot {
        ManifestDiagnosticsSnapshot::from_manifest(&self.diagnostics, &self.page_cache)
    }

    /// Get page cache metrics if a cache is configured.
    pub fn page_cache_metrics(&self) -> Option<PageCacheMetricsSnapshot> {
        self.page_cache.as_ref().map(|cache| cache.metrics())
    }

    /// Check if a crash was detected on startup.
    pub fn crash_detected(&self) -> bool {
        self.runtime_state.crash_detected
    }

    /// Get the runtime state record.
    pub fn runtime_state(&self) -> &RuntimeStateRecord {
        &self.runtime_state.record
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
