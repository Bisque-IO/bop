# BOP AOF User Manual

## Purpose
Provide operators and end-users with a single reference for installing, configuring, and operating bop AOF in production environments. This manual expands on the Getting Started guide by covering full workflows, troubleshooting, and support channels.

## Audience
- Platform administrators deploying bop AOF-backed services.
- Site Reliability Engineers responsible for day-to-day operations.
- Support and Enablement teams assisting customers.

## Prerequisites
- Access to the bop repository and release binaries.
- Credentials for Tier 2 object storage and monitoring systems.
- Familiarity with the concepts introduced in the [BOP AOF Overview](overview.md).

## Quick Start
1. Review environment prerequisites and install tooling (`./bake setup`, `xmake`).
2. Configure tier budgets and credentials using the templates in `examples/aof2-demo/config/`.
3. Run the demo ingest (`cargo run --bin aof2-demo`) and validate metrics via the provided dashboards.

## Key Tasks
- Deploy bop AOF across staging and production environments.
- Perform routine health checks and respond to alerts.
- Manage capacity, snapshots, and retention policies.

## Deep Dive
Sections below capture in-depth guidance for installation, operations, and support:

## Architecture Diagram
```mermaid
```
_Source: `docs/aof2/media/architecture-tier-overview.mmd`._

### Installation & Configuration
- Tooling requirements (`Rust 1.78+`, `xmake`, `cmake`).
- Configuration layout (`~/.config/bop-aof/config.toml`, environment variable overrides).
- Tier sizing guidance referencing `maintenance-handbook.md`.

### Operations & Monitoring
- Daily/weekly/monthly routines from [Operations Guide](operations.md) and [maintenance-handbook.md](../aof2/maintenance-handbook.md).
- Alert catalogue with links to runbook steps.
- Observability resources: dashboards, logs, tracing.

### Incident Response
- Quick triage matrix referencing [Tiered Operations Runbook](../aof2/runbooks/tiered-operations.md).
- Escalation paths and communication templates.
- Post-incident validation checklist.

```mermaid
flowchart TD
    Alert[Alert Fired] --> Detect[Confirm metrics/logs]
    Detect --> Identify[Identify impacted tier]
    Identify --> Mitigate[Apply tier-specific mitigation]
    Mitigate --> Validate[Validate metrics recovered]
    Validate --> PostMortem[Log follow-ups & updates]
    Mitigate --> Escalate{Need escalation?}
    Escalate -- Yes --> StorageOnCall[Storage Platform On-Call]
    Escalate -- No --> Validate
    StorageOnCall --> Validate
```
_Source: `docs/bop-aof/media/operations-health-check-flow.mmd`._

### Maintenance & Upgrades
- Procedures for tier budget adjustments, compaction, and retention.
- Rolling upgrade workflow (seal segments, monitor manifest replay).
- Catalog snapshot management.

### Support & Escalation
| Scenario | Primary Contact | Backup | Notes |
| --- | --- | --- | --- |
| Severity 1 (data loss risk) | Storage Platform On-Call | Platform Networking | Page via PagerDuty; include manifest offsets and Tier 2 status. |
| Severity 2 (degraded throughput) | Site Reliability Engineering | Docs Lead | Open incident channel; track mitigation steps in runbook. |
| Customer support request | Support Enablement | Product PM | Provide user manual excerpt; link to troubleshooting table. |

### Support Policy Summary
- **Coverage:** 24/7 for production incidents, business hours for feature questions.
- **Response Targets:** Sev1 acknowledgement in 15 minutes, Sev2 within 1 hour.
- **Handoffs:** Document context in the incident timeline and update `review_notes.md` after closure.

### Resources
- Internal channels: `#bop-docs`, `#bop-storage`, Storage Platform PagerDuty alias.
- External communications: use customer advisory template (see `docs/templates/public_doc_template.md`).
- Release notes: tracked in milestone change history and future `CHANGELOG.md`.

## Troubleshooting
| Issue | Diagnosis | Resolution | References |
| --- | --- | --- | --- |
| Admission backpressure (`BackpressureKind::Admission`) | `activation_queue_depth > 0`, Tier 0 at capacity | Raise Tier 0 budget, seal segments, or throttle writers | [Operations Guide](operations.md); [Tiered Runbook](../aof2/runbooks/tiered-operations.md) |
| Hydration backlog (`BackpressureKind::Hydration`) | Tier 1 hydration queue elevated, missing warm files | Scale hydrator workers, verify Tier 2 availability, trigger manual hydrate | [Maintenance Handbook](../aof2/maintenance-handbook.md); [Operations Guide](operations.md) |
| Tier 2 instability | Upload/delete retries climbing, object store alerts | Fail over to alternate bucket, engage Networking + Storage On-Call | [Tiered Runbook](../aof2/runbooks/tiered-operations.md) |
| CLI errors / permission denied | Insufficient privileges or wrong config path | Run with elevated permissions or set `AOF2_CONFIG`; capture logs for support | [CLI Reference](cli-reference.md) |
| Manifest mismatch | Replay marks segments `NeedsRecovery` | Run `aof2-admin manifest check`, reconcile Tier 2 objects, restore snapshots | [Segment Format](../aof2/segment-format.md); [Testing & Validation](../aof2/testing-validation.md) |

## Frequently Asked Questions
- **How do I upgrade without downtime?** Seal active segments, follow the rolling upgrade checklist in the maintenance handbook, and monitor manifest replay metrics.
- **Can I disable Tier 2?** Tier 2 is required for durability. For lab setups, keep Tier 2 pointed at a MinIO instance and adjust retention.
- **Where are release notes published?** Milestone summaries in `docs/bop-aof/progress_tracker.md` and the upcoming `CHANGELOG.md` (tracked in roadmap) capture release highlights.
- **How do I request new documentation?** Open an issue tagged `docs:bop-aof` or contact the Docs Lead via `#bop-docs`.

## Glossary
| Term | Definition |
| --- | --- |
| Admission Guard | Handle obtained before appending to Tier 0 to enforce capacity limits. |
| Hydration | Process of loading sealed segments from Tier 1/2 back into Tier 0. |
| Manifest Log | Ledger of tier lifecycle events enabling deterministic recovery. |
| Snapshot | Serialized manifest state paired with chunk offsets for fast restart. |
## Release Notes Integration
- Milestone-level highlights recorded in [progress_tracker.md](progress_tracker.md#change-history).
- Internal architecture changes documented in `docs/aof2/roadmap.md` decision log.
- Draft release notes maintained in [release_notes.md](release_notes.md) until the automated `CHANGELOG.md` is live.

## Related Resources
- [Release Notes](release_notes.md)
- [BOP AOF Overview](overview.md)
- [Getting Started Guide](getting-started.md)
- [API Guide](api-guide.md)
- [CLI Reference](cli-reference.md)
- [Operations Guide](operations.md)
- [Maintenance Handbook](../aof2/maintenance-handbook.md)
- [Tiered Operations Runbook](../aof2/runbooks/tiered-operations.md)

## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-25 | FAQ and release notes integration added | Docs Team |
| 2025-09-25 | Support policy, escalation matrix, troubleshooting table added | Docs Team |
| 2025-09-25 | Initial user manual draft created | Docs Team |
