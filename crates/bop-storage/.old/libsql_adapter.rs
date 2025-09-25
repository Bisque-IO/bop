//! Stack that adapts libsql VFS callbacks into chunk frames and shared caches.
use crate::chunk::{
    ChunkHeader, FrameHeader, LIBSQL_SUPER_FRAME_BYTES, LibsqlFrameHeader, LibsqlSuperFrame,
    RaftFrameMeta, SuperFrame, WalMode,
};
use crate::chunk_encoder::{ChunkEncodeError, ChunkRecordEncoder};
use crate::directory::DirectoryValue;
use crate::page_cache::{GlobalPageCache, PageCacheKey};
use std::io;
use std::sync::Arc;

/// Abstraction over the underlying chunk writer.
pub trait WalFrameSink {
    fn begin_segment(&mut self, header: &ChunkHeader, super_frame: &SuperFrame) -> io::Result<()>;
    fn append_frame(&mut self, frame: &FrameHeader, payload: &[u8]) -> io::Result<()>;
    fn finish_segment(&mut self) -> io::Result<()>;
}

/// Identifier for a libsql-backed page store participating in the global cache.
pub type PageStoreId = u64;

/// High-level libsql adapter wiring VFS callbacks into the shared chunk writer and cache.
#[derive(Debug)]
pub struct LibsqlWalAdapter<S> {
    sink: S,
    header: ChunkHeader,
    super_frame: Option<LibsqlSuperFrame>,
    encoder: ChunkRecordEncoder,
    cache: Arc<GlobalPageCache>,
    page_store_id: PageStoreId,
}

impl<S: WalFrameSink> LibsqlWalAdapter<S> {
    pub fn new(
        sink: S,
        sequence: u64,
        generation: u64,
        slab_count: u16,
        slab_size: u32,
        page_store_id: PageStoreId,
        cache: Arc<GlobalPageCache>,
    ) -> io::Result<Self> {
        let header = ChunkHeader {
            version: crate::chunk::CHUNK_VERSION,
            mode: WalMode::Libsql,
            slab_count,
            slab_size,
            sequence,
            generation,
            super_frame_bytes: 0,
        };
        Ok(Self {
            sink,
            header,
            super_frame: None,
            encoder: ChunkRecordEncoder::new(),
            cache,
            page_store_id,
        })
    }

    pub fn begin_segment(&mut self, super_frame: LibsqlSuperFrame) -> io::Result<()> {
        self.header.super_frame_bytes = LIBSQL_SUPER_FRAME_BYTES;
        let frame = SuperFrame::Libsql(super_frame.clone());
        self.sink.begin_segment(&self.header, &frame)?;
        self.super_frame = Some(super_frame);
        Ok(())
    }

    /// Appends a frame constructed by the caller and updates the shared page cache.
    pub fn append_frame(
        &mut self,
        generation: u64,
        mut frame_header: LibsqlFrameHeader,
        payload: Arc<Vec<u8>>,
    ) -> io::Result<()> {
        frame_header.payload_len = payload.len() as u32;
        self.sink
            .append_frame(&FrameHeader::Libsql(frame_header.clone()), &payload)?;
        let key = PageCacheKey {
            store_id: self.page_store_id,
            generation,
            page_id: frame_header.page_no as u64,
        };
        self.cache.insert(key, payload);
        Ok(())
    }

    /// Convenience helper that builds the frame header before appending and caching.
    pub fn append_encoded_frame(
        &mut self,
        generation: u64,
        page_no: u32,
        db_size_after_frame: u32,
        payload: Arc<Vec<u8>>,
        salt: (u32, u32),
        frame_checksum: Option<(u32, u32)>,
        raft: RaftFrameMeta,
    ) -> Result<(), LibsqlAdapterError> {
        let header = self.encoder.encode_libsql(
            page_no,
            db_size_after_frame,
            &payload,
            salt,
            frame_checksum,
            raft,
        )?;
        self.append_frame(generation, header, payload)
            .map_err(LibsqlAdapterError::Io)
    }

    pub fn finish_segment(&mut self) -> io::Result<()> {
        self.sink.finish_segment()?;
        self.super_frame = None;
        Ok(())
    }

    pub fn get_cached_page(&self, generation: u64, page_no: u32) -> Option<Arc<Vec<u8>>> {
        let key = PageCacheKey {
            store_id: self.page_store_id,
            generation,
            page_id: page_no as u64,
        };
        self.cache.get(&key)
    }
}

/// Errors surfaced by high-level libsql adapter operations.
#[derive(Debug, thiserror::Error)]
pub enum LibsqlAdapterError {
    #[error(transparent)]
    Encode(#[from] ChunkEncodeError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Convenience helper that builds a libsql frame header from raw VFS parameters.
pub fn build_libsql_frame_header(
    page_no: u32,
    db_size_after_frame: u32,
    page_checksum: (u32, u32),
    salt: (u32, u32),
    directory_value: DirectoryValue,
    raft: RaftFrameMeta,
) -> LibsqlFrameHeader {
    LibsqlFrameHeader {
        page_no,
        directory_value,
        db_size_after_frame,
        salt: [salt.0, salt.1],
        frame_checksum: [page_checksum.0, page_checksum.1],
        payload_len: 0,
        raft,
    }
}
