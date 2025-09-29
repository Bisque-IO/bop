use std::sync::atomic::{AtomicU64, Ordering};

/// Write-ahead log facade for DB instances.
/// Wal is broken down into Segments. The is always 1 tail segment where new records
/// are appended to and 1 segment that is archiving when checkpointing.
///
/// Checkpointing involves sealing the current tail segment and starting a new tail segment.
/// Then, the old tail segment can be archived by merging Slabs (zstd compressed) in with Chunk files
/// and uploading to S3 storage.
#[derive(Debug)]
pub struct Wal {
    last_sequence: AtomicU64,
}

impl Wal {
    /// Create a new WAL handle.
    pub fn new() -> Self {
        Self {
            last_sequence: AtomicU64::new(0),
        }
    }

    /// Record the latest observed sequence number.
    pub fn mark_progress(&self, sequence: u64) {
        self.last_sequence.store(sequence, Ordering::Relaxed);
    }

    /// Produce a diagnostic snapshot of WAL progress.
    pub fn diagnostics(&self) -> WalDiagnostics {
        WalDiagnostics {
            last_sequence: self.last_sequence(),
        }
    }

    /// Return the most recent sequence number tracked by this WAL.
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Default, Clone)]
pub struct WalDiagnostics {
    pub last_sequence: u64,
}

/// Logged segment placeholder.
pub struct WalSegment {}
