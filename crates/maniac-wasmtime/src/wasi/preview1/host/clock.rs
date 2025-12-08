//! Clock host functions using Maniac async time.

use super::WasiView;
use crate::wasi::types::*;
use wasmtime::Caller;

fn get_memory<T>(caller: &mut Caller<'_, T>) -> Option<wasmtime::Memory> {
    match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

pub fn clock_res_get<T: WasiView>(
    mut caller: Caller<'_, T>,
    clock_id: i32,
    resolution_ptr: i32,
) -> i32 {
    let clock = match ClockId::from_raw(clock_id as u32) {
        Some(c) => c,
        None => return Errno::Inval.raw() as i32,
    };

    let resolution: u64 = match clock {
        ClockId::Realtime | ClockId::Monotonic => 1, // nanosecond precision
        ClockId::ProcessCpuTimeId | ClockId::ThreadCpuTimeId => 1000, // microsecond
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    if mem
        .write(
            &mut caller,
            resolution_ptr as usize,
            &resolution.to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn clock_time_get<T: WasiView>(
    mut caller: Caller<'_, T>,
    clock_id: i32,
    _precision: i64,
    time_ptr: i32,
) -> i32 {
    let clock = match ClockId::from_raw(clock_id as u32) {
        Some(c) => c,
        None => return Errno::Inval.raw() as i32,
    };

    let time = match clock {
        ClockId::Realtime => {
            // Use maniac's worker time if available, otherwise system time
            if let Some(ns) = crate::runtime::worker::current_timer_wheel().map(|tw| tw.now_ns()) {
                ns
            } else {
                use std::time::{SystemTime, UNIX_EPOCH};
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0)
            }
        }
        ClockId::Monotonic => {
            // Use maniac's worker time if available
            if let Some(ns) = crate::runtime::worker::current_timer_wheel().map(|tw| tw.now_ns()) {
                ns
            } else {
                use std::time::Instant;
                // Fallback to std::time::Instant
                static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
                let start = START.get_or_init(Instant::now);
                start.elapsed().as_nanos() as u64
            }
        }
        ClockId::ProcessCpuTimeId | ClockId::ThreadCpuTimeId => {
            // These require platform-specific APIs
            // Return monotonic time as approximation
            use std::time::Instant;
            static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let start = START.get_or_init(Instant::now);
            start.elapsed().as_nanos() as u64
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    if mem
        .write(&mut caller, time_ptr as usize, &time.to_le_bytes())
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}
