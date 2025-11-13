use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::Mutex;

use maniac_sys::{
    bop_raft_cluster_config, bop_raft_log_store_ptr, bop_raft_srv_state, bop_raft_state_mgr_delete,
    bop_raft_state_mgr_make, bop_raft_state_mgr_ptr,
};

use crate::config::ClusterConfigView;
use crate::error::{RaftError, RaftResult};
use crate::state::ServerStateView;
use crate::storage::{LogStoreBuild, RawStateManager, StateManagerBuild, StorageBackendKind};
use crate::traits::StateManagerInterface;

use super::log_store::LogStoreHandle;

pub(crate) struct StateManagerHandle {
    ptr: NonNull<bop_raft_state_mgr_ptr>,
    backend: StorageBackendKind,
    kind: StateManagerHandleKind,
}

enum StateManagerHandleKind {
    Callback {
        adapter: *mut StateManagerAdapter,
    },
    Raw {
        _guard: RawStateManager,
        _log_store: Option<LogStoreHandle>,
    },
}

impl StateManagerHandle {
    pub(crate) fn new(
        state_manager: StateManagerBuild,
        log_store: Option<LogStoreHandle>,
        mut _logger_ptr: Option<*mut maniac_sys::bop_raft_logger_ptr>,
    ) -> RaftResult<Self> {
        match state_manager {
            StateManagerBuild::Callbacks { backend, manager } => {
                if backend != StorageBackendKind::Callbacks {
                    return Err(RaftError::ConfigError(format!(
                        "Unsupported state manager backend `{}` for callback adapter. Use `RaftServerBuilder::try_mdbx_storage` or provide a raw backend handle.",
                        backend
                    )));
                }
                let adapter = Box::new(StateManagerAdapter::new(manager, log_store));
                let adapter_ptr = Box::into_raw(adapter);

                let state_mgr_ptr = unsafe {
                    bop_raft_state_mgr_make(
                        adapter_ptr as *mut c_void,
                        Some(state_manager_load_config),
                        Some(state_manager_save_config),
                        Some(state_manager_read_state),
                        Some(state_manager_save_state),
                        Some(state_manager_load_log_store),
                        Some(state_manager_server_id),
                        Some(state_manager_system_exit),
                    )
                };

                let ptr = match NonNull::new(state_mgr_ptr) {
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
                    backend: StorageBackendKind::Callbacks,
                    kind: StateManagerHandleKind::Callback {
                        adapter: adapter_ptr,
                    },
                })
            }
            StateManagerBuild::Raw(guard) => {
                let ptr = NonNull::new(guard.as_ptr()).ok_or(RaftError::NullPointer)?;
                Ok(Self {
                    ptr,
                    backend: guard.backend(),
                    kind: StateManagerHandleKind::Raw {
                        _guard: guard,
                        _log_store: log_store,
                    },
                })
            }
            #[cfg(feature = "mdbx")]
            StateManagerBuild::Mdbx(builder) => {
                let logger_ptr = _logger_ptr.ok_or_else(|| {
                    RaftError::ConfigError(
                        "MDBX state manager requires a logger to be configured".to_string(),
                    )
                })?;
                let mut log_store_handle = log_store.ok_or_else(|| {
                    RaftError::ConfigError("MDBX state manager requires a log store".to_string())
                })?;
                let log_store_ptr = log_store_handle.as_ptr();
                let raw = builder.into_raw(logger_ptr, log_store_ptr)?;
                let backend = raw.backend();
                let ptr = NonNull::new(raw.as_ptr()).ok_or(RaftError::NullPointer)?;
                Ok(Self {
                    ptr,
                    backend,
                    kind: StateManagerHandleKind::Raw {
                        _guard: raw,
                        _log_store: Some(log_store_handle),
                    },
                })
            }
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut bop_raft_state_mgr_ptr {
        self.ptr.as_ptr()
    }

    pub(crate) fn backend(&self) -> StorageBackendKind {
        self.backend
    }

    pub(crate) fn take_last_error(&self) -> Option<RaftError> {
        match &self.kind {
            StateManagerHandleKind::Callback { adapter } => unsafe {
                let adapter_ptr = *adapter;
                let adapter_ref = &*adapter_ptr;
                adapter_ref.take_last_error()
            },
            StateManagerHandleKind::Raw { .. } => None,
        }
    }
}

impl Drop for StateManagerHandle {
    fn drop(&mut self) {
        match &self.kind {
            StateManagerHandleKind::Callback { adapter } => unsafe {
                bop_raft_state_mgr_delete(self.ptr.as_ptr());
                drop(Box::from_raw(*adapter));
            },
            StateManagerHandleKind::Raw { .. } => {}
        }
    }
}

unsafe impl Send for StateManagerHandle {}
unsafe impl Sync for StateManagerHandle {}

#[allow(dead_code)]
struct StateManagerAdapter {
    state_manager: Mutex<Box<dyn StateManagerInterface>>,
    log_store: Mutex<Option<LogStoreHandle>>,
    last_error: Mutex<Option<RaftError>>,
}

#[allow(dead_code)]
impl StateManagerAdapter {
    fn new(
        state_manager: Box<dyn StateManagerInterface>,
        log_store: Option<LogStoreHandle>,
    ) -> Self {
        Self {
            state_manager: Mutex::new(state_manager),
            log_store: Mutex::new(log_store),
            last_error: Mutex::new(None),
        }
    }

    fn with_state_manager<R>(
        &self,
        f: impl FnOnce(&mut dyn StateManagerInterface) -> RaftResult<R>,
    ) -> RaftResult<R> {
        let mut guard = self.state_manager.lock().map_err(|_| {
            RaftError::StateMachineError("State manager mutex poisoned".to_string())
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

    fn log_store_ptr(&self) -> RaftResult<*mut bop_raft_log_store_ptr> {
        if let Ok(mut guard) = self.log_store.lock() {
            if let Some(handle) = guard.as_ref() {
                return Ok(handle.as_ptr());
            }

            if let Some(ls_impl) = self.with_state_manager(|sm| sm.load_log_store())? {
                let backend = ls_impl.storage_backend();
                let handle = LogStoreHandle::new(
                    LogStoreBuild::Callbacks {
                        backend,
                        store: ls_impl,
                    },
                    None,
                )?;
                let ptr = handle.as_ptr();
                *guard = Some(handle);
                return Ok(ptr);
            }
        } else {
            return Err(RaftError::LogStoreError("Log store mutex poisoned".into()));
        }

        Ok(ptr::null_mut())
    }
}
unsafe extern "C" fn state_manager_load_config(
    user_data: *mut c_void,
) -> *mut bop_raft_cluster_config {
    let adapter = unsafe { &*(user_data as *mut StateManagerAdapter) };
    match adapter.with_state_manager(|sm| sm.load_config()) {
        Ok(Some(config)) => config.into_raw(),
        Ok(None) => ptr::null_mut(),
        Err(err) => {
            adapter.record_error(err);
            ptr::null_mut()
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_manager_save_config(
    user_data: *mut c_void,
    config: *const bop_raft_cluster_config,
) {
    let adapter = unsafe { &*(user_data as *mut StateManagerAdapter) };
    let view = unsafe { ClusterConfigView::new(config) };
    if let Some(view) = view {
        if let Err(err) = adapter.with_state_manager(|sm| sm.save_config(view)) {
            adapter.record_error(err);
        }
    } else {
        adapter.record_error(RaftError::NullPointer);
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_manager_read_state(user_data: *mut c_void) -> *mut bop_raft_srv_state {
    let adapter = unsafe { &*(user_data as *mut StateManagerAdapter) };
    match adapter.with_state_manager(|sm| sm.load_state()) {
        Ok(Some(state)) => state.into_raw(),
        Ok(None) => ptr::null_mut(),
        Err(err) => {
            adapter.record_error(err);
            ptr::null_mut()
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_manager_save_state(
    user_data: *mut c_void,
    state: *const bop_raft_srv_state,
) {
    let adapter = unsafe { &*(user_data as *mut StateManagerAdapter) };
    let view = unsafe { ServerStateView::new(state) };
    if let Some(view) = view {
        if let Err(err) = adapter.with_state_manager(|sm| sm.save_state(view)) {
            adapter.record_error(err);
        }
    } else {
        adapter.record_error(RaftError::NullPointer);
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_manager_load_log_store(
    user_data: *mut c_void,
) -> *mut bop_raft_log_store_ptr {
    let adapter = unsafe { &*(user_data as *mut StateManagerAdapter) };
    match adapter.log_store_ptr() {
        Ok(ptr) => ptr,
        Err(err) => {
            adapter.record_error(err);
            ptr::null_mut()
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_manager_server_id(user_data: *mut c_void) -> i32 {
    let adapter = unsafe { &*(user_data as *mut StateManagerAdapter) };
    match adapter.with_state_manager(|sm| Ok(sm.server_id().inner())) {
        Ok(id) => id,
        Err(err) => {
            adapter.record_error(err);
            0
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn state_manager_system_exit(user_data: *mut c_void, code: i32) {
    let adapter = unsafe { &*(user_data as *mut StateManagerAdapter) };
    if let Err(err) = adapter.with_state_manager(|sm| {
        sm.system_exit(code);
        Ok(())
    }) {
        adapter.record_error(err);
    }
}
