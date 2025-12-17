use std::future::Future;
use std::path::{Path, PathBuf};

/// Result type for chunk storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

/// Errors that can occur during chunk storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Chunk not found: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Storage error: {0}")]
    Other(String),
}

/// A key that uniquely identifies a chunk in storage.
/// Format: `{db_id}/{branch_id}/wal/{start_frame}-{end_frame}`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChunkKey {
    pub db_id: u64,
    pub branch_id: u32,
    pub start_frame: u64,
    pub end_frame: u64,
}

impl ChunkKey {
    pub fn new(db_id: u64, branch_id: u32, start_frame: u64, end_frame: u64) -> Self {
        Self {
            db_id,
            branch_id,
            start_frame,
            end_frame,
        }
    }

    /// Convert to S3-style key path.
    pub fn to_path(&self) -> String {
        format!(
            "{:016x}/{:08x}/wal/{:016x}-{:016x}",
            self.db_id, self.branch_id, self.start_frame, self.end_frame
        )
    }

    /// Parse from S3-style key path.
    pub fn from_path(path: &str) -> Option<Self> {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() != 4 || parts[2] != "wal" {
            return None;
        }

        let db_id = u64::from_str_radix(parts[0], 16).ok()?;
        let branch_id = u32::from_str_radix(parts[1], 16).ok()?;

        let frame_parts: Vec<&str> = parts[3].split('-').collect();
        if frame_parts.len() != 2 {
            return None;
        }

        let start_frame = u64::from_str_radix(frame_parts[0], 16).ok()?;
        let end_frame = u64::from_str_radix(frame_parts[1], 16).ok()?;

        Some(Self {
            db_id,
            branch_id,
            start_frame,
            end_frame,
        })
    }
}

/// Trait for S3-like chunk storage.
///
/// This abstracts over different storage backends (local filesystem, S3, etc.)
/// for storing WAL segment chunks.
pub trait ChunkStorage: Send + Sync {
    /// Put a chunk into storage.
    fn put(&self, key: &ChunkKey, data: &[u8]) -> impl Future<Output = StorageResult<()>> + Send;

    /// Get a chunk from storage.
    fn get(&self, key: &ChunkKey) -> impl Future<Output = StorageResult<Vec<u8>>> + Send;

    /// Delete a chunk from storage.
    fn delete(&self, key: &ChunkKey) -> impl Future<Output = StorageResult<()>> + Send;

    /// Check if a chunk exists in storage.
    fn exists(&self, key: &ChunkKey) -> impl Future<Output = StorageResult<bool>> + Send;

    /// List all chunks for a given database and branch.
    /// Returns chunks ordered by start_frame.
    fn list(
        &self,
        db_id: u64,
        branch_id: u32,
    ) -> impl Future<Output = StorageResult<Vec<ChunkKey>>> + Send;
}

/// Local filesystem implementation of ChunkStorage.
///
/// Stores chunks as files on the local filesystem, mimicking S3's
/// key-value semantics. Useful for development and testing.
pub struct LocalFsChunkStorage {
    root: PathBuf,
}

impl LocalFsChunkStorage {
    /// Create a new LocalFsChunkStorage rooted at the given path.
    pub fn new<P: AsRef<Path>>(root: P) -> StorageResult<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn key_to_path(&self, key: &ChunkKey) -> PathBuf {
        self.root.join(key.to_path())
    }

    fn ensure_parent_dir(&self, path: &Path) -> StorageResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

impl ChunkStorage for LocalFsChunkStorage {
    async fn put(&self, key: &ChunkKey, data: &[u8]) -> StorageResult<()> {
        let path = self.key_to_path(key);
        self.ensure_parent_dir(&path)?;
        std::fs::write(&path, data)?;
        Ok(())
    }

    async fn get(&self, key: &ChunkKey) -> StorageResult<Vec<u8>> {
        let path = self.key_to_path(key);
        if !path.exists() {
            return Err(StorageError::NotFound(key.to_path()));
        }
        Ok(std::fs::read(&path)?)
    }

    async fn delete(&self, key: &ChunkKey) -> StorageResult<()> {
        let path = self.key_to_path(key);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    async fn exists(&self, key: &ChunkKey) -> StorageResult<bool> {
        let path = self.key_to_path(key);
        Ok(path.exists())
    }

    async fn list(&self, db_id: u64, branch_id: u32) -> StorageResult<Vec<ChunkKey>> {
        let prefix = format!("{:016x}/{:08x}/wal", db_id, branch_id);
        let dir = self.root.join(&prefix);

        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut keys = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Parse frame range from filename
            let parts: Vec<&str> = name.split('-').collect();
            if parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (
                    u64::from_str_radix(parts[0], 16),
                    u64::from_str_radix(parts[1], 16),
                ) {
                    keys.push(ChunkKey {
                        db_id,
                        branch_id,
                        start_frame: start,
                        end_frame: end,
                    });
                }
            }
        }

        // Sort by start_frame
        keys.sort_by_key(|k| k.start_frame);
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_key_path_roundtrip() {
        let key = ChunkKey::new(0x123456789ABCDEF0, 0x12345678, 100, 200);
        let path = key.to_path();
        let parsed = ChunkKey::from_path(&path).unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn test_chunk_key_path_format() {
        let key = ChunkKey::new(1, 2, 100, 200);
        let path = key.to_path();
        assert_eq!(path, "0000000000000001/00000002/wal/0000000000000064-00000000000000c8");
    }
}
