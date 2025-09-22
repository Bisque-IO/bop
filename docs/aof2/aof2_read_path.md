# AOF2 Read Path Guide

## Overview
- Documents how `Aof::open_reader` and `Aof::open_reader_async` traverse the tiered store and where `WouldBlock` propagates.
- Complements the state machines in `aof2_design_next.md` and the hydration/notification details in `aof2_store.md`.
- Intended for service owners building on the new reader APIs ahead of recovery milestones.
- Recovery now follows the pointer-first workflow described in `aof2_catalog_snapshots.md`: the reader catalog is rebuilt from headers/footers after consulting `segments/current.sealed`, so sealed segments become available before the manifest replay finishes.

## Glossary
- **ResidentSegment** - reference-counted handle returned by `TieredInstance`; provides access to the memory map and shared `SegmentFlushState`.
- **Sealed Segment Stream** - async iterator over finalized segments (`rust/bop-rs/src/aof2/reader.rs:505`), used by long-lived tail readers.
- **TailFollower** - wrapper around `TailSignal` updates (`rust/bop-rs/src/aof2/mod.rs:604`) that surfaces activation, sealing, and rollover events.
- **BackpressureKind** - enum describing why an operation returned `WouldBlock` (`rust/bop-rs/src/aof2/error.rs:6`). Current values: `Admission`, `Rollover`, `Hydration`, `Flush`, `Tail`, `Unknown`.
- **Hydration Notifier** - per-instance `Notify` registered with the coordinator to wake waiters when a requested segment becomes resident.

## Synchronous Read Flow
1. Callers invoke `Aof::open_reader(segment_id)`.
2. The method checks the in-memory catalog (`rust/bop-rs/src/aof2/mod.rs:724`). If the `ResidentSegment` is still present, a `SegmentReader` is constructed immediately.
3. On a cache miss, it delegates to `TieredInstance::checkout_sealed_segment`. Outcomes:
   - `SegmentCheckout::Ready(resident)` gives an immediate reader; the caller can iterate records at once.
   - `SegmentCheckout::Pending(waiter)` returns `AofError::WouldBlock(BackpressureKind::Hydration)`.
4. Callers must decide whether to retry, back off, or switch to the async helper. Recommended retry strategy:
   - Wait on the hydration notifier exposed via `TieredCoordinatorNotifiers::hydration(instance_id)`.
   - Apply exponential backoff capped at a few hundred milliseconds to avoid thundering herds on cold tiers.
5. When the active segment is sealing, `open_reader` returns `WouldBlock(BackpressureKind::Rollover)` until the rollover ack fires.

### Error Surface
- `AofError::NotFound` - the catalog and tiers could not locate the requested segment; treat as unrecoverable.
- `AofError::WouldBlock(BackpressureKind::Hydration)` - wait for hydration notifier or escalate to async helper.
- `AofError::WouldBlock(BackpressureKind::Rollover)` - resume when the rollover notifier signals completion.
- `AofError::WouldBlock(BackpressureKind::Flush)` - the flush worker exhausted retries and raised the shared failure flag. Watch `FlushMetricsSnapshot::flush_failures` and the `aof_flush_metadata_*` counters, wait for them to settle, or call `flush_until` on the stalled record before retrying.

## Async Read Flow
1. `Aof::open_reader_async(segment_id)` follows the same steps but awaits hydration (`rust/bop-rs/src/aof2/mod.rs:769`).
2. `SegmentCheckout::Pending` is awaited inline; wakers are registered with the hydration notifier.
3. Once a `SegmentReader` is available, the future resolves and the caller can iterate synchronously.
4. `SealedSegmentStream::next_record_async` bridges sealed segments to async consumers, transparently awaiting hydration for each segment.
5. Streaming clients that tail the log should pair `TailFollower` with `ActiveSegmentCursor` to watch churn on the active segment while awaiting durability.

### Cancellation Safety
- All async loops respect the manager cancellation token; shutdown manifests as `AofError::WouldBlock(kind)` so callers can decide whether to retry or abort.
- Avoid holding `SegmentReader` across await points; capture the cursor state you need before async work.

## Backpressure Semantics
| BackpressureKind | Source | Resolution Strategy |
| ---------------- | ------ | ------------------ |
| `Admission` | `TieredInstance::admission_guard` rejects due to Tier 0 pressure | Wait on `TieredCoordinatorNotifiers::admission(instance_id)` or slow the producer. Applies mainly to writers but read-side admission is exposed for completeness. |
| `Hydration` | `TieredInstance::checkout_sealed_segment` returns `Pending` | Await hydration notifier; optionally enqueue a prefetch via `TieredCoordinator` for anticipated readers. |
| `Rollover` | Active segment is sealing (`Aof::queue_rollover`) | Await rollover notifier; readers should retry after a short delay or rely on async helper. |
| `Flush` | Durable watermark trails requested bytes (AP4) | Let the manager drive a flush, watch the `aof_flush_metadata_*` counters, or call the planned `await_flush(record_id)` helper once exposed. |
| `Tail` | Tail follower outran durability signal | Retry after `TailSignal` publishes the next event; typically transient during heavy write bursts. |

## Integration Patterns
### gRPC Services
- Map `WouldBlock` to `UNAVAILABLE` with a retryable status detail containing the `BackpressureKind`.
- Propagate a suggested backoff (for example start at 10 ms, cap at 250 ms) based on hydration queue metrics.

### HTTP / REST APIs
- Return `503 Service Unavailable` with a `Retry-After` header proportional to the observed delay.
- For streaming endpoints, emit server-sent events or chunked updates only after a successful retry to avoid partial responses.

### Batch / Offline Consumers
- Prefer `open_reader_async` and await hydration to avoid spinning.
- When iterating many segments, batch hydration requests by pre-registering waiters via `TieredInstance::prefetch_sealed` (planned) or by respecting the sealed stream ordering.

## Code Examples
```rust
use std::time::Duration;
use tracing::instrument;

use crate::aof2::{
    Aof, AofError, AofResult, BackpressureKind, InstanceId, SegmentId, SegmentReader,
    TieredCoordinatorNotifiers,
};

#[instrument(skip(aof, notifiers))]
pub async fn read_segment_with_backoff(
    aof: &Aof,
    notifiers: &TieredCoordinatorNotifiers,
    instance_id: InstanceId,
    segment_id: SegmentId,
) -> AofResult<SegmentReader> {
    let mut delay = Duration::from_millis(10);
    loop {
        match aof.open_reader(segment_id) {
            Ok(reader) => return Ok(reader),
            Err(AofError::WouldBlock(BackpressureKind::Hydration)) => {
                notifiers.hydration(instance_id).notified().await;
            }
            Err(AofError::WouldBlock(kind @ BackpressureKind::Rollover)) => {
                tracing::debug!(?kind, delay_ms = delay.as_millis(), "rollover in progress; retrying");
                tokio::time::sleep(delay).await;
            }
            Err(err) => return Err(err),
        }
        delay = (delay * 2).min(Duration::from_millis(250));
    }
}
```

```rust
pub async fn tail_stream(mut stream: SealedSegmentStream<'_>) -> AofResult<()> {
    while let Some(segment) = stream.next_segment().await? {
        match segment {
            StreamSegment::Sealed(mut cursor) => {
                while let Some(record) = cursor.next()? {
                    process_record(record);
                }
            }
            StreamSegment::Active(mut cursor) => {
                while let Some(record) = cursor.next()? {
                    process_record(record);
                }
            }
        }
    }
    Ok(())
}
```

## Observability Hooks
- `reader.rs` exposes counters for hydration retries and tail catches; wire them into your telemetry sink.
- The Tier 1 metrics snapshot now includes hydration queue depth and latency percentiles (`hydration_latency_p50_ms`, `p90`, `p99`). Track these alongside the residency gauge to spot segments stuck in `NeedsRecovery`.
- Trace spans wrap hydration waits and sealed stream transitions, making it easy to visualise reader stalls.
- Monitor `FlushMetricsSnapshot::scheduled_backpressure` to correlate reader stalls with outstanding writer flushes.
- Monitor `FlushMetricsSnapshot::flush_failures` alongside the flag on the append path; a non-zero value means writers are blocked on durability and readers should expect `WouldBlock(BackpressureKind::Flush)` until the queue drains.
- The `aof_flush_metadata_*` counters surface tier metadata persistence stalls; alert when retries climb so clients can widen backoff before reads see `WouldBlock(BackpressureKind::Flush)`.
- Structured logs under `aof2::tier1` (`tier1_hydration_retry_*`, `tier1_hydration_failed`, `tier1_hydration_success`) provide a breadcrumb trail for hydration behaviour. Filter on these events when debugging cold reads.
- `aof2-admin dump --root <path>` prints the same metrics and manifest residency on demand so operators can capture point-in-time state alongside log correlations.

## Migration Notes
- Legacy callers that expected blocking IO must add retry loops or switch to the async helpers.
- Integration tests should exercise both the happy path and `WouldBlock` returns to verify client behaviour.
- Document the new retry contract in downstream service READMEs so operators know how to tune backoff windows.
- Ensure restart flows wait for the pointer/bootstrap step to complete before issuing read traffic. Once `current.sealed` is processed the sealed catalog is immediately available; manifest replay simply restores Tier1/Tier2 residency.
- Feed real coordinator watermarks and external segment ids into `Aof::metadata_handle()` (or the manager-level handle) so readers observe the upstream position after restart rather than the test literals used during bring-up.

## Review Checklist
- [ ] Glossary terms align with the append/read diagrams in `aof2_design_next.md`.
- [ ] Code examples compile (convert to doctests once tier metadata work lands).
- [ ] Platform and recovery teams sign off before marking RD5 complete.
