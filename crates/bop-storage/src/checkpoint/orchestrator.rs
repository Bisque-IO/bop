//! Checkpoint job orchestration.
//!
//! This module implements T6 from the checkpointing plan: end-to-end
//! orchestration of checkpoint jobs from planning through S3 upload
//! to manifest publication.
//!
//! # Architecture
//!
//! The orchestrator coordinates:
//! 1. CheckpointPlanner - creates checkpoint plan
//! 2. CheckpointExecutor - stages chunks to disk
//! 3. RemoteStore - uploads chunks to S3 (async)
//! 4. Manifest - publishes chunk metadata atomically
//! 5. StorageQuota - enforces disk quota limits
//!
//! Progress is tracked and persisted for crash recovery.

use std::convert::TryFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::checkpoint::executor::{ExecutionPhase, ExecutorConfig, StagedChunk};
use crate::checkpoint::truncation::{LeaseMap, TruncationError};
use crate::checkpoint::{
    CheckpointExecutor, CheckpointJobState, CheckpointPlanner, ChunkAction, PlannerContext,
    WorkloadType,
};
use crate::manifest::epoch_millis;
use crate::manifest::{
    ChunkDeltaKey, ChunkDeltaRecord, ChunkEntryRecord, ChunkId, ChunkKey, ChunkResidency, DbId,
    Generation, JobId, Manifest, RemoteNamespaceId, RemoteObjectId, RemoteObjectKey,
    RemoteObjectKind,
};
use crate::remote_store::{BlobKey, RemoteStore};
use crate::storage_quota::StorageQuota;

/// Errors that can occur during orchestration.
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    /// Planner error.
    #[error("planner error: {0}")]
    Planner(#[from] crate::checkpoint::PlannerError),

    /// Executor error.
    #[error("executor error: {0}")]
    Executor(#[from] crate::checkpoint::ExecutorError),

    /// RemoteStore error.
    #[error("remote store error: {0}")]
    RemoteStore(#[from] crate::remote_store::RemoteStoreError),

    /// Manifest error.
    #[error("manifest error: {0}")]
    Manifest(String),

    /// Quota error.
    #[error("quota error: {0}")]
    Quota(#[from] crate::storage_quota::QuotaError),

    /// Filesystem I/O failure.
    #[error("I/O error accessing {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Unsupported action encountered in plan.
    #[error("unsupported checkpoint action: {0}")]
    UnsupportedAction(String),

    /// Lease coordination failure.
    #[error("lease error: {0}")]
    Lease(#[from] TruncationError),

    /// Generation could not be represented in 32 bits for delta indexing.
    #[error("generation {generation} does not fit into u32 for chunk {chunk_id}")]
    GenerationOverflow {
        chunk_id: ChunkId,
        generation: Generation,
    },

    /// Uploaded content hash did not match local staging hash.
    #[error("checksum mismatch detected after uploading chunk {chunk_id}")]
    UploadChecksumMismatch { chunk_id: ChunkId },

    /// Job not found.
    #[error("job {0} not found")]
    JobNotFound(JobId),

    /// Job already running.
    #[error("job {0} already running")]
    JobAlreadyRunning(JobId),
}

/// Configuration for the checkpoint orchestrator.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Scratch directory base path.
    pub scratch_base_dir: PathBuf,

    /// Maximum concurrent uploads to S3.
    pub max_concurrent_uploads: usize,

    /// Chunk size in bytes (typically 64 MiB).
    pub chunk_size_bytes: u64,

    /// Enable checksum validation during upload.
    pub enable_checksums: bool,

    /// Remote namespace where checkpoint artifacts are written.
    pub remote_namespace_id: RemoteNamespaceId,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            scratch_base_dir: PathBuf::from("checkpoint_scratch"),
            max_concurrent_uploads: 4,
            chunk_size_bytes: 64 * 1024 * 1024,
            enable_checksums: true,
            remote_namespace_id: 0,
        }
    }
}

#[derive(Clone)]
struct UploadedArtifact {
    action: ChunkAction,
    staged: StagedChunk,
    blob_key: BlobKey,
}

/// Checkpoint orchestrator that coordinates the full checkpoint pipeline.
///
/// # T6: Job Queue Orchestration
///
/// Orchestrates checkpoint jobs through these phases:
/// 1. **Preparing**: Reserve quota, create scratch directory
/// 2. **Planning**: Generate checkpoint plan (planner)
/// 3. **Staging**: Stage chunks to scratch directory (executor)
/// 4. **Uploading**: Upload chunks to S3 (remote_store, async)
/// 5. **Publishing**: Atomically update manifest
/// 6. **Cleanup**: Release quota, delete scratch directory
///
/// Progress is tracked in `CheckpointJobState` for crash recovery.
pub struct CheckpointOrchestrator {
    planner: CheckpointPlanner,
    executor: CheckpointExecutor,
    remote_store: Arc<RemoteStore>,
    manifest: Arc<Manifest>,
    quota: Arc<StorageQuota>,
    lease_map: Arc<LeaseMap>,
    config: OrchestratorConfig,
}

impl CheckpointOrchestrator {
    /// Creates a new checkpoint orchestrator.
    ///
    /// # Parameters
    /// - `lease_map`: Shared lease map for coordinating with truncation operations
    pub fn new(
        remote_store: Arc<RemoteStore>,
        manifest: Arc<Manifest>,
        quota: Arc<StorageQuota>,
        lease_map: Arc<LeaseMap>,
        config: OrchestratorConfig,
    ) -> Self {
        let planner = CheckpointPlanner::new();
        let executor_config = ExecutorConfig {
            scratch_base_dir: config.scratch_base_dir.clone(),
            max_concurrent_uploads: config.max_concurrent_uploads,
            chunk_size_bytes: config.chunk_size_bytes,
            enable_checksums: config.enable_checksums,
        };
        let executor = CheckpointExecutor::new(executor_config);

        Self {
            planner,
            executor,
            remote_store,
            manifest,
            quota,
            lease_map,
            config,
        }
    }

    /// Gets the lease map for external coordination.
    pub fn lease_map(&self) -> &Arc<LeaseMap> {
        &self.lease_map
    }

    /// Runs a checkpoint job end-to-end.
    ///
    /// # T6: Full Checkpoint Flow
    ///
    /// This is the main entry point for executing a checkpoint job.
    /// It orchestrates all phases from planning through publication.
    ///
    /// # Parameters
    ///
    /// - `db_id`: Database ID to checkpoint
    /// - `job_id`: Unique job ID for this checkpoint
    /// - `context`: Planner context (generation, WAL range, etc.)
    /// - `workload`: Append-only or libsql workload type
    ///
    /// # Returns
    ///
    /// The checkpoint job state after completion (or error).
    pub async fn run_checkpoint(
        &self,
        db_id: DbId,
        job_id: JobId,
        context: PlannerContext,
        _workload: WorkloadType,
    ) -> Result<CheckpointJobState, OrchestratorError> {
        // Phase 1: Preparing - Reserve quota and create scratch dir
        let mut state = CheckpointJobState {
            job_id,
            db_id,
            phase: ExecutionPhase::Preparing,
            total_chunks: 0,
            chunks_staged: 0,
            chunks_uploaded: 0,
            processed_chunks: Vec::new(),
        };

        // Create scratch directory
        let _scratch_dir = self.executor.create_scratch_dir(job_id)?;

        // Phase 2: Planning - Generate checkpoint plan
        state.phase = ExecutionPhase::Preparing;
        let plan = self.planner.plan(context)?;

        state.total_chunks = plan.actions.len() as u32;

        // Reserve quota for staging
        let estimated_size = plan.estimated_size_bytes;
        let _quota_guard = self.quota.reserve(db_id, job_id, estimated_size)?;

        // Phase 3: Staging - Stage chunks to scratch directory
        state.phase = ExecutionPhase::Staging;
        // T6b: Save progress after planning phase
        self.save_progress(&state, plan.target_generation)?;

        let mut staged_artifacts: Vec<(ChunkAction, StagedChunk)> = Vec::new();

        for action in &plan.actions {
            match action {
                ChunkAction::SealAsBase {
                    chunk_id,
                    segment_path,
                    ..
                } => {
                    self.lease_map
                        .register(
                            *chunk_id,
                            plan.target_generation,
                            job_id,
                            std::time::Duration::from_secs(300),
                        )
                        .map_err(OrchestratorError::from)?;

                    let staged = match self.stage_wal_segment(
                        job_id,
                        *chunk_id,
                        plan.target_generation,
                        segment_path.as_path(),
                    ) {
                        Ok(staged) => staged,
                        Err(err) => {
                            self.lease_map.release(*chunk_id);
                            return Err(err);
                        }
                    };

                    staged_artifacts.push((action.clone(), staged));
                    state.chunks_staged += 1;
                    state.processed_chunks.push(*chunk_id);

                    if state.chunks_staged % 10 == 0 {
                        self.save_progress(&state, plan.target_generation)?;
                    }
                }
                ChunkAction::CreateBase { .. } => {
                    return Err(OrchestratorError::UnsupportedAction(
                        "CreateBase actions are not yet supported by the checkpoint orchestrator"
                            .into(),
                    ));
                }
                ChunkAction::CreateDelta { .. } => {
                    return Err(OrchestratorError::UnsupportedAction(
                        "CreateDelta actions are not yet supported by the checkpoint orchestrator"
                            .into(),
                    ));
                }
                ChunkAction::RewriteBase { .. } => {
                    return Err(OrchestratorError::UnsupportedAction(
                        "RewriteBase actions are not yet supported by the checkpoint orchestrator"
                            .into(),
                    ));
                }
            }
        }

        // Phase 4: Uploading - Upload chunks to S3 (async)
        state.phase = ExecutionPhase::Uploading;
        // T6b: Save progress after staging phase
        self.save_progress(&state, plan.target_generation)?;

        let mut uploaded_artifacts = Vec::new();

        for (action, staged) in &staged_artifacts {
            let blob_key = match action {
                ChunkAction::SealAsBase { .. }
                | ChunkAction::CreateBase { .. }
                | ChunkAction::RewriteBase { .. } => BlobKey::chunk(
                    db_id,
                    staged.chunk_id,
                    staged.generation,
                    staged.content_hash,
                ),
                ChunkAction::CreateDelta {
                    base_chunk_id,
                    delta_generation,
                    delta_id,
                    ..
                } => BlobKey::delta(
                    db_id,
                    *base_chunk_id,
                    *delta_generation,
                    *delta_id,
                    staged.content_hash,
                ),
            };

            let content_hash = self
                .remote_store
                .put_blob_from_file(&blob_key, &staged.file_path)
                .await?;

            if self.config.enable_checksums && content_hash != staged.content_hash {
                return Err(OrchestratorError::UploadChecksumMismatch {
                    chunk_id: staged.chunk_id,
                });
            }

            uploaded_artifacts.push(UploadedArtifact {
                action: action.clone(),
                staged: staged.clone(),
                blob_key,
            });
            state.chunks_uploaded += 1;

            // T6b: Save progress after each upload
            if state.chunks_uploaded % 10 == 0 {
                self.save_progress(&state, plan.target_generation)?;
            }
        }

        // Phase 5: Publishing - Atomically update manifest
        state.phase = ExecutionPhase::Publishing;
        // T6b: Save progress before publishing
        self.save_progress(&state, plan.target_generation)?;

        self.publish_manifest_updates(db_id, &uploaded_artifacts)?;

        // T10b: Release all chunk leases after successful publication
        for chunk_id in &state.processed_chunks {
            self.lease_map.release(*chunk_id);
        }

        // Phase 6: Cleanup - Remove scratch directory
        self.executor.remove_scratch_dir(job_id)?;

        // Quota is automatically released when _quota_guard is dropped

        state.phase = ExecutionPhase::Completed;
        // T6b: Save final progress
        self.save_progress(&state, plan.target_generation)?;

        Ok(state)
    }

    fn publish_manifest_updates(
        &self,
        db_id: DbId,
        artifacts: &[UploadedArtifact],
    ) -> Result<(), OrchestratorError> {
        let mut txn = self.manifest.begin();

        for artifact in artifacts {
            match &artifact.action {
                ChunkAction::SealAsBase { chunk_id, .. }
                | ChunkAction::CreateBase { chunk_id, .. }
                | ChunkAction::RewriteBase { chunk_id, .. } => {
                    let chunk_key = ChunkKey::new(db_id, *chunk_id);
                    let mut record = ChunkEntryRecord::default();
                    record.file_name = artifact.blob_key.to_s3_key();
                    record.generation = artifact.staged.generation;
                    record.size_bytes = artifact.staged.size_bytes;
                    record.content_hash = artifact.staged.content_hash;
                    record.residency = ChunkResidency::RemoteArchived {
                        object: RemoteObjectKey {
                            namespace_id: self.config.remote_namespace_id,
                            object: RemoteObjectId {
                                origin_db_id: db_id,
                                chunk_id: *chunk_id,
                                kind: RemoteObjectKind::Base,
                                generation: artifact.staged.generation,
                            },
                        },
                        verified: true,
                    };
                    record.uploaded_at_epoch_ms = Some(epoch_millis());
                    txn.upsert_chunk(chunk_key, record);
                }
                ChunkAction::CreateDelta {
                    base_chunk_id,
                    delta_id,
                    ..
                } => {
                    let generation_u32 =
                        u32::try_from(artifact.staged.generation).map_err(|_| {
                            OrchestratorError::GenerationOverflow {
                                chunk_id: *base_chunk_id,
                                generation: artifact.staged.generation,
                            }
                        })?;
                    let delta_key = ChunkDeltaKey::new(db_id, *base_chunk_id, generation_u32);
                    let mut record = ChunkDeltaRecord::default();
                    record.base_chunk_id = *base_chunk_id;
                    record.delta_id = *delta_id;
                    record.delta_file = artifact.blob_key.to_s3_key();
                    record.size_bytes = artifact.staged.size_bytes;
                    record.content_hash = artifact.staged.content_hash;
                    record.residency = ChunkResidency::RemoteArchived {
                        object: RemoteObjectKey {
                            namespace_id: self.config.remote_namespace_id,
                            object: RemoteObjectId {
                                origin_db_id: db_id,
                                chunk_id: *base_chunk_id,
                                kind: RemoteObjectKind::Delta {
                                    delta_id: *delta_id,
                                },
                                generation: artifact.staged.generation,
                            },
                        },
                        verified: true,
                    };
                    txn.upsert_chunk_delta(delta_key, record);
                }
            }
        }

        txn.commit()
            .map_err(|e| OrchestratorError::Manifest(e.to_string()))?;

        Ok(())
    }

    fn stage_wal_segment(
        &self,
        job_id: JobId,
        chunk_id: ChunkId,
        generation: Generation,
        segment_path: &Path,
    ) -> Result<StagedChunk, OrchestratorError> {
        let data = std::fs::read(segment_path).map_err(|source| OrchestratorError::Io {
            path: segment_path.to_path_buf(),
            source,
        })?;

        self.executor
            .stage_chunk(job_id, chunk_id, generation, &data)
            .map_err(OrchestratorError::from)
    }

    /// Lists active checkpoint jobs.
    ///
    /// In production, would query the job queue for active jobs.
    pub fn list_active_jobs(&self) -> Vec<JobId> {
        // Stub implementation
        Vec::new()
    }

    /// Cancels a checkpoint job.
    ///
    /// In production, would coordinate with TruncationExecutor
    /// to cancel in-flight checkpoints.
    pub async fn cancel_job(&self, _job_id: JobId) -> Result<(), OrchestratorError> {
        // Stub implementation
        // Would:
        // 1. Mark job as cancelled in job queue
        // 2. Wait for current phase to complete
        // 3. Clean up scratch directory
        // 4. Release quota
        Ok(())
    }

    /// Persists checkpoint job progress to manifest (T6b).
    ///
    /// Stores the current state in the job queue for crash recovery.
    pub fn save_progress(
        &self,
        state: &CheckpointJobState,
        generation: u64,
    ) -> Result<(), OrchestratorError> {
        use crate::manifest::{JobDurableState, JobKind, JobPayload, JobRecord, epoch_millis};

        let existing_record = self
            .manifest
            .get_job(state.job_id)
            .map_err(|e| OrchestratorError::Manifest(e.to_string()))?;

        let existing_created = existing_record
            .as_ref()
            .map(|record| record.created_at_epoch_ms);

        let existing_started = existing_record
            .as_ref()
            .and_then(|record| match &record.state {
                JobDurableState::InFlight {
                    started_at_epoch_ms,
                } => Some(*started_at_epoch_ms),
                _ => None,
            });

        let now = epoch_millis();

        let created_at_epoch_ms = existing_created.unwrap_or(now);
        let started_at_epoch_ms = existing_started.unwrap_or(now);

        let payload = JobPayload::CheckpointProgress {
            phase: format!("{:?}", state.phase),
            total_chunks: state.total_chunks,
            chunks_staged: state.chunks_staged,
            chunks_uploaded: state.chunks_uploaded,
            processed_chunks: state.processed_chunks.clone(),
            generation,
        };

        let job_state = if state.phase == ExecutionPhase::Completed {
            JobDurableState::Completed {
                completed_at_epoch_ms: now,
            }
        } else {
            JobDurableState::InFlight {
                started_at_epoch_ms,
            }
        };

        let record = JobRecord {
            record_version: 1,
            job_id: state.job_id,
            created_at_epoch_ms,
            db_id: Some(state.db_id),
            kind: JobKind::Checkpoint,
            state: job_state,
            payload,
        };

        // Update job record in manifest
        let mut txn = self.manifest.begin();
        txn.upsert_job(state.job_id, record);
        txn.commit()
            .map_err(|e| OrchestratorError::Manifest(e.to_string()))?;

        Ok(())
    }

    /// Loads checkpoint job progress from manifest (T6b).
    ///
    /// Returns None if job not found or already completed.
    pub fn load_progress(
        &self,
        job_id: JobId,
    ) -> Result<Option<(CheckpointJobState, u64)>, OrchestratorError> {
        use crate::manifest::JobPayload;

        let job_record = self
            .manifest
            .get_job(job_id)
            .map_err(|e| OrchestratorError::Manifest(e.to_string()))?;

        let job_record = match job_record {
            Some(r) => r,
            None => return Ok(None),
        };

        // Parse payload
        let (phase_str, total, staged, uploaded, processed, generation) = match job_record.payload {
            JobPayload::CheckpointProgress {
                phase,
                total_chunks,
                chunks_staged,
                chunks_uploaded,
                processed_chunks,
                generation,
            } => (
                phase,
                total_chunks,
                chunks_staged,
                chunks_uploaded,
                processed_chunks,
                generation,
            ),
            _ => return Ok(None),
        };

        // Parse phase string back to ExecutionPhase
        let phase = match phase_str.as_str() {
            "Preparing" => ExecutionPhase::Preparing,
            "Staging" => ExecutionPhase::Staging,
            "Uploading" => ExecutionPhase::Uploading,
            "Publishing" => ExecutionPhase::Publishing,
            "Cleanup" => ExecutionPhase::Cleanup,
            "Completed" => return Ok(None), // Already done
            _ => ExecutionPhase::Preparing,
        };

        let db_id = job_record.db_id.ok_or_else(|| {
            OrchestratorError::Manifest("checkpoint job missing db_id".to_string())
        })?;

        let state = CheckpointJobState {
            job_id,
            db_id,
            phase,
            total_chunks: total,
            chunks_staged: staged,
            chunks_uploaded: uploaded,
            processed_chunks: processed,
        };

        Ok(Some((state, generation)))
    }

    /// Resumes a checkpoint job from saved progress (T6b).
    ///
    /// This allows crash recovery: if orchestrator crashes mid-checkpoint,
    /// it can resume from the last saved phase.
    pub async fn resume_checkpoint(
        &self,
        job_id: JobId,
    ) -> Result<CheckpointJobState, OrchestratorError> {
        let (state, _generation) = self
            .load_progress(job_id)?
            .ok_or(OrchestratorError::JobNotFound(job_id))?;

        // Resume from current phase
        match state.phase {
            ExecutionPhase::Preparing => {
                // Need to restart from scratch
                Err(OrchestratorError::Manifest(
                    "cannot resume from Preparing phase, must restart".to_string(),
                ))
            }
            ExecutionPhase::Staging => {
                // Resume staging from processed_chunks
                Err(OrchestratorError::Manifest(
                    "resume from Staging not yet implemented".to_string(),
                ))
            }
            ExecutionPhase::Uploading => {
                // Resume uploading from processed_chunks
                Err(OrchestratorError::Manifest(
                    "resume from Uploading not yet implemented".to_string(),
                ))
            }
            ExecutionPhase::Publishing => {
                // Retry publishing (idempotent)
                Err(OrchestratorError::Manifest(
                    "resume from Publishing not yet implemented".to_string(),
                ))
            }
            ExecutionPhase::Cleanup => {
                // Almost done, just cleanup
                Err(OrchestratorError::Manifest(
                    "resume from Cleanup not yet implemented".to_string(),
                ))
            }
            ExecutionPhase::Completed => {
                // Already done
                Ok(state)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_quota::{QuotaConfig, StorageQuota};
    use tempfile::TempDir;

    // Note: Full integration tests require MinIO and manifest database setup
    // These are unit tests for the orchestrator structure

    #[test]
    fn orchestrator_creation() {
        let temp = TempDir::new().unwrap();

        // Create components (using stubs for now)
        // In production test, would use real MinIO and manifest
        let quota_config = QuotaConfig::default();
        let _quota = StorageQuota::new(quota_config);

        let _orch_config = OrchestratorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            max_concurrent_uploads: 4,
            chunk_size_bytes: 64 * 1024 * 1024,
            enable_checksums: true,
            remote_namespace_id: 0,
        };

        // Can't easily create RemoteStore and Manifest in unit test without deps
        // This test just verifies the structure compiles
    }
}
