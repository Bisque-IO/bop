use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use tokio::{
    runtime::{Builder, Handle, Runtime},
    sync::Notify,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::trace;

use crate::config::FlushConfig;
use crate::error::{AofError, AofResult};
use crate::metrics::AdmissionMetricsSnapshot;
use crate::store::{
    InstanceId, Tier0Cache, Tier0CacheConfig, Tier1Cache, Tier1Config, Tier2Config, Tier2Manager,
    TieredCoordinator, TieredObservabilitySnapshot,
};

const DEFAULT_TIER0_CLUSTER_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB
const DEFAULT_TIER0_INSTANCE_QUOTA_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB
const DEFAULT_TIER1_CACHE_BYTES: u64 = 8 * 1024 * 1024 * 1024; // 8 GiB
const DEFAULT_RUNTIME_SHUTDOWN_TIMEOUT_SECS: u64 = 5;
const TIERED_SERVICE_IDLE_BACKOFF_MS: u64 = 10;

/// Configures the capacity and behavior of each storage tier managed by the AOF.
///
/// The tiered storage system provides a hierarchy of caches with different
/// performance and capacity characteristics. Each tier has specific roles:
///
/// - **Tier 0**: Fast in-memory cache for active segments
/// - **Tier 1**: SSD-based cache for recently accessed segments
/// - **Tier 2**: Remote storage for long-term persistence (optional)
///
/// # Example
///
/// ```rust
/// use bop_aof::{TieredStoreConfig, Tier0CacheConfig, Tier1Config};
///
/// let config = TieredStoreConfig {
///     tier0: Tier0CacheConfig::new(1 << 30, 256 << 20), // 1GB cluster, 256MB instance
///     tier1: Tier1Config::new(8 << 30),                  // 8GB cache
///     tier2: None,                                       // Disable remote tier
/// };
/// ```
#[derive(Debug, Clone)]
pub struct TieredStoreConfig {
    /// Tier 0 (memory) cache configuration
    pub tier0: Tier0CacheConfig,
    /// Tier 1 (SSD) cache configuration
    pub tier1: Tier1Config,
    /// Tier 2 (remote) storage configuration (optional)
    pub tier2: Option<Tier2Config>,
}

impl Default for TieredStoreConfig {
    fn default() -> Self {
        Self {
            tier0: Tier0CacheConfig::new(
                DEFAULT_TIER0_CLUSTER_BYTES,
                DEFAULT_TIER0_INSTANCE_QUOTA_BYTES,
            ),
            tier1: Tier1Config::new(DEFAULT_TIER1_CACHE_BYTES),
            tier2: None,
        }
    }
}

/// Top-level options for constructing an AofManager and its subsystems.
///
/// This configuration controls the shared infrastructure used by all AOF
/// instances managed by a single AofManager, including the async runtime,
/// tiered storage system, and flush pipeline behavior.
///
/// # Configuration Areas
///
/// - **Runtime**: Tokio runtime configuration for async operations
/// - **Storage**: Tiered cache hierarchy settings
/// - **Flush**: Durability and flush pipeline parameters
///
/// # Example
///
/// ```rust
/// use bop_aof::{AofManagerConfig, TieredStoreConfig};
/// use std::time::Duration;
///
/// let config = AofManagerConfig {
///     runtime_worker_threads: Some(4),                    // 4 async worker threads
///     runtime_shutdown_timeout: Duration::from_secs(10),  // 10 second shutdown timeout
///     store: TieredStoreConfig::default(),               // Default tiered storage
///     flush: Default::default(),                         // Default flush settings
/// };
/// ```
#[derive(Debug, Clone)]
pub struct AofManagerConfig {
    /// Number of worker threads for the async runtime (None = automatic)
    pub runtime_worker_threads: Option<usize>,
    /// Maximum time to wait for graceful runtime shutdown
    pub runtime_shutdown_timeout: Duration,
    /// Flush pipeline configuration
    pub flush: FlushConfig,
    /// Tiered storage system configuration
    pub store: TieredStoreConfig,
}

impl Default for AofManagerConfig {
    fn default() -> Self {
        Self {
            runtime_worker_threads: None,
            runtime_shutdown_timeout: Duration::from_secs(DEFAULT_RUNTIME_SHUTDOWN_TIMEOUT_SECS),
            flush: FlushConfig::default(),
            store: TieredStoreConfig::default(),
        }
    }
}

impl AofManagerConfig {
    pub fn without_tier2(mut self) -> Self {
        self.store.tier2 = None;
        self
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        let mut cfg = Self::default();
        cfg.runtime_worker_threads = Some(2);
        cfg.store.tier0 = Tier0CacheConfig::new(32 * 1024 * 1024, 32 * 1024 * 1024);
        cfg.store.tier1 = Tier1Config::new(64 * 1024 * 1024).with_worker_threads(1);
        cfg.store.tier2 = None;
        cfg
    }
}

/// Per-instance metadata tracked atomically for coordination and recovery.
///
/// Maintains critical per-instance state that needs to be shared across
/// threads and persisted for recovery purposes. Uses atomic operations
/// for lock-free access patterns.
///
/// # Thread Safety
///
/// All operations on this struct are lock-free and safe for concurrent
/// access from multiple threads. The atomic fields ensure consistency
/// without requiring explicit synchronization.
#[derive(Debug)]
pub struct InstanceMetadata {
    /// Current external ID counter for the instance
    current_ext_id: AtomicU64,
}

impl InstanceMetadata {
    /// Creates new instance metadata with default values.
    fn new() -> Self {
        Self {
            current_ext_id: AtomicU64::new(0),
        }
    }

    /// Returns the current external ID for this instance.
    ///
    /// Uses acquire ordering to ensure visibility of any updates
    /// made by other threads before the ID was set.
    pub fn current_ext_id(&self) -> u64 {
        self.current_ext_id.load(Ordering::Acquire)
    }

    /// Sets the current external ID for this instance.
    ///
    /// Uses release ordering to ensure that any updates made
    /// before this call are visible to subsequent acquire loads.
    ///
    /// # Arguments
    ///
    /// * `ext_id` - The new external ID value
    pub fn set_current_ext_id(&self, ext_id: u64) {
        self.current_ext_id.store(ext_id, Ordering::Release);
    }
}

/// Registry that owns InstanceMetadata records keyed by InstanceId.
///
/// Provides centralized storage and access to per-instance metadata,
/// enabling coordination across different subsystems. Uses reference
/// counting to allow safe sharing of metadata between components.
///
/// # Thread Safety
///
/// This registry is thread-safe and supports concurrent access from
/// multiple threads. Internal locking ensures consistency during
/// registration and lookup operations.
#[derive(Default)]
pub struct CoordinatorMetadataRegistry {
    /// Map of instance IDs to their metadata, protected by a mutex
    instances: Mutex<HashMap<InstanceId, Arc<InstanceMetadata>>>,
}

impl CoordinatorMetadataRegistry {
    /// Registers an instance and returns its metadata.
    ///
    /// If the instance is already registered, returns the existing metadata.
    /// Otherwise, creates new metadata and registers it.
    ///
    /// # Arguments
    ///
    /// * `instance_id` - The ID of the instance to register
    ///
    /// # Returns
    ///
    /// Reference-counted metadata for the instance
    pub fn register(&self, instance_id: InstanceId) -> Arc<InstanceMetadata> {
        let mut instances = self.instances.lock();
        Arc::clone(
            instances
                .entry(instance_id)
                .or_insert_with(|| Arc::new(InstanceMetadata::new())),
        )
    }

    /// Retrieves metadata for a registered instance.
    ///
    /// # Arguments
    ///
    /// * `instance_id` - The ID of the instance to look up
    ///
    /// # Returns
    ///
    /// Some(metadata) if the instance is registered, None otherwise
    pub fn get(&self, instance_id: InstanceId) -> Option<Arc<InstanceMetadata>> {
        let instances = self.instances.lock();
        instances.get(&instance_id).cloned()
    }

    /// Removes an instance from the registry.
    ///
    /// After unregistration, the instance metadata will no longer be
    /// accessible through this registry, though existing references
    /// may continue to exist.
    ///
    /// # Arguments
    ///
    /// * `instance_id` - The ID of the instance to unregister
    pub fn unregister(&self, instance_id: InstanceId) {
        self.instances.lock().remove(&instance_id);
    }
}

/// Cloneable handle that exposes safe accessors for InstanceMetadata.
#[derive(Clone)]
pub struct InstanceMetadataHandle {
    metadata: Arc<InstanceMetadata>,
}

impl InstanceMetadataHandle {
    pub(crate) fn new(metadata: Arc<InstanceMetadata>) -> Self {
        Self { metadata }
    }

    pub fn set_current_ext_id(&self, ext_id: u64) {
        self.metadata.set_current_ext_id(ext_id);
    }

    pub fn current_ext_id(&self) -> u64 {
        self.metadata.current_ext_id()
    }
}

/// Wrapper around the Tokio runtime used by tiered workers with coordinated shutdown.
pub(crate) struct TieredRuntime {
    runtime: Mutex<Option<Runtime>>,
    handle: Handle,
    shutdown_token: CancellationToken,
    shutdown_timeout: Duration,
}

impl TieredRuntime {
    pub fn create(worker_threads: Option<usize>, shutdown_timeout: Duration) -> AofResult<Self> {
        let mut builder = Builder::new_multi_thread();
        builder.enable_all();
        if let Some(threads) = worker_threads {
            builder.worker_threads(threads.max(1));
        }
        let runtime = builder
            .build()
            .map_err(|err| AofError::other(format!("failed to build Tokio runtime: {err}")))?;
        Ok(Self::from_runtime(runtime, shutdown_timeout))
    }

    pub fn from_runtime(runtime: Runtime, shutdown_timeout: Duration) -> Self {
        let handle = runtime.handle().clone();
        Self {
            runtime: Mutex::new(Some(runtime)),
            handle,
            shutdown_token: CancellationToken::new(),
            shutdown_timeout,
        }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    pub fn shutdown(&self) {
        self.shutdown_inner();
    }

    /// Internal shutdown logic for graceful runtime termination.
    ///
    /// Cancels the shutdown token to signal all background tasks,
    /// then attempts to shut down the Tokio runtime either via
    /// background shutdown (if already within async context) or
    /// with a timeout for synchronous contexts.
    fn shutdown_inner(&self) {
        if !self.shutdown_token.is_cancelled() {
            self.shutdown_token.cancel();
        }
        let mut guard = self.runtime.lock();
        if let Some(runtime) = guard.take() {
            if tokio::runtime::Handle::try_current().is_ok() {
                runtime.shutdown_background();
            } else {
                runtime.shutdown_timeout(self.shutdown_timeout);
            }
        }
    }
}

impl Drop for TieredRuntime {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}

/// Boots the shared runtime, tiered caches, and flush pipeline used by AOF instances.
///
/// The AofManager is the central orchestrator for the entire AOF system, managing:
/// - Shared async runtime for all background tasks
/// - Tiered storage coordinator and cache hierarchy
/// - Background service tasks (eviction, hydration, etc.)
/// - Coordinated shutdown and resource cleanup
///
/// # Lifecycle
///
/// 1. **Initialization**: Sets up runtime, storage tiers, and background services
/// 2. **Operation**: Provides handles for creating and managing AOF instances
/// 3. **Shutdown**: Coordinates graceful shutdown of all subsystems
///
/// # Resource Management
///
/// The manager owns expensive resources like the Tokio runtime and storage
/// caches that are shared across all AOF instances. This amortizes setup
/// costs and enables efficient resource utilization.
///
/// # Example
///
/// ```rust
/// use bop_aof::{AofManager, AofManagerConfig};
///
/// // Create manager with default configuration
/// let manager = AofManager::with_config(AofManagerConfig::default())?;
/// let handle = manager.handle();
///
/// // Manager automatically shuts down when dropped
/// ```
pub struct AofManager {
    /// Shared async runtime for all AOF operations
    runtime: Arc<TieredRuntime>,
    /// Configuration for flush operations
    flush_config: FlushConfig,
    /// Coordinator for the tiered storage system
    coordinator: Arc<TieredCoordinator>,
    /// Cloneable handle for creating AOF instances
    handle: Arc<AofManagerHandle>,
    /// Background task for tiered storage management
    tiered_task: Mutex<Option<JoinHandle<()>>>,
}

impl AofManager {
    /// Creates a new AofManager with the specified configuration.
    ///
    /// This is the primary constructor that sets up all shared infrastructure:
    /// - Creates and configures the Tokio runtime
    /// - Initializes the tiered storage system
    /// - Starts background service tasks
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the manager and its subsystems
    ///
    /// # Returns
    ///
    /// A configured AofManager ready to create AOF instances
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Runtime creation fails
    /// - Storage tier initialization fails
    /// - Background task spawning fails
    pub fn with_config(config: AofManagerConfig) -> AofResult<Self> {
        let AofManagerConfig {
            runtime_worker_threads,
            runtime_shutdown_timeout,
            flush,
            store,
        } = config;

        let runtime = TieredRuntime::create(runtime_worker_threads, runtime_shutdown_timeout)?;
        Self::from_parts(Arc::new(runtime), flush, store)
    }

    pub fn with_runtime(runtime: Runtime, config: AofManagerConfig) -> AofResult<Self> {
        let AofManagerConfig {
            runtime_worker_threads: _,
            runtime_shutdown_timeout,
            flush,
            store,
        } = config;

        let runtime = TieredRuntime::from_runtime(runtime, runtime_shutdown_timeout);
        Self::from_parts(Arc::new(runtime), flush, store)
    }

    /// Constructs AofManager from pre-configured components.
    ///
    /// Separates construction concerns by accepting already-configured
    /// runtime, flush, and store components. This enables dependency
    /// injection and simplifies testing scenarios.
    ///
    /// Initializes the tiered cache hierarchy, flush pipeline, and
    /// background coordination services.
    fn from_parts(
        runtime: Arc<TieredRuntime>,
        flush: FlushConfig,
        store: TieredStoreConfig,
    ) -> AofResult<Self> {
        let TieredStoreConfig {
            tier0,
            tier1,
            tier2,
        } = store;
        let tier0_cache = Tier0Cache::new(tier0);
        let tier1_cache = Tier1Cache::new(runtime.handle(), tier1)?;
        let tier2_manager = match tier2 {
            Some(cfg) => {
                match crate::test_support::tier2_manager_override(runtime.handle(), &cfg) {
                    Some(result) => Some(result?),
                    None => Some(Tier2Manager::new(runtime.handle(), cfg)?),
                }
            }
            None => None,
        };
        let coordinator = Arc::new(TieredCoordinator::new(
            tier0_cache,
            tier1_cache,
            tier2_manager,
        ));
        let handle = Arc::new(AofManagerHandle::new(
            runtime.clone(),
            coordinator.clone(),
            flush,
        ));
        let tiered_task = Self::spawn_tiered_service(&runtime, coordinator.clone());
        Ok(Self {
            runtime,
            flush_config: flush,
            coordinator,
            handle,
            tiered_task: Mutex::new(Some(tiered_task)),
        })
    }

    /// Spawns the background service for tiered storage management.
    ///
    /// Creates an async task that coordinates segment lifecycle operations
    /// across the tiered storage hierarchy, including compression, upload,
    /// hydration, and eviction processes.
    ///
    /// The service runs until the runtime shutdown token is cancelled.
    fn spawn_tiered_service(
        runtime: &Arc<TieredRuntime>,
        coordinator: Arc<TieredCoordinator>,
    ) -> JoinHandle<()> {
        let shutdown = runtime.shutdown_token();
        let activity = coordinator.activity_signal();
        runtime
            .handle()
            .spawn(Self::run_tiered_service(coordinator, activity, shutdown))
    }

    async fn run_tiered_service(
        coordinator: Arc<TieredCoordinator>,
        activity: Arc<Notify>,
        shutdown: CancellationToken,
    ) {
        let mut idle = false;
        loop {
            if idle {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = activity.notified() => {},
                    _ = tokio::time::sleep(Duration::from_millis(TIERED_SERVICE_IDLE_BACKOFF_MS)) => {},
                };
            } else if shutdown.is_cancelled() {
                break;
            }

            let outcome = tokio::select! {
                _ = shutdown.cancelled() => None,
                outcome = coordinator.poll() => Some(outcome),
            };

            let Some(outcome) = outcome else {
                break;
            };

            if !outcome.stats.is_idle() {
                trace!(
                    drop_events = outcome.stats.drop_events,
                    scheduled_evictions = outcome.stats.scheduled_evictions,
                    activation_grants = outcome.stats.activation_grants,
                    tier2_events = outcome.stats.tier2_events,
                    hydrations = outcome.stats.hydrations,
                    "tiered coordinator poll processed events",
                );
            }

            idle = outcome.stats.is_idle();
        }
    }

    pub fn runtime_handle(&self) -> Handle {
        self.runtime.handle()
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.runtime.shutdown_token()
    }

    pub fn tiered(&self) -> Arc<TieredCoordinator> {
        self.coordinator.clone()
    }

    pub fn handle(&self) -> Arc<AofManagerHandle> {
        self.handle.clone()
    }

    pub fn flush_config(&self) -> &FlushConfig {
        &self.flush_config
    }
}

impl Drop for AofManager {
    fn drop(&mut self) {
        if let Some(handle) = self.tiered_task.lock().take() {
            handle.abort();
        }
        self.runtime.shutdown();
    }
}

/// Cloneable handle that Aof instances use to reach shared runtime facilities.
pub struct AofManagerHandle {
    runtime: Arc<TieredRuntime>,
    coordinator: Arc<TieredCoordinator>,
    flush_config: FlushConfig,
    metadata: Arc<CoordinatorMetadataRegistry>,
}

impl AofManagerHandle {
    fn new(
        runtime: Arc<TieredRuntime>,
        coordinator: Arc<TieredCoordinator>,
        flush_config: FlushConfig,
    ) -> Self {
        Self {
            runtime,
            coordinator,
            flush_config,
            metadata: Arc::new(CoordinatorMetadataRegistry::default()),
        }
    }

    pub(crate) fn runtime(&self) -> Arc<TieredRuntime> {
        self.runtime.clone()
    }

    pub fn runtime_handle(&self) -> Handle {
        self.runtime.handle()
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.runtime.shutdown_token()
    }

    pub fn tiered(&self) -> Arc<TieredCoordinator> {
        self.coordinator.clone()
    }

    pub fn flush_config(&self) -> FlushConfig {
        self.flush_config
    }

    pub fn admission_metrics(&self) -> AdmissionMetricsSnapshot {
        self.coordinator.admission_metrics()
    }

    pub fn observability_snapshot(&self) -> TieredObservabilitySnapshot {
        self.coordinator.observability_snapshot()
    }

    pub(crate) fn register_instance_metadata(
        &self,
        instance_id: InstanceId,
    ) -> Arc<InstanceMetadata> {
        self.metadata.register(instance_id)
    }

    pub(crate) fn unregister_instance_metadata(&self, instance_id: InstanceId) {
        self.metadata.unregister(instance_id);
    }

    pub fn metadata_handle(&self, instance_id: InstanceId) -> Option<InstanceMetadataHandle> {
        self.metadata
            .get(instance_id)
            .map(InstanceMetadataHandle::new)
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;
    use tokio::runtime::Builder;

    #[test]
    fn manager_initializes_with_default_store() {
        let manager =
            AofManager::with_config(AofManagerConfig::for_tests()).expect("create manager");
        let tiered = manager.tiered();
        let metrics = tiered.tier0().metrics();
        assert_eq!(metrics.total_bytes, 0);
        drop(tiered);
    }

    #[test]
    fn manager_accepts_existing_runtime() {
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("build runtime");
        let manager = AofManager::with_runtime(runtime, AofManagerConfig::for_tests())
            .expect("create manager");
        let _handle = manager.runtime_handle();
    }
}
