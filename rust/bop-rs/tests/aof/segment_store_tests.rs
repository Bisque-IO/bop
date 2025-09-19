use bop_rs::aof::record::SegmentMetadata;
use bop_rs::aof::segment_store::{SegmentEntry, SegmentStatus};

fn sample_metadata() -> SegmentMetadata {
    SegmentMetadata {
        base_id: 10,
        last_id: 19,
        record_count: 10,
        created_at: 1_000,
        size: 4_096,
        checksum: 0xDEADBEEF,
        compressed: false,
        encrypted: false,
    }
}

#[test]
fn segment_entry_state_transitions_apply_expected_metadata() {
    let metadata = sample_metadata();
    let mut entry = SegmentEntry::new_active(&metadata, "segments/segment_10.log".into(), 0xC0FFEE);

    assert_eq!(entry.status, SegmentStatus::Active);
    assert_eq!(entry.local_path.as_deref(), Some("segments/segment_10.log"));
    assert_eq!(entry.archive_key, None);
    assert_eq!(entry.uncompressed_checksum, 0xC0FFEE);
    assert!(entry.contains_record(metadata.base_id));
    assert!(entry.contains_record(metadata.last_id));

    entry.finalize(2_000);
    assert_eq!(entry.status, SegmentStatus::Finalized);
    assert_eq!(entry.finalized_at, Some(2_000));
    assert!(entry.overlaps_timestamp(1_500));
    assert!(!entry.overlaps_timestamp(2_500));

    entry.archive(
        "archive/segment_10.zst".into(),
        3_000,
        Some(2_048),
        Some(0xFEEDFACE),
    );
    assert_eq!(entry.status, SegmentStatus::ArchivedRemote);
    assert_eq!(entry.archived_at, Some(3_000));
    assert_eq!(entry.archive_key.as_deref(), Some("archive/segment_10.zst"));
    assert_eq!(entry.compressed_size, Some(2_048));
    assert_eq!(entry.compressed_checksum, Some(0xFEEDFACE));
    assert!(entry.is_archived());

    let ratio = entry.compression_ratio.expect("compression ratio set");
    const TOLERANCE: f32 = 1e-6;
    assert!((ratio - 0.5).abs() < TOLERANCE);
}

#[test]
fn segment_entry_range_checks_reflect_record_and_time_bounds() {
    let metadata = SegmentMetadata {
        last_id: 40,
        record_count: 31,
        size: 8_192,
        ..sample_metadata()
    };
    let entry = SegmentEntry::new_finalized(
        &metadata,
        "segments/segment_10.log".into(),
        metadata.size,
        0x1234,
        1_750,
    );

    assert!(entry.contains_record(25));
    assert!(!entry.contains_record(9));
    assert!(!entry.contains_record(41));

    assert!(entry.overlaps_timestamp(metadata.created_at));
    assert!(entry.overlaps_timestamp(1_600));
    assert!(entry.overlaps_timestamp(1_750));
    assert!(!entry.overlaps_timestamp(1_751));
    assert!(!entry.is_archived());
    assert_eq!(entry.status, SegmentStatus::Finalized);
}

#[test]
fn segment_status_reports_archived_states() {
    assert!(!SegmentStatus::Active.is_archived());
    assert!(!SegmentStatus::Finalized.is_archived());
    assert!(SegmentStatus::ArchivedCached.is_archived());
    assert!(SegmentStatus::ArchivedRemote.is_archived());
}
