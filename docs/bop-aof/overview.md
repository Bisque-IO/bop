# BOP AOF Overview

## Purpose
BOP's append-only file (AOF2) subsystem provides durable, low-latency persistence with automated tiering across memory, disk, and object storage. This overview explains the architecture, highlights key differentiators, and points readers to the right deep-dive resources.

## What Is BOP AOF?
BOP AOF is a tiered event store used across the BOP platform for write-heavy workloads and replay-driven services.

### Tiered Architecture Snapshot
- **Tier 0 (Hot)** — memory-mapped segments for synchronous appends; optimized for single-digit millisecond latency.
- **Tier 1 (Warm)** — compressed, seekable segments cached on local disk to absorb read traffic and hydrate quickly.
- **Tier 2 (Cold)** — S3-compatible object storage providing durable archival and cross-region recovery.
- **Manifest Log** — binary ledger that coordinates segment lifecycle, enabling deterministic recovery and tooling support.

```
 client append -> Tier0 mmap -> seal -> Tier1 Zstd -> upload -> Tier2 object store
                                     ^                         |
                                     |---- hydration queue <---|
```



## Tier Pipeline Diagram
```mermaid
graph LR
    Client[Client / Service] -->|append| Tier0[Tier 0 (mmap)]
    Tier0 -->|seal & compress| Tier1[Tier 1 (Zstd cache)]
    Tier1 -->|upload| Tier2[Tier 2 (Object Store)]
    Tier1 -->|hydrate| Tier0
    Tier0 -->|manifest events| Manifest[Manifest Log]
    Tier1 -->|manifest events| Manifest
    Tier2 -->|manifest events| Manifest
    Manifest -->|replay| Followers[Followers / Readers]
```
_Source: `docs/bop-aof/media/overview-tier-pipeline.mmd`._

### Why Upgrade from AOF1?
- Deterministic tier movement with bounded local storage.
- Async flush and retry semantics that prevent cascading failures.
- Integrated observability (metrics, tracing, structured logs) aligned with BOP platform conventions.

## Documentation Map
| Question | Start Here | Stakeholder |
| --- | --- | --- |
| "How do I stand up AOF locally?" | [Getting Started](getting-started.md) | Developer Experience |
| "How do I embed the API in my service?" | [API Guide](api-guide.md) | Storage Platform Engineering |
| "Which CLI command inspects residency?" | [CLI Reference](cli-reference.md) | Tooling & DX |
| "How do I operate bop-aof in production?" | [Operations Guide](operations.md) | SRE |
| "What happens during an incident?" | [Operational Runbook](../aof2/aof2_runbook.md) | SRE |

## Adoption Scenarios
- **Stream ingestion services** needing crash-consistent append and long-term retention.
- **Change-data-capture pipelines** that replay events into downstream analytics.
- **Hybrid deployments** where Tier 2 offloads archival to cost-efficient object stores while Tier 0/1 deliver hot access.

## Terminology
| Term | Definition |
| --- | --- |
| Segment | Fixed-size chunk of append-only data tracked by the manifest.
| Hydration | Moving a segment from Tier 2/1 into Tier 1/0 for serving reads.
| BackpressureKind | Signal returned to clients indicating the tier that requires remediation.
| Residency Gauge | Snapshot of tiers showing which segments need recovery or hydration.

## Integration Checklist
1. Identify which workloads require hot, warm, and cold retention and size tier budgets accordingly.
2. Ensure Tier 2 credentials and network routes are resilient; the runbook assumes failover access.
3. Embed metrics and tracing exporters before launching to capture baseline SLOs.

## Troubleshooting
- **Mismatch between manifest and data directories**: run `aof2-admin dump` and compare reported segments with disk layouts; use manifest repair tooling if needed.
- **Tier 1 hydration thrash**: confirm retry policies and monitor `hydration_queue_depth` to tune worker counts.
- **Slow replay after restart**: inspect manifest chunk counts and enable log-only bootstrap as described in `aof2_manifest_log.md`.

## Related Resources
- [Milestone 2 Review Notes](review_notes.md)
- [Getting Started](getting-started.md)
- [BOP Platform Overview](../platform_overview.md)
- [AOF2 Tiered Store Design](../aof2/aof2_store.md)
- [AOF2 Operational Runbook](../aof2/aof2_runbook.md)
- [Stakeholder & Template Matrix](m1_foundation_summary.md)

## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-24 | SME sign-off and pipeline diagram added | Docs Team |
| 2025-09-23 | Expanded overview with architecture summary and documentation map | Docs Team |
| 2025-09-23 | Initial outline populated with discovery highlights | Docs Team |
