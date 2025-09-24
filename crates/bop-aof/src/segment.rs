use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::ptr;
use std::slice;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};

use crc64fast_nvme::Digest;
use memmap2::{Mmap, MmapMut};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

use super::config::{RecordId, SegmentId};
use super::error::{AofError, AofResult};
use super::flush::SegmentFlushState;
use super::fs::create_fixed_size_file;

pub(crate) const SEGMENT_HEADER_SIZE: u32 = 64;
pub(crate) const SEGMENT_FOOTER_SIZE: u32 = 96;
pub(crate) const RECORD_HEADER_SIZE: u32 = 24;
const SEGMENT_MAGIC: u32 = 0x414F_4632; // "AOF2"
const SEGMENT_VERSION: u16 = 2;
const SEGMENT_FOOTER_MAGIC: u32 = 0x464F_4F54; // "FOOT"

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentStatus {
    Active,
    Finalized,
}

#[derive(Debug, Clone, Copy)]
pub struct SegmentAppendResult {
    pub segment_offset: u32,
    pub last_offset: u64,
    pub logical_size: u32,
    pub is_full: bool,
}

#[must_use = "call complete() to finalize the reserved append"]
pub struct SegmentReservation {
    segment: Arc<Segment>,
    offset: u32,
    payload_len: u32,
    next_size: u32,
    timestamp: u64,
    completed: bool,
}

impl SegmentReservation {
    fn new(
        segment: Arc<Segment>,
        offset: u32,
        payload_len: u32,
        next_size: u32,
        timestamp: u64,
    ) -> Self {
        Self {
            segment,
            offset,
            payload_len,
            next_size,
            timestamp,
            completed: false,
        }
    }

    pub fn segment(&self) -> &Arc<Segment> {
        &self.segment
    }

    pub fn payload_len(&self) -> usize {
        self.payload_len as usize
    }

    pub fn payload_mut(&mut self) -> AofResult<&mut [u8]> {
        if self.completed {
            return Err(AofError::InvalidState(
                "segment reservation already completed".to_string(),
            ));
        }
        let start = (self.offset + RECORD_HEADER_SIZE) as usize;
        let end = start + self.payload_len as usize;
        self.segment.data.slice_mut(start..end)
    }

    pub fn complete(mut self) -> AofResult<SegmentAppendResult> {
        if self.completed {
            return Err(AofError::InvalidState(
                "segment reservation already completed".to_string(),
            ));
        }

        let start = (self.offset + RECORD_HEADER_SIZE) as usize;
        let end = start + self.payload_len as usize;
        let payload = self.segment.data.read_slice(start..end)?;

        let mut digest = Digest::new();
        digest.write(payload);
        let checksum = fold_crc64(digest.sum64());

        let mut header = [0u8; RECORD_HEADER_SIZE as usize];
        header[0..4].copy_from_slice(&self.payload_len.to_le_bytes());
        header[4..8].copy_from_slice(&checksum.to_le_bytes());
        header[8..16].copy_from_slice(&self.timestamp.to_le_bytes());
        let record_ext_id = self.segment.ext_id.load(Ordering::Acquire);
        header[16..24].copy_from_slice(&record_ext_id.to_le_bytes());

        self.segment
            .data
            .write_bytes(self.offset as usize, &header)?;

        self.segment.data.update_size(self.next_size as u64);
        self.segment.record_count.fetch_add(1, Ordering::AcqRel);
        self.segment
            .last_timestamp
            .store(self.timestamp, Ordering::Release);
        self.segment
            .last_record_offset
            .store(self.offset, Ordering::Release);
        self.segment.size.store(self.next_size, Ordering::Release);
        self.segment.flush_state.request_flush(self.next_size);

        let last_offset = self.segment.base_offset + self.next_size as u64;
        let is_full = self.next_size >= self.segment.usable_limit();

        self.completed = true;

        Ok(SegmentAppendResult {
            segment_offset: self.offset,
            last_offset,
            logical_size: self.next_size,
            is_full,
        })
    }
}

impl Drop for SegmentReservation {
    fn drop(&mut self) {
        debug_assert!(
            self.completed,
            "segment reservation dropped without completing the append"
        );
    }
}

pub(crate) struct RecordHeader {
    pub length: u32,
    pub checksum: u32,
    pub timestamp: u64,
    pub ext_id: u64,
}

pub(crate) struct SegmentRecordSlice<'a> {
    pub header: RecordHeader,
    pub payload: &'a [u8],
    pub total_len: u32,
}

#[derive(Debug, Clone)]
pub struct SegmentHeaderInfo {
    pub segment_index: u32,
    pub base_record_count: u64,
    pub base_offset: u64,
    pub max_size: u32,
    pub created_at: i64,
    pub base_timestamp: u64,
    pub ext_id: u64,
}

#[derive(Debug, Clone)]
pub struct SegmentScan {
    pub header: SegmentHeaderInfo,
    pub footer: Option<SegmentFooter>,
    pub record_count: u64,
    pub last_record_id: RecordId,
    pub last_timestamp: u64,
    pub logical_size: u32,
    pub checksum: u32,
    pub truncated: bool,
}

impl SegmentScan {
    pub fn from_trusted_footer(
        header: SegmentHeaderInfo,
        footer: SegmentFooter,
    ) -> AofResult<Self> {
        let relative_size = footer
            .durable_bytes
            .checked_sub(header.base_offset)
            .ok_or_else(|| {
                AofError::Corruption(format!(
                    "footer durable_bytes precedes base_offset for segment {}",
                    header.segment_index
                ))
            })?;
        if relative_size > header.max_size as u64 {
            return Err(AofError::Corruption(format!(
                "footer durable_bytes {} exceeds segment max_size {} for segment {}",
                footer.durable_bytes, header.max_size, header.segment_index
            )));
        }
        let logical_size = relative_size as u32;
        Ok(SegmentScan {
            header,
            footer: Some(footer),
            record_count: footer.record_count,
            last_record_id: footer.last_record_id,
            last_timestamp: footer.last_timestamp,
            logical_size,
            checksum: footer.checksum,
            truncated: false,
        })
    }
}

pub struct Segment {
    index: u32,
    base_offset: u64,
    base_record_count: u64,
    max_size: u32,
    created_at: i64,
    ext_id: AtomicU64,

    header_written: AtomicBool,
    sealed: AtomicBool,
    record_count: AtomicU32,
    size: AtomicU32,
    durable_size: AtomicU32,
    last_timestamp: AtomicU64,
    last_record_offset: AtomicU32,

    data: Arc<SegmentData>,
    flush_state: Arc<SegmentFlushState>,
    #[cfg(test)]
    flush_fail_injections: AtomicU32,
}

impl Segment {
    pub fn create_active(
        segment_id: SegmentId,
        base_offset: u64,
        base_record_count: u64,
        created_at: i64,
        max_bytes: u64,
        path: &Path,
    ) -> AofResult<Self> {
        if max_bytes > u32::MAX as u64 {
            return Err(AofError::invalid_config(
                "segment_max_bytes exceeds u32::MAX for this build",
            ));
        }

        let max_size = max_bytes as u32;
        if max_size <= SEGMENT_HEADER_SIZE + SEGMENT_FOOTER_SIZE {
            return Err(AofError::invalid_config(
                "segment_max_bytes must exceed reserved header and footer",
            ));
        }

        let data = SegmentData::create(path, max_size)?;
        let flush_state = SegmentFlushState::new(segment_id);

        Ok(Self {
            index: segment_id.as_u32(),
            base_offset,
            base_record_count,
            max_size,
            created_at,
            ext_id: AtomicU64::new(0),
            header_written: AtomicBool::new(false),
            sealed: AtomicBool::new(false),
            record_count: AtomicU32::new(0),
            size: AtomicU32::new(0),
            durable_size: AtomicU32::new(0),
            last_timestamp: AtomicU64::new(0),
            last_record_offset: AtomicU32::new(SEGMENT_HEADER_SIZE),
            data: Arc::new(data),
            flush_state,
            #[cfg(test)]
            flush_fail_injections: AtomicU32::new(0),
        })
    }

    #[inline]
    pub fn id(&self) -> SegmentId {
        SegmentId::new(self.index as u64)
    }

    #[inline]
    pub fn base_offset(&self) -> u64 {
        self.base_offset
    }

    #[inline]
    pub fn base_record_count(&self) -> u64 {
        self.base_record_count
    }

    #[inline]
    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    #[inline]
    pub fn max_size(&self) -> u32 {
        self.max_size
    }

    #[inline]
    pub fn flush_state(&self) -> Arc<SegmentFlushState> {
        self.flush_state.clone()
    }

    #[inline]
    pub fn path(&self) -> &Path {
        self.data.path()
    }

    pub fn load_header(path: &Path) -> AofResult<SegmentHeaderInfo> {
        let mut buf = [0u8; SEGMENT_HEADER_SIZE as usize];
        let mut file = File::open(path).map_err(AofError::from)?;
        file.read_exact(&mut buf).map_err(AofError::from)?;
        let header = SegmentHeader::decode(&buf).ok_or_else(|| {
            AofError::Corruption(format!("invalid segment header: {}", path.display()))
        })?;
        Ok(header.into())
    }

    pub fn from_recovered(path: &Path, scan: &SegmentScan) -> AofResult<Self> {
        let header = &scan.header;
        if scan.logical_size > header.max_size {
            return Err(AofError::Corruption(format!(
                "segment {} logical size {} exceeds max {}",
                path.display(),
                scan.logical_size,
                header.max_size
            )));
        }

        if scan.record_count > u32::MAX as u64 {
            return Err(AofError::Corruption(format!(
                "segment {} record count {} exceeds supported limit",
                path.display(),
                scan.record_count
            )));
        }

        let writable = scan.footer.is_none();
        let logical_size = scan.logical_size;
        let durable_size = if let Some(footer) = &scan.footer {
            footer
                .durable_bytes
                .saturating_sub(header.base_offset)
                .min(header.max_size as u64) as u32
        } else {
            logical_size
        };

        let data = SegmentData::open(path, header.max_size, logical_size, writable)?;
        let flush_state = SegmentFlushState::with_progress(
            SegmentId::new(header.segment_index as u64),
            logical_size,
            durable_size,
        );

        Ok(Self {
            index: header.segment_index,
            base_offset: header.base_offset,
            base_record_count: header.base_record_count,
            max_size: header.max_size,
            created_at: header.created_at,
            ext_id: AtomicU64::new(header.ext_id),
            header_written: AtomicBool::new(true),
            sealed: AtomicBool::new(!writable),
            record_count: AtomicU32::new(scan.record_count as u32),
            size: AtomicU32::new(logical_size),
            durable_size: AtomicU32::new(durable_size.min(logical_size)),
            last_timestamp: AtomicU64::new(scan.last_timestamp),
            last_record_offset: AtomicU32::new(scan.last_record_id.segment_offset()),
            data: Arc::new(data),
            flush_state,
            #[cfg(test)]
            flush_fail_injections: AtomicU32::new(0),
        })
    }

    pub fn scan_tail(path: &Path) -> AofResult<SegmentScan> {
        let file = File::open(path).map_err(AofError::from)?;
        let mmap = unsafe { Mmap::map(&file).map_err(AofError::from)? };
        if mmap.len() < SEGMENT_HEADER_SIZE as usize {
            return Err(AofError::Corruption(format!(
                "segment {} too small for header",
                path.display()
            )));
        }
        let header_bytes = &mmap[..SEGMENT_HEADER_SIZE as usize];
        let header = SegmentHeader::decode(header_bytes).ok_or_else(|| {
            AofError::Corruption(format!("segment {} has invalid header", path.display()))
        })?;
        if mmap.len() < header.max_size as usize {
            return Err(AofError::Corruption(format!(
                "segment {} truncated: expected {} bytes, found {}",
                path.display(),
                header.max_size,
                mmap.len()
            )));
        }
        let usable_limit = (header.max_size - SEGMENT_FOOTER_SIZE) as usize;
        let mut truncated = false;
        let mut footer = None;
        let summary = if header.max_size >= SEGMENT_FOOTER_SIZE {
            let footer_offset = (header.max_size - SEGMENT_FOOTER_SIZE) as usize;
            let footer_bytes = &mmap[footer_offset..footer_offset + SEGMENT_FOOTER_SIZE as usize];
            if let Some(candidate) = SegmentFooter::decode(footer_bytes) {
                let durable_len = candidate
                    .durable_bytes
                    .saturating_sub(header.base_offset)
                    .min(header.max_size as u64) as usize;
                let durable_limit = durable_len.max(SEGMENT_HEADER_SIZE as usize);
                let attempt = scan_records(&mmap, &header, durable_limit)?;
                if !attempt.truncated
                    && attempt.record_count == candidate.record_count
                    && attempt.last_record_id.as_u64() == candidate.last_record_id.as_u64()
                    && attempt.checksum == candidate.checksum
                {
                    footer = Some(candidate);
                    attempt
                } else {
                    truncated = true;
                    scan_records(&mmap, &header, usable_limit)?
                }
            } else {
                scan_records(&mmap, &header, usable_limit)?
            }
        } else {
            scan_records(&mmap, &header, usable_limit)?
        };

        let info: SegmentHeaderInfo = header.into();
        Ok(SegmentScan {
            header: info,
            footer,
            record_count: summary.record_count,
            last_record_id: summary.last_record_id,
            last_timestamp: summary.last_timestamp,
            logical_size: summary.consumed as u32,
            checksum: summary.checksum,
            truncated: truncated || summary.truncated,
        })
    }

    #[inline]
    fn usable_limit(&self) -> u32 {
        debug_assert!(self.max_size > SEGMENT_FOOTER_SIZE);
        self.max_size - SEGMENT_FOOTER_SIZE
    }

    #[inline]
    fn max_payload_capacity(&self) -> u32 {
        self.usable_limit().saturating_sub(SEGMENT_HEADER_SIZE)
    }

    #[inline]
    pub fn footer_offset(&self) -> u32 {
        self.max_size - SEGMENT_FOOTER_SIZE
    }

    pub fn append_record(&self, payload: &[u8], timestamp: u64) -> AofResult<SegmentAppendResult> {
        if self.is_sealed() {
            return Err(AofError::InvalidState(
                "cannot append to sealed segment".to_string(),
            ));
        }
        let usable_limit = self.usable_limit();
        let max_payload_capacity = self.max_payload_capacity() as u64;
        if (payload.len() as u64) + (RECORD_HEADER_SIZE as u64) > max_payload_capacity {
            return Err(AofError::SegmentFull(max_payload_capacity));
        }

        if self.record_count.load(Ordering::Acquire) == 0 {
            self.last_timestamp.store(timestamp, Ordering::Release);
        }

        self.ensure_header_written()?;

        let entry_len = RECORD_HEADER_SIZE + payload.len() as u32;
        let offset = match self
            .size
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                let next = current + entry_len;
                if next > usable_limit {
                    None
                } else {
                    Some(next)
                }
            }) {
            Ok(prev) => prev,
            Err(_) => return Err(AofError::SegmentFull(max_payload_capacity)),
        };

        let mut header = [0u8; RECORD_HEADER_SIZE as usize];
        header[0..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        let mut digest = Digest::new();
        digest.write(payload);
        let checksum = fold_crc64(digest.sum64());
        header[4..8].copy_from_slice(&checksum.to_le_bytes());
        header[8..16].copy_from_slice(&timestamp.to_le_bytes());
        let record_ext_id = self.ext_id.load(Ordering::Acquire);
        header[16..24].copy_from_slice(&record_ext_id.to_le_bytes());

        let write_offset = offset as usize;
        self.data.write_bytes(write_offset, &header)?;
        self.data
            .write_bytes(write_offset + RECORD_HEADER_SIZE as usize, payload)?;

        let next_size = offset + entry_len;
        self.data.update_size(next_size as u64);
        self.record_count.fetch_add(1, Ordering::AcqRel);
        self.last_timestamp.store(timestamp, Ordering::Release);
        self.size.store(next_size, Ordering::Release);
        self.last_record_offset.store(offset, Ordering::Release);
        self.flush_state.request_flush(next_size);

        let last_offset = self.base_offset + next_size as u64;
        let is_full = next_size >= usable_limit;

        Ok(SegmentAppendResult {
            segment_offset: offset,
            last_offset,
            logical_size: next_size,
            is_full,
        })
    }

    pub fn append_reserve(
        self: &Arc<Self>,
        payload_len: usize,
        timestamp: u64,
    ) -> AofResult<SegmentReservation> {
        let segment = self.as_ref();
        if segment.is_sealed() {
            return Err(AofError::InvalidState(
                "cannot append to sealed segment".to_string(),
            ));
        }
        if payload_len == 0 {
            return Err(AofError::InvalidState(
                "record payload is empty".to_string(),
            ));
        }

        let usable_limit = segment.usable_limit();
        let max_payload_capacity = segment.max_payload_capacity() as u64;
        if (payload_len as u64) + (RECORD_HEADER_SIZE as u64) > max_payload_capacity {
            return Err(AofError::SegmentFull(max_payload_capacity));
        }

        segment.ensure_header_written()?;

        let entry_len = RECORD_HEADER_SIZE + payload_len as u32;
        let offset =
            match segment
                .size
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    let next = current + entry_len;
                    if next > usable_limit {
                        None
                    } else {
                        Some(next)
                    }
                }) {
                Ok(prev) => prev,
                Err(_) => return Err(AofError::SegmentFull(max_payload_capacity)),
            };

        let next_size = offset + entry_len;
        segment.data.update_size(next_size as u64);
        segment.size.store(next_size, Ordering::Release);

        Ok(SegmentReservation::new(
            Arc::clone(self),
            offset,
            payload_len as u32,
            next_size,
            timestamp,
        ))
    }

    pub fn seal(
        &self,
        sealed_at: i64,
        coordinator_watermark: u64,
        flush_failure: bool,
    ) -> AofResult<SegmentFooter> {
        if self
            .sealed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(AofError::InvalidState("segment already sealed".to_string()));
        }

        let result = (|| -> AofResult<SegmentFooter> {
            let current_size = self.size.load(Ordering::Acquire);
            let durable = self.durable_size.load(Ordering::Acquire);
            if durable < current_size {
                return Err(AofError::InvalidState(
                    "cannot seal segment before durable bytes reach logical size".to_string(),
                ));
            }

            let record_count = self.record_count.load(Ordering::Acquire) as u64;
            let last_timestamp = self.last_timestamp.load(Ordering::Acquire);
            let last_record_offset = self.last_record_offset.load(Ordering::Acquire);
            let last_record_id = if record_count == 0 {
                RecordId::from_parts(self.index, SEGMENT_HEADER_SIZE)
            } else {
                RecordId::from_parts(self.index, last_record_offset)
            };

            let durable_end = durable.min(self.usable_limit()) as usize;
            let checksum = if record_count == 0 {
                0
            } else {
                self.compute_payload_checksum(durable_end)?
            };

            let footer = SegmentFooter {
                segment_index: self.index,
                base_record_count: self.base_record_count,
                record_count,
                durable_bytes: self.base_offset + durable as u64,
                last_record_id,
                last_timestamp,
                sealed_at,
                checksum,
                ext_id: self.ext_id.load(Ordering::Acquire),
                coordinator_watermark,
                flush_failure,
            };

            let mut buf = [0u8; SEGMENT_FOOTER_SIZE as usize];
            footer.encode(&mut buf);
            self.data.write_bytes(self.footer_offset() as usize, &buf)?;
            self.data.flush_and_sync()?;
            self.data.mark_read_only();

            Ok(footer)
        })();

        if result.is_err() {
            self.sealed.store(false, Ordering::Release);
        }

        result
    }

    pub fn set_ext_id(&self, ext_id: u64) -> AofResult<()> {
        if self.header_written.load(Ordering::Acquire) {
            let current = self.ext_id.load(Ordering::Acquire);
            if current != ext_id {
                return Err(AofError::InvalidState(
                    "cannot change segment ext_id after header written".to_string(),
                ));
            }
            return Ok(());
        }
        self.ext_id.store(ext_id, Ordering::Release);
        Ok(())
    }

    pub fn is_sealed(&self) -> bool {
        self.sealed.load(Ordering::Acquire)
    }

    pub fn current_size(&self) -> u32 {
        self.size.load(Ordering::Acquire)
    }

    pub fn durable_size(&self) -> u32 {
        self.durable_size.load(Ordering::Acquire)
    }

    pub fn record_count(&self) -> u64 {
        self.record_count.load(Ordering::Acquire) as u64
    }

    pub fn ext_id(&self) -> u64 {
        self.ext_id.load(Ordering::Acquire)
    }

    fn compute_payload_checksum(&self, limit: usize) -> AofResult<u32> {
        let usable_limit = self.usable_limit() as usize;
        let boundary = limit.min(usable_limit);
        if boundary <= SEGMENT_HEADER_SIZE as usize {
            return Ok(0);
        }

        let mut cursor = SEGMENT_HEADER_SIZE as usize;
        let mut digest = Digest::new();
        while cursor + RECORD_HEADER_SIZE as usize <= boundary {
            let header_bytes = self
                .data
                .read_slice(cursor..cursor + RECORD_HEADER_SIZE as usize)?;
            let length = u32::from_le_bytes(
                header_bytes[0..4]
                    .try_into()
                    .map_err(|_| AofError::Corruption("record length corrupt".to_string()))?,
            ) as usize;
            if length == 0 {
                break;
            }

            let payload_start = cursor + RECORD_HEADER_SIZE as usize;
            let payload_end = payload_start + length;
            if payload_end > boundary {
                break;
            }

            let payload = self.data.read_slice(payload_start..payload_end)?;
            digest.write(payload);
            cursor = payload_end;
        }

        Ok(fold_crc64(digest.sum64()))
    }

    pub fn truncate_segment(path: &Path, scan: &mut SegmentScan) -> AofResult<()> {
        let logical = scan.logical_size as usize;
        let usable_limit = (scan.header.max_size - SEGMENT_FOOTER_SIZE) as usize;
        if logical > usable_limit {
            return Err(AofError::Corruption(format!(
                "segment {} logical size {} exceeds usable limit {}",
                path.display(),
                logical,
                usable_limit
            )));
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(AofError::from)?;

        let mut mmap = unsafe { MmapMut::map_mut(&file).map_err(AofError::from)? };

        if logical < usable_limit {
            mmap[logical..usable_limit].fill(0);
        }

        let footer_offset = (scan.header.max_size - SEGMENT_FOOTER_SIZE) as usize;
        mmap[footer_offset..footer_offset + SEGMENT_FOOTER_SIZE as usize].fill(0);
        mmap.flush().map_err(AofError::from)?;

        scan.footer = None;
        scan.truncated = false;
        scan.logical_size = logical as u32;

        Ok(())
    }

    pub fn mark_durable(&self, bytes: u32) -> u32 {
        let capped = bytes.min(self.usable_limit());
        let mut current = self.durable_size.load(Ordering::Acquire);
        loop {
            if current >= capped {
                self.flush_state.mark_durable(current);
                return 0;
            }
            match self.durable_size.compare_exchange(
                current,
                capped,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.flush_state.mark_durable(capped);
                    return capped - current;
                }
                Err(observed) => current = observed,
            }
        }
    }

    pub fn restore_durability(&self, requested_bytes: u32, durable_bytes: u32) {
        let capped_requested = requested_bytes.min(self.usable_limit());
        let capped_durable = durable_bytes.min(capped_requested);
        self.flush_state.restore(capped_requested, capped_durable);
        self.durable_size.store(capped_durable, Ordering::Release);
    }

    pub fn segment_offset_for(&self, record_id: RecordId) -> Option<u32> {
        if record_id.segment_index() != self.index {
            return None;
        }
        Some(record_id.segment_offset())
    }

    #[cfg(test)]
    pub fn inject_flush_error(&self, attempts: u32) {
        self.flush_fail_injections
            .store(attempts, Ordering::Release);
    }

    pub fn record_end_offset(&self, segment_offset: u32) -> AofResult<u32> {
        if segment_offset < SEGMENT_HEADER_SIZE {
            return Err(AofError::CorruptedRecord(
                "record offset precedes segment header".to_string(),
            ));
        }
        let header_end = segment_offset
            .checked_add(RECORD_HEADER_SIZE)
            .ok_or_else(|| AofError::CorruptedRecord("record offset overflow".to_string()))?;
        let header_bytes = self
            .data
            .read_slice(segment_offset as usize..header_end as usize)?;
        let length = u32::from_le_bytes(
            header_bytes[0..4]
                .try_into()
                .map_err(|_| AofError::CorruptedRecord("record header corrupt".to_string()))?,
        );
        if length == 0 {
            return Err(AofError::CorruptedRecord(
                "record length cannot be zero".to_string(),
            ));
        }
        let end_offset = header_end
            .checked_add(length)
            .ok_or_else(|| AofError::CorruptedRecord("record length overflow".to_string()))?;
        if end_offset > self.current_size() {
            return Err(AofError::CorruptedRecord(
                "record extends beyond logical segment size".to_string(),
            ));
        }
        Ok(end_offset)
    }

    pub(crate) fn read_record_slice(
        &self,
        segment_offset: u32,
    ) -> AofResult<SegmentRecordSlice<'_>> {
        if segment_offset < SEGMENT_HEADER_SIZE {
            return Err(AofError::CorruptedRecord(
                "record offset precedes segment header".to_string(),
            ));
        }

        let header_end = segment_offset
            .checked_add(RECORD_HEADER_SIZE)
            .ok_or_else(|| AofError::CorruptedRecord("record offset overflow".to_string()))?;
        let header_bytes = self
            .data
            .read_slice(segment_offset as usize..header_end as usize)?;

        let length = u32::from_le_bytes(
            header_bytes[0..4]
                .try_into()
                .map_err(|_| AofError::CorruptedRecord("record header corrupt".to_string()))?,
        );
        if length == 0 {
            return Err(AofError::CorruptedRecord(
                "record length cannot be zero".to_string(),
            ));
        }

        let checksum = u32::from_le_bytes(
            header_bytes[4..8]
                .try_into()
                .map_err(|_| AofError::CorruptedRecord("record checksum corrupt".to_string()))?,
        );
        let timestamp = u64::from_le_bytes(
            header_bytes[8..16]
                .try_into()
                .map_err(|_| AofError::CorruptedRecord("record timestamp corrupt".to_string()))?,
        );
        let ext_id = u64::from_le_bytes(
            header_bytes[16..24]
                .try_into()
                .map_err(|_| AofError::CorruptedRecord("record ext id corrupt".to_string()))?,
        );

        let total_len = RECORD_HEADER_SIZE
            .checked_add(length)
            .ok_or_else(|| AofError::CorruptedRecord("record length overflow".to_string()))?;
        let end_offset = segment_offset
            .checked_add(total_len)
            .ok_or_else(|| AofError::CorruptedRecord("record length overflow".to_string()))?;
        if end_offset > self.current_size() {
            return Err(AofError::CorruptedRecord(
                "record extends beyond logical segment size".to_string(),
            ));
        }

        let payload_start = header_end as usize;
        let payload_end = payload_start
            .checked_add(length as usize)
            .ok_or_else(|| AofError::CorruptedRecord("record length overflow".to_string()))?;
        let payload = self.data.read_slice(payload_start..payload_end)?;

        Ok(SegmentRecordSlice {
            header: RecordHeader {
                length,
                checksum,
                timestamp,
                ext_id,
            },
            payload,
            total_len,
        })
    }

    fn ensure_header_written(&self) -> AofResult<()> {
        if self.header_written.load(Ordering::Acquire) {
            return Ok(());
        }
        if self
            .header_written
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }

        let header = SegmentHeader {
            segment_index: self.index,
            base_record_count: self.base_record_count,
            base_offset: self.base_offset,
            max_size: self.max_size,
            created_at: self.created_at,
            base_timestamp: self.last_timestamp.load(Ordering::Acquire),
            ext_id: self.ext_id.load(Ordering::Acquire),
        };

        let mut buf = [0u8; SEGMENT_HEADER_SIZE as usize];
        header.encode(&mut buf);
        self.data.write_bytes(0, &buf)?;
        self.data.update_size(SEGMENT_HEADER_SIZE as u64);
        self.size.store(SEGMENT_HEADER_SIZE, Ordering::Release);

        Ok(())
    }

    pub(crate) fn flush_to_disk(&self) -> AofResult<()> {
        #[cfg(test)]
        {
            let mut remaining = self.flush_fail_injections.load(Ordering::Acquire);
            while remaining > 0 {
                match self.flush_fail_injections.compare_exchange(
                    remaining,
                    remaining - 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        return Err(AofError::Io(std::io::Error::from_raw_os_error(libc::EINTR)));
                    }
                    Err(current) => remaining = current,
                }
            }
        }
        self.data.flush_and_sync()
    }
}

struct SegmentHeader {
    segment_index: u32,
    base_record_count: u64,
    base_offset: u64,
    max_size: u32,
    created_at: i64,
    base_timestamp: u64,
    ext_id: u64,
}

impl SegmentHeader {
    fn encode(&self, buf: &mut [u8]) {
        assert!(buf.len() >= SEGMENT_HEADER_SIZE as usize);
        buf.fill(0);
        buf[0..4].copy_from_slice(&SEGMENT_MAGIC.to_le_bytes());
        buf[4..6].copy_from_slice(&SEGMENT_VERSION.to_le_bytes());
        buf[6..8].copy_from_slice(&(SEGMENT_HEADER_SIZE as u16).to_le_bytes());
        buf[8..12].copy_from_slice(&self.segment_index.to_le_bytes());
        buf[12..20].copy_from_slice(&self.base_record_count.to_le_bytes());
        buf[20..28].copy_from_slice(&self.base_offset.to_le_bytes());
        buf[28..32].copy_from_slice(&self.max_size.to_le_bytes());
        buf[32..40].copy_from_slice(&self.created_at.to_le_bytes());
        buf[40..48].copy_from_slice(&self.base_timestamp.to_le_bytes());
        buf[48..56].copy_from_slice(&self.ext_id.to_le_bytes());
    }

    fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < SEGMENT_HEADER_SIZE as usize {
            return None;
        }
        let magic = u32::from_le_bytes(buf[0..4].try_into().ok()?);
        if magic != SEGMENT_MAGIC {
            return None;
        }
        let version = u16::from_le_bytes(buf[4..6].try_into().ok()?);
        if version != SEGMENT_VERSION {
            return None;
        }
        let header_len = u16::from_le_bytes(buf[6..8].try_into().ok()?);
        if header_len as u32 != SEGMENT_HEADER_SIZE {
            return None;
        }
        Some(SegmentHeader {
            segment_index: u32::from_le_bytes(buf[8..12].try_into().ok()?),
            base_record_count: u64::from_le_bytes(buf[12..20].try_into().ok()?),
            base_offset: u64::from_le_bytes(buf[20..28].try_into().ok()?),
            max_size: u32::from_le_bytes(buf[28..32].try_into().ok()?),
            created_at: i64::from_le_bytes(buf[32..40].try_into().ok()?),
            base_timestamp: u64::from_le_bytes(buf[40..48].try_into().ok()?),
            ext_id: u64::from_le_bytes(buf[48..56].try_into().ok()?),
        })
    }
}

impl From<SegmentHeader> for SegmentHeaderInfo {
    fn from(value: SegmentHeader) -> Self {
        SegmentHeaderInfo {
            segment_index: value.segment_index,
            base_record_count: value.base_record_count,
            base_offset: value.base_offset,
            max_size: value.max_size,
            created_at: value.created_at,
            base_timestamp: value.base_timestamp,
            ext_id: value.ext_id,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SegmentFooter {
    pub segment_index: u32,
    pub base_record_count: u64,
    pub record_count: u64,
    pub durable_bytes: u64,
    pub last_record_id: RecordId,
    pub last_timestamp: u64,
    pub sealed_at: i64,
    pub checksum: u32,
    pub ext_id: u64,
    pub coordinator_watermark: u64,
    pub flush_failure: bool,
}

impl SegmentFooter {
    fn encode(&self, buf: &mut [u8]) {
        assert!(buf.len() >= SEGMENT_FOOTER_SIZE as usize);
        buf.fill(0);
        buf[0..4].copy_from_slice(&SEGMENT_FOOTER_MAGIC.to_le_bytes());
        buf[4..8].copy_from_slice(&(SEGMENT_VERSION as u32).to_le_bytes());
        buf[8..12].copy_from_slice(&self.segment_index.to_le_bytes());
        buf[12..20].copy_from_slice(&self.base_record_count.to_le_bytes());
        buf[20..28].copy_from_slice(&self.record_count.to_le_bytes());
        buf[28..36].copy_from_slice(&self.durable_bytes.to_le_bytes());
        buf[36..44].copy_from_slice(&self.last_record_id.as_u64().to_le_bytes());
        buf[44..52].copy_from_slice(&self.last_timestamp.to_le_bytes());
        buf[52..60].copy_from_slice(&self.sealed_at.to_le_bytes());
        buf[60..64].copy_from_slice(&self.checksum.to_le_bytes());
        buf[64..72].copy_from_slice(&self.ext_id.to_le_bytes());
        buf[72..80].copy_from_slice(&self.coordinator_watermark.to_le_bytes());
        buf[80] = if self.flush_failure { 1 } else { 0 };
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < SEGMENT_FOOTER_SIZE as usize {
            return None;
        }
        let magic = u32::from_le_bytes(buf[0..4].try_into().ok()?);
        if magic != SEGMENT_FOOTER_MAGIC {
            return None;
        }
        let version = u32::from_le_bytes(buf[4..8].try_into().ok()?);
        if version != SEGMENT_VERSION as u32 {
            return None;
        }
        let segment_index = u32::from_le_bytes(buf[8..12].try_into().ok()?);
        let base_record_count = u64::from_le_bytes(buf[12..20].try_into().ok()?);
        let record_count = u64::from_le_bytes(buf[20..28].try_into().ok()?);
        let durable_bytes = u64::from_le_bytes(buf[28..36].try_into().ok()?);
        let last_record_id = u64::from_le_bytes(buf[36..44].try_into().ok()?);
        let last_timestamp = u64::from_le_bytes(buf[44..52].try_into().ok()?);
        let sealed_at = i64::from_le_bytes(buf[52..60].try_into().ok()?);
        let checksum = u32::from_le_bytes(buf[60..64].try_into().ok()?);
        let ext_id = u64::from_le_bytes(buf[64..72].try_into().ok()?);
        let coordinator_watermark = u64::from_le_bytes(buf[72..80].try_into().ok()?);
        let flush_failure = matches!(buf.get(80), Some(&1));
        Some(SegmentFooter {
            segment_index,
            base_record_count,
            record_count,
            durable_bytes,
            last_record_id: RecordId::new(last_record_id),
            last_timestamp,
            sealed_at,
            checksum,
            ext_id,
            coordinator_watermark,
            flush_failure,
        })
    }
}

#[derive(Debug)]
struct ScanSummary {
    consumed: usize,
    record_count: u64,
    last_record_id: RecordId,
    last_timestamp: u64,
    checksum: u32,
    truncated: bool,
}

fn scan_records(data: &[u8], header: &SegmentHeader, limit: usize) -> AofResult<ScanSummary> {
    let mut ceiling = limit.min(data.len());
    if ceiling < SEGMENT_HEADER_SIZE as usize {
        ceiling = SEGMENT_HEADER_SIZE as usize;
    }

    let mut cursor = SEGMENT_HEADER_SIZE as usize;
    let mut digest = Digest::new();
    let max_payload = (header.max_size - SEGMENT_HEADER_SIZE - SEGMENT_FOOTER_SIZE) as usize;
    let mut record_count = 0u64;
    let mut truncated = false;
    let mut last_timestamp = header.base_timestamp;
    let mut last_record_id = RecordId::from_parts(header.segment_index, SEGMENT_HEADER_SIZE);

    while cursor + RECORD_HEADER_SIZE as usize <= ceiling {
        let header_slice = &data[cursor..cursor + RECORD_HEADER_SIZE as usize];
        let length = u32::from_le_bytes(
            header_slice[0..4]
                .try_into()
                .map_err(|_| AofError::Corruption("record length slice corrupt".to_string()))?,
        );
        let checksum = u32::from_le_bytes(
            header_slice[4..8]
                .try_into()
                .map_err(|_| AofError::Corruption("record checksum slice corrupt".to_string()))?,
        );
        let timestamp = u64::from_le_bytes(
            header_slice[8..16]
                .try_into()
                .map_err(|_| AofError::Corruption("record timestamp slice corrupt".to_string()))?,
        );

        if length == 0 {
            break;
        }

        let payload_start = cursor + RECORD_HEADER_SIZE as usize;
        let payload_end = payload_start + length as usize;

        if payload_end > ceiling || payload_end > data.len() {
            truncated = true;
            break;
        }

        if length as usize > max_payload {
            truncated = true;
            break;
        }

        let payload = &data[payload_start..payload_end];
        let mut rec_digest = Digest::new();
        rec_digest.write(payload);
        if fold_crc64(rec_digest.sum64()) != checksum {
            truncated = true;
            break;
        }

        digest.write(payload);

        let record_offset = cursor as u32;
        last_record_id = RecordId::from_parts(header.segment_index, record_offset);
        last_timestamp = timestamp;
        record_count += 1;
        cursor = payload_end;
    }

    Ok(ScanSummary {
        consumed: cursor,
        record_count,
        last_record_id,
        last_timestamp,
        checksum: fold_crc64(digest.sum64()),
        truncated,
    })
}

enum SegmentMmap {
    Read(Mmap),
    Write(MmapMut),
}

impl SegmentMmap {
    #[cfg(test)]
    fn len(&self) -> usize {
        match self {
            SegmentMmap::Read(m) => m.len(),
            SegmentMmap::Write(m) => m.len(),
        }
    }
}

pub struct SegmentData {
    path: PathBuf,
    mmap: Mutex<SegmentMmap>,
    data: AtomicPtr<u8>,
    size: AtomicU64,
    max_size: u32,
    writable: AtomicBool,
}

unsafe impl Send for SegmentData {}
unsafe impl Sync for SegmentData {}

impl SegmentData {
    fn create(path: &Path, max_size: u32) -> AofResult<Self> {
        let file = create_fixed_size_file(path, max_size as u64)?;
        let mut mmap = unsafe { MmapMut::map_mut(&file).map_err(AofError::from)? };
        if mmap.len() != max_size as usize {
            return Err(AofError::InvalidSegmentSize(
                max_size as u64,
                mmap.len() as u64,
            ));
        }
        let data_ptr = mmap.as_mut_ptr();
        Ok(Self {
            path: path.to_path_buf(),
            mmap: Mutex::new(SegmentMmap::Write(mmap)),
            data: AtomicPtr::new(data_ptr),
            size: AtomicU64::new(0),
            max_size,
            writable: AtomicBool::new(true),
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn open(path: &Path, max_size: u32, logical_size: u32, writable: bool) -> AofResult<Self> {
        if logical_size > max_size {
            return Err(AofError::InvalidSegmentSize(
                max_size as u64,
                logical_size as u64,
            ));
        }

        let file = OpenOptions::new()
            .read(true)
            .write(writable)
            .open(path)
            .map_err(AofError::from)?;

        let (mmap, ptr) = if writable {
            let mut map = unsafe { MmapMut::map_mut(&file).map_err(AofError::from)? };
            if map.len() < max_size as usize {
                return Err(AofError::InvalidSegmentSize(
                    max_size as u64,
                    map.len() as u64,
                ));
            }
            let data_ptr = map.as_mut_ptr();
            (SegmentMmap::Write(map), data_ptr)
        } else {
            let map = unsafe { Mmap::map(&file).map_err(AofError::from)? };
            if map.len() < max_size as usize {
                return Err(AofError::InvalidSegmentSize(
                    max_size as u64,
                    map.len() as u64,
                ));
            }
            let data_ptr = map.as_ptr() as *mut u8;
            (SegmentMmap::Read(map), data_ptr)
        };

        Ok(Self {
            path: path.to_path_buf(),
            mmap: Mutex::new(mmap),
            data: AtomicPtr::new(ptr),
            size: AtomicU64::new(logical_size as u64),
            max_size,
            writable: AtomicBool::new(writable),
        })
    }

    fn write_bytes(&self, offset: usize, bytes: &[u8]) -> AofResult<()> {
        if offset + bytes.len() > self.max_size as usize {
            return Err(AofError::SegmentFull(self.max_size as u64));
        }
        if !self.is_writable() {
            return Err(AofError::InvalidState(
                "attempted to write to read-only segment".to_string(),
            ));
        }
        let ptr = self.data.load(Ordering::Acquire);
        if ptr.is_null() {
            return Err(AofError::InvalidState(
                "segment memory unmapped".to_string(),
            ));
        }
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), bytes.len());
        }
        Ok(())
    }

    fn read_slice(&self, range: Range<usize>) -> AofResult<&[u8]> {
        if range.end > self.max_size as usize || range.start > range.end {
            return Err(AofError::SegmentFull(self.max_size as u64));
        }
        let ptr = self.data.load(Ordering::Acquire);
        if ptr.is_null() {
            return Err(AofError::InvalidState(
                "segment memory unmapped".to_string(),
            ));
        }
        unsafe { Ok(slice::from_raw_parts(ptr.add(range.start), range.len())) }
    }

    fn slice_mut(&self, range: Range<usize>) -> AofResult<&mut [u8]> {
        if range.end > self.max_size as usize || range.start > range.end {
            return Err(AofError::SegmentFull(self.max_size as u64));
        }
        if !self.is_writable() {
            return Err(AofError::InvalidState(
                "attempted to write to read-only segment".to_string(),
            ));
        }
        let ptr = self.data.load(Ordering::Acquire);
        if ptr.is_null() {
            return Err(AofError::InvalidState(
                "segment memory unmapped".to_string(),
            ));
        }
        unsafe { Ok(slice::from_raw_parts_mut(ptr.add(range.start), range.len())) }
    }

    fn update_size(&self, logical_size: u64) {
        self.size.store(logical_size, Ordering::Release);
    }

    fn flush(&self) -> AofResult<()> {
        let mut guard = self.mmap.lock();
        match &mut *guard {
            SegmentMmap::Write(map) => {
                map.flush().map_err(AofError::from)?;
                Ok(())
            }
            SegmentMmap::Read(map) => {
                let _ = map.len();
                Ok(())
            }
        }
    }

    fn flush_and_sync(&self) -> AofResult<()> {
        self.flush()?;
        self.sync_file()
    }

    fn sync_file(&self) -> AofResult<()> {
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        let file = match options.open(&self.path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied => OpenOptions::new()
                .read(true)
                .open(&self.path)
                .map_err(AofError::from)?,
            Err(err) => return Err(AofError::from(err)),
        };

        match file.sync_data() {
            Ok(()) => Ok(()),
            Err(err) if sync_data_unsupported(&err) => file.sync_all().map_err(AofError::from),
            Err(err) => Err(AofError::from(err)),
        }
    }

    fn mark_read_only(&self) {
        self.writable.store(false, Ordering::Release);
    }

    #[cfg(test)]
    fn remap_read_only(&self) -> AofResult<()> {
        self.remap(false)
    }

    #[cfg(test)]
    fn remap_writable(&self) -> AofResult<()> {
        self.remap(true)
    }

    #[cfg(test)]
    fn remap(&self, writable: bool) -> AofResult<()> {
        if self.writable.load(Ordering::Acquire) == writable {
            return Ok(());
        }

        let file = OpenOptions::new()
            .read(true)
            .write(writable)
            .open(&self.path)
            .map_err(AofError::from)?;

        let mut mmap = if writable {
            SegmentMmap::Write(unsafe { MmapMut::map_mut(&file).map_err(AofError::from)? })
        } else {
            SegmentMmap::Read(unsafe { Mmap::map(&file).map_err(AofError::from)? })
        };

        if mmap.len() != self.max_size as usize {
            return Err(AofError::InvalidSegmentSize(
                self.max_size as u64,
                mmap.len() as u64,
            ));
        }

        let ptr = match &mut mmap {
            SegmentMmap::Read(map) => map.as_ptr() as *mut u8,
            SegmentMmap::Write(map) => map.as_mut_ptr(),
        };

        let mut guard = self.mmap.lock();
        *guard = mmap;
        self.data.store(ptr, Ordering::Release);
        self.writable.store(writable, Ordering::Release);
        Ok(())
    }

    fn is_writable(&self) -> bool {
        self.writable.load(Ordering::Acquire)
    }
}

fn sync_data_unsupported(err: &io::Error) -> bool {
    if matches!(err.kind(), io::ErrorKind::Unsupported) {
        return true;
    }
    if let Some(code) = err.raw_os_error() {
        if code == libc::ENOSYS || code == libc::EINVAL || code == libc::ENOTSUP {
            return true;
        }
        if cfg!(windows) && code == 1 {
            // ERROR_INVALID_FUNCTION: Treat as fdatasync unsupported and fall back to fsync.
            return true;
        }
    }
    false
}

fn fold_crc64(value: u64) -> u32 {
    let upper = (value >> 32) as u32;
    let lower = value as u32;
    upper ^ lower
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AofConfig;
    use proptest::prelude::*;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use tempfile::TempDir;

    #[test]
    fn header_emitted_on_first_append() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let cfg = AofConfig::default();
        let segment =
            Segment::create_active(SegmentId::new(1), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create");

        let mut buf = [0u8; 4];
        File::open(&path)
            .expect("open")
            .read_exact(&mut buf)
            .expect("read");
        assert_eq!(buf, [0; 4]);

        segment.append_record(b"hello", 42).expect("append");

        File::open(&path)
            .expect("open")
            .read_exact(&mut buf)
            .expect("read");
        assert_eq!(buf, SEGMENT_MAGIC.to_le_bytes());
    }

    #[test]
    fn append_reserves_footer_space() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 1024;
        cfg.segment_max_bytes = 1024;
        cfg.segment_target_bytes = 1024;
        let segment =
            Segment::create_active(SegmentId::new(1), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create");
        let payload = [42u8; 128];
        let mut appended = 0;
        loop {
            match segment.append_record(&payload, 1) {
                Ok(_) => {
                    appended += 1;
                    assert!(appended <= 32, "append exceeded expected iterations");
                }
                Err(AofError::SegmentFull(_)) => break,
                Err(err) => panic!("unexpected error: {err:?}"),
            }
        }
        let footer_offset = segment.footer_offset() as u64;
        let mut file = File::open(&path).expect("open");
        file.seek(SeekFrom::Start(footer_offset)).expect("seek");
        let mut buf = [0u8; SEGMENT_FOOTER_SIZE as usize];
        file.read_exact(&mut buf).expect("read");
        assert!(buf.iter().all(|b| *b == 0));
        assert!(matches!(
            segment.append_record(&payload, 1),
            Err(AofError::SegmentFull(_))
        ));
    }

    #[test]
    fn load_header_reads_metadata() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 1024;
        cfg.segment_max_bytes = 1024;
        cfg.segment_target_bytes = 1024;
        let segment =
            Segment::create_active(SegmentId::new(3), 0, 0, 77, cfg.segment_max_bytes, &path)
                .expect("create");
        segment.append_record(&[9u8; 4], 123).expect("append");

        let info = Segment::load_header(&path).expect("load header");
        assert_eq!(info.segment_index, 3);
        assert_eq!(info.base_record_count, 0);
        assert_eq!(info.base_offset, 0);
        assert_eq!(info.created_at, 77);
        assert_eq!(info.base_timestamp, 123);
        assert_eq!(info.ext_id, 0);
    }

    proptest! {
        #[test]
        fn record_end_offset_matches_payload_lengths(payloads in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..64), 1..16)) {
            let total: usize = payloads.iter().map(|p| p.len()).sum();
            prop_assume!(total as u32 <= 1024);

            let tmp = TempDir::new().expect("tempdir");
            let path = tmp.path().join("segment.seg");
            let mut cfg = AofConfig::default();
            cfg.segment_min_bytes = 4096;
            cfg.segment_max_bytes = 4096;
            cfg.segment_target_bytes = 4096;
            let segment = Segment::create_active(SegmentId::new(123), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create");
            let segment = Arc::new(segment);

            for payload in payloads {
                let append = segment.append_record(&payload, 1).expect("append");
                let expected_end = append
                    .segment_offset
                    .checked_add(RECORD_HEADER_SIZE + payload.len() as u32)
                    .expect("offset overflow");
                let end_offset = segment
                    .record_end_offset(append.segment_offset)
                    .expect("end offset");
                prop_assert_eq!(end_offset, expected_end);
                let _ = segment.mark_durable(append.logical_size);
            }
        }
    }

    fn arbitrary_max_size(seed: u32) -> u32 {
        let min_size = SEGMENT_HEADER_SIZE + SEGMENT_FOOTER_SIZE + 1;
        let range = u64::from(u32::MAX) - u64::from(min_size) + 1;
        let offset = u64::from(seed) % range;
        min_size + offset as u32
    }

    proptest! {
        #[test]
        fn segment_header_roundtrip(
            segment_index in any::<u32>(),
            base_record_count in any::<u64>(),
            base_offset in any::<u64>(),
            max_size_seed in any::<u32>(),
            created_at in any::<i64>(),
            base_timestamp in any::<u64>(),
            ext_id in any::<u64>(),
        ) {
            let max_size = arbitrary_max_size(max_size_seed);
            let header = SegmentHeader {
                segment_index,
                base_record_count,
                base_offset,
                max_size,
                created_at,
                base_timestamp,
                ext_id,
            };

            let mut buf = [0u8; SEGMENT_HEADER_SIZE as usize];
            header.encode(&mut buf);
            let decoded = SegmentHeader::decode(&buf).expect("header decode");

            prop_assert_eq!(decoded.segment_index, header.segment_index);
            prop_assert_eq!(decoded.base_record_count, header.base_record_count);
            prop_assert_eq!(decoded.base_offset, header.base_offset);
            prop_assert_eq!(decoded.max_size, header.max_size);
            prop_assert_eq!(decoded.created_at, header.created_at);
            prop_assert_eq!(decoded.base_timestamp, header.base_timestamp);
            prop_assert_eq!(decoded.ext_id, header.ext_id);
        }

        #[test]
        fn segment_header_rejects_corrupt_magic(
            segment_index in any::<u32>(),
            base_record_count in any::<u64>(),
            base_offset in any::<u64>(),
            max_size_seed in any::<u32>(),
            created_at in any::<i64>(),
            base_timestamp in any::<u64>(),
            ext_id in any::<u64>(),
            flip_index in 0usize..4,
        ) {
            let max_size = arbitrary_max_size(max_size_seed);
            let header = SegmentHeader {
                segment_index,
                base_record_count,
                base_offset,
                max_size,
                created_at,
                base_timestamp,
                ext_id,
            };

            let mut buf = [0u8; SEGMENT_HEADER_SIZE as usize];
            header.encode(&mut buf);
            buf[flip_index] ^= 0xFF;

            prop_assert!(SegmentHeader::decode(&buf).is_none());
        }

        #[test]
        fn segment_footer_roundtrip(
            segment_index in any::<u32>(),
            base_record_count in any::<u64>(),
            record_delta in any::<u64>(),
            durable_bytes in any::<u64>(),
            last_offset in any::<u32>(),
            last_timestamp in any::<u64>(),
            sealed_at in any::<i64>(),
            checksum in any::<u32>(),
            ext_id in any::<u64>(),
            coordinator_watermark in any::<u64>(),
            flush_failure in any::<bool>(),
        ) {
            let record_count = base_record_count.saturating_add(record_delta);
            let footer = SegmentFooter {
                segment_index,
                base_record_count,
                record_count,
                durable_bytes,
                last_record_id: RecordId::from_parts(segment_index, last_offset),
                last_timestamp,
                sealed_at,
                checksum,
                ext_id,
                coordinator_watermark,
                flush_failure,
            };

            let mut buf = [0u8; SEGMENT_FOOTER_SIZE as usize];
            footer.encode(&mut buf);
            let decoded = SegmentFooter::decode(&buf).expect("footer decode");

            prop_assert_eq!(decoded.segment_index, footer.segment_index);
            prop_assert_eq!(decoded.base_record_count, footer.base_record_count);
            prop_assert_eq!(decoded.record_count, footer.record_count);
            prop_assert_eq!(decoded.durable_bytes, footer.durable_bytes);
            prop_assert_eq!(decoded.last_record_id.as_u64(), footer.last_record_id.as_u64());
            prop_assert_eq!(decoded.last_timestamp, footer.last_timestamp);
            prop_assert_eq!(decoded.sealed_at, footer.sealed_at);
            prop_assert_eq!(decoded.checksum, footer.checksum);
            prop_assert_eq!(decoded.ext_id, footer.ext_id);
            prop_assert_eq!(decoded.coordinator_watermark, footer.coordinator_watermark);
            prop_assert_eq!(decoded.flush_failure, footer.flush_failure);
        }

        #[test]
        fn segment_footer_rejects_corrupt_magic(
            segment_index in any::<u32>(),
            base_record_count in any::<u64>(),
            record_delta in any::<u64>(),
            durable_bytes in any::<u64>(),
            last_offset in any::<u32>(),
            last_timestamp in any::<u64>(),
            sealed_at in any::<i64>(),
            checksum in any::<u32>(),
            ext_id in any::<u64>(),
            coordinator_watermark in any::<u64>(),
            flush_failure in any::<bool>(),
            flip_index in 0usize..4,
        ) {
            let record_count = base_record_count.saturating_add(record_delta);
            let footer = SegmentFooter {
                segment_index,
                base_record_count,
                record_count,
                durable_bytes,
                last_record_id: RecordId::from_parts(segment_index, last_offset),
                last_timestamp,
                sealed_at,
                checksum,
                ext_id,
                coordinator_watermark,
                flush_failure,
            };

            let mut buf = [0u8; SEGMENT_FOOTER_SIZE as usize];
            footer.encode(&mut buf);
            buf[flip_index] ^= 0xFF;

            prop_assert!(SegmentFooter::decode(&buf).is_none());
        }
    }

    #[test]
    fn trusted_footer_scan_uses_footer_metadata() {
        let header = SegmentHeaderInfo {
            segment_index: 11,
            base_record_count: 4,
            base_offset: 2_048,
            max_size: 4_096,
            created_at: 99,
            base_timestamp: 1_234,
            ext_id: 42,
        };
        let footer = SegmentFooter {
            segment_index: 11,
            base_record_count: 4,
            record_count: 8,
            durable_bytes: 2_560,
            last_record_id: RecordId::from_parts(11, SEGMENT_HEADER_SIZE),
            last_timestamp: 9_876,
            sealed_at: 777,
            checksum: 0xDEAD_BEEF,
            ext_id: 42,
            coordinator_watermark: 123,
            flush_failure: false,
        };
        let scan = SegmentScan::from_trusted_footer(header.clone(), footer).expect("scan");
        assert_eq!(scan.logical_size, 512);
        assert_eq!(scan.record_count, 8);
        assert_eq!(scan.last_timestamp, 9_876);
        assert_eq!(scan.footer.unwrap().coordinator_watermark, 123);
        assert!(!scan.truncated);
        assert_eq!(scan.header.segment_index, 11);
        assert_eq!(scan.header.ext_id, 42);
    }

    #[test]
    fn trusted_footer_rejects_durable_before_base_offset() {
        let header = SegmentHeaderInfo {
            segment_index: 3,
            base_record_count: 0,
            base_offset: 2_048,
            max_size: 4_096,
            created_at: 0,
            base_timestamp: 0,
            ext_id: 0,
        };
        let footer = SegmentFooter {
            segment_index: 3,
            base_record_count: 0,
            record_count: 0,
            durable_bytes: 1_024,
            last_record_id: RecordId::from_parts(3, SEGMENT_HEADER_SIZE),
            last_timestamp: 0,
            sealed_at: 0,
            checksum: 0,
            ext_id: 0,
            coordinator_watermark: 0,
            flush_failure: false,
        };
        let err = SegmentScan::from_trusted_footer(header, footer)
            .expect_err("durable before base offset");
        match err {
            AofError::Corruption(msg) => assert!(msg.contains("precedes")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn trusted_footer_rejects_durable_beyond_segment() {
        let header = SegmentHeaderInfo {
            segment_index: 4,
            base_record_count: 10,
            base_offset: 0,
            max_size: 2_048,
            created_at: 0,
            base_timestamp: 0,
            ext_id: 0,
        };
        let footer = SegmentFooter {
            segment_index: 4,
            base_record_count: 10,
            record_count: 10,
            durable_bytes: 10_000,
            last_record_id: RecordId::from_parts(4, SEGMENT_HEADER_SIZE),
            last_timestamp: 0,
            sealed_at: 0,
            checksum: 0,
            ext_id: 0,
            coordinator_watermark: 0,
            flush_failure: false,
        };
        let err =
            SegmentScan::from_trusted_footer(header, footer).expect_err("durable beyond segment");
        match err {
            AofError::Corruption(msg) => assert!(msg.contains("exceeds")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn scan_tail_without_footer() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 2048;
        cfg.segment_max_bytes = 2048;
        cfg.segment_target_bytes = 2048;
        let segment =
            Segment::create_active(SegmentId::new(4), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create");
        segment.append_record(b"abc", 222).expect("append");

        let scan = Segment::scan_tail(&path).expect("scan");
        assert!(scan.footer.is_none());
        assert_eq!(scan.record_count, 1);
        assert!(!scan.truncated);
        assert_eq!(scan.last_record_id.segment_index(), 4);
        assert_eq!(
            scan.logical_size as usize,
            SEGMENT_HEADER_SIZE as usize + RECORD_HEADER_SIZE as usize + 3
        );
    }

    #[test]
    fn scan_tail_with_footer() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        let segment =
            Segment::create_active(SegmentId::new(7), 0, 0, 123, cfg.segment_max_bytes, &path)
                .expect("create");
        segment.append_record(&[1u8; 32], 999).expect("append");
        let size = segment.current_size();
        let _ = segment.mark_durable(size);

        let footer = segment.seal(456, 0, false).expect("seal");

        let scan = Segment::scan_tail(&path).expect("scan");
        let scanned_footer = scan.footer.expect("footer");
        assert_eq!(scanned_footer.record_count, footer.record_count);
        assert_eq!(scanned_footer.sealed_at, footer.sealed_at);
        assert_eq!(
            scanned_footer.last_record_id.as_u64(),
            footer.last_record_id.as_u64()
        );
        assert_eq!(scan.record_count, 1);
        assert!(!scan.truncated);
    }

    #[test]
    fn seal_transitions_segment_data_to_read_only() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        let segment =
            Segment::create_active(SegmentId::new(8), 0, 0, 777, cfg.segment_max_bytes, &path)
                .expect("create");
        segment
            .append_record(b"payload", 456)
            .expect("append record");
        let size = segment.current_size();
        let _ = segment.mark_durable(size);

        let before_ptr = segment.data.data.load(Ordering::Acquire);

        segment.seal(999, 0, false).expect("seal segment");
        let after_ptr = segment.data.data.load(Ordering::Acquire);
        assert_eq!(
            before_ptr, after_ptr,
            "sealed segment remapped backing mmap"
        );
        assert!(!segment.data.is_writable());
        assert!(matches!(
            segment.data.write_bytes(0, &[]),
            Err(AofError::InvalidState(_))
        ));

        segment
            .data
            .remap_writable()
            .expect("remap writable after seal");
        assert!(segment.data.is_writable());

        segment
            .data
            .remap_read_only()
            .expect("remap back to read-only");
        assert!(!segment.data.is_writable());
    }

    #[test]
    fn set_ext_id_persists_to_header_and_footer() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 2048;
        cfg.segment_max_bytes = 2048;
        cfg.segment_target_bytes = 2048;
        let segment =
            Segment::create_active(SegmentId::new(5), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create segment");
        segment.set_ext_id(42).expect("set ext id");

        segment.append_record(b"payload", 1).expect("append record");
        let _ = segment.mark_durable(segment.current_size());
        let footer = segment.seal(123, 7, true).expect("seal with ext id");
        assert_eq!(footer.ext_id, 42);
        assert_eq!(footer.coordinator_watermark, 7);
        assert!(footer.flush_failure);

        let reloaded = Segment::load_header(&path).expect("header");
        assert_eq!(reloaded.ext_id, 42);

        let mut file = File::open(&path).expect("open segment");
        let mut header_buf = [0u8; RECORD_HEADER_SIZE as usize];
        file.seek(SeekFrom::Start(SEGMENT_HEADER_SIZE as u64))
            .expect("seek record header");
        file.read_exact(&mut header_buf)
            .expect("read record header");
        let mut ext_bytes = [0u8; 8];
        ext_bytes.copy_from_slice(&header_buf[16..24]);
        assert_eq!(u64::from_le_bytes(ext_bytes), 42);
    }

    #[test]
    fn set_ext_id_after_header_written_errors() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 2048;
        cfg.segment_max_bytes = 2048;
        cfg.segment_target_bytes = 2048;
        let segment =
            Segment::create_active(SegmentId::new(6), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create segment");

        segment.append_record(b"payload", 1).expect("append record");
        let err = segment.set_ext_id(99).expect_err("set ext id too late");
        match err {
            AofError::InvalidState(message) => {
                assert!(message.contains("header"), "unexpected message: {message}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
