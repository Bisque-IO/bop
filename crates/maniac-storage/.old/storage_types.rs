use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Identifier assigned to a storage pod within a fleet.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StoragePodId(Arc<str>);

impl StoragePodId {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        assert!(
            !id.trim().is_empty(),
            "storage pod identifiers must be non-empty"
        );
        Self(Arc::from(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for StoragePodId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("StoragePodId").field(&self.0).finish()
    }
}

impl fmt::Display for StoragePodId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for StoragePodId {
    fn from(value: &str) -> Self {
        Self::new(value.to_string())
    }
}

impl From<String> for StoragePodId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl Serialize for StoragePodId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for StoragePodId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::new(value))
    }
}

/// High level status describing a storage pod's lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PodStatus {
    Unknown,
    Initializing,
    Recovering,
    Ready,
    Degraded,
    Failed,
    ShuttingDown,
    Stopped,
}

impl Default for PodStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Snapshot of pod health that the fleet can surface to callers and metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePodHealthSnapshot {
    pub status: PodStatus,
    pub last_error: Option<String>,
    pub flush_failures: u64,
    pub last_success_at: Option<u64>,
    pub last_update_at: u64,
}

impl StoragePodHealthSnapshot {
    pub fn new(status: PodStatus) -> Self {
        let now = unix_timestamp(SystemTime::now());
        Self {
            status,
            last_error: None,
            flush_failures: 0,
            last_success_at: None,
            last_update_at: now,
        }
    }

    pub fn record_success(&mut self, timestamp: SystemTime) {
        self.last_success_at = Some(unix_timestamp(timestamp));
        self.last_update_at = unix_timestamp(timestamp);
        if !matches!(
            self.status,
            PodStatus::ShuttingDown | PodStatus::Stopped | PodStatus::Ready
        ) {
            self.status = PodStatus::Ready;
        }
        if matches!(self.status, PodStatus::Ready) {
            self.last_error = None;
        }
    }

    pub fn record_failure(&mut self, status: PodStatus, error: impl Into<String>) {
        debug_assert!(matches!(status, PodStatus::Degraded | PodStatus::Failed));
        self.status = status;
        self.last_error = Some(error.into());
        self.flush_failures = self.flush_failures.saturating_add(1);
        self.last_update_at = unix_timestamp(SystemTime::now());
    }

    pub fn set_status(&mut self, status: PodStatus) {
        self.status = status;
        self.last_update_at = unix_timestamp(SystemTime::now());
    }
}

pub fn unix_timestamp(ts: SystemTime) -> u64 {
    ts.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
