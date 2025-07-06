#+build linux, darwin, freebsd, openbsd, netbsd
package libbop

import "core:sys/posix"

import c "core:c/libc"

ENODATA :: posix.ENODATA
EINVAL  :: posix.EINVAL
EACCESS :: posix.EACCES
ENOMEM  :: posix.ENOMEM
EROFS   :: posix.EROFS
EIO     :: posix.EIO
EPERM   :: posix.EPERM
EINTR   :: posix.EINTR
ENOFILE :: posix.ENOENT
EREMOTE :: 15 // ENOTBLK
EDEADLK :: posix.EDEADLK
ENOSYS  :: posix.ENOSYS

FD :: posix.FD

IO_Vec :: struct {
    base: rawptr,
    len:  uint,
}

PID :: posix.pid_t
TID :: posix.pthread_t

SOCKET :: c.int
