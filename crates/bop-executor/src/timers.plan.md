## Timer Integration Implementation Plan

> High-level steps to implement the timer design described in `timers.md`. Tasks are grouped logically; each group can be tackled incrementally.

### 1. Core Infrastructure
- [ ] Introduce a `StripedAtomicU64` helper (cache-line aligned stripes, Power-of-two configurable).
- [ ] Define shared timer handle structure (`Arc<TimerInner>` with state enum, generation `AtomicU64`, and optional payload pointer).
- [ ] Add per-worker timer context (thread-local wheel pointer, `timer_now` atomic ref, `needs_poll` flag).

### 2. Dedicated Timer Thread
- [ ] Create timer-thread service responsible for:
  - Sleeping until next tick, updating each worker’s `timer_now`.
  - Draining a global `MergeQueue` (initially `Mutex<Vec<_>>`, future: lock-free).
  - Nudging workers that have ready buckets (e.g., by CAS-ing `needs_poll` or enqueuing a lightweight waker).
- [ ] Determine tick resolution configuration (hard-coded constant to start; later: config/auto-tuning).

### 3. Worker Integration
- [ ] Attach a `TimerWheelContext` to `Worker` (wheel structure, striped counters, `VecDeque` buckets).
- [ ] Implement scheduling:
  - Increment generation, set state `Scheduled`.
  - Push entry into local wheel bucket.
  - Mark `needs_poll`.
- [ ] Implement polling path:
  - Check `timer_now` vs. wheel tick.
  - Drain due bucket, compare generation/state, fire or drop.
  - Update per-bucket stale counters & decrement stripes.
- [ ] Add background scrub pass (bounded budget per loop).

### 4. Cancellation & Reschedule
- [ ] Expose cancel API: generation bump + state update + stripe increment.
- [ ] Ensure rescheduling via `MmapExecutorArena::spawn` (or higher-level API) uses new timer handle semantics.
- [ ] Maintain stats / counters (optional at first; at least track cancellations).

### 5. Shutdown & Merge Queue
- [ ] During `Worker::drop`, move outstanding buckets into `MergeQueue`.
- [ ] Timer thread reassigns entries to active workers (simple strategy: round-robin or worker with least pending).
- [ ] When reinserted, generation/state check ensures stale entries get dropped immediately.

### 6. Garbage Management
- [ ] Integrate `StripedAtomicU64` updates in cancel/reschedule paths.
- [ ] Implement lazy compaction on due buckets when stale ratio exceeds threshold.
- [ ] Implement round-robin background scrub (e.g., process 1 bucket per poll loop).
- [ ] Tune thresholds (e.g., stale > live or stale > configurable constant).

### 7. Testing & Instrumentation
- [ ] Unit tests for timer handle lifecycle (schedule → cancel → reschedule).
- [ ] Tests for wheel migration (simulate worker drop, ensure timers fire).
- [ ] Tests for garbage counters (ensure they drop after compaction).
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
