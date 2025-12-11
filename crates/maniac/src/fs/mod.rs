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
            let sys_file =
                std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_handle(handle) });
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

/// Read the entire contents of a file into a string.
///
/// This is a convenience function for reading an entire file into a String.
/// It returns an error if the file contents are not valid UTF-8.
///
/// # Examples
///
/// ```no_run
/// use maniac::fs;
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     let contents = fs::read_to_string("foo.txt").await?;
///     println!("File contents: {}", contents);
///     Ok(())
/// }
/// ```
pub async fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let bytes = read(path).await?;
    String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Reads the entries of a directory.
///
/// Returns a vector of directory entries. This function collects all entries
/// at once, which is suitable for most use cases.
///
/// # Examples
///
/// ```no_run
/// use maniac::fs;
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     let entries = fs::read_dir(".").await?;
///     for entry in entries {
///         println!("{:?}", entry.path());
///     }
///     Ok(())
/// }
/// ```
pub async fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<Vec<std::fs::DirEntry>> {
    let path = path.as_ref().to_owned();
    crate::blocking::unblock(move || {
        std::fs::read_dir(&path)?
            .collect::<Result<Vec<_>, _>>()
    })
    .await
}

/// Copies the contents of one file to another.
///
/// This function will overwrite the contents of `to` if it exists.
///
/// # Examples
///
/// ```no_run
/// use maniac::fs;
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     fs::copy("foo.txt", "bar.txt").await?;
///     Ok(())
/// }
/// ```
pub async fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<u64> {
    let from = from.as_ref().to_owned();
    let to = to.as_ref().to_owned();
    crate::blocking::unblock(move || std::fs::copy(&from, &to)).await
}

/// Returns the canonical, absolute form of a path with all intermediate
/// components normalized and symbolic links resolved.
///
/// # Examples
///
/// ```no_run
/// use maniac::fs;
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     let path = fs::canonicalize("../foo").await?;
///     println!("Canonical path: {:?}", path);
///     Ok(())
/// }
/// ```
pub async fn canonicalize<P: AsRef<Path>>(path: P) -> io::Result<std::path::PathBuf> {
    let path = path.as_ref().to_owned();
    crate::blocking::unblock(move || std::fs::canonicalize(&path)).await
}

/// Returns `true` if the path exists on disk.
///
/// This function will traverse symbolic links to query information about the
/// destination file.
///
/// # Examples
///
/// ```no_run
/// use maniac::fs;
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     if fs::exists("foo.txt").await? {
///         println!("File exists!");
///     }
///     Ok(())
/// }
/// ```
pub async fn exists<P: AsRef<Path>>(path: P) -> io::Result<bool> {
    let path = path.as_ref().to_owned();
    crate::blocking::unblock(move || Ok(path.exists())).await
}

/// Removes a directory and all of its contents.
///
/// # Examples
///
/// ```no_run
/// use maniac::fs;
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     fs::remove_dir_all("some/dir").await?;
///     Ok(())
/// }
/// ```
pub async fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let path = path.as_ref().to_owned();
    crate::blocking::unblock(move || std::fs::remove_dir_all(&path)).await
}

/// Creates a hard link on the filesystem.
///
/// The `link` path will be a link pointing to the `original` path.
///
/// # Examples
///
/// ```no_run
/// use maniac::fs;
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     fs::hard_link("original.txt", "link.txt").await?;
///     Ok(())
/// }
/// ```
pub async fn hard_link<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) -> io::Result<()> {
    let original = original.as_ref().to_owned();
    let link = link.as_ref().to_owned();
    crate::blocking::unblock(move || std::fs::hard_link(&original, &link)).await
}

/// Reads the target of a symbolic link.
///
/// # Examples
///
/// ```no_run
/// use maniac::fs;
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     let target = fs::read_link("symlink").await?;
///     println!("Symlink target: {:?}", target);
///     Ok(())
/// }
/// ```
pub async fn read_link<P: AsRef<Path>>(path: P) -> io::Result<std::path::PathBuf> {
    let path = path.as_ref().to_owned();
    crate::blocking::unblock(move || std::fs::read_link(&path)).await
}
