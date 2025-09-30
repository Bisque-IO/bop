//! Background worker thread for manifest operations.
//!
//! This module implements the manifest worker, which is responsible for:
//! - Batching and committing manifest transactions
//! - Managing change log truncation
//! - Processing cursor registration and acknowledgment requests
//! - Coordinating generation waits
//!
//! # Architecture
//!
//! The worker runs on a dedicated thread and communicates with the main manifest
//! API via channels. It processes commands from the command queue and batches
//! write operations together to improve throughput.
//!
//! # Batching Strategy
//!
//! Incoming write operations are held in a pending queue until either:
//! - A commit latency deadline is reached (for throughput)
//! - A shutdown is requested (for durability)
//!
//! This allows multiple concurrent callers to have their operations committed in
//! a single LMDB transaction, reducing overhead.
//!
//! # Crash Recovery
//!
//! Pending batches are journaled to LMDB before being applied. If the process crashes
//! before a batch is committed, it will be replayed on the next startup.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use heed::Env;
use serde::{Deserialize, Serialize};

use crate::page_cache::{PageCache, PageCacheKey};

use super::change_log::{ChangeLogState, maybe_truncate_change_log};
use super::cursors::{
    CursorAckRequest, CursorRegistrationRequest, acknowledge_cursor, register_cursor,
};
use super::operations::{commit_pending, persist_pending_batch};
use super::tables::{JOB_ID_COMPONENT, ManifestTables};
use super::{
    ChangeSignal, ComponentId, Generation, ManifestDiagnostics, ManifestError, ManifestOp,
};

/// Handle to the background worker thread.
///
/// Dropping this handle will join the worker thread, ensuring it completes gracefully.
#[derive(Debug)]
pub(super) struct WorkerHandle {
    /// Join handle for the worker thread (Some until joined, then None).
    pub(super) join: Option<JoinHandle<()>>,
}

impl WorkerHandle {
    /// Joins the worker thread, waiting for it to complete.
    pub(super) fn stop(&mut self) {
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

/// Entry in the generation wait queue for a specific component.
///
/// Waiters are processed in order by their target generation.
#[derive(Debug)]
struct WaitEntry {
    /// Target generation to wait for.
    target: Generation,

    /// Deadline for the wait operation.
    deadline: Instant,

    /// Channel to notify the waiter when the generation is reached or the deadline expires.
    responder: SyncSender<Result<(), ManifestError>>,
}

/// Commands that can be sent to the manifest worker thread.
#[derive(Debug)]
pub(super) enum ManifestCommand {
    /// Apply a batch of operations and return a commit receipt.
    Apply {
        batch: ManifestBatch,
        completion: SyncSender<Result<CommitReceipt, ManifestError>>,
    },

    /// Wait for a component to reach a target generation.
    Wait(WaitRequest),

    /// Register a new change cursor.
    RegisterCursor(CursorRegistrationRequest),

    /// Acknowledge changes for an existing cursor.
    AcknowledgeCursor(CursorAckRequest),

    /// Initiate graceful shutdown of the worker.
    Shutdown,
}

/// A pending commit that has been journaled but not yet applied to LMDB.
#[derive(Debug)]
pub(super) struct PendingCommit {
    /// Unique ID for this pending batch (used for crash recovery).
    pub(super) id: PendingBatchId,

    /// The batch of operations to commit.
    pub(super) batch: ManifestBatch,

    /// Optional completion channel to notify the caller (None for replayed batches).
    pub(super) completion: Option<SyncSender<Result<CommitReceipt, ManifestError>>>,
}

/// Persistent record of a pending batch for crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PendingBatchRecord {
    /// Record format version.
    pub(super) record_version: u16,

    /// The batch contents.
    pub(super) batch: ManifestBatch,
}

/// Unique identifier for a pending batch in the journal.
pub(super) type PendingBatchId = u64;

/// Current version of the pending batch record format.
pub(super) const PENDING_BATCH_RECORD_VERSION: u16 = 1;

/// A batch of manifest operations to be committed atomically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ManifestBatch {
    /// Operations in this batch.
    pub(super) ops: Vec<ManifestOp>,
}

/// Request to wait for a component to reach a target generation.
#[derive(Debug)]
pub(super) struct WaitRequest {
    /// Component ID to watch.
    pub(super) component: ComponentId,

    /// Target generation to wait for.
    pub(super) target: Generation,

    /// Deadline for the wait operation.
    pub(super) deadline: Instant,

    /// Channel to notify when complete or timed out.
    pub(super) responder: SyncSender<Result<(), ManifestError>>,
}

/// Receipt returned after successfully committing a batch.
///
/// Contains the generation numbers after the commit, allowing callers
/// to verify that their operations were applied.
#[derive(Debug, Clone)]
pub struct CommitReceipt {
    /// Current generation for each component after the commit.
    pub generations: HashMap<ComponentId, Generation>,
}

/// Main event loop for the manifest worker thread.
///
/// This function processes commands from the command channel, batches write operations,
/// commits them to LMDB, and coordinates state updates. It runs until a shutdown command
/// is received and all pending operations are flushed.
///
/// # Parameters
///
/// - `env`: LMDB environment
/// - `tables`: Manifest table handles
/// - `diagnostics`: Shared diagnostics counters
/// - `generation_cache`: Shared cache of current generations
/// - `job_id_counter`: Atomic counter for allocating job IDs
/// - `change_state_shared`: Shared change log state for readers
/// - `change_signal`: Condition variable for notifying change log consumers
/// - `cursor_id_counter`: Atomic counter for allocating cursor IDs
/// - `command_rx`: Channel for receiving commands from the main thread
/// - `generations`: Initial generation map (worker's working copy)
/// - `change_state`: Initial change log state (worker's working copy)
/// - `pending`: Initial pending commits (from crash recovery)
/// - `batch_journal_counter`: Counter for pending batch IDs
/// - `commit_latency`: How long to batch operations before committing
/// - `page_cache`: Optional page cache for change log entries
/// - `page_cache_object_id`: Cache key prefix for this manifest
///
/// # Concurrency
///
/// This function is the single writer for the manifest LMDB database. All write
/// operations are serialized through this worker to avoid conflicts.
#[allow(clippy::too_many_arguments)]
pub(super) fn worker_loop(
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
    mut pending: Vec<PendingCommit>,
    batch_journal_counter: Arc<AtomicU64>,
    commit_latency: std::time::Duration,
    page_cache: Option<Arc<PageCache<PageCacheKey>>>,
    page_cache_object_id: Option<u64>,
) {
    let mut waiters: HashMap<ComponentId, VecDeque<WaitEntry>> = HashMap::new();
    let mut deadline: Option<Instant> = if pending.is_empty() {
        None
    } else {
        Some(Instant::now())
    };
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
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
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
                    let batch_id = batch_journal_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Err(err) = persist_pending_batch(&env, &tables, batch_id, &batch) {
                        let _ = completion.send(Err(err));
                        continue;
                    }
                    pending.push(PendingCommit {
                        id: batch_id,
                        batch,
                        completion: Some(completion),
                    });
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
                        page_cache.as_ref(),
                        page_cache_object_id,
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
                pending.as_slice(),
                &mut generations,
                &mut persisted_job_counter,
                &diagnostics,
                &mut change_state,
                page_cache.as_ref(),
                page_cache_object_id,
            ) {
                Ok(snapshots) => {
                    for (entry, snapshot) in pending.iter().zip(snapshots.into_iter()) {
                        if let Some(completion) = &entry.completion {
                            let _ = completion.send(Ok(CommitReceipt {
                                generations: snapshot,
                            }));
                        }
                    }
                    pending.clear();
                    deadline = None;
                    if maybe_truncate_change_log(
                        &env,
                        &tables,
                        &mut change_state,
                        page_cache.as_ref(),
                        page_cache_object_id,
                    )
                    .is_err()
                    {
                        break;
                    }
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
                    for entry in pending.drain(..) {
                        if let Some(completion) = entry.completion {
                            let _ =
                                completion.send(Err(ManifestError::CommitFailed(err_msg.clone())));
                        }
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

/// Handles a generation wait request.
///
/// If the target generation has already been reached, the request is satisfied immediately.
/// Otherwise, the request is queued in the waiters map, sorted by target generation.
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

/// Processes the waiter queues, satisfying or timing out requests as appropriate.
///
/// This function walks through all waiter queues, checking if their target generations
/// have been reached or their deadlines have expired. Satisfied or expired waiters are
/// removed from the queue and notified.
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
