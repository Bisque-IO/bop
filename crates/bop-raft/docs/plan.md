# bop-raft Wrapper Plan

## Objectives
- Provide a 100% safe, idiomatic facade over `bop_sys::bop_raft_*` while keeping zero-cost abstraction guarantees and predictable performance.
- Encapsulate ownership of every raw pointer exported by the C API inside RAII handles with explicit lifetimes, thread-safety markers, and fallbacks for advanced users.
- Deliver a cohesive developer experience that guides applications through configuration, launching, operating, and shutting down Raft clusters.
- Expose async-friendly APIs that integrate cleanly with Tokio/async-std without leaking callbacks or blocking threads.
- Ship production-ready documentation, examples, and test coverage that exercise both happy-path and failure-path behaviour.

## Non-Goals (for this wrapper crate)
- Re-implement the underlying consensus algorithms; we stay on the C/C++ implementation and do not fork logic.
- Provide storage/network transports other than what the C API exposes; those live in other crates (e.g., bop-usockets, MDBX integrations).
- Guarantee backwards compatibility with yet-to-be-stabilised C APIs; we follow semver driven by bop-sys headers.

## Current State Snapshot
- `src/lib.rs` is a single 900+ line module mixing types, wrappers, traits, and builder scaffolding; many sections are placeholders or commented out (e.g., `AsioService`, `RpcListener`).
- Callback shims for the state machine, state manager, log store, logger, and server events now expose safe traits; unit tests cover the log store and logger bridges.
- Async result helpers surface boolean, u64, and buffer completions with reusable future glue, but higher level abstractions still need ergonomic wrappers.
- Server control APIs grew snapshot scheduling, commit waiters, and pause/resume helpers yet still omit portions of the NuRaft lifecycle surface.
- Vector outputs (peer info, server config lists) are not implemented; TODOs mark missing ownership management.

## Constraints & Dependencies
- Relies on `bop-sys` bindgen output; regeneration may move symbol names but will stay `bop_raft_*` prefixed.
- Competes with `Send`/`Sync` restrictions from NuRaft ? we must only mark handles as thread safe when the upstream contract allows it.
- Integration targets Tokio by default, but must stay runtime-agnostic (use trait-based wakeups or `poll_fn`).
- Memory management must honour ownership transfer rules documented in `raft.h` (e.g., buffers passed into commands must not be double-freed).

## Design Principles
- Minimise unsafe: isolate raw FFI calls to `ffi.rs`/`raw` modules and keep the public API safe by default.
- Model fallible operations with `RaftResult<T>`; extend `RaftError` to carry error codes, strings, and context.
- Prefer borrowing over copying; when copies are necessary (e.g., building Rust `String` from C buffers) document cost clearly.
- Provide explicit builder/config patterns so that it is impossible to launch an invalid server configuration at compile time.
- Support feature flags (`mdbx`, `metrics`, `tracing`) so integrators can opt in to heavier dependencies.

## Proposed Module Layout
- `src/lib.rs`: lightweight prelude exposing modules (`pub mod buffer;`, `pub mod server;`, etc.) and re-exporting core types.
- `src/error.rs`: `RaftError`, `RaftResult`, conversion helpers from error codes/strings.
- `src/types.rs`: strong newtypes (`ServerId`, `Term`, etc.), enums (`CallbackType`, `PrioritySetResult`).
- `src/buffer.rs`: `Buffer`, owned/borrowed views, conversions from `Vec<u8>`/`Bytes`, unsafe constructors for FFI edge-cases.
- `src/config.rs`: `ServerConfig`, `ClusterConfig`, `RaftParams`, builder helpers, iterator wrappers around FFI vectors.
- `src/log_entry.rs`: `LogEntry`, log entry vectors, conversions to/from owned payloads.
- `src/state.rs`: `ServerState`, `Snapshot`, peer info wrappers, async snapshot results.
- `src/async_result.rs`: wrappers for `AsyncBool`, `AsyncU64`, `AsyncBuffer`, `AsyncLogEntries` plus `Future` adapters.
- `src/metrics.rs`: `Counter`, `Gauge`, `Histogram` RAII + typed getters.
- `src/callbacks/`: submodules for `state_machine`, `state_manager`, `log_store`, `logger`, event callbacks; expose safe traits and hidden FFI shims.
- `src/server/`: `handle.rs` (owning `RaftServerPtr`), `builder.rs`, `control.rs` (lifecycle/membership APIs), `peer.rs`, `snapshot.rs`.
- `src/mdbx.rs`: optional feature module wrapping `bop_raft_mdbx_*` helpers and configuration structs.
- `src/runtime.rs`: optional async integration utilities (Tokio task wrappers, waker registration for async results).
- `examples/`: expand with runnable scenarios (`simple_cluster.rs`, `snapshot.rs`, `metrics.rs`).

## Ownership & Safety Strategy
- Every raw pointer type (`bop_raft_*_ptr`) gets a dedicated wrapper that implements `Drop` calling the matching `*_delete`/`*_free` function. Borrowed handles (e.g., `struct bop_raft_server` returned by `bop_raft_server_get`) are represented as non-owning newtypes referencing the owning pointer.
- Introduce `UnsafeSendSync` marker trait to gate `unsafe impl Send/Sync` behind audit macros; only apply when upstream documentation certifies thread-safety.
- `ServerConfig`, `ClusterConfig`, `Buffer`, `Snapshot` expose safe methods returning owned Rust types (Strings, Vec<u8>) by copying out of C buffers while preventing double-free by transferring ownership when required (via `forget` or dedicated `IntoRaw`).
- For callback shims, store trait object inside an `Arc<Mutex<T>>` referenced by the `user_data` pointer passed to C. Provide helper struct `CallbackRegistry` to manage lifetime and to drop gracefully when server shuts down.
- Wrap vector-like outputs (peer info, config lists) using safe iterators that allocate a temporary RAII vector handle, read elements into Rust structs, then release the C vector.

## FFI Coverage Map (high level)
- Buffers & snapshots: `bop_raft_buffer_*`, `bop_raft_snapshot_*` ? `Buffer`, `Snapshot`.
- Configurations: `bop_raft_srv_config_*`, `bop_raft_cluster_config_*`, vector helpers ? config module & iterators.
- Parameters & tuning: `bop_raft_params_*`, update/get server params ? `RaftParams` builder + `RaftServer::params()`.
- State machine: `bop_raft_fsm_make/delete`, adjust commit index helpers ? `StateMachine` trait with `StateMachineHandle` shim.
- State manager: `bop_raft_state_mgr_make/delete` ? `StateManagerBuilder` bridging trait `PersistentState`.
- Log store: `bop_raft_log_store_make/delete` etc. ? `LogStore` trait + `LogStoreHandle`.
- Server lifecycle & membership: `bop_raft_server_launch/get/stop/...` through to leadership, snapshots, pause/resume, wait_for_commit.
- Async results: `bop_raft_async_bool/u64/buffer/log_entries` -> safe wrappers + `Future` adaptors.
- Metrics: `bop_raft_counter/gauge/histogram` -> typed stat wrappers.
- Event callbacks & request contexts: `bop_raft_cb_*` -> `ServerCallbacks` trait.
- MDBX helpers: `bop_raft_mdbx_state_mgr_open`, `bop_raft_mdbx_log_store_open` with typed configurators.

## API Surface Details
### Base Types & Traits
- Keep current newtypes but move into `types.rs`, implement conversions, serde (feature gated), Display/Debug.
- Expand `CallbackType` and `PrioritySetResult` enums to match the exact C enumerations; ensure `repr(i32)` and exhaustive mapping.

### Buffers & Binary Payloads
- `Buffer::from_slice` to allocate and copy Rust bytes into NuRaft buffer.
- `Buffer::into_vec` consuming the RAII wrapper.
- Borrowed reader `BufferView` for functions returning borrowed pointer without transfer.

### Configuration Builders
- `ServerConfigBuilder` to compute `CString` endpoints/aux data, ensure ASCII/UTF-8 validation, and set optional flags (learner, priority, joiner).
- `ClusterConfig` to own vector of `ServerConfig` (with cloning support) and `user_ctx` accessors.
- `RaftParams` typed setters for each field in `raft.h`: election timeouts, append threads, snapshots, priority; emit default config via `Default` and `serde` feature.

### Log Entries & Commands
- Provide `LogEntry::new(term, payload, entry_type)` plus getters for timestamp and custom flags.
- Manage the log entry vector helper for append/batch operations and bridging to user log store trait.

### Server State Introspection
- `ServerState` to expose term, voted_for, snapshot flags.
- `PeerInfo` to convert from FFI vector; `RaftServer::peer_infos()` returns iterator of typed structs.

### Async Command Results
- Safe wrappers for bool/u64/buffer/log_entries. Each exposes both callback-based API and `Future` conversion (using `oneshot` + callback registration). Provide cancellation semantics by dropping the future.

### Snapshot Management
- High-level API for `create_snapshot`, `schedule_snapshot`, `last_snapshot_idx`, `pause_state_machine_execution`, `wait_for_state_machine_pause`.

### Metrics & Monitoring
- Provide typed wrappers for counters/gauges/histograms, with builder to construct known stat names, retrieval APIs returning typed results, and resets.
- Bridge to `metrics`/`opentelemetry` via optional feature to auto-export stats.

### Server Lifecycle & Control
- `RaftServer::launch` takes builder struct bundling user trait objects and runtime options. Returns `RaftServerHandle` that dereferences to a borrowed `RaftServer`.
- Expose membership operations (`add_server`, `remove_server`, `flip_learner_flag`) with async handlers. Provide synchronous convenience methods that await completion using futures.
- Provide context operations: `set_user_context`, `user_context()` to return `Vec<u8>`.
- Implement leadership helpers (`request_leadership`, `yield_leadership`, `restart_election_timer`, `is_leader_alive`, etc.).
- `RaftServerHandle::start` / `stop` / `shutdown` methods align with C `start_server`, `stop_server`, etc.

### Callback Surfaces
- `StateMachine` trait matching commit/pre-commit/apply snapshot flows; supply `StateMachineAdapter` that marshals data using buffers and ensures snapshot user contexts are freed.
- `StateManager` trait with load/save state/config/log store bridging. Provide default implementations keyed by `PersistentState` trait.
- `LogStore` trait capturing the full set of synchronous callbacks; include `WriteBatchContext` to guide batching hints.
- `ServerCallbacks` trait for event notifications from `bop_raft_cb_func`; unify callback dispatch into enums with rich context structs.
- `Logger` trait to hook into application logging (level, component, message fields).

### MDBX Helpers (feature `mdbx`)
- Provide safe wrappers for `bop_raft_mdbx_state_mgr_open` and `bop_raft_mdbx_log_store_open` using typed configuration structs (`MdbxEnvConfig`, `MdbxLogStoreConfig`).
- Optional integration with crate-level builder to auto-wire MDBX persistence.

## Async & Concurrency Integration
- Provide `AsyncResult<T>` generic built on top of `Arc<Inner<T>>` storing result/error and `AtomicWaker` to support `Future` implementations.
- Offer `ToFuture` trait implemented for `AsyncBool`, `AsyncU64`, etc., returning `impl Future<Output = RaftResult<T>>`.
- For operations that block (e.g., `stop`), wrap with `spawn_blocking` helpers or clearly document blocking behaviour.
- Ensure callback shims invoke user closures on a runtime-friendly executor ? default to spawning onto `tokio::runtime::Handle::current()` if available, otherwise execute inline with best-effort error logging.

## Error Handling & Diagnostics
- Extend `RaftError` with variants for FFI error strings (capture C char*), command result errors, callback panics (wrap in `Arc<Any>`), and IO/storage wrappers.
- Provide `ErrorContext` helper macros to annotate failing FFI calls with operation name, server id, etc.
- Integrate with `tracing` via optional feature for structured logs.

## Testing & Validation Strategy
- Unit tests per module verifying RAII behaviour (drop frees pointer), conversions (buffer <-> Vec), and builder validation.
- Mock FFI tests using `#[cfg(test)]` stub implementations that simulate bop_raft behaviour to exercise callback shims without needing full server.
- Integration tests (behind feature flag or `cargo test -- --ignored`) spinning up in-process Raft server with simple state machine/log store to validate membership changes, snapshot flows, async results.
- Fuzz tests for parsing config strings and log entry packing (libfuzzer or cargo-fuzz, optional).
- CI tasks running `xmake run test-uws` (existing harness) plus Rust `cargo test -p bop-raft`.

## Documentation & Examples
- Expand crate-level docs with quick-start showing builder usage and state machine implementation skeleton.
- Provide module-level docs explaining ownership contracts and unsafe escape hatches.
- Write cookbook examples covering: single-node bootstrap, dynamic membership, custom state machine, snapshot restore, metrics scraping, MDBX persistence.

## Implementation Roadmap
1. **Scaffold modules & errors**: create module tree, move existing types into dedicated files, confirm existing tests still pass.
2. **Base RAII & utilities**: complete buffer/config/state wrappers, ensure all `*_new`/`*_delete` pairs are covered, implement `PeerInfo` vector handling.
3. **Callback adapters**: implement state machine, state manager, log store, and logger shims with safety audits and unit tests.
4. **Server lifecycle**: finish `RaftServer::launch`, builder wiring, lifecycle controls, membership operations with async results.
5. **Async integration**: wrap cmd results, expose futures, implement wait-for-commit helpers and snapshot scheduling futures.
6. **Metrics & observability**: surface counters/gauges/histograms, optional tracing/metrics features, logging integration.
7. **MDBX & advanced features**: add persistence helpers, parameter hot-reload (`update_params`), state machine pause/resume, leadership transfer helpers.
8. **Docs & examples**: write guides, API docs, and runnable examples; ensure `cargo doc` warnings resolved.
9. **QA & polish**: audit `unsafe`, add clippy configuration, bench smoke tests, ensure MSRV (document, enforce in CI).

## Open Questions & Risks
- Confirm thread-safety guarantees for each pointer type (need upstream documentation or empirical validation before marking `Send/Sync`).
- Determine default runtime strategy for executing callbacks when no async runtime is present; consider exposing a pluggable executor trait.
- Clarify ownership rules for buffers returned by callbacks (e.g., does NuRaft free command buffers after callback returns?). Gather authoritative guidance from C headers or maintainers.
- Validate whether `bop_raft_server_launch` expects the builder to retain `params_given` pointer beyond the call (likely yes); design builder to keep owned handle alive.
- Investigate lifetime of `Logger` callbacks: must remain valid until server shutdown; ensure `Arc` reference counting covers this.
- Evaluate need for interior mutability wrappers (e.g., `Mutex`, `RwLock`) around trait objects to allow mutation across callbacks without deadlocks.

## Developer Experience Enhancements
- Provide `#[cfg(feature = "testkit")]` helpers that spin up in-process clusters for unit tests.
- Offer `cargo xtask` or `bake` integration to regenerate bindings and run conformance suites.
- Add `deny(missing_docs)` once API surface stabilises to keep documentation comprehensive.

