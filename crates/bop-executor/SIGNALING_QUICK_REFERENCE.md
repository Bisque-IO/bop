# Signaling Mechanism - Quick Reference

## Field Purposes (SignalWaker)

| Field | Purpose | Bits Used | Operations |
|-------|---------|-----------|------------|
| `status` | Worker-local state flags | Bit 0: yield available<br>Bit 1: tasks available | `mark_yield()`<br>`try_unmark_yield()`<br>`mark_tasks()`<br>`try_unmark_tasks()` |
| `summary` | Active signal words bitmap | All 64 bits: which of 64 signal words have queues | `mark_active()`<br>`try_unmark()` |
| `partition_summary` | Active leaves in partition | Up to 64 bits: which leaves in partition have work | `mark_partition_leaf_active()`<br>`clear_partition_leaf()`<br>`sync_partition_summary()` |
| `permits` | Counting semaphore | Full u64: number of wakeup permits | `try_acquire()`<br>`release()` |

**CRITICAL**: `status` and `summary` are INDEPENDENT fields. Never confuse them!

## Signal Flow (Task Becomes Runnable)

```
┌────────────────┐
│ Task::schedule │ Sets TASK_SCHEDULED flag
└───────┬────────┘
        │
        ▼
┌────────────────┐
│ TaskSignal.set │ Sets bit in signal word [AcqRel]
└───────┬────────┘
        │
        ▼
┌──────────────────────────────┐
│ SummaryTree.mark_signal_active│ Sets bit in leaf_word [AcqRel]
└──────────────┬───────────────┘
               │
               ▼
┌────────────────────────────────┐
│ notify_partition_owner_active  │ Computes owner_id from leaf_idx
└──────────────┬─────────────────┘
               │
               ▼
┌────────────────────────────────────┐
│ SignalWaker.mark_partition_leaf_active│ Sets bit in partition_summary [Relaxed]
└──────────────┬─────────────────────────┘
               │
               ▼ (if 0→non-zero)
┌────────────────┐
│ mark_tasks()   │ Sets status bit 1 [Relaxed]
└───────┬────────┘
        │
        ▼
┌────────────────┐
│ release(1)     │ Adds permit [Release]
└───────┬────────┘
        │
        ▼
┌────────────────┐
│ cv.notify_one()│ Wakes one sleeping worker
└────────────────┘
```

## Worker Discovery (Finding Work)

```
┌─────────────────────────────┐
│ Worker::try_acquire_task()  │
└──────────────┬──────────────┘
               │
               ▼
┌──────────────────────────────┐
│ Read leaf_words[leaf_idx]    │ [Acquire] - syncs with Producer's Release
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│ Find set bit (signal_idx)    │
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│ Read signal word             │ [Acquire]
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│ signal.try_acquire(bit_idx)  │ [AcqRel] - CAS to clear bit
└──────────────┬───────────────┘
               │
               ▼ (if acquired)
┌──────────────────────────────┐
│ Return TaskHandle            │
└──────────────────────────────┘
               │
               ▼ (if signal now empty)
┌──────────────────────────────┐
│ mark_signal_inactive()       │ [AcqRel] - lazy cleanup
└──────────────────────────────┘
```

## Status Bit Lifecycle

### Bit 1 (Tasks Available):

```
partition_summary: 0 (empty)
status bit 1: clear
          │
          │ Task scheduled in partition
          ▼
mark_partition_leaf_active()
          │
          ▼ (0→non-zero transition)
    mark_tasks()
          │
          ▼
partition_summary: 0b0001
status bit 1: SET ✓
permits: +1
          │
          │ More tasks scheduled (already non-zero)
          ▼
mark_partition_leaf_active()
          │
          ▼ (non-zero→non-zero, no change)
partition_summary: 0b0011
status bit 1: SET (unchanged)
permits: 1 (unchanged)
          │
          │ All tasks complete
          ▼
clear_partition_leaf() for last leaf
          │
          ▼ (non-zero→0 transition)
  try_unmark_tasks()
          │
          ▼
partition_summary: 0
status bit 1: clear ✓
```

### Bit 0 (Yield Available):

```
yield_queue: empty
status bit 0: clear
          │
          │ First task yields
          ▼
    mark_yield()
          │
          ▼
yield_queue: [task1]
status bit 0: SET ✓
permits: +1
          │
          │ More tasks yield
          ▼
    mark_yield() (no-op, already set)
          │
          ▼
yield_queue: [task1, task2, ...]
status bit 0: SET (unchanged)
permits: 1 (unchanged)
          │
          │ Worker drains all yielded tasks
          ▼
  try_unmark_yield()
          │
          ▼
yield_queue: empty
status bit 0: clear ✓
```

## Common Operations

### When Task Becomes Runnable:
```rust
task.schedule();  // Propagates through hierarchy automatically
```

### Before Worker Parks:
```rust
// Defensive sync to ensure waker state matches reality
wakers[worker_id].sync_partition_summary(
    partition_start,
    partition_end,
    &summary_tree.leaf_words,
);

// Park (will consume permit if available, otherwise blocks)
wakers[worker_id].acquire_timeout(duration);
```

### On Partition Rebalance:
```rust
// Update partition boundaries
self.partition_start = new_start;
self.partition_end = new_end;

// MUST sync to update partition_summary and status bit
wakers[worker_id].sync_partition_summary(
    new_start,
    new_end,
    &summary_tree.leaf_words,
);
// ✅ Status bit now matches actual partition state
```

### When Signal Word Becomes Empty:
```rust
// Worker does lazy cleanup
if signal.load(Ordering::Acquire) == 0 {
    summary_tree.mark_signal_inactive(leaf_idx, signal_idx);
}
```

## Memory Ordering Guide

| Operation | Ordering | Reason |
|-----------|----------|--------|
| TaskSignal.set() | AcqRel | Synchronizes task state with signal bit |
| leaf_words.fetch_or() | AcqRel | Makes signal bit visible to workers |
| signal.try_acquire() | AcqRel | Synchronizes task data with acquisition |
| permits.fetch_add() | Release | Makes work visible to acquire() |
| permits.fetch_update() | AcqRel (success) | Synchronizes with producer's Release |
| status.fetch_or() | Relaxed | Local hint, permit provides sync |
| summary.fetch_or() | Relaxed | Hint only, false positives OK |
| partition_summary | Relaxed | Hint only, verified by actual reads |

## Debugging Checklist

### Worker Won't Wake:
- [ ] Is status bit set? Check `status_bits()`
- [ ] Are permits available? Check `permits()`
- [ ] Is partition_summary non-zero? Check `partition_summary()`
- [ ] Does leaf_word match partition_summary? Call `sync_partition_summary()`
- [ ] Is notify_partition_owner_active() being called?

### Spurious Wakeups:
- [ ] Is status bit clearing properly? (Bug #1 and #2 were here!)
- [ ] Is partition actually empty? Check leaf_words directly
- [ ] Is try_unmark_* operating on `status` not `summary`?

### Missed Work:
- [ ] Is summary bit corruption happening? Check summary after status ops
- [ ] Is mark_signal_active() being called?
- [ ] Is partition owner computed correctly?
- [ ] Is lazy cleanup too aggressive?

## Bug History

### 2025-10-29: Critical Bugs Fixed

**Bug #1**: `try_unmark_tasks()` was clearing `summary` bit 1 instead of `status` bit 1
- **Symptom**: Workers always wake (status bit never clears)
- **Fix**: Changed to `self.status.fetch_and(...)`

**Bug #2**: `try_unmark_yield()` was clearing `summary` bit 0 instead of `status` bit 0
- **Symptom**: Workers always wake (status bit never clears)
- **Fix**: Changed to `self.status.fetch_and(...)`

**Root Cause**: Copy-paste error + field name confusion
**Prevention**: Added comprehensive regression tests

## Testing

### Run All Signaling Tests:
```bash
cargo test --test signaling_tests
cargo test --test signal_waker_bug_regression_tests
```

### Key Regression Tests (would FAIL with original bugs):
```bash
cargo test regression_try_unmark_tasks_clears_status_not_summary
cargo test regression_try_unmark_yield_clears_status_not_summary
cargo test test_signal_waker_summary_vs_status_separation
```

### Stress Tests:
```bash
cargo test concurrent --test signaling_tests
cargo test concurrent --test signal_waker_bug_regression_tests
```

---

**Last Updated**: 2025-10-29  
**Status**: All bugs fixed, comprehensive tests added
