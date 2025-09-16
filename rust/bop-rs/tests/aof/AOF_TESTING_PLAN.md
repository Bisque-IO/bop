# AOF System Comprehensive Testing Plan

## Overview

This document outlines a comprehensive testing strategy for the fully async AOF (Append-Only File) system that uses SegmentIndex (MDBX) as the source of truth. The testing plan covers all components, edge cases, failure scenarios, and performance characteristics.

## Current Test Status Analysis

**Existing Tests Found:**
- 2 basic AOF creation tests (outdated, need rework)
- 1 segment store lifecycle test (partially complete)
- Various component-level tests (filesystem, archive, S3)
- Missing: Comprehensive integration tests, edge case coverage, failure scenarios

**Major Gaps Identified:**
- No async/await integration testing
- No SegmentIndex persistence testing
- No cross-component failure scenarios
- No performance/load testing
- No recovery/corruption testing
- No concurrent access testing

## Testing Architecture

### Test Organization Structure

```
tests/
├── unit/                    # Individual component tests
│   ├── aof_core_test.rs    # Core AOF functionality
│   ├── segment_test.rs     # Segment operations
│   ├── appender_test.rs    # Appender functionality
│   ├── reader_test.rs      # Reader functionality
│   ├── segment_index_test.rs # MDBX SegmentIndex
│   └── filesystem_test.rs  # Filesystem operations
├── integration/             # Cross-component tests
│   ├── aof_lifecycle_test.rs # End-to-end lifecycle
│   ├── persistence_test.rs   # Persistence & recovery
│   ├── archive_test.rs      # Archive operations
│   └── concurrent_test.rs   # Concurrent access
├── performance/             # Performance benchmarks
│   ├── throughput_test.rs   # Write/read throughput
│   ├── latency_test.rs      # Operation latency
│   └── memory_test.rs       # Memory usage
├── fault_tolerance/         # Failure scenarios
│   ├── corruption_test.rs   # Data corruption handling
│   ├── crash_test.rs        # Crash recovery
│   ├── storage_full_test.rs # Storage exhaustion
│   └── network_failure_test.rs # S3/network failures
└── stress/                  # Stress testing
    ├── load_test.rs         # High load scenarios
    ├── memory_pressure_test.rs # Memory pressure
    └── long_running_test.rs # Long-term stability
```

## Component Testing Requirements

### 1. Core AOF (aof.rs) Tests

#### Unit Tests
- **Creation & Configuration**
  - `test_aof_creation_with_default_config()`
  - `test_aof_creation_with_custom_config()`
  - `test_aof_creation_with_invalid_config()`
  - `test_aof_creation_filesystem_errors()`

- **Basic Operations**
  - `test_append_single_record()`
  - `test_append_multiple_records()`
  - `test_append_large_record()`
  - `test_append_empty_record()`
  - `test_read_existing_record()`
  - `test_read_nonexistent_record()`
  - `test_read_after_append()`

- **Async Operations**
  - `test_async_append_operations()`
  - `test_async_flush_operations()`
  - `test_concurrent_append_read()`

#### Integration Tests
- **Segment Management**
  - `test_segment_rollover_on_size_limit()`
  - `test_segment_rollover_manual_trigger()`
  - `test_segment_finalization()`
  - `test_segment_metadata_persistence()`

- **Index Persistence**
  - `test_segment_index_persistence()`
  - `test_recovery_from_segment_index()`
  - `test_segment_index_corruption_handling()`

### 2. SegmentIndex (MDBX) Tests

#### Functional Tests
- **CRUD Operations**
  - `test_add_segment_to_index()`
  - `test_get_segment_from_index()`
  - `test_update_segment_status()`
  - `test_update_segment_tail()`
  - `test_remove_segment_from_index()`
  - `test_list_all_segments()`

- **Query Operations**
  - `test_find_segment_for_record_id()`
  - `test_get_next_segment_after()`
  - `test_segment_range_queries()`

- **Persistence & Recovery**
  - `test_index_persistence_across_restarts()`
  - `test_index_corruption_detection()`
  - `test_index_repair_operations()`

#### Edge Cases
- **Data Integrity**
  - `test_concurrent_index_operations()`
  - `test_index_transaction_rollback()`
  - `test_large_number_of_segments()`
  - `test_index_size_limits()`

### 3. Segment Tests

#### Memory Mapping Tests
- **Basic Operations**
  - `test_segment_memory_mapping()`
  - `test_segment_lazy_loading()`
  - `test_segment_unloading()`
  - `test_memory_map_failure_handling()`

- **Reading Operations**
  - `test_read_by_id_with_binary_index()`
  - `test_read_by_offset()`
  - `test_sequential_scan_fallback()`
  - `test_read_with_cache_hit_miss()`

#### Binary Index Tests
- **Index Types**
  - `test_in_segment_binary_index()`
  - `test_separate_file_binary_index()`
  - `test_hybrid_binary_index()`
  - `test_binary_index_corruption_handling()`

### 4. Appender Tests

#### Write Operations
- **Basic Functionality**
  - `test_appender_creation()`
  - `test_append_record_with_id()`
  - `test_append_multiple_records()`
  - `test_appender_flush_strategies()`

- **Index Building**
  - `test_binary_index_generation()`
  - `test_index_persistence_strategies()`
  - `test_index_checksum_validation()`

- **Finalization**
  - `test_appender_finalization()`
  - `test_segment_truncation()`
  - `test_metadata_generation()`

#### Edge Cases
- **Size Limits**
  - `test_segment_size_enforcement()`
  - `test_record_size_validation()`
  - `test_segment_near_full_behavior()`

- **Error Handling**
  - `test_write_failure_recovery()`
  - `test_flush_failure_handling()`
  - `test_disk_full_scenarios()`

### 5. Reader Tests

#### Reading Strategies
- **Reader Types**
  - `test_aof_reader_interface()`
  - `test_segment_reader_interface()`
  - `test_reader_positioning()`

- **Performance**
  - `test_reader_with_segment_cache()`
  - `test_reader_cache_eviction()`
  - `test_concurrent_readers()`

### 6. Filesystem Tests

#### Metrics & Performance
- **HDR Histogram Metrics**
  - `test_filesystem_metrics_collection()`
  - `test_latency_histogram_accuracy()`
  - `test_operation_counters()`
  - `test_metrics_under_load()`

- **Async Operations**
  - `test_async_file_operations()`
  - `test_memory_mapping_on_thread_pool()`
  - `test_filesystem_error_propagation()`

## Edge Cases & Failure Scenarios

### 1. Data Corruption Scenarios

#### File System Corruption
- **Test Cases:**
  - `test_segment_file_corruption_detection()`
  - `test_partial_write_corruption()`
  - `test_index_file_corruption_recovery()`
  - `test_mdbx_database_corruption()`

#### Memory Corruption
- **Test Cases:**
  - `test_memory_map_corruption_handling()`
  - `test_cache_corruption_detection()`

### 2. Resource Exhaustion

#### Storage Exhaustion
- **Test Cases:**
  - `test_disk_full_during_write()`
  - `test_disk_full_during_index_update()`
  - `test_segment_rollover_on_disk_full()`

#### Memory Exhaustion
- **Test Cases:**
  - `test_memory_pressure_segment_cache()`
  - `test_large_record_memory_handling()`
  - `test_memory_map_failure_recovery()`

### 3. Concurrent Access Issues

#### Race Conditions
- **Test Cases:**
  - `test_concurrent_append_operations()`
  - `test_concurrent_read_write()`
  - `test_concurrent_segment_rollover()`
  - `test_concurrent_index_updates()`

#### Deadlock Prevention
- **Test Cases:**
  - `test_reader_writer_deadlock_prevention()`
  - `test_index_lock_ordering()`

### 4. Network & Archive Failures

#### S3 Integration
- **Test Cases:**
  - `test_s3_upload_failure_handling()`
  - `test_s3_download_failure_recovery()`
  - `test_s3_network_timeout_handling()`
  - `test_archive_operation_retries()`

### 5. Recovery Scenarios

#### Crash Recovery
- **Test Cases:**
  - `test_recovery_from_unexpected_shutdown()`
  - `test_recovery_with_incomplete_segments()`
  - `test_recovery_with_corrupted_index()`
  - `test_recovery_with_missing_files()`

#### Rollback Scenarios
- **Test Cases:**
  - `test_failed_segment_rollover_recovery()`
  - `test_failed_index_update_rollback()`

## Performance Testing

### 1. Throughput Testing

#### Write Performance
- **Metrics:** Records/second, MB/second
- **Test Cases:**
  - `bench_single_threaded_append()`
  - `bench_multi_threaded_append()`
  - `bench_large_record_append()`
  - `bench_small_record_append()`

#### Read Performance
- **Metrics:** Reads/second, cache hit ratio
- **Test Cases:**
  - `bench_sequential_read()`
  - `bench_random_read()`
  - `bench_cached_read()`
  - `bench_uncached_read()`

### 2. Latency Testing

#### Operation Latency
- **Metrics:** P50, P95, P99, P99.9 latencies
- **Test Cases:**
  - `bench_append_latency()`
  - `bench_read_latency()`
  - `bench_flush_latency()`
  - `bench_segment_rollover_latency()`

### 3. Memory Usage Testing

#### Memory Efficiency
- **Metrics:** Memory usage, cache efficiency
- **Test Cases:**
  - `bench_memory_usage_scaling()`
  - `bench_segment_cache_efficiency()`
  - `bench_memory_map_overhead()`

## Test Implementation Guidelines

### 1. Test Utilities

#### Mock Components
```rust
// Mock filesystem for controlled testing
pub struct MockFileSystem {
    operations: Arc<Mutex<Vec<FilesystemOperation>>>,
    failures: Arc<Mutex<HashMap<String, AofError>>>,
}

// Mock S3 client for archive testing
pub struct MockS3Client {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    failure_patterns: Arc<Mutex<Vec<FailurePattern>>>,
}

// Test data generators
pub struct TestDataGenerator {
    record_sizes: Vec<usize>,
    patterns: Vec<DataPattern>,
}
```

#### Test Helpers
```rust
// AOF test harness
pub struct AofTestHarness {
    temp_dir: TempDir,
    aof: Aof<MockFileSystem>,
    metrics: AofPerformanceMetrics,
}

impl AofTestHarness {
    pub async fn new() -> Self { /* ... */ }
    pub async fn append_test_records(&mut self, count: usize) -> Vec<u64> { /* ... */ }
    pub async fn verify_record_integrity(&self) -> AofResult<()> { /* ... */ }
    pub fn inject_failure(&mut self, failure: TestFailure) { /* ... */ }
}
```

### 2. Property-Based Testing

#### Using proptest for fuzzing
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_append_read_consistency(
        records in prop::collection::vec(any::<Vec<u8>>(), 1..1000)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut harness = AofTestHarness::new().await;

            // Append all records
            let mut record_ids = Vec::new();
            for record in &records {
                let id = harness.aof.append(record).await.unwrap();
                record_ids.push(id);
            }

            // Verify all records can be read back
            for (i, expected_data) in records.iter().enumerate() {
                let actual_data = harness.aof.read(record_ids[i]).await.unwrap().unwrap();
                assert_eq!(actual_data, *expected_data);
            }
        });
    }
}
```

### 3. Integration Test Patterns

#### End-to-End Scenarios
```rust
#[tokio::test]
async fn test_complete_aof_lifecycle() {
    let harness = AofTestHarness::new().await;

    // Phase 1: Write data across multiple segments
    let record_ids = harness.append_test_records(10000).await;

    // Phase 2: Trigger segment rollovers
    harness.force_segment_rollover().await;

    // Phase 3: Archive old segments
    harness.archive_finalized_segments().await;

    // Phase 4: Simulate restart
    let harness2 = AofTestHarness::from_existing(harness.temp_dir()).await;

    // Phase 5: Verify data integrity after restart
    harness2.verify_all_records(&record_ids).await;

    // Phase 6: Continue operations
    let new_record_ids = harness2.append_test_records(1000).await;
    harness2.verify_all_records(&new_record_ids).await;
}
```

## Test Execution Strategy

### 1. Continuous Integration

#### Test Categories
- **Fast Tests** (< 1s): Unit tests, basic integration
- **Medium Tests** (< 30s): Complex integration, some performance
- **Slow Tests** (< 5m): Comprehensive scenarios, stress tests
- **Nightly Tests** (< 2h): Long-running, exhaustive testing

#### Parallel Execution
- Use `cargo nextest` for parallel test execution
- Group tests by resource requirements
- Isolate tests with temporary directories

### 2. Test Data Management

#### Test Data Sets
- **Small**: 100 records, 1KB each
- **Medium**: 10,000 records, 10KB each
- **Large**: 1,000,000 records, 100KB each
- **Mixed**: Varying sizes from 1B to 1MB

#### Cleanup Strategy
- Automatic cleanup with `TempDir`
- Manual cleanup for debugging scenarios
- Resource monitoring during tests

## Success Criteria

### 1. Code Coverage
- **Target**: 90%+ line coverage
- **Critical paths**: 100% coverage for error handling
- **Component coverage**: Each component >85%

### 2. Performance Benchmarks
- **Append throughput**: >100,000 ops/sec
- **Read throughput**: >1,000,000 ops/sec
- **P99 latency**: <10ms for append, <1ms for read
- **Memory efficiency**: <1GB for 1M records

### 3. Reliability Metrics
- **MTBF**: >1000 hours under normal load
- **Recovery time**: <5 seconds from crash
- **Data loss**: 0% for committed records
- **Corruption detection**: 100% detection rate

## Implementation Priority

### Phase 1: Core Functionality (Week 1-2)
1. Basic AOF unit tests
2. SegmentIndex functionality tests
3. Async operation tests
4. Basic integration tests

### Phase 2: Edge Cases (Week 3-4)
1. Failure scenario tests
2. Resource exhaustion tests
3. Corruption handling tests
4. Recovery tests

### Phase 3: Performance (Week 5-6)
1. Throughput benchmarks
2. Latency measurements
3. Memory usage tests
4. Load testing

### Phase 4: Stress Testing (Week 7-8)
1. Long-running stability tests
2. Concurrent access stress tests
3. Memory pressure tests
4. Network failure simulation

This comprehensive testing plan ensures the AOF system is robust, performant, and reliable under all conditions while maintaining data integrity and consistency.