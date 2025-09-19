use std::sync::Arc;

use crc64fast_nvme::Digest;
use tokio::sync::watch;

use super::{
    TailEvent, TailState,
    config::{RecordId, SegmentId},
    error::{AofError, AofResult},
    segment::Segment,
};

/// Logical bounds for a record within a segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordBounds {
    start: u32,
    end: u32,
}

impl RecordBounds {
    /// Create a new set of bounds where `start < end`.
    pub fn new(start: u32, end: u32) -> AofResult<Self> {
        if end <= start {
            return Err(AofError::CorruptedRecord(
                "record bounds are empty".to_string(),
            ));
        }
        Ok(Self { start, end })
    }

    /// Starting byte offset for the record header inside the segment.
    pub const fn start(&self) -> u32 {
        self.start
    }

    /// Exclusive end offset for the record payload inside the segment.
    pub const fn end(&self) -> u32 {
        self.end
    }

    /// Total bytes occupied by the record header plus payload.
    pub const fn len(&self) -> u32 {
        self.end - self.start
    }
}

/// Read-only view over a sealed segment that can resolve record offsets.
pub struct SegmentReader {
    segment: Arc<Segment>,
    logical_end: u32,
}

#[derive(Debug)]
pub struct SegmentRecord<'a> {
    record_id: RecordId,
    bounds: RecordBounds,
    timestamp: u64,
    payload: &'a [u8],
}

impl<'a> SegmentRecord<'a> {
    pub fn record_id(&self) -> RecordId {
        self.record_id
    }

    pub fn bounds(&self) -> RecordBounds {
        self.bounds
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }
}

#[derive(Debug)]
pub struct TailFollower {
    rx: watch::Receiver<TailState>,
    last_version: u64,
}

impl TailFollower {
    pub fn new(rx: watch::Receiver<TailState>) -> Self {
        let last_version = rx.borrow().version;
        Self { rx, last_version }
    }

    pub fn current(&self) -> TailState {
        self.rx.borrow().clone()
    }

    pub async fn next(&mut self) -> Option<TailState> {
        loop {
            let snapshot = self.rx.borrow().clone();
            if snapshot.version != self.last_version {
                self.last_version = snapshot.version;
                return Some(snapshot);
            }
            if self.rx.changed().await.is_err() {
                let snapshot = self.rx.borrow().clone();
                if snapshot.version != self.last_version {
                    self.last_version = snapshot.version;
                    return Some(snapshot);
                }
                return None;
            }
        }
    }

    pub async fn next_sealed_segment(&mut self) -> Option<SegmentId> {
        while let Some(state) = self.next().await {
            if let TailEvent::Sealed(segment_id) = state.last_event {
                return Some(segment_id);
            }
        }
        None
    }
}

impl SegmentReader {
    /// Creates a reader over a sealed segment.
    pub fn new(segment: Arc<Segment>) -> AofResult<Self> {
        if !segment.is_sealed() {
            return Err(AofError::InvalidState(
                "segment must be sealed before it can be read".to_string(),
            ));
        }
        let logical_end = segment.current_size();
        Ok(Self {
            segment,
            logical_end,
        })
    }

    /// Returns the identifier of the underlying segment.
    pub fn segment_id(&self) -> SegmentId {
        self.segment.id()
    }

    /// Returns the logical size of the sealed segment.
    pub fn logical_size(&self) -> u32 {
        self.logical_end
    }

    /// Returns true if the provided record id belongs to this segment.
    pub fn contains(&self, record_id: RecordId) -> bool {
        self.segment.segment_offset_for(record_id).is_some()
    }

    /// Resolves the byte range occupied by the given record id.
    pub fn record_bounds(&self, record_id: RecordId) -> AofResult<RecordBounds> {
        let offset = self
            .segment
            .segment_offset_for(record_id)
            .ok_or(AofError::RecordNotFound(record_id))?;

        let slice = self.segment.read_record_slice(offset)?;
        let total_len = slice.total_len;
        let end = offset
            .checked_add(total_len)
            .ok_or_else(|| AofError::CorruptedRecord("record length overflow".to_string()))?;
        if end > self.logical_end {
            return Err(AofError::CorruptedRecord(
                "record extends past sealed segment boundary".to_string(),
            ));
        }

        RecordBounds::new(offset, end)
    }

    /// Returns the record payload and metadata, verifying checksum integrity.
    pub fn read_record<'a>(&'a self, record_id: RecordId) -> AofResult<SegmentRecord<'a>> {
        let offset = self
            .segment
            .segment_offset_for(record_id)
            .ok_or(AofError::RecordNotFound(record_id))?;

        let slice = self.segment.read_record_slice(offset)?;
        let total_len = slice.total_len;
        let end = offset
            .checked_add(total_len)
            .ok_or_else(|| AofError::CorruptedRecord("record length overflow".to_string()))?;
        if end > self.logical_end {
            return Err(AofError::CorruptedRecord(
                "record extends past sealed segment boundary".to_string(),
            ));
        }

        let header = slice.header;
        let payload = slice.payload;
        if payload.len() != header.length as usize {
            return Err(AofError::CorruptedRecord(
                "payload length mismatch".to_string(),
            ));
        }

        let computed = checksum32(payload);
        if computed != header.checksum {
            return Err(AofError::CorruptedRecord(
                "record checksum mismatch".to_string(),
            ));
        }

        let bounds = RecordBounds::new(offset, end)?;
        Ok(SegmentRecord {
            record_id,
            bounds,
            timestamp: header.timestamp,
            payload,
        })
    }

    /// Returns the underlying segment handle for advanced consumers.
    pub fn segment(&self) -> &Arc<Segment> {
        &self.segment
    }
}

fn checksum32(payload: &[u8]) -> u32 {
    let mut digest = Digest::new();
    digest.write(payload);
    fold_crc64(digest.sum64())
}

fn fold_crc64(value: u64) -> u32 {
    let upper = (value >> 32) as u32;
    let lower = value as u32;
    upper ^ lower
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aof2::TailEvent;
    use crate::aof2::config::{AofConfig, SegmentId};
    use chrono::Utc;
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn rejects_unsealed_segments() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let cfg = AofConfig::default();
        let segment =
            Segment::create_active(SegmentId::new(7), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create");
        let segment = Arc::new(segment);

        let err = SegmentReader::new(segment.clone());
        match err {
            Err(AofError::InvalidState(msg)) => assert!(msg.contains("sealed")),
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("segment should require sealing"),
        }
    }

    #[test]
    fn resolves_record_bounds_for_sealed_segment() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 2048;
        cfg.segment_max_bytes = 2048;
        cfg.segment_target_bytes = 2048;
        let segment =
            Segment::create_active(SegmentId::new(3), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create");
        let segment = Arc::new(segment);

        let append = segment.append_record(b"hello world", 123).expect("append");
        let _advanced = segment.mark_durable(append.logical_size);
        let sealed_at = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        segment.seal(sealed_at).expect("seal");

        let reader = SegmentReader::new(segment.clone()).expect("reader");
        let record_id = RecordId::from_parts(segment.id().as_u32(), append.segment_offset);
        assert!(reader.contains(record_id));

        let bounds = reader.record_bounds(record_id).expect("bounds");
        assert_eq!(bounds.start(), append.segment_offset);
        let expected_end = segment
            .record_end_offset(append.segment_offset)
            .expect("segment end");
        assert_eq!(bounds.end(), expected_end);
        assert_eq!(bounds.len(), expected_end - append.segment_offset);

        let other_record = RecordId::from_parts(segment.id().as_u32() + 1, append.segment_offset);
        assert!(!reader.contains(other_record));
        let err = reader
            .record_bounds(other_record)
            .expect_err("record missing");
        assert!(matches!(err, AofError::RecordNotFound(rid) if rid == other_record));
    }

    #[test]
    fn read_record_returns_payload_and_timestamp() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 2048;
        cfg.segment_max_bytes = 2048;
        cfg.segment_target_bytes = 2048;
        let segment =
            Segment::create_active(SegmentId::new(9), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create");
        let segment = Arc::new(segment);

        let payload = b"reader payload";
        let append = segment.append_record(payload, 987_654).expect("append");
        let _ = segment.mark_durable(append.logical_size);
        let sealed_at = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        segment.seal(sealed_at).expect("seal");

        let reader = SegmentReader::new(segment.clone()).expect("reader");
        let record_id = RecordId::from_parts(segment.id().as_u32(), append.segment_offset);
        let record = reader.read_record(record_id).expect("record");
        assert_eq!(record.record_id(), record_id);
        assert_eq!(record.bounds().start(), append.segment_offset);
        assert_eq!(record.payload(), payload);
        assert_eq!(record.timestamp(), 987_654);
    }

    #[test]
    fn read_record_detects_checksum_mismatch() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 2048;
        cfg.segment_max_bytes = 2048;
        cfg.segment_target_bytes = 2048;
        let segment =
            Segment::create_active(SegmentId::new(11), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create");
        let segment = Arc::new(segment);

        let append = segment.append_record(b"corrupt me", 42).expect("append");

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open for corruption");
        file.seek(SeekFrom::Start((append.segment_offset + 4) as u64))
            .expect("seek checksum");
        file.write_all(&0u32.to_le_bytes())
            .expect("overwrite checksum");
        file.flush().expect("flush corruption");
        file.sync_data().expect("sync corruption");

        let _ = segment.mark_durable(append.logical_size);
        let sealed_at = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        segment.seal(sealed_at).expect("seal");

        let reader = SegmentReader::new(segment.clone()).expect("reader");
        let record_id = RecordId::from_parts(segment.id().as_u32(), append.segment_offset);
        let err = reader
            .read_record(record_id)
            .expect_err("checksum mismatch");
        assert!(matches!(err, AofError::CorruptedRecord(msg) if msg.contains("checksum")));
    }

    #[tokio::test]
    async fn tail_follower_emits_changes() {
        let initial = TailState::default();
        let (tx, rx) = watch::channel(initial);
        let mut follower = TailFollower::new(rx);
        assert_eq!(follower.current().version, 0);

        let mut updated = follower.current();
        updated.version = updated.version.wrapping_add(1);
        updated.last_event = TailEvent::Activated(SegmentId::new(7));
        tx.send(updated).expect("send update");

        let change = follower.next().await.expect("change");
        assert_eq!(change.last_event, TailEvent::Activated(SegmentId::new(7)));
        assert_eq!(change.version, follower.current().version);
    }

    #[tokio::test]
    async fn tail_follower_returns_none_when_sender_dropped() {
        use tokio::time::{Duration, timeout};

        let initial = TailState::default();
        let (tx, rx) = watch::channel(initial);
        drop(tx);
        let mut follower = TailFollower::new(rx);
        let outcome = timeout(Duration::from_millis(200), follower.next())
            .await
            .expect("tail follower future timed out");
        assert!(outcome.is_none());
    }
}
