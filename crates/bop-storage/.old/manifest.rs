use crc64fast_nvme::Digest;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::{self, Read, Write};

const MANIFEST_MAGIC: u32 = 0x424F_504D; // "BOPM"
const MANIFEST_VERSION: u16 = 1;

/// Logical identifier for an immutable chunk produced by a checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChunkId(pub u32);

impl ChunkId {
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Logical namespace for chunk artifacts on disk or in object storage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ManifestPath(String);

impl ManifestPath {
    pub fn new<P: Into<String>>(path: P) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Represents a namespace for manifests and superblocks (e.g. forks).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ManifestNamespace {
    components: Vec<String>,
}

impl ManifestNamespace {
    pub fn root() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    pub fn forks(id: impl Into<String>) -> Self {
        Self {
            components: vec!["forks".into(), id.into()],
        }
    }

    pub fn child(&self, name: impl Into<String>) -> Self {
        let mut components = self.components.clone();
        components.push(name.into());
        Self { components }
    }

    pub fn manifest_path(&self) -> String {
        self.join_with("manifest.log")
    }

    pub fn superblock_path(&self) -> String {
        self.join_with("superblock.bin")
    }

    pub fn to_path(&self) -> String {
        if self.components.is_empty() {
            ".".into()
        } else {
            self.components.join("/")
        }
    }

    fn join_with(&self, tail: &str) -> String {
        if self.components.is_empty() {
            tail.to_string()
        } else {
            format!("{}/{}", self.components.join("/"), tail)
        }
    }
}
/// Metadata captured for every chunk generation recorded in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// Generation identifier embedded in the chunk super-frame.
    pub chunk_generation: u64,
    /// Highest WAL sequence incorporated into the chunk.
    pub wal_sequence: u64,
    /// Hash of the roaring bitmap snapshot used when building the chunk.
    pub bitmap_hash: u64,
    /// Number of logical pages stored inside the chunk.
    pub page_count: u32,
    /// Total payload bytes persisted for the chunk.
    pub size_bytes: u64,
    /// Location for the chunk relative to the manifest namespace root.
    pub path: ManifestPath,
}

impl ChunkMetadata {
    pub fn with_wal_sequence(mut self, sequence: u64) -> Self {
        self.wal_sequence = sequence;
        self
    }
}

/// Append-only change captured in the manifest delta log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManifestDelta {
    /// Inserts or replaces metadata for `chunk_id`.
    UpsertChunk {
        chunk_id: ChunkId,
        chunk: ChunkMetadata,
    },
    /// Removes the chunk when the retained generation is at most `generation`.
    RemoveChunk { chunk_id: ChunkId, generation: u64 },
    /// Marks the completion of a checkpoint for `manifest_generation`.
    SealGeneration,
}

/// Record stored in the manifest delta log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestRecord {
    pub manifest_generation: u64,
    pub wal_sequence: u64,
    pub delta: ManifestDelta,
}

impl ManifestRecord {
    pub fn new(manifest_generation: u64, wal_sequence: u64, delta: ManifestDelta) -> Self {
        Self {
            manifest_generation,
            wal_sequence,
            delta,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("unexpected magic {magic:#x}")]
    BadMagic { magic: u32 },
    #[error("unsupported version {version}")]
    UnsupportedVersion { version: u16 },
    #[error("payload checksum mismatch expected {expected:#x} got {actual:#x}")]
    Checksum { expected: u64, actual: u64 },
    #[error("manifest record truncated")]
    TruncatedRecord,
}

impl From<bincode::Error> for ManifestError {
    fn from(err: bincode::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

struct RecordHeader {
    manifest_generation: u64,
    wal_sequence: u64,
    payload_len: u32,
    checksum: u64,
}

impl RecordHeader {
    const ENCODED_LEN: usize = 4 + 2 + 2 + 8 + 8 + 4 + 8;

    fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut buf = [0u8; Self::ENCODED_LEN];
        buf[0..4].copy_from_slice(&MANIFEST_MAGIC.to_le_bytes());
        buf[4..6].copy_from_slice(&MANIFEST_VERSION.to_le_bytes());
        buf[6..8].fill(0);
        buf[8..16].copy_from_slice(&self.manifest_generation.to_le_bytes());
        buf[16..24].copy_from_slice(&self.wal_sequence.to_le_bytes());
        buf[24..28].copy_from_slice(&self.payload_len.to_le_bytes());
        buf[28..36].copy_from_slice(&self.checksum.to_le_bytes());
        buf
    }

    fn decode(bytes: &[u8]) -> Result<Self, ManifestError> {
        if bytes.len() < Self::ENCODED_LEN {
            return Err(ManifestError::TruncatedRecord);
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic != MANIFEST_MAGIC {
            return Err(ManifestError::BadMagic { magic });
        }
        let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        if version != MANIFEST_VERSION {
            return Err(ManifestError::UnsupportedVersion { version });
        }
        Ok(Self {
            manifest_generation: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            wal_sequence: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            payload_len: u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            checksum: u64::from_le_bytes(bytes[28..36].try_into().unwrap()),
        })
    }
}

/// Append-only manifest writer.
#[derive(Debug)]
pub struct ManifestLogWriter<W> {
    writer: W,
}

impl<W: Write> ManifestLogWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn append(&mut self, record: &ManifestRecord) -> Result<(), ManifestError> {
        let payload = bincode::serialize(&record.delta)?;
        let mut hasher = Digest::new();
        hasher.write(&payload);
        let checksum = hasher.sum64();
        let header = RecordHeader {
            manifest_generation: record.manifest_generation,
            wal_sequence: record.wal_sequence,
            payload_len: payload.len() as u32,
            checksum,
        };
        self.writer.write_all(&header.encode())?;
        self.writer.write_all(&payload)?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), ManifestError> {
        self.writer.flush()?;
        Ok(())
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

/// Streaming reader for manifest delta logs.
#[derive(Debug)]
pub struct ManifestLogReader<R> {
    reader: R,
    eof: bool,
}

impl<R: Read> ManifestLogReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, eof: false }
    }

    pub fn next(&mut self) -> Result<Option<ManifestRecord>, ManifestError> {
        if self.eof {
            return Ok(None);
        }
        let mut header_buf = [0u8; RecordHeader::ENCODED_LEN];
        match Self::fill_buffer(&mut self.reader, &mut header_buf) {
            Ok(()) => {}
            Err(FillError::Eof { filled: 0 }) => {
                self.eof = true;
                return Ok(None);
            }
            Err(FillError::Eof { .. }) => return Err(ManifestError::TruncatedRecord),
            Err(FillError::Io(err)) => return Err(ManifestError::Io(err)),
        }
        let header = RecordHeader::decode(&header_buf)?;
        let mut payload = vec![0u8; header.payload_len as usize];
        if header.payload_len > 0 {
            match Self::fill_buffer(&mut self.reader, &mut payload) {
                Ok(()) => {}
                Err(FillError::Eof { .. }) => return Err(ManifestError::TruncatedRecord),
                Err(FillError::Io(err)) => return Err(ManifestError::Io(err)),
            }
        }
        let mut hasher = Digest::new();
        hasher.write(&payload);
        let checksum = hasher.sum64();
        if checksum != header.checksum {
            return Err(ManifestError::Checksum {
                expected: header.checksum,
                actual: checksum,
            });
        }
        let delta: ManifestDelta = bincode::deserialize(&payload)?;
        Ok(Some(ManifestRecord {
            manifest_generation: header.manifest_generation,
            wal_sequence: header.wal_sequence,
            delta,
        }))
    }

    fn fill_buffer(reader: &mut R, buf: &mut [u8]) -> Result<(), FillError> {
        let mut offset = 0;
        while offset < buf.len() {
            match reader.read(&mut buf[offset..]) {
                Ok(0) => return Err(FillError::Eof { filled: offset }),
                Ok(n) => offset += n,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(FillError::Io(err)),
            }
        }
        Ok(())
    }
}

enum FillError {
    Io(io::Error),
    Eof { filled: usize },
}

/// Materialised manifest state produced by replaying the delta log.
#[derive(Debug, Default, Clone)]
pub struct ManifestState {
    manifest_generation: u64,
    wal_sequence: u64,
    chunks: BTreeMap<ChunkId, ChunkMetadata>,
    ref_counts: ChunkRefCounter,
}

impl ManifestState {
    pub fn manifest_generation(&self) -> u64 {
        self.manifest_generation
    }

    pub fn wal_sequence(&self) -> u64 {
        self.wal_sequence
    }

    pub fn chunks(&self) -> &BTreeMap<ChunkId, ChunkMetadata> {
        &self.chunks
    }

    pub fn chunk(&self, id: ChunkId) -> Option<&ChunkMetadata> {
        self.chunks.get(&id)
    }

    pub fn apply_record(&mut self, record: ManifestRecord) {
        match record.delta {
            ManifestDelta::UpsertChunk {
                chunk_id,
                mut chunk,
            } => {
                chunk.wal_sequence = record.wal_sequence.max(chunk.wal_sequence);
                let new_generation = chunk.chunk_generation;
                if let Some(previous) = self.chunks.insert(chunk_id, chunk) {
                    self.ref_counts
                        .decrement(chunk_id, previous.chunk_generation);
                }
                self.ref_counts.increment(chunk_id, new_generation);
            }
            ManifestDelta::RemoveChunk {
                chunk_id,
                generation,
            } => {
                if let Some(existing) = self.chunks.get(&chunk_id) {
                    if existing.chunk_generation <= generation {
                        if let Some(removed) = self.chunks.remove(&chunk_id) {
                            self.ref_counts
                                .decrement(chunk_id, removed.chunk_generation);
                        }
                    }
                }
            }
            ManifestDelta::SealGeneration => {
                self.manifest_generation = self.manifest_generation.max(record.manifest_generation);
                self.wal_sequence = self.wal_sequence.max(record.wal_sequence);
                return;
            }
        }
        self.manifest_generation = self.manifest_generation.max(record.manifest_generation);
        self.wal_sequence = self.wal_sequence.max(record.wal_sequence);
    }

    pub fn chunk_ref_counts(&self) -> &ChunkRefCounter {
        &self.ref_counts
    }
}

#[derive(Debug, Default, Clone)]
pub struct ChunkRefCounter {
    counts: HashMap<(ChunkId, u64), usize>,
}

impl ChunkRefCounter {
    pub fn increment(&mut self, chunk_id: ChunkId, generation: u64) {
        *self.counts.entry((chunk_id, generation)).or_insert(0) += 1;
    }

    pub fn decrement(&mut self, chunk_id: ChunkId, generation: u64) {
        if let Some(count) = self.counts.get_mut(&(chunk_id, generation)) {
            if *count > 0 {
                *count -= 1;
            }
            if *count == 0 {
                self.counts.remove(&(chunk_id, generation));
            }
        }
    }

    pub fn get(&self, chunk_id: ChunkId, generation: u64) -> usize {
        self.counts
            .get(&(chunk_id, generation))
            .copied()
            .unwrap_or(0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&(ChunkId, u64), &usize)> {
        self.counts.iter()
    }
}

pub fn replay_manifest<R: Read>(
    mut reader: ManifestLogReader<R>,
) -> Result<ManifestState, ManifestError> {
    let mut state = ManifestState::default();
    loop {
        match reader.next()? {
            Some(record) => state.apply_record(record),
            None => break,
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk_metadata(path: &str, generation: u64, wal: u64) -> ChunkMetadata {
        ChunkMetadata {
            chunk_generation: generation,
            wal_sequence: wal,
            bitmap_hash: 0xDEADBEEF,
            page_count: 128,
            size_bytes: 4096 * 10,
            path: ManifestPath::new(path),
        }
    }

    #[test]
    fn replay_empty_manifest_is_default() {
        let reader = ManifestLogReader::new(std::io::Cursor::new(Vec::new()));
        let state = replay_manifest(reader).expect("replay");
        assert_eq!(state.manifest_generation(), 0);
        assert_eq!(state.wal_sequence(), 0);
        assert!(state.chunks().is_empty());
    }

    #[test]
    fn manifest_record_roundtrip() {
        let record = ManifestRecord::new(
            3,
            42,
            ManifestDelta::UpsertChunk {
                chunk_id: ChunkId(7),
                chunk: chunk_metadata("chunks/0007.chk", 3, 41),
            },
        );
        let mut buf = Vec::new();
        {
            let mut writer = ManifestLogWriter::new(&mut buf);
            writer.append(&record).expect("append");
            writer.flush().expect("flush");
        }
        let mut reader = ManifestLogReader::new(std::io::Cursor::new(buf));
        let decoded = reader.next().expect("read").expect("record");
        assert_eq!(decoded.manifest_generation, record.manifest_generation);
        assert_eq!(decoded.wal_sequence, record.wal_sequence);
        assert_eq!(decoded.delta, record.delta);
    }

    #[test]
    fn manifest_replay_applies_deltas() {
        let mut buf = Vec::new();
        {
            let mut writer = ManifestLogWriter::new(&mut buf);
            writer
                .append(&ManifestRecord::new(
                    1,
                    10,
                    ManifestDelta::UpsertChunk {
                        chunk_id: ChunkId(0),
                        chunk: chunk_metadata("chunks/0000.chk", 1, 10),
                    },
                ))
                .unwrap();
            writer
                .append(&ManifestRecord::new(
                    2,
                    20,
                    ManifestDelta::UpsertChunk {
                        chunk_id: ChunkId(1),
                        chunk: chunk_metadata("chunks/0001.chk", 2, 20),
                    },
                ))
                .unwrap();
            writer
                .append(&ManifestRecord::new(
                    2,
                    20,
                    ManifestDelta::RemoveChunk {
                        chunk_id: ChunkId(0),
                        generation: 2,
                    },
                ))
                .unwrap();
            writer
                .append(&ManifestRecord::new(2, 20, ManifestDelta::SealGeneration))
                .unwrap();
            writer.flush().unwrap();
        }
        let reader = ManifestLogReader::new(std::io::Cursor::new(buf));
        let state = replay_manifest(reader).expect("replay");
        assert_eq!(state.manifest_generation(), 2);
        assert_eq!(state.wal_sequence(), 20);
        assert!(state.chunk(ChunkId(0)).is_none());
        let chunk = state.chunk(ChunkId(1)).expect("chunk 1");
        assert_eq!(chunk.chunk_generation, 2);
        assert_eq!(chunk.wal_sequence, 20);
        assert_eq!(chunk.path.as_str(), "chunks/0001.chk");
    }

    #[test]
    fn ref_counts_track_generations() {
        let mut state = ManifestState::default();
        state.apply_record(ManifestRecord::new(
            1,
            1,
            ManifestDelta::UpsertChunk {
                chunk_id: ChunkId(1),
                chunk: ChunkMetadata {
                    chunk_generation: 5,
                    wal_sequence: 1,
                    bitmap_hash: 0,
                    page_count: 1,
                    size_bytes: 4096,
                    path: ManifestPath::new("chunks/1"),
                },
            },
        ));
        state.apply_record(ManifestRecord::new(
            2,
            2,
            ManifestDelta::UpsertChunk {
                chunk_id: ChunkId(1),
                chunk: ChunkMetadata {
                    chunk_generation: 6,
                    wal_sequence: 2,
                    bitmap_hash: 0,
                    page_count: 1,
                    size_bytes: 4096,
                    path: ManifestPath::new("chunks/1"),
                },
            },
        ));
        assert_eq!(state.chunk_ref_counts().get(ChunkId(1), 5), 0);
        assert_eq!(state.chunk_ref_counts().get(ChunkId(1), 6), 1);
        state.apply_record(ManifestRecord::new(
            2,
            2,
            ManifestDelta::RemoveChunk {
                chunk_id: ChunkId(1),
                generation: 6,
            },
        ));
        assert_eq!(state.chunk_ref_counts().get(ChunkId(1), 6), 0);
    }

    #[test]
    fn namespace_paths_join_correctly() {
        let root = ManifestNamespace::root();
        assert_eq!(root.manifest_path(), "manifest.log");
        let fork = ManifestNamespace::forks("alpha");
        assert_eq!(fork.manifest_path(), "forks/alpha/manifest.log");
        let child = fork.child("replays");
        assert_eq!(
            child.superblock_path(),
            "forks/alpha/replays/superblock.bin"
        );
    }

    #[test]
    fn reader_detects_corruption() {
        let mut buf = Vec::new();
        {
            let mut writer = ManifestLogWriter::new(&mut buf);
            writer
                .append(&ManifestRecord::new(
                    1,
                    1,
                    ManifestDelta::UpsertChunk {
                        chunk_id: ChunkId(1),
                        chunk: chunk_metadata("chunks/0001.chk", 1, 1),
                    },
                ))
                .unwrap();
            writer.flush().unwrap();
        }
        // Flip one byte in the payload to trigger a checksum mismatch.
        if let Some(byte) = buf.last_mut() {
            *byte ^= 0xFF;
        }
        let mut reader = ManifestLogReader::new(std::io::Cursor::new(buf));
        let err = reader.next().expect_err("checksum mismatch");
        match err {
            ManifestError::Checksum { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
