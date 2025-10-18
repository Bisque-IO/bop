# Timers Module Test Plan

## Scope
- timer core (context.rs, handle.rs, service.rs, worker.rs, sleep.rs)
- wheel data path: scheduling, polling, generation checks, stale accounting
- cancellation/reschedule, garbage counters, merge queue redistribution, background scrub
- async sleep integration and worker interaction via `tests`/timers.rs

## Preconditions
- deterministic tick sizing via TimerConfig
- controlled worker count (single worker for unit, multi-worker harness for integration)
- guard asynchronous waits with bounded retries and TimerWorkerShared::mark_needs_poll

## Unit Coverage
- TimerHandle: generation bumps, cancel idempotence, payload swap hygiene
- TimerWheelContext: schedule/poll paths, stale vs live bucket heuristics, scrub triggers
- TimerService: merge queue draining, round-robin worker selection, cancellation notifications
- StripedAtomicU64: stripe indexing, saturating subtract/increment semantics
- negative-path assertions: double cancel, migrating handles, stale generation comparisons

## Integration Scenarios
- single-worker deadlines firing through Worker::poll
- cross-worker migration via drained buckets and merge inbox handoff
- background scrub cleaning far-future stale entries
- async `sleep::sleep` waking a task through the arena
- shutdown flow: WorkerTimers::drop draining entries into merge queue
- interleaved cancellation vs poll to exercise race handling

## Stress & Concurrency
- rapid schedule/cancel/reschedule loops per worker while timer thread advances
- multi-worker merge contention with burst enqueues
- tick skew simulations (TimerWorkerShared::update_now) to validate stale detection
- optional Loom-style exhaustive runs when cfg permits

## Fault Injection
- drop timer handles mid-flight to test Arc ownership
- simulate worker failure via migrate_pending
- drive garbage stripes toward saturation for counter validation

## Performance / Soak
- long-running high-volume timer test ensuring bucket sizes remain bounded
- monitor `garbage.total`, stale ratio, scrub cadence

## Instrumentation & Metrics
- capture per-worker stats (garbage stripes, scrub cursor) during tests
- expose debug hooks for threshold tuning validation

## Automation
- ensure deterministic unit suites under `cargo test -p bop-executor timers::context::tests`
- add integration coverage in `tests/timers.rs` for multi-worker flows
- mark optional soak tests with #[ignore] for nightly execution
- run test matrix across varied tick durations via helper constructors

## Exit Criteria
- all outlined unit/integration cases implemented with stable assertions
- soak/ignored tests pass for configured duration without regressions
- no outstanding race reproductions
- timers.plan.md Section 7/8 checklist aligned with executed coverage

