use std::{io, path::Path};

/// Creates a new symbolic link on the filesystem.
/// The dst path will be a symbolic link pointing to the src path.
/// This is an async version of std::os::unix::fs::symlink.
pub async fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<()> {
    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    {
        // For non-io_uring systems, use blocking thread pool to avoid blocking the async runtime
        let src = src.as_ref().to_path_buf();
        let dst = dst.as_ref().to_path_buf();
        crate::blocking::unblock(move || {
            use std::os::unix::fs::symlink as unix_symlink;
            unix_symlink(&src, &dst)
        })
        .await
    }

    #[cfg(all(target_os = "linux", feature = "iouring"))]
    {
        // For io_uring systems, use the existing Op machinery
        use crate::driver::op::Op;
        Op::symlink(src, dst)?.await.meta.result?;
        Ok(())
    }
}
