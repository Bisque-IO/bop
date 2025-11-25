use std::path::{Path, PathBuf};

pub(super) struct BuilderInner;

impl BuilderInner {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) async fn mkdir(&self, path: &Path) -> std::io::Result<()> {
        #[cfg(not(all(target_os = "linux", feature = "iouring")))]
        {
            // For non-io_uring systems, use blocking thread pool to avoid blocking the async runtime
            let path = path.to_path_buf();
            crate::blocking::unblock(move || std::fs::create_dir(&path)).await
        }
        
        #[cfg(all(target_os = "linux", feature = "iouring"))]
        {
            // For io_uring systems, use the existing Op machinery
            use crate::driver::op::Op;
            Op::mkdir(path)?.await.meta.result.map(|_| ())
        }
    }
}
