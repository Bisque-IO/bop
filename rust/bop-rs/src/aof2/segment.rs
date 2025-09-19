use std::fs::File;
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use crc64fast_nvme::Digest;
use memmap2::{Mmap, MmapMut};
use serde::{Deserialize, Serialize};

use super::config::{AofConfig, RecordId, SegmentId};
use super::error::{AofError, AofResult};
use super::fs::create_fixed_size_file;

const SEGMENT_HEADER_SIZE: u32 = 64;
const RECORD_HEADER_SIZE: u32 = 16;
const SEGMENT_MAGIC: u32 = 0x414F_4632; // "AOF2"
const SEGMENT_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentStatus {
    Active,
    Finalized,
}

#[derive(Debug, Clone, Copy)]
pub struct SegmentAppendResult {
    pub segment_offset: u32,
    pub last_offset: u64,
    pub is_full: bool,
}

pub struct Segment {
    index: u32,
    base_offset: u64,
    base_record_count: u64,
    max_size: u32,
    created_at: i64,

    header_written: AtomicBool,
    record_count: AtomicU32,
    size: AtomicU32,
    durable_size: AtomicU32,
    last_timestamp: AtomicU64,

    data: Arc<SegmentData>,
}

impl Segment {
    pub fn create_active(
        segment_id: SegmentId,
        base_offset: u64,
        base_record_count: u64,
        created_at: i64,
        config: &AofConfig,
        path: &Path,
    ) -> AofResult<Self> {
        let max_bytes = config.segment_max_bytes;
        if max_bytes > u32::MAX as u64 {
            return Err(AofError::invalid_config(
                "segment_max_bytes exceeds u32::MAX for this build",
            ));
        }

        let max_size = max_bytes as u32;
        let data = SegmentData::create(path, max_size)?;

        Ok(Self {
            index: segment_id.as_u32(),
            base_offset,
            base_record_count,
            max_size,
            created_at,
            header_written: AtomicBool::new(false),
            record_count: AtomicU32::new(0),
            size: AtomicU32::new(0),
            durable_size: AtomicU32::new(0),
            last_timestamp: AtomicU64::new(0),
            data: Arc::new(data),
        })
    }

    #[inline]
    pub fn id(&self) -> SegmentId {
        SegmentId::new(self.index as u64)
    }

    pub fn append_record(
        &self,
        payload: &[u8],
        timestamp: u64,
    ) -> AofResult<SegmentAppendResult> {
        if payload.len() as u32 + RECORD_HEADER_SIZE > self.max_size {
            return Err(AofError::SegmentFull(self.max_size as u64));
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
                if next > self.max_size {
                    None
                } else {
                    Some(next)
                }
            })
        {
            Ok(prev) => prev,
            Err(_) => return Err(AofError::SegmentFull(self.max_size as u64)),
        };

        let mut header = [0u8; RECORD_HEADER_SIZE as usize];
        header[0..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        let mut digest = Digest::new();
        digest.write(payload);
        let checksum = fold_crc64(digest.sum64());
        header[4..8].copy_from_slice(&checksum.to_le_bytes());
        header[8..16].copy_from_slice(&timestamp.to_le_bytes());

        let write_offset = offset as usize;
        self.data.write_bytes(write_offset, &header)?;
        self.data
            .write_bytes(write_offset + RECORD_HEADER_SIZE as usize, payload)?;

        let next_size = offset + entry_len;
        self.data.update_size(next_size as u64);
        self.record_count.fetch_add(1, Ordering::AcqRel);
        self.last_timestamp.store(timestamp, Ordering::Release);
        self.size.store(next_size, Ordering::Release);

        let last_offset = self.base_offset + next_size as u64;
        let is_full = next_size >= self.max_size;

        Ok(SegmentAppendResult {
            segment_offset: offset,
            last_offset,
            is_full,
        })
    }

    pub fn mark_durable(&self, bytes: u32) {
        self.durable_size.store(bytes, Ordering::Release);
    }

    pub fn segment_offset_for(&self, record_id: RecordId) -> Option<u32> {
        if record_id.segment_index() != self.index {
            return None;
        }
        Some(record_id.segment_offset())
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
        };

        let mut buf = [0u8; SEGMENT_HEADER_SIZE as usize];
        header.encode(&mut buf);
        self.data.write_bytes(0, &buf)?;
        self.data.update_size(SEGMENT_HEADER_SIZE as u64);
        self.size.store(SEGMENT_HEADER_SIZE, Ordering::Release);

        Ok(())
    }

    fn make_record_id(&self, segment_offset: u32) -> RecordId {
        RecordId::from_parts(self.index, segment_offset)
    }
}

struct SegmentHeader {
    segment_index: u32,
    base_record_count: u64,
    base_offset: u64,
    max_size: u32,
    created_at: i64,
    base_timestamp: u64,
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
    }
}

enum SegmentMmap {
    Read(Mmap),
    Write(MmapMut),
}

impl SegmentMmap {
    fn len(&self) -> usize {
        match self {
            SegmentMmap::Read(m) => m.len(),
            SegmentMmap::Write(m) => m.len(),
        }
    }
}

pub struct SegmentData {
    mmap: SegmentMmap,
    data: *mut u8,
    size: AtomicU64,
    max_size: u32,
}

unsafe impl Send for SegmentData {}
unsafe impl Sync for SegmentData {}

impl SegmentData {
    fn create(path: &Path, max_size: u32) -> AofResult<Self> {
        let file = create_fixed_size_file(path, max_size as u64)?;
        let mmap = unsafe { MmapMut::map_mut(&file)? };
        let data_ptr = mmap.as_ptr() as *mut u8;
        Ok(Self {
            mmap: SegmentMmap::Write(mmap),
            data: data_ptr,
            size: AtomicU64::new(0),
            max_size,
        })
    }

    fn write_bytes(&self, offset: usize, bytes: &[u8]) -> AofResult<()> {
        if offset + bytes.len() > self.max_size as usize {
            return Err(AofError::SegmentFull(self.max_size as u64));
        }
        match &self.mmap {
            SegmentMmap::Write(_) => unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), self.data.add(offset), bytes.len());
                Ok(())
            },
            _ => Err(AofError::InvalidState(
                "attempted to write to read-only segment".to_string(),
            )),
        }
    }

    fn update_size(&self, logical_size: u64) {
        self.size.store(logical_size, Ordering::Release);
    }
}

fn fold_crc64(value: u64) -> u32 {
    let upper = (value >> 32) as u32;
    let lower = value as u32;
    upper ^ lower
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;
    use tempfile::TempDir;

    #[test]
    fn header_emitted_on_first_append() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let cfg = AofConfig::default();
        let segment = Segment::create_active(
            SegmentId::new(1),
            0,
            0,
            0,
            &cfg,
            &path,
        )
        .expect("create");

        let mut buf = [0u8; 4];
        File::open(&path)
            .expect("open")
            .read_exact(&mut buf)
            .expect("read");
        assert_eq!(buf, [0; 4]);

        segment
            .append_record(b"hello", 42)
            .expect("append");

        File::open(&path)
            .expect("open")
            .read_exact(&mut buf)
            .expect("read");
        assert_eq!(buf, SEGMENT_MAGIC.to_le_bytes());
    }
}
