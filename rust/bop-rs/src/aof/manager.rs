//! AOF Manager for managing multiple AOF instances
//!
//! The AofManager provides a centralized interface for managing multiple AOF logs,
//! supporting use cases like partitioned data, multiple streams, or tenant isolation.

use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{Mutex, RwLock};

use crate::aof::aof::Aof;
use crate::aof::archive::ArchiveStorage;
use crate::aof::error::{AofError, AofResult};
use crate::aof::filesystem::FileSystem;
use crate::aof::reader::{AofMetrics, Reader};
use crate::aof::record::AofConfig;

/// Comprehensive metrics for the AOF Manager
#[derive(Debug)]
pub struct AofManagerMetrics {
    // Instance management
    pub total_instances: AtomicU64,
    pub active_instances: AtomicU64,
    pub failed_instances: AtomicU64,

    // Operation counters across all instances
    pub total_appends: AtomicU64,
    pub total_flushes: AtomicU64,
    pub total_readers_created: AtomicU64,

    // Performance metrics
    pub operation_latency_histogram: StdMutex<Histogram<u64>>,
    pub instances_per_operation_histogram: StdMutex<Histogram<u64>>,

    // Error tracking
    pub append_errors: AtomicU64,
    pub flush_errors: AtomicU64,
    pub instance_creation_errors: AtomicU64,
}

impl Default for AofManagerMetrics {
    fn default() -> Self {
        Self {
            total_instances: AtomicU64::new(0),
            active_instances: AtomicU64::new(0),
            failed_instances: AtomicU64::new(0),
            total_appends: AtomicU64::new(0),
            total_flushes: AtomicU64::new(0),
            total_readers_created: AtomicU64::new(0),
            operation_latency_histogram: StdMutex::new(Histogram::new(3).unwrap()),
            instances_per_operation_histogram: StdMutex::new(Histogram::new(3).unwrap()),
            append_errors: AtomicU64::new(0),
            flush_errors: AtomicU64::new(0),
            instance_creation_errors: AtomicU64::new(0),
        }
    }
}

impl AofManagerMetrics {
    /// Record an operation across multiple instances
    pub fn record_multi_instance_operation(&self, latency_us: u64, instance_count: u64) {
        if let Ok(mut hist) = self.operation_latency_histogram.lock() {
            let _ = hist.record(latency_us);
        }
        if let Ok(mut hist) = self.instances_per_operation_histogram.lock() {
            let _ = hist.record(instance_count);
        }
    }

    /// Get operation latency percentile
    pub fn get_operation_latency_percentile(&self, percentile: f64) -> Option<u64> {
        Some(
            self.operation_latency_histogram
                .lock()
                .ok()?
                .value_at_percentile(percentile),
        )
    }

    /// Get instance count percentile for operations
    pub fn get_instances_per_operation_percentile(&self, percentile: f64) -> Option<u64> {
        Some(
            self.instances_per_operation_histogram
                .lock()
                .ok()?
                .value_at_percentile(percentile),
        )
    }

    /// Get total error count
    pub fn total_errors(&self) -> u64 {
        self.append_errors.load(Ordering::Relaxed)
            + self.flush_errors.load(Ordering::Relaxed)
            + self.instance_creation_errors.load(Ordering::Relaxed)
    }
}

/// Configuration for AOF instance creation
#[derive(Debug, Clone)]
pub struct AofInstanceConfig {
    pub config: AofConfig,
    pub base_path: String,
    pub auto_create: bool,
}

impl Default for AofInstanceConfig {
    fn default() -> Self {
        Self {
            config: AofConfig::default(),
            base_path: "aof".to_string(),
            auto_create: true,
        }
    }
}

/// Strategy for selecting AOF instances for operations
#[derive(Debug, Clone)]
pub enum SelectionStrategy {
    /// Use specific instance by ID
    Specific(String),
    /// Use all instances
    All,
    /// Use instances matching a pattern
    Pattern(String),
    /// Use a subset of instances by IDs
    Subset(Vec<String>),
    /// Use instances based on a hash of the data
    Hash,
    /// Use round-robin selection
    RoundRobin,
}

/// Manager for multiple AOF instances with comprehensive operations support
pub struct AofManager<
    FS: FileSystem + Send + Sync + Clone + 'static,
    A: ArchiveStorage + Send + Sync + Clone + 'static,
> {
    /// File system implementation
    fs: FS,
    /// Archive storage implementation
    archive_storage: A,
    /// Map of AOF instance ID to AOF instance
    instances: Arc<RwLock<HashMap<String, Arc<Mutex<Aof<FS, A>>>>>>,
    /// Default configuration for new instances
    default_config: AofInstanceConfig,
    /// Manager metrics
    metrics: Arc<AofManagerMetrics>,
    /// Round-robin counter for selection
    round_robin_counter: AtomicU64,
}

impl<
    FS: FileSystem + Send + Sync + Clone + 'static,
    A: ArchiveStorage + Send + Sync + Clone + 'static,
> AofManager<FS, A>
{
    /// Create a new AOF manager
    pub fn new(fs: FS, archive_storage: A, default_config: AofInstanceConfig) -> Self {
        Self {
            fs,
            archive_storage,
            instances: Arc::new(RwLock::new(HashMap::new())),
            default_config,
            metrics: Arc::new(AofManagerMetrics::default()),
            round_robin_counter: AtomicU64::new(0),
        }
    }

    /// Get manager metrics
    pub fn metrics(&self) -> &Arc<AofManagerMetrics> {
        &self.metrics
    }

    /// Create or get an AOF instance with the given ID
    pub async fn get_or_create_instance(
        &self,
        instance_id: &str,
    ) -> AofResult<Arc<Mutex<Aof<FS, A>>>> {
        let instances = self.instances.read().await;

        if let Some(instance) = instances.get(instance_id) {
            return Ok(instance.clone());
        }

        drop(instances);

        // Need to create a new instance
        let mut instances = self.instances.write().await;

        // Double-check in case another task created it
        if let Some(instance) = instances.get(instance_id) {
            return Ok(instance.clone());
        }

        match self.create_instance(instance_id).await {
            Ok(instance) => {
                let instance_arc = Arc::new(Mutex::new(instance));
                instances.insert(instance_id.to_string(), instance_arc.clone());

                self.metrics.total_instances.fetch_add(1, Ordering::Relaxed);
                self.metrics
                    .active_instances
                    .fetch_add(1, Ordering::Relaxed);

                Ok(instance_arc)
            }
            Err(e) => {
                self.metrics
                    .instance_creation_errors
                    .fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    /// Create a new AOF instance with custom configuration
    pub async fn create_instance_with_config(
        &self,
        instance_id: &str,
        config: AofInstanceConfig,
    ) -> AofResult<Arc<Mutex<Aof<FS, A>>>> {
        let instance = self.create_instance_internal(instance_id, &config).await?;
        let instance_arc = Arc::new(Mutex::new(instance));

        let mut instances = self.instances.write().await;
        instances.insert(instance_id.to_string(), instance_arc.clone());

        self.metrics.total_instances.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .active_instances
            .fetch_add(1, Ordering::Relaxed);

        Ok(instance_arc)
    }

    /// Remove an AOF instance
    pub async fn remove_instance(&self, instance_id: &str) -> AofResult<()> {
        let mut instances = self.instances.write().await;

        if instances.remove(instance_id).is_some() {
            self.metrics
                .active_instances
                .fetch_sub(1, Ordering::Relaxed);
        }

        Ok(())
    }

    /// List all instance IDs
    pub async fn list_instances(&self) -> Vec<String> {
        let instances = self.instances.read().await;
        instances.keys().cloned().collect()
    }

    /// Get the number of active instances
    pub async fn instance_count(&self) -> usize {
        let instances = self.instances.read().await;
        instances.len()
    }

    /// Flush instances based on selection strategy
    pub async fn flush(&self, strategy: SelectionStrategy) -> AofResult<Vec<String>> {
        let start_time = std::time::Instant::now();
        let selected_instances = self.select_instances(&strategy).await?;

        let mut flushed_instances = Vec::new();
        let mut error_count = 0;

        for (instance_id, instance) in selected_instances {
            match {
                let mut aof = instance.lock().await;
                aof.flush().await
            } {
                Ok(_) => {
                    flushed_instances.push(instance_id);
                }
                Err(e) => {
                    error_count += 1;
                    self.metrics.flush_errors.fetch_add(1, Ordering::Relaxed);
                    eprintln!("Failed to flush instance {}: {}", instance_id, e);
                }
            }
        }

        let latency_us = start_time.elapsed().as_micros() as u64;
        self.metrics
            .record_multi_instance_operation(latency_us, flushed_instances.len() as u64);
        self.metrics
            .total_flushes
            .fetch_add(flushed_instances.len() as u64, Ordering::Relaxed);

        if flushed_instances.is_empty() && error_count > 0 {
            return Err(AofError::Other("All flush operations failed".to_string()));
        }

        Ok(flushed_instances)
    }

    /// Create readers for instances based on selection strategy
    pub async fn create_readers_from_id(
        &self,
        record_id: u64,
        strategy: SelectionStrategy,
    ) -> AofResult<Vec<(String, Reader<FS>)>> {
        let selected_instances = self.select_instances(&strategy).await?;
        let mut readers = Vec::new();

        for (instance_id, instance) in selected_instances {
            match {
                let aof = instance.lock().await;
                aof.create_reader_from_id(record_id)
            } {
                Ok(reader) => {
                    readers.push((instance_id, reader));
                    self.metrics
                        .total_readers_created
                        .fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    eprintln!(
                        "Failed to create reader for instance {}: {}",
                        instance_id, e
                    );
                }
            }
        }

        Ok(readers)
    }

    /// Create readers from timestamp for instances based on selection strategy
    pub async fn create_readers_from_timestamp(
        &self,
        timestamp: u64,
        strategy: SelectionStrategy,
    ) -> AofResult<Vec<(String, Reader<FS>)>> {
        let selected_instances = self.select_instances(&strategy).await?;
        let mut readers = Vec::new();

        for (instance_id, instance) in selected_instances {
            match {
                let aof = instance.lock().await;
                aof.create_reader_from_timestamp(timestamp)
            } {
                Ok(reader) => {
                    readers.push((instance_id, reader));
                    self.metrics
                        .total_readers_created
                        .fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    eprintln!(
                        "Failed to create reader for instance {}: {}",
                        instance_id, e
                    );
                }
            }
        }

        Ok(readers)
    }

    /// Get health status of all instances
    pub async fn health_check(&self) -> HashMap<String, bool> {
        let instances = self.instances.read().await;
        let mut health_status = HashMap::new();

        for (instance_id, instance) in instances.iter() {
            // Simple health check - try to get next_id
            let is_healthy = match instance.try_lock() {
                Ok(aof) => {
                    // Instance is accessible and can provide basic info
                    aof.next_id() > 0
                }
                Err(_) => {
                    // Instance is locked/busy but not necessarily unhealthy
                    true
                }
            };

            health_status.insert(instance_id.clone(), is_healthy);
        }

        health_status
    }

    /// Get aggregate metrics across all instances
    pub async fn aggregate_metrics(&self) -> AofResult<HashMap<String, u64>> {
        let instances = self.instances.read().await;
        let mut total_records = 0;
        let mut total_instances = 0;

        for (_, instance) in instances.iter() {
            if let Ok(aof) = instance.try_lock() {
                total_records += aof.get_total_records();
                total_instances += 1;
            }
        }

        let mut metrics = HashMap::new();
        metrics.insert("total_records".to_string(), total_records);
        metrics.insert("active_instances".to_string(), total_instances);
        metrics.insert(
            "total_appends".to_string(),
            self.metrics.total_appends.load(Ordering::Relaxed),
        );
        metrics.insert(
            "total_flushes".to_string(),
            self.metrics.total_flushes.load(Ordering::Relaxed),
        );
        metrics.insert("total_errors".to_string(), self.metrics.total_errors());

        Ok(metrics)
    }

    /// Private helper to create an AOF instance
    async fn create_instance(&self, instance_id: &str) -> AofResult<Aof<FS, A>> {
        self.create_instance_internal(instance_id, &self.default_config)
            .await
    }

    /// Private helper to create an AOF instance with specific config
    async fn create_instance_internal(
        &self,
        instance_id: &str,
        config: &AofInstanceConfig,
    ) -> AofResult<Aof<FS, A>> {
        let instance_path = format!("{}/{}", config.base_path, instance_id);

        // Create directory if needed
        if config.auto_create {
            if let Err(_) = self.fs.file_exists(&instance_path).await {
                // Directory doesn't exist, we'll let the AOF creation handle it
            }
        }

        // Create instance-specific configuration with unique MDBX path
        let mut instance_config = config.config.clone();

        // Generate unique index path for this instance to avoid MDBX conflicts
        if instance_config.index_path.is_none() {
            let unique_index_path = format!(
                "{}/aof_segment_index_{}.mdbx",
                config.base_path.trim_end_matches('/'),
                instance_id
            );
            instance_config.index_path = Some(unique_index_path);
        }

        Aof::open_with_fs_and_config(
            self.fs.clone(),
            Arc::new(self.archive_storage.clone()),
            instance_config,
        )
        .await
    }

    /// Private helper to select instances based on strategy
    async fn select_instances(
        &self,
        strategy: &SelectionStrategy,
    ) -> AofResult<Vec<(String, Arc<Mutex<Aof<FS, A>>>)>> {
        let instances = self.instances.read().await;

        match strategy {
            SelectionStrategy::Specific(instance_id) => {
                if let Some(instance) = instances.get(instance_id) {
                    Ok(vec![(instance_id.clone(), instance.clone())])
                } else {
                    Err(AofError::Other(format!(
                        "Instance '{}' not found",
                        instance_id
                    )))
                }
            }
            SelectionStrategy::All => Ok(instances
                .iter()
                .map(|(id, instance)| (id.clone(), instance.clone()))
                .collect()),
            SelectionStrategy::Pattern(pattern) => {
                let filtered: Vec<_> = instances
                    .iter()
                    .filter(|(id, _)| id.contains(pattern))
                    .map(|(id, instance)| (id.clone(), instance.clone()))
                    .collect();
                Ok(filtered)
            }
            SelectionStrategy::Subset(instance_ids) => {
                let mut selected = Vec::new();
                for id in instance_ids {
                    if let Some(instance) = instances.get(id) {
                        selected.push((id.clone(), instance.clone()));
                    }
                }
                Ok(selected)
            }
            SelectionStrategy::Hash => {
                // For hash-based selection, we'd typically use the data hash
                // For now, just return the first instance if available
                if let Some((id, instance)) = instances.iter().next() {
                    Ok(vec![(id.clone(), instance.clone())])
                } else {
                    Ok(vec![])
                }
            }
            SelectionStrategy::RoundRobin => {
                if instances.is_empty() {
                    return Ok(vec![]);
                }

                let counter = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
                let index = (counter as usize) % instances.len();

                if let Some((id, instance)) = instances.iter().nth(index) {
                    Ok(vec![(id.clone(), instance.clone())])
                } else {
                    Ok(vec![])
                }
            }
        }
    }
}

/// Builder for creating AofManager instances with custom configuration
pub struct AofManagerBuilder<
    FS: FileSystem + Send + Sync + Clone + 'static,
    A: ArchiveStorage + Send + Sync + Clone + 'static,
> {
    fs: Option<FS>,
    archive_storage: Option<A>,
    default_config: AofInstanceConfig,
}

impl<
    FS: FileSystem + Send + Sync + Clone + 'static,
    A: ArchiveStorage + Send + Sync + Clone + 'static,
> AofManagerBuilder<FS, A>
{
    pub fn new() -> Self {
        Self {
            fs: None,
            archive_storage: None,
            default_config: AofInstanceConfig::default(),
        }
    }

    pub fn with_filesystem(mut self, fs: FS) -> Self {
        self.fs = Some(fs);
        self
    }

    pub fn with_archive_storage(mut self, archive_storage: A) -> Self {
        self.archive_storage = Some(archive_storage);
        self
    }

    pub fn with_default_config(mut self, config: AofInstanceConfig) -> Self {
        self.default_config = config;
        self
    }

    pub fn with_base_path(mut self, base_path: String) -> Self {
        self.default_config.base_path = base_path;
        self
    }

    pub fn with_aof_config(mut self, aof_config: AofConfig) -> Self {
        self.default_config.config = aof_config;
        self
    }

    pub fn build(self) -> AofResult<AofManager<FS, A>> {
        let fs = self
            .fs
            .ok_or_else(|| AofError::Other("FileSystem is required".to_string()))?;
        let archive_storage = self
            .archive_storage
            .ok_or_else(|| AofError::Other("ArchiveStorage is required".to_string()))?;
        Ok(AofManager::new(fs, archive_storage, self.default_config))
    }
}

impl<
    FS: FileSystem + Send + Sync + Clone + 'static,
    A: ArchiveStorage + Send + Sync + Clone + 'static,
> Default for AofManagerBuilder<FS, A>
{
    fn default() -> Self {
        Self::new()
    }
}
