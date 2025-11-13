use crate::storage_types::StoragePodId;
use std::io;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::{Semaphore, mpsc, oneshot, watch};

#[derive(Clone, Debug)]
pub struct FlushControllerConfig {
    pub queue_depth: usize,
    pub max_parallel: usize,
}

impl Default for FlushControllerConfig {
    fn default() -> Self {
        Self {
            queue_depth: 32,
            max_parallel: 4,
        }
    }
}

#[derive(Clone)]
pub struct FlushController {
    tx: Arc<Mutex<Option<mpsc::Sender<FlushEnvelope>>>>,
    state: Arc<FlushControllerState>,
    config: FlushControllerConfig,
}

impl FlushController {
    pub fn new(handle: Handle, config: FlushControllerConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.queue_depth);
        let (metrics_tx, _metrics_rx) = watch::channel(FlushControllerSnapshot::default());
        let sender = Arc::new(Mutex::new(Some(tx.clone())));
        let state = Arc::new(FlushControllerState {
            handle,
            semaphore: Arc::new(Semaphore::new(config.max_parallel.max(1))),
            metrics: Arc::new(FlushMetrics::default()),
            metrics_tx,
        });
        FlushController::spawn_worker(&state, rx);
        Self {
            tx: sender,
            state,
            config,
        }
    }

    fn spawn_worker(state: &Arc<FlushControllerState>, rx: mpsc::Receiver<FlushEnvelope>) {
        let state = state.clone();
        state.publish_metrics();
        let handle = state.handle.clone();
        handle.spawn(async move {
            let mut rx = rx;
            while let Some(envelope) = rx.recv().await {
                state.metrics.pending.fetch_sub(1, Ordering::SeqCst);
                state.publish_metrics();
                let FlushEnvelope { job, complete } = envelope;
                let permit = match state.semaphore.clone().acquire_owned().await {
                    Ok(permit) => permit,
                    Err(_) => {
                        let _ = complete.send(Err(FlushError::ControllerShutdown));
                        continue;
                    }
                };
                state.metrics.inflight.fetch_add(1, Ordering::SeqCst);
                state.publish_metrics();
                let state_clone = state.clone();
                let handle_clone = state.handle.clone();
                let handle_for_task = handle_clone.clone();
                handle_clone.spawn(async move {
                    let result = handle_for_task
                        .spawn_blocking(move || execute_batch(job))
                        .await
                        .map_err(|err| FlushError::WorkerPanic(err.to_string()))
                        .and_then(|res| res);
                    state_clone.metrics.inflight.fetch_sub(1, Ordering::SeqCst);
                    match &result {
                        Ok(outcome) if outcome.is_success() => {
                            state_clone.metrics.completed.fetch_add(1, Ordering::SeqCst);
                        }
                        Ok(outcome) => {
                            if outcome.has_failure() {
                                state_clone.metrics.failed.fetch_add(1, Ordering::SeqCst);
                            } else {
                                state_clone.metrics.completed.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                        Err(_) => {
                            state_clone.metrics.failed.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    state_clone.publish_metrics();
                    let _ = complete.send(result);
                    drop(permit);
                });
            }
            state.publish_metrics();
        });
    }

    pub async fn submit(
        &self,
        pod_id: StoragePodId,
        actions: Vec<FlushAction>,
    ) -> Result<FlushTicket, FlushScheduleError> {
        if actions.is_empty() {
            return Err(FlushScheduleError::EmptyBatch);
        }
        let (tx, rx) = oneshot::channel();
        let envelope = FlushEnvelope {
            job: FlushJob {
                pod_id,
                actions,
                submitted_at: SystemTime::now(),
            },
            complete: tx,
        };
        self.state.metrics.pending.fetch_add(1, Ordering::SeqCst);
        self.state.publish_metrics();
        let sender = {
            let guard = self.tx.lock().unwrap();
            guard.as_ref().cloned()
        };
        let tx = match sender {
            Some(tx) => tx,
            None => {
                self.state.metrics.pending.fetch_sub(1, Ordering::SeqCst);
                self.state.publish_metrics();
                return Err(FlushScheduleError::ControllerClosed);
            }
        };
        if tx.send(envelope).await.is_err() {
            self.state.metrics.pending.fetch_sub(1, Ordering::SeqCst);
            self.state.publish_metrics();
            return Err(FlushScheduleError::ControllerClosed);
        }
        Ok(FlushTicket { rx })
    }

    pub fn subscribe(&self) -> watch::Receiver<FlushControllerSnapshot> {
        self.state.metrics_tx.subscribe()
    }

    pub fn snapshot(&self) -> FlushControllerSnapshot {
        self.state.metrics_snapshot()
    }

    pub fn begin_shutdown(&self) {
        if let Ok(mut guard) = self.tx.lock() {
            guard.take();
        }
    }

    pub fn is_idle(&self) -> bool {
        let snapshot = self.state.metrics_snapshot();
        snapshot.pending == 0 && snapshot.inflight == 0
    }

    pub fn wait_for_idle(&self, timeout: Duration) -> bool {
        if self.is_idle() {
            return true;
        }
        let start = Instant::now();
        loop {
            if self.is_idle() {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn shutdown(&self, timeout: Duration) -> bool {
        self.begin_shutdown();
        self.wait_for_idle(timeout)
    }

    pub fn config(&self) -> &FlushControllerConfig {
        &self.config
    }
}

struct FlushControllerState {
    handle: Handle,
    semaphore: Arc<Semaphore>,
    metrics: Arc<FlushMetrics>,
    metrics_tx: watch::Sender<FlushControllerSnapshot>,
}

impl FlushControllerState {
    fn publish_metrics(&self) {
        let snapshot = self.metrics_snapshot();
        let _ = self.metrics_tx.send(snapshot);
    }

    fn metrics_snapshot(&self) -> FlushControllerSnapshot {
        FlushControllerSnapshot {
            pending: self.metrics.pending.load(Ordering::SeqCst),
            inflight: self.metrics.inflight.load(Ordering::SeqCst),
            completed: self.metrics.completed.load(Ordering::SeqCst),
            failed: self.metrics.failed.load(Ordering::SeqCst),
            timestamp: SystemTime::now(),
        }
    }
}

#[derive(Default)]
struct FlushMetrics {
    pending: AtomicU64,
    inflight: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
}

struct FlushEnvelope {
    job: FlushJob,
    complete: oneshot::Sender<Result<FlushOutcome, FlushError>>,
}

struct FlushJob {
    pod_id: StoragePodId,
    actions: Vec<FlushAction>,
    submitted_at: SystemTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlushStage {
    WalFsync,
    Manifest,
    Superblock,
    Checkpoint,
    Custom,
}

pub struct FlushAction {
    stage: FlushStage,
    label: String,
    work: Box<dyn FlushWork>,
}

impl FlushAction {
    pub fn new<F>(stage: FlushStage, label: impl Into<String>, work: F) -> Self
    where
        F: FnOnce() -> Result<(), FlushActionError> + Send + 'static,
    {
        Self {
            stage,
            label: label.into(),
            work: Box::new(FlushClosure::new(work)),
        }
    }

    pub fn stage(&self) -> FlushStage {
        self.stage
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn wal_fsync<F>(label: impl Into<String>, work: F) -> Self
    where
        F: FnOnce() -> Result<(), FlushActionError> + Send + 'static,
    {
        Self::new(FlushStage::WalFsync, label, work)
    }

    pub fn manifest<F>(label: impl Into<String>, work: F) -> Self
    where
        F: FnOnce() -> Result<(), FlushActionError> + Send + 'static,
    {
        Self::new(FlushStage::Manifest, label, work)
    }

    pub fn superblock<F>(label: impl Into<String>, work: F) -> Self
    where
        F: FnOnce() -> Result<(), FlushActionError> + Send + 'static,
    {
        Self::new(FlushStage::Superblock, label, work)
    }

    pub fn checkpoint<F>(label: impl Into<String>, work: F) -> Self
    where
        F: FnOnce() -> Result<(), FlushActionError> + Send + 'static,
    {
        Self::new(FlushStage::Checkpoint, label, work)
    }

    pub fn custom<F>(label: impl Into<String>, work: F) -> Self
    where
        F: FnOnce() -> Result<(), FlushActionError> + Send + 'static,
    {
        Self::new(FlushStage::Custom, label, work)
    }
}

trait FlushWork: Send {
    fn run(self: Box<Self>) -> Result<(), FlushActionError>;
}

struct FlushClosure<F>
where
    F: FnOnce() -> Result<(), FlushActionError> + Send + 'static,
{
    func: Option<F>,
}

impl<F> FlushClosure<F>
where
    F: FnOnce() -> Result<(), FlushActionError> + Send + 'static,
{
    fn new(func: F) -> Self {
        Self { func: Some(func) }
    }
}

impl<F> FlushWork for FlushClosure<F>
where
    F: FnOnce() -> Result<(), FlushActionError> + Send + 'static,
{
    fn run(mut self: Box<Self>) -> Result<(), FlushActionError> {
        let func = self.func.take().expect("flush closure already taken");
        func()
    }
}

fn execute_batch(job: FlushJob) -> Result<FlushOutcome, FlushError> {
    let mut results = Vec::with_capacity(job.actions.len());
    let mut success = true;
    for action in job.actions {
        let start = SystemTime::now();
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| action.work.run()))
            .map_err(|_| FlushActionError::panic());
        let result = match result {
            Ok(inner) => inner,
            Err(err) => Err(err),
        };
        let end = SystemTime::now();
        if result.is_err() {
            success = false;
        }
        results.push(FlushStageResult {
            stage: action.stage,
            label: action.label,
            duration: end
                .duration_since(start)
                .unwrap_or_else(|_| Duration::from_secs(0)),
            result,
        });
        if !success {
            break;
        }
    }
    Ok(FlushOutcome {
        pod_id: job.pod_id,
        submitted_at: job.submitted_at,
        completed_at: SystemTime::now(),
        stages: results,
    })
}

#[derive(Debug, Clone)]
pub struct FlushOutcome {
    pub pod_id: StoragePodId,
    pub submitted_at: SystemTime,
    pub completed_at: SystemTime,
    pub stages: Vec<FlushStageResult>,
}

impl FlushOutcome {
    pub fn is_success(&self) -> bool {
        self.stages.iter().all(|s| s.result.is_ok())
    }

    pub fn has_failure(&self) -> bool {
        self.stages.iter().any(|s| s.result.is_err())
    }

    pub fn failure(&self) -> Option<&FlushStageResult> {
        self.stages.iter().find(|s| s.result.is_err())
    }
}

#[derive(Debug, Clone)]
pub struct FlushStageResult {
    pub stage: FlushStage,
    pub label: String,
    pub duration: Duration,
    pub result: Result<(), FlushActionError>,
}

#[derive(Debug, Clone)]
pub struct FlushActionError {
    message: String,
}

impl FlushActionError {
    pub fn from_io(err: io::Error) -> Self {
        Self::new(err.to_string())
    }

    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }

    fn panic() -> Self {
        Self {
            message: "flush task panicked".to_string(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Error)]
pub enum FlushScheduleError {
    #[error("flush batch must contain at least one action")]
    EmptyBatch,
    #[error("flush controller is shutting down")]
    ControllerClosed,
}

#[derive(Debug, Error)]
pub enum FlushError {
    #[error("flush controller is shutting down")]
    ControllerShutdown,
    #[error("flush worker panicked: {0}")]
    WorkerPanic(String),
}

#[derive(Debug)]
pub struct FlushTicket {
    rx: oneshot::Receiver<Result<FlushOutcome, FlushError>>,
}

impl FlushTicket {
    pub async fn wait(self) -> Result<FlushOutcome, FlushError> {
        match self.rx.await {
            Ok(res) => res,
            Err(_) => Err(FlushError::ControllerShutdown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn controller_limits_parallelism() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("runtime");
        let controller = FlushController::new(
            runtime.handle().clone(),
            FlushControllerConfig {
                queue_depth: 8,
                max_parallel: 2,
            },
        );
        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        runtime.block_on(async {
            let pod = StoragePodId::from("controller-parallel");
            let mut tickets = Vec::new();
            for idx in 0..6 {
                let active = active.clone();
                let max_seen = max_seen.clone();
                let action = FlushAction::wal_fsync(format!("fsync-{idx}"), move || {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(current, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(20));
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                });
                let ticket = controller.submit(pod.clone(), vec![action]).await.unwrap();
                tickets.push(ticket);
            }
            for ticket in tickets {
                let outcome = ticket.wait().await.expect("ticket");
                assert!(outcome.is_success());
            }
        });
        assert!(controller.shutdown(Duration::from_secs(1)));
        runtime.shutdown_timeout(Duration::from_secs(1));
        assert!(max_seen.load(Ordering::SeqCst) <= 2);
    }

    #[test]
    fn controller_propagates_failure_and_metrics() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        let controller = FlushController::new(
            runtime.handle().clone(),
            FlushControllerConfig {
                queue_depth: 4,
                max_parallel: 1,
            },
        );
        runtime.block_on(async {
            let pod = StoragePodId::from("controller-failure");
            let actions = vec![
                FlushAction::wal_fsync("fsync-ok", || Ok(())),
                FlushAction::manifest("manifest-fail", || Err(FlushActionError::new("boom"))),
                FlushAction::superblock("skipped", || Ok(())),
            ];
            let ticket = controller.submit(pod.clone(), actions).await.unwrap();
            let outcome = ticket.wait().await.expect("ticket");
            assert!(outcome.has_failure());
            let failure = outcome.failure().expect("failure detail");
            assert_eq!(failure.label, "manifest-fail");
        });
        let snapshot = controller.snapshot();
        assert_eq!(snapshot.failed, 1);
        assert!(controller.shutdown(Duration::from_secs(1)));
        runtime.shutdown_timeout(Duration::from_secs(1));
    }
}

#[derive(Debug, Clone)]
pub struct FlushControllerSnapshot {
    pub pending: u64,
    pub inflight: u64,
    pub completed: u64,
    pub failed: u64,
    pub timestamp: SystemTime,
}

impl Default for FlushControllerSnapshot {
    fn default() -> Self {
        Self {
            pending: 0,
            inflight: 0,
            completed: 0,
            failed: 0,
            timestamp: SystemTime::now(),
        }
    }
}

impl From<io::Error> for FlushActionError {
    fn from(value: io::Error) -> Self {
        FlushActionError::from_io(value)
    }
}
