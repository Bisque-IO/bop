//! Builder wiring examples with mocked callbacks.

#![allow(clippy::needless_return)]

use bop_raft::buffer::Buffer;
use bop_raft::error::RaftResult;
use bop_raft::traits::{
    AdjustCommitIndex, LogStoreInterface, SnapshotChunk, SnapshotCreation, SnapshotMetadata,
    SnapshotReadResult, SnapshotReader, StateMachine, StateManagerInterface,
};
use bop_raft::{
    LogEntryRecord, LogEntryView, LogIndex, RaftParams, RaftServerBuilder, SnapshotRef, Term,
};

fn main() {
    mocked_launch_example().unwrap();
}

#[allow(dead_code)]
fn mocked_launch_example() -> RaftResult<()> {
    use bop_raft::AsioService;
    use std::sync::{Arc, Mutex};

    struct MockStateMachine;
    impl StateMachine for MockStateMachine {
        fn apply(&mut self, _log_idx: LogIndex, _data: &[u8]) -> RaftResult<Option<Buffer>> {
            Ok(None)
        }
        fn take_snapshot(&self) -> RaftResult<Buffer> {
            Buffer::from_bytes(&[])
        }
        fn restore_from_snapshot(&mut self, _snapshot: &[u8]) -> RaftResult<()> {
            Ok(())
        }
        fn last_applied_index(&self) -> LogIndex {
            LogIndex(0)
        }

        fn save_snapshot_chunk(&mut self, _chunk: SnapshotChunk<'_>) -> RaftResult<()> {
            Ok(())
        }

        fn apply_snapshot(&mut self, _snapshot: SnapshotMetadata<'_>) -> RaftResult<bool> {
            Ok(true)
        }

        fn snapshot_reader(&mut self) -> RaftResult<Box<dyn SnapshotReader>> {
            Ok(Box::new(MockSnapshotReader::default()))
        }

        fn finalize_snapshot_reader(&mut self, _reader: Box<dyn SnapshotReader>) -> RaftResult<()> {
            Ok(())
        }

        fn last_snapshot(&self) -> RaftResult<Option<SnapshotRef>> {
            Ok(None)
        }

        fn create_snapshot(&mut self, _snapshot: SnapshotCreation<'_>) -> RaftResult<()> {
            Ok(())
        }

        fn adjust_commit_index(&mut self, info: AdjustCommitIndex) -> RaftResult<LogIndex> {
            Ok(info.expected)
        }
    }

    #[derive(Default)]
    struct MockSnapshotReader;

    impl SnapshotReader for MockSnapshotReader {
        fn next(&mut self, _object_id: u64) -> RaftResult<SnapshotReadResult> {
            Ok(SnapshotReadResult::End)
        }
    }

    struct MockStateManager;
    impl StateManagerInterface for MockStateManager {
        fn load_state(&self) -> RaftResult<Option<bop_raft::ServerState>> {
            Ok(None)
        }
        fn save_state(&mut self, _state: bop_raft::state::ServerStateView<'_>) -> RaftResult<()> {
            Ok(())
        }
        fn load_config(&self) -> RaftResult<Option<bop_raft::ClusterConfig>> {
            Ok(None)
        }
        fn save_config(
            &mut self,
            _config: bop_raft::config::ClusterConfigView<'_>,
        ) -> RaftResult<()> {
            Ok(())
        }
    }

    #[derive(Default, Clone)]
    struct MockLogStore {
        entries: Arc<Mutex<Vec<LogEntryRecord>>>,
    }

    impl LogStoreInterface for MockLogStore {
        fn start_index(&self) -> LogIndex {
            LogIndex(0)
        }
        fn next_slot(&self) -> LogIndex {
            LogIndex(self.entries.lock().unwrap().len() as u64 + 1)
        }
        fn last_entry(&self) -> RaftResult<Option<LogEntryRecord>> {
            Ok(self.entries.lock().unwrap().last().cloned())
        }
        fn append(&mut self, entry: LogEntryView<'_>) -> RaftResult<LogIndex> {
            let mut entries = self.entries.lock().unwrap();
            entries.push(LogEntryRecord::from_view(entry));
            Ok(LogIndex(entries.len() as u64))
        }
        fn write_at(&mut self, idx: LogIndex, entry: LogEntryView<'_>) -> RaftResult<()> {
            let mut entries = self.entries.lock().unwrap();
            let record = LogEntryRecord::from_view(entry);
            if idx.0 as usize <= entries.len() {
                entries[idx.0 as usize - 1] = record;
            }
            Ok(())
        }
        fn end_of_append_batch(&mut self, _start: LogIndex, _count: u64) -> RaftResult<()> {
            Ok(())
        }
        fn log_entries(&self, start: LogIndex, end: LogIndex) -> RaftResult<Vec<LogEntryRecord>> {
            let entries = self.entries.lock().unwrap();
            Ok(entries[start.0 as usize..end.0 as usize].to_vec())
        }
        fn entry_at(&self, idx: LogIndex) -> RaftResult<Option<LogEntryRecord>> {
            Ok(self.entries.lock().unwrap().get(idx.0 as usize).cloned())
        }
        fn term_at(&self, _idx: LogIndex) -> RaftResult<Option<Term>> {
            Ok(Some(Term(1)))
        }
        fn pack(&self, _idx: LogIndex, _cnt: i32) -> RaftResult<Buffer> {
            Buffer::from_bytes(&[])
        }
        fn apply_pack(&mut self, _idx: LogIndex, _pack: &[u8]) -> RaftResult<()> {
            Ok(())
        }
        fn compact(&mut self, _last_log_index: LogIndex) -> RaftResult<bool> {
            Ok(true)
        }
        fn compact_async(&mut self, _last_log_index: LogIndex) -> RaftResult<bool> {
            Ok(true)
        }
        fn flush(&mut self) -> RaftResult<bool> {
            Ok(true)
        }
        fn last_durable_index(&self) -> LogIndex {
            LogIndex(0)
        }
    }

    let state_machine = Box::new(MockStateMachine);
    let state_manager = Box::new(MockStateManager);
    let log_store = Box::new(MockLogStore::default());
    let asio_service = unsafe { std::mem::MaybeUninit::<AsioService>::zeroed().assume_init() };
    let params = RaftParams::default();

    let _builder = RaftServerBuilder::new()
        .asio_service(asio_service)
        .params(params)
        .state_machine(state_machine)
        .state_manager(state_manager)
        .log_store(log_store);

    Ok(())
}
