# AOF2 Architecture

## Summary
This document captures the current internal architecture of the bop AOF2 tiered store. It consolidates the scattered design notes (`aof2_store.md`, `aof2_design_next.md`) into a single reference covering components, control flow, and operational boundaries used by Storage Platform engineers and reviewers.

## Goals
- Provide a canonical description of the tiered storage pipeline (Tier 0/1/2, manifest, coordinator).
- Highlight control flows that inform API behaviour and backpressure signals.
- Document the invariants required for durability, recovery, and observability.

## Non-Goals
- Public onboarding. See `../bop-aof/overview.md` for the external story.
- Detailed segment/manifest binary formats (captured in `segment-format.md`).
- Procedural runbooks (see `runbooks/tiered-operations.md`).

## Architecture Overview
| Component | Responsibility | Notes |
| --- | --- | --- |
| `AofManager` | Entry point for writers/readers; manages Tier 0 residency and admission guards. | Lives inside each bop worker; exposes async and sync APIs. |
| `TieredCoordinator` | Orchestrates segment rollover, compression, uploads, and hydration. | Serializes background jobs and enforces retry/backoff policy. |
| `Tier0Cache` | Memory mapped hot store for active + recently accessed segments. | Enforces `cluster_max_bytes`; emits `BackpressureKind::Admission` when saturated. |
| `Tier1Cache` | Warm cache of sealed segments stored as Zstd streams. | Owns compression workers, hydration backlog, and residency gauge. |
| `Tier2Client` | Abstraction over S3-compatible object storage. | Upload/delete operations use async tasks gated by semaphores. |
| `ManifestLog` | Append-only ledger of tier lifecycle events. | Drives deterministic recovery and reconciliation. |
| `CatalogSnapshot` | Periodic snapshot of manifest state. | Allows fast restart without replaying entire log. |

## Architecture Diagram
```mermaid
```
_Source: `docs/aof2/media/architecture-tier-overview.mmd`._

The tier pipeline is designed so only the Raft leader mutates Tier 1/2 and emits manifest records. Followers replay the manifest to mirror state.

## Data & Control Flow
## Manifest Lifecycle Diagram
```mermaid
```
_Source: `docs/aof2/media/manifest-lifecycle.mmd`._

1. **Append Path**
   - Writers acquire an `AdmissionGuard` from Tier 0.
   - Records are appended to the active segment; when thresholds are hit (`segment_target_bytes`, flush pressure, coordinator directives) the guard requests rollover.
   - Rollover posts a `CoordinatorCommand::Rollover` and surfaces `BackpressureKind::Rollover` until the coordinator acknowledges.
2. **Rollover Processing**
   - Coordinator seals the segment, writes a `SealSegment` manifest record, and stages Tier 1 compression.
   - Tier 0 releases residency; admission waiters resume once the new segment activates.
3. **Tier 1 Compression & Hydration**
   - Compression workers convert segments into Zstd streams and emit `CompressionDone` records.
   - Hydration workers satisfy read requests by loading segments from Tier 1 (or Tier 2) back into mmap form; failures raise `BackpressureKind::Hydration`.
4. **Tier 2 Upload/Delete**
   - Upload queue pushes warm artifacts to the configured object store; manifest records (`UploadStarted`, `UploadDone`) track progress.
   - Retention policies enqueue deletes (`Tier2DeleteQueued`, `Tier2Deleted`) once manifest and Tier 1 confirm success.
5. **Recovery**
   - On startup nodes load the latest snapshot, replay manifest chunks, and reconcile Tier 2 state.
   - Any gaps mark segments `NeedsRecovery`, surfaced via `Tier1MetricsSnapshot`.

## Follower Replay & Hydration Notes
- Followers replay manifest chunks sequentially but hydrate segments lazily. Keep the hydration worker pool sized so replay catch-up stays within the five-minute SLA.
- When snapshots lag behind manifest chunks, followers mark segments as `NeedsRecovery`; the coordinator backfills via Tier 1/2 hydration without blocking appenders.
- Cross-region followers should pin Tier 2 endpoints via DNS overrides to avoid hydration backoffs during failover tests.

## Persistence & Compatibility
- Manifest log uses versioned chunk headers (`version = 1`) and CRC enforcement. Future changes append fields and set reserved flags; consumers must ignore unknown bits.
- Segment files remain memory-map friendly to keep append latency deterministic.
- Tier 2 object keys use the stable layout `manifest/<stream_id>/<chunk_index>.mlog` for the manifest and `segments/<stream>/<segment_id>.zst` for data.
- Snapshots store both the manifest state and chunk offsets so older nodes can replay without reading entire history.

## Operational Considerations
- Hot path is intentionally lock-free outside admission and flush guards; instrumentation must avoid blocking.
- Hydration backlog is the primary signal for scaling Tier 1 workers or provisioning Tier 0 memory.
- Coordinator runs on a shared Tokio runtime; long-running callbacks must be spawned and awaited asynchronously to preserve responsiveness.
- Observability surfaces: `Tier0MetricsSnapshot`, `Tier1MetricsSnapshot`, `Tier2MetricsSnapshot`, and `TieredObservabilitySnapshot`.

## Open Risks
| Risk | Impact | Mitigation |
| --- | --- | --- |
| Tier 1 compression backlog during Tier 2 outage | Elevated hydration latency, potential admission stalls | Increase Tier 1 worker pool, enable adaptive retry backoff, consider circuit breaker described in `aof2_improvements_response.md`. |
| Manifest replay cost in large clusters | Slow restart and longer leader elections | Adopt incremental snapshots and chunk pruning (tracked in roadmap). |
| Mixed-version cluster rolling upgrades | Behavioural drift if manifest schema evolves | Gate new record types by feature flags; ensure readers ignore unknown payloads. |

## Decision Log
| Date | Decision | Owner | Notes |
| --- | --- | --- | --- |
| 2024-07-18 | Only leaders mutate Tier 1/2 and emit manifest entries | Storage Platform Architecture | Maintains deterministic replay; see `aof2_store.md` history. |
| 2024-08-05 | Adopt Zstd streams for Tier 1 artifacts | Storage Platform Architecture | Balances density with random access; captured in `aof2_design_next.md`. |
| 2025-01-12 | Introduce `TieredObservabilitySnapshot` aggregation | Reliability Architecture | Ensures metrics exporters avoid per-tier locking. |

## Related Resources
- [Milestone 2 Review Notes](../bop-aof/review_notes.md)
- [Segment Format](segment-format.md)
- [Maintenance Handbook](maintenance-handbook.md)
- [Testing & Validation](testing-validation.md)
- [Tiered Operations Runbook](runbooks/tiered-operations.md)
- [Roadmap](roadmap.md)


## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-25 | SME feedback incorporated (follower replay guidance) | Docs Team |
| 2025-09-24 | Added architecture & manifest diagrams; synced with review tracker | Docs Team |
