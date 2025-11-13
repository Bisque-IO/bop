use crate::ManifestPath;
use crate::manifest::{ChunkId, ManifestError, ManifestRecord};
use crate::superblock::{Superblock, SuperblockError};
use generator::{Gn, Scope};
use std::sync::{Arc, Mutex};

type CheckpointGen = generator::Generator<'static, Result<(), CheckpointError>, CheckpointStep>;

/// Planning metadata for flushing a newly materialised chunk during a checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkFlushPlan {
    pub chunk_id: ChunkId,
    pub wal_sequence: u64,
    pub output_path: ManifestPath,
}

impl ChunkFlushPlan {
    pub fn new(chunk_id: ChunkId, wal_sequence: u64, output_path: ManifestPath) -> Self {
        Self {
            chunk_id,
            wal_sequence,
            output_path,
        }
    }
}

/// Aggregate checkpoint inputs produced by the planner.
#[derive(Debug, Clone)]
pub struct CheckpointPlan {
    pub chunks: Vec<ChunkFlushPlan>,
    pub manifest_records: Vec<ManifestRecord>,
    pub superblock: Superblock,
}

impl CheckpointPlan {
    pub fn new(superblock: Superblock) -> Self {
        Self {
            chunks: Vec::new(),
            manifest_records: Vec::new(),
            superblock,
        }
    }

    pub fn with_chunks(mut self, chunks: Vec<ChunkFlushPlan>) -> Self {
        self.chunks = chunks;
        self
    }

    pub fn with_manifest(mut self, records: Vec<ManifestRecord>) -> Self {
        self.manifest_records = records;
        self
    }
}

/// Emits cooperative checkpoint steps that the driver can overlap on worker threads.
#[derive(Debug)]
pub struct CheckpointCoroutine {
    inner: CheckpointGen,
    result: Arc<Mutex<Option<Result<(), CheckpointError>>>>,
    started: bool,
    finished: bool,
}

impl CheckpointCoroutine {
    pub fn new(plan: CheckpointPlan) -> Self {
        let chunks = plan.chunks;
        let manifest_records = plan.manifest_records;
        let superblock = plan.superblock;
        let result_slot: Arc<Mutex<Option<Result<(), CheckpointError>>>> =
            Arc::new(Mutex::new(None));
        let slot = Arc::clone(&result_slot);
        let inner: CheckpointGen =
            Gn::<Result<(), CheckpointError>>::new_scoped(move |mut scope: Scope<_, _>| {
                for chunk in chunks.iter() {
                    if let Some(res) = scope.yield_(CheckpointStep::StreamWal {
                        chunk: chunk.clone(),
                    }) {
                        if let Err(err) = res {
                            *slot.lock().expect("result slot poisoned") = Some(Err(err));
                            generator::done!();
                        }
                    } else {
                        *slot.lock().expect("result slot poisoned") = Some(Ok(()));
                        generator::done!();
                    }
                    if let Some(res) = scope.yield_(CheckpointStep::FlushChunk {
                        chunk: chunk.clone(),
                    }) {
                        if let Err(err) = res {
                            *slot.lock().expect("result slot poisoned") = Some(Err(err));
                            generator::done!();
                        }
                    } else {
                        *slot.lock().expect("result slot poisoned") = Some(Ok(()));
                        generator::done!();
                    }
                }
                if !manifest_records.is_empty() {
                    if let Some(res) = scope.yield_(CheckpointStep::PersistManifest {
                        records: manifest_records.clone(),
                    }) {
                        if let Err(err) = res {
                            *slot.lock().expect("result slot poisoned") = Some(Err(err));
                            generator::done!();
                        }
                    } else {
                        *slot.lock().expect("result slot poisoned") = Some(Ok(()));
                        generator::done!();
                    }
                }
                if let Some(res) = scope.yield_(CheckpointStep::SwapSuperblock {
                    superblock: superblock.clone(),
                }) {
                    if let Err(err) = res {
                        *slot.lock().expect("result slot poisoned") = Some(Err(err));
                        generator::done!();
                    }
                } else {
                    *slot.lock().expect("result slot poisoned") = Some(Ok(()));
                    generator::done!();
                }
                *slot.lock().expect("result slot poisoned") = Some(Ok(()));
                generator::done!();
            });
        Self {
            inner,
            result: result_slot,
            started: false,
            finished: false,
        }
    }

    pub fn next(&mut self, input: Option<Result<(), CheckpointError>>) -> CheckpointStatus {
        if self.finished {
            panic!("checkpoint coroutine already completed");
        }
        let step = if !self.started {
            self.started = true;
            self.inner.raw_send(None)
        } else {
            let value = input.expect("checkpoint step result required after first yield");
            self.inner.raw_send(Some(value))
        };
        match step {
            Some(step) => CheckpointStatus::InProgress(step),
            None => {
                self.finished = true;
                let res = self
                    .result
                    .lock()
                    .expect("result slot poisoned")
                    .take()
                    .unwrap_or_else(|| Ok(()));
                CheckpointStatus::Finished(res)
            }
        }
    }
}

/// Driver-facing status returned when resuming the coroutine.
#[derive(Debug)]
pub enum CheckpointStatus {
    InProgress(CheckpointStep),
    Finished(Result<(), CheckpointError>),
}

impl CheckpointStatus {
    pub fn unwrap_step(self) -> CheckpointStep {
        match self {
            CheckpointStatus::InProgress(step) => step,
            CheckpointStatus::Finished(_) => panic!("checkpoint already finished"),
        }
    }

    pub fn expect_finished(self) -> Result<(), CheckpointError> {
        match self {
            CheckpointStatus::Finished(res) => res,
            CheckpointStatus::InProgress(_) => panic!("checkpoint still in progress"),
        }
    }
}

/// Units of work surfaced by the checkpoint coroutine.
#[derive(Debug, Clone)]
pub enum CheckpointStep {
    StreamWal { chunk: ChunkFlushPlan },
    FlushChunk { chunk: ChunkFlushPlan },
    PersistManifest { records: Vec<ManifestRecord> },
    SwapSuperblock { superblock: Superblock },
}

/// Errors surfaced while coordinating checkpoints.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("wal streaming failed: {reason}")]
    Wal { reason: String },
    #[error("chunk flush failed: {reason}")]
    ChunkFlush { reason: String },
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Superblock(#[from] SuperblockError),
}

impl CheckpointError {
    pub fn wal(reason: impl Into<String>) -> Self {
        Self::Wal {
            reason: reason.into(),
        }
    }

    pub fn chunk(reason: impl Into<String>) -> Self {
        Self::ChunkFlush {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ManifestPath;
    use crate::manifest::{ChunkMetadata, ManifestDelta, ManifestLogReader, ManifestLogWriter};
    use crate::superblock::SuperblockWriter;
    use std::io::Cursor;
    use tempfile::tempdir;

    fn sample_chunk_metadata(path: &str, generation: u64, wal_seq: u64) -> ChunkMetadata {
        ChunkMetadata {
            chunk_generation: generation,
            wal_sequence: wal_seq,
            bitmap_hash: 0xABCD_EF01,
            page_count: 128,
            size_bytes: 4096 * 4,
            path: ManifestPath::new(path),
        }
    }

    fn sample_superblock() -> Superblock {
        Superblock {
            manifest_path: ManifestPath::new("manifests/main.mlog"),
            manifest_generation: 2,
            wal_sequence: 40,
            snapshot_generation: 2,
        }
    }

    fn chunk_id_from_record(record: &ManifestRecord) -> ChunkId {
        match &record.delta {
            ManifestDelta::UpsertChunk { chunk_id, .. }
            | ManifestDelta::RemoveChunk { chunk_id, .. } => *chunk_id,
            ManifestDelta::SealGeneration => panic!("seal generation has no chunk id"),
        }
    }

    fn sample_plan() -> (CheckpointPlan, ManifestRecord) {
        let chunk = ChunkFlushPlan::new(ChunkId(1), 40, ManifestPath::new("chunks/0001.chk"));
        let metadata = sample_chunk_metadata("chunks/0001.chk", 2, 40);
        let record = ManifestRecord::new(
            2,
            40,
            ManifestDelta::UpsertChunk {
                chunk_id: chunk.chunk_id,
                chunk: metadata,
            },
        );
        let plan = CheckpointPlan::new(sample_superblock())
            .with_chunks(vec![chunk.clone()])
            .with_manifest(vec![record.clone()]);
        (plan, record)
    }

    #[test]
    fn checkpoint_enforces_order() {
        let (plan, _) = sample_plan();
        let mut co = CheckpointCoroutine::new(plan);
        match co.next(None) {
            CheckpointStatus::InProgress(CheckpointStep::StreamWal { chunk }) => {
                assert_eq!(chunk.chunk_id, ChunkId(1));
            }
            other => panic!("unexpected status: {other:?}"),
        }
        match co.next(Some(Ok(()))) {
            CheckpointStatus::InProgress(CheckpointStep::FlushChunk { chunk }) => {
                assert_eq!(chunk.chunk_id, ChunkId(1));
            }
            other => panic!("unexpected status: {other:?}"),
        }
        match co.next(Some(Ok(()))) {
            CheckpointStatus::InProgress(CheckpointStep::PersistManifest { records }) => {
                assert_eq!(records.len(), 1);
            }
            other => panic!("unexpected status: {other:?}"),
        }
        match co.next(Some(Ok(()))) {
            CheckpointStatus::InProgress(CheckpointStep::SwapSuperblock { superblock }) => {
                assert_eq!(superblock.manifest_generation, 2);
            }
            other => panic!("unexpected status: {other:?}"),
        }
        let final_status = co.next(Some(Ok(())));
        assert!(matches!(final_status, CheckpointStatus::Finished(Ok(()))));
    }

    #[test]
    fn flush_failure_aborts_before_superblock() {
        let (plan, _) = sample_plan();
        let mut co = CheckpointCoroutine::new(plan);
        assert!(matches!(
            co.next(None),
            CheckpointStatus::InProgress(CheckpointStep::StreamWal { .. })
        ));
        assert!(matches!(
            co.next(Some(Ok(()))),
            CheckpointStatus::InProgress(CheckpointStep::FlushChunk { .. })
        ));
        let final_result = co
            .next(Some(Err(CheckpointError::chunk("disk failure"))))
            .expect_finished();
        assert!(matches!(
            final_result,
            Err(CheckpointError::ChunkFlush { .. })
        ));
    }

    #[test]
    fn checkpoint_recovery_roundtrip() {
        let (plan, record) = sample_plan();
        let dir = tempdir().expect("tempdir");
        let superblock_path = dir.path().join("superblock.bin");
        let writer = SuperblockWriter::new(&superblock_path);
        let mut manifest_writer = ManifestLogWriter::new(Vec::new());
        let mut flushed_chunks = Vec::new();
        let mut co = CheckpointCoroutine::new(plan);

        let mut status = co.next(None);
        loop {
            match status {
                CheckpointStatus::InProgress(step) => {
                    status = match step {
                        CheckpointStep::StreamWal { .. } => co.next(Some(Ok(()))),
                        CheckpointStep::FlushChunk { chunk } => {
                            flushed_chunks.push(chunk.chunk_id);
                            co.next(Some(Ok(())))
                        }
                        CheckpointStep::PersistManifest { records } => {
                            for r in &records {
                                manifest_writer.append(r).expect("manifest append");
                            }
                            manifest_writer.flush().expect("manifest flush");
                            co.next(Some(Ok(())))
                        }
                        CheckpointStep::SwapSuperblock { superblock } => {
                            assert!(!flushed_chunks.is_empty());
                            writer.commit(&superblock).expect("commit superblock");
                            co.next(Some(Ok(())))
                        }
                    };
                }
                CheckpointStatus::Finished(res) => {
                    res.expect("checkpoint success");
                    break;
                }
            }
        }

        assert_eq!(flushed_chunks, vec![ChunkId(1)]);
        let manifest_bytes = manifest_writer.into_inner();
        let reader = ManifestLogReader::new(Cursor::new(manifest_bytes));
        let state = crate::manifest::replay_manifest(reader).expect("replay manifest");
        assert_eq!(state.manifest_generation(), record.manifest_generation);
        assert_eq!(state.wal_sequence(), record.wal_sequence);
        assert!(state.chunk(chunk_id_from_record(&record)).is_some());
        let loaded = writer.load().expect("load superblock");
        assert_eq!(loaded.manifest_generation, record.manifest_generation);
        assert_eq!(loaded.wal_sequence, record.wal_sequence);
    }
}
