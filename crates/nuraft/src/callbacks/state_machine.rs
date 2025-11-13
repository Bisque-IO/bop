use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::Mutex;

use bop_sys::{
    bop_raft_buffer, bop_raft_buffer_data, bop_raft_buffer_size, bop_raft_cluster_config,
    bop_raft_fsm_adjust_commit_index_params, bop_raft_fsm_adjust_commit_index_peer_indexes,
    bop_raft_fsm_delete, bop_raft_fsm_make, bop_raft_fsm_ptr, bop_raft_snapshot,
    bop_raft_snapshot_serialize,
};

use crate::buffer::Buffer;
use crate::config::{ClusterConfig, ClusterConfigView};
use crate::error::{RaftError, RaftResult};
use crate::traits::{
    AdjustCommitIndex, SnapshotChunk, SnapshotCreation, SnapshotMetadata, SnapshotReadResult,
    SnapshotReader, SnapshotRef, SnapshotType, StateMachine,
};
use crate::types::{LogIndex, ServerId, Term};

#[allow(dead_code)]
pub(crate) struct StateMachineHandle {
    ptr: NonNull<bop_raft_fsm_ptr>,
    adapter: *mut StateMachineAdapter,
}

#[allow(dead_code)]
impl StateMachineHandle {
    pub(crate) fn new(state_machine: Box<dyn StateMachine>) -> RaftResult<Self> {
        let adapter = Box::new(StateMachineAdapter::new(state_machine)?);
        let current_conf_ptr = adapter.current_conf.as_mut_ptr();
        let rollback_conf_ptr = adapter.rollback_conf.as_mut_ptr();
        let adapter_ptr = Box::into_raw(adapter);

        let fsm_ptr = unsafe {
            bop_raft_fsm_make(
                adapter_ptr as *mut c_void,
                current_conf_ptr,
                rollback_conf_ptr,
                Some(state_machine_commit),              // commit
                Some(state_machine_commit_config),       // commit_config
                Some(state_machine_pre_commit),          // pre_commit
                Some(state_machine_rollback),            // rollback
                Some(state_machine_rollback_config),     // rollback_config
                Some(state_machine_next_batch_hint),     // get_next_batch_size_hint_in_bytes
                Some(state_machine_save_snapshot),       // save_snapshot
                Some(state_machine_apply_snapshot),      // apply_snapshot
                Some(state_machine_read_snapshot),       // read_snapshot
                Some(state_machine_free_snapshot_ctx),   // free_snapshot_user_ctx
                Some(state_machine_last_snapshot),       // last_snapshot
                Some(state_machine_last_commit_index),   // last_commit_index
                Some(state_machine_create_snapshot),     // create_snapshot
                Some(state_machine_chk_create_snapshot), // chk_create_snapshot
                Some(state_machine_allow_transfer),      // allow_leadership_transfer
                Some(state_machine_adjust_commit_index), // adjust_commit_index
            )
        };

        let ptr = match NonNull::new(fsm_ptr) {
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

    pub(crate) fn as_ptr(&self) -> *mut bop_raft_fsm_ptr {
        self.ptr.as_ptr()
    }

    pub(crate) fn take_last_error(&self) -> Option<RaftError> {
        unsafe { (&*self.adapter).take_last_error() }
    }
}

impl Drop for StateMachineHandle {
    fn drop(&mut self) {
        unsafe {
            bop_raft_fsm_delete(self.ptr.as_ptr());
            drop(Box::from_raw(self.adapter));
        }
    }
}

unsafe impl Send for StateMachineHandle {}
unsafe impl Sync for StateMachineHandle {}

#[allow(dead_code)]
struct StateMachineAdapter {
    state_machine: Mutex<Box<dyn StateMachine>>,
    current_conf: ClusterConfig,
    rollback_conf: ClusterConfig,
    last_error: Mutex<Option<RaftError>>,
}

struct SnapshotReaderHolder {
    reader: Option<Box<dyn SnapshotReader>>,
}

const MAX_ADJUST_PEERS: usize = 16;

fn snapshot_metadata_from_parts(
    log_idx: u64,
    log_term: u64,
    config_ptr: *mut bop_raft_cluster_config,
    size: u64,
    ty: u8,
) -> SnapshotMetadata<'static> {
    let cluster_config =
        unsafe { ClusterConfigView::new(config_ptr as *const bop_raft_cluster_config) };
    SnapshotMetadata {
        log_idx: LogIndex(log_idx),
        log_term: Term(log_term),
        cluster_config,
        snapshot_size: size,
        snapshot_type: SnapshotType(ty),
    }
}

fn parse_snapshot_bytes(bytes: &[u8]) -> Option<SnapshotMetadata<'static>> {
    if bytes.len() < 1 + 8 * 3 {
        return None;
    }
    let snapshot_type = bytes[0];
    let mut offset = 1usize;
    let read_u64 = |buf: &[u8], offset: &mut usize| -> Option<u64> {
        if *offset + 8 > buf.len() {
            return None;
        }
        let mut data = [0u8; 8];
        data.copy_from_slice(&buf[*offset..*offset + 8]);
        *offset += 8;
        Some(u64::from_le_bytes(data))
    };
    let last_log_idx = read_u64(bytes, &mut offset)?;
    let last_log_term = read_u64(bytes, &mut offset)?;
    let snapshot_size = read_u64(bytes, &mut offset)?;

    Some(SnapshotMetadata {
        log_idx: LogIndex(last_log_idx),
        log_term: Term(last_log_term),
        cluster_config: None,
        snapshot_size,
        snapshot_type: SnapshotType(snapshot_type),
    })
}

fn snapshot_metadata_from_buffer(
    snapshot_buffer: *mut bop_raft_buffer,
) -> Option<SnapshotMetadata<'static>> {
    if snapshot_buffer.is_null() {
        return None;
    }
    unsafe {
        let size = bop_raft_buffer_size(snapshot_buffer);
        let data_ptr = bop_raft_buffer_data(snapshot_buffer);
        if data_ptr.is_null() {
            return None;
        }
        let bytes = slice::from_raw_parts(data_ptr, size);
        parse_snapshot_bytes(bytes)
    }
}

fn snapshot_metadata_from_snapshot_ref(snapshot: SnapshotRef) -> Option<SnapshotMetadata<'static>> {
    unsafe {
        let raw = bop_raft_snapshot_serialize(snapshot.as_ptr());
        let buffer = Buffer::from_raw(raw).ok()?;
        let bytes = buffer.to_vec();
        parse_snapshot_bytes(&bytes)
    }
}

#[allow(dead_code)]
impl StateMachineAdapter {
    fn new(state_machine: Box<dyn StateMachine>) -> RaftResult<Self> {
        Ok(Self {
            state_machine: Mutex::new(state_machine),
            current_conf: ClusterConfig::new()?,
            rollback_conf: ClusterConfig::new()?,
            last_error: Mutex::new(None),
        })
    }

    fn with_state_machine<R>(
        &self,
        f: impl FnOnce(&mut dyn StateMachine) -> RaftResult<R>,
    ) -> RaftResult<R> {
        let mut guard = self.state_machine.lock().map_err(|_| {
            RaftError::StateMachineError("State machine mutex poisoned".to_string())
        })?;
        f(guard.as_mut())
    }

    fn record_error(&self, err: RaftError) {
        if let Ok(mut guard) = self.last_error.lock() {
            *guard = Some(err);
        }
    }

    fn take_last_error(&self) -> Option<RaftError> {
        self.last_error
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_commit(
    user_data: *mut c_void,
    log_idx: u64,
    data: *const u8,
    size: usize,
    result: *mut *mut bop_raft_buffer,
) {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    let payload = if size == 0 || data.is_null() {
        &[]
    } else {
        // SAFETY: the caller guarantees `data` references `size` bytes.
        unsafe { slice::from_raw_parts(data, size) }
    };

    match adapter.with_state_machine(|sm| sm.apply(LogIndex(log_idx), payload)) {
        Ok(Some(buffer)) => {
            if !result.is_null() {
                unsafe {
                    *result = buffer.into_raw();
                }
            }
        }
        Ok(None) => {
            if !result.is_null() {
                unsafe {
                    *result = ptr::null_mut();
                }
            }
        }
        Err(err) => adapter.record_error(err),
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_commit_config(
    user_data: *mut c_void,
    log_idx: u64,
    new_conf: *mut c_void,
) {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    let cfg_ptr = new_conf as *mut bop_raft_cluster_config;
    let view = unsafe { ClusterConfigView::new(cfg_ptr as *const bop_raft_cluster_config) };
    if let Some(cfg) = view {
        if let Err(err) = adapter.with_state_machine(|sm| sm.commit_config(LogIndex(log_idx), cfg))
        {
            adapter.record_error(err);
        }
    } else {
        adapter.record_error(RaftError::NullPointer);
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_pre_commit(
    user_data: *mut c_void,
    log_idx: u64,
    data: *const u8,
    size: usize,
    result: *mut *mut bop_raft_buffer,
) {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    let payload = if size == 0 || data.is_null() {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, size) }
    };

    match adapter.with_state_machine(|sm| sm.pre_commit(LogIndex(log_idx), payload)) {
        Ok(Some(buffer)) => {
            if !result.is_null() {
                unsafe {
                    *result = buffer.into_raw();
                }
            }
        }
        Ok(None) => {
            if !result.is_null() {
                unsafe {
                    *result = ptr::null_mut();
                }
            }
        }
        Err(err) => adapter.record_error(err),
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_rollback(
    user_data: *mut c_void,
    log_idx: u64,
    data: *const u8,
    size: usize,
) {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    let payload = if size == 0 || data.is_null() {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, size) }
    };
    if let Err(err) = adapter.with_state_machine(|sm| sm.rollback(LogIndex(log_idx), payload)) {
        adapter.record_error(err);
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_rollback_config(
    user_data: *mut c_void,
    log_idx: u64,
    conf: *mut c_void,
) {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    let cfg_ptr = conf as *mut bop_raft_cluster_config;
    let view = unsafe { ClusterConfigView::new(cfg_ptr as *const bop_raft_cluster_config) };
    if let Some(cfg) = view {
        if let Err(err) =
            adapter.with_state_machine(|sm| sm.rollback_config(LogIndex(log_idx), cfg))
        {
            adapter.record_error(err);
        }
    } else {
        adapter.record_error(RaftError::NullPointer);
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_next_batch_hint(user_data: *mut c_void) -> i64 {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    match adapter.with_state_machine(|sm| Ok(sm.next_batch_size_hint())) {
        Ok(value) => value,
        Err(err) => {
            adapter.record_error(err);
            0
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_last_commit_index(user_data: *mut c_void) -> u64 {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    match adapter.with_state_machine(|sm| Ok(sm.last_applied_index().0)) {
        Ok(idx) => idx,
        Err(err) => {
            adapter.record_error(err);
            0
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_save_snapshot(
    user_data: *mut c_void,
    last_log_idx: u64,
    last_log_term: u64,
    last_config: *mut bop_raft_cluster_config,
    size: u64,
    snapshot_type: u8,
    is_first_obj: bool,
    is_last_obj: bool,
    data: *const u8,
    data_size: usize,
) {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    let payload = if data.is_null() || data_size == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, data_size) }
    };
    let metadata = snapshot_metadata_from_parts(
        last_log_idx,
        last_log_term,
        last_config,
        size,
        snapshot_type,
    );
    let chunk = SnapshotChunk {
        metadata,
        is_first_chunk: is_first_obj,
        is_last_chunk: is_last_obj,
        data: payload,
    };
    if let Err(err) = adapter.with_state_machine(|sm| sm.save_snapshot_chunk(chunk)) {
        adapter.record_error(err);
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_apply_snapshot(
    user_data: *mut c_void,
    last_log_idx: u64,
    last_log_term: u64,
    last_config: *mut bop_raft_cluster_config,
    size: u64,
    snapshot_type: u8,
) -> bool {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    let metadata = snapshot_metadata_from_parts(
        last_log_idx,
        last_log_term,
        last_config,
        size,
        snapshot_type,
    );
    match adapter.with_state_machine(|sm| sm.apply_snapshot(metadata)) {
        Ok(result) => result,
        Err(err) => {
            adapter.record_error(err);
            false
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_read_snapshot(
    user_data: *mut c_void,
    user_snapshot_ctx: *mut *mut c_void,
    obj_id: u64,
    data_out: *mut *mut bop_raft_buffer,
    is_last_obj: *mut bool,
) -> i32 {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    if !data_out.is_null() {
        unsafe {
            *data_out = ptr::null_mut();
        }
    }
    if !is_last_obj.is_null() {
        unsafe {
            *is_last_obj = false;
        }
    }
    if user_snapshot_ctx.is_null() {
        return -1;
    }

    let mut reader_ptr = unsafe { *user_snapshot_ctx } as *mut SnapshotReaderHolder;
    if reader_ptr.is_null() {
        match adapter.with_state_machine(|sm| sm.snapshot_reader()) {
            Ok(reader) => {
                let holder = Box::new(SnapshotReaderHolder {
                    reader: Some(reader),
                });
                reader_ptr = Box::into_raw(holder);
                unsafe {
                    *user_snapshot_ctx = reader_ptr as *mut c_void;
                }
            }
            Err(err) => {
                adapter.record_error(err);
                return -1;
            }
        }
    }

    let holder = unsafe { &mut *reader_ptr }; // SAFETY: pointer initialized above
    let reader = match holder.reader.as_mut() {
        Some(reader) => reader,
        None => {
            adapter.record_error(RaftError::StateMachineError(
                "snapshot reader already finalized".to_string(),
            ));
            return -1;
        }
    };
    match reader.next(obj_id) {
        Ok(SnapshotReadResult::Chunk { data, is_last }) => match Buffer::from_vec(data) {
            Ok(buffer) => {
                if !data_out.is_null() {
                    unsafe {
                        *data_out = buffer.into_raw();
                    }
                }
                if !is_last_obj.is_null() {
                    unsafe {
                        *is_last_obj = is_last;
                    }
                }
                0
            }
            Err(err) => {
                adapter.record_error(err);
                -1
            }
        },
        Ok(SnapshotReadResult::End) => {
            if !is_last_obj.is_null() {
                unsafe {
                    *is_last_obj = true;
                }
            }
            0
        }
        Err(err) => {
            adapter.record_error(err);
            -1
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_free_snapshot_ctx(
    user_data: *mut c_void,
    user_snapshot_ctx: *mut *mut c_void,
) {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    if user_snapshot_ctx.is_null() {
        return;
    }
    let holder_ptr = unsafe { *user_snapshot_ctx } as *mut SnapshotReaderHolder;
    unsafe {
        *user_snapshot_ctx = ptr::null_mut();
    }
    if holder_ptr.is_null() {
        return;
    }
    let mut holder = unsafe { Box::from_raw(holder_ptr) };
    if let Some(reader) = holder.reader.take() {
        if let Err(err) = adapter.with_state_machine(|sm| sm.finalize_snapshot_reader(reader)) {
            adapter.record_error(err);
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_last_snapshot(user_data: *mut c_void) -> *mut bop_raft_snapshot {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    match adapter.with_state_machine(|sm| sm.last_snapshot()) {
        Ok(Some(snapshot)) => snapshot.as_ptr(),
        Ok(None) => ptr::null_mut(),
        Err(err) => {
            adapter.record_error(err);
            ptr::null_mut()
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_create_snapshot(
    user_data: *mut c_void,
    snapshot: *mut bop_raft_snapshot,
    snapshot_buffer: *mut bop_raft_buffer,
    data_ptr: *mut c_void,
    data_size: usize,
) {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    let snapshot_ref = unsafe { SnapshotRef::new(snapshot) };
    let metadata = snapshot_metadata_from_buffer(snapshot_buffer)
        .or_else(|| snapshot_ref.and_then(snapshot_metadata_from_snapshot_ref))
        .unwrap_or_else(|| {
            adapter.record_error(RaftError::StateMachineError(
                "snapshot metadata unavailable".to_string(),
            ));
            SnapshotMetadata {
                log_idx: LogIndex(0),
                log_term: Term(0),
                cluster_config: None,
                snapshot_size: 0,
                snapshot_type: SnapshotType(0),
            }
        });
    let data = if data_ptr.is_null() || data_size == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data_ptr as *const u8, data_size) }
    };
    let creation = SnapshotCreation {
        metadata,
        data,
        snapshot: snapshot_ref,
    };
    if let Err(err) = adapter.with_state_machine(|sm| sm.create_snapshot(creation)) {
        adapter.record_error(err);
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_chk_create_snapshot(user_data: *mut c_void) -> bool {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    match adapter.with_state_machine(|sm| Ok(sm.should_create_snapshot())) {
        Ok(flag) => flag,
        Err(err) => {
            adapter.record_error(err);
            false
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_adjust_commit_index(
    user_data: *mut c_void,
    current_commit_index: u64,
    expected_commit_index: u64,
    params: *const bop_raft_fsm_adjust_commit_index_params,
) -> u64 {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    let params = params as *mut bop_raft_fsm_adjust_commit_index_params;
    if params.is_null() {
        adapter.record_error(RaftError::NullPointer);
        return expected_commit_index;
    }
    let mut peer_indexes = HashMap::new();
    let mut values = [u64::MAX; MAX_ADJUST_PEERS];
    let mut slots = [ptr::null_mut(); MAX_ADJUST_PEERS];
    for (idx, slot) in slots.iter_mut().enumerate() {
        *slot = &mut values[idx] as *mut u64;
    }
    unsafe {
        bop_raft_fsm_adjust_commit_index_peer_indexes(params, slots.as_mut_ptr());
    }
    for (peer_id, value) in values.iter().enumerate() {
        if *value != u64::MAX {
            peer_indexes.insert(ServerId(peer_id as i32), LogIndex(*value));
        }
    }
    let info = AdjustCommitIndex::new(
        LogIndex(current_commit_index),
        LogIndex(expected_commit_index),
        peer_indexes,
    );
    match adapter.with_state_machine(move |sm| sm.adjust_commit_index(info)) {
        Ok(idx) => idx.0,
        Err(err) => {
            adapter.record_error(err);
            expected_commit_index
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_machine_allow_transfer(user_data: *mut c_void) -> bool {
    let adapter = unsafe { &*(user_data as *mut StateMachineAdapter) };
    match adapter.with_state_machine(|sm| Ok(sm.allow_leadership_transfer())) {
        Ok(flag) => flag,
        Err(err) => {
            adapter.record_error(err);
            false
        }
    }
}
