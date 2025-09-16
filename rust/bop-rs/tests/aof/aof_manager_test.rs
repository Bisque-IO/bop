//! Basic tests for AOF Manager functionality
//! Full test coverage requires API alignment with current implementation

use std::collections::HashMap;
use tempfile::tempdir;

use bop_rs::aof::*;

/// Helper function to create test manager
async fn create_test_manager() -> (AofManager<LocalFileSystem>, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let fs = LocalFileSystem::new(temp_dir.path()).unwrap();

    let config = AofInstanceConfig::default();
    let manager = AofManager::new(fs, config);

    (manager, temp_dir)
}

#[tokio::test]
async fn test_manager_creation() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Verify manager is created correctly
    assert_eq!(manager.instance_count().await, 0);
    println!("Manager creation test completed");
}

#[tokio::test]
async fn test_instance_creation_and_management() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Test instance creation
    let instance1 = manager.get_or_create_instance("instance-1").await.unwrap();
    let instance2 = manager.get_or_create_instance("instance-2").await.unwrap();
    let instance3 = manager.get_or_create_instance("instance-3").await.unwrap();

    // Verify instances were created
    assert_eq!(manager.instance_count().await, 3);

    // Verify instances are different
    let ptr1 = std::sync::Arc::as_ptr(&instance1);
    let ptr2 = std::sync::Arc::as_ptr(&instance2);
    let ptr3 = std::sync::Arc::as_ptr(&instance3);

    assert_ne!(ptr1, ptr2);
    assert_ne!(ptr2, ptr3);
    assert_ne!(ptr1, ptr3);

    println!("Instance creation and management test completed");
}

#[tokio::test]
async fn test_instance_listing() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Create some instances
    manager.get_or_create_instance("test-1").await.unwrap();
    manager.get_or_create_instance("test-2").await.unwrap();
    manager.get_or_create_instance("test-3").await.unwrap();

    // Test instance listing
    let instances = manager.list_instances().await;
    assert_eq!(instances.len(), 3);

    // Verify all expected instances are present
    assert!(instances.contains(&"test-1".to_string()));
    assert!(instances.contains(&"test-2".to_string()));
    assert!(instances.contains(&"test-3".to_string()));

    println!("Instance listing test completed");
}

#[tokio::test]
async fn test_append_operations() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Create test instances
    manager
        .get_or_create_instance("append-test-1")
        .await
        .unwrap();
    manager
        .get_or_create_instance("append-test-2")
        .await
        .unwrap();

    // Test append with specific instance
    let strategy = SelectionStrategy::Specific("append-test-1".to_string());
    let results = manager.append(b"test data", strategy).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "append-test-1");
    assert_eq!(results[0].1, 1); // First record ID

    println!("Append operations test completed");
}

#[tokio::test]
async fn test_flush_operations() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Create test instances and add data
    manager
        .get_or_create_instance("flush-test-1")
        .await
        .unwrap();
    manager
        .get_or_create_instance("flush-test-2")
        .await
        .unwrap();

    // Add some data
    let strategy = SelectionStrategy::All;
    manager
        .append(b"data for flush test", strategy.clone())
        .await
        .unwrap();

    // Test flush
    let flushed_instances = manager.flush(strategy).await.unwrap();
    assert_eq!(flushed_instances.len(), 2);

    println!("Flush operations test completed");
}

#[tokio::test]
async fn test_health_check() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Create test instances
    manager.get_or_create_instance("health-1").await.unwrap();
    manager.get_or_create_instance("health-2").await.unwrap();

    // Test health check
    let health_status = manager.health_check().await;

    // Should have entries for both instances
    assert_eq!(health_status.len(), 2);
    assert!(health_status.contains_key("health-1"));
    assert!(health_status.contains_key("health-2"));

    // Basic health check should pass for new instances
    for (instance_id, is_healthy) in health_status {
        assert!(is_healthy, "Instance {} should be healthy", instance_id);
    }

    println!("Health check test completed");
}

#[tokio::test]
async fn test_aggregate_metrics() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Create instances and generate some activity
    manager.get_or_create_instance("metrics-1").await.unwrap();
    manager.get_or_create_instance("metrics-2").await.unwrap();

    // Add some data to generate metrics
    let strategy = SelectionStrategy::All;
    manager
        .append(b"metrics test data", strategy.clone())
        .await
        .unwrap();
    manager.flush(strategy).await.unwrap();

    // Test metrics aggregation
    let metrics = manager.aggregate_metrics().await.unwrap();

    // Should have some metrics data
    assert!(!metrics.is_empty());

    println!("Aggregate metrics test completed");
}

#[tokio::test]
async fn test_instance_removal() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Create test instances
    manager
        .get_or_create_instance("removal-test-1")
        .await
        .unwrap();
    manager
        .get_or_create_instance("removal-test-2")
        .await
        .unwrap();

    assert_eq!(manager.instance_count().await, 2);

    // Remove one instance
    manager.remove_instance("removal-test-1").await.unwrap();
    assert_eq!(manager.instance_count().await, 1);

    // Verify correct instance was removed
    let instances = manager.list_instances().await;
    assert!(!instances.contains(&"removal-test-1".to_string()));
    assert!(instances.contains(&"removal-test-2".to_string()));

    println!("Instance removal test completed");
}

#[tokio::test]
async fn test_reader_creation() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Create instance and add test data
    manager.get_or_create_instance("reader-test").await.unwrap();

    let strategy = SelectionStrategy::Specific("reader-test".to_string());

    // Add some records
    for i in 1..=5 {
        let data = format!("Reader test record {}", i);
        manager
            .append(data.as_bytes(), strategy.clone())
            .await
            .unwrap();
    }

    manager.flush(strategy.clone()).await.unwrap();

    // Test reader creation from ID
    let readers = manager
        .create_readers_from_id(3, strategy.clone())
        .await
        .unwrap();
    assert_eq!(readers.len(), 1);
    assert_eq!(readers[0].0, "reader-test");
    assert_eq!(readers[0].1.position().record_id, 3);

    // Test reader creation from timestamp
    let timestamp_readers = manager
        .create_readers_from_timestamp(0, strategy)
        .await
        .unwrap();
    assert_eq!(timestamp_readers.len(), 1);

    println!("Reader creation test completed");
}

#[tokio::test]
async fn test_selection_strategies() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Create multiple instances
    manager.get_or_create_instance("strategy-1").await.unwrap();
    manager.get_or_create_instance("strategy-2").await.unwrap();
    manager
        .get_or_create_instance("other-instance")
        .await
        .unwrap();

    // Test Specific strategy
    let specific_results = manager
        .append(
            b"specific test",
            SelectionStrategy::Specific("strategy-1".to_string()),
        )
        .await
        .unwrap();
    assert_eq!(specific_results.len(), 1);
    assert_eq!(specific_results[0].0, "strategy-1");

    // Test All strategy
    let all_results = manager
        .append(b"all test", SelectionStrategy::All)
        .await
        .unwrap();
    assert_eq!(all_results.len(), 3); // All instances

    // Test Subset strategy
    let subset_results = manager
        .append(
            b"subset test",
            SelectionStrategy::Subset(vec!["strategy-1".to_string(), "strategy-2".to_string()]),
        )
        .await
        .unwrap();
    assert_eq!(subset_results.len(), 2);

    // Test Pattern strategy (basic test)
    let pattern_results = manager
        .append(
            b"pattern test",
            SelectionStrategy::Pattern("strategy-*".to_string()),
        )
        .await
        .unwrap();
    // Pattern matching behavior depends on implementation
    assert!(!pattern_results.is_empty());

    println!("Selection strategies test completed");
}

#[tokio::test]
async fn test_manager_metrics() {
    let (manager, _temp_dir) = create_test_manager().await;

    // Get initial metrics
    let metrics = manager.metrics();
    let initial_instances = metrics
        .total_instances
        .load(std::sync::atomic::Ordering::Relaxed);

    // Create some instances
    manager
        .get_or_create_instance("metrics-test-1")
        .await
        .unwrap();
    manager
        .get_or_create_instance("metrics-test-2")
        .await
        .unwrap();

    // Verify metrics are updated
    let total_instances = metrics
        .total_instances
        .load(std::sync::atomic::Ordering::Relaxed);
    assert!(total_instances > initial_instances);

    println!("Manager metrics test completed");
}

// TODO: Add comprehensive manager tests once the following are implemented:
// - Load balancing strategies
// - Instance failover scenarios
// - Performance optimization features
// - Cross-instance coordination
// - Distributed operations
