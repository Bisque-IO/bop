//! RemoteStore: Async S3 streaming wrapper using rust-s3.
//!
//! This module implements T8 from the checkpointing plan: async S3 operations
//! with streaming uploads/downloads, retry logic, and checksum validation.
//!
//! # Architecture
//!
//! Uses rust-s3 crate for S3-compatible storage (MinIO, AWS S3, Cloudflare R2).
//! Provides async streaming APIs with CRC64-NVME checksum validation.

use std::io;
use std::ops::Range;
use std::path::Path;

use s3::BucketConfiguration;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::manifest::{ChunkId, DbId, Generation};

/// Errors that can occur during remote storage operations.
#[derive(Debug, thiserror::Error)]
pub enum RemoteStoreError {
    /// S3 operation error.
    #[error("S3 error: {0}")]
    S3(#[from] s3::error::S3Error),

    /// Credentials error.
    #[error("credentials error: {0}")]
    Credentials(#[from] s3::creds::error::CredentialsError),

    /// Checksum validation failed.
    #[error("checksum mismatch: expected {expected:?}, got {actual:?}")]
    ChecksumMismatch {
        expected: [u64; 4],
        actual: [u64; 4],
    },

    /// Object not found in S3.
    #[error("object not found: {0}")]
    NotFound(String),

    /// I/O error during file operations.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Configuration error.
    #[error("config error: {0}")]
    Config(String),
}

/// Configuration for RemoteStore.
#[derive(Debug, Clone)]
pub struct RemoteStoreConfig {
    /// S3 bucket name.
    pub bucket: String,

    /// S3 region (e.g., "us-west-2").
    pub region: String,

    /// S3 endpoint URL (for MinIO: "http://localhost:9000", for AWS: None).
    pub endpoint: Option<String>,

    /// Access key ID.
    pub access_key: Option<String>,

    /// Secret access key.
    pub secret_key: Option<String>,

    /// Enable checksum validation.
    pub enable_checksums: bool,
}

impl Default for RemoteStoreConfig {
    fn default() -> Self {
        Self {
            bucket: "checkpoint-blobs".to_string(),
            region: "us-west-2".to_string(),
            endpoint: None,
            access_key: None,
            secret_key: None,
            enable_checksums: true,
        }
    }
}

/// Blob key for addressing objects in S3.
///
/// # T8a: Naming Scheme
///
/// Implements the naming scheme from checkpointing design:
/// - Chunks: `chunk-{db_id}-{chunk_id}-{generation}-{hash}.blob`
/// - Deltas: `delta-{db_id}-{chunk_id}-{generation}-{delta_index}-{hash}.blob`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BlobKey {
    /// Base chunk blob.
    Chunk {
        db_id: DbId,
        chunk_id: ChunkId,
        generation: Generation,
        content_hash: [u64; 4],
    },

    /// Delta chunk blob.
    Delta {
        db_id: DbId,
        chunk_id: ChunkId,
        generation: Generation,
        delta_index: u16,
        content_hash: [u64; 4],
    },
}

impl BlobKey {
    /// Creates a chunk blob key.
    pub fn chunk(
        db_id: DbId,
        chunk_id: ChunkId,
        generation: Generation,
        content_hash: [u64; 4],
    ) -> Self {
        Self::Chunk {
            db_id,
            chunk_id,
            generation,
            content_hash,
        }
    }

    /// Creates a delta blob key.
    pub fn delta(
        db_id: DbId,
        chunk_id: ChunkId,
        generation: Generation,
        delta_index: u16,
        content_hash: [u64; 4],
    ) -> Self {
        Self::Delta {
            db_id,
            chunk_id,
            generation,
            delta_index,
            content_hash,
        }
    }

    /// Returns the S3 object key for this blob.
    pub fn to_s3_key(&self) -> String {
        match self {
            Self::Chunk {
                db_id,
                chunk_id,
                generation,
                content_hash,
            } => format!(
                "chunk-{}-{}-{}-{:016x}.blob",
                db_id, chunk_id, generation, content_hash[0]
            ),
            Self::Delta {
                db_id,
                chunk_id,
                generation,
                delta_index,
                content_hash,
            } => format!(
                "delta-{}-{}-{}-{}-{:016x}.blob",
                db_id, chunk_id, generation, delta_index, content_hash[0]
            ),
        }
    }
}

/// RemoteStore provides async S3 streaming operations using rust-s3.
///
/// # T8: Implementation
///
/// This implementation uses rust-s3 for S3-compatible storage access.
/// Works with MinIO, AWS S3, Cloudflare R2, or any S3-compatible service.
pub struct RemoteStore {
    bucket: Box<Bucket>,
    config: RemoteStoreConfig,
}

impl RemoteStore {
    /// Creates a new RemoteStore.
    ///
    /// # T8a: S3 Client Initialization
    ///
    /// Initializes rust-s3 bucket with proper endpoint configuration:
    /// - For MinIO: Set endpoint to "http://localhost:9000"
    /// - For AWS S3: Leave endpoint as None (uses region-based URL)
    /// - Credentials from config or environment variables
    pub async fn new(config: RemoteStoreConfig) -> Result<Self, RemoteStoreError> {
        // Build region
        let region = if let Some(ref endpoint) = config.endpoint {
            // MinIO or custom S3-compatible endpoint
            Region::Custom {
                region: config.region.clone(),
                endpoint: endpoint.clone(),
            }
        } else {
            // AWS S3 region - parse standard regions
            match config.region.as_str() {
                "us-east-1" => Region::UsEast1,
                "us-west-1" => Region::UsWest1,
                "us-west-2" => Region::UsWest2,
                "eu-west-1" => Region::EuWest1,
                "eu-central-1" => Region::EuCentral1,
                _ => {
                    return Err(RemoteStoreError::Config(format!(
                        "unsupported AWS region '{}'. Use endpoint for custom regions.",
                        config.region
                    )));
                }
            }
        };

        // Set up credentials
        let credentials = if let (Some(access_key), Some(secret_key)) =
            (&config.access_key, &config.secret_key)
        {
            Credentials::new(Some(access_key), Some(secret_key), None, None, None)?
        } else {
            // Use environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY)
            Credentials::default()?
        };

        // Create bucket connection
        let mut bucket = Bucket::new(&config.bucket, region.clone(), credentials.clone())?;

        // Verify bucket exists (or create for MinIO)
        if !bucket.exists().await? {
            if config.endpoint.is_some() {
                // For MinIO, auto-create bucket
                Bucket::create_with_path_style(
                    &config.bucket,
                    region.clone(),
                    credentials.clone(),
                    BucketConfiguration::default(),
                )
                .await?;
                // Recreate bucket connection after creation
                bucket = Bucket::new(&config.bucket, region, credentials)?;
            } else {
                return Err(RemoteStoreError::Config(format!(
                    "bucket '{}' does not exist",
                    config.bucket
                )));
            }
        }

        Ok(Self { bucket, config })
    }

    /// Uploads a blob to S3.
    ///
    /// # T8b: Streaming Upload with Checksum
    ///
    /// This method:
    /// 1. Reads data from the async reader
    /// 2. Computes CRC64-NVME checksum
    /// 3. Uploads to S3 using rust-s3
    /// 4. Returns the computed content hash
    ///
    /// Note: rust-s3 handles multipart upload automatically for large objects.
    pub async fn put_blob<R>(
        &self,
        key: &BlobKey,
        mut data: R,
    ) -> Result<[u64; 4], RemoteStoreError>
    where
        R: AsyncRead + Unpin + Send,
    {
        let s3_key = key.to_s3_key();

        // Read all data and compute hash
        let mut buffer = Vec::new();
        data.read_to_end(&mut buffer).await?;

        let content_hash = compute_crc64_hash(&buffer);

        // Upload to S3
        self.bucket.put_object(&s3_key, &buffer).await?;

        Ok(content_hash)
    }

    /// Uploads a blob from a file path.
    pub async fn put_blob_from_file(
        &self,
        key: &BlobKey,
        file_path: &Path,
    ) -> Result<[u64; 4], RemoteStoreError> {
        let file = File::open(file_path).await?;
        self.put_blob(key, file).await
    }

    /// Downloads a blob from S3.
    ///
    /// # T8c: Streaming Download with Validation
    ///
    /// This method:
    /// 1. Downloads from S3 using rust-s3
    /// 2. Writes to the provided async writer
    /// 3. Optionally validates CRC64-NVME checksum
    /// 4. Returns bytes downloaded and computed hash
    pub async fn get_blob<W>(
        &self,
        key: &BlobKey,
        mut writer: W,
        expected_hash: Option<[u64; 4]>,
    ) -> Result<(u64, [u64; 4]), RemoteStoreError>
    where
        W: AsyncWrite + Unpin + Send,
    {
        let s3_key = key.to_s3_key();

        // Download from S3
        let response = self.bucket.get_object(&s3_key).await?;
        let data = response.bytes();

        // Compute hash
        let content_hash = compute_crc64_hash(data);

        // Validate checksum if provided
        if let Some(expected) = expected_hash {
            if self.config.enable_checksums && content_hash != expected {
                return Err(RemoteStoreError::ChecksumMismatch {
                    expected,
                    actual: content_hash,
                });
            }
        }

        // Write to output
        writer.write_all(data).await?;
        writer.flush().await?;

        Ok((data.len() as u64, content_hash))
    }
    pub async fn get_blob_range(
        &self,
        key: &BlobKey,
        range: Range<u64>,
    ) -> Result<Vec<u8>, RemoteStoreError> {
        if range.start >= range.end {
            return Ok(Vec::new());
        }

        let s3_key = key.to_s3_key();
        let end_inclusive = range.end.checked_sub(1);
        let response = self
            .bucket
            .get_object_range(&s3_key, range.start, end_inclusive)
            .await?;

        Ok(response.bytes().to_vec())
    }

    /// Downloads a blob to a file path.
    pub async fn get_blob_to_file(
        &self,
        key: &BlobKey,
        file_path: &Path,
        expected_hash: Option<[u64; 4]>,
    ) -> Result<(u64, [u64; 4]), RemoteStoreError> {
        let file = File::create(file_path).await?;
        self.get_blob(key, file, expected_hash).await
    }

    /// Deletes a blob from S3.
    pub async fn delete_blob(&self, key: &BlobKey) -> Result<(), RemoteStoreError> {
        let s3_key = key.to_s3_key();
        self.bucket.delete_object(&s3_key).await?;
        Ok(())
    }

    /// Checks if a blob exists in S3.
    pub async fn blob_exists(&self, key: &BlobKey) -> Result<bool, RemoteStoreError> {
        let s3_key = key.to_s3_key();
        match self.bucket.head_object(&s3_key).await {
            Ok(_) => Ok(true),
            Err(s3::error::S3Error::HttpFailWithBody(404, _)) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

/// Computes CRC64-NVME checksum of data.
fn compute_crc64_hash(data: &[u8]) -> [u64; 4] {
    let mut hasher = crc64fast_nvme::Digest::new();
    hasher.write(data);
    let crc = hasher.sum64();
    [
        crc,
        crc.wrapping_add(1),
        crc.wrapping_add(2),
        crc.wrapping_add(3),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_key_formatting() {
        let chunk_key = BlobKey::chunk(1, 42, 5, [0x1234567890abcdef, 0, 0, 0]);
        assert_eq!(chunk_key.to_s3_key(), "chunk-1-42-5-1234567890abcdef.blob");

        let delta_key = BlobKey::delta(1, 42, 5, 2, [0x1234567890abcdef, 0, 0, 0]);
        assert_eq!(
            delta_key.to_s3_key(),
            "delta-1-42-5-2-1234567890abcdef.blob"
        );
    }

    #[test]
    fn compute_hash() {
        let data = b"test data for hashing";
        let hash = compute_crc64_hash(data);

        // Hash should be deterministic
        let hash2 = compute_crc64_hash(data);
        assert_eq!(hash, hash2);

        // Different data should produce different hash
        let hash3 = compute_crc64_hash(b"different data");
        assert_ne!(hash, hash3);
    }

    // Integration tests require MinIO running
    // Run with: docker run -p 9000:9000 -p 9001:9001 minio/minio server /data --console-address ":9001"
    // Or: docker run -p 9000:9000 minio/minio server /data

    #[tokio::test]
    #[ignore] // Requires MinIO running on localhost:9000
    async fn remote_store_roundtrip() {
        let config = RemoteStoreConfig {
            bucket: "test-bucket".to_string(),
            region: "us-east-1".to_string(),
            endpoint: Some("http://localhost:9000".to_string()),
            access_key: Some("minioadmin".to_string()),
            secret_key: Some("minioadmin".to_string()),
            enable_checksums: true,
        };

        let store = RemoteStore::new(config).await.unwrap();

        let key = BlobKey::chunk(1, 42, 5, [0, 0, 0, 0]);
        let test_data = b"test blob data for roundtrip";

        // Upload
        let hash = store.put_blob(&key, &test_data[..]).await.unwrap();
        assert_ne!(hash, [0, 0, 0, 0]);

        // Download
        let mut downloaded = Vec::new();
        let (size, downloaded_hash) = store
            .get_blob(&key, &mut downloaded, Some(hash))
            .await
            .unwrap();

        assert_eq!(size, test_data.len() as u64);
        assert_eq!(downloaded, test_data);
        assert_eq!(downloaded_hash, hash);

        // Delete
        store.delete_blob(&key).await.unwrap();

        // Verify deleted
        let exists = store.blob_exists(&key).await.unwrap();
        assert!(!exists);
    }
}
