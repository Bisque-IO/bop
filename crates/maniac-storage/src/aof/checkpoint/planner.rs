#![allow(dead_code)]

use std::path::PathBuf;

use tracing::{debug, error, info, instrument, trace, warn};

use crate::manifest::{ChunkId, DbId, Generation};

/// Tail chunk metadata supplied to the append-only planner.
#[derive(Debug, Clone)]
pub struct TailChunkDescriptor {
    pub chunk_id: ChunkId,
    pub start_lsn: u64,
    pub end_lsn: u64,
    pub size_bytes: u64,
    pub file_path: PathBuf,
}

impl TailChunkDescriptor {
    #[instrument(skip(self), fields(chunk_id = self.chunk_id))]
    pub fn overlaps_wal_range(&self, wal_low_lsn: u64, wal_high_lsn: u64) -> bool {
        let overlaps = if self.end_lsn <= wal_low_lsn {
            trace!(
                end_lsn = self.end_lsn,
                wal_low_lsn, "chunk ends before WAL range"
            );
            false
        } else if self.start_lsn >= wal_high_lsn {
            trace!(
                start_lsn = self.start_lsn,
                wal_high_lsn, "chunk starts after WAL range"
            );
            false
        } else {
            debug!(
                start_lsn = self.start_lsn,
                end_lsn = self.end_lsn,
                wal_low_lsn,
                wal_high_lsn,
                "chunk overlaps WAL range"
            );
            true
        };
        overlaps
    }
}

/// Context required to plan an append-only checkpoint.
#[derive(Debug, Clone)]
pub struct AofPlannerContext {
    pub db_id: DbId,
    pub current_generation: Generation,
    pub wal_low_lsn: u64,
    pub wal_high_lsn: u64,
}

/// Planned work for an append-only checkpoint.
#[derive(Debug, Clone)]
pub struct AofPlan {
    pub db_id: DbId,
    pub target_generation: Generation,
    pub wal_range: (u64, u64),
    pub chunks: Vec<ChunkPlan>,
    pub estimated_size_bytes: u64,
}

impl AofPlan {
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

/// Action required to publish a tail chunk as an immutable base chunk.
#[derive(Debug, Clone)]
pub struct ChunkPlan {
    pub chunk_id: ChunkId,
    pub size_bytes: u64,
    pub file_path: PathBuf,
}

/// Planner that converts tail chunk descriptors into checkpoint work.
#[derive(Debug, Default, Clone)]
pub struct AofPlanner;

impl AofPlanner {
    pub fn new() -> Self {
        Self
    }

    #[instrument(skip(self, chunks), fields(
        db_id = context.db_id,
        current_generation = context.current_generation,
        wal_low_lsn = context.wal_low_lsn,
        wal_high_lsn = context.wal_high_lsn,
        chunk_count = chunks.len()
    ))]
    pub fn plan(&self, context: &AofPlannerContext, chunks: &[TailChunkDescriptor]) -> AofPlan {
        info!(
            "starting checkpoint planning for WAL range [{}, {})",
            context.wal_low_lsn, context.wal_high_lsn
        );

        let target_generation = context.current_generation + 1;
        let mut planned_chunks = Vec::new();
        let mut estimated_size_bytes = 0u64;

        for chunk in chunks {
            if !chunk.overlaps_wal_range(context.wal_low_lsn, context.wal_high_lsn) {
                trace!(
                    chunk_id = chunk.chunk_id,
                    chunk_range = format!("[{}, {})", chunk.start_lsn, chunk.end_lsn),
                    "skipping chunk outside WAL range"
                );
                continue;
            }

            debug!(
                chunk_id = chunk.chunk_id,
                size_bytes = chunk.size_bytes,
                lsn_range = format!("[{}, {})", chunk.start_lsn, chunk.end_lsn),
                "including chunk in checkpoint plan"
            );

            planned_chunks.push(ChunkPlan {
                chunk_id: chunk.chunk_id,
                size_bytes: chunk.size_bytes,
                file_path: chunk.file_path.clone(),
            });

            estimated_size_bytes += chunk.size_bytes;
        }

        let plan = AofPlan {
            db_id: context.db_id,
            target_generation,
            wal_range: (context.wal_low_lsn, context.wal_high_lsn),
            chunks: planned_chunks,
            estimated_size_bytes,
        };

        if plan.is_empty() {
            info!("checkpoint plan is empty, no chunks to checkpoint");
        } else {
            info!(
                target_generation = plan.target_generation,
                chunk_count = plan.chunks.len(),
                estimated_size_bytes = plan.estimated_size_bytes,
                "checkpoint plan created successfully"
            );
        }

        plan
    }
}
