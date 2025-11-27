use std::path::{Path, PathBuf};

use libc::mode_t;

pub(super) struct BuilderInner {
    mode: libc::mode_t,
}

impl BuilderInner {
    pub(super) fn new() -> Self {
        Self { mode: 0o777 }
    }

    pub(super) async fn mkdir(&self, path: &Path) -> std::io::Result<()> {
        #[cfg(not(all(target_os = "linux", feature = "iouring")))]
        {
            // For non-io_uring systems, use blocking thread pool to avoid blocking the async runtime
            let path = path.to_path_buf();
            let mode = self.mode;
            crate::blocking::unblock(move || {
                let mut builder = std::fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(mode);
                }
                builder.create(&path)
            })
            .await
        }

        #[cfg(all(target_os = "linux", feature = "iouring"))]
        {
            // For io_uring systems, use the existing Op machinery
            use crate::driver::op::Op;
            Op::mkdir(path, self.mode)?.await.meta.result.map(|_| ())
        }
    }

    pub(super) fn set_mode(&mut self, mode: u32) {
        self.mode = mode as mode_t;
    }
}
