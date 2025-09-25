/// Write-ahead log facade for DB instances.
/// Wal is broken down into Segments. The is always 1 tail segment where new records
/// are appended to and 1 segment that is archiving when checkpointing.
///
/// Checkpointing involves sealing the current tail segment and starting a new tail segment.
/// Then, the old tail segment can be archived by merging Slabs (zstd compressed) in with Chunk files
/// and uploading to S3 storage.
pub struct Wal;

impl Wal {
    /// Create a new WAL handle.
    pub fn new() -> Self {
        Self
    }
}

///
pub struct WalSegment {}
