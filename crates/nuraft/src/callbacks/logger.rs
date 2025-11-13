use std::ffi::{CStr, c_char, c_void};
use std::ptr::NonNull;
use std::sync::Arc;

use bop_sys::{bop_raft_logger_delete, bop_raft_logger_make, bop_raft_logger_ptr};

use crate::error::{RaftError, RaftResult};
use crate::traits::Logger;
use crate::types::LogLevel;

pub(crate) struct LoggerHandle {
    ptr: NonNull<bop_raft_logger_ptr>,
    adapter: *mut LoggerAdapter,
}

impl LoggerHandle {
    pub(crate) fn new(logger: Arc<dyn Logger>) -> RaftResult<Self> {
        let adapter = Box::new(LoggerAdapter { logger });
        let adapter_ptr = Box::into_raw(adapter);
        let ptr =
            unsafe { bop_raft_logger_make(adapter_ptr as *mut c_void, Some(logger_put_details)) };
        let ptr = match NonNull::new(ptr) {
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

    pub(crate) fn as_ptr(&self) -> *mut bop_raft_logger_ptr {
        self.ptr.as_ptr()
    }

    #[cfg(test)]
    pub(super) fn adapter_ptr(&self) -> *mut c_void {
        self.adapter as *mut c_void
    }
}

impl Drop for LoggerHandle {
    fn drop(&mut self) {
        unsafe {
            bop_raft_logger_delete(self.ptr.as_ptr());
            drop(Box::from_raw(self.adapter));
        }
    }
}

unsafe impl Send for LoggerHandle {}
unsafe impl Sync for LoggerHandle {}

struct LoggerAdapter {
    logger: Arc<dyn Logger>,
}

unsafe extern "C" fn logger_put_details(
    user_data: *mut c_void,
    level: i32,
    source_file: *const c_char,
    func_name: *const c_char,
    line_number: usize,
    log_line: *const c_char,
    log_line_size: usize,
) {
    let adapter = unsafe { &*(user_data as *mut LoggerAdapter) };

    let source = c_string_to_owned(source_file);
    let func = c_string_to_owned(func_name);
    let message = if log_line.is_null() || log_line_size == 0 {
        String::new()
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(log_line as *const u8, log_line_size) };
        String::from_utf8_lossy(bytes).into_owned()
    };

    adapter.logger.log(
        LogLevel::from_raw(level),
        &source,
        &func,
        line_number,
        &message,
    );
}

fn c_string_to_owned(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }

    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LogLevel;
    use std::ffi::CString;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct TestLogger {
        records: Mutex<Vec<(LogLevel, String, String, usize, String)>>,
    }

    impl Logger for TestLogger {
        fn log(&self, level: LogLevel, source: &str, func: &str, line: usize, message: &str) {
            self.records.lock().unwrap().push((
                level,
                source.to_owned(),
                func.to_owned(),
                line,
                message.to_owned(),
            ));
        }
    }

    #[test]
    fn logger_adapter_forwards_details() {
        let logger = Arc::new(TestLogger::default());
        let handle = LoggerHandle::new(logger.clone()).expect("logger handle");
        let adapter = handle.adapter_ptr();

        let file = CString::new("src/lib.rs").unwrap();
        let func = CString::new("do_work").unwrap();
        let message = CString::new("hello world").unwrap();

        unsafe {
            super::logger_put_details(
                adapter,
                2,
                file.as_ptr(),
                func.as_ptr(),
                41,
                message.as_ptr(),
                message.as_bytes().len(),
            );
        }

        let records = logger.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        let (level, src, fun, line, msg) = &records[0];
        assert!(matches!(level, LogLevel::Info));
        assert_eq!(src, "src/lib.rs");
        assert_eq!(fun, "do_work");
        assert_eq!(*line, 41);
        assert_eq!(msg, "hello world");
    }
}
