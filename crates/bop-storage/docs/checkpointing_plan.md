# Checkpointing Implementation Plan

## Objectives
- Deliver the checkpointing system described in `checkpointing_design.md`, covering planner, executor, manifest integration, and retention flows.
- Ensure LMDB-backed manifests (`chunk_catalog`, `chunk_delta_index`, `change_log`) and S3 blobs stay strongly consistent under retries and restarts.
- Provide operational hooks for append-only truncation, cold scans, and manifest-driven job orchestration with clear observability.
- Keep PageStore, LocalStore, and RemoteStore interactions efficient while respecting quota and lease coordination rules.

## Scope
- **In scope**: Checkpoint planner/executor code paths, manifest writer batches, chunk delta handling, append-only truncation jobs, cold scan APIs, quota enforcement, GC integration, and monitoring.
- **Out of scope**: Non-checkpoint storage engines, libsql-specific planner optimisations beyond current design, UI tooling, or multi-tenant billing integrations.

## Dependencies
- Stable LMDB manifest APIs (`ManifestOp`, `ChunkEntryRecord`, `ChunkDeltaRecord`).
- S3 client abstractions with multipart upload + retry support.
- PageStore and PageCache primitives for lease tracking and dirty page enumeration.
- Job queue runtime capable of durable enqueue/ack (existing RocksDB-backed queue).

## Phased Delivery
1. **Foundations** – planner/executor scaffolding, manifest writer plumbing, storage quota service.
2. **Durability Features** – chunk delta pipeline, append-only truncation jobs, LMDB change-log updates.
3. **Operational Enhancements** – cold scan read mode, metrics/logging, disaster-recovery validation, GC alignment.
4. **Hardening** – fault-injection tests, lease race tests, documentation, readiness review.

## Task List
1. Implement checkpoint planner split for append-only vs. libsql workloads.
2. Build checkpoint executor staging pipeline (scratch dirs, hash verification, upload flow).
3. Wire manifest writer batches for chunk catalog, delta index, WAL catalog, and change log updates.
4. Add chunk delta production/consumption (delta files, manifest ops, snapshot references).
5. Implement append-only truncation interface plus manifest job (`TruncateAppendOnly`).
6. Extend job queue runner with planner/executor/truncation orchestration + progress reporting.
7. Finalise LocalStore quota enforcement, `hot_cache/` eviction, and scratch janitor behaviour.
8. Wrap RemoteStore S3 streams with retry/backoff, checksum validation, and naming scheme.
9. Expose cold scan (`ColdScan`) read APIs and integrate `RemoteReadLimiter` controls.
10. Integrate PageStore leasing, checkpoint leases, and cache population policies.
11. Add failure-handling surface (idempotent retries, compensating changes, recovery paths).
12. Instrument metrics/logging, publish dashboards, and document operational runbooks.

## Task Tracking
| ID | Task | Status | Notes |
|----|------|--------|-------|
| T1 | Checkpoint planner split (append-only vs libsql) | TODO | |
| T2 | Executor staging pipeline + upload verification | TODO | |
| T3 | Manifest writer batches for catalog/delta/change log | TODO | |
| T4 | Chunk delta production & snapshot integration | TODO | |
| T5 | Append-only truncation job + lease coordination | TODO | |
| T6 | Job queue orchestration & progress reporting | TODO | |
| T7 | LocalStore quota enforcement & janitor | TODO | |
| T8 | RemoteStore streaming with retries & checksums | TODO | |
| T9 | Cold scan API & RemoteReadLimiter integration | TODO | |
| T10 | PageStore lease wiring & cache policies | TODO | |
| T11 | Failure handling, recovery, and idempotency tests | TODO | |
| T12 | Metrics, logging, dashboards, and runbooks | TODO | |

## Milestones
- **M1 – Foundations Ready** (T1–T3, T7, T8): planner/executor functional with manifest durability.
- **M2 – Durability Enhancements** (T4–T6, T10): chunk deltas, truncation, and job orchestration operational.
- **M3 – Operational Readiness** (T9, T11, T12): cold scan support, resiliency validated, metrics live.

## Verification Strategy
- Unit tests for planner decisions, manifest ops, chunk delta layering, and truncation rules.
- Integration tests across checkpoint -> manifest -> recovery flows using temporary LMDB and S3 mocks.
- Fault-injection (e.g., upload failure, manifest writer panic) to confirm idempotent retries and compensating change logs.
- Load tests simulating mixed append-only/libsql workloads with quotas and cold scans.

## Rollout & Ops
- Feature flags to enable append-only truncation and cold scan mode per tenant.
- Staged rollout (internal cluster -> canary -> production) with checkpoints taken before each stage.
- Runbook updates covering lease troubleshooting, truncation cancellation, and recovery procedures.
- Metrics alerts for checkpoint duration, queue depth, quota exhaustion, remote bandwidth saturation.

## Rules
- After completing any task, immediately update the Task Tracking table (Status + Notes) before starting new work.
- Record test coverage (automated or manual validation) in the Notes column for each completed task.
- Keep edits ASCII-only unless touching files that already contain Unicode art (e.g., diagrams).
- Preserve terminology and naming from `checkpointing_design.md` when adding code or documentation references.
- Surface any unexpected manifest/schema change requirements to the team before implementation continues.
