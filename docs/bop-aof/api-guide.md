# BOP AOF API Guide

## Purpose
Describe the Rust crates and FFI surfaces that expose append, read, manifest, and maintenance capabilities so service owners can integrate bop-aof safely.

## Audience
- Rust engineers embedding bop-aof into services.
- SDK maintainers translating APIs to other languages.
- Documentation contributors curating examples for public distribution.

## Prerequisites
- Familiarity with async Rust (Tokio) and the BOP platform's tracing/metrics conventions.
- Understanding of tier terminology introduced in the [Overview](overview.md).
- Access to the source tree under `crates/bop-aof` and associated integration tests.

## Module Map
| Module | Description | Key Types |
| --- | --- | --- |
| `append` | Handles write-path clients and flush coordination | `AppendClient`, `AppendConfig` |
| `read` | Provides async readers with backpressure signaling | `Reader`, `ReadConfig`, `BackpressureKind` |
| `store::tier0` | Manages in-memory mmap segments and eviction | `Tier0Cache`, `Tier0MetricsSnapshot` |
| `store::tier1` | Compressed warm cache handling hydration | `Tier1Cache`, `HydrationTask` |
| `store::tier2` | Object store integration and retry policy | `Tier2Client`, `RetryPolicy` |
| `manifest` | Segment lifecycle ledger and inspectors | `ManifestWriter`, `ManifestInspector` |
| `ffi` | C ABI surface for non-Rust consumers | `bop_aof_open`, `bop_aof_append` |

## Quick Start Examples
### Append & Flush
```rust
use bop_aof::append::{AppendClient, AppendConfig};

async fn append_message(endpoint: &str, payload: &[u8]) -> anyhow::Result<()> {
    let cfg = AppendConfig { endpoint: endpoint.into(), ..Default::default() };
    let client = AppendClient::connect(cfg).await?;
    client.append(payload).await?;
    client.flush().await?;
    Ok(())
}
```

### Reader with Backpressure Awareness
```rust
use bop_aof::read::{Reader, ReaderBuilder, BackpressureKind};

async fn consume(stream: &str) -> anyhow::Result<()> {
    let mut reader = ReaderBuilder::new(stream).connect().await?;
    while let Some(item) = reader.next().await {
        match item {
            Ok(record) => handle(record),
            Err(err) if err.backpressure() == Some(BackpressureKind::Hydration) => {
                tracing::warn!(?err, "hydration pending; backing off");
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}
```

### Manifest Inspection
```rust
use bop_aof::manifest::inspect::ManifestInspector;

fn list_segments(manifest_path: &str) -> anyhow::Result<()> {
    let inspector = ManifestInspector::open(manifest_path)?;
    for seg in inspector.segments() {
        println!("segment {} tier {:?} size {}", seg.id, seg.tier, seg.len);
    }
    Ok(())
}
```

## Error Handling & Backpressure
- `BackpressureKind::Admission`: Tier 0 full; clients should throttle or allow rewinds.
- `BackpressureKind::Hydration`: Tier 1 backlog; retry with exponential backoff.
- `AofError::Retryable`: safe to retry respecting configured policy.
- `AofError::Fatal`: escalate immediately; inspect runbook for mitigation.

Recommended retry strategy:
```rust
use retry::{retry, delay::Fibonacci};
let result = retry(Fibonacci::from_millis(50).take(5), || async {
    client.append(payload).await.map_err(retry::Operation::from)
}).await;
```

## Configuration Reference
| Struct | Key Fields | Notes |
| --- | --- | --- |
| `Tier0CacheConfig` | `cluster_max_bytes`, `activation_queue_warn_threshold` | Sizing guidance in [Operations Guide](operations.md). |
| `Tier1Config` | `max_bytes`, `max_retries`, `retry_backoff` | Keep retries low to avoid masking Tier 2 failures. |
| `Tier2Config` | `retry`, `concurrency`, `bucket` | Align with object store SLAs. |
| `RetryPolicy` | `max_attempts`, `base_delay_ms`, `jitter` | Shared by upload/download/delete operations. |

Configuration can be loaded from TOML:
```rust
#[derive(serde::Deserialize)]
struct Settings {
    #[serde(default)]
    aof: bop_aof::config::AofManagerConfig,
}
```

## FFI Surface
- C ABI exposes `bop_aof_open`, `bop_aof_append`, `bop_aof_close`.
- Header files are generated into `target/include/bop_aof.h` during `cargo build --features ffi`.
- Consumers must link against `libbop_aof_ffi.a` and manage thread-safe retries similar to Rust clients.

## Compatibility & Upgrades
- Manifest schema is forward-compatible; new fields append with versioned headers.
- Segment compression defaults to Zstd level 8; ensure decompression support when integrating external readers.
- Rolling upgrades: seal active segments before deploying new binaries; monitor `manifest.replay_version` metrics.

## Troubleshooting
| Scenario | Guidance |
| --- | --- |
| Reader receives `BackpressureKind::Tail` repeatedly | Flush backlog may be high; coordinate with operations to increase flush workers. |
| Manifest open fails with checksum error | Run manifest validation tooling (`aof2-admin manifest check`) and consult the runbook for repair steps. |
| FFI consumer experiences deadlocks | Ensure callbacks execute off the bop-aof runtime threads; avoid blocking on async functions. |

## Related Resources
- [Milestone 2 Review Notes](review_notes.md)
- [CLI Reference](cli-reference.md)
- [Operations Guide](operations.md)
- [AOF2 Tiered Store Design](../aof2/aof2_store.md)
- [Runbook](../aof2/aof2_runbook.md)

## Changelog
| Date | Change | Author |
| --- | --- | --- |
| 2025-09-24 | SME sign-off and polish pass applied | Docs Team |
| 2025-09-23 | Added module map, error handling guidance, and configuration references | Docs Team |
| 2025-09-23 | Drafted code examples and API focus areas | Docs Team |
