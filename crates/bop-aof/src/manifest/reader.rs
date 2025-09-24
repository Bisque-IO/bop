use std::fs::{self, File};
use std::path::PathBuf;

use memmap2::Mmap;

use super::chunk::{CHUNK_HEADER_LEN, ChunkHeader, crc64};
use super::record::{
    ManifestRecord, ManifestRecordHeader, ManifestRecordPayload, RECORD_HEADER_LEN,
};
use crate::error::{AofError, AofResult};

/// Reader for manifest logs during recovery and replay.
///
/// Provides sequential access to manifest records stored in chunks,
/// with integrity verification and proper ordering.
///
/// ## Usage Pattern
///
/// ```ignore
/// let reader = ManifestLogReader::open(base_dir, stream_id)?;
/// for record in reader.iter() {
///     process_manifest_record(record?)?;
/// }
/// ```
pub struct ManifestLogReader {
    /// Memory-mapped chunks in sequential order
    chunks: Vec<ReplayChunk>,
}

impl ManifestLogReader {
    pub fn open(base_dir: PathBuf, stream_id: u64) -> AofResult<Self> {
        let stream_dir = base_dir.join(format!("{stream_id:020}"));
        if !stream_dir.exists() {
            return Ok(Self { chunks: Vec::new() });
        }
        let mut files: Vec<(u64, PathBuf)> = fs::read_dir(&stream_dir)
            .map_err(AofError::from)?
            .filter_map(|entry| match entry {
                Ok(dir_entry) => {
                    let path = dir_entry.path();
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(index) = name.parse::<u64>() {
                            if path.extension().and_then(|s| s.to_str()) == Some("mlog") {
                                return Some((index, path));
                            }
                        }
                    }
                    None
                }
                Err(_) => None,
            })
            .collect();
        files.sort_by_key(|(idx, _)| *idx);

        let mut chunks = Vec::with_capacity(files.len());
        for (expected_index, path) in files {
            let file = File::open(&path)?;
            let mmap = unsafe { Mmap::map(&file)? };
            if mmap.len() < CHUNK_HEADER_LEN {
                return Err(AofError::Corruption(format!(
                    "manifest chunk {} too small",
                    path.display()
                )));
            }
            let header = ChunkHeader::read_from(&mmap[..CHUNK_HEADER_LEN])?;
            if header.chunk_index != expected_index {
                return Err(AofError::Corruption(format!(
                    "manifest chunk index mismatch: expected {expected_index}, found {}",
                    header.chunk_index
                )));
            }
            if header.committed_len > mmap.len() {
                return Err(AofError::Corruption(format!(
                    "manifest chunk {} committed_len beyond file",
                    path.display()
                )));
            }
            if header.committed_len > CHUNK_HEADER_LEN {
                let computed = crc64(&mmap[CHUNK_HEADER_LEN..header.committed_len]);
                if computed != header.chunk_crc64 {
                    return Err(AofError::Corruption(format!(
                        "manifest chunk {} crc mismatch",
                        path.display()
                    )));
                }
            }
            chunks.push(ReplayChunk { mmap, header });
        }

        Ok(Self { chunks })
    }

    pub fn iter(&self) -> ManifestRecordIter<'_> {
        ManifestRecordIter {
            chunks: &self.chunks,
            chunk_index: 0,
            offset: CHUNK_HEADER_LEN,
        }
    }
}

/// Memory-mapped chunk for efficient record replay.
struct ReplayChunk {
    /// Memory-mapped chunk file
    mmap: Mmap,
    /// Parsed chunk header with metadata
    header: ChunkHeader,
}

/// Iterator over manifest records for sequential replay.
///
/// Maintains position across chunk boundaries and handles
/// integrity verification for each record.
pub struct ManifestRecordIter<'a> {
    /// All chunks to iterate over
    chunks: &'a [ReplayChunk],
    /// Current chunk being read
    chunk_index: usize,
    /// Current offset within chunk
    offset: usize,
}

impl<'a> Iterator for ManifestRecordIter<'a> {
    type Item = AofResult<ManifestRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let chunk = self.chunks.get(self.chunk_index)?;
            if self.offset >= chunk.header.committed_len {
                self.chunk_index += 1;
                self.offset = CHUNK_HEADER_LEN;
                continue;
            }
            if chunk.header.committed_len - self.offset < RECORD_HEADER_LEN {
                return Some(Err(AofError::Corruption(
                    "manifest record header truncated".into(),
                )));
            }
            let header_bytes = &chunk.mmap[self.offset..self.offset + RECORD_HEADER_LEN];
            let header = match ManifestRecordHeader::decode(header_bytes) {
                Ok(h) => h,
                Err(err) => return Some(Err(err)),
            };
            self.offset += RECORD_HEADER_LEN;

            let payload_len = header.payload_len as usize;
            if chunk.header.committed_len - self.offset < payload_len {
                return Some(Err(AofError::Corruption(
                    "manifest record payload truncated".into(),
                )));
            }
            let payload_slice = &chunk.mmap[self.offset..self.offset + payload_len];
            self.offset += payload_len;

            if header.payload_crc64 != 0 {
                let computed = crc64(payload_slice);
                if computed != header.payload_crc64 {
                    return Some(Err(AofError::Corruption(
                        "manifest record payload crc mismatch".into(),
                    )));
                }
            }

            let payload = match ManifestRecordPayload::decode(header.record_type, payload_slice) {
                Ok(payload) => payload,
                Err(err) => return Some(Err(err)),
            };
            let record = ManifestRecord {
                segment_id: header.segment_id,
                logical_offset: header.logical_offset,
                timestamp_ms: header.timestamp_ms,
                payload,
            };
            return Some(Ok(record));
        }
    }
}
