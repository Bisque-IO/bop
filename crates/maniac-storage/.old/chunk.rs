//! Unified chunk layout definitions shared by data and WAL artifacts.
use crate::directory::DirectoryValue;
use std::fmt;

/// Magic prefix written at the beginning of every chunk header ("BOPC").
pub const CHUNK_MAGIC: u32 = 0x4250_5043;

/// Version of the chunk header layout understood by this crate.
pub const CHUNK_VERSION: u16 = 1;

/// Encoded length of the base super-frame payload.
pub const BASE_SUPER_FRAME_BYTES: u32 = 24;

/// Encoded length of the libsql super-frame payload.
pub const LIBSQL_SUPER_FRAME_BYTES: u32 = 48;

/// Enumerates how frames inside a chunk should be interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalMode {
    /// Standard maniac-storage frames: roaring bitmap metadata + raw pages.
    Standard,
    /// Frames formatted for libsql-compatible VFS semantics.
    Libsql,
}

impl WalMode {
    pub fn from_u16(value: u16) -> Result<Self, ChunkParseError> {
        match value {
            0 => Ok(Self::Standard),
            1 => Ok(Self::Libsql),
            other => Err(ChunkParseError::UnknownWalMode(other)),
        }
    }

    pub fn to_u16(self) -> u16 {
        match self {
            Self::Standard => 0,
            Self::Libsql => 1,
        }
    }
}

/// Header that prefixes every chunk persisted to disk or S3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkHeader {
    pub version: u16,
    pub mode: WalMode,
    pub slab_count: u16,
    pub slab_size: u32,
    pub sequence: u64,
    pub generation: u64,
    pub super_frame_bytes: u32,
}

impl ChunkHeader {
    const RAW_SIZE: usize = 4  /* magic */
        + 2 /* version */
        + 2 /* wal mode */
        + 2 /* slab count */
        + 2 /* padding */
        + 4 /* slab size */
        + 8 /* sequence */
        + 8 /* generation */
        + 4 /* super frame len */
        + 4 /* reserved */;

    pub fn encode(&self) -> [u8; Self::RAW_SIZE] {
        let mut buf = [0u8; Self::RAW_SIZE];
        buf[0..4].copy_from_slice(&CHUNK_MAGIC.to_le_bytes());
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..8].copy_from_slice(&self.mode.to_u16().to_le_bytes());
        buf[8..10].copy_from_slice(&self.slab_count.to_le_bytes());
        buf[10..12].copy_from_slice(&0u16.to_le_bytes());
        buf[12..16].copy_from_slice(&self.slab_size.to_le_bytes());
        buf[16..24].copy_from_slice(&self.sequence.to_le_bytes());
        buf[24..32].copy_from_slice(&self.generation.to_le_bytes());
        buf[32..36].copy_from_slice(&self.super_frame_bytes.to_le_bytes());
        buf[36..40].copy_from_slice(&0u32.to_le_bytes());
        buf
    }

    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), ChunkParseError> {
        if bytes.len() < Self::RAW_SIZE {
            return Err(ChunkParseError::UnexpectedEof);
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic != CHUNK_MAGIC {
            return Err(ChunkParseError::BadMagic(magic));
        }
        let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        if version != CHUNK_VERSION {
            return Err(ChunkParseError::UnsupportedVersion(version));
        }
        let mode = WalMode::from_u16(u16::from_le_bytes(bytes[6..8].try_into().unwrap()))?;
        let slab_count = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
        let slab_size = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let sequence = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        let generation = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
        let super_frame_bytes = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
        Ok((
            Self {
                version,
                mode,
                slab_count,
                slab_size,
                sequence,
                generation,
                super_frame_bytes,
            },
            Self::RAW_SIZE,
        ))
    }

    /// Returns the encoded length of the header in bytes.
    pub const fn encoded_len() -> usize {
        Self::RAW_SIZE
    }
}

/// Common metadata captured in every super-frame regardless of WAL mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseSuperFrame {
    pub snapshot_generation: u64,
    pub wal_sequence: u64,
    pub bitmap_hash: u64,
}

/// Additional metadata required for libsql WAL replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibsqlSuperFrame {
    pub base: BaseSuperFrame,
    pub db_page_count: u32,
    pub change_counter: u32,
    pub salt: [u32; 2],
    pub schema_cookie: u32,
    pub mx_frame: u32,
}

/// Super-frame variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuperFrame {
    Base(BaseSuperFrame),
    Libsql(LibsqlSuperFrame),
}

impl SuperFrame {
    pub fn decode(mode: WalMode, bytes: &[u8]) -> Result<(Option<Self>, usize), ChunkParseError> {
        if bytes.is_empty() {
            return Ok((None, 0));
        }
        if bytes.len() < BASE_SUPER_FRAME_BYTES as usize {
            return Err(ChunkParseError::UnexpectedEof);
        }
        let base = BaseSuperFrame {
            snapshot_generation: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            wal_sequence: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            bitmap_hash: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        };
        match mode {
            WalMode::Standard => Ok((
                Some(SuperFrame::Base(base)),
                BASE_SUPER_FRAME_BYTES as usize,
            )),
            WalMode::Libsql => {
                if bytes.len() < LIBSQL_SUPER_FRAME_BYTES as usize {
                    return Err(ChunkParseError::UnexpectedEof);
                }
                let db_page_count = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
                let change_counter = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
                let salt0 = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
                let salt1 = u32::from_le_bytes(bytes[36..40].try_into().unwrap());
                let schema_cookie = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
                let mx_frame = u32::from_le_bytes(bytes[44..48].try_into().unwrap());
                let frame = LibsqlSuperFrame {
                    base,
                    db_page_count,
                    change_counter,
                    salt: [salt0, salt1],
                    schema_cookie,
                    mx_frame,
                };
                Ok((
                    Some(SuperFrame::Libsql(frame)),
                    LIBSQL_SUPER_FRAME_BYTES as usize,
                ))
            }
        }
    }
}

/// Frame header describing logical page metadata stored within a slab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameHeader {
    Standard(StandardFrameHeader),
    Libsql(LibsqlFrameHeader),
}

/// Raft metadata tracked for every WAL frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaftFrameMeta {
    pub term: u64,
    pub index: u64,
}

/// Standard maniac-storage frame metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardFrameHeader {
    pub page_id: u64,
    pub directory_value: DirectoryValue,
    pub payload_len: u32,
    pub checksum: u32,
    pub raft: RaftFrameMeta,
}

impl StandardFrameHeader {
    pub const ENCODED_LEN: usize = 8 + 8 + 4 + 4 + 8 + 8;

    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut buf = [0u8; Self::ENCODED_LEN];
        buf[0..8].copy_from_slice(&self.page_id.to_le_bytes());
        buf[8..16].copy_from_slice(&self.directory_value.to_packed().to_le_bytes());
        buf[16..20].copy_from_slice(&self.payload_len.to_le_bytes());
        buf[20..24].copy_from_slice(&self.checksum.to_le_bytes());
        buf[24..32].copy_from_slice(&self.raft.term.to_le_bytes());
        buf[32..40].copy_from_slice(&self.raft.index.to_le_bytes());
        buf
    }
}

/// libsql-compatible frame metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibsqlFrameHeader {
    pub page_no: u32,
    pub directory_value: DirectoryValue,
    pub db_size_after_frame: u32,
    pub salt: [u32; 2],
    pub frame_checksum: [u32; 2],
    pub payload_len: u32,
    pub raft: RaftFrameMeta,
}

impl LibsqlFrameHeader {
    pub const ENCODED_LEN: usize = 4  /* page_no */
        + 4 /* db_size */
        + 8 /* directory value */
        + 4 /* salt0 */
        + 4 /* salt1 */
        + 4 /* checksum0 */
        + 4 /* checksum1 */
        + 4 /* payload len */
        + 4 /* reserved */
        + 8 /* raft term */
        + 8 /* raft index */;

    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut buf = [0u8; Self::ENCODED_LEN];
        buf[0..4].copy_from_slice(&self.page_no.to_le_bytes());
        buf[4..8].copy_from_slice(&self.db_size_after_frame.to_le_bytes());
        buf[8..16].copy_from_slice(&self.directory_value.to_packed().to_le_bytes());
        buf[16..20].copy_from_slice(&self.salt[0].to_le_bytes());
        buf[20..24].copy_from_slice(&self.salt[1].to_le_bytes());
        buf[24..28].copy_from_slice(&self.frame_checksum[0].to_le_bytes());
        buf[28..32].copy_from_slice(&self.frame_checksum[1].to_le_bytes());
        buf[32..36].copy_from_slice(&self.payload_len.to_le_bytes());
        buf[36..40].fill(0);
        buf[40..48].copy_from_slice(&self.raft.term.to_le_bytes());
        buf[48..56].copy_from_slice(&self.raft.index.to_le_bytes());
        buf
    }
}

/// Lightweight descriptor emitted by the chunk parser per frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameDescriptor {
    pub header: FrameHeader,
    pub payload_offset: u64,
    pub payload_len: u32,
}

/// Result of parsing a chunk buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkDescriptor {
    pub header: ChunkHeader,
    pub super_frame: Option<SuperFrame>,
    pub frames: Vec<FrameDescriptor>,
}

/// Errors emitted while parsing chunk artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkParseError {
    BadMagic(u32),
    UnsupportedVersion(u16),
    UnknownWalMode(u16),
    UnexpectedEof,
    InvalidDirectoryValue,
}

impl fmt::Display for ChunkParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic(magic) => write!(f, "chunk header had unexpected magic 0x{magic:08x}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "chunk version {version} is not supported")
            }
            Self::UnknownWalMode(mode) => write!(f, "chunk referenced unknown wal mode {mode}"),
            Self::UnexpectedEof => write!(f, "chunk data ended unexpectedly"),
            Self::InvalidDirectoryValue => {
                write!(f, "chunk frame carried invalid directory metadata")
            }
        }
    }
}

impl std::error::Error for ChunkParseError {}

/// Prototype parser that walks a chunk buffer and emits decoded frame descriptors.
pub fn parse_chunk(bytes: &[u8]) -> Result<ChunkDescriptor, ChunkParseError> {
    let (header, mut offset) = ChunkHeader::decode(bytes)?;
    let super_frame_limit = offset + header.super_frame_bytes as usize;
    let (super_frame, _) = if header.super_frame_bytes == 0 {
        (None, 0usize)
    } else {
        let available = bytes
            .get(offset..super_frame_limit)
            .ok_or(ChunkParseError::UnexpectedEof)?;
        SuperFrame::decode(header.mode, available)?
    };
    offset = super_frame_limit;
    let mut frames = Vec::new();
    while offset < bytes.len() {
        let remaining = &bytes[offset..];
        if remaining.iter().all(|&b| b == 0) {
            break;
        }
        if remaining.len() < 16 {
            return Err(ChunkParseError::UnexpectedEof);
        }
        match header.mode {
            WalMode::Standard => {
                if remaining.len() < 40 {
                    return Err(ChunkParseError::UnexpectedEof);
                }
                let page_id = u64::from_le_bytes(remaining[0..8].try_into().unwrap());
                let dir = DirectoryValue::from_packed(u64::from_le_bytes(
                    remaining[8..16].try_into().unwrap(),
                ));
                let payload_len = u32::from_le_bytes(remaining[16..20].try_into().unwrap());
                let checksum = u32::from_le_bytes(remaining[20..24].try_into().unwrap());
                let raft_term = u64::from_le_bytes(remaining[24..32].try_into().unwrap());
                let raft_index = u64::from_le_bytes(remaining[32..40].try_into().unwrap());
                let payload_offset = (offset + 40) as u64;
                frames.push(FrameDescriptor {
                    header: FrameHeader::Standard(StandardFrameHeader {
                        page_id,
                        directory_value: dir,
                        payload_len,
                        checksum,
                        raft: RaftFrameMeta {
                            term: raft_term,
                            index: raft_index,
                        },
                    }),
                    payload_offset,
                    payload_len,
                });
                offset += 40 + payload_len as usize;
            }
            WalMode::Libsql => {
                if remaining.len() < 56 {
                    return Err(ChunkParseError::UnexpectedEof);
                }
                let page_no = u32::from_le_bytes(remaining[0..4].try_into().unwrap());
                let db_size_after_frame = u32::from_le_bytes(remaining[4..8].try_into().unwrap());
                let dir = DirectoryValue::from_packed(u64::from_le_bytes(
                    remaining[8..16].try_into().unwrap(),
                ));
                let salt0 = u32::from_le_bytes(remaining[16..20].try_into().unwrap());
                let salt1 = u32::from_le_bytes(remaining[20..24].try_into().unwrap());
                let checksum0 = u32::from_le_bytes(remaining[24..28].try_into().unwrap());
                let checksum1 = u32::from_le_bytes(remaining[28..32].try_into().unwrap());
                let payload_len = u32::from_le_bytes(remaining[32..36].try_into().unwrap());
                let reserved = u32::from_le_bytes(remaining[36..40].try_into().unwrap());
                let _ = reserved; // reserved for future fields
                let raft_term = u64::from_le_bytes(remaining[40..48].try_into().unwrap());
                let raft_index = u64::from_le_bytes(remaining[48..56].try_into().unwrap());
                let payload_offset = (offset + 56) as u64;
                frames.push(FrameDescriptor {
                    header: FrameHeader::Libsql(LibsqlFrameHeader {
                        page_no,
                        directory_value: dir,
                        db_size_after_frame,
                        salt: [salt0, salt1],
                        frame_checksum: [checksum0, checksum1],
                        payload_len,
                        raft: RaftFrameMeta {
                            term: raft_term,
                            index: raft_index,
                        },
                    }),
                    payload_offset,
                    payload_len,
                });
                offset += 56 + payload_len as usize;
            }
        }
    }
    Ok(ChunkDescriptor {
        header,
        super_frame,
        frames,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_directory_value() -> DirectoryValue {
        DirectoryValue::new(4096, 0xdead_beef, true, 0).unwrap()
    }

    #[test]
    fn parse_standard_chunk_with_single_frame() {
        let header = ChunkHeader {
            version: CHUNK_VERSION,
            mode: WalMode::Standard,
            slab_count: 8,
            slab_size: 1 << 20,
            sequence: 7,
            generation: 3,
            super_frame_bytes: 24,
        };
        let mut buf = Vec::new();
        buf.extend_from_slice(&header.encode());
        let base = BaseSuperFrame {
            snapshot_generation: 10,
            wal_sequence: 11,
            bitmap_hash: 12,
        };
        buf.extend_from_slice(&base.snapshot_generation.to_le_bytes());
        buf.extend_from_slice(&base.wal_sequence.to_le_bytes());
        buf.extend_from_slice(&base.bitmap_hash.to_le_bytes());
        let frame_offset = buf.len();
        buf.extend_from_slice(&42u64.to_le_bytes());
        buf.extend_from_slice(&fake_directory_value().to_packed().to_le_bytes());
        buf.extend_from_slice(&4096u32.to_le_bytes());
        buf.extend_from_slice(&0xabad1dea_u32.to_le_bytes());
        buf.extend_from_slice(&5u64.to_le_bytes());
        buf.extend_from_slice(&6u64.to_le_bytes());
        buf.extend_from_slice(&vec![0u8; 4096]);
        let descriptor = parse_chunk(&buf).expect("parse should succeed");
        assert_eq!(descriptor.header.mode, WalMode::Standard);
        assert_eq!(descriptor.frames.len(), 1);
        let frame = &descriptor.frames[0];
        assert_eq!(frame.payload_offset as usize, frame_offset + 40);
        assert_eq!(frame.payload_len, 4096);
        match &frame.header {
            FrameHeader::Standard(header) => {
                assert_eq!(header.page_id, 42);
                assert_eq!(header.payload_len, 4096);
                assert_eq!(header.directory_value, fake_directory_value());
                assert_eq!(header.raft.term, 5);
                assert_eq!(header.raft.index, 6);
            }
            _ => panic!("expected standard header"),
        }
    }

    #[test]
    fn parse_libsql_chunk_with_super_frame() {
        let header = ChunkHeader {
            version: CHUNK_VERSION,
            mode: WalMode::Libsql,
            slab_count: 8,
            slab_size: 1 << 20,
            sequence: 9,
            generation: 2,
            super_frame_bytes: 48,
        };
        let mut buf = Vec::new();
        buf.extend_from_slice(&header.encode());
        let base = BaseSuperFrame {
            snapshot_generation: 100,
            wal_sequence: 101,
            bitmap_hash: 102,
        };
        buf.extend_from_slice(&base.snapshot_generation.to_le_bytes());
        buf.extend_from_slice(&base.wal_sequence.to_le_bytes());
        buf.extend_from_slice(&base.bitmap_hash.to_le_bytes());
        buf.extend_from_slice(&123u32.to_le_bytes());
        buf.extend_from_slice(&999u32.to_le_bytes());
        buf.extend_from_slice(&0x1111_2222u32.to_le_bytes());
        buf.extend_from_slice(&0x3333_4444u32.to_le_bytes());
        buf.extend_from_slice(&0x5555_6666u32.to_le_bytes());
        buf.extend_from_slice(&0x7777_8888u32.to_le_bytes());
        buf.extend_from_slice(&17u32.to_le_bytes());
        buf.extend_from_slice(&0xaaaa_bccdu32.to_le_bytes());
        buf.extend_from_slice(&fake_directory_value().to_packed().to_le_bytes());
        buf.extend_from_slice(&0x1111_2222u32.to_le_bytes());
        buf.extend_from_slice(&0x3333_4444u32.to_le_bytes());
        buf.extend_from_slice(&0x5555_6666u32.to_le_bytes());
        buf.extend_from_slice(&0x7777_8888u32.to_le_bytes());
        buf.extend_from_slice(&4096u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&9u64.to_le_bytes());
        buf.extend_from_slice(&10u64.to_le_bytes());
        buf.extend_from_slice(&vec![0u8; 4096]);
        let descriptor = parse_chunk(&buf).expect("parse should succeed");
        assert_eq!(descriptor.header.mode, WalMode::Libsql);
        match descriptor.super_frame {
            Some(SuperFrame::Libsql(ref meta)) => {
                assert_eq!(meta.base.snapshot_generation, 100);
                assert_eq!(meta.db_page_count, 123);
            }
            _ => panic!("expected libsql super frame"),
        }
        assert_eq!(descriptor.frames.len(), 1);
        match &descriptor.frames[0].header {
            FrameHeader::Libsql(header) => {
                assert_eq!(header.page_no, 17);
                assert_eq!(header.directory_value, fake_directory_value());
                assert_eq!(header.db_size_after_frame, 0xaaaa_bccd);
                assert_eq!(header.raft.term, 9);
                assert_eq!(header.raft.index, 10);
            }
            _ => panic!("expected libsql header"),
        }
    }
}
