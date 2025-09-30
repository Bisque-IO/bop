//! Checkpointing infrastructure for bop-storage.
//!
//! This module implements the checkpoint pipeline as described in
//! `docs/checkpointing_design.md`. It coordinates the planner, executor,
//! and truncation subsystems to create durable checkpoints and manage
//! retention for append-only and libsql workloads.
//!
//! # Architecture
//!
//! - **CheckpointPlanner**: Analyzes dirty pages and WAL state to produce a
//!   `CheckpointPlan` describing which chunks need creation or updating.
//! - **CheckpointExecutor**: Stages dirty pages to `checkpoint_scratch/`,
//!   uploads to S3, and publishes manifest updates atomically.
//! - **Truncation**: Handles append-only head/tail truncation with lease
//!   coordination to prevent conflicts with in-flight checkpoints.
//!
//! # Key Types
//!
//! - `CheckpointOrchestrator`: End-to-end checkpoint coordination
//! - `CheckpointPlan`: Describes the work for a checkpoint job
//! - `CheckpointJobState`: Tracks progress for resumable execution
//! - `TruncationRequest`: Specifies head or tail truncation boundaries
//! - `LeaseMap`: Coordinates chunk leases between executor and truncation

pub mod delta;
pub mod executor;
pub mod orchestrator;
pub mod planner;
pub mod truncation;

pub use delta::{DeltaBuilder, DeltaHeader, DeltaReader, PageDirEntry};
pub use executor::{CheckpointExecutor, CheckpointJobState, ExecutorError};
pub use orchestrator::{CheckpointOrchestrator, OrchestratorConfig, OrchestratorError};
pub use planner::{
    CheckpointPlan, CheckpointPlanner, ChunkAction, PlannerContext, PlannerError,
    WorkloadType,
};
pub use truncation::{LeaseMap, TruncateDirection, TruncationError, TruncationRequest};
