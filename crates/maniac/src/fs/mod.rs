//! Filesystem manipulation operations.

mod file;
use std::{io, path::Path};

pub use file::File;

#[cfg(feature = "mkdirat")]
mod dir_builder;
#[cfg(feature = "mkdirat")]
pub use dir_builder::DirBuilder;

#[cfg(feature = "mkdirat")]
mod create_dir;
#[cfg(feature = "mkdirat")]
pub use create_dir::*;

#[cfg(all(unix, feature = "symlinkat"))]
mod symlink;
#[cfg(all(unix, feature = "symlinkat"))]
pub use symlink::symlink;

mod open_options;
pub use open_options::OpenOptions;

#[cfg(unix)]
mod metadata;
#[cfg(unix)]
pub use metadata::{Metadata, metadata, symlink_metadata};

#[cfg(unix)]
mod file_type;
#[cfg(unix)]
pub use file_type::FileType;

#[cfg(unix)]
mod permissions;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle};

#[cfg(unix)]
pub use permissions::Permissions;

use crate::buf::IoBuf;
use crate::driver::shared_fd::SharedFd;

/// A macro that generates the some Op-call functions.
#[macro_export]
macro_rules! uring_op {
    ($fn_name:ident<$trait_name:ident>($op_name: ident, $buf_name:ident $(, $pos:ident: $pos_type:ty)?)) => {
        pub(crate) async fn $fn_name<T: $trait_name>(fd: SharedFd, $buf_name: T, $($pos: $pos_type)?) -> $crate::BufResult<usize, T> {
            let op = $crate::driver::op::Op::$op_name(fd, $buf_name, $($pos)?).unwrap();
            op.result().await
        }
    };
}

/// Read the entire contents of a file into a bytes vector.
pub async fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    use crate::buf::IoBufMut;

    let file = File::open(path).await?;

    #[cfg(windows)]
    let size = {
        use std::os::windows::io::AsRawHandle;
        let handle = file.as_raw_handle() as std::os::windows::io::RawHandle;
        let handle = handle as usize;
        crate::blocking::unblock(move || {
            let handle = handle as std::os::windows::io::RawHandle;
            let sys_file = std::mem::ManuallyDrop::new(unsafe {
                std::fs::File::from_raw_handle(handle)
            });
            sys_file.metadata().map(|m| m.len() as usize)
        })
        .await?
    };

    #[cfg(unix)]
    let size = file.metadata().await?.len() as usize;

    let (res, buf) = file
        .read_exact_at(Vec::with_capacity(size).slice_mut(0..size), 0)
        .await;
    res?;
    Ok(buf.into_inner())
}

/// Write a buffer as the entire contents of a file.
pub async fn write<P: AsRef<Path>, C: IoBuf>(path: P, contents: C) -> (io::Result<()>, C) {
    match File::create(path).await {
        Ok(f) => f.write_all_at(contents, 0).await,
        Err(e) => (Err(e), contents),
    }
}

/// Removes a file from the filesystem.
#[cfg(feature = "unlinkat")]
pub async fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    {
        crate::driver::op::Op::unlink(path)?.await.meta.result?;
    }
    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    {
        crate::driver::op::Op::unlink(path)?.await.meta.result?;
        let path = path.as_ref().to_owned();
        crate::blocking::unblock(move || std::fs::remove_file(path)).await?;
    }
    Ok(())
}

/// Removes an empty directory.
#[cfg(feature = "unlinkat")]
pub async fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    {
        crate::driver::op::Op::rmdir(path)?.await.meta.result?;
    }
    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    {
        let path = path.as_ref().to_owned();
        crate::blocking::unblock(move || std::fs::remove_dir(path)).await?;
    }
    Ok(())
}

/// Rename a file or directory to a new name.
#[cfg(feature = "renameat")]
pub async fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    {
        crate::driver::op::Op::rename(from.as_ref(), to.as_ref())?
            .await
            .meta
            .result?;
    }
    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    {        
        let from = from.as_ref().to_owned();
        let to = to.as_ref().to_owned();
        crate::blocking::unblock(move || std::fs::rename(from, to)).await?;
    }
    Ok(())
}
