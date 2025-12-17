//! Args and environment host functions.

use super::WasiView;
use crate::wasi::types::Errno;
use wasmtime::Caller;

pub fn args_get<T: WasiView>(mut caller: Caller<'_, T>, argv_ptr: i32, argv_buf_ptr: i32) -> i32 {
    let ctx = caller.data().wasi_ctx();
    let args = ctx.argv.clone();
    let mem = match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => m,
        _ => return Errno::Inval.raw() as i32,
    };
    let mut argv_offset = argv_ptr as usize;
    let mut buf_offset = argv_buf_ptr as usize;
    for arg in &args {
        let arg_bytes = arg.as_bytes();
        let ptr_bytes = (buf_offset as u32).to_le_bytes();
        if mem.write(&mut caller, argv_offset, &ptr_bytes).is_err() {
            return Errno::Fault.raw() as i32;
        }
        argv_offset += 4;
        if mem.write(&mut caller, buf_offset, arg_bytes).is_err() {
            return Errno::Fault.raw() as i32;
        }
        buf_offset += arg_bytes.len();
        if mem.write(&mut caller, buf_offset, &[0u8]).is_err() {
            return Errno::Fault.raw() as i32;
        }
        buf_offset += 1;
    }
    Errno::Success.raw() as i32
}

pub fn args_sizes_get<T: WasiView>(
    mut caller: Caller<'_, T>,
    argc_ptr: i32,
    argv_buf_size_ptr: i32,
) -> i32 {
    let ctx = caller.data().wasi_ctx();
    let argc = ctx.argv.len() as u32;
    let argv_buf_size: u32 = ctx.argv.iter().map(|s| s.len() as u32 + 1).sum();
    let mem = match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => m,
        _ => return Errno::Inval.raw() as i32,
    };
    if mem
        .write(&mut caller, argc_ptr as usize, &argc.to_le_bytes())
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    if mem
        .write(
            &mut caller,
            argv_buf_size_ptr as usize,
            &argv_buf_size.to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn environ_get<T: WasiView>(
    mut caller: Caller<'_, T>,
    environ_ptr: i32,
    environ_buf_ptr: i32,
) -> i32 {
    let ctx = caller.data().wasi_ctx();
    let env = ctx.env.clone();
    let mem = match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => m,
        _ => return Errno::Inval.raw() as i32,
    };
    let mut ptr_offset = environ_ptr as usize;
    let mut buf_offset = environ_buf_ptr as usize;
    for (key, value) in &env {
        let entry = format!("{}={}", key, value);
        let entry_bytes = entry.as_bytes();
        let ptr_bytes = (buf_offset as u32).to_le_bytes();
        if mem.write(&mut caller, ptr_offset, &ptr_bytes).is_err() {
            return Errno::Fault.raw() as i32;
        }
        ptr_offset += 4;
        if mem.write(&mut caller, buf_offset, entry_bytes).is_err() {
            return Errno::Fault.raw() as i32;
        }
        buf_offset += entry_bytes.len();
        if mem.write(&mut caller, buf_offset, &[0u8]).is_err() {
            return Errno::Fault.raw() as i32;
        }
        buf_offset += 1;
    }
    Errno::Success.raw() as i32
}

pub fn environ_sizes_get<T: WasiView>(
    mut caller: Caller<'_, T>,
    environ_count_ptr: i32,
    environ_buf_size_ptr: i32,
) -> i32 {
    let ctx = caller.data().wasi_ctx();
    let count = ctx.env.len() as u32;
    let buf_size: u32 = ctx
        .env
        .iter()
        .map(|(k, v)| k.len() as u32 + 1 + v.len() as u32 + 1)
        .sum();
    let mem = match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => m,
        _ => return Errno::Inval.raw() as i32,
    };
    if mem
        .write(
            &mut caller,
            environ_count_ptr as usize,
            &count.to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    if mem
        .write(
            &mut caller,
            environ_buf_size_ptr as usize,
            &buf_size.to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}
