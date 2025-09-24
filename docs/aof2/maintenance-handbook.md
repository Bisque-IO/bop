# AOF2 Maintenance Handbook

## Summary
Playbook for storage and reliability engineers to perform planned maintenance on bop AOF2 clusters: budgeting tiers, performing upgrades, running compaction, and managing catalog snapshots.

## Maintenance Themes
1. Capacity tuning across tiers.
2. Lifecycle management (compaction, archival, deletion).
3. Upgrade coordination (binary, configuration, schema).
4. Catalog snapshots and integrity checks.

## Tier Budget Management
| Task | Frequency | Steps |
| --- | --- | --- |
| Review Tier 0 utilisation | Weekly | Compare `Tier0MetricsSnapshot.total_bytes` vs `cluster_max_bytes`; adjust config or rotate writers. |
| Adjust Tier 1 cache | Monthly | Evaluate `hydration_latency_p99_ms`; increase `Tier1Config::max_bytes` before raising Tier 0. |
| Tier 2 cost review | Monthly | Pull storage reports, archive cold segments per retention policy. |

### Procedure: Increase Tier 0 Budget
1. Collect latest `aof2-admin dump` and confirm hydration backlog is healthy.
2. Edit infra-managed config (e.g., `config/base/aof2.toml`) to raise `tier0.cluster_max_bytes`.
3. Deploy change via standard configuration pipeline.
4. Monitor `activation_queue_depth` and `total_bytes` for 30 minutes; revert if `Hydration` backpressure spikes.

## Lifecycle & Compaction
- Compaction choices are orchestrated via the coordinator; record outcomes in the manifest with `CustomEvent` entries.
- Use `aof2-admin segments delete` to retire Tier 1/Tier 2 artifacts post-compaction.
- Maintain catalog snapshots (`aof2_catalog_snapshots.md`) at least every 30 minutes for high-volume streams.

### Procedure: Catalog Snapshot Rotation
1. Trigger snapshot via control plane API (`POST /aof2/snapshot?stream=<id>`).
2. Verify `SnapshotMarker` manifest record appears with new `snapshot_id`.
3. Upload snapshot artifact to Tier 2 under `snapshots/<stream>/<snapshot_id>.json`.
4. Update recovery notes with new `chunk_index`/`chunk_offset`.

## Upgrade Coordination
| Upgrade Type | Checklist |
| --- | --- |
| Binary release | Seal active segments, announce to on-call, stage rollout, monitor `manifest.replay_version` and `flush_failures`. |
| Config change | Validate new values in staging, ensure `aof2-admin dump` reflects expectations, communicate via release channel. |
| Schema change | Gate behind feature flag, roll manifest readers first, then writers; backfill tests in `testing-validation.md`. |

## Snapshot & Recovery Hygiene
- Store snapshots and associated manifest chunks in off-site storage for disaster recovery.
- Perform quarterly recovery drills using the procedure in `testing-validation.md`.
- Record outcomes in the project tracker to update risk assessments.

## Tooling Inventory
| Tool | Purpose | Location |
| --- | --- | --- |
| `aof2-admin` | CLI for residency, seal, hydrate, delete | `crates/bop-aof-cli` |
| `manifest-inspect` | Inspect manifest chunks for diagnostics | `crates/bop-aof/src/manifest/inspect.rs` |
| `tier1-cache-util` | (Planned) recompress warm segments | Backlog item in `roadmap.md` |

## Communication & Change Management
- Log maintenance actions in the storage change calendar.
- Notify downstream service owners when Tier 2 retention policies shift.
- Capture learnings in `docs/bop-aof/review_notes.md` for the next public documentation update.

## Related Resources
- `runbooks/tiered-operations.md`
- `segment-format.md`
- `architecture.md`
- `../bop-aof/operations.md`

## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-25 | Added multi-region configuration guidance | Docs Team |
| 2025-09-24 | Initial consolidation of maintenance procedures | Docs Team |
