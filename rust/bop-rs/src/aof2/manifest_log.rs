use std::cmp::min;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use crc64fast_nvme::Digest;
use memmap2::{Mmap, MmapMut};

use crate::aof2::config::SegmentId;
use crate::aof2::error::{AofError, AofResult};
use crate::aof2::store::tier1::{ManifestEntry, Tier1ResidencyState};
use crate::aof2::store::tier2::Tier2Metadata;

const CHUNK_MAGIC: u64 = 0x414F46324D414E49; // "AOF2MANI"
const CHUNK_VERSION: u32 = 1;
const CHUNK_HEADER_LEN: usize = 64;
const RECORD_HEADER_LEN: usize = 40;
const FLAG_SEALED: u32 = 0x1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordType {
    Upsert = 1,
    Remove = 2,
}

impl RecordType {
    fn from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(RecordType::Upsert),
            2 => Some(RecordType::Remove),
            _ => None,
        }
    }
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

struct ChunkHeader {
    flags: u32,
    chunk_index: u64,
    base_record: u64,
    tail_offset: usize,
    committed_len: usize,
    checksum: u64,
}

impl ChunkHeader {
    fn read(bytes: &[u8]) -> AofResult<Self> {
        if bytes.len() < CHUNK_HEADER_LEN {
            return Err(AofError::Corruption("manifest chunk header truncated".into()));
        }
        let magic = LittleEndian::read_u64(&bytes[0..8]);
        if magic != CHUNK_MAGIC {
            return Err(AofError::Corruption("invalid manifest chunk magic".into()));
        }
        let version = LittleEndian::read_u32(&bytes[8..12]);
        if version != CHUNK_VERSION {
            return Err(AofError::Corruption("unsupported manifest chunk version".into()));
        }
        let header_len = LittleEndian::read_u32(&bytes[12..16]) as usize;
        if header_len != CHUNK_HEADER_LEN {
            return Err(AofError::Corruption("unexpected manifest chunk header len".into()));
        }
        let flags = LittleEndian::read_u32(&bytes[16..20]);
        let chunk_index = LittleEndian::read_u64(&bytes[24..32]);
        let base_record = LittleEndian::read_u64(&bytes[32..40]);
        let tail_offset = LittleEndian::read_u64(&bytes[40..48]) as usize;
        let committed_len = LittleEndian::read_u64(&bytes[48..56]) as usize;
        let checksum = LittleEndian::read_u64(&bytes[56..64]);
        Ok(Self {
            flags,
            chunk_index,
            base_record,
            tail_offset,
            committed_len,
            checksum,
        })
    }

    fn write(&self, bytes: &mut [u8]) {
        LittleEndian::write_u64(&mut bytes[0..8], CHUNK_MAGIC);
        LittleEndian::write_u32(&mut bytes[8..12], CHUNK_VERSION);
        LittleEndian::write_u32(&mut bytes[12..16], CHUNK_HEADER_LEN as u32);
        LittleEndian::write_u32(&mut bytes[16..20], self.flags);
        LittleEndian::write_u32(&mut bytes[20..24], 0);
        LittleEndian::write_u64(&mut bytes[24..32], self.chunk_index);
        LittleEndian::write_u64(&mut bytes[32..40], self.base_record);
        LittleEndian::write_u64(&mut bytes[40..48], self.tail_offset as u64);
        LittleEndian::write_u64(&mut bytes[48..56], self.committed_len as u64);
        LittleEndian::write_u64(&mut bytes[56..64], self.checksum);
    }
}

struct ChunkWriter {
    file: File,
    mmap: MmapMut,
    header: ChunkHeader,
    capacity: usize,
    path: PathBuf,
}

impl ChunkWriter {
    fn create(dir: &Path, chunk_index: u64, base_record: u64, capacity: usize) -> AofResult<Self> {
        fs::create_dir_all(dir).map_err(AofError::from)?;
        let path = dir.join(format!("chunk_{:016x}.log", chunk_index));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .map_err(AofError::from)?;
        file.set_len(capacity as u64).map_err(AofError::from)?;
        let mut mmap = unsafe { MmapMut::map_mut(&file).map_err(AofError::from)? };
        let header = ChunkHeader {
            flags: 0,
            chunk_index,
            base_record,
            tail_offset: CHUNK_HEADER_LEN,
            committed_len: CHUNK_HEADER_LEN,
            checksum: 0,
        };
        header.write(&mut mmap[..CHUNK_HEADER_LEN]);
        mmap.flush_range(0, CHUNK_HEADER_LEN).map_err(AofError::from)?;
        Ok(Self {
            file,
            mmap,
            header,
            capacity,
            path,
        })
    }

    fn open_existing(path: PathBuf, capacity: usize) -> AofResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(AofError::from)?;
        if file.metadata().map_err(AofError::from)?.len() as usize != capacity {
            return Err(AofError::Corruption(format!(
                "manifest chunk {} unexpected size",
                path.display()
            )));
        }
        let mut mmap = unsafe { MmapMut::map_mut(&file).map_err(AofError::from)? };
        let header = ChunkHeader::read(&mmap[..CHUNK_HEADER_LEN])?;
        Ok(Self {
            file,
            mmap,
            header,
            capacity,
            path,
        })
    }

    fn remaining(&self) -> usize {
        self.capacity.saturating_sub(self.header.tail_offset)
    }

    fn append(&mut self, record: &EncodedRecord) -> AofResult<()> {
        let total = RECORD_HEADER_LEN + record.payload.len();
        if self.remaining() < total {
            return Err(AofError::Corruption("manifest chunk overflow".into()));
        }
        let offset = self.header.tail_offset;
        self.mmap[offset..offset + RECORD_HEADER_LEN]
            .copy_from_slice(&record.header_bytes);
        if !record.payload.is_empty() {
            self.mmap[offset + RECORD_HEADER_LEN..offset + total]
                .copy_from_slice(&record.payload);
        }
        self.header.tail_offset += total;
        self.header.committed_len = self.header.tail_offset;
        self.header.write(&mut self.mmap[..CHUNK_HEADER_LEN]);
        Ok(())
    }

    fn flush(&mut self) -> AofResult<()> {
        let data_len = self.header.tail_offset.saturating_sub(CHUNK_HEADER_LEN);
        if data_len > 0 {
            self.mmap
                .flush_range(CHUNK_HEADER_LEN, data_len)
                .map_err(AofError::from)?;
        }
        let crc = crc64(&self.mmap[CHUNK_HEADER_LEN..self.header.committed_len]);
        self.header.checksum = crc;
        self.header.write(&mut self.mmap[..CHUNK_HEADER_LEN]);
        self.mmap.flush_range(0, CHUNK_HEADER_LEN).map_err(AofError::from)?;
        self.file.sync_data().map_err(AofError::from)?;
        Ok(())
    }

    fn seal(mut self) -> AofResult<SealedChunk> {
        self.header.flags |= FLAG_SEALED;
        self.header.write(&mut self.mmap[..CHUNK_HEADER_LEN]);
        self.mmap.flush_range(0, CHUNK_HEADER_LEN).map_err(AofError::from)?;
        self.file.sync_data().map_err(AofError::from)?;
        Ok(SealedChunk {
            path: self.path,
            chunk_index: self.header.chunk_index,
            committed_len: self.header.committed_len,
            checksum: self.header.checksum,
        })
    }
}

struct ChunkReader {
    mmap: Mmap,
    header: ChunkHeader,
}

impl ChunkReader {
    fn open(path: &Path, capacity: usize) -> AofResult<Self> {
        let file = File::open(path).map_err(AofError::from)?;
        if file.metadata().map_err(AofError::from)?.len() as usize != capacity {
            return Err(AofError::Corruption(format!(
                "manifest chunk {} unexpected size",
                path.display()
            )));
        }
        let mmap = unsafe { Mmap::map(&file).map_err(AofError::from)? };
        let header = ChunkHeader::read(&mmap[..CHUNK_HEADER_LEN])?;
        if header.committed_len > capacity {
            return Err(AofError::Corruption("manifest chunk committed_len too large".into()));
        }
        let computed_crc = crc64(&mmap[CHUNK_HEADER_LEN..header.committed_len]);
        if computed_crc != header.checksum && header.committed_len > CHUNK_HEADER_LEN {
            return Err(AofError::Corruption("manifest chunk checksum mismatch".into()));
        }
        Ok(Self { mmap, header })
    }

    fn iter_records(&self) -> ManifestRecordIter<'_> {
        ManifestRecordIter {
            data: &self.mmap,
            offset: CHUNK_HEADER_LEN,
            committed: self.header.committed_len,
        }
    }
}

struct ManifestRecordIter<'a> {
    data: &'a [u8],
    offset: usize,
    committed: usize,
}

impl<'a> Iterator for ManifestRecordIter<'a> {
    type Item = AofResult<DecodedRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.committed {
            return None;
        }
        if self.committed - self.offset < RECORD_HEADER_LEN {
            return Some(Err(AofError::Corruption(
                "manifest record truncated".into(),
            )));
        }
        let header_bytes = &self.data[self.offset..self.offset + RECORD_HEADER_LEN];
        let record_type = LittleEndian::read_u16(&header_bytes[0..2]);
        let payload_len = LittleEndian::read_u32(&header_bytes[4..8]) as usize;
        let segment_id = SegmentId::new(LittleEndian::read_u64(&header_bytes[8..16]));
        let logical_offset = LittleEndian::read_u64(&header_bytes[16..24]);
        let timestamp_ms = LittleEndian::read_u64(&header_bytes[24..32]);
        let crc64 = LittleEndian::read_u64(&header_bytes[32..40]);
        let total = RECORD_HEADER_LEN + payload_len;
        if self.committed - self.offset < total {
            return Some(Err(AofError::Corruption(
                "manifest record payload truncated".into(),
            )));
        }
        let payload = &self.data[self.offset + RECORD_HEADER_LEN..self.offset + total];
        if crc64 != 0 {
            let mut digest = Digest::new();
            digest.write(payload);
            let computed = digest.sum64();
            if computed != crc64 {
                return Some(Err(AofError::Corruption(
                    "manifest record crc mismatch".into(),
                )));
            }
        }
        self.offset += total;
        let kind = match RecordType::from_u16(record_type) {
            Some(k) => k,
            None => {
                return Some(Err(AofError::Corruption(
                    "unknown manifest record type".into(),
                )))
            }
        };
        Some(Ok(DecodedRecord {
            kind,
            segment_id,
            logical_offset,
            timestamp_ms,
            payload: payload.to_vec(),
        }))
    }
}

struct EncodedRecord {
    header_bytes: [u8; RECORD_HEADER_LEN],
    payload: Vec<u8>,
}

struct DecodedRecord {
    kind: RecordType,
    segment_id: SegmentId,
    logical_offset: u64,
    timestamp_ms: u64,
    payload: Vec<u8>,
}

fn crc64(data: &[u8]) -> u64 {
    if data.is_empty() {
        return 0;
    }
    let mut digest = Digest::new();
    digest.write(data);
    digest.sum64()
}

pub struct ManifestLog {
    dir: PathBuf,
    chunk_bytes: usize,
    next_chunk_index: u64,
    next_record_index: u64,
    writer: Option<ChunkWriter>,
    sealed_chunks: Vec<SealedChunk>,
}

#[derive(Debug, Clone)]
pub struct SealedChunk {
    pub path: PathBuf,
    pub chunk_index: u64,
    pub committed_len: usize,
    pub checksum: u64,
}

impl ManifestLog {
    pub fn open(dir: PathBuf, chunk_bytes: usize) -> AofResult<(Self, HashMap<SegmentId, ManifestEntry>)> {
        fs::create_dir_all(&dir).map_err(AofError::from)?;
        let mut entries = HashMap::new();
        let mut files: Vec<(u64, PathBuf)> = fs::read_dir(&dir)
            .map_err(AofError::from)?
            .filter_map(|entry| {
                entry.ok().and_then(|e| {
                    let path = e.path();
                    let name = path.file_name()?.to_str()?.to_string();
                    if let Some(stripped) = name.strip_prefix("chunk_") {
                        if let Some(hex) = stripped.strip_suffix(".log") {
                            if let Ok(idx) = u64::from_str_radix(hex, 16) {
                                return Some((idx, path));
                            }
                        }
                    }
                    None
                })
            })
            .collect();
        files.sort_by_key(|(idx, _)| *idx);

        let mut next_chunk_index = 0u64;
        let mut next_record_index = 0u64;
        let mut open_writer: Option<ChunkWriter> = None;

        for (idx, path) in files.into_iter() {
            let reader = ChunkReader::open(&path, chunk_bytes)?;
            let mut record_count = 0u64;
            for record in reader.iter_records() {
                let record = record?;
                match record.kind {
                    RecordType::Upsert => {
                        let mut entry = decode_manifest_entry(record.segment_id, &record.payload)?;
                        entry.segment_id = record.segment_id;
                        entries.insert(record.segment_id, entry);
                    }
                    RecordType::Remove => {
                        entries.remove(&record.segment_id);
                    }
                }
                next_record_index += 1;
                record_count += 1;
            }
            next_chunk_index = idx + 1;
            let sealed = (reader.header.flags & FLAG_SEALED) != 0;
            if !sealed {
                // reopen this chunk for appends
                open_writer = Some(ChunkWriter::open_existing(path, chunk_bytes)?);
            }
        }

        let log = ManifestLog {
            dir,
            chunk_bytes,
            next_chunk_index,
            next_record_index,
            writer: open_writer,
            sealed_chunks: Vec::new(),
        };
        Ok((log, entries))
    }

    fn ensure_writer(&mut self) -> AofResult<&mut ChunkWriter> {
        if self.writer.is_none() {
            let writer = ChunkWriter::create(
                &self.dir,
                self.next_chunk_index,
                self.next_record_index,
                self.chunk_bytes,
            )?;
            self.writer = Some(writer);
        }
        Ok(self.writer.as_mut().unwrap())
    }

    pub fn append_upsert(&mut self, entry: &ManifestEntry) -> AofResult<()> {
        let payload = encode_manifest_entry(entry)?;
        let record = build_record(RecordType::Upsert, entry.segment_id, entry.base_offset, &payload);
        self.append_record(record)
    }

    pub fn append_remove(&mut self, segment_id: SegmentId) -> AofResult<()> {
        let record = build_record(RecordType::Remove, segment_id, 0, &[]);
        self.append_record(record)
    }

    fn append_record(&mut self, record: EncodedRecord) -> AofResult<()> {
        let mut writer = self.ensure_writer()?;
        let needed = RECORD_HEADER_LEN + record.payload.len();
        if writer.remaining() < needed {
            let sealed = writer
                .seal()
                .map_err(|err| AofError::Io(io::Error::new(io::ErrorKind::Other, format!(
                    "failed to seal manifest chunk: {}",
                    err
                ))))?;
            self.sealed_chunks.push(sealed);
            self.writer = None;
            self.next_chunk_index += 1;
            writer = self.ensure_writer()?;
        }
        writer.append(&record)?;
        self.next_record_index += 1;
        Ok(())
    }

    pub fn flush(&mut self) -> AofResult<()> {
        if let Some(writer) = self.writer.as_mut() {
            writer.flush()?;
        }
        Ok(())
    }

    pub fn seal_current(&mut self) -> AofResult<()> {
        if let Some(writer) = self.writer.take() {
            let sealed = writer.seal().map_err(|err| {
                AofError::Io(io::Error::new(
                    io::ErrorKind::Other,
                    format!("failed to seal manifest chunk: {}", err),
                ))
            })?;
            self.sealed_chunks.push(sealed);
            self.next_chunk_index += 1;
        }
        Ok(())
    }

    pub fn take_sealed_chunks(&mut self) -> Vec<SealedChunk> {
        std::mem::take(&mut self.sealed_chunks)
    }
}

fn build_record(kind: RecordType, segment_id: SegmentId, logical_offset: u64, payload: &[u8]) -> EncodedRecord {
    let mut header_bytes = [0u8; RECORD_HEADER_LEN];
    LittleEndian::write_u16(&mut header_bytes[0..2], kind as u16);
    LittleEndian::write_u16(&mut header_bytes[2..4], 0);
    LittleEndian::write_u32(&mut header_bytes[4..8], payload.len() as u32);
    LittleEndian::write_u64(&mut header_bytes[8..16], segment_id.as_u64());
    LittleEndian::write_u64(&mut header_bytes[16..24], logical_offset);
    LittleEndian::write_u64(&mut header_bytes[24..32], current_epoch_ms());
    let crc = crc64(payload);
    LittleEndian::write_u64(&mut header_bytes[32..40], crc);
    EncodedRecord {
        header_bytes,
        payload: payload.to_vec(),
    }
}

fn encode_manifest_entry(entry: &ManifestEntry) -> AofResult<Vec<u8>> {
    let path_bytes = entry.compressed_path.as_bytes();
    let dict_bytes = entry.dictionary.as_ref().map(|s| s.as_bytes()).unwrap_or(&[]);
    let dict_flag = entry.dictionary.is_some() as u8;
    let offset_len = entry.offset_index.len() as u32;
    let tier2_flag = entry.tier2.is_some() as u8;
    let (tier2_key_bytes, tier2_etag_bytes, tier2_size, tier2_uploaded) = if let Some(ref meta) = entry.tier2 {
        (
            meta.object_key.as_bytes(),
            meta.etag.as_ref().map(|s| s.as_bytes()).unwrap_or(&[]),
            meta.size_bytes,
            meta.uploaded_epoch_ms,
        )
    } else {
        (&[][..], &[][..], 0u64, 0u64)
    };
    let etag_flag = entry.tier2.as_ref().and_then(|m| m.etag.as_ref()).is_some() as u8;

    let mut payload = Vec::with_capacity(
        8 * 9 + 4 + path_bytes.len() + dict_bytes.len() + (offset_len as usize * 4)
            + tier2_key_bytes.len()
            + tier2_etag_bytes.len()
            + 16,
    );
    payload.write_u64::<LittleEndian>(entry.base_offset)?;
    payload.write_u64::<LittleEndian>(entry.base_record_count)?;
    payload.write_i64::<LittleEndian>(entry.created_at)?;
    payload.write_i64::<LittleEndian>(entry.sealed_at)?;
    payload.write_u32::<LittleEndian>(entry.checksum)?;
    payload.write_u64::<LittleEndian>(entry.original_bytes)?;
    payload.write_u64::<LittleEndian>(entry.compressed_bytes)?;
    payload.write_u32::<LittleEndian>(path_bytes.len() as u32)?;
    payload.extend_from_slice(path_bytes);
    payload.write_u8(dict_flag)?;
    payload.write_u32::<LittleEndian>(dict_bytes.len() as u32)?;
    payload.extend_from_slice(dict_bytes);
    payload.write_u32::<LittleEndian>(offset_len)?;
    for offset in &entry.offset_index {
        payload.write_u32::<LittleEndian>(*offset)?;
    }
    payload.write_u8(residency_to_u8(entry.residency))?;
    payload.write_u8(tier2_flag)?;
    payload.write_u32::<LittleEndian>(tier2_key_bytes.len() as u32)?;
    payload.extend_from_slice(tier2_key_bytes);
    payload.write_u8(etag_flag)?;
    payload.write_u32::<LittleEndian>(tier2_etag_bytes.len() as u32)?;
    payload.extend_from_slice(tier2_etag_bytes);
    payload.write_u64::<LittleEndian>(tier2_size)?;
    payload.write_u64::<LittleEndian>(tier2_uploaded)?;
    payload.write_u64::<LittleEndian>(entry.last_access_epoch_ms)?;
    Ok(payload)
}

fn decode_manifest_entry(mut payload: &[u8]) -> AofResult<ManifestEntry> {
    let mut cursor = Cursor::new(&mut payload);
    let base_offset = cursor.read_u64::<LittleEndian>()?;
    let base_record_count = cursor.read_u64::<LittleEndian>()?;
    let created_at = cursor.read_i64::<LittleEndian>()?;
    let sealed_at = cursor.read_i64::<LittleEndian>()?;
    let checksum = cursor.read_u32::<LittleEndian>()?;
    let original_bytes = cursor.read_u64::<LittleEndian>()?;
    let compressed_bytes = cursor.read_u64::<LittleEndian>()?;
    let path_len = cursor.read_u32::<LittleEndian>()? as usize;
    let mut path_bytes = vec![0u8; path_len];
    cursor.read_exact(&mut path_bytes)?;
    let dict_flag = cursor.read_u8()?;
    let dict_len = cursor.read_u32::<LittleEndian>()? as usize;
    let mut dict_bytes = vec![0u8; dict_len];
    if dict_len > 0 {
        cursor.read_exact(&mut dict_bytes)?;
    }
    let offset_len = cursor.read_u32::<LittleEndian>()? as usize;
    let mut offset_index = Vec::with_capacity(offset_len);
    for _ in 0..offset_len {
        offset_index.push(cursor.read_u32::<LittleEndian>()?);
    }
    let residency = cursor.read_u8()?;
    let tier2_flag = cursor.read_u8()? != 0;
    let tier2_key_len = cursor.read_u32::<LittleEndian>()? as usize;
    let mut tier2_key = vec![0u8; tier2_key_len];
    if tier2_key_len > 0 {
        cursor.read_exact(&mut tier2_key)?;
    }
    let etag_flag = cursor.read_u8()? != 0;
    let tier2_etag_len = cursor.read_u32::<LittleEndian>()? as usize;
    let mut tier2_etag = vec![0u8; tier2_etag_len];
    if tier2_etag_len > 0 {
        cursor.read_exact(&mut tier2_etag)?;
    }
    let tier2_size = cursor.read_u64::<LittleEndian>()?;
    let tier2_uploaded = cursor.read_u64::<LittleEndian>()?;
    let last_access_epoch_ms = cursor.read_u64::<LittleEndian>()?;

    let dictionary = if dict_flag != 0 {
        Some(String::from_utf8(dict_bytes).map_err(|err| {
            AofError::Corruption(format!("manifest dictionary utf8 error: {}", err))
        })?)
    } else {
        None
    };
    let compressed_path = String::from_utf8(path_bytes).map_err(|err| {
        AofError::Corruption(format!("manifest path utf8 error: {}", err))
    })?;
    let tier2 = if tier2_flag {
        Some(Tier2Metadata {
            object_key: String::from_utf8(tier2_key).map_err(|err| {
                AofError::Corruption(format!("manifest tier2 key utf8 error: {}", err))
            })?,
            etag: if etag_flag {
                Some(String::from_utf8(tier2_etag).map_err(|err| {
                    AofError::Corruption(format!("manifest tier2 etag utf8 error: {}", err))
                })?)
            } else {
                None
            },
            size_bytes: tier2_size,
            uploaded_epoch_ms: tier2_uploaded,
        })
    } else {
        None
    };

    Ok(ManifestEntry {
        segment_id,
        base_offset,
        base_record_count,
        created_at,
        sealed_at,
        checksum,
        original_bytes,
        compressed_bytes,
        compressed_path,
        dictionary,
        offset_index,
        residency: residency_from_u8(residency)?,
        tier2,
        last_access_epoch_ms,
    })
}

fn residency_to_u8(residency: Tier1ResidencyState) -> u8 {
    match residency {
        Tier1ResidencyState::ResidentInTier0 => 0,
        Tier1ResidencyState::StagedForTier0 => 1,
        Tier1ResidencyState::UploadedToTier2 => 2,
    }
}

fn residency_from_u8(value: u8) -> AofResult<Tier1ResidencyState> {
    match value {
        0 => Ok(Tier1ResidencyState::ResidentInTier0),
        1 => Ok(Tier1ResidencyState::StagedForTier0),
        2 => Ok(Tier1ResidencyState::UploadedToTier2),
        _ => Err(AofError::Corruption("invalid residency state".into())),
    }
}
