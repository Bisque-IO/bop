//! S3-based archive storage implementation
//!
//! This module provides S3 storage backend for the AOF archive system.
//! It includes a complete configuration API and placeholder implementation
//! for MinIO/S3 storage operations.

use crate::aof::archive::ArchiveStorage;
use crate::aof::error::{AofError, AofResult};
// region provider not used in this module yet
use aws_credential_types::Credentials;
use aws_sdk_s3::config::SharedCredentialsProvider;
use aws_sdk_s3::{Client, Config};
use aws_smithy_types::byte_stream::ByteStream;
use aws_types::region::Region;
use bytes::Bytes;

/// S3 configuration for connecting to S3-compatible storage
#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: Option<String>,
    pub bucket: String,
    pub prefix: Option<String>,
}

impl S3Config {
    /// Create a new S3 configuration
    pub fn new(
        endpoint: impl Into<String>,
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
        bucket: impl Into<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            region: None,
            bucket: bucket.into(),
            prefix: None,
        }
    }

    /// Set the region (optional)
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Set the prefix for all keys (optional)
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }
}

/// S3-based archive storage
///
/// This implementation provides a complete interface for S3 storage operations.
/// The actual S3 operations are implemented as working stubs that can be easily
/// completed with proper MinIO client integration.
#[allow(dead_code)]
pub struct S3ArchiveStorage {
    config: S3Config,
    client: Client,
    bucket: String,
    prefix: String,
    index_key: String,
}

impl S3ArchiveStorage {
    /// Create a new S3 archive storage
    pub async fn new(config: S3Config) -> AofResult<Self> {
        // Validate configuration
        if config.endpoint.is_empty() {
            return Err(AofError::RemoteStorage(
                "S3 endpoint cannot be empty".to_string(),
            ));
        }
        if config.bucket.is_empty() {
            return Err(AofError::RemoteStorage(
                "S3 bucket cannot be empty".to_string(),
            ));
        }

        // Create credentials
        let credentials = Credentials::new(
            &config.access_key,
            &config.secret_key,
            None,     // session token
            None,     // expiry time
            "static", // provider name
        );

        // Determine region
        let region = if let Some(ref region_str) = config.region {
            Region::new(region_str.clone())
        } else {
            Region::new("us-east-1") // default region
        };

        // Create S3 client configuration
        let s3_config = Config::builder()
            .credentials_provider(SharedCredentialsProvider::new(credentials))
            .region(region)
            .endpoint_url(&config.endpoint)
            .force_path_style(true) // Required for MinIO
            .behavior_version_latest()
            .build();

        let client = Client::from_conf(s3_config);

        let prefix = config
            .prefix
            .clone()
            .unwrap_or_else(|| "aof-archive".to_string());
        let index_key = format!("{}/archive_index.bin", prefix);

        Ok(Self {
            client,
            bucket: config.bucket.clone(),
            prefix,
            index_key,
            config,
        })
    }

    /// Get the full S3 key for an archive key
    fn get_s3_key(&self, archive_key: &str) -> String {
        format!("{}/{}", self.prefix, archive_key)
    }
}

impl ArchiveStorage for S3ArchiveStorage {
    async fn store_segment(
        &self,
        _segment_path: &str,
        compressed_data: &[u8],
        archive_key: &str,
    ) -> AofResult<()> {
        let s3_key = self.get_s3_key(archive_key);

        // Store segment data in S3
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&s3_key)
            .body(ByteStream::from(Bytes::from(compressed_data.to_vec())))
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("Failed to store segment: {}", e)))?;

        Ok(())
    }

    async fn retrieve_segment(&self, archive_key: &str) -> AofResult<Vec<u8>> {
        let s3_key = self.get_s3_key(archive_key);

        // Retrieve segment data from S3
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&s3_key)
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("Failed to retrieve segment: {}", e)))?;

        // Collect all bytes from the response body
        let data = response
            .body
            .collect()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("Failed to read segment data: {}", e)))?
            .into_bytes();

        Ok(data.to_vec())
    }

    async fn delete_segment(&self, archive_key: &str) -> AofResult<()> {
        let s3_key = self.get_s3_key(archive_key);

        // Delete segment from S3
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&s3_key)
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("Failed to delete segment: {}", e)))?;

        Ok(())
    }

    async fn segment_exists(&self, archive_key: &str) -> AofResult<bool> {
        let s3_key = self.get_s3_key(archive_key);

        // Check if object exists using head_object
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&s3_key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // Object doesn't exist or error occurred
        }
    }

    async fn store_index(&self, index_data: &[u8]) -> AofResult<()> {
        // Store index data in S3
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&self.index_key)
            .body(ByteStream::from(Bytes::from(index_data.to_vec())))
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("Failed to store index: {}", e)))?;

        Ok(())
    }

    async fn retrieve_index(&self) -> AofResult<Option<Vec<u8>>> {
        // Try to retrieve index data from S3
        match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&self.index_key)
            .send()
            .await
        {
            Ok(response) => {
                // Collect all bytes from the response body
                let data = response
                    .body
                    .collect()
                    .await
                    .map_err(|e| {
                        AofError::RemoteStorage(format!("Failed to read index data: {}", e))
                    })?
                    .into_bytes();
                Ok(Some(data.to_vec()))
            }
            Err(_) => Ok(None), // Index doesn't exist
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test S3 configuration for MinIO
    fn create_test_config() -> S3Config {
        S3Config::new(
            "http://localhost:9000", // MinIO default endpoint
            "minioadmin",            // MinIO default access key
            "minioadmin",            // MinIO default secret key
            "test-bucket",           // Test bucket
        )
        .with_prefix("test-prefix")
    }

    #[tokio::test]
    #[ignore] // Ignore by default since it requires a running MinIO instance
    async fn test_s3_archive_storage_integration() {
        let config = create_test_config();
        let storage = S3ArchiveStorage::new(config)
            .await
            .expect("Failed to create S3 storage");

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

    #[test]
    fn test_s3_config_creation() {
        let config = S3Config::new(
            "https://s3.amazonaws.com",
            "access_key",
            "secret_key",
            "my-bucket",
        )
        .with_region("us-west-2")
        .with_prefix("aof-segments");

        assert_eq!(config.endpoint, "https://s3.amazonaws.com");
        assert_eq!(config.access_key, "access_key");
        assert_eq!(config.secret_key, "secret_key");
        assert_eq!(config.bucket, "my-bucket");
        assert_eq!(config.region, Some("us-west-2".to_string()));
        assert_eq!(config.prefix, Some("aof-segments".to_string()));
    }

    #[tokio::test]
    async fn test_s3_key_generation() {
        let config = S3Config::new(
            "http://localhost:9000",
            "access_key",
            "secret_key",
            "test-bucket",
        )
        .with_prefix("my-prefix");

        let storage = S3ArchiveStorage::new(config)
            .await
            .expect("Failed to create storage");

        let s3_key = storage.get_s3_key("segment_123.zst");
        assert_eq!(s3_key, "my-prefix/segment_123.zst");
    }
}
