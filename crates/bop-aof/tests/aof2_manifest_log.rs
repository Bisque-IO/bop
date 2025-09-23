use std::fs::{File, OpenOptions};
use std::future::Future;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bop_aof::config::{AofConfig, FlushConfig, SegmentId};
use bop_aof::error::{AofError, AofResult};
use bop_aof::fs::Layout;
use bop_aof::manifest::{CHUNK_HEADER_LEN, ChunkHeader};
use bop_aof::manifest::{
    ManifestLogReader, ManifestRecord, ManifestRecordPayload, RECORD_HEADER_LEN,
};
use bop_aof::metrics::manifest_replay::{
    METRIC_MANIFEST_REPLAY_CHUNK_COUNT, METRIC_MANIFEST_REPLAY_CHUNK_LAG_SECONDS,
    METRIC_MANIFEST_REPLAY_CORRUPTION_EVENTS, METRIC_MANIFEST_REPLAY_JOURNAL_LAG_BYTES,
    ManifestReplayMetrics,
};
use bop_aof::segment::Segment;
use bop_aof::store::InstanceId;
use bop_aof::store::tier2::{
    GetObjectRequest, GetObjectResult, HeadObjectResult, PutObjectRequest, PutObjectResult,
    RetryPolicy, Tier2Client,
};
use bop_aof::store::{
    ManifestLogWriter, ManifestLogWriterConfig, Tier0Cache, Tier0CacheConfig, Tier1Cache,
    Tier1Config, Tier2Config, Tier2Manager,
};
use futures::future::BoxFuture;
use tempfile::TempDir;
use tokio::runtime::Handle;
use tokio::time::sleep;

fn temporary_manifest_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn chunk_path(base: PathBuf, stream_id: u64, chunk_index: u64) -> PathBuf {
    base.join(format!("{stream_id:020}"))
        .join(format!("{chunk_index:020}.mlog"))
}

fn writer_config() -> ManifestLogWriterConfig {
    ManifestLogWriterConfig {
        chunk_capacity_bytes: 128,
        rotation_period: Duration::from_secs(10),
        growth_step_bytes: 64 * 1024,
        enabled: true,
    }
}

fn build_layout(dir: &TempDir) -> Layout {
    let config = AofConfig {
        root_dir: dir.path().to_path_buf(),
        flush: FlushConfig::default(),
        ..AofConfig::default()
    }
    .normalized();
    let layout = Layout::new(&config);
    layout.ensure().expect("layout ensure");
    layout
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn create_sealed_segment(layout: &Layout, id: u64, payload: &[u8]) -> Arc<Segment> {
    let segment_id = SegmentId::new(id);
    let path = layout.segment_file_path(segment_id, 0, 0);
    let capacity = (payload.len() as u64).max(1024) + 1024;
    let segment = Segment::create_active(segment_id, 0, 0, 0, capacity, path.as_path())
        .expect("segment create");
    let now = current_epoch_ms();
    let append = segment.append_record(payload, now).expect("append record");
    let flush_state = segment.flush_state();
    let _ = flush_state.try_begin_flush();
    flush_state.mark_durable(append.logical_size);
    flush_state.finish_flush();
    segment.mark_durable(append.logical_size);
    segment.seal(now as i64, 0, false).expect("seal segment");
    Arc::new(segment)
}

fn noisy_bytes(len: usize, seed: u32) -> Vec<u8> {
    let mut value = if seed == 0 { 1 } else { seed };
    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        bytes.push((value & 0xFF) as u8);
    }
    bytes
}

fn pump_tier2_events(tier1: &Tier1Cache, manager: &Tier2Manager) {
    let events = manager.drain_events();
    if !events.is_empty() {
        tier1.handle_tier2_events(events);
    }
}

fn load_manifest_records(
    layout: &Layout,
    instance_id: InstanceId,
) -> AofResult<Vec<ManifestRecord>> {
    let reader = ManifestLogReader::open(layout.manifest_dir().to_path_buf(), instance_id.get())?;
    reader.iter().collect()
}

async fn wait_for_result<F, Fut, T>(mut predicate: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    for _ in 0..200 {
        if let Some(value) = predicate().await {
            return Some(value);
        }
        sleep(Duration::from_millis(25)).await;
    }
    None
}

#[derive(Clone)]
struct FlakyTier2Client {
    upload_failures_before_success: u32,
    delete_failures_before_success: u32,
    upload_attempts: Arc<AtomicU32>,
    delete_attempts: Arc<AtomicU32>,
}

impl FlakyTier2Client {
    fn new(upload_failures_before_success: u32, delete_failures_before_success: u32) -> Self {
        Self {
            upload_failures_before_success,
            delete_failures_before_success,
            upload_attempts: Arc::new(AtomicU32::new(0)),
            delete_attempts: Arc::new(AtomicU32::new(0)),
        }
    }

    fn upload_attempts(&self) -> u32 {
        self.upload_attempts.load(Ordering::SeqCst)
    }

    fn delete_attempts(&self) -> u32 {
        self.delete_attempts.load(Ordering::SeqCst)
    }
}

impl Tier2Client for FlakyTier2Client {
    fn put_object(&self, request: PutObjectRequest) -> BoxFuture<'_, AofResult<PutObjectResult>> {
        let attempts = self.upload_attempts.clone();
        let failures = self.upload_failures_before_success;
        Box::pin(async move {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt <= failures {
                return Err(AofError::RemoteStorage(format!(
                    "forced upload failure on attempt {attempt}"
                )));
            }
            let size = tokio::fs::metadata(&request.source)
                .await
                .map_err(AofError::from)?
                .len();
            Ok(PutObjectResult {
                etag: Some(format!("etag-{attempt}")),
                size,
            })
        })
    }

    fn get_object(&self, _request: GetObjectRequest) -> BoxFuture<'_, AofResult<GetObjectResult>> {
        Box::pin(async {
            Err(AofError::InvalidState(
                "flaky client get_object not implemented".to_string(),
            ))
        })
    }

    fn delete_object(&self, _bucket: &str, _key: &str) -> BoxFuture<'_, AofResult<()>> {
        let attempts = self.delete_attempts.clone();
        let failures = self.delete_failures_before_success;
        Box::pin(async move {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt <= failures {
                return Err(AofError::RemoteStorage(format!(
                    "forced delete failure on attempt {attempt}"
                )));
            }
            Ok(())
        })
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
async fn manifest_rotation_boundaries() {
    let dir = temporary_manifest_dir();
    let stream_id = 7;
    let segment = SegmentId::new(11);

    let mut writer = ManifestLogWriter::new(dir.path().to_path_buf(), stream_id, writer_config())
        .expect("writer");

    let open_record = ManifestRecord::new(
        segment,
        0,
        1,
        ManifestRecordPayload::SegmentOpened {
            base_offset: 0,
            epoch: 1,
        },
    );
    writer.append(&open_record).expect("append opened");
    writer.commit().expect("commit opened");

    let seal_record = ManifestRecord::new(
        segment,
        1024,
        2,
        ManifestRecordPayload::SealSegment {
            durable_bytes: 1024,
            segment_crc64: 0xAABBCCDDu64,
            ext_id: 77,
            coordinator_watermark: 555,
            flush_failure: false,
        },
    );
    writer.append(&seal_record).expect("append seal");
    writer.commit().expect("commit seal");

    let upload_payload = ManifestRecordPayload::UploadStarted {
        key: "manifest/chunk/0001".to_string(),
    };
    let payload_len = upload_payload.encode().expect("encode payload").len();
    let upload_record = ManifestRecord::new(segment, 2048, 3, upload_payload);

    let predicted_len = RECORD_HEADER_LEN + payload_len;
    assert!(
        writer
            .maybe_rotate(predicted_len)
            .expect("maybe rotate should succeed")
    );

    writer.append(&upload_record).expect("append upload");
    writer.commit().expect("commit upload");
    writer.rotate_current().expect("seal final chunk");

    drop(writer); // ensure flush to disk

    let reader = ManifestLogReader::open(dir.path().to_path_buf(), stream_id).expect("open reader");
    let mut records = Vec::new();
    for item in reader.iter() {
        records.push(item.expect("record decode"));
    }

    assert_eq!(records.len(), 3);
    match &records[0].payload {
        ManifestRecordPayload::SegmentOpened { base_offset, epoch } => {
            assert_eq!(*base_offset, 0);
            assert_eq!(*epoch, 1);
        }
        other => panic!("unexpected first payload: {other:?}"),
    }
    match &records[1].payload {
        ManifestRecordPayload::SealSegment {
            durable_bytes,
            segment_crc64,
            ext_id,
            coordinator_watermark,
            flush_failure,
        } => {
            assert_eq!(*durable_bytes, 1024);
            assert_eq!(*segment_crc64, 0xAABBCCDDu64);
            assert_eq!(*ext_id, 77);
            assert_eq!(*coordinator_watermark, 555);
            assert!(!*flush_failure);
        }
        other => panic!("unexpected second payload: {other:?}"),
    }
    match &records[2].payload {
        ManifestRecordPayload::UploadStarted { key } => {
            assert_eq!(key, "manifest/chunk/0001");
        }
        other => panic!("unexpected third payload: {other:?}"),
    }
}

#[tokio::test]
async fn manifest_crash_mid_commit() {
    let dir = temporary_manifest_dir();
    let stream_id = 42;
    let segment = SegmentId::new(5);

    let mut writer = ManifestLogWriter::new(dir.path().to_path_buf(), stream_id, writer_config())
        .expect("writer");

    // Commit an initial record that represents the durable JSON manifest state.
    writer
        .append_now(
            segment,
            0,
            ManifestRecordPayload::CompressionDone {
                compressed_bytes: 512,
                dictionary_id: 0,
            },
        )
        .expect("append committed record");
    writer.commit().expect("commit committed record");

    // Append a second record but skip commit to mimic a crash after the log write.
    writer
        .append_now(
            segment,
            0,
            ManifestRecordPayload::Tier2DeleteQueued {
                key: "tier2/objects/stream/segment".to_string(),
            },
        )
        .expect("append uncommitted record");
    drop(writer); // crash before commit

    let chunk_path = chunk_path(dir.path().to_path_buf(), stream_id, 0);
    let mut chunk_file = File::open(&chunk_path).expect("open chunk for inspection");
    let mut header_bytes = vec![0u8; CHUNK_HEADER_LEN];
    chunk_file
        .read_exact(&mut header_bytes)
        .expect("read chunk header");
    let header = ChunkHeader::read_from(&header_bytes).expect("decode chunk header");
    let file_len = chunk_file.metadata().expect("chunk metadata").len() as usize;
    assert!(
        file_len > header.committed_len,
        "crash should leave trailing uncommitted bytes"
    );

    let uncommitted_bytes = file_len - header.committed_len as usize;

    drop(chunk_file);

    let metrics = ManifestReplayMetrics::new();
    let baseline = metrics.snapshot();
    assert_eq!(baseline.chunk_lag_seconds, 0.0);
    assert_eq!(baseline.journal_lag_bytes, 0);

    // Recovery trims the file back to its committed length before replay.
    let trim_file = OpenOptions::new()
        .write(true)
        .open(&chunk_path)
        .expect("open chunk for trimming");
    trim_file
        .set_len(header.committed_len as u64)
        .expect("truncate to committed length");
    trim_file.sync_data().expect("sync trimmed chunk");
    drop(trim_file);

    let replay_start = Instant::now();

    let reader = ManifestLogReader::open(dir.path().to_path_buf(), stream_id).expect("open reader");
    let records = reader
        .iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("collect manifest records");
    let replay_duration = replay_start.elapsed();
    metrics.record_chunk_lag(replay_duration);
    metrics.record_journal_lag_bytes(uncommitted_bytes as u64);

    metrics.record_chunk_count(1);

    let replay_snapshot = metrics.snapshot();
    assert!(
        replay_snapshot.chunk_lag_seconds >= 0.0 && replay_snapshot.chunk_lag_seconds < 5.0,
        "unexpectedly slow manifest replay: {}s",
        replay_snapshot.chunk_lag_seconds
    );
    assert_eq!(
        replay_snapshot.journal_lag_bytes, uncommitted_bytes as u64,
        "trailing bytes should be reported as journal lag"
    );
    assert_eq!(replay_snapshot.chunk_count, 1);
    assert_eq!(replay_snapshot.corruption_events, 0);

    assert_eq!(records.len(), 1, "replay should ignore uncommitted data");
    match &records[0].payload {
        ManifestRecordPayload::CompressionDone {
            compressed_bytes,
            dictionary_id,
        } => {
            assert_eq!(*compressed_bytes, 512);
            assert_eq!(*dictionary_id, 0);
        }
        other => panic!("unexpected replay payload: {other:?}"),
    }

    metrics.clear();
    let cleared = metrics.snapshot();
    assert_eq!(cleared.chunk_lag_seconds, 0.0);
    assert_eq!(cleared.journal_lag_bytes, 0);
    assert_eq!(cleared.chunk_count, 0);
    assert_eq!(cleared.corruption_events, 0);

    assert_eq!(
        METRIC_MANIFEST_REPLAY_CHUNK_LAG_SECONDS,
        "aof_manifest_replay_chunk_lag_seconds"
    );
    assert_eq!(
        METRIC_MANIFEST_REPLAY_JOURNAL_LAG_BYTES,
        "aof_manifest_replay_journal_lag_bytes"
    );
    assert_eq!(
        METRIC_MANIFEST_REPLAY_CHUNK_COUNT,
        "aof_manifest_replay_chunk_count"
    );
    assert_eq!(
        METRIC_MANIFEST_REPLAY_CORRUPTION_EVENTS,
        "aof_manifest_replay_corruption_events"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manifest_tier2_retry_flow() {
    let runtime = Handle::current();
    let temp_dir = temporary_manifest_dir();
    let layout = build_layout(&temp_dir);

    let tier0 = Tier0Cache::new(Tier0CacheConfig::new(8 * 1024 * 1024, 8 * 1024 * 1024));
    let tier0_instance = tier0.register_instance("tier2-retry", None);
    let instance_id = tier0_instance.instance_id();

    let mut tier1_config = Tier1Config::new(1_900)
        .with_worker_threads(1)
        .with_manifest_log_enabled(true);
    tier1_config.manifest_log.chunk_capacity_bytes = 128 * 1024;
    let tier1 = Tier1Cache::new(runtime.clone(), tier1_config).expect("tier1 cache");

    let tier2_config =
        Tier2Config::default().with_retry(RetryPolicy::new(3, Duration::from_millis(0)));
    let flaky_client = FlakyTier2Client::new(1, 1);
    let tier2_manager = Tier2Manager::with_client(
        runtime.clone(),
        tier2_config,
        Arc::new(flaky_client.clone()),
    )
    .expect("tier2 manager");
    tier1.attach_tier2(tier2_manager.handle());

    let tier1_instance = tier1
        .register_instance(instance_id, layout.clone())
        .expect("register tier1 instance");

    let payload_a = noisy_bytes(1_536, 0xACE1);
    let segment_a = create_sealed_segment(&layout, 11, &payload_a);
    let segment_a_id = segment_a.id();
    tier1_instance
        .schedule_compression(segment_a.clone())
        .expect("schedule compression a");
    drop(segment_a);

    let tier2_key = wait_for_result(|| async {
        pump_tier2_events(&tier1, &tier2_manager);
        tier1
            .manifest_snapshot(instance_id)
            .ok()
            .and_then(|entries| {
                entries.into_iter().find_map(|entry| {
                    if entry.segment_id == segment_a_id {
                        entry.tier2.as_ref().map(|meta| meta.object_key.clone())
                    } else {
                        None
                    }
                })
            })
    })
    .await
    .expect("tier2 metadata to be recorded");
    assert!(
        flaky_client.upload_attempts() >= 2,
        "expected at least one upload retry"
    );

    let mut upload_checks = 0;
    loop {
        let records = load_manifest_records(&layout, instance_id).expect("load manifest records");
        if records.iter().any(|record| {
            matches!(
                &record.payload,
                ManifestRecordPayload::UploadDone { key, .. } if key == &tier2_key
            )
        }) {
            break;
        }
        if upload_checks >= 400 {
            panic!(
                "upload done record not observed; payloads: {:?}",
                records
                    .iter()
                    .map(|record| &record.payload)
                    .collect::<Vec<_>>()
            );
        }
        upload_checks += 1;
        pump_tier2_events(&tier1, &tier2_manager);
        sleep(Duration::from_millis(25)).await;
    }

    let payload_b = noisy_bytes(4_096, 0xDEADBEEF);
    let segment_b = create_sealed_segment(&layout, 23, &payload_b);
    let segment_b_id = segment_b.id();
    tier1_instance
        .schedule_compression(segment_b.clone())
        .expect("schedule compression b");
    drop(segment_b);

    let mut compression_checks = 0;
    loop {
        pump_tier2_events(&tier1, &tier2_manager);
        let records = load_manifest_records(&layout, instance_id).expect("load manifest records");
        if records.iter().any(|record| {
            record.segment_id == segment_b_id
                && matches!(
                    record.payload,
                    ManifestRecordPayload::CompressionDone { .. }
                )
        }) {
            break;
        }
        if compression_checks >= 400 {
            panic!(
                "compression record not observed for segment {}",
                segment_b_id.as_u64()
            );
        }
        compression_checks += 1;
        sleep(Duration::from_millis(25)).await;
    }

    let mut eviction_checks = 0;
    loop {
        pump_tier2_events(&tier1, &tier2_manager);
        let snapshot = tier1
            .manifest_snapshot(instance_id)
            .expect("manifest snapshot");
        if !snapshot
            .iter()
            .any(|entry| entry.segment_id == segment_a_id)
        {
            break;
        }
        if eviction_checks >= 400 {
            panic!("segment_a should be evicted from manifest but is still present");
        }
        eviction_checks += 1;
        sleep(Duration::from_millis(25)).await;
    }

    assert!(
        flaky_client.delete_attempts() >= 2,
        "expected at least one delete retry"
    );

    let records = load_manifest_records(&layout, instance_id).expect("load manifest records");
    assert!(
        records.iter().any(|record| {
            record.segment_id == segment_b_id
                && matches!(
                    record.payload,
                    ManifestRecordPayload::CompressionDone { .. }
                )
        }),
        "compression record for segment_b not found in manifest log"
    );
    assert!(
        records
            .iter()
            .any(|record| matches!(&record.payload, ManifestRecordPayload::UploadDone { .. })),
        "upload done record missing from manifest log"
    );
}

#[tokio::test]
#[ignore = "pending snapshot replay scenario"]
async fn manifest_snapshot_replay_flow() {
    let dir = temporary_manifest_dir();
    let _writer =
        ManifestLogWriter::new(dir.path().to_path_buf(), 3, writer_config()).expect("writer");
    drop(dir);
}

#[tokio::test]
#[ignore = "pending follower catch-up scenario"]
async fn manifest_follower_catch_up_flow() {
    let dir = temporary_manifest_dir();
    let _writer =
        ManifestLogWriter::new(dir.path().to_path_buf(), 4, writer_config()).expect("writer");
    drop(dir);
}


