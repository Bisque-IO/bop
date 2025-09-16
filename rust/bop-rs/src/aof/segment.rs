use crc32fast::Hasher as Crc32Hasher;
use memmap2::Mmap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::aof::error::{AofError, AofResult};
use crate::aof::filesystem::{FileHandle, FileSystem};
use crate::aof::index::SegmentFooter;
use crate::aof::reader::BinaryIndex;
use crate::aof::record::{
    Record, RecordHeader, SegmentMetadata, current_timestamp, ensure_segment_size_is_valid,
};

/// Manages a read-only segment of the log with binary index access
pub struct Segment<FS: FileSystem> {
    /// File name for the segment.
    path: String,
    /// File system reference for operations.
    fs: Arc<FS>,
    /// The underlying file handle (None if not loaded).
    file_handle: Option<FS::Handle>,
    /// Read-only memory map of the file (None if not loaded).
    mmap: Option<Mmap>,
    /// Binary index information (replaces BTreeMap)
    binary_index: Option<BinaryIndex>,
    /// Segment metadata.
    metadata: SegmentMetadata,
    /// Last access time for LRU management.
    last_access: Arc<AtomicU64>,
    /// Whether this segment is currently loaded in memory.
    pub is_loaded: AtomicBool,
}

impl<FS: FileSystem> Segment<FS> {
    /// Opens an existing segment file (simplified without IndexStorage)
    pub async fn open(
        path: &str,
        fs: Arc<FS>,
        expected_metadata: Option<SegmentMetadata>,
    ) -> AofResult<Self> {
        let file_handle = fs.open_file(path).await?;
        let file_metadata = fs.file_metadata(path).await?;
        let size = file_metadata.size;
        ensure_segment_size_is_valid(size)?;

        let mmap = file_handle
            .memory_map()
            .await?
            .ok_or_else(|| AofError::FileSystem("Memory mapping not supported".to_string()))?;

        let (binary_index, segment_metadata) = if let Some(metadata) = expected_metadata {
            // Use provided metadata, no index loading since IndexStorage is removed
            (None, metadata)
        } else {
            // Scan file to build metadata (no index built during scanning)
            let (metadata, binary_index) = Self::build_metadata_and_find_index(&mmap, size)?;
            (binary_index, metadata)
        };

        Ok(Segment {
            path: path.to_string(),
            fs,
            file_handle: Some(file_handle),
            mmap: Some(mmap),
            binary_index,
            metadata: segment_metadata,
            last_access: Arc::new(AtomicU64::new(current_timestamp())),
            is_loaded: AtomicBool::new(true),
        })
    }

    /// Opens segment lazily (without loading mmap or index)
    pub fn open_lazy(path: &str, fs: Arc<FS>, metadata: SegmentMetadata) -> AofResult<Self> {
        Ok(Segment {
            path: path.to_string(),
            fs,
            file_handle: None,
            mmap: None,
            binary_index: None,
            metadata,
            last_access: Arc::new(AtomicU64::new(0)),
            is_loaded: AtomicBool::new(false),
        })
    }

    /// Ensures the segment is loaded in memory
    fn ensure_loaded(&mut self) -> AofResult<()> {
        if self.is_loaded.load(Ordering::Acquire) {
            self.last_access
                .store(current_timestamp(), Ordering::Release);
            return Ok(());
        }

        // Memory mapping should be fast since it's essentially just establishing a view
        let file_handle = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.fs.open_file(&self.path))
        })?;
        let mmap = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(file_handle.memory_map())
        })?
        .ok_or_else(|| AofError::FileSystem("Memory mapping not supported".to_string()))?;

        // No binary index loading since IndexStorage was removed
        let binary_index = None;

        self.file_handle = Some(file_handle);
        self.mmap = Some(mmap);
        self.binary_index = binary_index;
        self.last_access
            .store(current_timestamp(), Ordering::Release);
        self.is_loaded.store(true, Ordering::Release);

        Ok(())
    }

    /// Unloads the segment from memory
    #[allow(dead_code)]
    pub fn unload(&mut self) {
        self.file_handle = None;
        self.mmap = None;
        self.binary_index = None;
        self.is_loaded.store(false, Ordering::Release);
    }

    /// Builds metadata and finds binary index from the segment file
    fn build_metadata_and_find_index(
        mmap: &Mmap,
        size: u64,
    ) -> AofResult<(SegmentMetadata, Option<BinaryIndex>)> {
        let mut current_offset = 0u64;
        let mut record_count = 0u64;
        let mut base_id = 0u64;
        let mut last_id = 0u64;
        let mut hasher = Crc32Hasher::new();

        // Scan through records to build metadata
        while current_offset < size {
            if current_offset + RecordHeader::size() as u64 > size {
                break;
            }

            let header_bytes = &mmap
                [current_offset as usize..(current_offset + RecordHeader::size() as u64) as usize];
            let header = RecordHeader::from_bytes(header_bytes).map_err(|e| {
                AofError::CorruptedRecord(format!("Failed to deserialize header: {}", e))
            })?;

            if header.id == 0 && header.size == 0 && current_offset == 0 {
                break; // Empty segment
            }

            let data_start = current_offset + RecordHeader::size() as u64;
            let data_end = data_start + header.size as u64;
            if data_end > size {
                return Err(AofError::CorruptedRecord(
                    "Record extends beyond segment boundary".to_string(),
                ));
            }

            let data = &mmap[data_start as usize..data_end as usize];
            if !header.verify_checksum(data) {
                return Err(AofError::CorruptedRecord(format!(
                    "Checksum mismatch for record {}",
                    header.id
                )));
            }

            if record_count == 0 {
                base_id = header.id;
            }

            hasher.update(header_bytes);
            hasher.update(data);

            last_id = header.id;
            record_count += 1;
            current_offset = data_end;
        }

        let metadata = SegmentMetadata {
            base_id,
            last_id,
            record_count,
            created_at: current_timestamp(),
            size: current_offset, // Actual data size
            checksum: hasher.finalize(),
            compressed: false,
            encrypted: false,
        };

        // Try to find binary index after the data
        let binary_index = if current_offset < size {
            // Check for footer at the end of file
            match SegmentFooter::read_from_end(&mmap[..]) {
                Ok(footer) if footer.index_offset == current_offset => Some(BinaryIndex::new()),
                _ => None,
            }
        } else {
            None
        };

        Ok((metadata, binary_index))
    }

    /// Reads a record at a specific file offset (optimized for readers)
    pub fn read_at_offset(&mut self, file_offset: u64) -> AofResult<Option<(u64, Vec<u8>, u64)>> {
        self.ensure_loaded()?;
        let mmap = self.mmap.as_ref().unwrap();

        if file_offset + RecordHeader::size() as u64 > self.metadata.size {
            return Ok(None);
        }

        // Read header
        let header_bytes =
            &mmap[file_offset as usize..(file_offset + RecordHeader::size() as u64) as usize];
        let header = RecordHeader::from_bytes(header_bytes).map_err(|e| {
            AofError::CorruptedRecord(format!("Failed to deserialize header: {}", e))
        })?;

        let data_start = file_offset + RecordHeader::size() as u64;
        let data_end = data_start + header.size as u64;

        if data_end > self.metadata.size {
            return Err(AofError::CorruptedRecord(
                "Record data extends beyond segment".to_string(),
            ));
        }

        let data = &mmap[data_start as usize..data_end as usize];

        // Verify checksum
        if !header.verify_checksum(data) {
            return Err(AofError::CorruptedRecord(format!(
                "Checksum verification failed for record {}",
                header.id
            )));
        }

        self.last_access
            .store(current_timestamp(), Ordering::Release);

        // Return record ID, data, and next record offset
        Ok(Some((header.id, data.to_vec(), data_end)))
    }

    /// Reads a record by ID using binary index lookup (fallback method)
    pub fn read<'a>(&'a mut self, id: u64) -> AofResult<Option<Record<'a>>> {
        if id < self.metadata.base_id || id > self.metadata.last_id {
            return Ok(None);
        }

        self.ensure_loaded()?;
        let mmap = self.mmap.as_ref().unwrap();

        // Try to use binary index first
        if let Some(ref binary_index) = self.binary_index {
            if let Some(offset) = binary_index.find_offset(id) {
                // Read header directly from mmap
                let header_bytes =
                    &mmap[offset as usize..(offset + RecordHeader::size() as u64) as usize];
                let header = RecordHeader::from_bytes(header_bytes).map_err(|e| {
                    AofError::CorruptedRecord(format!("Failed to deserialize header: {}", e))
                })?;

                if header.id == id {
                    let data_start = offset + RecordHeader::size() as u64;
                    let data_end = data_start + header.size as u64;

                    if data_end > self.metadata.size {
                        return Err(AofError::CorruptedRecord(
                            "Record data extends beyond segment".to_string(),
                        ));
                    }

                    let data_slice = &mmap[data_start as usize..data_end as usize];

                    // Verify checksum
                    if !header.verify_checksum(data_slice) {
                        return Err(AofError::CorruptedRecord(format!(
                            "Checksum verification failed for record {}",
                            id
                        )));
                    }

                    self.last_access
                        .store(current_timestamp(), Ordering::Release);
                    return Ok(Some(Record {
                        header,
                        data: data_slice,
                    }));
                }
            }
        }

        // Fallback to sequential scan if no index or lookup failed
        self.sequential_scan_for_record(id, mmap)
    }

    /// Sequential scan fallback when binary index is not available
    fn sequential_scan_for_record<'a>(
        &self,
        target_id: u64,
        mmap: &'a Mmap,
    ) -> AofResult<Option<Record<'a>>> {
        let mut current_offset = 0u64;

        while current_offset < self.metadata.size {
            if current_offset + RecordHeader::size() as u64 > self.metadata.size {
                break;
            }

            let header_bytes = &mmap
                [current_offset as usize..(current_offset + RecordHeader::size() as u64) as usize];
            let header = RecordHeader::from_bytes(header_bytes).map_err(|e| {
                AofError::CorruptedRecord(format!("Failed to deserialize header: {}", e))
            })?;

            if header.id == target_id {
                let data_start = current_offset + RecordHeader::size() as u64;
                let data_end = data_start + header.size as u64;

                if data_end > self.metadata.size {
                    return Err(AofError::CorruptedRecord(
                        "Record data extends beyond segment".to_string(),
                    ));
                }

                let data = &mmap[data_start as usize..data_end as usize];

                // Verify checksum
                if !header.verify_checksum(data) {
                    return Err(AofError::CorruptedRecord(format!(
                        "Checksum verification failed for record {}",
                        target_id
                    )));
                }

                return Ok(Some(Record { header, data }));
            }

            // Move to next record
            let data_end = current_offset + RecordHeader::size() as u64 + header.size as u64;
            current_offset = data_end;
        }

        Ok(None)
    }

    /// Get the first record offset in this segment (for reader positioning)
    pub fn first_record_offset(&self) -> u64 {
        0 // Records always start at offset 0 in segments
    }

    /// Gets the last record ID in this segment.
    pub fn last_record_id(&self) -> Option<u64> {
        if self.metadata.record_count == 0 {
            None
        } else {
            Some(self.metadata.last_id)
        }
    }

    /// Helper method to get record count.
    #[allow(dead_code)]
    pub fn record_count(&self) -> u64 {
        self.metadata.record_count
    }

    /// Get segment metadata
    pub fn metadata(&self) -> &SegmentMetadata {
        &self.metadata
    }

    /// Get last access time for LRU management
    #[allow(dead_code)]
    pub fn last_access_time(&self) -> u64 {
        self.last_access.load(Ordering::Acquire)
    }

    /// Check if segment is loaded in memory
    #[allow(dead_code)]
    pub fn is_loaded(&self) -> bool {
        self.is_loaded.load(Ordering::Acquire)
    }

    /// Get direct access to the mmap data for efficient reading
    pub fn get_mmap_data(&self) -> AofResult<&[u8]> {
        match &self.mmap {
            Some(mmap) => Ok(&mmap[..]),
            None => Err(AofError::SegmentNotLoaded(
                "Segment mmap not available".to_string(),
            )),
        }
    }
}
