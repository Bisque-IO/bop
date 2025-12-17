pub mod async_io;
pub mod chunk_storage;
pub mod compactor;
pub mod database;
pub mod page_manager;
pub mod raft_log;
pub mod schema;
pub mod schema_evolution;
pub mod store;
pub mod virtual_wal;
pub mod wal_storage;
pub mod wal_stream;

pub use chunk_storage::{ChunkKey, ChunkStorage, LocalFsChunkStorage, StorageError, StorageResult};
pub use database::{
    Branch, Database, DatabaseConfig, DatabaseManager, StorageConfig, VirtualWalHandle,
    DEFAULT_CHUNK_SIZE, DEFAULT_PAGE_SIZE, DEFAULT_PAGES_PER_CHUNK,
};
pub use page_manager::{
    AsyncPageManager, CachedPage, ChunkConfig, ChunkHeader, PageCache, PageCacheKey, PageManager,
    compress_chunk, decompress_chunk, prepare_chunk_for_storage, restore_chunk_from_storage,
};
pub use raft_log::{
    BranchDurabilityState, BranchRef, RaftLogManager, RaftSegmentStorage, SegmentCompactionInfo,
};
pub use schema::{
    BranchId, BranchInfo, BranchKey, CheckpointInfo, ChunkId, DatabaseId, DatabaseInfo,
    Evolution, EvolutionId, EvolutionKey, EvolutionState, EvolutionStep, FoundChunk,
    RaftEntryType, RaftLogEntry, RaftLogKey, RaftSegment, RaftSegmentKey, RaftState,
    SegmentVersionKey, WalMapKey, WalSegment,
};
pub use schema_evolution::{ChangesetApplier, DdlExecutor, EvolutionManager, EvolutionStatus};
pub use store::Store;
pub use virtual_wal::{
    ChunkLocation, NoopAsyncStorage, VirtualWal, VirtualWalManager, WalFrame, WalState,
};
pub use wal_storage::{
    LocalWalStorage, NoopWalStorage, PendingFrame, WalSegmentHeader, WalSegmentKey,
    WalSegmentReader, WalStorageBackend, WalStorageError, WalStorageResult, WalWriter,
    WalWriterConfig, WalWriterStats, DEFAULT_FLUSH_INTERVAL, DEFAULT_MAX_FRAMES_PER_SEGMENT,
    DEFAULT_MAX_SEGMENT_SIZE, WAL_SEGMENT_MAGIC, WAL_SEGMENT_VERSION,
};
pub use async_io::{AsyncChunkStorage, AsyncPageLoader, AsyncWalContext, SyncAsyncBridge};
pub use wal_stream::{
    BatchHeader, GapBuffer, StreamPosition, decode_batch, decode_header, encode_batch,
    BATCH_HEADER_SIZE, MAX_FRAMES_PER_BATCH,
};

#[cfg(feature = "rusqlite")]
pub use database::Connection;
