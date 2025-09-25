//! Segmented Raft log store backed by bop-aof.

use std::convert::TryInto;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use bop_aof::config::AofConfig;
use bop_aof::error::AofError;
use bop_aof::{Aof, AofManager, AofManagerConfig, RecordId};
use bop_storage::StoragePodHandle;
use crc32fast::Hasher;

use crate::buffer::Buffer;
use crate::error::{RaftError, RaftResult};
use crate::log_entry::{LogEntryRecord, LogEntryView};
use crate::segmented::coordinator::{CoordinatorError, SegmentedCoordinator};
use crate::segmented::manifest::PartitionManifest;
use crate::segmented::partition_store::PartitionLogStore;
use crate::segmented::types::{
    PartitionAofMetadata, PartitionId, PartitionLogIndex, PartitionWatermark,
    SEGMENTED_RECORD_VERSION, SegmentedRecordFlags, SegmentedRecordHeader, SegmentedWatermarks,
};
use crate::traits::LogStoreInterface;
use crate::types::{LogIndex, Term};

const LEGACY_HEADER_LEN: usize = 16;
const HEADER_RESERVED_BYTES: usize = 2;
const SEGMENTED_RECORD_HEADER_LEN: usize = 1 + 1 + HEADER_RESERVED_BYTES + 8 + 8 + 8 + 4 + 4;
const CHECKSUM_PRESENT_FLAG: u8 = 0b1000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionPointer {
    pub partition_id: PartitionId,
    pub partition_index: PartitionLogIndex,
}

impl PartitionPointer {
    pub const fn new(partition_id: PartitionId, partition_index: PartitionLogIndex) -> Self {
        Self {
            partition_id,
            partition_index,
        }
    }
}

pub struct SegmentedLogStoreConfig {
    pub start_index: LogIndex,
    pub coordinator: Arc<Mutex<SegmentedCoordinator>>,
    pub aof: AofConfig,
    pub manager: AofManagerConfig,
    pub append_timeout: Duration,
    pub verify_checksums: bool,
    pub partition_root: PathBuf,
    pub storage_pod: Option<StoragePodHandle>,
}

impl SegmentedLogStoreConfig {
    pub fn new(
        start_index: LogIndex,
        coordinator: Arc<Mutex<SegmentedCoordinator>>,
        aof: AofConfig,
        manager: AofManagerConfig,
    ) -> Self {
        let partition_root = aof.root_dir.join("partitions");
        Self {
            start_index,
            coordinator,
            aof,
            manager,
            append_timeout: Duration::from_secs(5),
            verify_checksums: true,
            partition_root,
            storage_pod: None,
        }
    }

    pub fn with_partition_root(mut self, root: PathBuf) -> Self {
        self.partition_root = root;
        self
    }

    pub fn append_timeout(mut self, timeout: Duration) -> Self {
        self.append_timeout = timeout;
        self
    }

    pub fn verify_checksums(mut self, verify: bool) -> Self {
        self.verify_checksums = verify;
        self
    }

    pub fn with_storage_pod(mut self, pod: StoragePodHandle) -> Self {
        self.storage_pod = Some(pod);
        self
    }
}

#[derive(Clone)]
struct SegmentedEntry {
    record_id: RecordId,
    header: SegmentedRecordHeader,
    term: Term,
    timestamp: u64,
    crc32: Option<u32>,
    encoded: Arc<[u8]>,
}

pub struct SegmentedLogStore {
    start_index: LogIndex,
    entries: Vec<SegmentedEntry>,
    coordinator: Arc<Mutex<SegmentedCoordinator>>,
    last_durable_index: LogIndex,
    commit_index: LogIndex,
    append_timeout: Duration,
    verify_checksums: bool,
    #[allow(dead_code)]
    aof_manager: Arc<AofManager>,
    aof: Aof,
    partition_store: PartitionLogStore,
    #[allow(dead_code)]
    storage_pod: Option<StoragePodHandle>,
}

impl SegmentedLogStore {
    pub fn new(config: SegmentedLogStoreConfig) -> RaftResult<Self> {
        let SegmentedLogStoreConfig {
            start_index,
            coordinator,
            aof,
            manager,
            append_timeout,
            verify_checksums,
            partition_root,
            storage_pod,
        } = config;

        let manager = Arc::new(AofManager::with_config(manager).map_err(Self::map_aof_err)?);
        let segmented_handle = manager.handle();
        let partition_aof_template = aof.clone();
        let aof = Aof::new(segmented_handle, aof).map_err(Self::map_aof_err)?;
        let partition_store = PartitionLogStore::new(
            Arc::clone(&manager),
            partition_aof_template,
            partition_root,
            append_timeout,
            verify_checksums,
        )?;
        let prior = LogIndex(start_index.0.saturating_sub(1));

        Ok(Self {
            start_index,
            entries: Vec::new(),
            coordinator,
            last_durable_index: prior,
            commit_index: prior,
            append_timeout,
            verify_checksums,
            aof_manager: manager,
            aof,
            partition_store,
            storage_pod,
        })
    }

    fn map_error(err: CoordinatorError) -> RaftError {
        RaftError::LogStoreError(err.to_string())
    }

    fn map_aof_err(err: AofError) -> RaftError {
        RaftError::LogStoreError(format!("aof error: {err}"))
    }

    fn lock_coordinator(&self) -> RaftResult<MutexGuard<'_, SegmentedCoordinator>> {
        self.coordinator
            .lock()
            .map_err(|_| RaftError::LogStoreError("segmented coordinator mutex poisoned".into()))
    }

    fn ensure_partition(&self, partition_id: PartitionId) -> RaftResult<()> {
        let mut guard = self.lock_coordinator()?;
        if !guard.dispatcher().has_partition(partition_id) {
            guard
                .upsert_partition(PartitionManifest::new(partition_id), None, None, None)
                .map_err(Self::map_error)?;
        }
        Ok(())
    }

    fn offset_for(&self, idx: LogIndex) -> Option<usize> {
        if idx < self.start_index {
            return None;
        }
        let offset = (idx.0 - self.start_index.0) as usize;
        if offset < self.entries.len() {
            Some(offset)
        } else {
            None
        }
    }

    fn next_slot_internal(&self) -> LogIndex {
        LogIndex(self.start_index.0 + self.entries.len() as u64)
    }

    fn append_record_bytes(&self, bytes: &[u8]) -> RaftResult<RecordId> {
        self.aof
            .append_record_with_timeout(bytes, self.append_timeout)
            .map_err(Self::map_aof_err)
    }

    fn load_entry(&self, entry: &SegmentedEntry) -> RaftResult<LogEntryRecord> {
        if self.verify_checksums {
            if let Some(expected) = entry.header.checksum {
                let encoded = entry.encoded.as_ref();
                if encoded.len() < SEGMENTED_RECORD_HEADER_LEN {
                    return Err(RaftError::LogStoreError(
                        "segmented record payload shorter than header".into(),
                    ));
                }
                let data_len = entry.header.payload_len as usize;
                let start = SEGMENTED_RECORD_HEADER_LEN;
                let end = start + data_len;
                if encoded.len() < end {
                    return Err(RaftError::LogStoreError(
                        "segmented record payload truncated".into(),
                    ));
                }
                let mut hasher = Hasher::new();
                hasher.update(&encoded[start..end]);
                let actual = hasher.finalize();
                if actual != expected {
                    return Err(RaftError::LogStoreError(
                        "segmented payload checksum mismatch".into(),
                    ));
                }
            }
        }

        Ok(LogEntryRecord::new(
            entry.term,
            entry.timestamp,
            entry.encoded.as_ref().to_vec(),
            entry.crc32,
        ))
    }

    fn entries_in_range(&self, start: LogIndex, end: LogIndex) -> RaftResult<Vec<LogEntryRecord>> {
        if end < start {
            return Err(RaftError::LogStoreError(
                "invalid log range: end < start".to_string(),
            ));
        }
        if start == end {
            return Ok(Vec::new());
        }
        let start_off = self
            .offset_for(start)
            .ok_or_else(|| RaftError::LogStoreError("start index out of range".into()))?;
        let end_idx = LogIndex(end.0.saturating_sub(1));
        let end_off = self
            .offset_for(end_idx)
            .map(|idx| idx + 1)
            .unwrap_or_else(|| self.entries.len());
        if start_off > end_off || end_off > self.entries.len() {
            return Err(RaftError::LogStoreError("log range out of bounds".into()));
        }
        let mut records = Vec::with_capacity(end_off - start_off);
        for entry in &self.entries[start_off..end_off] {
            records.push(self.load_entry(entry)?);
        }
        Ok(records)
    }

    fn decode_encoded_record(
        idx: LogIndex,
        raw: &[u8],
    ) -> RaftResult<(SegmentedRecordHeader, Vec<u8>)> {
        if raw.len() < SEGMENTED_RECORD_HEADER_LEN {
            return Err(RaftError::LogStoreError(
                "segmented record header truncated".to_string(),
            ));
        }
        let version = raw[0];
        if version != SEGMENTED_RECORD_VERSION {
            return Err(RaftError::LogStoreError(format!(
                "unsupported segmented record version: {version}"
            )));
        }
        let flags_byte = raw[1];
        let checksum_present = (flags_byte & CHECKSUM_PRESENT_FLAG) != 0;
        let flags = SegmentedRecordFlags::from_bits(flags_byte & !CHECKSUM_PRESENT_FLAG);
        let partition_id = u64::from_le_bytes(raw[4..12].try_into().unwrap());
        let partition_index = u64::from_le_bytes(raw[12..20].try_into().unwrap());
        let _encoded_raft_index = u64::from_le_bytes(raw[20..28].try_into().unwrap());
        let payload_len = u32::from_le_bytes(raw[28..32].try_into().unwrap());
        let checksum_value = u32::from_le_bytes(raw[32..36].try_into().unwrap());
        let total_len = SEGMENTED_RECORD_HEADER_LEN + payload_len as usize;
        if raw.len() < total_len {
            return Err(RaftError::LogStoreError(
                "segmented record payload truncated".to_string(),
            ));
        }
        let payload = raw[SEGMENTED_RECORD_HEADER_LEN..total_len].to_vec();
        let mut header = SegmentedRecordHeader {
            version,
            flags,
            partition_id,
            partition_index,
            raft_index: idx.0,
            payload_len,
            checksum: if checksum_present {
                Some(checksum_value)
            } else {
                None
            },
        };
        let mut flag_bits = header.flags.bits();
        if payload.is_empty() {
            flag_bits &= !SegmentedRecordFlags::HAS_PAYLOAD.bits();
        } else {
            flag_bits |= SegmentedRecordFlags::HAS_PAYLOAD.bits();
        }
        header = header.with_flags(SegmentedRecordFlags::from_bits(flag_bits));
        Ok((header, payload))
    }

    fn parse_payload(
        &self,
        idx: LogIndex,
        raw: &[u8],
    ) -> RaftResult<(SegmentedRecordHeader, Vec<u8>)> {
        if raw.len() >= SEGMENTED_RECORD_HEADER_LEN && raw[0] == SEGMENTED_RECORD_VERSION {
            return Self::decode_encoded_record(idx, raw);
        }
        if raw.len() < LEGACY_HEADER_LEN {
            return Err(RaftError::LogStoreError(
                "segmented log entry payload too short".to_string(),
            ));
        }
        let partition_id = u64::from_le_bytes(raw[0..8].try_into().unwrap());
        let partition_index = u64::from_le_bytes(raw[8..16].try_into().unwrap());
        let payload = raw[LEGACY_HEADER_LEN..].to_vec();
        let payload_len: u32 = payload
            .len()
            .try_into()
            .map_err(|_| RaftError::LogStoreError("payload too large".into()))?;
        let checksum = compute_checksum(&payload);
        let mut header =
            SegmentedRecordHeader::new(partition_id, partition_index, idx.0, payload_len);
        header = header.with_checksum(checksum);
        let mut flag_bits = header.flags.bits();
        if payload.is_empty() {
            flag_bits &= !SegmentedRecordFlags::HAS_PAYLOAD.bits();
        } else {
            flag_bits |= SegmentedRecordFlags::HAS_PAYLOAD.bits();
        }
        header = header.with_flags(SegmentedRecordFlags::from_bits(flag_bits));
        Ok((header, payload))
    }

    fn encode_record_blob(header: &SegmentedRecordHeader, payload: &[u8]) -> RaftResult<Vec<u8>> {
        let expected_len = header.payload_len as usize;
        if payload.len() != expected_len {
            return Err(RaftError::LogStoreError(
                "segmented record payload length mismatch".into(),
            ));
        }
        let mut bytes = Vec::with_capacity(SEGMENTED_RECORD_HEADER_LEN + payload.len());
        let mut flags_byte = header.flags.bits();
        if header.checksum.is_some() {
            flags_byte |= CHECKSUM_PRESENT_FLAG;
        } else {
            flags_byte &= !CHECKSUM_PRESENT_FLAG;
        }
        bytes.push(header.version);
        bytes.push(flags_byte);
        bytes.extend_from_slice(&[0u8; HEADER_RESERVED_BYTES]);
        bytes.extend_from_slice(&header.partition_id.to_le_bytes());
        bytes.extend_from_slice(&header.partition_index.to_le_bytes());
        bytes.extend_from_slice(&header.raft_index.to_le_bytes());
        bytes.extend_from_slice(&header.payload_len.to_le_bytes());
        bytes.extend_from_slice(&header.checksum.unwrap_or_default().to_le_bytes());
        bytes.extend_from_slice(payload);
        Ok(bytes)
    }

    fn insert_record(
        &mut self,
        idx: LogIndex,
        view: LogEntryView<'_>,
        mut is_append: bool,
    ) -> RaftResult<()> {
        if is_append && idx != self.next_slot_internal() {
            return Err(RaftError::LogStoreError(format!(
                "append index {:?} does not match next slot {:?}",
                idx,
                self.next_slot_internal()
            )));
        }
        if !is_append && self.offset_for(idx).is_none() {
            if idx == self.next_slot_internal() {
                is_append = true;
            } else {
                return Err(RaftError::LogStoreError(format!(
                    "log index {:?} out of bounds for write",
                    idx
                )));
            }
        }

        let (mut header, payload) = self.parse_payload(idx, view.payload)?;
        let payload_len: u32 = payload
            .len()
            .try_into()
            .map_err(|_| RaftError::LogStoreError("payload too large".into()))?;
        header.payload_len = payload_len;
        header.raft_index = idx.0;
        let partition_id = header.partition_id;
        let partition_index = header.partition_index;

        self.ensure_partition(partition_id)?;

        if is_append {
            let mut guard = self.lock_coordinator()?;
            guard
                .admit(partition_id, self.commit_index.0)
                .map_err(Self::map_error)?;
        }

        header = header.with_checksum(compute_checksum(&payload));
        let mut flag_bits = header.flags.bits();
        if payload.is_empty() {
            flag_bits &= !SegmentedRecordFlags::HAS_PAYLOAD.bits();
        } else {
            flag_bits |= SegmentedRecordFlags::HAS_PAYLOAD.bits();
        }
        header = header.with_flags(SegmentedRecordFlags::from_bits(flag_bits));

        let encoded = Self::encode_record_blob(&header, &payload)?;
        let record_id = self.append_record_bytes(&encoded)?;
        self.aof.flush_until(record_id).map_err(Self::map_aof_err)?;

        let encoded_arc: Arc<[u8]> = Arc::from(encoded.into_boxed_slice());

        let entry = SegmentedEntry {
            record_id,
            header,
            term: view.term,
            timestamp: view.timestamp,
            crc32: view.crc32,
            encoded: encoded_arc,
        };

        {
            let mut guard = self.lock_coordinator()?;
            guard
                .record_segmented_append(partition_id, partition_index, idx.0)
                .map_err(Self::map_error)?;
        }

        if is_append {
            self.entries.push(entry);
        } else {
            let offset = self.offset_for(idx).unwrap();
            self.entries[offset] = entry;
        }

        self.mark_durable(idx, partition_id, partition_index)?;
        Ok(())
    }

    fn mark_durable(
        &mut self,
        idx: LogIndex,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
    ) -> RaftResult<()> {
        if idx > self.last_durable_index {
            self.last_durable_index = idx;
        }
        let offset = self
            .offset_for(idx)
            .ok_or_else(|| RaftError::LogStoreError(format!("log index {:?} not recorded", idx)))?;
        let entry = &self.entries[offset];
        let encoded = entry.encoded.as_ref();
        let payload_start = SEGMENTED_RECORD_HEADER_LEN;
        let payload_end = payload_start + entry.header.payload_len as usize;
        if encoded.len() < payload_end {
            return Err(RaftError::LogStoreError(
                "segmented record payload truncated".into(),
            ));
        }
        let payload = &encoded[payload_start..payload_end];
        self.partition_store
            .record_durable(partition_id, partition_index, idx.0, &payload)?;
        let mut guard = self.lock_coordinator()?;
        guard
            .record_durable(partition_id, partition_index, idx.0)
            .map_err(Self::map_error)?;
        Ok(())
    }

    fn encode_pack(records: &[LogEntryRecord]) -> RaftResult<Buffer> {
        let mut blob = Vec::new();
        blob.extend_from_slice(&(records.len() as u32).to_le_bytes());
        for record in records {
            blob.extend_from_slice(&record.term.0.to_le_bytes());
            blob.extend_from_slice(&record.timestamp.to_le_bytes());
            match record.crc32 {
                Some(crc) => {
                    blob.push(1);
                    blob.extend_from_slice(&crc.to_le_bytes());
                }
                None => blob.push(0),
            }
            let payload_len: u32 = record
                .payload
                .len()
                .try_into()
                .map_err(|_| RaftError::LogStoreError("payload too large".into()))?;
            blob.extend_from_slice(&payload_len.to_le_bytes());
            blob.extend_from_slice(&record.payload);
        }
        Buffer::from_vec(blob)
    }

    fn decode_pack(pack: &[u8]) -> RaftResult<Vec<LogEntryRecord>> {
        if pack.len() < 4 {
            return Err(RaftError::LogStoreError("pack blob too small".into()));
        }
        let mut cursor = &pack[..];
        let count = u32::from_le_bytes(cursor[0..4].try_into().unwrap()) as usize;
        cursor = &cursor[4..];
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            if cursor.len() < 17 {
                return Err(RaftError::LogStoreError("pack record truncated".into()));
            }
            let term = Term(u64::from_le_bytes(cursor[0..8].try_into().unwrap()));
            let timestamp = u64::from_le_bytes(cursor[8..16].try_into().unwrap());
            let has_crc = cursor[16] != 0;
            cursor = &cursor[17..];
            let crc32 = if has_crc {
                if cursor.len() < 4 {
                    return Err(RaftError::LogStoreError("pack crc truncated".into()));
                }
                let crc = u32::from_le_bytes(cursor[0..4].try_into().unwrap());
                cursor = &cursor[4..];
                Some(crc)
            } else {
                None
            };
            if cursor.len() < 4 {
                return Err(RaftError::LogStoreError(
                    "pack payload length missing".into(),
                ));
            }
            let payload_len = u32::from_le_bytes(cursor[0..4].try_into().unwrap()) as usize;
            cursor = &cursor[4..];
            if cursor.len() < payload_len {
                return Err(RaftError::LogStoreError("pack payload truncated".into()));
            }
            let payload = cursor[..payload_len].to_vec();
            cursor = &cursor[payload_len..];
            records.push(LogEntryRecord::new(term, timestamp, payload, crc32));
        }
        Ok(records)
    }

    pub fn coordinator(&self) -> Arc<Mutex<SegmentedCoordinator>> {
        Arc::clone(&self.coordinator)
    }

    pub fn partition_metadata(&self, partition_id: PartitionId) -> Option<PartitionAofMetadata> {
        self.partition_store.metadata(partition_id)
    }

    pub fn segmented_watermarks(&self) -> RaftResult<SegmentedWatermarks> {
        let guard = self.lock_coordinator()?;
        Ok(guard.segmented_watermarks())
    }

    pub fn partition_watermark(
        &self,
        partition_id: PartitionId,
    ) -> RaftResult<Option<PartitionWatermark>> {
        let guard = self.lock_coordinator()?;
        Ok(guard.partition_watermark(partition_id))
    }

    pub fn update_commit_index(&mut self, commit_index: LogIndex) {
        self.commit_index = commit_index;
    }

    pub fn record_applied(
        &mut self,
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        global_index: LogIndex,
    ) -> RaftResult<()> {
        let mut guard = self.lock_coordinator()?;
        guard
            .record_applied(partition_id, partition_index, global_index.0)
            .map_err(Self::map_error)?;
        Ok(())
    }

    pub fn encode_segmented_payload(
        partition_id: PartitionId,
        partition_index: PartitionLogIndex,
        data: &[u8],
    ) -> Vec<u8> {
        let payload_len: u32 = data.len().try_into().expect("payload too large");
        let mut header = SegmentedRecordHeader::new(partition_id, partition_index, 0, payload_len);
        header = header.with_checksum(compute_checksum(data));
        let mut flag_bits = header.flags.bits();
        if data.is_empty() {
            flag_bits &= !SegmentedRecordFlags::HAS_PAYLOAD.bits();
        } else {
            flag_bits |= SegmentedRecordFlags::HAS_PAYLOAD.bits();
        }
        header = header.with_flags(SegmentedRecordFlags::from_bits(flag_bits));
        Self::encode_record_blob(&header, data).expect("encode segmented payload")
    }
}

impl LogStoreInterface for SegmentedLogStore {
    fn start_index(&self) -> LogIndex {
        self.start_index
    }

    fn next_slot(&self) -> LogIndex {
        self.next_slot_internal()
    }

    fn last_entry(&self) -> RaftResult<Option<LogEntryRecord>> {
        match self.entries.last() {
            Some(entry) => Ok(Some(self.load_entry(entry)?)),
            None => Ok(None),
        }
    }

    fn append(&mut self, entry: LogEntryView<'_>) -> RaftResult<LogIndex> {
        let idx = self.next_slot_internal();
        self.insert_record(idx, entry, true)?;
        Ok(idx)
    }

    fn write_at(&mut self, idx: LogIndex, entry: LogEntryView<'_>) -> RaftResult<()> {
        let is_append = idx == self.next_slot_internal();
        self.insert_record(idx, entry, is_append)
    }

    fn end_of_append_batch(&mut self, _start: LogIndex, _count: u64) -> RaftResult<()> {
        Ok(())
    }

    fn log_entries(&self, start: LogIndex, end: LogIndex) -> RaftResult<Vec<LogEntryRecord>> {
        self.entries_in_range(start, end)
    }

    fn entry_at(&self, idx: LogIndex) -> RaftResult<Option<LogEntryRecord>> {
        if let Some(offset) = self.offset_for(idx) {
            Ok(Some(self.load_entry(&self.entries[offset])?))
        } else {
            Ok(None)
        }
    }

    fn term_at(&self, idx: LogIndex) -> RaftResult<Option<Term>> {
        Ok(self.offset_for(idx).map(|offset| self.entries[offset].term))
    }

    fn pack(&self, idx: LogIndex, count: i32) -> RaftResult<Buffer> {
        if count <= 0 {
            return Buffer::from_vec(Vec::new());
        }
        let end = LogIndex(idx.0 + count as u64);
        let records = self.entries_in_range(idx, end)?;
        Self::encode_pack(&records)
    }

    fn apply_pack(&mut self, idx: LogIndex, pack: &[u8]) -> RaftResult<()> {
        let records = Self::decode_pack(pack)?;
        for (offset, record) in records.into_iter().enumerate() {
            let index = LogIndex(idx.0 + offset as u64);
            let is_append = index == self.next_slot_internal();
            let view =
                LogEntryView::new(record.term, record.timestamp, &record.payload, record.crc32);
            self.insert_record(index, view, is_append)?;
        }
        Ok(())
    }

    fn compact(&mut self, last_log_index: LogIndex) -> RaftResult<bool> {
        if last_log_index < self.start_index {
            return Ok(false);
        }
        let new_start = LogIndex(last_log_index.0 + 1);
        if new_start <= self.start_index {
            return Ok(false);
        }
        let drain_count = (new_start.0 - self.start_index.0) as usize;
        if drain_count > self.entries.len() {
            return Err(RaftError::LogStoreError(
                "compact exceeds stored entries".into(),
            ));
        }
        self.entries.drain(0..drain_count);
        self.start_index = new_start;
        if self.last_durable_index < last_log_index {
            self.last_durable_index = last_log_index;
        }
        Ok(true)
    }

    fn compact_async(&mut self, last_log_index: LogIndex) -> RaftResult<bool> {
        self.compact(last_log_index)
    }

    fn flush(&mut self) -> RaftResult<bool> {
        if let Some(entry) = self.entries.last() {
            self.aof
                .flush_until(entry.record_id)
                .map_err(Self::map_aof_err)?;
            let durable = LogIndex(entry.header.raft_index);
            if durable > self.last_durable_index {
                self.last_durable_index = durable;
            }
        }
        Ok(true)
    }

    fn last_durable_index(&self) -> LogIndex {
        self.last_durable_index
    }
}

fn compute_checksum(payload: &[u8]) -> Option<u32> {
    if payload.is_empty() {
        None
    } else {
        let mut hasher = Hasher::new();
        hasher.update(payload);
        Some(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmented::manifest_store::PartitionManifestStore;
    use crate::segmented::types::{PartitionDispatcherConfig, ReplicaId};
    use bop_aof::AofManagerConfig;
    use bop_aof::config::AofConfig;
    use bop_aof::manifest::ManifestLogWriterConfig;
    use tempfile::TempDir;

    fn writer_config() -> ManifestLogWriterConfig {
        ManifestLogWriterConfig {
            chunk_capacity_bytes: 8 * 1024,
            rotation_period: std::time::Duration::from_secs(1),
            growth_step_bytes: 8 * 1024,
            enabled: true,
        }
    }

    fn build_store(tempdir: &TempDir) -> (SegmentedLogStore, Arc<Mutex<SegmentedCoordinator>>) {
        let manifest_store =
            PartitionManifestStore::new(tempdir.path(), 42, writer_config()).unwrap();
        let coordinator = Arc::new(Mutex::new(SegmentedCoordinator::new(
            PartitionDispatcherConfig::default(),
            manifest_store,
        )));
        let mut aof_cfg = AofConfig::default();
        aof_cfg.root_dir = tempdir.path().join("segmented-log");
        let manager_cfg = AofManagerConfig::default();
        let config = SegmentedLogStoreConfig::new(
            LogIndex(1),
            Arc::clone(&coordinator),
            aof_cfg,
            manager_cfg,
        );
        let store = SegmentedLogStore::new(config).unwrap();
        (store, coordinator)
    }

    fn decode_record(bytes: &[u8]) -> (SegmentedRecordHeader, Vec<u8>) {
        assert!(bytes.len() >= SEGMENTED_RECORD_HEADER_LEN);
        let version = bytes[0];
        assert_eq!(version, SEGMENTED_RECORD_VERSION);
        let flags_byte = bytes[1];
        let checksum_present = (flags_byte & CHECKSUM_PRESENT_FLAG) != 0;
        let flags = SegmentedRecordFlags::from_bits(flags_byte & !CHECKSUM_PRESENT_FLAG);
        let partition_id = u64::from_le_bytes(bytes[4..12].try_into().unwrap());
        let partition_index = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let raft_index = u64::from_le_bytes(bytes[20..28].try_into().unwrap());
        let payload_len = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        let checksum = if checksum_present {
            Some(u32::from_le_bytes(bytes[32..36].try_into().unwrap()))
        } else {
            None
        };
        let payload = bytes
            [SEGMENTED_RECORD_HEADER_LEN..SEGMENTED_RECORD_HEADER_LEN + payload_len as usize]
            .to_vec();
        (
            SegmentedRecordHeader {
                version,
                flags,
                partition_id,
                partition_index,
                raft_index,
                payload_len,
                checksum,
            },
            payload,
        )
    }

    #[test]
    fn append_records_persist_manifest() {
        let tempdir = TempDir::new().unwrap();
        let (mut log_store, coordinator) = build_store(&tempdir);

        let payload = SegmentedLogStore::encode_segmented_payload(10, 1, b"hello");
        let view = LogEntryView::new(Term(1), 0, &payload, None);
        let idx = log_store.append(view).expect("append");
        log_store.flush().expect("flush");
        assert_eq!(idx, LogIndex(1));

        let entry = log_store.entry_at(idx).unwrap().unwrap();
        let (header, data) = decode_record(&entry.payload);
        assert_eq!(header.partition_id, 10);
        assert_eq!(header.partition_index, 1);
        assert_eq!(header.raft_index, 1);
        assert_eq!(data, b"hello");

        let updates = {
            let guard = coordinator.lock().unwrap();
            guard.manifest_store().load_updates().expect("load")
        };
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, 10);
        assert_eq!(updates[0].1.durable_index, Some(1));
    }

    #[test]
    fn append_registers_ack_tracker_entry() {
        let tempdir = TempDir::new().unwrap();
        let (mut log_store, coordinator) = build_store(&tempdir);

        let payload = SegmentedLogStore::encode_segmented_payload(15, 4, b"payload");
        let view = LogEntryView::new(Term(1), 0, &payload, None);
        let idx = log_store.append(view).expect("append");
        log_store.flush().expect("flush");

        {
            let mut guard = coordinator.lock().unwrap();
            guard.update_membership([ReplicaId(1)]).expect("membership");
        }

        {
            let mut guard = coordinator.lock().unwrap();
            let (_, outcome) = guard
                .acknowledge(15, 4, idx.0, ReplicaId(1))
                .expect("ack entry present");
            assert_eq!(outcome.partition_id, 15);
            assert_eq!(outcome.partition_index, 4);
            assert_eq!(outcome.raft_index, idx.0);
        }

        let metadata = log_store.partition_metadata(15).expect("metadata");
        assert_eq!(metadata.highest_durable_index, 4);
        assert_eq!(metadata.next_partition_index, 5);
        assert_eq!(metadata.last_applied_raft_index, idx.0);
    }

    #[test]
    fn record_applied_updates_manifest() {
        let tempdir = TempDir::new().unwrap();
        let (mut log_store, coordinator) = build_store(&tempdir);

        let payload = SegmentedLogStore::encode_segmented_payload(7, 3, b"payload");
        let view = LogEntryView::new(Term(1), 0, &payload, None);
        let idx = log_store.append(view).unwrap();
        log_store.flush().expect("flush");
        assert_eq!(idx, LogIndex(1));
        log_store.record_applied(7, 3, LogIndex(1)).unwrap();

        let updates = {
            let guard = coordinator.lock().unwrap();
            guard.manifest_store().load_updates().unwrap()
        };
        assert!(updates.len() >= 2);
        let applied = updates
            .iter()
            .rev()
            .find(|(pid, _)| *pid == 7)
            .unwrap()
            .1
            .clone();
        assert_eq!(applied.applied_index, Some(3));
        assert_eq!(applied.applied_global, Some(1));
    }

    #[test]
    fn pack_and_apply_roundtrip() {
        let tempdir = TempDir::new().unwrap();
        let (mut log_store, _coord) = build_store(&tempdir);

        let payload1 = SegmentedLogStore::encode_segmented_payload(3, 1, b"alpha");
        let payload2 = SegmentedLogStore::encode_segmented_payload(3, 2, b"beta");
        let view1 = LogEntryView::new(Term(1), 0, &payload1, None);
        let view2 = LogEntryView::new(Term(1), 0, &payload2, None);
        log_store.append(view1).unwrap();
        log_store.append(view2).unwrap();
        log_store.flush().expect("flush");

        let packed_bytes = {
            let packed = log_store.pack(LogIndex(1), 2).expect("pack");
            packed.to_vec()
        };

        drop(log_store);

        let tempdir2 = TempDir::new().unwrap();
        let (mut other, _coord2) = build_store(&tempdir2);
        other.apply_pack(LogIndex(1), &packed_bytes).expect("apply");

        let entries = other
            .log_entries(LogIndex(1), LogIndex(3))
            .expect("entries");
        assert_eq!(entries.len(), 2);
        let (_, data1) = decode_record(&entries[0].payload);
        assert_eq!(data1, b"alpha");
        let (_, data2) = decode_record(&entries[1].payload);
        assert_eq!(data2, b"beta");
    }

    #[test]
    fn compact_discards_obsolete_entries() {
        let tempdir = TempDir::new().unwrap();
        let (mut log_store, _coord) = build_store(&tempdir);

        for idx in 0..3 {
            let payload = SegmentedLogStore::encode_segmented_payload(9, idx + 1, &[idx as u8]);
            let view = LogEntryView::new(Term(1), 0, &payload, None);
            log_store.append(view).unwrap();
            log_store.flush().expect("flush");
        }

        assert!(log_store.compact(LogIndex(2)).expect("compact"));
        assert_eq!(log_store.start_index, LogIndex(3));
        let entries = log_store
            .log_entries(LogIndex(3), LogIndex(4))
            .expect("entries");
        assert_eq!(entries.len(), 1);
        let (_, data) = decode_record(&entries[0].payload);
        assert_eq!(data, vec![2]);
    }

    #[test]
    fn durable_index_advances_monotonically() {
        let tempdir = TempDir::new().unwrap();
        let (mut log_store, _coord) = build_store(&tempdir);

        for idx in 0..3 {
            let payload = SegmentedLogStore::encode_segmented_payload(5, idx + 1, &[idx as u8 + 1]);
            let view = LogEntryView::new(Term(1), 0, &payload, None);
            let slot = log_store.append(view).unwrap();
            log_store.flush().expect("flush");
            assert_eq!(log_store.last_durable_index(), slot);
        }
    }

    #[test]
    fn write_at_replaces_existing_entry() {
        let tempdir = TempDir::new().unwrap();
        let (mut log_store, _coord) = build_store(&tempdir);

        let payload = SegmentedLogStore::encode_segmented_payload(2, 1, b"old");
        let view = LogEntryView::new(Term(1), 0, &payload, None);
        log_store.append(view).unwrap();
        log_store.flush().expect("flush old");

        let replacement = SegmentedLogStore::encode_segmented_payload(2, 1, b"new");
        let view = LogEntryView::new(Term(2), 1, &replacement, None);
        log_store.write_at(LogIndex(1), view).unwrap();
        log_store.flush().expect("flush new");

        let entry = log_store.entry_at(LogIndex(1)).unwrap().unwrap();
        let (header, data) = decode_record(&entry.payload);
        assert_eq!(header.raft_index, 1);
        assert_eq!(data, b"new");
        assert_eq!(entry.term, Term(2));
    }
}
