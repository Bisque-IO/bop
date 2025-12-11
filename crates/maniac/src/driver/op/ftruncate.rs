use std::io;

#[cfg(all(target_os = "linux", feature = "iouring"))]
use io_uring::{opcode, types};

use super::{super::shared_fd::SharedFd, Op, OpAble};
#[cfg(any(feature = "poll", feature = "poll-io"))]
use super::{MaybeFd, driver::ready::Direction};

pub(crate) struct Ftruncate {
    fd: SharedFd,
    len: u64,
}

impl Op<Ftruncate> {
    pub(crate) fn ftruncate(fd: &SharedFd, len: u64) -> io::Result<Op<Ftruncate>> {
        Op::submit_with(Ftruncate {
            fd: fd.clone(),
            len,
        })
    }
}

impl OpAble for Ftruncate {
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    fn uring_op(&mut self) -> io_uring::squeue::Entry {
        opcode::Ftruncate::new(types::Fd(self.fd.raw_fd()), self.len).build()
    }

    #[cfg(any(feature = "poll", feature = "poll-io"))]
    #[inline]
    fn legacy_interest(&self) -> Option<(Direction, usize)> {
        None
    }

    #[cfg(all(any(feature = "poll", feature = "poll-io"), windows))]
    fn legacy_call(&mut self) -> io::Result<MaybeFd> {
        use std::os::windows::prelude::{AsRawHandle, FromRawHandle};

        let handle = self.fd.as_raw_handle();
        let file = unsafe {
            std::mem::ManuallyDrop::new(std::fs::File::from_raw_handle(handle))
        };
        file.set_len(self.len)?;
        Ok(MaybeFd::default())
    }

    #[cfg(all(any(feature = "poll", feature = "poll-io"), unix))]
    fn legacy_call(&mut self) -> io::Result<MaybeFd> {
        crate::syscall!(ftruncate@NON_FD(self.fd.raw_fd(), self.len as libc::off_t))
    }
}
