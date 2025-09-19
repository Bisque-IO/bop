use std::convert::TryInto;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

use crate::aof::error::{AofError, AofResult};
use crate::aof::filesystem::FileSystem;
use crate::aof::record::{FlushCheckpointStamp, RecordHeader, RecordTrailer, SegmentHeader};
use crate::aof::segment::Segment;

/// Segment position information for a reader
#[derive(Debug, Clone, Copy)]
pub struct SegmentPosition {
    /// Segment file base ID
    pub segment_id: u64,
    /// File offset of the next record to read in this segment
    pub file_offset: u64,
    /// Record ID of the next record to read
    pub record_id: u64,
}

impl Default for SegmentPosition {
    fn default() -> Self {
        Self {
            segment_id: 0,
            file_offset: 0,
            record_id: 1, // Start with record ID 1
        }
    }
}

/// Minimal reader for local sequential access to AOF segments
pub struct Reader<FS: FileSystem> {
    /// Unique reader ID
    pub id: u64,
    /// Current segment being read from
    current_segment: Option<Arc<Segment<FS>>>,
    /// Current position in the segment file
    segment_offset: u64,
    /// Current record ID we're positioned at
    record_id: u64,
    /// Whether this reader is in tailing mode (caught up and needs notifications)
    pub is_tailing: AtomicBool,
    /// Optional lifecycle guard for cleanup callbacks
    lifecycle_guard: Option<ReaderLifecycleGuard>,
    /// Optional notify handle used when tailing
    tail_notify: Option<Arc<Notify>>,
}

impl<FS: FileSystem> Reader<FS> {
    pub fn new(reader_id: u64) -> Self {
        Reader {
            id: reader_id,
            current_segment: None,
            segment_offset: 0,
            record_id: 1, // Start with record ID 1
            is_tailing: AtomicBool::new(false),
            lifecycle_guard: None,
            tail_notify: None,
        }
    }

    /// Attach a lifecycle guard that will run when this reader is dropped
    pub fn set_lifecycle_guard(&mut self, guard: ReaderLifecycleGuard) {
        self.lifecycle_guard = Some(guard);
    }

    /// Attach a notify handle used when this reader tails the log
    pub fn set_tail_notify(&mut self, notify: Arc<Notify>) {
        self.tail_notify = Some(notify);
    }

    /// Get the tailing notify handle, if any
    pub fn tail_notify(&self) -> Option<Arc<Notify>> {
        self.tail_notify.as_ref().map(Arc::clone)
    }

    /// Wait for a tail notification; returns error if reader is not tailing
    pub async fn wait_for_tail_notification(&self) -> AofResult<()> {
        let notify = self
            .tail_notify()
            .ok_or_else(|| AofError::InvalidState("Reader is not tailing".to_string()))?;
        notify.notified().await;
        Ok(())
    }

    /// Wait for the next record when tailing; returns None if not tailing anymore
    pub async fn tail_next_record(&mut self) -> AofResult<Option<Vec<u8>>> {
        loop {
            if let Some(record) = self.read_next_record().await? {
                return Ok(Some(record));
            }

            if !self.is_tailing.load(Ordering::SeqCst) {
                return Ok(None);
            }

            self.wait_for_tail_notification().await?;
        }
    }

    /// Set the current segment for reading
    pub fn set_current_segment(&mut self, segment: Arc<Segment<FS>>, offset: u64) {
        self.current_segment = Some(segment);
        self.segment_offset = offset;
    }

    /// Read the next record from the current segment
    pub async fn read_next_record(&mut self) -> AofResult<Option<Vec<u8>>> {
        let segment = match &self.current_segment {
            Some(seg) => seg.clone(),
            None => return Ok(None), // No segment loaded
        };

        // Get mmap data from segment
        let mmap_data = segment.get_mmap_data()?;
        let current_offset = self.segment_offset;

        let header_size = std::mem::size_of::<RecordHeader>();
        let trailer_size = std::mem::size_of::<RecordTrailer>();
        let checkpoint_size = std::mem::size_of::<FlushCheckpointStamp>();
        let segment_header_size = std::mem::size_of::<SegmentHeader>();

        let mut offset = current_offset as usize;

        loop {
            if offset == 0 && mmap_data.len() >= segment_header_size {
                let header_bytes = &mmap_data[..segment_header_size];
                if let Ok(header) = bincode::deserialize::<SegmentHeader>(header_bytes) {
                    if header.verify() {
                        offset = segment_header_size;
                        continue;
                    }
                }
            }

            if offset + checkpoint_size <= mmap_data.len() {
                let magic = u32::from_le_bytes(
                    mmap_data[offset..offset + 4]
                        .try_into()
                        .expect("slice length checked"),
                );
                if magic == FlushCheckpointStamp::MAGIC {
                    offset += checkpoint_size;
                    if offset >= mmap_data.len() {
                        self.segment_offset = offset as u64;
                        return Ok(None);
                    }
                    continue;
                }
            }

            if offset + header_size > mmap_data.len() {
                self.segment_offset = offset as u64;
                return Ok(None);
            }

            let header_bytes = &mmap_data[offset..offset + header_size];
            let header = bincode::deserialize::<RecordHeader>(header_bytes).map_err(|e| {
                AofError::Serialization(format!("Failed to deserialize header: {}", e))
            })?;

            if header.size == 0 {
                self.segment_offset = offset as u64;
                return Ok(None);
            }

            let data_start = offset + header_size;
            let data_end = data_start + header.size as usize;
            if data_end > mmap_data.len() {
                return Err(AofError::CorruptedRecord(
                    "Record data extends beyond segment".to_string(),
                ));
            }

            if data_end + trailer_size > mmap_data.len() {
                return Err(AofError::CorruptedRecord(
                    "Record trailer extends beyond segment".to_string(),
                ));
            }

            let data = &mmap_data[data_start..data_end];
            if !header.verify_checksum(data) {
                return Err(AofError::CorruptedRecord(format!(
                    "Checksum verification failed for record {}",
                    header.id
                )));
            }

            let trailer_bytes = &mmap_data[data_end..data_end + trailer_size];
            let trailer = bincode::deserialize::<RecordTrailer>(trailer_bytes).map_err(|e| {
                AofError::Serialization(format!("Failed to deserialize trailer: {}", e))
            })?;

            if !trailer.validate(&header, None) {
                return Err(AofError::CorruptedRecord(format!(
                    "Trailer validation failed for record {}",
                    header.id
                )));
            }

            let next_offset = data_end + trailer_size;
            self.segment_offset = next_offset as u64;
            self.record_id = header.id;
            return Ok(Some(data.to_vec()));
        }
    }

    /// Get current position
    pub fn position(&self) -> SegmentPosition {
        let segment_id = match &self.current_segment {
            Some(segment) => segment.metadata().base_id,
            None => 0,
        };

        SegmentPosition {
            segment_id,
            file_offset: self.segment_offset,
            record_id: self.record_id,
        }
    }

    /// Set position to a specific segment and offset
    pub fn set_position(&mut self, pos: SegmentPosition) {
        self.segment_offset = pos.file_offset;
        self.record_id = pos.record_id;
    }

    /// Get current record ID
    pub fn current_record_id(&self) -> u64 {
        self.record_id
    }

    /// Seek to the beginning of the current segment
    pub fn seek_to_start(&mut self) {
        self.segment_offset = 0;
        self.record_id = 1;
    }

    /// Seek to a specific record ID within the current segment
    pub fn seek_to_record(&mut self, target_id: u64) -> AofResult<()> {
        let segment = self
            .current_segment
            .as_ref()
            .ok_or(AofError::RecordNotFound(target_id))?;

        let data = segment.get_mmap_data()?;
        let header_size = RecordHeader::size();
        let trailer_size = RecordTrailer::size();
        let checkpoint_size = FlushCheckpointStamp::size();
        let segment_header_size = SegmentHeader::size();
        let mut offset = 0usize;
        let segment_size = segment.metadata().size as usize;

        while offset + header_size <= segment_size {
            if offset == 0 && segment_size >= segment_header_size {
                let header_bytes = &data[..segment_header_size];
                if let Ok(header) = SegmentHeader::from_bytes(header_bytes) {
                    if header.verify() {
                        offset = segment_header_size;
                        continue;
                    }
                }
            }

            if offset + checkpoint_size <= segment_size {
                let magic = u32::from_le_bytes(
                    data[offset..offset + 4]
                        .try_into()
                        .expect("slice length checked"),
                );
                if magic == FlushCheckpointStamp::MAGIC {
                    offset += checkpoint_size;
                    continue;
                }
            }

            let header_bytes = &data[offset..offset + header_size];
            let header = RecordHeader::from_bytes(header_bytes).map_err(|e| {
                AofError::CorruptedRecord(format!("Failed to deserialize header: {}", e))
            })?;

            if header.id >= target_id {
                self.segment_offset = offset as u64;
                self.record_id = header.id.saturating_sub(1);
                return Ok(());
            }

            offset += header_size + header.size as usize + trailer_size;
        }

        self.segment_offset = segment_size as u64;
        self.record_id = segment.metadata().last_id;
        Err(AofError::RecordNotFound(target_id))
    }

    /// Check if at end of current segment
    pub fn is_at_end(&self) -> bool {
        match &self.current_segment {
            Some(segment) => {
                let mmap_data = segment.get_mmap_data().ok();
                match mmap_data {
                    Some(data) => self.segment_offset >= data.len() as u64,
                    None => true,
                }
            }
            None => true,
        }
    }
}

impl<FS: FileSystem> Drop for Reader<FS> {
    fn drop(&mut self) {
        if let Some(guard) = self.lifecycle_guard.take() {
            guard.release();
        }
    }
}

/// Hooks invoked for reader lifecycle events
pub trait ReaderLifecycleHooks: Send + Sync + 'static {
    /// Called when the reader is dropped
    fn on_drop(&self, reader_id: u64);
}

/// Guard that triggers lifecycle hooks when released or dropped
pub struct ReaderLifecycleGuard {
    reader_id: u64,
    hooks: Arc<dyn ReaderLifecycleHooks>,
    released: AtomicBool,
}

impl ReaderLifecycleGuard {
    pub fn new(reader_id: u64, hooks: Arc<dyn ReaderLifecycleHooks>) -> Self {
        Self {
            reader_id,
            hooks,
            released: AtomicBool::new(false),
        }
    }

    /// Explicitly release the guard, invoking hooks once
    pub fn release(self) {
        self.invoke_once();
    }

    fn invoke_once(&self) {
        if !self.released.swap(true, Ordering::AcqRel) {
            self.hooks.on_drop(self.reader_id);
        }
    }
}

impl Drop for ReaderLifecycleGuard {
    fn drop(&mut self) {
        if !self.released.load(Ordering::Acquire) {
            self.hooks.on_drop(self.reader_id);
            self.released.store(true, Ordering::Release);
        }
    }
}

/// Binary index entry for fast record lookups
#[derive(Debug, Clone, Copy)]
pub struct BinaryIndexEntry {
    pub record_id: u64,
    pub file_offset: u64,
}

impl BinaryIndexEntry {
    pub const SIZE: usize = 16; // 8 bytes record_id + 8 bytes file_offset

    pub fn new(record_id: u64, file_offset: u64) -> Self {
        Self {
            record_id,
            file_offset,
        }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0..8].copy_from_slice(&self.record_id.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.file_offset.to_le_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> AofResult<Self> {
        if bytes.len() < Self::SIZE {
            return Err(AofError::CorruptedRecord(
                "Invalid index entry size".to_string(),
            ));
        }

        let record_id = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let file_offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap());

        Ok(Self {
            record_id,
            file_offset,
        })
    }
}

/// Binary index for a segment
#[derive(Debug)]
pub struct BinaryIndex {
    entries: Vec<BinaryIndexEntry>,
}

impl BinaryIndex {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn add_entry(&mut self, entry: BinaryIndexEntry) {
        self.entries.push(entry);
    }

    pub fn find_offset(&self, record_id: u64) -> Option<u64> {
        self.entries
            .iter()
            .find(|entry| entry.record_id == record_id)
            .map(|entry| entry.file_offset)
    }

    pub fn entries(&self) -> &[BinaryIndexEntry] {
        &self.entries
    }
}

/// Minimal AOF metrics trait
pub trait AofMetrics {
    fn get_total_records(&self) -> u64;
}

/// Internal reader interface (simplified)
pub trait ReaderInternal<FS: FileSystem> {
    fn position(&self) -> SegmentPosition;
    fn advance(&mut self, next_offset: u64, next_record_id: u64);
    fn move_to_segment(&mut self, segment_id: u64, start_offset: u64, record_id: u64);
}

impl<FS: FileSystem> ReaderInternal<FS> for Reader<FS> {
    fn position(&self) -> SegmentPosition {
        self.position()
    }

    fn advance(&mut self, next_offset: u64, next_record_id: u64) {
        self.segment_offset = next_offset;
        self.record_id = next_record_id;
    }

    fn move_to_segment(&mut self, _segment_id: u64, start_offset: u64, record_id: u64) {
        // TODO: Update current_segment based on segment_id
        self.segment_offset = start_offset;
        self.record_id = record_id;
    }
}

/// Trait for AOF implementations that support direct segment offset reading
pub trait AofSegmentReader<FS: FileSystem> {
    /// Read a record at a specific segment file offset
    /// The reader will be updated with the next position after reading
    fn read_at_segment_offset(
        &mut self,
        segment_id: u64,
        file_offset: u64,
        expected_record_id: u64,
        reader: &Reader<FS>,
    ) -> AofResult<Option<(u64, Vec<u8>)>>;
}
