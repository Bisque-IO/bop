use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

use super::config::{AofConfig, SegmentId};
use super::error::{AofError, AofResult};

/// Represents the canonical on-disk layout for an AOF instance.
#[derive(Debug, Clone)]
pub struct Layout {
    root: PathBuf,
    segments: PathBuf,
    warm: PathBuf,
    manifest: PathBuf,
    archive: PathBuf,
}

pub const SEGMENT_FILE_EXTENSION: &str = ".seg";
pub const WARM_FILE_EXTENSION: &str = ".zst";
const SEGMENT_FILE_BREAK: char = '_';
const SEGMENT_FILE_PAD: usize = 20;

const CURRENT_POINTER_MAGIC: u32 = 0x43505332; // "CPS2"
const CURRENT_POINTER_VERSION: u16 = 1;
const CURRENT_POINTER_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentSealedPointer {
    pub segment_id: SegmentId,
    pub coordinator_watermark: u64,
    pub durable_bytes: u64,
}

/// Parsed components of a segment filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentFileName {
    pub segment_id: SegmentId,
    pub base_offset: u64,
    pub created_at: i64,
}

impl SegmentFileName {
    pub fn format(segment_id: SegmentId, base_offset: u64, created_at: i64) -> String {
        format!(
            "{id:0pad$}{sep}{offset:0pad$}{sep}{ts:0pad$}{ext}",
            id = segment_id.as_u64(),
            sep = SEGMENT_FILE_BREAK,
            offset = base_offset,
            ts = created_at,
            pad = SEGMENT_FILE_PAD,
            ext = SEGMENT_FILE_EXTENSION
        )
    }

    pub fn parse(name: &Path) -> AofResult<Self> {
        let stem = name
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| invalid_segment_filename(name))?;

        if !stem.ends_with(SEGMENT_FILE_EXTENSION) {
            return Err(invalid_segment_filename(name));
        }

        let trimmed = &stem[..stem.len() - SEGMENT_FILE_EXTENSION.len()];
        let mut parts = trimmed.split(SEGMENT_FILE_BREAK);

        let segment_id = parts
            .next()
            .map(str::trim)
            .and_then(|raw| raw.parse::<u64>().ok())
            .map(SegmentId::new)
            .ok_or_else(|| invalid_segment_filename(name))?;

        let base_offset = parts
            .next()
            .map(str::trim)
            .and_then(|raw| raw.parse::<u64>().ok())
            .ok_or_else(|| invalid_segment_filename(name))?;

        let created_at = parts
            .next()
            .map(str::trim)
            .and_then(|raw| raw.parse::<i64>().ok())
            .ok_or_else(|| invalid_segment_filename(name))?;

        if parts.next().is_some() {
            return Err(invalid_segment_filename(name));
        }

        Ok(SegmentFileName {
            segment_id,
            base_offset,
            created_at,
        })
    }
}

impl Layout {
    pub fn new(config: &AofConfig) -> Self {
        let root = config.root_dir.clone();
        let segments = root.join("segments");
        let warm = root.join("warm");
        let manifest = root.join("manifest");
        let archive = root.join("archive");
        Self {
            root,
            segments,
            warm,
            manifest,
            archive,
        }
    }

    pub fn ensure(&self) -> AofResult<()> {
        self.create_dir(&self.root)?;
        self.create_dir(&self.segments)?;
        self.create_dir(&self.warm)?;
        self.create_dir(&self.manifest)?;
        self.create_dir(&self.archive)?;
        let _ = fsync_dir(&self.root);
        Ok(())
    }

    pub fn root_dir(&self) -> &Path {
        &self.root
    }

    pub fn segments_dir(&self) -> &Path {
        &self.segments
    }

    pub fn current_sealed_pointer_path(&self) -> PathBuf {
        self.segments.join("current.sealed")
    }

    pub fn load_current_sealed_pointer(&self) -> AofResult<Option<CurrentSealedPointer>> {
        let path = self.current_sealed_pointer_path();
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    return Ok(None);
                }
                return Err(AofError::from(err));
            }
        };
        let mut buf = [0u8; CURRENT_POINTER_SIZE];
        if let Err(err) = file.read_exact(&mut buf) {
            return Err(AofError::from(err));
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&buf[0..4]);
        let magic = u32::from_le_bytes(magic);
        if magic != CURRENT_POINTER_MAGIC {
            return Err(AofError::Corruption(format!(
                "unexpected current.sealed pointer magic: {magic:#x}"
            )));
        }
        let mut version = [0u8; 2];
        version.copy_from_slice(&buf[4..6]);
        let version = u16::from_le_bytes(version);
        if version != CURRENT_POINTER_VERSION {
            return Err(AofError::Corruption(format!(
                "unsupported current.sealed pointer version: {version}"
            )));
        }
        let mut segment_bytes = [0u8; 8];
        segment_bytes.copy_from_slice(&buf[8..16]);
        let segment_id = SegmentId::new(u64::from_le_bytes(segment_bytes));
        let mut watermark_bytes = [0u8; 8];
        watermark_bytes.copy_from_slice(&buf[16..24]);
        let coordinator_watermark = u64::from_le_bytes(watermark_bytes);
        let mut durable_bytes_bytes = [0u8; 8];
        durable_bytes_bytes.copy_from_slice(&buf[24..32]);
        let durable_bytes = u64::from_le_bytes(durable_bytes_bytes);
        Ok(Some(CurrentSealedPointer {
            segment_id,
            coordinator_watermark,
            durable_bytes,
        }))
    }

    pub fn store_current_sealed_pointer(&self, pointer: CurrentSealedPointer) -> AofResult<()> {
        let mut guard = TempFileGuard::new(self.segments_dir(), "current.sealed", ".tmp")?;
        let mut buf = [0u8; CURRENT_POINTER_SIZE];
        buf[0..4].copy_from_slice(&CURRENT_POINTER_MAGIC.to_le_bytes());
        buf[4..6].copy_from_slice(&CURRENT_POINTER_VERSION.to_le_bytes());
        buf[6..8].fill(0);
        buf[8..16].copy_from_slice(&pointer.segment_id.as_u64().to_le_bytes());
        buf[16..24].copy_from_slice(&pointer.coordinator_watermark.to_le_bytes());
        buf[24..32].copy_from_slice(&pointer.durable_bytes.to_le_bytes());
        guard.file_mut().write_all(&buf).map_err(AofError::from)?;
        let path = self.current_sealed_pointer_path();
        guard.persist(&path)?;
        Ok(())
    }

    pub fn warm_dir(&self) -> &Path {
        &self.warm
    }

    pub fn manifest_dir(&self) -> &Path {
        &self.manifest
    }

    pub fn archive_dir(&self) -> &Path {
        &self.archive
    }

    pub fn segment_path(&self, name: &str) -> PathBuf {
        self.segments.join(name)
    }

    pub fn segment_file_path(
        &self,
        segment_id: SegmentId,
        base_offset: u64,
        created_at: i64,
    ) -> PathBuf {
        let name = SegmentFileName::format(segment_id, base_offset, created_at);
        self.segment_path(&name)
    }

    pub fn archive_path(&self, name: &str) -> PathBuf {
        self.archive.join(name)
    }

    pub fn warm_path(&self, name: &str) -> PathBuf {
        self.warm.join(name)
    }

    pub fn warm_file_path(
        &self,
        segment_id: SegmentId,
        base_offset: u64,
        sealed_at: i64,
    ) -> PathBuf {
        let name = format!(
            "{id:020}_{offset:020}_{sealed:020}{ext}",
            id = segment_id.as_u64(),
            offset = base_offset,
            sealed = sealed_at,
            ext = WARM_FILE_EXTENSION,
        );
        self.warm_path(&name)
    }

    pub fn archive_file_path(
        &self,
        segment_id: SegmentId,
        base_offset: u64,
        created_at: i64,
    ) -> PathBuf {
        let name = SegmentFileName::format(segment_id, base_offset, created_at);
        self.archive_path(&name)
    }

    fn create_dir(&self, path: &Path) -> AofResult<()> {
        fs::create_dir_all(path).map_err(AofError::from)?;
        Ok(())
    }
}

pub fn create_fixed_size_file(path: &Path, size: u64) -> AofResult<File> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(true)
        .open(path)
        .map_err(AofError::from)?;
    file.set_len(size).map_err(AofError::from)?;
    file.sync_all().map_err(AofError::from)?;
    Ok(file)
}

pub fn fsync_dir(path: &Path) -> AofResult<()> {
    match OpenOptions::new().read(true).open(path) {
        Ok(file) => match file.sync_all() {
            Ok(_) => Ok(()),
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::Unsupported | io::ErrorKind::PermissionDenied
                ) =>
            {
                Ok(())
            }
            Err(err) => Err(AofError::from(err)),
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(AofError::FileSystem(format!(
            "directory not found: {}",
            path.display()
        ))),
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        Err(err) => Err(AofError::from(err)),
    }
}

/// Guard for atomic file persistence.
pub struct TempFileGuard {
    inner: Option<NamedTempFile>,
    committed: bool,
}

impl TempFileGuard {
    pub fn new(parent: &Path, prefix: &str, suffix: &str) -> AofResult<Self> {
        let inner = tempfile::Builder::new()
            .prefix(prefix)
            .suffix(suffix)
            .tempfile_in(parent)
            .map_err(|err| AofError::FileSystem(err.to_string()))?;
        Ok(Self {
            inner: Some(inner),
            committed: false,
        })
    }

    pub fn file_mut(&mut self) -> &mut File {
        self.inner
            .as_mut()
            .expect("temp file already consumed")
            .as_file_mut()
    }

    pub fn persist(mut self, dst: &Path) -> AofResult<()> {
        let temp = self
            .inner
            .take()
            .ok_or_else(|| AofError::InvalidState("temp file already consumed".to_string()))?;
        temp.as_file().sync_all().map_err(AofError::from)?;
        let parent = dst
            .parent()
            .ok_or_else(|| AofError::invalid_config("destination path missing parent"))?;
        if dst.exists() {
            if let Err(err) = fs::remove_file(dst) {
                if err.kind() != io::ErrorKind::NotFound {
                    return Err(AofError::from(err));
                }
            }
        }
        match temp.persist(dst) {
            Ok(file) => {
                file.sync_all().map_err(AofError::from)?;
                let _ = fsync_dir(parent);
                self.committed = true;
                Ok(())
            }
            Err(err) => {
                self.inner = Some(err.file);
                Err(AofError::FileSystem(err.error.to_string()))
            }
        }
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(temp) = self.inner.take() {
                let _ = temp.close();
            }
        }
    }
}

pub fn invalid_segment_filename(name: &Path) -> AofError {
    AofError::InvalidState(format!("invalid segment filename: {}", name.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn segment_filename_roundtrip() {
        let id = SegmentId::new(42);
        let base_offset = 1_048_576;
        let created_at = 1_700_000_000_000_000_000;
        let name = SegmentFileName::format(id, base_offset, created_at);
        let parsed = SegmentFileName::parse(Path::new(&name)).expect("parse");
        assert_eq!(parsed.segment_id, id);
        assert_eq!(parsed.base_offset, base_offset);
        assert_eq!(parsed.created_at, created_at);
    }

    #[test]
    fn layout_creates_directories() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("aof_root");
        let layout = Layout::new(&cfg);
        layout.ensure().expect("ensure");
        assert!(layout.root_dir().exists());
        assert!(layout.segments_dir().exists());
        assert!(layout.warm_dir().exists());
        assert!(layout.warm_dir().exists());
        assert!(layout.archive_dir().exists());
    }

    #[test]
    fn current_pointer_roundtrip() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("pointer_root");
        let layout = Layout::new(&cfg);
        layout.ensure().expect("ensure layout");

        let pointer = CurrentSealedPointer {
            segment_id: SegmentId::new(7),
            coordinator_watermark: 42,
            durable_bytes: 9_999,
        };

        layout
            .store_current_sealed_pointer(pointer)
            .expect("store pointer");

        let loaded = layout
            .load_current_sealed_pointer()
            .expect("load pointer")
            .expect("pointer present");

        assert_eq!(loaded.segment_id, pointer.segment_id);
        assert_eq!(loaded.coordinator_watermark, pointer.coordinator_watermark);
        assert_eq!(loaded.durable_bytes, pointer.durable_bytes);
    }

    #[test]
    fn load_current_pointer_rejects_bad_magic() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("pointer_magic");
        let layout = Layout::new(&cfg);
        layout.ensure().expect("ensure layout");

        let path = layout.current_sealed_pointer_path();
        std::fs::write(&path, [0u8; CURRENT_POINTER_SIZE]).expect("write corrupt pointer");

        let err = layout
            .load_current_sealed_pointer()
            .expect_err("invalid magic should error");
        match err {
            AofError::Corruption(message) => {
                assert!(message.contains("magic"), "unexpected message: {message}");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn pointer_writer_replaces_atomically() {
        let tmp = TempDir::new().expect("tempdir");
        let mut cfg = AofConfig::default();
        cfg.root_dir = tmp.path().join("pointer_atomic");
        let layout = Layout::new(&cfg);
        layout.ensure().expect("ensure layout");

        let initial = CurrentSealedPointer {
            segment_id: SegmentId::new(1),
            coordinator_watermark: 10,
            durable_bytes: 512,
        };
        layout
            .store_current_sealed_pointer(initial)
            .expect("store initial");

        let updated = CurrentSealedPointer {
            segment_id: SegmentId::new(2),
            coordinator_watermark: 20,
            durable_bytes: 1024,
        };
        layout
            .store_current_sealed_pointer(updated)
            .expect("store updated");

        let files: Vec<_> = std::fs::read_dir(layout.segments_dir())
            .expect("segments dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .collect();
        assert_eq!(files.len(), 1, "only pointer file expected");
        assert_eq!(files[0].file_name(), "current.sealed");

        let loaded = layout
            .load_current_sealed_pointer()
            .expect("load pointer")
            .expect("pointer present");
        assert_eq!(loaded.segment_id, updated.segment_id);
        assert_eq!(loaded.coordinator_watermark, updated.coordinator_watermark);
        assert_eq!(loaded.durable_bytes, updated.durable_bytes);
    }
}
