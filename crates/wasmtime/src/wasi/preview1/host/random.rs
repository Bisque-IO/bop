//! Random number generation host function.

use super::WasiView;
use crate::wasi::types::Errno;
use rand::RngCore;
use wasmtime::Caller;

pub fn random_get<T: WasiView>(mut caller: Caller<'_, T>, buf_ptr: i32, buf_len: i32) -> i32 {
    let mem = match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => m,
        _ => return Errno::Inval.raw() as i32,
    };
    let mut buf = vec![0u8; buf_len as usize];
    rand::rng().fill_bytes(&mut buf);
    if mem.write(&mut caller, buf_ptr as usize, &buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}
