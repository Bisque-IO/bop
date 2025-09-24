# AOF2 Developer Manual

## Summary
Technical reference for engineers extending or integrating the bop AOF2 subsystem. It consolidates architecture, data formats, testing strategy, and contribution workflow in one manual for storage and platform developers.

## Goals
- Document internal components, APIs, and invariants required for safe changes.
- Provide guidance for adding new features, records, or tooling.
- Describe testing, validation, and release expectations for the AOF2 codebase.

## Audience
- Storage Platform engineers working on `crates/bop-aof`.
- Contributors maintaining the manifest, tiering, or tooling code paths.
- Quality and reliability engineers building automated coverage.

## System Overview
- Tier pipeline summary referencing [architecture.md](architecture.md) and embedded mermaid diagrams.
- Manifest and segment schema overview (see [segment-format.md](segment-format.md)).
- Coordinator command lifecycle and backpressure signalling.

## Module Guide
## Code Examples
```rust
// Example: extending manifest with a new record type
pub enum ManifestRecordType {
    // existing variants ...
    Tier2Throttle = 16,
}

// Update segment-format.md and replay logic accordingly
```

```rust
// Example: adding metrics for new coordinator path
fn record_custom_metric(coord: &Coordinator) {
    metrics::tier2_throttle_events().increment();
}
```

Additional snippets should be added alongside PRs and referenced in architecture/testing docs.

| Area | Description | Key Files |
| --- | --- | --- |
| Append path | Admission guards, segment management, flush integration | `crates/bop-aof/src/append/` |
| Tiered coordinator | Background workers for compression, upload, hydration | `crates/bop-aof/src/store/` |
| Manifest | Chunk writer/reader, snapshot management | `crates/bop-aof/src/manifest/` |
| Observability | Metrics, tracing, structured logs | `crates/bop-aof/src/metrics.rs`, `docs/aof2/aof2_tracing.md` |
| CLI tooling | `aof2-admin`, helpers | `crates/bop-aof-cli/` |

## Data Formats & Compatibility
- Manifest record expansion process: select new record id, update `segment-format.md`, extend replay logic, add tests.
- Segment footer/headers: guidelines for preserving mmap compatibility.
- Snapshot versioning and replay expectations.
- Reason-code table location and how to extend it.

## Development Workflow
1. Align proposed changes with [roadmap.md](roadmap.md) and capture ADR if required.
2. Update architecture/format docs alongside code changes.
3. Run test matrix (unit, failure tests, Python harness, benchmarks) and record results in PR description.
4. Seek review from storage architecture owner and relevant SMEs.

## Ownership & Release Checklist
| Area | Primary Owner | Responsibilities |
| --- | --- | --- |
| Append & Tiering | Storage Platform Architecture | Review API/backpressure changes, ensure invariants documented. |
| Manifest & Snapshots | Reliability Architecture | Approve schema updates, coordinate replay tooling. |
| Tooling & CLI | Tooling & DX Engineering | Maintain `aof2-admin`, ensure golden tests stay current. |
| Documentation | Docs Team | Keep manuals/architecture suite synced with code changes. |

### Release Checklist
1. Seal active segments and stage deployment plan (see maintenance handbook).
2. Run full test matrix: unit, failure, benchmarks, Python harness; capture results.
3. Update manuals, architecture/format docs, and changelog entries.
4. Align roadmap entry status and note upcoming automation actions.
5. Coordinate rollout announcement with Product & Support teams.

## Testing & Validation
- Required suites: unit (`cargo test`), failure tests (`--test aof2_failure_tests`), benchmarks (`cargo bench`), Python harness (see `aof2_python_test_runner.md`).
- Upcoming automation tasks (CLI golden tests, manifest fuzz harness, recovery drill scheduler) and ownership.
- Guidelines for adding new failpoints or harness scenarios.

## Operational Integration
- Coordinate with SRE for runbook updates when behaviour changes.
- Ensure telemetry additions align with observability dashboards.
- Update `maintenance-handbook.md` when maintenance procedures shift.

## Documentation Automation
- Run `scripts/docs_markdownlint.sh` or `./bake docs-lint` before submitting doc changes.
- Track future Vale/link-check integration via roadmap Near-Term items.
- Update manuals and review notes whenever automation tasks complete.

## Contribution Checklist
- Documentation updates: architecture, segment-format, manuals, changelog.
- Run `scripts/docs_markdownlint.sh` / `./bake docs-lint` plus code format/lints.
- Update `docs/bop-aof/progress_tracker.md` when milestones move.
- Record decisions in the Decision Log table below.

## Support & Escalation
- Ownership: Storage Platform architecture team.
- Escalation channel: `#bop-storage` and `#bop-docs`.
- On-call expectations for coordinating releases and hotfixes.

## Decision Log
| Date | Decision | Owner | Notes |
| --- | --- | --- | --- |
| 2025-09-25 | Documented ownership + release checklist | Storage Platform Architecture & Docs | Added responsibilities + release workflow. |
| 2025-09-25 | Developer manual initialised | Docs Team | Consolidated architecture/testing/process guidance. |

## Related Resources
- [Architecture](architecture.md)
- [Segment Format](segment-format.md)
- [Maintenance Handbook](maintenance-handbook.md)
- [Testing & Validation](testing-validation.md)
- [Tiered Operations Runbook](runbooks/tiered-operations.md)
- [Roadmap](roadmap.md)
- [BOP AOF User Manual](../bop-aof/user_manual.md)

## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-25 | Added code example scaffolding | Docs Team |
| 2025-09-25 | Added ownership + release checklist; documented SME feedback | Docs Team |
| 2025-09-25 | Initial developer manual draft | Docs Team |
