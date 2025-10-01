#![allow(dead_code)]

use std::path::PathBuf;

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
    pub fn overlaps_wal_range(&self, wal_low_lsn: u64, wal_high_lsn: u64) -> bool {
        if self.end_lsn <= wal_low_lsn {
            return false;
        }
        if self.start_lsn >= wal_high_lsn {
            return false;
        }
        true
    }
}

/// Context required to plan an append-only checkpoint.
#[derive(Debug, Clone)]
pub struct AppendOnlyPlannerContext {
    pub db_id: DbId,
    pub current_generation: Generation,
    pub wal_low_lsn: u64,
    pub wal_high_lsn: u64,
}

/// Planned work for an append-only checkpoint.
#[derive(Debug, Clone)]
pub struct AppendOnlyPlan {
    pub db_id: DbId,
    pub target_generation: Generation,
    pub wal_range: (u64, u64),
    pub chunks: Vec<ChunkPlan>,
    pub estimated_size_bytes: u64,
}

impl AppendOnlyPlan {
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
pub struct AppendOnlyPlanner;

impl AppendOnlyPlanner {
    pub fn new() -> Self {
        Self
    }

    pub fn plan(
        &self,
        context: &AppendOnlyPlannerContext,
        chunks: &[TailChunkDescriptor],
    ) -> AppendOnlyPlan {
        let target_generation = context.current_generation + 1;
        let mut planned_chunks = Vec::new();
        let mut estimated_size_bytes = 0u64;

        for chunk in chunks {
            if !chunk.overlaps_wal_range(context.wal_low_lsn, context.wal_high_lsn) {
                continue;
            }

            planned_chunks.push(ChunkPlan {
                chunk_id: chunk.chunk_id,
                size_bytes: chunk.size_bytes,
                file_path: chunk.file_path.clone(),
            });

            estimated_size_bytes += chunk.size_bytes;
        }

        AppendOnlyPlan {
            db_id: context.db_id,
            target_generation,
            wal_range: (context.wal_low_lsn, context.wal_high_lsn),
            chunks: planned_chunks,
            estimated_size_bytes,
        }
    }
}
