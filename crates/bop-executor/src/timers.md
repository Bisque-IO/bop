# Timer Integration Design

This document captures how timers integrate with the executor runtime and workers. The goals are:

- keep the steady-state timer path lock-free for each worker;
- allow timers to survive worker shutdowns without task intervention;
- avoid cross-thread wheel manipulation for cancellations;
- provide a global monotonic time source shared across workers.

## Overview

- A dedicated timer thread acts as the time metronome. It increments per-worker `timer_now` atomics at a fixed resolution (e.g., every microsecond tick). Workers read their per-thread `timer_now` to know the current slot.
- Each worker owns a local `TimerWheel`. Scheduling/polling stay entirely local: no locks, no remote calls.
- Each timer handle stores:
  - An `Arc` to the worker-local shared state;
  - An `AtomicU8` state (`Scheduled`, `Cancelled`, `Fired`, `Migrating`, etc.);
  - An `AtomicU64` generation counter.
- The worker that schedules a timer increments the generation and writes `(deadline_tick, generation, handle_ptr)` into its wheel. On poll it only fires the entry if the stored generation matches the handle's current generation. Any stale entry (cancelled, migrated) is skipped.
- Cancellation just bumps the generation and flips the state to `Cancelled`. That invalidates any outstanding wheel entry without touching the wheel itself; all polling paths check generation before firing.

## Timer Thread

- Maintains a loop that sleeps until the next resolution tick, then:
  - updates each worker's `timer_now` atomic;
  - wakes workers that have pending buckets ready (e.g., via a lightweight `needs_poll` flag).
- Drains the global merge queue (see below) on each tick and redistributes orphaned timers to active workers by inserting them into the workers' wheels with their original deadlines.

## Worker Lifecycle

- **Scheduling:** When a task running on a worker schedules a timer:
  1. It increments the timer handle's generation.
  2. Stores `(deadline_tick, generation)` into its own `TimerWheel`.
  3. Sets the timer's state to `Scheduled`.
  4. Sets a `needs_poll` bit so the next `run_once` call performs a wheel check.
- **Polling:** During `run_once`, after processing pending tasks, the worker:
  1. Reads `timer_now` and decides if any wheel bucket is due.
  2. Drains due bucket entries. For each entry, it loads the timer handle, compares the stored generation, and reads its state.
  3. If `generation` matches and state is `Scheduled`, it marks the timer `Fired` and enqueues the corresponding task/callback.
  4. If state is `Cancelled` or generation mismatched, it drops the entry (no action).
- **Shutdown:** When a worker exits:
  1. It atomically swaps out its wheel into a per-worker owned container (e.g., `Vec<TimerEntry>`).
  2. Pushes the now-orphaned entries into a global `MergeQueue` serviced by the timer thread.
  3. No timers are dropped: every entry carries its generation and state, so subsequent workers can adopt them safely.

## Merge Queue

- Stores orphaned entries `(deadline_tick, generation, handle_ptr)`.
- The timer thread drains it and reassigns entries to currently-active workers (e.g., round-robin or based on load).
- Insertions check the timer's state: if it already moved to `Cancelled` or a newer generation, the entry is discarded.
- Because the timer thread is the only consumer, we can use a lock-free queue or a simple `Mutex<Vec<_>>` depending on throughput needs.

## Cancellation Flow

- Cancellation by user code:
  1. Atomically bump generation (e.g., `fetch_add(1)`).
  2. Set state to `Cancelled`.
  3. Optionally record a "cancelled" counter for debugging/stats.
- Workers will see the mismatched generation / state and ignore the entry. No immediate remove is necessary.

### Garbage Tracking

- Maintain a `StripedAtomicU64` on each wheel (e.g., 32 cache-line aligned stripes). Any thread cancelling or rescheduling increments one stripe. This lets remote workers report garbage without contending on a single atomic.
- Each bucket also tracks a local stale counter. When draining a bucket, the worker decrements the stripe counter(s) for each stale entry it discards.

### Lazy Compaction & Scrub Pass

- **Lazy compaction:** When the worker drains a due bucket, if its stale counter exceeds a threshold (absolute or ratio of stale/live), it rebuilds the bucket into a fresh `VecDeque`, copying only entries whose generation matches the handle. This is purely local and keeps the fast path light.
- **Background scrub:** After polling timers, the worker spends a small, bounded budget visiting the next bucket in round-robin order. If that bucket’s stale counter is high, it performs the same rebuild. This keeps far-future buckets clean without stalling the hot path.
- The worker periodically snapshots the striped counters to decide whether additional cleanup is necessary. Large increases can trigger more aggressive scrubbing or defer new migrations into that wheel until it is cleaned.

## Rescheduling / Reuse

- Rescheduling the same timer handle simply follows the scheduling flow again. Generation ensures wheel entries from previous schedules do not fire.
- The handle remains valid across worker hops because the timer state lives in an `Arc` outside any particular worker.

## Benefits

- **Lock-free fast path:** Inserting and polling timers on a worker uses only local data structures and atomic ops.
- **No cross-thread wheel mutation:** Cancels and reschedules happen by state/generation updates, not by touching the wheel.
- **Worker independence:** Timers survive worker shutdowns automatically via the merge queue serviced by the timer thread.
- **Deterministic time:** The dedicated timer thread provides consistent tick increments, avoiding divergence between workers.

## Open Questions / Follow-ups

- Selecting a good tick resolution / adapting dynamically.
- Balancing the merge queue: equal distribution vs. worker affinity.
- Exposing timer statistics (pending count, dropped, rescheduled) for monitoring.
- Ensuring the timer thread itself is resilient (e.g., if it falls behind).

This design keeps timers robust across worker churn without sacrificing the low-latency, lock-free characteristics needed in the executor hot path.
