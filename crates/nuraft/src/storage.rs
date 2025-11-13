use std::fmt;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use maniac_storage::{
    PodStatus, StorageConfig as FleetStorageConfig, StorageFleet, StorageFleetConfig,
    StorageFleetError, StoragePodConfig, StoragePodError, StoragePodHandle, StoragePodId,
};
use maniac_sys::{
    bop_raft_log_store_delete, bop_raft_log_store_ptr, bop_raft_state_mgr_delete,
    bop_raft_state_mgr_ptr,
};

use crate::aof_state_manager::{AofStateManager, AofStateManagerConfig};
use crate::config::{ClusterConfig, ClusterConfigView};
use crate::error::{RaftError, RaftResult};
use crate::segmented::coordinator::{CoordinatorError, SegmentedCoordinator};
use crate::segmented::log_store::{SegmentedLogStore, SegmentedLogStoreConfig};
use crate::segmented::manifest_store::PartitionManifestStore;
use crate::segmented::types::{PartitionDispatcherConfig, ReplicaId};
use crate::state::{ServerState, ServerStateView};
use crate::traits::{LogStoreInterface, StateManagerInterface};
use crate::types::{LogIndex, ServerId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackendKind {
    Callbacks,
    #[cfg(feature = "mdbx")]
    Mdbx,
}

impl StorageBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StorageBackendKind::Callbacks => "callbacks",
            #[cfg(feature = "mdbx")]
            StorageBackendKind::Mdbx => "mdbx",
        }
    }
}

impl fmt::Display for StorageBackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub struct RawLogStore {
    ptr: NonNull<bop_raft_log_store_ptr>,
    backend: StorageBackendKind,
}

impl RawLogStore {
    pub unsafe fn from_raw(
        ptr: *mut bop_raft_log_store_ptr,
        backend: StorageBackendKind,
    ) -> RaftResult<Self> {
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr, backend })
    }

    pub fn as_ptr(&self) -> *mut bop_raft_log_store_ptr {
        self.ptr.as_ptr()
    }

    pub fn backend(&self) -> StorageBackendKind {
        self.backend
    }
}

impl Drop for RawLogStore {
    fn drop(&mut self) {
        unsafe {
            bop_raft_log_store_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for RawLogStore {}
unsafe impl Sync for RawLogStore {}

#[derive(Debug)]
pub struct RawStateManager {
    ptr: NonNull<bop_raft_state_mgr_ptr>,
    backend: StorageBackendKind,
}

impl RawStateManager {
    pub unsafe fn from_raw(
        ptr: *mut bop_raft_state_mgr_ptr,
        backend: StorageBackendKind,
    ) -> RaftResult<Self> {
        let ptr = NonNull::new(ptr).ok_or(RaftError::NullPointer)?;
        Ok(Self { ptr, backend })
    }

    pub fn as_ptr(&self) -> *mut bop_raft_state_mgr_ptr {
        self.ptr.as_ptr()
    }

    pub fn backend(&self) -> StorageBackendKind {
        self.backend
    }
}

impl Drop for RawStateManager {
    fn drop(&mut self) {
        unsafe {
            bop_raft_state_mgr_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for RawStateManager {}
unsafe impl Sync for RawStateManager {}

pub enum LogStoreBuild {
    Callbacks {
        backend: StorageBackendKind,
        store: Box<dyn LogStoreInterface>,
    },
    Raw(RawLogStore),
    #[cfg(feature = "mdbx")]
    Mdbx(crate::mdbx::MdbxLogStore),
}

pub enum StateManagerBuild {
    Callbacks {
        backend: StorageBackendKind,
        manager: Box<dyn StateManagerInterface>,
    },
    Raw(RawStateManager),
    #[cfg(feature = "mdbx")]
    Mdbx(crate::mdbx::MdbxStateManager),
}

impl LogStoreBuild {
    pub fn backend(&self) -> StorageBackendKind {
        match self {
            LogStoreBuild::Callbacks { backend, .. } => *backend,
            LogStoreBuild::Raw(raw) => raw.backend(),
            #[cfg(feature = "mdbx")]
            LogStoreBuild::Mdbx(_) => StorageBackendKind::Mdbx,
        }
    }
}

impl StateManagerBuild {
    pub fn backend(&self) -> StorageBackendKind {
        match self {
            StateManagerBuild::Callbacks { backend, .. } => *backend,
            StateManagerBuild::Raw(raw) => raw.backend(),
            #[cfg(feature = "mdbx")]
            StateManagerBuild::Mdbx(_) => StorageBackendKind::Mdbx,
        }
    }
}

pub struct StorageComponents {
    pub state_manager: StateManagerBuild,
    pub log_store: LogStoreBuild,
    pub storage_runtime: Option<SegmentedStorageRuntime>,
}

impl StorageComponents {
    pub fn new(
        state_manager: Box<dyn StateManagerInterface>,
        log_store: Box<dyn LogStoreInterface>,
        storage_runtime: Option<SegmentedStorageRuntime>,
    ) -> Self {
        Self {
            state_manager: StateManagerBuild::Callbacks {
                backend: state_manager.storage_backend(),
                manager: state_manager,
            },
            log_store: LogStoreBuild::Callbacks {
                backend: log_store.storage_backend(),
                store: log_store,
            },
            storage_runtime,
        }
    }

    pub fn into_parts(
        self,
    ) -> (
        StateManagerBuild,
        LogStoreBuild,
        Option<SegmentedStorageRuntime>,
    ) {
        (self.state_manager, self.log_store, self.storage_runtime)
    }
}

pub struct SegmentedStorageRuntime {
    fleet: StorageFleet,
    segmented_pod: StoragePodHandle,
    state_pod: StoragePodHandle,
}

impl fmt::Debug for SegmentedStorageRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SegmentedStorageRuntime")
            .field("segmented_pod", &self.segmented_pod.id().to_string())
            .field("state_pod", &self.state_pod.id().to_string())
            .finish()
    }
}

impl SegmentedStorageRuntime {
    pub fn segmented_pod(&self) -> StoragePodHandle {
        self.segmented_pod.clone()
    }

    pub fn state_pod(&self) -> StoragePodHandle {
        self.state_pod.clone()
    }

    pub fn fleet(&self) -> &StorageFleet {
        &self.fleet
    }
}

fn map_coordinator_error(err: CoordinatorError) -> RaftError {
    RaftError::LogStoreError(err.to_string())
}

fn coordinator_mutex_error() -> RaftError {
    RaftError::LogStoreError("segmented coordinator mutex poisoned".into())
}

fn map_storage_fleet_error(err: StorageFleetError) -> RaftError {
    RaftError::LogStoreError(format!("storage fleet error: {err}"))
}

fn map_storage_pod_error(err: StoragePodError) -> RaftError {
    RaftError::LogStoreError(format!("storage pod error: {err}"))
}

fn build_segmented_storage_runtime(
    server_id: ServerId,
    instance_id: u64,
    fleet_config: Option<StorageFleetConfig>,
    pods_root: &Path,
) -> RaftResult<SegmentedStorageRuntime> {
    let mut fleet =
        StorageFleet::new(fleet_config.unwrap_or_default()).map_err(map_storage_fleet_error)?;

    let segmented_root = pods_root.join("segmented-log");
    let state_root = pods_root.join("state-manager");
    let storage_cfg = FleetStorageConfig::default();

    let segmented_pod_config = StoragePodConfig::from_root(
        StoragePodId::new(format!(
            "raft-segmented-{}-{}",
            server_id.inner(),
            instance_id
        )),
        &segmented_root,
        storage_cfg,
    );
    let state_pod_config = StoragePodConfig::from_root(
        StoragePodId::new(format!("raft-state-{}-{}", server_id.inner(), instance_id)),
        &state_root,
        storage_cfg,
    );

    let segmented_pod = fleet
        .register_pod(segmented_pod_config)
        .map_err(map_storage_fleet_error)?;
    let state_pod = fleet
        .register_pod(state_pod_config)
        .map_err(map_storage_fleet_error)?;

    Ok(SegmentedStorageRuntime {
        fleet,
        segmented_pod,
        state_pod,
    })
}

fn collect_active_members(view: ClusterConfigView<'_>) -> Vec<ReplicaId> {
    view.servers()
        .filter(|server| !server.is_learner())
        .map(|server| ReplicaId(server.id().0 as u64))
        .collect()
}

fn apply_membership_from_view(
    coordinator: &Arc<Mutex<SegmentedCoordinator>>,
    view: ClusterConfigView<'_>,
) -> RaftResult<()> {
    let members = collect_active_members(view);
    let mut guard = coordinator.lock().map_err(|_| coordinator_mutex_error())?;
    guard
        .update_membership(members)
        .map_err(map_coordinator_error)?;
    Ok(())
}

struct SegmentedStateManager {
    inner: AofStateManager,
    coordinator: Arc<Mutex<SegmentedCoordinator>>,
    #[allow(dead_code)]
    storage_pod: Option<StoragePodHandle>,
}

impl SegmentedStateManager {
    fn new(
        inner: AofStateManager,
        coordinator: Arc<Mutex<SegmentedCoordinator>>,
        initial_config: Option<ClusterConfig>,
        storage_pod: Option<StoragePodHandle>,
    ) -> RaftResult<Self> {
        let manager = Self {
            inner,
            coordinator,
            storage_pod,
        };
        if let Some(config) = initial_config {
            apply_membership_from_view(&manager.coordinator, config.view())?;
        }
        Ok(manager)
    }
}

impl StateManagerInterface for SegmentedStateManager {
    fn storage_backend(&self) -> StorageBackendKind {
        self.inner.storage_backend()
    }

    fn load_state(&self) -> RaftResult<Option<ServerState>> {
        self.inner.load_state()
    }

    fn save_state(&mut self, state: ServerStateView<'_>) -> RaftResult<()> {
        self.inner.save_state(state)
    }

    fn load_config(&self) -> RaftResult<Option<ClusterConfig>> {
        self.inner.load_config()
    }

    fn save_config(&mut self, config: ClusterConfigView<'_>) -> RaftResult<()> {
        self.inner.save_config(config)?;
        apply_membership_from_view(&self.coordinator, config)?;
        Ok(())
    }

    fn load_log_store(&self) -> RaftResult<Option<Box<dyn LogStoreInterface>>> {
        self.inner.load_log_store()
    }

    fn server_id(&self) -> ServerId {
        self.inner.server_id()
    }

    fn system_exit(&self, code: i32) {
        self.inner.system_exit(code)
    }
}

/// Helper to create a segmented manifest store rooted at `base_dir/instance_id`.
pub fn segmented_manifest_store(
    base_dir: impl AsRef<Path>,
    instance_id: u64,
    config: ManifestLogWriterConfig,
) -> RaftResult<PartitionManifestStore> {
    PartitionManifestStore::new(base_dir, instance_id, config)
}

/// Construct storage components using the segmented log/manifest pipeline.
pub fn segmented_storage_components(
    start_index: LogIndex,
    dispatcher_config: PartitionDispatcherConfig,
    manifest_store: PartitionManifestStore,
    state_manager_config: AofStateManagerConfig,
    log_aof_config: AofConfig,
    log_manager_config: AofManagerConfig,
    log_append_timeout: Duration,
    log_verify_checksums: bool,
    partition_root: PathBuf,
    server_id: ServerId,
    instance_id: u64,
    storage_fleet_config: Option<StorageFleetConfig>,
    storage_pods_root: PathBuf,
) -> RaftResult<StorageComponents> {
    let coordinator = SegmentedCoordinator::with_recovery(dispatcher_config, manifest_store)
        .map_err(map_coordinator_error)?;
    let shared_coordinator = Arc::new(Mutex::new(coordinator));

    let storage_runtime = build_segmented_storage_runtime(
        server_id,
        instance_id,
        storage_fleet_config,
        &storage_pods_root,
    )?;
    let segmented_pod_handle = storage_runtime.segmented_pod();
    let state_pod_handle = storage_runtime.state_pod();

    let base_state_manager = AofStateManager::new(state_manager_config)?;
    let initial_config = base_state_manager.load_config()?;
    let state_manager = SegmentedStateManager::new(
        base_state_manager,
        Arc::clone(&shared_coordinator),
        initial_config,
        Some(state_pod_handle.clone()),
    )?;

    let log_config = SegmentedLogStoreConfig::new(
        start_index,
        Arc::clone(&shared_coordinator),
        log_aof_config,
        log_manager_config,
    )
    .with_partition_root(partition_root)
    .append_timeout(log_append_timeout)
    .verify_checksums(log_verify_checksums)
    .with_storage_pod(segmented_pod_handle.clone());
    let log_store = SegmentedLogStore::new(log_config)?;

    segmented_pod_handle
        .set_status(PodStatus::Ready)
        .map_err(map_storage_pod_error)?;
    state_pod_handle
        .set_status(PodStatus::Ready)
        .map_err(map_storage_pod_error)?;

    Ok(StorageComponents::new(
        Box::new(state_manager),
        Box::new(log_store),
        Some(storage_runtime),
    ))
}
/// Convenience settings for building segmented storage components.
#[derive(Debug, Clone)]
pub struct SegmentedStorageSettings {
    pub root_dir: PathBuf,
    pub instance_id: u64,
    pub server_id: ServerId,
    pub dispatcher: PartitionDispatcherConfig,
    pub manifest_writer: ManifestLogWriterConfig,
    pub aof: AofConfig,
    pub manager: AofManagerConfig,
    pub append_timeout: Duration,
    pub verify_checksums: bool,
    pub segmented_aof: Option<AofConfig>,
    pub segmented_manager: Option<AofManagerConfig>,
    pub segmented_append_timeout: Option<Duration>,
    pub segmented_verify_checksums: Option<bool>,
    pub segmented_partition_root: Option<PathBuf>,
    pub storage_fleet: Option<StorageFleetConfig>,
    pub storage_pods_root: Option<PathBuf>,
}

impl SegmentedStorageSettings {
    /// Create settings with sensible defaults for segmented storage.
    pub fn new(root_dir: impl Into<PathBuf>, server_id: ServerId, instance_id: u64) -> Self {
        Self {
            root_dir: root_dir.into(),
            instance_id,
            server_id,
            dispatcher: PartitionDispatcherConfig::default(),
            manifest_writer: ManifestLogWriterConfig::default(),
            aof: AofConfig::default(),
            manager: AofManagerConfig::default(),
            append_timeout: Duration::from_secs(5),
            verify_checksums: true,
            segmented_aof: None,
            segmented_manager: None,
            segmented_append_timeout: None,
            segmented_verify_checksums: None,
            segmented_partition_root: None,
            storage_fleet: None,
            storage_pods_root: None,
        }
    }

    pub fn dispatcher(mut self, dispatcher: PartitionDispatcherConfig) -> Self {
        self.dispatcher = dispatcher;
        self
    }

    pub fn manifest_writer(mut self, config: ManifestLogWriterConfig) -> Self {
        self.manifest_writer = config;
        self
    }

    pub fn aof_config(mut self, config: AofConfig) -> Self {
        self.aof = config;
        self
    }

    pub fn manager_config(mut self, config: AofManagerConfig) -> Self {
        self.manager = config;
        self
    }

    pub fn append_timeout(mut self, timeout: Duration) -> Self {
        self.append_timeout = timeout;
        self
    }

    pub fn verify_checksums(mut self, verify: bool) -> Self {
        self.verify_checksums = verify;
        self
    }

    pub fn segmented_aof_config(mut self, config: AofConfig) -> Self {
        self.segmented_aof = Some(config);
        self
    }

    pub fn segmented_manager_config(mut self, config: AofManagerConfig) -> Self {
        self.segmented_manager = Some(config);
        self
    }

    pub fn segmented_append_timeout(mut self, timeout: Duration) -> Self {
        self.segmented_append_timeout = Some(timeout);
        self
    }

    pub fn segmented_verify_checksums(mut self, verify: bool) -> Self {
        self.segmented_verify_checksums = Some(verify);
        self
    }

    pub fn storage_fleet_config(mut self, config: StorageFleetConfig) -> Self {
        self.storage_fleet = Some(config);
        self
    }

    pub fn storage_pods_root(mut self, root: PathBuf) -> Self {
        self.storage_pods_root = Some(root);
        self
    }

    /// Build storage components using the configured settings.
    pub fn build(self, start_index: LogIndex) -> RaftResult<StorageComponents> {
        let manifest_store = segmented_manifest_store(
            &self.root_dir,
            self.instance_id,
            self.manifest_writer.clone(),
        )?;

        let state_manager_config =
            AofStateManagerConfig::new(self.root_dir.clone(), self.server_id)
                .with_aof_config(self.aof.clone())
                .with_manager_config(self.manager.clone())
                .append_timeout(self.append_timeout)
                .verify_checksums(self.verify_checksums);

        let mut log_aof_config = if let Some(cfg) = self.segmented_aof.clone() {
            cfg
        } else {
            let mut cfg = self.aof.clone();
            cfg.root_dir = self.root_dir.join("segmented-log");
            cfg
        };
        if log_aof_config.root_dir.as_os_str().is_empty() {
            log_aof_config.root_dir = self.root_dir.join("segmented-log");
        }

        let log_manager_config = self
            .segmented_manager
            .clone()
            .unwrap_or_else(|| self.manager.clone());
        let log_append_timeout = self.segmented_append_timeout.unwrap_or(self.append_timeout);
        let log_verify_checksums = self
            .segmented_verify_checksums
            .unwrap_or(self.verify_checksums);
        let partition_root = self
            .segmented_partition_root
            .clone()
            .unwrap_or_else(|| self.root_dir.join("partitions"));

        let storage_pods_root = self
            .storage_pods_root
            .clone()
            .unwrap_or_else(|| self.root_dir.join("storage-pods"));

        segmented_storage_components(
            start_index,
            self.dispatcher.clone(),
            manifest_store,
            state_manager_config,
            log_aof_config,
            log_manager_config,
            log_append_timeout,
            log_verify_checksums,
            partition_root,
            self.server_id,
            self.instance_id,
            self.storage_fleet.clone(),
            storage_pods_root,
        )
    }
}
