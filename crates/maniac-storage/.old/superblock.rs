use crate::manifest::ManifestPath;
use crc64fast_nvme::Digest;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const SUPERBLOCK_MAGIC: u32 = 0x424F_5053; // "BOPS"
const SUPERBLOCK_VERSION: u16 = 1;
const SUPERBLOCK_HEADER_LEN: usize = 4 + 2 + 2 + 8 + 8 + 8 + 2; // magic + version + reserved + gen + wal + snapshot + path len

/// Persistent pointer to the active manifest and WAL checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Superblock {
    pub manifest_path: ManifestPath,
    pub manifest_generation: u64,
    pub wal_sequence: u64,
    pub snapshot_generation: u64,
}

impl Superblock {
    pub fn encode(&self) -> Result<Vec<u8>, SuperblockError> {
        let path_bytes = self.manifest_path.as_str().as_bytes();
        if path_bytes.len() > u16::MAX as usize {
            return Err(SuperblockError::PathTooLong {
                len: path_bytes.len(),
            });
        }
        let mut buf = Vec::with_capacity(SUPERBLOCK_HEADER_LEN + path_bytes.len() + 8);
        buf.extend_from_slice(&SUPERBLOCK_MAGIC.to_le_bytes());
        buf.extend_from_slice(&SUPERBLOCK_VERSION.to_le_bytes());
        buf.extend_from_slice(&[0u8; 2]);
        buf.extend_from_slice(&self.manifest_generation.to_le_bytes());
        buf.extend_from_slice(&self.wal_sequence.to_le_bytes());
        buf.extend_from_slice(&self.snapshot_generation.to_le_bytes());
        buf.extend_from_slice(&(path_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(path_bytes);
        let mut hasher = Digest::new();
        hasher.write(&buf);
        let checksum = hasher.sum64();
        buf.extend_from_slice(&checksum.to_le_bytes());
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SuperblockError> {
        if bytes.len() < SUPERBLOCK_HEADER_LEN + 8 {
            return Err(SuperblockError::Truncated);
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic != SUPERBLOCK_MAGIC {
            return Err(SuperblockError::BadMagic { magic });
        }
        let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        if version != SUPERBLOCK_VERSION {
            return Err(SuperblockError::UnsupportedVersion { version });
        }
        let manifest_generation = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let wal_sequence = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        let snapshot_generation = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
        let path_len = u16::from_le_bytes(bytes[32..34].try_into().unwrap()) as usize;
        let expected_len = SUPERBLOCK_HEADER_LEN + path_len + 8;
        if bytes.len() != expected_len {
            return Err(SuperblockError::Truncated);
        }
        let payload_end = SUPERBLOCK_HEADER_LEN + path_len;
        let checksum = u64::from_le_bytes(bytes[payload_end..payload_end + 8].try_into().unwrap());
        let mut hasher = Digest::new();
        hasher.write(&bytes[..payload_end]);
        let actual = hasher.sum64();
        if actual != checksum {
            return Err(SuperblockError::Checksum {
                expected: checksum,
                actual,
            });
        }
        let path_slice = &bytes[SUPERBLOCK_HEADER_LEN..payload_end];
        let path_str = std::str::from_utf8(path_slice)
            .map_err(|_| SuperblockError::InvalidUtf8)?
            .to_string();
        Ok(Self {
            manifest_path: ManifestPath::new(path_str),
            manifest_generation,
            wal_sequence,
            snapshot_generation,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SuperblockError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("unexpected superblock magic {magic:#x}")]
    BadMagic { magic: u32 },
    #[error("unsupported superblock version {version}")]
    UnsupportedVersion { version: u16 },
    #[error("superblock truncated")]
    Truncated,
    #[error("manifest path exceeds u16 limit: {len} bytes")]
    PathTooLong { len: usize },
    #[error("checksum mismatch expected {expected:#x} got {actual:#x}")]
    Checksum { expected: u64, actual: u64 },
    #[error("manifest path is not valid utf8")]
    InvalidUtf8,
}

/// Atomically swaps the active superblock once chunk flush and manifest persistence succeed.
#[derive(Debug, Clone)]
pub struct SuperblockWriter {
    path: PathBuf,
}

impl SuperblockWriter {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn commit(&self, superblock: &Superblock) -> Result<(), SuperblockError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let tmp_path = Self::tmp_path(&self.path);
        let bytes = superblock.encode()?;
        let commit_result = (|| -> Result<(), SuperblockError> {
            let mut file = File::create(&tmp_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&tmp_path, &self.path)?;
            Ok(())
        })();
        if commit_result.is_err() {
            let _ = fs::remove_file(&tmp_path);
        }
        commit_result
    }

    pub fn load(&self) -> Result<Superblock, SuperblockError> {
        let mut file = File::open(&self.path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Superblock::decode(&buf)
    }

    fn tmp_path(path: &Path) -> PathBuf {
        let mut tmp = path.to_path_buf();
        let ext = match path.extension().and_then(|s| s.to_str()) {
            Some(ext) => format!("{ext}.tmp"),
            None => String::from("sb.tmp"),
        };
        tmp.set_extension(ext);
        tmp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_superblock() -> Superblock {
        Superblock {
            manifest_path: ManifestPath::new("manifests/main.mlog"),
            manifest_generation: 7,
            wal_sequence: 42,
            snapshot_generation: 7,
        }
    }

    #[test]
    fn encode_decode_roundtrip() {
        let sb = sample_superblock();
        let bytes = sb.encode().expect("encode");
        let decoded = Superblock::decode(&bytes).expect("decode");
        assert_eq!(decoded, sb);
    }

    #[test]
    fn checksum_detects_corruption() {
        let sb = sample_superblock();
        let mut bytes = sb.encode().expect("encode");
        let last = bytes.len() - 9; // mutate payload, not checksum (leave crc64 intact)
        bytes[last] ^= 0xFF;
        let err = Superblock::decode(&bytes).expect_err("checksum mismatch");
        assert!(matches!(err, SuperblockError::Checksum { .. }));
    }

    #[test]
    fn writer_commits_atomically() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("superblock.bin");
        let writer = SuperblockWriter::new(&path);
        let sb = sample_superblock();
        writer.commit(&sb).expect("commit");
        let loaded = writer.load().expect("load");
        assert_eq!(loaded, sb);
        assert!(!SuperblockWriter::tmp_path(&path).exists());
    }
}
