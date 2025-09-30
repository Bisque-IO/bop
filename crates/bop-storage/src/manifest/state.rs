//! Runtime state and diagnostics for the manifest.
//!
//! This module contains types for tracking manifest diagnostics and managing
//! the runtime state sentinel used for crash detection.

use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};

use heed::Env;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::tables::{ManifestTables, RuntimeStateRecord, epoch_millis};
use super::{ChangeSequence, ManifestError};
use crate::page_cache::{PageCache, PageCacheKey, PageCacheMetricsSnapshot};

/// Current version of the runtime state record format.
pub(super) const RUNTIME_STATE_VERSION: u16 = 1;

/// Fixed key used for the runtime state sentinel record.
pub(super) const RUNTIME_STATE_KEY: u32 = 0;

/// Shared diagnostics counters for the manifest.
#[derive(Debug, Default)]
pub(super) struct ManifestDiagnostics {
    pub(super) committed_batches: AtomicU64,
}

/// Runtime state for crash detection.
///
/// The manifest writes a sentinel record on startup and clears it on clean shutdown.
/// If the record exists on startup, it indicates a previous crash.
#[derive(Debug)]
pub(super) struct ManifestRuntimeState {
    pub(super) key: u32,
    pub(super) record: RuntimeStateRecord,
    pub(super) crash_detected: bool,
}

impl ManifestRuntimeState {
    /// Initialize the runtime state sentinel.
    ///
    /// Returns a `ManifestRuntimeState` with `crash_detected = true` if a sentinel
    /// was already present.
    pub(super) fn initialize(env: &Env, tables: &ManifestTables) -> Result<Self, ManifestError> {
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

    /// Clear the runtime state sentinel on clean shutdown.
    pub(super) fn clear(&self, env: &Env, tables: &ManifestTables) -> Result<(), ManifestError> {
        let mut txn = env.write_txn()?;
        tables.runtime_state.delete(&mut txn, &self.key)?;
        txn.commit()?;
        Ok(())
    }
}

/// Snapshot of manifest diagnostics at a point in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestDiagnosticsSnapshot {
    pub committed_batches: u64,
    pub page_cache_hits: u64,
    pub page_cache_misses: u64,
    pub page_cache_insertions: u64,
    pub page_cache_evictions: u64,
}

impl ManifestDiagnosticsSnapshot {
    /// Create a diagnostics snapshot from the manifest's state.
    pub(super) fn from_manifest(
        diagnostics: &ManifestDiagnostics,
        page_cache: &Option<std::sync::Arc<PageCache<PageCacheKey>>>,
    ) -> Self {
        let mut snapshot = Self {
            committed_batches: diagnostics.committed_batches.load(Ordering::Relaxed),
            page_cache_hits: 0,
            page_cache_misses: 0,
            page_cache_insertions: 0,
            page_cache_evictions: 0,
        };

        if let Some(cache) = page_cache {
            let metrics = cache.metrics();
            snapshot.page_cache_hits = metrics.hits;
            snapshot.page_cache_misses = metrics.misses;
            snapshot.page_cache_insertions = metrics.insertions;
            snapshot.page_cache_evictions = metrics.evictions;
        }

        snapshot
    }
}

/// Condition variable for signaling new change log entries.
///
/// This allows readers to efficiently wait for new changes without polling.
#[derive(Debug)]
pub(super) struct ChangeSignal {
    last_sequence: Mutex<ChangeSequence>,
    condvar: Condvar,
}

impl ChangeSignal {
    /// Create a new change signal with the given initial sequence.
    pub(super) fn new(initial: ChangeSequence) -> Self {
        Self {
            last_sequence: Mutex::new(initial),
            condvar: Condvar::new(),
        }
    }

    /// Update the signal with a new sequence number and wake all waiters.
    pub(super) fn update(&self, latest: ChangeSequence) {
        let mut guard = self
            .last_sequence
            .lock()
            .expect("change signal mutex poisoned");
        if *guard < latest {
            *guard = latest;
            self.condvar.notify_all();
        }
    }

    /// Get the current sequence number.
    pub(super) fn current(&self) -> ChangeSequence {
        *self
            .last_sequence
            .lock()
            .expect("change signal mutex poisoned")
    }

    /// Wait for a sequence number greater than `since`.
    ///
    /// Returns the new sequence number or times out.
    pub(super) fn wait_for(
        &self,
        since: ChangeSequence,
        deadline: Option<std::time::Instant>,
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
                let now = std::time::Instant::now();
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

/// Bootstrap data for initializing change log state.
#[derive(Debug)]
pub(super) struct ChangeStateBootstrap {
    pub(super) state: super::change_log::ChangeLogState,
    pub(super) next_cursor_id: super::ChangeCursorId,
}
