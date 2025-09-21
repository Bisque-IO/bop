use std::collections::{HashMap, VecDeque};
use std::convert::TryInto;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crc64fast_nvme::Digest;
use futures::future::join_all;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tokio::runtime::Handle;
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::{JoinHandle, spawn_blocking};
use tokio::time::sleep as tokio_sleep;
use tracing::{debug, trace, warn};
use zstd::stream::{read::Decoder as ZstdDecoder, write::Encoder as ZstdEncoder};

use crate::aof2::config::SegmentId;
use crate::aof2::error::{AofError, AofResult};
use crate::aof2::fs::Layout;
use crate::aof2::segment::{
    RECORD_HEADER_SIZE, SEGMENT_FOOTER_SIZE, SEGMENT_HEADER_SIZE, Segment, SegmentFooter,
};

use super::tier0::{ActivationGrant, InstanceId, Tier0DropEvent};
use super::tier2::{
    Tier2DeleteComplete, Tier2DeleteRequest, Tier2Event, Tier2FetchRequest, Tier2Handle,
    Tier2Metadata, Tier2UploadComplete, Tier2UploadDescriptor,
};

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy)]
pub struct Tier1Config {
    pub max_bytes: u64,
    pub worker_threads: usize,
    pub compression_level: i32,
    pub max_retries: u32,
    pub retry_backoff: Duration,
}

impl Tier1Config {
    pub fn new(max_bytes: u64) -> Self {
        Self {
            max_bytes,
            worker_threads: 2,
            compression_level: 3,
            max_retries: 3,
            retry_backoff: Duration::from_millis(50),
        }
    }

    pub fn with_worker_threads(mut self, workers: usize) -> Self {
        self.worker_threads = workers.max(1);
        self
    }

    pub fn with_compression_level(mut self, level: i32) -> Self {
        self.compression_level = level;
        self
    }

    pub fn with_retry_policy(mut self, max_retries: u32, backoff: Duration) -> Self {
        self.max_retries = max_retries.max(1);
        self.retry_backoff = backoff;
        self
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Tier1MetricsSnapshot {
    pub stored_bytes: u64,
    pub compression_jobs: u64,
    pub compression_failures: u64,
    pub hydration_failures: u64,
}

#[derive(Debug, Default)]
pub struct Tier1Metrics {
    stored_bytes: AtomicU64,
    compression_jobs: AtomicU64,
    compression_failures: AtomicU64,
    hydration_failures: AtomicU64,
}

impl Tier1Metrics {
    fn snapshot(&self) -> Tier1MetricsSnapshot {
        Tier1MetricsSnapshot {
            stored_bytes: self.stored_bytes.load(AtomicOrdering::Relaxed),
            compression_jobs: self.compression_jobs.load(AtomicOrdering::Relaxed),
            compression_failures: self.compression_failures.load(AtomicOrdering::Relaxed),
            hydration_failures: self.hydration_failures.load(AtomicOrdering::Relaxed),
        }
    }

    fn set_stored_bytes(&self, bytes: u64) {
        self.stored_bytes.store(bytes, AtomicOrdering::Relaxed);
    }

    fn incr_compression_jobs(&self) {
        self.compression_jobs.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_compression_failures(&self) {
        self.compression_failures
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn incr_hydration_failures(&self) {
        self.hydration_failures
            .fetch_add(1, AtomicOrdering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier1ResidencyState {
    ResidentInTier0,
    StagedForTier0,
    UploadedToTier2,
}

impl Default for Tier1ResidencyState {
    fn default() -> Self {
        Tier1ResidencyState::StagedForTier0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub segment_id: SegmentId,
    pub base_offset: u64,
    pub base_record_count: u64,
    pub created_at: i64,
    pub sealed_at: i64,
    pub checksum: u32,
    pub original_bytes: u64,
    pub compressed_bytes: u64,
    pub compressed_path: String,
    #[serde(default)]
    pub dictionary: Option<String>,
    #[serde(default)]
    pub offset_index: Vec<u32>,
    #[serde(default)]
    pub residency: Tier1ResidencyState,
    #[serde(default)]
    pub tier2: Option<Tier2Metadata>,
    #[serde(default)]
    pub last_access_epoch_ms: u64,
}

impl ManifestEntry {
    fn touch(&mut self) {
        self.last_access_epoch_ms = current_epoch_ms();
    }
}

#[derive(Debug)]
struct Tier1Manifest {
    path: PathBuf,
    entries: HashMap<SegmentId, ManifestEntry>,
}

impl Tier1Manifest {
    fn load(path: PathBuf) -> AofResult<Self> {
        if path.exists() {
            let data = fs::read(&path).map_err(AofError::from)?;
            if data.is_empty() {
                return Ok(Self {
                    path,
                    entries: HashMap::new(),
                });
            }
            let entries: Vec<ManifestEntry> = serde_json::from_slice(&data)
                .map_err(|err| AofError::Serialization(err.to_string()))?;
            let map = entries
                .into_iter()
                .map(|entry| (entry.segment_id, entry))
                .collect();
            Ok(Self { path, entries: map })
        } else {
            Ok(Self {
                path,
                entries: HashMap::new(),
            })
        }
    }

    fn save(&self) -> AofResult<()> {
        let temp =
            NamedTempFile::new_in(self.path.parent().ok_or_else(|| {
                AofError::Io(io::Error::new(io::ErrorKind::Other, "missing parent"))
            })?)
            .map_err(AofError::from)?;
        let entries: Vec<&ManifestEntry> = self.entries.values().collect();
        serde_json::to_writer_pretty(temp.as_file(), &entries)
            .map_err(|err| AofError::Serialization(err.to_string()))?;
        temp.as_file().sync_all().map_err(AofError::from)?;
        temp.persist(&self.path)
            .map_err(|err| AofError::from(err.error))?;
        Ok(())
    }

    fn get(&self, segment_id: SegmentId) -> Option<&ManifestEntry> {
        self.entries.get(&segment_id)
    }

    fn get_mut(&mut self, segment_id: SegmentId) -> Option<&mut ManifestEntry> {
        self.entries.get_mut(&segment_id)
    }

    fn insert(&mut self, entry: ManifestEntry) -> Option<ManifestEntry> {
        self.entries.insert(entry.segment_id, entry)
    }

    fn remove(&mut self, segment_id: SegmentId) -> Option<ManifestEntry> {
        self.entries.remove(&segment_id)
    }

    fn all(&self) -> impl Iterator<Item = &ManifestEntry> {
        self.entries.values()
    }
}

#[derive(Debug)]
struct Tier1InstanceState {
    layout: Layout,
    manifest: Tier1Manifest,
    used_bytes: u64,
    lru: VecDeque<SegmentId>,
}

impl Tier1InstanceState {
    fn new(layout: Layout) -> AofResult<Self> {
        let manifest_path = layout.warm_dir().join("manifest.json");
        let manifest = Tier1Manifest::load(manifest_path)?;
        let used_bytes = manifest.all().map(|entry| entry.compressed_bytes).sum();
        let mut lru = VecDeque::new();
        let mut entries: Vec<_> = manifest.all().collect();
        entries.sort_by_key(|entry| entry.last_access_epoch_ms);
        for entry in entries {
            lru.push_back(entry.segment_id);
        }
        Ok(Self {
            layout,
            manifest,
            used_bytes,
            lru,
        })
    }

    fn touch(&mut self, segment_id: SegmentId) {
        self.lru.retain(|id| *id != segment_id);
        self.lru.push_back(segment_id);
        if let Some(entry) = self.manifest.get_mut(segment_id) {
            entry.touch();
        }
    }

    fn insert_entry(&mut self, entry: ManifestEntry, budget: u64) -> AofResult<Vec<ManifestEntry>> {
        let mut evicted = Vec::new();
        if let Some(existing) = self.manifest.insert(entry.clone()) {
            self.used_bytes = self.used_bytes.saturating_sub(existing.compressed_bytes);
        }
        self.used_bytes = self.used_bytes.saturating_add(entry.compressed_bytes);
        self.touch(entry.segment_id);
        while self.used_bytes > budget {
            if let Some(evict_id) = self.lru.pop_front() {
                if evict_id == entry.segment_id {
                    self.lru.push_back(evict_id);
                    break;
                }
                if let Some(evicted_entry) = self.manifest.remove(evict_id) {
                    let path = self.layout.warm_dir().join(&evicted_entry.compressed_path);
                    if let Err(err) = fs::remove_file(&path) {
                        warn!(path = %path.display(), "failed to remove warm file: {err}");
                    }
                    self.used_bytes = self
                        .used_bytes
                        .saturating_sub(evicted_entry.compressed_bytes);
                    evicted.push(evicted_entry);
                }
            } else {
                break;
            }
        }
        self.manifest.save()?;
        Ok(evicted)
    }

    fn update_residency(
        &mut self,
        segment_id: SegmentId,
        residency: Tier1ResidencyState,
    ) -> AofResult<()> {
        if let Some(entry) = self.manifest.get_mut(segment_id) {
            entry.residency = residency;
            entry.touch();
            self.touch(segment_id);
            self.manifest.save()?;
        }
        Ok(())
    }

    fn remove_entry(&mut self, segment_id: SegmentId) -> Option<ManifestEntry> {
        self.lru.retain(|id| *id != segment_id);
        if let Some(entry) = self.manifest.remove(segment_id) {
            self.used_bytes = self.used_bytes.saturating_sub(entry.compressed_bytes);
            let path = self.layout.warm_dir().join(&entry.compressed_path);
            if let Err(err) = fs::remove_file(&path) {
                warn!(path = %path.display(), "failed to remove warm file: {err}");
            }
            let _ = self.manifest.save();
            Some(entry)
        } else {
            None
        }
    }

    fn used_bytes(&self) -> u64 {
        self.used_bytes
    }
}

#[derive(Debug, Clone)]
pub struct HydrationOutcome {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub path: PathBuf,
    pub base_offset: u64,
    pub base_record_count: u64,
    pub created_at: i64,
    pub sealed_at: i64,
    pub checksum: u32,
    pub logical_size: u32,
}

pub struct Tier1Cache {
    inner: Arc<Tier1Inner>,
    dispatcher: Mutex<Option<JoinHandle<()>>>,
}

impl Tier1Cache {
    pub fn new(runtime: Handle, config: Tier1Config) -> AofResult<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let inner = Arc::new(Tier1Inner::new(runtime.clone(), config, tx));
        let dispatcher_inner = Arc::clone(&inner);
        let dispatcher = runtime.spawn(async move {
            dispatcher_inner.run(rx).await;
        });
        Ok(Self {
            inner,
            dispatcher: Mutex::new(Some(dispatcher)),
        })
    }

    pub fn metrics(&self) -> Tier1MetricsSnapshot {
        self.inner.metrics.snapshot()
    }

    pub fn register_instance(
        &self,
        instance_id: InstanceId,
        layout: Layout,
    ) -> AofResult<Tier1Instance> {
        self.inner.register_instance(instance_id, layout)?;
        Ok(Tier1Instance {
            inner: Arc::clone(&self.inner),
            instance_id,
        })
    }

    pub fn attach_tier2(&self, handle: Tier2Handle) {
        self.inner.attach_tier2(handle);
    }

    pub fn handle_tier2_events(&self, events: Vec<Tier2Event>) {
        if !events.is_empty() {
            self.inner.handle_tier2_events(events);
        }
    }

    pub fn handle_drop_events(&self, events: Vec<Tier0DropEvent>) {
        for event in events {
            if let Some(segment) = event.segment {
                if !event.was_sealed {
                    trace!(
                        segment = event.segment_id.as_u64(),
                        "skipping unsealed segment drop"
                    );
                    continue;
                }
                if let Err(err) = self
                    .inner
                    .enqueue_compression(event.instance_id, segment, None)
                {
                    warn!(
                        instance = event.instance_id.get(),
                        segment = event.segment_id.as_u64(),
                        "failed to enqueue compression: {err}"
                    );
                }
            } else {
                warn!(
                    instance = event.instance_id.get(),
                    segment = event.segment_id.as_u64(),
                    "drop event missing segment handle"
                );
            }
        }
    }

    pub fn update_residency(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
        residency: Tier1ResidencyState,
    ) -> AofResult<()> {
        self.inner
            .update_residency(instance_id, segment_id, residency)
    }

    pub async fn handle_activation_grants(
        &self,
        grants: Vec<ActivationGrant>,
    ) -> Vec<HydrationOutcome> {
        self.inner.process_hydration_requests_async(grants).await
    }

    pub fn manifest_snapshot(&self, instance_id: InstanceId) -> AofResult<Vec<ManifestEntry>> {
        self.inner.manifest_snapshot(instance_id)
    }
}

impl Drop for Tier1Cache {
    fn drop(&mut self) {
        if let Some(handle) = self.dispatcher.lock().take() {
            handle.abort();
        }
    }
}

#[derive(Clone)]
pub struct Tier1Instance {
    inner: Arc<Tier1Inner>,
    instance_id: InstanceId,
}

impl Tier1Instance {
    pub fn instance_id(&self) -> InstanceId {
        self.instance_id
    }

    pub fn schedule_compression(&self, segment: Arc<Segment>) -> AofResult<()> {
        self.schedule_compression_with_ack(segment, None)
    }

    pub fn schedule_compression_with_ack(
        &self,
        segment: Arc<Segment>,
        ack: Option<oneshot::Sender<AofResult<()>>>,
    ) -> AofResult<()> {
        self.inner
            .enqueue_compression(self.instance_id, segment, ack)
    }

    pub fn update_residency(
        &self,
        segment_id: SegmentId,
        residency: Tier1ResidencyState,
    ) -> AofResult<()> {
        self.inner
            .update_residency(self.instance_id, segment_id, residency)
    }

    pub fn manifest_entry(&self, segment_id: SegmentId) -> AofResult<Option<ManifestEntry>> {
        self.inner.manifest_entry(self.instance_id, segment_id)
    }

    pub fn hydrate_segment_blocking(
        &self,
        segment_id: SegmentId,
    ) -> AofResult<Option<HydrationOutcome>> {
        self.inner
            .hydrate_segment_blocking(self.instance_id, segment_id)
    }
}

struct CompressionJob {
    instance_id: InstanceId,
    segment: Arc<Segment>,
    ack: Option<oneshot::Sender<AofResult<()>>>,
}

#[derive(Debug)]
struct Tier1Inner {
    runtime: Handle,
    config: Tier1Config,
    compression_tx: mpsc::UnboundedSender<CompressionCommand>,
    semaphore: Arc<Semaphore>,
    instances: Mutex<HashMap<InstanceId, Tier1InstanceState>>,
    metrics: Tier1Metrics,
    tier2: Mutex<Option<Tier2Handle>>,
}

impl Tier1Inner {
    fn new(
        runtime: Handle,
        config: Tier1Config,
        compression_tx: mpsc::UnboundedSender<CompressionCommand>,
    ) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.worker_threads.max(1)));
        Self {
            runtime,
            config,
            compression_tx,
            semaphore,
            instances: Mutex::new(HashMap::new()),
            metrics: Tier1Metrics::default(),
            tier2: Mutex::new(None),
        }
    }

    async fn run(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<CompressionCommand>) {
        while let Some(command) = rx.recv().await {
            match command {
                CompressionCommand::Compress(job) => {
                    let permit = match Arc::clone(&self.semaphore).acquire_owned().await {
                        Ok(permit) => permit,
                        Err(_) => break,
                    };
                    let inner = Arc::clone(&self);
                    let runtime = self.runtime.clone();
                    runtime.spawn(async move {
                        let _permit = permit;
                        inner.process_compression_job(job).await;
                    });
                }
                CompressionCommand::Shutdown => break,
            }
        }
    }

    fn register_instance(&self, instance_id: InstanceId, layout: Layout) -> AofResult<()> {
        let mut instances = self.instances.lock();
        if !instances.contains_key(&instance_id) {
            let state = Tier1InstanceState::new(layout)?;
            instances.insert(instance_id, state);
        }
        Ok(())
    }

    fn enqueue_compression(
        self: &Arc<Self>,
        instance_id: InstanceId,
        segment: Arc<Segment>,
        ack: Option<oneshot::Sender<AofResult<()>>>,
    ) -> AofResult<()> {
        if !segment.is_sealed() {
            let message = "cannot compress unsealed segment".to_string();
            if let Some(tx) = ack {
                let _ = tx.send(Err(AofError::InvalidState(message.clone())));
            }
            return Err(AofError::InvalidState(message));
        }
        if !self.instances.lock().contains_key(&instance_id) {
            let message = format!("Tier1 instance {:?} not registered", instance_id.get());
            if let Some(tx) = ack {
                let _ = tx.send(Err(AofError::InvalidState(message.clone())));
            }
            return Err(AofError::InvalidState(message));
        }
        let job = CompressionJob {
            instance_id,
            segment,
            ack,
        };
        match self.compression_tx.send(CompressionCommand::Compress(job)) {
            Ok(()) => Ok(()),
            Err(err) => {
                if let CompressionCommand::Compress(job) = err.0 {
                    if let Some(tx) = job.ack {
                        let _ = tx.send(Err(AofError::other("compression queue closed")));
                    }
                }
                Err(AofError::other("failed to enqueue compression job"))
            }
        }
    }

    fn attach_tier2(&self, handle: Tier2Handle) {
        *self.tier2.lock() = Some(handle);
    }

    fn tier2(&self) -> Option<Tier2Handle> {
        self.tier2.lock().clone()
    }

    fn handle_tier2_events(&self, events: Vec<Tier2Event>) {
        for event in events {
            match event {
                Tier2Event::UploadCompleted(info) => {
                    if let Err(err) = self.apply_upload_complete(info) {
                        warn!("tier2 upload complete handling failed: {err}");
                    }
                }
                Tier2Event::UploadFailed(failure) => {
                    warn!(
                        instance = failure.instance_id.get(),
                        segment = failure.segment_id.as_u64(),
                        "tier2 upload failed: {}",
                        failure.error
                    );
                }
                Tier2Event::DeleteCompleted(info) => {
                    if let Err(err) = self.apply_delete_complete(info) {
                        warn!("tier2 delete complete handling failed: {err}");
                    }
                }
                Tier2Event::DeleteFailed(failure) => {
                    warn!(
                        instance = failure.instance_id.get(),
                        segment = failure.segment_id.as_u64(),
                        "tier2 delete failed: {}",
                        failure.error
                    );
                }
            }
        }
    }

    fn apply_upload_complete(&self, info: Tier2UploadComplete) -> AofResult<()> {
        let mut instances = self.instances.lock();
        let state = instances.get_mut(&info.instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {} not registered",
                info.instance_id.get()
            ))
        })?;
        if let Some(entry) = state.manifest.get_mut(info.segment_id) {
            entry.tier2 = Some(info.metadata.clone());
            entry.residency = Tier1ResidencyState::UploadedToTier2;
            entry.touch();
            state.touch(info.segment_id);
            state.manifest.save()?;
        }
        Ok(())
    }

    fn apply_delete_complete(&self, info: Tier2DeleteComplete) -> AofResult<()> {
        let mut instances = self.instances.lock();
        let state = instances.get_mut(&info.instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {} not registered",
                info.instance_id.get()
            ))
        })?;
        if let Some(entry) = state.manifest.get_mut(info.segment_id) {
            entry.tier2 = None;
            entry.touch();
            state.manifest.save()?;
        }
        Ok(())
    }

    async fn process_compression_job(self: Arc<Self>, mut job: CompressionJob) {
        let max_attempts = self.config.max_retries.max(1);
        let mut attempt = 0;
        let instance_id = job.instance_id;
        let segment = job.segment.clone();

        let final_result = loop {
            let inner = Arc::clone(&self);
            let segment_clone = segment.clone();
            let result = spawn_blocking(move || {
                inner.run_compression_job_blocking(instance_id, &segment_clone)
            })
            .await;

            match result {
                Ok(Ok(())) => break Ok(()),
                Ok(Err(err)) => {
                    attempt += 1;
                    if attempt >= max_attempts {
                        self.metrics.incr_compression_failures();
                        warn!(
                            instance = instance_id.get(),
                            segment = segment.id().as_u64(),
                            "compression job failed: {err}"
                        );
                        break Err(err);
                    }
                    tokio_sleep(self.config.retry_backoff * attempt as u32).await;
                }
                Err(join_err) => {
                    self.metrics.incr_compression_failures();
                    warn!(
                        instance = instance_id.get(),
                        segment = segment.id().as_u64(),
                        "compression job panicked: {join_err}"
                    );
                    break Err(AofError::other(format!(
                        "compression task panicked: {join_err}"
                    )));
                }
            }
        };

        if let Some(ack) = job.ack.take() {
            let _ = ack.send(final_result);
        }
    }

    fn run_compression_job_blocking(
        &self,
        instance_id: InstanceId,
        segment: &Arc<Segment>,
    ) -> AofResult<()> {
        let entry = self.compress_segment_blocking(instance_id, segment)?;
        self.finish_compression(instance_id, entry)?;
        Ok(())
    }

    fn finish_compression(&self, instance_id: InstanceId, entry: ManifestEntry) -> AofResult<()> {
        let tier2_handle = self.tier2();
        let (evicted, stored_bytes, upload_descriptor) = {
            let mut instances = self.instances.lock();
            let state = instances.get_mut(&instance_id).ok_or_else(|| {
                AofError::InvalidState(format!(
                    "Tier1 instance {:?} not registered",
                    instance_id.get()
                ))
            })?;
            let warm_path = state.layout.warm_dir().join(&entry.compressed_path);
            let descriptor = tier2_handle.as_ref().map(|_| Tier2UploadDescriptor {
                instance_id,
                segment_id: entry.segment_id,
                warm_path,
                sealed_at: entry.sealed_at,
                base_offset: entry.base_offset,
                base_record_count: entry.base_record_count,
                checksum: entry.checksum,
                compressed_bytes: entry.compressed_bytes,
                original_bytes: entry.original_bytes,
            });
            let evicted = state.insert_entry(entry, self.config.max_bytes)?;
            let stored_bytes = state.used_bytes();
            (evicted, stored_bytes, descriptor)
        };
        self.metrics.set_stored_bytes(stored_bytes);

        if let Some(tier2) = tier2_handle {
            if let Some(descriptor) = upload_descriptor {
                tier2
                    .schedule_upload(descriptor)
                    .map_err(AofError::rollover_failed)?;
            }
            for evicted_entry in evicted {
                if let Some(metadata) = evicted_entry.tier2 {
                    let request = Tier2DeleteRequest {
                        instance_id,
                        segment_id: evicted_entry.segment_id,
                        object_key: metadata.object_key,
                    };
                    if let Err(err) = tier2.schedule_delete(request) {
                        warn!(
                            instance = instance_id.get(),
                            segment = evicted_entry.segment_id.as_u64(),
                            "failed to schedule tier2 delete for evicted warm entry: {err}"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    fn compress_segment_blocking(
        &self,
        instance_id: InstanceId,
        segment: &Arc<Segment>,
    ) -> AofResult<ManifestEntry> {
        let footer = read_segment_footer(segment)?;
        let layout = {
            let instances = self.instances.lock();
            let state = instances.get(&instance_id).ok_or_else(|| {
                AofError::InvalidState(format!(
                    "Tier1 instance {:?} not registered",
                    instance_id.get()
                ))
            })?;
            state.layout.clone()
        };

        let sealed_at = footer.sealed_at;
        let warm_path = layout.warm_file_path(segment.id(), segment.base_offset(), sealed_at);
        if warm_path.exists() {
            fs::remove_file(&warm_path).map_err(AofError::from)?;
        }
        let warm_file_name = warm_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                AofError::invalid_config(format!("invalid warm file name: {}", warm_path.display()))
            })?
            .to_string();
        let temp = NamedTempFile::new_in(layout.warm_dir()).map_err(AofError::from)?;
        {
            let mut source = File::open(segment.path()).map_err(AofError::from)?;
            let mut encoder = ZstdEncoder::new(temp.as_file(), self.config.compression_level)
                .map_err(|err| AofError::CompressionError(err.to_string()))?;
            encoder
                .include_checksum(true)
                .map_err(|err| AofError::CompressionError(err.to_string()))?;
            io::copy(&mut source, &mut encoder).map_err(AofError::from)?;
            encoder
                .finish()
                .map_err(|err| AofError::CompressionError(err.to_string()))?;
        }
        temp.as_file().sync_all().map_err(AofError::from)?;
        let metadata = temp.as_file().metadata().map_err(AofError::from)?;
        temp.persist(&warm_path)
            .map_err(|err| AofError::from(err.error))?;

        let offset_index = build_offset_index(segment)?;

        Ok(ManifestEntry {
            segment_id: segment.id(),
            base_offset: segment.base_offset(),
            base_record_count: segment.base_record_count(),
            created_at: segment.created_at(),
            sealed_at,
            checksum: footer.checksum,
            original_bytes: segment.current_size() as u64,
            compressed_bytes: metadata.len(),
            compressed_path: warm_file_name,
            dictionary: None,
            offset_index,
            residency: Tier1ResidencyState::StagedForTier0,
            tier2: None,
            last_access_epoch_ms: current_epoch_ms(),
        })
    }

    async fn process_hydration_requests_async(
        self: &Arc<Self>,
        grants: Vec<ActivationGrant>,
    ) -> Vec<HydrationOutcome> {
        let futures = grants.into_iter().map(|grant| {
            let inner = Arc::clone(self);
            async move {
                let hydrate_owner = Arc::clone(&inner);
                match hydrate_owner
                    .hydrate_segment_async(grant.instance_id, grant.segment_id)
                    .await
                {
                    Ok(Some(outcome)) => Some(outcome),
                    Ok(None) => {
                        debug!(
                            instance = grant.instance_id.get(),
                            segment = grant.segment_id.as_u64(),
                            "hydrate skipped: no manifest entry"
                        );
                        None
                    }
                    Err(err) => {
                        inner.metrics.incr_hydration_failures();
                        warn!(
                            instance = grant.instance_id.get(),
                            segment = grant.segment_id.as_u64(),
                            "hydrate failed: {err}"
                        );
                        None
                    }
                }
            }
        });
        let results = join_all(futures).await;
        results.into_iter().flatten().collect()
    }

    async fn hydrate_segment_async(
        self: Arc<Self>,
        instance_id: InstanceId,
        segment_id: SegmentId,
    ) -> AofResult<Option<HydrationOutcome>> {
        let inner = Arc::clone(&self);
        spawn_blocking(move || inner.hydrate_segment_blocking(instance_id, segment_id))
            .await
            .map_err(|err| AofError::Other(format!("hydrate task panicked: {err}")))?
    }

    fn hydrate_segment_blocking(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
    ) -> AofResult<Option<HydrationOutcome>> {
        let (layout, mut entry) = {
            let mut instances = self.instances.lock();
            let state = match instances.get_mut(&instance_id) {
                Some(state) => state,
                None => {
                    return Err(AofError::InvalidState(format!(
                        "Tier1 instance {:?} not registered",
                        instance_id.get()
                    )));
                }
            };
            match state.manifest.get(segment_id).cloned() {
                Some(entry) => (state.layout.clone(), entry),
                None => {
                    trace!(
                        segment = segment_id.as_u64(),
                        "manifest missing entry for hydration"
                    );
                    return Ok(None);
                }
            }
        };

        let compressed_path = layout.warm_dir().join(&entry.compressed_path);
        trace!(
            segment = segment_id.as_u64(),
            path = %compressed_path.display(),
            exists = compressed_path.exists(),
            "hydrate warm lookup"
        );
        if !compressed_path.exists() {
            if let Some(tier2) = self.tier2() {
                let fetch_request = Tier2FetchRequest {
                    instance_id,
                    segment_id,
                    sealed_at: entry.sealed_at,
                    destination: compressed_path.clone(),
                };
                match tier2.fetch_segment(fetch_request) {
                    Ok(true) => {
                        debug!("tier2 provided warm file for hydrate");
                    }
                    Ok(false) => {
                        return Ok(None);
                    }
                    Err(err) => {
                        self.metrics.incr_hydration_failures();
                        warn!("tier2 fetch failed during hydrate: {err}");
                        return Err(err);
                    }
                }
            } else {
                warn!("warm file missing during hydrate and tier2 unavailable");
                return Ok(None);
            }
        }

        let mut decoder = ZstdDecoder::new(File::open(&compressed_path).map_err(AofError::from)?)
            .map_err(|err| AofError::CompressionError(err.to_string()))?;
        let mut decompressed_bytes = Vec::new();
        let mut temp = NamedTempFile::new_in(layout.segments_dir()).map_err(AofError::from)?;
        {
            let file = temp.as_file_mut();
            let mut buffer = [0u8; 8 * 1024];
            loop {
                let read_bytes = decoder.read(&mut buffer).map_err(AofError::from)?;
                if read_bytes == 0 {
                    break;
                }
                file.write_all(&buffer[..read_bytes])
                    .map_err(AofError::from)?;
                decompressed_bytes.extend_from_slice(&buffer[..read_bytes]);
            }
            file.sync_all().map_err(AofError::from)?;
        }
        let logical_size = decompressed_bytes.len() as u64;
        let checksum = compute_manifest_checksum(&decompressed_bytes)?;
        if checksum != entry.checksum {
            return Err(AofError::Corruption(format!(
                "hydrated checksum mismatch for segment {}",
                segment_id.as_u64()
            )));
        }

        let destination = layout.segment_file_path(segment_id, entry.base_offset, entry.created_at);
        temp.persist(&destination)
            .map_err(|err| AofError::from(err.error))?;

        entry.residency = Tier1ResidencyState::StagedForTier0;
        entry.touch();

        {
            let mut instances = self.instances.lock();
            if let Some(state) = instances.get_mut(&instance_id) {
                state.touch(segment_id);
                if let Some(manifest_entry) = state.manifest.get_mut(segment_id) {
                    *manifest_entry = entry.clone();
                }
                state.manifest.save()?;
            }
        }

        Ok(Some(HydrationOutcome {
            instance_id,
            segment_id,
            path: destination,
            base_offset: entry.base_offset,
            base_record_count: entry.base_record_count,
            created_at: entry.created_at,
            sealed_at: entry.sealed_at,
            checksum: entry.checksum,
            logical_size: logical_size as u32,
        }))
    }

    fn update_residency(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
        residency: Tier1ResidencyState,
    ) -> AofResult<()> {
        let mut instances = self.instances.lock();
        let state = instances.get_mut(&instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {:?} not registered",
                instance_id.get()
            ))
        })?;
        state.update_residency(segment_id, residency)
    }

    fn manifest_entry(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
    ) -> AofResult<Option<ManifestEntry>> {
        let instances = self.instances.lock();
        let state = instances.get(&instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {:?} not registered",
                instance_id.get()
            ))
        })?;
        Ok(state.manifest.get(segment_id).cloned())
    }

    fn manifest_snapshot(&self, instance_id: InstanceId) -> AofResult<Vec<ManifestEntry>> {
        let instances = self.instances.lock();
        let state = instances.get(&instance_id).ok_or_else(|| {
            AofError::InvalidState(format!(
                "Tier1 instance {:?} not registered",
                instance_id.get()
            ))
        })?;
        Ok(state.manifest.all().cloned().collect())
    }
}

enum CompressionCommand {
    Compress(CompressionJob),
    Shutdown,
}

fn read_segment_footer(segment: &Segment) -> AofResult<SegmentFooter> {
    let mut file = File::open(segment.path()).map_err(AofError::from)?;
    let mut buf = [0u8; SEGMENT_FOOTER_SIZE as usize];
    let footer_offset = segment.footer_offset() as u64;
    file.seek(SeekFrom::Start(footer_offset))
        .map_err(AofError::from)?;
    file.read_exact(&mut buf).map_err(AofError::from)?;
    SegmentFooter::decode(&buf).ok_or_else(|| {
        AofError::Corruption(format!(
            "failed to decode footer for segment {}",
            segment.id().as_u64()
        ))
    })
}

fn build_offset_index(segment: &Segment) -> AofResult<Vec<u32>> {
    let mut offsets = Vec::new();
    let mut file = File::open(segment.path()).map_err(AofError::from)?;
    let mut header = [0u8; RECORD_HEADER_SIZE as usize];
    let mut cursor = SEGMENT_HEADER_SIZE as u64;
    let durable = segment.durable_size() as u64;

    while cursor + RECORD_HEADER_SIZE as u64 <= durable {
        file.seek(SeekFrom::Start(cursor)).map_err(AofError::from)?;
        file.read_exact(&mut header).map_err(AofError::from)?;
        let length = u32::from_le_bytes(
            header[0..4]
                .try_into()
                .map_err(|_| AofError::Corruption("record header corrupt".to_string()))?,
        );
        if length == 0 {
            break;
        }
        offsets.push(cursor as u32);
        cursor = cursor
            .checked_add(RECORD_HEADER_SIZE as u64)
            .and_then(|value| value.checked_add(length as u64))
            .ok_or_else(|| AofError::Corruption("record length overflow".to_string()))?;
        if cursor > durable {
            return Err(AofError::Corruption(
                "record extends beyond durable size".to_string(),
            ));
        }
    }

    Ok(offsets)
}

fn compute_manifest_checksum(payload: &[u8]) -> AofResult<u32> {
    if payload.len() <= SEGMENT_HEADER_SIZE as usize {
        return Ok(0);
    }

    let usable_limit = payload.len().saturating_sub(SEGMENT_FOOTER_SIZE as usize);
    if usable_limit <= SEGMENT_HEADER_SIZE as usize {
        return Ok(0);
    }

    let mut cursor = SEGMENT_HEADER_SIZE as usize;
    let mut digest = Digest::new();
    while cursor + RECORD_HEADER_SIZE as usize <= usable_limit {
        let header = &payload[cursor..cursor + RECORD_HEADER_SIZE as usize];
        let length = u32::from_le_bytes(
            header[0..4]
                .try_into()
                .map_err(|_| AofError::Corruption("record length corrupt".to_string()))?,
        ) as usize;
        if length == 0 {
            break;
        }

        let payload_start = cursor + RECORD_HEADER_SIZE as usize;
        let payload_end = payload_start + length;
        if payload_end > usable_limit {
            break;
        }

        digest.write(&payload[payload_start..payload_end]);
        cursor = payload_end;
    }

    Ok(fold_crc64(digest.sum64()))
}

fn fold_crc64(value: u64) -> u32 {
    (value as u32) ^ ((value >> 32) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aof2::config::{AofConfig, FlushConfig};
    use crate::aof2::store::tier0::ActivationReason;
    use crate::aof2::store::tier0::{Tier0Cache, Tier0CacheConfig};
    use crate::aof2::store::tier2::{
        GetObjectRequest, GetObjectResult, HeadObjectResult, PutObjectRequest, PutObjectResult,
        Tier2Client, Tier2Config, Tier2Manager,
    };
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;

    fn build_layout(dir: &TempDir) -> Layout {
        let config = AofConfig {
            root_dir: dir.path().to_path_buf(),
            flush: FlushConfig::default(),
            ..AofConfig::default()
        }
        .normalized();
        let layout = Layout::new(&config);
        layout.ensure().expect("layout ensure");
        layout
    }

    fn create_sealed_segment(layout: &Layout, id: u64, payload: &[u8]) -> Arc<Segment> {
        let segment_id = SegmentId::new(id);
        let path = layout.segment_file_path(segment_id, 0, 0);
        let segment = Segment::create_active(segment_id, 0, 0, 0, 1024, path.as_path())
            .expect("segment create");
        let now = current_epoch_ms();
        let append = segment.append_record(payload, now).expect("append record");
        let flush_state = segment.flush_state();
        let _ = flush_state.try_begin_flush();
        flush_state.mark_durable(append.logical_size);
        flush_state.finish_flush();
        segment.mark_durable(append.logical_size);
        assert_eq!(segment.durable_size(), append.logical_size);
        segment.seal(now as i64).expect("seal segment");
        Arc::new(segment)
    }

    #[derive(Clone, Default)]
    struct RecordingTier2Client {
        uploads: Arc<StdMutex<Vec<String>>>,
    }

    impl RecordingTier2Client {
        fn upload_count(&self) -> usize {
            self.uploads.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl Tier2Client for RecordingTier2Client {
        async fn put_object(&self, request: PutObjectRequest) -> AofResult<PutObjectResult> {
            let size = tokio::fs::metadata(&request.source)
                .await
                .map_err(AofError::from)?
                .len();
            self.uploads.lock().unwrap().push(request.key.clone());
            Ok(PutObjectResult {
                etag: Some("mock-etag".to_string()),
                size,
            })
        }

        async fn get_object(&self, _request: GetObjectRequest) -> AofResult<GetObjectResult> {
            Err(AofError::InvalidState(
                "recording tier2 client does not support get_object".to_string(),
            ))
        }

        async fn delete_object(&self, _bucket: &str, _key: &str) -> AofResult<()> {
            Ok(())
        }

        async fn head_object(
            &self,
            _bucket: &str,
            _key: &str,
        ) -> AofResult<Option<HeadObjectResult>> {
            Ok(None)
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compression_pipeline_persists_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let tier0 = Tier0Cache::new(Tier0CacheConfig::new(1024 * 1024, 1024 * 1024));
        let tier0_instance = tier0.register_instance("test", None);
        let config = Tier1Config::new(10 * 1024 * 1024).with_worker_threads(1);
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");
        let tier1_instance = tier1
            .register_instance(tier0_instance.instance_id(), layout.clone())
            .expect("register tier1 instance");
        let segment = create_sealed_segment(&layout, 1, b"payload");

        tier1_instance
            .schedule_compression(segment.clone())
            .expect("schedule compression");

        wait_for(|| {
            tier1
                .manifest_snapshot(tier0_instance.instance_id())
                .ok()
                .and_then(|entries| {
                    entries
                        .into_iter()
                        .find(|entry| entry.segment_id == segment.id())
                })
                .is_some()
        })
        .await;

        let snapshot = tier1
            .manifest_snapshot(tier0_instance.instance_id())
            .expect("manifest snapshot");
        assert_eq!(snapshot.len(), 1);
        let entry = &snapshot[0];
        assert_eq!(entry.segment_id, segment.id());
        assert_eq!(entry.base_offset, segment.base_offset());
        assert_eq!(entry.base_record_count, segment.base_record_count());

        let warm_file = layout.warm_dir().join(&entry.compressed_path);
        assert!(warm_file.exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hydration_decompresses_segment() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let tier0 = Tier0Cache::new(Tier0CacheConfig::new(1024 * 1024, 1024 * 1024));
        let tier0_instance = tier0.register_instance("test", None);
        let config = Tier1Config::new(10 * 1024 * 1024).with_worker_threads(1);
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");
        let tier1_instance = tier1
            .register_instance(tier0_instance.instance_id(), layout.clone())
            .expect("register tier1 instance");

        let segment = create_sealed_segment(&layout, 2, b"hydrate");
        let segment_id = segment.id();
        let segment_bytes = segment.current_size() as u64;
        tier1_instance
            .schedule_compression(segment.clone())
            .expect("schedule compression");
        drop(segment);

        wait_for(|| {
            tier1
                .manifest_snapshot(tier0_instance.instance_id())
                .ok()
                .and_then(|entries| {
                    entries
                        .into_iter()
                        .find(|entry| entry.segment_id == segment_id)
                })
                .is_some()
        })
        .await;

        let grants = vec![ActivationGrant {
            instance_id: tier0_instance.instance_id(),
            segment_id,
            bytes: segment_bytes,
            priority: 10,
            reason: ActivationReason::ReadMiss,
        }];

        let outcomes = tier1.handle_activation_grants(grants).await;
        assert_eq!(outcomes.len(), 1);
        let outcome = &outcomes[0];
        assert_eq!(outcome.segment_id, segment_id);
        assert!(outcome.path.exists());

        let snapshot = tier1
            .manifest_snapshot(tier0_instance.instance_id())
            .expect("manifest snapshot");
        let entry = snapshot
            .into_iter()
            .find(|entry| entry.segment_id == segment_id)
            .expect("entry present");
        assert!(matches!(
            entry.residency,
            Tier1ResidencyState::ResidentInTier0 | Tier1ResidencyState::StagedForTier0
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compression_schedules_tier2_upload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = build_layout(&dir);
        let tier0 = Tier0Cache::new(Tier0CacheConfig::new(1024 * 1024, 1024 * 1024));
        let tier0_instance = tier0.register_instance("tier2", None);
        let config = Tier1Config::new(10 * 1024 * 1024).with_worker_threads(1);
        let tier1 =
            Tier1Cache::new(tokio::runtime::Handle::current(), config).expect("tier1 cache");

        let client = RecordingTier2Client::default();
        let tier2_manager = Tier2Manager::with_client(
            tokio::runtime::Handle::current(),
            Tier2Config::default(),
            Arc::new(client.clone()),
        )
        .expect("tier2 manager");
        tier1.attach_tier2(tier2_manager.handle());

        let tier1_instance = tier1
            .register_instance(tier0_instance.instance_id(), layout.clone())
            .expect("register tier1 instance");

        let segment = create_sealed_segment(&layout, 7, b"tier2 upload");
        tier1_instance
            .schedule_compression(segment.clone())
            .expect("schedule compression");

        wait_for(|| client.upload_count() >= 1).await;

        drop(segment);
        drop(tier2_manager);
    }

    async fn wait_for(mut predicate: impl FnMut() -> bool) {
        for _ in 0..50 {
            if predicate() {
                return;
            }
            tokio_sleep(Duration::from_millis(20)).await;
        }
        panic!("condition not satisfied");
    }
}
