# Timer Safety & Scheduling Refinement Plan

## Goals
- Preserve the zero-allocation timer fast path while eliminating the possibility of dangling task pointers.
- Let workers optimistically poll timer-driven tasks inline when safe, falling back to the existing scheduler otherwise.
- Keep timer/future APIs ergonomic (e.g. `TimerDelay`) without reintroducing `Arc<TimerInner>`.

## Core Design Tweaks
1. **Timer identity only**
   - Store `TaskSlot*` + `task_id` inside `TimerHandle`.
   - `prepare_schedule` records `task_slot`, `task_id`, target worker, and deadline.
   - Wheel entries never dereference user memory; they validate the slot + id before acting.

2. **Worker-side validation**
   - On expiry, worker loads the slot pointer.
   - If the slot is empty or the `task_id` mismatches, drop the entry (task finished or slot reused).
   - Otherwise proceed with wake/poll logic.

3. **Optimistic inline polling**
   - Attempt CAS from `TASK_IDLE → TASK_EXECUTING`.
     - Success: poll the task immediately; skip signal bookkeeping.
   - Else if state is `TASK_SCHEDULED`, run the normal `signal::acquire` path.
   - Any other state → fall back to scheduling (let the regular worker loop handle it).

4. **TimerDelay helper**
   - Provide a zero-allocation future that borrows the `TimerHandle` and just awaits the validated wakeup.
   - Ensures application code doesn’t accidentally drop the timer without cancelling.

## Safety Guarantees
- Task identity check prevents use-after-free even if a timer fires after the task completed.
- No raw payload pointer leaves the future; only slot/id + timestamps move around.
- Optimization respects existing scheduler invariants: we only bypass the signal path when the CAS proves exclusive ownership.

## Follow-up Tasks
- Implement `TimerHandle` identity storage + helper methods.
- Update worker timer polling to use the new validation and CAS flow.
- Replace existing pointer-based scheduling in tests/examples with the `TimerDelay` pattern.
- Audit task teardown to ensure handles cancel/clear on drop (still useful as a secondary guard).
