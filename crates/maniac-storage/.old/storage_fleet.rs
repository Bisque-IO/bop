use crate::flush_controller::{FlushController, FlushControllerConfig, FlushControllerSnapshot};
use crate::storage_pod::{StoragePodConfig, StoragePodError, StoragePodHandle, register_pod};
use crate::storage_types::{PodStatus, StoragePodHealthSnapshot, StoragePodId};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;
use thiserror::Error;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct StorageFleetConfig {
    pub worker_threads: usize,
    pub max_blocking_threads: usize,
    pub flush: FlushControllerConfig,
    pub shutdown_timeout: Duration,
}

impl Default for StorageFleetConfig {
    fn default() -> Self {
        Self {
            worker_threads: 4,
            max_blocking_threads: 64,
            flush: FlushControllerConfig::default(),
            shutdown_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Error)]
pub enum StorageFleetError {
    #[error("failed to initialize runtime: {0}")]
    RuntimeInit(std::io::Error),
    #[error("storage pod {0} already registered")]
    PodAlreadyExists(StoragePodId),
    #[error("storage pod error: {0}")]
    Pod(StoragePodError),
}

pub struct StorageFleet {
    runtime: Option<Runtime>,
    flush_controller: FlushController,
    pods: RwLock<HashMap<StoragePodId, StoragePodHandle>>,
    shutdown_tx: watch::Sender<bool>,
    config: StorageFleetConfig,
}

impl StorageFleet {
    pub fn new(config: StorageFleetConfig) -> Result<Self, StorageFleetError> {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .worker_threads(config.worker_threads)
            .max_blocking_threads(config.max_blocking_threads)
            .build()
            .map_err(StorageFleetError::RuntimeInit)?;
        let handle = runtime.handle().clone();
        let flush_controller = FlushController::new(handle.clone(), config.flush.clone());
        let (shutdown_tx, _shutdown_rx) = watch::channel(false);
        Ok(Self {
            runtime: Some(runtime),
            flush_controller,
            pods: RwLock::new(HashMap::new()),
            shutdown_tx,
            config,
        })
    }

    pub fn runtime_handle(&self) -> Handle {
        self.runtime
            .as_ref()
            .expect("fleet runtime available")
            .handle()
            .clone()
    }

    pub fn flush_controller(&self) -> FlushController {
        self.flush_controller.clone()
    }

    pub fn register_pod(
        &self,
        config: StoragePodConfig,
    ) -> Result<StoragePodHandle, StorageFleetError> {
        let id = config.id.clone();
        let mut pods = self.pods.write().unwrap();
        if pods.contains_key(&id) {
            return Err(StorageFleetError::PodAlreadyExists(id));
        }
        let handle = register_pod(config, self.runtime_handle(), self.flush_controller.clone())
            .map_err(StorageFleetError::Pod)?;
        let _ = handle.set_status(PodStatus::Recovering);
        pods.insert(id.clone(), handle.clone());
        Ok(handle)
    }

    pub fn unregister_pod(&self, id: &StoragePodId) -> Option<StoragePodHandle> {
        let mut pods = self.pods.write().unwrap();
        let handle = pods.remove(id);
        if let Some(handle) = &handle {
            let _ = handle.set_status(PodStatus::Stopped);
        }
        handle
    }

    pub fn get_pod(&self, id: &StoragePodId) -> Option<StoragePodHandle> {
        self.pods.read().unwrap().get(id).cloned()
    }

    pub fn pods(&self) -> Vec<StoragePodHandle> {
        self.pods.read().unwrap().values().cloned().collect()
    }

    pub fn fleet_health(&self) -> Vec<(StoragePodId, StoragePodHealthSnapshot)> {
        self.pods
            .read()
            .unwrap()
            .iter()
            .filter_map(|(id, pod)| {
                pod.health_snapshot()
                    .ok()
                    .map(|health| (id.clone(), health))
            })
            .collect()
    }

    pub fn flush_metrics(&self) -> FlushControllerSnapshot {
        self.flush_controller.snapshot()
    }

    pub fn subscribe_flush_metrics(&self) -> watch::Receiver<FlushControllerSnapshot> {
        self.flush_controller.subscribe()
    }

    pub fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    pub fn initiate_shutdown(&self) {
        self.flush_controller.begin_shutdown();
        let pods: Vec<StoragePodHandle> = {
            let guard = self.pods.read().unwrap();
            guard.values().cloned().collect()
        };
        for pod in pods {
            let _ = pod.set_status(PodStatus::ShuttingDown);
        }
        let _ = self.shutdown_tx.send(true);
    }
}

impl Drop for StorageFleet {
    fn drop(&mut self) {
        self.initiate_shutdown();
        if let Ok(mut pods) = self.pods.write() {
            pods.clear();
        }
        if let Some(runtime) = self.runtime.take() {
            let timeout = self.config.shutdown_timeout;
            let _ = self.flush_controller.wait_for_idle(timeout);
            runtime.shutdown_timeout(timeout);
        }
    }
}
