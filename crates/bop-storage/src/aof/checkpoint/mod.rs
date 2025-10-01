#![allow(dead_code, unused_imports)]

//! Append-only checkpoint engine.

pub mod executor;
pub mod planner;
pub mod truncation;

pub use executor::{AppendOnlyExecutionError, AppendOnlyExecutor, PreparedChunk};
pub use planner::{AofPlan, AofPlanner, AofPlannerContext, ChunkPlan, TailChunkDescriptor};
pub use truncation::{
    ChunkLease, LeaseMap, TruncateDirection, TruncationError, TruncationExecutor, TruncationRequest,
};

use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

use crate::manifest::{
    self, ChunkEntryRecord, ChunkId, ChunkKey, ChunkResidency, DbId, Generation, Manifest,
    RemoteNamespaceId, RemoteObjectId, RemoteObjectKey, epoch_millis,
};
use crate::storage_quota::{QuotaError, ReservationGuard, StorageQuota};

#[derive(Debug, Clone)]
pub struct AppendOnlyCheckpointConfig {
    pub remote_namespace_id: Option<RemoteNamespaceId>,
    pub lease_ttl: Duration,
}

impl Default for AppendOnlyCheckpointConfig {
    fn default() -> Self {
        Self {
            remote_namespace_id: None,
            lease_ttl: Duration::from_secs(300),
        }
    }
}

#[derive(Clone)]
pub struct AppendOnlyContext {
    pub manifest: Arc<Manifest>,
    pub quota: Arc<StorageQuota>,
    pub lease_map: Arc<LeaseMap>,
    pub config: AppendOnlyCheckpointConfig,
}

#[derive(Debug, Clone)]
pub struct AppendOnlyJob {
    pub job_id: manifest::JobId,
    pub db_id: DbId,
    pub current_generation: Generation,
    pub wal_low_lsn: u64,
    pub wal_high_lsn: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AppendOnlyOutcome {
    pub chunks_processed: usize,
    pub bytes_processed: u64,
}

#[derive(Debug, Error)]
pub enum AppendOnlyError {
    #[error("manifest error: {0}")]
    Manifest(String),
    #[error(transparent)]
    Quota(#[from] QuotaError),
    #[error(transparent)]
    Lease(#[from] TruncationError),
    #[error(transparent)]
    Execution(#[from] AppendOnlyExecutionError),
}

/// Runs an append-only checkpoint job end to end.
pub fn run_checkpoint(
    ctx: &AppendOnlyContext,
    job: &AppendOnlyJob,
) -> Result<AppendOnlyOutcome, AppendOnlyError> {
    let chunks = collect_tail_chunks(&ctx.manifest, job)?;

    let planner = AofPlanner::new();
    let planner_context = AofPlannerContext {
        db_id: job.db_id,
        current_generation: job.current_generation,
        wal_low_lsn: job.wal_low_lsn,
        wal_high_lsn: job.wal_high_lsn,
    };
    let plan = planner.plan(&planner_context, &chunks);

    if plan.is_empty() {
        return Ok(AppendOnlyOutcome::default());
    }

    let _quota_guard = reserve_quota(&ctx.quota, job, &plan)?;

    register_leases(&ctx.lease_map, job, &plan, ctx.config.lease_ttl)?;

    let executor = AppendOnlyExecutor::new();
    let prepared = executor.prepare_chunks(&plan)?;

    let published = publish_chunks(
        &ctx.manifest,
        ctx.config.remote_namespace_id,
        job.db_id,
        &prepared,
    )?;

    release_leases(&ctx.lease_map, &plan);

    Ok(AppendOnlyOutcome {
        chunks_processed: published,
        bytes_processed: plan.estimated_size_bytes,
    })
}

fn collect_tail_chunks(
    manifest: &Arc<Manifest>,
    job: &AppendOnlyJob,
) -> Result<Vec<TailChunkDescriptor>, AppendOnlyError> {
    let artifacts = manifest
        .wal_artifacts(job.db_id)
        .map_err(|err| AppendOnlyError::Manifest(err.to_string()))?;

    let mut chunks = Vec::new();
    for (_key, record) in artifacts {
        let manifest::AofWalArtifactRecord {
            artifact_id,
            kind,
            local_path,
            ..
        } = record;

        let manifest::WalArtifactKind::AppendOnlySegment {
            start_page_index,
            end_page_index,
            size_bytes,
        } = kind
        else {
            continue;
        };

        if end_page_index <= job.wal_low_lsn || start_page_index >= job.wal_high_lsn {
            continue;
        }

        let chunk_id = u32::try_from(artifact_id).unwrap_or(artifact_id as u32);

        chunks.push(TailChunkDescriptor {
            chunk_id,
            start_lsn: start_page_index,
            end_lsn: end_page_index,
            size_bytes,
            file_path: local_path,
        });
    }

    chunks.sort_by_key(|chunk| chunk.chunk_id);
    Ok(chunks)
}

fn reserve_quota(
    quota: &Arc<StorageQuota>,
    job: &AppendOnlyJob,
    plan: &AofPlan,
) -> Result<Option<ReservationGuard>, AppendOnlyError> {
    if plan.estimated_size_bytes == 0 {
        return Ok(None);
    }

    quota
        .reserve(job.db_id, job.job_id, plan.estimated_size_bytes)
        .map(Some)
        .map_err(AppendOnlyError::from)
}

fn register_leases(
    lease_map: &Arc<LeaseMap>,
    job: &AppendOnlyJob,
    plan: &AofPlan,
    ttl: Duration,
) -> Result<(), AppendOnlyError> {
    for chunk in &plan.chunks {
        lease_map.register(chunk.chunk_id, plan.target_generation, job.job_id, ttl)?;
    }
    Ok(())
}

fn release_leases(lease_map: &Arc<LeaseMap>, plan: &AofPlan) {
    for chunk in &plan.chunks {
        lease_map.release(chunk.chunk_id);
    }
}

fn publish_chunks(
    manifest: &Arc<Manifest>,
    remote_namespace_id: Option<RemoteNamespaceId>,
    db_id: DbId,
    chunks: &[PreparedChunk],
) -> Result<usize, AppendOnlyError> {
    let mut txn = manifest.begin();

    for prepared in chunks {
        let chunk_key = ChunkKey::new(db_id, prepared.chunk_id);
        let mut record = ChunkEntryRecord::default();

        record.file_name = file_name_from_path(&prepared.file_path);
        record.generation = prepared.generation;
        record.size_bytes = prepared.size_bytes;
        record.content_hash = prepared.content_hash;
        record.residency = residency_for(remote_namespace_id);
        if remote_namespace_id.is_some() {
            record.uploaded_at_epoch_ms = Some(epoch_millis());
        }

        txn.upsert_chunk(chunk_key, record);
    }

    txn.commit()
        .map_err(|err| AppendOnlyError::Manifest(err.to_string()))?;

    Ok(chunks.len())
}

fn file_name_from_path(path: &PathBuf) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn residency_for(_remote_namespace_id: Option<RemoteNamespaceId>) -> ChunkResidency {
    ChunkResidency::Local
}
