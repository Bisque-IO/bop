# Active Summary & Reservation Refactor Plan

## Goal
Unify task signals, yield queues, and worker/task reservations inside the mmap-backed arena so the two-level summary tree (root → leaf words) reflects **all** runnable work without any heap allocations.

## Design Overview

1. **Two-Level Summary**
   - `root_words`: `(leaf_count + 63) / 64` `AtomicU64` values.
   - `leaf_words`: `leaf_count` `AtomicU64` values.
     - Bits `0..63` mirror each `TaskSignal`.
     - Reserve a single bit (e.g. bit 63) per word for the leaf’s yield queue.

2. **Task Signals**
   - Stored directly after the leaf summary words in the arena.
   - Each word’s bits align with the associated leaf summary word.

3. **Reservation Maps**
   - Worker reservations: `AtomicU64` bitmap where each bit = leaf index with an assigned yield queue. `register_worker()` finds or fails.
   - Task registration: per-leaf `AtomicU64` (matching the task word) where zeros indicate free slots. `register_task()` flips a bit atomically; `unregister_task()` clears it.

4. **Activation Flow**
   - Task activation sets the task bit in the leaf word. If the word transitions from zero, set the corresponding root bit and release a permit.
   - Yield enqueue sets the leaf’s reserved yield bit; dequeue clears it when the queue drains.
   - Root summary always reflects `leaf_words != 0`; `poll_with_leaf_partition_blocking()` just reads the root and leaf words.

5. **Arena Layout**
   - `root_words[ root_count ]`
   - `leaf_words[ leaf_count ]`
   - `task_signals[ leaf_count ]`
   - `reservation_maps` (worker bitmap, per-leaf task bitmaps)
   - Remaining arrays (registered summary, task storage, etc.)

6. **Configuration Parameters**
   - `root_count`
   - `leaf_count`
   - `max_workers` (must be ≤ `leaf_count`)
   - `max_tasks`

## Implementation Steps

1. **ActiveSummaryTree Refactor**
   - Remove `Vec` usage; store raw pointers/lengths (`root_words`, `leaf_words`).
   - Expose helper methods (`mark_leaf_*`, `reserve_worker`, `reserve_task`, etc.).

2. **Arena Construction**
   - Lay out root/leaf/task words sequentially.
   - Materialize the tree with those pointers.
   - Initialize reservation bitmaps inside the arena.

3. **Worker Registration**
   - Add `reserve_worker()`/`release_worker()` APIs.
   - Worker startup requests a leaf index; teardown releases it.
   - Yield queue logic flips the reserved bit in that leaf word.

4. **Task Registration**
   - Implement `reserve_task()` that flips an available bit in the leaf’s task word and updates the summary if it transitions empty→non-empty.
   - Implement `release_task()` to clear the bit and propagate empty transitions.

5. **Update Executor/Workers**
   - Replace ad-hoc yield queue bits with the reserved bit.
  - Ensure task registration paths use the new reservation APIs.

6. **Benchmarks & Tests**
   - Extend `active_summary_benchmark.rs` to stress:
     - Uniform/hot leaf tasks
     - Yield-heavy scenarios
     - Task registration churn
   - Validate no residual bits remain (`snapshot_summary() == 0`) after each run.

7. **Cleanup**
   - Remove obsolete summary helpers and metadata structs.
   - Update documentation and comments to match the new layout.

## Follow-Up Prompt (copy/paste)

```
We’re continuing the ActiveSummaryTree refactor. Please implement:
1. Two-level summary layout (`root_words`, `leaf_words`) stored in the arena.
2. Worker and task reservation bitmaps with register/release APIs.
3. Yield bit integration: each leaf word reserves one bit for the yield queue and the workers flip it via the reservation map.
4. Update the executor/worker/task registration code to use the new APIs.
5. Expand `active_summary_benchmark.rs` to exercise task registration and yield scenarios.

Keep the layout parameters configurable (`root_count`, `leaf_count`, `max_workers`, `max_tasks`). After implementing, run the benchmarks and ensure no hanging or residual active bits remain.
```

