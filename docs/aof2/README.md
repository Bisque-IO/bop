# AOF2 Documentation Index

This directory centralizes design notes, rollout plans, and operational guidance for the AOF2 tiered store.

## Core Suite (Milestones 3-4)
- [architecture.md](architecture.md) — consolidated component and control-flow reference (includes architecture & manifest diagrams).
- [segment-format.md](segment-format.md) — manifest and segment schema definitions.
- [maintenance-handbook.md](maintenance-handbook.md) — planned maintenance procedures.
- [testing-validation.md](testing-validation.md) — test/benchmark matrix and drills (recovery drill diagram).
- [runbooks/tiered-operations.md](runbooks/tiered-operations.md) — on-call triage aid (pairs with `aof2_runbook.md`).
- [roadmap.md](roadmap.md) — forward-looking backlog.
- [developer_manual.md](developer_manual.md) — developer workflows, testing, and contribution guide.

## Supporting References
- [aof2_store.md](aof2_store.md) — legacy deep dive; source for architecture excerpts.
- [aof2_design_next.md](aof2_design_next.md) — historical design evolution and state machines.
- [aof2_manifest_log.md](aof2_manifest_log.md) — detailed manifest format (superseded by `segment-format.md`).
- [aof2_segment_metadata_plan.md](aof2_segment_metadata_plan.md) — background on metadata rollout.
- [aof2_catalog_snapshots.md](aof2_catalog_snapshots.md) — snapshot usage.
- [aof2_flush_metadata_note.md](aof2_flush_metadata_note.md) — flush pipeline context.
- [aof2_read_path.md](aof2_read_path.md) — reader behaviour and backpressure.
- [aof2_tracing.md](aof2_tracing.md) — telemetry implementation.
- [aof2_runbook.md](aof2_runbook.md) — detailed runbook (now complemented by the tiered operations summary).
- [aof2_python_test_runner.md](aof2_python_test_runner.md) — external harness docs.
- [aof2_implementation_plan.md](aof2_implementation_plan.md) & [aof2_progress.md](aof2_progress.md) — historical milestone tracking.
- [aof2_improvements.md](aof2_improvements.md) & [aof2_improvements_response.md](aof2_improvements_response.md) — backlog discussions feeding the roadmap.

## Usage
- Start with the Core Suite for current architecture and procedures.
- Consult supporting references for deep technical context or historical decisions.
- Update `roadmap.md` and `maintenance-handbook.md` whenever new features land or maintenance procedures evolve.
