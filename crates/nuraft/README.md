# bop-raft

A safe, in-progress wrapper around the BOP NuRaft C++ API. The crate focuses on building ergonomics for state machine callbacks, log stores, and server lifecycle management.

## Highlights

- Safe trait shims for state machine, state manager, log store, logger, and server events.
- Async command bridges (AsyncBool, AsyncU64, AsyncBuffer) with Future adapters for commit waiters and snapshot operations.
- Metrics helpers for counters, gauges, and histograms with convenience accessors on `RaftServer`.
- Builder support for wiring user-provided callbacks before launching the server.
- Hot-reloadable `RaftParams` with ergonomic helpers (`RaftServer::update_params` / `reload_params`) and reconnection controls.
- Read-only cluster membership snapshots with change detection utilities for external observers.
- Optional MDBX-backed persistence (`--features mdbx`) with RAII wrappers, builder integration, and an example (`examples/mdbx_cluster.rs`).

## Mocked launch sketch

```ignore
use bop_raft::{
    AsioService, Buffer, DataCenterId, LogEntryRecord, LogEntryView, LogIndex, MdbxStorage,
    RaftParams, RaftServerBuilder, ServerConfig, ServerId, StateMachine, StateManagerInterface,
    LogStoreInterface, Term,
};
use std::sync::{Arc, Mutex};

struct MockStateMachine;
impl StateMachine for MockStateMachine {
    fn apply(&mut self, _idx: LogIndex, _data: &[u8]) -> bop_raft::RaftResult<Option<Vec<u8>>> {
        Ok(None)
    }
    fn take_snapshot(&self) -> bop_raft::RaftResult<Buffer> { Buffer::from_bytes(&[]) }
    fn restore_from_snapshot(&mut self, _snapshot: &[u8]) -> bop_raft::RaftResult<()> { Ok(()) }
    fn last_applied_index(&self) -> LogIndex { LogIndex(0) }
}

#[derive(Default, Clone)]
struct MockLogStore { entries: Arc<Mutex<Vec<LogEntryRecord>>> }
impl LogStoreInterface for MockLogStore { /* stubbed methods, see examples/raft_example.rs */ }

struct MockStateManager;
impl StateManagerInterface for MockStateManager { /* load/save stubs returning Ok(()) */ }

fn acquire_native_asio_service() -> bop_raft::RaftResult<AsioService> {
    // Wrap bop_sys::bop_raft_asio_service_make once the helper is available.
    todo!("Provide an AsioService handle from bop-sys");
}

let server_config = ServerConfig::new(
    ServerId::new(1),
    DataCenterId::new(0),
    "127.0.0.1:5000",
    "node-a",
    false,
)?;

let mut params = RaftParams::new()?;
params.set_election_timeout(150, 450);
params.set_heart_beat_interval(120);

let builder = RaftServerBuilder::new()
    // Acquire a real AsioService from bop-sys; placeholder shown here.
    .asio_service(acquire_native_asio_service()?)
    .params(params.clone())
    .with_mdbx_storage(MdbxStorage::new(server_config, "/var/lib/raft"))
    .state_machine(Box::new(MockStateMachine))
    .state_manager(Box::new(MockStateManager))
    .log_store(Box::new(MockLogStore::default()));
// builder.build() still requires real handles from bop-sys.
```

See `examples/mdbx_cluster.rs` for a stand-alone snippet that wires up the MDBX backend,
captures membership snapshots, and demonstrates parameter hot reload without launching a
full server instance.

Refer to examples/raft_example.rs for the full stubbed implementations.
