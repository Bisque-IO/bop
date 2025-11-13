//! WAL batching, publishing, and direct I/O pipeline utilities shared across storage flavors.
use crate::chunk::{
    BASE_SUPER_FRAME_BYTES, CHUNK_VERSION, ChunkHeader, FrameHeader, LIBSQL_SUPER_FRAME_BYTES,
    LibsqlFrameHeader, RaftFrameMeta, StandardFrameHeader, SuperFrame, WalMode,
};
use crate::config::{PAGE_SIZE, StorageConfig};
use crate::direct_io::{DirectFile, DirectFileError, DirectIoError, DirectIoWriter};
use crate::directory::DirectoryValue;
use crate::slab::{
    PublishedSlab, SlabAllocationError, SlabFlags, SlabPool, SlabPublishError, SlabPublishParams,
    WriterLease,
};
use std::fmt;
use std::path::Path;

/// WAL record capturing the encoded frame metadata that was flushed to the chunk buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalRecord {
    pub header: FrameHeader,
}

impl WalRecord {
    pub fn payload_len(&self) -> u32 {
        match &self.header {
            FrameHeader::Standard(header) => header.payload_len,
            FrameHeader::Libsql(header) => header.payload_len,
        }
    }
}

/// Errors surfaced by the WAL writer path.
#[derive(Debug)]
pub enum WalError {
    AllocationFailed(SlabAllocationError),
    PublishFailed(SlabPublishError),
    FlushRequired,
    NoActiveBatch,
    InsufficientCapacity { requested: usize, remaining: usize },
    PayloadLengthMismatch { expected: usize, actual: usize },
    BatchInProgress,
    WalModeMismatch { expected: WalMode, actual: WalMode },
    PayloadNotPageAligned { len: usize },
}

impl From<SlabAllocationError> for WalError {
    fn from(value: SlabAllocationError) -> Self {
        Self::AllocationFailed(value)
    }
}

impl From<SlabPublishError> for WalError {
    fn from(value: SlabPublishError) -> Self {
        Self::PublishFailed(value)
    }
}

/// Published WAL fragment along with record metadata for replay.
#[derive(Debug)]
pub struct WalSegment {
    pub slab: PublishedSlab,
    pub records: Vec<WalRecord>,
    pub chunk_header: ChunkHeader,
    pub super_frame: Option<SuperFrame>,
    pub epoch: u64,
}

/// WAL writer building slabs via the shared pool and enforcing flush-before-publish semantics.
#[derive(Debug)]
pub struct WalWriter {
    pool: SlabPool,
    config: StorageConfig,
    mode: WalMode,
    current: Option<WalBatch>,
    epoch: u64,
    next_sequence: u64,
    super_frame: Option<SuperFrame>,
}

impl WalWriter {
    pub fn new(pool: SlabPool, config: StorageConfig) -> Self {
        Self::with_mode(pool, config, WalMode::Standard)
    }

    pub fn with_mode(pool: SlabPool, config: StorageConfig, mode: WalMode) -> Self {
        Self {
            pool,
            config,
            mode,
            current: None,
            epoch: 0,
            next_sequence: 0,
            super_frame: None,
        }
    }

    pub fn config(&self) -> StorageConfig {
        self.config
    }

    pub fn mode(&self) -> WalMode {
        self.mode
    }

    pub fn set_super_frame(&mut self, frame: SuperFrame) {
        self.super_frame = Some(frame);
    }

    pub fn clear_super_frame(&mut self) {
        self.super_frame = None;
    }

    pub fn append_standard(
        &mut self,
        page_id: u64,
        directory_value: DirectoryValue,
        checksum: u32,
        payload: &[u8],
        raft: RaftFrameMeta,
    ) -> Result<(), WalError> {
        if self.mode != WalMode::Standard {
            return Err(WalError::WalModeMismatch {
                expected: self.mode,
                actual: WalMode::Standard,
            });
        }
        if payload.len() % PAGE_SIZE as usize != 0 {
            return Err(WalError::PayloadNotPageAligned { len: payload.len() });
        }
        let payload_len = payload.len() as u32;
        let header = FrameHeader::Standard(StandardFrameHeader {
            page_id,
            directory_value,
            payload_len,
            checksum,
            raft,
        });
        self.append_frame(header, payload)
    }

    pub fn append_libsql(
        &mut self,
        mut header: LibsqlFrameHeader,
        payload: &[u8],
    ) -> Result<(), WalError> {
        if self.mode != WalMode::Libsql {
            return Err(WalError::WalModeMismatch {
                expected: self.mode,
                actual: WalMode::Libsql,
            });
        }
        header.payload_len = payload.len() as u32;
        self.append_frame(FrameHeader::Libsql(header), payload)
    }

    fn append_frame(&mut self, frame_header: FrameHeader, payload: &[u8]) -> Result<(), WalError> {
        let actual_mode = match &frame_header {
            FrameHeader::Standard(_) => WalMode::Standard,
            FrameHeader::Libsql(_) => WalMode::Libsql,
        };
        if self.mode != actual_mode {
            return Err(WalError::WalModeMismatch {
                expected: self.mode,
                actual: actual_mode,
            });
        }
        let batch = self.ensure_batch()?;
        batch.append_frame(frame_header, payload)
    }

    pub fn flush(&mut self) -> Result<(), WalError> {
        if let Some(batch) = &mut self.current {
            batch.flush()?;
        }
        Ok(())
    }

    pub fn publish(&mut self) -> Result<WalSegment, WalError> {
        let payload_len = {
            let batch = self.current.as_ref().ok_or(WalError::NoActiveBatch)?;
            if !batch.is_flushed() {
                return Err(WalError::FlushRequired);
            }
            batch.aligned_payload_len()
        };
        let params = self.build_publish_params(payload_len)?;
        let batch = self.current.take().expect("batch should be present");
        let (slab, records, chunk_header, super_frame) = batch.seal(params)?;
        let segment = WalSegment {
            slab,
            records,
            chunk_header,
            super_frame,
            epoch: self.epoch,
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        Ok(segment)
    }

    pub fn rotate_checkpoint(&mut self, new_epoch: u64) -> Result<(), WalError> {
        if self.current.is_some() {
            return Err(WalError::BatchInProgress);
        }
        self.epoch = new_epoch;
        self.next_sequence = 0;
        Ok(())
    }

    fn build_publish_params(&self, payload_len: usize) -> Result<SlabPublishParams, WalError> {
        if payload_len % PAGE_SIZE as usize != 0 {
            return Err(WalError::PayloadNotPageAligned { len: payload_len });
        }
        let page_count = (payload_len / PAGE_SIZE as usize) as u32;
        Ok(SlabPublishParams::new(0, page_count, payload_len).with_flags(SlabFlags::NONE))
    }

    fn ensure_batch(&mut self) -> Result<&mut WalBatch, WalError> {
        if self.current.is_none() {
            let lease = self.pool.acquire_writer()?;
            let extent = self.config.extent_size().get() as usize;
            let header = ChunkHeader {
                version: CHUNK_VERSION,
                mode: self.mode,
                slab_count: 1,
                slab_size: self.config.slab_size().get() as u32,
                sequence: self.next_sequence,
                generation: self.epoch,
                super_frame_bytes: 0,
            };
            let super_frame = self.super_frame.clone();
            self.current = Some(WalBatch::new(lease, extent, header, super_frame));
        }
        Ok(self.current.as_mut().expect("batch must exist"))
    }
}

#[derive(Debug)]
/// Errors emitted while streaming WAL batches into the durability pipeline.
pub enum WalPipelineError {
    Wal(WalError),
    Io(DirectIoError<DirectFileError>),
    ConfigMismatch {
        writer: StorageConfig,
        device: StorageConfig,
    },
}

impl fmt::Display for WalPipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wal(err) => write!(f, "wal pipeline wal error: {:?}", err),
            Self::Io(err) => write!(f, "wal pipeline io error: {err}"),
            Self::ConfigMismatch { writer, device } => write!(
                f,
                "wal pipeline config mismatch writer={:?} device={:?}",
                writer, device
            ),
        }
    }
}

impl std::error::Error for WalPipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Wal(_) => None,
            Self::Io(err) => Some(err),
            Self::ConfigMismatch { .. } => None,
        }
    }
}

impl From<WalError> for WalPipelineError {
    fn from(value: WalError) -> Self {
        Self::Wal(value)
    }
}

impl From<DirectIoError<DirectFileError>> for WalPipelineError {
    fn from(value: DirectIoError<DirectFileError>) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
/// Couples a `WalWriter` with a direct-I/O device to simplify publish/flush/checkpoint flows.
pub struct WalPipeline {
    writer: WalWriter,
    device: DirectIoWriter<DirectFile>,
}

impl WalPipeline {
    pub fn open(
        pool: SlabPool,
        config: StorageConfig,
        path: impl AsRef<Path>,
    ) -> Result<Self, DirectFileError> {
        let device = DirectIoWriter::open_path(path, config)?;
        Ok(Self {
            writer: WalWriter::new(pool, config),
            device,
        })
    }

    /// Reuses an existing writer/device pair after verifying they share configuration.
    pub fn with_parts(
        writer: WalWriter,
        device: DirectIoWriter<DirectFile>,
    ) -> Result<Self, WalPipelineError> {
        let writer_cfg = writer.config();
        let device_cfg = *device.config();
        if writer_cfg != device_cfg {
            return Err(WalPipelineError::ConfigMismatch {
                writer: writer_cfg,
                device: device_cfg,
            });
        }
        Ok(Self { writer, device })
    }

    /// Forwards a standard frame into the underlying WAL batcher.
    pub fn append_standard(
        &mut self,
        page_id: u64,
        directory_value: DirectoryValue,
        checksum: u32,
        payload: &[u8],
        raft: RaftFrameMeta,
    ) -> Result<(), WalPipelineError> {
        self.writer
            .append_standard(page_id, directory_value, checksum, payload, raft)
            .map_err(Into::into)
    }

    /// Forwards a libsql frame into the underlying WAL batcher.
    pub fn append_libsql(
        &mut self,
        header: LibsqlFrameHeader,
        payload: &[u8],
    ) -> Result<(), WalPipelineError> {
        self.writer
            .append_libsql(header, payload)
            .map_err(Into::into)
    }

    pub fn set_super_frame(&mut self, frame: SuperFrame) {
        self.writer.set_super_frame(frame);
    }

    pub fn clear_super_frame(&mut self) {
        self.writer.clear_super_frame();
    }

    /// Flushes both the WAL batch and the direct file to guarantee durability without publishing.
    pub fn flush(&mut self) -> Result<(), WalPipelineError> {
        self.writer.flush().map_err(WalPipelineError::from)?;
        self.device.flush().map_err(WalPipelineError::from)
    }

    /// Publishes the current batch and persists the resulting slab to disk.
    pub fn publish(&mut self) -> Result<WalSegment, WalPipelineError> {
        let segment = self.writer.publish().map_err(WalPipelineError::from)?;
        self.device
            .write_slab(&segment.slab)
            .map_err(WalPipelineError::from)?;
        Ok(segment)
    }

    /// Completes a checkpoint by rotating WAL state and syncing the underlying file.
    pub fn rotate_checkpoint(&mut self, new_epoch: u64) -> Result<(), WalPipelineError> {
        self.writer
            .rotate_checkpoint(new_epoch)
            .map_err(WalPipelineError::from)?;
        self.device
            .complete_checkpoint()
            .map_err(WalPipelineError::from)?;
        Ok(())
    }

    pub fn device(&self) -> &DirectIoWriter<DirectFile> {
        &self.device
    }

    pub fn writer(&self) -> &WalWriter {
        &self.writer
    }
}
#[derive(Debug)]
struct WalBatch {
    lease: WriterLease,
    extent: usize,
    cursor: usize,
    records: Vec<WalRecord>,
    flushed: bool,
    header: ChunkHeader,
    super_frame: Option<SuperFrame>,
    super_frame_bytes: Vec<u8>,
}

impl WalBatch {
    fn new(
        lease: WriterLease,
        extent: usize,
        mut header: ChunkHeader,
        super_frame: Option<SuperFrame>,
    ) -> Self {
        let super_frame_bytes = encode_super_frame(super_frame.as_ref());
        header.super_frame_bytes = super_frame_bytes.len() as u32;
        let mut batch = Self {
            lease,
            extent,
            cursor: 0,
            records: Vec::new(),
            flushed: false,
            header,
            super_frame,
            super_frame_bytes,
        };
        batch.initialise_buffer();
        batch
    }

    fn initialise_buffer(&mut self) {
        let header_bytes = self.header.encode();
        {
            let buf = self.lease.buffer_mut();
            let header_len = header_bytes.len();
            buf[..header_len].copy_from_slice(&header_bytes);
            self.cursor = header_len;
            if !self.super_frame_bytes.is_empty() {
                let end = self.cursor + self.super_frame_bytes.len();
                buf[self.cursor..end].copy_from_slice(&self.super_frame_bytes);
                self.cursor = end;
            }
        }
    }

    fn remaining(&self) -> usize {
        self.lease.capacity().saturating_sub(self.cursor)
    }

    fn append_frame(&mut self, frame_header: FrameHeader, payload: &[u8]) -> Result<(), WalError> {
        match &frame_header {
            FrameHeader::Standard(header) => {
                if header.payload_len as usize != payload.len() {
                    return Err(WalError::PayloadLengthMismatch {
                        expected: header.payload_len as usize,
                        actual: payload.len(),
                    });
                }
                if payload.len() % PAGE_SIZE as usize != 0 {
                    return Err(WalError::PayloadNotPageAligned { len: payload.len() });
                }
                let encoded = header.encode();
                self.write_frame_bytes(&encoded, payload)?;
            }
            FrameHeader::Libsql(header) => {
                if header.payload_len as usize != payload.len() {
                    return Err(WalError::PayloadLengthMismatch {
                        expected: header.payload_len as usize,
                        actual: payload.len(),
                    });
                }
                if payload.len() % PAGE_SIZE as usize != 0 {
                    return Err(WalError::PayloadNotPageAligned { len: payload.len() });
                }
                let encoded = header.encode();
                self.write_frame_bytes(&encoded, payload)?;
            }
        }
        self.records.push(WalRecord {
            header: frame_header,
        });
        Ok(())
    }

    fn write_frame_bytes(&mut self, header_bytes: &[u8], payload: &[u8]) -> Result<(), WalError> {
        let needed = header_bytes.len() + payload.len();
        if needed > self.remaining() {
            return Err(WalError::InsufficientCapacity {
                requested: needed,
                remaining: self.remaining(),
            });
        }
        {
            let buf = self.lease.buffer_mut();
            let start = self.cursor;
            let header_end = start + header_bytes.len();
            buf[start..header_end].copy_from_slice(header_bytes);
            let payload_end = header_end + payload.len();
            buf[header_end..payload_end].copy_from_slice(payload);
            self.cursor = payload_end;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), WalError> {
        self.flushed = true;
        Ok(())
    }

    fn is_flushed(&self) -> bool {
        self.flushed
    }

    fn aligned_payload_len(&self) -> usize {
        if self.cursor == 0 {
            return 0;
        }
        let align = self.extent;
        ((self.cursor + align - 1) / align) * align
    }

    fn seal(
        mut self,
        params: SlabPublishParams,
    ) -> Result<
        (
            PublishedSlab,
            Vec<WalRecord>,
            ChunkHeader,
            Option<SuperFrame>,
        ),
        WalError,
    > {
        if !self.flushed {
            return Err(WalError::FlushRequired);
        }
        let expected = self.aligned_payload_len();
        if expected != params.payload_len {
            return Err(WalError::PayloadLengthMismatch {
                expected,
                actual: params.payload_len,
            });
        }
        self.pad_tail(expected);
        let slab = self.lease.seal(params)?;
        let header = self.header.clone();
        let super_frame = self.super_frame.clone();
        Ok((slab, self.records, header, super_frame))
    }

    fn pad_tail(&mut self, target: usize) {
        if self.cursor >= target {
            return;
        }
        let buf = self.lease.buffer_mut();
        buf[self.cursor..target].fill(0);
        self.cursor = target;
    }
}

fn encode_super_frame(super_frame: Option<&SuperFrame>) -> Vec<u8> {
    match super_frame {
        None => Vec::new(),
        Some(SuperFrame::Base(base)) => {
            let mut buf = Vec::with_capacity(BASE_SUPER_FRAME_BYTES as usize);
            buf.extend_from_slice(&base.snapshot_generation.to_le_bytes());
            buf.extend_from_slice(&base.wal_sequence.to_le_bytes());
            buf.extend_from_slice(&base.bitmap_hash.to_le_bytes());
            buf
        }
        Some(SuperFrame::Libsql(meta)) => {
            let mut buf = Vec::with_capacity(LIBSQL_SUPER_FRAME_BYTES as usize);
            buf.extend_from_slice(&meta.base.snapshot_generation.to_le_bytes());
            buf.extend_from_slice(&meta.base.wal_sequence.to_le_bytes());
            buf.extend_from_slice(&meta.base.bitmap_hash.to_le_bytes());
            buf.extend_from_slice(&meta.db_page_count.to_le_bytes());
            buf.extend_from_slice(&meta.change_counter.to_le_bytes());
            buf.extend_from_slice(&meta.salt[0].to_le_bytes());
            buf.extend_from_slice(&meta.salt[1].to_le_bytes());
            buf.extend_from_slice(&meta.schema_cookie.to_le_bytes());
            buf.extend_from_slice(&meta.mx_frame.to_le_bytes());
            buf
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{BaseSuperFrame, LibsqlSuperFrame, SuperFrame, WalMode, parse_chunk};
    use crate::config::{MIN_EXTENT_SIZE, MIN_SLAB_SIZE, StorageConfig, StorageFlavor};
    use crate::directory::DirectoryValue;
    use crate::slab::SlabPool;
    use tempfile::tempdir;

    fn test_config() -> StorageConfig {
        StorageConfig::new(StorageFlavor::Aof, MIN_EXTENT_SIZE, MIN_SLAB_SIZE, 2).unwrap()
    }

    #[test]
    fn publish_requires_flush() {
        let config = test_config();
        let pool = SlabPool::new(config).expect("pool");
        let mut writer = WalWriter::new(pool.clone(), config);
        let payload = vec![0xCD; MIN_EXTENT_SIZE as usize];
        let directory_value = DirectoryValue::new(payload.len() as u32, 0xDEADBEEF, false, 0)
            .expect("directory value");
        let raft = RaftFrameMeta { term: 1, index: 1 };
        let super_frame = SuperFrame::Base(BaseSuperFrame {
            snapshot_generation: 10,
            wal_sequence: 11,
            bitmap_hash: 12,
        });
        writer.set_super_frame(super_frame.clone());
        writer
            .append_standard(0, directory_value, 0xABCD1234, &payload, raft)
            .expect("append");
        let err = writer.publish().expect_err("flush required");
        assert!(matches!(err, WalError::FlushRequired));
        writer.flush().expect("flush");
        let segment = writer.publish().expect("publish after flush");
        assert_eq!(segment.records.len(), 1);
        match &segment.records[0].header {
            FrameHeader::Standard(header) => {
                assert_eq!(header.page_id, 0);
                assert_eq!(header.raft, raft);
            }
            _ => panic!("expected standard frame"),
        }
        let descriptor = parse_chunk(segment.slab.payload()).expect("parse chunk");
        assert_eq!(descriptor.header.mode, WalMode::Standard);
        assert_eq!(descriptor.frames.len(), 1);
        assert!(matches!(segment.super_frame, Some(SuperFrame::Base(_))));
    }

    #[test]
    fn rotate_checkpoint_requires_idle_state() {
        let config = test_config();
        let pool = SlabPool::new(config).expect("pool");
        let mut writer = WalWriter::new(pool.clone(), config);
        let payload = vec![0x11; MIN_EXTENT_SIZE as usize];
        let directory_value = DirectoryValue::new(payload.len() as u32, 0, false, 0).unwrap();
        let raft = RaftFrameMeta { term: 2, index: 3 };
        writer
            .append_standard(4, directory_value, 0, &payload, raft)
            .expect("append");
        let err = writer.rotate_checkpoint(1).expect_err("active batch");
        assert!(matches!(err, WalError::BatchInProgress));
        writer.flush().expect("flush");
        let _ = writer.publish().expect("publish");
        writer.rotate_checkpoint(2).expect("checkpoint");
    }

    #[test]
    fn capacity_guard_raises_error() {
        let config = test_config();
        let pool = SlabPool::new(config).expect("pool");
        let mut writer = WalWriter::new(pool.clone(), config);
        let payload = vec![0xAB; (MIN_SLAB_SIZE + MIN_EXTENT_SIZE) as usize];
        let directory_value = DirectoryValue::new(payload.len() as u32, 0, false, 0).unwrap();
        let raft = RaftFrameMeta { term: 1, index: 42 };
        let err = writer
            .append_standard(8, directory_value, 0, &payload, raft)
            .expect_err("should exceed capacity");
        assert!(matches!(err, WalError::InsufficientCapacity { .. }));
    }
    #[test]
    fn pipeline_persists_segments_via_direct_file() {
        let config = test_config();
        let pool = SlabPool::new(config).expect("pool");
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("wal_pipeline.dat");
        let mut pipeline = WalPipeline::open(pool, config, &path).expect("pipeline");
        let payload = vec![0x42; MIN_EXTENT_SIZE as usize];
        let directory_value =
            DirectoryValue::new(payload.len() as u32, 0xBEEFCAFE, false, 0).expect("value");
        let raft = RaftFrameMeta { term: 5, index: 7 };
        let super_frame = SuperFrame::Base(BaseSuperFrame {
            snapshot_generation: 99,
            wal_sequence: 42,
            bitmap_hash: 31337,
        });
        pipeline.set_super_frame(super_frame.clone());
        pipeline
            .append_standard(0, directory_value, 0xCAFEBABE, &payload, raft)
            .expect("append");
        pipeline.flush().expect("flush batch");
        let segment = pipeline.publish().expect("publish");
        pipeline.flush().expect("flush device");
        pipeline.rotate_checkpoint(1).expect("checkpoint");
        let descriptor = parse_chunk(segment.slab.payload()).expect("parse chunk");
        assert_eq!(descriptor.header.mode, WalMode::Standard);
        assert!(matches!(segment.super_frame, Some(SuperFrame::Base(_))));
        assert_eq!(descriptor.frames.len(), 1);
    }

    #[test]
    fn libsql_chunk_roundtrip_parses() {
        let config = test_config();
        let pool = SlabPool::new(config).expect("pool");
        let mut writer = WalWriter::with_mode(pool.clone(), config, WalMode::Libsql);
        let payload = vec![0x55; PAGE_SIZE as usize];
        let directory_value =
            DirectoryValue::new(payload.len() as u32, 0xFACE_CAFE, false, 0).expect("value");
        let raft = RaftFrameMeta { term: 8, index: 9 };
        let super_frame = SuperFrame::Libsql(LibsqlSuperFrame {
            base: BaseSuperFrame {
                snapshot_generation: 7,
                wal_sequence: 8,
                bitmap_hash: 9,
            },
            db_page_count: 2048,
            change_counter: 3,
            salt: [0x1111_2222, 0x3333_4444],
            schema_cookie: 5,
            mx_frame: 6,
        });
        writer.set_super_frame(super_frame.clone());
        let header = LibsqlFrameHeader {
            page_no: 123,
            directory_value,
            db_size_after_frame: 2048,
            salt: [0xAAAA_BBBB, 0xCCCC_DDDD],
            frame_checksum: [0x1234_5678, 0x90AB_CDEF],
            payload_len: 0,
            raft,
        };
        writer
            .append_libsql(header, &payload)
            .expect("append libsql");
        writer.flush().expect("flush");
        let segment = writer.publish().expect("publish");
        let descriptor = parse_chunk(segment.slab.payload()).expect("parse chunk");
        assert_eq!(descriptor.header.mode, WalMode::Libsql);
        assert_eq!(descriptor.frames.len(), 1);
        match &descriptor.frames[0].header {
            FrameHeader::Libsql(frame) => {
                assert_eq!(frame.page_no, 123);
                assert_eq!(frame.raft, raft);
                assert_eq!(frame.payload_len, payload.len() as u32);
            }
            _ => panic!("expected libsql frame"),
        }
        assert!(matches!(segment.super_frame, Some(SuperFrame::Libsql(_))));
    }
}
