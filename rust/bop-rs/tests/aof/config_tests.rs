use bop_rs::aof::error::AofError;
use bop_rs::aof::record::{
    AofConfigBuilder, FlushStrategy, MIN_SEGMENT_SIZE, ensure_segment_size_is_valid,
};

#[test]
fn aof_config_builder_applies_customizations() {
    let config = AofConfigBuilder::new()
        .segment_size(2 * MIN_SEGMENT_SIZE)
        .flush_strategy(FlushStrategy::Batched(8))
        .segment_cache_size(32)
        .enable_compression(true)
        .enable_encryption(true)
        .ttl_seconds(Some(3600))
        .pre_allocate_threshold(0.9)
        .pre_allocate_enabled(false)
        .archive_enabled(true)
        .archive_compression_level(5)
        .archive_threshold(10)
        .index_path("/tmp/index.mdbx")
        .build();

    assert_eq!(config.segment_size, 2 * MIN_SEGMENT_SIZE);
    assert!(matches!(config.flush_strategy, FlushStrategy::Batched(8)));
    assert_eq!(config.segment_cache_size, 32);
    assert!(config.enable_compression);
    assert!(config.enable_encryption);
    assert_eq!(config.ttl_seconds, Some(3600));
    assert_eq!(config.pre_allocate_threshold, 0.9);
    assert!(!config.pre_allocate_enabled);
    assert!(config.archive_enabled);
    assert_eq!(config.archive_compression_level, 5);
    assert_eq!(config.archive_threshold, 10);
    assert_eq!(config.index_path.as_deref(), Some("/tmp/index.mdbx"));

    assert_eq!(config.archive_backpressure_segments, 500);
    assert_eq!(config.archive_backpressure_bytes, 10 * 1024 * 1024 * 1024);
}

#[test]
fn ensure_segment_size_is_valid_enforces_bounds() {
    assert!(ensure_segment_size_is_valid(MIN_SEGMENT_SIZE).is_ok());

    let too_small = ensure_segment_size_is_valid(MIN_SEGMENT_SIZE - 1);
    assert!(matches!(too_small, Err(AofError::InvalidSegmentSize(_))));

    let too_large = ensure_segment_size_is_valid(u64::MAX);
    assert!(matches!(too_large, Err(AofError::InvalidSegmentSize(_))));
}
