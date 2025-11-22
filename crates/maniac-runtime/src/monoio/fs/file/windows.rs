use std::{
    mem::ManuallyDrop,
    os::windows::io::{AsRawHandle, RawHandle},
};

use windows_sys::Win32::Networking::WinSock::WSABUF;

use super::File;
use crate::monoio::{
    buf::{IoBuf, IoBufMut, IoVecBuf, IoVecBufMut},
    driver::shared_fd::SharedFd,
};
use crate::uring_op;

impl AsRawHandle for File {
    fn as_raw_handle(&self) -> RawHandle {
        self.fd.raw_handle()
    }
}

uring_op!(read<IoBufMut>(read, buf));
uring_op!(read_at<IoBufMut>(read_at, buf, pos: u64));

uring_op!(write<IoBuf>(write, buf));
uring_op!(write_at<IoBuf>(write_at, buf, pos: u64));

/// The `readv` implement on windows.
///
/// Due to windows does not have syscall like `readv`, so we need to simulate it by ourself.
///
/// This function is just to fill each buffer by calling the `read` function.
pub(crate) async fn read_vectored<T: IoVecBufMut>(
    fd: SharedFd,
    mut buf_vec: T,
) -> crate::monoio::BufResult<usize, T> {
    // Convert the mutable buffer vector into raw pointers that can be used in unsafe operations
    let raw_bufs = buf_vec.write_wsabuf_ptr();
    let len = buf_vec.write_wsabuf_len();

    let wsabufs = unsafe { std::slice::from_raw_parts(raw_bufs, len) };

    let mut total_bytes_read = 0;

    // Iterate through each WSABUF structure and read data into it
    for wsabuf in wsabufs.iter() {
        // Safely create a Vec from the WSABUF pointer, then pass it to the read function
        let (res, _) = read(
            fd.clone(),
            ManuallyDrop::new(unsafe {
                Vec::from_raw_parts(wsabuf.buf, wsabuf.len as usize, wsabuf.len as usize)
            }),
        )
        .await;

        // Handle the result of the read operation
        match res {
            Ok(bytes_read) => {
                total_bytes_read += bytes_read;
                // If fewer bytes were read than requested, stop further reads
                if bytes_read < wsabuf.len as usize {
                    break;
                }
            }
            Err(e) => {
                // If an error occurs, return it along with the original buffer vector
                return (Err(e), buf_vec);
            }
        }
    }

    // Due to `read` will init each buffer, so we do need to set buffer len here.
    // Return the total bytes read and the buffer vector
    (Ok(total_bytes_read), buf_vec)
}

/// The `writev` implement on windows
///
/// Due to windows does not have syscall like `writev`, so we need to simulate it by ourself.
///
/// This function is just to write each buffer into file by calling the `write` function.
pub(crate) async fn write_vectored<T: IoVecBuf>(
    fd: SharedFd,
    buf_vec: T,
) -> crate::monoio::BufResult<usize, T> {
    // Convert the buffer vector into raw pointers that can be used in unsafe operations
    let raw_bufs = buf_vec.read_wsabuf_ptr() as *mut WSABUF;
    let len = buf_vec.read_wsabuf_len();

    let wsabufs = unsafe { std::slice::from_raw_parts(raw_bufs, len) };
    let mut total_bytes_write = 0;

    // Iterate through each WSABUF structure and write data from it
    for wsabuf in wsabufs.iter() {
        // Safely create a Vec from the WSABUF pointer, then pass it to the write function
        let (res, _) = write(
            fd.clone(),
            ManuallyDrop::new(unsafe {
                Vec::from_raw_parts(wsabuf.buf, wsabuf.len as usize, wsabuf.len as usize)
            }),
        )
        .await;

        // Handle the result of the write operation
        match res {
            Ok(bytes_write) => {
                total_bytes_write += bytes_write;
                // If fewer bytes were written than requested, stop further writes
                if bytes_write < wsabuf.len as usize {
                    break;
                }
            }
            Err(e) => {
                // If an error occurs, return it along with the original buffer vector
                return (Err(e), buf_vec);
            }
        }
    }

    // Return the total bytes written and the buffer vector
    (Ok(total_bytes_write), buf_vec)
}
