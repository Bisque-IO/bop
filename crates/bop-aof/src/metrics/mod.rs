//! Metrics and observability for the tiered AOF system.
//!
//! Provides comprehensive performance monitoring, debugging insights,
//! and operational visibility across all system components.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │                 Metrics System              │
//! ├─────────────────────────────────────────────┤
//! │ ┌─────────────────┐  ┌─────────────────────┐│
//! │ │   Admission     │  │ Manifest Replay     ││
//! │ │   Control       │  │    Metrics          ││
//! │ │   Metrics       │  │                     ││
//! │ └─────────────────┘  └─────────────────────┘│
//! └─────────────────────────────────────────────┘
//! ```
//!
//! ## Key Components
//!
//! - **Admission Control**: Backpressure and flow control metrics
//! - **Manifest Replay**: Recovery and bootstrap performance tracking
//! - **System-wide**: Cross-cutting observability and health indicators
//!
//! ## Design Principles
//!
//! - **Low Overhead**: Atomic counters and lock-free data structures
//! - **Snapshot Based**: Point-in-time consistent views for monitoring
//! - **Thread Safe**: Concurrent updates from multiple components
//! - **Actionable**: Metrics directly inform operational decisions
//!
//! ## Usage Patterns
//!
//! ```ignore
//! // Component updates metrics atomically
//! metrics.admission.record_backpressure(BackpressureKind::Tier0Full);
//!
//! // Monitoring system takes snapshots
//! let snapshot = metrics.admission.snapshot();
//! monitor_backpressure_rate(snapshot.backpressure_events);
//! ```

pub mod admission;
pub mod manifest_replay;

pub use admission::{AdmissionMetrics, AdmissionMetricsSnapshot};
