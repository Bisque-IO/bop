//! WASI Preview 1 host function implementations using Maniac async runtime.

mod args_env;
mod clock;
mod fd;
mod path;
mod poll;
mod proc;
mod random;
#[cfg(feature = "wasi-net")]
mod sock;

use super::{WasiCtx, WasiView};
use crate::wasi::types::*;
use anyhow::Result;
use wasmtime::{Caller, Linker};

pub use args_env::*;
pub use clock::*;
pub use fd::*;
pub use path::*;
pub use poll::*;
pub use proc::*;
pub use random::*;
#[cfg(feature = "wasi-net")]
pub use sock::*;

/// Register all wasi_snapshot_preview1 functions with the linker.
pub fn add_wasi_snapshot_preview1_to_linker<T: WasiView + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    // Args and environment
    linker.func_wrap("wasi_snapshot_preview1", "args_get", args_get)?;
    linker.func_wrap("wasi_snapshot_preview1", "args_sizes_get", args_sizes_get)?;
    linker.func_wrap("wasi_snapshot_preview1", "environ_get", environ_get)?;
    linker.func_wrap("wasi_snapshot_preview1", "environ_sizes_get", environ_sizes_get)?;

    // Clock
    linker.func_wrap("wasi_snapshot_preview1", "clock_res_get", clock_res_get)?;
    linker.func_wrap("wasi_snapshot_preview1", "clock_time_get", clock_time_get)?;

    // File descriptor operations
    linker.func_wrap("wasi_snapshot_preview1", "fd_advise", fd_advise)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_allocate", fd_allocate)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_close", fd_close)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_datasync", fd_datasync)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_fdstat_get", fd_fdstat_get)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_fdstat_set_flags", fd_fdstat_set_flags)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_fdstat_set_rights", fd_fdstat_set_rights)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_filestat_get", fd_filestat_get)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_filestat_set_size", fd_filestat_set_size)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_filestat_set_times", fd_filestat_set_times)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_pread", fd_pread)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_prestat_get", fd_prestat_get)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_prestat_dir_name", fd_prestat_dir_name)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_pwrite", fd_pwrite)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_read", fd_read)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_readdir", fd_readdir)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_renumber", fd_renumber)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_seek", fd_seek)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_sync", fd_sync)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_tell", fd_tell)?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_write", fd_write)?;

    // Path operations
    linker.func_wrap("wasi_snapshot_preview1", "path_create_directory", path_create_directory)?;
    linker.func_wrap("wasi_snapshot_preview1", "path_filestat_get", path_filestat_get)?;
    linker.func_wrap("wasi_snapshot_preview1", "path_filestat_set_times", path_filestat_set_times)?;
    linker.func_wrap("wasi_snapshot_preview1", "path_link", path_link)?;
    linker.func_wrap("wasi_snapshot_preview1", "path_open", path_open)?;
    linker.func_wrap("wasi_snapshot_preview1", "path_readlink", path_readlink)?;
    linker.func_wrap("wasi_snapshot_preview1", "path_remove_directory", path_remove_directory)?;
    linker.func_wrap("wasi_snapshot_preview1", "path_rename", path_rename)?;
    linker.func_wrap("wasi_snapshot_preview1", "path_symlink", path_symlink)?;
    linker.func_wrap("wasi_snapshot_preview1", "path_unlink_file", path_unlink_file)?;

    // Poll
    linker.func_wrap("wasi_snapshot_preview1", "poll_oneoff", poll_oneoff)?;

    // Process
    linker.func_wrap("wasi_snapshot_preview1", "proc_exit", proc_exit)?;
    linker.func_wrap("wasi_snapshot_preview1", "proc_raise", proc_raise)?;
    linker.func_wrap("wasi_snapshot_preview1", "sched_yield", sched_yield)?;

    // Random
    linker.func_wrap("wasi_snapshot_preview1", "random_get", random_get)?;

    // Sockets (optional)
    #[cfg(feature = "wasi-net")]
    {
        linker.func_wrap("wasi_snapshot_preview1", "sock_accept", sock_accept)?;
        linker.func_wrap("wasi_snapshot_preview1", "sock_recv", sock_recv)?;
        linker.func_wrap("wasi_snapshot_preview1", "sock_send", sock_send)?;
        linker.func_wrap("wasi_snapshot_preview1", "sock_shutdown", sock_shutdown)?;
    }

    Ok(())
}

