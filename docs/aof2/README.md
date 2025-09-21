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

## Operational Guides
- [aof2_read_path.md](aof2_read_path.md) — reader API behaviour, `WouldBlock` handling, and migration advice.
- [aof2_tracing.md](aof2_tracing.md) — tracing strategy and span layout for the tiered pipeline.
- [aof2_catalog_snapshots.md](aof2_catalog_snapshots.md) — catalog snapshot format, retention policy, and recovery usage.
- [aof2_flush_metadata_note.md](aof2_flush_metadata_note.md) — flush pipeline metadata responsibilities and durability markers.
- [aof2_python_test_runner.md](aof2_python_test_runner.md) — notes on the Python-based integration test harness.

