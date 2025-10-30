# WorkerService / Worker / SignalWaker / SummaryTree Testing Plan

This document enumerates the tests required to validate the executor’s core signalling stack.
Goals: prove we never lose wakeups or tasks, preserve task/permit invariants across worker churn, and surface performance regressions early.

---

## 1. Critical Invariants To Guard

- **SignalWaker**
  - Every queue-group (`status` bits 0–61) performs a single `0→1` permit release and never misses a `1→0` cleanup.
  - `STATUS_BIT_YIELD` / `STATUS_BIT_PARTITION` transitions are idempotent across concurrent `mark_*` / `try_unmark_*`.
  - Partition bitmap (`partition_summary`) always reflects a superset of live SummaryTree leaves; never drops a set bit while work exists.
  - `permits` is monotonically accurate: producers never lose increments; consumers eventually acquire or park.
  - Sleeping worker bookkeeping (`sleepers`) remains non-negative and in sync with parking/unparking.

- **SummaryTree**
  - Leaf/partition ownership matches `compute_partition_owner` and stays valid during rebalance.
  - `mark_leaf_bits`, `clear_leaf_bits`, and reservation logic honor mutual exclusion; no duplicate reservation; no missed `notify_partition_owner_*`.
  - Rebalance respects the current worker count without out-of-bounds indices.

- **Worker / WorkerService**
  - Startup/shutdown, tick thread, and dynamic scaling leave no dangling worker slots or leaked permits.
  - Partition rebalance messages cover every leaf exactly once; workers refresh `partition_summary` before sleeping.
  - Timers and Yield queues interact with the same waker without starving normal queues.
  - Task stealing respects `partition_summary` hints but degrades gracefully when stale.

---

## 2. Unit-Test Coverage

### SignalWaker
- Exhaustive bit lifecycle tests (`mark_active`, `mark_active_mask`, `try_unmark*`) across all 62 queue words.
- CAS race harness for `try_unmark_*` & `try_unmark_if_empty`.
- Permit counter monotonicity with multiple producers/consumers (including wrap-around close to `u64::MAX` in debug builds).
- Partition sync scenarios:
  - Fresh partition (0 length), dynamic expansion, shrink.
  - Worker register/unregister while sync in-flight.
- Sleepers counter: increment/decrement symmetry around `acquire` / `acquire_timeout`.
- Condvar wait: simulate spurious wakeups and ensure no double decrement.

### SummaryTree
- Reservation loops: `reserve_task_in_leaf` exhausts all bits under concurrency (loom-friendly variant).
- Notification edges:
  - `mark_signal_active` sets partition bit once per `0→1`.
  - `mark_signal_inactive` only clears when truly empty; cross-check with actual signal words.
- Rebalance helpers (`compute_partition_owner`, `partition_start_for_worker`, `partition_end_for_worker`, `global_to_local_leaf_idx`) against golden tables for varying worker counts, including non-divisible leaf counts.

### Worker Internals
- Yield queue transitions producing `mark_yield` / `try_unmark_yield`.
- Timer wheel tick updates verifying the waker sees partition activity.
- `try_partition_*` strategies verifying they honor `partition_summary` hints first.

---

## 3. Integration Tests

1. **Single Worker Lifecycle**
   - Spin up WorkerService with 1 worker, push tasks across all queue groups, assert each drain triggers exactly one permit and summary bit.
   - Force graceful shutdown and verify workers unregister, signals clear.

2. **Multi-Worker Partition Rebalance**
   - Start `N` workers, enqueue tasks targeting each partition, trigger manual and periodic rebalance, ensure no task is lost and partition caches realign.
   - Remove/add workers repeatedly; assert `mark_partition_leaf_active` is re-routed to new owners.

3. **Mixed Load (Tasks + Yields + Timers)**
   - Run timers firing at high frequency while tasks enqueue yields. Confirm yield bit never starves partition tasks and vice versa.

4. **Scaling Decisions**
   - Use synthetic load to cross scale-up/down thresholds; assert WorkerService messages wake the right wakers and no phantom workers remain.

5. **Shutdown Under Load**
   - Initiate shutdown while tasks are mid-flight, with sleepers parked. Confirm all signals clear, permits drain to zero, and no threads hang.

---

## 4. Concurrency & Stress Testing

- **Thread Races**
  - Expand existing stress tests (`regression_concurrent_status_summary_operations`, etc.) to cover:
    - `sync_partition_summary` racing with `mark_partition_leaf_active`.
    - Worker parking/unparking simultaneously with partition rebalance.
    - Rapid worker register/unregister loops.
- **Load Generators**
  - Long-running fuzz harness randomly choosing operations (`enqueue task`, `yield`, `timer tick`, `rebalance`, `sleep`) verifying:
    - Task completion count matches submissions.
    - No stale summary bits remain after drains.
- **Loom / Miri Sessions**
  - Simplified models (single worker, 2 queue groups) run on Loom to systematically explore interleavings of `mark_active` vs `try_unmark_if_empty`, and partition updates.

Implemented coverage:
- `stress_signal_waker_partition_sync_race` and `stress_signal_waker_summary_race` (signal_waker concurrency).
- `integration_runtime_randomized_load` (randomized runtime load generator exercising worker rebalance under mixed ops).

---

## 5. Property / Fuzz Testing

- **Permit Consistency Property**
  - Model inputs: sequence of producer events and consumer acquisitions.
  - Property: `permits` ≥ number of outstanding tasks; final value zero when all tasks processed.

- **Partition Hint Accuracy**
  - Randomly assign tasks to leaves, run `sync_partition_summary`, and assert returned bitmap == actual non-empty leaves (modulo allowed false positives).
  - Differential check between SummaryTree `leaf_words` and `partition_summary`.

- **Steal Fairness**
  - Fuzz worker stealing decisions; property: no leaf is starved indefinitely when work exists.

Implemented coverage:
- `property_signal_waker_permit_consistency`
- `property_partition_hint_accuracy`
- `property_summary_tree_reserve_fairness`

---

## 6. Performance Regression Checks

- Benchmark scenarios:
  - `mark_active` hot path (bit already set) vs cold path.
  - `sync_partition_summary` cost with varying partition sizes.
  - Worker run loop throughput under mixed load.
- Track using Criterion or manual harness; flag >5% regressions.

Implemented coverage:
- `signal_waker_perf::bench_mark_active_paths`
- `signal_waker_perf::bench_sync_partition_summary`
- `signal_waker_perf::bench_worker_mixed_loop`

---

## 7. Tooling & Instrumentation

- Add optional tracing toggles to dump wake/sleep events; used in integration/stress suites to assert sequence numbers.
- Compile with `-Z sanitizer=thread` (nightly) to catch race regressions in tests.
- `cargo --config check-cfg=names(feature)` to keep cfg hygiene (address the `disabled_tests` warning).

---

## 8. Regression Tracking

- Maintain targeted regression tests mirroring historical bugs (status vs summary mismatches, lost permits, partition drop-outs).
- Every bug fix should add a minimal reproducer under `tests/signal_waker_bug_regression_tests.rs`.
- Document known coverage gaps here and update as they close.
