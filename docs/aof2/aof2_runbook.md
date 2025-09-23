# AOF2 Operational Runbook

This guide aggregates the knobs, metrics, and procedures operators need to keep the tiered AOF2 store healthy. Pair it with the design notes in `aof2_store.md` and the read-path guidance in `aof2_read_path.md` for deeper context.

## 1. Configuration Knobs

| Setting | Where | Guidance |
| ------- | ----- | -------- |
| `Tier0CacheConfig::cluster_max_bytes` | `AofManagerConfig::store.tier0` | Global hot cache budget. Size it so two full segments per writer plus 20–30% headroom fit without paging. Low values surface `WouldBlock(Admission)` frequently. |
| `Tier0CacheConfig::activation_queue_warn_threshold` | same | Warn threshold for the activation queue. Tune to the number of concurrent readers that may hydrate at once. |
| `Tier1Config::max_bytes` | `AofManagerConfig::store.tier1` | Warm cache budget. Should cover the working set of sealed segments. When the residency gauge spends time in `NeedsRecovery`, increase this before raising Tier 0. |
| `Tier1Config::max_retries`, `retry_backoff` | same | Governs compression and hydration retries inside Tier 1. Defaults are 3 attempts with a 50 ms backoff. Increase if Tier 2 is eventually consistent; decrease to surface failures faster. |
| `Tier2Config::retry` (`RetryPolicy`) | `AofManagerConfig::store.tier2` | Controls upload/delete/get retries. Default is 3 attempts with a 100 ms base delay. Increase for flaky object stores; decrease if latency budgets cannot absorb repeated retries. |

## 2. Monitoring Checklist

### Metrics

- **Tier 0** (`Tier0MetricsSnapshot`)
  - `activation_queue_depth` — sustained values > 0 indicate hot cache exhaustion. Alert if depth ≥4 for more than 60 seconds.
  - `total_bytes` — should hover below `cluster_max_bytes`. Trending to the limit signals runaway writers or stalled evictions.

- **Tier 1** (`Tier1MetricsSnapshot`)
  - `hydration_queue_depth` — spikes correlate with `WouldBlock(Hydration)` responses. Alert if depth exceeds the number of reader workers for more than one minute.
  - `hydration_latency_p99_ms` — track the long tail. Alert if p99 exceeds twice the expected fetch latency (e.g. >2 s for local warm cache, >5 s when Tier 2 fetches dominate).
  - Residency gauge — investigate when `needs_recovery` is non-zero after hydration should have succeeded; typically indicates missing warm files or Tier 2 fetch failures.

- **Tier 2** (`Tier2MetricsSnapshot`)
  - `upload_retry_attempts` / `delete_retry_attempts` — baseline near zero. Alert if attempts increase while queue depth grows; this surfaces S3 instability.
  - `upload_queue_depth` / `delete_queue_depth` — sustained depth greater than the semaphore limit means workers are overwhelmed. Consider increasing concurrency or backoffs.
  - `download_failures` — increments mean hydration fetches failed even after retries; expect `NeedsRecovery` residency until corrected.

- **Flush** (`FlushMetricsSnapshot`)
  - `flush_failures` / `metadata_failures` — any non-zero value requires intervention; appenders are likely blocked.

### Logs

Enable `INFO` level for the following targets and feed them into your log aggregator:

- `aof2::tiered` — `tiered_admit_segment` admissions.
- `aof2::tier0` — `tier0_eviction_scheduled`, `tier0_segment_dropped` for churn diagnostics.
- `aof2::tier1` — hydration retry/failure events (`tier1_hydration_retry_*`, `tier1_hydration_failed`).
- `aof2::tier2` — upload/delete/fetch successes or failures with attempt counts.

Use these events to correlate with metric spikes when diagnosing latency or data-movement issues.

## 3. Troubleshooting by BackpressureKind

| BackpressureKind | Likely Cause | Action |
| ---------------- | ----------- | ------ |
| `Admission` | Tier 0 full (activation queue depth > 0) | Inspect `tier0_eviction_scheduled` logs, increase Tier 0 budget, seal active segments sooner, or slow writers. |
| `Hydration` | Warm file missing or hydrator backlog | Check `hydration_queue_depth` and `tier1_hydration_retry_*` logs. Ensure Tier 1 has space, confirm Tier 2 availability, and rerun the admin dump to verify residency. |
| `Rollover` | Coordinator still processing seal/rollover | Watch `tiered_admit_segment` events; ensure the flush backlog is not growing. Usually transient. |
| `Flush` | Flush worker exhausted retries | Investigate `flush_failures`, filesystem latency, and disk saturation. Consider throttling writers until the backlog clears. |
| `Tail` | Tail follower outran durability | Check append rate vs flush rate; improves once the flush backlog drains. |

## 4. Admin Tooling

Use the `aof2-admin` helper to capture residency snapshots:

```bash
cargo run --bin aof2-admin -- dump --root /var/lib/bop/aof2 > /tmp/aof2_dump.txt
```

The command prints a table summarising Tier 0/1/2 state and emits a JSON payload at the end. Archive the JSON when escalating incidents; it contains the manifest residency (including Tier 2 object keys) and queue depths required for offline analysis.

## 5. Alerts

- **Tier 0 exhaustion:** `activation_queue_depth > 0` for 60 seconds.
- **Hydration regression:** `hydration_latency_p99_ms > 2000` or `needs_recovery > 0` for five minutes.
- **Tier 2 instability:** `upload_retry_attempts` or `delete_retry_attempts` increase by more than 10 per minute, or queue depth exceeds the worker count.
- **Flush failure:** `flush_failures > 0` or `metadata_failures > 0`.
- **Tier 2 fetch failures:** presence of `tier2_fetch_failed` events more than twice within five minutes.

Configure dashboards to chart the metrics above and add log-based alerts for the structured event names. Combine the alerts with the admin dump to speed triage.

## 6. Validation Checklist

Run the targeted validation suite before enabling tiered mode in a new environment:

```bash
cargo test --manifest-path rust/bop-rs/Cargo.toml --test aof2_failure_tests
```

The suite exercises metadata retry logic and Tier 2 upload/delete/fetch retry paths using deterministic failure injection (see `rust/bop-rs/tests/aof2_failure_tests.rs`).

To capture baseline append throughput with metrics enabled:

```bash
cargo bench --manifest-path rust/bop-rs/Cargo.toml --bench aof_benchmarks -- --bench aof2_append_metrics
```

Record the reported throughput and track deltas across releases. Hydration load generation is pending; follow the `VAL3` item in `aof2_implementation_plan.md` for updates.
