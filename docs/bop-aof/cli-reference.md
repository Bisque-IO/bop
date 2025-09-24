# BOP AOF CLI Reference

## Purpose
Catalog command-line tools for inspecting, maintaining, and troubleshooting bop-aof tiers. The reference covers default flags, sample outputs, and integration tips.

## Audience
- Site Reliability Engineers executing operational workflows.
- Developers scripting maintenance tasks or verifying deployments.
- Support engineers guiding customers through diagnostics.

## Prerequisites
- CLI binaries built via `cargo install --path crates/bop-aof-cli` or shipped artifacts.
- Permissions to read/write the AOF data root.
- Access to Tier 2 credentials when running commands that touch the object store.

## Command Summary
| Command | Description | Common Flags |
| --- | --- | --- |
| `aof2-admin dump` | Print tier residency snapshot (table + JSON) | `--root`, `--stream`, `--json-only` |
| `aof2-admin seal` | Force segment rollover for a stream | `--stream`, `--force` |
| `aof2-admin hydrate` | Hydrate a segment into Tier 1 | `--segment`, `--tier1-path` |
| `aof2-admin flush` | Trigger flush of pending writes | `--stream`, `--timeout` |
| `aof2-admin segments list` | List manifest segments with tier and size info | `--stream`, `--tier` |
| `aof2-admin segments delete` | Remove retired segments from Tier 1/2 | `--segment`, `--force`, `--tier` |
| `aof2-admin manifest check` | Validate manifest integrity and checksums | `--root`, `--repair` |

## Usage Examples
### Dump Residency
```bash
aof2-admin dump --root /var/lib/bop/aof2
```
Result snippet:
```
Tier 0 | stream demo | seg 42 | state active | bytes 134217728
Tier 1 | stream demo | seg 41 | state hydrated | bytes 134217728
...
{"tier0":{"segments":5},"tier1":{"segments":12},"tier2":{"segments":32}}
```

### Force Seal
```bash
aof2-admin seal --stream demo --force
```
Useful before rolling upgrades to ensure clean hand-off.

### Hydrate Segment
```bash
aof2-admin hydrate --segment 41 --tier1-path /var/lib/bop/aof2/tier1 --max-retries 5
```
Hydrates a cold segment and surfaces progress through progress logs.

## Exit Codes
| Code | Meaning | Action |
| --- | --- | --- |
| `0` | Success | N/A |
| `1` | Usage error | Re-run `--help` to verify flags. |
| `2` | Segment not found | Confirm manifest state via `segments list`. |
| `3` | Authentication failure | Check Tier 2 credentials. |
| `4` | IO error | Inspect filesystem permissions or disk health. |

## Configuration Overrides
Commands read from `~/.config/bop-aof/config.toml` by default. You can override via environment variables:
- `AOF2_CONFIG` — absolute path to an alternate config file.
- `AOF2_TIER2_ENDPOINT`, `AOF2_TIER2_ACCESS_KEY`, `AOF2_TIER2_SECRET_KEY` — override Tier 2 connection.
- `AOF2_TLS_CERT`, `AOF2_TLS_KEY` — specify TLS assets when secure transport is enabled.

## Scripting Tips
- Combine `--json-only` with `jq` for automation:
  ```bash
  aof2-admin dump --root /var/lib/bop/aof2 --json-only | jq '.tier2.segments'
  ```
- Wrap critical operations with retries using `retry` or shell loops when network partitions are expected.
- Record command invocations in incident timelines; the runbook references CLI outputs for validation.

## Troubleshooting
| Issue | Recommendation |
| --- | --- |
| CLI hangs on hydrate | Verify Tier 2 availability and ensure hydration workers are running; check logs. |
| `seal` reports active flush | Wait for flush backlog to clear or investigate `flush_failures`. |
| JSON output empty | Command filtered to specific stream/tier; double-check filters or run without flags. |

## Related Resources
- [Milestone 2 Review Notes](review_notes.md)
- [API Guide](api-guide.md)
- [Operations Guide](operations.md)
- [AOF2 Runbook](../aof2/aof2_runbook.md)

## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-24 | SME sign-off and command matrix validated | Docs Team |
| 2025-09-23 | Added command matrix, exit codes, and scripting guidance | Docs Team |
| 2025-09-23 | Added command coverage and usage examples | Docs Team |
