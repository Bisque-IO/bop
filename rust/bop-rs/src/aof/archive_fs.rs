//! Filesystem-based archive storage implementation

use std::sync::Arc;

use crate::aof::archive::ArchiveStorage;
use crate::aof::error::{AofError, AofResult};
use crate::aof::filesystem::AsyncFileSystem;

/// Filesystem-based archive storage
#[derive(Clone)]
pub struct FilesystemArchiveStorage {
    fs: Arc<AsyncFileSystem>,
    archive_dir: String,
    index_file: String,
}

impl FilesystemArchiveStorage {
    /// Create a new filesystem archive storage
    pub async fn new(fs: Arc<AsyncFileSystem>, archive_dir: impl Into<String>) -> AofResult<Self> {
        let archive_dir = archive_dir.into();
        let index_file = format!("{}/archive_index.bin", archive_dir);

        // Ensure archive directory exists
        fs.create_directory(&archive_dir).await?;

        Ok(Self {
            fs,
            archive_dir,
            index_file,
        })
    }

    /// Get the full path for an archive key
    fn get_archive_path(&self, archive_key: &str) -> String {
        format!("{}/{}", self.archive_dir, archive_key)
    }
}

impl ArchiveStorage for FilesystemArchiveStorage {
    async fn store_segment(
        &self,
        _segment_path: &str,
        compressed_data: &[u8],
        archive_key: &str,
    ) -> AofResult<()> {
        let archive_path = self.get_archive_path(archive_key);
        let mut handle = self.fs.create_file(&archive_path).await?;
        handle.write_all(compressed_data).await?;
        handle.sync().await?;
        Ok(())
    }

    async fn retrieve_segment(&self, archive_key: &str) -> AofResult<Vec<u8>> {
        let archive_path = self.get_archive_path(archive_key);
        let mut handle = self.fs.open_file(&archive_path).await?;
        let buffer = handle.read_all().await?;
        Ok(buffer)
    }

    async fn delete_segment(&self, archive_key: &str) -> AofResult<()> {
        let archive_path = self.get_archive_path(archive_key);
        self.fs.delete_file(&archive_path).await?;
        Ok(())
    }

    async fn segment_exists(&self, archive_key: &str) -> AofResult<bool> {
        let archive_path = self.get_archive_path(archive_key);
        Ok(self.fs.file_exists(&archive_path).await?)
    }

    async fn store_index(&self, index_data: &[u8]) -> AofResult<()> {
        let mut handle = self.fs.create_file(&self.index_file).await?;
        handle.write_all(index_data).await?;
        handle.sync().await?;
        Ok(())
    }

    async fn retrieve_index(&self) -> AofResult<Option<Vec<u8>>> {
        match self.fs.file_exists(&self.index_file).await {
            Ok(true) => {
                let mut handle = self.fs.open_file(&self.index_file).await?;
                let buffer = handle.read_all().await?;
                Ok(Some(buffer))
            }
            Ok(false) => Ok(None), // Index file doesn't exist
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_filesystem_archive_storage() {
        let temp_dir = tempdir().unwrap();
        let fs = Arc::new(AsyncFileSystem::new(temp_dir.path()).unwrap());
        let archive_dir = "archive_test";

        let storage = FilesystemArchiveStorage::new(fs, archive_dir)
            .await
            .unwrap();

        // Test segment operations
        let test_data = b"test segment data";
        let archive_key = "test_segment.zst";

        // Store segment
        storage
            .store_segment("original_path", test_data, archive_key)
            .await
            .unwrap();

        // Check if segment exists
        assert!(storage.segment_exists(archive_key).await.unwrap());

        // Retrieve segment
        let retrieved_data = storage.retrieve_segment(archive_key).await.unwrap();
        assert_eq!(retrieved_data, test_data);

        // Test index operations
        let index_data = b"test index data";
        storage.store_index(index_data).await.unwrap();

        let retrieved_index = storage.retrieve_index().await.unwrap();
        assert_eq!(retrieved_index.unwrap(), index_data);

        // Delete segment
        storage.delete_segment(archive_key).await.unwrap();
        assert!(!storage.segment_exists(archive_key).await.unwrap());
    }
}
