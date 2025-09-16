//! S3 client abstraction for archive storage
//!
//! This module provides a clean S3 client interface specifically for use
//! with the archive storage system, without mixing filesystem concerns.

use crate::aof::error::{AofError, AofResult};
use std::time::SystemTime;

/// S3 object metadata
#[derive(Debug, Clone)]
pub struct S3ObjectMetadata {
    pub size: u64,
    pub last_modified: SystemTime,
    pub etag: String,
}

/// S3 client abstraction for archive operations
pub trait S3Client: Send + Sync {
    /// Put an object into S3
    async fn put_object(&self, bucket: &str, key: &str, data: &[u8]) -> AofResult<()>;

    /// Get an object from S3
    async fn get_object(&self, bucket: &str, key: &str) -> AofResult<Vec<u8>>;

    /// Delete an object from S3
    async fn delete_object(&self, bucket: &str, key: &str) -> AofResult<()>;

    /// Check if an object exists in S3
    async fn object_exists(&self, bucket: &str, key: &str) -> AofResult<bool>;

    /// List objects with a prefix
    async fn list_objects(&self, bucket: &str, prefix: &str) -> AofResult<Vec<String>>;

    /// Get object metadata
    async fn object_metadata(&self, bucket: &str, key: &str) -> AofResult<S3ObjectMetadata>;

    /// Put multiple objects in a batch (if supported)
    async fn put_objects(&self, bucket: &str, objects: Vec<(String, Vec<u8>)>) -> AofResult<()> {
        // Default implementation does individual puts
        for (key, data) in objects {
            self.put_object(bucket, &key, &data).await?;
        }
        Ok(())
    }

    /// Delete multiple objects in a batch (if supported)
    async fn delete_objects(&self, bucket: &str, keys: Vec<String>) -> AofResult<()> {
        // Default implementation does individual deletes
        for key in keys {
            self.delete_object(bucket, &key).await?;
        }
        Ok(())
    }
}

/// Mock S3 client for testing
#[cfg(test)]
pub struct MockS3Client {
    objects: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
}

#[cfg(test)]
impl MockS3Client {
    pub fn new() -> Self {
        Self {
            objects: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    fn get_key(bucket: &str, key: &str) -> String {
        format!("{}/{}", bucket, key)
    }
}

#[cfg(test)]
impl S3Client for MockS3Client {
    async fn put_object(&self, bucket: &str, key: &str, data: &[u8]) -> AofResult<()> {
        let full_key = Self::get_key(bucket, key);
        let mut objects = self
            .objects
            .lock()
            .map_err(|_| AofError::RemoteStorage("Failed to acquire lock".to_string()))?;
        objects.insert(full_key, data.to_vec());
        Ok(())
    }

    async fn get_object(&self, bucket: &str, key: &str) -> AofResult<Vec<u8>> {
        let full_key = Self::get_key(bucket, key);
        let objects = self
            .objects
            .lock()
            .map_err(|_| AofError::RemoteStorage("Failed to acquire lock".to_string()))?;

        objects
            .get(&full_key)
            .cloned()
            .ok_or_else(|| AofError::RemoteStorage(format!("Object not found: {}", full_key)))
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> AofResult<()> {
        let full_key = Self::get_key(bucket, key);
        let mut objects = self
            .objects
            .lock()
            .map_err(|_| AofError::RemoteStorage("Failed to acquire lock".to_string()))?;
        objects.remove(&full_key);
        Ok(())
    }

    async fn object_exists(&self, bucket: &str, key: &str) -> AofResult<bool> {
        let full_key = Self::get_key(bucket, key);
        let objects = self
            .objects
            .lock()
            .map_err(|_| AofError::RemoteStorage("Failed to acquire lock".to_string()))?;
        Ok(objects.contains_key(&full_key))
    }

    async fn list_objects(&self, bucket: &str, prefix: &str) -> AofResult<Vec<String>> {
        let full_prefix = Self::get_key(bucket, prefix);
        let objects = self
            .objects
            .lock()
            .map_err(|_| AofError::RemoteStorage("Failed to acquire lock".to_string()))?;

        let keys: Vec<String> = objects
            .keys()
            .filter(|k| k.starts_with(&full_prefix))
            .map(|k| {
                k.strip_prefix(&format!("{}/", bucket))
                    .unwrap_or(k)
                    .to_string()
            })
            .collect();

        Ok(keys)
    }

    async fn object_metadata(&self, bucket: &str, key: &str) -> AofResult<S3ObjectMetadata> {
        let data = self.get_object(bucket, key).await?;
        Ok(S3ObjectMetadata {
            size: data.len() as u64,
            last_modified: SystemTime::now(),
            etag: format!("mock-etag-{}", key.len()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_s3_client() {
        let client = MockS3Client::new();
        let bucket = "test-bucket";
        let key = "test-key";
        let data = b"test data";

        // Test put and get
        client.put_object(bucket, key, data).await.unwrap();
        let retrieved = client.get_object(bucket, key).await.unwrap();
        assert_eq!(retrieved, data);

        // Test existence check
        assert!(client.object_exists(bucket, key).await.unwrap());
        assert!(!client.object_exists(bucket, "nonexistent").await.unwrap());

        // Test metadata
        let metadata = client.object_metadata(bucket, key).await.unwrap();
        assert_eq!(metadata.size, data.len() as u64);

        // Test list objects
        client
            .put_object(bucket, "prefix/key1", b"data1")
            .await
            .unwrap();
        client
            .put_object(bucket, "prefix/key2", b"data2")
            .await
            .unwrap();
        client
            .put_object(bucket, "other/key3", b"data3")
            .await
            .unwrap();

        let prefix_objects = client.list_objects(bucket, "prefix").await.unwrap();
        assert_eq!(prefix_objects.len(), 2);
        assert!(prefix_objects.contains(&"prefix/key1".to_string()));
        assert!(prefix_objects.contains(&"prefix/key2".to_string()));

        // Test delete
        client.delete_object(bucket, key).await.unwrap();
        assert!(!client.object_exists(bucket, key).await.unwrap());
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let client = MockS3Client::new();
        let bucket = "test-bucket";

        // Test batch put
        let objects = vec![
            ("key1".to_string(), b"data1".to_vec()),
            ("key2".to_string(), b"data2".to_vec()),
        ];
        client.put_objects(bucket, objects).await.unwrap();

        assert!(client.object_exists(bucket, "key1").await.unwrap());
        assert!(client.object_exists(bucket, "key2").await.unwrap());

        // Test batch delete
        let keys = vec!["key1".to_string(), "key2".to_string()];
        client.delete_objects(bucket, keys).await.unwrap();

        assert!(!client.object_exists(bucket, "key1").await.unwrap());
        assert!(!client.object_exists(bucket, "key2").await.unwrap());
    }
}

// Example AWS S3 client implementation (commented out to avoid dependencies)
/*
use aws_sdk_s3::{Client, Error as S3Error};
use aws_smithy_http::body::SdkBody;

pub struct AwsS3Client {
    client: Client,
}

impl AwsS3Client {
    pub async fn new() -> Result<Self, S3Error> {
        let config = aws_config::load_from_env().await;
        let client = Client::new(&config);
        Ok(Self { client })
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }
}

impl S3Client for AwsS3Client {
    async fn put_object(&self, bucket: &str, key: &str, data: &[u8]) -> AofResult<()> {
        self.client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(SdkBody::from(data))
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("S3 put_object failed: {}", e)))?;
        Ok(())
    }

    async fn get_object(&self, bucket: &str, key: &str) -> AofResult<Vec<u8>> {
        let response = self.client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("S3 get_object failed: {}", e)))?;

        let data = response.body.collect().await
            .map_err(|e| AofError::RemoteStorage(format!("Failed to read S3 response: {}", e)))?;

        Ok(data.into_bytes().to_vec())
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> AofResult<()> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("S3 delete_object failed: {}", e)))?;
        Ok(())
    }

    async fn object_exists(&self, bucket: &str, key: &str) -> AofResult<bool> {
        match self.client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if it's a 404 error
                if e.to_string().contains("NotFound") {
                    Ok(false)
                } else {
                    Err(AofError::RemoteStorage(format!("S3 head_object failed: {}", e)))
                }
            }
        }
    }

    async fn list_objects(&self, bucket: &str, prefix: &str) -> AofResult<Vec<String>> {
        let response = self.client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("S3 list_objects failed: {}", e)))?;

        let keys = response.contents()
            .unwrap_or_default()
            .iter()
            .filter_map(|obj| obj.key())
            .map(|key| key.to_string())
            .collect();

        Ok(keys)
    }

    async fn object_metadata(&self, bucket: &str, key: &str) -> AofResult<S3ObjectMetadata> {
        let response = self.client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("S3 head_object failed: {}", e)))?;

        let size = response.content_length().unwrap_or(0) as u64;
        let last_modified = response.last_modified()
            .map(|dt| SystemTime::from(dt))
            .unwrap_or_else(SystemTime::now);
        let etag = response.e_tag().unwrap_or("").to_string();

        Ok(S3ObjectMetadata {
            size,
            last_modified,
            etag,
        })
    }

    async fn put_objects(&self, bucket: &str, objects: Vec<(String, Vec<u8>)>) -> AofResult<()> {
        // AWS S3 doesn't have native batch put, so we use individual puts with concurrency
        use futures::future::try_join_all;

        let futures = objects.into_iter().map(|(key, data)| {
            self.put_object(bucket, &key, &data)
        });

        try_join_all(futures).await?;
        Ok(())
    }

    async fn delete_objects(&self, bucket: &str, keys: Vec<String>) -> AofResult<()> {
        if keys.is_empty() {
            return Ok(());
        }

        let delete_objects: Vec<_> = keys.into_iter()
            .map(|key| ObjectIdentifier::builder().key(key).build())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AofError::RemoteStorage(format!("Failed to build delete objects: {}", e)))?;

        let delete_request = Delete::builder()
            .set_objects(Some(delete_objects))
            .build();

        self.client
            .delete_objects()
            .bucket(bucket)
            .delete(delete_request)
            .send()
            .await
            .map_err(|e| AofError::RemoteStorage(format!("S3 delete_objects failed: {}", e)))?;

        Ok(())
    }
}
*/
