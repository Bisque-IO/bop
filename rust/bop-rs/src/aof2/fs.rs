use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

use super::config::{AofConfig, SegmentId};
use super::error::{AofError, AofResult};

/// Represents the canonical on-disk layout for an AOF instance.
#[derive(Debug, Clone)]
pub struct Layout {
    root: PathBuf,
    segments: PathBuf,
    archive: PathBuf,
}

pub const SEGMENT_FILE_EXTENSION: &str = ".seg";
const SEGMENT_FILE_BREAK: char = '_';
const SEGMENT_FILE_PAD: usize = 20;

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
        let archive = root.join("archive");
        Self {
            root,
            segments,
            archive,
        }
    }

    pub fn ensure(&self) -> AofResult<()> {
        self.create_dir(&self.root)?;
        self.create_dir(&self.segments)?;
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
        assert!(layout.archive_dir().exists());
    }
}
