## Timer Integration Implementation Plan

> High-level steps to implement the timer design described in `timers.md`. Tasks are grouped logically; each group can be tackled incrementally.

### 1. Core Infrastructure
- [x] Introduce a `StripedAtomicU64` helper (cache-line aligned stripes, Power-of-two configurable).
- [x] Define shared timer handle structure (`Arc<TimerInner>` with state enum, generation `AtomicU64`, and optional payload pointer).
- [x] Add per-worker timer context (thread-local wheel pointer, `timer_now` atomic ref, `needs_poll` flag).

### 2. Dedicated Timer Thread
- [x] Create timer-thread service responsible for:
  - [x] Sleeping until next tick, updating each worker's `timer_now`.
  - [x] Draining a global `MergeQueue` (initially `Mutex<Vec<_>>`, future: lock-free).
  - [x] Nudging workers that have ready buckets (e.g., by CAS-ing `needs_poll` or enqueuing a lightweight waker).
- [x] Determine tick resolution configuration (hard-coded constant to start; later: config/auto-tuning).

### 3. Worker Integration
- [x] Attach a `TimerWheelContext` to `Worker` (wheel structure, striped counters, `VecDeque` buckets).
- [x] Implement scheduling:
  - Increment generation, set state `Scheduled`.
  - Push entry into local wheel bucket.
  - Mark `needs_poll`.
- [x] Implement polling path:
  - Check `timer_now` vs. wheel tick.
  - Drain due bucket, compare generation/state, fire or drop.
  - Update per-bucket stale counters & decrement stripes.
- [x] Add background scrub pass (bounded budget per loop).

### 4. Cancellation & Reschedule
- [x] Expose cancel API: generation bump + state update + stripe increment.
- [x] Ensure rescheduling via `MmapExecutorArena::spawn` (or higher-level API) uses new timer handle semantics.
- [x] Maintain stats / counters (optional at first; at least track cancellations).

### 5. Shutdown & Merge Queue
- [x] During `Worker::drop`, move outstanding buckets into `MergeQueue`.
- [x] Timer thread reassigns entries to active workers (simple strategy: round-robin or worker with least pending).
- [x] When reinserted, generation/state check ensures stale entries get dropped immediately.

### 6. Garbage Management
- [x] Integrate `StripedAtomicU64` updates in cancel/reschedule paths.
- [x] Implement lazy compaction on due buckets when stale ratio exceeds threshold.
- [x] Implement round-robin background scrub (e.g., process 1 bucket per poll loop).
- [x] Tune thresholds (e.g., stale > live or stale > configurable constant).

### 7. Testing & Instrumentation
- [x] Unit tests for timer handle lifecycle (schedule -> cancel -> reschedule).
- [x] Tests for wheel migration (simulate worker drop, ensure timers fire).
- [x] Tests for garbage counters (ensure they drop after compaction).
- [ ] Integrate logging/metrics hooks (optional, but helpful for tuning).

### 8. Documentation & Follow-ups
- [ ] Update `timers.md` if implementation details diverge.
- [ ] Document configuration knobs (tick resolution, scrub thresholds).
- [ ] Track unresolved follow-ups (dynamic resolution, load balancing strategy).

Execution can proceed in phases:
1. Skeleton infrastructure (Sections 1–3) to get timers running locally.
2. Cancellation + garbage tracking (Sections 4 & 6).
3. Worker shutdown + migration (Section 5).
4. Tests, instrumentation, refinements (Section 7 onwards).

