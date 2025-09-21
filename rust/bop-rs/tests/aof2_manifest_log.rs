use std::time::Duration;

use bop_rs::aof2::config::SegmentId;
use bop_rs::aof2::manifest::{
    ManifestLogReader, ManifestRecord, ManifestRecordPayload, RECORD_HEADER_LEN,
};
use bop_rs::aof2::store::{ManifestLogWriter, ManifestLogWriterConfig};

fn temporary_manifest_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn writer_config() -> ManifestLogWriterConfig {
    ManifestLogWriterConfig {
        chunk_capacity_bytes: 128,
        rotation_period: Duration::from_secs(10),
        growth_step_bytes: 64 * 1024,
        enabled: true,
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
        } => {
            assert_eq!(*durable_bytes, 1024);
            assert_eq!(*segment_crc64, 0xAABBCCDDu64);
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
#[ignore = "pending crash mid-commit scenario"]
async fn manifest_crash_mid_commit() {
    let dir = temporary_manifest_dir();
    let _writer =
        ManifestLogWriter::new(dir.path().to_path_buf(), 1, writer_config()).expect("writer");
    drop(dir);
}

#[tokio::test]
#[ignore = "pending tier2 retry scenario"]
async fn manifest_tier2_retry_flow() {
    let dir = temporary_manifest_dir();
    let _writer =
        ManifestLogWriter::new(dir.path().to_path_buf(), 2, writer_config()).expect("writer");
    drop(dir);
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
