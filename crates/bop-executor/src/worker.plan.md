# Worker/Service Refactor Plan

## Goals
- Centralize worker lifecycle under `WorkerService`, including timer orchestration and message mesh.
- Ensure timer wheel integration uses a shared, service-driven notion of time.
- Build a lazy mesh of per-worker `mpsc::Sender<WorkerMessage>` handles for cross-worker communication.

## Work Breakdown
1. **Service/Core Wiring** ✅
   - Fix `WorkerService::workers` storage to a growable, addressable collection (e.g. `Vec<WorkerState>`).
   - Initialize the `min_workers` slots in `start`, passing `Weak<WorkerService>` and arena clones.
   - Track worker identifiers and expose helpers for worker lookup / mesh access.
2. **Worker Timer Plumbing**
   - Add reusable timer output buffer and update `poll_timers` to pull `now_ns` from the atomic state.
   - Process `WorkerMessage`s inside the worker loop to handle remote scheduling and merges.
   - Ensure timer TLS setup/teardown aligns with the new message pathways.
3. **Mesh Construction**
   - Extend `Worker` to request peer senders lazily via the service, caching results.
   - Populate senders on first use and guarantee each worker has a unique clone for each peer.
   - Add helper APIs on `WorkerService` to coordinate mesh sender registration.
4. **Timing Thread**
   - Spawn a dedicated service thread that periodically updates each worker’s `now_ns` and triggers timer polls.
   - Respect `next_tick` scheduling to minimize unnecessary wake-ups.
5. **Validation**
   - Add targeted tests covering cross-worker timer scheduling, message routing, and timer migration on worker drop.
   - Run existing worker/timer tests to confirm regressions are avoided.

## Open Questions
- How should `WorkerService` surface handles for external components to submit `WorkerMessage`s (e.g. via the mesh)?
- Do we need backpressure or batching guarantees beyond `TimerBatch` for mesh sends?
- What cadence should the timing thread use relative to `TimerWheel` resolution to balance precision and overhead?
