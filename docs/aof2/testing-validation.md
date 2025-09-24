# AOF2 Testing & Validation

## Summary
Centralised guide for exercising AOF2 through unit tests, integration suites, and operational drills. Consolidates context from `aof2_python_test_runner.md`, benchmark notes, and milestone plans.

## Test Matrix
| Area | Goal | Entry Point |
| --- | --- | --- |
| Unit tests | Validate core data structures (manifest, tier caches, retry logic). | `cargo test --package bop-aof` |
| Failure integration tests | Exercise tier retries, crash recovery, hydration backoffs. | `cargo test --manifest-path crates/bop-aof/Cargo.toml --test aof2_failure_tests` |
| Python harness | End-to-end disk + object store simulations with fault injection. | `docs/aof2/aof2_python_test_runner.md` |
| Benchmarks | Measure append throughput, hydration latency. | `cargo bench --manifest-path crates/bop-aof/Cargo.toml --bench aof_benchmarks` |
| Disaster recovery drill | Replay from snapshot and manifest after simulated outage. | See "Operational Drills" below. |

## Environment Setup
- Use `./bake setup` followed by `xmake f -m release && xmake` for release builds.
- For failure tests, configure local MinIO (see `../bop-aof/getting-started.md`).
- Export `AOF2_TEST_LOG=info` to capture structured telemetry during runs.

## Continuous Integration Expectations
- Unit and failure tests run in CI (link to workflow TBD).
- Markdown documentation updates require `scripts/docs_markdownlint.sh` (see `.github/workflows/docs-lint.yml`).
- Future addition: lint manifest schemas using `cargo xtask check-manifest` (tracked in `roadmap.md`).

## Recovery Drill Diagram
```mermaid
```
_Source: `docs/aof2/media/recovery-drill.mmd`._

## Operational Drills
### Quarterly Recovery Simulation
1. Snapshot active streams using control plane API.
2. Simulate outage by wiping Tier 0/1 caches; keep Tier 2 objects.
3. Bring nodes back online, replay manifest from snapshot offset.
4. Validate hydration metrics return to steady state within SLA.
5. Record findings, update `maintenance-handbook.md` if new steps emerge.

### Hydration Backpressure Test
1. Run load generator to saturate Tier 1 hydration queue.
2. Confirm `BackpressureKind::Hydration` surfaces to clients and dashboards signal latency.
3. Tune `Tier1Config::max_retries` / worker counts; rerun to ensure mitigation works.

## Tooling & Harnesses
- Python harness (`aof2_python_test_runner.md`) wraps CLI + manifest parsing for deterministic failure injection.
- `tests/aof2_failure_tests.rs` enumerates scenarios with `#[cfg(feature = "failpoints")]` to guard environment-specific runs.
- Benchmark harness exposes `aof2_append_metrics` and `hydrate_segment_latency` to compare releases.

## Coverage Gaps & TODOs
- Automate manifest log fuzzing using `cargo fuzz` (backlog item).
- Expand chaos tests for Tier 2 partial outages with long-lived retries.
- Capture CLI golden outputs to detect regressions (once CLI stabilises post-M2).

## Upcoming Automation
- **CLI Golden Tests:** capture reference JSON/TTY output for `aof2-admin` commands and assert via snapshot tests (tracked for next release).
- **Manifest Fuzz Harness:** integrate `cargo fuzz` corpus leveraging `segment-format.md` schema; run nightly on staging hardware.
- **Recovery Drill Scheduler:** automate the quarterly drill via CI pipeline, publishing metrics to the storage perf dashboard.

## Reporting & Dashboards
- Publish junit XML from failure tests for aggregation.
- Feed benchmark results into the storage perf dashboard; annotate releases.

## Related Resources
- `maintenance-handbook.md`
- `aof2_python_test_runner.md`
- `roadmap.md`

## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-25 | Added automation follow-up plan (CLI golden, fuzz harness, drill) | Docs Team |
| 2025-09-24 | Added recovery drill diagram and test matrix consolidation | Docs Team |
| 2025-09-24 | Initial consolidated testing guide | Docs Team |

## Documentation Automation
- `scripts/docs_markdownlint.sh`: run locally before opening PRs to catch markdown style issues.
- Future: integrate vale or link checkers (tracked in roadmap).
- Ensure manuals include glossary and release notes pointers to reduce duplication.
