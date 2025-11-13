# Checkpoint Engine Split Plan

## Goals
- Eliminate the shared `checkpoint` module in favor of two self-contained engines: AppendOnly and LibSql.
- Allow each engine to evolve independent WAL, staging, and manifest flows without leaking implementation details through shared enums or structs.
- Preserve the existing operational guarantees (idempotent retries, quota enforcement, lease coordination) while simplifying the code path per engine.

## Guiding Principles
- **Isolation first**: engines own their planners, executors, manifest adapters, and WAL metadata. The only shared layer is the job scheduling shell that decides which engine to invoke.
- **Job-centric contracts**: the outer service passes a minimal `CheckpointJobContext` with shared dependencies (manifest handle, remote store, quota guard, lease map, scratch allocator). Engines translate that into their own plan/execution types.
- **Explicit artifacts**: AppendOnly uses sealed WAL segments and base chunks; LibSql uses dirty page batches, delta blobs, and rewrite chunks. No cross-engine enums.
- **Replace, donâ€™t evolve**: the current planner/executor/orchestrator pipeline is removed after the new engines cover parity. Avoid temporary adapters that keep old types alive.

## Architecture Overview
The new layout introduces `crates/bop-storage/src/append_only/` for the append-only pipeline. The legacy `checkpoint/` directory has been removed, and the future LibSql engine will live under `crates/bop-storage/src/libsql/` once it is rebuilt.

The append-only engine exposes its own runner (`AppendOnlyContext` / `run_checkpoint`) and owns planning, execution, manifest publishing, and lease/quota orchestration. No shared checkpoint service layer remains in the crate; future engines will add their own entry points when they land.

### AppendOnly Engine
**Responsibilities**
- Track sealed WAL segments produced by the append-only writer.
- Materialize base chunks by copying segment files into scratch storage and uploading them verbatim.
- Coordinate truncation leases so head/tail truncation only occurs after successful checkpointing.

**Data Flow**
1. Planner gathers the manifest's tail chunk descriptors (LSN window, file path, logical size).
2. Each descriptor becomes a `ChunkPlan { chunk_id, size_bytes, file_path }`.
3. Executor hashes the existing chunk file in place (no scratch copy) and prepares metadata for publication.
4. Manifest adapter upserts the chunk entry and marks residency/local state in a single transaction.

**Crash Recovery**
- Persist progress after each segment: last uploaded chunk, remote blob key, manifest commit flag.
- Re-uploads are idempotent because blob keys are content-addressed by segment hash.

**Truncation Integration**
- Engine exposes `append_only::leases::LeaseMap` reused by truncation jobs.
- Completed chunks notify truncation service to release locks and trigger configured retention policies.

**Implementation Milestones**
1. Create module skeleton with stub planner/executor returning `Plan::empty()`.
2. Move existing append-only logic (segment sealing, truncation) into the module, replacing old types with engine-local equivalents.
3. Wire orchestrator dispatch to call into new engine and delete old append-only planner paths.
4. Add engine-specific tests for segment planning, upload idempotency, and manifest commits.

### LibSql Engine
**Responsibilities**
- Capture dirty pages from the LibSql pager, diff them against existing chunks, and emit deltas or rewrites.
- Manage LibSql WAL (frames, framesets) to maintain MVCC snapshots during checkpoint.
- Publish manifest deltas that express page-level updates with generation tracking.

**Data Flow**
1. Planner queries pager for dirty page batches grouped by chunk.
2. For each batch, compute whether to emit `DeltaChunkPlan` (small updates) or `RewriteChunkPlan` (heavy updates) based on configurable thresholds.
3. Executor pulls page images from the pager/WAL, writes them into scratch artifacts, and uploads them using LibSql-specific blob naming.
4. Manifest adapter writes delta entries, updates chunk generations, and appends change log records.

**Crash Recovery**
- Persist per-chunk progress (pages staged, deltas uploaded, manifest txn commit).
- Support deterministic blob keys by hashing page content and generation.
- Integrate with LibSql checkpoint-safe snapshot to resume diffing after restart.

**WAL Coordination**
- Maintain LibSql WAL cursor independent of append-only segments.
- Provide hooks for background WAL compaction once checkpoint succeeds.

**Implementation Milestones**
1. Define LibSql-specific plan structs and planner skeleton (initially returning empty plan).
2. Port existing LibSql placeholder planning code into the module with concrete types for dirty page batches.
3. Build executor that interfaces with LibSql pager APIs (TBD integration work).
4. Implement manifest commits for delta and rewrite operations and add targeted tests.

## Shared Infrastructure Adjustments
- Replace `WorkloadType` with an engine selector stored alongside tenant metadata. The dispatcher reads this once per job.
- Move `CheckpointJobState` into `services/checkpoint` and refactor it to store only generic fields (job id, db id, engine id, phase). Engine-specific progress is serialized into an opaque payload handled by each module.
- Keep remote store and scratch directory management shared, exposed through a thin `CheckpointRuntime` context passed to engines.
- Telemetry, metrics, and error reporting gather tags like `engine = "append_only" | "libsql"` to retain observability.

## Migration Plan
1. Finish new append-only engine (context, planner, executor, manifest publishing) — **done**.
2. Integrate truncation/quota/lease wiring in the new engine — **done**.
3. Rebuild LibSql checkpointing on top of a fresh module (TBD).

## Open Questions
- How should job queue storage persist engine-specific state? Options include protobuf payload per engine or dedicated tables.
- Can remote upload pooling remain shared, or does LibSql need custom throttling (e.g., per-delta streaming)?
- Do we need to version manifest operations to support concurrent old/new engines during rollout?
- How does LibSql pager surface dirty pages today, and what blocking API changes are required?
