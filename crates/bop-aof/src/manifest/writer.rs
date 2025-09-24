use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use memmap2::MmapMut;

use super::chunk::{CHUNK_HEADER_LEN, ChunkHeader, crc64};
use super::record::{ManifestRecord, ManifestRecordPayload};
use crate::config::SegmentId;
use crate::error::{AofError, AofResult};
const DEFAULT_GROWTH_STEP: usize = 64 * 1024; // 64 KiB

/// Configuration for manifest log writers.
#[derive(Debug, Clone)]
pub struct ManifestLogWriterConfig {
    pub chunk_capacity_bytes: usize,
    pub rotation_period: Duration,
    pub growth_step_bytes: usize,
    pub enabled: bool,
}

impl Default for ManifestLogWriterConfig {
    fn default() -> Self {
        Self {
            chunk_capacity_bytes: 256 * 1024,
            rotation_period: Duration::from_millis(250),
            growth_step_bytes: DEFAULT_GROWTH_STEP,
            enabled: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SealedChunkHandle {
    pub chunk_index: u64,
    pub committed_len: usize,
    pub chunk_crc64: u64,
    pub path: PathBuf,
}

/// RAII wrapper around an mmap-backed chunk file.
pub struct ChunkHandle {
    file: File,
    mmap: MmapMut,
    header: ChunkHeader,
    capacity: usize,
    path: PathBuf,
}

impl ChunkHandle {
    pub fn create(
        dir: &Path,
        chunk_index: u64,
        base_record: u64,
        capacity: usize,
    ) -> AofResult<Self> {
        fs::create_dir_all(dir)?;
        let final_path = dir.join(format!("{chunk_index:020}.mlog"));

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&final_path)?;
        file.set_len(capacity as u64)?;

        let mut mmap = unsafe { MmapMut::map_mut(&file)? };
        let header = ChunkHeader::new(chunk_index, base_record);
        header.write_into(&mut mmap[..CHUNK_HEADER_LEN]);
        mmap.flush_range(0, CHUNK_HEADER_LEN)?;

        Ok(Self {
            file,
            mmap,
            header,
            capacity,
            path: final_path,
        })
    }

    pub fn tail_offset(&self) -> usize {
        self.header.tail_offset
    }

    pub fn committed_len(&self) -> usize {
        self.header.committed_len
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn chunk_index(&self) -> u64 {
        self.header.chunk_index
    }

    pub fn ensure_capacity(&mut self, required_tail: usize, growth_step: usize) -> AofResult<()> {
        if required_tail <= self.capacity {
            return Ok(());
        }
        let growth_step = growth_step.max(DEFAULT_GROWTH_STEP);
        let mut new_capacity = self.capacity;
        while new_capacity < required_tail {
            new_capacity = new_capacity.saturating_add(growth_step);
        }
        self.file.set_len(new_capacity as u64)?;
        self.file.seek(SeekFrom::Start(new_capacity as u64 - 1))?;
        self.file.write_all(&[0])?;
        self.file.flush()?;
        self.mmap = unsafe { MmapMut::map_mut(&self.file)? };
        self.capacity = new_capacity;
        Ok(())
    }

    pub fn append_bytes(&mut self, bytes: &[u8]) -> AofResult<ChunkCursor> {
        let write_end = self
            .header
            .tail_offset
            .checked_add(bytes.len())
            .ok_or_else(|| AofError::other("manifest chunk offset overflow"))?;
        if write_end > self.capacity {
            return Err(AofError::other("manifest chunk capacity exceeded"));
        }
        let range = self.header.tail_offset..write_end;
        self.mmap[range.clone()].copy_from_slice(bytes);
        self.header.tail_offset = write_end;
        Ok(ChunkCursor {
            chunk_index: self.header.chunk_index,
            offset: range.start,
            len: bytes.len(),
        })
    }

    pub fn commit(&mut self) -> AofResult<()> {
        if self.header.tail_offset < CHUNK_HEADER_LEN {
            return Err(AofError::other("manifest chunk tail before header"));
        }
        let committed_len = self.header.tail_offset;
        let data_len = committed_len.saturating_sub(CHUNK_HEADER_LEN);
        if data_len > 0 {
            self.mmap.flush_range(CHUNK_HEADER_LEN, data_len)?;
        }
        let chunk_crc = crc64(&self.mmap[CHUNK_HEADER_LEN..committed_len]);
        self.header.committed_len = committed_len;
        self.header.chunk_crc64 = chunk_crc;
        self.header.write_into(&mut self.mmap[..CHUNK_HEADER_LEN]);
        self.mmap.flush_range(0, CHUNK_HEADER_LEN)?;
        self.file.sync_data()?;
        Ok(())
    }

    pub fn seal(mut self) -> AofResult<SealedChunkHandle> {
        self.header.flags |= ChunkHeader::FLAG_SEALED;
        self.header.write_into(&mut self.mmap[..CHUNK_HEADER_LEN]);
        self.mmap.flush_range(0, CHUNK_HEADER_LEN)?;
        self.file.sync_data()?;
        Ok(SealedChunkHandle {
            chunk_index: self.header.chunk_index,
            committed_len: self.header.committed_len,
            chunk_crc64: self.header.chunk_crc64,
            path: self.path,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChunkCursor {
    pub chunk_index: u64,
    pub offset: usize,
    pub len: usize,
}

/// Chunked manifest writer that matches the MAN1 scaffolding plan.
pub struct ManifestLogWriter {
    dir: PathBuf,
    config: ManifestLogWriterConfig,
    next_chunk_index: u64,
    next_record_ordinal: u64,
    last_commit: Instant,
    active: Option<ChunkHandle>,
    sealed_chunks: Vec<SealedChunkHandle>,
}

impl ManifestLogWriter {
    pub fn new(
        base_dir: PathBuf,
        stream_id: u64,
        config: ManifestLogWriterConfig,
    ) -> AofResult<Self> {
        fs::create_dir_all(&base_dir)?;
        let stream_dir = base_dir.join(format!("{stream_id:020}"));
        fs::create_dir_all(&stream_dir)?;
        Ok(Self {
            dir: stream_dir,
            config,
            next_chunk_index: 0,
            next_record_ordinal: 0,
            last_commit: Instant::now(),
            active: None,
            sealed_chunks: Vec::new(),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn append_record(&mut self, header: &[u8], payload: &[u8]) -> AofResult<ChunkCursor> {
        if !self.config.enabled {
            return Err(AofError::other("manifest log disabled"));
        }
        if header.len() != 40 {
            return Err(AofError::invalid_config("record header must be 40 bytes"));
        }
        let total = header
            .len()
            .checked_add(payload.len())
            .ok_or_else(|| AofError::other("record length overflow"))?;
        self.ensure_active_chunk(total)?;
        let mut record_buf = Vec::with_capacity(total);
        record_buf.extend_from_slice(header);
        record_buf.extend_from_slice(payload);
        let cursor = {
            let chunk = self.active.as_mut().expect("active chunk present");
            chunk.append_bytes(&record_buf)?
        };
        self.next_record_ordinal = self
            .next_record_ordinal
            .checked_add(1)
            .ok_or_else(|| AofError::other("record ordinal overflow"))?;
        Ok(cursor)
    }

    pub fn append(&mut self, record: &ManifestRecord) -> AofResult<ChunkCursor> {
        let (header, payload) = record.encode()?;
        self.append_record(&header, &payload)
    }

    pub fn append_with_timestamp(
        &mut self,
        segment_id: SegmentId,
        logical_offset: u64,
        timestamp_ms: u64,
        payload: ManifestRecordPayload,
    ) -> AofResult<ChunkCursor> {
        let record = ManifestRecord::new(segment_id, logical_offset, timestamp_ms, payload);
        self.append(&record)
    }

    pub fn append_now(
        &mut self,
        segment_id: SegmentId,
        logical_offset: u64,
        payload: ManifestRecordPayload,
    ) -> AofResult<ChunkCursor> {
        let timestamp_ms = current_epoch_ms();
        self.append_with_timestamp(segment_id, logical_offset, timestamp_ms, payload)
    }

    pub fn ensure_capacity(&mut self, additional_bytes: usize) -> AofResult<()> {
        if let Some(chunk) = self.active.as_mut() {
            let required_tail = chunk
                .tail_offset()
                .checked_add(additional_bytes)
                .ok_or_else(|| AofError::other("manifest chunk tail overflow"))?;
            chunk.ensure_capacity(required_tail, self.config.growth_step_bytes)?;
        }
        Ok(())
    }

    pub fn commit(&mut self) -> AofResult<()> {
        if let Some(chunk) = self.active.as_mut() {
            chunk.commit()?;
        }
        self.last_commit = Instant::now();
        Ok(())
    }

    pub fn maybe_rotate(&mut self, max_record_len: usize) -> AofResult<bool> {
        if self.active.is_none() {
            return Ok(false);
        }
        let should_rotate = {
            let chunk = self.active.as_ref().unwrap();
            let tail = chunk.tail_offset();
            let capacity_trigger = tail + max_record_len > self.config.chunk_capacity_bytes;
            let time_trigger = self.last_commit.elapsed() >= self.config.rotation_period;
            capacity_trigger || time_trigger
        };
        if should_rotate {
            self.rotate_current()?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn rotate_current(&mut self) -> AofResult<()> {
        if let Some(chunk) = self.active.take() {
            let sealed = chunk.seal()?;
            self.sealed_chunks.push(sealed);
            self.next_chunk_index = self
                .next_chunk_index
                .checked_add(1)
                .ok_or_else(|| AofError::other("chunk index overflow"))?;
        }
        Ok(())
    }

    pub fn take_sealed_chunks(&mut self) -> Vec<SealedChunkHandle> {
        std::mem::take(&mut self.sealed_chunks)
    }

    fn ensure_active_chunk(&mut self, record_len: usize) -> AofResult<&mut ChunkHandle> {
        if self.active.is_none() {
            let chunk = ChunkHandle::create(
                &self.dir,
                self.next_chunk_index,
                self.next_record_ordinal,
                self.config.chunk_capacity_bytes,
            )?;
            self.active = Some(chunk);
        }
        let chunk = self.active.as_mut().unwrap();
        let required_tail = chunk
            .tail_offset()
            .checked_add(record_len)
            .ok_or_else(|| AofError::other("manifest chunk tail overflow"))?;
        chunk.ensure_capacity(required_tail, self.config.growth_step_bytes)?;
        Ok(chunk)
    }
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
