# bop-raft

A safe, in-progress wrapper around the BOP NuRaft C++ API. The crate focuses on building ergonomics for state machine callbacks, log stores, and server lifecycle management.

## Highlights

- Safe trait shims for state machine, state manager, log store, logger, and server events.
- Async command bridges (AsyncBool, AsyncU64, AsyncBuffer) with Future adapters for commit waiters and snapshot operations.
- Metrics helpers for counters, gauges, and histograms with convenience accessors on RaftServer.
- Builder support for wiring user-provided callbacks before launching the server.

## Mocked launch sketch

```ignore
use bop_raft::{
    AsioService, LogEntryRecord, LogEntryView, LogIndex, RaftParams, RaftServerBuilder,
    StateMachine, StateManagerInterface, LogStoreInterface, Buffer, Term
};
use std::sync::{Arc, Mutex};

struct MockStateMachine;
impl StateMachine for MockStateMachine {
    fn apply(&mut self, _idx: LogIndex, _data: &[u8]) -> bop_raft::RaftResult<Option<Vec<u8>>> { Ok(None) }
    fn take_snapshot(&self) -> bop_raft::RaftResult<Buffer> { Buffer::from_bytes(&[]) }
    fn restore_from_snapshot(&mut self, _snapshot: &[u8]) -> bop_raft::RaftResult<()> { Ok(()) }
    fn last_applied_index(&self) -> LogIndex { LogIndex(0) }
}

#[derive(Default, Clone)]
struct MockLogStore { entries: Arc<Mutex<Vec<LogEntryRecord>>> }
impl LogStoreInterface for MockLogStore { /* stubbed methods, see examples/raft_example.rs */ }

struct MockStateManager;
impl StateManagerInterface for MockStateManager { /* load/save stubs returning Ok(()) */ }

let asio_service = unsafe { std::mem::MaybeUninit::<AsioService>::zeroed().assume_init() };
let params = RaftParams::default();
let builder = RaftServerBuilder::new()
    .asio_service(asio_service)
    .params(params)
    .state_machine(Box::new(MockStateMachine))
    .state_manager(Box::new(MockStateManager))
    .log_store(Box::new(MockLogStore::default()));
// builder.build() requires real handles from bop-sys.
```

Refer to examples/raft_example.rs for the full stubbed implementations.
