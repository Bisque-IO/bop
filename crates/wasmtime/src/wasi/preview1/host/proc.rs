//! Process-related host functions using Maniac.

use super::WasiView;
use crate::wasi::types::Errno;
use crate::generator::{is_generator, yield_now};
use wasmtime::Caller;

pub fn proc_exit<T: WasiView>(mut caller: Caller<'_, T>, code: i32) -> () {
    caller.data_mut().wasi_ctx_mut().exit_code = Some(code);
}

pub fn proc_raise<T: WasiView>(_caller: Caller<'_, T>, _sig: i32) -> i32 {
    Errno::Success.raw() as i32
}

pub fn sched_yield<T: WasiView>(_caller: Caller<'_, T>) -> i32 {
    // If we're in a coroutine, yield to the scheduler
    // Otherwise, yield the OS thread
    if is_generator() {
        yield_now();
    } else {
        std::thread::yield_now();
    }
    Errno::Success.raw() as i32
}
