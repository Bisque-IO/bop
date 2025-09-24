# AOF2 Roadmap

## Summary
Tracks upcoming work for the AOF2 subsystem across reliability, performance, tooling, and operations. Based on historical implementation plans and improvement logs.

## Near-Term (Q4 2025)
| Item | Owner | OKR Alignment | Notes |
| --- | --- | --- | --- |
| Release notes automation pipeline | Product PM + Docs Team | Developer Experience KR1 | Generate `CHANGELOG.md` from milestone data and release_notes.md. |
| Tier 1 hydration worker auto-scaling | Storage Platform | Storage Reliability KR2 | Derive worker count from backlog metrics; ties into `BackpressureKind::Hydration` telemetry. |
| Tier 2 circuit breaker | Reliability Architecture | Storage Reliability KR3 | Prevent repeated retries during object store failures (`aof2_improvements_response.md`). |
| Manifest snapshot pruning | Storage Platform | Storage Reliability KR1 | Drop obsolete chunks after Tier 2 confirmation to shrink restart time. |
| Markdownlint/vale CI expansion | Docs Team | Developer Experience KR1 | Extend documentation automation beyond markdown style checks (include manuals). |

## Mid-Term (H1 2026)
| Item | Owner | OKR Alignment | Notes |
| --- | --- | --- | --- |
| Adaptive tier budgeting | Storage Platform | Storage Efficiency KR1 | Feedback loops adjust Tier 0/1 budgets based on workload patterns. |
| Manifest fuzz testing automation | QA Automation | Storage Quality KR2 | Integrate with `testing-validation.md` plan. |
| CLI golden tests | Tooling & DX | Developer Experience KR2 | Capture deterministic outputs for regression detection. |
| Tiered observability dashboard refresh | Reliability Architecture | Observability KR1 | Align metrics with new snapshots + health indicators. |

## Long-Term (H2 2026+)
| Item | Owner | OKR Alignment | Notes |
| --- | --- | --- | --- |
| Segment compaction framework | Storage Platform | Storage Efficiency KR3 | Replace ad-hoc retention policies with coordinated compaction. |
| Multi-cluster replication | Platform Architecture | Platform Resilience KR1 | Extend manifest log replication to DR clusters. |
| Hybrid cold storage backends | Storage Platform | Cost Optimization KR2 | Support Glacier-like tiers via pluggable adapters. |

## Dependencies & Risks
- Circuit breaker work depends on metrics instrumentation landing in the next release.
- Adaptive budgeting requires reliable telemetry; tie-in with dashboard refresh.
- Multi-cluster replication pending Raft upgrades scheduled separately.

## Reporting
- Track progress in `docs/bop-aof/progress_tracker.md` per milestone.
- Update this roadmap at least once per quarter; log significant decisions in the table below.

## Decision Log
| Date | Decision | Owner | Notes |
| --- | --- | --- | --- |
| 2025-09-25 | Roadmap aligned with 2026 OKRs | Docs Team | Updated milestone groupings to map to quarterly objectives. |
| 2025-09-24 | Established unified roadmap view | Docs Team | Derived from `aof2_improvements.md` and implementation plan. |

## Related Resources
- [Milestone 2 Review Notes](../bop-aof/review_notes.md)
- [Architecture](architecture.md)
- [Segment Format](segment-format.md)
- [Maintenance Handbook](maintenance-handbook.md)
- [Testing & Validation](testing-validation.md)
- [Tiered Operations Runbook](runbooks/tiered-operations.md)

