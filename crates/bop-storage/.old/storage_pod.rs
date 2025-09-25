use crate::config::StorageConfig;
use crate::flush_controller::{
    FlushAction, FlushController, FlushControllerSnapshot, FlushError, FlushOutcome,
    FlushScheduleError,
};
use crate::storage_types::{PodStatus, StoragePodHealthSnapshot, StoragePodId, unix_timestamp};
use serde::{Deserialize, Serialize};
use crc64fast_nvme::Digest;
use std::convert::TryInto;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct StoragePodConfig {
    pub id: StoragePodId,
    pub root: PathBuf,
    pub wal_dir: PathBuf,
    pub manifest_dir: PathBuf,
    pub superblock_path: PathBuf,
    pub metadata_path: PathBuf,
    pub restart_log_path: PathBuf,
    pub config: StorageConfig,
}

impl StoragePodConfig {
    pub fn from_root(id: StoragePodId, root: impl AsRef<Path>, config: StorageConfig) -> Self {
        let root = root.as_ref().to_path_buf();
        let wal_dir = root.join("wal");
        let manifest_dir = root.join("manifest");
        let superblock_path = root.join("superblock.bin");
        let metadata_path = root.join("metadata.log");
        let restart_log_path = root.join("restart.log");
        Self {
            id,
            root,
            wal_dir,
            manifest_dir,
            superblock_path,
            metadata_path,
            restart_log_path,
            config,
        }
    }

    pub fn ensure_layout(&self) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(&self.wal_dir)?;
        fs::create_dir_all(&self.manifest_dir)?;
        Ok(())
    }
}

const METADATA_MAGIC: u32 = 0x504F_444D; // 'PODM'
const METADATA_VERSION: u16 = 1;
const METADATA_HEADER_LEN: usize = 4 + 2 + 2 + 4; // magic + version + reserved + payload len
const METADATA_FOOTER_LEN: usize = 8; // checksum
const METADATA_MAX_LOG_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePodMetadata {
    pub pod_id: StoragePodId,
    pub restart_count: u64,
    pub last_restart_at: Option<u64>,
    pub last_flush_success_at: Option<u64>,
    pub last_flush_failure_at: Option<u64>,
    pub last_flush_error: Option<String>,
    pub last_wal_sequence: Option<u64>,
    pub flush_failures: u64,
    pub pending_snapshot_label: Option<String>,
    pub pending_snapshot_at: Option<u64>,
    #[serde(default)]
    pub last_snapshot_label: Option<String>,
    #[serde(default)]
    pub last_snapshot_at: Option<u64>,
    #[serde(default)]
    pub snapshot_failures: u64,
    #[serde(default)]
    pub last_snapshot_error: Option<String>,
    #[serde(default)]
    pub last_snapshot_failure_at: Option<u64>,
}

impl StoragePodMetadata {
    pub fn new(pod_id: StoragePodId) -> Self {
        Self {
            pod_id,
            restart_count: 0,
            last_restart_at: None,
            last_flush_success_at: None,
            last_flush_failure_at: None,
            last_flush_error: None,
            last_wal_sequence: None,
            flush_failures: 0,
            pending_snapshot_label: None,
            pending_snapshot_at: None,
            last_snapshot_label: None,
            last_snapshot_at: None,
            snapshot_failures: 0,
            last_snapshot_error: None,
            last_snapshot_failure_at: None,
        }
    }

    pub fn load_or_default(
        path: &Path,
        pod_id: StoragePodId,
    ) -> Result<Self, StoragePodMetadataError> {
        if !path.exists() {
            return Ok(Self::new(pod_id));
        }
        let mut file = fs::File::open(path).map_err(StoragePodMetadataError::Io)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(StoragePodMetadataError::Io)?;
        let mut cursor = buf.as_slice();
        let mut last_good = None;
        while cursor.len() >= METADATA_HEADER_LEN + METADATA_FOOTER_LEN {
            match MetadataRecord::decode(cursor) {
                Ok((record, rest)) => {
                    last_good = Some(record.metadata);
                    cursor = rest;
                }
                Err(MetadataDecodeError::Truncated)
                | Err(MetadataDecodeError::Corrupted)
                | Err(MetadataDecodeError::UnsupportedVersion)
                | Err(MetadataDecodeError::InvalidPayload) => {
                    break;
                }
            }
        }
        Ok(last_good.unwrap_or_else(|| Self::new(pod_id)))
    }

    pub fn persist(&self, path: &Path) -> Result<(), StoragePodMetadataError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(StoragePodMetadataError::Io)?;
        }
        let record = MetadataRecord::from_metadata(self.clone())?;
        let encoded = record.encode()?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(StoragePodMetadataError::Io)?;
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(StoragePodMetadataError::Io)?;
        if file
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0)
            > METADATA_MAX_LOG_BYTES
        {
            drop(file);
            Self::rewrite_log(path, &encoded)?;
        }
        Ok(())
    }

    fn rewrite_log(path: &Path, encoded: &[u8]) -> Result<(), StoragePodMetadataError> {
        let tmp_path = path.with_extension("tmp");
        {
            let mut tmp = fs::File::create(&tmp_path).map_err(StoragePodMetadataError::Io)?;
            tmp.write_all(encoded)
                .and_then(|_| tmp.sync_all())
                .map_err(StoragePodMetadataError::Io)?;
        }
        match fs::rename(&tmp_path, path) {
            Ok(_) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(path).map_err(StoragePodMetadataError::Io)?;
                fs::rename(&tmp_path, path).map_err(StoragePodMetadataError::Io)
            }
            Err(err) => {
                let _ = fs::remove_file(&tmp_path);
                Err(StoragePodMetadataError::Io(err))
            }
        }
    }

    pub fn record_restart(&mut self, timestamp: SystemTime) {
        self.restart_count = self.restart_count.saturating_add(1);
        self.last_restart_at = Some(unix_timestamp(timestamp));
    }

    pub fn record_flush_success(&mut self, timestamp: SystemTime, wal_sequence: Option<u64>) {
        self.last_flush_success_at = Some(unix_timestamp(timestamp));
        self.last_flush_failure_at = None;
        self.last_flush_error = None;
        if let Some(seq) = wal_sequence {
            self.last_wal_sequence = Some(seq);
        }
    }

    pub fn record_flush_failure(&mut self, timestamp: SystemTime, error: impl Into<String>) {
        self.last_flush_failure_at = Some(unix_timestamp(timestamp));
        self.last_flush_error = Some(error.into());
        self.flush_failures = self.flush_failures.saturating_add(1);
    }

    pub fn record_snapshot_request(&mut self, label: Option<String>, timestamp: SystemTime) {
        self.pending_snapshot_label = label;
        self.pending_snapshot_at = Some(unix_timestamp(timestamp));
    }

    pub fn complete_snapshot(&mut self, label: Option<String>, timestamp: SystemTime) {
        self.pending_snapshot_label = None;
        self.pending_snapshot_at = None;
        self.last_snapshot_label = label;
        self.last_snapshot_at = Some(unix_timestamp(timestamp));
        self.last_snapshot_error = None;
        self.last_snapshot_failure_at = None;
    }

    pub fn fail_snapshot(&mut self, error: impl Into<String>, timestamp: SystemTime) {
        self.pending_snapshot_label = None;
        self.pending_snapshot_at = None;
        self.last_snapshot_error = Some(error.into());
        self.last_snapshot_failure_at = Some(unix_timestamp(timestamp));
        self.snapshot_failures = self.snapshot_failures.saturating_add(1);
    }
}

#[derive(Debug, Clone)]
pub struct PodFlushState {
    pub last_success_sequence: Option<u64>,
    pub last_success_at: Option<SystemTime>,
    pub last_failure: Option<PodFlushFailure>,
}

impl PodFlushState {
    fn record_success(&mut self, sequence: u64, at: SystemTime) {
        match self.last_success_sequence {
            Some(existing) if existing >= sequence => {}
            _ => {
                self.last_success_sequence = Some(sequence);
            }
        }
        self.last_success_at = Some(at);
        self.last_failure = None;
    }

    fn record_failure(&mut self, sequence: Option<u64>, message: String, at: SystemTime) {
        self.last_failure = Some(PodFlushFailure {
            sequence,
            message,
            at,
        });
    }
}

impl Default for PodFlushState {
    fn default() -> Self {
        Self {
            last_success_sequence: None,
            last_success_at: None,
            last_failure: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PodFlushFailure {
    pub sequence: Option<u64>,
    pub message: String,
    pub at: SystemTime,
}

#[derive(Debug, Error)]
pub enum StoragePodMetadataError {
    #[error("metadata IO error: {0}")]
    Io(#[from] io::Error),
    #[error("metadata decode error: {0}")]
    Deserialize(serde_json::Error),
    #[error("metadata encode error: {0}")]
    Serialize(serde_json::Error),
}

#[derive(Debug, Error)]
pub enum StoragePodError {
    #[error("metadata error: {0}")]
    Metadata(#[from] StoragePodMetadataError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("state lock poisoned")]
    Poisoned,
    #[error("flush scheduling failed: {0}")]
    FlushSchedule(#[from] FlushScheduleError),
    #[error("flush operation failed: {0}")]
    FlushFailure(String),
    #[error("flush worker panicked: {0}")]
    FlushWorker(String),
}

#[derive(Debug, Clone)]
pub struct PodSnapshotIntent {
    pub pod_id: StoragePodId,
    pub manifest_dir: PathBuf,
    pub superblock_path: PathBuf,
    pub created_at: SystemTime,
    pub wal_sequence_hint: Option<u64>,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SnapshotPublish {
    Success { wal_sequence: Option<u64> },
    Failure { error: String },
}

impl SnapshotPublish {
    pub fn success(wal_sequence: Option<u64>) -> Self {
        Self::Success { wal_sequence }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self::Failure {
            error: error.into(),
        }
    }
}

#[derive(Clone)]
pub struct StoragePodHandle {
    inner: Arc<StoragePodInner>,
}

impl StoragePodHandle {
    pub fn id(&self) -> StoragePodId {
        self.inner.id.clone()
    }

    pub fn config(&self) -> Arc<StoragePodConfig> {
        Arc::clone(&self.inner.config)
    }

    pub fn metadata(&self) -> Result<StoragePodMetadata, StoragePodError> {
        self.inner
            .metadata
            .read()
            .map(|m| m.clone())
            .map_err(|_| StoragePodError::Poisoned)
    }

    pub fn health_snapshot(&self) -> Result<StoragePodHealthSnapshot, StoragePodError> {
        self.inner
            .health
            .read()
            .map(|h| h.clone())
            .map_err(|_| StoragePodError::Poisoned)
    }

    pub fn watch_health(&self) -> watch::Receiver<StoragePodHealthSnapshot> {
        self.inner.health_tx.subscribe()
    }

    pub fn flush_state(&self) -> Result<PodFlushState, StoragePodError> {
        self.inner
            .flush_state
            .read()
            .map(|state| state.clone())
            .map_err(|_| StoragePodError::Poisoned)
    }

    pub fn watch_flush_state(&self) -> watch::Receiver<PodFlushState> {
        self.inner.flush_state_tx.subscribe()
    }

    pub async fn wait_for_flush(&self, wal_sequence: u64) -> Result<(), StoragePodError> {
        let mut rx = self.inner.flush_state_tx.subscribe();
        loop {
            let decision = {
                let snapshot = rx.borrow().clone();
                if snapshot
                    .last_success_sequence
                    .map(|seq| seq >= wal_sequence)
                    .unwrap_or(false)
                {
                    Some(Ok(()))
                } else if let Some(failure) = snapshot.last_failure {
                    let applies = failure
                        .sequence
                        .map(|seq| seq >= wal_sequence)
                        .unwrap_or(true);
                    if applies {
                        Some(Err(StoragePodError::FlushFailure(failure.message)))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(result) = decision {
                return result;
            }
            rx.changed().await.map_err(|_| {
                StoragePodError::FlushSchedule(FlushScheduleError::ControllerClosed)
            })?;
        }
    }

    pub async fn enqueue_flush(
        &self,
        actions: Vec<FlushAction>,
        wal_sequence: Option<u64>,
    ) -> Result<FlushOutcome, StoragePodError> {
        let ticket = self.inner.flush.submit(self.id(), actions).await?;
        let outcome = ticket
            .wait()
            .await
            .map_err(StoragePodError::from_flush_error)?;
        self.inner.apply_flush_outcome(&outcome, wal_sequence)?;
        if let Some(failure) = outcome.failure() {
            let reason = failure
                .result
                .as_ref()
                .err()
                .map(|err| err.message().to_string())
                .unwrap_or_else(|| "flush failure".to_string());
            return Err(StoragePodError::FlushFailure(format!(
                "{:?} stage failed: {reason}",
                failure.stage
            )));
        }
        Ok(outcome)
    }

    pub fn prepare_snapshot(
        &self,
        label: Option<String>,
    ) -> Result<PodSnapshotIntent, StoragePodError> {
        let mut metadata = self
            .inner
            .metadata
            .write()
            .map_err(|_| StoragePodError::Poisoned)?;
        let now = SystemTime::now();
        metadata.record_snapshot_request(label.clone(), now);
        metadata.persist(self.inner.config.metadata_path.as_path())?;
        drop(metadata);
        let mut health = self
            .inner
            .health
            .write()
            .map_err(|_| StoragePodError::Poisoned)?;
        health.set_status(PodStatus::Recovering);
        let snapshot = health.clone();
        drop(health);
        let _ = self.inner.health_tx.send(snapshot);
        Ok(PodSnapshotIntent {
            pod_id: self.id(),
            manifest_dir: self.inner.config.manifest_dir.clone(),
            superblock_path: self.inner.config.superblock_path.clone(),
            created_at: now,
            wal_sequence_hint: self
                .inner
                .metadata
                .read()
                .ok()
                .and_then(|metadata| metadata.last_wal_sequence),
            label,
        })
    }

    pub fn publish_snapshot(&self, publish: SnapshotPublish) -> Result<(), StoragePodError> {
        let mut metadata = self
            .inner
            .metadata
            .write()
            .map_err(|_| StoragePodError::Poisoned)?;
        let mut health = self
            .inner
            .health
            .write()
            .map_err(|_| StoragePodError::Poisoned)?;
        let now = SystemTime::now();
        let pending_label = metadata.pending_snapshot_label.clone();
        let flush_sequence = match &publish {
            SnapshotPublish::Success { wal_sequence } => {
                metadata.complete_snapshot(pending_label, now);
                if let Some(seq) = wal_sequence {
                    metadata.last_wal_sequence = Some(*seq);
                }
                health.record_success(now);
                *wal_sequence
            }
            SnapshotPublish::Failure { error } => {
                metadata.fail_snapshot(error.clone(), now);
                health.record_failure(PodStatus::Degraded, error.clone());
                None
            }
        };
        metadata.persist(self.inner.config.metadata_path.as_path())?;
        let snapshot = health.clone();
        drop(health);
        drop(metadata);
        let _ = self.inner.health_tx.send(snapshot);
        if let Some(sequence) = flush_sequence {
            self.inner.update_flush_success(Some(sequence), now)?;
        } else if matches!(publish, SnapshotPublish::Success { wal_sequence: None }) {
            self.inner.update_flush_success(None, now)?;
        }
        Ok(())
    }

    pub fn set_status(&self, status: PodStatus) -> Result<(), StoragePodError> {
        let mut health = self
            .inner
            .health
            .write()
            .map_err(|_| StoragePodError::Poisoned)?;
        health.set_status(status);
        let snapshot = health.clone();
        drop(health);
        let _ = self.inner.health_tx.send(snapshot);
        Ok(())
    }

    pub fn flush_metrics(&self) -> FlushControllerSnapshot {
        self.inner.flush.snapshot()
    }

    pub fn runtime_handle(&self) -> Handle {
        self.inner.runtime.clone()
    }
}

impl StoragePodHandle {
    fn new(
        config: StoragePodConfig,
        runtime: Handle,
        flush: FlushController,
    ) -> Result<Self, StoragePodError> {
        config.ensure_layout()?;
        let metadata =
            StoragePodMetadata::load_or_default(&config.metadata_path, config.id.clone())?;
        let mut metadata = metadata;
        metadata.record_restart(SystemTime::now());
        metadata.persist(&config.metadata_path)?;
        let health = StoragePodHealthSnapshot::new(PodStatus::Initializing);
        let (health_tx, _health_rx) = watch::channel(health.clone());
        let flush_state = PodFlushState::default();
        let (flush_state_tx, _flush_state_rx) = watch::channel(flush_state.clone());
        let inner = Arc::new(StoragePodInner {
            id: config.id.clone(),
            config: Arc::new(config),
            metadata: RwLock::new(metadata),
            health: RwLock::new(health),
            health_tx,
            flush_state: RwLock::new(flush_state),
            flush_state_tx,
            runtime,
            flush,
        });
        Ok(Self { inner })
    }
}

struct StoragePodInner {
    id: StoragePodId,
    config: Arc<StoragePodConfig>,
    metadata: RwLock<StoragePodMetadata>,
    health: RwLock<StoragePodHealthSnapshot>,
    health_tx: watch::Sender<StoragePodHealthSnapshot>,
    flush_state: RwLock<PodFlushState>,
    flush_state_tx: watch::Sender<PodFlushState>,
    runtime: Handle,
    flush: FlushController,
}

impl StoragePodInner {
    fn apply_flush_outcome(
        &self,
        outcome: &FlushOutcome,
        wal_sequence: Option<u64>,
    ) -> Result<(), StoragePodError> {
        let mut metadata = self
            .metadata
            .write()
            .map_err(|_| StoragePodError::Poisoned)?;
        let mut health = self.health.write().map_err(|_| StoragePodError::Poisoned)?;
        let now = SystemTime::now();
        let mut failure_message: Option<String> = None;
        if outcome.is_success() {
            metadata.record_flush_success(now, wal_sequence);
            health.record_success(now);
        } else if let Some(failure) = outcome.failure() {
            let message = failure
                .result
                .as_ref()
                .err()
                .map(|err| err.message().to_string())
                .unwrap_or_else(|| "flush failure".to_string());
            metadata.record_flush_failure(now, message.clone());
            health.record_failure(PodStatus::Degraded, message.clone());
            failure_message = Some(message);
        }
        metadata.persist(self.config.metadata_path.as_path())?;
        let snapshot = health.clone();
        drop(health);
        drop(metadata);
        let _ = self.health_tx.send(snapshot);
        if let Some(message) = failure_message {
            self.update_flush_failure(wal_sequence, message, now)?;
        } else if outcome.is_success() {
            self.update_flush_success(wal_sequence, now)?;
        }
        Ok(())
    }

    fn update_flush_success(
        &self,
        wal_sequence: Option<u64>,
        timestamp: SystemTime,
    ) -> Result<(), StoragePodError> {
        let mut state = self
            .flush_state
            .write()
            .map_err(|_| StoragePodError::Poisoned)?;
        if let Some(sequence) = wal_sequence {
            state.record_success(sequence, timestamp);
        } else {
            state.last_success_at = Some(timestamp);
            state.last_failure = None;
        }
        let snapshot = state.clone();
        drop(state);
        let _ = self.flush_state_tx.send(snapshot);
        Ok(())
    }

    fn update_flush_failure(
        &self,
        wal_sequence: Option<u64>,
        message: String,
        timestamp: SystemTime,
    ) -> Result<(), StoragePodError> {
        let mut state = self
            .flush_state
            .write()
            .map_err(|_| StoragePodError::Poisoned)?;
        state.record_failure(wal_sequence, message.clone(), timestamp);
        let snapshot = state.clone();
        drop(state);
        let _ = self.flush_state_tx.send(snapshot);
        Ok(())
    }
}

pub fn register_pod(
    config: StoragePodConfig,
    runtime: Handle,
    flush: FlushController,
) -> Result<StoragePodHandle, StoragePodError> {
    StoragePodHandle::new(config, runtime, flush)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::flush_controller::{FlushActionError, FlushControllerConfig};
    use std::time::Duration;
    use tempfile::tempdir;

    fn setup_controller(max_parallel: usize) -> (tokio::runtime::Runtime, FlushController) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("runtime");
        let controller = FlushController::new(
            runtime.handle().clone(),
            FlushControllerConfig {
                queue_depth: 8,
                max_parallel,
            },
        );
        (runtime, controller)
    }

    #[test]
    fn pod_waiters_receive_success() {
        let (runtime, controller) = setup_controller(2);
        let tmp = tempdir().expect("tempdir");
        let config = StoragePodConfig::from_root(
            StoragePodId::from("pod-success"),
            tmp.path(),
            StorageConfig::default(),
        );
        let pod = register_pod(config, runtime.handle().clone(), controller.clone()).expect("pod");
        runtime.block_on(async {
            let waiter = tokio::spawn({
                let pod = pod.clone();
                async move { pod.wait_for_flush(5).await }
            });
            let outcome = pod
                .enqueue_flush(vec![FlushAction::wal_fsync("fsync-ok", || Ok(()))], Some(5))
                .await
                .expect("flush");
            assert!(outcome.is_success());
            assert!(waiter.await.expect("waiter join").is_ok());
        });
        let flush_state = pod.flush_state().expect("flush state");
        assert_eq!(flush_state.last_success_sequence, Some(5));
        assert!(flush_state.last_failure.is_none());
        let metadata = pod.metadata().expect("metadata");
        assert_eq!(metadata.last_wal_sequence, Some(5));
        let health = pod.health_snapshot().expect("health");
        assert_eq!(health.status, PodStatus::Ready);
        drop(pod);
        assert!(controller.shutdown(Duration::from_secs(1)));
        runtime.shutdown_timeout(Duration::from_secs(1));
    }

    #[test]
    fn pod_waiters_fail_on_flush_error() {
        let (runtime, controller) = setup_controller(1);
        let tmp = tempdir().expect("tempdir");
        let config = StoragePodConfig::from_root(
            StoragePodId::from("pod-fail"),
            tmp.path(),
            StorageConfig::default(),
        );
        let pod = register_pod(config, runtime.handle().clone(), controller.clone()).expect("pod");
        runtime.block_on(async {
            let waiter = tokio::spawn({
                let pod = pod.clone();
                async move { pod.wait_for_flush(9).await }
            });
            let err = pod
                .enqueue_flush(
                    vec![FlushAction::wal_fsync("fsync-fail", || {
                        Err(FlushActionError::new("disk"))
                    })],
                    Some(9),
                )
                .await
                .expect_err("flush should fail");
            assert!(matches!(&err, StoragePodError::FlushFailure(msg) if msg.contains("disk")));
            let waiter_err = waiter
                .await
                .expect("waiter join")
                .expect_err("waiter should fail");
            assert!(matches!(waiter_err, StoragePodError::FlushFailure(_)));
        });
        let flush_state = pod.flush_state().expect("flush state");
        assert!(flush_state.last_failure.is_some());
        assert_eq!(flush_state.last_failure.as_ref().unwrap().message, "disk");
        let metadata = pod.metadata().expect("metadata");
        assert_eq!(metadata.flush_failures, 1);
        assert_eq!(metadata.last_flush_error.as_deref(), Some("disk"));
        let health = pod.health_snapshot().expect("health");
        assert_eq!(health.status, PodStatus::Degraded);
        drop(pod);
        assert!(controller.shutdown(Duration::from_secs(1)));
        runtime.shutdown_timeout(Duration::from_secs(1));
    }

    #[test]
    fn pod_snapshot_lifecycle_updates_metadata() {
        let (runtime, controller) = setup_controller(2);
        let tmp = tempdir().expect("tempdir");
        let config = StoragePodConfig::from_root(
            StoragePodId::from("pod-snapshot"),
            tmp.path(),
            StorageConfig::default(),
        );
        let pod = register_pod(config, runtime.handle().clone(), controller.clone()).expect("pod");
        pod.prepare_snapshot(Some("snap-1".to_string()))
            .expect("prepare");
        runtime.block_on(async {
            pod.enqueue_flush(vec![FlushAction::wal_fsync("fsync", || Ok(()))], Some(42))
                .await
                .expect("flush");
        });
        pod.publish_snapshot(SnapshotPublish::success(Some(42)))
            .expect("publish");
        let metadata = pod.metadata().expect("metadata");
        assert_eq!(metadata.last_snapshot_label.as_deref(), Some("snap-1"));
        assert!(metadata.last_snapshot_at.is_some());
        assert_eq!(metadata.pending_snapshot_label, None);
        assert_eq!(metadata.last_wal_sequence, Some(42));
        let health = pod.health_snapshot().expect("health");
        assert_eq!(health.status, PodStatus::Ready);
        let flush_state = pod.flush_state().expect("flush state");
        assert_eq!(flush_state.last_success_sequence, Some(42));
        drop(pod);
        assert!(controller.shutdown(Duration::from_secs(1)));
        runtime.shutdown_timeout(Duration::from_secs(1));
    }

    #[test]
    fn pod_snapshot_failure_records_error() {
        let (runtime, controller) = setup_controller(1);
        let tmp = tempdir().expect("tempdir");
        let config = StoragePodConfig::from_root(
            StoragePodId::from("pod-snapshot-fail"),
            tmp.path(),
            StorageConfig::default(),
        );
        let pod = register_pod(config, runtime.handle().clone(), controller.clone()).expect("pod");
        pod.prepare_snapshot(None).expect("prepare");
        pod.publish_snapshot(SnapshotPublish::failure("snap failed"))
            .expect("publish");
        let metadata = pod.metadata().expect("metadata");
        assert_eq!(metadata.pending_snapshot_label, None);
        assert_eq!(metadata.last_snapshot_error.as_deref(), Some("snap failed"));
        assert_eq!(metadata.snapshot_failures, 1);
        let health = pod.health_snapshot().expect("health");
        assert_eq!(health.status, PodStatus::Degraded);
        let flush_state = pod.flush_state().expect("flush state");
        assert!(flush_state.last_failure.is_none());
        drop(pod);
        assert!(controller.shutdown(Duration::from_secs(1)));
        runtime.shutdown_timeout(Duration::from_secs(1));
    }

    #[test]
    fn metadata_persists_across_restart() {
        let tmp = tempdir().expect("tempdir");
        let base_config = StoragePodConfig::from_root(
            StoragePodId::from("pod-restart"),
            tmp.path(),
            StorageConfig::default(),
        );

        let (runtime1, controller1) = setup_controller(2);
        let pod1 = register_pod(
            base_config.clone(),
            runtime1.handle().clone(),
            controller1.clone(),
        )
        .expect("pod1");
        runtime1.block_on(async {
            pod1.enqueue_flush(vec![FlushAction::wal_fsync("fsync", || Ok(()))], Some(11))
                .await
                .expect("flush");
        });
        let metadata1 = pod1.metadata().expect("metadata1");
        assert_eq!(metadata1.restart_count, 1);
        drop(pod1);
        assert!(controller1.shutdown(Duration::from_secs(1)));
        runtime1.shutdown_timeout(Duration::from_secs(1));

        let (runtime2, controller2) = setup_controller(2);
        let pod2 = register_pod(
            base_config.clone(),
            runtime2.handle().clone(),
            controller2.clone(),
        )
        .expect("pod2");
        let metadata2 = pod2.metadata().expect("metadata2");
        assert_eq!(metadata2.restart_count, 2);
        assert_eq!(metadata2.last_wal_sequence, Some(11));
        assert!(metadata2.last_restart_at.is_some());
        drop(pod2);
        assert!(controller2.shutdown(Duration::from_secs(1)));
        runtime2.shutdown_timeout(Duration::from_secs(1));
    }
}

impl StoragePodError {
    pub fn from_flush_error(error: FlushError) -> Self {
        match error {
            FlushError::ControllerShutdown => {
                StoragePodError::FlushSchedule(FlushScheduleError::ControllerClosed)
            }
            FlushError::WorkerPanic(reason) => StoragePodError::FlushWorker(reason),
        }
    }
}
