# AOF2 Documentation Index

This directory centralizes design notes, rollout plans, and operational guidance for the AOF2 tiered store.

## Core Designs
- [aof2_store.md](aof2_store.md) — end-to-end architecture for the tiered segment store across tiers 0–2.
- [aof2_design_next.md](aof2_design_next.md) — detailed state machines, coordinator flows, and async backpressure diagrams.
- [aof2_manifest_log.md](aof2_manifest_log.md) — binary manifest log format, chunk lifecycle, replay, and tooling.

## Plans and Status
- [aof2_implementation_plan.md](aof2_implementation_plan.md) — milestone tracking for the ongoing async tiered rollout.
- [aof2_progress.md](aof2_progress.md) — snapshot of completed and in-flight work streams.
- [aof2_improvements.md](aof2_improvements.md) — open improvement opportunities and backlog discussion.
- [aof2_improvements_response.md](aof2_improvements_response.md) — responses and decisions on the improvement backlog.

## Replay Metrics
Tiered manifest replay emits metrics that back operators and alerting. Use these to confirm crash-mid-commit recovery stays inside SLOs.
- `aof_manifest_replay_chunk_lag_seconds`: Wall-clock seconds spent trimming and replaying the most recent chunk. Alert if this exceeds 5s for two consecutive runs.
- `aof_manifest_replay_journal_lag_bytes`: Trailing uncommitted bytes detected during trim. This should normally be < 1 chunk record (~512 bytes).
- `aof_manifest_replay_chunk_count`: Number of manifest log chunks inspected during replay.
- `aof_manifest_replay_corruption_events`: Count of CRC or decode failures observed while replaying a stream.

Grafana dashboard: `Tiered AOF2 / Manifest Replay`.
- Panel ID `12`: charts `aof_manifest_replay_chunk_lag_seconds` with warn=3s / critical=5s thresholds.
- Panel ID `13`: charts `aof_manifest_replay_journal_lag_bytes` with warn=1024 bytes / critical=4096 bytes.

For log-based checks use the query `metric_name:"aof_manifest_replay_*" stream_id:42` to inspect the integration-test stream or drop the filter to review production workloads.
- Enable log-only bootstrap by setting `tier1_manifest_log_only` (or `AOF2_MANIFEST_LOG_ONLY=1`) once parity is confirmed; revert by clearing the flag and rerunning the crash replay test.

## Operational Guides
- [aof2_read_path.md](aof2_read_path.md) — reader API behaviour, `WouldBlock` handling, and migration advice.
- [aof2_tracing.md](aof2_tracing.md) — tracing strategy and span layout for the tiered pipeline.
- [aof2_catalog_snapshots.md](aof2_catalog_snapshots.md) — catalog snapshot format, retention policy, and recovery usage.
- [aof2_flush_metadata_note.md](aof2_flush_metadata_note.md) — flush pipeline metadata responsibilities and durability markers.
- [aof2_python_test_runner.md](aof2_python_test_runner.md) — notes on the Python-based integration test harness.
- `ManifestInspector` (see `crates/bop-aof/src/manifest/inspect.rs`) — developer helper to dump manifest chunk metadata for a stream.




