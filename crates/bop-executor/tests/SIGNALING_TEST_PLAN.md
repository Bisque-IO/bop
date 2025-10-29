# Signaling Mechanism Test Plan

## Overview

This document describes the comprehensive testing strategy for the signaling mechanism across Task, TaskArena, SummaryTree, SignalWaker, and Worker components.

## Critical Bugs Addressed

### Bug #1: `try_unmark_tasks()` Operating on Wrong Field
- **Issue**: Was clearing `summary` bit 1 instead of `status` bit 1
- **Impact**: Status bit never cleared → spurious wakeups; Summary corruption
- **Fix**: Changed to operate on `status` field
- **Tests**: `signal_waker_bug_regression_tests.rs`

### Bug #2: `try_unmark_yield()` Operating on Wrong Field
- **Issue**: Was clearing `summary` bit 0 instead of `status` bit 0  
- **Impact**: Status bit never cleared → spurious wakeups; Summary corruption
- **Fix**: Changed to operate on `status` field
- **Tests**: `signal_waker_bug_regression_tests.rs`

## Test Categories

### 1. Unit Tests: SignalWaker Status Bits (`signaling_tests.rs`)

#### `test_signal_waker_status_bit_yield_lifecycle`
- **Purpose**: Validate yield bit (status bit 0) full lifecycle
- **Steps**:
  1. Verify initial state is clear
  2. Call `mark_yield()` → verify bit set + permit added
  3. Call `try_unmark_yield()` → verify bit cleared
  4. Verify permit remains (not consumed by try_unmark)

#### `test_signal_waker_status_bit_tasks_lifecycle`
- **Purpose**: Validate tasks bit (status bit 1) full lifecycle
- **Steps**:
  1. Verify initial state is clear
  2. Call `mark_tasks()` → verify bit set + permit added
  3. Call `try_unmark_tasks()` → verify bit cleared
  4. Verify permit remains

#### `test_signal_waker_status_bits_idempotent`
- **Purpose**: Verify multiple mark_* calls only add one permit
- **Validates**: 0→1 transition detection works correctly

#### `test_signal_waker_status_bits_independent`
- **Purpose**: Verify yield and tasks bits don't interfere with each other
- **Steps**:
  1. Set both bits
  2. Clear yield bit → verify tasks bit unaffected
  3. Clear tasks bit → verify yield bit remains clear

#### `test_signal_waker_summary_vs_status_separation`
- **Purpose**: **CRITICAL** - Verify summary and status are completely independent
- **Validates**: Operations on status don't affect summary and vice versa
- **This test would have caught the original bugs**

### 2. Unit Tests: SignalWaker Partition Summary (`signaling_tests.rs`)

#### `test_signal_waker_partition_summary_transitions`
- **Purpose**: Validate partition_summary 0↔non-zero transitions update status correctly
- **Steps**:
  1. Start empty (partition_summary = 0)
  2. Mark first leaf active → verify 0→non-zero triggers mark_tasks()
  3. Mark second leaf → verify no additional permit (already non-zero)
  4. Clear first leaf → verify bit cleared but tasks bit remains
  5. Clear last leaf → verify non-zero→0 triggers try_unmark_tasks()

#### `test_signal_waker_sync_partition_summary`
- **Purpose**: Validate sync_partition_summary() correctly samples leaf_words
- **Steps**:
  1. Create mock leaf_words with known pattern
  2. Call sync_partition_summary()
  3. Verify partition_summary bitmap matches non-zero leaves
  4. Verify status bit set on 0→non-zero
  5. Clear leaves and sync again
  6. Verify status bit cleared on non-zero→0

### 3. Integration Tests: Task → SummaryTree → SignalWaker (`signaling_tests.rs`)

#### `test_task_schedule_propagates_to_summary_tree`
- **Purpose**: Validate full signaling path from Task::schedule() to worker notification
- **Flow**:
  ```
  Task::schedule()
    → TaskSignal.set()
    → SummaryTree.mark_signal_active()
    → notify_partition_owner_active()
    → SignalWaker.mark_partition_leaf_active()
    → mark_tasks() → release(1)
  ```
- **Validates**:
  - Signal bit set in TaskSignal
  - Leaf_word bit set in SummaryTree
  - Partition owner's tasks bit set
  - Permit added to partition owner

#### `test_multiple_tasks_same_leaf_notification`
- **Purpose**: Verify multiple tasks in same leaf only trigger one notification
- **Validates**: Idempotency of notification system

### 4. Regression Tests (`signal_waker_bug_regression_tests.rs`)

These tests specifically target the fixed bugs and would have caught them during development.

#### `regression_try_unmark_tasks_clears_status_not_summary`
- **Purpose**: **CRITICAL** - Verify try_unmark_tasks() operates on status field
- **Setup**:
  1. Set summary bits 0, 1, 2 via mark_active()
  2. Set status bit 1 via mark_tasks()
- **Validate**:
  - try_unmark_tasks() clears status bit 1 ✓
  - try_unmark_tasks() does NOT affect summary bit 1 ✓
- **This test FAILS with the original bug**

#### `regression_try_unmark_yield_clears_status_not_summary`
- **Purpose**: **CRITICAL** - Verify try_unmark_yield() operates on status field
- **Setup**:
  1. Set summary bits 0, 1, 2 via mark_active()
  2. Set status bit 0 via mark_yield()
- **Validate**:
  - try_unmark_yield() clears status bit 0 ✓
  - try_unmark_yield() does NOT affect summary bit 0 ✓
- **This test FAILS with the original bug**

#### `regression_status_bits_full_lifecycle`
- **Purpose**: Verify status bits can cycle multiple times without corrupting summary
- **Steps**: Cycle mark/try_unmark 5 times, verify summary unchanged

#### `regression_sync_partition_summary_updates_status_not_summary`
- **Purpose**: Verify sync_partition_summary() updates status, not summary
- **Critical**: Tests the interaction between partition operations and status bits

#### `regression_status_and_summary_full_independence`
- **Purpose**: Exhaustive test of all status × summary state combinations
- **Coverage**: 4 status states × 5 summary states = 20 combinations
- **Validates**: Complete independence between the two fields

### 5. Stress Tests (`signaling_tests.rs`, `signal_waker_bug_regression_tests.rs`)

#### `test_concurrent_task_scheduling`
- **Purpose**: Stress test with 8 threads × 100 tasks each
- **Validates**: No race conditions in concurrent scheduling

#### `test_concurrent_acquire_and_schedule`
- **Purpose**: Concurrent spawning (4 threads × 25 tasks) + worker execution
- **Validates**: All tasks complete without loss

#### `regression_rapid_status_cycling_preserves_summary`
- **Purpose**: 1000 rapid cycles of mark/unmark status bits
- **Validates**: Summary completely unaffected by status operations

#### `regression_concurrent_status_summary_operations`
- **Purpose**: 3 threads concurrently manipulating status, summary, and partition
- **Validates**: No data races or crashes

### 6. Edge Case Tests

#### `test_status_bit_race_on_partition_empty_to_nonempty`
- **Purpose**: Rapid empty↔non-empty transitions
- **Validates**: Permits accumulate correctly

#### `test_try_unmark_when_already_clear`
- **Purpose**: Call try_unmark_* when bits already clear
- **Validates**: No-op behavior, no crashes

#### `test_summary_tree_lazy_cleanup`
- **Purpose**: Verify stale summary bits are acceptable and cleaned up
- **Validates**: mark_signal_inactive_if_empty() cleans up

#### `regression_status_bit_toctou_is_safe`
- **Purpose**: Test TOCTOU race in try_unmark_* check-then-clear pattern
- **Validates**: Worst case is spurious wakeup (safe)

## Running the Tests

### Run all signaling tests:
```bash
cargo test --test signaling_tests
cargo test --test signal_waker_bug_regression_tests
```

### Run specific test categories:
```bash
# Unit tests only
cargo test --test signaling_tests test_signal_waker

# Integration tests only  
cargo test --test signaling_tests test_task_schedule

# Regression tests for the bugs
cargo test --test signal_waker_bug_regression_tests regression_try_unmark

# Stress tests
cargo test --test signaling_tests concurrent
cargo test --test signal_waker_bug_regression_tests concurrent
```

### Run with output for debugging:
```bash
cargo test --test signal_waker_bug_regression_tests -- --nocapture
```

## Test Coverage Analysis

### Memory Ordering Coverage
- ✅ AcqRel on TaskSignal.set()
- ✅ Acquire on SummaryTree leaf_words read
- ✅ Release on permits.fetch_add()
- ✅ Relaxed on status bits (acceptable)

### State Transition Coverage
- ✅ 0→1 transitions (mark_*)
- ✅ 1→0 transitions (try_unmark_*)
- ✅ 1→1 (idempotency)
- ✅ 0→0 (no-op)

### Partition Rebalancing Coverage
- ✅ Empty partition → receives tasks
- ✅ Non-empty partition → becomes empty
- ✅ Partition boundaries change via sync

### Concurrency Coverage
- ✅ Multiple threads scheduling tasks
- ✅ Multiple threads spawning work
- ✅ Concurrent status/summary operations
- ✅ Rapid cycling stress test

## Success Criteria

### All Tests Must Pass
- ❌ **CRITICAL**: If any regression test fails, the bugs have returned
- ❌ **CRITICAL**: If summary_vs_status_separation fails, fields are not independent

### Performance Criteria
- Stress tests should complete in < 5 seconds each
- No deadlocks or livelocks
- No spurious test failures (all tests deterministic)

### Regression Detection
The regression tests are specifically designed to fail with the original bugs:

| Test | With Original Bug | After Fix |
|------|-------------------|-----------|
| `regression_try_unmark_tasks_clears_status_not_summary` | ❌ FAIL | ✅ PASS |
| `regression_try_unmark_yield_clears_status_not_summary` | ❌ FAIL | ✅ PASS |
| `test_signal_waker_summary_vs_status_separation` | ❌ FAIL | ✅ PASS |

## Future Test Additions

### Consider Adding:
1. **Property-based tests** using `proptest` for state machine validation
2. **Loom tests** for exhaustive concurrency checking
3. **Performance benchmarks** for hot paths
4. **Integration with actual Runtime** end-to-end tests
5. **Fault injection** tests for error paths

## Maintenance

### When Adding New Features:
1. Add corresponding unit tests for new behavior
2. Update integration tests if signaling flow changes
3. Add regression tests if fixing bugs
4. Update this document with new test descriptions

### When Refactoring:
1. All existing tests must continue to pass
2. If changing semantics, update test expectations with clear justification
3. Maintain independence between status and summary fields
