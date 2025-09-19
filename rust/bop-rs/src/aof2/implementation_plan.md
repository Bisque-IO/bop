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
- [ ] Finalise `Segment::append_record` edge cases (overflow checks, timestamp handling, CRC folding validation).
- [ ] Implement `Segment::seal` to flush, write the footer into the reserved tail slot, remap read-only, and flip status to `Finalized`.
- [ ] Introduce `SegmentFooter` encode/decode utilities with checksum validation and footer-at-end expectations.
- [ ] Provide recovery helpers: `Segment::load_header`, `Segment::scan_tail`, and a truncation routine for partial records.
- [ ] Ensure `SegmentData` can remap between read-write (active) and read-only (sealed) views while protecting the footer reservation.

### Milestone 2 – AOF Runtime Pipeline
- [ ] Flesh out `Aof::new` to optionally open existing directories, validate configuration bounds, and trigger recovery.
- [ ] Implement append loop logic for rollover (`Segment::is_full`, active tail swap, preallocation pipeline) that respects the reserved footer space.
- [ ] Enforce the “two unfinalized segments” invariant before activating additional preallocated segments.
- [ ] Expose explicit APIs: `Aof::seal_active`, `Aof::force_rollover`, `Aof::flush_until(record_id)`.
- [ ] Track in-memory catalog of segments (ordered by `SegmentId`) with cumulative counters only—no manifest writes.
- [ ] Provide async-safe notification mechanism for tail changes (event `Notify` or channel broadcast).

### Milestone 3 – Flush and Durability Coordination
- [ ] Define `AofManager` background thread structure and command queue types.
- [ ] Implement per-segment flush scheduling using `FlushConfig` thresholds and expose finalisation confirmation events.
- [ ] Ensure only one flush per segment runs concurrently and update `Segment::mark_durable` on completion.
- [ ] Add `Aof::flush(record_id)` waiting semantics backed by durable byte tracking.
- [ ] Integrate fsync/fdatasync calls with retry/backoff strategy.

### Milestone 4 – Recovery and Bootstrap
- [ ] Walk `segments/` at startup, parse filenames, and sort by `SegmentId`.
- [ ] Rebuild `next_segment_index`, `next_offset`, and `record_count` from headers, trusting all but the newest two segments.
- [ ] Scan the tail segment (and penultimate if required) to truncate corrupt bytes and recompute cumulative counters.
- [ ] Reopen the last active segment for appends, leaving others sealed and mapped read-only.
- [ ] Provide unit/integration tests that model crash states (torn headers, partial payload writes, missing footers).

### Milestone 5 – Reader APIs
- [ ] Define synchronous reader over sealed segments that accepts `RecordId` (packed `{segment_index, segment_offset}`) and validates checksums.
- [ ] Implement tail-following async reader that registers for notifications and resumes when new segments appear.
- [ ] Add optional sparse index generation on seal to accelerate `seek(record_id)` for large segments.

### Milestone 6 – Retention and Archival
- [ ] Implement retention evaluator for `RetentionPolicy` variants (size-based, time-based, keep-all).
- [ ] Add archival move/copy into `archive/`, ensuring atomic renames and fsync of parent directories.
- [ ] Provide hooks so retention runs in background without blocking the writer, coordinated via `AofManager`.

### Milestone 7 – Observability and Tooling
- [ ] Instrument append, flush, seal, and recovery paths with `tracing` spans and metrics counters.
- [ ] Expose metrics (e.g., via internal struct or optional feature) for append throughput, flush latency, and segment sizing decisions.
- [ ] Create administrative helpers or CLI hooks to inspect segment state and trigger manual rollover.
- [ ] Document operational runbooks (recovery steps, retention configuration, monitoring guidelines).

## Detailed Task List
| Task ID | Area | Description | Deliverable | Status | Dependencies |
|---------|------|-------------|-------------|--------|--------------|
| CFG1    | Config  | Add `segment_min_bytes`/`segment_max_bytes` validation enforcing power-of-two bounds (64 MiB–1 GiB) and clamp `segment_target_bytes`. | `config.rs` updates + tests | todo | Milestone 1 |
| S1      | Segment | Write `SegmentFooter` struct with encode/decode/checksum helpers fixed to the tail slot. | `segment.rs` footer module + tests | todo | Milestone 1 |
| S2      | Segment | Implement `Segment::seal` with footer emission at `max_size - FOOTER_SIZE` and remap to read-only. | Updated `segment.rs`, unit tests | todo | S1 |
| S3      | Segment | Add tail scan + truncation helper for recovery. | `segment.rs` recovery API + tests | todo | S1 |
| S4      | Segment | Guard record writes so they never overlap the reserved footer region. | `segment.rs` bounds checks + tests | todo | S1 |
| R1      | Runtime | Expand `Aof::new` to detect existing segments and call recovery. | `mod.rs` updates + integration test | todo | S1, S3 |
| R2      | Runtime | Handle rollover path (`Segment::append_record` -> `SegmentFull`) while honouring footer reservation. | `mod.rs` append loop, tests | in-progress | S2, S4 |
| R3      | Runtime | Implement `Aof::seal_active` and state transitions. | `mod.rs` API + tests | todo | R2 |
| R4      | Runtime | Enforce the two-unfinalized invariant before activating additional segments. | `mod.rs`/manager coordination + tests | todo | R2 |
| R5      | Runtime | Implement dynamic segment size selection within configured bounds (heuristics + telemetry). | `mod.rs` sizing logic + tests | todo | CFG1 |
| F1      | Flush | Define manager command enums and background worker. | `flush.rs`/`mod.rs` updates | todo | R2 |
| F2      | Flush | Implement flush scheduling that respects `FlushConfig`. | Manager + segment updates | todo | F1 |
| F3      | Flush | Provide `Aof::flush(record_id)` waiting semantics. | `mod.rs` API + tests | todo | F2 |
| RC1     | Recovery | Walk filesystem, parse headers, rebuild counters. | New `recovery.rs` or `mod.rs` section + tests | todo | S3 |
| RC2     | Recovery | Tail verification with optional footer trust. | Recovery module + tests | todo | RC1 |
| RD1     | Reader | Implement sealed-segment reader API returning payload slices. | `reader.rs` updates + tests | todo | R2 |
| RD2     | Reader | Async tail follower with packed id seek support. | `reader.rs` + integration tests | todo | RD1 |
| RT1     | Retention | Evaluate retention policy and queue archival actions. | `flush.rs` or new module + tests | todo | F1 |
| RT2     | Retention | Move sealed segments into `archive/` atomically. | `fs.rs` helpers + tests | todo | RT1 |
| OB1     | Observability | Add tracing spans/counters for key operations including dynamic sizing and finalization lag. | instrumentation across modules | todo | R2, F2, R5 |
| OB2     | Observability | Produce docs + sample metrics wiring. | Markdown docs | todo | OB1 |
| QA1     | Testing | Build integration tests covering append/read, rollover, crash recovery, and two-unfinalized enforcement. | `tests/` additions | todo | Depends on Milestones 1–4 |
| QA2     | Testing | Implement property-based tests for record/header parsing and footer placement. | `tests/` + proptest harness | todo | S1, RC2 |
| QA3     | Testing | Add benchmark suite for append throughput and flush latency under varying segment sizes. | Criterion bench + scripts | todo | R2, F2, R5 |

## Testing and Verification Strategy
- Unit coverage: segment header/footer encode/decode, CRC folding, `RecordId` packing helpers, and reserved footer bounds enforcement.
- Integration coverage: multi-segment append/read, recovery after simulated crash scenarios, flush durability guarantees, and confirmation that no more than two segments remain unfinalized.
- Stress and soak: long-running append with periodic fsync to ensure counters stay consistent, verified via metrics that track segment sizing decisions and finalization lag.
- Property-based validation: randomized payload and truncation to confirm recovery never exposes corrupt records.
- Benchmarking: optional gated benchmarks to measure throughput with varying segment sizes and flush intervals.

## Tooling and Documentation
- Extend developer README with instructions for configuring storage directories and running the new tests.
- Provide troubleshooting guide for recovery warnings, footer corruption, and finalization backpressure.
- Capture operational defaults (segment size band, flush thresholds) in configuration docs once runtime stabilises.
