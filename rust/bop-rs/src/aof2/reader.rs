use std::{collections::VecDeque, fmt, sync::Arc};

use crc64fast_nvme::Digest;
use tokio::{pin, select, sync::watch};

use super::{
    Aof, ResidentSegment, SegmentStatusSnapshot, TailEvent, TailState,
    config::{RecordId, SegmentId},
    error::{AofError, AofResult, BackpressureKind},
    segment::{SEGMENT_HEADER_SIZE, Segment},
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
    resident: ResidentSegment,
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

pub struct SegmentCursor {
    reader: SegmentReader,
    next_offset: u32,
}

#[derive(Debug)]
pub struct TailFollower {
    rx: watch::Receiver<TailState>,
    last_version: u64,
}

pub struct SealedSegmentStream<'a> {
    aof: &'a Aof,
    follower: TailFollower,
    sealed_queue: VecDeque<SegmentId>,
    resume_from: Option<RecordId>,
}

pub struct ActiveSegmentCursor {
    segment: Arc<Segment>,
    next_offset: u32,
    durable_end: u32,
}

impl fmt::Debug for ActiveSegmentCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActiveSegmentCursor")
            .field("segment_id", &self.segment.id())
            .field("next_offset", &self.next_offset)
            .field("durable_end", &self.durable_end)
            .finish()
    }
}

impl ActiveSegmentCursor {
    pub fn new(segment: Arc<Segment>, start_offset: u32, durable_end: u32) -> Self {
        Self {
            segment,
            next_offset: start_offset,
            durable_end,
        }
    }

    pub fn segment_id(&self) -> SegmentId {
        self.segment.id()
    }

    pub fn position(&self) -> u32 {
        self.next_offset
    }

    pub fn durable_end(&self) -> u32 {
        self.durable_end
    }

    pub fn segment(&self) -> &Arc<Segment> {
        &self.segment
    }

    pub fn is_finished(&self) -> bool {
        self.next_offset >= self.durable_end
    }

    pub fn next(&mut self) -> AofResult<Option<SegmentRecord<'_>>> {
        if self.is_finished() {
            return Ok(None);
        }

        let offset = self.next_offset;
        let slice = self.segment.read_record_slice(offset)?;
        let end = offset
            .checked_add(slice.total_len)
            .ok_or_else(|| AofError::CorruptedRecord("record length overflow".to_string()))?;
        if end > self.durable_end {
            return Err(AofError::CorruptedRecord(
                "record extends beyond durable frontier".to_string(),
            ));
        }

        let payload = slice.payload;
        if payload.len() != slice.header.length as usize {
            return Err(AofError::CorruptedRecord(
                "payload length mismatch".to_string(),
            ));
        }

        let computed = checksum32(payload);
        if computed != slice.header.checksum {
            return Err(AofError::CorruptedRecord(
                "record checksum mismatch".to_string(),
            ));
        }

        let bounds = RecordBounds::new(offset, end)?;
        let record_id = RecordId::from_parts(self.segment.id().as_u32(), offset);
        self.next_offset = end;

        Ok(Some(SegmentRecord {
            record_id,
            bounds,
            timestamp: slice.header.timestamp,
            payload,
        }))
    }
}

pub enum StreamSegment {
    Sealed(SegmentCursor),
    Active(ActiveSegmentCursor),
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

impl<'a> SealedSegmentStream<'a> {
    /// Creates a stream that replays sealed segments in their logical order.
    pub fn new(aof: &'a Aof) -> AofResult<Self> {
        Self::with_start_from(aof, None)
    }

    /// Creates a stream that resumes from the supplied record identifier.
    pub fn with_start_from(aof: &'a Aof, start_from: Option<RecordId>) -> AofResult<Self> {
        let follower = aof.tail_follower();
        let mut snapshots = aof.segment_snapshot();
        snapshots.sort_by_key(|snapshot| snapshot.segment_id.as_u64());

        let lower_bound = start_from.map(|rid| rid.segment_index() as u64);
        let mut sealed_queue = VecDeque::new();
        for snapshot in snapshots.into_iter() {
            if !snapshot.sealed {
                continue;
            }
            if let Some(bound) = lower_bound {
                if snapshot.segment_id.as_u64() < bound {
                    continue;
                }
            }
            sealed_queue.push_back(snapshot.segment_id);
        }

        if let Some(record_id) = start_from {
            let segment_id = SegmentId::new(record_id.segment_index() as u64);
            if sealed_queue
                .front()
                .map(|candidate| candidate.as_u64() == segment_id.as_u64())
                .unwrap_or(false)
            {
                let reader = aof.open_reader(segment_id)?;
                reader.record_bounds(record_id)?;
            }

            let segment = aof
                .segment_by_id(segment_id)
                .ok_or(AofError::RecordNotFound(record_id))?;
            segment.record_end_offset(record_id.segment_offset())?;
        }

        Ok(Self {
            aof,
            follower,
            sealed_queue,
            resume_from: start_from,
        })
    }

    /// Returns the next logical segment in the stream.
    pub async fn next_segment(&mut self) -> AofResult<Option<StreamSegment>> {
        loop {
            match self.pop_sealed_cursor() {
                Ok(Some(cursor)) => return Ok(Some(StreamSegment::Sealed(cursor))),
                Ok(None) => {}
                Err(AofError::WouldBlock(BackpressureKind::Hydration)) => {
                    let segment_id = match self.sealed_queue.front().copied() {
                        Some(id) => id,
                        None => continue,
                    };
                    let reader = self.aof.open_reader_async(segment_id).await?;
                    self.sealed_queue.pop_front();
                    let mut cursor = SegmentCursor::new(reader);
                    self.align_cursor(segment_id, &mut cursor)?;
                    if cursor.is_finished() {
                        continue;
                    }
                    return Ok(Some(StreamSegment::Sealed(cursor)));
                }
                Err(err) => return Err(err),
            }

            if let Some(segment) = self.current_active_segment() {
                let start_offset = self.active_start_offset(&segment)?;
                let durable = segment.durable_size();

                if durable > start_offset {
                    if let Some(cursor) =
                        self.collect_active_cursor(&segment, start_offset, durable)?
                    {
                        return Ok(Some(StreamSegment::Active(cursor)));
                    }
                }

                let target = start_offset.saturating_add(1);
                let flush_state = segment.flush_state();
                let flush_wait = flush_state.wait_for(target);
                pin!(flush_wait);
                select! {
                    _ = &mut flush_wait => {}
                    update = self.follower.next() => {
                        match update {
                            Some(state) => self.record_tail_update(state),
                            None => return Ok(None),
                        }
                    }
                }
                continue;
            }

            match self.follower.next().await {
                Some(state) => self.record_tail_update(state),
                None => return Ok(None),
            }
        }
    }

    fn pop_sealed_cursor(&mut self) -> AofResult<Option<SegmentCursor>> {
        while let Some(&segment_id) = self.sealed_queue.front() {
            let reader = match self.aof.open_reader(segment_id) {
                Ok(reader) => reader,
                Err(AofError::WouldBlock(BackpressureKind::Hydration)) => {
                    return Err(AofError::would_block(BackpressureKind::Hydration));
                }
                Err(err) => return Err(err),
            };
            self.sealed_queue.pop_front();
            let mut cursor = SegmentCursor::new(reader);
            self.align_cursor(segment_id, &mut cursor)?;
            if cursor.is_finished() {
                continue;
            }
            return Ok(Some(cursor));
        }
        Ok(None)
    }

    fn align_cursor(&mut self, segment_id: SegmentId, cursor: &mut SegmentCursor) -> AofResult<()> {
        if let Some(resume) = self.resume_from {
            if resume.segment_index() == segment_id.as_u32() {
                cursor.seek_after(resume)?;
                self.resume_from = None;
            }
        }
        Ok(())
    }

    fn current_active_segment(&self) -> Option<Arc<Segment>> {
        self.latest_active_snapshot()
            .and_then(|snapshot| self.aof.segment_by_id(snapshot.segment_id))
    }

    fn active_start_offset(&mut self, segment: &Arc<Segment>) -> AofResult<u32> {
        let mut start_offset = SEGMENT_HEADER_SIZE;
        if let Some(resume) = self.resume_from {
            if resume.segment_index() == segment.id().as_u32() {
                start_offset = segment.record_end_offset(resume.segment_offset())?;
            }
        }
        Ok(start_offset)
    }

    fn record_tail_update(&mut self, state: TailState) {
        self.enqueue_sealed(state.tail);
        for pending in state.pending {
            self.enqueue_sealed(Some(pending));
        }
        if let TailEvent::Sealed(segment_id) = state.last_event {
            self.push_sealed(segment_id);
        }
    }

    fn enqueue_sealed(&mut self, snapshot: Option<SegmentStatusSnapshot>) {
        if let Some(snapshot) = snapshot {
            if snapshot.sealed {
                self.push_sealed(snapshot.segment_id);
            }
        }
    }

    fn push_sealed(&mut self, segment_id: SegmentId) {
        if self
            .sealed_queue
            .iter()
            .any(|candidate| *candidate == segment_id)
        {
            return;
        }
        self.sealed_queue.push_back(segment_id);
    }

    fn collect_active_cursor(
        &mut self,
        segment: &Arc<Segment>,
        start_offset: u32,
        durable: u32,
    ) -> AofResult<Option<ActiveSegmentCursor>> {
        let start = start_offset.max(SEGMENT_HEADER_SIZE);
        if start >= durable {
            return Ok(None);
        }

        let mut cursor = start;
        let mut last_record = None;
        let segment_id = segment.id();

        while cursor < durable {
            let end = segment.record_end_offset(cursor)?;
            if end > durable {
                break;
            }
            last_record = Some(RecordId::from_parts(segment_id.as_u32(), cursor));
            cursor = end;
        }

        let Some(last) = last_record else {
            return Ok(None);
        };

        self.resume_from = Some(last);

        Ok(Some(ActiveSegmentCursor::new(
            segment.clone(),
            start,
            cursor,
        )))
    }

    fn latest_active_snapshot(&self) -> Option<SegmentStatusSnapshot> {
        self.aof
            .segment_snapshot()
            .into_iter()
            .filter(|snapshot| !snapshot.sealed)
            .max_by_key(|snapshot| snapshot.segment_id.as_u64())
    }
}

impl SegmentReader {
    /// Creates a reader over a sealed segment.
    pub fn new(resident: ResidentSegment) -> AofResult<Self> {
        if !resident.is_sealed() {
            return Err(AofError::InvalidState(
                "segment must be sealed before it can be read".to_string(),
            ));
        }
        let logical_end = resident.current_size();
        Ok(Self {
            resident,
            logical_end,
        })
    }

    /// Returns the identifier of the underlying segment.
    pub fn segment_id(&self) -> SegmentId {
        self.resident.id()
    }

    /// Returns the logical size of the sealed segment.
    pub fn logical_size(&self) -> u32 {
        self.logical_end
    }

    /// Returns true if the provided record id belongs to this segment.
    pub fn contains(&self, record_id: RecordId) -> bool {
        self.resident.segment_offset_for(record_id).is_some()
    }

    /// Resolves the byte range occupied by the given record id.
    pub fn record_bounds(&self, record_id: RecordId) -> AofResult<RecordBounds> {
        let offset = self
            .resident
            .segment_offset_for(record_id)
            .ok_or(AofError::RecordNotFound(record_id))?;

        let slice = self.resident.read_record_slice(offset)?;
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
            .resident
            .segment_offset_for(record_id)
            .ok_or(AofError::RecordNotFound(record_id))?;

        let slice = self.resident.read_record_slice(offset)?;
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
        self.resident.segment()
    }
}

impl SegmentCursor {
    /// Creates a cursor positioned at the first record in the segment.
    pub fn new(reader: SegmentReader) -> Self {
        Self {
            next_offset: SEGMENT_HEADER_SIZE,
            reader,
        }
    }

    /// Returns the identifier of the underlying segment.
    pub fn segment_id(&self) -> SegmentId {
        self.reader.segment_id()
    }

    /// Returns the current cursor offset within the segment.
    pub fn position(&self) -> u32 {
        self.next_offset
    }

    /// Returns the underlying reader reference for advanced consumers.
    pub fn reader(&self) -> &SegmentReader {
        &self.reader
    }

    /// Consumes the cursor and returns the inner `SegmentReader`.
    pub fn into_reader(self) -> SegmentReader {
        self.reader
    }

    /// Seeks to the beginning of the specified record within the segment.
    pub fn seek(&mut self, record_id: RecordId) -> AofResult<()> {
        self.ensure_same_segment(record_id)?;
        let bounds = self.reader.record_bounds(record_id)?;
        self.next_offset = bounds.start();
        Ok(())
    }

    /// Seeks to the byte immediately following the specified record.
    pub fn seek_after(&mut self, record_id: RecordId) -> AofResult<()> {
        self.ensure_same_segment(record_id)?;
        let bounds = self.reader.record_bounds(record_id)?;
        self.next_offset = bounds.end();
        Ok(())
    }

    /// Returns true when the cursor has reached the end of the segment.
    pub fn is_finished(&self) -> bool {
        self.next_offset >= self.reader.logical_size()
    }

    /// Returns the next record in the segment, advancing the cursor.
    pub fn next(&mut self) -> AofResult<Option<SegmentRecord<'_>>> {
        if self.is_finished() {
            return Ok(None);
        }

        let segment_index = self.reader.segment_id().as_u32();
        let record_id = RecordId::from_parts(segment_index, self.next_offset);
        let record = self.reader.read_record(record_id)?;
        self.next_offset = record.bounds().end();
        Ok(Some(record))
    }

    fn ensure_same_segment(&self, record_id: RecordId) -> AofResult<()> {
        if record_id.segment_index() != self.reader.segment_id().as_u32() {
            return Err(AofError::RecordNotFound(record_id));
        }
        Ok(())
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
    use crate::aof2::config::{AofConfig, SegmentId};
    use crate::aof2::error::{AofError, BackpressureKind};
    use crate::aof2::{Aof, AofManager, AofManagerConfig, AofManagerHandle, TailEvent};
    use chrono::Utc;
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn build_manager(config: AofManagerConfig) -> (AofManager, Arc<AofManagerHandle>) {
        let manager = AofManager::with_config(config).expect("create manager");
        let handle = manager.handle();
        (manager, handle)
    }

    #[test]
    fn rejects_unsealed_segments() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let cfg = AofConfig::default();
        let segment =
            Segment::create_active(SegmentId::new(7), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create");
        let segment = Arc::new(segment);

        let err = SegmentReader::new(ResidentSegment::new_for_tests(segment.clone()));
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

        let reader =
            SegmentReader::new(ResidentSegment::new_for_tests(segment.clone())).expect("reader");
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

        let reader =
            SegmentReader::new(ResidentSegment::new_for_tests(segment.clone())).expect("reader");
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

        let reader =
            SegmentReader::new(ResidentSegment::new_for_tests(segment.clone())).expect("reader");
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

    fn stream_test_config(root: &Path) -> AofConfig {
        let mut cfg = AofConfig::default();
        cfg.root_dir = root.join("aof");
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        cfg.flush.flush_watermark_bytes = 0;
        cfg.flush.flush_interval_ms = 0;
        cfg.flush.max_unflushed_bytes = u64::MAX;
        cfg
    }

    async fn seal_active_with_ack(aof: &Aof, manager: &AofManager) -> AofResult<Option<SegmentId>> {
        if !cfg!(feature = "tiered-store") {
            return aof.seal_active();
        }
        loop {
            match aof.seal_active() {
                Ok(result @ Some(_)) => return Ok(result),
                Ok(result @ None) => return Ok(result),
                Err(AofError::WouldBlock(BackpressureKind::Rollover)) => {
                    manager.tiered().poll().await;
                }
                Err(err) => return Err(err),
            }
        }
    }

    #[test]
    fn segment_cursor_iterates_records_in_order() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        let segment =
            Segment::create_active(SegmentId::new(21), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create segment");
        let segment = Arc::new(segment);

        let payloads = [&b"one"[..], &b"two"[..], &b"three"[..]];
        for &payload in payloads.iter() {
            let append = segment.append_record(payload, 11).expect("append");
            segment.mark_durable(append.logical_size);
        }
        let sealed_at = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        segment.seal(sealed_at).expect("seal");

        let reader =
            SegmentReader::new(ResidentSegment::new_for_tests(segment.clone())).expect("reader");
        let mut cursor = SegmentCursor::new(reader);

        let mut observed = Vec::new();
        while let Some(record) = cursor.next().expect("next") {
            observed.push(record.payload().to_vec());
        }

        let expected: Vec<Vec<u8>> = payloads.iter().map(|payload| (*payload).to_vec()).collect();
        assert_eq!(observed, expected);
    }

    #[test]
    fn segment_cursor_seek_after_skips_consumed_records() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("segment.seg");
        let mut cfg = AofConfig::default();
        cfg.segment_min_bytes = 4096;
        cfg.segment_max_bytes = 4096;
        cfg.segment_target_bytes = 4096;
        let segment =
            Segment::create_active(SegmentId::new(23), 0, 0, 0, cfg.segment_max_bytes, &path)
                .expect("create segment");
        let segment = Arc::new(segment);

        let first = segment.append_record(b"first", 0).expect("append first");
        segment.mark_durable(first.logical_size);
        let second = segment.append_record(b"second", 0).expect("append second");
        segment.mark_durable(second.logical_size);

        let sealed_at = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp() * 1_000_000_000);
        segment.seal(sealed_at).expect("seal");

        let reader =
            SegmentReader::new(ResidentSegment::new_for_tests(segment.clone())).expect("reader");
        let mut cursor = SegmentCursor::new(reader);

        let first_id = RecordId::from_parts(segment.id().as_u32(), first.segment_offset);
        cursor.seek_after(first_id).expect("seek after");

        let record = cursor.next().expect("next").expect("record");
        assert_eq!(record.payload(), b"second");
        assert!(cursor.next().expect("end").is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sealed_segment_stream_replays_existing_segments() {
        let tmp = TempDir::new().expect("tempdir");
        let cfg = stream_test_config(tmp.path());
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");

        let record_id = aof.append_record(b"sealed-stream").expect("append");
        seal_active_with_ack(&aof, &manager)
            .await
            .expect("seal active");

        let mut stream = SealedSegmentStream::new(&aof).expect("stream");
        let segment = stream
            .next_segment()
            .await
            .expect("segment result")
            .expect("segment");
        let mut cursor = match segment {
            StreamSegment::Sealed(cursor) => cursor,
            StreamSegment::Active(_) => panic!("expected sealed segment"),
        };
        let record = cursor.next().expect("next").expect("record");
        assert_eq!(record.record_id(), record_id);
        assert!(cursor.next().expect("end").is_none());

        drop(aof);
        drop(manager);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sealed_segment_stream_receives_future_segments() {
        let tmp = TempDir::new().expect("tempdir");
        let cfg = stream_test_config(tmp.path());
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");

        let mut stream = SealedSegmentStream::new(&aof).expect("stream");

        let record_id = aof.append_record(b"future").expect("append");
        seal_active_with_ack(&aof, &manager)
            .await
            .expect("seal active");

        let segment = stream
            .next_segment()
            .await
            .expect("segment result")
            .expect("segment");
        let mut cursor = match segment {
            StreamSegment::Sealed(cursor) => cursor,
            StreamSegment::Active(_) => panic!("expected sealed segment"),
        };
        let record = cursor.next().expect("next").expect("record");
        assert_eq!(record.record_id(), record_id);
        assert!(cursor.next().expect("end").is_none());

        drop(aof);
        drop(manager);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sealed_segment_stream_resumes_after_record() {
        let tmp = TempDir::new().expect("tempdir");
        let cfg = stream_test_config(tmp.path());
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");

        let first = aof.append_record(b"first").expect("append first");
        let second = aof.append_record(b"second").expect("append second");
        seal_active_with_ack(&aof, &manager)
            .await
            .expect("seal active");

        let mut stream = SealedSegmentStream::with_start_from(&aof, Some(first)).expect("stream");
        let segment = stream
            .next_segment()
            .await
            .expect("segment result")
            .expect("segment");
        let mut cursor = match segment {
            StreamSegment::Sealed(cursor) => cursor,
            StreamSegment::Active(_) => panic!("expected sealed segment"),
        };

        let record = cursor.next().expect("next").expect("record");
        assert_eq!(record.record_id(), second);
        assert!(cursor.next().expect("end").is_none());

        drop(aof);
        drop(manager);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn sealed_segment_stream_replays_multiple_sealed_segments() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = stream_test_config(tmp.path());
        cfg.segment_min_bytes = 512;
        cfg.segment_max_bytes = 512;
        cfg.segment_target_bytes = 512;
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());

        let aof = Aof::new(handle, cfg).expect("aof");

        let first = aof.append_record(b"sealed-0").expect("append first");
        seal_active_with_ack(&aof, &manager)
            .await
            .expect("seal first");

        let second = aof.append_record(b"sealed-1").expect("append second");
        seal_active_with_ack(&aof, &manager)
            .await
            .expect("seal second");

        let mut stream = SealedSegmentStream::new(&aof).expect("stream");

        let first_segment = stream
            .next_segment()
            .await
            .expect("segment result")
            .expect("segment");
        let mut first_cursor = match first_segment {
            StreamSegment::Sealed(cursor) => cursor,
            StreamSegment::Active(_) => panic!("expected sealed segment"),
        };
        let record = first_cursor.next().expect("next").expect("record");
        assert_eq!(record.record_id(), first);
        assert_eq!(record.payload(), b"sealed-0");
        assert!(first_cursor.next().expect("end").is_none());

        let second_segment = stream
            .next_segment()
            .await
            .expect("segment result")
            .expect("segment");
        let mut second_cursor = match second_segment {
            StreamSegment::Sealed(cursor) => cursor,
            StreamSegment::Active(_) => panic!("expected sealed segment"),
        };
        let record = second_cursor.next().expect("next").expect("record");
        assert_eq!(record.record_id(), second);
        assert_eq!(record.payload(), b"sealed-1");
        assert!(second_cursor.next().expect("end").is_none());

        drop(aof);
        drop(manager);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn open_reader_discovers_sealed_segments_across_rollover() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = stream_test_config(tmp.path());
        cfg.segment_min_bytes = 512;
        cfg.segment_max_bytes = 512;
        cfg.segment_target_bytes = 512;
        let (manager, handle) = build_manager(AofManagerConfig::for_tests());

        let aof = Aof::new(handle, cfg).expect("aof");

        let first = aof.append_record(b"sealed-0").expect("append first");
        seal_active_with_ack(&aof, &manager)
            .await
            .expect("seal first");

        let second = aof.append_record(b"sealed-1").expect("append second");
        seal_active_with_ack(&aof, &manager)
            .await
            .expect("seal second");

        let first_reader = aof
            .open_reader(SegmentId::new(first.segment_index() as u64))
            .expect("first reader");
        let first_record = first_reader.read_record(first).expect("first record");
        assert_eq!(first_record.payload(), b"sealed-0");

        let second_reader = aof
            .open_reader(SegmentId::new(second.segment_index() as u64))
            .expect("second reader");
        let second_record = second_reader.read_record(second).expect("second record");
        assert_eq!(second_record.payload(), b"sealed-1");

        drop(aof);
        drop(manager);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sealed_segment_stream_bridges_active_tail() {
        let tmp = TempDir::new().expect("tempdir");
        let cfg = stream_test_config(tmp.path());
        let (_manager, handle) = build_manager(AofManagerConfig::for_tests());
        let aof = Aof::new(handle, cfg).expect("aof");

        aof.flush.shutdown_worker_for_tests();

        let first = aof.append_record(b"active-0").expect("append first");
        let segment_id = SegmentId::new(first.segment_index() as u64);
        let segment = aof.segment_by_id(segment_id).expect("segment");
        segment.mark_durable(segment.current_size());

        let mut stream = SealedSegmentStream::new(&aof).expect("stream");
        let mut cursor = match stream
            .next_segment()
            .await
            .expect("segment result")
            .expect("segment")
        {
            StreamSegment::Active(cursor) => cursor,
            StreamSegment::Sealed(_) => panic!("expected active cursor"),
        };

        assert_eq!(cursor.segment_id(), segment_id);
        let record = cursor.next().expect("next").expect("record");
        assert_eq!(record.record_id(), first);
        assert_eq!(record.payload(), b"active-0");
        assert!(cursor.next().expect("end").is_none());

        let second = aof.append_record(b"active-1").expect("append second");
        segment.mark_durable(segment.current_size());

        let mut cursor = match stream
            .next_segment()
            .await
            .expect("segment result")
            .expect("segment")
        {
            StreamSegment::Active(cursor) => cursor,
            StreamSegment::Sealed(_) => panic!("expected active cursor"),
        };

        let record = cursor.next().expect("next").expect("record");
        assert_eq!(record.record_id(), second);
        assert_eq!(record.payload(), b"active-1");
        assert!(cursor.next().expect("end").is_none());

        drop(aof);
    }
}
