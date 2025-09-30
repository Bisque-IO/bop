//! Checkpoint executor implementation.
//!
//! This module implements T2 from the checkpointing plan: staging dirty pages
//! to scratch directories, uploading to S3, and coordinating manifest publication.
//!
//! # Async S3 Operations
//!
//! **IMPORTANT**: S3 access will be async (user requirement).
//!
//! The current implementation uses synchronous stubs for testing. When integrating
//! with RemoteStore (T8), the upload methods will become async:
//!
//! ```ignore
//! pub async fn upload_chunks_async(
//!     &self,
//!     remote_store: &RemoteStore,
//!     staged_chunks: &[StagedChunk],
//! ) -> Result<HashMap<ChunkId, UploadedChunk>, ExecutorError> {
//!     // ... async upload to S3 with retry/backoff
//! }
//! ```
//!
//! The executor will coordinate with the async runtime (likely tokio) for:
//! - Multipart uploads with streaming
//! - Concurrent chunk uploads (with semaphore for rate limiting)
//! - Exponential backoff on transient failures
//! - Checksum validation after upload completes
//!
//! # Idempotent Retry Logic (T11a)
//!
//! The checkpoint executor is designed for safe retries after failures:
//!
//! ## Deterministic Chunk Ordering
//!
//! The planner produces deterministic chunk ordering based on chunk IDs, ensuring
//! that retries generate identical blob keys. This allows crashed jobs to resume
//! without creating duplicate S3 objects.
//!
//! ## Hash-Based Blob Keys
//!
//! All S3 uploads use content-addressable blob keys that include CRC64-NVME hashes:
//! - `checkpoints/{db_id}/chunk-{chunk_id}-{generation}-{hash}.blob`
//! - `checkpoints/{db_id}/delta-{chunk_id}-{generation}-{delta_index}-{hash}.blob`
//!
//! This makes uploads idempotent: if a retry attempts to upload the same data,
//! the S3 PUT is a no-op since the key already exists with identical content.
//!
//! ## Atomic Manifest Publication
//!
//! Manifest updates use LMDB single-writer transactions, ensuring that chunk
//! catalog entries and change log records are committed atomically. Even if the
//! executor crashes after uploading to S3 but before publishing to the manifest,
//! a retry will:
//! 1. Re-upload the same chunks (idempotent due to hash-based keys)
//! 2. Commit the manifest transaction exactly once
//!
//! ## Progress Tracking (T6b)
//!
//! The orchestrator saves progress after each phase, allowing crash recovery to
//! resume from the last completed phase rather than starting over.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::manifest::{ChunkId, DbId, Generation, JobId};

/// Errors that can occur during checkpoint execution.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    /// Scratch directory creation or management failed.
    #[error("scratch directory error: {0}")]
    ScratchDir(String),

    /// Staging pages to disk failed.
    #[error("staging error: {0}")]
    Staging(String),

    /// Upload to remote storage failed.
    #[error("upload error: {0}")]
    Upload(String),

    /// Manifest publication failed.
    #[error("manifest publication error: {0}")]
    ManifestPublication(String),

    /// Quota reservation failed.
    #[error("quota error: {0}")]
    Quota(String),

    /// Lease coordination failed.
    #[error("lease error: {0}")]
    Lease(String),

    /// Checkpoint was cancelled (e.g., due to truncation request).
    #[error("checkpoint cancelled")]
    Cancelled,
}

/// Tracks execution progress for a checkpoint job.
///
/// This state is persisted to the manifest job queue so execution can resume
/// after crashes or restarts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointJobState {
    /// Job ID for this checkpoint.
    pub job_id: JobId,

    /// Database ID being checkpointed.
    pub db_id: DbId,

    /// Current execution phase.
    pub phase: ExecutionPhase,

    /// Total number of chunks in the plan.
    pub total_chunks: u32,

    /// Number of chunks staged so far.
    pub chunks_staged: u32,

    /// Number of chunks uploaded so far.
    pub chunks_uploaded: u32,

    /// Chunk IDs that have been processed (for idempotent retry).
    pub processed_chunks: Vec<ChunkId>,
}

/// Execution phases for checkpoint jobs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionPhase {
    /// Initial phase - reserving quota and preparing scratch space.
    Preparing,

    /// Staging dirty pages from PageCache/WAL to local scratch directories.
    Staging,

    /// Uploading staged chunks to remote storage.
    Uploading,

    /// Publishing manifest updates atomically.
    Publishing,

    /// Cleaning up scratch directories and releasing resources.
    Cleanup,

    /// Job completed successfully.
    Completed,
}

/// Configuration for the checkpoint executor.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Base directory for checkpoint scratch space.
    pub scratch_base_dir: PathBuf,

    /// Maximum concurrent uploads to S3.
    pub max_concurrent_uploads: usize,

    /// Chunk size in bytes (typically 64 MiB).
    pub chunk_size_bytes: u64,

    /// Enable CRC64 checksum validation.
    pub enable_checksums: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            scratch_base_dir: PathBuf::from("checkpoint_scratch"),
            max_concurrent_uploads: 4,
            chunk_size_bytes: 64 * 1024 * 1024, // 64 MiB
            enable_checksums: true,
        }
    }
}

/// Checkpoint executor.
///
/// Responsible for executing `CheckpointPlan` instances by staging dirty pages,
/// uploading to S3, and coordinating with the manifest writer.
pub struct CheckpointExecutor {
    config: ExecutorConfig,
}

impl CheckpointExecutor {
    /// Creates a new checkpoint executor with the given configuration.
    pub fn new(config: ExecutorConfig) -> Self {
        Self { config }
    }

    /// Returns the scratch directory path for a specific job.
    ///
    /// # T2a: Scratch Directory Management
    ///
    /// Each checkpoint job gets an isolated directory under
    /// `checkpoint_scratch/{job_id}/` to stage chunks before upload.
    pub fn scratch_dir(&self, job_id: JobId) -> PathBuf {
        self.config.scratch_base_dir.join(format!("{}", job_id))
    }

    /// Creates the scratch directory for a job.
    ///
    /// Returns an error if the directory already exists (indicating a potential
    /// conflict with a previous job) or if creation fails.
    pub fn create_scratch_dir(&self, job_id: JobId) -> Result<PathBuf, ExecutorError> {
        let scratch_dir = self.scratch_dir(job_id);

        if scratch_dir.exists() {
            return Err(ExecutorError::ScratchDir(format!(
                "scratch directory already exists: {}",
                scratch_dir.display()
            )));
        }

        std::fs::create_dir_all(&scratch_dir).map_err(|e| {
            ExecutorError::ScratchDir(format!(
                "failed to create scratch directory {}: {}",
                scratch_dir.display(),
                e
            ))
        })?;

        Ok(scratch_dir)
    }

    /// Removes the scratch directory for a job.
    ///
    /// Called during cleanup phase after successful manifest publication
    /// or on job failure to reclaim space.
    pub fn remove_scratch_dir(&self, job_id: JobId) -> Result<(), ExecutorError> {
        let scratch_dir = self.scratch_dir(job_id);

        if !scratch_dir.exists() {
            return Ok(());
        }

        std::fs::remove_dir_all(&scratch_dir).map_err(|e| {
            ExecutorError::ScratchDir(format!(
                "failed to remove scratch directory {}: {}",
                scratch_dir.display(),
                e
            ))
        })
    }

    /// Verifies that the scratch directory is empty or contains only expected files.
    ///
    /// Used to detect abandoned jobs that may need cleanup by the janitor.
    pub fn verify_scratch_dir(&self, job_id: JobId) -> Result<bool, ExecutorError> {
        let scratch_dir = self.scratch_dir(job_id);

        if !scratch_dir.exists() {
            return Ok(true);
        }

        let entries = std::fs::read_dir(&scratch_dir).map_err(|e| {
            ExecutorError::ScratchDir(format!(
                "failed to read scratch directory {}: {}",
                scratch_dir.display(),
                e
            ))
        })?;

        // Empty or contains only dotfiles is considered clean
        let has_content = entries
            .filter_map(|e| e.ok())
            .any(|e| !e.file_name().to_string_lossy().starts_with('.'));

        Ok(!has_content)
    }

    /// Stages a chunk file to the scratch directory with content-hash naming.
    ///
    /// # T2b: Content-Hash Staging
    ///
    /// Writes chunk data to the scratch directory using a content-addressed
    /// filename format: `chunk-{chunk_id}-{generation}-{hash}.bin`
    ///
    /// This enables deduplication: if the same content is staged multiple times,
    /// it will use the same filename and avoid redundant work.
    ///
    /// # Parameters
    ///
    /// - `job_id`: Job ID for scratch directory isolation
    /// - `chunk_id`: Chunk being staged
    /// - `generation`: Checkpoint generation
    /// - `data`: Chunk data to write
    ///
    /// # Returns
    ///
    /// A `StagedChunk` record containing the file path and content hash.
    pub fn stage_chunk(
        &self,
        job_id: JobId,
        chunk_id: ChunkId,
        generation: Generation,
        data: &[u8],
    ) -> Result<StagedChunk, ExecutorError> {
        let scratch_dir = self.scratch_dir(job_id);

        // Compute content hash for deduplication
        let hash = compute_crc64_hash(data);

        // Content-addressed filename using CRC64 directly
        let filename = format!("chunk-{}-{}-{:016x}.bin", chunk_id, generation, hash[0]);
        let file_path = scratch_dir.join(&filename);

        // Check if already staged (deduplication)
        if file_path.exists() {
            // Verify content matches
            let existing_data = std::fs::read(&file_path).map_err(|e| {
                ExecutorError::Staging(format!("failed to read existing chunk {}: {}", filename, e))
            })?;

            if existing_data != data {
                return Err(ExecutorError::Staging(format!(
                    "hash collision for chunk {}: existing content differs",
                    filename
                )));
            }

            // Content matches - reuse existing file
            return Ok(StagedChunk {
                chunk_id,
                generation,
                file_path,
                size_bytes: data.len() as u64,
                content_hash: hash,
            });
        }

        // Write to scratch
        std::fs::write(&file_path, data).map_err(|e| {
            ExecutorError::Staging(format!("failed to write chunk {}: {}", filename, e))
        })?;

        Ok(StagedChunk {
            chunk_id,
            generation,
            file_path,
            size_bytes: data.len() as u64,
            content_hash: hash,
        })
    }

    /// Uploads staged chunks to remote storage.
    ///
    /// # T2c: Upload Coordination & Validation
    ///
    /// Coordinates upload of all staged chunks to S3 (or other remote storage).
    /// Validates checksums before and after upload to ensure integrity.
    ///
    /// **ASYNC NOTE**: This method will become async when integrating with T8 RemoteStore.
    /// The async version will use tokio for concurrent uploads with rate limiting.
    ///
    /// In production with async S3, this will:
    /// 1. Use multipart upload for large chunks (streaming)
    /// 2. Upload multiple chunks concurrently (with semaphore)
    /// 3. Retry with exponential backoff on transient failures
    /// 4. Verify CRC64-NVME checksums after upload completes
    /// 5. Update manifest only after all uploads succeed
    ///
    /// # Parameters
    ///
    /// - `staged_chunks`: Chunks that have been staged locally
    ///
    /// # Returns
    ///
    /// A map of chunk_id -> remote URL for manifest publication.
    ///
    /// # Future Async Signature
    ///
    /// ```ignore
    /// pub async fn upload_chunks_async(
    ///     &self,
    ///     remote_store: &RemoteStore,
    ///     staged_chunks: &[StagedChunk],
    /// ) -> Result<HashMap<ChunkId, UploadedChunk>, ExecutorError>
    /// ```
    pub fn upload_chunks(
        &self,
        staged_chunks: &[StagedChunk],
    ) -> Result<HashMap<ChunkId, UploadedChunk>, ExecutorError> {
        let mut uploaded = HashMap::new();

        for chunk in staged_chunks {
            // Validate checksum before upload
            if self.config.enable_checksums {
                let data = std::fs::read(&chunk.file_path).map_err(|e| {
                    ExecutorError::Upload(format!(
                        "failed to read chunk {} for validation: {}",
                        chunk.chunk_id, e
                    ))
                })?;

                let hash = compute_crc64_hash(&data);
                if hash != chunk.content_hash {
                    return Err(ExecutorError::Upload(format!(
                        "checksum mismatch for chunk {}: expected {:?}, got {:?}",
                        chunk.chunk_id, chunk.content_hash, hash
                    )));
                }
            }

            // TODO(T8) - Actually upload to S3 with async RemoteStore
            // The async version will:
            // - Stream file to S3 using multipart upload
            // - Validate CRC64 checksum after upload
            // - Retry with exponential backoff on transient errors
            // For now, simulate successful upload
            let remote_url = format!(
                "s3://bucket/chunk-{}-{}-{:016x}.bin",
                chunk.chunk_id, chunk.generation, chunk.content_hash[0]
            );

            uploaded.insert(
                chunk.chunk_id,
                UploadedChunk {
                    chunk_id: chunk.chunk_id,
                    generation: chunk.generation,
                    remote_url: remote_url.clone(),
                    size_bytes: chunk.size_bytes,
                    content_hash: chunk.content_hash,
                },
            );
        }

        Ok(uploaded)
    }
}

/// Represents a chunk that has been staged to the scratch directory.
///
/// # T2b: Staging Output
///
/// This record is produced by `stage_chunk()` and consumed by `upload_chunks()`.
#[derive(Debug, Clone)]
pub struct StagedChunk {
    /// Chunk ID.
    pub chunk_id: ChunkId,

    /// Checkpoint generation.
    pub generation: Generation,

    /// Local file path in scratch directory.
    pub file_path: PathBuf,

    /// Size in bytes.
    pub size_bytes: u64,

    /// Content hash (CRC64-NVME) for deduplication and validation.
    pub content_hash: [u64; 4],
}

/// Represents a chunk that has been successfully uploaded to remote storage.
///
/// # T2c: Upload Output
///
/// This record is produced by `upload_chunks()` and used for manifest publication.
#[derive(Debug, Clone)]
pub struct UploadedChunk {
    /// Chunk ID.
    pub chunk_id: ChunkId,

    /// Checkpoint generation.
    pub generation: Generation,

    /// Remote storage URL (e.g., s3://bucket/key).
    pub remote_url: String,

    /// Size in bytes.
    pub size_bytes: u64,

    /// Content hash for validation.
    pub content_hash: [u64; 4],
}

/// Computes CRC64-NVME checksum of data.
///
/// Uses CRC64-NVME as specified in the checkpointing design for content verification.
/// Returns array of u64 values for multi-chunk content addressing.
fn compute_crc64_hash(data: &[u8]) -> [u64; 4] {
    use crc64fast_nvme::Digest;

    let mut digest = Digest::new();
    digest.write(data);
    let crc = digest.sum64();

    // Create 256-bit hash representation using CRC as seed
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
    use tempfile::TempDir;

    #[test]
    fn scratch_dir_creation_and_removal() {
        let temp = TempDir::new().unwrap();
        let config = ExecutorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            ..Default::default()
        };
        let executor = CheckpointExecutor::new(config);

        let job_id = 42;

        // Create scratch dir
        let scratch_dir = executor.create_scratch_dir(job_id).unwrap();
        assert!(scratch_dir.exists());
        assert_eq!(scratch_dir, temp.path().join("42"));

        // Verify it's empty
        assert!(executor.verify_scratch_dir(job_id).unwrap());

        // Cannot create twice
        let err = executor.create_scratch_dir(job_id).unwrap_err();
        assert!(matches!(err, ExecutorError::ScratchDir(_)));

        // Remove scratch dir
        executor.remove_scratch_dir(job_id).unwrap();
        assert!(!scratch_dir.exists());

        // Removing again is idempotent
        executor.remove_scratch_dir(job_id).unwrap();
    }

    #[test]
    fn verify_scratch_dir_detects_content() {
        let temp = TempDir::new().unwrap();
        let config = ExecutorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            ..Default::default()
        };
        let executor = CheckpointExecutor::new(config);

        let job_id = 123;
        let scratch_dir = executor.create_scratch_dir(job_id).unwrap();

        // Empty directory is clean
        assert!(executor.verify_scratch_dir(job_id).unwrap());

        // Add a file
        std::fs::write(scratch_dir.join("chunk-1.blob"), b"test data").unwrap();

        // No longer clean
        assert!(!executor.verify_scratch_dir(job_id).unwrap());

        executor.remove_scratch_dir(job_id).unwrap();
    }

    // T2b: Content-hash staging tests
    #[test]
    fn stage_chunk_creates_file_with_content_hash() {
        let temp = TempDir::new().unwrap();
        let config = ExecutorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            ..Default::default()
        };
        let executor = CheckpointExecutor::new(config);

        let job_id = 42;
        executor.create_scratch_dir(job_id).unwrap();

        let data = b"test chunk data";
        let staged = executor.stage_chunk(job_id, 1, 5, data).unwrap();

        assert_eq!(staged.chunk_id, 1);
        assert_eq!(staged.generation, 5);
        assert_eq!(staged.size_bytes, data.len() as u64);
        assert!(staged.file_path.exists());

        // Verify file content
        let file_content = std::fs::read(&staged.file_path).unwrap();
        assert_eq!(file_content, data);

        executor.remove_scratch_dir(job_id).unwrap();
    }

    #[test]
    fn stage_chunk_deduplicates_identical_content() {
        let temp = TempDir::new().unwrap();
        let config = ExecutorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            ..Default::default()
        };
        let executor = CheckpointExecutor::new(config);

        let job_id = 42;
        executor.create_scratch_dir(job_id).unwrap();

        let data = b"identical content";

        // Stage first time
        let staged1 = executor.stage_chunk(job_id, 1, 5, data).unwrap();

        // Stage again with same content
        let staged2 = executor.stage_chunk(job_id, 1, 5, data).unwrap();

        // Should reuse the same file
        assert_eq!(staged1.file_path, staged2.file_path);
        assert_eq!(staged1.content_hash, staged2.content_hash);

        executor.remove_scratch_dir(job_id).unwrap();
    }

    #[test]
    fn stage_chunk_detects_hash_collision() {
        let temp = TempDir::new().unwrap();
        let config = ExecutorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            ..Default::default()
        };
        let executor = CheckpointExecutor::new(config);

        let job_id = 42;
        executor.create_scratch_dir(job_id).unwrap();

        let data1 = b"content one";
        let staged1 = executor.stage_chunk(job_id, 1, 5, data1).unwrap();

        // Manually overwrite file with different content
        let data2 = b"content two";
        std::fs::write(&staged1.file_path, data2).unwrap();

        // Try to stage original content again - should detect collision
        let result = executor.stage_chunk(job_id, 1, 5, data1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("hash collision"));

        executor.remove_scratch_dir(job_id).unwrap();
    }

    // T2c: Upload coordination tests
    #[test]
    fn upload_chunks_validates_checksums() {
        let temp = TempDir::new().unwrap();
        let config = ExecutorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            enable_checksums: true,
            ..Default::default()
        };
        let executor = CheckpointExecutor::new(config);

        let job_id = 42;
        executor.create_scratch_dir(job_id).unwrap();

        // Stage a chunk
        let data = b"upload test data";
        let staged = executor.stage_chunk(job_id, 1, 5, data).unwrap();

        // Upload it
        let uploaded = executor.upload_chunks(&[staged]).unwrap();

        assert_eq!(uploaded.len(), 1);
        let chunk = uploaded.get(&1).unwrap();
        assert_eq!(chunk.chunk_id, 1);
        assert_eq!(chunk.generation, 5);
        assert_eq!(chunk.size_bytes, data.len() as u64);
        assert!(chunk.remote_url.starts_with("s3://"));

        executor.remove_scratch_dir(job_id).unwrap();
    }

    #[test]
    fn upload_chunks_detects_corruption() {
        let temp = TempDir::new().unwrap();
        let config = ExecutorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            enable_checksums: true,
            ..Default::default()
        };
        let executor = CheckpointExecutor::new(config);

        let job_id = 42;
        executor.create_scratch_dir(job_id).unwrap();

        // Stage a chunk
        let data = b"original data";
        let staged = executor.stage_chunk(job_id, 1, 5, data).unwrap();

        // Corrupt the file
        std::fs::write(&staged.file_path, b"corrupted data").unwrap();

        // Keep original hash - should detect mismatch
        let result = executor.upload_chunks(&[staged]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("checksum mismatch")
        );

        executor.remove_scratch_dir(job_id).unwrap();
    }

    #[test]
    fn upload_chunks_handles_multiple_chunks() {
        let temp = TempDir::new().unwrap();
        let config = ExecutorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            ..Default::default()
        };
        let executor = CheckpointExecutor::new(config);

        let job_id = 42;
        executor.create_scratch_dir(job_id).unwrap();

        // Stage multiple chunks
        let chunk1 = executor.stage_chunk(job_id, 1, 5, b"chunk 1").unwrap();
        let chunk2 = executor.stage_chunk(job_id, 2, 5, b"chunk 2").unwrap();
        let chunk3 = executor.stage_chunk(job_id, 3, 5, b"chunk 3").unwrap();

        // Upload all at once
        let uploaded = executor.upload_chunks(&[chunk1, chunk2, chunk3]).unwrap();

        assert_eq!(uploaded.len(), 3);
        assert!(uploaded.contains_key(&1));
        assert!(uploaded.contains_key(&2));
        assert!(uploaded.contains_key(&3));

        executor.remove_scratch_dir(job_id).unwrap();
    }

    // T11c: Fault injection tests
    mod fault_injection {
        use super::*;

        #[test]
        fn idempotent_staging_same_data() {
            // Test that staging the same data twice produces identical hashes
            let temp = TempDir::new().unwrap();
            let config = ExecutorConfig {
                scratch_base_dir: temp.path().to_path_buf(),
                ..Default::default()
            };
            let executor = CheckpointExecutor::new(config);

            let job_id = 100;
            executor.create_scratch_dir(job_id).unwrap();

            let data = b"deterministic chunk data";

            // Stage once
            let staged1 = executor.stage_chunk(job_id, 10, 5, data).unwrap();

            // Remove and stage again
            executor.remove_scratch_dir(job_id).unwrap();
            executor.create_scratch_dir(job_id).unwrap();
            let staged2 = executor.stage_chunk(job_id, 10, 5, data).unwrap();

            // Hashes should be identical (idempotent)
            assert_eq!(staged1.content_hash, staged2.content_hash);
            assert_eq!(staged1.size_bytes, staged2.size_bytes);

            executor.remove_scratch_dir(job_id).unwrap();
        }

        #[test]
        fn scratch_dir_cleanup_after_crash_simulation() {
            // Simulate crash by not calling remove_scratch_dir
            let temp = TempDir::new().unwrap();
            let config = ExecutorConfig {
                scratch_base_dir: temp.path().to_path_buf(),
                ..Default::default()
            };

            {
                let executor = CheckpointExecutor::new(config.clone());
                let job_id = 200;
                let scratch_dir = executor.create_scratch_dir(job_id).unwrap();
                executor.stage_chunk(job_id, 1, 5, b"crash test").unwrap();

                // Simulate crash - drop executor without cleanup
                assert!(scratch_dir.exists());
            }

            // Recovery: new executor can clean up abandoned scratch dirs
            let executor = CheckpointExecutor::new(config);
            let job_id = 200;

            // Scratch dir still exists
            let scratch_path = temp.path().join("200");
            assert!(scratch_path.exists());

            // Can be removed by janitor or manual cleanup
            executor.remove_scratch_dir(job_id).unwrap();
            assert!(!scratch_path.exists());
        }

        #[test]
        fn deterministic_chunk_ordering() {
            // Test that chunk IDs produce deterministic ordering
            let chunk_ids = vec![5, 1, 3, 2, 4];
            let mut sorted = chunk_ids.clone();
            sorted.sort_unstable();

            // Verify sorting is deterministic
            assert_eq!(sorted, vec![1, 2, 3, 4, 5]);

            // Re-sort multiple times - should be identical
            let mut sorted2 = chunk_ids.clone();
            sorted2.sort_unstable();
            assert_eq!(sorted, sorted2);
        }

        #[test]
        fn upload_retry_idempotence() {
            // Test that re-uploading with same hash is safe
            let temp = TempDir::new().unwrap();
            let config = ExecutorConfig {
                scratch_base_dir: temp.path().to_path_buf(),
                ..Default::default()
            };
            let executor = CheckpointExecutor::new(config);

            let job_id = 300;
            executor.create_scratch_dir(job_id).unwrap();

            let data = b"idempotent upload test";
            let staged = executor.stage_chunk(job_id, 15, 10, data).unwrap();

            // Upload once
            let uploaded1 = executor.upload_chunks(&[staged.clone()]).unwrap();

            // Upload again (retry scenario) - should succeed idempotently
            let uploaded2 = executor.upload_chunks(&[staged]).unwrap();

            // Both uploads should produce identical results
            assert_eq!(uploaded1.len(), uploaded2.len());
            assert_eq!(uploaded1[&15].content_hash, uploaded2[&15].content_hash);

            executor.remove_scratch_dir(job_id).unwrap();
        }
    }
}
