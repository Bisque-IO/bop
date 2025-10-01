#![allow(dead_code)]

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use tracing::{debug, error, info, instrument, trace, warn};

use crate::manifest::{ChunkId, Generation};

use super::planner::{AofPlan, ChunkPlan};

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

    #[instrument(skip(self, plan), fields(
        db_id = plan.db_id,
        target_generation = plan.target_generation,
        chunk_count = plan.chunks.len(),
        estimated_size_bytes = plan.estimated_size_bytes
    ))]
    pub fn prepare_chunks(
        &self,
        plan: &AofPlan,
    ) -> Result<Vec<PreparedChunk>, AppendOnlyExecutionError> {
        info!("starting chunk preparation for checkpoint execution");

        let mut prepared = Vec::with_capacity(plan.chunks.len());
        let mut total_size = 0u64;

        for (idx, chunk) in plan.chunks.iter().enumerate() {
            debug!(
                chunk_id = chunk.chunk_id,
                chunk_index = idx,
                total_chunks = plan.chunks.len(),
                "preparing chunk for checkpoint"
            );

            match prepare_chunk(chunk, plan.target_generation) {
                Ok(prepared_chunk) => {
                    total_size += prepared_chunk.size_bytes;
                    debug!(
                        chunk_id = prepared_chunk.chunk_id,
                        size_bytes = prepared_chunk.size_bytes,
                        content_hash = format!("{:016x}", prepared_chunk.content_hash[0]),
                        "chunk prepared successfully"
                    );
                    prepared.push(prepared_chunk);
                }
                Err(e) => {
                    error!(
                        chunk_id = chunk.chunk_id,
                        error = %e,
                        "failed to prepare chunk"
                    );
                    return Err(e);
                }
            }
        }

        info!(
            prepared_count = prepared.len(),
            total_size_bytes = total_size,
            "all chunks prepared successfully"
        );

        Ok(prepared)
    }
}

#[instrument(fields(chunk_id = chunk.chunk_id, generation))]
fn prepare_chunk(
    chunk: &ChunkPlan,
    generation: Generation,
) -> Result<PreparedChunk, AppendOnlyExecutionError> {
    trace!(file_path = ?chunk.file_path, "reading chunk metadata");

    let file_path = chunk.file_path.clone();
    let size_bytes = std::fs::metadata(&file_path)
        .map_err(|source| {
            error!(
                path = ?file_path,
                error = %source,
                "failed to read file metadata"
            );
            AppendOnlyExecutionError::Io {
                path: file_path.clone(),
                source,
            }
        })?
        .len();

    debug!(size_bytes, "computing content hash for chunk");

    let content_hash = compute_crc64_hash_for_file(file_path.as_path()).map_err(|source| {
        error!(
            path = ?file_path,
            error = %source,
            "failed to compute content hash"
        );
        AppendOnlyExecutionError::Io {
            path: file_path.clone(),
            source,
        }
    })?;

    trace!(
        content_hash = format!("{:016x}", content_hash[0]),
        "content hash computed"
    );

    Ok(PreparedChunk {
        chunk_id: chunk.chunk_id,
        generation,
        size_bytes,
        content_hash,
        file_path,
    })
}

#[instrument(skip(path), fields(path = ?path))]
fn compute_crc64_hash_for_file(path: &Path) -> Result<[u64; 4], io::Error> {
    use crc64fast_nvme::Digest;

    trace!("opening file for hash computation");
    let mut digest = Digest::new();
    let mut file = File::open(path)?;
    let mut buffer = [0u8; 64 * 1024];
    let mut total_bytes = 0u64;

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.write(&buffer[..read]);
        total_bytes += read as u64;
    }

    let crc = digest.sum64();
    trace!(
        total_bytes,
        crc = format!("{:016x}", crc),
        "file hash computation complete"
    );

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
