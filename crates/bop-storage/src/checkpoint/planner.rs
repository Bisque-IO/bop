//! Checkpoint planner implementation.
//!
//! This module implements T1 from the checkpointing plan: analyzing dirty pages
//! and WAL state to produce executable checkpoint plans.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::manifest::{ChunkId, DbId, Generation};

/// Represents a sealed WAL segment ready for checkpointing.
///
/// # T1b: WAL Segment Input
///
/// This type is provided by the checkpoint trigger (from manifest query or
/// WAL manager) to describe sealed segments that should be converted to chunks.
#[derive(Debug, Clone)]
pub struct SealedWalSegment {
    /// Unique segment identifier.
    pub segment_id: u64,

    /// Starting LSN for this segment (inclusive).
    pub start_lsn: u64,

    /// Ending LSN for this segment (exclusive).
    pub end_lsn: u64,

    /// Size of the segment file in bytes.
    pub size_bytes: u64,

    /// Local path to the segment file (for executor staging).
    pub local_path: String,
}

/// Represents a dirty page in the PageCache that needs checkpointing.
///
/// # T1c: libsql Dirty Page Input
///
/// This type is provided by the PageCache to describe pages that have been
/// modified and need to be included in the next checkpoint.
#[derive(Debug, Clone)]
pub struct DirtyPage {
    /// Page number within the database.
    pub page_no: u32,

    /// Chunk ID this page belongs to (if known).
    ///
    /// For existing pages, this identifies which chunk they're part of.
    /// For new pages, this will be assigned by the planner.
    pub chunk_id: Option<ChunkId>,

    /// Size of the page in bytes (typically 4KB or 16KB).
    pub size_bytes: u64,

    /// Generation of the last checkpoint that included this page.
    ///
    /// Used to determine if we should emit a delta or rewrite the chunk.
    pub last_checkpointed_generation: Option<Generation>,
}

/// Configuration for delta vs full chunk decisions.
///
/// # T1c: Delta Threshold Logic
///
/// These thresholds control when the planner emits deltas vs full rewrites.
#[derive(Debug, Clone)]
pub struct DeltaConfig {
    /// Maximum size for a delta chunk (default: 10 MiB).
    ///
    /// If dirty pages in a chunk exceed this, emit a full rewrite.
    pub max_delta_size_bytes: u64,

    /// Maximum number of deltas per chunk (default: 3).
    ///
    /// If a chunk already has this many deltas, emit a full rewrite to coalesce.
    pub max_deltas_per_chunk: u32,

    /// Chunk size (default: 64 MiB).
    ///
    /// Used to calculate whether dirty pages exceed the rewrite threshold.
    pub chunk_size_bytes: u64,
}

impl Default for DeltaConfig {
    fn default() -> Self {
        Self {
            max_delta_size_bytes: 10 * 1024 * 1024, // 10 MiB
            max_deltas_per_chunk: 3,
            chunk_size_bytes: 64 * 1024 * 1024, // 64 MiB
        }
    }
}

/// Metadata about an existing chunk (from manifest query).
///
/// # T1c: Chunk Metadata Input
///
/// Used by the libsql planner to determine delta counts and decide
/// whether to emit a delta or full rewrite.
#[derive(Debug, Clone)]
pub struct ChunkMetadata {
    /// Chunk ID.
    pub chunk_id: ChunkId,

    /// Current generation of this chunk.
    pub generation: Generation,

    /// Number of deltas currently associated with this chunk.
    pub delta_count: u32,

    /// Total size of all deltas for this chunk.
    pub total_delta_size_bytes: u64,
}

/// Errors that can occur during checkpoint planning.
#[derive(Debug, thiserror::Error)]
pub enum PlannerError {
    /// Manifest query failed.
    #[error("manifest error: {0}")]
    Manifest(String),

    /// PageCache query failed.
    #[error("page cache error: {0}")]
    PageCache(String),

    /// WAL metadata unavailable.
    #[error("wal error: {0}")]
    Wal(String),

    /// Invalid planner state.
    #[error("invalid state: {0}")]
    InvalidState(String),
}

/// Context required for checkpoint planning.
///
/// Captures the current state of the manifest, PageCache, and WAL
/// at the time planning begins.
#[derive(Debug, Clone)]
pub struct PlannerContext {
    /// Database being checkpointed.
    pub db_id: DbId,

    /// Current checkpoint generation watermark.
    pub current_generation: Generation,

    /// WAL low-water mark (oldest LSN not yet checkpointed).
    pub wal_low_lsn: u64,

    /// WAL high-water mark (latest durable LSN).
    pub wal_high_lsn: u64,

    /// Workload type detection.
    pub workload_type: WorkloadType,
}

/// Detected workload type for planning strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkloadType {
    /// Append-only workload (records appended sequentially).
    AppendOnly,

    /// libsql-style workload (random page updates).
    Libsql,
}

/// A plan for executing a checkpoint.
///
/// Emitted by the planner and consumed by the executor. Contains all
/// information needed to stage, upload, and publish a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointPlan {
    /// Database ID for this checkpoint.
    pub db_id: DbId,

    /// Target generation for this checkpoint.
    pub target_generation: Generation,

    /// Actions to perform (one per chunk).
    pub actions: Vec<ChunkAction>,

    /// Estimated total size in bytes.
    pub estimated_size_bytes: u64,

    /// WAL range covered by this checkpoint.
    pub wal_range: (u64, u64),
}

/// Action to perform for a single chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkAction {
    /// Seal an append-only WAL segment as a new base chunk.
    SealAsBase {
        /// Chunk ID to assign.
        chunk_id: ChunkId,

        /// WAL segment ID being sealed.
        segment_id: u64,

        /// Size in bytes.
        size_bytes: u64,

        /// Local filesystem path to the sealed WAL segment.
        segment_path: PathBuf,
    },

    /// Create a new base chunk from dirty pages.
    CreateBase {
        /// Chunk ID to assign.
        chunk_id: ChunkId,

        /// Page numbers included in this chunk.
        pages: Vec<u32>,

        /// Estimated size in bytes.
        estimated_size_bytes: u64,
    },

    /// Create a delta chunk for an existing base.
    CreateDelta {
        /// Base chunk ID.
        base_chunk_id: ChunkId,

        /// Delta generation number.
        delta_generation: Generation,

        /// Sequential identifier for this delta within the chunk's chain.
        delta_id: u16,

        /// Page numbers modified in this delta.
        pages: Vec<u32>,

        /// Estimated size in bytes.
        estimated_size_bytes: u64,
    },

    /// Rewrite a full chunk (coalescing deltas or high churn).
    RewriteBase {
        /// Chunk ID being rewritten.
        chunk_id: ChunkId,

        /// Previous generation being superseded.
        prev_generation: Generation,

        /// All pages in the rewritten chunk.
        pages: Vec<u32>,

        /// Estimated size in bytes.
        estimated_size_bytes: u64,
    },
}

impl ChunkAction {
    /// Returns the target chunk ID for this action.
    pub fn chunk_id(&self) -> ChunkId {
        match self {
            ChunkAction::SealAsBase { chunk_id, .. } => *chunk_id,
            ChunkAction::CreateBase { chunk_id, .. } => *chunk_id,
            ChunkAction::CreateDelta { base_chunk_id, .. } => *base_chunk_id,
            ChunkAction::RewriteBase { chunk_id, .. } => *chunk_id,
        }
    }

    /// Returns the estimated size in bytes for this action.
    pub fn estimated_size(&self) -> u64 {
        match self {
            ChunkAction::SealAsBase { size_bytes, .. } => *size_bytes,
            ChunkAction::CreateBase {
                estimated_size_bytes,
                ..
            } => *estimated_size_bytes,
            ChunkAction::CreateDelta {
                estimated_size_bytes,
                ..
            } => *estimated_size_bytes,
            ChunkAction::RewriteBase {
                estimated_size_bytes,
                ..
            } => *estimated_size_bytes,
        }
    }
}

/// Checkpoint planner.
///
/// Analyzes the current system state and produces executable checkpoint plans.
pub struct CheckpointPlanner;

impl CheckpointPlanner {
    /// Creates a new checkpoint planner.
    pub fn new() -> Self {
        Self
    }

    /// Plans a checkpoint for the given context.
    ///
    /// # Returns
    ///
    /// A `CheckpointPlan` describing the work to be done, or an error if
    /// planning fails.
    ///
    /// # Planning Strategy
    ///
    /// - **Append-only**: Seals the current WAL segment as a new base chunk
    /// - **libsql**: Diffs dirty pages, emits deltas for small changes or
    ///   full rewrites for large changes
    pub fn plan(&self, context: PlannerContext) -> Result<CheckpointPlan, PlannerError> {
        match context.workload_type {
            WorkloadType::AppendOnly => self.plan_append_only(context),
            WorkloadType::Libsql => self.plan_libsql(context),
        }
    }

    /// Plans a checkpoint for append-only workloads.
    ///
    /// # T1b: Append-Only Planner
    ///
    /// For append-only workloads, WAL segments map 1:1 to chunks. Each sealed
    /// segment becomes a base chunk with no diffing required.
    ///
    /// In production, this would query the manifest's `wal_catalog` to find
    /// sealed segments. Use `plan_append_only_with_segments` for the full
    /// implementation with explicit segment lists.
    fn plan_append_only(&self, context: PlannerContext) -> Result<CheckpointPlan, PlannerError> {
        // TODO: Query manifest wal_catalog for sealed segments in range
        // For now, return empty plan as we don't have segment metadata

        Ok(CheckpointPlan {
            db_id: context.db_id,
            target_generation: context.current_generation + 1,
            actions: vec![],
            estimated_size_bytes: 0,
            wal_range: (context.wal_low_lsn, context.wal_high_lsn),
        })
    }

    /// Plans a checkpoint for append-only workloads with explicit segment list.
    ///
    /// # T1b: Real Implementation
    ///
    /// This is the production implementation that takes sealed WAL segments
    /// and produces `SealAsBase` actions for the executor.
    ///
    /// # Parameters
    ///
    /// - `segments`: Sealed WAL segments eligible for checkpointing
    ///
    /// # Strategy
    ///
    /// 1. Filter segments to those overlapping [low_lsn, high_lsn)
    /// 2. For each segment, create a `SealAsBase` action
    /// 3. Assign monotonic chunk IDs
    /// 4. Calculate total estimated size
    pub fn plan_append_only_with_segments(
        &self,
        context: PlannerContext,
        segments: Vec<SealedWalSegment>,
    ) -> Result<CheckpointPlan, PlannerError> {
        let target_generation = context.current_generation + 1;
        let mut actions = Vec::new();
        let mut estimated_size_bytes = 0u64;

        for segment in segments {
            // Validate segment is within WAL range
            if segment.end_lsn <= context.wal_low_lsn {
                continue; // Already checkpointed
            }
            if segment.start_lsn >= context.wal_high_lsn {
                continue; // Not yet eligible
            }

            // Assign chunk ID (in production, this would come from manifest)
            let chunk_id = segment.segment_id as ChunkId;

            actions.push(ChunkAction::SealAsBase {
                chunk_id,
                segment_id: segment.segment_id,
                size_bytes: segment.size_bytes,
                segment_path: PathBuf::from(&segment.local_path),
            });

            estimated_size_bytes += segment.size_bytes;
        }

        Ok(CheckpointPlan {
            db_id: context.db_id,
            target_generation,
            actions,
            estimated_size_bytes,
            wal_range: (context.wal_low_lsn, context.wal_high_lsn),
        })
    }

    fn plan_libsql(&self, context: PlannerContext) -> Result<CheckpointPlan, PlannerError> {
        // TODO: Query PageCache for dirty pages
        // For now, return empty plan as we don't have PageCache integration

        Ok(CheckpointPlan {
            db_id: context.db_id,
            target_generation: context.current_generation + 1,
            actions: vec![],
            estimated_size_bytes: 0,
            wal_range: (context.wal_low_lsn, context.wal_high_lsn),
        })
    }

    /// Plans a checkpoint for libsql workloads with explicit dirty pages.
    ///
    /// # T1c: Real Implementation
    ///
    /// This is the production implementation that takes dirty pages from
    /// PageCache and produces delta or full rewrite actions.
    ///
    /// # Parameters
    ///
    /// - `dirty_pages`: List of modified pages from PageCache
    /// - `chunk_metadata`: Metadata about existing chunks (for delta decisions)
    /// - `config`: Delta threshold configuration
    ///
    /// # Strategy
    ///
    /// 1. Group dirty pages by chunk ID
    /// 2. For each chunk, calculate total dirty page size
    /// 3. Decide delta vs full rewrite based on:
    ///    - Delta size < max_delta_size_bytes
    ///    - Delta count < max_deltas_per_chunk
    ///    - Dirty pages < 50% of chunk size
    /// 4. Generate appropriate ChunkAction (CreateDelta or RewriteBase)
    pub fn plan_libsql_with_pages(
        &self,
        context: PlannerContext,
        dirty_pages: Vec<DirtyPage>,
        chunk_metadata: &[ChunkMetadata],
        config: &DeltaConfig,
    ) -> Result<CheckpointPlan, PlannerError> {
        use std::collections::HashMap;

        let target_generation = context.current_generation + 1;
        let mut actions = Vec::new();
        let mut estimated_size_bytes = 0u64;

        // Build metadata lookup
        let metadata_map: HashMap<ChunkId, &ChunkMetadata> =
            chunk_metadata.iter().map(|m| (m.chunk_id, m)).collect();

        // Group pages by chunk
        let mut chunks_to_pages: HashMap<ChunkId, Vec<&DirtyPage>> = HashMap::new();
        for page in &dirty_pages {
            if let Some(chunk_id) = page.chunk_id {
                chunks_to_pages.entry(chunk_id).or_default().push(page);
            }
        }

        // Process each chunk
        for (chunk_id, pages) in chunks_to_pages {
            let total_dirty_size: u64 = pages.iter().map(|p| p.size_bytes).sum();
            let page_numbers: Vec<u32> = pages.iter().map(|p| p.page_no).collect();

            // Get existing chunk metadata
            let metadata = metadata_map.get(&chunk_id);

            // Decide: Delta or Full Rewrite?
            let should_emit_delta = if let Some(meta) = metadata {
                // Check delta thresholds
                total_dirty_size <= config.max_delta_size_bytes
                    && meta.delta_count < config.max_deltas_per_chunk
                    && total_dirty_size < config.chunk_size_bytes / 2
            } else {
                // New chunk - emit as base
                false
            };

            if should_emit_delta {
                // Emit delta
                let meta = metadata.unwrap(); // Safe because we checked above
                let next_delta_id = meta.delta_count.saturating_add(1).min(u16::MAX as u32) as u16;

                actions.push(ChunkAction::CreateDelta {
                    base_chunk_id: chunk_id,
                    delta_generation: target_generation,
                    delta_id: next_delta_id,
                    pages: page_numbers,
                    estimated_size_bytes: total_dirty_size,
                });
            } else {
                // Emit full rewrite
                let prev_generation = metadata.map(|m| m.generation).unwrap_or(0);
                actions.push(ChunkAction::RewriteBase {
                    chunk_id,
                    prev_generation,
                    pages: page_numbers,
                    estimated_size_bytes: config.chunk_size_bytes,
                });
                // Full chunk size for rewrites
                estimated_size_bytes += config.chunk_size_bytes;
                continue;
            }

            estimated_size_bytes += total_dirty_size;
        }

        Ok(CheckpointPlan {
            db_id: context.db_id,
            target_generation,
            actions,
            estimated_size_bytes,
            wal_range: (context.wal_low_lsn, context.wal_high_lsn),
        })
    }
}

impl Default for CheckpointPlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_creates_empty_plan_for_append_only() {
        let planner = CheckpointPlanner::new();
        let context = PlannerContext {
            db_id: 1,
            current_generation: 10,
            wal_low_lsn: 0,
            wal_high_lsn: 1024,
            workload_type: WorkloadType::AppendOnly,
        };

        let plan = planner.plan(context).unwrap();
        assert_eq!(plan.db_id, 1);
        assert_eq!(plan.target_generation, 11);
        assert_eq!(plan.wal_range, (0, 1024));
    }

    #[test]
    fn planner_creates_empty_plan_for_libsql() {
        let planner = CheckpointPlanner::new();
        let context = PlannerContext {
            db_id: 2,
            current_generation: 5,
            wal_low_lsn: 100,
            wal_high_lsn: 500,
            workload_type: WorkloadType::Libsql,
        };

        let plan = planner.plan(context).unwrap();
        assert_eq!(plan.db_id, 2);
        assert_eq!(plan.target_generation, 6);
        assert_eq!(plan.wal_range, (100, 500));
    }

    #[test]
    fn chunk_action_accessors() {
        let seal = ChunkAction::SealAsBase {
            chunk_id: 42,
            segment_id: 1,
            size_bytes: 1024,
            segment_path: PathBuf::from("/tmp/segment-42.wal"),
        };
        assert_eq!(seal.chunk_id(), 42);
        assert_eq!(seal.estimated_size(), 1024);

        let delta = ChunkAction::CreateDelta {
            base_chunk_id: 99,
            delta_generation: 3,
            delta_id: 7,
            pages: vec![1, 2, 3],
            estimated_size_bytes: 512,
        };
        assert_eq!(delta.chunk_id(), 99);
        assert_eq!(delta.estimated_size(), 512);
    }

    #[test]
    fn append_only_planner_seals_segments() {
        let planner = CheckpointPlanner::new();
        let context = PlannerContext {
            db_id: 1,
            current_generation: 5,
            wal_low_lsn: 0,
            wal_high_lsn: 3000,
            workload_type: WorkloadType::AppendOnly,
        };

        let segments = vec![
            SealedWalSegment {
                segment_id: 1,
                start_lsn: 0,
                end_lsn: 1000,
                size_bytes: 64 * 1024 * 1024,
                local_path: "segment-1.wal".into(),
            },
            SealedWalSegment {
                segment_id: 2,
                start_lsn: 1000,
                end_lsn: 2000,
                size_bytes: 64 * 1024 * 1024,
                local_path: "segment-2.wal".into(),
            },
            SealedWalSegment {
                segment_id: 3,
                start_lsn: 2000,
                end_lsn: 3000,
                size_bytes: 32 * 1024 * 1024,
                local_path: "segment-3.wal".into(),
            },
        ];

        let plan = planner
            .plan_append_only_with_segments(context, segments)
            .unwrap();

        assert_eq!(plan.db_id, 1);
        assert_eq!(plan.target_generation, 6);
        assert_eq!(plan.actions.len(), 3);
        assert_eq!(plan.estimated_size_bytes, 160 * 1024 * 1024);
        assert_eq!(plan.wal_range, (0, 3000));

        // Verify actions
        match &plan.actions[0] {
            ChunkAction::SealAsBase {
                chunk_id,
                segment_id,
                size_bytes,
                segment_path,
            } => {
                assert_eq!(*chunk_id, 1);
                assert_eq!(*segment_id, 1);
                assert_eq!(*size_bytes, 64 * 1024 * 1024);
                assert_eq!(segment_path, &PathBuf::from("segment-1.wal"));
            }
            _ => panic!("expected SealAsBase"),
        }
    }

    #[test]
    fn append_only_planner_filters_out_of_range_segments() {
        let planner = CheckpointPlanner::new();
        let context = PlannerContext {
            db_id: 1,
            current_generation: 10,
            wal_low_lsn: 1000,
            wal_high_lsn: 2500,
            workload_type: WorkloadType::AppendOnly,
        };

        let segments = vec![
            // Already checkpointed (end_lsn <= low_lsn)
            SealedWalSegment {
                segment_id: 1,
                start_lsn: 0,
                end_lsn: 500,
                size_bytes: 64 * 1024 * 1024,
                local_path: "segment-1.wal".into(),
            },
            // Eligible
            SealedWalSegment {
                segment_id: 2,
                start_lsn: 1000,
                end_lsn: 2000,
                size_bytes: 64 * 1024 * 1024,
                local_path: "segment-2.wal".into(),
            },
            // Not yet eligible (start_lsn >= high_lsn)
            SealedWalSegment {
                segment_id: 3,
                start_lsn: 3000,
                end_lsn: 4000,
                size_bytes: 32 * 1024 * 1024,
                local_path: "segment-3.wal".into(),
            },
        ];

        let plan = planner
            .plan_append_only_with_segments(context, segments)
            .unwrap();

        // Only segment 2 should be included
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.estimated_size_bytes, 64 * 1024 * 1024);

        match &plan.actions[0] {
            ChunkAction::SealAsBase { segment_id, .. } => {
                assert_eq!(*segment_id, 2);
            }
            _ => panic!("expected SealAsBase"),
        }
    }

    #[test]
    fn append_only_planner_handles_empty_segment_list() {
        let planner = CheckpointPlanner::new();
        let context = PlannerContext {
            db_id: 1,
            current_generation: 5,
            wal_low_lsn: 0,
            wal_high_lsn: 1000,
            workload_type: WorkloadType::AppendOnly,
        };

        let plan = planner
            .plan_append_only_with_segments(context, vec![])
            .unwrap();

        assert_eq!(plan.actions.len(), 0);
        assert_eq!(plan.estimated_size_bytes, 0);
        assert_eq!(plan.target_generation, 6);
    }

    // T1c: libsql planner tests
    #[test]
    fn libsql_planner_emits_delta_for_small_changes() {
        let planner = CheckpointPlanner::new();
        let context = PlannerContext {
            db_id: 1,
            current_generation: 10,
            wal_low_lsn: 0,
            wal_high_lsn: 1000,
            workload_type: WorkloadType::Libsql,
        };

        let dirty_pages = vec![
            DirtyPage {
                page_no: 100,
                chunk_id: Some(42),
                size_bytes: 4096,
                last_checkpointed_generation: Some(9),
            },
            DirtyPage {
                page_no: 101,
                chunk_id: Some(42),
                size_bytes: 4096,
                last_checkpointed_generation: Some(9),
            },
        ];

        let metadata = vec![ChunkMetadata {
            chunk_id: 42,
            generation: 10,
            delta_count: 1,
            total_delta_size_bytes: 8192,
        }];

        let config = DeltaConfig::default();
        let plan = planner
            .plan_libsql_with_pages(context, dirty_pages, &metadata, &config)
            .unwrap();

        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.target_generation, 11);

        // Should emit delta
        match &plan.actions[0] {
            ChunkAction::CreateDelta {
                base_chunk_id,
                delta_generation,
                delta_id,
                pages,
                estimated_size_bytes,
            } => {
                assert_eq!(*base_chunk_id, 42);
                assert_eq!(*delta_generation, 11);
                assert_eq!(pages.len(), 2);
                assert_eq!(*estimated_size_bytes, 8192);
                assert_eq!(*delta_id, 2);
            }
            _ => panic!("expected CreateDelta"),
        }
    }

    #[test]
    fn libsql_planner_emits_rewrite_when_delta_count_exceeded() {
        let planner = CheckpointPlanner::new();
        let context = PlannerContext {
            db_id: 1,
            current_generation: 10,
            wal_low_lsn: 0,
            wal_high_lsn: 1000,
            workload_type: WorkloadType::Libsql,
        };

        let dirty_pages = vec![DirtyPage {
            page_no: 100,
            chunk_id: Some(42),
            size_bytes: 4096,
            last_checkpointed_generation: Some(9),
        }];

        // Chunk already has 3 deltas - should trigger rewrite
        let metadata = vec![ChunkMetadata {
            chunk_id: 42,
            generation: 10,
            delta_count: 3,
            total_delta_size_bytes: 16384,
        }];

        let config = DeltaConfig::default();
        let plan = planner
            .plan_libsql_with_pages(context, dirty_pages, &metadata, &config)
            .unwrap();

        assert_eq!(plan.actions.len(), 1);

        // Should emit full rewrite
        match &plan.actions[0] {
            ChunkAction::RewriteBase {
                chunk_id,
                prev_generation,
                ..
            } => {
                assert_eq!(*chunk_id, 42);
                assert_eq!(*prev_generation, 10);
            }
            _ => panic!("expected RewriteBase"),
        }
    }

    #[test]
    fn libsql_planner_emits_rewrite_when_delta_size_too_large() {
        let planner = CheckpointPlanner::new();
        let context = PlannerContext {
            db_id: 1,
            current_generation: 10,
            wal_low_lsn: 0,
            wal_high_lsn: 1000,
            workload_type: WorkloadType::Libsql,
        };

        // 15 MiB of dirty pages (exceeds 10 MiB default)
        let dirty_pages = vec![DirtyPage {
            page_no: 100,
            chunk_id: Some(42),
            size_bytes: 15 * 1024 * 1024,
            last_checkpointed_generation: Some(9),
        }];

        let metadata = vec![ChunkMetadata {
            chunk_id: 42,
            generation: 10,
            delta_count: 1,
            total_delta_size_bytes: 8192,
        }];

        let config = DeltaConfig::default();
        let plan = planner
            .plan_libsql_with_pages(context, dirty_pages, &metadata, &config)
            .unwrap();

        assert_eq!(plan.actions.len(), 1);

        // Should emit full rewrite
        match &plan.actions[0] {
            ChunkAction::RewriteBase { chunk_id, .. } => {
                assert_eq!(*chunk_id, 42);
            }
            _ => panic!("expected RewriteBase"),
        }
    }

    #[test]
    fn libsql_planner_groups_pages_by_chunk() {
        let planner = CheckpointPlanner::new();
        let context = PlannerContext {
            db_id: 1,
            current_generation: 5,
            wal_low_lsn: 0,
            wal_high_lsn: 1000,
            workload_type: WorkloadType::Libsql,
        };

        let dirty_pages = vec![
            DirtyPage {
                page_no: 100,
                chunk_id: Some(1),
                size_bytes: 4096,
                last_checkpointed_generation: Some(4),
            },
            DirtyPage {
                page_no: 200,
                chunk_id: Some(2),
                size_bytes: 4096,
                last_checkpointed_generation: Some(4),
            },
            DirtyPage {
                page_no: 101,
                chunk_id: Some(1),
                size_bytes: 4096,
                last_checkpointed_generation: Some(4),
            },
        ];

        let metadata = vec![
            ChunkMetadata {
                chunk_id: 1,
                generation: 5,
                delta_count: 0,
                total_delta_size_bytes: 0,
            },
            ChunkMetadata {
                chunk_id: 2,
                generation: 5,
                delta_count: 0,
                total_delta_size_bytes: 0,
            },
        ];

        let config = DeltaConfig::default();
        let plan = planner
            .plan_libsql_with_pages(context, dirty_pages, &metadata, &config)
            .unwrap();

        // Should emit 2 deltas (one per chunk)
        assert_eq!(plan.actions.len(), 2);

        // Find actions for each chunk
        let chunk1_action = plan
            .actions
            .iter()
            .find(|a| a.chunk_id() == 1)
            .expect("chunk 1 action");
        let chunk2_action = plan
            .actions
            .iter()
            .find(|a| a.chunk_id() == 2)
            .expect("chunk 2 action");

        match chunk1_action {
            ChunkAction::CreateDelta { pages, .. } => {
                assert_eq!(pages.len(), 2); // Pages 100 and 101
            }
            _ => panic!("expected CreateDelta for chunk 1"),
        }

        match chunk2_action {
            ChunkAction::CreateDelta { pages, .. } => {
                assert_eq!(pages.len(), 1); // Page 200
            }
            _ => panic!("expected CreateDelta for chunk 2"),
        }
    }
}
