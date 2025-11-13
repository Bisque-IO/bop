//! Storage primitives shared across bop components.
//!
//! The implementation will follow the design documented in `docs/design.md`.

pub mod checkpoint;
pub mod chunk;
pub mod chunk_encoder;
pub mod config;
pub mod direct_io;
pub mod directory;
pub mod flush_controller;
pub mod libsql_adapter;
pub mod libsql_vfs;
pub mod manifest;
pub mod page_cache;
pub mod paged_file;
pub mod slab;
pub mod storage_fleet;
pub mod storage_pod;
pub mod storage_types;
pub mod superblock;
pub mod wal;

pub use checkpoint::{
    CheckpointCoroutine, CheckpointError, CheckpointPlan, CheckpointStatus, CheckpointStep,
    ChunkFlushPlan,
};
pub use chunk::{
    BASE_SUPER_FRAME_BYTES, BaseSuperFrame, CHUNK_MAGIC, CHUNK_VERSION, ChunkDescriptor,
    ChunkHeader, ChunkParseError, FrameDescriptor, FrameHeader, LIBSQL_SUPER_FRAME_BYTES,
    LibsqlFrameHeader, LibsqlSuperFrame, RaftFrameMeta, SuperFrame, WalMode, parse_chunk,
};
pub use chunk_encoder::{ChunkEncodeError, ChunkRecordEncoder};
pub use config::{ExtentSize, SlabSize, StorageConfig, StorageFlavor};
pub use direct_io::{
    DirectFile, DirectFileError, DirectIoConstraints, DirectIoError, DirectIoWriter,
    buffer_alignment, file_offset_alignment, platform_constraints,
};
pub use directory::{DirectoryValue, DirectoryValueError, PageDirectory};
pub use flush_controller::{
    FlushAction, FlushActionError, FlushController, FlushControllerConfig, FlushControllerSnapshot,
    FlushError, FlushScheduleError, FlushStage, FlushTicket,
};
pub use libsql_adapter::{
    LibsqlAdapterError, LibsqlWalAdapter, PageStoreId, WalFrameSink, build_libsql_frame_header,
};
pub use libsql_vfs::{LibsqlVfs, VfsError, VfsOp, VfsReadCoroutine, VfsStatus, VfsWriteCoroutine};
pub use manifest::{
    ChunkId, ChunkMetadata, ChunkRefCounter, ManifestDelta, ManifestError, ManifestLogReader,
    ManifestLogWriter, ManifestNamespace, ManifestPath, ManifestRecord, ManifestState,
    replay_manifest,
};
pub use page_cache::{GlobalPageCache, PageCacheKey};
pub use paged_file::{MockPagedFile, MockPagedFileError, PageId, PageRange, PagedFile};
pub use slab::{
    AlignedSlab, PublishedSlab, ReadaheadPolicy, ReaderCursor, SlabAllocationError, SlabFlags,
    SlabPool, SlabPublishError, SlabPublishParams, TailMetrics, WriterLease,
};
pub use storage_fleet::{StorageFleet, StorageFleetConfig, StorageFleetError};
pub use storage_pod::{
    PodFlushFailure, PodFlushState, PodSnapshotIntent, SnapshotPublish, StoragePodConfig,
    StoragePodError, StoragePodHandle, StoragePodMetadata,
};
pub use storage_types::{PodStatus, StoragePodHealthSnapshot, StoragePodId};
pub use superblock::{Superblock, SuperblockError, SuperblockWriter};
pub use wal::{WalError, WalPipeline, WalPipelineError, WalRecord, WalSegment, WalWriter};
