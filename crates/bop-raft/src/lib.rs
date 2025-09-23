//! Idiomatic Rust wrappers for the BOP Raft consensus implementation.
//!
//! The crate exposes safe, ergonomic Rust bindings for the NuRaft C++ library
//! wrapped by BOP. The API is being modularised; Phase 2 focuses on wiring the
//! server- and async-facing FFI surfaces while the higher-level ergonomics mature.

pub mod async_result;
pub mod buffer;
pub(crate) mod callbacks;
pub mod config;
pub mod error;
pub mod log_entry;
pub mod metrics;
pub mod observability;
pub mod server;
pub mod snapshot;
pub mod state;
pub mod storage;
pub mod traits;
pub mod types;

#[cfg(feature = "mdbx")]
pub mod mdbx;

#[cfg(feature = "mdbx")]
pub use mdbx::{MdbxLogStore, MdbxOptions, MdbxStateManager, MdbxStorage};

pub use async_result::{AsyncBool, AsyncBuffer, AsyncU64};
pub use buffer::Buffer;
pub use config::{
    ClusterConfig, ClusterMembershipSnapshot, MembershipChangeSet, MembershipDelta,
    MembershipEntry, RaftParams, ServerConfig, ServerConfigView,
};
pub use error::{RaftError, RaftResult};
pub use log_entry::{LogEntry, LogEntryRecord, LogEntryView};
pub use metrics::{Counter, Gauge, Histogram, MetricsSnapshot, ServerMetrics};
#[cfg(feature = "tracing")]
pub use observability::TracingObserver;
#[cfg(feature = "serde")]
pub use observability::{JsonMetricsExporter, metric_descriptor_catalog_json};
pub use observability::{
    MetricDescriptor, MetricKind, ObservabilityError, ObservabilityResult, PrometheusExporter,
    metric_descriptor_by_rust_name, metric_descriptors,
};
pub use server::{AsioService, RaftServer, RaftServerBuilder};
pub use snapshot::Snapshot;
pub use state::{PeerInfo, ServerState};
pub use storage::{LogStoreBuild, StateManagerBuild, StorageBackendKind, StorageComponents};
pub use traits::{LogStoreInterface, Logger, ServerCallbacks, StateMachine, StateManagerInterface};
pub use types::{
    CallbackAction, CallbackContext, CallbackParam, CallbackType, DataCenterId, LogEntryType,
    LogIndex, LogLevel, PrioritySetResult, ServerId, Term,
};

#[cfg(test)]
mod tests;
