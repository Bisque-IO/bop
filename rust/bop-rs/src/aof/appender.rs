use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use memmap2::MmapMut;
use crc32fast::Hasher as Crc32Hasher;
use crossbeam::queue::SegQueue;
use hdrhistogram::Histogram;

use crate::aof::error::{AofError, AofResult};
use crate::aof::filesystem::{FileSystem, FileHandle};
use crate::aof::index::SegmentFooter;
use crate::aof::record::{FlushStrategy, RecordHeader, SegmentMetadata, current_timestamp, ensure_segment_size_is_valid};
use crate::aof::reader::BinaryIndexEntry;

/// Comprehensive flush metrics with HDR histograms and detailed counters
#[derive(Debug)]
pub struct FlushControllerMetrics {
    // Flush operation counters
    pub total_flushes: AtomicU64,
    pub successful_flushes: AtomicU64,
    pub failed_flushes: AtomicU64,
    pub forced_flushes: AtomicU64,
    pub periodic_flushes: AtomicU64,
    pub batched_flushes: AtomicU64,

    // Record counters
    pub total_records_flushed: AtomicU64,
    pub total_bytes_flushed: AtomicU64,
    pub pending_records: AtomicU64,
    pub pending_bytes: AtomicU64,

    // Timing counters
    pub total_flush_time_us: AtomicU64,
    pub last_flush_timestamp: AtomicU64,
    pub longest_flush_us: AtomicU64,
    pub shortest_flush_us: AtomicU64,

    // HDR Histograms for latency tracking (microseconds, 1-60 second range)
    pub flush_latency_histogram: Mutex<Histogram<u64>>,
    pub records_per_flush_histogram: Mutex<Histogram<u64>>,
    pub bytes_per_flush_histogram: Mutex<Histogram<u64>>,

    // Throughput metrics
    pub records_per_second: AtomicU64,
    pub bytes_per_second: AtomicU64,
    pub average_record_size: AtomicU64,

    // Efficiency metrics
    pub flush_efficiency_percent: AtomicU64, // (records_flushed / pending_at_start) * 100
    pub time_between_flushes_us: AtomicU64,
}

impl Default for FlushControllerMetrics {
    fn default() -> Self {
        Self {
            total_flushes: AtomicU64::new(0),
            successful_flushes: AtomicU64::new(0),
            failed_flushes: AtomicU64::new(0),
            forced_flushes: AtomicU64::new(0),
            periodic_flushes: AtomicU64::new(0),
            batched_flushes: AtomicU64::new(0),
            total_records_flushed: AtomicU64::new(0),
            total_bytes_flushed: AtomicU64::new(0),
            pending_records: AtomicU64::new(0),
            pending_bytes: AtomicU64::new(0),
            total_flush_time_us: AtomicU64::new(0),
            last_flush_timestamp: AtomicU64::new(current_timestamp()),
            longest_flush_us: AtomicU64::new(0),
            shortest_flush_us: AtomicU64::new(u64::MAX),
            flush_latency_histogram: Mutex::new(Histogram::new(3).unwrap()),
            records_per_flush_histogram: Mutex::new(Histogram::new(3).unwrap()),
            bytes_per_flush_histogram: Mutex::new(Histogram::new(3).unwrap()),
            records_per_second: AtomicU64::new(0),
            bytes_per_second: AtomicU64::new(0),
            average_record_size: AtomicU64::new(0),
            flush_efficiency_percent: AtomicU64::new(0),
            time_between_flushes_us: AtomicU64::new(0),
        }
    }
}

impl FlushControllerMetrics {
    /// Record a successful flush operation with comprehensive metrics
    pub fn record_flush(&self, latency_us: u64, records_flushed: u64, bytes_flushed: u64, flush_type: FlushType) {
        let now = current_timestamp();
        let last_flush = self.last_flush_timestamp.swap(now, Ordering::Relaxed);

        // Update counters
        self.total_flushes.fetch_add(1, Ordering::Relaxed);
        self.successful_flushes.fetch_add(1, Ordering::Relaxed);
        self.total_records_flushed.fetch_add(records_flushed, Ordering::Relaxed);
        self.total_bytes_flushed.fetch_add(bytes_flushed, Ordering::Relaxed);
        self.total_flush_time_us.fetch_add(latency_us, Ordering::Relaxed);

        // Update timing extremes
        let mut current_longest = self.longest_flush_us.load(Ordering::Relaxed);
        while latency_us > current_longest {
            match self.longest_flush_us.compare_exchange_weak(
                current_longest, latency_us, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(actual) => current_longest = actual,
            }
        }

        let mut current_shortest = self.shortest_flush_us.load(Ordering::Relaxed);
        while latency_us < current_shortest {
            match self.shortest_flush_us.compare_exchange_weak(
                current_shortest, latency_us, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(actual) => current_shortest = actual,
            }
        }

        // Update flush type counters
        match flush_type {
            FlushType::Forced => self.forced_flushes.fetch_add(1, Ordering::Relaxed),
            FlushType::Periodic => self.periodic_flushes.fetch_add(1, Ordering::Relaxed),
            FlushType::Batched => self.batched_flushes.fetch_add(1, Ordering::Relaxed),
        };

        // Calculate time between flushes
        if last_flush > 0 {
            self.time_between_flushes_us.store(now - last_flush, Ordering::Relaxed);
        }

        // Update histograms
        if let Ok(mut hist) = self.flush_latency_histogram.lock() {
            let _ = hist.record(latency_us);
        }
        if let Ok(mut hist) = self.records_per_flush_histogram.lock() {
            let _ = hist.record(records_flushed);
        }
        if let Ok(mut hist) = self.bytes_per_flush_histogram.lock() {
            let _ = hist.record(bytes_flushed);
        }

        // Calculate throughput (records and bytes per second)
        if latency_us > 0 {
            let records_per_sec = (records_flushed * 1_000_000) / latency_us;
            let bytes_per_sec = (bytes_flushed * 1_000_000) / latency_us;
            self.records_per_second.store(records_per_sec, Ordering::Relaxed);
            self.bytes_per_second.store(bytes_per_sec, Ordering::Relaxed);
        }

        // Calculate average record size
        if records_flushed > 0 {
            self.average_record_size.store(bytes_flushed / records_flushed, Ordering::Relaxed);
        }

        // Reset pending counters
        self.pending_records.store(0, Ordering::Relaxed);
        self.pending_bytes.store(0, Ordering::Relaxed);
    }

    /// Record a failed flush operation
    pub fn record_flush_failure(&self, latency_us: u64) {
        self.total_flushes.fetch_add(1, Ordering::Relaxed);
        self.failed_flushes.fetch_add(1, Ordering::Relaxed);
        self.total_flush_time_us.fetch_add(latency_us, Ordering::Relaxed);
    }

    /// Record an append operation
    pub fn record_append(&self, record_size: u64) {
        self.pending_records.fetch_add(1, Ordering::Relaxed);
        self.pending_bytes.fetch_add(record_size, Ordering::Relaxed);
    }

    /// Get percentile latency from flush histogram
    pub fn get_flush_latency_percentile(&self, percentile: f64) -> Option<u64> {
        Some(self.flush_latency_histogram.lock().ok()?.value_at_percentile(percentile))
    }

    /// Get percentile from records per flush histogram
    pub fn get_records_per_flush_percentile(&self, percentile: f64) -> Option<u64> {
        Some(self.records_per_flush_histogram.lock().ok()?.value_at_percentile(percentile))
    }

    /// Get percentile from bytes per flush histogram
    pub fn get_bytes_per_flush_percentile(&self, percentile: f64) -> Option<u64> {
        Some(self.bytes_per_flush_histogram.lock().ok()?.value_at_percentile(percentile))
    }

    /// Get current throughput in records per second
    pub fn current_records_per_second(&self) -> u64 {
        self.records_per_second.load(Ordering::Relaxed)
    }

    /// Get current throughput in bytes per second
    pub fn current_bytes_per_second(&self) -> u64 {
        self.bytes_per_second.load(Ordering::Relaxed)
    }

    /// Get flush success rate as percentage
    pub fn flush_success_rate(&self) -> f64 {
        let total = self.total_flushes.load(Ordering::Relaxed);
        if total == 0 { return 100.0; }
        let successful = self.successful_flushes.load(Ordering::Relaxed);
        (successful as f64 / total as f64) * 100.0
    }

    /// Get average flush latency in microseconds
    pub fn average_flush_latency_us(&self) -> u64 {
        let total_time = self.total_flush_time_us.load(Ordering::Relaxed);
        let total_flushes = self.total_flushes.load(Ordering::Relaxed);
        if total_flushes == 0 { return 0; }
        total_time / total_flushes
    }
}

/// Types of flush operations for metrics tracking
#[derive(Debug, Clone, Copy)]
pub enum FlushType {
    Forced,    // Manually triggered flush
    Periodic,  // Time-based flush
    Batched,   // Batch size threshold flush
}

/// Enhanced flush control with comprehensive metrics
pub struct FlushController {
    strategy: FlushStrategy,
    last_flush: Arc<AtomicU64>,
    metrics: Arc<FlushControllerMetrics>,
    #[allow(dead_code)]
    flush_queue: Arc<SegQueue<u64>>,
}

impl FlushController {
    pub fn new(strategy: FlushStrategy) -> Self {
        Self {
            strategy,
            last_flush: Arc::new(AtomicU64::new(current_timestamp())),
            metrics: Arc::new(FlushControllerMetrics::default()),
            flush_queue: Arc::new(SegQueue::new()),
        }
    }

    /// Get access to comprehensive flush metrics
    pub fn metrics(&self) -> &Arc<FlushControllerMetrics> {
        &self.metrics
    }

    pub fn should_flush(&self) -> bool {
        match &self.strategy {
            FlushStrategy::Sync => true,
            FlushStrategy::Async => false,
            FlushStrategy::Batched(size) => {
                self.metrics.pending_records.load(Ordering::Acquire) >= *size as u64
            }
            FlushStrategy::Periodic(interval_ms) => {
                let now = current_timestamp();
                let last = self.last_flush.load(Ordering::Acquire);
                (now - last) >= (*interval_ms * 1000) // Convert ms to microseconds
            }
            FlushStrategy::BatchedOrPeriodic { batch_size, interval_ms } => {
                let pending = self.metrics.pending_records.load(Ordering::Acquire);
                let now = current_timestamp();
                let last = self.last_flush.load(Ordering::Acquire);
                pending >= *batch_size as u64 || (now - last) >= (*interval_ms * 1000)
            }
        }
    }

    /// Determine the type of flush based on current conditions
    pub fn flush_type(&self) -> FlushType {
        match &self.strategy {
            FlushStrategy::Sync => FlushType::Forced,
            FlushStrategy::Async => FlushType::Forced,
            FlushStrategy::Batched(size) => {
                if self.metrics.pending_records.load(Ordering::Acquire) >= *size as u64 {
                    FlushType::Batched
                } else {
                    FlushType::Forced
                }
            }
            FlushStrategy::Periodic(_) => FlushType::Periodic,
            FlushStrategy::BatchedOrPeriodic { batch_size, interval_ms } => {
                let pending = self.metrics.pending_records.load(Ordering::Acquire);
                let now = current_timestamp();
                let last = self.last_flush.load(Ordering::Acquire);

                if pending >= *batch_size as u64 {
                    FlushType::Batched
                } else if (now - last) >= (*interval_ms * 1000) {
                    FlushType::Periodic
                } else {
                    FlushType::Forced
                }
            }
        }
    }

    pub fn record_append(&self, record_size: u64) {
        self.metrics.record_append(record_size);
    }

    pub fn record_flush(&self, latency_us: u64, records_flushed: u64, bytes_flushed: u64) {
        let flush_type = self.flush_type();
        self.metrics.record_flush(latency_us, records_flushed, bytes_flushed, flush_type);
        self.last_flush.store(current_timestamp(), Ordering::Relaxed);
    }

    pub fn record_flush_failure(&self, latency_us: u64) {
        self.metrics.record_flush_failure(latency_us);
    }
}

/// Manages the currently active, writable part of the log.
pub struct Appender<FS: FileSystem> {
    /// File name for the segment.
    path: String,
    /// File system reference.
    fs: Arc<FS>,
    /// The underlying file handle.
    file_handle: FS::Handle,
    /// Mutable memory map of the file.
    mmap: MmapMut,
    /// The current write offset within this segment.
    write_offset: u64,
    /// The base ID for the first record in this segment.
    pub base_id: u64,
    /// The configured size of this segment.
    size: u64,
    /// The ID of the last record appended to the mmap (uncommitted).
    uncommitted_tail_id: AtomicU64,
    /// The ID of the last record flushed to disk (committed).
    committed_tail_id: AtomicU64,
    /// Flush controller for managing flush strategies.
    flush_controller: FlushController,
    /// Binary index entries for this segment.
    index_entries: Vec<BinaryIndexEntry>,
    /// Index storage configuration.
    index_storage: IndexStorage,
    /// Data checksum accumulator for InSegment indexing.
    data_checksum: Crc32Hasher,
}

impl<FS: FileSystem> Appender<FS> {
    /// Creates a new, writable segment file.
    pub async fn create_new(
        path: &str,
        fs: Arc<FS>,
        size: u64,
        base_id: u64,
        flush_strategy: FlushStrategy,
        index_storage: IndexStorage,
    ) -> AofResult<Self> {
        ensure_segment_size_is_valid(size)?;
        let file_handle = fs.create_file(path).await?;
        file_handle.set_size(size).await?;
        let mmap = file_handle.memory_map_mut().await?.ok_or_else(||
            AofError::FileSystem("Memory mapping not supported".to_string()))?;

        Ok(Appender {
            path: path.to_string(),
            fs,
            file_handle,
            mmap,
            write_offset: 0,
            base_id,
            size,
            uncommitted_tail_id: AtomicU64::new(base_id.saturating_sub(1)),
            committed_tail_id: AtomicU64::new(base_id.saturating_sub(1)),
            flush_controller: FlushController::new(flush_strategy),
            index_entries: Vec::new(),
            index_storage,
            data_checksum: Crc32Hasher::new(),
        })
    }

    /// Loads an existing segment file as writable.
    pub async fn load_existing(
        path: &str,
        fs: Arc<FS>,
        size: u64,
        base_id: u64,
        flush_strategy: FlushStrategy,
        index_storage: IndexStorage,
    ) -> AofResult<Self> {
        ensure_segment_size_is_valid(size)?;
        let file_handle = fs.open_file_mut(path).await?;
        file_handle.set_size(size).await?;
        let mmap = file_handle.memory_map_mut().await?.ok_or_else(||
            AofError::FileSystem("Memory mapping not supported".to_string()))?;

        let (write_offset, initial_tail_id, index, data_checksum) = Self::scan_existing_data(&mmap, size, base_id)?;

        Ok(Appender {
            path: path.to_string(),
            fs,
            file_handle,
            mmap,
            write_offset,
            base_id,
            size,
            uncommitted_tail_id: AtomicU64::new(initial_tail_id),
            committed_tail_id: AtomicU64::new(initial_tail_id),
            flush_controller: FlushController::new(flush_strategy),
            index_entries: index,
            index_storage,
            data_checksum,
        })
    }

    /// Scans existing data in mmap and builds binary index
    fn scan_existing_data(mmap: &MmapMut, size: u64, base_id: u64) -> AofResult<(u64, u64, Vec<BinaryIndexEntry>, Crc32Hasher)> {
        let mut write_offset = 0;
        let mut current_offset = 0;
        let mut index_entries = Vec::new();
        let mut last_id = base_id.saturating_sub(1);
        let mut data_checksum = Crc32Hasher::new();

        while current_offset < size {
            if current_offset + RecordHeader::size() as u64 > size {
                break;
            }

            let header_bytes = &mmap[current_offset as usize..(current_offset + RecordHeader::size() as u64) as usize];
            let header = RecordHeader::from_bytes(header_bytes)
                .map_err(|e| AofError::CorruptedRecord(format!("Failed to deserialize header: {}", e)))?;

            if header.id == 0 && header.size == 0 && current_offset == 0 {
                break; // Empty segment
            }

            let record_end = current_offset + RecordHeader::size() as u64 + header.size as u64;
            if record_end > size {
                return Err(AofError::CorruptedRecord("Record extends beyond segment size".to_string()));
            }

            // Verify checksum
            let data_start = current_offset + RecordHeader::size() as u64;
            let data = &mmap[data_start as usize..record_end as usize];
            if !header.verify_checksum(data) {
                return Err(AofError::CorruptedRecord(format!("Checksum verification failed for record {}", header.id)));
            }

            index_entries.push(BinaryIndexEntry::new(header.id, current_offset));

            // Update data checksum with header and data
            let header_bytes = &mmap[current_offset as usize..(current_offset + RecordHeader::size() as u64) as usize];
            let data_bytes = &mmap[data_start as usize..record_end as usize];
            data_checksum.update(header_bytes);
            data_checksum.update(data_bytes);

            last_id = header.id;
            write_offset = record_end;
            current_offset = record_end;
        }

        Ok((write_offset, last_id, index_entries, data_checksum))
    }

    /// Tries to append a record to the segment using safe serialization.
    pub fn append(&mut self, data: &[u8], id: u64) -> AofResult<bool> {
        let header = RecordHeader::new(id, data);
        let header_bytes = header.to_bytes()
            .map_err(|e| AofError::Serialization(format!("Failed to serialize header: {}", e)))?;

        let record_size = header_bytes.len() as u64 + data.len() as u64;
        if self.write_offset + record_size > self.size {
            return Ok(false); // Segment is full
        }

        // Write header safely
        let header_end = self.write_offset + header_bytes.len() as u64;
        self.mmap[self.write_offset as usize..header_end as usize]
            .copy_from_slice(&header_bytes);

        // Write data
        let data_start = header_end;
        let data_end = data_start + data.len() as u64;
        self.mmap[data_start as usize..data_end as usize]
            .copy_from_slice(data);

        // Update index
        self.index_entries.push(BinaryIndexEntry::new(id, self.write_offset));

        // Update data checksum with header and data
        self.data_checksum.update(&header_bytes);
        self.data_checksum.update(data);

        self.write_offset = data_end;

        // Update uncommitted tail ID
        self.uncommitted_tail_id.store(id, Ordering::SeqCst);

        // Record append for flush control
        let total_record_size = header_bytes.len() as u64 + data.len() as u64;
        self.flush_controller.record_append(total_record_size);

        Ok(true)
    }

    /// Flushes the mmap content to disk.
    pub fn flush(&mut self) -> AofResult<u64> {
        let start_time = Instant::now();
        let pending_records = self.flush_controller.metrics().pending_records.load(Ordering::Relaxed);
        let pending_bytes = self.flush_controller.metrics().pending_bytes.load(Ordering::Relaxed);

        match self.mmap.flush() {
            Ok(_) => {
                let last_committed_id = self.uncommitted_tail_id.load(Ordering::SeqCst);
                self.committed_tail_id.store(last_committed_id, Ordering::SeqCst);

                let latency_us = start_time.elapsed().as_micros() as u64;
                self.flush_controller.record_flush(latency_us, pending_records, pending_bytes);
                Ok(last_committed_id)
            }
            Err(e) => {
                let latency_us = start_time.elapsed().as_micros() as u64;
                self.flush_controller.record_flush_failure(latency_us);
                Err(AofError::FileSystem(format!("Flush failed: {}", e)))
            }
        }
    }

    /// Checks if flush is needed based on strategy
    pub fn should_flush(&self) -> bool {
        self.flush_controller.should_flush()
    }

    /// Get current usage ratio for pre-allocation decisions
    pub fn usage_ratio(&self) -> f32 {
        self.write_offset as f32 / self.size as f32
    }

    /// Finalizes the writer, flushing all data to disk with binary index persistence and truncation.
    pub async fn finalize(mut self) -> AofResult<(String, SegmentMetadata, Vec<BinaryIndexEntry>)> {
        // Handle index persistence based on configuration
        match self.index_storage {
            IndexStorage::InSegment => {
                self.persist_index_in_segment().await?;
            }
            IndexStorage::SeparateFile => {
                self.persist_index_to_separate_file().await?;
            }
            IndexStorage::Hybrid => {
                // Write to both in-segment and separate file for redundancy
                self.persist_index_in_segment().await?;
                self.persist_index_to_separate_file().await?;
            }
            IndexStorage::InMemory => {
                // No persistence needed
            }
        }

        self.mmap.flush()?;
        self.file_handle.sync().await?;

        let data_checksum = self.data_checksum.clone().finalize();
        let last_id = self.index_entries.last()
            .map(|entry| entry.record_id)
            .unwrap_or(self.base_id.saturating_sub(1));

        let metadata = SegmentMetadata {
            base_id: self.base_id,
            last_id,
            record_count: self.index_entries.len() as u64,
            created_at: current_timestamp(),
            size: self.write_offset, // Actual data size, not file size
            checksum: data_checksum,
            compressed: false,
            encrypted: false,
        };

        Ok((self.path, metadata, self.index_entries))
    }

    /// Persist binary index data within the segment file and truncate unused space
    async fn persist_index_in_segment(&mut self) -> AofResult<()> {
        if self.index_entries.is_empty() {
            return Ok(());
        }

        // Serialize binary index entries
        let mut index_data = Vec::with_capacity(self.index_entries.len() * BinaryIndexEntry::SIZE);
        for entry in &self.index_entries {
            index_data.extend_from_slice(&entry.to_bytes());
        }

        // Calculate space needed for index and footer
        let footer_size = SegmentFooter::size() as u64;
        let index_size = index_data.len() as u64;
        let total_needed = index_size + footer_size;
        let available_space = self.size.saturating_sub(self.write_offset);

        if total_needed > available_space {
            return Err(AofError::IndexError(
                "Not enough space in segment for binary index data".to_string()
            ));
        }

        // Write binary index data after the records
        let index_offset = self.write_offset;
        let index_start = index_offset as usize;
        let index_end = index_start + index_data.len();

        self.mmap[index_start..index_end].copy_from_slice(&index_data);

        // Calculate checksums
        let data_checksum = self.data_checksum.clone().finalize();
        let mut index_hasher = Crc32Hasher::new();
        index_hasher.update(&index_data);
        let index_checksum = index_hasher.finalize();

        // Create and write footer
        let footer = SegmentFooter::new(
            index_offset,
            index_size,
            self.index_entries.len() as u64,
            data_checksum,
            index_checksum,
        );

        let footer_data = footer.serialize()?;
        let footer_start = (index_offset + index_size) as usize;
        let footer_end = footer_start + footer_data.len();

        self.mmap[footer_start..footer_end].copy_from_slice(&footer_data);

        // Calculate the final file size (data + index + footer)
        let final_size = index_offset + index_size + footer_size;

        // Truncate the file to remove unused space
        if final_size < self.size {
            self.file_handle.set_size(final_size).await?;
        }

        Ok(())
    }

    /// Persist binary index data to a separate .idx file
    async fn persist_index_to_separate_file(&self) -> AofResult<()> {
        if self.index_entries.is_empty() {
            return Ok(());
        }

        let index_path = format!("{}.idx", self.path);

        // Serialize binary index entries
        let mut index_data = Vec::with_capacity(self.index_entries.len() * BinaryIndexEntry::SIZE);
        for entry in &self.index_entries {
            index_data.extend_from_slice(&entry.to_bytes());
        }

        let mut index_handle = self.fs.create_file(&index_path).await?;
        index_handle.write_at(0, &index_data).await?;
        index_handle.sync().await?;

        Ok(())
    }

    /// Reads a record at a specific logical ID using binary index.
    pub fn read(&self, id: u64) -> AofResult<Option<Vec<u8>>> {
        if id < self.base_id {
            return Ok(None);
        }

        // Find the record in the binary index
        let offset = self.index_entries.iter()
            .find(|entry| entry.record_id == id)
            .map(|entry| entry.file_offset);

        if let Some(offset) = offset {
            let header_bytes = &self.mmap[offset as usize..(offset + RecordHeader::size() as u64) as usize];
            let header = RecordHeader::from_bytes(header_bytes)
                .map_err(|e| AofError::CorruptedRecord(format!("Failed to deserialize header: {}", e)))?;

            let data_start = offset + RecordHeader::size() as u64;
            let data_end = data_start + header.size as u64;

            if data_end > self.write_offset {
                return Err(AofError::CorruptedRecord("Record data extends beyond write offset".to_string()));
            }

            let data_slice = &self.mmap[data_start as usize..data_end as usize];

            // Verify checksum
            if !header.verify_checksum(data_slice) {
                return Err(AofError::CorruptedRecord(format!("Checksum verification failed for record {}", id)));
            }

            return Ok(Some(data_slice.to_vec()));
        }

        Ok(None)
    }

    /// Gets the last record ID in this writable segment.
    pub fn last_record_id(&self) -> Option<u64> {
        self.index_entries.last().map(|entry| entry.record_id)
    }

    /// Get record count in this segment
    #[allow(dead_code)]
    pub fn record_count(&self) -> u64 {
        self.index_entries.len() as u64
    }

    /// Get current write offset
    pub fn write_offset(&self) -> u64 {
        self.write_offset
    }

    /// Get current index entry at position
    pub fn get_index_entry(&self, index: usize) -> Option<&BinaryIndexEntry> {
        self.index_entries.get(index)
    }

    /// Get index entries count
    pub fn index_entries_len(&self) -> usize {
        self.index_entries.len()
    }
}