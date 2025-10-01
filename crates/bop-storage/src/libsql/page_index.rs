use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::manifest::{
    ManifestTxn, PageVersionHeadKey, PageVersionHeadRecord, PageVersionIndexKey, PageVersionRecord,
};
use tracing::{debug, error, info, instrument, trace, warn};

/// Errors emitted by the WAL page index journal.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PageIndexJournalError {
    #[error("page index journal sealed; no further mutations accepted")]
    Sealed,
}

/// Operation captured by the WAL page index journal.
#[derive(Debug, Clone)]
pub enum WalPageIndexOp {
    PutVersion {
        key: PageVersionIndexKey,
        record: PageVersionRecord,
    },
    DeleteVersion {
        key: PageVersionIndexKey,
    },
    PutHead {
        key: PageVersionHeadKey,
        record: PageVersionHeadRecord,
    },
    DeleteHead {
        key: PageVersionHeadKey,
    },
}

/// Thread-safe journal that records page version metadata alongside WAL frames.
#[derive(Debug)]
pub struct WalPageIndexJournal {
    sealed: AtomicBool,
    ops: Mutex<Vec<WalPageIndexOp>>,
}

impl Default for WalPageIndexJournal {
    fn default() -> Self {
        Self::new()
    }
}

impl WalPageIndexJournal {
    /// Create an empty journal.
    pub fn new() -> Self {
        Self {
            sealed: AtomicBool::new(false),
            ops: Mutex::new(Vec::new()),
        }
    }

    /// Returns true if the journal has been sealed.
    pub fn is_sealed(&self) -> bool {
        self.sealed.load(Ordering::Acquire)
    }

    /// Seal the journal, preventing further mutations.
    pub fn seal(&self) {
        self.sealed.store(true, Ordering::Release);
        debug!(op_count = self.len(), "Sealed page index journal");
    }

    /// Append an operation to the journal.
    pub fn record(&self, op: WalPageIndexOp) -> Result<(), PageIndexJournalError> {
        if self.is_sealed() {
            warn!("Attempted to record operation on sealed journal");
            return Err(PageIndexJournalError::Sealed);
        }
        let mut guard = self.ops.lock().expect("page index journal mutex poisoned");
        guard.push(op);
        trace!(op_count = guard.len(), "Recorded operation to page index journal");
        Ok(())
    }

    /// Extend the journal with multiple operations.
    pub fn extend<I>(&self, iter: I) -> Result<(), PageIndexJournalError>
    where
        I: IntoIterator<Item = WalPageIndexOp>,
    {
        if self.is_sealed() {
            warn!("Attempted to extend sealed journal");
            return Err(PageIndexJournalError::Sealed);
        }
        let mut guard = self.ops.lock().expect("page index journal mutex poisoned");
        let before_len = guard.len();
        guard.extend(iter);
        let added = guard.len() - before_len;
        debug!(added, total = guard.len(), "Extended page index journal");
        Ok(())
    }

    /// Drain all recorded operations.
    pub fn drain(&self) -> Vec<WalPageIndexOp> {
        let mut guard = self.ops.lock().expect("page index journal mutex poisoned");
        guard.drain(..).collect()
    }

    /// Returns the number of recorded operations.
    pub fn len(&self) -> usize {
        self.ops
            .lock()
            .expect("page index journal mutex poisoned")
            .len()
    }

    /// Returns true if the journal is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Apply recorded operations to a manifest transaction.
#[instrument(skip(ops, txn))]
pub fn apply_page_index_ops<'a, I>(ops: I, txn: &mut ManifestTxn<'a>)
where
    I: IntoIterator<Item = WalPageIndexOp>,
{
    let mut put_version_count = 0;
    let mut delete_version_count = 0;
    let mut put_head_count = 0;
    let mut delete_head_count = 0;

    for op in ops {
        match op {
            WalPageIndexOp::PutVersion { key, record } => {
                txn.put_page_version(key, record);
                put_version_count += 1;
            }
            WalPageIndexOp::DeleteVersion { key } => {
                txn.delete_page_version(key);
                delete_version_count += 1;
            }
            WalPageIndexOp::PutHead { key, record } => {
                txn.put_page_head(key, record);
                put_head_count += 1;
            }
            WalPageIndexOp::DeleteHead { key } => {
                txn.delete_page_head(key);
                delete_head_count += 1;
            }
        }
    }

    debug!(
        put_version_count,
        delete_version_count,
        put_head_count,
        delete_head_count,
        "Applied page index operations to manifest"
    );
}
