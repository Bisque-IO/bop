# Getting Started with BOP AOF

## Purpose
Provide a structured path to install prerequisites, run the tiered store locally, and validate reads/writes with observability wired in.

## Audience
- Application developers integrating bop-aof backed services.
- QA and release engineering validating onboarding flows.
- Technical writers preparing tutorials for public consumption.

## Prerequisites
- Rust toolchain 1.78+ (`rustup show` to confirm).
- `xmake`, `cmake`, and build essentials (use `./bake setup`).
- Access to an S3-compatible object store (AWS S3, MinIO) with API credentials.
- Optional: Docker Desktop for running MinIO locally.

## Bootstrap Steps
### 1. Install Tooling
```bash
./bake setup
```
Generates `compile_commands.json`, installs dependencies, and ensures `xmake` is available.

### 2. Configure Build
```bash
xmake f -m debug
xmake
```
Produces development binaries with symbols for easier debugging.

### 3. Seed Configuration
Copy example configuration and adjust tier budgets and credentials:
```bash
mkdir -p ~/.config/bop-aof
cp examples/aof2-demo/config/local.toml ~/.config/bop-aof/config.toml
$EDITOR ~/.config/bop-aof/config.toml
```
Key values:
- `tier0.cluster_max_bytes`
- `tier1.max_bytes`
- `tier2.endpoint`, `access_key`, `secret_key`

### 4. Launch Demo Ingest
```bash
cargo run --bin aof2-demo -- --records 1000 --stream demo
```
Produces sample segments, seals, and hydrates them through Tier 0/1/2.

### 5. Inspect Residency
```bash
cargo run --bin aof2-admin -- dump --root /var/lib/bop/aof2
```
Verify the JSON footer shows the expected number of segments per tier.

### 6. Observe Metrics
- Import the forthcoming Grafana dashboard (`docs/bop-aof/media/aof2_grafana.json`).
- Confirm `activation_queue_depth`, `hydration_latency_p99_ms`, and Tier 2 retry counters update during the demo run.

## Enabling Tier 2 Locally
If you do not have a managed object store, run MinIO:
```bash
docker run -p 9000:9000 -p 9001:9001   -e MINIO_ROOT_USER=miniouser   -e MINIO_ROOT_PASSWORD=miniosecret   -v ~/minio/data:/data   quay.io/minio/minio server /data --console-address ":9001"
```
Update the config file to point to `http://127.0.0.1:9000` and export credentials:
```bash
export AOF2_TIER2_ENDPOINT=http://127.0.0.1:9000
export AOF2_TIER2_ACCESS_KEY=miniouser
export AOF2_TIER2_SECRET_KEY=miniosecret
```
Rerun the demo ingest to confirm uploads succeed.

## Topology Diagram
```mermaid
flowchart TD
    Dev[Developer Workstation] -->|clone repo| Repo[bisque-io/bop]
    Dev -->|run scripts| Tooling[bake / xmake]
    Tooling -->|build binaries| LocalTier0[Tier 0 (mmap)]
    Tooling -->|configure| LocalTier1[Tier 1 cache]
    LocalTier1 -->|upload| Tier2[(Tier 2 Object Store)]
    Dev -->|execute demos| CLI[aof2-admin / demos]
    CLI -->|inspect| LocalTier0
    CLI -->|hydrate| LocalTier1
    Tier2 -->|rehydrate| LocalTier1
```
_Source: `docs/bop-aof/media/getting-started-topology.mmd`._

## Developing Against the Crate
- Add `crates/bop-aof` as a dependency in your workspace and enable the feature flags required for your workload (`tiered`, `metrics`, `ffi`).
- Use the API snippets in the [API Guide](api-guide.md) to wire append/reader clients.
- Run `cargo test --package bop-aof -- --ignored` to exercise integration tests.

## Troubleshooting
| Symptom | Likely Cause | Fix |
| --- | --- | --- |
| `WouldBlock(Admission)` on ingest | Tier 0 full | Increase Tier 0 budget or seal segments faster (`aof2-admin seal`). |
| `WouldBlock(Hydration)` on read | Tier 1 backlog | Verify Tier 1 residency, increase workers, or check Tier 2 connectivity. |
| CLI fails with `permission denied` | Missing directory access | Run commands with appropriate user or adjust data root path. |
| Tier 2 upload errors | Credentials or endpoint mismatch | Confirm environment variables and object store availability. |

## Next Steps
- Review the [API Guide](api-guide.md) to embed the client in your service.
- Follow the [Operations Guide](operations.md) to align monitoring before staging rollout.
- Schedule a walkthrough with the Docs Lead if onboarding friction persists.

## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-24 | SME sign-off and topology diagram added | Docs Team |
| 2025-09-23 | Added full bootstrap flow, MinIO walkthrough, and troubleshooting matrix | Docs Team |
| 2025-09-23 | First-wave onboarding draft created | Docs Team |

## Related Resources
- [Milestone 2 Review Notes](review_notes.md)
- [API Guide](api-guide.md)
- [CLI Reference](cli-reference.md)
- [Operations Guide](operations.md)
- [AOF2 Runbook](../aof2/aof2_runbook.md)

