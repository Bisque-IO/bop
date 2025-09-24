use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use crate::error::{AofError, AofResult};

use super::chunk::{CHUNK_HEADER_LEN, ChunkHeader, crc64};
use super::record::{ManifestRecordHeader, ManifestRecordPayload, RECORD_HEADER_LEN};

/// Summary of a manifest chunk on disk.
///
/// Provides detailed information about chunk health,
/// integrity, and content for debugging and verification.
#[derive(Debug, Clone)]
pub struct ChunkSummary {
    /// Sequential chunk identifier
    pub chunk_index: u64,
    /// First record index in this chunk
    pub base_record: u64,
    /// Current tail position for appends
    pub tail_offset: usize,
    /// Length of committed (durable) data
    pub committed_len: usize,
    /// Number of records found in chunk
    pub record_count: usize,
    /// Stored CRC64 checksum
    pub chunk_crc64: u64,
    /// Computed CRC64 checksum for verification
    pub computed_crc64: u64,
    /// Number of records with CRC failures
    pub payload_crc_failures: usize,
    /// Bytes past committed boundary
    pub uncommitted_bytes: u64,
}

impl ChunkSummary {
    pub fn crc_ok(&self) -> bool {
        self.chunk_crc64 == self.computed_crc64 && self.payload_crc_failures == 0
    }
}

/// Aggregated inspection details for a manifest stream.
///
/// Contains summary information across all chunks in a stream
/// for comprehensive health assessment.
#[derive(Debug, Clone, Default)]
pub struct ManifestInspection {
    /// Stream identifier
    pub stream_id: u64,
    /// Summary of each chunk in the stream
    pub chunks: Vec<ChunkSummary>,
    /// All Tier 2 object keys referenced
    pub tier2_keys: Vec<String>,
    /// Total records across all chunks
    pub total_records: usize,
}

/// Convenience wrapper for inspecting manifest chunk files.
///
/// Provides debugging and verification tools for manifest streams,
/// useful in developer tooling, tests, and operational debugging.
///
/// ## Usage Pattern
///
/// ```ignore
/// let inspection = ManifestInspector::inspect_stream(base_dir, stream_id)?;
/// for chunk in &inspection.chunks {
///     if !chunk.crc_ok() {
///         eprintln!("Chunk {} has integrity issues", chunk.chunk_index);
///     }
/// }
/// ```
pub struct ManifestInspector;

impl ManifestInspector {
    pub fn inspect_stream<P: AsRef<Path>>(
        base_dir: P,
        stream_id: u64,
    ) -> AofResult<ManifestInspection> {
        let stream_dir = base_dir.as_ref().join(format!("{stream_id:020}"));
        if !stream_dir.exists() {
            return Ok(ManifestInspection {
                stream_id,
                ..ManifestInspection::default()
            });
        }

        let mut chunk_paths: Vec<_> = fs::read_dir(&stream_dir)
            .map_err(AofError::from)?
            .filter_map(|entry| {
                entry.ok().and_then(|entry| {
                    let path = entry.path();
                    if path.extension().and_then(|ext| ext.to_str()) == Some("mlog") {
                        Some(path)
                    } else {
                        None
                    }
                })
            })
            .collect();
        chunk_paths.sort();

        let mut inspection = ManifestInspection {
            stream_id,
            chunks: Vec::with_capacity(chunk_paths.len()),
            tier2_keys: Vec::new(),
            total_records: 0,
        };

        for path in chunk_paths {
            let (summary, tier2_keys, records) = inspect_chunk(&path)?;
            inspection.total_records += records;
            inspection.tier2_keys.extend(tier2_keys);
            inspection.chunks.push(summary);
        }

        Ok(inspection)
    }
}

fn inspect_chunk(path: &Path) -> AofResult<(ChunkSummary, Vec<String>, usize)> {
    let mut file = File::open(path).map_err(AofError::from)?;
    let mut header_bytes = vec![0u8; CHUNK_HEADER_LEN];
    file.read_exact(&mut header_bytes).map_err(AofError::from)?;
    let header = ChunkHeader::read_from(&header_bytes)?;

    let file_len = file.metadata().map_err(AofError::from)?.len();
    if file_len < header.committed_len as u64 {
        return Err(AofError::Corruption(format!(
            "manifest chunk {} smaller than committed_len",
            path.display()
        )));
    }

    let mut data = vec![0u8; header.committed_len.saturating_sub(CHUNK_HEADER_LEN)];
    file.read_exact(&mut data).map_err(AofError::from)?;

    let computed_crc = crc64(&data);

    let mut offset = 0usize;
    let mut record_count = 0usize;
    let mut payload_crc_failures = 0usize;
    let mut tier2_keys = Vec::new();

    while offset + RECORD_HEADER_LEN <= data.len() {
        let header_bytes = &data[offset..offset + RECORD_HEADER_LEN];
        let record_header = ManifestRecordHeader::decode(header_bytes)?;
        offset += RECORD_HEADER_LEN;

        let payload_len = record_header.payload_len as usize;
        if payload_len == 0 {
            break;
        }
        if offset + payload_len > data.len() {
            return Err(AofError::Corruption(format!(
                "manifest record payload truncated in chunk {}",
                path.display()
            )));
        }
        let payload_bytes = &data[offset..offset + payload_len];
        offset += payload_len;

        if record_header.payload_crc64 != 0 {
            let crc = crc64(payload_bytes);
            if crc != record_header.payload_crc64 {
                payload_crc_failures += 1;
                continue;
            }
        }

        let payload = ManifestRecordPayload::decode(record_header.record_type, payload_bytes)?;
        record_count += 1;
        collect_tier2_keys(&payload, &mut tier2_keys);
    }

    let summary = ChunkSummary {
        chunk_index: header.chunk_index,
        base_record: header.base_record,
        tail_offset: header.tail_offset,
        committed_len: header.committed_len,
        record_count,
        chunk_crc64: header.chunk_crc64,
        computed_crc64: computed_crc,
        payload_crc_failures,
        uncommitted_bytes: file_len.saturating_sub(header.committed_len as u64),
    };

    Ok((summary, tier2_keys, record_count))
}

fn collect_tier2_keys(payload: &ManifestRecordPayload, out: &mut Vec<String>) {
    match payload {
        ManifestRecordPayload::UploadStarted { key }
        | ManifestRecordPayload::UploadDone { key, .. }
        | ManifestRecordPayload::UploadFailed { key, .. }
        | ManifestRecordPayload::Tier2DeleteQueued { key }
        | ManifestRecordPayload::Tier2Deleted { key } => out.push(key.clone()),
        ManifestRecordPayload::CustomEvent { .. }
        | ManifestRecordPayload::CompressionStarted { .. }
        | ManifestRecordPayload::CompressionDone { .. }
        | ManifestRecordPayload::CompressionFailed { .. }
        | ManifestRecordPayload::SegmentOpened { .. }
        | ManifestRecordPayload::SealSegment { .. }
        | ManifestRecordPayload::HydrationStarted { .. }
        | ManifestRecordPayload::HydrationDone { .. }
        | ManifestRecordPayload::HydrationFailed { .. }
        | ManifestRecordPayload::LocalEvicted { .. }
        | ManifestRecordPayload::SnapshotMarker { .. }
        | ManifestRecordPayload::Tier1Snapshot { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SegmentId;
    use crate::manifest::{
        ManifestLogWriter, ManifestLogWriterConfig, ManifestRecord, ManifestRecordPayload,
    };

    #[test]
    fn inspect_stream_reports_chunk_details() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stream_id = 17;
        let mut cfg = ManifestLogWriterConfig::default();
        cfg.enabled = true;
        let mut writer = ManifestLogWriter::new(dir.path().to_path_buf(), stream_id, cfg)
            .expect("manifest writer");
        let segment = SegmentId::new(1);
        writer
            .append(&ManifestRecord::new(
                segment,
                0,
                1,
                ManifestRecordPayload::SegmentOpened {
                    base_offset: 0,
                    epoch: 1,
                },
            ))
            .expect("append open");
        writer.commit().expect("commit open");
        writer
            .append(&ManifestRecord::new(
                segment,
                1,
                2,
                ManifestRecordPayload::UploadDone {
                    key: "tier2/object/key".to_string(),
                    etag: Some("etag".to_string()),
                },
            ))
            .expect("append upload");
        writer.commit().expect("commit upload");
        drop(writer);

        let inspection =
            ManifestInspector::inspect_stream(dir.path(), stream_id).expect("inspect stream");
        assert_eq!(inspection.stream_id, stream_id);
        assert_eq!(inspection.total_records, 2);
        assert_eq!(inspection.tier2_keys, vec!["tier2/object/key".to_string()]);
        assert_eq!(inspection.chunks.len(), 1);
        let chunk = &inspection.chunks[0];
        assert_eq!(chunk.record_count, 2);
        assert!(chunk.crc_ok());
    }
}
