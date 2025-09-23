use std::io::{Cursor, Read};

use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};

use super::chunk::crc64;
use crate::config::SegmentId;
use crate::error::{AofError, AofResult};

pub const RECORD_HEADER_LEN: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum RecordType {
    SegmentOpened = 0,
    SealSegment = 1,
    CompressionStarted = 2,
    CompressionDone = 3,
    CompressionFailed = 4,
    UploadStarted = 5,
    UploadDone = 6,
    UploadFailed = 7,
    Tier2DeleteQueued = 8,
    Tier2Deleted = 9,
    HydrationStarted = 10,
    HydrationDone = 11,
    HydrationFailed = 12,
    LocalEvicted = 13,
    SnapshotMarker = 14,
    CustomEvent = 15,
    Tier1Snapshot = 16,
}

impl TryFrom<u16> for RecordType {
    type Error = AofError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => RecordType::SegmentOpened,
            1 => RecordType::SealSegment,
            2 => RecordType::CompressionStarted,
            3 => RecordType::CompressionDone,
            4 => RecordType::CompressionFailed,
            5 => RecordType::UploadStarted,
            6 => RecordType::UploadDone,
            7 => RecordType::UploadFailed,
            8 => RecordType::Tier2DeleteQueued,
            9 => RecordType::Tier2Deleted,
            10 => RecordType::HydrationStarted,
            11 => RecordType::HydrationDone,
            12 => RecordType::HydrationFailed,
            13 => RecordType::LocalEvicted,
            14 => RecordType::SnapshotMarker,
            15 => RecordType::CustomEvent,
            16 => RecordType::Tier1Snapshot,
            _ => {
                return Err(AofError::Corruption(format!(
                    "unknown manifest record type: {value}"
                )));
            }
        })
    }
}

#[derive(Debug, Clone)]
pub enum ManifestRecordPayload {
    SegmentOpened {
        base_offset: u64,
        epoch: u32,
    },
    SealSegment {
        durable_bytes: u64,
        segment_crc64: u64,
        ext_id: u64,
        coordinator_watermark: u64,
        flush_failure: bool,
    },
    CompressionStarted {
        job_id: u32,
    },
    CompressionDone {
        compressed_bytes: u64,
        dictionary_id: u32,
    },
    CompressionFailed {
        reason_code: u32,
    },
    UploadStarted {
        key: String,
    },
    UploadDone {
        key: String,
        etag: Option<String>,
    },
    UploadFailed {
        key: String,
        reason_code: u32,
    },
    Tier2DeleteQueued {
        key: String,
    },
    Tier2Deleted {
        key: String,
    },
    HydrationStarted {
        source_tier: u64,
    },
    HydrationDone {
        resident_bytes: u64,
    },
    HydrationFailed {
        reason_code: u32,
    },
    LocalEvicted {
        reclaimed_bytes: u64,
    },
    SnapshotMarker {
        chunk_index: u64,
        chunk_offset: u64,
        snapshot_id: u64,
    },
    CustomEvent {
        schema_id: u32,
        blob: Vec<u8>,
    },
    Tier1Snapshot {
        json: Vec<u8>,
    },
}

impl ManifestRecordPayload {
    pub fn record_type(&self) -> RecordType {
        match self {
            ManifestRecordPayload::SegmentOpened { .. } => RecordType::SegmentOpened,
            ManifestRecordPayload::SealSegment { .. } => RecordType::SealSegment,
            ManifestRecordPayload::CompressionStarted { .. } => RecordType::CompressionStarted,
            ManifestRecordPayload::CompressionDone { .. } => RecordType::CompressionDone,
            ManifestRecordPayload::CompressionFailed { .. } => RecordType::CompressionFailed,
            ManifestRecordPayload::UploadStarted { .. } => RecordType::UploadStarted,
            ManifestRecordPayload::UploadDone { .. } => RecordType::UploadDone,
            ManifestRecordPayload::UploadFailed { .. } => RecordType::UploadFailed,
            ManifestRecordPayload::Tier2DeleteQueued { .. } => RecordType::Tier2DeleteQueued,
            ManifestRecordPayload::Tier2Deleted { .. } => RecordType::Tier2Deleted,
            ManifestRecordPayload::HydrationStarted { .. } => RecordType::HydrationStarted,
            ManifestRecordPayload::HydrationDone { .. } => RecordType::HydrationDone,
            ManifestRecordPayload::HydrationFailed { .. } => RecordType::HydrationFailed,
            ManifestRecordPayload::LocalEvicted { .. } => RecordType::LocalEvicted,
            ManifestRecordPayload::SnapshotMarker { .. } => RecordType::SnapshotMarker,
            ManifestRecordPayload::CustomEvent { .. } => RecordType::CustomEvent,
            ManifestRecordPayload::Tier1Snapshot { .. } => RecordType::Tier1Snapshot,
        }
    }

    pub fn encode(&self) -> AofResult<Vec<u8>> {
        let mut buf = Vec::new();
        match self {
            ManifestRecordPayload::SegmentOpened { base_offset, epoch } => {
                buf.write_u64::<LittleEndian>(*base_offset)?;
                buf.write_u32::<LittleEndian>(*epoch)?;
            }
            ManifestRecordPayload::SealSegment {
                durable_bytes,
                segment_crc64,
                ext_id,
                coordinator_watermark,
                flush_failure,
            } => {
                buf.write_u64::<LittleEndian>(*durable_bytes)?;
                buf.write_u64::<LittleEndian>(*segment_crc64)?;
                buf.write_u64::<LittleEndian>(*ext_id)?;
                buf.write_u64::<LittleEndian>(*coordinator_watermark)?;
                buf.write_u8(if *flush_failure { 1 } else { 0 })?;
            }
            ManifestRecordPayload::CompressionStarted { job_id } => {
                buf.write_u32::<LittleEndian>(*job_id)?;
            }
            ManifestRecordPayload::CompressionDone {
                compressed_bytes,
                dictionary_id,
            } => {
                buf.write_u64::<LittleEndian>(*compressed_bytes)?;
                buf.write_u32::<LittleEndian>(*dictionary_id)?;
            }
            ManifestRecordPayload::CompressionFailed { reason_code } => {
                buf.write_u32::<LittleEndian>(*reason_code)?;
            }
            ManifestRecordPayload::UploadStarted { key } => {
                write_short_blob(&mut buf, key.as_bytes())?;
            }
            ManifestRecordPayload::UploadDone { key, etag } => {
                write_short_blob(&mut buf, key.as_bytes())?;
                let etag_bytes = etag.as_deref().unwrap_or("").as_bytes();
                ensure_short_len(etag_bytes.len(), "upload etag")?;
                buf.write_u16::<LittleEndian>(etag_bytes.len() as u16)?;
                buf.extend_from_slice(etag_bytes);
            }
            ManifestRecordPayload::UploadFailed { key, reason_code } => {
                ensure_short_len(key.as_bytes().len(), "upload failure key")?;
                buf.write_u16::<LittleEndian>(key.as_bytes().len() as u16)?;
                buf.write_u32::<LittleEndian>(*reason_code)?;
                buf.extend_from_slice(key.as_bytes());
            }
            ManifestRecordPayload::Tier2DeleteQueued { key }
            | ManifestRecordPayload::Tier2Deleted { key } => {
                write_short_blob(&mut buf, key.as_bytes())?;
            }
            ManifestRecordPayload::HydrationStarted { source_tier } => {
                buf.write_u64::<LittleEndian>(*source_tier)?;
            }
            ManifestRecordPayload::HydrationDone { resident_bytes } => {
                buf.write_u64::<LittleEndian>(*resident_bytes)?;
            }
            ManifestRecordPayload::HydrationFailed { reason_code } => {
                buf.write_u32::<LittleEndian>(*reason_code)?;
            }
            ManifestRecordPayload::LocalEvicted { reclaimed_bytes } => {
                buf.write_u64::<LittleEndian>(*reclaimed_bytes)?;
            }
            ManifestRecordPayload::SnapshotMarker {
                chunk_index,
                chunk_offset,
                snapshot_id,
            } => {
                buf.write_u64::<LittleEndian>(*chunk_index)?;
                buf.write_u64::<LittleEndian>(*chunk_offset)?;
                buf.write_u64::<LittleEndian>(*snapshot_id)?;
            }
            ManifestRecordPayload::CustomEvent { schema_id, blob } => {
                buf.write_u32::<LittleEndian>(*schema_id)?;
                ensure_short_len(blob.len(), "custom event blob")?;
                buf.write_u16::<LittleEndian>(blob.len() as u16)?;
                buf.extend_from_slice(blob);
            }
            ManifestRecordPayload::Tier1Snapshot { json } => {
                ensure_long_len(json.len(), "tier1 snapshot")?;
                buf.write_u32::<LittleEndian>(json.len() as u32)?;
                buf.extend_from_slice(json);
            }
        }
        Ok(buf)
    }

    pub fn decode(record_type: RecordType, payload: &[u8]) -> AofResult<Self> {
        let mut cursor = Cursor::new(payload);
        let result = match record_type {
            RecordType::SegmentOpened => {
                let base_offset = cursor.read_u64::<LittleEndian>()?;
                let epoch = cursor.read_u32::<LittleEndian>()?;
                ManifestRecordPayload::SegmentOpened { base_offset, epoch }
            }
            RecordType::SealSegment => {
                let durable_bytes = cursor.read_u64::<LittleEndian>()?;
                let segment_crc64 = cursor.read_u64::<LittleEndian>()?;
                let ext_id = cursor.read_u64::<LittleEndian>()?;
                let coordinator_watermark = cursor.read_u64::<LittleEndian>()?;
                let flush_failure = cursor.read_u8()? != 0;
                ManifestRecordPayload::SealSegment {
                    durable_bytes,
                    segment_crc64,
                    ext_id,
                    coordinator_watermark,
                    flush_failure,
                }
            }
            RecordType::CompressionStarted => {
                let job_id = cursor.read_u32::<LittleEndian>()?;
                ManifestRecordPayload::CompressionStarted { job_id }
            }
            RecordType::CompressionDone => {
                let compressed_bytes = cursor.read_u64::<LittleEndian>()?;
                let dictionary_id = cursor.read_u32::<LittleEndian>()?;
                ManifestRecordPayload::CompressionDone {
                    compressed_bytes,
                    dictionary_id,
                }
            }
            RecordType::CompressionFailed => {
                let reason_code = cursor.read_u32::<LittleEndian>()?;
                ManifestRecordPayload::CompressionFailed { reason_code }
            }
            RecordType::UploadStarted => {
                let key = read_short_string(&mut cursor, payload)?;
                ManifestRecordPayload::UploadStarted { key }
            }
            RecordType::UploadDone => {
                let key = read_short_string(&mut cursor, payload)?;
                let etag_len = cursor.read_u16::<LittleEndian>()? as usize;
                ensure_remaining(payload, cursor.position(), etag_len)?;
                let mut buf = vec![0u8; etag_len];
                cursor.read_exact(&mut buf)?;
                let etag = if etag_len == 0 {
                    None
                } else {
                    Some(String::from_utf8(buf).map_err(|err| {
                        AofError::Corruption(format!("upload etag utf8 error: {err}"))
                    })?)
                };
                ManifestRecordPayload::UploadDone { key, etag }
            }
            RecordType::UploadFailed => {
                let key_len = cursor.read_u16::<LittleEndian>()? as usize;
                let reason_code = cursor.read_u32::<LittleEndian>()?;
                ensure_remaining(payload, cursor.position(), key_len)?;
                let mut buf = vec![0u8; key_len];
                cursor.read_exact(&mut buf)?;
                let key = String::from_utf8(buf)
                    .map_err(|err| AofError::Corruption(format!("upload key utf8 error: {err}")))?;
                ManifestRecordPayload::UploadFailed { key, reason_code }
            }
            RecordType::Tier2DeleteQueued | RecordType::Tier2Deleted => {
                let key = read_short_string(&mut cursor, payload)?;
                if record_type == RecordType::Tier2DeleteQueued {
                    ManifestRecordPayload::Tier2DeleteQueued { key }
                } else {
                    ManifestRecordPayload::Tier2Deleted { key }
                }
            }
            RecordType::HydrationStarted => {
                let source_tier = cursor.read_u64::<LittleEndian>()?;
                ManifestRecordPayload::HydrationStarted { source_tier }
            }
            RecordType::HydrationDone => {
                let resident_bytes = cursor.read_u64::<LittleEndian>()?;
                ManifestRecordPayload::HydrationDone { resident_bytes }
            }
            RecordType::HydrationFailed => {
                let reason_code = cursor.read_u32::<LittleEndian>()?;
                ManifestRecordPayload::HydrationFailed { reason_code }
            }
            RecordType::LocalEvicted => {
                let reclaimed_bytes = cursor.read_u64::<LittleEndian>()?;
                ManifestRecordPayload::LocalEvicted { reclaimed_bytes }
            }
            RecordType::SnapshotMarker => {
                let chunk_index = cursor.read_u64::<LittleEndian>()?;
                let chunk_offset = cursor.read_u64::<LittleEndian>()?;
                let snapshot_id = cursor.read_u64::<LittleEndian>()?;
                ManifestRecordPayload::SnapshotMarker {
                    chunk_index,
                    chunk_offset,
                    snapshot_id,
                }
            }
            RecordType::CustomEvent => {
                let schema_id = cursor.read_u32::<LittleEndian>()?;
                let blob_len = cursor.read_u16::<LittleEndian>()? as usize;
                ensure_remaining(payload, cursor.position(), blob_len)?;
                let mut blob = vec![0u8; blob_len];
                cursor.read_exact(&mut blob)?;
                ManifestRecordPayload::CustomEvent { schema_id, blob }
            }
            RecordType::Tier1Snapshot => {
                let len = cursor.read_u32::<LittleEndian>()? as usize;
                ensure_remaining(payload, cursor.position(), len)?;
                let mut json = vec![0u8; len];
                cursor.read_exact(&mut json)?;
                ManifestRecordPayload::Tier1Snapshot { json }
            }
        };

        ensure_consumed(payload, cursor.position())?;
        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct ManifestRecord {
    pub segment_id: SegmentId,
    pub logical_offset: u64,
    pub timestamp_ms: u64,
    pub payload: ManifestRecordPayload,
}

impl ManifestRecord {
    pub fn new(
        segment_id: SegmentId,
        logical_offset: u64,
        timestamp_ms: u64,
        payload: ManifestRecordPayload,
    ) -> Self {
        Self {
            segment_id,
            logical_offset,
            timestamp_ms,
            payload,
        }
    }

    pub fn encode(&self) -> AofResult<([u8; RECORD_HEADER_LEN], Vec<u8>)> {
        let payload_bytes = self.payload.encode()?;
        let payload_len = payload_bytes.len();
        if payload_len > u32::MAX as usize {
            return Err(AofError::invalid_config("manifest payload too large"));
        }
        let payload_crc64 = crc64(&payload_bytes);
        let header = ManifestRecordHeader {
            record_type: self.payload.record_type(),
            flags: 0,
            payload_len: payload_len as u32,
            segment_id: self.segment_id,
            logical_offset: self.logical_offset,
            timestamp_ms: self.timestamp_ms,
            payload_crc64,
        };
        Ok((header.encode(), payload_bytes))
    }
}

#[derive(Debug, Clone)]
pub struct ManifestRecordHeader {
    pub record_type: RecordType,
    pub flags: u16,
    pub payload_len: u32,
    pub segment_id: SegmentId,
    pub logical_offset: u64,
    pub timestamp_ms: u64,
    pub payload_crc64: u64,
}

impl ManifestRecordHeader {
    pub fn encode(&self) -> [u8; RECORD_HEADER_LEN] {
        let mut buf = [0u8; RECORD_HEADER_LEN];
        LittleEndian::write_u16(&mut buf[0..2], self.record_type as u16);
        LittleEndian::write_u16(&mut buf[2..4], self.flags);
        LittleEndian::write_u32(&mut buf[4..8], self.payload_len);
        LittleEndian::write_u64(&mut buf[8..16], self.segment_id.as_u64());
        LittleEndian::write_u64(&mut buf[16..24], self.logical_offset);
        LittleEndian::write_u64(&mut buf[24..32], self.timestamp_ms);
        LittleEndian::write_u64(&mut buf[32..40], self.payload_crc64);
        buf
    }

    pub fn decode(bytes: &[u8]) -> AofResult<Self> {
        if bytes.len() < RECORD_HEADER_LEN {
            return Err(AofError::Corruption(
                "manifest record header truncated".into(),
            ));
        }
        let record_type = RecordType::try_from(LittleEndian::read_u16(&bytes[0..2]))?;
        let flags = LittleEndian::read_u16(&bytes[2..4]);
        let payload_len = LittleEndian::read_u32(&bytes[4..8]);
        let segment_id = SegmentId::new(LittleEndian::read_u64(&bytes[8..16]));
        let logical_offset = LittleEndian::read_u64(&bytes[16..24]);
        let timestamp_ms = LittleEndian::read_u64(&bytes[24..32]);
        let payload_crc64 = LittleEndian::read_u64(&bytes[32..40]);

        Ok(Self {
            record_type,
            flags,
            payload_len,
            segment_id,
            logical_offset,
            timestamp_ms,
            payload_crc64,
        })
    }
}

fn ensure_long_len(len: usize, label: &str) -> AofResult<()> {
    if len > u32::MAX as usize {
        return Err(AofError::invalid_config(format!(
            "{label} length exceeds u32::MAX ({len})"
        )));
    }
    Ok(())
}

fn ensure_short_len(len: usize, label: &str) -> AofResult<()> {
    if len > u16::MAX as usize {
        return Err(AofError::invalid_config(format!(
            "{label} length exceeds u16::MAX ({len})"
        )));
    }
    Ok(())
}

fn write_short_blob(buf: &mut Vec<u8>, bytes: &[u8]) -> AofResult<()> {
    ensure_short_len(bytes.len(), "blob")?;
    buf.write_u16::<LittleEndian>(bytes.len() as u16)?;
    buf.extend_from_slice(bytes);
    Ok(())
}

fn read_short_string(cursor: &mut Cursor<&[u8]>, payload: &[u8]) -> AofResult<String> {
    let len = cursor.read_u16::<LittleEndian>()? as usize;
    ensure_remaining(payload, cursor.position(), len)?;
    let mut buf = vec![0u8; len];
    cursor.read_exact(&mut buf)?;
    String::from_utf8(buf)
        .map_err(|err| AofError::Corruption(format!("manifest string utf8 error: {err}")))
}

fn ensure_remaining(payload: &[u8], consumed: u64, required: usize) -> AofResult<()> {
    let consumed = consumed as usize;
    if payload.len().saturating_sub(consumed) < required {
        return Err(AofError::Corruption("manifest payload truncated".into()));
    }
    Ok(())
}

fn ensure_consumed(payload: &[u8], consumed: u64) -> AofResult<()> {
    if consumed as usize != payload.len() {
        return Err(AofError::Corruption(
            "manifest payload has trailing bytes".into(),
        ));
    }
    Ok(())
}

