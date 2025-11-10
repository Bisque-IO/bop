# Timer Safety & Scheduling Refinement Plan

## Goals
- Preserve the zero-allocation timer fast path while eliminating the possibility of dangling task pointers.
- Let workers optimistically poll timer-driven tasks inline when safe, falling back to the existing scheduler otherwise.
- Keep timer/future APIs ergonomic (e.g. `TimerDelay`) without reintroducing `Arc<TimerInner>`.

## Core Design Tweaks
1. **Timer identity only**
   - `TimerHandle` is a plain POD carrying `TaskSlot*`, `task_id`, and the `(worker_id, timer_id)` pair returned from `TimerWheel::schedule_timer`.
   - Futures allocate/snapshot this handle locally when they arm a timer; no atomics or shared state live inside the handle.
   - Wheel entries never dereference user memory beyond the slot/generation/timer id validation.

2. **Worker-side validation**
   - On expiry, worker reads the `TaskSlot` and retrieves the live task pointer.
   - Validate both the task’s current `task_id` and the pending `(worker_id, timer_id)` match the handle.
   - Drop the entry if any check fails; otherwise proceed with wake/poll logic.

3. **Optimistic inline polling**
   - Attempt CAS from `TASK_IDLE → TASK_EXECUTING`.
     - Success: poll the task immediately; skip signal bookkeeping.
   - Else if state is `TASK_SCHEDULED`, run the normal `signal::acquire` path.
   - Any other state → fall back to scheduling (let the regular worker loop handle it).

4. **TimerDelay helper**
   - Futures manage their own `TimerCell` (deadline, optional interval, local generation).
   - Polling compares the cell’s deadline against the task’s `now_ns` and re-arms by minting a fresh `TimerHandle`.
   - Ensures application code doesn’t accidentally drop the timer without cancelling.

## Safety Guarantees
- Task identity check prevents use-after-free even if a timer fires after the task completed.
- No raw payload pointer leaves the future; only slot/id + timer metadata move around.
- Optimization respects existing scheduler invariants: we only bypass the signal path when the CAS proves exclusive ownership.
- Remote cancellation can target `(worker_id, timer_id)` without peeking into future state.

## Execution Plan

### Phase 1 – Identity Data Structures
- [ ] Replace the old `Timer`/`TimerHandle` definition with a POD handle (`TaskSlot*`, `task_id`, `(worker_id, timer_id)`) and a lightweight `TimerCell`.
- [ ] Provide helpers on `Timer` for `prepare`, `commit_schedule`, `reset`, and handle creation.
- [ ] Add a per-timer generation (`timer_id`) recorded when committing the schedule.

### Phase 2 – Worker Integration
- [ ] Update `TimerSchedule`, `TimerBatch`, and `WorkerMessage` to carry `TimerHandle` values (no raw pointers).
- [ ] Rework `Worker::handle_timer_schedule` / `enqueue_timer_entry` to commit schedules and record timer IDs.
- [ ] Implement worker-side cancellation (`Worker::cancel_timer`, public `cancel_timer_for_current_task`) and `current_worker_now_ns`.
- [ ] Simplify timer expiry to validate handle identity and mark the timer ready without inline polling changes yet.

### Phase 3 – Polling & Futures
- [ ] Rewrite `schedule_timer_for_current_task` to use the new timer cell, cancelling/rescheduling any prior entry.
- [ ] Implement `TimerDelay` atop `TimerCell`, using `current_worker_now_ns` for deadline checks and automatic re-arming by futures.
- [ ] Update `Timer` consumers (tests/examples) to leverage `TimerDelay` and remove legacy pointer scheduling.

### Phase 4 – Validation & Cleanup
- [ ] Adjust tests to cover identity mismatches, cancellation, and repeating timers using `TimerCell`.
- [ ] Audit task teardown / drop paths to ensure timers reset/cancel cleanly.
- [ ] Run `cargo fmt` and targeted `cargo test -p bop-executor` to confirm the refactor.
