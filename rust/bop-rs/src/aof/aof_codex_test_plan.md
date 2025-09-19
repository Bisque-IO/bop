# AOF Test Plan

## Phase Tracker
- [ ] Phase 1: Core Foundations (in progress) - expand unit coverage for core data structures and controllers
- [ ] Phase 2: Persistence & Recovery - exercise append/flush/finalize flows and crash recovery sequences
- [ ] Phase 3: Concurrency & Tail - validate multi-reader/multi-writer behaviour and tail notifications
- [ ] Phase 4: Archival & Storage - test archival transitions, remote/local parity, and index updates
- [ ] Phase 5: Stress & Property - apply fuzzing, randomized workloads, and long-running stress harnesses
- [ ] Phase 6: Tooling & CI - integrate with automation, feature flags, and documentation

## Phase Details

### Phase 1: Core Foundations
**Objectives**
- Cover `record.rs` primitives (segment footer, metadata stamps, serialization failures)
- Validate `segment_store.rs` transitions and invariants
- Exercise `reader.rs` binary index helpers and tail notifications
- Test `flush.rs` strategy switching and metrics math
- Add filesystem adapter smoke tests

**Deliverables**
- New unit tests under `tests/aof/*` or module-local `#[cfg(test)]`
- Updated documentation of coverage in this plan
- Passing `cargo test --test aof_integration_tests`

**Progress**
- Segment footer round-trip and read-from-end tests added in `record.rs`
- Reader tail-notify coverage: async tests validating notify requirement, signal handling, and non-tailing behaviour (`tests/aof/reader_tests.rs`)
- AsyncFileSystem smoke tests cover create/move/copy/delete and random-access IO (`tests/aof/filesystem_tests.rs`)

### Phase 2: Persistence & Recovery
- End-to-end append/flush/finalize using temp dirs (Windows + Unix semantics)
- Verify segment bytes, metadata records, and footer checksum integrity
- Reload sequences after unclean shutdown, truncated files, or missing checkpoints

### Phase 3: Concurrency & Tail
- Multi-threaded append/read with ordering assertions
- Tail readers observing live appends with notify drops and slow consumers
- Ensure no deadlocks when backpressure kicks in

### Phase 4: Archival & Storage
- Filesystem archive happy path and failure injection
- Remote (S3-mock) archival with reconnection scenarios
- Status transitions reflected in MDBX/SQLite indices

### Phase 5: Stress & Property
- `proptest` property cases for record/index round-trips
- Randomized append/flush sequences validating monotonic IDs and index coherence
- Long-lived workers driving rollover, archive, and reader churn

### Phase 6: Tooling & CI
- Feature gating for slow/failure tests, documented in `Cargo.toml`
- CI matrix entries for Windows/Linux runners
- Nightly stress job and clippy/lint integration
- Keep this plan synchronized with progress notes

## Immediate Next Steps
- [x] Implement Phase 1 coverage for record primitives (SegmentFooter tests added in `record.rs`)
- [x] Expand reader/flush tests to exercise tail notifications and success-rate math
- [x] Introduce AsyncFileSystem smoke tests covering create/read/write/delete paths
- [ ] Begin Phase 2 work: design first append/flush/finalize integration scenario using temp dirs