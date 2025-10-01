#![allow(dead_code)]

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::manifest::{ChunkId, Generation};

use super::planner::{AppendOnlyPlan, ChunkPlan};

/// Prepared chunk ready for publication.
#[derive(Debug, Clone)]
pub struct PreparedChunk {
    pub chunk_id: ChunkId,
    pub generation: Generation,
    pub size_bytes: u64,
    pub content_hash: [u64; 4],
    pub file_path: PathBuf,
}

/// Append-only checkpoint execution helpers.
#[derive(Debug, Default, Clone)]
pub struct AppendOnlyExecutor;

impl AppendOnlyExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn prepare_chunks(
        &self,
        plan: &AppendOnlyPlan,
    ) -> Result<Vec<PreparedChunk>, AppendOnlyExecutionError> {
        let mut prepared = Vec::with_capacity(plan.chunks.len());

        for chunk in &plan.chunks {
            prepared.push(prepare_chunk(chunk, plan.target_generation)?);
        }

        Ok(prepared)
    }
}

fn prepare_chunk(
    chunk: &ChunkPlan,
    generation: Generation,
) -> Result<PreparedChunk, AppendOnlyExecutionError> {
    let file_path = chunk.file_path.clone();
    let size_bytes = std::fs::metadata(&file_path)
        .map_err(|source| AppendOnlyExecutionError::Io {
            path: file_path.clone(),
            source,
        })?
        .len();

    let content_hash = compute_crc64_hash_for_file(file_path.as_path()).map_err(|source| {
        AppendOnlyExecutionError::Io {
            path: file_path.clone(),
            source,
        }
    })?;

    Ok(PreparedChunk {
        chunk_id: chunk.chunk_id,
        generation,
        size_bytes,
        content_hash,
        file_path,
    })
}

fn compute_crc64_hash_for_file(path: &Path) -> Result<[u64; 4], io::Error> {
    use crc64fast_nvme::Digest;

    let mut digest = Digest::new();
    let mut file = File::open(path)?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.write(&buffer[..read]);
    }

    let crc = digest.sum64();
    Ok([
        crc,
        crc.wrapping_add(1),
        crc.wrapping_add(2),
        crc.wrapping_add(3),
    ])
}

/// Errors that can occur while preparing append-only chunks.
#[derive(Debug, thiserror::Error)]
pub enum AppendOnlyExecutionError {
    #[error("I/O error reading chunk at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
