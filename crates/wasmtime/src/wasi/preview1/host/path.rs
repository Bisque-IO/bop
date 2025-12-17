//! Path-related WASI Preview 1 host functions using Maniac async I/O.

use super::WasiView;
use crate::wasi::preview1::fd_table::{DirState, FdEntry};
use crate::wasi::types::*;
use crate::{as_coroutine, sync_await, try_sync_await};
use std::path::PathBuf;
use std::sync::Arc;
use wasmtime::Caller;

fn get_memory<T>(caller: &mut Caller<'_, T>) -> Option<wasmtime::Memory> {
    match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

pub fn path_create_directory<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    path_ptr: i32,
    path_len: i32,
) -> i32 {
    let fd = fd as u32;
    let dir_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::PATH_CREATE_DIRECTORY) {
            return e.raw() as i32;
        }
        match entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut path_buf = vec![0u8; path_len as usize];
    if mem.read(&caller, path_ptr as usize, &mut path_buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    let path = match std::str::from_utf8(&path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let resolved = dir_state.resolve(path);

    as_coroutine();
    let resolved_clone = resolved.clone();
    match try_sync_await(crate::unblock(move || std::fs::create_dir(&resolved_clone))) {
        Some(Ok(())) => Errno::Success.raw() as i32,
        Some(Err(e)) => io_error_to_errno(e).raw() as i32,
        None => {
            // Fallback for non-coroutine
            match std::fs::create_dir(&resolved) {
                Ok(()) => Errno::Success.raw() as i32,
                Err(e) => io_error_to_errno(e).raw() as i32,
            }
        }
    }
}

pub fn path_filestat_get<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    flags: i32,
    path_ptr: i32,
    path_len: i32,
    filestat_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let _flags = flags as u32;
    let dir_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::PATH_FILESTAT_GET) {
            return e.raw() as i32;
        }
        match entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut path_buf = vec![0u8; path_len as usize];
    if mem.read(&caller, path_ptr as usize, &mut path_buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    let path = match std::str::from_utf8(&path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let resolved = dir_state.resolve(path);

    as_coroutine();
    let stat = match try_sync_await(crate::fs::metadata(&resolved)) {
        Some(Ok(m)) => metadata_to_filestat(&m),
        Some(Err(e)) => return io_error_to_errno(e).raw() as i32,
        None => {
            // Fallback
            match std::fs::metadata(&resolved) {
                Ok(m) => std_metadata_to_filestat(&m),
                Err(e) => return io_error_to_errno(e).raw() as i32,
            }
        }
    };

    let buf = filestat_to_bytes(&stat);
    if mem.write(&mut caller, filestat_ptr as usize, &buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn path_filestat_set_times<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    _flags: i32,
    path_ptr: i32,
    path_len: i32,
    atim: i64,
    mtim: i64,
    fst_flags: i32,
) -> i32 {
    let fd = fd as u32;
    let dir_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::PATH_FILESTAT_SET_TIMES) {
            return e.raw() as i32;
        }
        match entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut path_buf = vec![0u8; path_len as usize];
    if mem.read(&caller, path_ptr as usize, &mut path_buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    let path = match std::str::from_utf8(&path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let resolved = dir_state.resolve(path);

    let fst_flags = FstFlags(fst_flags as u16);
    let atim = if fst_flags.contains(FstFlags::ATIM_NOW) {
        Some(std::time::SystemTime::now())
    } else if fst_flags.contains(FstFlags::ATIM) {
        Some(std::time::UNIX_EPOCH + std::time::Duration::from_nanos(atim as u64))
    } else {
        None
    };
    let mtim = if fst_flags.contains(FstFlags::MTIM_NOW) {
        Some(std::time::SystemTime::now())
    } else if fst_flags.contains(FstFlags::MTIM) {
        Some(std::time::UNIX_EPOCH + std::time::Duration::from_nanos(mtim as u64))
    } else {
        None
    };

    as_coroutine();
    match try_sync_await(crate::unblock(move || {
        // Open file just to set times. Note: This might fail if it's a directory/symlink on some platforms?
        // std::fs::File::open follows symlinks.
        // If we need to operate on symlinks themselves (PATH_SYMLINK_NO_FOLLOW), we'd need more logic.
        // For now standard behavior (follow symlinks) is acceptable for MVP compliance.
        let file = std::fs::File::open(&resolved)?;

        let mut times = std::fs::FileTimes::new();
        if let Some(t) = atim {
            times = times.set_accessed(t);
        }
        if let Some(t) = mtim {
            times = times.set_modified(t);
        }
        if atim.is_some() || mtim.is_some() {
            file.set_times(times)?;
        }
        Ok::<(), std::io::Error>(())
    })) {
        Some(Ok(())) => Errno::Success.raw() as i32,
        Some(Err(e)) => io_error_to_errno(e).raw() as i32,
        None => Errno::NotSup.raw() as i32,
    }
}

pub fn path_link<T: WasiView>(
    mut caller: Caller<'_, T>,
    old_fd: i32,
    _old_flags: i32,
    old_path_ptr: i32,
    old_path_len: i32,
    new_fd: i32,
    new_path_ptr: i32,
    new_path_len: i32,
) -> i32 {
    let (old_fd, new_fd) = (old_fd as u32, new_fd as u32);

    let (old_dir, new_dir) = {
        let ctx = caller.data().wasi_ctx();
        let old_entry = match ctx.fd_table.get(old_fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = old_entry.check_rights(Rights::PATH_LINK_SOURCE) {
            return e.raw() as i32;
        }
        let new_entry = match ctx.fd_table.get(new_fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = new_entry.check_rights(Rights::PATH_LINK_TARGET) {
            return e.raw() as i32;
        }
        let old_dir = match old_entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        };
        let new_dir = match new_entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        };
        (old_dir, new_dir)
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut old_path_buf = vec![0u8; old_path_len as usize];
    let mut new_path_buf = vec![0u8; new_path_len as usize];
    if mem
        .read(&caller, old_path_ptr as usize, &mut old_path_buf)
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    if mem
        .read(&caller, new_path_ptr as usize, &mut new_path_buf)
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    let old_path = match std::str::from_utf8(&old_path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let new_path = match std::str::from_utf8(&new_path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let old_resolved = old_dir.resolve(old_path);
    let new_resolved = new_dir.resolve(new_path);

    as_coroutine();
    match try_sync_await(crate::unblock(move || {
        std::fs::hard_link(&old_resolved, &new_resolved)
    })) {
        Some(Ok(())) => Errno::Success.raw() as i32,
        Some(Err(e)) => io_error_to_errno(e).raw() as i32,
        None => Errno::NotSup.raw() as i32,
    }
}

pub fn path_open<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    _dirflags: i32,
    path_ptr: i32,
    path_len: i32,
    oflags: i32,
    rights_base: i64,
    rights_inheriting: i64,
    fdflags: i32,
    opened_fd_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let oflags = OFlags(oflags as u16);
    let fdflags = FdFlags(fdflags as u16);
    let rights_base = Rights(rights_base as u64);
    let rights_inheriting = Rights(rights_inheriting as u64);

    let dir_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::PATH_OPEN) {
            return e.raw() as i32;
        }
        match entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut path_buf = vec![0u8; path_len as usize];
    if mem.read(&caller, path_ptr as usize, &mut path_buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    let path = match std::str::from_utf8(&path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let resolved = dir_state.resolve(path);

    let read = rights_base.contains(Rights::FD_READ);
    let write = rights_base.contains(Rights::FD_WRITE);
    let create = oflags.contains(OFlags::CREATE);
    let truncate = oflags.contains(OFlags::TRUNC);
    let directory = oflags.contains(OFlags::DIRECTORY);
    let excl = oflags.contains(OFlags::EXCL);
    let append = fdflags.contains(FdFlags::APPEND);

    if directory {
        // Opening a directory
        as_coroutine();
        let resolved_clone = resolved.clone();
        let is_dir = match try_sync_await(crate::unblock(move || {
            std::fs::metadata(&resolved_clone).map(|m| m.is_dir())
        })) {
            Some(Ok(b)) => b,
            Some(Err(e)) => return io_error_to_errno(e).raw() as i32,
            None => match std::fs::metadata(&resolved) {
                Ok(m) => m.is_dir(),
                Err(e) => return io_error_to_errno(e).raw() as i32,
            },
        };
        if !is_dir {
            return Errno::NotDir.raw() as i32;
        }
        let ctx = caller.data_mut().wasi_ctx_mut();
        let new_fd = ctx
            .fd_table
            .push_dir(resolved, rights_base, rights_inheriting, fdflags);
        if mem
            .write(&mut caller, opened_fd_ptr as usize, &new_fd.to_le_bytes())
            .is_err()
        {
            return Errno::Fault.raw() as i32;
        }
        return Errno::Success.raw() as i32;
    }

    // Opening a file - verify it can be opened with the requested mode
    as_coroutine();
    let file_check = try_sync_await(crate::unblock({
        let resolved = resolved.clone();
        move || {
            let mut opts = std::fs::OpenOptions::new();
            opts.read(read);
            opts.write(write);
            opts.create(create);
            opts.truncate(truncate);
            opts.create_new(excl);
            opts.append(append);
            opts.open(&resolved).map(|_| ())
        }
    }));

    match file_check {
        Some(Ok(())) => {}
        Some(Err(e)) => return io_error_to_errno(e).raw() as i32,
        None => {
            // Fallback
            let mut opts = std::fs::OpenOptions::new();
            opts.read(read);
            opts.write(write);
            opts.create(create);
            opts.truncate(truncate);
            opts.create_new(excl);
            opts.append(append);
            if let Err(e) = opts.open(&resolved) {
                return io_error_to_errno(e).raw() as i32;
            }
        }
    }

    let ctx = caller.data_mut().wasi_ctx_mut();
    let new_fd = ctx.fd_table.push_file(
        resolved,
        read,
        write,
        append,
        rights_base,
        rights_inheriting,
        fdflags,
    );

    if mem
        .write(&mut caller, opened_fd_ptr as usize, &new_fd.to_le_bytes())
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn path_readlink<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    path_ptr: i32,
    path_len: i32,
    buf_ptr: i32,
    buf_len: i32,
    bufused_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let dir_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::PATH_READLINK) {
            return e.raw() as i32;
        }
        match entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut path_buf = vec![0u8; path_len as usize];
    if mem.read(&caller, path_ptr as usize, &mut path_buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    let path = match std::str::from_utf8(&path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let resolved = dir_state.resolve(path);

    as_coroutine();
    let resolved_clone = resolved.clone();
    let target = match try_sync_await(crate::unblock(move || std::fs::read_link(&resolved_clone))) {
        Some(Ok(t)) => t,
        Some(Err(e)) => return io_error_to_errno(e).raw() as i32,
        None => match std::fs::read_link(&resolved) {
            Ok(t) => t,
            Err(e) => return io_error_to_errno(e).raw() as i32,
        },
    };
    let target_bytes = target.to_string_lossy();
    let target_bytes = target_bytes.as_bytes();
    let len = target_bytes.len().min(buf_len as usize);
    if mem
        .write(&mut caller, buf_ptr as usize, &target_bytes[..len])
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    if mem
        .write(
            &mut caller,
            bufused_ptr as usize,
            &(len as u32).to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn path_remove_directory<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    path_ptr: i32,
    path_len: i32,
) -> i32 {
    let fd = fd as u32;
    let dir_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::PATH_REMOVE_DIRECTORY) {
            return e.raw() as i32;
        }
        match entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut path_buf = vec![0u8; path_len as usize];
    if mem.read(&caller, path_ptr as usize, &mut path_buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    let path = match std::str::from_utf8(&path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let resolved = dir_state.resolve(path);

    as_coroutine();
    match try_sync_await(crate::fs::remove_dir(&resolved)) {
        Some(Ok(())) => Errno::Success.raw() as i32,
        Some(Err(e)) => io_error_to_errno(e).raw() as i32,
        None => match std::fs::remove_dir(&resolved) {
            Ok(()) => Errno::Success.raw() as i32,
            Err(e) => io_error_to_errno(e).raw() as i32,
        },
    }
}

pub fn path_rename<T: WasiView>(
    mut caller: Caller<'_, T>,
    old_fd: i32,
    old_path_ptr: i32,
    old_path_len: i32,
    new_fd: i32,
    new_path_ptr: i32,
    new_path_len: i32,
) -> i32 {
    let (old_fd, new_fd) = (old_fd as u32, new_fd as u32);

    let (old_dir, new_dir) = {
        let ctx = caller.data().wasi_ctx();
        let old_entry = match ctx.fd_table.get(old_fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = old_entry.check_rights(Rights::PATH_RENAME_SOURCE) {
            return e.raw() as i32;
        }
        let new_entry = match ctx.fd_table.get(new_fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = new_entry.check_rights(Rights::PATH_RENAME_TARGET) {
            return e.raw() as i32;
        }
        let old_dir = match old_entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        };
        let new_dir = match new_entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        };
        (old_dir, new_dir)
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut old_path_buf = vec![0u8; old_path_len as usize];
    let mut new_path_buf = vec![0u8; new_path_len as usize];
    if mem
        .read(&caller, old_path_ptr as usize, &mut old_path_buf)
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    if mem
        .read(&caller, new_path_ptr as usize, &mut new_path_buf)
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    let old_path = match std::str::from_utf8(&old_path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let new_path = match std::str::from_utf8(&new_path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let old_resolved = old_dir.resolve(old_path);
    let new_resolved = new_dir.resolve(new_path);

    as_coroutine();
    match try_sync_await(crate::fs::rename(&old_resolved, &new_resolved)) {
        Some(Ok(())) => Errno::Success.raw() as i32,
        Some(Err(e)) => io_error_to_errno(e).raw() as i32,
        None => match std::fs::rename(&old_resolved, &new_resolved) {
            Ok(()) => Errno::Success.raw() as i32,
            Err(e) => io_error_to_errno(e).raw() as i32,
        },
    }
}

pub fn path_symlink<T: WasiView>(
    mut caller: Caller<'_, T>,
    old_path_ptr: i32,
    old_path_len: i32,
    fd: i32,
    new_path_ptr: i32,
    new_path_len: i32,
) -> i32 {
    let fd = fd as u32;
    let dir_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::PATH_SYMLINK) {
            return e.raw() as i32;
        }
        match entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut old_path_buf = vec![0u8; old_path_len as usize];
    let mut new_path_buf = vec![0u8; new_path_len as usize];
    if mem
        .read(&caller, old_path_ptr as usize, &mut old_path_buf)
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    if mem
        .read(&caller, new_path_ptr as usize, &mut new_path_buf)
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    let old_path = match std::str::from_utf8(&old_path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let new_path = match std::str::from_utf8(&new_path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let new_resolved = dir_state.resolve(new_path);

    #[cfg(unix)]
    {
        let old_path_owned = old_path.to_string();
        as_coroutine();
        match try_sync_await(crate::unblock(move || {
            std::os::unix::fs::symlink(&old_path_owned, &new_resolved)
        })) {
            Some(Ok(())) => Errno::Success.raw() as i32,
            Some(Err(e)) => io_error_to_errno(e).raw() as i32,
            None => Errno::NotSup.raw() as i32,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (old_path, new_resolved); // suppress unused warning
        Errno::NotSup.raw() as i32
    }
}

pub fn path_unlink_file<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    path_ptr: i32,
    path_len: i32,
) -> i32 {
    let fd = fd as u32;
    let dir_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::PATH_UNLINK_FILE) {
            return e.raw() as i32;
        }
        match entry.dir_state() {
            Some(s) => s,
            None => return Errno::NotDir.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut path_buf = vec![0u8; path_len as usize];
    if mem.read(&caller, path_ptr as usize, &mut path_buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    let path = match std::str::from_utf8(&path_buf) {
        Ok(s) => s,
        Err(_) => return Errno::Ilseq.raw() as i32,
    };
    let resolved = dir_state.resolve(path);

    as_coroutine();
    match try_sync_await(crate::fs::remove_file(&resolved)) {
        Some(Ok(())) => Errno::Success.raw() as i32,
        Some(Err(e)) => io_error_to_errno(e).raw() as i32,
        None => match std::fs::remove_file(&resolved) {
            Ok(()) => Errno::Success.raw() as i32,
            Err(e) => io_error_to_errno(e).raw() as i32,
        },
    }
}

fn io_error_to_errno(e: std::io::Error) -> Errno {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::NotFound => Errno::NoEnt,
        ErrorKind::PermissionDenied => Errno::Access,
        ErrorKind::AlreadyExists => Errno::Exist,
        ErrorKind::InvalidInput => Errno::Inval,
        ErrorKind::NotADirectory => Errno::NotDir,
        ErrorKind::IsADirectory => Errno::IsDir,
        ErrorKind::DirectoryNotEmpty => Errno::NotEmpty,
        ErrorKind::WouldBlock => Errno::Again,
        ErrorKind::Interrupted => Errno::Intr,
        _ => Errno::Io,
    }
}

fn metadata_to_filestat(m: &crate::fs::Metadata) -> Filestat {
    let filetype = if m.is_dir() {
        Filetype::Directory
    } else if m.is_file() {
        Filetype::RegularFile
    } else if m.is_symlink() {
        Filetype::SymbolicLink
    } else {
        Filetype::Unknown
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Filestat {
            dev: m.dev(),
            ino: m.ino(),
            filetype: filetype as u8,
            nlink: m.nlink(),
            size: m.len(),
            atim: (m.atime() as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(m.atime_nsec() as u64),
            mtim: (m.mtime() as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(m.mtime_nsec() as u64),
            ctim: (m.ctime() as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(m.ctime_nsec() as u64),
        }
    }

    #[cfg(not(unix))]
    Filestat {
        dev: 0,
        ino: 0,
        filetype: filetype as u8,
        nlink: 1,
        size: m.len(),
        atim: 0,
        mtim: 0,
        ctim: 0,
    }
}

fn std_metadata_to_filestat(m: &std::fs::Metadata) -> Filestat {
    let filetype = if m.is_dir() {
        Filetype::Directory
    } else if m.is_file() {
        Filetype::RegularFile
    } else if m.is_symlink() {
        Filetype::SymbolicLink
    } else {
        Filetype::Unknown
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Filestat {
            dev: m.dev(),
            ino: m.ino(),
            filetype: filetype as u8,
            nlink: m.nlink(),
            size: m.len(),
            atim: (m.atime() as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(m.atime_nsec() as u64),
            mtim: (m.mtime() as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(m.mtime_nsec() as u64),
            ctim: (m.ctime() as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(m.ctime_nsec() as u64),
        }
    }

    #[cfg(not(unix))]
    Filestat {
        dev: 0,
        ino: 0,
        filetype: filetype as u8,
        nlink: 1,
        size: m.len(),
        atim: 0,
        mtim: 0,
        ctim: 0,
    }
}

fn filestat_to_bytes(stat: &Filestat) -> [u8; 64] {
    let mut buf = [0u8; 64];
    buf[0..8].copy_from_slice(&stat.dev.to_le_bytes());
    buf[8..16].copy_from_slice(&stat.ino.to_le_bytes());
    buf[16] = stat.filetype;
    buf[24..32].copy_from_slice(&stat.nlink.to_le_bytes());
    buf[32..40].copy_from_slice(&stat.size.to_le_bytes());
    buf[40..48].copy_from_slice(&stat.atim.to_le_bytes());
    buf[48..56].copy_from_slice(&stat.mtim.to_le_bytes());
    buf[56..64].copy_from_slice(&stat.ctim.to_le_bytes());
    buf
}
