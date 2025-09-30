//! Flush sink integration for the manifest system.
//!
//! This module provides a flush sink implementation that updates the manifest's
//! WAL state when flush operations complete. The flush sink is used by the WAL
//! flush coordinator to track durable progress.

use std::sync::Arc;
use std::thread;

use crate::flush::{FlushSink, FlushSinkError, FlushSinkRequest};

use super::tables::{WalStateKey, WalStateRecord, epoch_millis};
use super::{DbId, Manifest};

/// Flush sink that updates manifest WAL state on flush completion.
///
/// This sink is installed in the WAL flush coordinator. When a flush completes,
/// it updates the `last_applied_lsn` and flush gate state in the manifest to
/// reflect the new durable point.
pub(crate) struct ManifestFlushSink {
    /// Reference to the manifest instance.
    manifest: Arc<Manifest>,

    /// Database ID that this sink is tracking.
    db_id: DbId,
}

impl ManifestFlushSink {
    /// Creates a new manifest flush sink for the given database.
    pub(crate) fn new(manifest: Arc<Manifest>, db_id: DbId) -> Self {
        Self { manifest, db_id }
    }
}

impl FlushSink for ManifestFlushSink {
    /// Applies a flush by updating the manifest WAL state.
    ///
    /// This operation runs asynchronously on a background thread. It updates the
    /// `last_applied_lsn` field in the WAL state record to reflect the new durable
    /// point, and clears any error state in the flush gate.
    ///
    /// If the target LSN is not greater than the current `last_applied_lsn`, the
    /// operation still succeeds but doesn't update the LSN (it only clears errors).
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest transaction fails.
    fn apply_flush(&self, request: FlushSinkRequest) -> Result<(), FlushSinkError> {
        let manifest = self.manifest.clone();
        let db_id = self.db_id;
        // Spawn a background thread to avoid blocking the flush coordinator
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
