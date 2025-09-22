//! Manifest subsystem for per-segment metadata and inspection.
//!
//! This module coordinates reading/writing of per-segment manifests and exposes
//! helpers to inspect and replay metadata for recovery. It is primarily used by
//! the tier1 cache and not typically required by application code.
mod chunk;
mod inspect;
mod reader;
mod record;
mod writer;

pub use chunk::{CHUNK_HEADER_LEN, ChunkHeader};
pub use inspect::{ChunkSummary, ManifestInspection, ManifestInspector};
pub use reader::{ManifestLogReader, ManifestRecordIter};
pub use record::{ManifestRecord, ManifestRecordPayload, RECORD_HEADER_LEN, RecordType};
pub use writer::{ChunkHandle, ManifestLogWriter, ManifestLogWriterConfig, SealedChunkHandle};
