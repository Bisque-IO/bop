use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::Mutex;

use bop_sys::{bop_raft_buffer, bop_raft_fsm_delete, bop_raft_fsm_make, bop_raft_fsm_ptr};

use crate::buffer::Buffer;
use crate::config::ClusterConfig;
use crate::error::{RaftError, RaftResult};
use crate::traits::StateMachine;
use crate::types::LogIndex;

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
                Some(state_machine_commit),            // commit
                None,                                  // commit_config
                None,                                  // pre_commit
                None,                                  // rollback
                None,                                  // rollback_config
                None,                                  // get_next_batch_size_hint_in_bytes
                None,                                  // save_snapshot
                None,                                  // apply_snapshot
                None,                                  // read_snapshot
                None,                                  // free_snapshot_user_ctx
                None,                                  // last_snapshot
                Some(state_machine_last_commit_index), // last_commit_index
                None,                                  // create_snapshot
                None,                                  // chk_create_snapshot
                Some(state_machine_allow_transfer),    // allow_leadership_transfer
                None,                                  // adjust_commit_index
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
        Ok(Some(bytes)) => match Buffer::from_vec(bytes) {
            Ok(buffer) => {
                if !result.is_null() {
                    unsafe {
                        *result = buffer.into_raw();
                    }
                }
            }
            Err(err) => adapter.record_error(err),
        },
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
