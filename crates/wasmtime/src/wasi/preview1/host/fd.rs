//! File descriptor host functions using Maniac async I/O.
//!
//! All I/O operations use `sync_await` to await maniac's async primitives
//! from within synchronous host functions, yielding the coroutine when pending.

use super::WasiView;
use crate::wasi::preview1::fd_table::{DirState, FdEntry, FileState};
use crate::wasi::types::*;
use crate::{as_coroutine, sync_await, try_sync_await};
use std::io::{self, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use wasmtime::Caller;

fn get_memory<T>(caller: &mut Caller<'_, T>) -> Option<wasmtime::Memory> {
    match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

pub fn fd_advise<T: WasiView>(
    caller: Caller<'_, T>,
    fd: i32,
    _offset: i64,
    _len: i64,
    _advice: i32,
) -> i32 {
    let fd = fd as u32;
    let ctx = caller.data().wasi_ctx();
    match ctx.fd_table.get(fd) {
        Ok(entry) => {
            if let Err(e) = entry.check_rights(Rights::FD_ADVISE) {
                return e.raw() as i32;
            }
        }
        Err(e) => return e.raw() as i32,
    }
    Errno::Success.raw() as i32
}

pub fn fd_allocate<T: WasiView>(caller: Caller<'_, T>, fd: i32, _offset: i64, _len: i64) -> i32 {
    let fd = fd as u32;
    let ctx = caller.data().wasi_ctx();
    match ctx.fd_table.get(fd) {
        Ok(entry) => {
            if let Err(e) = entry.check_rights(Rights::FD_ALLOCATE) {
                return e.raw() as i32;
            }
        }
        Err(e) => return e.raw() as i32,
    }
    Errno::NotSup.raw() as i32
}

pub fn fd_close<T: WasiView>(mut caller: Caller<'_, T>, fd: i32) -> i32 {
    let fd = fd as u32;
    if fd < 3 {
        return Errno::NotSup.raw() as i32;
    }
    let ctx = caller.data_mut().wasi_ctx_mut();
    match ctx.fd_table.remove(fd) {
        Ok(_) => Errno::Success.raw() as i32,
        Err(e) => e.raw() as i32,
    }
}

pub fn fd_datasync<T: WasiView>(caller: Caller<'_, T>, fd: i32) -> i32 {
    let fd = fd as u32;
    let file_path = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_DATASYNC) {
            return e.raw() as i32;
        }
        match entry.file_state() {
            Some(state) => state.lock().unwrap().path.clone(),
            None => return Errno::Inval.raw() as i32,
        }
    };

    // Use async fsync via sync_await
    as_coroutine();
    match try_sync_await(async_datasync(file_path)) {
        Some(Ok(())) => Errno::Success.raw() as i32,
        Some(Err(_)) => Errno::Io.raw() as i32,
        None => {
            // Fallback for non-coroutine context
            Errno::Success.raw() as i32
        }
    }
}

async fn async_datasync(path: PathBuf) -> io::Result<()> {
    let file = crate::fs::File::open(&path).await?;
    file.sync_data().await?;
    file.close().await
}

pub fn fd_fdstat_get<T: WasiView>(mut caller: Caller<'_, T>, fd: i32, fdstat_ptr: i32) -> i32 {
    let fd = fd as u32;
    let (filetype, flags, rights_base, rights_inheriting) = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        (
            entry.filetype(),
            entry.flags(),
            entry.rights_base(),
            entry.rights_inheriting(),
        )
    };
    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut buf = [0u8; 24];
    buf[0] = filetype as u8;
    buf[2..4].copy_from_slice(&flags.raw().to_le_bytes());
    buf[8..16].copy_from_slice(&rights_base.raw().to_le_bytes());
    buf[16..24].copy_from_slice(&rights_inheriting.raw().to_le_bytes());
    if mem.write(&mut caller, fdstat_ptr as usize, &buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn fd_fdstat_set_flags<T: WasiView>(mut caller: Caller<'_, T>, fd: i32, flags: i32) -> i32 {
    let fd = fd as u32;
    let flags = flags as u16;
    let ctx = caller.data_mut().wasi_ctx_mut();
    let entry = match ctx.fd_table.get_mut(fd) {
        Ok(e) => e,
        Err(e) => return e.raw() as i32,
    };
    if let Err(e) = entry.check_rights(Rights::FD_FDSTAT_SET_FLAGS) {
        return e.raw() as i32;
    }
    entry.set_flags(FdFlags(flags));
    Errno::Success.raw() as i32
}

pub fn fd_fdstat_set_rights<T: WasiView>(
    _caller: Caller<'_, T>,
    _fd: i32,
    _rb: i64,
    _ri: i64,
) -> i32 {
    Errno::Success.raw() as i32
}

pub fn fd_filestat_get<T: WasiView>(mut caller: Caller<'_, T>, fd: i32, filestat_ptr: i32) -> i32 {
    let fd = fd as u32;
    let stat = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_FILESTAT_GET) {
            return e.raw() as i32;
        }

        match entry {
            FdEntry::File { state, .. } => {
                let path = state.lock().unwrap().path.clone();
                as_coroutine();
                match try_sync_await(async_metadata(path)) {
                    Some(Ok(m)) => m,
                    Some(Err(_)) => return Errno::Io.raw() as i32,
                    None => return Errno::NotSup.raw() as i32,
                }
            }
            FdEntry::Dir { state, .. } => {
                let path = state.host_path.clone();
                as_coroutine();
                match try_sync_await(async_metadata(path)) {
                    Some(Ok(m)) => m,
                    Some(Err(_)) => return Errno::Io.raw() as i32,
                    None => return Errno::NotSup.raw() as i32,
                }
            }
            FdEntry::Stdin | FdEntry::Stdout | FdEntry::Stderr => Filestat {
                dev: 0,
                ino: fd as u64,
                filetype: Filetype::CharacterDevice as u8,
                nlink: 1,
                size: 0,
                atim: 0,
                mtim: 0,
                ctim: 0,
            },
            #[cfg(feature = "wasi-net")]
            _ => return Errno::Inval.raw() as i32,
        }
    };
    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let buf = filestat_to_bytes(&stat);
    if mem.write(&mut caller, filestat_ptr as usize, &buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

async fn async_metadata(path: PathBuf) -> io::Result<Filestat> {
    let metadata = crate::fs::metadata(&path).await?;
    Ok(metadata_to_filestat(&metadata))
}

pub fn fd_filestat_set_size<T: WasiView>(caller: Caller<'_, T>, fd: i32, size: i64) -> i32 {
    let fd = fd as u32;
    let file_path = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_FILESTAT_SET_SIZE) {
            return e.raw() as i32;
        }
        match entry.file_state() {
            Some(state) => state.lock().unwrap().path.clone(),
            None => return Errno::Inval.raw() as i32,
        }
    };

    // Use blocking unblock for truncate as maniac::fs doesn't have set_len
    as_coroutine();
    let size = size as u64;
    match try_sync_await(crate::unblock(move || {
        std::fs::File::options()
            .write(true)
            .open(&file_path)
            .and_then(|f| f.set_len(size))
    })) {
        Some(Ok(())) => Errno::Success.raw() as i32,
        Some(Err(_)) => Errno::Io.raw() as i32,
        None => Errno::NotSup.raw() as i32,
    }
}

pub fn fd_filestat_set_times<T: WasiView>(
    caller: Caller<'_, T>,
    fd: i32,
    atim: i64,
    mtim: i64,
    fst_flags: i32,
) -> i32 {
    let fd = fd as u32;
    let ctx = caller.data().wasi_ctx();
    // Use sync implementation wrapped in unblock as there's no async set_times in maniac::fs yet
    let path = match ctx.fd_table.get(fd) {
        Ok(e) => {
            if let Err(e) = e.check_rights(Rights::FD_FILESTAT_SET_TIMES) {
                return e.raw() as i32;
            }
            match e.file_state() {
                Some(s) => s.lock().unwrap().path.clone(),
                None => return Errno::Inval.raw() as i32,
            }
        }
        Err(e) => return e.raw() as i32,
    };

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
        let file = std::fs::File::open(&path)?;
        if let Some(t) = atim {
            file.set_times(std::fs::FileTimes::new().set_accessed(t))?;
        }
        // Re-open/handle mtim separately or together?
        // std::fs::FileTimes allows setting both.
        // But we need to keep existing time if one is None.
        // Loading metadata to check existing is expensive.
        // For strict WASI compliance we often need to be careful.
        // However, most runtimes just set what is provided.
        // Let's assume FileTimes builder works incrementally if we construct it carefully.
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

pub fn fd_pread<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    iovs_ptr: i32,
    iovs_len: i32,
    offset: i64,
    nread_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let file_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_READ | Rights::FD_SEEK) {
            return e.raw() as i32;
        }
        match entry.file_state() {
            Some(s) => s,
            None => return Errno::Inval.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let iovecs = match read_iovecs(&mem, &caller, iovs_ptr as u32, iovs_len as u32) {
        Ok(v) => v,
        Err(e) => return e.raw() as i32,
    };

    let path = file_state.lock().unwrap().path.clone();
    let total_len: usize = iovecs.iter().map(|(_, len)| *len).sum();

    as_coroutine();
    let read_result = try_sync_await(async_pread(path, offset as u64, total_len));

    let data = match read_result {
        Some(Ok(d)) => d,
        Some(Err(_)) => return Errno::Io.raw() as i32,
        None => return Errno::NotSup.raw() as i32,
    };

    // Copy data to iovecs
    let mut data_offset = 0usize;
    let mut total_written = 0usize;
    for (ptr, len) in &iovecs {
        let to_write = (*len).min(data.len() - data_offset);
        if to_write == 0 {
            break;
        }
        if mem
            .write(
                &mut caller,
                *ptr,
                &data[data_offset..data_offset + to_write],
            )
            .is_err()
        {
            return Errno::Fault.raw() as i32;
        }
        data_offset += to_write;
        total_written += to_write;
    }

    if mem
        .write(
            &mut caller,
            nread_ptr as usize,
            &(total_written as u32).to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

async fn async_pread(path: PathBuf, offset: u64, len: usize) -> io::Result<Vec<u8>> {
    let file = crate::fs::File::open(&path).await?;
    let buf = vec![0u8; len];
    let (result, buf) = file.read_at(buf, offset).await;
    result?;
    Ok(buf)
}

pub fn fd_prestat_get<T: WasiView>(mut caller: Caller<'_, T>, fd: i32, prestat_ptr: i32) -> i32 {
    let fd = fd as u32;
    let path_len = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        match entry.preopen_path() {
            Some(p) => p.len(),
            None => return Errno::BadF.raw() as i32,
        }
    };
    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let mut buf = [0u8; 8];
    buf[0] = 0;
    buf[4..8].copy_from_slice(&(path_len as u32).to_le_bytes());
    if mem.write(&mut caller, prestat_ptr as usize, &buf).is_err() {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn fd_prestat_dir_name<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    path_ptr: i32,
    path_len: i32,
) -> i32 {
    let fd = fd as u32;
    let path = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        match entry.preopen_path() {
            Some(p) => p.to_string(),
            None => return Errno::BadF.raw() as i32,
        }
    };
    if path.len() > path_len as usize {
        return Errno::NameTooLong.raw() as i32;
    }
    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    if mem
        .write(&mut caller, path_ptr as usize, path.as_bytes())
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn fd_pwrite<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    iovs_ptr: i32,
    iovs_len: i32,
    offset: i64,
    nwritten_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let file_state = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_WRITE | Rights::FD_SEEK) {
            return e.raw() as i32;
        }
        match entry.file_state() {
            Some(s) => s,
            None => return Errno::Inval.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let iovecs = match read_iovecs(&mem, &caller, iovs_ptr as u32, iovs_len as u32) {
        Ok(v) => v,
        Err(e) => return e.raw() as i32,
    };

    // Gather data from iovecs
    let mut data = Vec::new();
    for (ptr, len) in &iovecs {
        let mut buf = vec![0u8; *len];
        if mem.read(&caller, *ptr, &mut buf).is_err() {
            return Errno::Fault.raw() as i32;
        }
        data.extend_from_slice(&buf);
    }

    let path = file_state.lock().unwrap().path.clone();

    as_coroutine();
    let write_result = try_sync_await(async_pwrite(path, offset as u64, data));

    let written = match write_result {
        Some(Ok(n)) => n,
        Some(Err(_)) => return Errno::Io.raw() as i32,
        None => return Errno::NotSup.raw() as i32,
    };

    if mem
        .write(
            &mut caller,
            nwritten_ptr as usize,
            &(written as u32).to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

async fn async_pwrite(path: PathBuf, offset: u64, data: Vec<u8>) -> io::Result<usize> {
    let file = crate::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .await?;
    let len = data.len();
    let (result, _) = file.write_at(data, offset).await;
    result?;
    Ok(len)
}

pub fn fd_read<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    iovs_ptr: i32,
    iovs_len: i32,
    nread_ptr: i32,
) -> i32 {
    let fd = fd as u32;

    enum ReadSource {
        Stdin,
        File { path: PathBuf, position: u64 },
    }

    let (source, file_state) = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_READ) {
            return e.raw() as i32;
        }
        match entry {
            FdEntry::Stdin => (ReadSource::Stdin, None),
            FdEntry::File { state, .. } => {
                let s = state.lock().unwrap();
                (
                    ReadSource::File {
                        path: s.path.clone(),
                        position: s.position,
                    },
                    Some(Arc::clone(state)),
                )
            }
            _ => return Errno::Inval.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let iovecs = match read_iovecs(&mem, &caller, iovs_ptr as u32, iovs_len as u32) {
        Ok(v) => v,
        Err(e) => return e.raw() as i32,
    };

    let total_len: usize = iovecs.iter().map(|(_, len)| *len).sum();

    let (data, bytes_read) = match source {
        ReadSource::Stdin => {
            // Use blocking unblock for stdin
            as_coroutine();
            match try_sync_await(crate::unblock(move || {
                use std::io::Read;
                let mut buf = vec![0u8; total_len];
                let n = std::io::stdin().read(&mut buf)?;
                buf.truncate(n);
                Ok::<_, io::Error>(buf)
            })) {
                Some(Ok(buf)) => {
                    let n = buf.len();
                    (buf, n)
                }
                Some(Err(_)) => return Errno::Io.raw() as i32,
                None => return Errno::NotSup.raw() as i32,
            }
        }
        ReadSource::File { path, position } => {
            as_coroutine();
            match try_sync_await(async_pread(path, position, total_len)) {
                Some(Ok(buf)) => {
                    let n = buf.len();
                    (buf, n)
                }
                Some(Err(_)) => return Errno::Io.raw() as i32,
                None => return Errno::NotSup.raw() as i32,
            }
        }
    };

    // Update file position
    if let Some(state) = file_state {
        state.lock().unwrap().position += bytes_read as u64;
    }

    // Copy data to iovecs
    let mut data_offset = 0usize;
    for (ptr, len) in &iovecs {
        let to_write = (*len).min(data.len() - data_offset);
        if to_write == 0 {
            break;
        }
        if mem
            .write(
                &mut caller,
                *ptr,
                &data[data_offset..data_offset + to_write],
            )
            .is_err()
        {
            return Errno::Fault.raw() as i32;
        }
        data_offset += to_write;
    }

    if mem
        .write(
            &mut caller,
            nread_ptr as usize,
            &(bytes_read as u32).to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn fd_readdir<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    buf_ptr: i32,
    buf_len: i32,
    cookie: i64,
    bufused_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let dir_path = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_READDIR) {
            return e.raw() as i32;
        }
        match entry.dir_state() {
            Some(s) => s.host_path.clone(),
            None => return Errno::NotDir.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };

    // Read directory entries using blocking unblock
    as_coroutine();
    let entries: Vec<(usize, (String, Filetype))> =
        match try_sync_await(crate::unblock(move || {
            let entries: Vec<_> = std::fs::read_dir(&dir_path)?
                .filter_map(|e| e.ok())
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let ft = e.file_type().ok();
                    let filetype = if ft.as_ref().map(|t| t.is_dir()).unwrap_or(false) {
                        Filetype::Directory
                    } else if ft.as_ref().map(|t| t.is_symlink()).unwrap_or(false) {
                        Filetype::SymbolicLink
                    } else {
                        Filetype::RegularFile
                    };
                    (name, filetype)
                })
                .enumerate()
                .collect();
            Ok::<_, io::Error>(entries)
        })) {
            Some(Ok(e)) => e,
            _ => return Errno::Io.raw() as i32,
        };

    let mut offset = 0usize;
    let buf_len = buf_len as usize;
    for (idx, (name, filetype)) in entries.into_iter().skip(cookie as usize) {
        let dirent_size = 24;
        let entry_size = dirent_size + name.len();
        if offset + entry_size > buf_len {
            break;
        }
        let mut dirent = [0u8; 24];
        dirent[0..8].copy_from_slice(&((idx + 1) as u64).to_le_bytes());
        dirent[8..16].copy_from_slice(&(idx as u64).to_le_bytes());
        dirent[16..20].copy_from_slice(&(name.len() as u32).to_le_bytes());
        dirent[20] = filetype as u8;
        if mem
            .write(&mut caller, buf_ptr as usize + offset, &dirent)
            .is_err()
        {
            return Errno::Fault.raw() as i32;
        }
        offset += dirent_size;
        if mem
            .write(&mut caller, buf_ptr as usize + offset, name.as_bytes())
            .is_err()
        {
            return Errno::Fault.raw() as i32;
        }
        offset += name.len();
    }
    if mem
        .write(
            &mut caller,
            bufused_ptr as usize,
            &(offset as u32).to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn fd_renumber<T: WasiView>(mut caller: Caller<'_, T>, fd: i32, to: i32) -> i32 {
    let (fd, to) = (fd as u32, to as u32);
    if fd < 3 || to < 3 {
        return Errno::NotSup.raw() as i32;
    }
    let ctx = caller.data_mut().wasi_ctx_mut();
    match ctx.fd_table.renumber(fd, to) {
        Ok(_) => Errno::Success.raw() as i32,
        Err(e) => e.raw() as i32,
    }
}

pub fn fd_seek<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    offset: i64,
    whence: i32,
    newoffset_ptr: i32,
) -> i32 {
    let fd = fd as u32;
    let whence = match Whence::from_raw(whence as u8) {
        Some(w) => w,
        None => return Errno::Inval.raw() as i32,
    };

    let new_offset = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_SEEK) {
            return e.raw() as i32;
        }
        let file_state = match entry.file_state() {
            Some(s) => s,
            None => return Errno::SPipe.raw() as i32,
        };

        let state = file_state.lock().unwrap();

        // Get file size for End seeking
        let file_size = if matches!(whence, Whence::End) {
            let path = state.path.clone();
            drop(state);
            as_coroutine();
            match try_sync_await(crate::unblock(move || {
                std::fs::metadata(&path).map(|m| m.len())
            })) {
                Some(Ok(size)) => size,
                _ => return Errno::Io.raw() as i32,
            }
        } else {
            0
        };

        let mut state = file_state.lock().unwrap();
        let new_pos = match whence {
            Whence::Set => offset as u64,
            Whence::Cur => (state.position as i64 + offset) as u64,
            Whence::End => (file_size as i64 + offset) as u64,
        };
        state.position = new_pos;
        new_pos
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    if mem
        .write(
            &mut caller,
            newoffset_ptr as usize,
            &new_offset.to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn fd_sync<T: WasiView>(caller: Caller<'_, T>, fd: i32) -> i32 {
    let fd = fd as u32;
    let file_path = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_SYNC) {
            return e.raw() as i32;
        }
        match entry.file_state() {
            Some(state) => state.lock().unwrap().path.clone(),
            None => return Errno::Inval.raw() as i32,
        }
    };

    as_coroutine();
    match try_sync_await(async_sync(file_path)) {
        Some(Ok(())) => Errno::Success.raw() as i32,
        Some(Err(_)) => Errno::Io.raw() as i32,
        None => Errno::Success.raw() as i32,
    }
}

async fn async_sync(path: PathBuf) -> io::Result<()> {
    let file = crate::fs::File::open(&path).await?;
    file.sync_all().await?;
    file.close().await
}

pub fn fd_tell<T: WasiView>(mut caller: Caller<'_, T>, fd: i32, offset_ptr: i32) -> i32 {
    let fd = fd as u32;
    let offset = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_TELL) {
            return e.raw() as i32;
        }
        match entry.file_state() {
            Some(s) => s.lock().unwrap().position,
            None => return Errno::SPipe.raw() as i32,
        }
    };
    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    if mem
        .write(&mut caller, offset_ptr as usize, &offset.to_le_bytes())
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

pub fn fd_write<T: WasiView>(
    mut caller: Caller<'_, T>,
    fd: i32,
    iovs_ptr: i32,
    iovs_len: i32,
    nwritten_ptr: i32,
) -> i32 {
    let fd = fd as u32;

    enum WriteTarget {
        Stdout,
        Stderr,
        File {
            path: PathBuf,
            position: u64,
            append: bool,
        },
    }

    let (target, file_state) = {
        let ctx = caller.data().wasi_ctx();
        let entry = match ctx.fd_table.get(fd) {
            Ok(e) => e,
            Err(e) => return e.raw() as i32,
        };
        if let Err(e) = entry.check_rights(Rights::FD_WRITE) {
            return e.raw() as i32;
        }
        match entry {
            FdEntry::Stdout => (WriteTarget::Stdout, None),
            FdEntry::Stderr => (WriteTarget::Stderr, None),
            FdEntry::File { state, .. } => {
                let s = state.lock().unwrap();
                (
                    WriteTarget::File {
                        path: s.path.clone(),
                        position: s.position,
                        append: s.append,
                    },
                    Some(Arc::clone(state)),
                )
            }
            _ => return Errno::Inval.raw() as i32,
        }
    };

    let mem = match get_memory(&mut caller) {
        Some(m) => m,
        None => return Errno::Inval.raw() as i32,
    };
    let iovecs = match read_iovecs(&mem, &caller, iovs_ptr as u32, iovs_len as u32) {
        Ok(v) => v,
        Err(e) => return e.raw() as i32,
    };

    // Gather data from iovecs
    let mut data = Vec::new();
    for (ptr, len) in &iovecs {
        let mut buf = vec![0u8; *len];
        if mem.read(&caller, *ptr, &mut buf).is_err() {
            return Errno::Fault.raw() as i32;
        }
        data.extend_from_slice(&buf);
    }

    let total = data.len();

    let bytes_written = match target {
        WriteTarget::Stdout => {
            as_coroutine();
            match try_sync_await(crate::unblock(move || {
                use std::io::Write;
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(&data)?;
                stdout.flush()?;
                Ok::<_, io::Error>(total)
            })) {
                Some(Ok(n)) => n,
                Some(Err(_)) => return Errno::Io.raw() as i32,
                None => return Errno::NotSup.raw() as i32,
            }
        }
        WriteTarget::Stderr => {
            as_coroutine();
            match try_sync_await(crate::unblock(move || {
                use std::io::Write;
                let mut stderr = std::io::stderr().lock();
                stderr.write_all(&data)?;
                stderr.flush()?;
                Ok::<_, io::Error>(total)
            })) {
                Some(Ok(n)) => n,
                Some(Err(_)) => return Errno::Io.raw() as i32,
                None => return Errno::NotSup.raw() as i32,
            }
        }
        WriteTarget::File {
            path,
            position,
            append,
        } => {
            as_coroutine();
            let write_pos = if append { u64::MAX } else { position };
            match try_sync_await(async_write_file(path, write_pos, data, append)) {
                Some(Ok((n, new_pos))) => {
                    if let Some(state) = &file_state {
                        state.lock().unwrap().position = new_pos;
                    }
                    n
                }
                Some(Err(_)) => return Errno::Io.raw() as i32,
                None => return Errno::NotSup.raw() as i32,
            }
        }
    };

    if mem
        .write(
            &mut caller,
            nwritten_ptr as usize,
            &(bytes_written as u32).to_le_bytes(),
        )
        .is_err()
    {
        return Errno::Fault.raw() as i32;
    }
    Errno::Success.raw() as i32
}

async fn async_write_file(
    path: PathBuf,
    position: u64,
    data: Vec<u8>,
    append: bool,
) -> io::Result<(usize, u64)> {
    let file = crate::fs::OpenOptions::new()
        .write(true)
        .append(append)
        .open(&path)
        .await?;

    let len = data.len();
    let write_pos = if append {
        // Get current file size for append
        let metadata = crate::fs::metadata(&path).await?;
        metadata.len()
    } else {
        position
    };

    let (result, _) = file.write_at(data, write_pos).await;
    result?;

    let new_pos = write_pos + len as u64;
    Ok((len, new_pos))
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
