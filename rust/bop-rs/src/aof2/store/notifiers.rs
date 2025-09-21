use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use parking_lot::Mutex;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::aof2::config::SegmentId;
use crate::aof2::error::{AofError, AofResult, BackpressureKind};

use super::tier0::InstanceId;

const ROLLOVER_PENDING: u8 = 0;
const ROLLOVER_SUCCESS: u8 = 1;
const ROLLOVER_FAILED: u8 = 2;

#[derive(Debug)]
pub struct RolloverSignal {
    state: AtomicU8,
    result: Mutex<Option<Result<(), String>>>,
    notify: Notify,
}

impl RolloverSignal {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(ROLLOVER_PENDING),
            result: Mutex::new(None),
            notify: Notify::new(),
        }
    }

    pub fn complete(&self, result: AofResult<()>) {
        let success = result.is_ok();
        let stored = match result {
            Ok(()) => Ok(()),
            Err(err) => Err(err.to_string()),
        };
        *self.result.lock() = Some(stored);
        let state = if success {
            ROLLOVER_SUCCESS
        } else {
            ROLLOVER_FAILED
        };
        self.state.store(state, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub fn is_ready(&self) -> bool {
        self.state.load(Ordering::Acquire) != ROLLOVER_PENDING
    }

    pub fn result(&self) -> Option<Result<(), String>> {
        self.result.lock().as_ref().cloned()
    }

    pub async fn wait(&self, shutdown: &CancellationToken) -> AofResult<()> {
        loop {
            if let Some(result) = self.result() {
                return result.map_err(AofError::rollover_failed);
            }
            tokio::select! {
                _ = self.notify.notified() => {},
                _ = shutdown.cancelled() => {
                    return Err(AofError::would_block(BackpressureKind::Rollover));
                }
            }
        }
    }
}

#[derive(Debug)]
struct InstanceNotifiers {
    admission: Arc<Notify>,
    rollover: Arc<Notify>,
    signals: Mutex<HashMap<SegmentId, Arc<RolloverSignal>>>,
}

impl InstanceNotifiers {
    fn new() -> Self {
        Self {
            admission: Arc::new(Notify::new()),
            rollover: Arc::new(Notify::new()),
            signals: Mutex::new(HashMap::new()),
        }
    }

    fn insert_signal(&self, segment_id: SegmentId, signal: Arc<RolloverSignal>) {
        self.signals.lock().insert(segment_id, signal);
    }

    fn signal(&self, segment_id: SegmentId) -> Option<Arc<RolloverSignal>> {
        self.signals.lock().get(&segment_id).cloned()
    }

    fn remove_signal(&self, segment_id: &SegmentId) {
        self.signals.lock().remove(segment_id);
    }
}

#[derive(Debug, Default)]
pub struct TieredCoordinatorNotifiers {
    instances: Mutex<HashMap<InstanceId, Arc<InstanceNotifiers>>>,
}

impl TieredCoordinatorNotifiers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_instance(&self, instance_id: InstanceId) {
        let mut instances = self.instances.lock();
        instances
            .entry(instance_id)
            .or_insert_with(|| Arc::new(InstanceNotifiers::new()));
    }

    pub fn unregister_instance(&self, instance_id: InstanceId) {
        self.instances.lock().remove(&instance_id);
    }

    fn ensure_instance(&self, instance_id: InstanceId) -> Arc<InstanceNotifiers> {
        let mut instances = self.instances.lock();
        Arc::clone(
            instances
                .entry(instance_id)
                .or_insert_with(|| Arc::new(InstanceNotifiers::new())),
        )
    }

    fn get_instance(&self, instance_id: InstanceId) -> Option<Arc<InstanceNotifiers>> {
        let instances = self.instances.lock();
        instances.get(&instance_id).cloned()
    }

    pub fn admission(&self, instance_id: InstanceId) -> Arc<Notify> {
        self.ensure_instance(instance_id).admission.clone()
    }

    pub fn notify_admission(&self, instance_id: InstanceId) {
        if let Some(instance) = self.get_instance(instance_id) {
            instance.admission.notify_waiters();
        }
    }

    pub fn register_rollover(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
    ) -> Arc<RolloverSignal> {
        let instance = self.ensure_instance(instance_id);
        let signal = Arc::new(RolloverSignal::new());
        instance.insert_signal(segment_id, Arc::clone(&signal));
        instance.rollover.notify_waiters();
        signal
    }

    pub fn complete_rollover(
        &self,
        instance_id: InstanceId,
        segment_id: SegmentId,
        result: AofResult<()>,
    ) {
        if let Some(instance) = self.get_instance(instance_id) {
            if let Some(signal) = instance.signal(segment_id) {
                signal.complete(result);
                instance.rollover.notify_waiters();
            }
        }
    }

    pub fn remove_rollover(&self, instance_id: InstanceId, segment_id: SegmentId) {
        if let Some(instance) = self.get_instance(instance_id) {
            instance.remove_signal(&segment_id);
        }
    }

    pub fn rollover(&self, instance_id: InstanceId) -> Arc<Notify> {
        self.ensure_instance(instance_id).rollover.clone()
    }

    pub fn notify_rollover(&self, instance_id: InstanceId) {
        if let Some(instance) = self.get_instance(instance_id) {
            instance.rollover.notify_waiters();
        }
    }
}
