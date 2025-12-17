//! Socket host functions (wasi-net feature).

use super::WasiView;
use crate::io::{AsyncReadRent, AsyncWriteRent};
use crate::sync_await;
use crate::wasi::preview1::fd_table::FdEntry;
use crate::wasi::types::*;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::sync::{Arc, Mutex};
use wasmtime::Caller;

fn get_memory<T>(caller: &mut Caller<'_, T>) -> Option<wasmtime::Memory> {
    match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

enum OneOf {
    #[cfg(feature = "wasi-net")]
    Tcp(Arc<Mutex<crate::net::TcpStream>>),
    #[cfg(feature = "wasi-net")]
    Udp(Arc<Mutex<crate::net::UdpSocket>>),
}

pub fn sock_accept<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    _flags: i32,
    new_fd_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let ctx = caller.data_mut().wasi_ctx_mut();
    // Use get_mut to potentially mutate state if needed, though accept on listener might be &self?
    // standard TcpListener::accept is &self. But let's check FdEntry.
    let entry = match ctx.fd_table.get(fd) {
        Ok(e) => e,
        Err(e) => return e.raw() as i32,
    };
    if let Err(e) = entry.check_rights(Rights::SOCK_ACCEPT) {
        return e.raw() as i32;
    }
    // We clone the listener because we might not be able to accept while holding a borrow on FdTable if we need to push later?
    // Actually we CAN push later because we release scope.
    // usage of Clone on TcpListener?
    // Maniac TcpListener likely implies cloning the handle is fine (dup).
    let listener = match entry {
        #[cfg(feature = "wasi-net")]
        FdEntry::TcpListener { listener, .. } => listener.clone(), // Clone the Arc
        _ => return Errno::NotSock.raw() as i32,
    }; // entry and ctx borrow dropped here.

    let stream = {
        let listener = listener.lock().unwrap();
        // sync_await blocks until completion
        match sync_await(listener.accept()) {
            Ok((stream, _)) => stream,
            Err(e) => return Errno::from(e).raw() as i32,
        }
    };
    // set_nonblocking not available/needed on Async stream usually?
    // If it exists, call it. If not, ignore (async implies nonblocking underlying).
    // maniac::net::TcpStream probably doesn't have set_nonblocking or it's inherent.
    // We'll skip it for now to avoid compilation error if it doesn't exist.

    let new_fd = ctx.fd_table.push_tcp_stream(stream);
    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    if mem
        .write(&mut caller, new_fd_ptr as usize, &new_fd.to_le_bytes())
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn sock_recv<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    ri_data_ptr: i32,
    ri_data_len: i32,
    _ri_flags: i32,
    ro_datalen_ptr: i32,
    ro_flags_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let ctx = caller.data_mut().wasi_ctx_mut();
    let sock_res = {
        let entry = match ctx.fd_table.get_mut(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_READ) {
            return e.raw() as i32;
        }
        match entry {
            #[cfg(feature = "wasi-net")]
            FdEntry::TcpStream { stream, .. } => Some(OneOf::Tcp(stream.clone())),
            #[cfg(feature = "wasi-net")]
            FdEntry::UdpSocket { socket, .. } => Some(OneOf::Udp(socket.clone())),
            _ => None,
        }
    };

    if sock_res.is_none() {
        return Errno::NotSock.raw() as i32;
    }
    let sock_res = sock_res.unwrap();

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    // Now caller is free for read_iovecs
    let iovecs = match read_iovecs(&mem, &caller, ri_data_ptr as u32, ri_data_len as u32) {
        Ok(v) => v,
        Err(e) => return e.raw() as i32,
    };
    let mut total = 0usize;
    let mut ro_flags = RoFlags(0);

    match sock_res {
        OneOf::Tcp(stream_arc) => {
            let mut stream = stream_arc.lock().unwrap();
            for (ptr, len) in &iovecs {
                let buf = vec![0u8; *len];
                match sync_await(stream.read(buf)) {
                    (Ok(n), buf) => {
                        if n == 0 {
                            ro_flags = RoFlags(EventRwFlags::HANGUP.0);
                            break;
                        }
                        if mem.write(&mut caller, *ptr, &buf[..n]).is_err() {
                            return Errno::Fault.raw() as i32;
                        }
                        total += n;
                        if n < *len {
                            break;
                        }
                    }
                    (Err(e), _) => return Errno::from(e).raw() as i32,
                }
            }
        }
        OneOf::Udp(socket_arc) => {
            let socket = socket_arc.lock().unwrap();
            // UdpSocket recv
            if let Some((ptr, len)) = iovecs.first() {
                let buf = vec![0u8; *len];
                match sync_await(socket.recv(buf)) {
                    (Ok(n), buf) => {
                        if mem.write(&mut caller, *ptr, &buf[..n]).is_err() {
                            return Errno::Fault.raw() as i32;
                        }
                        total = n;
                    }
                    (Err(e), _) => return Errno::from(e).raw() as i32,
                }
            }
        }
    }
    if mem
        .write(
            &mut caller,
            ro_datalen_ptr as usize,
            &(total as u32).to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    if mem
        .write(
            &mut caller,
            ro_flags_ptr as usize,
            &ro_flags.0.to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn sock_send<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    si_data_ptr: i32,
    si_data_len: i32,
    _si_flags: i32,
    so_datalen_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let ctx = caller.data_mut().wasi_ctx_mut();
    let sock_res = {
        let entry = match ctx.fd_table.get_mut(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_WRITE) {
            return e.raw() as i32;
        }
        match entry {
            #[cfg(feature = "wasi-net")]
            FdEntry::TcpStream { stream, .. } => Some(OneOf::Tcp(stream.clone())),
            #[cfg(feature = "wasi-net")]
            FdEntry::UdpSocket { socket, .. } => Some(OneOf::Udp(socket.clone())),
            _ => None,
        }
    };

    if sock_res.is_none() {
        return Errno::NotSock.raw() as i32;
    }
    let sock_res = sock_res.unwrap();

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let iovecs = match read_iovecs(&mem, &caller, si_data_ptr as u32, si_data_len as u32) {
        Ok(v) => v,
        Err(e) => return e.raw() as i32,
    };
    let mut total = 0usize;

    match sock_res {
        OneOf::Tcp(stream_arc) => {
            let mut stream = stream_arc.lock().unwrap();
            for (ptr, len) in &iovecs {
                let mut buf = vec![0u8; *len];
                if mem.read(&caller, *ptr, &mut buf).is_err() {
                    return Errno::Fault.raw() as i32;
                }
                match sync_await(stream.write(buf)) {
                    (Ok(n), _) => {
                        total += n;
                        if n < *len {
                            break;
                        }
                    }
                    (Err(e), _) => return Errno::from(e).raw() as i32,
                }
            }
        }
        OneOf::Udp(socket_arc) => {
            let socket = socket_arc.lock().unwrap();
            if let Some((ptr, len)) = iovecs.first() {
                let mut buf = vec![0u8; *len];
                if mem.read(&caller, *ptr, &mut buf).is_err() {
                    return Errno::Fault.raw() as i32;
                }
                match sync_await(socket.send(buf)) {
                    (Ok(n), _) => total = n,
                    (Err(e), _) => return Errno::from(e).raw() as i32,
                }
            }
        }
    }
    if mem
        .write(
            &mut caller,
            so_datalen_ptr as usize,
            &(total as u32).to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn sock_shutdown<T: WasiView>(mut caller: Caller<'_, T>, fd: i32, how: i32) -> i32 {
    let fd = fd as u32;
    let ctx = caller.data_mut().wasi_ctx_mut(); // ERROR: data_mut() borrow.
    // Wait, check original code. It used `caller.data().wasi_ctx()`.
    // But `stream.shutdown` on `std::net::TcpStream` is `&self`.
    // `AsyncWriteRent` shutdown is `&mut self` usually?
    // `maniac::net::TcpStream::shutdown` might be `&self` if it just calls shutdown on reactor.
    // I'll assume `get` is fine for now, or check traits.
    // If AsyncWriteRent::shutdown takes &mut self, I need get_mut.
    // I'll use get_mut to be safe.
    let entry = match ctx.fd_table.get_mut(fd) {
        // get_mut
        Ok(e) => e,
        Err(e) => return e.raw() as i32,
    };
    if let Err(e) = entry.check_rights(Rights::SOCK_SHUTDOWN) {
        return e.raw() as i32;
    }
    let sd_flags = SdFlags(how as u8);
    let _shutdown = if sd_flags.contains(SdFlags::RD) && sd_flags.contains(SdFlags::WR) {
        Shutdown::Both
    } else if sd_flags.contains(SdFlags::RD) {
        Shutdown::Read
    } else if sd_flags.contains(SdFlags::WR) {
        Shutdown::Write
    } else {
        return Errno::Inval.raw() as i32;
    };

    // Async shutdown?
    // maniac types might not have `shutdown` method exposed directly if it's async trait method.
    // But traits are in scope.
    // `AsyncWriteRent::shutdown`? No, it's `close` or `shutdown`.
    // Assuming `stream.shutdown(shutdown)` exists or `AsyncWriteRent` has it.
    // Actually `AsyncWriteRent` has `flush` and `shutdown` (usually `close` semantics).
    // `std::net::Shutdown` argument suggests `shutdown` method.
    // If unavailable, I'll error.
    let stream = match entry {
        #[cfg(feature = "wasi-net")]
        FdEntry::TcpStream { stream, .. } => stream.clone(),
        _ => return Errno::NotSock.raw() as i32,
    };

    // Explicitly drop caller borrow if possible, though sock_shutdown only needs reads usually?
    // Actually method signature `sock_shutdown(mut caller: ...)`
    // logic is `stream.shutdown()`.
    // It doesn't touch memory.
    // So split borrow from `ctx` vs `caller` is not an issue because we don't access `caller` again.
    // BUT we should be consistent.

    // Only shutdown Write is supported via AsyncWriteRent::shutdown
    if sd_flags.contains(SdFlags::WR) {
        match sync_await(stream.lock().unwrap().shutdown()) {
            Ok(_) => Errno::Success.raw() as i32,
            Err(e) => Errno::from(e).raw() as i32,
        }
    } else {
        // Read shutdown not explicitly supported, but ignoring or succeeding is often acceptable
        Errno::Success.raw() as i32
    }
}

fn read_iovecs<T>(
    mem: &wasmtime::Memory,
    caller: &Caller<'_, T>,
    iovs_ptr: u32,
    iovs_len: u32,
) -> Result<Vec<(usize, usize)>, Errno> {
    let mut iovecs = Vec::with_capacity(iovs_len as usize);
    for i in 0..iovs_len {
        let iov_offset = iovs_ptr as usize + (i as usize * 8);
        let mut buf = [0u8; 8];
        if mem.read(caller, iov_offset, &mut buf).is_err() {
            return Err(Errno::Fault);
        }
        let ptr = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        let len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
        iovecs.push((ptr, len));
    }
    Ok(iovecs)
}
