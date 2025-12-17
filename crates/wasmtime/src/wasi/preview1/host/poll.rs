//! poll_oneoff implementation using Maniac async time.

use super::WasiView;
use crate::wasi::types::*;
use crate::{as_coroutine, try_sync_await};
use std::time::Duration;
use wasmtime::Caller;

pub fn poll_oneoff<T: WasiView>(
    mut caller: Caller<'_, T>,
    in_ptr: i32,
    out_ptr: i32,
    nsubscriptions: i32,
    nevents_ptr: i32,
) -> i32 {
    if nsubscriptions <= 0 {
        return Errno::Inval.raw() as i32;
    }
    let mem = match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => m,
        _ => return Errno::Inval.raw() as i32,
    };
    const SUBSCRIPTION_SIZE: usize = 48;
    const EVENT_SIZE: usize = 32;
    let mut events = Vec::new();
    let mut min_timeout: Option<Duration> = None;

    for i in 0..nsubscriptions as u32 {
        let sub_offset = in_ptr as usize + (i as usize * SUBSCRIPTION_SIZE);
        let mut sub_buf = [0u8; SUBSCRIPTION_SIZE];
        if mem.read(&caller, sub_offset, &mut sub_buf).is_err() {
            return Errno::Fault.raw() as i32;
        }
        let userdata = u64::from_le_bytes(sub_buf[0..8].try_into().unwrap());
        let tag = sub_buf[8];
        match tag {
            0 => {
                // Clock subscription
                let timeout = u64::from_le_bytes(sub_buf[24..32].try_into().unwrap());
                let flags = u16::from_le_bytes(sub_buf[40..42].try_into().unwrap());
                let is_abstime = SubclockFlags(flags).contains(SubclockFlags::ABSTIME);
                let duration = if is_abstime {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as u64;
                    if timeout > now {
                        Duration::from_nanos(timeout - now)
                    } else {
                        Duration::ZERO
                    }
                } else {
                    Duration::from_nanos(timeout)
                };
                min_timeout = Some(min_timeout.map_or(duration, |t| t.min(duration)));
                events.push((
                    userdata,
                    EventType::Clock,
                    Errno::Success,
                    0u64,
                    EventRwFlags(0),
                ));
            }
            1 => {
                // FD read subscription
                let fd = u32::from_le_bytes(sub_buf[16..20].try_into().unwrap());
                let ctx = caller.data().wasi_ctx();
                match ctx.fd_table.get(fd) {
                    Ok(entry) => {
                        if entry.check_rights(Rights::POLL_FD_READWRITE).is_err() {
                            events.push((
                                userdata,
                                EventType::FdRead,
                                Errno::NotCapable,
                                0,
                                EventRwFlags(0),
                            ));
                        } else {
                            // For simplicity, report ready immediately
                            // A full implementation would check actual fd readiness
                            events.push((
                                userdata,
                                EventType::FdRead,
                                Errno::Success,
                                u64::MAX,
                                EventRwFlags(0),
                            ));
                        }
                    }
                    Err(e) => events.push((userdata, EventType::FdRead, e, 0, EventRwFlags(0))),
                }
            }
            2 => {
                // FD write subscription
                let fd = u32::from_le_bytes(sub_buf[16..20].try_into().unwrap());
                let ctx = caller.data().wasi_ctx();
                match ctx.fd_table.get(fd) {
                    Ok(entry) => {
                        if entry.check_rights(Rights::POLL_FD_READWRITE).is_err() {
                            events.push((
                                userdata,
                                EventType::FdWrite,
                                Errno::NotCapable,
                                0,
                                EventRwFlags(0),
                            ));
                        } else {
                            events.push((
                                userdata,
                                EventType::FdWrite,
                                Errno::Success,
                                u64::MAX,
                                EventRwFlags(0),
                            ));
                        }
                    }
                    Err(e) => events.push((userdata, EventType::FdWrite, e, 0, EventRwFlags(0))),
                }
            }
            _ => return Errno::Inval.raw() as i32,
        }
    }

    // Handle timeout using maniac's async sleep
    if let Some(duration) = min_timeout {
        if !duration.is_zero() {
            as_coroutine();
            // Use maniac's async sleep via sync_await
            if try_sync_await(crate::time::sleep(duration)).is_none() {
                // Fallback to blocking sleep if not in coroutine
                std::thread::sleep(duration);
            }
        }
    }

    let nevents = events.len().min(nsubscriptions as usize);
    for (i, (userdata, event_type, errno, nbytes, flags)) in events.iter().take(nevents).enumerate()
    {
        let event_offset = out_ptr as usize + (i * EVENT_SIZE);
        let mut event_buf = [0u8; EVENT_SIZE];
        event_buf[0..8].copy_from_slice(&userdata.to_le_bytes());
        event_buf[8..10].copy_from_slice(&errno.raw().to_le_bytes());
        event_buf[10] = *event_type as u8;
        event_buf[16..24].copy_from_slice(&nbytes.to_le_bytes());
        event_buf[24..26].copy_from_slice(&flags.0.to_le_bytes());
        if mem.write(&mut caller, event_offset, &event_buf).is_err() {
            return Errno::Fault.raw() as i32;
        }
    }
    if mem
        .write(
            &mut caller,
            nevents_ptr as usize,
            &(nevents as u32).to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}
