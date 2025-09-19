# AOF2 Constitution

## Blocking and Async Semantics
- `append_record` must remain a non-blocking hot path. It may return `AofError::WouldBlock` rather than waiting.
- `append_record_with_timeout` is the sanctioned synchronous, potentially blocking variant used when callers explicitly opt in to waiting.
- `append_record_async` should call `append_record` first and, on `AofError::WouldBlock`, fall back to `append_record_with_timeout` via `tokio::task::spawn_blocking` to avoid stalling async workers.
- Reader APIs must mirror the writer model: provide a synchronous method that can return `AofError::WouldBlock`, and an async wrapper that waits cooperatively without blocking the runtime. Pure read paths never need to hop onto the blocking pool.
- Public-facing APIs must provide deterministic behaviour—no hidden blocking or backoff beyond what the signature promises.
- Never block a Tokio runtime worker; any filesystem, mmap, or other potentially blocking operation must execute inside `spawn_blocking` (or equivalent) so cooperative multitasking is preserved.
- `read_record` remains a synchronous call that may return `AofError::WouldBlock`.
- `read_record_async` must first attempt the non-blocking path and, on `WouldBlock`, await additional readiness using the async notification mechanism. It never uses the blocking pool because reads complete without filesystem syscalls.


## Recovery and Metrics
- Recovery routines must reseed both the flush queue and the global backlog gauge so restarted nodes resume outstanding durability work immediately.
- `FlushMetricsSnapshot` must include retry counters and the live `backlog_bytes` gauge; update it after every append, recovery pass, or flush completion.

## Testing Expectations
- Every new blocking path requires a regression test that exercises both the WouldBlock fast path and the blocking fallback.
- Recovery changes must be covered by restart-oriented tests so crash-mode invariants remain enforced.

