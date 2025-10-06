//! Tests for the manifest module.

use super::change_log::CHANGE_LOG_TRUNCATION_THRESHOLD;
use super::operations::persist_pending_batch;
use super::state::{RUNTIME_STATE_KEY, RUNTIME_STATE_VERSION};
use super::worker::ManifestBatch;
use super::*;
use heed::EnvOpenOptions;
use heed::types::{SerdeBincode, U32};
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const TEST_COMPONENT: ComponentId = 1;

#[test]
fn lmdb_keys_iterate_in_numeric_order() {
    fn chunk_record(name: &str) -> ChunkEntryRecord {
        let mut record = ChunkEntryRecord::default();
        record.file_name = name.to_string();
        record.size_bytes = 1;
        record
    }

    #[cfg(feature = "libsql")]
    fn chunk_delta_record(base: ChunkId, delta_id: u16, name: &str) -> LibSqlChunkDeltaRecord {
        let mut record = LibSqlChunkDeltaRecord::default();
        record.base_chunk_id = base;
        record.delta_id = delta_id;
        record.delta_file = name.to_string();
        record.size_bytes = 1;
        record
    }

    #[cfg(feature = "libsql")]
    fn snapshot_record(
        db_id: DbId,
        snapshot_id: SnapshotId,
        chunk_id: ChunkId,
    ) -> LibSqlSnapshotRecord {
        LibSqlSnapshotRecord {
            record_version: SNAPSHOT_RECORD_VERSION,
            db_id,
            snapshot_id,
            created_at_epoch_ms: epoch_millis(),
            source_generation: 0,
            chunks: vec![SnapshotChunkRef::base(chunk_id)],
        }
    }

    fn metric_delta(count: u64) -> MetricDelta {
        MetricDelta {
            count,
            sum: count as f64,
            min: Some(count as f64),
            max: Some(count as f64),
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

    let mut txn = manifest.begin();
    txn.put_aof_state(AofStateKey::new(2), AofStateRecord::default());
    txn.put_aof_state(AofStateKey::new(1), AofStateRecord::default());

    txn.upsert_chunk(ChunkKey::new(2, 5), chunk_record("chunk-2-5.bin"));
    txn.upsert_chunk(ChunkKey::new(1, 7), chunk_record("chunk-1-7.bin"));
    txn.upsert_chunk(ChunkKey::new(1, 3), chunk_record("chunk-1-3.bin"));

    #[cfg(feature = "libsql")]
    {
        txn.upsert_chunk_delta(
            ChunkDeltaKey::new(2, 5, 4),
            chunk_delta_record(5, 4, "delta-2-5-4.bin"),
        );
        txn.upsert_chunk_delta(
            ChunkDeltaKey::new(1, 3, 9),
            chunk_delta_record(3, 9, "delta-1-3-9.bin"),
        );
        txn.upsert_chunk_delta(
            ChunkDeltaKey::new(1, 3, 1),
            chunk_delta_record(3, 1, "delta-1-3-1.bin"),
        );

        let snapshot_a = snapshot_record(2, 9, 5);
        let snapshot_b = snapshot_record(1, 4, 3);
        txn.publish_libsql_snapshot(snapshot_a);
        txn.publish_libsql_snapshot(snapshot_b);
    }

    txn.register_wal_artifact(
        WalArtifactKey::new(2, 10),
        AofWalArtifactRecord::new(
            2,
            10,
            WalArtifactKind::AppendOnlySegment {
                start_page_index: 0,
                end_page_index: 1,
                size_bytes: 1,
            },
            PathBuf::from("artifact-2-10.wal"),
        ),
    );
    txn.register_wal_artifact(
        WalArtifactKey::new(1, 40),
        AofWalArtifactRecord::new(
            1,
            40,
            WalArtifactKind::AppendOnlySegment {
                start_page_index: 0,
                end_page_index: 1,
                size_bytes: 1,
            },
            PathBuf::from("artifact-1-40.wal"),
        ),
    );

    txn.upsert_pending_job(PendingJobKey::new(2, 200), 3);
    txn.upsert_pending_job(PendingJobKey::new(1, 500), 5);
    txn.upsert_pending_job(PendingJobKey::new(1, 100), 4);

    txn.merge_metric(MetricKey::new(2, 7), metric_delta(1));
    txn.merge_metric(MetricKey::new(1, 2), metric_delta(2));
    txn.merge_metric(MetricKey::new(1, 5), metric_delta(3));

    txn.commit().unwrap();

    #[cfg(feature = "libsql")]
    let (
        aof_state_keys,
        chunk_keys,
        chunk_delta_keys,
        snapshot_keys,
        wal_artifact_keys,
        pending_keys,
        metric_keys,
    ) = manifest
        .read(|tables, txn| {
            let aof_state = {
                let mut cursor = tables.aof_state.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(AofStateKey::decode(raw));
                }
                out
            };
            let chunk = {
                let mut cursor = tables.chunk_catalog.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(ChunkKey::decode(raw));
                }
                out
            };
            let chunk_delta = {
                let mut cursor = tables.libsql_chunk_delta_index.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(ChunkDeltaKey::decode(raw));
                }
                out
            };
            let snapshots = {
                let mut cursor = tables.libsql_snapshot_index.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(SnapshotKey::decode(raw));
                }
                out
            };
            let artifacts = {
                let mut cursor = tables.aof_wal.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(WalArtifactKey::decode(raw));
                }
                out
            };
            let pending = {
                let mut cursor = tables.job_pending_index.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(PendingJobKey::decode(raw));
                }
                out
            };
            let metrics = {
                let mut cursor = tables.metrics.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(MetricKey::decode(raw));
                }
                out
            };
            Ok((
                aof_state,
                chunk,
                chunk_delta,
                snapshots,
                artifacts,
                pending,
                metrics,
            ))
        })
        .unwrap();

    #[cfg(not(feature = "libsql"))]
    let (aof_state_keys, chunk_keys, wal_artifact_keys, pending_keys, metric_keys) = manifest
        .read(|tables, txn| {
            let aof_state = {
                let mut cursor = tables.aof_state.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(AofStateKey::decode(raw));
                }
                out
            };
            let chunk = {
                let mut cursor = tables.chunk_catalog.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(ChunkKey::decode(raw));
                }
                out
            };
            let artifacts = {
                let mut cursor = tables.aof_wal.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(WalArtifactKey::decode(raw));
                }
                out
            };
            let pending = {
                let mut cursor = tables.job_pending_index.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(PendingJobKey::decode(raw));
                }
                out
            };
            let metrics = {
                let mut cursor = tables.metrics.iter(txn)?;
                let mut out = Vec::new();
                while let Some((raw, _)) = cursor.next().transpose()? {
                    out.push(MetricKey::decode(raw));
                }
                out
            };
            Ok((aof_state, chunk, artifacts, pending, metrics))
        })
        .unwrap();

    assert_eq!(
        aof_state_keys
            .iter()
            .map(|key| key.db_id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        chunk_keys
            .iter()
            .map(|key| (key.db_id, key.chunk_id))
            .collect::<Vec<_>>(),
        vec![(1, 3), (1, 7), (2, 5)]
    );
    #[cfg(feature = "libsql")]
    {
        assert_eq!(
            chunk_delta_keys
                .iter()
                .map(|key| (key.db_id, key.chunk_id, key.generation))
                .collect::<Vec<_>>(),
            vec![(1, 3, 1), (1, 3, 9), (2, 5, 4)]
        );
        assert_eq!(
            snapshot_keys
                .iter()
                .map(|key| (key.db_id, key.snapshot_id))
                .collect::<Vec<_>>(),
            vec![(1, 4), (2, 9)]
        );
    }
    assert_eq!(
        wal_artifact_keys
            .iter()
            .map(|key| (key.db_id, key.artifact_id))
            .collect::<Vec<_>>(),
        vec![(1, 40), (2, 10)]
    );
    assert_eq!(
        pending_keys
            .iter()
            .map(|key| (key.db_id, key.job_kind))
            .collect::<Vec<_>>(),
        vec![(1, 100), (1, 500), (2, 200)]
    );
    assert_eq!(
        metric_keys
            .iter()
            .map(|key| (key.scope, key.metric_kind))
            .collect::<Vec<_>>(),
        vec![(1, 2), (1, 5), (2, 7)]
    );
}

#[test]
fn change_log_records_commits_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

    for _ in 0..3 {
        let mut txn = manifest.begin();
        txn.bump_generation(TEST_COMPONENT, 1);
        txn.commit().unwrap();
    }

    let cursor = manifest
        .register_change_cursor(ChangeCursorStart::Oldest)
        .unwrap();
    assert_eq!(cursor.next_sequence, cursor.oldest_sequence);

    let page = manifest.fetch_change_page(cursor.cursor_id, 10).unwrap();
    assert_eq!(page.changes.len(), 3);
    let sequences: Vec<_> = page.changes.iter().map(|change| change.sequence).collect();
    assert_eq!(sequences, vec![1, 2, 3]);
    for change in &page.changes {
        assert!(matches!(
            change.operations.as_slice(),
            [ManifestOp::BumpGeneration { component, .. }]
            if *component == TEST_COMPONENT
        ));
    }

    let ack = manifest
        .acknowledge_changes(cursor.cursor_id, page.next_sequence.saturating_sub(1))
        .unwrap();
    assert_eq!(ack.acked_sequence, 3);
}

#[test]
fn batched_commits_have_ordered_change_log() {
    let dir = tempfile::tempdir().unwrap();
    let mut options = ManifestOptions::default();
    options.commit_latency = Duration::from_millis(80);
    let manifest = Manifest::open(dir.path(), options).unwrap();

    thread::scope(|scope| {
        let manifest_ref = &manifest;
        scope.spawn(move || {
            let mut txn = manifest_ref.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        });

        let mut txn = manifest.begin();
        txn.bump_generation(TEST_COMPONENT, 1);
        txn.commit().unwrap();
    });

    let diagnostics = manifest.diagnostics();
    assert_eq!(diagnostics.committed_batches, 1);

    let cursor = manifest
        .register_change_cursor(ChangeCursorStart::Oldest)
        .unwrap();
    let page = manifest.fetch_change_page(cursor.cursor_id, 10).unwrap();
    let sequences: Vec<_> = page.changes.iter().map(|change| change.sequence).collect();
    assert_eq!(sequences, vec![1, 2]);
}

#[test]
fn cursor_resume_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let cursor_id;
    {
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let cursor = manifest
            .register_change_cursor(ChangeCursorStart::Oldest)
            .unwrap();
        cursor_id = cursor.cursor_id;

        for _ in 0..2 {
            let mut txn = manifest.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        }

        let page = manifest.fetch_change_page(cursor.cursor_id, 1).unwrap();
        assert_eq!(page.changes.len(), 1);
        assert_eq!(page.changes[0].sequence, 1);

        manifest.acknowledge_changes(cursor.cursor_id, 1).unwrap();
    }

    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();
    let page = manifest.fetch_change_page(cursor_id, 10).unwrap();
    assert!(page.changes.first().is_some());
    assert_eq!(page.changes[0].sequence, 2);

    let ack = manifest
        .acknowledge_changes(cursor_id, page.next_sequence.saturating_sub(1))
        .unwrap();
    assert_eq!(ack.acked_sequence, 2);
}

#[test]
fn change_log_truncation_without_cursors() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

    let iterations = (CHANGE_LOG_TRUNCATION_THRESHOLD as usize) + 10;
    for _ in 0..iterations {
        let mut txn = manifest.begin();
        txn.bump_generation(TEST_COMPONENT, 1);
        txn.commit().unwrap();
    }

    let sequences = manifest
        .read(|tables, txn| {
            let mut iter = tables.change_log.iter(txn)?;
            let mut seqs = Vec::new();
            while let Some((seq, _)) = iter.next().transpose()? {
                seqs.push(seq);
            }
            Ok(seqs)
        })
        .unwrap();

    assert!(!sequences.is_empty());
    assert!(
        sequences[0] >= CHANGE_LOG_TRUNCATION_THRESHOLD + 1,
        "expected truncation to advance oldest sequence"
    );
    assert!(
        sequences.len() <= CHANGE_LOG_TRUNCATION_THRESHOLD as usize,
        "log retained {} entries which exceeds threshold {}",
        sequences.len(),
        CHANGE_LOG_TRUNCATION_THRESHOLD
    );
}

#[test]
fn active_cursor_preserves_history_during_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

    let cursor = manifest
        .register_change_cursor(ChangeCursorStart::Oldest)
        .unwrap();

    let iterations = (CHANGE_LOG_TRUNCATION_THRESHOLD as usize) + 10;
    for _ in 0..iterations {
        let mut txn = manifest.begin();
        txn.bump_generation(TEST_COMPONENT, 1);
        txn.commit().unwrap();
    }

    let sequences = manifest
        .read(|tables, txn| {
            let mut iter = tables.change_log.iter(txn)?;
            let mut seqs = Vec::new();
            while let Some((seq, _)) = iter.next().transpose()? {
                seqs.push(seq);
            }
            Ok(seqs)
        })
        .unwrap();

    assert_eq!(sequences.len(), iterations);
    assert_eq!(sequences.first().copied(), Some(1));

    let ack_target = (iterations as u64) / 2;
    manifest
        .acknowledge_changes(cursor.cursor_id, ack_target)
        .unwrap();

    let sequences_after_ack = manifest
        .read(|tables, txn| {
            let mut iter = tables.change_log.iter(txn)?;
            let mut seqs = Vec::new();
            while let Some((seq, _)) = iter.next().transpose()? {
                seqs.push(seq);
            }
            Ok(seqs)
        })
        .unwrap();

    assert_eq!(
        sequences_after_ack.first().copied(),
        Some(ack_target.saturating_add(1))
    );
    assert!(
        sequences_after_ack.len() as u64 <= CHANGE_LOG_TRUNCATION_THRESHOLD,
        "post-ack window exceeded truncation threshold"
    );
}

#[test]
fn truncation_respects_min_ack() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

    for _ in 0..3 {
        let mut txn = manifest.begin();
        txn.bump_generation(TEST_COMPONENT, 1);
        txn.commit().unwrap();
    }

    let cursor_a = manifest
        .register_change_cursor(ChangeCursorStart::Oldest)
        .unwrap();
    let cursor_b = manifest
        .register_change_cursor(ChangeCursorStart::Oldest)
        .unwrap();

    manifest.acknowledge_changes(cursor_a.cursor_id, 3).unwrap();
    manifest.acknowledge_changes(cursor_b.cursor_id, 1).unwrap();

    let sequences = manifest
        .read(|tables, txn| {
            let mut iter = tables.change_log.iter(txn)?;
            let mut seqs = Vec::new();
            while let Some((seq, _)) = iter.next().transpose()? {
                seqs.push(seq);
            }
            Ok(seqs)
        })
        .unwrap();
    assert_eq!(sequences, vec![2, 3]);

    manifest.acknowledge_changes(cursor_b.cursor_id, 3).unwrap();

    let sequences = manifest
        .read(|tables, txn| {
            let mut iter = tables.change_log.iter(txn)?;
            let mut seqs = Vec::new();
            while let Some((seq, _)) = iter.next().transpose()? {
                seqs.push(seq);
            }
            Ok(seqs)
        })
        .unwrap();
    assert!(sequences.is_empty());
}

#[test]
fn cursor_start_sequence_clamps_to_available_window() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

    for _ in 0..3 {
        let mut txn = manifest.begin();
        txn.bump_generation(TEST_COMPONENT, 1);
        txn.commit().unwrap();
    }

    let cursor = manifest
        .register_change_cursor(ChangeCursorStart::Sequence(2))
        .unwrap();
    assert_eq!(cursor.next_sequence, 2);

    manifest.acknowledge_changes(cursor.cursor_id, 3).unwrap();

    for _ in 0..2 {
        let mut txn = manifest.begin();
        txn.bump_generation(TEST_COMPONENT, 1);
        txn.commit().unwrap();
    }

    let cursor_b = manifest
        .register_change_cursor(ChangeCursorStart::Sequence(1))
        .unwrap();
    // All prior entries were truncated, so the cursor should start at the new oldest sequence.
    assert_eq!(cursor_b.next_sequence, cursor_b.oldest_sequence);
    let page = manifest.fetch_change_page(cursor_b.cursor_id, 10).unwrap();
    assert!(page.changes.first().is_some());
    assert_eq!(page.changes[0].sequence, cursor_b.next_sequence);
}

#[test]
fn wait_for_change_unblocks_on_commit() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Arc::new(Manifest::open(dir.path(), ManifestOptions::default()).unwrap());

    let waiter = Arc::clone(&manifest);
    let handle = thread::spawn(move || {
        waiter
            .wait_for_change(0, Some(Instant::now() + Duration::from_secs(1)))
            .unwrap()
    });

    thread::sleep(Duration::from_millis(50));
    let mut txn = manifest.begin();
    txn.bump_generation(TEST_COMPONENT, 1);
    txn.commit().unwrap();

    let latest = handle.join().unwrap();
    assert!(latest >= 1);
}

#[test]
fn batching_coalesces_commits() {
    let dir = tempfile::tempdir().unwrap();
    let mut options = ManifestOptions::default();
    options.commit_latency = Duration::from_millis(80);
    let manifest = Manifest::open(dir.path(), options).unwrap();

    thread::scope(|scope| {
        let manifest_ref = &manifest;
        scope.spawn(move || {
            let mut txn = manifest_ref.begin();
            txn.bump_generation(TEST_COMPONENT, 1);
            txn.commit().unwrap();
        });

        let mut txn = manifest.begin();
        txn.bump_generation(TEST_COMPONENT, 1);
        txn.commit().unwrap();
    });

    let diagnostics = manifest.diagnostics();
    assert_eq!(diagnostics.committed_batches, 1);
}

#[test]
fn pending_batches_replay_on_restart() {
    let dir = tempfile::tempdir().unwrap();
    {
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();
        let batch = ManifestBatch {
            ops: vec![ManifestOp::BumpGeneration {
                component: TEST_COMPONENT,
                increment: 1,
                timestamp_ms: epoch_millis(),
            }],
        };
        persist_pending_batch(&manifest.env, &manifest.tables, 1, &batch).unwrap();
    }

    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();
    let start = Instant::now();
    while manifest.current_generation(TEST_COMPONENT).unwrap_or(0) < 1 {
        if start.elapsed() > Duration::from_secs(1) {
            panic!("pending batches were not applied on restart");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[cfg(feature = "libsql")]
fn snapshot_refcounts_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

    let chunk_key = ChunkKey::new(7, 42);
    let mut chunk = ChunkEntryRecord::default();
    chunk.generation = 1;
    chunk.size_bytes = 1024;
    chunk.file_name = String::from("chunk-42.dat");

    let mut txn = manifest.begin();
    txn.upsert_chunk(chunk_key, chunk.clone());
    txn.commit().unwrap();

    let snapshot = LibSqlSnapshotRecord {
        record_version: SNAPSHOT_RECORD_VERSION,
        db_id: 7,
        snapshot_id: 1,
        created_at_epoch_ms: epoch_millis(),
        source_generation: 1,
        chunks: vec![SnapshotChunkRef::base(42)],
    };

    let mut txn = manifest.begin();
    txn.publish_libsql_snapshot(snapshot.clone());
    txn.commit().unwrap();

    let refcount = manifest
        .read(|tables, txn| {
            Ok(tables
                .gc_refcounts
                .get(txn, &(42u64))?
                .map(|record| record.strong)
                .unwrap_or(0))
        })
        .unwrap();
    assert_eq!(refcount, 1);

    let mut txn = manifest.begin();
    txn.drop_libsql_snapshot(snapshot.key());
    txn.commit().unwrap();

    let refcount = manifest
        .read(|tables, txn| {
            Ok(tables
                .gc_refcounts
                .get(txn, &(42u64))?
                .map(|record| record.strong)
                .unwrap_or(0))
        })
        .unwrap();
    assert_eq!(refcount, 0);
}

#[test]
#[cfg(feature = "libsql")]
fn retention_clears_refcount_on_last_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

    let chunk_key = ChunkKey::new(9, 100);
    let mut chunk_entry = ChunkEntryRecord::default();
    chunk_entry.file_name = String::from("chunk-100.bin");

    let mut txn = manifest.begin();
    txn.upsert_chunk(chunk_key, chunk_entry.clone());
    txn.commit().unwrap();

    let snapshot_a = LibSqlSnapshotRecord {
        record_version: SNAPSHOT_RECORD_VERSION,
        db_id: 9,
        snapshot_id: 1,
        created_at_epoch_ms: epoch_millis(),
        source_generation: 1,
        chunks: vec![SnapshotChunkRef::base(100)],
    };
    let snapshot_b = LibSqlSnapshotRecord {
        record_version: SNAPSHOT_RECORD_VERSION,
        db_id: 9,
        snapshot_id: 2,
        created_at_epoch_ms: epoch_millis(),
        source_generation: 1,
        chunks: vec![SnapshotChunkRef::base(100)],
    };

    let mut txn = manifest.begin();
    txn.publish_libsql_snapshot(snapshot_a.clone());
    txn.commit().unwrap();
    let mut txn = manifest.begin();
    txn.publish_libsql_snapshot(snapshot_b.clone());
    txn.commit().unwrap();

    let refcount = manifest
        .read(|tables, txn| {
            Ok(tables
                .gc_refcounts
                .get(txn, &(100u64))?
                .map(|record| record.strong)
                .unwrap_or(0))
        })
        .unwrap();
    assert_eq!(refcount, 2);

    let mut txn = manifest.begin();
    txn.drop_libsql_snapshot(snapshot_a.key());
    txn.commit().unwrap();
    let refcount = manifest
        .read(|tables, txn| {
            Ok(tables
                .gc_refcounts
                .get(txn, &(100u64))?
                .map(|record| record.strong)
                .unwrap_or(0))
        })
        .unwrap();
    assert_eq!(refcount, 1);

    let mut txn = manifest.begin();
    txn.drop_libsql_snapshot(snapshot_b.key());
    txn.commit().unwrap();
    let refcount = manifest
        .read(|tables, txn| {
            Ok(tables
                .gc_refcounts
                .get(txn, &(100u64))?
                .map(|record| record.strong)
                .unwrap_or(0))
        })
        .unwrap();
    assert_eq!(refcount, 0);
}

#[test]
fn fork_db_clones_state_and_refcounts() {
    const SOURCE_DB: DbId = 11;
    const TARGET_DB: DbId = 12;
    const CHUNK_ID: ChunkId = 77;

    let dir = tempfile::tempdir().unwrap();
    let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

    let mut source_descriptor = AofDescriptorRecord::default();
    source_descriptor.name = "source".into();

    let mut target_descriptor = AofDescriptorRecord::default();
    target_descriptor.name = "fork".into();

    let mut chunk_entry = ChunkEntryRecord::default();
    chunk_entry.file_name = "chunk-77.bin".into();
    chunk_entry.generation = 3;
    chunk_entry.size_bytes = 4_096;

    let mut aof_state = AofStateRecord::default();
    aof_state.last_applied_lsn = 1024;

    let wal_artifact_key = WalArtifactKey::new(SOURCE_DB, 42);
    let wal_artifact = AofWalArtifactRecord::new(
        SOURCE_DB,
        42,
        WalArtifactKind::AppendOnlySegment {
            start_page_index: 0,
            end_page_index: 1_024,
            size_bytes: 1_024,
        },
        PathBuf::from("segment-42.wal"),
    );

    let mut txn = manifest.begin();
    txn.put_aof_db(SOURCE_DB, source_descriptor.clone());
    txn.put_aof_state(AofStateKey::new(SOURCE_DB), aof_state.clone());
    txn.upsert_chunk(ChunkKey::new(SOURCE_DB, CHUNK_ID), chunk_entry.clone());
    txn.register_wal_artifact(wal_artifact_key, wal_artifact.clone());
    txn.adjust_refcount(CHUNK_ID, 1);
    txn.commit().unwrap();

    let before_refcount = manifest
        .read(|tables, txn| {
            Ok(tables
                .gc_refcounts
                .get(txn, &(CHUNK_ID as u64))?
                .map(|record| record.strong)
                .unwrap_or(0))
        })
        .unwrap();
    assert_eq!(before_refcount, 1);

    manifest
        .fork_db(SOURCE_DB, TARGET_DB, target_descriptor.clone())
        .unwrap();

    manifest
        .read(|tables, txn| {
            let chunk_key = ChunkKey::new(TARGET_DB, CHUNK_ID).encode();
            assert!(tables.chunk_catalog.get(txn, &chunk_key)?.is_some());

            let aof_key = AofStateKey::new(TARGET_DB).encode();
            let state = tables.aof_state.get(txn, &aof_key)?.unwrap();
            assert_eq!(state.last_applied_lsn, 1024);

            let artifact_key = WalArtifactKey::new(TARGET_DB, 42).encode();
            let artifact = tables.aof_wal.get(txn, &artifact_key)?.unwrap();
            assert_eq!(artifact.db_id, TARGET_DB);
            assert_eq!(artifact.artifact_id, 42);

            Ok(())
        })
        .unwrap();

    let after_refcount = manifest
        .read(|tables, txn| {
            Ok(tables
                .gc_refcounts
                .get(txn, &(CHUNK_ID as u64))?
                .map(|record| record.strong)
                .unwrap_or(0))
        })
        .unwrap();
    assert_eq!(after_refcount, 2);
}

#[test]
fn open_sets_runtime_state_sentinel() {
    let dir = tempfile::tempdir().unwrap();
    let options = ManifestOptions::default();

    let manifest = Manifest::open(dir.path(), options).expect("open manifest");

    assert!(!manifest.crash_detected());
    let runtime = manifest.runtime_state();
    assert_eq!(runtime.record_version, RUNTIME_STATE_VERSION);
    assert_eq!(runtime.pid, process::id());
}

fn open_detects_leftover_runtime_state() {
    let dir = tempfile::tempdir().unwrap();
    let options = ManifestOptions::default();

    {
        let manifest = Manifest::open(dir.path(), options.clone()).expect("first open");
        assert!(!manifest.crash_detected());
    }

    {
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(options.initial_map_size)
                .max_dbs(options.max_dbs)
                .open(dir.path())
                .expect("env open")
        };

        let mut txn = env.write_txn().expect("write txn");
        let runtime_db = env
            .create_database::<U32<heed::byteorder::BigEndian>, SerdeBincode<RuntimeStateRecord>>(
                &mut txn,
                Some("runtime_state"),
            )
            .expect("runtime db");
        let record = RuntimeStateRecord {
            record_version: RUNTIME_STATE_VERSION,
            instance_id: Uuid::new_v4(),
            pid: 9999,
            started_at_epoch_ms: epoch_millis(),
        };
        runtime_db
            .put(&mut txn, &RUNTIME_STATE_KEY, &record)
            .expect("insert runtime state");
        txn.commit().expect("commit runtime state");
    }

    let manifest = Manifest::open(dir.path(), options).expect("reopen manifest");
    assert!(manifest.crash_detected());
    assert_eq!(
        manifest.runtime_state().record_version,
        RUNTIME_STATE_VERSION
    );
}

/// T3: Tests for checkpoint batch operations
mod checkpoint_batch_tests {
    use super::*;

    #[test]
    fn batch_atomic_commit_chunk_operations() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let mut chunk_entry = ChunkEntryRecord::default();
        chunk_entry.file_name = "chunk-1-base.bin".into();
        chunk_entry.generation = 1;
        chunk_entry.size_bytes = 64 * 1024 * 1024;

        #[cfg(feature = "libsql")]
        {
            let mut delta_entry = LibSqlChunkDeltaRecord::default();
            delta_entry.base_chunk_id = 42;
            delta_entry.delta_id = 1;
            delta_entry.delta_file = "delta-1-1.bin".into();
            delta_entry.size_bytes = 1024;

            let mut txn = manifest.begin();
            txn.upsert_chunk(ChunkKey::new(1, 42), chunk_entry.clone());
            txn.upsert_chunk_delta(ChunkDeltaKey::new(1, 42, 1), delta_entry.clone());
            txn.bump_generation(TEST_COMPONENT, 1);
            let receipt = txn.commit().unwrap();

            assert!(receipt.generations.get(&TEST_COMPONENT).is_some());

            let (chunk_exists, delta_exists) = manifest
                .read(|tables, txn| {
                    let chunk = tables
                        .chunk_catalog
                        .get(txn, &ChunkKey::new(1, 42).encode())?
                        .is_some();
                    let delta = tables
                        .libsql_chunk_delta_index
                        .get(txn, &ChunkDeltaKey::new(1, 42, 1).encode())?
                        .is_some();
                    Ok((chunk, delta))
                })
                .unwrap();

            assert!(chunk_exists, "chunk should be committed");
            assert!(delta_exists, "delta should be committed");
        }
    }

    #[test]
    fn batch_atomic_rollback_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let mut chunk_entry = ChunkEntryRecord::default();
        chunk_entry.file_name = "chunk-2-base.bin".into();
        chunk_entry.generation = 1;
        chunk_entry.size_bytes = 64 * 1024 * 1024;

        let mut txn = manifest.begin();
        txn.upsert_chunk(ChunkKey::new(2, 100), chunk_entry.clone());
        txn.commit().unwrap();

        let chunk_exists = manifest
            .read(|tables, txn| {
                Ok(tables
                    .chunk_catalog
                    .get(txn, &ChunkKey::new(2, 100).encode())?
                    .is_some())
            })
            .unwrap();
        assert!(chunk_exists, "initial chunk should exist");
    }

    #[test]
    fn change_log_appends_for_checkpoint_batch() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let mut chunk_entry = ChunkEntryRecord::default();
        chunk_entry.file_name = "checkpoint-chunk.bin".into();
        chunk_entry.generation = 1;
        chunk_entry.size_bytes = 64 * 1024 * 1024;

        #[cfg(feature = "libsql")]
        {
            let mut delta_entry = LibSqlChunkDeltaRecord::default();
            delta_entry.base_chunk_id = 50;
            delta_entry.delta_id = 1;
            delta_entry.delta_file = "checkpoint-delta.bin".into();
            delta_entry.size_bytes = 2048;

            let mut txn = manifest.begin();
            txn.upsert_chunk(ChunkKey::new(3, 50), chunk_entry);
            txn.upsert_chunk_delta(ChunkDeltaKey::new(3, 50, 1), delta_entry);
            txn.commit().unwrap();
        }

        let cursor = manifest
            .register_change_cursor(ChangeCursorStart::Oldest)
            .unwrap();

        let page = manifest.fetch_change_page(cursor.cursor_id, 10).unwrap();

        #[cfg(feature = "libsql")]
        {
            assert_eq!(
                page.changes.len(),
                1,
                "should have one change log entry for the batch"
            );

            let ops = &page.changes[0].operations;
            assert_eq!(ops.len(), 2, "batch should contain both operations");

            let has_upsert_chunk = ops
                .iter()
                .any(|op| matches!(op, ManifestOp::UpsertChunk { .. }));
            let has_upsert_delta = ops
                .iter()
                .any(|op| matches!(op, ManifestOp::UpsertLibSqlChunkDelta { .. }));

            assert!(has_upsert_chunk, "should have UpsertChunk operation");
            assert!(has_upsert_delta, "should have UpsertChunkDelta operation");
        }

        #[cfg(not(feature = "libsql"))]
        {
            assert_eq!(
                page.changes.len(),
                0,
                "should have no change log entries without libsql feature"
            );
        }
    }

    #[test]
    #[cfg(feature = "libsql")]
    fn chunk_delta_upsert_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let mut delta_entry = LibSqlChunkDeltaRecord::default();
        delta_entry.base_chunk_id = 99;
        delta_entry.delta_id = 5;
        delta_entry.delta_file = "test-delta.bin".into();
        delta_entry.size_bytes = 512;

        let delta_key = ChunkDeltaKey::new(4, 99, 5);

        let mut txn = manifest.begin();
        txn.upsert_chunk_delta(delta_key, delta_entry.clone());
        txn.commit().unwrap();

        let delta_exists = manifest
            .read(|tables, txn| {
                Ok(tables
                    .libsql_chunk_delta_index
                    .get(txn, &delta_key.encode())?
                    .is_some())
            })
            .unwrap();
        assert!(delta_exists, "delta should exist after upsert");

        let mut txn = manifest.begin();
        txn.delete_chunk_delta(delta_key);
        txn.commit().unwrap();

        let delta_exists = manifest
            .read(|tables, txn| {
                Ok(tables
                    .libsql_chunk_delta_index
                    .get(txn, &delta_key.encode())?
                    .is_some())
            })
            .unwrap();
        assert!(!delta_exists, "delta should not exist after delete");
    }

    #[test]
    fn multiple_chunk_operations_in_single_batch() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::open(dir.path(), ManifestOptions::default()).unwrap();

        let mut txn = manifest.begin();

        for chunk_id in 1..=5 {
            let mut chunk_entry = ChunkEntryRecord::default();
            chunk_entry.file_name = format!("chunk-{}.bin", chunk_id);
            chunk_entry.generation = 1;
            chunk_entry.size_bytes = 64 * 1024 * 1024;
            txn.upsert_chunk(ChunkKey::new(5, chunk_id), chunk_entry);
        }

        #[cfg(feature = "libsql")]
        for delta_id in 1..=3 {
            let mut delta_entry = LibSqlChunkDeltaRecord::default();
            delta_entry.base_chunk_id = 1;
            delta_entry.delta_id = delta_id as u16;
            delta_entry.delta_file = format!("delta-1-{}.bin", delta_id);
            delta_entry.size_bytes = 1024 * delta_id as u64;
            txn.upsert_chunk_delta(ChunkDeltaKey::new(5, 1, delta_id), delta_entry);
        }

        txn.commit().unwrap();

        let (chunk_count, delta_count) = manifest
            .read(|tables, txn| {
                let chunks = tables
                    .chunk_catalog
                    .iter(txn)?
                    .filter_map(|r| r.ok())
                    .filter(|(raw, _)| ChunkKey::decode(*raw).db_id == 5)
                    .count();

                #[cfg(feature = "libsql")]
                let deltas = tables
                    .libsql_chunk_delta_index
                    .iter(txn)?
                    .filter_map(|r| r.ok())
                    .filter(|(raw, _)| ChunkDeltaKey::decode(*raw).db_id == 5)
                    .count();
                #[cfg(not(feature = "libsql"))]
                let deltas = 0;

                Ok((chunks, deltas))
            })
            .unwrap();

        assert_eq!(chunk_count, 5, "should have 5 chunks");
        #[cfg(feature = "libsql")]
        assert_eq!(delta_count, 3, "should have 3 deltas");
    }
}
