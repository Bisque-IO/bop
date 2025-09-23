use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::Mutex;

use bop_sys::{
    bop_raft_buffer, bop_raft_log_entry, bop_raft_log_entry_ptr, bop_raft_log_entry_vec_push,
    bop_raft_log_entry_vector, bop_raft_log_store_delete, bop_raft_log_store_make,
    bop_raft_log_store_ptr,
};

use crate::buffer::Buffer;
use crate::error::{RaftError, RaftResult};
use crate::log_entry::{LogEntryRecord, LogEntryView};
use crate::traits::LogStoreInterface;
use crate::types::{LogIndex, Term};

pub(crate) struct LogStoreHandle {
    ptr: NonNull<bop_raft_log_store_ptr>,
    adapter: *mut LogStoreAdapter,
}

impl LogStoreHandle {
    pub(crate) fn new(log_store: Box<dyn LogStoreInterface>) -> RaftResult<Self> {
        let adapter = Box::new(LogStoreAdapter::new(log_store));
        let adapter_ptr = Box::into_raw(adapter);

        let log_store_ptr = unsafe {
            bop_raft_log_store_make(
                adapter_ptr as *mut c_void,
                Some(log_store_next_slot),
                Some(log_store_start_index),
                Some(log_store_last_entry),
                Some(log_store_append),
                Some(log_store_write_at),
                Some(log_store_end_of_append_batch),
                Some(log_store_log_entries),
                Some(log_store_entry_at),
                Some(log_store_term_at),
                Some(log_store_pack),
                Some(log_store_apply_pack),
                Some(log_store_compact),
                Some(log_store_compact_async),
                Some(log_store_flush),
                Some(log_store_last_durable_index),
            )
        };

        let ptr = match NonNull::new(log_store_ptr) {
            Some(ptr) => ptr,
            None => {
                unsafe {
                    drop(Box::from_raw(adapter_ptr));
                }
                return Err(RaftError::NullPointer);
            }
        };

        Ok(Self {
            ptr,
            adapter: adapter_ptr,
        })
    }

    pub(crate) fn as_ptr(&self) -> *mut bop_raft_log_store_ptr {
        self.ptr.as_ptr()
    }

    #[allow(dead_code)]
    pub(crate) fn take_last_error(&self) -> Option<RaftError> {
        unsafe { (&*self.adapter).take_last_error() }
    }

    #[cfg(test)]
    pub(super) fn adapter_ptr(&self) -> *mut c_void {
        self.adapter as *mut c_void
    }
}

impl Drop for LogStoreHandle {
    fn drop(&mut self) {
        unsafe {
            bop_raft_log_store_delete(self.ptr.as_ptr());
            drop(Box::from_raw(self.adapter));
        }
    }
}

unsafe impl Send for LogStoreHandle {}
unsafe impl Sync for LogStoreHandle {}

struct LogStoreAdapter {
    log_store: Mutex<Box<dyn LogStoreInterface>>,
    last_error: Mutex<Option<RaftError>>,
}

impl LogStoreAdapter {
    fn new(log_store: Box<dyn LogStoreInterface>) -> Self {
        Self {
            log_store: Mutex::new(log_store),
            last_error: Mutex::new(None),
        }
    }

    fn with_store<R>(
        &self,
        f: impl FnOnce(&mut dyn LogStoreInterface) -> RaftResult<R>,
    ) -> RaftResult<R> {
        let mut guard = self
            .log_store
            .lock()
            .map_err(|_| RaftError::LogStoreError("Log store mutex poisoned".into()))?;
        f(guard.as_mut())
    }

    fn record_error(&self, err: RaftError) {
        if let Ok(mut guard) = self.last_error.lock() {
            *guard = Some(err);
        }
    }

    #[allow(dead_code)]
    fn take_last_error(&self) -> Option<RaftError> {
        self.last_error
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }
}

fn view_from_raw<'a>(
    term: u64,
    timestamp: u64,
    data: *mut u8,
    size: usize,
    has_crc32: bool,
    crc32: u32,
) -> LogEntryView<'a> {
    let payload = if size == 0 || data.is_null() {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data as *const u8, size) }
    };
    LogEntryView::new(Term(term), timestamp, payload, has_crc32.then_some(crc32))
}

unsafe extern "C" fn log_store_next_slot(user_data: *mut c_void) -> u64 {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| Ok(store.next_slot().0)) {
        Ok(idx) => idx,
        Err(err) => {
            adapter.record_error(err);
            0
        }
    }
}

unsafe extern "C" fn log_store_start_index(user_data: *mut c_void) -> u64 {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| Ok(store.start_index().0)) {
        Ok(idx) => idx,
        Err(err) => {
            adapter.record_error(err);
            0
        }
    }
}

unsafe extern "C" fn log_store_last_entry(user_data: *mut c_void) -> *mut bop_raft_log_entry {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| store.last_entry()) {
        Ok(Some(record)) => match record.into_ffi_entry() {
            Ok(entry) => entry.into_raw(),
            Err(err) => {
                adapter.record_error(err);
                ptr::null_mut()
            }
        },
        Ok(None) => ptr::null_mut(),
        Err(err) => {
            adapter.record_error(err);
            ptr::null_mut()
        }
    }
}

unsafe extern "C" fn log_store_append(
    user_data: *mut c_void,
    _ptr: bop_raft_log_entry_ptr,
    term: u64,
    data: *mut u8,
    data_size: usize,
    timestamp: u64,
    has_crc32: bool,
    crc32: u32,
) -> u64 {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    let view = view_from_raw(term, timestamp, data, data_size, has_crc32, crc32);
    match adapter.with_store(|store| store.append(view)) {
        Ok(idx) => idx.0,
        Err(err) => {
            adapter.record_error(err);
            0
        }
    }
}

unsafe extern "C" fn log_store_write_at(
    user_data: *mut c_void,
    index: u64,
    term: u64,
    data: *mut u8,
    data_size: usize,
    timestamp: u64,
    has_crc32: bool,
    crc32: u32,
) {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    let view = view_from_raw(term, timestamp, data, data_size, has_crc32, crc32);
    if let Err(err) = adapter.with_store(|store| store.write_at(LogIndex(index), view)) {
        adapter.record_error(err);
    }
}

unsafe extern "C" fn log_store_end_of_append_batch(user_data: *mut c_void, start: u64, count: u64) {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    if let Err(err) = adapter.with_store(|store| store.end_of_append_batch(LogIndex(start), count))
    {
        adapter.record_error(err);
    }
}

unsafe extern "C" fn log_store_log_entries(
    user_data: *mut c_void,
    vec_ptr: *mut bop_raft_log_entry_vector,
    start: u64,
    end: u64,
) -> bool {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    let vec_ptr = if vec_ptr.is_null() {
        adapter.record_error(RaftError::NullPointer);
        return false;
    } else {
        vec_ptr
    };

    match adapter.with_store(|store| store.log_entries(LogIndex(start), LogIndex(end))) {
        Ok(entries) => {
            for record in entries {
                let LogEntryRecord {
                    term,
                    timestamp,
                    payload,
                    crc32,
                } = record;

                match Buffer::from_vec(payload) {
                    Ok(buffer) => {
                        let raw = buffer.into_raw();
                        unsafe {
                            bop_raft_log_entry_vec_push(
                                vec_ptr,
                                term.0,
                                raw,
                                timestamp,
                                crc32.is_some(),
                                crc32.unwrap_or(0),
                            );
                        }
                    }
                    Err(err) => {
                        adapter.record_error(err);
                        return false;
                    }
                }
            }
            true
        }
        Err(err) => {
            adapter.record_error(err);
            false
        }
    }
}

unsafe extern "C" fn log_store_entry_at(
    user_data: *mut c_void,
    index: u64,
) -> *mut bop_raft_log_entry {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| store.entry_at(LogIndex(index))) {
        Ok(Some(record)) => match record.into_ffi_entry() {
            Ok(entry) => entry.into_raw(),
            Err(err) => {
                adapter.record_error(err);
                ptr::null_mut()
            }
        },
        Ok(None) => ptr::null_mut(),
        Err(err) => {
            adapter.record_error(err);
            ptr::null_mut()
        }
    }
}

unsafe extern "C" fn log_store_term_at(user_data: *mut c_void, index: u64) -> u64 {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| store.term_at(LogIndex(index))) {
        Ok(Some(term)) => term.0,
        Ok(None) => 0,
        Err(err) => {
            adapter.record_error(err);
            0
        }
    }
}

unsafe extern "C" fn log_store_pack(
    user_data: *mut c_void,
    index: u64,
    cnt: i32,
) -> *mut bop_raft_buffer {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| store.pack(LogIndex(index), cnt)) {
        Ok(buffer) => buffer.into_raw(),
        Err(err) => {
            adapter.record_error(err);
            ptr::null_mut()
        }
    }
}

unsafe extern "C" fn log_store_apply_pack(
    user_data: *mut c_void,
    index: u64,
    pack: *mut bop_raft_buffer,
) {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    if pack.is_null() {
        adapter.record_error(RaftError::NullPointer);
        return;
    }

    let result = unsafe { Buffer::from_raw(pack) }.and_then(|buffer| {
        let data = buffer.to_vec();
        adapter.with_store(|store| store.apply_pack(LogIndex(index), &data))
    });

    if let Err(err) = result {
        adapter.record_error(err);
    }
}

unsafe extern "C" fn log_store_compact(user_data: *mut c_void, last_log_index: u64) -> bool {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| store.compact(LogIndex(last_log_index))) {
        Ok(result) => result,
        Err(err) => {
            adapter.record_error(err);
            false
        }
    }
}

unsafe extern "C" fn log_store_compact_async(user_data: *mut c_void, last_log_index: u64) -> bool {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| store.compact_async(LogIndex(last_log_index))) {
        Ok(result) => result,
        Err(err) => {
            adapter.record_error(err);
            false
        }
    }
}

unsafe extern "C" fn log_store_flush(user_data: *mut c_void) -> bool {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| store.flush()) {
        Ok(result) => result,
        Err(err) => {
            adapter.record_error(err);
            false
        }
    }
}

unsafe extern "C" fn log_store_last_durable_index(user_data: *mut c_void) -> u64 {
    let adapter = unsafe { &*(user_data as *mut LogStoreAdapter) };
    match adapter.with_store(|store| Ok(store.last_durable_index().0)) {
        Ok(idx) => idx,
        Err(err) => {
            adapter.record_error(err);
            0
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;
    use crate::log_entry::LogEntryRecord;
    use crate::traits::LogStoreInterface;
    use crate::types::{LogIndex, Term};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MockLogStore {
        appended: Arc<Mutex<Vec<LogEntryRecord>>>,
        writes: Arc<Mutex<Vec<(LogIndex, LogEntryRecord)>>>,
    }

    impl LogStoreInterface for MockLogStore {
        fn start_index(&self) -> LogIndex {
            LogIndex(0)
        }

        fn next_slot(&self) -> LogIndex {
            let entries = self.appended.lock().unwrap();
            LogIndex(entries.len() as u64 + 1)
        }

        fn last_entry(&self) -> RaftResult<Option<LogEntryRecord>> {
            Ok(self.appended.lock().unwrap().last().cloned())
        }

        fn append(&mut self, entry: LogEntryView<'_>) -> RaftResult<LogIndex> {
            let mut entries = self.appended.lock().unwrap();
            entries.push(LogEntryRecord::from_view(entry));
            Ok(LogIndex(entries.len() as u64))
        }

        fn write_at(&mut self, idx: LogIndex, entry: LogEntryView<'_>) -> RaftResult<()> {
            let mut writes = self.writes.lock().unwrap();
            writes.push((idx, LogEntryRecord::from_view(entry)));
            Ok(())
        }

        fn end_of_append_batch(&mut self, _start: LogIndex, _count: u64) -> RaftResult<()> {
            Ok(())
        }

        fn log_entries(&self, _start: LogIndex, _end: LogIndex) -> RaftResult<Vec<LogEntryRecord>> {
            Ok(self.appended.lock().unwrap().clone())
        }

        fn entry_at(&self, idx: LogIndex) -> RaftResult<Option<LogEntryRecord>> {
            let entries = self.appended.lock().unwrap();
            Ok(entries.get(idx.0 as usize).cloned())
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

    #[test]
    fn append_callback_forwards_payload() {
        let mock = MockLogStore::default();
        let handle = LogStoreHandle::new(Box::new(mock.clone())).expect("handle");
        let adapter = handle.adapter_ptr();

        let mut payload = vec![1_u8, 2, 3];
        let index = unsafe {
            super::log_store_append(
                adapter,
                bop_raft_log_entry_ptr { data: [0; 2] },
                7,
                payload.as_mut_ptr(),
                payload.len(),
                42,
                true,
                99,
            )
        };

        assert_eq!(index, 1);
        let entries = mock.appended.lock().unwrap();
        assert_eq!(entries.len(), 1);
        let record = &entries[0];
        assert_eq!(record.term, Term(7));
        assert_eq!(record.timestamp, 42);
        assert_eq!(record.payload, vec![1, 2, 3]);
        assert_eq!(record.crc32, Some(99));
    }

    #[test]
    fn write_at_callback_records_update() {
        let mock = MockLogStore::default();
        let handle = LogStoreHandle::new(Box::new(mock.clone())).expect("handle");
        let adapter = handle.adapter_ptr();

        let mut payload = vec![5_u8, 6, 7];
        unsafe {
            super::log_store_write_at(
                adapter,
                4,
                3,
                payload.as_mut_ptr(),
                payload.len(),
                11,
                false,
                0,
            );
        }

        let writes = mock.writes.lock().unwrap();
        assert_eq!(writes.len(), 1);
        let (idx, record) = &writes[0];
        assert_eq!(*idx, LogIndex(4));
        assert_eq!(record.term, Term(3));
        assert_eq!(record.payload, vec![5, 6, 7]);
        assert_eq!(record.crc32, None);
    }
}
