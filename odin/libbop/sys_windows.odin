#+build windows

package libbop

import "core:os"
import "core:os/os2"
import "core:sys/windows"

ENODATA :: windows.ERROR_HANDLE_EOF
EINVAL :: windows.ERROR_INVALID_PARAMETER
EACCESS :: windows.ERROR_ACCESS_DENIED
ENOMEM :: windows.ERROR_OUTOFMEMORY
EROFS :: 6009 // ERROR_FILE_READ_ONLY
ENOSYS :: windows.ERROR_NOT_SUPPORTED
EIO :: 29 // ERROR_WRITE_FAULT
EPERM :: windows.ERROR_INVALID_FUNCTION
EINTR :: 1223 // ERROR_CANCELLED
ENOFILE :: windows.ERROR_FILE_NOT_FOUND
EREMOTE :: 4352 // ERROR_REMOTE_STORAGE_MEDIA_ERROR
EDEADLK :: 1131 // ERROR_POSSIBLE_DEADLOCK

FD :: windows.HANDLE

IO_Vec :: struct {
	base: [^]byte,
	len:  uint,
}

PID :: windows.DWORD
TID :: windows.DWORD

SOCKET :: windows.SOCKET
