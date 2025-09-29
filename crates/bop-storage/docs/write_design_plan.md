# Write/Flush Controller Implementation Plan

## Objectives
- Implement production-ready write and flush controllers around `WalSegment` primitives while preserving segment invariants and recovery guarantees.
- Provide flexible buffering through `WriteChunk` and shared runtime plumbing via `Arc<Runtime>`.
- Surface diagnostics and manifest durability updates without embedding controller logic inside `Wal`.

## Task List
1. Establish runtime plumbing and shared infrastructure.
2. Extend `WalSegment` with write buffers (`WriteChunk` batches) and queue guards.
3. Implement the write controller (`WriteTask` queue, batching, blocking I/O dispatch).
4. Implement the flush controller (`FlushTask` queue, `fsync`, manifest integration, backoff).
5. Wire controllers into higher-level orchestration (DB initialization, manifest adapter hooks, diagnostics exposure).
6. Add regression tests, metrics validation, and documentation updates.

## Task Tracking
| ID | Task | Status | Notes |
|----|------|--------|-------|
| T1 | Runtime plumbing and shared infrastructure | DONE | Added shared `StorageRuntime` (`Arc<Runtime>` plumbing), manager/DB wiring, shutdown coordination; verified with `cargo check -p bop-storage`. |
| T2 | `WalSegment` buffering and guard enhancements | DONE | Added `WriteChunk`/`WriteBatch`, double-buffer swap paths, queue flags/metrics; covered by new wal buffer tests. |
| T3 | Write controller implementation | DONE | Added `WriteTask` queue, write controller, blocking I/O pipeline, and tests covering success/failure paths. |
| T4 | Flush controller implementation | DONE | Added `FlushController` with `FlushTask` queue, runtime-backed fsync + manifest sink handling, exponential backoff, and regression tests (`flush::tests::flushes_segment_to_durable`, `flush_failure_retries_until_success`, `sink_failure_triggers_retry`). |
| T5 | Integration and diagnostics wiring | DONE | Controllers wired through DB/Manager, manifest-backed sink, diagnostics/tests updated. |
| T6 | Verification and documentation | TODO | Add unit/integration tests, metrics assertions, finalize docs and release notes. |

## Rules
- After completing any task, update the Task Tracking table (status and notes) before beginning new work.
- Maintain ASCII-only edits in controller modules and documentation unless existing files prove otherwise.
- Keep queues and buffering logic in sync with the naming defined in `write_design.md` (use `WriteChunk`, `WriteTask`, `FlushTask`).
- Ensure each code change includes corresponding test coverage or an explicit rationale recorded in the Notes column.

