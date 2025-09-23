use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use bop_sys::{
    bop_raft_cb_func, bop_raft_cb_param, bop_raft_cb_return_code_bop_raft_cb_return_code_ok,
    bop_raft_cb_return_code_bop_raft_cb_return_code_return_null,
};

use crate::error::{RaftError, RaftResult};
use crate::traits::ServerCallbacks;
use crate::types::{CallbackAction, CallbackContext, CallbackType};

pub(crate) struct ServerCallbacksHandle {
    adapter: NonNull<ServerCallbacksAdapter>,
}

impl ServerCallbacksHandle {
    pub(crate) fn new(callbacks: Arc<dyn ServerCallbacks>) -> Self {
        let adapter = Box::new(ServerCallbacksAdapter::new(callbacks));
        let ptr = NonNull::from(Box::leak(adapter));
        Self { adapter: ptr }
    }

    pub(crate) fn user_data(&self) -> *mut c_void {
        self.adapter.as_ptr() as *mut c_void
    }

    pub(crate) fn func(&self) -> bop_raft_cb_func {
        Some(dispatch_callback)
    }

    pub(crate) fn take_last_error(&self) -> Option<RaftError> {
        unsafe { self.adapter.as_ref().take_last_error() }
    }
}

impl Drop for ServerCallbacksHandle {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(self.adapter.as_ptr()));
        }
    }
}

unsafe impl Send for ServerCallbacksHandle {}
unsafe impl Sync for ServerCallbacksHandle {}

struct ServerCallbacksAdapter {
    callbacks: Arc<dyn ServerCallbacks>,
    last_error: Mutex<Option<RaftError>>,
}

impl ServerCallbacksAdapter {
    fn new(callbacks: Arc<dyn ServerCallbacks>) -> Self {
        Self {
            callbacks,
            last_error: Mutex::new(None),
        }
    }

    fn handle_event(
        &self,
        ty: CallbackType,
        param: *mut bop_raft_cb_param,
    ) -> RaftResult<CallbackAction> {
        let context = unsafe { CallbackContext::from_raw(ty, param) };
        self.callbacks.handle_event(context)
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

unsafe extern "C" fn dispatch_callback(
    user_data: *mut c_void,
    type_id: i32,
    param: *mut bop_raft_cb_param,
) -> i32 {
    let adapter = unsafe { &*(user_data as *mut ServerCallbacksAdapter) };
    let callback_type = CallbackType::from_raw(type_id);

    match adapter.handle_event(callback_type, param) {
        Ok(action) => match action {
            CallbackAction::Continue => bop_raft_cb_return_code_bop_raft_cb_return_code_ok,
            CallbackAction::ReturnNull => {
                bop_raft_cb_return_code_bop_raft_cb_return_code_return_null
            }
        },
        Err(err) => {
            adapter.record_error(err);
            bop_raft_cb_return_code_bop_raft_cb_return_code_return_null
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CallbackType;
    use std::ptr;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct TestCallbacks {
        events: Mutex<Vec<CallbackType>>,
    }

    impl ServerCallbacks for TestCallbacks {
        fn handle_event(&self, context: CallbackContext<'_>) -> RaftResult<CallbackAction> {
            self.events.lock().unwrap().push(context.callback_type());
            Ok(CallbackAction::Continue)
        }
    }

    #[derive(Default)]
    struct FailingCallbacks;

    impl ServerCallbacks for FailingCallbacks {
        fn handle_event(&self, _context: CallbackContext<'_>) -> RaftResult<CallbackAction> {
            Err(RaftError::ServerError)
        }
    }

    #[test]
    fn dispatch_callback_invokes_handler() {
        let callbacks = Arc::new(TestCallbacks::default());
        let handle = ServerCallbacksHandle::new(callbacks.clone());

        let rc = unsafe {
            super::dispatch_callback(
                handle.user_data(),
                CallbackType::BecomeLeader.as_raw(),
                ptr::null_mut(),
            )
        };

        assert_eq!(rc, bop_raft_cb_return_code_bop_raft_cb_return_code_ok);
        let events = callbacks.events.lock().unwrap();
        assert_eq!(events.as_slice(), &[CallbackType::BecomeLeader]);
    }

    #[test]
    fn dispatch_callback_propagates_error() {
        let handle = ServerCallbacksHandle::new(Arc::new(FailingCallbacks));

        let rc = unsafe {
            super::dispatch_callback(
                handle.user_data(),
                CallbackType::ProcessRequest.as_raw(),
                ptr::null_mut(),
            )
        };

        assert_eq!(
            rc,
            bop_raft_cb_return_code_bop_raft_cb_return_code_return_null
        );
    }
}
