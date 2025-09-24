//! Manifest system for durable metadata tracking.
//!
//! The manifest system provides append-only durability for critical metadata
//! changes across the tiered storage system. It records segment lifecycle
//! events, tier transitions, and storage operations in a write-ahead log.
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │                     Manifest System                │
//! ├────────────────────────────────────────────────────┤
//! │ ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
//! │ │   Record    │  │    Chunk    │  │   Writer    │  │
//! │ │   Types     │  │  Management │  │  Batching   │  │
//! │ └─────────────┘  └─────────────┘  └─────────────┘  │
//! │                              │                     │
//! │ ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
//! │ │   Reader    │  │  Inspector  │  │ Durability  │  │
//! │ │  Playback   │  │    Tools    │  │ Guarantees  │  │
//! │ └─────────────┘  └─────────────┘  └─────────────┘  │
//! └────────────────────────────────────────────────────┘
//! ```
//!
//! ## Key Components
//!
//! - **Records**: Structured metadata events (segment creation, sealing, etc.)
//! - **Chunks**: Batched record groups with integrity verification
//! - **Writer**: Atomic chunk writing with durability guarantees
//! - **Reader**: Efficient record replay and recovery
//! - **Inspector**: Debugging and verification tools
//!
//! ## Durability Model
//!
//! The manifest uses a write-ahead log approach:
//! 1. Records are batched into chunks for efficiency
//! 2. Chunks are written atomically with checksums
//! 3. Fsync ensures durability before operations complete
//! 4. Recovery replays the manifest to reconstruct state
//!
//! ## Usage Patterns
//!
//! ```ignore
//! // Writing manifest entries
//! let writer = ManifestLogWriter::new(config)?;
//! let chunk = writer.chunk_handle();
//! chunk.append_record(record)?;
//! chunk.seal()?; // Atomic durability
//!
//! // Reading during recovery
//! let reader = ManifestLogReader::open(path)?;
//! for record in reader.records() {
//!     process_recovery_record(record?)?;
//! }
//! ```

mod chunk;
mod inspect;
mod reader;
mod record;
mod writer;

pub use chunk::{CHUNK_HEADER_LEN, ChunkHeader};
pub use inspect::{ChunkSummary, ManifestInspection, ManifestInspector};
pub use reader::{ManifestLogReader, ManifestRecordIter};
pub use record::{
    ManifestRecord, ManifestRecordPayload, RECORD_HEADER_LEN, RecordType, SEAL_RESERVED_BYTES,
};
pub use writer::{ChunkHandle, ManifestLogWriter, ManifestLogWriterConfig, SealedChunkHandle};
