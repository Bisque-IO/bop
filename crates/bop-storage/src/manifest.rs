use std::collections::{HashMap, VecDeque, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bincode::serde::{decode_from_slice, encode_to_vec};
use heed::types::{SerdeBincode, U32, U64, U128};
use heed::{Database, Env, EnvOpenOptions, RoTxn, RwTxn};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::flush::{FlushSink, FlushSinkError, FlushSinkRequest};
use crate::page_cache::{PageCache, PageCacheKey, PageCacheMetricsSnapshot};

pub type DbId = u32;
pub type ChunkId = u32;
pub type SnapshotId = u32;
pub type JobId = u64;
pub type ComponentId = u32;
pub type Generation = u64;
pub type RemoteNamespaceId = u32;
pub type WalArtifactId = u64;
pub type ChangeSequence = u64;
pub type ChangeCursorId = u64;

const JOB_ID_COMPONENT: ComponentId = 0;

const DB_DESCRIPTOR_VERSION: u16 = 1;
const WAL_STATE_VERSION: u16 = 1;
const CHUNK_ENTRY_VERSION: u16 = 1;
const CHUNK_DELTA_VERSION: u16 = 1;
const SNAPSHOT_RECORD_VERSION: u16 = 1;
const WAL_ARTIFACT_VERSION: u16 = 1;
const JOB_RECORD_VERSION: u16 = 1;
const METRIC_RECORD_VERSION: u16 = 1;
const GENERATION_RECORD_VERSION: u16 = 1;
const CHUNK_REFCOUNT_VERSION: u16 = 1;
#[allow(dead_code)]
const REMOTE_NAMESPACE_VERSION: u16 = 1;
const CHANGE_RECORD_VERSION: u16 = 1;
const CURSOR_RECORD_VERSION: u16 = 1;
const CHANGE_LOG_CACHE_PREFILL_LIMIT: usize = 256;
const RUNTIME_STATE_VERSION: u16 = 1;
const RUNTIME_STATE_KEY: u32 = 0;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("heed error: {0}")]
    Heed(#[from] heed::Error),
    #[error("manifest worker terminated")]
    WorkerClosed,
    #[error("wait for generation timed out")]
    WaitTimeout,
    #[error("manifest commit failed: {0}")]
    CommitFailed(String),
    #[error("manifest invariant violated: {0}")]
    InvariantViolation(String),
    #[error("page cache serialization failed: {0}")]
    CacheSerialization(String),
}

#[derive(Debug, Clone)]
pub struct ManifestOptions {
    pub map_size: usize,
    pub max_dbs: u32,
    pub queue_capacity: usize,
    pub commit_latency: Duration,
    pub page_cache: Option<Arc<PageCache<PageCacheKey>>>,
    pub change_log_cache_object_id: Option<u64>,
}

impl Default for ManifestOptions {
    fn default() -> Self {
        Self {
            map_size: 256 * 1024 * 1024,
            max_dbs: 16,
            queue_capacity: 128,
            commit_latency: Duration::from_millis(5),
            page_cache: None,
            change_log_cache_object_id: None,
        }
    }
}

fn compute_change_log_cache_id(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeCursorStart {
    Oldest,
    Latest,
    Sequence(ChangeSequence),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeCursorSnapshot {
    pub cursor_id: ChangeCursorId,
    pub acked_sequence: ChangeSequence,
    pub next_sequence: ChangeSequence,
    pub latest_sequence: ChangeSequence,
    pub oldest_sequence: ChangeSequence,
}

#[derive(Debug, Clone)]
pub struct ChangeBatchPage {
    pub cursor_id: ChangeCursorId,
    pub changes: Vec<ManifestChangeRecord>,
    pub next_sequence: ChangeSequence,
    pub latest_sequence: ChangeSequence,
    pub oldest_sequence: ChangeSequence,
}

#[derive(Debug)]
pub struct Manifest {
    env: Env,
    tables: Arc<ManifestTables>,
    command_tx: SyncSender<ManifestCommand>,
    worker: WorkerHandle,
    diagnostics: Arc<ManifestDiagnostics>,
    generation_cache: Arc<Mutex<HashMap<ComponentId, Generation>>>,
    job_id_counter: Arc<AtomicU64>,
    change_state: Arc<Mutex<ChangeLogState>>,
    change_signal: Arc<ChangeSignal>,
    page_cache: Option<Arc<PageCache<PageCacheKey>>>,
    change_log_cache_object_id: Option<u64>,
    runtime_state: ManifestRuntimeState,
}

#[derive(Debug, Default)]
struct ManifestDiagnostics {
    committed_batches: AtomicU64,
}

#[derive(Debug)]
struct ManifestRuntimeState {
    key: u32,
    record: RuntimeStateRecord,
    crash_detected: bool,
}

impl ManifestRuntimeState {
    fn initialize(env: &Env, tables: &ManifestTables) -> Result<Self, ManifestError> {
        let record = RuntimeStateRecord {
            record_version: RUNTIME_STATE_VERSION,
            instance_id: Uuid::new_v4(),
            pid: process::id(),
            started_at_epoch_ms: epoch_millis(),
        };

        let crash_detected = {
            let mut txn = env.write_txn()?;
            let existing = tables.runtime_state.get(&txn, &RUNTIME_STATE_KEY)?;
            tables
                .runtime_state
                .put(&mut txn, &RUNTIME_STATE_KEY, &record)?;
            txn.commit()?;
            existing.is_some()
        };

        Ok(Self {
            key: RUNTIME_STATE_KEY,
            record,
            crash_detected,
        })
    }

    fn clear(&self, env: &Env, tables: &ManifestTables) -> Result<(), ManifestError> {
        let mut txn = env.write_txn()?;
        tables.runtime_state.delete(&mut txn, &self.key)?;
        txn.commit()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestDiagnosticsSnapshot {
    pub committed_batches: u64,
    pub page_cache_hits: u64,
    pub page_cache_misses: u64,
    pub page_cache_insertions: u64,
    pub page_cache_evictions: u64,
}

#[derive(Debug, Clone)]
struct ChangeLogState {
    next_sequence: ChangeSequence,
    oldest_sequence: ChangeSequence,
    cursors: HashMap<ChangeCursorId, ChangeCursorState>,
}

impl ChangeLogState {
    fn latest_sequence(&self) -> ChangeSequence {
        self.next_sequence.saturating_sub(1)
    }

    fn min_acked_sequence(&self) -> Option<ChangeSequence> {
        self.cursors
            .values()
            .map(|cursor| cursor.acked_sequence)
            .min()
    }

    fn snapshot_for_cursor(&self, cursor: &ChangeCursorState) -> ChangeCursorSnapshot {
        let oldest_sequence = self.oldest_sequence;
        let latest_sequence = self.latest_sequence();
        let mut next_sequence = cursor.acked_sequence.saturating_add(1);
        if next_sequence < oldest_sequence {
            next_sequence = oldest_sequence;
        }
        ChangeCursorSnapshot {
            cursor_id: cursor.cursor_id,
            acked_sequence: cursor.acked_sequence,
            next_sequence,
            latest_sequence,
            oldest_sequence,
        }
    }
}

#[derive(Debug, Clone)]
struct ChangeCursorState {
    cursor_id: ChangeCursorId,
    acked_sequence: ChangeSequence,
    created_at_epoch_ms: u64,
    updated_at_epoch_ms: u64,
}

struct ChangeSignal {
    last_sequence: Mutex<ChangeSequence>,
    condvar: Condvar,
}

impl ChangeSignal {
    fn new(initial: ChangeSequence) -> Self {
        Self {
            last_sequence: Mutex::new(initial),
            condvar: Condvar::new(),
        }
    }

    fn update(&self, latest: ChangeSequence) {
        let mut guard = self
            .last_sequence
            .lock()
            .expect("change signal mutex poisoned");
        if *guard < latest {
            *guard = latest;
            self.condvar.notify_all();
        }
    }

    fn current(&self) -> ChangeSequence {
        *self
            .last_sequence
            .lock()
            .expect("change signal mutex poisoned")
    }

    fn wait_for(
        &self,
        since: ChangeSequence,
        deadline: Option<Instant>,
    ) -> Result<ChangeSequence, ManifestError> {
        let mut guard = self
            .last_sequence
            .lock()
            .expect("change signal mutex poisoned");
        if *guard > since {
            return Ok(*guard);
        }

        loop {
            if let Some(limit) = deadline {
                let now = Instant::now();
                if now >= limit {
                    return Err(ManifestError::WaitTimeout);
                }
                let timeout = limit - now;
                let (next_guard, status) = self
                    .condvar
                    .wait_timeout(guard, timeout)
                    .expect("change signal mutex poisoned");
                guard = next_guard;
                if *guard > since {
                    return Ok(*guard);
                }
                if status.timed_out() {
                    return Err(ManifestError::WaitTimeout);
                }
            } else {
                guard = self
                    .condvar
                    .wait(guard)
                    .expect("change signal mutex poisoned");
                if *guard > since {
                    return Ok(*guard);
                }
            }
        }
    }
}

impl std::fmt::Debug for ChangeSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChangeSignal").finish()
    }
}

#[derive(Debug)]
struct WorkerHandle {
    join: Option<JoinHandle<()>>,
}

impl WorkerHandle {
    fn stop(&mut self) {
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug)]
pub(crate) struct ManifestTables {
    db: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<DbDescriptorRecord>>,
    wal_state: Database<U32<heed::byteorder::BigEndian>, SerdeBincode<WalStateRecord>>,
    chunk_catalog: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<ChunkEntryRecord>>,
    chunk_delta_index: Database<U128<heed::byteorder::BigEndian>, SerdeBincode<ChunkDeltaRecord>>,
    snapshot_index: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<SnapshotRecord>>,
    wal_catalog: Database<U128<heed::byteorder::BigEndian>, SerdeBincode<WalArtifactRecord>>,
    job_queue: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<JobRecord>>,
    job_pending_index: Database<U64<heed::byteorder::BigEndian>, U64<heed::byteorder::BigEndian>>,
    metrics: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<MetricRecord>>,
    generation_watermarks:
        Database<U64<heed::byteorder::BigEndian>, SerdeBincode<GenerationRecord>>,
    gc_refcounts: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<ChunkRefcountRecord>>,
    remote_namespaces:
        Database<U32<heed::byteorder::BigEndian>, SerdeBincode<RemoteNamespaceRecord>>,
    change_log: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<ManifestChangeRecord>>,
    change_cursors: Database<U64<heed::byteorder::BigEndian>, SerdeBincode<ManifestCursorRecord>>,
    runtime_state: Database<U32<heed::byteorder::BigEndian>, SerdeBincode<RuntimeStateRecord>>,
}

impl Manifest {
    pub fn open(path: impl AsRef<Path>, options: ManifestOptions) -> Result<Self, ManifestError> {
        let path_ref = path.as_ref();
        std::fs::create_dir_all(path_ref)?;

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(options.map_size)
                .max_dbs(options.max_dbs)
                .open(path_ref)?
        };

        let tables = {
            let mut txn = env.write_txn()?;
            let db = env.create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<DbDescriptorRecord>>(&mut txn, Some("db"))?;
            let wal_state = env
                .create_database::<U32<heed::byteorder::BigEndian>, SerdeBincode<WalStateRecord>>(
                    &mut txn,
                    Some("wal_state"),
                )?;
            let chunk_catalog = env
                .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<ChunkEntryRecord>>(
                    &mut txn,
                    Some("chunk_catalog"),
                )?;
            let chunk_delta_index = env.create_database::<U128<heed::byteorder::BigEndian>, SerdeBincode<ChunkDeltaRecord>>(
                &mut txn,
                Some("chunk_delta_index"),
            )?;
            let snapshot_index = env
                .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<SnapshotRecord>>(
                    &mut txn,
                    Some("snapshot_index"),
                )?;
            let wal_catalog = env.create_database::<U128<heed::byteorder::BigEndian>, SerdeBincode<WalArtifactRecord>>(
                &mut txn,
                Some("wal_catalog"),
            )?;
            let job_queue = env
                .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<JobRecord>>(
                    &mut txn,
                    Some("job_queue"),
                )?;
            let job_pending_index = env
                .create_database::<U64<heed::byteorder::BigEndian>, U64<heed::byteorder::BigEndian>>(
                    &mut txn,
                    Some("job_pending_index"),
                )?;
            let metrics = env
                .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<MetricRecord>>(
                    &mut txn,
                    Some("metrics"),
                )?;
            let generation_watermarks = env
                .create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<GenerationRecord>>(
                    &mut txn,
                    Some("generation_watermarks"),
                )?;
            let gc_refcounts = env.create_database::<U64<heed::byteorder::BigEndian>, SerdeBincode<ChunkRefcountRecord>>(&mut txn, Some("gc_refcounts"))?;
            let remote_namespaces = env.create_database::<U32<heed::byteorder::BigEndian>, SerdeBincode<RemoteNamespaceRecord>>(&mut txn, Some("remote_namespaces"))?;
            let change_log = env
                .create_database::<
                    U64<heed::byteorder::BigEndian>,
                    SerdeBincode<ManifestChangeRecord>,
                >(&mut txn, Some("change_log"))?;
            let change_cursors = env
                .create_database::<
                    U64<heed::byteorder::BigEndian>,
                    SerdeBincode<ManifestCursorRecord>,
                >(&mut txn, Some("change_cursors"))?;
            let runtime_state = env
                .create_database::<U32<heed::byteorder::BigEndian>, SerdeBincode<RuntimeStateRecord>>(
                    &mut txn,
                    Some("runtime_state"),
                )?;
            txn.commit()?;

            Arc::new(ManifestTables {
                db,
                wal_state,
                chunk_catalog,
                chunk_delta_index,
                snapshot_index,
                wal_catalog,
                job_queue,
                job_pending_index,
                metrics,
                generation_watermarks,
                gc_refcounts,
                remote_namespaces,
                change_log,
                change_cursors,
                runtime_state,
            })
        };

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

        let (command_tx, command_rx) = mpsc::sync_channel(options.queue_capacity.max(1));

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

    pub fn begin(&self) -> ManifestTxn<'_> {
        self.begin_with_capacity(16)
    }

    pub fn begin_with_capacity(&self, capacity: usize) -> ManifestTxn<'_> {
        ManifestTxn {
            manifest: self,
            ops: Vec::with_capacity(capacity),
        }
    }

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

    pub fn wait_for_change(
        &self,
        since: ChangeSequence,
        deadline: Option<Instant>,
    ) -> Result<ChangeSequence, ManifestError> {
        self.change_signal.wait_for(since, deadline)
    }

    pub fn latest_change_sequence(&self) -> ChangeSequence {
        self.change_signal.current()
    }

    fn change_log_cache(&self) -> Option<(Arc<PageCache<PageCacheKey>>, u64)> {
        self.page_cache.as_ref().and_then(|cache| {
            self.change_log_cache_object_id
                .map(|id| (cache.clone(), id))
        })
    }

    pub(crate) fn read<T, F>(&self, f: F) -> Result<T, ManifestError>
    where
        F: FnOnce(&ManifestTables, &RoTxn<'_>) -> Result<T, ManifestError>,
    {
        let txn = self.env.read_txn()?;
        let result = f(&self.tables, &txn)?;
        txn.commit()?;
        Ok(result)
    }

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

    pub fn reserve_job_id(&self) -> JobId {
        self.next_job_id()
    }

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

    pub fn wal_state(&self, db_id: DbId) -> Result<Option<WalStateRecord>, ManifestError> {
        self.read(|tables, txn| {
            let key = WalStateKey::new(db_id).encode();
            Ok(tables.wal_state.get(txn, &key)?)
        })
    }

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

    pub fn current_generation(&self, component: ComponentId) -> Option<Generation> {
        self.generation_cache
            .lock()
            .ok()
            .and_then(|map| map.get(&component).copied())
    }

    pub fn diagnostics(&self) -> ManifestDiagnosticsSnapshot {
        let mut snapshot = ManifestDiagnosticsSnapshot {
            committed_batches: self.diagnostics.committed_batches.load(Ordering::Relaxed),
            page_cache_hits: 0,
            page_cache_misses: 0,
            page_cache_insertions: 0,
            page_cache_evictions: 0,
        };

        if let Some(metrics) = self.page_cache_metrics() {
            snapshot.page_cache_hits = metrics.hits;
            snapshot.page_cache_misses = metrics.misses;
            snapshot.page_cache_insertions = metrics.insertions;
            snapshot.page_cache_evictions = metrics.evictions;
        }

        snapshot
    }

    pub fn page_cache_metrics(&self) -> Option<PageCacheMetricsSnapshot> {
        self.page_cache.as_ref().map(|cache| cache.metrics())
    }

    pub fn crash_detected(&self) -> bool {
        self.runtime_state.crash_detected
    }

    pub fn runtime_state(&self) -> &RuntimeStateRecord {
        &self.runtime_state.record
    }

    fn send_command(&self, command: ManifestCommand) -> Result<(), ManifestError> {
        self.command_tx
            .send(command)
            .map_err(|_| ManifestError::WorkerClosed)
    }

    fn next_job_id(&self) -> JobId {
        self.job_id_counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn current_generation_snapshot(&self) -> HashMap<ComponentId, Generation> {
        self.generation_cache
            .lock()
            .map(|map| map.clone())
            .unwrap_or_default()
    }

    fn change_state_snapshot(&self) -> Result<ChangeLogState, ManifestError> {
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

fn load_generations(
    env: &Env,
    tables: &ManifestTables,
) -> Result<HashMap<ComponentId, Generation>, ManifestError> {
    let txn = env.read_txn()?;
    let mut map = HashMap::new();
    {
        let mut iter = tables.generation_watermarks.iter(&txn)?;
        while let Some((component, record)) = iter.next().transpose()? {
            map.insert(component as ComponentId, record.current_generation);
        }
    }
    txn.commit()?;
    Ok(map)
}

struct ChangeStateBootstrap {
    state: ChangeLogState,
    next_cursor_id: ChangeCursorId,
}

fn load_change_state(
    env: &Env,
    tables: &ManifestTables,
) -> Result<ChangeStateBootstrap, ManifestError> {
    let txn = env.read_txn()?;
    let mut oldest_sequence: ChangeSequence = 1;
    let mut next_sequence: Option<ChangeSequence> = None;

    {
        let mut iter = tables.change_log.iter(&txn)?;
        if let Some((seq, _)) = iter.next().transpose()? {
            oldest_sequence = seq;
        }
    }

    {
        let mut rev_iter = tables.change_log.rev_iter(&txn)?;
        if let Some((seq, _)) = rev_iter.next().transpose()? {
            next_sequence = Some(seq.saturating_add(1));
        }
    }

    let mut cursors = HashMap::new();
    let mut max_cursor_id: ChangeCursorId = 0;
    let next_sequence = next_sequence.unwrap_or(oldest_sequence);
    let latest_sequence = next_sequence.saturating_sub(1);
    let min_sequence = oldest_sequence.saturating_sub(1);

    {
        let mut cursor_iter = tables.change_cursors.iter(&txn)?;
        while let Some((cursor_id_raw, record)) = cursor_iter.next().transpose()? {
            let cursor_id = cursor_id_raw as ChangeCursorId;
            let mut acked_sequence = record.acked_sequence;
            if acked_sequence > latest_sequence {
                acked_sequence = latest_sequence;
            }
            if acked_sequence < min_sequence {
                acked_sequence = min_sequence;
            }
            max_cursor_id = max_cursor_id.max(cursor_id);
            cursors.insert(
                cursor_id,
                ChangeCursorState {
                    cursor_id,
                    acked_sequence,
                    created_at_epoch_ms: record.created_at_epoch_ms,
                    updated_at_epoch_ms: record.updated_at_epoch_ms,
                },
            );
        }
    }

    txn.commit()?;

    let state = ChangeLogState {
        next_sequence: next_sequence.max(1),
        oldest_sequence: oldest_sequence.max(1),
        cursors,
    };

    Ok(ChangeStateBootstrap {
        state,
        next_cursor_id: max_cursor_id.saturating_add(1).max(1),
    })
}

fn apply_startup_truncation(
    env: &Env,
    tables: &ManifestTables,
    change_state: &mut ChangeLogState,
) -> Result<(), ManifestError> {
    let truncate_before = change_state
        .min_acked_sequence()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(change_state.oldest_sequence);

    if truncate_before <= change_state.oldest_sequence {
        return Ok(());
    }

    let mut txn = env.write_txn()?;
    let mut current = change_state.oldest_sequence;
    while current < truncate_before {
        tables.change_log.delete(&mut txn, &current)?;
        current = current.saturating_add(1);
    }
    txn.commit()?;

    change_state.oldest_sequence = truncate_before;
    Ok(())
}

fn hydrate_change_log_cache(
    env: &Env,
    tables: &ManifestTables,
    cache: &Arc<PageCache<PageCacheKey>>,
    object_id: u64,
    oldest_sequence: ChangeSequence,
    latest_sequence: ChangeSequence,
) -> Result<(), ManifestError> {
    if latest_sequence < oldest_sequence || CHANGE_LOG_CACHE_PREFILL_LIMIT == 0 {
        return Ok(());
    }

    let available = latest_sequence
        .saturating_sub(oldest_sequence)
        .saturating_add(1);
    let start = if available > CHANGE_LOG_CACHE_PREFILL_LIMIT as u64 {
        latest_sequence
            .saturating_sub(CHANGE_LOG_CACHE_PREFILL_LIMIT as u64)
            .saturating_add(1)
    } else {
        oldest_sequence
    };

    let txn = env.read_txn()?;
    let config = bincode::config::standard();
    let mut sequence = start;
    while sequence <= latest_sequence {
        if let Some(record) = tables.change_log.get(&txn, &sequence)? {
            if let Ok(bytes) = encode_to_vec(&record, config) {
                cache.insert(
                    PageCacheKey::manifest(object_id, sequence),
                    Arc::from(bytes.into_boxed_slice()),
                );
            }
        }
        sequence = sequence.saturating_add(1);
    }
    txn.commit()?;
    Ok(())
}

struct ForkData {
    chunk_entries: Vec<(ChunkKey, ChunkEntryRecord)>,
    chunk_deltas: Vec<(ChunkDeltaKey, ChunkDeltaRecord)>,
    wal_states: Vec<(WalStateKey, WalStateRecord)>,
    wal_artifacts: Vec<(WalArtifactKey, WalArtifactRecord)>,
}

#[derive(Debug)]
pub struct ManifestTxn<'a> {
    manifest: &'a Manifest,
    ops: Vec<ManifestOp>,
}

impl<'a> ManifestTxn<'a> {
    pub fn put_db(&mut self, db_id: DbId, value: DbDescriptorRecord) -> &mut Self {
        self.ops.push(ManifestOp::PutDb { db_id, value });
        self
    }

    pub fn delete_db(&mut self, db_id: DbId) -> &mut Self {
        self.ops.push(ManifestOp::DeleteDb { db_id });
        self
    }

    pub fn put_wal_state(&mut self, key: WalStateKey, value: WalStateRecord) -> &mut Self {
        self.ops.push(ManifestOp::PutWalState { key, value });
        self
    }

    pub fn delete_wal_state(&mut self, key: WalStateKey) -> &mut Self {
        self.ops.push(ManifestOp::DeleteWalState { key });
        self
    }

    pub fn upsert_chunk(&mut self, key: ChunkKey, value: ChunkEntryRecord) -> &mut Self {
        self.ops.push(ManifestOp::UpsertChunk { key, value });
        self
    }

    pub fn delete_chunk(&mut self, key: ChunkKey) -> &mut Self {
        self.ops.push(ManifestOp::DeleteChunk { key });
        self
    }

    pub fn upsert_chunk_delta(&mut self, key: ChunkDeltaKey, value: ChunkDeltaRecord) -> &mut Self {
        self.ops.push(ManifestOp::UpsertChunkDelta { key, value });
        self
    }

    pub fn delete_chunk_delta(&mut self, key: ChunkDeltaKey) -> &mut Self {
        self.ops.push(ManifestOp::DeleteChunkDelta { key });
        self
    }

    pub fn publish_snapshot(&mut self, record: SnapshotRecord) -> &mut Self {
        self.ops.push(ManifestOp::PublishSnapshot { record });
        self
    }

    pub fn drop_snapshot(&mut self, key: SnapshotKey) -> &mut Self {
        self.ops.push(ManifestOp::DropSnapshot { key });
        self
    }

    pub fn register_wal_artifact(
        &mut self,
        key: WalArtifactKey,
        record: WalArtifactRecord,
    ) -> &mut Self {
        self.ops.push(ManifestOp::UpsertWalArtifact { key, record });
        self
    }

    pub fn remove_wal_artifact(&mut self, key: WalArtifactKey) -> &mut Self {
        self.ops.push(ManifestOp::DeleteWalArtifact { key });
        self
    }

    pub fn enqueue_job(
        &mut self,
        db_id: Option<DbId>,
        kind: JobKind,
        payload: JobPayload,
    ) -> JobId {
        let job_id = self.manifest.next_job_id();
        let now = epoch_millis();
        let record = JobRecord {
            record_version: JOB_RECORD_VERSION,
            job_id,
            created_at_epoch_ms: now,
            db_id,
            kind,
            state: JobDurableState::Pending {
                enqueued_at_epoch_ms: now,
            },
            payload,
        };
        self.ops.push(ManifestOp::PutJob { record });
        self.ops.push(ManifestOp::PersistJobCounter {
            next: job_id,
            timestamp_ms: now,
        });
        job_id
    }

    pub fn update_job_state(&mut self, job_id: JobId, new_state: JobDurableState) -> &mut Self {
        self.ops
            .push(ManifestOp::UpdateJobState { job_id, new_state });
        self
    }

    pub fn remove_job(&mut self, job_id: JobId) -> &mut Self {
        self.ops.push(ManifestOp::RemoveJob { job_id });
        self
    }

    pub fn upsert_pending_job(&mut self, key: PendingJobKey, job_id: JobId) -> &mut Self {
        self.ops.push(ManifestOp::UpsertPendingJob { key, job_id });
        self
    }

    pub fn delete_pending_job(&mut self, key: PendingJobKey) -> &mut Self {
        self.ops.push(ManifestOp::DeletePendingJob { key });
        self
    }

    pub fn merge_metric(&mut self, key: MetricKey, delta: MetricDelta) -> &mut Self {
        self.ops.push(ManifestOp::MergeMetric { key, delta });
        self
    }

    pub fn adjust_refcount(&mut self, chunk_id: ChunkId, delta: i64) -> &mut Self {
        if delta != 0 {
            self.ops
                .push(ManifestOp::AdjustRefcount { chunk_id, delta });
        }
        self
    }

    pub fn bump_generation(&mut self, component: ComponentId, increment: u64) -> &mut Self {
        if increment > 0 {
            self.ops.push(ManifestOp::BumpGeneration {
                component,
                increment,
                timestamp_ms: epoch_millis(),
            });
        }
        self
    }

    pub fn commit(mut self) -> Result<CommitReceipt, ManifestError> {
        if self.ops.is_empty() {
            return Ok(CommitReceipt {
                generations: self.manifest.current_generation_snapshot(),
            });
        }

        let batch = ManifestBatch {
            ops: self.ops.drain(..).collect(),
        };
        let (tx, rx) = mpsc::sync_channel(0);
        self.manifest.send_command(ManifestCommand::Apply {
            batch,
            completion: tx,
        })?;
        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err(ManifestError::WorkerClosed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommitReceipt {
    pub generations: HashMap<ComponentId, Generation>,
}
#[derive(Debug)]
struct WaitEntry {
    target: Generation,
    deadline: Instant,
    responder: SyncSender<Result<(), ManifestError>>,
}

#[derive(Debug)]
enum ManifestCommand {
    Apply {
        batch: ManifestBatch,
        completion: SyncSender<Result<CommitReceipt, ManifestError>>,
    },
    Wait(WaitRequest),
    RegisterCursor(CursorRegistrationRequest),
    AcknowledgeCursor(CursorAckRequest),
    Shutdown,
}

#[derive(Debug)]
struct CursorRegistrationRequest {
    start: ChangeCursorStart,
    responder: SyncSender<Result<ChangeCursorSnapshot, ManifestError>>,
}

#[derive(Debug)]
struct CursorAckRequest {
    cursor_id: ChangeCursorId,
    sequence: ChangeSequence,
    responder: SyncSender<Result<ChangeCursorSnapshot, ManifestError>>,
}

#[derive(Debug)]
struct ManifestBatch {
    ops: Vec<ManifestOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManifestOp {
    PutDb {
        db_id: DbId,
        value: DbDescriptorRecord,
    },
    DeleteDb {
        db_id: DbId,
    },
    PutWalState {
        key: WalStateKey,
        value: WalStateRecord,
    },
    DeleteWalState {
        key: WalStateKey,
    },
    UpsertChunk {
        key: ChunkKey,
        value: ChunkEntryRecord,
    },
    DeleteChunk {
        key: ChunkKey,
    },
    UpsertChunkDelta {
        key: ChunkDeltaKey,
        value: ChunkDeltaRecord,
    },
    DeleteChunkDelta {
        key: ChunkDeltaKey,
    },
    PublishSnapshot {
        record: SnapshotRecord,
    },
    DropSnapshot {
        key: SnapshotKey,
    },
    UpsertWalArtifact {
        key: WalArtifactKey,
        record: WalArtifactRecord,
    },
    DeleteWalArtifact {
        key: WalArtifactKey,
    },
    PutJob {
        record: JobRecord,
    },
    UpdateJobState {
        job_id: JobId,
        new_state: JobDurableState,
    },
    RemoveJob {
        job_id: JobId,
    },
    UpsertPendingJob {
        key: PendingJobKey,
        job_id: JobId,
    },
    DeletePendingJob {
        key: PendingJobKey,
    },
    MergeMetric {
        key: MetricKey,
        delta: MetricDelta,
    },
    AdjustRefcount {
        chunk_id: ChunkId,
        delta: i64,
    },
    BumpGeneration {
        component: ComponentId,
        increment: u64,
        timestamp_ms: u64,
    },
    PersistJobCounter {
        next: JobId,
        timestamp_ms: u64,
    },
}

#[derive(Debug)]
struct WaitRequest {
    component: ComponentId,
    target: Generation,
    deadline: Instant,
    responder: SyncSender<Result<(), ManifestError>>,
}
fn worker_loop(
    env: Env,
    tables: Arc<ManifestTables>,
    diagnostics: Arc<ManifestDiagnostics>,
    generation_cache: Arc<Mutex<HashMap<ComponentId, Generation>>>,
    job_id_counter: Arc<AtomicU64>,
    change_state_shared: Arc<Mutex<ChangeLogState>>,
    change_signal: Arc<ChangeSignal>,
    cursor_id_counter: Arc<AtomicU64>,
    command_rx: Receiver<ManifestCommand>,
    mut generations: HashMap<ComponentId, Generation>,
    mut change_state: ChangeLogState,
    commit_latency: Duration,
    page_cache: Option<Arc<PageCache<PageCacheKey>>>,
    page_cache_object_id: Option<u64>,
) {
    let mut waiters: HashMap<ComponentId, VecDeque<WaitEntry>> = HashMap::new();
    let mut pending: Vec<(
        ManifestBatch,
        SyncSender<Result<CommitReceipt, ManifestError>>,
    )> = Vec::new();
    let mut deadline: Option<Instant> = None;
    let mut shutdown = false;
    let mut persisted_job_counter = generations.get(&JOB_ID_COMPONENT).copied().unwrap_or(0);

    loop {
        if shutdown && pending.is_empty() {
            break;
        }

        let command = if let Some(limit) = deadline {
            let now = Instant::now();
            if now >= limit {
                None
            } else {
                match command_rx.recv_timeout(limit - now) {
                    Ok(command) => Some(command),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        shutdown = true;
                        None
                    }
                }
            }
        } else {
            match command_rx.recv() {
                Ok(command) => Some(command),
                Err(_) => {
                    shutdown = true;
                    None
                }
            }
        };

        if let Some(command) = command {
            match command {
                ManifestCommand::Apply { batch, completion } => {
                    pending.push((batch, completion));
                    if deadline.is_none() {
                        deadline = Some(Instant::now() + commit_latency);
                    }
                }
                ManifestCommand::Wait(request) => {
                    handle_wait_request(&mut waiters, request, &generations);
                }
                ManifestCommand::RegisterCursor(request) => {
                    let result = register_cursor(
                        &env,
                        &tables,
                        &mut change_state,
                        &cursor_id_counter,
                        request.start,
                    );
                    match result {
                        Ok(snapshot) => {
                            if let Ok(mut shared_state) = change_state_shared.lock() {
                                *shared_state = change_state.clone();
                            }
                            let _ = request.responder.send(Ok(snapshot));
                        }
                        Err(err) => {
                            let _ = request.responder.send(Err(err));
                        }
                    }
                }
                ManifestCommand::AcknowledgeCursor(request) => {
                    let result = acknowledge_cursor(
                        &env,
                        &tables,
                        &mut change_state,
                        request.cursor_id,
                        request.sequence,
                    );
                    match result {
                        Ok(snapshot) => {
                            if let Ok(mut shared_state) = change_state_shared.lock() {
                                *shared_state = change_state.clone();
                            }
                            let _ = request.responder.send(Ok(snapshot));
                        }
                        Err(err) => {
                            let _ = request.responder.send(Err(err));
                        }
                    }
                }
                ManifestCommand::Shutdown => {
                    shutdown = true;
                }
            }
        }

        expire_waiters(&mut waiters, &generations);

        let should_commit = !pending.is_empty()
            && (shutdown || deadline.map_or(false, |limit| Instant::now() >= limit));

        if should_commit {
            match commit_pending(
                &env,
                &tables,
                &mut pending,
                &mut generations,
                &mut persisted_job_counter,
                &diagnostics,
                &mut change_state,
                page_cache.as_ref(),
                page_cache_object_id,
            ) {
                Ok(snapshots) => {
                    for ((_, completion), snapshot) in pending.iter().zip(snapshots.into_iter()) {
                        let _ = completion.send(Ok(CommitReceipt {
                            generations: snapshot,
                        }));
                    }
                    pending.clear();
                    deadline = None;
                    if let Ok(mut cache) = generation_cache.lock() {
                        *cache = generations.clone();
                    }
                    if let Ok(mut shared_state) = change_state_shared.lock() {
                        *shared_state = change_state.clone();
                    }
                    change_signal.update(change_state.latest_sequence());
                    job_id_counter.store(persisted_job_counter, Ordering::SeqCst);
                    expire_waiters(&mut waiters, &generations);
                }
                Err(err) => {
                    let err_msg = format!("{err}");
                    for (_, completion) in pending.drain(..) {
                        let _ = completion.send(Err(ManifestError::CommitFailed(err_msg.clone())));
                    }
                    break;
                }
            }
        }

        if pending.is_empty() {
            deadline = None;
        }
    }

    for queue in waiters.values_mut() {
        while let Some(entry) = queue.pop_front() {
            let _ = entry.responder.send(Err(ManifestError::WorkerClosed));
        }
    }
    while let Ok(ManifestCommand::Wait(request)) = command_rx.try_recv() {
        let _ = request.responder.send(Err(ManifestError::WorkerClosed));
    }
}

fn handle_wait_request(
    waiters: &mut HashMap<ComponentId, VecDeque<WaitEntry>>,
    request: WaitRequest,
    generations: &HashMap<ComponentId, Generation>,
) {
    let current = generations.get(&request.component).copied().unwrap_or(0);
    if current >= request.target {
        let _ = request.responder.send(Ok(()));
        return;
    }

    let queue = waiters.entry(request.component).or_default();
    let mut index = queue.len();
    for (idx, entry) in queue.iter().enumerate() {
        if request.target < entry.target {
            index = idx;
            break;
        }
    }
    queue.insert(
        index,
        WaitEntry {
            target: request.target,
            deadline: request.deadline,
            responder: request.responder,
        },
    );
}

fn expire_waiters(
    waiters: &mut HashMap<ComponentId, VecDeque<WaitEntry>>,
    generations: &HashMap<ComponentId, Generation>,
) {
    let now = Instant::now();
    waiters.retain(|component, queue| {
        while let Some(entry) = queue.front() {
            let current = generations.get(component).copied().unwrap_or(0);
            if current >= entry.target {
                let entry = queue.pop_front().unwrap();
                let _ = entry.responder.send(Ok(()));
            } else if now >= entry.deadline {
                let entry = queue.pop_front().unwrap();
                let _ = entry.responder.send(Err(ManifestError::WaitTimeout));
            } else {
                break;
            }
        }
        !queue.is_empty()
    });
}
fn commit_pending(
    env: &Env,
    tables: &ManifestTables,
    pending: &mut Vec<(
        ManifestBatch,
        SyncSender<Result<CommitReceipt, ManifestError>>,
    )>,
    generations: &mut HashMap<ComponentId, Generation>,
    persisted_job_counter: &mut JobId,
    diagnostics: &ManifestDiagnostics,
    change_state: &mut ChangeLogState,
    page_cache: Option<&Arc<PageCache<PageCacheKey>>>,
    page_cache_object_id: Option<u64>,
) -> Result<Vec<HashMap<ComponentId, Generation>>, heed::Error> {
    let mut txn = env.write_txn()?;
    let mut snapshots = Vec::with_capacity(pending.len());
    let mut working_generations = generations.clone();
    let config = bincode::config::standard();

    for (batch, _) in pending.iter() {
        let before_generations = working_generations.clone();
        apply_batch(
            &mut txn,
            tables,
            batch,
            &mut working_generations,
            persisted_job_counter,
        )?;
        let sequence = change_state.next_sequence;
        change_state.next_sequence = change_state.next_sequence.saturating_add(1);
        if change_state.oldest_sequence == 0 || change_state.oldest_sequence > sequence {
            change_state.oldest_sequence = sequence;
        }
        let generation_updates =
            compute_generation_updates(&before_generations, &working_generations);
        let change_record = ManifestChangeRecord {
            record_version: CHANGE_RECORD_VERSION,
            sequence,
            committed_at_epoch_ms: epoch_millis(),
            operations: batch.ops.clone(),
            generation_updates,
        };
        tables.change_log.put(&mut txn, &sequence, &change_record)?;
        if let (Some(cache), Some(object_id)) = (page_cache, page_cache_object_id) {
            if let Ok(bytes) = encode_to_vec(&change_record, config) {
                cache.insert(
                    PageCacheKey::manifest(object_id, sequence),
                    Arc::from(bytes.into_boxed_slice()),
                );
            }
        }
        snapshots.push(working_generations.clone());
    }

    txn.commit()?;
    diagnostics
        .committed_batches
        .fetch_add(1, Ordering::Relaxed);
    *generations = working_generations;

    Ok(snapshots)
}

fn register_cursor(
    env: &Env,
    tables: &ManifestTables,
    change_state: &mut ChangeLogState,
    cursor_id_counter: &Arc<AtomicU64>,
    start: ChangeCursorStart,
) -> Result<ChangeCursorSnapshot, ManifestError> {
    let cursor_id = cursor_id_counter.fetch_add(1, Ordering::SeqCst);
    let min_sequence = change_state.oldest_sequence.saturating_sub(1);
    let latest_sequence = change_state.latest_sequence();
    let desired = match start {
        ChangeCursorStart::Oldest => min_sequence,
        ChangeCursorStart::Latest => latest_sequence,
        ChangeCursorStart::Sequence(seq) => seq.saturating_sub(1),
    };
    let acked_sequence = clamp_sequence(desired, min_sequence, latest_sequence);
    let now = epoch_millis();
    let record = ManifestCursorRecord {
        record_version: CURSOR_RECORD_VERSION,
        cursor_id,
        acked_sequence,
        created_at_epoch_ms: now,
        updated_at_epoch_ms: now,
    };

    {
        let mut txn = env.write_txn()?;
        tables.change_cursors.put(&mut txn, &cursor_id, &record)?;
        txn.commit()?;
    }

    let cursor_state = ChangeCursorState {
        cursor_id,
        acked_sequence,
        created_at_epoch_ms: now,
        updated_at_epoch_ms: now,
    };
    change_state.cursors.insert(cursor_id, cursor_state.clone());

    Ok(change_state.snapshot_for_cursor(&cursor_state))
}

fn acknowledge_cursor(
    env: &Env,
    tables: &ManifestTables,
    change_state: &mut ChangeLogState,
    cursor_id: ChangeCursorId,
    sequence: ChangeSequence,
) -> Result<ChangeCursorSnapshot, ManifestError> {
    let existing_state = change_state
        .cursors
        .get(&cursor_id)
        .cloned()
        .ok_or_else(|| {
            ManifestError::InvariantViolation(format!("cursor {cursor_id} not registered"))
        })?;

    let min_sequence = change_state.oldest_sequence.saturating_sub(1);
    let latest_sequence = change_state.latest_sequence();
    let desired = sequence;
    let ack_target = clamp_sequence(desired, min_sequence, latest_sequence);

    if ack_target <= existing_state.acked_sequence {
        return Ok(change_state.snapshot_for_cursor(change_state.cursors.get(&cursor_id).unwrap()));
    }

    let now = epoch_millis();
    let mut updated_entry = existing_state.clone();
    updated_entry.acked_sequence = ack_target;
    updated_entry.updated_at_epoch_ms = now;

    let mut projected_state = change_state.clone();
    if let Some(cursor) = projected_state.cursors.get_mut(&cursor_id) {
        cursor.acked_sequence = ack_target;
        cursor.updated_at_epoch_ms = now;
    }

    let truncate_before = projected_state
        .min_acked_sequence()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(projected_state.oldest_sequence);

    {
        let mut txn = env.write_txn()?;
        let record = ManifestCursorRecord {
            record_version: CURSOR_RECORD_VERSION,
            cursor_id,
            acked_sequence: ack_target,
            created_at_epoch_ms: updated_entry.created_at_epoch_ms,
            updated_at_epoch_ms: now,
        };
        tables.change_cursors.put(&mut txn, &cursor_id, &record)?;

        if truncate_before > projected_state.oldest_sequence {
            let mut current = projected_state.oldest_sequence;
            while current < truncate_before {
                tables.change_log.delete(&mut txn, &current)?;
                current = current.saturating_add(1);
            }
        }

        txn.commit()?;
    }

    change_state
        .cursors
        .insert(cursor_id, updated_entry.clone());
    if truncate_before > change_state.oldest_sequence {
        change_state.oldest_sequence = truncate_before;
    }

    Ok(change_state.snapshot_for_cursor(&updated_entry))
}

fn clamp_sequence(
    value: ChangeSequence,
    min: ChangeSequence,
    max: ChangeSequence,
) -> ChangeSequence {
    let mut clamped = value;
    if clamped < min {
        clamped = min;
    }
    if clamped > max {
        clamped = max;
    }
    clamped
}

fn compute_generation_updates(
    before: &HashMap<ComponentId, Generation>,
    after: &HashMap<ComponentId, Generation>,
) -> Vec<ComponentGeneration> {
    after
        .iter()
        .filter_map(|(component, generation)| {
            if before.get(component) != Some(generation) {
                Some(ComponentGeneration {
                    component: *component,
                    generation: *generation,
                })
            } else {
                None
            }
        })
        .collect()
}

fn apply_batch(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    batch: &ManifestBatch,
    generations: &mut HashMap<ComponentId, Generation>,
    persisted_job_counter: &mut JobId,
) -> Result<(), heed::Error> {
    for op in &batch.ops {
        match op {
            ManifestOp::PutDb { db_id, value } => {
                tables.db.put(txn, &(*db_id as u64), value)?;
            }
            ManifestOp::DeleteDb { db_id } => {
                tables.db.delete(txn, &(*db_id as u64))?;
            }
            ManifestOp::PutWalState { key, value } => {
                let key = key.encode();
                tables.wal_state.put(txn, &key, value)?;
            }
            ManifestOp::DeleteWalState { key } => {
                let key = key.encode();
                tables.wal_state.delete(txn, &key)?;
            }
            ManifestOp::UpsertChunk { key, value } => {
                let key = key.encode();
                tables.chunk_catalog.put(txn, &key, value)?;
            }
            ManifestOp::DeleteChunk { key } => {
                let key = key.encode();
                tables.chunk_catalog.delete(txn, &key)?;
            }
            ManifestOp::UpsertChunkDelta { key, value } => {
                let key = key.encode();
                tables.chunk_delta_index.put(txn, &key, value)?;
            }
            ManifestOp::DeleteChunkDelta { key } => {
                let key = key.encode();
                tables.chunk_delta_index.delete(txn, &key)?;
            }
            ManifestOp::PublishSnapshot { record } => {
                publish_snapshot(txn, tables, record.clone())?;
            }
            ManifestOp::DropSnapshot { key } => {
                drop_snapshot(txn, tables, *key)?;
            }
            ManifestOp::UpsertWalArtifact { key, record } => {
                let key = key.encode();
                tables.wal_catalog.put(txn, &key, record)?;
            }
            ManifestOp::DeleteWalArtifact { key } => {
                let key = key.encode();
                tables.wal_catalog.delete(txn, &key)?;
            }
            ManifestOp::PutJob { record } => {
                tables.job_queue.put(txn, &record.job_id, record)?;
            }
            ManifestOp::UpdateJobState { job_id, new_state } => {
                if let Some(mut record) = tables.job_queue.get(txn, job_id)? {
                    record.state = new_state.clone();
                    tables.job_queue.put(txn, job_id, &record)?;
                }
            }
            ManifestOp::RemoveJob { job_id } => {
                tables.job_queue.delete(txn, job_id)?;
            }
            ManifestOp::UpsertPendingJob { key, job_id } => {
                let key = key.encode();
                tables.job_pending_index.put(txn, &key, job_id)?;
            }
            ManifestOp::DeletePendingJob { key } => {
                let key = key.encode();
                tables.job_pending_index.delete(txn, &key)?;
            }
            ManifestOp::MergeMetric { key, delta } => {
                merge_metric(txn, tables, *key, delta.clone())?;
            }
            ManifestOp::AdjustRefcount { chunk_id, delta } => {
                adjust_refcount(txn, tables, *chunk_id, *delta)?;
            }
            ManifestOp::BumpGeneration {
                component,
                increment,
                timestamp_ms,
            } => {
                bump_generation(
                    txn,
                    tables,
                    *component,
                    *increment,
                    *timestamp_ms,
                    generations,
                )?;
            }
            ManifestOp::PersistJobCounter { next, timestamp_ms } => {
                persist_job_counter(
                    txn,
                    tables,
                    *next,
                    *timestamp_ms,
                    generations,
                    persisted_job_counter,
                )?;
            }
        }
    }

    Ok(())
}

fn publish_snapshot(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    record: SnapshotRecord,
) -> Result<(), heed::Error> {
    let key = record.key().encode();
    for entry in &record.chunks {
        adjust_refcount(txn, tables, entry.chunk_id, 1)?;
    }
    tables.snapshot_index.put(txn, &key, &record)
}

fn drop_snapshot(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    key: SnapshotKey,
) -> Result<(), heed::Error> {
    let key = key.encode();
    if let Some(record) = tables.snapshot_index.get(txn, &key)? {
        for entry in &record.chunks {
            adjust_refcount(txn, tables, entry.chunk_id, -1)?;
        }
        tables.snapshot_index.delete(txn, &key)?;
    }
    Ok(())
}

fn merge_metric(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    key: MetricKey,
    delta: MetricDelta,
) -> Result<(), heed::Error> {
    let key = key.encode();
    let mut record = tables
        .metrics
        .get(txn, &key)?
        .unwrap_or_else(|| MetricRecord {
            record_version: METRIC_RECORD_VERSION,
            count: 0,
            sum: 0.0,
            min: None,
            max: None,
        });

    record.count = record.count.saturating_add(delta.count);
    record.sum += delta.sum;
    record.min = match (record.min, delta.min) {
        (Some(current), Some(other)) => Some(current.min(other)),
        (Some(current), None) => Some(current),
        (None, other) => other,
    };
    record.max = match (record.max, delta.max) {
        (Some(current), Some(other)) => Some(current.max(other)),
        (Some(current), None) => Some(current),
        (None, other) => other,
    };

    tables.metrics.put(txn, &key, &record)
}

fn adjust_refcount(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    chunk_id: ChunkId,
    delta: i64,
) -> Result<(), heed::Error> {
    if delta == 0 {
        return Ok(());
    }

    let key = chunk_id as u64;
    let mut record = tables
        .gc_refcounts
        .get(txn, &key)?
        .unwrap_or_else(|| ChunkRefcountRecord {
            record_version: CHUNK_REFCOUNT_VERSION,
            chunk_id,
            strong: 0,
        });

    let new_value = record.strong as i64 + delta;
    if new_value <= 0 {
        tables.gc_refcounts.delete(txn, &key)?;
    } else {
        record.strong = new_value as u64;
        tables.gc_refcounts.put(txn, &key, &record)?;
    }

    Ok(())
}

fn bump_generation(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    component: ComponentId,
    increment: u64,
    timestamp_ms: u64,
    generations: &mut HashMap<ComponentId, Generation>,
) -> Result<(), heed::Error> {
    if increment == 0 {
        return Ok(());
    }

    let key = component as u64;
    let mut record = tables
        .generation_watermarks
        .get(txn, &key)?
        .unwrap_or_else(|| GenerationRecord {
            record_version: GENERATION_RECORD_VERSION,
            component,
            current_generation: 0,
            updated_at_epoch_ms: timestamp_ms,
        });
    record.current_generation = record.current_generation.saturating_add(increment);
    record.updated_at_epoch_ms = timestamp_ms;
    tables.generation_watermarks.put(txn, &key, &record)?;
    generations.insert(component, record.current_generation);
    Ok(())
}

fn persist_job_counter(
    txn: &mut RwTxn<'_>,
    tables: &ManifestTables,
    next: JobId,
    timestamp_ms: u64,
    generations: &mut HashMap<ComponentId, Generation>,
    persisted_job_counter: &mut JobId,
) -> Result<(), heed::Error> {
    let key = JOB_ID_COMPONENT as u64;
    let mut record = tables
        .generation_watermarks
        .get(txn, &key)?
        .unwrap_or_else(|| GenerationRecord {
            record_version: GENERATION_RECORD_VERSION,
            component: JOB_ID_COMPONENT,
            current_generation: 0,
            updated_at_epoch_ms: timestamp_ms,
        });

    if record.current_generation < next {
        record.current_generation = next;
        record.updated_at_epoch_ms = timestamp_ms;
        tables.generation_watermarks.put(txn, &key, &record)?;
        generations.insert(JOB_ID_COMPONENT, record.current_generation);
        *persisted_job_counter = (*persisted_job_counter).max(next);
    }

    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WalStateKey {
    pub db_id: DbId,
}

impl WalStateKey {
    pub fn new(db_id: DbId) -> Self {
        Self { db_id }
    }

    fn encode(&self) -> u32 {
        self.db_id
    }

    fn decode(raw: u32) -> Self {
        Self { db_id: raw }
    }

    fn with_db(self, db_id: DbId) -> Self {
        Self { db_id, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkKey {
    pub db_id: DbId,
    pub chunk_id: ChunkId,
}

impl ChunkKey {
    pub fn new(db_id: DbId, chunk_id: ChunkId) -> Self {
        Self { db_id, chunk_id }
    }

    fn encode(&self) -> u64 {
        ((self.db_id as u64) << 32) | (self.chunk_id as u64)
    }

    fn decode(raw: u64) -> Self {
        Self {
            db_id: (raw >> 32) as DbId,
            chunk_id: raw as ChunkId,
        }
    }

    fn with_db(self, db_id: DbId) -> Self {
        Self { db_id, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkDeltaKey {
    pub db_id: DbId,
    pub chunk_id: ChunkId,
    pub generation: u32,
}

impl ChunkDeltaKey {
    pub fn new(db_id: DbId, chunk_id: ChunkId, generation: u32) -> Self {
        Self {
            db_id,
            chunk_id,
            generation,
        }
    }

    fn encode(&self) -> u128 {
        ((self.db_id as u128) << 96)
            | ((self.chunk_id as u128) << 64)
            | ((self.generation as u128) << 32)
    }

    fn decode(raw: u128) -> Self {
        Self {
            db_id: (raw >> 96) as DbId,
            chunk_id: ((raw >> 64) & 0xffff_ffff) as ChunkId,
            generation: ((raw >> 32) & 0xffff_ffff) as u32,
        }
    }

    fn with_db(self, db_id: DbId) -> Self {
        Self { db_id, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotKey {
    pub db_id: DbId,
    pub snapshot_id: SnapshotId,
}

impl SnapshotKey {
    pub fn new(db_id: DbId, snapshot_id: SnapshotId) -> Self {
        Self { db_id, snapshot_id }
    }

    fn encode(&self) -> u64 {
        ((self.db_id as u64) << 32) | (self.snapshot_id as u64)
    }

    fn decode(raw: u64) -> Self {
        Self {
            db_id: (raw >> 32) as DbId,
            snapshot_id: raw as SnapshotId,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingJobKey {
    pub db_id: DbId,
    pub job_kind: u16,
}

impl PendingJobKey {
    pub fn new(db_id: DbId, job_kind: u16) -> Self {
        Self { db_id, job_kind }
    }

    fn encode(&self) -> u64 {
        ((self.db_id as u64) << 32) | (self.job_kind as u64)
    }

    fn decode(raw: u64) -> Self {
        Self {
            db_id: (raw >> 32) as DbId,
            job_kind: raw as u16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MetricKey {
    pub scope: u32,
    pub metric_kind: u32,
}

impl MetricKey {
    pub fn new(scope: u32, metric_kind: u32) -> Self {
        Self { scope, metric_kind }
    }

    fn encode(&self) -> u64 {
        ((self.scope as u64) << 32) | (self.metric_kind as u64)
    }

    fn decode(raw: u64) -> Self {
        Self {
            scope: (raw >> 32) as u32,
            metric_kind: raw as u32,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DbDescriptorRecord {
    pub record_version: u16,
    pub created_at_epoch_ms: u64,
    pub name: String,
    pub config_hash: [u8; 32],
    pub wal_shards: u16,
    pub options: ManifestDbOptions,
    pub status: DbLifecycle,
}

impl Default for DbDescriptorRecord {
    fn default() -> Self {
        Self {
            record_version: DB_DESCRIPTOR_VERSION,
            created_at_epoch_ms: epoch_millis(),
            name: String::new(),
            config_hash: [0; 32],
            wal_shards: 1,
            options: ManifestDbOptions::default(),
            status: DbLifecycle::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestDbOptions {
    pub wal_dir: PathBuf,
    pub chunk_dir: PathBuf,
    pub compression: CompressionConfig,
    pub encryption: Option<EncryptionConfig>,
    pub retention: RetentionPolicy,
}

impl Default for ManifestDbOptions {
    fn default() -> Self {
        Self {
            wal_dir: PathBuf::new(),
            chunk_dir: PathBuf::new(),
            compression: CompressionConfig::default(),
            encryption: None,
            retention: RetentionPolicy::KeepAll,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DbLifecycle {
    Active,
    Draining,
    Tombstoned { tombstoned_at_epoch_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompressionConfig {
    pub codec: CompressionCodec,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            codec: CompressionCodec::None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionCodec {
    None,
    Zstd,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptionConfig {
    pub algorithm: EncryptionAlgorithm,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    None,
    Aes256Gcm,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RetentionPolicy {
    KeepAll,
    KeepGenerations { count: u32 },
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        RetentionPolicy::KeepAll
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalStateRecord {
    pub record_version: u16,
    pub last_sealed_segment: u64,
    pub last_applied_lsn: u64,
    pub last_checkpoint_generation: u64,
    pub restart_epoch: u32,
    pub flush_gate_state: FlushGateState,
}

impl Default for WalStateRecord {
    fn default() -> Self {
        Self {
            record_version: WAL_STATE_VERSION,
            last_sealed_segment: 0,
            last_applied_lsn: 0,
            last_checkpoint_generation: 0,
            restart_epoch: 0,
            flush_gate_state: FlushGateState::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeStateRecord {
    pub record_version: u16,
    pub instance_id: Uuid,
    pub pid: u32,
    pub started_at_epoch_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlushGateState {
    pub active_job_id: Option<JobId>,
    pub last_success_at_epoch_ms: Option<u64>,
    pub errored_since_epoch_ms: Option<u64>,
}

impl Default for FlushGateState {
    fn default() -> Self {
        Self {
            active_job_id: None,
            last_success_at_epoch_ms: None,
            errored_since_epoch_ms: None,
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct ManifestFlushSink {
    manifest: Arc<Manifest>,
    db_id: DbId,
}

impl ManifestFlushSink {
    pub(crate) fn new(manifest: Arc<Manifest>, db_id: DbId) -> Self {
        Self { manifest, db_id }
    }
}

impl FlushSink for ManifestFlushSink {
    fn apply_flush(&self, request: FlushSinkRequest) -> Result<(), FlushSinkError> {
        let manifest = self.manifest.clone();
        let db_id = self.db_id;
        thread::spawn(move || {
            let FlushSinkRequest {
                responder, target, ..
            } = request;
            let result = (|| -> Result<(), FlushSinkError> {
                let key = WalStateKey::new(db_id);
                let mut record = manifest
                    .wal_state(db_id)
                    .map_err(|err| FlushSinkError::Message(err.to_string()))?
                    .unwrap_or_else(WalStateRecord::default);

                let needs_update = target > record.last_applied_lsn
                    || record.flush_gate_state.errored_since_epoch_ms.is_some();

                if !needs_update {
                    return Ok(());
                }

                if target > record.last_applied_lsn {
                    record.last_applied_lsn = target;
                }

                record.flush_gate_state.last_success_at_epoch_ms = Some(epoch_millis());
                record.flush_gate_state.errored_since_epoch_ms = None;

                let mut txn = manifest.begin_with_capacity(1);
                txn.put_wal_state(key, record);
                txn.commit()
                    .map_err(|err| FlushSinkError::Message(err.to_string()))?;
                Ok(())
            })();

            match result {
                Ok(()) => responder.succeed(),
                Err(err) => responder.fail(err),
            }
        });
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkEntryRecord {
    pub record_version: u16,
    pub file_name: String,
    pub generation: u64,
    pub size_bytes: u64,
    pub compression: CompressionCodec,
    pub encrypted: bool,
    pub residency: ChunkResidency,
    pub uploaded_at_epoch_ms: Option<u64>,
    pub purge_after_epoch_ms: Option<u64>,
}

impl Default for ChunkEntryRecord {
    fn default() -> Self {
        Self {
            record_version: CHUNK_ENTRY_VERSION,
            file_name: String::new(),
            generation: 0,
            size_bytes: 0,
            compression: CompressionCodec::None,
            encrypted: false,
            residency: ChunkResidency::Local,
            uploaded_at_epoch_ms: None,
            purge_after_epoch_ms: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChunkResidency {
    Local,
    RemoteArchived {
        object: RemoteObjectKey,
        verified: bool,
    },
    LocalAndRemote {
        object: RemoteObjectKey,
        verified: bool,
    },
    Evicted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteObjectKey {
    pub namespace_id: RemoteNamespaceId,
    pub object: RemoteObjectId,
}

impl RemoteObjectKey {
    pub fn render_path(&self, namespace: &RemoteNamespaceRecord) -> String {
        let suffix = self.object.render_suffix();
        let prefix = namespace.prefix.trim_matches('/');
        if prefix.is_empty() {
            format!("{}/{}", namespace.bucket, suffix)
        } else {
            format!("{}/{}/{}", namespace.bucket, prefix, suffix)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteObjectId {
    pub origin_db_id: DbId,
    pub chunk_id: ChunkId,
    pub kind: RemoteObjectKind,
    pub generation: Generation,
}

impl RemoteObjectId {
    pub fn render_suffix(&self) -> String {
        format!(
            "{db_id:08x}/{chunk_id:08x}/{variant}:{generation:08x}",
            db_id = self.origin_db_id,
            chunk_id = self.chunk_id,
            variant = self.kind.variant_tag(),
            generation = self.generation
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RemoteObjectKind {
    Base,
    Delta { delta_id: u16 },
    Wal { shard: u16 },
    Custom { tag: String },
}

impl RemoteObjectKind {
    pub fn variant_tag(&self) -> String {
        match self {
            RemoteObjectKind::Base => "base".to_string(),
            RemoteObjectKind::Delta { delta_id } => format!("delta-{delta_id:04x}"),
            RemoteObjectKind::Wal { shard } => format!("wal-{shard:04x}"),
            RemoteObjectKind::Custom { tag } => tag.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WalArtifactKey {
    pub db_id: DbId,
    pub artifact_id: WalArtifactId,
}

impl WalArtifactKey {
    pub fn new(db_id: DbId, artifact_id: WalArtifactId) -> Self {
        Self { db_id, artifact_id }
    }

    fn encode(&self) -> u128 {
        ((self.db_id as u128) << 96) | (self.artifact_id as u128)
    }

    fn decode(raw: u128) -> Self {
        Self {
            db_id: (raw >> 96) as DbId,
            artifact_id: raw as WalArtifactId,
        }
    }

    fn with_db(self, db_id: DbId) -> Self {
        Self { db_id, ..self }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WalArtifactKind {
    AppendOnlySegment {
        start_page_index: u64,
        end_page_index: u64,
        size_bytes: u64,
    },
    PagerBundle {
        frame_count: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalArtifactRecord {
    pub record_version: u16,
    pub db_id: DbId,
    pub artifact_id: WalArtifactId,
    pub created_at_epoch_ms: u64,
    pub kind: WalArtifactKind,
    pub local_path: PathBuf,
}

impl WalArtifactRecord {
    pub fn new(
        db_id: DbId,
        artifact_id: WalArtifactId,
        kind: WalArtifactKind,
        local_path: PathBuf,
    ) -> Self {
        Self {
            record_version: WAL_ARTIFACT_VERSION,
            db_id,
            artifact_id,
            created_at_epoch_ms: epoch_millis(),
            kind,
            local_path,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteNamespaceRecord {
    pub record_version: u16,
    pub bucket: String,
    pub prefix: String,
    pub encryption_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkDeltaRecord {
    pub record_version: u16,
    pub base_chunk_id: ChunkId,
    pub delta_id: u16,
    pub delta_file: String,
    pub size_bytes: u64,
    pub residency: ChunkResidency,
}

impl Default for ChunkDeltaRecord {
    fn default() -> Self {
        Self {
            record_version: CHUNK_DELTA_VERSION,
            base_chunk_id: 0,
            delta_id: 0,
            delta_file: String::new(),
            size_bytes: 0,
            residency: ChunkResidency::Local,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotRecord {
    pub record_version: u16,
    pub db_id: DbId,
    pub snapshot_id: SnapshotId,
    pub created_at_epoch_ms: u64,
    pub source_generation: u64,
    pub chunks: Vec<SnapshotChunkRef>,
}

impl SnapshotRecord {
    pub fn key(&self) -> SnapshotKey {
        SnapshotKey::new(self.db_id, self.snapshot_id)
    }
}

impl Default for SnapshotRecord {
    fn default() -> Self {
        Self {
            record_version: SNAPSHOT_RECORD_VERSION,
            db_id: 0,
            snapshot_id: 0,
            created_at_epoch_ms: epoch_millis(),
            source_generation: 0,
            chunks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotChunkRef {
    pub chunk_id: ChunkId,
    pub kind: SnapshotChunkKind,
}

impl SnapshotChunkRef {
    pub fn base(chunk_id: ChunkId) -> Self {
        Self {
            chunk_id,
            kind: SnapshotChunkKind::Base,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SnapshotChunkKind {
    Base,
    Delta { base_chunk_id: ChunkId },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobRecord {
    pub record_version: u16,
    pub job_id: JobId,
    pub created_at_epoch_ms: u64,
    pub db_id: Option<DbId>,
    pub kind: JobKind,
    pub state: JobDurableState,
    pub payload: JobPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobKind {
    Flush,
    Checkpoint,
    Upload,
    Delete,
    Gc,
    Custom(u16),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobDurableState {
    Pending {
        enqueued_at_epoch_ms: u64,
    },
    InFlight {
        started_at_epoch_ms: u64,
    },
    Completed {
        completed_at_epoch_ms: u64,
    },
    Failed {
        failed_at_epoch_ms: u64,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobPayload {
    None,
    FlushShard { shard: u16 },
    ChunkUpload { chunk_id: ChunkId },
    Custom { bytes: Vec<u8> },
}

impl Default for JobPayload {
    fn default() -> Self {
        JobPayload::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricRecord {
    pub record_version: u16,
    pub count: u64,
    pub sum: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricDelta {
    pub count: u64,
    pub sum: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationRecord {
    pub record_version: u16,
    pub component: ComponentId,
    pub current_generation: Generation,
    pub updated_at_epoch_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentGeneration {
    pub component: ComponentId,
    pub generation: Generation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestChangeRecord {
    pub record_version: u16,
    pub sequence: ChangeSequence,
    pub committed_at_epoch_ms: u64,
    pub operations: Vec<ManifestOp>,
    pub generation_updates: Vec<ComponentGeneration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestCursorRecord {
    record_version: u16,
    cursor_id: ChangeCursorId,
    acked_sequence: ChangeSequence,
    created_at_epoch_ms: u64,
    updated_at_epoch_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkRefcountRecord {
    pub record_version: u16,
    pub chunk_id: ChunkId,
    pub strong: u64,
}
fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::page_cache::{PageCache, PageCacheConfig};
    use std::sync::Arc;
    use std::thread;

    const TEST_COMPONENT: ComponentId = 1;

    #[test]
    fn lmdb_keys_iterate_in_numeric_order() {
        fn chunk_record(name: &str) -> ChunkEntryRecord {
            let mut record = ChunkEntryRecord::default();
            record.file_name = name.to_string();
            record.size_bytes = 1;
            record
        }

        fn chunk_delta_record(base: ChunkId, delta_id: u16, name: &str) -> ChunkDeltaRecord {
            let mut record = ChunkDeltaRecord::default();
            record.base_chunk_id = base;
            record.delta_id = delta_id;
            record.delta_file = name.to_string();
            record.size_bytes = 1;
            record
        }

        fn snapshot_record(
            db_id: DbId,
            snapshot_id: SnapshotId,
            chunk_id: ChunkId,
        ) -> SnapshotRecord {
            SnapshotRecord {
                record_version: SNAPSHOT_RECORD_VERSION,
                db_id,
                snapshot_id,
                created_at_epoch_ms: epoch_millis(),
                source_generation: 0,
                chunks: vec![SnapshotChunkRef::base(chunk_id)],
            }
        }

        fn metric_delta(count: u64) -> MetricDelta {
            MetricDelta {
                count,
                sum: count as f64,
                min: Some(count as f64),
                max: Some(count as f64),
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let mut txn = manifest.begin();
        txn.put_wal_state(WalStateKey::new(2), WalStateRecord::default());
        txn.put_wal_state(WalStateKey::new(1), WalStateRecord::default());

        txn.upsert_chunk(ChunkKey::new(2, 5), chunk_record("chunk-2-5.bin"));
        txn.upsert_chunk(ChunkKey::new(1, 7), chunk_record("chunk-1-7.bin"));
        txn.upsert_chunk(ChunkKey::new(1, 3), chunk_record("chunk-1-3.bin"));

        txn.upsert_chunk_delta(
            ChunkDeltaKey::new(2, 5, 4),
            chunk_delta_record(5, 4, "delta-2-5-4.bin"),
        );
        txn.upsert_chunk_delta(
            ChunkDeltaKey::new(1, 3, 9),
            chunk_delta_record(3, 9, "delta-1-3-9.bin"),
        );
        txn.upsert_chunk_delta(
            ChunkDeltaKey::new(1, 3, 1),
            chunk_delta_record(3, 1, "delta-1-3-1.bin"),
        );

        let snapshot_a = snapshot_record(2, 9, 5);
        let snapshot_b = snapshot_record(1, 4, 3);
        txn.publish_snapshot(snapshot_a);
        txn.publish_snapshot(snapshot_b);

        txn.register_wal_artifact(
            WalArtifactKey::new(2, 10),
            WalArtifactRecord::new(
                2,
                10,
                WalArtifactKind::AppendOnlySegment {
                    start_page_index: 0,
                    end_page_index: 1,
                    size_bytes: 1,
                },
                PathBuf::from("artifact-2-10.wal"),
            ),
        );
        txn.register_wal_artifact(
            WalArtifactKey::new(1, 40),
            WalArtifactRecord::new(
                1,
                40,
                WalArtifactKind::AppendOnlySegment {
                    start_page_index: 0,
                    end_page_index: 1,
                    size_bytes: 1,
                },
                PathBuf::from("artifact-1-40.wal"),
            ),
        );

        txn.upsert_pending_job(PendingJobKey::new(2, 200), 3);
        txn.upsert_pending_job(PendingJobKey::new(1, 500), 5);
        txn.upsert_pending_job(PendingJobKey::new(1, 100), 4);

        txn.merge_metric(MetricKey::new(2, 7), metric_delta(1));
        txn.merge_metric(MetricKey::new(1, 2), metric_delta(2));
        txn.merge_metric(MetricKey::new(1, 5), metric_delta(3));

        txn.commit().unwrap();

        let (
            wal_state_keys,
            chunk_keys,
            chunk_delta_keys,
            snapshot_keys,
            wal_artifact_keys,
            pending_keys,
            metric_keys,
        ) = manifest
            .read(|tables, txn| {
                let wal_state = {
                    let mut cursor = tables.wal_state.iter(txn)?;
                    let mut out = Vec::new();
                    while let Some((raw, _)) = cursor.next().transpose()? {
                        out.push(WalStateKey::decode(raw));
                    }
                    out
                };
                let chunk = {
                    let mut cursor = tables.chunk_catalog.iter(txn)?;
                    let mut out = Vec::new();
                    while let Some((raw, _)) = cursor.next().transpose()? {
                        out.push(ChunkKey::decode(raw));
                    }
                    out
                };
                let chunk_delta = {
                    let mut cursor = tables.chunk_delta_index.iter(txn)?;
                    let mut out = Vec::new();
                    while let Some((raw, _)) = cursor.next().transpose()? {
                        out.push(ChunkDeltaKey::decode(raw));
                    }
                    out
                };
                let snapshots = {
                    let mut cursor = tables.snapshot_index.iter(txn)?;
                    let mut out = Vec::new();
                    while let Some((raw, _)) = cursor.next().transpose()? {
                        out.push(SnapshotKey::decode(raw));
                    }
                    out
                };
                let artifacts = {
                    let mut cursor = tables.wal_catalog.iter(txn)?;
                    let mut out = Vec::new();
                    while let Some((raw, _)) = cursor.next().transpose()? {
                        out.push(WalArtifactKey::decode(raw));
                    }
                    out
                };
                let pending = {
                    let mut cursor = tables.job_pending_index.iter(txn)?;
                    let mut out = Vec::new();
                    while let Some((raw, _)) = cursor.next().transpose()? {
                        out.push(PendingJobKey::decode(raw));
                    }
                    out
                };
                let metrics = {
                    let mut cursor = tables.metrics.iter(txn)?;
                    let mut out = Vec::new();
                    while let Some((raw, _)) = cursor.next().transpose()? {
                        out.push(MetricKey::decode(raw));
                    }
                    out
                };
                Ok((
                    wal_state,
                    chunk,
                    chunk_delta,
                    snapshots,
                    artifacts,
                    pending,
                    metrics,
                ))
            })
            .unwrap();

        assert_eq!(
            wal_state_keys
                .iter()
                .map(|key| key.db_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            chunk_keys
                .iter()
                .map(|key| (key.db_id, key.chunk_id))
                .collect::<Vec<_>>(),
            vec![(1, 3), (1, 7), (2, 5)]
        );
        assert_eq!(
            chunk_delta_keys
                .iter()
                .map(|key| (key.db_id, key.chunk_id, key.generation))
                .collect::<Vec<_>>(),
            vec![(1, 3, 1), (1, 3, 9), (2, 5, 4)]
        );
        assert_eq!(
            snapshot_keys
                .iter()
                .map(|key| (key.db_id, key.snapshot_id))
                .collect::<Vec<_>>(),
            vec![(1, 4), (2, 9)]
        );
        assert_eq!(
            wal_artifact_keys
                .iter()
                .map(|key| (key.db_id, key.artifact_id))
                .collect::<Vec<_>>(),
            vec![(1, 40), (2, 10)]
        );
        assert_eq!(
            pending_keys
                .iter()
                .map(|key| (key.db_id, key.job_kind))
                .collect::<Vec<_>>(),
            vec![(1, 100), (1, 500), (2, 200)]
        );
        assert_eq!(
            metric_keys
                .iter()
                .map(|key| (key.scope, key.metric_kind))
                .collect::<Vec<_>>(),
            vec![(1, 2), (1, 5), (2, 7)]
        );
    }

    #[test]
    fn change_log_records_commits_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        for _ in 0..3 {
            let mut txn = manifest.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        }

        let cursor = manifest
            .register_change_cursor(ChangeCursorStart::Oldest)
            .unwrap();
        assert_eq!(cursor.next_sequence, cursor.oldest_sequence);

        let page = manifest.fetch_change_page(cursor.cursor_id, 10).unwrap();
        assert_eq!(page.changes.len(), 3);
        let sequences: Vec<_> = page.changes.iter().map(|change| change.sequence).collect();
        assert_eq!(sequences, vec![1, 2, 3]);
        for change in &page.changes {
            assert!(matches!(
                change.operations.as_slice(),
                [ManifestOp::BumpGeneration { component, .. }]
                if *component == TEST_COMPONENT
            ));
        }

        let ack = manifest
            .acknowledge_changes(cursor.cursor_id, page.next_sequence.saturating_sub(1))
            .unwrap();
        assert_eq!(ack.acked_sequence, 3);
    }

    #[test]
    fn batched_commits_have_ordered_change_log() {
        let dir = tempfile::tempdir().unwrap();
        let mut options = ManifestOptions::default();
        options.commit_latency = Duration::from_millis(80);
        let manifest = Manifest::open(dir.path(), options).unwrap();

        thread::scope(|scope| {
            let manifest_ref = &manifest;
            scope.spawn(move || {
                let mut txn = manifest_ref.begin();
                txn.bump_generation(TEST_COMPONENT, 1);
                txn.commit().unwrap();
            });

            let mut txn = manifest.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        });

        let diagnostics = manifest.diagnostics();
        assert_eq!(diagnostics.committed_batches, 1);

        let cursor = manifest
            .register_change_cursor(ChangeCursorStart::Oldest)
            .unwrap();
        let page = manifest.fetch_change_page(cursor.cursor_id, 10).unwrap();
        let sequences: Vec<_> = page.changes.iter().map(|change| change.sequence).collect();
        assert_eq!(sequences, vec![1, 2]);
    }

    #[test]
    fn cursor_resume_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let cursor_id;
        {
            let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

            let cursor = manifest
                .register_change_cursor(ChangeCursorStart::Oldest)
                .unwrap();
            cursor_id = cursor.cursor_id;

            for _ in 0..2 {
                let mut txn = manifest.begin();
                txn.bump_generation(TEST_COMPONENT, 1);
                txn.commit().unwrap();
            }

            let page = manifest.fetch_change_page(cursor.cursor_id, 1).unwrap();
            assert_eq!(page.changes.len(), 1);
            assert_eq!(page.changes[0].sequence, 1);

            manifest.acknowledge_changes(cursor.cursor_id, 1).unwrap();
        }

        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();
        let page = manifest.fetch_change_page(cursor_id, 10).unwrap();
        assert!(page.changes.first().is_some());
        assert_eq!(page.changes[0].sequence, 2);

        let ack = manifest
            .acknowledge_changes(cursor_id, page.next_sequence.saturating_sub(1))
            .unwrap();
        assert_eq!(ack.acked_sequence, 2);
    }

    #[test]
    fn truncation_respects_min_ack() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        for _ in 0..3 {
            let mut txn = manifest.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        }

        let cursor_a = manifest
            .register_change_cursor(ChangeCursorStart::Oldest)
            .unwrap();
        let cursor_b = manifest
            .register_change_cursor(ChangeCursorStart::Oldest)
            .unwrap();

        manifest.acknowledge_changes(cursor_a.cursor_id, 3).unwrap();
        manifest.acknowledge_changes(cursor_b.cursor_id, 1).unwrap();

        let sequences = manifest
            .read(|tables, txn| {
                let mut iter = tables.change_log.iter(txn)?;
                let mut seqs = Vec::new();
                while let Some((seq, _)) = iter.next().transpose()? {
                    seqs.push(seq);
                }
                Ok(seqs)
            })
            .unwrap();
        assert_eq!(sequences, vec![2, 3]);

        manifest.acknowledge_changes(cursor_b.cursor_id, 3).unwrap();

        let sequences = manifest
            .read(|tables, txn| {
                let mut iter = tables.change_log.iter(txn)?;
                let mut seqs = Vec::new();
                while let Some((seq, _)) = iter.next().transpose()? {
                    seqs.push(seq);
                }
                Ok(seqs)
            })
            .unwrap();
        assert!(sequences.is_empty());
    }

    #[test]
    fn cursor_start_sequence_clamps_to_available_window() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        for _ in 0..3 {
            let mut txn = manifest.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        }

        let cursor = manifest
            .register_change_cursor(ChangeCursorStart::Sequence(2))
            .unwrap();
        assert_eq!(cursor.next_sequence, 2);

        manifest.acknowledge_changes(cursor.cursor_id, 3).unwrap();

        for _ in 0..2 {
            let mut txn = manifest.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        }

        let cursor_b = manifest
            .register_change_cursor(ChangeCursorStart::Sequence(1))
            .unwrap();
        // All prior entries were truncated, so the cursor should start at the new oldest sequence.
        assert_eq!(cursor_b.next_sequence, cursor_b.oldest_sequence);
        let page = manifest.fetch_change_page(cursor_b.cursor_id, 10).unwrap();
        assert!(page.changes.first().is_some());
        assert_eq!(page.changes[0].sequence, cursor_b.next_sequence);
    }

    #[test]
    fn wait_for_change_unblocks_on_commit() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Arc::new(Manifest::open(dir.path(), ManifestOptions::default()).unwrap());

        let waiter = Arc::clone(&manifest);
        let handle = thread::spawn(move || {
            waiter
                .wait_for_change(0, Some(Instant::now() + Duration::from_secs(1)))
                .unwrap()
        });

        thread::sleep(Duration::from_millis(50));
        let mut txn = manifest.begin();
        txn.bump_generation(TEST_COMPONENT, 1);
        txn.commit().unwrap();

        let latest = handle.join().unwrap();
        assert!(latest >= 1);
    }

    #[test]
    fn batching_coalesces_commits() {
        let dir = tempfile::tempdir().unwrap();
        let mut options = ManifestOptions::default();
        options.commit_latency = Duration::from_millis(80);
        let manifest = Manifest::open(dir.path(), options).unwrap();

        thread::scope(|scope| {
            let manifest_ref = &manifest;
            scope.spawn(move || {
                let mut txn = manifest_ref.begin();
                txn.bump_generation(TEST_COMPONENT, 1);
                txn.commit().unwrap();
            });

            let mut txn = manifest.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        });

        let diagnostics = manifest.diagnostics();
        assert_eq!(diagnostics.committed_batches, 1);
    }

    #[test]
    fn snapshot_refcounts_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let chunk_key = ChunkKey::new(7, 42);
        let mut chunk = ChunkEntryRecord::default();
        chunk.generation = 1;
        chunk.size_bytes = 1024;
        chunk.file_name = String::from("chunk-42.dat");

        let mut txn = manifest.begin();
        txn.upsert_chunk(chunk_key, chunk.clone());
        txn.commit().unwrap();

        let snapshot = SnapshotRecord {
            record_version: SNAPSHOT_RECORD_VERSION,
            db_id: 7,
            snapshot_id: 1,
            created_at_epoch_ms: epoch_millis(),
            source_generation: 1,
            chunks: vec![SnapshotChunkRef::base(42)],
        };

        let mut txn = manifest.begin();
        txn.publish_snapshot(snapshot.clone());
        txn.commit().unwrap();

        let refcount = manifest
            .read(|tables, txn| {
                Ok(tables
                    .gc_refcounts
                    .get(txn, &(42u64))?
                    .map(|record| record.strong)
                    .unwrap_or(0))
            })
            .unwrap();
        assert_eq!(refcount, 1);

        let mut txn = manifest.begin();
        txn.drop_snapshot(snapshot.key());
        txn.commit().unwrap();

        let refcount = manifest
            .read(|tables, txn| {
                Ok(tables
                    .gc_refcounts
                    .get(txn, &(42u64))?
                    .map(|record| record.strong)
                    .unwrap_or(0))
            })
            .unwrap();
        assert_eq!(refcount, 0);
    }

    #[test]
    fn retention_clears_refcount_on_last_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let chunk_key = ChunkKey::new(9, 100);
        let mut chunk_entry = ChunkEntryRecord::default();
        chunk_entry.file_name = String::from("chunk-100.bin");

        let mut txn = manifest.begin();
        txn.upsert_chunk(chunk_key, chunk_entry.clone());
        txn.commit().unwrap();

        let snapshot_a = SnapshotRecord {
            record_version: SNAPSHOT_RECORD_VERSION,
            db_id: 9,
            snapshot_id: 1,
            created_at_epoch_ms: epoch_millis(),
            source_generation: 1,
            chunks: vec![SnapshotChunkRef::base(100)],
        };
        let snapshot_b = SnapshotRecord {
            record_version: SNAPSHOT_RECORD_VERSION,
            db_id: 9,
            snapshot_id: 2,
            created_at_epoch_ms: epoch_millis(),
            source_generation: 1,
            chunks: vec![SnapshotChunkRef::base(100)],
        };

        let mut txn = manifest.begin();
        txn.publish_snapshot(snapshot_a.clone());
        txn.commit().unwrap();
        let mut txn = manifest.begin();
        txn.publish_snapshot(snapshot_b.clone());
        txn.commit().unwrap();

        let refcount = manifest
            .read(|tables, txn| {
                Ok(tables
                    .gc_refcounts
                    .get(txn, &(100u64))?
                    .map(|record| record.strong)
                    .unwrap_or(0))
            })
            .unwrap();
        assert_eq!(refcount, 2);

        let mut txn = manifest.begin();
        txn.drop_snapshot(snapshot_a.key());
        txn.commit().unwrap();
        let refcount = manifest
            .read(|tables, txn| {
                Ok(tables
                    .gc_refcounts
                    .get(txn, &(100u64))?
                    .map(|record| record.strong)
                    .unwrap_or(0))
            })
            .unwrap();
        assert_eq!(refcount, 1);

        let mut txn = manifest.begin();
        txn.drop_snapshot(snapshot_b.key());
        txn.commit().unwrap();
        let refcount = manifest
            .read(|tables, txn| {
                Ok(tables
                    .gc_refcounts
                    .get(txn, &(100u64))?
                    .map(|record| record.strong)
                    .unwrap_or(0))
            })
            .unwrap();
        assert_eq!(refcount, 0);
    }

    #[test]
    fn fork_db_clones_state_and_refcounts() {
        const SOURCE_DB: DbId = 11;
        const TARGET_DB: DbId = 12;
        const CHUNK_ID: ChunkId = 77;

        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let mut source_descriptor = DbDescriptorRecord::default();
        source_descriptor.name = "source".into();

        let mut target_descriptor = DbDescriptorRecord::default();
        target_descriptor.name = "fork".into();

        let mut chunk_entry = ChunkEntryRecord::default();
        chunk_entry.file_name = "chunk-77.bin".into();
        chunk_entry.generation = 3;
        chunk_entry.size_bytes = 4_096;

        let mut wal_state = WalStateRecord::default();
        wal_state.last_sealed_segment = 2;
        wal_state.flush_gate_state.active_job_id = Some(99);

        let wal_artifact_key = WalArtifactKey::new(SOURCE_DB, 42);
        let wal_artifact = WalArtifactRecord::new(
            SOURCE_DB,
            42,
            WalArtifactKind::AppendOnlySegment {
                start_page_index: 0,
                end_page_index: 1_024,
                size_bytes: 1_024,
            },
            PathBuf::from("segment-42.wal"),
        );

        let mut txn = manifest.begin();
        txn.put_db(SOURCE_DB, source_descriptor.clone());
        txn.put_wal_state(WalStateKey::new(SOURCE_DB), wal_state.clone());
        txn.upsert_chunk(ChunkKey::new(SOURCE_DB, CHUNK_ID), chunk_entry.clone());
        txn.register_wal_artifact(wal_artifact_key, wal_artifact.clone());
        txn.adjust_refcount(CHUNK_ID, 1);
        txn.commit().unwrap();

        let before_refcount = manifest
            .read(|tables, txn| {
                Ok(tables
                    .gc_refcounts
                    .get(txn, &(CHUNK_ID as u64))?
                    .map(|record| record.strong)
                    .unwrap_or(0))
            })
            .unwrap();
        assert_eq!(before_refcount, 1);

        manifest
            .fork_db(SOURCE_DB, TARGET_DB, target_descriptor.clone())
            .unwrap();

        manifest
            .read(|tables, txn| {
                let chunk_key = ChunkKey::new(TARGET_DB, CHUNK_ID).encode();
                assert!(tables.chunk_catalog.get(txn, &chunk_key)?.is_some());

                let wal_key = WalStateKey::new(TARGET_DB).encode();
                let state = tables.wal_state.get(txn, &wal_key)?.unwrap();
                assert_eq!(state.flush_gate_state.active_job_id, None);

                let artifact_key = WalArtifactKey::new(TARGET_DB, 42).encode();
                let artifact = tables.wal_catalog.get(txn, &artifact_key)?.unwrap();
                assert_eq!(artifact.db_id, TARGET_DB);
                assert_eq!(artifact.artifact_id, 42);

                Ok(())
            })
            .unwrap();

        let after_refcount = manifest
            .read(|tables, txn| {
                Ok(tables
                    .gc_refcounts
                    .get(txn, &(CHUNK_ID as u64))?
                    .map(|record| record.strong)
                    .unwrap_or(0))
            })
            .unwrap();
        assert_eq!(after_refcount, 2);
    }

    #[test]
    fn change_page_hits_cache_on_second_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(PageCache::new(PageCacheConfig {
            capacity_bytes: Some(16 * 1024),
        }));
        let mut options = ManifestOptions::default();
        options.page_cache = Some(cache.clone());
        let manifest = Manifest::open(dir.path(), options).unwrap();

        {
            let mut txn = manifest.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        }

        let cursor = manifest
            .register_change_cursor(ChangeCursorStart::Oldest)
            .unwrap();
        let page = manifest.fetch_change_page(cursor.cursor_id, 10).unwrap();
        assert_eq!(page.changes.len(), 1);

        let metrics_before = cache.metrics();
        let _ = manifest.fetch_change_page(cursor.cursor_id, 10).unwrap();
        let metrics_after = cache.metrics();
        assert!(metrics_after.hits > metrics_before.hits);

        let diagnostics = manifest.diagnostics();
        assert_eq!(diagnostics.page_cache_hits, 2);
        assert_eq!(diagnostics.page_cache_misses, 0);

        let cache_metrics = manifest.page_cache_metrics().unwrap();
        assert_eq!(cache_metrics.hits, diagnostics.page_cache_hits);
        assert_eq!(cache_metrics.misses, diagnostics.page_cache_misses);
    }

    #[test]
    fn open_sets_runtime_state_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        let options = ManifestOptions::default();

        let manifest = Manifest::open(dir.path(), options).expect("open manifest");

        assert!(!manifest.crash_detected());
        let runtime = manifest.runtime_state();
        assert_eq!(runtime.record_version, RUNTIME_STATE_VERSION);
        assert_eq!(runtime.pid, process::id());
    }

    #[test]
    fn manifest_flush_sink_updates_state() {
        use crate::flush::{FlushSinkRequest, FlushSinkResponder, FlushTask};
        use crate::io::{IoFile, IoResult, IoVec, IoVecMut};
        use crate::wal::WalSegment;
        use crossfire::mpsc;

        #[derive(Default)]
        struct NoopIoFile;

        impl IoFile for NoopIoFile {
            fn readv_at(&self, _offset: u64, _bufs: &mut [IoVecMut<'_>]) -> IoResult<usize> {
                Ok(0)
            }

            fn writev_at(&self, _offset: u64, bufs: &[IoVec<'_>]) -> IoResult<usize> {
                Ok(bufs.iter().map(IoVec::len).sum())
            }

            fn allocate(&self, _offset: u64, _len: u64) -> IoResult<()> {
                Ok(())
            }

            fn flush(&self) -> IoResult<()> {
                Ok(())
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let manifest = Arc::new(Manifest::open(dir.path(), ManifestOptions::default()).unwrap());
        let sink = ManifestFlushSink::new(manifest.clone(), 42);
        let segment = Arc::new(WalSegment::new(Arc::new(NoopIoFile::default()), 0, 0));
        let (tx, rx) = mpsc::bounded_blocking(1);
        let sender = Arc::new(tx);

        let responder = FlushSinkResponder::new(sender.clone(), segment.clone(), 0, 512);
        let request = FlushSinkRequest {
            segment: segment.clone(),
            target: 512,
            responder: responder.for_request(),
        };
        sink.apply_flush(request).expect("apply flush");
        match rx.recv().expect("sink ack") {
            FlushTask::SinkAck { result, .. } => assert!(result.is_ok()),
            other => panic!("unexpected flush task: {:?}", other),
        }

        let state = manifest
            .read(|tables, txn| {
                Ok(tables
                    .wal_state
                    .get(txn, &WalStateKey::new(42).encode())?
                    .unwrap())
            })
            .expect("wal state");
        assert_eq!(state.last_applied_lsn, 512);
        assert!(state.flush_gate_state.last_success_at_epoch_ms.is_some());
        assert!(state.flush_gate_state.errored_since_epoch_ms.is_none());

        let responder = FlushSinkResponder::new(sender.clone(), segment.clone(), 0, 256);
        let request = FlushSinkRequest {
            segment: segment.clone(),
            target: 256,
            responder: responder.for_request(),
        };
        sink.apply_flush(request).expect("apply flush");
        match rx.recv().expect("sink ack") {
            FlushTask::SinkAck { result, .. } => assert!(result.is_ok()),
            other => panic!("unexpected flush task: {:?}", other),
        }
        let state = manifest
            .read(|tables, txn| {
                Ok(tables
                    .wal_state
                    .get(txn, &WalStateKey::new(42).encode())?
                    .unwrap())
            })
            .expect("wal state");
        assert_eq!(state.last_applied_lsn, 512);
    }

    fn open_detects_leftover_runtime_state() {
        let dir = tempfile::tempdir().unwrap();
        let options = ManifestOptions::default();

        {
            let manifest = Manifest::open(dir.path(), options.clone()).expect("first open");
            assert!(!manifest.crash_detected());
        }

        {
            let env = unsafe {
                EnvOpenOptions::new()
                    .map_size(options.map_size)
                    .max_dbs(options.max_dbs)
                    .open(dir.path())
                    .expect("env open")
            };

            let mut txn = env.write_txn().expect("write txn");
            let runtime_db = env
                .create_database::<U32<heed::byteorder::BigEndian>, SerdeBincode<RuntimeStateRecord>>(
                    &mut txn,
                    Some("runtime_state"),
                )
                .expect("runtime db");
            let record = RuntimeStateRecord {
                record_version: RUNTIME_STATE_VERSION,
                instance_id: Uuid::new_v4(),
                pid: 9999,
                started_at_epoch_ms: epoch_millis(),
            };
            runtime_db
                .put(&mut txn, &RUNTIME_STATE_KEY, &record)
                .expect("insert runtime state");
            txn.commit().expect("commit runtime state");
        }

        let manifest = Manifest::open(dir.path(), options).expect("reopen manifest");
        assert!(manifest.crash_detected());
        assert_eq!(
            manifest.runtime_state().record_version,
            RUNTIME_STATE_VERSION
        );
    }
}
