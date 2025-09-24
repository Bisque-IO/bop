use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::sync::{Semaphore, mpsc};
use tokio::time::sleep;
use tracing::{trace, warn};

use crate::config::SegmentId;
use crate::error::{AofError, AofResult};

use super::tier0::InstanceId;

/// Returns current time as milliseconds since Unix epoch.
///
/// Provides timestamp generation for Tier 2 operations,
/// upload metadata, and retry logic. Falls back to 0
/// if system time is unavailable.
fn current_epoch_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

/// Retry policy configuration for Tier 2 operations.
///
/// Controls backoff behavior for failed remote storage operations
/// including uploads, downloads, and deletions.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Base delay between retries (exponential backoff)
    pub base_delay: Duration,
}

impl RetryPolicy {
    pub fn new(max_attempts: u32, base_delay: Duration) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            base_delay,
        }
    }

    pub async fn sleep_for(&self, attempt: u32) {
        if self.base_delay.is_zero() {
            return;
        }
        let delay = self.base_delay * attempt.max(1);
        sleep(delay).await;
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
        }
    }
}

/// Types of operations that can be retried.
#[derive(Debug, Clone, Copy)]
enum RetryKind {
    /// Uploading segments to remote storage
    Upload,
    /// Deleting segments from remote storage
    Delete,
    /// Fetching segments from remote storage
    Fetch,
}

/// Wrapper tracking retry attempts for an operation.
#[derive(Debug)]
struct RetryStats<T> {
    /// The wrapped value/result
    value: T,
    /// Number of attempts made
    attempts: u32,
}

/// Error that occurred after exhausting retry attempts.
#[derive(Debug)]
struct RetryError {
    /// The final error that caused retry exhaustion
    error: AofError,
    /// Total attempts made before giving up
    attempts: u32,
}

/// Security configuration for Tier 2 remote storage.
///
/// Supports various encryption options for data at rest
/// in remote storage systems like S3.
#[derive(Debug, Clone)]
pub enum Tier2Security {
    /// Server-side encryption with S3 managed keys
    SseS3,
    /// Server-side encryption with AWS KMS
    SseKms {
        /// KMS key identifier
        key_id: String,
        /// Optional encryption context
        context: Option<String>,
    },
    /// Customer-provided encryption keys
    CustomerKey {
        /// Base64-encoded encryption key
        key: String,
    },
}

/// Metadata for a segment stored in Tier 2 remote storage.
///
/// Contains all information needed to retrieve and verify
/// segments from remote storage systems.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tier2Metadata {
    /// Remote object key/path
    pub object_key: String,
    /// Entity tag for version/change detection
    #[serde(default)]
    pub etag: Option<String>,
    /// Size of the stored object in bytes
    #[serde(default)]
    pub size_bytes: u64,
    /// Timestamp when upload completed
    #[serde(default)]
    pub uploaded_epoch_ms: u64,
}

impl Tier2Metadata {
    pub fn new(object_key: String, etag: Option<String>, size_bytes: u64) -> Self {
        Self {
            object_key,
            etag,
            size_bytes,
            uploaded_epoch_ms: current_epoch_ms(),
        }
    }
}

/// Configuration for Tier 2 remote storage operations.
///
/// Configures connection parameters, concurrency limits, retry behavior,
/// and security settings for remote object storage (e.g., S3).
#[derive(Debug, Clone)]
pub struct Tier2Config {
    /// Remote storage endpoint URL
    pub endpoint: String,
    /// AWS region or equivalent
    pub region: String,
    /// Storage bucket name
    pub bucket: String,
    /// Object key prefix for organization
    pub prefix: String,
    /// Access key for authentication
    pub access_key: String,
    /// Secret key for authentication
    pub secret_key: String,
    /// Optional session token for temporary credentials
    pub session_token: Option<String>,
    /// Maximum concurrent data transfers
    pub max_concurrent_transfers: usize,
    /// Maximum concurrent upload operations
    pub max_concurrent_uploads: usize,
    /// Maximum concurrent download operations
    pub max_concurrent_downloads: usize,
    /// Maximum concurrent metadata operations
    pub max_concurrent_operations: usize,
    /// Retry policy for failed operations
    pub retry: RetryPolicy,
    /// Optional TTL for object retention
    pub retention_ttl: Option<Duration>,
    /// Optional encryption configuration
    pub security: Option<Tier2Security>,
}

impl Tier2Config {
    pub fn new(
        endpoint: impl Into<String>,
        region: impl Into<String>,
        bucket: impl Into<String>,
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            region: region.into(),
            bucket: bucket.into(),
            prefix: "aof2".to_string(),
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            session_token: None,
            max_concurrent_transfers: 2,
            max_concurrent_uploads: 2,
            max_concurrent_downloads: 2,
            max_concurrent_operations: 2,
            retry: RetryPolicy::default(),
            retention_ttl: None,
            security: None,
        }
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    pub fn with_session_token(mut self, token: impl Into<Option<String>>) -> Self {
        self.session_token = token.into();
        self
    }

    pub fn with_concurrency(mut self, workers: usize) -> Self {
        let workers = workers.max(1);
        self.max_concurrent_transfers = workers;
        self.max_concurrent_uploads = workers;
        self.max_concurrent_downloads = workers;
        self.max_concurrent_operations = workers;
        self
    }

    pub fn with_upload_concurrency(mut self, workers: usize) -> Self {
        self.max_concurrent_uploads = workers.max(1);
        self
    }

    pub fn with_download_concurrency(mut self, workers: usize) -> Self {
        self.max_concurrent_downloads = workers.max(1);
        self
    }

    pub fn with_operation_concurrency(mut self, workers: usize) -> Self {
        self.max_concurrent_operations = workers.max(1);
        self
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_retention_ttl(mut self, ttl: Option<Duration>) -> Self {
        self.retention_ttl = ttl;
        self
    }

    pub fn with_security(mut self, security: Option<Tier2Security>) -> Self {
        self.security = security;
        self
    }

    pub fn object_key(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
        sealed_at: i64,
    ) -> String {
        format!(
            "{}/{:016x}/segment-{:020}-{}.zst",
            self.prefix,
            instance_id.get(),
            segment_id.as_u64(),
            sealed_at.max(0)
        )
    }
}

impl Default for Tier2Config {
    fn default() -> Self {
        Self::new(
            "http://127.0.0.1:9000",
            "us-east-1",
            "aof",
            "minioadmin",
            "minioadmin",
        )
    }
}

/// Point-in-time metrics snapshot for Tier 2 remote storage.
///
/// Provides comprehensive observability into remote storage operations,
/// including success rates, retry behavior, and queue depths.
#[derive(Debug, Default, Clone, Copy)]
pub struct Tier2MetricsSnapshot {
    /// Total upload attempts (lifetime)
    pub upload_attempts: u64,
    /// Total upload failures (lifetime)
    pub upload_failures: u64,
    /// Total download operations (lifetime)
    pub downloads: u64,
    /// Total download failures (lifetime)
    pub download_failures: u64,
    /// Total delete operations (lifetime)
    pub deletes: u64,
    /// Total delete failures (lifetime)
    pub delete_failures: u64,
    /// Total upload retry attempts (lifetime)
    pub upload_retry_attempts: u64,
    /// Total upload retry failures (lifetime)
    pub upload_retry_failures: u64,
    /// Total delete retry attempts (lifetime)
    pub delete_retry_attempts: u64,
    /// Total delete retry failures (lifetime)
    pub delete_retry_failures: u64,
    /// Current upload queue depth
    pub upload_queue_depth: u32,
    /// Current delete queue depth
    pub delete_queue_depth: u32,
}

/// Thread-safe metrics collection for Tier 2 operations.
///
/// Uses atomic counters for concurrent metric updates from
/// upload, download, and delete worker threads.
#[derive(Debug, Default)]
pub struct Tier2Metrics {
    /// Total upload attempts
    upload_attempts: AtomicU64,
    /// Total upload failures
    upload_failures: AtomicU64,
    /// Total download operations
    downloads: AtomicU64,
    /// Total download failures
    download_failures: AtomicU64,
    /// Total delete operations
    deletes: AtomicU64,
    /// Total delete failures
    delete_failures: AtomicU64,
    /// Total upload retry attempts
    upload_retry_attempts: AtomicU64,
    /// Total upload retry failures
    upload_retry_failures: AtomicU64,
    /// Total delete retry attempts
    delete_retry_attempts: AtomicU64,
    /// Total delete retry failures
    delete_retry_failures: AtomicU64,
    /// Current upload queue depth
    upload_queue_depth: AtomicU32,
    /// Current delete queue depth
    delete_queue_depth: AtomicU32,
}

impl Tier2Metrics {
    fn snapshot(&self) -> Tier2MetricsSnapshot {
        Tier2MetricsSnapshot {
            upload_attempts: self.upload_attempts.load(AtomicOrdering::Relaxed),
            upload_failures: self.upload_failures.load(AtomicOrdering::Relaxed),
            downloads: self.downloads.load(AtomicOrdering::Relaxed),
            download_failures: self.download_failures.load(AtomicOrdering::Relaxed),
            deletes: self.deletes.load(AtomicOrdering::Relaxed),
            delete_failures: self.delete_failures.load(AtomicOrdering::Relaxed),
            upload_retry_attempts: self.upload_retry_attempts.load(AtomicOrdering::Relaxed),
            upload_retry_failures: self.upload_retry_failures.load(AtomicOrdering::Relaxed),
            delete_retry_attempts: self.delete_retry_attempts.load(AtomicOrdering::Relaxed),
            delete_retry_failures: self.delete_retry_failures.load(AtomicOrdering::Relaxed),
            upload_queue_depth: self.upload_queue_depth.load(AtomicOrdering::Relaxed),
            delete_queue_depth: self.delete_queue_depth.load(AtomicOrdering::Relaxed),
        }
    }

    fn incr_upload_attempt(&self) {
        self.upload_attempts.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_upload_failure(&self) {
        self.upload_failures.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_upload_retry_attempt(&self) {
        self.upload_retry_attempts
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_upload_retry_failure(&self) {
        self.upload_retry_failures
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_download(&self) {
        self.downloads.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_download_failure(&self) {
        self.download_failures.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_delete(&self) {
        self.deletes.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_delete_failure(&self) {
        self.delete_failures.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_delete_retry_attempt(&self) {
        self.delete_retry_attempts
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_delete_retry_failure(&self) {
        self.delete_retry_failures
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_upload_queue(&self) {
        self.upload_queue_depth
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn decr_upload_queue(&self) {
        let _ = self.upload_queue_depth.fetch_update(
            AtomicOrdering::AcqRel,
            AtomicOrdering::Relaxed,
            |value| value.checked_sub(1),
        );
    }

    fn incr_delete_queue(&self) {
        self.delete_queue_depth
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn decr_delete_queue(&self) {
        let _ = self.delete_queue_depth.fetch_update(
            AtomicOrdering::AcqRel,
            AtomicOrdering::Relaxed,
            |value| value.checked_sub(1),
        );
    }

    fn record_retry_attempt(&self, kind: RetryKind) {
        match kind {
            RetryKind::Upload => self.incr_upload_retry_attempt(),
            RetryKind::Delete => self.incr_delete_retry_attempt(),
            RetryKind::Fetch => {}
        }
    }

    fn record_retry_failure(&self, kind: RetryKind) {
        match kind {
            RetryKind::Upload => self.incr_upload_retry_failure(),
            RetryKind::Delete => self.incr_delete_retry_failure(),
            RetryKind::Fetch => {}
        }
    }
}

/// Descriptor for uploading a segment to Tier 2 storage.
///
/// Contains all metadata and file information needed for
/// remote storage upload operations.
#[derive(Debug, Clone)]
pub struct Tier2UploadDescriptor {
    /// Instance owning the segment
    pub instance_id: InstanceId,
    /// Segment to upload
    pub segment_id: SegmentId,
    /// Path to compressed segment file
    pub warm_path: PathBuf,
    /// Timestamp when segment was sealed
    pub sealed_at: i64,
    /// Base offset in original AOF log
    pub base_offset: u64,
    /// Record count at sealing time
    pub base_record_count: u64,
    /// Integrity checksum
    pub checksum: u32,
    /// Compressed file size in bytes
    pub compressed_bytes: u64,
    /// Original uncompressed size in bytes
    pub original_bytes: u64,
}

/// Request to fetch a segment from Tier 2 storage.
///
/// Used when Tier 1 cache miss requires retrieving segments
/// from remote storage for hydration.
#[derive(Debug, Clone)]
pub struct Tier2FetchRequest {
    /// Instance requesting the fetch
    pub instance_id: InstanceId,
    /// Segment to fetch
    pub segment_id: SegmentId,
    /// Timestamp when segment was sealed
    pub sealed_at: i64,
    /// Local path where fetched data should be written
    pub destination: PathBuf,
}

/// Request to delete a segment from Tier 2 storage.
///
/// Used when segments are no longer needed in remote storage,
/// typically due to retention policies or manual cleanup.
#[derive(Debug, Clone)]
pub struct Tier2DeleteRequest {
    /// Instance owning the segment
    pub instance_id: InstanceId,
    /// Segment to delete
    pub segment_id: SegmentId,
    /// Remote object key for deletion
    pub object_key: String,
}

/// Low-level request for uploading objects to remote storage.
#[derive(Clone)]
pub struct PutObjectRequest {
    /// Target bucket name
    pub bucket: String,
    /// Object key/path
    pub key: String,
    /// Local file to upload
    pub source: PathBuf,
    /// Custom metadata to attach
    pub metadata: HashMap<String, String>,
    /// Encryption configuration
    pub security: Option<Tier2Security>,
}

/// Result of a successful object upload.
pub struct PutObjectResult {
    /// Entity tag from remote storage
    pub etag: Option<String>,
    /// Size of uploaded object
    pub size: u64,
}

/// Low-level request for downloading objects from remote storage.
#[derive(Clone)]
pub struct GetObjectRequest {
    /// Source bucket name
    pub bucket: String,
    /// Object key/path
    pub key: String,
    /// Local path to write downloaded data
    pub destination: PathBuf,
    /// Decryption configuration
    pub security: Option<Tier2Security>,
}

/// Result of a successful object download.
pub struct GetObjectResult {
    /// Entity tag from remote storage
    pub etag: Option<String>,
    /// Size of downloaded object
    pub size: u64,
}

/// Result of object metadata query.
pub struct HeadObjectResult {
    /// Object size in bytes
    pub size: u64,
    /// Entity tag for version detection
    pub etag: Option<String>,
    /// Custom metadata attached to object
    pub metadata: HashMap<String, String>,
}

/// Trait for remote storage client implementations.
///
/// Abstracts over different remote storage backends (S3, MinIO, etc.)
/// with async operations for object management.
pub trait Tier2Client: Send + Sync {
    /// Upload an object to remote storage
    fn put_object(&self, request: PutObjectRequest) -> BoxFuture<'_, AofResult<PutObjectResult>>;
    /// Download an object from remote storage
    fn get_object(&self, request: GetObjectRequest) -> BoxFuture<'_, AofResult<GetObjectResult>>;
    /// Delete an object from remote storage
    fn delete_object(&self, bucket: &str, key: &str) -> BoxFuture<'_, AofResult<()>>;
    /// Query object metadata without downloading
    fn head_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> BoxFuture<'_, AofResult<Option<HeadObjectResult>>>;
}

use minio::s3::builders::ObjectContent;
use minio::s3::creds::StaticProvider;
use minio::s3::error::{Error as MinioError, ErrorCode};
use minio::s3::http::BaseUrl;
use minio::s3::multimap::{Multimap, MultimapExt};
use minio::s3::sse::{Sse, SseCustomerKey, SseKms, SseS3};
use minio::s3::types::S3Api;
use minio::s3::{Client as MinioClient, ClientBuilder};

#[derive(Clone)]
pub struct S3Tier2Client {
    client: MinioClient,
}

impl S3Tier2Client {
    pub fn new(config: &Tier2Config) -> AofResult<Self> {
        let base_url: BaseUrl = config.endpoint.parse().map_err(|err| {
            AofError::invalid_config(format!("invalid tier2 endpoint {}: {err}", config.endpoint))
        })?;
        let static_provider = StaticProvider::new(
            &config.access_key,
            &config.secret_key,
            config.session_token.as_deref(),
        );
        let client = ClientBuilder::new(base_url)
            .provider(Some(Box::new(static_provider)))
            .build()
            .map_err(|err| {
                AofError::invalid_config(format!("failed to build tier2 client: {err}"))
            })?;
        Ok(Self { client })
    }

    fn to_sse(&self, security: &Option<Tier2Security>) -> Option<Arc<dyn Sse>> {
        match security {
            Some(Tier2Security::SseS3) => Some(Arc::new(SseS3::new()) as Arc<dyn Sse>),
            Some(Tier2Security::SseKms { key_id, context }) => {
                Some(Arc::new(SseKms::new(key_id, context.as_deref())) as Arc<dyn Sse>)
            }
            Some(Tier2Security::CustomerKey { key }) => {
                Some(Arc::new(SseCustomerKey::new(key)) as Arc<dyn Sse>)
            }
            None => None,
        }
    }
}

fn map_minio_error(err: MinioError) -> AofError {
    AofError::RemoteStorage(err.to_string())
}

fn metadata_to_multimap(metadata: &HashMap<String, String>) -> Option<Multimap> {
    if metadata.is_empty() {
        return None;
    }
    let mut map = Multimap::new();
    for (key, value) in metadata {
        map.add(key.clone(), value.clone());
    }
    Some(map)
}

impl Tier2Client for S3Tier2Client {
    fn put_object(&self, request: PutObjectRequest) -> BoxFuture<'_, AofResult<PutObjectResult>> {
        let client = self.client.clone();
        let metadata = request.metadata.clone();
        let sse = self.to_sse(&request.security);
        let path = request.source.clone();
        let bucket = request.bucket.clone();
        let key = request.key.clone();
        Box::pin(async move {
            let mut builder =
                client.put_object_content(&bucket, &key, ObjectContent::from(path.as_path()));
            if let Some(meta) = metadata_to_multimap(&metadata) {
                builder = builder.user_metadata(Some(meta));
            }
            if let Some(sse) = sse {
                builder = builder.sse(Some(sse));
            }
            let response = builder.send().await.map_err(map_minio_error)?;
            Ok(PutObjectResult {
                etag: Some(response.etag),
                size: response.object_size,
            })
        })
    }

    fn get_object(&self, request: GetObjectRequest) -> BoxFuture<'_, AofResult<GetObjectResult>> {
        let client = self.client.clone();
        let destination = request.destination.clone();
        let bucket = request.bucket.clone();
        let key = request.key.clone();
        Box::pin(async move {
            let response = client
                .get_object(&bucket, &key)
                .send()
                .await
                .map_err(map_minio_error)?;
            if let Some(parent) = destination.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(AofError::from)?;
            }
            response
                .content
                .to_file(destination.as_path())
                .await
                .map_err(AofError::from)?;
            Ok(GetObjectResult {
                etag: response.etag,
                size: response.object_size,
            })
        })
    }

    fn delete_object(&self, bucket: &str, key: &str) -> BoxFuture<'_, AofResult<()>> {
        let client = self.client.clone();
        let bucket = bucket.to_owned();
        let key = key.to_owned();
        Box::pin(async move {
            client
                .delete_object(&bucket, &key)
                .send()
                .await
                .map_err(map_minio_error)?;
            Ok(())
        })
    }

    fn head_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> BoxFuture<'_, AofResult<Option<HeadObjectResult>>> {
        let client = self.client.clone();
        let bucket = bucket.to_owned();
        let key = key.to_owned();
        Box::pin(async move {
            match client.stat_object(&bucket, &key).send().await {
                Ok(resp) => {
                    let mut metadata = HashMap::new();
                    for (header, value) in resp.headers.iter() {
                        let name = header.as_str();
                        if name.starts_with("x-amz-meta-") {
                            if let Ok(v) = value.to_str() {
                                metadata.insert(
                                    name.trim_start_matches("x-amz-meta-").to_string(),
                                    v.to_string(),
                                );
                            }
                        }
                    }
                    Ok(Some(HeadObjectResult {
                        size: resp.size,
                        etag: Some(resp.etag),
                        metadata,
                    }))
                }
                Err(MinioError::S3Error(s3)) => match s3.code {
                    ErrorCode::NoSuchKey
                    | ErrorCode::NoSuchBucket
                    | ErrorCode::ResourceNotFound => Ok(None),
                    _ => Err(map_minio_error(MinioError::S3Error(s3))),
                },
                Err(other) => Err(map_minio_error(other)),
            }
        })
    }
}

#[derive(Debug)]
/// Events generated by Tier 2 storage operations.
///
/// Used to notify other tiers about the completion or failure
/// of remote storage operations.
pub enum Tier2Event {
    /// Upload operation completed successfully
    UploadCompleted(Tier2UploadComplete),
    /// Upload operation failed
    UploadFailed(Tier2UploadFailed),
    /// Delete operation completed successfully
    DeleteCompleted(Tier2DeleteComplete),
    /// Delete operation failed
    DeleteFailed(Tier2DeleteFailed),
    /// Fetch operation completed successfully
    FetchCompleted(Tier2FetchComplete),
    /// Fetch operation failed
    FetchFailed(Tier2FetchFailed),
}

#[derive(Debug)]
pub struct Tier2UploadComplete {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub metadata: Tier2Metadata,
    pub base_offset: u64,
    pub base_record_count: u64,
    pub checksum: u32,
}

#[derive(Debug)]
pub struct Tier2UploadFailed {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub error: AofError,
}

#[derive(Debug)]
pub struct Tier2DeleteComplete {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub object_key: String,
}

#[derive(Debug)]
pub struct Tier2DeleteFailed {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub object_key: String,
    pub error: AofError,
}

#[derive(Debug)]
pub struct Tier2FetchComplete {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub destination: PathBuf,
}

#[derive(Debug)]
pub struct Tier2FetchFailed {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub error: AofError,
}

#[derive(Debug)]
enum Tier2Command {
    Upload(UploadJob),
    Delete(DeleteJob),
    Fetch(FetchJob),
}

#[derive(Debug)]
struct UploadJob {
    descriptor: Tier2UploadDescriptor,
    object_key: String,
    metadata: HashMap<String, String>,
    security: Option<Tier2Security>,
    inflight_key: (InstanceId, SegmentId),
}

#[derive(Debug)]
struct DeleteJob {
    request: Tier2DeleteRequest,
}

#[derive(Debug)]
struct FetchJob {
    request: Tier2FetchRequest,
}

#[derive(Debug, Clone)]
struct RetentionEntry {
    request: Tier2DeleteRequest,
    deadline: Instant,
}

struct Tier2Inner {
    runtime: Handle,
    config: Tier2Config,
    client: Arc<dyn Tier2Client>,
    command_tx: mpsc::UnboundedSender<Tier2Command>,
    events_tx: mpsc::UnboundedSender<Tier2Event>,
    retry: RetryPolicy,
    retention: Mutex<VecDeque<RetentionEntry>>,
    inflight_uploads: Mutex<HashSet<(InstanceId, SegmentId)>>,
    inflight_fetches: Mutex<HashSet<(InstanceId, SegmentId)>>,
    metrics: Tier2Metrics,
    ops_semaphore: Arc<Semaphore>,
    upload_semaphore: Arc<Semaphore>,
    download_semaphore: Arc<Semaphore>,
}

impl Tier2Inner {
    fn build_metadata(
        &self,
        descriptor: &Tier2UploadDescriptor,
        object_key: &str,
    ) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("object_key".to_string(), object_key.to_string());
        metadata.insert(
            "instance_id".to_string(),
            descriptor.instance_id.get().to_string(),
        );
        metadata.insert(
            "segment_id".to_string(),
            descriptor.segment_id.as_u64().to_string(),
        );
        metadata.insert("sealed_at".to_string(), descriptor.sealed_at.to_string());
        metadata.insert(
            "base_offset".to_string(),
            descriptor.base_offset.to_string(),
        );
        metadata.insert(
            "base_record_count".to_string(),
            descriptor.base_record_count.to_string(),
        );
        metadata.insert("checksum".to_string(), descriptor.checksum.to_string());
        metadata.insert(
            "compressed_bytes".to_string(),
            descriptor.compressed_bytes.to_string(),
        );
        metadata.insert(
            "original_bytes".to_string(),
            descriptor.original_bytes.to_string(),
        );
        metadata
    }

    fn schedule_upload(&self, descriptor: Tier2UploadDescriptor) -> AofResult<()> {
        if !descriptor.warm_path.exists() {
            return Err(AofError::FileSystem(format!(
                "warm file missing for segment {}",
                descriptor.segment_id.as_u64()
            )));
        }
        let key = (descriptor.instance_id, descriptor.segment_id);
        {
            let inflight = self.inflight_uploads.lock();
            if inflight.contains(&key) {
                trace!(
                    instance = descriptor.instance_id.get(),
                    segment = descriptor.segment_id.as_u64(),
                    "tier2 upload already in flight"
                );
                return Ok(());
            }
        }
        {
            let mut inflight = self.inflight_uploads.lock();
            inflight.insert(key);
        }
        let object_key = self.config.object_key(
            descriptor.instance_id,
            descriptor.segment_id,
            descriptor.sealed_at,
        );
        let metadata = self.build_metadata(&descriptor, &object_key);
        let job = UploadJob {
            descriptor,
            object_key,
            metadata,
            security: self.config.security.clone(),
            inflight_key: key,
        };
        if let Err(err) = self.command_tx.send(Tier2Command::Upload(job)) {
            let mut inflight = self.inflight_uploads.lock();
            inflight.remove(&key);
            return Err(AofError::InternalError(err.to_string()));
        }
        self.metrics.incr_upload_queue();
        Ok(())
    }

    fn schedule_delete(&self, request: Tier2DeleteRequest) -> AofResult<()> {
        self.command_tx
            .send(Tier2Command::Delete(DeleteJob { request }))
            .map_err(|err| AofError::InternalError(err.to_string()))?;
        self.metrics.incr_delete_queue();
        Ok(())
    }

    fn schedule_fetch(&self, request: Tier2FetchRequest) -> AofResult<()> {
        let key = (request.instance_id, request.segment_id);
        {
            let inflight = self.inflight_fetches.lock();
            if inflight.contains(&key) {
                trace!(
                    instance = request.instance_id.get(),
                    segment = request.segment_id.as_u64(),
                    "tier2 fetch already in flight"
                );
                return Ok(());
            }
        }
        {
            let mut inflight = self.inflight_fetches.lock();
            inflight.insert(key);
        }
        if let Err(err) = self
            .command_tx
            .send(Tier2Command::Fetch(FetchJob { request }))
        {
            let mut inflight = self.inflight_fetches.lock();
            inflight.remove(&key);
            return Err(AofError::InternalError(err.to_string()));
        }
        Ok(())
    }

    fn process_retention(&self) {
        let Some(_ttl) = self.config.retention_ttl else {
            return;
        };
        let now = Instant::now();
        let mut pending = self.retention.lock();
        while let Some(entry) = pending.front() {
            if entry.deadline > now {
                break;
            }
            let entry = pending.pop_front().expect("entry present");
            drop(pending);
            if let Err(err) = self.schedule_delete(entry.request) {
                warn!("failed to schedule retention delete: {err}");
                break;
            }
            pending = self.retention.lock();
        }
    }

    async fn run(self: Arc<Self>, mut commands: mpsc::UnboundedReceiver<Tier2Command>) {
        while let Some(command) = commands.recv().await {
            match command {
                Tier2Command::Upload(job) => self.spawn_upload(job),
                Tier2Command::Delete(job) => self.spawn_delete(job),
                Tier2Command::Fetch(job) => self.spawn_fetch(job),
            }
        }
    }

    fn spawn_upload(self: &Arc<Self>, job: UploadJob) {
        let inner = Arc::clone(self);
        self.runtime.spawn(async move {
            let ops_permit = match inner.ops_semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => return,
            };
            let upload_permit = match inner.upload_semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => return,
            };
            let _ops = ops_permit;
            let _upload = upload_permit;
            inner.handle_upload(job).await;
        });
    }

    fn spawn_delete(self: &Arc<Self>, job: DeleteJob) {
        let inner = Arc::clone(self);
        self.runtime.spawn(async move {
            let ops_permit = match inner.ops_semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => return,
            };
            let _ops = ops_permit;
            inner.handle_delete(job).await;
        });
    }

    fn spawn_fetch(self: &Arc<Self>, job: FetchJob) {
        let inner = Arc::clone(self);
        self.runtime.spawn(async move {
            let ops_permit = match inner.ops_semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => return,
            };
            let download_permit = match inner.download_semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => return,
            };
            let _ops = ops_permit;
            let _download = download_permit;
            inner.handle_fetch(job).await;
        });
    }

    async fn handle_upload(self: Arc<Self>, job: UploadJob) {
        let UploadJob {
            descriptor,
            object_key,
            metadata,
            security,
            inflight_key,
        } = job;
        let result = self
            .perform_upload(&descriptor, &object_key, &metadata, security.clone())
            .await;
        match result {
            Ok(stats) => {
                let attempts = stats.attempts;
                let complete = stats.value;
                tracing::event!(
                    target: "aof2::tier2",
                    tracing::Level::INFO,
                    instance = descriptor.instance_id.get(),
                    segment = descriptor.segment_id.as_u64(),
                    attempts,
                    object_key = %complete.metadata.object_key,
                    compressed_bytes = descriptor.compressed_bytes,
                    original_bytes = descriptor.original_bytes,
                    "tier2_upload_success"
                );
                if let Err(err) = self.events_tx.send(Tier2Event::UploadCompleted(complete)) {
                    tracing::event!(
                        target: "aof2::tier2",
                        tracing::Level::WARN,
                        instance = descriptor.instance_id.get(),
                        segment = descriptor.segment_id.as_u64(),
                        error = %err,
                        "tier2_upload_event_send_failed"
                    );
                }
                if let Some(ttl) = self.config.retention_ttl {
                    if let Ok(entry) = self.build_retention_entry(&descriptor, &object_key, ttl) {
                        self.retention.lock().push_back(entry);
                    }
                }
            }
            Err(err) => {
                self.metrics.incr_upload_failure();
                let attempts = err.attempts;
                let error = err.error;
                tracing::event!(
                    target: "aof2::tier2",
                    tracing::Level::WARN,
                    instance = descriptor.instance_id.get(),
                    segment = descriptor.segment_id.as_u64(),
                    attempts,
                    error = %error,
                    "tier2_upload_failed"
                );
                if let Err(send_err) =
                    self.events_tx
                        .send(Tier2Event::UploadFailed(Tier2UploadFailed {
                            instance_id: descriptor.instance_id,
                            segment_id: descriptor.segment_id,
                            error,
                        }))
                {
                    tracing::event!(
                        target: "aof2::tier2",
                        tracing::Level::WARN,
                        instance = descriptor.instance_id.get(),
                        segment = descriptor.segment_id.as_u64(),
                        error = %send_err,
                        "tier2_upload_failure_event_send_failed"
                    );
                }
            }
        }
        self.metrics.decr_upload_queue();
        let mut inflight = self.inflight_uploads.lock();
        inflight.remove(&inflight_key);
    }

    async fn handle_fetch(self: Arc<Self>, job: FetchJob) {
        let request = job.request;
        let key = (request.instance_id, request.segment_id);
        let result = self.fetch_segment(request.clone()).await;
        match result {
            Ok(stats) => {
                tracing::event!(
                    target: "aof2::tier2",
                    tracing::Level::INFO,
                    instance = request.instance_id.get(),
                    segment = request.segment_id.as_u64(),
                    attempts = stats.attempts,
                    destination = %request.destination.display(),
                    "tier2_fetch_success"
                );
                if let Err(err) =
                    self.events_tx
                        .send(Tier2Event::FetchCompleted(Tier2FetchComplete {
                            instance_id: request.instance_id,
                            segment_id: request.segment_id,
                            destination: request.destination.clone(),
                        }))
                {
                    tracing::event!(
                        target: "aof2::tier2",
                        tracing::Level::WARN,
                        instance = request.instance_id.get(),
                        segment = request.segment_id.as_u64(),
                        error = %err,
                        "tier2_fetch_event_send_failed"
                    );
                }
            }
            Err(err) => {
                let attempts = err.attempts;
                let error = err.error;
                tracing::event!(
                    target: "aof2::tier2",
                    tracing::Level::WARN,
                    instance = request.instance_id.get(),
                    segment = request.segment_id.as_u64(),
                    attempts,
                    error = %error,
                    "tier2_fetch_failed"
                );
                if let Err(send_err) =
                    self.events_tx
                        .send(Tier2Event::FetchFailed(Tier2FetchFailed {
                            instance_id: request.instance_id,
                            segment_id: request.segment_id,
                            error,
                        }))
                {
                    tracing::event!(
                        target: "aof2::tier2",
                        tracing::Level::WARN,
                        instance = request.instance_id.get(),
                        segment = request.segment_id.as_u64(),
                        error = %send_err,
                        "tier2_fetch_failure_event_send_failed"
                    );
                }
            }
        }
        let mut inflight = self.inflight_fetches.lock();
        inflight.remove(&key);
    }

    async fn perform_upload(
        &self,
        descriptor: &Tier2UploadDescriptor,
        object_key: &str,
        metadata: &HashMap<String, String>,
        security: Option<Tier2Security>,
    ) -> Result<RetryStats<Tier2UploadComplete>, RetryError> {
        self.metrics.incr_upload_attempt();
        let bucket = self.config.bucket.clone();
        let head_stats = match self
            .with_retry(RetryKind::Upload, || {
                let client = Arc::clone(&self.client);
                let bucket = bucket.clone();
                let key = object_key.to_string();
                async move { client.head_object(&bucket, &key).await }
            })
            .await
        {
            Ok(stats) => stats,
            Err(err) => return Err(err),
        };
        if let Some(existing) = head_stats.value {
            if self.remote_matches(&existing, descriptor) {
                let HeadObjectResult { size, etag, .. } = existing;
                trace!(
                    instance = descriptor.instance_id.get(),
                    segment = descriptor.segment_id.as_u64(),
                    "tier2 upload skipped due to matching remote"
                );
                return Ok(RetryStats {
                    attempts: head_stats.attempts,
                    value: Tier2UploadComplete {
                        instance_id: descriptor.instance_id,
                        segment_id: descriptor.segment_id,
                        metadata: Tier2Metadata::new(object_key.to_string(), etag, size),
                        base_offset: descriptor.base_offset,
                        base_record_count: descriptor.base_record_count,
                        checksum: descriptor.checksum,
                    },
                });
            }
        }
        let put_stats = match self
            .with_retry(RetryKind::Upload, || {
                let client = Arc::clone(&self.client);
                let bucket = bucket.clone();
                let key = object_key.to_string();
                let metadata = metadata.clone();
                let security = security.clone();
                let source = descriptor.warm_path.clone();
                async move {
                    client
                        .put_object(PutObjectRequest {
                            bucket: bucket.clone(),
                            key: key.clone(),
                            source,
                            metadata,
                            security,
                        })
                        .await
                }
            })
            .await
        {
            Ok(stats) => stats,
            Err(err) => return Err(err),
        };
        let PutObjectResult { etag, size } = put_stats.value;
        Ok(RetryStats {
            attempts: put_stats.attempts,
            value: Tier2UploadComplete {
                instance_id: descriptor.instance_id,
                segment_id: descriptor.segment_id,
                metadata: Tier2Metadata::new(object_key.to_string(), etag, size),
                base_offset: descriptor.base_offset,
                base_record_count: descriptor.base_record_count,
                checksum: descriptor.checksum,
            },
        })
    }

    async fn handle_delete(self: Arc<Self>, job: DeleteJob) {
        let DeleteJob { request } = job;
        self.metrics.incr_delete();
        let bucket = self.config.bucket.clone();
        match self
            .with_retry(RetryKind::Delete, || {
                let client = Arc::clone(&self.client);
                let bucket = bucket.clone();
                let key = request.object_key.clone();
                async move { client.delete_object(&bucket, &key).await }
            })
            .await
        {
            Ok(stats) => {
                tracing::event!(
                    target: "aof2::tier2",
                    tracing::Level::INFO,
                    instance = request.instance_id.get(),
                    segment = request.segment_id.as_u64(),
                    attempts = stats.attempts,
                    object_key = %request.object_key,
                    "tier2_delete_success"
                );
                if let Err(err) =
                    self.events_tx
                        .send(Tier2Event::DeleteCompleted(Tier2DeleteComplete {
                            instance_id: request.instance_id,
                            segment_id: request.segment_id,
                            object_key: request.object_key,
                        }))
                {
                    tracing::event!(
                        target: "aof2::tier2",
                        tracing::Level::WARN,
                        instance = request.instance_id.get(),
                        segment = request.segment_id.as_u64(),
                        error = %err,
                        "tier2_delete_event_send_failed"
                    );
                }
            }
            Err(err) => {
                self.metrics.incr_delete_failure();
                let attempts = err.attempts;
                let error = err.error;
                tracing::event!(
                    target: "aof2::tier2",
                    tracing::Level::WARN,
                    instance = request.instance_id.get(),
                    segment = request.segment_id.as_u64(),
                    attempts,
                    object_key = %request.object_key,
                    error = %error,
                    "tier2_delete_failed"
                );
                if let Err(send_err) =
                    self.events_tx
                        .send(Tier2Event::DeleteFailed(Tier2DeleteFailed {
                            instance_id: request.instance_id,
                            segment_id: request.segment_id,
                            object_key: request.object_key,
                            error,
                        }))
                {
                    tracing::event!(
                        target: "aof2::tier2",
                        tracing::Level::WARN,
                        instance = request.instance_id.get(),
                        segment = request.segment_id.as_u64(),
                        error = %send_err,
                        "tier2_delete_failure_event_send_failed"
                    );
                }
            }
        }
        self.metrics.decr_delete_queue();
    }

    async fn fetch_segment(
        &self,
        request: Tier2FetchRequest,
    ) -> Result<RetryStats<bool>, RetryError> {
        if request.destination.exists() {
            return Ok(RetryStats {
                value: true,
                attempts: 0,
            });
        }
        let object_key =
            self.config
                .object_key(request.instance_id, request.segment_id, request.sealed_at);
        let temp = request.destination.with_extension("download");
        let bucket = self.config.bucket.clone();
        let result = self
            .with_retry(RetryKind::Fetch, || {
                let client = Arc::clone(&self.client);
                let bucket = bucket.clone();
                let key = object_key.clone();
                let destination = temp.clone();
                let security = self.config.security.clone();
                async move {
                    client
                        .get_object(GetObjectRequest {
                            bucket: bucket.clone(),
                            key: key.clone(),
                            destination,
                            security,
                        })
                        .await
                }
            })
            .await;
        match result {
            Ok(stats) => {
                let attempts = stats.attempts;
                let _response = stats.value;
                tokio::fs::rename(&temp, &request.destination)
                    .await
                    .map_err(AofError::from)
                    .map_err(|error| RetryError { error, attempts })?;
                self.metrics.incr_download();
                Ok(RetryStats {
                    value: true,
                    attempts,
                })
            }
            Err(err) => {
                self.metrics.incr_download_failure();
                let _ = tokio::fs::remove_file(&temp).await;
                Err(err)
            }
        }
    }

    fn remote_matches(&self, head: &HeadObjectResult, descriptor: &Tier2UploadDescriptor) -> bool {
        if head.size != descriptor.compressed_bytes {
            return false;
        }
        match head.metadata.get("checksum") {
            Some(value) => value == &descriptor.checksum.to_string(),
            None => false,
        }
    }

    fn build_retention_entry(
        &self,
        descriptor: &Tier2UploadDescriptor,
        object_key: &str,
        ttl: Duration,
    ) -> AofResult<RetentionEntry> {
        Ok(RetentionEntry {
            request: Tier2DeleteRequest {
                instance_id: descriptor.instance_id,
                segment_id: descriptor.segment_id,
                object_key: object_key.to_string(),
            },
            deadline: Instant::now() + ttl,
        })
    }

    async fn with_retry<F, Fut, T>(
        &self,
        kind: RetryKind,
        mut op: F,
    ) -> Result<RetryStats<T>, RetryError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = AofResult<T>>,
    {
        let mut attempts = 0;
        loop {
            attempts += 1;
            match op().await {
                Ok(result) => {
                    return Ok(RetryStats {
                        value: result,
                        attempts,
                    });
                }
                Err(err) => {
                    if attempts >= self.retry.max_attempts {
                        self.metrics.record_retry_failure(kind);
                        return Err(RetryError {
                            error: err,
                            attempts,
                        });
                    }
                    self.metrics.record_retry_attempt(kind);
                    self.retry.sleep_for(attempts).await;
                }
            }
        }
    }
}

/// The Tier 2 remote storage manager.
///
/// Coordinates all remote storage operations including uploads,
/// downloads, deletions, and event processing.
///
/// ## Architecture
///
/// ```text
/// ┌─────────────────────────────────────────────┐
/// │              Tier2Manager                   │
/// ├─────────────────────────────────────────────┤
/// │ • Upload/Download Coordination              │
/// │ • Retry Logic & Error Handling              │
/// │ • Concurrency Control                       │
/// │ • Event Generation                          │
/// │ • Client Abstraction                        │
/// └─────────────────────────────────────────────┘
/// ```
pub struct Tier2Manager {
    /// Core implementation and worker coordination
    inner: Arc<Tier2Inner>,
    /// Event receiver for operation results
    events_rx: Mutex<mpsc::UnboundedReceiver<Tier2Event>>,
    /// Background dispatcher task handle
    dispatcher: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

/// Handle for submitting operations to Tier 2 storage.
///
/// Provides a cloneable interface for submitting upload, download,
/// and delete requests to the remote storage system.
#[derive(Clone)]
pub struct Tier2Handle {
    /// Shared implementation
    inner: Arc<Tier2Inner>,
    /// Runtime handle for async operations
    runtime: Handle,
}

impl std::fmt::Debug for Tier2Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tier2Handle").finish_non_exhaustive()
    }
}

impl Tier2Manager {
    pub fn new(runtime: Handle, config: Tier2Config) -> AofResult<Self> {
        let client: Arc<dyn Tier2Client> = Arc::new(S3Tier2Client::new(&config)?);
        Self::with_client(runtime, config, client)
    }

    pub fn with_client(
        runtime: Handle,
        config: Tier2Config,
        client: Arc<dyn Tier2Client>,
    ) -> AofResult<Self> {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let ops_semaphore = Arc::new(Semaphore::new(config.max_concurrent_operations.max(1)));
        let upload_semaphore = Arc::new(Semaphore::new(config.max_concurrent_uploads.max(1)));
        let download_semaphore = Arc::new(Semaphore::new(config.max_concurrent_downloads.max(1)));
        let inner = Arc::new(Tier2Inner {
            runtime: runtime.clone(),
            config: config.clone(),
            client,
            command_tx,
            events_tx,
            retry: config.retry,
            retention: Mutex::new(VecDeque::new()),
            inflight_uploads: Mutex::new(HashSet::new()),
            inflight_fetches: Mutex::new(HashSet::new()),
            metrics: Tier2Metrics::default(),
            ops_semaphore,
            upload_semaphore,
            download_semaphore,
        });
        let dispatcher_inner = Arc::clone(&inner);
        let dispatcher = runtime.spawn(async move {
            dispatcher_inner.run(command_rx).await;
        });
        Ok(Self {
            inner,
            events_rx: Mutex::new(events_rx),
            dispatcher: Mutex::new(Some(dispatcher)),
        })
    }

    pub fn handle(&self) -> Tier2Handle {
        Tier2Handle {
            inner: self.inner.clone(),
            runtime: self.inner.runtime.clone(),
        }
    }

    pub fn drain_events(&self) -> Vec<Tier2Event> {
        let mut events = Vec::new();
        let receiver = &mut *self.events_rx.lock();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }
        events
    }

    pub fn process_retention(&self) {
        self.inner.process_retention();
    }

    pub fn metrics(&self) -> Tier2MetricsSnapshot {
        self.inner.metrics.snapshot()
    }
}

impl Drop for Tier2Manager {
    fn drop(&mut self) {
        if let Some(handle) = self.dispatcher.lock().take() {
            handle.abort();
        }
    }
}

impl Tier2Handle {
    pub fn schedule_upload(&self, descriptor: Tier2UploadDescriptor) -> AofResult<()> {
        self.inner.schedule_upload(descriptor)
    }

    pub fn schedule_delete(&self, request: Tier2DeleteRequest) -> AofResult<()> {
        self.inner.schedule_delete(request)
    }

    pub fn schedule_fetch(&self, request: Tier2FetchRequest) -> AofResult<()> {
        self.inner.schedule_fetch(request)
    }

    pub fn fetch_segment(&self, request: Tier2FetchRequest) -> AofResult<bool> {
        match self.runtime.block_on(self.inner.fetch_segment(request)) {
            Ok(stats) => Ok(stats.value),
            Err(err) => Err(err.error),
        }
    }

    pub async fn fetch_segment_async(&self, request: Tier2FetchRequest) -> AofResult<bool> {
        match self.inner.fetch_segment(request).await {
            Ok(stats) => Ok(stats.value),
            Err(err) => Err(err.error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as StdHashMap;
    use std::fmt;
    use std::sync::{Arc, Mutex as StdMutex};
    use std::time::Duration;

    use tempfile::tempdir;
    use tracing::Subscriber;
    use tracing::field::{Field, Visit};
    use tracing_subscriber::{
        Layer, Registry, layer::Context, layer::SubscriberExt, registry::LookupSpan,
    };

    #[derive(Clone, Debug)]
    struct CapturedEvent {
        fields: StdHashMap<String, String>,
    }

    #[derive(Clone)]
    struct RecordingLayer {
        events: Arc<StdMutex<Vec<CapturedEvent>>>,
    }

    impl RecordingLayer {
        fn new(events: Arc<StdMutex<Vec<CapturedEvent>>>) -> Self {
            Self { events }
        }
    }

    struct FieldVisitor<'a> {
        fields: &'a mut StdHashMap<String, String>,
    }

    impl<'a> FieldVisitor<'a> {
        fn normalize_debug_value(value: &str) -> Option<String> {
            if !value.starts_with('"') || !value.ends_with('"') {
                return None;
            }
            serde_json::from_str::<String>(value).ok()
        }
    }

    impl<'a> Visit for FieldVisitor<'a> {
        fn record_i64(&mut self, field: &Field, value: i64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_u64(&mut self, field: &Field, value: u64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_u128(&mut self, field: &Field, value: u128) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_bool(&mut self, field: &Field, value: bool) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
            let mut rendered = format!("{:?}", value);
            if let Some(unescaped) = FieldVisitor::normalize_debug_value(&rendered) {
                rendered = unescaped;
            }
            self.fields.insert(field.name().to_string(), rendered);
        }
    }

    impl<S> Layer<S> for RecordingLayer
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut captured = CapturedEvent {
                fields: StdHashMap::new(),
            };
            event.record(&mut FieldVisitor {
                fields: &mut captured.fields,
            });
            if let Ok(mut events) = self.events.lock() {
                events.push(captured);
            }
        }
    }

    #[derive(Default)]
    struct StoredObject {
        bytes: Vec<u8>,
        metadata: HashMap<String, String>,
    }

    #[derive(Default, Clone)]
    struct MockTier2Client {
        store: Arc<StdMutex<StdHashMap<String, StoredObject>>>,
        put_calls: Arc<StdMutex<u32>>,
        get_calls: Arc<StdMutex<u32>>,
    }

    impl MockTier2Client {
        fn put_count(&self) -> u32 {
            *self.put_calls.lock().unwrap()
        }

        fn get_count(&self) -> u32 {
            *self.get_calls.lock().unwrap()
        }
    }

    struct AlwaysFailTier2Client;

    impl Tier2Client for MockTier2Client {
        fn put_object(
            &self,
            request: PutObjectRequest,
        ) -> BoxFuture<'_, AofResult<PutObjectResult>> {
            let store = &self.store;
            let put_calls = &self.put_calls;
            Box::pin(async move {
                let bytes = tokio::fs::read(&request.source)
                    .await
                    .map_err(AofError::from)?;
                let mut guard = store.lock().unwrap();
                guard.insert(
                    request.key.clone(),
                    StoredObject {
                        bytes: bytes.clone(),
                        metadata: request.metadata.clone(),
                    },
                );
                *put_calls.lock().unwrap() += 1;
                Ok(PutObjectResult {
                    etag: Some(format!("etag-{}", request.key)),
                    size: bytes.len() as u64,
                })
            })
        }

        fn get_object(
            &self,
            request: GetObjectRequest,
        ) -> BoxFuture<'_, AofResult<GetObjectResult>> {
            let store = &self.store;
            let get_calls = &self.get_calls;
            Box::pin(async move {
                let (bytes, size) = {
                    let guard = store.lock().unwrap();
                    let object = guard.get(&request.key).ok_or_else(|| {
                        AofError::RemoteStorage(format!("missing object {}", request.key))
                    })?;
                    (object.bytes.clone(), object.bytes.len() as u64)
                };
                if let Some(parent) = request.destination.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(AofError::from)?;
                }
                tokio::fs::write(&request.destination, &bytes)
                    .await
                    .map_err(AofError::from)?;
                *get_calls.lock().unwrap() += 1;
                Ok(GetObjectResult {
                    etag: Some(format!("etag-{}", request.key)),
                    size,
                })
            })
        }

        fn delete_object(&self, _bucket: &str, key: &str) -> BoxFuture<'_, AofResult<()>> {
            let store = &self.store;
            let key = key.to_owned();
            Box::pin(async move {
                store.lock().unwrap().remove(&key);
                Ok(())
            })
        }

        fn head_object(
            &self,
            _bucket: &str,
            key: &str,
        ) -> BoxFuture<'_, AofResult<Option<HeadObjectResult>>> {
            let store = &self.store;
            let key = key.to_owned();
            Box::pin(async move {
                let result = store
                    .lock()
                    .unwrap()
                    .get(&key)
                    .map(|object| HeadObjectResult {
                        size: object.bytes.len() as u64,
                        etag: Some(format!("etag-{key}")),
                        metadata: object.metadata.clone(),
                    });
                Ok(result)
            })
        }
    }

    impl Tier2Client for AlwaysFailTier2Client {
        fn put_object(
            &self,
            _request: PutObjectRequest,
        ) -> BoxFuture<'_, AofResult<PutObjectResult>> {
            Box::pin(async { Err(AofError::RemoteStorage("put failed".to_string())) })
        }

        fn get_object(
            &self,
            _request: GetObjectRequest,
        ) -> BoxFuture<'_, AofResult<GetObjectResult>> {
            Box::pin(async { Err(AofError::RemoteStorage("get failed".to_string())) })
        }

        fn delete_object(&self, _bucket: &str, _key: &str) -> BoxFuture<'_, AofResult<()>> {
            Box::pin(async { Err(AofError::RemoteStorage("delete failed".to_string())) })
        }

        fn head_object(
            &self,
            _bucket: &str,
            _key: &str,
        ) -> BoxFuture<'_, AofResult<Option<HeadObjectResult>>> {
            Box::pin(async { Ok(None) })
        }
    }

    #[tokio::test]
    async fn upload_idempotent_and_fetch() {
        let temp = tempdir().expect("tempdir");
        let warm_path = temp.path().join("segment.zst");
        tokio::fs::write(&warm_path, b"payload")
            .await
            .expect("write warm");

        let mut config = Tier2Config::new(
            "http://localhost:9000",
            "us-east-1",
            "bucket",
            "access",
            "secret",
        )
        .with_concurrency(1)
        .with_retention_ttl(None);
        config.prefix = "test".to_string();

        let client = Arc::new(MockTier2Client::default());
        let runtime = Handle::current();
        let manager = Tier2Manager::with_client(runtime.clone(), config.clone(), client.clone())
            .expect("manager");
        let handle = manager.handle();

        let descriptor = Tier2UploadDescriptor {
            instance_id: InstanceId::new(1),
            segment_id: SegmentId::new(42),
            warm_path: warm_path.clone(),
            sealed_at: 123,
            base_offset: 0,
            base_record_count: 1,
            checksum: 99,
            compressed_bytes: 7,
            original_bytes: 7,
        };

        handle.schedule_upload(descriptor.clone()).expect("upload");

        wait_for(|| !manager.drain_events().is_empty()).await;
        assert_eq!(client.put_count(), 1);

        handle
            .schedule_upload(descriptor.clone())
            .expect("upload again");
        wait_for(|| {
            let events = manager.drain_events();
            events
                .iter()
                .any(|event| matches!(event, Tier2Event::UploadCompleted(_)))
        })
        .await;
        assert_eq!(client.put_count(), 1, "second upload should be idempotent");

        tokio::fs::remove_file(&warm_path)
            .await
            .expect("remove warm");
        let fetch_request = Tier2FetchRequest {
            instance_id: descriptor.instance_id,
            segment_id: descriptor.segment_id,
            sealed_at: descriptor.sealed_at,
            destination: warm_path.clone(),
        };
        assert!(
            handle
                .fetch_segment_async(fetch_request)
                .await
                .expect("fetch")
        );
        assert!(warm_path.exists());
        assert_eq!(client.get_count(), 1);
    }

    #[tokio::test]
    async fn upload_success_emits_structured_event() {
        let temp = tempdir().expect("tempdir");
        let warm_path = temp.path().join("segment.zst");
        tokio::fs::write(&warm_path, b"payload")
            .await
            .expect("write warm");

        let mut config = Tier2Config::new(
            "http://localhost:9000",
            "us-east-1",
            "bucket",
            "access",
            "secret",
        )
        .with_concurrency(1)
        .with_retention_ttl(None);
        config.retry = RetryPolicy::new(3, Duration::from_millis(0));
        config.prefix = "logs".to_string();

        let client = Arc::new(MockTier2Client::default());
        let events = Arc::new(StdMutex::new(Vec::new()));
        let subscriber = Registry::default().with(RecordingLayer::new(events.clone()));
        let _guard = tracing::subscriber::set_default(subscriber);

        let runtime = Handle::current();
        let manager = Tier2Manager::with_client(runtime.clone(), config.clone(), client.clone())
            .expect("manager");
        let inner = manager.inner.clone();
        let instance_id = InstanceId::new(11);
        let segment_id = SegmentId::new(7);
        let sealed_at = 33;
        let descriptor = Tier2UploadDescriptor {
            instance_id,
            segment_id,
            warm_path: warm_path.clone(),
            sealed_at,
            base_offset: 0,
            base_record_count: 0,
            checksum: 0xABCD,
            compressed_bytes: 7,
            original_bytes: 7,
        };
        let object_key = inner.config.object_key(instance_id, segment_id, sealed_at);
        let metadata = inner.build_metadata(&descriptor, &object_key);
        let job = UploadJob {
            descriptor,
            object_key,
            metadata,
            security: inner.config.security.clone(),
            inflight_key: (instance_id, segment_id),
        };
        inner.handle_upload(job).await;

        let recorded = events.lock().expect("events lock");
        let upload_event = recorded
            .iter()
            .find(|event| {
                event.fields.get("message").map(String::as_str) == Some("tier2_upload_success")
            })
            .expect("upload success event");
        assert_eq!(
            upload_event.fields.get("attempts").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            upload_event
                .fields
                .get("compressed_bytes")
                .map(String::as_str),
            Some("7")
        );
    }

    #[tokio::test]
    async fn upload_failure_emits_attempt_count() {
        let temp = tempdir().expect("tempdir");
        let warm_path = temp.path().join("segment.zst");
        tokio::fs::write(&warm_path, b"payload")
            .await
            .expect("write warm");

        let mut config = Tier2Config::new(
            "http://localhost:9000",
            "us-east-1",
            "bucket",
            "access",
            "secret",
        )
        .with_concurrency(1)
        .with_retention_ttl(None);
        config.retry = RetryPolicy::new(2, Duration::from_millis(0));
        config.prefix = "logs".to_string();

        let client = Arc::new(AlwaysFailTier2Client);
        let events = Arc::new(StdMutex::new(Vec::new()));
        let subscriber = Registry::default().with(RecordingLayer::new(events.clone()));
        let _guard = tracing::subscriber::set_default(subscriber);

        let runtime = Handle::current();
        let manager = Tier2Manager::with_client(runtime.clone(), config.clone(), client.clone())
            .expect("manager");
        let inner = manager.inner.clone();
        let instance_id = InstanceId::new(17);
        let segment_id = SegmentId::new(5);
        let sealed_at = 21;
        let descriptor = Tier2UploadDescriptor {
            instance_id,
            segment_id,
            warm_path: warm_path.clone(),
            sealed_at,
            base_offset: 0,
            base_record_count: 0,
            checksum: 0x1234,
            compressed_bytes: 7,
            original_bytes: 7,
        };
        let object_key = inner.config.object_key(instance_id, segment_id, sealed_at);
        let metadata = inner.build_metadata(&descriptor, &object_key);
        let job = UploadJob {
            descriptor,
            object_key,
            metadata,
            security: inner.config.security.clone(),
            inflight_key: (instance_id, segment_id),
        };
        inner.handle_upload(job).await;

        let recorded = events.lock().expect("events lock");
        let upload_event = recorded
            .iter()
            .find(|event| {
                event.fields.get("message").map(String::as_str) == Some("tier2_upload_failed")
            })
            .expect("upload failure event");
        assert_eq!(
            upload_event.fields.get("attempts").map(String::as_str),
            Some("2")
        );
    }

    async fn wait_for<F>(mut predicate: F)
    where
        F: FnMut() -> bool,
    {
        for _ in 0..50 {
            if predicate() {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not satisfied");
    }
}
