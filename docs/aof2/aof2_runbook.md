# AOF2 Operational Runbook

# Summary
The runbook guides operators through detecting, triaging, and resolving incidents in the tiered bop-aof (AOF2) store. Use it alongside the public operations guide for routine tasks and the architecture notes (`aof2_store.md`, `aof2_read_path.md`) when deeper context is needed.

> Quick reference: see `runbooks/tiered-operations.md` for a condensed checklist used during paging.


# Preconditions
- BOP deployment with tiered storage enabled and the `aof2-admin` CLI installed on admin hosts.
- Access to Grafana dashboards for Tier 0/1/2 metrics and log aggregation for structured `aof2::*` events.
- Credentials for the backing object store (Tier 2) and SSH access to affected nodes.
- Awareness of current capacity targets (Tier 0/1 budgets, retry policies) and change approvals for scaling.

# Detection
- **Metrics**
  - `Tier0MetricsSnapshot.activation_queue_depth` > 0 for ≥60s indicates hot cache exhaustion.
  - `Tier0MetricsSnapshot.total_bytes` trending toward `cluster_max_bytes` suggests runaway writers.
  - `Tier1MetricsSnapshot.hydration_queue_depth` spikes or sustained `hydration_latency_p99_ms` > 2s imply hydration backlog.
  - `Tier1MetricsSnapshot.residency.needs_recovery` > 0 after hydration completes points to missing warm files.
  - `Tier2MetricsSnapshot.upload_retry_attempts` / `delete_retry_attempts` increasing alongside queue depth highlights Tier 2 instability.
  - `FlushMetricsSnapshot.flush_failures` / `metadata_failures` non-zero means writers are blocked.
- **Logs**
  - `aof2::tiered` `tiered_admit_segment` for rollover pacing.
  - `aof2::tier0` `tier0_eviction_scheduled`, `tier0_segment_dropped` for churn diagnostics.
  - `aof2::tier1` `tier1_hydration_retry_*`, `tier1_hydration_failed` for warm-cache health.
  - `aof2::tier2` upload/delete/fetch events to correlate with object-store behavior.
- **Alerts**
  - Tier 0 exhaustion: `activation_queue_depth > 0` sustained 60s.
  - Hydration regression: `hydration_latency_p99_ms > 2000` or `needs_recovery > 0` for 5 minutes.
  - Tier 2 instability: retry attempts +10/min or queue depth > worker count.
  - Flush failure: any increment to `flush_failures` or `metadata_failures`.
  - Tier 2 fetch failures: more than two `tier2_fetch_failed` events within 5 minutes.

# Triage Checklist
1. Confirm which tier is degraded by reviewing the dashboard and scraping the latest `aof2-admin dump` (command below).
2. Check for recent configuration changes in Tier 0/1 budgets or retry policies; roll back unsafe adjustments.
3. Inspect structured logs to correlate with metric spikes (hydration retries, flush failures, Tier 2 errors).
4. Determine backpressure mode by sampling error responses from clients or service logs.
5. If Tier 2 instability is suspected, validate object-store availability and credentials immediately.
6. Document findings in the incident timeline and notify the on-call Storage Platform owner.

# Mitigation
## Configuration Adjustments
| Setting | Location | Guidance |
| --- | --- | --- |
| `Tier0CacheConfig::cluster_max_bytes` | `AofManagerConfig::store.tier0` | Increase if admission backpressure is sustained; ensure headroom for two full segments per writer. |
| `Tier0CacheConfig::activation_queue_warn_threshold` | same | Tune to match expected concurrent hydrations; raise temporarily during load-tests. |
| `Tier1Config::max_bytes` | `AofManagerConfig::store.tier1` | Expand when residency regularly hits recovery state; adjust before scaling Tier 0. |
| `Tier1Config::max_retries` / `retry_backoff` | same | Increase for eventually consistent storage, decrease to surface failures faster. |
| `Tier2Config::retry` (`RetryPolicy`) | `AofManagerConfig::store.tier2` | Tune retry attempts/delay to balance durability vs. latency budgets. |

## Backpressure Response Matrix
| BackpressureKind | Likely Cause | Immediate Action |
| --- | --- | --- |
| `Admission` | Tier 0 is full | Inspect `tier0_eviction_scheduled` logs, increase Tier 0 budget, seal segments sooner, or rate-limit writers. |
| `Hydration` | Warm files missing or backlog | Check `hydration_queue_depth` and Tier 1 retries, free Tier 1 space, ensure Tier 2 availability, rerun admin dump to verify residency. |
| `Rollover` | Coordinator sealing segments | Observe `tiered_admit_segment` events, confirm flush backlog is steady; usually transient. |
| `Flush` | Flush worker retries exhausted | Investigate filesystem latency, disk saturation, and `flush_failures`; consider throttling writers. |
| `Tail` | Reader outran durability | Compare append vs flush rate; issue resolves after backlog drains. |

## Operational Actions
- Run `aof2-admin` snapshot to capture current residency and queue depths for escalation:
  ```bash
  cargo run --bin aof2-admin -- dump --root /var/lib/bop/aof2 > /tmp/aof2_dump.txt
  ```
  Archive the emitted JSON for offline analysis.
- For Tier 2 instability, shift traffic to a healthy region or object-store bucket if available, then rehydrate impacted segments once the store stabilizes.
- When flush failures persist, rotate logs, verify disk IO, and restart the flush worker after ensuring capacity is available.

# Escalation
- **Primary:** Storage Platform on-call engineer (check paging rotation).
- **Secondary:** Site Reliability Engineering for sustained Tier 2/object-store outages.
- **Additional:** Docs Lead for updates required in public/internal guides when new mitigation steps emerge.
Provide the `aof2-admin` dump, recent dashboard screenshots, and timeline segments when escalating.

# Validation
- Re-run targeted failure tests:
  ```bash
  cargo test --manifest-path crates/bop-aof/Cargo.toml --test aof2_failure_tests
  ```
  Confirms retry paths and metadata consistency.
- Benchmark append throughput to ensure pipeline recovered:
  ```bash
  cargo bench --manifest-path crates/bop-aof/Cargo.toml --bench aof_benchmarks -- --bench aof2_append_metrics
  ```
  Track throughput deltas against pre-incident baselines.
- Verify dashboards show queues drained and alerts cleared; confirm clients receive normal responses for 10+ minutes.

# Post-Incident Tasks
- Update incident log and file follow-up tickets for configuration or automation changes.
- Capture any new mitigations in this runbook and notify Docs Lead for documentation sync.
- Review capacity targets and adjust Tier 0/1 budgets if usage trends changed.
- Schedule retrospective with Storage Platform and SRE teams within two business days.
