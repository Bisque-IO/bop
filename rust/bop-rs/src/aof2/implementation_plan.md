# AOF2 Implementation Plan

## Scope and Objectives
- Deliver the redesigned append-only file (AOF2) with manifest-free recovery, packed record identifiers, and lazy segment headers.
- Maintain compatibility with the current async runtime expectations (Tokio, memmap2) while minimising shared mutable state.
- Provide a staged path to production: foundation primitives, runtime functionality, durability guarantees, observability, and retention.
- Enforce deterministic footer placement and segment-count invariants so recovery can trust on-disk layout even when the filesystem is slow.
- Adapt segment sizing dynamically within a 64 MiB–1 GiB power-of-two window to match storage throughput while preserving rollover latency.

## Milestones
### Milestone 0 – Baseline (Completed)
- [x] Configuration and identifier packing (`SegmentId`, `RecordId`, helpers).
- [x] Error surface covering filesystem, recovery, and flush semantics.
- [x] Filesystem scaffolding (`Layout`, directory helpers, segment file naming, temp guards).

### Milestone 1 – Segment Primitives
- [x] Finalise `Segment::append_record` edge cases (overflow checks, timestamp handling, CRC folding validation).
- [x] Implement `Segment::seal` to flush, write the footer into the reserved tail slot, remap read-only, and flip status to `Finalized`.
- [x] Introduce `SegmentFooter` encode/decode utilities with checksum validation and footer-at-end expectations.
- [x] Provide recovery helpers: `Segment::load_header`, `Segment::scan_tail`, and a truncation routine for partial records.
- [x] Ensure `SegmentData` can remap between read-write (active) and read-only (sealed) views while protecting the footer reservation.

### Milestone 2 – AOF Runtime Pipeline
- [x] Flesh out `Aof::new` to optionally open existing directories, validate configuration bounds, and trigger recovery.
- [x] Implement append loop logic for rollover (`Segment::is_full`, active tail swap, preallocation pipeline) that respects the reserved footer space.
- [x] Enforce the “two unfinalized segments” invariant before activating additional preallocated segments.
- [x] Split hot-path append state from management state so the append fast path relies only on atomics.
- [x] Provide `wait_for_writable_segment(timeout)` / `append_record_with_timeout` so callers can opt into bounded waits.
- [x] Expose explicit APIs: `Aof::seal_active`, `Aof::force_rollover`, `Aof::flush_until(record_id)`.
- [x] Track in-memory catalog of segments (ordered by `SegmentId`) with cumulative counters only—no manifest writes.
- [x] Provide async-safe notification mechanism for tail changes (event `Notify` or channel broadcast).

### Milestone 3 – Flush and Durability Coordination
- [x] Define `AofManager` background thread structure and command queue types.
- [x] Implement per-segment flush scheduling using `FlushConfig` thresholds and expose finalisation confirmation events.
- [x] Ensure only one flush per segment runs concurrently and update `Segment::mark_durable` on completion.
- [x] Add `Aof::flush(record_id)` waiting semantics backed by durable byte tracking.
- [x] Integrate fsync/fdatasync calls with retry/backoff strategy.
  - Cover transient errors (`EINTR`, `EAGAIN`, `EBUSY`, timeouts) with bounded exponential backoff and jitter before giving up.
  - Surface fatal errors (`ENOSPC`, `EROFS`, integrity failures) with enough context for operators to take action and keep the backlog gauge accurate.
  - Track retry/success/failure counts in `FlushMetrics` so observability can correlate delay with disk behaviour.
- [x] Capture flush backlog snapshots into `FlushMetrics` so observability can expose latency vs. backlog correlations.
  - Record the live `backlog_bytes` gauge after every append, recovery pass, and flush completion.

### Milestone 4 – Recovery and Bootstrap
- [x] Walk `segments/` at startup, parse filenames, and sort by `SegmentId`.
- [x] Rebuild `next_segment_index`, `next_offset`, and `record_count` from headers, trusting all but the newest two segments.
- [x] Scan the tail segment (and penultimate if required) to truncate corrupt bytes and recompute cumulative counters.
- [x] Reopen the last active segment for appends, leaving others sealed and mapped read-only.
- [x] Seed the flush backlog counter and manager queue from recovered durability markers so restarted nodes resume pending work.
- [x] Provide unit/integration tests that model crash states (torn headers, partial payload writes, missing footers).

### Milestone 5 – Reader APIs
- [ ] Define synchronous reader over sealed segments that accepts `RecordId` (packed `{segment_index, segment_offset}`) and validates checksums.
- [ ] Implement tail-following async reader that registers for notifications and resumes when new segments appear.
- [ ] Add optional sparse index generation on seal to accelerate `seek(record_id)` for large segments.

#### Execution Outline
- Build a `reader` module with a `SegmentReader` struct that wraps sealed `Arc<Segment>` handles, decodes record headers via `Segment::record_end_offset`, and enforces CRC verification before exposing payloads.
- Introduce an internal `RecordCursor` that advances by header-derived lengths and returns borrowed slices when the segment is sealed, falling back to owned buffers for the active tail.
- Extend `Aof` with lightweight catalog accessors (`open_reader`, `segment_snapshot`) so readers discover sealed segments without racing the writer fast path.
- Provide retry-aware helpers for the async tail follower that subscribe to `TailSignal`, wait for `TailEvent::Activated`, and bridge into the synchronous reader when segments finalize.

#### Integration Points
- Reuse `SegmentStatusSnapshot` to describe available data to readers and keep reader state aligned with the manager's pending-finalize queue.
- Share the existing `FlushMetrics` backlog gauge to surface reader catch-up lag and expose hooks for eventual telemetry wiring.
- Gate sparse-index generation behind a feature flag so it can evolve independently of the baseline reader work.

#### Validation Strategy
- Unit tests over `SegmentReader` exercising checksum failures, zero-copy slices, and `RecordId` boundary checks.
- Integration tests that roll segments, seal them, and confirm tail followers resume reading without duplicating or skipping records.
- Property and soak tests that iterate randomized record sizes to ensure cursor math never escapes the logical segment size.

### Milestone 6 – Retention and Archival
- [ ] Implement retention evaluator for `RetentionPolicy` variants (size-based, time-based, keep-all).
- [ ] Add archival move/copy into `archive/`, ensuring atomic renames and fsync of parent directories.
- [ ] Provide hooks so retention runs in background without blocking the writer, coordinated via `AofManager`.

### Milestone 7 – Observability and Tooling
- [ ] Instrument append, flush, seal, and recovery paths with `tracing` spans and metrics counters.
- [ ] Expose metrics (e.g., via internal struct or optional feature) for append throughput, flush latency, and segment sizing decisions.
- [ ] Create administrative helpers or CLI hooks to inspect segment state and trigger manual rollover.
- [ ] Document operational runbooks (recovery steps, retention configuration, monitoring guidelines).

## Near-Term Focus (Post-M4)
1. Finish `QA4` by covering recovery-triggered `flush_until` completions now that queue-saturation coverage exists in the Rust harness.
2. Capture recovery backlog reconciliation as deterministic fixtures to lock in tail validation guarantees before layering reader surfaces.
3. Start `RD2a` by drafting the async tail follower that consumes `TailEvent` notifications and reuses the new reader path.
4. Kick off `OB1` tracing/metrics wiring so the provisional exporter has downstream consumers ready.

## Risk Tracking
- Retry loops must never starve the flush queue; guard with max-attempt threshold and alerting via metrics.
- Recovery walk relies on consistent segment naming—add sanity checks so unexpected files do not block startup.
- Flush backlog seeding on restart introduces race risk with fresh appends; verify atomic ordering when merging recovered counters with live state.

## Detailed Task List
| Task ID | Area | Description | Deliverable | Status | Dependencies |
|---------|------|-------------|-------------|--------|--------------|
| CFG1    | Config  | Add `segment_min_bytes`/`segment_max_bytes` validation enforcing power-of-two bounds (64 MiB–1 GiB) and clamp `segment_target_bytes`. | `config.rs` updates + tests | done | Milestone 1 |
| S1      | Segment | Write `SegmentFooter` struct with encode/decode/checksum helpers fixed to the tail slot. | `segment.rs` footer module + tests | done | Milestone 1 |
| S2      | Segment | Implement `Segment::seal` with footer emission at `max_size - FOOTER_SIZE` and remap to read-only. | Updated `segment.rs`, unit tests | done | S1 |
| S3      | Segment | Add tail scan + truncation helper for recovery. | `segment.rs` recovery API + tests | done | S1 |
| S4      | Segment | Guard record writes so they never overlap the reserved footer region. | `segment.rs` bounds checks + tests | done | S1 |
| R1      | Runtime | Expand `Aof::new` to detect existing segments and call recovery. | `mod.rs` updates + integration test | done | S1, S3 |
| R2      | Runtime | Handle rollover path (`Segment::append_record` -> `SegmentFull`) while honouring footer reservation. | `mod.rs` append loop, tests | done | S2, S4 |
| R3      | Runtime | Implement `Aof::seal_active` and state transitions. | `mod.rs` API + tests | done | R2 |
| R4      | Runtime | Enforce the two-unfinalized invariant before activating additional segments. | `mod.rs`/manager coordination + tests | done | R2 |
| R5      | Runtime | Implement dynamic segment size selection within configured bounds (heuristics + telemetry). | `mod.rs` sizing logic + tests | done | CFG1 |
| R6      | Runtime | Split hot-path append state from management mutex so appends rely on atomics only. | `mod.rs` refactor + docs | done | R2 |
| R7      | Runtime | Implement `wait_for_writable_segment(timeout)` and `append_record_with_timeout`. | `mod.rs` API + tests | done | R6 |
| F1      | Flush | Define manager command enums, lock-free queues, and `SegmentFlushState` registry for concurrent streams. | `flush.rs`/`mod.rs` updates | done | R2 |
| F2      | Flush | Implement flush scheduling that respects `FlushConfig`. | Manager + segment updates | done | F1 |
| F3      | Flush | Provide `Aof::flush(record_id)` waiting semantics. | `mod.rs` API + tests | done | F2 |
| F4      | Flush | Add retry/backoff wrapper around `fsync`/`fdatasync`, expose retry counters via `FlushMetrics`, and gate persistent failures behind structured errors. | `flush.rs` updates + tests | done | F1, F2 |
| RC1     | Recovery | Walk filesystem, parse headers, rebuild counters. | `mod.rs` recovery integration + tests | done | S3 |
| RC2     | Recovery | Tail verification with optional footer trust. | `segment.rs` recovery API + tests | done | RC1 |
| RC3     | Recovery | Seed flush backlog/manager queues from recovered durability markers so restart resumes pending work. | `mod.rs` recovery integration + tests | done | RC1 |
| RD1     | Reader | Implement sealed-segment reader API returning payload slices. | `reader.rs` updates + tests | todo | R2 |
| RD2     | Reader | Async tail follower with packed id seek support. | `reader.rs` + integration tests | todo | RD1 |
| RD1a    | Reader | Scaffold `reader.rs` with `SegmentReader` decoding record headers using existing segment helpers. | `reader.rs` skeleton + unit tests | done | RD1 |
| RD1b    | Reader | Implement checksum enforcement and zero-copy payload borrowing for sealed segments. | `reader.rs` implementation + proptest coverage | done | RD1a |
| RD1c    | Reader | Expose `Aof::open_reader` and hook reader catalog updates into `TailSignal`. | `mod.rs` updates + integration tests | done | RD1a |
| RD2a    | Reader | Build async tail follower that streams sealed segments on `TailEvent` notifications and hands off to the active tail. | `reader.rs` async API + tests | in_progress | RD2 |
| RT1     | Retention | Evaluate retention policy and queue archival actions. | `flush.rs` or new module + tests | todo | F1 |
| RT2     | Retention | Move sealed segments into `archive/` atomically. | `fs.rs` helpers + tests | todo | RT1 |
| OB0     | Observability | Surface `FlushMetricsSnapshot` retry/backlog counters via a provisional exporter struct for telemetry consumers. | `mod.rs` metrics adapter + docs | done | F4 |
| OB1     | Observability | Add tracing spans/counters for key operations including dynamic sizing and finalization lag. | Instrumentation across modules | todo | R2, F2, R5 |
| OB2     | Observability | Produce docs + sample metrics wiring. | Markdown docs | todo | OB1 |
| QA1     | Testing | Build integration tests covering append/read, rollover, crash recovery, and two-unfinalized enforcement. | `tests/` additions | todo | Depends on Milestones 1–4 |
| QA2     | Testing | Implement property-based tests for record/header parsing and footer placement. | `tests/` + proptest harness | todo | S1, RC2 |
| QA3     | Testing | Add benchmark suite for append throughput and flush latency under varying segment sizes. | Criterion bench + scripts | todo | R2, F2, R5 |
| QA4     | Testing | Expand `flush_until` tests to cover backlog saturation, recovery-triggered completion, and retry accounting (queue/restart coverage landed; retry accounting pending). | Integration tests + harness updates | in_progress | F3, F4, RC3 |
| QA5     | Testing | Add reader integration tests covering sealed-segment reads, rollover handoff, and checksum mismatch handling. | `tests/` additions | todo | RD1c, RD2a |

## Testing and Verification Strategy
- Unit coverage: segment header/footer encode/decode, CRC folding, `RecordId` packing helpers, and reserved footer bounds enforcement.
- Integration coverage: multi-segment append/read, recovery after simulated crash scenarios, flush durability guarantees, and confirmation that no more than two segments remain unfinalized.
- Reader validation: sealed-segment iteration, tail follower handoff under rollover, and CRC mismatch detection for corrupted payloads.
- Stress and soak: long-running append with periodic fsync to ensure counters stay consistent, verified via metrics that track segment sizing decisions and finalization lag.
- Property-based validation: randomized payload and truncation to confirm recovery never exposes corrupt records.
- Benchmarking: optional gated benchmarks to measure throughput with varying segment sizes and flush intervals.
- Recovery regression harness: restart from persisted backlog snapshots to ensure flush manager convergence.
- `flush_until` queue saturation tests: simulate synchronous fallback paths and confirm retry counters stay bounded.

## Tooling and Documentation
- Extend developer README with instructions for configuring storage directories and running the new tests.
- Provide troubleshooting guide for recovery warnings, footer corruption, and finalization backpressure.
- Capture operational defaults (segment size band, flush thresholds) in configuration docs once runtime stabilises.
- Document expected telemetry outputs for backlog counters, retry rates, and segment rollover cadence.




