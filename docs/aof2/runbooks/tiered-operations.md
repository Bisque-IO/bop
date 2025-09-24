# Tiered Operations Runbook

# Summary
Guides on-call engineers through responding to tier-related incidents in bop AOF2. Use alongside the detailed `../aof2_runbook.md` which retains historical context.

# Preconditions
- Access to AOF2 admin CLI on affected nodes.
- Grafana dashboards for Tier 0/1/2 metrics and alerting integrated with paging.
- Credentials for Tier 2 object store and SSH access to cluster nodes.

# Detection
- Metrics alerts: `activation_queue_depth`, `hydration_latency_p99_ms`, `upload_retry_attempts`, `flush_failures`.
- Logs: `aof2::tier0` eviction events, `aof2::tier1` hydration retries, `aof2::tier2` upload/delete failures.
- Synthetic probes: smoke tests hitting append/read endpoints.

# Triage Checklist
1. Confirm which tier triggered the alert using dashboards and `aof2-admin dump`.
2. Review recent configuration or deployment changes.
3. Check manifest log for recent retries (`manifest-inspect --recent`).
4. Assess backpressure type returned to clients to target mitigation.
5. Document findings and notify Storage Platform on-call if escalation seems likely.

# Mitigation
- **Tier 0 admission pressure**: increase `tier0.cluster_max_bytes` temporarily, seal segments, or throttle writers.
- **Tier 1 hydration backlog**: scale hydrator workers, clear failed warm artifacts, ensure Tier 2 reachable.
- **Tier 2 instability**: shift traffic to alternate bucket or pause uploads; monitor retry queues.
- **Flush failures**: inspect disk IO, restart flush worker after verifying capacity.

# Escalation
- Primary: Storage Platform on-call.
- Secondary: Site Reliability Engineering for infra or Tier 2 outages.
- Provide dashboard snapshots, latest `aof2-admin dump`, and manifest timestamps when escalating.

# Validation
- Re-run targeted failure test (`cargo test --manifest-path crates/bop-aof/Cargo.toml --test aof2_failure_tests`).
- Ensure dashboards return to baseline and alerts clear for 15 minutes.
- Confirm clients resume normal throughput (application-level telemetry).

# Post-Incident Tasks
- Update `maintenance-handbook.md` if new recurring steps identified.
- Record incident summary in project board (`bop-aof-docs`).
- Schedule follow-up to adjust capacity or configuration.


# Multi-Region Escalation
- When incidents affect cross-region replication, notify the Platform Networking team alongside Storage On-Call.
- Document region-specific mitigations (Tier 2 endpoint overrides, manifest replay sequencing) and update `maintenance-handbook.md`.
- Coordinate with DR lead to confirm failover readiness after remediation.

## Related Resources
- [Milestone 2 Review Notes](../bop-aof/review_notes.md)
- [Architecture](architecture.md)
- [Segment Format](segment-format.md)
- [Maintenance Handbook](maintenance-handbook.md)
- [Testing & Validation](testing-validation.md)
- [Roadmap](roadmap.md)


## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-25 | Added multi-region escalation guidance | Docs Team |
